//! Files dragged onto a window on a Wayland desktop.
//!
//! The window library (winit 0.30) says when files come over a window and when they are dropped
//! on macOS, Windows and X11, and nothing at all on Wayland. There the app has to hear of them
//! itself, and it can only through the clipboard each of its windows already has: a drag is
//! told to one of a program's "data devices" for a seat, the compositor's choice, and every
//! window's clipboard holds one (a listener with a device of its own heard of drags only until
//! the next window opened). So SoftPresenter's copy of the clipboard crate
//! (`third_party/smithay-clipboard`) answers drags too, and asks [`FileDrops`] here what to
//! answer: the order of a drag ([`drag`]) and the reading of the list of files the other
//! program hands over ([`uri_list`]). On other systems only those two are built.

#[cfg(any(target_os = "linux", test))]
mod drag;
pub mod uri_list;

#[cfg(target_os = "linux")]
mod destination;

#[cfg(target_os = "linux")]
pub use destination::FileDrops;
pub use uri_list::Files;

use std::path::PathBuf;

/// What is said of files dragged from another program, in the order it happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Told {
    /// These files came over the window.
    Over(Vec<PathBuf>),
    /// They left it again without being dropped.
    Gone,
    /// They were dropped on it. With no files (none of what was dragged is a file on this
    /// computer, or the other program handed nothing over), nothing was dropped and the drag is
    /// over.
    Dropped(Files),
}
