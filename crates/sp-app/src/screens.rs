//! The audience screens: what each one is called and where its picture goes. The list is kept
//! in `screens.json` in the app's folder.

use serde::{Deserialize, Serialize};
use sp_model::{Rgba, Uuid};

use crate::displays::{DisplayInfo, Kept, Placement, choose_output, for_automatic, kept_for_operator};
use crate::i18n::{t, tf, tgf};

mod background_ui;
mod decklink_ui;
pub use background_ui::{background_rows, paint_background};
pub use decklink_ui::decklink_rows;

/// Where a screen's picture goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    /// The first display the operator's window is not on, filled; a window if there is none.
    Automatic,
    /// A display, by the name the system gives it, filled.
    Display(String),
    /// An ordinary window.
    Window,
    /// The network, over NDI, under this name.
    Ndi(String),
    /// A Blackmagic Design DeckLink card's output (SDI or HDMI).
    #[serde(rename = "decklink")]
    DeckLink(DeckLinkOut),
    /// Nowhere. The screen is kept in the list for later.
    Off,
}

/// A DeckLink output a screen plays out of: the device as the driver names it ("DeckLink Duo
/// (1)"), the driver's persistent id for it (the device is found by that when it has one,
/// never by its name alone, so a screen never moves to another card of the same name), the
/// mode ("1080i59.94"), and how the card's keyer is used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckLinkOut {
    pub device: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Keyer::is_off")]
    pub keyer: Keyer,
}

impl DeckLinkOut {
    /// True when both name the same device.
    pub fn same_device(&self, other: &DeckLinkOut) -> bool {
        match (self.id, other.id) {
            (Some(one), Some(two)) => one == two,
            _ => self.device == other.device,
        }
    }

    /// Where its format comes from, against the card as last found: in words ("1080p30, as
    /// Desktop Video Setup has it"; "720p50, set in SoftPresenter: Desktop Video Setup has
    /// 1080p30"), and as one word for scripts: "card" (the card's own default, as Desktop Video
    /// Setup sets it), "usual" (the usual format a new output starts in when the card's can't be
    /// used), or "set" (another, set in SoftPresenter). Worked out afresh each time, never saved,
    /// so it follows what Desktop Video Setup has now. `None` while the device isn't connected.
    pub fn format_from(&self, found: &sp_decklink::Found) -> Option<(String, &'static str)> {
        let device = found.find(self.id, &self.device)?;
        let from = match device.start(false) {
            Some(start) if start.mode.name == self.mode && start.from_card() => "card",
            Some(start) if start.mode.name == self.mode => "usual",
            _ => "set",
        };
        Some((device.format_in_words(&self.mode), from))
    }

    /// The format Desktop Video Setup has for this output, where it isn't the screen's and
    /// SoftPresenter can play it here (and key in it, when the card keys): the Screens window
    /// offers to switch to it, as a switcher usually needs every input in its own format. `None`
    /// when the two are the same, Desktop Video Setup sets none (or one that can't be used), or
    /// the device isn't connected.
    pub fn card_format_to_offer(&self, found: &sp_decklink::Found) -> Option<String> {
        let device = found.find(self.id, &self.device)?;
        match device.card_format(self.keyer != Keyer::Off) {
            sp_decklink::CardFormat::Usable(mode) if !mode.name.eq_ignore_ascii_case(self.mode.trim()) => {
                Some(mode.name.clone())
            }
            _ => None,
        }
    }

    /// What the output thread is to play.
    pub fn wanted(&self) -> sp_decklink::Wanted {
        sp_decklink::Wanted {
            device: self.device.clone(),
            id: self.id,
            mode: self.mode.clone(),
            keying: self.keyer.keying(),
        }
    }
}

/// How a DeckLink output uses its card's keyer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keyer {
    /// Not at all: the picture goes solid.
    #[default]
    Off,
    /// The card sends the colours (fill) and the alpha (key) on two connectors.
    External,
    /// The card lays the picture over the video coming into it.
    Internal,
}

impl Keyer {
    pub fn is_off(&self) -> bool {
        *self == Keyer::Off
    }

    pub fn keying(self) -> sp_decklink::Keying {
        match self {
            Keyer::Off => sp_decklink::Keying::Off,
            Keyer::External => sp_decklink::Keying::External,
            Keyer::Internal => sp_decklink::Keying::Internal,
        }
    }

    pub fn name(self) -> &'static str {
        self.keying().name()
    }

    pub fn parse(word: &str) -> Option<Keyer> {
        Some(match sp_decklink::Keying::parse(word)? {
            sp_decklink::Keying::Off => Keyer::Off,
            sp_decklink::Keying::External => Keyer::External,
            sp_decklink::Keying::Internal => Keyer::Internal,
        })
    }
}

/// The DeckLink outputs a screen can be given: each device that can play now, in the format a
/// new output starts in (the card's own, as Desktop Video Setup sets it, where it can be used;
/// see `sp_decklink::Device::start`); and the one the screen has, kept as it is (its format and
/// keyer), even while it isn't connected or the card's own format has changed. None without
/// Desktop Video.
pub fn decklink_choices(current: &Output, found: &sp_decklink::Found) -> Vec<Output> {
    let mut choices = Vec::new();
    let mut kept = false;
    for device in found.outputs() {
        let Some(mode) = device.default_mode() else { continue };
        let out =
            DeckLinkOut { device: device.name.clone(), id: device.id, mode: mode.name.clone(), keyer: Keyer::Off };
        match current {
            Output::DeckLink(have) if have.same_device(&out) => {
                kept = true;
                choices.push(current.clone());
            }
            _ => choices.push(Output::DeckLink(out)),
        }
    }
    if matches!(current, Output::DeckLink(_)) && !kept {
        choices.push(current.clone());
    }
    choices
}

/// A DeckLink screen's format and where it comes from, for `sp screens` (see
/// [`DeckLinkOut::format_from`]). `None` for a screen on anything else, or while its device
/// isn't connected.
pub fn decklink_format(screen: &Screen, found: &sp_decklink::Found) -> Option<(String, &'static str)> {
    match &screen.output {
        Output::DeckLink(out) => out.format_from(found),
        _ => None,
    }
}

/// What a screen is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// For the people in the room, the stream and the recording: the slides themselves.
    #[default]
    Audience,
    /// For the people on stage: the slide on screen, the next slide and the time.
    Stage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    pub id: Uuid,
    pub name: String,
    pub output: Output,
    #[serde(default)]
    pub kind: Kind,
    /// For a stage screen, the stage layout it is drawn with, by its id; `None` uses the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_layout: Option<Uuid>,
    /// For an audience screen, which layers it shows, its colour, and its covered edges.
    #[serde(default, skip_serializing_if = "Look::is_default")]
    pub look: Look,
    /// For an audience screen, the theme its slides are drawn with, by its id: the same words
    /// laid out another way (a lower third for the stream, say). `None` uses the main theme, or
    /// a song's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Uuid>,
    /// For a screen sent over NDI, whether the sound the app plays (videos, audio playlists)
    /// goes with its pictures. On unless switched off, so only "off" is written.
    #[serde(default = "sound_on", skip_serializing_if = "is_on")]
    pub sound: bool,
    /// For a screen sent over NDI, how many pictures a second it sends. 29.97 unless another is
    /// chosen, so only another is written.
    #[serde(default, skip_serializing_if = "NdiRate::is_default")]
    pub rate: NdiRate,
    /// The screen's own switch. Off, it stops going out but keeps where it goes (its display or
    /// window, its NDI name, its DeckLink output, format and keyer), so On brings it back
    /// exactly as it was: a display or window screen shows nothing, and one on NDI or a
    /// DeckLink output goes on sending, black, so a stream or switcher keeps it as a source. On
    /// unless switched off, so only "off" is written; a screen saved before it had a switch is
    /// on, and one saved going nowhere (`Output::Off`) still goes nowhere.
    #[serde(default = "sound_on", skip_serializing_if = "is_on")]
    pub on: bool,
}

/// How many pictures a second a screen sent over NDI sends: the rate the equipment that shows it
/// plays at. A decoder or switcher playing at another rate drops or repeats a picture every so
/// often (a 59.94 Hz one given 30 a second, about every 33 s), which is seen as a small pause.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NdiRate {
    #[default]
    #[serde(rename = "29.97")]
    R29_97,
    #[serde(rename = "30")]
    R30,
    #[serde(rename = "59.94")]
    R59_94,
    #[serde(rename = "60")]
    R60,
    #[serde(rename = "25")]
    R25,
    #[serde(rename = "50")]
    R50,
}

impl NdiRate {
    /// In the order they are offered.
    pub const ALL: [NdiRate; 6] =
        [NdiRate::R29_97, NdiRate::R30, NdiRate::R59_94, NdiRate::R60, NdiRate::R25, NdiRate::R50];

    /// The rate as NDI is told it, an exact fraction.
    pub fn rate(self) -> sp_ndi::Rate {
        match self {
            NdiRate::R29_97 => sp_ndi::Rate::R29_97,
            NdiRate::R30 => sp_ndi::Rate::R30,
            NdiRate::R59_94 => sp_ndi::Rate::R59_94,
            NdiRate::R60 => sp_ndi::Rate::R60,
            NdiRate::R25 => sp_ndi::Rate::R25,
            NdiRate::R50 => sp_ndi::Rate::R50,
        }
    }

    /// As it is said and typed: "29.97", "60".
    pub fn name(self) -> String {
        self.rate().name()
    }

    /// The rate typed, if it is one of them.
    pub fn parse(typed: &str) -> Option<NdiRate> {
        NdiRate::ALL.into_iter().find(|rate| rate.name() == typed.trim())
    }

    fn is_default(&self) -> bool {
        *self == NdiRate::default()
    }
}

