//! The song catalog's songbooks in the windows: the setup's Songs step, and the page
//! Settings › Integrations › Worship Leader App (drawn in `integrations`). Both are offered only
//! where [`crate::catalog::offered`] says, in development builds for now.
//!
//! A window only shows and asks ([`Ask`]); the app (`catalog_cli`) downloads, removes and asks
//! the source how big a songbook is, each on a thread of its own, and keeps [`Known`] up to
//! date. The songbooks recommended (ticked to start with in the setup, listed first in Settings)
//! are the operator's language and the languages the church shows ([`crate::catalog::recommended`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use egui::{Align2, Color32, Sense, Stroke, vec2};

use crate::catalog::{self, Songbook};
use crate::i18n::{t, tf, tn};
use crate::studio::{CARD, CARE, HOVER, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// What the windows know of the songbooks, kept up to date by the app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Known {
    /// The songbooks on this computer, with their songs.
    pub installed: BTreeMap<String, usize>,
    /// Those being downloaded, with how far each has got when the source said how big it is.
    pub busy: BTreeMap<String, Option<u8>>,
    /// How big each download is, as the source said (`None`: it didn't say).
    pub sizes: HashMap<String, Option<u64>>,
    /// Asked about, and waiting for the source's answer.
    pub looking: BTreeSet<String>,
    /// The source couldn't be reached the last time it was asked.
    pub offline: bool,
}

impl Known {
    /// Takes in the source's answer about how big some songbooks are. The source counts as out
    /// of reach only while it has answered nothing at all, so one language it doesn't have (or
    /// one request lost) never unticks the rest.
    pub fn take_sizes(&mut self, sizes: Vec<catalog::Size>) {
        let mut reached = false;
        for size in sizes {
            self.looking.remove(&size.lang);
            if size.problem.is_none() {
                reached = true;
                self.sizes.insert(size.lang, size.download_bytes);
            }
        }
        if reached {
            self.offline = false;
        } else if self.sizes.is_empty() {
            self.offline = true;
        }
    }
}

/// What a window asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Download these songbooks.
    Download(Vec<String>),
    /// Take this songbook off the computer (songs already in the library stay).
    Remove(String),
    /// Ask the source how big these are (a request that downloads nothing).
    Look(Vec<String>),
}

/// The songbooks in the windows: what is known, what was asked, and the Settings page's state.
#[derive(Debug, Default)]
pub struct View {
    pub known: Known,
    pub asks: Vec<Ask>,
    /// Settings › Integrations shows the page of Worship Leader App instead of its list.
    pub open: bool,
    /// The page lists every language, not only those on the computer and recommended.
    others: bool,
    /// The songbook whose Remove is asking first.
    removing: Option<String>,
    /// Languages asked about already, so each is asked once (Try Again asks again).
    asked: HashSet<String>,
}

impl View {
    /// Asks the source how big each of these is, once each; the ones on this computer are
    /// known already.
    fn ask_sizes<'a>(&mut self, codes: impl IntoIterator<Item = &'a str>) {
        let new: Vec<String> = codes
            .into_iter()
            .filter(|code| !self.known.installed.contains_key(*code) && !self.known.busy.contains_key(*code))
            .filter(|code| self.asked.insert((*code).to_owned()))
            .map(str::to_owned)
            .collect();
        if !new.is_empty() {
            self.asks.push(Ask::Look(new));
        }
    }

    /// Try Again after the source couldn't be reached: everything shown is asked about again.
    fn try_again(&mut self) {
        self.asked.clear();
        self.known.offline = false;
    }

    /// Whether anything is under way, or just asked for, that the window must be drawn again for
    /// (an idle window is drawn only when something asks).
    fn waiting(&self) -> bool {
        !self.asks.is_empty() || !self.known.busy.is_empty() || !self.known.looking.is_empty()
    }

    /// What a songbook's row says under its name: how far its download has got, its songs once
    /// on this computer, or how big it is to download.
    pub fn detail(&self, code: &str) -> String {
        if let Some(per_cent) = self.known.busy.get(code) {
            return match per_cent {
                Some(per) => tf(
                    "integrations.songbooks.downloading_per",
                    "Downloading \u{b7} {per}%",
                    &[("per", &per.to_string())],
                ),
                None => t("integrations.songbooks.downloading", "Downloading\u{2026}").to_owned(),
            };
        }
        if let Some(songs) = self.known.installed.get(code) {
            return tf(
                "integrations.songbooks.on_computer",
                "On this computer \u{b7} {songs}",
                &[("songs", &songs_words(*songs))],
            );
        }
        match self.known.sizes.get(code) {
            Some(Some(bytes)) => tf(
                "integrations.songbooks.to_download",
                "{size} MB to download",
                &[("size", &crate::i18n::number((*bytes as f64 / 1_000_000.0).max(0.1), 1))],
            ),
            _ => String::new(),
        }
    }
}

