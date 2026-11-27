//! The order of one drag from another program, apart from the wire: what the window system
//! says goes in, and what to answer it and to tell the app comes out. The clipboard that heard
//! of the drag does the answering (`destination`).
//!
//! The list of files is asked for twice at most: when the drag comes over the window, so the
//! app can show where those files would land, and again at the drop if the first hasn't come
//! (a program may hand its list over only once it is dropped). The window system says the
//! drag has left straight after a drop; that is no leaving, and the app is told nothing until
//! the dropped files have been read.

use crate::Told;
use crate::uri_list::Files;

/// One thing to do, in the order given.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Tell the window system the drag can be dropped here, as a copy of a list of files.
    Accept,
    /// Tell it the drag can't be dropped here.
    Refuse,
    /// Ask for the list of files held over the window, and give what is read to
    /// [`Drag::read`] with this number.
    Peek(u64),
    /// Keep what was dropped under this number, until `Finish` or `Discard` names it.
    Hold(u64),
    /// Ask for the list of files held under this number, for [`Drag::read`].
    Take(u64),
    Tell(Told),
    /// Tell the other program its files were taken, and let go of what was held.
    Finish(u64),
    /// Let go of what was held, nothing taken.
    Discard(u64),
}

/// Where the files held over the window stand.
#[derive(Debug, Default)]
enum Over {
    /// Nothing is over the window that it takes.
    #[default]
    Not,
    /// Their list was asked for under this number.
    Reading(u64),
    /// Their list couldn't be read yet.
    Unread,
    /// The app was told of them.
    Known(Files),
    /// Their list holds no file on this computer.
    Refused,
}

#[derive(Debug, Default)]
pub(crate) struct Drag {
    over: Over,
    /// The lists of dropped files still being read.
    landing: Vec<u64>,
    reads: u64,
}

impl Drag {
    fn next(&mut self) -> u64 {
        self.reads += 1;
        self.reads
    }

    /// A drag came over a surface: `takes` when it is the window's own and a list of files is
    /// offered.
    pub fn entered(&mut self, takes: bool) -> Vec<Step> {
        if !takes {
            self.over = Over::Not;
            return vec![Step::Refuse];
        }
        let number = self.next();
        self.over = Over::Reading(number);
        vec![Step::Accept, Step::Peek(number)]
    }

    /// The pointer moved with the drag over the surface.
    pub fn moved(&mut self) -> Vec<Step> {
        if self.is_over() { vec![Step::Accept] } else { Vec::new() }
    }

    /// The drag left the surface, or was dropped on it a moment ago.
    pub fn left(&mut self) -> Vec<Step> {
        let was_over = self.is_over();
        self.over = Over::Not;
        if was_over { vec![Step::Tell(Told::Gone)] } else { Vec::new() }
    }

    /// The drag was dropped on the surface.
    pub fn dropped(&mut self) -> Vec<Step> {
        let number = self.next();
        match std::mem::take(&mut self.over) {
            Over::Known(files) => vec![Step::Hold(number), Step::Tell(Told::Dropped(files)), Step::Finish(number)],
            Over::Reading(_) | Over::Unread => {
                self.landing.push(number);
                vec![Step::Hold(number), Step::Take(number)]
            }
            Over::Not | Over::Refused => vec![Step::Hold(number), Step::Discard(number)],
        }
    }

    /// The list asked for under `number` was read, or (`None`) couldn't be.
    pub fn read(&mut self, number: u64, files: Option<Files>) -> Vec<Step> {
        if let Some(at) = self.landing.iter().position(|landing| *landing == number) {
            self.landing.remove(at);
            let files = files.unwrap_or_default();
            let last = if files.paths.is_empty() { Step::Discard(number) } else { Step::Finish(number) };
            return vec![Step::Tell(Told::Dropped(files)), last];
        }
        if !matches!(self.over, Over::Reading(waited) if waited == number) {
            return Vec::new();
        }
        match files {
            Some(files) if files.paths.is_empty() => {
                self.over = Over::Refused;
                vec![Step::Refuse]
            }
            Some(files) => {
                let told = Told::Over(files.paths.clone());
                self.over = Over::Known(files);
                vec![Step::Tell(told)]
            }
            None => {
                self.over = Over::Unread;
                Vec::new()
            }
        }
    }

