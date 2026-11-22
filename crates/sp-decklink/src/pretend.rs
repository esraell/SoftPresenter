//! Testing aid: SOFTPRESENTER_PRETEND_DECKLINK=1 lists made-up DeckLink devices in place of
//! the real ones, so the settings, the command line and the whole way a picture travels can be
//! tried on a computer without a card. Their names start "Pretend", so they are never taken
//! for a real card, and nothing they are sent leaves the computer.

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

/// Four outputs as a four-connector card shows them, and one more that can key.
pub fn devices() -> Vec<Device> {
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
    });
    devices
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
        assert_eq!(devices[0].default_mode().map(|mode| mode.name.as_str()), Some("1080i59.94"));
        assert!(devices.iter().filter(|device| device.external_keying).count() == 1);
        for device in &devices {
            for mode in device.offered() {
                assert_eq!(mode.name, mode.driver_name, "the names given match the names worked out");
            }
        }
    }
}
