//! The middle of the Show view: the open playlist as one scroll, item after item, each under a
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
}

/// What stands under an item's bar.
#[derive(Clone, Debug)]
pub enum Body {
    /// Nothing (a playlist header).
    None,
    /// Tiles in a grid: how many.
    Tiles(usize),
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
        for (_, slide) in deck.slides() {
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
                state.bible.open_saved(saved, show.live().map(|at| at.presentation));
                // Only if the passage was made: a slide kept waiting would go on screen with
                // the next passage anyone looks up.
                state.bible.go_live = state.bible.pending.is_some().then_some(slide);
            }
            (None, None) => {}
        }
        if self.item.is_some() {
            state.services.chosen = self.item;
        }
    }
}

/// What the middle of the Show view shows: the playlist in front, every item of it, when the
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
                    }),
                    Some(Err(problem)) => Deck::Note { item, title, kind: "Bible", words: problem },
                    None => Deck::Note { item, title, kind: "Bible", words: "Looking it up\u{2026}".to_owned() },
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
                }),
                None => Deck::Note { item, title: String::new(), kind: "Missing", words: String::new() },
            },
            (Item::Song { title, .. }, None) => Deck::Note {
                item,
                title,
                kind: "Missing",
                words: "This song is not in the library any more.".to_owned(),
            },
        })
        .collect()
}

/// One song (or the Bible page's passage, or the VoxLive item) shown by itself.
fn song_alone(show: &Show, state: &HomeState, which: usize, bar: bool) -> Option<Deck> {
    let deck = show.presentations().get(which)?.clone();
    let bible = state.bible.slots.is_some_and(|slots| slots.contains(&which));
    Some(Deck::Slides(SlideDeck {
        item: None,
        at: Some(which),
        deck,
        passage: None,
        made: state.app_made(which),
        bible,
        bar,
    }))
}

/// The song each item of the playlist in front names, by its place among the songs, looked
/// up again only when the songs or the items change.
fn playlist_songs(show: &Show, state: &mut HomeState) -> Arc<Vec<Option<usize>>> {
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
        commands.push(Command::Select(song));
        return true;
    }
    if was_selected != selected && !state.hidden_slots().contains(&selected) {
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

    /// Where a slide of an item is: its tile, or its row.
    fn slide(&self, place: &Place, body: &Body, index: usize) -> Rect {
        match body {
            Body::Rows(rows) if index < rows.tops.len() => self.row(place, rows, index),
            _ => self.tile(place, index),
        }
    }

    fn in_view(&self, rect: Rect) -> bool {
        rect.max.y >= self.clip.min.y && rect.min.y <= self.clip.max.y
    }

    /// The whole width of the scroll between two heights of its contents.
    fn band(&self, top: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(self.clip.min.x, self.y(top)), vec2(self.clip.width(), height))
    }
}

