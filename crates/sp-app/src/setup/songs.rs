//! The setup's Songs step: two cards side by side. Already on this computer: the search
//! Settings › Song files › Find songs on this computer makes (`find_songs`, the same state and
//! the same rows), with what it found ticked to bring in. Free songbooks: those shared by
//! churches (`free_songbooks`), and the song catalog's where it is offered (`songbooks`), the
//! ones in the church's languages ticked and marked Recommended. Nothing comes in until the
//! step is left with its key, which then says so: Bring In and Continue.

use egui::{FontId, Rect, Sense, pos2, vec2};

use super::{Env, LANGUAGES, Setup, SetupRequest, System};
use crate::find_songs::{Ask, Phase, Usual, brought_words};
use crate::home::HomeState;
use crate::home::settings::find_card;
use crate::i18n::{t, tf, tn};
use crate::quick_help::Hint;
use crate::studio::{CARD, CARE, PANEL, READY, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// True when songs found on this computer are ticked to come in: the step's key then reads
/// Bring In and Continue, and brings them in.
pub fn bringing(state: &HomeState) -> bool {
    state.find_songs.phase == Phase::Found && state.find_songs.new_ticked() > 0
}

/// The step was passed with its key: the songbooks ticked start downloading, and the songs
/// ticked among those found start coming in. Left any other way, nothing does.
pub fn passed(setup: &mut Setup, state: &mut HomeState) {
    setup.free_songs.confirmed = true;
    setup.shared.confirmed = true;
    crate::songbooks::setup_closed(&setup.free_songs, &mut state.songbooks);
    crate::free_songbooks::setup_closed(&setup.shared, &mut state.free_songbooks);
    setup.free_songs.confirmed = false;
    setup.shared.confirmed = false;
    if bringing(state) {
        state.find_songs.asks.push(Ask::Import);
    }
}

/// What is downloading, for the foot of every step: "↓ Песнь Возрождения 3300 38%".
pub fn downloading_words(state: &HomeState) -> Option<String> {
    let per = |per: &Option<u8>| per.map(|per| format!(" {per}%")).unwrap_or_default();
    let mut parts: Vec<String> = state
        .free_songbooks
        .known
        .busy
        .iter()
        .filter_map(|(id, at)| crate::free_songbooks::by_id(id).map(|book| format!("{}{}", book.name, per(at))))
        .collect();
    parts.extend(
        state
            .songbooks
            .known
            .busy
            .iter()
            .filter_map(|(code, at)| crate::catalog::songbook(code).map(|book| format!("{}{}", book.name(), per(at)))),
    );
    (!parts.is_empty()).then(|| format!("\u{2193} {}", parts.join(" \u{b7} ")))
}

/// What the line under the rows found says: how many songs the ticked rows bring in with the
/// step's key, or to tick some; opened again, that songs already in the library stay out.
pub fn coming_words(new: usize, again: bool) -> String {
    let mut words = if new == 0 {
        t("setup.songs.tick", "Tick what to bring in.").to_owned()
    } else {
        tn(
            "setup.songs.coming",
            new as u64,
            "{n} song comes in with Continue, each library under its own name.",
            "{n} songs come in with Continue, each library under its own name.",
            &[],
        )
    };
    if again {
        words.push(' ');
        words.push_str(t("setup.songs.stay_out", "Songs already in your library stay out."));
    }
    words
}

/// What the card says a Mac will ask when it looks.
fn mac_words() -> &'static str {
    t(
        "setup.songs.mac_asks",
        "macOS may ask whether SoftPresenter can look in each folder. Choose OK; it only reads songs.",
    )
}

/// What the free songbooks' card says under its title: whose they are.
pub fn free_about(catalog: bool) -> &'static str {
    if catalog {
        t("setup.songs.free.catalog", "From Worship Leader App, kept on this computer apart from your own songs")
    } else {
        t("setup.songs.free.shared", "Shared by churches; each comes in as a library of its own")
    }
}

/// A card's title, and the line beside it.
fn card_head(ui: &mut egui::Ui, title: &str, about: &str) {
    super::words(ui, title, semibold(14.0), TEXT, 20.0, 1);
    if !about.is_empty() {
        super::small(ui, about, 1);
    }
    ui.add_space(8.0);
}

