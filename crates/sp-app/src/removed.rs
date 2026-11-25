//! What was deleted that Undo puts back. The owner's rule: anything Undo can bring back goes at
//! once, with no question and no notice; anything it can't asks first (see `home::cant_undo`).
//! Each delete here keeps what went, with its place, in `HomeState::removed`; the app moves it
//! into its one history with the songs' changes (`ShowHistory` in main.rs), so Undo puts it back
//! where it was, picked, and Redo deletes it again.

use sp_model::{ThemeTemplate, ThemeVersion, Uuid};

use crate::home::HomeState;
use crate::i18n::{t, tf};
use crate::service::{Item, Service};

/// A kind of thing, as [`Removed::named`] gives it, in the language set: for what the window
/// says.
fn kind_shown(kind: &'static str) -> &'static str {
    match kind {
        "playlist" => t("notices.undo.kind.playlist", "playlist"),
        "library" => t("notices.undo.kind.library", "library"),
        "audio playlist" => t("notices.undo.kind.audio_playlist", "audio playlist"),
        "timer" => t("notices.undo.kind.timer", "timer"),
        "message" => t("notices.undo.kind.message", "message"),
        "prop" => t("notices.undo.kind.prop", "prop"),
        "macro" => t("notices.undo.kind.macro", "macro"),
        "stage layout" => t("notices.undo.kind.stage_layout", "stage layout"),
        "template" => t("notices.undo.kind.template", "template"),
        "version" => t("notices.undo.kind.version", "version"),
        "media playlist" => t("notices.undo.kind.media_playlist", "media playlist"),
        "media" => t("notices.undo.kind.media", "media"),
        _ => kind,
    }
}

/// Items taken out of a playlist: the playlist (by its place, and its name, so that a playlist
/// since renamed or deleted is left alone), and each item with the place it had.
#[derive(Clone, Debug, PartialEq)]
pub struct RemovedItems {
    pub playlist: usize,
    pub name: String,
    pub items: Vec<(usize, Item)>,
}

/// Something deleted, as it was, with its place in its list.
#[derive(Clone, Debug, PartialEq)]
pub enum Removed {
    Items(RemovedItems),
    Playlist {
        at: usize,
        playlist: Service,
    },
    Library {
        at: usize,
        library: crate::libraries::Library,
    },
    AudioPlaylist {
        at: usize,
        playlist: crate::audio::Playlist,
    },
    Timer {
        at: usize,
        timer: crate::timers::Timer,
    },
    Message {
        at: usize,
        message: crate::messages::Message,
    },
    Prop {
        at: usize,
        prop: crate::props::Prop,
    },
    Macro {
        at: usize,
        made: crate::macros::Macro,
    },
    StageLayout {
        at: usize,
        layout: ThemeTemplate,
    },
    /// A template of a theme (by its id), from its usual templates (`set` `None`) or a version's.
    /// The last template of a version takes the version out of the theme: `version` keeps it
    /// then, with its place among the theme's versions, to put back with it.
    Template {
        theme: Uuid,
        set: Option<Uuid>,
        at: usize,
        template: ThemeTemplate,
        version: Option<(usize, ThemeVersion)>,
    },
    /// A version taken out of a theme (by its id), with its templates.
    Version {
        theme: Uuid,
        at: usize,
        version: ThemeVersion,
    },
    /// Pictures and videos taken out of a media playlist (by its id, and its name), each with
    /// its place in it; they stayed in the media bin.
    MediaPlaylistFiles {
        playlist: Uuid,
        name: String,
        files: Vec<(usize, std::path::PathBuf)>,
    },
    /// Not a delete but an add, kept the same way so one Undo takes it away (and Redo puts it
    /// back): the sample backgrounds that were added to the media bin, with their playlist.
    Samples(Box<crate::samples::Added>),
}

/// Puts `thing` back at `at` in `list` (at its end if the list has since grown shorter).
fn put_back<T: Clone>(list: &mut Vec<T>, at: usize, thing: &T) -> usize {
    let at = at.min(list.len());
    list.insert(at, thing.clone());
    at
}

/// Takes the one with this id out of `list` again (a redo). False when it isn't there.
fn take_again<T>(list: &mut Vec<T>, is_it: impl Fn(&T) -> bool) -> Option<T> {
    let at = list.iter().position(is_it)?;
    Some(list.remove(at))
}

impl Removed {
    /// What kind of thing it is and its name, as a reply names it.
    fn named(&self) -> (&'static str, &str) {
        match self {
            Removed::Items(removed) => ("playlist", &removed.name),
            Removed::Playlist { playlist, .. } => ("playlist", &playlist.name),
            Removed::Library { library, .. } => ("library", &library.name),
            Removed::AudioPlaylist { playlist, .. } => ("audio playlist", &playlist.name),
            Removed::Timer { timer, .. } => ("timer", &timer.name),
            Removed::Message { message, .. } => ("message", &message.name),
            Removed::Prop { prop, .. } => ("prop", &prop.name),
            Removed::Macro { made, .. } => ("macro", &made.name),
            Removed::StageLayout { layout, .. } => ("stage layout", &layout.name),
            Removed::Template { template, .. } => ("template", &template.name),
            Removed::Version { version, .. } => ("version", &version.name),
            Removed::MediaPlaylistFiles { name, .. } => ("media playlist", name),
            Removed::Samples(_) => ("media", crate::samples::PLAYLIST_NAME),
        }
    }

    /// Whether it is the sample backgrounds added, which the app keeps (they are in its media
    /// bin): the app takes them away and puts them back (`App::redo_samples`), not
    /// [`redo`](Self::redo).
    pub fn is_samples(&self) -> bool {
        matches!(self, Removed::Samples(_))
    }

    /// Whether it was in a theme, which the app keeps: the app puts it back
    /// (`App::redo_theme_removed`), not [`redo`](Self::redo).
    pub fn in_a_theme(&self) -> bool {
        matches!(self, Removed::Template { .. } | Removed::Version { .. })
    }

    /// What `sp undo` (or `sp redo`) says it did: put it back (`back`), or deleted it again. The
    /// window says nothing: what is put back is there to see.
    pub fn said(&self, back: bool) -> String {
        let taken_out = match self {
            Removed::Items(removed) => Some((
                match removed.items.as_slice() {
                    [(_, one)] => one.title().to_owned(),
                    many => format!("{} items", many.len()),
                },
                &removed.name,
            )),
            Removed::MediaPlaylistFiles { name, files, .. } => Some((
                match files.as_slice() {
                    [(_, one)] => one.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    many => format!("{} items", many.len()),
                },
                name,
            )),
            _ => None,
        };
        if self.is_samples() {
            // An add: Undo (`back`) takes it away, and Redo puts it back.
            return if back { "Took the sample backgrounds away." } else { "Put the sample backgrounds back." }
                .to_owned();
        }
        if let Some((what, name)) = taken_out {
            return if back {
                format!("Put {what} back in {name}.")
            } else {
                format!("Took {what} out of {name} again.")
            };
        }
        let (kind, name) = self.named();
        if back { format!("Put the {kind} {name} back.") } else { format!("Deleted the {kind} {name} again.") }
    }

    /// Why Undo (or Redo) couldn't: what it was in has changed too much since.
    pub fn changed_since(&self) -> String {
        let (kind, name) = self.named();
        match self {
            Removed::Items(_) | Removed::MediaPlaylistFiles { .. } => {
                tf("notices.undo.changed", "{name} has changed since, so that can't be undone.", &[("name", name)])
            }
            Removed::Samples(_) => {
                t("notices.undo.samples_changed", "The sample backgrounds have changed since, so that can't be done.")
                    .to_owned()
            }
            _ => tf(
                "notices.undo.kind_changed",
                "The {kind} {name} has changed since, so that can't be undone.",
                &[("kind", kind_shown(kind)), ("name", name)],
            ),
        }
    }

    /// Puts it back where it was, picked where its list picks (`back`), or deletes it again (a
    /// redo), as the app's history asks. False, changing nothing, when what it was in has
    /// changed so much that it can't be.
    pub fn redo(&self, state: &mut HomeState, back: bool) -> bool {
        match self {
            Removed::Items(removed) => redo_items(state, removed, back),
            Removed::Playlist { at, playlist } => {
                let services = &mut state.services;
                if back {
                    let at = put_back(&mut services.list, *at, playlist);
                    // It is in front again, and listed.
                    services.current = at;
                    services.chosen = None;
                    services.changed = true;
                    state.playlist_open = true;
                    true
                } else {
                    services.list.get(*at) == Some(playlist) && services.remove_at(*at)
                }
            }
            Removed::Library { at, library } => {
                let gone = if back {
                    put_back(&mut state.libraries, *at, library);
                    // It is the library shown again.
                    state.library_shown = Some(library.id);
                    state.playlist_open = false;
                    true
                } else {
                    let gone = take_again(&mut state.libraries, |have| have.id == library.id).is_some();
                    if state.library_shown == Some(library.id) {
                        state.library_shown = None;
                    }
                    gone
                };
                state.libraries_changed |= gone;
                gone
            }
            Removed::AudioPlaylist { at, playlist } => {
                let playing = state.audio_playing.track;
                let audio = &mut state.audio;
                if back {
                    audio.current = put_back(&mut audio.playlists, *at, playlist);
                    // A track of it that was playing is in its playlist again.
                    if audio.leaving.as_ref().is_some_and(|leaving| leaving.playlist == playlist.id) {
                        audio.leaving = None;
                    }
                } else {
                    let Some(at) = audio.playlists.iter().position(|have| have.id == playlist.id) else { return false };
                    if !audio.remove_playlist(at, playing) {
                        return false;
                    }
                    audio.current = audio.current.min(audio.playlists.len() - 1);
                }
                state.audio_changed = true;
                true
            }
            Removed::Timer { at, timer } => {
                let done = if back {
                    put_back(&mut state.timers, *at, timer);
                    true
                } else {
                    take_again(&mut state.timers, |have| have.id == timer.id).is_some()
                };
                state.timers_changed |= done;
                done
            }
            Removed::Message { at, message } => {
                let done = if back {
                    put_back(&mut state.messages, *at, message);
                    true
                } else {
                    let gone = take_again(&mut state.messages, |have| have.id == message.id).is_some();
                    if state.message_live.is_some_and(|(live, _)| live == message.id) {
                        state.message_live = None;
                    }
                    gone
                };
                state.messages_changed |= done;
                done
            }
            Removed::Prop { at, prop } => {
                if back {
                    let at = put_back(&mut state.props, *at, prop);
                    state.props_changed = true;
                    // In the Edit view's props (or their empty list), it is the one open again.
                    let props_open =
                        state.editing.as_ref().is_some_and(|draft| draft.at.presentation == crate::props::PROP_SLOT);
                    if props_open || (state.props_editor_open && state.editing.is_none()) {
                        state.editing = crate::studio::prop_draft(state, at);
                        state.props_editor_open = false;
                    }
                    true
                } else {
                    let at = state.props.iter().position(|have| have.id == prop.id);
                    at.and_then(|at| state.take_prop(at)).is_some()
                }
            }
            Removed::Macro { at, made } => {
                let done = if back {
                    put_back(&mut state.macros, *at, made);
                    true
                } else {
                    take_again(&mut state.macros, |have| have.id == made.id).is_some()
                };
                state.macros_changed |= done;
                done
            }
            Removed::StageLayout { at, layout } => {
                if back {
                    let at = put_back(&mut state.stage_layouts, *at, layout);
                    state.stage_layouts_changed = true;
                    // In Edit › Stage, it is the one open again.
                    if state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) {
                        state.stage_edit = (at, None);
                        state.editing = crate::stage_editor::draft(state, at);
                    }
                    true
                } else {
                    let at = state.stage_layouts.iter().position(|have| have.id == layout.id);
                    at.and_then(|at| crate::stage_editor::take_out(state, at)).is_some()
                }
            }
            Removed::MediaPlaylistFiles { playlist, files, .. } => {
                let Some(list) = state.media_playlists.iter_mut().find(|have| have.id == *playlist) else {
                    return false;
                };
                if back {
                    // Each at its old place, the earliest first, so every place is as it was.
                    for (at, file) in files {
                        let at = (*at).min(list.files.len());
                        list.files.insert(at, file.clone());
                    }
                } else {
                    let there = files.iter().all(|(at, file)| list.files.get(*at) == Some(file));
                    if !there {
                        return false;
                    }
                    for (at, _) in files.iter().rev() {
                        list.files.remove(*at);
                    }
                }
                state.media_playlists_changed = true;
                true
            }
            // The app keeps the themes, and puts these back itself (see `redo_in_theme`).
            Removed::Template { .. } | Removed::Version { .. } | Removed::Samples(_) => false,
        }
    }

    /// For a template or a version: puts it back in its theme where it was (`back`; a version's
    /// last template comes back with its version), or deletes it again. Returns the set and the
    /// template the Theme Editor then shows, or `None`, changing nothing, when the theme has
    /// changed so much that it can't be.
    pub fn redo_in_theme(&self, theme: &mut sp_model::Theme, back: bool) -> Option<(Option<Uuid>, usize)> {
        let has = |theme: &sp_model::Theme, id: Uuid| theme.versions.iter().any(|have| have.id == id);
        Some(match (self, back) {
            (Removed::Template { version: Some((place, version)), .. }, true) if !has(theme, version.id) => {
                theme.versions.insert((*place).min(theme.versions.len()), version.clone());
                (Some(version.id), 0)
            }
            (Removed::Template { set, at, template, .. }, true) => {
                let templates = theme.set_mut(*set)?;
                let at = (*at).min(templates.len());
                templates.insert(at, template.clone());
                (*set, at)
            }
            (Removed::Template { set, template, .. }, false) => {
                let at = theme.set(*set).iter().position(|have| have.id == template.id)?;
                if !theme.remove_template_in(*set, at) {
                    return None;
                }
                (set.filter(|id| theme.has_version(*id)), at.saturating_sub(1))
            }
            (Removed::Version { at, version, .. }, true) if !has(theme, version.id) => {
                theme.versions.insert((*at).min(theme.versions.len()), version.clone());
                (Some(version.id), 0)
            }
            (Removed::Version { version, .. }, false) if theme.remove_version(version.id) => (None, 0),
            _ => return None,
        })
    }
}

