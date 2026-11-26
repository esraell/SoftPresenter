//! Where installed Bibles are kept on this computer, and fetching new ones.
//!
//! A downloaded file is only kept if its checksum is the one this version of the app expects
//! and it reads as a Bible, so a damaged or swapped file is never installed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::i18n::{t, tf};
use sha2::{Digest, Sha256};
use sp_bible::{Bible, Info};

/// Where the Bible files are published: the address their names are added to. They are
/// files attached to a release of the project on GitHub. While the project is private this
/// address answers nobody, and Install says the download could not be made.
const DOWNLOADS: Option<&str> = Some("https://github.com/esraell/SoftPresenter/releases/download/bibles-4");

/// The largest Bible file that will be fetched. The biggest so far is under 2 MB.
const LARGEST: u64 = 16 * 1024 * 1024;

/// The address Bible files are fetched from, if there is one yet. A testing aid can name
/// another with `SOFTPRESENTER_BIBLE_URL`; the checksum is checked all the same.
pub fn download_base() -> Option<String> {
    std::env::var("SOFTPRESENTER_BIBLE_URL").ok().filter(|url| !url.is_empty()).or_else(|| DOWNLOADS.map(str::to_owned))
}

/// The folder installed Bibles are kept in.
pub fn folder() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("Bibles"))
}

/// The folder the app keeps its own files in. `SOFTPRESENTER_DATA` names another place,
/// which tests use.
pub fn data_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if let Some(base) = var("SOFTPRESENTER_DATA") {
        base
    } else if cfg!(target_os = "macos") {
        // ~/Documents/SoftPresenter, or the old place until the library has moved there.
        crate::library_folder::folder_in(&var("HOME")?)
    } else if cfg!(target_os = "windows") {
        var("APPDATA")?.join("SoftPresenter")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local/share")))?.join("softpresenter")
    };
    Some(base)
}

/// The Bible files in the folder that read as Bibles, with what each says about itself.
pub fn installed() -> Vec<(PathBuf, Info)> {
    let Some(entries) = folder().and_then(|folder| std::fs::read_dir(folder).ok()) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Info)> = entries
        .flatten()
        .map(|entry| entry.path())
        // Bibles kept before the project was renamed end in .pcbible; they still open.
        .filter(|path| path.extension().is_some_and(|ext| ext == "spbible" || ext == "pcbible"))
        .filter_map(|path| {
            let info = Bible::read_info(&std::fs::read(&path).ok()?).ok()?;
            Some((path, info))
        })
        .collect();
    found.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    found
}

fn checksum(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A file name made only from the Bible's short name's safe letters, so what a file says about
/// itself can never choose where it is written; never one Windows keeps for a device.
fn file_name(id: &str) -> Option<String> {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    let safe = if sp_bible::import::is_device_name(&safe) { format!("{safe}-bible") } else { safe };
    (!safe.is_empty() && safe.len() <= 40).then(|| format!("{safe}.spbible"))
}

/// Checks that the bytes are a whole Bible and keeps them in the folder under its own short
/// name for files; returns where, and what the Bible says about itself. With `trash_old`, a
/// file already there under that name goes to the Trash first, as Remove puts it there, so the
/// copy replaced can be put back.
fn install_bytes_in(folder: &Path, bytes: &[u8], trash_old: bool) -> Result<(PathBuf, Info), String> {
    let bible = Bible::from_bytes(bytes)?;
    let name = file_name(&bible.info.id)
        .ok_or_else(|| t("bibles.store.no_id", "This Bible file does not say which Bible it is.").to_owned())?;
    std::fs::create_dir_all(folder).map_err(|e| {
        tf(
            "bibles.store.no_folder",
            "Could not make the Bibles folder: {error}",
            &[("error", &crate::notices::write_problem(&e))],
        )
    })?;
    let path = folder.join(name);
    // Write beside it and rename, so a half-written file is never left under the real name.
    let partial = path.with_extension("spbible.part");
    std::fs::write(&partial, bytes).map_err(|e| {
        tf(
            "bibles.store.not_saved",
            "Could not save the Bible: {error}",
            &[("error", &crate::notices::write_problem(&e))],
        )
    })?;
    if trash_old && path.is_file() {
        crate::sweep::to_trash(&path).map_err(|e| {
            let _ = std::fs::remove_file(&partial);
            tf(
                "bibles.store.not_trashed",
                "Could not move the earlier copy to the Trash: {error}",
                &[("error", &e.to_string())],
            )
        })?;
    }
    std::fs::rename(&partial, &path).map_err(|e| {
        tf(
            "bibles.store.not_saved",
            "Could not save the Bible: {error}",
            &[("error", &crate::notices::write_problem(&e))],
        )
    })?;
    // The same Bible kept before the rename is replaced, not left beside it.
    let _ = std::fs::remove_file(path.with_extension("pcbible"));
    Ok((path, bible.info))
}

/// True for a SoftPresenter Bible file.
fn is_own_bible_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("spbible") || ext.eq_ignore_ascii_case("pcbible"))
}

