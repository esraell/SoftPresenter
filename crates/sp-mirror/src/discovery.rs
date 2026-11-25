//! Being found: the two DNS-SD records a sender looks for, and announcing them.
//!
//! The records are built by plain functions, so they can be checked without touching the
//! network; only [`announce`] opens sockets, and it runs only when the program was started with
//! `--announce`.
//!
//! The records' fields follow UxPlay's `lib/dnssd.c`, `lib/dnssdint.h` and `lib/dns_sd/dns_sd.c`
//! (copyright (C) 2011-2012 Juho Vähä-Herttua, (C) 2021-2026 F. Duncanh; LGPL-2.1-or-later).

/// The model the receiver says it is. A name of our own: SoftPresenter is not an Apple TV.
pub const MODEL: &str = "SoftPresenter1,1";
/// The version of the protocol's reference source that this receiver follows.
pub const SOURCE_VERSION: &str = "220.68";
/// The same for every receiver that follows UxPlay; senders do not seem to look at it.
pub const PI: &str = "2e388006-13ba-4041-9a67-25dd4a43d536";

/// The first word of the features: UxPlay's `0x5A7FFEE6` with bit 27 (legacy pairing) off, so a
/// sender connects at once instead of pairing first. Bit 7 is screen mirroring; bit 9 (audio)
/// stays on though no sound is played yet, or senders leave after a minute.
pub const FEATURES_1: u32 = 0x527F_FEE6;
/// The second word: bit 42, several video codecs (a sender may then send HEVC).
pub const FEATURES_2: u32 = 0x400;
/// Without HEVC.
pub const FEATURES_2_H264_ONLY: u32 = 0;
/// Both words as the one number `GET /info` gives.
pub const fn features(second: u32) -> u64 {
    ((second as u64) << 32) | FEATURES_1 as u64
}

/// The longest name a receiver can announce: the `_raop._tcp` record's instance is the device
/// id, an `@` and the name, 63 bytes at most.
pub const MAX_NAME_BYTES: usize = 50;

/// The name cut to what can be announced, on a character's edge.
pub fn announced_name(name: &str) -> &str {
    let mut end = name.len().min(MAX_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].trim_end()
}

/// A device id is six bytes in hex with colons: `AA:BB:CC:DD:EE:FF`.
pub fn valid_device_id(id: &str) -> bool {
    id.len() == 17
        && id.bytes().enumerate().all(|(at, byte)| if at % 3 == 2 { byte == b':' } else { byte.is_ascii_hexdigit() })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What goes in the records.
pub struct Record<'a> {
    pub name: &'a str,
    pub device_id: &'a str,
    pub public_key: &'a [u8; 32],
    pub model: &'a str,
    pub features_2: u32,
}

impl Record<'_> {
    fn features(&self) -> String {
        format!("0x{FEATURES_1:X},0x{:X}", self.features_2)
    }

    /// The `_airplay._tcp` record's fields.
    pub fn airplay_txt(&self) -> Vec<(&'static str, String)> {
        vec![
            ("deviceid", self.device_id.to_owned()),
            ("features", self.features()),
            ("pw", "false".to_owned()),
            ("flags", "0x4".to_owned()),
            ("model", self.model.to_owned()),
            ("pk", hex(self.public_key)),
            ("pi", PI.to_owned()),
            ("srcvers", SOURCE_VERSION.to_owned()),
            ("vv", "2".to_owned()),
        ]
    }

    /// The `_raop._tcp` record's fields.
    pub fn raop_txt(&self) -> Vec<(&'static str, String)> {
        vec![
            ("ch", "2".to_owned()),
            ("cn", "0,1,2,3".to_owned()),
            ("da", "true".to_owned()),
            ("et", "0,3,5".to_owned()),
            ("vv", "2".to_owned()),
            ("ft", self.features()),
            ("am", self.model.to_owned()),
            ("md", "0,1,2".to_owned()),
            ("rhd", "5.6.0.0".to_owned()),
            ("pw", "false".to_owned()),
            ("sf", "0x4".to_owned()),
            ("sr", "44100".to_owned()),
            ("ss", "16".to_owned()),
            ("sv", "false".to_owned()),
            ("tp", "UDP".to_owned()),
            ("txtvers", "1".to_owned()),
            ("vs", SOURCE_VERSION.to_owned()),
            ("vn", "65537".to_owned()),
            ("pk", hex(self.public_key)),
        ]
    }

    /// The `_raop._tcp` record's instance: the device id without its colons, `@`, the name.
    pub fn raop_instance(&self) -> String {
        format!("{}@{}", self.device_id.replace(':', ""), self.name)
    }

    /// The `_airplay._tcp` fields as the bytes of a TXT record (each a length and `key=value`),
    /// which the first `GET /info` of a connection asks for.
    pub fn airplay_txt_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (key, value) in self.airplay_txt() {
            let entry = format!("{key}={value}");
            // Every entry here is far under 255 bytes.
            out.push(entry.len().min(255) as u8);
            out.extend_from_slice(&entry.as_bytes()[..entry.len().min(255)]);
        }
        out
    }
}

