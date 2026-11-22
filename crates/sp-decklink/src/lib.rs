//! Plays a screen's picture out of a Blackmagic Design DeckLink card (SDI or HDMI), for
//! SoftPresenter for Blackmagic Design DeckLink.
//!
//! DeckLink is a trademark of Blackmagic Design Pty. Ltd. The DeckLink driver (Desktop Video)
//! is not part of SoftPresenter and is not needed to build it or to start it: it is installed
//! by the person using SoftPresenter, looked for while the app runs, off the threads that draw
//! the screens and the window, and without it there are simply no DeckLink outputs to choose.
//! The SDK's headers it is compiled with are in `third_party/decklink-sdk`, copied unchanged;
//! see `docs/licences/decklink.md`.
//!
//! - [`Discovery`] lists the devices (each output a card's driver offers, such as each of a
//!   DeckLink Duo 2's four), the ways each can play out, and whether it can key, on its own
//!   thread, and lists them again when a device is plugged in or taken away.
//!
//! On Windows the crate is built without its bridge to the driver (Windows' DeckLink
//! interface needs Microsoft's MIDL compiler), and behaves as on a computer without the
//! driver.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unsafe_code)]
mod ffi;

pub mod check;
mod discover;
mod pretend;

use std::path::PathBuf;

pub use discover::Discovery;

/// What the operator is told when the driver can't be found.
pub const NOT_INSTALLED: &str = "Desktop Video (Blackmagic's DeckLink driver) isn't installed on this computer.";

/// Where Desktop Video can be downloaded, shown where DeckLink is offered.
pub const DOWNLOAD_URL: &str = "https://www.blackmagicdesign.com/support/family/capture-and-playback";

/// The oldest driver SoftPresenter works with: Desktop Video 16.0, whose interfaces the bridge
/// is compiled against (BMDDeckLinkAPIVersion's form: one byte each for the major, minor and
/// sub version, then a zero byte).
pub const NEEDS_VERSION: u32 = 0x1000_0000;

/// `BMDFieldDominance` values, from `DeckLinkAPIModes.h`.
const LOWER_FIELD_FIRST: u32 = u32::from_be_bytes(*b"lowr");
const UPPER_FIELD_FIRST: u32 = u32::from_be_bytes(*b"uppr");
const PROGRESSIVE: u32 = u32::from_be_bytes(*b"prog");

/// `bmdDuplexInactive`: a sub-device its card's profile leaves unused.
const DUPLEX_INACTIVE: u32 = u32::from_be_bytes(*b"dxin");

/// How the driver stands on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Driver {
    /// Still being looked for.
    Looking,
    /// Not installed (or hidden by the testing switch).
    NotInstalled,
    /// Installed, but older than SoftPresenter needs; its version.
    TooOld(String),
    /// Installed, but it could not be opened; why, in plain words.
    Failed(String),
    /// Not on this kind of computer yet (Windows).
    Unsupported,
    /// Open and listing devices; its version.
    Ready(String),
}

impl Driver {
    /// What the operator is told of it, in plain words.
    pub fn in_words(&self) -> String {
        match self {
            Driver::Looking => "Looking for DeckLink devices…".to_owned(),
            Driver::NotInstalled => NOT_INSTALLED.to_owned(),
            Driver::TooOld(version) => format!(
                "Desktop Video {version} is installed; SoftPresenter needs 16.0 or later. Install the latest from Blackmagic Design."
            ),
            Driver::Failed(why) => why.clone(),
            Driver::Unsupported => "DeckLink output isn't available on this kind of computer yet.".to_owned(),
            Driver::Ready(version) if version == "pretend" => {
                "Pretend devices (the testing switch SOFTPRESENTER_PRETEND_DECKLINK)".to_owned()
            }
            Driver::Ready(version) => format!("Desktop Video {version}"),
        }
    }
}

/// What a listing found: how the driver stands, and the devices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub driver: Driver,
    pub devices: Vec<Device>,
}

