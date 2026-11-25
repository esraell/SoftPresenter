//! Loading songs from files: SoftPresenter's own, and songs imported from text, lyric, ChordPro,
//! rich-text and Word files.

use std::path::{Path, PathBuf};

use crate::i18n::{t, tf};
use sp_model::bounded_zip::{ZipLimits, ZipRefusal};
use sp_model::{
    LARGEST_PRO_FILE, Presentation, decode_text, from_json, import_song_text, read_pro_file, rtf_to_text, to_json,
};

/// The song shown when the program is started without any files.
pub const SAMPLE_SONG: &str = include_str!("../../../samples/sample-song.txt");

/// A song and the file it came from, so edits can be saved back.
pub struct Loaded {
    pub song: Presentation,
    pub path: PathBuf,
}

/// The extension of SoftPresenter's own song file.
pub const SONG_EXTENSION: &str = "sppres";
/// The extension songs were saved with before the project was renamed; they still open, and
/// are saved again under the same name.
pub const OLD_SONG_EXTENSION: &str = "pcpres";

/// Endings of text files read as songs: plain text, lyric files and ChordPro.
pub const TEXT_ENDINGS: [&str; 6] = ["txt", "text", "cho", "chordpro", "chopro", "crd"];
/// Endings of documents read as songs through macOS's own text converter. Not web pages: the
/// converter may fetch what a page links to from the internet.
pub const DOCUMENT_ENDINGS: [&str; 3] = ["doc", "docx", "odt"];
/// Rich text, read by SoftPresenter itself.
pub const RICH_TEXT_ENDING: &str = "rtf";
/// The documents ProPresenter 7 saves songs in.
pub const PRO_ENDING: &str = "pro";
/// The bundles it exports a song in, with the pictures and videos the song shows (see
/// `bundle.rs`).
pub const BUNDLE_ENDING: &str = crate::bundle::ENDING;
/// SoftProjector's songbooks: its own database, or a songbook exported from it. Both are
/// SQLite files holding many songs.
pub const SONGBOOK_ENDINGS: [&str; 2] = ["sps", "sqlite"];
/// Songbooks are passed around zipped; a zip is one when it holds a songbook.
pub const SONGBOOK_ZIP_ENDING: &str = "zip";
/// A song file is a few kilobytes; anything far bigger is not one, and isn't read into memory.
const LARGEST_SONG_FILE: u64 = 4 * 1024 * 1024;

/// True for a file SoftPresenter can import as a song (besides its own song files).
pub fn is_importable(path: &Path) -> bool {
    TEXT_ENDINGS
        .iter()
        .chain(&DOCUMENT_ENDINGS)
        .chain(&[RICH_TEXT_ENDING, PRO_ENDING, BUNDLE_ENDING])
        .chain(&SONGBOOK_ENDINGS)
        .any(|ending| has_extension(path, ending))
        || is_songbook_zip(path)
}

/// True for a file of many songs: a SoftProjector database or songbook, or a zip holding some.
pub fn is_songbook(path: &Path) -> bool {
    is_songbook_file(path) || is_songbook_zip(path)
}

/// True for a name with a songbook's own ending.
fn is_songbook_file(path: &Path) -> bool {
    SONGBOOK_ENDINGS.iter().any(|ending| has_extension(path, ending))
}

/// True for a zip that lists a songbook among its files. Only files ending `.zip` are opened,
/// and only their list of names is read.
fn is_songbook_zip(path: &Path) -> bool {
    has_extension(path, SONGBOOK_ZIP_ENDING) && songbooks_in_zip(path).is_some_and(|(_, books)| !books.is_empty())
}

/// How much is read of a zip that may hold songbooks: the largest songbooks there are unpack to
/// four megabytes, and a church's whole database to a few dozen.
const SONGBOOK_ZIP: ZipLimits =
    ZipLimits { file: 256 * 1024 * 1024, entry: 256 * 1024 * 1024, unpacked: 256 * 1024 * 1024, entries: 2000 };

/// A name inside a zip, as it was written: UTF-8 when its bytes are that, else the zip's own
/// old encoding (code page 437), in which every byte is a letter or sign.
fn zip_entry_name(raw: &[u8]) -> String {
    /// The upper half of code page 437.
    const UPPER: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
    match std::str::from_utf8(raw) {
        Ok(name) => name.to_owned(),
        Err(_) => raw
            .iter()
            .map(|byte| match byte.checked_sub(0x80) {
                Some(upper) => UPPER.chars().nth(usize::from(upper)).unwrap_or(char::REPLACEMENT_CHARACTER),
                None => char::from(*byte),
            })
            .collect(),
    }
}

/// True for a name inside a zip that is a songbook's: a file with a songbook's ending, not a
/// hidden one, and not one of the shadow files a Mac adds to a zip.
fn is_songbook_entry(name: &str) -> bool {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    !file.starts_with('.') && !name.starts_with("__MACOSX/") && is_songbook_file(Path::new(file))
}

