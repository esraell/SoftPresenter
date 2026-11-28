//! The middle of the Service view: the open playlist as one scroll, item after item, each under a
//! bar with its name that stays at the top while its slides scroll under it; or the one song
//! chosen in a library, shown the same way. The Bible page shows its passage with the same
//! tiles.
//!
//! A playlist can hold thousands of slides, so the scroll is laid out as plain numbers first
//! (where each item starts and how tall it is) and only what is in view is drawn.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use egui::style::ScrollAnimation;
use sp_model::{Presentation, Uuid};

use super::*;
use crate::i18n::{t, tf, tk, tn};
use crate::key_table::{ClickDoes, slide_click_does};
use crate::outside::Waiting;
use crate::quick_help::Hint;
use crate::service::{Item, SavedPassage};
use crate::studio::{
    CARD, ICON_BUTTON, ItemAsk, LINE, MORE_BUTTON, PANEL, RADIUS, ROW_HOVER, SongAsks, TALLY, TEXT, TEXT_DIM,
    TEXT_FAINT, fixed, fixed_semibold, icon_button, more_button, semibold,
};

/// The room at each side of the scroll, and under its last item (so the last row stands
/// clear of the fade above the dock).
const EDGE: f32 = 16.0;
const FOOT: f32 = 36.0;
/// The room round an item's tiles or rows.
const INSET: f32 = 4.0;
/// The room between tiles, across and down.
const GAP_X: f32 = 14.0;
const GAP_Y: f32 = 16.0;
/// How tall the strip under a tile's picture is.
const STRIP: f32 = 28.0;
/// How tall an item's bar is: a song's, which carries tools, and any other's.
const SONG_BAR: f32 = 52.0;
const ITEM_BAR: f32 = 43.0;
/// How tall a playlist header's line is.
const HEADING: f32 = 39.5;
/// The least height of a row in the words-only and table views, the room between rows, and
/// the height of the table's headings.
const LIST_ROW: f32 = 34.0;
const LIST_GAP: f32 = 2.0;
const TABLE_HEAD: f32 = 19.0;
/// The room kept between the slide on screen and the top of the view, where an item's bar
/// stays, and between it and the bottom, where the fade is.
pub const CLEAR_ABOVE: f32 = 60.0;
pub const CLEAR_BELOW: f32 = 44.0;
/// How long a slide found with the find box keeps its ring, in seconds.
const SPOTLIGHT_FOR: f64 = 1.8;
/// How tall the line is that says "Not in this arrangement", over the slides it leaves out.
const LEFT_OUT_HEAD: f32 = 34.0;
/// The ring round slides picked out to change several at once.
const PICKED: Color32 = Color32::from_rgb(0x6a, 0xa5, 0xff);
/// The narrowest a slide in the grid gets, in points, before fewer go across.
const SMALLEST_SLIDE: f32 = 150.0;
/// How rounded a tile's picture is.
const ROUND: f32 = RADIUS;

/// How the tiles are sized for the width there is and the size chosen with the slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub view: SlideView,
    /// How many tiles stand in a row.
    pub columns: usize,
    /// A tile: its picture with the strip under it.
    pub tile: egui::Vec2,
    pub picture: egui::Vec2,
    /// The width the items have, between the scroll's edges.
    pub width: f32,
}

impl Grid {
    /// `across` is the width between the scroll's edges; `wanted` how many tiles the slider
    /// asks for in a row (2 to 7). Fewer stand in a row when the room is narrow, so no tile
    /// is too small to read.
    pub fn new(view: SlideView, across: f32, wanted: usize) -> Grid {
        let inner = (across - 2.0 * INSET).max(40.0);
        let fit = ((inner + GAP_X) / (SMALLEST_SLIDE + GAP_X)).floor().max(1.0) as usize;
        let columns = wanted.clamp(2, 7).min(fit);
        let width = ((inner - GAP_X * (columns as f32 - 1.0)) / columns as f32).floor().max(40.0);
        let picture = vec2(width, (width * 9.0 / 16.0).round());
        Grid { view, columns, tile: vec2(width, picture.y + STRIP), picture, width: across }
    }

    /// How tall the tiles of an item with `count` slides are, with the room round them.
    fn tiles_height(&self, count: usize) -> f32 {
        let rows = count.div_ceil(self.columns).max(1) as f32;
        2.0 * INSET + rows * self.tile.y + (rows - 1.0) * GAP_Y
    }

    /// Where a tile is, from the top left corner of its item's tiles.
    fn tile_at(&self, index: usize) -> egui::Vec2 {
        let (row, column) = (index / self.columns, index % self.columns);
        vec2(INSET + column as f32 * (self.tile.x + GAP_X), INSET + row as f32 * (self.tile.y + GAP_Y))
    }
}

/// The rows of one item in the words-only or table view: where each starts, from the top of
/// the item's rows, and how tall it is (a row is as tall as its words, wrapped).
#[derive(Debug, Default, PartialEq)]
pub struct Rows {
    tops: Vec<f32>,
    heights: Vec<f32>,
    total: f32,
    /// Where the line over the slides an arrangement leaves out starts, if it has any.
    left_out: Option<f32>,
}

/// What stands under an item's bar.
#[derive(Clone, Debug)]
pub enum Body {
    /// Nothing (a playlist header).
    None,
    /// Tiles in a grid: how many.
    Tiles(usize),
    /// A song in an arrangement: its tiles in the order sung, then, under a line saying so,
    /// the tiles of the groups the arrangement leaves out (none, when it leaves none out).
    Sung { sung: usize, out: usize },
    /// One box the size of a tile's picture, with no strip (a placeholder).
    Box,
    /// Rows, in the words-only and table views.
    Rows(Rc<Rows>),
    /// A line of words (a song that can't be found, a song with no slides).
    Line,
}

/// How much room one item takes: its bar, the strip of its arrangement, and what is under
/// them.
#[derive(Clone, Debug)]
pub struct Shape {
    pub bar: f32,
    pub strip: f32,
    pub body: Body,
}

/// Where one item stands in the scroll, from the top of the scroll's contents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub top: f32,
    pub bar: f32,
    pub strip: f32,
    pub body: f32,
}

impl Place {
    /// Where the item's tiles or rows start.
    pub fn body_top(&self) -> f32 {
        self.top + self.bar + self.strip
    }

    pub fn bottom(&self) -> f32 {
        self.body_top() + self.body
    }
}

/// Lays the items out one under another. Returns where each stands and how tall they are
/// together, with the room under the last. Nothing is drawn or measured here, so it costs the
/// same however many slides the items have.
pub fn lay_out(shapes: &[Shape], grid: &Grid) -> (Vec<Place>, f32) {
    let mut top = 0.0;
    let places = shapes
        .iter()
        .map(|shape| {
            let body = match &shape.body {
                Body::None => 0.0,
                Body::Tiles(count) => grid.tiles_height(*count),
                Body::Sung { sung, out } => {
                    grid.tiles_height(*sung) + if *out > 0 { LEFT_OUT_HEAD + grid.tiles_height(*out) } else { 0.0 }
                }
                Body::Box => grid.picture.y + 2.0 * INSET,
                Body::Rows(rows) => rows.total,
                Body::Line => LIST_ROW + 2.0 * INSET,
            };
            let place = Place { top, bar: shape.bar, strip: shape.strip, body };
            top = place.bottom();
            place
        })
        .collect();
    (places, top + FOOT)
}

/// How tall the scroll's contents are, for items laid out at `places`, `total` tall with the
/// room under the last (see [`lay_out`]), in a view `view` tall: with room under a song by
/// itself (`alone`) to drop a picture or video on; under a playlist's last item, room as tall
/// as the view less that item, so that any item, the last too, can be scrolled to the top.
fn scroll_height(places: &[Place], total: f32, view: f32, alone: bool) -> f32 {
    match places.last() {
        _ if alone => (total + 90.0).max(view),
        Some(last) => total.max(last.top + view),
        None => total,
    }
}

/// Where the faint line across goes that starts the room under a playlist's last item (from
/// the top of the scroll's contents), so the end reads as an end and not as slides still to
/// come: a little under the item. None under a song by itself, and in an empty playlist.
fn tail_line(places: &[Place], alone: bool) -> Option<f32> {
    places.last().filter(|_| !alone).map(|last| last.bottom() + TAIL_GAP)
}

/// How far under a playlist's last item the line across is, and how far in from either side.
const TAIL_GAP: f32 = 6.0;
const TAIL_IN: f32 = 3.0;

/// Keeps the playlist item that was at the top of the slides there, as far into it as the view
/// was, for a few passes after a drop put something in above it ([`HomeState::slides_hold`]),
/// unless the view is being taken elsewhere (`going`) or scrolled. The pass drawn with the view
/// pushed down is thrown away, and the next begins with the view where it is to be
/// (`slides_set_offset`), so it is never seen. Otherwise it notes which item is at the top, and
/// how far into it the view is, for the next drop.
fn keep_top(ui: &egui::Ui, state: &mut HomeState, decks: &[Deck], places: &[Place], viewport: Rect, going: bool) {
    let scrolled = ui.input(|input| input.smooth_scroll_delta.y != 0.0);
    if going || scrolled {
        state.slides_hold = None;
    }
    if let Some((item, into, passes)) = state.slides_hold.take() {
        state.slides_hold = passes.checked_sub(1).filter(|left| *left > 0).map(|left| (item, into, left));
        ui.ctx().request_repaint();
        if let Some(place) = decks.iter().position(|deck| deck.item() == Some(item)).map(|at| places[at]) {
            let to = place.top + into;
            if (to - viewport.min.y).abs() > 0.5 {
                // Set as the next pass begins: a scroll asked for now would only be under way
                // by then.
                state.slides_set_offset = Some(to);
                ui.ctx().request_discard("the slides keep the item at their top after a drop");
            }
        }
        return;
    }
    state.slides_top = decks
        .iter()
        .zip(places)
        .rev()
        .find(|(deck, place)| deck.item().is_some() && place.top <= viewport.min.y + 0.5)
        .and_then(|(deck, place)| Some((deck.item()?, viewport.min.y - place.top)));
}

/// Where the view is to be scrolled so that the slide on screen is in sight: `offset` is how
/// far the view is scrolled now, `height` how tall it is, and `slide` the slide's top and
/// bottom in the scroll's contents. The slide is kept clear of the top of the view, where
/// its item's bar stays, and of the bottom, where the fade is. `None` when it is in sight
/// already; the view then stays where the operator left it.
pub fn follow_offset(offset: f32, height: f32, slide: egui::Rangef) -> Option<f32> {
    let wanted = if slide.min < offset + CLEAR_ABOVE {
        (slide.min - CLEAR_ABOVE).max(0.0)
    } else if slide.max > offset + height - CLEAR_BELOW {
        slide.max - height + CLEAR_BELOW
    } else {
        return None;
    };
    ((wanted - offset).abs() > 0.5).then_some(wanted)
}

/// What to bring into sight for a slide picked (`picked`, its top and bottom in the scroll): the
/// slide, and with it the last slide of the passage it begins (`end`) when both fit in a view
/// `height` high clear of the bar and the fade.
pub fn with_passage_end(picked: egui::Rangef, end: Option<egui::Rangef>, height: f32) -> egui::Rangef {
    match end.map(|end| picked.union(end)) {
        Some(wider) if wider.span() <= height - CLEAR_ABOVE - CLEAR_BELOW => wider,
        _ => picked,
    }
}

/// Bible passages kept in playlists, made into slides once and kept: making one reads its
/// Bibles and lays out every verse, and each making gives its slides new ids, which the
/// thumbnails are kept by.
#[derive(Default)]
pub struct PassageDecks {
    made: Vec<MadePassage>,
    /// What the passages were made with (the Bible theme's boxes and template, the Bibles
    /// installed); one made with something else is made again.
    with: String,
    frame: u64,
    /// A passage is still to be made: the window is drawn again to make it.
    waiting: bool,
}

struct MadePassage {
    saved: SavedPassage,
    with: String,
    deck: Result<Arc<Presentation>, String>,
    used: u64,
}

impl PassageDecks {
    /// Called once a frame, before any passage is asked for.
    fn begin(&mut self, bible: &BiblePage) {
        self.frame += 1;
        self.waiting = false;
        let with = (bible.template, &bible.boxes, bible.shelf.bibles.len());
        self.with.clear();
        use std::fmt::Write;
        let _ = write!(self.with, "{with:?}");
    }

    /// A saved passage's slides, or why they could not be made. `None` while it is still to
    /// be made and this frame has made its share (`budget`): a playlist full of passages is
    /// made a few a frame, so opening it never stalls the window.
    fn get(
        &mut self,
        bible: &mut BiblePage,
        saved: &SavedPassage,
        budget: &mut u32,
    ) -> Option<Result<Arc<Presentation>, String>> {
        if let Some(made) = self.made.iter_mut().find(|made| made.saved == *saved && made.with == self.with) {
            made.used = self.frame;
            return Some(made.deck.clone());
        }
        if *budget == 0 {
            self.waiting = true;
            return None;
        }
        *budget -= 1;
        // Those no playlist in front has asked for lately make room.
        let frame = self.frame;
        if self.made.len() >= 64 {
            self.made.retain(|made| made.used + 1 >= frame);
        }
        let deck = bible.passage_of(saved).map(Arc::new);
        self.made.push(MadePassage { saved: saved.clone(), with: self.with.clone(), deck: deck.clone(), used: frame });
        Some(deck)
    }
}

/// The heights of rows in the words-only and table views, kept from one frame to the next:
/// working them out lays out every slide's words.
#[derive(Default)]
pub struct RowHeights {
    kept: HashMap<usize, KeptRows>,
    frame: u64,
}

struct KeptRows {
    /// The song the rows are of. Keeping it here keeps its address, which the rows are kept
    /// by, from being given to another song.
    _deck: Arc<Presentation>,
    width: f32,
    view: SlideView,
    rows: Rc<Rows>,
    used: u64,
}

impl RowHeights {
    fn begin(&mut self) {
        self.frame += 1;
        // Songs no longer shown (or changed, which makes a new copy of them) are let go.
        if self.kept.len() > 512 {
            let frame = self.frame;
            self.kept.retain(|_, kept| kept.used + 1 >= frame);
        }
    }

    fn get(&mut self, ui: &egui::Ui, deck: &Arc<Presentation>, view: SlideView, columns: &ListColumns) -> Rc<Rows> {
        let key = Arc::as_ptr(deck) as usize;
        let width = columns.words.span();
        if let Some(kept) = self.kept.get_mut(&key)
            && kept.width == width
            && kept.view == view
        {
            kept.used = self.frame;
            return kept.rows.clone();
        }
        let font = columns.font.clone();
        let mut rows = Rows::default();
        let mut top = if view == SlideView::Table { TABLE_HEAD + LIST_GAP } else { 0.0 };
        for entry in entries(deck) {
            let Some((_, slide)) = deck.slide(entry.slide) else { continue };
            if entry.out && rows.left_out.is_none() {
                rows.left_out = Some(top);
                top += LEFT_OUT_HEAD;
            }
            let words = ui.painter().layout(slide_words(slide), font.clone(), TEXT, width.max(20.0));
            let height = (words.size().y + 8.0).max(LIST_ROW).round();
            rows.tops.push(top);
            rows.heights.push(height);
            top += height + LIST_GAP;
        }
        rows.total = top + 6.0;
        let rows = Rc::new(rows);
        self.kept.insert(key, KeptRows { _deck: deck.clone(), width, view, rows: rows.clone(), used: self.frame });
        rows
    }
}

/// One tile (or row) of a song in the scroll: which slide; with an arrangement, its place in
/// the order sung and whether it is sung again there, or that the arrangement leaves it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub slide: usize,
    pub step: Option<usize>,
    pub again: bool,
    pub out: bool,
}

/// A song's tiles in the order they stand: as written, or, with an arrangement, in the order
/// sung (a group sung again stands again, as the same slides), then what it leaves out.
pub fn entries(song: &Presentation) -> Vec<Entry> {
    match song.sung() {
        Some((sung, out)) => sung
            .into_iter()
            .map(|one| Entry { slide: one.slide, step: Some(one.step), again: one.again, out: false })
            .chain(out.into_iter().map(|slide| Entry { slide, step: None, again: false, out: true }))
            .collect(),
        None => (0..song.slide_count()).map(|slide| Entry { slide, step: None, again: false, out: false }).collect(),
    }
}

/// Which of a song's tiles is the slide on screen: `slide` at `step` in the order sung (an
/// arrangement's slide sung twice is lit only where the song is), or a slide it leaves out.
pub fn live_entry(entries: &[Entry], slide: usize, step: Option<usize>) -> Option<usize> {
    entries
        .iter()
        .position(|entry| entry.slide == slide && entry.step.is_some() && entry.step == step)
        .or_else(|| entries.iter().position(|entry| entry.slide == slide && (entry.out || entry.step.is_none())))
        .or_else(|| entries.iter().position(|entry| entry.slide == slide))
}

/// A slide's words on one line, its own lines set apart by a stroke.
fn slide_words(slide: &Slide) -> String {
    slide.texts().into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n").replace('\n', " / ")
}

/// Where the columns of a row are in the words-only and table views.
struct ListColumns {
    font: FontId,
    /// The left edges of the number and of the group (table) or the middle of the dot (words
    /// only).
    number: f32,
    dot: f32,
    words: egui::Rangef,
    /// The group's name: right-aligned at `max` in the words-only view, from `min` in the table.
    group: egui::Rangef,
    notes: egui::Rangef,
    /// The middle of the "more" button.
    more: f32,
}

impl ListColumns {
    fn new(view: SlideView, left: f32, width: f32) -> ListColumns {
        let (min, max) = (left + INSET + 10.0, left + width - INSET - 10.0);
        let more = max - 11.0;
        if view == SlideView::Table {
            let notes = egui::Rangef::new(max - 148.0, max - 30.0);
            ListColumns {
                font: FontId::proportional(13.0),
                number: min,
                dot: min + 36.0 + 4.0,
                group: egui::Rangef::new(min + 36.0, min + 186.0),
                words: egui::Rangef::new(min + 196.0, notes.min - 10.0),
                notes,
                more,
            }
        } else {
            let group = egui::Rangef::new(max - 32.0 - 110.0, max - 32.0);
            ListColumns {
                font: FontId::proportional(14.0),
                number: min,
                dot: min + 41.0,
                words: egui::Rangef::new(min + 56.0, group.min - 10.0),
                group,
                notes: egui::Rangef::new(max, max),
                more,
            }
        }
    }
}

/// One item of the scroll.
pub enum Deck {
    /// A playlist header: a small capital line.
    Heading { item: usize, name: String },
    /// Slides: a song, a Bible passage or the VoxLive item.
    Slides(SlideDeck),
    /// A picture or video from the media bin: `at` is its place there, while it is there.
    Media { item: usize, at: Option<usize>, name: String },
    /// A media playlist: the first few of its pictures and videos in the bin, how many are
    /// there, and whether the playlist itself has gone.
    MediaPlaylist { item: usize, id: Uuid, name: String, shown: Vec<usize>, count: usize, gone: bool },
    /// A place kept for something to come.
    Placeholder { item: usize, name: String },
    /// Something with nothing to show but a line of words: a song the library doesn't have, a
    /// passage that can't be made.
    Note { item: usize, title: String, kind: &'static str, words: String },
}

/// The slides of a song, a Bible passage or the VoxLive item.
pub struct SlideDeck {
    /// Its place in the playlist in front; none for a song shown by itself.
    pub item: Option<usize>,
    /// Its place among the show's presentations. None for a playlist's Bible passage, which
    /// is put there when one of its slides is clicked.
    pub at: Option<usize>,
    pub deck: Arc<Presentation>,
    /// The passage a playlist keeps, for its slides to be put on screen.
    pub passage: Option<SavedPassage>,
    /// Slides the app makes (a Bible passage, the VoxLive item): shown, never changed.
    pub made: bool,
    /// Drawn with the theme Bible slides use.
    pub bible: bool,
    /// Whether it has a bar with its name over it (the Bible page's passage has none).
    pub bar: bool,
    /// The slides of the passage asked for, ringed, when the Bible page made them into its
    /// chapter's slides (see `bible::page_deck`).
    pub ringed: Option<std::ops::Range<usize>>,
}

impl Deck {
    /// The playlist item this is, if it is one.
    fn item(&self) -> Option<usize> {
        match self {
            Deck::Slides(deck) => deck.item,
            Deck::Heading { item, .. }
            | Deck::Media { item, .. }
            | Deck::MediaPlaylist { item, .. }
            | Deck::Placeholder { item, .. }
            | Deck::Note { item, .. } => Some(*item),
        }
    }
}

impl SlideDeck {
    /// The slide of this deck that is on screen, if one is. A playlist's Bible passage is on
    /// screen when one of the Bible page's two places holds it and a slide of that is up.
    fn live_slide(&self, show: &Show, state: &HomeState) -> Option<usize> {
        let live = show.live()?;
        match self.at {
            Some(at) => (live.presentation == at).then_some(live.slide),
            None => {
                let slots = state.bible.slots?;
                let up = show.presentations().get(live.presentation).filter(|_| slots.contains(&live.presentation))?;
                (up.title == self.deck.title && live.slide < self.deck.slide_count()).then_some(live.slide)
            }
        }
    }

    /// A slide of the deck as the show knows it, when the show holds the deck.
    fn slide_ref(&self, slide: usize) -> Option<SlideRef> {
        self.at.map(|presentation| SlideRef { presentation, slide })
    }

    /// Puts a slide of the deck on screen, as a click on it does, and makes its item the
    /// open one. A playlist's Bible passage is first looked up as the Bible page would, with
    /// the translations and options it was kept with, which puts it among the show's
    /// presentations; its slide goes on screen once it is there.
    fn show_slide(&self, show: &Show, state: &mut HomeState, slide: usize, commands: &mut Vec<Command>) {
        match (self.slide_ref(slide), &self.passage) {
            (Some(here), _) => commands.push(Command::GoTo(here)),
            (None, Some(saved)) => {
                state.bible.open_saved(saved, show.live().map(|at| at.presentation), crate::home::SlidesOf::Passage);
                // Only if the passage was made: a slide kept waiting would go on screen with
                // the next passage anyone looks up.
                state.bible.go_live = state.bible.pending.is_some().then_some(slide);
            }
            (None, None) => {}
        }
        if self.item.is_some() {
            state.services.chosen = self.item;
            state.live_item = self.item;
        }
    }

    /// Picks a slide of the deck for Enter, as a click does when set to a double-click (Settings
    /// › Keys): its song is chosen and its item opened, as when it goes on screen, but nothing
    /// goes on screen. Under an arrangement it keeps the place in the order sung clicked
    /// (`step`), for Enter to go to (`HomeState::pick_command`). It is in view already, so the
    /// view stays where it is.
    fn pick_slide(
        &self,
        show: &Show,
        state: &mut HomeState,
        here: SlideRef,
        step: Option<usize>,
        commands: &mut Vec<Command>,
    ) {
        let bible = state.bible_view_now();
        if !bible && show.selected() != here.presentation && !commands.contains(&Command::Select(here.presentation)) {
            commands.push(Command::Select(here.presentation));
        }
        if self.item.is_some() {
            state.services.chosen = self.item;
        }
        state.set_pick(show, here, bible);
        if let Some(picked) = state.picked.as_mut() {
            picked.step = step;
        }
        state.pick_scroll = 0;
    }

    /// Picks a slide of a playlist's Bible passage for Enter, as a click does when set to a
    /// double-click: the passage is looked up as a click on its slide looks it up, and once the
    /// app has put it there its slide is the one picked, the passage chosen and its item opened;
    /// nothing goes on screen. It goes into the Bible page's place that neither is on screen nor
    /// holds the slide last on screen (the show's cursor, after a clear): Next goes on from that
    /// slide, so a passage picked in its place would be gone on through from where it stood.
    fn pick_passage_slide(&self, show: &Show, state: &mut HomeState, slide: usize, commands: &mut Vec<Command>) {
        if self.item.is_some() {
            state.services.chosen = self.item;
        }
        let Some(saved) = &self.passage else { return };
        let kept = show.live().or(show.cursor()).map(|at| at.presentation);
        state.bible.open_saved(saved, kept, crate::home::SlidesOf::Passage);
        state.bible.go_live = None;
        let made = state.bible.pending.as_ref().map(|(slot, passage)| (*slot, passage.slide_count()));
        if let Some((slot, _)) = made.filter(|(_, count)| slide < *count) {
            commands.push(Command::Select(slot));
            state.set_pick(show, SlideRef { presentation: slot, slide }, false);
            state.pick_scroll = 0;
        }
    }

