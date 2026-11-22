//! The Bible view's own parts: the left column (the books, chapters and verses to click, or the
//! passages that were on screen) and the page that reads the passage's chapter.
//!
//! Nothing here puts anything on screen but a click on a verse of the passage on the page, as a
//! click on one of its slides does. A book, chapter or verse clicked in the left column, or a
//! verse of the chapter outside the passage clicked on the page, is looked up and its first
//! verse picked, for Enter.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Color32, FontId, Galley, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_bible::{Bible, Range, VerseRef};
use sp_engine::{Command, Show, SlideRef};

use crate::bible::list_row_id;
use crate::home::{BiblePage, BibleSide, HomeState};
use crate::studio::{
    CARD, Column, Flat, HOVER, LEFT_WIDTH, ROW_HOVER, SELECTED, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, cap, column_frame,
    column_outer, column_panel, even_switch, fixed, fixed_semibold, flat_button, round_corners, semibold,
};

/// What a click among the books, chapters and verses asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// A book, by its place: its first chapter.
    Book(usize),
    /// A chapter of the book open.
    Chapter(u16),
    /// A verse of the chapter open: that verse alone, or with Shift the passage from the verse
    /// clicked before; a double-click takes the whole chapter with the verse picked.
    Verse { verse: u16, shift: bool, double: bool },
}

/// Looks up what was chosen, named as the first translation names it (the field then says it
/// in the operator's language), and returns the slide to pick for Enter when a passage was made.
/// Nothing goes on screen.
pub fn choose(page: &mut BiblePage, choice: Choice, live: Option<usize>) -> Option<usize> {
    let bible = page.lead().and_then(|lead| page.shelf.get(lead).ok())?;
    let mut pick = 0;
    let reference = match choice {
        Choice::Book(book) => {
            page.list_book = book;
            page.list_chapter = 1;
            page.list_from = None;
            format!("{} 1", bible.book_name(book))
        }
        Choice::Chapter(chapter) => {
            page.list_chapter = chapter;
            page.list_from = None;
            format!("{} {chapter}", bible.book_name(page.list_book))
        }
        Choice::Verse { verse, shift, double } => {
            let (book, chapter) = (page.list_book, page.list_chapter);
            let place = |verse| VerseRef { book, chapter, verse };
            if double {
                let (whole, slide) = crate::bible::in_its_chapter(&bible, place(verse));
                pick = if page.options.break_on_new_verse { slide } else { 0 };
                whole
            } else {
                let from = page.list_from.filter(|_| shift);
                if !shift {
                    page.list_from = Some(verse);
                }
                match from {
                    Some(from) => bible.label(place(from.min(verse)), place(from.max(verse))),
                    None => bible.label(place(verse), place(verse)),
                }
            }
        }
    };
    // A search's verses listed give way to the passage.
    page.found.clear();
    page.open(reference, live);
    page.pending.is_some().then_some(pick)
}

/// The passage now shown, as the first translation reads it.
fn shown_range(page: &mut BiblePage) -> Option<(Arc<Bible>, Range)> {
    let bible = page.lead().and_then(|lead| page.shelf.get(lead).ok())?;
    let range = sp_bible::parse_reference_in(&page.shown, &bible.info.language).ok()?;
    Some((bible, range))
}

