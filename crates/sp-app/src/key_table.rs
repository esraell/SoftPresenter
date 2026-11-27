//! What each key that runs the show does (Settings › Keys): Esc, F1 to F8 (off a Mac), Space,
//! the arrows, Home and End, each doing one action. A style gives every key the action it comes
//! with: SoftPresenter's (Esc and F1 clear everything, F2 to F6 one layer each) or SoftProjector's
//! (Esc hides the words, F1 clears everything, F5 shows, F2 adds to the playlist). Only the keys
//! changed from their style's action are kept, with the preferences, by name. A name this copy
//! doesn't know (a newer copy's key or action) is passed over, and that key does what its style
//! says.
//!
//! ↑ and ↓, Enter, / and the letters are not in the table: what they do is fixed (`main.rs`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::i18n::t;

/// A key that runs the show and that Settings › Keys can change, in the order of its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShowKey {
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    Space,
    Right,
    Left,
    Home,
    End,
}

impl ShowKey {
    /// Every key, in the order of the rows.
    #[allow(dead_code)] // Settings › Keys lists its rows from it (`rows`), next.
    const ALL: [ShowKey; 14] = [
        ShowKey::Escape,
        ShowKey::F1,
        ShowKey::F2,
        ShowKey::F3,
        ShowKey::F4,
        ShowKey::F5,
        ShowKey::F6,
        ShowKey::F7,
        ShowKey::F8,
        ShowKey::Space,
        ShowKey::Right,
        ShowKey::Left,
        ShowKey::Home,
        ShowKey::End,
    ];

    /// The show key the window reports, if it is one.
    pub fn of(key: &winit::keyboard::Key) -> Option<ShowKey> {
        use winit::keyboard::{Key, NamedKey};
        let Key::Named(named) = key else { return None };
        Some(match named {
            NamedKey::Escape => ShowKey::Escape,
            NamedKey::F1 => ShowKey::F1,
            NamedKey::F2 => ShowKey::F2,
            NamedKey::F3 => ShowKey::F3,
            NamedKey::F4 => ShowKey::F4,
            NamedKey::F5 => ShowKey::F5,
            NamedKey::F6 => ShowKey::F6,
            NamedKey::F7 => ShowKey::F7,
            NamedKey::F8 => ShowKey::F8,
            NamedKey::Space => ShowKey::Space,
            NamedKey::ArrowRight => ShowKey::Right,
            NamedKey::ArrowLeft => ShowKey::Left,
            NamedKey::Home => ShowKey::Home,
            NamedKey::End => ShowKey::End,
            _ => return None,
        })
    }

    /// The key's name in the preferences file.
    pub fn name(self) -> &'static str {
        match self {
            ShowKey::Escape => "esc",
            ShowKey::F1 => "f1",
            ShowKey::F2 => "f2",
            ShowKey::F3 => "f3",
            ShowKey::F4 => "f4",
            ShowKey::F5 => "f5",
            ShowKey::F6 => "f6",
            ShowKey::F7 => "f7",
            ShowKey::F8 => "f8",
            ShowKey::Space => "space",
            ShowKey::Right => "right",
            ShowKey::Left => "left",
            ShowKey::Home => "home",
            ShowKey::End => "end",
        }
    }

    /// The key as its cap says it, as the list of keys writes it.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn cap(self) -> String {
        match self {
            ShowKey::Escape => "Esc",
            ShowKey::F1 => "F1",
            ShowKey::F2 => "F2",
            ShowKey::F3 => "F3",
            ShowKey::F4 => "F4",
            ShowKey::F5 => "F5",
            ShowKey::F6 => "F6",
            ShowKey::F7 => "F7",
            ShowKey::F8 => "F8",
            ShowKey::Space => t("keys.cap.space", "Space"),
            ShowKey::Right => "\u{2192}",
            ShowKey::Left => "\u{2190}",
            ShowKey::Home => t("keys.cap.home", "Home"),
            ShowKey::End => t("keys.cap.end", "End"),
        }
        .to_owned()
    }

    /// Whether it is an F key, which has a row off a Mac only.
    pub fn is_f_key(self) -> bool {
        matches!(
            self,
            ShowKey::F1
                | ShowKey::F2
                | ShowKey::F3
                | ShowKey::F4
                | ShowKey::F5
                | ShowKey::F6
                | ShowKey::F7
                | ShowKey::F8
        )
    }
}