    /// Whether `at` is a slide of this deck when the deck is a playlist's Bible passage: in one
    /// of the Bible page's places, made of the same passage (by its name, as `live_slide` knows
    /// it on screen).
    fn passage_holds(&self, show: &Show, state: &HomeState, at: SlideRef) -> bool {
        self.at.is_none()
            && state.bible.slots.is_some_and(|slots| slots.contains(&at.presentation))
            && show
                .presentations()
                .get(at.presentation)
                .is_some_and(|up| up.title == self.deck.title && at.slide < self.deck.slide_count())
    }
}

/// What a plain click on a slide's tile does (`response`, just clicked), by Settings › Keys'
/// Show a slide with (`key_table::slide_click_does`): `live_item` is whether a slide of its song
/// or passage is on screen, `here_live` whether this one is. A double-click counts only when its
/// first click was on this same tile a moment before, which every plain click notes
/// ([`HomeState::second_click_on`]).
fn click_does(state: &mut HomeState, response: &egui::Response, live_item: bool, here_live: bool) -> ClickDoes {
    let within = response.ctx.options(|options| options.input_options.max_double_click_delay);
    let same = state.second_click_on(response.id, response.ctx.input(|input| input.time), within);
    slide_click_does(state.slide_click(), live_item, response.double_clicked(), same, here_live)
}

/// What the middle of the Service view shows: the playlist in front, every item of it, when the
/// left column lists a playlist; otherwise the song chosen, by itself. Also keeps the open
/// item and the song chosen in step (see [`keep_in_step`]).
pub fn decks(show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) -> Vec<Deck> {
    state.passages.begin(&state.bible);
    if !state.playlist_open {
        state.open_seen = None;
        return song_alone(show, state, show.selected(), true).into_iter().collect();
    }
    let songs = playlist_songs(show, state);
    if !keep_in_step(show, state, &songs, commands) {
        return song_alone(show, state, show.selected(), true).into_iter().collect();
    }
    let items = state.services.front().items.clone();
    let mut budget = 2;
    items
        .into_iter()
        .zip(songs.iter().copied())
        .enumerate()
        .map(|(item, (entry, at))| match (entry, at) {
            (Item::Header { name }, _) => Deck::Heading { item, name },
            (Item::Media { id, name }, _) => {
                // Its heading names it without the ending, as the playlist's row does.
                let name = crate::media::label(&name, show.media());
                Deck::Media { item, at: show.media().iter().position(|media| media.id == id), name }
            }
            (Item::MediaPlaylist { id, name }, _) => {
                let playlist = state.media_playlists.iter().find(|playlist| playlist.id == id);
                let places: Vec<usize> = playlist
                    .map(|playlist| {
                        playlist
                            .files
                            .iter()
                            .filter_map(|file| show.media().iter().position(|media| &media.path == file))
                            .collect()
                    })
                    .unwrap_or_default();
                let (count, gone) = (places.len(), playlist.is_none());
                Deck::MediaPlaylist { item, id, name, shown: places.into_iter().take(3).collect(), count, gone }
            }
            (Item::Placeholder { name }, _) => Deck::Placeholder { item, name },
            (Item::Passage(saved), _) => {
                let title = saved.lookup.clone();
                match state.passages.get(&mut state.bible, &saved, &mut budget) {
                    Some(Ok(deck)) => Deck::Slides(SlideDeck {
                        item: Some(item),
                        at: None,
                        deck,
                        passage: Some(saved),
                        made: true,
                        bible: true,
                        bar: true,
                        ringed: None,
                    }),
                    Some(Err(problem)) => Deck::Note { item, title, kind: "Bible", words: problem },
                    None => Deck::Note {
                        item,
                        title,
                        kind: "Bible",
                        words: t("slides.note.looking_up", "Looking it up\u{2026}").to_owned(),
                    },
                }
            }
            (Item::Song { .. }, Some(at)) => match show.presentations().get(at) {
                Some(deck) => Deck::Slides(SlideDeck {
                    item: Some(item),
                    at: Some(at),
                    deck: deck.clone(),
                    passage: None,
                    made: state.app_made(at),
                    bible: false,
                    bar: true,
                    ringed: None,
                }),
                None => Deck::Note { item, title: String::new(), kind: "Missing", words: String::new() },
            },
            (Item::Song { title, .. }, None) => Deck::Note {
                item,
                title,
                kind: "Missing",
                words: t("slides.note.missing", "This song is not in the library any more.").to_owned(),
            },
        })
        .collect()
}

/// One song (or the Bible page's passage, or the VoxLive item) shown by itself.
fn song_alone(show: &Show, state: &HomeState, which: usize, bar: bool) -> Option<Deck> {
    let deck = show.presentations().get(which)?.clone();
    let bible = state.bible.slots.is_some_and(|slots| slots.contains(&which));
    let ringed = state.bible.ringed(which, &deck);
    Some(Deck::Slides(SlideDeck {
        item: None,
        at: Some(which),
        deck,
        passage: None,
        made: state.app_made(which),
        bible,
        bar,
        ringed,
    }))
}

/// The song each item of the playlist in front names, by its place among the songs, looked
/// up again only when the songs or the items change.
pub(super) fn playlist_songs(show: &Show, state: &mut HomeState) -> Arc<Vec<Option<usize>>> {
    let items = &state.services.front().items;
    let epoch = state.lists.songs.epoch();
    match state.lists.playlist_songs.get(|(made_in, made_of)| *made_in == epoch && made_of == items) {
        Some(songs) => songs,
        None => {
            let songs = items
                .iter()
                .map(|item| match item {
                    Item::Song { key, .. } => crate::studio::playlist_song(show, state, key),
                    _ => None,
                })
                .collect();
            let items = items.clone();
            state.lists.playlist_songs.keep((epoch, items), songs)
        }
    }
}

/// Keeps the open item of the playlist in front and the song chosen in step, whichever of
/// them changed since the last frame, and says whether the playlist is still what the middle
/// shows.
///
/// - An item opened (a click on its row, ↑ and ↓, something added) that names a song: that
///   song is chosen, so the tools that work on the song chosen work on it.
/// - Another song chosen (a click on one of its slides, Next going on into it, a search, the
///   command line): the item that names it becomes the open one, the next one down the
///   playlist first. A song the playlist doesn't have is shown by itself, with the left
///   column back on a library that has it.
/// - A Bible passage or the VoxLive item chosen: the playlist stays.
fn keep_in_step(show: &Show, state: &mut HomeState, songs: &[Option<usize>], commands: &mut Vec<Command>) -> bool {
    let selected = show.selected();
    let chosen = state.services.chosen;
    let now = (state.services.current, chosen, selected);
    let seen = state.open_seen.replace(now);
    let names = |item: Option<usize>| item.and_then(|item| songs.get(item).copied().flatten());
    if names(chosen) == Some(selected) {
        return true;
    }
    let Some((list, was_chosen, was_selected)) = seen else { return true };
    if (list != now.0 || was_chosen != chosen)
        && let Some(song) = names(chosen)
    {
        if !commands.contains(&Command::Select(song)) {
            commands.push(Command::Select(song));
        }
        return true;
    }
    // A Bible passage on the Bible page's own places leaves the playlist showing.
    let passage = state.bible.slots.is_some_and(|slots| slots.contains(&selected));
    if was_selected != selected && !passage {
        let from = chosen.unwrap_or(0).min(songs.len());
        let found = songs[from..]
            .iter()
            .position(|at| *at == Some(selected))
            .map(|after| after + from)
            .or_else(|| songs.iter().position(|at| *at == Some(selected)));
        match found {
            Some(item) => {
                state.services.chosen = Some(item);
                state.open_seen = Some((now.0, Some(item), selected));
            }
            None => {
                // Not in this playlist: the song is shown by itself, from a library.
                state.playlist_open = false;
                let key = state.song_keys.get(selected);
                let shown = state.library_shown.and_then(|id| state.libraries.iter().find(|l| l.id == id));
                if shown.is_some_and(|library| key.is_none_or(|key| !library.has(key))) {
                    state.library_shown = None;
                }
                state.open_seen = None;
                return false;
            }
        }
    }
    true
}

/// What the scroll's parts need to know about the frame they are drawn in.
struct Sheet {
    /// The top left corner of the scroll's contents, on screen.
    origin: egui::Pos2,
    /// The left edge of the items.
    left: f32,
    grid: Grid,
    columns: ListColumns,
    /// What of the scroll is in view, on screen.
    clip: Rect,
}

impl Sheet {
    /// Where a height in the scroll's contents is on screen.
    fn y(&self, content: f32) -> f32 {
        self.origin.y + content
    }

    fn tile(&self, place: &Place, index: usize) -> Rect {
        Rect::from_min_size(pos2(self.left, self.y(place.body_top())) + self.grid.tile_at(index), self.grid.tile)
    }

    fn row(&self, place: &Place, rows: &Rows, index: usize) -> Rect {
        Rect::from_min_size(
            pos2(self.left + INSET, self.y(place.body_top()) + rows.tops[index]),
            vec2(self.grid.width - 2.0 * INSET, rows.heights[index]),
        )
    }

    /// Where a tile or row of an item is, by its place among them (see [`entries`]).
    fn slide(&self, place: &Place, body: &Body, index: usize) -> Rect {
        match body {
            Body::Rows(rows) if index < rows.tops.len() => self.row(place, rows, index),
            // Under an arrangement, what it leaves out stands under its own line.
            Body::Sung { sung, .. } if index >= *sung => {
                self.tile(place, index - sung).translate(vec2(0.0, self.grid.tiles_height(*sung) + LEFT_OUT_HEAD))
            }
            _ => self.tile(place, index),
        }
    }

    /// The line over the slides an arrangement leaves out, if the item has any.
    fn left_out_line(&self, place: &Place, body: &Body) -> Option<Rect> {
        let top = match body {
            Body::Sung { sung, out } if *out > 0 => place.body_top() + self.grid.tiles_height(*sung),
            Body::Rows(rows) => place.body_top() + rows.left_out?,
            _ => return None,
        };
        Some(Rect::from_min_size(
            pos2(self.left + INSET, self.y(top)),
            vec2(self.grid.width - 2.0 * INSET, LEFT_OUT_HEAD),
        ))
    }

    fn in_view(&self, rect: Rect) -> bool {
        rect.max.y >= self.clip.min.y && rect.min.y <= self.clip.max.y
    }

    /// The whole width of the scroll between two heights of its contents.
    fn band(&self, top: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(self.clip.min.x, self.y(top)), vec2(self.clip.width(), height))
    }
}

/// How tall the strip asking about a song changed outside SoftPresenter is, under its title bar
/// (none while nothing is asked about it), for slides laid out `width` wide.
fn ask_height(ui: &egui::Ui, state: &HomeState, slides: &SlideDeck, width: f32) -> f32 {
    let asked = slides.at.filter(|_| !slides.made).is_some_and(|at| state.notices.asks_about(Waiting::Song(at)));
    if asked { crate::notices::ask_strip_height(ui, width - 2.0 * INSET) } else { 0.0 }
}

/// How much room an item takes.
fn shape_of(ui: &egui::Ui, state: &mut HomeState, deck: &Deck, sheet_grid: &Grid, columns: &ListColumns) -> Shape {
    match deck {
        Deck::Heading { .. } => Shape { bar: HEADING, strip: 0.0, body: Body::None },
        Deck::Slides(slides) => {
            let count = slides.deck.slide_count();
            let tools = slides.at.is_some() && !slides.made;
            // A song of the song catalog being looked at has tools of its own (see `item_bar`).
            let catalog = slides.at.is_some_and(|at| state.catalog.holds(at));
            let bar = match (slides.bar, tools || catalog) {
                (false, _) => 0.0,
                (true, true) => SONG_BAR,
                (true, false) => ITEM_BAR,
            };
            let strip = if tools {
                ask_height(ui, state, slides, sheet_grid.width)
                    + arrangement_layout(ui, &slides.deck, sheet_grid.width).map_or(0.0, |layout| layout.height)
            } else {
                0.0
            };
            let body = if count == 0 {
                Body::Line
            } else if sheet_grid.view == SlideView::Grid {
                match slides.deck.sung() {
                    Some((sung, out)) => Body::Sung { sung: sung.len(), out: out.len() },
                    None => Body::Tiles(count),
                }
            } else {
                Body::Rows(state.rows.get(ui, &slides.deck, sheet_grid.view, columns))
            };
            Shape { bar, strip, body }
        }
        Deck::Media { .. } | Deck::MediaPlaylist { .. } => Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Tiles(1) },
        Deck::Placeholder { .. } => Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Box },
        Deck::Note { .. } => Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Line },
    }
}

/// The slides as one scroll: every item under its bar, only what is in view drawn. `alone` is
/// true when one song is shown by itself (a media item dropped under it then becomes a slide
/// of it).
pub fn slide_scroll(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    decks: &[Deck],
    theme: Option<&Theme>,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let view = state.slide_view;
    state.rows.begin();
    if state.passages.waiting {
        ui.ctx().request_repaint();
    }
    let mut ask: Option<ItemAsk> = None;
    let mut asks = SongAsks::default();
    let alone = matches!(decks, [Deck::Slides(SlideDeck { item: None, .. })]);
    let mut area = egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]);
    // Where a drop's hold wants the view, before anything is laid out (`keep_top`).
    if let Some(offset) = state.slides_set_offset.take() {
        area = area.vertical_scroll_offset(offset);
    }
    area.show_viewport(ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        let across = (ui.available_width() - 2.0 * EDGE).max(60.0);
        let grid = Grid::new(view, across, state.columns);
        let left = origin.x + EDGE;
        let columns = ListColumns::new(view, left, across);
        let shapes: Vec<Shape> = decks.iter().map(|deck| shape_of(ui, state, deck, &grid, &columns)).collect();
        let (places, total) = lay_out(&shapes, &grid);
        let total = scroll_height(&places, total, viewport.height(), alone);
        // Anything that takes the view elsewhere this frame goes before keeping its top item.
        let live_now = show.live().map(|live| (live, show.position().map(|(step, _)| step)));
        let going = state.slides_jump_to.is_some()
            || state.spotlight_scroll > 0
            || state.pick_scroll > 0
            || state.live_menu.is_some()
            || live_now != state.live_seen;
        if alone {
            (state.slides_top, state.slides_hold) = (None, None);
        } else {
            keep_top(ui, state, decks, &places, viewport, going);
        }
        ui.allocate_rect(Rect::from_min_size(origin, vec2(ui.available_width(), total)), Sense::hover());
        // Only the scroll's own width: a scroll area that scrolls up and down leaves the
        // width of its clip to whatever holds it, which can be wider.
        let clip = ui.clip_rect().intersect(Rect::from_x_y_ranges(ui.max_rect().x_range(), ui.clip_rect().y_range()));
        let sheet = Sheet { origin, left, grid, columns, clip };
        // Between the slides: a click picks nothing; a drag draws a box that picks out every
        // slide it touches.
        let background = ui.interact(clip, ui.scope_id().with("slide background"), Sense::click_and_drag()).no_hint();
        if background.clicked() {
            state.picked_slides.clear();
            state.selected_slide = None;
        }
        start_slide_box(ui, state, &background);
        if egui::DragAndDrop::has_payload_of_type::<SlideDrag>(ui.ctx()) {
            scroll_while_dragging(ui);
        }
        // Cut, Copy and Paste work on the song chosen, when it is one of those shown.
        let chosen = decks
            .iter()
            .any(|deck| matches!(deck, Deck::Slides(slides) if slides.at == Some(show.selected()) && !slides.made));
        if chosen {
            slide_clipboard_keys(ui, show, show.selected(), state);
        }
        // Where every slide of the show is, for the box that picks slides and for Shift-click:
        // plain sums, however many slides there are.
        state.slide_rects.clear();
        for ((deck, place), shape) in decks.iter().zip(&places).zip(&shapes) {
            if let Deck::Slides(slides) = deck
                && let Some(presentation) = slides.at
            {
                // Each slide once: a slide an arrangement sings again is picked where it first stands.
                if slides.deck.active_arrangement().is_some() {
                    for (index, entry) in entries(&slides.deck).iter().enumerate().filter(|(_, entry)| !entry.again) {
                        let rect = sheet.slide(place, &shape.body, index);
                        state.slide_rects.push((SlideRef { presentation, slide: entry.slide }, rect));
                    }
                } else {
                    for slide in 0..slides.deck.slide_count() {
                        let rect = sheet.slide(place, &shape.body, slide);
                        state.slide_rects.push((SlideRef { presentation, slide }, rect));
                    }
                }
            }
        }
        // An item just opened is brought to the top of the view; the slide that has just gone
        // on screen is brought into sight. Both at once, not by a glide: a glide can be cut
        // short by the next key and leave the slide out of sight.
        if let Some(item) = state.slides_jump_to.take()
            && let Some(place) = decks.iter().position(|deck| deck.item() == Some(item)).map(|at| places[at])
        {
            ui.scroll_with_delta_animation(vec2(0.0, viewport.min.y - place.top), ScrollAnimation::none());
        }
        // A slide found with the find box: the view goes to it, in the middle, once its song is
        // shown (it may take a frame or two to be).
        if state.spotlight_scroll > 0
            && let Some((found, _)) = state.spotlight
        {
            state.spotlight_scroll -= 1;
            let place = decks.iter().enumerate().find_map(|(at, deck)| match deck {
                Deck::Slides(slides) if slides.at == Some(found.presentation) => {
                    let index = entries(&slides.deck).iter().position(|entry| entry.slide == found.slide)?;
                    Some(sheet.slide(&places[at], &shapes[at].body, index))
                }
                _ => None,
            });
            if let Some(rect) = place {
                let middle = rect.center().y - origin.y;
                let to = (middle - viewport.height() / 2.0).max(0.0);
                ui.scroll_with_delta_animation(vec2(0.0, viewport.min.y - to), ScrollAnimation::none());
                state.spotlight_scroll = 0;
            } else {
                ui.ctx().request_repaint();
            }
        }
        // A slide just picked (by the find box, Scripture Lookup, ↑ or ↓) is brought into sight
        // as the slide on screen is, once its song or passage is shown.
        if state.pick_scroll > 0 {
            state.pick_scroll -= 1;
            let picked = state.pick(show);
            let rect_of = |at: SlideRef| state.slide_rects.iter().find(|(here, _)| *here == at).map(|(_, rect)| *rect);
            // On the Bible page, the passage's last slide among its chapter's comes into sight
            // with the verse picked, when the two fit.
            let passage_end = picked.and_then(|picked| {
                let deck = show.presentations().get(picked.presentation)?;
                let ringed = state.bible.ringed(picked.presentation, deck)?;
                rect_of(SlideRef { presentation: picked.presentation, slide: ringed.end.checked_sub(1)? })
            });
            match picked.and_then(rect_of) {
                Some(rect) => {
                    let span = |rect: Rect| egui::Rangef::new(rect.min.y - origin.y, rect.max.y - origin.y);
                    let range = with_passage_end(span(rect), passage_end.map(span), viewport.height());
                    if let Some(to) = follow_offset(viewport.min.y, viewport.height(), range) {
                        ui.scroll_with_delta_animation(vec2(0.0, viewport.min.y - to), ScrollAnimation::none());
                    }
                    state.pick_scroll = 0;
                }
                None => ui.ctx().request_repaint(),
            }
        }
        // The menu of the slide on screen asked for (modifier-O): its slide is brought into view
        // too, without making its item the open one again.
        let changed = live_now != state.live_seen;
        if changed || state.live_menu.is_some() {
            state.live_seen = live_now;
            let (open, started) = (state.services.chosen, state.live_item);
            // When a song is in the playlist twice: the item it was put on screen from (a
            // click, a key), then the open item, then the first.
            let found = decks
                .iter()
                .enumerate()
                .filter_map(|(at, deck)| match deck {
                    Deck::Slides(slides) => slides.live_slide(show, state).map(|slide| {
                        (at, (slides.item.is_some() && slides.item == started, slides.item == open), slide)
                    }),
                    _ => None,
                })
                .rev()
                .max_by_key(|(_, rank, _)| *rank);
            if let Some((at, _, slide)) = found {
                // The item whose slide has just gone on screen is the open one: Next going on
                // into the next song takes the open item with it.
                if changed && let Some(item) = decks[at].item() {
                    state.services.chosen = Some(item);
                    state.live_item = Some(item);
                }
                // Its tile: under an arrangement, the one for where the song is.
                let step = show.position().map(|(step, _)| step);
                let index = match &decks[at] {
                    Deck::Slides(deck) if deck.deck.active_arrangement().is_some() => {
                        live_entry(&entries(&deck.deck), slide, step).unwrap_or(slide)
                    }
                    _ => slide,
                };
                let rect = sheet.slide(&places[at], &shapes[at].body, index);
                let range = egui::Rangef::new(rect.min.y - origin.y, rect.max.y - origin.y);
                if let Some(to) = follow_offset(viewport.min.y, viewport.height(), range) {
                    ui.scroll_with_delta_animation(vec2(0.0, viewport.min.y - to), ScrollAnimation::none());
                }
            }
        }
        // What is under each bar, for the items in view.
        for ((deck, place), shape) in decks.iter().zip(&places).zip(&shapes) {
            if !sheet.in_view(sheet.band(place.top, place.bottom() - place.top)) {
                continue;
            }
            match deck {
                Deck::Heading { item, name } => {
                    let line = sheet.band(place.top, HEADING);
                    let response = ui.interact(line, ui.scope_id().with(("heading", *item)), Sense::click());
                    let at = pos2(left + INSET, line.min.y + 22.0 + 7.75);
                    crate::studio::paint_spaced(
                        ui,
                        at,
                        Align2::LEFT_CENTER,
                        &name.to_uppercase(),
                        fixed_semibold(11.5),
                        TEXT_DIM,
                        0.9,
                    );
                    response.context_menu(|ui| {
                        crate::studio::item_menu(ui, show, state, commands, *item, &mut ask, &mut asks);
                    });
                }
                Deck::Slides(slides) => {
                    // A song changed outside SoftPresenter asks under its title bar, as the
                    // notices do, then comes its arrangement.
                    let ask = ask_height(ui, state, slides, across);
                    if let Some(at) = slides.at.filter(|_| ask > 0.0) {
                        let top = sheet.y(place.top + place.bar);
                        let band = Rect::from_min_size(pos2(left + INSET, top), vec2(across - 2.0 * INSET, ask));
                        crate::notices::ask_strip(ui, state, band, Waiting::Song(at));
                    }
                    if place.strip > ask {
                        arrangement_strip(
                            ui,
                            show,
                            slides,
                            state,
                            commands,
                            pos2(left, sheet.y(place.top + place.bar + ask)),
                            across,
                        );
                    }
                    draw_slides(ui, show, state, slides, place, &shape.body, &sheet, thumb, commands);
                }
                Deck::Media { item, at, .. } => {
                    let tile = sheet.tile(place, 0);
                    let length =
                        at.and_then(|at| show.media().get(at)).and_then(|m| state.media_lengths.get(&m.id).copied());
                    let announced = at
                        .and_then(|at| show.media().get(at))
                        .is_some_and(|m| crate::announcements::shows_item(state, m.id));
                    let live = at.is_some() && *at == show.live_media() || announced;
                    // The strip under the picture has one line: a short form where the words don't fit.
                    let fit = |(key, english): (&'static str, &'static str)| {
                        crate::studio::within(ui, key, english, fixed_semibold(12.0), 0.5, tile.width() - 6.0, true)
                    };
                    let words = match at {
                        None => fit(tk("slides.media.gone", "Gone from the media strip")),
                        Some(_) if live => fit(tk("slides.media.live", "On screen")),
                        Some(_) => fit(tk("slides.media.show", "Click to show it")),
                    };
                    let pictures: Vec<Option<TextureId>> =
                        at.iter().map(|at| thumb(ThumbOf::Media(*at)).map(|(texture, _)| texture)).collect();
                    let response = item_tile(ui, &sheet, tile, ("media tile", *item), &pictures, &words, live, length);
                    if let Some(at) = at
                        && response.hint(t("slides.media.show_hint", "Show it on the screens")).clicked()
                    {
                        state.services.chosen = Some(*item);
                        crate::studio::show_media_item(show, state, *at, commands);
                    }
                }
                Deck::MediaPlaylist { item, id, shown, count, gone, .. } => {
                    let tile = sheet.tile(place, 0);
                    let live = state.media_playing == Some(*id) || crate::announcements::plays_playlist(state, *id);
                    let words = if *gone {
                        t("slides.playlist.gone", "Gone").to_owned()
                    } else if *count == 0 {
                        t("slides.playlist.empty", "Nothing in it yet").to_owned()
                    } else if live {
                        let (key, english) = tk("slides.playlist.playing", "Playing round and round");
                        crate::studio::within(ui, key, english, fixed_semibold(12.0), 0.5, tile.width() - 6.0, true)
                    } else {
                        tn(
                            "slides.playlist.click",
                            *count as u64,
                            "{n} item \u{b7} click to play them round",
                            "{n} items \u{b7} click to play them round",
                            &[],
                        )
                    };
                    let pictures: Vec<Option<TextureId>> =
                        shown.iter().map(|at| thumb(ThumbOf::Media(*at)).map(|(texture, _)| texture)).collect();
                    let response = item_tile(ui, &sheet, tile, ("playlist tile", *item), &pictures, &words, live, None);
                    if !*gone
                        && *count > 0
                        && response
                            .hint(t(
                                "slides.playlist.hint",
                                "Play its pictures (8 seconds each) and videos in turn, round and round",
                            ))
                            .clicked()
                    {
                        state.services.chosen = Some(*item);
                        crate::studio::play_media_playlist(state, *id);
                    }
                }
                Deck::Placeholder { item, .. } => {
                    let tile = Rect::from_min_size(sheet.tile(place, 0).min, sheet.grid.picture);
                    placeholder_tile(ui, show, state, *item, tile, commands);
                }
                Deck::Note { words, .. } => {
                    let at = pos2(left + INSET, sheet.y(place.body_top()) + INSET + LIST_ROW / 2.0);
                    ui.painter().text(at, Align2::LEFT_CENTER, words, FontId::proportional(12.5), TEXT_FAINT);
                }
            }
        }
        // Under a song shown by itself: a picture or video dragged from the media strip and
        // dropped here is added as a slide of its own.
        if alone
            && let (Some(Deck::Slides(slides)), Some(place)) = (decks.first(), places.first())
            && !slides.made
        {
            let top = sheet.y(place.bottom());
            let zone = Rect::from_min_max(pos2(left, top), pos2(left + across, (origin.y + total).max(top + 90.0)));
            let response = ui.interact(zone, ui.scope_id().with("under the slides"), Sense::hover());
            if response.dnd_hover_payload::<MediaDrag>().is_some() {
                let place = Rect::from_min_size(zone.min + vec2(INSET, 8.0), vec2(zone.width() - 2.0 * INSET, 70.0));
                ui.painter().rect_stroke(place, ROUND, Stroke::new(2.0, TEXT), StrokeKind::Inside);
                ui.painter().text(
                    place.center(),
                    Align2::CENTER_CENTER,
                    t("slides.drop_slide", "Drop to add as a slide"),
                    FontId::proportional(13.0),
                    TEXT_DIM,
                );
            }
            if let Some(dropped) = response.dnd_release_payload::<MediaDrag>() {
                state.media_request = Some(MediaRequest::AddAsSlide(dropped.0));
            }
        }
        // Under a playlist's last item: a faint line across, 1 point of the divider grey at 70%,
        // then nothing in the room under it.
        if let Some(y) = tail_line(&places, alone) {
            let y = ui.painter().round_to_pixel_center(sheet.y(y));
            let across_line = egui::Rangef::new(left + TAIL_IN, left + across - TAIL_IN);
            ui.painter().hline(across_line, y, Stroke::new(1.0, LINE.gamma_multiply(0.7)));
        }
        // The bars, after what is under them, so that a bar takes the clicks on it and covers
        // the slides that scroll under it. The bar of the item at the top of the view stays
        // there until the next item's bar comes up to it.
        let top = clip.min.y;
        let has_bar = |at: usize| places[at].bar > 0.0 && !matches!(decks[at], Deck::Heading { .. });
        let held = (0..decks.len()).rev().find(|at| has_bar(*at) && sheet.y(places[*at].top) <= top);
        for (at, deck) in decks.iter().enumerate() {
            if !has_bar(at) {
                continue;
            }
            let place = &places[at];
            let natural = sheet.y(place.top);
            let bar = if held == Some(at) {
                // Held at the top, and cut off where the next bar has come up to it.
                let next = (at + 1..decks.len()).find(|next| has_bar(*next)).map(|next| sheet.y(places[next].top));
                let bottom = next.map_or(top + place.bar, |next| next.min(top + place.bar));
                Rect::from_min_max(pos2(clip.min.x, top), pos2(clip.max.x, bottom))
            } else if natural > top && natural < clip.max.y {
                Rect::from_min_size(pos2(clip.min.x, natural), vec2(clip.width(), place.bar))
            } else {
                continue;
            };
            if bar.height() < 1.0 {
                continue;
            }
            let open = deck.item().is_some() && deck.item() == state.services.chosen;
            item_bar(ui, show, state, deck, bar, place.bar, &sheet, theme, open, commands, &mut ask, &mut asks);
        }
        sweep_slides(ui, state);
        // Asked for while the slide on screen is out of view: it is scrolled to, and its menu
        // opens when it is drawn, a frame or two later; not kept for longer.
        if let Some(asked) = state.live_menu {
            if ui.ctx().cumulative_frame_nr() > asked + 3 {
                state.live_menu = None;
            } else {
                ui.ctx().request_repaint();
            }
        }
    });
    asks.carry_out(show, state);
    if let Some(ask) = ask {
        crate::studio::carry_out_item_ask(show, state, commands, ask);
    }
}