/// The Bible view's left column, in place of the playlist: Books | History and Hide at its top,
/// then the books, chapters and verses, or the passages that were on screen.
pub fn left_column(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let frame = column_frame(Column::Left).inner_margin(egui::Margin { left: 10, right: 10, top: 12, bottom: 12 });
    let panel = egui::Panel::left("bible books")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(column_outer(LEFT_WIDTH))
        .frame(frame);
    let live = show.live().map(|at| at.presentation);
    let shown = panel.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let width = ui.available_width();
        let (bar, _) = ui.allocate_exact_size(vec2(width, 30.2), Sense::hover());
        const HIDE: f32 = 43.7;
        let track = Rect::from_min_max(bar.min, pos2(bar.max.x - HIDE - 6.0, bar.max.y));
        let side = match state.bible.side {
            BibleSide::Books => 0,
            BibleSide::History => 1,
        };
        let hints = ["The books, chapters and verses", "The passages that were on screen, newest first"];
        match even_switch(
            ui,
            "bible side",
            track,
            &["Books", "History"],
            &hints,
            Some(side),
            FontId::proportional(12.0),
        ) {
            Some(0) => state.bible.side = BibleSide::Books,
            Some(_) => state.bible.side = BibleSide::History,
            None => {}
        }
        let hide = Rect::from_min_size(pos2(bar.max.x - HIDE, bar.center().y - 12.0), vec2(HIDE, 24.0));
        if flat_button(ui, hide, list_row_id("hide"), "Hide", FontId::proportional(11.5), Flat::Plain, true)
            .on_hover_text("Put the books away; Books, beside the reference, brings them back")
            .clicked()
        {
            state.bible.lists_open = false;
        }
        let picked = match state.bible.side {
            BibleSide::Books => books(ui, &mut state.bible, live),
            BibleSide::History => {
                history(ui, &mut state.bible, live);
                None
            }
        };
        if let Some(slide) = picked {
            state.pick_made_verse(show, slide);
        }
    });
    round_corners(ui, column_panel(shown.response.rect, Column::Left));
}

/// The Books side: the heading, then the book, chapter and verse lists. Returns the slide to
/// pick when something was looked up.
fn books(ui: &mut egui::Ui, page: &mut BiblePage, live: Option<usize>) -> Option<usize> {
    let width = ui.available_width();
    let (head, _) = ui.allocate_exact_size(vec2(width, 35.0), Sense::hover());
    cap(ui, pos2(head.min.x, head.min.y + 16.4), Align2::LEFT_CENTER, "Book", TEXT_DIM);
    let Some(bible) = page.lead().and_then(|lead| page.shelf.get(lead).ok()) else {
        ui.add(
            egui::Label::new(egui::RichText::new("Choose a Bible for the first translation.").color(TEXT_DIM)).wrap(),
        );
        return None;
    };
    let choice = lists(ui, page, &bible);
    choice.and_then(|choice| choose(page, choice, live))
}

/// How tall a row of the lists is, and the room between rows.
const ROW: f32 = 28.0;
const ROW_GAP: f32 = 1.0;
/// How wide the chapter and verse lists are, and the room between the lists.
const NUMBERS: f32 = 46.0;
const LIST_GAP: f32 = 4.0;

/// What a row of the lists shows, and how.
#[derive(Clone, Copy)]
struct Row<'a> {
    text: &'a str,
    /// The one open (a book or chapter), or part of the passage shown (a verse).
    chosen: bool,
    /// The Bible has it; one it lacks is dimmed.
    has: bool,
    /// A number, set in the middle in the fixed-width face on a well.
    number: bool,
}

/// Draws one row of the lists in `rect` and returns its response: a book's name on the panel,
/// a number on a well; the chosen one a well with a faint inner ring and heavier words.
fn list_row(ui: &egui::Ui, id: egui::Id, rect: Rect, row: Row) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    let fill = match (row.chosen, row.number, hovered) {
        (true, _, _) => Some(CARD),
        (false, true, true) => Some(HOVER),
        (false, true, false) => Some(CARD),
        (false, false, true) => Some(ROW_HOVER),
        (false, false, false) => None,
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect, 7.0, fill);
    }
    if row.chosen {
        painter.rect_stroke(rect, 7.0, Stroke::new(1.0, SELECTED), StrokeKind::Inside);
    }
    let ink = if row.chosen || hovered {
        TEXT
    } else if row.has {
        TEXT_DIM
    } else {
        TEXT_FAINT
    };
    if row.number {
        let font = if row.chosen { fixed_semibold(13.0) } else { fixed(13.0) };
        painter.text(rect.center(), Align2::CENTER_CENTER, row.text, font, ink);
    } else {
        let font = if row.chosen { semibold(13.0) } else { FontId::proportional(13.0) };
        painter.with_clip_rect(rect.shrink2(vec2(8.0, 0.0))).text(
            pos2(rect.min.x + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            row.text,
            font,
            ink,
        );
    }
    response
}

/// A small heading in the lists: "Old Testament", "Ch".
fn list_heading(ui: &egui::Ui, at: egui::Pos2, anchor: Align2, text: &str) {
    crate::studio::paint_spaced(ui, at, anchor, &text.to_uppercase(), fixed_semibold(10.5), TEXT_FAINT, 0.8);
}

