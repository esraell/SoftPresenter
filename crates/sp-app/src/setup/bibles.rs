//! The setup's Bibles step: every Bible there is, those on this computer and those to
//! download. On view, the recommended one for each of the church's languages (the first in it,
//! one on this computer before one to download); the rest under More Bibles, by language, the
//! church's languages first. With two languages or more each row has On top and Below; with
//! one, a plain choice of one translation. The Bible page's rows take the choice when the step
//! is passed with Continue: one still to download starts downloading then, and takes its row
//! once it is here (`place_downloaded`).

use std::collections::HashMap;

use egui::{Align2, FontId, Rect, Sense, Stroke, pos2, vec2};

use super::{Env, Setup, language_own_name};
use crate::bible::Shelf;
use crate::home::{BiblePage, HomeState};
use crate::i18n::{t, tf};
use crate::studio::{CARD, Flat, HOVER, LINE, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// A Bible the step lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its short name for files, as the shelf and the downloads know it.
    pub id: String,
    pub name: String,
    /// Its language, as a code (`en`).
    pub language: String,
    /// On this computer (built in, or installed); else it downloads with Continue.
    pub here: bool,
    /// The terms one to download is offered under, in a few words.
    pub terms: String,
}

/// A language as a code, from a code or the English name a Bible offered is listed under.
fn code_of(language: &str) -> String {
    crate::bible::iso_code(language).map_or_else(|| language.to_lowercase(), str::to_owned)
}

/// Every Bible there is, in the order the step recommends from: those on this computer as the
/// shelf has them (the ones built in first), then, where Bibles can be downloaded
/// (`download`), those offered that aren't here yet.
pub fn every(shelf: &Shelf, download: bool) -> Vec<Entry> {
    let mut all: Vec<Entry> = shelf
        .bibles
        .iter()
        .map(|bible| Entry {
            id: bible.id.clone(),
            name: bible.name.clone(),
            language: code_of(&bible.language),
            here: true,
            terms: String::new(),
        })
        .collect();
    if download {
        all.extend(crate::bible::OFFERED.iter().filter(|bible| shelf.position(bible.id).is_none()).map(|bible| {
            Entry {
                id: bible.id.to_owned(),
                name: bible.name.to_owned(),
                language: code_of(bible.language),
                here: false,
                terms: crate::bible::terms_shown(bible.terms).to_owned(),
            }
        }));
    }
    all
}

/// The Bible recommended for a language: the first in it, one on this computer before one to
/// download. `None` for a language with no Bible.
pub fn recommended<'a>(all: &'a [Entry], language: &str) -> Option<&'a Entry> {
    all.iter().find(|bible| bible.language == language)
}

/// The recommended Bible of each of the church's languages, in their order, each once.
pub fn recommended_for<'a>(all: &'a [Entry], languages: &[String]) -> Vec<&'a Entry> {
    let mut out: Vec<&Entry> = Vec::new();
    for language in languages {
        if let Some(bible) = recommended(all, language)
            && !out.iter().any(|have| have.id == bible.id)
        {
            out.push(bible);
        }
    }
    out
}

/// The languages of the Bibles not on view, in the order More Bibles lists them: the church's
/// first, then the rest as the list has them.
pub fn more_languages(more: &[&Entry], languages: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for language in languages.iter().chain(more.iter().map(|bible| &bible.language)) {
        if !out.contains(language) && more.iter().any(|bible| &bible.language == language) {
            out.push(language.clone());
        }
    }
    out
}

/// The Bibles chosen on the step: by their short names, since one to download has no place on
/// the shelf yet.
#[derive(Clone, Debug, Default)]
pub struct Choice {
    pub top: Option<String>,
    pub below: Option<String>,
    /// More Bibles is open.
    pub more_open: bool,
    /// John 3:16 in the Bibles on this computer already read for the picture, by short name:
    /// its words and how that Bible names it.
    verses: HashMap<String, Option<(String, String)>>,
}