/// How much room an item takes.
fn shape_of(ui: &egui::Ui, state: &mut HomeState, deck: &Deck, sheet_grid: &Grid, columns: &ListColumns) -> Shape {
    match deck {
        Deck::Heading { .. } => Shape { bar: HEADING, strip: 0.0, body: Body::None },
        Deck::Slides(slides) => {
            let count = slides.deck.slide_count();
            let tools = slides.at.is_some() && !slides.made;
            let bar = match (slides.bar, tools) {
                (false, _) => 0.0,
                (true, true) => SONG_BAR,
                (true, false) => ITEM_BAR,
            };
            let strip = if tools {
                arrangement_layout(ui, &slides.deck, sheet_grid.width).map_or(0.0, |layout| layout.height)
            } else {
                0.0
            };
            let body = if count == 0 {
                Body::Line
            } else if sheet_grid.view == SlideView::Grid {
                Body::Tiles(count)
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
    let mut ask: Option<ItemAsk> = None;
    let mut asks = SongAsks::default();
    let alone = matches!(decks, [Deck::Slides(SlideDeck { item: None, .. })]);
    let area = egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]);
    area.show_viewport(ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        let across = (ui.available_width() - 2.0 * EDGE).max(60.0);
        let grid = Grid::new(view, across, state.columns);
        let left = origin.x + EDGE;
        let columns = ListColumns::new(view, left, across);
        let shapes: Vec<Shape> = decks.iter().map(|deck| shape_of(ui, state, deck, &grid, &columns)).collect();
        let (places, total) = lay_out(&shapes, &grid);
        // The scroll is as tall as its items, with room under a song by itself to drop a
        // picture or video on.
        let total = if alone { (total + 90.0).max(viewport.height()) } else { total };
        ui.allocate_rect(Rect::from_min_size(origin, vec2(ui.available_width(), total)), Sense::hover());
        // Only the scroll's own width: a scroll area that scrolls up and down leaves the
        // width of its clip to whatever holds it, which can be wider.
        let clip = ui.clip_rect().intersect(Rect::from_x_y_ranges(ui.max_rect().x_range(), ui.clip_rect().y_range()));
        let sheet = Sheet { origin, left, grid, columns, clip };
        // Between the slides: a click picks nothing; a drag draws a box that picks out every
        // slide it touches.
        let background = ui.interact(clip, ui.scope_id().with("slide background"), Sense::click_and_drag());
        if background.clicked() {
            state.picked_slides.clear();
            state.selected_slide = None;
        }
        start_slide_box(ui, state, &background);
        scroll_while_dragging(ui);
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
                for slide in 0..slides.deck.slide_count() {
                    state.slide_rects.push((SlideRef { presentation, slide }, sheet.slide(place, &shape.body, slide)));
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
        let live_now = show.live().map(|live| (live, show.position().map(|(step, _)| step)));
        if live_now != state.live_seen {
            state.live_seen = live_now;
            let open = state.services.chosen;
            // The open item's slide first, when a song is in the playlist twice.
            let found = decks
                .iter()
                .enumerate()
                .filter_map(|(at, deck)| match deck {
                    Deck::Slides(slides) => {
                        slides.live_slide(show, state).map(|slide| (at, slides.item == open, slide))
                    }
                    _ => None,
                })
                .max_by_key(|(_, is_open, _)| *is_open);
            if let Some((at, _, slide)) = found {
                let rect = sheet.slide(&places[at], &shapes[at].body, slide);
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
                    if place.strip > 0.0 {
                        arrangement_strip(
                            ui,
                            show,
                            slides,
                            state,
                            commands,
                            pos2(left, sheet.y(place.top + place.bar)),
                            across,
                        );
                    }
                    draw_slides(ui, show, state, slides, place, &shape.body, &sheet, thumb, commands);
                }
                Deck::Media { item, at, .. } => {
                    let tile = sheet.tile(place, 0);
                    let video =
                        at.is_some_and(|at| show.media().get(at).is_some_and(|m| crate::media::is_video(&m.path)));
                    let live = at.is_some() && *at == show.live_media();
                    let words = match at {
                        None => "Gone from the media strip",
                        Some(_) if live => "On screen",
                        Some(_) => "Click to show it",
                    };
                    let pictures: Vec<Option<TextureId>> =
                        at.iter().map(|at| thumb(ThumbOf::Media(*at)).map(|(texture, _)| texture)).collect();
                    let response = item_tile(ui, &sheet, tile, ("media tile", *item), &pictures, words, live, video);
                    if let Some(at) = at
                        && response.on_hover_text("Show it on the screens").clicked()
                    {
                        state.services.chosen = Some(*item);
                        commands.push(Command::ShowMedia(*at));
                    }
                }
                Deck::MediaPlaylist { item, id, shown, count, gone, .. } => {
                    let tile = sheet.tile(place, 0);
                    let live = state.media_playing == Some(*id);
                    let words = if *gone {
                        "Gone".to_owned()
                    } else if *count == 0 {
                        "Nothing in it yet".to_owned()
                    } else if live {
                        "Playing round and round".to_owned()
                    } else {
                        let items = if *count == 1 { "1 item".to_owned() } else { format!("{count} items") };
                        format!("{items} \u{b7} click to play them round")
                    };
                    let pictures: Vec<Option<TextureId>> =
                        shown.iter().map(|at| thumb(ThumbOf::Media(*at)).map(|(texture, _)| texture)).collect();
                    let response =
                        item_tile(ui, &sheet, tile, ("playlist tile", *item), &pictures, &words, live, false);
                    if !*gone
                        && *count > 0
                        && response
                            .on_hover_text("Play its pictures (8 seconds each) and videos in turn, round and round")
                            .clicked()
                    {
                        state.services.chosen = Some(*item);
                        state.media_request = Some(MediaRequest::PlayPlaylist(Some(*id)));
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
                    "Drop to add as a slide",
                    FontId::proportional(13.0),
                    TEXT_DIM,
                );
            }
            if let Some(dropped) = response.dnd_release_payload::<MediaDrag>() {
                state.media_request = Some(MediaRequest::AddAsSlide(dropped.0));
            }
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
    if matches!(body, Body::Line) {
        let at = pos2(sheet.left + INSET, sheet.y(place.body_top()) + INSET + LIST_ROW / 2.0);
        ui.painter().text(at, Align2::LEFT_CENTER, "No slides yet.", FontId::proportional(12.5), TEXT_FAINT);
        return;
    }
    // The table's headings.
    if view == SlideView::Table {
        let middle = sheet.y(place.body_top()) + TABLE_HEAD / 2.0;
        for (x, words) in
            [(sheet.columns.group.min, "Group"), (sheet.columns.words.min, "Words"), (sheet.columns.notes.min, "Notes")]
        {
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
    // Slides of this song being dragged to another place in it.
    let moving =
        deck.at.and_then(|at| egui::DragAndDrop::payload::<SlideDrag>(ui.ctx()).filter(|drag| drag.presentation == at));
    let mut moving_rects: Vec<Rect> = Vec::new();
    let list = view != SlideView::Grid;
    let mut last_group = None;
    for (index, (group, slide)) in song.slides().enumerate() {
        // A group's name is heavier on its first slide.
        let first = last_group != Some(group.id);
        last_group = Some(group.id);
        let rect = sheet.slide(place, body, index);
        if !sheet.in_view(rect) {
            continue;
        }
        let here = deck.slide_ref(index);
        let id = ui.scope_id().with(("slide", deck.item, deck.at, index));
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
                    // picked out with it when it is one of them; a line shows where.
                    if response.dragged() {
                        let mut picked: Vec<usize> =
                            state.picked_slides.iter().filter(|p| p.presentation == which).map(|p| p.slide).collect();
                        picked.sort_unstable();
                        let slides = if picked.contains(&index) { picked } else { vec![index] };
                        response.dnd_set_drag_payload(SlideDrag { presentation: which, slides });
                    }
                    let target = |drag: &SlideDrag| drag.presentation == which && !drag.slides.contains(&index);
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
                    deck.show_slide(show, state, index, commands);
                }
                if response.secondary_clicked() {
                    state.selected_slide = Some(here);
                }
            }
            // A playlist's Bible passage: a click puts the slide on screen, and that is all.
            None => {
                if response.clicked() {
                    state.selected_slide = None;
                    state.picked_slides.clear();
                    state.picking = Picking::Nothing;
                    deck.show_slide(show, state, index, commands);
                }
            }
        }
        let near = ui.rect_contains_pointer(rect);
        let is_live = live == Some(index);
        let picked = here.is_some_and(|here| state.selected_slide == Some(here) || state.picked_slides.contains(&here));
        // The slide that was on screen before it was cleared: Next goes on from it.
        let was_live = show.live().is_none() && here.is_some() && show.cursor() == here;
        let number = format!("{:02}", index + 1);
        let name = match slide.label.as_deref().filter(|label| !label.is_empty()) {
            Some(label) if group.name.is_empty() => label.to_owned(),
            Some(label) => format!("{} \u{b7} {label}", group.name),
            None => group.name.clone(),
        };
        let dot = if is_live { TALLY } else { dot_colour(group.color) };
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
        let has_menu = here.is_some();
        match view {
            SlideView::Grid => {
                let picture = Rect::from_min_size(rect.min, sheet.grid.picture);
                ui.painter().rect_filled(picture, ROUND, Color32::BLACK);
                let of = match (deck.bible, deck.at) {
                    (true, _) => ThumbOf::BibleSlide(slide),
                    (false, Some(at)) => ThumbOf::SongSlide(at, slide),
                    (false, None) => ThumbOf::Slide(slide),
                };
                match thumb(of) {
                    Some((texture, overflow)) => {
                        egui::Image::from_texture((texture, picture.size())).corner_radius(ROUND).paint_at(ui, picture);
                        if overflow {
                            // Its words don't fit their box: take care.
                            let mark = picture.right_top() + vec2(-14.0, 14.0);
                            ui.painter().circle_filled(mark, 9.0, crate::studio::CARE);
                            ui.painter().text(mark, Align2::CENTER_CENTER, "!", semibold(13.0), Color32::BLACK);
                        }
                    }
                    None => ui.ctx().request_repaint(),
                }
                // A slide switched off stays in the song but is stepped over: it is dimmed.
                if !slide.enabled {
                    ui.painter().rect_filled(picture, ROUND, PANEL.gamma_multiply(0.65));
                }
                if is_live {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, TALLY), StrokeKind::Outside);
                } else if picked {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, PICKED), StrokeKind::Outside);
                } else if was_live {
                    ui.painter().rect_stroke(picture, ROUND, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Outside);
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
                for (words, key) in slide_tags(slide).into_iter().rev() {
                    let galley = crate::studio::spaced(ui, &words, fixed_semibold(11.0), TEXT_DIM, 0.3);
                    let tag = Rect::from_min_max(
                        pos2(right - galley.size().x - 8.0, middle - 8.5),
                        pos2(right, middle + 8.5),
                    );
                    if tag.min.x < drawn.max.x + 40.0 {
                        break;
                    }
                    ui.painter().rect_filled(tag, 4.0, if key { crate::studio::BUTTON } else { CARD });
                    let ink = if key { TEXT } else { TEXT_DIM };
                    ui.painter().galley(pos2(tag.min.x + 4.0, middle - galley.size().y / 2.0), galley, ink);
                    right = tag.min.x - 4.0;
                }
                let (font, ink) = if is_live {
                    (if first { fixed_semibold(12.0) } else { fixed(12.0) }, TALLY)
                } else if first {
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
        // The pencil opens the slide in the editor; the "more" button opens what a
        // right-click does. Made after the slide itself, so a click on one of them is theirs
        // and never puts the slide on screen.
        if let (Some(place), Some(here)) = (pencil, here) {
            let strength = if near { 1.0 } else { 0.5 };
            let button = icon_button(ui, place, id.with("pencil"), Icon::Pencil, strength, false);
            if button.on_hover_text("Edit this slide").clicked() {
                state.editing = EditDraft::of(show, here);
            }
        }
        if let Some(here) = here {
            let button = more_button(ui, more, id.with("more"), near);
            egui::Popup::menu(&button).show(|ui| slide_menu(ui, show, state, song, here, slide, deck.made));
            response.context_menu(|ui| slide_menu(ui, show, state, song, here, slide, deck.made));
        }
    }
    // The slides being moved are dimmed where they are, and one of them follows the pointer,
    // with how many there are.
    if let (Some(drag), Some(at)) = (moving, deck.at) {
        for rect in &moving_rects {
            ui.painter().rect_filled(*rect, ROUND, PANEL.gamma_multiply(0.7));
        }
        let first = drag.slides.first().and_then(|at| song.slide(*at)).map(|(_, slide)| slide);
        let texture = first.and_then(|slide| {
            thumb(if deck.bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::SongSlide(at, slide) })
        });
        paint_drag_ghost(ui, texture.map(|(texture, _)| texture), drag.slides.len());
    }
}

/// The colour of a group's dot: the group's own, or a grey-blue when that is too dark to see
/// on the panel (a Bible passage's groups have no colour of their own).
fn dot_colour(colour: Rgba) -> Color32 {
    let light = 0.299 * f32::from(colour.r) + 0.587 * f32::from(colour.g) + 0.114 * f32::from(colour.b);
    if light < 60.0 { Color32::from_rgb(0x6b, 0x7f, 0x99) } else { color32(colour) }
}

/// The small tags in a slide's strip for what the slide does: its hot key (true: drawn as a
/// key), the time after which it goes on by itself, how many actions it has, its own
/// transition, and that it is switched off.
fn slide_tags(slide: &Slide) -> Vec<(String, bool)> {
    let mut tags = Vec::new();
    if let Some(key) = slide.hot_key {
        tags.push((key.to_string(), true));
    }
    if let Some(seconds) = slide.advance {
        tags.push((format!("{seconds:.0}S"), false));
    }
    if !slide.actions.is_empty() {
        tags.push((format!("{} ACT", slide.actions.len()), false));
    }
    if let Some(own) = slide.transition {
        tags.push((own.kind.name().to_uppercase(), false));
    }
    if !slide.enabled {
        tags.push(("OFF".to_owned(), false));
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
    video: bool,
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
    if video {
        let galley = crate::studio::spaced(ui, "\u{25b6} VIDEO", fixed(11.0), Color32::WHITE, 0.3);
        let tag = Rect::from_min_size(
            pos2(picture.min.x + 8.0, picture.max.y - 8.0 - galley.size().y - 4.0),
            galley.size() + vec2(10.0, 4.0),
        );
        ui.painter().rect_filled(tag, 3.0, Color32::from_black_alpha(178));
        ui.painter().galley(tag.min + vec2(5.0, 2.0), galley, Color32::WHITE);
    }
    if live {
        ui.painter().rect_stroke(picture, ROUND, Stroke::new(3.0, TALLY), StrokeKind::Outside);
    }
    let ink = if live { TALLY } else { TEXT };
    let strip = Rect::from_min_max(pos2(rect.min.x + 3.0, picture.max.y), pos2(rect.max.x - 3.0, rect.max.y));
    let galley = crate::studio::spaced(ui, &words.to_uppercase(), fixed_semibold(12.0), ink, 0.5);
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
    let words = ui.painter().layout_no_wrap("Choose a song\u{2026}".to_owned(), FontId::proportional(13.0), ink);
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
        state.services.fill(item, filled);
    }
    // Or a song chosen from a list.
    let response = response.on_hover_text("Fill the placeholder with a song from the library");
    let mut picked = None;
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| picked = crate::studio::song_picker(ui, show, state));
    if let Some(song) = picked
        && let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song))
    {
        let filled = Item::Song { key: key.clone(), title: found.title.clone() };
        if state.services.fill(item, filled) {
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
    let id = ui.scope_id().with(("bar", item, slides.and_then(|slides| slides.at)));
    let response = ui.interact(bar, id, Sense::click());
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
        Deck::Slides(slides) if slides.bible => (slides.deck.title.as_str(), Some("Bible"), false),
        Deck::Slides(slides) if slides.made => (slides.deck.title.as_str(), Some("Live captions"), false),
        Deck::Slides(slides) => (slides.deck.title.as_str(), slides.deck.loops.then_some("Loops"), false),
        Deck::Media { at, name, .. } => {
            let video = at.is_some_and(|at| show.media().get(at).is_some_and(|m| crate::media::is_video(&m.path)));
            let kind = match at {
                None => "Gone",
                Some(_) if video => "Video",
                Some(_) => "Picture",
            };
            (name.as_str(), Some(kind), at.is_none())
        }
        Deck::MediaPlaylist { name, gone, .. } => (name.as_str(), Some("Media playlist"), *gone),
        Deck::Placeholder { name, .. } => (name.as_str(), Some("Placeholder"), false),
        Deck::Note { title, kind, .. } => (title.as_str(), Some(*kind), true),
        Deck::Heading { name, .. } => (name.as_str(), None, false),
    };
    // The name; for the song chosen a double click turns it into a field to retype it.
    let font = semibold(14.0);
    let renaming = state.renaming_song.is_some() && song.is_some_and(|(at, _)| at == show.selected());
    if renaming && let (Some(typed), Some((at, song))) = (state.renaming_song.as_mut(), song) {
        let wide = painter.layout_no_wrap(typed.clone(), font.clone(), TEXT).size().x;
        let field = crate::studio::rename_field_at(
            ui,
            typed,
            font,
            TEXT,
            crate::studio::ACCENT,
            pos2(x, middle),
            (x + wide + TITLE_ROOM).min(right),
        );
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
        x += wide + TITLE_ROOM;
    } else {
        let ink = if faint { TEXT_FAINT } else { TEXT };
        let words = if title.is_empty() { "Untitled" } else { title };
        let drawn = painter.text(pos2(x, middle), Align2::LEFT_CENTER, words, font, ink);
        if let Some((at, song)) = song {
            let slides = song.slide_count();
            let count = if slides == 1 { "1 slide".to_owned() } else { format!("{slides} slides") };
            let name = ui
                .interact(drawn.intersect(bar), id.with("name"), Sense::click())
                .on_hover_text(format!("{count} \u{b7} double-click to rename"));
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
        }
        x = drawn.max.x;
    }
    if let Some(kind) = kind {
        x = pill(ui, &painter, pos2(x + 10.0, middle), kind).max.x;
    }
    // What the "more" button and a right-click on the bar open.
    let mut menu = |ui: &mut egui::Ui, state: &mut HomeState| {
        if let Some((at, _)) = song {
            if ui.button("Add Slide").on_hover_text("Add a slide laid out like the last one").clicked() {
                state.add_slide = Some(at);
                ui.close();
            }
            ui.separator();
            crate::studio::song_menu(ui, show, state, commands, at, asks);
        }
        if let Some(item) = item {
            if song.is_some() {
                ui.separator();
            }
            crate::studio::item_moves(ui, show, state, item, ask);
        }
    };
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
            .on_hover_text("Details: the song's writer, publisher and CCLI number");
        egui::Popup::from_toggle_button_response(&info)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(320.0);
                let mut details = song.copyright.clone();
                egui::Grid::new("song info").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    for (label, value) in [
                        ("Artist", &mut details.artist),
                        ("Author", &mut details.author),
                        ("Publisher", &mut details.publisher),
                        ("Year", &mut details.year),
                        ("CCLI Song #", &mut details.ccli_song),
                    ] {
                        ui.label(egui::RichText::new(label).color(DIM));
                        ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Settings, General says when the copyright line is shown.")
                        .size(11.5)
                        .color(DIM),
                );
                if details != song.copyright {
                    state.copyright_request = Some((at, details));
                }
            });
        let words = icon_button(ui, place(), id.with("words"), Icon::Reflow, strength, false)
            .on_hover_text("Words as text: edit the song's words as text (Control-R)");
        if words.clicked() {
            state.reflow = Some((at, sp_model::to_song_text(song)));
        }
        let arranged = song.active_arrangement().is_some();
        let arrangement = icon_button(ui, place(), id.with("arrangement"), Icon::Arrangement, strength, arranged)
            .on_hover_text("Arrangement: play the song's groups in another order");
        arrangement_menu(at, song, state, &arrangement);
        let notes = icon_button(ui, place(), id.with("notes"), Icon::Notes, strength, state.show_notes)
            .on_hover_text("Notes: show the notes of the slide on screen");
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
}