/// True for a Bible the app can install: its own file, or a Bible in another program's format
/// (a file, a zip or a folder of books), told by what it holds (a SoftProjector Bible, `.spb`,
/// by its ending too). Only files whose ending a Bible may have (or none) are looked into, and
/// only their first few kilobytes.
pub fn is_bible_file(path: &Path) -> bool {
    if is_own_bible_file(path) {
        return true;
    }
    let ending = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    match ending.as_deref() {
        None | Some("xml" | "xmm" | "zip" | "usfm" | "sfm" | "usx" | "osis" | "zef" | "txt" | "bible" | "spb") => {
            sp_bible::import::looks_like_bible(path)
        }
        _ => false,
    }
}

/// One Bible on the shelf, as an install from a file needs to know it.
#[derive(Clone, Debug)]
pub struct KnownBible {
    pub id: String,
    pub name: String,
    pub short: String,
    /// It comes with the app.
    pub shipped: bool,
}

/// The Bibles on the shelf when an install from a file starts: it runs on a thread of its own,
/// away from the shelf, and decides from this which short name for files is free and which
/// earlier copy it replaces.
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub bibles: Vec<KnownBible>,
}

/// The Bible offered for download under this short name for files, in any case.
fn offered(id: &str) -> Option<&'static crate::bible::Offered> {
    crate::bible::OFFERED.iter().find(|bible| bible.id.eq_ignore_ascii_case(id))
}

impl Known {
    /// True when the short name for files is in use, in any case (the Mac's disk doesn't tell
    /// "ASV" from "asv"): by a Bible on the shelf, one offered, or a file in the folder.
    fn taken(&self, folder: &Path, id: &str) -> bool {
        self.bibles.iter().any(|bible| bible.id.eq_ignore_ascii_case(id))
            || offered(id).is_some()
            || file_name(id)
                .is_some_and(|name| folder.join(&name).exists() || folder.join(name).with_extension("pcbible").exists())
    }

    /// A free short name for files, from `base`: itself, else `base-2`, `base-3`…
    fn free(&self, folder: &Path, base: &str) -> Result<String, String> {
        std::iter::once(base.to_owned())
            .chain((2..1000).map(|n| format!("{base}-{n}")))
            .find(|id| !self.taken(folder, id))
            .ok_or_else(|| t("bibles.store.too_many", "Too many Bibles are installed under that name.").to_owned())
    }

    /// The Bible installed from a file before (never one built in or offered) under this short
    /// name for files, in any case.
    fn installed(&self, id: &str) -> Option<&KnownBible> {
        self.bibles
            .iter()
            .find(|bible| !bible.shipped && offered(&bible.id).is_none() && bible.id.eq_ignore_ascii_case(id))
    }

