//! The Bible view's own parts: the left column (the books, chapters and verses to click, or the
//! passages that were on screen) and the page that reads the passage's chapter.
//!
//! Nothing here puts anything on screen but a click on a verse on the page that has a slide (any
//! verse of the chapter, with a slide a verse; the passage, made into one slide), as a click on
//! the slide does. A book, chapter or verse clicked in the left column, or a verse on the page
//! with no slide, is looked up and its first verse picked, for Enter.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Color32, FontId, Galley, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_bible::{Bible, Range, VerseRef};
use sp_engine::{Command, Show, SlideRef};

use crate::bible::list_row_id;
use crate::home::{BiblePage, BibleSide, BookPicker, HomeState};
use crate::icons::Icon;
use crate::studio::{
    BUTTON, BUTTON_HOVER, CARD, Column, Flat, HOVER, LEFT_WIDTH, ROW_HOVER, SELECTED, TALLY, TEXT, TEXT_DIM,
    TEXT_FAINT, cap, column_frame, column_outer, column_panel, even_switch, fixed, fixed_semibold, flat_button,
    round_corners, semibold,
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
/// in the operator's language), and returns the slide to pick for Enter when a passage was made:
/// the passage's first verse, or the verse double-clicked. A book or chapter is the whole
/// chapter; a verse is that verse alone (its slides its chapter's, with a slide a verse). Nothing
/// goes on screen.
pub fn choose(page: &mut BiblePage, choice: Choice, live: Option<usize>) -> Option<usize> {
    let bible = page.lead().and_then(|lead| page.shelf.get(lead).ok())?;
    let mut pick = None;
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
                pick = Some(if page.options.break_on_new_verse { slide } else { 0 });
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
    page.pending.is_some().then(|| pick.unwrap_or(page.passage_slides().start))
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
            BibleSide::History => history(ui, &mut state.bible, live),
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
    cap(ui, pos2(head.min.x + 8.0, head.min.y + 16.4), Align2::LEFT_CENTER, "Book", TEXT_DIM);
    // Keypad | Lists, at the right of the heading, where it stays whichever is chosen.
    let at = pos2(head.max.x - 60.0, head.min.y + 16.4 - 12.0);
    let chosen = match page.books {
        BookPicker::Keypad => 0,
        BookPicker::Lists => 1,
    };
    let hints = ["Every book as a key, then the chapters and verses", "Books, chapters and verses as lists"];
    match crate::studio::icon_switch(ui, "bible books as", at, &[Icon::Grid, Icon::Rows], &hints, Some(chosen)).1 {
        Some(0) => page.books = BookPicker::Keypad,
        Some(_) => page.books = BookPicker::Lists,
        None => {}
    }
    if chosen != usize::from(page.books == BookPicker::Lists) {
        // Opened on what is chosen.
        page.list_scroll = true;
    }
    let Some(bible) = page.lead().and_then(|lead| page.shelf.get(lead).ok()) else {
        ui.add(
            egui::Label::new(egui::RichText::new("Choose a Bible for the first translation.").color(TEXT_DIM)).wrap(),
        );
        return None;
    };
    let choice = match page.books {
        BookPicker::Keypad => keypad(ui, page, &bible),
        BookPicker::Lists => lists(ui, page, &bible),
    };
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
        // A name too long for the row ends in an ellipsis (its tooltip has it whole).
        let font = if row.chosen { semibold(13.0) } else { FontId::proportional(13.0) };
        let galley = one_line(ui, row.text, font, ink, rect.width() - 16.0);
        painter.galley(pos2(rect.min.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    }
    response
}

/// Words on one line no wider than `width`, ending in an ellipsis when they are longer.
fn one_line(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, ink));
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// A small heading in the lists: "Old Testament", "Ch". 11 points, the smallest the design
/// sets words in.
fn list_heading(ui: &egui::Ui, at: egui::Pos2, anchor: Align2, text: &str) {
    crate::studio::paint_spaced(ui, at, anchor, &text.to_uppercase(), fixed_semibold(11.0), TEXT_FAINT, 0.8);
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

/// A book's key on the keypad, in the operator's language: in English its code with the first
/// letter capital (`Gen`, `1sa`, `Joh`); in Russian or Ukrainian the first short form the
/// reference reader knows that has two letters or more, fits four places and begins like a
/// word of the book's name (`Быт`, `1Цар`, `Мф`, `Нав`), else the first that fits, else the
/// first short form cut to four places, else the name's start.
pub fn book_key(book: usize, language: &str) -> String {
    let entry = &sp_bible::BOOKS[book.min(sp_bible::BOOKS.len() - 1)];
    let (name, shorts) = match language {
        "ru" => (entry.russian, entry.russian_short),
        "uk" => (entry.ukrainian, entry.ukrainian_short),
        _ => {
            let mut code = entry.code.chars();
            return code.next().into_iter().chain(code.flat_map(char::to_lowercase)).collect();
        }
    };
    let capital = |short: &str| -> String {
        let mut seen = false;
        short
            .chars()
            .flat_map(|c| {
                let first = !seen && c.is_alphabetic();
                seen |= first;
                if first { c.to_uppercase().collect::<Vec<_>>() } else { vec![c] }
            })
            .collect()
    };
    let letters = |short: &str| short.chars().filter(|c| c.is_alphabetic()).count();
    let fits = |short: &str| letters(short) >= 2 && short.chars().count() <= 4;
    // The words of the name that say which book it is: not "От", "К", "Від" or "До".
    let joins = ["от", "к", "від", "до"];
    let initials: Vec<char> = name
        .split_whitespace()
        .filter(|word| !joins.contains(&word.to_lowercase().as_str()))
        .filter_map(|word| word.chars().find(|c| c.is_alphabetic()))
        .flat_map(char::to_lowercase)
        .collect();
    let begins_a_word = |short: &str| short.chars().find(|c| c.is_alphabetic()).is_some_and(|c| initials.contains(&c));
    let fitting = || shorts.iter().copied().filter(|short| fits(short));
    if let Some(short) = fitting().find(|short| begins_a_word(short)).or_else(|| fitting().next()) {
        return capital(short);
    }
    if let Some(short) = shorts.first() {
        return capital(&short.chars().take(4).collect::<String>());
    }
    if name.chars().count() <= 4 && !name.contains(' ') {
        return name.to_owned();
    }
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    let word = name.split_whitespace().rfind(|word| !joins.contains(&word.to_lowercase().as_str())).unwrap_or(name);
    format!("{digits}{}", word.chars().take(4 - digits.chars().count().min(3)).collect::<String>())
}

/// How many keys stand in a row of the keypad, how tall a key is, and the room between keys.
const KEYS_ACROSS: usize = 6;
const KEY_HEIGHT: f32 = 30.0;
const KEY_GAP: f32 = 4.0;

/// How a key of the keypad is drawn: the button grey with its words in ink when the Bible has
/// what it stands for, darker with faint words when not; the one chosen in the lighter grey of
/// every choice that is on, with its faint inner edge.
fn key(ui: &egui::Ui, id: egui::Id, rect: Rect, text: &str, has: bool, chosen: bool) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    let down = response.is_pointer_button_down_on();
    // Pressed, it gives a little, as a flat key does.
    let face = if down { rect.shrink(0.6) } else { rect };
    if chosen {
        crate::studio::paint_flat(painter, face, 6.0, Flat::On, response.hovered());
    } else {
        let fill = match (has, response.hovered()) {
            (true, true) => BUTTON_HOVER,
            (true, false) => BUTTON,
            (false, true) => BUTTON,
            (false, false) => ROW_HOVER,
        };
        painter.rect_filled(face, 6.0, fill);
    }
    let ink = if has || response.hovered() { TEXT } else { TEXT_FAINT };
    painter.text(face.center(), Align2::CENTER_CENTER, text, fixed_semibold(11.0), ink);
    response
}

/// A block of keys, six to a row, `count` of them at the next free place; `each(index, rect)`
/// draws the one at `index` when it is in view. The key at `bring` is scrolled into view, in
/// view or not.
fn key_grid(ui: &mut egui::Ui, count: usize, bring: Option<usize>, mut each: impl FnMut(&mut egui::Ui, usize, Rect)) {
    let width = ui.available_width();
    let wide = (width - KEY_GAP * (KEYS_ACROSS - 1) as f32) / KEYS_ACROSS as f32;
    let rows = count.div_ceil(KEYS_ACROSS);
    let height = rows as f32 * KEY_HEIGHT + rows.saturating_sub(1) as f32 * KEY_GAP;
    let block = ui.allocate_space(vec2(width, height)).1;
    for index in 0..count {
        let (row, column) = (index / KEYS_ACROSS, index % KEYS_ACROSS);
        let corner = block.min + vec2(column as f32 * (wide + KEY_GAP), row as f32 * (KEY_HEIGHT + KEY_GAP));
        let rect = Rect::from_min_size(corner, vec2(wide, KEY_HEIGHT));
        if bring == Some(index) {
            ui.scroll_to_rect_animation(rect, None, egui::style::ScrollAnimation::none());
        }
        if ui.is_rect_visible(rect) {
            each(ui, index, rect);
        }
    }
}

/// The keypad: every book as a key (the Old Testament, a little room, the New), then the
/// chapters of the book open, then the verses of the chapter open, all one click away. Books
/// are named in the operator's language; chapters and verses are the first translation's.
/// Returns what was clicked.
fn keypad(ui: &mut egui::Ui, page: &mut BiblePage, bible: &Bible) -> Option<Choice> {
    let names = page.names_language();
    let shown = sp_bible::parse_reference_in(&page.shown, &bible.info.language).ok();
    let scroll = std::mem::take(&mut page.list_scroll);
    let shift = ui.input(|input| input.modifiers.shift);
    let (open_book, open_chapter) = (page.list_book, page.list_chapter);
    let new_testament = sp_bible::BOOKS.iter().position(|book| book.code == "MAT").unwrap_or(39);
    let mut clicked = None;
    egui::ScrollArea::vertical().id_salt("bible keypad").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::Frame::new().inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 14 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let keys = book_keys(&names);
            for (testament, books) in [(0, 0..new_testament), (1, new_testament..sp_bible::BOOKS.len())] {
                if testament == 1 {
                    ui.add_space(KEY_GAP + 5.0 + KEY_GAP);
                }
                let first = books.start;
                key_grid(ui, books.len(), None, |ui, at, rect| {
                    let book = first + at;
                    let name = sp_bible::book_name(book, &names);
                    let response = key(
                        ui,
                        list_row_id(("book", book)),
                        rect,
                        &keys[book],
                        bible.chapter_count(book) > 0,
                        book == open_book,
                    );
                    if response.on_hover_text(name).clicked() {
                        clicked = Some(Choice::Book(book));
                    }
                });
            }
            // The chapters of the book open.
            let chapters = bible.chapter_count(open_book);
            section_heading(ui, &format!("{} \u{b7} chapter", sp_bible::book_name(open_book, &names)), None);
            // Opened on the chapter chosen.
            let bring = (scroll && open_chapter > 0).then(|| usize::from(open_chapter) - 1);
            key_grid(ui, usize::from(chapters), bring, |ui, at, rect| {
                let chapter = at as u16 + 1;
                let chosen = chapter == open_chapter;
                if key(ui, list_row_id(("chapter", chapter)), rect, &chapter.to_string(), true, chosen).clicked() {
                    clicked = Some(Choice::Chapter(chapter));
                }
            });
            // The verses of the chapter open: Shift takes the passage from the verse clicked
            // before; a double-click the whole chapter with the verse picked.
            let verses = if open_chapter == 0 || open_chapter > chapters {
                0
            } else {
                bible.last_verse(open_book, open_chapter)
            };
            section_heading(ui, "Verse", Some(" \u{b7} \u{21e7} for a passage"));
            key_grid(ui, usize::from(verses), None, |ui, at, rect| {
                let verse = at as u16 + 1;
                let here = (open_book, open_chapter, verse);
                let chosen = shown.is_some_and(|range| {
                    (range.start.book, range.start.chapter, range.start.verse) <= here
                        && here <= (range.end.book, range.end.chapter, range.end.verse)
                });
                let response = key(ui, list_row_id(("verse", verse)), rect, &verse.to_string(), true, chosen)
                    .on_hover_text("Click a verse; Shift-click another to take the passage between them");
                if response.double_clicked() {
                    clicked = Some(Choice::Verse { verse, shift: false, double: true });
                } else if response.clicked() {
                    clicked = Some(Choice::Verse { verse, shift, double: false });
                }
            });
        });
    });
    clicked
}

