//! Testing aid: SOFTPRESENTER_PRETEND_DECKLINK=1 lists made-up DeckLink devices in place of
//! the real ones, so the settings, the command line and the whole way a picture travels can be
//! tried on a computer without a card. Their names start "Pretend", so they are never taken
//! for a real card, and nothing they are sent leaves the computer.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::convert::Layout;
use crate::play::{Opened, Wanted, choose};
use crate::player::{Card, Finished, Halt};
use crate::{Device, Mode};

/// True when the testing switch asks for pretend devices.
pub fn active() -> bool {
    // Testing aid: list made-up DeckLink devices (see this module).
    std::env::var_os("SOFTPRESENTER_PRETEND_DECKLINK").is_some_and(|value| !value.is_empty() && value != "0")
}

/// The persistent ids pretend devices have: far from any real card's.
pub const FIRST_ID: i64 = 0x5EED_0000;

/// The modes a pretend device has: an HD card's, and a standard-definition one SoftPresenter
/// doesn't offer.
fn modes(keying: bool) -> Vec<Mode> {
    let code = |name: &[u8; 4]| u32::from_be_bytes(*name);
    let progressive = u32::from_be_bytes(*b"prog");
    let interlaced = u32::from_be_bytes(*b"uppr");
    let sd = Mode::new(
        code(b"ntsc"),
        "NTSC".to_owned(),
        (720, 486),
        (1001, 30000),
        u32::from_be_bytes(*b"lowr"),
        (true, true, keying),
    );
    let mut modes = vec![sd];
    for (four, name, height, interlace, duration, scale) in [
        (b"hp50", "720p50", 720, false, 1000, 50000),
        (b"hp59", "720p59.94", 720, false, 1001, 60000),
        (b"hp60", "720p60", 720, false, 1000, 60000),
        (b"Hi50", "1080i50", 1080, true, 1000, 25000),
        (b"Hi59", "1080i59.94", 1080, true, 1001, 30000),
        (b"Hi60", "1080i60", 1080, true, 1000, 30000),
        (b"23ps", "1080p23.98", 1080, false, 1001, 24000),
        (b"24ps", "1080p24", 1080, false, 1000, 24000),
        (b"Hp25", "1080p25", 1080, false, 1000, 25000),
        (b"Hp29", "1080p29.97", 1080, false, 1001, 30000),
        (b"Hp30", "1080p30", 1080, false, 1000, 30000),
        (b"Hp50", "1080p50", 1080, false, 1000, 50000),
        (b"Hp59", "1080p59.94", 1080, false, 1001, 60000),
        (b"Hp60", "1080p60", 1080, false, 1000, 60000),
    ] {
        let width = if height == 720 { 1280 } else { 1920 };
        let dominance = if interlace { interlaced } else { progressive };
        modes.push(Mode::new(
            code(four),
            name.to_owned(),
            (width, height),
            (duration, scale),
            dominance,
            (true, true, keying),
        ));
    }
    modes
}

/// Four outputs as a four-connector card shows them, and one more that can key. Their own
/// default formats, as Desktop Video Setup would set them, cover each way a new output's
/// format is chosen: the first is set to 1080p30, which a new output starts in; the second to
/// NTSC, which SoftPresenter doesn't offer; the third to none; the fourth doesn't say, so a new
/// output on it starts in 1080i59.94; and the keyer is set to 1080p29.97, as for a switcher.
pub fn devices() -> Vec<Device> {
    let card_modes = [Some(*b"Hp30"), Some(*b"ntsc"), Some(*b"iunk"), None];
    let mut devices: Vec<Device> = (0..4)
        .map(|at| Device {
            name: format!("Pretend DeckLink ({})", at + 1),
            model: "Pretend DeckLink".to_owned(),
            id: Some(FIRST_ID + 1 + at),
            sub_device: Some(at as u32),
            sub_devices: Some(4),
            profile: u32::from_be_bytes(*b"4dhd"),
            duplex: u32::from_be_bytes(*b"dxha"),
            connections: 1,
            can_play: true,
            internal_keying: false,
            external_keying: false,
            modes: modes(false),
            card_mode: card_modes[at as usize].map(|code| i64::from(u32::from_be_bytes(code))),
            card_mode_3d: false,
        })
        .collect();
    devices.push(Device {
        name: "Pretend DeckLink Keyer".to_owned(),
        model: "Pretend DeckLink".to_owned(),
        id: Some(FIRST_ID + 0x10),
        sub_device: Some(0),
        sub_devices: Some(1),
        profile: u32::from_be_bytes(*b"1dfd"),
        duplex: u32::from_be_bytes(*b"dxfu"),
        connections: 1,
        can_play: true,
        internal_keying: true,
        external_keying: true,
        modes: modes(true),
        card_mode: Some(i64::from(u32::from_be_bytes(*b"Hp29"))),
        card_mode_3d: false,
    });
    devices
}

