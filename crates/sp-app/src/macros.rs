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
}

/// What an action does, in words, given the names of what it names.
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
    let gone = || "(gone)".to_owned();
    match action {
        ShowAction::Clear { what } => what.name().to_owned(),
        ShowAction::Message { id, show } => {
            format!("{} Message: {}", if *show { "Show" } else { "Hide" }, message_name(*id).unwrap_or_else(gone))
        }
        ShowAction::Prop { id, show } => {
            format!("{} Prop: {}", if *show { "Show" } else { "Hide" }, prop_name(*id).unwrap_or_else(gone))
        }
        ShowAction::Timer { id, act } => format!("{} Timer: {}", act.name(), timer_name(*id).unwrap_or_else(gone)),
        ShowAction::Audience { on } => format!("Audience Screens {}", if *on { "On" } else { "Off" }),
        ShowAction::Song { key } => format!("Go to Song: {}", song_name(key).unwrap_or_else(gone)),
        ShowAction::Media { id } => format!("Show Media: {}", media_name(*id).unwrap_or_else(gone)),
        ShowAction::Announce { what } => {
            let name = match what {
                Announced::Song { key } => song_name(key),
                Announced::Media { id } => media_name(*id),
                Announced::MediaPlaylist { id } => media_playlist_name(*id),
            };
            format!("Play on the Announcements Layer: {}", name.unwrap_or_else(gone))
        }
        ShowAction::CanvaSync => "Sync from Canva".to_owned(),
    }
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
        let copy = start.duplicate();
        assert_ne!(copy.id, start.id);
        let saved = vec![start, copy];
        assert_eq!(serde_json::from_str::<Vec<Macro>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
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
}
