//! Getting the text out of what was given: one file, a zip, or a folder of book files, never
//! reading more than the limits allow. A zip entry's name is only looked at (for its ending
//! and to say which file is which), never used as a place on disk, and a zip inside a zip is
//! not opened.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use sp_model::bounded_zip::{ZipRefusal, is_zip};

use super::{Document, kind_of_text};

/// How much may be read for one Bible.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The largest file read (a Bible's XML, or the zip it comes in).
    pub file: u64,
    /// The largest one file in a zip or folder, unpacked.
    pub entry: u64,
    /// Everything in a zip or folder together, unpacked.
    pub unpacked: u64,
    /// The most entries a zip may list, or files a folder may hold.
    pub entries: usize,
}

impl Limits {
    /// Room for the largest Bibles there are: a whole Bible in XML with Strong's numbers is
    /// about 40 MB; eBible's King James in USFM is 70 files, 12.5 MB unpacked.
    pub const NORMAL: Limits =
        Limits { file: 64 * 1024 * 1024, entry: 64 * 1024 * 1024, unpacked: 128 * 1024 * 1024, entries: 2_000 };
}

/// Whether a file in a zip or folder is read: any file whatever its ending (what it holds
/// decides), but for hidden ones and those that are surely something else (pictures, sound,
/// video, fonts, style sheets, scripts, signatures, documents, web pages but eBible's
/// copyright page). The limits bound what is read all the same.
fn worth_reading(name: &str) -> bool {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
    if file.is_empty() || file.starts_with('.') || name.contains("__MACOSX") {
        return false;
    }
    if file == "copr.htm" {
        return true;
    }
    let Some((_, ending)) = file.rsplit_once('.') else { return true };
    !matches!(
        ending,
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "svg"
            | "ico"
            | "bmp"
            | "tif"
            | "tiff"
            | "mp3"
            | "m4a"
            | "wav"
            | "ogg"
            | "mp4"
            | "mov"
            | "m4v"
            | "webm"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "css"
            | "js"
            | "asc"
            | "sig"
            | "pdf"
            | "epub"
            | "doc"
            | "docx"
            | "htm"
            | "html"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "db"
            | "sqlite"
    )
}

fn too_large(name: &str) -> String {
    format!("{name} is larger than any Bible, so it wasn't read.")
}

fn too_many(name: &str) -> String {
    format!("{name} has too many files in it to be a Bible.")
}

fn unpacks_too_much(name: &str) -> String {
    format!("{name} unpacks to more than any Bible, so it wasn't read.")
}

/// The documents at a path: the file's, a zip's, or a folder's. Anything that is neither a
/// file nor a folder (a pipe, a device) is refused without being opened, as opening one could
/// wait for ever.
pub(super) fn documents_at(path: &Path, name: &str, limits: &Limits) -> Result<Vec<Document>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Could not read {name}: {e}"))?;
    if meta.is_dir() {
        return documents_in_folder(path, name, limits);
    }
    if !meta.is_file() {
        return Err(format!("{name} isn't a file or folder SoftPresenter can read."));
    }
    if meta.len() > limits.file {
        return Err(too_large(name));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limits.file + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Could not read {name}: {e}"))?;
    if bytes.len() as u64 > limits.file {
        return Err(too_large(name));
    }
    documents_in(&bytes, name, limits)
}

/// Text decoded from what was read, counted against what may be unpacked in all: decoding can
/// make text longer than its bytes (one byte of an old encoding can be three of UTF-8).
fn decoded(bytes: &[u8], unpacked: &mut u64, limits: &Limits, name: &str) -> Result<String, String> {
    let text = sp_model::decode_text(bytes);
    *unpacked += (text.len() as u64).max(bytes.len() as u64);
    if *unpacked > limits.unpacked {
        return Err(unpacks_too_much(name));
    }
    Ok(text)
}

/// The documents in a file's bytes: itself, or what a zip holds.
pub(super) fn documents_in(bytes: &[u8], name: &str, limits: &Limits) -> Result<Vec<Document>, String> {
    if bytes.len() as u64 > limits.file {
        return Err(too_large(name));
    }
    let mut unpacked = 0_u64;
    if !is_zip(bytes) {
        let text = decoded(bytes, &mut unpacked, limits, name)?;
        return Ok(vec![Document { name: name.to_owned(), text }]);
    }
    let zip_limits = sp_model::bounded_zip::ZipLimits {
        file: limits.file,
        entry: limits.entry,
        unpacked: limits.unpacked,
        entries: limits.entries,
    };
    let mut documents = Vec::new();
    let read =
        sp_model::bounded_zip::for_each_entry(bytes, &zip_limits, worth_reading, |entry_name, read, unpacked| {
            // Decoding can make text longer than its bytes, and that counts too.
            let text = sp_model::decode_text(&read);
            *unpacked += (text.len() as u64).max(read.len() as u64);
            if *unpacked > limits.unpacked {
                return Err(ZipRefusal::UnpacksTooMuch);
            }
            documents.push(Document { name: entry_name, text });
            Ok(())
        });
    read.map_err(|refusal| match refusal {
        ZipRefusal::Damaged => format!("{name} is a damaged zip file."),
        ZipRefusal::Overlap => format!("{name} is a damaged zip file: its files overlap."),
        ZipRefusal::TooMany => too_many(name),
        ZipRefusal::UnpacksTooMuch => unpacks_too_much(name),
    })?;
    Ok(documents)
}