/// Opens pretend devices, for an output started while the testing switch is on.
pub struct PretendOpener;

impl crate::play::Opener for PretendOpener {
    type Card = SimCard;

    fn open(&mut self, wanted: &Wanted) -> Result<Opened<SimCard>, String> {
        let devices = devices();
        let device = devices
            .iter()
            .find(|device| device.is(wanted.id, &wanted.device))
            .ok_or_else(|| format!("{} isn't connected.", wanted.device))?;
        let (mode, format) = choose(device, wanted)?;
        let layout = Layout { width: mode.width, height: mode.height, format, row_bytes: format.row_bytes(mode.width) };
        let card = SimCard::new();
        // Testing aid: SOFTPRESENTER_PRETEND_DECKLINK_STALL=<seconds> has each pretend card stop
        // giving frames back that long after it starts, as a card taken away does, so the
        // output's opening it again and the operator's warning can be seen.
        card.sim().stall_after = std::env::var("SOFTPRESENTER_PRETEND_DECKLINK_STALL")
            .ok()
            .and_then(|seconds| seconds.trim().parse::<f64>().ok())
            .filter(|seconds| *seconds > 0.0)
            .map(Duration::from_secs_f64);
        Ok(Opened { card, device: device.name.clone(), mode, layout, keyed: wanted.keying })
    }
}

/// A card that only keeps time: it gives each frame back once the time it was to be shown for
/// is over, on the computer's clock, as a real card does on its own. What it is sent goes
/// nowhere. Tests can make it stall or change profile.
pub struct SimCard {
    sim: Arc<Mutex<Sim>>,
}

/// A way to look at and steer a [`SimCard`] from elsewhere, in the tests.
#[cfg(test)]
#[derive(Clone)]
pub struct SimControl(Arc<Mutex<Sim>>);

#[derive(Default)]
struct Sim {
    /// When the clock started, and the time it started from.
    started: Option<(Instant, i64)>,
    scale: i64,
    /// Frames held: key, when to show, for how long, and whether it was late when scheduled.
    held: Vec<(usize, i64, i64, bool)>,
    next_key: usize,
    /// The last frame written: its length and first bytes.
    written: Option<(usize, Vec<u8>)>,
    stalled: bool,
    /// How long after it starts the card stops giving frames back (the testing switch).
    stall_after: Option<Duration>,
    profile: bool,
    closed: bool,
}

impl Sim {
    fn clock(&self) -> Option<i64> {
        let (since, at) = self.started?;
        Some(at + (since.elapsed().as_secs_f64() * self.scale as f64) as i64)
    }
}

impl SimCard {
    pub fn new() -> SimCard {
        SimCard { sim: Arc::new(Mutex::new(Sim::default())) }
    }

    #[cfg(test)]
    pub fn control(&self) -> SimControl {
        SimControl(self.sim.clone())
    }

    fn sim(&self) -> std::sync::MutexGuard<'_, Sim> {
        self.sim.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for SimCard {
    fn default() -> SimCard {
        SimCard::new()
    }
}

impl Drop for SimCard {
    fn drop(&mut self) {
        self.sim().closed = true;
    }
}

#[cfg(test)]
impl SimControl {
    fn sim(&self) -> std::sync::MutexGuard<'_, Sim> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The length and first sixteen bytes of the last frame written.
    pub fn last_written(&self) -> Option<(usize, Vec<u8>)> {
        self.sim().written.clone()
    }

