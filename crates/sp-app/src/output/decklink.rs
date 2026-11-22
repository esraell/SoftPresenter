//! Screens played out of DeckLink cards, on the screens' thread. A DeckLink output is given the
//! same pictures as an NDI source that is always watched: drawn at 1920 x 1080 with the other
//! NDI pictures and read back from the GPU once (`send_ndi`, `finish_ndi`), then handed to the
//! output's own thread (`sp_decklink::DeckLinkOutput`), which turns them into the card's frames
//! and plays them on the card's clock. Handing them over never waits, so the card can never
//! hold up the screens. As over NDI, a new slide goes at once, without a dissolve.

use std::sync::{Arc, Mutex};

use sp_decklink::{Alpha, DeckLinkOutput, Wanted};
use sp_model::Uuid;

use super::{Output, OutputHandle, Waker};

/// Each screen's DeckLink output as the window sees it: started, or refused before it could.
#[derive(Clone)]
pub(super) enum Entry {
    Started(sp_decklink::Watch),
    Refused(String),
}

pub(super) type Statuses = Arc<Mutex<Vec<(Uuid, Entry)>>>;

/// What a screen's DeckLink output is doing, for the operator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeckLinkStatus {
    /// Not asked for.
    Off,
    /// Opening the device.
    Starting,
    /// Playing out: the mode and the keyer, in words.
    Playing(String),
    /// Not playing, and trying again: why, in plain words.
    Waiting(String),
}

impl OutputHandle {
    /// What a screen's DeckLink output is doing now. Never waits.
    pub fn decklink_status(&self, screen: Uuid) -> DeckLinkStatus {
        let entry = self
            .decklink_status
            .lock()
            .ok()
            .and_then(|all| all.iter().find(|(id, _)| *id == screen).map(|(_, entry)| entry.clone()));
        match entry {
            None => DeckLinkStatus::Off,
            Some(Entry::Refused(why)) => DeckLinkStatus::Waiting(why),
            Some(Entry::Started(watch)) => match watch.state() {
                sp_decklink::State::Starting => DeckLinkStatus::Starting,
                sp_decklink::State::Playing(what) => DeckLinkStatus::Playing(what),
                sp_decklink::State::Waiting(why) => DeckLinkStatus::Waiting(why),
            },
        }
    }
}

/// What an output's thread says goes to the app's log, and wakes the window, which shows what
/// each screen is doing.
fn log_to_app(waker: Option<Waker>) -> sp_decklink::Log {
    Arc::new(move |line: &str, problem: bool| {
        if problem {
            crate::applog::problem("decklink", line);
        } else {
            crate::applog::event("decklink", line);
        }
        if let Some(wake) = &waker {
            wake();
        }
    })
}

/// How a DeckLink output is to read the alpha of a picture read back for NDI.
fn alpha_of(sent: sp_ndi::Sent) -> Alpha {
    match sent {
        sp_ndi::Sent::Opaque => Alpha::Opaque,
        sp_ndi::Sent::Straight => Alpha::Straight,
        sp_ndi::Sent::Premultiplied => Alpha::Premultiplied,
        sp_ndi::Sent::Key => Alpha::Key,
    }
}

impl Output {
    /// Starts, changes or stops a screen's DeckLink output, and records how that went for the
    /// operator. A change goes to the output already running, whose own thread closes the
    /// device and opens it again, so it never finds the device busy with itself.
    pub(super) fn set_decklink(&mut self, id: Uuid, wanted: Option<Wanted>) {
        let entry = match wanted {
            Some(wanted) => match self.decklink.iter().find(|(have, _)| *have == id) {
                Some((_, output)) => {
                    output.change(wanted);
                    Some(Entry::Started(output.watch()))
                }
                None => match DeckLinkOutput::start(wanted, log_to_app(self.waker.clone())) {
                    Ok(output) => {
                        let entry = Entry::Started(output.watch());
                        self.decklink.push((id, output));
                        Some(entry)
                    }
                    Err(why) => Some(Entry::Refused(why)),
                },
            },
            None => {
                self.decklink.retain(|(have, _)| *have != id);
                None
            }
        };
        if self.ndi.is_empty() && self.decklink.is_empty() {
            self.forget_readbacks();
        }
        self.ndi_drawn_for.retain(|(have, _)| *have != id);
        // A new output gets the picture on screen now.
        if !self.decklink.is_empty() {
            let frame = self.frame.clone();
            self.send_ndi(&frame, super::NdiChange::Nothing);
        }
        if let Ok(mut shared) = self.decklink_status.lock() {
            shared.retain(|(have, _)| *have != id);
            shared.extend(entry.map(|entry| (id, entry)));
        }
    }

    /// The DeckLink outputs a picture read back is for, each with how to read its alpha: as
    /// `sent` says, or as its key for those of them that are `keys` (made from these same
    /// pixels, so the key never shows another moment than its fill).
    pub(super) fn decklink_for(
        &self,
        ids: &[Uuid],
        keys: &[Uuid],
        sent: sp_ndi::Sent,
    ) -> Vec<(&DeckLinkOutput, Alpha)> {
        self.decklink
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(id, output)| (output, if keys.contains(id) { Alpha::Key } else { alpha_of(sent) }))
            .collect()
    }
}
