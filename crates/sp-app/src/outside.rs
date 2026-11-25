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
//!
//! The app's own saves are written a moment later, on the library writer's thread (`writer`):
//! while one of a file is on its way, the file counts as unchanged (what is on disk is about to
//! be the app's), and once it is written, its stamp is the one the writer found straight after.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::writer::Guard;

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

/// The stamps of the files the app read or wrote, and the files it is writing.
#[derive(Default)]
pub struct Stamps {
    seen: HashMap<PathBuf, Stamp>,
    /// Files with a save of the app's on its way, by the number the writer gave the last one.
    writing: HashMap<PathBuf, u64>,
}

impl Stamps {
    /// Notes how the file is now, as the app has just read it.
    pub fn note(&mut self, path: &Path) {
        self.set(path, stamp(path));
    }

    fn set(&mut self, path: &Path, now: Option<Stamp>) {
        match now {
            Some(now) => self.seen.insert(path.to_path_buf(), now),
            None => self.seen.remove(path),
        };
    }

    /// How the file is now, beside how the app last saw it. A file the app is writing is as the
    /// app has it.
    pub fn check(&self, path: &Path) -> Now {
        if self.writing.contains_key(path) {
            return Now::Same;
        }
        match (self.seen.get(path), stamp(path)) {
            (Some(then), Some(now)) if *then == now => Now::Same,
            (None, None) => Now::Same,
            (Some(_), None) => Now::Gone,
            _ => Now::Changed,
        }
    }

    /// Whether a save of the app's is on its way to the file.
    pub fn is_writing(&self, path: &Path) -> bool {
        self.writing.contains_key(path)
    }

    /// When a save of the file may go ahead: only if it is still as the app last saw it, or, with
    /// a save of it still on its way, as that save leaves it.
    pub fn guard(&self, path: &Path) -> Guard {
        if self.writing.contains_key(path) { Guard::AsLeft } else { Guard::Unchanged(self.seen.get(path).copied()) }
    }

    /// A save of the file was handed to the writer, which numbered it `seq`.
    pub fn writing(&mut self, path: &Path, seq: u64) {
        self.writing.insert(path.to_path_buf(), seq);
    }

    /// A save of the file ended: written, leaving the file as `now` says (`Some`), or not
    /// (`None`: the file is as it was). Only the last save handed over counts; until it ends,
    /// the file is still being written.
    pub fn written(&mut self, path: &Path, seq: u64, now: Option<Option<Stamp>>) {
        if self.writing.get(path) != Some(&seq) {
            return;
        }
        self.writing.remove(path);
        if let Some(now) = now {
            self.set(path, now);
        }
    }
}

/// Something the app is waiting on a choice for: a song or theme changed outside it, a
/// device that wants to show its screen (by the number the receiving program gave it), or the
/// one-time notice that anonymous reports are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Waiting {
    Song(usize),
    Theme(usize),
    Mirror(u64),
    Reports,
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

    #[test]
    fn the_apps_own_saves_on_their_way_are_never_taken_for_changes_made_outside() {
        use crate::writer::{Done, Kind, Save, Writer};
        let dir = std::env::temp_dir().join(format!("sp-outside-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("song.sppres");
        std::fs::write(&song, "read at the start").unwrap();
        let mut stamps = Stamps::default();
        stamps.note(&song);
        let read = stamp(&song);
        assert_eq!(stamps.guard(&song), Guard::Unchanged(read), "saved only over the file as read");
        // Two saves handed over, one after the other.
        stamps.writing(&song, 1);
        assert_eq!(stamps.guard(&song), Guard::AsLeft, "a save behind another of the app's");
        stamps.writing(&song, 2);
        // The writer writes the first; the app hasn't been told yet.
        let writer = Writer::start(|| {});
        writer.write(Save::new(song.clone(), Kind::Song, Guard::Any, || b"the app's first save".to_vec()));
        writer.drain();
        let first = writer.finished().into_iter().find_map(|done| match done {
            Done::Written { stamp, .. } => Some(stamp),
            _ => None,
        });
        assert!(first.is_some_and(|first| first != read), "the file on disk is new");
        assert_eq!(stamps.check(&song), Now::Same, "the app's own save, not a change made outside");
        // The first ending changes nothing while the second is on its way.
        stamps.written(&song, 1, first);
        assert!(stamps.is_writing(&song));
        assert_eq!(stamps.check(&song), Now::Same);
        // The second, written, is how the app last saw the file.
        std::fs::write(&song, "the app's second save, longer").unwrap();
        stamps.written(&song, 2, Some(stamp(&song)));
        assert!(!stamps.is_writing(&song));
        assert_eq!(stamps.check(&song), Now::Same);
        assert_eq!(stamps.guard(&song), Guard::Unchanged(stamp(&song)));
        // A change made outside afterwards is seen.
        std::fs::write(&song, "changed by someone else").unwrap();
        assert_eq!(stamps.check(&song), Now::Changed);
        // A save that ended unwritten (kept from that change, or failed) leaves the last stamp.
        stamps.writing(&song, 3);
        stamps.written(&song, 3, None);
        assert_eq!(stamps.check(&song), Now::Changed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