    /// From now on the card gives no frame back, as a card that has stopped.
    pub fn stall(&self) {
        self.sim().stalled = true;
    }

    /// The card says its profile is changing.
    pub fn change_profile(&self) {
        self.sim().profile = true;
    }

    /// The card has been let go.
    pub fn closed(&self) -> bool {
        self.sim().closed
    }
}

impl Card for SimCard {
    type Frame = usize;

    fn new_frame(&mut self) -> Result<usize, String> {
        let mut sim = self.sim();
        sim.next_key += 1;
        Ok(sim.next_key)
    }

    fn write(&mut self, _frame: &mut usize, bytes: &[u8]) -> Result<(), String> {
        self.sim().written = Some((bytes.len(), bytes[..bytes.len().min(16)].to_vec()));
        Ok(())
    }

    fn key(frame: &usize) -> usize {
        *frame
    }

    fn schedule(&mut self, frame: &usize, at: i64, duration: i64, _scale: i64) -> Result<(), String> {
        let mut sim = self.sim();
        let late = sim.clock().is_some_and(|now| at < now);
        sim.held.push((*frame, at, duration, late));
        Ok(())
    }

    fn start(&mut self, at: i64, scale: i64) -> Result<(), String> {
        let mut sim = self.sim();
        sim.started = Some((Instant::now(), at));
        sim.scale = scale.max(1);
        Ok(())
    }

    fn stream_time(&mut self, _scale: i64) -> Option<i64> {
        self.sim().clock()
    }

    fn wait(&mut self, timeout: Duration) -> (Vec<(usize, Finished)>, Halt) {
        let until = Instant::now() + timeout;
        loop {
            let pause = {
                let mut sim = self.sim();
                if std::mem::take(&mut sim.profile) {
                    return (Vec::new(), Halt { stopped: false, profile: true });
                }
                let stalled = sim.stalled
                    || sim.stall_after.zip(sim.started).is_some_and(|(after, (since, _))| since.elapsed() >= after);
                if let (Some(now), false) = (sim.clock(), stalled) {
                    let done: Vec<(usize, Finished)> = sim
                        .held
                        .iter()
                        .filter(|(_, at, duration, _)| at + duration <= now)
                        .map(|(key, _, _, late)| (*key, if *late { Finished::Late } else { Finished::Shown }))
                        .collect();
                    if !done.is_empty() {
                        sim.held.retain(|(key, ..)| !done.iter().any(|(gone, _)| gone == key));
                        return (done, Halt::default());
                    }
                    let scale = sim.scale as f64;
                    sim.held
                        .iter()
                        .map(|(_, at, duration, _)| {
                            Duration::from_secs_f64(((at + duration - now).max(0) as f64 / scale).max(0.0))
                        })
                        .min()
                } else {
                    None
                }
            };
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return (Vec::new(), Halt::default());
            }
            std::thread::sleep(pause.map_or(left, |pause| pause.min(left)).max(Duration::from_micros(200)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretend_devices_offer_hd_modes_only_and_one_can_key() {
        let devices = devices();
        assert!(devices.iter().all(|device| device.name.starts_with("Pretend")));
        assert!(devices.iter().all(Device::usable));
        assert_eq!(devices[0].offered().count(), 14, "the standard-definition mode is left out");
        let starts: Vec<String> = devices.iter().map(|device| device.start(false).unwrap().in_words()).collect();
        assert_eq!(
            starts,
            [
                "1080p30, as Desktop Video Setup has it",
                "1080i59.94, the usual HD format: Desktop Video Setup has NTSC, which SoftPresenter doesn't offer",
                "1080i59.94, the usual HD format: Desktop Video Setup sets none",
                "1080i59.94, the usual HD format: Desktop Video Setup sets none",
                "1080p29.97, as Desktop Video Setup has it",
            ]
        );
        assert_eq!(devices[0].default_mode().map(|mode| mode.name.as_str()), Some("1080p30"));
        assert!(devices.iter().filter(|device| device.external_keying).count() == 1);
        for device in &devices {
            for mode in device.offered() {
                assert_eq!(mode.name, mode.driver_name, "the names given match the names worked out");
            }
        }
    }
}
