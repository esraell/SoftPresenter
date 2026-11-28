//! What's new: a window over Settings with the changes of the version running, then the versions
//! before it to scroll through, read from CHANGELOG.md as it was when this program was built.
//! Nothing is fetched: the changelog is in the program.
//!
//! The changelog's own words are shown as they are written (English); the window's own words go
//! through `t()`. A version written in parts (`### New`, `### Better`, `### Fixed`) shows them
//! in that order, other headings joining Better; an older one, a flat list or under headings of
//! its own, shows one part, Changes. Each bullet is `**Name.** One sentence. More…`: the window
//! shows the name and the first sentence. The look is the design's (26 November, with the
//! owner's changes): 700 points wide, each part a rounded panel in the well grey, orange only on
//! New, the versions before in outlined panels.

use std::sync::{Arc, OnceLock};

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Galley, Key, Modifiers, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::i18n::{t, tf, tn};
use crate::quick_help::Hint;
use crate::studio::{CARD, LINE, PANEL, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, fixed, fixed_semibold, semibold};

/// The changelog, as it was when this program was built.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// The most versions the window lists, the one shown included.
pub const MOST: usize = 20;

/// A part of a version's changes, as its pill names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    New,
    Better,
    Fixed,
    /// A version written before the changelog had the three parts: all its changes.
    Changes,
}

impl Kind {
    /// The pill's word.
    fn words(self) -> &'static str {
        match self {
            Kind::New => t("settings.whats_new.part.new", "New"),
            Kind::Better => t("settings.whats_new.part.better", "Better"),
            Kind::Fixed => t("settings.whats_new.part.fixed", "Fixed"),
            Kind::Changes => t("settings.whats_new.part.changes", "Changes"),
        }
    }
}

/// One change: its short name, where the changelog gives one, and its first sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub name: Option<String>,
    pub sentence: String,
}

/// A part and its changes, in the changelog's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub kind: Kind,
    pub changes: Vec<Change>,
}

/// A version's section of the changelog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// "0.9.0-dev.25", or "Unreleased".
    pub number: String,
    /// As the heading writes it: "26 November 2026".
    pub date: Option<String>,
    /// The sentence under the heading saying what the version is about.
    pub lead: Option<String>,
    pub parts: Vec<Part>,
}

impl Version {
    /// A version that was made (not Unreleased): its heading starts with a version number.
    pub fn released(&self) -> bool {
        crate::updates::Version::parse(&self.number).is_some()
    }

    /// How many changes it has, in all its parts.
    pub fn changes(&self) -> usize {
        self.parts.iter().map(|part| part.changes.len()).sum()
    }
}

/// A heading inside a version, as far as the window is concerned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Heading {
    /// No heading yet: a flat list.
    None,
    New,
    Better,
    Fixed,
    /// Any other (Removed, Security, or an old version's topics).
    Other,
}

/// A version being read, line by line.
struct Draft {
    number: String,
    date: Option<String>,
    lead: Vec<String>,
    /// The opening sentence is over: a blank line after it, a heading or a bullet came.
    lead_done: bool,
    heading: Heading,
    /// A heading of the three was seen: the version is written in parts.
    in_parts: bool,
    bullets: Vec<(Heading, String)>,
    /// The line before was part of a bullet, so a line under it carries it on.
    in_bullet: bool,
}

impl Draft {
    /// From a `## ` heading's words: "0.9.0-dev.25 — 26 November 2026 (development build)".
    fn new(heading: &str) -> Draft {
        let (number, rest) = match heading.split_once('\u{2014}') {
            Some((number, rest)) => (number.trim(), Some(rest)),
            None => (heading.trim(), None),
        };
        let date = rest
            .map(|rest| rest.split_once(" (").map_or(rest, |(date, _)| date).trim().to_owned())
            .filter(|date| !date.is_empty());
        Draft {
            number: number.to_owned(),
            date,
            lead: Vec::new(),
            lead_done: false,
            heading: Heading::None,
            in_parts: false,
            bullets: Vec::new(),
            in_bullet: false,
        }
    }

    fn line(&mut self, line: &str) {
        let trimmed = line.trim();
        if let Some(heading) = line.strip_prefix("### ") {
            self.heading = match heading.trim() {
                "New" => Heading::New,
                "Better" => Heading::Better,
                "Fixed" => Heading::Fixed,
                _ => Heading::Other,
            };
            self.in_parts |= matches!(self.heading, Heading::New | Heading::Better | Heading::Fixed);
            (self.lead_done, self.in_bullet) = (true, false);
        } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            self.bullets.push((self.heading, item.trim().to_owned()));
            (self.lead_done, self.in_bullet) = (true, true);
        } else if trimmed.is_empty() {
            self.lead_done |= !self.lead.is_empty();
            self.in_bullet = false;
        } else if self.in_bullet
            && let Some((_, item)) = self.bullets.last_mut()
        {
            item.push(' ');
            item.push_str(trimmed);
        } else if !self.lead_done {
            self.lead.push(trimmed.to_owned());
        }
    }

    fn finish(self) -> Version {
        let changes = |wanted: &dyn Fn(Heading) -> bool| -> Vec<Change> {
            self.bullets.iter().filter(|(heading, _)| wanted(*heading)).map(|(_, item)| change(item)).collect()
        };
        let parts: Vec<Part> = if self.in_parts {
            [
                (Kind::New, changes(&|heading| heading == Heading::New)),
                (Kind::Better, changes(&|heading| !matches!(heading, Heading::New | Heading::Fixed))),
                (Kind::Fixed, changes(&|heading| heading == Heading::Fixed)),
            ]
            .into_iter()
            .filter(|(_, changes)| !changes.is_empty())
            .map(|(kind, changes)| Part { kind, changes })
            .collect()
        } else {
            let all = changes(&|_| true);
            if all.is_empty() { Vec::new() } else { vec![Part { kind: Kind::Changes, changes: all }] }
        };
        let lead = (!self.lead.is_empty()).then(|| without_code_marks(&self.lead.join(" ")));
        Version { number: self.number, date: self.date, lead, parts }
    }
}