/// A section's heading in the keypad, in small capitals with room above it: "Psalms ·
/// chapter", and a fainter note after it.
fn section_heading(ui: &mut egui::Ui, text: &str, note: Option<&str>) {
    let width = ui.available_width();
    ui.add_space(14.0);
    let (row, _) = ui.allocate_exact_size(vec2(width, 23.5), Sense::hover());
    let said = cap(ui, pos2(row.min.x, row.min.y + 7.75), Align2::LEFT_CENTER, text, TEXT_DIM);
    if let Some(note) = note {
        let clip = Rect::from_min_max(pos2(said.max.x, row.min.y), row.max);
        let galley = crate::studio::spaced(ui, &note.to_uppercase(), fixed_semibold(11.5), TEXT_FAINT, 0.2);
        ui.painter().with_clip_rect(clip).galley(
            pos2(said.max.x, said.center().y - galley.size().y / 2.0),
            galley,
            TEXT_FAINT,
        );
    }
}

/// Every book's key in a language, worked out once for each language.
fn book_keys(language: &str) -> &'static [String] {
    static KEYS: std::sync::OnceLock<[Vec<String>; 3]> = std::sync::OnceLock::new();
    let all = KEYS.get_or_init(|| {
        ["en", "ru", "uk"].map(|language| (0..sp_bible::BOOKS.len()).map(|book| book_key(book, language)).collect())
    });
    match language {
        "ru" => &all[1],
        "uk" => &all[2],
        _ => &all[0],
    }
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
/// operator's language; a click looks one up again, as the page makes any passage. Returns the
/// slide to pick (its first verse) when one was looked up.
fn history(ui: &mut egui::Ui, page: &mut BiblePage, live: Option<usize>) -> Option<usize> {
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
        return None;
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
            let ink = if response.hovered() { TEXT } else { TEXT_DIM };
            let galley = one_line(ui, &name, FontId::proportional(13.5), ink, when.min.x - 8.0 - rect.min.x - 10.0);
            ui.painter().galley(pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, ink);
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
    let passage = chosen?;
    // Kept as it was on screen; the field says it in the operator's language.
    page.found.clear();
    page.open(passage, live);
    page.pending.is_some().then(|| page.passage_slides().start)
}