/// "13,613 songs", as the language writes it.
pub fn songs_words(songs: usize) -> String {
    tn("integrations.songbooks.songs", songs as u64, "{n} song", "{n} songs", &[])
}

/// The setup's choice of the catalog's songbooks on its Songs step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picked {
    /// The songbooks ticked, recommended ones first.
    pub ticked: Vec<String>,
    /// Ticked or unticked by hand: the recommendation no longer changes the ticks.
    touched: bool,
    /// The Songs step was left with Continue, so what is ticked downloads. Leaving it any
    /// other way (Skip, Back, Escape, Skip Setup) downloads nothing.
    pub confirmed: bool,
}

impl Picked {
    /// Ticks the recommended songbooks not on this computer yet, until one is ticked by hand.
    fn recommend(&mut self, recommended: &[&str], known: &Known) {
        if !self.touched {
            self.ticked = recommended
                .iter()
                .filter(|code| !known.installed.contains_key(**code) && !known.busy.contains_key(**code))
                .map(|code| (*code).to_owned())
                .collect();
        }
    }

    fn toggle(&mut self, code: &str) {
        self.touched = true;
        match self.ticked.iter().position(|have| have == code) {
            Some(at) => {
                self.ticked.remove(at);
            }
            None => self.ticked.push(code.to_owned()),
        }
    }

    /// What is to download when the setup closes: nothing unless the step was left with Continue,
    /// nothing while the source can't be reached (the step shows nothing ticked then; Settings
    /// downloads later), and nothing on this computer already.
    pub fn to_download(&self, known: &Known) -> Vec<String> {
        if !self.confirmed || known.offline {
            return Vec::new();
        }
        self.ticked
            .iter()
            .filter(|code| !known.installed.contains_key(*code) && !known.busy.contains_key(*code))
            .cloned()
            .collect()
    }
}

/// The languages the church shows, as the setup's Languages step was answered and the
/// preferences keep them, the one on top first. Kept here for the places that recommend
/// songbooks and Bibles outside the setup (`set_church_languages` when the preferences are read
/// and when the step is passed); the rules themselves take the languages as they are given.
#[cfg(not(test))]
static CHURCH: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

