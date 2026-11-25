//! Theme versions while the show runs. A version (the offering, say) is a set of templates a
//! theme keeps beside its usual ones; while it is on, every audience screen whose theme has it
//! draws with it. It is switched on by hand (the button beside Continuous), by a macro, from the
//! command line, or by a playlist item set to show with it. When the next item goes on screen it
//! is back to the usual look, unless it was kept; then it stays until switched back.

use sp_model::Uuid;

use crate::i18n::{t, tf};
use crate::service::ShowWith;

/// Something asked about theme versions from the window, a macro or the command line, for the
/// app to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Show the screens in a version, or in the usual look (`None`), kept after the item or not.
    Use { version: Option<Uuid>, keep: bool },
    /// Keep the version on now after its item, or not.
    Keep(bool),
    /// Give a theme (by its place in the list) a version by name: the one another theme has by
    /// that name, or a new one. Its templates are copies of the theme's usual ones at `from`.
    Add { theme: usize, name: String, from: Vec<usize> },
    /// Add a copy of one of a theme's usual templates to its version.
    AddCopy { theme: usize, version: Uuid, from: usize },
    /// Rename a version in every theme that has it.
    Rename { version: Uuid, name: String },
    /// Take a version out of a theme, with its templates.
    Remove { theme: usize, version: Uuid },
}

/// What a slide on screen is from: its song (its place in the show) and, when it was put on
/// screen from the playlist in front, the item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Showing {
    pub song: usize,
    pub item: Option<usize>,
}

impl Showing {
    /// The same item: the same song, and the same item of the playlist when both are known (a
    /// song the playlist holds twice is two items).
    fn same(self, other: Showing) -> bool {
        self.song == other.song && (self.item == other.item || self.item.is_none() || other.item.is_none())
    }
}

/// A version switched on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct On {
    pub version: Uuid,
    /// The item it is on for; none while it waits for the next item to go on screen.
    pub from: Option<Showing>,
    /// Kept after its item, until it is switched back.
    pub keep: bool,
}

/// The version the screens are in, and the item last on screen.
#[derive(Clone, Debug, Default)]
pub struct Switch {
    pub on: Option<On>,
    last: Option<Showing>,
}

impl Switch {
    /// The version the screens draw with now, if any.
    pub fn version(&self) -> Option<Uuid> {
        self.on.map(|on| on.version)
    }

    /// Switched to a version, or back to the usual look, by hand, a macro or the command line.
    /// With a slide on screen it is that item's version; with nothing on screen it waits for
    /// the next item.
    pub fn use_version(&mut self, version: Option<Uuid>, keep: bool, showing: Option<Showing>) {
        self.on = version.map(|version| On { version, from: showing, keep });
    }

    /// Kept after its item, or not, for the version on now.
    pub fn set_keep(&mut self, keep: bool) {
        if let Some(on) = self.on.as_mut() {
            on.keep = keep;
        }
    }

    /// A slide is on screen from `here`, whose playlist item asks to be shown in `asked`.
    /// Another item than the one the version is on for ends it, unless it is kept; a version
    /// waiting for the next item takes this one; an item set to a version brings it when it goes
    /// up. Returns true when the version changed.
    pub fn slide_on(&mut self, here: Showing, asked: Option<ShowWith>) -> bool {
        let before = self.on;
        let new_item = self.last.is_none_or(|last| !last.same(here));
        self.last = Some(here);
        // Whether the version on is this item's: switched on with it, or waiting for it.
        let mut on_for_it = false;
        if let Some(on) = self.on.as_mut() {
            match on.from {
                None => {
                    on.from = Some(here);
                    on_for_it = true;
                }
                Some(from) if from.same(here) => on_for_it = true,
                // Kept, it goes on with the item now on screen: should it stop being kept, the
                // next item ends it.
                Some(_) if on.keep => on.from = Some(here),
                Some(_) => self.on = None,
            }
        }
        if new_item
            && !on_for_it
            && let Some(asked) = asked
        {
            self.on = Some(On { version: asked.version, from: Some(here), keep: asked.keep });
        }
        self.on != before
    }

