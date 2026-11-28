//! The setup's Languages step: which languages the church shows, and in what order. Its
//! answer chooses which songbooks and Bibles the next two steps recommend, and is kept
//! (`HomeState::church_languages`) once the step is passed with Continue, so the Free Songbooks
//! and Bibles windows list those languages first afterwards.

use egui::{Color32, FontId, Sense, Stroke, pos2, vec2};

use super::{LANGUAGES, Setup, language_own_name};
use crate::home::HomeState;
use crate::i18n::{t, tf, tn};
use crate::studio::{CARD, HOVER, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// The languages the step starts with: the church's as they were answered before (`kept`);
/// else, opened again, those of the Bibles the Bible page shows (`standing`, top first); else
/// those of the computer's preferred languages (`computer`, as the system writes them); each
/// only where the step offers it, and English when none is.
pub fn starting(kept: &[String], standing: &[String], computer: &[String]) -> Vec<String> {
    let offered = |code: &str| LANGUAGES.contains(&code);
    let mut out: Vec<String> = Vec::new();
    let from: Vec<String> = if !kept.is_empty() {
        kept.to_vec()
    } else if standing.iter().any(|code| offered(code)) {
        standing.to_vec()
    } else {
        computer
            .iter()
            .map(|tag| tag.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase())
            .collect()
    };
    for code in from {
        if offered(&code) && !out.contains(&code) {
            out.push(code);
        }
    }
    if out.is_empty() {
        out.push("en".to_owned());
    }
    out
}

/// The step was passed (Continue, or a step further on clicked): the church's languages are
/// kept, the recommendations everywhere follow them, and the Bibles step starts from the
/// recommended Bible of each. Opened again and passed with the languages it opened with,
/// nothing is kept anew and the Bibles chosen stay as they stand.
pub fn passed(setup: &mut Setup, state: &mut HomeState) {
    if setup.again() && setup.languages == setup.languages_opened {
        return;
    }
    if state.church_languages != setup.languages {
        state.church_languages.clone_from(&setup.languages);
        crate::songbooks::set_church_languages(&state.church_languages);
    }
    setup.bibles.follow_languages(state, &setup.languages);
    setup.languages_opened.clone_from(&setup.languages);
}

/// How many free songs there are to start with in a language: its free songbooks' songs.
pub fn free_songs(code: &str) -> usize {
    crate::free_songbooks::of_language(code).map(|book| book.songs).sum()
}

/// The line under a language's name on its card: its name in English where the window is in
/// English and the name is written otherwise, and its free songs when it has any.
pub fn card_line(code: &str) -> String {
    let own = language_own_name(code);
    let english = crate::free_songbooks::language_english(code);
    let mut parts: Vec<String> = Vec::new();
    if crate::i18n::current() == crate::i18n::Language::English && !english.is_empty() && english != own {
        parts.push(english.to_owned());
    }
    let songs = free_songs(code);
    if songs > 0 {
        parts.push(tn("setup.languages.free_songs", songs as u64, "{n} free song", "{n} free songs", &[]));
    }
    parts.join(" \u{b7} ")
}

/// What is said under the slide: with none chosen, to choose one; with one, that a second can
/// be; with two, what the picture is; with more, that the third's songs and Bibles are there.
pub fn caption(languages: &[String]) -> String {
    match languages {
        [] => t("setup.languages.choose", "Choose at least one.").to_owned(),
        [_] => t("setup.languages.one", "One language on screen. Choose a second for songs and passages in two.")
            .to_owned(),
        [_, _] => t("setup.languages.two", "How a song will look.").to_owned(),
        [_, _, third, ..] => tf(
            "setup.languages.more",
            "{language} songs and Bibles are there too, for when you want them.",
            &[("language", &language_own_name(third))],
        ),
    }
}

/// Two lines written for the picture in each language the step offers (not a song's words:
/// they only show which language is on top and which below).
pub fn sample_lines(code: &str) -> [&'static str; 2] {
    match code {
        "es" => ["La letra de un canto", "como la ver\u{e1} la iglesia"],
        "ru" => ["Слова песни", "как их увидит церковь"],
        "uk" => ["Слова пісні", "як їх побачить церква"],
        _ => ["The words of a song", "as the church will see them"],
    }
}

/// The slide beside the cards and on Ready: the first language's lines large, the second's
/// under them, smaller and dimmer, as a song in two languages shows.
pub fn preview(ui: &mut egui::Ui, width: f32, languages: &[String]) -> egui::Rect {
    let mut lines: Vec<(String, f32, Color32)> = Vec::new();
    if let Some(top) = languages.first() {
        lines.push((sample_lines(top).join("\n"), 112.0, TEXT));
    }
    if let Some(below) = languages.get(1) {
        lines.push((sample_lines(below).join("\n"), 78.0, TEXT_DIM));
    }
    super::slide_picture(ui, width, &lines)
}

/// The step: four cards, two by two, and beside them a slide in the languages chosen with On
/// top and Below under it, always there (greyed with one language) so nothing moves.
pub fn step(ui: &mut egui::Ui, setup: &mut Setup, again: bool) {
    super::head(
        ui,
        t("setup.languages.title", "Which languages does your church show?"),
        t("setup.languages.about", "Songs and Bible passages can show two, one above the other."),
    );
    let side = if again { 260.0 } else { 300.0 };
    let gap = 24.0;
    let left = ui.available_width() - side - gap;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(vec2(left, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(left);
            let card = vec2(((left - 10.0) / 2.0).floor(), 92.0);
            for pair in LANGUAGES.chunks(2) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for code in pair {
                        if language_card(ui, card, code, setup.languages.iter().any(|have| have == code)).clicked() {
                            match setup.languages.iter().position(|have| have == code) {
                                Some(at) => {
                                    setup.languages.remove(at);
                                }
                                None => setup.languages.push((*code).to_owned()),
                            }
                        }
                    }
                });
                ui.add_space(10.0);
            }
        });
        ui.add_space(gap);
        ui.allocate_ui_with_layout(vec2(side, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(side);
            preview(ui, side, &setup.languages);
            ui.add_space(12.0);
            order_menus(ui, side, &mut setup.languages);
            ui.add_space(10.0);
            super::small(ui, &caption(&setup.languages), 2);
        });
    });
}

