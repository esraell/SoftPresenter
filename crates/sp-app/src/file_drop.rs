//! Files dragged onto the window from the Finder (or Explorer): what they are, the place under
//! the pointer they would land in, the ink outline that shows it, and where each goes when they
//! are dropped.
//!
//! Song files (every song format, song packs and bundles, folders of them) always go to the
//! songs, never into the media bin. Dropped on a playlist (a row of the chooser, or the open
//! playlist, after the item under the pointer) they come into the library and go into that
//! playlist; on a library (a row of the chooser, or the song list) into that library; anywhere
//! else into the library shown. Pictures and videos go into the media bin, and into a playlist
//! they are dropped on. The media strip takes pictures and videos only: songs over it go to the
//! songs as if dropped anywhere else, and the strip doesn't light up for them. While song files
//! are over the window the chooser shows above the list, as it does while songs are carried in
//! the window, so any playlist or library can be aimed at.
//!
//! The window library (winit 0.30) says when files come over the window and when they are
//! dropped, but not where the pointer is, and sends no movement in between, on any system;
//! egui's pointer, fed by it, stays where it was or is gone. On macOS the pointer's place on the
//! screen is read from the system each frame while files are over the window (CoreGraphics'
//! event location, a safe call) and turned into the window's points. Elsewhere there is no live
//! position, so nothing counts as under the pointer: songs go to the library shown and pictures
//! to the media bin, as before, and only those places light up. The system's own drop cursor
//! can't change from place to place either: the window library accepts the drag as a whole.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Stroke, StrokeKind, pos2};
use sp_model::Uuid;

use crate::home::HomeState;

/// What a file dragged onto the window is, for where it can land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A song file of any kind, a song bundle, a songbook, or a folder of songs.
    Song,
    /// A song pack, whose songs go into a library (it may bring its own playlist), never into a
    /// playlist dropped on.
    Pack,
    /// A picture or video.
    Media,
    /// A presentation, PDF or folder of pictures, which becomes slides in the library shown.
    Slides,
    /// A Bible, or anything else.
    Other,
}

/// What a dropped file is, as the drop decides it (song packs and Bibles first, then slides,
/// then songs, then pictures and videos).
pub fn kind_of(path: &Path) -> Kind {
    if crate::pack::is_pack(path) {
        return Kind::Pack;
    }
    if path.extension().is_some_and(|ext| ext == "spbible" || ext == "pcbible") {
        return Kind::Other;
    }
    if crate::deck::is_pdf(path)
        || crate::deck::is_presentation_file(path)
        || (path.is_dir() && crate::deck::pictures_in(path).is_some())
    {
        return Kind::Slides;
    }
    if crate::load::is_importable(path)
        || crate::load::is_song_file(path)
        || (path.is_dir() && !crate::load::files_to_import(&[path.to_owned()]).is_empty())
    {
        return Kind::Song;
    }
    if crate::media::is_media(path) {
        return Kind::Media;
    }
    Kind::Other
}

/// A place files can be dropped on.
#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    /// A playlist, by its place among the playlists; in the open playlist, the item what is
    /// dropped goes after (`None`: at the end).
    Playlist { index: usize, after: Option<usize> },
    /// A song library, or All Songs (`None`).
    Library(Option<Uuid>),
    /// The media strip.
    MediaStrip,
}

impl DropTarget {
    /// Whether files of this kind can land here.
    pub fn takes(&self, kind: Kind) -> bool {
        match self {
            DropTarget::Playlist { .. } => matches!(kind, Kind::Song | Kind::Media),
            DropTarget::Library(_) => matches!(kind, Kind::Song | Kind::Pack),
            DropTarget::MediaStrip => kind == Kind::Media,
        }
    }
}

/// A place drawn this frame that files could be dropped on.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// Where the pointer must be (what is in view of it).
    pub hit: Rect,
    /// What lights up when it is under the pointer.
    pub outline: Rect,
    /// A line drawn where what is dropped goes in, across the outline, at this height.
    pub line: Option<f32>,
    pub target: DropTarget,
    /// The song list: where songs go when they are dropped on no particular place.
    pub songs_land: bool,
}

