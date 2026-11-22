//! The audience screens: what each one is called and where its picture goes. The list is kept
//! in `screens.json` in the app's folder.

use serde::{Deserialize, Serialize};
use sp_model::{Rgba, Uuid};

use crate::displays::{DisplayInfo, Placement, choose_output};

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
    /// Nowhere. The screen is kept in the list for later.
    Off,
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
    /// inputs. The key is made from the very picture that screen shows (see `window_fill`
    /// and `ndi_fill` in output.rs).
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
}

/// Where a screen ends up on this computer as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placed {
    /// Filling the display with this index.
    Fullscreen(usize),
    Window,
    Ndi(String),
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

/// Where each screen goes, in the list's order. `operator` are the displays the operator works
/// on (the home window's and the one with the menu bar), which Automatic never takes. A display is filled by one screen only and an NDI name is used
/// once: the first screen in the list that asks gets it. A display that is not connected is
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
            Output::Off => Placed::Nothing(String::new()),
        };
        let taken = placed
            .iter()
            .position(|earlier| matches!(earlier, Placed::Fullscreen(_) | Placed::Ndi(_)) && *earlier == wanted);
        placed.push(match (taken, &wanted) {
            (Some(by), Placed::Fullscreen(_)) => {
                Placed::Nothing(format!("\"{}\" is already on that display.", screens[by].name))
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
        // Straight is the usual kind.
        assert_eq!(Alpha::default(), Alpha::Straight);
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

    #[test]
    fn a_display_that_is_not_plugged_in_is_never_swapped_for_another() {
        let screens = [screen("A", Output::Display("Projector".to_owned()))];
        let placed = place_all(&screens, &displays(&["Built-in"]), &[0]);
        assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
        // Plugged in again, it is found by name wherever it now sits in the list.
        assert_eq!(place_all(&screens, &displays(&["Projector", "Built-in"]), &[1]), [Placed::Fullscreen(0)]);
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