/// The slides of one deck, as the view chosen draws them: only those in view.
#[allow(clippy::too_many_arguments)]
fn draw_slides(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    deck: &SlideDeck,
    place: &Place,
    body: &Body,
    sheet: &Sheet,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let song = &deck.deck;
    let view = sheet.grid.view;
    // An item set to show in a theme version has its tiles drawn in it.
    let version = deck
        .item
        .and_then(|at| state.services.front().items.get(at))
        .and_then(crate::service::Item::show_with)
        .map(|wanted| wanted.version)
        .filter(|id| state.versions.iter().any(|version| version.id == *id));
    if matches!(body, Body::Line) {
        let at = pos2(sheet.left + INSET, sheet.y(place.body_top()) + INSET + LIST_ROW / 2.0);
        ui.painter().text(
            at,
            Align2::LEFT_CENTER,
            t("slides.none", "No slides yet."),
            FontId::proportional(12.5),
            TEXT_FAINT,
        );
        return;
    }
    // The table's headings.
    if view == SlideView::Table {
        let middle = sheet.y(place.body_top()) + TABLE_HEAD / 2.0;
        for (x, words) in [
            (sheet.columns.group.min, t("slides.table.group", "Group")),
            (sheet.columns.words.min, t("slides.table.words", "Words")),
            (sheet.columns.notes.min, t("slides.table.notes", "Notes")),
        ] {
            crate::studio::paint_spaced(
                ui,
                pos2(x, middle),
                Align2::LEFT_CENTER,
                &words.to_uppercase(),
                fixed_semibold(11.0),
                TEXT_FAINT,
                0.6,
            );
        }
    }
    let live = deck.live_slide(show, state);
    // The slide picked to go on screen next by Enter: drawn picked out, never orange. A
    // playlist's passage picked is in a Bible page's place, which its tiles know by its name;
    // a slide picked at a place in the order sung is drawn at that place.
    let cue = state.pick(show);
    let passage_cue = cue.filter(|cue| deck.passage_holds(show, state, *cue)).map(|cue| cue.slide);
    let cue_step = state.picked.as_ref().and_then(|picked| picked.step);
    // The tiles in the order they stand: as written, or in the order sung.
    let order = entries(song);
    let arranged = song.active_arrangement().is_some();
    let live_at = live.and_then(|slide| {
        if arranged { live_entry(&order, slide, show.position().map(|(step, _)| step)) } else { Some(slide) }
    });
    // The group each slide is in, for a drag under an arrangement, which stays in its group.
    let group_of = |slide: usize| song.slide(slide).map(|(group, _)| group.id);
    // The line over the slides an arrangement leaves out.
    if let Some(line) = sheet.left_out_line(place, body).filter(|line| sheet.in_view(*line)) {
        crate::studio::paint_spaced(
            ui,
            pos2(line.min.x, line.center().y + 2.0),
            Align2::LEFT_CENTER,
            &t("slides.left_out", "Not in this arrangement \u{b7} still there to click").to_uppercase(),
            fixed_semibold(11.5),
            TEXT_FAINT,
            0.6,
        );
    }
    // Slides of this song being dragged to another place in it.
    let moving =
        deck.at.and_then(|at| egui::DragAndDrop::payload::<SlideDrag>(ui.ctx()).filter(|drag| drag.presentation == at));
    let mut moving_rects: Vec<Rect> = Vec::new();
    let list = view != SlideView::Grid;
    let mut last_group = None;
    let mut was_out = false;
    // A Bible passage's slides are named as its first Bible names verses; the bar under each
    // reads in the language the operator reads in (the Аа button), as the keypad and lists do.
    let bible_names = deck.bible.then(|| (state.bible.bibles_in_use(), state.bible.names_language()));
    for (at, entry) in order.iter().enumerate() {
        let index = entry.slide;
        let Some((group, slide)) = song.slide(index) else { continue };
        // A group's name is heavier on its first slide (the first left out starts afresh).
        if entry.out && !was_out {
            last_group = None;
        }
        was_out = entry.out;
        let first = last_group != Some(group.id);
        last_group = Some(group.id);
        let rect = sheet.slide(place, body, at);
        if !sheet.in_view(rect) {
            continue;
        }
        let here = deck.slide_ref(index);
        let id = ui.scope_id().with(("slide", deck.item, deck.at, at));
        let response = ui.interact(rect, id, Sense::click_and_drag());
        if moving.as_ref().is_some_and(|drag| drag.slides.contains(&index)) {
            moving_rects.push(rect);
        }
        let mods = ui.input(|input| input.modifiers);
        match here {
            Some(here) => {
                let which = here.presentation;
                if deck.made {
                    // Slides the app makes stay in the order they are made: a drag on one
                    // draws a box that picks out the slides it touches, as a drag between
                    // slides does.
                    start_slide_box(ui, state, &response);
                } else {
                    // A slide dragged goes to another place in its song, and the slides
                    // picked out with it when it is one of them; a line shows where. A slide
                    // sung again is dragged where it first stands.
                    if response.dragged() && !entry.again {
                        let mut picked: Vec<usize> =
                            state.picked_slides.iter().filter(|p| p.presentation == which).map(|p| p.slide).collect();
                        picked.sort_unstable();
                        let slides = if picked.contains(&index) { picked } else { vec![index] };
                        response.dnd_set_drag_payload(SlideDrag { presentation: which, slides });
                    }
                    // Under an arrangement the groups stand where it puts them: a slide moves
                    // only among its own group.
                    let same_group = |drag: &SlideDrag| {
                        !arranged || drag.slides.iter().all(|slide| group_of(*slide) == Some(group.id))
                    };
                    let target = |drag: &SlideDrag| {
                        drag.presentation == which && !drag.slides.contains(&index) && same_group(drag)
                    };
                    if response.dnd_hover_payload::<SlideDrag>().is_some_and(|drag| target(&drag)) {
                        let after = drop_after(ui, rect, list);
                        paint_drop_line(ui, rect, after, list, if list { LIST_GAP } else { GAP_X });
                    }
                    if let Some(dropped) = response.dnd_release_payload::<SlideDrag>().filter(|drag| target(drag)) {
                        let after = drop_after(ui, rect, list);
                        state.slide_request = Some((
                            SlideRef { presentation: which, slide: dropped.slides[0] },
                            SlideRequest::Move { slides: dropped.slides.clone(), to: index, after },
                        ));
                    }
                    // A media item dragged from the bin onto a slide becomes what it shows.
                    if response.dnd_hover_payload::<MediaDrag>().is_some() {
                        ui.painter().rect_stroke(rect, ROUND, Stroke::new(2.0, TEXT), StrokeKind::Outside);
                    }
                    if let Some(dropped) = response.dnd_release_payload::<MediaDrag>()
                        && let Some(item) = show.media().get(dropped.0)
                    {
                        state.pending_edit = Some((here, slide_with_media(slide, Some(item.id))));
                    }
                }
                // What a plain click does, by Settings › Keys' Show a slide with: puts the slide on
                // screen, picks it for Enter, or nothing more (`slide_click_does`).
                let plain = response.clicked() && !(mods.command || mods.shift);
                let does = plain.then(|| click_does(state, &response, live.is_some(), live == Some(index)));
                // A click on a slide takes over from one picked by the keys.
                if response.clicked() {
                    state.picked = None;
                }
                if response.clicked() && (mods.command || mods.shift) {
                    // Picked out without going on screen, to change several at once.
                    state.picking = Picking::Slides;
                    let order: Vec<SlideRef> = state.slide_rects.iter().map(|(at, _)| *at).collect();
                    let anchor = state.selected_slide.or(show.live()).filter(|at| at.presentation == which);
                    pick_slide(&mut state.picked_slides, &order, here, mods, anchor);
                } else if response.clicked() {
                    state.selected_slide = None;
                    state.picked_slides.clear();
                    // Delete no longer means the songs or media last picked out.
                    state.picking = Picking::Nothing;
                    match does {
                        // Under an arrangement, the song goes to that place in the order sung.
                        Some(ClickDoes::Show) => match entry.step {
                            Some(step) => {
                                commands.push(Command::GoToStep { presentation: which, step });
                                if deck.item.is_some() {
                                    state.services.chosen = deck.item;
                                    state.live_item = deck.item;
                                }
                            }
                            None => deck.show_slide(show, state, index, commands),
                        },
                        Some(ClickDoes::Pick) => deck.pick_slide(show, state, here, entry.step, commands),
                        Some(ClickDoes::Nothing) | None => {}
                    }
                }
                if response.secondary_clicked() {
                    state.selected_slide = Some(here);
                }
            }
            // A playlist's Bible passage: a click puts the slide on screen, and that is all. Set to
            // a double-click, a click on one not on screen looks it up and picks the slide, for
            // Enter, and a double-click puts the slide on.
            None => {
                if response.clicked() {
                    let does = click_does(state, &response, live.is_some(), live == Some(index));
                    state.picked = None;
                    state.selected_slide = None;
                    state.picked_slides.clear();
                    state.picking = Picking::Nothing;
                    match does {
                        ClickDoes::Show => deck.show_slide(show, state, index, commands),
                        ClickDoes::Pick => deck.pick_passage_slide(show, state, index, commands),
                        ClickDoes::Nothing => {}
                    }
                }
            }
        }
        let near = ui.rect_contains_pointer(rect);
        // The key that jumps to a group is said on the group's first slide.
        if first
            && !deck.made
            && let Some(key) = sp_model::key_for_group(&group.name)
        {
            response.clone().hint(tf(
                "slides.group_key",
                "{group} (key {key})",
                &[("group", &group.name), ("key", &key.to_string())],
            ));
        } else {
            response.clone().hint(t("quickhelp.slides.slide", "Put this slide on screen."));
        }
        let is_live = live_at == Some(at);
        // The slide that waits for Enter: in ink, never orange, and not the blue of slides
        // picked out to change several at once.
        let picked_here = match here {
            Some(here) => cue == Some(here),
            None => passage_cue == Some(index),
        };
        let at_its_place = match cue_step {
            Some(step) => entry.step == Some(step),
            None => !entry.again,
        };
        let cued = !is_live && picked_here && at_its_place;
        let picked =
            !cued && here.is_some_and(|here| state.selected_slide == Some(here) || state.picked_slides.contains(&here));
        // A slide of the passage asked for, among its chapter's on the Bible page: a fine faint
        // ring, under the rings of what is on screen, picked or chosen.
        let ringed = !is_live && !cued && !picked && deck.ringed.as_ref().is_some_and(|ringed| ringed.contains(&index));
        // The slide that was on screen before it was cleared: Next goes on from it.
        let was_live = show.live().is_none() && here.is_some() && show.cursor() == here;
        // Numbered in the order sung, under an arrangement.
        let number = format!("{:02}", entry.step.unwrap_or(index) + 1);
        let name = strip_name(group, slide);
        let name = match &bible_names {
            Some((bibles, language)) => {
                let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
                crate::bible::slide_name_for_operator(&name, &in_use, language)
            }
            None => name,
        };
        // Slides the app makes (Bible passages, live captions) have an ink dot, as the Bible's
        // mark in the find box is ink; a group's colour is the church's own.
        let dot = if is_live {
            TALLY
        } else if deck.made {
            crate::studio::TEXT
        } else {
            dot_colour(group.color)
        };
        // The "more" button and, on a song's slide, the pencil: where they go.
        let (more, pencil) = if list {
            let more =
                Rect::from_center_size(pos2(sheet.columns.more, rect.center().y), egui::Vec2::splat(MORE_BUTTON));
            (more, None)
        } else {
            let middle = rect.max.y - STRIP / 2.0;
            let more = Rect::from_center_size(
                pos2(rect.max.x - 3.0 - MORE_BUTTON / 2.0, middle),
                egui::Vec2::splat(MORE_BUTTON),
            );
            let pencil = (!deck.made && here.is_some()).then(|| more.translate(vec2(-MORE_BUTTON, 0.0)));
            (more, pencil)
        };
        // Slides the app makes (a Bible passage, VoxLive's captions) have no menu: nothing in
        // it applies to them, and they are never edited.
        let has_menu = here.is_some() && !deck.made;
        match view {
            SlideView::Grid => {
                let picture = Rect::from_min_size(rect.min, sheet.grid.picture);
                ui.painter().rect_filled(picture, ROUND, Color32::BLACK);
                let of = match (deck.bible, deck.at, version) {
                    (true, _, Some(version)) => ThumbOf::InVersion(None, slide, version),
                    (true, _, None) => ThumbOf::BibleSlide(slide),
                    (false, Some(at), Some(version)) => ThumbOf::InVersion(Some(at), slide, version),
                    (false, Some(at), None) => ThumbOf::SongSlide(at, slide),
                    (false, None, _) => ThumbOf::Slide(slide),
                };
                if let Some(page) = state.pages_waiting.get(&slide.id) {
                    // Its picture isn't drawn yet: the page it will be, faint on the dark.
                    ui.painter().text(
                        picture.center(),
                        Align2::CENTER_CENTER,
                        tf("slides.page_waiting", "Page {page}\u{2026}", &[("page", &page.to_string())]),
                        FontId::proportional(14.0),
                        TEXT_FAINT,
                    );
                } else {
                    match thumb(of) {
                        Some((texture, overflow)) => {
                            egui::Image::from_texture((texture, picture.size()))
                                .corner_radius(ROUND)
                                .paint_at(ui, picture);
                            if overflow {
                                // Its words don't fit their box: take care.
                                let mark = picture.right_top() + vec2(-14.0, 14.0);
                                ui.painter().circle_filled(mark, 9.0, crate::studio::CARE);
                                ui.painter().text(mark, Align2::CENTER_CENTER, "!", semibold(13.0), Color32::BLACK);
                            }
                        }
                        None => ui.ctx().request_repaint(),
                    }
                }
                // A slide switched off stays in the song but is stepped over: it is dimmed.
                if !slide.enabled {
                    ui.painter().rect_filled(picture, ROUND, PANEL.gamma_multiply(0.65));
                }
                if is_live {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, TALLY), StrokeKind::Outside);
                } else if cued {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(2.0, TEXT), StrokeKind::Outside);
                } else if picked {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, PICKED), StrokeKind::Outside);
                } else if was_live {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Outside);
                } else if ringed {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(2.0, TEXT_FAINT), StrokeKind::Outside);
                }
                // The strip: the number, the group's dot and name, small tags for what the
                // slide does, then the pencil and the "more" button.
                let middle = rect.max.y - STRIP / 2.0;
                let ink = if is_live { TALLY } else { TEXT };
                let drawn = crate::studio::paint_spaced(
                    ui,
                    pos2(rect.min.x + 3.0, middle),
                    Align2::LEFT_CENTER,
                    &number,
                    fixed_semibold(12.0),
                    ink,
                    0.5,
                );
                let strength = if first || is_live { 1.0 } else { 0.45 };
                ui.painter().circle_filled(pos2(drawn.max.x + 7.0 + 4.0, middle), 4.0, dot.gamma_multiply(strength));
                let mut right =
                    pencil.map_or(if has_menu { more.min.x } else { rect.max.x - 3.0 }, |pencil| pencil.min.x) - 4.0;
                let mut tags = slide_tags(slide);
                if entry.again {
                    tags.insert(0, Tag::new(t("slides.tag.again", "Again").to_uppercase(), TagLook::Plain));
                }
                for tag in tags.into_iter().rev() {
                    let (fill, ink) = match tag.look {
                        TagLook::Check => (crate::studio::CARE, Color32::from_gray(0x11)),
                        TagLook::Key => (crate::studio::BUTTON, TEXT),
                        TagLook::Plain => (CARD, TEXT_DIM),
                    };
                    let galley = crate::studio::spaced(ui, &tag.words, fixed_semibold(11.0), ink, 0.3);
                    let place = Rect::from_min_max(
                        pos2(right - galley.size().x - 8.0, middle - 8.5),
                        pos2(right, middle + 8.5),
                    );
                    if place.min.x < drawn.max.x + 40.0 {
                        break;
                    }
                    ui.painter().rect_filled(place, 4.0, fill);
                    ui.painter().galley(pos2(place.min.x + 4.0, middle - galley.size().y / 2.0), galley, ink);
                    if let Some(hover) = tag.hover {
                        // Only hovering: a click on the tag is still a click on the slide.
                        let id = ui.scope_id().with(("tag", slide.id, tag.words.clone()));
                        ui.interact(place, id, Sense::hover()).full_name(hover);
                    }
                    right = place.min.x - 4.0;
                }
                let (font, ink) = if is_live {
                    (if first { fixed_semibold(12.0) } else { fixed(12.0) }, TALLY)
                } else if first || cued {
                    (fixed_semibold(12.0), TEXT)
                } else {
                    (fixed(12.0), TEXT_FAINT)
                };
                let label = Rect::from_min_max(pos2(drawn.max.x + 22.0, rect.max.y - STRIP), pos2(right, rect.max.y));
                let galley = crate::studio::spaced(ui, &name.to_uppercase(), font, ink, 0.5);
                ui.painter().with_clip_rect(label).galley(
                    pos2(label.min.x, middle - galley.size().y / 2.0),
                    galley,
                    ink,
                );
            }
            SlideView::Easy | SlideView::Table => {
                let fill = if is_live {
                    Some(TALLY.gamma_multiply(0.2))
                } else if cued {
                    Some(CARD)
                } else if picked {
                    Some(PICKED.gamma_multiply(0.2))
                } else if near {
                    Some(ROW_HOVER)
                } else {
                    None
                };
                if let Some(fill) = fill {
                    ui.painter().rect_filled(rect, ROUND, fill);
                }
                // The row that waits for Enter: the well, with a fine dim ring inside; a row of the
                // passage among its chapter's, a fainter ring.
                if cued {
                    ui.painter().rect_stroke(rect, ROUND, Stroke::new(1.5, TEXT_DIM), StrokeKind::Inside);
                } else if ringed {
                    ui.painter().rect_stroke(rect, ROUND, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Inside);
                }
                let columns = &sheet.columns;
                let middle = rect.center().y;
                let ink = if is_live { TALLY } else { TEXT_DIM };
                ui.painter().text(pos2(columns.number, middle), Align2::LEFT_CENTER, &number, fixed(13.0), ink);
                let strength = if first || is_live { 1.0 } else { 0.45 };
                ui.painter().circle_filled(pos2(columns.dot, middle), 4.0, dot.gamma_multiply(strength));
                let dim = if slide.enabled { 1.0 } else { 0.45 };
                let words = ui.painter().layout(
                    slide_words(slide),
                    columns.font.clone(),
                    TEXT.gamma_multiply(dim),
                    columns.words.span().max(20.0),
                );
                let at = pos2(columns.words.min, middle - words.size().y / 2.0);
                ui.painter().with_clip_rect(rect).galley(at, words, TEXT);
                if view == SlideView::Table {
                    let clip =
                        Rect::from_min_max(pos2(columns.group.min, rect.min.y), pos2(columns.group.max, rect.max.y));
                    ui.painter().with_clip_rect(clip).text(
                        pos2(columns.dot + 12.0, middle),
                        Align2::LEFT_CENTER,
                        &name,
                        FontId::proportional(13.0),
                        if first { TEXT_DIM } else { TEXT_FAINT },
                    );
                    if let Some(notes) = slide.notes.as_deref().filter(|notes| !notes.is_empty()) {
                        let clip = Rect::from_min_max(
                            pos2(columns.notes.min, rect.min.y),
                            pos2(columns.notes.max, rect.max.y),
                        );
                        let notes = ui.painter().layout(
                            notes.replace('\n', " / "),
                            FontId::proportional(13.0),
                            TEXT_DIM,
                            columns.notes.span().max(20.0),
                        );
                        let top = (middle - notes.size().y / 2.0).max(rect.min.y + 4.0);
                        ui.painter().with_clip_rect(clip).galley(pos2(columns.notes.min, top), notes, TEXT_DIM);
                    }
                } else {
                    let clip =
                        Rect::from_min_max(pos2(columns.group.min, rect.min.y), pos2(columns.group.max, rect.max.y));
                    let ink = if is_live { TALLY } else { TEXT_FAINT };
                    let galley = crate::studio::spaced(ui, &name.to_uppercase(), fixed(12.0), ink, 0.6);
                    let at = pos2(
                        (columns.group.max - galley.size().x).max(columns.group.min),
                        middle - galley.size().y / 2.0,
                    );
                    ui.painter().with_clip_rect(clip).galley(at, galley, ink);
                }
            }
        }
        // A slide found with the find box: an ink ring for a moment.
        if let (Some(here), Some((found, since))) = (here, state.spotlight.as_mut())
            && here == *found
            && !entry.again
        {
            let now = ui.input(|input| input.time);
            let since = *since.get_or_insert(now);
            if now - since < SPOTLIGHT_FOR {
                let ring = if list { rect } else { Rect::from_min_size(rect.min, sheet.grid.picture) };
                ui.painter().rect_stroke(ring, ROUND, Stroke::new(3.0, TEXT), StrokeKind::Outside);
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SPOTLIGHT_FOR - (now - since)));
            } else {
                state.spotlight = None;
            }
        }
        // Under an arrangement: a slide sung again is a little faint, and one it leaves out more
        // so (both still take clicks); not while on screen.
        let faint = if entry.out {
            0.5
        } else if entry.again {
            0.22
        } else {
            0.0
        };
        if faint > 0.0 && !is_live {
            ui.painter().rect_filled(rect.expand(1.0), ROUND, PANEL.gamma_multiply(faint));
        }
        // The pencil opens the slide in the editor; the "more" button opens what a
        // right-click does. Made after the slide itself, so a click on one of them is theirs
        // and never puts the slide on screen.
        if let (Some(place), Some(here)) = (pencil, here) {
            let strength = if near { 1.0 } else { 0.5 };
            let button = icon_button(ui, place, id.with("pencil"), Icon::Pencil, strength, false);
            if button.hint(t("slides.edit_this", "Edit this slide")).clicked() {
                state.editing = EditDraft::of(show, here);
            }
        }
        if let Some(here) = here.filter(|_| has_menu) {
            let button = more_button(ui, more, id.with("more"), near);
            // Modifier-O: the menu of the slide on screen opens as if its button were clicked.
            if is_live && state.live_menu.take().is_some() {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&button));
            }
            egui::Popup::menu(&button).show(|ui| slide_menu(ui, show, state, song, here, slide));
            response.context_menu(|ui| slide_menu(ui, show, state, song, here, slide));
        }
    }
    // The slides being moved are dimmed where they are, and one of them follows the pointer,
    // with how many there are.
    if let (Some(drag), Some(at)) = (moving, deck.at) {
        for rect in &moving_rects {
            ui.painter().rect_filled(*rect, ROUND, PANEL.gamma_multiply(0.6));
        }
        let first = drag.slides.first().and_then(|at| song.slide(*at)).map(|(_, slide)| slide);
        let texture = first.and_then(|slide| {
            thumb(if deck.bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::SongSlide(at, slide) })
        });
        paint_drag_ghost(ui, texture.map(|(texture, _)| texture), drag.slides.len());
    }
}

