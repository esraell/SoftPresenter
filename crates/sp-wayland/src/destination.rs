//! The app's answer to the clipboards' question of what to do with a drag (see the crate's
//! notes): `drag` decides, the app is told, and the pointer's place is kept for it.

use std::sync::{Arc, Mutex, PoisonError};

use raw_window_handle::{RawWindowHandle, WindowHandle};
use smithay_clipboard::dnd::{DragDestination, Request, set_destination};

use crate::Told;
use crate::drag::{Drag, Step};
use crate::uri_list::local_files;

/// The kind of data a list of files is handed over as.
const LIST: &str = "text/uri-list";

/// Takes files dragged onto one window from other programs, until dropped.
pub struct FileDrops {
    window: Arc<Window>,
}

/// The one window that takes files, as the clipboards' threads see it.
struct Window {
    /// Its surface, as the address the system's Wayland library knows it by.
    surface: usize,
    drag: Mutex<Dragging>,
    tell: Box<dyn Fn(Told) + Send + Sync>,
    /// This computer's name, which a file's address may carry.
    host: Option<String>,
}

#[derive(Default)]
struct Dragging {
    drag: Drag,
    /// Where the pointer is on the window while files are over it, and where it was at the
    /// last drop, in the window system's own measure.
    pointer: Option<(f64, f64)>,
}

impl FileDrops {
    /// Starts taking files dragged onto `window`. `tell` is called with what happens, on a
    /// thread of one of the app's clipboards, in order. `None` when the window isn't on
    /// Wayland: the window library says of drags itself then.
    ///
    /// One window of the app can take files this way at a time; dropping this stops it.
    pub fn listen(window: WindowHandle<'_>, tell: impl Fn(Told) + Send + Sync + 'static) -> Option<FileDrops> {
        let RawWindowHandle::Wayland(handle) = window.as_raw() else {
            return None;
        };
        let window = Arc::new(Window {
            surface: handle.surface.as_ptr() as usize,
            drag: Mutex::default(),
            tell: Box::new(tell),
            host: std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|name| name.trim().to_owned()),
        });
        set_destination(Some(window.clone()));
        Some(FileDrops { window })
    }

    /// Where the pointer is on the window while files it takes are over it, and where it was
    /// when they were last dropped: in the window system's own measure, which the window's
    /// scale factor turns into pixels.
    pub fn pointer(&self) -> Option<(f64, f64)> {
        self.window.drag.lock().unwrap_or_else(PoisonError::into_inner).pointer
    }
}

impl Drop for FileDrops {
    fn drop(&mut self) {
        set_destination(None);
    }
}

impl Window {
    /// Has `drag` decide (with the pointer's place noted first, when it is given and files the
    /// window takes are then over it), tells the app what there is to tell, and hands back
    /// what the clipboard is to answer. The lock is held throughout, so the app hears things
    /// in the order they happened whichever threads they happened on.
    fn decide(&self, pointer: Option<(f64, f64)>, decide: impl FnOnce(&mut Drag) -> Vec<Step>) -> Vec<Request> {
        let mut dragging = self.drag.lock().unwrap_or_else(PoisonError::into_inner);
        let steps = decide(&mut dragging.drag);
        if pointer.is_some() && dragging.drag.is_over() {
            dragging.pointer = pointer;
        }
        let mut requests = Vec::new();
        for step in steps {
            match step {
                Step::Accept => requests.push(Request::Accept(LIST.to_owned())),
                Step::Refuse => requests.push(Request::Refuse),
                Step::Peek(number) => requests.push(Request::Peek(number, LIST.to_owned())),
                Step::Hold(number) => requests.push(Request::Hold(number)),
                Step::Take(number) => requests.push(Request::Take(number, LIST.to_owned())),
                Step::Finish(number) => requests.push(Request::Finish(number)),
                Step::Discard(number) => requests.push(Request::Discard(number)),
                Step::Tell(told) => {
                    if told == Told::Gone {
                        dragging.pointer = None;
                    }
                    (self.tell)(told);
                }
            }
        }
        requests
    }
}

impl DragDestination for Window {
    fn entered(&self, surface: usize, x: f64, y: f64, mime_types: &[String]) -> Vec<Request> {
        // Only the window itself takes files: not the app's other windows, nor the title bar
        // the window library draws around this one (surfaces of their own).
        let takes = surface == self.surface && mime_types.iter().any(|kind| kind == LIST);
        self.decide(Some((x, y)), |drag| drag.entered(takes))
    }

    fn moved(&self, x: f64, y: f64) -> Vec<Request> {
        self.decide(Some((x, y)), Drag::moved)
    }

    fn left(&self) -> Vec<Request> {
        self.decide(None, Drag::left)
    }

    fn dropped(&self) -> Vec<Request> {
        self.decide(None, Drag::dropped)
    }

    fn read(&self, number: u64, data: Option<Vec<u8>>) -> Vec<Request> {
        let files = data.map(|list| local_files(&list, self.host.as_deref()));
        self.decide(None, |drag| drag.read(number, files))
    }
}
