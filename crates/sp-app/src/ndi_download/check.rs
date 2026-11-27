//! Whether a newer NDI may be out.
//!
//! NDI publishes no list of versions. What can be known is whether NDI's download has changed
//! since the NDI on this computer was installed: its server says, without sending the file
//! (a HEAD request), when the download was last changed, a mark that changes with its contents
//! (its ETag) and its size. For an NDI the app installed, those three were noted at the time,
//! and any that differs now means NDI has put another download there. For an NDI installed some
//! other way, the download's date is set against the runtime file's own. Either way the answer
//! is only ever "a newer NDI may be available": no version number is known, and none is said.
//!
//! The request goes to the same fixed address as the download, over HTTPS, and only when the
//! operator asks or, at most once a day, when the NDI card is opened; a test copy asks nobody
//! but its own stand-in. What the server answers is untrusted: each mark is kept only if it is
//! short, plain text, compared for equality or as a date, and never shown.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::sdk::Problem;

/// The longest mark kept. NDI's are 34 and 29 letters.
const LONGEST_MARK: usize = 128;
/// Where the last check is noted, in the app's data folder.
const CHECKED_FILE: &str = "ndi-check.json";
/// How long a check made on opening the card stands before another is made.
pub const DAY: i64 = 24 * 60 * 60;

/// What NDI's server says of its download.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marks {
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

/// A header's value, if it is a mark worth keeping: short and plain.
fn mark(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let plain = !value.is_empty() && value.len() <= LONGEST_MARK && value.bytes().all(|b| (0x20..0x7f).contains(&b));
    plain.then(|| value.to_owned())
}

impl Marks {
    /// The marks in an answer's headers.
    pub fn of(etag: Option<&str>, last_modified: Option<&str>, size: Option<u64>) -> Marks {
        Marks { etag: mark(etag), last_modified: mark(last_modified), size: size.filter(|size| *size > 0) }
    }

    pub fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none() && self.size.is_none()
    }

    /// When the download was last changed, if the server said so in the form HTTP uses.
    fn changed(&self) -> Option<SystemTime> {
        let date = chrono::DateTime::parse_from_rfc2822(self.last_modified.as_deref()?).ok()?;
        Some(UNIX_EPOCH + Duration::from_secs(u64::try_from(date.timestamp()).ok()?))
    }
}

/// Whether the download has changed since an NDI the app installed was downloaded: `then` are
/// the marks noted at the time, `now` the server's. The mark that changes with the contents
/// decides where both have it; else the date; else the size. None when nothing can be compared.
pub fn newer_than_installed(then: &Marks, now: &Marks) -> Option<bool> {
    if let (Some(then), Some(now)) = (&then.etag, &now.etag) {
        return Some(then != now);
    }
    if let (Some(then), Some(now)) = (&then.last_modified, &now.last_modified) {
        return Some(then != now);
    }
    Some(then.size? != now.size?)
}

/// Whether the download was changed after a runtime file's own date (`file`: the later of when
/// it was made and when it was last written, so a copy an installer made with its old date
/// still counts from the day it was installed). None when the server gave no date.
pub fn newer_than_file(file: SystemTime, now: &Marks) -> Option<bool> {
    Some(now.changed()? > file)
}

/// The date of a runtime file for [`newer_than_file`].
pub fn file_date(runtime: &Path) -> Option<SystemTime> {
    let about = std::fs::metadata(runtime).ok()?;
    let written = about.modified().ok();
    let made = about.created().ok();
    match (written, made) {
        (Some(written), Some(made)) => Some(written.max(made)),
        (one, other) => one.or(other),
    }
}

/// Asks NDI's server for its download's marks, without the download.
pub fn head(address: &str, stand_in: bool) -> Result<Marks, Problem> {
    use ureq::ResponseExt;
    let response = super::agent(address, stand_in, Duration::from_secs(30))
        .head(address)
        .call()
        .map_err(|e| super::request_problem(&e))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Problem::Network(format!("NDI's server answered {status}")));
    }
    if !stand_in && !super::from_ndi(response.get_uri().host()) {
        return Err(Problem::Damaged("the answer came from somewhere other than NDI's server".to_owned()));
    }
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok());
    let size = header("content-length").and_then(|length| length.trim().parse().ok());
    let marks = Marks::of(header("etag"), header("last-modified"), size);
    if marks.is_empty() {
        return Err(Problem::Network("NDI's server said nothing of its download".to_owned()));
    }
    Ok(marks)
}

/// The last check: when, and what it found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checked {
    /// When, in seconds since 1970.
    pub at: i64,
    pub newer: bool,
    /// The server's marks then, for an update to be noted with.
    #[serde(default)]
    pub marks: Marks,
}

