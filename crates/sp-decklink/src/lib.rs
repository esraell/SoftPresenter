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
//! - [`DeckLinkOutput`] plays a screen out of one device, on its own thread: the screens'
//!   thread hands it each finished picture (as it does to NDI) and never waits; the output
//!   turns the picture into the card's format (8-bit BGRA where the mode takes it, else 8-bit
//!   YUV 4:2:2) and size (720p is scaled down from the 1920 x 1080 picture), and plays it on the
//!   card's own clock a few frames ahead, the last picture again whenever no new one has come,
//!   so a slow moment never leaves the card without a frame. A device that stops, goes away or
//!   changes profile is opened again, every couple of seconds, until it is back.
//!
//! On Windows the crate is built without its bridge to the driver (Windows' DeckLink
//! interface needs Microsoft's MIDL compiler), and behaves as on a computer without the
//! driver.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unsafe_code)]
mod ffi;

pub mod check;
mod convert;
mod discover;
mod play;
mod player;
mod pretend;

use std::path::PathBuf;

pub use convert::PixelFormat;
pub use discover::Discovery;
pub use play::{DeckLinkOutput, Log, State, Try, Wanted, Watch};
pub use player::Counts;

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

/// `bmdModeUnknown`: no display mode.
const MODE_UNKNOWN: u32 = u32::from_be_bytes(*b"iunk");

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
    /// The format it plays out in by default, as Desktop Video Setup sets it: the value of
    /// `bmdDeckLinkConfigDefaultVideoOutputMode` exactly as the driver gives it (a
    /// `BMDDisplayMode`; 0 or `bmdModeUnknown` when none is set), or `None` when the driver
    /// doesn't say. See [`Device::card_format`].
    pub card_mode: Option<i64>,
    /// That format is meant as dual-stream 3D (`bmdDeckLinkConfigDefaultVideoOutputModeFlags`
    /// has `bmdVideoOutputDualStream3D`), which SoftPresenter doesn't play.
    pub card_mode_3d: bool,
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

    /// The mode a new output starts in: see [`Device::start`].
    pub fn default_mode(&self) -> Option<&Mode> {
        self.start(false).map(|start| start.mode)
    }

    /// The format a new output on this device starts in, and why: the device's own default, as
    /// Desktop Video Setup sets it, where SoftPresenter offers that format here and can play it
    /// (and key in it, when `keying`); else the usual one ([`default_mode`]).
    pub fn start(&self, keying: bool) -> Option<Start<'_>> {
        let card = self.card_format(keying);
        let mode = match card {
            CardFormat::Usable(mode) => mode,
            _ => default_mode(&self.offered().collect::<Vec<_>>())?,
        };
        Some(Start { mode, card })
    }

    /// What the device's own default format is to SoftPresenter. The driver gives it as a
    /// `BMDDisplayMode`, which is looked for among every mode the device lists (not only those
    /// offered), so one that can't be used can still be named.
    pub fn card_format(&self, keying: bool) -> CardFormat<'_> {
        let Some(code) =
            self.card_mode.and_then(|raw| u32::try_from(raw).ok()).filter(|code| *code != 0 && *code != MODE_UNKNOWN)
        else {
            return CardFormat::NotSet;
        };
        let Some(mode) = self.modes.iter().find(|mode| mode.code == code) else { return CardFormat::NotListed(code) };
        if !mode.offered() {
            CardFormat::NotOffered(mode)
        } else if self.card_mode_3d {
            CardFormat::ThreeD(mode)
        } else if keying && !mode.keying {
            CardFormat::CantKey(mode)
        } else {
            CardFormat::Usable(mode)
        }
    }

    /// Where a screen's format on this device comes from, in words: the format a new output
    /// starts in, with why ("1080p30, as Desktop Video Setup has it"; "1080i59.94, the usual HD
    /// format: Desktop Video Setup sets none"), or another, set in SoftPresenter, with what
    /// Desktop Video Setup has ("720p50, set in SoftPresenter: Desktop Video Setup has 1080p30").
    pub fn format_in_words(&self, mode: &str) -> String {
        match self.start(false) {
            Some(start) if start.mode.name.eq_ignore_ascii_case(mode.trim()) => start.in_words(),
            _ => format!("{}, set in SoftPresenter: {}", mode.trim(), self.card_format(false).in_words()),
        }
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

/// The mode a new DeckLink output starts in, among the modes offered, when the device's own
/// default (as Desktop Video Setup sets it) can't be used: 1080i59.94, the usual HD format for
/// SDI in countries with 60 Hz power, which every HD switcher and recorder takes and which
/// needs only a 1.5G link; else 1080i50, its counterpart where power is 50 Hz; else the first
/// 1080-line mode, as SoftPresenter draws every screen at 1920 x 1080; else the first. The
/// operator picks the mode their switcher or projector is set to.
pub fn default_mode<'m>(modes: &[&'m Mode]) -> Option<&'m Mode> {
    ["1080i59.94", "1080i50"]
        .iter()
        .find_map(|name| modes.iter().find(|mode| mode.name == *name))
        .or_else(|| modes.iter().find(|mode| mode.height == 1080))
        .or_else(|| modes.first())
        .copied()
}

