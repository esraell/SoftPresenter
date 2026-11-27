//! Settings › Song files › Find songs on this computer, the page's first card, as the design
//! draws it (26 November): before a search, one line, Look for songs you already have, saying
//! where it looks, with Choose a Folder… and Find; looking, with Stop, the rows coming in as
//! they are found; a row for each songbook, library or folder of song files, ticked to import;
//! bringing them in; and what came in. Nothing comes in until the operator chooses. What the
//! card asks for is carried out by the app (`find_songs_cli`), and its search and dry run are
//! `find_songs`.

use std::sync::Arc;

use egui::{FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};

use super::parts::{self, Key, card, keys_line, note};
use crate::find_songs::{Ask, Card, Kind, Phase, Source, brought_words, folder_name, row_words};
use crate::i18n::{t, tf, tn};
use crate::studio::{CARE, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT, fixed_semibold, semibold};
use unicode_segmentation::UnicodeSegmentation;

/// How many songbooks and libraries were found, as the card says it.
fn found_words(n: usize) -> String {
    tn("settings.find.found", n as u64, "{n} songbook or library found", "{n} songbooks and libraries found", &[])
}

/// The card, in the phase it is in.
pub fn find_card(ui: &mut egui::Ui, state: &mut Card) {
    let about = match state.phase {
        Phase::Before => t("settings.find.about", "Songbooks, libraries and song files").to_owned(),
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

/// Search Another Folder…, the folder window's key once a search has found nothing.
fn another_folder() -> &'static str {
    t("settings.find.another", "Search Another Folder\u{2026}")
}

/// Choose a Folder…, the folder window's key before a search.
fn choose_folder() -> &'static str {
    t("settings.find.choose_folder", "Choose a Folder\u{2026}")
}

/// How wide Find is at least, as the design draws its main key.
const FIND_WIDTH: f32 = 84.0;

fn find_again() -> &'static str {
    t("settings.find.again", "Find Again")
}

/// How many lines the sentence before a search keeps room for from the start, in every
/// language, so choosing a folder moves nothing (the owner: nothing changes size when clicked).
const SENTENCE_LINES: usize = 3;

/// A folder's name as long as an ordinary one gets, made the hardest to wrap: 28 letters of
/// Cyrillic (which takes more room than Latin) in one word, which a line can't break. The line's
/// keys go beside its words only where the sentence with a folder of this name fits in its three
/// lines there, so an ordinary name never has to be cut. Never shown.
const ORDINARY_NAME: &str = "песнимолодёжногохорагодовые2";

/// Whether the line's keys go beside its words, which have `room` there: where the sentence with
/// one folder of an ordinary name's length fits in its three lines. The window's width and the
/// language decide it, never the folders chosen, so nothing moves as they are.
fn keys_beside(ui: &egui::Ui, room: f32) -> bool {
    parts::note_lines(ui, &looks_in_named(&[ORDINARY_NAME.to_owned()], 0), room) <= SENTENCE_LINES
}

/// Before a search, one line (the owner, 26 November): Look for songs you already have, with
/// where it looks and that nothing comes in unasked in its sentence, and Choose a Folder… and
/// Find at its right, or under the sentence where the window is too narrow for an ordinary
/// folder's name beside them. The sentence keeps three lines' room, names the first two folders
/// added and counts the others, a name cut in its middle only where it would take more, with the
/// folders' whole paths under the pointer.
fn before(ui: &mut egui::Ui, state: &mut Card) {
    let label = t("settings.find.look", "Look for songs you already have");
    let keys = [Key::plain(choose_folder()), Key::main(t("settings.find.find", "Find")).at_least(FIND_WIDTH)];
    let width = ui.available_width();
    let beside = keys_beside(ui, parts::room_beside_keys(ui, width, &keys));
    let room = parts::keys_row_note_room(ui, width, &keys, beside);
    let sentence = looks_in_fitted(ui, &state.added, room, SENTENCE_LINES);
    let (clicked, words) =
        parts::keys_row_keeping(ui, "find before", label, (&sentence, SENTENCE_LINES), &keys, beside);
    if !state.added.is_empty() {
        ui.interact(words, ui.scope_id().with("find before folders"), Sense::hover())
            .on_hover_text(added_paths(&state.added));
    }
    match clicked {
        Some(0) => state.asks.push(Ask::AnotherFolder),
        Some(_) => state.asks.push(Ask::Find),
        None => {}
    }
}

/// Every folder added, whole, one a line: the hover of the line's words.
fn added_paths(added: &[std::path::PathBuf]) -> String {
    added.iter().map(|folder| folder.display().to_string()).collect::<Vec<_>>().join("\n")
}

/// Where it looks, as the line's sentence says it: the usual places, then the first two folders
/// added with Choose a Folder…, by their own names, and how many more.
fn looks_in_words(added: &[std::path::PathBuf]) -> String {
    let named: Vec<String> = added.iter().take(2).map(|folder| folder_name(folder)).collect();
    looks_in_named(&named, added.len().saturating_sub(2))
}

/// The sentence, naming the folders `named` (the first two added, as they are to be said) and
/// counting `more` others.
fn looks_in_named(named: &[String], more: usize) -> String {
    match named {
        [] => t(
            "settings.find.looks_in",
            "Looks in Documents, Downloads, the Desktop and SoftProjector\u{2019}s folder. Nothing comes in until you choose.",
        )
        .to_owned(),
        [folder] => tf(
            "settings.find.looks_in_one",
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder and {folder}. Nothing comes in until you choose.",
            &[("folder", folder)],
        ),
        [first, second, ..] if more == 0 => tf(
            "settings.find.looks_in_two",
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, {first} and {second}. Nothing comes in until you choose.",
            &[("first", first), ("second", second)],
        ),
        [first, second, ..] => tn(
            "settings.find.looks_in_many",
            more as u64,
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, {first}, {second} and {n} more folder. Nothing comes in until you choose.",
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, {first}, {second} and {n} more folders. Nothing comes in until you choose.",
            &[("first", first), ("second", second)],
        ),
    }
}