/// Puts items taken out of a playlist back where they were (`back`), picked out again in the
/// playlist in front, or takes them out again (a redo). False when the playlist has changed so
/// much that they can't be (it is gone, renamed, or the items aren't where they were).
fn redo_items(state: &mut HomeState, removed: &RemovedItems, back: bool) -> bool {
    let Some(playlist) = state.services.list.get(removed.playlist).filter(|list| list.name == removed.name) else {
        return false;
    };
    if back {
        if !state.services.put_back(removed.playlist, &removed.items) {
            return false;
        }
        if removed.playlist == state.services.current {
            state.item_picks.places = removed.items.iter().map(|(at, _)| *at).collect();
            state.item_picks.anchor = state.item_picks.places.first().copied();
            state.item_picks.made_in(&state.services);
        }
    } else {
        let there = removed.items.iter().all(|(at, item)| playlist.items.get(*at) == Some(item));
        if !there {
            return false;
        }
        let places: Vec<usize> = removed.items.iter().map(|(at, _)| *at).collect();
        state.services.remove_items(removed.playlist, &places);
        state.item_picks = crate::home::ItemPicks::default();
    }
    true
}

/// The deletes Undo covers, each keeping what went for the app's history. The window's menus,
/// keys and cards and the command line all delete through these.
impl HomeState {
    /// Takes items out of a playlist (by its place among the playlists) at once: no song is
    /// deleted, so nothing is asked, and nothing is said. Nothing on the screens changes, even
    /// when the item taken out is the one on screen. Returns what went.
    pub fn take_out_items(&mut self, playlist: usize, places: &[usize]) -> Vec<(usize, Item)> {
        let items = self.services.remove_items(playlist, places);
        if items.is_empty() {
            return items;
        }
        let name = self.services.list.get(playlist).map_or_else(String::new, |list| list.name.clone());
        self.removed.push(Removed::Items(RemovedItems { playlist, name, items: items.clone() }));
        if playlist == self.services.current {
            self.item_picks = crate::home::ItemPicks::default();
        }
        items
    }