impl Place {
    /// A place that lights up itself.
    pub fn onto(rect: Rect, target: DropTarget) -> Place {
        Place { hit: rect, outline: rect, line: None, target, songs_land: false }
    }

    /// Lights up `outline` (the list a row is in, say) rather than the place itself.
    pub fn outlining(self, outline: Rect) -> Place {
        Place { outline, ..self }
    }

    /// With a line where what is dropped goes in.
    pub fn line_at(self, y: f32) -> Place {
        Place { line: Some(y), ..self }
    }

    /// The song list, where songs land when dropped on no particular place.
    pub fn where_songs_land(self) -> Place {
        Place { songs_land: true, ..self }
    }
}

/// Files being dragged over the window.
#[derive(Debug, Default)]
pub struct FileDrag {
    /// Each file, and what it is.
    pub files: Vec<(PathBuf, Kind)>,
    /// Where the pointer is, in the window's points, when the system says during the drag.
    pub pointer: Option<Pos2>,
    /// The places a drop could land, as this frame draws them.
    pub places: Vec<Place>,
    /// The place under the pointer, as the last frame drew it.
    pub hit: Option<Place>,
    /// When the files came over the window.
    pub since: Option<Instant>,
}

/// How long the window keeps being drawn again to follow the pointer during one drag. A drag
/// is over in seconds; this keeps a drag the system never says has ended from drawing for ever.
const FOLLOW_FOR: Duration = Duration::from_secs(60);
/// How often the window is drawn again to follow the pointer.
const FOLLOW_EVERY: Duration = Duration::from_millis(33);

impl FileDrag {
    /// Notes a file come over the window.
    pub fn add(&mut self, path: PathBuf) {
        self.since.get_or_insert_with(Instant::now);
        if !self.files.iter().any(|(have, _)| *have == path) {
            let kind = kind_of(&path);
            self.files.push((path, kind));
        }
    }

    pub fn has(&self, kind: Kind) -> bool {
        self.files.iter().any(|(_, have)| *have == kind)
    }

    /// Whether songs are coming: song files or song packs.
    pub fn has_songs(&self) -> bool {
        self.has(Kind::Song) || self.has(Kind::Pack)
    }

    fn kinds(&self) -> Vec<Kind> {
        self.files.iter().map(|(_, kind)| *kind).collect()
    }
}

/// Where `files` dropped with the pointer at `pointer` land, among `places`: `None` when the
/// pointer is on no place that takes any of them, or isn't known.
pub fn target_at(places: &[Place], files: &[(PathBuf, Kind)], pointer: Option<Pos2>) -> Option<DropTarget> {
    let kinds: Vec<Kind> = files.iter().map(|(_, kind)| *kind).collect();
    pointer.and_then(|at| place_at(places, at, &kinds)).map(|place| place.target.clone())
}

/// The place under `pointer` that takes some of `kinds`: of several, the smallest (a row
/// before the list it is in).
pub fn place_at<'a>(places: &'a [Place], pointer: Pos2, kinds: &[Kind]) -> Option<&'a Place> {
    places
        .iter()
        .filter(|place| place.hit.contains(pointer) && kinds.iter().any(|kind| place.target.takes(*kind)))
        .min_by(|a, b| a.hit.area().total_cmp(&b.hit.area()))
}

/// Starts a frame of the window: the place under the pointer, from the places the last frame
/// drew, and an empty list for this frame's.
pub fn begin_frame(ctx: &egui::Context, state: &mut HomeState) {
    let drag = &mut state.file_drag;
    if !state.drop_hover {
        drag.places.clear();
        drag.hit = None;
        return;
    }
    let kinds = drag.kinds();
    drag.hit = drag.pointer.and_then(|at| place_at(&drag.places, at, &kinds).cloned());
    drag.places.clear();
    // Nothing else changes under the drag; the window is drawn again only to follow the pointer,
    // and only while the system says where it is.
    if drag.pointer.is_some() && drag.since.is_none_or(|since| since.elapsed() < FOLLOW_FOR) {
        ctx.request_repaint_after(FOLLOW_EVERY);
    }
}

