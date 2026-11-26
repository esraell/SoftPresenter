//! What the log says of a file an import refused, without its contents: its size and what its
//! first bytes look like, a guess made from a list of known signatures ("SQLite", "ZIP", "UTF-16
//! text", "UTF-8 text starting \"##spDataVersion: 1\""), so a refusal can be understood from the
//! log alone.
//!
//! A log is shared when a problem is looked into, and the words of songs and Bibles must never be
//! in it. So nothing of a file's own text is ever said beyond a marker word of a known
//! format from the list below (with at most a version number after it): text that starts with
//! no known marker is said to be text, and nothing more.

use std::io::Read;
use std::path::Path;

/// How much of a file is looked at.
const HEAD: usize = 512;

/// A file, as the log says it: "4.4 MB, UTF-8 text starting \"##spDataVersion: 1\"", "a
/// folder". `None` when nothing is there to look at.
pub fn of(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        return Some("a folder".to_owned());
    }
    // A pipe or a device is never opened: opening one can wait for ever.
    if !meta.is_file() {
        return Some("not a file".to_owned());
    }
    if meta.len() == 0 {
        return Some("empty".to_owned());
    }
    let mut head = Vec::with_capacity(HEAD);
    let read = std::fs::File::open(path).and_then(|file| file.take(HEAD as u64).read_to_end(&mut head));
    let looks = if read.is_ok() { guess(&head) } else { "couldn't be read".to_owned() };
    Some(format!("{}, {looks}", size_words(meta.len())))
}

/// What the log says of a refusal: the sentence the operator was given, in English, with what
/// the file looks like after it in brackets (in place of its full stop). The sentence alone
/// when there is nothing to add.
pub fn noted(said: &str, kind: Option<&str>) -> String {
    let Some(kind) = kind else { return said.to_owned() };
    // Put back into English first: the log does that by whole sentences, which a sentence with
    // something added to it no longer is.
    let english = crate::i18n::english_of(said);
    let sentence = english.trim_end();
    let sentence = sentence.strip_suffix('.').unwrap_or(sentence);
    format!("{sentence} ({kind})")
}

/// A size as the log says it: "512 bytes", "12 KB", "4.4 MB".
fn size_words(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let size = bytes as f64;
    match bytes {
        1 => "1 byte".to_owned(),
        0..1024 => format!("{bytes} bytes"),
        _ if size < KB * KB => format!("{:.0} KB", size / KB),
        _ if size < KB * KB * KB => format!("{:.1} MB", size / (KB * KB)),
        _ => format!("{:.1} GB", size / (KB * KB * KB)),
    }
}

/// Formats known by their first bytes, and what the log calls them.
const SIGNATURES: &[(&[u8], &str)] = &[
    (b"SQLite format 3\0", "SQLite"),
    (b"PK\x03\x04", "ZIP"),
    (b"PK\x05\x06", "ZIP"),
    (b"PK\x07\x08", "ZIP"),
    (b"\x1f\x8b", "gzip"),
    (b"BZh", "bzip2"),
    (b"\xfd7zXZ\0", "xz"),
    (b"\x28\xb5\x2f\xfd", "zstd"),
    (b"7z\xbc\xaf\x27\x1c", "7-Zip"),
    (b"Rar!\x1a\x07", "RAR"),
    (b"%PDF-", "PDF"),
    (b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", "an old Office file (OLE)"),
    (b"{\\rtf", "RTF"),
    (b"\x89PNG\r\n\x1a\n", "PNG"),
    (b"\xff\xd8\xff", "JPEG"),
    (b"GIF87a", "GIF"),
    (b"GIF89a", "GIF"),
    (b"II*\0", "TIFF"),
    (b"MM\0*", "TIFF"),
    (b"8BPS", "Photoshop"),
    (b"ID3", "MP3"),
    (b"fLaC", "FLAC"),
    (b"OggS", "Ogg"),
    (b"\x1a\x45\xdf\xa3", "Matroska or WebM"),
    (b"bplist00", "binary property list"),
];

/// What the first bytes of a file look like.
fn guess(head: &[u8]) -> String {
    if head.is_empty() {
        return "empty".to_owned();
    }
    if let Some(name) = signature(head) {
        return name.to_owned();
    }
    text(head).unwrap_or_else(|| "binary".to_owned())
}

/// The format a file's first bytes name, if they are one of the known signatures.
fn signature(head: &[u8]) -> Option<&'static str> {
    if let Some(name) = SIGNATURES.iter().find(|(start, _)| head.starts_with(start)).map(|(_, name)| *name) {
        return Some(name);
    }
    let at = |from: usize| head.get(from..from + 4);
    if at(0) == Some(b"RIFF".as_slice()) {
        return Some(match at(8) {
            Some(b"WEBP") => "WebP",
            Some(b"WAVE") => "WAV",
            Some(b"AVI ") => "AVI",
            _ => "RIFF",
        });
    }
    // An ISO media file (MP4, QuickTime, HEIF…): its type box first, with its brand.
    if at(4) == Some(b"ftyp".as_slice()) {
        return Some(match at(8) {
            Some(b"qt  ") => "QuickTime",
            Some(b"heic" | b"heix" | b"hevc" | b"mif1" | b"msf1") => "HEIF",
            Some(b"avif") => "AVIF",
            Some(b"M4A ") => "M4A",
            _ => "MP4",
        });
    }
    None
}