/// A language's card: its name in itself, one line under it, and a tick at its corner; chosen,
/// it carries the ink ring.
fn language_card(ui: &mut egui::Ui, size: egui::Vec2, code: &str, chosen: bool) -> egui::Response {
    let (place, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect_filled(place, 9.0, if response.hovered() { HOVER } else { CARD });
    if chosen {
        ui.painter().rect_stroke(place, 9.0, Stroke::new(2.0, TEXT), egui::StrokeKind::Inside);
    }
    let name = language_own_name(code);
    ui.painter().text(place.min + vec2(16.0, 16.0), egui::Align2::LEFT_TOP, name, semibold(21.0), TEXT);
    ui.painter().text(
        place.min + vec2(16.0, 50.0),
        egui::Align2::LEFT_TOP,
        card_line(code),
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    let centre = pos2(place.max.x - 24.0, place.min.y + 24.0);
    if chosen {
        ui.painter().circle_filled(centre, 10.0, TEXT);
        super::tick(ui.painter(), centre, 4.0, Stroke::new(1.6, crate::studio::PANEL));
    } else {
        ui.painter().circle_stroke(centre, 9.25, Stroke::new(1.5, TEXT_FAINT));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// On top and Below: each a menu of the languages chosen. Choosing one for a place swaps it
/// with what the place had. With one language Below is greyed and says so; with none both are.
fn order_menus(ui: &mut egui::Ui, width: f32, languages: &mut [String]) {
    let labels = [t("setup.bibles.top", "On top"), t("setup.bibles.below", "Below")];
    let label_width = labels
        .iter()
        .map(|label| ui.painter().layout_no_wrap((*label).to_owned(), FontId::proportional(12.5), TEXT).size().x)
        .fold(0.0, f32::max)
        .ceil();
    let menu = width - label_width - 12.0 - 8.0;
    let mut swap = None;
    for (place, label) in labels.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (at, _) = ui.allocate_exact_size(vec2(label_width, 26.0), Sense::hover());
            ui.painter().text(
                at.left_center(),
                egui::Align2::LEFT_CENTER,
                *label,
                FontId::proportional(12.5),
                TEXT_DIM,
            );
            let shown = match languages.get(place) {
                Some(code) => language_own_name(code),
                None if place == 1 && languages.len() == 1 => {
                    t("setup.languages.none_below", "None (one language)").to_owned()
                }
                None => "\u{2014}".to_owned(),
            };
            ui.add_enabled_ui(languages.len() > place && languages.len() > 1, |ui| {
                egui::ComboBox::from_id_salt(("setup language order", place)).width(menu).selected_text(shown).show_ui(
                    ui,
                    |ui| {
                        for (index, code) in languages.iter().enumerate() {
                            if ui.selectable_label(index == place, language_own_name(code)).clicked() && index != place
                            {
                                swap = Some((place, index));
                            }
                        }
                    },
                );
            });
        });
        ui.add_space(8.0);
    }
    if let Some((a, b)) = swap {
        languages.swap(a, b);
    }
}
