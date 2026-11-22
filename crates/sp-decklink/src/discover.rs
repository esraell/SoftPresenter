//! Finding the DeckLink devices, on a thread of its own: opening the driver can take a moment,
//! and neither the window nor the screens may wait for it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Driver, Found};

/// How often the driver is looked for again while it isn't installed, or can't be used.
const LOOK_AGAIN: Duration = Duration::from_secs(15);

/// How long after a device is plugged in or taken away it is listed: the driver tells of each
/// sub-device of a card one after another.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const SETTLE: Duration = Duration::from_millis(300);

/// The devices, kept up to date on their own thread. Dropping it lets the thread end.
pub struct Discovery {
    shared: Arc<Shared>,
    refresh: Sender<()>,
}

struct Shared {
    found: Mutex<Found>,
    /// Goes up each time what was found changes.
    changes: AtomicU64,
}

impl Discovery {
    /// Starts looking. `wake` is called, on the discovery thread, each time what was found
    /// changes (the first listing included); it should only pass the word on and return.
    pub fn start(wake: impl Fn() + Send + 'static) -> Discovery {
        let shared = Arc::new(Shared {
            found: Mutex::new(Found { driver: Driver::Looking, devices: Vec::new() }),
            changes: AtomicU64::new(0),
        });
        let (refresh, asked) = channel();
        let theirs = shared.clone();
        let started = std::thread::Builder::new()
            .name("decklink discovery".to_owned())
            .spawn(move || run(&theirs, &asked, &wake));
        if let Err(e) = started {
            set(
                &shared,
                Found {
                    driver: Driver::Failed(format!("Could not look for DeckLink devices: {e}")),
                    devices: Vec::new(),
                },
            );
        }
        Discovery { shared, refresh }
    }

    /// What was found, as last listed. Never waits on the driver.
    pub fn found(&self) -> Found {
        self.shared
            .found
            .lock()
            .map(|found| found.clone())
            .unwrap_or(Found { driver: Driver::Looking, devices: Vec::new() })
    }

    /// How many times what was found has changed: look again when this moves on.
    pub fn changes(&self) -> u64 {
        self.shared.changes.load(Ordering::Acquire)
    }

    /// Asks for the devices to be listed again soon (a card's profile may have changed, which
    /// the driver doesn't tell of). Never waits.
    pub fn refresh(&self) {
        let _ = self.refresh.send(());
    }
}

/// Keeps what was found, and says whether it changed.
fn set(shared: &Shared, now: Found) -> bool {
    let Ok(mut found) = shared.found.lock() else { return false };
    if *found == now {
        return false;
    }
    *found = now;
    shared.changes.fetch_add(1, Ordering::AcqRel);
    true
}