/// A bullet's words as the window shows them: `**Name.** One sentence. More…` gives the name and
/// "One sentence."; one with no bold name, its first sentence alone.
fn change(item: &str) -> Change {
    let item = without_code_marks(item);
    if let Some(rest) = item.strip_prefix("**")
        && let Some((name, after)) = rest.split_once("**")
    {
        let name = name.trim();
        let name = name.strip_suffix('.').unwrap_or(name).trim();
        if !name.is_empty() {
            return Change { name: Some(name.to_owned()), sentence: first_sentence(after) };
        }
    }
    Change { name: None, sentence: first_sentence(&item) }
}

/// The words without the marks that write a name as code (`sp setup` reads sp setup).
fn without_code_marks(words: &str) -> String {
    words.replace('`', "")
}

/// The first sentence: up to the first full stop, question or exclamation mark that has a space
/// (or the end) after it, with a closing quote or bracket that follows it. A point inside a
/// number (0.5) or a name (.spb) ends nothing.
fn first_sentence(words: &str) -> String {
    let words = words.trim();
    let mut chars = words.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        while let Some(&(_, next)) = chars.peek()
            && matches!(next, '"' | '\'' | '\u{201d}' | '\u{2019}' | ')' | ']' | '\u{bb}')
        {
            chars.next();
        }
        match chars.peek() {
            None => break,
            Some(&(at, next)) if next.is_whitespace() => return words[..at].to_owned(),
            Some(_) => {}
        }
    }
    words.to_owned()
}

/// Reads every version in a changelog, newest first, as it is written. What comes before the
/// first `## ` heading (the title and its note) is passed over.
pub fn parse(text: &str) -> Vec<Version> {
    let mut versions = Vec::new();
    let mut draft: Option<Draft> = None;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            versions.extend(draft.take().map(Draft::finish));
            draft = Some(Draft::new(heading));
        } else if let Some(draft) = draft.as_mut() {
            draft.line(line);
        }
    }
    versions.extend(draft.map(Draft::finish));
    versions
}

/// The changelog's months, as its headings write them.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A date as the changelog writes it ("26 November 2026"): its day, month (1 is January) and year.
fn date_parts(date: &str) -> Option<(u32, usize, u32)> {
    let mut words = date.split_whitespace();
    let day = words.next()?.parse().ok()?;
    let month = words.next()?;
    let month = MONTHS.iter().position(|name| *name == month)? + 1;
    let year = words.next()?.parse().ok()?;
    words.next().is_none().then_some((day, month, year))
}

/// A month's name as a date says it ("26 ноября 2026"), 1 being January.
fn month_words(month: usize) -> &'static str {
    match month {
        1 => t("settings.whats_new.month.1", "January"),
        2 => t("settings.whats_new.month.2", "February"),
        3 => t("settings.whats_new.month.3", "March"),
        4 => t("settings.whats_new.month.4", "April"),
        5 => t("settings.whats_new.month.5", "May"),
        6 => t("settings.whats_new.month.6", "June"),
        7 => t("settings.whats_new.month.7", "July"),
        8 => t("settings.whats_new.month.8", "August"),
        9 => t("settings.whats_new.month.9", "September"),
        10 => t("settings.whats_new.month.10", "October"),
        11 => t("settings.whats_new.month.11", "November"),
        _ => t("settings.whats_new.month.12", "December"),
    }
}

/// A version's date in the window's language; as the changelog writes it if it can't be read.
fn date_words(date: &str) -> String {
    match date_parts(date) {
        Some((day, month, year)) => tf(
            "settings.whats_new.date",
            "{day} {month} {year}",
            &[("day", &day.to_string()), ("month", month_words(month)), ("year", &year.to_string())],
        ),
        None => date.to_owned(),
    }
}

/// The caption under the title: "Released 26 November 2026 · 13 changes".
fn released_words(version: &Version) -> String {
    let changes = tn("settings.whats_new.changes", version.changes() as u64, "{n} change", "{n} changes", &[]);
    match &version.date {
        Some(date) => tf(
            "settings.whats_new.released",
            "Released {date} \u{b7} {changes}",
            &[("date", &date_words(date)), ("changes", &changes)],
        ),
        None => changes,
    }
}

/// The versions of the changelog built in, read once.
fn versions() -> &'static [Version] {
    static PARSED: OnceLock<Vec<Version>> = OnceLock::new();
    PARSED.get_or_init(|| parse(CHANGELOG))
}

/// Whether two versions are the same (`v0.9.0` and `0.9.0` are).
fn same(one: &str, other: &str) -> bool {
    one == other
        || crate::updates::Version::parse(one).is_some_and(|one| crate::updates::Version::parse(other) == Some(one))
}

/// What the window lists: the version shown first, then the versions before it, newest first,
/// `MOST` at most; and which of the versions before is the one running.
pub struct Listing<'a> {
    pub versions: Vec<&'a Version>,
    pub here: Option<usize>,
}

/// The listing for `asked` (the version to show) on a computer running `running`. A version the
/// changelog doesn't have (a build of the source between versions, which calls itself 0.9.0)
/// shows the newest the changelog has. Unreleased is never listed.
pub fn listing<'a>(all: &'a [Version], asked: &str, running: &str) -> Listing<'a> {
    let made: Vec<&Version> = all.iter().filter(|version| version.released()).collect();
    let from = made.iter().position(|version| same(&version.number, asked)).unwrap_or(0);
    let versions: Vec<&Version> = made.into_iter().skip(from).take(MOST).collect();
    let here = versions.iter().skip(1).position(|version| same(&version.number, running)).map(|at| at + 1);
    Listing { versions, here }
}

/// Where the window scrolls to when it is next drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scroll {
    Top,
    End,
}

/// The What's new window's state, kept with the rest of the window's.
#[derive(Default)]
pub struct View {
    /// The version shown, while the window is open.
    shown: Option<String>,
    scroll: Option<Scroll>,
}

impl View {
    /// Opens the window on a version (`open_over_about` gives it the one running).
    pub fn open(&mut self, version: &str) {
        self.shown = Some(version.to_owned());
        self.scroll = Some(Scroll::Top);
    }