    /// Deletes a playlist, by its place: the playlist in front stays in front, unless it is
    /// this one. The last playlist can't go. True when it went.
    pub fn delete_playlist(&mut self, at: usize) -> bool {
        let Some(playlist) = self.services.list.get(at).cloned() else { return false };
        if !self.services.remove_at(at) {
            return false;
        }
        self.renaming_service = None;
        self.removed.push(Removed::Playlist { at, playlist });
        true
    }

    /// Deletes a song library (its songs stay in All Songs). True when it went.
    pub fn delete_library(&mut self, id: sp_model::Uuid) -> bool {
        let Some(at) = self.libraries.iter().position(|library| library.id == id) else { return false };
        let library = self.libraries.remove(at);
        if self.library_shown == Some(id) {
            self.library_shown = None;
        }
        self.libraries_changed = true;
        self.removed.push(Removed::Library { at, library });
        true
    }

    /// Deletes an audio playlist, by its place (not the last); a track of it that is playing
    /// plays on to its end. The one before it is shown. True when it went.
    pub fn delete_audio_playlist(&mut self, at: usize) -> bool {
        let Some(playlist) = self.audio.playlists.get(at).cloned() else { return false };
        if !self.audio.remove_playlist(at, self.audio_playing.track) {
            return false;
        }
        self.audio.current = at.saturating_sub(1).min(self.audio.playlists.len() - 1);
        self.audio_changed = true;
        self.removed.push(Removed::AudioPlaylist { at, playlist });
        true
    }