impl Found {
    /// The devices that can play out now, in the order the driver lists them.
    pub fn outputs(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| device.usable())
    }

    /// The device with this persistent id, or with no id, this name. Never another device of
    /// the same name: an output saved for one card is never moved to another.
    pub fn find(&self, id: Option<i64>, name: &str) -> Option<&Device> {
        self.devices.iter().find(|device| device.is(id, name))
    }
}

/// One output the driver offers: a card, or one of its sub-devices (a DeckLink Duo 2 has four,
/// one on each SDI connector, until its profile pairs them up).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// As the driver names it, "DeckLink Duo (1)": what the operator is shown.
    pub name: String,
    pub model: String,
    /// The driver's persistent id for it, which outlasts a restart.
    pub id: Option<i64>,
    /// Which of its card's sub-devices it is, from nought, and how many the card has.
    pub sub_device: Option<u32>,
    pub sub_devices: Option<u32>,
    /// The card's profile (`BMDProfileID`), 0 for a card with one way only.
    pub profile: u32,
    /// `BMDDuplexMode`, 0 when the driver doesn't say.
    pub duplex: u32,
    /// The connections it can play out on (`BMDVideoConnection` bits).
    pub connections: u32,
    pub can_play: bool,
    /// Its keyer can lay the pictures over its own input.
    pub internal_keying: bool,
    /// It can send the pictures' colours (fill) and alpha (key) on two connectors.
    pub external_keying: bool,
    /// Every way it can play out, as the driver lists them.
    pub modes: Vec<Mode>,
}

impl Device {
    /// True when this is the device with this id, or with no id, this name.
    pub fn is(&self, id: Option<i64>, name: &str) -> bool {
        match id {
            Some(id) => self.id == Some(id),
            None => self.name == name,
        }
    }

    /// True when it can play out now: its profile hasn't left it unused, and it has a way of
    /// playing out SoftPresenter offers.
    pub fn usable(&self) -> bool {
        self.can_play && self.duplex != DUPLEX_INACTIVE && self.offered().next().is_some()
    }

    /// The ways of playing out SoftPresenter offers: 720p and 1080 lines, interlaced or
    /// progressive, at the rates the device has.
    pub fn offered(&self) -> impl Iterator<Item = &Mode> {
        self.modes.iter().filter(|mode| mode.offered())
    }

    /// The offered mode of this name ("1080i59.94").
    pub fn mode(&self, name: &str) -> Option<&Mode> {
        self.offered().find(|mode| mode.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The mode a new output starts in: see [`default_mode`].
    pub fn default_mode(&self) -> Option<&Mode> {
        default_mode(&self.offered().collect::<Vec<_>>())
    }

    /// Whether it can key at all, in words: "external and internal keying", "no keying".
    pub fn keying_in_words(&self) -> &'static str {
        match (self.external_keying, self.internal_keying) {
            (true, true) => "external and internal keying",
            (true, false) => "external keying",
            (false, true) => "internal keying",
            (false, false) => "no keying",
        }
    }

    /// The connections it plays out on, in words: "SDI", "SDI, HDMI".
    pub fn connections_in_words(&self) -> String {
        const NAMES: [(u32, &str); 7] = [
            (1, "SDI"),
            (1 << 1, "HDMI"),
            (1 << 2, "optical SDI"),
            (1 << 3, "component"),
            (1 << 4, "composite"),
            (1 << 5, "S-Video"),
            (1 << 6, "Ethernet"),
        ];
        let names: Vec<&str> =
            NAMES.iter().filter(|(bit, _)| self.connections & bit != 0).map(|(_, name)| *name).collect();
        if names.is_empty() { "not said".to_owned() } else { names.join(", ") }
    }

    /// Its card's profile in words, where the card has more than one.
    pub fn profile_in_words(&self) -> Option<&'static str> {
        Some(match &self.profile.to_be_bytes() {
            b"1dfd" => "one sub-device, full duplex",
            b"1dhd" => "one sub-device, half duplex",
            b"2dfd" => "two sub-devices, full duplex",
            b"2dhd" => "two sub-devices, half duplex",
            b"4dhd" => "four sub-devices, half duplex",
            _ => return None,
        })
    }
}