/// The book, chapter and verse lists, side by side and each scrolling on its own, opened on
/// what is chosen: the books named in full in the operator's language under the testaments,
/// the chapters and verses as the first translation numbers them. Returns what was clicked.
fn lists(ui: &mut egui::Ui, page: &mut BiblePage, bible: &Bible) -> Option<Choice> {
    let names = page.names_language();
    let shown = sp_bible::parse_reference_in(&page.shown, &bible.info.language).ok();
    let scroll = std::mem::take(&mut page.list_scroll);
    let shift = ui.input(|input| input.modifiers.shift);
    let area = ui.available_rect_before_wrap();
    let books_width = area.width() - 2.0 * NUMBERS - 2.0 * LIST_GAP;
    let column = |left: f32, width: f32| Rect::from_min_max(pos2(left, area.min.y), pos2(left + width, area.max.y));
    let book_column = column(area.min.x, books_width);
    let chapter_column = column(book_column.max.x + LIST_GAP, NUMBERS);
    let verse_column = column(chapter_column.max.x + LIST_GAP, NUMBERS);
    let mut clicked = None;
    let new_testament = sp_bible::BOOKS.iter().position(|book| book.code == "MAT").unwrap_or(39);
    let (open_book, open_chapter) = (page.list_book, page.list_chapter);

    // Books: every one, the ones this Bible lacks dimmed.
    in_column(ui, "bible book list", book_column, |ui| {
        let width = ui.available_width();
        for book in 0..sp_bible::BOOKS.len() {
            if book == 0 || book == new_testament {
                let room = if book == 0 { 18.0 } else { 28.0 };
                let heading = ui.allocate_space(vec2(width, room)).1;
                let words = if book == 0 { "Old Testament" } else { "New Testament" };
                list_heading(ui, pos2(heading.min.x + 8.0, heading.max.y - 4.0), Align2::LEFT_BOTTOM, words);
            }
            let rect = ui.allocate_space(vec2(width, ROW)).1;
            ui.add_space(ROW_GAP);
            let chosen = open_book == book;
            if chosen && scroll {
                ui.scroll_to_rect_animation(rect, Some(Align::Center), egui::style::ScrollAnimation::none());
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let name = sp_bible::book_name(book, &names);
            let row = Row { text: name, chosen, has: bible.chapter_count(book) > 0, number: false };
            if list_row(ui, list_row_id(("book", book)), rect, row).on_hover_text(name).clicked() {
                clicked = Some(Choice::Book(book));
            }
        }
    });
    // Chapters of the book open.
    let chapters = bible.chapter_count(open_book);
    numbers_column(ui, "bible chapter list", chapter_column, "Ch", |ui, width| {
        for chapter in 1..=chapters {
            let rect = ui.allocate_space(vec2(width, ROW)).1;
            ui.add_space(ROW_GAP);
            let chosen = open_chapter == chapter;
            if chosen && scroll {
                ui.scroll_to_rect_animation(rect, Some(Align::Center), egui::style::ScrollAnimation::none());
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let row = Row { text: &chapter.to_string(), chosen, has: true, number: true };
            if list_row(ui, list_row_id(("chapter", chapter)), rect, row).clicked() {
                clicked = Some(Choice::Chapter(chapter));
            }
        }
    });
    // Verses of the chapter open: Shift-click takes the passage from the verse clicked before,
    // a double-click the whole chapter with the verse picked.
    let verses =
        if open_chapter == 0 || open_chapter > chapters { 0 } else { bible.last_verse(open_book, open_chapter) };
    numbers_column(ui, "bible verse list", verse_column, "Verse", |ui, width| {
        for verse in 1..=verses {
            let rect = ui.allocate_space(vec2(width, ROW)).1;
            ui.add_space(ROW_GAP);
            let here = (open_book, open_chapter, verse);
            let chosen = shown.is_some_and(|range| {
                (range.start.book, range.start.chapter, range.start.verse) <= here
                    && here <= (range.end.book, range.end.chapter, range.end.verse)
            });
            if chosen && scroll {
                ui.scroll_to_rect_animation(rect, Some(Align::Center), egui::style::ScrollAnimation::none());
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let row = Row { text: &verse.to_string(), chosen, has: true, number: true };
            let response = list_row(ui, list_row_id(("verse", verse)), rect, row)
                .on_hover_text("Shift-click another to take the passage between them");
            if response.double_clicked() {
                clicked = Some(Choice::Verse { verse, shift: false, double: true });
            } else if response.clicked() {
                clicked = Some(Choice::Verse { verse, shift, double: false });
            }
        }
    });
    ui.allocate_rect(area, Sense::hover());
    clicked
}

/// One of the lists: a column of its own that scrolls.
fn in_column(ui: &mut egui::Ui, id: &str, rect: Rect, add: impl FnOnce(&mut egui::Ui)) {
    let mut column = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(Align::Min)));
    column.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).max_height(rect.height()).show(
        &mut column,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.set_width(rect.width());
            add(ui);
            ui.add_space(8.0);
        },
    );
}

