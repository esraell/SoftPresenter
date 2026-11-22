//! Where installed Bibles are kept on this computer, and fetching new ones.
//!
//! A downloaded file is only kept if its checksum is the one this version of the app expects
//! and it reads as a Bible, so a damaged or swapped file is never installed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use sha2::{Digest, Sha256};
use sp_bible::{Bible, Info};

/// Where the Bible files are published: the address their names are added to. They are
/// files attached to a release of the project on GitHub. While the project is private this
/// address answers nobody, and Install says the download could not be made.
const DOWNLOADS: Option<&str> = Some("https://github.com/esraell/SoftPresenter/releases/download/bibles-3");

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
/// itself can never choose where it is written.
fn file_name(id: &str) -> Option<String> {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    (!safe.is_empty() && safe.len() <= 40).then(|| format!("{safe}.spbible"))
}

/// Checks that the bytes are a whole Bible and keeps them in the folder. Returns where, and
/// what the Bible says about itself. The error is a plain sentence.
pub fn install_bytes(bytes: &[u8]) -> Result<(PathBuf, Info), String> {
    let folder = folder().ok_or_else(|| "Could not find where to keep Bibles on this computer.".to_owned())?;
    install_bytes_in(&folder, bytes)
}

fn install_bytes_in(folder: &Path, bytes: &[u8]) -> Result<(PathBuf, Info), String> {
    let bible = Bible::from_bytes(bytes)?;
    let name = file_name(&bible.info.id).ok_or_else(|| "This Bible file does not say which Bible it is.".to_owned())?;
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make the Bibles folder: {e}"))?;
    let path = folder.join(name);
    // Write beside it and rename, so a half-written file is never left under the real name.
    let partial = path.with_extension("spbible.part");
    std::fs::write(&partial, bytes)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the Bible: {e}"))?;
    // The same Bible kept before the rename is replaced, not left beside it.
    let _ = std::fs::remove_file(path.with_extension("pcbible"));
    Ok((path, bible.info))
}

/// Installs a Bible file the operator has, such as one dropped on the window.
pub fn install_file(path: &Path) -> Result<(PathBuf, Info), String> {
    let size = std::fs::metadata(path).map_err(|e| format!("Could not read that file: {e}"))?.len();
    if size > LARGEST {
        return Err("That file is too large to be a Bible.".to_owned());
    }
    install_bytes(&std::fs::read(path).map_err(|e| format!("Could not read that file: {e}"))?)
}

/// True for a SoftPresenter Bible file.
fn is_own_bible_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("spbible") || ext.eq_ignore_ascii_case("pcbible"))
}

/// True for a Bible the app can install: its own file, or a Bible in another program's format
/// (a file, a zip or a folder of books), told by what it holds. Only files whose ending a Bible
/// may have (or none) are looked into, and only their first few kilobytes.
pub fn is_bible_file(path: &Path) -> bool {
    if is_own_bible_file(path) {
        return true;
    }
    let ending = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    match ending.as_deref() {
        None | Some("xml" | "xmm" | "zip" | "usfm" | "sfm" | "usx" | "osis" | "zef" | "txt" | "bible") => {
            sp_bible::import::looks_like_bible(path)
        }
        _ => false,
    }
}

/// How opening a Bible file the operator has went.
#[derive(Debug)]
pub enum Opened {
    /// It was installed: where it is kept, what it says about itself, and what to tell the
    /// operator ("Installed … (66 books, 31,102 verses) …").
    Installed(PathBuf, Info, String),
    /// It was read, but doesn't say its name, short name or language: they are asked for, and
    /// then [`install_imported`] installs it.
    NeedsDetails(sp_bible::import::Imported),
}

/// Opens a Bible file the operator has (dropped on the window, chosen with Install from File…,
/// or named to `sp bible install`): the app's own file is installed as it is; one in another
/// program's format is read, made into the app's own and installed, with the details `given`
/// in place of the file's. What is installed goes on the shelf with
/// [`crate::bible::Shelf::put_installed`].
pub fn open_file(path: &Path, given: &sp_bible::import::About, shelf: &crate::bible::Shelf) -> Result<Opened, String> {
    if is_own_bible_file(path) {
        let (kept, info) = install_file(path)?;
        let said = format!("Installed the {}.", info.name);
        return Ok(Opened::Installed(kept, info, said));
    }
    let imported = sp_bible::import::read_path(path, &sp_bible::import::Limits::NORMAL)?;
    match imported.details(given) {
        Some(details) => {
            let (kept, info, said) = install_imported(&imported, &details, shelf)?;
            Ok(Opened::Installed(kept, info, said))
        }
        None => Ok(Opened::NeedsDetails(imported)),
    }
}

