//! Song bundles saved by ProPresenter 7 (`.proBundle`): one song document and the pictures and
//! videos its slides show, in one zip file. The song is read as a `.pro` document is
//! (`sp_model::read_pro_document`); its pictures and videos come into the library's Media
//! folder and the media bin, and its slides show them.
//!
//! Two layouts are known. A bundle the program exports keeps each picture and video under the
//! whole path it had on the computer it came from, with its leading slash, and the song names
//! it by its `file://` address there. A bundle written by other tools keeps them as bare file
//! names beside the song, which names them the same way. Both are read: a picture is found by
//! its whole path, by its path from a known folder, or by a file name only one entry has.
//!
//! A bundle is untrusted input, read the way a song pack is (`pack.rs`): no entry's name is ever
//! used as a path, only as a label to look things up by. Only the one song document and the
//! pictures and videos the song shows are taken out, each within a size cap whatever the entry
//! says, checked against the free disk space, into a folder of the app's own under a name the
//! app makes. An entry whose name climbs out of its folder (`..`), a link, or an encrypted
//! entry is left out and named. Nothing in a bundle is run.
//!
//! Archives exported by that program are reported to give the place of their ZIP64 end record
//! wrongly in the record that points to it; a strict zip reader refuses them. That one number
//! is put right as the file is read, before the zip reader sees it.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sp_model::{Presentation, ProMedia, ShowAction, Uuid};

use crate::pack::{self, shown};

/// A song bundle's file ending (written `.proBundle`; read in any case).
pub const ENDING: &str = "probundle";

pub fn is_bundle(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(ENDING))
}

/// How much a bundle may hold.
pub struct Limits {
    pub entries: usize,
    pub song: u64,
    pub picture: u64,
    pub video: u64,
    /// Room left free on the disk after the pictures and videos are taken out.
    pub keep_free: u64,
}

impl Limits {
    pub const NORMAL: Limits = Limits {
        entries: 20_000,
        song: sp_model::LARGEST_PRO_FILE as u64,
        picture: pack::LARGEST_PICTURE,
        video: 20 * 1024 * 1024 * 1024,
        keep_free: pack::KEEP_FREE,
    };
}

/// A picture or video taken out of a bundle.
#[derive(Debug)]
pub struct Staged {
    /// Where it was put, in the staging folder, under a name the app made.
    pub file: PathBuf,
    /// Its file name in the bundle, to show.
    pub name: String,
    pub video: bool,
    pub looping: bool,
}

/// A bundle read and checked, its pictures and videos put in a folder of the app's own.
#[derive(Debug)]
pub struct Opened {
    pub song: Presentation,
    pub files: Vec<Staged>,
    /// Each slide that shows a picture or video: the slide's id and its place in `files`.
    pub slides: Vec<(Uuid, usize)>,
    /// The pictures and videos the song shows that the bundle doesn't hold, by file name.
    pub missing: Vec<String>,
    /// What was left out of the bundle, and why.
    pub left_out: Vec<String>,
    /// The folder the pictures and videos were put in, removed once the song is in.
    pub staging: PathBuf,
}

/// Reads a bundle into `staging` (a new folder of the app's). The error is a sentence for the
/// operator.
pub fn open(path: &Path, staging: &Path) -> Result<Opened, String> {
    open_with(path, staging, &Limits::NORMAL)
}

/// The song in a bundle, words only, without taking anything else out.
pub fn read_words(path: &Path) -> Result<Presentation, String> {
    let name = file_name(path);
    let mut zip = archive(path).map_err(|_| not_a_bundle(&name))?;
    let (song_at, _, _) = entries(&mut zip, &name, &Limits::NORMAL)?;
    let (bytes, title) = read_song(&mut zip, song_at, &name, &Limits::NORMAL)?;
    sp_model::read_pro_file(&bytes, &title).map_err(|why| format!("{name} could not be read ({why})."))
}