/// The documents in a folder and the folders in it, three deep (a Bible bundle keeps its books
/// a folder or two down), taken in name order. Links are not followed, and no more than
/// [`Limits::entries`] entries are looked at in all, files or not.
fn documents_in_folder(folder: &Path, name: &str, limits: &Limits) -> Result<Vec<Document>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut folders: Vec<(PathBuf, usize)> = vec![(folder.to_owned(), 0)];
    let mut looked = 0_usize;
    while let Some((here, depth)) = folders.pop() {
        let Ok(read) = std::fs::read_dir(&here) else { continue };
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in read.flatten() {
            looked += 1;
            if looked > limits.entries {
                return Err(too_many(name));
            }
            entries.push(entry.path());
        }
        entries.sort();
        for path in entries {
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if file_name.starts_with('.') {
                continue;
            }
            if meta.is_dir() && depth < 3 {
                folders.push((path, depth + 1));
            } else if meta.is_file() && worth_reading(&file_name) {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut documents = Vec::new();
    let mut unpacked = 0_u64;
    for path in files {
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let room = limits.entry.min(limits.unpacked.saturating_sub(unpacked));
        let mut bytes = Vec::new();
        if std::fs::File::open(&path).and_then(|file| file.take(room + 1).read_to_end(&mut bytes)).is_err() {
            continue;
        }
        if bytes.len() as u64 > room {
            return Err(format!("{name} holds more than any Bible, so it wasn't read."));
        }
        if is_zip(&bytes) {
            unpacked += bytes.len() as u64;
            continue;
        }
        let text = decoded(&bytes, &mut unpacked, limits, name)?;
        documents.push(Document { name: file_name, text });
    }
    Ok(documents)
}

/// How much of a file is read to tell what it is.
const HEAD: u64 = 4096;

/// The first part of a file.
fn head_bytes(reader: impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(HEAD).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The first part of a file, as text.
fn head_of(reader: impl Read) -> Option<String> {
    head_bytes(reader).map(|bytes| sp_model::decode_text(&bytes))
}

/// Whether the text starting a file is one of the formats read (not what is said about one).
fn head_is_bible(head: &str) -> bool {
    kind_of_text(head).is_some()
}

/// Ends that name USFM or USX books.
fn book_ending(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".usfm", ".sfm", ".usx"].iter().any(|ending| lower.ends_with(ending))
}

/// See [`super::looks_like_bible`]. Only a plain file or a folder is opened (never a pipe or a
/// device, which could keep it waiting), and a zip only when it is small enough to be read.
pub(super) fn looks_like_bible(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if meta.is_dir() {
        // Books by their endings, three folders deep as a bundle keeps them (four hundred
        // entries looked at in all); any other file only at the top, by its start.
        let mut folders = vec![(path.to_owned(), 0)];
        let mut looked = 0;
        while let Some((here, depth)) = folders.pop() {
            let Ok(read) = std::fs::read_dir(&here) else { continue };
            for entry in read.flatten() {
                looked += 1;
                if looked > 400 {
                    return false;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(kind) = entry.file_type() else { continue };
                if name.starts_with('.') {
                    continue;
                }
                if kind.is_dir() && depth < 3 {
                    folders.push((entry.path(), depth + 1));
                } else if kind.is_file()
                    && (book_ending(&name)
                        || (depth == 0
                            && worth_reading(&name)
                            && std::fs::File::open(entry.path())
                                .ok()
                                .and_then(head_of)
                                .is_some_and(|head| head_is_bible(&head))))
                {
                    return true;
                }
            }
        }
        return false;
    }
    if !meta.is_file() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let Some(head) = head_bytes(&mut file) else { return false };
    if !is_zip(&head) {
        return head_is_bible(&sp_model::decode_text(&head));
    }
    if meta.len() > Limits::NORMAL.file || file.rewind().is_err() {
        return false;
    }
    let Ok(mut zip) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return false };
    if zip.len() > Limits::NORMAL.entries {
        return false;
    }
    let names: Vec<String> = (0..zip.len())
        .filter_map(|at| zip.by_index_raw(at).ok().map(|entry| String::from_utf8_lossy(entry.name_raw()).into_owned()))
        .collect();
    if names.iter().any(|name| book_ending(name) && worth_reading(name)) {
        return true;
    }
    // Otherwise the start of the first few that could be a Bible.
    names
        .iter()
        .enumerate()
        .filter(|(_, name)| worth_reading(name))
        .take(8)
        .any(|(at, _)| zip.by_index(at).ok().and_then(head_of).is_some_and(|head| head_is_bible(&head)))
}
