//! Macros: named, coloured buttons that do several things at once when clicked. Kept in
//! `macros.json`.

use serde::{Deserialize, Serialize};
use sp_model::{Announced, ClearWhat, Rgba, ShowAction, TimerDo, Uuid};

use crate::i18n::{t, tf, tk};

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

/// The colours a macro can be given, each with its name (a key and its English, for `t`).
pub const COLORS: [((&str, &str), Rgba); 8] = [
    (tk("tools.color.blue", "Blue"), Rgba::rgb(66, 133, 244)),
    (tk("tools.color.green", "Green"), Rgba::rgb(52, 168, 110)),
    (tk("tools.color.orange", "Orange"), Rgba::rgb(244, 160, 0)),
    (tk("tools.color.purple", "Purple"), Rgba::rgb(155, 89, 182)),
    (tk("tools.color.teal", "Teal"), Rgba::rgb(0, 172, 193)),
    (tk("tools.color.pink", "Pink"), Rgba::rgb(204, 78, 180)),
    (tk("tools.color.grey", "Grey"), Rgba::rgb(120, 144, 156)),
    (tk("tools.color.brown", "Brown"), Rgba::rgb(141, 110, 99)),
];

impl Macro {
    pub fn new(name: String) -> Macro {
        Macro { id: Uuid::new_v4(), name, color: COLORS[0].1, actions: Vec::new(), on_startup: false }
    }