    /// The same Bible installed from a file before: its name and short name alike.
    fn again(&self, name: &str, short: &str) -> Option<&KnownBible> {
        self.bibles
            .iter()
            .find(|bible| !bible.shipped && offered(&bible.id).is_none() && bible.name == name && bible.short == short)
    }
}

/// How opening a Bible file the operator has went.
#[derive(Debug)]
pub enum Opened {
    /// It was installed: where it is kept, what it says about itself, and what to tell the
    /// operator ("Installed … (66 books, 31,102 verses) …", or "Replaced your earlier …").
    Installed(PathBuf, Info, String),
    /// It was read, but doesn't say its name, short name or language: they are asked for, and
    /// then [`Work::Install`] installs it.
    NeedsDetails(sp_bible::import::Imported),
}

/// What a thread of its own does with a Bible file (see [`run`]).
#[derive(Debug)]
pub enum Work {
    /// Read the file at a path and install it, with the details given in place of the file's.
    /// With `ask`, one that doesn't say what it is comes back to be asked about; without (the
    /// command line, where nothing can ask), it is refused naming the flags to add.
    Open { path: PathBuf, given: sp_bible::import::About, ask: bool },
    /// Install a Bible already read, under the details the operator gave.
    Install { imported: Box<sp_bible::import::Imported>, details: sp_bible::import::About },
}

/// Only one install writes at a time, so two can't take the same free short name for files.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Does a Bible file's work, away from the window: reading, converting and writing a whole
/// Bible takes a moment, and a file may be made to take longer. `known` is the shelf as the
/// work started. What is installed goes on the shelf with
/// [`crate::bible::Shelf::put_installed`].
pub fn run(work: Work, known: &Known) -> Result<Opened, String> {
    let folder = folder().ok_or_else(|| {
        t("bibles.store.no_place", "Could not find where to keep Bibles on this computer.").to_owned()
    })?;
    run_in(&folder, work, known, true)
}

fn run_in(folder: &Path, work: Work, known: &Known, trash: bool) -> Result<Opened, String> {
    match work {
        Work::Open { path, given, ask } => {
            if is_own_bible_file(&path) {
                return install_own_file(folder, &path, known, trash);
            }
            let imported = sp_bible::import::read_path(&path, &sp_bible::import::Limits::NORMAL)?;
            match imported.details(&given) {
                Some(details) => install_imported(folder, &imported, &details, known, trash),
                None if ask => Ok(Opened::NeedsDetails(imported)),
                None => Err(missing_details(&imported, &given)),
            }
        }
        Work::Install { imported, details } => install_imported(folder, &imported, &details, known, trash),
    }
}

/// What the command line says of a file that doesn't say what it is: the flags to add, with
/// what would be offered.
fn missing_details(imported: &sp_bible::import::Imported, given: &sp_bible::import::About) -> String {
    let missing = imported.missing_flags(given);
    let offered = imported.prefilled();
    format!(
        "{} doesn't say its {}. Give {} too, such as: --name \"{}\" --short {} --language {}",
        imported.file_name,
        missing.replace("--short", "short name").replace("--", ""),
        missing,
        offered.name,
        if offered.short.is_empty() { "SHORT" } else { offered.short.as_str() },
        if offered.language.is_empty() { "en" } else { offered.language.as_str() }
    )
}

/// Installs a Bible read from another program's file, under the details given. The same Bible
/// installed from a file before (its name and short name alike) is replaced, its earlier copy
/// going to the Trash; otherwise it gets a short name for its file nothing has (`rst`, else
/// `rst-2`…), so it never stands in for a Bible built in, offered or installed.
fn install_imported(
    folder: &Path,
    imported: &sp_bible::import::Imported,
    details: &sp_bible::import::About,
    known: &Known,
    trash: bool,
) -> Result<Opened, String> {
    let _writing = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let again = known.again(&details.name, &details.short).map(|bible| bible.id.clone());
    let id = match &again {
        Some(id) => id.clone(),
        None => known.free(folder, &sp_bible::import::id_from(&details.short, &details.name))?,
    };
    let bible = imported.bible(&id, details);
    let (kept, info) = install_bytes_in(folder, &bible.to_bytes(), trash && again.is_some())?;
    let said = imported.summary(&info.name);
    let said = match again {
        // "Installed X (…)" says "Replaced your earlier X (…)" instead.
        Some(_) => format!("Replaced your earlier {}", said.trim_start_matches("Installed ")),
        None => said,
    };
    Ok(Opened::Installed(kept, info, said))
}