/// What a slide's strip names it: its group, and its label after a dot when it has one (the
/// label alone in a group with no name). One of the app's own labels, which a song keeps in
/// English, is said in the language set, as the Label menus say it.
pub(super) fn strip_name(group: &sp_model::Group, slide: &Slide) -> String {
    match slide.label.as_deref().filter(|label| !label.is_empty()).map(super::slide_label_words) {
        Some(label) if group.name.is_empty() => label.to_owned(),
        Some(label) => format!("{} \u{b7} {label}", group.name),
        None => group.name.clone(),
    }
}

/// The colour of a group's dot: the group's own, or a grey-blue when that is too dark to see
/// on the panel (a Bible passage's groups have no colour of their own).
pub(super) fn dot_colour(colour: Rgba) -> Color32 {
    let light = 0.299 * f32::from(colour.r) + 0.587 * f32::from(colour.g) + 0.114 * f32::from(colour.b);
    if light < 60.0 { Color32::from_rgb(0x6b, 0x7f, 0x99) } else { color32(colour) }
}

/// How a small tag in a slide's strip is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TagLook {
    /// Grey on a card.
    Plain,
    /// A key, on a button's grey.
    Key,
    /// Amber, black words: the slide came in from a presentation and needs a look.
    Check,
}

/// One small tag in a slide's strip.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct Tag {
    pub(super) words: String,
    pub(super) look: TagLook,
    /// What the tag says when the pointer rests on it.
    pub(super) hover: Option<String>,
}

impl Tag {
    fn new(words: impl Into<String>, look: TagLook) -> Tag {
        Tag { words: words.into(), look, hover: None }
    }
}

/// The small tags in a slide's strip for what the slide does: its hot key, the time after which
/// it goes on by itself, how many actions it has, its own transition, that it is switched off,
/// and last (so it is the one nearest the pencil, and the last to go when there is no room) that
/// it came in from a presentation and is to be checked, with why.
pub(super) fn slide_tags(slide: &Slide) -> Vec<Tag> {
    let mut tags = Vec::new();
    if let Some(key) = slide.hot_key {
        tags.push(Tag::new(key.to_string(), TagLook::Key));
    }
    if let Some(seconds) = slide.advance {
        tags.push(Tag::new(format!("{seconds:.0}S"), TagLook::Plain));
    }
    if !slide.actions.is_empty() {
        tags.push(Tag::new(
            tf("slides.tag.actions", "{n} act", &[("n", &slide.actions.len().to_string())]).to_uppercase(),
            TagLook::Plain,
        ));
    }
    if let Some(own) = slide.transition {
        tags.push(Tag::new(crate::studio::transition_name(own.kind).to_uppercase(), TagLook::Plain));
    }
    if !slide.enabled {
        tags.push(Tag::new(t("slides.tag.off", "Off").to_uppercase(), TagLook::Plain));
    }
    if let Some(why) = slide.check.as_deref() {
        let hover = tf(
            "slides.tag.check.hint",
            "Check this slide: {why}. Mark as Checked, in its menu, takes this tag off.",
            &[("why", why)],
        );
        tags.push(Tag {
            hover: Some(hover),
            ..Tag::new(t("slides.tag.check", "Check").to_uppercase(), TagLook::Check)
        });
    }
    tags
}

/// The one tile of a picture or video or of a media playlist, the size of a slide: its
/// picture (a media playlist's first few, one behind another) with a line of words in the
/// strip under it. Ringed in orange, with orange words, while it is on the screens.
#[allow(clippy::too_many_arguments)]
fn item_tile(
    ui: &mut egui::Ui,
    sheet: &Sheet,
    rect: Rect,
    id: (&'static str, usize),
    pictures: &[Option<TextureId>],
    words: &str,
    live: bool,
    length: Option<f64>,
) -> egui::Response {
    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
    let picture = Rect::from_min_size(rect.min, sheet.grid.picture);
    ui.painter().rect_filled(picture, ROUND, Color32::BLACK);
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    match pictures {
        [] => {}
        [one] => match one {
            Some(texture) => {
                egui::Image::from_texture((*texture, picture.size())).corner_radius(ROUND).paint_at(ui, picture);
            }
            None => ui.ctx().request_repaint(),
        },
        several => {
            // One behind another, the first in front at the lower left.
            let step = vec2(picture.width(), picture.height()) * 0.07;
            for (place, texture) in several.iter().enumerate().rev() {
                let n = place as f32;
                let behind = (several.len() - 1 - place) as f32;
                let card = Rect::from_min_max(
                    picture.min + vec2(n * step.x, behind * step.y),
                    picture.max - vec2(behind * step.x, n * step.y),
                );
                ui.painter().rect_filled(card, 4.0, Color32::from_gray(24));
                match texture {
                    Some(texture) => {
                        ui.painter().image(*texture, card, whole, Color32::WHITE);
                    }
                    None => ui.ctx().request_repaint(),
                }
                ui.painter().rect_stroke(
                    card,
                    4.0,
                    Stroke::new(1.0, Color32::from_white_alpha(30)),
                    StrokeKind::Inside,
                );
            }
        }
    }
    // A video says how long it is in its corner, as in the media strip.
    if let Some(seconds) = length {
        crate::studio::length_chip(ui.painter(), picture, seconds);
    }
    if live {
        ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, TALLY), StrokeKind::Outside);
    }
    let ink = if live { TALLY } else { TEXT };
    let strip = Rect::from_min_max(pos2(rect.min.x + 3.0, picture.max.y), pos2(rect.max.x - 3.0, rect.max.y));
    let mut galley = crate::studio::spaced(ui, &words.to_uppercase(), fixed_semibold(12.0), ink, 0.5);
    // Too long for the line: what comes before " · " (the count, without what a click does).
    if galley.size().x > strip.width()
        && let Some((first, _)) = words.split_once(" \u{b7} ")
    {
        galley = crate::studio::spaced(ui, &first.to_uppercase(), fixed_semibold(12.0), ink, 0.5);
    }
    ui.painter().with_clip_rect(strip).galley(pos2(strip.min.x, strip.center().y - galley.size().y / 2.0), galley, ink);
    response
}

/// A placeholder's box: an outline the size of a slide's picture with "Choose a song…" in
/// it, which opens the list of songs to fill the placeholder with; a picture or video
/// dropped on it takes its place.
fn placeholder_tile(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    item: usize,
    tile: Rect,
    commands: &mut Vec<Command>,
) {
    let response = ui.interact(tile, ui.scope_id().with(("placeholder", item)), Sense::click());
    let carried = response.dnd_hover_payload::<MediaDrag>().is_some();
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let hot = response.hovered() || open;
    let (edge, ink) = if carried {
        (Stroke::new(2.0, TEXT), TEXT)
    } else if hot {
        (Stroke::new(1.0, TEXT_FAINT), TEXT)
    } else {
        (Stroke::new(1.0, LINE), TEXT_DIM)
    };
    ui.painter().rect_stroke(tile, ROUND, edge, StrokeKind::Inside);
    let words = ui.painter().layout_no_wrap(
        t("slides.choose_song", "Choose a song\u{2026}").to_owned(),
        FontId::proportional(13.0),
        ink,
    );
    let left = tile.center().x - (words.size().x + 20.0) / 2.0;
    crate::icons::paint(
        ui.painter(),
        Icon::Plus,
        Rect::from_center_size(pos2(left + 6.0, tile.center().y), vec2(12.0, 12.0)),
        ink,
    );
    ui.painter().galley(pos2(left + 20.0, tile.center().y - words.size().y / 2.0), words, ink);
    // A picture or video dropped on it takes its place.
    if let Some(dropped) = response.dnd_release_payload::<MediaDrag>()
        && let Some(media) = show.media().get(dropped.0)
    {
        let filled = Item::Media { id: media.id, name: media.name.clone() };
        crate::studio::fill_placeholder(state, item, filled);
    }
    // Or a song chosen from a list.
    let response = response.hint(t("slides.placeholder.hint", "Fill the placeholder with a song from the library"));
    let mut picked = None;
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| picked = crate::studio::song_picker(ui, show, state));
    if let Some(song) = picked
        && let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song))
    {
        let filled = Item::song(key.clone(), found.title.clone());
        if crate::studio::fill_placeholder(state, item, filled) {
            commands.push(Command::Select(song));
        }
    }
}

/// A small round label after an item's name, saying what kind of thing it is.
fn pill(ui: &egui::Ui, painter: &egui::Painter, at: egui::Pos2, words: &str) -> Rect {
    let galley = crate::studio::spaced(ui, words, fixed_semibold(11.0), TEXT_DIM, 0.5);
    let rect = Rect::from_min_size(pos2(at.x, at.y - 10.0), vec2(galley.size().x + 18.0, 20.0));
    painter.rect_filled(rect, 10.0, CARD);
    painter.galley(pos2(rect.min.x + 9.0, at.y - galley.size().y / 2.0), galley, TEXT_DIM);
    rect
}

/// The bar over an item: a dot when it is the open item, its name, then either a small label
/// saying what it is or, for a song, its tools (details, words as text, arrangement, notes,
/// more), and a hairline to the right. `bar` is where it is drawn (cut short when the next
/// item's bar has come up to it); `height` is its whole height.
#[allow(clippy::too_many_arguments)]
fn item_bar(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    deck: &Deck,
    bar: Rect,
    height: f32,
    sheet: &Sheet,
    theme: Option<&Theme>,
    open: bool,
    commands: &mut Vec<Command>,
    ask: &mut Option<ItemAsk>,
    asks: &mut SongAsks,
) {
    // Its own grey, over the slides that scroll under it.
    ui.painter().rect_filled(bar, 0.0, PANEL);
    let item = deck.item();
    let slides = match deck {
        Deck::Slides(slides) => Some(slides),
        _ => None,
    };
    // The song whose tools the bar carries: one the show holds and the app didn't make.
    let song = slides.filter(|slides| !slides.made).and_then(|slides| Some((slides.at?, &slides.deck)));
    // Or a song of the song catalog being looked at, as made, whose tools put it to use.
    let catalog = slides.and_then(|slides| state.catalog.held_at(slides.at?)).cloned();
    let id = ui.scope_id().with(("bar", item, slides.and_then(|slides| slides.at)));
    let response = ui
        .interact(bar, id, Sense::click())
        .hint(t("quickhelp.slides.bar", "This item\u{2019}s name and tools. Its menu is on a right-click."));
    // A picture or video dropped on a song's bar becomes a slide of its own at the song's end
    // (after the slide on screen, when that is in it); an ink outline shows it will.
    if let Some((at, _)) = song {
        if response.dnd_hover_payload::<MediaDrag>().is_some() {
            ui.painter().rect_stroke(bar.shrink(2.0), ROUND, Stroke::new(2.0, TEXT), StrokeKind::Inside);
        }
        if let Some(dropped) = response.dnd_release_payload::<MediaDrag>() {
            if show.selected() != at {
                commands.push(Command::Select(at));
            }
            state.media_request = Some(MediaRequest::AddAsSlide(dropped.0));
        }
    }
    let near = ui.rect_contains_pointer(bar);
    let painter = ui.painter().with_clip_rect(bar);
    let middle = bar.min.y + if height >= SONG_BAR { 30.0 } else { 25.5 };
    let right = sheet.left + sheet.grid.width - INSET;
    let mut x = sheet.left + INSET;
    if open {
        painter.circle_filled(pos2(x + 3.0, middle), 3.0, TEXT);
        x += 14.0;
    }
    let (title, kind, faint) = match deck {
        Deck::Slides(slides) if slides.bible => {
            (slides.deck.title.as_str(), Some(t("slides.kind.bible", "Bible")), false)
        }
        // Where it is from, by name: it isn't the church's yet.
        Deck::Slides(slides) if catalog.is_some() => {
            (slides.deck.title.as_str(), Some(crate::catalog::SOURCE_NAME), false)
        }
        Deck::Slides(slides) if slides.made => {
            (slides.deck.title.as_str(), Some(t("slides.kind.captions", "Live captions")), false)
        }
        Deck::Slides(slides) => {
            (slides.deck.title.as_str(), slides.deck.loops.then_some(t("slides.kind.loops", "Loops")), false)
        }
        Deck::Media { at, name, .. } => {
            let video = at.is_some_and(|at| show.media().get(at).is_some_and(|m| crate::media::is_video(&m.path)));
            let kind = match at {
                None => t("slides.kind.gone", "Gone"),
                Some(_) if video => t("slides.kind.video", "Video"),
                Some(_) => t("slides.kind.picture", "Picture"),
            };
            (name.as_str(), Some(kind), at.is_none())
        }
        Deck::MediaPlaylist { name, gone, .. } => {
            (name.as_str(), Some(t("slides.kind.media_playlist", "Media playlist")), *gone)
        }
        Deck::Placeholder { name, .. } => (name.as_str(), Some(t("slides.kind.placeholder", "Placeholder")), false),
        Deck::Note { title, kind, .. } => {
            let kind = match *kind {
                "Bible" => t("slides.kind.bible", "Bible"),
                "Missing" => t("slides.kind.missing", "Missing"),
                other => other,
            };
            (title.as_str(), Some(kind), true)
        }
        Deck::Heading { name, .. } => (name.as_str(), None, false),
    };
    // The name; for the song chosen a double click turns it into a field to retype it.
    let mut name_response = None;
    let font = semibold(14.0);
    let renaming = state.renaming_song.is_some() && song.is_some_and(|(at, _)| at == show.selected());
    if renaming && let (Some(typed), Some((at, song))) = (state.renaming_song.as_mut(), song) {
        // The field takes the room the title had, so the tools after it stay where they were;
        // a longer title scrolls inside it.
        let shown = if song.title.is_empty() { t("left.untitled", "Untitled") } else { song.title.as_str() };
        let wide = painter.layout_no_wrap(shown.to_owned(), font.clone(), TEXT).size().x;
        let look = crate::studio::Lettering::plain(font, TEXT);
        let field = crate::studio::rename_field_at(ui, typed, &look, pos2(x, middle), (x + wide).min(right));
        if std::mem::take(&mut state.rename_started) {
            field.request_focus();
        }
        let (enter, escape) =
            ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
        if escape {
            state.renaming_song = None;
        } else if enter || field.lost_focus() {
            if !typed.trim().is_empty() && typed.trim() != song.title {
                state.pending_title = Some((at, typed.trim().to_owned()));
            }
            state.renaming_song = None;
        }
        x += wide;
    } else {
        let ink = if faint { TEXT_FAINT } else { TEXT };
        let words = if title.is_empty() { t("left.untitled", "Untitled") } else { title };
        let drawn = painter.text(pos2(x, middle), Align2::LEFT_CENTER, words, font, ink);
        if let Some((at, song)) = song {
            let slides = song.slide_count();
            let name = ui.interact(drawn.intersect(bar), id.with("name"), Sense::click()).hint(tn(
                "slides.name.hint",
                slides as u64,
                "{n} slide \u{b7} double-click to rename",
                "{n} slides \u{b7} double-click to rename",
                &[],
            ));
            if name.double_clicked() {
                state.renaming_song = Some(song.title.clone());
                state.rename_started = true;
                if at != show.selected() {
                    commands.push(Command::Select(at));
                }
            }
            if name.clicked() && item.is_some() {
                state.services.chosen = item;
            }
            // A right-click on the name opens the bar's menu, as anywhere else on the bar.
            name_response = Some(name);
        }
        x = drawn.max.x;
    }
    if let Some(kind) = kind {
        let drawn = pill(ui, &painter, pos2(x + 10.0, middle), kind);
        x = drawn.max.x;
        // A catalog song's says what becomes of it.
        if catalog.is_some() {
            let hint = tf(
                "slides.catalog.hint",
                "From {source}, not in your library: it joins it, in {library}, once you add it to a playlist or change it. Putting it on screen adds nothing.",
                &[("source", crate::catalog::SOURCE_NAME), ("library", crate::catalog::LIBRARY_NAME)],
            );
            ui.interact(drawn.intersect(bar), id.with("catalog"), Sense::hover()).status(hint);
        }
    }
    // A passage's version button, after its kind.
    if let (Deck::Slides(slides), Some(item)) = (deck, item)
        && slides.bible
        && crate::studio::can_show_with(state, item)
    {
        let rect = Rect::from_center_size(pos2(x + 10.0 + ICON_BUTTON / 2.0, middle), egui::Vec2::splat(ICON_BUTTON));
        x = rect.max.x;
        version_button(ui, show, state, rect.intersect(bar), id.with("version"), item, if near { 1.0 } else { 0.6 });
    }
    // What the "more" button and a right-click on the bar open: a playlist item's one menu, as
    // its row has it; for a song shown by itself (from a library), Add Slide and its menu.
    let mut menu = |ui: &mut egui::Ui, state: &mut HomeState| match (item, song) {
        (Some(item), _) => crate::studio::item_menu(ui, show, state, commands, item, ask, asks),
        (None, Some((at, _))) => {
            if ui
                .button(t("slides.add_slide", "Add Slide"))
                .hint(t("slides.add_slide.hint", "Add a slide laid out like the last one"))
                .clicked()
            {
                state.add_slide = Some(at);
                ui.close();
            }
            ui.separator();
            crate::studio::song_menu(ui, show, state, commands, at, asks, None);
        }
        (None, None) => {
            if let Some(held) = &catalog {
                catalog_menu(ui, state, held);
            }
        }
    };
    // A catalog song's tools, each of which makes it join the library: Add to the playlist in
    // front, and Edit Words; the rest in its menu.
    if let Some(held) = &catalog {
        let strength = if near { 1.0 } else { 0.6 };
        x += 10.0;
        let mut place = || {
            let rect = Rect::from_center_size(pos2(x + ICON_BUTTON / 2.0, middle), egui::Vec2::splat(ICON_BUTTON));
            x += ICON_BUTTON + 1.0;
            rect.intersect(bar)
        };
        let front = state.services.front().name.clone();
        let library: &[(&str, &str)] = &[("list", &front), ("library", crate::catalog::LIBRARY_NAME)];
        let add_hint =
            tf("slides.catalog.add.hint", "Add to {list}: the song joins your library, in {library}", library);
        let words_hint = tf(
            "slides.catalog.words.hint",
            "Edit Words: the song joins your library, in {library}, to be changed",
            library,
        );
        if icon_button(ui, place(), id.with("catalog add"), Icon::Plus, strength, false).hint(add_hint).clicked() {
            catalog_add(state, held);
        }
        if icon_button(ui, place(), id.with("catalog words"), Icon::Reflow, strength, false).hint(words_hint).clicked()
        {
            state.catalog.join(held, crate::catalog_songs::Then::Words);
        }
        let more = more_button(ui, place(), id.with("more"), near);
        egui::Popup::menu(&more).show(|ui| menu(ui, state));
    }
    if let Some((at, song)) = song {
        // The song's tools: at part strength until the pointer is on the bar.
        let strength = if near { 1.0 } else { 0.6 };
        x += 10.0;
        let mut place = || {
            let rect = Rect::from_center_size(pos2(x + ICON_BUTTON / 2.0, middle), egui::Vec2::splat(ICON_BUTTON));
            x += ICON_BUTTON + 1.0;
            rect.intersect(bar)
        };
        let info = icon_button(ui, place(), id.with("details"), Icon::Status, strength, false)
            .hint(t("slides.details", "Details: the song's writer, publisher and CCLI number"));
        egui::Popup::from_toggle_button_response(&info)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(320.0);
                let mut details = song.copyright.clone();
                egui::Grid::new("song info").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    for (label, value) in [
                        (t("slides.details.artist", "Artist"), &mut details.artist),
                        (t("slides.details.author", "Author"), &mut details.author),
                        (t("slides.details.publisher", "Publisher"), &mut details.publisher),
                        (t("slides.details.year", "Year"), &mut details.year),
                        (t("slides.details.ccli", "CCLI Song #"), &mut details.ccli_song),
                    ] {
                        ui.label(egui::RichText::new(label).color(DIM));
                        ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(t(
                        "slides.details.copyright",
                        "Settings, General says when the copyright line is shown.",
                    ))
                    .size(11.5)
                    .color(DIM),
                );
                if details != song.copyright {
                    state.copyright_request = Some((at, details));
                }
            });
        // Control-R is the Mac menu bar's key for it.
        let words_hint = match crate::keys::words_key() {
            Some(key) => {
                tf("slides.words.hint_key", "Edit Words: the song's words as text, to change ({key})", &[("key", &key)])
            }
            None => t("slides.words.hint", "Edit Words: the song's words as text, to change").to_owned(),
        };
        let words = icon_button(ui, place(), id.with("words"), Icon::Reflow, strength, false).hint(words_hint);
        if words.clicked() {
            state.reflow = Some((at, sp_model::to_song_text(song)));
        }
        let arranged = song.active_arrangement().is_some();
        let arrangement = icon_button(ui, place(), id.with("arrangement"), Icon::Arrangement, strength, arranged)
            .hint(t("slides.arrangement", "Arrangement: play the song's groups in another order"));
        arrangement_menu(at, song, state, &arrangement);
        if let Some(item) = item.filter(|item| crate::studio::can_show_with(state, *item)) {
            version_button(ui, show, state, place(), id.with("version"), item, strength);
        }
        let notes = icon_button(ui, place(), id.with("notes"), Icon::Notes, strength, state.show_notes)
            .hint(t("slides.notes", "Notes: show the notes of the slide on screen"));
        if notes.clicked() {
            state.show_notes = !state.show_notes;
        }
        let more = more_button(ui, place(), id.with("more"), near);
        egui::Popup::menu(&more).show(|ui| menu(ui, state));
        // The fonts the song uses that this computer doesn't have, said on the open song's bar.
        if let Some(theme) = theme.filter(|_| open || item.is_none()) {
            let epoch = state.lists.songs.epoch();
            let kept = state.lists.missing_fonts.get(|(made_in, made_for, look, fonts)| {
                *made_in == epoch && *made_for == at && look == theme && std::sync::Arc::ptr_eq(fonts, &state.fonts)
            });
            let missing = match kept {
                Some(missing) => missing,
                None => {
                    let missing = sp_model::missing_fonts(&sp_model::fonts_used(song, theme), &state.fonts);
                    state.lists.missing_fonts.keep((epoch, at, theme.clone(), state.fonts.clone()), missing)
                }
            };
            if !missing.is_empty() {
                let room = Rect::from_min_max(pos2(x, bar.min.y), pos2(right, bar.max.y));
                let layout = Layout::left_to_right(Align::Center);
                let used = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(room).layout(layout), |ui| {
                        ui.set_height(room.height().min(2.0 * (middle - bar.min.y)));
                        missing_font_notice(ui, &missing);
                    })
                    .response
                    .rect;
                x = used.max.x;
            }
        }
    }
    // A hairline from there to the right.
    if x + 10.0 < right {
        painter.hline(egui::Rangef::new(x + 10.0, right), middle, Stroke::new(1.0, LINE));
    }
    if response.clicked() && item.is_some() {
        state.services.chosen = item;
    }
    response.context_menu(|ui| menu(ui, state));
    if let Some(name) = name_response {
        name.context_menu(|ui| menu(ui, state));
    }
}