/// The keys Settings › Keys has a row for, in order: the F keys only with `f_keys` (off a Mac,
/// `keys::f_keys()`), since a Mac's are the system's (brightness, sound, Mission Control).
#[allow(dead_code)] // Settings › Keys reads it, next.
pub fn rows(f_keys: bool) -> Vec<ShowKey> {
    ShowKey::ALL.into_iter().filter(|key| f_keys || !key.is_f_key()).collect()
}

/// What a show key can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyAction {
    ClearAll,
    ClearSlide,
    ClearMedia,
    ClearProps,
    ClearAudio,
    ClearMessages,
    /// The slide's words go; its background, the theme's objects and the media stay.
    HideWords,
    Next,
    Previous,
    First,
    Last,
    /// What Enter puts on screen.
    ShowSelected,
    /// The hidden words back, or the slide the last clear took off.
    ShowAgain,
    AddToPlaylist,
    RemoveFromPlaylist,
    GoToBible,
    GoToService,
    /// Nothing in the show.
    Nothing,
}

/// The parts the chooser of an action lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionGroup {
    Clear,
    Slides,
    PlaylistViews,
    None,
}

impl KeyAction {
    /// Every action, group after group in the chooser's order.
    pub const ALL: [KeyAction; 18] = [
        KeyAction::ClearAll,
        KeyAction::HideWords,
        KeyAction::ClearSlide,
        KeyAction::ClearMedia,
        KeyAction::ClearProps,
        KeyAction::ClearAudio,
        KeyAction::ClearMessages,
        KeyAction::ShowSelected,
        KeyAction::ShowAgain,
        KeyAction::Next,
        KeyAction::Previous,
        KeyAction::First,
        KeyAction::Last,
        KeyAction::AddToPlaylist,
        KeyAction::RemoveFromPlaylist,
        KeyAction::GoToBible,
        KeyAction::GoToService,
        KeyAction::Nothing,
    ];