    /// Deletes a timer, by its place. Returns it.
    pub fn delete_timer(&mut self, at: usize) -> Option<crate::timers::Timer> {
        (at < self.timers.len()).then(|| {
            let timer = self.timers.remove(at);
            self.timers_changed = true;
            self.removed.push(Removed::Timer { at, timer: timer.clone() });
            timer
        })
    }

    /// Deletes a message, by its place; it comes off the screens if it is showing. Returns it.
    pub fn delete_message(&mut self, at: usize) -> Option<crate::messages::Message> {
        (at < self.messages.len()).then(|| {
            let message = self.messages.remove(at);
            if self.message_live.is_some_and(|(live, _)| live == message.id) {
                self.message_live = None;
            }
            self.messages_changed = true;
            self.removed.push(Removed::Message { at, message: message.clone() });
            message
        })
    }

    /// Deletes a prop, by its place; it comes off the screens if it is showing, and the Edit
    /// view stays on a prop (see `take_prop`). Returns it.
    pub fn delete_prop(&mut self, at: usize) -> Option<crate::props::Prop> {
        let in_editor = self.editing.as_ref().is_some_and(|draft| draft.at.presentation == crate::props::PROP_SLOT);
        let prop = self.take_prop(at)?;
        self.removed.push(Removed::Prop { at, prop: prop.clone() });
        if in_editor {
            self.note_deleted_in_editor();
        }
        Some(prop)
    }