/// The menu of a song of the song catalog being looked at (from its bar's "more" button or a
/// right-click): the ways to use it, each of which makes it join the library.
fn catalog_menu(ui: &mut egui::Ui, state: &mut HomeState, held: &Arc<Presentation>) {
    use crate::catalog_songs::Then;
    let front = state.services.front().name.clone();
    if ui.button(tf("left.song.add_to", "Add to {list}", &[("list", &front)])).clicked() {
        catalog_add(state, held);
        ui.close();
    }
    ui.separator();
    if ui.button(t("left.song.edit_words", "Edit Words")).clicked() {
        state.catalog.join(held, Then::Words);
        ui.close();
    }
    if ui.button(t("slides.menu.edit", "Edit")).clicked() {
        state.catalog.join(held, Then::Edit);
        ui.close();
    }
}

/// Adds a song of the song catalog being looked at to the playlist in front, after the open
/// item, under the key it is filed under: it joins the library.
fn catalog_add(state: &mut HomeState, held: &Arc<Presentation>) {
    let item = Item::song(crate::catalog_songs::key_of(held), held.title.clone());
    if crate::studio::add_items(state, state.services.front_at(), vec![item]) > 0 {
        state.catalog.join(held, crate::catalog_songs::Then::Nothing);
    }
}

/// The version button among a playlist item's title-bar tools, once a theme has a version: the
/// theme icon, in the "on" grey while the item is set to show in a version, and orange while it
/// is on screen in it. A click opens what the item shows in when it goes on screen.
#[allow(clippy::too_many_arguments)]
fn version_button(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    rect: Rect,
    id: egui::Id,
    item: usize,
    strength: f32,
) {
    use crate::studio::{BUTTON, Flat, RADIUS, TALLY, TEXT, TEXT_DIM, paint_flat};
    let wanted = state.services.front().items.get(item).and_then(crate::service::Item::show_with);
    let set = wanted.and_then(|wanted| {
        let version = state.versions.iter().find(|version| version.id == wanted.version)?;
        Some((version.id, version.name.clone(), wanted.keep))
    });
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let hot = response.hovered() || open;
    if set.is_some() {
        paint_flat(ui.painter(), rect, RADIUS, Flat::On, false);
    } else if hot {
        ui.painter().rect_filled(rect, RADIUS, BUTTON);
    }
    let lit = show.live().is_some() && set.as_ref().is_some_and(|(id, ..)| state.version_switch.lights(item, *id));
    let ink = if lit {
        TALLY
    } else if hot || set.is_some() {
        TEXT
    } else {
        TEXT_DIM.gamma_multiply(strength)
    };
    icons::paint(ui.painter(), Icon::Theme, Rect::from_center_size(rect.center(), egui::Vec2::splat(14.0)), ink);
    let hint = match &set {
        Some((_, name, keep)) => {
            if *keep {
                tf(
                    "slides.version.set_kept",
                    "Shows with the {name} version, kept after it. Click to change",
                    &[("name", name)],
                )
            } else {
                tf("slides.version.set", "Shows with the {name} version. Click to change", &[("name", name)])
            }
        }
        None => match state.versions.first() {
            Some(version) => tf(
                "slides.version.example",
                "Show this with a theme version, such as {example}",
                &[("example", &version.name)],
            ),
            None => t("slides.version.none", "Show this with a theme version").to_owned(),
        },
    };
    let response = response.hint(hint);
    egui::Popup::menu(&response).show(|ui| crate::studio::show_with_menu(ui, state, item, true));
}

/// The name a new arrangement is given, with the first number the song's arrangements leave
/// free: "Arrangement 2", in the language set when it is made (it is the church's from then on).
pub fn arrangement_name(n: usize) -> String {
    crate::i18n::tgf("slides.arrangement.numbered", "Arrangement {n}", &[("n", &n.to_string())])
}

/// The name the arrangement that sings the chorus after each verse is given, in the language
/// set when it is made (it is the church's from then on, so `sp` prints it as saved); in English
/// it is [`sp_model::CHORUS_AFTER_EACH_VERSE`].
pub fn chorus_after_name() -> String {
    crate::i18n::tg("slides.arrangement.chorus_after.name", "Chorus after each verse").to_owned()
}

/// The arrangement menu, from the Arrangement button beside a song's title or the arrangement's
/// name at the start of its row: sing the song as written or in one of its arrangements, sing
/// the chorus after each verse (a song with a verse and a chorus), make a new one, rename,
/// duplicate or delete the one in use.
fn arrangement_menu(index: usize, song: &Presentation, state: &mut HomeState, button: &egui::Response) {
    let active = song.active_arrangement();
    let named = |arrangement: &sp_model::Arrangement| {
        if arrangement.name.trim().is_empty() {
            t("left.untitled", "Untitled").to_owned()
        } else {
            arrangement.name.clone()
        }
    };
    let mut arrangements = song.arrangements.clone();
    let mut chosen = song.arrangement;
    // Rename… opens a field in the menu, kept open while the menu is.
    let renaming_id = button.id.with("renaming");
    egui::Popup::from_toggle_button_response(button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(220.0);
            if ui
                .add(egui::Button::selectable(active.is_none(), t("slides.arrangement.as_written", "As written")))
                .clicked()
            {
                chosen = None;
                ui.close();
            }
            for arrangement in &song.arrangements {
                let on = active.is_some_and(|have| have.id == arrangement.id);
                if ui.add(egui::Button::selectable(on, named(arrangement))).clicked() {
                    chosen = Some(arrangement.id);
                    ui.close();
                }
            }
            ui.separator();
            if song.chorus_after_each_verse().is_some()
                && ui
                    .button(t("slides.arrangement.chorus_after", "Chorus After Each Verse"))
                    .hint(t(
                        "slides.arrangement.chorus_after.hint",
                        "Sing the chorus again after every verse; the order follows the song's parts until you change it",
                    ))
                    .clicked()
            {
                let mut sung = song.clone();
                sung.sing_chorus_after_each_verse(&chorus_after_name());
                arrangements = sung.arrangements;
                chosen = sung.arrangement;
                ui.close();
            }
            if ui
                .button(t("slides.arrangement.new", "New Arrangement"))
                .hint(t("slides.arrangement.new.hint", "Starts with the groups as written"))
                .clicked()
            {
                let made = song.new_arrangement(arrangement_name);
                chosen = Some(made.id);
                arrangements.push(made);
                ui.data_mut(|data| data.insert_temp(renaming_id, true));
            }
            if let Some(at) = active.and_then(|have| arrangements.iter().position(|a| a.id == have.id)) {
                let renaming = ui.data(|data| data.get_temp::<bool>(renaming_id).unwrap_or(false));
                if renaming {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(t("slides.arrangement.name", "Name")).color(DIM));
                        ui.add(egui::TextEdit::singleline(&mut arrangements[at].name).desired_width(150.0));
                    });
                } else if ui.button(t("slides.arrangement.rename", "Rename\u{2026}")).clicked() {
                    ui.data_mut(|data| data.insert_temp(renaming_id, true));
                }
                if ui.button(t("slides.duplicate", "Duplicate")).clicked() {
                    let copy = sp_model::Arrangement {
                        id: sp_model::Uuid::new_v4(),
                        name: tf("slides.arrangement.copy", "{name} copy", &[("name", &arrangements[at].name)]),
                        groups: arrangements[at].groups.clone(),
                        // A copy is the operator's, whatever it was copied from.
                        auto: false,
                    };
                    chosen = Some(copy.id);
                    arrangements.push(copy);
                }
                if ui.button(t("slides.delete", "Delete")).clicked() {
                    arrangements.remove(at);
                    chosen = None;
                    ui.close();
                }
            }
        });
    if !egui::Popup::is_id_open(&button.ctx, egui::Popup::default_response_id(button)) {
        button.ctx.data_mut(|data| data.remove::<bool>(renaming_id));
    }
    renamed_by_hand(song, &mut arrangements);
    if chosen != song.arrangement || arrangements != song.arrangements {
        state.arrangement_request = Some((index, arrangements, chosen));
    }
}

/// Renamed by hand, an arrangement made to follow the song's parts is the operator's from then
/// on (see [`sp_model::Arrangement::auto`]).
fn renamed_by_hand(song: &Presentation, arrangements: &mut [sp_model::Arrangement]) {
    for arrangement in arrangements {
        if song.arrangements.iter().any(|was| was.id == arrangement.id && was.name != arrangement.name) {
            arrangement.auto = false;
        }
    }
}

/// The song's arrangements with the one of this id given these groups by hand (a pill moved,
/// taken out, added or sung more times): it is the operator's from then on, and no longer
/// follows the song's parts.
fn changed_by_hand(song: &Presentation, id: Uuid, groups: Vec<Uuid>) -> Vec<sp_model::Arrangement> {
    let mut arrangements = song.arrangements.clone();
    if let Some(edited) = arrangements.iter_mut().find(|have| have.id == id) {
        edited.groups = groups;
        edited.auto = false;
    }
    arrangements
}

/// Where the parts of a song's arrangement row go, from the row's top left corner: the
/// arrangement's name, a pill for each piece in the order sung (a group sung several times
/// running is one pill), and the button that adds one. They wrap to more rows when there are
/// many.
struct ArrangementLayout {
    name: Rect,
    chips: Vec<ChipPlace>,
    add: Rect,
    height: f32,
}

struct ChipPlace {
    rect: Rect,
    /// The piece: where it starts in the arrangement's list of groups and how many times it is
    /// sung; and the group's place in the song.
    run: sp_model::Run,
    group: usize,
    /// The places in the order sung it covers: the first, and how many one time through takes.
    first: usize,
    len: usize,
}

const CHIP: f32 = 26.0;

/// The words of a piece's mark: ×3, or while it is on screen which time through it is (2/3).
fn chip_mark(count: usize, time: Option<usize>) -> Option<String> {
    (count > 1).then(|| match time {
        Some(time) => format!("{time}/{count}"),
        None => format!("\u{d7}{count}"),
    })
}

fn arrangement_layout(ui: &egui::Ui, song: &Presentation, width: f32) -> Option<ArrangementLayout> {
    let arrangement = song.active_arrangement()?;
    let wide = |words: &str, font: FontId| ui.painter().layout_no_wrap(words.to_owned(), font, TEXT).size().x;
    let (mut x, mut y) = (INSET, 4.0);
    let mut put = |w: f32| {
        if x > INSET && x + w > width - INSET {
            x = INSET;
            y += CHIP + 6.0;
        }
        let rect = Rect::from_min_size(pos2(x, y), vec2(w, CHIP));
        x += w + 6.0;
        rect
    };
    let label =
        if arrangement.name.trim().is_empty() { t("left.untitled", "Untitled") } else { arrangement.name.as_str() };
    let name = put(wide(&format!("{label} \u{25be}"), fixed_semibold(11.0)) + 26.0);
    let mut chips = Vec::new();
    let steps = song.run_steps();
    for (run, (first, len)) in arrangement.runs().into_iter().zip(steps) {
        let Some(group) = song.groups.iter().position(|group| group.id == run.group) else { continue };
        // Room for the widest mark it can show (its count, or "n/n" while on screen).
        let mark = chip_mark(run.count, Some(run.count)).map_or(0.0, |mark| wide(&mark, fixed_semibold(11.0)) + 16.0);
        let rect = put(10.0
            + 7.0
            + 6.0
            + wide(&song.groups[group].name, FontId::proportional(12.0))
            + mark
            + 6.0
            + 13.0
            + 8.0);
        chips.push(ChipPlace { rect, run, group, first, len });
    }
    let add = put(CHIP);
    Some(ArrangementLayout { name, chips, add, height: y + CHIP + 12.0 })
}

/// An arrangement's piece being dragged to another place in its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PieceDrag {
    song: usize,
    start: usize,
}

/// Where a piece dragged along an arrangement's row would land, with the pointer at `pointer`:
/// the nearest piece decides (nearness across counts four times less than up and down), and
/// which half of it the pointer is over says before it or after it. `None` when it would land
/// where it already is. Returns the piece's start and whether it goes after it.
fn piece_drop(chips: &[Rect], starts: &[usize], moving: usize, pointer: egui::Pos2) -> Option<(usize, bool)> {
    let nearest = chips
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let far = |r: &Rect| (pointer.y - r.center().y).abs() * 4.0 + (pointer.x - r.center().x).abs();
            far(a).total_cmp(&far(b))
        })
        .map(|(at, _)| at)?;
    let after = pointer.x > chips[nearest].center().x;
    let me = starts.iter().position(|start| *start == moving)?;
    // On itself, or beside it on the side it already stands: nothing would change.
    if nearest == me || (after && nearest + 1 == me) || (!after && nearest == me + 1) {
        return None;
    }
    Some((starts[nearest], after))
}

/// The menu of a piece of an arrangement: how many times it is sung, moving it, taking it out.
/// Returns the arrangement's groups changed, if they were.
fn piece_menu(ui: &mut egui::Ui, arrangement: &sp_model::Arrangement, chip: usize, count: usize) -> Option<Vec<Uuid>> {
    let runs = arrangement.runs();
    let run = runs.get(chip)?;
    let mut changed = arrangement.clone();
    ui.set_min_width(190.0);
    ui.label(egui::RichText::new(t("slides.times", "How many times")).size(11.5).color(TEXT_FAINT));
    let mut done = false;
    for (times, words) in [(1, t("slides.times.once", "Once")), (2, "\u{d7}2"), (3, "\u{d7}3"), (4, "\u{d7}4")] {
        if ui.add(egui::Button::selectable(run.count == times, words)).clicked() {
            changed.set_times(run.start, times);
            done = true;
        }
    }
    // More: up to twenty times.
    let mut times = run.count.max(5);
    ui.horizontal(|ui| {
        ui.label(if run.count > 4 {
            tf("slides.times.more_now", "\u{d7}{n} \u{b7} More", &[("n", &run.count.to_string())])
        } else {
            t("slides.times.more", "More\u{2026}").to_owned()
        });
        let field = ui.add(
            egui::DragValue::new(&mut times).range(1..=sp_model::MOST_TIMES).suffix(t("slides.times.suffix", " times")),
        );
        if field.changed() || (field.lost_focus() && run.count <= 4 && times > 4) {
            changed.set_times(run.start, times);
            done = true;
        }
    });
    ui.separator();
    if ui.add_enabled(chip > 0, egui::Button::new(t("slides.piece.move_left", "Move Left"))).clicked() {
        changed.move_run(run.start, runs[chip - 1].start, false);
        done = true;
        ui.close();
    }
    if ui.add_enabled(chip + 1 < count, egui::Button::new(t("slides.piece.move_right", "Move Right"))).clicked() {
        changed.move_run(run.start, runs[chip + 1].start, true);
        done = true;
        ui.close();
    }
    if ui.add_enabled(runs.len() > 1, egui::Button::new(t("slides.piece.remove", "Remove"))).clicked() {
        changed.remove_run(run.start);
        done = true;
        ui.close();
    }
    (done && changed.groups != arrangement.groups).then_some(changed.groups)
}

/// The arrangement a song is sung in, under its bar: its name (which opens the arrangement
/// menu), then a pill for each piece in the order sung, a group sung several times running
/// marked ×2, ×3…; the piece on screen is orange and says which time through it is (2/3). A
/// click goes to that point in the song; the arrow (or a right-click) sets how many times,
/// moves or removes it; a drag along the row moves it; + adds a group at the end.
fn arrangement_strip(
    ui: &mut egui::Ui,
    show: &Show,
    deck: &SlideDeck,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    origin: egui::Pos2,
    width: f32,
) {
    let song = &deck.deck;
    let (Some(index), Some(arrangement)) = (deck.at, song.active_arrangement()) else { return };
    let Some(layout) = arrangement_layout(ui, song, width) else { return };
    let id = ui.scope_id().with(("arrangement", deck.item, index));
    let live = show.live().filter(|live| live.presentation == index).and(show.position()).map(|(step, _)| step);
    // The arrangement's name: the same menu as the tool on the bar.
    let name = layout.name.translate(origin.to_vec2());
    let pill_at = Rect::from_center_size(name.center(), vec2(name.width(), 21.0));
    let button = ui.interact(name, id.with("name"), Sense::click());
    ui.painter().rect_filled(pill_at, 10.5, if button.hovered() { crate::studio::BUTTON } else { CARD });
    let label =
        if arrangement.name.trim().is_empty() { t("left.untitled", "Untitled") } else { arrangement.name.as_str() };
    crate::studio::paint_spaced(
        ui,
        pill_at.center(),
        Align2::CENTER_CENTER,
        &format!("{label} \u{25be}"),
        fixed_semibold(11.0),
        TEXT_DIM,
        0.7,
    );
    let hint = t("slides.arrangement.hint", "Choose, make or change an arrangement");
    arrangement_menu(index, song, state, &button.hint(hint));
    let mut groups: Option<Vec<Uuid>> = None;
    let count = layout.chips.len();
    let rects: Vec<Rect> = layout.chips.iter().map(|chip| chip.rect.translate(origin.to_vec2())).collect();
    let starts: Vec<usize> = layout.chips.iter().map(|chip| chip.run.start).collect();
    let carried = egui::DragAndDrop::payload::<PieceDrag>(ui.ctx()).filter(|drag| drag.song == index).map(|drag| *drag);
    for (chip, place) in layout.chips.iter().enumerate() {
        let rect = rects[chip];
        let group = &song.groups[place.group];
        let span = place.len * place.run.count;
        let on = live.is_some_and(|step| step >= place.first && step < place.first + span);
        let time = live.filter(|_| on && place.len > 0).map(|step| (step - place.first) / place.len + 1);
        let response = ui.interact(rect, id.with(("chip", chip)), Sense::click_and_drag());
        let moving = carried.is_some_and(|drag| drag.start == place.run.start);
        let fill = if on {
            TALLY
        } else if response.hovered() && carried.is_none() {
            crate::studio::BUTTON
        } else {
            CARD
        };
        // The piece being carried goes faint where it is.
        let strength = if moving { 0.4 } else { 1.0 };
        ui.painter().rect_filled(rect, CHIP / 2.0, fill.gamma_multiply(strength));
        let (dot, ink) = if on { (Color32::WHITE, Color32::WHITE) } else { (dot_colour(group.color), TEXT_DIM) };
        ui.painter().circle_filled(pos2(rect.min.x + 10.0 + 3.5, rect.center().y), 3.5, dot.gamma_multiply(strength));
        let words = ui.painter().text(
            pos2(rect.min.x + 23.0, rect.center().y),
            Align2::LEFT_CENTER,
            group.name.as_str(),
            FontId::proportional(12.0),
            ink.gamma_multiply(strength),
        );
        if let Some(mark) = chip_mark(place.run.count, time) {
            let galley = ui.painter().layout_no_wrap(mark, fixed_semibold(11.0), Color32::WHITE);
            let back =
                Rect::from_min_size(pos2(words.max.x + 6.0, rect.center().y - 8.4), vec2(galley.size().x + 10.0, 16.8));
            let shade = if on { Color32::from_black_alpha(77) } else { crate::studio::BUTTON };
            ui.painter().rect_filled(back, 5.0, shade);
            let mark_ink = if on { Color32::WHITE } else { TEXT };
            ui.painter().galley(pos2(back.min.x + 5.0, rect.center().y - galley.size().y / 2.0), galley, mark_ink);
        }
        // The arrow: how many times, move, remove.
        let arrow = Rect::from_center_size(pos2(rect.max.x - 8.0 - 6.5, rect.center().y), vec2(13.0, 13.0));
        let arrow_reach = Rect::from_min_max(pos2(arrow.min.x - 4.0, rect.min.y), rect.max);
        let arrow_response = ui.interact(arrow_reach, id.with(("chip arrow", chip)), Sense::click());
        let arrow_ink = if on { Color32::WHITE } else { TEXT_DIM };
        let arrow_strength = if arrow_response.hovered() { 1.0 } else { 0.6 };
        icons::paint(ui.painter(), Icon::ChevronDown, arrow, arrow_ink.gamma_multiply(arrow_strength));
        egui::Popup::menu(&arrow_response.hint(t("slides.chip.arrow", "Times, move or remove"))).show(|ui| {
            if let Some(changed) = piece_menu(ui, arrangement, chip, count) {
                groups = Some(changed);
            }
        });
        // A drag along the row moves the piece; a press that barely moves is a click.
        if response.drag_started() {
            response.dnd_set_drag_payload(PieceDrag { song: index, start: place.run.start });
        }
        let response = response
            .hint(t("slides.chip.hint", "Click to go there \u{b7} drag to move \u{b7} the arrow sets how many times"));
        if response.clicked() {
            commands.push(Command::GoToStep { presentation: index, step: place.first });
            if deck.item.is_some() {
                state.services.chosen = deck.item;
                state.live_item = deck.item;
            }
        }
        response.context_menu(|ui| {
            if let Some(changed) = piece_menu(ui, arrangement, chip, count) {
                groups = Some(changed);
            }
        });
    }
    // The whole row takes a dropped piece; an ink line shows the gap it will go into.
    if let Some(drag) = carried
        && let Some(pointer) = ui.ctx().pointer_hover_pos()
    {
        let row = Rect::from_min_size(origin, vec2(width, layout.height)).expand2(vec2(0.0, 4.0));
        if row.contains(pointer)
            && let Some((to, after)) = piece_drop(&rects, &starts, drag.start, pointer)
            && let Some(target) = starts.iter().position(|start| *start == to)
        {
            let rect = rects[target];
            let x = if after { rect.max.x + 3.0 } else { rect.min.x - 3.0 };
            ui.painter().vline(x, rect.y_range().expand(2.0), Stroke::new(2.0, TEXT));
            if ui.input(|input| input.pointer.any_released()) {
                let mut moved = arrangement.clone();
                moved.move_run(drag.start, to, after);
                groups = Some(moved.groups);
            }
        }
        ui.ctx().request_repaint();
    }
    let add = icon_button(ui, layout.add.translate(origin.to_vec2()), id.with("add"), Icon::Plus, 1.0, false)
        .hint(t("slides.add_group", "Add a group to the end"));
    egui::Popup::menu(&add).show(|ui| {
        for group in &song.groups {
            if ui.button(group.name.as_str()).clicked() {
                let mut added = arrangement.groups.clone();
                added.push(group.id);
                groups = Some(added);
            }
        }
    });
    if let Some(groups) = groups {
        let arrangements = changed_by_hand(song, arrangement.id, groups);
        state.arrangement_request = Some((index, arrangements, song.arrangement));
    }
}

/// The middle of the Service view: the playlist in front as one scroll, or the song chosen in a
/// library, with the bar under the slides at its foot.
pub fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        // The words of a song as text take the slides' place while they are being edited.
        let reflowing = state.reflow.as_ref().and_then(|(index, _)| show.presentations().get(*index));
        if let Some(song) = reflowing {
            reflow_editor(ui, &song.title, state);
            return;
        }
        // The bar under the slides: the transition, Continuous, Undo and Redo, the slide view
        // and the slide size.
        crate::studio::dock_bar(ui, state);
        // Operator notes: the notes of the slide on screen, or of the one picked out.
        if state.show_notes {
            let at = show.live().or(state.selected_slide).or(show.cursor());
            let notes = at
                .and_then(|at| show.presentations().get(at.presentation)?.slide(at.slide))
                .and_then(|(_, slide)| slide.notes.clone());
            let strip = Frame::new().fill(CARD).inner_margin(egui::Margin::symmetric(20, 8));
            egui::Panel::top("operator notes").resizable(false).frame(strip).show(ui, |ui| {
                let (text, ink) = match notes {
                    Some(notes) => (notes, TEXT),
                    None => (
                        t("slides.no_notes", "No notes for this slide. Add them in the editor with Slide Notes.")
                            .to_owned(),
                        TEXT_FAINT,
                    ),
                };
                ui.label(egui::RichText::new(text).size(13.0).color(ink));
            });
        }
        let decks = decks(show, state, commands);
        if decks.is_empty() {
            nothing_open(ui, show, state);
            return;
        }
        let area = ui.available_rect_before_wrap();
        slide_scroll(ui, show, state, &decks, Some(theme), thumb, commands);
        paint_fade(ui, area);
    });
}