/// A zip open for reading.
type ZipFile = zip::ZipArchive<std::io::BufReader<std::fs::File>>;

/// The songbooks a zip holds: each one's place in the zip and its name. `None` when the file
/// isn't a zip SoftPresenter looks into (too large, too many entries, damaged, or entries
/// sharing their bytes), an empty list when it holds no songbook. Only the zip's list of names
/// is read.
fn songbooks_in_zip(path: &Path) -> Option<(ZipFile, Vec<(usize, String)>)> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > SONGBOOK_ZIP.file {
        return None;
    }
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    if zip.len() > SONGBOOK_ZIP.entries || zip.has_overlapping_files().unwrap_or(true) {
        return None;
    }
    let books = (0..zip.len())
        .filter_map(|at| {
            let entry = zip.by_index_raw(at).ok()?;
            let name = zip_entry_name(entry.name_raw());
            (!entry.is_dir() && is_songbook_entry(&name)).then_some((at, name))
        })
        .collect();
    Some((zip, books))
}

/// The songs of every songbook in a zip (the songbooks SoftProjector offers for download come
/// zipped). The zip is read with limits and nothing is unpacked to disk; a songbook in it that
/// can't be read is said, and the others still come in.
fn import_zipped_songbooks(path: &Path, name: &str) -> Result<Songbook, String> {
    let not_songbook = || {
        tf(
            "notices.import.zip_no_songbook",
            "{name} has no SoftProjector songbook in it that SoftPresenter can read.",
            &[("name", name)],
        )
    };
    let (mut zip, books) = songbooks_in_zip(path).filter(|(_, books)| !books.is_empty()).ok_or_else(not_songbook)?;
    let mut all = Songbook { songs: Vec::new(), cautions: Vec::new() };
    let mut room = SONGBOOK_ZIP.unpacked;
    for (at, entry) in books {
        let file = entry.rsplit(['/', '\\']).next().unwrap_or(&entry);
        let inside = tf("notices.import.inside_zip", "{file} in {name}", &[("file", file), ("name", name)]);
        let read = match sp_model::bounded_zip::read_entry(&mut zip, at, room.min(SONGBOOK_ZIP.entry)) {
            Ok(bytes) => {
                room -= bytes.len() as u64;
                songbook_songs(std::io::Cursor::new(bytes), &inside)
            }
            Err(ZipRefusal::UnpacksTooMuch) => Err(tf(
                "notices.import.zip_too_much",
                "{name} unpacks to more than any songbook, so it wasn't read.",
                &[("name", &inside)],
            )),
            Err(_) => Err(tf(
                "notices.import.unreadable",
                "{name} could not be read ({error}).",
                &[("name", &inside), ("error", t("notices.import.zip_damaged", "the zip is damaged"))],
            )),
        };
        match read {
            Ok(songs) => all.songs.extend(songs),
            Err(problem) => all.cautions.push(problem),
        }
    }
    // Nothing came in: what went wrong is the answer.
    if all.songs.is_empty() && !all.cautions.is_empty() {
        return Err(all.cautions.join(" "));
    }
    Ok(all)
}

/// What a file of many songs gave: each song with the name of its songbook, and anything the
/// operator should know about what came in.
pub struct Songbook {
    pub songs: Vec<(Presentation, Option<String>)>,
    pub cautions: Vec<String>,
}

/// Every song in a SoftProjector database or exported songbook, each with the name of its
/// songbook. The two keep songs differently: the database has a `Songbooks` table and each
/// song names its songbook; an exported songbook has one `SongBook` row (its title) and its
/// songs. Both are SQLite files, which SoftPresenter reads itself (`sp_model::sqlite_read`), the
/// same way on every system, and never writes.
pub fn import_songbook(path: &Path) -> Result<Songbook, String> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    if has_extension(path, SONGBOOK_ZIP_ENDING) {
        return import_zipped_songbooks(path, &name);
    }
    let file = std::fs::File::open(path).map_err(|e| {
        tf(
            "notices.import.not_found",
            "{name} could not be found ({error}).",
            &[("name", &name), ("error", &e.to_string())],
        )
    })?;
    let songs = songbook_songs(file, &name)?;
    Ok(Songbook { songs, cautions: unsaved_changes(path, &name).into_iter().collect() })
}