/// The chapter or verse list: its heading kept over it, the numbers scrolling under.
fn numbers_column(ui: &mut egui::Ui, id: &str, rect: Rect, heading: &str, add: impl FnOnce(&mut egui::Ui, f32)) {
    list_heading(ui, pos2(rect.center().x, rect.min.y + 14.0), Align2::CENTER_BOTTOM, heading);
    let below = Rect::from_min_max(pos2(rect.min.x, rect.min.y + 18.0), rect.max);
    in_column(ui, id, below, |ui| {
        let width = ui.available_width();
        add(ui, width);
    });
}

/// The History side: the passages that were on screen, newest first, each named in the
/// operator's language; a click looks one up again (its first verse picked).
fn history(ui: &mut egui::Ui, page: &mut BiblePage, live: Option<usize>) {
    ui.add_space(14.0);
    if page.history.is_empty() {
        ui.add(
            egui::Label::new(
                egui::RichText::new("Passages that were on screen are listed here, newest first.")
                    .size(13.0)
                    .color(TEXT_DIM),
            )
            .wrap(),
        );
        return;
    }
    let lead = page.lead().and_then(|lead| page.shelf.get(lead).ok());
    let language = page.names_language();
    let today = chrono::Local::now().date_naive();
    let mut chosen = None;
    let mut clear = false;
    egui::ScrollArea::vertical().id_salt("bible history").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let width = ui.available_width();
        for (at, entry) in page.history.iter().enumerate() {
            let rect = ui.allocate_space(vec2(width, 34.0)).1;
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let response = ui.interact(rect, egui::Id::unique(("bible history row", at)), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 7.0, ROW_HOVER);
            }
            let when = ui.painter().text(
                pos2(rect.max.x - 10.0, rect.center().y),
                Align2::RIGHT_CENTER,
                crate::home::history_when(&entry.at, today),
                fixed(11.0),
                TEXT_FAINT,
            );
            let name = lead.as_ref().map_or_else(
                || entry.passage.clone(),
                |lead| crate::bible::name_for_operator(&entry.passage, lead, &language),
            );
            let clip = Rect::from_min_max(pos2(rect.min.x + 10.0, rect.min.y), pos2(when.min.x - 8.0, rect.max.y));
            let ink = if response.hovered() { TEXT } else { TEXT_DIM };
            ui.painter().with_clip_rect(clip).text(
                pos2(clip.min.x, rect.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(13.5),
                ink,
            );
            if response.clicked() {
                chosen = Some(entry.passage.clone());
            }
        }
        ui.add_space(10.0);
        let rect = ui.allocate_space(vec2(width, 30.0)).1;
        let wide = crate::studio::flat_button_width(ui, &["Clear history"], &FontId::proportional(12.5), 13.0);
        let button = Rect::from_min_size(rect.min, vec2(wide, 30.0));
        if flat_button(
            ui,
            button,
            egui::Id::unique("bible history clear"),
            "Clear history",
            FontId::proportional(12.5),
            Flat::Plain,
            true,
        )
        .clicked()
        {
            clear = true;
        }
    });
    if clear {
        page.history.clear();
        page.history_changed = true;
    }
    if let Some(passage) = chosen {
        // Kept as it was on screen; the field says it in the operator's language.
        page.found.clear();
        page.open(passage, live);
    }
}