    pub fn close(&mut self) {
        self.shown = None;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_open(&self) -> bool {
        self.shown.is_some()
    }

    /// Testing aid: the window is scrolled to its end when it is next drawn, for a picture of it.
    pub fn scroll_to_end(&mut self) {
        self.scroll = Some(Scroll::End);
    }
}

// The window's measures, in points, as the design draws them.
/// Its width, and the room it leaves at least either side of it.
const WIDTH: f32 = 700.0;
const WIDTH_LEFT: f32 = 48.0;
/// The room it leaves at least above and below it.
const HEIGHT_LEFT: f32 = 32.0;
/// The title's band at its top, and the foot's at its bottom.
const HEAD: f32 = 52.0;
const FOOT: f32 = 58.0;
/// The room at its left and right.
const SIDE: f32 = 18.0;
/// Between the parts of the version shown (New, Better, Fixed).
const PARTS_APART: f32 = 10.0;
/// Between the versions before it.
const VERSIONS_APART: f32 = 20.0;
/// The words on the bright Done key.
const DONE_INK: Color32 = PANEL;

/// Where the window stands over a window of `screen`, its body `content` tall: 700 wide, as tall
/// as it needs up to 32 points less than the window, in its middle.
fn sheet_rect(screen: Rect, content: f32) -> Rect {
    let width = WIDTH.min(screen.width() - WIDTH_LEFT).max(0.0);
    let height = (HEAD + content + FOOT).min(screen.height() - HEIGHT_LEFT).max(0.0);
    Rect::from_center_size(screen.center(), vec2(width, height))
}

/// What the What's new key and action do (Settings › About's key, Help › What's New on a Mac,
/// `sp menu WhatsNew`): Settings opens at About, with the window over it, on the version running.
pub fn open_over_about(state: &mut crate::home::HomeState) {
    state.preferences_open = true;
    state.preferences_pane = crate::home::Pane::About;
    // The update work passes the version coming here instead, while one is downloading or ready,
    // and that build's changelog with it: the one built in never has a version newer than itself.
    state.whats_new.open(crate::updates::VERSION);
}

/// The window, over the Settings window, while it is open.
pub fn window(ctx: &egui::Context, view: &mut View) {
    draw(ctx, view, versions(), crate::updates::VERSION);
}

/// The window over the window of `ctx`: the title, the body that scrolls, and the foot with
/// "↓ Scroll for earlier versions" (its place kept once it goes) and Done. Esc, Enter, Done, the
/// × or a click outside it close it.
fn draw(ctx: &egui::Context, view: &mut View, all: &[Version], running: &str) {
    let Some(asked) = view.shown.clone() else { return };
    let listing = listing(all, &asked, running);
    let number = listing.versions.first().map_or(asked.as_str(), |version| version.number.as_str());
    let title = tf("settings.whats_new.title", "What\u{2019}s new in {version}", &[("version", number)]);
    let screen = ctx.content_rect();
    let mut close = ctx.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::Escape) || input.consume_key(Modifiers::NONE, Key::Enter)
    });
    let area = egui::Area::new(egui::Id::unique("whats new"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false);
    area.show(ctx, |ui| {
        ui.set_clip_rect(screen);
        let width = WIDTH.min(screen.width() - WIDTH_LEFT).max(0.0);
        let plan = plan(ui.painter(), &listing, width - 2.0 * SIDE);
        let sheet = sheet_rect(screen, plan.height);
        // The window under it takes the pointer, so nothing there is clicked; a click outside
        // the sheet puts it away.
        let outside = ui.allocate_rect(screen, Sense::click());
        if outside.clicked() && !outside.interact_pointer_pos().is_some_and(|at| sheet.contains(at)) {
            close = true;
        }
        let painter = ui.painter().clone();
        crate::studio::paint_sheet(&painter, screen, sheet);
        let room = sheet.width() - 2.0 * SIDE - 42.0;
        let heading = painter.layout(title, semibold(15.0), TEXT, room);
        painter.galley(pos2(sheet.min.x + SIDE, sheet.min.y + 29.0 - heading.size().y / 2.0), heading, TEXT);
        let cross = Rect::from_min_size(pos2(sheet.max.x - 42.0, sheet.min.y + 14.0), vec2(30.0, 30.0));
        let cross_response = ui.interact(cross, ui.scope_id().with("close whats new"), Sense::click());
        if cross_response.hovered() {
            painter.rect_filled(cross, 7.0, CARD);
        }
        crate::icons::paint(
            &painter,
            crate::icons::Icon::Close,
            Rect::from_center_size(cross.center(), vec2(14.0, 14.0)),
            TEXT_DIM,
        );
        if cross_response.hint(t("settings.whats_new.close", "Close (Esc)")).clicked() {
            close = true;
        }
        // The body, which scrolls; its end in view takes the foot's line away.
        let body = Rect::from_min_max(pos2(sheet.min.x, sheet.min.y + HEAD), pos2(sheet.max.x, sheet.max.y - FOOT));
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(body));
        let mut scroll = egui::ScrollArea::vertical().id_salt("whats new body").auto_shrink([false, false]);
        match view.scroll.take() {
            Some(Scroll::Top) => scroll = scroll.vertical_scroll_offset(0.0),
            Some(Scroll::End) => scroll = scroll.vertical_scroll_offset((plan.height - body.height()).max(0.0)),
            None => {}
        }
        let shown = scroll.show(&mut inside, |ui| {
            let (place, _) = ui.allocate_exact_size(vec2(body.width(), plan.height), Sense::hover());
            paint(ui.painter(), place.min.to_vec2() + vec2(SIDE, 0.0), &plan);
        });
        let end_in_view = shown.state.clamped_offset().y + shown.inner_rect.height() >= shown.content_size.y - 8.0;
        // The foot: Done at the right, as wide as its word in the language set; at the left,
        // while there is more below, the line that says so (its place kept, so nothing moves).
        let done_words = t("settings.whats_new.done", "Done");
        let done_galley = painter.layout_no_wrap(done_words.to_owned(), FontId::proportional(12.5), DONE_INK);
        let done_wide = done_galley.size().x + 26.0;
        let done =
            Rect::from_min_size(pos2(sheet.max.x - SIDE - done_wide, sheet.max.y - FOOT + 12.0), vec2(done_wide, 30.0));
        let done_response = ui.interact(done, ui.scope_id().with("done whats new"), Sense::click());
        painter.rect_filled(done, 9.0, if done_response.hovered() { Color32::WHITE } else { TEXT });
        painter.galley(done.center() - done_galley.size() / 2.0, done_galley, DONE_INK);
        if done_response.clicked() {
            close = true;
        }
        if listing.versions.len() > 1 && !end_in_view {
            let words = format!("\u{2193} {}", t("settings.whats_new.more", "Scroll for earlier versions"));
            let line = painter.layout_no_wrap(words, FontId::proportional(12.0), TEXT_FAINT);
            let room = Rect::from_min_max(pos2(sheet.min.x + SIDE, done.min.y), pos2(done.min.x - 12.0, done.max.y));
            let at = pos2(room.min.x, done.center().y - line.size().y / 2.0);
            painter.with_clip_rect(room).galley(at, line, TEXT_FAINT);
        }
    });
    if close {
        view.close();
    }
}

