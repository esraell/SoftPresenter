//! Macros: named, coloured buttons that do several things at once when clicked. Kept in
//! `macros.json`.

use serde::{Deserialize, Serialize};
use sp_model::{Announced, Rgba, ShowAction, Uuid};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub id: Uuid,
    pub name: String,
    pub color: Rgba,
    pub actions: Vec<ShowAction>,
    /// Run once when SoftPresenter starts.
    #[serde(default)]
    pub on_startup: bool,
}

/// The colours a macro can be given.
pub const COLORS: [(&str, Rgba); 8] = [
    ("Blue", Rgba::rgb(66, 133, 244)),
    ("Green", Rgba::rgb(52, 168, 110)),
    ("Orange", Rgba::rgb(244, 160, 0)),
    ("Purple", Rgba::rgb(155, 89, 182)),
    ("Teal", Rgba::rgb(0, 172, 193)),
    ("Pink", Rgba::rgb(204, 78, 180)),
    ("Grey", Rgba::rgb(120, 144, 156)),
    ("Brown", Rgba::rgb(141, 110, 99)),
];

impl Macro {
    pub fn new(name: String) -> Macro {
        Macro { id: Uuid::new_v4(), name, color: COLORS[0].1, actions: Vec::new(), on_startup: false }
    }

    pub fn duplicate(&self) -> Macro {
        Macro { id: Uuid::new_v4(), name: format!("{} copy", self.name), ..self.clone() }
    }

    /// Puts `step` where the step at `at` (counting from 0) is, so it is done at the same point.
    /// Returns the step it took the place of, or `None` when there is no step there.
    pub fn replace_step(&mut self, at: usize, step: ShowAction) -> Option<ShowAction> {
        self.actions.get_mut(at).map(|place| std::mem::replace(place, step))
    }

    /// Moves the step at `from` so that it becomes the step at `to` (both counting from 0); the
    /// steps between close up. False when either is past the last step, or they are the same.
    pub fn move_step(&mut self, from: usize, to: usize) -> bool {
        let count = self.actions.len();
        if from >= count || to >= count || from == to {
            return false;
        }
        let step = self.actions.remove(from);
        self.actions.insert(to, step);
        true
    }
}

/// What an action does, in words, given the names of what it names. Something it names that is
/// gone is said plainly ("a song no longer in the library"), so the step can be seen to need
/// changing.
#[allow(clippy::too_many_arguments)]
pub fn describe(
    action: &ShowAction,
    message_name: impl Fn(Uuid) -> Option<String>,
    prop_name: impl Fn(Uuid) -> Option<String>,
    timer_name: impl Fn(Uuid) -> Option<String>,
    song_name: impl Fn(&str) -> Option<String>,
    media_name: impl Fn(Uuid) -> Option<String>,
    media_playlist_name: impl Fn(Uuid) -> Option<String>,
) -> String {
    let gone = |what: &'static str| move || format!("({what})");
    let (song_gone, media_gone) =
        (gone("a song no longer in the library"), gone("a picture or video no longer in the media bin"));
    match action {
        ShowAction::Clear { what } => what.name().to_owned(),
        ShowAction::Message { id, show } => {
            let name = message_name(*id).unwrap_or_else(gone("a message that has been deleted"));
            format!("{} Message: {name}", if *show { "Show" } else { "Hide" })
        }
        ShowAction::Prop { id, show } => {
            let name = prop_name(*id).unwrap_or_else(gone("a prop that has been deleted"));
            format!("{} Prop: {name}", if *show { "Show" } else { "Hide" })
        }
        ShowAction::Timer { id, act } => {
            format!("{} Timer: {}", act.name(), timer_name(*id).unwrap_or_else(gone("a timer that has been deleted")))
        }
        ShowAction::Audience { on } => format!("Audience Screens {}", if *on { "On" } else { "Off" }),
        ShowAction::Song { key } => format!("Go to Song: {}", song_name(key).unwrap_or_else(song_gone)),
        ShowAction::Media { id } => format!("Show Media: {}", media_name(*id).unwrap_or_else(media_gone)),
        ShowAction::Announce { what } => {
            let name = match what {
                Announced::Song { key } => song_name(key).unwrap_or_else(song_gone),
                Announced::Media { id } => media_name(*id).unwrap_or_else(media_gone),
                Announced::MediaPlaylist { id } => {
                    media_playlist_name(*id).unwrap_or_else(gone("a media playlist that has been deleted"))
                }
            };
            format!("Play on the Announcements Layer: {name}")
        }
        ShowAction::CanvaSync => "Sync from Canva".to_owned(),
    }
}