    /// Takes a prop out of the list (it comes off the screens), keeping the Edit view's props on
    /// a prop when they are open: the one open, wherever it went, or the one now in its place
    /// (the one before, for the last); with none left, the empty list of props stays open.
    pub fn take_prop(&mut self, at: usize) -> Option<crate::props::Prop> {
        if at >= self.props.len() {
            return None;
        }
        let editing_props = self.editing.as_ref().is_some_and(|draft| draft.at.presentation == crate::props::PROP_SLOT);
        let open = self.editing.as_ref().filter(|_| editing_props).map(|draft| draft.at.slide);
        let open_id = open.and_then(|open| self.props.get(open)).map(|prop| prop.id);
        let prop = self.props.remove(at);
        self.props_live.retain(|(live, _)| *live != prop.id);
        self.props_changed = true;
        if editing_props {
            if self.props.is_empty() {
                self.editing = None;
                self.props_editor_open = true;
            } else {
                match open_id.and_then(|id| self.props.iter().position(|prop| prop.id == id)) {
                    Some(still) => {
                        if let Some(draft) = self.editing.as_mut() {
                            draft.at.slide = still;
                        }
                    }
                    None => self.editing = crate::studio::prop_draft(self, at.min(self.props.len() - 1)),
                }
            }
        }
        Some(prop)
    }

