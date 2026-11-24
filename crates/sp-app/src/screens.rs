//! The audience screens: what each one is called and where its picture goes. The list is kept
//! in `screens.json` in the app's folder.

use serde::{Deserialize, Serialize};
use sp_model::{Rgba, Uuid};

use crate::displays::{DisplayInfo, Placement, choose_output};

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
        return Some("The screen it is the key of has been removed, so it shows black.".to_owned());
    };
    let fill = &screens[at];
    let name = &fill.name;
    if at == index {
        return Some("A screen can't be its own key, so it shows black.".to_owned());
    }
    if fill.kind == Kind::Stage {
        return Some(format!("\"{name}\" is a stage screen, which has no key, so this shows black."));
    }
    if fill.look.key_of.is_some() {
        return Some(format!("\"{name}\" is a key itself, so this shows black."));
    }
    let read_back = |screen: &Screen| matches!(screen.output, Output::Ndi(_) | Output::DeckLink(_));
    if read_back(screen) != read_back(fill) {
        let on = match fill.output {
            Output::Ndi(_) => "NDI",
            Output::DeckLink(_) => "a DeckLink output",
            _ => "a display or window",
        };
        return Some(format!(
            "\"{name}\" is on {on}. A key goes out beside its fill on two displays or windows, or on two NDI or DeckLink outputs, so this shows black."
        ));
    }
    if !shown.get(at).copied().unwrap_or(false) {
        return Some(format!("\"{name}\" isn't going out, so this shows black."));
    }
    if fill.look.see_through.is_none() {
        return Some(format!("\"{name}\" is solid, so its key is all white. Make it see-through."));
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
        }
    }

    pub fn stage(name: String, output: Output) -> Screen {
        Screen { kind: Kind::Stage, ..Screen::new(name, output) }
    }
}