/// One verse of the chapter on the page: its words, where it stands and how tall it is. Its
/// text is laid out again when it is drawn, through the toolkit's own cache: a laid-out text
/// points into the font atlas, which the toolkit may clear between frames.
struct Line {
    verse: VerseRef,
    words: String,
    /// The same verse in the second Bible read, under it, and how tall the first Bible's words
    /// are, so the second's can be put under them.
    under: Option<String>,
    words_height: f32,
    top: f32,
    height: f32,
    /// Its slide, when the slides made for the passage have one: the place of its verse among
    /// those they were made from (its chapter's, with a slide a verse), or the passage's one
    /// slide when the passage is one slide.
    slide: Option<usize>,
    /// Part of the passage asked for: lit.
    asked: bool,
}

/// What the page was laid out from, so it is laid out again only when one of them changes.
#[derive(Clone, Debug, Default, PartialEq)]
struct Made {
    shown: String,
    deck: String,
    choices: String,
    bibles: (usize, Option<usize>),
    width: i32,
    /// The display's scale: the text is measured again on a display of another density.
    pixels_per_point: i32,
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
    /// How many slides were made for the passage, as the page worked them out: a slide index is
    /// put on screen only while the place holds that many.
    slides: usize,
    /// Every verse of the chapter has a slide (made a slide a verse).
    every_verse: bool,
}