    /// Whether files the window takes are over it, so where the pointer is matters.
    pub fn is_over(&self) -> bool {
        !matches!(self.over, Over::Not | Over::Refused)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn two() -> Files {
        Files { paths: vec![PathBuf::from("/photos/sky.png"), PathBuf::from("/photos/sea.jpg")], skipped: 0 }
    }

    /// What a drag tells the app, of all it does.
    fn told(steps: &[Step]) -> Vec<Told> {
        steps.iter().filter_map(|step| if let Step::Tell(told) = step { Some(told.clone()) } else { None }).collect()
    }

    #[test]
    fn files_held_over_the_window_are_told_then_dropped_as_the_window_library_tells_them() {
        let mut drag = Drag::default();
        // Over the window: accepted, and the list asked for, so the app can show where they land.
        assert_eq!(drag.entered(true), [Step::Accept, Step::Peek(1)]);
        assert!(drag.is_over());
        assert_eq!(drag.moved(), [Step::Accept]);
        assert_eq!(drag.read(1, Some(two())), [Step::Tell(Told::Over(two().paths))]);
        assert_eq!(drag.moved(), [Step::Accept]);
        // Dropped: the files told of are the files dropped, and the other program hears so.
        assert_eq!(drag.dropped(), [Step::Hold(2), Step::Tell(Told::Dropped(two())), Step::Finish(2)]);
        // The window system says the drag left right after a drop: the app must not hear that
        // as the files going away, which would forget where they were dropped.
        assert_eq!(drag.left(), []);
        assert!(!drag.is_over());
    }

    #[test]
    fn files_that_leave_without_being_dropped_are_gone() {
        let mut drag = Drag::default();
        drag.entered(true);
        drag.read(1, Some(two()));
        assert_eq!(drag.left(), [Step::Tell(Told::Gone)]);
        assert!(!drag.is_over());
        // Nothing more is said of them, and the pointer moving on says nothing.
        assert_eq!((drag.left(), drag.moved()), (vec![], vec![]));
        // They left before their list came: the list, when it comes, is of no drag.
        assert_eq!(drag.entered(true), [Step::Accept, Step::Peek(2)]);
        assert_eq!(drag.left(), [Step::Tell(Told::Gone)]);
        assert_eq!(drag.read(2, Some(two())), []);
    }

    #[test]
    fn a_list_that_comes_only_at_the_drop_is_read_then() {
        // Dropped before the list held over the window had come: asked for again, and the app
        // hears of the files once that has been read, though the drag has "left" meanwhile.
        let mut drag = Drag::default();
        drag.entered(true);
        assert_eq!(drag.dropped(), [Step::Hold(2), Step::Take(2)]);
        assert_eq!(drag.left(), []);
        assert_eq!(drag.read(1, Some(two())), [], "the first asking is no longer waited for");
        assert_eq!(drag.read(2, Some(two())), [Step::Tell(Told::Dropped(two())), Step::Finish(2)]);
        // A program that hands nothing over before the drop: still accepted, and read at the drop.
        let mut drag = Drag::default();
        drag.entered(true);
        assert_eq!(drag.read(1, None), []);
        assert_eq!(drag.moved(), [Step::Accept]);
        assert_eq!(drag.dropped(), [Step::Hold(2), Step::Take(2)]);
        // Another drag may come over the window while that one is read.
        assert_eq!(drag.entered(true), [Step::Accept, Step::Peek(3)]);
        assert_eq!(drag.read(2, Some(two())), [Step::Tell(Told::Dropped(two())), Step::Finish(2)]);
        assert_eq!(drag.read(3, Some(two())), [Step::Tell(Told::Over(two().paths))]);
    }

    #[test]
    fn a_drop_that_brings_no_file_ends_the_drag_and_takes_nothing() {
        let mut drag = Drag::default();
        drag.entered(true);
        drag.read(1, None);
        drag.dropped();
        // Nothing could be read: the app is told the drag is over, with nothing dropped, and
        // the other program is not told its files were taken.
        assert_eq!(drag.read(2, None), [Step::Tell(Told::Dropped(Files::default())), Step::Discard(2)]);
    }

    #[test]
    fn a_drag_with_no_file_of_this_computer_is_refused() {
        // Web addresses only: refused once the list is read, and from then on.
        let mut drag = Drag::default();
        drag.entered(true);
        let none = Files { paths: Vec::new(), skipped: 2 };
        assert_eq!(drag.read(1, Some(none)), [Step::Refuse]);
        assert!(!drag.is_over());
        assert_eq!(drag.moved(), []);
        assert_eq!(told(&drag.left()), []);
        // Nothing the window takes (text, or over another of the app's windows): refused at once.
        assert_eq!(drag.entered(false), [Step::Refuse]);
        assert_eq!((drag.moved(), drag.left()), (vec![], vec![]));
        // Should the window system say it was dropped all the same, it is let go of.
        drag.entered(false);
        assert_eq!(drag.dropped(), [Step::Hold(2), Step::Discard(2)]);
    }

    #[test]
    fn coming_over_the_window_again_starts_afresh() {
        let mut drag = Drag::default();
        drag.entered(true);
        drag.read(1, Some(two()));
        drag.left();
        assert_eq!(drag.entered(true), [Step::Accept, Step::Peek(2)]);
        let one = Files { paths: vec![PathBuf::from("/photos/sky.png")], skipped: 1 };
        assert_eq!(told(&drag.read(2, Some(one.clone()))), [Told::Over(one.paths.clone())]);
        assert_eq!(told(&drag.dropped()), [Told::Dropped(one)]);
    }
}