/// What a file of text looks like: its encoding, and the known format or marker it starts with.
/// `None` when it isn't text.
fn text(head: &[u8]) -> Option<String> {
    let (encoding, decoded): (&str, String) = if let Some(units) = utf16(head) {
        ("UTF-16", char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect())
    } else if head.contains(&0) {
        return None;
    } else {
        let body = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
        match std::str::from_utf8(body) {
            Ok(text) => ("UTF-8", text.to_owned()),
            // Cut short in the middle of a letter, where the bytes looked at end.
            Err(cut) if cut.error_len().is_none() => {
                ("UTF-8", String::from_utf8_lossy(&body[..cut.valid_up_to()]).into_owned())
            }
            // Text in an older encoding (a Windows code page, say): almost every byte a letter, a
            // sign or a line break.
            Err(_)
                if body.iter().filter(|&&b| b >= 0x20 || matches!(b, b'\t' | b'\n' | b'\r')).count() * 50
                    >= body.len() * 49 =>
            {
                ("non-UTF-8", String::from_utf8_lossy(body).into_owned())
            }
            Err(_) => return None,
        }
    };
    if !mostly_text(&decoded) {
        return None;
    }
    Some(match marker(&decoded) {
        Some(Marker::Format(name)) if encoding == "UTF-8" => name.to_owned(),
        Some(Marker::Format(name)) => format!("{name} in {encoding}"),
        Some(Marker::Starting(marker)) => format!("{encoding} text starting \"{marker}\""),
        None => format!("{encoding} text"),
    })
}

/// The bytes as UTF-16, if they are: they start with its byte-order mark, or every other byte
/// is nought (as in text in a Latin script).
fn utf16(head: &[u8]) -> Option<Vec<u16>> {
    let (body, little) = if let Some(body) = head.strip_prefix(b"\xff\xfe") {
        (body, true)
    } else if let Some(body) = head.strip_prefix(b"\xfe\xff") {
        (body, false)
    } else {
        let pairs = head.as_chunks::<2>().0;
        let nought_after = pairs.iter().filter(|[first, second]| *first != 0 && *second == 0).count();
        let nought_before = pairs.iter().filter(|[first, second]| *first == 0 && *second != 0).count();
        if pairs.len() < 4 {
            return None;
        } else if nought_after * 10 >= pairs.len() * 8 {
            (head, true)
        } else if nought_before * 10 >= pairs.len() * 8 {
            (head, false)
        } else {
            return None;
        }
    };
    let unit = |pair: &[u8; 2]| if little { u16::from_le_bytes(*pair) } else { u16::from_be_bytes(*pair) };
    Some(body.as_chunks::<2>().0.iter().map(unit).collect())
}

/// Whether decoded bytes read as text: almost no control characters besides line breaks, tabs
/// and page breaks.
fn mostly_text(text: &str) -> bool {
    let total = text.chars().count().max(1);
    let control = text.chars().filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}')).count();
    control * 50 <= total
}