#[cfg(test)]
impl PageCache {
    /// The verses lit as the passage, and those with a slide, as last laid out.
    pub fn verses(&self) -> (Vec<u16>, Vec<u16>) {
        let lit = self.lines.iter().filter(|line| line.asked).map(|line| line.verse.verse).collect();
        let with_slides = self.lines.iter().filter(|line| line.slide.is_some()).map(|line| line.verse.verse).collect();
        (lit, with_slides)
    }
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
    let pixels_per_point = (ui.ctx().pixels_per_point() * 100.0).round() as i32;
    let held = page.held[page.showing].clone();
    let made = Made {
        shown: page.shown.clone(),
        deck: held.deck.clone(),
        choices: page.choices(),
        bibles,
        width: width.round() as i32,
        pixels_per_point,
    };
    if page.page.made.as_ref() == Some(&made) {
        return;
    }
    let break_on = page.options.break_on_new_verse;
    let mut lines = Vec::new();
    let mut chapter = None;
    let mut slides = 0;
    let mut top = 0.0;
    if let (Some((lead, range)), Ok(reader)) = (shown_range(page), page.shelf.get(bibles.0)) {
        let second = bibles.1.and_then(|at| page.shelf.get(at).ok());
        let (book, number) = (range.start.book, range.start.chapter);
        chapter = Some((book, number));
        let whole = Range {
            start: VerseRef { book, chapter: number, verse: 1 },
            end: VerseRef { book, chapter: number, verse: sp_bible::LAST },
        };
        // The verses the slides were made from, as `build_passage` makes them: the chapter's
        // round the passage (see `bible::page_deck`), or the passage's own.
        let deck = Some(&held)
            .filter(|held| held.passage == page.shown)
            .and_then(|held| sp_bible::parse_reference_in(&held.deck, &lead.info.language).ok())
            .unwrap_or(range);
        let places: Vec<VerseRef> = lead.verses(deck).into_iter().map(|(place, _)| place).collect();
        slides = if break_on { places.len() } else { usize::from(!places.is_empty()) };
        let text_width = page_text_width(width);
        let number = ui.fonts_mut(|fonts| fonts.layout_no_wrap("0".to_owned(), fixed_semibold(12.0), TEXT)).size().y;
        for (verse, lead_words) in lead.verses(whole) {
            let in_reader = sp_bible::convert(verse, lead.info.numbering, reader.info.numbering);
            let words = reader.verse(in_reader).unwrap_or(lead_words).to_owned();
            let under = second.as_ref().and_then(|second| {
                let there = sp_bible::convert(verse, lead.info.numbering, second.info.numbering);
                second.verse(there).map(str::to_owned)
            });
            let words_height = page_text(ui, &words, false, text_width).size().y;
            let under_height =
                under.as_ref().map_or(0.0, |under| 3.0 + page_text(ui, under, true, text_width).size().y);
            let height = 2.0 * PAD_Y + (words_height + under_height).max(number + 7.0);
            let slide = if break_on {
                places.iter().position(|place| *place == verse)
            } else {
                places.contains(&verse).then_some(0)
            };
            let asked = range.start <= verse && verse <= range.end;
            lines.push(Line { verse, words, under, words_height, top, height, slide, asked });
            top += height;
        }
    }
    let every_verse = break_on && !lines.is_empty() && lines.iter().all(|line| line.slide.is_some());
    page.page = PageCache { made: Some(made), lines, height: top, chapter, slides, every_verse };
}