/// What the body of the window is made of, laid out from its top left.
struct Plan {
    marks: Vec<Mark>,
    height: f32,
}

/// One thing the body draws, placed from its top left.
enum Mark {
    Words(Pos2, Arc<Galley>),
    /// A panel or a pill: its fill and its edge (none where transparent).
    Panel {
        rect: Rect,
        round: f32,
        fill: Color32,
        edge: Color32,
    },
    Dot(Pos2, Color32),
    /// A line across, between an earlier version's parts.
    Rule {
        y: f32,
        from: f32,
        to: f32,
    },
}

/// Lays the body out.
struct Planner<'a> {
    painter: &'a Painter,
    marks: Vec<Mark>,
}

impl Planner<'_> {
    /// Words set in `font`, wrapped at `width`, each line `line` tall when given (the words in
    /// its middle, as on a web page) and `spacing` between the letters.
    fn words(
        &self,
        text: &str,
        font: FontId,
        ink: Color32,
        line: Option<f32>,
        spacing: f32,
        width: f32,
    ) -> Arc<Galley> {
        let format = TextFormat {
            font_id: font,
            color: ink,
            line_height: line,
            extra_letter_spacing: spacing,
            ..TextFormat::default()
        };
        let mut job = LayoutJob::single_section(text.to_owned(), format);
        job.wrap.max_width = width;
        self.painter.layout_job(job)
    }

    /// Puts words at `at` and says how tall they are.
    fn put(&mut self, at: Pos2, galley: Arc<Galley>) -> f32 {
        let height = galley.size().y;
        self.marks.push(Mark::Words(at, galley));
        height
    }

    /// Small capitals in the fixed face, at `at`; says how tall they are.
    fn caps(&mut self, at: Pos2, words: &str, size: f32, width: f32) -> f32 {
        let galley =
            self.words(&words.to_uppercase(), fixed_semibold(size), TEXT_FAINT, Some(size * 1.35), size * 0.05, width);
        self.put(at, galley)
    }

    /// A pill with words in capitals in the fixed face, `height` tall and `pad` either side of
    /// its words.
    #[allow(clippy::too_many_arguments)]
    fn pill(
        &mut self,
        at: Pos2,
        words: &str,
        size: f32,
        spacing: f32,
        pad: f32,
        height: f32,
        fill: Color32,
        edge: Color32,
        ink: Color32,
    ) -> Rect {
        let galley = self.words(&words.to_uppercase(), fixed_semibold(size), ink, None, size * spacing, f32::INFINITY);
        let rect = Rect::from_min_size(at, vec2(galley.size().x + 2.0 * pad, height));
        self.marks.push(Mark::Panel { rect, round: height / 2.0, fill, edge });
        self.put(pos2(at.x + pad, rect.center().y - galley.size().y / 2.0), galley);
        rect
    }

    /// A part's pill and count, then its changes: each a dot, its name and its sentence. Starts
    /// with the pill's top at `top`, `width` wide from `left`; gives the bottom of its list (10
    /// under its last change). `large`: the version shown; the versions before are a point
    /// smaller.
    fn part(&mut self, part: &Part, left: f32, top: f32, width: f32, large: bool) -> f32 {
        let new = part.kind == Kind::New;
        let (fill, edge, ink) =
            if new { (TALLY.gamma_multiply(0.22), Color32::TRANSPARENT, TALLY) } else { (PANEL, LINE, TEXT_DIM) };
        let pill = self.pill(pos2(left, top), part.kind.words(), 11.5, 0.08, 10.0, 22.0, fill, edge, ink);
        let count =
            self.words(&part.changes.len().to_string(), fixed(12.0), TEXT_FAINT, Some(16.2), 0.0, f32::INFINITY);
        let count_top = pill.center().y - count.size().y / 2.0;
        self.put(pos2(pill.max.x + 10.0, count_top), count);
        let dot = if new { TALLY } else { TEXT_FAINT };
        let (name, sentence) = if large { ((15.0, 20.25), (14.0, 21.0)) } else { ((14.0, 18.9), (13.5, 20.25)) };
        let room = width - 16.0;
        let mut y = pill.max.y + 12.0;
        for (at, change) in part.changes.iter().enumerate() {
            if at > 0 {
                y += 14.0;
            }
            self.marks.push(Mark::Dot(pos2(left + 2.5, y + 10.5), dot));
            if let Some(words) = &change.name {
                let galley = self.words(words, semibold(name.0), TEXT, Some(name.1), 0.0, room);
                y += self.put(pos2(left + 16.0, y), galley);
                if !change.sentence.is_empty() {
                    y += 3.0;
                }
            }
            if !change.sentence.is_empty() {
                let ink = TEXT.gamma_multiply(0.74);
                let galley =
                    self.words(&change.sentence, FontId::proportional(sentence.0), ink, Some(sentence.1), 0.0, room);
                y += self.put(pos2(left + 16.0, y), galley);
            }
        }
        y + 10.0
    }

    /// The version shown's part: a rounded panel in the well grey. Gives its bottom.
    fn shown_part(&mut self, part: &Part, top: f32, width: f32) -> f32 {
        let panel = self.marks.len();
        self.marks.push(Mark::Panel { rect: Rect::NOTHING, round: 12.0, fill: CARD, edge: Color32::TRANSPARENT });
        let bottom = self.part(part, 16.0, top + 14.0, width - 32.0, true) + 6.0;
        let rect = Rect::from_min_max(pos2(0.0, top), pos2(width, bottom));
        self.marks[panel] = Mark::Panel { rect, round: 12.0, fill: CARD, edge: Color32::TRANSPARENT };
        bottom
    }

    /// A version before the one shown: an outlined panel with its number, "On this computer"
    /// when it is the one running, its date at the right, its sentence, and its parts divided
    /// by lines. Gives its bottom.
    fn earlier(&mut self, version: &Version, here: bool, top: f32, width: f32) -> f32 {
        let panel = self.marks.len();
        self.marks.push(Mark::Panel { rect: Rect::NOTHING, round: 12.0, fill: Color32::TRANSPARENT, edge: LINE });
        let (left, inner) = (16.0, width - 32.0);
        let head = top + 16.0;
        let number = self.words(&version.number, semibold(17.0), TEXT, Some(22.95), 0.0, f32::INFINITY);
        let number_right = left + number.size().x;
        self.put(pos2(left, head), number);
        if let Some(date) = &version.date {
            let size = 11.5;
            let galley = self.words(
                &date_words(date).to_uppercase(),
                fixed_semibold(size),
                TEXT_FAINT,
                Some(size * 1.35),
                size * 0.05,
                f32::INFINITY,
            );
            self.put(pos2(width - 16.0 - galley.size().x, head + 6.0), galley);
        }
        if here {
            let words = t("settings.whats_new.here", "On this computer");
            self.pill(
                pos2(number_right + 10.0, head + 5.0),
                words,
                10.5,
                0.06,
                7.0,
                16.0,
                Color32::TRANSPARENT,
                LINE,
                TEXT_DIM,
            );
        }
        let mut y = head + 22.95 + 6.0;
        if let Some(lead) = &version.lead {
            let galley =
                self.words(lead, FontId::proportional(14.5), TEXT.gamma_multiply(0.8), Some(21.025), 0.0, inner);
            y += self.put(pos2(left, y), galley) + 12.0;
        }
        for part in &version.parts {
            self.marks.push(Mark::Rule { y, from: left, to: left + inner });
            y = self.part(part, left, y + 11.0, inner, false);
        }
        let bottom = y + 6.0;
        let rect = Rect::from_min_max(pos2(0.0, top), pos2(width, bottom));
        self.marks[panel] = Mark::Panel { rect, round: 12.0, fill: Color32::TRANSPARENT, edge: LINE };
        bottom
    }
}