impl Choice {
    /// The choice the step starts with. Opened again: the Bible page's two rows as they stand.
    /// On a first start: the recommended Bible of the first language on top, the second's below.
    pub fn starting(state: &HomeState, languages: &[String], again: bool, download: bool) -> Choice {
        let mut choice = Choice::default();
        if again {
            let shelf = &state.bible.shelf;
            let id_at = |row: usize| {
                state
                    .bible
                    .rows
                    .get(row)
                    .and_then(|row| row.bible)
                    .and_then(|at| shelf.bibles.get(at))
                    .map(|b| b.id.clone())
            };
            choice.top = id_at(0);
            choice.below = id_at(1);
        } else {
            let all = every(&state.bible.shelf, download);
            choice.recommend(&all, languages);
        }
        choice
    }

    /// The recommended Bibles of the first two languages, on top and below; one language
    /// leaves one translation.
    fn recommend(&mut self, all: &[Entry], languages: &[String]) {
        self.top = languages.first().and_then(|language| recommended(all, language)).map(|bible| bible.id.clone());
        self.below = languages.get(1).and_then(|language| recommended(all, language)).map(|bible| bible.id.clone());
        if self.below == self.top {
            self.below = None;
        }
    }

    /// The Languages step was passed: a place whose Bible isn't in that place's language takes
    /// the language's recommended one (the first language's on top, the second's below). A
    /// Bible chosen in the right language stays.
    pub fn follow_languages(&mut self, state: &HomeState, languages: &[String]) {
        let download = crate::bible_store::download_base().is_some();
        let all = every(&state.bible.shelf, download);
        self.follow(&all, languages);
    }

    /// [`Choice::follow_languages`] among `all`.
    pub fn follow(&mut self, all: &[Entry], languages: &[String]) {
        let language_of = |id: &Option<String>| {
            id.as_ref().and_then(|id| all.iter().find(|bible| &bible.id == id)).map(|bible| bible.language.clone())
        };
        if let Some(first) = languages.first()
            && language_of(&self.top).as_ref() != Some(first)
            && let Some(bible) = recommended(all, first)
        {
            self.top = Some(bible.id.clone());
        }
        match languages.get(1) {
            Some(second) => {
                if language_of(&self.below).as_ref() != Some(second)
                    && let Some(bible) = recommended(all, second)
                {
                    self.below = Some(bible.id.clone());
                }
            }
            // One language shows one translation.
            None => self.below = None,
        }
        if self.below == self.top {
            self.below = None;
        }
    }

    /// On top was pressed on a Bible: the place moves to it; the two swap if it was below.
    pub fn put_on_top(&mut self, id: &str) {
        if self.top.as_deref() == Some(id) {
            return;
        }
        if self.below.as_deref() == Some(id) {
            self.below = self.top.take();
        }
        self.top = Some(id.to_owned());
    }

    /// Below was pressed on a Bible: the place moves to it (the two swap if it was on top);
    /// pressed on the one below, it leaves one translation.
    pub fn put_below(&mut self, id: &str) {
        if self.below.as_deref() == Some(id) {
            self.below = None;
        } else if self.top.as_deref() == Some(id) {
            if self.below.is_some() {
                std::mem::swap(&mut self.top, &mut self.below);
            }
        } else {
            self.below = Some(id.to_owned());
        }
    }

    /// One language: the one translation passages show.
    pub fn only(&mut self, id: &str) {
        self.top = Some(id.to_owned());
        self.below = None;
    }

    fn has(&self, id: &str) -> bool {
        self.top.as_deref() == Some(id) || self.below.as_deref() == Some(id)
    }
}

/// Gives a row of the Bible page a Bible on the shelf, adding the second row when it isn't
/// there, as the page's own menus do. Nothing changes when the row has it already.
fn set_row(bible: &mut BiblePage, row: usize, at: usize) {
    if bible.rows.get(row).and_then(|row| row.bible) == Some(at) {
        return;
    }
    if row >= bible.rows.len() && bible.add_row() {
        bible.assign_boxes();
    }
    if let Some(row) = bible.rows.get_mut(row) {
        row.bible = Some(at);
        bible.made_with.clear();
    }
}

