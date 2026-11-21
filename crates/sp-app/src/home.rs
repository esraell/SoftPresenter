//! The home window: toolbar, library, slide grid, media bin and the live panel.
//!
//! This draws one frame and returns what the operator asked for. It changes nothing itself.
//!
//! Many controls are placeholders for parts that are not built yet. They are drawn in their
//! final positions so the layout is settled, they are dim, and they say "not built yet".

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind, TextureId, pos2, vec2};
use sp_engine::{Command, Show, SlideRef};
use sp_model::{Rgba, Slide, SlideRect, Theme};
use sp_render::object_frame;

use crate::displays::DisplayInfo;
use crate::editor::EditDraft;
use crate::screens::{Kind, Output, Screen};

use crate::icons::{self, Icon};

/// Bars around the slide area and the media bin.
const BAR: Color32 = crate::studio::PANEL;
/// The pills that hold toolbar items, and text fields.
const FIELD: Color32 = crate::studio::CARD;
/// The outline of what is chosen.
const CHOSEN_LINE: Color32 = crate::studio::ACCENT;
const SIDE: Color32 = crate::studio::PANEL;
const CANVAS: Color32 = crate::studio::PANEL;
const ON: Color32 = Color32::from_rgb(50, 208, 116);
const DIM: Color32 = crate::studio::TEXT_DIM;

/// Which list the Delete key deletes from: the one last clicked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Picking {
    #[default]
    Nothing,
    Songs,
    Media,
    Slides,
    /// A theme in the Theme Editor's list.
    Themes,
}

/// A drag across a list that picks out what it passes over. It starts on an item (or, in the
/// media bin, between items); while the pointer stays in the list it picks out everything
/// from where it started to where it is; once the pointer leaves the list, what was picked
/// is carried (to a playlist, or onto a slide).
#[derive(Clone, Copy, Debug)]
pub struct Sweep {
    pub list: Picking,
    /// The item it started on (`None` between items), and where the pointer was pressed.
    pub from: Option<usize>,
    pub start: egui::Pos2,
    /// It started on an item already among several picked out: they are carried, not re-picked.
    pub keeps: bool,
    /// What was picked has been handed to the drag-and-drop, to be dropped somewhere else.
    pub carried: bool,
}

/// Picks out `index` in a list, as a click does in the Finder: alone; with Command, added or
/// taken away; with Shift, everything from the last one clicked to it, in the order `shown`.
pub fn pick(picked: &mut Vec<usize>, anchor: &mut Option<usize>, shown: &[usize], index: usize, mods: egui::Modifiers) {
    if mods.shift
        && let Some(from) = anchor.and_then(|from| shown.iter().position(|at| *at == from))
        && let Some(to) = shown.iter().position(|at| *at == index)
    {
        let (a, b) = (from.min(to), from.max(to));
        *picked = shown[a..=b].to_vec();
        return;
    }
    if mods.command {
        match picked.iter().position(|at| *at == index) {
            Some(at) => {
                picked.remove(at);
            }
            None => picked.push(index),
        }
    } else {
        *picked = vec![index];
    }
    *anchor = Some(index);
}

/// How a presentation's slides are laid out in the Show view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlideView {
    /// Thumbnails in rows.
    #[default]
    Grid,
    /// The same tiles showing only each slide's words, which is quicker to read.
    Easy,
    /// One slide to a row: thumbnail, then its words.
    Table,
}

/// What a song pack is made of, from a right-click menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportRequest {
    /// These songs (by their place in the show).
    Songs(Vec<usize>),
    /// This playlist and its songs.
    Playlist(usize),
}

/// What the Edit view is editing, chosen by the switch at the top of its left column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// A song's slides.
    Slides,
    /// The themes and their templates (the Theme Editor).
    Themes,
    /// The props.
    Props,
    /// The stage screens' layouts.
    Stage,
}

pub struct HomeState {
    /// How slides are laid out, and how many fit across the grid (2 to 7).
    pub slide_view: SlideView,
    pub columns: usize,
    /// The slide picked out without being shown, by a right-click.
    pub selected_slide: Option<SlideRef>,
    /// Whether one slide fades into the next, and over how many seconds.
    /// How one slide gives way to the next on the screens.
    pub transition: sp_model::Transition,
    /// How one picture or video in the media bin gives way to the next.
    pub media_transition: sp_model::Transition,
    /// True on the frame a rename began in the Objects list, so its field takes the keyboard.
    pub rename_started: bool,
    /// Set when a new song is asked for (Command-N). The app makes it and clears this.
    pub new_song: bool,
    /// The New Song sheet while it is open, and a song it asked to be made.
    pub new_song_sheet: Option<NewSong>,
    pub new_song_request: Option<NewSong>,
    /// The song's title while it is being retyped in the heading, and the title just typed,
    /// with the song's place, for the app to take.
    pub renaming_song: Option<String>,
    pub pending_title: Option<(usize, String)>,
    /// The group being renamed in the editor's slide list (its place in the song) and the name
    /// as typed; and a name just typed, with the song's and group's places, for the app to take.
    pub renaming_group: Option<(usize, String)>,
    pub pending_group_name: Option<(usize, usize, String)>,
    /// The theme the operator has asked to delete, while they are being asked to confirm.
    pub confirm_delete_theme: Option<usize>,
    /// The open theme's name while it is being retyped in the list of themes.
    pub renaming_theme: Option<String>,

    /// The song a new slide has just been asked for, at its end. The app adds it.
    pub add_slide: Option<usize>,
    /// Objects copied in the editor, and the words that went to the system clipboard with them.
    pub object_clip: Vec<sp_model::Object>,
    pub object_clip_text: String,
    /// The running orders, and the one in front.
    pub services: crate::service::Services,
    /// For each presentation, the name its song is found by again in a service.
    pub song_keys: Vec<String>,
    /// The service's name while it is being retyped.
    pub renaming_service: Option<String>,
    /// Displayed thumbnail width in points.
    /// Text typed into the library's filter box.
    pub filter: String,
    /// Set by the app when the output has stopped drawing, so the operator is told.
    pub output_stopped: bool,
    /// Displayed media thumbnail width in points.
    /// True while a file is being dragged over the window.
    pub drop_hover: bool,
    /// A short message for the operator, shown in the media bin (a file that could not be
    /// added, a song that could not be saved).
    pub notice: Option<String>,
    /// Songs and themes changed outside SoftPresenter that wait on Reload or Keep mine, and the
    /// choice made in the bar that offers them (true for Reload).
    pub waiting: Vec<crate::outside::Waiting>,
    pub waiting_choice: Option<(crate::outside::Waiting, bool)>,
    /// Export…, chosen on a song's or a playlist's right-click menu.
    pub export_request: Option<ExportRequest>,
    /// The slide being edited. While this is set the window shows the Edit view.
    pub editing: Option<EditDraft>,
    /// The slide as it was just changed in the Edit view. The app takes this and puts it into
    /// the song.
    pub pending_edit: Option<(SlideRef, Slide)>,
    /// When on, edits to a slide that is on screen go to the output as they are typed.
    pub live_edit: bool,
    /// The font families installed on this computer, for the editor's font list.
    pub fonts: std::sync::Arc<Vec<String>>,
    /// True while the Bible page is showing in place of the slide grid.
    pub bible_open: bool,
    /// A slide cut or copied from its menu, for Paste.
    /// Slides copied (or cut), in order, and the words put on the system clipboard with them:
    /// a paste is of these slides only while the clipboard still holds those words.
    pub slide_clip: Vec<Slide>,
    pub slide_clip_text: String,
    /// The Edit view's undo order, newest last: each step a change to the slide being edited
    /// (undone by the draft) or to the song's slides (moved, pasted, deleted; undone by the
    /// app); and the steps undone, to redo. The draft the slide's steps belong to.
    /// Scroll the Edit view's list to the slide being edited, once (after ↑ or ↓).
    pub edit_list_scroll: bool,
    /// Scroll the playlist to this item, once (after ↑ or ↓).
    pub playlist_scroll_to: Option<usize>,
    pub edit_steps: Vec<EditStep>,
    pub edit_redo: Vec<EditStep>,
    pub edit_steps_draft: u64,
    /// Asked of the app: undo (false) or redo (true) the song's last change of its slides.
    pub song_undo: Option<bool>,
    /// Asked of the app: this song's changes can no longer be undone safely; drop them.
    pub forget_song_history: Option<usize>,
    /// Asked of the app: drop what the song's history could redo, as a change was made.
    pub clear_song_redo: bool,
    /// A change to a slide asked for from its menu.
    pub slide_request: Option<(SlideRef, SlideRequest)>,
    /// A song's arrangements changed in the Show view: the song, its arrangements, and the one
    /// it is played in.
    pub arrangement_request: Option<(usize, Vec<sp_model::Arrangement>, Option<sp_model::Uuid>)>,
    /// The libraries, the one shown (`None` for All Songs), and true when they have changed and
    /// are to be saved.
    pub libraries: Vec<crate::libraries::Library>,
    pub library_shown: Option<sp_model::Uuid>,
    pub libraries_changed: bool,
    /// A song whose looping is to be turned on or off, from its menu.
    pub song_loop_request: Option<usize>,
    /// A song asked to play on the Announcements layer (`Some(None)` to stop it), and the one
    /// playing there.
    pub announcement_request: Option<Option<usize>>,
    pub announcement_live: Option<usize>,
    /// A song given a transition of its own (or none, to follow the one for all).
    pub song_transition_request: Option<(usize, Option<sp_model::Transition>)>,
    /// A theme chosen for a song from its menu: the song, and the theme by its place (`None`
    /// puts it back on the songs theme).
    pub song_theme_request: Option<(usize, Option<usize>)>,
    /// The id of each theme, in the order of `theme_names`.
    pub theme_ids: Vec<sp_model::Uuid>,
    /// A change to a song asked for from its menu, and a song waiting to be confirmed deleted.
    pub song_request: Option<(usize, SongRequest)>,
    /// Songs waiting for a yes before they are deleted, and songs to delete now.
    pub confirm_delete_songs: Vec<usize>,
    pub songs_to_delete: Vec<usize>,
    /// Media bin items waiting for a yes before they are taken out of the bin.
    pub confirm_delete_media: Vec<usize>,
    /// Slides waiting for a yes before they are deleted, and slides to delete now.
    pub confirm_delete_slides: Vec<SlideRef>,
    pub slides_to_delete: Vec<SlideRef>,
    /// The slides picked out in the slide area (Command- or Shift-click, or a box dragged
    /// across them), and where each slide was drawn last, for the box to find them.
    pub picked_slides: Vec<SlideRef>,
    pub slide_rects: Vec<(SlideRef, Rect)>,
    /// The songs and media items picked out (several with Command or Shift, or by dragging
    /// across them), the one a Shift-click counts from in each, and which list Delete deletes
    /// from: the one last clicked.
    pub picked_songs: Vec<usize>,
    pub song_anchor: Option<usize>,
    pub picked_media: Vec<usize>,
    pub media_anchor: Option<usize>,
    pub picking: Picking,
    /// A drag across a list picking things out, while it lasts.
    pub sweep: Option<Sweep>,
    /// The playlist item whose slides the slide area last scrolled to, and whether the song
    /// was just picked in the slide area itself (so it isn't scrolled away from).
    pub slides_scrolled_to: Option<usize>,
    pub slides_picked_here: bool,
    /// The Reflow editor, while it is open: the song being reflowed and its words as text.
    pub reflow: Option<(usize, String)>,
    /// Words finished in the Reflow editor, for the app to rebuild the song from.
    pub reflow_done: Option<(usize, String)>,
    /// The library search pop-up (Command-F), while it is open: what is typed, and the row
    /// picked out with the arrow keys.
    pub library_search: Option<(String, usize)>,
    /// Where the find box is in the bar along the top, when the window is wide enough for it:
    /// what a search finds is listed under it.
    pub find_rect: Option<Rect>,
    /// The find box's Bible results, found on a thread of their own.
    pub finder: crate::find::Finder,
    /// How the computer writes the time of day, for the clock in the bar along the top.
    pub clock: crate::clock::SystemClock,
    /// A change to the media bin asked for.
    pub media_request: Option<MediaRequest>,
    /// Songs taken out of the library this run, by their place in the show.
    pub removed_songs: Vec<usize>,
    /// A change to the selected objects asked for from the menu bar.
    pub edit_request: Option<EditRequest>,
    /// A menu bar command chosen from a right-click menu in the window, carried out as if it
    /// had been chosen from the menu bar.
    pub menu_asked: Option<crate::menubar::Action>,
    /// True when a picture is to be chosen for the slide being edited; the picture chosen, copied
    /// into the Images folder, waiting to be put on the slide.
    pub picture_request: bool,
    /// True when a picture is to be chosen for the theme being edited.
    pub theme_picture_request: bool,
    pub picture_to_add: Vec<crate::images::Imported>,
    /// A view asked for from the menu bar: 0 Show, 1 Edit, 2 Bible, 3 Themes. The toolbar
    /// takes it as if its tab had been clicked.
    pub view_request: Option<usize>,
    /// True while the Settings window is open, and which of its panes is showing.
    pub preferences_open: bool,
    pub preferences_pane: usize,
    /// Settings › Integrations: VoxLive's state, and what is asked of it.
    pub voxlive: crate::voxlive::View,
    /// Where the VoxLive item is among the presentations.
    pub voxlive_slot: Option<usize>,
    /// A VoxLive screen chosen in the VoxLive tab, by its slide in the VoxLive item, to put on
    /// screen.
    pub voxlive_go: Option<usize>,
    /// When a song's copyright line is shown, and the church's CCLI licence number.
    pub copyright_show: crate::service::CopyrightShow,
    pub ccli_licence: String,
    /// True to start with the audience screens off.
    pub start_audience_off: bool,
    /// What starts playing by itself when the app opens.
    pub start_with: crate::service::StartWith,
    /// Every song by its key and title, for choosing one in Settings › General; made while
    /// Settings is drawn.
    pub song_choices: Vec<(String, String)>,
    /// Lists worked out from the songs, kept from one frame to the next.
    pub lists: crate::kept::Lists,
    /// The media playlist playing item after item on the media layer, if one is.
    pub media_playing: Option<sp_model::Uuid>,
    /// New copyright details for a song, from its Info pop-over.
    pub copyright_request: Option<(usize, sp_model::Copyright)>,
    /// True while the Screens window is open.
    pub settings_open: bool,
    /// The displays connected now, kept up to date by the app.
    pub displays: Vec<DisplayInfo>,
    /// The audience screens, in order. The Screens window changes this list and sets
    /// `screens_changed`; the app then saves it and moves the pictures.
    pub screens: Vec<Screen>,
    pub screens_changed: bool,
    /// What each screen is doing now, in words, and whether it is being shown somewhere.
    pub screen_status: Vec<(String, bool)>,
    /// The screen chosen in the Screens window.
    pub screen_selected: usize,
    /// A service header being renamed, by its place, and its name as typed so far.
    pub renaming_header: Option<(usize, String)>,

    /// The left column lists a playlist's items, not a library's songs.
    pub playlist_open: bool,
    /// The chooser of playlists and libraries is open in the left column, in place of the list.
    pub chooser_open: bool,
    /// The order a library's songs are listed in.
    pub song_order: crate::kept::SongOrder,
    /// The item of the playlist in front that was on screen when the left column last looked:
    /// its row is brought into view once, when that changes.
    pub live_row_seen: Option<usize>,
    /// The media bin's playlists, the one shown (all media when none), and whether they changed
    /// since they were last saved.
    pub media_playlists: Vec<crate::media::Playlist>,
    pub media_playlist_shown: Option<sp_model::Uuid>,
    pub media_playlists_changed: bool,

    /// A library being renamed in its row, and the name typed so far.
    pub renaming_library: Option<(sp_model::Uuid, String)>,
    /// Next at the end of a song goes on to the next song in the service.
    pub continuous: bool,
    /// A test pattern shown on the audience screens in place of everything else.
    pub test_pattern: Option<crate::props::TestPattern>,
    /// True while the notes of the slide on screen are shown above the slides.
    pub show_notes: bool,
    /// The props, and true when they have changed and are to be saved.
    pub props: Vec<crate::props::Prop>,
    pub props_changed: bool,
    /// The props on screen, in the order they were put on, and since when.
    pub props_live: Vec<(sp_model::Uuid, std::time::Instant)>,
    /// A prop being renamed, and its name as typed so far.
    pub prop_rename: Option<(sp_model::Uuid, String)>,
    /// The macros, and true when they have changed and are to be saved.
    pub macros: Vec<crate::macros::Macro>,
    pub macros_changed: bool,
    /// A macro being renamed, and its name as typed so far.
    pub macro_rename: Option<(sp_model::Uuid, String)>,
    /// Actions to carry out, from a macro that was clicked. The app does them.
    pub actions_to_run: Vec<sp_model::ShowAction>,
    /// How far along a chain of slides' actions the waiting actions are: 0 for a macro or a
    /// slide the operator put on screen.
    pub actions_depth: u8,
    /// The audio tracks, and true when the list has changed and is to be saved.
    pub audio: crate::audio::Library,
    pub audio_changed: bool,
    /// Something asked of the audio player, what it is playing, and its volume.
    pub audio_request: Option<AudioRequest>,
    /// The video on screen, as the operator sees it, and what they asked of it.
    pub video_live: Option<VideoLive>,
    pub video_request: Option<VideoRequest>,
    pub audio_playing: crate::audio::Playing,
    pub audio_volume: f32,
    /// The tab open under the Program panel: 0 Timers, 1 Messages, 2 Props, 3 Macros, 4 Stage,
    /// 5 Audio.
    pub live_tab: usize,
    /// The timers, and true when their settings have changed and are to be saved.
    pub timers: Vec<crate::timers::Timer>,
    pub timers_changed: bool,
    /// The timer whose settings are open.
    pub timer_open: Option<sp_model::Uuid>,
    /// The messages, and true when they have changed and are to be saved.
    pub messages: Vec<crate::messages::Message>,
    pub messages_changed: bool,
    pub message_selected: Option<sp_model::Uuid>,
    /// True when the media bin is folded away (View > Media Bin).
    pub media_hidden: bool,
    /// How large the Edit view shows the slide: 0 fits it to the window, otherwise how many
    /// screen pixels each of the slide's pixels takes. How far it has been moved when it is
    /// larger than the window, and whether the grid shows.
    pub edit_zoom: f32,
    pub edit_pan: egui::Vec2,
    pub edit_grid: bool,
    /// The stage layouts, and true when they have changed and are to be saved.
    pub stage_layouts: Vec<sp_model::ThemeTemplate>,
    pub stage_layouts_changed: bool,
    /// True while the Stage Layouts window is open; the layout chosen in it, and its item chosen.
    pub stage_editor_open: bool,
    pub stage_edit: (usize, Option<usize>),
    /// True when the Program preview shows what the stage screens show, not the audience's.
    pub preview_stage: bool,
    /// A message cut or copied, to paste; and one being renamed in its row, with its name so far.
    pub message_clip: Option<crate::messages::Message>,
    pub message_rename: Option<(sp_model::Uuid, String)>,
    /// The message showing over the slides, and since when.
    pub message_live: Option<(sp_model::Uuid, std::time::Instant)>,
    /// The note for the people on stage, and whether it is showing on the stage screens.
    pub stage_message: String,
    pub stage_message_live: bool,
    /// True while the stage screens are on.
    pub stage_on: bool,
    /// True while every screen shows its own name instead of the show.
    pub identify: bool,
    /// The setup a fresh install starts with, while it is open.
    pub setup: Option<crate::setup::Setup>,
    /// What the setup asked the app to do.
    pub setup_request: Option<crate::setup::SetupRequest>,
    /// True once the setup has been seen, so it isn't shown again at the next start.
    pub setup_done: bool,
    /// An NDI name being typed in the Screens window, and the screen it is for. It is used
    /// when the field is left, so NDI is not restarted at every letter.
    pub ndi_draft: Option<(sp_model::Uuid, String)>,
    /// True when this build includes NDI.
    pub ndi_available: bool,
    /// The Edit view's right-hand panel.
    pub inspector: crate::inspector::InspectorState,
    /// The Theme Editor, while it is open. The layout being worked on is the draft in `editing`.
    pub theme_edit: Option<ThemeEdit>,
    /// Every theme's name, and the places of the ones songs and Bible slides use.
    pub theme_names: Vec<String>,
    /// The places in the list of the themes that can't be deleted: the two built in.
    pub themes_kept: Vec<usize>,
    /// What each theme calls its two templates, the one for two languages first.
    pub template_names: Vec<Vec<String>>,
    /// The templates of the theme songs use, by id and name, for a slide's Template menu.
    pub song_templates: Vec<(sp_model::Uuid, String)>,
    /// A slide whose template was just chosen from its menu (`None` lets the theme choose).
    /// The app changes the slide and the screen.
    pub pending_template: Option<(SlideRef, Option<sp_model::Uuid>)>,
    /// The template being renamed in the Theme Editor, and its name as typed so far.
    pub renaming_template: Option<(usize, String)>,
    /// The template of the theme being edited that is about to be deleted, once confirmed.
    pub confirm_delete_template: Option<usize>,
    /// Each theme's two layouts as boxes, for the pictures in the theme picker.
    pub theme_layouts: Vec<Vec<Vec<(sp_model::Role, SlideRect)>>>,
    /// Which layout of its theme Bible slides use.
    pub bible_layout: usize,
    pub theme_for_songs: usize,
    pub theme_for_bible: usize,
    /// What the Theme menu or the Theme Editor has just asked for. The app takes this.
    pub theme_request: Option<ThemeRequest>,
    /// A kind of the Edit view asked for (by its switch, the View menu, the command line, the
    /// Stage tab), until the top bar carries it out.
    pub edit_kind_request: Option<EditKind>,
    /// What is typed in the Bible page's two fields, and its rows of translations.
    pub bible: BiblePage,
}

/// Where a translation's words, or its reference, go on a Bible slide: one of the Bible
/// theme's boxes, or nowhere.
pub type BibleTarget = Option<sp_model::Role>;

/// The boxes one of a theme's templates has, and where each sits.
pub fn layout_boxes(theme: &Theme, template: usize) -> Vec<(sp_model::Role, SlideRect)> {
    let boxes = theme.templates.get(template).map_or(&[][..], |template| template.boxes.as_slice());
    boxes.iter().map(|style| (style.role.clone(), style.rect)).collect()
}

/// What each box of a template is called, in the order `layout_boxes` gives them.
pub fn layout_labels(theme: &Theme, template: usize) -> Vec<String> {
    let boxes = theme.templates.get(template).map_or(&[][..], |template| template.boxes.as_slice());
    boxes.iter().map(sp_model::BoxStyle::label).collect()
}

/// A small picture of a slide with its boxes outlined and one of them filled, to show which
/// box a menu entry means.
pub fn box_diagram(
    painter: &egui::Painter,
    area: Rect,
    boxes: &[(sp_model::Role, SlideRect)],
    lit: Option<&sp_model::Role>,
) {
    painter.rect_filled(area, 2.0, Color32::BLACK);
    let scale = area.width() / sp_model::SLIDE_WIDTH;
    for (role, frame) in boxes {
        let rect = Rect::from_min_size(area.min + vec2(frame.x, frame.y) * scale, vec2(frame.w, frame.h) * scale);
        if lit == Some(role) {
            painter.rect_filled(rect, 1.0, Color32::from_rgb(250, 204, 21));
        } else {
            painter.rect_stroke(rect, 1.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Inside);
        }
    }
}

/// One translation on the Bible page and where its text goes.
#[derive(Clone, Debug, PartialEq)]
pub struct BibleRow {
    /// Which Bible, as its place on the shelf; `None` while the row shows none, so its Bible
    /// can go to another row (to swap two, say).
    pub bible: Option<usize>,
    pub verse: BibleTarget,
    pub reference: BibleTarget,
}

/// The Bible page: what is typed, the translations chosen, and the passage being shown.
#[derive(Debug)]
pub struct BiblePage {
    pub lookup: String,
    pub search: String,
    pub rows: Vec<BibleRow>,
    pub options: crate::bible::BibleOptions,
    /// The Bibles installed.
    pub shelf: crate::bible::Shelf,
    /// The boxes of the theme layout Bible slides use, and where each sits on the slide. The
    /// Verse and Reference menus offer these.
    pub boxes: Vec<(sp_model::Role, SlideRect)>,
    /// What each of those boxes is called, in the same order.
    pub box_labels: Vec<String>,
    /// The template of the Bible theme that Bible slides are drawn with.
    pub template: Option<sp_model::Uuid>,
    /// Bibles being downloaded.
    pub downloads: crate::bible_store::Downloads,
    /// The two places among the show's presentations kept for Bible passages. A new passage
    /// goes into the one that is not on screen, so looking up the next verses never changes
    /// what the audience sees.
    pub slots: Option<[usize; 2]>,
    /// Which of the two the page is showing.
    pub showing: usize,
    /// What was typed for the passage now shown, and the choices it was made with.
    pub shown: String,
    pub made_with: String,
    /// A passage just made. The app takes it and puts it in the slot named.
    pub pending: Option<(usize, sp_model::Presentation)>,
    /// What went wrong with the last lookup, for the operator.
    pub message: Option<String>,
    /// Verses found by the last search: the verse's reference, its chapter's, and its words.
    pub found: Vec<(String, String, String)>,
    /// When Scripture Lookup was last typed in: its passage is made a moment after typing
    /// stops, without waiting for Return.
    pub typed_at: Option<f64>,
    /// Put the cursor in Scripture Lookup with its words selected, when it is next drawn.
    pub focus_lookup: bool,
    /// The passages that were on screen, newest first, and whether that list has changed
    /// since it was last saved.
    pub history: Vec<crate::service::HistoryEntry>,
    pub history_changed: bool,
    /// The book, chapter and verse lists beside the slides: whether they show, the book and
    /// chapter open in them (chapter 0 for none yet), the verse a Shift-click runs from, and
    /// whether to scroll them to what is open.
    pub lists_open: bool,
    /// Whether the table of translations (which Bibles, which boxes) is open, rather than
    /// folded into one line.
    pub rows_open: bool,
    pub list_book: usize,
    pub list_chapter: u16,
    pub list_from: Option<u16>,
    pub list_scroll: bool,
    /// A slide of the passage just looked up to put on screen once the app has it (a verse
    /// double-clicked in the lists), by its place in the passage.
    pub go_live: Option<usize>,
    /// Two verses made with the choices of the moment, for the Options panel's preview when
    /// no passage is showing, and the choices they were made with.
    pub sample: Option<(String, sp_model::Presentation)>,
}

impl Default for BiblePage {
    fn default() -> BiblePage {
        let shelf = crate::bible::Shelf::shipped();
        let place = |id: &str| shelf.position(id).unwrap_or(0);
        BiblePage {
            lookup: String::new(),
            search: String::new(),
            options: crate::bible::BibleOptions::default(),
            // Two translations, one for each language, as the built-in Bible theme lays them out:
            // the Russian Synodal on top, the King James Version below, each with its reference.
            rows: vec![
                BibleRow {
                    bible: Some(place("syn")),
                    verse: Some(sp_model::Role::primary()),
                    reference: Some(sp_model::Role::reference()),
                },
                BibleRow {
                    bible: Some(place("kjv")),
                    verse: Some(sp_model::Role::translation()),
                    reference: Some(sp_model::Role::translation_reference()),
                },
            ],
            shelf,
            boxes: layout_boxes(&Theme::builtin_bible(), 0),
            box_labels: layout_labels(&Theme::builtin_bible(), 0),
            template: None,
            downloads: crate::bible_store::Downloads::default(),
            slots: None,
            showing: 0,
            shown: String::new(),
            made_with: String::new(),
            pending: None,
            message: None,
            found: Vec::new(),
            typed_at: None,
            focus_lookup: false,
            history: Vec::new(),
            history_changed: false,
            lists_open: true,
            rows_open: false,
            list_book: 0,
            list_chapter: 0,
            list_from: None,
            list_scroll: false,
            go_live: None,
            sample: None,
        }
    }
}

impl BiblePage {
    /// The most translations that can be shown together.
    pub const MAX_ROWS: usize = 4;

    /// Adds a translation row, up to the limit. Returns true if one was added. When every
    /// installed Bible is already in a row, it says so.
    pub fn add_row(&mut self) -> bool {
        // The next Bible no row is using yet.
        let unused = (0..self.shelf.bibles.len()).find(|at| self.rows.iter().all(|row| row.bible != Some(*at)));
        let Some(bible) = unused.filter(|_| self.rows.len() < Self::MAX_ROWS) else {
            if unused.is_none() {
                self.message = Some(
                    "All available bibles are in use. If you would like to add another translation, please install more bibles."
                        .to_owned(),
                );
            }
            return false;
        };
        self.rows.push(BibleRow { bible: Some(bible), verse: None, reference: None });
        true
    }

    /// Removes a downloaded Bible, and points any row that used it at the first Bible.
    pub fn remove_bible(&mut self, index: usize) {
        match self.shelf.remove(index) {
            Ok(()) => {
                // A row that showed the Bible removed goes, and the rows close up (one always
                // stays, showing the first Bible); the rest keep theirs.
                self.rows.retain(|row| row.bible != Some(index));
                for row in &mut self.rows {
                    if let Some(at) = row.bible
                        && at > index
                    {
                        row.bible = Some(at - 1);
                    }
                }
                if self.rows.is_empty() {
                    self.rows.push(BibleRow { bible: Some(0), verse: None, reference: None });
                }
                self.assign_boxes();
            }
            Err(problem) => self.options.problem = Some(problem),
        }
    }

    /// Points each translation at the boxes of the layout now in use, as choosing a theme
    /// does: the first translation's words in the main box and its reference in the reference
    /// box, the second's words in the translation box, the rest nowhere.
    pub fn assign_boxes(&mut self) {
        let has = |role: sp_model::Role| self.boxes.iter().any(|(have, _)| *have == role).then_some(role);
        let main = has(sp_model::Role::primary()).or_else(|| self.boxes.first().map(|(role, _)| role.clone()));
        let reference = has(sp_model::Role::reference());
        let translation = has(sp_model::Role::translation());
        let translation_reference = has(sp_model::Role::translation_reference());
        for (index, row) in self.rows.iter_mut().enumerate() {
            (row.verse, row.reference) = match index {
                0 => (main.clone(), reference.clone()),
                1 => (translation.clone(), translation_reference.clone()),
                _ => (None, None),
            };
        }
        // Whatever is showing is made again with the new boxes.
        self.made_with.clear();
    }

    /// The passage now shown, as something a service can keep.
    pub fn saved(&self) -> Option<crate::service::SavedPassage> {
        (!self.shown.is_empty()).then(|| self.settings())
    }

    /// How the page is set up now: its translations, the boxes their words and references
    /// go in, and the slide options. Kept between runs, and with each saved passage.
    pub fn settings(&self) -> crate::service::SavedPassage {
        use crate::bible::ReferenceStyle;
        let rows = self
            .rows
            .iter()
            .filter_map(|row| {
                Some(crate::service::SavedRow {
                    bible: self.shelf.bibles.get(row.bible?)?.id.clone(),
                    verse: row.verse.as_ref().map(|role| role.0.clone()),
                    reference: row.reference.as_ref().map(|role| role.0.clone()),
                })
            })
            .collect();
        let o = &self.options;
        crate::service::SavedPassage {
            lookup: self.shown.clone(),
            rows,
            show_verse_numbers: o.show_verse_numbers,
            break_on_new_verse: o.break_on_new_verse,
            display_translation: o.display_translation,
            reference_style: match o.reference {
                ReferenceStyle::VerseReferences => "verse",
                ReferenceStyle::PassageEach => "passage_each",
                ReferenceStyle::PassageLast => "passage_last",
                ReferenceStyle::NoReference => "none",
            }
            .to_owned(),
        }
    }

    /// Brings a saved passage back: its translations and options are set as they were, and
    /// it is looked up. A translation that is no longer installed is left out, and said so.
    pub fn open_saved(&mut self, saved: &crate::service::SavedPassage, live: Option<usize>) {
        let missing = self.apply_settings(saved);
        self.lookup = saved.lookup.clone();
        self.look_up(live);
        if !missing.is_empty() && self.message.is_none() {
            self.message = Some(format!("Not installed, so left out: {}.", missing.join(", ")));
        }
    }

    /// Sets the page up as it was saved: translations, boxes and slide options. Returns the
    /// translations that are no longer installed, which are left out.
    pub fn apply_settings(&mut self, saved: &crate::service::SavedPassage) -> Vec<String> {
        let (rows, options, missing) = self.saved_choices(saved);
        if !rows.is_empty() {
            self.rows = rows;
        }
        let o = &mut self.options;
        o.show_verse_numbers = options.show_verse_numbers;
        o.break_on_new_verse = options.break_on_new_verse;
        o.display_translation = options.display_translation;
        o.reference = options.reference;
        missing
    }

    /// What a saved passage asks the page for: its translations as rows (each translation no
    /// longer installed left out, and named), and its slide options.
    fn saved_choices(
        &self,
        saved: &crate::service::SavedPassage,
    ) -> (Vec<BibleRow>, crate::bible::BibleOptions, Vec<String>) {
        use crate::bible::ReferenceStyle;
        let mut rows = Vec::new();
        let mut missing = Vec::new();
        // A box the Bible template no longer has is not a place to send anything.
        let known = |role: &Option<String>| {
            role.clone().map(sp_model::Role).filter(|role| self.boxes.iter().any(|(have, _)| have == role))
        };
        for row in &saved.rows {
            match self.shelf.position(&row.bible) {
                Some(bible) => rows.push(BibleRow {
                    bible: Some(bible),
                    verse: known(&row.verse),
                    reference: known(&row.reference),
                }),
                None => missing.push(row.bible.to_uppercase()),
            }
        }
        let options = crate::bible::BibleOptions {
            show_verse_numbers: saved.show_verse_numbers,
            break_on_new_verse: saved.break_on_new_verse,
            display_translation: saved.display_translation,
            reference: match saved.reference_style.as_str() {
                "passage_each" => ReferenceStyle::PassageEach,
                "passage_last" => ReferenceStyle::PassageLast,
                "none" => ReferenceStyle::NoReference,
                _ => ReferenceStyle::VerseReferences,
            },
            ..crate::bible::BibleOptions::default()
        };
        (rows, options, missing)
    }

    /// A saved passage's slides, made as opening it would make them, but leaving the page as
    /// it is: its translations, options, what is typed and the passage shown all stay, so the
    /// command line can read a playlist's passage while the operator uses the page.
    pub fn passage_of(&mut self, saved: &crate::service::SavedPassage) -> Result<sp_model::Presentation, String> {
        let (rows, options, _) = self.saved_choices(saved);
        // None of its translations installed: the page's own, as opening it would leave them.
        let rows = if rows.is_empty() { self.rows.clone() } else { rows };
        let choices = self.choices_for(&rows)?;
        crate::bible::build_passage(&saved.lookup, &choices, &options, self.template)
    }

    /// The choices a passage is made with, as text, so a change to any of them can be noticed.
    /// The first row's Bible (skipping rows set to none): the one a passage is looked up and
    /// searched in.
    pub fn lead(&self) -> Option<usize> {
        self.rows.iter().find_map(|row| row.bible)
    }

    pub fn choices(&self) -> String {
        let o = &self.options;
        format!(
            "{:?} {} {} {} {:?}",
            self.rows, o.show_verse_numbers, o.break_on_new_verse, o.display_translation, o.reference
        )
    }

    /// Looks up what is typed and readies its slides for the app to take. `live` is the
    /// presentation on screen now, if any.
    pub fn look_up(&mut self, live: Option<usize>) {
        self.typed_at = None;
        self.make(live, false);
    }

    /// The same while typing: the slides follow what is typed, but the field is left as
    /// typed and a reference not yet finished says nothing.
    pub fn preview(&mut self, live: Option<usize>) {
        self.make(live, true);
    }

    /// The translations a passage is made in, with where each one's words and reference go.
    /// A row set to no Bible is left out. The error is a plain sentence.
    fn row_choices(&mut self) -> Result<Vec<crate::bible::RowChoice>, String> {
        let rows = self.rows.clone();
        self.choices_for(&rows)
    }

    /// The same for rows other than the page's own (a saved passage's).
    fn choices_for(&mut self, rows: &[BibleRow]) -> Result<Vec<crate::bible::RowChoice>, String> {
        let mut choices = Vec::new();
        for row in rows {
            let Some(at) = row.bible else { continue };
            let bible = self.shelf.get(at)?;
            choices.push(crate::bible::RowChoice { bible, verse: row.verse.clone(), reference: row.reference.clone() });
        }
        if choices.is_empty() {
            return Err("Choose a Bible for at least one row.".to_owned());
        }
        Ok(choices)
    }

    /// Two verses made with the choices of the moment, for the Options panel to preview when
    /// no passage is showing. Made again only when a choice changes, so its pictures are drawn
    /// once each time.
    pub fn sample(&mut self) -> Option<&sp_model::Presentation> {
        let made_with = format!("{} {:?}", self.choices(), self.template);
        if self.sample.as_ref().is_none_or(|(had, _)| *had != made_with) {
            let rows = self.row_choices().ok()?;
            let passage = crate::bible::build_passage(SAMPLE_PASSAGE, &rows, &self.options, self.template).ok()?;
            self.sample = Some((made_with, passage));
        }
        self.sample.as_ref().map(|(_, passage)| passage)
    }

    fn make(&mut self, live: Option<usize>, quiet: bool) {
        let Some(slots) = self.slots else { return };
        let rows = match self.row_choices() {
            Ok(rows) => rows,
            Err(problem) => {
                if !quiet {
                    self.message = Some(problem);
                }
                return;
            }
        };
        match crate::bible::build_passage(&self.lookup, &rows, &self.options, self.template) {
            Ok(passage) => {
                let passage_name = passage.title.clone();
                // Leave a passage that is on screen alone: use the other slot.
                if live == Some(slots[self.showing]) {
                    self.showing = 1 - self.showing;
                }
                self.pending = Some((slots[self.showing], passage));
                // The lists open on the passage, wherever it came from.
                if let Some(range) = self
                    .lead()
                    .and_then(|lead| self.shelf.get(lead).ok())
                    .and_then(|bible| sp_bible::parse_reference_in(&passage_name, &bible.info.language).ok())
                    && (range.start.book, range.start.chapter) != (self.list_book, self.list_chapter)
                {
                    (self.list_book, self.list_chapter) = (range.start.book, range.start.chapter);
                    self.list_scroll = true;
                }
                self.shown = passage_name.clone();
                self.made_with = self.choices();
                self.message = None;
                if !quiet {
                    // The field then says the passage in full, in the first Bible's language:
                    // "john 3" becomes "John 3:1-36".
                    self.lookup = passage_name;
                    self.found.clear();
                }
            }
            Err(problem) if !quiet => self.message = Some(problem),
            Err(_) => {}
        }
    }

    /// Notes a passage that has just gone on screen, at the top of the history. One already
    /// there moves up; the same passage again (its next verse, say) changes nothing.
    pub fn note_shown(&mut self, passage: &str, at: chrono::NaiveDateTime) {
        if passage.is_empty() || self.history.first().is_some_and(|newest| newest.passage == passage) {
            return;
        }
        self.history.retain(|entry| entry.passage != passage);
        let at = at.format("%Y-%m-%dT%H:%M:%S").to_string();
        self.history.insert(0, crate::service::HistoryEntry { passage: passage.to_owned(), at });
        self.history.truncate(HISTORY_LENGTH);
        self.history_changed = true;
    }

    /// Makes the passage shown one verse longer, at its end or at its start.
    pub fn step(&mut self, forward: bool, live: Option<usize>) {
        let Some(lead) = self.lead() else { return };
        let Ok(bible) = self.shelf.get(lead) else { return };
        let Ok(range) = sp_bible::parse_reference_in(&self.shown, &bible.info.language) else { return };
        match bible.extend(range, forward) {
            Some(longer) => {
                self.lookup = bible.label(longer.start, longer.end);
                self.look_up(live);
            }
            None => {
                let end = if forward { "end" } else { "start" };
                self.message = Some(format!("That is the {end} of {}.", bible.book_name(range.start.book)));
            }
        }
    }

    /// Finds verses holding the words typed in Search, in the first translation.
    pub fn find(&mut self) {
        self.found.clear();
        self.message = None;
        let Some(lead) = self.lead() else { return };
        match self.shelf.get(lead) {
            Ok(bible) => {
                for (place, words) in bible.search(&self.search, 60) {
                    let chapter = format!("{} {}", bible.book_name(place.book), place.chapter);
                    self.found.push((bible.label(place, place), chapter, words.to_owned()));
                }
                if self.found.is_empty() && !self.search.trim().is_empty() {
                    self.message = Some(format!("Nothing in the {} has those words.", bible.info.name));
                }
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// Removes a translation row, always leaving one. Returns true if one was removed.
    pub fn remove_row(&mut self, index: usize) -> bool {
        if self.rows.len() <= 1 || index >= self.rows.len() {
            return false;
        }
        self.rows.remove(index);
        true
    }
}

/// The Theme Editor's working copy of a theme.
#[derive(Clone, Debug)]
pub struct ThemeEdit {
    /// The theme's place in the list.
    pub index: usize,
    pub theme: Theme,
    /// Which of the theme's two layouts is on the canvas.
    pub template: usize,
    /// Set when the theme has changed this frame. The app takes the theme and clears this.
    pub changed: bool,
    /// Set when the operator asks to go back to the Show view.
    pub leave: bool,
}

impl ThemeEdit {
    pub fn of(index: usize, theme: Theme) -> ThemeEdit {
        ThemeEdit { index, theme, template: 0, changed: false, leave: false }
    }
}

/// Something asked of the app about themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeRequest {
    /// Songs use the theme at this place in the list.
    UseForSongs(usize),
    /// Bible slides use this template of the theme at this place in the list.
    UseForBible(usize, usize),
    /// Make a copy of the theme at this place in the list and open it in the Theme Editor.
    Duplicate(usize),
    /// Make a new theme, a copy of the one songs use, and open it in the Theme Editor.
    New,
    /// Open the Theme Editor on the theme at this place in the list.
    Edit(usize),
    /// Delete the theme at this place in the list, for good.
    Delete(usize),
}

/// The picture of the audience screen for the Program panel: what is there now, what was
/// there before, and how far the new one has faded in (1 when there is no fade).
#[derive(Clone, Copy, Debug, Default)]
pub struct Preview {
    pub now: Option<TextureId>,
    pub before: Option<TextureId>,
    pub amount: f32,
    /// Why a screen is not showing this picture but the one before it, when one is not.
    pub undrawn: Option<crate::output::Undrawn>,
}

/// Which thumbnail the window is asking for.
#[derive(Clone, Copy)]
pub enum ThumbOf<'a> {
    /// A slide of a Bible passage, drawn with the theme Bible slides use.
    BibleSlide(&'a Slide),
    /// One of a theme's layouts, by its place; drawn fresh whenever the theme changes.
    Layout(usize, &'a Slide),
    Slide(&'a Slide),
    /// A media item, by its index in the show.
    Media(usize),
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState {
            slide_view: SlideView::Grid,
            columns: 4,
            selected_slide: None,
            transition: sp_model::Transition::default(),
            media_transition: sp_model::Transition::default(),
            rename_started: false,
            new_song: false,
            new_song_sheet: None,
            new_song_request: None,
            renaming_song: None,
            pending_title: None,
            renaming_group: None,
            pending_group_name: None,
            confirm_delete_theme: None,
            renaming_theme: None,

            add_slide: None,
            object_clip: Vec::new(),
            object_clip_text: String::new(),
            services: crate::service::Services::default(),
            song_keys: Vec::new(),
            renaming_service: None,
            filter: String::new(),
            output_stopped: false,
            drop_hover: false,
            notice: None,
            waiting: Vec::new(),
            waiting_choice: None,
            export_request: None,
            editing: None,
            pending_edit: None,
            live_edit: false,
            fonts: std::sync::Arc::default(),
            bible_open: false,
            settings_open: false,
            displays: Vec::new(),
            view_request: None,
            preferences_open: false,
            preferences_pane: 0,
            voxlive: Default::default(),
            voxlive_slot: None,
            voxlive_go: None,
            copyright_show: crate::service::CopyrightShow::Never,
            ccli_licence: String::new(),
            start_audience_off: false,
            start_with: crate::service::StartWith::Nothing,
            song_choices: Vec::new(),
            lists: crate::kept::Lists::default(),
            media_playing: None,
            copyright_request: None,
            edit_request: None,
            menu_asked: None,
            picture_request: false,
            theme_picture_request: false,
            picture_to_add: Vec::new(),
            slide_clip: Vec::new(),
            slide_clip_text: String::new(),
            edit_list_scroll: false,
            playlist_scroll_to: None,
            edit_steps: Vec::new(),
            edit_redo: Vec::new(),
            edit_steps_draft: 0,
            song_undo: None,
            forget_song_history: None,
            clear_song_redo: false,
            slide_request: None,
            song_request: None,
            song_theme_request: None,
            song_loop_request: None,
            song_transition_request: None,
            announcement_request: None,
            announcement_live: None,
            libraries: Vec::new(),
            library_shown: None,
            libraries_changed: false,
            arrangement_request: None,
            theme_ids: Vec::new(),
            confirm_delete_songs: Vec::new(),
            songs_to_delete: Vec::new(),
            confirm_delete_media: Vec::new(),
            confirm_delete_slides: Vec::new(),
            slides_to_delete: Vec::new(),
            picked_slides: Vec::new(),
            slide_rects: Vec::new(),
            picked_songs: Vec::new(),
            song_anchor: None,
            picked_media: Vec::new(),
            media_anchor: None,
            picking: Picking::Nothing,
            sweep: None,
            slides_scrolled_to: None,
            slides_picked_here: false,
            removed_songs: Vec::new(),
            media_request: None,
            library_search: None,
            find_rect: None,
            finder: crate::find::Finder::default(),
            clock: crate::clock::SystemClock::default(),
            reflow: None,
            reflow_done: None,
            screens: crate::screens::starting(None),
            screens_changed: false,
            screen_status: Vec::new(),
            screen_selected: 0,
            show_notes: false,

            playlist_open: false,
            chooser_open: false,
            song_order: crate::kept::SongOrder::Listed,
            live_row_seen: None,
            media_playlists: Vec::new(),
            media_playlist_shown: None,
            media_playlists_changed: false,
            renaming_library: None,
            renaming_header: None,
            continuous: false,
            test_pattern: None,
            props: Vec::new(),
            props_changed: false,
            macros: Vec::new(),
            audio: crate::audio::Library::default(),
            audio_changed: false,
            audio_request: None,
            video_live: None,
            video_request: None,
            audio_playing: crate::audio::Playing::default(),
            audio_volume: 1.0,
            macros_changed: false,
            macro_rename: None,
            actions_to_run: Vec::new(),
            actions_depth: 0,
            props_live: Vec::new(),
            prop_rename: None,
            live_tab: 0,
            timers: Vec::new(),
            timers_changed: false,
            timer_open: None,
            messages: Vec::new(),
            messages_changed: false,
            message_selected: None,
            media_hidden: false,
            edit_zoom: 0.0,
            edit_pan: egui::Vec2::ZERO,
            edit_grid: false,
            stage_layouts: crate::stage::builtin_layouts(),
            stage_layouts_changed: false,
            stage_editor_open: false,
            stage_edit: (0, None),
            preview_stage: false,
            message_clip: None,
            message_rename: None,
            message_live: None,
            stage_message: String::new(),
            stage_message_live: false,
            stage_on: true,
            identify: false,
            setup: None,
            setup_request: None,
            setup_done: false,
            ndi_draft: None,
            // Assumed until it has been looked for.
            ndi_available: true,
            inspector: crate::inspector::InspectorState::default(),
            theme_edit: None,
            theme_names: Vec::new(),
            themes_kept: vec![0],
            template_names: Vec::new(),
            song_templates: Vec::new(),
            pending_template: None,
            renaming_template: None,
            confirm_delete_template: None,
            theme_layouts: Vec::new(),
            bible_layout: 0,
            theme_for_songs: 0,
            theme_for_bible: 0,
            theme_request: None,
            edit_kind_request: None,
            bible: BiblePage::default(),
        }
    }
}

impl HomeState {
    /// Which kind of the Edit view is open, if it is: the one answer to "is the editor open".
    pub fn edit_kind(&self) -> Option<EditKind> {
        if self.theme_edit.is_some() {
            Some(EditKind::Themes)
        } else if let Some(draft) = self.editing.as_ref() {
            Some(if draft.at.presentation == crate::props::PROP_SLOT { EditKind::Props } else { EditKind::Slides })
        } else {
            self.stage_editor_open.then_some(EditKind::Stage)
        }
    }

    /// True for a presentation the app makes itself (a Bible passage, the VoxLive item): shown,
    /// never edited, moved or deleted.
    pub fn app_made(&self, presentation: usize) -> bool {
        self.bible.slots.is_some_and(|slots| slots.contains(&presentation)) || self.voxlive_slot == Some(presentation)
    }

    /// The presentations kept out of the song lists: the Bible page's two and the VoxLive item
    /// (which has its own row under LIBRARY).
    pub fn hidden_slots(&self) -> [usize; 3] {
        let [first, second] = self.bible.slots.unwrap_or([usize::MAX; 2]);
        [first, second, self.voxlive_slot.unwrap_or(usize::MAX)]
    }

    /// True while a question asking whether to delete something is open: then no key reaches
    /// the show (Space, the arrows and Escape answer it or wait).
    pub fn alert_open(&self) -> bool {
        !self.confirm_delete_songs.is_empty()
            || !self.confirm_delete_media.is_empty()
            || !self.confirm_delete_slides.is_empty()
            || self.confirm_delete_theme.is_some()
            || self.confirm_delete_template.is_some()
    }

    /// The small choices kept between runs, as they are now.
    pub fn prefs(&self) -> crate::service::Prefs {
        crate::service::Prefs {
            dissolve: self.transition.kind != sp_model::TransitionKind::Cut,
            dissolve_time: self.transition.seconds,
            transition: Some(self.transition),
            media_transition: Some(self.media_transition),
            columns: self.columns,
            slide_view: match self.slide_view {
                SlideView::Grid => "grid",
                SlideView::Easy => "easy",
                SlideView::Table => "table",
            }
            .to_owned(),

            continuous: self.continuous,
            copyright_show: self.copyright_show,
            ccli_licence: self.ccli_licence.clone(),
            start_audience_off: self.start_audience_off,
            start_with: self.start_with.clone(),
            stage_off: !self.stage_on,
            setup_done: self.setup_done,
            bible_lists_hidden: !self.bible.lists_open,
            bible_rows_open: self.bible.rows_open,
            playlist_in_front: self.playlist_open,
            song_order: self.song_order.name().to_owned(),
            ..crate::service::Prefs::default()
        }
    }

    /// Sets the small choices as they were saved.
    pub fn apply_prefs(&mut self, prefs: &crate::service::Prefs) {
        self.transition = prefs.transition.unwrap_or_else(|| {
            if prefs.dissolve {
                sp_model::Transition { seconds: prefs.dissolve_time.clamp(0.1, 5.0), ..sp_model::Transition::default() }
            } else {
                sp_model::Transition::of(sp_model::TransitionKind::Cut)
            }
        });
        self.media_transition = prefs.media_transition.unwrap_or_default();
        self.columns = prefs.columns.clamp(2, 7);
        self.slide_view = match prefs.slide_view.as_str() {
            "easy" => SlideView::Easy,
            "table" => SlideView::Table,
            _ => SlideView::Grid,
        };

        self.continuous = prefs.continuous;
        self.copyright_show = prefs.copyright_show;
        self.ccli_licence = prefs.ccli_licence.clone();
        self.start_audience_off = prefs.start_audience_off;
        self.start_with = prefs.start_with.clone();
        self.stage_on = !prefs.stage_off;
        self.setup_done = prefs.setup_done;
        self.bible.lists_open = !prefs.bible_lists_hidden;
        self.bible.rows_open = prefs.bible_rows_open;
        self.playlist_open = prefs.playlist_in_front;
        self.song_order = crate::kept::SongOrder::named(&prefs.song_order);
    }

    /// How long a new picture takes to fade in on the output; zero when slides cut.
    pub fn dissolve_seconds(&self) -> f32 {
        self.transition.duration()
    }
}

pub fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Black or white, whichever reads better on the colour.
fn text_on(c: Rgba) -> Color32 {
    let luminance = 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b);
    if luminance > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// True when a title matches what was typed in the filter box (case-insensitive, any part).
pub fn matches_filter(title: &str, filter: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty() || title.to_lowercase().contains(&filter.to_lowercase())
}

/// True when this frame has a press of `key` with Command (and Shift, exactly when `shift`).
/// It is judged by the key press itself, not by the keys held now, so a shortcut chosen from
/// the menu bar, which arrives with no key held, counts as well.
fn command_key(input: &egui::InputState, key: egui::Key, shift: bool) -> bool {
    input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed_key, pressed: true, modifiers, .. }
            if *pressed_key == key && modifiers.command && modifiers.shift == shift)
    })
}

fn panel_frame(fill: Color32, margin: i8) -> Frame {
    Frame::new().fill(fill).inner_margin(margin)
}

/// Draws the home window for one frame and returns the commands the operator issued.
///
/// `thumb` returns the thumbnail of a slide or a media item and whether its text overflows, or
/// `None` while the thumbnail is still being made. It is only called for what is on screen.
/// `preview` is the picture of the audience screen and `canvas` the picture of the slide being
/// edited.
pub fn home_ui(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    preview: Preview,
    canvas: Option<TextureId>,
) -> Vec<Command> {
    let mut commands = Vec::new();
    // The lists made from the songs are made again only once the songs have changed.
    let hidden = state.hidden_slots();
    state.lists.songs.check(show, &state.song_keys, hidden, &state.removed_songs);
    // Text boxes are dark wells in the lighter side columns.
    ui.visuals_mut().extreme_bg_color = FIELD;
    crate::studio::toolbar(ui, show, state, &mut commands);
    crate::studio::changed_outside_bar(ui, show, state);
    crate::setup::setup_ui(ui.ctx(), show, state, &mut commands);
    if ui.input(|input| command_key(input, egui::Key::Comma, false)) {
        state.preferences_open = true;
    }
    // Command-F searches the library (on the Mac the menu bar's Find does this).
    if ui.input(|input| command_key(input, egui::Key::F, false)) {
        state.library_search = Some((String::new(), 0));
    }
    // Command-N makes a new song, wherever the keyboard is not in a field.
    if !ui.ctx().text_edit_focused() && ui.input(|input| command_key(input, egui::Key::N, false)) {
        state.new_song = true;
    }
    // New Song, from anywhere, opens the sheet that asks for the song's title, theme and words.
    if std::mem::take(&mut state.new_song) {
        let taken = |name: &str| show.presentations().iter().any(|song| song.title == name);
        let title = std::iter::once("New Song".to_owned())
            .chain((2..).map(|n| format!("New Song {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        state.new_song_sheet = Some(NewSong { title, focus: true, ..NewSong::default() });
    }
    new_song_sheet(ui, state);
    // Bibles finished downloading go on the shelf whether or not the Bibles window is open.
    {
        let bible = &mut state.bible;
        crate::bible::collect_downloads(&mut bible.options, &mut bible.shelf, &mut bible.downloads);
        if bible.options.manage_open {
            crate::bible::manage_bibles(ui.ctx(), &mut bible.options, &mut bible.shelf, &mut bible.downloads);
        }
        if let Some(index) = bible.options.remove.take() {
            bible.remove_bible(index);
        }
    }
    confirm_delete_theme(ui, state);
    confirm_delete_song(ui, show, state);
    confirm_delete_media(ui, show, state);
    confirm_delete_slides(ui, show, state);
    // Delete (or Backspace) deletes what is picked out in the list last clicked, after asking;
    // with neither list last clicked, the slide picked out by a right-click.
    let delete_key = ui.input(|input| input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace));
    if delete_key
        && state.edit_kind().is_none()
        && !ui.ctx().text_edit_focused()
        && !state.alert_open()
        && !egui::Popup::is_any_open(ui.ctx())
    {
        match state.picking {
            Picking::Songs => {
                let picked: Vec<usize> =
                    if state.picked_songs.is_empty() { vec![show.selected()] } else { state.picked_songs.clone() };
                let hidden = state.hidden_slots();
                state.confirm_delete_songs = picked
                    .into_iter()
                    .filter(|at| {
                        *at < show.presentations().len() && !hidden.contains(at) && !state.removed_songs.contains(at)
                    })
                    .collect();
            }
            Picking::Media => {
                state.confirm_delete_media =
                    state.picked_media.iter().copied().filter(|at| *at < show.media().len()).collect();
            }
            Picking::Slides => {
                let from_bible = |at: &SlideRef| state.app_made(at.presentation);
                state.confirm_delete_slides =
                    state.picked_slides.iter().copied().filter(|at| !from_bible(at)).collect();
            }
            Picking::Nothing => {
                let from_bible = |at: SlideRef| state.app_made(at.presentation);
                state.confirm_delete_slides = state.selected_slide.filter(|at| !from_bible(*at)).into_iter().collect();
            }
            // Themes are deleted in the Theme Editor, where the list is.
            Picking::Themes => {}
        }
    }
    library_search(ui, show, state, &mut commands);
    if state.editing.is_some() {
        edit_view(ui, show, state, theme, thumb, canvas, preview, &mut commands);
        return commands;
    }
    if state.edit_kind() == Some(EditKind::Stage) {
        stage_view(ui, show, state, preview, &mut commands);
        return commands;
    }
    crate::studio::library(ui, show, state, &mut commands);
    crate::studio::right_column(ui, show, state, preview, &mut commands, |ui, state| {
        crate::live_tabs::live_tabs(ui, show, state);
    });
    // The middle column is what the side columns leave: the slides (or the Bible page) with
    // the media bin at its foot.
    let middle = crate::studio::column_panel(ui.available_rect_before_wrap(), crate::studio::Column::Middle);
    crate::studio::media_strip(ui, show, state, thumb, &mut commands);
    if state.bible_open {
        bible_area(ui, show, state, thumb, &mut commands);
    } else {
        slide_area(ui, show, state, theme, thumb, &mut commands);
    }
    crate::studio::round_corners(ui, middle);
    commands
}

const EDIT_PANEL: Color32 = crate::studio::PANEL;
const EDIT_HEADER: Color32 = Color32::from_rgb(44, 44, 46);
const EDIT_SURROUND: Color32 = Color32::from_rgb(54, 54, 56);
const EDIT_STATUS: Color32 = crate::studio::PANEL;
const EDIT_SELECTED: Color32 = crate::studio::SELECTED;
const EDIT_OUTLINE: Color32 = Color32::from_rgb(230, 230, 234);
/// Half the side of a resize handle, in screen points.
const HANDLE: f32 = 4.5;
/// How far from a handle's middle a press still grabs it, in screen points: more than the
/// handle drawn, so it is easy to catch.
const HANDLE_REACH: f32 = 10.0;

/// The Edit view. It takes the place of the left and middle columns: the song's slides down the
/// left with the chosen slide's objects under them, and a canvas in the middle where objects
/// are selected, dragged and resized. The right column stays, with the inspector for the
/// selected object under Clear All, where the tool tabs are in the other views.
#[allow(clippy::too_many_arguments)]
fn edit_view(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    canvas: Option<TextureId>,
    preview: Preview,
    commands: &mut Vec<Command>,
) {
    let Some(at) = state.editing.as_ref().map(|draft| draft.at) else { return };
    // The draft is a song's slide, or one of a theme's layouts when the Theme Editor is open.
    let theme_mode = state.theme_edit.is_some();
    let presentation = show.presentations().get(at.presentation);
    let prop_mode = at.presentation == crate::props::PROP_SLOT;
    if presentation.is_none() && !theme_mode && !prop_mode {
        state.editing = None;
        return;
    }
    // Another slide opened (or the same one again): the old draft's own steps went with it.
    if let Some(draft) = state.editing.as_ref()
        && draft.serial != state.edit_steps_draft
    {
        if drop_slide_steps(&mut state.edit_steps) {
            state.forget_song_history = Some(draft.at.presentation);
        }
        state.edit_redo.clear();
        state.edit_steps_draft = draft.serial;
    }
    let depth_before = state.editing.as_ref().map(EditDraft::undo_depth);
    let before = state.editing.as_ref().map(|draft| draft.slide.clone());
    let now = ui.input(|input| input.time);
    let typing = ui.ctx().text_edit_focused();

    // Keys: undo and redo always; delete and nudging when not typing in a box.
    let (undo, redo, delete, nudge) = ui.input(|input| {
        let undo = command_key(input, egui::Key::Z, false);
        let redo = command_key(input, egui::Key::Z, true);
        let delete = !typing && (input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace));
        let step = if input.modifiers.shift { 10.0 } else { 1.0 };
        let mut nudge = (0.0, 0.0);
        if !typing {
            for (key, by) in [
                (egui::Key::ArrowLeft, (-step, 0.0)),
                (egui::Key::ArrowRight, (step, 0.0)),
                (egui::Key::ArrowUp, (0.0, -step)),
                (egui::Key::ArrowDown, (0.0, step)),
            ] {
                if input.key_pressed(key) {
                    nudge = (nudge.0 + by.0, nudge.1 + by.1);
                }
            }
        }
        (undo, redo, delete, nudge)
    });
    // Copy, paste, duplicate and select all act on objects unless a field is being typed in.
    if !typing {
        let (copy, cut, pasted, duplicate, all) = ui.input(|input| {
            let has = |wanted: fn(&egui::Event) -> bool| input.events.iter().any(wanted);
            let pasted = input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            });
            (
                has(|event| matches!(event, egui::Event::Copy)),
                has(|event| matches!(event, egui::Event::Cut)),
                pasted,
                command_key(input, egui::Key::D, false),
                command_key(input, egui::Key::A, false),
            )
        });
        if let Some(draft) = state.editing.as_mut() {
            if (copy || cut || duplicate) && draft.selected.is_some() {
                let copies = draft.copied(theme);
                if duplicate {
                    draft.paste(&copies);
                } else {
                    // The words also go to the system clipboard, so they can be pasted
                    // elsewhere, and so a paste here can tell these are the objects meant.
                    let words: Vec<&str> = copies.iter().filter_map(sp_model::Object::as_text).collect();
                    state.object_clip_text = words.join("\n");
                    ui.ctx().copy_text(state.object_clip_text.clone());
                    state.object_clip = copies;
                    if cut {
                        draft.delete_selected();
                    }
                }
            } else if let Some(text) = pasted.as_ref()
                && !state.object_clip.is_empty()
                && *text == state.object_clip_text
            {
                draft.paste(&state.object_clip);
                // Pasting again lands further on, not on top of the last copy.
                state.object_clip = draft.copied(theme);
            } else if copy && draft.selected.is_none() && !theme_mode && !prop_mode {
                // With no object selected, Copy takes the slides picked out in the list, or the
                // slide itself, as it now is.
                let mut picked: Vec<usize> = state
                    .picked_slides
                    .iter()
                    .filter(|p| p.presentation == draft.at.presentation)
                    .map(|p| p.slide)
                    .collect();
                picked.sort_unstable();
                let song = show.presentations().get(draft.at.presentation).cloned();
                match song {
                    Some(song) if picked.len() > 1 => copy_slides(ui.ctx(), state, &song, &picked),
                    _ => {
                        let slide = draft.slide.clone();
                        copy_these_slides(ui.ctx(), state, vec![slide]);
                    }
                }
            } else if let Some(text) = pasted.as_ref()
                && !state.slide_clip.is_empty()
                && *text == state.slide_clip_text
                && !theme_mode
                && !prop_mode
            {
                // Slides copied go just after the one being edited.
                state.slide_request = Some((draft.at, SlideRequest::PasteAfter(state.slide_clip.clone())));
            } else if all {
                draft.select_all();
            }
        }
    }
    // In the editor (a song's or a theme's), a letter adds an object: T a text box, and each
    // shape its own key.
    if !typing && !ui.ctx().text_edit_focused() && state.editing.is_some() && state.renaming_template.is_none() {
        let pressed = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Text(text) if !input.modifiers.command && !input.modifiers.ctrl => {
                    text.chars().next().map(|c| c.to_ascii_uppercase())
                }
                _ => None,
            })
        });
        if let (Some(key), Some(draft)) = (pressed, state.editing.as_mut()) {
            if key == 'T' {
                draft.add_text();
            } else if let Some(at) = SHAPE_KEYS.iter().position(|have| *have == key) {
                draft.add_shape(sp_model::Shape::ALL[at]);
            }
        }
    }
    // A picture or video chosen for the slide (or the theme's layout), put on it now so the
    // change is saved like any other.
    let pictures = std::mem::take(&mut state.picture_to_add);
    if let Some(draft) = state.editing.as_mut() {
        for picture in &pictures {
            draft.add_image(picture);
        }
    }
    // A change asked for from the menu bar.
    if let (Some(request), Some(draft)) = (state.edit_request.take(), state.editing.as_mut()) {
        match request {
            EditRequest::AddText => draft.add_text(),
            EditRequest::AddShape(shape) => draft.add_shape(shape),
            EditRequest::Bold => {
                let bold = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.weight >= 600);
                draft.restyle(theme, |look| look.weight = if bold { 400 } else { 700 });
            }
            EditRequest::Italic => {
                let italic = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.italic);
                draft.restyle(theme, |look| look.italic = !italic);
            }
            EditRequest::Underline => {
                let underline = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.underline);
                draft.restyle(theme, |look| look.underline = !underline);
            }
            EditRequest::Bigger => {
                draft.restyle(theme, |look| look.size = (look.size * 1.1).round().min(1000.0));
            }
            EditRequest::Smaller => {
                draft.restyle(theme, |look| look.size = (look.size / 1.1).round().max(8.0));
            }
            EditRequest::Align(align) => {
                draft.restyle(theme, |look| look.align = align);
            }
            EditRequest::UpperCase => {
                draft.retext(str::to_uppercase);
            }
            EditRequest::LowerCase => {
                draft.retext(str::to_lowercase);
            }
            EditRequest::Capitalize => {
                draft.retext(capitalize);
            }
            EditRequest::CopyStyle => {
                state.inspector.copied_style = draft.selected_object().and_then(|object| object.text_style().cloned());
            }
            EditRequest::PasteStyle => {
                if let Some(copied) = state.inspector.copied_style.clone() {
                    let ids = draft.selection();
                    for object in draft.slide.objects.iter_mut().filter(|object| ids.contains(&object.id)) {
                        if let Some(style) = object.text_style_mut() {
                            *style = copied.clone();
                        }
                    }
                }
            }
            EditRequest::BringForward => {
                draft.restack(true);
            }
            EditRequest::SendBackward => {
                draft.restack(false);
            }
            EditRequest::BringToFront => {
                draft.move_to_end_of_stack(true);
            }
            EditRequest::SendToBack => {
                draft.move_to_end_of_stack(false);
            }
            EditRequest::AlignObjects(edge) => {
                draft.align_to_slide(theme, edge);
            }
            EditRequest::Flip(across) => {
                draft.flip(across);
            }
            EditRequest::Lock => {
                draft.lock(true);
            }
            EditRequest::UnlockAll => {
                draft.lock(false);
            }
            EditRequest::DeselectAll => draft.select_only(None),
        }
    }
    // Whether a box was selected when Delete was pressed: deleting it leaves nothing selected,
    // and the same key press must not then go on to delete the template behind it.
    let had_selection = state.editing.as_ref().is_some_and(|draft| draft.selected.is_some());
    // Delete goes to what was clicked last: with a theme picked in the Theme Editor's list, it
    // asks about deleting that theme (never one of the two built in), not a box or template.
    let delete_theme = delete
        && theme_mode
        && state.picking == Picking::Themes
        && state.confirm_delete_theme.is_none()
        && state.renaming_theme.is_none();
    if delete_theme
        && let Some(open) = state.theme_edit.as_ref().map(|edit| edit.index)
        && !state.themes_kept.contains(&open)
    {
        state.confirm_delete_theme = Some(open);
    }
    if let Some(draft) = state.editing.as_mut() {
        // Undo and Redo take the newest step, whichever it is: a change to this slide (its
        // draft's own) or to the song's slides (moved, pasted, deleted: the app's).
        if undo && !typing {
            if state.edit_steps.last() == Some(&EditStep::Song) {
                state.edit_steps.pop();
                state.edit_redo.push(EditStep::Song);
                state.song_undo = Some(false);
            } else if draft.undo() {
                if state.edit_steps.last() == Some(&EditStep::Slide) {
                    state.edit_steps.pop();
                }
                state.edit_redo.push(EditStep::Slide);
            }
        } else if redo && !typing {
            if state.edit_redo.last() == Some(&EditStep::Song) {
                state.edit_redo.pop();
                state.edit_steps.push(EditStep::Song);
                state.song_undo = Some(true);
            } else if draft.redo() {
                if state.edit_redo.last() == Some(&EditStep::Slide) {
                    state.edit_redo.pop();
                }
                state.edit_steps.push(EditStep::Slide);
            }
        } else if delete && !delete_theme {
            draft.delete_selected();
        } else if nudge != (0.0, 0.0) && draft.selected.is_some() {
            let snapshot = draft.slide.clone();
            draft.nudge(theme, nudge);
            draft.record_run(snapshot, now);
        }
    }

    // Delete with no box selected, in the Theme Editor, is for the template itself.
    let delete_template_key = delete
        && theme_mode
        && !delete_theme
        && !had_selection
        && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none())
        && state.renaming_template.is_none()
        && state.confirm_delete_template.is_none()
        && state.confirm_delete_theme.is_none();
    // Left: the slide navigator, with the slide's objects below it.
    let mut chosen = None;
    // With no object selected and nothing being typed, ↑ and ↓ open the slide before or after.
    if !theme_mode
        && !prop_mode
        && !typing
        && let Some(draft) = state.editing.as_ref().filter(|draft| draft.selected.is_none())
    {
        let (up, down) =
            ui.input(|input| (input.key_pressed(egui::Key::ArrowUp), input.key_pressed(egui::Key::ArrowDown)));
        let count = show.presentations().get(draft.at.presentation).map_or(0, |song| song.slide_count());
        let at = draft.at;
        let to = if up && at.slide > 0 {
            Some(at.slide - 1)
        } else if down && at.slide + 1 < count {
            Some(at.slide + 1)
        } else {
            None
        };
        if let Some(slide) = to {
            chosen = Some(SlideRef { slide, ..at });
            state.picked_slides.clear();
            state.edit_list_scroll = true;
        }
    }
    let mut chosen_template = None;
    let mut open_prop = None;
    // The left column, as wide here as in every other view.
    use crate::studio::Column;
    let left = egui::Panel::left("edit slides")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(crate::studio::column_outer(crate::studio::LEFT_WIDTH))
        .frame(crate::studio::column_frame(Column::Left));
    let left = left.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        crate::studio::edit_kinds(ui, state);
        ui.add_space(10.0);
        if theme_mode {
            theme_list(ui, state);
        } else if prop_mode {
            open_prop = prop_list(ui, state);
        } else if let Some(presentation) = presentation {
            Frame::new().fill(EDIT_HEADER).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(presentation.title.as_str()).strong().size(12.5).color(Color32::WHITE),
                    );
                    ui.add_space(ui.available_width());
                });
            });
        }

        egui::Panel::bottom("edit objects").resizable(false).exact_size(200.0).frame(panel_frame(EDIT_PANEL, 0)).show(
            ui,
            |ui| {
                Frame::new().fill(EDIT_HEADER).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Objects").strong().size(12.5).color(Color32::WHITE));
                        ui.add_space(ui.available_width());
                    });
                });
                egui::ScrollArea::vertical().id_salt("edit object list").auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(4.0);
                    let Some(draft) = state.editing.as_mut() else { return };
                    // The top of the stack is listed first, as it is in front on the slide.
                    let mut pick = None;
                    // What a row's icons or its right-click menu asked for.
                    enum Ask {
                        Hide(bool),
                        Lock(bool),
                        Delete,
                    }
                    let mut asked: Option<(sp_model::Uuid, Ask)> = None;
                    // A rename that has just begun takes the keyboard once; one that has
                    // ended is carried out after the list is drawn.
                    let mut rename_started = std::mem::take(&mut state.rename_started);
                    let mut rename_done: Option<Option<(sp_model::Uuid, String)>> = None;
                    let mut start_rename = None;
                    let extend = ui.input(|input| input.modifiers.shift || input.modifiers.command);
                    // Rows can be dragged up and down: the order is front to back.
                    let mut rows: Vec<(sp_model::Uuid, Rect)> = Vec::new();
                    let mut dragging = None;
                    let mut dropped = None;
                    for object in draft.slide.objects.iter().rev() {
                        let selected = draft.is_selected(object.id);
                        let (row, response) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
                        rows.push((object.id, row));
                        if response.dragged() {
                            dragging = Some(object.id);
                        }
                        if response.drag_stopped() {
                            dropped = Some(object.id);
                        }
                        if selected {
                            ui.painter().rect_filled(row, 0.0, EDIT_SELECTED);
                        } else if response.hovered() {
                            ui.painter().rect_filled(row, 0.0, Color32::from_gray(46));
                        }
                        // A hidden object's row is greyed.
                        let ink = if object.hidden { Color32::from_gray(120) } else { Color32::from_gray(230) };
                        // T for text, a diamond for a shape, a globe for a web page.
                        let mark = match object.kind {
                            sp_model::ObjectKind::Text { .. } => "T",
                            sp_model::ObjectKind::Image { .. } => "▣",
                            sp_model::ObjectKind::Shape { .. } => "◆",
                            sp_model::ObjectKind::Web { .. } => "🌐",
                        };
                        ui.painter().text(
                            row.left_center() + vec2(12.0, 0.0),
                            Align2::LEFT_CENTER,
                            mark,
                            FontId::proportional(if mark == "T" { 15.0 } else { 12.0 }),
                            ink,
                        );
                        let name_area = Rect::from_min_max(row.min, pos2(row.max.x - 46.0, row.max.y));
                        // The name, or a field in its place while it is being retyped.
                        let being_renamed = draft.renaming.as_ref().is_some_and(|(id, _)| *id == object.id);
                        if being_renamed && let Some((_, name)) = draft.renaming.as_mut() {
                            let field = crate::studio::rename_field_at(
                                ui,
                                name,
                                FontId::proportional(13.0),
                                crate::studio::TEXT,
                                crate::studio::ACCENT,
                                row.left_center() + vec2(32.0, 0.0),
                                row.max.x - 46.0,
                            );
                            if std::mem::take(&mut rename_started) {
                                field.request_focus();
                            }
                            let (enter, escape) = ui.input(|input| {
                                (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape))
                            });
                            if escape {
                                rename_done = Some(None);
                            } else if enter || field.lost_focus() {
                                rename_done = Some(Some((object.id, name.clone())));
                            }
                        } else {
                            let shown = if theme_mode { object.name.clone() } else { listed_name(object) };
                            ui.painter().with_clip_rect(name_area).text(
                                row.left_center() + vec2(32.0, 0.0),
                                Align2::LEFT_CENTER,
                                shown,
                                FontId::proportional(13.0),
                                ink,
                            );
                        }
                        // Hide and lock sit at the right of the row under the pointer, the
                        // selected row, and any row that is already hidden or locked.
                        let marks = [
                            (object.hidden, "Hide", "Show", pos2(row.max.x - 34.0, row.center().y)),
                            (object.locked, "Lock", "Unlock", pos2(row.max.x - 14.0, row.center().y)),
                        ];
                        for (index, (on, to_set, to_clear, centre)) in marks.into_iter().enumerate() {
                            if !(on || selected || response.hovered()) {
                                continue;
                            }
                            let spot = Rect::from_center_size(centre, vec2(18.0, 18.0));
                            let mark = ui.interact(
                                spot,
                                ui.scope_id().with(("object mark", object.id, index)),
                                Sense::click(),
                            );
                            let mark_ink = if on { Color32::from_rgb(240, 190, 40) } else { Color32::from_gray(170) };
                            let p = ui.painter();
                            if index == 0 {
                                // An eye; struck through when the object is hidden.
                                p.circle_stroke(centre, 5.0, Stroke::new(1.2, mark_ink));
                                p.circle_filled(centre, 2.0, mark_ink);
                                if on {
                                    p.line_segment(
                                        [centre + vec2(-6.0, 6.0), centre + vec2(6.0, -6.0)],
                                        Stroke::new(1.4, mark_ink),
                                    );
                                }
                            } else {
                                // A padlock; its shackle is open when unlocked.
                                p.rect_filled(
                                    Rect::from_center_size(centre + vec2(0.0, 2.5), vec2(9.0, 7.0)),
                                    1.5,
                                    mark_ink,
                                );
                                let lift = if on { 0.0 } else { -2.0 };
                                p.line_segment(
                                    [centre + vec2(-2.5, -1.0), centre + vec2(-2.5, -4.5 + lift)],
                                    Stroke::new(1.3, mark_ink),
                                );
                                p.line_segment(
                                    [centre + vec2(-2.5, -4.5 + lift), centre + vec2(2.5, -4.5 + lift)],
                                    Stroke::new(1.3, mark_ink),
                                );
                                if on {
                                    p.line_segment(
                                        [centre + vec2(2.5, -1.0), centre + vec2(2.5, -4.5)],
                                        Stroke::new(1.3, mark_ink),
                                    );
                                }
                            }
                            if mark.on_hover_text(if on { to_clear } else { to_set }).clicked() {
                                asked = Some((object.id, if index == 0 { Ask::Hide(!on) } else { Ask::Lock(!on) }));
                            }
                        }
                        // A click selects; a click on the name of the one already selected,
                        // or a double click, starts renaming it, as in a file list.
                        let on_name = response
                            .interact_pointer_pos()
                            .is_some_and(|at| name_area.contains(at) && at.x > row.min.x + 26.0);
                        if response.double_clicked()
                            || (response.clicked() && !extend && selected && draft.also.is_empty() && on_name)
                        {
                            start_rename = Some((object.id, object.name.clone()));
                        } else if response.clicked() {
                            pick = Some(object.id);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                start_rename = Some((object.id, object.name.clone()));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(if object.hidden { "Show" } else { "Hide" }).clicked() {
                                asked = Some((object.id, Ask::Hide(!object.hidden)));
                                ui.close();
                            }
                            if ui.button(if object.locked { "Unlock" } else { "Lock" }).clicked() {
                                asked = Some((object.id, Ask::Lock(!object.locked)));
                                ui.close();
                            }
                            if ui.button("Delete").clicked() {
                                asked = Some((object.id, Ask::Delete));
                                ui.close();
                            }
                        });
                    }
                    // Where a dragged row would land: a line between the rows, and on
                    // release the object moves there in the stack.
                    let pointer_y = ui.input(|input| input.pointer.interact_pos()).map(|at| at.y);
                    let slot = pointer_y.map(|y| rows.iter().filter(|(_, row)| row.center().y < y).count());
                    if let (Some(_), Some(slot)) = (dragging, slot) {
                        let y = rows
                            .get(slot)
                            .map_or_else(|| rows.last().map_or(0.0, |(_, row)| row.max.y), |(_, row)| row.min.y);
                        let (left, right) = rows.first().map_or((0.0, 0.0), |(_, row)| (row.min.x, row.max.x));
                        ui.painter().line_segment(
                            [pos2(left + 6.0, y), pos2(right - 6.0, y)],
                            Stroke::new(2.0, crate::studio::ACCENT),
                        );
                    }
                    if let (Some(id), Some(slot)) = (dropped, slot) {
                        draft.move_in_stack(id, slot);
                    }
                    if let Some((id, ask)) = asked {
                        if matches!(ask, Ask::Delete) {
                            draft.selected = Some(id);
                            draft.delete_selected();
                        } else if let Some(object) = draft.slide.objects.iter_mut().find(|o| o.id == id) {
                            match ask {
                                Ask::Hide(hidden) => object.hidden = hidden,
                                Ask::Lock(locked) => object.locked = locked,
                                Ask::Delete => {}
                            }
                        }
                    }
                    if draft.slide.objects.is_empty() {
                        ui.label(egui::RichText::new("   This slide is empty").size(12.0).color(DIM));
                    }
                    if let Some(id) = pick {
                        if extend {
                            draft.toggle_selected(id);
                        } else {
                            draft.select_only(Some(id));
                        }
                    }
                    match rename_done {
                        Some(Some((id, name))) => {
                            if !name.trim().is_empty()
                                && let Some(object) = draft.slide.objects.iter_mut().find(|o| o.id == id)
                            {
                                object.name = name.trim().to_owned();
                            }
                            draft.renaming = None;
                        }
                        Some(None) => draft.renaming = None,
                        None => {}
                    }
                    if let Some((id, name)) = start_rename {
                        draft.select_only(Some(id));
                        draft.renaming = Some((id, name));
                        rename_started = true;
                    }
                    state.rename_started = rename_started;
                });
            },
        );

        if let Some(edit) = state.theme_edit.as_ref() {
            // The theme's templates, each drawn as it will look. Tags say what each is for:
            // a song's slide uses, of the ones for songs, the one that suits what is on it;
            // Bible slides use the one for Bible slides.
            let marks: Vec<(bool, bool)> = edit.theme.templates.iter().map(|t| (t.songs, t.bible)).collect();
            let mut link_bible = None;
            let mut toggle_songs = None;
            let layouts: Vec<(usize, String)> =
                edit.theme.templates.iter().map(|template| template.name.clone()).enumerate().collect();
            let only_one = layouts.len() <= 1;
            let mut renamed_template: Option<Option<(usize, String)>> = None;
            // Asked for from a template's menu, or the tile after the last: a copy of
            // a template, or a template to delete.
            let mut add_after = None;
            let mut ask_delete = None;
            let mut rename_started = std::mem::take(&mut state.rename_started);
            let mut start_rename = None;
            let slides: Vec<Slide> = edit.theme.templates.iter().map(sp_model::ThemeTemplate::as_slide).collect();
            let current = edit.template;
            egui::ScrollArea::vertical().id_salt("theme layout list").auto_shrink([false, false]).show(ui, |ui| {
                let width = ui.available_width();
                let image_size = vec2(width - 16.0, (width - 16.0) * 9.0 / 16.0);
                for (index, ((template, name), slide)) in layouts.into_iter().zip(&slides).enumerate() {
                    let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 30.0), Sense::click());
                    let selected = template == current;
                    ui.painter().rect_filled(cell, 0.0, if selected { EDIT_SELECTED } else { Color32::from_gray(52) });
                    let image = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                    ui.painter().rect_filled(image, 0.0, Color32::BLACK);
                    if let Some(texture) = canvas.filter(|_| selected) {
                        // The layout on the canvas is shown as it is this instant.
                        egui::Image::from_texture((texture, image_size)).paint_at(ui, image);
                    } else {
                        match thumb(ThumbOf::Layout(index, slide)) {
                            Some((texture, _)) => egui::Image::from_texture((texture, image_size)).paint_at(ui, image),
                            None => ui.ctx().request_repaint(),
                        }
                    }
                    if selected {
                        ui.painter().rect_stroke(image, 0.0, Stroke::new(2.0, EDIT_OUTLINE), StrokeKind::Outside);
                    }
                    // The template's number and name. A click on the name of the template
                    // on the canvas, or a double click, turns the name into a field.
                    let ink = Color32::from_gray(230);
                    let number = ui.painter().text(
                        pos2(cell.min.x + 9.0, image.max.y + 10.0),
                        Align2::LEFT_CENTER,
                        format!("{}.", index + 1),
                        FontId::proportional(11.5),
                        ink,
                    );
                    let name_area = Rect::from_min_max(
                        pos2(number.max.x + 2.0, image.max.y + 1.0),
                        pos2(cell.max.x - 96.0, cell.max.y - 1.0),
                    );
                    let being_renamed = state.renaming_template.as_ref().is_some_and(|(which, _)| *which == template);
                    if being_renamed && let Some((_, typed)) = state.renaming_template.as_mut() {
                        let field = crate::studio::rename_field_at(
                            ui,
                            typed,
                            FontId::proportional(11.5),
                            ink,
                            ink,
                            pos2(number.max.x + 6.0, image.max.y + 10.0),
                            name_area.max.x,
                        );
                        if std::mem::take(&mut rename_started) {
                            field.request_focus();
                        }
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                        if escape {
                            renamed_template = Some(None);
                        } else if enter || field.lost_focus() {
                            renamed_template = Some(Some((template, typed.clone())));
                        }
                    } else {
                        ui.painter().with_clip_rect(name_area).text(
                            pos2(number.max.x + 6.0, image.max.y + 10.0),
                            Align2::LEFT_CENTER,
                            name.as_str(),
                            FontId::proportional(11.5),
                            ink,
                        );
                        let on_name = response.interact_pointer_pos().is_some_and(|at| name_area.contains(at));
                        if response.double_clicked() || (response.clicked() && selected && on_name) {
                            start_rename = Some((template, name.clone()));
                        }
                    }
                    // What is linked to this template, at the right of its name.
                    let mut right = cell.max.x - 8.0;
                    let (for_songs, for_bible) = marks.get(template).copied().unwrap_or_default();
                    for (linked, tag) in [(for_bible, "Bible"), (for_songs, "Songs")] {
                        if !linked {
                            continue;
                        }
                        let ink = Color32::from_gray(225);
                        let text = ui.painter().layout_no_wrap(tag.to_owned(), FontId::proportional(10.0), ink);
                        let pill = Rect::from_min_size(
                            pos2(right - text.size().x - 10.0, image.max.y + 2.0),
                            vec2(text.size().x + 10.0, 16.0),
                        );
                        ui.painter().rect_filled(pill, 8.0, Color32::from_white_alpha(32));
                        ui.painter().galley(pos2(pill.min.x + 5.0, pill.center().y - text.size().y / 2.0), text, ink);
                        right = pill.min.x - 4.0;
                    }
                    if response.clicked() && !selected {
                        chosen_template = Some(template);
                    }
                    response.context_menu(|ui| {
                        // What the template is for: songs pick among the templates for songs;
                        // Bible slides use one template.
                        let mut songs = for_songs;
                        if ui
                            .checkbox(&mut songs, "Use for Songs")
                            .on_hover_text("Songs pick, among these, the one that suits each slide")
                            .clicked()
                        {
                            toggle_songs = Some(template);
                            ui.close();
                        }
                        if ui
                            .add_enabled(!for_bible, egui::Button::new("Use for Bible Slides"))
                            .on_hover_text("Bible slides are drawn with this template, in this theme")
                            .clicked()
                        {
                            link_bible = Some(template);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Rename").clicked() {
                            start_rename = Some((template, name.clone()));
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            add_after = Some(template);
                            ui.close();
                        }
                        if ui.add_enabled(!only_one, egui::Button::new("Delete…")).clicked() {
                            ask_delete = Some(template);
                            ui.close();
                        }
                    });
                }
                // After the last template: a blank tile with a plus. It adds a template
                // that starts as a copy of the one being edited.
                let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 16.0), Sense::click());
                let tile = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                let line = if response.hovered() { crate::studio::ACCENT } else { Color32::from_gray(90) };
                ui.painter().rect_filled(tile, 4.0, Color32::from_gray(if response.hovered() { 34 } else { 24 }));
                ui.painter().rect_stroke(tile, 4.0, Stroke::new(1.0, line), StrokeKind::Inside);
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(tile.center(), vec2(18.0, 18.0)), line);
                if response.on_hover_text("Add a template. It starts as a copy of the one being edited.").clicked() {
                    add_after = Some(current);
                }
            });
            // Delete, with no box selected, deletes the template being edited (once confirmed).
            if delete_template_key && !only_one {
                ask_delete = Some(current);
            }
            if let Some(template) = ask_delete {
                state.confirm_delete_template = Some(template);
            }
            if let (Some(after), Some(edit)) = (add_after, state.theme_edit.as_mut()) {
                let at = edit.theme.add_template(after);
                edit.changed = true;
                chosen_template = Some(at);
                // Its name is ready to be typed over.
                start_rename = Some((at, edit.theme.templates[at].name.clone()));
            }
            if let Some(edit) = state.theme_edit.as_mut() {
                if let Some(template) = link_bible {
                    edit.theme.set_bible_template(template);
                    edit.changed = true;
                }
                if let Some(toggled) = toggle_songs.and_then(|at| edit.theme.templates.get_mut(at)) {
                    toggled.songs = !toggled.songs;
                    edit.changed = true;
                }
            }
            if let Some(start) = start_rename {
                state.renaming_template = Some(start);
                rename_started = true;
            }
            state.rename_started = rename_started;
            if let Some(outcome) = renamed_template {
                state.renaming_template = None;
                if let (Some((template, name)), Some(edit)) = (outcome, state.theme_edit.as_mut()) {
                    let name = name.trim();
                    if let Some(renamed) = edit.theme.templates.get_mut(template)
                        && !name.is_empty()
                        && renamed.name != name
                    {
                        renamed.name = name.to_owned();
                        edit.changed = true;
                    }
                }
            }
        } else {
            egui::ScrollArea::vertical().id_salt("edit slide list").auto_shrink([false, false]).show(ui, |ui| {
                let width = ui.available_width();
                let image_size = vec2(width - 16.0, (width - 16.0) * 9.0 / 16.0);
                let mut index = 0;
                let groups = presentation.map(|p| p.groups.as_slice()).unwrap_or_default();
                let movable = !state.app_made(at.presentation);
                let moving = egui::DragAndDrop::payload::<SlideDrag>(ui.ctx())
                    .filter(|drag| movable && drag.presentation == at.presentation);
                scroll_while_dragging(ui);
                for (group_index, group) in groups.iter().enumerate() {
                    let (row, bar_response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
                    let bar = Rect::from_min_max(row.min + vec2(8.0, 6.0), row.max - vec2(8.0, 2.0));
                    ui.painter().rect_filled(bar, 4.0, color32(group.color).gamma_multiply(0.8));
                    // Slides dropped on a group's name go at its start.
                    let first_here = index;
                    if movable
                        && let Some(dropped) = bar_response
                            .dnd_release_payload::<SlideDrag>()
                            .filter(|drag| drag.presentation == at.presentation && !drag.slides.contains(&first_here))
                    {
                        state.slide_request = Some((
                            SlideRef { presentation: at.presentation, slide: dropped.slides[0] },
                            SlideRequest::Move { slides: dropped.slides.clone(), to: first_here, after: false },
                        ));
                    }
                    if movable && bar_response.dnd_hover_payload::<SlideDrag>().is_some() {
                        ui.painter().rect_stroke(bar, 4.0, Stroke::new(2.0, DROP_LINE), StrokeKind::Outside);
                    }
                    let ink = text_on(group.color);
                    icons::paint(
                        ui.painter(),
                        Icon::ChevronDown,
                        Rect::from_center_size(bar.left_center() + vec2(9.0, 0.0), vec2(8.0, 8.0)),
                        ink,
                    );
                    // The group's name; a click turns it into a field to retype it.
                    let renaming = state.renaming_group.as_ref().is_some_and(|(at, _)| *at == group_index);
                    if renaming && let Some((_, typed)) = state.renaming_group.as_mut() {
                        let field = crate::studio::rename_field_at(
                            ui,
                            typed,
                            FontId::proportional(12.5),
                            ink,
                            ink,
                            bar.left_center() + vec2(18.0, 0.0),
                            bar.max.x - 4.0,
                        );
                        if std::mem::take(&mut state.rename_started) {
                            field.request_focus();
                        }
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                        if escape {
                            state.renaming_group = None;
                        } else if enter || field.lost_focus() {
                            if typed.trim() != group.name {
                                state.pending_group_name =
                                    Some((at.presentation, group_index, typed.trim().to_owned()));
                            }
                            state.renaming_group = None;
                        }
                    } else {
                        let name = if group.name.is_empty() { "No group" } else { group.name.as_str() };
                        ui.painter().text(
                            bar.left_center() + vec2(18.0, 0.0),
                            Align2::LEFT_CENTER,
                            name,
                            FontId::proportional(12.5),
                            ink,
                        );
                        if bar_response.on_hover_text("Click to rename this group").clicked() {
                            state.renaming_group = Some((group_index, group.name.clone()));
                            state.rename_started = true;
                        }
                    }

                    // Each slide a card, as in the Show view: its picture, and under it a bar in
                    // the group's colour with its number (and the group's name on its first
                    // slide), apart from the next. A card dragged goes to another place.
                    for (in_group, slide) in group.slides.iter().enumerate() {
                        let here = SlideRef { presentation: at.presentation, slide: index };
                        let (cell, response) = ui.allocate_exact_size(
                            vec2(width, image_size.y + EDIT_CARD_BAR + EDIT_CARD_GAP),
                            Sense::click_and_drag(),
                        );
                        let card = Rect::from_min_size(
                            cell.min + vec2(8.0, EDIT_CARD_GAP / 2.0),
                            vec2(image_size.x, image_size.y + EDIT_CARD_BAR),
                        );
                        let selected = here == at;
                        let round = SLIDE_ROUND;
                        if ui.is_rect_visible(cell) {
                            let image = Rect::from_min_size(card.min, image_size);
                            let top = egui::CornerRadius { nw: round, ne: round, sw: 0, se: 0 };
                            ui.painter().rect_filled(image, top, Color32::BLACK);
                            let of = if movable { ThumbOf::Slide(slide) } else { ThumbOf::BibleSlide(slide) };
                            match thumb(of) {
                                Some((texture, _)) => egui::Image::from_texture((texture, image_size))
                                    .corner_radius(top)
                                    .paint_at(ui, image),
                                None => ui.ctx().request_repaint(),
                            }
                            let bar = Rect::from_min_max(pos2(card.min.x, image.max.y), card.max);
                            let bottom = egui::CornerRadius { nw: 0, ne: 0, sw: round, se: round };
                            ui.painter().rect_filled(bar, bottom, color32(group.color));
                            let ink = text_on(group.color);
                            let words = match (in_group, slide.label.as_deref()) {
                                (_, Some(label)) => format!("{}  {label}", index + 1),
                                (0, None) if !group.name.is_empty() => format!("{}  {}", index + 1, group.name),
                                _ => (index + 1).to_string(),
                            };
                            ui.painter().with_clip_rect(bar.shrink(2.0)).text(
                                bar.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                words,
                                crate::studio::semibold(12.0),
                                ink,
                            );
                            // A hairline round it, so a black slide does not melt into the column.
                            ui.painter().rect_stroke(
                                card,
                                f32::from(round),
                                Stroke::new(1.0, crate::studio::LINE),
                                StrokeKind::Outside,
                            );
                            let outline = if selected {
                                Some(Stroke::new(2.0, EDIT_OUTLINE))
                            } else if state.picked_slides.contains(&here) {
                                Some(Stroke::new(2.0, SELECTED_OUTLINE))
                            } else if response.hovered() {
                                Some(Stroke::new(1.0, crate::studio::TEXT_FAINT))
                            } else {
                                None
                            };
                            if let Some(stroke) = outline {
                                ui.painter().rect_stroke(card, f32::from(round) + 1.0, stroke, StrokeKind::Outside);
                            }
                            if moving.as_ref().is_some_and(|drag| drag.slides.contains(&index)) {
                                ui.painter().rect_filled(card, f32::from(round), Color32::from_black_alpha(150));
                            }
                        }
                        if selected && std::mem::take(&mut state.edit_list_scroll) {
                            response.scroll_to_me(Some(Align::Center));
                        }
                        if movable {
                            // Dragged, it goes with the slides picked out with it.
                            if response.dragged() {
                                let mut picked: Vec<usize> = state
                                    .picked_slides
                                    .iter()
                                    .filter(|p| p.presentation == at.presentation)
                                    .map(|p| p.slide)
                                    .collect();
                                picked.sort_unstable();
                                let slides = if picked.contains(&index) { picked } else { vec![index] };
                                response.dnd_set_drag_payload(SlideDrag { presentation: at.presentation, slides });
                            }
                            let target = |drag: &SlideDrag| {
                                drag.presentation == at.presentation && !drag.slides.contains(&index)
                            };
                            if response.dnd_hover_payload::<SlideDrag>().is_some_and(|drag| target(&drag)) {
                                paint_drop_line(ui, card, drop_after(ui, card, true), true, EDIT_CARD_GAP);
                            }
                            if let Some(dropped) =
                                response.dnd_release_payload::<SlideDrag>().filter(|drag| target(drag))
                            {
                                let after = drop_after(ui, card, true);
                                state.slide_request = Some((
                                    SlideRef { presentation: at.presentation, slide: dropped.slides[0] },
                                    SlideRequest::Move { slides: dropped.slides.clone(), to: index, after },
                                ));
                            }
                            // Or a step at a time, from its menu.
                            let count = presentation.map_or(0, |song| song.slide_count());
                            response.context_menu(|ui| {
                                if ui.add_enabled(index > 0, egui::Button::new("Move Up")).clicked() {
                                    state.slide_request = Some((
                                        here,
                                        SlideRequest::Move { slides: vec![index], to: index - 1, after: false },
                                    ));
                                    ui.close();
                                }
                                if ui.add_enabled(index + 1 < count, egui::Button::new("Move Down")).clicked() {
                                    state.slide_request = Some((
                                        here,
                                        SlideRequest::Move { slides: vec![index], to: index + 1, after: true },
                                    ));
                                    ui.close();
                                }
                            });
                        }
                        if response.clicked() {
                            let mods = ui.input(|input| input.modifiers);
                            if (mods.command || mods.shift) && movable {
                                // Picked out, to move, copy or delete several; the slide open
                                // stays open, and is picked out with them.
                                let total = presentation.map_or(0, |song| song.slide_count());
                                let order: Vec<SlideRef> =
                                    (0..total).map(|slide| SlideRef { presentation: at.presentation, slide }).collect();
                                state.picked_slides.retain(|p| p.presentation == at.presentation);
                                if state.picked_slides.is_empty() {
                                    state.picked_slides.push(at);
                                }
                                pick_slide(&mut state.picked_slides, &order, here, mods, Some(at));
                            } else {
                                state.picked_slides.clear();
                                if !selected {
                                    chosen = Some(here);
                                }
                            }
                        }
                        index += 1;
                    }
                }
                // One of the slides being dragged follows the pointer.
                if let Some(drag) = &moving {
                    let first =
                        drag.slides.first().and_then(|at| presentation.and_then(|p| p.slide(*at))).map(|(_, s)| s);
                    let texture = first.and_then(|slide| thumb(ThumbOf::Slide(slide)));
                    paint_drag_ghost(ui, texture.map(|(texture, _)| texture), drag.slides.len());
                }
                // After the last slide: a blank tile with a plus. It adds a slide laid out
                // like the one before it.
                if !groups.is_empty() || presentation.is_some() {
                    let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 22.0), Sense::click());
                    let tile = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                    let hovered = response.hovered();
                    let line = if hovered { crate::studio::ACCENT } else { Color32::from_gray(90) };
                    ui.painter().rect_filled(tile, 4.0, Color32::from_gray(if hovered { 34 } else { 24 }));
                    ui.painter().rect_stroke(tile, 4.0, Stroke::new(1.0, line), StrokeKind::Inside);
                    icons::paint(
                        ui.painter(),
                        Icon::Plus,
                        Rect::from_center_size(tile.center(), vec2(18.0, 18.0)),
                        line,
                    );
                    if response.on_hover_text("Add a slide, laid out like the one before it").clicked() {
                        state.add_slide = Some(at.presentation);
                    }
                }
            });
        }
    });

    crate::studio::round_corners(ui, crate::studio::column_panel(left.response.rect, Column::Left));

    // Right: the column every view has, with the inspector where the tool tabs are in the others.
    let mut leave = false;
    crate::studio::right_column(ui, show, state, preview, commands, |ui, state| {
        leave = crate::inspector::inspector(ui, state, theme);
    });

    // Middle: the canvas.
    let middle = crate::studio::column_panel(ui.available_rect_before_wrap(), Column::Middle);
    let surround = panel_frame(EDIT_SURROUND, 0).outer_margin(crate::studio::column_margin(Column::Middle));
    egui::CentralPanel::default().frame(surround).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::Panel::bottom("edit status").resizable(false).frame(panel_frame(EDIT_STATUS, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let frame = state
                    .editing
                    .as_ref()
                    .and_then(|draft| draft.selected_object().map(|object| object_frame(&draft.slide, object, theme)));
                for (name, value) in [
                    ("X", frame.map(|f| f.x)),
                    ("Y", frame.map(|f| f.y)),
                    ("W", frame.map(|f| f.w)),
                    ("H", frame.map(|f| f.h)),
                ] {
                    let text = value.map_or_else(|| format!("{name}: ----"), |v| format!("{name}: {v:.0}"));
                    ui.label(egui::RichText::new(text).size(11.5).color(Color32::from_gray(200)));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    // How large the slide is shown: fitted to the window, or a size of its own.
                    let shown = if state.edit_zoom <= 0.0 {
                        "Fit".to_owned()
                    } else {
                        format!("{:.0}%", state.edit_zoom * 100.0)
                    };
                    ui.menu_button(egui::RichText::new(shown).size(11.5), |ui| {
                        if ui.add(egui::Button::selectable(state.edit_zoom <= 0.0, "Fit")).clicked() {
                            state.edit_zoom = 0.0;
                            state.edit_pan = egui::Vec2::ZERO;
                            ui.close();
                        }
                        for zoom in [0.25, 0.5, 0.75, 1.0, 1.5, 2.0] {
                            let on = (state.edit_zoom - zoom).abs() < 0.001;
                            if ui.add(egui::Button::selectable(on, format!("{:.0}%", zoom * 100.0))).clicked() {
                                state.edit_zoom = zoom;
                                state.edit_pan = egui::Vec2::ZERO;
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text("How large the slide is shown. Pinch to zoom; scroll to move a zoomed slide.");
                    if ui
                        .add(egui::Button::selectable(state.edit_grid, egui::RichText::new("Grid").size(11.5)))
                        .on_hover_text("Show a grid that boxes snap to")
                        .clicked()
                    {
                        state.edit_grid = !state.edit_grid;
                    }
                });
            });
        });
        let area = ui.available_rect_before_wrap();
        let ruler = 16.0;
        let inner = Rect::from_min_max(area.min + vec2(ruler, ruler), area.max).shrink(28.0);
        // Fitted, the slide fills the room under the tools; zoomed, it is its own size, moved by
        // scrolling. A pinch zooms from wherever it is.
        let room = Rect::from_min_max(pos2(inner.min.x, inner.min.y + 44.0), inner.max);
        let fit_width = inner.width().min(room.height() * 16.0 / 9.0).max(1.0);
        let pixels = ui.ctx().pixels_per_point();
        let canvas_area = Rect::from_min_max(area.min + vec2(ruler, ruler), area.max);
        if ui.rect_contains_pointer(canvas_area) {
            let (scroll, pinch) = ui.input(|input| (input.smooth_scroll_delta, input.zoom_delta()));
            if (pinch - 1.0).abs() > 0.001 {
                let now =
                    if state.edit_zoom > 0.0 { state.edit_zoom } else { fit_width * pixels / sp_model::SLIDE_WIDTH };
                state.edit_zoom = (now * pinch).clamp(0.1, 4.0);
            } else if state.edit_zoom > 0.0 && scroll != egui::Vec2::ZERO {
                state.edit_pan += scroll;
            }
        }
        let width = if state.edit_zoom > 0.0 { sp_model::SLIDE_WIDTH * state.edit_zoom / pixels } else { fit_width };
        let fitted_centre = pos2(inner.center().x, room.min.y + fit_width * 9.0 / 32.0);
        if state.edit_zoom <= 0.0 {
            state.edit_pan = egui::Vec2::ZERO;
        }
        // A zoomed slide can be moved only so far that some of it stays in view.
        let reach = vec2(width / 2.0 + inner.width() / 2.0 - 60.0, width * 9.0 / 32.0 + inner.height() / 2.0 - 60.0)
            .max(egui::Vec2::ZERO);
        state.edit_pan = state.edit_pan.clamp(-reach, reach);
        let slide = Rect::from_center_size(fitted_centre + state.edit_pan, vec2(width, width * 9.0 / 16.0));
        let scale = slide.width() / sp_model::SLIDE_WIDTH;
        let selected_frame = state
            .editing
            .as_ref()
            .and_then(|draft| draft.selected_object().map(|object| object_frame(&draft.slide, object, theme)));
        rulers(ui.painter(), area, ruler, slide, scale, selected_frame);
        // Everything after this stays inside the canvas, clear of the rulers.
        ui.set_clip_rect(canvas_area);
        ui.painter().rect_filled(slide.expand(1.0), 0.0, Color32::from_black_alpha(90));
        ui.painter().rect_filled(slide, 0.0, Color32::BLACK);
        if let Some(texture) = canvas {
            egui::Image::from_texture((texture, slide.size())).paint_at(ui, slide);
        }
        // The grid, faint over the slide.
        if state.edit_grid {
            let line = Stroke::new(1.0, Color32::from_white_alpha(28));
            let mut x = crate::editor::GRID;
            while x < sp_model::SLIDE_WIDTH {
                let at = slide.min.x + x * scale;
                ui.painter().line_segment([pos2(at, slide.min.y), pos2(at, slide.max.y)], line);
                x += crate::editor::GRID;
            }
            let mut y = crate::editor::GRID;
            while y < sp_model::SLIDE_HEIGHT {
                let at = slide.min.y + y * scale;
                ui.painter().line_segment([pos2(slide.min.x, at), pos2(slide.max.x, at)], line);
                y += crate::editor::GRID;
            }
        }
        if let Some(draft) = state.editing.as_mut() {
            draft.grid = state.edit_grid;
        }

        // Pointer on the canvas: select, move, resize. Positions are turned into slide units.
        // The safe edge: a faint frame a little inside the slide. Boxes snap to it, so text can
        // be kept an even distance from the edge of the screen.
        let (safe_x, safe_y) = crate::editor::SAFE_EDGE;
        let safe = slide.shrink2(vec2(safe_x * scale, safe_y * scale));
        ui.painter().rect_stroke(safe, 0.0, Stroke::new(1.0, Color32::from_white_alpha(38)), StrokeKind::Inside);
        let to_slide = |p: egui::Pos2| ((p.x - slide.min.x) / scale, (p.y - slide.min.y) / scale);
        let to_screen = |f: SlideRect| Rect::from_min_size(slide.min + vec2(f.x, f.y) * scale, vec2(f.w, f.h) * scale);
        let response = ui.interact(area, ui.scope_id().with("canvas"), Sense::click_and_drag());
        let for_theme = state.theme_edit.is_some();
        // A file dragged over the editor is put on the slide when dropped.
        if state.drop_hover {
            let painter = ui.painter();
            painter.rect_filled(slide, 0.0, crate::studio::ACCENT.gamma_multiply(0.15));
            painter.rect_stroke(slide.shrink(4.0), 6.0, Stroke::new(2.0, crate::studio::ACCENT), StrokeKind::Inside);
            let words = if for_theme { "Drop to put it on the layout" } else { "Drop to put it on the slide" };
            painter.text(slide.center(), Align2::CENTER_CENTER, words, crate::studio::semibold(16.0), crate::studio::TEXT);
        }
        // A press on the slide makes Delete and the other keys act on its boxes again.
        if response.drag_started() || response.clicked() || response.secondary_clicked() {
            state.picking = Picking::Nothing;
        }
        if let Some(draft) = state.editing.as_mut() {
            // A press anywhere on the canvas ends typing; the field itself is in front of the
            // canvas, so presses inside it never reach here.
            if response.drag_started() || response.clicked() {
                draft.typing = None;
            }
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = to_slide(pointer);
                // Shift or Command adds to the selection; while dragging, Command lets a box
                // move freely, without being pulled onto the guide lines.
                let (extend, free) =
                    ui.input(|input| (input.modifiers.shift || input.modifiers.command, input.modifiers.command));
                if response.drag_started() || response.clicked() {
                    // Where the button went down, not where the pointer is once the drag has
                    // begun a few points further on: on a small handle the difference decides
                    // between resizing the box and moving it.
                    let pressed = ui.input(|input| input.pointer.press_origin()).map_or(point, to_slide);
                    draft.press(theme, pressed, HANDLE_REACH / scale, extend);
                }
                // A double click on a text box starts typing its words right there. A theme's
                // boxes only ever say "Text", so there is nothing to type in them.
                if response.double_clicked() && !for_theme && draft.selected.is_some() && draft.also.is_empty() {
                    draft.typing = draft.selected;
                    draft.typing_started = true;
                }
                if response.dragged() {
                    let keep_shape = ui.input(|input| input.modifiers.shift);
                    draft.drag_to(point, keep_shape, !free);
                }
                // A right-click on an object selects it (keeping the selection when it is in
                // it), so what the menu does is done to what was clicked, not to the slide or
                // template behind it.
                if response.secondary_clicked() {
                    let hit = crate::editor::object_at(&draft.slide, theme, point);
                    if !hit.is_some_and(|id| draft.is_selected(id)) {
                        draft.select_only(hit);
                    }
                }
            }
            let any = draft.selected.is_some();
            let mut asked = None;
            response.context_menu(|ui| {
                use crate::menubar::Action as A;
                let mut item = |ui: &mut egui::Ui, label: &str, action: A| {
                    if ui.button(label).clicked() {
                        asked = Some(action);
                        ui.close();
                    }
                };
                if any {
                    item(ui, "Cut", A::Cut);
                    item(ui, "Copy", A::Copy);
                    item(ui, "Paste", A::Paste);
                    item(ui, "Duplicate", A::Duplicate);
                    item(ui, "Delete", A::Delete);
                    ui.separator();
                    item(ui, "Bring to Front", A::BringToFront);
                    item(ui, "Bring Forward", A::BringForward);
                    item(ui, "Send Backward", A::SendBackward);
                    item(ui, "Send to Back", A::SendToBack);
                    ui.separator();
                    ui.menu_button("Align to Slide", |ui| {
                        for (label, edge) in
                            [("Left", 0), ("Centre", 1), ("Right", 2), ("Top", 3), ("Middle", 4), ("Bottom", 5)]
                        {
                            item(ui, label, A::AlignObjects(edge));
                        }
                    });
                    item(ui, "Copy Style", A::CopyStyle);
                    item(ui, "Paste Style", A::PasteStyle);
                    item(ui, "Lock", A::Lock);
                } else {
                    item(ui, "Paste", A::Paste);
                    item(ui, "Select All", A::SelectAll);
                    item(ui, "Add Text", A::InsertText);
                }
            });
            if asked.is_some() {
                state.menu_asked = asked;
            }
            if response.drag_stopped() || response.clicked() {
                draft.release();
            }

            // Outlines: every object faintly, the selected one with handles.
            for object in &draft.slide.objects {
                let frame = object_frame(&draft.slide, object, theme);
                let rect = to_screen(frame);
                let selected = draft.is_selected(object.id);
                let centre = rect.center();
                let (sin, cos) = object.rotation.to_radians().sin_cos();
                let corners: Vec<egui::Pos2> =
                    [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
                        .into_iter()
                        .map(|c| {
                            let d = c - centre;
                            centre + vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
                        })
                        .collect();
                let stroke = if selected {
                    Stroke::new(1.5, CHOSEN_LINE)
                } else {
                    Stroke::new(1.0, Color32::from_white_alpha(45))
                };
                ui.painter().add(egui::Shape::closed_line(corners, stroke));
                if selected && draft.also.is_empty() && object.rotation == 0.0 && draft.typing != Some(object.id) {
                    for handle in crate::editor::Handle::ALL {
                        let (x, y) = handle.position(frame);
                        let at = slide.min + vec2(x, y) * scale;
                        let knob = Rect::from_center_size(at, vec2(HANDLE * 2.0, HANDLE * 2.0));
                        ui.painter().rect_filled(knob, 1.5, Color32::WHITE);
                        ui.painter().rect_stroke(knob, 1.5, Stroke::new(1.0, CHOSEN_LINE), StrokeKind::Inside);
                    }
                }
            }
        }

        // The rectangle being dragged out to select with.
        if let Some(((ax, ay), (bx, by))) = state.editing.as_ref().and_then(|draft| draft.marquee) {
            let rect = Rect::from_two_pos(slide.min + vec2(ax, ay) * scale, slide.min + vec2(bx, by) * scale);
            ui.painter().rect_filled(rect, 0.0, crate::studio::ACCENT.gamma_multiply(0.15));
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, crate::studio::ACCENT), StrokeKind::Inside);
        }

        // The lines a dragged box is sitting on: the slide's middle or edges, or another box's.
        if let Some(draft) = state.editing.as_ref() {
            let pen = Stroke::new(1.0, Color32::from_rgb(255, 64, 196));
            for guide in &draft.guides {
                match guide {
                    crate::editor::Guide::Down(x) => {
                        let x = slide.min.x + x * scale;
                        ui.painter().line_segment([pos2(x, slide.min.y), pos2(x, slide.max.y)], pen);
                    }
                    crate::editor::Guide::Across(y) => {
                        let y = slide.min.y + y * scale;
                        ui.painter().line_segment([pos2(slide.min.x, y), pos2(slide.max.x, y)], pen);
                    }
                }
            }
        }

        // Typing on the slide: a field over the box being edited, in the box's own place.
        if let Some(draft) = state.editing.as_mut()
            && let Some(id) = draft.typing
        {
            let started = std::mem::take(&mut draft.typing_started);
            // Only a text object can be typed in.
            let placed = draft.slide.objects.iter().find(|o| o.id == id).and_then(|object| {
                let look = theme.text_style(object.role.as_ref(), object.text_style()?);
                Some((to_screen(object_frame(&draft.slide, object, theme)), look))
            });
            let object = draft.slide.objects.iter_mut().find(|o| o.id == id);
            match (placed, object) {
                (Some((rect, look)), Some(sp_model::Object { kind: sp_model::ObjectKind::Text { text, .. }, .. })) => {
                    // The box is covered while it is typed in, so only the words being typed
                    // are read; the slide shows them in its own look as soon as typing ends.
                    ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(8, 9, 11));
                    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.5, crate::studio::ACCENT), StrokeKind::Outside);
                    let size = look.map_or(20.0, |look| (look.size * scale).clamp(13.0, 34.0));
                    let field = egui::TextEdit::multiline(text)
                        .frame(Frame::NONE)
                        .font(FontId::proportional(size))
                        .align(Align2::CENTER_TOP)
                        .desired_width(rect.width() - 16.0);
                    let field = ui.put(rect.shrink(8.0), field);
                    if started {
                        field.request_focus();
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        draft.typing = None;
                    }
                }
                // The object went away while it was being typed in.
                _ => draft.typing = None,
            }
        }

        // The object tools, the same for a song's slide and a theme's layout: a text box, the
        // shapes, and pictures and videos.
        let offset = -104.0;
        // A picture from a file.
        let picture_bar =
            Rect::from_center_size(pos2(inner.center().x + 104.0, inner.min.y + 22.0), vec2(96.0, 28.0));
        let picture = ui.interact(picture_bar, ui.scope_id().with("add picture"), Sense::click());
        crate::studio::paint_button(ui.painter(), picture_bar, picture.hovered());
        ui.painter().text(
            picture_bar.center(),
            Align2::CENTER_CENTER,
            "Media…",
            crate::studio::semibold(12.5),
            crate::studio::TEXT,
        );
        if picture
            .on_hover_text("Put a picture or video from a file on the slide (a video plays in its box when the slide is on screen)")
            .clicked()
        {
            state.picture_request = true;
        }
        // A live web page, in a box.
        let web_bar = Rect::from_center_size(pos2(inner.center().x + 208.0, inner.min.y + 22.0), vec2(96.0, 28.0));
        let web = ui.interact(web_bar, ui.scope_id().with("add web page"), Sense::click());
        crate::studio::paint_button(ui.painter(), web_bar, web.hovered());
        ui.painter().text(web_bar.center(), Align2::CENTER_CENTER, "Web Page", crate::studio::semibold(12.5), crate::studio::TEXT);
        if web
            .on_hover_text("Put a live web page on the slide, in a box (it shows while the slide is on screen)")
            .clicked()
            && let Some(draft) = state.editing.as_mut()
        {
            draft.add_web();
            // The inspector's address field takes the keyboard, ready for the address.
            ui.data_mut(|data| data.insert_temp(egui::Id::unique("web address focus"), true));
        }
        let shapes_bar = Rect::from_center_size(pos2(inner.center().x, inner.min.y + 22.0), vec2(96.0, 28.0));
        let shapes = ui.interact(shapes_bar, ui.scope_id().with("add shape"), Sense::click());
        crate::studio::paint_button(ui.painter(), shapes_bar, shapes.hovered());
        ui.painter().text(
            shapes_bar.center(),
            Align2::CENTER_CENTER,
            "Shapes ▾",
            crate::studio::semibold(12.5),
            crate::studio::TEXT,
        );
        let shapes = shapes.on_hover_text("Add a shape. Each has a key: O R E G L H P S D A");
        egui::Popup::menu(&shapes).show(|ui| {
            for (shape, key) in sp_model::Shape::ALL.into_iter().zip(SHAPE_KEYS) {
                if ui.add(egui::Button::new(shape.name()).shortcut_text(key.to_string())).clicked()
                    && let Some(draft) = state.editing.as_mut()
                {
                    draft.add_shape(shape);
                }
            }
        });
        let bar = Rect::from_center_size(pos2(inner.center().x + offset, inner.min.y + 22.0), vec2(96.0, 28.0));
        let add = ui.interact(bar, ui.scope_id().with("add text"), Sense::click());
        crate::studio::paint_button(ui.painter(), bar, add.hovered());
        ui.painter().text(
            bar.center(),
            Align2::CENTER_CENTER,
            "Add Text",
            crate::studio::semibold(12.5),
            crate::studio::TEXT,
        );
        if add.on_hover_text("Add a text box to the slide").clicked()
            && let Some(draft) = state.editing.as_mut()
        {
            draft.add_text();
        }
    });
    crate::studio::round_corners(ui, middle);

    // Whatever changed this frame (a drag, typing, a button) is handed to the app, and typing
    // or dragging numbers is gathered into single undo steps.
    if let (Some(draft), Some(before)) = (state.editing.as_mut(), before)
        && draft.slide != before
    {
        if draft.drag.is_none() && !draft.stepped {
            draft.record_run(before, now);
        }
        match state.theme_edit.as_mut() {
            // A theme's layout: the theme is remade from it, and the app takes the theme.
            Some(edit) => {
                // A box added in the editor is given a role of its own once, so renaming it
                // later does not turn it into a different box.
                let mut taken: Vec<sp_model::Role> =
                    draft.slide.objects.iter().filter_map(|o| o.role.clone()).collect();
                if let Some(template) = edit.theme.templates.get_mut(edit.template) {
                    let is_text = |o: &sp_model::Object| matches!(o.kind, sp_model::ObjectKind::Text { .. });
                    for object in draft.slide.objects.iter_mut().filter(|o| o.role.is_none() && is_text(o)) {
                        let role = template.unused_role(&object.name, &taken);
                        taken.push(role.clone());
                        object.role = Some(role);
                    }
                    // A layout's shapes, pictures and videos are drawn behind the words on its
                    // slides, so the editor keeps them behind its text boxes too.
                    draft.slide.objects.sort_by_key(is_text);
                    *template = template.with_slide(&draft.slide);
                    edit.changed = true;
                }
            }
            // A prop: the prop takes the slide as it now is.
            None if draft.at.presentation == crate::props::PROP_SLOT => {
                if let Some(prop) = state.props.get_mut(draft.at.slide) {
                    prop.slide = draft.slide.clone();
                    state.props_changed = true;
                }
            }
            None => state.pending_edit = Some((draft.at, draft.slide.clone())),
        }
    }
    // A new step of this slide's own goes in the undo order; what could be redone goes.
    if !(undo || redo)
        && let (Some(draft), Some(before)) = (state.editing.as_ref(), depth_before)
        && draft.undo_depth() > before
    {
        for _ in before..draft.undo_depth() {
            state.edit_steps.push(EditStep::Slide);
        }
        state.edit_redo.clear();
        state.clear_song_redo = true;
    }
    if let Some(draft) = state.editing.as_mut() {
        draft.stepped = false;
    }
    if let Some(edit) = state.theme_edit.as_mut() {
        if leave {
            edit.leave = true;
        } else if let Some(template) = chosen_template
            && let Some(opened) = edit.theme.templates.get(template)
        {
            edit.template = template;
            state.editing = Some(EditDraft::of_slide(opened.as_slide(), template));
        }
    } else if leave {
        state.editing = None;
    } else if let Some(here) = chosen {
        state.editing = EditDraft::of(show, here);
    } else if let Some(prop) = open_prop
        && let Some(draft) = crate::studio::prop_draft(state, prop)
    {
        state.editing = Some(draft);
    }
}

/// The Edit view on the stage layouts: the layouts down the left under the switch, the picture
/// of the stage screen with its boxes in the middle, and the chosen box's settings in the right
/// column, where the inspector is for a slide.
fn stage_view(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, preview: Preview, commands: &mut Vec<Command>) {
    use crate::studio::Column;
    crate::stage_editor::prepare(state);
    let left = egui::Panel::left("edit stage layouts")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(crate::studio::column_outer(crate::studio::LEFT_WIDTH))
        .frame(crate::studio::column_frame(Column::Left));
    let left = left.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        crate::studio::edit_kinds(ui, state);
        let lists = Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 12 });
        lists.show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            crate::stage_editor::list(ui, state);
        });
    });
    crate::studio::round_corners(ui, crate::studio::column_panel(left.response.rect, Column::Left));
    crate::studio::right_column(ui, show, state, preview, commands, crate::stage_editor::item);
    let middle = crate::studio::column_panel(ui.available_rect_before_wrap(), Column::Middle);
    let frame = panel_frame(CANVAS, 16).outer_margin(crate::studio::column_margin(Column::Middle));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| crate::stage_editor::canvas(ui, state));
    crate::studio::round_corners(ui, middle);
}

/// The props, in the Edit view's left column while a prop is edited: a click opens another.
/// Returns the one clicked. (Props are made, renamed and deleted in the Props tab.)
fn prop_list(ui: &mut egui::Ui, state: &HomeState) -> Option<usize> {
    let open = state.editing.as_ref().map(|draft| draft.at.slide);
    let mut clicked = None;
    Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 4, bottom: 8 }).show(ui, |ui| {
        let (heading, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        crate::studio::paint_spaced(
            ui,
            heading.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            "PROPS",
            crate::studio::fixed_semibold(11.5),
            crate::studio::TEXT_DIM,
            0.6,
        );
        egui::ScrollArea::vertical().id_salt("edit props").max_height(170.0).show(ui, |ui| {
            for (index, prop) in state.props.iter().enumerate() {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let chosen = open == Some(index);
                if chosen {
                    ui.painter().rect_filled(row, crate::studio::RADIUS, crate::studio::CARD);
                } else if response.hovered() {
                    ui.painter().rect_filled(row, crate::studio::RADIUS, Color32::from_rgb(0x1f, 0x1f, 0x1f));
                }
                let (ink, font) = if chosen {
                    (crate::studio::TEXT, crate::studio::semibold(13.5))
                } else {
                    (crate::studio::TEXT_DIM, FontId::proportional(13.5))
                };
                let words = Rect::from_min_max(row.min + vec2(10.0, 0.0), row.max - vec2(10.0, 0.0));
                ui.painter().with_clip_rect(words).text(
                    words.left_center(),
                    Align2::LEFT_CENTER,
                    &prop.name,
                    font,
                    ink,
                );
                if response.clicked() && !chosen {
                    clicked = Some(index);
                }
            }
        });
    });
    clicked
}

/// What an output is called in the Screens window's menus.
fn output_label(output: &Output, displays: &[DisplayInfo]) -> String {
    match output {
        Output::Automatic => "Not chosen yet".to_owned(),
        Output::Display(name) => match displays.iter().find(|display| display.is_named(name)) {
            Some(display) if display.main => format!("{}   (main display)", display.name),
            Some(display) => format!("{}   {} x {}", display.name, display.w, display.h),
            None => format!("{name}   (disconnected)"),
        },
        Output::Window => "A window".to_owned(),
        Output::Ndi(_) => "NDI".to_owned(),
        Output::Off => "Off".to_owned(),
    }
}

/// The outputs a screen can be given: each connected display but the main one (where the
/// operator works), a window, NDI, off.
fn output_choices(current: &Output, screen_name: &str, displays: &[DisplayInfo]) -> Vec<Output> {
    let mut choices: Vec<Output> =
        displays.iter().filter(|display| !display.main).map(|display| Output::Display(display.name.clone())).collect();
    choices.push(Output::Window);
    // Choosing NDI again keeps the name already given.
    choices.push(match current {
        Output::Ndi(name) => Output::Ndi(name.clone()),
        _ => Output::Ndi(format!("SoftPresenter {screen_name}")),
    });
    choices.push(Output::Off);
    choices
}

/// A change the menu bar asks the editor to make to the selected objects. The editor makes it
/// during its own frame, so it can be undone and is saved like any other edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditRequest {
    Bold,
    Italic,
    Underline,
    Bigger,
    Smaller,
    Align(sp_model::Align),
    UpperCase,
    LowerCase,
    Capitalize,
    CopyStyle,
    PasteStyle,
    BringForward,
    BringToFront,
    SendBackward,
    SendToBack,
    Lock,
    UnlockAll,
    DeselectAll,
    AddText,
    AddShape(sp_model::Shape),
    /// Line the selection up with the slide: 0 left, 1 centre, 2 right, 3 top, 4 middle, 5 bottom.
    AlignObjects(u8),
    /// Mirror the selection: true left to right, false top to bottom.
    Flip(bool),
}

/// Each word with a capital first letter, the rest as they are.
fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.push(c);
            if c.is_whitespace() {
                start = true;
            } else if c.is_alphabetic() {
                start = false;
            }
        }
    }
    out
}

/// One step of the Edit view's undo order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditStep {
    /// A change to the slide being edited, which its draft undoes.
    Slide,
    /// A change to the song's slides (moved, pasted, deleted), which the app undoes.
    Song,
}

/// When the slide being edited is left (another opened, or the editor closed), its own steps
/// go with it. The song's steps stay, unless the slide was changed after one of them: undoing
/// that step would bring the song back as it was before, losing the change, so they go too.
/// True when they did, and the app should drop them.
pub fn drop_slide_steps(steps: &mut Vec<EditStep>) -> bool {
    let changed_after = steps
        .iter()
        .rposition(|step| *step == EditStep::Song)
        .is_some_and(|last| steps[last + 1..].contains(&EditStep::Slide));
    if changed_after {
        steps.clear();
    } else {
        steps.retain(|step| *step == EditStep::Song);
    }
    changed_after
}

/// A change to one slide asked for from its right-click menu. The app makes it.
#[derive(Clone, Debug, PartialEq)]
pub enum SlideRequest {
    /// Move these slides of the song, in their order, to just before the slide at `to` (or
    /// just after it, with `after`), into its group.
    Move {
        slides: Vec<usize>,
        to: usize,
        after: bool,
    },
    /// Delete these slides of the song at once (one step for Undo).
    DeleteMany(Vec<usize>),
    /// Put these slides, in order, just after this one.
    PasteAfter(Vec<Slide>),
    Group(String),
    Label(Option<String>),
}

/// A new song as the New Song sheet describes it: its title, its theme by its place (`None`
/// for the theme songs use), and its words in the song text format, if any were given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewSong {
    pub title: String,
    pub theme: Option<usize>,
    pub words: String,
    /// True until the title field has been given the keyboard.
    pub focus: bool,
}

/// The video on screen: its name, how far in it is, and how it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoLive {
    pub name: String,
    pub position: f64,
    pub duration: Option<f64>,
    pub playing: bool,
    pub looping: bool,
    pub muted: bool,
}

/// What the operator asked of the video on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoRequest {
    TogglePause,
    Restart,
    Looping(bool),
    Muted(bool),
    /// Go to this many seconds in.
    Seek(f64),
    /// The playhead being dragged to this many seconds in: followed live by a video the
    /// system plays; one played through FFmpeg waits for the [`VideoRequest::Seek`] at the end,
    /// since each jump restarts it.
    Scrub(f64),
}

/// Something asked of the audio player from the Audio tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioRequest {
    Add,
    Play(sp_model::Uuid),
    TogglePause,
    Stop,
    Skip(f32),
    Volume(f32),
    Remove(sp_model::Uuid),
}

/// A change to the media bin asked for: pictures to choose and add, or one to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaRequest {
    Choose,
    /// Take these items out of the bin (the files stay where they are).
    Delete(Vec<usize>),
    /// A slide showing this media item, added to the song shown after its chosen slide.
    AddAsSlide(usize),
    /// How an item fills the screen, and whether a video loops.
    Look(usize, sp_model::ImageFit, bool),
    /// Play a media playlist item after item on the media layer, or stop (`None`).
    PlayPlaylist(Option<sp_model::Uuid>),
    /// Choose a folder for a media playlist to follow, or stop following one (`false`).
    WatchFolder(sp_model::Uuid, bool),
}

/// A media bin item being dragged, by its place in the bin.
#[derive(Clone, Copy)]
pub struct MediaDrag(pub usize);

/// Slides being dragged to another place in their song: the song, and the slides' places.
#[derive(Clone, Debug, PartialEq)]
pub struct SlideDrag {
    pub presentation: usize,
    pub slides: Vec<usize>,
}

/// A slide that shows a media item when it goes on screen (or none), its words and the rest
/// kept: its Show Media action is the item.
pub fn slide_with_media(slide: &Slide, media: Option<sp_model::Uuid>) -> Slide {
    let mut actions: Vec<sp_model::ShowAction> =
        slide.actions.iter().filter(|action| !matches!(action, sp_model::ShowAction::Media { .. })).cloned().collect();
    actions.extend(media.map(|id| sp_model::ShowAction::Media { id }));
    Slide { actions, ..slide.clone() }
}

/// The media item a slide shows when it goes on screen, if it has one.
pub fn media_of(slide: &Slide) -> Option<sp_model::Uuid> {
    slide.actions.iter().rev().find_map(|action| match action {
        sp_model::ShowAction::Media { id } => Some(*id),
        _ => None,
    })
}

/// A change to a song asked for from its right-click menu in the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongRequest {
    Duplicate,
    Delete,
}

/// The group names a slide's Group menu offers.
const GROUP_NAMES: [&str; 19] = [
    "Verse 1",
    "Verse 2",
    "Verse 3",
    "Verse 4",
    "Verse 5",
    "Verse 6",
    "Chorus",
    "Chorus 2",
    "Chorus 3",
    "Chorus 4",
    "Bridge",
    "Bridge 2",
    "Bridge 3",
    "Pre-chorus",
    "Tag",
    "Intro",
    "Ending",
    "Outro",
    "Interlude",
];

/// The labels a slide's Label menu offers.
const SLIDE_LABELS: [&str; 4] = ["Blank Slide", "Instrumental", "Repeat", "Spoken Word"];

/// What an object is called in the Objects list: the name it was given, or, until it is given
/// one, the first line of its words.
fn listed_name(object: &sp_model::Object) -> String {
    let named = !(object.name.is_empty()
        || object.role.as_ref().is_some_and(|role| object.name == role.label())
        || object.name.strip_prefix("Text ").is_some_and(|number| number.parse::<u32>().is_ok()));
    let first_line = object.as_text().and_then(|text| text.lines().map(str::trim).find(|line| !line.is_empty()));
    match first_line {
        Some(line) if !named => line.to_owned(),
        _ => object.name.clone(),
    }
}

/// The Reflow editor: a song's words as text, to type into. `[Group]` starts a group, a blank
/// line starts a slide, and `--` comes before a translation. Done rebuilds the slides.
fn reflow_editor(ui: &mut egui::Ui, title: &str, state: &mut HomeState) {
    let Some((index, text)) = state.reflow.as_mut() else { return };
    let index = *index;
    let mut done = None;
    let field_id = ui.scope_id().with("reflow text");
    let heading = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 12, bottom: 8 });
    egui::Panel::top("reflow header").resizable(false).frame(heading).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Reflow  ·  {title}"))
                    .font(crate::studio::semibold(18.0))
                    .color(crate::studio::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Done").on_hover_text("Rebuild the slides from these words").clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
                ui.add_space(12.0);
                // Insert at the cursor: a slide break, or a group's heading.
                let cursor = egui::text_edit::TextEditState::load(ui.ctx(), field_id)
                    .and_then(|edit| edit.cursor.char_range())
                    .map_or(text.chars().count(), |range| range.primary.index.into());
                let mut insert = |what: &str| {
                    let at = text.char_indices().nth(cursor).map_or(text.len(), |(byte, _)| byte);
                    text.insert_str(at, what);
                };
                ui.menu_button("Insert Group", |ui| {
                    for name in GROUP_NAMES {
                        if ui.button(name).clicked() {
                            insert(&format!("\n[{name}]\n"));
                            ui.close();
                        }
                    }
                });
                if ui.button("Insert Slide Break").on_hover_text("Start a new slide here").clicked() {
                    insert("\n\n");
                }
            });
        });
        ui.label(
            egui::RichText::new(
                "[Group] starts a group  ·  a blank line starts a new slide  ·  -- comes before the translation",
            )
            .size(11.5)
            .color(crate::studio::TEXT_FAINT),
        );
    });
    egui::ScrollArea::vertical().id_salt("reflow").auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(egui::Margin::symmetric(20, 8)).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .id(field_id)
                    .font(FontId::proportional(16.0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .lock_focus(true),
            );
        });
    });
    match done {
        Some(true) => {
            state.reflow_done = state.reflow.take().map(|(_, text)| (index, text));
        }
        Some(false) => state.reflow = None,
        None => {}
    }
}

/// The songs a library search finds, best first: titles that hold the words, then songs whose
/// lyrics do, with the line that matched. Songs not listed in the library are left out.
fn search_songs(show: &Show, listed: impl Fn(usize) -> bool, query: &str) -> Vec<(usize, Option<String>)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut by_title = Vec::new();
    let mut by_words = Vec::new();
    for (index, song) in show.presentations().iter().enumerate().filter(|(index, _)| listed(*index)) {
        if song.title.to_lowercase().contains(&query) {
            by_title.push((index, None));
            continue;
        }
        let line = song
            .slides()
            .flat_map(|(_, slide)| slide.texts().into_iter().map(|(_, text)| text))
            .flat_map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
            .find(|line| line.to_lowercase().contains(&query));
        if let Some(line) = line {
            by_words.push((index, Some(line)));
        }
    }
    by_title.extend(by_words);
    by_title
}

/// How many songs the find box lists.
const SONGS_LISTED: usize = 6;

/// What the find box in the top bar finds for the words typed in it, listed under it: a Bible
/// passage first, when the words are a reference; then songs (titles first, then lines of their
/// words); then Bible verses that hold the words. Return, or a click, opens the one picked: a
/// song in the Show view, a passage or verse in the Bible view. Nothing goes on the screens.
/// With Command (Ctrl elsewhere), Return adds it to the playlist in front instead.
fn library_search(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let hidden = state.hidden_slots();
    // A Bible the search read for the first time stays read.
    for (id, bible) in state.finder.take_read() {
        state.bible.shelf.keep(&id, bible);
    }
    let Some((query, picked)) = state.library_search.as_mut() else { return };
    // Worked out again only when what is typed, or the songs, change: not on every frame the
    // list is open (the cursor blinking, the mouse moving).
    let epoch = state.lists.songs.epoch();
    let songs = match state.lists.search.get(|(made_in, typed)| *made_in == epoch && typed == query) {
        Some(found) => found,
        None => {
            let removed = &state.removed_songs;
            let found = search_songs(show, |index| !hidden.contains(&index) && !removed.contains(&index), query);
            state.lists.search.keep((epoch, query.clone()), found)
        }
    };
    let songs = &songs[..songs.len().min(SONGS_LISTED)];
    // The Bibles' part is found on a thread of its own and comes when it is ready; until then
    // the songs are listed without it.
    let typed = query.trim();
    let (shelf, rows) = (&state.bible.shelf, &state.bible.rows);
    let bible_rows = if typed.is_empty() {
        Some(&[][..])
    } else {
        let in_use: Vec<String> =
            rows.iter().filter_map(|row| Some(shelf.bibles.get(row.bible?)?.id.clone())).collect();
        let readers = || rows.iter().filter_map(|row| shelf.reader(row.bible?)).collect();
        state.finder.rows(typed, &in_use, readers, ui.ctx())
    };
    let answered = bible_rows.is_some();
    let bible_rows = bible_rows.unwrap_or(&[]);
    // A passage stands before the songs; verses found by their words after them.
    let (passage, verses) = match bible_rows.first() {
        Some(first) if first.passage => (bible_rows, &[][..]),
        _ => (&[][..], bible_rows),
    };
    let total = passage.len() + songs.len() + verses.len();
    *picked = (*picked).min(total.saturating_sub(1));
    let (up, down, enter, command, escape) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::Enter),
            input.modifiers.command,
            input.key_pressed(egui::Key::Escape),
        )
    });
    if up {
        *picked = picked.saturating_sub(1);
    }
    if down && *picked + 1 < total {
        *picked += 1;
    }
    let picked = *picked;
    // What a place in the list holds.
    enum Pick<'a> {
        Song(usize),
        Bible(&'a crate::find::BibleRow),
    }
    let at = |place: usize| -> Option<Pick<'_>> {
        if let Some(row) = passage.get(place) {
            return Some(Pick::Bible(row));
        }
        let place = place - passage.len();
        if let Some((index, _)) = songs.get(place) {
            return Some(Pick::Song(*index));
        }
        verses.get(place - songs.len()).map(Pick::Bible)
    };
    let mut clicked = None;
    // What is found is listed under the find box, where the words are typed (in the middle of
    // a window too narrow to have the box).
    let screen = ui.ctx().content_rect();
    let (under, wide) = state.find_rect.map_or((pos2(screen.center().x - 180.0, screen.min.y + 49.0), 360.0), |find| {
        (pos2(find.min.x, find.max.y + 6.0), find.width().max(360.0))
    });
    let mut list = None;
    if !typed.is_empty() {
        use crate::studio::{CARD, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT};
        let area = egui::Area::new(egui::Id::unique("library search"))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(under);
        let frame = Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0, LINE))
            .corner_radius(14)
            .inner_margin(6)
            .shadow(egui::Shadow { offset: [0, 12], blur: 30, spread: 0, color: Color32::from_black_alpha(140) });
        let shown = area.show(ui.ctx(), |ui| {
            frame.show(ui, |ui| {
                ui.set_width(wide - 12.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // One result: its name, what it is at the right, and a line of its words under.
                let mut row = |ui: &mut egui::Ui, place: usize, title: &str, tag: &str, line: Option<&str>| {
                    let height = if line.is_some() { 48.0 } else { 34.0 };
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
                    if place == picked || response.hovered() {
                        ui.painter().rect_filled(rect, 9.0, CARD);
                    }
                    let first = if line.is_some() { rect.min.y + 16.0 } else { rect.center().y };
                    let tag_at = pos2(rect.max.x - 10.0, first);
                    let font = crate::studio::fixed(11.0);
                    let tagged =
                        crate::studio::paint_spaced(ui, tag_at, Align2::RIGHT_CENTER, tag, font, TEXT_DIM, 0.6);
                    let words =
                        Rect::from_min_max(pos2(rect.min.x + 10.0, rect.min.y), pos2(tagged.min.x - 10.0, rect.max.y));
                    ui.painter().with_clip_rect(words).text(
                        pos2(words.min.x, first),
                        Align2::LEFT_CENTER,
                        title,
                        crate::studio::semibold(13.0),
                        TEXT,
                    );
                    if let Some(line) = line {
                        let under = Rect::from_min_max(pos2(rect.min.x + 10.0, rect.min.y), rect.max - vec2(10.0, 0.0));
                        ui.painter().with_clip_rect(under).text(
                            pos2(under.min.x, rect.min.y + 34.0),
                            Align2::LEFT_CENTER,
                            line,
                            FontId::proportional(12.0),
                            TEXT_FAINT,
                        );
                    }
                    if response.clicked() {
                        clicked = Some(place);
                    }
                };
                for (place, found) in passage.iter().enumerate() {
                    row(ui, place, &found.title, &found.tag, Some(&found.line));
                }
                for (place, (index, line)) in songs.iter().enumerate() {
                    let Some(song) = show.presentations().get(*index) else { continue };
                    row(ui, passage.len() + place, &song.title, "SONG", line.as_deref());
                }
                for (place, found) in verses.iter().enumerate() {
                    row(ui, passage.len() + songs.len() + place, &found.title, &found.tag, Some(&found.line));
                }
                if total == 0 {
                    let words = if answered { "Nothing found" } else { "Looking in the Bibles…" };
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                    ui.painter().text(
                        rect.left_center() + vec2(10.0, 0.0),
                        Align2::LEFT_CENTER,
                        words,
                        FontId::proportional(13.0),
                        TEXT_FAINT,
                    );
                }
                let keys = if cfg!(target_os = "macos") {
                    "↵ Open    ⌘↵ Add to Playlist    esc Close"
                } else {
                    "Enter: Open    Ctrl+Enter: Add to Playlist    Esc: Close"
                };
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                ui.painter().text(
                    rect.left_center() + vec2(10.0, 2.0),
                    Align2::LEFT_CENTER,
                    keys,
                    FontId::proportional(11.0),
                    TEXT_FAINT,
                );
            });
        });
        list = Some(shown.response.rect);
    }
    // What to do, worked out while the lists are still at hand: a song by its place, a passage
    // by what to look up; and whether to add it to the playlist in front instead of opening it.
    enum Do {
        Song(usize),
        Passage(String),
    }
    let chosen = clicked.map(|place| (place, false)).or((enter && total > 0).then_some((picked, command)));
    let chosen = chosen.and_then(|(place, add)| {
        Some((
            match at(place)? {
                Pick::Song(index) => Do::Song(index),
                Pick::Bible(row) => Do::Passage(row.lookup.clone()),
            },
            add,
        ))
    });
    // A press anywhere but in the find box or the list ends the search (and does what it would
    // have done anyway).
    let pressed_outside = ui.input(|input| {
        let at = input.pointer.interact_pos().filter(|_| input.pointer.any_pressed())?;
        let inside = |rect: Option<Rect>| rect.is_some_and(|rect| rect.contains(at));
        Some(!inside(state.find_rect) && !inside(list))
    }) == Some(true);
    let ended = chosen.is_some() || escape || pressed_outside;
    match chosen {
        Some((Do::Song(index), true)) => {
            if let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index)) {
                state.services.add(crate::service::Item::Song { key: key.clone(), title: song.title.clone() });
            }
        }
        Some((Do::Song(index), false)) => {
            commands.push(Command::Select(index));
            state.editing = None;
            state.bible_open = false;
            state.reflow = None;
        }
        // Kept in the playlist with the translations and choices the Bible view has now, as its
        // own Add does.
        Some((Do::Passage(lookup), true)) => {
            let saved = crate::service::SavedPassage { lookup, ..state.bible.settings() };
            state.services.add(crate::service::Item::Passage(saved));
        }
        // The Bible view opens on the passage. It is made in the place that is not on screen,
        // so the screens keep what they show.
        Some((Do::Passage(lookup), false)) => {
            state.view_request = Some(2);
            state.reflow = None;
            state.bible.lookup = lookup;
            state.bible.look_up(show.live().map(|at| at.presentation));
        }
        None => {}
    }
    if ended {
        // The find box gives the keyboard back, so the keys run the show again.
        state.library_search = None;
        ui.memory_mut(|memory| memory.surrender_focus(crate::studio::find_box_id()));
    }
}

/// The key that adds each shape in the editor, in the order of `Shape::ALL`.
const SHAPE_KEYS: [char; 10] = ['O', 'R', 'E', 'G', 'L', 'H', 'P', 'S', 'D', 'A'];

/// Room after a song's title, for the field it turns into when renamed to fit in.
const TITLE_ROOM: f32 = 12.0;

/// The Settings window's panes, in order (`preferences_pane` is a place in it).
pub const PANES: [&str; 5] = ["General", "Screens", "Groups", "Key Mappings", "Integrations"];

/// The Settings window's contents: its panes down the left, the chosen one on the right.
pub fn settings_ui(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{ACCENT, HOVER, PANEL, TEXT, TEXT_DIM};
    egui::Panel::left("settings panes")
        .resizable(false)
        .exact_size(180.0)
        .frame(Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 10, right: 10, top: 14, bottom: 10 }))
        .show(ui, |ui| {
            for (index, name) in PANES.into_iter().enumerate() {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                if state.preferences_pane == index {
                    ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
                } else if response.hovered() {
                    ui.painter().rect_filled(row, 6.0, HOVER);
                }
                ui.painter().text(
                    row.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(13.5),
                    TEXT,
                );
                if response.clicked() {
                    state.preferences_pane = index;
                }
            }
        });
    egui::CentralPanel::default().frame(Frame::new().fill(crate::studio::BG).inner_margin(20)).show(ui, |ui| {
        let pane = PANES.get(state.preferences_pane).copied().unwrap_or("General");
        ui.label(egui::RichText::new(pane).font(crate::studio::semibold(18.0)).color(TEXT));
        ui.add_space(12.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane {
            "Integrations" => crate::integrations::pane(ui, &mut state.voxlive),
            "General" => {
                crate::studio::caption(ui, "COPYRIGHT");
                ui.label("Show a song's copyright line on the audience screens:");
                for choice in crate::service::CopyrightShow::ALL {
                    ui.radio_value(&mut state.copyright_show, choice, choice.name());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("CCLI Licence Number");
                    ui.add(egui::TextEdit::singleline(&mut state.ccli_licence).desired_width(160.0));
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "A song's writer, publisher and CCLI song number are set with the Info icon in its header.",
                    )
                    .size(11.5)
                    .color(TEXT_DIM),
                );
            }
            "Screens" => {
                if ui.button("Configure Screens…").on_hover_text("Option-Command-1").clicked() {
                    state.settings_open = true;
                }
                ui.add_space(10.0);
                ui.add_space(14.0);
                start_with_ui(ui, state);
                ui.add_space(14.0);
                ui.checkbox(&mut state.start_audience_off, "Start with the audience screens off")
                    .on_hover_text("When SoftPresenter opens, nothing is shown until Audience is switched on");
                ui.checkbox(&mut state.stage_on, "Stage screens on");
            }
            "Groups" => {
                ui.label(egui::RichText::new("Each group's colour, and the key that jumps to it.").color(TEXT_DIM));
                ui.add_space(8.0);
                egui::Grid::new("groups").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
                    for (key, names) in sp_model::GROUP_KEYS {
                        let name =
                            names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
                        let mut shown = name.to_owned();
                        if let Some(first) = shown.get_mut(0..1) {
                            first.make_ascii_uppercase();
                        }
                        let (swatch, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        ui.painter().rect_filled(swatch, 4.0, color32(sp_model::group_color(name)));
                        ui.label(shown);
                        ui.label(egui::RichText::new(key.to_ascii_uppercase().to_string()).monospace().color(TEXT_DIM));
                        ui.end_row();
                    }
                });
            }
            _ => {
                ui.label(egui::RichText::new("The keys that run the show, and the menu shortcuts.").color(TEXT_DIM));
                ui.add_space(8.0);
                egui::Grid::new("keys").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                    let show_keys = [
                        ("Show", "Next slide", "→ or Space"),
                        ("Show", "Previous slide", "←"),
                        ("Show", "First / last slide", "Home / End"),
                        ("Show", "Clear All", "Esc or F1"),
                        ("Show", "Clear Slide", "F2"),
                        ("Show", "Clear Media", "F3"),
                        ("Show", "Clear Props", "F4"),
                        ("Show", "Clear Messages", "F6"),
                        ("Show", "Jump to a group", "its letter (see Groups)"),
                        ("Show", "Find a song, a line or a verse", "/"),
                    ];
                    for (area, command, key) in show_keys {
                        ui.label(egui::RichText::new(area).color(TEXT_DIM));
                        ui.label(command);
                        ui.label(egui::RichText::new(key).monospace());
                        ui.end_row();
                    }
                    for (menu, command, key) in crate::menubar::shortcuts() {
                        ui.label(egui::RichText::new(menu).color(TEXT_DIM));
                        ui.label(command.trim_end_matches('…'));
                        ui.label(egui::RichText::new(key).monospace());
                        ui.end_row();
                    }
                });
            }
        });
    });
}

/// What starts playing by itself when the app opens: nothing, a media playlist, or a song on
/// the Announcements layer.
fn start_with_ui(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::service::StartWith;
    crate::studio::caption(ui, "WHEN SOFTPRESENTER OPENS");
    if ui.radio(state.start_with == StartWith::Nothing, "Play nothing").clicked() {
        state.start_with = StartWith::Nothing;
    }
    let first_playlist = state.media_playlists.first().map(|playlist| playlist.id);
    ui.horizontal(|ui| {
        let on = matches!(state.start_with, StartWith::MediaPlaylist { .. });
        if ui.add_enabled(first_playlist.is_some(), egui::RadioButton::new(on, "Play the media playlist")).clicked()
            && !on
            && let Some(id) = first_playlist
        {
            state.start_with = StartWith::MediaPlaylist { id };
        }
        if let StartWith::MediaPlaylist { id } = &mut state.start_with {
            let shown = state.media_playlists.iter().find(|p| p.id == *id).map_or("", |p| p.name.as_str()).to_owned();
            egui::ComboBox::from_id_salt("start playlist").selected_text(shown).show_ui(ui, |ui| {
                for playlist in &state.media_playlists {
                    ui.selectable_value(id, playlist.id, playlist.name.as_str());
                }
            });
        }
    });
    let first_song = state.song_choices.first().map(|(key, _)| key.clone());
    ui.horizontal(|ui| {
        let on = matches!(state.start_with, StartWith::Announcements { .. });
        if ui.add_enabled(first_song.is_some(), egui::RadioButton::new(on, "Play on the Announcements layer")).clicked()
            && !on
            && let Some(song) = first_song.clone()
        {
            state.start_with = StartWith::Announcements { song };
        }
        if let StartWith::Announcements { song } = &mut state.start_with {
            let shown = state.song_choices.iter().find(|(key, _)| key == song).map_or("", |(_, title)| title.as_str());
            egui::ComboBox::from_id_salt("start song").selected_text(shown.to_owned()).show_ui(ui, |ui| {
                for (key, title) in &state.song_choices {
                    ui.selectable_value(song, key.clone(), title.as_str());
                }
            });
        }
    });
    ui.label(
        egui::RichText::new(
            "For a loop before and after the service. A media playlist can follow a folder (its menu beside MEDIA).",
        )
        .size(11.5)
        .color(crate::studio::TEXT_DIM),
    );
}

fn switch(ui: &mut egui::Ui, on: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
    let how_far = ui.ctx().animate_bool(response.id, on);
    let track = Color32::from_gray(70).lerp_to_gamma(ON, how_far);
    ui.painter().rect_filled(rect, 10.0, track);
    let knob = pos2(egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, how_far), rect.center().y);
    ui.painter().circle_filled(knob, 8.0, Color32::WHITE);
    response.on_hover_text(hint).clicked()
}

/// The Screens window's contents: the audience screens on the left, and where the chosen one's
/// picture goes on the right. Every screen shows the same picture. Changes take effect, and are
/// saved, as they are made. `output_on` is whether the audience output is on; switching it is
/// returned as a command.
pub fn screens_ui(ui: &mut egui::Ui, state: &mut HomeState, output_on: bool) -> Vec<Command> {
    use crate::studio::{ACCENT, CARD, HOVER, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT};
    let mut commands = Vec::new();
    let mut changed = false;
    state.screen_selected = state.screen_selected.min(state.screens.len().saturating_sub(1));

    // The list of screens, with + and − under it, as Mac lists have.
    egui::Panel::left("screen list")
        .resizable(false)
        .exact_size(214.0)
        .frame(Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 12 }))
        .show(ui, |ui| {
            egui::Panel::bottom("screen list buttons").resizable(false).frame(Frame::new()).show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let small = vec2(26.0, 22.0);
                    let only = state.screens.iter().filter(|screen| screen.kind == Kind::Audience).count() <= 1
                        && state.screens.get(state.screen_selected).is_some_and(|screen| screen.kind == Kind::Audience);
                    let remove = ui
                        .add_enabled_ui(!only, |ui| ui.add_sized(small, egui::Button::new("−")))
                        .inner
                        .on_hover_text("Remove the selected screen")
                        .on_disabled_hover_text(
                            "There is always one audience screen. Set its output to Off to show nothing.",
                        );
                    if remove.clicked() && !only && state.screen_selected < state.screens.len() {
                        state.screens.remove(state.screen_selected);
                        state.screen_selected = state.screen_selected.saturating_sub(1);
                        state.ndi_draft = None;
                        changed = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let identify = ui.add(egui::Button::selectable(state.identify, "Identify")).on_hover_text(
                            "Mark each screen with its name and its own colour, to tell them apart; the colours \
                                 are shown here beside the screens",
                        );
                        if identify.clicked() {
                            state.identify = !state.identify;
                        }
                    });
                });
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for kind in [Kind::Audience, Kind::Stage] {
                    // The section's heading, with + to add a screen of its kind and a switch for all of them.
                    ui.horizontal(|ui| {
                        crate::studio::caption(ui, if kind == Kind::Audience { "AUDIENCE" } else { "STAGE" });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (on, what) = match kind {
                                Kind::Audience => (output_on, "audience"),
                                Kind::Stage => (state.stage_on, "stage"),
                            };
                            let hint = format!("The {what} screens are {}", if on { "on" } else { "off" });
                            if switch(ui, on, &hint) {
                                match kind {
                                    Kind::Audience => commands.push(Command::SetOutputEnabled(!output_on)),
                                    Kind::Stage => state.stage_on = !state.stage_on,
                                }
                            }
                            let add = ui
                                .add_sized(vec2(22.0, 20.0), egui::Button::new("+").frame(false))
                                .on_hover_text(format!("Add a {what} screen"));
                            if add.clicked() {
                                let name = crate::screens::next_name(&state.screens, kind);
                                // A new screen goes nowhere until it is told where, so nothing
                                // changes on any display by surprise.
                                let screen = match kind {
                                    Kind::Audience => Screen::new(name, Output::Off),
                                    Kind::Stage => Screen::stage(name, Output::Off),
                                };
                                state.screens.push(screen);
                                state.screen_selected = state.screens.len() - 1;
                                state.ndi_draft = None;
                                changed = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let mut any = false;
                    for index in 0..state.screens.len() {
                        if state.screens[index].kind != kind {
                            continue;
                        }
                        any = true;
                        let shown = state.screen_status.get(index).is_some_and(|(_, shown)| *shown);
                        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                        let selected = state.screen_selected == index;
                        if selected {
                            ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
                        } else if response.hovered() {
                            ui.painter().rect_filled(row, 6.0, HOVER);
                        }
                        // A green light when the screen is being shown somewhere.
                        let light = if shown { ON } else { TEXT_FAINT };
                        ui.painter().circle_filled(row.left_center() + vec2(12.0, 0.0), 4.0, light);
                        // While identifying, the colour the screen is marked with, at the right.
                        if state.identify {
                            let swatch = Rect::from_center_size(row.right_center() - vec2(16.0, 0.0), vec2(16.0, 12.0));
                            ui.painter().rect_filled(swatch, 3.0, color32(crate::screens::identify_color(index)));
                        }
                        ui.painter().with_clip_rect(row.shrink(2.0)).text(
                            row.left_center() + vec2(24.0, 0.0),
                            Align2::LEFT_CENTER,
                            state.screens[index].name.as_str(),
                            FontId::proportional(13.5),
                            TEXT,
                        );
                        if response.clicked() {
                            state.screen_selected = index;
                            state.ndi_draft = None;
                        }
                    }
                    if !any {
                        ui.label(egui::RichText::new("None. Press + to add one.").size(12.0).color(TEXT_FAINT));
                    }
                    ui.add_space(14.0);
                }
            });
        });

    // The chosen screen.
    egui::CentralPanel::default().frame(Frame::new().fill(crate::studio::BG).inner_margin(20)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let index = state.screen_selected;
        let Some(screen) = state.screens.get(index).cloned() else { return };
        let (status, shown) = state.screen_status.get(index).cloned().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(screen.name.as_str()).font(crate::studio::semibold(18.0)).color(TEXT));
            let what = if screen.kind == Kind::Stage { "Stage  ·  1920 × 1080" } else { "1920 × 1080" };
            ui.label(egui::RichText::new(what).size(13.0).color(TEXT_DIM));
        });
        ui.add_space(12.0);

        // Where the picture goes, as a little screen.
        let width = ui.available_width();
        let (picture, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).min(190.0)), Sense::hover());
        let frame_rect = Rect::from_center_size(picture.center(), vec2(picture.height() * 16.0 / 9.0, picture.height()));
        ui.painter().rect_filled(frame_rect, 8.0, Color32::BLACK);
        ui.painter().rect_stroke(frame_rect, 8.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let target = match &screen.output {
            Output::Automatic => "Automatic".to_owned(),
            Output::Display(name) => name.clone(),
            Output::Window => "Window".to_owned(),
            Output::Ndi(name) => format!("NDI  ·  {name}"),
            Output::Off => "Off".to_owned(),
        };
        let repeats = status.trim() == target.trim();
        ui.painter().text(
            frame_rect.center() - vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            target,
            crate::studio::semibold(16.0),
            if shown { TEXT } else { TEXT_DIM },
        );
        // What the screen is doing, under where it goes, unless that says it already ("Off").
        if !repeats {
            ui.painter().text(
                frame_rect.center() + vec2(0.0, 14.0),
                Align2::CENTER_CENTER,
                status.as_str(),
                FontId::proportional(12.0),
                if shown { ON } else { TEXT_DIM },
            );
        }
        ui.add_space(16.0);

        // The settings, as rows in a rounded box, labels on the left.
        Frame::new().fill(CARD).corner_radius(10).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("screen settings").num_columns(2).spacing([16.0, 12.0]).min_col_width(84.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                let mut name = screen.name.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY)).changed() {
                    state.screens[index].name = name;
                    changed = true;
                }
                ui.end_row();

                ui.label(egui::RichText::new("Output").color(TEXT_DIM));
                egui::ComboBox::from_id_salt("screen output")
                    .selected_text(output_label(&screen.output, &state.displays))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for choice in output_choices(&screen.output, &screen.name, &state.displays) {
                            let is_ndi = matches!(choice, Output::Ndi(_));
                            let label = output_label(&choice, &state.displays);
                            let on = std::mem::discriminant(&choice) == std::mem::discriminant(&screen.output)
                                && (is_ndi || choice == screen.output);
                            let mut entry = ui.add(egui::Button::selectable(on, label));
                            if is_ndi && !state.ndi_available {
                                entry = entry.on_hover_text(sp_ndi::NOT_INSTALLED);
                            }
                            if entry.clicked() && choice != screen.output {
                                state.screens[index].output = choice;
                                state.ndi_draft = None;
                                changed = true;
                            }
                        }
                    });
                ui.end_row();

                if let Output::Ndi(current) = &screen.output {
                    ui.label(egui::RichText::new("NDI name").color(TEXT_DIM));
                    let mut typed = match &state.ndi_draft {
                        Some((id, typed)) if *id == screen.id => typed.clone(),
                        _ => current.clone(),
                    };
                    let field = ui.add(egui::TextEdit::singleline(&mut typed).desired_width(f32::INFINITY));
                    if field.changed() {
                        state.ndi_draft = Some((screen.id, typed.clone()));
                    }
                    // The name is used once the field is left, or Return is pressed, so NDI is
                    // not restarted at every letter.
                    if field.lost_focus() && state.ndi_draft.take().is_some() && typed != *current {
                        state.screens[index].output = Output::Ndi(typed);
                        changed = true;
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(10.0);
        let about = match &screen.output {
            Output::Automatic => "Choose the display this screen fills. The main display, with the menu bar, is kept for SoftPresenter's own window.",
            Output::Display(_) => {
                "If this display is unplugged the screen shows nowhere until it is back; it never moves to another display."
            }
            Output::Window => "An ordinary window, for trying things out or for capturing in other software.",
            Output::Ndi(_) => {
                "Other computers on the network can show this screen, at 1920 × 1080. NDI® is a registered trademark of Vizrt NDI AB."
            }
            Output::Off => "This screen is kept in the list and shows nowhere.",
        };
        ui.label(egui::RichText::new(about).size(11.5).color(TEXT_DIM));
        // Where NDI comes from, wherever it is chosen; and, without it, that it isn't installed.
        if matches!(screen.output, Output::Ndi(_)) {
            ui.horizontal_wrapped(|ui| {
                if !state.ndi_available {
                    ui.label(
                        egui::RichText::new("NDI isn't installed on this computer. Install NDI Tools (free), then turn this screen on again:")
                            .size(11.5)
                            .color(Color32::from_rgb(255, 170, 120)),
                    );
                }
                ui.hyperlink_to(egui::RichText::new("ndi.video").size(11.5), sp_ndi::DOWNLOAD_URL);
            });
        }
        // An audience screen's look: which layers it shows, its colour, its covered edges.
        if screen.kind == Kind::Audience {
            ui.add_space(14.0);
            crate::studio::caption(ui, "LOOK");
            ui.add_space(4.0);
            // The theme its slides are drawn with: the main theme, or one of its own.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Theme").color(TEXT_DIM))
                    .on_hover_text("The same words, laid out by this theme on this screen");
                ui.add_space(8.0);
                let main_name = state.theme_names.get(state.theme_for_songs).cloned().unwrap_or_default();
                let own = state.screens[index].theme;
                let own_name = own
                    .and_then(|id| state.theme_ids.iter().position(|have| *have == id))
                    .and_then(|at| state.theme_names.get(at).cloned());
                let shown = own_name.unwrap_or_else(|| format!("Main theme ({main_name})"));
                let mut picked = None;
                egui::ComboBox::from_id_salt(("screen theme", index)).selected_text(shown).width(220.0).show_ui(ui, |ui| {
                    if ui.selectable_label(own.is_none(), format!("Main theme ({main_name})")).clicked() {
                        picked = Some(None);
                    }
                    ui.separator();
                    for (id, name) in state.theme_ids.iter().zip(&state.theme_names) {
                        if ui.selectable_label(own == Some(*id), name.as_str()).clicked() {
                            picked = Some(Some(*id));
                        }
                    }
                });
                if let Some(theme) = picked
                    && theme != own
                {
                    state.screens[index].theme = theme;
                    changed = true;
                }
            });
            ui.add_space(4.0);
            let look = &mut state.screens[index].look;
            let before = *look;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Shows").color(TEXT_DIM));
                ui.add_space(8.0);
                ui.checkbox(&mut look.slides, "Slides");
                ui.checkbox(&mut look.media, "Media");
                ui.checkbox(&mut look.props, "Props");
                ui.checkbox(&mut look.messages, "Messages");
                ui.checkbox(&mut look.announcements, "Announcements");
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Colour").color(TEXT_DIM))
                    .on_hover_text("Shown where nothing is drawn: around the slide, and when the screen is clear");
                ui.add_space(8.0);
                let mut color = Color32::from_rgba_unmultiplied(look.color.r, look.color.g, look.color.b, 255);
                if ui.color_edit_button_srgba(&mut color).changed() {
                    let [r, g, b, _] = color.to_srgba_unmultiplied();
                    look.color = sp_model::Rgba::rgb(r, g, b);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Cover edges").color(TEXT_DIM))
                    .on_hover_text("How much of each edge to cover, in the slide's pixels, for a screen that cuts them off");
                ui.add_space(8.0);
                for (at, name) in ["Left", "Top", "Right", "Bottom"].into_iter().enumerate() {
                    ui.add(egui::DragValue::new(&mut look.mask[at]).range(0.0..=540.0).speed(1.0).prefix(format!("{name} ")));
                }
            });
            if *look != before {
                changed = true;
            }
            if !look.is_default() && ui.small_button("Show Everything").on_hover_text("Back to every layer, black, nothing covered").clicked() {
                *look = crate::screens::Look::default();
                changed = true;
            }
        }
        if screen.kind == Kind::Stage {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("A stage screen shows the words on screen now, the next slide's words under them, and the time.")
                    .size(11.5)
                    .color(TEXT_DIM),
            );
        }
        });
    });
    state.screens_changed |= changed;
    commands
}

/// Says which of a song's fonts this computer does not have, and offers a place to look for
/// each. Until the font is installed the song is drawn with a substitute, so it will not look
/// the way it does on the computer it came from.
fn missing_font_notice(ui: &mut egui::Ui, missing: &[String]) {
    if missing.is_empty() {
        return;
    }
    ui.add_space(10.0);
    let label = if missing.len() == 1 {
        format!("Missing font: {}", missing[0])
    } else {
        format!("{} missing fonts", missing.len())
    };
    let warning = Color32::from_rgb(240, 190, 40);
    ui.menu_button(egui::RichText::new(label).size(12.0).color(warning), |ui| {
        ui.set_max_width(320.0);
        ui.label("This song uses fonts that are not installed on this computer. It is shown with a substitute until they are installed.");
        ui.separator();
        for family in missing {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(family.as_str()).strong());
                let query: String = family.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
                ui.hyperlink_to("Look for it", format!("https://fonts.google.com/?query={query}"))
                    .on_hover_text("Opens a font library in your browser. Not every font is free to download.");
            });
        }
        ui.separator();
        ui.label(egui::RichText::new("After installing a font, restart SoftPresenter.").size(11.5).color(DIM));
    });
}

/// Asks before a theme is deleted, since it cannot be brought back.
/// Asks before taking a song out of the library.
/// The New Song sheet: the song's title, its theme, and its words to paste in (the song text
/// format: [Verse 1] and so on, a blank line between slides). Create makes it; Escape or Cancel
/// closes the sheet.
fn new_song_sheet(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(sheet) = state.new_song_sheet.as_mut() else { return };
    let mut decided = None;
    egui::Window::new("New Song")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fade_in(false)
        .anchor(Align2::CENTER_CENTER, [0.0, -30.0])
        .show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("New Song").font(crate::studio::semibold(16.0)));
            ui.add_space(8.0);
            egui::Grid::new("new song").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Title").color(DIM));
                let title = ui.add(egui::TextEdit::singleline(&mut sheet.title).desired_width(340.0));
                if std::mem::take(&mut sheet.focus) {
                    title.request_focus();
                }
                if title.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    decided = Some(true);
                }
                ui.end_row();
                ui.label(egui::RichText::new("Theme").color(DIM));
                let shown = sheet.theme.and_then(|at| state.theme_names.get(at)).map_or("Main Theme", String::as_str);
                egui::ComboBox::from_id_salt("new song theme").selected_text(shown).width(340.0).show_ui(ui, |ui| {
                    if ui.add(egui::Button::selectable(sheet.theme.is_none(), "Main Theme")).clicked() {
                        sheet.theme = None;
                    }
                    for (at, name) in state.theme_names.iter().enumerate() {
                        if ui.add(egui::Button::selectable(sheet.theme == Some(at), name.as_str())).clicked() {
                            sheet.theme = Some(at);
                        }
                    }
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Words (optional)").color(DIM));
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut sheet.words)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .hint_text("[Verse 1]\nThe first slide's lines\n\nThe next slide\n\n[Chorus]\n…"),
                );
            });
            ui.label(
                egui::RichText::new("A line in square brackets starts a group; a blank line starts a new slide; -- before a line starts its translation.")
                    .size(11.0)
                    .color(DIM),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let size = vec2(96.0, 30.0);
                    let ready = !sheet.title.trim().is_empty();
                    let emphasis = if ready { crate::studio::Emphasis::Plain } else { crate::studio::Emphasis::Idle };
                    if crate::studio::block_button(ui, size, "Create", "", emphasis).clicked() {
                        decided = Some(true);
                    }
                    if crate::studio::block_button(ui, size, "Cancel", "", crate::studio::Emphasis::Plain).clicked() {
                        decided = Some(false);
                    }
                });
            });
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                decided = Some(false);
            }
        });
    match decided {
        Some(true) if !sheet.title.trim().is_empty() => {
            let mut made = state.new_song_sheet.take().unwrap_or_default();
            made.title = made.title.trim().to_owned();
            state.new_song_request = Some(made);
        }
        Some(false) => state.new_song_sheet = None,
        _ => {}
    }
}

fn confirm_delete_song(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    if state.confirm_delete_songs.is_empty() {
        return;
    }
    let songs = state.confirm_delete_songs.clone();
    let question = match songs.as_slice() {
        [one] => match show.presentations().get(*one) {
            Some(song) => format!("Delete \u{201c}{}\u{201d}?", song.title),
            None => {
                state.confirm_delete_songs.clear();
                return;
            }
        },
        many => format!("Delete {} songs?", many.len()),
    };
    let note = if songs.len() == 1 {
        "A song made in SoftPresenter goes to the Trash. One opened from elsewhere stays where it is on your Mac."
    } else {
        "Songs made in SoftPresenter go to the Trash. Ones opened from elsewhere stay where they are on your Mac."
    };
    if let Some(delete) = confirm_delete(ui, &question, note) {
        state.confirm_delete_songs.clear();
        if delete {
            state.picked_songs.clear();
            state.songs_to_delete = songs;
        }
    }
}

/// Asks before deleting slides from their songs, and deletes them.
fn confirm_delete_slides(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let present = |at: &SlideRef| show.presentations().get(at.presentation).is_some_and(|s| at.slide < s.slide_count());
    state.confirm_delete_slides.retain(present);
    let slides = state.confirm_delete_slides.clone();
    let question = match slides.as_slice() {
        [] => return,
        [one] => {
            let title = show.presentations().get(one.presentation).map_or("", |song| song.title.as_str());
            format!("Delete slide {} of \u{201c}{title}\u{201d}?", one.slide + 1)
        }
        many => format!("Delete {} slides?", many.len()),
    };
    let note = if slides.len() == 1 { "Edit > Undo brings it back." } else { "Edit > Undo brings them back." };
    if let Some(delete) = confirm_delete(ui, &question, note) {
        state.confirm_delete_slides.clear();
        if delete {
            state.selected_slide = None;
            state.picked_slides.clear();
            state.slides_to_delete = slides;
        }
    }
}

/// Asks before taking items out of the media bin, and takes them out.
fn confirm_delete_media(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    if state.confirm_delete_media.is_empty() {
        return;
    }
    let items = state.confirm_delete_media.clone();
    let question = match items.as_slice() {
        [one] => match show.media().get(*one) {
            Some(item) => format!("Delete \u{201c}{}\u{201d} from the media bin?", item.name),
            None => {
                state.confirm_delete_media.clear();
                return;
            }
        },
        many => format!("Delete {} items from the media bin?", many.len()),
    };
    let note = if items.len() == 1 {
        "The file stays where it is on your Mac."
    } else {
        "The files stay where they are on your Mac."
    };
    if let Some(delete) = confirm_delete(ui, &question, note) {
        state.confirm_delete_media.clear();
        if delete {
            state.picked_media.clear();
            state.media_request = Some(MediaRequest::Delete(items));
        }
    }
}

fn confirm_delete_theme(ui: &mut egui::Ui, state: &mut HomeState) {
    confirm_delete_template(ui, state);
    let Some(index) = state.confirm_delete_theme else { return };
    let Some(name) = state.theme_names.get(index).cloned() else {
        state.confirm_delete_theme = None;
        return;
    };
    let in_use = if state.theme_for_songs == index {
        "It is the main theme: songs and Bible slides will use the built-in theme instead."
    } else {
        ""
    };
    if let Some(delete) = confirm_delete(ui, &format!("Delete the theme \"{name}\"?"), in_use) {
        state.confirm_delete_theme = None;
        if delete {
            state.theme_request = Some(ThemeRequest::Delete(index));
        }
    }
}

/// Asks before deleting one of the templates of the theme being edited, and deletes it.
fn confirm_delete_template(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(template) = state.confirm_delete_template else { return };
    let bible_uses_it =
        state.theme_edit.as_ref().and_then(|edit| edit.theme.templates.get(template)).is_some_and(|t| t.bible);
    let name = state.theme_edit.as_ref().and_then(|edit| edit.theme.templates.get(template)).map(|t| t.name.clone());
    let Some(name) = name else {
        state.confirm_delete_template = None;
        return;
    };
    let note = if bible_uses_it {
        "Bible slides use it; they will use the template that suits them best instead."
    } else {
        ""
    };
    let Some(delete) = confirm_delete(ui, &format!("Delete the template \"{name}\"?"), note) else { return };
    state.confirm_delete_template = None;
    if delete
        && let Some(edit) = state.theme_edit.as_mut()
        && edit.theme.remove_template(template)
    {
        edit.changed = true;
        // The template being edited stays open. If it was the one deleted, the one before
        // it (or the new first one) is opened in its place.
        let was = edit.template;
        edit.template = if was >= template { was.saturating_sub(1) } else { was };
        if was == template {
            state.editing = Some(EditDraft::of_slide(edit.theme.templates[edit.template].as_slide(), edit.template));
        } else if let Some(draft) = state.editing.as_mut() {
            draft.at.slide = edit.template;
        }
    }
}

/// Asks whether to delete something, as a macOS alert does. Return deletes, as the Delete
/// button does; Escape cancels. `None` until it is answered.
fn confirm_delete(ui: &mut egui::Ui, question: &str, note: &str) -> Option<bool> {
    let message = if note.is_empty() { "You can\u{2019}t undo this.".to_owned() } else { note.to_owned() };
    crate::studio::alert(ui.ctx(), question, crate::icons::Icon::Trash, question, &message, "Delete")
}

/// The top of the Theme Editor's left column: every theme in a plain list. A click opens a
/// theme; a click on the open theme's name renames it; a tag shows which kinds of slide use
/// each one, and a right-click changes that. The plus makes a new theme.
fn theme_list(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{ACCENT, CARD, TEXT, TEXT_DIM, TEXT_FAINT};
    let editing = state.theme_edit.as_ref().map_or(0, |edit| edit.index);
    Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 6 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
        ui.horizontal(|ui| {
            crate::studio::caption(ui, "THEMES");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
                let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                if response.on_hover_text("New theme: a copy of the one songs use").clicked() {
                    state.theme_request = Some(ThemeRequest::New);
                }
            });
        });
        ui.add_space(4.0);
        let names = state.theme_names.clone();
        let mut start_rename = false;
        let mut renamed: Option<Option<String>> = None;
        egui::ScrollArea::vertical().id_salt("theme list").max_height(150.0).auto_shrink([false, true]).show(
            ui,
            |ui| {
                for (index, name) in names.iter().enumerate() {
                    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                    let open = index == editing;
                    if open {
                        ui.painter().rect_filled(row, 7.0, ACCENT.gamma_multiply(0.22));
                    } else if response.hovered() {
                        ui.painter().rect_filled(row, 7.0, CARD);
                    }
                    // Tags at the right say what uses this theme.
                    let mut right = row.max.x - 6.0;
                    // Main for the main theme; the names of the screens set to any other.
                    let id = state.theme_ids.get(index).copied();
                    let mut tags: Vec<(bool, &str)> = vec![(state.theme_for_songs == index, "Main")];
                    tags.extend(
                        state
                            .screens
                            .iter()
                            .filter(|screen| screen.theme.is_some() && screen.theme == id)
                            .map(|screen| (true, screen.name.as_str())),
                    );
                    for (used, tag) in tags {
                        if !used {
                            continue;
                        }
                        let text = ui.painter().layout_no_wrap(tag.to_owned(), FontId::proportional(10.0), TEXT_DIM);
                        let pill = Rect::from_min_size(
                            pos2(right - text.size().x - 10.0, row.center().y - 8.0),
                            vec2(text.size().x + 10.0, 16.0),
                        );
                        ui.painter().rect_filled(pill, 8.0, Color32::from_white_alpha(18));
                        ui.painter().galley(
                            pos2(pill.min.x + 5.0, pill.center().y - text.size().y / 2.0),
                            text,
                            TEXT_DIM,
                        );
                        right = pill.min.x - 4.0;
                    }
                    let name_area =
                        Rect::from_min_max(pos2(row.min.x + 6.0, row.min.y + 3.0), pos2(right - 2.0, row.max.y - 3.0));
                    if open && let Some(typed) = state.renaming_theme.as_mut() {
                        let field = crate::studio::rename_field_at(
                            ui,
                            typed,
                            FontId::proportional(13.5),
                            TEXT,
                            ACCENT,
                            pos2(row.min.x + 10.0, row.center().y),
                            name_area.max.x,
                        );
                        if std::mem::take(&mut state.rename_started) {
                            field.request_focus();
                        }
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                        if escape {
                            renamed = Some(None);
                        } else if enter || field.lost_focus() {
                            renamed = Some(Some(typed.clone()));
                        }
                    } else {
                        ui.painter().with_clip_rect(name_area).text(
                            pos2(row.min.x + 10.0, row.center().y),
                            Align2::LEFT_CENTER,
                            name.as_str(),
                            FontId::proportional(13.5),
                            if open { Color32::WHITE } else { TEXT },
                        );
                        // A click opens the theme; a click on the open theme's name, or a double
                        // click, renames it, as in a file list.
                        if response.clicked() || response.double_clicked() {
                            state.picking = Picking::Themes;
                        }
                        if response.double_clicked() || (response.clicked() && open) {
                            if !open {
                                state.theme_request = Some(ThemeRequest::Edit(index));
                            } else {
                                start_rename = true;
                            }
                        } else if response.clicked() {
                            state.theme_request = Some(ThemeRequest::Edit(index));
                        }
                    }
                    response.context_menu(|ui| {
                        // The main theme draws songs and Bible slides on every screen.
                        if ui
                            .add_enabled(state.theme_for_songs != index, egui::Button::new("Use as Main Theme"))
                            .on_hover_text("Songs and Bible slides are drawn with the main theme")
                            .clicked()
                        {
                            state.theme_request = Some(ThemeRequest::UseForSongs(index));
                            ui.close();
                        }
                        // Which audience screens draw with it: those set to it, and, for the main
                        // theme, those left on the main theme.
                        let id = state.theme_ids.get(index).copied();
                        let main = state.theme_for_songs == index;
                        let audience: Vec<usize> = (0..state.screens.len())
                            .filter(|at| state.screens[*at].kind == crate::screens::Kind::Audience)
                            .collect();
                        if !audience.is_empty() {
                            ui.menu_button("Use on", |ui| {
                                for at in audience {
                                    let screen = &mut state.screens[at];
                                    let mut on = screen.theme == id || (main && screen.theme.is_none());
                                    if ui.checkbox(&mut on, screen.name.as_str()).clicked() {
                                        // On: this theme (the main one needs nothing of its own).
                                        // Off: back to the main theme.
                                        screen.theme = if on && !main { id } else { None };
                                        state.screens_changed = true;
                                    }
                                }
                            });
                        }
                        if ui.button("Duplicate").clicked() {
                            state.theme_request = Some(ThemeRequest::Duplicate(index));
                            ui.close();
                        }
                        if open {
                            ui.separator();
                            if ui.button("Rename").clicked() {
                                start_rename = true;
                                ui.close();
                            }
                        }
                        // Deleting asks first. The built-in theme is always kept.
                        ui.separator();
                        let delete = ui
                            .add_enabled(!state.themes_kept.contains(&index), egui::Button::new("Delete Theme…"))
                            .on_disabled_hover_text("The built-in themes are always kept");
                        if delete.clicked() {
                            state.confirm_delete_theme = Some(index);
                            ui.close();
                        }
                    });
                }
            },
        );
        if start_rename {
            state.renaming_theme = names.get(editing).cloned();
            state.rename_started = true;
        }
        if let Some(outcome) = renamed {
            state.renaming_theme = None;
            if let (Some(name), Some(edit)) = (outcome, state.theme_edit.as_mut())
                && !name.trim().is_empty()
                && name.trim() != edit.theme.name
            {
                edit.theme.name = name.trim().to_owned();
                edit.changed = true;
            }
        }
        ui.add_space(8.0);
        crate::studio::caption(ui, "TEMPLATES");
    });
}

/// A menu that points a translation's words, or its reference, at one of the theme's boxes.
/// Each entry has a small picture of the slide with its box filled in. Boxes in `taken` are
/// in use elsewhere and cannot be chosen. `with_verse`, for a reference menu, is the box the
/// same translation's words are in: choosing "With Verse" puts the reference there too.
#[allow(clippy::too_many_arguments)]
fn target_menu(
    ui: &mut egui::Ui,
    salt: (&str, usize),
    target: &mut BibleTarget,
    boxes: &[(sp_model::Role, SlideRect)],
    labels: &[String],
    taken: &[sp_model::Role],
    with_verse: Option<&sp_model::Role>,
    width: f32,
) {
    let is_with_verse = with_verse.is_some() && target.as_ref() == with_verse;
    // A box is called what the theme calls it.
    let label_of = |role: &sp_model::Role| {
        boxes
            .iter()
            .position(|(have, _)| have == role)
            .and_then(|at| labels.get(at).cloned())
            .unwrap_or_else(|| role.label())
    };
    let shown = match target {
        _ if is_with_verse => "With Verse".to_owned(),
        Some(role) => label_of(role),
        None => "None".to_owned(),
    };
    egui::ComboBox::from_id_salt(salt).selected_text(shown).width(width).height(360.0).truncate().show_ui(ui, |ui| {
        let entry = |ui: &mut egui::Ui, name: &str, lit: Option<&sp_model::Role>, chosen: bool, free: bool| -> bool {
            let (row, response) =
                ui.allocate_exact_size(vec2(170.0, 34.0), if free { Sense::click() } else { Sense::hover() });
            if chosen {
                ui.painter().rect_filled(row, 5.0, crate::studio::ACCENT.gamma_multiply(0.3));
            } else if free && response.hovered() {
                ui.painter().rect_filled(row, 5.0, crate::studio::HOVER);
            }
            let picture = Rect::from_min_size(row.min + vec2(4.0, 4.0), vec2(46.0, 26.0));
            box_diagram(ui.painter(), picture, boxes, lit);
            let ink = if free { crate::studio::TEXT } else { crate::studio::TEXT_FAINT };
            ui.painter().text(
                pos2(picture.max.x + 10.0, row.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(13.0),
                ink,
            );
            if !free {
                response.on_hover_text("Another translation is using this box");
                return false;
            }
            response.clicked()
        };
        if entry(ui, "None", None, target.is_none(), true) {
            *target = None;
            ui.close();
        }
        if let Some(with) = with_verse
            && entry(ui, "With Verse", Some(with), is_with_verse, true)
        {
            *target = Some(with.clone());
            ui.close();
        }
        for (role, _) in boxes {
            // In a reference menu the words' own box is reached by "With Verse".
            if Some(role) == with_verse {
                continue;
            }
            let chosen = target.as_ref() == Some(role);
            let free = chosen || !taken.contains(role);
            if entry(ui, &label_of(role), Some(role), chosen, free) {
                *target = Some(role.clone());
                ui.close();
            }
        }
    });
}

/// The Bible page, in place of the slide grid: a field for a reference, a search field, a row
/// for each translation saying where its verse and its reference go on the slide, the book,
/// chapter and verse lists, and the passage's slides.
fn bible_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let live = show.live().map(|at| at.presentation);
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        egui::Panel::bottom("bible bar").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                // Make the passage one verse longer, at its start or its end.
                let shown = !state.bible.shown.is_empty();
                let back = ui.add_enabled(shown, egui::Button::new("‹ Verse")).on_hover_text("Add the verse before");
                let on = ui.add_enabled(shown, egui::Button::new("Verse ›")).on_hover_text("Add the verse after");
                if back.clicked() {
                    state.bible.step(false, live);
                } else if on.clicked() {
                    state.bible.step(true, live);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| crate::studio::view_controls(ui, state));
            });
        });

        Frame::new().fill(SIDE).inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            // One row whenever the two fields can keep a usable width beside Books and the
            // actions (measured as they are drawn); the fields shrink to make room. Only when
            // they can't do the actions take a second row.
            let across = ui.available_width();
            let gap = ui.spacing().item_spacing.x;
            let button = |ui: &egui::Ui, words: &str| {
                let font = egui::TextStyle::Button.resolve(ui.style());
                ui.painter().layout_no_wrap(words.to_owned(), font, Color32::WHITE).size().x
                    + ui.spacing().button_padding.x * 2.0
            };
            let front = state.services.front().name.clone();
            let mut actions =
                button(ui, "Options") + gap + button(ui, &format!("Add to \u{201c}{front}\u{201d}")) + gap;
            if state.services.list.len() > 1 {
                actions += button(ui, "▾") + gap;
            }
            let books = button(ui, "Books") + gap + button(ui, "History") + gap;
            // Each field also has its own spacing after it.
            let room_in_one = across - books - actions - gap * 2.0 - 8.0;
            let wide = room_in_one >= 260.0;
            let (lookup_width, search_width) = if wide {
                ((room_in_one * 0.55).min(280.0), (room_in_one * 0.45).min(240.0))
            } else {
                // The whole first row, less the room between the two.
                let fields = across - gap * 3.0;
                ((fields * 0.58).floor(), (fields * 0.42).floor())
            };
            ui.horizontal(|ui| {
                let entered = |response: &egui::Response, ui: &egui::Ui| {
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                };
                let lookup_id = ui.scope_id().with("scripture lookup");
                // While a book's name is being typed, the books it could be are offered under
                // the field, each named in the language typed in: arrows pick, Tab or Return
                // (or a click) takes one, Escape hides them.
                let language = state
                    .bible
                    .lead()
                    .and_then(|at| state.bible.shelf.get(at).ok())
                    .map(|bible| bible.info.language.clone())
                    .unwrap_or_default();
                let completion = egui::CompletionPopup::new(lookup_id)
                    .word_boundary(|_| false)
                    .align(egui::RectAlign::BOTTOM_START)
                    .max_height(240.0)
                    .show(
                        ui,
                        egui::TextEdit::singleline(&mut state.bible.lookup)
                            .hint_text("Scripture Lookup")
                            .desired_width(lookup_width),
                        |query| book_suggestions(query.text, &language),
                    );
                let lookup = completion
                    .text_edit
                    .response
                    .response
                    .clone()
                    .on_hover_text("Type a reference, such as John 3:16 (Control-B comes here)");
                let now = ui.input(|input| input.time);
                if lookup.changed() && completion.accepted.is_none() {
                    state.bible.typed_at = Some(now);
                }
                if entered(&lookup, ui) {
                    state.bible.look_up(live);
                    // The field keeps the keyboard with its words selected, ready for the next one.
                    lookup.request_focus();
                    select_all_text(ui.ctx(), lookup_id, &state.bible.lookup);
                } else if let Some(at) = state.bible.typed_at {
                    // A moment after typing stops, the slides show what is typed. Nothing goes on
                    // screen until a slide is clicked.
                    let wait = LOOKUP_PAUSE - (now - at);
                    if wait <= 0.0 {
                        state.bible.typed_at = None;
                        if has_chapter(&state.bible.lookup) {
                            state.bible.preview(live);
                        }
                    } else {
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
                    }
                }
                if std::mem::take(&mut state.bible.focus_lookup) {
                    lookup.request_focus();
                    select_all_text(ui.ctx(), lookup_id, &state.bible.lookup);
                    // Drawn again at once, so the field shows it has the keyboard without
                    // waiting for the mouse to move.
                    ui.ctx().request_repaint();
                }
                if wide {
                    bible_nav(ui, state, live);
                }
                let search = ui
                    .add(
                        egui::TextEdit::singleline(&mut state.bible.search)
                            .hint_text("Search")
                            .desired_width(search_width),
                    )
                    .on_hover_text("Type words to find in the first Bible, and press Return");
                if entered(&search, ui) {
                    state.bible.find();
                }
                if wide {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        bible_actions(ui, show, state, &mut *thumb)
                    });
                }
            });
            // Too narrow for one row: the two fields share the first, and Books, History and the
            // actions go on a second, at its two ends.
            if !wide {
                ui.horizontal(|ui| {
                    bible_nav(ui, state, live);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        bible_actions(ui, show, state, &mut *thumb)
                    });
                });
            }

            let mut add = false;
            let mut remove = None;
            let count = state.bible.rows.len();
            let boxes = state.bible.boxes.clone();
            let labels = state.bible.box_labels.clone();
            let used: Vec<(BibleTarget, BibleTarget)> =
                state.bible.rows.iter().map(|row| (row.verse.clone(), row.reference.clone())).collect();
            let bibles_used: Vec<usize> = state.bible.rows.iter().filter_map(|row| row.bible).collect();
            // The translations as a small table: what each column is, said once, then a row for
            // each translation whose pickers share the width, so each stays on one line however
            // narrow the window. License (i) and + or − at the end of each row.
            let gap = 8.0;
            let ends = 26.0 + 28.0 + gap * 2.0;
            let room = (ui.available_width() - ends - gap * 2.0).max(240.0);
            let (bible_width, menu_width) = ((room * 0.44).floor(), (room * 0.28).floor());
            // Which Bibles show, and where their words go, is set once and seldom changed: folded
            // into one line of the translations' short names, with Change to open the table (and
            // Done to fold it again). Whether it is open is kept between starts.
            let mut fold = false;
            if state.bible.rows_open {
                // The headings' row is as tall as a button, so Done at its end is a whole one.
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    ui.spacing_mut().item_spacing.x = gap;
                    for (heading, width) in
                        [("TRANSLATION", bible_width), ("VERSE GOES IN", menu_width), ("REFERENCE GOES IN", menu_width)]
                    {
                        let (place, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
                        ui.painter().with_clip_rect(place).text(
                            place.left_center() + vec2(2.0, 0.0),
                            Align2::LEFT_CENTER,
                            heading,
                            FontId::proportional(10.0),
                            Color32::from_gray(130),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Done").on_hover_text("Fold the translations away into one line").clicked() {
                            fold = true;
                        }
                    });
                });
                for (index, row) in state.bible.rows.iter_mut().enumerate() {
                    // Boxes another translation already fills are not offered again.
                    let taken = |by_verse: bool| -> Vec<sp_model::Role> {
                        used.iter()
                            .enumerate()
                            .flat_map(|(other, (verse, reference))| {
                                let mine = other == index;
                                let verse = verse.clone().filter(|_| !(mine && by_verse));
                                let reference = reference.clone().filter(|_| !mine);
                                [verse, reference]
                            })
                            .flatten()
                            .collect()
                    };
                    let shelved = row.bible.and_then(|at| state.bible.shelf.bibles.get(at));
                    let chosen = shelved.map_or("None", |b| b.name.as_str()).to_owned();
                    // Each picker held to its column's width; a longer name is cut off.
                    let column = |ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.allocate_ui_with_layout(vec2(width, 28.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.set_max_width(width);
                            add(ui);
                        });
                    };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        column(ui, bible_width, &mut |ui| {
                            egui::ComboBox::from_id_salt(("bible translation", index))
                                .selected_text(&chosen)
                                .width(bible_width)
                                .truncate()
                                .show_ui(ui, |ui| {
                                    // None frees the row's Bible for another row: to swap two, set one
                                    // row to None, give its Bible to the other, then pick again.
                                    if ui
                                        .add(egui::Button::selectable(row.bible.is_none(), "None"))
                                        .on_hover_text("Show no Bible in this row, so its Bible can go to another row")
                                        .clicked()
                                    {
                                        row.bible = None;
                                    }
                                    ui.separator();
                                    // A Bible another row already shows is greyed out.
                                    for (place, bible) in state.bible.shelf.bibles.iter().enumerate() {
                                        let elsewhere = row.bible != Some(place) && bibles_used.contains(&place);
                                        let label = format!("{} ({})", bible.name, bible.short);
                                        let item = ui.add_enabled(
                                            !elsewhere,
                                            egui::Button::selectable(row.bible == Some(place), label),
                                        );
                                        if item
                                            .on_disabled_hover_text(
                                                "Another row shows this Bible. Set that row to None first.",
                                            )
                                            .clicked()
                                        {
                                            row.bible = Some(place);
                                        }
                                    }
                                });
                        });
                        column(ui, menu_width, &mut |ui| {
                            target_menu(
                                ui,
                                ("bible verse", index),
                                &mut row.verse,
                                &boxes,
                                &labels,
                                &taken(true),
                                None,
                                menu_width,
                            );
                        });
                        let with = row.verse.clone();
                        column(ui, menu_width, &mut |ui| {
                            target_menu(
                                ui,
                                ("bible reference", index),
                                &mut row.reference,
                                &boxes,
                                &labels,
                                &taken(false),
                                with.as_ref(),
                                menu_width,
                            );
                        });
                        bible_row_extras(ui, index, count, shelved, &mut add, &mut remove);
                    });
                }
            } else {
                let mut open = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(egui::RichText::new("TRANSLATIONS").size(10.0).color(Color32::from_gray(130)));
                    ui.add_space(4.0);
                    for row in &state.bible.rows {
                        let Some(bible) = row.bible.and_then(|at| state.bible.shelf.bibles.get(at)) else { continue };
                        let goes = |target: &BibleTarget| {
                            target.as_ref().map_or("nowhere".to_owned(), |role| {
                                boxes
                                    .iter()
                                    .position(|(have, _)| have == role)
                                    .and_then(|at| labels.get(at).cloned())
                                    .unwrap_or_else(|| role.label())
                            })
                        };
                        let tag =
                            egui::Button::new(egui::RichText::new(bible.short.as_str()).size(12.0)).corner_radius(10);
                        let hint = format!(
                            "{}: its words go in {}, its reference in {}",
                            bible.name,
                            goes(&row.verse),
                            goes(&row.reference)
                        );
                        if ui.add(tag).on_hover_text(hint).clicked() {
                            open = true;
                        }
                    }
                    if ui.button("Change").on_hover_text("Choose the Bibles shown and the boxes they go in").clicked() {
                        open = true;
                    }
                });
                if open {
                    state.bible.rows_open = true;
                }
            }
            if fold {
                state.bible.rows_open = false;
            }
            if add {
                state.bible.add_row();
            }
            if let Some(index) = remove {
                state.bible.remove_row(index);
            }
        });

        // Say so when two Bibles are shown together and one only mostly follows a known numbering.
        let own: Vec<&str> = state
            .bible
            .rows
            .iter()
            .filter_map(|row| row.bible.and_then(|at| state.bible.shelf.bibles.get(at)))
            .filter(|b| b.loose)
            .map(|b| b.name.as_str())
            .collect();
        if !own.is_empty() && state.bible.rows.len() > 1 {
            let note = format!(
                "{} numbers a few verses its own way, so beside another Bible a verse here and there may be one off.",
                own.join(" and ")
            );
            Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
                ui.label(egui::RichText::new(note).size(11.5).color(Color32::from_rgb(240, 190, 40)));
            });
        }
        // A change to the Bibles or the options remakes the passage that is showing.
        if !state.bible.shown.is_empty() && state.bible.made_with != state.bible.choices() {
            state.bible.lookup = state.bible.shown.clone();
            state.bible.look_up(live);
        }
        if let Some(message) = state.bible.message.clone() {
            Frame::new().inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.label(egui::RichText::new(message).color(Color32::from_rgb(255, 150, 120)));
            });
        }
        if state.bible.lists_open {
            ui.horizontal_top(|ui| {
                let height = ui.available_height();
                ui.allocate_ui_with_layout(
                    vec2(crate::bible::LISTS_WIDTH, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_size(vec2(crate::bible::LISTS_WIDTH, height));
                        crate::bible::lists(ui, &mut state.bible, live);
                    },
                );
                ui.allocate_ui_with_layout(ui.available_size(), Layout::top_down(Align::Min), |ui| {
                    // The slides keep to their own column: without this, what they draw and the
                    // space between them (which takes clicks) reach over the lists.
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    bible_results(ui, show, state, thumb, commands, live);
                });
            });
        } else {
            bible_results(ui, show, state, thumb, commands, live);
        }
    });
}

/// What the Bible page shows below its controls: verses found by a search, or the passage's
/// slides, or a word on how to start.
fn bible_results(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
    live: Option<usize>,
) {
    if !state.bible.found.is_empty() {
        // Search results: each can be opened as its verse or as its whole chapter.
        let mut picked = None;
        egui::ScrollArea::vertical().id_salt("bible found").auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    let count = state.bible.found.len();
                    let found = if count == 1 { "1 verse found".to_owned() } else { format!("{count} verses found") };
                    ui.label(egui::RichText::new(found).size(12.0).color(DIM));
                    if ui.button("Close").clicked() {
                        picked = Some(String::new());
                    }
                });
                for (reference, chapter, words) in &state.bible.found {
                    Frame::new()
                        .fill(crate::studio::CARD)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Chapter").on_hover_text("Open the whole chapter as slides").clicked()
                                    {
                                        picked = Some(chapter.clone());
                                    }
                                    if ui.button("Verse").on_hover_text("Open this verse as a slide").clicked() {
                                        picked = Some(reference.clone());
                                    }
                                    ui.with_layout(Layout::left_to_right(Align::Center).with_main_wrap(false), |ui| {
                                        ui.label(
                                            egui::RichText::new(reference.as_str()).font(crate::studio::semibold(13.0)),
                                        );
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(words.as_str()).size(13.0).color(DIM))
                                                .truncate(),
                                        );
                                    });
                                });
                            });
                        });
                }
            });
        });
        match picked {
            Some(reference) if reference.is_empty() => state.bible.found.clear(),
            Some(reference) => {
                state.bible.lookup = reference;
                state.bible.look_up(live);
            }
            None => {}
        }
        return;
    }
    let slot = state.bible.slots.map(|slots| slots[state.bible.showing]);
    match slot.filter(|slot| show.presentations().get(*slot).is_some_and(|p| p.slide_count() > 0)) {
        Some(slot) => slide_grid(ui, show, slot, state, thumb, commands),
        None => {
            // Centred, in lines no wider than reads well, with room either side.
            let room = ui.available_size();
            ui.add_space((room.y * 0.38).max(20.0));
            ui.vertical_centered(|ui| {
                ui.set_max_width((room.x - 48.0).clamp(160.0, 440.0));
                for line in [
                    "Type a reference such as John 3:16-18 in Scripture Lookup.",
                    "Its verses appear here as slides as you type; click one to show it.",
                    "Control-B goes to Scripture Lookup from anywhere.",
                ] {
                    ui.add(egui::Label::new(egui::RichText::new(line).color(DIM)).wrap().halign(Align::Center));
                }
            });
        }
    }
}

/// What the Options panel previews: the passage showing, whose slides are made again whenever
/// a choice changes, or two sample verses when none is.
fn bible_preview(
    show: &Show,
    page: &mut BiblePage,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
) -> crate::bible::SlidesPreview {
    let translations = page
        .rows
        .iter()
        .filter_map(|row| row.bible)
        .filter_map(|at| page.shelf.bibles.get(at).map(|bible| bible.short.clone()))
        .collect::<Vec<_>>()
        .join(", ");
    let showing = page
        .slots
        .map(|slots| slots[page.showing])
        .and_then(|slot| show.presentations().get(slot))
        .filter(|passage| !page.shown.is_empty() && passage.slide_count() > 0)
        .cloned();
    let sample = showing.is_none();
    let passage = match showing {
        Some(passage) => Some(passage),
        None => page.sample().cloned().map(std::sync::Arc::new),
    };
    let Some(passage) = passage else {
        return crate::bible::SlidesPreview {
            first: None,
            last: None,
            count: 0,
            name: String::new(),
            sample,
            translations,
        };
    };
    let slides: Vec<&sp_model::Slide> = passage.slides().map(|(_, slide)| slide).collect();
    let mut picture = |slide: Option<&&sp_model::Slide>| {
        slide.and_then(|slide| thumb(ThumbOf::BibleSlide(slide))).map(|(texture, _)| texture)
    };
    let first = picture(slides.first());
    let last = if slides.len() > 1 { picture(slides.last()) } else { None };
    crate::bible::SlidesPreview { first, last, count: slides.len(), name: passage.title.clone(), sample, translations }
}

/// The rulers along the top and left of the Edit view's canvas, in the slide's own pixels: a
/// mark every 100, numbered as often as there is room for, with the selected object's extent
/// shown on each.
/// Books, which shows or hides the book, chapter and verse lists, and History, the passages
/// that were on screen.
fn bible_nav(ui: &mut egui::Ui, state: &mut HomeState, live: Option<usize>) {
    // The book, chapter and verse lists beside the slides, for when a passage is
    // easier to pick than type.
    let mut chosen = None;
    if ui
        .add(egui::Button::new("Books").selected(state.bible.lists_open))
        .on_hover_text("Show or hide the book, chapter and verse lists")
        .clicked()
    {
        state.bible.lists_open = !state.bible.lists_open;
        state.bible.list_scroll = true;
    }
    // The passages that were on screen, newest first: one click brings one back.
    ui.menu_button("History", |ui| {
        if state.bible.history.is_empty() {
            ui.label(egui::RichText::new("Passages shown on screen are listed here.").color(crate::studio::TEXT_DIM));
            return;
        }
        let today = chrono::Local::now().date_naive();
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for entry in &state.bible.history {
                let button = egui::Button::new(entry.passage.as_str())
                    .right_text(egui::RichText::new(history_when(&entry.at, today)).color(crate::studio::TEXT_DIM))
                    .min_size(vec2(260.0, 0.0));
                if ui.add(button).clicked() {
                    chosen = Some(entry.passage.clone());
                }
            }
        });
        ui.separator();
        if ui.button("Clear History").clicked() {
            state.bible.history.clear();
            state.bible.history_changed = true;
            ui.close();
        }
    })
    .response
    .on_hover_text("Passages shown on screen, newest first");
    if let Some(reference) = chosen {
        state.bible.lookup = reference;
        state.bible.look_up(live);
        ui.close();
    }
}

/// The Bible page's actions, laid out from the right: Options, then keeping the passage in the
/// playlist in front (or another playlist), to open again with its settings.
fn bible_actions(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
) {
    let button = ui
        .add(egui::Button::selectable(state.bible.options.open, "Options"))
        .on_hover_text("How verses become slides, and which Bibles are installed");
    if button.clicked() {
        state.bible.options.open = !state.bible.options.open;
    }
    if state.bible.options.open {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.bible.options.open = false;
        }
        let preview = bible_preview(show, &mut state.bible, thumb);
        let bible = &mut state.bible;
        let picked = crate::bible::options_popup(
            ui.ctx(),
            &mut bible.options,
            button.rect,
            &crate::bible::ThemeChoice {
                names: &state.theme_names,
                template_names: &state.template_names,
                layouts: &state.theme_layouts,
                theme: state.theme_for_bible,
                layout: state.bible_layout,
            },
            &preview,
        );
        if let Some((index, layout)) = picked {
            state.theme_request = Some(ThemeRequest::UseForBible(index, layout));
        }
        if let Some(index) = state.bible.options.remove.take() {
            state.bible.remove_bible(index);
        }
    }
    // One click each: the passage into the playlist in front (named on the
    // button; the arrow beside it picks another), or a presentation of its own.
    let saved = state.bible.saved();
    let front = state.services.front().name.clone();
    // Laid out right to left: the arrow, to pick another playlist, sits after Add to.
    if state.services.list.len() > 1 {
        let other = ui.add_enabled(saved.is_some(), egui::Button::new("▾")).on_hover_text("Add to another playlist");
        egui::Popup::from_toggle_button_response(&other)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let Some(saved) = saved.clone() else { return };
                let names: Vec<String> = state.services.list.iter().map(|playlist| playlist.name.clone()).collect();
                for (at, name) in names.iter().enumerate() {
                    if ui.button(format!("Add to \u{201c}{name}\u{201d}")).clicked() {
                        state.services.add_to(at, crate::service::Item::Passage(saved.clone()));
                        state.notice = Some(format!("Added {} to {name}.", saved.lookup));
                        ui.close();
                    }
                }
            });
    }
    let add = ui
        .add_enabled(saved.is_some(), egui::Button::new(format!("Add to \u{201c}{front}\u{201d}")))
        .on_hover_text("Put this passage in the playlist, to open again with these settings")
        .on_disabled_hover_text("Look a passage up first");
    if add.clicked()
        && let Some(saved) = saved.clone()
    {
        state.notice = Some(format!("Added {} to {front}.", saved.lookup));
        state.services.add(crate::service::Item::Passage(saved));
    }
}

/// The right end of a Bible row: + to add a translation (on the first row) or − to remove this
/// one, and License, which shows the Bible's name, abbreviation and terms.
fn bible_row_extras(
    ui: &mut egui::Ui,
    index: usize,
    count: usize,
    shelved: Option<&crate::bible::Shelved>,
    add: &mut bool,
    remove: &mut Option<usize>,
) {
    if index == 0 {
        if ui
            .add_enabled(count < BiblePage::MAX_ROWS, egui::Button::new("+"))
            .on_hover_text("Add a translation")
            .clicked()
        {
            *add = true;
        }
    } else if ui.button("−").on_hover_text("Remove this translation").clicked() {
        *remove = Some(index);
    }
    // License: a small (i), which opens the Bible's name, abbreviation and terms.
    let (mark, license) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let ink = if license.hovered() { Color32::WHITE } else { DIM };
    ui.painter().circle_stroke(mark.center(), 8.0, Stroke::new(1.2, ink));
    ui.painter().text(mark.center(), Align2::CENTER_CENTER, "i", crate::studio::semibold(11.5), ink);
    let license = license.on_hover_text("License: who holds the rights to this translation, and its terms");
    egui::Popup::from_toggle_button_response(&license)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(300.0);
            if let Some(bible) = shelved {
                egui::Grid::new(("license", index)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    for (label, value) in [
                        ("Bible", bible.name.as_str()),
                        ("Abbreviation", bible.short.as_str()),
                        ("Copyright", bible.terms.as_str()),
                        ("Credit", bible.credit.as_str()),
                    ] {
                        if value.is_empty() {
                            continue;
                        }
                        ui.label(egui::RichText::new(label).color(DIM));
                        ui.add(egui::Label::new(value).wrap());
                        ui.end_row();
                    }
                });
            }
        });
}

fn rulers(painter: &egui::Painter, area: Rect, ruler: f32, slide: Rect, scale: f32, selected: Option<SlideRect>) {
    let band = Color32::from_rgb(12, 12, 12);
    let top = Rect::from_min_max(pos2(area.min.x + ruler, area.min.y), pos2(area.max.x, area.min.y + ruler));
    let left = Rect::from_min_max(pos2(area.min.x, area.min.y + ruler), pos2(area.min.x + ruler, area.max.y));
    painter.rect_filled(Rect::from_min_size(area.min, vec2(area.width(), ruler)), 0.0, band);
    painter.rect_filled(Rect::from_min_size(area.min, vec2(ruler, area.height())), 0.0, band);
    if let Some(frame) = selected {
        let shade = crate::studio::ACCENT.gamma_multiply(0.45);
        let across = Rect::from_x_y_ranges(
            slide.min.x + frame.x * scale..=slide.min.x + (frame.x + frame.w) * scale,
            top.y_range(),
        );
        let down = Rect::from_x_y_ranges(
            left.x_range(),
            slide.min.y + frame.y * scale..=slide.min.y + (frame.y + frame.h) * scale,
        );
        painter.with_clip_rect(top).rect_filled(across, 0.0, shade);
        painter.with_clip_rect(left).rect_filled(down, 0.0, shade);
    }
    // Numbers every 100, 200, 500 or 1000 pixels, whichever leaves room between them.
    let every = [100.0, 200.0, 500.0, 1000.0].into_iter().find(|step| step * scale >= 46.0).unwrap_or(1000.0);
    let ink = Color32::from_gray(150);
    let font = FontId::proportional(9.5);
    let marks = |size: f32| (0..=(size / 100.0) as usize).map(|n| n as f32 * 100.0);
    let top_painter = painter.with_clip_rect(top);
    for at in marks(sp_model::SLIDE_WIDTH) {
        let x = slide.min.x + at * scale;
        let numbered = (at % every).abs() < 0.5;
        let reach = if numbered { 0.0 } else { ruler * 0.6 };
        top_painter.line_segment([pos2(x, top.min.y + reach), pos2(x, top.max.y)], Stroke::new(1.0, ink));
        if numbered {
            top_painter.text(pos2(x + 3.0, top.min.y + 1.0), Align2::LEFT_TOP, format!("{at:.0}"), font.clone(), ink);
        }
    }
    let left_painter = painter.with_clip_rect(left);
    for at in marks(sp_model::SLIDE_HEIGHT) {
        let y = slide.min.y + at * scale;
        let numbered = (at % every).abs() < 0.5;
        let reach = if numbered { 0.0 } else { ruler * 0.6 };
        left_painter.line_segment([pos2(left.min.x + reach, y), pos2(left.max.x, y)], Stroke::new(1.0, ink));
        if numbered {
            // Turned on its side, reading upward, as a Mac's rulers number their side.
            let galley = left_painter.layout_no_wrap(format!("{at:.0}"), font.clone(), ink);
            let start = pos2(left.min.x + 1.0, y - 3.0);
            left_painter.add(egui::epaint::TextShape::new(start, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2));
        }
    }
}

/// The Arrangement button beside a song's title: play the song as written or in one of its
/// arrangements, make a new one, rename, duplicate or delete the one in use.
/// A small icon button in the song's header, lit while what it shows is on.
fn header_icon(ui: &mut egui::Ui, icon: crate::icons::Icon, hint: &str, on: bool) -> egui::Response {
    let (cell, response) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
    if on {
        ui.painter().rect_filled(cell, 6.0, crate::studio::SELECTED);
    } else if response.hovered() {
        ui.painter().rect_filled(cell, 6.0, crate::studio::HOVER);
    }
    let ink = if on || response.hovered() { crate::studio::TEXT } else { crate::studio::TEXT_DIM };
    crate::icons::paint(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(15.0, 15.0)), ink);
    response.on_hover_text(hint)
}

fn arrangement_menu(index: usize, song: &sp_model::Presentation, state: &mut HomeState, button: &egui::Response) {
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

/// The groups of the arrangement in use, in the order they are played. The one on screen is
/// lit; a click goes to it; its menu moves or removes it; + adds a group at the end.
fn arrangement_strip(
    ui: &mut egui::Ui,
    show: &Show,
    song: &sp_model::Presentation,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
) {
    let Some(arrangement) = song.active_arrangement() else { return };
    let index = show.selected();
    // Each entry: its place in the arrangement, its group, and the steps of the play order it covers.
    let mut step = 0;
    let mut chips = Vec::new();
    for (at, id) in arrangement.groups.iter().enumerate() {
        let Some(group) = song.groups.iter().find(|group| group.id == *id) else { continue };
        chips.push((at, group, step));
        step += group.slides.len();
    }
    let here = show.cursor().filter(|cursor| cursor.presentation == index).and(show.position()).map(|(step, _)| step);
    let mut groups = arrangement.groups.clone();
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let count = chips.len();
        for (chip, (at, group, first)) in chips.iter().enumerate() {
            let live = here.is_some_and(|step| step >= *first && step < first + group.slides.len());
            let ink = color32(group.color);
            let words = egui::RichText::new(group.name.as_str()).size(12.0).color(Color32::WHITE);
            let button = egui::Button::new(words)
                .fill(if live { ink } else { ink.gamma_multiply(0.45) })
                .stroke(if live { Stroke::new(1.5, Color32::WHITE) } else { Stroke::NONE })
                .corner_radius(5.0)
                .min_size(vec2(0.0, 22.0));
            let response = ui.add(button).on_hover_text("Click to show this part; right-click to move or remove it");
            if response.clicked() {
                commands.push(Command::GoToStep { presentation: index, step: *first });
            }
            response.context_menu(|ui| {
                if ui.add_enabled(chip > 0, egui::Button::new("Move Left")).clicked() {
                    groups.swap(*at, chips[chip - 1].0);
                    changed = true;
                    ui.close();
                }
                if ui.add_enabled(chip + 1 < count, egui::Button::new("Move Right")).clicked() {
                    groups.swap(*at, chips[chip + 1].0);
                    changed = true;
                    ui.close();
                }
                if ui.button("Repeat").on_hover_text("Play this part again straight after").clicked() {
                    groups.insert(*at + 1, group.id);
                    changed = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Remove").clicked() {
                    groups.remove(*at);
                    changed = true;
                    ui.close();
                }
            });
        }
        let add = ui.small_button("+").on_hover_text("Add a group to the end");
        egui::Popup::menu(&add).show(|ui| {
            for group in &song.groups {
                if ui.button(group.name.as_str()).clicked() {
                    groups.push(group.id);
                    changed = true;
                }
            }
        });
    });
    if changed {
        let mut arrangements = song.arrangements.clone();
        if let Some(edited) = arrangements.iter_mut().find(|have| have.id == arrangement.id) {
            edited.groups = groups;
        }
        state.arrangement_request = Some((index, arrangements, song.arrangement));
    }
}

/// The middle: the selected song's name and how its slides are laid out, then the slides.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        let Some(presentation) = show.presentations().get(show.selected()) else {
            ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Open a song to see its slides.").color(DIM)));
            return;
        };
        // The Reflow editor takes the slides' place while it is open.
        if state.reflow.as_ref().is_some_and(|(index, _)| *index == show.selected()) {
            reflow_editor(ui, &presentation.title, state);
            return;
        }
        let heading = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 12, bottom: 6 });
        egui::Panel::top("song header").resizable(false).frame(heading).show(ui, |ui| {
            ui.horizontal(|ui| {
                // The title; a double click, or a new song, turns it into a field to retype it.
                if let Some(title) = state.renaming_song.as_mut() {
                    // The field takes the title's own room, growing as it is typed, as a file
                    // name does in the Finder.
                    let font = crate::studio::semibold(18.0);
                    let width = ui.painter().layout_no_wrap(title.clone(), font.clone(), Color32::WHITE).size().x;
                    let line = ui.fonts_mut(|fonts| fonts.row_height(&font));
                    let (place, _) = ui.allocate_exact_size(vec2(width + TITLE_ROOM, line), Sense::hover());
                    let field = crate::studio::rename_field_at(
                        ui,
                        title,
                        font,
                        crate::studio::TEXT,
                        crate::studio::ACCENT,
                        place.left_center(),
                        place.max.x,
                    );
                    if std::mem::take(&mut state.rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        state.renaming_song = None;
                    } else if enter || field.lost_focus() {
                        if !title.trim().is_empty() && title.trim() != presentation.title {
                            state.pending_title = Some((show.selected(), title.trim().to_owned()));
                        }
                        state.renaming_song = None;
                    }
                } else {
                    let title = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(presentation.title.as_str()).font(crate::studio::semibold(18.0)),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_text("Double-click to rename");
                    // The same room the field takes, so renaming moves nothing.
                    ui.add_space(TITLE_ROOM);
                    if title.double_clicked() {
                        state.renaming_song = Some(presentation.title.clone());
                        state.rename_started = true;
                    }
                }
                let slides = presentation.slide_count();
                let count = if slides == 1 { "1 slide".to_owned() } else { format!("{slides} slides") };
                ui.label(egui::RichText::new(count).size(12.0).color(crate::studio::TEXT_FAINT));
                let from_bible = state.app_made(show.selected());
                // Every slide's text is looked through again only when the song, its theme or
                // the fonts installed change.
                let epoch = state.lists.songs.epoch();
                let song = show.selected();
                let kept = state.lists.missing_fonts.get(|(made_in, made_for, look, fonts)| {
                    *made_in == epoch
                        && *made_for == song
                        && look == theme
                        && std::sync::Arc::ptr_eq(fonts, &state.fonts)
                });
                let missing = match kept {
                    Some(missing) => missing,
                    None => {
                        let missing = sp_model::missing_fonts(&sp_model::fonts_used(presentation, theme), &state.fonts);
                        state.lists.missing_fonts.keep((epoch, song, theme.clone(), state.fonts.clone()), missing)
                    }
                };
                missing_font_notice(ui, &missing);
                // The song's tools, as small icons at the right, lit
                // while what they show is on. Laid out right to left: Notes is rightmost.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if header_icon(
                        ui,
                        crate::icons::Icon::Notes,
                        "Notes: show the notes of the slide on screen",
                        state.show_notes,
                    )
                    .clicked()
                    {
                        state.show_notes = !state.show_notes;
                    }
                    if !from_bible {
                        let on = presentation.active_arrangement().is_some();
                        let button = header_icon(
                            ui,
                            crate::icons::Icon::Arrangement,
                            "Arrangement: play the song's groups in another order",
                            on,
                        );
                        arrangement_menu(show.selected(), presentation, state, &button);
                    }
                    if !from_bible
                        && header_icon(
                            ui,
                            crate::icons::Icon::Reflow,
                            "Reflow: edit the song's words as text (Control-R)",
                            false,
                        )
                        .clicked()
                    {
                        state.reflow = Some((show.selected(), sp_model::to_song_text(presentation)));
                    }
                    if !from_bible {
                        let info = header_icon(
                            ui,
                            crate::icons::Icon::Status,
                            "Info: the song's writer, publisher and CCLI number",
                            false,
                        );
                        egui::Popup::from_toggle_button_response(&info)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                ui.set_width(320.0);
                                let mut details = presentation.copyright.clone();
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
                                        .size(11.0)
                                        .color(DIM),
                                );
                                if details != presentation.copyright {
                                    state.copyright_request = Some((show.selected(), details));
                                }
                            });
                    }
                });
            });
        });
        // The arrangement the song is played in, as a row of its groups.
        if presentation.active_arrangement().is_some() {
            let strip = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 0, bottom: 8 });
            egui::Panel::top("arrangement strip")
                .resizable(false)
                .frame(strip)
                .show(ui, |ui| arrangement_strip(ui, show, presentation, state, commands));
        }
        // The bar under the slides: Add Slide on the left; the
        // transition, the slide view and the slide size on the right.
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
            let strip = Frame::new().fill(crate::studio::CARD).inner_margin(egui::Margin::symmetric(20, 8));
            egui::Panel::top("operator notes").resizable(false).frame(strip).show(ui, |ui| {
                let (text, ink) = match notes {
                    Some(notes) => (notes, crate::studio::TEXT),
                    None => (
                        "No notes for this slide. Add them in the editor with Slide Notes.".to_owned(),
                        crate::studio::TEXT_FAINT,
                    ),
                };
                ui.label(egui::RichText::new(text).size(13.0).color(ink));
            });
        }
        slide_grid(ui, show, show.selected(), state, thumb, commands);
    });
}

/// The outline of the slide on screen, of a slide picked out by a right-click, and of the
/// slide that was last on screen once the screen has been cleared.
const LIVE_OUTLINE: Color32 = crate::studio::TALLY;
const SELECTED_OUTLINE: Color32 = crate::studio::ACCENT;
const WAS_LIVE_OUTLINE: Color32 = Color32::from_rgb(110, 60, 58);

/// One presentation's slides, in the view chosen: a grid of thumbnails, the same grid showing
/// only words, or a table. Clicking a slide puts it on screen; a right-click picks it out
/// without showing it.
/// The passage the Options panel previews when none is showing: two verses, so that one verse
/// a slide, and the reference on the last slide only, can be seen to change it.
const SAMPLE_PASSAGE: &str = "John 3:16-17";

/// How many passages the Bible history keeps.
const HISTORY_LENGTH: usize = 100;

/// When a passage in the history was on screen: the time for today ("10:42 AM"), the day
/// before that ("Sun 17 Nov").
fn history_when(at: &str, today: chrono::NaiveDate) -> String {
    match chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S") {
        Ok(at) if at.date() == today => at.format("%-I:%M %p").to_string(),
        Ok(at) => at.format("%a %-d %b").to_string(),
        Err(_) => String::new(),
    }
}

/// How long after typing stops in Scripture Lookup its passage is made, in seconds.
const LOOKUP_PAUSE: f64 = 0.25;

/// Whether what is typed in Scripture Lookup has got as far as a chapter: a number after the
/// book's name. A number before it is part of the name ("1 John").
fn has_chapter(typed: &str) -> bool {
    let name_onwards = typed.trim().trim_start_matches(|c: char| c.is_ascii_digit() || c.is_whitespace());
    name_onwards.chars().any(|c| c.is_ascii_digit())
}

/// The books offered under Scripture Lookup for what is typed: only while a book's name is
/// being typed (no chapter yet), and not when the one book offered is already what is typed.
fn book_suggestions(typed: &str, language: &str) -> Vec<egui::Suggestion> {
    let trimmed = typed.trim();
    let name_onwards = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c.is_whitespace());
    if name_onwards.is_empty() || has_chapter(trimmed) || trimmed.contains(':') {
        return Vec::new();
    }
    let books = sp_bible::suggest_books(trimmed, language, 8);
    if let [(_, name)] = books.as_slice()
        && name.to_lowercase() == trimmed.to_lowercase()
    {
        return Vec::new();
    }
    books.into_iter().map(|(_, name)| egui::Suggestion::new(format!("{name} "))).collect()
}

/// Selects all the words in a text field, as after a lookup, so the next typing replaces them.
fn select_all_text(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut edit) = egui::text_edit::TextEditState::load(ctx, id) {
        let all =
            egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
        edit.cursor.set_char_range(Some(all));
        edit.store(ctx, id);
    }
}

/// The narrowest a slide in the grid gets, in points, before fewer go across.
const SMALLEST_SLIDE: f32 = 150.0;

/// How rounded a slide's corners are in the slide grid, in points.
const SLIDE_ROUND: u8 = 3;

fn slide_grid(
    ui: &mut egui::Ui,
    show: &Show,
    which: usize,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    if show.presentations().get(which).is_none() {
        return;
    }
    let from_bible = state.bible.slots.is_some_and(|slots| slots.contains(&which));
    // A song opened from a playlist is shown with the playlist's other songs above and below
    // it, in the playlist's order, so scrolling down goes on into the next song.
    let sections = if from_bible { Vec::new() } else { playlist_sections(show, state, which) };
    // Slides the app makes (a Bible passage, the VoxLive item) are shown, never changed.
    let from_bible = from_bible || state.app_made(which);
    let view = state.slide_view;
    let gap = 10.0;
    let bar_height = 22.0;
    egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]).show(ui, |ui| {
        // Between the slides: a click picks nothing; a drag draws a box that picks out every
        // slide it touches. Only the grid's own width: a scroll area that scrolls up and down
        // leaves the width of its clip to whatever holds it, which can be wider.
        let visible =
            ui.clip_rect().intersect(Rect::from_x_y_ranges(ui.max_rect().x_range(), ui.clip_rect().y_range()));
        let background = ui.interact(visible, ui.scope_id().with("slide background"), Sense::click_and_drag());
        if background.clicked() {
            state.picked_slides.clear();
            state.selected_slide = None;
        }
        start_slide_box(ui, state, &background);
        scroll_while_dragging(ui);
        if !from_bible {
            slide_clipboard_keys(ui, show, which, state);
        }
        state.slide_rects.clear();
        ui.add_space(10.0);
        Frame::new().inner_margin(egui::Margin::symmetric(20, 0)).show(ui, |ui| {
            // The slider sets how many slides fit across; they stretch to fill the width.
            let across = ui.available_width();
            // Fewer fit across when the room is narrow (a small window, or the Bible page's
            // lists beside them), so no slide is too small to read.
            let fit = ((across + gap) / (SMALLEST_SLIDE + gap)).floor().max(1.0) as usize;
            let columns = if view == SlideView::Table { 1 } else { state.columns.clamp(2, 7).min(fit) };
            let width = ((across - gap * (columns as f32 - 1.0)) / columns as f32).floor().max(40.0);
            let image_size = if view == SlideView::Table {
                vec2(204.0, 204.0 * 9.0 / 16.0)
            } else {
                vec2(width, (width * 9.0 / 16.0).round())
            };
            let cell = if view == SlideView::Table {
                vec2(across, image_size.y)
            } else {
                vec2(width, image_size.y + bar_height)
            };
            let sizes = Cells { view, gap, bar_height, width, image_size, cell };
            if sections.is_empty() {
                song_slides(ui, show, which, None, state, thumb, commands, &sizes);
            } else {
                playlist_slides(ui, show, which, &sections, state, thumb, commands, &sizes);
            }
            // Under the slides: a media item dragged from the bin and dropped here is added as a
            // slide of its own.
            if !from_bible && sections.is_empty() {
                let height = ui.available_height().max(90.0);
                let (place, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
                if response.dnd_hover_payload::<MediaDrag>().is_some() {
                    let zone = Rect::from_min_size(place.min + vec2(0.0, 8.0), vec2(place.width(), 70.0));
                    ui.painter().rect_stroke(zone, 8.0, Stroke::new(2.0, crate::studio::ACCENT), StrokeKind::Inside);
                    ui.painter().text(
                        zone.center(),
                        Align2::CENTER_CENTER,
                        "Drop to add as a slide",
                        FontId::proportional(13.0),
                        crate::studio::TEXT_DIM,
                    );
                }
                if let Some(dropped) = response.dnd_release_payload::<MediaDrag>() {
                    state.media_request = Some(MediaRequest::AddAsSlide(dropped.0));
                }
            }
        });
        ui.add_space(12.0);
        sweep_slides(ui, state);
    });
}

/// Starts a box across the slide area when a drag starts on `response` (a slide, or the space
/// between them). Where it started is kept from the top of the slides, so the box stays put
/// on them as the area scrolls.
fn start_slide_box(ui: &egui::Ui, state: &mut HomeState, response: &egui::Response) {
    if !response.drag_started() {
        return;
    }
    let Some(at) = ui.input(|input| input.pointer.press_origin()) else { return };
    let mods = ui.input(|input| input.modifiers);
    if !mods.command && !mods.shift {
        state.picked_slides.clear();
    }
    state.selected_slide = None;
    state.picking = Picking::Slides;
    let top = ui.max_rect().min;
    state.sweep =
        Some(Sweep { list: Picking::Slides, from: None, start: (at - top).to_pos2(), keeps: false, carried: false });
}

/// Carries on a box dragged across the slide area: every slide it touches is picked out, and
/// the area scrolls when the pointer nears its top or bottom.
fn sweep_slides(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(sweep) = state.sweep.filter(|sweep| sweep.list == Picking::Slides) else { return };
    let (pointer, down) = ui.input(|input| (input.pointer.interact_pos(), input.pointer.primary_down()));
    if !down {
        state.sweep = None;
        return;
    }
    let Some(at) = pointer else { return };
    let top = ui.max_rect().min;
    let start = top + sweep.start.to_vec2();
    if at.distance(start) < 3.0 {
        return;
    }
    let band = Rect::from_two_pos(start, at);
    let touched: Vec<SlideRef> =
        state.slide_rects.iter().filter(|(_, rect)| rect.intersects(band)).map(|(here, _)| *here).collect();
    state.picked_slides = touched;
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("slide band")));
    let clip = ui.clip_rect();
    painter.with_clip_rect(clip).rect_filled(band, 2.0, crate::studio::ACCENT.gamma_multiply(0.12));
    painter.with_clip_rect(clip).rect_stroke(
        band,
        2.0,
        Stroke::new(1.0, crate::studio::ACCENT.gamma_multiply(0.8)),
        StrokeKind::Inside,
    );
    let edge = 30.0;
    if at.y < clip.min.y + edge {
        ui.scroll_with_delta(vec2(0.0, 12.0));
        ui.ctx().request_repaint();
    } else if at.y > clip.max.y - edge {
        ui.scroll_with_delta(vec2(0.0, -12.0));
        ui.ctx().request_repaint();
    }
}

/// Picks out a slide as a Command- or Shift-click does: Command adds or takes away one; Shift
/// picks the run from the last one picked to it, in the order the slides are shown.
/// Picks out slides as the Finder does: Shift picks every slide from the last one picked (or,
/// with none, from `anchor`, the slide clicked last) to this one; Command picks this one out, or
/// puts it back.
fn pick_slide(
    picked: &mut Vec<SlideRef>,
    order: &[SlideRef],
    here: SlideRef,
    mods: egui::Modifiers,
    anchor: Option<SlideRef>,
) {
    if mods.shift
        && let Some(from) = picked.last().copied().or(anchor).and_then(|last| order.iter().position(|at| *at == last))
        && let Some(to) = order.iter().position(|at| *at == here)
    {
        // As in the lists of songs and media: the slides between, and only those, the one it
        // ran from last so a further Shift-click runs from the same place.
        let (a, b) = (from.min(to), from.max(to));
        let start = order[from];
        *picked = order[a..=b].iter().copied().filter(|at| *at != start).collect();
        picked.push(start);
        return;
    }
    match picked.iter().position(|at| *at == here) {
        Some(at) => {
            picked.remove(at);
        }
        None => picked.push(here),
    }
}

/// The sizes of the slides in the slide area.
#[derive(Clone, Copy)]
struct Cells {
    view: SlideView,
    gap: f32,
    bar_height: f32,
    width: f32,
    image_size: egui::Vec2,
    cell: egui::Vec2,
}

/// One part of a playlist shown in the slide area.
enum Section {
    /// A playlist header, grouping the items under it.
    Header(String),
    /// A song: its place in the playlist and among the songs.
    Song { item: usize, at: usize },
    /// A song the library doesn't have open, or a Bible passage: shown by name, and a click
    /// on a passage opens it.
    Other { item: usize, title: String, passage: bool },
    /// A picture or video from the media bin: `at` is its place there, while it is there.
    Media { item: usize, at: Option<usize>, name: String },
    /// A media playlist: the first of its pictures and videos in the bin, how many are there,
    /// and whether the playlist itself has gone.
    MediaPlaylist { item: usize, id: sp_model::Uuid, name: String, first: Option<usize>, count: usize, gone: bool },
    /// A place kept for something to come.
    Placeholder { item: usize, name: String },
}

/// What the slide area shows for the song `which` when it was opened from the playlist in
/// front: every item of the playlist, in order. Empty when it wasn't (a song picked in a
/// library, or the playlist no longer has it); then only its own slides are shown.
fn playlist_sections(show: &Show, state: &mut HomeState, which: usize) -> Vec<Section> {
    use crate::service::Item;
    let Some(chosen) = state.services.chosen else { return Vec::new() };
    let items = state.services.front().items.clone();
    // The song each item names, looked up again only when the songs or the items change.
    let epoch = state.lists.songs.epoch();
    let songs = match state.lists.playlist_songs.get(|(made_in, made_of)| *made_in == epoch && *made_of == items) {
        Some(songs) => songs,
        None => {
            let songs = items
                .iter()
                .map(|item| match item {
                    Item::Song { key, .. } => crate::studio::playlist_song(show, state, key),
                    _ => None,
                })
                .collect();
            state.lists.playlist_songs.keep((epoch, items.clone()), songs)
        }
    };
    // A picture or video, a media playlist or a placeholder chosen in the playlist is shown
    // with the rest of the playlist, whatever song is open: it has no slides of its own to open.
    let other =
        matches!(items.get(chosen), Some(Item::Media { .. } | Item::MediaPlaylist { .. } | Item::Placeholder { .. }));
    // The playlist row lit up follows the song shown, as it changes by a click on another
    // song's slide or by Next going on into the next song.
    if !other && songs.get(chosen).copied().flatten() != Some(which) {
        let after = songs.iter().skip(chosen).position(|at| *at == Some(which)).map(|i| i + chosen);
        match after.or_else(|| songs.iter().position(|at| *at == Some(which))) {
            Some(item) => state.services.chosen = Some(item),
            None => return Vec::new(),
        }
    }
    items
        .into_iter()
        .zip(songs.iter().copied())
        .enumerate()
        .map(|(item, (entry, at))| match (entry, at) {
            (Item::Header { name }, _) => Section::Header(name),
            (Item::Media { id, name }, _) => {
                Section::Media { item, at: show.media().iter().position(|media| media.id == id), name }
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
                let (first, count, gone) = (places.first().copied(), places.len(), playlist.is_none());
                Section::MediaPlaylist { item, id, name, first, count, gone }
            }
            (Item::Placeholder { name }, _) => Section::Placeholder { item, name },
            (_, Some(at)) => Section::Song { item, at },
            (entry, None) => {
                Section::Other { item, title: entry.title().to_owned(), passage: matches!(entry, Item::Passage(_)) }
            }
        })
        .collect()
}

/// Every item of the playlist in front, one after another: each song under a bar with its
/// name, then its slides. The song being shown is scrolled to when it changes.
#[allow(clippy::too_many_arguments)]
fn playlist_slides(
    ui: &mut egui::Ui,
    show: &Show,
    which: usize,
    sections: &[Section],
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
    sizes: &Cells,
) {
    let live_in = show.live().map(|at| at.presentation);
    for (place, section) in sections.iter().enumerate() {
        if place > 0 {
            ui.add_space(14.0);
        }
        match section {
            Section::Header(name) => {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                ui.painter().text(
                    row.left_center(),
                    Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    crate::studio::semibold(11.5),
                    crate::studio::TEXT_DIM,
                );
                ui.painter().hline(row.x_range(), row.max.y - 1.0, Stroke::new(1.0, crate::studio::LINE));
            }
            Section::Other { item, title, passage } => {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let fill = if response.hovered() && *passage { crate::studio::HOVER } else { crate::studio::PANEL };
                ui.painter().rect_filled(row, 6.0, fill);
                let words = if *passage {
                    format!("{title}  ·  Bible passage, click to open it")
                } else {
                    format!("{title}  ·  not open in the library")
                };
                ui.painter().text(
                    row.left_center() + vec2(12.0, 0.0),
                    Align2::LEFT_CENTER,
                    words,
                    FontId::proportional(13.0),
                    crate::studio::TEXT_FAINT,
                );
                if *passage
                    && response.clicked()
                    && let Some(crate::service::Item::Passage(saved)) = state.services.front().items.get(*item).cloned()
                {
                    state.services.chosen = Some(*item);
                    state.bible_open = true;
                    state.bible.open_saved(&saved, live_in);
                }
            }
            Section::Media { item, at, name } => {
                let chosen = state.services.chosen == Some(*item);
                let video = at.is_some_and(|at| show.media().get(at).is_some_and(|m| crate::media::is_video(&m.path)));
                let live = at.is_some() && *at == show.live_media();
                let kind = match at {
                    None => "no longer in the media bin",
                    Some(_) if video => "video",
                    Some(_) => "picture",
                };
                let bar = item_bar(ui, name, kind, chosen, live);
                follow_chosen(ui, state, *item, chosen, bar.rect);
                if bar.clicked() {
                    state.services.chosen = Some(*item);
                }
                ui.add_space(8.0);
                let picture = at.and_then(|at| thumb(ThumbOf::Media(at))).map(|(texture, _)| texture);
                let words = match at {
                    None => "Gone from the media bin",
                    Some(_) if live => "On screen",
                    Some(_) => "Click to show",
                };
                let badge = if video { Some(crate::icons::Icon::Play) } else { None };
                let tile = item_tile(ui, sizes, picture, words, live, at.is_none(), badge);
                if let Some(at) = at
                    && tile.on_hover_text("Show it on the screens").clicked()
                {
                    state.services.chosen = Some(*item);
                    commands.push(Command::ShowMedia(*at));
                }
            }
            Section::MediaPlaylist { item, id, name, first, count, gone } => {
                let chosen = state.services.chosen == Some(*item);
                let live = state.media_playing == Some(*id);
                let kind = if *gone {
                    "media playlist, deleted".to_owned()
                } else {
                    let items = if *count == 1 { "1 item".to_owned() } else { format!("{count} items") };
                    format!("media playlist \u{b7} {items}, round and round")
                };
                let bar = item_bar(ui, name, &kind, chosen, live);
                follow_chosen(ui, state, *item, chosen, bar.rect);
                if bar.clicked() {
                    state.services.chosen = Some(*item);
                }
                ui.add_space(8.0);
                let picture = first.and_then(|at| thumb(ThumbOf::Media(at))).map(|(texture, _)| texture);
                let empty = *gone || *count == 0;
                let words = if *gone {
                    "Deleted"
                } else if *count == 0 {
                    "Nothing in it yet"
                } else if live {
                    "Playing"
                } else {
                    "Click to play"
                };
                let tile = item_tile(ui, sizes, picture, words, live, empty, Some(crate::icons::Icon::Loop));
                if !empty
                    && tile
                        .on_hover_text("Play its pictures (8 seconds each) and videos in turn, round and round")
                        .clicked()
                {
                    state.services.chosen = Some(*item);
                    state.media_request = Some(MediaRequest::PlayPlaylist(Some(*id)));
                }
            }
            Section::Placeholder { item, name } => {
                let chosen = state.services.chosen == Some(*item);
                let bar = item_bar(ui, name, "placeholder", chosen, false);
                follow_chosen(ui, state, *item, chosen, bar.rect);
                if bar.clicked() {
                    state.services.chosen = Some(*item);
                }
                ui.add_space(8.0);
                let words = "To come: drop a picture or video here";
                let tile = item_tile(ui, sizes, None, words, false, true, None);
                // A picture or video dropped on it takes its place.
                if tile.dnd_hover_payload::<MediaDrag>().is_some() {
                    ui.painter().rect_stroke(
                        tile.rect,
                        6.0,
                        Stroke::new(2.0, crate::studio::ACCENT),
                        StrokeKind::Inside,
                    );
                }
                if let Some(dropped) = tile.dnd_release_payload::<MediaDrag>()
                    && let Some(media) = show.media().get(dropped.0)
                {
                    let filled = crate::service::Item::Media { id: media.id, name: media.name.clone() };
                    state.services.fill(*item, filled);
                }
                // Or a song chosen from a list.
                let button = ui
                    .put(
                        Rect::from_center_size(tile.rect.center() + vec2(0.0, 22.0), vec2(150.0, 26.0)),
                        egui::Button::new("Choose a Song\u{2026}"),
                    )
                    .on_hover_text("Fill the placeholder with a song from the library");
                let mut picked = None;
                egui::Popup::from_toggle_button_response(&button)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| picked = crate::studio::song_picker(ui, show, state));
                if let Some(song) = picked
                    && let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song))
                {
                    let filled = crate::service::Item::Song { key: key.clone(), title: found.title.clone() };
                    if state.services.fill(*item, filled) {
                        commands.push(Command::Select(song));
                    }
                }
            }
            Section::Song { item, at } => {
                let Some(song) = show.presentations().get(*at) else { continue };
                let shown = *at == which && state.services.chosen == Some(*item);
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                let fill = if shown { crate::studio::SELECTED } else { crate::studio::PANEL };
                ui.painter().rect_filled(row, 6.0, fill);
                let title = ui.painter().text(
                    row.left_center() + vec2(12.0, 0.0),
                    Align2::LEFT_CENTER,
                    song.title.as_str(),
                    crate::studio::semibold(14.0),
                    crate::studio::TEXT,
                );
                let slides = song.slide_count();
                let count = if slides == 1 { "1 slide".to_owned() } else { format!("{slides} slides") };
                ui.painter().text(
                    pos2(title.max.x + 10.0, row.center().y),
                    Align2::LEFT_CENTER,
                    count,
                    FontId::proportional(11.5),
                    crate::studio::TEXT_FAINT,
                );
                if live_in == Some(*at) {
                    ui.painter().circle_filled(row.right_center() - vec2(14.0, 0.0), 4.0, crate::studio::TALLY);
                }
                if response.on_hover_text("Open this song").clicked() {
                    state.services.chosen = Some(*item);
                    state.slides_picked_here = true;
                    commands.push(Command::Select(*at));
                }
                // When the song shown changes (a click in the playlist, or Next going on into
                // it), its bar is scrolled to the top; not when it was picked here, in view.
                if shown && state.slides_scrolled_to != Some(*item) {
                    if !std::mem::take(&mut state.slides_picked_here) {
                        ui.scroll_to_rect(row, Some(Align::TOP));
                    }
                    state.slides_scrolled_to = Some(*item);
                }
                ui.add_space(8.0);
                song_slides(ui, show, *at, Some(*item), state, thumb, commands, sizes);
            }
        }
    }
}

/// The bar over a picture or video, a media playlist or a placeholder in the playlist shown in
/// the slide area: its name and what kind of item it is, lit while it is the item chosen, with
/// a red dot while it is on the screens.
fn item_bar(ui: &mut egui::Ui, name: &str, kind: &str, chosen: bool, live: bool) -> egui::Response {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let fill = if chosen { crate::studio::SELECTED } else { crate::studio::PANEL };
    ui.painter().rect_filled(row, 6.0, fill);
    let title = ui.painter().text(
        row.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        crate::studio::semibold(14.0),
        crate::studio::TEXT,
    );
    ui.painter().text(
        pos2(title.max.x + 10.0, row.center().y),
        Align2::LEFT_CENTER,
        kind,
        FontId::proportional(11.5),
        crate::studio::TEXT_FAINT,
    );
    if live {
        ui.painter().circle_filled(row.right_center() - vec2(14.0, 0.0), 4.0, crate::studio::TALLY);
    }
    response
}

/// Scrolls a playlist item's bar to the top of the slide area when it becomes the item chosen
/// (by a click in the playlist, or ↑ and ↓), as a song's bar is.
fn follow_chosen(ui: &mut egui::Ui, state: &mut HomeState, item: usize, chosen: bool, bar: Rect) {
    if chosen && state.slides_scrolled_to != Some(item) {
        if !std::mem::take(&mut state.slides_picked_here) {
            ui.scroll_to_rect(bar, Some(Align::TOP));
        }
        state.slides_scrolled_to = Some(item);
    }
}

/// The one tile of a picture or video, a media playlist or a placeholder, the size of a slide:
/// its picture (or `words` alone, when `empty`), a mark in its corner, and `words` under it
/// (beside it in Table view). Outlined in red while it is on the screens.
fn item_tile(
    ui: &mut egui::Ui,
    sizes: &Cells,
    picture: Option<TextureId>,
    words: &str,
    live: bool,
    empty: bool,
    badge: Option<crate::icons::Icon>,
) -> egui::Response {
    let Cells { view, bar_height, width, image_size, cell, .. } = *sizes;
    let sense = if empty { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(cell, sense);
    let image = Rect::from_min_size(rect.min, image_size);
    let round = f32::from(SLIDE_ROUND);
    if empty {
        ui.painter().rect_filled(image, round, crate::studio::PANEL);
        ui.painter().rect_stroke(image, round, Stroke::new(1.0, crate::studio::LINE), StrokeKind::Inside);
    } else {
        ui.painter().rect_filled(image, round, Color32::BLACK);
        match picture {
            Some(texture) => {
                egui::Image::from_texture((texture, image_size)).corner_radius(round).paint_at(ui, image);
            }
            None => ui.ctx().request_repaint(),
        }
    }
    if let Some(icon) = badge.filter(|_| !empty) {
        let mark = Rect::from_min_size(image.left_bottom() + vec2(6.0, -22.0), vec2(24.0, 16.0));
        ui.painter().rect_filled(mark, 3.0, Color32::from_black_alpha(170));
        crate::icons::paint(
            ui.painter(),
            icon,
            Rect::from_center_size(mark.center(), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
    }
    let ink = if empty { crate::studio::TEXT_FAINT } else { crate::studio::TEXT_DIM };
    if view == SlideView::Table {
        ui.painter().text(
            pos2(image.max.x + 14.0, image.center().y),
            Align2::LEFT_CENTER,
            words,
            FontId::proportional(13.0),
            ink,
        );
    } else if empty {
        // Wrapped to the tile, centred, a little above the middle (a button may sit under it).
        let mut job =
            egui::text::LayoutJob::simple(words.to_owned(), FontId::proportional(12.0), ink, image.width() - 20.0);
        job.halign = Align::Center;
        let galley = ui.painter().layout_job(job);
        let top = image.center().y - 14.0 - galley.size().y / 2.0;
        ui.painter().galley(pos2(image.center().x, top), galley, ink);
    } else {
        let label = Rect::from_min_size(pos2(rect.min.x, image.max.y), vec2(width, bar_height));
        ui.painter().rect_filled(label, 0.0, crate::studio::CARD);
        ui.painter().text(
            label.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            words,
            FontId::proportional(11.5),
            ink,
        );
    }
    let outline = if live {
        Some(Stroke::new(2.0, crate::studio::TALLY))
    } else if response.hovered() && !empty {
        Some(Stroke::new(1.0, crate::studio::TEXT_FAINT))
    } else {
        None
    };
    if let Some(stroke) = outline {
        ui.painter().rect_stroke(image, round, stroke, StrokeKind::Outside);
    }
    response
}

/// One song's slides, as the view chosen draws them. `item` is the song's place in the
/// playlist in front when the playlist is shown; a click on a slide then lights that row.
#[allow(clippy::too_many_arguments)]
fn song_slides(
    ui: &mut egui::Ui,
    show: &Show,
    which: usize,
    item: Option<usize>,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
    sizes: &Cells,
) {
    let Some(presentation) = show.presentations().get(which) else { return };
    let from_bible = state.bible.slots.is_some_and(|slots| slots.contains(&which));
    // Slides the app makes (a Bible passage, the VoxLive item) are shown, never changed.
    let made = state.app_made(which);
    let Cells { view, gap, bar_height, width, image_size, cell } = *sizes;
    // Slides of this song being dragged to another place in it.
    let moving = egui::DragAndDrop::payload::<SlideDrag>(ui.ctx()).filter(|drag| drag.presentation == which);
    let mut moving_rects: Vec<Rect> = Vec::new();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = if view == SlideView::Table { vec2(0.0, 2.0) } else { vec2(gap, gap) };
        let mut last_group = None;
        for (index, (group, slide)) in presentation.slides().enumerate() {
            // A group's name is shown on its first slide only.
            let starts_group = last_group != Some(group.id);
            last_group = Some(group.id);
            let (rect, response) = ui.allocate_exact_size(cell, Sense::click_and_drag());
            let here = SlideRef { presentation: which, slide: index };
            state.slide_rects.push((here, rect));
            if moving.as_ref().is_some_and(|drag| drag.slides.contains(&index)) {
                moving_rects.push(rect);
            }
            if made {
                // A Bible passage's slides stay in the order they are made: a drag on one draws a
                // box that picks out the slides it touches, as a drag between slides does.
                start_slide_box(ui, state, &response);
            } else {
                // A slide dragged goes to another place in its song, and the slides picked out
                // with it when it is one of them; a line shows where they will go.
                if response.dragged() {
                    let mut picked: Vec<usize> =
                        state.picked_slides.iter().filter(|p| p.presentation == which).map(|p| p.slide).collect();
                    picked.sort_unstable();
                    let slides = if picked.contains(&index) { picked } else { vec![index] };
                    response.dnd_set_drag_payload(SlideDrag { presentation: which, slides });
                }
                let target = |drag: &SlideDrag| drag.presentation == which && !drag.slides.contains(&index);
                if response.dnd_hover_payload::<SlideDrag>().is_some_and(|drag| target(&drag)) {
                    let after = drop_after(ui, rect, view == SlideView::Table);
                    paint_drop_line(ui, rect, after, view == SlideView::Table, gap);
                }
                if let Some(dropped) = response.dnd_release_payload::<SlideDrag>().filter(|drag| target(drag)) {
                    let after = drop_after(ui, rect, view == SlideView::Table);
                    state.slide_request = Some((
                        SlideRef { presentation: which, slide: dropped.slides[0] },
                        SlideRequest::Move { slides: dropped.slides.clone(), to: index, after },
                    ));
                }
            }
            let mods = ui.input(|input| input.modifiers);
            if response.clicked() && (mods.command || mods.shift) {
                // Picked out without going on screen, to delete several at once.
                state.picking = Picking::Slides;
                let order: Vec<SlideRef> = state.slide_rects.iter().map(|(at, _)| *at).collect();
                let anchor = state.selected_slide.or(show.live()).filter(|at| at.presentation == which);
                pick_slide(&mut state.picked_slides, &order, here, mods, anchor);
            } else if response.clicked() {
                state.selected_slide = None;
                state.picked_slides.clear();
                // Delete no longer means the songs or media last picked out.
                state.picking = Picking::Nothing;
                commands.push(Command::GoTo(here));
                // In the playlist, the song's row lights up; the view stays where it is.
                if item.is_some() {
                    state.services.chosen = item;
                    state.slides_picked_here = true;
                }
            }
            // A media item dragged from the bin onto a slide becomes what the slide shows.
            if !made {
                if response.dnd_hover_payload::<MediaDrag>().is_some() {
                    ui.painter().rect_stroke(rect, 6.0, Stroke::new(2.0, crate::studio::ACCENT), StrokeKind::Outside);
                }
                if let Some(dropped) = response.dnd_release_payload::<MediaDrag>()
                    && let Some(item) = show.media().get(dropped.0)
                {
                    state.pending_edit = Some((here, slide_with_media(slide, Some(item.id))));
                }
            }
            if response.secondary_clicked() {
                state.selected_slide = Some(here);
            }
            // Editing is on the right-click menu, so reaching for it never puts a
            // slide on screen by accident.
            response.context_menu(|ui| {
                if ui.button("Edit Slide").clicked() {
                    state.editing = EditDraft::of(show, here);
                    ui.close();
                }
                if !made {
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
                        state.inspector.copied_style =
                            slide.objects.iter().find_map(|object| object.text_style().cloned());
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
                }
                // A disabled slide stays in the song but is stepped over and never shown.
                if !made {
                    let label = if slide.enabled { "Disable Slide" } else { "Enable Slide" };
                    if ui.button(label).clicked() {
                        state.pending_edit = Some((here, Slide { enabled: !slide.enabled, ..slide.clone() }));
                        ui.close();
                    }
                }
                // Which of the theme's templates the slide is drawn with. Automatic
                // lets the theme pick the one that suits what is on the slide.
                if !made && !state.song_templates.is_empty() {
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
            });
            // Slides scrolled out of view cost nothing: no thumbnail is asked for.
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let painter = ui.painter().clone();
            let ink = text_on(group.color);
            let words = || slide.texts().into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n");
            let mut picture = |ui: &mut egui::Ui, image: Rect| {
                painter.rect_filled(
                    image,
                    egui::CornerRadius { nw: SLIDE_ROUND, ne: SLIDE_ROUND, sw: 0, se: 0 },
                    Color32::BLACK,
                );
                let of = if from_bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::Slide(slide) };
                match thumb(of) {
                    Some((texture, overflow)) => {
                        egui::Image::from_texture((texture, image.size()))
                            .corner_radius(egui::CornerRadius { nw: SLIDE_ROUND, ne: SLIDE_ROUND, sw: 0, se: 0 })
                            .paint_at(ui, image);
                        if overflow {
                            let mark = image.right_top() + vec2(-14.0, 14.0);
                            painter.circle_filled(mark, 9.0, Color32::from_rgb(240, 190, 40));
                            painter.text(mark, Align2::CENTER_CENTER, "!", FontId::proportional(14.0), Color32::BLACK);
                        }
                    }
                    None => ui.ctx().request_repaint(),
                }
            };
            match view {
                SlideView::Table => {
                    // A strip in the group's colour with the number, the thumbnail, the words.
                    let strip = Rect::from_min_size(rect.min, vec2(34.0, rect.height()));
                    painter.rect_filled(rect, 0.0, Color32::from_gray(38));
                    painter.rect_filled(strip, 0.0, color32(group.color));
                    painter.text(
                        strip.center(),
                        Align2::CENTER_CENTER,
                        format!("{}.", index + 1),
                        FontId::proportional(13.0),
                        ink,
                    );
                    let image = Rect::from_min_size(rect.min + vec2(34.0, 0.0), image_size);
                    picture(ui, image);
                    let text_at = pos2(image.max.x + 14.0, rect.min.y + 8.0);
                    let mut top = text_at.y;
                    if starts_group && !group.name.is_empty() {
                        painter.text(
                            text_at,
                            Align2::LEFT_TOP,
                            group.name.as_str(),
                            FontId::proportional(12.0),
                            Color32::from_gray(160),
                        );
                        top += 18.0;
                    }
                    let clip = Rect::from_min_max(pos2(text_at.x, rect.min.y), rect.max);
                    painter.with_clip_rect(clip).text(
                        pos2(text_at.x, top),
                        Align2::LEFT_TOP,
                        words(),
                        FontId::proportional(13.0),
                        Color32::from_gray(230),
                    );
                }
                SlideView::Grid | SlideView::Easy => {
                    let image = Rect::from_min_size(rect.min, image_size);
                    if view == SlideView::Easy {
                        // Only the words, plain, on black: quick to read at any size.
                        painter.rect_filled(image, 0.0, Color32::BLACK);
                        // The lines wrap at the tile's width, so every word shows.
                        let font = FontId::proportional((width / 15.0).clamp(10.0, 18.0));
                        let galley =
                            painter.layout(words(), font, Color32::from_gray(235), (image.width() - 16.0).max(20.0));
                        painter.with_clip_rect(image).galley(
                            image.min + vec2(8.0, 6.0),
                            galley,
                            Color32::from_gray(235),
                        );
                    } else {
                        picture(ui, image);
                    }
                    // Under the slide, a bar in its group's colour across the whole width:
                    // the slide's number, the group's name on the group's first slide, and
                    // the key that jumps to the group at the right.
                    let foot = Rect::from_min_max(pos2(rect.min.x, image.max.y), rect.max);
                    let corners = egui::CornerRadius { nw: 0, ne: 0, sw: SLIDE_ROUND, se: SLIDE_ROUND };
                    painter.rect_filled(foot, corners, color32(group.color));
                    let number = painter.text(
                        foot.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        (index + 1).to_string(),
                        crate::studio::semibold(12.0),
                        ink,
                    );
                    if starts_group && !group.name.is_empty() {
                        let key = if from_bible { None } else { sp_model::key_for_group(&group.name) };
                        let name_end = foot.max.x - if key.is_some() { 22.0 } else { 8.0 };
                        let clip = Rect::from_min_max(pos2(number.max.x + 6.0, foot.min.y), pos2(name_end, foot.max.y));
                        painter.with_clip_rect(clip).text(
                            pos2(number.max.x + 8.0, foot.center().y),
                            Align2::LEFT_CENTER,
                            group.name.as_str(),
                            FontId::proportional(12.0),
                            ink,
                        );
                        if let Some(key) = key {
                            painter.text(
                                foot.right_center() - vec2(8.0, 0.0),
                                Align2::RIGHT_CENTER,
                                key,
                                FontId::monospace(10.5),
                                ink.gamma_multiply(0.7),
                            );
                        }
                    }
                }
            }
            // A slide's hot key, in a small badge at its top right.
            if let Some(key) = slide.hot_key {
                let badge = Rect::from_center_size(pos2(rect.max.x - 13.0, rect.min.y + 12.0), vec2(18.0, 16.0));
                painter.rect_filled(badge, 4.0, Color32::from_black_alpha(200));
                painter.text(
                    badge.center(),
                    Align2::CENTER_CENTER,
                    key.to_string(),
                    FontId::monospace(11.0),
                    Color32::WHITE,
                );
            }
            // A slide with actions has a small mark at its top left.
            if !slide.actions.is_empty() {
                let mark = pos2(rect.min.x + 12.0, rect.min.y + 12.0);
                painter.circle_filled(mark, 8.0, Color32::from_black_alpha(190));
                painter.text(
                    mark,
                    Align2::CENTER_CENTER,
                    "⚡",
                    FontId::proportional(10.0),
                    Color32::from_rgb(250, 200, 60),
                );
            }
            // A slide that goes on by itself shows after how long, beside that.
            if let Some(seconds) = slide.advance {
                let left = rect.min.x + if slide.actions.is_empty() { 4.0 } else { 24.0 };
                let badge = Rect::from_min_size(pos2(left, rect.min.y + 4.0), vec2(34.0, 16.0));
                painter.rect_filled(badge, 4.0, Color32::from_black_alpha(200));
                painter.text(
                    badge.center(),
                    Align2::CENTER_CENTER,
                    format!("{seconds:.0}s ›"),
                    FontId::proportional(10.5),
                    Color32::WHITE,
                );
            }
            // A slide's label, in a small dark tab at the picture's bottom right.
            if let Some(label) = slide.label.as_deref().filter(|label| !label.is_empty()) {
                let ink = Color32::from_gray(225);
                let text = painter.layout_no_wrap(label.to_owned(), FontId::proportional(10.5), ink);
                let bottom = if view == SlideView::Table { rect.max.y - 6.0 } else { rect.max.y - bar_height - 6.0 };
                let tab = Rect::from_min_max(
                    pos2(rect.max.x - text.size().x - 18.0, bottom - text.size().y - 4.0),
                    pos2(rect.max.x - 6.0, bottom),
                );
                painter.rect_filled(tab, 4.0, Color32::from_black_alpha(190));
                painter.galley(pos2(tab.min.x + 6.0, tab.min.y + 2.0), text, ink);
            }
            // A disabled slide is dimmed and says so: it is never shown.
            if !slide.enabled {
                painter.rect_filled(rect, f32::from(SLIDE_ROUND), Color32::from_black_alpha(170));
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Disabled",
                    crate::studio::semibold(13.0),
                    crate::studio::TEXT_DIM,
                );
            }
            // A hairline round the slide and its bar, so black slides do not melt into the background.
            if view != SlideView::Table {
                painter.rect_stroke(
                    rect,
                    f32::from(SLIDE_ROUND),
                    Stroke::new(1.0, crate::studio::LINE),
                    StrokeKind::Outside,
                );
            }
            // Tally red for the slide on screen; the accent for one picked
            // out; a dim red for the slide that was on screen before it was cleared.
            let framed = rect;
            let is_live = show.live() == Some(here);
            let outline = if is_live {
                Some(Stroke::new(2.5, LIVE_OUTLINE))
            } else if state.selected_slide == Some(here) || state.picked_slides.contains(&here) {
                Some(Stroke::new(2.0, SELECTED_OUTLINE))
            } else if show.live().is_none() && show.cursor() == Some(here) {
                Some(Stroke::new(1.5, WAS_LIVE_OUTLINE))
            } else if response.hovered() {
                Some(Stroke::new(1.0, crate::studio::TEXT_FAINT))
            } else {
                None
            };
            if let Some(stroke) = outline {
                painter.rect_stroke(framed, f32::from(SLIDE_ROUND) + 1.0, stroke, StrokeKind::Outside);
            }
        }
    });
    // The slides being moved are dimmed where they are, and one of them follows the pointer,
    // with how many there are.
    if let Some(drag) = moving {
        for rect in &moving_rects {
            ui.painter().rect_filled(*rect, f32::from(SLIDE_ROUND), Color32::from_black_alpha(150));
        }
        let first = drag.slides.first().and_then(|at| presentation.slide(*at)).map(|(_, slide)| slide);
        let texture =
            first.and_then(|slide| thumb(if from_bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::Slide(slide) }));
        paint_drag_ghost(ui, texture.map(|(texture, _)| texture), drag.slides.len());
    }
}

/// Puts slides of a song (by their places) where Paste finds them, and their words on the
/// system clipboard, so a paste can tell they are still what was copied last.
pub fn copy_slides(ctx: &egui::Context, state: &mut HomeState, song: &sp_model::Presentation, places: &[usize]) {
    let slides: Vec<Slide> = places.iter().filter_map(|at| song.slide(*at)).map(|(_, slide)| slide.clone()).collect();
    copy_these_slides(ctx, state, slides);
}

/// Puts these slides where Paste finds them, and their words on the system clipboard.
fn copy_these_slides(ctx: &egui::Context, state: &mut HomeState, slides: Vec<Slide>) {
    if slides.is_empty() {
        return;
    }
    let words: Vec<String> = slides
        .iter()
        .map(|slide| slide.texts().into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n"))
        .collect();
    let mut text = words.join("\n\n");
    // Something for the clipboard even when the slides have no words, so Paste comes back.
    if text.trim().is_empty() {
        text = if slides.len() == 1 { "Slide".to_owned() } else { format!("{} slides", slides.len()) };
    }
    state.slide_clip = slides;
    state.slide_clip_text = text.clone();
    ctx.copy_text(text);
}

/// In the Show view, with nothing being typed: Copy (and Cut) take the slides picked out in the
/// song shown, or the one selected; Paste puts slides copied last just after the last slide
/// picked out (or the one selected, or the one on screen, or at the end).
fn slide_clipboard_keys(ui: &egui::Ui, show: &Show, which: usize, state: &mut HomeState) {
    if ui.ctx().text_edit_focused() || state.editing.is_some() {
        return;
    }
    let Some(song) = show.presentations().get(which) else { return };
    let (copy, cut, pasted) = ui.input(|input| {
        let has = |wanted: fn(&egui::Event) -> bool| input.events.iter().any(wanted);
        let pasted = input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        });
        (has(|e| matches!(e, egui::Event::Copy)), has(|e| matches!(e, egui::Event::Cut)), pasted)
    });
    let mut picked: Vec<usize> =
        state.picked_slides.iter().filter(|p| p.presentation == which).map(|p| p.slide).collect();
    picked.sort_unstable();
    if picked.is_empty()
        && let Some(selected) = state.selected_slide.filter(|s| s.presentation == which)
    {
        picked.push(selected.slide);
    }
    if (copy || cut) && !picked.is_empty() {
        copy_slides(ui.ctx(), state, song, &picked);
        if cut {
            state.slide_request =
                Some((SlideRef { presentation: which, slide: picked[0] }, SlideRequest::DeleteMany(picked.clone())));
        }
    }
    if let Some(text) = pasted
        && !state.slide_clip.is_empty()
        && text == state.slide_clip_text
        && song.slide_count() > 0
    {
        let on_screen = show.live().filter(|at| at.presentation == which).map(|at| at.slide);
        let after = picked.last().copied().or(on_screen).unwrap_or(song.slide_count() - 1);
        state.slide_request =
            Some((SlideRef { presentation: which, slide: after }, SlideRequest::PasteAfter(state.slide_clip.clone())));
    }
}

/// The Edit view's slide cards: the bar under each picture, and the room between cards.
const EDIT_CARD_BAR: f32 = 22.0;
const EDIT_CARD_GAP: f32 = 10.0;

/// While slides are dragged near the top or bottom of a scrolling list of them, the list
/// scrolls that way, faster nearer the edge.
fn scroll_while_dragging(ui: &mut egui::Ui) {
    if !egui::DragAndDrop::has_payload_of_type::<SlideDrag>(ui.ctx()) {
        return;
    }
    let Some(pointer) = ui.ctx().pointer_hover_pos() else { return };
    let area = ui.clip_rect();
    if !area.x_range().contains(pointer.x) {
        return;
    }
    let edge = 48.0;
    let by = if pointer.y < area.min.y + edge {
        (area.min.y + edge - pointer.y).min(edge) * 0.35
    } else if pointer.y > area.max.y - edge {
        -(pointer.y - (area.max.y - edge)).min(edge) * 0.35
    } else {
        0.0
    };
    if by != 0.0 {
        ui.scroll_with_delta(vec2(0.0, by));
        ui.ctx().request_repaint();
    }
}

/// The colour of the line that shows where dragged slides will go.
const DROP_LINE: Color32 = Color32::from_rgb(80, 150, 255);

/// Whether slides dropped on a slide go after it: on its right half in a grid, its lower half
/// in a list.
fn drop_after(ui: &egui::Ui, rect: Rect, list: bool) -> bool {
    let pointer = ui.input(|input| input.pointer.interact_pos().or(input.pointer.hover_pos())).unwrap_or(rect.center());
    if list { pointer.y > rect.center().y } else { pointer.x > rect.center().x }
}

/// The line between two slides where dragged ones will go: beside a slide in a grid, above or
/// below it in a list.
fn paint_drop_line(ui: &egui::Ui, rect: Rect, after: bool, list: bool, gap: f32) {
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("slide drop line")))
        .with_clip_rect(ui.clip_rect());
    let stroke = Stroke::new(3.0, DROP_LINE);
    if list {
        let y = if after { rect.max.y + gap / 2.0 } else { rect.min.y - gap / 2.0 };
        painter.hline(rect.x_range(), y, stroke);
    } else {
        let x = if after { rect.max.x + gap / 2.0 } else { rect.min.x - gap / 2.0 };
        painter.vline(x, rect.y_range(), stroke);
    }
}

/// What follows the pointer while slides are dragged: the first of them, small, and how many
/// there are when there are several.
fn paint_drag_ghost(ui: &egui::Ui, texture: Option<TextureId>, count: usize) {
    let Some(pointer) = ui.ctx().pointer_hover_pos() else { return };
    let painter =
        ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("slide drag ghost")));
    let card = Rect::from_min_size(pointer + vec2(14.0, 10.0), vec2(144.0, 81.0));
    painter.rect_filled(card, f32::from(SLIDE_ROUND), Color32::BLACK);
    if let Some(texture) = texture {
        painter.image(
            texture,
            card,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::from_white_alpha(220),
        );
    }
    painter.rect_stroke(card, f32::from(SLIDE_ROUND), Stroke::new(2.0, DROP_LINE), StrokeKind::Outside);
    if count > 1 {
        let badge = egui::Rect::from_center_size(card.right_top() + vec2(-2.0, 2.0), vec2(24.0, 20.0));
        painter.rect_filled(badge, 10.0, DROP_LINE);
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            count.to_string(),
            crate::studio::semibold(12.0),
            Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Event, Key, Modifiers, RawInput};
    use sp_model::parse_song_text;

    use super::*;

    #[test]
    fn the_bible_pages_set_up_survives_being_saved_and_read_back() {
        let role = |name: &str| Some(sp_model::Role(name.to_owned()));
        let mut page = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        let (kjv, syn) = (page.shelf.position("kjv").unwrap(), page.shelf.position("syn").unwrap());
        // Not the usual set-up: Synodal first, in the translation box; KJV in the main box
        // with no reference; and the options changed.
        page.rows = vec![
            BibleRow { bible: Some(syn), verse: role("translation"), reference: role("reference") },
            BibleRow { bible: Some(kjv), verse: role("primary"), reference: None },
        ];
        page.options.show_verse_numbers = !page.options.show_verse_numbers;
        page.options.reference = crate::bible::ReferenceStyle::PassageLast;
        let text = serde_json::to_string(&page.settings()).unwrap();

        let mut fresh = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        assert_ne!(fresh.choices(), page.choices());
        let missing = fresh.apply_settings(&serde_json::from_str(&text).unwrap());
        assert!(missing.is_empty());
        assert_eq!(fresh.rows.len(), 2);
        for (got, want) in fresh.rows.iter().zip(&page.rows) {
            assert_eq!((got.bible, &got.verse, &got.reference), (want.bible, &want.verse, &want.reference));
        }
        assert_eq!(fresh.options.show_verse_numbers, page.options.show_verse_numbers);
        assert_eq!(fresh.options.reference, crate::bible::ReferenceStyle::PassageLast);
        assert_eq!(fresh.choices(), page.choices());

        // A box the Bible template does not have is sent nowhere, not drawn over the main text.
        let mut narrow = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        narrow.boxes.retain(|(have, _)| *have != sp_model::Role::translation());
        narrow.apply_settings(&serde_json::from_str(&text).unwrap());
        assert_eq!((&narrow.rows[0].verse, &narrow.rows[0].reference), (&None, &role("reference")));
    }

    #[test]
    fn a_saved_passage_is_made_without_changing_the_page() {
        let role = |name: &str| Some(sp_model::Role(name.to_owned()));
        let mut page = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        page.slots = Some([0, 1]);
        page.lookup = "Psalm 23".to_owned();
        page.look_up(None);
        page.pending = None;
        let before = (page.choices(), page.lookup.clone(), page.shown.clone(), page.showing);
        // Kept with the King James Version alone, one slide for the whole passage, the
        // reference on the last.
        let saved = crate::service::SavedPassage {
            lookup: "John 3:16-18".to_owned(),
            rows: vec![crate::service::SavedRow {
                bible: "kjv".to_owned(),
                verse: role("primary").map(|r| r.0),
                reference: role("reference").map(|r| r.0),
            }],
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: "passage_last".to_owned(),
        };
        let passage = page.passage_of(&saved).expect("made");
        assert_eq!(passage.title, "John 3:16-18");
        assert_eq!(passage.slide_count(), 3, "one slide a verse");
        let texts = |at: usize| passage.slide(at).map(|(_, slide)| slide.texts().len()).unwrap_or(0);
        assert_eq!((texts(0), texts(2)), (1, 2), "only the last slide has the reference");
        assert_eq!((page.choices(), page.lookup.clone(), page.shown.clone(), page.showing), before);
        assert!(page.pending.is_none(), "nothing goes into the page's slots");

        // A translation that isn't installed: the page's own rows make it, as opening it would.
        let gone = crate::service::SavedPassage {
            rows: vec![crate::service::SavedRow { bible: "nosuch".to_owned(), verse: None, reference: None }],
            ..saved
        };
        assert_eq!(page.passage_of(&gone).expect("made").title, "От Иоанна 3:16-18");
    }

    fn input(key: Option<Key>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            events: key
                .map(|key| Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .into_iter()
                .collect(),
            ..RawInput::default()
        }
    }

    /// Runs one frame and returns the commands and how many thumbnails were asked for.
    fn frame(ctx: &egui::Context, show: &Show, key: Option<Key>) -> (Vec<Command>, usize) {
        let mut commands = Vec::new();
        let mut asked = 0;
        let mut state = HomeState::default();
        let output = ctx.run_ui(input(key), |ui| {
            asked = 0;
            // As in the app: a frame may run twice and only the first run sees the input.
            commands.extend(home_ui(
                ui,
                show,
                &mut state,
                &Theme::builtin(),
                &mut |_| {
                    asked += 1;
                    None
                },
                Preview::default(),
                None,
            ));
        });
        output.drop_without_applying_deltas();
        (commands, asked)
    }

    fn show(slides: usize) -> Show {
        let text = (0..slides).map(|i| format!("Slide {i}\n")).collect::<Vec<_>>().join("\n");
        Show::new(vec![Arc::new(parse_song_text("Song", &text))])
    }

    /// Runs frames of the home screen on the Bible page with the window active, as typing in
    /// it would be, and returns the words drawn in the last one.
    fn bible_frames(state: &mut HomeState, frames: usize) -> Vec<String> {
        fn words(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| words(shape, into)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let show = show(1);
        let mut drawn = Vec::new();
        for at in 0..frames {
            let raw = RawInput { focused: true, time: Some(at as f64), ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            drawn.clear();
            output.shapes.iter().for_each(|clipped| words(&clipped.shape, &mut drawn));
            output.drop_without_applying_deltas();
        }
        drawn
    }

    #[test]
    fn typing_a_book_offers_its_names_and_typing_a_reference_fills_the_slides_without_return() {
        // A book's name half typed: the books it could be are offered under the field.
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.lookup = "jo".to_owned();
        state.bible.focus_lookup = true;
        let drawn = bible_frames(&mut state, 4);
        for book in ["Joshua", "Job", "John"] {
            assert!(drawn.iter().any(|words| words.trim() == book), "{book} is offered");
        }
        assert!(state.bible.pending.is_none(), "a book's name alone makes no slides");

        // A reference typed, no Return: a moment later its slides are made, and the field is
        // left as typed.
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        state.bible.lookup = "jn 3:16".to_owned();
        state.bible.typed_at = Some(0.0);
        bible_frames(&mut state, 3);
        let (_, passage) = state.bible.pending.as_ref().expect("the passage is made");
        assert_eq!(passage.title, "От Иоанна 3:16", "made in the first translation (Russian Synodal)");
        assert_eq!(state.bible.lookup, "jn 3:16");
        assert_eq!(state.bible.message, None);
    }

    #[test]
    fn the_history_keeps_each_passage_once_newest_first() {
        let at = |h: u32| chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap().and_hms_opt(h, 0, 0).unwrap();
        let mut page = BiblePage::default();
        page.note_shown("John 3:16", at(9));
        page.note_shown("John 3:16", at(10));
        page.note_shown("Psalm 23", at(11));
        page.note_shown("John 3:16", at(12));
        let passages: Vec<&str> = page.history.iter().map(|e| e.passage.as_str()).collect();
        assert_eq!(passages, ["John 3:16", "Psalm 23"]);
        assert_eq!(page.history[0].at, "2026-11-19T12:00:00", "moved up, with its new time");
        assert!(page.history_changed);
        for n in 0..150 {
            page.note_shown(&format!("Psalm {n}"), at(13));
        }
        assert_eq!(page.history.len(), HISTORY_LENGTH);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        assert_eq!(history_when("2026-11-19T10:42:05", today), "10:42 AM");
        assert_eq!(history_when("2026-11-17T09:30:00", today), "Tue 17 Nov");
    }

    #[test]
    fn the_lists_open_on_the_passage_looked_up_however_it_was_typed() {
        let mut page = BiblePage { slots: Some([0, 1]), ..BiblePage::default() };
        page.lookup = "jn 3:16".to_owned();
        page.look_up(None);
        let john = sp_bible::BOOKS.iter().position(|book| book.code == "JOH").unwrap();
        assert_eq!((page.list_book, page.list_chapter), (john, 3));
        assert!(page.list_scroll, "scrolled to it");
        // Looking up another verse of the same chapter leaves the lists where they are.
        page.list_scroll = false;
        page.lookup = "jn 3:17".to_owned();
        page.look_up(None);
        assert!(!page.list_scroll);
    }

    #[test]
    fn the_options_sample_is_made_again_only_when_a_choice_changes() {
        let mut page = BiblePage::default();
        let first = |page: &mut BiblePage| page.sample().unwrap().slides().next().unwrap().1.id;
        let id = first(&mut page);
        assert_eq!(first(&mut page), id, "the same choices keep the same slides, and their pictures");
        assert_eq!(page.sample().unwrap().slide_count(), 2, "one verse a slide");
        page.options.break_on_new_verse = false;
        assert_eq!(page.sample().unwrap().slide_count(), 1, "as many verses as fit");
        assert_ne!(first(&mut page), id);
    }

    #[test]
    fn a_verse_in_the_lists_can_be_clicked_while_slides_are_showing() {
        // Slides showing beside the lists (the song's, standing in for a passage).
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        state.bible.list_book = sp_bible::BOOKS.iter().position(|book| book.code == "JOH").unwrap();
        state.bible.list_chapter = 3;
        let ctx = egui::Context::default();
        let show = show(3);
        let run = |state: &mut HomeState, events: Vec<Event>, at: f64| {
            let raw = RawInput { focused: true, time: Some(at), events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        run(&mut state, Vec::new(), 0.0);
        run(&mut state, Vec::new(), 0.1);
        let row = ctx.read_response(crate::bible::list_row_id(("verse", 5u16))).expect("verse 5 is drawn").rect;
        let press = |pressed| Event::PointerButton {
            pos: row.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(&mut state, vec![Event::PointerMoved(row.center())], 0.2);
        run(&mut state, vec![press(true)], 0.3);
        run(&mut state, vec![press(false)], 0.35);
        assert_eq!(state.bible.lookup, "От Иоанна 3:5", "the click looked the verse up");

        // Hide, at the top of the lists, puts them away.
        let hide = ctx.read_response(crate::bible::list_row_id("hide")).expect("Hide is drawn").rect;
        let press = |pressed| Event::PointerButton {
            pos: hide.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(&mut state, vec![Event::PointerMoved(hide.center())], 0.5);
        run(&mut state, vec![press(true)], 0.6);
        run(&mut state, vec![press(false)], 0.65);
        assert!(!state.bible.lists_open);
        assert!(state.prefs().bible_lists_hidden, "and stay hidden at the next start");
    }

    #[test]
    fn a_double_click_in_the_lists_loads_the_chapter_and_puts_a_verse_on_screen() {
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        state.bible.list_book = sp_bible::BOOKS.iter().position(|book| book.code == "JOH").unwrap();
        state.bible.list_chapter = 3;
        let ctx = egui::Context::default();
        let show = show(3);
        let mut clock = 0.0;
        let mut run = |state: &mut HomeState, events: Vec<Event>| {
            clock += 0.05;
            let raw = RawInput { focused: true, time: Some(clock), events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        run(&mut state, Vec::new());
        run(&mut state, Vec::new());
        let double_click = |run: &mut dyn FnMut(&mut HomeState, Vec<Event>), state: &mut HomeState, at: egui::Pos2| {
            let press = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            run(state, vec![Event::PointerMoved(at)]);
            for _ in 0..2 {
                run(state, vec![press(true)]);
                run(state, vec![press(false)]);
            }
        };
        // The chapter: all of it.
        let chapter = ctx.read_response(crate::bible::list_row_id(("chapter", 3u16))).unwrap().rect.center();
        double_click(&mut run, &mut state, chapter);
        assert_eq!(state.bible.lookup, "От Иоанна 3:1-36");
        assert_eq!(state.bible.go_live, None, "nothing goes on screen");
        // A verse: its chapter, and its slide put on screen once the app has the chapter. (A
        // pause first, or the clicks would count on from the chapter's as a triple click.)
        state.bible.pending = None;
        for _ in 0..10 {
            run(&mut state, Vec::new());
        }
        let verse = ctx.read_response(crate::bible::list_row_id(("verse", 5u16))).unwrap().rect.center();
        double_click(&mut run, &mut state, verse);
        assert_eq!(state.bible.lookup, "От Иоанна 3:1-36");
        assert!(state.bible.pending.is_some());
        assert_eq!(state.bible.go_live, Some(4), "the fifth slide, one verse a slide");
    }

    /// Drags in the Show view's grid from one point to another, over a few frames.
    fn drag_in_grid(state: &mut HomeState, show: &Show, from: egui::Pos2, to: egui::Pos2) {
        let ctx = egui::Context::default();
        let mut clock = 0.0;
        let mut run = |state: &mut HomeState, events: Vec<Event>| {
            clock += 0.05;
            let raw = RawInput { focused: true, time: Some(clock), events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(state, Vec::new());
        run(state, vec![Event::PointerMoved(from)]);
        run(state, vec![button(from, true)]);
        for step in 1..=6 {
            run(state, vec![Event::PointerMoved(from + (to - from) * (step as f32 / 6.0))]);
        }
        run(state, vec![button(to, false)]);
        run(state, Vec::new());
    }

    #[test]
    fn slides_dragged_in_the_grid_are_asked_to_move_where_they_are_dropped() {
        let show = show(4);
        // Lay the grid out once, to know where the slides are.
        let mut state = HomeState::default();
        let ctx = egui::Context::default();
        let output = ctx.run_ui(input(None), |ui| {
            home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
        let rect = |at: usize| state.slide_rects.iter().find(|(here, _)| here.slide == at).map(|(_, r)| *r).unwrap();
        let (first, third) = (rect(0), rect(2));
        // One slide, onto the right half of the third: after it.
        let mut moving = HomeState::default();
        drag_in_grid(&mut moving, &show, first.center(), third.center() + vec2(third.width() / 4.0, 0.0));
        let asked = moving.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![0], to: 2, after: true }));
        // Two picked out, one of them dragged: both go, before the fourth.
        let fourth = rect(3);
        let mut picked = HomeState {
            picked_slides: vec![SlideRef { presentation: 0, slide: 1 }, SlideRef { presentation: 0, slide: 0 }],
            ..HomeState::default()
        };
        drag_in_grid(&mut picked, &show, rect(1).center(), fourth.center() - vec2(fourth.width() / 4.0, 0.0));
        let asked = picked.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![0, 1], to: 3, after: false }));
    }

    #[test]
    fn leaving_a_slide_drops_the_songs_steps_only_when_the_slide_changed_after_them() {
        use EditStep::{Slide as S, Song as G};
        // Moved, then the slide edited: undoing the move would lose the edit, so all go.
        let mut steps = vec![S, G, S];
        assert!(drop_slide_steps(&mut steps));
        assert!(steps.is_empty());
        // Edited, then moved: the move's copy of the song has the edit; it stays.
        let mut steps = vec![S, G];
        assert!(!drop_slide_steps(&mut steps));
        assert_eq!(steps, [G]);
    }

    #[test]
    fn copied_slides_paste_after_the_slide_picked_out() {
        let show = show(4);
        let ctx = egui::Context::default();
        let mut state = HomeState {
            picked_slides: vec![SlideRef { presentation: 0, slide: 1 }, SlideRef { presentation: 0, slide: 0 }],
            ..HomeState::default()
        };
        let run = |state: &mut HomeState, events: Vec<Event>| {
            let raw = RawInput { focused: true, events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        run(&mut state, vec![Event::Copy]);
        assert_eq!(state.slide_clip.len(), 2, "the two picked out, in order");
        assert_eq!(state.slide_clip_text, "Slide 0\n\nSlide 1");
        state.picked_slides = vec![SlideRef { presentation: 0, slide: 3 }];
        let text = state.slide_clip_text.clone();
        run(&mut state, vec![Event::Paste(text)]);
        let (at, request) = state.slide_request.take().expect("a paste asked for");
        assert_eq!(at.slide, 3);
        assert!(matches!(request, SlideRequest::PasteAfter(slides) if slides.len() == 2));
        // Something else copied since: Paste is not of the slides.
        run(&mut state, vec![Event::Paste("other words".to_owned())]);
        assert!(state.slide_request.is_none());
    }

    #[test]
    fn undo_in_the_editor_takes_back_a_move_when_it_was_the_last_change() {
        let show = show(3);
        let ctx = egui::Context::default();
        let mut state =
            HomeState { editing: EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }), ..HomeState::default() };
        let run = |state: &mut HomeState, events: Vec<Event>| {
            let raw = RawInput { focused: true, events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        run(&mut state, Vec::new());
        // A move, as the app notes it.
        state.edit_steps.push(EditStep::Song);
        let z = |modifiers| Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers };
        run(&mut state, vec![z(Modifiers::COMMAND)]);
        assert_eq!(state.song_undo, Some(false), "the app is asked to undo the move");
        assert_eq!(state.edit_redo, [EditStep::Song]);
        state.song_undo = None;
        run(&mut state, vec![z(Modifiers::COMMAND | Modifiers::SHIFT)]);
        assert_eq!(state.song_undo, Some(true), "and to redo it");
    }

    #[test]
    fn a_number_starting_a_books_name_is_not_its_chapter() {
        assert!(!has_chapter("1 ко"));
        assert!(!has_chapter("1 John"));
        assert!(has_chapter("1 John 3"));
        assert!(has_chapter("jn 3:16"));
        assert!(book_suggestions("1 ко", "uk").iter().any(|s| s.insert == "1 до коринтян "));
        assert!(book_suggestions("John 3", "en").is_empty());
        assert!(book_suggestions("Job", "en").is_empty(), "nothing offered for a name already typed in full");
    }

    #[test]
    fn a_bible_row_set_to_none_frees_its_bible_and_is_left_out() {
        let mut page = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        let first = page.rows[0].bible;
        page.rows[0].bible = None;
        assert_eq!(page.lead(), page.rows[1].bible, "the lookup uses the next row with a Bible");
        // The first row's Bible can now go to the second row: the two have swapped.
        page.rows[1].bible = first;
        assert_eq!(page.lead(), first);
        assert_eq!(page.settings().rows.len(), 1, "a row with no Bible isn't kept");
    }

    #[test]
    fn filter_matches_any_part_ignoring_case() {
        assert!(matches_filter("Morning Song", ""));
        assert!(matches_filter("Morning Song", "  "));
        assert!(matches_filter("Morning Song", "song"));
        assert!(matches_filter("Песня Утра", "утра"));
        assert!(!matches_filter("Morning Song", "evening"));
    }

    #[test]
    fn group_bar_text_contrasts_with_its_colour() {
        assert_eq!(text_on(Rgba::rgb(250, 240, 200)), Color32::BLACK);
        assert_eq!(text_on(Rgba::rgb(20, 40, 120)), Color32::WHITE);
    }

    /// Runs two frames and returns how many slide and media thumbnails the last one asked for.
    fn count_thumbs(ctx: &egui::Context, show: &Show, state: &mut HomeState) -> (usize, usize) {
        let (mut slides, mut media) = (0, 0);
        for _ in 0..2 {
            let output = ctx.run_ui(input(None), |ui| {
                (slides, media) = (0, 0);
                home_ui(
                    ui,
                    show,
                    state,
                    &Theme::builtin(),
                    &mut |which| {
                        match which {
                            ThumbOf::Slide(_) | ThumbOf::BibleSlide(_) | ThumbOf::Layout(..) => slides += 1,
                            ThumbOf::Media(_) => media += 1,
                        }
                        None
                    },
                    Preview::default(),
                    None,
                );
            });
            output.drop_without_applying_deltas();
        }
        (slides, media)
    }

    fn show_with_media() -> Show {
        let mut show = show(3);
        for name in ["a.png", "b.png"] {
            show.add_media(sp_model::MediaItem::new(name.into(), name.into()));
        }
        show.apply(Command::ShowMedia(1));
        show
    }

    #[test]
    fn the_show_view_draws_the_media_bin_with_a_notice_and_a_drag() {
        let show = show_with_media();
        let mut state = HomeState {
            drop_hover: true,
            notice: Some("x.mp4: videos are not supported yet, only pictures.".into()),
            ..HomeState::default()
        };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(media, 2, "both media thumbnails should be asked for");
        assert_eq!(slides, 3);
    }

    #[test]
    fn the_edit_view_replaces_the_show_view_and_changes_nothing_by_itself() {
        let show = show_with_media();
        let editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 1 });
        let mut state = HomeState { editing, ..HomeState::default() };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(media, 0, "the media bin is not part of the Edit view");
        assert_eq!(slides, 3, "the slide list down the left shows the song's slides");
        assert_eq!(state.editing.as_ref().map(|d| d.at.slide), Some(1), "drawing must not leave the Edit view");
        assert!(state.pending_edit.is_none(), "nothing was typed, so nothing changed");
    }

    /// Runs one frame of the home view with `state`, pressing `key` if given.
    fn frame_with(ctx: &egui::Context, show: &Show, state: &mut HomeState, key: Option<Key>) {
        let output = ctx.run_ui(input(key), |ui| {
            home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
    }

    fn three_songs() -> Show {
        Show::new(["One", "Two", "Three"].iter().map(|title| Arc::new(parse_song_text(title, "Words\n"))).collect())
    }

    #[test]
    fn delete_with_a_box_selected_in_the_theme_editor_deletes_the_box_not_the_template() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let theme = Theme::builtin();
        let slide = theme.templates[0].as_slide();
        let boxes = slide.objects.len();
        let mut draft = EditDraft::of_slide(slide, 0);
        draft.select_only(draft.slide.objects.first().map(|o| o.id));
        let mut state =
            HomeState { theme_edit: Some(ThemeEdit::of(0, theme)), editing: Some(draft), ..HomeState::default() };
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_template, None, "the template isn't asked about");
        assert_eq!(state.editing.as_ref().map(|d| d.slide.objects.len()), Some(boxes - 1), "the box is gone");
        // With nothing selected, Delete is for the template.
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_template, Some(0));
    }

    #[test]
    fn delete_after_picking_a_theme_asks_about_that_theme_but_never_a_built_in_one() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let mut theme = Theme::builtin();
        theme.id = sp_model::Uuid::new_v4();
        let slide = theme.templates[0].as_slide();
        let boxes = slide.objects.len();
        let mut state = HomeState {
            theme_edit: Some(ThemeEdit::of(2, theme.clone())),
            editing: Some(EditDraft::of_slide(slide.clone(), 0)),
            picking: Picking::Themes,
            themes_kept: vec![0, 1],
            ..HomeState::default()
        };
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_theme, Some(2));
        assert_eq!(state.confirm_delete_template, None);
        assert_eq!(state.editing.as_ref().map(|d| d.slide.objects.len()), Some(boxes));
        // The built-in Bible theme, picked, is kept.
        let mut kept = HomeState {
            theme_edit: Some(ThemeEdit::of(1, theme)),
            editing: Some(EditDraft::of_slide(slide, 0)),
            picking: Picking::Themes,
            themes_kept: vec![0, 1],
            ..HomeState::default()
        };
        frame_with(&ctx, &show, &mut kept, Some(Key::Delete));
        assert_eq!((kept.confirm_delete_theme, kept.confirm_delete_template), (None, None));
    }

    #[test]
    fn delete_asks_about_every_song_picked_out_and_return_deletes_them() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let mut state = HomeState { picked_songs: vec![0, 2], picking: Picking::Songs, ..HomeState::default() };
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_songs, vec![0, 2], "Delete asks first");
        assert!(state.songs_to_delete.is_empty(), "nothing is deleted before the answer");
        assert!(state.alert_open(), "the show's keys wait while it asks");
        frame_with(&ctx, &show, &mut state, Some(Key::Enter));
        assert_eq!(state.songs_to_delete, vec![0, 2], "Return deletes both");
        assert!(state.confirm_delete_songs.is_empty() && state.picked_songs.is_empty());
    }

    #[test]
    fn escape_keeps_the_media_picked_out() {
        let (ctx, show) = (egui::Context::default(), show_with_media());
        let mut state = HomeState { picked_media: vec![0, 1], picking: Picking::Media, ..HomeState::default() };
        frame_with(&ctx, &show, &mut state, Some(Key::Backspace));
        assert_eq!(state.confirm_delete_media, vec![0, 1]);
        frame_with(&ctx, &show, &mut state, Some(Key::Escape));
        assert!(state.confirm_delete_media.is_empty());
        assert!(state.media_request.is_none(), "Escape deletes nothing");
        assert_eq!(state.picked_media, vec![0, 1]);
    }

    #[test]
    fn delete_after_a_slide_was_clicked_deletes_nothing() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let mut state = HomeState { picking: Picking::Nothing, ..HomeState::default() };
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert!(!state.alert_open());
    }

    /// Runs one frame of the home view with these pointer events.
    fn frame_events(ctx: &egui::Context, show: &Show, state: &mut HomeState, events: Vec<Event>, time: f64) {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            time: Some(time),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn a_layer_key_and_clear_all_clear_in_the_show_view_and_in_the_edit_view() {
        use sp_engine::Layer;
        let show = three_songs();
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // In a window of 1280 by 800 the right column's keys start 390 down and 920 across:
        // Slide is the first of them, Props the third, and Clear All lies under the two rows.
        let click = |state: &mut HomeState, at: egui::Pos2| {
            let ctx = egui::Context::default();
            let mut asked = Vec::new();
            let frames = [vec![Event::PointerMoved(at)], vec![press(at, true)], vec![press(at, false)], vec![]];
            for (n, events) in frames.into_iter().enumerate() {
                let raw = RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
                    time: Some(n as f64 * 0.1),
                    events,
                    ..RawInput::default()
                };
                let output = ctx.run_ui(raw, |ui| {
                    asked.extend(home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
                });
                output.drop_without_applying_deltas();
            }
            asked
        };
        for editing in [false, true] {
            let mut state = HomeState::default();
            if editing {
                state.editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
                assert!(state.editing.is_some());
            }
            assert_eq!(
                click(&mut state, pos2(960.0, 412.0)),
                vec![Command::Clear(Layer::Slides)],
                "editing: {editing}"
            );
            assert_eq!(
                click(&mut state, pos2(1200.0, 412.0)),
                vec![Command::Clear(Layer::Props)],
                "editing: {editing}"
            );
            assert_eq!(click(&mut state, pos2(1090.0, 526.0)), vec![Command::ClearAll], "editing: {editing}");
            assert_eq!(state.editing.is_some(), editing, "clearing leaves the view as it was");
        }
    }

    #[test]
    fn the_edit_views_switch_goes_between_slides_themes_props_and_stage() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let mut state = HomeState::default();
        state.props.push(crate::props::Prop::new("Logo".to_owned()));
        state.props.push(crate::props::Prop::new("Name".to_owned()));
        // Asked for by name (the View menu, the command line), Edit is the slide editor.
        state.view_request = Some(1);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Slides));
        // Props opens the first prop; a click on another in the list opens that one.
        state.edit_kind_request = Some(EditKind::Props);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Props));
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(0));
        // Asked for again by name, Edit goes back to the slides.
        state.view_request = Some(1);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Slides));
        // Themes is opened by the app (it holds the themes): it is asked to.
        state.view_request = Some(3);
        frame_with(&ctx, &show, &mut state, None);
        assert!(matches!(state.theme_request, Some(ThemeRequest::Edit(_))));
        // From the Theme Editor, another kind waits for the app to save the theme and close it.
        let theme = Theme::builtin();
        state.editing = Some(EditDraft::of_slide(theme.templates[0].as_slide(), 0));
        state.theme_edit = Some(ThemeEdit::of(0, theme));
        assert_eq!(state.edit_kind(), Some(EditKind::Themes));
        state.edit_kind_request = Some(EditKind::Props);
        frame_with(&ctx, &show, &mut state, None);
        assert!(state.theme_edit.as_ref().is_some_and(|edit| edit.leave), "the app is asked to close the theme");
        assert_eq!(state.edit_kind(), Some(EditKind::Themes));
        (state.theme_edit, state.editing) = (None, None);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Props));
        // Stage has no slide of its own open; a slide opened after it takes its place.
        state.edit_kind_request = Some(EditKind::Stage);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Stage));
        assert!(state.editing.is_none() && !state.stage_layouts.is_empty());
        state.edit_kind_request = Some(EditKind::Slides);
        frame_with(&ctx, &show, &mut state, None);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!((state.edit_kind(), state.stage_editor_open), (Some(EditKind::Slides), false));
        state.edit_kind_request = Some(EditKind::Stage);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Stage));
        // Show leaves the Edit view whichever kind is open.
        state.view_request = Some(0);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), None);
        assert!(!state.stage_editor_open);
        // With no props there is nothing for Props to open: things stay as they are.
        state.props.clear();
        state.edit_kind_request = Some(EditKind::Props);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!((state.edit_kind(), state.edit_kind_request), (None, None));
    }

    #[test]
    fn a_click_on_edit_in_the_top_bar_while_a_prop_is_edited_changes_nothing() {
        let show = three_songs();
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // Click all along the switch between the views. Show and Bible leave the Edit view;
        // Edit, the view that is open, must not swap the prop for a song's slide.
        let mut stayed = 0;
        for x in (150..460).step_by(6) {
            let ctx = egui::Context::default();
            let mut state = HomeState::default();
            state.props.push(crate::props::Prop::new("Logo".to_owned()));
            state.edit_kind_request = Some(EditKind::Props);
            let at = pos2(x as f32, 26.0);
            frame_events(&ctx, &show, &mut state, vec![Event::PointerMoved(at)], 0.0);
            frame_events(&ctx, &show, &mut state, vec![press(at, true)], 0.1);
            frame_events(&ctx, &show, &mut state, vec![press(at, false)], 0.2);
            frame_events(&ctx, &show, &mut state, vec![], 0.3);
            assert_ne!(state.edit_kind(), Some(EditKind::Slides), "clicked at {x}");
            stayed += usize::from(state.edit_kind() == Some(EditKind::Props));
        }
        assert!(stayed > 8, "some of the clicks were on Edit, and elsewhere in the bar: {stayed}");
    }

    /// One frame of the window with these events, returning the commands it gave.
    fn frame_commands(
        ctx: &egui::Context,
        show: &Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
    ) -> Vec<Command> {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            time: Some(time),
            events,
            ..RawInput::default()
        };
        let mut commands = Vec::new();
        let output = ctx.run_ui(raw, |ui| {
            commands.extend(home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
        });
        output.drop_without_applying_deltas();
        commands
    }

    #[test]
    fn a_reference_in_the_find_box_opens_the_bible_view_on_its_passage() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        let mut commands = frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        // The passage is the first thing listed: Return opens it.
        commands.extend(frame_commands(&ctx, &show, &mut state, vec![key(Key::Enter)], 0.1));
        assert!(state.library_search.is_none(), "the search ends");
        let lead = state.bible.lead().and_then(|lead| state.bible.shelf.get(lead).ok()).expect("a Bible");
        let range = sp_bible::parse_reference_in(&state.bible.lookup, &lead.info.language).expect("a reference");
        assert_eq!((range.start.book, range.start.chapter, range.start.verse), (42, 3, 16), "{}", state.bible.lookup);
        assert_eq!(range.end.verse, 18);
        commands.extend(frame_commands(&ctx, &show, &mut state, vec![], 0.2));
        assert!(state.bible_open && !ctx.text_edit_focused(), "the Bible view opens, and the keys are free");
        assert!(commands.is_empty(), "nothing is put on the screens: {commands:?}");
    }

    #[test]
    fn words_in_the_find_box_find_songs_and_verses_and_a_verse_can_be_added_to_the_playlist() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![
            Arc::new(parse_song_text("Still waters", "The shepherd leads me on\n")),
            Arc::new(parse_song_text("Another", "Words\n")),
        ]);
        let mut state = HomeState { library_search: Some(("shepherd".to_owned(), 0)), ..HomeState::default() };
        state.song_keys = vec!["one".to_owned(), "two".to_owned()];
        frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        // The song is found by a line of its words, and the Bibles in use give verses.
        let songs = search_songs(&show, |_| true, "shepherd");
        assert_eq!(songs, vec![(0, Some("The shepherd leads me on".to_owned()))]);
        let in_use: Vec<String> =
            state.bible.rows.iter().filter_map(|row| Some(state.bible.shelf.bibles[row.bible?].id.clone())).collect();
        let verses = state.finder.rows("shepherd", &in_use, Vec::new, &ctx).expect("answered").to_vec();
        assert_eq!(verses.len(), crate::find::VERSES_LISTED);
        assert!(verses.iter().all(|verse| !verse.passage && verse.line.to_lowercase().contains("shepherd")));
        // The list is the song, then the verses. Down to the last verse; Command-Return adds it
        // to the playlist in front and nothing is put on the screens.
        let key = |key: Key, modifiers: Modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let mut commands = Vec::new();
        for step in 0..songs.len() + verses.len() - 1 {
            let down = vec![key(Key::ArrowDown, Modifiers::NONE)];
            commands.extend(frame_commands(&ctx, &show, &mut state, down, 0.1 + step as f64 * 0.1));
        }
        let before = state.services.front().items.len();
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            time: Some(1.0),
            events: vec![Event::ModifiersChanged(Modifiers::COMMAND), key(Key::Enter, Modifiers::COMMAND)],
            ..RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            commands.extend(home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
        });
        output.drop_without_applying_deltas();
        let items = &state.services.front().items;
        assert_eq!(items.len(), before + 1);
        let last = verses.last().expect("a verse");
        assert!(
            matches!(items.last(), Some(crate::service::Item::Passage(saved)) if saved.lookup == last.lookup),
            "{:?}",
            items.last()
        );
        assert!(state.library_search.is_none() && !state.bible_open && commands.is_empty());
    }

    #[test]
    fn a_search_that_ends_gives_the_keyboard_back_to_the_show() {
        let show = three_songs();
        let ctx = egui::Context::default();
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // Find (from the menu, or its shortcut) hands the find box the keyboard; Escape ends the
        // search and the keyboard is free again, so Space and Escape run the show.
        let mut state = HomeState { library_search: Some((String::new(), 0)), ..HomeState::default() };
        frame_events(&ctx, &show, &mut state, vec![], 0.0);
        frame_events(&ctx, &show, &mut state, vec![Event::Text("tw".to_owned())], 0.1);
        assert!(ctx.text_edit_focused(), "the find box has the keyboard");
        assert_eq!(state.library_search.as_ref().map(|(words, _)| words.as_str()), Some("tw"));
        frame_events(&ctx, &show, &mut state, vec![key(Key::Escape)], 0.2);
        frame_events(&ctx, &show, &mut state, vec![], 0.3);
        assert!(state.library_search.is_none() && !ctx.text_edit_focused(), "Escape ends the search");
        // A click in the box opens a search; a press anywhere else ends it.
        let find = state.find_rect.expect("a window this wide has the find box").center();
        frame_events(&ctx, &show, &mut state, vec![Event::PointerMoved(find)], 0.4);
        frame_events(&ctx, &show, &mut state, vec![press(find, true)], 0.5);
        frame_events(&ctx, &show, &mut state, vec![press(find, false)], 0.6);
        frame_events(&ctx, &show, &mut state, vec![], 0.7);
        assert!(state.library_search.is_some() && ctx.text_edit_focused(), "a click in the box opens a search");
        let elsewhere = pos2(640.0, 500.0);
        frame_events(&ctx, &show, &mut state, vec![Event::PointerMoved(elsewhere)], 0.8);
        frame_events(&ctx, &show, &mut state, vec![press(elsewhere, true)], 0.9);
        frame_events(&ctx, &show, &mut state, vec![press(elsewhere, false)], 1.0);
        frame_events(&ctx, &show, &mut state, vec![], 1.1);
        assert!(state.library_search.is_none() && !ctx.text_edit_focused(), "a press elsewhere ends it");
        // Return opens the song found, and that ends the search too.
        state.library_search = Some(("thr".to_owned(), 0));
        frame_events(&ctx, &show, &mut state, vec![], 1.2);
        frame_events(&ctx, &show, &mut state, vec![key(Key::Enter)], 1.3);
        frame_events(&ctx, &show, &mut state, vec![], 1.4);
        assert!(state.library_search.is_none() && !ctx.text_edit_focused(), "opening a song ends the search");
    }

    #[test]
    fn a_box_dragged_across_the_slides_picks_the_ones_it_touches_and_delete_asks_for_them() {
        let (ctx, show) = (egui::Context::default(), show(6));
        let mut state = HomeState::default();
        frame_events(&ctx, &show, &mut state, Vec::new(), 0.0);
        let rect = |n: usize| state.slide_rects.iter().find(|(at, _)| at.slide == n).map(|(_, r)| *r).unwrap();
        // From just above and left of the first slide (between slides) to the middle of the second.
        let (start, end) = (rect(0).min - vec2(6.0, 4.0), rect(1).center());
        let press = |at, pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let steps = [
            vec![Event::PointerMoved(start)],
            vec![press(start, true)],
            vec![Event::PointerMoved(start + vec2(20.0, 10.0))],
            vec![Event::PointerMoved(end)],
            vec![Event::PointerMoved(end)],
        ];
        for (n, events) in steps.into_iter().enumerate() {
            frame_events(&ctx, &show, &mut state, events, 0.1 * (n + 1) as f64);
        }
        let mut picked: Vec<usize> = state.picked_slides.iter().map(|at| at.slide).collect();
        picked.sort_unstable();
        assert_eq!(picked, vec![0, 1], "the box touches the first two slides");
        frame_events(&ctx, &show, &mut state, vec![press(end, false)], 0.7);
        assert!(state.sweep.is_none());
        assert!(show.live().is_none(), "picking puts nothing on screen");
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_slides.len(), 2, "Delete asks about both");
        frame_with(&ctx, &show, &mut state, Some(Key::Enter));
        assert_eq!(state.slides_to_delete.len(), 2);
    }

    #[test]
    fn up_and_down_in_the_show_view_go_through_the_song_list() {
        let songs = ["Alpha", "Beta", "Gamma"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let show = Show::new(songs.to_vec());
        let (commands, _) = frame(&egui::Context::default(), &show, Some(Key::ArrowDown));
        assert!(commands.contains(&Command::Select(1)), "{commands:?}");
        let (commands, _) = frame(&egui::Context::default(), &show, Some(Key::ArrowUp));
        assert!(!commands.iter().any(|c| matches!(c, Command::Select(_))), "already at the top: {commands:?}");
        // On the Bible page the list is left alone.
        let ctx = egui::Context::default();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        let mut asked = Vec::new();
        let output = ctx.run_ui(input(Some(Key::ArrowDown)), |ui| {
            asked = home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
        assert!(!asked.iter().any(|c| matches!(c, Command::Select(_))));
    }

    /// Clicks once at a place in the window: the pointer goes there, presses and lets go.
    fn click_at(ctx: &egui::Context, show: &Show, state: &mut HomeState, at: egui::Pos2, time: &mut f64) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)], Vec::new()] {
            *time += 0.1;
            frame_events(ctx, show, state, events, *time);
        }
    }

    #[test]
    fn the_name_over_the_list_opens_the_chooser_and_choosing_puts_the_list_back() {
        let songs = ["Alpha", "Beta"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let show = Show::new(songs.to_vec());
        let mut state = HomeState { playlist_open: true, filter: "al".to_owned(), ..HomeState::default() };
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        frame_events(&ctx, &show, &mut state, Vec::new(), time);
        // The name of what is listed is under the small label at the top of the left column.
        let top = crate::studio::TOP_BAR + 16.0 + 36.0;
        click_at(&ctx, &show, &mut state, pos2(110.0, top + 17.0), &mut time);
        assert!(state.chooser_open, "a click on the name opens the chooser");
        assert!(state.playlist_open, "and changes nothing else");
        // In the chooser: the Playlists heading, the one playlist, the Song libraries heading
        // (set apart by a line), then All Songs.
        let all_songs = top + 39.6 + 36.0 + 1.0 + 36.0 + 1.0 + 53.0 + 1.0 + 18.0;
        click_at(&ctx, &show, &mut state, pos2(110.0, all_songs), &mut time);
        assert!(!state.playlist_open, "All Songs is listed");
        assert!(state.library_shown.is_none());
        assert!(!state.chooser_open, "choosing puts the chooser away");
        assert!(state.filter.is_empty(), "and starts with nothing filtered out");
        assert!(show.live().is_none(), "nothing went on screen");
    }

    #[test]
    fn up_and_down_go_through_a_playlist_passing_over_headers() {
        use crate::service::Item;
        let songs = ["Alpha", "Beta"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let show = Show::new(songs.to_vec());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.add(Item::Song { key: "title:Alpha".to_owned(), title: "Alpha".to_owned() });
        state.services.add(Item::Header { name: "Sermon".to_owned() });
        state.services.add(Item::Song { key: "title:Beta".to_owned(), title: "Beta".to_owned() });
        state.services.chosen = Some(0);
        let ctx = egui::Context::default();
        let mut asked = Vec::new();
        let output = ctx.run_ui(input(Some(Key::ArrowDown)), |ui| {
            asked = home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        output.drop_without_applying_deltas();
        // The item after the header is opened (the app then shows its song, Beta).
        assert_eq!(state.playlist_scroll_to, Some(2), "the header is passed over");
        assert!(asked.contains(&Command::Select(1)), "{asked:?}");
    }

    #[test]
    fn a_video_or_a_placeholder_in_a_playlist_stays_chosen_and_opening_it_puts_nothing_on_screen() {
        use crate::service::Item;
        let songs = ["Alpha", "Beta"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let mut show = Show::new(songs.to_vec());
        show.add_media(sp_model::MediaItem::new("Walk-in.mp4".into(), "Walk-in.mp4".into()));
        let walk_in = show.media()[0].id;
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.add(Item::Song { key: "title:Alpha".to_owned(), title: "Alpha".to_owned() });
        state.services.add(Item::Media { id: walk_in, name: "Walk-in.mp4".to_owned() });
        state.services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        state.services.add(Item::Song { key: "title:Beta".to_owned(), title: "Beta".to_owned() });
        state.services.chosen = Some(0);
        let ctx = egui::Context::default();
        let mut all_asked = Vec::new();
        let mut step = |show: &mut Show, state: &mut HomeState, key: Option<Key>| {
            let mut asked = Vec::new();
            let output = ctx.run_ui(input(key), |ui| {
                asked = home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
            for command in asked {
                all_asked.push(command);
                show.apply(command);
            }
        };
        step(&mut show, &mut state, None);
        step(&mut show, &mut state, Some(Key::ArrowDown));
        step(&mut show, &mut state, None);
        step(&mut show, &mut state, None);
        assert_eq!(state.services.chosen, Some(1), "the video stays chosen; Alpha, still open, doesn't take it back");
        step(&mut show, &mut state, Some(Key::ArrowDown));
        step(&mut show, &mut state, None);
        assert_eq!(state.services.chosen, Some(2), "on to the placeholder");
        step(&mut show, &mut state, Some(Key::ArrowDown));
        step(&mut show, &mut state, None);
        assert_eq!((state.services.chosen, show.selected()), (Some(3), 1), "and past it to Beta");
        assert!(show.live_media().is_none(), "opening the video didn't put it on screen");
        assert!(!all_asked.iter().any(|c| matches!(c, Command::ShowMedia(_))), "{all_asked:?}");
    }

    /// Carries media bin item 0 (as a drag out of the bin does) to `at` in the window and lets go,
    /// with a playlist of Alpha, a placeholder and Beta open in the left column. Returns the
    /// playlist's items after.
    fn drop_media_on_playlist(at: egui::Pos2) -> Vec<crate::service::Item> {
        use crate::service::Item;
        let songs = ["Alpha", "Beta"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let mut show = Show::new(songs.to_vec());
        show.add_media(sp_model::MediaItem::new("Welcome.png".into(), "Welcome.png".into()));
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.add(Item::Song { key: "title:Alpha".to_owned(), title: "Alpha".to_owned() });
        state.services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        state.services.add(Item::Song { key: "title:Beta".to_owned(), title: "Beta".to_owned() });
        let ctx = egui::Context::default();
        let press = |pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let frames = [
            vec![Event::PointerMoved(at)],
            vec![press(true)],
            vec![Event::PointerMoved(at + vec2(0.0, 1.0)), Event::PointerMoved(at)],
            vec![press(false)],
            vec![],
        ];
        for (n, events) in frames.into_iter().enumerate() {
            let raw = RawInput { events, time: Some(n as f64 * 0.1), ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                if n >= 1 {
                    egui::DragAndDrop::set_payload(ui.ctx(), MediaDrag(0));
                }
                home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        }
        state.services.front().items.clone()
    }

    #[test]
    fn a_picture_dragged_from_the_bin_fills_a_placeholder_or_goes_between_two_items() {
        use crate::service::Item;
        let welcome = |items: &[Item]| {
            items.iter().position(|item| matches!(item, Item::Media { name, .. } if name == "Welcome.png"))
        };
        // Down the left column, where the playlist's rows are: the first place a drop changes
        // the playlist is over (or just above) its first row.
        let mut filled = None;
        let mut between = None;
        for y in (150..500).step_by(2) {
            let items = drop_media_on_playlist(pos2(60.0, y as f32));
            match (items.len(), welcome(&items)) {
                (3, Some(1)) if filled.is_none() => filled = Some(y),
                (4, Some(at)) if between.is_none() && at > 0 => between = Some((y, at)),
                _ => {}
            }
        }
        assert!(filled.is_some(), "a drop on the placeholder's row puts the picture in its place");
        let (_, at) = between.expect("a drop elsewhere on the list puts it between two items");
        assert!((1..=3).contains(&at));
    }

    #[test]
    fn up_and_down_go_down_a_playlist_step_by_step_as_the_app_runs_it() {
        use crate::service::Item;
        let songs = ["Alpha", "Beta", "Gamma"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let mut show = Show::new(songs.to_vec());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        for title in ["Alpha", "Beta", "Gamma"] {
            state.services.add(Item::Song { key: format!("title:{title}"), title: title.to_owned() });
        }
        state.services.chosen = Some(0);
        let ctx = egui::Context::default();
        // Each frame as the app runs it: the window, then what it asked for carried out.
        let step = |show: &mut Show, state: &mut HomeState, key: Option<Key>| {
            let mut asked = Vec::new();
            let output = ctx.run_ui(input(key), |ui| {
                asked = home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
            for command in asked {
                show.apply(command);
            }
        };
        step(&mut show, &mut state, None);
        step(&mut show, &mut state, Some(Key::ArrowDown));
        step(&mut show, &mut state, None);
        assert_eq!((state.services.chosen, show.selected()), (Some(1), 1), "Beta");
        step(&mut show, &mut state, Some(Key::ArrowDown));
        step(&mut show, &mut state, None);
        assert_eq!((state.services.chosen, show.selected()), (Some(2), 2), "Gamma");
        step(&mut show, &mut state, Some(Key::ArrowUp));
        step(&mut show, &mut state, None);
        assert_eq!((state.services.chosen, show.selected()), (Some(1), 1), "back to Beta");
    }

    #[test]
    fn up_and_down_in_the_editor_open_the_slide_before_and_after() {
        let show = show(3);
        let ctx = egui::Context::default();
        let mut state =
            HomeState { editing: EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }), ..HomeState::default() };
        let key =
            |key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let run = |state: &mut HomeState, events: Vec<Event>| {
            let raw = RawInput { focused: true, events, ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        let open = |state: &HomeState| state.editing.as_ref().map(|draft| draft.at.slide);
        run(&mut state, Vec::new());
        run(&mut state, vec![key(Key::ArrowDown)]);
        assert_eq!(open(&state), Some(1));
        run(&mut state, vec![key(Key::ArrowDown)]);
        run(&mut state, vec![key(Key::ArrowDown)]);
        assert_eq!(open(&state), Some(2), "stops at the last");
        run(&mut state, vec![key(Key::ArrowUp)]);
        assert_eq!(open(&state), Some(1));
    }

    #[test]
    fn command_and_shift_pick_slides_as_the_finder_does() {
        let at = |slide| SlideRef { presentation: 0, slide };
        let order: Vec<SlideRef> = (0..6).map(at).collect();
        let sorted = |picked: &[SlideRef]| {
            let mut slides: Vec<usize> = picked.iter().map(|at| at.slide).collect();
            slides.sort_unstable();
            slides
        };
        let mut picked = Vec::new();
        pick_slide(&mut picked, &order, at(1), Modifiers::COMMAND, None);
        pick_slide(&mut picked, &order, at(4), Modifiers::SHIFT, None);
        assert_eq!(sorted(&picked), [1, 2, 3, 4]);
        // Shift again runs from the same place, and only that range is picked.
        pick_slide(&mut picked, &order, at(2), Modifiers::SHIFT, None);
        assert_eq!(sorted(&picked), [1, 2]);
        pick_slide(&mut picked, &order, at(5), Modifiers::COMMAND, None);
        assert_eq!(sorted(&picked), [1, 2, 5]);
        pick_slide(&mut picked, &order, at(2), Modifiers::COMMAND, None);
        assert_eq!(sorted(&picked), [1, 5]);
        // Nothing picked yet: Shift runs from the slide clicked last.
        let mut picked = Vec::new();
        pick_slide(&mut picked, &order, at(3), Modifiers::SHIFT, Some(at(0)));
        assert_eq!(sorted(&picked), [0, 1, 2, 3]);
    }

    #[test]
    fn picking_follows_the_finder_with_command_and_shift() {
        let shown = [4, 7, 1, 9];
        let (mut picked, mut anchor) = (Vec::new(), None);
        pick(&mut picked, &mut anchor, &shown, 7, Modifiers::NONE);
        assert_eq!(picked, vec![7]);
        pick(&mut picked, &mut anchor, &shown, 9, Modifiers::SHIFT);
        assert_eq!(picked, vec![7, 1, 9], "Shift picks the run in the order shown");
        pick(&mut picked, &mut anchor, &shown, 1, Modifiers::COMMAND);
        assert_eq!(picked, vec![7, 9], "Command takes one away");
        pick(&mut picked, &mut anchor, &shown, 4, Modifiers::COMMAND);
        assert_eq!(picked, vec![7, 9, 4], "and adds one");
        pick(&mut picked, &mut anchor, &shown, 1, Modifiers::NONE);
        assert_eq!(picked, vec![1], "a plain click picks one alone");
    }

    #[test]
    fn the_edit_view_closes_if_its_song_is_gone() {
        let show = show(2);
        let mut editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
        if let Some(draft) = editing.as_mut() {
            draft.at = SlideRef { presentation: 5, slide: 0 };
        }
        let mut state = HomeState { editing, ..HomeState::default() };
        count_thumbs(&egui::Context::default(), &show, &mut state);
        assert!(state.editing.is_none());
    }

    #[test]
    fn the_bible_page_takes_the_place_of_the_slide_grid() {
        let show = show_with_media();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(slides, 0, "the slide grid is not drawn while the Bible page is open");
        assert_eq!(media, 2, "the media bin stays");
    }

    #[test]
    fn bible_rows_can_be_added_up_to_a_limit_and_removed_down_to_one() {
        let mut page = BiblePage::default();
        assert_eq!(page.rows.len(), 2);
        assert!(page.add_row());
        assert!(page.add_row());
        assert!(!page.add_row(), "four translations is the limit");
        assert!(page.remove_row(3));
        assert!(!page.remove_row(9));
        assert!(page.remove_row(0) && page.remove_row(0));
        assert_eq!(page.rows.len(), 1);
        assert!(!page.remove_row(0), "one row always stays");
    }

    #[test]
    fn an_object_is_listed_by_its_words_until_it_is_named() {
        let mut object = sp_model::Object::text(sp_model::Role::primary(), "\nLight rises\nover the hills");
        assert_eq!(listed_name(&object), "Light rises");
        object.name = "Lyrics Box".to_owned();
        assert_eq!(listed_name(&object), "Lyrics Box");
        let shape = sp_model::Object::shape(sp_model::Shape::Star);
        assert_eq!(listed_name(&shape), "Star");
    }

    #[test]
    fn a_library_search_finds_titles_first_then_words() {
        let show = Show::new(vec![
            std::sync::Arc::new(sp_model::parse_song_text("Harbour Lights", "When the night is long\n")),
            std::sync::Arc::new(sp_model::parse_song_text("Steady Ground", "You are the light on the farther side\n")),
        ]);
        let found = search_songs(&show, |_| true, "light");
        assert_eq!(found.iter().map(|(index, _)| *index).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(found[1].1.as_deref(), Some("You are the light on the farther side"));
        assert!(search_songs(&show, |_| true, "  ").is_empty());
        assert!(search_songs(&show, |index| index != 0, "harbour").is_empty(), "a song not listed is left out");
    }

    #[test]
    fn capitalize_makes_each_word_start_with_a_capital() {
        assert_eq!(capitalize("light rises over the hills"), "Light Rises Over The Hills");
        assert_eq!(capitalize("свет встаёт\nи утро"), "Свет Встаёт\nИ Утро");
        assert_eq!(capitalize("don't stop"), "Don't Stop");
    }

    #[test]
    fn the_screens_window_draws_every_kind_of_screen_and_changes_nothing_by_itself() {
        let ctx = egui::Context::default();
        let screens = vec![
            Screen::new("A".to_owned(), Output::Automatic),
            Screen::new("B".to_owned(), Output::Display("Projector".to_owned())),
            Screen::new("C".to_owned(), Output::Display("Gone".to_owned())),
            Screen::new("D".to_owned(), Output::Window),
            Screen::new("E".to_owned(), Output::Ndi("Main".to_owned())),
            Screen::new("F".to_owned(), Output::Off),
        ];
        for selected in 0..screens.len() + 1 {
            let mut state = HomeState {
                settings_open: true,
                screen_selected: selected,
                screens: screens.clone(),
                displays: vec![
                    DisplayInfo {
                        index: 0,
                        name: "Built-in".into(),
                        w: 3024,
                        h: 1964,
                        main: true,
                        ..Default::default()
                    },
                    DisplayInfo { index: 1, name: "Projector".into(), x: 3024, w: 1920, h: 1080, ..Default::default() },
                ],
                ..HomeState::default()
            };
            for _ in 0..2 {
                let mut drawn = ctx.run_ui(input(None), |ui| {
                    assert!(screens_ui(ui, &mut state, true).is_empty(), "nothing was clicked");
                });
                drawn.textures_delta.clear();
            }
            assert!(!state.screens_changed, "nothing was clicked, so nothing is changed");
            assert_eq!(state.screens, screens);
        }
        assert_eq!(output_label(&screens[2].output, &[]), "Gone   (disconnected)");
    }

    #[test]
    fn empty_show_draws_without_panicking() {
        let ctx = egui::Context::default();
        assert!(frame(&ctx, &Show::new(vec![]), None).0.is_empty());
    }

    #[test]
    fn offscreen_thumbnails_are_not_requested() {
        let ctx = egui::Context::default();
        let show = show(400);
        frame(&ctx, &show, None);
        let (_, asked) = frame(&ctx, &show, None);
        assert!(asked > 0, "visible thumbnails should be requested");
        assert!(asked < 80, "asked for {asked} thumbnails of 400");
    }
}