/// Notes a place drawn this frame that files could be dropped on, as far as it is in view.
pub fn offer(ui: &egui::Ui, state: &mut HomeState, mut place: Place) {
    if !state.drop_hover {
        return;
    }
    let view = ui.clip_rect();
    place.hit = place.hit.intersect(view);
    place.outline = place.outline.intersect(view);
    if place.hit.is_positive() {
        state.file_drag.places.push(place);
    }
}

/// Notes a library's row in the chooser (`None` for All Songs). While a playlist is listed, the
/// row of the library shown is where songs land when dropped on no particular place (with a
/// library listed, its song list is).
pub fn offer_library_row(ui: &egui::Ui, state: &mut HomeState, row: Rect, library: Option<Uuid>) {
    let place = Place::onto(row, DropTarget::Library(library));
    let lands = state.playlist_open && state.library_shown == library;
    offer(ui, state, if lands { place.where_songs_land() } else { place });
}

/// Whether song files are coming and the system says where the pointer is, so the chooser
/// shows to aim them at a playlist or library.
pub fn wants_chooser(state: &HomeState) -> bool {
    state.drop_hover && state.file_drag.has_songs() && state.file_drag.pointer.is_some()
}

/// Whether pictures or videos are coming, so the media strip opens.
pub fn media_coming(state: &HomeState) -> bool {
    state.drop_hover && state.file_drag.has(Kind::Media)
}

/// Whether the media strip lights up: pictures or videos are coming and would land in it (it
/// is under the pointer, or nothing under the pointer takes them).
pub fn strip_lit(state: &HomeState) -> bool {
    media_coming(state)
        && state
            .file_drag
            .hit
            .as_ref()
            .is_none_or(|place| place.target == DropTarget::MediaStrip || !place.target.takes(Kind::Media))
}

/// Draws the ink outline on the place under the pointer and, for songs with no particular
/// place under it, on the song list they land in.
pub fn paint(ctx: &egui::Context, state: &HomeState) {
    if !state.drop_hover {
        return;
    }
    let drag = &state.file_drag;
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("file drop")));
    let ink = Stroke::new(2.0, crate::studio::TEXT);
    let light = |place: &Place| {
        painter.rect_stroke(place.outline, crate::studio::RADIUS, ink, StrokeKind::Inside);
        if let Some(y) = place.line {
            let (left, right) = (place.outline.min.x + 4.0, place.outline.max.x - 4.0);
            painter.line_segment([pos2(left, y), pos2(right, y)], ink);
        }
    };
    let hit = drag.hit.as_ref().filter(|place| place.target != DropTarget::MediaStrip);
    if let Some(place) = hit {
        light(place);
    }
    let songs_placed = drag.hit.as_ref().is_some_and(|place| {
        drag.files.iter().any(|(_, kind)| matches!(kind, Kind::Song | Kind::Pack) && place.target.takes(*kind))
    });
    if drag.has_songs() && !songs_placed {
        drag.places.iter().filter(|place| place.songs_land).for_each(light);
    }
}

/// Where dropped files go: songs (to the playlist or library dropped on, if any), pictures and
/// videos (and the playlist dropped on), slides, and the rest.
#[derive(Debug, Default, PartialEq)]
pub struct Route {
    pub songs: Vec<PathBuf>,
    pub songs_to: Option<DropTarget>,
    pub media: Vec<PathBuf>,
    /// A playlist the pictures and videos go into too, and the item they go after.
    pub media_playlist: Option<(usize, Option<usize>)>,
    pub slides: Vec<PathBuf>,
    pub other: Vec<PathBuf>,
}