/// What a device's own default format, as Desktop Video Setup sets it, is to SoftPresenter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardFormat<'d> {
    /// None is set, or the driver doesn't say.
    NotSet,
    /// A format SoftPresenter offers on the device and can play (and key in, when asked): a new
    /// output starts in it.
    Usable(&'d Mode),
    /// A format the device doesn't list (the `BMDDisplayMode` the driver gave).
    NotListed(u32),
    /// A format SoftPresenter doesn't offer: not 720p or 1080 lines, segmented frames, or
    /// neither 8-bit BGRA nor 8-bit YUV.
    NotOffered(&'d Mode),
    /// Meant as dual-stream 3D.
    ThreeD(&'d Mode),
    /// A format the device can't key in, when keying is asked.
    CantKey(&'d Mode),
}

impl CardFormat<'_> {
    /// The format's name, where the device lists it: SoftPresenter's name, or for one it
    /// doesn't offer, the driver's own name for it (which says what it is, where SoftPresenter's
    /// name might not, as for segmented frames).
    pub fn name(&self) -> Option<&str> {
        match self {
            CardFormat::Usable(mode) | CardFormat::ThreeD(mode) | CardFormat::CantKey(mode) => Some(&mode.name),
            CardFormat::NotOffered(mode) if !mode.driver_name.is_empty() => Some(&mode.driver_name),
            CardFormat::NotOffered(mode) => Some(&mode.name),
            CardFormat::NotSet | CardFormat::NotListed(_) => None,
        }
    }

    /// What Desktop Video Setup has, in words: "Desktop Video Setup has 1080p30", "Desktop
    /// Video Setup sets none".
    pub fn in_words(&self) -> String {
        let name = self.name().unwrap_or_default();
        match self {
            CardFormat::NotSet => "Desktop Video Setup sets none".to_owned(),
            CardFormat::Usable(_) => format!("Desktop Video Setup has {name}"),
            CardFormat::NotListed(code) => {
                format!("Desktop Video Setup has a format this output doesn't list ({})", code_in_words(*code))
            }
            CardFormat::NotOffered(_) => format!("Desktop Video Setup has {name}, which SoftPresenter doesn't offer"),
            CardFormat::ThreeD(_) => format!("Desktop Video Setup has {name} in 3D, which SoftPresenter doesn't play"),
            CardFormat::CantKey(_) => format!("Desktop Video Setup has {name}, which this output can't key in"),
        }
    }
}

/// The format a new output starts in, and the device's own default beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start<'d> {
    pub mode: &'d Mode,
    /// The device's own default: [`CardFormat::Usable`] when `mode` is it.
    pub card: CardFormat<'d>,
}

impl Start<'_> {
    /// True when the format is the device's own default.
    pub fn from_card(&self) -> bool {
        matches!(self.card, CardFormat::Usable(_))
    }

    /// The format and where it comes from: "1080p30, as Desktop Video Setup has it";
    /// "1080i59.94, the usual HD format: Desktop Video Setup sets none".
    pub fn in_words(&self) -> String {
        if self.from_card() {
            return format!("{}, as Desktop Video Setup has it", self.mode.name);
        }
        let usual = match self.mode.name.as_str() {
            "1080i59.94" => "the usual HD format",
            "1080i50" => "the usual HD format where power is 50 Hz",
            _ if self.mode.height == 1080 => "this output's first 1080-line format",
            _ => "this output's first format",
        };
        format!("{}, {usual}: {}", self.mode.name, self.card.in_words())
    }
}

