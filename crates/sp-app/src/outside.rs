//! Songs and themes changed outside the app (by a person, a script or an agent). The app
//! remembers each file's modification time and size from when it read or last wrote it, so it
//! can tell a file changed since:
//! - it never saves over such a file; it offers Reload (take the file's version) or Keep mine
//!   (write the app's version over it);
//! - a song is read again from its file when it is opened, unless it is on screen, open in the
//!   editor, or holds changes not yet saved (then Reload is offered instead).
//!
//! No folder is watched: a change comes in when the song is opened or reloaded, never in the
//! middle of a slide that is on screen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A file's modification time and size.
pub type Stamp = (SystemTime, u64);

/// The stamp of a file as it is now, or `None` when there is no such file.
pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok().filter(|meta| meta.is_file())?;
    Some((meta.modified().ok()?, meta.len()))
}

/// How a file is now, beside how the app last saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Now {
    /// As the app last read or wrote it (or there was none and there is none).
    Same,
    /// Written since, or made where there was none.
    Changed,
    /// Gone since.
    Gone,
}

/// The stamps of the files the app read or wrote.
#[derive(Default)]
pub struct Stamps(HashMap<PathBuf, Stamp>);

impl Stamps {
    /// Notes how the file is now, as the app has just read or written it.
    pub fn note(&mut self, path: &Path) {
        match stamp(path) {
            Some(now) => self.0.insert(path.to_path_buf(), now),
            None => self.0.remove(path),
        };
    }

    /// How the file is now, beside how the app last saw it.
    pub fn check(&self, path: &Path) -> Now {
        match (self.0.get(path), stamp(path)) {
            (Some(then), Some(now)) if *then == now => Now::Same,
            (None, None) => Now::Same,
            (Some(_), None) => Now::Gone,
            _ => Now::Changed,
        }
    }
}

/// Something the app is waiting on a choice for: a song or theme changed outside it, or a
/// device that wants to show its screen (by the number the receiving program gave it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Waiting {
    Song(usize),
    Theme(usize),
    Mirror(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_written_or_removed_since_is_told_apart_from_one_left_alone() {
        let dir = std::env::temp_dir().join(format!("sp-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("song.sppres");
        let mut stamps = Stamps::default();
        assert_eq!(stamps.check(&song), Now::Same, "none then, none now");
        std::fs::write(&song, "one").unwrap();
        assert_eq!(stamps.check(&song), Now::Changed, "made since");
        stamps.note(&song);
        assert_eq!(stamps.check(&song), Now::Same);
        std::fs::write(&song, "a longer one").unwrap();
        assert_eq!(stamps.check(&song), Now::Changed, "written since");
        stamps.note(&song);
        std::fs::remove_file(&song).unwrap();
        assert_eq!(stamps.check(&song), Now::Gone);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