/// The Arrangement button beside a song's title: play the song as written or in one of its
/// arrangements, make a new one, rename, duplicate or delete the one in use.
fn arrangement_menu(index: usize, song: &Presentation, state: &mut HomeState, button: &egui::Response) {
    let active = song.active_arrangement();
    let named = |arrangement: &sp_model::Arrangement| {
        if arrangement.name.trim().is_empty() { "Untitled".to_owned() } else { arrangement.name.clone() }
    };
    let mut arrangements = song.arrangements.clone();
    let mut chosen = song.arrangement;
    egui::Popup::from_toggle_button_response(button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(220.0);
            if ui.add(egui::Button::selectable(active.is_none(), "As Written")).clicked() {
                chosen = None;
            }
            for arrangement in &song.arrangements {
                let on = active.is_some_and(|have| have.id == arrangement.id);
                if ui.add(egui::Button::selectable(on, named(arrangement))).clicked() {
                    chosen = Some(arrangement.id);
                }
            }
            ui.separator();
            if ui.button("New Arrangement").on_hover_text("Starts with the groups as written").clicked() {
                let made = song.new_arrangement();
                chosen = Some(made.id);
                arrangements.push(made);
            }
            if let Some(at) = active.and_then(|have| arrangements.iter().position(|a| a.id == have.id)) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Name").color(DIM));
                    ui.add(egui::TextEdit::singleline(&mut arrangements[at].name).desired_width(150.0));
                });
                ui.horizontal(|ui| {
                    if ui.button("Duplicate").clicked() {
                        let copy = sp_model::Arrangement {
                            id: sp_model::Uuid::new_v4(),
                            name: format!("{} copy", arrangements[at].name),
                            groups: arrangements[at].groups.clone(),
                        };
                        chosen = Some(copy.id);
                        arrangements.push(copy);
                    }
                    if ui.button("Delete").clicked() {
                        arrangements.remove(at);
                        chosen = None;
                    }
                });
            }
        });
    if chosen != song.arrangement || arrangements != song.arrangements {
        state.arrangement_request = Some((index, arrangements, chosen));
    }
}