// In the tests each thread has its own, so one test's church never recommends in another's.
#[cfg(test)]
thread_local! {
    static CHURCH: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The languages the church shows, the one on top first. None until the setup's Languages step
/// is answered: only the operator's language is recommended then, as before the step was there.
#[cfg(not(test))]
pub fn church_languages() -> Vec<String> {
    CHURCH.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

/// The church's languages were read from the preferences, or answered in the setup.
#[cfg(not(test))]
pub fn set_church_languages(languages: &[String]) {
    *CHURCH.write().unwrap_or_else(std::sync::PoisonError::into_inner) = languages.to_vec();
}

#[cfg(test)]
pub fn church_languages() -> Vec<String> {
    CHURCH.with(|church| church.borrow().clone())
}

#[cfg(test)]
pub fn set_church_languages(languages: &[String]) {
    CHURCH.with(|church| *church.borrow_mut() = languages.to_vec());
}

/// The songbooks recommended now: the operator's language (SoftPresenter's own, as chosen in
/// the setup's first step or Settings) and the church's.
pub fn recommended_now() -> Vec<&'static str> {
    catalog::recommended(crate::i18n::current().code(), &church_languages())
}

/// The songbooks the setup recommends for a church that shows `church`: one for each of its
/// languages the source publishes, the one on top first, and nothing else (the app's own
/// language says what the operator reads, not what the church sings).
pub fn recommended_for(church: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for book in church.iter().filter_map(|code| catalog::songbook(code)) {
        if !out.contains(&book.code) {
            out.push(book.code);
        }
    }
    out
}

/// The song catalog's rows on the setup's Songs step: the songbooks of the church's languages
/// (`church`), ticked to start with and marked Recommended, then the setup's other languages
/// (`offered`), and any on this computer or ticked already. While the source can't be reached,
/// its line and Try Again stand above them.
pub fn setup_rows(ui: &mut egui::Ui, picked: &mut Picked, view: &mut View, church: &[String], offered: &[&str]) {
    let recommended = recommended_for(church);
    picked.recommend(&recommended, &view.known);
    status(ui, view);
    let mut shown: Vec<&'static Songbook> = Vec::new();
    let wanted = recommended
        .iter()
        .copied()
        .chain(offered.iter().copied())
        .chain(view.known.installed.keys().map(String::as_str))
        .chain(view.known.busy.keys().map(String::as_str))
        .chain(picked.ticked.iter().map(String::as_str));
    for code in wanted {
        if let Some(book) = catalog::songbook(code)
            && !shown.contains(&book)
        {
            shown.push(book);
        }
    }
    view.ask_sizes(shown.iter().map(|book| book.code));
    for book in &shown {
        let here = view.known.installed.contains_key(book.code) || view.known.busy.contains_key(book.code);
        let ticked = here || (!view.known.offline && picked.ticked.iter().any(|code| code == book.code));
        let can = !here && !view.known.offline;
        let mut detail = view.detail(book.code);
        if recommended.contains(&book.code) {
            detail = crate::setup::recommended_detail(&detail);
        }
        if tick_card(ui, book.name(), &detail, ticked, here, can).clicked() {
            picked.toggle(book.code);
        }
        ui.add_space(6.0);
    }
    if view.waiting() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// The setup's Songs step was passed: what was ticked and confirmed starts downloading.
pub fn setup_closed(picked: &Picked, view: &mut View) {
    let langs = picked.to_download(&view.known);
    if !langs.is_empty() {
        view.asks.push(Ask::Download(langs));
    }
}

/// While the source is being asked, a short line; when it couldn't be reached, why, and Try
/// Again.
pub fn status(ui: &mut egui::Ui, view: &mut View) {
    if view.known.offline {
        ui.add(
            egui::Label::new(
                egui::RichText::new(t(
                    "integrations.songbooks.offline",
                    "Worship Leader App can\u{2019}t be reached: this computer may not be connected to the internet.",
                ))
                .size(12.5)
                .color(CARE),
            )
            .wrap(),
        );
        ui.add_space(4.0);
        if ui.button(t("integrations.songbooks.try_again", "Try Again")).clicked() {
            view.try_again();
        }
        ui.add_space(8.0);
    } else if !view.known.looking.is_empty() {
        ui.label(
            egui::RichText::new(t(
                "integrations.songbooks.looking",
                "Looking for the songbooks at Worship Leader App\u{2026}",
            ))
            .size(12.0)
            .color(TEXT_FAINT),
        );
        ui.add_space(6.0);
    }
}

/// A card in a setup list of songbooks (the catalog's, and the free ones shared by churches,
/// `free_songbooks`): a tick box, the name and what it is. `here` (on this computer, or on its
/// way) shows a tick that can't be taken away; `can` says a click ticks or unticks it.
pub(crate) fn tick_card(
    ui: &mut egui::Ui,
    name: &str,
    detail: &str,
    ticked: bool,
    here: bool,
    can: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (place, response) =
        ui.allocate_exact_size(vec2(width, 40.0), if can { Sense::click() } else { Sense::hover() });
    let fill = if ticked && !here {
        SELECTED
    } else if can && response.hovered() {
        HOVER
    } else {
        // A shade darker than the card the list sits on.
        crate::studio::PANEL
    };
    ui.painter().rect_filled(place, 9.0, fill);
    let tick = egui::Rect::from_center_size(place.left_center() + vec2(20.0, 0.0), vec2(16.0, 16.0));
    paint_tick_box(ui.painter(), tick, ticked, here, can);
    let (name_at, detail_at) = if detail.is_empty() { (0.0, 0.0) } else { (-7.0, 9.0) };
    // In one line each, cut short with "…" where the card is narrower than the words.
    let room = width - 40.0 - 12.0;
    let one_line = crate::home::settings::parts::one_line;
    let name_ink = if can || here { TEXT } else { TEXT_FAINT };
    let name = one_line(ui, name, semibold(13.0), name_ink, room);
    let at = place.left_center() + vec2(40.0, name_at - name.size().y / 2.0);
    ui.painter().galley(at, name, name_ink);
    if !detail.is_empty() {
        let detail = one_line(ui, detail, egui::FontId::proportional(11.0), TEXT_DIM, room);
        let at = place.left_center() + vec2(40.0, detail_at - detail.size().y / 2.0);
        ui.painter().galley(at, detail, TEXT_DIM);
    }
    response
}

/// A tick box, 16 square in `tick`: ticked, a tick on green (on grey when `fixed`, a tick that
/// can't be taken away); not, an outline, fainter when it can't be ticked (`can`).
pub(crate) fn paint_tick_box(painter: &egui::Painter, tick: egui::Rect, ticked: bool, fixed: bool, can: bool) {
    if ticked {
        painter.rect_filled(tick, 4.0, if fixed { TEXT_FAINT } else { READY });
        painter.text(tick.center(), Align2::CENTER_CENTER, "\u{2713}", semibold(12.0), Color32::BLACK);
    } else {
        let ink = if can { TEXT_DIM } else { TEXT_FAINT };
        painter.rect_stroke(tick, 4.0, Stroke::new(1.2, ink), egui::StrokeKind::Inside);
    }
}

/// A songbook on the Settings page: a card with its name and what it is, and a button (Download
/// or Remove) when one applies. True when the button was clicked.
fn button_row(ui: &mut egui::Ui, book: &Songbook, detail: &str, button: Option<&str>) -> bool {
    button_card(ui, book.name(), detail, button)
}

/// A card in a list of songbooks to download (the catalog's on its Settings page, and the
/// Free Songbooks window, `free_songbooks`): the name and what it is, and a button when one
/// applies. True when the button was clicked.
pub(crate) fn button_card(ui: &mut egui::Ui, name: &str, detail: &str, button: Option<&str>) -> bool {
    let width = ui.available_width().min(560.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
    ui.painter().rect_filled(place, 9.0, CARD);
    let (name_at, detail_at) = if detail.is_empty() { (0.0, 0.0) } else { (-8.0, 9.0) };
    ui.painter().text(place.left_center() + vec2(14.0, name_at), Align2::LEFT_CENTER, name, semibold(13.0), TEXT);
    if !detail.is_empty() {
        ui.painter().text(
            place.left_center() + vec2(14.0, detail_at),
            Align2::LEFT_CENTER,
            detail,
            egui::FontId::proportional(11.0),
            TEXT_DIM,
        );
    }
    let Some(label) = button else { return false };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let wide = ui.painter().layout_no_wrap(label.to_owned(), font, TEXT).size().x + 20.0;
    let at = egui::Rect::from_center_size(place.right_center() - vec2(12.0 + wide / 2.0, 0.0), vec2(wide, 24.0));
    ui.put(at, egui::Button::new(label)).clicked()
}

/// The page Settings › Integrations › Worship Leader App, under its heading: the songbooks on
/// this computer, each with Remove (asking first), then the recommended ones and, on request,
/// every other language, each with Download.
pub fn settings_page(ui: &mut egui::Ui, view: &mut View) {
    ui.label(
        egui::RichText::new(t(
            "integrations.songbooks.about",
            "Free songs from Worship Leader App, a songbook for each language. They are kept on this computer, apart from your own songs, and join your library only when you use one. Removing a songbook leaves the songs already in your library.",
        ))
        .color(TEXT_DIM),
    );
    ui.add_space(6.0);
    if ui.button(format!("{} \u{2197}", catalog::SOURCE_SITE.trim_start_matches("https://"))).clicked()
        && let Err(problem) = crate::canva::open_page(catalog::SOURCE_SITE)
    {
        crate::applog::problem("catalog", &format!("Could not open the web page: {problem}"));
    }
    ui.add_space(12.0);
    status(ui, view);
    let mut asks = Vec::new();
    let here: Vec<&'static Songbook> = catalog::in_name_order()
        .into_iter()
        .filter(|book| view.known.installed.contains_key(book.code) || view.known.busy.contains_key(book.code))
        .collect();
    if !here.is_empty() {
        crate::studio::caption(ui, &t("integrations.songbooks.here", "On This Computer").to_uppercase());
        for book in &here {
            let busy = view.known.busy.contains_key(book.code);
            let button = (!busy).then(|| t("integrations.songbooks.remove", "Remove"));
            if button_row(ui, book, &view.detail(book.code), button) {
                view.removing = Some(book.code.to_owned());
            }
            ui.add_space(6.0);
        }
        ui.add_space(8.0);
    }
    let recommended: Vec<&'static Songbook> =
        recommended_now().into_iter().filter_map(catalog::songbook).filter(|book| !here.contains(book)).collect();
    view.ask_sizes(recommended.iter().map(|book| book.code));
    let download = t("integrations.songbooks.download", "Download");
    let offer = if view.known.offline { None } else { Some(download) };
    if !recommended.is_empty() {
        crate::studio::caption(ui, &t("integrations.songbooks.recommended", "Recommended").to_uppercase());
        for book in &recommended {
            if button_row(ui, book, &view.detail(book.code), offer) {
                asks.push(Ask::Download(vec![book.code.to_owned()]));
            }
            ui.add_space(6.0);
        }
        ui.add_space(8.0);
    }
    if view.others {
        crate::studio::caption(ui, &t("integrations.songbooks.other_languages", "Other Languages").to_uppercase());
        for book in
            catalog::in_name_order().into_iter().filter(|book| !here.contains(book) && !recommended.contains(book))
        {
            if button_row(ui, book, &view.detail(book.code), offer) {
                asks.push(Ask::Download(vec![book.code.to_owned()]));
            }
            ui.add_space(6.0);
        }
    } else if ui.button(t("setup.free_songs.others", "Other languages\u{2026}")).clicked() {
        view.others = true;
    }
    view.asks.extend(asks);
    if view.waiting() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    // Remove asks first.
    if let Some(code) = view.removing.clone() {
        let name = catalog::songbook(&code).map_or(code.as_str(), |book| book.name());
        let title = tf("integrations.songbooks.remove_title", "Remove the {name} songbook?", &[("name", name)]);
        let note = t(
            "integrations.songbooks.remove_note",
            "Its songs leave this computer. Songs already in your library stay, and the songbook can be downloaded again.",
        );
        let icon = crate::icons::Icon::Close;
        let remove = t("integrations.songbooks.remove", "Remove");
        if let Some(answer) = crate::studio::alert(ui.ctx(), "remove songbook", icon, &title, note, remove) {
            view.removing = None;
            if answer {
                view.asks.push(Ask::Remove(code));
            }
        }
    }
}

/// How the songbooks stand, for the Integrations list: downloading, how many are on this
/// computer, or none.
pub fn state_words(view: &View) -> (bool, String) {
    if !view.known.busy.is_empty() {
        (false, t("integrations.songbooks.downloading", "Downloading\u{2026}").to_owned())
    } else if view.known.installed.is_empty() {
        (false, t("integrations.songbooks.none", "Not downloaded").to_owned())
    } else {
        let n = view.known.installed.len() as u64;
        (true, tn("integrations.songbooks.count", n, "{n} songbook", "{n} songbooks", &[]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    fn known(installed: &[(&str, usize)]) -> Known {
        Known { installed: installed.iter().map(|(code, n)| ((*code).to_owned(), *n)).collect(), ..Known::default() }
    }

    #[test]
    fn the_operators_language_is_ticked_and_nothing_on_the_computer_is() {
        // A Spanish speaker: Spanish is ticked.
        let recommended = with_language(Language::Spanish, recommended_now);
        assert_eq!(recommended, ["es"]);
        let mut picked = Picked::default();
        picked.recommend(&recommended, &Known::default());
        assert_eq!(picked.ticked, ["es"]);
        // A Russian and English church: both, the operator's first; one already downloaded isn't
        // ticked again.
        let church = ["ru".to_owned(), "en".to_owned()];
        let recommended = catalog::recommended("ru", &church);
        let mut picked = Picked::default();
        picked.recommend(&recommended, &known(&[("en", 3146)]));
        assert_eq!(picked.ticked, ["ru"]);
        picked.recommend(&recommended, &Known::default());
        assert_eq!(picked.ticked, ["ru", "en"]);
        // Once ticked by hand, a change of language doesn't change the ticks.
        picked.toggle("en");
        picked.recommend(&["uk"], &Known::default());
        assert_eq!(picked.ticked, ["ru"]);
    }

    #[test]
    fn only_continue_or_not_now_downloads_and_only_what_isnt_here() {
        let mut picked = Picked { ticked: vec!["ru".to_owned(), "en".to_owned()], ..Picked::default() };
        let mut view = View::default();
        // Closed with Escape or Skip Setup: nothing.
        setup_closed(&picked, &mut view);
        assert!(view.asks.is_empty());
        // Continued: what is ticked and not on the computer.
        picked.confirmed = true;
        view.known = known(&[("en", 10)]);
        setup_closed(&picked, &mut view);
        assert_eq!(view.asks, [Ask::Download(vec!["ru".to_owned()])]);
        // With the source out of reach, nothing (the step shows nothing ticked then).
        let mut offline = View { known: Known { offline: true, ..Known::default() }, ..View::default() };
        setup_closed(&picked, &mut offline);
        assert!(offline.asks.is_empty());
        // Not Now: nothing ticked, nothing asked.
        let not_now = Picked { confirmed: true, touched: true, ..Picked::default() };
        let mut view = View::default();
        setup_closed(&not_now, &mut view);
        assert!(view.asks.is_empty());
    }

    #[test]
    fn each_songbook_is_asked_about_once_until_try_again() {
        let mut view = View { known: known(&[("en", 10)]), ..View::default() };
        view.ask_sizes(["es", "en", "ru"]);
        view.ask_sizes(["es", "ru"]);
        assert_eq!(view.asks, [Ask::Look(vec!["es".to_owned(), "ru".to_owned()])]);
        view.known.offline = true;
        view.try_again();
        view.ask_sizes(["es"]);
        assert_eq!(view.asks.len(), 2);
        assert!(!view.known.offline);
    }

    fn size(lang: &str, bytes: Option<u64>, problem: Option<&str>) -> catalog::Size {
        catalog::Size {
            lang: lang.to_owned(),
            songs: None,
            download_bytes: bytes,
            disk_bytes: 0,
            installed: false,
            problem: problem.map(str::to_owned),
        }
    }

    #[test]
    fn the_source_is_out_of_reach_only_while_it_has_answered_nothing() {
        let mut known = Known { looking: ["es".to_owned(), "ru".to_owned()].into(), ..Known::default() };
        // Nothing answered: out of reach.
        known.take_sizes(vec![size("es", None, Some("Could not reach the song source"))]);
        assert!(known.offline && known.looking.contains("ru") && !known.looking.contains("es"));
        // An answer: in reach again.
        known.take_sizes(vec![size("ru", Some(15_663_550), None)]);
        assert!(!known.offline && known.looking.is_empty());
        assert_eq!(known.sizes.get("ru"), Some(&Some(15_663_550)));
        // One language more that it can't answer for doesn't untick the rest.
        known.take_sizes(vec![size("de", None, Some("There are no de songs at the source."))]);
        assert!(!known.offline);
    }

    #[test]
    fn a_window_that_has_just_asked_is_drawn_again() {
        let mut view = View::default();
        assert!(!view.waiting());
        view.ask_sizes(["es"]);
        assert!(view.waiting(), "the ask goes out after the frame: the next frame must come");
    }

    #[test]
    fn a_row_says_how_far_its_download_is_or_its_songs_or_its_size() {
        with_language(Language::English, || {
            let mut view = View { known: known(&[("ru", 13613)]), ..View::default() };
            view.known.busy.insert("uk".to_owned(), Some(42));
            view.known.busy.insert("es".to_owned(), None);
            view.known.sizes.insert("en".to_owned(), Some(4_774_974));
            view.known.sizes.insert("de".to_owned(), None);
            assert_eq!(view.detail("ru"), "On this computer \u{b7} 13,613 songs");
            assert_eq!(view.detail("uk"), "Downloading \u{b7} 42%");
            assert_eq!(view.detail("es"), "Downloading\u{2026}");
            assert_eq!(view.detail("en"), "4.8 MB to download");
            assert_eq!(view.detail("de"), "");
            assert_eq!(state_words(&view), (false, "Downloading\u{2026}".to_owned()));
            view.known.busy.clear();
            assert_eq!(state_words(&view), (true, "1 songbook".to_owned()));
        });
    }
}