/// The songs of one songbook, as presentations; `name` is the file's, for what is said of it.
fn songbook_songs<R: std::io::Read + std::io::Seek>(
    source: R,
    name: &str,
) -> Result<Vec<(Presentation, Option<String>)>, String> {
    use sp_model::SongbookProblem;
    let songs = sp_model::read_songbook(source).map_err(|problem| match problem {
        SongbookProblem::NotSongbook => tf(
            "notices.import.not_songbook",
            "{name} isn't a SoftProjector songbook SoftPresenter can read.",
            &[("name", name)],
        ),
        SongbookProblem::NoSongs => tf("notices.import.no_songs", "{name} has no songs in it.", &[("name", name)]),
        SongbookProblem::Unreadable(why) => {
            tf("notices.import.unreadable", "{name} could not be read ({error}).", &[("name", name), ("error", &why)])
        }
    })?;
    Ok(songs
        .into_iter()
        .map(|song| {
            let made = sp_model::from_softprojector(song.number, &song.title, &song.words, &song.music, &song.text);
            (made, song.book)
        })
        .filter(|(song, _)| song.slide_count() > 0)
        .collect())
}

/// A database copied while the program that keeps it had it open may have changes waiting in
/// a file beside it (`-wal`, or `-journal` part way through a change). Only the database itself
/// is read, so the operator is told its latest changes may be missing.
fn unsaved_changes(path: &Path, name: &str) -> Option<String> {
    let beside = |ending: &str| {
        let mut other = path.as_os_str().to_owned();
        other.push(ending);
        std::fs::metadata(other).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
    };
    (beside("-wal") || beside("-journal")).then(|| {
        tf(
            "notices.import.songbook_unsaved",
            "{name} was still open in SoftProjector when it was copied, so its latest changes may be missing. Close SoftProjector and import it again to be sure.",
            &[("name", name)],
        )
    })
}

/// The files to import for the paths given, each with the library it goes into. A file goes
/// into no particular library; a folder's songs go into a library named after it, and the songs
/// in each folder inside it into a library named after that one (so a folder of libraries
/// comes across as those libraries). Hidden files and folders are passed over; files are taken
/// in name order.
pub fn files_to_import(paths: &[PathBuf]) -> Vec<(PathBuf, Option<String>)> {
    let name_of = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned());
    let hidden = |path: &Path| name_of(path).is_some_and(|n| n.starts_with('.'));
    let listed = |folder: &Path| {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
            .map(|read| read.filter_map(Result::ok).map(|entry| entry.path()).filter(|p| !hidden(p)).collect())
            .unwrap_or_default();
        entries.sort();
        entries
    };
    let wanted = |path: &Path| path.is_file() && (is_importable(path) || is_song_file(path));
    let mut files = Vec::new();
    for path in paths {
        if !path.is_dir() {
            files.push((path.clone(), None));
            continue;
        }
        let entries = listed(path);
        files.extend(entries.iter().filter(|p| wanted(p)).map(|p| (p.clone(), name_of(path))));
        for folder in entries.iter().filter(|p| p.is_dir()) {
            files.extend(listed(folder).into_iter().filter(|p| wanted(p)).map(|p| (p, name_of(folder))));
        }
    }
    files
}

/// A bundle's ending as file dialogs are given it: as the program writes it, and in lower case,
/// for a system whose dialogs tell the two apart.
pub const PRO_BUNDLE_ENDINGS: [&str; 2] = ["proBundle", BUNDLE_ENDING];

/// File › Import Songs › Song Files…: songs saved by SoftPresenter, song documents, ChordPro and
/// lyric files, and songbooks.
pub fn song_file_endings() -> Vec<&'static str> {
    let mut endings = vec![SONG_EXTENSION, OLD_SONG_EXTENSION, PRO_ENDING];
    endings.extend(TEXT_ENDINGS.iter().filter(|ending| !matches!(**ending, "txt" | "text")));
    endings.extend(SONGBOOK_ENDINGS);
    endings.push(SONGBOOK_ZIP_ENDING);
    endings
}

/// File › Import Songs › A Song Bundle…: song bundles and song packs, songs with the pictures and
/// videos they show.
pub fn bundle_endings() -> Vec<&'static str> {
    let mut endings = PRO_BUNDLE_ENDINGS.to_vec();
    endings.push(crate::pack::ENDING);
    endings
}

/// File › Import Songs › Words from Text…: plain text, rich text, and Word and OpenDocument files.
pub fn words_endings() -> Vec<&'static str> {
    let mut endings = vec!["txt", "text", RICH_TEXT_ENDING];
    endings.extend(DOCUMENT_ENDINGS);
    endings
}

/// A file window's filter: the kind of file, and its endings ("Song files (.pro, .cho)"). An
/// ending that differs only in its capitals is named once.
pub fn filter_name(kind: &str, endings: &[&str]) -> String {
    let mut named: Vec<&str> = Vec::new();
    for ending in endings {
        if !named.iter().any(|have| have.eq_ignore_ascii_case(ending)) {
            named.push(ending);
        }
    }
    let listed: Vec<String> = named.iter().map(|ending| format!(".{ending}")).collect();
    format!("{kind} ({})", listed.join(", "))
}