/// Where the parts of a song's arrangement strip go, from the strip's top left corner: the
/// arrangement's name, a pill for each group in the order played, and the button that adds
/// one. They wrap to more rows when there are many.
struct ArrangementLayout {
    name: Rect,
    chips: Vec<ChipPlace>,
    add: Rect,
    height: f32,
}

struct ChipPlace {
    rect: Rect,
    /// Its place in the arrangement's list of groups, and the group's place in the song.
    at: usize,
    group: usize,
    /// The steps of the play order it covers: the first, and how many.
    first: usize,
    len: usize,
}

const CHIP: f32 = 26.0;

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
    let label = if arrangement.name.trim().is_empty() { "Untitled" } else { arrangement.name.as_str() };
    let name = put(wide(&format!("{label} \u{25be}"), fixed_semibold(11.0)) + 26.0);
    let mut chips = Vec::new();
    let mut step = 0;
    for (at, id) in arrangement.groups.iter().enumerate() {
        let Some(group) = song.groups.iter().position(|group| group.id == *id) else { continue };
        let found = &song.groups[group];
        let rect = put(wide(&found.name, FontId::proportional(12.0)) + 33.0);
        chips.push(ChipPlace { rect, at, group, first: step, len: found.slides.len() });
        step += found.slides.len();
    }
    let add = put(CHIP);
    Some(ArrangementLayout { name, chips, add, height: y + CHIP + 12.0 })
}