/// The sentence as the line shows it, in `kept` lines at most at `room` wide: the folders named
/// whole where that fits, else each cut in its middle to the most letters that fit ("Worship
/// so…Elm Street"), one letter and "…" at the least. Its end is never cut.
fn looks_in_fitted(ui: &egui::Ui, added: &[std::path::PathBuf], room: f32, kept: usize) -> String {
    let fits = |sentence: &str| parts::note_lines(ui, sentence, room) <= kept;
    let whole = looks_in_words(added);
    if fits(&whole) {
        return whole;
    }
    let named: Vec<String> = added.iter().take(2).map(|folder| folder_name(folder)).collect();
    let more = added.len().saturating_sub(2);
    let cut_to =
        |keep: usize| looks_in_named(&named.iter().map(|name| cut_middle(name, keep)).collect::<Vec<_>>(), more);
    let longest = named.iter().map(|name| name.graphemes(true).count()).max().unwrap_or(1);
    // The most letters kept that fit: the sentence grows with what each name keeps.
    let (mut fit, mut over) = (1, longest);
    while over - fit > 1 {
        let keep = (fit + over) / 2;
        if fits(&cut_to(keep)) { fit = keep } else { over = keep }
    }
    cut_to(fit)
}

/// `name` in `keep` letters at most (as a reader counts them: a letter and its accent are one):
/// its start and its end, with "…" between them where it is longer, and no space beside it.
fn cut_middle(name: &str, keep: usize) -> String {
    let letters: Vec<&str> = name.graphemes(true).collect();
    if letters.len() <= keep {
        return name.to_owned();
    }
    let end = keep / 2;
    let start = letters[..keep - end].concat();
    let end = letters[letters.len() - end..].concat();
    format!("{}\u{2026}{}", start.trim_end(), end.trim_start())
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
    rows(ui, state, None, false);
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
        note(ui, &format!("{cut} {}", t("settings.find.so_far", "This is what it found until then.")));
    }
    if let Some(at) = rows(ui, state, None, true)
        && let Some(tick) = state.ticked.get_mut(at)
    {
        *tick = !*tick;
    }
    // The key keeps its words, and so its width, whatever is ticked; the count is in the line
    // beside it, which keeps the room of its longest wording so nothing moves as rows are ticked.
    let new = state.new_ticked();
    let lead = coming_words(new);
    // Room for the most it could say: every row ticked (a song in two rows counted twice here, so
    // never short of the digits the count can have).
    let most = coming_words(state.sources.iter().filter(|source| source.can_import()).map(Source::fresh).sum());
    let none = coming_words(0);
    let import = t("settings.find.import_new", "Import New Songs");
    let keys = [Key::plain(find_again()), Key::main(import).enabled(new > 0)];
    match foot(ui, &lead, &[&most, &none], &keys) {
        Some(0) => state.asks.push(Ask::Find),
        Some(_) => state.asks.push(Ask::Import),
        None => {}
    }
}