/// One way a device can play out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    /// The driver's `BMDDisplayMode`.
    pub code: u32,
    /// SoftPresenter's short name for it, the same on every device: "1080i59.94".
    pub name: String,
    /// The driver's own name for it.
    pub driver_name: String,
    pub width: u32,
    pub height: u32,
    /// A frame lasts `frame_duration / time_scale` seconds (an interlaced frame is two fields).
    pub frame_duration: i64,
    pub time_scale: i64,
    pub field_dominance: u32,
    /// Whether 8-bit BGRA, and 8-bit YUV 4:2:2, can be played in it.
    pub bgra: bool,
    pub yuv: bool,
    /// Whether 8-bit BGRA can be keyed in it.
    pub keying: bool,
}

impl Mode {
    pub fn new(
        code: u32,
        driver_name: String,
        (width, height): (u32, u32),
        (frame_duration, time_scale): (i64, i64),
        field_dominance: u32,
        (bgra, yuv, keying): (bool, bool, bool),
    ) -> Mode {
        let interlaced = matches!(field_dominance, LOWER_FIELD_FIRST | UPPER_FIELD_FIRST);
        Mode {
            code,
            name: mode_name(height, interlaced, frame_duration, time_scale),
            driver_name,
            width,
            height,
            frame_duration,
            time_scale,
            field_dominance,
            bgra,
            yuv,
            keying,
        }
    }

    pub fn interlaced(&self) -> bool {
        matches!(self.field_dominance, LOWER_FIELD_FIRST | UPPER_FIELD_FIRST)
    }

    /// True for a mode SoftPresenter offers: 1280 x 720 or 1920 x 1080, interlaced or
    /// progressive (not segmented frames), at a rate it can say, in a format it can send.
    pub fn offered(&self) -> bool {
        matches!((self.width, self.height), (1280, 720) | (1920, 1080))
            && matches!(self.field_dominance, LOWER_FIELD_FIRST | UPPER_FIELD_FIRST | PROGRESSIVE)
            && self.frame_duration > 0
            && self.time_scale > 0
            && (self.bgra || self.yuv)
    }

    /// How long one frame lasts.
    pub fn frame_time(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.frame_duration as f64 / self.time_scale.max(1) as f64)
    }
}

/// SoftPresenter's name for a mode: its lines, `i` or `p`, and its rate, the field rate for an
/// interlaced mode as is usual ("1080i59.94", "720p50", "1080p23.98").
pub fn mode_name(height: u32, interlaced: bool, frame_duration: i64, time_scale: i64) -> String {
    let frames = if frame_duration > 0 { time_scale as f64 / frame_duration as f64 } else { 0.0 };
    let rate = if interlaced { frames * 2.0 } else { frames };
    let rate =
        if (rate - rate.round()).abs() < 0.005 { format!("{}", rate.round() as i64) } else { format!("{rate:.2}") };
    format!("{height}{}{rate}", if interlaced { "i" } else { "p" })
}

/// The mode a new DeckLink output starts in, among the modes offered: 1080i59.94, the usual HD
/// format for SDI in countries with 60 Hz power, which every HD switcher and recorder takes and
/// which needs only a 1.5G link; else 1080i50, its counterpart where power is 50 Hz; else the
/// first 1080-line mode, as SoftPresenter draws every screen at 1920 x 1080; else the first.
/// The operator picks the mode their switcher or projector is set to.
pub fn default_mode<'m>(modes: &[&'m Mode]) -> Option<&'m Mode> {
    ["1080i59.94", "1080i50"]
        .iter()
        .find_map(|name| modes.iter().find(|mode| mode.name == *name))
        .or_else(|| modes.iter().find(|mode| mode.height == 1080))
        .or_else(|| modes.first())
        .copied()
}