/// The arrangement a song is played in, under its bar: its name (which opens the arrangement
/// menu), then its groups in the order they are played. The one on screen is orange; a click
/// goes to it; its menu moves or removes it; + adds a group at the end.
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
    let label = if arrangement.name.trim().is_empty() { "Untitled" } else { arrangement.name.as_str() };
    crate::studio::paint_spaced(
        ui,
        pill_at.center(),
        Align2::CENTER_CENTER,
        &format!("{label} \u{25be}"),
        fixed_semibold(11.0),
        TEXT_DIM,
        0.5,
    );
    arrangement_menu(index, song, state, &button.on_hover_text("Choose, make or change an arrangement"));
    let mut groups = arrangement.groups.clone();
    let mut changed = false;
    let count = layout.chips.len();
    for (chip, place) in layout.chips.iter().enumerate() {
        let rect = place.rect.translate(origin.to_vec2());
        let group = &song.groups[place.group];
        let on = live.is_some_and(|step| step >= place.first && step < place.first + place.len);
        let response = ui.interact(rect, id.with(("chip", chip)), Sense::click());
        let fill = if on {
            TALLY
        } else if response.hovered() {
            crate::studio::BUTTON
        } else {
            CARD
        };
        ui.painter().rect_filled(rect, CHIP / 2.0, fill);
        let (dot, ink) = if on { (Color32::WHITE, Color32::WHITE) } else { (dot_colour(group.color), TEXT_DIM) };
        ui.painter().circle_filled(pos2(rect.min.x + 10.0 + 3.5, rect.center().y), 3.5, dot);
        ui.painter().text(
            pos2(rect.min.x + 23.0, rect.center().y),
            Align2::LEFT_CENTER,
            group.name.as_str(),
            FontId::proportional(12.0),
            ink,
        );
        let response = response.on_hover_text("Click to show this part; right-click to move or remove it");
        if response.clicked() {
            commands.push(Command::GoToStep { presentation: index, step: place.first });
            if deck.item.is_some() {
                state.services.chosen = deck.item;
            }
        }
        response.context_menu(|ui| {
            if ui.add_enabled(chip > 0, egui::Button::new("Move Left")).clicked() {
                groups.swap(place.at, layout.chips[chip - 1].at);
                changed = true;
                ui.close();
            }
            if ui.add_enabled(chip + 1 < count, egui::Button::new("Move Right")).clicked() {
                groups.swap(place.at, layout.chips[chip + 1].at);
                changed = true;
                ui.close();
            }
            if ui.button("Repeat").on_hover_text("Play this part again straight after").clicked() {
                groups.insert(place.at + 1, group.id);
                changed = true;
                ui.close();
            }
            ui.separator();
            if ui.button("Remove").clicked() {
                groups.remove(place.at);
                changed = true;
                ui.close();
            }
        });
    }
    let add = icon_button(ui, layout.add.translate(origin.to_vec2()), id.with("add"), Icon::Plus, 1.0, false)
        .on_hover_text("Add a group to the end");
    egui::Popup::menu(&add).show(|ui| {
        for group in &song.groups {
            if ui.button(group.name.as_str()).clicked() {
                groups.push(group.id);
                changed = true;
            }
        }
    });
    if changed {
        let mut arrangements = song.arrangements.clone();
        if let Some(edited) = arrangements.iter_mut().find(|have| have.id == arrangement.id) {
            edited.groups = groups;
        }
        state.arrangement_request = Some((index, arrangements, song.arrangement));
    }
}