/// The screens a fresh start has: one, placed automatically. `ndi` adds a second sent over
/// NDI under that name, for someone who had NDI switched on before screens could be listed.
pub fn starting(ndi: Option<&str>) -> Vec<Screen> {
    let mut screens = vec![Screen::new("Screen 1".to_owned(), Output::Automatic)];
    if let Some(name) = ndi {
        screens.push(Screen::new("Screen 2".to_owned(), Output::Ndi(name.to_owned())));
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
    let base = match kind {
        Kind::Audience => "Screen",
        Kind::Stage => "Stage Screen",
    };
    let of_kind = screens.iter().filter(|screen| screen.kind == kind).count();
    (of_kind + 1..)
        .map(|n| format!("{base} {n}"))
        .find(|name| screens.iter().all(|screen| screen.name != *name))
        .unwrap_or_default()
}

/// Gives the screens the displays' names as the system shows them, and says whether any
/// changed (to be saved). A screen set up as Automatic (no longer offered) is given the first
/// display that isn't the main one and isn't taken; one saved under a display's old name
/// ("Monitor #0", `\\.\DISPLAY2`) is given the name the system shows ("3DTV", "Mi TV").
pub fn give_displays(screens: &mut [Screen], displays: &[DisplayInfo]) -> bool {
    let mut given = false;
    for at in 0..screens.len() {
        let taken: Vec<&str> = screens
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != at)
            .filter_map(|(_, screen)| match &screen.output {
                Output::Display(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let now = match &screens[at].output {
            Output::Automatic => {
                displays.iter().find(|d| !d.main && !taken.iter().any(|name| d.is_named(name))).map(|d| d.name.clone())
            }
            Output::Display(name) => {
                displays.iter().find(|d| d.old_name == *name && d.name != *name).map(|d| d.name.clone())
            }
            _ => None,
        };
        if let Some(name) = now {
            screens[at].output = Output::Display(name);
            given = true;
        }
    }
    given
}

/// Where each screen goes, in the list's order. `operator` are the displays the operator works
/// on (the home window's and the one with the menu bar), which Automatic never takes. A display
/// is filled by one screen only, an NDI name is used once, and a DeckLink output plays one
/// screen: the first screen in the list that asks gets it. A display that is not connected is
/// never replaced by another, so a screen can never land on the operator's display by surprise.
pub fn place_all(screens: &[Screen], displays: &[DisplayInfo], operator: &[usize]) -> Vec<Placed> {
    let mut placed: Vec<Placed> = Vec::with_capacity(screens.len());
    for screen in screens {
        let wanted = match &screen.output {
            Output::Automatic => match choose_output(displays, operator, None) {
                Ok(Placement::Fullscreen(index)) => Placed::Fullscreen(index),
                _ => Placed::Window,
            },
            Output::Display(name) => match displays.iter().find(|display| display.is_named(name)) {
                // The main display is where the operator works; a screen never covers it.
                Some(display) if display.main => Placed::Nothing(format!(
                    "\"{}\" is the main display, where SoftPresenter's own window is. Choose another display.",
                    display.name
                )),
                Some(display) => Placed::Fullscreen(display.index),
                None => Placed::Nothing(format!("Disconnected: \"{name}\" is not plugged in.")),
            },
            Output::Window => Placed::Window,
            Output::Ndi(name) if name.trim().is_empty() => Placed::Nothing("Give the NDI output a name.".to_owned()),
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
            (Some(by), Placed::Fullscreen(_)) => {
                Placed::Nothing(format!("\"{}\" is already on that display.", screens[by].name))
            }
            (Some(by), Placed::DeckLink(_)) => {
                Placed::Nothing(format!("\"{}\" already plays out of that DeckLink output.", screens[by].name))
            }
            (Some(by), _) => Placed::Nothing(format!("\"{}\" already uses that NDI name.", screens[by].name)),
            (None, _) => wanted,
        });
    }
    placed
}

/// What a placed screen is doing, in words for the operator.
pub fn describe(placed: &Placed, displays: &[DisplayInfo]) -> String {
    match placed {
        Placed::Fullscreen(index) => match displays.iter().find(|display| display.index == *index) {
            Some(display) => format!("Filling \"{}\", {} x {}", display.name, display.w, display.h),
            None => "Filling a display".to_owned(),
        },
        Placed::Window => "In a window".to_owned(),
        Placed::Ndi(name) => format!("Starting NDI as \"{name}\""),
        Placed::DeckLink(out) => format!("Starting {} at {}", out.device, out.mode),
        Placed::Nothing(why) if why.is_empty() => "Off".to_owned(),
        Placed::Nothing(why) => why.clone(),
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

/// Reads the list. A file that is not a screens file, or lists none, gives `None`.
pub fn from_json(text: &str) -> Option<Vec<Screen>> {
    let file: File = serde_json::from_str(text).ok()?;
    ((file.format == FORMAT || file.format == OLD_FORMAT) && file.version == 1 && !file.screens.is_empty())
        .then_some(file.screens)
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("screens.json"))
}

/// The saved list, if there is one.
pub fn load() -> Option<Vec<Screen>> {
    from_json(&std::fs::read_to_string(file()?).ok()?)
}

pub fn save(screens: &[Screen]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, to_json(screens))
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the screens: {e}"))
}

#[cfg(test)]
mod tests {

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
        assert_eq!(from_json(&text), Some(vec![stream.clone(), key.clone()]));
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
        assert!(give_displays(&mut screens, &displays));
        let outputs: Vec<&Output> = screens.iter().map(|s| &s.output).collect();
        assert_eq!(
            outputs,
            [
                &Output::Display("Mi TV".to_owned()),
                &Output::Display(r"\\.\DISPLAY3".to_owned()),
                &Output::Display("Philips FTV".to_owned()),
            ]
        );
        assert!(!give_displays(&mut screens, &displays), "nothing more to change");
        assert_eq!(place_all(&screens, &displays, &[0])[..2], [Placed::Fullscreen(1), Placed::Fullscreen(2)]);
    }

    #[test]
    fn a_screen_on_a_monitors_own_name_waits_for_it_and_is_kept() {
        // Mi TV unplugged: the screen waits, under the same name, and nothing takes its place.
        let mut displays = windows_displays();
        displays.remove(1);
        let mut screens = vec![screen("Audience", Output::Display("Mi TV".to_owned()))];
        assert!(!give_displays(&mut screens, &displays));
        assert_eq!(screens[0].output, Output::Display("Mi TV".to_owned()));
        let placed = place_all(&screens, &displays, &[0]);
        assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
    }

    #[test]
    fn automatic_is_given_the_first_free_display_by_its_name() {
        let displays = windows_displays();
        let mut screens =
            vec![screen("Audience", Output::Display(r"\\.\DISPLAY2".to_owned())), screen("Stage", Output::Automatic)];
        assert!(give_displays(&mut screens, &displays));
        assert_eq!(screens[1].output, Output::Display(r"\\.\DISPLAY3".to_owned()), "Mi TV is taken");
    }

    #[test]
    fn a_display_that_is_not_plugged_in_is_never_swapped_for_another() {
        let screens = [screen("A", Output::Display("Projector".to_owned()))];
        let placed = place_all(&screens, &displays(&["Built-in"]), &[0]);
        assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
        // Plugged in again, it is found by name wherever it now sits in the list.
        assert_eq!(place_all(&screens, &displays(&["Projector", "Built-in"]), &[1]), [Placed::Fullscreen(0)]);
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
    fn a_decklink_output_survives_its_file_and_saves_only_what_it_needs() {
        let screen = Screen::new("Switcher".to_owned(), decklink("DeckLink Duo (1)", Some(42)));
        let text = to_json(std::slice::from_ref(&screen));
        assert!(text.contains("\"decklink\"") && text.contains("\"mode\": \"1080i59.94\""), "{text}");
        assert!(!text.contains("keyer"), "a keyer that is off is not written");
        assert_eq!(from_json(&text), Some(vec![screen.clone()]));
        let keyed = Screen {
            output: Output::DeckLink(DeckLinkOut {
                keyer: Keyer::External,
                id: None,
                ..match screen.output {
                    Output::DeckLink(out) => out,
                    _ => unreachable!(),
                }
            }),
            ..screen
        };
        let text = to_json(std::slice::from_ref(&keyed));
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
        let out = &saved["screens"][0]["output"]["decklink"];
        assert_eq!(out["keyer"], "external");
        assert!(out.get("id").is_none(), "a device without an id is saved without one: {text}");
        assert_eq!(from_json(&text), Some(vec![keyed]));
    }

    #[test]
    fn one_decklink_output_plays_one_screen_and_another_card_of_the_same_name_is_another_output() {
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
    fn one_display_and_one_ndi_name_serve_one_screen_each() {
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
    }

    #[test]
    fn the_list_survives_its_file_and_other_files_are_refused() {
        let screens = starting(Some("SoftPresenter Audience"));
        assert_eq!(screens.len(), 2);
        assert_eq!(from_json(&to_json(&screens)), Some(screens.clone()));
        assert_eq!(from_json("{}"), None);
        assert_eq!(from_json(&to_json(&[])), None);
        assert_eq!(next_name(&screens, Kind::Audience), "Screen 3");
        assert_eq!(next_name(&screens, Kind::Stage), "Stage Screen 1");
        // A file from before stage screens reads, its screens being audience screens.
        let old = r#"{"format":"softpresenter-screens","version":1,"screens":[{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"A","output":"window"}]}"#;
        assert_eq!(from_json(old).map(|list| list[0].kind), Some(Kind::Audience));
        let mixed = vec![Screen::new("A".to_owned(), Output::Window), Screen::stage("B".to_owned(), Output::Off)];
        assert_eq!(from_json(&to_json(&mixed)), Some(mixed));
        assert_eq!(starting(None).len(), 1);
    }
}