/// Sorts dropped files by what they are, each to the place dropped on where it takes them.
pub fn route(files: &[(PathBuf, Kind)], target: Option<&DropTarget>) -> Route {
    let mut route = Route::default();
    for (path, kind) in files {
        match kind {
            Kind::Song | Kind::Pack => route.songs.push(path.clone()),
            Kind::Media => route.media.push(path.clone()),
            Kind::Slides => route.slides.push(path.clone()),
            Kind::Other => route.other.push(path.clone()),
        }
    }
    let songs: Vec<Kind> =
        files.iter().map(|(_, kind)| *kind).filter(|kind| matches!(kind, Kind::Song | Kind::Pack)).collect();
    route.songs_to = target.filter(|target| songs.iter().any(|kind| target.takes(*kind))).cloned();
    if let Some(DropTarget::Playlist { index, after }) = target {
        route.media_playlist = Some((*index, *after));
    }
    route
}

/// The pointer's place in the window's points, from its place on the screen (in points from
/// the top left of the main display, as macOS gives it), the window's inner top left corner
/// (in pixels in that same space, as winit gives it), the window's scale factor, and egui's
/// pixels per point (the scale factor times any zoom).
pub fn window_point(screen: (f64, f64), inner: (i32, i32), scale: f64, pixels_per_point: f32) -> Pos2 {
    let per_point = f64::from(pixels_per_point.max(0.01));
    let x = (screen.0 * scale - f64::from(inner.0)) / per_point;
    let y = (screen.1 * scale - f64::from(inner.1)) / per_point;
    pos2(x as f32, y as f32)
}

/// Where the pointer is on the screen, in points from the top left of the main display, read
/// from the system: CoreGraphics gives it at any time, during a drag from another program too.
#[cfg(target_os = "macos")]
pub fn pointer_on_screen() -> Option<(f64, f64)> {
    use objc2_core_graphics::CGEvent;
    let event = CGEvent::new(None)?;
    let at = CGEvent::location(Some(&event));
    (at.x.is_finite() && at.y.is_finite()).then_some((at.x, at.y))
}