/// The Bible page's passage: its slides as tiles, with no bar over them.
pub fn slide_grid(
    ui: &mut egui::Ui,
    show: &Show,
    which: usize,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let Some(deck) = song_alone(show, state, which, false) else { return };
    slide_scroll(ui, show, state, &[deck], None, thumb, commands);
}

/// The slides fade into the panel's grey just above the dock, so a row cut off there reads as
/// "more below".
fn paint_fade(ui: &egui::Ui, area: Rect) {
    let band = Rect::from_min_max(pos2(area.min.x, area.max.y - 34.0), area.max);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(band.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(band.left_bottom(), PANEL);
    mesh.colored_vertex(band.right_bottom(), PANEL);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// What the middle says when there is nothing to show: an empty playlist, or no song.
fn nothing_open(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    use crate::menubar::Action;
    let playlist = state.playlist_open;
    let (title, words) = if playlist {
        (
            tf("slides.empty.playlist", "{list} is empty", &[("list", &state.services.front().name)]),
            t(
                "slides.empty.playlist.about",
                "A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the order they come.",
            ),
        )
    } else if show.presentations().is_empty() {
        (
            t("slides.empty.no_songs", "No songs yet").to_owned(),
            t("slides.empty.no_songs.about", "Make a song, or import some."),
        )
    } else {
        (
            t("slides.empty.no_song_open", "No song is open").to_owned(),
            t("slides.empty.no_song_open.about", "Pick a song on the left to see its slides."),
        )
    };
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(40.0));
        ui.label(egui::RichText::new(title).font(semibold(20.0)).color(TEXT));
        ui.add_space(6.0);
        ui.scope(|ui| {
            ui.set_max_width(420.0);
            ui.label(egui::RichText::new(words).size(13.0).color(TEXT_DIM));
        });
        ui.add_space(12.0);
        let wide = if playlist { 300.0 } else { 190.0 };
        ui.allocate_ui_with_layout(vec2(wide, 30.0), Layout::left_to_right(Align::Center), |ui| {
            if playlist {
                if ui
                    .button(t("slides.empty.add_bible", "Add a Bible passage"))
                    .hint(t(
                        "quickhelp.slides.add_bible",
                        "Look a passage up on the Bible page, to add to this playlist.",
                    ))
                    .clicked()
                {
                    state.bible_open = true;
                    state.bible.focus_lookup = true;
                }
                if ui
                    .button(t("slides.empty.add_header", "Add a header"))
                    .hint(t("left.add.header.hint", "A heading over the items under it"))
                    .clicked()
                {
                    let name = t("left.add.header.name", "Header");
                    state.services.add(Item::Header { name: name.to_owned() });
                    let at = state.services.front().items.len() - 1;
                    state.renaming_header = Some((at, name.to_owned()));
                    state.rename_started = true;
                }
            } else if ui
                .button(t("left.new_song", "New song"))
                .hint(crate::keys::hint(t("left.new_song", "New song"), "N", false))
                .clicked()
            {
                state.new_song = true;
            }
            if ui
                .button(t("slides.empty.import", "Import\u{2026}"))
                .hint(t("left.import", "Import songs or slides"))
                .clicked()
            {
                state.menu_asked = Some(Action::ImportSongs);
            }
        });
        if playlist {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(t(
                    "slides.empty.add_hint",
                    "Songs are added with Add to playlist, at the foot of the left column.",
                ))
                .size(12.0)
                .color(TEXT_FAINT),
            );
        }
    });
}

/// A song slide's menu, from its "more" button or a right-click. Editing is here, and on the
/// pencil, so that reaching for it never puts a slide on screen by accident. Slides the app
/// makes have none.
fn slide_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    presentation: &Presentation,
    here: SlideRef,
    slide: &Slide,
) {
    let (which, index) = (here.presentation, here.slide);
    if ui.button(t("slides.menu.edit", "Edit")).clicked() {
        state.editing = EditDraft::of(show, here);
        ui.close();
    }
    ui.separator();
    // Everything here acts on the slides of this song picked out with this one, when it is one
    // of them, or else on this one: each change is one step for Undo.
    let mut these: Vec<usize> = state
        .picked_slides
        .iter()
        .filter(|p| p.presentation == which && state.picked_slides.contains(&here))
        .map(|p| p.slide)
        .collect();
    these.sort_unstable();
    these.dedup();
    if these.is_empty() {
        these.push(index);
    }
    // Each of them changed the same way, put back in one step.
    let edited = |change: &dyn Fn(&Slide) -> Slide| {
        let edits = these.iter().filter_map(|at| presentation.slide(*at).map(|(_, one)| (*at, change(one))));
        Some((here, SlideRequest::Edit(edits.collect())))
    };
    let key = |letter: &str| crate::keys::with_modifier(letter);
    // A song always keeps one slide: with every slide of it picked (or its only one), Cut and
    // Delete are greyed, and say so.
    let all = these.len() >= presentation.slide_count();
    let cut = ui.add_enabled(!all, egui::Button::new(t("slides.menu.cut", "Cut")).shortcut_text(key("X")));
    let one_slide = t("slides.menu.one_slide", "A song always keeps one slide");
    if cut.reason(one_slide).clicked() {
        copy_slides(ui.ctx(), state, presentation, &these);
        state.slide_request = Some((here, SlideRequest::DeleteMany(these.clone())));
        ui.close();
    }
    if ui.add(egui::Button::new(t("slides.menu.copy", "Copy")).shortcut_text(key("C"))).clicked() {
        copy_slides(ui.ctx(), state, presentation, &these);
        ui.close();
    }
    let clip = state.slide_clip.clone();
    let paste =
        ui.add_enabled(!clip.is_empty(), egui::Button::new(t("slides.menu.paste", "Paste")).shortcut_text(key("V")));
    if paste.reason(t("slides.menu.paste.none", "Copy or cut a slide first")).clicked() {
        state.slide_request = Some((here, SlideRequest::PasteAfter(clip)));
        ui.close();
    }
    // A copy of the slides (this one, or those picked with it) just after them.
    if ui.button(t("slides.duplicate", "Duplicate")).clicked() {
        let copies: Vec<Slide> =
            these.iter().filter_map(|at| presentation.slide(*at).map(|(_, one)| one.clone())).collect();
        let last = SlideRef { slide: these.last().copied().unwrap_or(index), ..here };
        state.slide_request = Some((last, SlideRequest::PasteAfter(copies)));
        ui.close();
    }
    // At once, asking nothing (Undo puts them back): this slide, or the slides of this song
    // picked with it, never slides of another song.
    let delete = if these.len() > 1 {
        let n = crate::i18n::count(these.len() as u64);
        tf("slides.menu.delete_many", "Delete {n} Slides", &[("n", &n)])
    } else {
        t("slides.delete", "Delete").to_owned()
    };
    if ui.add_enabled(!all, egui::Button::new(delete)).reason(one_slide).clicked() {
        crate::home::delete_slides(state, which, &these);
        ui.close();
    }
    // One place up or down the song (with the slides picked with it).
    let (first, last) = (these.first().copied().unwrap_or(index), these.last().copied().unwrap_or(index));
    let up = ui.add_enabled(first > 0, egui::Button::new(t("left.move_up", "Move Up")));
    if up.reason(t("left.at_top", "Already at the top")).clicked() {
        let to = first - 1;
        state.slide_request = Some((here, SlideRequest::Move { slides: these.clone(), to, after: false }));
        ui.close();
    }
    let down =
        ui.add_enabled(last + 1 < presentation.slide_count(), egui::Button::new(t("left.move_down", "Move Down")));
    if down.reason(t("left.at_bottom", "Already at the bottom")).clicked() {
        let to = last + 1;
        state.slide_request = Some((here, SlideRequest::Move { slides: these.clone(), to, after: true }));
        ui.close();
    }
    ui.separator();
    // Where a group starts: this slide and the rest of its group take the name (several picked:
    // just they become the group), in the language set. The group it is in now is ticked, named
    // in whichever language: "Verse 1" ticks «Куплет 1».
    let group_now = presentation.slide(index).map(|(group, _)| group.name.clone()).unwrap_or_default();
    ui.menu_button(t("slides.menu.group", "Group"), |ui| {
        for name in group_names() {
            if ui.add(egui::Button::selectable(sp_model::same_part(&group_now, name), name)).clicked() {
                state.slide_request = Some((here, SlideRequest::Group(these.clone(), name.to_owned())));
                ui.close();
            }
        }
    });
    // The media the slide puts behind its words when it goes on screen.
    ui.menu_button(t("slides.menu.media", "Media"), |ui| {
        let own = media_of(slide);
        if ui.add(egui::Button::selectable(own.is_none(), t("slides.menu.media.none", "None"))).clicked() {
            state.slide_request = edited(&|one| slide_with_media(one, None));
            ui.close();
        }
        ui.separator();
        if show.media().is_empty() {
            ui.label(egui::RichText::new(t("left.add.media.empty", "The media bin is empty.")).color(DIM));
        }
        for item in show.media() {
            if ui.add(egui::Button::selectable(own == Some(item.id), item.name.as_str())).clicked() {
                state.slide_request = edited(&|one| slide_with_media(one, Some(item.id)));
                ui.close();
            }
        }
    });
    ui.menu_button(t("slides.menu.transition", "Transition"), |ui| {
        // Usual: the song's own transition, or else the one under the slides, named.
        let usual = crate::studio::usual_transition(presentation.transition.unwrap_or(state.transition));
        if ui.add(egui::Button::selectable(slide.transition.is_none(), usual)).clicked() {
            state.slide_request = edited(&|one| Slide { transition: None, ..one.clone() });
            ui.close();
        }
        ui.separator();
        for kind in sp_model::TransitionKind::ALL {
            let on = slide.transition.is_some_and(|own| own.kind == kind);
            if ui.add(egui::Button::selectable(on, crate::studio::transition_name(kind))).clicked() {
                let own = sp_model::Transition::of(kind);
                state.slide_request = edited(&|one| Slide { transition: Some(own), ..one.clone() });
                ui.close();
            }
        }
    });
    // What the slide does when it goes on screen.
    ui.menu_button(t("slides.menu.actions", "Actions"), |ui| {
        for (at, action) in slide.actions.iter().enumerate() {
            let words = crate::live_tabs::describe_action(show, state, action);
            if ui.button(tf("slides.menu.actions.remove", "Remove: {action}", &[("action", &words)])).clicked() {
                let mut kept = slide.clone();
                kept.actions.remove(at);
                state.pending_edit = Some((here, kept));
                ui.close();
            }
        }
        if !slide.actions.is_empty() {
            ui.separator();
        }
        ui.menu_button(t("slides.menu.actions.add", "Add Action"), |ui| {
            if let Some(action) = crate::live_tabs::add_action_menu(ui, show, state) {
                let mut more = slide.clone();
                more.actions.push(action);
                state.pending_edit = Some((here, more));
                ui.close();
            }
        });
    });
    // Going on to the next slide by itself, after a time.
    ui.menu_button(t("slides.menu.go_next", "Go to Next"), |ui| {
        if ui.add(egui::Button::selectable(slide.advance.is_none(), t("slides.menu.go_next.none", "None"))).clicked() {
            state.slide_request = edited(&|one| Slide { advance: None, ..one.clone() });
            ui.close();
        }
        ui.separator();
        for seconds in [3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0] {
            let on = slide.advance == Some(seconds);
            let label = tf("slides.menu.go_next.after", "After {n} Seconds", &[("n", &format!("{seconds:.0}"))]);
            if ui.add(egui::Button::selectable(on, label)).clicked() {
                state.slide_request = edited(&|one| Slide { advance: Some(seconds), ..one.clone() });
                ui.close();
            }
        }
    });
    // A key that puts this slide on screen in the Service view.
    ui.menu_button(t("slides.menu.hot_key", "Hot Key"), |ui| {
        if ui.add(egui::Button::selectable(slide.hot_key.is_none(), t("slides.menu.hot_key.none", "None"))).clicked() {
            state.slide_request = edited(&|one| Slide { hot_key: None, ..one.clone() });
            ui.close();
        }
        ui.separator();
        egui::Grid::new("hot keys").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
            for (index, key) in ('A'..='Z').chain('0'..='9').enumerate() {
                let on = slide.hot_key == Some(key);
                let button = egui::Button::selectable(on, key.to_string()).min_size(vec2(26.0, 22.0));
                if ui.add(button).clicked() {
                    state.slide_request = edited(&|one| Slide { hot_key: Some(key), ..one.clone() });
                    ui.close();
                }
                if index % 6 == 5 {
                    ui.end_row();
                }
            }
        });
    });
    // A slide that came in from a presentation with a reason to look at it: marked as looked at,
    // which takes its tag off (Undo puts it back).
    let to_check = these.iter().any(|at| presentation.slide(*at).is_some_and(|(_, one)| one.check.is_some()));
    if to_check && ui.button(t("slides.menu.checked", "Mark as Checked")).clicked() {
        state.slide_request = edited(&|one| Slide { check: None, ..one.clone() });
        ui.close();
    }
    // The look of the slide's text, to give another slide.
    if ui.button(t("slides.menu.copy_style", "Copy Style")).clicked() {
        state.inspector.copied_style = slide.objects.iter().find_map(|object| object.text_style().cloned());
        ui.close();
    }
    if ui
        .add_enabled(
            state.inspector.copied_style.is_some(),
            egui::Button::new(t("slides.menu.paste_style", "Paste Style")),
        )
        .hint(t(
            "slides.menu.paste_style.hint",
            "Give every text box on the slide (and the slides picked with it) the look that was copied",
        ))
        .reason(t("slides.menu.paste_style.none", "Copy a style first"))
        .clicked()
        && let Some(copied) = state.inspector.copied_style.clone()
    {
        state.slide_request = edited(&|one| {
            let mut styled = one.clone();
            for object in &mut styled.objects {
                if let Some(style) = object.text_style_mut() {
                    *style = copied.clone();
                }
            }
            styled
        });
        ui.close();
    }
    ui.menu_button(t("slides.menu.label", "Label"), |ui| {
        if ui.add(egui::Button::selectable(slide.label.is_none(), t("slides.menu.label.none", "None"))).clicked() {
            state.slide_request = edited(&|one| Slide { label: None, ..one.clone() });
            ui.close();
        }
        ui.separator();
        for label in SLIDE_LABELS {
            let name = crate::i18n::tg(label.0, label.1);
            if ui.add(egui::Button::selectable(label_is(slide.label.as_deref(), label), name)).clicked() {
                state.slide_request = edited(&|one| Slide { label: Some(name.to_owned()), ..one.clone() });
                ui.close();
            }
        }
    });
    // A disabled slide stays in the song but is stepped over and never shown. The slides picked
    // with it all take what it offers: off when this one is on, on when it is off.
    let label =
        if slide.enabled { t("slides.menu.disable", "Disable Slide") } else { t("slides.menu.enable", "Enable Slide") };
    if ui.button(label).clicked() {
        let enabled = !slide.enabled;
        state.slide_request = edited(&|one| Slide { enabled, ..one.clone() });
        ui.close();
    }
    // Which of the theme's templates the slide is drawn with. Usual lets the theme pick the
    // one that suits what is on the slide.
    if !state.song_templates.is_empty() {
        ui.menu_button(t("slides.menu.template", "Template"), |ui| {
            let current = slide.template.filter(|id| state.song_templates.iter().any(|(t, _)| t == id));
            let usual = t("slides.menu.template.usual", "Usual (the theme picks)");
            if ui.add(egui::Button::selectable(current.is_none(), usual)).clicked() {
                state.slide_request = edited(&|one| Slide { template: None, ..one.clone() });
                ui.close();
            }
            ui.separator();
            for (id, name) in &state.song_templates {
                let shown = crate::theme_store::template_name(*id, name);
                if ui.add(egui::Button::selectable(current == Some(*id), shown.as_ref())).clicked() {
                    let template = Some(*id);
                    state.slide_request = edited(&|one| Slide { template, ..one.clone() });
                    ui.close();
                }
            }
        });
    }
}