/// Waits for a refresh to be asked for, or `for_at_most`. False when the `Discovery` is gone.
fn wait_for_refresh(asked: &Receiver<()>, for_at_most: Duration) -> bool {
    match asked.recv_timeout(for_at_most) {
        Ok(()) | Err(RecvTimeoutError::Timeout) => {
            while asked.try_recv().is_ok() {}
            true
        }
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn run(shared: &Shared, asked: &Receiver<()>, wake: &dyn Fn()) {
    // Testing aid: SOFTPRESENTER_PRETEND_DECKLINK lists made-up devices instead (see `pretend`).
    if crate::pretend::active() {
        if set(shared, Found { driver: Driver::Ready("pretend".to_owned()), devices: crate::pretend::devices() }) {
            wake();
        }
        while wait_for_refresh(asked, Duration::from_secs(3600)) {}
        return;
    }
    loop {
        let found = list_now();
        let ready = matches!(found.driver, Driver::Ready(_));
        if set(shared, found) {
            wake();
        }
        if !ready {
            if !wait_for_refresh(asked, LOOK_AGAIN) {
                return;
            }
            continue;
        }
        if !watch(shared, asked, wake) {
            return;
        }
    }
}

/// With the driver open: lists the devices again each time one is plugged in or taken away, or
/// a refresh is asked for. False when the `Discovery` is gone; true when the driver should be
/// looked at afresh.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn watch(shared: &Shared, asked: &Receiver<()>, wake: &dyn Fn()) -> bool {
    let notices = crate::ffi::Discovery::start();
    let mut seen = notices.as_ref().map_or(0, |notices| notices.wait(u64::MAX, Duration::ZERO));
    loop {
        // A device plugged in or taken away, a refresh asked for, or the app closing.
        let changed = match &notices {
            Some(notices) => {
                let now = notices.wait(seen, Duration::from_millis(250));
                let changed = now != seen;
                seen = now;
                changed
            }
            None => {
                std::thread::sleep(Duration::from_millis(250));
                false
            }
        };
        let refresh = match asked.try_recv() {
            Ok(()) => true,
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
        };
        if !changed && !refresh {
            continue;
        }
        if changed {
            std::thread::sleep(SETTLE);
            if let Some(notices) = &notices {
                seen = notices.wait(seen, Duration::ZERO);
            }
        }
        while asked.try_recv().is_ok() {}
        let found = list_now();
        let ready = matches!(found.driver, Driver::Ready(_));
        if set(shared, found) {
            wake();
        }
        if !ready {
            return true;
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn watch(_shared: &Shared, asked: &Receiver<()>, _wake: &dyn Fn()) -> bool {
    wait_for_refresh(asked, LOOK_AGAIN)
}

/// Opens the driver, if it is installed, and lists its devices, on the caller's thread.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn list_now() -> Found {
    if crate::pretend::active() {
        return Found { driver: Driver::Ready("pretend".to_owned()), devices: crate::pretend::devices() };
    }
    let nothing = |driver| Found { driver, devices: Vec::new() };
    let Some(path) = crate::driver_path() else { return nothing(Driver::NotInstalled) };
    if let Err(why) = crate::ffi::load(&path) {
        return nothing(if why == crate::NOT_INSTALLED { Driver::NotInstalled } else { Driver::Failed(why) });
    }
    let version = crate::ffi::api_version();
    let words = version.map_or_else(|| "(version not known)".to_owned(), crate::version_in_words);
    // A driver older than the interfaces the bridge is built for has none of its devices'
    // outputs to give: say so, rather than list devices that can't play.
    if version.is_some_and(|version| version < crate::NEEDS_VERSION) {
        return nothing(Driver::TooOld(words));
    }
    match crate::ffi::list() {
        Some(devices) => Found { driver: Driver::Ready(words), devices },
        None => nothing(Driver::Failed("The DeckLink driver could not list its devices.".to_owned())),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn list_now() -> Found {
    if crate::pretend::active() {
        return Found { driver: Driver::Ready("pretend".to_owned()), devices: crate::pretend::devices() };
    }
    Found { driver: Driver::Unsupported, devices: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_found_counts_as_changed_only_when_it_differs() {
        let shared = Shared {
            found: Mutex::new(Found { driver: Driver::Looking, devices: Vec::new() }),
            changes: AtomicU64::new(0),
        };
        assert!(set(&shared, Found { driver: Driver::NotInstalled, devices: Vec::new() }));
        assert!(!set(&shared, Found { driver: Driver::NotInstalled, devices: Vec::new() }));
        assert_eq!(shared.changes.load(Ordering::Acquire), 1);
    }

    /// Runs against whatever driver this computer has: none, an old one, or one with no card
    /// (this crate's own Mac has Desktop Video 15.3.1 and no card). Whatever it is, the listing
    /// says so plainly and offers no output it can't play.
    #[test]
    fn listing_the_real_driver_says_how_it_stands_and_offers_nothing_unplayable() {
        let found = list_now();
        match &found.driver {
            Driver::Ready(_) => assert!(found.outputs().all(|device| device.offered().next().is_some())),
            Driver::NotInstalled | Driver::TooOld(_) | Driver::Failed(_) | Driver::Unsupported => {
                assert!(found.devices.is_empty(), "{found:?}");
            }
            Driver::Looking => panic!("a listing is never still looking"),
        }
        assert!(!found.driver.in_words().is_empty());
    }

    /// With the driver open, whatever its version, asking it for its devices never fails or
    /// crashes: this crate's own Mac, with Desktop Video 15.3.1 and no card, lists none.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_driver_that_is_open_lists_its_devices_whatever_its_version() {
        if !matches!(list_now().driver, Driver::Ready(_) | Driver::TooOld(_)) || crate::pretend::active() {
            return;
        }
        let devices = crate::ffi::list().expect("the driver is open");
        if matches!(list_now().driver, Driver::TooOld(_)) {
            assert!(devices.iter().all(|device| !device.can_play), "an older driver offers no SDK 16 output");
        }
    }

    #[test]
    fn discovery_starts_without_waiting_and_is_woken_with_the_first_listing() {
        let (woken, wait) = channel();
        let started = std::time::Instant::now();
        let discovery = Discovery::start(move || {
            let _ = woken.send(());
        });
        assert!(started.elapsed() < Duration::from_millis(50), "start returns at once");
        wait.recv_timeout(Duration::from_secs(10)).expect("told of the first listing");
        assert_ne!(discovery.found().driver, Driver::Looking);
        assert!(discovery.changes() >= 1);
        discovery.refresh();
    }
}