    pub fn duplicate(&self) -> Macro {
        Macro {
            id: Uuid::new_v4(),
            name: tf("tools.copy_name", "{name} copy", &[("name", &self.name)]),
            ..self.clone()
        }
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

/// What an action does, in words, given the names of what it names, always in English: for the
/// command line, which speaks English whatever the window's language. The window says it with
/// [`said`].
#[allow(clippy::too_many_arguments)]
pub fn describe(
    action: &ShowAction,
    message_name: impl Fn(Uuid) -> Option<String>,
    prop_name: impl Fn(Uuid) -> Option<String>,
    timer_name: impl Fn(Uuid) -> Option<String>,
    song_name: impl Fn(&str) -> Option<String>,
    media_name: impl Fn(Uuid) -> Option<String>,
    media_playlist_name: impl Fn(Uuid) -> Option<String>,
    version_name: impl Fn(Uuid) -> Option<String>,
) -> String {
    crate::i18n::with_language(crate::i18n::Language::English, || {
        said(action, message_name, prop_name, timer_name, song_name, media_name, media_playlist_name, version_name)
    })
}

/// What an action does, in words, in the window's language, given the names of what it names.
/// Something it names that is gone is said plainly ("a song no longer in the library"), so the
/// step can be seen to need changing.
#[allow(clippy::too_many_arguments)]
pub fn said(
    action: &ShowAction,
    message_name: impl Fn(Uuid) -> Option<String>,
    prop_name: impl Fn(Uuid) -> Option<String>,
    timer_name: impl Fn(Uuid) -> Option<String>,
    song_name: impl Fn(&str) -> Option<String>,
    media_name: impl Fn(Uuid) -> Option<String>,
    media_playlist_name: impl Fn(Uuid) -> Option<String>,
    version_name: impl Fn(Uuid) -> Option<String>,
) -> String {
    let gone = |what: &'static str| move || what.to_owned();
    let (song_gone, media_gone) = (
        gone(t("tools.step.gone.song", "(a song no longer in the library)")),
        gone(t("tools.step.gone.media", "(a picture or video no longer in the media bin)")),
    );
    let named = |template: &str, name: &str| crate::i18n::fill(template, &[("name", name)]);
    match action {
        ShowAction::Clear { what } => clear_words(*what).to_owned(),
        ShowAction::Message { id, show } => {
            let name = message_name(*id)
                .unwrap_or_else(gone(t("tools.step.gone.message", "(a message that has been deleted)")));
            if *show {
                named(t("tools.step.show_message", "Show Message: {name}"), &name)
            } else {
                named(t("tools.step.hide_message", "Hide Message: {name}"), &name)
            }
        }
        ShowAction::Prop { id, show } => {
            let name = prop_name(*id).unwrap_or_else(gone(t("tools.step.gone.prop", "(a prop that has been deleted)")));
            if *show {
                named(t("tools.step.show_prop", "Show Prop: {name}"), &name)
            } else {
                named(t("tools.step.hide_prop", "Hide Prop: {name}"), &name)
            }
        }
        ShowAction::Timer { id, act } => {
            let name =
                timer_name(*id).unwrap_or_else(gone(t("tools.step.gone.timer", "(a timer that has been deleted)")));
            let template = match act {
                TimerDo::Start => t("tools.step.start_timer", "Start Timer: {name}"),
                TimerDo::Stop => t("tools.step.stop_timer", "Stop Timer: {name}"),
                TimerDo::Reset => t("tools.step.reset_timer", "Reset Timer: {name}"),
            };
            named(template, &name)
        }
        ShowAction::Audience { on: true } => t("tools.step.audience_on", "Audience Screens On").to_owned(),
        ShowAction::Audience { on: false } => t("tools.step.audience_off", "Audience Screens Off").to_owned(),
        ShowAction::Song { key } => {
            named(t("tools.step.song", "Go to Song: {name}"), &song_name(key).unwrap_or_else(song_gone))
        }
        ShowAction::Media { id } => {
            named(t("tools.step.media", "Show Media: {name}"), &media_name(*id).unwrap_or_else(media_gone))
        }
        ShowAction::Announce { what } => {
            let name = match what {
                Announced::Song { key } => song_name(key).unwrap_or_else(song_gone),
                Announced::Media { id } => media_name(*id).unwrap_or_else(media_gone),
                Announced::MediaPlaylist { id } => media_playlist_name(*id).unwrap_or_else(gone(t(
                    "tools.step.gone.media_playlist",
                    "(a media playlist that has been deleted)",
                ))),
            };
            named(t("tools.step.announce", "Play on the Announcements Layer: {name}"), &name)
        }
        ShowAction::CanvaSync => t("tools.step.canva", "Sync from Canva").to_owned(),
        ShowAction::Version { id: None, .. } => t("tools.step.usual_look", "Back to the usual look").to_owned(),
        ShowAction::Version { id: Some(id), keep } => {
            let gone_version = t("tools.step.gone.version", "(a version that has been removed)");
            match (version_name(*id), keep) {
                (Some(name), false) => named(t("tools.step.version", "Use the {name} version"), &name),
                (Some(name), true) => {
                    named(t("tools.step.version.kept", "Use the {name} version until switched back"), &name)
                }
                (None, false) => named(t("tools.step.version.gone", "Use {name}"), gone_version),
                (None, true) => {
                    named(t("tools.step.version.gone_kept", "Use {name} until switched back"), gone_version)
                }
            }
        }
    }
}

/// What a Clear step clears, in words, in the window's language (sp-model's own names are the
/// English ones).
pub fn clear_words(what: ClearWhat) -> &'static str {
    match what {
        ClearWhat::All => t("right.clear_all", "Clear All"),
        ClearWhat::Slide => t("tools.step.clear_slide", "Clear Slide"),
        ClearWhat::Media => t("tools.step.clear_media", "Clear Media"),
        ClearWhat::Props => t("tools.step.clear_props", "Clear Props"),
        ClearWhat::Messages => t("tools.step.clear_messages", "Clear Messages"),
        ClearWhat::Audio => t("tools.step.clear_audio", "Clear Audio"),
        ClearWhat::Announcements => t("tools.step.clear_announcements", "Clear Announcements"),
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

/// The name a macro made from what Settings once said to do when SoftPresenter opened is given,
/// in the language set.
fn opening_macro_name() -> &'static str {
    crate::i18n::tg("tools.macro.opening", "When SoftPresenter opens")
}

/// Start-up belongs in macros: what an earlier version's Settings › Screens › When SoftPresenter
/// Opens said to play becomes a macro that plays it on the Announcements layer, with Run When
/// SoftPresenter Opens ticked. Returns the macro's step and the macro's id when one was made;
/// nothing when there was nothing to play, or such a macro is there already (the preferences,
/// which no longer keep the choice, are written later, so this may be asked again at the next
/// start).
pub fn from_opening_choice(macros: &mut Vec<Macro>, choice: &crate::service::StartWith) -> Option<(ShowAction, Uuid)> {
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
    let mut made = Macro::new(opening_macro_name().to_owned());
    made.actions = vec![step.clone()];
    made.on_startup = true;
    let id = made.id;
    macros.push(made);
    Some((step, id))
}

/// True when a macro run as SoftPresenter opens switches the audience screens off. The app does
/// that before any screen draws, so they never show anything first.
pub fn starts_with_audience_off(macros: &[Macro]) -> bool {
    macros.iter().any(|made| made.on_startup && made.actions.contains(&ShowAction::Audience { on: false }))
}

/// Start-up belongs in macros: an earlier version's Settings › Screens › Start with the
/// Audience Screens Off becomes the step Audience Screens Off, first in the macro run as
/// SoftPresenter opens: `opening`, the macro just made from what it played
/// ([`from_opening_choice`]), or else a new one. Returns the id of the macro the step went in;
/// nothing changes when the setting was off or a start-up macro already has the step.
pub fn from_audience_off(macros: &mut Vec<Macro>, off: bool, opening: Option<Uuid>) -> Option<Uuid> {
    if !off || starts_with_audience_off(macros) {
        return None;
    }
    let step = ShowAction::Audience { on: false };
    match macros.iter_mut().find(|made| made.on_startup && Some(made.id) == opening) {
        Some(made) => {
            made.actions.insert(0, step);
            Some(made.id)
        }
        None => {
            let mut made = Macro::new(opening_macro_name().to_owned());
            made.actions = vec![step];
            made.on_startup = true;
            let id = made.id;
            macros.push(made);
            Some(id)
        }
    }
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("macros.json"))
}

/// The macros as last saved; and a file there that couldn't be read, kept aside (`whole_file`).
pub fn load() -> (Vec<Macro>, Option<crate::whole_file::Unread>) {
    file().map_or((Vec::new(), None), |path| load_from(&path))
}

fn load_from(path: &std::path::Path) -> (Vec<Macro>, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::Macros, |text| {
        serde_json::from_str(text).map_err(|e| e.to_string())
    })
}