    /// True while a playlist item (by its place) is the one the version on is on for, in this
    /// version: its pill and its button light while it is on screen.
    pub fn lights(&self, item: usize, version: Uuid) -> bool {
        self.on.is_some_and(|on| on.version == version && on.from.is_some_and(|from| from.item == Some(item)))
    }

    /// A version no theme has any more is switched off. Returns true if it was on.
    pub fn forget_unless(&mut self, have: impl Fn(Uuid) -> bool) -> bool {
        let gone = self.on.is_some_and(|on| !have(on.version));
        if gone {
            self.on = None;
        }
        gone
    }
}

/// Playlist items set to a version no theme has any more go back to the usual look. Returns
/// true if any did.
pub fn forget_in_playlists(services: &mut crate::service::Services, have: impl Fn(Uuid) -> bool) -> bool {
    let mut changed = false;
    for item in services.list.iter_mut().flat_map(|list| list.items.iter_mut()) {
        if let Some(set) = item.show_with_mut()
            && set.is_some_and(|wanted| !have(wanted.version))
        {
            *set = None;
            changed = true;
        }
    }
    if changed {
        services.changed = true;
    }
    changed
}

/// Where a word is among `words`, splitting them there: the words before it and after it.
fn split_at<'a>(words: &'a [String], keyword: &str) -> (&'a [String], Option<&'a [String]>) {
    match words.iter().rposition(|word| word.eq_ignore_ascii_case(keyword)) {
        Some(at) => (&words[..at], Some(&words[at + 1..])),
        None => (words, None),
    }
}

impl crate::App {
    /// Carries out what the window or a macro asked about versions this frame; a problem is
    /// said as a refusal's cue (nothing was done).
    pub(crate) fn handle_versions(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        for ask in std::mem::take(&mut running.home_state.version_asks) {
            if let Err(problem) = self.version_ask(ask)
                && let Some(running) = self.running.as_mut()
            {
                running.home_state.notices.refuse(problem);
                running.home_window.request_redraw();
            }
        }
    }

    /// The version a name (or number) stands for, among those the themes have.
    fn version_named(&self, typed: &str) -> Result<sp_model::Version, String> {
        let versions = sp_model::all_versions(&self.themes);
        if versions.is_empty() {
            return Err("No theme has a version yet: sp version add NAME to THEME".to_owned());
        }
        let names: Vec<String> = versions.iter().map(|version| version.name.clone()).collect();
        let at = crate::control::find("version", &names, typed)?;
        Ok(versions[at].clone())
    }