fn sound_on() -> bool {
    true
}

fn is_on(on: &bool) -> bool {
    *on
}

/// How an audience screen shows the show: which layers (a lobby screen might show only the
/// media), the colour where nothing is drawn, and how much of each edge to cover, for a screen
/// whose edges are cut off; whether it is see-through, for a video switcher to lay the words
/// over a camera picture; or whether it shows another screen's key instead.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub slides: bool,
    pub media: bool,
    pub props: bool,
    pub messages: bool,
    pub announcements: bool,
    pub color: Rgba,
    /// Left, top, right, bottom, in slide units.
    pub mask: [f32; 4],
    /// See-through, and how a partly solid pixel's colour is sent; `None` is solid, as usual.
    /// A see-through screen is clear wherever nothing is drawn, and leaves out what would
    /// cover the whole picture: the media behind the words, every theme's background colour
    /// and picture, and its own colour. Its covered edges are clear. What is placed on the
    /// slides stays: the words, the layout's and the slide's own shapes and pictures, props,
    /// messages, the copyright line and the announcements' words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub see_through: Option<Alpha>,
    /// Shows another audience screen's key instead of the show, by that screen's id: how
    /// solid each pixel of its picture is, as a grey (white where solid, black where clear,
    /// greys at the soft edges), for a switcher that takes the words and their key as two
    /// inputs. The key is made from the very picture that screen shows (see `key_trouble`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_of: Option<Uuid>,
}

/// How a see-through screen sends the colour of a pixel that is only partly solid, such as the
/// soft edge of a letter. Video switchers and streaming software expect one or the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alpha {
    /// The colour as it is, with how solid it is beside it: NDI's own way, and the usual one.
    #[default]
    Straight,
    /// The colour already multiplied by how solid it is, as if laid over black.
    Premultiplied,
}

impl Alpha {
    pub fn name(self) -> &'static str {
        match self {
            Alpha::Straight => "straight",
            Alpha::Premultiplied => "premultiplied",
        }
    }

    /// The kind a word names, as `sp` takes it.
    pub fn parse(word: &str) -> Option<Alpha> {
        match word.trim().to_lowercase().as_str() {
            "straight" => Some(Alpha::Straight),
            "premultiplied" | "premultiply" | "pre-multiplied" => Some(Alpha::Premultiplied),
            _ => None,
        }
    }
}

impl Default for Look {
    fn default() -> Look {
        Look {
            slides: true,
            media: true,
            props: true,
            messages: true,
            announcements: true,
            color: Rgba::BLACK,
            mask: [0.0; 4],
            see_through: None,
            key_of: None,
        }
    }
}

impl Look {
    pub fn is_default(&self) -> bool {
        *self == Look::default()
    }

    /// Every layer, black, nothing covered: the look given back by Show Everything, which
    /// keeps whether the screen is see-through or a key, so a stream never turns solid by it.
    pub fn everything(self) -> Look {
        Look { see_through: self.see_through, key_of: self.key_of, ..Look::default() }
    }

    /// True when it shows every layer, black, nothing covered, whatever its background.
    pub fn shows_everything(&self) -> bool {
        self.everything() == *self
    }
}

/// What the operator is told of a key screen, in plain words, when it is not showing its
/// fill's key as it should: then it shows black (which keys nothing in, so a switcher shows
/// its camera untouched), or for a solid fill, white. `None` when all is well, or the screen
/// is not a key. `shown` says, for each screen in the list, whether it is going out now.
///
/// A key is made from the very picture its fill shows, so the two never show different
/// moments: the key of a display or window from that screen's painted picture as its window
/// is given it, the key of an NDI or DeckLink screen from the same picture as it comes back
/// from the GPU. So a key and its fill are both displays or windows, or both NDI or DeckLink,
/// and both going out. (A DeckLink card with a keyer sends a screen's key itself, beside it.)
pub fn key_trouble(screens: &[Screen], index: usize, shown: &[bool]) -> Option<String> {
    let screen = screens.get(index)?;
    let fill_id = screen.look.key_of.filter(|_| screen.kind == Kind::Audience)?;
    let Some(at) = screens.iter().position(|other| other.id == fill_id) else {
        return Some(
            t("screens.key.fill_removed", "The screen it is the key of has been removed, so it shows black.")
                .to_owned(),
        );
    };
    let fill = &screens[at];
    let name = &fill.name;
    if at == index {
        return Some(t("screens.key.own", "A screen can't be its own key, so it shows black.").to_owned());
    }
    if fill.kind == Kind::Stage {
        return Some(tf(
            "screens.key.stage",
            "\"{name}\" is a stage screen, which has no key, so this shows black.",
            &[("name", name)],
        ));
    }
    if fill.look.key_of.is_some() {
        return Some(tf("screens.key.is_key", "\"{name}\" is a key itself, so this shows black.", &[("name", name)]));
    }
    let read_back = |screen: &Screen| matches!(screen.output, Output::Ndi(_) | Output::DeckLink(_));
    if read_back(screen) != read_back(fill) {
        return Some(match fill.output {
            Output::Ndi(_) => tf(
                "screens.key.mixed.ndi",
                "\"{name}\" is on NDI. A key goes out beside its fill on two displays or windows, or on two NDI or DeckLink outputs, so this shows black.",
                &[("name", name)],
            ),
            Output::DeckLink(_) => tf(
                "screens.key.mixed.decklink",
                "\"{name}\" is on a DeckLink output. A key goes out beside its fill on two displays or windows, or on two NDI or DeckLink outputs, so this shows black.",
                &[("name", name)],
            ),
            _ => tf(
                "screens.key.mixed.display",
                "\"{name}\" is on a display or window. A key goes out beside its fill on two displays or windows, or on two NDI or DeckLink outputs, so this shows black.",
                &[("name", name)],
            ),
        });
    }
    if !shown.get(at).copied().unwrap_or(false) {
        return Some(tf("screens.key.not_out", "\"{name}\" isn't going out, so this shows black.", &[("name", name)]));
    }
    if fill.look.see_through.is_none() {
        return Some(tf(
            "screens.key.solid",
            "\"{name}\" is solid, so its key is all white. Make it see-through.",
            &[("name", name)],
        ));
    }
    None
}

/// What a screen's background is, in a few words, for `sp screens` and the Screens window:
/// `None` for an ordinary solid screen.
pub fn background_words(screens: &[Screen], index: usize) -> Option<String> {
    let screen = screens.get(index).filter(|screen| screen.kind == Kind::Audience)?;
    if let Some(fill) = screen.look.key_of {
        let name = screens.iter().find(|other| other.id == fill).map_or("a screen that has gone", |other| &other.name);
        return Some(format!("key of \"{name}\""));
    }
    screen.look.see_through.map(|alpha| format!("see-through, {} alpha", alpha.name()))
}

/// Where a screen ends up on this computer as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placed {
    /// Filling the display with this index.
    Fullscreen(usize),
    Window,
    Ndi(String),
    DeckLink(DeckLinkOut),
    /// Not shown anywhere, and why, in plain words. Empty when the screen is simply off.
    Nothing(String),
}

impl Screen {
    pub fn new(name: String, output: Output) -> Screen {
        Screen {
            id: Uuid::new_v4(),
            name,
            output,
            kind: Kind::Audience,
            stage_layout: None,
            look: Look::default(),
            theme: None,
            sound: true,
            rate: NdiRate::default(),
            on: true,
        }
    }

    /// True when the screen goes nowhere: switched off, or with nowhere to go.
    pub fn is_off(&self) -> bool {
        !self.on || self.output == Output::Off
    }

    /// True when a screen switched off still sends (black): it is on NDI or a DeckLink output,
    /// whose receivers or switcher keep it as a source.
    pub fn sends_while_off(&self) -> bool {
        !self.on && matches!(&self.output, Output::Ndi(_) | Output::DeckLink(_))
    }

    /// Sends the screen to `output` and switches it on: choosing where a screen goes is what
    /// switching it on with nowhere kept to go asks for.
    pub fn go_to(&mut self, output: Output) {
        self.output = output;
        self.on = true;
    }

    /// What a choice in the Screens window's Output menu does: Off switches the screen off,
    /// keeping where it goes; where it went (the same display, NDI, the same DeckLink output)
    /// switches it on again as it was, its NDI name and DeckLink format and keyer too; anywhere
    /// else sends it there, switched on.
    pub fn choose_output(&mut self, choice: Output) {
        if choice == Output::Off {
            self.on = false;
            return;
        }
        let same = match (&choice, &self.output) {
            (Output::DeckLink(one), Output::DeckLink(two)) => one.same_device(two),
            (Output::Ndi(_), Output::Ndi(_)) => true,
            _ => choice == self.output,
        };
        if same {
            self.on = true;
        } else {
            self.go_to(choice);
        }
    }

    pub fn stage(name: String, output: Output) -> Screen {
        Screen { kind: Kind::Stage, ..Screen::new(name, output) }
    }

    /// True for an audience screen played out of a DeckLink card that keys it: fill and key, or
    /// over the card's input. Such a screen is always see-through (see [`Screen::set_keyer`]).
    pub fn keyed(&self) -> bool {
        self.kind == Kind::Audience && matches!(&self.output, Output::DeckLink(out) if out.keyer != Keyer::Off)
    }

    /// Sets how a DeckLink screen uses its card's keyer, and its background with it, as one
    /// choice. Keyed (fill and key, or over the card's input), it is see-through, so that only
    /// the words are keyed, with its soft edges premultiplied, as a switcher's keyer usually
    /// expects, unless `alpha` says otherwise; off, it is solid, a full picture. Nothing changes
    /// for a screen on anything else.
    pub fn set_keyer(&mut self, keyer: Keyer, alpha: Option<Alpha>) {
        let Output::DeckLink(out) = &mut self.output else { return };
        out.keyer = keyer;
        self.look.see_through = match keyer {
            Keyer::Off => None,
            Keyer::External | Keyer::Internal => Some(alpha.unwrap_or(Alpha::Premultiplied)),
        };
    }
}

