//! Smithay Clipboard
//!
//! Provides access to the Wayland clipboard for gui applications. The user
//! should have surface around.

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
use std::ffi::c_void;
use std::io::Result;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, Weak};

use sctk::reexports::calloop::channel::{self, Sender};
use sctk::reexports::client::Connection;
use sctk::reexports::client::backend::Backend;

// SoftPresenter: drags from other programs are answered here too (see PROVENANCE.md).
pub mod dnd;
mod mime;
mod state;
mod worker;

/// Access to a Wayland clipboard.
// SoftPresenter: every `Clipboard` made for one display shares one worker (one thread, one
// queue, one `wl_data_device` for each seat) instead of having its own; see PROVENANCE.md.
// GNOME's compositor keeps one data device for a program: a second one made while the program
// doesn't have the keyboard takes the first one's place for good, so a program with a
// clipboard for each window could find the clipboard (and drags) dead in every window but the
// newest, and in all of them once that one closed.
pub struct Clipboard {
    worker: Arc<Worker>,
}

/// The thread that talks to the compositor, and the way to it.
struct Worker {
    /// The `wl_display` it was made for, as its address.
    display: usize,
    request_sender: Sender<worker::Command>,
    /// A request and its reply are one exchange: locked for both.
    request_receiver: Mutex<Receiver<Result<String>>>,
    clipboard_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// The worker in use, if a `Clipboard` is alive.
static SHARED: Mutex<Weak<Worker>> = Mutex::new(Weak::new());

impl Worker {
    /// Whether a new `Clipboard` for `display` can share this worker: it was made for that
    /// display and its thread still runs. The thread ends early when the compositor can't be
    /// spoken to any more or something on it panicked, and a worker whose thread has ended
    /// drops what is copied and hears of no drag: a new `Clipboard` gets a fresh one instead.
    fn serves(&self, display: usize) -> bool {
        let thread = self.clipboard_thread.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.display == display && thread.as_ref().is_some_and(|thread| !thread.is_finished())
    }
}

impl Clipboard {
    /// Creates new clipboard which will be running on its own thread with its
    /// own event queue to handle clipboard requests.
    ///
    /// SoftPresenter: the thread and queue are shared with every other `Clipboard` alive for
    /// the same display, and go when the last of them is dropped. Should the thread have ended
    /// by itself, the next `Clipboard` made starts another (those made before keep the ended
    /// one, and say "clipboard is dead" when asked to load).
    ///
    /// # Safety
    ///
    /// `display` must be a valid `*mut wl_display` pointer, and it must remain
    /// valid for as long as `Clipboard` object is alive.
    pub unsafe fn new(display: *mut c_void) -> Self {
        let mut shared = SHARED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // SoftPresenter: only a worker for this display whose thread still runs is shared.
        if let Some(worker) = shared.upgrade().filter(|worker| worker.serves(display as usize)) {
            return Self { worker };
        }

        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);

        // Create channel to send data to clipboard thread.
        let (request_sender, rx_chan) = channel::channel();
        // Create channel to get data from the clipboard thread.
        let (clipboard_reply_sender, request_receiver) = mpsc::channel();

        let name = String::from("smithay-clipboard");
        // SoftPresenter: the worker is given a way to send itself commands, for the threads
        // that read what a drag hands over.
        let clipboard_thread = worker::spawn(
            name,
            connection,
            rx_chan,
            request_sender.clone(),
            clipboard_reply_sender,
        );

        let worker = Arc::new(Worker {
            display: display as usize,
            request_receiver: Mutex::new(request_receiver),
            request_sender,
            clipboard_thread: Mutex::new(clipboard_thread),
        });
        *shared = Arc::downgrade(&worker);
        Self { worker }
    }

    /// Sends a request for the clipboard's content and waits for the reply.
    fn request(&self, request: worker::Command) -> Result<String> {
        let receiver =
            self.worker.request_receiver.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = self.worker.request_sender.send(request);

        if let Ok(reply) = receiver.recv() {
            reply
        } else {
            // The clipboard thread is dead, however we shouldn't crash downstream, so
            // propogating an error.
            Err(std::io::Error::other("clipboard is dead."))
        }
    }

    /// Load clipboard data.
    ///
    /// Loads content from a clipboard on a last observed seat.
    pub fn load(&self) -> Result<String> {
        self.request(worker::Command::Load)
    }

    /// Store to a clipboard.
    ///
    /// Stores to a clipboard on a last observed seat.
    pub fn store<T: Into<String>>(&self, text: T) {
        let request = worker::Command::Store(text.into());
        let _ = self.worker.request_sender.send(request);
    }

    /// Load primary clipboard data.
    ///
    /// Loads content from a  primary clipboard on a last observed seat.
    pub fn load_primary(&self) -> Result<String> {
        self.request(worker::Command::LoadPrimary)
    }

    /// Store to a primary clipboard.
    ///
    /// Stores to a primary clipboard on a last observed seat.
    pub fn store_primary<T: Into<String>>(&self, text: T) {
        let request = worker::Command::StorePrimary(text.into());
        let _ = self.worker.request_sender.send(request);
    }
}

// SoftPresenter: was `impl Drop for Clipboard`; the worker goes with the last `Clipboard`.
impl Drop for Worker {
    fn drop(&mut self) {
        // Shutdown smithay-clipboard.
        let _ = self.request_sender.send(worker::Command::Exit);
        let thread = self.clipboard_thread.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(clipboard_thread) = thread.take() {
            let _ = clipboard_thread.join();
        }
    }
}

// SoftPresenter: the tests are SoftPresenter's.
#[cfg(test)]
mod tests {
    use super::*;

    /// A worker for display 7 whose thread runs until the returned sender is dropped.
    fn worker() -> (Worker, mpsc::Sender<()>) {
        let (request_sender, _commands) = channel::channel();
        let (_replies, request_receiver) = mpsc::channel();
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let _ = stopped.recv();
        });
        let worker = Worker {
            display: 7,
            request_sender,
            request_receiver: Mutex::new(request_receiver),
            clipboard_thread: Mutex::new(Some(thread)),
        };
        (worker, stop)
    }

    #[test]
    fn a_worker_is_shared_only_for_its_display_and_only_while_its_thread_runs() {
        let (worker, stop) = worker();
        assert!(worker.serves(7));
        assert!(!worker.serves(8), "another display");
        // The thread ends by itself, as on an error from the compositor.
        drop(stop);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while worker.serves(7) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!worker.serves(7), "a worker whose thread has ended is not handed out again");
        // One whose thread could never be started is no use either.
        let thread = worker.clipboard_thread.lock().unwrap().take();
        let _ = thread.map(std::thread::JoinHandle::join);
        assert!(!worker.serves(7));
    }
}