    /// Carries out one thing asked about versions. Returns what was done, in words.
    pub(crate) fn version_ask(&mut self, ask: Ask) -> Result<String, String> {
        let name_of = |themes: &[sp_model::Theme], id: Uuid| {
            sp_model::all_versions(themes).into_iter().find(|version| version.id == id).map(|version| version.name)
        };
        match ask {
            Ask::Use { version, keep } => {
                let name = match version {
                    Some(id) => Some(
                        name_of(&self.themes, id)
                            .ok_or(t("notices.versions.gone", "No theme has that version now."))?,
                    ),
                    None => None,
                };
                let showing = self.showing();
                let state =
                    &mut self.running.as_mut().ok_or(t("notices.not_ready", "The app isn't ready."))?.home_state;
                state.version_switch.use_version(version, keep, showing);
                crate::applog::event("version", name.as_deref().unwrap_or("the usual look"));
                self.refresh_output();
                let kept = if keep { ", kept until switched back" } else { "" };
                Ok(match (name, showing) {
                    (Some(name), Some(_)) => format!("The screens show the {name} version{kept}."),
                    (Some(name), None) => {
                        format!("The {name} version is on from the next item to go on screen{kept}.")
                    }
                    (None, _) => "The screens show the usual look.".to_owned(),
                })
            }
            Ask::Keep(keep) => {
                let state =
                    &mut self.running.as_mut().ok_or(t("notices.not_ready", "The app isn't ready."))?.home_state;
                let on = state
                    .version_switch
                    .on
                    .ok_or(t("notices.versions.none_on", "No version is on: the screens show the usual look."))?;
                state.version_switch.set_keep(keep);
                let name = state.versions.iter().find(|v| v.id == on.version).map_or("", |v| v.name.as_str());
                Ok(if keep {
                    format!("The {name} version stays on until switched back.")
                } else {
                    format!("The {name} version ends when the next item goes on screen.")
                })
            }
            Ask::Add { theme, name, from } => {
                let typed = name.trim();
                if typed.is_empty() {
                    return Err(t("notices.versions.needs_name", "A version needs a name.").to_owned());
                }
                // The same name as another theme's version is that version, so the screens
                // switch together.
                let version = sp_model::all_versions(&self.themes)
                    .into_iter()
                    .find(|have| have.name.to_lowercase() == typed.to_lowercase())
                    .unwrap_or_else(|| sp_model::Version { id: Uuid::new_v4(), name: typed.to_owned() });
                let target = self.themes.get_mut(theme).ok_or(t("notices.theme_gone", "That theme is gone."))?;
                if target.has_version(version.id) {
                    return Err(tf(
                        "notices.versions.has_already",
                        "{theme} has the {version} version already.",
                        &[("theme", &target.name), ("version", &version.name)],
                    ));
                }
                if from.is_empty() {
                    return Err(t(
                        "notices.versions.choose_template",
                        "Choose a template for the version to start from.",
                    )
                    .to_owned());
                }
                // The copies have ids of their own, so they take the names the window shows: a
                // built-in template's in the language set.
                let copied: Vec<String> = from
                    .iter()
                    .filter_map(|at| target.templates.get(*at))
                    .map(crate::theme_store::given_name)
                    .collect();
                if !target.add_version(&version, &from) {
                    return Err(
                        t("notices.versions.templates_missing", "Those templates are not in the theme.").to_owned()
                    );
                }
                if let Some(added) = target.versions.iter_mut().find(|have| have.id == version.id) {
                    for (copy, name) in added.templates.iter_mut().zip(&copied) {
                        copy.name.clone_from(name);
                    }
                }
                let said =
                    format!("{} has the {} version: copies of {}.", target.name, version.name, copied.join(", "));
                self.versions_saved(&[theme], Some((Some(version.id), 0)));
                Ok(said)
            }
            Ask::AddCopy { theme, version, from } => {
                let target = self.themes.get_mut(theme).ok_or(t("notices.theme_gone", "That theme is gone."))?;
                let copied = target.templates.get(from).map(crate::theme_store::given_name).unwrap_or_default();
                let at = target
                    .add_copy_to_version(version, from)
                    .ok_or(t("notices.versions.template_missing", "That template is not in the theme."))?;
                if let Some(copy) = target.set_mut(Some(version)).and_then(|templates| templates.get_mut(at)) {
                    copy.name.clone_from(&copied);
                }
                self.versions_saved(&[theme], Some((Some(version), at)));
                Ok(format!("Added a copy of {copied} to the version."))
            }
            Ask::Rename { version, name } => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return Err(t("notices.versions.needs_name", "A version needs a name.").to_owned());
                }
                let old = name_of(&self.themes, version)
                    .ok_or(t("notices.versions.gone", "No theme has that version now."))?;
                let taken = sp_model::all_versions(&self.themes)
                    .iter()
                    .any(|have| have.id != version && have.name.to_lowercase() == name.to_lowercase());
                if taken {
                    return Err(tf(
                        "notices.versions.name_taken",
                        "Another version is called {name} already.",
                        &[("name", &name)],
                    ));
                }
                let changed: Vec<usize> =
                    (0..self.themes.len()).filter(|at| self.themes[*at].rename_version(version, &name)).collect();
                self.versions_saved(&changed, None);
                Ok(format!("Renamed the {old} version to {name}, in every theme that has it."))
            }
            // At once, asking nothing: Undo puts it back, with its templates.
            Ask::Remove { theme, version } => {
                let old = name_of(&self.themes, version)
                    .ok_or(t("notices.versions.gone", "No theme has that version now."))?;
                let target = self.themes.get_mut(theme).ok_or(t("notices.theme_gone", "That theme is gone."))?;
                let theme_name = target.name.clone();
                let place = target.versions.iter().position(|have| have.id == version);
                let Some(gone) = place.map(|at| target.versions[at].clone()).filter(|_| target.remove_version(version))
                else {
                    return Err(tf(
                        "notices.versions.not_in_theme",
                        "{theme} has no {version} version.",
                        &[("theme", &theme_name), ("version", &old)],
                    ));
                };
                let removed =
                    crate::removed::Removed::Version { theme: target.id, at: place.unwrap_or(0), version: gone };
                if let Some(running) = self.running.as_mut() {
                    running.home_state.removed.push(removed);
                }
                self.versions_saved(&[theme], Some((None, 0)));
                let left = self.themes.iter().any(|theme| theme.has_version(version));
                Ok(if left {
                    format!("Took the {old} version out of {theme_name}.")
                } else {
                    format!(
                        "Took the {old} version out of {theme_name}. No theme has it now, so it is gone, and \
                         items set to it show in the usual look."
                    )
                })
            }
        }
    }

    /// Puts back a template or a version deleted in the Theme Editor (`back`), where it was in
    /// its theme and open there if the editor shows that theme, or deletes it again (a redo).
    /// A version's last template comes back with its version. False, changing nothing, when the
    /// theme is gone or has changed so much that it can't be.
    pub(crate) fn redo_theme_removed(&mut self, removed: &crate::removed::Removed, back: bool) -> bool {
        use crate::removed::Removed;
        let (Removed::Template { theme, .. } | Removed::Version { theme, .. }) = removed else { return false };
        let Some(index) = self.themes.iter().position(|have| have.id == *theme) else { return false };
        let Some(show) = removed.redo_in_theme(&mut self.themes[index], back) else { return false };
        // A template for Bible slides put back in the main theme takes the translations back.
        let bible =
            matches!(removed, Removed::Template { template, .. } if template.bible) && index == self.songs_theme;
        self.versions_saved(&[index], Some(show));
        if bible && let Some(running) = self.running.as_mut() {
            running.home_state.bible.assign_boxes();
        }
        true
    }

    /// After versions changed in these themes (by their places): each is saved, the Theme
    /// Editor's copy of one brought up to date (showing `show`, a set and a template of it, when
    /// given), and everything drawn with them drawn again.
    fn versions_saved(&mut self, changed: &[usize], show: Option<(Option<Uuid>, usize)>) {
        for at in changed {
            if !self.themes_unsaved.contains(at) {
                self.themes_unsaved.push(*at);
            }
        }
        self.unsaved_since = Some(std::time::Instant::now());
        self.themes_changed();
        if let Some(running) = self.running.as_mut()
            && let Some(edit) = running.home_state.theme_edit.as_mut()
            && changed.contains(&edit.index)
            && let Some(theme) = self.themes.get(edit.index)
        {
            edit.theme = theme.clone();
            let (version, template) = show.unwrap_or((edit.version, edit.template));
            running.home_state.editing = edit.show(version, template);
            running.home_state.renaming_template = None;
            running.canvas_for = None;
        }
        self.refresh_output();
        self.save_unsaved();
    }

    /// `version list | add NAME to THEME [from TEMPLATE…] | rename OLD NEW | remove NAME from
    /// THEME | use NAME [--keep] | off | keep on|off`.
    pub(crate) fn ctl_version(&mut self, args: &[String]) -> Result<crate::control::Reply, String> {
        use crate::control::{Reply, find};
        let usage = "version list | version add NAME to THEME [from TEMPLATE…] | version rename OLD NEW | \
                     version remove NAME from THEME | version use NAME [--keep] | version off | version keep on|off";
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_else(|| "list".to_owned());
        let words: Vec<String> = args.iter().skip(2).filter(|w| *w != "--yes" && *w != "--keep").cloned().collect();
        let flag = |name: &str| args.iter().any(|w| w == name);
        let theme_names: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
        match word.as_str() {
            "list" => {
                let on = self.running.as_ref().and_then(|r| r.home_state.version_switch.on);
                let mut lines = Vec::new();
                let mut data = Vec::new();
                for version in sp_model::all_versions(&self.themes) {
                    let themes: Vec<(&str, Vec<&str>)> = self
                        .themes
                        .iter()
                        .filter(|theme| theme.has_version(version.id))
                        .map(|theme| {
                            let templates = theme.set(Some(version.id)).iter().map(|t| t.name.as_str()).collect();
                            (theme.name.as_str(), templates)
                        })
                        .collect();
                    let is_on = on.filter(|on| on.version == version.id);
                    let state = match is_on {
                        Some(on) if on.keep => " (on, kept until switched back)",
                        Some(on) if on.from.is_none() => " (on from the next item)",
                        Some(_) => " (on)",
                        None => "",
                    };
                    let listed: Vec<String> =
                        themes.iter().map(|(theme, templates)| format!("{theme}: {}", templates.join(", "))).collect();
                    lines.push(format!("{}{state} — {}", version.name, listed.join("; ")));
                    data.push(serde_json::json!({
                        "name": version.name,
                        "id": version.id,
                        "on": is_on.is_some(),
                        "kept": is_on.is_some_and(|on| on.keep),
                        "themes": themes.iter().map(|(theme, templates)| serde_json::json!({"theme": theme, "templates": templates})).collect::<Vec<_>>(),
                    }));
                }
                if lines.is_empty() {
                    lines.push("No theme has a version yet: sp version add NAME to THEME".to_owned());
                }
                Ok(Reply::with(lines.join("\n"), serde_json::json!(data)))
            }
            "add" => {
                let usage = "version add NAME to THEME [from TEMPLATE…]   (each template one word, or quoted)";
                let (name, rest) = match split_at(&words, "to") {
                    (name, Some(rest)) if !name.is_empty() && !rest.is_empty() => (name.join(" "), rest),
                    _ => return Err(usage.to_owned()),
                };
                let (theme_words, from) = split_at(rest, "from");
                let theme = find("theme", &theme_names, &theme_words.join(" "))?;
                let usual = &self.themes[theme].templates;
                let template_names: Vec<String> = usual.iter().map(|t| t.name.clone()).collect();
                let from: Vec<usize> = match from {
                    Some(typed) if !typed.is_empty() => typed
                        .iter()
                        .map(|one| {
                            find("template", &template_names, one).map_err(|problem| {
                                format!("{problem} {}'s templates: {}.", theme_names[theme], template_names.join(", "))
                            })
                        })
                        .collect::<Result<_, _>>()?,
                    // As the sheet starts: copies of the templates for songs.
                    _ => (0..usual.len()).filter(|at| usual[*at].songs).collect(),
                };
                self.version_ask(Ask::Add { theme, name, from }).map(Reply::done)
            }
            "rename" => {
                let (old, new) = match split_at(&words, "to") {
                    (old, Some(new)) if !old.is_empty() && !new.is_empty() => (old.join(" "), new.join(" ")),
                    _ => match words.as_slice() {
                        [old, new @ ..] if !new.is_empty() => (old.clone(), new.join(" ")),
                        _ => return Err("version rename OLD NEW   (quote names with spaces)".to_owned()),
                    },
                };
                let version = self.version_named(&old)?;
                self.version_ask(Ask::Rename { version: version.id, name: new }).map(Reply::done)
            }
            "remove" => {
                let (name, theme_words) = match split_at(&words, "from") {
                    (name, Some(theme)) if !name.is_empty() && !theme.is_empty() => (name.join(" "), theme.join(" ")),
                    _ => return Err("version remove NAME from THEME".to_owned()),
                };
                let version = self.version_named(&name)?;
                let theme = find("theme", &theme_names, &theme_words)?;
                // At once, as in the window: `sp undo` puts it back (an old --yes is let by).
                self.version_ask(Ask::Remove { theme, version: version.id }).map(Reply::done)
            }
            "use" => {
                let version = self.version_named(&words.join(" "))?;
                self.version_ask(Ask::Use { version: Some(version.id), keep: flag("--keep") }).map(Reply::done)
            }
            "off" | "usual" => self.version_ask(Ask::Use { version: None, keep: false }).map(Reply::done),
            "keep" => match words.first().map(|w| w.to_lowercase()).as_deref() {
                Some("on") => self.version_ask(Ask::Keep(true)).map(Reply::done),
                Some("off") => self.version_ask(Ask::Keep(false)).map(Reply::done),
                _ => Err("version keep on|off".to_owned()),
            },
            _ => Err(usage.to_owned()),
        }
    }

    /// `playlist item N show-with NAME|usual [--keep] [in PLAYLIST]`: the version a song or a
    /// passage of a playlist is shown in when it goes on screen, kept after it or not.
    pub(crate) fn playlist_show_with(&mut self, words: &[String]) -> Result<crate::control::Reply, String> {
        let usage = "playlist item N show-with NAME|usual [--keep] [in PLAYLIST]";
        let keep = words.iter().any(|w| w == "--keep");
        let words: Vec<String> = words.iter().filter(|w| *w != "--keep").cloned().collect();
        let (head, list) = split_at(&words, "in");
        let (item, wanted) = match split_at(head, "show-with") {
            (item, Some(wanted)) if !item.is_empty() && !wanted.is_empty() => (item.join(" "), wanted.join(" ")),
            _ => return Err(usage.to_owned()),
        };
        let list = self.playlist_at_for_cli(list.map(|words| words.join(" ")).as_deref())?;
        let at = {
            let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
            let titles: Vec<String> = state.services.list[list].items.iter().map(|i| i.title().to_owned()).collect();
            crate::control::find("item", &titles, &item)?
        };
        let version = if wanted.eq_ignore_ascii_case("usual") { None } else { Some(self.version_named(&wanted)?) };
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let entry = &mut state.services.list[list].items[at];
        let title = entry.title().to_owned();
        let set = entry.show_with_mut().ok_or("Only a song or a passage can be shown in a version.")?;
        *set = version.as_ref().map(|version| crate::service::ShowWith { version: version.id, keep });
        state.services.changed = true;
        Ok(crate::control::Reply::done(match version {
            Some(version) if keep => {
                format!("{title} shows in the {} version, kept after it until switched back.", version.name)
            }
            Some(version) => format!("{title} shows in the {} version.", version.name),
            None => format!("{title} shows in the usual look."),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFERING: Uuid = Uuid::from_u128(1);
    const COMMUNION: Uuid = Uuid::from_u128(2);

    fn item(song: usize, item: usize) -> Showing {
        Showing { song, item: Some(item) }
    }

    #[test]
    fn an_item_set_to_a_version_brings_it_and_the_next_item_ends_it() {
        let mut switch = Switch::default();
        let offering = Some(ShowWith { version: OFFERING, keep: false });
        assert!(!switch.slide_on(item(0, 0), None));
        assert!(switch.slide_on(item(1, 1), offering));
        assert_eq!(switch.version(), Some(OFFERING));
        assert!(switch.lights(1, OFFERING) && !switch.lights(0, OFFERING) && !switch.lights(1, COMMUNION));
        // Its next slide keeps it; so does the same item put on screen again.
        assert!(!switch.slide_on(item(1, 1), offering));
        assert_eq!(switch.version(), Some(OFFERING));
        // The next item is in the usual look.
        assert!(switch.slide_on(item(2, 2), None));
        assert_eq!(switch.version(), None);
        // The same song held twice in the playlist is two items.
        switch.slide_on(item(1, 1), offering);
        assert!(switch.slide_on(item(1, 5), None));
        assert_eq!(switch.version(), None);
    }

    #[test]
    fn a_kept_version_stays_after_its_item_until_switched_back() {
        let mut switch = Switch::default();
        switch.slide_on(item(1, 1), Some(ShowWith { version: OFFERING, keep: true }));
        switch.slide_on(item(2, 2), None);
        switch.slide_on(item(3, 3), None);
        assert_eq!(switch.version(), Some(OFFERING));
        // Another item set to another version brings that one, on its own terms.
        switch.slide_on(item(4, 4), Some(ShowWith { version: COMMUNION, keep: false }));
        assert_eq!(switch.version(), Some(COMMUNION));
        switch.slide_on(item(5, 5), None);
        assert_eq!(switch.version(), None);
        // Kept by hand from the menu, it outlives the item too; no longer kept, the next item
        // ends it, even the one it was switched on with.
        switch.use_version(Some(OFFERING), false, Some(item(5, 5)));
        switch.set_keep(true);
        switch.slide_on(item(6, 6), None);
        assert_eq!(switch.version(), Some(OFFERING));
        switch.set_keep(false);
        switch.slide_on(item(5, 5), None);
        assert_eq!(switch.version(), None);
        switch.use_version(Some(OFFERING), true, Some(item(6, 6)));
        switch.use_version(None, false, Some(item(6, 6)));
        assert_eq!(switch.version(), None);
    }

    #[test]
    fn switched_on_with_nothing_on_screen_it_waits_for_the_next_item() {
        let mut switch = Switch::default();
        switch.use_version(Some(OFFERING), false, None);
        assert_eq!(switch.on.and_then(|on| on.from), None);
        // The next item takes it, and the one after that ends it.
        switch.slide_on(item(3, 3), None);
        assert_eq!(switch.on.and_then(|on| on.from), Some(item(3, 3)));
        switch.slide_on(item(3, 3), None);
        assert_eq!(switch.version(), Some(OFFERING));
        switch.slide_on(item(4, 4), None);
        assert_eq!(switch.version(), None);
        // Switched on with a slide on screen, it is that item's.
        switch.use_version(Some(OFFERING), false, Some(item(4, 4)));
        switch.slide_on(item(4, 4), None);
        assert_eq!(switch.version(), Some(OFFERING));
        switch.slide_on(item(5, 5), None);
        assert_eq!(switch.version(), None);
    }

    #[test]
    fn switched_back_by_hand_the_item_on_screen_stays_in_the_usual_look() {
        let mut switch = Switch::default();
        let offering = Some(ShowWith { version: OFFERING, keep: false });
        switch.slide_on(item(1, 1), offering);
        switch.use_version(None, false, Some(item(1, 1)));
        // Its next slide does not bring it back; going up again after another item does.
        switch.slide_on(item(1, 1), offering);
        assert_eq!(switch.version(), None);
        switch.slide_on(item(2, 2), None);
        switch.slide_on(item(1, 1), offering);
        assert_eq!(switch.version(), Some(OFFERING));
    }

    #[test]
    fn a_version_no_theme_has_any_more_goes_off_and_items_set_to_it_go_back_to_usual() {
        let mut switch = Switch::default();
        switch.use_version(Some(OFFERING), true, None);
        assert!(!switch.forget_unless(|version| version == OFFERING));
        assert!(switch.forget_unless(|version| version == COMMUNION));
        assert_eq!(switch.version(), None);
        let mut services = crate::service::Services::default();
        let mut song = crate::service::Item::song("a", "A");
        *song.show_with_mut().unwrap() = Some(ShowWith { version: OFFERING, keep: true });
        let mut other = crate::service::Item::song("b", "B");
        *other.show_with_mut().unwrap() = Some(ShowWith { version: COMMUNION, keep: false });
        services.list[0].items = vec![song, other, crate::service::Item::Header { name: "Word".to_owned() }];
        assert!(forget_in_playlists(&mut services, |version| version == COMMUNION));
        let left: Vec<Option<ShowWith>> = services.list[0].items.iter().map(|item| item.show_with()).collect();
        assert_eq!(left, [None, Some(ShowWith { version: COMMUNION, keep: false }), None]);
        assert!(services.changed);
        assert!(!forget_in_playlists(&mut services, |_| true));
    }
}