/// What a text starts with that the log may say.
#[derive(Debug, PartialEq)]
enum Marker {
    /// A format of its own ("XML", "JSON").
    Format(&'static str),
    /// A format's marker, said as it is (with its version, if it gives one).
    Starting(String),
}

/// The known format or marker a text starts with. Only the marker itself is ever given back,
/// never what follows it.
fn marker(text: &str) -> Option<Marker> {
    let start = text.trim_start_matches('\u{feff}').trim_start();
    let starts = |prefix: &str| start.get(..prefix.len()).is_some_and(|begins| begins.eq_ignore_ascii_case(prefix));
    if starts("<!doctype html") || starts("<html") {
        return Some(Marker::Format("HTML"));
    }
    if start.starts_with("<?xml") || start.strip_prefix('<').is_some_and(|rest| rest.starts_with(char::is_alphabetic)) {
        return Some(Marker::Format("XML"));
    }
    if start.starts_with("{\\rtf") {
        return Some(Marker::Format("RTF"));
    }
    if start.strip_prefix('{').is_some_and(|rest| rest.trim_start().starts_with('"')) {
        return Some(Marker::Format("JSON"));
    }
    // SoftProjector's Bibles and songs: "##spDataVersion:" and the version of its layout.
    if let Some(rest) = start.strip_prefix("##spDataVersion") {
        let rest = rest.trim_start_matches([':', ' ', '\t']);
        let version: String = rest.chars().take_while(char::is_ascii_digit).take(4).collect();
        return Some(Marker::Starting(if version.is_empty() {
            "##spDataVersion".to_owned()
        } else {
            format!("##spDataVersion: {version}")
        }));
    }
    // A USFM book.
    if start.strip_prefix("\\id").is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return Some(Marker::Starting("\\id".to_owned()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_formats_are_named_by_their_first_bytes() {
        assert_eq!(guess(b"SQLite format 3\0\x10\0\x01\x01"), "SQLite");
        assert_eq!(guess(b"PK\x03\x04\x14\0\0\0\x08\0"), "ZIP");
        assert_eq!(guess(b"\x1f\x8b\x08\0\0\0\0\0"), "gzip");
        assert_eq!(guess(b"%PDF-1.7\n%\xe2\xe3"), "PDF");
        assert_eq!(guess(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), "PNG");
        assert_eq!(guess(b"\0\0\0\x20ftypqt  \0\0\x02\0"), "QuickTime");
        assert_eq!(guess(b"\0\0\0\x18ftypisom\0\0\x02\0"), "MP4");
        assert_eq!(guess(b"RIFF\x24\0\0\0WAVEfmt "), "WAV");
        assert_eq!(guess(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<osis>"), "XML");
        assert_eq!(guess(b"\xef\xbb\xbf<XMLBIBLE biblename=\"A Bible\">"), "XML");
        assert_eq!(guess(b"<!DOCTYPE html>\n<html>"), "HTML");
        assert_eq!(guess(b"{\n  \"title\": \"A song\"\n}"), "JSON");
        assert_eq!(guess(b"\0\x01\x02\x03\xff\xfe\x00\x10binary\0\0"), "binary");
        assert_eq!(guess(b""), "empty");
    }

    #[test]
    fn text_says_its_encoding_and_a_known_marker_but_never_its_words() {
        // Words written for this test.
        let words = "Plinth morrow quaver, the lantern hums";
        let said = guess(format!("{words}\nand the second line\n").as_bytes());
        assert_eq!(said, "UTF-8 text");
        // A SoftProjector Bible: its marker and version, nothing of what follows.
        let spb = format!("##spDataVersion:\t1\n##Title:\t{words}\n##Abbreviation:\tPMQ\n");
        let said = guess(spb.as_bytes());
        assert_eq!(said, "UTF-8 text starting \"##spDataVersion: 1\"");
        assert!(!said.contains("Title") && !said.contains("Plinth"), "{said}");
        assert_eq!(guess(b"##spDataVersion: 12345\n"), "UTF-8 text starting \"##spDataVersion: 1234\"");
        assert_eq!(guess(format!("##spDataVersion {words}").as_bytes()), "UTF-8 text starting \"##spDataVersion\"");
        assert_eq!(guess(format!("\\id GEN {words}\n\\c 1").as_bytes()), "UTF-8 text starting \"\\id\"");
        // UTF-16, with its byte-order mark and without.
        let utf16 = |text: &str, mark: bool| {
            let mut bytes = if mark { vec![0xff, 0xfe] } else { Vec::new() };
            bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            bytes
        };
        assert_eq!(guess(&utf16(words, true)), "UTF-16 text");
        assert_eq!(guess(&utf16(words, false)), "UTF-16 text");
        assert_eq!(guess(&utf16("<?xml version=\"1.0\"?><bible>", true)), "XML in UTF-16");
        let big: Vec<u8> =
            std::iter::once([0xfe, 0xff]).chain(words.encode_utf16().map(u16::to_be_bytes)).flatten().collect();
        assert_eq!(guess(&big), "UTF-16 text");
        // An older encoding: Cyrillic in Windows-1251 (made-up letters, any will do).
        assert_eq!(guess(b"\xcf\xf0\xe8\xe2\xe5\xf2 \xec\xe8\xf0\n\xe4\xe0\xeb\xe5\xe5"), "non-UTF-8 text");
        // Cut short in the middle of a letter: still UTF-8.
        let mut cut = format!("a{}", "Ñ".repeat(300)).into_bytes();
        cut.truncate(HEAD);
        assert!(std::str::from_utf8(&cut).is_err(), "the last letter is cut");
        assert_eq!(guess(&cut), "UTF-8 text");
    }

    #[test]
    fn a_file_is_said_with_its_size_and_a_refusal_with_it_in_brackets() {
        let dir = std::env::temp_dir().join(format!("softpresenter-file-kind-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Made up.spb");
        let mut bytes = b"##spDataVersion: 1\n".to_vec();
        bytes.resize(4_613_734, b'x');
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(of(&file).as_deref(), Some("4.4 MB, UTF-8 text starting \"##spDataVersion: 1\""));
        let empty = dir.join("empty.txt");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(of(&empty).as_deref(), Some("empty"));
        assert_eq!(of(&dir).as_deref(), Some("a folder"));
        assert_eq!(of(&dir.join("not there")), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            noted("Made up.spb isn't a Bible file SoftPresenter can read.", Some("4.4 MB, UTF-8 text")),
            "Made up.spb isn't a Bible file SoftPresenter can read (4.4 MB, UTF-8 text)"
        );
        assert_eq!(noted("Made up.spb has no words in it.", None), "Made up.spb has no words in it.");
        assert_eq!(size_words(1), "1 byte");
        assert_eq!(size_words(512), "512 bytes");
        assert_eq!(size_words(12 * 1024 + 100), "12 KB");
        assert_eq!(size_words(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }
}