/// Installs a Bible read from another program's file, under the details given. The same Bible
/// installed from a file before (its name and short name alike) is replaced; otherwise it gets
/// a short name for its file no Bible installed or offered has (`rst`, else `rst-2`…), so it
/// never stands in for one of them. Returns where, what it says about itself, and what to tell
/// the operator.
pub fn install_imported(
    imported: &sp_bible::import::Imported,
    details: &sp_bible::import::About,
    shelf: &crate::bible::Shelf,
) -> Result<(PathBuf, Info, String), String> {
    let folder = folder().ok_or_else(|| "Could not find where to keep Bibles on this computer.".to_owned())?;
    let offered = |id: &str| crate::bible::OFFERED.iter().any(|bible| bible.id == id);
    let again = shelf.installed_as(&details.name, &details.short).filter(|id| !offered(id));
    install_imported_in(&folder, imported, details, again, &|id| shelf.position(id).is_some() || offered(id))
}

fn install_imported_in(
    folder: &Path,
    imported: &sp_bible::import::Imported,
    details: &sp_bible::import::About,
    again: Option<&str>,
    taken: &dyn Fn(&str) -> bool,
) -> Result<(PathBuf, Info, String), String> {
    let base = sp_bible::import::id_from(&details.short, &details.name);
    let id = match again {
        Some(id) => id.to_owned(),
        None => std::iter::once(base.clone())
            .chain((2..1000).map(|n| format!("{base}-{n}")))
            .find(|id| !taken(id))
            .ok_or_else(|| "Too many Bibles are installed under that name.".to_owned())?,
    };
    let bible = imported.bible(&id, details);
    let (kept, info) = install_bytes_in(folder, &bible.to_bytes())?;
    let said = imported.summary(&info.name);
    Ok((kept, info, said))
}

/// Downloads a Bible's file, trying once more if the first try fails or comes back short.
fn fetch(address: &str) -> Result<Vec<u8>, String> {
    fetch_once(address).or_else(|_| fetch_once(address))
}

/// One try: it gives up after two minutes, and a file shorter than the server said it was is
/// "cut short", never mistaken for the wrong file.
fn fetch_once(address: &str) -> Result<Vec<u8>, String> {
    let offline = |e: ureq::Error| format!("Could not download the Bible: {e}");
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
        return Err("The Bible's download was cut short. Try again.".to_owned());
    }
    Ok(bytes)
}

/// Fetches one Bible and installs it, if it is the file expected.
fn download_into(folder: &Path, address: &str, id: &str, expected_checksum: &str) -> Result<(PathBuf, Info), String> {
    let bytes = fetch(address)?;
    if checksum(&bytes) != expected_checksum {
        return Err("The downloaded Bible is not the file this version expects, so it was not installed.".to_owned());
    }
    // Checked before anything is written, so a wrong file leaves nothing behind.
    if Bible::read_info(&bytes)?.id != id {
        return Err("The downloaded file is a different Bible, so it was not installed.".to_owned());
    }
    install_bytes_in(folder, &bytes)
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
                .ok_or_else(|| "Could not find where to keep Bibles on this computer.".to_owned())
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
        let imported =
            sp_bible::import::read_bytes(zefania.as_bytes(), "test.xml", &sp_bible::import::Limits::NORMAL).unwrap();
        let details = imported.details(&sp_bible::import::About::default()).unwrap();
        // "npu" is taken (as an installed or offered Bible's would be): the next is used.
        let (path, info, said) = install_imported_in(&folder, &imported, &details, None, &|id| id == "npu").unwrap();
        assert_eq!(path, folder.join("npu-2.spbible"));
        assert_eq!((info.id.as_str(), info.short.as_str(), info.language.as_str()), ("npu-2", "НПУ", "uk"));
        assert!(said.starts_with("Installed Тестова Біблія (1 book, 1 verse)"), "{said}");
        let bible = Bible::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(bible.verse_count(), 1);
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
        // A Bible module ending in .xmm is looked into too.
        let module = folder.join("KJV.xmm");
        std::fs::write(&module, r#"<bible><b n="John"><c n="3"><v n="16">Words.</v></c></b></bible>"#).unwrap();
        assert!(is_bible_file(&module));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_bible_is_installed_under_its_own_name_and_rubbish_is_refused() {
        let folder = empty_folder("install");
        let (path, info) = install_bytes_in(&folder, &small_bible("tb")).unwrap();
        assert_eq!(path, folder.join("tb.spbible"));
        assert_eq!(info.name, "Test Bible");
        assert!(Bible::from_bytes(&std::fs::read(&path).unwrap()).is_ok());
        assert!(install_bytes_in(&folder, b"not a bible").is_err());
        // A file that names itself with a path still lands inside the folder.
        let (path, _) = install_bytes_in(&folder, &small_bible("../escape")).unwrap();
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