/// The middle of the Show view: the playlist in front as one scroll, or the song chosen in a
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
        // The bar under the slides: Add Slide on the left; the transition, the slide view and
        // the slide size on the right.
        egui::Panel::bottom("slide bar").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let from_bible = state.app_made(show.selected());
                if !from_bible {
                    let add = ui.button("+  Add Slide").on_hover_text("Add a slide laid out like the last one");
                    if add.clicked() {
                        state.add_slide = Some(show.selected());
                    }
                    // A media item dropped on it is added as a slide.
                    if let Some(dropped) = add.dnd_release_payload::<MediaDrag>() {
                        state.media_request = Some(MediaRequest::AddAsSlide(dropped.0));
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| crate::studio::view_controls(ui, state));
            });
        });
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
                    None => {
                        ("No notes for this slide. Add them in the editor with Slide Notes.".to_owned(), TEXT_FAINT)
                    }
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
            format!("{} is empty", state.services.front().name),
            "A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the \
             order they come.",
        )
    } else if show.presentations().is_empty() {
        ("No songs yet".to_owned(), "Make a song, or import some.")
    } else {
        ("No song is open".to_owned(), "Pick a song on the left to see its slides.")
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
                if ui.button("Add a Bible passage").clicked() {
                    state.bible_open = true;
                    state.bible.focus_lookup = true;
                }
                if ui.button("Add a header").clicked() {
                    state.services.add(Item::Header { name: "Header".to_owned() });
                    let at = state.services.front().items.len() - 1;
                    state.renaming_header = Some((at, "Header".to_owned()));
                    state.rename_started = true;
                }
            } else if ui.button("New song").clicked() {
                state.new_song = true;
            }
            if ui.button("Import\u{2026}").clicked() {
                state.menu_asked = Some(Action::ImportSongs);
            }
        });
        if playlist {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Songs are added with Add to playlist, at the foot of the left column.")
                    .size(12.0)
                    .color(TEXT_FAINT),
            );
        }
    });
}