impl Checked {
    /// The last check noted in the data folder, if there is one.
    pub fn read(data: &Path) -> Option<Checked> {
        let bytes = std::fs::read(data.join(CHECKED_FILE)).ok()?;
        (bytes.len() < 4096).then(|| serde_json::from_slice(&bytes).ok())?
    }

    pub fn write(&self, data: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = super::sdk::write_whole(data, CHECKED_FILE, &bytes);
        }
    }

    pub fn forget(data: &Path) {
        let _ = std::fs::remove_file(data.join(CHECKED_FILE));
    }

    /// True when a check made on opening the card is due: none yet, or the last a day old.
    pub fn due(last: Option<&Checked>, now: i64) -> bool {
        last.is_none_or(|last| now - last.at >= DAY || now < last.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(etag: &str, changed: &str, size: u64) -> Marks {
        Marks::of(Some(etag), Some(changed), Some(size))
    }

    /// The marks NDI's server gave for its SDK for Linux on 27 November 2026.
    fn ndis() -> Marks {
        marks("\"3ddab50374ef2369cc0bc63847dcd08a\"", "Wed, 18 Nov 2026 15:58:50 GMT", 67_470_104)
    }

    #[test]
    fn the_same_download_is_not_newer_and_a_changed_one_is() {
        assert_eq!(newer_than_installed(&ndis(), &ndis()), Some(false));
        let replaced = marks("\"0000b50374ef2369cc0bc63847dc0000\"", "Tue, 02 Feb 2027 10:00:00 GMT", 68_000_000);
        assert_eq!(newer_than_installed(&ndis(), &replaced), Some(true));
        // The mark that changes with the contents decides: the same file served again with a
        // new date is not newer.
        let served_again = Marks { last_modified: Some("Tue, 02 Feb 2027 10:00:00 GMT".to_owned()), ..ndis() };
        assert_eq!(newer_than_installed(&ndis(), &served_again), Some(false));
        // Without that mark on one side, the date; without both, the size.
        let dated = Marks { etag: None, ..ndis() };
        assert_eq!(newer_than_installed(&dated, &ndis()), Some(false));
        assert_eq!(newer_than_installed(&dated, &replaced), Some(true));
        let sized = Marks { etag: None, last_modified: None, size: Some(67_470_104) };
        assert_eq!(newer_than_installed(&sized, &ndis()), Some(false));
        assert_eq!(newer_than_installed(&sized, &replaced), Some(true));
        // An install from before the marks were noted can't be compared this way.
        assert_eq!(newer_than_installed(&Marks::default(), &ndis()), None);
    }

    #[test]
    fn a_runtime_installed_some_other_way_goes_by_its_files_date() {
        let at = |text: &str| {
            UNIX_EPOCH
                + Duration::from_secs(chrono::DateTime::parse_from_rfc3339(text).unwrap().timestamp().unsigned_abs())
        };
        // Installed in October: the download of 18 November is newer.
        assert_eq!(newer_than_file(at("2026-10-01T09:00:00Z"), &ndis()), Some(true));
        // Installed after the download was last changed: nothing newer.
        assert_eq!(newer_than_file(at("2026-11-20T09:00:00Z"), &ndis()), Some(false));
        // A server that gives no date, or one that isn't a date, decides nothing.
        assert_eq!(newer_than_file(at("2026-10-01T09:00:00Z"), &Marks { last_modified: None, ..ndis() }), None);
        let odd = Marks { last_modified: Some("yesterday".to_owned()), ..ndis() };
        assert_eq!(newer_than_file(at("2026-10-01T09:00:00Z"), &odd), None);
    }

    #[test]
    fn only_short_plain_marks_are_kept() {
        let long = "x".repeat(LONGEST_MARK + 1);
        let kept = Marks::of(Some(&long), Some("Wed, 18 Nov 2026\r\nSet-Cookie: x"), Some(0));
        assert!(kept.is_empty(), "{kept:?}");
        let kept = Marks::of(Some(" \"abc\" "), None, Some(12));
        assert_eq!(kept, Marks { etag: Some("\"abc\"".to_owned()), last_modified: None, size: Some(12) });
    }

    #[test]
    fn a_check_on_opening_the_card_is_due_once_a_day() {
        let last = Checked { at: 1_000_000, newer: false, marks: Marks::default() };
        assert!(Checked::due(None, 1_000_000));
        assert!(!Checked::due(Some(&last), 1_000_000 + DAY - 1));
        assert!(Checked::due(Some(&last), 1_000_000 + DAY));
        // A clock set back doesn't stop checks for ever.
        assert!(Checked::due(Some(&last), 999_000));
    }

    #[test]
    fn the_last_check_is_kept_and_read_back() {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(Checked::read(&folder), None);
        let checked = Checked { at: 1_795_000_000, newer: true, marks: ndis() };
        checked.write(&folder);
        assert_eq!(Checked::read(&folder), Some(checked));
        Checked::forget(&folder);
        assert_eq!(Checked::read(&folder), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