/// Whether a slide's label is this one of the Label menu's, given in any language: a slide
/// labelled "Repeat" has «Повтор» ticked in Russian.
fn label_is(label: Option<&str>, offered: (&'static str, &'static str)) -> bool {
    label.is_some_and(|label| crate::i18n::every_wording(offered).any(|said| said == label))
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, RawInput};
    use sp_model::parse_song_text;

    use super::*;

    #[test]
    fn a_slides_strip_says_the_apps_own_labels_in_the_language_set() {
        let song = parse_song_text("S", "[Verse 1]\nWe lift our voices\n");
        let (group, slide) = song.slide(0).expect("a slide");
        let group = group.clone();
        assert_eq!(group.name, "Verse 1");
        let labelled = |label: &str| Slide { label: Some(label.to_owned()), ..slide.clone() };
        for language in [crate::i18n::Language::Russian, crate::i18n::Language::Spanish] {
            crate::i18n::with_language(language, || {
                // One of the app's four labels, kept in English in the song's file.
                let said = t("slides.label.blank", "Blank Slide");
                assert_ne!(said, "Blank Slide", "{language:?} has its own words for it");
                assert_eq!(strip_name(&group, &labelled("Blank Slide")), format!("Verse 1 \u{b7} {said}"));
                // A label of the person's own is said as typed.
                assert_eq!(strip_name(&group, &labelled("Bridge twice")), "Verse 1 \u{b7} Bridge twice");
            });
        }
        crate::i18n::with_language(crate::i18n::Language::English, || {
            assert_eq!(strip_name(&group, &labelled("Blank Slide")), "Verse 1 \u{b7} Blank Slide");
        });
    }

    /// One frame of the window with these events. Returns what it asked for.
    fn run(
        ctx: &egui::Context,
        show: &Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: &mut f64,
    ) -> Vec<Command> {
        *time += 0.1;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            time: Some(*time),
            events,
            ..RawInput::default()
        };
        let mut asked = Vec::new();
        let output = ctx.run_ui(raw, |ui| {
            asked.extend(home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
        });
        output.drop_without_applying_deltas();
        asked
    }

    /// A click at a place in the window. Returns everything asked for on the way.
    fn click(ctx: &egui::Context, show: &Show, state: &mut HomeState, at: egui::Pos2, time: &mut f64) -> Vec<Command> {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut asked = Vec::new();
        for events in [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)], Vec::new()] {
            asked.extend(run(ctx, show, state, events, time));
        }
        asked
    }

    fn song(title: &str, slides: usize) -> Arc<Presentation> {
        let words = (0..slides).map(|n| format!("Line {n} of {title}\n")).collect::<Vec<_>>().join("\n");
        Arc::new(parse_song_text(title, &words))
    }

    /// A show of these songs with a playlist of them all in front, in order.
    fn playlist(songs: &[(&str, usize)]) -> (Show, HomeState) {
        let show = Show::new(songs.iter().map(|(title, slides)| song(title, *slides)).collect());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        for (title, _) in songs {
            state.services.add(Item::song(format!("title:{title}"), (*title).to_owned()));
        }
        state.services.chosen = Some(0);
        (show, state)
    }

    fn rect_of(state: &HomeState, presentation: usize, slide: usize) -> Rect {
        let here = SlideRef { presentation, slide };
        state.slide_rects.iter().find(|(at, _)| *at == here).map(|(_, rect)| *rect).expect("the slide is laid out")
    }

    fn went_live(asked: &[Command]) -> bool {
        asked.iter().any(|command| matches!(command, Command::GoTo(_) | Command::GoToStep { .. }))
    }

    /// Clicks `words` in a slide's menu opened on `here`, and returns what it asked for.
    fn slide_menu_click(show: &Show, state: &mut HomeState, here: SlideRef, words: &str) -> Option<SlideRequest> {
        let ctx = crate::studio::tests::styled();
        let song = show.presentations()[here.presentation].clone();
        let slide = song.slide(here.slide).map(|(_, slide)| slide.clone()).expect("a slide");
        let output = crate::studio::tests::drawn(&ctx, vec![], |ui| slide_menu(ui, show, state, &song, here, &slide));
        let at = crate::studio::tests::place_of(&output, words).unwrap_or_else(|| panic!("{words} is offered"));
        output.drop_without_applying_deltas();
        for events in crate::studio::tests::click_frames(at, egui::PointerButton::Primary) {
            let output =
                crate::studio::tests::drawn(&ctx, events, |ui| slide_menu(ui, show, state, &song, here, &slide));
            output.drop_without_applying_deltas();
        }
        state.slide_request.take().map(|(_, request)| request)
    }

    /// Opens `menu` in a slide's menu opened on `here`, clicks `words` in it, and returns what it
    /// asked for.
    fn slide_submenu_click(
        show: &Show,
        state: &mut HomeState,
        here: SlideRef,
        menu: &str,
        words: &str,
    ) -> Option<SlideRequest> {
        use crate::studio::tests::{click_frames, drawn, place_of, place_on_top, styled};
        let ctx = styled();
        let song = show.presentations()[here.presentation].clone();
        let slide = song.slide(here.slide).map(|(_, slide)| slide.clone()).expect("a slide");
        let output = drawn(&ctx, vec![], |ui| slide_menu(ui, show, state, &song, here, &slide));
        let at = place_of(&output, menu).unwrap_or_else(|| panic!("{menu} is offered"));
        output.drop_without_applying_deltas();
        for events in click_frames(at, egui::PointerButton::Primary) {
            drawn(&ctx, events, |ui| slide_menu(ui, show, state, &song, here, &slide)).drop_without_applying_deltas();
        }
        // A menu is drawn from its second frame, once it knows its size.
        drawn(&ctx, vec![], |ui| slide_menu(ui, show, state, &song, here, &slide)).drop_without_applying_deltas();
        let output = drawn(&ctx, vec![], |ui| slide_menu(ui, show, state, &song, here, &slide));
        let at = place_on_top(&output, words).unwrap_or_else(|| panic!("{words} is offered in {menu}"));
        output.drop_without_applying_deltas();
        for events in click_frames(at, egui::PointerButton::Primary) {
            drawn(&ctx, events, |ui| slide_menu(ui, show, state, &song, here, &slide)).drop_without_applying_deltas();
        }
        state.slide_request.take().map(|(_, request)| request)
    }

    #[test]
    fn the_group_and_label_menus_offer_their_names_in_the_language_set() {
        use crate::i18n::{Language, with_language};
        // A song you already have, its parts named in English.
        let song = Arc::new(parse_song_text("Song", "[Verse 1]\nOne\n\nTwo\n\n[Chorus]\nThree\n"));
        let show = Show::new(vec![song.clone()]);
        let mut state = HomeState::default();
        let second = SlideRef { presentation: 0, slide: 1 };
        // In Russian the second slide starts «Припев 2», in Russian.
        let asked =
            with_language(Language::Russian, || slide_submenu_click(&show, &mut state, second, "Часть", "Припев 2"));
        let Some(SlideRequest::Group(slides, name)) = asked else { panic!("asked {asked:?}") };
        assert_eq!((slides.as_slice(), name.as_str()), ([1].as_slice(), "Припев 2"));
        let grouped = song.with_group_over(&slides, &name).expect("grouped");
        // Only that slide's group is named: the others keep their English names.
        let names: Vec<&str> = grouped.groups.iter().map(|group| group.name.as_str()).collect();
        assert_eq!(names, ["Verse 1", "Припев 2", "Chorus"]);
        assert_eq!(grouped.groups[1].color, sp_model::group_color("Chorus 2"));
        assert_eq!(sp_model::key_for_group(&grouped.groups[1].name), Some('X'), "the key of Chorus 2");
        // In English the menu is as it was.
        let asked = slide_submenu_click(&show, &mut state, second, "Group", "Bridge");
        assert_eq!(asked, Some(SlideRequest::Group(vec![1], "Bridge".to_owned())));
        // A label is given in the language set too.
        let labelled = |request: Option<SlideRequest>| match request {
            Some(SlideRequest::Edit(edits)) => edits.into_iter().map(|(_, slide)| slide.label).collect::<Vec<_>>(),
            other => panic!("asked {other:?}"),
        };
        let asked =
            with_language(Language::Ukrainian, || slide_submenu_click(&show, &mut state, second, "Мітка", "Повтор"));
        assert_eq!(labelled(asked), [Some("Повтор".to_owned())]);
        let asked = with_language(Language::Spanish, || {
            slide_submenu_click(&show, &mut state, second, "Etiqueta", "Diapositiva en blanco")
        });
        assert_eq!(labelled(asked), [Some("Diapositiva en blanco".to_owned())]);
        let asked = slide_submenu_click(&show, &mut state, second, "Label", "Spoken Word");
        assert_eq!(labelled(asked), [Some("Spoken Word".to_owned())]);
    }

    #[test]
    fn a_label_given_in_any_language_is_ticked_in_every_language() {
        let [blank, _, repeat, _] = SLIDE_LABELS;
        for label in ["Repeat", "Repetir", "Повтор"] {
            assert!(label_is(Some(label), repeat), "{label}");
            assert!(!label_is(Some(label), blank), "{label}");
        }
        assert!(label_is(Some("Пустой слайд"), blank));
        assert!(label_is(Some("Порожній слайд"), blank));
        assert!(!label_is(Some("Repeat twice"), repeat), "a label of the church's own is none of them");
        assert!(!label_is(None, repeat));
    }

    #[test]
    fn a_slides_menu_acts_on_the_slides_of_its_song_picked_with_it() {
        let song = Arc::new(parse_song_text("Song", "One\n\nTwo\n\nThree\n"));
        let show = Show::new(vec![song.clone(), song]);
        let at = |presentation, slide| SlideRef { presentation, slide };
        // The first and third slides of the first song picked, and a slide of the other song.
        let picks = vec![at(0, 0), at(0, 2), at(1, 1)];
        let mut state = HomeState { picked_slides: picks.clone(), ..HomeState::default() };
        let changed = |request: Option<SlideRequest>| match request {
            Some(SlideRequest::Edit(edits)) => edits.into_iter().map(|(at, slide)| (at, slide.enabled)).collect(),
            other => panic!("asked {other:?}"),
        };
        // Disable takes both picked slides of this song, in one step, and none of the other's.
        let disabled: Vec<(usize, bool)> = changed(slide_menu_click(&show, &mut state, at(0, 2), "Disable Slide"));
        assert_eq!(disabled, [(0, false), (2, false)]);
        // Delete says how many, and takes them at once.
        let deleted = slide_menu_click(&show, &mut state, at(0, 0), "Delete 2 Slides");
        assert_eq!(deleted, Some(SlideRequest::DeleteMany(vec![0, 2])));
        // On a slide not among them, it alone.
        state.picked_slides = picks;
        let disabled: Vec<(usize, bool)> = changed(slide_menu_click(&show, &mut state, at(0, 1), "Disable Slide"));
        assert_eq!(disabled, [(1, false)]);
        let deleted = slide_menu_click(&show, &mut state, at(0, 1), "Delete");
        assert_eq!(deleted, Some(SlideRequest::DeleteMany(vec![1])));
    }

    #[test]
    fn under_a_playlists_last_item_there_is_room_for_it_to_reach_the_top_and_a_line_starts_it() {
        let grid = Grid::new(SlideView::Grid, 760.0, 3);
        let shapes = [
            Shape { bar: SONG_BAR, strip: 0.0, body: Body::Tiles(7) },
            Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Line },
        ];
        let (places, total) = lay_out(&shapes, &grid);
        let last = places[1];
        // As tall as the view less the last item: scrolled to the end, the last is at the top.
        assert_eq!(scroll_height(&places, total, 700.0, false), last.top + 700.0);
        // In a view shorter than the last item, the items' own height (with the room under
        // the last) is enough already.
        let short = last.bottom() - last.top - 10.0;
        assert_eq!(scroll_height(&places, total, short, false), total);
        // The faint line is a little under the last item, in either case.
        assert_eq!(tail_line(&places, false), Some(last.bottom() + TAIL_GAP));
        // A song shown by itself keeps its room to drop a picture or video on, and no line.
        let alone = &places[..1];
        let (_, alone_total) = lay_out(&shapes[..1], &grid);
        assert_eq!(scroll_height(alone, alone_total, 700.0, true), (alone_total + 90.0).max(700.0));
        assert_eq!(tail_line(alone, true), None);
        // An empty playlist: nothing to reach the top, no line.
        assert_eq!((scroll_height(&[], FOOT, 700.0, false), tail_line(&[], false)), (FOOT, None));
    }

    #[test]
    fn items_are_laid_out_one_under_another_without_drawing_them() {
        let grid = Grid::new(SlideView::Grid, 760.0, 3);
        assert_eq!(grid.columns, 3);
        let shapes = [
            Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Tiles(1) },
            Shape { bar: HEADING, strip: 0.0, body: Body::None },
            Shape { bar: SONG_BAR, strip: 0.0, body: Body::Tiles(7) },
            Shape { bar: ITEM_BAR, strip: 0.0, body: Body::Box },
            Shape { bar: SONG_BAR, strip: 30.0, body: Body::Tiles(100_000) },
        ];
        let (places, total) = lay_out(&shapes, &grid);
        let row = grid.tile.y;
        assert_eq!(places[0].top, 0.0);
        assert_eq!(places[1].top, ITEM_BAR + 2.0 * INSET + row);
        assert_eq!(places[2].top, places[1].top + HEADING);
        // Seven tiles, three to a row: three rows with the room between them.
        assert_eq!(places[2].body, 2.0 * INSET + 3.0 * row + 2.0 * GAP_Y);
        assert_eq!(places[3].body, grid.picture.y + 2.0 * INSET);
        assert_eq!(places[4].body_top(), places[4].top + SONG_BAR + 30.0);
        assert_eq!(total, places[4].bottom() + FOOT);
        // The second tile of the second row.
        assert_eq!(grid.tile_at(4), vec2(INSET + grid.tile.x + GAP_X, INSET + row + GAP_Y));
    }

    #[test]
    fn the_view_goes_to_the_slide_on_screen_only_when_it_is_out_of_sight() {
        let slide = |top: f32| egui::Rangef::new(top, top + 160.0);
        // In sight, clear of the bar at the top and of the fade at the bottom: the view stays.
        assert_eq!(follow_offset(1000.0, 600.0, slide(1100.0)), None);
        assert_eq!(follow_offset(1000.0, 600.0, slide(1000.0 + CLEAR_ABOVE)), None);
        // Under the bar at the top, or above the view: its top comes clear of the bar.
        assert_eq!(follow_offset(1000.0, 600.0, slide(1020.0)), Some(1020.0 - CLEAR_ABOVE));
        assert_eq!(follow_offset(1000.0, 600.0, slide(200.0)), Some(200.0 - CLEAR_ABOVE));
        // In the fade at the bottom, or below the view: its bottom comes clear of the fade.
        assert_eq!(follow_offset(1000.0, 600.0, slide(1420.0)), Some(1420.0 + 160.0 - 600.0 + CLEAR_BELOW));
        assert_eq!(follow_offset(0.0, 600.0, slide(5000.0)), Some(5160.0 - 600.0 + CLEAR_BELOW));
        // At the very top of the scroll there is nowhere further up to go.
        assert_eq!(follow_offset(0.0, 600.0, slide(10.0)), None);
        // A verse picked on the Bible page brings the rest of its passage with it, when it fits.
        let (picked, end) = (slide(1420.0), slide(1600.0));
        assert_eq!(with_passage_end(picked, Some(end), 600.0), egui::Rangef::new(1420.0, 1760.0));
        assert_eq!(
            follow_offset(1000.0, 600.0, with_passage_end(picked, Some(end), 600.0)),
            Some(1760.0 - 600.0 + CLEAR_BELOW)
        );
        assert_eq!(with_passage_end(picked, Some(slide(2000.0)), 600.0), picked, "too far to fit");
        assert_eq!(with_passage_end(picked, None, 600.0), picked);
    }

    #[test]
    fn a_playlist_is_shown_whole_and_only_what_is_in_view_asks_for_pictures() {
        let songs: Vec<(String, usize)> = (0..40).map(|n| (format!("Song {n}"), 50)).collect();
        let named: Vec<(&str, usize)> = songs.iter().map(|(title, slides)| (title.as_str(), *slides)).collect();
        let (show, mut state) = playlist(&named);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let mut asked = 0;
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
                ..RawInput::default()
            },
            |ui| {
                asked = 0;
                home_ui(
                    ui,
                    &show,
                    &mut state,
                    &Theme::builtin(),
                    &mut |_| {
                        asked += 1;
                        None
                    },
                    Preview::default(),
                    None,
                );
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(state.slide_rects.len(), 2000, "every slide of the playlist has its place");
        assert!(asked > 0 && asked < 40, "{asked} pictures asked for, of 2000 slides");
        // The second song's slides are under the first's: the playlist is one scroll.
        assert!(rect_of(&state, 1, 0).min.y > rect_of(&state, 0, 49).max.y);
    }

    #[test]
    fn a_click_on_a_slide_shows_it_and_the_pencil_and_the_more_button_never_do() {
        let (show, mut state) = playlist(&[("Alpha", 2), ("Beta", 3)]);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let tile = rect_of(&state, 1, 1);
        let middle = tile.max.y - STRIP / 2.0;
        // The "more" button, at the right of the strip under the picture.
        let more = pos2(tile.max.x - 3.0 - MORE_BUTTON / 2.0, middle);
        let asked = click(&ctx, &show, &mut state, more, &mut time);
        assert!(!went_live(&asked), "the more button opens a menu: {asked:?}");
        assert!(state.editing.is_none());
        // A click elsewhere closes the menu (on the name of what the left column lists).
        click(&ctx, &show, &mut state, pos2(110.0, crate::studio::TOP_BAR + 16.0 + 36.0 + 17.0), &mut time);
        state.chooser_open = false;
        // The pencil, beside it.
        let pencil = more - vec2(MORE_BUTTON, 0.0);
        let asked = click(&ctx, &show, &mut state, pencil, &mut time);
        assert!(!went_live(&asked), "the pencil opens the editor: {asked:?}");
        assert_eq!(state.editing.as_ref().map(|draft| draft.at), Some(SlideRef { presentation: 1, slide: 1 }));
        state.editing = None;
        assert_eq!(state.services.chosen, Some(0), "neither opened Beta");
        // The picture itself: the slide goes on screen and its item becomes the open one.
        let asked = click(&ctx, &show, &mut state, tile.center() - vec2(0.0, STRIP), &mut time);
        assert!(asked.contains(&Command::GoTo(SlideRef { presentation: 1, slide: 1 })), "{asked:?}");
        assert_eq!(state.services.chosen, Some(1));
    }

    /// One frame at `time`, then what it asked for carried out, as the app does. Returns what it
    /// asked for.
    fn frame_applied(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
    ) -> Vec<Command> {
        let mut at = time - 0.1;
        let asked = run(ctx, show, state, events, &mut at);
        for command in &asked {
            show.apply(*command);
        }
        asked
    }

    /// Down and up at `at`, at `time` and a little after, each a frame carried out by the app.
    fn press_and_release(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        at: egui::Pos2,
        time: f64,
    ) -> Vec<Command> {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut asked = frame_applied(ctx, show, state, vec![Event::PointerMoved(at), button(true)], time);
        asked.extend(frame_applied(ctx, show, state, vec![button(false)], time + 0.03));
        asked
    }

    #[test]
    fn set_to_a_double_click_the_slide_on_screen_double_clicked_adds_nothing_and_slides_moved_between_clicks_only_pick()
    {
        let (mut show, mut state) = playlist(&[("Alpha", 3), ("Beta", 12)]);
        state.choose_slide_click(crate::key_table::SlideClick::Double);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        let ctx = egui::Context::default();
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.0);
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.1);
        // A double-click on the slide on screen: its first click is a click in the item on
        // screen (it brings back words hidden, as ever); its second asks for nothing.
        let alpha = rect_of(&state, 0, 0).center() - vec2(0.0, STRIP);
        let first = press_and_release(&ctx, &mut show, &mut state, alpha, 0.5);
        assert_eq!(first, vec![Command::GoTo(SlideRef { presentation: 0, slide: 0 })]);
        let second = press_and_release(&ctx, &mut show, &mut state, alpha, 0.6);
        assert!(second.is_empty(), "{second:?}");
        // A double-click whose slides moved under the pointer between its clicks (here the
        // number of slides a row changed; on a computer, the column still scrolling): its
        // second click lands on another slide, which it only picks.
        let beta = SlideRef { presentation: 1, slide: 1 };
        let at = rect_of(&state, 1, 1).center() - vec2(0.0, STRIP);
        let asked = press_and_release(&ctx, &mut show, &mut state, at, 2.0);
        assert!(!went_live(&asked), "{asked:?}");
        assert_eq!(state.pick(&show), Some(beta), "the first click picks Beta's second slide");
        let under =
            |state: &HomeState| state.slide_rects.iter().find(|(_, rect)| rect.contains(at)).map(|(here, _)| *here);
        let mut moved = None;
        let ways: [fn(&mut HomeState); 4] = [
            |state| state.slide_view = SlideView::Table,
            |state| state.slide_view = SlideView::Easy,
            |state| state.columns = 2,
            |state| state.columns = 7,
        ];
        for way in ways {
            let (view, columns) = (state.slide_view, state.columns);
            way(&mut state);
            frame_applied(&ctx, &mut show, &mut state, Vec::new(), 2.05);
            if let Some(other) = under(&state).filter(|other| *other != beta && other.presentation == 1) {
                moved = Some(other);
                break;
            }
            (state.slide_view, state.columns) = (view, columns);
        }
        let other = moved.expect("another of Beta\u{2019}s slides under the pointer");
        let asked = press_and_release(&ctx, &mut show, &mut state, at, 2.1);
        assert!(!went_live(&asked), "{asked:?}");
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "Alpha stays on screen");
        assert_eq!(state.pick(&show), Some(other), "the slide it landed on is picked, and only picked");
        // A third click straight after is a triple-click, which counts as one click: it stays
        // picked. A double-click of its own on it puts it on screen.
        let asked = press_and_release(&ctx, &mut show, &mut state, at, 2.2);
        assert!(!went_live(&asked), "{asked:?}");
        assert_eq!(state.pick(&show), Some(other));
        press_and_release(&ctx, &mut show, &mut state, at, 3.0);
        let asked = press_and_release(&ctx, &mut show, &mut state, at, 3.1);
        assert!(asked.contains(&Command::GoTo(other)), "{asked:?}");
        assert_eq!(show.live(), Some(other));
    }

    #[test]
    fn set_to_a_double_click_a_click_on_a_playlists_passage_picks_that_verse_for_enter_and_a_double_click_shows_it() {
        let (mut show, mut state) = playlist(&[("Alpha", 1)]);
        state.choose_slide_click(crate::key_table::SlideClick::Double);
        let slots = [show.add_presentation(song("", 0)), show.add_presentation(song("", 0))];
        state.bible.slots = Some(slots);
        let saved = SavedPassage { lookup: "John 3:16-18".to_owned(), ..state.bible.settings() };
        state.services.add(Item::Passage(saved));
        state.services.chosen = Some(0);
        let alpha_on = SlideRef { presentation: 0, slide: 0 };
        show.apply(Command::GoTo(alpha_on));
        let ctx = egui::Context::default();
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.0);
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.1);
        let alpha = rect_of(&state, 0, 0);
        let second = alpha.center() + vec2(alpha.width() + GAP_X, alpha.height() + INSET + ITEM_BAR + INSET);
        // One click on John 3:17: the passage is looked up, its item opened; nothing goes on.
        let asked = press_and_release(&ctx, &mut show, &mut state, second, 0.5);
        assert!(!went_live(&asked), "{asked:?}");
        assert!(state.bible.go_live.is_none(), "nothing to go on screen");
        assert_eq!(state.services.chosen, Some(1), "its item is the open one");
        let (slot, passage) = state.bible.pending.take().expect("the passage was looked up");
        assert!(asked.contains(&Command::Select(slot)), "{asked:?}");
        // As the app does next: the passage goes into its place; the verse clicked is picked.
        let asked = state.bible.asked_slides(slot);
        crate::place_passage(&mut show, slot, passage, asked);
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.55);
        let verse = SlideRef { presentation: slot, slide: 1 };
        assert_eq!(state.pick(&show), Some(verse), "John 3:17 picked");
        assert_eq!(show.live(), Some(alpha_on), "Alpha stays on screen");
        assert!(state.playlist_open, "the playlist stays");
        // Enter (Show Selected, Space taking the pick) puts that verse on, not the passage's first.
        assert_eq!(state.pick_command(&show), Some(Command::GoTo(verse)));
        let listed = crate::running::Listed::Playlist(state.services.chosen);
        assert_eq!(
            crate::running::enter(&[], listed, state.pick(&show)),
            Some(crate::running::Do::Show(Command::GoTo(verse)))
        );
        // The second click of a double-click on it: looked up again, that verse to go on screen.
        press_and_release(&ctx, &mut show, &mut state, second, 0.6);
        assert_eq!(state.bible.go_live, Some(1), "the second verse");
        assert!(state.bible.pending.is_some());
    }

    #[test]
    fn a_passage_picked_by_a_click_after_a_clear_leaves_the_place_cleared_for_next_to_go_on_from() {
        let (mut show, mut state) = playlist(&[("Alpha", 1)]);
        state.choose_slide_click(crate::key_table::SlideClick::Double);
        let slots = [show.add_presentation(song("", 0)), show.add_presentation(song("", 0))];
        state.bible.slots = Some(slots);
        let saved = SavedPassage { lookup: "John 3:16-18".to_owned(), ..state.bible.settings() };
        state.services.add(Item::Passage(saved.clone()));
        state.services.chosen = Some(0);
        // A verse from the Bible view on screen, then cleared: the show's cursor stays on it.
        let reading = SavedPassage { lookup: "Psalm 23:1-3".to_owned(), ..state.bible.settings() };
        state.bible.open_saved(&reading, None, crate::home::SlidesOf::Passage);
        let (cleared, passage) = state.bible.pending.take().expect("looked up");
        let read_title = passage.title.clone();
        show.replace_presentation(cleared, Arc::new(passage));
        show.apply(Command::GoTo(SlideRef { presentation: cleared, slide: 0 }));
        show.apply(Command::ClearAll);
        assert_eq!((show.live(), show.cursor()), (None, Some(SlideRef { presentation: cleared, slide: 0 })));
        let ctx = egui::Context::default();
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.0);
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.1);
        let alpha = rect_of(&state, 0, 0);
        let second = alpha.center() + vec2(alpha.width() + GAP_X, alpha.height() + INSET + ITEM_BAR + INSET);
        // A click on the playlist passage's John 3:17: it is looked up into the other place, so
        // the place cleared still holds the reading the cursor is on.
        press_and_release(&ctx, &mut show, &mut state, second, 0.5);
        let (slot, passage) = state.bible.pending.take().expect("the passage was looked up");
        assert_ne!(slot, cleared, "not into the place the show's cursor is on");
        show.replace_presentation(slot, Arc::new(passage));
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.55);
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 1 }));
        let at_cursor = show.presentations()[cleared].title.clone();
        assert_eq!(at_cursor, read_title, "the reading cleared is still where the cursor is");
        // So no item of the playlist holds the cursor (as `cursor_item` finds it), and Next from
        // the phone or a Stream Deck, aimed at the passage's item with nothing on screen, starts
        // that item, as before, never the slide after the cursor in the passage picked.
        assert!(!state.bible.holds_kept(cleared, &at_cursor, &saved.lookup));
        let stops = [crate::running::Stop::Song { at: 0, slides: true }, crate::running::Stop::Passage];
        let titles = ["Alpha".to_owned(), saved.lookup.clone()];
        let situation = crate::running::Situation {
            stops: &stops,
            titles: &titles,
            open: Some(1),
            here: crate::running::Here::Elsewhere,
            at_end: false,
            at_start: true,
            continuous: false,
            picked: None,
            showing: false,
        };
        assert_eq!(crate::running::step(&situation, true), crate::running::Do::Start { item: 1, last: false });
    }

    #[test]
    fn set_to_a_double_click_a_group_sung_again_picked_by_a_click_goes_on_at_that_place() {
        let (mut show, mut state) = arranged();
        state.choose_slide_click(crate::key_table::SlideClick::Double);
        // The song was on screen and was cleared: nothing is on screen, so a click only picks.
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        show.apply(Command::ClearAll);
        let ctx = egui::Context::default();
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.0);
        frame_applied(&ctx, &mut show, &mut state, Vec::new(), 0.1);
        // Verse 2 is the fifth tile; the one before it is the chorus sung the third time.
        let (_, place) = state.slide_rects.iter().find(|(at, _)| at.slide == 2).copied().expect("Verse 2's tile");
        let third = Rect::from_min_size(place.min - vec2(place.width() + GAP_X, 0.0), place.size());
        let asked = press_and_release(&ctx, &mut show, &mut state, third.center(), 0.5);
        assert!(!went_live(&asked), "{asked:?}");
        let chorus = SlideRef { presentation: 0, slide: 1 };
        assert_eq!(state.pick(&show), Some(chorus));
        // Enter goes to that place in the order sung …
        let enter = state.pick_command(&show);
        assert_eq!(enter, Some(Command::GoToStep { presentation: 0, step: 3 }));
        show.apply(enter.expect("picked"));
        assert_eq!(show.live(), Some(chorus));
        assert_eq!(show.position().map(|(step, _)| step), Some(3));
        // … so Next goes on to Verse 2, not to the chorus after the first.
        show.apply(Command::Next);
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 2 }));
        // A slide picked by the keys has no place: Enter puts the slide on, as ever.
        state.set_pick(&show, chorus, false);
        assert_eq!(state.pick_command(&show), Some(Command::GoTo(chorus)));
    }

    #[test]
    fn a_click_on_a_bar_held_at_the_top_is_the_bars_not_the_slide_under_it() {
        let (show, mut state) = playlist(&[("Alpha", 60), ("Beta", 3)]);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let first = rect_of(&state, 0, 0);
        // Scrolled down into Alpha's slides: its bar stays at the top, over them.
        let over = first.center();
        run(&ctx, &show, &mut state, vec![Event::PointerMoved(over)], &mut time);
        let wheel = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -700.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        };
        run(&ctx, &show, &mut state, vec![wheel], &mut time);
        for _ in 0..30 {
            run(&ctx, &show, &mut state, Vec::new(), &mut time);
        }
        let top = first.min.y - INSET - SONG_BAR;
        let under: Vec<usize> = state
            .slide_rects
            .iter()
            .filter(|(at, rect)| at.presentation == 0 && rect.contains(pos2(over.x, top + 20.0)))
            .map(|(at, _)| at.slide)
            .collect();
        assert!(!under.is_empty(), "a slide has scrolled under where the bar is held");
        state.services.chosen = Some(1);
        state.open_seen = None;
        // To the right of the bar's name and tools, where only its hairline is.
        let asked = click(&ctx, &show, &mut state, pos2(over.x + 2.0 * first.width(), top + 20.0), &mut time);
        assert!(!went_live(&asked), "the bar took the click: {asked:?}");
        assert_eq!(state.services.chosen, Some(0), "which opens its item");
    }

    #[test]
    fn opening_an_item_chooses_its_song_and_a_song_chosen_elsewhere_opens_its_item() {
        let (mut show, mut state) = playlist(&[("Alpha", 2), ("Beta", 2), ("Gamma", 2)]);
        show.add_presentation(song("Outside", 2));
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        let mut step = |show: &mut Show, state: &mut HomeState| {
            let asked = run(&ctx, show, state, Vec::new(), &mut time);
            for command in &asked {
                show.apply(*command);
            }
            asked
        };
        step(&mut show, &mut state);
        // An item opened (by its row, ↑ and ↓, the command line): its song is chosen.
        state.services.chosen = Some(2);
        let asked = step(&mut show, &mut state);
        assert_eq!(asked, vec![Command::Select(2)]);
        step(&mut show, &mut state);
        // Another song chosen (Next going on into it, a search): its item becomes the open one.
        show.apply(Command::Select(1));
        let asked = step(&mut show, &mut state);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(state.services.chosen, Some(1));
        assert!(state.playlist_open);
        // A song the playlist doesn't have is shown by itself, from the library.
        show.apply(Command::Select(3));
        step(&mut show, &mut state);
        assert!(!state.playlist_open, "the left column goes back to the library");
        assert_eq!(state.slide_rects.len(), 2, "only its own slides are shown");
        assert!(show.live().is_none(), "none of this put anything on screen");
    }

    #[test]
    fn a_playlists_bible_passage_is_tiles_and_a_click_on_one_puts_that_verse_on_screen() {
        let (mut show, mut state) = playlist(&[("Alpha", 1)]);
        // The Bible page's two places among the presentations.
        let slots = [show.add_presentation(song("", 0)), show.add_presentation(song("", 0))];
        state.bible.slots = Some(slots);
        let saved = SavedPassage { lookup: "John 3:16-18".to_owned(), ..state.bible.settings() };
        state.services.add(Item::Passage(saved.clone()));
        state.services.chosen = Some(0);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let made = decks(&show, &mut state, &mut Vec::new());
        let Some(Deck::Slides(passage)) = made.get(1) else { panic!("the passage is slides") };
        assert_eq!(passage.deck.slide_count(), 3, "a slide for each verse");
        assert!(passage.at.is_none() && passage.made && passage.bible);
        let title = passage.deck.title.clone();
        // Its tiles are under Alpha's one slide: past the room under it, and the passage's bar.
        let alpha = rect_of(&state, 0, 0);
        let second = alpha.center() + vec2(alpha.width() + GAP_X, alpha.height() + INSET + ITEM_BAR + INSET);
        let asked = click(&ctx, &show, &mut state, second, &mut time);
        assert!(!went_live(&asked), "the verse goes on screen once the passage is among the presentations");
        assert_eq!(state.bible.go_live, Some(1), "the second verse");
        let (slot, looked_up) = state.bible.pending.take().expect("the passage was looked up");
        assert_eq!(looked_up.title, title);
        assert_eq!(state.services.chosen, Some(1), "and the passage is the open item");
        assert!(!state.bible_open, "the Service view stays");
        // As the app does next: the passage goes into its place and its slide on screen.
        let asked = state.bible.asked_slides(slot);
        crate::place_passage(&mut show, slot, looked_up, asked);
        show.apply(Command::GoTo(SlideRef { presentation: slot, slide: 1 }));
        let made = decks(&show, &mut state, &mut Vec::new());
        let Some(Deck::Slides(passage)) = made.get(1) else { panic!("the passage is slides") };
        assert_eq!(passage.live_slide(&show, &state), Some(1), "its tile is the one lit");
        let Some(Deck::Slides(alpha)) = made.first() else { panic!("Alpha is slides") };
        assert_eq!(alpha.live_slide(&show, &state), None);
        // A passage that can't be made asks for nothing to go on screen later.
        let nowhere = SavedPassage { lookup: "Nosuchbook 1:1".to_owned(), ..saved };
        let broken = SlideDeck {
            item: Some(1),
            at: None,
            deck: song("", 0),
            passage: Some(nowhere),
            made: true,
            bible: true,
            bar: true,
            ringed: None,
        };
        broken.show_slide(&show, &mut state, 0, &mut Vec::new());
        assert!(state.bible.pending.is_none(), "{:?}", state.bible.pending.as_ref().map(|(_, deck)| &deck.title));
        assert!(state.bible.go_live.is_none());
    }

    /// How far Redo is from Undo in the bar.
    const UNDO_STEP: f32 = 70.0;

    #[test]
    fn the_bar_under_the_slides_switches_continuous_and_asks_for_undo() {
        let (show, mut state) = playlist(&[("Alpha", 2)]);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        // The bar is over the media strip, at the foot of the middle column, which starts
        // after the left column. In it: the transition switch, a line, Continuous, a line,
        // Undo and Redo.
        let middle = 800.0 - crate::studio::GAP - 151.0 - 49.0 + 25.0;
        let left = crate::studio::LEFT_WIDTH + 2.0 * crate::studio::GAP + 20.0;
        let continuous = pos2(left + 210.7 + 13.0 + 47.0, middle);
        let asked = click(&ctx, &show, &mut state, continuous, &mut time);
        assert!(state.continuous, "Continuous is switched on");
        assert!(asked.is_empty(), "{asked:?}");
        click(&ctx, &show, &mut state, continuous, &mut time);
        assert!(!state.continuous, "and off again");
        let undo = continuous + vec2(47.0 + 13.0 + 29.0, 0.0);
        // With nothing to undo the button is greyed and asks for nothing.
        click(&ctx, &show, &mut state, undo, &mut time);
        assert_eq!(state.menu_asked, None, "greyed: nothing to undo");
        state.show_history = (true, false);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        click(&ctx, &show, &mut state, undo, &mut time);
        assert_eq!(state.menu_asked, Some(crate::menubar::Action::Undo));
        state.menu_asked = None;
        // Redo, next to it, is still greyed.
        click(&ctx, &show, &mut state, undo + vec2(UNDO_STEP, 0.0), &mut time);
        assert_eq!(state.menu_asked, None, "greyed: nothing to redo");
        // The transition: Cut is the first part of the switch.
        assert_ne!(state.transition.kind, sp_model::TransitionKind::Cut);
        click(&ctx, &show, &mut state, pos2(left + 20.0, middle), &mut time);
        assert_eq!(state.transition.kind, sp_model::TransitionKind::Cut);
        assert!(show.live().is_none());
    }

    #[test]
    fn a_click_on_a_playlist_row_only_opens_the_item() {
        let (mut show, mut state) = playlist(&[("Alpha", 2), ("Beta", 2)]);
        let slots = [show.add_presentation(song("", 0)), show.add_presentation(song("", 0))];
        state.bible.slots = Some(slots);
        let saved = SavedPassage { lookup: "John 3:16-18".to_owned(), ..state.bible.settings() };
        state.services.add(Item::Header { name: "The Word".to_owned() });
        state.services.add(Item::Passage(saved));
        state.services.chosen = Some(0);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        // The rows start under the label and the name: 42 high each, a header 34.5.
        let first = crate::studio::TOP_BAR + 16.0 + 36.0 + 39.6;
        let row = |top: f32| pos2(110.0, first + top + 15.0);
        // Beta's row: Beta is opened (chosen, and scrolled to), and the screens keep Alpha.
        let asked = click(&ctx, &show, &mut state, row(42.0), &mut time);
        assert!(!asked.is_empty() && asked.iter().all(|command| *command == Command::Select(1)), "{asked:?}");
        show.apply(Command::Select(1));
        assert_eq!(state.services.chosen, Some(1));
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 1 }));
        // The header: nothing is opened; the slides go to it.
        state.slides_jump_to = None;
        let before = state.services.chosen;
        run(&ctx, &show, &mut state, vec![Event::PointerMoved(row(84.0))], &mut time);
        let asked = click(&ctx, &show, &mut state, row(84.0), &mut time);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(state.services.chosen, before);
        // The passage: opened in the Service view; the Bible page is left as it was.
        let asked = click(&ctx, &show, &mut state, row(84.0 + 34.5), &mut time);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(state.services.chosen, Some(3));
        assert!(!state.bible_open, "the Service view stays");
        assert!(state.bible.pending.is_none() && state.bible.go_live.is_none(), "nothing was looked up");
        // Open on the Bible Page, in its menu, is how it gets there.
        crate::studio::carry_out_item_ask(&show, &mut state, &mut Vec::new(), ItemAsk::Bible(3));
        assert!(state.bible_open && state.bible.pending.is_some());
    }

    #[test]
    fn the_view_goes_to_the_slide_on_screen_and_then_stays_where_it_is_scrolled() {
        let (mut show, mut state) = playlist(&[("Alpha", 60), ("Beta", 30)]);
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let top = rect_of(&state, 0, 0).min.y - INSET - SONG_BAR;
        let bottom = 800.0 - crate::studio::GAP - 151.0 - 49.0;
        // A slide far down Beta goes on screen (Next going on into it, say).
        let far = SlideRef { presentation: 1, slide: 20 };
        assert!(rect_of(&state, 1, 20).min.y > bottom, "it starts out of sight");
        show.apply(Command::GoTo(far));
        for _ in 0..4 {
            run(&ctx, &show, &mut state, Vec::new(), &mut time);
        }
        let now = rect_of(&state, 1, 20);
        assert!(now.min.y >= top + CLEAR_ABOVE - 1.0, "clear of the bar at the top: {now:?}");
        assert!(now.max.y <= bottom - CLEAR_BELOW + 1.0, "clear of the fade at the bottom: {now:?}");
        assert_eq!(state.services.chosen, Some(1), "and its item is the open one");
        // Scrolled away by hand: the view stays there while the same slide is on screen.
        let over = pos2(now.center().x, now.center().y);
        run(&ctx, &show, &mut state, vec![Event::PointerMoved(over)], &mut time);
        let wheel = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 900.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        };
        run(&ctx, &show, &mut state, vec![wheel], &mut time);
        for _ in 0..30 {
            run(&ctx, &show, &mut state, Vec::new(), &mut time);
        }
        let away = rect_of(&state, 1, 20);
        assert!(away.min.y > bottom, "the operator's scrolling is left alone: {away:?}");
    }

    #[test]
    fn a_playlist_of_many_passages_is_made_a_few_a_frame_and_asks_to_be_drawn_until_all_are() {
        let (mut show, mut state) = playlist(&[("Alpha", 1)]);
        let slots = [show.add_presentation(song("", 0)), show.add_presentation(song("", 0))];
        state.bible.slots = Some(slots);
        for verse in 1..=5 {
            let saved = SavedPassage { lookup: format!("John 3:{verse}"), ..state.bible.settings() };
            state.services.add(Item::Passage(saved));
        }
        state.services.chosen = Some(0);
        let made =
            |decks: &[Deck]| decks.iter().filter(|deck| matches!(deck, Deck::Slides(slides) if slides.bible)).count();
        let first = decks(&show, &mut state, &mut Vec::new());
        assert_eq!(made(&first), 2, "two in the first frame");
        assert!(state.passages.waiting, "and the window is to be drawn again for the rest");
        let second = decks(&show, &mut state, &mut Vec::new());
        assert_eq!(made(&second), 4);
        let third = decks(&show, &mut state, &mut Vec::new());
        assert_eq!(made(&third), 5);
        assert!(!state.passages.waiting, "all made: nothing more is asked for");
        // Made once: the same slides, with the same ids, from then on.
        let ids = |decks: &[Deck]| -> Vec<Uuid> {
            decks
                .iter()
                .filter_map(|deck| match deck {
                    Deck::Slides(slides) if slides.bible => slides.deck.slides().next().map(|(_, slide)| slide.id),
                    _ => None,
                })
                .collect()
        };
        let again = decks(&show, &mut state, &mut Vec::new());
        assert_eq!(ids(&third), ids(&again));
    }

    #[test]
    fn songs_carried_out_of_the_list_can_be_dropped_on_a_playlist_in_the_chooser() {
        let songs = ["Alpha", "Beta", "Gamma"].map(|title| song(title, 1));
        let show = Show::new(songs.to_vec());
        let song_keys = ["Alpha", "Beta", "Gamma"].map(|title| format!("title:{title}")).to_vec();
        let mut state = HomeState { song_keys, ..HomeState::default() };
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let first = crate::studio::TOP_BAR + 16.0 + 36.0 + 39.6;
        let from = pos2(110.0, first + 21.0);
        let press = |at: egui::Pos2, pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(&ctx, &show, &mut state, vec![Event::PointerMoved(from)], &mut time);
        run(&ctx, &show, &mut state, vec![press(from, true)], &mut time);
        // Out of the list, into the middle: the song is carried, and the chooser shows.
        for at in [from + vec2(40.0, 4.0), pos2(400.0, 300.0), pos2(500.0, 300.0)] {
            run(&ctx, &show, &mut state, vec![Event::PointerMoved(at)], &mut time);
        }
        assert!(!state.chooser_open, "the chooser shows only while the song is carried");
        // Back onto the one playlist's row: under the Playlists heading.
        let row = pos2(110.0, first + 36.0 + 18.0);
        for _ in 0..2 {
            run(&ctx, &show, &mut state, vec![Event::PointerMoved(row)], &mut time);
        }
        run(&ctx, &show, &mut state, vec![press(row, false)], &mut time);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let items = &state.services.front().items;
        assert!(matches!(items.as_slice(), [Item::Song { title, .. }] if title == "Alpha"), "{items:?}");
    }

    /// The arrangement menu opened on a song, one frame with these events: the words drawn in
    /// it, and where.
    fn arrangement_menu_frame(
        ctx: &egui::Context,
        song: &Presentation,
        state: &mut HomeState,
        time: f64,
        events: Vec<Event>,
        open: bool,
    ) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let input = RawInput {
            time: Some(time),
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(input, |ui| {
            let button = ui.button("Arrangement");
            if open {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&button));
            }
            arrangement_menu(0, song, state, &button);
        });
        let mut words = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
        output.drop_without_applying_deltas();
        words
    }

    #[test]
    fn chorus_after_each_verse_in_the_arrangement_menu_makes_the_arrangement_and_chooses_it() {
        use crate::i18n::{Language, with_language};
        // A song from before, sung as written: Verse 1, Chorus, Verse 2.
        let song = parse_song_text("Old", "[Verse 1]\nA\n\n[Chorus]\nB\n\n[Verse 2]\nC\n");
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let item = "Chorus After Each Verse";
        arrangement_menu_frame(&ctx, &song, &mut state, 0.0, Vec::new(), true);
        let words = arrangement_menu_frame(&ctx, &song, &mut state, 0.1, Vec::new(), false);
        let place = words.iter().find(|(said, _)| said == item).map(|(_, at)| at.center()).expect("offered");
        let press = |pressed| Event::PointerButton {
            pos: place,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for (at, events) in
            [vec![Event::PointerMoved(place)], vec![press(true)], vec![press(false)]].into_iter().enumerate()
        {
            arrangement_menu_frame(&ctx, &song, &mut state, 0.2 + at as f64 / 10.0, events, false);
        }
        let (index, arrangements, chosen) = state.arrangement_request.take().expect("asked for");
        assert_eq!(index, 0);
        let [made] = arrangements.as_slice() else { panic!("one made: {arrangements:?}") };
        assert!(made.auto);
        assert_eq!(made.name, "Chorus after each verse");
        assert_eq!(made.name, sp_model::CHORUS_AFTER_EACH_VERSE);
        let [v1, chorus, v2] = [song.groups[0].id, song.groups[1].id, song.groups[2].id];
        assert_eq!(made.groups, [v1, chorus, v2, chorus]);
        assert_eq!(chosen, Some(made.id), "and chosen");
        // In Russian it is made with its Russian name.
        assert_eq!(with_language(Language::Russian, chorus_after_name), "Припев после каждого куплета");
        assert_eq!(with_language(Language::Ukrainian, chorus_after_name), "Приспів після кожного куплету");
        // A song without a chorus isn't offered it.
        let verses = parse_song_text("Verses", "[Verse 1]\nA\n\n[Verse 2]\nB\n");
        let ctx = egui::Context::default();
        arrangement_menu_frame(&ctx, &verses, &mut state, 0.0, Vec::new(), true);
        let words = arrangement_menu_frame(&ctx, &verses, &mut state, 0.1, Vec::new(), false);
        assert!(words.iter().any(|(said, _)| said == "New Arrangement"), "the menu is open: {words:?}");
        assert!(!words.iter().any(|(said, _)| said == item), "{words:?}");
    }

    #[test]
    fn an_arrangement_renamed_or_changed_by_hand_no_longer_follows_the_songs_parts() {
        let mut song = parse_song_text("Song", "[Verse 1]\nA\n\n[Chorus]\nB\n\n[Verse 2]\nC\n");
        song.follow_parts_as_text("Chorus after each verse");
        let made = song.arrangements[0].clone();
        assert!(made.auto);
        // A pill taken out (or moved, or sung more times): the operator's from then on.
        let mut groups = made.groups.clone();
        groups.pop();
        let changed = changed_by_hand(&song, made.id, groups.clone());
        assert_eq!((changed[0].groups.clone(), changed[0].auto), (groups, false));
        // Renamed in the menu: the same.
        let mut arrangements = song.arrangements.clone();
        arrangements[0].name = "Sunday".to_owned();
        renamed_by_hand(&song, &mut arrangements);
        assert!(!arrangements[0].auto);
        // Only chosen, or not renamed: still following.
        let mut arrangements = song.arrangements.clone();
        renamed_by_hand(&song, &mut arrangements);
        assert!(arrangements[0].auto);
        // And a song whose parts change afterwards leaves it as it was left.
        song.arrangements = changed;
        let edited = song.reflowed("[Verse 1]\nA\n\n[Chorus]\nB\n\n[Verse 2]\nC\n\n[Verse 3]\nD\n");
        assert_eq!(edited.arrangements, song.arrangements);
    }

    #[test]
    fn a_new_arrangement_is_named_in_the_language_set() {
        use crate::i18n::{Language, with_language};
        let (show, _) = playlist(&[("Alpha", 2)]);
        let mut song = (*show.presentations()[0]).clone();
        let first = with_language(Language::Spanish, || song.new_arrangement(arrangement_name));
        assert_eq!(first.name, "Arreglo 1");
        song.arrangements.push(first);
        assert_eq!(with_language(Language::Spanish, || song.new_arrangement(arrangement_name)).name, "Arreglo 2");
        assert_eq!(with_language(Language::English, || song.new_arrangement(arrangement_name)).name, "Arrangement 1");
    }

    #[test]
    fn a_group_of_an_arrangement_goes_to_its_own_song_not_the_one_chosen() {
        let (mut show, mut state) = playlist(&[("Alpha", 2), ("Beta", 2)]);
        // Beta is played in an arrangement; Alpha is the song chosen.
        let mut beta = (*show.presentations()[1]).clone();
        let made = beta.new_arrangement(arrangement_name);
        beta.arrangement = Some(made.id);
        beta.arrangements.push(made);
        show.replace_presentation(1, Arc::new(beta));
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        assert_eq!(show.selected(), 0);
        // The strip is between Beta's bar and its slides: its name first, then its groups.
        let tile = rect_of(&state, 1, 0);
        let layout = {
            let mut found = None;
            let output = ctx.run_ui(RawInput::default(), |ui| {
                found = arrangement_layout(ui, &show.presentations()[1], 600.0)
                    .map(|layout| (layout.chips[0].rect, layout.height));
            });
            output.drop_without_applying_deltas();
            found.expect("Beta has an arrangement")
        };
        let (chip, height) = layout;
        let strip_top = tile.min.y - INSET - height;
        // On the pill's dot and name (its arrow, at the right end, opens its menu).
        let at = pos2(tile.min.x - INSET + chip.min.x + 14.0, strip_top + chip.center().y);
        let asked = click(&ctx, &show, &mut state, at, &mut time);
        assert!(asked.contains(&Command::GoToStep { presentation: 1, step: 0 }), "{asked:?}");
        assert!(!asked.iter().any(|c| matches!(c, Command::GoToStep { presentation: 0, .. } | Command::GoTo(_))));
    }

    /// A song with Verse 1, Chorus and Verse 2 (one slide each), sung Verse 1, Chorus ×3,
    /// Verse 2, in a playlist of its own.
    fn arranged() -> (Show, HomeState) {
        let mut song = parse_song_text("Arranged", "[Verse 1]\nA\n\n[Chorus]\nB\n\n[Verse 2]\nC\n");
        let [v1, chorus, v2] = [song.groups[0].id, song.groups[1].id, song.groups[2].id];
        let arrangement = sp_model::Arrangement {
            id: Uuid::new_v4(),
            name: "As sung".to_owned(),
            groups: vec![v1, chorus, chorus, chorus, v2],
            auto: false,
        };
        song.arrangement = Some(arrangement.id);
        song.arrangements.push(arrangement);
        let show = Show::new(vec![Arc::new(song)]);
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.add(Item::song("title:Arranged".to_owned(), "Arranged".to_owned()));
        state.services.chosen = Some(0);
        (show, state)
    }

    #[test]
    fn a_group_sung_three_times_running_is_one_pill_and_its_slides_stand_in_the_order_sung() {
        let (show, mut state) = arranged();
        let ctx = egui::Context::default();
        let mut pills = Vec::new();
        ctx.run_ui(RawInput::default(), |ui| {
            let layout = arrangement_layout(ui, &show.presentations()[0], 700.0).expect("arranged");
            pills = layout.chips.iter().map(|chip| chip.run.count).collect();
        })
        .drop_without_applying_deltas();
        assert_eq!(pills, [1, 3, 1], "Verse 1, Chorus ×3, Verse 2");
        assert_eq!(chip_mark(3, None).as_deref(), Some("\u{d7}3"));
        assert_eq!(chip_mark(3, Some(2)).as_deref(), Some("2/3"), "the second time through, on screen");
        assert_eq!(chip_mark(1, None), None);
        // Five tiles in the order sung: the chorus three times, the second and third AGAIN.
        let order = entries(&show.presentations()[0]);
        assert_eq!(order.iter().map(|e| e.slide).collect::<Vec<_>>(), [0, 1, 1, 1, 2]);
        assert_eq!(order.iter().map(|e| e.again).collect::<Vec<_>>(), [false, false, true, true, false]);
        // A click on the third chorus tile puts the song at that place, not at the first chorus.
        let mut time = 0.0;
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let third = state.slide_rects.iter().filter(|(at, _)| at.slide == 1).count();
        assert_eq!(third, 1, "a slide sung again is picked where it first stands");
        let (_, place) = state.slide_rects.iter().find(|(at, _)| at.slide == 2).copied().expect("Verse 2's tile");
        // Verse 2 is the fifth tile: the one before it is the third chorus.
        let before = Rect::from_min_size(place.min - vec2(place.width() + GAP_X, 0.0), place.size());
        let asked = click(&ctx, &show, &mut state, before.center(), &mut time);
        assert!(asked.contains(&Command::GoToStep { presentation: 0, step: 3 }), "{asked:?}");
    }

    #[test]
    fn a_piece_lands_before_or_after_the_nearest_by_which_half_the_pointer_is_over() {
        let pill = |x: f32| Rect::from_min_size(pos2(x, 0.0), vec2(80.0, 26.0));
        let chips = [pill(0.0), pill(86.0), pill(172.0)];
        let starts = [0, 1, 4];
        // The last piece onto the left half of the first: in front.
        assert_eq!(piece_drop(&chips, &starts, 4, pos2(10.0, 13.0)), Some((0, false)));
        // The first onto the right half of the second: after it.
        assert_eq!(piece_drop(&chips, &starts, 0, pos2(160.0, 13.0)), Some((1, true)));
        // Past the end of the row: after the last.
        assert_eq!(piece_drop(&chips, &starts, 0, pos2(400.0, 13.0)), Some((4, true)));
        // Beside where it already stands, or on itself: nothing changes.
        assert_eq!(piece_drop(&chips, &starts, 0, pos2(90.0, 13.0)), None);
        assert_eq!(piece_drop(&chips, &starts, 1, pos2(100.0, 13.0)), None);
    }

    #[test]
    fn a_slide_dragged_lands_before_or_after_a_tile_by_which_half_the_pointer_is_over() {
        let (show, mut state) = playlist(&[("Alpha", 4)]);
        let ctx = egui::Context::default();
        let mut time = 0.0;
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let (from, onto) = (rect_of(&state, 0, 0), rect_of(&state, 0, 2));
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let drag_to = |state: &mut HomeState, time: &mut f64, to: egui::Pos2| {
            let start = from.center();
            for events in [
                vec![Event::PointerMoved(start)],
                vec![button(start, true)],
                vec![Event::PointerMoved(start + vec2(30.0, 0.0))],
                vec![Event::PointerMoved(to)],
                vec![Event::PointerMoved(to)],
                vec![button(to, false)],
                Vec::new(),
            ] {
                run(&ctx, &show, state, events, time);
            }
            state.slide_request.take()
        };
        let right_half = pos2(onto.max.x - 10.0, onto.center().y);
        match drag_to(&mut state, &mut time, right_half) {
            Some((_, SlideRequest::Move { slides, to, after })) => assert_eq!((slides, to, after), (vec![0], 2, true)),
            other => panic!("expected a move after the third slide, got {other:?}"),
        }
        let left_half = pos2(onto.min.x + 10.0, onto.center().y);
        match drag_to(&mut state, &mut time, left_half) {
            Some((_, SlideRequest::Move { to, after, .. })) => assert_eq!((to, after), (2, false)),
            other => panic!("expected a move before the third slide, got {other:?}"),
        }
    }

    #[test]
    fn a_press_on_a_pill_that_barely_moves_is_a_click() {
        let (show, mut state) = arranged();
        let ctx = egui::Context::default();
        let mut time = 0.0;
        run(&ctx, &show, &mut state, Vec::new(), &mut time);
        let tile = rect_of(&state, 0, 0);
        let mut layout = None;
        ctx.run_ui(RawInput::default(), |ui| {
            layout = arrangement_layout(ui, &show.presentations()[0], 700.0).map(|l| (l.chips[1].rect, l.height));
        })
        .drop_without_applying_deltas();
        let (chip, height) = layout.expect("arranged");
        let at = pos2(tile.min.x - INSET + chip.min.x + 14.0, tile.min.y - INSET - height + chip.center().y);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let moved = at + vec2(2.0, 1.0);
        let mut asked = Vec::new();
        for events in [
            vec![Event::PointerMoved(at)],
            vec![button(at, true)],
            vec![Event::PointerMoved(moved)],
            vec![button(moved, false)],
            Vec::new(),
        ] {
            asked.extend(run(&ctx, &show, &mut state, events, &mut time));
        }
        assert!(asked.contains(&Command::GoToStep { presentation: 0, step: 1 }), "the chorus pill: {asked:?}");
        assert!(egui::DragAndDrop::payload::<PieceDrag>(&ctx).is_none(), "nothing is being carried");
    }
}