/// A slide's menu, from its "more" button or a right-click. Editing is here, and on the
/// pencil, so that reaching for it never puts a slide on screen by accident.
fn slide_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    presentation: &Presentation,
    here: SlideRef,
    slide: &Slide,
    made: bool,
) {
    let (which, index) = (here.presentation, here.slide);
    if ui.button("Edit Slide").clicked() {
        state.editing = EditDraft::of(show, here);
        ui.close();
    }
    if made {
        return;
    }
    ui.separator();
    // Cut and Copy take the slides picked out with this one, or this one.
    let mut these: Vec<usize> = state
        .picked_slides
        .iter()
        .filter(|p| p.presentation == which && state.picked_slides.contains(&here))
        .map(|p| p.slide)
        .collect();
    these.sort_unstable();
    if these.is_empty() {
        these.push(index);
    }
    if ui.button("Cut").clicked() {
        copy_slides(ui.ctx(), state, presentation, &these);
        state.slide_request = Some((here, SlideRequest::DeleteMany(these.clone())));
        ui.close();
    }
    if ui.button("Copy").clicked() {
        copy_slides(ui.ctx(), state, presentation, &these);
        ui.close();
    }
    let clip = state.slide_clip.clone();
    if ui.add_enabled(!clip.is_empty(), egui::Button::new("Paste")).clicked() {
        state.slide_request = Some((here, SlideRequest::PasteAfter(clip)));
        ui.close();
    }
    if ui.button("Delete…").clicked() {
        state.confirm_delete_slides =
            if state.picked_slides.contains(&here) { state.picked_slides.clone() } else { vec![here] };
        ui.close();
    }
    ui.separator();
    // Where a group starts: this slide and the rest of its group take the name.
    ui.menu_button("Group", |ui| {
        for name in GROUP_NAMES {
            if ui.button(name).clicked() {
                state.slide_request = Some((here, SlideRequest::Group(name.to_owned())));
                ui.close();
            }
        }
    });
    // The media the slide puts behind its words when it goes on screen.
    ui.menu_button("Media", |ui| {
        let own = media_of(slide);
        if ui.add(egui::Button::selectable(own.is_none(), "No Media")).clicked() {
            state.pending_edit = Some((here, slide_with_media(slide, None)));
            ui.close();
        }
        ui.separator();
        if show.media().is_empty() {
            ui.label(egui::RichText::new("The media bin is empty.").color(DIM));
        }
        for item in show.media() {
            if ui.add(egui::Button::selectable(own == Some(item.id), item.name.as_str())).clicked() {
                state.pending_edit = Some((here, slide_with_media(slide, Some(item.id))));
                ui.close();
            }
        }
    });
    ui.menu_button("Transition", |ui| {
        if ui.add(egui::Button::selectable(slide.transition.is_none(), "As All Slides")).clicked() {
            state.pending_edit = Some((here, Slide { transition: None, ..slide.clone() }));
            ui.close();
        }
        ui.separator();
        for kind in sp_model::TransitionKind::ALL {
            let on = slide.transition.is_some_and(|own| own.kind == kind);
            if ui.add(egui::Button::selectable(on, kind.name())).clicked() {
                let own = sp_model::Transition::of(kind);
                state.pending_edit = Some((here, Slide { transition: Some(own), ..slide.clone() }));
                ui.close();
            }
        }
    });
    // What the slide does when it goes on screen.
    ui.menu_button("Actions", |ui| {
        for (at, action) in slide.actions.iter().enumerate() {
            let words = crate::live_tabs::describe_action(show, state, action);
            if ui.button(format!("Remove: {words}")).clicked() {
                let mut kept = slide.clone();
                kept.actions.remove(at);
                state.pending_edit = Some((here, kept));
                ui.close();
            }
        }
        if !slide.actions.is_empty() {
            ui.separator();
        }
        ui.menu_button("Add Action", |ui| {
            if let Some(action) = crate::live_tabs::add_action_menu(ui, show, state) {
                let mut more = slide.clone();
                more.actions.push(action);
                state.pending_edit = Some((here, more));
                ui.close();
            }
        });
    });
    // Going on to the next slide by itself, after a time.
    ui.menu_button("Go to Next", |ui| {
        if ui.add(egui::Button::selectable(slide.advance.is_none(), "Off")).clicked() {
            state.pending_edit = Some((here, Slide { advance: None, ..slide.clone() }));
            ui.close();
        }
        ui.separator();
        for seconds in [3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0] {
            let on = slide.advance == Some(seconds);
            let label = format!("After {seconds:.0} Seconds");
            if ui.add(egui::Button::selectable(on, label)).clicked() {
                state.pending_edit = Some((here, Slide { advance: Some(seconds), ..slide.clone() }));
                ui.close();
            }
        }
    });
    // A key that puts this slide on screen in the Show view.
    ui.menu_button("Hot Key", |ui| {
        if ui.add(egui::Button::selectable(slide.hot_key.is_none(), "None")).clicked() {
            state.pending_edit = Some((here, Slide { hot_key: None, ..slide.clone() }));
            ui.close();
        }
        ui.separator();
        egui::Grid::new("hot keys").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
            for (index, key) in ('A'..='Z').chain('0'..='9').enumerate() {
                let on = slide.hot_key == Some(key);
                let button = egui::Button::selectable(on, key.to_string()).min_size(vec2(26.0, 22.0));
                if ui.add(button).clicked() {
                    state.pending_edit = Some((here, Slide { hot_key: Some(key), ..slide.clone() }));
                    ui.close();
                }
                if index % 6 == 5 {
                    ui.end_row();
                }
            }
        });
    });
    // The look of the slide's text, to give another slide.
    if ui.button("Copy Text Style").clicked() {
        state.inspector.copied_style = slide.objects.iter().find_map(|object| object.text_style().cloned());
        ui.close();
    }
    if ui
        .add_enabled(state.inspector.copied_style.is_some(), egui::Button::new("Paste Text Style"))
        .on_hover_text("Give every text box on this slide the look that was copied")
        .clicked()
        && let Some(copied) = state.inspector.copied_style.clone()
    {
        let mut styled = slide.clone();
        for object in &mut styled.objects {
            if let Some(style) = object.text_style_mut() {
                *style = copied.clone();
            }
        }
        state.pending_edit = Some((here, styled));
        ui.close();
    }
    ui.menu_button("Label", |ui| {
        if ui.add(egui::Button::selectable(slide.label.is_none(), "None")).clicked() {
            state.slide_request = Some((here, SlideRequest::Label(None)));
            ui.close();
        }
        ui.separator();
        for name in SLIDE_LABELS {
            let on = slide.label.as_deref() == Some(name);
            if ui.add(egui::Button::selectable(on, name)).clicked() {
                state.slide_request = Some((here, SlideRequest::Label(Some(name.to_owned()))));
                ui.close();
            }
        }
    });
    // A disabled slide stays in the song but is stepped over and never shown.
    let label = if slide.enabled { "Disable Slide" } else { "Enable Slide" };
    if ui.button(label).clicked() {
        state.pending_edit = Some((here, Slide { enabled: !slide.enabled, ..slide.clone() }));
        ui.close();
    }
    // Which of the theme's templates the slide is drawn with. Automatic lets the theme pick
    // the one that suits what is on the slide.
    if !state.song_templates.is_empty() {
        ui.menu_button("Template", |ui| {
            let current = slide.template.filter(|id| state.song_templates.iter().any(|(t, _)| t == id));
            if ui.add(egui::Button::selectable(current.is_none(), "Automatic")).clicked() {
                state.pending_template = Some((here, None));
                ui.close();
            }
            ui.separator();
            for (id, name) in &state.song_templates {
                if ui.add(egui::Button::selectable(current == Some(*id), name.as_str())).clicked() {
                    state.pending_template = Some((here, Some(*id)));
                    ui.close();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, RawInput};
    use sp_model::parse_song_text;

    use super::*;

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
            state.services.add(Item::Song { key: format!("title:{title}"), title: (*title).to_owned() });
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
        assert!(!state.bible_open, "the Show view stays");
        // As the app does next: the passage goes into its place and its slide on screen.
        show.replace_presentation(slot, Arc::new(looked_up));
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
        };
        broken.show_slide(&show, &mut state, 0, &mut Vec::new());
        assert!(state.bible.pending.is_none(), "{:?}", state.bible.pending.as_ref().map(|(_, deck)| &deck.title));
        assert!(state.bible.go_live.is_none());
    }

    #[test]
    fn a_group_of_an_arrangement_goes_to_its_own_song_not_the_one_chosen() {
        let (mut show, mut state) = playlist(&[("Alpha", 2), ("Beta", 2)]);
        // Beta is played in an arrangement; Alpha is the song chosen.
        let mut beta = (*show.presentations()[1]).clone();
        let made = beta.new_arrangement();
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
        let at = pos2(tile.min.x - INSET + chip.center().x, strip_top + chip.center().y);
        let asked = click(&ctx, &show, &mut state, at, &mut time);
        assert!(asked.contains(&Command::GoToStep { presentation: 1, step: 0 }), "{asked:?}");
        assert!(!asked.iter().any(|c| matches!(c, Command::GoToStep { presentation: 0, .. } | Command::GoTo(_))));
    }
}