/// One verse of the chapter on the page, laid out.
struct Line {
    verse: VerseRef,
    number: Arc<Galley>,
    words: Arc<Galley>,
    /// The same verse in the second Bible read, under it.
    under: Option<Arc<Galley>>,
    top: f32,
    height: f32,
    /// Its slide, when it is part of the passage shown: the place of its verse among those the
    /// passage was made from (the first slide, when the passage is one slide).
    slide: Option<usize>,
}

/// What the page was laid out from, so it is laid out again only when one of them changes.
#[derive(Clone, Debug, Default, PartialEq)]
struct Made {
    shown: String,
    choices: String,
    bibles: (usize, Option<usize>),
    width: i32,
}

/// The page's chapter laid out once for the passage, the Bibles read and the width, so a long
/// chapter costs nothing to draw again: only the verses in view are painted.
#[derive(Default)]
pub struct PageCache {
    made: Option<Made>,
    lines: Vec<Line>,
    height: f32,
    /// The chapter, as the first translation numbers it.
    chapter: Option<(usize, u16)>,
}

impl std::fmt::Debug for PageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageCache").field("made", &self.made).field("lines", &self.lines.len()).finish()
    }
}

/// The room round a verse on the page, and the column its number takes.
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 9.0;
const NUMBER_COLUMN: f32 = 30.0;
const NUMBER_GAP: f32 = 8.0;

/// The Bibles the page reads, by their places on the shelf: the one in the operator's language
/// first, then the first other Bible in use, if any.
pub fn page_bibles(page: &BiblePage) -> Option<(usize, Option<usize>)> {
    let (places, languages) = page.in_use();
    let language = page.names_language();
    let first = languages.iter().position(|have| *have == language).unwrap_or(0);
    let reader = *places.get(first)?;
    let second = places.iter().copied().find(|place| *place != reader);
    Some((reader, second))
}

/// Lays the chapter out for the page, when what it depends on has changed.
fn lay_out(ui: &egui::Ui, page: &mut BiblePage, width: f32) {
    let Some(bibles) = page_bibles(page) else {
        page.page.made = None;
        page.page.lines.clear();
        return;
    };
    let made = Made { shown: page.shown.clone(), choices: page.choices(), bibles, width: width.round() as i32 };
    if page.page.made.as_ref() == Some(&made) {
        return;
    }
    let break_on = page.options.break_on_new_verse;
    let mut lines = Vec::new();
    let mut chapter = None;
    let mut top = 0.0;
    if let (Some((lead, range)), Ok(reader)) = (shown_range(page), page.shelf.get(bibles.0)) {
        let second = bibles.1.and_then(|at| page.shelf.get(at).ok());
        let (book, number) = (range.start.book, range.start.chapter);
        chapter = Some((book, number));
        let whole = Range {
            start: VerseRef { book, chapter: number, verse: 1 },
            end: VerseRef { book, chapter: number, verse: sp_bible::LAST },
        };
        // The verses the passage's slides were made from, as `build_passage` makes them.
        let places: Vec<VerseRef> = lead.verses(range).into_iter().map(|(place, _)| place).collect();
        let text_width = (width - 2.0 * PAD_X - NUMBER_COLUMN - NUMBER_GAP).max(80.0);
        let layout = |text: &str, size: f32, line: f32| {
            let mut job = LayoutJob::single_section(
                text.to_owned(),
                TextFormat {
                    font_id: crate::studio::reading(size),
                    color: Color32::PLACEHOLDER,
                    line_height: Some(line),
                    ..TextFormat::default()
                },
            );
            job.wrap.max_width = text_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        for (verse, lead_words) in lead.verses(whole) {
            let in_reader = sp_bible::convert(verse, lead.info.numbering, reader.info.numbering);
            let words = layout(reader.verse(in_reader).unwrap_or(lead_words), 19.0, 28.5);
            let under = second.as_ref().and_then(|second| {
                let there = sp_bible::convert(verse, lead.info.numbering, second.info.numbering);
                second.verse(there).map(|text| layout(text, 15.0, 22.5))
            });
            let number = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(verse.verse.to_string(), fixed_semibold(12.0), Color32::PLACEHOLDER)
            });
            let body = words.size().y + under.as_ref().map_or(0.0, |under| 3.0 + under.size().y);
            let height = 2.0 * PAD_Y + body.max(number.size().y + 7.0);
            let slide = if break_on {
                places.iter().position(|place| *place == verse)
            } else {
                places.contains(&verse).then_some(0)
            };
            lines.push(Line { verse, number, words, under, top, height, slide });
            top += height;
        }
    }
    page.page = PageCache { made: Some(made), lines, height: top, chapter };
}