/// Settles the keying of screens saved before keying and the background were one choice, whose
/// key was all white (every pixel solid), so a switcher keyed in the whole picture: a keyed
/// audience screen left solid is made see-through, with premultiplied soft edges, and a stage
/// screen's keyer (its picture is always solid) is turned off. Gives a line for the log for each
/// screen changed; run on the list as it is loaded, it finds nothing the next time.
pub fn settle_keying(screens: &mut [Screen]) -> Vec<String> {
    let mut said = Vec::new();
    for screen in screens {
        if screen.keyed() && screen.look.see_through.is_none() {
            screen.look.see_through = Some(Alpha::Premultiplied);
            said.push(format!(
                "\"{}\" is keyed by its DeckLink card but was solid, so its key was all white: made see-through, with premultiplied soft edges",
                screen.name
            ));
        } else if screen.kind == Kind::Stage
            && let Output::DeckLink(out) = &mut screen.output
            && out.keyer != Keyer::Off
        {
            out.keyer = Keyer::Off;
            said.push(format!(
                "\"{}\" is a stage screen, whose picture is always solid, so its key was all white: its DeckLink card's keyer is turned off",
                screen.name
            ));
        }
    }
    said
}

/// The screens a fresh start has: one, placed automatically. `ndi` adds a second sent over
/// NDI under that name, for someone who had NDI switched on before screens could be listed.
pub fn starting(ndi: Option<&str>) -> Vec<Screen> {
    let numbered = |n: &str| tgf("screens.name.numbered", "Screen {n}", &[("n", n)]);
    let mut screens = vec![Screen::new(numbered("1"), Output::Automatic)];
    if let Some(name) = ndi {
        screens.push(Screen::new(numbered("2"), Output::Ndi(name.to_owned())));
    }
    screens
}

/// The colour each screen is marked with while screens are identified, by its place in the
/// list: bright, easy to tell apart, and never red, which means live.
pub const IDENTIFY_COLORS: [sp_model::Rgba; 8] = [
    sp_model::Rgba { r: 10, g: 132, b: 255, a: 255 },
    sp_model::Rgba { r: 48, g: 209, b: 88, a: 255 },
    sp_model::Rgba { r: 255, g: 159, b: 10, a: 255 },
    sp_model::Rgba { r: 191, g: 90, b: 242, a: 255 },
    sp_model::Rgba { r: 100, g: 210, b: 255, a: 255 },
    sp_model::Rgba { r: 255, g: 214, b: 10, a: 255 },
    sp_model::Rgba { r: 94, g: 92, b: 230, a: 255 },
    sp_model::Rgba { r: 172, g: 142, b: 104, a: 255 },
];

/// The colour the screen at `place` in the list is marked with while screens are identified.
pub fn identify_color(place: usize) -> sp_model::Rgba {
    IDENTIFY_COLORS[place % IDENTIFY_COLORS.len()]
}

/// A name no screen in the list has for a new screen of a kind: "Screen 3" when there are two
/// audience screens, "Stage Screen 1" for the first stage screen, and so on.
pub fn next_name(screens: &[Screen], kind: Kind) -> String {
    // A name the app gives and saves, in the language set (see `i18n::tg`).
    let named = |n: usize| match kind {
        Kind::Audience => tgf("screens.name.numbered", "Screen {n}", &[("n", &n.to_string())]),
        Kind::Stage => tgf("screens.name.stage_numbered", "Stage Screen {n}", &[("n", &n.to_string())]),
    };
    let of_kind = screens.iter().filter(|screen| screen.kind == kind).count();
    (of_kind + 1..).map(named).find(|name| screens.iter().all(|screen| screen.name != *name)).unwrap_or_default()
}