/// Installs a SoftPresenter Bible file the operator has. Its own short name for files is kept
/// unless something else has it: the very file offered for download (its checksum the one the
/// app expects) installs as that Bible; one that names a Bible built in or offered, or a file of
/// another Bible, gets a free one instead, so a dropped file can never stand in for the checked
/// download; one that names a Bible installed from a file before replaces it, the earlier copy
/// going to the Trash.
fn install_own_file(folder: &Path, path: &Path, known: &Known, trash: bool) -> Result<Opened, String> {
    use std::io::Read;
    let shown =
        sp_bible::import::shown_name(&path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
    let meta = std::fs::metadata(path).map_err(|e| {
        tf("bibles.store.cant_read", "Could not read {file}: {error}", &[("file", &shown), ("error", &e.to_string())])
    })?;
    // A pipe or device is never opened: opening one can wait for ever.
    if !meta.is_file() {
        return Err(tf(
            "bibles.store.not_bible",
            "{file} isn't a Bible file SoftPresenter can read.",
            &[("file", &shown)],
        ));
    }
    if meta.len() > LARGEST {
        return Err(t("bibles.store.too_large", "That file is too large to be a Bible.").to_owned());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|file| file.take(LARGEST + 1).read_to_end(&mut bytes)).map_err(|e| {
        tf("bibles.store.cant_read", "Could not read {file}: {error}", &[("file", &shown), ("error", &e.to_string())])
    })?;
    if bytes.len() as u64 > LARGEST {
        return Err(t("bibles.store.too_large", "That file is too large to be a Bible.").to_owned());
    }
    let mut bible = Bible::from_bytes(&bytes)?;
    let own = bible.info.id.clone();
    let _writing = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let the_download = offered(&own).is_some_and(|offered| checksum(&bytes) == offered.checksum);
    let earlier = known.installed(&own).map(|bible| bible.id.clone());
    let (bytes, replacing) = if the_download {
        (bytes, false)
    } else if let Some(id) = &earlier {
        if *id != own {
            bible.info.id.clone_from(id);
            (bible.to_bytes(), true)
        } else {
            (bytes, true)
        }
    } else if known.taken(folder, &own) || file_name(&own).is_none_or(|name| name != format!("{own}.spbible")) {
        let base = sp_bible::import::id_from(&own, &bible.info.name);
        bible.info.id = known.free(folder, &base)?;
        (bible.to_bytes(), false)
    } else {
        (bytes, false)
    };
    let (kept, info) = install_bytes_in(folder, &bytes, trash && replacing)?;
    let said = if replacing {
        tf("bibles.store.replaced", "Replaced your earlier {name}.", &[("name", &info.name)])
    } else {
        tf("bibles.store.installed", "Installed the {name}.", &[("name", &info.name)])
    };
    Ok(Opened::Installed(kept, info, said))
}

/// Downloads a Bible's file, trying once more if the first try fails or comes back short.
fn fetch(address: &str) -> Result<Vec<u8>, String> {
    fetch_once(address).or_else(|_| fetch_once(address))
}

/// One try: it gives up after two minutes, and a file shorter than the server said it was is
/// "cut short", never mistaken for the wrong file.
fn fetch_once(address: &str) -> Result<Vec<u8>, String> {
    let offline = |e: ureq::Error| {
        tf("bibles.store.not_downloaded", "Could not download the Bible: {error}", &[("error", &e.to_string())])
    };
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).build().into();
    let mut response = agent.get(address).call().map_err(offline)?;
    let expected = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let bytes = response.body_mut().with_config().limit(LARGEST).read_to_vec().map_err(offline)?;
    if expected.is_some_and(|expected| expected != bytes.len() as u64) {
        return Err(t("bibles.store.cut_short", "The Bible's download was cut short. Try again.").to_owned());
    }
    Ok(bytes)
}