/// The song an Announcements step plays, by its place among the open songs: the one kept under
/// exactly the step's key.
pub fn announced_song(state: &crate::home::HomeState, key: &str) -> Option<usize> {
    state.song_keys.iter().position(|have| have == key)
}

/// The song a step names, by its place among the open songs, found as running the step finds
/// it: an Announcements step's by `announced_song`, a Go to Song step's as a playlist finds its
/// songs (by key, id or their one title). `None` when the step names no song, or it can't be
/// found, so a step reads as gone exactly when running it would find nothing.
pub fn step_song(show: &sp_engine::Show, state: &crate::home::HomeState, step: &ShowAction) -> Option<usize> {
    match step {
        ShowAction::Announce { what: Announced::Song { key } } => announced_song(state, key),
        ShowAction::Song { key } => crate::studio::playlist_song(show, state, key),
        _ => None,
    }
}

/// The title of the song a step names, found as `step_song` finds it.
pub fn step_song_title(show: &sp_engine::Show, state: &crate::home::HomeState, step: &ShowAction) -> Option<String> {
    step_song(show, state, step).and_then(|at| show.presentations().get(at)).map(|song| song.title.clone())
}

/// The name of the macro made from what Settings once said to play when SoftPresenter opened.
pub const OPENING_MACRO: &str = "When SoftPresenter opens";

/// Start-up belongs in macros: what an earlier version's Settings › Screens › When SoftPresenter
/// Opens said to play becomes a macro that plays it on the Announcements layer, with Run When
/// SoftPresenter Opens ticked. Returns the macro's step when one was made; nothing when there
/// was nothing to play, or such a macro is there already (the preferences, which no longer
/// keep the choice, are written later, so this may be asked again at the next start).
pub fn from_opening_choice(macros: &mut Vec<Macro>, choice: &crate::service::StartWith) -> Option<ShowAction> {
    use crate::service::StartWith;
    let what = match choice {
        StartWith::Nothing => return None,
        StartWith::MediaPlaylist { id } => Announced::MediaPlaylist { id: *id },
        StartWith::Announcements { song } => Announced::Song { key: song.clone() },
    };
    let step = ShowAction::Announce { what };
    if macros.iter().any(|made| made.on_startup && made.actions.contains(&step)) {
        return None;
    }
    let mut made = Macro::new(OPENING_MACRO.to_owned());
    made.actions = vec![step.clone()];
    made.on_startup = true;
    macros.push(made);
    Some(step)
}

/// True when a macro run as SoftPresenter opens switches the audience screens off. The app does
/// that before any screen draws, so they never show anything first.
pub fn starts_with_audience_off(macros: &[Macro]) -> bool {
    macros.iter().any(|made| made.on_startup && made.actions.contains(&ShowAction::Audience { on: false }))
}

/// Start-up belongs in macros: an earlier version's Settings › Screens › Start with the
/// Audience Screens Off becomes the step Audience Screens Off, first in the macro run as
/// SoftPresenter opens (the one made from what it played, or a new one). True when the step was
/// added; nothing changes when the setting was off or a start-up macro already has the step.
pub fn from_audience_off(macros: &mut Vec<Macro>, off: bool) -> bool {
    if !off || starts_with_audience_off(macros) {
        return false;
    }
    let step = ShowAction::Audience { on: false };
    match macros.iter_mut().find(|made| made.on_startup && made.name == OPENING_MACRO) {
        Some(made) => made.actions.insert(0, step),
        None => {
            let mut made = Macro::new(OPENING_MACRO.to_owned());
            made.actions = vec![step];
            made.on_startup = true;
            macros.push(made);
        }
    }
    true
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("macros.json"))
}