/// The words at the foot of the page.
const PAGE_NOTE: &str = "Click a verse of the passage to put it on screen; a verse outside it is looked up instead. \
     Space and the arrows go on through the passage.";

/// The page: the passage's chapter to read, a verse to a row with the same verse in the second
/// Bible under it, the passage's verses brighter. The verse on screen is tinted orange, the one
/// picked for Enter has a well and a ring. A click on a verse of the passage puts its slide on
/// screen, as a click on the slide does; a click on another verse of the chapter looks it up,
/// with Shift the passage from the verse clicked before, and picks it. Only the verses in view
/// are drawn.
pub fn page_view(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let live = show.live().map(|at| at.presentation);
    let width = ui.available_width();
    lay_out(ui, &mut state.bible, width);
    let slot = state.bible.slots.map(|slots| slots[state.bible.showing]);
    // The slides are there to put on screen only once the show holds the passage.
    let held = slot.filter(|slot| {
        show.presentations()
            .get(*slot)
            .is_some_and(|passage| passage.title == state.bible.shown && passage.slide_count() > 0)
    });
    let picked = state.pick(show).filter(|at| Some(at.presentation) == held).map(|at| at.slide);
    let on_screen = live_range(show, &mut state.bible);
    let chapter = state.bible.page.chapter;
    let mut clicked: Option<(VerseRef, Option<usize>)> = None;
    let shift = ui.input(|input| input.modifiers.shift);
    let follow_pick = state.pick_scroll > 0;
    let mut followed = false;
    let live_seen = show.live();
    let follow_live = live_seen != state.bible.page_live_seen;
    state.bible.page_live_seen = live_seen;
    let note_height = 72.0;
    egui::ScrollArea::vertical().id_salt("bible page").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let cache = &state.bible.page;
        ui.set_height(cache.height + note_height);
        let origin = ui.max_rect().min;
        let painter = ui.painter().clone();
        // The first verse in view, found by halving: rows differ in height.
        let first = cache.lines.partition_point(|line| line.top + line.height < viewport.min.y);
        let row_rect = |line: &Line| Rect::from_min_size(origin + vec2(0.0, line.top), vec2(width, line.height));
        for line in &cache.lines[first..] {
            if line.top > viewport.max.y {
                break;
            }
            let rect = row_rect(line);
            let response = ui.interact(rect, egui::Id::unique(("bible page verse", line.verse.verse)), Sense::click());
            let in_passage = line.slide.is_some();
            let is_live = on_screen.is_some_and(|range| range.start <= line.verse && line.verse <= range.end);
            let is_picked = picked.is_some() && line.slide.is_some() && line.slide == picked;
            if is_live {
                painter.rect_filled(rect, 10.0, TALLY.gamma_multiply(0.2));
            } else if is_picked {
                painter.rect_filled(rect, 10.0, CARD);
                painter.rect_stroke(rect, 10.0, Stroke::new(1.5, TEXT_DIM), StrokeKind::Inside);
            } else if response.hovered() {
                painter.rect_filled(rect, 10.0, ROW_HOVER);
            }
            let bright = in_passage || is_live || is_picked || response.hovered();
            let (number_ink, words_ink, under_ink) = match (is_live, bright) {
                (true, _) => (TALLY, TEXT, TEXT_DIM),
                (false, true) => (TEXT, TEXT, TEXT_DIM),
                (false, false) => (TEXT_FAINT, TEXT_DIM, TEXT_FAINT),
            };
            let left = rect.min.x + PAD_X;
            painter.galley(pos2(left, rect.min.y + PAD_Y + 7.0), line.number.clone(), number_ink);
            let text_left = left + NUMBER_COLUMN + NUMBER_GAP;
            painter.galley(pos2(text_left, rect.min.y + PAD_Y), line.words.clone(), words_ink);
            if let Some(under) = &line.under {
                let y = rect.min.y + PAD_Y + line.words.size().y + 3.0;
                painter.galley(pos2(text_left, y), under.clone(), under_ink);
            }
            if response.clicked() {
                clicked = Some((line.verse, line.slide));
            }
        }
        // The verse picked (after a lookup, ↑ or ↓) or the verse on screen kept in sight.
        let target = |want: &dyn Fn(&Line) -> bool| cache.lines.iter().find(|line| want(line)).map(row_rect);
        if follow_pick
            && let Some(rect) = target(&|line| picked.is_some() && line.slide == picked)
                .or_else(|| target(&|line| line.slide.is_some()))
        {
            ui.scroll_to_rect_animation(rect, Some(Align::Center), egui::style::ScrollAnimation::none());
            followed = true;
        } else if follow_live
            && let Some(range) = on_screen
            && let Some(rect) = target(&|line| range.start <= line.verse && line.verse <= range.end)
            && !viewport.translate(origin.to_vec2()).contains_rect(rect)
        {
            ui.scroll_to_rect_animation(rect, None, egui::style::ScrollAnimation::none());
        }
        if !cache.lines.is_empty() {
            let note = Rect::from_min_size(origin + vec2(PAD_X, cache.height + 18.0), vec2(width - 2.0 * PAD_X, 40.0));
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout(PAGE_NOTE.to_owned(), FontId::proportional(13.0), TEXT_FAINT, note.width())
            });
            painter.galley(note.min, galley, TEXT_FAINT);
        }
    });
    if followed {
        state.pick_scroll = 0;
    } else if follow_pick {
        state.pick_scroll -= 1;
        ui.ctx().request_repaint();
    }
    let Some((verse, slide)) = clicked else { return };
    match (slide, held) {
        // A verse of the passage: its slide on screen, as a click on the slide does.
        (Some(slide), Some(slot)) => {
            state.picked = None;
            commands.push(Command::GoTo(SlideRef { presentation: slot, slide }));
        }
        // Another verse of the chapter: looked up, and picked for Enter.
        _ => {
            if let Some((book, number)) = chapter {
                (state.bible.list_book, state.bible.list_chapter) = (book, number);
            }
            let choice = Choice::Verse { verse: verse.verse, shift, double: false };
            if let Some(slide) = choose(&mut state.bible, choice, live) {
                state.pick_made_verse(show, slide);
            }
        }
    }
}