/// Fetches one Bible and installs it, if it is the file expected.
fn download_into(folder: &Path, address: &str, id: &str, expected_checksum: &str) -> Result<(PathBuf, Info), String> {
    let bytes = fetch(address)?;
    if checksum(&bytes) != expected_checksum {
        return Err(t(
            "bibles.store.wrong_file",
            "The downloaded Bible is not the file this version expects, so it was not installed.",
        )
        .to_owned());
    }
    // Checked before anything is written, so a wrong file leaves nothing behind.
    if Bible::read_info(&bytes)?.id != id {
        return Err(t(
            "bibles.store.other_bible",
            "The downloaded file is a different Bible, so it was not installed.",
        )
        .to_owned());
    }
    install_bytes_in(folder, &bytes, false)
}

/// How a download ended: the Bible's short name for files, and the installed file or why not.
pub type Finished = (&'static str, Result<(PathBuf, Info), String>);

/// Downloads under way. Each runs on its own thread so the window never waits on the network.
pub struct Downloads {
    tx: Sender<Finished>,
    rx: Receiver<Finished>,
    busy: Vec<&'static str>,
}

impl Default for Downloads {
    fn default() -> Downloads {
        let (tx, rx) = channel();
        Downloads { tx, rx, busy: Vec::new() }
    }
}

impl std::fmt::Debug for Downloads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Downloads({:?})", self.busy)
    }
}

impl Downloads {
    pub fn is_busy(&self, id: &str) -> bool {
        self.busy.contains(&id)
    }

    /// Starts fetching a Bible. `wake` is called when it ends, to redraw the window.
    pub fn start(&mut self, id: &'static str, expected_checksum: &'static str, wake: impl FnOnce() + Send + 'static) {
        let Some(base) = download_base() else { return };
        if self.is_busy(id) {
            return;
        }
        self.busy.push(id);
        let tx = self.tx.clone();
        // The files published for downloading keep the ending they were published with.
        let address = format!("{}/{id}.pcbible", base.trim_end_matches('/'));
        let work = move || {
            let result = folder()
                .ok_or_else(|| {
                    t("bibles.store.no_place", "Could not find where to keep Bibles on this computer.").to_owned()
                })
                .and_then(|folder| download_into(&folder, &address, id, expected_checksum));
            let _ = tx.send((id, result));
            wake();
        };
        if std::thread::Builder::new().name("bible download".to_owned()).spawn(work).is_err() {
            self.busy.retain(|busy| *busy != id);
        }
    }