/// How wide a verse's words are on a page `width` wide.
fn page_text_width(width: f32) -> f32 {
    (width - 2.0 * PAD_X - NUMBER_COLUMN - NUMBER_GAP).max(80.0)
}

/// A verse's words laid out in the book face, wrapped to `width`: the first Bible's large, the
/// second's (`under`) smaller. Through the toolkit's cache, so a verse in view costs a lookup.
fn page_text(ui: &egui::Ui, text: &str, under: bool, width: f32) -> Arc<Galley> {
    let (size, line) = if under { (15.0, 22.5) } else { (19.0, 28.5) };
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: crate::studio::reading(size),
            color: Color32::PLACEHOLDER,
            line_height: Some(line),
            ..TextFormat::default()
        },
    );
    job.wrap.max_width = width;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// The words at the foot of the page: with a slide for every verse of the chapter, or not.
const PAGE_NOTE: &str = "Click a verse to put it on screen. Space and the arrows go on through the chapter.";
const PASSAGE_NOTE: &str = "Click the passage to put it on screen; a verse outside it is looked up instead.";

/// The page: the passage's chapter to read, a verse to a row with the same verse in the second
/// Bible under it, the passage's verses brighter. The verse on screen is tinted orange, the one
/// picked for Enter has a well and a ring. A click on a verse with a slide (any verse of the
/// chapter, with a slide a verse; the passage, made into one slide) puts that slide on screen,
/// as a click on the slide does; a click on a verse with none looks it up, with Shift the
/// passage from the verse clicked before, and picks it. Only the verses in view are drawn.
pub fn page_view(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let live = show.live().map(|at| at.presentation);
    let width = ui.available_width();
    lay_out(ui, &mut state.bible, width);
    let slot = state.bible.slots.map(|slots| slots[state.bible.showing]);
    // The slides are there to put on screen only once the show holds those made for the
    // passage, as the page worked them out (so an index can never put up another verse).
    let slides = state.bible.page.slides;
    let deck = &state.bible.held[state.bible.showing].deck;
    let held = slot.filter(|slot| {
        show.presentations().get(*slot).is_some_and(|passage| {
            passage.title == *deck && passage.slide_count() > 0 && passage.slide_count() == slides
        })
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
            let in_passage = line.asked;
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
            painter.text(
                pos2(left, rect.min.y + PAD_Y + 7.0),
                Align2::LEFT_TOP,
                line.verse.verse,
                fixed_semibold(12.0),
                number_ink,
            );
            let text_left = left + NUMBER_COLUMN + NUMBER_GAP;
            let text_width = page_text_width(width);
            painter.galley(
                pos2(text_left, rect.min.y + PAD_Y),
                page_text(ui, &line.words, false, text_width),
                words_ink,
            );
            if let Some(under) = &line.under {
                let y = rect.min.y + PAD_Y + line.words_height + 3.0;
                painter.galley(pos2(text_left, y), page_text(ui, under, true, text_width), under_ink);
            }
            if response.clicked() {
                clicked = Some((line.verse, line.slide));
            }
        }
        // The verse picked (after a lookup, ↑ or ↓) or the verse on screen kept in sight.
        let target = |want: &dyn Fn(&Line) -> bool| cache.lines.iter().find(|line| want(line)).map(row_rect);
        if follow_pick
            && let Some(rect) =
                target(&|line| picked.is_some() && line.slide == picked).or_else(|| target(&|line| line.asked))
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
            let words = if cache.every_verse { PAGE_NOTE } else { PASSAGE_NOTE };
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout(words.to_owned(), FontId::proportional(13.0), TEXT_FAINT, note.width())
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
        // A verse with a slide: its slide on screen, as a click on the slide does. The passage
        // stays as it is.
        (Some(slide), Some(slot)) => {
            state.picked = None;
            commands.push(Command::GoTo(SlideRef { presentation: slot, slide }));
        }
        // A verse with none: looked up, and picked for Enter.
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
/// Bible passage: read from the name of its slide (each Bible slide is named by its verses,
/// with the Bible's short name after them in brackets when Options asks for it).
fn live_range(show: &Show, page: &mut BiblePage) -> Option<Range> {
    let live = show.live()?;
    page.slots.filter(|slots| slots.contains(&live.presentation))?;
    let (group, _) = show.presentations().get(live.presentation)?.slide(live.slide)?;
    let lead = page.lead().and_then(|lead| page.shelf.get(lead).ok())?;
    sp_bible::parse_reference_in(verses_named(&group.name), &lead.info.language).ok()
}

/// The verses a Bible slide's name gives: "John 3:16 (KJV)" is John 3:16.
fn verses_named(name: &str) -> &str {
    match name.strip_suffix(')').and_then(|name| name.rsplit_once(" (")) {
        Some((verses, _)) => verses,
        None => name,
    }
}

/// The title over the page or the slides: the passage asked for, in the operator's language,
/// whole chapters by their numbers alone ("John 3", "Psalm 23", "Psalms 23–24"), and the short
/// names of the Bibles read.
pub fn title(page: &mut BiblePage) -> Option<(String, String)> {
    let (lead, range) = shown_range(page)?;
    let whole = range.start.verse == 1
        && range.start.book == range.end.book
        && range.end.verse >= lead.last_verse(range.end.book, range.end.chapter);
    let (book, from, to) = (range.start.book, range.start.chapter, range.end.chapter);
    let words = match (whole, from == to) {
        (true, true) => format!("{} {from}", sp_bible::reading_book_name(book, &page.names_language(), true)),
        (true, false) => {
            format!("{} {from}\u{2013}{to}", sp_bible::reading_book_name(book, &page.names_language(), false))
        }
        (false, _) => page.reading.clone(),
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
            let ink = if response.hovered() { TEXT } else { TEXT_DIM };
            let words = one_line(ui, &found.words, crate::studio::reading(15.0), ink, clip.width());
            ui.painter().galley(pos2(left, row.min.y + 34.0 - words.size().y / 2.0), words, ink);
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
                // The verse alone is the passage, its first verse picked; its whole chapter, the
                // verse picked in it.
                match place.filter(|_| chapter) {
                    Some(place) => {
                        let slide = if state.bible.options.break_on_new_verse { place } else { 0 };
                        state.pick_made_verse(show, slide);
                    }
                    None => {
                        state.pick_passage(show);
                    }
                }
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bible_slides_name_gives_its_verses_with_or_without_the_bibles_name() {
        assert_eq!(verses_named("John 3:16 (KJV)"), "John 3:16");
        assert_eq!(verses_named("От Иоанна 3:16-18 (SYN)"), "От Иоанна 3:16-18");
        assert_eq!(verses_named("John 3:16"), "John 3:16");
        // With the translation's name on the slides, the slide on screen still reads as its verse.
        let mut shelf = crate::bible::Shelf::shipped();
        let kjv = shelf.get(shelf.position("kjv").unwrap()).unwrap();
        let rows = [crate::bible::RowChoice {
            bible: kjv,
            verse: Some(sp_model::Role::primary()),
            reference: Some(sp_model::Role::reference()),
        }];
        let options = crate::bible::BibleOptions { display_translation: true, ..Default::default() };
        let passage = crate::bible::build_passage("John 3:16-17", &rows, &options, None).unwrap();
        let (group, _) = passage.slide(1).unwrap();
        assert_eq!(group.name, "John 3:17 (KJV)");
        let range = sp_bible::parse_reference_in(verses_named(&group.name), "en").unwrap();
        assert_eq!((range.start.chapter, range.start.verse, range.end.verse), (3, 17, 17));
    }
}