/// A `BMDDisplayMode` as the SDK writes it, its four letters ('Hp30'), or in hex when they
/// aren't letters.
pub fn code_in_words(code: u32) -> String {
    let bytes = code.to_be_bytes();
    if bytes.iter().all(|byte| byte.is_ascii_graphic() || *byte == b' ') {
        format!("'{}'", String::from_utf8_lossy(&bytes))
    } else {
        format!("{code:#010x}")
    }
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
    /// The key of a see-through picture: its alpha as solid greys, white where solid, black
    /// where clear (for a screen that is another's key, played out of a card without a keyer).
    Key,
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

/// True when the testing switch SOFTPRESENTER_PRETEND_DECKLINK has made-up devices listed in
/// place of the real ones: what is played out of them goes nowhere.
pub fn pretending() -> bool {
    pretend::active()
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

    /// A device with these modes, each with its own code, its own default as `card_mode`.
    fn card(modes: Vec<Mode>, card_mode: Option<i64>) -> Device {
        let modes = modes.into_iter().enumerate().map(|(at, mode)| Mode { code: 100 + at as u32, ..mode }).collect();
        Device {
            name: "DeckLink Duo (1)".to_owned(),
            model: "DeckLink Duo 2".to_owned(),
            id: Some(1),
            sub_device: Some(0),
            sub_devices: Some(4),
            profile: 0,
            duplex: 0,
            connections: 1,
            can_play: true,
            internal_keying: false,
            external_keying: false,
            modes,
            card_mode,
            card_mode_3d: false,
        }
    }

    fn hd_modes() -> Vec<Mode> {
        vec![
            mode(720, b"prog", 1000, 50000),                                      // 100
            mode(1080, b"uppr", 1001, 30000),                                     // 101
            mode(1080, b"prog", 1000, 30000),                                     // 102
            mode(1080, b"psf ", 1000, 25000),                                     // 103, segmented frames: not offered
            Mode { bgra: false, yuv: false, ..mode(1080, b"prog", 1000, 25000) }, // 104, can't be sent
            Mode::new(
                0,
                "NTSC".to_owned(),
                (720, 486),
                (1001, 30000),
                u32::from_be_bytes(*b"lowr"),
                (true, true, false),
            ), // 105
        ]
    }

    #[test]
    fn a_new_output_starts_in_the_cards_own_default_where_it_is_offered_and_can_be_played() {
        let set = card(hd_modes(), Some(102));
        let start = set.start(false).unwrap();
        assert_eq!((start.mode.name.as_str(), start.from_card()), ("1080p30", true));
        assert_eq!(start.in_words(), "1080p30, as Desktop Video Setup has it");
        assert_eq!(set.default_mode().map(|mode| mode.name.as_str()), Some("1080p30"));
        let p720 = card(hd_modes(), Some(100));
        assert_eq!(p720.default_mode().map(|mode| mode.name.as_str()), Some("720p50"), "a 720p default too");
    }

    #[test]
    fn a_default_that_is_unset_or_cant_be_used_leaves_the_usual_format() {
        let usual = |device: &Device| {
            let start = device.start(false).unwrap();
            assert!(!start.from_card(), "{start:?}");
            (start.mode.name.clone(), start.in_words())
        };
        for raw in [None, Some(0), Some(i64::from(MODE_UNKNOWN)), Some(-1), Some(i64::from(u32::MAX) + 1)] {
            let unset = card(hd_modes(), raw);
            assert_eq!(unset.card_format(false), CardFormat::NotSet, "{raw:?}");
            assert_eq!(
                usual(&unset),
                ("1080i59.94".to_owned(), "1080i59.94, the usual HD format: Desktop Video Setup sets none".to_owned())
            );
        }
        let segmented = card(hd_modes(), Some(103));
        assert!(matches!(segmented.card_format(false), CardFormat::NotOffered(_)));
        assert_eq!(usual(&segmented).0, "1080i59.94");
        let unplayable = card(hd_modes(), Some(104));
        assert!(matches!(unplayable.card_format(false), CardFormat::NotOffered(_)), "neither BGRA nor YUV");
        let sd = card(hd_modes(), Some(105));
        assert_eq!(
            usual(&sd).1,
            "1080i59.94, the usual HD format: Desktop Video Setup has NTSC, which SoftPresenter doesn't offer"
        );
        let unlisted = card(hd_modes(), Some(i64::from(u32::from_be_bytes(*b"4k30"))));
        assert_eq!(unlisted.card_format(false), CardFormat::NotListed(u32::from_be_bytes(*b"4k30")));
        assert!(usual(&unlisted).1.ends_with("Desktop Video Setup has a format this output doesn't list ('4k30')"));
        let three_d = Device { card_mode_3d: true, ..card(hd_modes(), Some(102)) };
        assert!(usual(&three_d).1.ends_with("Desktop Video Setup has 1080p30 in 3D, which SoftPresenter doesn't play"));
        // Without 1080i59.94, the rest of the usual order, with its own words.
        let no_5994 = card(vec![mode(720, b"prog", 1000, 50000), mode(1080, b"uppr", 1000, 25000)], None);
        assert_eq!(
            usual(&no_5994).1,
            "1080i50, the usual HD format where power is 50 Hz: Desktop Video Setup sets none"
        );
        let only_p = card(vec![mode(720, b"prog", 1000, 50000), mode(1080, b"prog", 1000, 25000)], Some(999));
        assert!(usual(&only_p).1.starts_with("1080p25, this output's first 1080-line format: "));
        assert!(card(Vec::new(), Some(100)).start(false).is_none(), "nothing to start in");
    }

    #[test]
    fn to_key_the_cards_default_must_be_one_the_device_keys_in() {
        let mut modes = hd_modes();
        modes[1].keying = true; // 1080i59.94 keys, 1080p30 doesn't
        let device = card(modes, Some(102));
        assert!(device.start(false).unwrap().from_card());
        let keyed = device.start(true).unwrap();
        assert_eq!(keyed.card, CardFormat::CantKey(&device.modes[2]));
        assert_eq!(keyed.mode.name, "1080i59.94");
        assert!(keyed.in_words().ends_with("Desktop Video Setup has 1080p30, which this output can't key in"));
    }

    #[test]
    fn a_screens_format_says_where_it_came_from() {
        let set = card(hd_modes(), Some(102));
        assert_eq!(set.format_in_words("1080p30"), "1080p30, as Desktop Video Setup has it");
        assert_eq!(
            set.format_in_words("1080i59.94"),
            "1080i59.94, set in SoftPresenter: Desktop Video Setup has 1080p30"
        );
        let unset = card(hd_modes(), None);
        assert_eq!(
            unset.format_in_words("1080i59.94"),
            "1080i59.94, the usual HD format: Desktop Video Setup sets none"
        );
        assert_eq!(unset.format_in_words("720p50"), "720p50, set in SoftPresenter: Desktop Video Setup sets none");
        assert_eq!(code_in_words(u32::from_be_bytes(*b"Hp30")), "'Hp30'");
        assert_eq!(code_in_words(0), "0x00000000");
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
            card_mode: None,
            card_mode_3d: false,
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
