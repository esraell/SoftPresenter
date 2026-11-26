//! Settings › Song files › Find songs on this computer, the page's first card, as the design
//! draws it (26 November): where it looks, with Search Another Folder… and Find; looking, with
//! Stop, the rows coming in as they are found; a row for each songbook, library or folder of
//! song files, ticked to import; bringing them in; and what came in. Nothing comes in until the
//! operator chooses. What the card asks for is carried out by the app (`find_songs_cli`), and
//! its search and dry run are `find_songs`.

use std::sync::Arc;

use egui::{FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};

use super::parts::{self, Key, card, chips_row, keys_line, note};
use crate::find_songs::{Ask, Card, Kind, Phase, Source, brought_words, row_words};
use crate::i18n::{t, tf, tn};
use crate::studio::{CARE, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT, fixed_semibold, semibold};

/// How many songbooks and libraries were found, as the card says it.
fn found_words(n: usize) -> String {
    tn("settings.find.found", n as u64, "{n} songbook or library found", "{n} songbooks and libraries found", &[])
}

/// The card, in the phase it is in.
pub fn find_card(ui: &mut egui::Ui, state: &mut Card) {
    let about = match state.phase {
        Phase::Before => t("settings.find.about", "Songbooks, libraries and song files you already have").to_owned(),
        Phase::Searching => t("settings.find.about.looking", "Looking\u{2026}").to_owned(),
        Phase::Found if state.sources.is_empty() => t("settings.find.about.none", "Nothing found").to_owned(),
        Phase::Found => found_words(state.sources.len()),
        Phase::Importing => t("settings.find.about.bringing", "Bringing them in\u{2026}").to_owned(),
        Phase::Done => t("settings.find.about.done", "Done").to_owned(),
    };
    card(ui, t("settings.find.title", "Find songs on this computer"), &about, None, |ui| match state.phase {
        Phase::Before => before(ui, state),
        Phase::Searching => searching(ui, state),
        Phase::Found if state.sources.is_empty() => nothing_found(ui, state),
        Phase::Found => found(ui, state),
        Phase::Importing => importing(ui, state),
        Phase::Done => done(ui, state),
    });
}

/// Search Another Folder…, the folder window's key.
fn another_folder() -> &'static str {
    t("settings.find.another", "Search Another Folder\u{2026}")
}

fn find_again() -> &'static str {
    t("settings.find.again", "Find Again")
}

/// Before a search: where it looks, Search Another Folder… and Find, and that nothing comes in
/// unasked.
fn before(ui: &mut egui::Ui, state: &mut Card) {
    let places: Vec<String> = state.places().iter().map(|place| place.words()).collect();
    chips_row(ui, t("settings.find.where", "Where it looks"), &places);
    let find = t("settings.find.find", "Find");
    match keys_line(ui, "find before", "", &[Key::plain(another_folder()), Key::main(find)]) {
        Some(0) => state.asks.push(Ask::AnotherFolder),
        Some(_) => state.asks.push(Ask::Find),
        None => {}
    }
    note(
        ui,
        t(
            "settings.find.note",
            "Nothing comes in until you choose. A song already in your library, word for word under the same title, is never brought in twice.",
        ),
    );
}

/// Looking: where, how many found so far and Stop, and the rows found, coming in as they are.
fn searching(ui: &mut egui::Ui, state: &mut Card) {
    let place = state.looking.as_ref().map(|place| place.words()).unwrap_or_default();
    let mut words = tf("settings.find.looking", "Looking in {place}\u{2026}", &[("place", &place)]);
    if !state.sources.is_empty() {
        words = format!("{words} {}", found_words(state.sources.len()));
    }
    if progress_line(ui, &words, Some(t("settings.find.stop", "Stop"))) {
        state.asks.push(Ask::Stop);
    }
    for (at, source) in state.sources.iter().enumerate() {
        source_row(ui, source, state.ticked.get(at).copied().unwrap_or(false), false);
    }
    // The rows and the place change as the search goes.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
}