pub fn open_with(path: &Path, staging: &Path, limits: &Limits) -> Result<Opened, String> {
    let name = file_name(path);
    let mut zip = archive(path).map_err(|_| not_a_bundle(&name))?;
    let (song_at, labels, left_out) = entries(&mut zip, &name, limits)?;
    let (bytes, title) = read_song(&mut zip, song_at, &name, limits)?;
    let document =
        sp_model::read_pro_document(&bytes, &title).map_err(|why| format!("{name} could not be read ({why})."))?;
    std::fs::create_dir_all(staging).map_err(|e| format!("Could not make room for the bundle: {e}"))?;
    let room = pack::free_space(staging).map_or(u64::MAX, |free| free.saturating_sub(limits.keep_free));
    let mut opened = Opened {
        song: document.song,
        files: Vec::new(),
        slides: Vec::new(),
        missing: Vec::new(),
        left_out,
        staging: staging.to_path_buf(),
    };
    // Each entry taken out once, however many slides show it; and an entry that couldn't be,
    // named once.
    let mut taken: HashMap<usize, Option<usize>> = HashMap::new();
    let mut written: u64 = 0;
    for (slide, media) in &document.media {
        let Some(found) = find(&labels, media) else {
            let missing = shown(&shown_name(media));
            if !opened.missing.contains(&missing) {
                opened.missing.push(missing);
            }
            continue;
        };
        let label = &labels[found];
        let place = match taken.get(&label.at) {
            Some(place) => *place,
            None => {
                let place = take_out(&mut zip, label, media, staging, limits, room, &mut written, &mut opened);
                taken.insert(label.at, place);
                place
            }
        };
        if let Some(place) = place {
            opened.slides.push((*slide, place));
        }
    }
    Ok(opened)
}

/// Takes one picture or video out into the staging folder: at most its cap (and the room left
/// on the disk), whatever the entry says. Its place in `files`, or `None` (and why, in
/// `left_out`).
#[allow(clippy::too_many_arguments)]
fn take_out(
    zip: &mut Zip,
    label: &Label,
    media: &ProMedia,
    staging: &Path,
    limits: &Limits,
    room: u64,
    written: &mut u64,
    opened: &mut Opened,
) -> Option<usize> {
    let file = file_part(&label.name).to_owned();
    let path = Path::new(&file);
    let refuse = |opened: &mut Opened, why: &str| opened.left_out.push(format!("{} ({why})", shown(&label.name)));
    if !crate::media::is_media(path) {
        refuse(opened, "not a picture or video SoftPresenter shows");
        return None;
    }
    let video = crate::media::is_video(path);
    let left = room.saturating_sub(*written);
    if label.size > left {
        refuse(opened, "not enough room on this disk");
        return None;
    }
    let cap = left.min(if video { limits.video } else { limits.picture });
    let ending = path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
    let into = staging.join(format!("{}{ending}", Uuid::new_v4()));
    let mut entry = match zip.by_index(label.at) {
        Ok(entry) => entry,
        Err(_) => {
            refuse(opened, "damaged");
            return None;
        }
    };
    match pack::put_file(&mut entry, &into, cap) {
        Ok(size) => {
            *written += size;
            let name = shown(&file);
            opened.files.push(Staged { file: into, name, video, looping: video && media.looping });
            Some(opened.files.len() - 1)
        }
        Err(why) => {
            refuse(opened, &why);
            None
        }
    }
}

/// Brings a bundle's pictures and videos into the library's Media folder (or finds the file
/// with the same contents there already), and has each slide of `song` (the bundle's song, or
/// a copy of it) show its one. `item_for` gives the media bin item for a file in the library
/// (adding one if the bin hasn't got it); a file just copied in that it gives none for (one
/// that can't be read as a picture or video) is taken out again. Says the files that couldn't
/// be brought.
pub fn bring(
    song: &mut Presentation,
    opened: &Opened,
    library: &Path,
    brought: &mut pack::Brought,
    mut item_for: impl FnMut(&Path, &Staged) -> Option<Uuid>,
) -> Vec<String> {
    let dir = library.join("Media");
    let mut failed = Vec::new();
    let items: Vec<Option<Uuid>> = opened
        .files
        .iter()
        .map(|staged| {
            let before = (brought.pictures, brought.videos);
            let placed = pack::place(&staged.file, &staged.name, &dir, brought, staged.video).map(|n| dir.join(n));
            let item = placed.as_deref().and_then(|path| item_for(path, staged));
            if item.is_none() {
                if let Some(path) = placed.filter(|_| (brought.pictures, brought.videos) != before) {
                    let _ = std::fs::remove_file(path);
                    (brought.pictures, brought.videos) = before;
                }
                failed.push(staged.name.clone());
            }
            item
        })
        .collect();
    for (id, place) in &opened.slides {
        let Some(item) = items.get(*place).copied().flatten() else { continue };
        if let Some(slide) = song.groups.iter_mut().flat_map(|g| &mut g.slides).find(|s| s.id == *id) {
            slide.actions.retain(|action| !matches!(action, ShowAction::Media { .. }));
            slide.actions.push(ShowAction::Media { id: item });
        }
    }
    failed
}