/// The step was passed with Continue: the Bible page's rows take the choice. One on this
/// computer goes to its row now; one to download starts downloading (where Bibles can be) and
/// takes its row once it is here, the row keeping what it has until then. No Bible below
/// leaves one translation, once the one on top is there.
pub fn passed(setup: &mut Setup, state: &mut HomeState, env: &Env, ctx: &egui::Context) {
    let choice = setup.bibles.clone();
    let bible = &mut state.bible;
    let mut start = Vec::new();
    apply(&choice, bible, env.bibles_download, &mut |offered| start.push(offered));
    for offered in start {
        let ctx = ctx.clone();
        bible.downloads.start(offered.id, offered.checksum, move || ctx.request_repaint());
    }
}

/// Gives the Bible page the choice (see [`passed`]); `start` is told each Bible to download.
pub fn apply(
    choice: &Choice,
    bible: &mut BiblePage,
    download: bool,
    start: &mut dyn FnMut(&'static crate::bible::Offered),
) {
    bible.wanted.clear();
    bible.wanted_failed.clear();
    bible.wanted_alone = false;
    let mut top_waits = false;
    for (row, id) in [(0, choice.top.clone()), (1, choice.below.clone())] {
        match id {
            Some(id) => match bible.shelf.position(&id) {
                Some(at) => set_row(bible, row, at),
                None => {
                    if download && let Some(offered) = crate::bible::OFFERED.iter().find(|offered| offered.id == id) {
                        start(offered);
                        bible.wanted.push((id, row));
                        top_waits |= row == 0;
                    }
                }
            },
            // One translation. While the one on top is still to come the row below stays, so a
            // download that fails never leaves the Bible page with the wrong Bible alone.
            None if row == 1 && top_waits => bible.wanted_alone = true,
            None if row == 1 && choice.top.as_ref().is_some_and(|top| bible.shelf.position(top).is_some()) => {
                empty_below(bible);
            }
            None => {}
        }
    }
}

/// Empties the row below: one translation.
fn empty_below(bible: &mut BiblePage) {
    if let Some(second) = bible.rows.get_mut(1)
        && second.bible.is_some()
    {
        second.bible = None;
        bible.made_with.clear();
    }
}

/// A Bible's name as the Bibles offered have it, by its short name.
fn offered_name(id: &str) -> String {
    crate::bible::OFFERED.iter().find(|offered| offered.id == id).map_or_else(|| id.to_owned(), |o| o.name.to_owned())
}

/// A Bible the setup chose that has finished downloading takes its row (and, where one
/// translation was chosen, the row below is emptied then). One whose download ended without it
/// is waited for no longer and kept as failed: the rows stay as they were. The names of those
/// that failed just now, for a notice.
pub fn place_downloaded(bible: &mut BiblePage) -> Vec<String> {
    if bible.wanted.is_empty() {
        return Vec::new();
    }
    let mut failed = Vec::new();
    let wanted = std::mem::take(&mut bible.wanted);
    for (id, row) in wanted {
        match bible.shelf.position(&id) {
            Some(at) => {
                set_row(bible, row, at);
                if row == 0 && std::mem::take(&mut bible.wanted_alone) {
                    empty_below(bible);
                }
            }
            None if bible.downloads.is_busy(&id) => bible.wanted.push((id, row)),
            None => {
                failed.push(offered_name(&id));
                if row == 0 {
                    bible.wanted_alone = false;
                }
                bible.wanted_failed.push((id, row));
            }
        }
    }
    failed
}

/// What the notice says when a Bible the setup asked for didn't download.
pub fn bible_failed_words(name: &str) -> String {
    tf(
        "setup.bibles.failed",
        "{name} couldn\u{2019}t be downloaded, so the Bible page keeps the Bibles it had. Try again in Settings \u{203a} Set up SoftPresenter\u{2026} or in the Bibles window.",
        &[("name", name)],
    )
}

/// Try Again on Ready: the Bibles whose download failed are asked for again.
pub fn retry(state: &mut HomeState, ctx: &egui::Context) {
    let bible = &mut state.bible;
    for (id, row) in std::mem::take(&mut bible.wanted_failed) {
        if let Some(offered) = crate::bible::OFFERED.iter().find(|offered| offered.id == id) {
            let ctx = ctx.clone();
            bible.downloads.start(offered.id, offered.checksum, move || ctx.request_repaint());
            bible.wanted.push((id, row));
        }
    }
}

/// The names of the Bibles the setup asked for whose download failed.
pub fn failed_names(state: &HomeState) -> Vec<String> {
    state.bible.wanted_failed.iter().map(|(id, _)| offered_name(id)).collect()
}

/// What a row says under a Bible's name: its language, and whether it is on this computer or
/// downloads with Continue.
pub fn row_words(bible: &Entry) -> String {
    let language = language_own_name(&bible.language);
    if bible.here {
        tf("setup.bibles.here", "{language} \u{b7} On this computer", &[("language", &language)])
    } else {
        tf("setup.bibles.downloads", "{language} \u{b7} Downloads with Continue", &[("language", &language)])
    }
}

/// The languages of the Bibles the Bible page's first two rows show, top first, each once.
pub fn languages_standing(state: &HomeState) -> Vec<String> {
    let bible = &state.bible;
    let mut out: Vec<String> = Vec::new();
    for row in bible.rows.iter().take(2) {
        if let Some(shelved) = row.bible.and_then(|at| bible.shelf.bibles.get(at)) {
            let code = code_of(&shelved.language);
            if !out.contains(&code) {
                out.push(code);
            }
        }
    }
    out
}

/// The names of the Bibles the Bible page shows, top first, as Ready says them: its rows as
/// they stand, a row waiting for a download named by what it waits for.
pub fn names(state: &HomeState) -> Vec<String> {
    let bible = &state.bible;
    let mut out = Vec::new();
    for row in 0..2 {
        // One translation was chosen and is on its way: the row below goes once it is here.
        if row == 1 && bible.wanted_alone {
            continue;
        }
        let waiting = bible.wanted.iter().find(|(_, wanted)| *wanted == row).map(|(id, _)| offered_name(id));
        let stands = bible
            .rows
            .get(row)
            .and_then(|row| row.bible)
            .and_then(|at| bible.shelf.bibles.get(at))
            .map(|b| b.name.clone());
        if let Some(name) = waiting.or(stands) {
            out.push(name);
        }
    }
    out
}

/// John 3:16 in a Bible on this computer: its words, and how that Bible names it. Read once
/// for each Bible, then kept.
fn verse(choice: &mut Choice, shelf: &mut Shelf, id: &str) -> Option<(String, String)> {
    if let Some(known) = choice.verses.get(id) {
        return known.clone();
    }
    let read = shelf.position(id).and_then(|at| shelf.get(at).ok()).and_then(|bible| {
        let place = sp_bible::parse_reference("John 3:16").ok()?.start;
        Some((bible.verse(place)?.to_owned(), bible.label(place, place)))
    });
    choice.verses.insert(id.to_owned(), read.clone());
    read
}

/// The step.
pub fn step(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env) {
    let two = setup.languages.len() > 1;
    if two {
        super::head(
            ui,
            t("setup.bibles.title", "Your Bibles"),
            t(
                "setup.bibles.about",
                "Bible passages show two translations, one above the other. Choose which goes on top and which below.",
            ),
        );
    } else {
        super::head(
            ui,
            t("setup.bibles.title_one", "Your Bible"),
            t("setup.bibles.about_one", "Choose the translation passages show."),
        );
    }
    let all = every(&state.bible.shelf, env.bibles_download);
    let shown = recommended_for(&all, &setup.languages);
    let more: Vec<&Entry> = all.iter().filter(|bible| !shown.iter().any(|have| have.id == bible.id)).collect();
    // One chosen from More Bibles is on view: More opens by itself.
    if more.iter().any(|bible| setup.bibles.has(&bible.id)) {
        setup.bibles.more_open = true;
    }
    let side = if setup.again() { 240.0 } else { 280.0 };
    let gap = 24.0;
    let left = ui.available_width() - side - gap;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(vec2(left, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(left);
            super::cap(ui, t("setup.bibles.yours", "For your languages"));
            ui.add_space(4.0);
            for bible in &shown {
                row(ui, &mut setup.bibles, bible, two, true);
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            let title = tf(
                "setup.bibles.more_bibles",
                "More Bibles {n}",
                &[("n", &crate::i18n::count(more.len() as u64))],
            );
            if fold_key(ui, "setup more bibles", &title, setup.bibles.more_open).clicked() {
                setup.bibles.more_open = !setup.bibles.more_open;
            }
            if setup.bibles.more_open {
                let languages = more_languages(&more, &setup.languages);
                let mut others = false;
                for language in &languages {
                    ui.add_space(8.0);
                    if setup.languages.contains(language) {
                        super::cap(ui, &language_own_name(language));
                    } else if !others {
                        others = true;
                        super::cap(ui, t("bibles.window.other_languages", "Other languages"));
                    }
                    ui.add_space(4.0);
                    for bible in more.iter().filter(|bible| &bible.language == language) {
                        row(ui, &mut setup.bibles, bible, two, false);
                        ui.add_space(4.0);
                    }
                }
            }
            ui.add_space(8.0);
            let note = if env.bibles_download {
                t(
                    "setup.bibles.note",
                    "Each one to download is public domain or free to share; a row\u{2019}s terms show under the pointer.",
                )
            } else {
                t("setup.bibles.note_here", "More translations can be installed from a file, in the Bibles window.")
            };
            super::small(ui, note, 1);
        });
        ui.add_space(gap);
        ui.allocate_ui_with_layout(vec2(side, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(side);
            preview(ui, side, &mut setup.bibles, &mut state.bible.shelf, &all);
        });
    });
}