/// Whether the screens' thread may hand pictures over with alpha: how the alpha is to be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alpha {
    /// Solid: the fourth byte of each pixel is not looked at.
    #[default]
    Opaque,
    /// See-through, the colours as they are (not multiplied by the alpha), as the renderer
    /// leaves them.
    Straight,
    /// See-through, each colour to be multiplied by its alpha before it goes, for a keyer that
    /// expects that.
    Premultiplied,
}

/// How an output uses its card's keyer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Keying {
    /// Not at all: the pictures go solid.
    #[default]
    Off,
    /// The card sends the pictures' colours (the fill) and their alpha (the key) on two
    /// connectors, for a switcher's keyer.
    External,
    /// The card lays the pictures over the video coming into it, by their alpha.
    Internal,
}

impl Keying {
    pub fn name(self) -> &'static str {
        match self {
            Keying::Off => "off",
            Keying::External => "external",
            Keying::Internal => "internal",
        }
    }

    pub fn parse(word: &str) -> Option<Keying> {
        match word.trim().to_lowercase().as_str() {
            "off" | "none" | "no" => Some(Keying::Off),
            "external" | "fill-and-key" | "fill+key" => Some(Keying::External),
            "internal" => Some(Keying::Internal),
            _ => None,
        }
    }
}

/// True when the testing switch SOFTPRESENTER_HIDE_DECKLINK asks to behave as on a computer
/// without the driver.
fn hidden() -> bool {
    // Testing aid: behave as on a computer without Desktop Video.
    std::env::var_os("SOFTPRESENTER_HIDE_DECKLINK").is_some()
}

/// Where the driver is on this computer, if it is: only looks for its file, so it can be
/// asked from anywhere.
pub fn driver_path() -> Option<PathBuf> {
    if hidden() {
        return None;
    }
    driver_places().into_iter().find(|path| path.exists())
}

/// Whether the driver is on this computer (a quick look for its file), or pretend devices are
/// asked for.
pub fn installed() -> bool {
    pretend::active() || driver_path().is_some()
}

/// Where Desktop Video puts the driver.
fn driver_places() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Library/Frameworks/DeckLinkAPI.framework")]
    } else if cfg!(target_os = "linux") {
        ["/usr/lib", "/usr/lib64", "/usr/local/lib", "/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu"]
            .iter()
            .map(|folder| PathBuf::from(folder).join("libDeckLinkAPI.so"))
            .collect()
    } else {
        Vec::new()
    }
}

/// A driver version as BMDDeckLinkAPIVersion gives it, in words: "15.3.1", "16.0".
pub fn version_in_words(version: u32) -> String {
    let [major, minor, sub, _] = version.to_be_bytes();
    if sub == 0 { format!("{major}.{minor}") } else { format!("{major}.{minor}.{sub}") }
}