/// Announces the receiver on the local network until the value returned is dropped. This is
/// the only place that sends anything nobody asked for; it is not called without `--announce`.
pub fn announce(record: &Record<'_>, port: u16) -> Result<Announced, String> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|error| error.to_string())?;
    let host = format!("softpresenter-{}.local.", record.device_id.replace(':', "").to_lowercase());
    let register = |kind: &str, instance: &str, txt: Vec<(&'static str, String)>| -> Result<(), String> {
        let info = mdns_sd::ServiceInfo::new(kind, instance, &host, "", port, &txt[..])
            .map_err(|error| error.to_string())?
            .enable_addr_auto();
        daemon.register(info).map_err(|error| error.to_string())
    };
    register("_airplay._tcp.local.", record.name, record.airplay_txt())?;
    register("_raop._tcp.local.", &record.raop_instance(), record.raop_txt())?;
    Ok(Announced { daemon })
}

/// The announcement; dropping it takes the records back.
pub struct Announced {
    daemon: mdns_sd::ServiceDaemon,
}

impl Drop for Announced {
    fn drop(&mut self) {
        if let Ok(done) = self.daemon.shutdown() {
            let _ = done.recv_timeout(std::time::Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record<'static> {
        Record {
            name: "SoftPresenter \u{2014} Front Mac",
            device_id: "AA:BB:CC:DD:EE:FF",
            public_key: &[0xab; 32],
            model: MODEL,
            features_2: FEATURES_2,
        }
    }

    #[test]
    fn the_features_say_mirroring_and_audio_but_not_pairing() {
        assert_eq!(FEATURES_1 & (1 << 7), 1 << 7);
        assert_eq!(FEATURES_1 & (1 << 9), 1 << 9);
        assert_eq!(FEATURES_1 & (1 << 27), 0);
        assert_eq!(features(FEATURES_2), 0x0000_0400_527F_FEE6);
        assert_eq!(features(FEATURES_2) >> 42 & 1, 1);
        assert_eq!(features(FEATURES_2_H264_ONLY), 0x527F_FEE6);
    }

    #[test]
    fn the_records_carry_what_a_sender_reads() {
        let record = record();
        let airplay = record.airplay_txt();
        let get = |key: &str| airplay.iter().find(|(k, _)| *k == key).unwrap().1.clone();
        assert_eq!(get("deviceid"), "AA:BB:CC:DD:EE:FF");
        assert_eq!(get("features"), "0x527FFEE6,0x400");
        assert_eq!(get("model"), "SoftPresenter1,1");
        assert_eq!(get("pw"), "false");
        assert_eq!(get("pk"), "ab".repeat(32));
        let raop = record.raop_txt();
        assert!(raop.contains(&("ft", "0x527FFEE6,0x400".to_owned())));
        assert!(raop.contains(&("am", "SoftPresenter1,1".to_owned())));
        assert_eq!(record.raop_instance(), "AABBCCDDEEFF@SoftPresenter \u{2014} Front Mac");
        assert!(record.raop_instance().len() <= 63);
        let bytes = record.airplay_txt_bytes();
        assert_eq!(bytes[0] as usize, "deviceid=AA:BB:CC:DD:EE:FF".len());
        assert_eq!(&bytes[1..27], b"deviceid=AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn a_long_name_is_cut_on_a_character_and_a_device_id_is_checked() {
        let long = "\u{2014}".repeat(40);
        let cut = announced_name(&long);
        assert!(cut.len() <= MAX_NAME_BYTES && cut.chars().all(|c| c == '\u{2014}'));
        assert_eq!(announced_name("Front Mac"), "Front Mac");
        assert!(valid_device_id("AA:BB:CC:DD:EE:0f"));
        for bad in
            ["", "AA:BB:CC:DD:EE", "AA:BB:CC:DD:EE:FG", "AABBCCDDEEFF", "AA:BB:CC:DD:EE:FF:00", "AA-BB-CC-DD-EE-FF"]
        {
            assert!(!valid_device_id(bad), "{bad}");
        }
    }
}