/// The verses on screen, as the first translation numbers them, when what is on screen is a
/// Bible passage: read from the name of its slide (each Bible slide is named by its verses).
fn live_range(show: &Show, page: &mut BiblePage) -> Option<Range> {
    let live = show.live()?;
    page.slots.filter(|slots| slots.contains(&live.presentation))?;
    let (group, _) = show.presentations().get(live.presentation)?.slide(live.slide)?;
    let lead = page.lead().and_then(|lead| page.shelf.get(lead).ok())?;
    sp_bible::parse_reference_in(&group.name, &lead.info.language).ok()
}

/// The title over the page or the slides: the chapter (on the page) or the passage (over its
/// slides), in the operator's language, and the short names of the Bibles read.
pub fn title(page: &mut BiblePage, slides: bool) -> Option<(String, String)> {
    let (_, range) = shown_range(page)?;
    let words = if slides {
        page.reading.clone()
    } else {
        format!("{} {}", sp_bible::book_name(range.start.book, &page.names_language()), range.start.chapter)
    };
    let (reader, second) = page_bibles(page)?;
    let short = |at: usize| page.shelf.bibles.get(at).map_or(String::new(), |bible| bible.short.clone());
    let pill = match second {
        Some(second) => format!("{} + {}", short(reader), short(second)),
        None => short(reader),
    };
    Some((words, pill))
}

/// Draws the title line: the words large, then the pill of the Bibles read.
pub fn paint_title(ui: &mut egui::Ui, words: &str, pill: &str) {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(vec2(width, 55.0), Sense::hover());
    let middle = row.min.y + 10.0 + 17.5;
    let galley = crate::studio::spaced(ui, words, semibold(26.0), TEXT, -0.8);
    let title = Align2::LEFT_CENTER.anchor_size(pos2(row.min.x, middle), galley.size());
    ui.painter().with_clip_rect(row).galley(title.min, galley, TEXT);
    if !pill.is_empty() {
        let galley = crate::studio::spaced(ui, &pill.to_uppercase(), fixed_semibold(11.0), TEXT_DIM, 0.7);
        let pill_rect =
            Rect::from_min_size(pos2(title.max.x + 12.0, middle - 10.5), vec2(galley.size().x + 18.0, 21.0));
        ui.painter().rect_filled(pill_rect, 10.5, CARD);
        ui.painter().galley(pos2(pill_rect.min.x + 9.0, middle - galley.size().y / 2.0), galley, TEXT_DIM);
    }
}