    /// Something was just deleted from one of the Edit view's lists: it takes its turn in the
    /// Edit view's undo order, so Command-Z there puts it back (from the app's history).
    pub fn note_deleted_in_editor(&mut self) {
        if self.editing.is_some() {
            self.edit_steps.push(crate::home::EditStep::Deleted);
            self.edit_redo.clear();
        }
    }

    /// Deletes a macro, by its place. Returns it.
    pub fn delete_macro(&mut self, at: usize) -> Option<crate::macros::Macro> {
        (at < self.macros.len()).then(|| {
            let made = self.macros.remove(at);
            self.macros_changed = true;
            self.removed.push(Removed::Macro { at, made: made.clone() });
            made
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window with three of everything a delete here covers, each named One, Two, Three.
    fn three_of_each() -> HomeState {
        let names = ["One", "Two", "Three"];
        let mut state = HomeState::default();
        state.services.list = names
            .iter()
            .map(|name| Service { name: (*name).to_owned(), items: Vec::new(), uids: Default::default() })
            .collect();
        state.libraries = names.iter().map(|name| crate::libraries::Library::new((*name).to_owned())).collect();
        state.audio.playlists = names.iter().map(|name| crate::audio::Playlist::new((*name).to_owned())).collect();
        state.timers = names.iter().map(|name| crate::timers::Timer::new((*name).to_owned())).collect();
        state.messages = names.iter().map(|name| crate::messages::Message::new((*name).to_owned())).collect();
        state.props = names.iter().map(|name| crate::props::Prop::new((*name).to_owned())).collect();
        state.macros = names.iter().map(|name| crate::macros::Macro::new((*name).to_owned())).collect();
        state
    }

    /// The names in every list, in order.
    fn names(state: &HomeState) -> Vec<Vec<String>> {
        vec![
            state.services.list.iter().map(|it| it.name.clone()).collect(),
            state.libraries.iter().map(|it| it.name.clone()).collect(),
            state.audio.playlists.iter().map(|it| it.name.clone()).collect(),
            state.timers.iter().map(|it| it.name.clone()).collect(),
            state.messages.iter().map(|it| it.name.clone()).collect(),
            state.props.iter().map(|it| it.name.clone()).collect(),
            state.macros.iter().map(|it| it.name.clone()).collect(),
        ]
    }

    #[test]
    fn each_delete_goes_at_once_and_undo_puts_it_back_where_it_was() {
        let mut state = three_of_each();
        let before = names(&state);
        let library = state.libraries[1].id;
        assert!(state.delete_playlist(1));
        assert!(state.delete_library(library));
        assert!(state.delete_audio_playlist(1));
        assert!(state.delete_timer(1).is_some());
        assert!(state.delete_message(1).is_some());
        assert!(state.delete_prop(1).is_some());
        assert!(state.delete_macro(1).is_some());
        let gone: Vec<Vec<String>> = before.iter().map(|list| vec![list[0].clone(), list[2].clone()]).collect();
        assert_eq!(names(&state), gone, "the middle one of each went");
        assert_eq!(state.removed.len(), 7, "each kept for Undo");
        let removed = std::mem::take(&mut state.removed);
        // Undo, newest first, puts each back in its place; Redo deletes each again.
        for step in removed.iter().rev() {
            assert!(step.redo(&mut state, true), "{step:?}");
        }
        assert_eq!(names(&state), before);
        for step in &removed {
            assert!(step.redo(&mut state, false), "{step:?}");
        }
        assert_eq!(names(&state), gone);
        assert!(state.removed.is_empty(), "Undo and Redo keep nothing more");
        assert_eq!(removed[3].said(true), "Put the timer Two back.");
        assert_eq!(removed[0].said(false), "Deleted the playlist Two again.");
    }

    #[test]
    fn a_playlist_or_library_put_back_is_the_one_shown_again() {
        let mut state = three_of_each();
        state.services.current = 2;
        assert!(state.delete_playlist(0));
        assert_eq!(state.services.front().name, "Three", "the one in front stays in front");
        let step = state.removed.pop().expect("kept");
        state.playlist_open = false;
        assert!(step.redo(&mut state, true));
        assert_eq!((state.services.front().name.as_str(), state.playlist_open), ("One", true), "picked");
        // A playlist changed since isn't deleted again by Redo.
        state.services.list[0].items.push(Item::Header { name: "New".to_owned() });
        assert!(!step.redo(&mut state, false));
        assert_eq!(state.services.list.len(), 3);
        assert_eq!(step.changed_since(), "The playlist One has changed since, so that can't be undone.");

        let library = state.libraries[2].id;
        state.library_shown = Some(library);
        assert!(state.delete_library(library));
        assert_eq!(state.library_shown, None, "All Songs is shown in its place");
        let step = state.removed.pop().expect("kept");
        state.playlist_open = true;
        assert!(step.redo(&mut state, true));
        assert_eq!((state.library_shown, state.playlist_open), (Some(library), false), "picked");
    }

    #[test]
    fn the_last_playlist_or_audio_playlist_stays_and_nothing_is_kept() {
        let mut state = HomeState::default();
        assert!(!state.delete_playlist(0));
        assert!(!state.delete_audio_playlist(0));
        assert!(state.delete_timer(0).is_none());
        assert!(state.removed.is_empty());
    }

    #[test]
    fn pictures_taken_out_of_a_media_playlist_come_back_where_they_were() {
        let mut state = HomeState::default();
        let mut playlist = crate::media::Playlist::new("Loop".to_owned());
        playlist.files = ["a.png", "b.png", "c.png", "d.png"].iter().map(std::path::PathBuf::from).collect();
        let id = playlist.id;
        state.media_playlists = vec![playlist];
        let gone = Removed::MediaPlaylistFiles {
            playlist: id,
            name: "Loop".to_owned(),
            files: vec![(1, "b.png".into()), (3, "d.png".into())],
        };
        state.media_playlists[0].files.retain(|file| file != "b.png" && file != "d.png");
        let files = |state: &HomeState| state.media_playlists[0].files.clone();
        assert!(gone.redo(&mut state, true));
        assert_eq!(files(&state), ["a.png", "b.png", "c.png", "d.png"].map(std::path::PathBuf::from));
        assert!(gone.redo(&mut state, false));
        assert_eq!(files(&state), ["a.png", "c.png"].map(std::path::PathBuf::from));
        assert_eq!(gone.said(true), "Put 2 items back in Loop.");
        // Gone since: nothing to put back into.
        state.media_playlists.clear();
        assert!(!gone.redo(&mut state, true));
    }

    #[test]
    fn a_message_or_prop_deleted_comes_off_the_screens_and_stays_off_when_put_back() {
        let mut state = three_of_each();
        let now = std::time::Instant::now();
        state.message_live = Some((state.messages[0].id, now));
        state.props_live = vec![(state.props[0].id, now)];
        state.delete_message(0);
        state.delete_prop(0);
        assert!(state.message_live.is_none() && state.props_live.is_empty());
        for step in std::mem::take(&mut state.removed).iter().rev() {
            step.redo(&mut state, true);
        }
        assert_eq!(state.messages[0].name, "One");
        assert!(state.message_live.is_none() && state.props_live.is_empty(), "back in the list, not on screen");
    }
}