/// A fold's key: a triangle pointing at its title while closed, down while open, and the
/// title; it keeps its size either way.
pub(super) fn fold_key(ui: &mut egui::Ui, salt: &str, title: &str, open: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(title.to_owned(), FontId::proportional(13.0), TEXT_DIM);
    let (place, _) = ui.allocate_exact_size(vec2(14.0 + galley.size().x + 4.0, 26.0), Sense::hover());
    let response = ui.interact(place, ui.scope_id().with(salt), Sense::click());
    let ink = if response.hovered() { TEXT } else { TEXT_DIM };
    let middle = pos2(place.min.x + 4.0, place.center().y);
    let corners = if open {
        [middle + vec2(-4.0, -2.5), middle + vec2(4.0, -2.5), middle + vec2(0.0, 3.5)]
    } else {
        [middle + vec2(-2.5, -4.0), middle + vec2(3.5, 0.0), middle + vec2(-2.5, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(corners.to_vec(), ink, Stroke::NONE));
    ui.painter().galley_with_override_text_color(
        pos2(place.min.x + 14.0, place.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A Bible's row: its name, its language and where it is, Recommended in green on the
/// recommended ones, and its keys (On top and Below with two languages, a round mark for the
/// one translation with one).
fn row(ui: &mut egui::Ui, choice: &mut Choice, bible: &Entry, two: bool, recommended: bool) {
    let width = ui.available_width();
    let (place, _) = ui.allocate_exact_size(vec2(width, 46.0), Sense::hover());
    let chosen = choice.has(&bible.id);
    let id = ui.scope_id().with(("setup bible", bible.id.as_str()));
    // With one language the whole row is the choice.
    let whole = (!two).then(|| ui.interact(place, id.with("row"), Sense::click()));
    let hovered = whole.as_ref().is_some_and(egui::Response::hovered);
    ui.painter().rect_filled(place, 9.0, if hovered { HOVER } else { CARD });
    if chosen {
        ui.painter().rect_stroke(place, 9.0, Stroke::new(1.0, SELECTED), egui::StrokeKind::Inside);
    }
    let font = FontId::proportional(12.0);
    let labels = [t("setup.bibles.top", "On top"), t("setup.bibles.below", "Below")];
    let key = (crate::studio::flat_button_width(ui, &labels, &font, 10.0)).ceil();
    let keys = if two { key * 2.0 + 2.0 } else { 0.0 };
    let text_left = place.min.x + if two { 12.0 } else { 38.0 };
    let room = place.max.x - 12.0 - keys - 10.0 - text_left;
    if !two {
        let centre = pos2(place.min.x + 20.0, place.center().y);
        ui.painter().circle_stroke(centre, 7.5, Stroke::new(1.5, if chosen { TEXT } else { TEXT_FAINT }));
        if chosen {
            ui.painter().circle_filled(centre, 4.0, TEXT);
        }
    }
    let name = crate::home::settings::parts::one_line(ui, &bible.name, semibold(13.0), TEXT, room);
    ui.painter().galley(pos2(text_left, place.min.y + 7.0), name, TEXT);
    let words = row_words(bible);
    let under = ui.painter().layout_no_wrap(words, FontId::proportional(11.5), TEXT_DIM);
    let under_at = pos2(text_left, place.min.y + 26.0);
    let under_width = under.size().x;
    ui.painter().galley(under_at, under, TEXT_DIM);
    if recommended {
        ui.painter().text(
            under_at + vec2(under_width, 0.0),
            Align2::LEFT_TOP,
            format!(" \u{b7} {}", super::recommended_words()),
            FontId::proportional(11.5),
            super::OK,
        );
    }
    if two {
        let right = place.max.x - 12.0;
        let top = Rect::from_min_size(pos2(right - keys, place.center().y - 13.0), vec2(key, 26.0));
        let below = Rect::from_min_size(pos2(right - key, place.center().y - 13.0), vec2(key, 26.0));
        let on_top = choice.top.as_deref() == Some(bible.id.as_str());
        let is_below = choice.below.as_deref() == Some(bible.id.as_str());
        let look = |on: bool| if on { Flat::On } else { Flat::Plain };
        if crate::studio::flat_button(ui, top, id.with("top"), labels[0], font.clone(), look(on_top), true).clicked() {
            choice.put_on_top(&bible.id);
        }
        if crate::studio::flat_button(ui, below, id.with("below"), labels[1], font, look(is_below), true).clicked() {
            choice.put_below(&bible.id);
        }
    } else if whole.as_ref().is_some_and(egui::Response::clicked) {
        choice.only(&bible.id);
    }
    if !bible.terms.is_empty() {
        ui.interact(place, id.with("terms"), Sense::hover()).on_hover_text(&bible.terms);
    }
}

/// John 3:16 as passages will show, in the translations chosen that are on this computer; one
/// still to download shows once it is in, and the frame says so when none is here yet.
fn preview(ui: &mut egui::Ui, width: f32, choice: &mut Choice, shelf: &mut Shelf, all: &[Entry]) {
    let chosen: Vec<String> = [choice.top.clone(), choice.below.clone()].into_iter().flatten().collect();
    let name_of =
        |id: &str| all.iter().find(|bible| bible.id == id).map(|bible| bible.name.clone()).unwrap_or_default();
    let mut lines: Vec<(String, f32, egui::Color32)> = Vec::new();
    let mut later: Vec<String> = Vec::new();
    // One translation shows large; two share the slide.
    let one = chosen.len() == 1;
    for (at, id) in chosen.iter().enumerate() {
        match verse(choice, shelf, id) {
            Some((words, label)) => {
                let (size, ink) = match (one, at) {
                    (true, _) => (84.0, TEXT),
                    (false, 0) => (64.0, TEXT),
                    (false, _) => (52.0, TEXT_DIM),
                };
                lines.push((words, size, ink));
                lines.push((label, if one { 50.0 } else { 40.0 }, TEXT_FAINT));
            }
            None => later.push(name_of(id)),
        }
    }
    if lines.is_empty() {
        let place = super::slide_picture(ui, width, &[]);
        if let Some(first) = later.first() {
            ui.painter().text(
                place.center() - vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                first,
                FontId::proportional(13.0),
                TEXT,
            );
            ui.painter().text(
                place.center() + vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                t("setup.bibles.preview.waits", "John 3:16 shows here once it is downloaded"),
                FontId::proportional(11.5),
                TEXT_FAINT,
            );
        }
        ui.painter().rect_stroke(place, 6.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
        ui.add_space(8.0);
        super::small(ui, "", 2);
        return;
    }
    super::slide_picture(ui, width, &lines);
    ui.add_space(8.0);
    let caption = if later.is_empty() {
        t("setup.bibles.preview", "John 3:16, as passages will show.").to_owned()
    } else {
        tf(
            "setup.bibles.preview.later",
            "John 3:16, as passages will show ({names} once downloaded).",
            &[("names", &later.join(", "))],
        )
    };
    super::small(ui, &caption, 2);
}