    /// The action's name in the preferences file.
    pub fn name(self) -> &'static str {
        match self {
            KeyAction::ClearAll => "clear_all",
            KeyAction::ClearSlide => "clear_slide",
            KeyAction::ClearMedia => "clear_media",
            KeyAction::ClearProps => "clear_props",
            KeyAction::ClearAudio => "clear_audio",
            KeyAction::ClearMessages => "clear_messages",
            KeyAction::HideWords => "hide_words",
            KeyAction::Next => "next",
            KeyAction::Previous => "previous",
            KeyAction::First => "first",
            KeyAction::Last => "last",
            KeyAction::ShowSelected => "show_selected",
            KeyAction::ShowAgain => "show_again",
            KeyAction::AddToPlaylist => "add_to_playlist",
            KeyAction::RemoveFromPlaylist => "remove_from_playlist",
            KeyAction::GoToBible => "go_bible",
            KeyAction::GoToService => "go_service",
            KeyAction::Nothing => "nothing",
        }
    }

    /// The action of that name in the preferences file, if this copy knows it.
    pub fn from_name(name: &str) -> Option<KeyAction> {
        KeyAction::ALL.into_iter().find(|action| action.name() == name)
    }

    /// The action in words, as its chooser says it.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn words(self) -> String {
        match self {
            KeyAction::ClearAll => t("settings.keys.do.clear_all", "Clear All"),
            KeyAction::ClearSlide => t("settings.keys.do.clear_slide", "Clear Slide"),
            KeyAction::ClearMedia => t("settings.keys.do.clear_media", "Clear Media"),
            KeyAction::ClearProps => t("settings.keys.do.clear_props", "Clear Props"),
            KeyAction::ClearAudio => t("settings.keys.do.clear_audio", "Clear Audio"),
            KeyAction::ClearMessages => t("settings.keys.do.clear_messages", "Clear Messages"),
            KeyAction::HideWords => t("settings.keys.do.hide_words", "Hide the Words"),
            KeyAction::Next => t("settings.keys.do.next", "Next Slide"),
            KeyAction::Previous => t("settings.keys.do.previous", "Previous Slide"),
            KeyAction::First => t("settings.keys.do.first", "First Slide"),
            KeyAction::Last => t("settings.keys.do.last", "Last Slide"),
            KeyAction::ShowSelected => t("settings.keys.do.show_selected", "Show Selected"),
            KeyAction::ShowAgain => t("settings.keys.do.show_again", "Show Again"),
            KeyAction::AddToPlaylist => t("settings.keys.do.add_to_playlist", "Add to Playlist"),
            KeyAction::RemoveFromPlaylist => t("settings.keys.do.remove_from_playlist", "Remove from Playlist"),
            KeyAction::GoToBible => t("settings.keys.do.go_bible", "Go to Bible"),
            KeyAction::GoToService => t("settings.keys.do.go_service", "Go to Service"),
            KeyAction::Nothing => t("settings.keys.do.nothing", "Nothing"),
        }
        .to_owned()
    }

    /// The part of the chooser the action is listed in.
    pub fn group(self) -> ActionGroup {
        match self {
            KeyAction::ClearAll
            | KeyAction::HideWords
            | KeyAction::ClearSlide
            | KeyAction::ClearMedia
            | KeyAction::ClearProps
            | KeyAction::ClearAudio
            | KeyAction::ClearMessages => ActionGroup::Clear,
            KeyAction::ShowSelected
            | KeyAction::ShowAgain
            | KeyAction::Next
            | KeyAction::Previous
            | KeyAction::First
            | KeyAction::Last => ActionGroup::Slides,
            KeyAction::AddToPlaylist
            | KeyAction::RemoveFromPlaylist
            | KeyAction::GoToBible
            | KeyAction::GoToService => ActionGroup::PlaylistViews,
            KeyAction::Nothing => ActionGroup::None,
        }
    }
}

/// Which program's keys the table starts from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStyle {
    #[default]
    SoftPresenter,
    SoftProjector,
}

/// What every show key does: the style chosen, and the keys changed from it, by name (key name →
/// action name), kept as words so that a name this copy doesn't know is passed over.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "serde_json::Value")]
pub struct KeyTable {
    pub style: KeyStyle,
    changed: BTreeMap<String, String>,
}

impl From<serde_json::Value> for KeyTable {
    /// The table as saved, taking what can be read: a style this copy doesn't know is
    /// SoftPresenter's, and a row saved as anything but a name is passed over. The preferences are
    /// read as one, so a table that failed to read would lose every other choice with it.
    fn from(saved: serde_json::Value) -> KeyTable {
        let style = saved.get("style").and_then(|style| KeyStyle::deserialize(style).ok()).unwrap_or_default();
        let changed = saved
            .get("changed")
            .and_then(serde_json::Value::as_object)
            .map(|rows| {
                rows.iter().filter_map(|(key, action)| Some((key.clone(), action.as_str()?.to_owned()))).collect()
            })
            .unwrap_or_default();
        KeyTable { style, changed }
    }
}

impl KeyTable {
    /// What `key` does: what it was changed to, else what its style says.
    pub fn action(&self, key: ShowKey) -> KeyAction {
        self.changed
            .get(key.name())
            .and_then(|name| KeyAction::from_name(name))
            .unwrap_or_else(|| KeyTable::default_action(self.style, key))
    }