/// The macros' file and what goes in it, for the library's writer to write (`writer`), paced (`writer::Paced`). The
/// error is a plain sentence.
pub fn to_save(macros: &[Macro]) -> Result<crate::writer::Save, String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(to_save_at(path, macros))
}

fn to_save_at(path: std::path::PathBuf, macros: &[Macro]) -> crate::writer::Save {
    crate::whole_file::settings_save(
        path,
        crate::whole_file::What::Macros,
        serde_json::to_string_pretty(macros).unwrap_or_default().into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::{ClearWhat, TimerDo};

    #[test]
    fn a_macros_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let mut start = Macro::new("Start Service".to_owned());
        start.actions = vec![ShowAction::Clear { what: ClearWhat::All }];
        let good = serde_json::to_string_pretty(&vec![start]).unwrap();
        crate::whole_file::tests::check_loader(
            "macros.json",
            crate::whole_file::What::Macros,
            good.as_bytes(),
            |path| load_from(path).1,
            |path| crate::whole_file::tests::written(to_save_at(path.to_owned(), &[])),
        );
    }

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
            .map(|action| describe(action, |_| None, |_| None, named, |_| None, |_| None, |_| None, |_| None))
            .collect();
        assert_eq!(words, ["Clear All", "Start Timer: Countdown", "Audience Screens On"]);
        let loop_list = Uuid::new_v4();
        let announce = ShowAction::Announce { what: Announced::MediaPlaylist { id: loop_list } };
        let playlist = |id: Uuid| (id == loop_list).then(|| "Announcements".to_owned());
        assert_eq!(
            describe(&announce, |_| None, |_| None, |_| None, |_| None, |_| None, playlist, |_| None),
            "Play on the Announcements Layer: Announcements"
        );
        // Something a step names that has gone is said plainly.
        let song = ShowAction::Announce { what: Announced::Song { key: "41".to_owned() } };
        assert_eq!(
            describe(&song, |_| None, |_| None, |_| None, |_| None, |_| None, |_| None, |_| None),
            "Play on the Announcements Layer: (a song no longer in the library)"
        );
        assert_eq!(
            describe(&start.actions[1], |_| None, |_| None, |_| None, |_| None, |_| None, |_| None, |_| None),
            "Start Timer: (a timer that has been deleted)"
        );
        // A theme version's steps, by the version's name.
        let offering = Uuid::new_v4();
        let version = |id: Uuid| (id == offering).then(|| "Offering".to_owned());
        let steps = [
            ShowAction::Version { id: Some(offering), keep: false },
            ShowAction::Version { id: Some(offering), keep: true },
            ShowAction::Version { id: None, keep: false },
            ShowAction::Version { id: Some(Uuid::new_v4()), keep: false },
        ];
        let words: Vec<String> = steps
            .iter()
            .map(|step| describe(step, |_| None, |_| None, |_| None, |_| None, |_| None, |_| None, version))
            .collect();
        assert_eq!(
            words,
            [
                "Use the Offering version",
                "Use the Offering version until switched back",
                "Back to the usual look",
                "Use (a version that has been removed)"
            ]
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
        let (step, id) = from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist }).unwrap();
        assert_eq!(step, ShowAction::Announce { what: Announced::MediaPlaylist { id: playlist } });
        let made = macros.last().unwrap();
        assert_eq!((made.id, made.on_startup, made.actions.len()), (id, true, 1));
        // Asked again (the preferences not yet written without the choice): no second macro.
        assert_eq!(from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist }), None);
        assert_eq!(macros.len(), 2);
        // A song on the Announcements layer, by its key.
        let step = from_opening_choice(&mut macros, &StartWith::Announcements { song: "41".to_owned() });
        assert_eq!(
            step.map(|(step, _)| step),
            Some(ShowAction::Announce { what: Announced::Song { key: "41".to_owned() } })
        );
    }

    #[test]
    fn the_start_up_macro_is_named_in_the_language_set_and_found_by_its_id() {
        use crate::i18n::{Language, with_language};
        use crate::service::StartWith;
        let playlist = StartWith::MediaPlaylist { id: Uuid::new_v4() };
        let mut macros = Vec::new();
        let (_, id) = with_language(Language::Spanish, || from_opening_choice(&mut macros, &playlist)).unwrap();
        assert_eq!(macros[0].name, "Al abrir SoftPresenter");
        // Renamed by the church: the audience-off step still goes in it, found by its id.
        macros[0].name = "Morning".to_owned();
        assert_eq!(from_audience_off(&mut macros, true, Some(id)), Some(id));
        assert_eq!((macros.len(), macros[0].actions[0].clone()), (1, ShowAction::Audience { on: false }));
        // A macro of the same name made by hand is not it.
        let mut by_hand = vec![Macro { on_startup: true, ..Macro::new("When SoftPresenter opens".to_owned()) }];
        let made = with_language(Language::English, || from_audience_off(&mut by_hand, true, None)).unwrap();
        assert_eq!((by_hand.len(), by_hand[1].id, by_hand[1].name.as_str()), (2, made, "When SoftPresenter opens"));
        assert!(by_hand[0].actions.is_empty());
    }

    #[test]
    fn starting_with_the_audience_screens_off_becomes_the_first_step_of_the_start_up_macro() {
        use crate::service::StartWith;
        let off = ShowAction::Audience { on: false };
        // Off, with nothing played at start: a start-up macro of its own.
        let mut macros = Vec::new();
        assert!(from_audience_off(&mut macros, false, None).is_none(), "nothing when the setting was off");
        assert!(from_audience_off(&mut macros, true, None).is_some());
        assert_eq!((macros.len(), macros[0].on_startup, macros[0].actions.clone()), (1, true, vec![off.clone()]));
        assert!(starts_with_audience_off(&macros));
        assert!(from_audience_off(&mut macros, true, None).is_none(), "never added twice");
        // Off, with a media playlist played at start: first in that macro, before it plays.
        let mut macros = Vec::new();
        let playlist = Uuid::new_v4();
        let opening = from_opening_choice(&mut macros, &StartWith::MediaPlaylist { id: playlist }).map(|(_, id)| id);
        assert!(from_audience_off(&mut macros, true, opening).is_some());
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].actions[0], off);
        // A macro that isn't run at start doesn't count.
        let mut plain = Macro::new("Off".to_owned());
        plain.actions = vec![off];
        assert!(!starts_with_audience_off(&[plain]));
    }
}