/// Settles the screens' displays against the ones connected now, and says whether any changed
/// (to be saved). A screen set up as Automatic (no longer offered) is given, by name, the first
/// display Automatic may fill (`for_automatic`: never the operator's) that no other screen has,
/// so it stays there; while there is none (one display, or not knowing which is the operator's)
/// it stays Automatic, in a window, and takes a TV or projector the moment one is plugged in. A
/// screen saved under a display's old name ("Monitor #0", `\\.\DISPLAY2`) is given the name the
/// system shows ("3DTV", "Mi TV").
pub fn settle(screens: &mut [Screen], displays: &[DisplayInfo], operator: &[usize]) -> bool {
    let mut changed = false;
    for at in 0..screens.len() {
        let taken: Vec<String> = screens
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != at)
            .filter_map(|(_, screen)| match &screen.output {
                Output::Display(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let now = match &screens[at].output {
            Output::Automatic => for_automatic(displays, operator)
                .find(|d| !taken.iter().any(|name| d.is_named(name)))
                .map(|d| d.name.clone()),
            Output::Display(name) => {
                displays.iter().find(|d| d.old_name == *name && d.name != *name).map(|d| d.name.clone())
            }
            _ => None,
        };
        if let Some(name) = now {
            screens[at].output = Output::Display(name);
            changed = true;
        }
    }
    changed
}

/// Why a screen chosen for a display the operator works on shows nowhere, in words.
fn kept_in_words(kept: Kept, display: &str) -> String {
    match kept {
        Kept::Only => tf(
            "screens.kept.only",
            "\"{display}\" is the only display, where you work with SoftPresenter's own window. Plug in a TV or projector and choose it.",
            &[("display", display)],
        ),
        Kept::Main => tf(
            "screens.kept.main",
            "\"{display}\" is the main display, where SoftPresenter's own window is. Choose another display.",
            &[("display", display)],
        ),
        Kept::HomeWindow => tf(
            "screens.kept.home_window",
            "\"{display}\" is the display SoftPresenter's own window is on. Choose another display, or move the window.",
            &[("display", display)],
        ),
    }
}

/// Where each screen goes, in the list's order. `operator` are the displays the operator works
/// on (the home window's and the one with the menu bar), which Automatic never takes. A display
/// is filled by one screen only, an NDI name is used once, and a DeckLink output plays one
/// screen: the first screen in the list that asks gets it. A display that is not connected is
/// never replaced by another, so a screen can never land on the operator's display by surprise.
pub fn place_all(screens: &[Screen], displays: &[DisplayInfo], operator: &[usize]) -> Vec<Placed> {
    let mut placed: Vec<Placed> = Vec::with_capacity(screens.len());
    for screen in screens {
        // Switched off, a display or window screen shows nowhere (and takes no display from a
        // screen after it); one on NDI or a DeckLink output is placed as usual, and sends black.
        if !screen.on && !matches!(&screen.output, Output::Ndi(_) | Output::DeckLink(_) | Output::Off) {
            placed.push(Placed::Nothing(t("screens.status.switched_off", "Switched off").to_owned()));
            continue;
        }
        let wanted = match &screen.output {
            Output::Automatic => match choose_output(displays, operator, None) {
                Ok(Placement::Fullscreen(index)) => Placed::Fullscreen(index),
                _ => Placed::Window,
            },
            Output::Display(name) => match displays.iter().find(|display| display.is_named(name)) {
                // The operator works there (the main display, the only one, or where the home
                // window is when no display is known as the main one): a screen never covers it.
                Some(display) => match kept_for_operator(displays, operator, display.index) {
                    Some(kept) => Placed::Nothing(kept_in_words(kept, &display.name)),
                    None => Placed::Fullscreen(display.index),
                },
                None => Placed::Nothing(tf(
                    "screens.place.disconnected",
                    "Disconnected: \"{name}\" is not plugged in.",
                    &[("name", name)],
                )),
            },
            Output::Window => Placed::Window,
            Output::Ndi(name) if name.trim().is_empty() => {
                Placed::Nothing(t("screens.place.ndi_unnamed", "Give the NDI output a name.").to_owned())
            }
            Output::Ndi(name) => Placed::Ndi(name.trim().to_owned()),
            Output::DeckLink(out) => Placed::DeckLink(out.clone()),
            Output::Off => Placed::Nothing(String::new()),
        };
        let taken = placed.iter().position(|earlier| match (earlier, &wanted) {
            (Placed::DeckLink(one), Placed::DeckLink(two)) => one.same_device(two),
            (Placed::Fullscreen(_) | Placed::Ndi(_), _) => *earlier == wanted,
            _ => false,
        });
        placed.push(match (taken, &wanted) {
            (Some(by), Placed::Fullscreen(_)) => Placed::Nothing(tf(
                "screens.place.display_taken",
                "\"{name}\" is already on that display.",
                &[("name", &screens[by].name)],
            )),
            (Some(by), Placed::DeckLink(_)) => Placed::Nothing(tf(
                "screens.place.decklink_taken",
                "\"{name}\" already plays out of that DeckLink output.",
                &[("name", &screens[by].name)],
            )),
            (Some(by), _) => Placed::Nothing(tf(
                "screens.place.ndi_taken",
                "\"{name}\" already uses that NDI name.",
                &[("name", &screens[by].name)],
            )),
            (None, _) => wanted,
        });
    }
    placed
}

/// What a placed screen is doing, in words for the operator.
pub fn describe(placed: &Placed, displays: &[DisplayInfo]) -> String {
    match placed {
        Placed::Fullscreen(index) => match displays.iter().find(|display| display.index == *index) {
            Some(display) => tf(
                "screens.status.filling",
                "Filling \"{name}\", {w} x {h}",
                &[("name", &display.name), ("w", &display.w.to_string()), ("h", &display.h.to_string())],
            ),
            None => t("screens.status.filling_display", "Filling a display").to_owned(),
        },
        Placed::Window => t("screens.status.window", "In a window").to_owned(),
        Placed::Ndi(name) => tf("screens.status.ndi_starting", "Starting NDI as \"{name}\"", &[("name", name)]),
        Placed::DeckLink(out) => tf(
            "screens.status.decklink_starting",
            "Starting {device} at {mode}",
            &[("device", &out.device), ("mode", &out.mode)],
        ),
        Placed::Nothing(why) if why.is_empty() => t("screens.status.off", "Off").to_owned(),
        Placed::Nothing(why) => why.clone(),
    }
}

/// A screen's status as `sp` gives it: in English whatever the window speaks. A sentence the
/// window said is put back by `i18n::english_of`; the few words it leaves alone (a status of
/// one word in the language set), here.
pub fn status_in_english(doing: &str) -> String {
    for (key, english) in [
        crate::i18n::tk("screens.status.off", "Off"),
        crate::i18n::tk("screens.status.switched_off", "Switched off"),
        crate::i18n::tk("screens.status.off_black", "Off \u{b7} sending black"),
        crate::i18n::tk("screens.status.window", "In a window"),
    ] {
        if doing == t(key, english) {
            return english.to_owned();
        }
    }
    crate::i18n::english_of(doing)
}

/// What a DeckLink screen that plays says of itself, in the window's language: the driver's
/// "1080i50" or "1080i50, fill and key" (`sp_decklink::State::Playing`) after "Playing at".
pub fn decklink_playing_words(what: &str) -> String {
    match what.split_once(", ") {
        Some((mode, "fill and key")) => {
            tf("screens.status.decklink_playing.fill_and_key", "Playing at {mode}, fill and key", &[("mode", mode)])
        }
        Some((mode, "keyed over its input")) => tf(
            "screens.status.decklink_playing.over_input",
            "Playing at {mode}, keyed over its input",
            &[("mode", mode)],
        ),
        _ => tf("screens.status.decklink_playing", "Playing at {what}", &[("what", what)]),
    }
}

/// What the operator is told when NDI isn't installed, in the window's language: the words of
/// `sp_ndi::NOT_INSTALLED`, which the log and `sp` keep as they are.
pub fn ndi_not_installed() -> &'static str {
    if cfg!(target_os = "linux") {
        t(
            "screens.ndi.not_installed.linux",
            "NDI isn't installed on this computer. Download NDI\u{2026} in the Screens window gets it (free, from NDI).",
        )
    } else {
        t(
            "screens.ndi.not_installed.tools",
            "NDI isn't installed on this computer. Install NDI Tools (free, from https://ndi.video/), then turn this screen on again.",
        )
    }
}

/// How the DeckLink driver stands, in the window's language: the words of
/// `sp_decklink::Driver::in_words`, which `sp decklink` keeps in English. A failure the driver
/// describes itself stays in its own words.
pub fn decklink_driver_words(driver: &sp_decklink::Driver) -> String {
    use sp_decklink::Driver;
    match driver {
        Driver::Looking => t("screens.decklink.looking", "Looking for DeckLink devices\u{2026}").to_owned(),
        Driver::NotInstalled => t(
            "screens.decklink.not_installed",
            "Desktop Video (Blackmagic's DeckLink driver) isn't installed on this computer.",
        )
        .to_owned(),
        Driver::TooOld(version) => tf(
            "screens.decklink.too_old",
            "Desktop Video {version} is installed; SoftPresenter needs 16.0 or later. Install the latest from Blackmagic Design.",
            &[("version", version)],
        ),
        Driver::Unsupported => {
            t("screens.decklink.unsupported", "DeckLink output isn't available on this kind of computer yet.")
                .to_owned()
        }
        Driver::Failed(_) | Driver::Ready(_) => driver.in_words(),
    }
}

/// A DeckLink screen's format and where it comes from, in the window's language (the words of
/// `sp_decklink::Device::format_in_words`, which `sp` keeps in English): "1080p30, as Desktop
/// Video Setup has it"; "720p50, set in SoftPresenter: Desktop Video Setup has 1080p30".
pub fn decklink_format_words(device: &sp_decklink::Device, mode: &str) -> String {
    match device.start(false) {
        Some(start) if start.mode.name.eq_ignore_ascii_case(mode.trim()) => {
            let name = start.mode.name.as_str();
            if start.from_card() {
                return tf("screens.decklink.format.card", "{mode}, as Desktop Video Setup has it", &[("mode", name)]);
            }
            let card = card_format_words(&start.card);
            let args = [("mode", name), ("card", card.as_str())];
            match name {
                "1080i59.94" => tf("screens.decklink.format.usual", "{mode}, the usual HD format: {card}", &args),
                "1080i50" => tf(
                    "screens.decklink.format.usual_50",
                    "{mode}, the usual HD format where power is 50 Hz: {card}",
                    &args,
                ),
                _ if start.mode.height == 1080 => tf(
                    "screens.decklink.format.first_1080",
                    "{mode}, this output's first 1080-line format: {card}",
                    &args,
                ),
                _ => tf("screens.decklink.format.first", "{mode}, this output's first format: {card}", &args),
            }
        }
        _ => tf(
            "screens.decklink.format.set",
            "{mode}, set in SoftPresenter: {card}",
            &[("mode", mode.trim()), ("card", &card_format_words(&device.card_format(false)))],
        ),
    }
}

/// What Desktop Video Setup has for a device, in the window's language (the words of
/// `sp_decklink::CardFormat::in_words`).
fn card_format_words(card: &sp_decklink::CardFormat) -> String {
    use sp_decklink::CardFormat;
    let name = [("name", card.name().unwrap_or_default())];
    match card {
        CardFormat::NotSet => t("screens.decklink.card.none", "Desktop Video Setup sets none").to_owned(),
        CardFormat::Usable(_) => tf("screens.decklink.card.has", "Desktop Video Setup has {name}", &name),
        CardFormat::NotListed(code) => tf(
            "screens.decklink.card.not_listed",
            "Desktop Video Setup has a format this output doesn't list ({code})",
            &[("code", &sp_decklink::code_in_words(*code))],
        ),
        CardFormat::NotOffered(_) => tf(
            "screens.decklink.card.not_offered",
            "Desktop Video Setup has {name}, which SoftPresenter doesn't offer",
            &name,
        ),
        CardFormat::ThreeD(_) => tf(
            "screens.decklink.card.three_d",
            "Desktop Video Setup has {name} in 3D, which SoftPresenter doesn't play",
            &name,
        ),
        CardFormat::CantKey(_) => tf(
            "screens.decklink.card.cant_key",
            "Desktop Video Setup has {name}, which this output can't key in",
            &name,
        ),
    }
}

const FORMAT: &str = "softpresenter-screens";
/// What a screens file saved before the project was renamed says it is; it still opens.
const OLD_FORMAT: &str = "presentercraft-screens";

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    screens: Vec<Screen>,
}

pub fn to_json(screens: &[Screen]) -> String {
    let file = File { format: FORMAT.to_owned(), version: 1, screens: screens.to_vec() };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads the list (which may list none). The error, for the log, says why a file isn't a
/// screens file this build reads.
pub fn from_json(text: &str) -> Result<Vec<Screen>, String> {
    let file: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.format != FORMAT && file.format != OLD_FORMAT {
        return Err(format!("it is not a screens file ({:?})", file.format));
    }
    if file.version != 1 {
        return Err(format!("it is version {} of the screens file, made by a newer SoftPresenter", file.version));
    }
    Ok(file.screens)
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("screens.json"))
}

/// The saved list, if there is one that lists any; and a file there that couldn't be read,
/// kept aside (`whole_file`).
pub fn load() -> (Option<Vec<Screen>>, Option<crate::whole_file::Unread>) {
    file().map_or((None, None), |path| load_from(&path))
}

fn load_from(path: &std::path::Path) -> (Option<Vec<Screen>>, Option<crate::whole_file::Unread>) {
    match crate::whole_file::read(path, crate::whole_file::What::Screens, from_json) {
        Ok(screens) => (screens.filter(|screens| !screens.is_empty()), None),
        Err(unread) => (None, Some(unread)),
    }
}

pub fn save(screens: &[Screen]) -> Result<(), String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    save_to(&path, screens)
}