/// What a bundle's name says about it, for messages.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn not_a_bundle(name: &str) -> String {
    format!("{name} isn't a song bundle SoftPresenter can read.")
}

/// An entry of a bundle that might be a picture or video: its place, its name (a label only)
/// and the size it says it has.
struct Label {
    at: usize,
    name: String,
    size: u64,
}

type Zip = zip::ZipArchive<Mended<std::io::BufReader<std::fs::File>>>;

/// Looks through a bundle's entries: the song's place, the entries that may be pictures or
/// videos, and what was left out (and why).
fn entries(zip: &mut Zip, name: &str, limits: &Limits) -> Result<(usize, Vec<Label>, Vec<String>), String> {
    if zip.len() > limits.entries {
        return Err(format!("{name} has too many files in it to be a song bundle."));
    }
    let (mut song, mut labels, mut left_out) = (None, Vec::new(), Vec::new());
    for at in 0..zip.len() {
        let entry = zip.by_index_raw(at).map_err(|_| not_a_bundle(name))?;
        let raw = String::from_utf8_lossy(entry.name_raw()).into_owned();
        if entry.is_dir() || is_junk(&raw) {
            continue;
        }
        if entry.is_symlink() || entry.encrypted() || climbs(&raw) {
            left_out.push(format!("{} (not a plain file in the bundle)", shown(&raw)));
            continue;
        }
        if is_song(&raw) {
            match song {
                None => song = Some(at),
                Some(_) => left_out.push(format!("{} (a second song)", shown(&raw))),
            }
            continue;
        }
        labels.push(Label { at, name: raw, size: entry.size() });
    }
    let song = song.ok_or_else(|| format!("{name} has no song in it."))?;
    Ok((song, labels, left_out))
}