/// The step. `room`: how tall the step's room is, so its cards fill it and keep their height
/// whatever the search finds.
pub fn step(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env, room: f32) {
    let top = ui.cursor().min.y;
    super::head(
        ui,
        t("setup.songs.title", "Bring your songs in"),
        t(
            "setup.songs.about",
            "What you already have, and free songbooks to start with. Nothing comes in until you choose.",
        ),
    );
    let height = (room - (ui.cursor().min.y - top) - 2.0).max(200.0);
    let width = ui.available_width();
    let (both, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let half = ((width - 14.0) / 2.0).floor();
    let cards = [
        Rect::from_min_size(both.min, vec2(half, height)),
        Rect::from_min_size(pos2(both.max.x - half, both.min.y), vec2(half, height)),
    ];
    for (at, card) in cards.into_iter().enumerate() {
        ui.painter().rect_filled(card, 9.0, CARD);
        let inside = card.shrink2(vec2(16.0, 14.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inside)
                .layout(egui::Layout::top_down(egui::Align::Min))
                .id_salt(("setup songs card", at)),
        );
        child.set_clip_rect(card);
        child.set_width(inside.width());
        if at == 0 {
            already_here(&mut child, setup, state, env, inside);
        } else {
            free_songbooks(&mut child, setup, state, env, inside);
        }
    }
}

/// Already on this computer: before a search, where it looks, Choose Song Files… and Look on
/// This Computer; then the search's own line and rows, ticked to bring in.
fn already_here(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env, inside: Rect) {
    let card = &mut state.find_songs;
    let found = !card.sources.is_empty();
    card_head(
        ui,
        t("setup.songs.here", "Already on this computer"),
        // The line goes once there are results: the rows say what they are.
        if found { "" } else { t("setup.songs.here.about", "Song libraries, SoftProjector songbooks and song files") },
    );
    // The rows scroll inside the card, above the line that says what comes in.
    let list = |ui: &mut egui::Ui, below: f32| (inside.max.y - ui.cursor().min.y - below).max(60.0);
    let mut choose = false;
    match card.phase {
        Phase::Before => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.label(egui::RichText::new(t("setup.songs.looks_in", "It looks in")).size(12.0).color(TEXT_FAINT));
                for usual in Usual::ALL {
                    chip(ui, usual.name());
                }
                for folder in &card.added {
                    chip(ui, &crate::find_songs::folder_name(folder));
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                choose =
                    super::key(ui, "choose songs", t("setup.songs.choose", "Choose Song Files\u{2026}"), 90.0, true)
                        .hint(t("quickhelp.setup.choose", "Choose song files on this computer to bring in."))
                        .clicked();
                if super::key(ui, "look", t("setup.songs.look_here", "Look on This Computer"), 90.0, true)
                    .hint(t(
                        "quickhelp.setup.look_here",
                        "Look on this computer for song libraries, songbooks and song files.",
                    ))
                    .clicked()
                {
                    card.asks.push(Ask::Find);
                }
            });
            if env.system == System::Mac {
                ui.add_space(10.0);
                super::small(ui, mac_words(), 1);
            }
        }
        Phase::Searching => {
            let place = card.looking.as_ref().map(|place| place.words()).unwrap_or_default();
            let mut words = tf("settings.find.looking", "Looking in {place}\u{2026}", &[("place", &place)]);
            if found {
                words = format!("{words} {}", find_card::found_words(card.sources.len()));
            }
            if find_card::progress_line(ui, &words, Some(t("settings.find.stop", "Stop"))) {
                card.asks.push(Ask::Stop);
            }
            let height = list(ui, 0.0);
            egui::ScrollArea::vertical().id_salt("setup songs found").max_height(height).show(ui, |ui| {
                find_card::rows(ui, card, None, false);
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        Phase::Found if !found => {
            let places: Vec<String> = card.looked.iter().map(|place| place.words()).collect();
            super::small(
                ui,
                &tf(
                    "setup.songs.none_found",
                    "No song libraries or song files were found in {places}.",
                    &[("places", &places.join(", "))],
                ),
                1,
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                choose =
                    super::key(ui, "choose songs", t("setup.songs.choose", "Choose Song Files\u{2026}"), 90.0, true)
                        .hint(t("quickhelp.setup.choose", "Choose song files on this computer to bring in."))
                        .clicked();
                if super::key(ui, "look again", t("setup.songs.look_again", "Look Again"), 90.0, true)
                    .hint(t("quickhelp.setup.look_again", "Look on this computer again."))
                    .clicked()
                {
                    card.asks.push(Ask::Find);
                }
            });
        }
        Phase::Found => {
            // Two lines' room for the line under the rows, in every language.
            let height = list(ui, 8.0 + 2.0 * 17.0);
            egui::ScrollArea::vertical()
                .id_salt("setup songs found")
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(at) = find_card::rows(ui, card, None, true)
                        && let Some(tick) = card.ticked.get_mut(at)
                    {
                        *tick = !*tick;
                    }
                });
            ui.add_space(8.0);
            super::small(ui, &coming_words(card.new_ticked(), setup.again()), 2);
        }
        Phase::Importing => {
            let percent = (card.progress.clamp(0.0, 1.0) * 100.0).floor() as u64;
            let percent = tf("settings.size.percent", "{n}%", &[("n", &crate::i18n::count(percent))]);
            let words = tn(
                "settings.find.bringing",
                card.bringing as u64,
                "Bringing in {n} song\u{2026} {percent}",
                "Bringing in {n} songs\u{2026} {percent}",
                &[("percent", &percent)],
            );
            find_card::progress_line(ui, &words, None);
            let chosen = card.chosen();
            let height = list(ui, 0.0);
            egui::ScrollArea::vertical().id_salt("setup songs found").max_height(height).show(ui, |ui| {
                find_card::rows(ui, card, Some(&chosen), false);
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        Phase::Done => {
            if let Some(brought) = &card.brought {
                let said = brought_words(brought).concat();
                super::words(ui, &said, FontId::proportional(12.5), TEXT_DIM, 18.0, 1);
                ui.add_space(10.0);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                choose =
                    super::key(ui, "choose songs", t("setup.songs.choose", "Choose Song Files\u{2026}"), 90.0, true)
                        .hint(t("quickhelp.setup.choose", "Choose song files on this computer to bring in."))
                        .clicked();
                if super::key(ui, "look again", t("setup.songs.look_again", "Look Again"), 90.0, true)
                    .hint(t("quickhelp.setup.look_again", "Look on this computer again."))
                    .clicked()
                {
                    card.asks.push(Ask::Find);
                }
            });
        }
    }
    if choose {
        state.setup_request = Some(SetupRequest::ChooseSongFiles);
    }
    // How bringing chosen song files in went, as the notices say it.
    if let Some(said) = &setup.songs_said
        && matches!(state.find_songs.phase, Phase::Before | Phase::Done)
    {
        ui.add_space(10.0);
        super::words(ui, said, FontId::proportional(12.5), READY, 18.0, 1);
    }
    // For the checks that the card holds what it says: how far down its words went, and its room.
    let fit = super::Fit { content: ui.cursor().min.y - inside.min.y, room: inside.height() };
    ui.data_mut(|data| data.insert_temp(card_fit_id(), fit));
}

fn card_fit_id() -> egui::Id {
    egui::Id::unique("setup songs card fit")
}

/// How well the Already on this computer card's words fit it, at the last drawing.
#[cfg(test)]
pub fn card_fit(ctx: &egui::Context) -> super::Fit {
    ctx.data(|data| data.get_temp::<super::Fit>(card_fit_id()).unwrap_or_default())
}

/// A place's name in a small pill.
fn chip(ui: &mut egui::Ui, name: &str) {
    let galley = ui.painter().layout_no_wrap(name.to_owned(), FontId::proportional(11.5), TEXT_DIM);
    let (place, _) = ui.allocate_exact_size(galley.size() + vec2(16.0, 6.0), Sense::hover());
    ui.painter().rect_filled(place, place.height() / 2.0, PANEL);
    ui.painter().galley(place.min + vec2(8.0, 3.0), galley, TEXT_DIM);
}

/// Free songbooks: the song catalog's where it is offered, then those shared by churches, the
/// church's languages' first and ticked, in a list that scrolls inside the card.
fn free_songbooks(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env, inside: Rect) {
    card_head(ui, t("setup.songs.free", "Free songbooks"), free_about(env.catalog));
    if let Some(problem) = &state.free_songbooks.problem {
        super::words(ui, problem, FontId::proportional(12.0), CARE, 17.0, 1);
        ui.add_space(6.0);
    }
    let languages = crate::free_songbooks::church_languages_offered(&setup.languages);
    let height = (inside.max.y - ui.cursor().min.y).max(60.0);
    egui::ScrollArea::vertical().id_salt("setup free songbooks").max_height(height).auto_shrink([false, false]).show(
        ui,
        |ui| {
            // The rows are a shade darker than the card they sit on.
            if env.catalog {
                crate::songbooks::setup_rows(
                    ui,
                    &mut setup.free_songs,
                    &mut state.songbooks,
                    &setup.languages,
                    &LANGUAGES,
                );
                ui.add_space(6.0);
                super::cap(ui, t("freesongbooks.setup.heading", "Free songbooks shared by churches"));
                ui.add_space(4.0);
            }
            crate::free_songbooks::setup_rows(ui, &mut setup.shared, &mut state.free_songbooks, &languages);
        },
    );
}