fn save_to(path: &std::path::Path, screens: &[Screen]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            tf(
                "media.no_folder",
                "Could not make the app's folder: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })?;
    }
    crate::whole_file::write(path, to_json(screens).as_bytes()).map_err(|e| {
        tf("screens.not_saved", "Could not save the screens: {error}", &[("error", &crate::notices::write_problem(&e))])
    })
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_screens_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let good = to_json(&[Screen::new("Stream".to_owned(), Output::Ndi("Sanctuary".to_owned()))]);
        crate::whole_file::tests::check_loader(
            "screens.json",
            crate::whole_file::What::Screens,
            good.as_bytes(),
            |path| {
                let (screens, unread) = load_from(path);
                assert_eq!(screens.is_some(), unread.is_none(), "a good file's screens, or none");
                unread
            },
            |path| save_to(path, &starting(None)).unwrap(),
        );
        // A file listing no screens is no damage: the screens are made anew, as with no file.
        let dir = crate::whole_file::tests::TestDir::new("no-screens");
        let path = dir.0.join("screens.json");
        std::fs::write(&path, to_json(&[])).unwrap();
        let (screens, unread) = load_from(&path);
        assert!(screens.is_none() && unread.is_none());
        assert!(path.exists());
    }

    #[test]
    fn a_screens_own_theme_is_saved_and_one_without_says_nothing() {
        let mut stream = Screen::new("Stream".to_owned(), Output::Window);
        let plain = serde_json::to_string(&stream).unwrap();
        assert!(!plain.contains("theme"), "no theme of its own is not written");
        stream.theme = Some(Uuid::from_u128(7));
        let back: Screen = serde_json::from_str(&serde_json::to_string(&stream).unwrap()).unwrap();
        assert_eq!(back.theme, Some(Uuid::from_u128(7)));
        let old: Screen = serde_json::from_str(&plain).unwrap();
        assert_eq!(old.theme, None);
    }

    use super::*;

    #[test]
    fn an_ndi_screen_sends_29_97_pictures_a_second_unless_another_rate_is_chosen() {
        // A screen saved before the rate could be chosen sends 29.97, as a new one does.
        let old: Screen = serde_json::from_str(
            r#"{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Stream","output":{"ndi":"Sanctuary"}}"#,
        )
        .unwrap();
        assert_eq!(old.rate, NdiRate::R29_97);
        assert_eq!(old.rate.rate(), sp_ndi::Rate { n: 30_000, d: 1001 });
        let mut stream = Screen::new("Stream".to_owned(), Output::Ndi("Sanctuary".to_owned()));
        assert!(!serde_json::to_string(&stream).unwrap().contains("rate"), "the usual rate is not written");
        // Each rate is kept by its name, and sent as its exact fraction.
        for rate in NdiRate::ALL {
            stream.rate = rate;
            let saved = serde_json::to_string(&stream).unwrap();
            let back: Screen = serde_json::from_str(&saved).unwrap();
            assert_eq!(back.rate, rate, "{saved}");
            assert_eq!(NdiRate::parse(&rate.name()), Some(rate));
        }
        stream.rate = NdiRate::R59_94;
        assert!(serde_json::to_string(&stream).unwrap().contains(r#""rate":"59.94""#));
        assert_eq!(NdiRate::R59_94.rate(), sp_ndi::Rate { n: 60_000, d: 1001 });
        assert_eq!(NdiRate::parse("24"), None);
    }

    #[test]
    fn a_screen_sends_sound_unless_switched_off_and_only_off_is_written() {
        // A screen saved before sound could be sent sends it.
        let old: Screen = serde_json::from_str(
            r#"{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Stream","output":{"ndi":"Sanctuary"}}"#,
        )
        .unwrap();
        assert!(old.sound);
        let mut stream = Screen::new("Stream".to_owned(), Output::Ndi("Sanctuary".to_owned()));
        assert!(stream.sound);
        assert!(!serde_json::to_string(&stream).unwrap().contains("sound"), "on is not written");
        stream.sound = false;
        let saved = serde_json::to_string(&stream).unwrap();
        assert!(saved.contains(r#""sound":false"#), "{saved}");
        let back: Screen = serde_json::from_str(&saved).unwrap();
        assert!(!back.sound);
        assert_eq!(back, stream);
    }

    #[test]
    fn a_screen_without_a_look_shows_everything_and_a_look_survives_its_file() {
        let old: Screen =
            serde_json::from_str(r#"{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Main","output":"off"}"#)
                .unwrap();
        assert!(old.look.is_default());
        let lobby = Screen {
            look: Look { slides: false, props: false, mask: [10.0, 0.0, 10.0, 0.0], ..Look::default() },
            ..Screen::new("Lobby".to_owned(), Output::Off)
        };
        let back: Screen = serde_json::from_str(&serde_json::to_string(&lobby).unwrap()).unwrap();
        assert_eq!(back, lobby);
        assert!(!back.look.slides && back.look.media);
    }

    #[test]
    fn a_see_through_screen_and_a_key_survive_their_file_and_old_files_open_solid() {
        // A file from before see-through screens: every screen solid, none a key.
        let old = r#"{"format":"softpresenter-screens","version":1,"screens":[
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Main","output":"window"},
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e02","name":"Lobby","output":"off",
             "look":{"slides":false,"media":true,"props":true,"messages":true,"announcements":true,
                     "color":{"r":0,"g":0,"b":0,"a":255},"mask":[0.0,0.0,0.0,0.0]}}]}"#;
        let screens = from_json(old).expect("an old file opens");
        assert!(screens.iter().all(|s| s.look.see_through.is_none() && s.look.key_of.is_none()));
        assert!(!screens[1].look.slides, "and keeps what it said");
        // A solid screen writes nothing of either.
        let plain = serde_json::to_string(&Screen::new("A".to_owned(), Output::Window)).unwrap();
        assert!(!plain.contains("see_through") && !plain.contains("key_of"), "{plain}");
        // A see-through screen and its key, there and back.
        let stream = Screen {
            look: Look { see_through: Some(Alpha::Premultiplied), media: false, ..Look::default() },
            ..Screen::new("Stream".to_owned(), Output::Display("Switcher 1".to_owned()))
        };
        let key = Screen {
            look: Look { key_of: Some(stream.id), ..Look::default() },
            ..Screen::new("Stream key".to_owned(), Output::Display("Switcher 2".to_owned()))
        };
        let text = to_json(&[stream.clone(), key.clone()]);
        assert!(text.contains("\"see_through\": \"premultiplied\""), "{text}");
        assert_eq!(from_json(&text), Ok(vec![stream.clone(), key.clone()]));
        // Straight is the usual kind; every kind reads back by its name.
        assert_eq!(Alpha::default(), Alpha::Straight);
        for alpha in [Alpha::Straight, Alpha::Premultiplied] {
            assert_eq!(Alpha::parse(alpha.name()), Some(alpha));
        }
        assert_eq!(Alpha::parse("Premultiplied "), Some(Alpha::Premultiplied));
        assert_eq!(Alpha::parse("shaped"), None);
        // Show Everything gives back every layer but keeps the background, so a stream is
        // never made solid by it.
        assert_eq!(stream.look.everything(), Look { see_through: Some(Alpha::Premultiplied), ..Look::default() });
        assert!(!stream.look.shows_everything() && stream.look.everything().shows_everything());
        assert!(!stream.look.everything().is_default(), "still see-through, so still sent to the screens");
        assert_eq!(
            background_words(&[stream.clone(), key.clone()], 0).as_deref(),
            Some("see-through, premultiplied alpha")
        );
        assert_eq!(background_words(&[stream, key], 1).as_deref(), Some("key of \"Stream\""));
    }

    #[test]
    fn a_key_says_why_it_shows_black_whenever_it_cannot_follow_its_fill() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let see_through = Look { see_through: Some(Alpha::Straight), ..Look::default() };
            let fill = Screen { look: see_through, ..Screen::new("Stream".to_owned(), Output::Window) };
            let key_of = |id| Look { key_of: Some(id), ..Look::default() };
            let key = Screen { look: key_of(fill.id), ..Screen::new("Key".to_owned(), Output::Window) };
            let trouble = |screens: &[Screen], shown: &[bool]| key_trouble(screens, screens.len() - 1, shown);
            // Two windows (or displays), both going out: all is well.
            assert_eq!(trouble(&[fill.clone(), key.clone()], &[true, true]), None);
            assert_eq!(key_trouble(&[fill.clone(), key.clone()], 0, &[true, true]), None, "the fill is no key");
            // Both on NDI: all is well too.
            let ndi = |screen: &Screen, name: &str| Screen { output: Output::Ndi(name.to_owned()), ..screen.clone() };
            assert_eq!(trouble(&[ndi(&fill, "Fill"), ndi(&key, "Key")], &[true, true]), None);
            // One on NDI and one in a window: they could not show the same moment.
            let mixed = trouble(&[ndi(&fill, "Fill"), key.clone()], &[true, true]).unwrap();
            assert!(mixed.contains("is on NDI") && mixed.contains("shows black"), "{mixed}");
            let mixed = trouble(&[fill.clone(), ndi(&key, "Key")], &[true, true]).unwrap();
            assert!(mixed.contains("display or window"), "{mixed}");
            // A DeckLink output goes with NDI: both are given the same pictures read back.
            let card = |screen: &Screen, device: &str| Screen {
                output: Output::DeckLink(DeckLinkOut {
                    device: device.to_owned(),
                    id: None,
                    mode: "1080i50".to_owned(),
                    keyer: Keyer::Off,
                }),
                ..screen.clone()
            };
            assert_eq!(trouble(&[card(&fill, "Duo (1)"), card(&key, "Duo (2)")], &[true, true]), None);
            assert_eq!(trouble(&[ndi(&fill, "Fill"), card(&key, "Duo (2)")], &[true, true]), None);
            let mixed = trouble(&[card(&fill, "Duo (1)"), key.clone()], &[true, true]).unwrap();
            assert!(mixed.contains("is on a DeckLink output"), "{mixed}");
            // The fill not going out (off, or its display unplugged).
            let off = trouble(&[fill.clone(), key.clone()], &[false, true]).unwrap();
            assert!(off.contains("isn't going out"), "{off}");
            // A solid fill: its key is all white.
            let solid = Screen { look: Look::default(), ..fill.clone() };
            assert!(trouble(&[solid, key.clone()], &[true, true]).unwrap().contains("all white"));
            // Its own key, a stage screen's, another key's, or one that has gone.
            let own = Screen { look: key_of(key.id), ..key.clone() };
            assert!(trouble(&[fill.clone(), own], &[true, true]).unwrap().contains("own key"));
            let stage = Screen { look: see_through, ..Screen::stage("Band".to_owned(), Output::Window) };
            let of_stage = Screen { look: key_of(stage.id), ..key.clone() };
            assert!(trouble(&[stage, of_stage], &[true, true]).unwrap().contains("stage screen"));
            let other_key = Screen { look: key_of(fill.id), ..Screen::new("Key 1".to_owned(), Output::Window) };
            let of_key = Screen { look: key_of(other_key.id), ..key.clone() };
            assert!(trouble(&[fill.clone(), other_key, of_key], &[true, true, true]).unwrap().contains("a key itself"));
            assert!(trouble(std::slice::from_ref(&key), &[true]).unwrap().contains("removed"));
            // A stage screen is never a key, whatever its file says.
            let stage_key = Screen { look: key_of(fill.id), ..Screen::stage("Band".to_owned(), Output::Window) };
            assert_eq!(trouble(&[fill, stage_key], &[true, true]), None);
        });
    }

    fn displays(names: &[&str]) -> Vec<DisplayInfo> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| DisplayInfo {
                index,
                name: (*name).to_owned(),
                w: 1920,
                h: 1080,
                ..Default::default()
            })
            .collect()
    }

    fn screen(name: &str, output: Output) -> Screen {
        Screen::new(name.to_owned(), output)
    }

    #[test]
    fn automatic_takes_the_other_display_or_a_window() {
        let one = [screen("A", Output::Automatic)];
        assert_eq!(place_all(&one, &displays(&["Built-in"]), &[0]), [Placed::Window]);
        assert_eq!(place_all(&one, &displays(&["Built-in", "Projector"]), &[0]), [Placed::Fullscreen(1)]);
        assert_eq!(place_all(&one, &displays(&["Built-in", "Projector"]), &[1]), [Placed::Fullscreen(0)]);
    }

    #[test]
    fn no_screen_fills_the_main_display_and_old_display_names_still_match() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let displays = vec![
                DisplayInfo {
                    index: 0,
                    name: "HP 27f".into(),
                    old_name: "Monitor #13643".into(),
                    main: true,
                    ..Default::default()
                },
                DisplayInfo { index: 1, name: "3DTV".into(), old_name: "Monitor #0".into(), ..Default::default() },
            ];
            let on_main = [screen("A", Output::Display("Monitor #13643".to_owned()))];
            let placed = place_all(&on_main, &displays, &[0]);
            assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("main display")), "{placed:?}");
            let on_tv = [screen("A", Output::Display("Monitor #0".to_owned()))];
            assert_eq!(place_all(&on_tv, &displays, &[0]), [Placed::Fullscreen(1)], "saved under its old name");
            let automatic = [screen("A", Output::Automatic)];
            assert_eq!(place_all(&automatic, &displays, &[]), [Placed::Window], "not knowing the operator's display");
            assert_eq!(place_all(&automatic, &displays, &[0]), [Placed::Fullscreen(1)]);
        });
    }

    /// Two monitors on Windows as `display_list` lists them: the main one, and a TV that gives
    /// its own name; and a third that gives none.
    fn windows_displays() -> Vec<DisplayInfo> {
        let display = |index: usize, name: &str, device: &str| DisplayInfo {
            index,
            name: name.to_owned(),
            old_name: device.to_owned(),
            main: index == 0,
            w: 1920,
            h: 1080,
            ..Default::default()
        };
        vec![
            display(0, "Philips FTV", r"\\.\DISPLAY1"),
            display(1, "Mi TV", r"\\.\DISPLAY2"),
            display(2, r"\\.\DISPLAY3", r"\\.\DISPLAY3"),
        ]
    }

    #[test]
    fn a_screen_saved_under_a_windows_device_name_finds_its_display_and_takes_its_name() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let displays = windows_displays();
            let mut screens = vec![
                screen("Audience", Output::Display(r"\\.\DISPLAY2".to_owned())),
                screen("Stage", Output::Display(r"\\.\DISPLAY3".to_owned())),
                screen("Lobby", Output::Display(r"\\.\DISPLAY1".to_owned())),
            ];
            let placed = place_all(&screens, &displays, &[0]);
            assert_eq!(placed[..2], [Placed::Fullscreen(1), Placed::Fullscreen(2)]);
            assert!(matches!(&placed[2], Placed::Nothing(why) if why.contains("main display")), "{placed:?}");
            // Saved from now on under the names Windows shows; a display with no name of its own
            // keeps its device name.
            assert!(settle(&mut screens, &displays, &[0]));
            let outputs: Vec<&Output> = screens.iter().map(|s| &s.output).collect();
            assert_eq!(
                outputs,
                [
                    &Output::Display("Mi TV".to_owned()),
                    &Output::Display(r"\\.\DISPLAY3".to_owned()),
                    &Output::Display("Philips FTV".to_owned()),
                ]
            );
            assert!(!settle(&mut screens, &displays, &[0]), "nothing more to change");
            assert_eq!(place_all(&screens, &displays, &[0])[..2], [Placed::Fullscreen(1), Placed::Fullscreen(2)]);
        });
    }

    #[test]
    fn a_screen_on_a_monitors_own_name_waits_for_it_and_is_kept() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            // Mi TV unplugged: the screen waits, under the same name, and nothing takes its place.
            let mut displays = windows_displays();
            displays.remove(1);
            let mut screens = vec![screen("Audience", Output::Display("Mi TV".to_owned()))];
            assert!(!settle(&mut screens, &displays, &[0]));
            assert_eq!(screens[0].output, Output::Display("Mi TV".to_owned()));
            let placed = place_all(&screens, &displays, &[0]);
            assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
        });
    }

    #[test]
    fn automatic_is_given_the_first_free_display_by_its_name() {
        let displays = windows_displays();
        let mut screens =
            vec![screen("Audience", Output::Display(r"\\.\DISPLAY2".to_owned())), screen("Stage", Output::Automatic)];
        assert!(settle(&mut screens, &displays, &[0]));
        assert_eq!(screens[1].output, Output::Display(r"\\.\DISPLAY3".to_owned()), "Mi TV is taken");
    }

    #[test]
    fn a_lone_display_is_the_operators_and_no_screen_fills_it() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            // Wayland with one display: no display is the main one, and the home window's display may
            // or may not be known yet.
            let one = displays(&["DP-1"]);
            for operator in [&[][..], &[0]] {
                assert_eq!(place_all(&[screen("A", Output::Automatic)], &one, operator), [Placed::Window]);
                let chosen = place_all(&[screen("A", Output::Display("DP-1".to_owned()))], &one, operator);
                assert!(
                    matches!(&chosen[0], Placed::Nothing(why) if why.contains("\"DP-1\" is the only display")),
                    "{chosen:?}"
                );
                // Automatic is never given the one display to keep.
                let mut list = [screen("A", Output::Automatic)];
                assert!(!settle(&mut list, &one, operator));
                assert_eq!(list[0].output, Output::Automatic);
            }
        });
    }

    #[test]
    fn with_a_known_main_display_a_chosen_display_is_kept_wherever_the_home_window_goes() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut known = displays(&["Built-in", "Projector", "TV"]);
            known[0].main = true;
            let on_tv = [screen("A", Output::Display("TV".to_owned()))];
            // The operator dragged the home window onto the TV: the audience screen stays.
            assert_eq!(place_all(&on_tv, &known, &[2, 0]), [Placed::Fullscreen(2)]);
            let on_main = place_all(&[screen("A", Output::Display("Built-in".to_owned()))], &known, &[0]);
            assert!(matches!(&on_main[0], Placed::Nothing(why) if why.contains("main display")), "{on_main:?}");
            // Automatic takes neither the main display nor the one holding the home window.
            let mut list = [screen("A", Output::Automatic)];
            assert!(settle(&mut list, &known, &[1, 0]));
            assert_eq!(list[0].output, Output::Display("TV".to_owned()));
        });
    }

    #[test]
    fn with_no_known_main_display_the_home_windows_is_never_covered() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            // Wayland with a projector plugged in.
            let two = displays(&["DP-1", "HDMI-1"]);
            let on_home = place_all(&[screen("A", Output::Display("DP-1".to_owned()))], &two, &[0]);
            assert!(matches!(&on_home[0], Placed::Nothing(why) if why.contains("own window is on")), "{on_home:?}");
            assert_eq!(
                place_all(&[screen("A", Output::Display("HDMI-1".to_owned()))], &two, &[0]),
                [Placed::Fullscreen(1)]
            );
            // The audience appears on the projector as soon as it is plugged in...
            let mut list = [screen("A", Output::Automatic)];
            assert!(settle(&mut list, &two, &[0]));
            assert_eq!(list[0].output, Output::Display("HDMI-1".to_owned()));
            // ...but not while it isn't known which display the home window is on: a window then.
            let mut list = [screen("A", Output::Automatic)];
            assert!(!settle(&mut list, &two, &[]));
            assert_eq!(place_all(&list, &two, &[]), [Placed::Window]);
            // A display another screen has is not given again; old names become the new ones.
            let mut list = [screen("A", Output::Display("HDMI-1".to_owned())), screen("B", Output::Automatic)];
            assert!(!settle(&mut list, &two, &[0]));
            assert_eq!(list[1].output, Output::Automatic);
            let renamed = vec![
                DisplayInfo { index: 0, name: "HP 27f".into(), main: true, ..Default::default() },
                DisplayInfo { index: 1, name: "3DTV".into(), old_name: "Monitor #0".into(), ..Default::default() },
            ];
            let mut list = [screen("A", Output::Display("Monitor #0".to_owned()))];
            assert!(settle(&mut list, &renamed, &[0]));
            assert_eq!(list[0].output, Output::Display("3DTV".to_owned()));
        });
    }

    #[test]
    fn a_display_that_is_not_plugged_in_is_never_swapped_for_another() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let screens = [screen("A", Output::Display("Projector".to_owned()))];
            let placed = place_all(&screens, &displays(&["Built-in"]), &[0]);
            assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
            // Plugged in again, it is found by name wherever it now sits in the list.
            assert_eq!(place_all(&screens, &displays(&["Projector", "Built-in"]), &[1]), [Placed::Fullscreen(0)]);
        });
    }

    fn decklink(device: &str, id: Option<i64>) -> Output {
        Output::DeckLink(DeckLinkOut {
            device: device.to_owned(),
            id,
            mode: "1080i59.94".to_owned(),
            keyer: Keyer::Off,
        })
    }

    #[test]
    fn a_screen_switched_off_keeps_where_it_goes_and_comes_back_as_it_was() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let keyed = Output::DeckLink(DeckLinkOut {
                device: "DeckLink Duo (1)".to_owned(),
                id: Some(7),
                mode: "1080p29.97".to_owned(),
                keyer: Keyer::External,
            });
            let mut screens = vec![
                screen("Projector", Output::Display("Projector".to_owned())),
                screen("Stream", Output::Ndi("Church Stream".to_owned())),
                screen("Switcher", keyed.clone()),
            ];
            let all = displays(&["Built-in", "Projector"]);
            let before = place_all(&screens, &all, &[0]);
            assert_eq!(before[0], Placed::Fullscreen(1));
            for screen in &mut screens {
                screen.choose_output(Output::Off);
            }
            assert!(screens.iter().all(|screen| !screen.on && screen.is_off()));
            assert_eq!(screens[0].output, Output::Display("Projector".to_owned()), "kept");
            assert_eq!(screens[1].output, Output::Ndi("Church Stream".to_owned()), "kept");
            assert_eq!(screens[2].output, keyed, "its format and keyer kept");
            let off = place_all(&screens, &all, &[0]);
            // The display shows nothing; NDI and DeckLink go on, drawn black, so receivers and
            // switchers keep them as sources.
            assert_eq!(off[0], Placed::Nothing("Switched off".to_owned()));
            assert_eq!(off[1], Placed::Ndi("Church Stream".to_owned()));
            assert!(
                matches!(&off[2], Placed::DeckLink(out) if out.mode == "1080p29.97" && out.keyer == Keyer::External)
            );
            assert_eq!(screens.iter().map(Screen::sends_while_off).collect::<Vec<_>>(), [false, true, true]);
            // On again, by choosing where it went (the Screens window), or by its switch (the card).
            screens[0].choose_output(Output::Display("Projector".to_owned()));
            screens[1].choose_output(Output::Ndi("SoftPresenter Stream".to_owned()));
            screens[2].on = true;
            assert_eq!(place_all(&screens, &all, &[0]), before, "back exactly as it was");
            assert_eq!(screens[1].output, Output::Ndi("Church Stream".to_owned()), "its own NDI name");
            // Choosing somewhere else sends it there, switched on.
            screens[0].on = false;
            screens[0].choose_output(Output::Window);
            assert!(screens[0].on && screens[0].output == Output::Window);
            // A switched-off display screen takes no display from a screen after it.
            let mut two = vec![
                screen("A", Output::Display("Projector".to_owned())),
                screen("B", Output::Display("Projector".to_owned())),
            ];
            two[0].on = false;
            assert_eq!(place_all(&two, &all, &[0])[1], Placed::Fullscreen(1));
        });
    }

    #[test]
    fn a_screens_switch_is_saved_only_when_off_and_older_files_open_as_before() {
        // Saved before a screen had a switch: on. Saved going nowhere: still nowhere.
        let old: Vec<Screen> = from_json(
            r#"{"format":"softpresenter-screens","version":1,"screens":[
                {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Main","output":{"display":"HP 27f"}},
                {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e02","name":"Spare","output":"off"}]}"#,
        )
        .expect("an older file");
        assert!(old[0].on && !old[0].is_off());
        assert!(old[1].on && old[1].is_off() && old[1].output == Output::Off);
        assert_eq!(place_all(&old[1..], &[], &[]), [Placed::Nothing(String::new())], "goes nowhere, as before");
        // On is not written; off is, and comes back.
        let mut stream = screen("Stream", Output::Ndi("Church Stream".to_owned()));
        assert!(!serde_json::to_string(&stream).unwrap().contains("\"on\""));
        stream.on = false;
        let saved = to_json(std::slice::from_ref(&stream));
        assert!(saved.contains("\"on\": false") || saved.contains("\"on\":false"), "{saved}");
        let back = from_json(&saved).expect("its own file");
        assert_eq!(back[0], stream);
    }

    #[test]
    fn a_decklink_output_survives_its_file_and_saves_only_what_it_needs() {
        let screen = Screen::new("Switcher".to_owned(), decklink("DeckLink Duo (1)", Some(42)));
        let text = to_json(std::slice::from_ref(&screen));
        assert!(text.contains("\"decklink\"") && text.contains("\"mode\": \"1080i59.94\""), "{text}");
        assert!(!text.contains("keyer"), "a keyer that is off is not written");
        assert_eq!(from_json(&text), Ok(vec![screen.clone()]));
        let mut keyed = Screen {
            output: Output::DeckLink(DeckLinkOut {
                id: None,
                ..match screen.output {
                    Output::DeckLink(out) => out,
                    _ => unreachable!(),
                }
            }),
            ..screen
        };
        keyed.set_keyer(Keyer::External, None);
        let text = to_json(std::slice::from_ref(&keyed));
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
        let out = &saved["screens"][0]["output"]["decklink"];
        assert_eq!(out["keyer"], "external");
        assert_eq!(saved["screens"][0]["look"]["see_through"], "premultiplied", "{text}");
        assert!(out.get("id").is_none(), "a device without an id is saved without one: {text}");
        assert_eq!(from_json(&text), Ok(vec![keyed]));
    }

    #[test]
    fn keying_a_decklink_screen_makes_it_see_through_and_premultiplied_and_off_makes_it_solid() {
        let mut screen = Screen::new("Lyrics".to_owned(), decklink("DeckLink Duo (1)", Some(1)));
        assert!(!screen.keyed());
        for keyer in [Keyer::External, Keyer::Internal] {
            screen.set_keyer(keyer, None);
            assert!(screen.keyed());
            assert_eq!(screen.look.see_through, Some(Alpha::Premultiplied), "{keyer:?}");
        }
        // An alpha given with the choice wins.
        screen.set_keyer(Keyer::External, Some(Alpha::Straight));
        assert_eq!(screen.look.see_through, Some(Alpha::Straight));
        // Off: a full picture, solid.
        screen.set_keyer(Keyer::Off, None);
        assert!(!screen.keyed());
        assert_eq!(screen.look.see_through, None);
        assert!(matches!(&screen.output, Output::DeckLink(out) if out.keyer == Keyer::Off));
        // A screen on anything else is left as it is.
        let mut stream = Screen::new("Stream".to_owned(), Output::Ndi("Sanctuary".to_owned()));
        stream.look.see_through = Some(Alpha::Straight);
        stream.set_keyer(Keyer::Off, None);
        assert_eq!(stream.look.see_through, Some(Alpha::Straight));
        // A stage screen on a keying output isn't keyed: its picture is always solid.
        let mut band = Screen::stage("Band".to_owned(), decklink("DeckLink Duo (1)", Some(1)));
        if let Output::DeckLink(out) = &mut band.output {
            out.keyer = Keyer::External;
        }
        assert!(!band.keyed());
    }

    #[test]
    fn a_keyed_screen_saved_solid_opens_see_through_and_premultiplied_once() {
        // The church's screen before keying and the background were one choice: fill and key,
        // left solid, so its key was all white. Beside it, a solid screen not keyed, a keyed one
        // already see-through with straight edges, and a stage screen on a keying output.
        let old = r#"{"format":"softpresenter-screens","version":1,"screens":[
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"Lyrics",
             "output":{"decklink":{"device":"DeckLink Duo (1)","mode":"1080p30","keyer":"external"}}},
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e02","name":"Room",
             "output":{"decklink":{"device":"DeckLink Duo (3)","mode":"1080p30"}}},
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e03","name":"Bible",
             "output":{"decklink":{"device":"DeckLink Duo (2)","mode":"1080p30","keyer":"internal"}},
             "look":{"see_through":"straight"}},
            {"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e04","name":"Band","kind":"stage",
             "output":{"decklink":{"device":"DeckLink Duo (4)","mode":"1080p30","keyer":"external"}}}]}"#;
        let mut screens = from_json(old).expect("an old file opens");
        let said = settle_keying(&mut screens);
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].starts_with("\"Lyrics\" is keyed by its DeckLink card but was solid"), "{said:?}");
        assert!(said[1].starts_with("\"Band\" is a stage screen") && said[1].contains("turned off"), "{said:?}");
        let backgrounds: Vec<Option<Alpha>> = screens.iter().map(|screen| screen.look.see_through).collect();
        assert_eq!(backgrounds, [Some(Alpha::Premultiplied), None, Some(Alpha::Straight), None]);
        let keyers: Vec<Keyer> = screens
            .iter()
            .map(|screen| match &screen.output {
                Output::DeckLink(out) => out.keyer,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(keyers, [Keyer::External, Keyer::Off, Keyer::Internal, Keyer::Off], "the stage screen's is off");
        // Saved so, it is never changed again.
        let mut reopened = from_json(&to_json(&screens)).unwrap();
        assert!(settle_keying(&mut reopened).is_empty());
        assert_eq!(reopened, screens);
    }

    #[test]
    fn one_decklink_output_plays_one_screen_and_another_card_of_the_same_name_is_another_output() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let screens = [
                screen("A", decklink("DeckLink Duo (1)", Some(1))),
                screen("B", decklink("DeckLink Duo (1)", Some(1))),
                screen("C", decklink("DeckLink Duo (1)", Some(2))),
                screen("D", decklink("DeckLink Duo (2)", None)),
                screen("E", decklink("DeckLink Duo (2)", None)),
            ];
            let placed = place_all(&screens, &displays(&["Built-in"]), &[0]);
            assert!(matches!(&placed[0], Placed::DeckLink(out) if out.id == Some(1)));
            assert!(
                matches!(&placed[1], Placed::Nothing(why) if why.contains("\"A\" already plays out of that DeckLink")),
                "{placed:?}"
            );
            assert!(matches!(&placed[2], Placed::DeckLink(_)), "the same name with another id is another card");
            assert!(matches!(&placed[3], Placed::DeckLink(_)));
            assert!(matches!(&placed[4], Placed::Nothing(_)));
            assert_eq!(describe(&placed[0], &[]), "Starting DeckLink Duo (1) at 1080i59.94");
        });
    }

    #[test]
    fn decklink_choices_are_the_devices_that_can_play_and_the_one_in_use_is_kept() {
        let none = sp_decklink::Found { driver: sp_decklink::Driver::NotInstalled, devices: Vec::new() };
        assert!(decklink_choices(&Output::Window, &none).is_empty(), "no DeckLink choices without the driver");
        let devices = vec![sp_decklink::Device {
            name: "DeckLink Duo (1)".to_owned(),
            model: "DeckLink Duo 2".to_owned(),
            id: Some(7),
            sub_device: Some(0),
            sub_devices: Some(4),
            profile: 0,
            duplex: 0,
            connections: 1,
            can_play: true,
            internal_keying: false,
            external_keying: false,
            modes: vec![sp_decklink::Mode::new(
                0,
                String::new(),
                (1920, 1080),
                (1000, 25000),
                u32::from_be_bytes(*b"uppr"),
                (true, true, false),
            )],
            card_mode: None,
            card_mode_3d: false,
        }];
        let found = sp_decklink::Found { driver: sp_decklink::Driver::Ready("16.3".to_owned()), devices };
        let choices = decklink_choices(&Output::Window, &found);
        assert_eq!(
            choices,
            vec![Output::DeckLink(DeckLinkOut {
                device: "DeckLink Duo (1)".to_owned(),
                id: Some(7),
                mode: "1080i50".to_owned(),
                keyer: Keyer::Off
            })]
        );
        let mine = Output::DeckLink(DeckLinkOut {
            mode: "720p50".to_owned(),
            ..match &choices[0] {
                Output::DeckLink(out) => out.clone(),
                _ => unreachable!(),
            }
        });
        assert_eq!(decklink_choices(&mine, &found), vec![mine.clone()], "kept with its own format");
        let gone = decklink("DeckLink 8K Pro (1)", Some(99));
        assert_eq!(decklink_choices(&gone, &found).last(), Some(&gone), "kept while it isn't connected");
    }

    #[test]
    fn a_new_decklink_screen_starts_in_the_cards_own_format_and_a_saved_one_keeps_its_own() {
        let card = |name: &str, id: i64, card_mode: Option<&[u8; 4]>| {
            let mode = |code: &[u8; 4], height, dominance: &[u8; 4], rate: (i64, i64)| {
                sp_decklink::Mode::new(
                    u32::from_be_bytes(*code),
                    String::new(),
                    (if height == 720 { 1280 } else { 1920 }, height),
                    rate,
                    u32::from_be_bytes(*dominance),
                    (true, true, false),
                )
            };
            sp_decklink::Device {
                name: name.to_owned(),
                model: "DeckLink Duo 2".to_owned(),
                id: Some(id),
                sub_device: Some(0),
                sub_devices: Some(4),
                profile: 0,
                duplex: 0,
                connections: 1,
                can_play: true,
                internal_keying: false,
                external_keying: false,
                modes: vec![
                    mode(b"hp50", 720, b"prog", (1000, 50000)),
                    mode(b"Hi59", 1080, b"uppr", (1001, 30000)),
                    mode(b"Hp30", 1080, b"prog", (1000, 30000)),
                ],
                card_mode: card_mode.map(|code| i64::from(u32::from_be_bytes(*code))),
                card_mode_3d: false,
            }
        };
        let found = sp_decklink::Found {
            driver: sp_decklink::Driver::Ready("16.3".to_owned()),
            devices: vec![
                card("DeckLink Duo (1)", 1, Some(b"Hp30")),
                card("DeckLink Duo (2)", 2, Some(b"ntsc")),
                card("DeckLink Duo (3)", 3, None),
            ],
        };
        // A new screen: the first card is set to 1080p30 in Desktop Video Setup, the second to
        // a format it doesn't list, the third to none.
        let choices = decklink_choices(&Output::Window, &found);
        let mode_of = |output: &Output| match output {
            Output::DeckLink(out) => out.mode.clone(),
            _ => unreachable!(),
        };
        assert_eq!(choices.iter().map(mode_of).collect::<Vec<_>>()[..3], ["1080p30", "1080i59.94", "1080i59.94"]);
        let new = Screen::new("Switcher".to_owned(), choices[0].clone());
        assert_eq!(decklink_format(&new, &found), Some(("1080p30, as Desktop Video Setup has it".to_owned(), "card")));
        let usual = Screen::new("Stream".to_owned(), choices[2].clone());
        assert_eq!(
            decklink_format(&usual, &found),
            Some(("1080i59.94, the usual HD format: Desktop Video Setup sets none".to_owned(), "usual"))
        );
        // A screen saved before, at 1080i59.94 on the card now set to 1080p30, keeps its format,
        // in its file and in the choices, and says what the card has.
        let saved = Screen::new(
            "Old".to_owned(),
            Output::DeckLink(DeckLinkOut {
                device: "DeckLink Duo (1)".to_owned(),
                id: Some(1),
                mode: "1080i59.94".to_owned(),
                keyer: Keyer::Off,
            }),
        );
        let reopened = from_json(&to_json(std::slice::from_ref(&saved))).unwrap();
        assert_eq!(reopened, vec![saved.clone()], "its file is as it was");
        assert_eq!(decklink_choices(&saved.output, &found)[0], saved.output, "chosen again, it keeps its format");
        assert_eq!(
            decklink_format(&saved, &found),
            Some(("1080i59.94, set in SoftPresenter: Desktop Video Setup has 1080p30".to_owned(), "set"))
        );
        let away = sp_decklink::Found { driver: sp_decklink::Driver::Ready("16.3".to_owned()), devices: Vec::new() };
        assert_eq!(decklink_format(&saved, &away), None, "nothing said while the card is away");
        assert_eq!(decklink_format(&Screen::new("W".to_owned(), Output::Window), &found), None);
    }

    #[test]
    fn sp_gets_a_screens_status_in_english_whatever_the_window_speaks() {
        use crate::i18n::{Language, with_language};
        let displays = [DisplayInfo { index: 1, name: "Projector".into(), w: 1920, h: 1080, ..Default::default() }];
        with_language(Language::Spanish, || {
            // The one-word statuses, which english_of leaves alone, and sentences it puts back.
            for placed in [
                Placed::Nothing(String::new()),
                Placed::Window,
                Placed::Fullscreen(1),
                Placed::Ndi("Santuario".to_owned()),
            ] {
                let said = describe(&placed, &displays);
                let english = with_language(Language::English, || describe(&placed, &displays));
                assert_ne!(said, english, "the window speaks Spanish");
                assert_eq!(status_in_english(&said), english);
            }
            assert_eq!(status_in_english("Apagada"), "Off");
            // A reason given when the screens are switched off is said as it was made.
            let switched_off =
                describe(&Placed::Nothing(t("screens.status.switched_off", "Switched off").to_owned()), &[]);
            assert_ne!(switched_off, "Switched off");
            assert_eq!(status_in_english(&switched_off), "Switched off");
        });
    }

    #[test]
    fn one_display_and_one_ndi_name_serve_one_screen_each() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let screens = [
                screen("A", Output::Automatic),
                screen("B", Output::Display("Projector".to_owned())),
                screen("C", Output::Ndi("Main".to_owned())),
                screen("D", Output::Ndi(" Main ".to_owned())),
                screen("E", Output::Window),
                screen("F", Output::Window),
                screen("G", Output::Off),
                screen("H", Output::Ndi("  ".to_owned())),
            ];
            let placed = place_all(&screens, &displays(&["Built-in", "Projector"]), &[0]);
            assert_eq!(placed[0], Placed::Fullscreen(1));
            assert!(matches!(&placed[1], Placed::Nothing(why) if why.contains("\"A\"")), "{placed:?}");
            assert_eq!(placed[2], Placed::Ndi("Main".to_owned()));
            assert!(matches!(&placed[3], Placed::Nothing(why) if why.contains("\"C\"")));
            // Any number of windows is fine.
            assert_eq!((&placed[4], &placed[5]), (&Placed::Window, &Placed::Window));
            assert_eq!(placed[6], Placed::Nothing(String::new()));
            assert!(matches!(&placed[7], Placed::Nothing(why) if why.contains("name")));
            assert_eq!(describe(&placed[6], &[]), "Off");
        });
    }

    #[test]
    fn the_list_survives_its_file_and_other_files_are_refused() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let screens = starting(Some("SoftPresenter Audience"));
            assert_eq!(screens.len(), 2);
            assert_eq!(from_json(&to_json(&screens)), Ok(screens.clone()));
            assert!(from_json("{}").is_err(), "not a screens file");
            assert!(
                from_json(r#"{"format":"softpresenter-screens","version":2,"screens":[]}"#).is_err(),
                "a newer one"
            );
            assert_eq!(from_json(&to_json(&[])), Ok(Vec::new()), "lists none: made anew as it loads");
            assert_eq!(next_name(&screens, Kind::Audience), "Screen 3");
            assert_eq!(next_name(&screens, Kind::Stage), "Stage Screen 1");
            // A file from before stage screens reads, its screens being audience screens.
            let old = r#"{"format":"softpresenter-screens","version":1,"screens":[{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"A","output":"window"}]}"#;
            assert_eq!(from_json(old).map(|list| list[0].kind), Ok(Kind::Audience));
            let mixed = vec![Screen::new("A".to_owned(), Output::Window), Screen::stage("B".to_owned(), Output::Off)];
            assert_eq!(from_json(&to_json(&mixed)), Ok(mixed));
            assert_eq!(starting(None).len(), 1);
        });
    }
}