/// Reads a song from a file to import: text in any common encoding, a lyric file, ChordPro,
/// rich text, a song document or bundle saved by ProPresenter 7 (a bundle's words only: the
/// import brings its pictures and videos, see `bundle.rs`), or (on macOS) a Word or
/// OpenDocument file. The error is a plain sentence for the operator.
pub fn import_file(path: &Path) -> Result<Presentation, String> {
    let name = path
        .file_name()
        .map_or_else(|| t("notices.import.the_file", "The file").to_owned(), |n| n.to_string_lossy().into_owned());
    let unreadable = |error: &str| {
        tf("notices.import.unreadable", "{name} could not be read ({error}).", &[("name", &name), ("error", error)])
    };
    let size = std::fs::metadata(path).map_err(|e| unreadable(&e.to_string()))?.len();
    if is_songbook(path) {
        return Err(tf(
            "notices.import.many_songs",
            "{name} holds many songs: import it with File › Import Songs › Song Files… or by dropping it on the window.",
            &[("name", &name)],
        ));
    }
    // Only the kinds of file songs come in: a sound, a picture or anything else is not read as
    // words.
    if !is_importable(path) && !is_song_file(path) {
        if crate::audio::is_audio(path) {
            return Err(tf(
                "notices.import.sound",
                "{name} is a sound: sounds go in the Audio tab.",
                &[("name", &name)],
            ));
        }
        return Err(tf(
            "notices.import.not_song_kind",
            "{name} isn't a kind of file songs come in. Songs come from text, lyric and ChordPro files, rich text, Word and OpenDocument files, .pro and .proBundle files, .sps and .sqlite songbooks (zipped or not) and SoftPresenter songs; PDFs, presentation files (.pptx, .ppsx, .key, .odp, .ppt, .pps) and folders of pictures come in as slides.",
            &[("name", &name)],
        ));
    }
    if crate::bundle::is_bundle(path) {
        return crate::bundle::read_words(path);
    }
    let pro = has_extension(path, PRO_ENDING);
    let largest = if pro { LARGEST_PRO_FILE as u64 } else { LARGEST_SONG_FILE };
    if size > largest {
        return Err(tf("notices.import.too_large", "{name} is too large to be a song.", &[("name", &name)]));
    }
    let title = path
        .file_stem()
        .map_or_else(|| t("left.untitled", "Untitled").to_owned(), |s| s.to_string_lossy().into_owned());
    let read = || std::fs::read(path).map_err(|e| unreadable(&e.to_string()));
    let song = if pro {
        read_pro_file(&read()?, &title).map_err(|reason| unreadable(&reason))?
    } else {
        let text = if has_extension(path, RICH_TEXT_ENDING) {
            rtf_to_text(&read()?)
        } else if DOCUMENT_ENDINGS.iter().any(|ending| has_extension(path, ending)) {
            document_text(path).map_err(|reason| unreadable(&reason))?
        } else {
            decode_text(&read()?)
        };
        if !looks_like_text(&text) {
            return Err(tf(
                "notices.import.not_text",
                "{name} doesn't read as text (is it the right file?).",
                &[("name", &name)],
            ));
        }
        import_song_text(&title, &text)
    };
    if song.slide_count() == 0 {
        return Err(tf("notices.import.no_words", "{name} has no words in it.", &[("name", &name)]));
    }
    Ok(song)
}

/// Whether decoded words look like text and not a binary file read as letters: almost no
/// control characters besides line breaks and tabs.
pub fn looks_like_text(text: &str) -> bool {
    let total = text.chars().count().max(1);
    let control = text.chars().filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')).count();
    control * 100 <= total
}