/// Lays out the body, `width` wide: the caption (released, how many changes), the version's
/// sentence, its parts, then "Earlier versions", each of them, and "The first version listed
/// here".
fn plan(painter: &Painter, listing: &Listing, width: f32) -> Plan {
    let mut plan = Planner { painter, marks: Vec::new() };
    let Some((shown, earlier)) = listing.versions.split_first() else {
        return Plan { marks: plan.marks, height: 0.0 };
    };
    let mut y = 6.0;
    y += plan.caps(pos2(0.0, y), &released_words(shown), 11.0, width) + 20.0;
    if let Some(lead) = &shown.lead {
        let galley = plan.words(lead, semibold(17.0), TEXT, Some(23.8), -0.17, width);
        y += plan.put(pos2(0.0, y), galley) + 28.0;
    }
    for (at, part) in shown.parts.iter().enumerate() {
        if at > 0 {
            y += PARTS_APART;
        }
        y = plan.shown_part(part, y, width);
    }
    if earlier.is_empty() {
        return Plan { marks: plan.marks, height: y + 16.0 };
    }
    y += 42.0;
    y += plan.caps(pos2(2.0, y), t("settings.whats_new.earlier", "Earlier versions"), 11.0, width - 4.0) + 6.0 + 22.0;
    for (at, version) in earlier.iter().enumerate() {
        if at > 0 {
            y += VERSIONS_APART;
        }
        y = plan.earlier(version, listing.here == Some(at + 1), y, width);
    }
    y += 28.0;
    let size = 11.5;
    let end = t("settings.whats_new.first", "The first version listed here").to_uppercase();
    let galley = plan.words(&end, fixed_semibold(size), TEXT_FAINT, Some(size * 1.35), size * 0.05, width);
    let left = ((width - galley.size().x) / 2.0).max(0.0);
    y += plan.put(pos2(left, y), galley) + 12.0;
    Plan { marks: plan.marks, height: y }
}