pub fn load() -> Vec<Macro> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(macros: &[Macro]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(macros).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the macros: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::{ClearWhat, TimerDo};

    #[test]
    fn macros_describe_their_actions_and_survive_their_file() {
        let timer = Uuid::new_v4();
        let mut start = Macro::new("Start Service".to_owned());
        start.actions = vec![
            ShowAction::Clear { what: ClearWhat::All },
            ShowAction::Timer { id: timer, act: TimerDo::Start },
            ShowAction::Audience { on: true },
        ];
        let named = |id: Uuid| (id == timer).then(|| "Countdown".to_owned());
        let words: Vec<String> = start
            .actions
            .iter()
            .map(|action| describe(action, |_| None, |_| None, named, |_| None, |_| None, |_| None))
            .collect();
        assert_eq!(words, ["Clear All", "Start Timer: Countdown", "Audience Screens On"]);
        let loop_list = Uuid::new_v4();
        let announce = ShowAction::Announce { what: Announced::MediaPlaylist { id: loop_list } };
        let playlist = |id: Uuid| (id == loop_list).then(|| "Announcements".to_owned());
        assert_eq!(
            describe(&announce, |_| None, |_| None, |_| None, |_| None, |_| None, playlist),
            "Play on the Announcements Layer: Announcements"
        );
        // Something a step names that has gone is said plainly.
        let song = ShowAction::Announce { what: Announced::Song { key: "41".to_owned() } };
        assert_eq!(
            describe(&song, |_| None, |_| None, |_| None, |_| None, |_| None, |_| None),
            "Play on the Announcements Layer: (a song no longer in the library)"
        );
        assert_eq!(
            describe(&start.actions[1], |_| None, |_| None, |_| None, |_| None, |_| None, |_| None),
            "Start Timer: (a timer that has been deleted)"
        );
        let copy = start.duplicate();
        assert_ne!(copy.id, start.id);
        let saved = vec![start, copy];
        assert_eq!(serde_json::from_str::<Vec<Macro>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }

    #[test]
    fn a_step_changed_keeps_its_place_and_a_step_moved_takes_the_place_it_is_given() {
        let steps = [ClearWhat::All, ClearWhat::Slide, ClearWhat::Media, ClearWhat::Props];
        let mut made = Macro::new("Order".to_owned());
        made.actions = steps.iter().map(|what| ShowAction::Clear { what: *what }).collect();
        let off = ShowAction::Audience { on: false };
        assert_eq!(made.replace_step(1, off.clone()), Some(ShowAction::Clear { what: ClearWhat::Slide }));
        assert_eq!(made.actions[1], off);
        assert_eq!(made.actions.len(), 4);
        assert_eq!(made.replace_step(4, off.clone()), None, "no fifth step");
        let clears = |made: &Macro| -> Vec<Option<ClearWhat>> {
            made.actions
                .iter()
                .map(|step| if let ShowAction::Clear { what } = step { Some(*what) } else { None })
                .collect()
        };
        // Down: the first step becomes the third.
        assert!(made.move_step(0, 2));
        assert_eq!(clears(&made), [None, Some(ClearWhat::Media), Some(ClearWhat::All), Some(ClearWhat::Props)]);
        // Up: the last becomes the first.
        assert!(made.move_step(3, 0));
        assert_eq!(clears(&made), [Some(ClearWhat::Props), None, Some(ClearWhat::Media), Some(ClearWhat::All)]);
        assert!(!made.move_step(1, 1), "to where it is already");
        assert!(!made.move_step(0, 4) && !made.move_step(4, 0), "past the last step");
    }

    #[test]
    fn what_once_played_when_the_app_opened_becomes_a_macro_once() {
        use crate::service::StartWith;
        let mut macros = vec![Macro::new("Before service".to_owned())];
        assert_eq!(from_opening_choice(&mut macros, &StartWith::Nothing), None);
        assert_eq!(macros.len(), 1, "nothing to play makes no macro");
        let playlist = Uuid::new_v4();
        let step = from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist });
        assert_eq!(step, Some(ShowAction::Announce { what: Announced::MediaPlaylist { id: playlist } }));
        let made = macros.last().unwrap();
        assert_eq!((made.name.as_str(), made.on_startup, made.actions.len()), (OPENING_MACRO, true, 1));
        // Asked again (the preferences not yet written without the choice): no second macro.
        assert_eq!(from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist }), None);
        assert_eq!(macros.len(), 2);
        // A song on the Announcements layer, by its key.
        let step = from_opening_choice(&mut macros, &StartWith::Announcements { song: "41".to_owned() });
        assert_eq!(step, Some(ShowAction::Announce { what: Announced::Song { key: "41".to_owned() } }));
    }

    #[test]
    fn starting_with_the_audience_screens_off_becomes_the_first_step_of_the_start_up_macro() {
        use crate::service::StartWith;
        let off = ShowAction::Audience { on: false };
        // Off, with nothing played at start: a start-up macro of its own.
        let mut macros = Vec::new();
        assert!(!from_audience_off(&mut macros, false), "nothing when the setting was off");
        assert!(from_audience_off(&mut macros, true));
        assert_eq!((macros.len(), macros[0].on_startup, macros[0].actions.clone()), (1, true, vec![off.clone()]));
        assert!(starts_with_audience_off(&macros));
        assert!(!from_audience_off(&mut macros, true), "never added twice");
        // Off, with a media playlist played at start: first in that macro, before it plays.
        let mut macros = Vec::new();
        let playlist = Uuid::new_v4();
        from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist });
        assert!(from_audience_off(&mut macros, true));
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].actions[0], off);
        // A macro that isn't run at start doesn't count.
        let mut plain = Macro::new("Off".to_owned());
        plain.actions = vec![off];
        assert!(!starts_with_audience_off(&[plain]));
    }
}