/// The song document's bytes (at most its cap, whatever the entry says) and the title from
/// its file name.
fn read_song(zip: &mut Zip, at: usize, name: &str, limits: &Limits) -> Result<(Vec<u8>, String), String> {
    let mut entry = zip.by_index(at).map_err(|_| not_a_bundle(name))?;
    let raw = String::from_utf8_lossy(entry.name_raw()).into_owned();
    let mut bytes = Vec::new();
    entry
        .by_ref()
        .take(limits.song.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{name} could not be read ({e})."))?;
    if bytes.len() as u64 > limits.song {
        return Err(format!("{name}: its song is too large to be a song."));
    }
    let file = file_part(&raw);
    let title = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    Ok((bytes, shown(title)))
}

/// The last part of an entry's name, with either kind of slash.
fn file_part(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// The song document: a `.pro` file, not hidden.
fn is_song(name: &str) -> bool {
    let file = file_part(name);
    !file.starts_with('.') && file.rsplit_once('.').is_some_and(|(_, ending)| ending.eq_ignore_ascii_case("pro"))
}

/// What the Mac's Finder adds when it makes a zip file (`__MACOSX/…`, `._name`, `.DS_Store`):
/// passed over without a word.
fn is_junk(name: &str) -> bool {
    let file = file_part(name);
    name.split(['/', '\\']).any(|part| part == "__MACOSX") || file.starts_with("._") || file == ".DS_Store"
}

/// A name that climbs out of its folder (`..`), or holds characters no file name has.
fn climbs(name: &str) -> bool {
    name.split(['/', '\\']).any(|part| part == "..") || name.chars().any(char::is_control)
}

/// A path to compare: either kind of slash, no leading slash or drive, in lower case.
fn normal(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.trim_start_matches('/');
    let path = match path.split_once(':') {
        Some((drive, rest)) if drive.len() == 1 && drive.chars().all(|c| c.is_ascii_alphabetic()) => {
            rest.trim_start_matches('/')
        }
        _ => path,
    };
    path.to_lowercase()
}

/// The path in a `file://` address, with its `%20`s and the like turned back into letters; any
/// other name as it is.
fn address_path(address: &str) -> String {
    let Some(rest) = address.get(..7).filter(|scheme| scheme.eq_ignore_ascii_case("file://")).map(|_| &address[7..])
    else {
        return address.to_owned();
    };
    // `file:///path`, or `file://host/path`: the path starts at the first slash.
    let path = rest.find('/').map_or(rest, |at| &rest[at..]);
    let mut bytes = Vec::with_capacity(path.len());
    let mut chars = path.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let hex: Vec<u8> = chars.clone().take(2).collect();
            if let Some(value) =
                std::str::from_utf8(&hex).ok().filter(|h| h.len() == 2).and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                bytes.push(value);
                chars.nth(1);
                continue;
            }
        }
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The names a song gives a picture or video, as paths to compare.
fn wanted(media: &ProMedia) -> Vec<String> {
    let mut names: Vec<String> = media
        .names
        .iter()
        .enumerate()
        .map(|(at, name)| if at == 0 { address_path(name) } else { name.clone() })
        .map(|name| normal(&name))
        .filter(|name| !name.is_empty())
        .collect();
    names.dedup();
    names
}

/// The file name a picture or video is known by, for saying it is missing.
fn shown_name(media: &ProMedia) -> String {
    let names =
        media.names.iter().enumerate().map(|(at, name)| if at == 0 { address_path(name) } else { name.clone() });
    names.map(|name| file_part(&name).to_owned()).find(|name| !name.is_empty()).unwrap_or_default()
}

/// The entry holding a picture or video the song shows: the one with its whole path, else the
/// one whose path ends with the path the song gives (from a known folder, say), else the only
/// one with its file name. `None` when none (or more than one) fits.
fn find(labels: &[Label], media: &ProMedia) -> Option<usize> {
    let names = wanted(media);
    let paths: Vec<String> = labels.iter().map(|label| normal(&label.name)).collect();
    let only = |fits: &dyn Fn(&str, &str) -> bool| {
        for name in &names {
            let mut found = paths.iter().enumerate().filter(|(_, path)| fits(path, name));
            if let (Some((at, _)), None) = (found.next(), found.next()) {
                return Some(at);
            }
        }
        None
    };
    only(&|path, name| path == name)
        .or_else(|| only(&|path, name| path.ends_with(&format!("/{name}"))))
        .or_else(|| only(&|path, name| file_part(path) == file_part(name)))
}

/// Opens a bundle as a zip file, with the place of its ZIP64 end record put right if the record
/// pointing to it has it wrong.
fn archive(path: &Path) -> Result<Zip, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let patch = mended_locator(&mut file).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let reader = Mended { inner: std::io::BufReader::new(file), pos: 0, patch };
    zip::ZipArchive::new(reader).map_err(|e| e.to_string())
}

const END: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const END64: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
const LOCATOR: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];

/// Where the record pointing to a ZIP64 end record says it is, and where it really is, when
/// those differ: the place of the eight bytes to put right, and what goes there. `None` for a
/// file that is fine, or isn't a ZIP64 zip file at all.
fn mended_locator(file: &mut (impl Read + Seek)) -> std::io::Result<Option<(u64, [u8; 8])>> {
    let length = file.seek(SeekFrom::End(0))?;
    if length < 22 + 20 + 56 {
        return Ok(None);
    }
    // The end record (22 bytes, and a comment of up to 65,535), the locator (20) and a ZIP64
    // end record (56 at least) are all in the last part of the file.
    let tail_length = length.min(22 + 65_535 + 20 + 56 + 1024);
    let base = length - tail_length;
    file.seek(SeekFrom::Start(base))?;
    let mut tail = vec![0u8; usize::try_from(tail_length).unwrap_or(0)];
    file.read_exact(&mut tail)?;
    let u16_at = |at: usize| u64::from(u16::from_le_bytes([tail[at], tail[at + 1]]));
    let u64_at = |at: usize| u64::from_le_bytes(tail[at..at + 8].try_into().unwrap_or([0; 8]));
    // The end record: the last one whose comment reaches exactly to the end of the file.
    let Some(end) = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&at| tail[at..at + 4] == END && at as u64 + 22 + u16_at(at + 20) == tail_length)
    else {
        return Ok(None);
    };
    let Some(locator) = end.checked_sub(20).filter(|&at| tail[at..at + 4] == LOCATOR) else {
        return Ok(None);
    };
    // A ZIP64 end record ends where its locator starts.
    let ends_at_locator = |at: usize| {
        at + 12 <= locator
            && tail[at..at + 4] == END64
            && (at as u64 + 12).checked_add(u64_at(at + 4)) == Some(locator as u64)
    };
    let stated = u64_at(locator + 8);
    if stated.checked_sub(base).and_then(|at| usize::try_from(at).ok()).is_some_and(ends_at_locator) {
        return Ok(None);
    }
    let found = (0..=locator.saturating_sub(56)).rev().find(|&at| ends_at_locator(at));
    Ok(found.map(|at| (base + locator as u64 + 8, (base + at as u64).to_le_bytes())))
}

