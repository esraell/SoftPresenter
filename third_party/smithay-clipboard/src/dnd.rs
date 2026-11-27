// SoftPresenter: this whole file is SoftPresenter's addition (see PROVENANCE.md).

//! Being the destination of a drag from another program.
//!
//! A clipboard needs a `wl_data_device` for its seat, and so does a drag's destination. A
//! program that makes two of them for one seat finds that some compositors (GNOME's, for one)
//! tell only one of them about a drag, and not one the program can choose: so a drag has to be
//! answered here, on the device the clipboard already has, or the program can't rely on
//! hearing of it at all.
//!
//! This crate does the talking to the compositor and the reading of what is handed over. What
//! to answer is the program's to decide: it registers one [`DragDestination`] for the whole
//! process ([`set_destination`]), which every [`crate::Clipboard`] asks, whichever of them the
//! compositor tells. Each of its methods says what happened and returns the [`Request`]s to
//! make in reply, in order. They are called on a clipboard's own thread and must not block.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::sync::{Arc, RwLock};

use sctk::data_device_manager::data_device::DataDeviceData;
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::reexports::calloop::channel::Sender;
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_surface::WlSurface;

use crate::state::State;
use crate::worker::Command;

/// The most that is read of what a drag hands over, in bytes: a source that writes without end
/// is not read for ever.
pub const MOST_READ: u64 = 1 << 20;

/// Decides what happens to drags that come over the program's windows.
pub trait DragDestination: Send + Sync {
    /// A drag came over a surface of the program: the surface as the address of its
    /// `wl_surface` (what `raw-window-handle` gives for a window), the pointer's place on it,
    /// and the kinds of data offered.
    fn entered(&self, surface: usize, x: f64, y: f64, mime_types: &[String]) -> Vec<Request>;
    /// The pointer moved with the drag over that surface.
    fn moved(&self, x: f64, y: f64) -> Vec<Request>;
    /// The drag left the surface. The compositor says so after a drop on it too.
    fn left(&self) -> Vec<Request>;
    /// The drag was dropped on the surface.
    fn dropped(&self) -> Vec<Request>;
    /// What was asked for with `Peek` or `Take` under this number has been read, or (`None`)
    /// couldn't be: nothing came, or more than [`MOST_READ`].
    fn read(&self, number: u64, data: Option<Vec<u8>>) -> Vec<Request>;
}

/// One thing to answer a drag with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Say the drag can be dropped here, as a copy of this kind of data.
    Accept(String),
    /// Say the drag can't be dropped here.
    Refuse,
    /// Ask for the data of the drag over the surface, in this kind, to be given to
    /// [`DragDestination::read`] with this number.
    Peek(u64, String),
    /// Keep what was just dropped under this number, until `Finish` or `Discard` names it.
    Hold(u64),
    /// Ask for the data held under this number, in this kind, for [`DragDestination::read`].
    Take(u64, String),
    /// Tell the source its data was taken, and let go of what was held under this number.
    Finish(u64),
    /// Let go of what was held under this number, nothing taken.
    Discard(u64),
}

static DESTINATION: RwLock<Option<Arc<dyn DragDestination>>> = RwLock::new(None);

/// Sets the one destination of drags for the whole process, or (`None`) takes it away, after
/// which drags are refused as they were before.
pub fn set_destination(destination: Option<Arc<dyn DragDestination>>) {
    *DESTINATION.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = destination;
}