/// The line beside Import: how many new songs the rows ticked bring in, and how they come in;
/// with nothing ticked, to tick them.
fn coming_words(new: usize) -> String {
    if new == 0 {
        return t("settings.find.tick", "Tick what to bring in. Songs already here stay out.").to_owned();
    }
    tn(
        "settings.find.coming",
        new as u64,
        "{n} new song comes in, each songbook or library as a library of its own name. Songs already here stay out.",
        "{n} new songs come in, each songbook or library as a library of its own name. Songs already here stay out.",
        &[],
    )
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
    let chosen = state.chosen();
    rows(ui, state, Some(&chosen), false);
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
/// it when both don't fit side by side). It keeps two lines' room, or the room of the longest of
/// `also` (its other wordings) where that is more, so it doesn't change height as it changes.
/// The key clicked, if any.
fn foot(ui: &mut egui::Ui, lead: &str, also: &[&str], keys: &[Key]) -> Option<usize> {
    let width = ui.available_width();
    let top = ui.cursor().min;
    ui.painter().hline(top.x..=top.x + width, top.y + 0.5, Stroke::new(1.0, LINE));
    ui.add_space(12.0);
    let keys_wide = parts::keys_width(ui, keys);
    let room = width - keys_wide - 16.0;
    let beside = room >= 220.0;
    // The words wrap in the room the keys leave, as the design's note does; under them, in the
    // width a note takes.
    let wrap = if beside { room.min(360.0) } else { width.min(600.0) };
    let laid = |words: &str| parts::lines(ui, words, FontId::proportional(12.0), TEXT_FAINT, wrap, 18.0);
    let galley = laid(lead);
    let tallest = also.iter().map(|words| laid(words).size().y).fold(36.0_f32, f32::max);
    let height = galley.size().y.max(tallest);
    if beside {
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
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().galley(place.min, galley, TEXT_FAINT);
    ui.add_space(12.0);
    keys_line(ui, "find foot", "", keys)
}

/// The rows of the sources (`chosen`: only these, ticked), only those in view laid out: a search
/// may find thousands, and the page is drawn many times a second. A row out of view takes the
/// room it took when last laid out at this width, or the last row's until then (worked out when
/// it comes into view). `can_tick`: as for [`source_row`]. The row clicked, if any.
fn rows(ui: &mut egui::Ui, state: &mut Card, chosen: Option<&[usize]>, can_tick: bool) -> Option<usize> {
    let Card { sources, ticked, heights, .. } = state;
    let width = ui.available_width();
    if heights.width != width {
        heights.width = width;
        heights.heights.iter_mut().for_each(|height| *height = None);
    }
    heights.heights.resize(sources.len(), None);
    if heights.typical <= 0.0 {
        heights.typical = 58.0;
    }
    let count = chosen.map_or(sources.len(), <[usize]>::len);
    let index = |k: usize| chosen.map_or(k, |chosen| chosen[k]);
    // Where each row starts, from the top of the list, and where the list ends.
    let mut tops = Vec::with_capacity(count + 1);
    let mut y = 0.0;
    for k in 0..count {
        tops.push(y);
        y += heights.heights.get(index(k)).copied().flatten().unwrap_or(heights.typical);
    }
    tops.push(y);
    let top = ui.cursor().min.y;
    let view = ui.clip_rect().translate(vec2(0.0, -top));
    let (first, end) = crate::studio::rows_in_view(&tops, view);
    ui.add_space(tops[first]);
    let mut clicked = None;
    for k in first..end {
        let at = index(k);
        let Some(source) = sources.get(at) else { continue };
        let tick = chosen.is_some() || ticked.get(at).copied().unwrap_or(false);
        let before = ui.cursor().min.y;
        if source_row(ui, source, tick, can_tick) {
            clicked = Some(at);
        }
        let height = ui.cursor().min.y - before;
        heights.heights[at] = Some(height);
        heights.typical = height;
    }
    ui.add_space(tops[count] - tops[end]);
    clicked
}

/// The design's least room for a row's words beside its counts: past that, the counts go under
/// the words.
const COUNTS_SHARE: f32 = 0.45;

/// How many lines a row's reason for not being read takes at most; the whole of it is the row's
/// hover.
const WHY_LINES: usize = 2;

/// A row for a source found: its tick box, its name, what and where it is and how many songs,
/// under them what the operator should know of it (in amber), and at its right how many are new
/// and already here (in the fixed-width face), or that everything is here. Why one couldn't be
/// read goes under its name instead, across the row, in amber, in two lines at most. One with
/// nothing new, or that couldn't be read, is dimmed and can't be ticked. `can_tick`: the rows can
/// be ticked now (once the search has ended). True when it was clicked to tick or untick it.
fn source_row(ui: &mut egui::Ui, source: &Source, ticked: bool, can_tick: bool) -> bool {
    let width = ui.available_width();
    let usable = source.can_import();
    let clickable = can_tick && usable;
    let (name, line, right) = row_words(source);
    let name_ink = if usable { TEXT } else { TEXT_FAINT };
    let warn = source.why.is_some();
    let room = width - 30.0;
    // The counts at the right, in the fixed-width face; under the words when they don't fit
    // beside them. A reason for not being read is always under them, across the row.
    let counts_ink = if usable { TEXT } else { TEXT_FAINT };
    let right_galley: Arc<egui::Galley> = if warn {
        parts::lines_at_most(ui, &right, FontId::proportional(12.0), CARE, (room, 16.0), WHY_LINES)
    } else {
        parts::words(ui, &right, fixed_semibold(11.0), counts_ink, 0.44, f32::INFINITY)
    };
    let beside = !warn && right_galley.size().x <= room * COUNTS_SHARE;
    let text_room = if beside { room - right_galley.size().x - 12.0 } else { room };
    let name_galley = parts::lines(ui, &name, semibold(13.5), name_ink, text_room, 18.0);
    let line_galley = parts::lines(ui, &line, FontId::proportional(12.0), TEXT_DIM, text_room, 16.0);
    let caution_galley = source
        .caution
        .as_ref()
        .map(|caution| parts::lines(ui, caution, FontId::proportional(12.0), CARE, text_room, 16.0));
    let caution_height = caution_galley.as_ref().map_or(0.0, |galley| 2.0 + galley.size().y);
    let words_height = name_galley.size().y + 2.0 + line_galley.size().y + caution_height;
    // Under the words: the design's 12 between them and a reason, 4 above counts.
    let under = if warn { 12.0 } else { 4.0 };
    let inner =
        if beside { words_height.max(right_galley.size().y) } else { words_height + under + right_galley.size().y };
    let sense = if clickable { Sense::click() } else { Sense::hover() };
    let (place, response) = ui.allocate_exact_size(vec2(width, inner + 22.0), sense);
    let response = response.on_hover_text(hover_words(source));
    let painter = ui.painter();
    painter.hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, LINE));
    let top = place.min.y + 11.0 + if beside { (inner - words_height) / 2.0 } else { 0.0 };
    // The tick box (a place that couldn't be looked in has none): beside the name and the line
    // under it when a reason for not being read is under them, else in the row's middle.
    if source.kind != Kind::Place {
        let middle = if warn { top + words_height / 2.0 } else { place.center().y };
        let tick = Rect::from_center_size(pos2(place.min.x + 8.0, middle), vec2(16.0, 16.0));
        crate::songbooks::paint_tick_box(painter, tick, ticked && usable, !can_tick && usable, clickable);
    }
    let left = place.min.x + 30.0;
    let name_height = name_galley.size().y;
    let line_height = line_galley.size().y;
    painter.galley(pos2(left, top), name_galley, name_ink);
    painter.galley(pos2(left, top + name_height + 2.0), line_galley, TEXT_DIM);
    if let Some(caution) = caution_galley {
        painter.galley(pos2(left, top + name_height + 2.0 + line_height + 2.0), caution, CARE);
    }
    let right_at: Pos2 = if beside {
        pos2(place.max.x - right_galley.size().x, place.center().y - right_galley.size().y / 2.0)
    } else {
        pos2(left, top + words_height + under)
    };
    let right_ink = if warn { CARE } else { counts_ink };
    painter.galley(right_at, right_galley, right_ink);
    clickable && response.clicked()
}