/// A file read as it is, except for eight bytes put right (see [`mended_locator`]).
struct Mended<R> {
    inner: R,
    pos: u64,
    patch: Option<(u64, [u8; 8])>,
}

impl<R: Read> Read for Mended<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if let Some((at, bytes)) = self.patch {
            for (n, byte) in bytes.iter().enumerate() {
                let place = at + n as u64;
                if let Some(into) =
                    place.checked_sub(self.pos).and_then(|i| usize::try_from(i).ok()).filter(|i| *i < read)
                {
                    buf[into] = *byte;
                }
            }
        }
        self.pos += read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for Mended<R> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.pos = self.inner.seek(to)?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use sp_model::pro_example::song_showing;

    use super::*;

    /// A fresh folder under the system temporary folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("softpresenter-bundle-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A small picture made for the test: 2 by 2, in four colours.
    fn picture() -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]).unwrap();
        writer.finish().unwrap();
        bytes
    }

    /// Writes a zip file of the entries given, by their names exactly as given.
    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn names_in(path: &Path) -> Vec<String> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        (0..zip.len()).map(|at| String::from_utf8_lossy(zip.by_index(at).unwrap().name_raw()).into_owned()).collect()
    }

    fn files_in(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(dir).map(|read| read.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        files.sort();
        files
    }

    #[test]
    fn an_exported_bundle_with_whole_paths_brings_its_song_and_pictures() {
        let dir = TempDir::new("exported");
        let picture = picture();
        let song = song_showing(&[
            ("file:///Volumes/Church/Backgrounds/Sky%20Blue.png", "Backgrounds/Sky Blue.png", None),
            ("file:///Volumes/Church/Clips/Waves.mov", "Clips/Waves.mov", Some(true)),
            // A second slide with the same picture: it is taken out once.
            ("file:///Volumes/Church/Backgrounds/Sky%20Blue.png", "Backgrounds/Sky Blue.png", None),
        ]);
        let bundle = dir.0.join("Harbour Lights.proBundle");
        zip_of(
            &bundle,
            &[
                ("/Volumes/Church/Backgrounds/Sky Blue.png", &picture),
                ("/Volumes/Church/Clips/Waves.mov", b"not really a film, but its bytes come across"),
                ("Harbour Lights.pro", &song),
            ],
        );
        assert!(names_in(&bundle)[0].starts_with('/'), "the entry keeps its leading slash");
        let staging = dir.0.join(".opening-pack-1");
        let opened = open(&bundle, &staging).unwrap();
        assert_eq!(opened.song.title, "Harbour Lights");
        assert_eq!(opened.song.slide_count(), 4);
        assert_eq!(opened.files.len(), 2, "{:?}", opened.files);
        assert_eq!(opened.slides.len(), 3);
        assert!(opened.missing.is_empty() && opened.left_out.is_empty(), "{opened:?}");
        let (sky, waves) = (&opened.files[0], &opened.files[1]);
        assert_eq!((sky.name.as_str(), sky.video), ("Sky Blue.png", false));
        assert_eq!((waves.name.as_str(), waves.video, waves.looping), ("Waves.mov", true, true));
        assert_eq!(std::fs::read(&sky.file).unwrap(), picture);
        // Taken out under names the app made, inside the staging folder.
        for staged in &opened.files {
            assert_eq!(staged.file.parent(), Some(staging.as_path()));
            assert!(Uuid::parse_str(staged.file.file_stem().unwrap().to_str().unwrap()).is_ok());
        }
    }

    #[test]
    fn a_bundle_with_bare_file_names_brings_them_into_the_library_and_the_slides_show_them() {
        let dir = TempDir::new("flat");
        let picture = picture();
        let song = song_showing(&[("Sky.png", "Sky.png", None), ("Sky.png", "Sky.png", None)]);
        let bundle = dir.0.join("Harbour Lights.probundle");
        zip_of(&bundle, &[("Sky.png", &picture), ("Harbour Lights.pro", &song)]);
        let opened = open(&bundle, &dir.0.join("staging")).unwrap();
        assert_eq!((opened.files.len(), opened.slides.len()), (1, 2));
        let mut song = opened.song.clone();
        // Brought into the library: the picture is moved into its Media folder under a new
        // name, and both slides show the one bin item made for it.
        let library = dir.0.join("library");
        let item = Uuid::new_v4();
        let mut asked = Vec::new();
        let mut brought = pack::Brought::default();
        let failed = bring(&mut song, &opened, &library, &mut brought, |path, staged| {
            asked.push((path.to_path_buf(), staged.name.clone()));
            Some(item)
        });
        assert!(failed.is_empty());
        assert_eq!(asked.len(), 1);
        let (placed, shown) = &asked[0];
        assert_eq!(shown, "Sky.png");
        assert_eq!(placed.parent(), Some(library.join("Media").as_path()));
        assert_eq!(std::fs::read(placed).unwrap(), picture);
        assert!(files_in(&opened.staging).is_empty(), "nothing is left pointing into the staging folder");
        let showing: Vec<Option<Uuid>> = song.groups[0]
            .slides
            .iter()
            .map(|slide| {
                slide.actions.iter().find_map(|a| match a {
                    ShowAction::Media { id } => Some(*id),
                    _ => None,
                })
            })
            .collect();
        assert_eq!(showing, vec![Some(item), Some(item), None]);
        // The same bundle again finds the picture in the library already: nothing is added.
        let again = open(&bundle, &dir.0.join("staging-2")).unwrap();
        let mut brought = pack::Brought::default();
        bring(&mut again.song.clone(), &again, &library, &mut brought, |path, _| {
            assert_eq!(path, placed);
            Some(item)
        });
        assert_eq!((brought.pictures, brought.already), (0, 1));
        assert_eq!(files_in(&library.join("Media")).len(), 1);
        // A picture the bin can't read is not left behind in a library it was copied into, and
        // its slides show nothing.
        let other = open(&bundle, &dir.0.join("staging-3")).unwrap();
        let fresh = dir.0.join("fresh library");
        let mut brought = pack::Brought::default();
        let mut song = other.song.clone();
        let failed = bring(&mut song, &other, &fresh, &mut brought, |_, _| None);
        assert_eq!(failed, vec!["Sky.png"]);
        assert_eq!((brought.pictures, files_in(&fresh.join("Media")).len()), (0, 0));
        assert!(song.groups[0].slides.iter().all(|slide| slide.actions.is_empty()));
    }

    #[test]
    fn a_bundle_whose_pictures_are_missing_still_brings_its_words_and_names_what_is_missing() {
        let dir = TempDir::new("missing");
        let song = song_showing(&[
            ("file:///Volumes/Church/Backgrounds/Sky%20Blue.png", "Backgrounds/Sky Blue.png", None),
            ("Clip.mp4", "Clip.mp4", Some(false)),
        ]);
        let bundle = dir.0.join("Harbour Lights.proBundle");
        zip_of(&bundle, &[("Harbour Lights.pro", &song)]);
        let opened = open(&bundle, &dir.0.join("staging")).unwrap();
        assert_eq!(opened.song.slide_count(), 3);
        assert!(opened.files.is_empty() && opened.slides.is_empty());
        assert_eq!(opened.missing, vec!["Sky Blue.png", "Clip.mp4"]);
        // Read for its words alone, it is the same song.
        assert_eq!(read_words(&bundle).unwrap().slide_count(), 3);
        // No song in it at all, or not a zip file: said plainly.
        let empty = dir.0.join("Empty.proBundle");
        zip_of(&empty, &[("Sky.png", &picture())]);
        assert!(open(&empty, &dir.0.join("s2")).unwrap_err().contains("has no song in it"));
        let text = dir.0.join("Text.proBundle");
        std::fs::write(&text, b"just words").unwrap();
        assert!(open(&text, &dir.0.join("s3")).unwrap_err().contains("isn't a song bundle"));
    }

    #[test]
    fn names_that_climb_out_and_entries_too_large_are_left_out_and_nothing_lands_outside() {
        let dir = TempDir::new("hostile");
        let inside = dir.0.join("inside");
        std::fs::create_dir_all(&inside).unwrap();
        let picture = picture();
        let song = song_showing(&[
            ("../../escaped.png", "../../escaped.png", None),
            ("/tmp/../../escaped2.png", "", None),
            ("Big.png", "Big.png", None),
            ("Fine.png", "Fine.png", None),
        ]);
        let bundle = dir.0.join("Harbour Lights.proBundle");
        let big = vec![7u8; 5000];
        zip_of(
            &bundle,
            &[
                ("../../escaped.png", &picture),
                ("/tmp/../../escaped2.png", &picture),
                ("__MACOSX/._Fine.png", b"finder's extra"),
                ("Big.png", &big),
                ("Fine.png", &picture),
                ("Harbour Lights.pro", &song),
            ],
        );
        let staging = inside.join("staging");
        let limits = Limits { picture: 1000, ..Limits::NORMAL };
        let opened = open_with(&bundle, &staging, &limits).unwrap();
        assert_eq!(opened.files.len(), 1, "only the fine picture comes out: {opened:?}");
        assert_eq!(opened.files[0].name, "Fine.png");
        let left: Vec<&str> = opened.left_out.iter().map(String::as_str).collect();
        assert_eq!(
            left,
            [
                "../../escaped.png (not a plain file in the bundle)",
                "/tmp/../../escaped2.png (not a plain file in the bundle)",
                "Big.png (too large)"
            ]
        );
        assert_eq!(opened.missing, vec!["escaped.png", "escaped2.png"]);
        // On the disk: only the staging folder, holding only the fine picture.
        assert_eq!(files_in(&inside), vec![staging.clone()]);
        assert_eq!(files_in(&staging).len(), 1);
        assert_eq!(files_in(&dir.0).len(), 2, "the bundle and the folder: {:?}", files_in(&dir.0));
        assert!(!dir.0.parent().unwrap().join("escaped.png").exists());
        // A song itself larger than its cap is refused.
        let small = Limits { song: 10, ..Limits::NORMAL };
        assert!(open_with(&bundle, &dir.0.join("s2"), &small).unwrap_err().contains("too large"));
        // So is a bundle of more entries than a bundle has.
        let few = Limits { entries: 3, ..Limits::NORMAL };
        assert!(open_with(&bundle, &dir.0.join("s3"), &few).unwrap_err().contains("too many files"));
    }

    #[test]
    fn a_picture_is_found_by_its_path_from_a_known_folder_or_a_name_only_one_entry_has() {
        let labels = |names: &[&str]| -> Vec<Label> {
            names.iter().enumerate().map(|(at, name)| Label { at, name: (*name).to_owned(), size: 1 }).collect()
        };
        let media = |address: &str, local: &str| ProMedia {
            names: vec![address.to_owned(), local.to_owned(), String::new()],
            ..ProMedia::default()
        };
        let entries = labels(&["/Volumes/A/Media/Sky.png", "/Volumes/B/Other/Sky.png", "C:\\Shows\\Media\\Sea.JPG"]);
        // The whole path, whatever slashes and case.
        assert_eq!(find(&entries, &media("file:///Volumes/B/Other/Sky.png", "")), Some(1));
        assert_eq!(find(&entries, &media("file:///C:/Shows/Media/sea.jpg", "")), Some(2));
        // Moved: the path from a known folder still fits only one.
        assert_eq!(find(&entries, &media("file:///Users/Gone/Media/Sky.png", "A/Media/Sky.png")), Some(0));
        // Only a file name, which two entries have: neither is guessed at.
        assert_eq!(find(&entries, &media("Sky.png", "Sky.png")), None);
        assert_eq!(find(&entries, &media("Sea.jpg", "")), Some(2));
        assert_eq!(
            address_path("file://localhost/Volumes/A/Two%20Words%E2%80%94Dash.png"),
            "/Volumes/A/Two Words\u{2014}Dash.png"
        );
        assert_eq!(address_path("Sky%20.png"), "Sky%20.png", "a bare name is not an address");
    }

    /// A zip file with a ZIP64 end, written by hand: stored entries, the central directory,
    /// the ZIP64 end record, the locator (saying the end record is `shift` bytes from where it
    /// is) and the end record.
    fn zip64_of(entries: &[(&str, &[u8])], shift: i64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, bytes) in entries {
            let mut crc = flate2::Crc::new();
            crc.update(bytes);
            let (crc, size, offset) = (crc.sum(), bytes.len() as u32, out.len() as u32);
            let common = |out: &mut Vec<u8>| {
                out.extend(20u16.to_le_bytes()); // version needed
                out.extend(0u16.to_le_bytes()); // flags
                out.extend(0u16.to_le_bytes()); // stored
                out.extend([0u8; 4]); // time and date
                out.extend(crc.to_le_bytes());
                out.extend(size.to_le_bytes());
                out.extend(size.to_le_bytes());
                out.extend((name.len() as u16).to_le_bytes());
                out.extend(0u16.to_le_bytes()); // no extra field
            };
            out.extend([0x50, 0x4b, 0x03, 0x04]);
            common(&mut out);
            out.extend(name.as_bytes());
            out.extend(*bytes);
            central.extend([0x50, 0x4b, 0x01, 0x02]);
            central.extend(45u16.to_le_bytes()); // made by
            common(&mut central);
            central.extend([0u8; 8]); // comment length, disk, internal and external attributes
            central.extend([0u8; 2]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let (central_at, count) = (out.len() as u64, entries.len() as u64);
        out.extend(&central);
        let end64_at = out.len() as u64;
        out.extend(END64);
        out.extend(44u64.to_le_bytes());
        out.extend(45u16.to_le_bytes());
        out.extend(45u16.to_le_bytes());
        out.extend([0u8; 8]); // this disk, the disk with the directory
        out.extend(count.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend((central.len() as u64).to_le_bytes());
        out.extend(central_at.to_le_bytes());
        out.extend(LOCATOR);
        out.extend(0u32.to_le_bytes());
        out.extend(end64_at.wrapping_add_signed(shift).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(END);
        out.extend([0u8; 4]);
        out.extend([0xff; 4]); // entries: see the ZIP64 end
        out.extend([0xff; 8]); // directory size and place: see the ZIP64 end
        out.extend(0u16.to_le_bytes());
        out
    }

    #[test]
    fn a_zip64_end_record_put_in_the_wrong_place_is_found_either_way() {
        let dir = TempDir::new("zip64");
        let picture = picture();
        let song = song_showing(&[("Sky.png", "Sky.png", None)]);
        let entries: [(&str, &[u8]); 2] = [("Sky.png", &picture), ("Harbour Lights.pro", &song)];
        let read_all = |bytes: &[u8]| -> Result<Vec<Vec<u8>>, String> {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            (0..zip.len())
                .map(|at| {
                    let mut out = Vec::new();
                    zip.by_index(at).map_err(|e| e.to_string())?.read_to_end(&mut out).map_err(|e| e.to_string())?;
                    Ok(out)
                })
                .collect()
        };
        // Written right, the zip reader reads it, and nothing needs putting right.
        let right = zip64_of(&entries, 0);
        assert_eq!(read_all(&right).unwrap(), vec![picture.clone(), song.clone()]);
        assert_eq!(mended_locator(&mut std::io::Cursor::new(&right)).unwrap(), None);
        for shift in [98, -98] {
            let wrong = zip64_of(&entries, shift);
            assert_ne!(
                read_all(&wrong).ok(),
                Some(vec![picture.clone(), song.clone()]),
                "a plain zip reader can't read it {shift} out"
            );
            let bundle = dir.0.join(format!("Shifted {shift}.proBundle"));
            std::fs::write(&bundle, &wrong).unwrap();
            let opened = open(&bundle, &dir.0.join(format!("staging {shift}"))).unwrap();
            assert_eq!(opened.song.title, "Harbour Lights");
            assert_eq!(std::fs::read(&opened.files[0].file).unwrap(), picture);
        }
    }
}