/// Lists what is there now, on the caller's thread: opens the driver if it is installed (which
/// may take a moment), and lists its devices. For a program that runs once (the check);
/// the app uses [`Discovery`].
pub fn list_now() -> Found {
    discover::list_now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(name_height: u32, dominance: &[u8; 4], duration: i64, scale: i64) -> Mode {
        let width = if name_height == 720 { 1280 } else { 1920 };
        Mode::new(
            0,
            String::new(),
            (width, name_height),
            (duration, scale),
            u32::from_be_bytes(*dominance),
            (true, true, false),
        )
    }

    #[test]
    fn modes_are_named_by_their_lines_and_rate_as_is_usual() {
        assert_eq!(mode(1080, b"uppr", 1001, 30000).name, "1080i59.94");
        assert_eq!(mode(1080, b"uppr", 1000, 25000).name, "1080i50");
        assert_eq!(mode(1080, b"prog", 1001, 24000).name, "1080p23.98");
        assert_eq!(mode(1080, b"prog", 1000, 30000).name, "1080p30");
        assert_eq!(mode(720, b"prog", 1001, 60000).name, "720p59.94");
        assert_eq!(mode(720, b"prog", 1000, 50000).name, "720p50");
    }

    #[test]
    fn only_hd_modes_that_can_be_sent_are_offered() {
        assert!(mode(1080, b"uppr", 1001, 30000).offered());
        assert!(mode(720, b"prog", 1000, 50000).offered());
        assert!(!mode(1080, b"psf ", 1000, 25000).offered(), "segmented frames are left out");
        let sd =
            Mode::new(0, String::new(), (720, 486), (1001, 30000), u32::from_be_bytes(*b"lowr"), (true, true, false));
        assert!(!sd.offered(), "standard definition is left out");
        let neither = Mode { bgra: false, yuv: false, ..mode(1080, b"prog", 1000, 25000) };
        assert!(!neither.offered(), "a mode neither format can be sent in");
    }

    #[test]
    fn a_new_output_starts_at_1080i5994_or_the_nearest_the_device_has() {
        let all = [
            mode(720, b"prog", 1000, 50000),
            mode(1080, b"uppr", 1000, 25000),
            mode(1080, b"uppr", 1001, 30000),
            mode(1080, b"prog", 1000, 30000),
        ];
        let refs: Vec<&Mode> = all.iter().collect();
        assert_eq!(default_mode(&refs).unwrap().name, "1080i59.94");
        assert_eq!(default_mode(&[&all[0], &all[1], &all[3]]).unwrap().name, "1080i50");
        assert_eq!(default_mode(&[&all[0], &all[3]]).unwrap().name, "1080p30");
        assert_eq!(default_mode(&[&all[0]]).unwrap().name, "720p50");
        assert!(default_mode(&[]).is_none());
    }

    #[test]
    fn a_device_is_found_by_its_id_and_never_swapped_for_another_of_the_same_name() {
        let device = |name: &str, id: Option<i64>| Device {
            name: name.to_owned(),
            model: "DeckLink Duo 2".to_owned(),
            id,
            sub_device: Some(0),
            sub_devices: Some(4),
            profile: 0,
            duplex: 0,
            connections: 1,
            can_play: true,
            internal_keying: false,
            external_keying: false,
            modes: vec![mode(1080, b"uppr", 1001, 30000)],
        };
        let found =
            Found { driver: Driver::Ready("16.3".to_owned()), devices: vec![device("DeckLink Duo (1)", Some(7))] };
        assert!(found.find(Some(7), "anything").is_some());
        assert!(found.find(Some(8), "DeckLink Duo (1)").is_none(), "the same name on another card is not it");
        assert!(found.find(None, "DeckLink Duo (1)").is_some(), "without an id, by name");
        let mut unused = device("DeckLink Duo (2)", Some(9));
        unused.duplex = DUPLEX_INACTIVE;
        assert!(!unused.usable(), "a sub-device its profile leaves unused is not offered");
        assert_eq!(found.devices[0].mode("1080I59.94").map(|mode| mode.code), Some(0));
        assert_eq!(found.devices[0].connections_in_words(), "SDI");
    }

    #[test]
    fn versions_are_said_as_desktop_video_numbers_them() {
        assert_eq!(version_in_words(0x0F03_0100), "15.3.1");
        assert_eq!(version_in_words(0x1003_0000), "16.3");
        const { assert!(0x0F03_0100 < NEEDS_VERSION && 0x1003_0000 >= NEEDS_VERSION) };
    }

    #[test]
    fn keying_is_named_and_read_back() {
        for keying in [Keying::Off, Keying::External, Keying::Internal] {
            assert_eq!(Keying::parse(keying.name()), Some(keying));
        }
        assert_eq!(Keying::parse("sideways"), None);
    }
}