/// Found nothing: the places it looked in (or that it stopped before it had looked
/// everywhere), Search Another Folder… and Find Again.
fn nothing_found(ui: &mut egui::Ui, state: &mut Card) {
    let places: Vec<String> = state.looked.iter().map(|place| place.words()).collect();
    let words = match state.ended.words() {
        Some(cut) => format!("{cut} {}", t("settings.find.none_so_far", "Nothing was found until then.")),
        None => tf(
            "settings.find.none",
            "No songbooks, libraries or song files were found in {places}.",
            &[("places", &places.join(", "))],
        ),
    };
    note(ui, &words);
    match keys_line(ui, "find none", "", &[Key::plain(another_folder()), Key::plain(find_again())]) {
        Some(0) => state.asks.push(Ask::AnotherFolder),
        Some(_) => state.asks.push(Ask::Find),
        None => {}
    }
}

/// What was found: that the search stopped before it had looked everywhere, when it did; a row
/// for each, ticked to import (one with nothing new, or that couldn't be read, dimmed and
/// unticked), what comes in of them, Find Again and Import.
fn found(ui: &mut egui::Ui, state: &mut Card) {
    if let Some(cut) = state.ended.words() {
        note(ui, &format!("{cut} {}", t("settings.find.so_far", "These are what it found until then.")));
    }
    for at in 0..state.sources.len() {
        let ticked = state.ticked.get(at).copied().unwrap_or(false);
        if source_row(ui, &state.sources[at], ticked, true)
            && let Some(tick) = state.ticked.get_mut(at)
        {
            *tick = !*tick;
        }
    }
    let new = state.new_ticked();
    let import = if new > 0 {
        tn("settings.find.import", new as u64, "Import {n} New Song", "Import {n} New Songs", &[])
    } else {
        t("settings.find.import_none", "Import").to_owned()
    };
    let lead = t(
        "settings.find.each",
        "Each songbook or library comes in as a library of its own name, with only its new songs. Songs already here stay out.",
    );
    let keys = [Key::plain(find_again()), Key::main(&import).enabled(new > 0)];
    match foot(ui, lead, &keys) {
        Some(0) => state.asks.push(Ask::Find),
        Some(_) => state.asks.push(Ask::Import),
        None => {}
    }
}