/// Verses found by words (typed in the reference field, or by `sp bible search`), in place of
/// the page: each opens as its verse with a click (picked for Enter), or as its whole chapter
/// with the verse picked. Nothing goes on screen.
pub fn found_list(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let live = show.live().map(|at| at.presentation);
    let mut picked: Option<Option<(usize, bool)>> = None;
    let mixed = state.bible.found.iter().any(|found| found.bible != state.bible.found[0].bible);
    let count = state.bible.found.len();
    let words = if count == 1 { "1 verse".to_owned() } else { format!("{count} verses") };
    let width = ui.available_width();
    egui::ScrollArea::vertical().id_salt("bible found").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        let (head, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
        let said = cap(ui, pos2(head.min.x + PAD_X, head.center().y), Align2::LEFT_CENTER, &words, TEXT_DIM);
        let close = Rect::from_min_size(pos2(said.max.x + 12.0, head.center().y - 12.0), vec2(52.0, 24.0));
        if flat_button(
            ui,
            close,
            egui::Id::unique("bible found close"),
            "Close",
            FontId::proportional(11.5),
            Flat::Plain,
            true,
        )
        .on_hover_text("Back to the passage")
        .clicked()
        {
            picked = Some(None);
        }
        for (at, found) in state.bible.found.iter().enumerate() {
            let reference =
                if mixed { format!("{} \u{b7} {}", found.reference, found.bible) } else { found.reference.clone() };
            let (row, response) = ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
            let chapter_button =
                Rect::from_min_size(pos2(row.max.x - PAD_X - 104.0, row.center().y - 12.0), vec2(104.0, 24.0));
            if response.hovered() {
                ui.painter().rect_filled(row, 10.0, ROW_HOVER);
            }
            let left = row.min.x + PAD_X;
            ui.painter().with_clip_rect(row).text(
                pos2(left, row.min.y + 15.0),
                Align2::LEFT_CENTER,
                &reference,
                fixed_semibold(12.0),
                TEXT,
            );
            let clip = Rect::from_min_max(pos2(left, row.min.y), pos2(chapter_button.min.x - 10.0, row.max.y));
            ui.painter().with_clip_rect(clip).text(
                pos2(left, row.min.y + 34.0),
                Align2::LEFT_CENTER,
                &found.words,
                crate::studio::reading(15.0),
                if response.hovered() { TEXT } else { TEXT_DIM },
            );
            let chapter = flat_button(
                ui,
                chapter_button,
                egui::Id::unique(("bible found chapter", at)),
                "Whole chapter",
                FontId::proportional(11.5),
                Flat::Plain,
                true,
            )
            .on_hover_text("Open its chapter, with this verse picked");
            if chapter.clicked() {
                picked = Some(Some((at, true)));
            } else if response.on_hover_text("Open this verse, picked for Enter").clicked() {
                picked = Some(Some((at, false)));
            }
        }
    });
    match picked {
        Some(None) => state.bible.found.clear(),
        // The verse, or its chapter, with the verse picked for Enter.
        Some(Some((at, chapter))) => {
            if let Some(found) = state.bible.found.get(at).cloned() {
                state.bible.open_found(&found, chapter, live);
                let lead = state.bible.lead().and_then(|lead| state.bible.shelf.get(lead).ok());
                let place = lead.as_ref().and_then(|lead| {
                    let verse = sp_bible::parse_reference_in(&found.kept, &lead.info.language).ok()?.start;
                    Some(crate::bible::in_its_chapter(lead, verse).1)
                });
                let slide = place.filter(|_| chapter && state.bible.options.break_on_new_verse).unwrap_or(0);
                state.pick_made_verse(show, slide);
            }
        }
        None => {}
    }
}