/// The words of a Word or OpenDocument file, from macOS's own converter
/// (textutil), which ships with every Mac. It gets ten seconds.
#[cfg(target_os = "macos")]
fn document_text(path: &Path) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    // A full path, so a file name starting with "-" can't be taken for an option.
    let full = path.canonicalize().map_err(|e| e.to_string())?;
    let mut child = Command::new("/usr/bin/textutil")
        .args(["-convert", "txt", "-encoding", "UTF-8", "-stdout"])
        .arg(&full)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Read while it runs, so a long document can't fill the pipe and stall it.
    let mut stdout = child.stdout.take().ok_or(t("notices.import.no_output", "no output"))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.by_ref().take(LARGEST_SONG_FILE * 4).read_to_end(&mut bytes);
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() > Duration::from_secs(10) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(t("notices.import.too_long", "it took too long to read").to_owned());
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let bytes = reader.join().map_err(|_| t("notices.import.converter_failed", "the converter failed").to_owned())?;
    if !status.success() {
        return Err(t("notices.import.not_document", "macOS could not read it as a document").to_owned());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn document_text(_path: &Path) -> Result<String, String> {
    Err(t(
        "notices.import.word_mac_only",
        "Word files can be imported on macOS for now; save the song as a .txt or .rtf file",
    )
    .to_owned())
}

/// Reads one of SoftPresenter's own song files.
pub fn read_song_file(path: &Path) -> Result<Presentation, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > LARGEST_SONG_FILE * 4 {
        return Err(t("notices.import.too_large_file", "too large to be a song file").to_owned());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    from_json(&text)
}

/// True for a SoftPresenter song file, of either extension.
pub fn is_song_file(path: &Path) -> bool {
    has_extension(path, SONG_EXTENSION) || has_extension(path, OLD_SONG_EXTENSION)
}

/// The SoftPresenter song file that belongs with a path: the path itself for a song file;
/// for a text file, the song file beside it with the same name (one saved before the rename
/// if that is what is there). A text file keeps its words only; anything more (where objects
/// sit, how they look) lives in this file beside it.
pub fn song_file_for(path: &Path) -> PathBuf {
    if is_song_file(path) {
        return path.to_path_buf();
    }
    let old = path.with_extension(OLD_SONG_EXTENSION);
    if old.is_file() && !path.with_extension(SONG_EXTENSION).is_file() {
        return old;
    }
    path.with_extension(SONG_EXTENSION)
}

/// Saves a song as a SoftPresenter song file beside the file it came from, and returns where
/// it was written. A text file it was read from is left untouched. The new file is written
/// under a temporary name first and then moved into place, so a crash cannot leave half a song.
pub fn save_song(path: &Path, song: &Presentation) -> Result<PathBuf, String> {
    let target = song_file_for(path);
    let name = target.file_name().map_or_else(|| "song".into(), |n| n.to_string_lossy().into_owned());
    let temporary = target.with_file_name(format!(".{name}.saving"));
    std::fs::write(&temporary, to_json(song)).and_then(|()| std::fs::rename(&temporary, &target)).map_err(|e| {
        tf(
            "notices.song.not_saved",
            "{file}: could not be saved ({error}).",
            &[("file", &target.display().to_string()), ("error", &crate::notices::write_problem(&e))],
        )
    })?;
    Ok(target)
}

/// Loads every path given: a file is one song, a folder is every song directly inside it in
/// name order. A song is a text file or a SoftPresenter song file; where both exist with the
/// same name, the SoftPresenter file is the one read, because it holds the later edits. Returns the songs in order and one plain message for each path that
/// could not be used.
pub fn load_songs(paths: &[PathBuf]) -> (Vec<Loaded>, Vec<String>) {
    let mut songs = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        if path.is_dir() {
            match text_files_in(path) {
                Ok(files) if files.is_empty() => problems.push(tf(
                    "notices.import.empty_folder",
                    "{folder}: no songs in this folder",
                    &[("folder", &path.display().to_string())],
                )),
                Ok(files) => files.iter().for_each(|f| load_one(f, &mut songs, &mut problems)),
                Err(reason) => problems.push(format!("{}: {reason}", path.display())),
            }
        } else {
            load_one(path, &mut songs, &mut problems);
        }
    }
    (songs, problems)
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(wanted))
}

fn text_files_in(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && (is_importable(p) || is_song_file(p)))
        .collect();
    // Of song.txt and song.sppres (or song.pcpres), keep only the SoftPresenter file.
    let ours: Vec<PathBuf> = files.iter().filter(|p| is_song_file(p)).cloned().collect();
    files.retain(|p| is_song_file(p) || !ours.contains(&song_file_for(p)));
    files.sort();
    Ok(files)
}