    /// What `key` does in `style`, as it comes.
    pub fn default_action(style: KeyStyle, key: ShowKey) -> KeyAction {
        match (style, key) {
            (_, ShowKey::Space | ShowKey::Right) => KeyAction::Next,
            (_, ShowKey::Left) => KeyAction::Previous,
            (_, ShowKey::Home) => KeyAction::First,
            (_, ShowKey::End) => KeyAction::Last,
            (KeyStyle::SoftPresenter, ShowKey::Escape | ShowKey::F1) => KeyAction::ClearAll,
            (KeyStyle::SoftPresenter, ShowKey::F2) => KeyAction::ClearSlide,
            (KeyStyle::SoftPresenter, ShowKey::F3) => KeyAction::ClearMedia,
            (KeyStyle::SoftPresenter, ShowKey::F4) => KeyAction::ClearProps,
            (KeyStyle::SoftPresenter, ShowKey::F5) => KeyAction::ClearAudio,
            (KeyStyle::SoftPresenter, ShowKey::F6) => KeyAction::ClearMessages,
            (KeyStyle::SoftPresenter, ShowKey::F7 | ShowKey::F8) => KeyAction::Nothing,
            (KeyStyle::SoftProjector, ShowKey::Escape) => KeyAction::HideWords,
            (KeyStyle::SoftProjector, ShowKey::F1) => KeyAction::ClearAll,
            (KeyStyle::SoftProjector, ShowKey::F8) => KeyAction::Nothing,
            (KeyStyle::SoftProjector, ShowKey::F2) => KeyAction::AddToPlaylist,
            (KeyStyle::SoftProjector, ShowKey::F3) => KeyAction::RemoveFromPlaylist,
            (KeyStyle::SoftProjector, ShowKey::F4) => KeyAction::ShowAgain,
            (KeyStyle::SoftProjector, ShowKey::F5) => KeyAction::ShowSelected,
            (KeyStyle::SoftProjector, ShowKey::F6) => KeyAction::GoToBible,
            (KeyStyle::SoftProjector, ShowKey::F7) => KeyAction::GoToService,
        }
    }

    /// Sets what `key` does. Its style's own action is no change, so the row is forgotten.
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn set(&mut self, key: ShowKey, action: KeyAction) {
        if action == KeyTable::default_action(self.style, key) {
            self.changed.remove(key.name());
        } else {
            self.changed.insert(key.name().to_owned(), action.name().to_owned());
        }
    }

    /// Chooses a style: every key does what it says, and the keys changed are changed no more.
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn set_style(&mut self, style: KeyStyle) {
        self.style = style;
        self.changed.clear();
    }

    /// Every key back to what its style says (Reset to Defaults).
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn reset(&mut self) {
        self.changed.clear();
    }