/// Bringing them in: how many and how far, a thin bar, and the rows being brought in.
fn importing(ui: &mut egui::Ui, state: &mut Card) {
    let percent = (state.progress.clamp(0.0, 1.0) * 100.0).floor() as u64;
    let percent = tf("settings.size.percent", "{n}%", &[("n", &crate::i18n::count(percent))]);
    let words = tn(
        "settings.find.bringing",
        state.bringing as u64,
        "Bringing in {n} song\u{2026} {percent}",
        "Bringing in {n} songs\u{2026} {percent}",
        &[("percent", &percent)],
    );
    progress_line(ui, &words, None);
    let width = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
    ui.painter().rect_filled(bar, 2.0, PANEL);
    let filled = Rect::from_min_size(bar.min, vec2(width * state.progress.clamp(0.0, 1.0), 4.0));
    ui.painter().rect_filled(filled, 2.0, TEXT);
    ui.add_space(6.0);
    for at in state.chosen() {
        source_row(ui, &state.sources[at], true, false);
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
}

/// Done: how many came in, from which, and how many were left out; Find Again.
fn done(ui: &mut egui::Ui, state: &mut Card) {
    if let Some(brought) = &state.brought {
        let [before, came, after] = brought_words(brought);
        let width = ui.available_width();
        let mut job = egui::text::LayoutJob::default();
        let plain = egui::TextFormat {
            font_id: FontId::proportional(12.0),
            color: TEXT_DIM,
            line_height: Some(18.0),
            ..Default::default()
        };
        job.append(&before, 0.0, plain.clone());
        job.append(&came, 0.0, egui::TextFormat { font_id: semibold(12.0), color: TEXT, ..plain.clone() });
        job.append(&after, 0.0, plain);
        job.wrap.max_width = width.min(600.0);
        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        let (place, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
        ui.painter().galley(place.min, galley, TEXT_DIM);
        ui.add_space(12.0);
    }
    if keys_line(ui, "find done", "", &[Key::plain(find_again())]).is_some() {
        state.asks.push(Ask::Find);
    }
}

/// A line saying what is under way, at the left, with a key at its right when given. True
/// when the key was clicked.
fn progress_line(ui: &mut egui::Ui, words: &str, key: Option<&str>) -> bool {
    let width = ui.available_width();
    let key_size = key.map(|key| parts::key_size(ui, key));
    let room = width - key_size.map_or(0.0, |size| size.x + 12.0);
    let galley = parts::lines(ui, words, FontId::proportional(13.0), TEXT_DIM, room, 18.0);
    let height = galley.size().y.max(30.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, height + 12.0), Sense::hover());
    let middle = place.min.y + 4.0 + height / 2.0;
    ui.painter().galley(pos2(place.min.x, middle - galley.size().y / 2.0), galley, TEXT_DIM);
    let clicked = match (key, key_size) {
        (Some(key), Some(size)) => {
            let rect = Rect::from_min_size(pos2(place.max.x - size.x, middle - size.y / 2.0), size);
            parts::key(ui, rect, ui.scope_id().with(("find progress", key)), key, false).clicked()
        }
        _ => false,
    };
    ui.add_space(4.0);
    clicked
}

/// The line under the rows: what an import does, at the left, and the keys at the right (under
/// it when both don't fit side by side). The key clicked, if any.
fn foot(ui: &mut egui::Ui, lead: &str, keys: &[Key]) -> Option<usize> {
    let width = ui.available_width();
    let top = ui.cursor().min;
    ui.painter().hline(top.x..=top.x + width, top.y + 0.5, Stroke::new(1.0, LINE));
    ui.add_space(12.0);
    let keys_wide = parts::keys_width(ui, keys);
    let room = width - keys_wide - 16.0;
    if room >= 220.0 {
        // The words wrap in the room the keys leave, as the design's note does.
        let galley = parts::lines(ui, lead, FontId::proportional(12.0), TEXT_FAINT, room.min(360.0), 18.0);
        let height = galley.size().y.max(30.0);
        let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        ui.painter().galley(pos2(place.min.x, place.center().y - galley.size().y / 2.0), galley, TEXT_FAINT);
        let mut inside = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(place.max.x - keys_wide, place.center().y - 15.0), place.max)),
        );
        let clicked = keys_line(&mut inside, "find foot", "", keys);
        ui.add_space(12.0);
        return clicked;
    }
    note(ui, lead);
    keys_line(ui, "find foot", "", keys)
}

/// The design's least room for a row's words beside its counts: past that, the counts go under
/// the words.
const COUNTS_SHARE: f32 = 0.45;