fn load_one(path: &Path, songs: &mut Vec<Loaded>, problems: &mut Vec<String>) {
    // A text file that has been edited has a SoftPresenter file beside it; read that.
    let edited = song_file_for(path);
    let path = if !is_song_file(path) && edited.is_file() { edited.as_path() } else { path };
    let read = if is_song_file(path) { read_song_file(path) } else { import_file(path) };
    let song = match read {
        Ok(song) => song,
        Err(reason) => return problems.push(format!("{}: {reason}", path.display())),
    };
    if song.slide_count() == 0 {
        return problems.push(tf(
            "notices.import.no_slides",
            "{file}: no slides found",
            &[("file", &path.display().to_string())],
        ));
    }
    songs.push(Loaded { song, path: path.to_owned() });
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_sound_or_a_binary_file_is_not_imported_as_a_song() {
        let folder = std::env::temp_dir().join(format!("softpresenter-binary-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let sound = folder.join("Welcome.m4a");
        std::fs::write(&sound, [0u8, 0, 0, 0x20, b'f', b't', b'y', b'p', 0, 1, 2, 3, 4, 5]).unwrap();
        assert!(import_file(&sound).unwrap_err().contains("is a sound"));
        let other = folder.join("Welcome.bin");
        std::fs::write(&other, [0u8, 1, 2, 3]).unwrap();
        assert!(import_file(&other).unwrap_err().contains("isn't a kind of file songs come in"));
        let disguised = folder.join("Welcome.txt");
        let bytes: Vec<u8> = (0..400u32).map(|n| (n % 32) as u8).collect();
        std::fs::write(&disguised, bytes).unwrap();
        assert!(import_file(&disguised).unwrap_err().contains("doesn't read as text"));
        assert!(looks_like_text("Amazing grace\nHow sweet the sound\n"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    use sp_model::{Role, parse_song_text};

    use super::*;

    /// A fresh folder under the system temporary folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("softpresenter-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_song_bundle_is_imported_wherever_a_song_document_is() {
        for name in ["Song.proBundle", "Song.probundle", "Song.PROBUNDLE", "Song.pro"] {
            assert!(is_importable(Path::new(name)), "{name}");
        }
        let (bundles, files) = (bundle_endings(), song_file_endings());
        assert!(bundles.contains(&"proBundle") && bundles.contains(&"probundle") && files.contains(&"pro"));
        // In a folder chosen or dropped, a bundle is one of its songs.
        let dir = TempDir::new("bundles");
        dir.write("Harbour Lights.proBundle", b"PK");
        dir.write("Notes.md", b"not a song");
        let files = files_to_import(std::slice::from_ref(&dir.0));
        assert_eq!(files.len(), 1);
        assert!(files[0].0.ends_with("Harbour Lights.proBundle"));
    }

    #[test]
    fn a_songbook_and_a_database_come_in_on_every_system() {
        use sp_model::songbook_examples::{DATABASE, SONGBOOK};
        let dir = TempDir::new("songbooks");
        // An exported songbook: its songs under its title, numbers before their titles.
        let book = import_songbook(&dir.write("Zkouška.sps", SONGBOOK)).unwrap();
        assert!(book.cautions.is_empty());
        assert_eq!(book.songs.len(), 7);
        assert!(book.songs.iter().all(|(_, book)| book.as_deref() == Some("Zpěvník pro zkoušku")));
        let titles: Vec<&str> = book.songs.iter().map(|(song, _)| song.title.as_str()).collect();
        assert_eq!(titles[..4], ["Без номера", "1. První píseň", "2. Druhá píseň", "3. Третья"]);
        let second = &book.songs[2].0;
        assert_eq!(second.copyright.author, "Jana / Petr");
        assert_eq!(second.groups[0].slides[0].text_for(&Role::primary()), Some("Žluťoučký kůň běží polem"));
        assert_eq!(second.groups[1].name, "Refrén");
        // A whole database: each song under its own songbook; the two without words stay out.
        let all = import_songbook(&dir.write("spData.sqlite", DATABASE)).unwrap();
        assert_eq!(all.songs.len(), 224);
        let mut books: Vec<Option<&str>> = all.songs.iter().map(|(_, book)| book.as_deref()).collect();
        books.dedup();
        assert_eq!(books, [None, Some("Evening Hymns"), Some("Вечерние песни"), Some("Ранкові пісні")]);
        assert!(
            all.songs.iter().any(|(song, book)| song.title == "7. Ранок" && book.as_deref() == Some("Ранкові пісні"))
        );
        // Changes still waiting in a file beside it are not read, and that is said.
        dir.write("spData.sqlite-wal", b"changes not yet in the database");
        let all = import_songbook(&dir.0.join("spData.sqlite")).unwrap();
        assert_eq!(all.songs.len(), 224);
        assert!(all.cautions[0].contains("latest changes may be missing"), "{:?}", all.cautions);
        // What isn't a songbook says so, in a sentence.
        let problem = |path: &Path| import_songbook(path).err().unwrap();
        assert!(problem(&dir.write("Notes.sps", b"just some words")).contains("isn't a SoftProjector songbook"));
        assert!(
            problem(&dir.write("Cut.sps", &SONGBOOK[..5 * 1024]))
                .contains("Cut.sps could not be read (the file is cut short")
        );
        assert!(problem(&dir.0.join("Gone.sps")).contains("Gone.sps could not be found"));
    }

    #[test]
    fn a_zip_of_songbooks_comes_in_as_its_songbooks() {
        use std::io::Write;

        use sp_model::songbook_examples::{DATABASE, SONGBOOK};
        let dir = TempDir::new("songbook-zips");
        let zip_of = |name: &str, entries: &[(&str, &[u8])]| {
            let path = dir.0.join(name);
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
            path
        };
        // Two songbooks, one in a folder, beside files that aren't: a Mac's shadow file, a
        // hidden one, a note, and a file with a songbook's ending that isn't one.
        let path = zip_of(
            "Пісні.zip",
            &[
                ("Zkouška.sps", SONGBOOK),
                ("__MACOSX/._Zkouška.sps", b"shadow"),
                (".hidden.sps", b"hidden"),
                ("Read me.txt", b"A note"),
                ("Старі/spData.sqlite", DATABASE),
                ("Broken.sps", b"not a songbook at all"),
            ],
        );
        assert!(is_songbook(&path) && is_importable(&path));
        assert_eq!(crate::file_drop::kind_of(&path), crate::file_drop::Kind::Song);
        let all = import_songbook(&path).unwrap();
        assert_eq!(all.songs.len(), 7 + 224);
        assert_eq!(all.songs[1].0.title, "1. První píseň");
        assert_eq!(all.songs[1].1.as_deref(), Some("Zpěvník pro zkoušku"));
        assert!(
            all.songs.iter().any(|(song, book)| song.title == "7. Ранок" && book.as_deref() == Some("Ранкові пісні"))
        );
        assert_eq!(all.cautions, ["Broken.sps in Пісні.zip isn't a SoftProjector songbook SoftPresenter can read."]);
        // A zip of other things isn't taken for songs, and asked for anyway says why.
        let other = zip_of("Photos.zip", &[("Sky.png", b"a picture"), ("Notes.txt", b"words")]);
        assert!(!is_songbook(&other) && !is_importable(&other));
        assert!(import_songbook(&other).err().unwrap().contains("Photos.zip has no SoftProjector songbook in it"));
        let not_zip = dir.write("Not.zip", b"not a zip");
        assert!(!is_songbook(&not_zip));
        assert!(import_songbook(&not_zip).err().unwrap().contains("has no SoftProjector songbook"));
        // A zip whose only songbook can't be read gives that as the problem.
        let bad = zip_of("Bad.zip", &[("Cut.sps", &SONGBOOK[..5 * 1024])]);
        assert!(is_songbook(&bad));
        let problem = import_songbook(&bad).err().unwrap();
        assert!(problem.starts_with("Cut.sps in Bad.zip could not be read (the file is cut short"), "{problem}");
        // A zip inside a zip is not opened.
        let inner = std::fs::read(&path).unwrap();
        let nested = zip_of("Nested.zip", &[("Inner.sps", &inner)]);
        assert!(import_songbook(&nested).err().unwrap().contains("the zip is damaged"));
        // One songbook imports as itself when asked for as one song: it holds many.
        assert!(import_file(&path).unwrap_err().contains("holds many songs"));
    }

    #[test]
    fn a_name_inside_a_zip_reads_as_it_was_written() {
        assert_eq!(zip_entry_name("Пісні/Збірка.sps".as_bytes()), "Пісні/Збірка.sps");
        // Not UTF-8: the zip's own old encoding, in which every byte is something.
        assert_eq!(zip_entry_name(b"Ges\x84nge f\x81r K\x94nig.sps"), "Gesänge für König.sps");
        assert_eq!(zip_entry_name(&[0x80, 0xff, b'.', b's', b'p', b's']), "Ç\u{a0}.sps");
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(zip_entry_name(&all).chars().count(), 256);
        assert!(is_songbook_entry("folder/Book.SPS") && is_songbook_entry("folder\\spData.sqlite"));
        assert!(
            !is_songbook_entry("__MACOSX/Book.sps")
                && !is_songbook_entry("a/.Book.sps")
                && !is_songbook_entry("Book.txt")
        );
    }

    #[test]
    fn import_songs_kinds_of_file_take_every_ending_a_song_comes_in_once() {
        let mut every = vec![SONG_EXTENSION, OLD_SONG_EXTENSION, RICH_TEXT_ENDING, PRO_ENDING, crate::pack::ENDING];
        every.extend(TEXT_ENDINGS);
        every.extend(DOCUMENT_ENDINGS);
        every.extend(PRO_BUNDLE_ENDINGS);
        every.extend(SONGBOOK_ENDINGS);
        every.push(SONGBOOK_ZIP_ENDING);
        let kinds = [song_file_endings(), bundle_endings(), words_endings()];
        for ending in &every {
            let found = kinds.iter().filter(|kind| kind.contains(ending)).count();
            assert_eq!(found, 1, "{ending} is in one kind of file");
        }
        assert_eq!(kinds.iter().map(Vec::len).sum::<usize>(), every.len(), "and nothing else");
        // Words from Text… is text, rich text and Word; a song bundle's ending is named once.
        assert_eq!(words_endings(), ["txt", "text", "rtf", "doc", "docx", "odt"]);
        assert_eq!(filter_name("Song bundles", &bundle_endings()), "Song bundles (.proBundle, .sppack)");
    }

    #[test]
    fn loads_a_file_with_title_from_name() {
        let dir = TempDir::new("file");
        let path = dir.write("Morning Song.txt", "[V]\nA\n--\nБ\n".as_bytes());
        let (songs, problems) = load_songs(&[path]);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(songs[0].song.title, "Morning Song");
        assert_eq!(songs[0].song.slide_count(), 1);
    }

    #[test]
    fn loads_a_folder_sorted_and_only_txt() {
        let dir = TempDir::new("folder");
        dir.write("b.txt", b"B\n");
        dir.write("a.TXT", b"A\n");
        dir.write("notes.md", b"not a song\n");
        let (songs, problems) = load_songs(std::slice::from_ref(&dir.0));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(songs.iter().map(|s| s.song.title.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn missing_file_is_reported_and_skipped() {
        let dir = TempDir::new("missing");
        let good = dir.write("good.txt", b"A\n");
        let (songs, problems) = load_songs(&[dir.0.join("nope.txt"), good]);
        assert_eq!(songs.len(), 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("nope.txt"));
    }

    #[test]
    fn text_in_an_older_windows_encoding_is_read() {
        let dir = TempDir::new("cp1251");
        // "Свет над рекой" saved as Windows-1251, as older Russian files are.
        let path = dir.write(
            "old.txt",
            &[0xD1, 0xE2, 0xE5, 0xF2, b' ', 0xED, 0xE0, 0xE4, b' ', 0xF0, 0xE5, 0xEA, 0xEE, 0xE9, b'\n'],
        );
        let (songs, problems) = load_songs(&[path]);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(songs[0].song.slide(0).unwrap().1.text_for(&Role::primary()), Some("Свет над рекой"));
    }

    #[test]
    fn file_without_slides_is_reported() {
        let dir = TempDir::new("empty");
        let path = dir.write("empty.txt", b"\n\n[Verse]\n\n");
        let (songs, problems) = load_songs(&[path]);
        assert!(songs.is_empty());
        assert!(problems[0].contains("has no words in it"), "{problems:?}");
    }

    #[test]
    fn empty_folder_is_reported() {
        let dir = TempDir::new("emptyfolder");
        let (songs, problems) = load_songs(std::slice::from_ref(&dir.0));
        assert!(songs.is_empty());
        assert!(problems[0].contains("no songs in this folder"));
    }

    #[test]
    fn byte_order_mark_is_not_part_of_the_text() {
        let dir = TempDir::new("bom");
        let path = dir.write("bom.txt", "\u{feff}[Verse]\nA\n".as_bytes());
        let (songs, _) = load_songs(&[path]);
        assert_eq!(songs[0].song.groups[0].name, "Verse");
    }

    #[test]
    fn saving_writes_our_file_beside_the_text_file_and_leaves_the_text_alone() {
        let dir = TempDir::new("save");
        let path = dir.write("song.txt", "[Verse]\nOld\n".as_bytes());
        let (mut songs, _) = load_songs(std::slice::from_ref(&path));
        let mut edited = songs.remove(0).song.with_slide_text(0, "New words", "Новые слова").unwrap();
        edited.groups[0].slides[0].objects[0].frame =
            Some(sp_model::SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 200.0 });
        let written = save_song(&path, &edited).unwrap();
        assert_eq!(written, dir.0.join("song.sppres"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[Verse]\nOld\n", "the text file must not change");

        // Opening the text file again gives the edited song, moved object and all.
        let (again, problems) = load_songs(std::slice::from_ref(&path));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(again[0].song, edited);
        assert_eq!(again[0].path, written);

        let mut left: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, vec!["song.sppres", "song.txt"], "no temporary file should remain");
    }

    #[test]
    fn a_folder_reads_our_file_in_place_of_a_text_file_with_the_same_name() {
        let dir = TempDir::new("prefer");
        let a = dir.write("a.txt", b"From text\n");
        dir.write("b.txt", b"Only text\n");
        let edited = parse_song_text("a", "From our file\n");
        save_song(&a, &edited).unwrap();
        let (songs, problems) = load_songs(std::slice::from_ref(&dir.0));
        assert!(problems.is_empty(), "{problems:?}");
        let words: Vec<_> =
            songs.iter().map(|s| s.song.slide(0).unwrap().1.objects[0].as_text().unwrap().to_owned()).collect();
        assert_eq!(words, vec!["From our file", "Only text"]);
    }

    #[test]
    fn a_damaged_or_newer_song_file_is_reported_and_skipped() {
        let dir = TempDir::new("damaged");
        let bad = dir.write("bad.pcpres", b"{ not json");
        let newer = dir.write("newer.pcpres", br#"{"format":"softpresenter-presentation","version":99}"#);
        let (songs, problems) = load_songs(&[bad, newer]);
        assert!(songs.is_empty());
        assert!(problems[0].contains("not a SoftPresenter song file"), "{problems:?}");
        assert!(problems[1].contains("newer SoftPresenter"), "{problems:?}");
    }

    #[test]
    fn saving_into_a_missing_folder_is_reported() {
        let dir = TempDir::new("nosave");
        let song = parse_song_text("x", "A\n");
        assert!(save_song(&dir.0.join("gone").join("song.txt"), &song).unwrap_err().contains("could not be saved"));
    }

    #[test]
    fn sample_song_parses_with_both_languages() {
        let song = parse_song_text("Sample song", SAMPLE_SONG);
        assert!(song.slide_count() >= 6);
        assert!(song.slides().any(|(_, s)| s.text_for(&Role::translation()).is_some()));
        assert!(song.slides().any(|(_, s)| s.objects.len() == 1));
    }
}