/// Draws the body laid out by `plan`, its top left at `origin`.
fn paint(painter: &Painter, origin: Vec2, plan: &Plan) {
    for mark in &plan.marks {
        match mark {
            Mark::Words(at, galley) => painter.galley(*at + origin, galley.clone(), TEXT),
            Mark::Panel { rect, round, fill, edge } => {
                let rect = rect.translate(origin);
                painter.rect_filled(rect, *round, *fill);
                if *edge != Color32::TRANSPARENT {
                    painter.rect_stroke(rect, *round, Stroke::new(1.0, *edge), StrokeKind::Inside);
                }
            }
            Mark::Dot(at, ink) => {
                painter.circle_filled(*at + origin, 2.5, *ink);
            }
            Mark::Rule { y, from, to } => {
                painter.hline(origin.x + from..=origin.x + to, origin.y + y + 0.5, Stroke::new(1.0, LINE));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};
    use egui::{Event, Key, Modifiers, RawInput, pos2, vec2};

    /// A changelog written for these tests: Unreleased, two versions in parts (one with a
    /// heading that isn't one of the three), one under headings of its own, and a flat one.
    const SAMPLE: &str = "# What's new

What changed, newest first.

## Unreleased

Nothing yet.

## 0.9.0-dev.3 \u{2014} 3 December 2026 (development build)

Keys you can set, and a quieter start.

### New

- **Every key can be set.** Settings \u{203a} Keys lists what each key does. Change any of them.
- A bullet with no name. It has two sentences.

### Better

- **A shorter setup.** Five steps instead of eight, with `sp setup` too.

### Removed

- **An old switch.** The Classic look is gone.

### Fixed

- **Fades.** Slides fade evenly (0.5 seconds) on a second screen. Before, they stuttered.
- **Quotes.** It said \"Done.\" Then it closed.

## 0.9.0-dev.2 \u{2014} 2 December 2026 (development build, Mac only)

### Fixed

- **NDI.** An NDI screen no longer stalls.

## 0.9.0-dev.1 \u{2014} 1 December 2026

The first one.

### Songs

- Songs come in from text files (.txt) and more. Even big ones.
- Bibles show up to four translations.

### Fixes worth knowing

- Linux quits cleanly.

## 0.9.0-dev.0 \u{2014} 30 November 2026

- A flat line, with no heading. And more.
- Another flat line
  that goes on, indented.
";

    fn named(name: &str, sentence: &str) -> Change {
        Change { name: Some(name.to_owned()), sentence: sentence.to_owned() }
    }

    fn plain(sentence: &str) -> Change {
        Change { name: None, sentence: sentence.to_owned() }
    }

    fn numbers(listing: &Listing) -> Vec<String> {
        listing.versions.iter().map(|version| version.number.clone()).collect()
    }

    #[test]
    fn a_changelog_gives_its_versions_their_opening_sentence_parts_names_and_first_sentences() {
        let all = parse(SAMPLE);
        let names: Vec<&str> = all.iter().map(|version| version.number.as_str()).collect();
        assert_eq!(names, ["Unreleased", "0.9.0-dev.3", "0.9.0-dev.2", "0.9.0-dev.1", "0.9.0-dev.0"]);
        assert!(!all[0].released() && all[1..].iter().all(Version::released), "Unreleased is no version yet");
        let three = &all[1];
        assert_eq!(three.date.as_deref(), Some("3 December 2026"), "the date, without what kind of build");
        assert_eq!(three.lead.as_deref(), Some("Keys you can set, and a quieter start."));
        let kinds: Vec<Kind> = three.parts.iter().map(|part| part.kind).collect();
        assert_eq!(kinds, [Kind::New, Kind::Better, Kind::Fixed]);
        assert_eq!(
            three.parts[0].changes[0],
            named("Every key can be set", "Settings \u{203a} Keys lists what each key does."),
            "the name without its full stop, and only the first sentence"
        );
        assert_eq!(
            three.parts[2].changes,
            [
                named("Fades", "Slides fade evenly (0.5 seconds) on a second screen."),
                named("Quotes", "It said \"Done.\""),
            ],
            "a number's point ends no sentence; a full stop inside quotes does, with its quote"
        );
        assert_eq!(
            three.parts[1].changes[0],
            named("A shorter setup", "Five steps instead of eight, with sp setup too.")
        );
        assert_eq!(three.changes(), 6);
        let two = &all[2];
        assert_eq!(two.lead, None, "a version may have no opening sentence");
        assert_eq!(
            two.parts,
            [Part { kind: Kind::Fixed, changes: vec![named("NDI", "An NDI screen no longer stalls.")] }]
        );
    }

    #[test]
    fn a_heading_other_than_the_three_joins_better() {
        let all = parse(SAMPLE);
        let better = &all[1].parts[1];
        assert_eq!(better.kind, Kind::Better);
        assert_eq!(better.changes[1], named("An old switch", "The Classic look is gone."));
    }

    #[test]
    fn an_old_flat_version_is_one_part_called_changes() {
        let all = parse(SAMPLE);
        assert_eq!(
            all[4].parts,
            [Part {
                kind: Kind::Changes,
                changes: vec![
                    plain("A flat line, with no heading."),
                    plain("Another flat line that goes on, indented.")
                ],
            }],
            "its lines as sentences with no name; a line carried on to the next is one"
        );
        // One under headings of its own (the first numbered build's) is one part too, in order.
        assert_eq!(all[3].lead.as_deref(), Some("The first one."));
        assert_eq!(
            all[3].parts,
            [Part {
                kind: Kind::Changes,
                changes: vec![
                    plain("Songs come in from text files (.txt) and more."),
                    plain("Bibles show up to four translations."),
                    plain("Linux quits cleanly."),
                ],
            }]
        );
    }

    #[test]
    fn a_bullet_with_no_bold_name_shows_its_sentence_alone() {
        let all = parse(SAMPLE);
        assert_eq!(all[1].parts[0].changes[1], plain("A bullet with no name."));
    }

    #[test]
    fn the_window_lists_twenty_versions_at_most() {
        let mut text = String::new();
        for n in (0..25).rev() {
            text.push_str(&format!(
                "## 0.9.0-dev.{n} \u{2014} 1 December 2026\n\n### Fixed\n\n- **Fix {n}.** It works.\n\n"
            ));
        }
        let all = parse(&text);
        assert_eq!(all.len(), 25);
        let newest = listing(&all, "0.9.0-dev.24", "0.9.0-dev.24");
        assert_eq!(newest.versions.len(), MOST);
        assert_eq!(newest.versions[0].number, "0.9.0-dev.24");
        assert_eq!(newest.versions[MOST - 1].number, "0.9.0-dev.5");
        // From an older one, the versions before it, as many as there are up to twenty.
        let older = listing(&all, "0.9.0-dev.10", "0.9.0-dev.10");
        assert_eq!(older.versions.len(), 11);
        assert_eq!(older.versions[0].number, "0.9.0-dev.10");
    }

    #[test]
    fn the_version_shown_comes_first_then_the_ones_before_and_never_unreleased() {
        let all = parse(SAMPLE);
        assert_eq!(
            numbers(&listing(&all, "0.9.0-dev.3", "0.9.0-dev.3")),
            ["0.9.0-dev.3", "0.9.0-dev.2", "0.9.0-dev.1", "0.9.0-dev.0"]
        );
        assert_eq!(
            numbers(&listing(&all, "0.9.0-dev.2", "0.9.0-dev.2")),
            ["0.9.0-dev.2", "0.9.0-dev.1", "0.9.0-dev.0"]
        );
        // A build of the source between versions calls itself 0.9.0, which the changelog doesn't
        // have: the newest version it has is shown.
        let between = listing(&all, "0.9.0", "0.9.0");
        assert_eq!(numbers(&between)[0], "0.9.0-dev.3");
        assert_eq!(between.here, None);
    }

    #[test]
    fn the_version_running_is_marked_among_the_ones_before() {
        let all = parse(SAMPLE);
        // The version coming is shown; the one running is the next one down.
        assert_eq!(listing(&all, "0.9.0-dev.3", "0.9.0-dev.2").here, Some(1));
        // Showing the one running, nothing below is it.
        assert_eq!(listing(&all, "0.9.0-dev.3", "0.9.0-dev.3").here, None);
        // And the window says so on that version, once.
        let here = with_language(Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut view = View::default();
            view.open("0.9.0-dev.3");
            let frames = frames(&ctx, &mut view, "0.9.0-dev.2", &[vec![], vec![]]);
            let last = frames.last().expect("a frame");
            texts(last).into_iter().filter(|(words, _)| words == "ON THIS COMPUTER").count()
        });
        assert_eq!(here, 1);
        let mine = with_language(Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut view = View::default();
            view.open("0.9.0-dev.3");
            let frames = frames(&ctx, &mut view, "0.9.0-dev.3", &[vec![], vec![]]);
            texts(frames.last().expect("a frame")).into_iter().any(|(words, _)| words == "ON THIS COMPUTER")
        });
        assert!(!mine, "the version shown is the one running: nothing is marked");
    }

    /// Every line of words a frame drew, with where it was drawn.
    fn texts(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    into.push((text.galley.job.text.clone(), text.galley.rect.translate(text.pos.to_vec2())))
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut found));
        found
    }

    /// Where a frame drew `words`.
    fn place(output: &egui::FullOutput, words: &str) -> Option<Rect> {
        texts(output).into_iter().find(|(text, _)| text == words).map(|(_, rect)| rect)
    }

    /// Draws the window over a Settings window of 1100 × 700 (the size the design is drawn at),
    /// a frame for each list of events, on the sample changelog.
    fn frames(ctx: &egui::Context, view: &mut View, running: &str, events: &[Vec<Event>]) -> Vec<egui::FullOutput> {
        let all = parse(SAMPLE);
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0));
        events
            .iter()
            .enumerate()
            .map(|(n, events)| {
                let input = RawInput {
                    time: Some(n as f64 * 0.05),
                    screen_rect: Some(screen),
                    events: events.clone(),
                    ..RawInput::default()
                };
                let mut output = ctx.run_ui(input, |ui| draw(ui.ctx(), view, &all, running));
                // Only what was drawn is looked at; the fonts' texture goes nowhere.
                output.textures_delta.clear();
                output
            })
            .collect()
    }

    #[test]
    fn the_window_is_700_points_wide() {
        let (title, done) = with_language(Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut view = View::default();
            view.open("0.9.0-dev.3");
            let frames = frames(&ctx, &mut view, "0.9.0-dev.3", &[vec![], vec![]]);
            let last = frames.last().expect("a frame");
            (place(last, "What\u{2019}s new in 0.9.0-dev.3"), place(last, "Done"))
        });
        let (title, done) = (title.expect("the title is drawn"), done.expect("Done is drawn"));
        // The title starts 18 in from the window's left edge; Done ends 18 in from its right,
        // with 13 either side of its word.
        let left = title.min.x - 18.0;
        let right = done.max.x + 13.0 + 18.0;
        assert!((right - left - 700.0).abs() < 1.0, "the window is {} wide", right - left);
        assert!((left - 200.0).abs() < 1.0, "and in the middle of the Settings window: it starts at {left}");
    }

    #[test]
    fn nothing_moves_when_the_end_comes_into_view_and_the_line_at_the_foot_goes() {
        let more = "\u{2193} Scroll for earlier versions";
        let (top, end) = with_language(Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut view = View::default();
            view.open("0.9.0-dev.3");
            let top = frames(&ctx, &mut view, "0.9.0-dev.3", &[vec![], vec![]]).pop().expect("a frame");
            view.scroll_to_end();
            let end = frames(&ctx, &mut view, "0.9.0-dev.3", &[vec![], vec![]]).pop().expect("a frame");
            let at = |output: &egui::FullOutput| {
                (
                    place(output, "What\u{2019}s new in 0.9.0-dev.3"),
                    place(output, "Done"),
                    place(output, more),
                    place(output, "THE FIRST VERSION LISTED HERE"),
                )
            };
            (at(&top), at(&end))
        });
        assert!(top.0.is_some() && top.1.is_some(), "the title and Done are drawn");
        assert!(top.2.is_some(), "at the top, the foot says there is more below");
        assert!(end.2.is_none(), "at the end, it doesn't");
        assert!(end.3.is_some(), "and the end is in view");
        assert_eq!(top.0, end.0, "the title stays where it was");
        assert_eq!(top.1, end.1, "Done stays where it was");
    }

    #[test]
    fn escape_closes_the_window() {
        let open = with_language(Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut view = View::default();
            view.open("0.9.0-dev.3");
            let escape = Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            frames(&ctx, &mut view, "0.9.0-dev.3", &[vec![], vec![escape], vec![]]);
            view.shown.is_some()
        });
        assert!(!open);
    }

    // -----------------------------------------------------------------------------------------
    // The changelog built in

    /// Each section of a changelog from its `## ` heading on, with the heading's words.
    fn sections(text: &str) -> Vec<(&str, Vec<&str>)> {
        let mut found: Vec<(&str, Vec<&str>)> = Vec::new();
        for line in text.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                found.push((heading, Vec::new()));
            } else if let Some((_, lines)) = found.last_mut() {
                lines.push(line);
            }
        }
        found
    }

    /// What is wrong with a section that must be in the window's shape: an opening sentence,
    /// then New, Better and Fixed (each at most once, in that order, none empty), every bullet
    /// `- **Name.** Sentence.`. Unreleased may be empty, or say "Nothing yet.", instead (as it
    /// is straight after a version is made).
    fn shape_problems(heading: &str, lines: &[&str]) -> Vec<String> {
        let mut problems = Vec::new();
        let filled: Vec<&str> = lines.iter().copied().filter(|line| !line.trim().is_empty()).collect();
        if heading == "Unreleased" && (filled.is_empty() || filled == ["Nothing yet."]) {
            return problems;
        }
        match filled.first() {
            Some(first) if !first.starts_with('#') && !first.starts_with("- ") => {}
            _ => problems.push(format!("{heading}: no opening sentence under the heading")),
        }
        let order = ["### New", "### Better", "### Fixed"];
        let mut last = None;
        let mut bullets_under = 0;
        for line in &filled {
            if line.starts_with('#') {
                if last.is_some() && bullets_under == 0 {
                    problems.push(format!("{heading}: a part with no changes before {line:?}"));
                }
                match order.iter().position(|part| part == line) {
                    Some(at) if last.is_none_or(|before| at > before) => last = Some(at),
                    Some(_) => problems.push(format!("{heading}: {line:?} out of order or twice")),
                    None => problems.push(format!("{heading}: {line:?} isn't New, Better or Fixed")),
                }
                bullets_under = 0;
            } else if let Some(item) = line.strip_prefix("- ") {
                bullets_under += 1;
                if last.is_none() {
                    problems.push(format!("{heading}: a change before any part: {item:?}"));
                }
                let shaped =
                    item.strip_prefix("**").and_then(|rest| rest.split_once("**")).is_some_and(|(name, rest)| {
                        name.len() > 1 && name.ends_with('.') && rest.starts_with(' ') && !rest.trim().is_empty()
                    });
                if !shaped {
                    problems.push(format!("{heading}: not \"**Name.** Sentence.\": {item:?}"));
                }
            } else if last.is_some() {
                problems.push(format!("{heading}: a line that is no change: {line:?}"));
            }
        }
        if last.is_none() {
            problems.push(format!("{heading}: no New, Better or Fixed"));
        } else if bullets_under == 0 {
            problems.push(format!("{heading}: its last part has no changes"));
        }
        problems
    }

    /// What is wrong with a `## ` heading: it must be Unreleased, or a version, an em dash and
    /// its date as "26 November 2026", with anything more in brackets after it.
    fn heading_problem(heading: &str) -> Option<String> {
        if heading == "Unreleased" {
            return None;
        }
        let dated = |date: &str| {
            date_parts(date).is_some_and(|(day, month, year)| format!("{day} {} {year}", MONTHS[month - 1]) == date)
        };
        let fine = heading.split_once(" \u{2014} ").is_some_and(|(number, rest)| {
            let (date, note) = match rest.split_once(" (") {
                Some((date, note)) => (date, Some(note)),
                None => (rest, None),
            };
            crate::updates::Version::parse(number).is_some()
                && number.starts_with(|c: char| c.is_ascii_digit())
                && dated(date)
                && note.is_none_or(|note| note.ends_with(')') && !note[..note.len() - 1].contains(['(', ')']))
        });
        (!fine).then(|| {
            format!(
                "## {heading}: not \"Unreleased\" or \"<version> \u{2014} <d Month yyyy>\" with any more in brackets"
            )
        })
    }

    #[test]
    fn the_shape_check_takes_an_empty_unreleased() {
        // Straight after a version is made, Unreleased is empty, or says so.
        assert_eq!(shape_problems("Unreleased", &[]), Vec::<String>::new());
        assert_eq!(shape_problems("Unreleased", &["", ""]), Vec::<String>::new());
        assert_eq!(shape_problems("Unreleased", &["", "Nothing yet.", ""]), Vec::<String>::new());
        // Anything else in it is in the full shape.
        assert_eq!(
            shape_problems("Unreleased", &["Faster.", "", "### Fixed", "", "- **Fades.** Even."]),
            Vec::<String>::new()
        );
        assert!(!shape_problems("Unreleased", &["- **Fades.** Even."]).is_empty(), "a change in no part");
        assert!(
            !shape_problems("Unreleased", &["Faster.", "", "### Fixed", "", "- Fades are even."]).is_empty(),
            "no name"
        );
        assert!(
            !shape_problems("Unreleased", &["Faster.", "", "### Added", "", "- **Fades.** Even."]).is_empty(),
            "Added"
        );
    }

    #[test]
    fn the_heading_check_refuses_a_heading_written_otherwise() {
        // Headings: Unreleased, or a version, an em dash and its date, and a note in brackets.
        for good in [
            "Unreleased",
            "0.9.0 \u{2014} 27 November 2026",
            "0.9.0-dev.22 \u{2014} 25 November 2026 (development build, Windows only)",
        ] {
            assert_eq!(heading_problem(good), None, "{good}");
        }
        for bad in [
            "0.9.0-dev.26 - 26 November 2026",
            "0.9.0-dev.26 \u{2013} 26 November 2026",
            "0.9.0-dev.26\u{2014}26 November 2026",
            "New",
            "unreleased",
            "0.9.0-dev.26",
            "0.9.0 \u{2014} 2026-11-27",
            "0.9.0 \u{2014} 27 Nov 2026",
            "0.9.0 \u{2014} 07 November 2026",
            "0.9.0 \u{2014} 27 November 2026 extra",
            "0.9.0 \u{2014} 27 November 2026 (development build",
            "Version 0.9.0 \u{2014} 27 November 2026",
        ] {
            assert!(heading_problem(bad).is_some(), "{bad} passed");
        }
    }

    #[test]
    fn the_changelog_built_in_is_in_the_shape_the_window_reads() {
        use crate::updates::Version as Number;
        let all = parse(CHANGELOG);
        let made: Vec<&Version> = all.iter().filter(|version| version.released()).collect();
        assert!(made.len() >= 13, "only {} versions read", made.len());
        // Every `## ` heading is Unreleased or a version with its date.
        let wrong: Vec<String> =
            sections(CHANGELOG).iter().filter_map(|(heading, _)| heading_problem(heading)).collect();
        assert!(wrong.is_empty(), "CHANGELOG.md:\n{}", wrong.join("\n"));
        // Newest first, each with its date and its changes.
        for pair in made.windows(2) {
            assert!(
                Number::parse(&pair[0].number) > Number::parse(&pair[1].number),
                "{} comes before {}",
                pair[0].number,
                pair[1].number
            );
        }
        for version in &made {
            assert!(
                version.date.as_deref().and_then(date_parts).is_some(),
                "{}: no date like \"26 November 2026\"",
                version.number
            );
            assert!(version.changes() > 0, "{}: no changes", version.number);
            assert!(
                version
                    .parts
                    .iter()
                    .flat_map(|part| &part.changes)
                    .all(|change| !change.sentence.is_empty() || change.name.is_some()),
                "{}: an empty change",
                version.number
            );
        }
        // Unreleased, and every version from dev.20 on, is written in the window's shape.
        let shaped_from = Number::parse("0.9.0-dev.20");
        let mut problems = Vec::new();
        for (heading, lines) in sections(CHANGELOG) {
            let number = heading.split(" \u{2014} ").next().unwrap_or(heading).trim();
            let due = number == "Unreleased" || Number::parse(number).is_some_and(|n| Some(&n) >= shaped_from.as_ref());
            if due {
                problems.extend(shape_problems(number, &lines));
            }
        }
        assert!(problems.is_empty(), "CHANGELOG.md:\n{}", problems.join("\n"));
        // What this build shows has something in it.
        let shown = listing(&all, crate::updates::VERSION, crate::updates::VERSION);
        assert!(shown.versions.first().is_some_and(|version| version.changes() > 0));
    }
}