    /// Whether `key` does something other than its style says.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn is_changed(&self, key: ShowKey) -> bool {
        self.action(key) != KeyTable::default_action(self.style, key)
    }

    /// Whether any key does something other than its style says, an F key on a Mac too.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn any_changed(&self) -> bool {
        ShowKey::ALL.into_iter().any(|key| self.is_changed(key))
    }

    /// The keys with a row (the F keys only with `f_keys`) that do `action`, in the rows' order.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn keys_for(&self, action: KeyAction, f_keys: bool) -> Vec<ShowKey> {
        rows(f_keys).into_iter().filter(|key| self.action(*key) == action).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn each_style_gives_the_spec_table() {
        use KeyAction::*;
        use ShowKey::*;
        // The key, then what it does in SoftPresenter's style and in SoftProjector's.
        let table = [
            (Escape, ClearAll, HideWords),
            (F1, ClearAll, ClearAll),
            (F2, ClearSlide, AddToPlaylist),
            (F3, ClearMedia, RemoveFromPlaylist),
            (F4, ClearProps, ShowAgain),
            (F5, ClearAudio, ShowSelected),
            (F6, ClearMessages, GoToBible),
            (F7, Nothing, GoToService),
            (F8, Nothing, Nothing),
            (Space, Next, Next),
            (Right, Next, Next),
            (Left, Previous, Previous),
            (Home, First, First),
            (End, Last, Last),
        ];
        for (key, softpresenter, softprojector) in table {
            assert_eq!(KeyTable::default_action(KeyStyle::SoftPresenter, key), softpresenter, "{key:?}");
            assert_eq!(KeyTable::default_action(KeyStyle::SoftProjector, key), softprojector, "{key:?}");
            assert_eq!(KeyTable::default().action(key), softpresenter, "a new table is SoftPresenter's: {key:?}");
        }
        assert_eq!(table.map(|row| row.0).to_vec(), rows(true), "every row is in the table");
    }

    #[test]
    fn a_mac_has_no_f_rows() {
        use ShowKey::*;
        assert_eq!(rows(false), [Escape, Space, Right, Left, Home, End]);
        assert_eq!(rows(true), [Escape, F1, F2, F3, F4, F5, F6, F7, F8, Space, Right, Left, Home, End]);
    }

    #[test]
    fn a_changed_row_is_kept_and_choosing_its_default_again_forgets_it() {
        let mut table = KeyTable::default();
        table.set(ShowKey::Escape, KeyAction::ClearSlide);
        assert_eq!(table.action(ShowKey::Escape), KeyAction::ClearSlide);
        assert!(table.is_changed(ShowKey::Escape));
        assert!(table.any_changed());
        assert!(!table.is_changed(ShowKey::F1), "the other rows are as they came");
        assert_eq!(table.action(ShowKey::F1), KeyAction::ClearAll);
        table.set(ShowKey::Escape, KeyAction::ClearAll);
        assert!(!table.is_changed(ShowKey::Escape));
        assert!(!table.any_changed());
        assert_eq!(table, KeyTable::default(), "nothing is left of the change");
    }

    #[test]
    fn choosing_a_style_sets_every_row_and_drops_changes() {
        let mut table = KeyTable::default();
        table.set(ShowKey::F2, KeyAction::Nothing);
        table.set_style(KeyStyle::SoftProjector);
        assert_eq!(table.style, KeyStyle::SoftProjector);
        assert_eq!(table.action(ShowKey::F2), KeyAction::AddToPlaylist);
        assert!(!table.any_changed());
        // In this style Clear All on Esc is a change, and Reset goes back to this style's Esc.
        table.set(ShowKey::Escape, KeyAction::ClearAll);
        assert!(table.is_changed(ShowKey::Escape));
        table.reset();
        assert_eq!((table.style, table.action(ShowKey::Escape)), (KeyStyle::SoftProjector, KeyAction::HideWords));
        assert!(!table.any_changed());
    }

    #[test]
    fn keys_for_finds_every_key_doing_an_action() {
        let table = KeyTable::default();
        assert_eq!(table.keys_for(KeyAction::ClearAll, true), [ShowKey::Escape, ShowKey::F1]);
        assert_eq!(table.keys_for(KeyAction::ClearAll, false), [ShowKey::Escape]);
        assert_eq!(table.keys_for(KeyAction::Next, false), [ShowKey::Space, ShowKey::Right]);
        assert!(table.keys_for(KeyAction::ShowAgain, true).is_empty(), "no key does it in this style");
        let mut table = table;
        table.set(ShowKey::F2, KeyAction::ClearAll);
        assert_eq!(table.keys_for(KeyAction::ClearAll, true), [ShowKey::Escape, ShowKey::F1, ShowKey::F2]);
        assert_eq!(table.keys_for(KeyAction::ClearAll, false), [ShowKey::Escape], "an F row only off a Mac");
        // SoftProjector's style clears everything with F1 alone, so a Mac has no key for it there.
        table.set_style(KeyStyle::SoftProjector);
        assert_eq!(table.keys_for(KeyAction::ClearAll, true), [ShowKey::F1]);
        assert!(table.keys_for(KeyAction::ClearAll, false).is_empty());
    }

    #[test]
    fn every_action_is_saved_by_its_name_and_listed_in_its_group() {
        let names: Vec<&str> = KeyAction::ALL.iter().map(|action| action.name()).collect();
        assert_eq!(
            names,
            [
                "clear_all",
                "hide_words",
                "clear_slide",
                "clear_media",
                "clear_props",
                "clear_audio",
                "clear_messages",
                "show_selected",
                "show_again",
                "next",
                "previous",
                "first",
                "last",
                "add_to_playlist",
                "remove_from_playlist",
                "go_bible",
                "go_service",
                "nothing",
            ]
        );
        for action in KeyAction::ALL {
            assert_eq!(KeyAction::from_name(action.name()), Some(action));
        }
        assert_eq!(KeyAction::from_name("fly_away"), None);
        // ALL walks the groups in order, each group's actions together.
        let groups: Vec<ActionGroup> = KeyAction::ALL.iter().map(|action| action.group()).collect();
        let mut expected = vec![ActionGroup::Clear; 7];
        expected.extend([ActionGroup::Slides; 6]);
        expected.extend([ActionGroup::PlaylistViews; 4]);
        expected.push(ActionGroup::None);
        assert_eq!(groups, expected);
        let words: Vec<String> = with_language(Language::English, || KeyAction::ALL.map(KeyAction::words).to_vec());
        assert_eq!(
            words,
            [
                "Clear All",
                "Hide the Words",
                "Clear Slide",
                "Clear Media",
                "Clear Props",
                "Clear Audio",
                "Clear Messages",
                "Show Selected",
                "Show Again",
                "Next Slide",
                "Previous Slide",
                "First Slide",
                "Last Slide",
                "Add to Playlist",
                "Remove from Playlist",
                "Go to Bible",
                "Go to Service",
                "Nothing",
            ]
        );
    }

    #[test]
    fn a_show_key_is_known_by_what_the_window_reports_and_saved_by_its_name() {
        use winit::keyboard::{Key, NamedKey};
        let named = |key| ShowKey::of(&Key::Named(key));
        let reported = [
            (NamedKey::Escape, ShowKey::Escape),
            (NamedKey::F1, ShowKey::F1),
            (NamedKey::F2, ShowKey::F2),
            (NamedKey::F3, ShowKey::F3),
            (NamedKey::F4, ShowKey::F4),
            (NamedKey::F5, ShowKey::F5),
            (NamedKey::F6, ShowKey::F6),
            (NamedKey::F7, ShowKey::F7),
            (NamedKey::F8, ShowKey::F8),
            (NamedKey::Space, ShowKey::Space),
            (NamedKey::ArrowRight, ShowKey::Right),
            (NamedKey::ArrowLeft, ShowKey::Left),
            (NamedKey::Home, ShowKey::Home),
            (NamedKey::End, ShowKey::End),
        ];
        for (key, show_key) in reported {
            assert_eq!(named(key), Some(show_key), "{key:?}");
        }
        for key in [NamedKey::F9, NamedKey::ArrowUp, NamedKey::ArrowDown, NamedKey::Enter] {
            assert_eq!(named(key), None, "{key:?} is no row");
        }
        assert_eq!(ShowKey::of(&Key::Character("a".into())), None);
        let names: Vec<&str> = rows(true).iter().map(|key| key.name()).collect();
        assert_eq!(
            names,
            ["esc", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "space", "right", "left", "home", "end"]
        );
        let caps: Vec<String> = with_language(Language::English, || rows(false).iter().map(|key| key.cap()).collect());
        assert_eq!(caps, ["Esc", "Space", "\u{2192}", "\u{2190}", "Home", "End"]);
        assert_eq!(ShowKey::F7.cap(), "F7");
    }
}