/// Other systems don't say where the pointer is during a drag (see the module's notes).
#[cfg(not(target_os = "macos"))]
pub fn pointer_on_screen() -> Option<(f64, f64)> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::vec2;
    use sp_engine::Show;

    use super::*;
    use crate::home::{Preview, home_ui};

    #[test]
    fn a_dropped_song_file_never_becomes_media() {
        for song in [
            "Harbour Lights.proBundle",
            "Harbour Lights.pro",
            "Lantern.txt",
            "Lantern.cho",
            "Lantern.docx",
            "Lantern.sppres",
            "Hymns.sps",
        ] {
            assert_eq!(kind_of(Path::new(song)), Kind::Song, "{song}");
        }
        // A song pack goes to the songs too, into a library: not into a playlist dropped on.
        assert_eq!(kind_of(Path::new("Sunday.sppack")), Kind::Pack);
        let pack = vec![(PathBuf::from("Sunday.sppack"), Kind::Pack)];
        assert_eq!(route(&pack, Some(&DropTarget::MediaStrip)).songs, vec![PathBuf::from("Sunday.sppack")]);
        assert_eq!(route(&pack, Some(&DropTarget::Playlist { index: 0, after: None })).songs_to, None);
        assert!(DropTarget::Library(None).takes(Kind::Pack));
        for media in ["Sky.png", "Waves.mov", "Clip.mp4"] {
            assert_eq!(kind_of(Path::new(media)), Kind::Media, "{media}");
        }
        assert_eq!(kind_of(Path::new("Talk.pdf")), Kind::Slides);
        assert_eq!(kind_of(Path::new("KJV.spbible")), Kind::Other);
        // Dropped on the media strip, a song still goes to the songs (into the library shown),
        // and only the picture goes into the bin.
        let files =
            vec![(PathBuf::from("Harbour Lights.proBundle"), Kind::Song), (PathBuf::from("Sky.png"), Kind::Media)];
        let strip = route(&files, Some(&DropTarget::MediaStrip));
        assert_eq!(strip.songs, vec![PathBuf::from("Harbour Lights.proBundle")]);
        assert_eq!((strip.songs_to, strip.media_playlist), (None, None));
        assert_eq!(strip.media, vec![PathBuf::from("Sky.png")]);
        // On a playlist, both go into it; on a library, the song goes into it and the picture
        // into the bin.
        let playlist = DropTarget::Playlist { index: 1, after: Some(2) };
        let into = route(&files, Some(&playlist));
        assert_eq!((into.songs_to, into.media_playlist), (Some(playlist), Some((1, Some(2)))));
        let id = Uuid::new_v4();
        let library = route(&files, Some(&DropTarget::Library(Some(id))));
        assert_eq!((library.songs_to, library.media_playlist), (Some(DropTarget::Library(Some(id))), None));
    }

    #[test]
    fn the_place_under_the_pointer_is_the_smallest_that_takes_what_is_dragged() {
        let list = Rect::from_min_size(pos2(0.0, 100.0), vec2(300.0, 600.0));
        let row = Rect::from_min_size(pos2(0.0, 142.0), vec2(300.0, 42.0));
        let strip = Rect::from_min_size(pos2(320.0, 600.0), vec2(600.0, 150.0));
        let places = vec![
            Place::onto(list, DropTarget::Playlist { index: 0, after: None }),
            Place::onto(row, DropTarget::Playlist { index: 0, after: Some(1) }).outlining(list).line_at(row.max.y),
            Place::onto(strip, DropTarget::MediaStrip),
        ];
        let songs = [Kind::Song];
        let on_row = place_at(&places, pos2(50.0, 150.0), &songs).unwrap();
        assert_eq!(on_row.target, DropTarget::Playlist { index: 0, after: Some(1) });
        assert_eq!((on_row.outline, on_row.line), (list, Some(row.max.y)));
        let below = place_at(&places, pos2(50.0, 500.0), &songs).unwrap();
        assert_eq!(below.target, DropTarget::Playlist { index: 0, after: None });
        // The media strip takes no songs; it takes pictures.
        assert_eq!(place_at(&places, pos2(400.0, 650.0), &songs), None);
        assert_eq!(place_at(&places, pos2(400.0, 650.0), &[Kind::Media]).unwrap().target, DropTarget::MediaStrip);
        assert_eq!(place_at(&places, pos2(1000.0, 10.0), &songs), None);
    }

    #[test]
    fn the_pointer_on_the_screen_is_turned_into_the_windows_points() {
        // A window whose inside starts at (100, 50) points on a display drawn at 2 pixels a
        // point: winit gives (200, 100) pixels.
        assert_eq!(window_point((130.0, 80.0), (200, 100), 2.0, 2.0), pos2(30.0, 30.0));
        // The interface zoomed in to 1.25: the same place is fewer of its points in.
        assert_eq!(window_point((130.0, 80.0), (200, 100), 2.0, 2.5), pos2(24.0, 24.0));
        // On a display at 1 pixel a point, to the left of the main display.
        assert_eq!(window_point((-900.0, 300.0), (-1000, 280), 1.0, 1.0), pos2(100.0, 20.0));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_says_where_the_pointer_is() {
        let (x, y) = pointer_on_screen().expect("CoreGraphics gives the pointer's place");
        assert!(x.is_finite() && y.is_finite());
    }

    /// Where the files being dragged land with the pointer at `pointer`.
    fn landing(state: &HomeState, pointer: Option<Pos2>) -> Option<DropTarget> {
        target_at(&state.file_drag.places, &state.file_drag.files, pointer)
    }

    /// Runs one frame of the window as the app draws it.
    fn frame(ctx: &egui::Context, show: &Show, state: &mut HomeState) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(input, |ui| {
            home_ui(ui, show, state, &sp_model::Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn a_song_file_lands_on_the_playlist_or_library_under_the_pointer() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("Lantern", "A light\n"))]);
        let mut state = HomeState::default();
        let hymns = crate::libraries::Library::new("Hymns".to_owned());
        let id = hymns.id;
        state.libraries.push(hymns);
        state.drop_hover = true;
        state.file_drag.add(PathBuf::from("Harbour Lights.proBundle"));
        // The pointer in the window, but on nothing that takes songs.
        state.file_drag.pointer = Some(pos2(1270.0, 790.0));
        frame(&ctx, &show, &mut state);
        assert!(wants_chooser(&state));
        let place_of = |state: &HomeState, target: &DropTarget| {
            state.file_drag.places.iter().find(|place| place.target == *target).map(|place| place.hit)
        };
        // The chooser shows above the song list, with a row for the playlist and the library.
        let row = place_of(&state, &DropTarget::Library(Some(id))).expect("the library's row");
        let playlist = place_of(&state, &DropTarget::Playlist { index: 0, after: None }).expect("the playlist's row");
        let list = state.file_drag.places.iter().find(|place| place.songs_land).expect("the song list").clone();
        assert_eq!(list.target, DropTarget::Library(None), "All Songs is listed");
        assert!(row.max.y <= list.hit.min.y && playlist.max.y <= row.min.y);
        // Nothing under the pointer: the song list, where the song goes, lights up; the media
        // strip doesn't, as no pictures are coming.
        assert_eq!(landing(&state, state.file_drag.pointer), None);
        assert!(!strip_lit(&state));
        // Over the library's row, a drop goes there, and the next frame lights it up.
        state.file_drag.pointer = Some(row.center());
        assert_eq!(landing(&state, Some(row.center())), Some(DropTarget::Library(Some(id))));
        frame(&ctx, &show, &mut state);
        assert_eq!(state.file_drag.hit.as_ref().map(|place| place.target.clone()), Some(DropTarget::Library(Some(id))));
        // Over the playlist's row.
        assert_eq!(landing(&state, Some(playlist.center())), Some(DropTarget::Playlist { index: 0, after: None }));
        // Over the media strip, a song lands nowhere in particular (the library shown); a picture
        // lands in the strip, which lights up.
        let strip = state.file_drag.places.iter().find(|place| place.target == DropTarget::MediaStrip);
        assert!(strip.is_none(), "the strip is not a place while only songs come");
        state.file_drag.add(PathBuf::from("Sky.png"));
        frame(&ctx, &show, &mut state);
        let strip = place_of(&state, &DropTarget::MediaStrip).expect("the strip, now pictures come");
        state.file_drag.pointer = Some(strip.center());
        frame(&ctx, &show, &mut state);
        assert!(strip_lit(&state));
        let songs_only = FileDrag {
            files: vec![(PathBuf::from("Harbour Lights.proBundle"), Kind::Song)],
            places: state.file_drag.places.clone(),
            ..FileDrag::default()
        };
        assert_eq!(target_at(&songs_only.places, &songs_only.files, Some(strip.center())), None);
    }

    #[test]
    fn over_the_open_playlist_a_song_goes_after_the_item_under_the_pointer() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("Lantern", "A light\n"))]);
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        for name in ["Welcome", "Notices", "Sending"] {
            state.services.add(crate::service::Item::Header { name: name.to_owned() });
        }
        state.drop_hover = true;
        state.file_drag.add(PathBuf::from("Harbour Lights.pro"));
        state.file_drag.pointer = Some(pos2(1270.0, 790.0));
        frame(&ctx, &show, &mut state);
        let rows: Vec<&Place> = state
            .file_drag
            .places
            .iter()
            .filter(|place| matches!(place.target, DropTarget::Playlist { after: Some(_), .. }))
            .collect();
        assert_eq!(rows.len(), 3, "{:?}", state.file_drag.places);
        // With a playlist listed, a song dropped on nothing in particular lands in the library
        // shown (All Songs), whose row in the chooser lights up.
        let lands: Vec<&DropTarget> =
            state.file_drag.places.iter().filter(|place| place.songs_land).map(|place| &place.target).collect();
        assert_eq!(lands, [&DropTarget::Library(None)]);
        let second = rows[1].hit.center();
        assert_eq!(landing(&state, Some(second)), Some(DropTarget::Playlist { index: 0, after: Some(1) }));
        // Below the last item: at the end.
        let end = pos2(second.x, rows[2].hit.max.y + 20.0);
        assert_eq!(landing(&state, Some(end)), Some(DropTarget::Playlist { index: 0, after: None }));
    }
}