/// A row for a source found: its tick box, its name, what and where it is and how many songs,
/// under them what the operator should know of it (in amber), and at its right how many are new
/// and already here (in the fixed-width face), that everything is here, or why it couldn't be
/// read (in amber). One with nothing new, or that couldn't be read, is dimmed and can't be
/// ticked. `can_tick`: the rows can be ticked now (once the search has ended). True when it was
/// clicked to tick or untick it.
fn source_row(ui: &mut egui::Ui, source: &Source, ticked: bool, can_tick: bool) -> bool {
    let width = ui.available_width();
    let usable = source.can_import();
    let clickable = can_tick && usable;
    let (name, line, right) = row_words(source);
    let name_ink = if usable { TEXT } else { TEXT_FAINT };
    let warn = source.why.is_some();
    // What it says at its right: the counts in the fixed-width face, a why in amber.
    let (right_font, right_ink) = if warn {
        (FontId::proportional(12.0), CARE)
    } else {
        (fixed_semibold(11.0), if usable { TEXT } else { TEXT_FAINT })
    };
    let spacing = if warn { 0.0 } else { 0.44 };
    let share = (width - 30.0) * COUNTS_SHARE;
    let whole = parts::words(ui, &right, right_font.clone(), right_ink, spacing, f32::INFINITY);
    // Counts that don't fit beside the words go under them, whole; a why wraps in its share.
    let beside = warn || whole.size().x <= share;
    let right_galley: Arc<egui::Galley> =
        if warn { parts::words(ui, &right, right_font.clone(), right_ink, 0.0, share.min(300.0)) } else { whole };
    let text_room = if beside { width - 30.0 - right_galley.size().x - 12.0 } else { width - 30.0 };
    let name_galley = parts::lines(ui, &name, semibold(13.5), name_ink, text_room, 18.0);
    let line_galley = parts::lines(ui, &line, FontId::proportional(12.0), TEXT_DIM, text_room, 16.0);
    let caution_galley = source
        .caution
        .as_ref()
        .map(|caution| parts::lines(ui, caution, FontId::proportional(12.0), CARE, text_room, 16.0));
    let caution_height = caution_galley.as_ref().map_or(0.0, |galley| 2.0 + galley.size().y);
    let words_height = name_galley.size().y + 2.0 + line_galley.size().y + caution_height;
    let inner =
        if beside { words_height.max(right_galley.size().y) } else { words_height + 4.0 + right_galley.size().y };
    let sense = if clickable { Sense::click() } else { Sense::hover() };
    let (place, response) = ui.allocate_exact_size(vec2(width, inner + 22.0), sense);
    let response = response.on_hover_text(source.path.display().to_string());
    let painter = ui.painter();
    painter.hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, LINE));
    let middle = place.center().y;
    // The tick box, where there is one to tick (a place that couldn't be looked in has none).
    if source.kind != Kind::Place {
        let tick = Rect::from_center_size(pos2(place.min.x + 8.0, middle), vec2(16.0, 16.0));
        crate::songbooks::paint_tick_box(painter, tick, ticked && usable, !can_tick && usable, clickable);
    }
    let left = place.min.x + 30.0;
    let top = place.min.y + 11.0 + if beside { (inner - words_height) / 2.0 } else { 0.0 };
    let name_height = name_galley.size().y;
    let line_height = line_galley.size().y;
    painter.galley(pos2(left, top), name_galley, name_ink);
    painter.galley(pos2(left, top + name_height + 2.0), line_galley, TEXT_DIM);
    if let Some(caution) = caution_galley {
        painter.galley(pos2(left, top + name_height + 2.0 + line_height + 2.0), caution, CARE);
    }
    let right_at: Pos2 = if beside {
        pos2(place.max.x - right_galley.size().x, middle - right_galley.size().y / 2.0)
    } else {
        pos2(left, top + words_height + 4.0)
    };
    painter.galley(right_at, right_galley, right_ink);
    clickable && response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find_songs::{Brought, Ended, PlaceName, Read, Usual, Where};
    use crate::home::HomeState;
    use crate::home::settings::{Pane, settings_ui};
    use sp_engine::Show;

    fn source(name: &str, kind: Kind, (place, parts): (Usual, &[&str]), songs: usize, here: usize) -> Source {
        Source {
            name: name.to_owned(),
            kind,
            place: Where {
                place: PlaceName::Usual(place),
                parts: parts.iter().map(|part| (*part).to_owned()).collect(),
            },
            path: std::path::PathBuf::from(name),
            read: Read::Nothing,
            songs,
            here,
            twice: 0,
            why: None,
            caution: None,
            new: (0..songs.saturating_sub(here) as u64).map(|n| n + songs as u64 * 1000).collect(),
        }
    }

    /// The design's five rows: a songbook with a few here, a library, song files all new, song
    /// files all here, and a database that couldn't be read.
    fn sample() -> Vec<Source> {
        let broken = Source {
            why: Some("the file is cut short or damaged".to_owned()),
            ..source("spData.sqlite", Kind::Songbook, (Usual::Downloads, &["old laptop"]), 0, 0)
        };
        // Copied while SoftProjector had it open.
        let open = crate::i18n::tf(
            "notices.import.songbook_unsaved",
            "{name} was still open in SoftProjector when it was copied, so its latest changes may be missing. Close SoftProjector and import it again to be sure.",
            &[("name", "spData.sqlite")],
        );
        vec![
            Source {
                caution: Some(open),
                ..source("Zpěvník pro zkoušku", Kind::Songbook, (Usual::SoftProjector, &[]), 3301, 13)
            },
            source("Youth Songs", Kind::Library, (Usual::Documents, &["Worship"]), 214, 16),
            // Two of its songs are there twice over.
            Source { songs: 34, twice: 2, ..source("Christmas 2025", Kind::SongFiles, (Usual::Downloads, &[]), 32, 0) },
            source("Hymns", Kind::SongFiles, (Usual::Desktop, &[]), 48, 48),
            broken,
        ]
    }

    fn state_in(phase: Phase) -> HomeState {
        let mut state = HomeState { preferences_pane: Pane::SongFiles, ..HomeState::default() };
        let card = &mut state.find_songs;
        card.phase = phase;
        card.looked = Usual::ALL.iter().map(|usual| PlaceName::Usual(*usual)).collect();
        card.looking = Some(PlaceName::Usual(Usual::Downloads));
        if phase != Phase::Before {
            for source in sample() {
                card.add(source);
            }
        }
        card.bringing = card.new_ticked();
        card.progress = 0.4;
        if phase == Phase::Done {
            card.brought = Some(Brought {
                came: 3518,
                from: vec![("Zpěvník pro zkoušku".to_owned(), 3288), ("Youth Songs".to_owned(), 198)],
                left_out: 29,
                twice: 4,
            });
        }
        state
    }

    fn click(state: &mut HomeState, words: &str) -> bool {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        crate::home::settings::tests::click(&ctx, state, &show, words)
    }

    #[test]
    fn each_key_asks_for_what_it_says() {
        let mut state = state_in(Phase::Before);
        assert!(click(&mut state, "Find"));
        assert!(click(&mut state, "Search Another Folder\u{2026}"));
        assert_eq!(state.find_songs.asks, [Ask::Find, Ask::AnotherFolder]);
        let mut state = state_in(Phase::Searching);
        assert!(click(&mut state, "Stop"));
        assert_eq!(state.find_songs.asks, [Ask::Stop]);
        // While looking, the rows can't be ticked yet.
        assert!(click(&mut state, "Youth Songs"));
        assert!(state.find_songs.ticked[1]);
        let mut state = state_in(Phase::Found);
        state.find_songs.sources.clear();
        state.find_songs.ticked.clear();
        assert!(click(&mut state, "Find Again"));
        assert!(click(&mut state, "Search Another Folder\u{2026}"));
        assert_eq!(state.find_songs.asks, [Ask::Find, Ask::AnotherFolder]);
        let mut state = state_in(Phase::Found);
        let new = state.find_songs.new_ticked();
        assert_eq!(new, 3288 + 198 + 32, "the rows with something new start ticked");
        assert!(click(&mut state, "Import 3,518 New Songs"));
        assert!(click(&mut state, "Find Again"));
        assert_eq!(state.find_songs.asks, [Ask::Import, Ask::Find]);
        let mut state = state_in(Phase::Done);
        assert!(click(&mut state, "Find Again"));
        assert_eq!(state.find_songs.asks, [Ask::Find]);
    }

    #[test]
    fn a_tick_takes_its_songs_out_of_the_count_and_a_dimmed_row_cant_be_ticked() {
        let mut state = state_in(Phase::Found);
        assert!(click(&mut state, "Youth Songs"));
        assert!(!state.find_songs.ticked[1]);
        assert_eq!(state.find_songs.new_ticked(), 3288 + 32);
        // Everything here already, and couldn't be read: dimmed, unticked, never ticked.
        assert!(click(&mut state, "Hymns"));
        assert!(click(&mut state, "spData.sqlite"));
        assert_eq!(state.find_songs.ticked, [true, false, true, false, false]);
        for name in ["Zpěvník pro zkoušku", "Christmas 2025"] {
            assert!(click(&mut state, name));
        }
        assert_eq!(state.find_songs.new_ticked(), 0);
        // With nothing ticked, the key is a greyed Import that asks nothing.
        assert!(click(&mut state, "Import"));
        assert!(state.find_songs.asks.is_empty());
    }

    #[test]
    fn each_state_says_what_the_design_says() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        for (phase, words) in [
            (
                Phase::Before,
                &[
                    "Where it looks",
                    "Documents",
                    "SoftProjector\u{2019}s folder",
                    "SONGBOOKS, LIBRARIES AND SONG FILES YOU ALREADY HAVE",
                ][..],
            ),
            (Phase::Searching, &["Looking in Downloads\u{2026} 5 songbooks and libraries found", "LOOKING\u{2026}"]),
            (
                Phase::Found,
                &[
                    "5 SONGBOOKS AND LIBRARIES FOUND",
                    "3,288 new \u{b7} 13 already here",
                    "32 new \u{b7} 2 found twice",
                    "Song files \u{b7} Downloads \u{b7} 34 songs",
                    "Everything here is already in your library",
                    "Couldn\u{2019}t be read: the file is cut short or damaged",
                    "Library \u{b7} from SoftProjector \u{b7} 3,301 songs",
                    "spData.sqlite was still open in SoftProjector when it was copied, so its latest changes may be missing. Close SoftProjector and import it again to be sure.",
                    "Library \u{b7} Documents \u{203a} Worship \u{b7} 214 songs",
                    "Library \u{b7} from SoftProjector \u{b7} Downloads \u{203a} old laptop",
                ],
            ),
            (Phase::Importing, &["Bringing in 3,518 songs\u{2026} 40%", "BRINGING THEM IN\u{2026}"]),
            (Phase::Done, &["DONE", "Find Again"]),
        ] {
            let mut state = state_in(phase);
            let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
            for words in words {
                assert!(crate::studio::tests::place_of(&drawn, words).is_some(), "{phase:?}: {words}");
            }
            drawn.drop_without_applying_deltas();
        }
        // Found nothing names the places it looked in.
        let mut state = state_in(Phase::Found);
        state.find_songs.sources.clear();
        state.find_songs.ticked.clear();
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let none = "No songbooks, libraries or song files were found in Documents, Downloads, Desktop, SoftProjector\u{2019}s folder.";
        assert!(crate::studio::tests::place_of(&drawn, none).is_some());
        drawn.drop_without_applying_deltas();
        // A search cut short says so, over what it found, or that it found nothing until then.
        let mut state = state_in(Phase::Found);
        state.find_songs.ended = Ended::TooLong;
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let cut = "It stopped after 90 seconds, the longest it looks, before it had looked everywhere. These are what it found until then.";
        assert!(crate::studio::tests::place_of(&drawn, cut).is_some());
        drawn.drop_without_applying_deltas();
        state.find_songs.ended = Ended::Stopped;
        state.find_songs.sources.clear();
        state.find_songs.ticked.clear();
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let cut = "It was stopped before it had looked everywhere. Nothing was found until then.";
        assert!(crate::studio::tests::place_of(&drawn, cut).is_some());
        assert!(crate::studio::tests::place_of(&drawn, none).is_none(), "it doesn't say it looked everywhere");
        drawn.drop_without_applying_deltas();
        // Done says what came in, the count in bold.
        let words = brought_words(state_in(Phase::Done).find_songs.brought.as_ref().unwrap()).concat();
        assert_eq!(
            words,
            "3,518 songs came in: Zpěvník pro zkoušku (3,288), Youth Songs (198). 29 already in your library were left out. 4 found in more than one place came in once."
        );
    }

    #[test]
    fn nothing_passes_the_narrowest_settings_window_in_any_language() {
        use crate::i18n::Language;
        let right = 820.0 - crate::studio::GAP - 28.0;
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for phase in [Phase::Before, Phase::Searching, Phase::Found, Phase::Importing, Phase::Done] {
                    let mut state = state_in(phase);
                    state.find_songs.added.push(std::path::PathBuf::from("/Volumes/Old laptop/Worship songs 2019"));
                    // The longest words a search cut short says.
                    state.find_songs.ended = Ended::TooMany;
                    let ctx = crate::studio::tests::styled();
                    let show = Show::new(Vec::new());
                    let mut drawn = Vec::new();
                    for _ in 0..2 {
                        let raw = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(820.0, 4000.0))),
                            ..egui::RawInput::default()
                        };
                        let output = ctx.run_ui(raw, |ui| settings_ui(ui, &mut state, &show));
                        drawn = crate::home::settings::tests::words_drawn(&output);
                        output.drop_without_applying_deltas();
                    }
                    for (words, rect) in drawn.iter().filter(|(words, _)| !words.is_empty()) {
                        assert!(
                            rect.max.x <= right + 0.5,
                            "{language:?}, {phase:?}: \u{201c}{words}\u{201d} ends at {}",
                            rect.max.x
                        );
                    }
                }
            });
        }
    }
}