    /// The downloads that have ended since the last look.
    pub fn finished(&mut self) -> Vec<Finished> {
        let done: Vec<Finished> = self.rx.try_iter().collect();
        for (id, _) in &done {
            self.busy.retain(|busy| busy != id);
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use sp_bible::Numbering;

    use super::*;

    fn small_bible(id: &str) -> Vec<u8> {
        let info = Info {
            id: id.to_owned(),
            name: "Test Bible".to_owned(),
            short: "TB".to_owned(),
            language: "en".to_owned(),
            terms: "Public domain".to_owned(),
            credit: String::new(),
            source: "made for a test".to_owned(),
            numbering: Numbering::English,
            loose: false,
        };
        Bible::from_verse_lines(info, "JOH 3:16 For God so loved the world", str::to_owned).to_bytes()
    }

    fn empty_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Answers one request on this computer with the given bytes, and returns its address.
    fn serve_once(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/file.pcbible", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    #[test]
    fn a_file_name_comes_only_from_safe_letters() {
        assert_eq!(file_name("kjv").as_deref(), Some("kjv.spbible"));
        assert_eq!(file_name("../../etc/passwd").as_deref(), Some("etcpasswd.spbible"));
        assert_eq!(file_name("a/b\\c:d").as_deref(), Some("abcd.spbible"));
        assert_eq!(file_name("..//"), None);
        assert_eq!(file_name(&"x".repeat(41)), None);
    }

    #[test]
    fn the_checksum_is_sha256_in_hex() {
        assert_eq!(checksum(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn a_bible_in_another_format_is_installed_under_a_short_name_nothing_else_has() {
        let folder = empty_folder("imported");
        let zefania = "<XMLBIBLE biblename=\"Тестова Біблія\"><INFORMATION><identifier>НПУ</identifier><language>ukr</language></INFORMATION>\
<BIBLEBOOK bnumber=\"43\"><CHAPTER cnumber=\"3\"><VERS vnumber=\"16\">Слова для перевірки.</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>";
        let file = folder.with_extension("xml");
        std::fs::write(&file, zefania).unwrap();
        let open = || Work::Open { path: file.clone(), given: sp_bible::import::About::default(), ask: true };
        // "NPU" is on the shelf (in other letters: the Mac's disk doesn't tell them apart), so
        // the next is used.
        let known = Known {
            bibles: vec![KnownBible {
                id: "NPU".to_owned(),
                name: "Other".to_owned(),
                short: "O".to_owned(),
                shipped: false,
            }],
        };
        let Ok(Opened::Installed(path, info, said)) = run_in(&folder, open(), &known, false) else { panic!() };
        assert_eq!(path, folder.join("npu-2.spbible"));
        assert_eq!((info.id.as_str(), info.short.as_str(), info.language.as_str()), ("npu-2", "НПУ", "uk"));
        assert!(said.starts_with("Installed Тестова Біблія (1 book, 1 verse)"), "{said}");
        assert_eq!(Bible::from_bytes(&std::fs::read(&path).unwrap()).unwrap().verse_count(), 1);
        // A file in the folder takes its name too, though nothing on the shelf has it.
        let Ok(Opened::Installed(path, ..)) = run_in(&folder, open(), &known, false) else { panic!() };
        assert_eq!(path, folder.join("npu-3.spbible"));
        // The same Bible installed again replaces its earlier copy, and says so.
        let known = Known {
            bibles: vec![KnownBible {
                id: "npu-2".to_owned(),
                name: "Тестова Біблія".to_owned(),
                short: "НПУ".to_owned(),
                shipped: false,
            }],
        };
        let Ok(Opened::Installed(path, _, said)) = run_in(&folder, open(), &known, false) else { panic!() };
        assert_eq!(path, folder.join("npu-2.spbible"));
        assert!(said.starts_with("Replaced your earlier Тестова Біблія (1 book"), "{said}");
        // Without the details, the command line is told which flags to add; the window asks.
        let tags = folder.with_extension("tags.xml");
        std::fs::write(&tags, r#"<bible><b n="John"><c n="3"><v n="16">Words.</v></c></b></bible>"#).unwrap();
        let given = sp_bible::import::About::default();
        let refused =
            run_in(&folder, Work::Open { path: tags.clone(), given: given.clone(), ask: false }, &known, false);
        assert!(refused.unwrap_err().contains("Give --name, --short and --language too"));
        let asked = run_in(&folder, Work::Open { path: tags.clone(), given, ask: true }, &known, false);
        assert!(matches!(asked, Ok(Opened::NeedsDetails(_))));
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(&tags);
    }

    #[test]
    fn a_softprojector_bible_is_installed_as_one_of_the_apps_own() {
        let folder = empty_folder("softprojector");
        // As SoftProjector exports one (words written for this test), saved with Windows' line
        // ends: it doesn't say its language.
        let spb = "\u{feff}##spDataVersion:\t1\r\n##Title:\tТестова Біблія\r\n##Abbreviation:\tТБ\r\n\
##Information:\tНаписано для тестів.\r\n##RightToLeft:\t0\r\n43\tВід Івана\t21\r\n-----\r\n\
B043C003V016\t43\t3\t16\tСлова для перевірки, і ще її слова.\r\n";
        let file = folder.with_extension("Тестова Біблія.spb");
        std::fs::write(&file, spb).unwrap();
        assert!(is_bible_file(&file));
        let open = |given: sp_bible::import::About, ask: bool| Work::Open { path: file.clone(), given, ask };
        // The window asks for the language, offering Ukrainian; the command line is told to give it.
        let Ok(Opened::NeedsDetails(imported)) =
            run_in(&folder, open(Default::default(), true), &Known::default(), false)
        else {
            panic!()
        };
        assert_eq!(imported.prefilled().language, "uk");
        let refused = run_in(&folder, open(Default::default(), false), &Known::default(), false).unwrap_err();
        assert!(refused.contains("--language"), "{refused}");
        let given = sp_bible::import::About { language: "uk".to_owned(), ..Default::default() };
        let Ok(Opened::Installed(path, info, said)) = run_in(&folder, open(given, false), &Known::default(), false)
        else {
            panic!()
        };
        assert_eq!(path, folder.join("tb.spbible"));
        assert_eq!(
            (info.name.as_str(), info.short.as_str(), info.language.as_str(), info.terms.as_str()),
            ("Тестова Біблія", "ТБ", "uk", "Написано для тестів.")
        );
        assert!(said.starts_with("Installed Тестова Біблія (1 book, 1 verse)"), "{said}");
        let bible = Bible::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert!(bible.info.source.ends_with("Тестова Біблія.spb (SoftProjector Bible)"), "{}", bible.info.source);
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn a_dropped_bible_file_never_stands_in_for_a_built_in_or_offered_one() {
        let folder = empty_folder("own");
        let shelf = Known {
            bibles: vec![
                KnownBible {
                    id: "kjv".to_owned(),
                    name: "King James Version".to_owned(),
                    short: "KJV".to_owned(),
                    shipped: true,
                },
                KnownBible {
                    id: "mine".to_owned(),
                    name: "Test Bible".to_owned(),
                    short: "TB".to_owned(),
                    shipped: false,
                },
            ],
        };
        let install = |id: &str| {
            let file = folder.with_extension(format!("{id}.spbible"));
            std::fs::write(&file, small_bible(id)).unwrap();
            let done =
                run_in(&folder, Work::Open { path: file.clone(), given: Default::default(), ask: true }, &shelf, false);
            let _ = std::fs::remove_file(&file);
            match done {
                Ok(Opened::Installed(path, info, said)) => (path, info.id, said),
                other => panic!("{other:?}"),
            }
        };
        // One that says it is the ASV (offered, checked by its checksum when downloaded) or the
        // King James (built in) is kept under a name of its own.
        let (path, id, said) = install("ASV");
        assert_eq!((path, id.as_str()), (folder.join("asv-2.spbible"), "asv-2"));
        assert_eq!(said, "Installed the Test Bible.");
        assert_eq!(install("kjv").1, "kjv-2");
        // One with a device's name is kept under another.
        assert_eq!(install("con").1, "con-bible");
        // One installed from a file before is replaced, and said so.
        let (path, id, said) = install("MINE");
        assert_eq!((path, id.as_str()), (folder.join("mine.spbible"), "mine"));
        assert_eq!(said, "Replaced your earlier Test Bible.");
        // A new one keeps its own name.
        assert_eq!(install("fresh").1, "fresh");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_named_like_a_bible_file_is_never_opened() {
        let folder = empty_folder("pipe");
        std::fs::create_dir_all(&folder).unwrap();
        let pipe = folder.join("KJV.spbible");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().unwrap().success());
        let (tx, rx) = std::sync::mpsc::channel();
        let (at, path) = (folder.join("Bibles"), pipe.clone());
        std::thread::spawn(move || {
            let work = Work::Open { path, given: Default::default(), ask: true };
            let _ = tx.send(run_in(&at, work, &Known::default(), false).map(|_| ()));
        });
        let done = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("reading a pipe waited");
        assert!(done.unwrap_err().contains("isn't a Bible file"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn bible_files_are_told_by_what_they_hold_and_only_likely_ones_are_read() {
        let folder = empty_folder("sniff");
        std::fs::create_dir_all(&folder).unwrap();
        let usfm = folder.join("GEN.txt");
        std::fs::write(&usfm, "\\id GEN\n\\c 1\n\\v 1 Words for a test.\n").unwrap();
        assert!(is_bible_file(&usfm), "a USFM book ending in .txt");
        let song = folder.join("Song.txt");
        std::fs::write(&song, "Words of a song\n\nAnother slide").unwrap();
        assert!(!is_bible_file(&song));
        // An ending no Bible has is never opened, whatever it holds.
        let movie = folder.join("film.mp4");
        std::fs::write(&movie, "\\id GEN\n\\c 1\n\\v 1 Words.\n").unwrap();
        assert!(!is_bible_file(&movie));
        assert!(is_bible_file(Path::new("missing/KJV.spbible")));
        // A SoftProjector Bible, of the kind read or the older one (which is told why not).
        let spb = folder.join("Тестова.spb");
        std::fs::write(&spb, "1\t1\t1\tWords.\n").unwrap();
        assert!(is_bible_file(&spb));
        // A Bible module ending in .xmm is looked into too.
        let module = folder.join("KJV.xmm");
        std::fs::write(&module, r#"<bible><b n="John"><c n="3"><v n="16">Words.</v></c></b></bible>"#).unwrap();
        assert!(is_bible_file(&module));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_bible_is_installed_under_its_own_name_and_rubbish_is_refused() {
        let folder = empty_folder("install");
        let (path, info) = install_bytes_in(&folder, &small_bible("tb"), false).unwrap();
        assert_eq!(path, folder.join("tb.spbible"));
        assert_eq!(info.name, "Test Bible");
        assert!(Bible::from_bytes(&std::fs::read(&path).unwrap()).is_ok());
        assert!(install_bytes_in(&folder, b"not a bible", false).is_err());
        // A file that names itself with a path still lands inside the folder.
        let (path, _) = install_bytes_in(&folder, &small_bible("../escape"), false).unwrap();
        assert_eq!(path, folder.join("escape.spbible"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_download_is_installed_only_when_it_is_the_expected_file() {
        let folder = empty_folder("download");
        let bytes = small_bible("tb");
        let sum = checksum(&bytes);

        let (path, info) = download_into(&folder, &serve_once(bytes.clone()), "tb", &sum).unwrap();
        assert_eq!((path, info.id.as_str()), (folder.join("tb.spbible"), "tb"));
        let _ = std::fs::remove_dir_all(&folder);

        // The wrong checksum: nothing is written.
        let wrong = download_into(&folder, &serve_once(bytes.clone()), "tb", &"0".repeat(64)).unwrap_err();
        assert!(wrong.contains("not the file this version expects"), "{wrong}");
        assert!(!folder.exists());

        // The right checksum for a different Bible than asked for: nothing is written.
        let other = download_into(&folder, &serve_once(bytes), "kjv", &sum).unwrap_err();
        assert!(other.contains("different Bible"), "{other}");
        assert!(!folder.exists());

        // Nobody answering: a plain sentence, not a crash.
        let nobody = download_into(&folder, "http://127.0.0.1:9/none.pcbible", "tb", &sum).unwrap_err();
        assert!(nobody.contains("Could not download"), "{nobody}");
    }
}