/// A row's hover: where it is; for one that couldn't be read, the whole reason first, lead-in and
/// all, since the row shows two lines of it at most.
fn hover_words(source: &Source) -> String {
    let path = source.path.display().to_string();
    if source.why.is_some() { format!("{}\n{path}", row_words(source).2) } else { path }
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
            cloud: false,
            new: (0..songs.saturating_sub(here) as u64).map(|n| n + songs as u64 * 1000).collect(),
        }
    }

    /// The design's five rows: a songbook with a few here, a library, song files all new, song
    /// files all here, and a database that couldn't be read.
    fn sample() -> Vec<Source> {
        let broken = Source {
            why: Some(crate::load::Why::Reader(sp_model::Reason::RowLongerThanFile)),
            ..source("spData.sqlite", Kind::Songbook, (Usual::Downloads, &["old laptop"]), 0, 0)
        };
        // Copied while SoftProjector had it open.
        let open = t(
            "settings.find.caution.unsaved",
            "Copied while it was open in SoftProjector: its latest changes may be missing",
        )
        .to_owned();
        vec![
            Source {
                caution: Some(open),
                ..source("Zpěvník pro zkoušku", Kind::Songbook, (Usual::SoftProjector, &[]), 3301, 13)
            },
            source("Youth Songs", Kind::Library, (Usual::Documents, &["Worship"]), 214, 16),
            // Two of its songs are there twice over.
            Source {
                songs: 34,
                twice: 2,
                caution: Some(crate::i18n::tn(
                    "settings.find.caution.cloud",
                    2,
                    "{n} more song file here is only in the cloud, so it wasn\u{2019}t looked at",
                    "{n} more song files here are only in the cloud, so they weren\u{2019}t looked at",
                    &[],
                )),
                ..source("Christmas 2025", Kind::SongFiles, (Usual::Downloads, &[]), 32, 0)
            },
            source("Hymns", Kind::SongFiles, (Usual::Desktop, &[]), 48, 48),
            broken,
            // Only in the cloud, with nothing read; and a folder with two more song files there.
            Source { cloud: true, ..source("Hymnal.sps", Kind::Songbook, (Usual::Desktop, &[]), 0, 0) },
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
                failed: false,
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
        assert!(click(&mut state, "Choose a Folder\u{2026}"));
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
        assert!(click(&mut state, "Import New Songs"));
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
        assert_eq!(state.find_songs.ticked, [true, false, true, false, false, false]);
        for name in ["Zpěvník pro zkoušku", "Christmas 2025"] {
            assert!(click(&mut state, name));
        }
        assert_eq!(state.find_songs.new_ticked(), 0);
        // With nothing ticked, the key is greyed and asks nothing.
        assert!(click(&mut state, "Import New Songs"));
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
                    "Find songs on this computer",
                    "SONGBOOKS, LIBRARIES AND SONG FILES",
                    "Look for songs you already have",
                    "Looks in Documents, Downloads, the Desktop and SoftProjector\u{2019}s folder. Nothing comes in until you choose.",
                    "Choose a Folder\u{2026}",
                    "Find",
                ][..],
            ),
            (Phase::Searching, &["Looking in Downloads\u{2026} 6 songbooks and libraries found", "LOOKING\u{2026}"]),
            (
                Phase::Found,
                &[
                    "6 SONGBOOKS AND LIBRARIES FOUND",
                    "In the cloud, not on this computer",
                    "Library \u{b7} from SoftProjector \u{b7} Desktop",
                    "3,288 new \u{b7} 13 already here",
                    "32 new \u{b7} 2 found twice",
                    "Song files \u{b7} Downloads \u{b7} 34\u{a0}songs",
                    "Everything here is already in your library",
                    "Couldn\u{2019}t be read: the file is cut short or damaged (a row is longer than the file)",
                    "Library \u{b7} from SoftProjector \u{b7} 3,301\u{a0}songs",
                    "Copied while it was open in SoftProjector: its latest changes may be missing",
                    "Library \u{b7} Documents \u{203a} Worship \u{b7} 214\u{a0}songs",
                    "Library \u{b7} from SoftProjector \u{b7} Downloads \u{203a} old laptop",
                    "3,518 new songs come in, each songbook or library as a library of its own name. Songs already here stay out.",
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
        // Before a search, one line: no row of where it looks with its folders' pills, no note
        // under it, and the key to add a folder is Choose a Folder….
        let mut state = state_in(Phase::Before);
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        for gone in [
            "Where it looks",
            "Documents",
            "Downloads",
            "Desktop",
            "SoftProjector\u{2019}s folder",
            "Search Another Folder\u{2026}",
            "Nothing comes in until you choose. A song already in your library, word for word under the same title, is never brought in twice.",
        ] {
            assert!(crate::studio::tests::place_of(&drawn, gone).is_none(), "{gone} is still drawn");
        }
        drawn.drop_without_applying_deltas();
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
        let cut = "It stopped after 90 seconds, the longest it looks, before it had looked everywhere. This is what it found until then.";
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
    fn ticking_rows_moves_nothing_on_the_page() {
        // What the line beside Import says follows the ticks; the key keeps its words, and
        // nothing on the page moves or changes size, in the narrowest Settings window or a wide
        // one, in any language.
        use crate::i18n::Language;
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for width in [820.0, 1280.0] {
                    let mut state = state_in(Phase::Found);
                    let ctx = crate::studio::tests::styled();
                    let show = Show::new(Vec::new());
                    let frame = |state: &mut HomeState| {
                        let mut drawn = Vec::new();
                        for _ in 0..2 {
                            let raw = egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                                ..egui::RawInput::default()
                            };
                            let output = ctx.run_ui(raw, |ui| settings_ui(ui, state, &show));
                            drawn = crate::home::settings::tests::words_drawn(&output);
                            output.drop_without_applying_deltas();
                        }
                        drawn
                    };
                    let all = frame(&mut state);
                    let import = t("settings.find.import_new", "Import New Songs");
                    assert!(all.iter().any(|(words, _)| words == import), "{language:?}: {import}");
                    // Some ticked, then none.
                    state.find_songs.ticked[0] = false;
                    let some = frame(&mut state);
                    state.find_songs.ticked.iter_mut().for_each(|tick| *tick = false);
                    let none = frame(&mut state);
                    let lines = [coming_words(3288 + 198 + 32), coming_words(198 + 32), coming_words(0)];
                    // All but the line and the ticks themselves (drawn as a mark).
                    let kept = |drawn: &Vec<(String, Rect)>| -> Vec<(String, Rect)> {
                        drawn
                            .iter()
                            .filter(|(words, _)| !lines.contains(words) && words != "\u{2713}")
                            .cloned()
                            .collect()
                    };
                    for (drawn, line) in [(&all, &lines[0]), (&some, &lines[1]), (&none, &lines[2])] {
                        let (shown, first) = (kept(drawn), kept(&all));
                        assert_eq!(shown, first, "{language:?} at {width}: something moved for \u{201c}{line}\u{201d}");
                        assert!(
                            drawn.iter().any(|(words, _)| words == line),
                            "{language:?} at {width}: \u{201c}{line}\u{201d} isn't shown"
                        );
                    }
                }
            });
        }
    }

    #[test]
    fn a_reason_for_not_being_read_goes_under_the_name_across_the_row_in_two_lines_at_most() {
        use crate::i18n::{Language, with_language};
        use sp_model::{Damage, Reason};
        let why = crate::load::Why::Reader(Reason::Table {
            table: "Songs".to_owned(),
            why: Box::new(Reason::Damaged(Damage::TableOutsideFile)),
        });
        let broken = Source {
            why: Some(why),
            ..source("spData.sqlite", Kind::Songbook, (Usual::Downloads, &["old laptop"]), 0, 0)
        };
        for language in [Language::English, Language::Russian] {
            with_language(language, || {
                let (name, line, reason) = row_words(&broken);
                // The narrowest Settings window, and a wide one.
                for width in [820.0, 1400.0] {
                    let mut state = state_in(Phase::Found);
                    state.find_songs.sources = vec![broken.clone()];
                    state.find_songs.ticked = vec![false];
                    let ctx = crate::studio::tests::styled();
                    let show = Show::new(Vec::new());
                    let mut drawn = Painted::default();
                    for _ in 0..2 {
                        let raw = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                            ..egui::RawInput::default()
                        };
                        let output = ctx.run_ui(raw, |ui| settings_ui(ui, &mut state, &show));
                        drawn = painted(&output);
                        output.drop_without_applying_deltas();
                    }
                    let find = |words: &str| {
                        drawn
                            .texts
                            .iter()
                            .find(|(galley, _)| galley.text() == words)
                            .cloned()
                            .unwrap_or_else(|| panic!("{language:?} {width}: {words} isn't drawn"))
                    };
                    let (name_galley, name_at) = find(&name);
                    let (line_galley, line_at) = find(&line);
                    let (why_galley, why_at) = find(&reason);
                    let say = format!("{language:?} at {width}: {reason}");
                    // Under the name and the line under it, from the name's left edge.
                    assert!((why_at.x - name_at.x).abs() < 0.5, "{say}");
                    assert!(why_at.y >= line_at.y + line_galley.size().y + 8.0, "{say}");
                    assert!(name_at.y + name_galley.size().y <= line_at.y, "{say}");
                    // Across the row: wrapped in the room from there to the row's right edge (the
                    // end of the line drawn over the row).
                    let row_right = drawn
                        .rules
                        .iter()
                        .filter(|(y, _)| *y <= name_at.y)
                        .max_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, right)| *right)
                        .expect("the line over the row");
                    assert!((why_at.x + why_galley.job.wrap.max_width - row_right).abs() < 0.5, "{say}");
                    assert!(why_galley.rows.len() <= 2, "{say}: {} lines", why_galley.rows.len());
                    if why_galley.elided {
                        assert!(why_galley.rows[1].text().ends_with('\u{2026}'), "{say}");
                    }
                    if language == Language::Russian && width == 820.0 {
                        assert!(why_galley.elided, "too long for two lines there, so cut short: {say}");
                    }
                    // The row is tall enough for it: the line under the row (the foot's) is below
                    // the reason, with the row's own room under it.
                    let under_row = drawn
                        .rules
                        .iter()
                        .map(|(y, _)| *y)
                        .filter(|y| *y > name_at.y)
                        .min_by(f32::total_cmp)
                        .expect("the line under the row");
                    assert!(
                        why_at.y + why_galley.size().y + 10.0 <= under_row,
                        "{say}: the reason runs into what follows"
                    );
                    // The tick box is beside the name and the line under it, not in the row's
                    // middle, which the reason has moved down.
                    let words_middle = (name_at.y + line_at.y + line_galley.size().y) / 2.0;
                    let tick = drawn
                        .boxes
                        .iter()
                        .find(|tick| tick.max.x < name_at.x && tick.y_range().contains(words_middle))
                        .unwrap_or_else(|| panic!("{say}: no tick box beside the words"));
                    assert!(
                        (tick.center().y - words_middle).abs() < 0.5,
                        "{say}: the tick box is at {}",
                        tick.center().y
                    );
                }
                // The whole reason, lead-in and all, is the row's hover, before where it is.
                assert_eq!(hover_words(&broken), format!("{reason}\nspData.sqlite"));
            });
        }
    }

    #[test]
    fn only_the_rows_in_view_are_laid_out() {
        // 20,000 songbooks found, in the narrowest Settings window: the page scrolls, and only
        // the rows in view are laid out, the others taking their room.
        let mut state = state_in(Phase::Found);
        state.find_songs.sources.clear();
        state.find_songs.ticked.clear();
        for n in 0..20_000 {
            state.find_songs.add(source(&format!("Songbook {n}"), Kind::Songbook, (Usual::Downloads, &[]), 10, 0));
        }
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut drawn = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(820.0, 700.0))),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| settings_ui(ui, &mut state, &show));
            drawn = crate::home::settings::tests::words_drawn(&output);
            output.drop_without_applying_deltas();
        }
        let shown = |name: &str| drawn.iter().any(|(words, _)| words == name);
        assert!(shown("Songbook 0") && shown("Songbook 3"), "the first rows are drawn");
        assert!(!shown("Songbook 100") && !shown("Songbook 19999"), "rows out of view aren't laid out");
        let laid_out = state.find_songs.heights.heights.iter().flatten().count();
        assert!(laid_out < 30, "{laid_out} rows were laid out");
        // Its count is worked out once for the ticks as they are.
        assert_eq!(state.find_songs.new_ticked(), 10);
    }

    /// What a frame drew: every piece of text, with where its top left is; every level line, by
    /// its height and where it ends at the right; and every tick box (a square of 16).
    #[derive(Default)]
    struct Painted {
        texts: Vec<(Arc<egui::Galley>, Pos2)>,
        rules: Vec<(f32, f32)>,
        boxes: Vec<Rect>,
        /// Every key's block (a filled rectangle 30 high).
        keys: Vec<Rect>,
        /// Every rectangle drawn, filled or outlined, in the order drawn.
        rects: Vec<Rect>,
    }

    impl Painted {
        /// The first piece of text that says `words`, with where its top left is.
        fn text(&self, words: &str) -> Option<(Arc<egui::Galley>, Pos2)> {
            self.texts.iter().find(|(galley, _)| galley.text() == words).cloned()
        }

        /// Whether everything is where it is in `other`, at its size: the same words in the
        /// same places, and the same rectangles and lines.
        fn same_place(&self, other: &Painted) -> bool {
            let texts = |painted: &Painted| -> Vec<(String, Rect)> {
                painted
                    .texts
                    .iter()
                    .map(|(galley, at)| (galley.text().to_owned(), Rect::from_min_size(*at, galley.size())))
                    .collect()
            };
            texts(self) == texts(other) && self.rects == other.rects && self.rules == other.rules
        }
    }

    fn painted(output: &egui::FullOutput) -> Painted {
        fn walk(shape: &egui::Shape, into: &mut Painted) {
            match shape {
                egui::Shape::Text(text) => into.texts.push((text.galley.clone(), text.pos)),
                egui::Shape::LineSegment { points: [a, b], .. } if (a.y - b.y).abs() < 0.01 => {
                    into.rules.push((a.y, a.x.max(b.x)));
                }
                egui::Shape::Rect(rect) => {
                    if rect.rect.size() == vec2(16.0, 16.0) {
                        into.boxes.push(rect.rect);
                    }
                    if rect.rect.height() == 30.0 && rect.fill != egui::Color32::TRANSPARENT {
                        into.keys.push(rect.rect);
                    }
                    into.rects.push(rect.rect);
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut into = Painted::default();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut into));
        into
    }

    #[test]
    fn a_folder_chosen_joins_the_sentence_of_where_it_looks() {
        use crate::i18n::{Language, with_language};
        use std::path::PathBuf;
        let folders = |names: &[&str]| -> Vec<PathBuf> {
            names.iter().map(|name| PathBuf::from("/Volumes/Old laptop").join(name)).collect()
        };
        let english = |names: &[&str]| with_language(Language::English, || looks_in_words(&folders(names)));
        assert_eq!(
            english(&[]),
            "Looks in Documents, Downloads, the Desktop and SoftProjector\u{2019}s folder. Nothing comes in until you choose."
        );
        assert_eq!(
            english(&["Worship songs 2019"]),
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder and Worship songs 2019. Nothing comes in until you choose.",
            "a folder joins the sentence by its own name"
        );
        assert_eq!(
            english(&["Worship songs 2019", "Choir"]),
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, Worship songs 2019 and Choir. Nothing comes in until you choose."
        );
        // Three or more: the first two by name, and how many more.
        assert_eq!(
            english(&["Worship songs 2019", "Choir", "Youth"]),
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, Worship songs 2019, Choir and 1 more folder. Nothing comes in until you choose."
        );
        assert_eq!(
            english(&["Worship songs 2019", "Choir", "Youth", "Christmas", "Easter"]),
            "Looks in Documents, Downloads, the Desktop, SoftProjector\u{2019}s folder, Worship songs 2019, Choir and 3 more folders. Nothing comes in until you choose."
        );
        // Every language has its own words for each, with the first two folders in them, and a
        // count of the others in the form its number takes.
        let five = ["Worship songs 2019", "Choir", "Youth", "Christmas", "Easter"];
        for language in [Language::Spanish, Language::Russian, Language::Ukrainian] {
            for count in 0..=five.len() {
                let names = &five[..count];
                let said = with_language(language, || looks_in_words(&folders(names)));
                assert_ne!(said, english(names), "{language:?} has no words for {names:?}");
                assert!(!said.contains('{') && !said.contains('}'), "{language:?}: {said}");
                assert!(names.iter().take(2).all(|name| said.contains(name)), "{language:?}: {said}");
                assert!(names.iter().skip(2).all(|name| !said.contains(name)), "{language:?}: {said}");
                if count > 2 {
                    assert!(said.contains(&(count - 2).to_string()), "{language:?}: {said}");
                }
            }
        }
        let russian = |count: usize| {
            let names: Vec<String> = (0..count).map(|n| format!("Folder {n}")).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            with_language(Language::Russian, || looks_in_words(&folders(&names)))
        };
        assert!(russian(3).contains("и ещё в 1 папке"), "{}", russian(3));
        assert!(russian(5).contains("и ещё в 3 папках"), "{}", russian(5));
        assert!(russian(9).contains("и ещё в 7 папках"), "{}", russian(9));
        // The card says it, with the folder added.
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_in(Phase::Before);
        state.find_songs.added = folders(&["Worship songs 2019"]);
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        assert!(crate::studio::tests::place_of(&drawn, &english(&["Worship songs 2019"])).is_some());
        drawn.drop_without_applying_deltas();
    }

    /// Draws Settings before a search, `width` wide, three frames (the last with `events`, made
    /// from where the second drew things), and gives what the last drew.
    fn idle_frame(
        ctx: &egui::Context,
        state: &mut HomeState,
        width: f32,
        events: &dyn Fn(&Painted) -> Vec<egui::Event>,
    ) -> Painted {
        let show = Show::new(Vec::new());
        let mut drawn = Painted::default();
        for round in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                events: if round == 2 { events(&drawn) } else { Vec::new() },
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| settings_ui(ui, state, &show));
            drawn = painted(&output);
            output.drop_without_applying_deltas();
        }
        drawn
    }

    /// The block of the key that says `words`.
    fn key_of(drawn: &Painted, words: &str) -> Option<Rect> {
        let (galley, at) = drawn.text(words)?;
        let middle = Rect::from_min_size(at, galley.size()).center();
        drawn.keys.iter().copied().find(|key| key.contains(middle))
    }

    /// What a frame drew but the words `except`, with where it is: words, blocks and lines.
    #[allow(clippy::type_complexity)]
    fn all_but(painted: &Painted, except: &str) -> (Vec<(String, Rect)>, Vec<Rect>, Vec<(f32, f32)>) {
        let texts = painted
            .texts
            .iter()
            .filter(|(galley, _)| galley.text() != except)
            .map(|(galley, at)| (galley.text().to_owned(), Rect::from_min_size(*at, galley.size())))
            .collect();
        (texts, painted.rects.clone(), painted.rules.clone())
    }

    #[test]
    fn the_line_before_a_search_holds_still_when_its_keys_are_pointed_at_pressed_and_clicked() {
        // Before a search, in every language, in the narrowest Settings window (820), where the
        // layout may change (940), its usual width (1040) and a wide one: the keys are under the
        // sentence at 820 and beside it at 1040 and 1280, Find at least the design's 84 points
        // wide; pointing at either key, pressing it and letting go (a click, which asks for what
        // it says) moves nothing on the page and changes the size of nothing.
        use crate::i18n::{Language, with_language};
        for language in Language::ALL {
            with_language(language, || {
                let choose = t("settings.find.choose_folder", "Choose a Folder\u{2026}");
                let find = t("settings.find.find", "Find");
                let label = t("settings.find.look", "Look for songs you already have");
                for width in [820.0, 940.0, 1040.0, 1280.0] {
                    let ctx = crate::studio::tests::styled();
                    let say = |what: &str| format!("{language:?} at {width}: {what}");
                    let mut state = state_in(Phase::Before);
                    let rest = idle_frame(&ctx, &mut state, width, &|_| Vec::new());
                    let sentence = looks_in_words(&[]);
                    let (sentence_galley, sentence_at) =
                        rest.text(&sentence).unwrap_or_else(|| panic!("{}", say("no sentence")));
                    let (label_galley, label_at) = rest.text(label).unwrap_or_else(|| panic!("{}", say("no label")));
                    let key = |drawn: &Painted, words: &str| {
                        key_of(drawn, words).unwrap_or_else(|| panic!("{}", say(&format!("no key under {words}"))))
                    };
                    let (choose_key, find_key) = (key(&rest, choose), key(&rest, find));
                    assert!(find_key.width() >= 84.0 - 0.01, "{}", say(&format!("Find is {} wide", find_key.width())));
                    assert!(choose_key.max.x < find_key.min.x, "{}", say("Choose a Folder\u{2026} comes before Find"));
                    assert!(
                        sentence_galley.rows.len() <= SENTENCE_LINES && !sentence_galley.elided,
                        "{}",
                        say(&format!("the sentence takes {} lines", sentence_galley.rows.len()))
                    );
                    let row_right = rest
                        .rules
                        .iter()
                        .filter(|(y, _)| *y <= label_at.y)
                        .max_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, right)| *right)
                        .expect("the line over the row");
                    let under = choose_key.min.y > sentence_at.y + sentence_galley.size().y;
                    if width == 820.0 {
                        assert!(under, "{}", say("the keys are beside the sentence"));
                    }
                    if width >= 1040.0 {
                        assert!(!under, "{}", say("the keys are under the sentence"));
                    }
                    if under {
                        // Under the sentence, at its left, 8 below its three lines' room,
                        // Choose a Folder… first; the sentence has the card's whole width.
                        let room_bottom = sentence_at.y + SENTENCE_LINES as f32 * 18.0;
                        assert!((choose_key.min.x - label_at.x).abs() < 0.5, "{}", say("the keys start at the left"));
                        assert!(
                            (choose_key.min.y - room_bottom - 8.0).abs() < 0.5,
                            "{}",
                            say(&format!("the keys are at {}, not 8 under the sentence", choose_key.min.y))
                        );
                        assert_eq!(find_key.min.y, choose_key.min.y, "{}", say("the keys share a line"));
                        assert!((find_key.min.x - choose_key.max.x - 8.0).abs() < 0.5, "{}", say("8 between the keys"));
                        assert!(
                            (sentence_galley.job.wrap.max_width - (row_right - label_at.x)).abs() < 0.5,
                            "{}",
                            say("the sentence has the card\u{2019}s whole width")
                        );
                    } else {
                        // At the right of the line, beside its words, Find last.
                        let words_right =
                            (sentence_at.x + sentence_galley.size().x).max(label_at.x + label_galley.size().x);
                        assert!(
                            words_right < choose_key.min.x,
                            "{}",
                            say("the keys are beside the line\u{2019}s words")
                        );
                        assert!(
                            (find_key.max.x - row_right).abs() < 0.5,
                            "{}",
                            say("Find ends at the row\u{2019}s right")
                        );
                        let words_rows = label_at.y..=sentence_at.y + sentence_galley.size().y;
                        assert!(
                            words_rows.contains(&find_key.center().y),
                            "{}",
                            say("the keys are on the line\u{2019}s height")
                        );
                    }
                    // Pointing at a key, pressing it and letting go moves nothing; the click asks.
                    for (words, asked) in [(choose, Ask::AnotherFolder), (find, Ask::Find)] {
                        let at = |drawn: &Painted| key(drawn, words).center();
                        let button = |drawn: &Painted, pressed| egui::Event::PointerButton {
                            pos: at(drawn),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        };
                        let pointed =
                            idle_frame(&ctx, &mut state, width, &|drawn| vec![egui::Event::PointerMoved(at(drawn))]);
                        let pressed = idle_frame(&ctx, &mut state, width, &|drawn| {
                            vec![egui::Event::PointerMoved(at(drawn)), button(drawn, true)]
                        });
                        let let_go = idle_frame(&ctx, &mut state, width, &|drawn| vec![button(drawn, false)]);
                        for (drawn, how) in [(&pointed, "pointed at"), (&pressed, "pressed"), (&let_go, "clicked")] {
                            assert!(drawn.same_place(&rest), "{}", say(&format!("{words} {how} moved something")));
                        }
                        assert_eq!(state.find_songs.asks, [asked], "{}", say(&format!("a click on {words}")));
                        state.find_songs.asks.clear();
                        idle_frame(&ctx, &mut state, width, &|_| vec![egui::Event::PointerGone]);
                    }
                }
            });
        }
    }

    #[test]
    fn a_folder_chosen_moves_nothing_and_the_sentence_is_never_cut_at_any_width() {
        // In every language, at every width from the narrowest Settings window (820) to 1280, 5
        // points at a time: where the keys are depends on the width alone (under the sentence up
        // to a width, beside it from there on); and a folder chosen, or several, changes only the
        // sentence, which keeps to three lines and is never cut at its end. One folder of an
        // ordinary name (28 letters or fewer, Latin or Cyrillic) comes whole; only a very long one
        // is cut, in its middle with "…". With two folders or more, it names the first two (each
        // whole, or cut in its middle to one letter at the least where the sentence would take
        // more) and counts the others.
        use crate::i18n::{Language, with_language};
        use std::path::PathBuf;
        // About as long as a folder's name can be (255 bytes).
        let very_long = "Worship songs 2019 from the old church on Elm Street, kept by the choir director before the move to the new building across town, with the youth group's songs, the Christmas cantata and the hymns the choir sang the year the roof was mended, all together";
        let ordinary = "Youth choir songs, 2019\u{2013}2020";
        let ordinary_cyrillic = "Хвала и поклонение 2018\u{2013}2019";
        assert_eq!((ordinary.chars().count(), ordinary_cyrillic.chars().count()), (28, 28));
        // Whole, cut, or either (several folders).
        #[derive(Clone, Copy, PartialEq)]
        enum Named {
            Whole,
            Cut,
            Either,
        }
        let cases: [(&[&str], Named); 9] = [
            (&["Choir"], Named::Whole),
            (&["Worship songs"], Named::Whole),
            (&[ordinary], Named::Whole),
            (&[ordinary_cyrillic], Named::Whole),
            (&[very_long], Named::Cut),
            (&["Choir", "Worship songs"], Named::Either),
            (&["Choir", "Worship songs", "Youth"], Named::Either),
            (&["Choir", "Worship songs", "Youth", "Christmas", "Easter"], Named::Either),
            (&[very_long, ordinary_cyrillic, "Youth", "Christmas", "Easter"], Named::Either),
        ];
        for language in Language::ALL {
            with_language(language, || {
                let label = t("settings.find.look", "Look for songs you already have");
                let choose = t("settings.find.choose_folder", "Choose a Folder\u{2026}");
                let sentence = looks_in_words(&[]);
                let mut beside_from = None;
                for width in (820..=1280).step_by(5).map(|width| width as f32) {
                    let ctx = crate::studio::tests::styled();
                    let say = |what: &str| format!("{language:?} at {width}: {what}");
                    let rest = idle_frame(&ctx, &mut state_in(Phase::Before), width, &|_| Vec::new());
                    let (sentence_galley, sentence_at) =
                        rest.text(&sentence).unwrap_or_else(|| panic!("{}", say("no sentence")));
                    let under = key_of(&rest, choose).expect("Choose a Folder\u{2026}").min.y
                        > sentence_at.y + sentence_galley.size().y;
                    if width == 820.0 {
                        // At the narrowest layout, the least a cut leaves still fits in the three
                        // lines' room: one letter of each of the two folders named, and the
                        // count of the other three, so the sentence never has to be cut at its
                        // end.
                        let least = looks_in_named(&[cut_middle(very_long, 1), cut_middle(ordinary_cyrillic, 1)], 3);
                        let room = sentence_galley.job.wrap.max_width;
                        let mut lines = usize::MAX;
                        ctx.run_ui(egui::RawInput::default(), |ui| lines = parts::note_lines(ui, &least, room))
                            .drop_without_applying_deltas();
                        assert!(lines <= SENTENCE_LINES, "{}", say(&format!("{lines} lines for {least}")));
                    }
                    match (under, beside_from) {
                        (false, None) => beside_from = Some(width),
                        (true, Some(from)) => {
                            panic!("{}", say(&format!("the keys are under again, beside from {from}")))
                        }
                        _ => {}
                    }
                    for (names, named) in cases {
                        let mut chosen = state_in(Phase::Before);
                        chosen.find_songs.added =
                            names.iter().map(|name| PathBuf::from("/Volumes/Old laptop").join(name)).collect();
                        let after = idle_frame(&ctx, &mut chosen, width, &|_| Vec::new());
                        let shown = after
                            .texts
                            .iter()
                            .find(|(galley, at)| *at == sentence_at && galley.text() != label)
                            .map(|(galley, _)| galley.clone())
                            .unwrap_or_else(|| panic!("{}", say(&format!("no sentence for {names:?}"))));
                        let said = shown.text();
                        assert!(
                            shown.rows.len() <= SENTENCE_LINES && !shown.elided,
                            "{}",
                            say(&format!("{} lines, or cut at its end, for {names:?}: {said}", shown.rows.len()))
                        );
                        let whole = looks_in_words(&chosen.find_songs.added);
                        match named {
                            Named::Whole => assert_eq!(said, whole, "{}", say(&format!("{names:?} cut"))),
                            Named::Cut => assert_ne!(said, whole, "{}", say(&format!("{names:?} not cut"))),
                            Named::Either => {}
                        }
                        if said != whole {
                            // The first two names, each cut in its middle to the same letters (one
                            // at the least), and the count of the others.
                            let first_two: Vec<&str> = names.iter().take(2).copied().collect();
                            let longest = first_two.iter().map(|name| name.chars().count()).max().unwrap_or(0);
                            let more = names.len().saturating_sub(2);
                            let cut = (1..longest).any(|keep| {
                                let cut: Vec<String> = first_two.iter().map(|name| cut_middle(name, keep)).collect();
                                said == looks_in_named(&cut, more)
                            });
                            assert!(cut, "{}", say(&format!("not the first two names cut in their middle: {said}")));
                        }
                        assert_eq!(
                            all_but(&after, said),
                            all_but(&rest, &sentence),
                            "{}",
                            say(&format!("something moved for {names:?}"))
                        );
                    }
                }
                assert!(beside_from.is_some_and(|from| from <= 1040.0), "{language:?}: beside from {beside_from:?}");
            });
        }
    }

    #[test]
    fn a_folders_name_is_cut_in_its_middle_by_letter_and_its_whole_path_is_under_the_pointer() {
        assert_eq!(cut_middle("Worship songs 2019", 18), "Worship songs 2019");
        assert_eq!(cut_middle("Worship songs 2019", 7), "Wors\u{2026}019");
        assert_eq!(cut_middle("Песни хора", 4), "Пе\u{2026}ра", "letters, not bytes");
        // Never a space beside "…".
        assert_eq!(cut_middle("Worship songs 2019", 10), "Worsh\u{2026}2019");
        assert_eq!(cut_middle("Worship songs 2019", 15), "Worship\u{2026}gs 2019");
        // A letter with its accent written as two characters stays one letter.
        assert_eq!(cut_middle("ab\u{301}cd", 3), "ab\u{301}\u{2026}d");
        assert_eq!(cut_middle("Worship songs 2019", 1), "W\u{2026}");
        let added = [std::path::PathBuf::from("/Volumes/Old laptop/Choir"), std::path::PathBuf::from("/Users/a/Youth")];
        assert_eq!(added_paths(&added), "/Volumes/Old laptop/Choir\n/Users/a/Youth");
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