fn destination() -> Option<Arc<dyn DragDestination>> {
    DESTINATION.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// What a drag offers, and the action the compositor has settled on for it.
#[derive(Debug)]
struct Offered {
    offer: DragOffer,
    action: DndAction,
}

/// The drags one clipboard's thread is answering.
#[derive(Debug, Default)]
pub(crate) struct Drags {
    /// What is over a surface now. One place for the whole thread, and the events that
    /// follow an enter (motion, leave, drop) are taken without looking at which data device
    /// spoke: one drag at a time. Two seats dragging over the program's windows at once would
    /// cross, each one's events answered with the other's offer.
    over: Option<Offered>,
    /// What was dropped and is not let go of yet, by `Request::Hold`'s number.
    held: Vec<(u64, Offered)>,
}

impl State {
    pub(crate) fn drag_entered(&mut self, device: &WlDataDevice, x: f64, y: f64, surface: &WlSurface) {
        let offer = device.data::<DataDeviceData>().and_then(DataDeviceData::drag_offer);
        let kinds = offer.as_ref().map(|offer| offer.with_mime_types(<[String]>::to_vec)).unwrap_or_default();
        self.drags.over = offer.map(|offer| Offered { action: offer.selected_action, offer });
        if let Some(destination) = destination() {
            let requests = destination.entered(surface.id().as_ptr() as usize, x, y, &kinds);
            self.drag_answer(requests);
        }
    }

    pub(crate) fn drag_moved(&mut self, x: f64, y: f64) {
        if let Some(destination) = destination() {
            let requests = destination.moved(x, y);
            self.drag_answer(requests);
        }
    }

    pub(crate) fn drag_left(&mut self) {
        if let Some(destination) = destination() {
            let requests = destination.left();
            self.drag_answer(requests);
        }
        // The toolkit has destroyed an offer that wasn't dropped; one that was is held by now.
        self.drags.over = None;
    }

    pub(crate) fn drag_dropped(&mut self) {
        if let Some(destination) = destination() {
            let requests = destination.dropped();
            self.drag_answer(requests);
        }
    }

    /// A reading thread has what was asked for under `number`.
    pub(crate) fn drag_read(&mut self, number: u64, data: Option<Vec<u8>>) {
        if let Some(destination) = destination() {
            let requests = destination.read(number, data);
            self.drag_answer(requests);
        }
    }

    /// The compositor settled on an action for `offer`.
    pub(crate) fn drag_action(&mut self, offer: &DragOffer, action: DndAction) {
        let mut all = self.drags.over.iter_mut().chain(self.drags.held.iter_mut().map(|(_, held)| held));
        if let Some(offered) = all.find(|offered| offered.offer.inner() == offer.inner()) {
            offered.action = action;
        }
    }

    /// Makes the destination's requests, in order. They are sent when the thread next waits.
    fn drag_answer(&mut self, requests: Vec<Request>) {
        for request in requests {
            let finish = matches!(request, Request::Finish(_));
            match request {
                Request::Accept(kind) => {
                    if let Some(over) = &self.drags.over {
                        over.offer.accept_mime_type(over.offer.serial, Some(kind));
                        over.offer.set_actions(DndAction::Copy, DndAction::Copy);
                    }
                },
                Request::Refuse => {
                    if let Some(over) = &self.drags.over {
                        over.offer.accept_mime_type(over.offer.serial, None);
                        // No action either, so a source that looks only at the action (not at
                        // the kind accepted) shows "not here" too.
                        over.offer.set_actions(DndAction::empty(), DndAction::empty());
                    }
                },
                Request::Peek(number, kind) => {
                    let offer = self.drags.over.as_ref().map(|over| over.offer.clone());
                    self.drag_receive(number, offer, kind);
                },
                Request::Hold(number) => {
                    if let Some(over) = self.drags.over.take() {
                        self.drags.held.push((number, over));
                    }
                },
                Request::Take(number, kind) => {
                    let held = self.drags.held.iter().find(|(held, _)| *held == number);
                    let offer = held.map(|(_, held)| held.offer.clone());
                    self.drag_receive(number, offer, kind);
                },
                Request::Finish(number) | Request::Discard(number) => {
                    let Some(at) = self.drags.held.iter().position(|(held, _)| *held == number) else {
                        continue;
                    };
                    let (_, held) = self.drags.held.remove(at);
                    // "Finished" may be said only of a drop with an action agreed; said of
                    // any other it is a protocol error, which ends the whole connection.
                    if finish && held.action == DndAction::Copy {
                        held.offer.finish();
                    }
                    held.offer.destroy();
                },
            }
        }
    }

    /// Asks `offer` for its data in `kind` and reads it on a thread of its own, which hands it
    /// to this one (`Command::DragRead`) when it has it all: a source may take as long as it
    /// likes to write, or never finish. With no offer, pipe or thread, the destination hears
    /// now that nothing could be read.
    fn drag_receive(&mut self, number: u64, offer: Option<DragOffer>, kind: String) {
        let pipe = offer.and_then(|offer| offer.receive(kind).ok());
        let reading = pipe.and_then(|pipe| {
            let (data, done) = (File::from(OwnedFd::from(pipe)), self.drag_done.clone());
            let read = move || {
                let mut bytes = Vec::new();
                let read = data.take(MOST_READ + 1).read_to_end(&mut bytes);
                let whole = read.is_ok() && bytes.len() as u64 <= MOST_READ;
                let _ = done.send(Command::DragRead(number, whole.then_some(bytes)));
            };
            std::thread::Builder::new().name("smithay-clipboard-drag".into()).spawn(read).ok()
        });
        if reading.is_none() {
            self.drag_read(number, None);
        }
    }
}

/// Where reading threads hand in what they read: the clipboard's own channel of commands.
pub(crate) type DragDone = Sender<Command>;
