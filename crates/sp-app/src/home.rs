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

mod slides;
use slides::{slide_area, slide_grid};

/// The pills that hold toolbar items, and text fields.
const FIELD: Color32 = crate::studio::CARD;
/// The outline of what is chosen.
const CHOSEN_LINE: Color32 = crate::studio::ACCENT;
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
    /// Items of the playlist in front, picked out or opened (see [`ItemPicks`]): Delete takes
    /// them out of the playlist, never deleting a song.
    Items,
}

/// The items of the playlist in front picked out, as songs are in a library: by their places,
/// with the one a Shift-click counts from, and the playlist as it was when they were picked.
/// When the playlist changes (another comes to the front, or items are added, moved or taken
/// out) the places no longer name the same items, and they are let go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemPicks {
    pub places: Vec<usize>,
    pub anchor: Option<usize>,
    of: Option<(usize, Vec<crate::service::Item>)>,
}

impl ItemPicks {
    /// Lets the picks go if the playlist in front is not the one they were made in, as it was.
    pub fn check(&mut self, services: &crate::service::Services) {
        if self.of.as_ref().is_some_and(|(at, items)| *at != services.current || *items != services.front().items) {
            *self = ItemPicks::default();
        }
    }

    /// Notes that the picks are of the playlist in front as it is now.
    pub fn made_in(&mut self, services: &crate::service::Services) {
        self.of = Some((services.current, services.front().items.clone()));
    }

    /// Whether several are picked, so that they are lit together and move together.
    pub fn several(&self) -> bool {
        self.places.len() > 1
    }
}

/// A slide picked to go on screen next by Enter (see [`HomeState::pick`]): where it is; in the
/// Bible view, the passage it is in, as the page names what it shows; and the slide on screen
/// when it was picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    pub at: SlideRef,
    pub passage: Option<String>,
    pub live_then: Option<SlideRef>,
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
    /// The sheet that adds a version to the theme being edited, or renames one, while it is open.
    pub version_sheet: Option<VersionSheet>,
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
    /// A playlist being renamed in the chooser: its place among the playlists, and the name as
    /// typed so far.
    pub renaming_service: Option<(usize, String)>,
    /// Displayed thumbnail width in points.
    /// Text typed into the library's filter box.
    pub filter: String,
    /// Set by the app when the output has stopped drawing, so the operator is told.
    pub output_stopped: bool,
    /// Displayed media thumbnail width in points.
    /// True while a file is being dragged over the window.
    pub drop_hover: bool,
    /// The files being dragged over the window, where the pointer is, and the places they could
    /// land (`file_drop.rs`).
    pub file_drag: crate::file_drop::FileDrag,
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
    /// Scroll the playlist to this item, once (after ↑ or ↓).
    pub playlist_scroll_to: Option<usize>,
    /// The Edit view's undo order, newest last: each step a change to the slide being edited
    /// (undone by the draft) or to the song's slides (moved, pasted, deleted; undone by the
    /// app); and the steps undone, to redo. The draft the slide's steps belong to.
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
    /// Something asked to play on the Announcements layer (`Some(None)` to stop it), and what
    /// plays there, with its name.
    pub announcement_request: Option<Option<crate::announcements::Announce>>,
    pub announcement_live: Option<(crate::announcements::Announce, String)>,
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
    /// The items of the playlist in front picked out (several with Command or Shift).
    pub item_picks: ItemPicks,
    /// What was just deleted that Undo can put back, for the app to keep in its one history
    /// (see `removed`).
    pub removed: Vec<crate::removed::Removed>,
    pub picking: Picking,
    /// A drag across a list picking things out, while it lasts.
    pub sweep: Option<Sweep>,
    /// A playlist item just opened (a click on its row, ↑ or ↓): the slides are scrolled so
    /// that it is at the top, once.
    pub slides_jump_to: Option<usize>,
    /// The slide on screen, and its place in its song as played, when the slides last looked:
    /// the view goes to the slide once, when that changes, and otherwise stays where the
    /// operator scrolled it.
    pub live_seen: Option<(SlideRef, Option<usize>)>,
    /// The playlist in front, its open item and the song chosen when the slides last looked,
    /// so that whichever of them changes next brings the others into step.
    pub open_seen: Option<(usize, Option<usize>, usize)>,
    /// The playlist item whose slide went on screen last (a click on it, or a key starting
    /// it), so a song the playlist holds twice is known to be on screen from the right place.
    pub live_item: Option<usize>,
    /// A slide found with the find box, lit for a moment with an ink ring, and when it was first
    /// drawn; and for how many more frames the view may still be brought to it (the song may
    /// first have to be shown).
    pub spotlight: Option<(SlideRef, Option<f64>)>,
    pub spotlight_scroll: u8,
    /// The slide picked to go on screen next by Enter (see `running` and [`HomeState::pick`]),
    /// and for how many more frames the view may still be brought to it.
    pub picked: Option<Picked>,
    pub pick_scroll: u8,
    /// A notice that comes and goes under the top bar (see `say`), and when it was first shown.
    pub toast: Option<(String, Option<f64>)>,
    /// The numbers on the open tool tab's cards or the media strip's pictures, after
    /// modifier-1 to 7 or Shift-modifier-M (see `keys`).
    pub numbers: Option<crate::keys::Numbers>,
    /// Whether each button's key is written beside it in a small tag, all the time (Help › Show
    /// Keys on Buttons, or the switch in the list of keys; kept with the preferences), and
    /// whether a testing aid shows them for a picture without changing the setting.
    pub keys_on_buttons: bool,
    pub key_tags_for_capture: bool,
    /// The list of every key is open.
    pub key_sheet: bool,
    /// Open the menu of the slide on screen, as its "more" button does (modifier-O): the frame
    /// it was asked in, as the slide may first need to be scrolled into view.
    pub live_menu: Option<u64>,
    /// The Bible passages kept in playlists, as slides.
    pub passages: slides::PassageDecks,
    /// How tall each slide's row is in the words-only and table views.
    pub rows: slides::RowHeights,
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
    /// Where each result the find box lists was drawn last, in the list's order (for a testing
    /// aid to drag one, and for tests).
    pub find_rows: Vec<Rect>,
    /// The find box's Bible results, and the Bible view's Search's verses as they are typed,
    /// each found on a thread of its own.
    pub finder: crate::find::Finder,
    pub page_finder: crate::find::Finder<crate::find::PageFound>,
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
    /// Canva Sync's page in Settings › Integrations, and what the top bar says of a sync.
    pub canva: crate::canva::View,
    /// Where the VoxLive item is among the presentations.
    pub voxlive_slot: Option<usize>,
    /// A VoxLive screen chosen in the VoxLive tab, by its slide in the VoxLive item, to put on
    /// screen.
    pub voxlive_go: Option<usize>,
    /// When a song's copyright line is shown, and the church's CCLI licence number.
    pub copyright_show: crate::service::CopyrightShow,
    pub ccli_licence: String,
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
    /// True while Edit › Stage is open: the open layout is then a draft in `editing` (made by
    /// `stage_editor::prepare`). The layout to open first, by its place in the list, and an
    /// object to pick when it opens (its place in the Objects list, front first).
    pub stage_editor_open: bool,
    pub stage_edit: (usize, Option<usize>),
    /// True while Edit › Props is open with no props yet: its list shows, empty, with Add.
    /// With a prop, the prop open is a draft in `editing` instead.
    pub props_editor_open: bool,
    /// A stage layout being renamed in the list, with its name so far, and whether that has
    /// only just begun (so its field takes the keyboard once).
    pub stage_rename: Option<(usize, String)>,
    pub stage_rename_started: bool,
    /// True when the Program preview shows what the stage screens show, not the audience's.
    pub preview_stage: bool,
    /// The one screen the Program preview looks at (the small arrow in its head), drawn as that
    /// screen draws it; `None` for what the room (or the stage) sees.
    pub preview_screen: Option<sp_model::Uuid>,
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
    /// The DeckLink devices found, and how the driver stands (no devices without it).
    pub decklink: sp_decklink::Found,
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
    /// Every version a theme has, for the menus that switch to one (see `versions`).
    pub versions: Vec<sp_model::Version>,
    /// The version the screens are in now, and when it ends.
    pub version_switch: crate::versions::Switch,
    /// What was asked about versions this frame, for the app to carry out.
    pub version_asks: Vec<crate::versions::Ask>,
    /// The templates of the theme songs use, by id and name, for a slide's Template menu.
    pub song_templates: Vec<(sp_model::Uuid, String)>,
    /// A slide whose template was just chosen from its menu (`None` lets the theme choose).
    /// The app changes the slide and the screen.
    pub pending_template: Option<(SlideRef, Option<sp_model::Uuid>)>,
    /// The template being renamed in the Theme Editor, and its name as typed so far.
    pub renaming_template: Option<(usize, String)>,
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
    /// The Bible page: what is typed in its reference field, its translations and the passage.
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

/// Which side of the Bible view's left column is open: the books, or the passages that were
/// on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BibleSide {
    #[default]
    Books,
    History,
}

/// How the Bible view's left column offers the books: a key for every book (the design's
/// keypad), or the scrolling book, chapter and verse lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BookPicker {
    #[default]
    Keypad,
    Lists,
}

/// How the Bible page shows a passage: its chapter as a page to read, or its slides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BibleView {
    #[default]
    Page,
    Slides,
}

/// What one of the Bible page's two places was given: the title of the slides made (what they
/// were made from: the passage, or the whole chapter round it), the passage asked for (as the
/// first translation names it: what a playlist keeps and the history notes), and the place of
/// the passage's slides among them. See `bible::page_deck`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub deck: String,
    pub passage: String,
    pub slides: std::ops::Range<usize>,
    /// The passage as it is kept (by Add, and in the history): named in the language it was
    /// typed or found in with that language's numbering, or as it was kept before, or as the
    /// first translation names it; and that numbering.
    pub kept: String,
    pub numbering: Option<sp_bible::Numbering>,
}

/// How the passage asked for is kept (by Add, and in the history): see `Held::kept`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Keep {
    /// As the first translation names and numbers it.
    #[default]
    Lead,
    /// Named in this language (`en`, `ru`, `uk`) with its numbering: the language it was typed
    /// or found in.
    In(String),
    /// As it was kept (a playlist's passage, the history's): this name, in this numbering (none
    /// for the first translation's).
    As(String, Option<sp_bible::Numbering>),
}

/// How a passage kept in a playlist is made into slides when it is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlidesOf {
    /// Its own slides, as the playlist shows them: for running the playlist, whose tiles and
    /// keys count its slides and go on into the next item after its last.
    Passage,
    /// As the page makes any passage (its whole chapter, with a slide a verse): for opening it
    /// on the Bible page to read round it.
    Page,
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
    /// The passage now shown: what was asked for (John 3:16), named as the first translation
    /// names it. The title names it, Add keeps it, and only it is lit as the passage on the page
    /// and the keys. Its slides may be its whole chapter's (see `held`). And the choices it was
    /// made with.
    pub shown: String,
    pub made_with: String,
    /// What each of the two places was last given, by place (as `slots`): the slides' title,
    /// the passage asked for and its slides among them.
    pub held: [Held; 2],
    /// How the passage shown is kept, for when it is made again or a verse longer.
    pub keep: Keep,
    /// The passage now shown as the operator reads it: named in the operator's language (the
    /// language chosen, or the one it was typed in, or the session's) and numbered as that
    /// language numbers (see `bible::numbering_of`). What Scripture Lookup says after a lookup.
    pub reading: String,
    /// The language a reference was last typed in this session (`en`, `ru`, `uk`): what the
    /// operator reads in when nothing typed says. See `session_language`.
    pub language: Option<String>,
    /// The computer's language, read once when the app starts: what a session starts in.
    pub system_language: crate::system_language::SystemLanguage,
    /// A passage just made. The app takes it and puts it in the slot named.
    pub pending: Option<(usize, sp_model::Presentation)>,
    /// What went wrong with the last lookup, for the operator.
    pub message: Option<String>,
    /// Verses found by the last search.
    pub found: Vec<crate::bible::FoundVerse>,
    /// The words typed in the reference field whose verses are listed now, if any.
    pub live_words: Option<String>,
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
    /// chapter open in them (chapter 0 for none yet), numbered as the page is read (see
    /// `page_bible`), the verse a Shift-click runs from, and whether to scroll them to what is
    /// open.
    pub lists_open: bool,
    /// Whether the table of translations (which Bibles, which boxes) is open, rather than
    /// folded into one line.
    pub rows_open: bool,
    pub list_book: usize,
    pub list_chapter: u16,
    pub list_from: Option<u16>,
    /// The numbering `list_chapter` and `list_from` are in; another lets go of `list_from`.
    pub list_numbering: Option<sp_bible::Numbering>,
    pub list_scroll: bool,
    /// A slide of the passage just looked up to put on screen once the app has it (a verse
    /// double-clicked in the lists), by its place in the passage.
    pub go_live: Option<usize>,
    /// Two verses made with the choices of the moment, for the Options panel's preview when
    /// no passage is showing, and the choices they were made with.
    pub sample: Option<(String, sp_model::Presentation)>,
    /// The left column's side, and how the passage is shown (kept between starts).
    pub side: BibleSide,
    pub view: BibleView,
    /// Keypad or lists for the books (kept between starts).
    pub books: BookPicker,
    /// The language chosen with the page's Аа button (`en`, `ru`, `uk`), or `None` for Auto:
    /// the language typed, else the session's. Kept between starts. See `Reading`.
    pub read_in: Option<String>,
    /// Open the Аа menu when the page is next drawn (a testing aid, for a picture of it).
    pub open_read_in: bool,
    /// The page's chapter as laid out, and what was on screen when it was last drawn (to keep
    /// the verse on screen in sight as it changes).
    pub page: crate::bible_page::PageCache,
    pub page_live_seen: Option<SlideRef>,
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
            held: Default::default(),
            keep: Keep::Lead,
            reading: String::new(),
            language: None,
            system_language: crate::system_language::SystemLanguage::default(),
            pending: None,
            message: None,
            found: Vec::new(),
            live_words: None,
            typed_at: None,
            focus_lookup: false,
            history: Vec::new(),
            history_changed: false,
            lists_open: true,
            rows_open: false,
            list_book: 0,
            list_chapter: 0,
            list_from: None,
            list_numbering: None,
            list_scroll: false,
            go_live: None,
            sample: None,
            side: BibleSide::Books,
            view: BibleView::Page,
            books: BookPicker::Keypad,
            read_in: None,
            open_read_in: false,
            page: crate::bible_page::PageCache::default(),
            page_live_seen: None,
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
                let lost = self.rows.iter().any(|row| row.bible == Some(index));
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
                // Only when a row lost its Bible do the boxes move up with the rows; otherwise
                // every translation keeps the boxes it was given.
                if lost {
                    self.assign_boxes();
                } else {
                    self.made_with.clear();
                }
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

    /// The passage now shown, as something a service can keep: as it was asked for, named in
    /// the language it was typed in with that language's numbering (or as it was kept, or as
    /// the first translation names it), the numbering kept beside it so it opens as the same
    /// verses whatever the Bibles' order.
    pub fn saved(&self) -> Option<crate::service::SavedPassage> {
        let held = &self.held[self.showing];
        (!self.shown.is_empty()).then(|| {
            let (lookup, numbering) = if held.passage == self.shown && !held.kept.is_empty() {
                (held.kept.clone(), held.numbering)
            } else {
                (self.shown.clone(), None)
            };
            crate::service::SavedPassage {
                lookup,
                numbering: numbering.map(|numbering| numbering.name().to_owned()),
                ..self.settings()
            }
        })
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
            numbering: None,
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
            show_with: None,
        }
    }

    /// Brings a saved passage back: its translations and options are set as they were, and
    /// it is looked up (named for the operator in the session's language), made into its own
    /// slides or as the page makes a passage (`slides`). A translation that is no longer
    /// installed is left out, and said so.
    pub fn open_saved(&mut self, saved: &crate::service::SavedPassage, live: Option<usize>, slides: SlidesOf) {
        let missing = self.apply_settings(saved);
        self.typed_at = None;
        self.lookup = saved.lookup.clone();
        let numbering = saved.numbering.as_deref().and_then(sp_bible::Numbering::from_name);
        self.make(live, false, false, slides == SlidesOf::Page, Keep::As(saved.lookup.clone(), numbering));
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
        // Its numbers in the numbering it was kept in, as its first translation numbers them.
        let in_use: Vec<&sp_bible::Bible> = choices.iter().map(|choice| choice.bible.as_ref()).collect();
        let numbering =
            saved.numbering.as_deref().and_then(sp_bible::Numbering::from_name).unwrap_or(in_use[0].info.numbering);
        let lookup = crate::bible::renumbered(&saved.lookup, numbering, &in_use);
        crate::bible::build_passage(&lookup, &choices, &options, self.template)
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
    /// presentation on screen now, if any. The operator reads it in the language it was typed
    /// in, and the session goes on in that language.
    pub fn look_up(&mut self, live: Option<usize>) {
        self.typed_at = None;
        self.make(live, false, true, true, Keep::Lead);
    }

    /// The same while typing: the slides follow what is typed, but the field is left as
    /// typed and a reference not yet finished says nothing.
    pub fn preview(&mut self, live: Option<usize>) {
        self.make(live, true, true, true, Keep::Lead);
    }

    /// Looks up a passage the operator did not type, named as the first translation names and
    /// numbers it. Its slides are made as for what is typed; the operator reads it in the
    /// session's language, which stays as it is. With a language (the find box's passages and
    /// verses, the page's verses found, what is clicked on the keypad), it is kept named in that
    /// language with its numbering; else as the first translation names it.
    pub fn open_in(&mut self, kept: String, language: Option<String>, live: Option<usize>) {
        self.typed_at = None;
        self.lookup = kept;
        let keep = language.filter(|language| !language.is_empty()).map_or(Keep::Lead, Keep::In);
        self.make(live, false, false, true, keep);
    }

    /// Looks up a passage as it was kept (the history's): `kept` named in the numbering given,
    /// or the first translation's, so it is the same verses whatever the Bibles' order.
    pub fn open_kept(&mut self, kept: String, numbering: Option<sp_bible::Numbering>, live: Option<usize>) {
        self.typed_at = None;
        self.lookup = kept.clone();
        self.make(live, false, false, true, Keep::As(kept, numbering));
    }

    /// Makes the passage shown again (other translations, boxes or options), kept as it was.
    pub fn remake(&mut self, live: Option<usize>) {
        self.typed_at = None;
        self.lookup = self.shown.clone();
        let keep = self.keep.clone();
        self.make(live, false, false, true, keep);
    }

    /// The Bibles in use, read, the first translation first.
    pub(crate) fn bibles_in_use(&mut self) -> Vec<std::sync::Arc<sp_bible::Bible>> {
        let (places, _) = self.in_use();
        places.into_iter().filter_map(|at| self.shelf.get(at).ok()).collect()
    }

    /// The Bible the page is read in: the first in use of the operator's language (see
    /// `bible_page::page_bibles`). Its numbering is the page's one numbering (the owner's rule,
    /// 2026-11-22): the keypad's and the lists' chapters and verses, the page's verse numbers and
    /// the title are numbered as it numbers.
    pub fn page_bible(&mut self) -> Option<std::sync::Arc<sp_bible::Bible>> {
        let (reader, _) = crate::bible_page::page_bibles(self)?;
        self.shelf.get(reader).ok()
    }

    /// The same for a book: the first translation when the page's Bible lacks the book (one
    /// with the New Testament and the Psalms only), so its chapters can still be chosen.
    pub fn page_bible_for(&mut self, book: usize) -> Option<std::sync::Arc<sp_bible::Bible>> {
        let lead = self.lead().and_then(|lead| self.shelf.get(lead).ok());
        match self.page_bible() {
            Some(reader) if reader.chapter_count(book) > 0 => Some(reader),
            reader => lead.or(reader),
        }
    }

    /// Opens the keypad and the lists on the passage shown, numbered as the page is read, and
    /// scrolls them to it when that is another chapter.
    fn sync_lists(&mut self) {
        let bibles = self.bibles_in_use();
        let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
        let Some(lead) = in_use.first() else { return };
        let Ok(range) = sp_bible::parse_reference_in(&self.shown, &lead.info.language) else { return };
        let Some(page) = self.page_bible_for(range.start.book) else { return };
        let there = crate::bible::renumber(range, lead.info.numbering, page.info.numbering, &in_use);
        // A verse a Shift-click runs from, counted in another numbering, is let go of.
        if self.list_numbering != Some(page.info.numbering) {
            self.list_numbering = Some(page.info.numbering);
            self.list_from = None;
        }
        if (there.start.book, there.start.chapter) != (self.list_book, self.list_chapter) {
            (self.list_book, self.list_chapter) = (there.start.book, there.start.chapter);
            self.list_scroll = true;
        }
    }

    /// Looks up a passage named and numbered as the page's Bible has it (`numbering`: a book,
    /// chapter or verse clicked on the keypad, in the lists or on the page), as the first
    /// translation numbers it, kept named in the language the page is read in.
    pub fn open_read(&mut self, name: String, numbering: sp_bible::Numbering, live: Option<usize>) {
        let bibles = self.bibles_in_use();
        let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
        let lookup = crate::bible::renumbered(&name, numbering, &in_use);
        let language = self.names_language();
        self.open_in(lookup, Some(language), live);
    }

    /// The place among the slides made for the passage shown of a verse, as the first
    /// translation numbers it: its place among their verses with a slide a verse, else the one
    /// slide. `None` when they don't hold it.
    pub fn slide_of(&mut self, verse: sp_bible::VerseRef) -> Option<usize> {
        let lead = self.lead().and_then(|lead| self.shelf.get(lead).ok())?;
        let deck = sp_bible::parse_reference_in(&self.held[self.showing].deck, &lead.info.language).ok()?;
        let places = lead.verses(deck);
        let at = places.iter().position(|(place, _)| *place == verse)?;
        Some(if self.options.break_on_new_verse { at } else { 0 })
    }

    /// The Bible in use whose numbers a reference typed this way is read in: the first of the
    /// numbering of the language typed (see `bible::numbering_of`), else the first Bible.
    pub fn counted_in(&mut self, typed: &str) -> Option<std::sync::Arc<sp_bible::Bible>> {
        let bibles = self.bibles_in_use();
        let lead = bibles.first()?.clone();
        let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
        let said = sp_bible::read_reference(typed, &lead.info.language).map(|r| r.languages).unwrap_or_default();
        let (_, languages) = self.in_use();
        let Some(typed_in) = crate::bible_language::typed_bible(&said, &self.session_language(), &languages).1 else {
            return Some(lead);
        };
        let numbering = crate::bible::numbering_of(&typed_in, &in_use);
        Some(bibles.iter().find(|bible| bible.info.numbering == numbering).cloned().unwrap_or(lead))
    }

    /// The place of the passage shown's slides among the slides made for it: its verses among
    /// its chapter's, or all of them.
    pub fn passage_slides(&self) -> std::ops::Range<usize> {
        self.held[self.showing].slides.clone()
    }

    /// The place of the passage asked for's slides among those of the presentation `deck` at
    /// `presentation`, when that is one of the page's places and the slides are some of the
    /// passage's chapter's: to ring them.
    pub fn ringed(&self, presentation: usize, deck: &sp_model::Presentation) -> Option<std::ops::Range<usize>> {
        let place = self.slots?.iter().position(|slot| *slot == presentation)?;
        let held = &self.held[place];
        let count = deck.slide_count();
        (held.deck == deck.title && held.slides.end <= count && held.slides.len() < count).then(|| held.slides.clone())
    }

    /// Whether the presentation at `presentation`, titled `title`, holds a playlist's passage
    /// kept as `lookup`: its slides were made from it (their title is it, or the place keeps it
    /// so, named in another language or numbering). How the playlist knows its passage is on
    /// screen.
    pub fn holds_kept(&self, presentation: usize, title: &str, lookup: &str) -> bool {
        lookup == title || self.passage_held(presentation, title).0 == lookup
    }

    /// The passage a slide of one of the page's places stands for, when it is on screen, and
    /// the numbering it is named in: the passage asked for whose slides the place holds (the
    /// presentation titled `title`), as it is kept; or the slides' own title (in the first
    /// translation's numbering) when the place holds something else.
    pub fn passage_held(&self, presentation: usize, title: &str) -> (String, Option<sp_bible::Numbering>) {
        let place = self.slots.and_then(|slots| slots.iter().position(|slot| *slot == presentation));
        match place.map(|place| &self.held[place]) {
            Some(held) if held.deck == title && !held.kept.is_empty() => (held.kept.clone(), held.numbering),
            _ => (title.to_owned(), None),
        }
    }

    /// The language the operator reads in when nothing typed says: the one a reference was
    /// last typed in this session, or else the computer's, or else English.
    pub fn session_language(&self) -> String {
        self.language
            .clone()
            .or_else(|| self.system_language.get().map(str::to_owned))
            .unwrap_or_else(|| "en".to_owned())
    }

    /// The Bibles in use, in the translations' order (a row set to none left out): each one's
    /// place on the shelf and its language.
    pub(crate) fn in_use(&self) -> (Vec<usize>, Vec<String>) {
        let places: Vec<usize> = self.rows.iter().filter_map(|row| row.bible).collect();
        let languages = places
            .iter()
            .map(|at| {
                self.shelf.bibles.get(*at).map_or_else(String::new, |b| crate::bible_language::language_of(&b.language))
            })
            .collect();
        (places, languages)
    }

    /// How the operator's language is decided now: the session's, and the language chosen with
    /// the Аа button, if one is.
    pub fn reading(&self) -> crate::bible_language::Reading {
        crate::bible_language::Reading { session: self.session_language(), chosen: self.read_in.clone() }
    }

    /// The language the operator reads in when nothing typed says, as a Bible in use has it:
    /// the language chosen when a Bible in use is in it; else the session's when one of the
    /// Bibles in use is in it, else the first Bible's. Books are named in it in the keypad and
    /// the lists.
    pub fn names_language(&self) -> String {
        let (_, languages) = self.in_use();
        let reading = self.reading();
        languages.get(crate::bible_language::reading_untyped(&reading, &languages)).cloned().unwrap_or(reading.session)
    }

    /// The language Auto would read in now, for the Аа menu's first choice.
    pub fn auto_language(&self) -> String {
        let (_, languages) = self.in_use();
        let session = self.session_language();
        languages.get(crate::bible_language::session_bible(&session, &languages)).cloned().unwrap_or(session)
    }

    /// The languages of the Bibles in use, each once, as the Аа menu offers them.
    pub fn languages_in_use(&self) -> Vec<String> {
        let (_, languages) = self.in_use();
        let mut each = Vec::new();
        for language in languages {
            if !language.is_empty() && !each.contains(&language) {
                each.push(language);
            }
        }
        each
    }

    /// Chooses the language the operator reads in (`None` for Auto). The passage showing is
    /// named again in it, in the field too unless something else has been typed there; its
    /// slides are not made again, so nothing on the screens changes.
    pub fn set_read_in(&mut self, chosen: Option<String>) {
        self.read_in = chosen;
        // Verses listed for words typed are asked for again, named in the new language.
        self.live_words = None;
        if self.shown.is_empty() {
            return;
        }
        // The keypad and the lists are numbered as the page is now read.
        self.sync_lists();
        let bibles = self.bibles_in_use();
        let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
        if in_use.is_empty() {
            return;
        }
        let renamed = crate::bible::name_for_operator(&self.shown, &in_use, &self.names_language());
        if self.lookup == self.reading {
            self.lookup = renamed.clone();
        }
        self.reading = renamed;
    }

    /// The language a reference typed this way would be named back in, as `look_up` would
    /// name it, without looking it up.
    pub fn typed_language(&self, typed: &str) -> String {
        let (_, languages) = self.in_use();
        // Read as `build_passage` reads it: the first Bible's language preferred.
        let lead =
            self.lead().and_then(|at| self.shelf.bibles.get(at)).map_or_else(String::new, |b| b.language.clone());
        let said = sp_bible::read_reference(typed, &lead).map(|reference| reference.languages).unwrap_or_default();
        let (reader, _) = crate::bible_language::reading_typed(&said, &self.reading(), &languages);
        languages.get(reader).cloned().unwrap_or_default()
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

    /// Makes the passage `lookup` names and readies it for the app. The slides are made from
    /// the translations chosen whatever the language typed; with `around`, as the page makes a
    /// passage (its whole chapter, with a slide a verse: see `bible::page_deck`), else the
    /// passage's own. What the operator reads is named in the language it was typed in
    /// (`typed`), or the session's, and the field then says it (unless `quiet`, while typing).
    ///
    /// Chapter and verse numbers follow the language (the owner's rule, 2026-11-22): what is
    /// typed is read in the numbering of the language typed ("Psalm 23" in English is the
    /// English 23rd psalm, the Synodal's 22nd, whichever Bible is first), and a name the
    /// operator reads is numbered as its language numbers. A passage kept (`keep: As`) is read in
    /// the numbering it was kept in. Inside, everything is numbered as the first translation
    /// numbers it. See `bible::numbering_of`.
    fn make(&mut self, live: Option<usize>, quiet: bool, typed: bool, around: bool, keep: Keep) {
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
        let bibles: Vec<std::sync::Arc<sp_bible::Bible>> = rows.iter().map(|row| row.bible.clone()).collect();
        let in_use: Vec<&sp_bible::Bible> = bibles.iter().map(|bible| bible.as_ref()).collect();
        let first_numbering = in_use[0].info.numbering;
        let languages: Vec<String> =
            rows.iter().map(|row| crate::bible_language::language_of(&row.bible.info.language)).collect();
        let reading = self.reading();
        // Typed: the languages its book's name was typed in, and the one it was typed in (what
        // the session goes on in, whatever the language chosen), whose numbering it is read in.
        let said = if typed {
            sp_bible::read_reference(&self.lookup, &in_use[0].info.language)
                .map(|reference| reference.languages)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let typed_in = crate::bible_language::typed_bible(&said, &reading.session, &languages).1.filter(|_| typed);
        let lookup = match (&typed_in, &keep) {
            (Some(language), _) => {
                crate::bible::renumbered(&self.lookup, crate::bible::numbering_of(language, &in_use), &in_use)
            }
            (None, Keep::As(name, numbering)) => {
                crate::bible::renumbered(name, numbering.unwrap_or(first_numbering), &in_use)
            }
            _ => self.lookup.clone(),
        };
        let keep = typed_in.clone().map_or(keep, Keep::In);
        match crate::bible::build_passage(&lookup, &rows, &self.options, self.template) {
            Ok(asked) => {
                let passage_name = asked.title.clone();
                let (passage, slides) = if around {
                    crate::bible::page_deck(asked, &rows, &self.options, self.template)
                } else {
                    let count = asked.slide_count();
                    (asked, 0..count)
                };
                // Leave a passage that is on screen alone: use the other slot.
                if live == Some(slots[self.showing]) {
                    self.showing = 1 - self.showing;
                }
                let (kept, numbering) = match &keep {
                    Keep::Lead => (passage_name.clone(), first_numbering),
                    Keep::In(language) => (
                        crate::bible::named_in(&passage_name, first_numbering, language, &in_use, false),
                        crate::bible::numbering_of(language, &in_use),
                    ),
                    Keep::As(name, numbering) => (name.clone(), numbering.unwrap_or(first_numbering)),
                };
                self.held[self.showing] = Held {
                    deck: passage.title.clone(),
                    passage: passage_name.clone(),
                    slides,
                    kept,
                    numbering: Some(numbering),
                };
                self.keep = keep;
                self.pending = Some((slots[self.showing], passage));
                self.shown = passage_name.clone();
                self.made_with = self.choices();
                self.message = None;
                // The Bible the operator reads it in: the one in use of the language typed, or of
                // the session's when nothing was typed; the first Bible when that language has
                // none. Typing in a language makes it the session's. A language chosen with the
                // Аа button holds, while a Bible in use is in it; the session still goes on in
                // the language typed. The name is numbered as its language numbers.
                let reader = if typed {
                    let (reader, goes_on) = crate::bible_language::reading_typed(&said, &reading, &languages);
                    if goes_on.is_some() {
                        self.language = goes_on;
                    }
                    reader
                } else {
                    crate::bible_language::reading_untyped(&reading, &languages)
                };
                self.reading =
                    crate::bible::name_for_operator(&passage_name, &in_use, &languages[reader.min(rows.len() - 1)]);
                // The lists open on the passage, wherever it came from.
                self.sync_lists();
                if !quiet {
                    // The field then says the passage in full, in the operator's language:
                    // "john 3" becomes "John 3:1-36", "ин 3" "От Иоанна 3:1-36".
                    self.lookup = self.reading.clone();
                    self.found.clear();
                }
            }
            Err(problem) if !quiet => self.message = Some(problem),
            Err(_) => {}
        }
    }

    /// Notes a passage that has just gone on screen, at the top of the history. One already
    /// there moves up; the same passage again (its next verse, say) changes nothing.
    pub fn note_shown(&mut self, passage: &str, numbering: Option<sp_bible::Numbering>, at: chrono::NaiveDateTime) {
        if passage.is_empty() || self.history.first().is_some_and(|newest| newest.passage == passage) {
            return;
        }
        self.history.retain(|entry| entry.passage != passage);
        let at = at.format("%Y-%m-%dT%H:%M:%S").to_string();
        let numbering = numbering.map(|numbering| numbering.name().to_owned());
        self.history.insert(0, crate::service::HistoryEntry { passage: passage.to_owned(), at, numbering });
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
                // Still kept in the language it was typed in.
                let keep = match &self.keep {
                    Keep::In(language) => Keep::In(language.clone()),
                    _ => Keep::Lead,
                };
                self.typed_at = None;
                self.lookup = bible.label(longer.start, longer.end);
                self.make(live, false, false, true, keep);
            }
            None => {
                let end = if forward { "end" } else { "start" };
                let book = sp_bible::book_name(range.start.book, &self.names_language());
                self.message = Some(format!("That is the {end} of {book}."));
            }
        }
    }

    /// Finds verses holding the words typed in Search: in the Bibles in use written in the
    /// words' alphabet (the session's language first), or, with none, in the first translation
    /// as before. Each verse is named in the language of the Bible it was found in, with that
    /// Bible's numbers, and opened as the first translation numbers it.
    pub fn find(&mut self) {
        self.found.clear();
        self.message = None;
        let (places, _) = self.in_use();
        let mut bibles = Vec::new();
        for (at, place) in places.into_iter().enumerate() {
            match self.shelf.get(place) {
                Ok(bible) => bibles.push((bible.info.short.clone(), bible)),
                // The first numbers the verses: without it there is nothing to name them by.
                Err(problem) if at == 0 => {
                    self.message = Some(problem);
                    return;
                }
                Err(problem) => self.message = Some(problem),
            }
        }
        let words = self.search.clone();
        self.show_found(crate::find::search_page(&words, &bibles, &self.reading()));
    }

    /// Lists verses Search found (in place of the slides), or says why there are none.
    pub fn show_found(&mut self, found: crate::find::PageFound) {
        self.found = found.verses;
        if self.message.is_none() || !self.found.is_empty() {
            self.message = found.message;
        }
    }

    /// Opens a verse found by Search, or its chapter: the session goes on in the language of
    /// the Bible it was found in.
    pub fn open_found(&mut self, found: &crate::bible::FoundVerse, chapter: bool, live: Option<usize>) {
        if !found.language.is_empty() {
            self.language = Some(found.language.clone());
        }
        let kept = if chapter { found.kept_chapter.clone() } else { found.kept.clone() };
        self.open_in(kept, Some(found.language.clone()), live);
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
    /// The templates shown in the left column: the usual ones (`None`) or a version's.
    pub version: Option<sp_model::Uuid>,
    /// Which of those templates is on the canvas.
    pub template: usize,
    /// Set when the theme has changed this frame. The app takes the theme and clears this.
    pub changed: bool,
    /// Set when the operator asks to go back to the Show view.
    pub leave: bool,
}

impl ThemeEdit {
    pub fn of(index: usize, theme: Theme) -> ThemeEdit {
        ThemeEdit { index, theme, version: None, template: 0, changed: false, leave: false }
    }

    /// The templates shown: the usual ones, or the version's.
    pub fn templates(&self) -> &[sp_model::ThemeTemplate] {
        self.theme.set(self.version)
    }

    /// The template on the canvas.
    pub fn open_template(&self) -> Option<&sp_model::ThemeTemplate> {
        self.templates().get(self.template)
    }

    /// The template on the canvas, to change.
    pub fn open_template_mut(&mut self) -> Option<&mut sp_model::ThemeTemplate> {
        let at = self.template;
        self.theme.set_mut(self.version).and_then(|templates| templates.get_mut(at))
    }

    /// Shows the usual templates or a version's (one the theme no longer has shows the usual
    /// ones), with one of them on the canvas. Returns the draft to edit it in.
    pub fn show(&mut self, version: Option<sp_model::Uuid>, template: usize) -> Option<EditDraft> {
        self.version = version.filter(|id| self.theme.has_version(*id));
        self.template = template.min(self.templates().len().saturating_sub(1));
        self.open_template().map(|opened| EditDraft::of_slide(opened.as_slide(), self.template))
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
    /// Move the theme at a place in the list to another (never the built-in theme, which stays
    /// first).
    Move(usize, usize),
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
    /// A slide of a song, by the song's place in the show: drawn with that song's own theme,
    /// whichever song is chosen.
    SongSlide(usize, &'a Slide),
    /// A slide of a playlist item set to show in a theme version, drawn in it: a song's (by its
    /// place, with its own theme) or, with none, a Bible passage's (with the Bible theme).
    InVersion(Option<usize>, &'a Slide, sp_model::Uuid),
    /// A media item, by its index in the show.
    Media(usize),
    /// A slide drawn with a theme of its own (a prop's, a stage layout's), kept under the
    /// slide's id; `true` draws it again first, for when what it shows has changed.
    Own(&'a Slide, &'a sp_model::Theme, bool),
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
            version_sheet: None,
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
            file_drag: crate::file_drop::FileDrag::default(),
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
            canva: Default::default(),
            voxlive_slot: None,
            voxlive_go: None,
            copyright_show: crate::service::CopyrightShow::Never,
            ccli_licence: String::new(),
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
            picked_slides: Vec::new(),
            slide_rects: Vec::new(),
            picked_songs: Vec::new(),
            song_anchor: None,
            picked_media: Vec::new(),
            media_anchor: None,
            item_picks: ItemPicks::default(),
            removed: Vec::new(),
            picking: Picking::Nothing,
            sweep: None,
            slides_jump_to: None,
            live_seen: None,
            open_seen: None,
            passages: slides::PassageDecks::default(),
            rows: slides::RowHeights::default(),
            removed_songs: Vec::new(),
            media_request: None,
            library_search: None,
            find_rect: None,
            find_rows: Vec::new(),
            finder: crate::find::Finder::default(),
            page_finder: crate::find::Finder::new(crate::find::search_page),
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
            live_item: None,
            toast: None,
            spotlight: None,
            spotlight_scroll: 0,
            picked: None,
            pick_scroll: 0,
            numbers: None,
            keys_on_buttons: false,
            key_tags_for_capture: false,
            key_sheet: false,
            live_menu: None,

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
            props_editor_open: false,
            stage_rename: None,
            stage_rename_started: false,
            preview_stage: false,
            preview_screen: None,
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
            decklink: sp_decklink::Found { driver: sp_decklink::Driver::Looking, devices: Vec::new() },
            inspector: crate::inspector::InspectorState::default(),
            theme_edit: None,
            theme_names: Vec::new(),
            themes_kept: vec![0],
            template_names: Vec::new(),
            versions: Vec::new(),
            version_switch: crate::versions::Switch::default(),
            version_asks: Vec::new(),
            song_templates: Vec::new(),
            pending_template: None,
            renaming_template: None,
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
            Some(match draft.at.presentation {
                crate::props::PROP_SLOT => EditKind::Props,
                crate::stage::STAGE_SLOT => EditKind::Stage,
                _ => EditKind::Slides,
            })
        } else if self.props_editor_open {
            Some(EditKind::Props)
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

    /// Whether the Bible view is open, or asked for this frame (the switch comes in the next).
    fn bible_view_now(&self) -> bool {
        match self.view_request {
            Some(view) => view == 2,
            None => self.bible_open,
        }
    }

    /// The slide picked to go on screen next by Enter, while it still can be: in the view open
    /// now (in the Bible view, in the passage shown; in the Show view, in the song chosen), there
    /// and switched on, and not on screen. A slide of its passage or song gone on screen since
    /// it was picked (by Enter, a click or a key) has taken its place.
    pub fn pick(&self, show: &Show) -> Option<SlideRef> {
        let picked = self.picked.as_ref()?;
        let at = picked.at;
        let editing = self.view_request.is_none() && self.edit_kind().is_some();
        let in_view = !editing
            && match &picked.passage {
                Some(passage) => {
                    self.bible_view_now()
                        && *passage == self.bible.shown
                        && self.bible.slots.map(|slots| slots[self.bible.showing]) == Some(at.presentation)
                }
                None => !self.bible_view_now() && show.selected() == at.presentation,
            };
        let there = show.presentations().get(at.presentation).and_then(|p| p.slide(at.slide));
        let moved_on = show
            .live()
            .filter(|live| live.presentation == at.presentation)
            .is_some_and(|live| Some(live) != picked.live_then);
        (in_view && there.is_some_and(|(_, slide)| slide.enabled) && !moved_on && show.live() != Some(at)).then_some(at)
    }

    /// What the slide picked is, for the notice that says Enter puts it on screen.
    pub fn picked_word(&self, show: &Show) -> Option<&'static str> {
        self.pick(show)?;
        Some(if self.picked.as_ref().is_some_and(|picked| picked.passage.is_some()) { "verse" } else { "slide" })
    }

    /// Picks a slide to go on screen next by Enter, and brings it into view: a verse of the
    /// passage the Bible view shows (`bible`), or a slide of the song chosen.
    pub fn set_pick(&mut self, show: &Show, at: SlideRef, bible: bool) {
        let passage = bible.then(|| self.bible.shown.clone());
        self.picked = Some(Picked { at, passage, live_then: show.live() });
        self.pick_scroll = 12;
    }

    /// Picks the first verse of the passage the Bible view has just looked up (among its
    /// chapter's slides, when it was made into them), if it made one. Returns whether it did.
    pub fn pick_passage(&mut self, show: &Show) -> bool {
        let first = self.bible.passage_slides().start;
        self.pick_made_verse(show, first)
    }

    /// Picks a slide (by its place, its first when it hasn't that many) of the passage the Bible
    /// view has just looked up, if it made one. Returns whether it did.
    pub fn pick_made_verse(&mut self, show: &Show, slide: usize) -> bool {
        let made = self.bible.pending.as_ref().map(|(slot, passage)| (*slot, passage.slide_count()));
        let showing = self.bible.slots.map(|slots| slots[self.bible.showing]);
        match made.filter(|(slot, count)| Some(*slot) == showing && *count > 0) {
            Some((slot, count)) => {
                let slide = if slide < count { slide } else { 0 };
                self.set_pick(show, SlideRef { presentation: slot, slide }, true);
                true
            }
            None => false,
        }
    }

    /// Lets the slide picked go once it can no longer be put on screen (see [`HomeState::pick`]),
    /// so it never comes back later to be put on by surprise.
    fn let_go_of_old_pick(&mut self, show: &Show) {
        if self.picked.is_some() && self.pick(show).is_none() {
            self.picked = None;
        }
    }

    /// Renames a timer (by its place), and in the same change every message that shows it, so
    /// a message on the screens goes on showing its time. A name another timer has is refused,
    /// as is an empty one: the words say why. Returns the name it had.
    pub fn rename_timer(&mut self, index: usize, name: &str) -> Result<String, String> {
        let name = name.trim();
        let Some(was) = self.timers.get(index).map(|timer| timer.name.clone()) else {
            return Err("There is no such timer.".to_owned());
        };
        if name.is_empty() {
            return Err("A timer needs a name.".to_owned());
        }
        if name == was {
            return Ok(was);
        }
        if self.timers.iter().enumerate().any(|(at, timer)| at != index && timer.name.eq_ignore_ascii_case(name)) {
            return Err(format!("There is a timer called {name} already."));
        }
        // A message's token names the first timer called so; only that timer takes it along.
        let shows_it = self.timers.iter().position(|timer| timer.name == was) == Some(index);
        if shows_it && crate::messages::rename_timer(&mut self.messages, &was, name) {
            self.messages_changed = true;
        }
        self.timers[index].name = name.to_owned();
        self.timers_changed = true;
        Ok(was)
    }

    /// Says something for a moment, under the top bar (that the playlist has ended, say). It
    /// changes nothing.
    pub fn say(&mut self, words: impl Into<String>) {
        self.toast = Some((words.into(), None));
    }

    /// True while a question asking whether to delete something is open: then no key reaches
    /// the show (Space, the arrows and Escape answer it or wait).
    pub fn alert_open(&self) -> bool {
        !self.confirm_delete_songs.is_empty()
            || !self.confirm_delete_media.is_empty()
            || self.confirm_delete_theme.is_some()
    }

    /// Whether an Escape pressed now belongs to something over the window (a menu, a question,
    /// a sheet, the find box, the list of keys, the setup), which takes it before the chooser or
    /// the Reflow editor under it.
    pub fn escape_taken_above(&self, ctx: &egui::Context) -> bool {
        egui::Popup::is_any_open(ctx)
            || self.key_sheet
            || self.alert_open()
            || self.new_song_sheet.is_some()
            || self.library_search.is_some()
            || self.setup.is_some()
            || !self.bible.options.details.is_empty()
    }

    /// Whether the playlist chooser or the Reflow editor is open in the Show view, where
    /// Escape closes it instead of clearing the screens.
    pub fn show_view_overlay_open(&self) -> bool {
        let show_view = !self.bible_open && self.editing.is_none();
        show_view && (self.chooser_open || self.reflow.is_some())
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
            start_audience_off: false,
            stage_off: !self.stage_on,
            setup_done: self.setup_done,
            bible_lists_hidden: !self.bible.lists_open,
            bible_rows_open: self.bible.rows_open,
            bible_view: match self.bible.view {
                BibleView::Page => "page",
                BibleView::Slides => "slides",
            }
            .to_owned(),
            bible_books: match self.bible.books {
                BookPicker::Keypad => "keypad",
                BookPicker::Lists => "lists",
            }
            .to_owned(),
            bible_language: self.bible.read_in.clone().unwrap_or_else(|| "auto".to_owned()),
            playlist_in_front: self.playlist_open,
            song_order: self.song_order.name().to_owned(),
            keys_on_buttons: self.keys_on_buttons,
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
        self.stage_on = !prefs.stage_off;
        self.setup_done = prefs.setup_done;
        self.bible.lists_open = !prefs.bible_lists_hidden;
        self.bible.rows_open = prefs.bible_rows_open;
        self.bible.view = if prefs.bible_view == "slides" { BibleView::Slides } else { BibleView::Page };
        self.bible.books = if prefs.bible_books == "lists" { BookPicker::Lists } else { BookPicker::Keypad };
        self.bible.read_in = match prefs.bible_language.as_str() {
            language @ ("en" | "ru" | "uk") => Some(language.to_owned()),
            _ => None,
        };
        self.playlist_open = prefs.playlist_in_front;
        self.song_order = crate::kept::SongOrder::named(&prefs.song_order);
        self.keys_on_buttons = prefs.keys_on_buttons;
    }

    /// How long a new picture takes to fade in on the output; zero when slides cut.
    pub fn dissolve_seconds(&self) -> f32 {
        self.transition.duration()
    }
}

pub fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// True when a title matches what was typed in the filter box (case-insensitive, any part).
pub fn matches_filter(title: &str, filter: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty() || title.to_lowercase().contains(&filter.to_lowercase())
}

/// True when this frame has a press of `key` with Command (and Shift, exactly when `shift`).
/// It is judged by the key press itself, not by the keys held now, so a shortcut chosen from
/// the menu bar, which arrives with no key held, counts as well.
pub fn command_key(input: &egui::InputState, key: egui::Key, shift: bool) -> bool {
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
    // The window's own colour behind everything, the gaps between the three columns and round
    // their corners included: the margins a column's frame leaves are not painted by it, and
    // would otherwise show the black the window is cleared to.
    ui.ctx().layer_painter(egui::LayerId::background()).rect_filled(ui.ctx().content_rect(), 0.0, crate::studio::BG);
    // Files dragged from the Finder: the place under the pointer, from the last frame's places.
    crate::file_drop::begin_frame(ui.ctx(), state);
    // The lists made from the songs are made again only once the songs have changed.
    let hidden = state.hidden_slots();
    state.lists.songs.check(show, &state.song_keys, hidden, &state.removed_songs);
    // Text boxes are dark wells in the lighter side columns.
    ui.visuals_mut().extreme_bg_color = FIELD;
    // The keys for getting around: the views, the tool tabs and their numbers, the screens.
    let voxlive =
        state.voxlive_slot.and_then(|slot| show.presentations().get(slot)).is_some_and(|v| v.slide_count() > 0);
    match crate::keys::frame(ui.ctx(), state, voxlive) {
        Some(crate::keys::Ask::Audience) => commands.push(Command::SetOutputEnabled(!show.output_enabled())),
        // A Bible verse or VoxLive's captions, which the app makes, have no menu: it says so.
        Some(crate::keys::Ask::LiveMenu) if show.live().is_some_and(|at| state.app_made(at.presentation)) => {
            state.say("The verse or caption on screen has no menu.");
        }
        // The menu of the slide on screen: its slide is brought into view, and its menu opened.
        Some(crate::keys::Ask::LiveMenu) if show.live().is_some() && !state.bible_open => {
            state.live_menu = Some(ui.ctx().cumulative_frame_nr());
        }
        Some(crate::keys::Ask::LiveMenu) => state.say("Put a slide on screen to open its menu."),
        None => {}
    }
    // A press that ends on a slider or a menu leaves the keyboard to the show again.
    release_focus(ui.ctx());
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
        // Bible files that don't say what they are: each asked about in turn, then installed on
        // a thread of its own (`main.rs` starts it).
        let waiting = bible.options.details.len().saturating_sub(1);
        let decided =
            bible.options.details.front_mut().and_then(|sheet| crate::bible::details_sheet(ui.ctx(), sheet, waiting));
        if let Some(install) = decided
            && let Some(sheet) = bible.options.details.pop_front()
            && install
            && let Some(details) = sheet.imported.details(&sheet.about)
        {
            bible.options.installs.push((sheet.imported, details));
        }
    }
    confirm_delete_theme(ui, state);
    version_sheet(ui, state);
    confirm_delete_song(ui, show, state);
    confirm_delete_media(ui, show, state);
    // Delete (or Backspace) deletes what is picked out in the list last clicked: what Undo
    // brings back at once (slides, playlist items), the rest after asking; with neither list
    // last clicked, the slide picked out by a right-click.
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
            // In a media playlist's own view, the items picked are taken out of it at once (one
            // that follows a folder says why it can't be); otherwise deleted from the bin, after
            // asking.
            Picking::Media => {
                let picked: Vec<usize> =
                    state.picked_media.iter().copied().filter(|at| *at < show.media().len()).collect();
                let shown = state.media_playlist_shown.and_then(|id| state.media_playlists.iter().find(|p| p.id == id));
                match shown.map(|playlist| (playlist.arranged_by_hand(), playlist.name.clone())) {
                    Some(_) if picked.is_empty() => {}
                    Some((true, _)) => crate::studio::remove_from_media_playlist(show, state, &picked),
                    Some((false, name)) => {
                        state.say(format!("\u{201c}{name}\u{201d} follows a folder: take the file out of the folder."));
                    }
                    None => state.confirm_delete_media = picked,
                }
            }
            // Slides go at once, asking nothing (Undo puts them back): those picked out of one
            // song, the song of the slide picked last. A slide the app made has none to delete.
            Picking::Slides => {
                if let Some((song, slides)) = picks_of_one_song(&state.picked_slides)
                    && !state.app_made(song)
                {
                    delete_slides(state, song, &slides);
                }
            }
            Picking::Nothing => {
                if let Some(at) = state.selected_slide.filter(|at| !state.app_made(at.presentation)) {
                    delete_slides(state, at.presentation, &[at.slide]);
                }
            }
            // The playlist's items picked (or the one open) are taken out of it at once: no
            // song is deleted, so nothing is asked, and Undo puts them back. Only while the
            // keyboard is the left column's, as for ↑ and ↓ (the Show view, the search closed,
            // the list of keys away), with the playlist listed and the chooser put away.
            Picking::Items
                if state.playlist_open && !state.chooser_open && crate::studio::list_keys_work(ui, state) =>
            {
                let places = crate::studio::items_to_take_out(state);
                state.take_out_items(state.services.current, &places);
            }
            // Themes are deleted in the Theme Editor, where the list is.
            Picking::Themes | Picking::Items => {}
        }
    }
    // After the view asked for has opened, and before the find box can pick something new.
    state.let_go_of_old_pick(show);
    // Testing aid (see `find_return_words`): the words typed, once.
    static FIND_TYPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Some(words) = find_return_words()
        && !FIND_TYPED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        state.library_search = Some((words.to_owned(), 0));
    }
    library_search(ui, show, state, &mut commands);
    // Edit › Stage edits the open layout as a draft, as the other kinds do.
    crate::stage_editor::prepare(state);
    if state.editing.is_some() {
        edit_view(ui, show, state, theme, thumb, canvas, preview, &mut commands);
        return commands;
    }
    if state.props_editor_open && state.theme_edit.is_none() {
        empty_props_view(ui, show, state, preview, &mut commands);
        return commands;
    }
    // In the Bible view the left column holds the books and the history instead (none when
    // they are put away).
    if !state.bible_open {
        crate::studio::library(ui, show, state, &mut commands);
    } else if state.bible.lists_open {
        crate::bible_page::left_column(ui, show, state);
    } else {
        // Put away: the middle still stands clear of the window's edge.
        egui::Panel::left("bible books away")
            .resizable(false)
            .show_separator_line(false)
            .exact_size(crate::studio::GAP)
            .frame(Frame::NONE)
            .show(ui, |_| {});
    }
    crate::studio::right_column(ui, show, state, preview, &mut commands, |ui, state| {
        crate::live_tabs::live_tabs(ui, show, state, &mut *thumb);
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
    crate::keys::key_sheet(ui.ctx(), state);
    crate::studio::toast(ui.ctx(), state);
    crate::file_drop::paint(ui.ctx(), state);
    commands
}

/// After the pointer lets go of a slider, a dropdown or a button, the keyboard goes back to
/// running the show: nothing but a text field keeps it. Not while a menu is open, where the
/// arrows move between its choices.
fn release_focus(ctx: &egui::Context) {
    let released = ctx.input(|input| input.pointer.any_released());
    if released && !ctx.text_edit_focused() && !egui::Popup::is_any_open(ctx) {
        ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }
}

/// Edit › Props with no props yet: the props' list, empty, with its Add key, which makes the
/// first and opens it; the right column with nothing to inspect under Clear All, and a word in
/// the middle on what a prop is.
fn empty_props_view(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    preview: Preview,
    commands: &mut Vec<Command>,
) {
    use crate::studio::Column;
    let left = egui::Panel::left("edit slides")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(crate::studio::column_outer(crate::studio::LEFT_WIDTH))
        .frame(crate::studio::column_frame(Column::Left));
    let left = left.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        crate::studio::edit_kinds(ui, state);
        if let Some(ask) = prop_list_head(ui, state, false) {
            apply_prop_ask(state, ask);
        }
        Frame::new().inner_margin(egui::Margin::symmetric(18, 6)).show(ui, |ui| {
            ui.label(egui::RichText::new("No props yet. Add makes one.").size(12.0).color(DIM));
        });
    });
    crate::studio::round_corners(ui, crate::studio::column_panel(left.response.rect, Column::Left));
    // Nothing to inspect yet: the column ends at Clear All.
    crate::studio::right_column(ui, show, state, preview, commands, |_, _| {});
    let middle = crate::studio::column_panel(ui.available_rect_before_wrap(), Column::Middle);
    let surround = panel_frame(EDIT_SURROUND, 0).outer_margin(crate::studio::column_margin(Column::Middle));
    egui::CentralPanel::default().frame(surround).show(ui, |ui| {
        let room = ui.max_rect();
        ui.painter().text(
            room.center(),
            Align2::CENTER_CENTER,
            "A prop stays over the slides until it is taken off, such as a name in a corner.",
            FontId::proportional(13.0),
            DIM,
        );
    });
    crate::studio::round_corners(ui, middle);
}

const EDIT_PANEL: Color32 = crate::studio::PANEL;
const EDIT_HEADER: Color32 = Color32::from_rgb(44, 44, 46);
const EDIT_SURROUND: Color32 = Color32::from_rgb(54, 54, 56);
const EDIT_STATUS: Color32 = crate::studio::PANEL;
const EDIT_SELECTED: Color32 = crate::studio::SELECTED;
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
    // A stage layout is edited with the stage's plain look and nothing drawn behind it.
    let stage_mode = at.presentation == crate::stage::STAGE_SLOT && !theme_mode;
    let stage_theme = stage_mode.then(|| state.stage_layouts.get(at.slide).map(crate::stage::editor_theme)).flatten();
    if presentation.is_none() && !theme_mode && !prop_mode && stage_theme.is_none() {
        state.editing = None;
        return;
    }
    let theme = stage_theme.as_ref().unwrap_or(theme);
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
    // The keyboard is taken while a field is typed in, and while a question is open (whether to
    // delete a prop, a stage layout, a template or slides): then Delete, the arrows and the
    // other keys never reach the slide, its objects or the list behind the question.
    let keys_taken = ui.ctx().text_edit_focused() || state.alert_open();

    // Keys: undo and redo, delete and nudging, each only when the keyboard isn't taken.
    let (undo, redo, delete, nudge) = ui.input(|input| {
        let undo = command_key(input, egui::Key::Z, false);
        let redo = command_key(input, egui::Key::Z, true);
        let delete = !keys_taken && (input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace));
        // On a stage layout the arrows move 10 and Shift one pixel, as the new design has it;
        // on a slide, a template or a prop, still one, and Shift 10.
        let step = match (stage_mode, input.modifiers.shift) {
            (true, true) | (false, false) => 1.0,
            (true, false) | (false, true) => 10.0,
        };
        let mut nudge = (0.0, 0.0);
        if !keys_taken {
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
    if !keys_taken {
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
            } else if copy && draft.selected.is_none() && !theme_mode && !prop_mode && !stage_mode {
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
                && !stage_mode
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
    if !keys_taken && !ui.ctx().text_edit_focused() && state.editing.is_some() && state.renaming_template.is_none() {
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
        // draft's own), or one the app takes back: to the song's slides (moved, pasted,
        // deleted), or something deleted from the list (a template, prop, layout or version).
        let apps = |step: Option<&EditStep>| matches!(step, Some(EditStep::Song | EditStep::Deleted));
        if undo && !keys_taken {
            if apps(state.edit_steps.last()) {
                let step = state.edit_steps.pop().unwrap_or(EditStep::Song);
                state.edit_redo.push(step);
                state.song_undo = Some(false);
            } else if draft.undo() {
                if state.edit_steps.last() == Some(&EditStep::Slide) {
                    state.edit_steps.pop();
                }
                state.edit_redo.push(EditStep::Slide);
            }
        } else if redo && !keys_taken {
            if apps(state.edit_redo.last()) {
                let step = state.edit_redo.pop().unwrap_or(EditStep::Song);
                state.edit_steps.push(step);
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

    // Delete with nothing picked on the canvas is the Delete key of the list on the left, for
    // the one open there (or the slides picked out): a template, a prop, a stage layout, slides.
    let renaming = state.renaming_template.is_some()
        || state.prop_rename.is_some()
        || state.stage_rename.is_some()
        || state.renaming_group.is_some();
    let list_delete = delete
        && !delete_theme
        && !had_selection
        && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none())
        && !renaming
        && !state.alert_open();
    // Left: the slide navigator, with the slide's objects below it.
    let mut chosen = None;
    // With no object selected and nothing being typed, ↑ and ↓ open the slide before or after.
    if !theme_mode
        && !prop_mode
        && !stage_mode
        && !keys_taken
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
        // The list brings the slide opened into sight by itself (see `list_picture`).
        if let Some(slide) = to {
            chosen = Some(SlideRef { slide, ..at });
            state.picked_slides.clear();
        }
    }
    let mut chosen_template = None;
    let mut prop_ask = None;
    let mut stage_request = None;
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
        // Slides, props and stage layouts: a heading and the keys that act on the one open here
        // (for slides, the songs above them); their pictures fill the room between these and
        // the Objects shelf, further down.
        if theme_mode {
            theme_column(ui, state, list_delete);
        } else if prop_mode {
            prop_ask = prop_list_head(ui, state, list_delete);
        } else if stage_mode {
            stage_request = crate::stage_editor::list_head(ui, state, list_delete);
        } else if presentation.is_some()
            && let Some(open) = slide_list_head(ui, show, state, at, list_delete)
        {
            chosen = Some(open);
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
                        // A screen picture: a screen.
                        let mark = match object.kind {
                            sp_model::ObjectKind::Text { .. } => "T",
                            sp_model::ObjectKind::Image { .. } => "▣",
                            sp_model::ObjectKind::Shape { .. } => "◆",
                            sp_model::ObjectKind::Web { .. } => "🌐",
                            sp_model::ObjectKind::Screen { .. } => "",
                        };
                        if mark.is_empty() {
                            let spot = Rect::from_center_size(row.left_center() + vec2(17.0, 0.0), vec2(13.0, 13.0));
                            icons::paint(ui.painter(), Icon::Stage, spot, ink);
                        }
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
                            let look = crate::studio::Lettering::plain(FontId::proportional(13.0), ink);
                            let field = crate::studio::rename_field_at(
                                ui,
                                name,
                                &look,
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
                            let shown =
                                if theme_mode || stage_mode { object.name.clone() } else { listed_name(object) };
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
                        let empty = if stage_mode { "   Nothing on this layout yet" } else { "   This slide is empty" };
                        ui.label(egui::RichText::new(empty).size(12.0).color(DIM));
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

        if state.theme_edit.is_some() {
            chosen_template = template_pictures(ui, state, thumb, canvas);
        } else if prop_mode {
            prop_ask = prop_ask.or(prop_list(ui, state, thumb));
        } else if stage_mode {
            let pictures = crate::live_tabs::StagePictures::of(show, state);
            stage_request = stage_request.take().or(crate::stage_editor::list(ui, state, &pictures, thumb));
        } else if let Some(open) = slide_list(ui, show, state, thumb, at) {
            chosen = Some(open);
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
        // On a stage layout, two lines at the foot: the keyboard edits here, and how.
        if stage_mode {
            let margin = egui::Margin { left: 20, right: 20, top: 10, bottom: 12 };
            let hint = Frame::new().fill(EDIT_SURROUND).inner_margin(margin);
            egui::Panel::bottom("stage hint").resizable(false).frame(hint).show(ui, crate::stage_editor::hint);
        }
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
        // A picture or video dragged over the editor is put on the slide when dropped.
        if state.drop_hover && state.file_drag.has(crate::file_drop::Kind::Media) {
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
                // A stage item shows what it shows, not words typed in it.
                let shows_item =
                    stage_mode && draft.selected_object().is_some_and(|o| crate::stage::item_of(o).is_some());
                if response.double_clicked()
                    && !for_theme
                    && !shows_item
                    && draft.selected.is_some()
                    && draft.also.is_empty()
                {
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
                    // Named as the Arrange menu names them.
                    ui.menu_button("Align", |ui| {
                        for (label, edge) in [
                            ("Align Left Edges", 0),
                            ("Align Centres", 1),
                            ("Align Right Edges", 2),
                            ("Align Tops", 3),
                            ("Align Middles", 4),
                            ("Align Bottoms", 5),
                        ] {
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

        // On a stage layout: its name, and one Add menu (with the items it can show live and
        // the screens' pictures), Undo and Redo.
        if stage_mode {
            stage_tools(ui, state, inner, at.slide);
        } else {
        // The object tools, the same for a song's slide and a theme's layout: a text box, the
        // shapes, and pictures and videos.
        let offset = -104.0;
        // In the Theme Editor they stand at the right, leaving the left for what is on the canvas.
        let anchor = if theme_mode { inner.max.x - 256.0 } else { inner.center().x };
        if theme_mode {
            theme_canvas_title(ui, state, inner, anchor + offset - 48.0 - 14.0);
        }
        // A picture from a file.
        let picture_bar =
            Rect::from_center_size(pos2(anchor + 104.0, inner.min.y + 22.0), vec2(96.0, 28.0));
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
        let web_bar = Rect::from_center_size(pos2(anchor + 208.0, inner.min.y + 22.0), vec2(96.0, 28.0));
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
        let shapes_bar = Rect::from_center_size(pos2(anchor, inner.min.y + 22.0), vec2(96.0, 28.0));
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
        let bar = Rect::from_center_size(pos2(anchor + offset, inner.min.y + 22.0), vec2(96.0, 28.0));
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
                if let Some(template) = edit.open_template_mut() {
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
            // A stage layout: the layout takes the slide's objects.
            None if stage_mode => {
                if let Some(layout) = state.stage_layouts.get_mut(draft.at.slide) {
                    *layout = crate::stage::with_slide(layout, &draft.slide);
                    state.stage_layouts_changed = true;
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
            && let Some(draft) = edit.show(edit.version, template)
        {
            state.editing = Some(draft);
        }
    } else if leave {
        state.editing = None;
        state.stage_editor_open = false;
    } else if let Some(here) = chosen {
        state.editing = EditDraft::of(show, here);
    } else if let Some(ask) = prop_ask {
        apply_prop_ask(state, ask);
    } else if let Some(request) = stage_request {
        crate::stage_editor::apply(state, request);
    }
}

/// The name of what is on the theme canvas, at its top left: the theme, the version when the
/// template is a version's, and the template ("Built-in · Offering · Two languages"); then,
/// where there is room before `right`, what it is in small capitals ("Template · Offering
/// version").
fn theme_canvas_title(ui: &egui::Ui, state: &HomeState, inner: Rect, right: f32) {
    let Some(edit) = state.theme_edit.as_ref() else { return };
    let Some(template) = edit.open_template() else { return };
    let version = edit.version.and_then(|id| edit.theme.versions.iter().find(|v| v.id == id));
    let (name, what) = match version {
        Some(version) => (
            format!("{} \u{b7} {} \u{b7} {}", edit.theme.name, version.name, template.name),
            format!("Template \u{b7} {} version", version.name),
        ),
        None => (format!("{} \u{b7} {}", edit.theme.name, template.name), "Template".to_owned()),
    };
    let y = inner.min.y + 22.0;
    let room = Rect::from_min_max(inner.min, pos2(right, inner.min.y + 44.0));
    let font = crate::studio::semibold(16.0);
    let title = ui.painter().with_clip_rect(room).text(
        pos2(inner.min.x, y),
        Align2::LEFT_CENTER,
        name,
        font,
        crate::studio::TEXT,
    );
    let look = crate::studio::Lettering::cap(crate::studio::TEXT_FAINT);
    let wide =
        crate::studio::spaced(ui, &what.to_uppercase(), crate::studio::fixed(11.5), look.ink, look.spacing).size().x;
    if title.max.x + 8.0 + wide < right {
        let at = pos2(title.max.x + 8.0, y + 1.0);
        crate::studio::paint_spaced(
            ui,
            at,
            Align2::LEFT_CENTER,
            &what.to_uppercase(),
            crate::studio::fixed(11.5),
            look.ink,
            look.spacing,
        );
    }
}

/// The tools over a stage layout's canvas: the layout's name at the left, and at the right Add
/// (a menu of everything a layout can hold), Undo and Redo.
fn stage_tools(ui: &mut egui::Ui, state: &mut HomeState, inner: Rect, layout: usize) {
    let y = inner.min.y + 22.0;
    let redo_bar = Rect::from_min_size(pos2(inner.max.x - 72.0, y - 15.0), vec2(72.0, 30.0));
    let undo_bar = redo_bar.translate(vec2(-80.0, 0.0));
    let add_bar = Rect::from_min_size(pos2(undo_bar.min.x - 88.0, y - 15.0), vec2(80.0, 30.0));
    let name = state.stage_layouts.get(layout).map(|layout| layout.name.clone()).unwrap_or_default();
    let room = Rect::from_min_max(pos2(inner.min.x, inner.min.y), pos2(add_bar.min.x - 14.0, inner.min.y + 44.0));
    let title = ui.painter().with_clip_rect(room).text(
        pos2(inner.min.x, y),
        Align2::LEFT_CENTER,
        name,
        crate::studio::semibold(16.0),
        crate::studio::TEXT,
    );
    if title.max.x + 110.0 < room.max.x {
        let at = pos2(title.max.x + 8.0, y + 1.0);
        crate::studio::cap(ui, at, Align2::LEFT_CENTER, "Stage layout", crate::studio::TEXT_FAINT);
    }
    let screens = crate::stage_editor::audience_screens(state);
    let add = ui.interact(add_bar, ui.scope_id().with("stage add"), Sense::click());
    crate::studio::paint_button(ui.painter(), add_bar, add.hovered());
    let font = crate::studio::semibold(12.5);
    ui.painter().text(add_bar.center(), Align2::CENTER_CENTER, "Add ▾", font.clone(), crate::studio::TEXT);
    let add = add.on_hover_text("Add words, a shape, a picture, something the stage shows live, or a screen's picture");
    let mut wants_picture = false;
    egui::Popup::menu(&add).show(|ui| {
        if let Some(draft) = state.editing.as_mut() {
            wants_picture = crate::stage_editor::add_menu(ui, draft, &screens);
        }
    });
    if wants_picture {
        state.picture_request = true;
    }
    let (can_undo, can_redo) =
        state.editing.as_ref().map_or((false, false), |draft| (draft.can_undo(), draft.can_redo()));
    let (undo_hint, redo_hint) =
        (crate::keys::hint("Undo the last change", "Z", false), crate::keys::hint("Redo it", "Z", true));
    for (bar, words, enabled, hint, back) in [
        (undo_bar, "Undo", can_undo, undo_hint.as_str(), true),
        (redo_bar, "Redo", can_redo, redo_hint.as_str(), false),
    ] {
        let id = ui.scope_id().with(("stage history", words));
        let look = crate::studio::Flat::Plain;
        let pressed = crate::studio::flat_button(ui, bar, id, words, font.clone(), look, enabled);
        if pressed.on_hover_text(hint).clicked()
            && let Some(draft) = state.editing.as_mut()
        {
            if back {
                draft.undo();
            } else {
                draft.redo();
            }
        }
    }
}

/// The most room the songs over the Edit view's slides take: four and a half rows, so half a
/// row shows there are more to scroll to, and the slides keep their room.
const EDIT_SONGS_TALL: f32 = 4.5 * crate::studio::PLAIN_ROW;

/// The songs the Edit view offers over the slides: those of the playlist the Show view's left
/// column lists, in its order and each once, or else the songs of the library it lists, as it
/// lists them; and the song open, at the end, when they don't include it.
fn songs_to_edit(show: &Show, state: &mut HomeState, open: usize) -> Vec<usize> {
    let mut songs: Vec<usize> = if state.playlist_open {
        let mut seen = std::collections::HashSet::new();
        slides::playlist_songs(show, state).iter().flatten().copied().filter(|song| seen.insert(*song)).collect()
    } else {
        crate::studio::listed_songs(show, state).shown.clone()
    };
    if !songs.contains(&open) {
        songs.push(open);
    }
    songs
}

/// The slides of the song open in the Edit view that its keys act on: those picked out, in
/// order, or else the one open.
fn slides_to_act_on(state: &HomeState, at: SlideRef) -> Vec<usize> {
    let mut picked: Vec<usize> =
        state.picked_slides.iter().filter(|p| p.presentation == at.presentation).map(|p| p.slide).collect();
    picked.sort_unstable();
    picked.dedup();
    if picked.is_empty() {
        picked.push(at.slide);
    }
    picked
}

/// The top of the slides' part of the Edit view's left column: the songs to choose from under
/// their heading, then the slides' heading and the keys that act on the slides picked out (or
/// the one open): Add (a slide after the one open), Duplicate, Delete (at once), ↑ and ↓.
/// `delete` is the Delete key pressed with nothing picked on the canvas, which is the Delete
/// key here. Returns the song to open, when another one is clicked.
fn slide_list_head(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    at: SlideRef,
    delete: bool,
) -> Option<SlideRef> {
    use crate::studio::PLAIN_ROW;
    let songs = songs_to_edit(show, state, at.presentation);
    let mut open = None;
    Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
        crate::studio::list_heading(ui, "Song");
        // A long list scrolls; it is scrolled to the song open when that changes.
        let mut area =
            egui::ScrollArea::vertical().id_salt("edit songs").max_height(EDIT_SONGS_TALL).auto_shrink([false, true]);
        let scrolled = egui::Id::unique("edit songs scrolled to");
        if ui.data(|data| data.get_temp::<usize>(scrolled)) != Some(at.presentation) {
            ui.data_mut(|data| data.insert_temp(scrolled, at.presentation));
            if let Some(place) = songs.iter().position(|song| *song == at.presentation) {
                area = area.vertical_scroll_offset((place as f32 - 1.0).max(0.0) * PLAIN_ROW);
            }
        }
        let mut empty = None;
        area.show_rows(ui, PLAIN_ROW, songs.len(), |ui, shown| {
            for song in &songs[shown] {
                let found = show.presentations().get(*song);
                let title = found.map_or("", |song| song.title.as_str());
                let is_open = *song == at.presentation;
                let look = crate::studio::PlainRow { icon: Icon::Document, name: Some(title), tag: None, on: is_open };
                if crate::studio::plain_row(ui, &look).0.clicked() && !is_open {
                    // A song with no slides has nothing to open: the Edit view stays as it is.
                    if found.is_some_and(|song| song.slide_count() > 0) {
                        open = Some(SlideRef { presentation: *song, slide: 0 });
                    } else {
                        empty = Some(title.to_owned());
                    }
                }
            }
        });
        if let Some(title) = empty {
            state.say(format!("\u{201c}{title}\u{201d} has no slides yet."));
        }
        ui.add_space(4.0);
        crate::studio::list_heading(ui, "Slides");
        let count = show.presentations().get(at.presentation).map_or(0, |song| song.slide_count());
        let movable = !state.app_made(at.presentation);
        let these = slides_to_act_on(state, at);
        let (first, last) = (these[0], these[these.len() - 1]);
        let several = these.len() > 1;
        let can = [movable, movable, movable && these.len() < count, movable && first > 0, movable && last + 1 < count];
        let hints = [
            "A new slide after this one, laid out like it",
            if several { "Copies of the slides picked out, after them" } else { "A copy of this slide, after it" },
            if several { "Delete the slides picked out" } else { "Delete this slide" },
            if several { "Move the slides picked out up one place" } else { "Move this slide up one place" },
            if several { "Move the slides picked out down one place" } else { "Move this slide down one place" },
        ];
        if let Some(key) = crate::studio::list_keys(ui, "slide keys", can, hints).or(list_delete_key(delete, can)) {
            act_on_slides(show, state, at, &these, key);
        }
    });
    if open.is_some() {
        state.picked_slides.clear();
    }
    open
}

/// Carries out one of the keys over the Edit view's slides, or the same choice from a slide's
/// menu, on `these`, slides of the song open, in order: Add puts a new slide laid out like the
/// one open just after it; Copy puts copies just after the last of them; Delete deletes them at
/// once (Undo puts them back); ↑ and ↓ move them one place. The app makes the change after the
/// frame; the slide open follows its slide, and Add and Copy open the first slide they made.
fn act_on_slides(show: &Show, state: &mut HomeState, at: SlideRef, these: &[usize], key: crate::studio::ListKey) {
    use crate::studio::ListKey;
    let Some(song) = show.presentations().get(at.presentation) else { return };
    let (Some(&first), Some(&last)) = (these.first(), these.last()) else { return };
    // The slide open as the editor has it now; the others as the song has them.
    let open = state.editing.as_ref().filter(|draft| draft.at == at).map(|draft| draft.slide.clone());
    let slide = |index: usize| {
        open.clone().filter(|_| index == at.slide).or_else(|| song.slide(index).map(|(_, slide)| slide.clone()))
    };
    let request = match key {
        ListKey::Add => slide(at.slide).map(|open| (at, SlideRequest::AddAfter(vec![open.empty_copy()]))),
        ListKey::Copy => {
            let copies = these.iter().filter_map(|index| slide(*index)).collect();
            Some((SlideRef { slide: last, ..at }, SlideRequest::AddAfter(copies)))
        }
        ListKey::Delete => Some((at, SlideRequest::DeleteMany(these.to_vec()))),
        ListKey::Up if first > 0 => {
            Some((at, SlideRequest::Move { slides: these.to_vec(), to: first - 1, after: false }))
        }
        ListKey::Down if last + 1 < song.slide_count() => {
            Some((at, SlideRequest::Move { slides: these.to_vec(), to: last + 1, after: true }))
        }
        ListKey::Up | ListKey::Down => None,
    };
    if request.is_some() {
        state.slide_request = request;
    }
}

/// Where slides dragged in the Edit view's list go, as [`SlideRequest::Move`] takes it: the
/// slide they go next to, and whether after it. The ink line is between two pictures (see
/// `drop_slot`); with the pointer nearer the one above, they go after it, into its group, and
/// otherwise before the one below, into its group. The slides being moved can't be the one
/// they go next to, so the nearest other is taken. `None` when every slide is being moved.
fn slide_drop(rows: &[Rect], moving: &[usize], y: f32) -> Option<(usize, bool)> {
    let slot = crate::studio::drop_slot(rows, y);
    let count = rows.len();
    let after_above = match slot.checked_sub(1) {
        None => false,
        Some(_) if slot >= count => true,
        Some(above) => y < (rows[above].max.y + rows[slot].min.y) / 2.0,
    };
    let still = |at: &usize| !moving.contains(at);
    let above = (0..slot).rev().find(still).map(|to| (to, true));
    let below = (slot..count).find(still).map(|to| (to, false));
    if after_above { above.or(below) } else { below.or(above) }
}

/// What a slide's menu in the Edit view's list was asked for.
enum SlideChoice {
    RenameGroup,
    Key(crate::studio::ListKey),
}

/// The song open's slides in the Edit view's left column, under their keys: each its picture,
/// with its number, its group's dot and its group's name (and label) in the strip under it. The
/// slide open is ringed, the one on screen has an orange strip, those picked out a faint ring,
/// and one switched off is dimmed. A click opens a slide; Command and Shift pick out several, as
/// in the Finder; a double click retypes its group's name in place; dragging moves it (with the
/// others picked out with it) to where the ink line is; a right-click offers Rename Group,
/// Duplicate, Delete…, Move Up and Move Down. Only the pictures in sight are drawn. Returns the
/// slide to open, when another one is clicked.
fn slide_list(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut crate::studio::Thumb,
    at: SlideRef,
) -> Option<SlideRef> {
    use crate::studio::{LIST_PICTURE_CORNER, ListKey, ListPicture, PANEL, TALLY, TEXT, TEXT_FAINT};
    let song = show.presentations().get(at.presentation)?;
    let movable = !state.app_made(at.presentation);
    let count = song.slide_count();
    let live = show.live().filter(|live| live.presentation == at.presentation).map(|live| live.slide);
    let mut picked: Vec<usize> =
        state.picked_slides.iter().filter(|p| p.presentation == at.presentation).map(|p| p.slide).collect();
    picked.sort_unstable();
    // A group being renamed has its field on the slide open, when that is in the group, or else
    // on the group's first slide.
    let renaming_on = state.renaming_group.as_ref().and_then(|(group, _)| {
        let first: usize = song.groups.iter().take(*group).map(|group| group.slides.len()).sum();
        let slides = first..first + song.groups.get(*group)?.slides.len();
        Some(if slides.contains(&at.slide) { at.slide } else { first })
    });
    let list = egui::Id::unique(("edit slide pictures", song.id));
    let mut open = None;
    egui::ScrollArea::vertical().id_salt(("edit slide list", song.id)).auto_shrink([false, false]).show(ui, |ui| {
        // 10 in from the column's sides, as the design's slides view has it, and 4 further
        // under the keys than the props' pictures are.
        let side = egui::Margin { left: 10, right: 10, top: (crate::studio::LIST_TOP + 4.0) as i8, bottom: 12 };
        Frame::new().inner_margin(side).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, crate::studio::LIST_BETWEEN);
            let mut rows: Vec<Rect> = Vec::with_capacity(count);
            let (mut dragging, mut dropped) = (None, None);
            let slides = song
                .groups
                .iter()
                .enumerate()
                .flat_map(|(at, group)| group.slides.iter().map(move |slide| (at, group, slide)));
            for (index, (group_index, group, slide)) in slides.enumerate() {
                let here = SlideRef { presentation: at.presentation, slide: index };
                let name = slides::strip_name(group, slide);
                let look = ListPicture {
                    number: index + 1,
                    name: &name,
                    dot: Some(slides::dot_colour(group.color)),
                    open: index == at.slide,
                    live: live == Some(index),
                    renaming: renaming_on == Some(index),
                };
                let (response, places) = crate::studio::list_picture(ui, list, index, &look);
                rows.push(response.rect);
                // Only the pictures in sight are drawn, or asked for.
                if ui.is_rect_visible(places.picture) {
                    ui.painter().rect_filled(places.picture, LIST_PICTURE_CORNER, Color32::BLACK);
                    let of = if movable { ThumbOf::Slide(slide) } else { ThumbOf::BibleSlide(slide) };
                    match thumb(of) {
                        Some((texture, _)) => egui::Image::from_texture((texture, places.picture.size()))
                            .corner_radius(LIST_PICTURE_CORNER)
                            .paint_at(ui, places.picture),
                        None => ui.ctx().request_repaint(),
                    }
                    // A slide switched off stays in the song but is stepped over: it is dimmed.
                    if !slide.enabled {
                        ui.painter().rect_filled(places.picture, LIST_PICTURE_CORNER, PANEL.gamma_multiply(0.65));
                    }
                }
                if picked.contains(&index) && index != at.slide {
                    ui.painter().rect_stroke(response.rect, 9.0, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Inside);
                }
                // The group's name retyped in the strip, set as it is shown.
                if look.renaming
                    && let Some((_, typed)) = state.renaming_group.as_mut()
                {
                    let ink = if look.live { TALLY } else { TEXT };
                    let look = crate::studio::list_name_look(ink);
                    let field = crate::studio::rename_field_at(ui, typed, &look, places.name, places.name_right);
                    if std::mem::take(&mut state.rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        state.renaming_group = None;
                    } else if enter || field.lost_focus() {
                        let typed = typed.trim().to_owned();
                        if typed != group.name {
                            state.pending_group_name = Some((at.presentation, group_index, typed));
                        }
                        state.renaming_group = None;
                    }
                }
                if !movable {
                    if response.clicked() && index != at.slide {
                        open = Some(here);
                    }
                    continue;
                }
                if response.dragged() {
                    dragging = Some(index);
                }
                if crate::studio::drag_let_go(&response) {
                    dropped = Some(index);
                }
                // The menu acts on the slides picked out when this is one of them.
                let these = if picked.contains(&index) { picked.clone() } else { vec![index] };
                let (first, last) = (these[0], these[these.len() - 1]);
                let mut chose = None;
                response.context_menu(|ui| {
                    let mut item = |ui: &mut egui::Ui, words: &str, can: bool, choice: SlideChoice| {
                        if ui.add_enabled(can, egui::Button::new(words)).clicked() {
                            chose = Some(choice);
                            ui.close();
                        }
                    };
                    item(ui, "Rename Group", true, SlideChoice::RenameGroup);
                    ui.separator();
                    item(ui, "Duplicate", true, SlideChoice::Key(ListKey::Copy));
                    item(ui, "Delete", these.len() < count, SlideChoice::Key(ListKey::Delete));
                    ui.separator();
                    item(ui, "Move Up", first > 0, SlideChoice::Key(ListKey::Up));
                    item(ui, "Move Down", last + 1 < count, SlideChoice::Key(ListKey::Down));
                });
                match chose {
                    // Its field is on this slide, which opens.
                    Some(SlideChoice::RenameGroup) => {
                        state.renaming_group = Some((group_index, group.name.clone()));
                        state.rename_started = true;
                        if index != at.slide {
                            open = Some(here);
                        }
                    }
                    Some(SlideChoice::Key(key)) => act_on_slides(show, state, at, &these, key),
                    None => {}
                }
                if response.double_clicked() {
                    state.renaming_group = Some((group_index, group.name.clone()));
                    state.rename_started = true;
                } else if response.clicked() {
                    let mods = ui.input(|input| input.modifiers);
                    if mods.command || mods.shift {
                        // Picked out, to move, copy or delete several; the slide open stays open,
                        // and is picked out with them.
                        let order: Vec<SlideRef> =
                            (0..count).map(|slide| SlideRef { presentation: at.presentation, slide }).collect();
                        state.picked_slides.retain(|p| p.presentation == at.presentation);
                        if state.picked_slides.is_empty() {
                            state.picked_slides.push(at);
                        }
                        pick_slide(&mut state.picked_slides, &order, here, mods, Some(at));
                    } else {
                        state.picked_slides.clear();
                        if index != at.slide {
                            open = Some(here);
                        }
                    }
                }
            }
            // A slide being dragged goes faint, with the others picked out with it; what it is
            // follows the pointer, and an ink line shows where it goes. Let go over the list, it
            // goes there; let go elsewhere (over the slide being edited), or stopped with Escape,
            // it stays where it was.
            let moving: Vec<usize> = match dragging.or(dropped) {
                Some(held) if picked.len() > 1 && picked.contains(&held) => picked.clone(),
                Some(held) => vec![held],
                None => Vec::new(),
            };
            let words = match moving.as_slice() {
                _ if dragging.is_none() => String::new(),
                [one] => song
                    .slide(*one)
                    .map(|(group, slide)| format!("{:02} {}", one + 1, slides::strip_name(group, slide)))
                    .unwrap_or_default(),
                many => format!("{} slides", many.len()),
            };
            if let Some(y) = list_drag(ui, &rows, (dragging, dropped), &moving, &words)
                && let Some((to, after)) = slide_drop(&rows, &moving, y)
            {
                state.slide_request = Some((
                    SlideRef { presentation: at.presentation, slide: moving[0] },
                    SlideRequest::Move { slides: moving.clone(), to, after },
                ));
            }
        });
    });
    open
}

/// What the list of props in the Edit view asked for: another prop opened, or something asked
/// of the prop at a place (for Add, of none).
#[derive(Clone, Copy, Debug, PartialEq)]
enum PropListAsk {
    Open(usize),
    Of(usize, crate::live_tabs::PropAsk),
    /// The prop at a place dragged to another.
    Move(usize, usize),
}

/// The top of the props' part of the Edit view's left column: the heading, and the keys that
/// act on the prop open (Add, Duplicate, Delete, which asks first, and ↑ and ↓ to move it).
/// `delete` is the Delete key pressed with nothing picked on the canvas: the Delete key here.
/// With no props, only Add can be pressed.
fn prop_list_head(ui: &mut egui::Ui, state: &mut HomeState, delete: bool) -> Option<PropListAsk> {
    use crate::live_tabs::PropAsk;
    use crate::studio::ListKey;
    let open = state.editing.as_ref().map(|draft| draft.at.slide).filter(|at| *at < state.props.len());
    let count = state.props.len();
    let mut asked = None;
    Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
        crate::studio::list_heading(ui, "Props");
        let can = [
            true,
            open.is_some(),
            open.is_some(),
            open.is_some_and(|at| at > 0),
            open.is_some_and(|at| at + 1 < count),
        ];
        let hints = [
            "A new prop",
            "A copy of this prop",
            "Delete this prop; it comes off the screens if it is showing",
            "Move this prop up the list",
            "Move this prop down the list",
        ];
        let pressed = crate::studio::list_keys(ui, "prop keys", can, hints).or(list_delete_key(delete, can));
        asked = match (pressed, open) {
            (Some(ListKey::Add), _) => Some(PropListAsk::Of(open.unwrap_or(0), PropAsk::New)),
            (Some(ListKey::Copy), Some(at)) => Some(PropListAsk::Of(at, PropAsk::Duplicate)),
            (Some(ListKey::Delete), Some(at)) => Some(PropListAsk::Of(at, PropAsk::Delete)),
            (Some(ListKey::Up), Some(at)) => Some(PropListAsk::Of(at, PropAsk::Move(true))),
            (Some(ListKey::Down), Some(at)) => Some(PropListAsk::Of(at, PropAsk::Move(false))),
            _ => None,
        };
    });
    asked
}

/// The props, under their heading and keys: each its picture, with its number and name under
/// it. A click opens one, a double click retypes its name in place, a drag moves it to where
/// the ink line is, and a right-click offers what the Props tab's ⋯ does (but Edit in the
/// Editor). Returns what was asked for.
fn prop_list(ui: &mut egui::Ui, state: &mut HomeState, thumb: &mut crate::studio::Thumb) -> Option<PropListAsk> {
    use crate::studio::{ListPicture, TEXT};
    let open = state.editing.as_ref().map(|draft| draft.at.slide);
    let mut asked = None;
    let list = egui::Id::unique("prop pictures");
    egui::ScrollArea::vertical().id_salt("edit props").auto_shrink([false, false]).show(ui, |ui| {
        let side = egui::Margin { left: 20, right: 20, top: crate::studio::LIST_TOP as i8, bottom: 12 };
        Frame::new().inner_margin(side).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, crate::studio::LIST_BETWEEN);
            let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
            let mut rename_started = std::mem::take(&mut state.rename_started);
            let (mut rows, mut dragging, mut dropped) = (Vec::with_capacity(state.props.len()), None, None);
            for (index, prop) in state.props.iter().enumerate() {
                let renaming = state.prop_rename.as_ref().is_some_and(|(id, _)| *id == prop.id);
                let look = ListPicture {
                    number: index + 1,
                    name: &prop.name,
                    dot: None,
                    open: open == Some(index),
                    live: state.props_live.iter().any(|(live, _)| *live == prop.id),
                    renaming,
                };
                let (response, places) = crate::studio::list_picture(ui, list, index, &look);
                rows.push(response.rect);
                if response.dragged() {
                    dragging = Some(index);
                }
                if crate::studio::drag_let_go(&response) {
                    dropped = Some(index);
                }
                // Only the pictures in sight are drawn, or asked for.
                if ui.is_rect_visible(places.picture) {
                    crate::live_tabs::prop_picture(ui, thumb, places.picture, crate::studio::LIST_PICTURE_CORNER, prop);
                }
                if renaming && let Some((_, typed)) = state.prop_rename.as_mut() {
                    let look = crate::studio::list_name_look(if look.live { crate::studio::TALLY } else { TEXT });
                    let field = crate::studio::rename_field_at(ui, typed, &look, places.name, places.name_right);
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((prop.id, typed.clone())));
                    }
                }
                let auto_clear = prop.auto_clear;
                let mut chose = None;
                response.context_menu(|ui| chose = crate::live_tabs::prop_menu(ui, auto_clear, true).or(chose));
                if response.double_clicked() {
                    chose = Some(crate::live_tabs::PropAsk::Rename);
                }
                if let Some(ask) = chose {
                    asked = Some(PropListAsk::Of(index, ask));
                } else if response.clicked() && open != Some(index) {
                    asked = Some(PropListAsk::Open(index));
                }
            }
            state.rename_started = rename_started;
            let moving: Vec<usize> = dragging.or(dropped).into_iter().collect();
            let words = dragging.and_then(|at| state.props.get(at)).map(|prop| prop.name.clone()).unwrap_or_default();
            if let (Some(from), Some(y)) = (dropped, list_drag(ui, &rows, (dragging, dropped), &moving, &words))
                && let Some(to) = dragged_to(&rows, from, y)
            {
                asked = Some(PropListAsk::Move(from, to));
            }
            match renamed {
                Some(Some((id, name))) => {
                    if let Some(prop) = state.props.iter_mut().find(|prop| prop.id == id)
                        && !name.trim().is_empty()
                        && prop.name != name.trim()
                    {
                        prop.name = name.trim().to_owned();
                        state.props_changed = true;
                    }
                    state.prop_rename = None;
                }
                Some(None) => state.prop_rename = None,
                None => {}
            }
        });
    });
    asked
}

/// Carries out what the list of props in the Edit view asked for, once the frame's change to
/// the prop open has gone back into it, and keeps the editor on the right prop: the one opened,
/// made or copied, or else the one that was open, wherever it went. Delete goes at once (Undo
/// puts it back); the one after it opens in its place, and with the last prop deleted, the
/// editor goes on to the slides (see `HomeState::take_prop`).
fn apply_prop_ask(state: &mut HomeState, ask: PropListAsk) {
    use crate::live_tabs::PropAsk;
    let (index, ask) = match ask {
        PropListAsk::Open(index) => {
            if let Some(draft) = crate::studio::prop_draft(state, index) {
                state.editing = Some(draft);
            }
            return;
        }
        PropListAsk::Of(index, PropAsk::Delete) => {
            state.delete_prop(index);
            return;
        }
        // Dragged to another place: the prop open stays open, in the same draft, wherever it went.
        PropListAsk::Move(from, to) => {
            if from < state.props.len() && to < state.props.len() {
                let prop = state.props.remove(from);
                state.props.insert(to, prop);
                state.props_changed = true;
                if let Some(draft) =
                    state.editing.as_mut().filter(|draft| draft.at.presentation == crate::props::PROP_SLOT)
                {
                    draft.at.slide = place_after_move(draft.at.slide, from, to);
                }
            }
            return;
        }
        PropListAsk::Of(index, ask) => (index, ask),
    };
    let open = state.editing.as_ref().filter(|draft| draft.at.presentation == crate::props::PROP_SLOT);
    let open_id = open.and_then(|draft| state.props.get(draft.at.slide)).map(|prop| prop.id);
    let made = crate::live_tabs::carry_out_prop_ask(state, index, ask);
    let kept = open_id.and_then(|id| state.props.iter().position(|prop| prop.id == id));
    let to = if matches!(ask, PropAsk::New | PropAsk::Duplicate) { made } else { kept };
    match to {
        // The prop open, wherever it went: its draft goes with it, with the changes it can undo.
        Some(to) if Some(to) == kept => {
            if let Some(draft) = state.editing.as_mut() {
                draft.at.slide = to;
            }
        }
        Some(to) => state.editing = crate::studio::prop_draft(state, to),
        None => {}
    }
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
        // As the driver names it ("DeckLink Duo (1)"), and nothing more.
        Output::DeckLink(out) => out.device.clone(),
        Output::Off => "Off".to_owned(),
    }
}

/// The outputs a screen can be given: each connected display but the main one (where the
/// operator works), a window, NDI, each DeckLink output that can play (none without Desktop
/// Video), off.
fn output_choices(
    current: &Output,
    screen_name: &str,
    displays: &[DisplayInfo],
    decklink: &sp_decklink::Found,
) -> Vec<Output> {
    let mut choices: Vec<Output> =
        displays.iter().filter(|display| !display.main).map(|display| Output::Display(display.name.clone())).collect();
    choices.push(Output::Window);
    // Choosing NDI again keeps the name already given.
    choices.push(match current {
        Output::Ndi(name) => Output::Ndi(name.clone()),
        _ => Output::Ndi(format!("SoftPresenter {screen_name}")),
    });
    choices.extend(crate::screens::decklink_choices(current, decklink));
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
    /// Something deleted from one of the Edit view's lists (a template, a prop, a stage layout,
    /// a theme's version), which the app puts back from its one history (see `removed`).
    Deleted,
}

/// When the slide being edited is left (another opened, or the editor closed), its own steps
/// go with it. The song's steps stay, unless the slide was changed after one of them: undoing
/// that step would bring the song back as it was before, losing the change, so they go too.
/// What was deleted from a list always stays. True when the song's steps went, and the app
/// should drop them.
pub fn drop_slide_steps(steps: &mut Vec<EditStep>) -> bool {
    let changed_after = steps
        .iter()
        .rposition(|step| *step == EditStep::Song)
        .is_some_and(|last| steps[last + 1..].contains(&EditStep::Slide));
    if changed_after {
        steps.retain(|step| *step == EditStep::Deleted);
    } else {
        steps.retain(|step| *step != EditStep::Slide);
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
    /// Put these slides, in order, just after this one, and open the first of them in the Edit
    /// view: its Add and Copy.
    AddAfter(Vec<Slide>),
    Group(String),
    Label(Option<String>),
}

/// Where the slide at `place` of a song is once `deleted` (places in the song, in any order)
/// have gone: the slides before it that went are taken off. For a slide that went itself, the
/// place of the first slide after it that stayed.
pub fn place_after_deleting(place: usize, deleted: &[usize]) -> usize {
    place - deleted.iter().filter(|gone| **gone < place).count()
}

/// Where the slide being edited is once its song's slides have changed (`song`, as it is now):
/// found again by its id (true), or, when it went, the slide now at `instead`, or the last
/// (false). `None` when the song has no slides left.
pub fn edited_slide_place(song: &sp_model::Presentation, id: sp_model::Uuid, instead: usize) -> Option<(usize, bool)> {
    if let Some(found) = song.slides().position(|(_, slide)| slide.id == id) {
        return Some((found, true));
    }
    song.slide_count().checked_sub(1).map(|last| (instead.min(last), false))
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

/// A result of the find box being carried to the left column (see `library_search`): what a
/// playlist keeps for it (a song, or a Bible passage or verse with the Bible view's
/// translations and choices as they were when the drag began), the song by its place among
/// the songs (none for a passage or verse, which no library takes), its name, which follows the
/// pointer, and which result it is, so that its row goes faint while it is carried.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundDrag {
    pub item: crate::service::Item,
    pub song: Option<usize>,
    pub name: String,
    pub from: Found,
}

/// One result of the find box, by what it is rather than its place in the list (a passage can
/// arrive above the songs while one of them is carried): a song and the slide found in it, or
/// a Bible passage or verse by what it looks up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Song(usize, usize),
    Bible(String),
}

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
    // Escape cancels, as Cancel does (the words typed are not kept), unless it is for a menu or
    // something else over the window; it clears nothing.
    let escape = !state.escape_taken_above(ui.ctx()) && ui.input(|input| input.key_pressed(egui::Key::Escape));
    let Some((index, text)) = state.reflow.as_mut() else { return };
    let index = *index;
    let mut done = escape.then_some(false);
    let field_id = ui.scope_id().with("reflow text");
    let heading = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 12, bottom: 8 });
    egui::Panel::top("reflow header").resizable(false).frame(heading).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Edit Words  ·  {title}"))
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
fn search_songs(show: &Show, listed: impl Fn(usize) -> bool, query: &str) -> Vec<(usize, usize, Option<String>)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut by_title = Vec::new();
    let mut by_words = Vec::new();
    for (index, song) in show.presentations().iter().enumerate().filter(|(index, _)| listed(*index)) {
        // A title found: the song once, at its first slide.
        if song.title.to_lowercase().contains(&query) {
            by_title.push((index, 0, None));
            continue;
        }
        // Words found: each slide that has them, with the line.
        for (slide, (_, one)) in song.slides().enumerate() {
            let line = one
                .texts()
                .into_iter()
                .flat_map(|(_, text)| text.lines().map(str::to_owned).collect::<Vec<_>>())
                .find(|line| line.to_lowercase().contains(&query));
            if let Some(line) = line {
                by_words.push((index, slide, Some(line)));
            }
        }
    }
    by_title.extend(by_words);
    by_title
}

/// How many songs the find box lists.
const SONGS_LISTED: usize = 6;

/// Testing aid: SOFTPRESENTER_FIND_RETURN=WORDS types the words in the find box when the window
/// first draws, and presses Return once something is listed and the Bibles have answered, as an
/// operator would after `/`; for a quiet copy, which takes no keyboard, in any run (not only a
/// capture run), so `sp` can be tried on what it opens.
fn find_return_words() -> Option<&'static str> {
    static WORDS: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| std::env::var("SOFTPRESENTER_FIND_RETURN").ok().filter(|words| !words.is_empty())).as_deref()
}

/// Testing aid: SOFTPRESENTER_FIND_DRAG=WORDS@X,Y types the words in the find box and, once
/// what they find has been listed for a moment, presses on the first result and drags it to
/// X,Y (the window's points) over a few frames, then holds it there, for a picture of a drag in
/// progress; `WORDS@X,Y,drop` lets go there too. The window's own pointer is moved, not the
/// mouse. It goes a step a frame, so it is for a capture run (`--capture-home`), whose window is
/// drawn wherever it is: macOS doesn't draw a quiet copy's window kept behind the others.
/// Returns the pointer's events for this frame, to go in with the window's own.
pub fn find_drag_events(ctx: &egui::Context, state: &mut HomeState) -> Vec<egui::Event> {
    /// What the switch asks: the words, where to, and whether to let go there.
    fn asked() -> Option<&'static (String, egui::Pos2, bool)> {
        static ASKED: std::sync::OnceLock<Option<(String, egui::Pos2, bool)>> = std::sync::OnceLock::new();
        ASKED
            .get_or_init(|| {
                let asked = std::env::var("SOFTPRESENTER_FIND_DRAG").ok()?;
                let (words, place) = asked.rsplit_once('@')?;
                let mut numbers = place.split(',');
                let x = numbers.next()?.trim().parse().ok()?;
                let y = numbers.next()?.trim().parse().ok()?;
                let drop = numbers.next().is_some_and(|word| word.trim() == "drop");
                Some((words.to_owned(), pos2(x, y), drop))
            })
            .as_ref()
    }
    /// How far it has got: frames since the words were typed.
    static STEP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let Some((words, to, drop)) = asked() else { return Vec::new() };
    let step = STEP.load(std::sync::atomic::Ordering::Relaxed);
    let (events, next) = find_drag_step(ctx, state, step, words, *to, *drop);
    STEP.store(next, std::sync::atomic::Ordering::Relaxed);
    events
}

/// One frame of `find_drag_events`, at `step`: the pointer's events for it, and the next step.
fn find_drag_step(
    ctx: &egui::Context,
    state: &mut HomeState,
    step: u32,
    words: &str,
    to: egui::Pos2,
    drop: bool,
) -> (Vec<egui::Event>, u32) {
    /// Frames the list is left to settle (the Bibles answer on a thread of their own).
    const SETTLE: u32 = 40;
    // The first result.
    let from = state.find_rows.first().map(|row| row.center());
    let events = match step {
        0 => {
            state.library_search = Some((words.to_owned(), 0));
            Vec::new()
        }
        // Until something is listed, the count waits.
        _ if step < SETTLE && from.is_none() => {
            ctx.request_repaint();
            return (Vec::new(), step);
        }
        _ if step < SETTLE => Vec::new(),
        _ => match from.map(|from| pointer_drag(step - SETTLE, from, to, drop)) {
            Some(None) => return (Vec::new(), step),
            Some(Some(events)) => events,
            None => Vec::new(),
        },
    };
    ctx.request_repaint();
    (events, step + 1)
}

/// Testing aid: the pointer's events for one frame of a drag, `step` frames after it starts: it
/// goes to `from` and presses there, goes a little way down (far enough for the press to be a
/// drag) and on to `to` over a few frames, and holds there, letting go after a while when
/// `drop`. `None` once it is over.
pub fn pointer_drag(step: u32, from: egui::Pos2, to: egui::Pos2, drop: bool) -> Option<Vec<egui::Event>> {
    /// The frames the pointer takes to get there, and how long it holds before letting go.
    const MOVES: u32 = 10;
    const HOLD: u32 = 20;
    let press = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let start = from + vec2(0.0, 12.0);
    Some(match step {
        0 => vec![egui::Event::PointerMoved(from)],
        1 => vec![press(from, true)],
        _ if step <= 1 + MOVES => {
            let along = (step - 1) as f32 / MOVES as f32;
            vec![egui::Event::PointerMoved(start + (to - start) * along)]
        }
        _ if step == 1 + MOVES + HOLD && drop => vec![press(to, false)],
        _ if step > 1 + MOVES + HOLD => return None,
        _ => Vec::new(),
    })
}

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
    // What the words say nothing of a language for is read in the session's (or the language
    // chosen on the Bible page).
    let reading = state.bible.reading();
    let (shelf, rows) = (&state.bible.shelf, &state.bible.rows);
    let bible_rows = if typed.is_empty() {
        Some(&[][..])
    } else {
        let in_use: Vec<String> =
            rows.iter().filter_map(|row| Some(shelf.bibles.get(row.bible?)?.id.clone())).collect();
        let readers = || rows.iter().filter_map(|row| shelf.reader(row.bible?)).collect();
        state.finder.rows(typed, &in_use, &reading, readers, ui.ctx())
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
    // Testing aid (see `find_return_words`): Return, once, when something is listed.
    static RETURNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let test_return = answered
        && total > 0
        && find_return_words().is_some()
        && !RETURNED.swap(true, std::sync::atomic::Ordering::Relaxed);
    let enter = enter || test_return;
    if up {
        *picked = picked.saturating_sub(1);
    }
    if down && *picked + 1 < total {
        *picked += 1;
    }
    let picked = *picked;
    // What a place in the list holds: a song by its place and the slide found (and whether it
    // was found by its title), or a Bible passage or verse.
    enum Pick<'a> {
        Song(usize, usize, bool),
        Bible(&'a crate::find::BibleRow),
    }
    let at = |place: usize| -> Option<Pick<'_>> {
        if let Some(row) = passage.get(place) {
            return Some(Pick::Bible(row));
        }
        let place = place - passage.len();
        if let Some((index, slide, line)) = songs.get(place) {
            return Some(Pick::Song(*index, *slide, line.is_none()));
        }
        verses.get(place - songs.len()).map(Pick::Bible)
    };
    let mut clicked = None;
    // A result can be dragged onto the left column (a playlist, or a library for a song): the
    // one being carried goes faint in the list, which stays open until it is dropped.
    let mut dragged = None;
    let carried = egui::DragAndDrop::payload::<FoundDrag>(ui.ctx()).map(|drag| drag.from.clone());
    let mut rects = Vec::new();
    // What is found is listed under the find box, where the words are typed (in the middle of
    // a window too narrow to have the box).
    let screen = ui.ctx().content_rect();
    let (under, wide) = state.find_rect.map_or((pos2(screen.center().x - 180.0, screen.min.y + 49.0), 360.0), |find| {
        (pos2(find.min.x, find.max.y + 6.0), find.width().max(360.0))
    });
    let mut list = None;
    if !typed.is_empty() {
        use crate::studio::{CARD, LINE, PANEL, TEXT, TEXT_FAINT};
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
                // One result: the mark of its kind (a song or the Bible), its name, what it is
                // at the right in its kind's tint, and a line of its words under.
                let mut row = |ui: &mut egui::Ui,
                               place: usize,
                               bible: bool,
                               title: &str,
                               tag: &str,
                               line: Option<&str>,
                               faint: bool| {
                    let height = if line.is_some() { 48.0 } else { 34.0 };
                    let size = vec2(ui.available_width(), height);
                    // A press that barely moves is a click, as everywhere else.
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
                    rects.push(rect);
                    if place == picked || response.hovered() {
                        ui.painter().rect_filled(rect, 9.0, CARD);
                    }
                    let first = if line.is_some() { rect.min.y + 16.0 } else { rect.center().y };
                    let (icon, tint) = crate::studio::found_look(bible);
                    let mark = Rect::from_center_size(pos2(rect.min.x + 19.0, rect.center().y), vec2(16.0, 16.0));
                    crate::icons::paint(ui.painter(), icon, mark, tint);
                    let left = mark.max.x + 12.0;
                    let tag_at = pos2(rect.max.x - 10.0, first);
                    let font = crate::studio::fixed(11.0);
                    // The Bible's tag is dim, so its title reads first; a song's is in its tint.
                    let tag_ink = if bible { DIM } else { tint };
                    let tagged = crate::studio::paint_spaced(ui, tag_at, Align2::RIGHT_CENTER, tag, font, tag_ink, 0.6);
                    let words = Rect::from_min_max(pos2(left, rect.min.y), pos2(tagged.min.x - 10.0, rect.max.y));
                    ui.painter().with_clip_rect(words).text(
                        pos2(words.min.x, first),
                        Align2::LEFT_CENTER,
                        title,
                        crate::studio::semibold(13.0),
                        TEXT,
                    );
                    if let Some(line) = line {
                        let under = Rect::from_min_max(pos2(left, rect.min.y), rect.max - vec2(10.0, 0.0));
                        ui.painter().with_clip_rect(under).text(
                            pos2(under.min.x, rect.min.y + 34.0),
                            Align2::LEFT_CENTER,
                            line,
                            FontId::proportional(12.0),
                            TEXT_FAINT,
                        );
                    }
                    if faint {
                        ui.painter().rect_filled(rect, 9.0, PANEL.gamma_multiply(0.6));
                    }
                    if response.clicked() {
                        clicked = Some(place);
                    }
                    if response.drag_started() {
                        dragged = Some(place);
                    }
                };
                let faint = |found: Found| carried.as_ref() == Some(&found);
                for (place, found) in passage.iter().enumerate() {
                    let faint = faint(Found::Bible(found.lookup.clone()));
                    row(ui, place, true, &found.title, &found.tag, Some(&found.line), faint);
                }
                // A song found by its title, or each slide found by its words: its group at the
                // right ("SONG · CHORUS"), the line found under.
                for (place, (index, slide, line)) in songs.iter().enumerate() {
                    let Some(song) = show.presentations().get(*index) else { continue };
                    let tag = match (line, song.slide(*slide)) {
                        (Some(_), Some((group, _))) => format!("SONG \u{b7} {}", group.name.to_uppercase()),
                        _ => "SONG".to_owned(),
                    };
                    let faint = faint(Found::Song(*index, *slide));
                    row(ui, passage.len() + place, false, &song.title, &tag, line.as_deref(), faint);
                }
                for (place, found) in verses.iter().enumerate() {
                    let (place, faint) =
                        (passage.len() + songs.len() + place, faint(Found::Bible(found.lookup.clone())));
                    row(ui, place, true, &found.title, &found.tag, Some(&found.line), faint);
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
    // A result dragged away is carried as what a playlist would keep for it: a song (a slide
    // found by its words carries its song, as playlists hold songs), or a passage or verse with
    // the Bible view's translations and choices, as Command-Return adds it.
    if let Some(place) = dragged {
        let carry = match at(place) {
            Some(Pick::Song(index, slide, _)) => {
                state.song_keys.get(index).zip(show.presentations().get(index)).map(|(key, song)| FoundDrag {
                    item: crate::service::Item::song(key.clone(), song.title.clone()),
                    song: Some(index),
                    name: song.title.clone(),
                    from: Found::Song(index, slide),
                })
            }
            Some(Pick::Bible(row)) => Some(FoundDrag {
                item: crate::service::Item::Passage(crate::service::SavedPassage {
                    lookup: row.kept.clone(),
                    numbering: Some(row.numbering.name().to_owned()),
                    ..state.bible.settings()
                }),
                song: None,
                name: row.title.clone(),
                from: Found::Bible(row.lookup.clone()),
            }),
            None => None,
        };
        if let Some(carry) = carry {
            egui::DragAndDrop::set_payload(ui.ctx(), carry);
        }
    }
    // What is carried says its name beside the pointer until it is dropped.
    if let Some(carried) = egui::DragAndDrop::payload::<FoundDrag>(ui.ctx()) {
        crate::studio::paint_carried(ui.ctx(), &carried.name);
    }
    // What to do, worked out while the lists are still at hand: a song by its place, a passage
    // by what to look up; and whether to add it to the playlist in front instead of opening it.
    enum Do {
        /// A song, and the slide found in it.
        Song(usize, usize, bool),
        /// What to look up (as the first Bible names it), the language it was typed or found in,
        /// and what a playlist keeps for it (named and numbered in that language) with its
        /// numbering.
        Passage(String, Option<String>, (String, sp_bible::Numbering)),
    }
    let chosen = clicked.map(|place| (place, false)).or((enter && total > 0).then_some((picked, command)));
    let chosen = chosen.and_then(|(place, add)| {
        Some((
            match at(place)? {
                Pick::Song(index, slide, by_title) => Do::Song(index, slide, by_title),
                Pick::Bible(row) => {
                    Do::Passage(row.lookup.clone(), row.language.clone(), (row.kept.clone(), row.numbering))
                }
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
        Some((Do::Song(index, ..), true)) => {
            if let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index)) {
                state.services.add(crate::service::Item::song(key.clone(), song.title.clone()));
            }
        }
        // The song, with the view brought to the slide found, lit for a moment; nothing goes on
        // the screens. The slide found by its words is picked, or for a song found by its title
        // its first slide as sung: Enter puts it on screen.
        Some((Do::Song(index, slide, by_title), false)) => {
            commands.push(Command::Select(index));
            state.editing = None;
            state.bible_open = false;
            state.reflow = None;
            state.spotlight = Some((SlideRef { presentation: index, slide }, None));
            state.spotlight_scroll = 12;
            let first = |song: &sp_model::Presentation| {
                song.order().into_iter().find(|at| song.slide(*at).is_some_and(|(_, one)| one.enabled))
            };
            let slide =
                if by_title { show.presentations().get(index).and_then(|song| first(song)) } else { Some(slide) };
            if let Some(slide) = slide {
                state.set_pick(show, SlideRef { presentation: index, slide }, false);
            }
        }
        // Kept in the playlist with the translations and choices the Bible view has now, as its
        // own Add does: what was found, a single verse alone.
        Some((Do::Passage(_, _, (kept, numbering)), true)) => {
            let saved = crate::service::SavedPassage {
                lookup: kept,
                numbering: Some(numbering.name().to_owned()),
                ..state.bible.settings()
            };
            state.services.add(crate::service::Item::Passage(saved));
        }
        // The Bible view opens on the passage, named in the language the operator found it in,
        // which the session goes on in. It is made in the place that is not on screen, so the
        // screens keep what they show. The passage is what was found (a single verse, alone),
        // and its first verse is picked: Enter puts it on screen. With a slide a verse its slides
        // are its whole chapter's, so Space goes on through the chapter after it (see
        // `bible::page_deck`).
        Some((Do::Passage(lookup, language, _), false)) => {
            state.view_request = Some(2);
            state.reflow = None;
            if language.is_some() {
                state.bible.language.clone_from(&language);
            }
            let live = show.live().map(|at| at.presentation);
            state.bible.open_in(lookup, language, live);
            state.pick_passage(show);
        }
        None => {}
    }
    state.find_rows = rects;
    if ended {
        end_search(ui.ctx(), state);
    }
}

/// Ends the find box's search: the list under it goes, and the box gives the keyboard back, so
/// the keys run the show again. A result dropped on the left column ends it too.
pub fn end_search(ctx: &egui::Context, state: &mut HomeState) {
    state.library_search = None;
    state.find_rows.clear();
    ctx.memory_mut(|memory| memory.surrender_focus(crate::studio::find_box_id()));
}

/// The key that adds each shape in the editor, in the order of `Shape::ALL`.
pub const SHAPE_KEYS: [char; 10] = ['O', 'R', 'E', 'G', 'L', 'H', 'P', 'S', 'D', 'A'];

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
            "Integrations" => crate::integrations::pane(ui, &mut state.voxlive, &mut state.canva),
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
                // Its key is the Mac menu bar's.
                let hint = if cfg!(target_os = "macos") {
                    "Configure Screens (Option-Command-1)"
                } else {
                    "Configure Screens"
                };
                if ui.button("Configure Screens…").on_hover_text(hint).clicked() {
                    state.settings_open = true;
                }
                ui.add_space(14.0);
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
                    // Every key the list of keys (`?`) has, from the same list so the two always
                    // agree; then the menus' shortcuts.
                    for (group, rows) in crate::keys::every_key() {
                        for (keys, what) in rows {
                            ui.label(egui::RichText::new(group).color(TEXT_DIM));
                            ui.label(what);
                            ui.label(egui::RichText::new(keys.join("  ")).monospace());
                            ui.end_row();
                        }
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

pub(crate) fn switch(ui: &mut egui::Ui, on: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
    let how_far = ui.ctx().animate_bool(response.id, on);
    let track = Color32::from_gray(70).lerp_to_gamma(ON, how_far);
    ui.painter().rect_filled(rect, 10.0, track);
    let knob = pos2(egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, how_far), rect.center().y);
    ui.painter().circle_filled(knob, 8.0, Color32::WHITE);
    response.on_hover_text(hint).clicked()
}

/// Where the Screens window keeps the screen its − asked to remove, while the question is open.
fn removing_screen_id() -> egui::Id {
    egui::Id::unique("removing screen")
}

/// The question the Screens window's − asks, as Undo can't bring a screen back, and the removing
/// once it is answered. Returns true when the screen went.
fn confirm_remove_screen(ui: &mut egui::Ui, state: &mut HomeState) -> bool {
    let Some(id) = ui.data(|data| data.get_temp::<sp_model::Uuid>(removing_screen_id())) else { return false };
    let Some(at) = state.screens.iter().position(|screen| screen.id == id) else {
        ui.data_mut(|data| data.remove::<sp_model::Uuid>(removing_screen_id()));
        return false;
    };
    let question = format!("Remove the screen \u{201c}{}\u{201d}?", state.screens[at].name);
    let note = cant_undo("Nothing more is sent to it, and its settings go.");
    let Some(remove) = crate::studio::alert(ui.ctx(), "remove screen", Icon::Trash, &question, &note, "Remove") else {
        return false;
    };
    ui.data_mut(|data| data.remove::<sp_model::Uuid>(removing_screen_id()));
    if !remove {
        return false;
    }
    state.screens.remove(at);
    state.screen_selected = state.screen_selected.min(state.screens.len().saturating_sub(1));
    state.ndi_draft = None;
    true
}

/// The Screens window's contents: the audience screens on the left, and where the chosen one's
/// picture goes on the right. Every screen shows the same picture. Changes take effect, and are
/// saved, as they are made. `output_on` is whether the audience output is on; switching it is
/// returned as a command.
pub fn screens_ui(ui: &mut egui::Ui, state: &mut HomeState, output_on: bool) -> Vec<Command> {
    use crate::studio::{ACCENT, CARD, HOVER, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT};
    let mut commands = Vec::new();
    let mut changed = confirm_remove_screen(ui, state);
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
                    // Undo can't bring a screen back, so it asks first (see `confirm_remove_screen`).
                    if remove.clicked()
                        && !only
                        && let Some(screen) = state.screens.get(state.screen_selected)
                    {
                        ui.data_mut(|data| data.insert_temp(removing_screen_id(), screen.id));
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
        crate::screens::paint_background(ui.painter(), frame_rect, &screen.look);
        ui.painter().rect_stroke(frame_rect, 8.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let target = match &screen.output {
            Output::Automatic => "Automatic".to_owned(),
            Output::Display(name) => name.clone(),
            Output::Window => "Window".to_owned(),
            Output::Ndi(name) => format!("NDI  ·  {name}"),
            Output::DeckLink(out) => out.device.clone(),
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
                        for choice in output_choices(&screen.output, &screen.name, &state.displays, &state.decklink) {
                            let is_ndi = matches!(choice, Output::Ndi(_));
                            let label = output_label(&choice, &state.displays);
                            let on = match (&choice, &screen.output) {
                                (Output::DeckLink(one), Output::DeckLink(two)) => one.same_device(two),
                                _ => {
                                    std::mem::discriminant(&choice) == std::mem::discriminant(&screen.output)
                                        && (is_ndi || choice == screen.output)
                                }
                            };
                            let mut entry = ui.add(egui::Button::selectable(on, label));
                            if is_ndi && !state.ndi_available {
                                entry = entry.on_hover_text(sp_ndi::NOT_INSTALLED);
                            }
                            if entry.clicked() && !on {
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
            Output::DeckLink(_) => {
                "Played out of a Blackmagic Design DeckLink card, on its connector, in the format chosen here. A new slide goes at once, without a dissolve."
            }
            Output::Off => "This screen is kept in the list and shows nowhere.",
        };
        // A DeckLink output's format, under its output.
        if matches!(screen.output, Output::DeckLink(_)) {
            changed |= crate::screens::decklink_rows(ui, &mut state.screens[index], &state.decklink);
            ui.add_space(6.0);
        }
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
            if !look.shows_everything() && ui.small_button("Show Everything").on_hover_text("Back to every layer, black, nothing covered").clicked() {
                *look = look.everything();
                changed = true;
            }
            // Solid or see-through, or the key of another screen.
            changed |= crate::screens::background_rows(ui, &mut state.screens, index, &state.screen_status);
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
    let mut note = if songs.len() == 1 {
        "A song made in SoftPresenter goes to the Trash. One opened from elsewhere stays where it is on your Mac."
    } else {
        "Songs made in SoftPresenter go to the Trash. Ones opened from elsewhere stay where they are on your Mac."
    }
    .to_owned();
    // A song on the screens comes off them when it is deleted: say so first.
    if show.live().is_some_and(|live| songs.contains(&live.presentation)) {
        let on_screen = if songs.len() == 1 {
            "It\u{2019}s on the screens now; it comes off."
        } else {
            "One of them is on the screens now; it comes off."
        };
        note = format!("{on_screen} {note}");
    }
    if let Some(delete) = confirm_delete(ui, &question, &note) {
        state.confirm_delete_songs.clear();
        if delete {
            state.picked_songs.clear();
            state.songs_to_delete = songs;
        }
    }
}

/// Deletes slides of one song at once, asking nothing, in one step Undo takes back whole (it puts
/// them back where they were, picked). `slides` are places in the song; a song keeps at least
/// one slide.
pub fn delete_slides(state: &mut HomeState, song: usize, slides: &[usize]) {
    let Some(&first) = slides.iter().min() else { return };
    state.selected_slide = None;
    state.picked_slides.clear();
    state.slide_request =
        Some((SlideRef { presentation: song, slide: first }, SlideRequest::DeleteMany(slides.to_vec())));
}

/// The slides picked out of one song only: those of the song of the slide picked last. Delete
/// never takes slides from more than one song.
pub fn picks_of_one_song(picked: &[SlideRef]) -> Option<(usize, Vec<usize>)> {
    let song = picked.last()?.presentation;
    let mut slides: Vec<usize> = picked.iter().filter(|at| at.presentation == song).map(|at| at.slide).collect();
    slides.sort_unstable();
    slides.dedup();
    Some((song, slides))
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

/// Deletes one of the templates shown in the Theme Editor (the usual ones or a version's) at
/// once, asking nothing: Undo puts it back. A version's last template takes the version out of
/// the theme (and Undo puts the version back with it); Bible slides that used it use the
/// template that suits them best instead.
fn delete_template(state: &mut HomeState, template: usize) {
    let Some(edit) = state.theme_edit.as_mut() else { return };
    let Some(doomed) = edit.templates().get(template).cloned() else { return };
    // The version, as it was, when this is its last template.
    let version = edit.version.and_then(|id| {
        let at = edit.theme.versions.iter().position(|have| have.id == id)?;
        let version = &edit.theme.versions[at];
        (version.templates.len() == 1).then(|| (at, version.clone()))
    });
    let (theme, set) = (edit.theme.id, edit.version);
    if edit.theme.remove_template_in(edit.version, template) {
        state.removed.push(crate::removed::Removed::Template { theme, set, at: template, template: doomed, version });
        let Some(edit) = state.theme_edit.as_mut() else { return };
        edit.changed = true;
        // The template being edited stays open. If it was the one deleted, the one before it
        // (or the new first one) is opened in its place; with a version gone, the usual ones.
        let was = edit.template;
        let open = if was >= template { was.saturating_sub(1) } else { was };
        if was == template || !edit.theme.has_version(edit.version.unwrap_or_default()) {
            state.editing = edit.show(edit.version, open);
        } else {
            edit.template = open;
            if let Some(draft) = state.editing.as_mut() {
                draft.at.slide = open;
            }
        }
        state.note_deleted_in_editor();
    }
}

/// Asks whether to delete something Undo can't bring back, as a macOS alert does: its note,
/// then always "This can't be undone." (see [`cant_undo`]). Return deletes, as the Delete
/// button does; Escape cancels. `None` until it is answered.
fn confirm_delete(ui: &mut egui::Ui, question: &str, note: &str) -> Option<bool> {
    crate::studio::alert(ui.ctx(), question, crate::icons::Icon::Trash, question, &cant_undo(note), "Delete")
}

/// What a question about something Undo can't bring back says under it: its note, then always
/// "This can't be undone." Anything Undo can bring back goes at once, with no question.
pub fn cant_undo(note: &str) -> String {
    const CANT: &str = "This can\u{2019}t be undone.";
    if note.is_empty() { CANT.to_owned() } else { format!("{note} {CANT}") }
}

/// What a template is for, in a word or two, as its strip says it: Songs, Bible, Songs and
/// Bible, or By hand (only slides that name it use it).
fn used_for(songs: bool, bible: bool) -> &'static str {
    match (songs, bible) {
        (true, true) => "Songs and Bible",
        (true, false) => "Songs",
        (false, true) => "Bible",
        (false, false) => "By hand",
    }
}

/// The first of `base`, "`base` 2", "`base` 3"… that none of `names` is.
fn unused_name(base: &str, names: &[String]) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_owned() } else { format!("{base} {n}") })
        .find(|name| !names.contains(name))
        .unwrap_or_else(|| base.to_owned())
}

/// The top of Edit › Themes' left column, in the slides view's look: THEME with a +, the themes
/// as plain rows (MAIN on the main one: a click opens one, a click on the open one's name or a
/// double click renames it, and a right-click has its other choices); then TEMPLATES: a switch
/// between the usual templates and each version the theme has, + Version, a line about the
/// version shown with its ⋯, and the row of actions on the templates shown.
/// The Theme Editor's left column: the themes, then the templates' heading, the switch between
/// the usual templates and the versions, and the keys over the templates (`delete`: the Delete
/// key pressed with nothing picked on the canvas, the Delete key there).
fn theme_column(ui: &mut egui::Ui, state: &mut HomeState, delete: bool) {
    Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if crate::studio::list_heading_with_add(ui, "Theme", "New theme: a copy of the main theme").clicked() {
            state.theme_request = Some(ThemeRequest::New);
        }
        theme_rows(ui, state);
        crate::studio::list_heading(ui, "Templates");
        version_switch(ui, state);
        version_line(ui, state);
        template_actions(ui, state, delete);
    });
}

/// The themes, each a plain row: the theme icon, its name, MAIN on the main one. A row is
/// dragged to another place as slides are; the built-in theme stays first.
fn theme_rows(ui: &mut egui::Ui, state: &mut HomeState) {
    let editing = state.theme_edit.as_ref().map_or(0, |edit| edit.index);
    let names = state.theme_names.clone();
    let mut start_rename = None;
    let mut renamed: Option<Option<String>> = None;
    egui::ScrollArea::vertical().id_salt("theme list").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (mut rows, mut dragging, mut dropped) = (Vec::with_capacity(names.len()), None, None);
        for (index, name) in names.iter().enumerate() {
            let open = index == editing;
            let renaming = open && state.renaming_theme.is_some();
            let main = state.theme_for_songs == index;
            let look = crate::studio::PlainRow {
                icon: Icon::Theme,
                name: (!renaming).then_some(name.as_str()),
                tag: main.then_some("Main"),
                on: open,
            };
            // The built-in theme, first, stays there.
            let sense = if index == 0 { Sense::click() } else { Sense::click_and_drag() };
            let (response, start, right) = crate::studio::plain_row_sensing(ui, &look, sense);
            rows.push(response.rect);
            if response.dragged() {
                dragging = Some(index);
            }
            if crate::studio::drag_let_go(&response) {
                dropped = Some(index);
            }
            if renaming && let Some(typed) = state.renaming_theme.as_mut() {
                let look = crate::studio::Lettering::plain(crate::studio::semibold(13.5), crate::studio::TEXT);
                let field = crate::studio::rename_field_at(ui, typed, &look, start, right);
                if std::mem::take(&mut state.rename_started) {
                    field.request_focus();
                }
                let (enter, escape) =
                    ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                if escape {
                    renamed = Some(None);
                } else if enter || field.lost_focus() {
                    renamed = Some(Some(typed.clone()));
                }
            } else {
                // A click opens the theme; a click on the open theme's name, or a double click,
                // renames it, as in a file list.
                let on_name = response.interact_pointer_pos().is_some_and(|at| at.x >= start.x && at.x <= right);
                if response.clicked() || response.double_clicked() {
                    state.picking = Picking::Themes;
                }
                if !open && (response.clicked() || response.double_clicked()) {
                    state.theme_request = Some(ThemeRequest::Edit(index));
                }
                if response.double_clicked() || (response.clicked() && open && on_name) {
                    start_rename = Some(index);
                }
            }
            response.context_menu(|ui| theme_row_menu(ui, state, index, &mut start_rename));
        }
        let moving: Vec<usize> = dragging.or(dropped).into_iter().collect();
        let words = dragging.and_then(|at| names.get(at)).cloned().unwrap_or_default();
        if let (Some(from), Some(y)) = (dropped, list_drag(ui, &rows, (dragging, dropped), &moving, &words))
            && let Some(to) = dragged_to(&rows, from, y)
        {
            state.theme_request = Some(ThemeRequest::Move(from, to.max(1)));
        }
    });
    if let Some(index) = start_rename {
        // Renaming another theme opens it first; its name is retyped once it is open.
        if index != editing {
            state.theme_request = Some(ThemeRequest::Edit(index));
        }
        state.renaming_theme = names.get(index).cloned();
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
}

/// A theme row's right-click menu: Make It the Main Theme, Use On (the audience screens that
/// draw with it), Rename…, Duplicate and Delete… (the built-in theme is always kept).
fn theme_row_menu(ui: &mut egui::Ui, state: &mut HomeState, index: usize, start_rename: &mut Option<usize>) {
    let main = state.theme_for_songs == index;
    let make_main = ui
        .add_enabled(!main, egui::Button::new("Make It the Main Theme"))
        .on_hover_text("Songs and Bible slides are drawn with the main theme");
    if make_main.clicked() {
        state.theme_request = Some(ThemeRequest::UseForSongs(index));
        ui.close();
    }
    if state.screens.iter().any(|screen| screen.kind == crate::screens::Kind::Audience) {
        ui.menu_button("Use On", |ui| use_on_menu(ui, state, index));
    }
    ui.separator();
    if ui.button("Rename\u{2026}").clicked() {
        *start_rename = Some(index);
        ui.close();
    }
    if ui.button("Duplicate").clicked() {
        state.theme_request = Some(ThemeRequest::Duplicate(index));
        ui.close();
    }
    let delete = ui
        .add_enabled(!state.themes_kept.contains(&index), egui::Button::new("Delete\u{2026}"))
        .on_disabled_hover_text("The built-in themes are always kept");
    if delete.clicked() {
        state.confirm_delete_theme = Some(index);
        ui.close();
    }
}

/// A theme's Use On menu: the audience screens that draw with it, ticked (those set to it, and
/// for the main theme those left on the main theme). Ticking one sets it to the theme; unticking
/// puts it back on the main theme. Under the main theme a ticked screen can't be unticked, since
/// that means the main theme, this one: its tick is greyed, and says why.
fn use_on_menu(ui: &mut egui::Ui, state: &mut HomeState, index: usize) {
    let main = state.theme_for_songs == index;
    let id = state.theme_ids.get(index).copied();
    for screen in state.screens.iter_mut().filter(|screen| screen.kind == crate::screens::Kind::Audience) {
        let mut on = screen.theme == id || (main && screen.theme.is_none());
        let tick = ui
            .add_enabled(!(main && on), egui::Checkbox::new(&mut on, screen.name.as_str()))
            .on_disabled_hover_text("Follows the main theme");
        if tick.clicked() {
            // On: this theme (the main one needs nothing of its own). Off: back to the main theme.
            screen.theme = if on && !main { id } else { None };
            state.screens_changed = true;
        }
    }
}

/// The switch between the theme's usual templates and each of its versions, and + Version,
/// which wraps under it when they don't fit on one line. With no version: "Usual templates".
fn version_switch(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{SwitchLook, TEXT, TEXT_FAINT};
    let Some(edit) = state.theme_edit.as_ref() else { return };
    let versions: Vec<(sp_model::Uuid, String)> = edit.theme.versions.iter().map(|v| (v.id, v.name.clone())).collect();
    let chosen = edit.version.and_then(|id| versions.iter().position(|(have, _)| *have == id)).map_or(0, |at| at + 1);
    let look = SwitchLook {
        font: FontId::proportional(11.5),
        spacing: 0.0,
        pad: 10.0,
        height: 23.5,
        inset: 3.0,
        track: crate::studio::CARD,
        idle: crate::studio::TEXT_DIM,
    };
    let labels: Vec<&str> = std::iter::once("Usual").chain(versions.iter().map(|(_, name)| name.as_str())).collect();
    const BUTTON: egui::Vec2 = vec2(79.7, 24.0);
    let room = ui.available_width() - 16.0;
    let left = ui.cursor().min.x + 8.0;
    let top = ui.cursor().min.y + 2.0;
    let switch_wide = crate::studio::pill_cells(ui, pos2(left, top), &labels, &look)
        .last()
        .map_or(0.0, |cell| cell.max.x + look.inset - left);
    let line = look.height + 2.0 * look.inset;
    let words_wide = 100.0;
    let first_wide = if versions.is_empty() { words_wide } else { switch_wide };
    let one_line = first_wide + 8.0 + BUTTON.x <= room;
    let tall = if one_line { line } else { line + 8.0 + BUTTON.y };
    let (block, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0 + tall + 8.0), Sense::hover());
    if versions.is_empty() {
        let at = pos2(left, block.min.y + 2.0 + line / 2.0);
        ui.painter().text(at, Align2::LEFT_CENTER, "Usual templates", FontId::proportional(12.0), TEXT_FAINT);
    } else {
        let hints: Vec<String> = std::iter::once("The templates used in the usual look".to_owned())
            .chain(versions.iter().map(|(_, name)| format!("The templates used while the {name} version is on")))
            .collect();
        let hints: Vec<&str> = hints.iter().map(String::as_str).collect();
        let (_, clicked) =
            crate::studio::pill_switch(ui, "theme versions", pos2(left, top), &labels, &hints, Some(chosen), &look);
        if let Some(clicked) = clicked.filter(|clicked| *clicked != chosen)
            && let Some(edit) = state.theme_edit.as_mut()
        {
            let version = clicked.checked_sub(1).and_then(|at| versions.get(at)).map(|(id, _)| *id);
            state.editing = edit.show(version, 0);
            state.renaming_template = None;
        }
    }
    // + Version: at the right of the line, or of a line of its own under the switch.
    let y = if one_line { block.min.y + 2.0 + line / 2.0 } else { block.min.y + 2.0 + line + 8.0 + BUTTON.y / 2.0 };
    let place = Rect::from_center_size(pos2(left + room - BUTTON.x / 2.0, y), BUTTON);
    let response = ui.interact(place, ui.scope_id().with("add version"), Sense::click());
    crate::studio::paint_flat(
        ui.painter(),
        place,
        crate::studio::RADIUS,
        crate::studio::Flat::Plain,
        response.hovered(),
    );
    let words = ui.painter().layout_no_wrap("Version".to_owned(), FontId::proportional(11.5), TEXT);
    let start = place.center().x - (words.size().x + 10.0 + 7.0) / 2.0;
    let mark = Rect::from_center_size(pos2(start + 5.0, place.center().y), vec2(10.0, 10.0));
    icons::paint(ui.painter(), Icon::Plus, mark, TEXT);
    ui.painter().galley(pos2(start + 17.0, place.center().y - words.size().y / 2.0), words, TEXT);
    let hint = "Add a version to this theme: other templates for a moment in the service, such as the offering";
    if response.on_hover_text(hint).clicked() {
        state.version_sheet = Some(VersionSheet::adding(state));
    }
}

/// Under the switch, while a version is shown: what it is for, and its ⋯ (Rename…, Add a Copy
/// Of, Remove It From the theme).
fn version_line(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{MORE_BUTTON, TEXT_FAINT};
    let Some(edit) = state.theme_edit.as_ref() else { return };
    let Some(version) = edit.version.and_then(|id| edit.theme.versions.iter().find(|v| v.id == id)) else { return };
    let (id, name, theme_name, theme) = (version.id, version.name.clone(), edit.theme.name.clone(), edit.index);
    // The usual templates the version has no copy of yet, by name and what they are for.
    let copyable: Vec<(usize, String)> = edit
        .theme
        .templates
        .iter()
        .enumerate()
        .filter(|(_, usual)| {
            !version
                .templates
                .iter()
                .any(|have| have.name == usual.name && (have.songs, have.bible) == (usual.songs, usual.bible))
        })
        .map(|(at, usual)| (at, format!("{} ({})", usual.name, used_for(usual.songs, usual.bible).to_lowercase())))
        .collect();
    let words = format!("Used while the {name} version is on, on every screen with this theme.");
    let wide = ui.available_width() - 16.0 - MORE_BUTTON - 6.0;
    let galley = ui.painter().layout(words, FontId::proportional(11.5), TEXT_FAINT, wide);
    let (block, _) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y + 8.0), Sense::hover());
    let text_top = block.min.y;
    ui.painter().galley(pos2(block.min.x + 8.0, text_top), galley.clone(), TEXT_FAINT);
    let more = Rect::from_center_size(
        pos2(block.max.x - 8.0 - MORE_BUTTON / 2.0, text_top + galley.size().y / 2.0),
        egui::Vec2::splat(MORE_BUTTON),
    );
    let more = crate::studio::more_button(ui, more, ui.scope_id().with(("version more", id)), true);
    egui::Popup::menu(&more).show(|ui| {
        if ui.button("Rename\u{2026}").on_hover_text("Renames it in every theme that has it").clicked() {
            state.version_sheet =
                Some(VersionSheet { renaming: Some(id), name, songs: false, bible: false, focus: true });
            ui.close();
        }
        ui.menu_button("Add a Copy Of", |ui| {
            if copyable.is_empty() {
                ui.add_enabled(false, egui::Button::new("Every usual template is in it already"));
            }
            for (from, label) in &copyable {
                if ui.button(label.as_str()).clicked() {
                    state.version_asks.push(crate::versions::Ask::AddCopy { theme, version: id, from: *from });
                    ui.close();
                }
            }
        });
        ui.separator();
        // At once, asking nothing: Undo puts it back, with its templates.
        let hint = "Its templates go with it; Undo puts them back";
        if ui.button(format!("Remove It From \u{201c}{theme_name}\u{201d}")).on_hover_text(hint).clicked() {
            state.version_asks.push(crate::versions::Ask::Remove { theme, version: id });
            state.note_deleted_in_editor();
            ui.close();
        }
    });
}

/// The row of actions on the templates shown: Add (a new one with the open one's main box),
/// Duplicate, Delete (at once, Undo putting it back; also the Delete key, `delete`), and ↑ and ↓
/// to move the open one among them.
fn template_actions(ui: &mut egui::Ui, state: &mut HomeState, delete: bool) {
    let Some(edit) = state.theme_edit.as_ref() else { return };
    let (count, at, version) = (edit.templates().len(), edit.template, edit.version);
    let can_delete = count > 1 || version.is_some();
    let enabled = [true, count > 0, can_delete, at > 0, at + 1 < count];
    let hints = [
        "Add a template with the open one's main box",
        "Add a copy of the open template",
        "Delete the open template",
        "Move the open template up",
        "Move the open template down",
    ];
    match crate::studio::list_keys(ui, "template keys", enabled, hints).or(list_delete_key(delete, enabled)) {
        Some(crate::studio::ListKey::Add) => add_template_shown(state, false),
        Some(crate::studio::ListKey::Copy) => add_template_shown(state, true),
        Some(crate::studio::ListKey::Delete) => delete_template(state, at),
        Some(crate::studio::ListKey::Up) => move_template_shown(state, true),
        Some(crate::studio::ListKey::Down) => move_template_shown(state, false),
        None => {}
    }
}

/// Adds a template to the set shown in the Theme Editor, after the one open, and opens it with
/// its name ready to retype: with `copy` a copy of the open one; otherwise a new one with the
/// open one's main box only, for songs in a version and by hand among the usual templates (so
/// it takes no song's slides until it is told to).
fn add_template_shown(state: &mut HomeState, copy: bool) {
    let Some(edit) = state.theme_edit.as_mut() else { return };
    let (set, after) = (edit.version, edit.template);
    let original = edit.templates().get(after).map(|t| t.name.clone()).unwrap_or_default();
    let names: Vec<String> = edit.templates().iter().map(|t| t.name.clone()).collect();
    let Some(at) = edit.theme.add_template_in(set, after) else { return };
    let Some(added) = edit.theme.set_mut(set).and_then(|templates| templates.get_mut(at)) else { return };
    if copy {
        added.name = unused_name(&format!("{original} copy"), &names);
    } else {
        added.boxes = added.box_for(Some(&sp_model::Role::primary())).cloned().into_iter().collect();
        added.objects.clear();
        added.songs = set.is_some();
    }
    let name = added.name.clone();
    edit.changed = true;
    state.editing = edit.show(set, at);
    state.renaming_template = Some((at, name));
    state.rename_started = true;
}

/// Moves the open template one place up or down among those shown.
fn move_template_shown(state: &mut HomeState, up: bool) {
    let Some(edit) = state.theme_edit.as_mut() else { return };
    if let Some(to) = edit.theme.move_template_in(edit.version, edit.template, up) {
        edit.template = to;
        edit.changed = true;
        if let Some(draft) = state.editing.as_mut() {
            draft.at.slide = to;
        }
    }
}

/// The templates shown in Edit › Themes, each a picture with a strip (its number, its name and
/// what it is for), the open one in the chosen grey. A click opens one; a click on the open
/// one's name, or a double click, renames it; a drag moves it to where the ink line is; a
/// right-click has Used For, Rename…, Duplicate and Delete…. Returns the template clicked, to
/// open.
fn template_pictures(
    ui: &mut egui::Ui,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    canvas: Option<TextureId>,
) -> Option<usize> {
    let edit = state.theme_edit.as_ref()?;
    let shown: Vec<(String, bool, bool, Slide)> =
        edit.templates().iter().map(|t| (t.name.clone(), t.songs, t.bible, t.as_slide())).collect();
    let (current, version) = (edit.template, edit.version);
    let can_delete = shown.len() > 1 || version.is_some();
    let mut chosen = None;
    let mut ask_delete = None;
    let mut used: Option<(usize, bool, bool)> = None;
    let mut duplicate = None;
    let mut start_rename = None;
    let mut renamed: Option<Option<(usize, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let mut moved = None;
    let list = egui::Id::unique(("theme template pictures", version));
    egui::ScrollArea::vertical().id_salt("theme layout list").auto_shrink([false, false]).show(ui, |ui| {
        let side = egui::Margin { left: 20, right: 20, top: crate::studio::LIST_TOP as i8, bottom: 12 };
        Frame::new().inner_margin(side).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, crate::studio::LIST_BETWEEN);
            let (mut rows, mut dragging, mut dropped) = (Vec::with_capacity(shown.len()), None, None);
            for (index, (name, songs, bible, slide)) in shown.iter().enumerate() {
                let open = index == current;
                let renaming = state.renaming_template.as_ref().is_some_and(|(which, _)| *which == index);
                let label = format!("{name} \u{b7} {}", used_for(*songs, *bible));
                let look = crate::studio::ListPicture {
                    number: index + 1,
                    name: &label,
                    dot: None,
                    open,
                    live: false,
                    renaming,
                };
                let (response, places) = crate::studio::list_picture(ui, list, index, &look);
                rows.push(response.rect);
                if response.dragged() {
                    dragging = Some(index);
                }
                if crate::studio::drag_let_go(&response) {
                    dropped = Some(index);
                }
                let image = places.picture;
                // Only the pictures in sight are drawn, or asked for; the open template is shown
                // as it is on the canvas this instant.
                if ui.is_rect_visible(image) {
                    let corner = crate::studio::LIST_PICTURE_CORNER;
                    ui.painter().rect_filled(image, corner, Color32::BLACK);
                    let picture = match canvas.filter(|_| open) {
                        Some(texture) => Some(texture),
                        None => thumb(ThumbOf::Layout(index, slide)).map(|(texture, _)| texture),
                    };
                    match picture {
                        Some(texture) => {
                            egui::Image::from_texture((texture, image.size()))
                                .corner_radius(corner)
                                .paint_at(ui, image);
                        }
                        None => ui.ctx().request_repaint(),
                    }
                }
                if renaming && let Some((_, typed)) = state.renaming_template.as_mut() {
                    let look = crate::studio::list_name_look(crate::studio::TEXT);
                    let field = crate::studio::rename_field_at(ui, typed, &look, places.name, places.name_right);
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((index, typed.clone())));
                    }
                } else {
                    let on_name = response.interact_pointer_pos().is_some_and(|at| at.y > image.max.y);
                    if response.double_clicked() || (response.clicked() && open && on_name) {
                        start_rename = Some((index, name.clone()));
                    } else if response.clicked() && !open {
                        chosen = Some(index);
                    }
                }
                response.context_menu(|ui| {
                    ui.menu_button("Used For", |ui| {
                        // Named as the strip under each picture names them.
                        for (wanted, hint) in [
                            ((true, false), "A song's slide uses, of these, the one that suits it"),
                            ((false, true), "Bible slides are drawn with it, in this set"),
                            ((true, true), "Songs pick among it, and Bible slides use it"),
                            ((false, false), "Only slides that name it use it"),
                        ] {
                            let on = (*songs, *bible) == wanted;
                            let words = used_for(wanted.0, wanted.1);
                            if ui.add(egui::Button::selectable(on, words)).on_hover_text(hint).clicked() {
                                used = Some((index, wanted.0, wanted.1));
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Rename\u{2026}").clicked() {
                        start_rename = Some((index, name.clone()));
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        duplicate = Some(index);
                        ui.close();
                    }
                    if ui.add_enabled(can_delete, egui::Button::new("Delete")).clicked() {
                        ask_delete = Some(index);
                        ui.close();
                    }
                });
            }
            let moving: Vec<usize> = dragging.or(dropped).into_iter().collect();
            let words = dragging.and_then(|at| shown.get(at)).map(|(name, ..)| name.clone()).unwrap_or_default();
            if let (Some(from), Some(y)) = (dropped, list_drag(ui, &rows, (dragging, dropped), &moving, &words)) {
                moved = dragged_to(&rows, from, y).map(|to| (from, to));
            }
        });
    });
    // Dragged to another place: the template open stays open, wherever it went.
    if let Some((from, to)) = moved
        && let Some(edit) = state.theme_edit.as_mut()
        && let Some(templates) = edit.theme.set_mut(version)
        && from < templates.len()
        && to < templates.len()
    {
        let template = templates.remove(from);
        templates.insert(to, template);
        edit.template = place_after_move(edit.template, from, to);
        edit.changed = true;
        if let Some(draft) = state.editing.as_mut() {
            draft.at.slide = edit.template;
        }
        state.renaming_template = None;
    }
    // A picture's Delete deletes that template at once (Undo puts it back).
    if let Some(template) = ask_delete {
        delete_template(state, template);
    }
    if let Some((index, songs, bible)) = used
        && let Some(edit) = state.theme_edit.as_mut()
    {
        if bible {
            edit.theme.set_bible_template_in(version, index);
        }
        if let Some(template) = edit.theme.set_mut(version).and_then(|templates| templates.get_mut(index)) {
            template.songs = songs;
            template.bible = bible;
        }
        edit.changed = true;
    }
    if let Some(index) = duplicate
        && let Some(draft) = state.theme_edit.as_mut().and_then(|edit| edit.show(version, index))
    {
        state.editing = Some(draft);
        add_template_shown(state, true);
        rename_started = state.rename_started;
    }
    if let Some(start) = start_rename {
        state.renaming_template = Some(start);
        rename_started = true;
    }
    state.rename_started = rename_started;
    if let Some(outcome) = renamed {
        state.renaming_template = None;
        if let (Some((template, name)), Some(edit)) = (outcome, state.theme_edit.as_mut()) {
            let name = name.trim();
            if let Some(changed) = edit.theme.set_mut(version).and_then(|templates| templates.get_mut(template))
                && !name.is_empty()
                && changed.name != name
            {
                changed.name = name.to_owned();
                edit.changed = true;
            }
        }
    }
    chosen
}

/// The sheet that adds a version to the theme being edited, or renames one in every theme.
pub struct VersionSheet {
    /// The version renamed; none when one is being added.
    pub renaming: Option<sp_model::Uuid>,
    pub name: String,
    /// Adding: start from copies of the theme's song templates, and of its Bible template.
    pub songs: bool,
    pub bible: bool,
    /// The name takes the keyboard once, when the sheet opens.
    pub focus: bool,
}

impl VersionSheet {
    /// Adding a version to the theme being edited: the name another theme's version has (so the
    /// screens switch together), starting from copies of its song templates.
    pub fn adding(state: &HomeState) -> VersionSheet {
        let theme = state.theme_edit.as_ref().map(|edit| &edit.theme);
        let other = state.versions.iter().find(|version| theme.is_none_or(|theme| !theme.has_version(version.id)));
        let songs = theme.is_some_and(|theme| theme.templates.iter().any(|t| t.songs));
        VersionSheet {
            renaming: None,
            name: other.map(|v| v.name.clone()).unwrap_or_default(),
            songs,
            bible: false,
            focus: true,
        }
    }
}

/// The sheet over the Theme Editor that adds a version to the theme ("Add a version to …": its
/// name, offered from the versions other themes have, and which templates it starts as copies
/// of), or renames one. Return does it; Escape or Cancel closes it.
fn version_sheet(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{Flat, TEXT, TEXT_DIM, TEXT_FAINT};
    let Some(edit) = state.theme_edit.as_ref() else {
        state.version_sheet = None;
        return;
    };
    let Some(sheet) = state.version_sheet.as_mut() else { return };
    let theme = &edit.theme;
    let others: Vec<String> =
        state.versions.iter().filter(|version| !theme.has_version(version.id)).map(|v| v.name.clone()).collect();
    let song_names: Vec<String> = theme.templates.iter().filter(|t| t.songs).map(|t| t.name.clone()).collect();
    let bible_name = theme.templates.iter().find(|t| t.bible && !t.songs).map(|t| t.name.clone());
    let title = match sheet.renaming {
        Some(_) => "Rename version".to_owned(),
        None => format!("Add a version to \u{201c}{}\u{201d}", theme.name),
    };
    let frame = Frame::new()
        .fill(crate::studio::PANEL)
        .stroke(Stroke::new(1.0, crate::studio::LINE))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 18, right: 18, top: 16, bottom: 16 });
    let mut decided = None;
    let id = egui::Id::unique("version sheet");
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0)).fade_in(false);
    egui::Modal::new(id).area(area).backdrop_color(Color32::from_black_alpha(110)).frame(frame).show(ui.ctx(), |ui| {
        ui.set_width(424.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let note = |ui: &mut egui::Ui, words: &str| {
            ui.add(egui::Label::new(
                egui::RichText::new(words.to_uppercase())
                    .font(crate::studio::fixed(11.0))
                    .extra_letter_spacing(0.6)
                    .color(TEXT_FAINT),
            ));
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&title).font(crate::studio::semibold(15.0)).color(TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (close, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                if crate::studio::quiet_icon(ui, close, ui.scope_id().with("close"), Icon::Close, 11.0, 0.7, None)
                    .on_hover_text("Close")
                    .clicked()
                {
                    decided = Some(false);
                }
            });
        });
        let about = match sheet.renaming {
            Some(_) => "The name is shared: every theme\u{2019}s version of this name is renamed with it.",
            None => {
                "A version is another set of templates for a moment in the service, such as the offering, communion \
                 or a kids\u{2019} segment. Switching to it by name changes every screen whose theme has a version of \
                 that name, each in its own way; the others stay as they are."
            }
        };
        ui.label(egui::RichText::new(about).size(12.5).color(TEXT_DIM));
        ui.label(egui::RichText::new("Name").size(12.5).color(TEXT));
        ui.horizontal(|ui| {
            let wide = if sheet.renaming.is_none() && !others.is_empty() { 424.0 - 38.0 } else { 424.0 };
            let field = ui.add(egui::TextEdit::singleline(&mut sheet.name).hint_text("Offering").desired_width(wide));
            if std::mem::take(&mut sheet.focus) {
                field.request_focus();
            }
            if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                decided = Some(true);
            }
            // The names other themes' versions have, to take one.
            if sheet.renaming.is_none() && !others.is_empty() {
                let pick = ui.add(egui::Button::new("\u{25be}").min_size(vec2(30.0, 24.0)));
                egui::Popup::menu(&pick.on_hover_text("The versions other themes have")).show(|ui| {
                    for other in &others {
                        if ui.button(other.as_str()).clicked() {
                            sheet.name = other.clone();
                            ui.close();
                        }
                    }
                });
            }
        });
        if sheet.renaming.is_none() {
            if !others.is_empty() {
                let quoted: Vec<String> = others.iter().map(|name| format!("\u{201c}{name}\u{201d}")).collect();
                let verb = if others.len() == 1 { "is" } else { "are" };
                note(
                    ui,
                    &format!(
                        "{} {verb} in another theme: use the same name so the screens switch together.",
                        quoted.join(", ")
                    ),
                );
            }
            ui.label(egui::RichText::new("Start from copies of").size(12.5).color(TEXT));
            let songs_words = match song_names.is_empty() {
                true => "The song templates".to_owned(),
                false => format!("The song templates ({})", song_names.join(", ")),
            };
            ui.add_enabled(!song_names.is_empty(), egui::Checkbox::new(&mut sheet.songs, songs_words));
            let bible_words = match &bible_name {
                Some(name) => format!("The Bible template ({name})"),
                None => "The Bible template".to_owned(),
            };
            ui.add_enabled(bible_name.is_some(), egui::Checkbox::new(&mut sheet.bible, bible_words));
            note(ui, "A kind it has no template for (songs or Bible) keeps the usual look while the version is on.");
        }
        ui.add_space(4.0);
        let ready = !sheet.name.trim().is_empty() && (sheet.renaming.is_some() || sheet.songs || sheet.bible);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        let font = FontId::proportional(12.5);
        let yes_words = if sheet.renaming.is_some() { "Rename" } else { "Add version" };
        let yes_wide = crate::studio::flat_button_width(ui, &[yes_words], &font, 13.0);
        let yes = Rect::from_min_size(pos2(row.max.x - yes_wide, row.min.y), vec2(yes_wide, 30.0));
        let no_wide = crate::studio::flat_button_width(ui, &["Cancel"], &font, 13.0);
        let no = Rect::from_min_size(pos2(yes.min.x - 8.0 - no_wide, row.min.y), vec2(no_wide, 30.0));
        if crate::studio::flat_button(ui, no, ui.scope_id().with("cancel"), "Cancel", font.clone(), Flat::Plain, true)
            .clicked()
        {
            decided = Some(false);
        }
        if crate::studio::flat_button(ui, yes, ui.scope_id().with("yes"), yes_words, font, Flat::Main, ready).clicked()
        {
            decided = Some(true);
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            decided = Some(false);
        }
        if decided == Some(true) && !ready {
            decided = None;
        }
    });
    match decided {
        Some(true) => {
            let Some(sheet) = state.version_sheet.take() else { return };
            let name = sheet.name.trim().to_owned();
            let ask = match sheet.renaming {
                Some(version) => crate::versions::Ask::Rename { version, name },
                None => {
                    let from = (0..theme.templates.len())
                        .filter(|at| {
                            let template = &theme.templates[*at];
                            (sheet.songs && template.songs) || (sheet.bible && template.bible && !template.songs)
                        })
                        .collect();
                    crate::versions::Ask::Add { theme: edit.index, name, from }
                }
            };
            state.version_asks.push(ask);
        }
        Some(false) => state.version_sheet = None,
        None => {}
    }
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

/// ↑ and ↓ in the Bible view. Under Slides they pick the verse before or after (see
/// `running::move_pick`) and never put it on screen; Enter does. On the page, where the verses
/// run down, ↓ is as → and ↑ as ←, by the owner's rules (`running::pick_key`, 2026-11-22): with a
/// verse picked and nothing on screen they move the pick to the verse below or above; with
/// something on screen they go on with it, as → and ← do (Enter takes the pick). With neither,
/// both pick the passage's first verse. Not while something is typed, under a menu, the Options
/// or the list of keys, or with a search's verses listed in place of the slides.
fn bible_pick_keys(ui: &egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let free = !ui.ctx().text_edit_focused()
        && state.library_search.is_none()
        && state.edit_kind().is_none()
        && !state.key_sheet
        && !state.alert_open()
        && !state.bible.options.open
        && !state.bible.options.manage_open
        && state.bible.options.details.is_empty()
        && state.bible.found.is_empty()
        && !egui::Popup::is_any_open(ui.ctx());
    let (up, down) = ui.input(|input| (input.key_pressed(egui::Key::ArrowUp), input.key_pressed(egui::Key::ArrowDown)));
    if !free || !(up || down) {
        return;
    }
    // With something on screen, ↓ and ↑ go on and back with it on the page and under Slides
    // alike, as → and ← do (the owner's rule); otherwise they move the pick.
    let showing = show.live().is_some();
    if crate::running::pick_key(false, down, up, state.pick(show).is_some(), showing, false)
        == crate::running::PickKey::Pass
        && showing
    {
        commands.push(if down { Command::Next } else { Command::Previous });
        return;
    }
    let Some(slot) = state.bible.slots.map(|slots| slots[state.bible.showing]) else { return };
    let count = show.presentations().get(slot).map_or(0, |passage| passage.slide_count());
    let picked = state.pick(show).filter(|at| at.presentation == slot).map(|at| at.slide);
    let live = show.live().filter(|at| at.presentation == slot).map(|at| at.slide);
    // With neither, the passage's first verse, among its chapter's slides when they were made.
    let first = state.bible.passage_slides().start;
    let moved = match (picked, live) {
        (None, None) if first < count => Ok(first),
        _ => crate::running::move_pick(count, picked, live, down),
    };
    match moved {
        Ok(slide) => state.set_pick(show, SlideRef { presentation: slot, slide }, true),
        Err(words) => state.say(words),
    }
}

/// The Bible page, in place of the slides: the bar along its top (the reference, a verse longer
/// at either end, Add, Page | Slides, Options), the translations line, then the passage's
/// chapter as a page to read, or its slides, or the verses words were found in.
fn bible_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let live = show.live().map(|at| at.presentation);
    bible_pick_keys(ui, show, state, commands);
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let side = |left: i8, top: i8| egui::Margin { left, right: left, top, bottom: 0 };
        Frame::new().inner_margin(side(40, 22)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            bible_bar(ui, show, state, thumb, live);
            ui.add_space(14.0);
            bible_translations(ui, state);
            // Said when two Bibles are shown together and one only mostly follows a numbering.
            let own: Vec<&str> = state
                .bible
                .rows
                .iter()
                .filter_map(|row| row.bible.and_then(|at| state.bible.shelf.bibles.get(at)))
                .filter(|b| b.loose)
                .map(|b| b.name.as_str())
                .collect();
            let loose = (!own.is_empty() && state.bible.rows.len() > 1).then(|| {
                format!(
                    "{} numbers a few verses its own way, so beside another Bible a verse here and there may be one off.",
                    own.join(" and ")
                )
            });
            for note in loose.into_iter().chain(state.bible.message.clone()) {
                ui.add_space(4.0);
                ui.add(egui::Label::new(egui::RichText::new(note).size(12.5).color(crate::studio::CARE)).wrap());
            }
        });
        // A change to the Bibles or the options remakes the passage that is showing, named for
        // the operator as before.
        if !state.bible.shown.is_empty() && state.bible.made_with != state.bible.choices() {
            state.bible.remake(live);
        }
        let slides = state.bible.view == BibleView::Slides;
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]);
        let showing = slot
            .filter(|slot| show.presentations().get(*slot).is_some_and(|passage| passage.slide_count() > 0))
            .filter(|_| !state.bible.shown.is_empty());
        if state.bible.found.is_empty()
            && showing.is_some()
            && let Some((words, pill)) = crate::bible_page::title(&mut state.bible)
        {
            Frame::new().inner_margin(side(40, 0)).show(ui, |ui| crate::bible_page::paint_title(ui, &words, &pill));
        }
        Frame::new().inner_margin(side(28, 0)).show(ui, |ui| {
            if !state.bible.found.is_empty() {
                crate::bible_page::found_list(ui, show, state);
                return;
            }
            match showing {
                Some(slot) if slides => slide_grid(ui, show, slot, state, thumb, commands),
                Some(_) => crate::bible_page::page_view(ui, show, state, commands),
                None => bible_start(ui),
            }
        });
    });
}

/// What the Bible page says before anything is looked up: centred, in lines no wider than read
/// well.
fn bible_start(ui: &mut egui::Ui) {
    let room = ui.available_size();
    ui.add_space((room.y * 0.3).max(20.0));
    ui.vertical_centered(|ui| {
        ui.set_max_width((room.x - 48.0).clamp(160.0, 440.0));
        let back = format!("{} comes to the reference from anywhere.", crate::keys::hint_key("2"));
        for line in [
            "Type a reference such as John 3:16-18 (or Ин 3:16, or words to find) in the field above, or click a book.",
            "The chapter appears here as you type. Return picks the first verse, Enter puts it on screen.",
            back.as_str(),
        ] {
            ui.add(egui::Label::new(egui::RichText::new(line).color(DIM)).wrap().halign(Align::Center));
            ui.add_space(4.0);
        }
    });
}

/// The look of the Bible page's small switches (Page | Slides).
fn bible_switch_look() -> crate::studio::SwitchLook {
    crate::studio::SwitchLook {
        font: FontId::proportional(12.0),
        spacing: 0.0,
        pad: 11.0,
        height: 24.2,
        inset: 3.0,
        track: crate::studio::CARD,
        idle: crate::studio::TEXT_DIM,
    }
}

/// The id of the reference field.
fn scripture_lookup_id() -> egui::Id {
    egui::Id::unique("bible scripture lookup")
}

/// The bar along the top of the Bible page: the reference field (236 wide when there is room,
/// so "От Иоанна 3:16" fits), Books (while the books are put away), a verse longer at the start
/// and at the end; at the right Add with its arrow for another playlist, Page | Slides and
/// Options. In a middle too narrow for one row, the right part takes a second.
fn bible_bar(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    live: Option<usize>,
) {
    use crate::studio::{Flat, flat_button, flat_button_width};
    const GAP: f32 = 12.0;
    const STEP: f32 = 34.0;
    const ARROW: f32 = 26.0;
    const LANGUAGE: f32 = 76.0;
    let font = FontId::proportional(12.5);
    let width = ui.available_width();
    let look = bible_switch_look();
    let views = ["Page", "Slides"];
    let views_width =
        crate::studio::pill_cells(ui, pos2(0.0, 0.0), &views, &look).last().map_or(0.0, |cell| cell.max.x) + look.inset;
    let options_width = flat_button_width(ui, &["Options"], &font, 13.0);
    let add_width = flat_button_width(ui, &["Add"], &font, 13.0) + 18.0;
    let books_width = if state.bible.lists_open { 0.0 } else { flat_button_width(ui, &["Books"], &font, 13.0) + GAP };
    let left = books_width + GAP + STEP + 6.0 + STEP;
    let right = add_width + 1.0 + ARROW + GAP + 1.0 + GAP + views_width + 8.0 + LANGUAGE + 8.0 + options_width;
    let one_row = width - left - GAP - right;
    let two_rows = one_row < 170.0;
    let field_width = if two_rows { width - left } else { one_row }.clamp(120.0, 236.0);
    let (bar, _) = ui.allocate_exact_size(vec2(width, if two_rows { 34.0 + 8.0 + 30.0 } else { 34.0 }), Sense::hover());
    let middle = bar.min.y + 17.0;
    bible_field(ui, show, state, Rect::from_min_size(bar.min, vec2(field_width, 34.0)), live);
    let mut x = bar.min.x + field_width + GAP;
    let key = |x: f32, wide: f32, middle: f32| Rect::from_min_size(pos2(x, middle - 15.0), vec2(wide, 30.0));
    if !state.bible.lists_open {
        let books = key(x, books_width - GAP, middle);
        if flat_button(ui, books, egui::Id::unique("bible books button"), "Books", font.clone(), Flat::Plain, true)
            .on_hover_text("Bring the books, chapters and verses back")
            .clicked()
        {
            state.bible.lists_open = true;
            state.bible.list_scroll = true;
        }
        x += books_width;
    }
    // A verse longer at the start, or at the end.
    let shown = !state.bible.shown.is_empty();
    let back = key(x, STEP, middle);
    let on = key(x + STEP + 6.0, STEP, middle);
    let step_font = crate::studio::semibold(12.5);
    if flat_button(ui, back, egui::Id::unique("bible verse before"), "\u{2039}", step_font.clone(), Flat::Plain, shown)
        .on_hover_text("A verse longer at the start")
        .clicked()
    {
        state.bible.step(false, live);
    }
    if flat_button(ui, on, egui::Id::unique("bible verse after"), "\u{203a}", step_font, Flat::Plain, shown)
        .on_hover_text("A verse longer at the end")
        .clicked()
    {
        state.bible.step(true, live);
    }
    // From the right: Options, Page | Slides, the divider, Add.
    let middle = if two_rows { bar.max.y - 15.0 } else { middle };
    let options = key(bar.max.x - options_width, options_width, middle);
    let language = key(options.min.x - 8.0 - LANGUAGE, LANGUAGE, middle);
    bible_read_in(ui, state, language);
    let views_at = pos2(language.min.x - 8.0 - views_width, middle - (look.height + 2.0 * look.inset) / 2.0);
    let divider = views_at.x - GAP - 0.5;
    ui.painter().vline(divider, middle - 10.0..=middle + 10.0, Stroke::new(1.0, crate::studio::LINE));
    let arrow = key(divider - 0.5 - GAP - ARROW, ARROW, middle);
    let add = key(arrow.min.x - 1.0 - add_width, add_width, middle);
    bible_add(ui, state, add, arrow);
    let chosen = match state.bible.view {
        BibleView::Page => 0,
        BibleView::Slides => 1,
    };
    let hints = [
        "The chapter, to read and click",
        "The slides, as the Bible theme draws them: the chapter's, with the passage's ringed, a slide a verse",
    ];
    match crate::studio::pill_switch(ui, "bible view", views_at, &views, &hints, Some(chosen), &look).1 {
        Some(0) => state.bible.view = BibleView::Page,
        Some(_) => state.bible.view = BibleView::Slides,
        None => {}
    }
    let open = state.bible.options.open;
    let button = flat_button(
        ui,
        options,
        egui::Id::unique("bible options button"),
        "Options",
        font,
        if open { Flat::On } else { Flat::Plain },
        true,
    )
    .on_hover_text("How verses become slides, and which Bibles are installed");
    if button.clicked() {
        state.bible.options.open = !open;
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
}

/// The Аа button: the language the operator reads the Bible page in (book names, the reference,
/// the page's first translation, the find box's Bible rows), Auto or one with a Bible in use.
/// 76 wide whatever it says, so nothing moves. Its menu: Auto with the language it reads in
/// now, then English, Русский and Українська (one with no Bible in use greyed). The slides on
/// screen never change with it.
fn bible_read_in(ui: &mut egui::Ui, state: &mut HomeState, rect: Rect) {
    use crate::studio::{Flat, TEXT, TEXT_FAINT, paint_flat};
    let response = ui.interact(rect, egui::Id::unique("bible read in"), Sense::click());
    if std::mem::take(&mut state.bible.open_read_in) {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
    }
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    paint_flat(ui.painter(), rect, 9.0, Flat::Plain, response.hovered() || open);
    let in_use = state.bible.languages_in_use();
    let chosen = state.bible.read_in.clone().filter(|language| in_use.contains(language));
    let code = chosen.as_deref().map_or_else(|| "Auto".to_owned(), str::to_uppercase);
    let mark = ui.painter().layout_no_wrap("Аа".to_owned(), crate::studio::semibold(12.0), TEXT_FAINT);
    let words = ui.painter().layout_no_wrap(code, FontId::proportional(12.5), TEXT);
    let left = rect.center().x - (mark.size().x + 6.0 + words.size().x) / 2.0;
    let words_at = pos2(left + mark.size().x + 6.0, rect.center().y - words.size().y / 2.0);
    ui.painter().galley(pos2(left, rect.center().y - mark.size().y / 2.0), mark, TEXT_FAINT);
    ui.painter().galley(words_at, words, TEXT);
    let response = response.on_hover_text(
        "The language you read this page in: book names, the reference and the verse text. The slides on screen \
         don't change; they keep the translations line.",
    );
    let mut picked: Option<Option<&str>> = None;
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(220.0);
        ui.label(egui::RichText::new("Read the Bible page in").size(11.5).color(TEXT_FAINT));
        let auto = crate::bible::language_name(&state.bible.auto_language());
        if ui
            .add(egui::Button::selectable(chosen.is_none(), format!("Auto \u{b7} {auto}")))
            .on_hover_text("The language typed, else the last one typed, else the computer's")
            .clicked()
        {
            picked = Some(None);
        }
        for language in ["en", "ru", "uk"] {
            let name = crate::bible::language_name(language);
            let has = in_use.iter().any(|have| have == language);
            let words = if has { name.to_owned() } else { format!("{name} \u{b7} no Bible in use") };
            if ui.add_enabled(has, egui::Button::selectable(chosen.as_deref() == Some(language), words)).clicked() {
                picked = Some(Some(language));
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("The slides on screen keep the translations line").size(11.5).color(TEXT_FAINT));
    });
    if let Some(choice) = picked {
        state.bible.set_read_in(choice.map(str::to_owned));
    }
}

/// Add, which keeps the passage in the playlist in front (to open again with these settings),
/// and the arrow beside it, which keeps it in another playlist.
fn bible_add(ui: &mut egui::Ui, state: &mut HomeState, add: Rect, arrow: Rect) {
    use crate::studio::{BUTTON, BUTTON_HOVER, TEXT, TEXT_FAINT};
    let saved = state.bible.saved();
    let enabled = saved.is_some();
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let corner = |left: bool| {
        let (round, square) = (crate::studio::RADIUS_CARD as u8, 0);
        if left {
            egui::CornerRadius { nw: round, sw: round, ne: square, se: square }
        } else {
            egui::CornerRadius { nw: square, sw: square, ne: round, se: round }
        }
    };
    let front = state.services.front().name.clone();
    let response = ui.interact(add, egui::Id::unique("bible add"), sense);
    let open_arrow = ui.interact(arrow, egui::Id::unique("bible add elsewhere"), sense);
    let ink = if enabled { TEXT } else { TEXT_FAINT };
    for (rect, hovered, left) in [(add, response.hovered(), true), (arrow, open_arrow.hovered(), false)] {
        let fill = if !enabled {
            BUTTON.gamma_multiply(0.5)
        } else if hovered {
            BUTTON_HOVER
        } else {
            BUTTON
        };
        ui.painter().rect_filled(rect, corner(left), fill);
    }
    let words = ui.painter().layout_no_wrap("Add".to_owned(), FontId::proportional(12.5), ink);
    let start = add.center().x - (words.size().x + 18.0) / 2.0;
    icons::paint(
        ui.painter(),
        Icon::Plus,
        Rect::from_center_size(pos2(start + 6.0, add.center().y), vec2(12.0, 12.0)),
        ink,
    );
    ui.painter().galley(pos2(start + 18.0, add.center().y - words.size().y / 2.0), words, ink);
    ui.painter().text(arrow.center(), Align2::CENTER_CENTER, "\u{25be}", FontId::proportional(11.0), ink);
    let reading = state.bible.reading.clone();
    let response = response
        .on_hover_text(format!("Add {reading} to \u{201c}{front}\u{201d}, to open again with these settings"))
        .on_disabled_hover_text("Look a passage up first");
    if response.clicked()
        && let Some(saved) = saved.clone()
    {
        let front = state.services.front_at();
        add_passage(state, Some(front), saved, &reading);
    }
    // The other playlists (Add already adds to the one in front), then a new one.
    let open_arrow = open_arrow.on_hover_text("Add it to another playlist");
    egui::Popup::from_toggle_button_response(&open_arrow)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let Some(saved) = saved.clone() else { return };
            let front = state.services.front_at();
            let names: Vec<(usize, String)> = state
                .services
                .list
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != front)
                .map(|(at, playlist)| (at, playlist.name.clone()))
                .collect();
            let mut chosen = None;
            for (at, name) in &names {
                if ui.button(format!("Add to \u{201c}{name}\u{201d}")).clicked() {
                    chosen = Some(Some(*at));
                }
            }
            if !names.is_empty() {
                ui.separator();
            }
            if ui.button("New Playlist\u{2026}").on_hover_text("A new playlist with this passage in it").clicked() {
                chosen = Some(None);
            }
            if let Some(playlist) = chosen {
                add_passage(state, playlist, saved, &reading);
                ui.close();
            }
        });
}

/// Keeps the passage on the Bible page (`saved`, named `reading`) in a playlist, by its place,
/// and says so; in the playlist in front it becomes the open item. The same passage in the same
/// translations goes in once: then the notice says it is there already. With no playlist given,
/// a new one is made with the passage in it and comes to the front, its name ready to type in
/// the chooser of the Show view, as a song's New Playlist… does.
fn add_passage(state: &mut HomeState, playlist: Option<usize>, saved: crate::service::SavedPassage, reading: &str) {
    let item = crate::service::Item::Passage(saved);
    let Some(at) = playlist else {
        state.services.add_service();
        state.services.add(item);
        let name = state.services.front().name.clone();
        state.notice = Some(format!("Added {reading} to {name}."));
        state.playlist_open = true;
        state.chooser_open = true;
        state.renaming_service = Some((state.services.front_at(), name));
        state.rename_started = true;
        state.view_request = Some(0);
        return;
    };
    let Some(name) = state.services.list.get(at).map(|list| list.name.clone()) else { return };
    let added =
        if at == state.services.front_at() { state.services.add(item) } else { state.services.add_to(at, item) };
    state.notice =
        Some(if added { format!("Added {reading} to {name}.") } else { format!("{reading} is already in {name}.") });
}

/// The reference field: a reference in English, Russian or Ukrainian (whole names or short
/// forms), or words to find. The chapter follows what is typed a moment after typing stops;
/// Return picks the passage's first verse and gives the keyboard back, for Enter. While a
/// book's name is typed, the books it could be are offered under it.
fn bible_field(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, rect: Rect, live: Option<usize>) {
    let lookup_id = scripture_lookup_id();
    let entered = |response: &egui::Response, ui: &egui::Ui| {
        response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
    };
    // A letter or number typed with no field taking it (after Return has picked a verse and
    // given the keyboard back) starts the next reference here; the app keeps such keys from the
    // show in the Bible view (see `show_key_held_back`).
    let started = next_reference_typed(ui, state);
    if let Some(typed) = &started {
        state.bible.lookup = typed.clone();
        state.bible.typed_at = Some(ui.input(|input| input.time));
    }
    let language = state
        .bible
        .lead()
        .and_then(|at| state.bible.shelf.get(at).ok())
        .map(|bible| bible.info.language.clone())
        .unwrap_or_default();
    let field = egui::TextEdit::singleline(&mut state.bible.lookup)
        .hint_text("John 3:16 \u{b7} \u{41f}\u{441} 22 \u{b7} or words")
        .font(crate::studio::semibold(13.0))
        .align(Align2::LEFT_CENTER)
        .frame(Frame::new().fill(crate::studio::CARD).corner_radius(17).inner_margin(egui::Margin::symmetric(14, 0)))
        .min_size(rect.size())
        .desired_width(rect.width() - 28.0);
    // In a place of its own in the bar, which leaves the bar's room as it was laid out (the bar
    // may be two rows high).
    let mut place = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
    let completion = egui::CompletionPopup::new(lookup_id)
        .word_boundary(|_| false)
        .align(egui::RectAlign::BOTTOM_START)
        .max_height(240.0)
        .show(&mut place, field, |query| book_suggestions(query.text, &language));
    let lookup = completion.text_edit.response.response.clone().on_hover_text(format!(
        "Type a reference, such as John 3:16, or words to find. Return picks its first verse, Enter puts it on \
         screen; with nothing on screen the arrows pick another ({} comes here)",
        crate::keys::hint_key("2")
    ));
    if lookup.has_focus() {
        ui.painter().rect_stroke(rect, 17.0, Stroke::new(1.5, crate::studio::TEXT_FAINT), StrokeKind::Inside);
    }
    let now = ui.input(|input| input.time);
    if lookup.changed() && completion.accepted.is_none() {
        state.bible.typed_at = Some(now);
    }
    if entered(&lookup, ui) && lookup_words(&state.bible.lookup, &language).is_some() {
        // Words, not a reference: their verses are listed already (see `bible_live_search`);
        // the field keeps the keyboard.
        lookup.request_focus();
    } else if entered(&lookup, ui) {
        state.bible.look_up(live);
        // Its first verse is picked and the keyboard goes back to the show, as from the find
        // box: Enter puts the verse on screen; with nothing on screen the arrows pick another (with
        // something on screen they go on with it; see `bible_pick_keys`). A reference not understood
        // keeps the keyboard, its words selected, to be put right.
        if !state.pick_passage(show) {
            lookup.request_focus();
            select_all_text(ui.ctx(), lookup_id, &state.bible.lookup);
        }
    } else if let Some(at) = state.bible.typed_at {
        // A moment after typing stops, the page shows what is typed. Nothing goes on screen.
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
        // Drawn again at once, so the field shows it has the keyboard without waiting for the
        // mouse to move.
        ui.ctx().request_repaint();
    } else if started.is_some() {
        // The field takes the keyboard, the cursor after what was typed.
        lookup.request_focus();
        cursor_to_end(ui.ctx(), lookup_id, &state.bible.lookup);
        ui.ctx().request_repaint();
    }
    bible_live_search(ui.ctx(), state);
}

/// The translations line: which Bibles are on the slides, as small pills (each says where its
/// words and reference go), and Change, which opens them as rows: a Bible, the box its verse
/// goes in and the box its reference goes in, up to four, each with its licence and × to take
/// it off; Another translation, Manage Bibles… and Done under them. Whether they are open is
/// kept between starts.
fn bible_translations(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{CARD, Flat, PANEL, TEXT_DIM, cap, flat_button, flat_button_width};
    let width = ui.available_width();
    let small = FontId::proportional(11.5);
    let boxes = state.bible.boxes.clone();
    let labels = state.bible.box_labels.clone();
    let goes = |target: &BibleTarget| {
        target.as_ref().map_or("nowhere".to_owned(), |role| {
            boxes
                .iter()
                .position(|(have, _)| have == role)
                .and_then(|at| labels.get(at).cloned())
                .unwrap_or_else(|| role.label())
        })
    };
    if !state.bible.rows_open {
        let (line, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
        let middle = line.min.y + 12.0;
        let mut x = cap(ui, pos2(line.min.x, middle), Align2::LEFT_CENTER, "Translations", TEXT_DIM).max.x + 8.0;
        let mut open = false;
        for (at, row) in state.bible.rows.iter().enumerate() {
            let Some(bible) = row.bible.and_then(|at| state.bible.shelf.bibles.get(at)) else { continue };
            let galley = crate::studio::spaced(
                ui,
                &bible.short.to_uppercase(),
                crate::studio::fixed_semibold(11.0),
                TEXT_DIM,
                0.7,
            );
            let pill = Rect::from_min_size(pos2(x, middle - 10.5), vec2(galley.size().x + 18.0, 21.0));
            let response = ui.interact(pill, egui::Id::unique(("bible translation pill", at)), Sense::click());
            ui.painter().rect_filled(pill, 10.5, if response.hovered() { crate::studio::BUTTON } else { CARD });
            ui.painter().galley(pos2(pill.min.x + 9.0, middle - galley.size().y / 2.0), galley, TEXT_DIM);
            let hint = format!(
                "{}: its words go in {}, its reference in {}",
                bible.name,
                goes(&row.verse),
                goes(&row.reference)
            );
            if response.on_hover_text(hint).clicked() {
                open = true;
            }
            x = pill.max.x + 8.0;
        }
        let wide = flat_button_width(ui, &["Change"], &small, 9.0);
        let change = Rect::from_min_size(pos2(x, middle - 12.0), vec2(wide, 24.0));
        if flat_button(ui, change, egui::Id::unique("bible translations change"), "Change", small, Flat::Plain, true)
            .on_hover_text("Choose the Bibles shown and the boxes they go in")
            .clicked()
        {
            open = true;
        }
        if open {
            state.bible.rows_open = true;
        }
        return;
    }
    let mut add = false;
    let mut remove = None;
    let mut fold = false;
    let mut manage = false;
    let count = state.bible.rows.len();
    let used: Vec<(BibleTarget, BibleTarget)> =
        state.bible.rows.iter().map(|row| (row.verse.clone(), row.reference.clone())).collect();
    let bibles_used: Vec<usize> = state.bible.rows.iter().filter_map(|row| row.bible).collect();
    Frame::new()
        .fill(CARD)
        .corner_radius(11)
        .inner_margin(10)
        .outer_margin(egui::Margin { bottom: 8, ..Default::default() })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            ui.spacing_mut().interact_size.y = 30.0;
            // The menus are the panel's grey on the well.
            let widgets = &mut ui.visuals_mut().widgets;
            for widget in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.open] {
                widget.corner_radius = egui::CornerRadius::same(9);
            }
            widgets.inactive.weak_bg_fill = PANEL;
            widgets.inactive.bg_fill = PANEL;
            let inner = ui.available_width();
            let caps = |ui: &egui::Ui, text: &str| {
                crate::studio::spaced(ui, &text.to_uppercase(), crate::studio::fixed_semibold(11.5), TEXT_DIM, 0.6)
                    .size()
                    .x
            };
            let (verse_cap, reference_cap) = (caps(ui, "Verse in"), caps(ui, "Reference in"));
            let room = (inner - verse_cap - reference_cap - 26.0 - 28.0 - 8.0 * 6.0).max(240.0);
            let (bible_width, menu_width) = ((room * 0.42).floor(), (room * 0.29).floor());
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
                let column = |ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(vec2(width, 30.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_max_width(width);
                        add(ui);
                    });
                };
                let cap_at = |ui: &mut egui::Ui, text: &str, wide: f32| {
                    let (place, _) = ui.allocate_exact_size(vec2(wide, 30.0), Sense::hover());
                    cap(ui, place.left_center(), Align2::LEFT_CENTER, text, TEXT_DIM);
                };
                ui.horizontal(|ui| {
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
                    cap_at(ui, "Verse in", verse_cap);
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
                    cap_at(ui, "Reference in", reference_cap);
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
                    bible_row_extras(ui, index, count, shelved, &mut remove);
                });
            }
            // Under the rows: another translation (up to four), the Bibles installed, and Done.
            let (foot, _) = ui.allocate_exact_size(vec2(inner, 24.0), Sense::hover());
            let middle = foot.center().y;
            let mut x = foot.min.x;
            if count < BiblePage::MAX_ROWS {
                let words = "+ Another translation";
                let wide = flat_button_width(ui, &[words], &small, 9.0);
                let rect = Rect::from_min_size(pos2(x, middle - 12.0), vec2(wide, 24.0));
                if flat_button(ui, rect, egui::Id::unique("bible row add"), words, small.clone(), Flat::Plain, true)
                    .on_hover_text("Show another Bible on the slides")
                    .clicked()
                {
                    add = true;
                }
                x = rect.max.x + 8.0;
            } else {
                let said =
                    ui.painter().text(pos2(x, middle), Align2::LEFT_CENTER, "Four is the most", small.clone(), DIM);
                x = said.max.x + 8.0;
            }
            let words = "Manage Bibles\u{2026}";
            let wide = flat_button_width(ui, &[words], &small, 9.0);
            let rect = Rect::from_min_size(pos2(x, middle - 12.0), vec2(wide, 24.0));
            if flat_button(ui, rect, egui::Id::unique("bible manage"), words, small.clone(), Flat::Plain, true)
                .on_hover_text("Install and remove Bibles")
                .clicked()
            {
                manage = true;
            }
            let wide = flat_button_width(ui, &["Done"], &small, 9.0);
            let rect = Rect::from_min_size(pos2(foot.max.x - wide, middle - 12.0), vec2(wide, 24.0));
            if flat_button(ui, rect, egui::Id::unique("bible rows done"), "Done", small.clone(), Flat::Main, true)
                .on_hover_text("Fold the translations away into one line")
                .clicked()
            {
                fold = true;
            }
        });
    if fold {
        state.bible.rows_open = false;
    }
    if add {
        state.bible.add_row();
    }
    if manage {
        state.bible.options.manage_open = true;
    }
    if let Some(index) = remove {
        state.bible.remove_row(index);
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
    let mut slides: Vec<&sp_model::Slide> = passage.slides().map(|(_, slide)| slide).collect();
    // The passage's own slides, when they are among its chapter's.
    let own = page.passage_slides();
    if !sample && own.end <= slides.len() && !own.is_empty() {
        slides = slides[own].to_vec();
    }
    let mut picture = |slide: Option<&&sp_model::Slide>| {
        slide.and_then(|slide| thumb(ThumbOf::BibleSlide(slide))).map(|(texture, _)| texture)
    };
    let first = picture(slides.first());
    let last = if slides.len() > 1 { picture(slides.last()) } else { None };
    let name = if sample { passage.title.clone() } else { page.shown.clone() };
    crate::bible::SlidesPreview { first, last, count: slides.len(), name, sample, translations }
}

/// The right end of a translation's row: its licence (a small (i) that opens the Bible's name,
/// short name and terms) and × to take the translation off (one always stays).
fn bible_row_extras(
    ui: &mut egui::Ui,
    index: usize,
    count: usize,
    shelved: Option<&crate::bible::Shelved>,
    remove: &mut Option<usize>,
) {
    let (mark, licence) = ui.allocate_exact_size(vec2(26.0, 30.0), Sense::click());
    let ink = if licence.hovered() { Color32::WHITE } else { DIM };
    ui.painter().circle_stroke(mark.center(), 8.0, Stroke::new(1.2, ink));
    ui.painter().text(mark.center(), Align2::CENTER_CENTER, "i", crate::studio::semibold(11.5), ink);
    let licence = licence.on_hover_text("Licence: who holds the rights to this translation, and its terms");
    egui::Popup::from_toggle_button_response(&licence)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(300.0);
            if let Some(bible) = shelved {
                egui::Grid::new(("licence", index)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
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
    let (place, _) = ui.allocate_exact_size(vec2(28.0, 30.0), Sense::hover());
    let cell = Rect::from_center_size(place.center(), vec2(28.0, 28.0));
    if count > 1 {
        let id = egui::Id::unique(("bible row remove", index));
        if crate::studio::icon_button(ui, cell, id, Icon::Close, 1.0, false)
            .on_hover_text("Take this translation off")
            .clicked()
        {
            *remove = Some(index);
        }
    } else {
        let glyph = Rect::from_center_size(cell.center(), vec2(14.0, 14.0));
        icons::paint(ui.painter(), Icon::Close, glyph, crate::studio::TEXT_FAINT.gamma_multiply(0.5));
    }
}

/// The rulers along the top and left of the Edit view's canvas, in the slide's own pixels: a
/// mark every 100, numbered as often as there is room for, with the selected object's extent
/// shown on each.
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

/// The passage the Options panel previews when none is showing: two verses, so that one verse
/// a slide, and the reference on the last slide only, can be seen to change it.
const SAMPLE_PASSAGE: &str = "John 3:16-17";

/// How many passages the Bible history keeps.
const HISTORY_LENGTH: usize = 100;

/// When a passage in the history was on screen: the time for today ("10:42 AM"), the day
/// before that ("Sun 17 Nov").
pub(crate) fn history_when(at: &str, today: chrono::NaiveDate) -> String {
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

/// What was typed this frame in the Bible view that starts a new reference: letters or numbers
/// (not `?`, which opens the list of keys, nor `/`, the find box) with no field taking the
/// keyboard and nothing open over the view.
fn next_reference_typed(ui: &egui::Ui, state: &HomeState) -> Option<String> {
    let free = !ui.ctx().text_edit_focused()
        && state.library_search.is_none()
        && !state.key_sheet
        && state.numbers.is_none()
        && !state.alert_open()
        && !state.bible.options.open
        && !state.bible.options.manage_open
        && state.bible.options.details.is_empty()
        && !egui::Popup::is_any_open(ui.ctx());
    if !free {
        return None;
    }
    let typed: String = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    });
    typed.chars().next().filter(|first| first.is_alphanumeric()).map(|_| typed)
}

/// Puts the cursor of a text field after its last letter.
fn cursor_to_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    let mut edit = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    edit.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(text.chars().count()))));
    edit.store(ctx, id);
}

/// What is typed in Scripture Lookup when it is words to search for rather than a reference: at
/// least three letters, no chapter number, and no book's name begun (`language` is the first
/// translation's, which book names are read in first).
fn lookup_words<'a>(typed: &'a str, language: &str) -> Option<&'a str> {
    let trimmed = typed.trim();
    // A book's name, whole or begun, in any of the languages read, or a short form of one.
    let book = !sp_bible::suggest_books(trimmed, language, 1).is_empty()
        || sp_bible::read_reference(&format!("{trimmed} 1"), language).is_ok();
    (trimmed.chars().count() >= 3 && !has_chapter(trimmed) && !trimmed.contains(':') && !book).then_some(trimmed)
}

/// The Bible view's search as it is typed: the verses holding the words typed in the reference
/// field, when they are words, not a reference, are listed in place of the page as soon as they
/// are found, on a thread of their own (only the newest words are answered). When the words go
/// (cleared, or become a reference), so does the list they gave.
fn bible_live_search(ctx: &egui::Context, state: &mut HomeState) {
    for (id, bible) in state.page_finder.take_read() {
        state.bible.shelf.keep(&id, bible);
    }
    let page = &state.bible;
    let language =
        page.lead().and_then(|at| page.shelf.bibles.get(at)).map_or_else(String::new, |b| b.language.clone());
    let Some(words) = lookup_words(&page.lookup, &language).map(str::to_owned) else {
        if state.bible.live_words.take().is_some() {
            state.bible.found.clear();
            state.bible.message = None;
        }
        return;
    };
    if state.bible.live_words.as_deref() == Some(words.as_str()) {
        return;
    }
    let reading = state.bible.reading();
    let (shelf, rows) = (&state.bible.shelf, &state.bible.rows);
    let in_use: Vec<String> = rows.iter().filter_map(|row| Some(shelf.bibles.get(row.bible?)?.id.clone())).collect();
    let readers = || rows.iter().filter_map(|row| shelf.reader(row.bible?)).collect();
    if let Some(found) = state.page_finder.answer(&words, &in_use, &reading, readers, ctx).cloned() {
        state.bible.message = None;
        state.bible.show_found(found);
        state.bible.live_words = Some(words);
    }
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

/// How rounded a slide's corners are in the slide grid, in points.
const SLIDE_ROUND: u8 = 3;

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

/// While slides are dragged near the top or bottom of a scrolling list of them, the list
/// scrolls that way, faster nearer the edge. Its caller knows slides are being dragged.
fn scroll_while_dragging(ui: &mut egui::Ui) {
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

/// Carries on a drag in one of the Edit view's lists (`rows`, top to bottom, as drawn this
/// frame), as the slides' list has it: while one is being dragged (`held.0`), the ones moving
/// (by place) go faint, an ink line shows where they would go, `words` follow the pointer, and
/// the list scrolls at its ends. The list is the rows and the room either side of them, to the
/// column's edges, as much of it as shows (not the keys above it or what is under it). Returns
/// the pointer's height when the one dragged was let go over the list (`held.1`); let go
/// elsewhere, or stopped with Escape, nothing moves.
pub(crate) fn list_drag(
    ui: &mut egui::Ui,
    rows: &[Rect],
    held: (Option<usize>, Option<usize>),
    moving: &[usize],
    words: &str,
) -> Option<f32> {
    let (dragging, dropped) = held;
    let across = rows.first().map(|row| row.x_range().expand(10.0));
    let down = ui.clip_rect().y_range();
    let pointer = ui
        .input(|input| input.pointer.interact_pos())
        .filter(|pointer| across.is_some_and(|across| across.contains(pointer.x)) && down.contains(pointer.y));
    if dragging.is_some() {
        if let Some(pointer) = pointer {
            let slot = crate::studio::drop_slot(rows, pointer.y);
            crate::studio::paint_rows_moving(ui.painter(), rows, |at| moving.contains(&at), slot, crate::studio::PANEL);
        }
        crate::studio::paint_carried(ui.ctx(), words.trim());
        scroll_while_dragging(ui);
    }
    dropped.and(pointer).map(|pointer| pointer.y)
}

/// Where one of a list's rows dragged from `from` and let go with the pointer at `y` goes: the
/// place of the ink line, in the list without it. `None` when that is where it was.
pub(crate) fn dragged_to(rows: &[Rect], from: usize, y: f32) -> Option<usize> {
    let slot = crate::studio::drop_slot(rows, y);
    let to = if slot > from { slot - 1 } else { slot };
    (to != from && to < rows.len()).then_some(to)
}

/// The Delete key pressed with nothing picked on the canvas, as the list's own Delete key, when
/// that can be pressed (`can`, as [`crate::studio::list_keys`] takes it).
pub(crate) fn list_delete_key(delete: bool, can: [bool; 5]) -> Option<crate::studio::ListKey> {
    (delete && can[2]).then_some(crate::studio::ListKey::Delete)
}

/// Where the one at `at` in a list is once the one at `from` has moved to `to`.
pub(crate) fn place_after_move(at: usize, from: usize, to: usize) -> usize {
    if at == from {
        to
    } else if from < at && at <= to {
        at - 1
    } else if to <= at && at < from {
        at + 1
    } else {
        at
    }
}

/// The colour of the line that shows where dragged things will go, and of the outline round
/// what they are dropped onto: ink, never the on-screen orange.
const DROP_LINE: Color32 = crate::studio::TEXT;

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
    let stroke = Stroke::new(2.0, DROP_LINE);
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
            Color32::from_rgb(0x11, 0x11, 0x11),
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
    fn removing_a_bible_no_row_shows_keeps_every_translations_boxes() {
        let role = |name: &str| Some(sp_model::Role(name.to_owned()));
        let mut page = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        // Two Bibles installed from files that are not there (so removing one moves nothing to
        // the Trash): Extra, then Other.
        let bytes = include_bytes!("../../../assets/bibles/web.spbible");
        for id in ["extra", "other"] {
            let mut info = sp_bible::Bible::read_info(bytes).unwrap();
            info.id = id.to_owned();
            page.shelf.add_file(format!("/nonexistent/softpresenter-test/{id}.spbible").into(), info);
        }
        let (kjv, extra, other) = (
            page.shelf.position("kjv").unwrap(),
            page.shelf.position("extra").unwrap(),
            page.shelf.position("other").unwrap(),
        );
        // Not the usual boxes: the KJV in the translation box, Other in the main box.
        page.rows = vec![
            BibleRow { bible: Some(kjv), verse: role("translation"), reference: None },
            BibleRow { bible: Some(other), verse: role("primary"), reference: role("reference") },
        ];
        let rows = page.rows.clone();
        // Extra, which no row shows, goes: the rows keep their Bibles and their boxes.
        page.remove_bible(extra);
        assert!(page.options.problem.is_none(), "{:?}", page.options.problem);
        assert_eq!(page.shelf.position("other"), Some(other - 1));
        assert_eq!(page.rows[0], rows[0]);
        assert_eq!(page.rows[1], BibleRow { bible: Some(other - 1), ..rows[1].clone() });
        // Other, which a row shows, goes: that row goes and the boxes are given out again.
        page.remove_bible(other - 1);
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].bible, Some(kjv));
        assert_eq!(page.rows[0].verse, Some(sp_model::Role::primary()), "the first translation in the main box");
    }

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
            numbering: None,
            rows: vec![crate::service::SavedRow {
                bible: "kjv".to_owned(),
                verse: role("primary").map(|r| r.0),
                reference: role("reference").map(|r| r.0),
            }],
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: "passage_last".to_owned(),
            show_with: None,
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
        assert_eq!(state.bible.shown, "От Иоанна 3:16", "made in the first translation (Russian Synodal)");
        assert_eq!(passage.title, "От Иоанна 3:1-36", "its slides its chapter's, a slide a verse");
        assert_eq!(state.bible.lookup, "jn 3:16");
        assert_eq!(state.bible.message, None);
    }

    #[test]
    fn the_history_keeps_each_passage_once_newest_first() {
        let at = |h: u32| chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap().and_hms_opt(h, 0, 0).unwrap();
        let mut page = BiblePage::default();
        page.note_shown("John 3:16", None, at(9));
        page.note_shown("John 3:16", None, at(10));
        page.note_shown("Psalm 23", None, at(11));
        page.note_shown("John 3:16", None, at(12));
        let passages: Vec<&str> = page.history.iter().map(|e| e.passage.as_str()).collect();
        assert_eq!(passages, ["John 3:16", "Psalm 23"]);
        assert_eq!(page.history[0].at, "2026-11-19T12:00:00", "moved up, with its new time");
        assert!(page.history_changed);
        for n in 0..150 {
            page.note_shown(&format!("Psalm {n}"), None, at(13));
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

    /// A passage's slides as the screens get them: every box's role and words, each slide's
    /// template and bar, the title; without the ids made afresh each time.
    fn slides_as_shown(passage: &sp_model::Presentation) -> serde_json::Value {
        fn without_ids(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("id");
                    map.values_mut().for_each(without_ids);
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(without_ids),
                _ => {}
            }
        }
        let mut value = serde_json::to_value(passage).expect("a passage is written as JSON");
        without_ids(&mut value);
        value
    }

    /// The Bible page as the app starts (the Russian Synodal first, the King James Version
    /// second), ready to make passages.
    fn bible_page() -> BiblePage {
        BiblePage { slots: Some([0, 1]), ..BiblePage::default() }
    }

    /// Looks a reference up as if typed and Return pressed: what the field then says, the
    /// session's language, and the slides made.
    fn typed(page: &mut BiblePage, reference: &str) -> (String, String, serde_json::Value) {
        page.lookup = reference.to_owned();
        page.look_up(None);
        let (_, passage) = page.pending.take().unwrap_or_else(|| panic!("{reference} is made: {:?}", page.message));
        (page.lookup.clone(), page.session_language(), slides_as_shown(&passage))
    }

    /// Return pressed again on what the field says.
    fn retyped(page: &mut BiblePage) -> (String, String, serde_json::Value) {
        let again = page.lookup.clone();
        typed(page, &again)
    }

    #[test]
    fn the_operator_reads_what_was_typed_in_its_language_and_the_screens_get_the_same_slides() {
        for (english, russian, ukrainian, names) in [
            ("gen 1 1", "Быт 1 1", "Бут 1 1", ["Genesis 1:1", "Бытие 1:1"]),
            ("John 3:16-18", "Ин 3:16-18", "Ів 3:16-18", ["John 3:16-18", "От Иоанна 3:16-18"]),
            // The numbers follow the language typed: the English 23rd psalm is the Synodal 22nd
            // (and Ukrainian, with no Ukrainian Bible in use, the first Bible's numbering).
            ("ps 23", "Пс 22", "Псалом 22", ["Psalm 23:1-6", "Псалом 22:1-6"]),
        ] {
            let mut page = bible_page();
            let (in_english, english_session, english_slides) = typed(&mut page, english);
            let (in_russian, russian_session, russian_slides) = typed(&mut page, russian);
            let (in_ukrainian, ukrainian_session, ukrainian_slides) = typed(&mut page, ukrainian);
            assert_eq!((in_english.as_str(), english_session.as_str()), (names[0], "en"), "{english}");
            assert_eq!((in_russian.as_str(), russian_session.as_str()), (names[1], "ru"), "{russian}");
            // Ukrainian, with no Ukrainian Bible in use: the first Bible's language, and the
            // session goes on in Ukrainian ("Псалом" is Ukrainian and a Russian short form alike:
            // the Russian Bible, the only one of the two).
            assert_eq!(in_ukrainian, names[1], "{ukrainian}");
            assert!(["uk", "ru"].contains(&ukrainian_session.as_str()), "{ukrainian}: {ukrainian_session}");
            // Rule 2: the same slides, whatever the language typed.
            assert_eq!(english_slides, russian_slides, "{english} and {russian}");
            assert_eq!(russian_slides, ukrainian_slides, "{russian} and {ukrainian}");
        }
    }

    #[test]
    fn a_name_written_back_reads_again_as_the_same_passage() {
        // With the Russian Synodal first, the numbers still follow the language typed: "Psalm 23"
        // is the English 23rd psalm, the Synodal 22nd, named back in English as typed.
        let mut page = bible_page();
        let (field, _, slides) = typed(&mut page, "Psalm 23");
        assert_eq!(field, "Psalm 23:1-6");
        let shown = page.shown.clone();
        assert_eq!(shown, "Псалтирь 22:1-6");
        // Return again on what the field says: the same passage, again and again.
        for _ in 0..3 {
            let (again, session, same) = retyped(&mut page);
            assert_eq!((again.as_str(), session.as_str(), &same), (field.as_str(), "en", &slides));
            assert_eq!(page.shown, shown);
        }
        // A verse more, then Return on what the field says.
        page.step(true, None);
        page.pending = None;
        assert_eq!(page.lookup, "Psalms 23:1-24:1");
        let (stepped, _, _) = retyped(&mut page);
        assert_eq!((stepped.as_str(), page.shown.as_str()), ("Psalms 23:1-24:1", "Псалтирь 22:1-23:1"));
        // As the first translation names it (the lists, the page): named for the operator in the
        // session's language with its numbering, and Return on it is the same passage.
        page.open_in(shown.clone(), None, None);
        let (_, passage) = page.pending.take().expect("made");
        assert_eq!((page.lookup.as_str(), slides_as_shown(&passage)), (field.as_str(), slides.clone()));
        let (again, _, same) = retyped(&mut page);
        assert_eq!((again, same), (field.clone(), slides.clone()));
        // A playlist keeps it as typed, with the English numbering beside it, and it opens the
        // same, here and with the King James Version first.
        let saved = page.saved().expect("a passage is showing");
        assert_eq!((saved.lookup.as_str(), saved.numbering.as_deref()), ("Psalms 23:1-6", Some("english")));
        page.open_saved(&saved, None, SlidesOf::Page);
        assert_eq!((page.lookup.as_str(), page.shown.as_str()), (field.as_str(), shown.as_str()));
        // New choices make it again, still named in English, still kept as typed.
        page.options.show_verse_numbers = true;
        page.remake(None);
        assert_eq!((page.lookup.as_str(), page.session_language().as_str()), (field.as_str(), "en"));
        assert_eq!(page.saved().map(|saved| saved.lookup).as_deref(), Some("Psalms 23:1-6"));
    }

    #[test]
    fn the_numbers_follow_the_language_typed_whichever_bible_is_first() {
        // The owner's rule (2026-11-22): typed in English, the English numbering; in Russian, the
        // Synodal's; whichever Bible is listed first. The Synodal first, the King James second.
        let blessed = "Blessed are the undefiled in the way";
        let english_words = |passage: &sp_model::Presentation, at: usize| {
            let (_, slide) = passage.slide(at).expect("a slide");
            slide.texts().into_iter().map(|(_, text)| text.to_owned()).collect::<Vec<_>>().join(" | ")
        };
        let mut page = bible_page();
        page.lookup = "Psalm 119:1".to_owned();
        page.look_up(None);
        let (_, passage) = page.pending.take().expect("made");
        let first = page.passage_slides().start;
        assert!(english_words(&passage, first).contains(blessed), "{}", english_words(&passage, first));
        assert_eq!(page.shown, "Псалтирь 118:1", "the Synodal's 118th");
        assert_eq!(page.lookup, "Psalm 119:1", "named in English with the English numbers");
        assert_eq!(crate::bible_page::title(&mut page).map(|(words, _)| words).as_deref(), Some("Psalm 119:1"));
        let english = page.saved().expect("kept");
        assert_eq!((english.lookup.as_str(), english.numbering.as_deref()), ("Psalms 119:1", Some("english")));
        // Typed in Russian: the Synodal's numbers, the same verse.
        page.lookup = "Пс 118:1".to_owned();
        page.look_up(None);
        let (_, same) = page.pending.take().expect("made");
        assert_eq!(slides_as_shown(&same), slides_as_shown(&passage));
        assert_eq!((page.lookup.as_str(), page.shown.as_str()), ("Псалом 118:1", "Псалтирь 118:1"));
        // A whole psalm typed in English: the English 23rd, titled "Psalm 23" over the Synodal
        // 22nd's slides; and with Russian chosen, the same verses titled "Псалом 22".
        page.lookup = "Psalm 23".to_owned();
        page.look_up(None);
        page.pending = None;
        assert_eq!(page.shown, "Псалтирь 22:1-6");
        assert_eq!(crate::bible_page::title(&mut page).map(|(words, _)| words).as_deref(), Some("Psalm 23"));
        page.set_read_in(Some("ru".to_owned()));
        assert_eq!(crate::bible_page::title(&mut page).map(|(words, _)| words).as_deref(), Some("Псалом 22"));
        page.lookup = "Psalm 23".to_owned();
        page.look_up(None);
        page.pending = None;
        assert_eq!((page.lookup.as_str(), page.shown.as_str()), ("Псалом 22:1-6", "Псалтирь 22:1-6"));
        assert_eq!(page.saved().map(|saved| saved.lookup).as_deref(), Some("Psalms 23:1-6"), "kept as typed");
        // The playlist knows its passage is on screen by the place's record, though the slides'
        // title is the Synodal's.
        let slot = page.slots.map(|slots| slots[page.showing]).unwrap();
        assert!(page.holds_kept(slot, "Псалтирь 22:1-6", "Psalms 23:1-6"));
        assert!(page.holds_kept(slot, "Псалтирь 22:1-6", "Псалтирь 22:1-6"));
        assert!(!page.holds_kept(slot, "Псалтирь 22:1-6", "Psalms 24:1-10"));
        assert!(!page.holds_kept(slot, "Псалтирь 23:1-10", "Psalms 23:1-6"), "another passage there now");
        page.set_read_in(None);
        // A psalm the traditions split: the English 116th is the Synodal 114th and 115th.
        page.lookup = "Psalm 116".to_owned();
        page.look_up(None);
        page.pending = None;
        assert_eq!(page.shown, "Псалтирь 114:1-115:10");
        assert_eq!(crate::bible_page::title(&mut page).map(|(words, _)| words).as_deref(), Some("Psalm 116"));
        assert_eq!(page.passage_slides(), 0..19, "across two chapters: its own slides");

        // The English passage kept in a playlist opens as the same verses with its Bibles in the
        // other order (the King James first), whatever the language chosen; and from the
        // history, on a page with the Bibles that way round.
        let mut swapped = english.clone();
        swapped.rows.swap(0, 1);
        let mut reordered = bible_page();
        for chosen in [None, Some("ru"), Some("en")] {
            reordered.set_read_in(chosen.map(str::to_owned));
            reordered.open_saved(&swapped, None, SlidesOf::Page);
            let (_, again) = reordered.pending.take().expect("made");
            let first = reordered.passage_slides().start;
            assert!(english_words(&again, first).contains(blessed), "{chosen:?}: {}", english_words(&again, first));
            assert_eq!(reordered.shown, "Psalms 119:1", "{chosen:?}: as the King James numbers it");
            assert_eq!(reordered.saved().map(|saved| saved.lookup).as_deref(), Some("Psalms 119:1"));
        }
        let mut page = bible_page();
        page.open_kept("Psalms 119:1".to_owned(), Some(sp_bible::Numbering::English), None);
        assert_eq!(page.shown, "Псалтирь 118:1", "the Synodal first: its 118th");
        reordered.open_kept("Psalms 119:1".to_owned(), Some(sp_bible::Numbering::English), None);
        assert_eq!(reordered.shown, "Psalms 119:1", "the King James first: its 119th");
        // A passage kept before numberings were kept opens in the first translation's, as it did.
        let old = crate::service::SavedPassage { lookup: "Psalms 119:1".to_owned(), numbering: None, ..english };
        let mut page = bible_page();
        page.open_saved(&old, None, SlidesOf::Page);
        assert_eq!(page.shown, "Псалтирь 119:1", "read in the Synodal's numbering, as before");
    }

    #[test]
    fn with_nothing_typed_the_session_starts_in_the_computers_language() {
        let page = |tag: Option<&str>| BiblePage {
            system_language: tag.map(crate::system_language::SystemLanguage::answered).unwrap_or_default(),
            ..bible_page()
        };
        // The lists name the books, and a passage clicked is named, in the computer's language.
        assert_eq!(
            (page(Some("ru-RU")).session_language(), page(Some("ru-RU")).names_language()),
            ("ru".into(), "ru".into())
        );
        assert_eq!(
            (page(Some("en-US")).session_language(), page(Some("en-US")).names_language()),
            ("en".into(), "en".into())
        );
        assert_eq!(page(Some("de-DE")).names_language(), "en", "any other language: English");
        assert_eq!(page(None).names_language(), "en", "not read (yet): English");
        // Ukrainian with no Ukrainian Bible in use: the first Bible's language.
        let ukrainian = page(Some("uk-UA"));
        assert_eq!((ukrainian.session_language(), ukrainian.names_language()), ("uk".into(), "ru".into()));
        // A verse clicked in the lists on a computer set to Russian is named in Russian.
        let mut russian = page(Some("ru"));
        russian.open_in("John 3:16".to_owned(), None, None);
        assert_eq!(russian.lookup, "От Иоанна 3:16");
        // Once a reference is typed, its language is the session's, whatever the computer's.
        typed(&mut russian, "gen 1 1");
        assert_eq!(russian.names_language(), "en");
        russian.open_in("Бытие 1:2".to_owned(), None, None);
        assert_eq!(russian.lookup, "Genesis 1:2");
        // The language a reference would be named back in, for `sp bible`'s replies.
        assert_eq!(russian.typed_language("Быт 99"), "ru");
        assert_eq!(russian.typed_language("Бут 99"), "ru", "no Ukrainian Bible: the first Bible's");
        assert_eq!(russian.typed_language("gen 99"), "en");
    }

    #[test]
    fn a_language_with_no_bible_in_use_falls_back_to_the_first_bible() {
        let only = |id: &str| {
            let mut page = bible_page();
            let at = page.shelf.position(id).expect("shipped");
            page.rows = vec![BibleRow { bible: Some(at), verse: Some(sp_model::Role::primary()), reference: None }];
            page
        };
        let mut english = only("kjv");
        assert_eq!(typed(&mut english, "Быт 1 1").0, "Genesis 1:1");
        assert_eq!(english.session_language(), "ru", "the session goes on in the language typed");
        assert_eq!(english.names_language(), "en");
        let mut russian = only("syn");
        assert_eq!(typed(&mut russian, "gen 1 1").0, "Бытие 1:1");
        // A row set to none is passed over: the next Bible is the first.
        let mut skipped = bible_page();
        skipped.rows[0].bible = None;
        assert_eq!(typed(&mut skipped, "Быт 1 1").0, "Genesis 1:1");
    }

    #[test]
    fn search_looks_in_the_bibles_of_the_words_alphabet_and_opens_as_the_first_bible_numbers() {
        let mut page = bible_page();
        page.search = "valley of the shadow".to_owned();
        page.find();
        assert!(!page.found.is_empty() && page.found.iter().all(|found| found.bible == "KJV"), "{:?}", page.found);
        let psalm =
            page.found.iter().find(|found| found.words.contains("shadow of death")).cloned().expect("Psalm 23:4");
        // Named in English with the Synodal numbers the slides follow.
        assert_eq!((psalm.reference.as_str(), psalm.kept.as_str()), ("Psalm 23:4", "Псалтирь 22:4"));
        assert_eq!((psalm.chapter.as_str(), psalm.kept_chapter.as_str()), ("Psalm 23", "Псалтирь 22"));
        page.open_found(&psalm, false, None);
        let (_, passage) = page.pending.take().expect("made");
        assert_eq!(page.shown, "Псалтирь 22:4", "the verse is the passage");
        let verse = page.passage_slides().start;
        assert_eq!(verse, 3, "among its chapter's slides");
        let english = passage.slide(verse).and_then(|(_, slide)| slide.text_for(&sp_model::Role::translation()));
        assert!(english.is_some_and(|words| words.contains("shadow of death")));
        assert_eq!((page.lookup.as_str(), page.session_language().as_str()), ("Psalm 23:4", "en"));
        // Russian words, in the Russian Bible.
        page.search = "долиною смертной тени".to_owned();
        page.find();
        assert!(!page.found.is_empty() && page.found.iter().all(|found| found.bible == "SYN"));
        // Named as its own Bible reads it ("Псалом 22:4" for what is kept as "Псалтирь 22:4"): the
        // same verse.
        let read = |name: &str| sp_bible::parse_reference_in(name, "ru").ok();
        assert_eq!(read(&page.found[0].reference), read(&page.found[0].kept));
        // No Bible in use in that alphabet: the first Bible, as before.
        page.search = "λόγος".to_owned();
        page.find();
        assert!(page.found.is_empty());
        assert_eq!(page.message.as_deref(), Some("Nothing in the Синодальный перевод has those words."));
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

    /// A click on a row of the Bible view's book, chapter or verse lists, then a frame for the
    /// app to take what it looked up. Returns the commands the frames gave.
    fn click_list_row(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        row: impl std::hash::Hash + std::fmt::Debug,
        time: f64,
    ) -> Vec<Command> {
        let id = crate::bible::list_row_id(row);
        // Out of view (the keypad's verses under a long book's chapters): the column is
        // scrolled down to it, as the operator would, until the scrolling has settled.
        let mut clock = time - 0.049;
        let mut tick = || {
            clock += 0.0001;
            clock
        };
        for _ in 0..40 {
            let whole = |row: egui::Response| row.interact_rect.height() >= row.rect.height() - 0.5;
            if ctx.read_response(id).is_some_and(whole) {
                break;
            }
            let column = ctx.read_response(crate::bible::list_row_id("hide")).expect("the column is drawn").rect;
            let over = pos2(column.max.x - 100.0, column.max.y + 300.0);
            let wheel = Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -120.0),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            frame_then_app(ctx, show, state, vec![Event::PointerMoved(over), wheel], tick());
            for _ in 0..5 {
                frame_then_app(ctx, show, state, vec![], tick());
            }
        }
        let at = ctx.read_response(id).expect("the row is drawn").rect.center();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut commands = frame_then_app(ctx, show, state, vec![Event::PointerMoved(at)], time);
        commands.extend(frame_then_app(ctx, show, state, vec![press(true)], time + 0.05));
        commands.extend(frame_then_app(ctx, show, state, vec![press(false)], time + 0.1));
        commands.extend(frame_then_app(ctx, show, state, vec![], time + 0.15));
        commands
    }

    #[test]
    fn a_book_click_loads_its_first_chapter_a_chapter_click_that_chapter_and_a_verse_click_that_verse() {
        // The Russian Synodal first, the King James Version second, as the app starts.
        // On the keypad and in the lists alike.
        for (computer, genesis_1, genesis_2, verse, books) in [
            (None, "Genesis 1:1-31", "Genesis 2:1-25", "Genesis 2:3", BookPicker::Keypad),
            (Some("ru-RU"), "Бытие 1:1-31", "Бытие 2:1-25", "Бытие 2:3", BookPicker::Keypad),
            (None, "Genesis 1:1-31", "Genesis 2:1-25", "Genesis 2:3", BookPicker::Lists),
            (Some("ru-RU"), "Бытие 1:1-31", "Бытие 2:1-25", "Бытие 2:3", BookPicker::Lists),
        ] {
            let ctx = egui::Context::default();
            let mut show = service_with_bible_places();
            let mut state = HomeState { bible_open: true, ..HomeState::default() };
            state.bible.slots = Some([2, 3]);
            state.bible.books = books;
            if let Some(computer) = computer {
                state.bible.system_language = crate::system_language::SystemLanguage::answered(computer);
            }
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.05);
            let picked_first = |state: &HomeState, show: &Show| {
                let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
                state.pick(show) == Some(SlideRef { presentation: slot, slide: 0 })
            };
            // A book: its first chapter, the whole chapter the passage (so Add keeps it, named as
            // the operator reads it), first verse picked.
            let mut commands = click_list_row(&ctx, &mut show, &mut state, ("book", 0usize), 0.1);
            assert_eq!(state.bible.lookup, genesis_1, "{computer:?}");
            assert_eq!(state.bible.shown, "Бытие 1:1-31", "made as the first translation names it");
            assert_eq!(state.bible.saved().map(|saved| saved.lookup).as_deref(), Some(genesis_1));
            assert!(picked_first(&state, &show), "{computer:?}: the first verse picked");
            // A chapter, with one click: that chapter, all of it the passage.
            commands.extend(click_list_row(&ctx, &mut show, &mut state, ("chapter", 2u16), 0.5));
            assert_eq!(state.bible.lookup, genesis_2, "{computer:?}");
            assert_eq!(state.bible.saved().map(|saved| saved.lookup).as_deref(), Some(genesis_2));
            assert_eq!(state.bible.passage_slides(), 0..25);
            assert!(picked_first(&state, &show));
            // A verse of it: that verse alone is the passage, its slides the chapter's, the
            // verse picked among them.
            commands.extend(click_list_row(&ctx, &mut show, &mut state, ("verse", 3u16), 1.0));
            assert_eq!(state.bible.lookup, verse, "{computer:?}");
            assert_eq!(state.bible.saved().map(|saved| saved.lookup).as_deref(), Some(verse));
            let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
            assert_eq!(show.presentations()[slot].slide_count(), 25);
            assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 2 }), "{computer:?}");
            // Nothing went on screen; Enter puts the verse on.
            assert!(!goes_on_screen(&commands), "{commands:?}");
            assert!(state.bible.go_live.is_none());
            assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "the song stays on screen");
            press_enter(&mut show, &state);
            assert_eq!(show.live(), Some(SlideRef { presentation: slot, slide: 2 }));
        }
    }

    #[test]
    fn words_typed_in_search_or_scripture_lookup_list_their_verses_without_return() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        // In the reference field (the page's one field; it took the Search field's place):
        // English words, found in the King James Version as they are typed.
        state.bible.focus_lookup = true;
        let mut commands = frame_then_app(&ctx, &mut show, &mut state, vec![], 0.1);
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("valley of".to_owned())], 0.2));
        assert!(!state.bible.found.is_empty(), "listed without Return");
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text(" the shadow".to_owned())], 0.3));
        assert!(state.bible.found.iter().all(|found| found.bible == "KJV"), "{:?}", state.bible.found);
        assert!(state.bible.found[0].words.to_lowercase().contains("valley of the shadow"), "the newest words");
        // Emptied: the list goes, and the page comes back.
        state.bible.lookup.clear();
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.4);
        assert!(state.bible.found.is_empty());
        // `sp bible search` still lists verses for words without the field.
        state.bible.search = "valley of the shadow".to_owned();
        state.bible.find();
        assert!(state.bible.found.iter().all(|found| found.bible == "KJV") && !state.bible.found.is_empty());
        state.bible.found.clear();
        // In Scripture Lookup: Russian words that are no reference and no book's name.
        state.bible.focus_lookup = true;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.5);
        commands.extend(frame_then_app(
            &ctx,
            &mut show,
            &mut state,
            vec![Event::Text("долиною смертной".to_owned())],
            0.6,
        ));
        assert!(!state.bible.found.is_empty(), "listed without Return");
        assert!(state.bible.found.iter().all(|found| found.bible == "SYN"));
        // The words become a reference: the list gives way to the passage, a moment later.
        state.bible.focus_lookup = true;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.7);
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("jn 3:16".to_owned())], 0.8));
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 1.5));
        assert!(state.bible.found.is_empty());
        assert_eq!(state.bible.shown, "От Иоанна 3:16");
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }));
        // A book's name being typed, or a short form of one, is not searched for.
        for book in ["Быт", "John", "1 Цар", "Флп", "Деян", "Откр", "phil", "1 cor", "Пс", "Ін", "Від Івана"]
        {
            for language in ["ru", "en"] {
                assert_eq!(lookup_words(book, language), None, "{book} ({language})");
            }
        }
        assert_eq!(lookup_words("love one another", "ru"), Some("love one another"));
        assert_eq!(lookup_words("долиною смертной", "ru"), Some("долиною смертной"));
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
        // Looked up from the Russian Synodal's lists, named in the session's language (English,
        // with nothing typed and no computer's language in a test).
        assert_eq!(state.bible.lookup, "John 3:5", "the click looked the verse up");
        assert_eq!(state.bible.shown, "От Иоанна 3:5");
        assert_eq!(state.bible.language, None, "a click says nothing of a language");

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
    fn a_double_click_in_the_lists_loads_the_chapter_and_picks_a_verse_without_putting_it_on() {
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        // A computer set to Russian: with nothing typed yet, the operator reads Russian.
        state.bible.system_language = crate::system_language::SystemLanguage::answered("ru-RU");
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
        // A verse: its chapter, with its slide picked for Enter; nothing goes on screen. (A
        // pause first, or the clicks would count on from the chapter's as a triple click.)
        state.bible.pending = None;
        for _ in 0..10 {
            run(&mut state, Vec::new());
        }
        let verse = ctx.read_response(crate::bible::list_row_id(("verse", 5u16))).unwrap().rect.center();
        double_click(&mut run, &mut state, verse);
        assert_eq!(state.bible.lookup, "От Иоанна 3:1-36");
        assert!(state.bible.pending.is_some());
        assert_eq!(state.bible.go_live, None, "nothing goes on screen");
        let picked = state.picked.as_ref().map(|picked| picked.at.slide);
        assert_eq!(picked, Some(4), "the fifth slide picked, one verse a slide");
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
                            ThumbOf::Slide(_)
                            | ThumbOf::SongSlide(..)
                            | ThumbOf::InVersion(..)
                            | ThumbOf::BibleSlide(_)
                            | ThumbOf::Layout(..) => slides += 1,
                            ThumbOf::Media(_) => media += 1,
                            ThumbOf::Own(..) => {}
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

    #[test]
    fn a_long_songs_slides_in_the_edit_view_ask_only_for_the_pictures_in_sight() {
        let show = show(40);
        let editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
        let mut state = HomeState { editing, ..HomeState::default() };
        let (slides, _) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert!((1..=4).contains(&slides), "{slides} of 40 pictures asked for");
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
        let templates = |state: &HomeState| state.theme_edit.as_ref().map_or(0, |edit| edit.theme.templates.len());
        let before = templates(&state);
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(templates(&state), before, "the template stays");
        assert_eq!(state.editing.as_ref().map(|d| d.slide.objects.len()), Some(boxes - 1), "the box is gone");
        // With nothing selected, Delete is for the template: it goes at once, asking nothing,
        // and is kept for Undo, which Command-Z in the editor asks of the app.
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(templates(&state), before - 1);
        assert!(!state.alert_open());
        assert!(matches!(state.removed.as_slice(), [crate::removed::Removed::Template { at: 0, set: None, .. }]));
        assert_eq!(state.edit_steps.last(), Some(&EditStep::Deleted));
        let undo =
            Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND };
        frame_events(&ctx, &show, &mut state, vec![undo], 1.0);
        assert_eq!((state.song_undo, state.edit_redo.as_slice()), (Some(false), &[EditStep::Deleted][..]));
    }

    /// Runs one frame of the home view with these events, and returns each piece of text it drew,
    /// with where, in the order drawn.
    fn frame_texts(
        ctx: &egui::Context,
        show: &Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
    ) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))),
            time: Some(time),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
        });
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        output.drop_without_applying_deltas();
        texts
    }

    /// The middle of where `words` were drawn, or none.
    fn drawn_at(texts: &[(String, Rect)], words: &str) -> Option<egui::Pos2> {
        texts.iter().rfind(|(text, _)| text == words).map(|(_, rect)| rect.center())
    }

    /// The built-in theme with an Offering version made of copies of its two song templates.
    fn offering_theme() -> (Theme, sp_model::Version) {
        let mut theme = Theme::builtin();
        let offering = sp_model::Version { id: sp_model::Uuid::from_u128(77), name: "Offering".to_owned() };
        assert!(theme.add_version(&offering, &[0, 1]));
        (theme, offering)
    }

    #[test]
    fn a_version_or_its_last_template_deleted_comes_back_with_undo() {
        use crate::removed::Removed;
        let (theme, offering) = offering_theme();
        // A version taken out, with its two templates: Undo puts it back where it was, opened;
        // Redo takes it out again.
        let mut changed = theme.clone();
        let gone = Removed::Version { theme: theme.id, at: 0, version: changed.versions[0].clone() };
        assert!(changed.remove_version(offering.id));
        assert_eq!(gone.redo_in_theme(&mut changed, true), Some((Some(offering.id), 0)));
        assert_eq!(changed, theme);
        assert_eq!(gone.redo_in_theme(&mut changed, true), None, "it is there already");
        assert_eq!(gone.redo_in_theme(&mut changed, false), Some((None, 0)));
        assert!(changed.versions.is_empty());
        // The last template of a version deleted in the Theme Editor takes the version with it,
        // and Undo brings both back.
        let mut one = Theme::builtin();
        assert!(one.add_version(&offering, &[0]));
        let first = EditDraft::of_slide(one.templates[0].as_slide(), 0);
        let mut state = HomeState { theme_edit: Some(ThemeEdit::of(0, one.clone())), ..HomeState::default() };
        state.theme_edit.as_mut().expect("open").version = Some(offering.id);
        state.editing = Some(first);
        delete_template(&mut state, 0);
        let edit = state.theme_edit.as_ref().expect("still open");
        assert!(edit.theme.versions.is_empty() && edit.version.is_none(), "the version went with it");
        let gone = state.removed.pop().expect("kept for Undo");
        assert!(matches!(&gone, Removed::Template { version: Some((0, _)), set: Some(id), .. } if *id == offering.id));
        let mut changed = edit.theme.clone();
        assert_eq!(gone.redo_in_theme(&mut changed, true), Some((Some(offering.id), 0)));
        assert_eq!(changed, one);
        assert_eq!(gone.redo_in_theme(&mut changed, false), Some((None, 0)));
        assert!(changed.versions.is_empty());
        assert_eq!(gone.said(true), format!("Put the template {} back.", one.versions[0].templates[0].name));
    }

    #[test]
    fn the_themes_column_shows_a_versions_templates_and_the_canvas_names_the_version() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let (theme, offering) = offering_theme();
        let first = EditDraft::of_slide(theme.templates[0].as_slide(), 0);
        let mut state = HomeState {
            theme_names: vec![theme.name.clone()],
            versions: vec![offering.clone()],
            theme_edit: Some(ThemeEdit::of(0, theme)),
            editing: Some(first),
            ..HomeState::default()
        };
        let mut time = 0.0;
        let texts = frame_texts(&ctx, &show, &mut state, Vec::new(), time);
        for words in ["THEME", "Built-in", "MAIN", "TEMPLATES", "Usual", "Offering", "Built-in \u{b7} Two languages"] {
            assert!(drawn_at(&texts, words).is_some(), "{words} is drawn");
        }
        assert!(drawn_at(&texts, "TWO LANGUAGES \u{b7} SONGS").is_some(), "each template's strip says what it is for");
        // A click on Offering shows the version's templates, its first on the canvas.
        let at = drawn_at(&texts, "Offering").unwrap();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)]] {
            time += 0.1;
            frame_texts(&ctx, &show, &mut state, events, time);
        }
        let edit = state.theme_edit.as_ref().unwrap();
        assert_eq!(edit.version, Some(offering.id));
        let opened = state.editing.as_ref().and_then(|draft| draft.slide.template);
        assert_eq!(opened, Some(edit.theme.versions[0].templates[0].id));
        let texts = frame_texts(&ctx, &show, &mut state, Vec::new(), time + 0.1);
        assert!(drawn_at(&texts, "Built-in \u{b7} Offering \u{b7} Two languages").is_some(), "the canvas names it");
        assert!(drawn_at(&texts, "Used while the Offering version is on, on every screen with this theme.").is_some());
        // Its templates can all be deleted (the last takes the version out), at once.
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        let edit = state.theme_edit.as_ref().unwrap();
        assert_eq!(edit.theme.versions[0].templates.len(), 1, "one of its two went");
        assert!(matches!(state.removed.as_slice(), [crate::removed::Removed::Template { at: 0, .. }]));
    }

    #[test]
    fn a_playlist_items_menu_has_a_version_section_with_every_version() {
        let ctx = egui::Context::default();
        let (_, offering) = offering_theme();
        let mut state = HomeState::default();
        state.services.add(crate::service::Item::song("title:Harbour", "Harbour"));
        let words_drawn = |state: &mut HomeState| {
            let output = ctx.run_ui(input(None), |ui| crate::studio::version_section(ui, state, 0));
            let mut words = Vec::new();
            fn walk(shape: &egui::Shape, words: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => words.push(text.galley.text().to_owned()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, words)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
            words
        };
        // No version in any theme: no section.
        assert!(words_drawn(&mut state).is_empty());
        state.versions = vec![offering];
        let words = words_drawn(&mut state);
        for wanted in ["VERSION", "The usual look", "The Offering version", "Keep It After This Item"] {
            assert!(words.iter().any(|w| w == wanted), "{wanted} in {words:?}");
        }
        assert!(!words.iter().any(|w| w.contains("applies in this playlist only")), "the note is the button's only");
    }

    #[test]
    fn a_playlist_item_shown_in_a_version_has_its_name_under_its_title() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(parse_song_text("Harbour", "Words\n"))]);
        let (_, offering) = offering_theme();
        let mut state = HomeState { playlist_open: true, versions: vec![offering.clone()], ..HomeState::default() };
        state.services.add(crate::service::Item::song("title:Harbour", "Harbour"));
        let texts = frame_texts(&ctx, &show, &mut state, Vec::new(), 0.0);
        assert!(drawn_at(&texts, "OFFERING").is_none());
        let set = crate::service::ShowWith { version: offering.id, keep: false };
        assert!(state.services.set_show_with(0, Some(set)));
        let texts = frame_texts(&ctx, &show, &mut state, Vec::new(), 0.1);
        let title = drawn_at(&texts, "Harbour").expect("the title");
        let pill = drawn_at(&texts, "OFFERING").expect("the version's name");
        assert!(pill.y > title.y, "on a line of its own under the title");
        // A version no theme has any more shows no pill.
        state.versions.clear();
        let texts = frame_texts(&ctx, &show, &mut state, Vec::new(), 0.2);
        assert!(drawn_at(&texts, "OFFERING").is_none());
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
        assert!(state.removed.is_empty(), "no template is deleted");
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
        assert_eq!((kept.confirm_delete_theme, kept.removed.len()), (None, 0));
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
    fn delete_in_a_media_playlists_view_takes_the_items_out_of_it_alone() {
        let (ctx, show) = (egui::Context::default(), show_with_media());
        let mut playlist = crate::media::Playlist::new("Loop".to_owned());
        playlist.files = vec!["b.png".into(), "a.png".into()];
        let id = playlist.id;
        let mut other = crate::media::Playlist::new("Other".to_owned());
        other.files = vec!["a.png".into()];
        let mut state = HomeState {
            media_playlists: vec![playlist, other],
            media_playlist_shown: Some(id),
            picked_media: vec![0],
            picking: Picking::Media,
            ..HomeState::default()
        };
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert!(!state.alert_open(), "nothing is asked");
        assert!(state.confirm_delete_media.is_empty() && state.media_request.is_none(), "nothing leaves the bin");
        assert_eq!(state.media_playlists[0].files, [std::path::PathBuf::from("b.png")]);
        assert_eq!(state.media_playlists[1].files, [std::path::PathBuf::from("a.png")], "another keeps it");
        assert!(state.media_playlists_changed);
        // One that follows a folder keeps what the folder has, and says so.
        state.media_playlists[0].folder = Some("/a".into());
        state.picked_media = vec![1];
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.media_playlists[0].files.len(), 1);
        let said = state.toast.as_ref().map(|(words, _)| words.as_str());
        assert!(said.is_some_and(|said| said.contains("follows a folder")), "{said:?}");
        // In All Media, Delete still asks before deleting from the bin.
        state.media_playlist_shown = None;
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.confirm_delete_media, vec![1]);
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
        // Stage opens the first layout in the editor, as a slide of objects; a slide opened after
        // it takes its place.
        state.edit_kind_request = Some(EditKind::Stage);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), Some(EditKind::Stage));
        assert!(!state.stage_layouts.is_empty());
        let draft = state.editing.as_ref().expect("the layout's draft");
        assert_eq!((draft.at.presentation, draft.at.slide), (crate::stage::STAGE_SLOT, 0));
        assert_eq!(draft.slide.objects.len(), state.stage_layouts[0].objects.len());
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
        // With no props, Props opens their list, empty, with Add, which makes one and opens it.
        state.props.clear();
        state.edit_kind_request = Some(EditKind::Props);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!((state.edit_kind(), state.edit_kind_request), (Some(EditKind::Props), None));
        assert!(state.editing.is_none() && state.props_editor_open);
        let asked = click_prop_words(&ctx, &mut state, "Add", egui::PointerButton::Primary).expect("Add");
        apply_prop_ask(&mut state, asked);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.props.len(), 1);
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.presentation), Some(crate::props::PROP_SLOT));
        assert!(!state.props_editor_open, "the prop open takes the empty list's place");
        // Show leaves it, as any kind.
        state.props.clear();
        state.editing = None;
        state.edit_kind_request = Some(EditKind::Props);
        frame_with(&ctx, &show, &mut state, None);
        state.view_request = Some(0);
        frame_with(&ctx, &show, &mut state, None);
        assert_eq!(state.edit_kind(), None);
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

    /// Props with these names, the one at `open` open in the editor.
    fn props_open(names: &[&str], open: usize) -> HomeState {
        let mut state = HomeState::default();
        state.props = names.iter().map(|name| crate::props::Prop::new((*name).to_owned())).collect();
        state.editing = crate::studio::prop_draft(&state, open);
        state
    }

    /// One frame of the props' part of the Edit view's left column, as wide as the column: what
    /// it drew, and what it asked for.
    fn prop_column(
        ctx: &egui::Context,
        state: &mut HomeState,
        events: Vec<Event>,
    ) -> (egui::FullOutput, Option<PropListAsk>) {
        let mut asked = None;
        let column = Rect::from_min_size(pos2(10.0, 0.0), vec2(crate::studio::LEFT_WIDTH, 800.0));
        let output = crate::studio::tests::drawn(ctx, events, |ui| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let head = prop_list_head(ui, state, false);
                let list = prop_list(ui, state, &mut |_| None);
                asked = head.or(list);
            });
        });
        (output, asked)
    }

    #[test]
    fn a_prop_dragged_in_the_edit_views_list_moves_there_and_the_one_open_stays_open() {
        let ctx = crate::studio::tests::styled();
        let mut state = props_open(&["Logo", "Name", "Welcome"], 1);
        let (output, _) = prop_column(&ctx, &mut state, vec![]);
        let logo = crate::studio::tests::place_of(&output, "LOGO").expect("drawn");
        let welcome = crate::studio::tests::place_of(&output, "WELCOME").expect("drawn");
        output.drop_without_applying_deltas();
        let mut asked = None;
        for events in crate::studio::tests::drag_frames(logo, welcome) {
            let (output, now) = prop_column(&ctx, &mut state, events);
            output.drop_without_applying_deltas();
            asked = asked.or(now);
        }
        assert_eq!(asked, Some(PropListAsk::Move(0, 2)));
        apply_prop_ask(&mut state, PropListAsk::Move(0, 2));
        let names: Vec<&str> = state.props.iter().map(|prop| prop.name.as_str()).collect();
        assert_eq!(names, ["Name", "Welcome", "Logo"]);
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(0), "Name stays open");
        assert!(state.props_changed);
    }

    #[test]
    fn the_keys_over_a_list_fit_the_left_column_with_duplicate_spelt_out() {
        let ctx = crate::studio::tests::styled();
        let mut state = props_open(&["Logo"], 0);
        let (output, _) = prop_column(&ctx, &mut state, vec![]);
        let place = |words: &str| crate::studio::tests::place_of(&output, words).expect(words);
        let (add, duplicate, delete, down) = (place("Add"), place("Duplicate"), place("Delete"), place("\u{2193}"));
        output.drop_without_applying_deltas();
        assert!(add.x < duplicate.x && duplicate.x < delete.x && delete.x < down.x);
        // The column's room is LEFT_WIDTH less 10 either side; the keys stand 8 in from it, the
        // last 26 wide.
        let right = 10.0 + crate::studio::LEFT_WIDTH - 10.0 - 8.0;
        assert!(down.x + 13.0 <= right + 0.5, "the last key ends at {} of {right}", down.x + 13.0);
    }

    #[test]
    fn a_row_dragged_in_a_list_lands_where_the_ink_line_is() {
        let rows: Vec<Rect> =
            (0..4).map(|n| Rect::from_min_size(pos2(0.0, n as f32 * 50.0), vec2(200.0, 40.0))).collect();
        // The first onto the third's lower half: third place, counting without it.
        assert_eq!(dragged_to(&rows, 0, 2.0 * 50.0 + 30.0), Some(2));
        // The last above the first.
        assert_eq!(dragged_to(&rows, 3, 5.0), Some(0));
        // Let go where it was: nothing moves.
        assert_eq!(dragged_to(&rows, 1, 50.0 + 10.0), None);
        assert_eq!(dragged_to(&rows, 1, 50.0 + 30.0), None);
        // What was open follows the move.
        assert_eq!([0, 1, 2, 3].map(|at| place_after_move(at, 0, 2)), [2, 0, 1, 3]);
        assert_eq!([0, 1, 2, 3].map(|at| place_after_move(at, 3, 1)), [0, 2, 3, 1]);
    }

    #[test]
    fn delete_with_nothing_picked_on_the_canvas_is_the_lists_delete_key() {
        let ctx = egui::Context::default();
        // A prop open: Delete deletes it at once, as the list's Delete key does (Undo puts it
        // back).
        let show = three_songs();
        let mut state = props_open(&["Logo", "Name"], 1);
        frame_with(&ctx, &show, &mut state, None);
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert_eq!(state.props.iter().map(|prop| prop.name.as_str()).collect::<Vec<_>>(), ["Logo"]);
        assert!(matches!(state.removed.as_slice(), [crate::removed::Removed::Prop { at: 1, .. }]));
        // A song's slide open, nothing picked on the canvas: the slide, at once.
        let (show, mut state) = song_open(&[("Verse 1", 2), ("Chorus", 1)], 1);
        frame_with(&ctx, &show, &mut state, None);
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::DeleteMany(vec![1])), "deleted at once");
        // With a box picked on the canvas, Delete takes the box, and the slide stays.
        let (show, mut state) = song_open(&[("Verse 1", 2), ("Chorus", 1)], 1);
        frame_with(&ctx, &show, &mut state, None);
        if let Some(draft) = state.editing.as_mut() {
            let first = draft.slide.objects.first().map(|object| object.id);
            draft.select_only(first);
        }
        let boxes = state.editing.as_ref().map_or(0, |draft| draft.slide.objects.len());
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert!(state.slide_request.is_none());
        assert_eq!(state.editing.as_ref().map(|draft| draft.slide.objects.len()), Some(boxes - 1));
        // A stage layout open: the layout.
        let show = three_songs();
        let mut state = HomeState { edit_kind_request: Some(EditKind::Stage), ..HomeState::default() };
        frame_with(&ctx, &show, &mut state, None);
        frame_with(&ctx, &show, &mut state, None);
        let first = state.stage_layouts[0].id;
        frame_with(&ctx, &show, &mut state, Some(Key::Delete));
        assert!(state.stage_layouts.iter().all(|layout| layout.id != first), "deleted at once");
        assert!(matches!(state.removed.as_slice(), [crate::removed::Removed::StageLayout { at: 0, .. }]));
    }

    #[test]
    fn templates_and_themes_drag_to_another_place_in_the_theme_editor() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let theme = Theme::builtin();
        let first = theme.templates[0].name.clone();
        let draft = EditDraft::of_slide(theme.templates[0].as_slide(), 0);
        let mut state = HomeState {
            theme_edit: Some(ThemeEdit::of(0, theme)),
            editing: Some(draft),
            theme_names: vec!["Built-in".to_owned(), "Alpha".to_owned(), "Beta".to_owned()],
            ..HomeState::default()
        };
        let texts = frame_texts(&ctx, &show, &mut state, vec![], 0.0);
        let mut labels: Vec<&(String, Rect)> =
            texts.iter().filter(|(text, rect)| text.contains(" \u{b7} ") && rect.min.x < 300.0).collect();
        labels.sort_by(|a, b| a.1.min.y.total_cmp(&b.1.min.y));
        let (top, second) = (labels[0].1.center(), labels[1].1.center());
        // The first template, open, onto the second's lower half: after it, and still open.
        let mut time = 0.1;
        for events in crate::studio::tests::drag_frames(top, second + vec2(0.0, 6.0)) {
            frame_texts(&ctx, &show, &mut state, events, time);
            time += 0.1;
        }
        let edit = state.theme_edit.as_ref().expect("still editing");
        assert_eq!(edit.templates()[1].name, first);
        assert_eq!(edit.template, 1, "the template open stays open");
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(1));
        // A theme onto the row under it: asked of the app; the built-in theme stays first.
        let place = |texts: &[(String, Rect)], name: &str| {
            texts.iter().find(|(text, _)| text == name).map(|(_, rect)| rect.center()).expect(name)
        };
        let texts = frame_texts(&ctx, &show, &mut state, vec![], time);
        let (alpha, beta, built_in) = (place(&texts, "Alpha"), place(&texts, "Beta"), place(&texts, "Built-in"));
        for events in crate::studio::tests::drag_frames(alpha, beta + vec2(0.0, 8.0)) {
            time += 0.1;
            frame_texts(&ctx, &show, &mut state, events, time);
        }
        assert_eq!(state.theme_request, Some(ThemeRequest::Move(1, 2)));
        state.theme_request = None;
        for events in crate::studio::tests::drag_frames(beta, built_in - vec2(0.0, 8.0)) {
            time += 0.1;
            frame_texts(&ctx, &show, &mut state, events, time);
        }
        assert_eq!(state.theme_request, Some(ThemeRequest::Move(2, 1)), "under the built-in theme");
        state.theme_request = None;
        for events in crate::studio::tests::drag_frames(built_in, beta + vec2(0.0, 8.0)) {
            time += 0.1;
            frame_texts(&ctx, &show, &mut state, events, time);
        }
        assert_eq!(state.theme_request, None, "the built-in theme isn't dragged");
    }

    /// Clicks `words` where the props' column draws them, and returns what was asked for.
    fn click_prop_words(
        ctx: &egui::Context,
        state: &mut HomeState,
        words: &str,
        button: egui::PointerButton,
    ) -> Option<PropListAsk> {
        let (output, _) = prop_column(ctx, state, vec![]);
        let at = crate::studio::tests::place_on_top(&output, words).expect(words);
        output.drop_without_applying_deltas();
        let mut asked = None;
        for events in crate::studio::tests::click_frames(at, button) {
            let (output, now) = prop_column(ctx, state, events);
            output.drop_without_applying_deltas();
            asked = asked.or(now);
        }
        asked
    }

    #[test]
    fn a_props_right_click_menu_offers_what_its_more_button_in_the_props_tab_does() {
        use crate::live_tabs::PropAsk;
        use egui::PointerButton::{Primary, Secondary};
        let ctx = crate::studio::tests::styled();
        let mut state = props_open(&["Logo", "Name", "Welcome"], 0);
        assert_eq!(click_prop_words(&ctx, &mut state, "NAME", Secondary), None);
        // A menu is drawn from its second frame, once it knows its size.
        let (menu, _) = prop_column(&ctx, &mut state, vec![]);
        for offered in ["Clears Itself", "Rename", "Duplicate", "Delete"] {
            assert!(crate::studio::tests::place_of(&menu, offered).is_some(), "{offered} is offered");
        }
        // It is open already: there is no Edit in the Editor.
        assert!(crate::studio::tests::place_of(&menu, "Edit in the Editor").is_none());
        menu.drop_without_applying_deltas();
        let asked = click_prop_words(&ctx, &mut state, "Rename", Primary);
        assert_eq!(asked, Some(PropListAsk::Of(1, PropAsk::Rename)));
        apply_prop_ask(&mut state, asked.expect("asked"));
        assert_eq!(state.prop_rename, Some((state.props[1].id, "Name".to_owned())));
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(0), "the open prop stays open");
        // A click on another opens it.
        state.prop_rename = None;
        let asked = click_prop_words(&ctx, &mut state, "WELCOME", Primary);
        assert_eq!(asked, Some(PropListAsk::Open(2)));
        apply_prop_ask(&mut state, asked.expect("asked"));
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(2));
    }

    #[test]
    fn the_keys_over_the_props_act_on_the_one_open() {
        use egui::PointerButton::Primary;
        let ctx = crate::studio::tests::styled();
        let mut state = props_open(&["Logo", "Name", "Welcome"], 1);
        let open = |state: &HomeState| state.editing.as_ref().map(|draft| draft.at.slide);
        let names = |state: &HomeState| state.props.iter().map(|prop| prop.name.clone()).collect::<Vec<_>>();
        // Duplicate: a copy of the open prop just after it, and it opens.
        let asked = click_prop_words(&ctx, &mut state, "Duplicate", Primary).expect("asked");
        apply_prop_ask(&mut state, asked);
        assert_eq!(names(&state), ["Logo", "Name", "Name copy", "Welcome"]);
        assert_eq!(open(&state), Some(2));
        assert!(state.props_changed);
        // Down: it goes one place down, and stays open in the same draft.
        let serial = state.editing.as_ref().map(|draft| draft.serial);
        let asked = click_prop_words(&ctx, &mut state, "\u{2193}", Primary).expect("asked");
        apply_prop_ask(&mut state, asked);
        assert_eq!(names(&state), ["Logo", "Name", "Welcome", "Name copy"]);
        assert_eq!((open(&state), state.editing.as_ref().map(|draft| draft.serial)), (Some(3), serial));
        // The last can't go further down.
        assert_eq!(click_prop_words(&ctx, &mut state, "\u{2193}", Primary), None);
        // Delete goes at once, asking nothing: the prop goes, off the screens, and the one
        // before it (it was the last) opens.
        let copy = state.props[3].clone();
        state.props_live.push((copy.id, std::time::Instant::now()));
        let asked = click_prop_words(&ctx, &mut state, "Delete", Primary).expect("asked");
        apply_prop_ask(&mut state, asked);
        assert!(!state.alert_open());
        assert_eq!(names(&state), ["Logo", "Name", "Welcome"]);
        assert_eq!(open(&state), Some(2));
        assert!(state.props_live.is_empty(), "it comes off the screens");
        // Undo puts it back where it was, open again; Command-Z in the editor asks for it.
        assert_eq!(state.edit_steps.last(), Some(&EditStep::Deleted));
        let gone = state.removed.pop().expect("kept for Undo");
        assert_eq!(gone, crate::removed::Removed::Prop { at: 3, prop: copy.clone() });
        assert!(gone.redo(&mut state, true));
        assert_eq!((names(&state)[3].as_str(), open(&state)), ("Name copy", Some(3)));
        assert!(gone.redo(&mut state, false));
        assert_eq!(
            (names(&state), open(&state)),
            (vec!["Logo".to_owned(), "Name".to_owned(), "Welcome".to_owned()], Some(2))
        );
        // Add: a new prop at the end, open, with its name ready to be typed over.
        let asked = click_prop_words(&ctx, &mut state, "Add", Primary).expect("asked");
        apply_prop_ask(&mut state, asked);
        assert_eq!((state.props.len(), open(&state)), (4, Some(3)));
        assert_eq!(state.prop_rename, Some((state.props[3].id, "Prop 1".to_owned())));
        // Deleting a prop before the open one keeps the open one open.
        state.delete_prop(0);
        assert_eq!((names(&state)[2].as_str(), open(&state)), ("Prop 1", Some(2)));
        // With the last prop deleted, the props' list stays, empty, with Add.
        let mut state = props_open(&["Logo"], 0);
        state.delete_prop(0);
        assert!(state.props.is_empty());
        assert_eq!((state.editing.is_none(), state.edit_kind()), (true, Some(EditKind::Props)));
    }

    #[test]
    fn a_prop_or_layout_deleted_in_the_edit_view_goes_at_once_and_the_next_opens() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let mut state = HomeState::default();
        for name in ["Logo", "Name", "Welcome"] {
            state.props.push(crate::props::Prop::new(name.to_owned()));
        }
        state.edit_kind_request = Some(EditKind::Props);
        frame_events(&ctx, &show, &mut state, vec![], 0.0);
        assert_eq!(state.edit_kind(), Some(EditKind::Props));
        // The second open, and deleted: it goes at once and the one after it opens.
        state.editing = crate::studio::prop_draft(&state, 1);
        apply_prop_ask(&mut state, PropListAsk::Of(1, crate::live_tabs::PropAsk::Delete));
        frame_events(&ctx, &show, &mut state, vec![], 0.1);
        assert!(!state.alert_open(), "nothing is asked");
        let names: Vec<&str> = state.props.iter().map(|prop| prop.name.as_str()).collect();
        assert_eq!(names, ["Logo", "Welcome"]);
        assert_eq!(
            state.editing.as_ref().map(|draft| (draft.at.presentation, draft.at.slide)),
            Some((crate::props::PROP_SLOT, 1))
        );
        // With the last one deleted, the empty list stays, with Add; Undo opens the prop again.
        state.props.truncate(1);
        state.editing = crate::studio::prop_draft(&state, 0);
        state.removed.clear();
        apply_prop_ask(&mut state, PropListAsk::Of(0, crate::live_tabs::PropAsk::Delete));
        frame_events(&ctx, &show, &mut state, vec![], 0.5);
        assert!(state.props.is_empty());
        assert_eq!(state.edit_kind(), Some(EditKind::Props), "the empty list, with Add");
        let gone = state.removed.pop().expect("kept for Undo");
        assert!(gone.redo(&mut state, true));
        assert_eq!(
            state.editing.as_ref().map(|draft| (draft.at.presentation, draft.at.slide)),
            Some((crate::props::PROP_SLOT, 0))
        );
        assert!(!state.props_editor_open, "the prop open takes the empty list's place");
        // A stage layout the same way.
        state.edit_kind_request = Some(EditKind::Stage);
        frame_events(&ctx, &show, &mut state, vec![], 0.6);
        frame_events(&ctx, &show, &mut state, vec![], 0.7);
        assert_eq!(state.edit_kind(), Some(EditKind::Stage));
        let second = state.stage_layouts[1].id;
        crate::stage_editor::apply(&mut state, crate::stage_editor::ListRequest::Delete(1));
        frame_events(&ctx, &show, &mut state, vec![], 0.8);
        assert_eq!(state.stage_layouts.len(), 3);
        assert!(state.stage_layouts.iter().all(|layout| layout.id != second));
        assert_eq!(state.edit_kind(), Some(EditKind::Stage));
        assert!(!state.alert_open());
    }

    #[test]
    fn a_prop_renamed_in_the_edit_views_list_keeps_its_name_where_and_as_it_was() {
        use crate::studio::tests::{key, letters, same_letters};
        let ctx = crate::studio::tests::styled();
        let mut state = props_open(&["Logo", "Speaker name"], 0);
        let before = letters(prop_column(&ctx, &mut state, vec![]).0, "SPEAKER NAME");
        state.prop_rename = Some((state.props[1].id, "Speaker name".to_owned()));
        state.rename_started = true;
        prop_column(&ctx, &mut state, vec![]).0.drop_without_applying_deltas();
        same_letters(&before, &letters(prop_column(&ctx, &mut state, vec![]).0, "SPEAKER NAME"), "prop");
        prop_column(&ctx, &mut state, vec![Event::Text(" two".to_owned())]).0.drop_without_applying_deltas();
        prop_column(&ctx, &mut state, vec![key(Key::Enter)]).0.drop_without_applying_deltas();
        assert_eq!(state.props[1].name, "Speaker name two", "Return keeps what was typed");
        assert!(state.prop_rename.is_none());
        assert!(state.props_changed);
    }

    /// A song of these groups (a name and how many slides), open in the Edit view at slide
    /// `open`. Each slide says its group's name and its place in it.
    fn song_open(groups: &[(&str, usize)], open: usize) -> (Show, HomeState) {
        let text: Vec<String> = groups
            .iter()
            .map(|(name, slides)| {
                let lines: Vec<String> = (0..*slides).map(|n| format!("{name} line {n}\n")).collect();
                format!("[{name}]\n{}", lines.join("\n"))
            })
            .collect();
        let show = Show::new(vec![Arc::new(parse_song_text("Harbour", &text.join("\n")))]);
        let editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: open });
        (show, HomeState { editing, ..HomeState::default() })
    }

    /// One frame, a twentieth of a second after the last, of the slides' part of the Edit view's
    /// left column, as wide as the column and tall enough for every picture: what it drew, and
    /// the slide it asked to open.
    fn slide_column(
        ctx: &egui::Context,
        show: &Show,
        state: &mut HomeState,
        events: Vec<Event>,
    ) -> (egui::FullOutput, Option<SlideRef>) {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 2400.0))),
            time: Some(ctx.input(|input| input.time) + 0.05),
            events,
            ..RawInput::default()
        };
        let at = state.editing.as_ref().map(|draft| draft.at).expect("a slide open");
        let column = Rect::from_min_size(pos2(10.0, 0.0), vec2(crate::studio::LEFT_WIDTH, 2400.0));
        let mut asked = None;
        let output = ctx.run_ui(raw, |ui| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let head = slide_list_head(ui, show, state, at, false);
                let list = slide_list(ui, show, state, &mut |_| None, at);
                asked = head.or(list);
            });
        });
        (output, asked)
    }

    /// Where the slides' column draws `words`.
    fn slide_column_place(ctx: &egui::Context, show: &Show, state: &mut HomeState, words: &str) -> egui::Pos2 {
        let (output, _) = slide_column(ctx, show, state, vec![]);
        let at = crate::studio::tests::place_of(&output, words).expect(words);
        output.drop_without_applying_deltas();
        at
    }

    /// Clicks where the slides' column draws `words`; returns the slide it asked to open.
    fn click_slide_words(ctx: &egui::Context, show: &Show, state: &mut HomeState, words: &str) -> Option<SlideRef> {
        let at = slide_column_place(ctx, show, state, words);
        let mut asked = None;
        for events in crate::studio::tests::click_frames(at, egui::PointerButton::Primary) {
            let (output, now) = slide_column(ctx, show, state, events);
            output.drop_without_applying_deltas();
            asked = asked.or(now);
        }
        asked
    }

    /// Drags in the slides' column from one point to another and lets go there (or, with
    /// `escape`, presses Escape first). Returns what the frame before letting go drew.
    fn drag_slides(
        ctx: &egui::Context,
        show: &Show,
        state: &mut HomeState,
        from: egui::Pos2,
        to: egui::Pos2,
        escape: bool,
    ) -> egui::FullOutput {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let run = |state: &mut HomeState, events: Vec<Event>| slide_column(ctx, show, state, events).0;
        run(state, vec![Event::PointerMoved(from)]).drop_without_applying_deltas();
        run(state, vec![button(from, true)]).drop_without_applying_deltas();
        for step in 1..=4 {
            let at = from + (to - from) * (step as f32 / 4.0);
            run(state, vec![Event::PointerMoved(at)]).drop_without_applying_deltas();
        }
        let held = run(state, vec![]);
        if escape {
            run(state, vec![crate::studio::tests::key(Key::Escape)]).drop_without_applying_deltas();
        }
        run(state, vec![button(to, false)]).drop_without_applying_deltas();
        run(state, vec![]).drop_without_applying_deltas();
        held
    }

    #[test]
    fn slides_dragged_in_the_edit_view_go_where_the_ink_line_is() {
        let ctx = crate::studio::tests::styled();
        let (show, mut state) = song_open(&[("Verse 1", 2), ("Chorus", 2), ("Verse 2", 2)], 0);
        let strip = |state: &mut HomeState, number: &str| slide_column_place(&ctx, &show, state, number);
        // A picture's strip is its lower half; 110 points above the strip is its upper half.
        let (first, third, fifth) = (strip(&mut state, "01"), strip(&mut state, "03"), strip(&mut state, "05"));
        // The first, onto the lower half of the third: it goes after the third, in its group,
        // and its name follows the pointer until it is let go.
        let held = drag_slides(&ctx, &show, &mut state, first - vec2(0.0, 60.0), third, false);
        assert!(crate::studio::tests::place_of(&held, "01 Verse 1").is_some(), "its name follows the pointer");
        held.drop_without_applying_deltas();
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![0], to: 2, after: true }));
        // Two picked out, one of them dragged: both go, before the fifth.
        state.picked_slides = vec![SlideRef { presentation: 0, slide: 1 }, SlideRef { presentation: 0, slide: 0 }];
        let second = strip(&mut state, "02");
        let held = drag_slides(&ctx, &show, &mut state, second - vec2(0.0, 60.0), fifth - vec2(0.0, 110.0), false);
        assert!(crate::studio::tests::place_of(&held, "2 slides").is_some());
        held.drop_without_applying_deltas();
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![0, 1], to: 4, after: false }));
        // Stopped with Escape, or let go away from the list, nothing moves.
        state.picked_slides.clear();
        drag_slides(&ctx, &show, &mut state, first - vec2(0.0, 60.0), third, true).drop_without_applying_deltas();
        assert_eq!(state.slide_request, None, "Escape");
        let away = pos2(600.0, third.y);
        drag_slides(&ctx, &show, &mut state, first - vec2(0.0, 60.0), away, false).drop_without_applying_deltas();
        assert_eq!(state.slide_request, None, "let go over the slide being edited");
        let keys = slide_column_place(&ctx, &show, &mut state, "Duplicate");
        drag_slides(&ctx, &show, &mut state, third - vec2(0.0, 60.0), keys, false).drop_without_applying_deltas();
        assert_eq!(state.slide_request, None, "let go over the keys above the list");
    }

    #[test]
    fn where_dragged_slides_go_by_the_pointer_between_two_pictures() {
        // Three pictures 100 high with 6 between them.
        let rows: Vec<Rect> =
            (0..3).map(|n| Rect::from_min_size(pos2(0.0, n as f32 * 106.0), vec2(200.0, 100.0))).collect();
        // Over the lower half of the second, or the room just under it: after the second.
        assert_eq!(slide_drop(&rows, &[0], 180.0), Some((1, true)));
        assert_eq!(slide_drop(&rows, &[0], 208.0), Some((1, true)));
        // Just above the third, or over its upper half: before the third.
        assert_eq!(slide_drop(&rows, &[0], 210.0), Some((2, false)));
        assert_eq!(slide_drop(&rows, &[0], 230.0), Some((2, false)));
        // Next to itself: the nearest slide not being moved instead.
        assert_eq!(slide_drop(&rows, &[1], 180.0), Some((0, true)));
        assert_eq!(slide_drop(&rows, &[1, 2], 300.0), Some((0, true)));
        assert_eq!(slide_drop(&rows, &[0, 1], -10.0), Some((2, false)));
        assert_eq!(slide_drop(&rows, &[0, 1, 2], 80.0), None, "nowhere to go");
    }

    #[test]
    fn a_group_renamed_in_the_edit_views_slides_keeps_its_name_where_and_as_it_was() {
        use crate::studio::tests::{key, letters, same_letters};
        let ctx = crate::studio::tests::styled();
        let (show, mut state) = song_open(&[("Verse 1", 1), ("Chorus", 1)], 1);
        let before = letters(slide_column(&ctx, &show, &mut state, vec![]).0, "CHORUS");
        state.renaming_group = Some((1, "Chorus".to_owned()));
        state.rename_started = true;
        slide_column(&ctx, &show, &mut state, vec![]).0.drop_without_applying_deltas();
        let typing = letters(slide_column(&ctx, &show, &mut state, vec![]).0, "CHORUS");
        same_letters(&before, &typing, "group");
        slide_column(&ctx, &show, &mut state, vec![Event::Text(" two".to_owned())]).0.drop_without_applying_deltas();
        slide_column(&ctx, &show, &mut state, vec![key(Key::Enter)]).0.drop_without_applying_deltas();
        assert_eq!(state.pending_group_name, Some((0, 1, "Chorus two".to_owned())), "Return keeps what was typed");
        assert!(state.renaming_group.is_none());
        // A double click on a slide retypes its group's name; the first click opens the slide.
        let at = slide_column_place(&ctx, &show, &mut state, "01");
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut opened = None;
        let presses = [true, false, true, false].map(|pressed| vec![button(pressed)]);
        for events in std::iter::once(vec![Event::PointerMoved(at)]).chain(presses) {
            let (output, asked) = slide_column(&ctx, &show, &mut state, events);
            output.drop_without_applying_deltas();
            opened = opened.or(asked);
        }
        assert_eq!(opened, Some(SlideRef { presentation: 0, slide: 0 }));
        assert_eq!(state.renaming_group, Some((0, "Verse 1".to_owned())));
    }

    #[test]
    fn the_keys_over_the_edit_views_slides_act_on_the_slides_picked_out() {
        let ctx = crate::studio::tests::styled();
        let (show, mut state) = song_open(&[("Verse 1", 3), ("Chorus", 2)], 1);
        let words = |slides: &[Slide]| slides.iter().map(|slide| slide.texts()[0].1.clone()).collect::<Vec<_>>();
        // The second and third picked out.
        state.picked_slides = vec![SlideRef { presentation: 0, slide: 2 }, SlideRef { presentation: 0, slide: 1 }];
        click_slide_words(&ctx, &show, &mut state, "Duplicate");
        match state.slide_request.take() {
            Some((at, SlideRequest::AddAfter(copies))) => {
                assert_eq!(at.slide, 2, "after the last of them");
                assert_eq!(words(&copies), ["Verse 1 line 1", "Verse 1 line 2"]);
            }
            other => panic!("Copy asked for {other:?}"),
        }
        click_slide_words(&ctx, &show, &mut state, "\u{2191}");
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![1, 2], to: 0, after: false }));
        click_slide_words(&ctx, &show, &mut state, "\u{2193}");
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::Move { slides: vec![1, 2], to: 3, after: true }));
        click_slide_words(&ctx, &show, &mut state, "Delete");
        let asked = state.slide_request.take().map(|(_, request)| request);
        assert_eq!(asked, Some(SlideRequest::DeleteMany(vec![1, 2])), "deleted at once, in one step");
        assert!(!state.alert_open(), "nothing is asked");
        // Add: one slide after the one open, laid out like it with no words.
        click_slide_words(&ctx, &show, &mut state, "Add");
        match state.slide_request.take() {
            Some((at, SlideRequest::AddAfter(added))) => {
                assert_eq!((at.slide, words(&added)), (1, vec![String::new()]));
            }
            other => panic!("Add asked for {other:?}"),
        }
        // Nothing picked out: Duplicate copies the slide open, as the editor has it now.
        state.picked_slides.clear();
        if let Some(draft) = state.editing.as_mut() {
            draft.slide.set_words("Changed in the editor", "");
        }
        click_slide_words(&ctx, &show, &mut state, "Duplicate");
        match state.slide_request.take() {
            Some((at, SlideRequest::AddAfter(copies))) => {
                assert_eq!((at.slide, words(&copies)), (1, vec!["Changed in the editor".to_owned()]));
            }
            other => panic!("Copy asked for {other:?}"),
        }
        // Every slide picked out: Delete would leave none, so it does nothing.
        state.picked_slides = (0..5).map(|slide| SlideRef { presentation: 0, slide }).collect();
        click_slide_words(&ctx, &show, &mut state, "Delete");
        assert!(state.slide_request.is_none());
    }

    #[test]
    fn a_slide_deleted_from_under_the_editor_hands_its_place_to_the_next_that_stayed() {
        assert_eq!(place_after_deleting(4, &[1, 2]), 2);
        assert_eq!(place_after_deleting(2, &[3, 1, 2]), 1, "gone: the first after it that stayed");
        assert_eq!(place_after_deleting(0, &[0]), 0);
    }

    #[test]
    fn the_slide_being_edited_is_found_again_wherever_a_change_of_its_song_puts_it() {
        let (show, _) = song_open(&[("Verse 1", 3), ("Chorus", 2)], 2);
        let song = &show.presentations()[0];
        let slide = |song: &sp_model::Presentation, at: usize| song.slide(at).map(|(_, s)| s.texts()[0].1.clone());
        let open = song.slide(2).map(|(_, s)| s.id).expect("the third slide");
        // A slide before it deleted: it is one place higher, and is still the one open.
        let fewer = song.without_slide(0).expect("deleted");
        assert_eq!(edited_slide_place(&fewer, open, 2), Some((1, true)));
        // Copies of the first two put in after the second: it is two places lower.
        let copies: Vec<Slide> = (0..2).filter_map(|at| song.slide(at).map(|(_, s)| s.clone())).collect();
        let more = song.with_slides_after(1, &copies).expect("copied");
        assert_eq!(edited_slide_place(&more, open, 2), Some((4, true)));
        assert_eq!(slide(&more, 2).as_deref(), Some("Verse 1 line 0"), "the first copy opens after Copy");
        // It deleted itself: the slide now in its place opens, the Chorus's first.
        let gone = song.without_slide(2).expect("deleted");
        let place = edited_slide_place(&gone, open, place_after_deleting(2, &[2]));
        assert_eq!(place, Some((2, false)));
        assert_eq!(slide(&gone, 2).as_deref(), Some("Chorus line 0"));
        // The last ones deleted, it among them: the slide that is now last.
        let mut short = (**song).clone();
        for at in [4, 3, 2] {
            short = short.without_slide(at).expect("deleted");
        }
        assert_eq!(edited_slide_place(&short, open, place_after_deleting(2, &[2, 3, 4])), Some((1, false)));
        // Every slide gone: nothing to edit.
        let none = sp_model::Presentation { groups: Vec::new(), ..(**song).clone() };
        assert_eq!(edited_slide_place(&none, open, 0), None);
    }

    #[test]
    fn a_song_with_no_slides_offered_over_the_slides_opens_nothing() {
        let ctx = crate::studio::tests::styled();
        let (show, mut state) = song_open(&[("Verse 1", 2)], 0);
        let empty = sp_model::Presentation {
            title: "Empty".to_owned(),
            groups: Vec::new(),
            ..(*show.presentations()[0]).clone()
        };
        let show = Show::new(vec![show.presentations()[0].clone(), Arc::new(empty)]);
        assert_eq!(click_slide_words(&ctx, &show, &mut state, "Empty"), None, "the Edit view stays");
        assert!(state.toast.as_ref().is_some_and(|(words, _)| words.contains("no slides")));
    }

    #[test]
    fn the_edit_view_offers_the_playlists_songs_and_a_click_opens_one() {
        let ctx = crate::studio::tests::styled();
        let show = three_songs();
        let mut state = HomeState::default();
        // Three, One, and Three again; Two, open, isn't in it.
        let current = state.services.current;
        for at in [2, 0, 2] {
            let song = &show.presentations()[at];
            let item =
                crate::service::Item::Song { key: song.id.to_string(), title: song.title.clone(), show_with: None };
            state.services.list[current].items.push(item);
        }
        state.playlist_open = true;
        state.editing = EditDraft::of(&show, SlideRef { presentation: 1, slide: 0 });
        let (output, _) = slide_column(&ctx, &show, &mut state, vec![]);
        let place = |words: &str| crate::studio::tests::place_of(&output, words).map(|at| at.y);
        let (three, one, two) = (place("Three"), place("One"), place("Two"));
        output.drop_without_applying_deltas();
        assert!(three.is_some() && one.is_some() && two.is_some(), "the playlist's songs, and the one open");
        assert!(three < one && one < two, "in the playlist's order, each once, the one open after them");
        assert_eq!(click_slide_words(&ctx, &show, &mut state, "Three"), Some(SlideRef { presentation: 2, slide: 0 }));
    }

    #[test]
    fn an_open_question_takes_delete_and_the_arrows_from_the_edit_view() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        // A prop open, its words picked, and a question open (whether to delete a theme, which
        // Undo can't bring back).
        let mut state = props_open(&["Logo", "Name"], 0);
        if let Some(draft) = state.editing.as_mut() {
            draft.select_only(draft.slide.objects.first().map(|object| object.id));
        }
        let before = state.editing.as_ref().map(|draft| draft.slide.clone());
        state.theme_names = vec!["Built-in".to_owned(), "Evening".to_owned()];
        state.confirm_delete_theme = Some(1);
        for key in [Key::Delete, Key::Backspace, Key::ArrowRight, Key::ArrowDown] {
            frame_events(&ctx, &show, &mut state, pressed(key, Modifiers::NONE), 0.1);
        }
        assert!(state.alert_open(), "still asking");
        assert_eq!(state.editing.as_ref().map(|draft| draft.slide.clone()), before, "the words stay, unmoved");
        assert_eq!(state.props.len(), 2);
        // A song's slide open, the song asked about: ↓ doesn't open the next slide.
        let (show, mut state) = song_open(&[("Verse 1", 3)], 0);
        state.confirm_delete_songs = vec![0];
        frame_events(&ctx, &show, &mut state, pressed(Key::ArrowDown, Modifiers::NONE), 0.2);
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(0));
        // A theme asked about, with a box of a template picked: Delete doesn't take the box.
        let theme = Theme::builtin();
        let mut draft = EditDraft::of_slide(theme.templates[0].as_slide(), 0);
        draft.select_only(draft.slide.objects.first().map(|object| object.id));
        let boxes = draft.slide.objects.len();
        let mut state = HomeState {
            theme_edit: Some(ThemeEdit::of(1, theme)),
            editing: Some(draft),
            theme_names: vec!["Built-in".to_owned(), "Evening".to_owned()],
            confirm_delete_theme: Some(1),
            ..HomeState::default()
        };
        frame_events(&ctx, &show, &mut state, pressed(Key::Delete, Modifiers::NONE), 0.3);
        assert_eq!(state.editing.as_ref().map(|draft| draft.slide.objects.len()), Some(boxes));
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

    /// A key pressed with these modifiers, as the window passes it on.
    fn pressed(key: Key, modifiers: Modifiers) -> Vec<Event> {
        vec![
            Event::ModifiersChanged(modifiers),
            Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers },
        ]
    }

    #[test]
    fn option_one_then_one_starts_the_first_timer_and_shift_modifier_m_then_one_shows_a_picture() {
        let (ctx, show) = (egui::Context::default(), {
            let mut show = Show::new(vec![Arc::new(parse_song_text("Alpha", "Words\n"))]);
            show.add_media(sp_model::MediaItem::new("Welcome.png".into(), "Welcome.png".into()));
            show
        });
        let mut state = HomeState::default();
        state.timers.push(crate::timers::Timer::new("Sermon".to_owned()));
        state.live_tab = 2;
        frame_commands(&ctx, &show, &mut state, vec![], 1.0);
        frame_commands(&ctx, &show, &mut state, pressed(Key::Num1, Modifiers::ALT), 1.1);
        assert_eq!(state.live_tab, 0, "the Timers tab");
        frame_commands(&ctx, &show, &mut state, pressed(Key::Num1, Modifiers::NONE), 1.2);
        assert!(state.timers[0].running(), "1 started the first timer, as its Start key does");
        frame_commands(&ctx, &show, &mut state, vec![], 1.3);
        assert!(state.timers[0].running(), "and only once");
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        frame_commands(&ctx, &show, &mut state, pressed(Key::M, shift), 2.0);
        let asked = frame_commands(&ctx, &show, &mut state, pressed(Key::Num1, Modifiers::NONE), 2.1);
        assert!(asked.contains(&Command::ShowMedia(0)), "{asked:?}");
    }

    #[test]
    fn the_list_of_keys_takes_every_click_and_a_click_outside_it_puts_it_away() {
        let ctx = egui::Context::default();
        let show = three_songs();
        let mut state = HomeState { key_sheet: true, ..HomeState::default() };
        frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        // At the right column's foot, under the dimmed window: nothing there is pressed.
        let at = pos2(1240.0, 780.0);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut asked = Vec::new();
        for (time, events) in
            [(0.1, vec![Event::PointerMoved(at)]), (0.2, vec![button(true)]), (0.3, vec![button(false)])]
        {
            asked.extend(frame_commands(&ctx, &show, &mut state, events, time));
        }
        assert!(asked.is_empty(), "{asked:?}");
        assert!(!state.key_sheet, "a click outside the sheet closes it");
    }

    #[test]
    fn up_and_down_leave_the_open_item_alone_under_a_menu_or_the_list_of_keys() {
        let ctx = egui::Context::default();
        let show = three_songs();
        let mut state = HomeState { playlist_open: true, key_sheet: true, ..HomeState::default() };
        for title in ["One", "Two", "Three"] {
            state.services.add(crate::service::Item::song(format!("title:{title}"), title.to_owned()));
        }
        state.services.chosen = Some(0);
        frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        frame_commands(&ctx, &show, &mut state, pressed(Key::ArrowDown, Modifiers::NONE), 0.1);
        assert_eq!(state.services.chosen, Some(0));
    }

    #[test]
    fn a_reference_in_the_find_box_opens_the_bible_view_on_its_passage() {
        let (ctx, show) = (egui::Context::default(), three_songs());
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        let mut commands = frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        // The passage is the first thing listed: Return opens it.
        commands.extend(frame_commands(&ctx, &show, &mut state, vec![key(Key::Enter)], 0.1));
        assert!(state.library_search.is_none(), "the search ends");
        let lead = state.bible.lead().and_then(|lead| state.bible.shelf.get(lead).ok()).expect("a Bible");
        let range = sp_bible::parse_reference_in(&state.bible.lookup, &lead.info.language).expect("a reference");
        assert_eq!((range.start.book, range.start.chapter, range.start.verse), (42, 3, 16), "{}", state.bible.lookup);
        assert_eq!(range.end.verse, 18);
        // Typed in English: the operator reads it in English, the slides are as the translations say.
        assert_eq!(state.bible.lookup, "John 3:16-18");
        assert_eq!(state.bible.shown, "От Иоанна 3:16-18");
        assert_eq!(state.bible.pending.as_ref().map(|(_, p)| p.title.as_str()), Some("От Иоанна 3:1-36"));
        commands.extend(frame_commands(&ctx, &show, &mut state, vec![], 0.2));
        assert!(state.bible_open && !ctx.text_edit_focused(), "the Bible view opens, and the keys are free");
        assert!(commands.is_empty(), "nothing is put on the screens: {commands:?}");
    }

    /// Two songs with three slides each, then the Bible view's two places, as the app lays the
    /// show out; the first song's first slide on screen, as during a service.
    fn service_with_bible_places() -> Show {
        let words = |first: &str| format!("[Verse 1]\n{first}\n\n[Verse 2]\nSecond lines\n\n[Chorus]\nThird lines\n");
        let mut all = vec![
            Arc::new(parse_song_text("Alpha", &words("Morning lines"))),
            Arc::new(parse_song_text("Beta", &words("Evening lines"))),
        ];
        for _ in 0..2 {
            all.push(Arc::new(sp_model::Presentation {
                id: sp_model::Uuid::new_v4(),
                title: "Bible".to_owned(),
                groups: Vec::new(),
                ..(*all[0]).clone()
            }));
        }
        let mut show = Show::new(all);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        show
    }

    /// What the app does after drawing a frame: carries out its commands, and puts a passage
    /// just looked up into its place (and on screen, if a verse was double-clicked).
    fn as_the_app_does(show: &mut Show, state: &mut HomeState, commands: Vec<Command>) {
        for command in commands {
            show.apply(command);
        }
        if let Some((slot, passage)) = state.bible.pending.take() {
            show.replace_presentation(slot, Arc::new(passage));
            if let Some(slide) = state.bible.go_live.take() {
                show.apply(Command::GoTo(SlideRef { presentation: slot, slide }));
            }
        }
    }

    /// One frame of the window with these events, then what the app does after it. Returns
    /// the commands the frame gave.
    fn frame_then_app(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
    ) -> Vec<Command> {
        let commands = frame_commands(ctx, show, state, events, time);
        as_the_app_does(show, state, commands.clone());
        commands
    }

    /// Whether any of these commands changes what is on the slide or media layer.
    fn goes_on_screen(commands: &[Command]) -> bool {
        commands.iter().any(|command| {
            matches!(
                command,
                Command::GoTo(_)
                    | Command::GoToStep { .. }
                    | Command::Next
                    | Command::Previous
                    | Command::First
                    | Command::Last
                    | Command::ShowMedia(_)
            )
        })
    }

    /// Enter, as the app carries it out once its key is free (see `enter_key` in `main.rs`).
    fn press_enter(show: &mut Show, state: &HomeState) {
        let listed = if state.bible_open {
            crate::running::Listed::Bible
        } else if state.playlist_open {
            crate::running::Listed::Playlist(state.services.chosen)
        } else {
            crate::running::Listed::Library
        };
        if let Some(crate::running::Do::Show(command)) = crate::running::enter(&[], listed, state.pick(show)) {
            show.apply(command);
        }
    }

    /// Space (`forward`) or ←, on the Bible page or with a library listed, as the app carries it
    /// out (see `step_keys` in `main.rs`). Returns what was said instead, if anything.
    fn press_step(show: &mut Show, state: &HomeState, forward: bool) -> Option<String> {
        let picked = state.picked_word(show);
        let showing = show.live().is_some();
        let command = if forward { Command::Next } else { Command::Previous };
        let decided = if state.bible_open {
            crate::running::held_for_enter(picked, showing).unwrap_or(crate::running::Do::Show(command))
        } else {
            let here =
                if show.cursor().is_some() { crate::running::Here::Elsewhere } else { crate::running::Here::Nothing };
            crate::running::step(
                &crate::running::Situation {
                    stops: &[],
                    titles: &[],
                    open: None,
                    here,
                    at_end: show.cursor().is_some() && show.upcoming().is_none(),
                    at_start: show.cursor().is_some() && show.preceding().is_none(),
                    continuous: false,
                    picked,
                    showing,
                },
                forward,
            )
        };
        match decided {
            crate::running::Do::Show(command) => {
                show.apply(command);
                None
            }
            crate::running::Do::Say(words) => Some(words),
            crate::running::Do::Start { .. } => panic!("nothing in a playlist is started here"),
        }
    }

    #[test]
    fn slash_a_reference_and_return_pick_its_first_verse_and_enter_then_space_and_left_run_it() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState::default();
        state.bible.slots = Some([2, 3]);
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        // `/` opens the find box empty (the app does this before the window sees the key).
        state.library_search = Some((String::new(), 0));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("jn 3:16-18".to_owned())], 0.1);
        assert!(ctx.text_edit_focused(), "typed in the find box");
        // Return on the passage, the first thing listed.
        let mut commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.2);
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 0.3));
        assert!(!goes_on_screen(&commands), "Return puts nothing on screen: {commands:?}");
        assert!(state.bible.go_live.is_none());
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "the song stays on screen");
        assert!(state.bible_open, "the Bible view is open");
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).expect("a place for passages");
        assert_eq!(show.presentations()[slot].slide_count(), 36, "John 3, a slide a verse, round the passage");
        assert_eq!(state.bible.passage_slides(), 15..18, "John 3:16-18");
        let verse = |slide: usize| Some(SlideRef { presentation: slot, slide });
        assert_eq!(state.pick(&show), verse(15), "its first verse is picked");
        assert!(state.library_search.is_none() && !ctx.text_edit_focused(), "the search has ended");
        assert!(ctx.memory(|memory| memory.focused().is_none()), "the keyboard is back with the window");
        // Enter puts the picked verse on screen; the pick has done its work.
        press_enter(&mut show, &state);
        assert_eq!(show.live(), verse(15));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.4);
        assert!(state.picked.is_none(), "let go once on screen");
        // Space goes on to the next verse, ← back; past the passage's last, on into the chapter.
        assert_eq!(press_step(&mut show, &state, true), None);
        assert_eq!(show.live(), verse(16));
        assert_eq!(press_step(&mut show, &state, false), None);
        assert_eq!(show.live(), verse(15));
        for _ in 0..3 {
            assert_eq!(press_step(&mut show, &state, true), None);
        }
        assert_eq!(show.live(), verse(18), "John 3:19");
        assert_eq!(state.bible.shown, "От Иоанна 3:16-18", "the passage is still the one asked for");
    }

    #[test]
    fn a_single_verse_from_the_find_box_stays_the_passage_picked_among_its_chapters_slides() {
        let ctx = egui::Context::default();
        let enter = |modifiers| {
            vec![
                Event::ModifiersChanged(modifiers),
                Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers },
            ]
        };
        // The King James Version first, so the slides' words are easy to read here.
        let english_first = |state: &mut HomeState| {
            let kjv = state.bible.shelf.position("kjv");
            state.bible.rows.truncate(1);
            state.bible.rows[0].bible = kjv;
        };
        let mut show = service_with_bible_places();
        let mut state = HomeState { library_search: Some(("gen 1 4".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        english_first(&mut state);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        let mut commands = frame_then_app(&ctx, &mut show, &mut state, enter(Modifiers::NONE), 0.1);
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2));
        assert!(!goes_on_screen(&commands), "{commands:?}");
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 31, "all of Genesis 1, a slide a verse");
        assert_eq!(state.bible.lookup, "Genesis 1:4", "the field names the verse");
        assert_eq!(state.bible.saved().map(|saved| saved.lookup).as_deref(), Some("Genesis 1:4"));
        let verse = |slide: usize| SlideRef { presentation: slot, slide };
        assert_eq!(state.pick(&show), Some(verse(3)), "verse 4 picked");
        let words = |show: &Show, at: SlideRef| {
            let (_, slide) = show.presentations()[at.presentation].slide(at.slide).expect("a slide");
            slide.texts().into_iter().map(|(_, text)| text.to_owned()).collect::<Vec<_>>().join(" ")
        };
        assert!(words(&show, verse(3)).contains("God saw the light, that it was good"));
        press_enter(&mut show, &state);
        assert_eq!(show.live(), Some(verse(3)));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.3);
        assert_eq!(press_step(&mut show, &state, true), None);
        assert_eq!(show.live(), Some(verse(4)), "Space goes on to verse 5");
        assert!(words(&show, verse(4)).contains("God called the light Day"));

        // Command-Return still adds the verse alone to the playlist.
        let mut show = service_with_bible_places();
        let mut state = HomeState { library_search: Some(("gen 1 4".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        english_first(&mut state);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, enter(Modifiers::COMMAND), 0.1);
        let added = state.services.front().items.last().cloned();
        assert!(
            matches!(&added, Some(crate::service::Item::Passage(saved)) if saved.lookup == "Genesis 1:4"),
            "{added:?}"
        );

        // With the verses made into one slide, the verse opens alone (a chapter would overfill it).
        let mut show = service_with_bible_places();
        let mut state = HomeState { library_search: Some(("gen 1 4".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        state.bible.options.break_on_new_verse = false;
        english_first(&mut state);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, enter(Modifiers::NONE), 0.1);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!((state.bible.lookup.as_str(), show.presentations()[slot].slide_count()), ("Genesis 1:4", 1));
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 0 }));
    }

    /// The King James Version alone, so the slides' names are easy to read in a test.
    fn english_only(state: &mut HomeState) {
        let kjv = state.bible.shelf.position("kjv");
        state.bible.rows.truncate(1);
        state.bible.rows[0].bible = kjv;
    }

    /// The name of the slide on screen (a Bible slide is named by its verse).
    fn live_name(show: &Show) -> Option<String> {
        let at = show.live()?;
        show.presentations()[at.presentation].slide(at.slide).map(|(group, _)| group.name.clone())
    }

    #[test]
    fn a_single_verse_looked_up_stays_the_passage_add_keeps_it_and_space_after_enter_goes_on() {
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        // Typed in the reference field, or in the find box (`/` or Command-F).
        for find_box in [false, true] {
            let ctx = egui::Context::default();
            let mut show = service_with_bible_places();
            let mut state = HomeState { bible_open: true, ..HomeState::default() };
            state.bible.slots = Some([2, 3]);
            english_only(&mut state);
            if find_box {
                state.library_search = Some(("jn 3:16".to_owned(), 0));
                frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
            } else {
                state.bible.focus_lookup = true;
                frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
                frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("John 3:16".to_owned())], 0.05);
            }
            let commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.1);
            assert!(!goes_on_screen(&commands), "{commands:?}");
            for at in 0..3 {
                frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2 + f64::from(at) * 0.02);
            }
            let how = if find_box { "find box" } else { "reference field" };
            // The passage is the verse asked for: the field, the title and Add say so.
            assert_eq!(state.bible.shown, "John 3:16", "{how}");
            assert_eq!(state.bible.lookup, "John 3:16", "{how}");
            let title = crate::bible_page::title(&mut state.bible).map(|(words, _)| words);
            assert_eq!(title.as_deref(), Some("John 3:16"), "{how}");
            // Only it is lit, on the page and on the verse keys; the chapter is there to read
            // and click, every verse with its slide.
            let (lit, with_slides) = state.bible.page.verses();
            assert_eq!(lit, [16], "{how}");
            assert_eq!(with_slides, (1..=36).collect::<Vec<u16>>(), "{how}");
            drawn_frame(&ctx, &mut show, &mut state, 0.3);
            let (rects, _) = drawn_frame(&ctx, &mut show, &mut state, 0.31);
            let key_of = |verse: u16| ctx.read_response(crate::bible::list_row_id(("verse", verse))).map(|r| r.rect);
            let lit_key = |verse: u16| {
                let rect = key_of(verse).expect("the verse key is drawn");
                rects.iter().any(|(at, fill, _)| *at == rect && *fill == crate::studio::SELECTED)
            };
            assert!(lit_key(16) && !lit_key(15) && !lit_key(17), "{how}: only verse 16's key is lit");
            // Its slides are the chapter's, the verse picked among them.
            let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
            let verse = |slide: usize| Some(SlideRef { presentation: slot, slide });
            assert_eq!(show.presentations()[slot].slide_count(), 36, "{how}");
            assert_eq!(state.pick(&show), verse(15), "{how}");
            // + Add keeps exactly the verse.
            click_id(&ctx, &mut show, &mut state, egui::Id::unique("bible add"), 0.5);
            let added = state.services.front().items.last().cloned();
            assert!(
                matches!(&added, Some(crate::service::Item::Passage(saved)) if saved.lookup == "John 3:16"),
                "{how}: {added:?}"
            );
            let front = state.services.front().name.clone();
            assert_eq!(state.notice, Some(format!("Added John 3:16 to {front}.")), "{how}");
            // Enter puts it on screen; Space goes on to John 3:17, ← back.
            press_enter(&mut show, &state);
            assert_eq!(live_name(&show).as_deref(), Some("John 3:16"), "{how}");
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.7);
            assert_eq!(press_step(&mut show, &state, true), None);
            assert_eq!(live_name(&show).as_deref(), Some("John 3:17"), "{how}: Space");
            assert_eq!(press_step(&mut show, &state, false), None);
            assert_eq!(live_name(&show).as_deref(), Some("John 3:16"), "{how}: \u{2190}");
            // The history notes the verse asked for, not the chapter its slides were made from.
            let title = show.presentations()[slot].title.clone();
            assert_eq!(title, "John 3:1-36");
            let english = Some(sp_bible::Numbering::English);
            assert_eq!(state.bible.passage_held(slot, &title), ("John 3:16".to_owned(), english), "{how}");
            assert_eq!(state.bible.passage_held(slot, "Something else"), ("Something else".to_owned(), None));
            // New choices make it again: still the verse, among the chapter's slides.
            state.bible.options.show_verse_numbers = true;
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.8);
            assert_eq!((state.bible.shown.as_str(), state.bible.passage_slides()), ("John 3:16", 15..16), "{how}");
        }
    }

    #[test]
    fn a_passage_of_verses_and_a_whole_chapter_are_the_passage_add_keeps() {
        for (typed, shown, title, slides) in [
            ("John 3:16-18", "John 3:16-18", "John 3:16-18", 15..18),
            ("John 3", "John 3:1-36", "John 3", 0..36),
            ("John 3:35-4:2", "John 3:35-4:2", "John 3:35-4:2", 0..4),
        ] {
            let mut page = bible_page();
            page.rows.truncate(1);
            page.rows[0].bible = page.shelf.position("kjv");
            page.lookup = typed.to_owned();
            page.look_up(None);
            assert_eq!(page.shown, shown, "{typed}");
            assert_eq!(page.saved().map(|saved| saved.lookup).as_deref(), Some(shown), "{typed}");
            assert_eq!(crate::bible_page::title(&mut page).map(|(words, _)| words).as_deref(), Some(title));
            assert_eq!(page.passage_slides(), slides, "{typed}");
        }
    }

    #[test]
    fn the_slides_are_the_chapters_with_the_passage_ringed_and_any_of_them_goes_up_with_a_click() {
        use crate::studio::{TEXT, TEXT_FAINT};
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let looked_up = |ctx: &egui::Context, one_a_verse: bool| {
            let mut show = service_with_bible_places();
            let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
            state.bible.slots = Some([2, 3]);
            state.bible.view = BibleView::Slides;
            state.bible.options.break_on_new_verse = one_a_verse;
            english_only(&mut state);
            frame_then_app(ctx, &mut show, &mut state, vec![], 0.0);
            frame_then_app(ctx, &mut show, &mut state, vec![enter.clone()], 0.1);
            // The slides follow the pick into view, and settle.
            for at in 0..14 {
                frame_then_app(ctx, &mut show, &mut state, vec![], 0.2 + f64::from(at) * 0.005);
            }
            (show, state)
        };
        // Each ring drawn round a tile's picture: by the tile's slide, its width and colour.
        let rings = |rects: &[(Rect, Color32, Stroke)], state: &HomeState| -> Vec<(usize, f32, Color32)> {
            state
                .slide_rects
                .iter()
                .flat_map(|(at, tile)| {
                    rects
                        .iter()
                        .filter(|(rect, _, stroke)| rect.min == tile.min && stroke.width > 0.0)
                        .map(|(_, _, stroke)| (at.slide, stroke.width, stroke.color))
                })
                .collect()
        };

        // A slide a verse: the chapter's 36 slides, at the Show view's size; the passage's first
        // picked (the ink ring), the rest of the passage in a faint ring, the others none.
        let ctx = egui::Context::default();
        let (mut show, mut state) = looked_up(&ctx, true);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 36);
        let (rects, _) = drawn_frame(&ctx, &mut show, &mut state, 0.4);
        let drawn = rings(&rects, &state);
        let ringed = |slide: usize, width: f32, colour: Color32| drawn.contains(&(slide, width, colour));
        assert!(ringed(15, 2.0, TEXT), "John 3:16, picked: {drawn:?}");
        assert!(ringed(16, 2.0, TEXT_FAINT) && ringed(17, 2.0, TEXT_FAINT), "John 3:17-18: {drawn:?}");
        for other in [13, 14, 18, 19] {
            assert!(!drawn.iter().any(|(slide, ..)| *slide == other), "John 3:{}: {drawn:?}", other + 1);
        }
        let tiles: Vec<usize> = state.slide_rects.iter().map(|(at, _)| at.slide).collect();
        assert!(tiles.contains(&14) && tiles.contains(&19), "the chapter's slides round the passage: {tiles:?}");
        // Back in the usual window, settled: the pick was brought into sight with the passage's
        // last slide, so John 3:18 can be clicked as well as 3:15, before the passage. Either goes
        // on screen with a click, as any slide does, and the passage stays.
        for at in 0..20 {
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.5 + f64::from(at) * 0.005);
        }
        let mut time = 0.6;
        for slide in [17, 14] {
            let rect = state
                .slide_rects
                .iter()
                .find(|(at, _)| at.slide == slide)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("John 3:{} is laid out", slide + 1));
            let press = |pressed| Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let mut commands =
                frame_then_app(&ctx, &mut show, &mut state, vec![Event::PointerMoved(rect.center())], time);
            commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![press(true)], time + 0.05));
            commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![press(false)], time + 0.1));
            assert_eq!(commands, vec![Command::GoTo(SlideRef { presentation: slot, slide })], "John 3:{}", slide + 1);
            assert_eq!(state.bible.shown, "John 3:16-18", "the passage stays");
            time += 0.5;
        }

        // Made into one slide: that slide alone, and nothing ringed.
        let ctx = egui::Context::default();
        let (mut show, mut state) = looked_up(&ctx, false);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 1);
        let (rects, _) = drawn_frame(&ctx, &mut show, &mut state, 0.4);
        let drawn = rings(&rects, &state);
        assert!(!drawn.iter().any(|(_, _, colour)| *colour == TEXT_FAINT), "{drawn:?}");
    }

    #[test]
    fn a_psalm_is_titled_psalm_and_psalms_across_several_in_each_language() {
        // The King James Version alone: English.
        let mut page = bible_page();
        page.rows.truncate(1);
        page.rows[0].bible = page.shelf.position("kjv");
        let title = |page: &mut BiblePage, typed: &str| {
            page.lookup = typed.to_owned();
            page.look_up(None);
            let words = crate::bible_page::title(page).map(|(words, _)| words).unwrap_or_default();
            (words, page.lookup.clone(), page.shown.clone())
        };
        let field_reads_back = |page: &BiblePage| {
            let lead = page.shelf.bibles[page.lead().unwrap()].language.clone();
            let read = |name: &str| sp_bible::parse_reference_in(name, &lead).map(|range| (range.start, range.end));
            assert_eq!(read(&page.lookup), read(&page.shown), "{} reads back as {}", page.lookup, page.shown);
        };
        for (typed, words, field, kept) in [
            ("ps 23", "Psalm 23", "Psalm 23:1-6", "Psalms 23:1-6"),
            ("ps 23:1-3", "Psalm 23:1-3", "Psalm 23:1-3", "Psalms 23:1-3"),
            ("ps 23-24", "Psalms 23\u{2013}24", "Psalms 23:1-24:10", "Psalms 23:1-24:10"),
            ("ps 23:6-24:1", "Psalms 23:6-24:1", "Psalms 23:6-24:1", "Psalms 23:6-24:1"),
            ("John 3", "John 3", "John 3:1-36", "John 3:1-36"),
        ] {
            assert_eq!(title(&mut page, typed), (words.to_owned(), field.to_owned(), kept.to_owned()), "{typed}");
            field_reads_back(&page);
        }
        // The Russian Synodal first, read in Russian: one psalm is "Псалом", several keep the
        // book's name, "Псалтирь"; what is kept is as the Synodal names it.
        let mut page = bible_page();
        page.read_in = Some("ru".to_owned());
        for (typed, words, field, kept) in [
            ("Пс 22", "Псалом 22", "Псалом 22:1-6", "Псалтирь 22:1-6"),
            ("Пс 22:1-3", "Псалом 22:1-3", "Псалом 22:1-3", "Псалтирь 22:1-3"),
            ("Пс 22-23", "Псалтирь 22\u{2013}23", "Псалтирь 22:1-23:10", "Псалтирь 22:1-23:10"),
        ] {
            assert_eq!(title(&mut page, typed), (words.to_owned(), field.to_owned(), kept.to_owned()), "{typed}");
            field_reads_back(&page);
        }
    }

    #[test]
    fn the_page_keypad_and_title_are_numbered_as_the_page_is_read() {
        // The owner's rule: the Bible page uses one numbering, the language it is read in. The
        // Russian Synodal first, the King James Version second, as the app starts.
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        let psalms = sp_bible::BOOKS.iter().position(|book| book.code == "PSA").unwrap();
        let time = std::cell::Cell::new(0.0);
        let settle = |ctx: &egui::Context, show: &mut Show, state: &mut HomeState| {
            let mut commands = Vec::new();
            for _ in 0..3 {
                time.set(time.get() + 0.05);
                commands.extend(frame_then_app(ctx, show, state, vec![], time.get()));
            }
            time.set(time.get() + 0.05);
            let (rects, _) = drawn_frame(ctx, show, state, time.get());
            commands.extend(frame_then_app(ctx, show, state, vec![], time.get() + 0.01));
            (rects, commands)
        };
        // The chapter key drawn lit, by its number.
        let lit_key = |ctx: &egui::Context, rects: &[(Rect, Color32, Stroke)], chapter: u16| {
            let key = ctx.read_response(crate::bible::list_row_id(("chapter", chapter))).expect("the key").rect;
            rects.iter().any(|(rect, fill, _)| *rect == key && *fill == crate::studio::SELECTED)
        };
        let title = |state: &mut HomeState| crate::bible_page::title(&mut state.bible).map(|(words, _)| words);

        // "Psalm 23" typed in English: the English 23rd psalm; key 23 lit, titled "Psalm 23", the
        // page's verse 1 its first verse, with the Synodal's own number beside its line.
        state.bible.lookup = "Psalm 23".to_owned();
        state.bible.look_up(None);
        let (rects, mut commands) = settle(&ctx, &mut show, &mut state);
        assert_eq!(state.bible.shown, "Псалтирь 22:1-6", "the Synodal's 22nd makes the slides");
        assert_eq!((state.bible.list_book, state.bible.list_chapter), (psalms, 23));
        assert!(lit_key(&ctx, &rects, 23) && !lit_key(&ctx, &rects, 22), "key 23 lit");
        assert_eq!(title(&mut state).as_deref(), Some("Psalm 23"));
        let (chapter, rows) = state.bible.page.rows();
        assert_eq!(chapter, Some((psalms, 23)));
        let (number, words, note) = &rows[0];
        assert_eq!(*number, 1);
        assert!(words.contains("The LORD is my shepherd"), "{words}");
        assert_eq!(note.as_deref(), Some("Псалом 22:1"));

        // Аа on Russian: the same passage, numbered as the Synodal numbers; key 22 lit, titled
        // "Псалом 22", the King James line with its own number. Nothing is made again.
        state.bible.set_read_in(Some("ru".to_owned()));
        let (rects, more) = settle(&ctx, &mut show, &mut state);
        commands.extend(more);
        assert!(state.bible.pending.is_none(), "nothing made again");
        assert_eq!(state.bible.shown, "Псалтирь 22:1-6", "the same verses");
        assert_eq!(state.bible.list_chapter, 22);
        assert!(lit_key(&ctx, &rects, 22) && !lit_key(&ctx, &rects, 23), "key 22 lit");
        assert_eq!(title(&mut state).as_deref(), Some("Псалом 22"));
        let (chapter, rows) = state.bible.page.rows();
        assert_eq!(chapter, Some((psalms, 22)));
        assert!(rows[0].1.contains("Господь - Пастырь мой"), "{}", rows[0].1);
        assert_eq!(rows[0].2.as_deref(), Some("Psalm 23:1"));

        // A Synodal psalm whose title is two verses: its 50:3 is the King James 51:1, and the
        // King James line says so.
        state.bible.lookup = "Пс 50".to_owned();
        state.bible.look_up(None);
        settle(&ctx, &mut show, &mut state);
        let (_, rows) = state.bible.page.rows();
        let third = rows.iter().find(|(number, ..)| *number == 3).expect("verse 3");
        assert_eq!(third.2.as_deref(), Some("Psalm 51:1"));
        // Its verses 1 and 2 (the title, the King James heading of its verse 1) carry no second
        // line: Psalm 51:1 is under verse 3 only, once.
        let under_51_1 = rows.iter().filter(|(.., note)| note.as_deref() == Some("Psalm 51:1")).count();
        assert_eq!(under_51_1, 1);
        // Read in English: the King James numbers, the Synodal line with its own.
        state.bible.set_read_in(Some("en".to_owned()));
        settle(&ctx, &mut show, &mut state);
        assert_eq!(state.bible.list_chapter, 51);
        let (_, rows) = state.bible.page.rows();
        assert_eq!((rows[0].0, rows[0].2.as_deref()), (1, Some("Псалом 50:3")));

        // Key 23 clicked while reading in English: the English 23rd psalm (the Synodal's 22nd).
        state.bible.set_read_in(None);
        state.bible.language = Some("en".to_owned());
        settle(&ctx, &mut show, &mut state);
        time.set(time.get() + 1.0);
        commands.extend(click_list_row(&ctx, &mut show, &mut state, ("chapter", 23u16), time.get()));
        time.set(time.get() + 0.5);
        assert_eq!(state.bible.shown, "Псалтирь 22:1-6");
        assert_eq!(title(&mut state).as_deref(), Some("Psalm 23"));
        assert_eq!(state.bible.saved().map(|saved| saved.lookup).as_deref(), Some("Psalms 23:1-6"));
        // The verses too (in the lists, where every verse row is in view): Psalm 51:1 typed in
        // English is the Synodal's 50:3; read in Russian its row 3 is lit among 21, read in
        // English row 1 among 19.
        state.bible.books = BookPicker::Lists;
        state.bible.lookup = "Psalm 51:1".to_owned();
        state.bible.look_up(None);
        state.bible.set_read_in(Some("ru".to_owned()));
        let lit_row = |ctx: &egui::Context, rects: &[(Rect, Color32, Stroke)], verse: u16| {
            let row = ctx.read_response(crate::bible::list_row_id(("verse", verse))).expect("the row").rect;
            rects.iter().any(|(rect, _, stroke)| *rect == row && stroke.color == crate::studio::SELECTED)
        };
        let rows_drawn = |ctx: &egui::Context, last: u16| {
            ctx.read_response(crate::bible::list_row_id(("verse", last))).is_some()
                && ctx.read_response(crate::bible::list_row_id(("verse", last + 1))).is_none()
        };
        let (rects, more) = settle(&ctx, &mut show, &mut state);
        commands.extend(more);
        assert!(lit_row(&ctx, &rects, 3) && !lit_row(&ctx, &rects, 1), "the Synodal's verse 3");
        assert!(rows_drawn(&ctx, 21), "the Synodal's 50th psalm has 21 verses");
        state.bible.set_read_in(Some("en".to_owned()));
        let (rects, more) = settle(&ctx, &mut show, &mut state);
        commands.extend(more);
        assert!(lit_row(&ctx, &rects, 1) && !lit_row(&ctx, &rects, 3), "the King James verse 1");
        assert!(rows_drawn(&ctx, 19), "the King James 51st psalm has 19");
        // Nothing went on screen through any of it.
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "the song stays on screen");
    }

    #[test]
    fn a_passage_from_history_is_looked_up_as_the_page_makes_any_and_its_first_verse_picked() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        english_only(&mut state);
        state.bible.side = BibleSide::History;
        let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap().and_hms_opt(9, 0, 0).unwrap();
        state.bible.note_shown("John 3:16", None, at);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        let commands = click_id(&ctx, &mut show, &mut state, egui::Id::unique(("bible history row", 0usize)), 0.2);
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!(state.bible.shown, "John 3:16");
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 36, "its slides its chapter's");
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 15 }), "the verse picked");
    }

    #[test]
    fn a_playlists_passage_runs_as_its_own_slides_and_opens_on_the_page_as_any_passage() {
        let mut page = bible_page();
        page.rows.truncate(1);
        page.rows[0].bible = page.shelf.position("kjv");
        let saved = crate::service::SavedPassage { lookup: "John 3:16".to_owned(), ..page.settings() };
        // Run from the playlist (a click on its tile, Next going into it): its own one slide, so
        // the playlist counts its slides and goes on into the next item after it.
        page.open_saved(&saved, None, SlidesOf::Passage);
        let (_, passage) = page.pending.take().expect("made");
        assert_eq!((passage.title.as_str(), passage.slide_count()), ("John 3:16", 1));
        assert_eq!(page.passage_slides(), 0..1);
        // Open on the Bible Page: as the page makes any passage, the verse among its chapter's.
        page.open_saved(&saved, None, SlidesOf::Page);
        let (_, passage) = page.pending.take().expect("made");
        assert_eq!((passage.title.as_str(), passage.slide_count()), ("John 3:1-36", 36));
        assert_eq!((page.shown.as_str(), page.passage_slides()), ("John 3:16", 15..16));
    }

    #[test]
    fn while_a_verse_is_picked_space_runs_the_song_on_and_with_nothing_on_screen_says_to_press_enter() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        let enter = vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }];
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, enter, 0.1);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        let picked = state.pick(&show).expect("a verse picked");
        // The song on screen goes on with Space; the verse waits for Enter.
        assert_eq!(press_step(&mut show, &state, true), None);
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 1 }));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.3);
        assert_eq!(state.pick(&show), Some(picked), "still picked");
        // Cleared: Space and ← put nothing on (not the verse, not the song), and say so.
        show.apply(Command::ClearAll);
        let said = press_step(&mut show, &state, true);
        assert_eq!(said.as_deref(), Some("Press Enter to put the picked verse on screen."));
        assert_eq!(press_step(&mut show, &state, false), said);
        assert_eq!(show.live(), None);
        press_enter(&mut show, &state);
        assert_eq!(show.live(), Some(picked));
    }

    #[test]
    fn up_and_down_under_slides_follow_the_owners_rules_as_on_the_page() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        // Nothing on screen.
        show.apply(Command::ClearAll);
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        state.bible.view = BibleView::Slides;
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.1);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        // The passage's slides are its chapter's: ↑ and ↓ go on through the chapter.
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        let verse = |slide: usize| Some(SlideRef { presentation: slot, slide });
        assert_eq!(state.pick(&show), verse(15), "John 3:16");
        // With no verse picked and none on screen, ↓ (or ↑) picks the passage's first.
        state.picked = None;
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowUp)], 0.25);
        assert_eq!(state.pick(&show), verse(15), "John 3:16 again");
        // Nothing on screen: ↓ and ↑ move the pick, putting nothing on.
        let mut commands = Vec::new();
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.3));
        assert_eq!(state.pick(&show), verse(16));
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.4));
        assert_eq!(state.pick(&show), verse(17));
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.5));
        assert_eq!(state.pick(&show), verse(18), "on past the passage, into the chapter");
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowUp)], 0.6));
        assert_eq!(state.pick(&show), verse(17));
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert!(show.live().is_none(), "nothing on screen");
        // Enter puts the verse picked on; then ↓ goes on to the verse after it, as → does.
        press_enter(&mut show, &state);
        assert_eq!(show.live(), verse(17));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.7);
        let commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.8);
        assert_eq!(commands, vec![Command::Next]);
        assert_eq!(show.live(), verse(18), "the next verse on screen");
        // With the screen cleared, ↓ picks again from the passage's first verse, and the pick
        // stays at the chapter's last verse, and says so.
        show.apply(Command::ClearAll);
        for at in 0..25 {
            frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.9 + f64::from(at) * 0.01);
        }
        assert_eq!(state.pick(&show), verse(35), "the last verse stays picked");
        assert_eq!(state.toast.as_ref().map(|(words, _)| words.as_str()), Some("That is the last verse."));
    }

    #[test]
    fn on_the_page_the_arrows_go_the_way_they_point_by_the_owners_rules() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        // Nothing on screen.
        show.apply(Command::ClearAll);
        assert!(show.live().is_none());
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        assert_eq!(state.bible.view, BibleView::Page);
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.1);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        let verse = |slide: usize| Some(SlideRef { presentation: slot, slide });
        assert_eq!(state.pick(&show), verse(15), "John 3:16 picked");
        // A verse picked, nothing on screen: ↓ picks the verse below, ↑ the one above, and
        // nothing goes on screen; the pick is brought into sight as it moves.
        let mut commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.3);
        assert_eq!(state.pick(&show), verse(16), "John 3:17");
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.35);
        let in_sight = ctx.read_response(page_verse(17)).is_some_and(|row| row.interact_rect.height() > 20.0);
        assert!(in_sight, "John 3:17 brought into sight");
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.4));
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowUp)], 0.5));
        assert_eq!(state.pick(&show), verse(16));
        assert!(!goes_on_screen(&commands) && show.live().is_none(), "{commands:?}");
        // Not while the reference field has the keyboard: the arrows are the field's.
        state.bible.focus_lookup = true;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.55);
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.6);
        assert_eq!(state.pick(&show), verse(16), "the field's keys");
        ctx.memory_mut(|memory| memory.surrender_focus(scripture_lookup_id()));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.65);
        // Enter puts it on screen; then, with no pick, ↓ goes on to the verse below, as → does,
        // and ↑ back.
        press_enter(&mut show, &state);
        assert_eq!(show.live(), verse(16));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.7);
        let commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.8);
        assert_eq!(commands, vec![Command::Next]);
        assert_eq!(show.live(), verse(17), "John 3:18 on screen");
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowUp)], 0.9);
        assert_eq!(show.live(), verse(16));
        // A verse picked while something else is on screen (the song): ↓ goes on with the song,
        // as → does, and the pick waits for Enter.
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        state.set_pick(&show, SlideRef { presentation: slot, slide: 20 }, true);
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 1.0);
        let song = |slide: usize| Some(SlideRef { presentation: 0, slide });
        assert_eq!((show.live(), state.pick(&show)), (song(1), verse(20)));
        press_enter(&mut show, &state);
        assert_eq!(show.live(), verse(20), "Enter takes the pick");
        // Under Slides the same: with a verse on screen and none picked, ↓ goes on to the next.
        state.bible.view = BibleView::Slides;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 1.1);
        let commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 1.2);
        assert_eq!(commands, vec![Command::Next]);
        assert_eq!(show.live(), verse(21), "the next verse on screen under Slides too");
    }

    #[test]
    fn return_in_scripture_lookup_picks_the_first_verse_and_gives_the_keyboard_back() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        state.bible.focus_lookup = true;
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("jn 3:16-18".to_owned())], 0.1);
        assert!(ctx.text_edit_focused(), "typed in Scripture Lookup");
        let mut commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.2);
        commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 0.3));
        assert!(!goes_on_screen(&commands), "{commands:?}");
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 15 }), "John 3:16");
        assert!(ctx.memory(|memory| memory.focused().is_none()), "the keyboard is back with the window");
        // Typing again with no field taking the keyboard starts the next reference in the field
        // (the app keeps letters from the show in the Bible view): nothing goes on screen.
        let typed = frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("p".to_owned())], 0.35);
        assert!(!goes_on_screen(&typed), "{typed:?}");
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.36);
        assert!(ctx.text_edit_focused(), "Scripture Lookup has the keyboard again");
        frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("s 23".to_owned())], 0.37);
        assert_eq!(state.bible.lookup, "ps 23", "the typing went on in the field");
        // A reference not understood keeps the keyboard, to be put right.
        state.bible.focus_lookup = true;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.4);
        frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("nosuchbook 9".to_owned())], 0.5);
        frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.6);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.7);
        assert!(ctx.text_edit_focused(), "still in Scripture Lookup");
    }

    #[test]
    fn a_song_found_by_its_title_or_its_words_is_picked_and_enter_then_space_and_left_run_it() {
        let ctx = egui::Context::default();
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        // By its title: its first slide; by a line of its words: that slide.
        for (typed, picked) in [("beta", 0), ("evening lines", 0), ("third lines", 2)] {
            let mut show = service_with_bible_places();
            let mut state = HomeState { library_search: Some((typed.to_owned(), 0)), ..HomeState::default() };
            state.bible.slots = Some([2, 3]);
            state.song_keys = vec!["alpha".to_owned(), "beta".to_owned()];
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
            // "third lines" is in both songs: down to Beta's.
            if typed == "third lines" {
                frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::ArrowDown)], 0.05);
            }
            let mut commands = frame_then_app(&ctx, &mut show, &mut state, vec![key(Key::Enter)], 0.1);
            commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2));
            assert!(!goes_on_screen(&commands), "{typed}: {commands:?}");
            assert_eq!(show.selected(), 1, "{typed}: Beta is open");
            assert_eq!(state.pick(&show), Some(SlideRef { presentation: 1, slide: picked }), "{typed}");
            assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "{typed}: Alpha stays on screen");
            assert!(ctx.memory(|memory| memory.focused().is_none()), "{typed}: the keyboard is back");
            press_enter(&mut show, &state);
            assert_eq!(show.live(), Some(SlideRef { presentation: 1, slide: picked }), "{typed}");
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.3);
            if picked < 2 {
                assert_eq!(press_step(&mut show, &state, true), None);
                assert_eq!(show.live(), Some(SlideRef { presentation: 1, slide: picked + 1 }), "{typed}: Space");
            }
            assert_eq!(press_step(&mut show, &state, false), None);
            let back = if picked < 2 { picked } else { 1 };
            assert_eq!(show.live(), Some(SlideRef { presentation: 1, slide: back }), "{typed}: \u{2190}");
        }
    }

    /// Command-3 (Ctrl+3 elsewhere) pressed in the window, and the frame after it, in which the
    /// view switch opens what was asked for. Returns the song the slide editor opened, if any.
    fn press_edit_key(ctx: &egui::Context, show: &Show, state: &mut HomeState) -> Option<usize> {
        frame_commands(ctx, show, state, vec![], 0.0);
        frame_commands(ctx, show, state, pressed(Key::Num3, Modifiers::COMMAND), 0.1);
        frame_commands(ctx, show, state, vec![], 0.2);
        state.editing.as_ref().map(|draft| draft.at.presentation)
    }

    #[test]
    fn edit_opens_the_song_chosen_whatever_playlist_item_is_open() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        show.apply(Command::Select(1));
        show.add_media(sp_model::MediaItem::new("Welcome.png".into(), "Welcome.png".into()));
        let id = show.media()[0].id;
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        state.song_keys = vec!["title:Alpha".to_owned(), "title:Beta".to_owned()];
        state.services.add(crate::service::Item::song("title:Beta".to_owned(), "Beta".to_owned()));
        state.services.add(crate::service::Item::Media { id, name: "Welcome.png".to_owned() });
        state.services.add(crate::service::Item::Placeholder { name: "Sermon".to_owned() });
        // A picture, then a placeholder, opened from the playlist: Edit opens Beta, the song chosen.
        for item in [1, 2] {
            let mut state = HomeState { services: state.services.clone(), ..HomeState::default() };
            (state.playlist_open, state.bible.slots, state.song_keys) =
                (true, Some([2, 3]), vec!["title:Alpha".to_owned(), "title:Beta".to_owned()]);
            state.services.chosen = Some(item);
            assert_eq!(press_edit_key(&ctx, &show, &mut state), Some(1), "item {item}");
        }
        state.services.chosen = Some(0);
        assert_eq!(press_edit_key(&ctx, &show, &mut state), Some(1));
    }

    #[test]
    fn edit_never_does_nothing_and_never_opens_a_bible_passage() {
        let ctx = egui::Context::default();
        // The song chosen has no slides: the song on screen (Alpha) is opened instead.
        let mut show = service_with_bible_places();
        let empty = sp_model::Presentation {
            title: "Empty".to_owned(),
            groups: Vec::new(),
            ..(*show.presentations()[1]).clone()
        };
        show.replace_presentation(1, Arc::new(empty));
        show.apply(Command::Select(1));
        let mut state = HomeState::default();
        state.bible.slots = Some([2, 3]);
        assert_eq!(press_edit_key(&ctx, &show, &mut state), Some(0), "the song on screen");
        // A Bible passage chosen (as `sp bible --show` leaves it) and on screen: a song, not the passage.
        let mut show = service_with_bible_places();
        let mut state = HomeState::default();
        state.bible.slots = Some([2, 3]);
        state.bible.lookup = "jn 3:16".to_owned();
        state.bible.look_up(None);
        as_the_app_does(&mut show, &mut state, vec![]);
        show.apply(Command::Select(2));
        show.apply(Command::GoTo(SlideRef { presentation: 2, slide: 0 }));
        assert_eq!(press_edit_key(&ctx, &show, &mut state), Some(0), "the first song");
        // No song at all: Edit says so, and the view stays.
        let mut show = service_with_bible_places();
        for at in 0..2 {
            let empty = sp_model::Presentation { groups: Vec::new(), ..(*show.presentations()[at]).clone() };
            show.replace_presentation(at, Arc::new(empty));
        }
        let mut state = HomeState::default();
        state.bible.slots = Some([2, 3]);
        assert_eq!(press_edit_key(&ctx, &show, &mut state), None);
        let said = state.toast.as_ref().map(|(words, _)| words.clone()).unwrap_or_default();
        assert!(said.starts_with("There is no song to edit yet"), "{said}");
    }

    #[test]
    fn a_click_on_a_slide_puts_it_on_screen_as_before_and_the_pick_goes() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        // The passage as slides (the page's own click is `a_click_on_a_verse_of_the_passage…`).
        state.bible.view = BibleView::Slides;
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![enter], 0.1);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        // The slides follow the pick into view (among the chapter's), and settle.
        for at in 0..12 {
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.21 + f64::from(at) * 0.005);
        }
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert!(state.pick(&show).is_some());
        // The passage's second verse, which is in view.
        let second = SlideRef { presentation: slot, slide: state.bible.passage_slides().start + 1 };
        let at =
            state.slide_rects.iter().find(|(here, _)| *here == second).map(|(_, rect)| rect.center()).expect("drawn");
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut commands = Vec::new();
        for (time, events) in
            [(0.3, vec![Event::PointerMoved(at)]), (0.4, vec![button(true)]), (0.5, vec![button(false)])]
        {
            commands.extend(frame_then_app(&ctx, &mut show, &mut state, events, time));
        }
        assert_eq!(commands, vec![Command::GoTo(second)], "the slide clicked goes on screen");
        assert_eq!(show.live(), Some(second));
        assert!(state.picked.is_none(), "the pick goes");
    }

    /// A click at the middle of something the last frame drew, by its id, then a frame for the
    /// app to take what it asked for. Returns the commands the frames gave.
    fn click_id(ctx: &egui::Context, show: &mut Show, state: &mut HomeState, id: egui::Id, time: f64) -> Vec<Command> {
        // Two frames first, so a scroll asked for in the last (to keep the verse on screen in
        // sight) has happened and is what is read back; then the middle of what can be seen of it.
        frame_then_app(ctx, show, state, vec![], time - 0.02);
        frame_then_app(ctx, show, state, vec![], time - 0.01);
        let at = ctx.read_response(id).expect("drawn").interact_rect.center();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut commands = frame_then_app(ctx, show, state, vec![Event::PointerMoved(at)], time);
        commands.extend(frame_then_app(ctx, show, state, vec![press(true)], time + 0.05));
        commands.extend(frame_then_app(ctx, show, state, vec![press(false)], time + 0.1));
        commands.extend(frame_then_app(ctx, show, state, vec![], time + 0.15));
        commands
    }

    /// The id of a verse's row on the Bible page.
    fn page_verse(verse: u16) -> egui::Id {
        egui::Id::unique(("bible page verse", verse))
    }

    /// Scrolls the Bible page back a little, as the operator would, to the verses before the
    /// one in view, and returns a verse in view that is not one of `not`.
    fn scroll_page_back(ctx: &egui::Context, show: &mut Show, state: &mut HomeState, from: u16, not: &[u16]) -> u16 {
        let page = ctx.read_response(page_verse(from)).expect("drawn").interact_rect.center();
        let wheel = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 300.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        };
        frame_then_app(ctx, show, state, vec![Event::PointerMoved(page), wheel], 0.8);
        for at in 1..8 {
            frame_then_app(ctx, show, state, vec![], 0.8 + f64::from(at) * 0.02);
        }
        (1..=36u16)
            .filter(|verse| !not.contains(verse))
            .find(|verse| ctx.read_response(page_verse(*verse)).is_some_and(|row| row.interact_rect.height() > 20.0))
            .expect("another verse is in view")
    }

    #[test]
    fn a_click_on_a_verse_with_a_slide_puts_it_on_and_a_verse_with_none_is_looked_up_and_picked() {
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let looked_up = |ctx: &egui::Context, one_a_verse: bool| {
            let mut show = service_with_bible_places();
            let mut state = HomeState { library_search: Some(("jn 3:16-18".to_owned(), 0)), ..HomeState::default() };
            state.bible.slots = Some([2, 3]);
            state.bible.options.break_on_new_verse = one_a_verse;
            assert_eq!(state.bible.view, BibleView::Page, "the page is the Bible view's first look");
            frame_then_app(ctx, &mut show, &mut state, vec![], 0.0);
            frame_then_app(ctx, &mut show, &mut state, vec![enter.clone()], 0.1);
            for at in 0..3 {
                frame_then_app(ctx, &mut show, &mut state, vec![], 0.2 + f64::from(at) * 0.02);
            }
            (show, state)
        };

        // A slide a verse: the slides are the chapter's, so any verse of the page puts its slide
        // on screen, as a click on the slide does; the pick goes, the passage stays.
        let ctx = egui::Context::default();
        let (mut show, mut state) = looked_up(&ctx, true);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 36, "John 3, a verse a slide");
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 15 }), "John 3:16 picked");
        let commands = click_id(&ctx, &mut show, &mut state, page_verse(17), 0.5);
        assert_eq!(commands, vec![Command::GoTo(SlideRef { presentation: slot, slide: 16 })]);
        assert!(state.picked.is_none(), "the pick goes");
        let outside = scroll_page_back(&ctx, &mut show, &mut state, 17, &[16, 17, 18]);
        let commands = click_id(&ctx, &mut show, &mut state, page_verse(outside), 1.0);
        let slide = usize::from(outside) - 1;
        assert_eq!(commands, vec![Command::GoTo(SlideRef { presentation: slot, slide })], "John 3:{outside}");
        assert_eq!(state.bible.shown, "От Иоанна 3:16-18", "the passage is still the one asked for");
        assert!(state.bible.pending.is_none(), "nothing was looked up");

        // Made into one slide: the passage's verses put it up; another verse is looked up and
        // picked, nothing going on screen (the passage on screen stays, in the other place).
        let ctx = egui::Context::default();
        let (mut show, mut state) = looked_up(&ctx, false);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_eq!(show.presentations()[slot].slide_count(), 1, "John 3:16-18 as one slide");
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 0 }));
        let commands = click_id(&ctx, &mut show, &mut state, page_verse(17), 0.5);
        assert_eq!(commands, vec![Command::GoTo(SlideRef { presentation: slot, slide: 0 })]);
        assert_eq!(show.live(), Some(SlideRef { presentation: slot, slide: 0 }));
        let outside = scroll_page_back(&ctx, &mut show, &mut state, 17, &[16, 17, 18]);
        let commands = click_id(&ctx, &mut show, &mut state, page_verse(outside), 1.0);
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!(state.bible.lookup, format!("John 3:{outside}"), "named as typed, in English");
        assert_eq!(show.live(), Some(SlideRef { presentation: slot, slide: 0 }), "still on screen");
        let now = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        assert_ne!(now, slot, "the new passage went into the other place");
        assert_eq!(state.pick(&show), Some(SlideRef { presentation: now, slide: 0 }));
    }

    #[test]
    fn the_page_draws_only_the_verses_in_view_of_a_long_chapter() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        // The longest chapter: the Synodal's 118th psalm (the English 119th), 176 verses.
        state.bible.open_in("Псалтирь 118".to_owned(), None, None);
        for at in 0..3 {
            frame_then_app(&ctx, &mut show, &mut state, vec![], 0.1 + f64::from(at) * 0.05);
        }
        let drawn = (1..=176u16).filter(|verse| ctx.read_response(page_verse(*verse)).is_some()).count();
        assert!(drawn > 0 && drawn < 30, "{drawn} of 176 verses drawn");
        // Laid out once: another frame with nothing changed lays out nothing again.
        let made = format!("{:?}", state.bible.page);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.5);
        assert_eq!(format!("{:?}", state.bible.page), made);
    }

    #[test]
    fn the_keypad_has_every_book_in_order_and_a_click_on_one_opens_its_first_chapter() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        assert_eq!(state.bible.books, BookPicker::Keypad, "the keypad is the first look");
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.05);
        // All 66 books, each a key, in the order of the books: row by row, left to right.
        let key = |book: usize| ctx.read_response(crate::bible::list_row_id(("book", book))).map(|r| r.rect);
        let keys: Vec<Rect> =
            (0..66).map(|book| key(book).unwrap_or_else(|| panic!("book {book} has a key"))).collect();
        for pair in keys.windows(2) {
            let (before, after) = (pair[0], pair[1]);
            let later = after.min.y > before.min.y + 1.0 || (after.min.y == before.min.y && after.min.x > before.min.x);
            assert!(later, "{before:?} then {after:?}");
            assert!(before.width() < 40.0, "a small key: {before:?}");
        }
        // A click on a book opens its first chapter, its first verse picked; nothing on screen.
        let mut commands = Vec::new();
        for (at, (book, first)) in
            [(0, "Бытие 1:1-31"), (18, "Псалтирь 1:1-6"), (42, "От Иоанна 1:1-51"), (65, "Откровение 1:1-20")]
                .into_iter()
                .enumerate()
        {
            let id = crate::bible::list_row_id(("book", book));
            commands.extend(click_id(&ctx, &mut show, &mut state, id, 0.2 + at as f64));
            assert_eq!(state.bible.shown, first, "book {book}");
            let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
            assert_eq!(state.pick(&show), Some(SlideRef { presentation: slot, slide: 0 }), "book {book}");
        }
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }), "the song stays on screen");
    }

    #[test]
    fn the_books_switch_between_keypad_and_lists_and_the_choice_is_kept() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.05);
        // The switch is at the right of the Book heading, under Hide: its two pills, 26 wide
        // with 2 between, 3 in from the end of a track 24 high whose middle is 16.4 below the
        // top of the heading, which starts where the bar with Hide ends.
        let hide = ctx.read_response(crate::bible::list_row_id("hide")).expect("Hide is drawn").rect;
        let middle = hide.center().y + 15.1 + 16.4;
        let pill = |index: f32| pos2(hide.max.x - 3.0 - 13.0 - (1.0 - index) * 28.0, middle);
        let press = |at: egui::Pos2, pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let click = |state: &mut HomeState, show: &mut Show, at: egui::Pos2, time: f64| {
            frame_then_app(&ctx, show, state, vec![Event::PointerMoved(at)], time);
            frame_then_app(&ctx, show, state, vec![press(at, true)], time + 0.05);
            frame_then_app(&ctx, show, state, vec![press(at, false)], time + 0.1);
            frame_then_app(&ctx, show, state, vec![], time + 0.15)
        };
        let genesis = || ctx.read_response(crate::bible::list_row_id(("book", 0usize))).expect("Genesis").rect;
        assert!(genesis().width() < 40.0, "a key");
        let commands = click(&mut state, &mut show, pill(1.0), 0.2);
        assert_eq!(state.bible.books, BookPicker::Lists);
        assert!(genesis().width() > 100.0, "a row of the book list");
        assert!(!goes_on_screen(&commands) && state.bible.pending.is_none(), "it looks nothing up");
        // Kept between starts, and back to the keypad.
        let prefs = state.prefs();
        assert_eq!(prefs.bible_books, "lists");
        let mut fresh = HomeState::default();
        fresh.apply_prefs(&prefs);
        assert_eq!(fresh.bible.books, BookPicker::Lists);
        fresh.apply_prefs(&crate::service::Prefs::default());
        assert_eq!(fresh.bible.books, BookPicker::Keypad, "a file from before: the keypad");
        click(&mut state, &mut show, pill(0.0), 0.6);
        assert_eq!(state.bible.books, BookPicker::Keypad);
        assert_eq!(state.prefs().bible_books, "keypad");
    }

    /// What one frame of the window draws: each rectangle (where, its fill, its outline) and
    /// each run of words (and the size of its letters). The window is tall, so the keypad's
    /// verses are in view under a book's chapters. The app carries out the frame after.
    type Painted = (Vec<(Rect, Color32, Stroke)>, Vec<(String, f32)>);
    fn drawn_frame(ctx: &egui::Context, show: &mut Show, state: &mut HomeState, time: f64) -> Painted {
        fn walk(shape: &egui::Shape, drawn: &mut Painted) {
            match shape {
                egui::Shape::Rect(rect) => drawn.0.push((rect.rect, rect.fill, rect.stroke)),
                egui::Shape::Text(text) => {
                    let size = text.galley.job.sections.first().map_or(0.0, |section| section.format.font_id.size);
                    drawn.1.push((text.galley.text().to_owned(), size));
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, drawn)),
                _ => {}
            }
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1400.0))),
            time: Some(time),
            ..RawInput::default()
        };
        let mut commands = Vec::new();
        let output = ctx.run_ui(raw, |ui| {
            commands.extend(home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
        });
        let mut drawn = (Vec::new(), Vec::new());
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut drawn));
        output.drop_without_applying_deltas();
        as_the_app_does(show, state, commands);
        drawn
    }

    #[test]
    fn the_book_open_is_lit_as_every_choice_that_is_on_and_the_lists_headings_are_11_points() {
        use crate::studio::{SELECTED, paint_flat};
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        let psalms = sp_bible::BOOKS.iter().position(|book| book.code == "PSA").unwrap();
        state.bible.list_book = psalms;
        drawn_frame(&ctx, &mut show, &mut state, 0.0);
        let (rects, _) = drawn_frame(&ctx, &mut show, &mut state, 0.05);
        // The chosen key is drawn as a flat button that is on: the same fill and inner edge.
        let key = ctx.read_response(crate::bible::list_row_id(("book", psalms))).expect("the key").rect;
        let on = {
            let shapes = egui::Context::default();
            let mut drawn = Vec::new();
            let output = shapes.run_ui(RawInput::default(), |ui| {
                paint_flat(ui.painter(), key, 6.0, crate::studio::Flat::On, false);
            });
            for clipped in &output.shapes {
                if let egui::Shape::Rect(rect) = &clipped.shape {
                    drawn.push((rect.rect, rect.fill, rect.stroke));
                }
            }
            output.drop_without_applying_deltas();
            drawn
        };
        assert!(on.iter().any(|(_, fill, _)| *fill == SELECTED));
        for look in &on {
            assert!(
                rects.contains(look),
                "the chosen key has {look:?}: {:?}",
                rects.iter().filter(|r| r.0 == key).collect::<Vec<_>>()
            );
        }
        // Another key keeps the button grey.
        let genesis = ctx.read_response(crate::bible::list_row_id(("book", 0usize))).expect("Genesis").rect;
        assert!(rects.iter().any(|(rect, fill, _)| *rect == genesis && *fill == crate::studio::BUTTON));
        assert!(!rects.iter().any(|(rect, fill, _)| *rect == genesis && *fill == SELECTED));
        // In the lists, the testaments (and Ch and Verse) are named at 11 points.
        state.bible.books = BookPicker::Lists;
        drawn_frame(&ctx, &mut show, &mut state, 0.1);
        let (_, texts) = drawn_frame(&ctx, &mut show, &mut state, 0.15);
        for heading in ["OLD TESTAMENT", "CH", "VERSE"] {
            let size = texts.iter().find(|(text, _)| text == heading).map(|(_, size)| *size);
            assert_eq!(size, Some(11.0), "{heading}");
        }
    }

    #[test]
    fn every_books_key_is_short_and_its_own_in_each_language() {
        for language in ["en", "ru", "uk"] {
            let keys: Vec<String> = (0..66).map(|book| crate::bible_page::book_key(book, language)).collect();
            for (book, key) in keys.iter().enumerate() {
                let places = key.chars().count();
                assert!((2..=4).contains(&places), "{language} {book}: {key:?}");
                assert_eq!(keys.iter().filter(|other| *other == key).count(), 1, "{language}: {key} twice");
            }
        }
        let key = crate::bible_page::book_key;
        let joh = sp_bible::BOOKS.iter().position(|book| book.code == "JOH").unwrap();
        assert_eq!([key(0, "en"), key(8, "en"), key(joh, "en")], ["Gen", "1sa", "Joh"]);
        assert_eq!(
            [key(0, "ru"), key(10, "ru"), key(5, "ru"), key(18, "ru"), key(39, "ru"), key(joh, "ru")],
            ["Быт", "3Цар", "Нав", "Пс", "Мф", "Ин"]
        );
        assert_eq!([key(0, "uk"), key(5, "uk"), key(joh, "uk"), key(65, "uk")], ["Бут", "Нав", "Ів", "Об"]);
    }

    #[test]
    fn a_language_chosen_names_the_books_and_reads_the_page_in_it_and_never_changes_the_slides() {
        // The Russian Synodal first, the King James Version second, as the app starts.
        let mut page = bible_page();
        let syn = page.shelf.position("syn").unwrap();
        let kjv = page.shelf.position("kjv").unwrap();
        let (field, _, auto_slides) = typed(&mut page, "John 3:16");
        assert_eq!(field, "John 3:16");
        assert_eq!(crate::bible_page::page_bibles(&page), Some((kjv, Some(syn))), "English first, as typed");
        // Russian chosen: the field, the books and the page's first Bible are Russian, and no
        // slides are made again.
        page.set_read_in(Some("ru".to_owned()));
        assert_eq!(page.lookup, "От Иоанна 3:16");
        assert_eq!(page.names_language(), "ru");
        assert_eq!(crate::bible_page::page_bibles(&page), Some((syn, Some(kjv))));
        assert!(page.pending.is_none(), "nothing made again");
        // Typed in English while Russian is chosen: found, read in Russian, and the slides are
        // the very slides Auto made (rule 2); the session still goes on in English.
        let (field, session, slides) = typed(&mut page, "John 3:16");
        assert_eq!((field.as_str(), session.as_str()), ("От Иоанна 3:16", "en"));
        assert_eq!(slides, auto_slides, "the same slides whatever the language");
        // English chosen, Russian typed: the Synodal's numbers say which verses (the Synodal 22nd
        // psalm), named in English with the English numbers (the English 23rd), never a Russian
        // number under an English name; the slides as Auto makes them.
        page.set_read_in(Some("en".to_owned()));
        let mut auto = bible_page();
        let (_, _, auto_psalm) = typed(&mut auto, "Пс 22");
        let (field, session, psalm) = typed(&mut page, "Пс 22");
        assert_eq!((field.as_str(), session.as_str()), ("Psalm 23:1-6", "ru"));
        assert_eq!(page.shown, "Псалтирь 22:1-6", "kept as the first translation names it");
        assert_eq!(psalm, auto_psalm);
        assert_eq!(crate::bible_page::page_bibles(&page), Some((kjv, Some(syn))));
        // Back to Auto: the language typed last, as before.
        page.set_read_in(None);
        assert_eq!((page.lookup.as_str(), page.names_language().as_str()), ("Псалом 22:1-6", "ru"));
        // Only languages with a Bible in use: Ukrainian chosen with none counts as Auto.
        assert_eq!(page.languages_in_use(), ["ru", "en"]);
        page.set_read_in(Some("uk".to_owned()));
        assert_eq!(page.names_language(), "ru");
        // Something else typed in the field is left as it is.
        page.set_read_in(Some("en".to_owned()));
        page.lookup = "ps 9".to_owned();
        page.set_read_in(Some("ru".to_owned()));
        assert_eq!((page.lookup.as_str(), page.reading.as_str()), ("ps 9", "Псалом 22:1-6"));
    }

    #[test]
    fn choosing_a_language_puts_nothing_on_screen_and_the_find_box_reads_in_it() {
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        state.bible.lookup = "John 3:16-18".to_owned();
        state.bible.look_up(None);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.1);
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]).unwrap();
        show.apply(Command::GoTo(SlideRef { presentation: slot, slide: 1 }));
        let on_screen = (show.live(), slides_as_shown(&show.presentations()[slot]));
        let mut commands = Vec::new();
        for (at, language) in [Some("ru"), Some("en"), None, Some("ru")].into_iter().enumerate() {
            state.bible.set_read_in(language.map(str::to_owned));
            commands.extend(frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2 + at as f64 * 0.1));
            assert!(state.bible.pending.is_none(), "{language:?}: nothing made again");
        }
        assert!(!goes_on_screen(&commands), "{commands:?}");
        assert_eq!((show.live(), slides_as_shown(&show.presentations()[slot])), on_screen);
        // The find box's Bible rows, in Russian: a passage typed in English, and verses found
        // by English words (quoted from the Russian Bible, and tagged with it).
        state.library_search = Some(("John 3:16".to_owned(), 0));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 1.0);
        let in_use: Vec<String> =
            state.bible.rows.iter().filter_map(|row| Some(state.bible.shelf.bibles[row.bible?].id.clone())).collect();
        let reading = state.bible.reading();
        let passage = state.finder.rows("John 3:16", &in_use, &reading, Vec::new, &ctx).expect("answered").to_vec();
        assert_eq!(passage[0].title, "От Иоанна 3:16");
        assert!(passage[0].line.starts_with("Ибо так возлюбил"), "{}", passage[0].line);
        state.library_search = Some(("valley of the shadow".to_owned(), 0));
        frame_then_app(&ctx, &mut show, &mut state, vec![], 1.1);
        let verses = state.finder.rows("valley of the shadow", &in_use, &reading, Vec::new, &ctx).expect("answered");
        assert!(!verses.is_empty() && verses.iter().all(|row| row.tag == "BIBLE \u{b7} SYN"), "{verses:?}");
        assert!(verses.iter().any(|row| row.title == "Псалом 22:4"), "{verses:?}");
    }

    #[test]
    fn the_keys_and_the_verses_listed_follow_the_language_chosen() {
        // The keypad's keys, as drawn: English on Auto (nothing typed, no computer's language in
        // a test), Russian once Russian is chosen.
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        let drawn = bible_frames(&mut state, 3);
        assert!(drawn.iter().any(|words| words == "Gen") && !drawn.iter().any(|words| words == "Быт"));
        state.bible.set_read_in(Some("ru".to_owned()));
        let drawn = bible_frames(&mut state, 3);
        assert!(drawn.iter().any(|words| words == "Быт") && !drawn.iter().any(|words| words == "Gen"), "{drawn:?}");
        // Verses listed for English words typed in the field are asked for again when the
        // language changes, and come back named and quoted in it.
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        state.bible.focus_lookup = true;
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![Event::Text("valley of the shadow".to_owned())], 0.1);
        let psalm = |state: &HomeState| state.bible.found.iter().find(|found| found.kept == "Псалтирь 22:4").cloned();
        let english = psalm(&state).expect("found in the King James Version");
        assert_eq!((english.reference.as_str(), english.bible.as_str()), ("Psalm 23:4", "KJV"));
        state.bible.set_read_in(Some("ru".to_owned()));
        let commands = frame_then_app(&ctx, &mut show, &mut state, vec![], 0.2);
        let russian = psalm(&state).expect("still listed");
        assert_eq!((russian.reference.as_str(), russian.bible.as_str()), ("Псалом 22:4", "SYN"));
        assert!(russian.words.contains("долиною смертной тени"), "{}", russian.words);
        assert!(!goes_on_screen(&commands) && state.bible.pending.is_none());
    }

    #[test]
    fn the_language_chosen_is_kept_between_starts() {
        let mut state = HomeState::default();
        assert_eq!(state.prefs().bible_language, "auto");
        state.bible.read_in = Some("ru".to_owned());
        let prefs = state.prefs();
        assert_eq!(prefs.bible_language, "ru");
        let mut fresh = HomeState::default();
        fresh.apply_prefs(&prefs);
        assert_eq!(fresh.bible.read_in.as_deref(), Some("ru"));
        for (saved, read) in [("auto", None), ("", None), ("de", None), ("uk", Some("uk")), ("en", Some("en"))] {
            fresh.apply_prefs(&crate::service::Prefs { bible_language: saved.to_owned(), ..Default::default() });
            assert_eq!(fresh.bible.read_in.as_deref(), read, "{saved:?}");
        }
    }

    #[test]
    fn in_a_narrow_middle_the_bar_takes_two_rows_and_nothing_overlaps_it() {
        // At 1280 wide the middle has room for the field, but not for everything beside it.
        let ctx = egui::Context::default();
        let mut show = service_with_bible_places();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        state.bible.slots = Some([2, 3]);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.0);
        frame_then_app(&ctx, &mut show, &mut state, vec![], 0.05);
        let rect = |id: &str| ctx.read_response(egui::Id::unique(id)).unwrap_or_else(|| panic!("{id}")).rect;
        let (field, options, change) =
            (rect("bible scripture lookup"), rect("bible options button"), rect("bible translations change"));
        assert!(options.min.y > field.max.y, "Options on a second row: {options:?} under {field:?}");
        assert!(change.min.y > options.max.y, "the translations line under both rows: {change:?}");
        assert!(field.width() >= 120.0 && field.width() <= 236.0, "{field:?}");
    }

    #[test]
    fn page_or_slides_is_kept_between_starts() {
        let mut state = HomeState::default();
        assert_eq!(state.prefs().bible_view, "page");
        state.bible.view = BibleView::Slides;
        let prefs = state.prefs();
        assert_eq!(prefs.bible_view, "slides");
        let mut fresh = HomeState::default();
        fresh.apply_prefs(&prefs);
        assert_eq!(fresh.bible.view, BibleView::Slides);
        // A preferences file from before the page: the page.
        fresh.apply_prefs(&crate::service::Prefs::default());
        assert_eq!(fresh.bible.view, BibleView::Page);
    }

    #[test]
    fn a_reference_found_in_russian_or_english_opens_the_same_slides_named_as_typed() {
        let show = three_songs();
        let key =
            |key: Key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let found = |typed: &str| {
            let ctx = egui::Context::default();
            let mut state = HomeState { library_search: Some((typed.to_owned(), 0)), ..HomeState::default() };
            state.bible.slots = Some([0, 1]);
            frame_commands(&ctx, &show, &mut state, vec![], 0.0);
            frame_commands(&ctx, &show, &mut state, vec![key(Key::Enter)], 0.1);
            let (_, passage) = state.bible.pending.take().expect("the passage is made");
            (state.bible.lookup.clone(), state.bible.session_language(), slides_as_shown(&passage))
        };
        // The numbers follow the language typed: the English 23rd psalm is the Synodal 22nd.
        let (english, english_session, english_slides) = found("Ps 23:1-2");
        let (russian, russian_session, russian_slides) = found("Пс 22:1-2");
        assert_eq!((english.as_str(), english_session.as_str()), ("Psalm 23:1-2", "en"));
        assert_eq!((russian.as_str(), russian_session.as_str()), ("Псалом 22:1-2", "ru"));
        assert_eq!(english_slides, russian_slides, "the screens get the same slides");
    }

    #[test]
    fn a_verse_found_by_english_words_puts_those_words_in_the_english_box() {
        // The Russian Synodal first, the King James Version second, as the app starts.
        let ctx = egui::Context::default();
        let show = three_songs();
        let mut state =
            HomeState { library_search: Some(("valley of the shadow".to_owned(), 0)), ..HomeState::default() };
        state.bible.slots = Some([0, 1]);
        frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let commands = frame_commands(&ctx, &show, &mut state, vec![enter], 0.1);
        // The verse is the passage, its slides its chapter's, picked there.
        let (slot, passage) = state.bible.pending.as_ref().expect("the verse is made");
        let picked = state.picked.as_ref().map(|picked| picked.at);
        assert_eq!(picked, Some(SlideRef { presentation: *slot, slide: 3 }), "the fourth verse of the chapter");
        let (_, slide) = passage.slide(3).expect("a slide");
        let english = slide.text_for(&sp_model::Role::translation()).unwrap_or_default();
        let russian = slide.text_for(&sp_model::Role::primary()).unwrap_or_default();
        assert!(english.contains("valley of the shadow of death"), "{english}");
        assert!(russian.contains("долиною смертной тени"), "{russian}");
        // Named in English with the English numbers, and the session goes on in English.
        assert_eq!(state.bible.lookup, "Psalm 23:4", "the King James Version's numbers");
        assert_eq!(state.bible.language.as_deref(), Some("en"));
        assert!(commands.is_empty(), "nothing is put on the screens");
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
        assert_eq!(songs, vec![(0, 0, Some("The shepherd leads me on".to_owned()))]);
        let in_use: Vec<String> =
            state.bible.rows.iter().filter_map(|row| Some(state.bible.shelf.bibles[row.bible?].id.clone())).collect();
        let reading = state.bible.reading();
        let verses = state.finder.rows("shepherd", &in_use, &reading, Vec::new, &ctx).expect("answered").to_vec();
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
        // Kept as the Bible it was found in names and numbers it.
        assert!(
            matches!(items.last(), Some(crate::service::Item::Passage(saved))
                if saved.lookup == last.kept && saved.numbering.as_deref() == Some(last.numbering.name())),
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
        assert!(!state.alert_open(), "Delete asks nothing");
        let asked = state.slide_request.take().map(|(at, request)| (at.presentation, request));
        assert_eq!(asked, Some((0, SlideRequest::DeleteMany(vec![0, 1]))), "both, at once, in one step");
        assert!(state.picked_slides.is_empty());
    }

    #[test]
    fn delete_takes_the_picked_slides_of_one_song_only() {
        let at = |presentation, slide| SlideRef { presentation, slide };
        // Picked in two songs: the song of the slide picked last.
        assert_eq!(picks_of_one_song(&[at(0, 2), at(1, 0), at(0, 1)]), Some((0, vec![1, 2])));
        assert_eq!(picks_of_one_song(&[at(0, 2), at(1, 3), at(1, 0)]), Some((1, vec![0, 3])));
        assert_eq!(picks_of_one_song(&[]), None);
        let mut state = HomeState { picked_slides: vec![at(0, 2), at(1, 0), at(0, 1)], ..HomeState::default() };
        let (song, slides) = picks_of_one_song(&state.picked_slides).expect("picked");
        delete_slides(&mut state, song, &slides);
        assert_eq!(state.slide_request, Some((at(0, 1), SlideRequest::DeleteMany(vec![1, 2]))));
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
        state.services.add(Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(Item::Header { name: "Sermon".to_owned() });
        state.services.add(Item::song("title:Beta".to_owned(), "Beta".to_owned()));
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
        state.services.add(Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(Item::Media { id: walk_in, name: "Walk-in.mp4".to_owned() });
        state.services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        state.services.add(Item::song("title:Beta".to_owned(), "Beta".to_owned()));
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
        state.services.add(Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        state.services.add(Item::song("title:Beta".to_owned(), "Beta".to_owned()));
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

    /// For dragging what the find box finds: the songs Alpha and Beta in the playlist in front,
    /// which is listed, Gamma only in the library, another playlist (Evening) and a library
    /// (Hymns); the Bible view's two places after the songs; Alpha's first slide on screen, as
    /// during a service.
    fn find_drag_setting() -> (Show, HomeState) {
        use crate::service::{Item, Service};
        let words = |first: &str| format!("[Verse 1]\n{first}\n\n[Chorus]\nThird lines\n");
        let mut all: Vec<Arc<sp_model::Presentation>> =
            [("Alpha", "Morning lines"), ("Beta", "Evening lines"), ("Gamma", "Harbour lantern")]
                .iter()
                .map(|(title, first)| Arc::new(parse_song_text(title, &words(first))))
                .collect();
        for _ in 0..2 {
            all.push(Arc::new(sp_model::Presentation {
                id: sp_model::Uuid::new_v4(),
                title: "Bible".to_owned(),
                groups: Vec::new(),
                ..(*all[0]).clone()
            }));
        }
        let mut show = Show::new(all);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.bible.slots = Some([3, 4]);
        state.song_keys = ["alpha", "beta", "gamma", "bible-a", "bible-b"].map(str::to_owned).to_vec();
        state.services.add(Item::song("alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(Item::song("beta".to_owned(), "Beta".to_owned()));
        state.services.chosen = None;
        state.services.list.push(Service { name: "Evening".to_owned(), items: Vec::new() });
        state.libraries.push(crate::libraries::Library::new("Hymns".to_owned()));
        (show, state)
    }

    /// What one frame of a drag drew: each piece of text and where, and the pointer's shape.
    struct Drawn {
        texts: Vec<(String, Rect)>,
        cursor: egui::CursorIcon,
    }

    impl Drawn {
        /// Where these words were drawn in the left column, last (a playlist's row in the
        /// chooser, not its name over the column).
        fn left(&self, words: &str) -> Rect {
            self.texts
                .iter()
                .rfind(|(text, rect)| text == words && rect.max.x < crate::studio::LEFT_WIDTH + 20.0)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{words} is drawn in the left column"))
        }
    }

    /// One frame of the window with these events, then what the app does after it; every
    /// command given is kept in `commands`.
    fn found_step(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
        commands: &mut Vec<Command>,
    ) -> Drawn {
        step_holding(ctx, show, state, events, Modifiers::NONE, time, commands)
    }

    /// `found_step`, with these modifier keys held.
    fn step_holding(
        ctx: &egui::Context,
        show: &mut Show,
        state: &mut HomeState,
        events: Vec<Event>,
        modifiers: Modifiers,
        time: f64,
        commands: &mut Vec<Command>,
    ) -> Drawn {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let events = std::iter::once(Event::ModifiersChanged(modifiers)).chain(events).collect();
        let raw = RawInput { time: Some(time), events, ..input(None) };
        let mut asked = Vec::new();
        let output = ctx.run_ui(raw, |ui| {
            asked.extend(home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None));
        });
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        let cursor = output.platform_output.cursor_icon;
        output.drop_without_applying_deltas();
        as_the_app_does(show, state, asked.clone());
        commands.extend(asked);
        Drawn { texts, cursor }
    }

    /// Types `words` in the find box, presses on the result at `place` in its list and drags it
    /// (a little way first, as a hand does), then to the place `to` picks from what is drawn
    /// while it is carried, and lets go there. Returns every command given on the way, and what
    /// was drawn with the pointer over that place.
    fn drag_found(
        show: &mut Show,
        state: &mut HomeState,
        words: &str,
        place: usize,
        to: impl Fn(&Drawn) -> egui::Pos2,
    ) -> (Vec<Command>, Drawn, egui::Context) {
        let ctx = egui::Context::default();
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut commands = Vec::new();
        state.library_search = Some((words.to_owned(), 0));
        found_step(&ctx, show, state, vec![], 0.0, &mut commands);
        found_step(&ctx, show, state, vec![], 0.1, &mut commands);
        let from = state.find_rows.get(place).expect("the result is listed").center();
        found_step(&ctx, show, state, vec![Event::PointerMoved(from)], 0.2, &mut commands);
        found_step(&ctx, show, state, vec![press(from, true)], 0.3, &mut commands);
        found_step(&ctx, show, state, vec![Event::PointerMoved(from + vec2(-4.0, 12.0))], 0.4, &mut commands);
        let carrying =
            found_step(&ctx, show, state, vec![Event::PointerMoved(from + vec2(-60.0, 40.0))], 0.5, &mut commands);
        assert!(egui::DragAndDrop::has_payload_of_type::<FoundDrag>(&ctx), "the result is carried");
        assert!(state.library_search.is_some(), "the list stays open while it is carried");
        let at = to(&carrying);
        let over = found_step(&ctx, show, state, vec![Event::PointerMoved(at)], 0.6, &mut commands);
        found_step(&ctx, show, state, vec![press(at, false)], 0.7, &mut commands);
        found_step(&ctx, show, state, vec![], 0.8, &mut commands);
        assert!(!egui::DragAndDrop::has_any_payload(&ctx), "the drag has ended");
        (commands, over, ctx)
    }

    /// Nothing went on the screens: the commands put nothing on, and Alpha's first slide is
    /// still what the screens show.
    fn nothing_went_live(show: &Show, commands: &[Command]) {
        assert!(!goes_on_screen(commands), "nothing is put on the screens: {commands:?}");
        assert_eq!(show.live(), Some(SlideRef { presentation: 0, slide: 0 }));
    }

    #[test]
    fn a_playlist_item_dragged_goes_where_it_is_let_go_and_escape_leaves_it_where_it_was() {
        use crate::service::Item;
        for escape in [false, true] {
            let (mut show, mut state) = find_drag_setting();
            let ctx = egui::Context::default();
            let mut commands = Vec::new();
            let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let drawn = found_step(&ctx, &mut show, &mut state, vec![], 0.0, &mut commands);
            let from = drawn.left("Alpha").center();
            // Over the lower half of Beta's row: after it.
            let to = drawn.left("Beta").center() + vec2(0.0, 6.0);
            let key = Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            let mut frames = vec![
                vec![Event::PointerMoved(from)],
                vec![press(from, true)],
                vec![Event::PointerMoved(from + vec2(0.0, 12.0))],
                vec![Event::PointerMoved(to)],
            ];
            if escape {
                frames.push(vec![key]);
            }
            frames.extend([vec![press(to, false)], vec![]]);
            for (n, events) in frames.into_iter().enumerate() {
                found_step(&ctx, &mut show, &mut state, events, 0.1 * (n + 1) as f64, &mut commands);
            }
            let titles: Vec<&str> = state.services.front().items.iter().map(Item::title).collect();
            let order = if escape { ["Alpha", "Beta"] } else { ["Beta", "Alpha"] };
            assert_eq!(titles, order, "with Escape: {escape}");
        }
    }

    #[test]
    fn a_song_found_dragged_between_two_playlist_items_goes_in_there_and_opens() {
        use crate::service::Item;
        let (mut show, mut state) = find_drag_setting();
        // Over the upper half of Beta's row: the ink line is between Alpha and Beta.
        let (commands, over, ctx) = drag_found(&mut show, &mut state, "gamma", 0, |drawn| {
            let beta = drawn.left("Beta");
            pos2(beta.center().x, beta.center().y - 8.0)
        });
        assert_eq!(over.cursor, egui::CursorIcon::Grabbing, "a place it can go");
        let titles: Vec<&str> = state.services.front().items.iter().map(Item::title).collect();
        assert_eq!(titles, ["Alpha", "Gamma", "Beta"]);
        // Opened, as a click on its row opens it; the search is over and the keys are free.
        assert_eq!((state.services.chosen, show.selected()), (Some(1), 2));
        assert!(state.library_search.is_none() && !ctx.text_edit_focused());
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn a_song_found_dropped_on_a_playlist_in_the_chooser_goes_at_its_end() {
        use crate::service::Item;
        let (mut show, mut state) = find_drag_setting();
        let (commands, ..) = drag_found(&mut show, &mut state, "gamma", 0, |drawn| drawn.left("Evening").center());
        assert_eq!(state.services.list[1].items, [Item::song("gamma".to_owned(), "Gamma".to_owned())]);
        assert_eq!(state.services.front().items.len(), 2, "the playlist in front is as it was");
        assert!(state.library_search.is_none());
        assert!(state.toast.as_ref().is_some_and(|(words, _)| words == "Added Gamma to Evening."), "{:?}", state.toast);
        nothing_went_live(&show, &commands);
        // On the playlist in front's own row: at its end, and opened.
        let (mut show, mut state) = find_drag_setting();
        let (commands, ..) = drag_found(&mut show, &mut state, "gamma", 0, |drawn| drawn.left("Service").center());
        let titles: Vec<&str> = state.services.front().items.iter().map(Item::title).collect();
        assert_eq!(titles, ["Alpha", "Beta", "Gamma"]);
        assert_eq!((state.services.chosen, show.selected()), (Some(2), 2));
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn a_passage_found_dragged_onto_the_playlist_becomes_a_passage_item() {
        use crate::service::Item;
        let (mut show, mut state) = find_drag_setting();
        // Over the lower half of Beta's row: after it, at the end.
        let (commands, ..) = drag_found(&mut show, &mut state, "jn 3:16", 0, |drawn| {
            let beta = drawn.left("Beta");
            pos2(beta.center().x, beta.center().y + 8.0)
        });
        let items = &state.services.front().items;
        assert_eq!(items.len(), 3);
        let Some(Item::Passage(saved)) = items.last() else { panic!("a passage: {items:?}") };
        // Named in the language it was typed in, with that numbering kept beside it, and the
        // Bible view's translations and choices, as Command-Return adds it.
        let (lookup, numbering) = ("John 3:16".to_owned(), Some("english".to_owned()));
        assert_eq!(saved, &crate::service::SavedPassage { lookup, numbering, ..state.bible.settings() });
        assert_eq!(state.services.chosen, Some(2));
        assert!(!state.bible_open && state.library_search.is_none());
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn a_passage_is_refused_by_a_library_and_a_song_found_goes_into_one() {
        let (mut show, mut state) = find_drag_setting();
        let before = state.services.clone();
        let (commands, over, _) = drag_found(&mut show, &mut state, "jn 3:16", 0, |drawn| drawn.left("Hymns").center());
        assert_eq!(over.cursor, egui::CursorIcon::NoDrop, "the pointer says it can't go there");
        assert!(state.libraries[0].songs.is_empty(), "no library takes a passage");
        assert_eq!(state.services.list, before.list);
        assert!(state.library_search.is_some(), "dropped where it can't go, the search goes on");
        nothing_went_live(&show, &commands);
        // A song goes into the library, on its row in the chooser...
        let (mut show, mut state) = find_drag_setting();
        let (commands, ..) = drag_found(&mut show, &mut state, "gamma", 0, |drawn| drawn.left("Hymns").center());
        assert_eq!(state.libraries[0].songs, ["gamma"]);
        assert!(state.libraries_changed && state.library_search.is_none());
        nothing_went_live(&show, &commands);
        // ...or on its list, with the library listed; a passage there is refused.
        let (mut show, mut state) = find_drag_setting();
        state.playlist_open = false;
        state.library_shown = Some(state.libraries[0].id);
        let (_, over, _) = drag_found(&mut show, &mut state, "jn 3:16", 0, |_| pos2(120.0, 600.0));
        assert_eq!(over.cursor, egui::CursorIcon::NoDrop);
        assert!(state.libraries[0].songs.is_empty());
        let (commands, ..) = drag_found(&mut show, &mut state, "gamma", 0, |_| pos2(120.0, 600.0));
        assert_eq!(state.libraries[0].songs, ["gamma"]);
        assert_eq!(show.selected(), 2, "picked out in the list, as a click on it does");
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn the_testing_aid_drags_the_first_result_where_it_is_told_and_lets_go() {
        use crate::service::Item;
        let (mut show, mut state) = find_drag_setting();
        let ctx = egui::Context::default();
        let mut commands = Vec::new();
        // Where to: the upper half of Beta's row as it is drawn while a result is carried.
        let (mut probe_show, mut probe) = find_drag_setting();
        let (_, over, _) = drag_found(&mut probe_show, &mut probe, "gamma", 0, |drawn| drawn.left("Beta").center());
        let to = over.left("Beta").center() - vec2(0.0, 8.0);
        let mut step = 0;
        for frame in 0..120 {
            let (events, next) = find_drag_step(&ctx, &mut state, step, "gamma", to, true);
            step = next;
            found_step(&ctx, &mut show, &mut state, events, frame as f64 * 0.016, &mut commands);
        }
        let titles: Vec<&str> = state.services.front().items.iter().map(Item::title).collect();
        assert_eq!(titles, ["Alpha", "Gamma", "Beta"]);
        assert!(state.library_search.is_none());
        nothing_went_live(&show, &commands);
    }

    /// Five songs, Alpha to Epsilon, all in the playlist in front, which is listed; and the
    /// Bible view's two places; nothing on screen yet but Alpha's first slide.
    fn picking_setting() -> (Show, HomeState) {
        use crate::service::Item;
        let titles = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];
        let mut all: Vec<Arc<sp_model::Presentation>> =
            titles.iter().map(|title| Arc::new(parse_song_text(title, "Words written for a test\n"))).collect();
        for _ in 0..2 {
            all.push(Arc::new(sp_model::Presentation {
                id: sp_model::Uuid::new_v4(),
                title: "Bible".to_owned(),
                groups: Vec::new(),
                ..(*all[0]).clone()
            }));
        }
        let mut show = Show::new(all);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.bible.slots = Some([5, 6]);
        state.song_keys = titles.iter().map(|title| title.to_lowercase()).collect();
        state.song_keys.extend(["bible-a".to_owned(), "bible-b".to_owned()]);
        for title in titles {
            state.services.add(Item::song(title.to_lowercase(), title.to_owned()));
        }
        state.services.chosen = None;
        (show, state)
    }

    /// Clicks in the window with these modifier keys held, as a hand does: moved there, pressed
    /// and let go. Returns what the frame after drew.
    fn click_holding(
        ctx: &egui::Context,
        (show, state): (&mut Show, &mut HomeState),
        at: egui::Pos2,
        modifiers: Modifiers,
        time: &mut f64,
        commands: &mut Vec<Command>,
    ) -> Drawn {
        let button =
            |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers };
        let mut drawn = None;
        for events in [vec![Event::PointerMoved(at)], vec![button(true)], vec![button(false)], vec![]] {
            *time += 0.1;
            drawn = Some(step_holding(ctx, show, state, events, modifiers, *time, commands));
        }
        drawn.expect("frames ran")
    }

    #[test]
    fn playlist_items_are_picked_as_in_the_finder() {
        let (mut show, mut state) = picking_setting();
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        let row = |title: &str| drawn.left(title).center();
        let (command, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
        let mut click = |state: &mut HomeState, title: &str, mods: Modifiers| {
            click_holding(&ctx, (&mut show, state), row(title), mods, &mut time, &mut commands);
        };
        // A click picks one, and opens it.
        click(&mut state, "Beta", Modifiers::NONE);
        assert_eq!((state.item_picks.places.clone(), state.services.chosen), (vec![1], Some(1)));
        // Shift-click: every row from Beta to the one clicked, down and then up.
        click(&mut state, "Delta", shift);
        assert_eq!(state.item_picks.places, [1, 2, 3]);
        click(&mut state, "Alpha", shift);
        assert_eq!(state.item_picks.places, [0, 1], "from Beta up to Alpha, in place of the run down");
        assert_eq!(state.services.chosen, Some(1), "Shift only picks: Beta stays open");
        // Command-click adds one or takes it away, and counts from there after.
        click(&mut state, "Delta", command);
        assert_eq!(state.item_picks.places, [0, 1, 3]);
        click(&mut state, "Alpha", command);
        assert_eq!(state.item_picks.places, [1, 3]);
        click(&mut state, "Delta", command);
        click(&mut state, "Delta", command);
        assert_eq!(state.item_picks.anchor, Some(3));
        click(&mut state, "Epsilon", shift);
        assert_eq!(state.item_picks.places, [3, 4], "Shift after Command counts from the row Command-clicked");
        // Command-A picks them all; a plain click on one picked lets the others go and opens it.
        time += 0.1;
        frame_commands(&ctx, &show, &mut state, pressed(Key::A, Modifiers::COMMAND), time);
        assert_eq!(state.item_picks.places, [0, 1, 2, 3, 4]);
        click_holding(&ctx, (&mut show, &mut state), row("Gamma"), Modifiers::NONE, &mut time, &mut commands);
        assert_eq!((state.item_picks.places.clone(), state.services.chosen), (vec![2], Some(2)));
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn several_playlist_items_picked_move_together_in_their_order() {
        use crate::service::Item;
        let (mut show, mut state) = picking_setting();
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        let (gamma, alpha, epsilon) =
            (drawn.left("Gamma").center(), drawn.left("Alpha").center(), drawn.left("Epsilon"));
        // Gamma, then Alpha, picked with Command: dragged by Gamma below Epsilon, both go, Alpha first.
        click_holding(&ctx, (&mut show, &mut state), gamma, Modifiers::COMMAND, &mut time, &mut commands);
        click_holding(&ctx, (&mut show, &mut state), alpha, Modifiers::COMMAND, &mut time, &mut commands);
        assert_eq!(state.item_picks.places, [2, 0]);
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let below = pos2(epsilon.center().x, epsilon.center().y + 10.0);
        let frames = [
            vec![Event::PointerMoved(gamma)],
            vec![press(gamma, true)],
            vec![Event::PointerMoved(gamma + vec2(0.0, 12.0))],
            vec![Event::PointerMoved(below)],
            vec![press(below, false)],
            vec![],
        ];
        for events in frames {
            time += 0.1;
            found_step(&ctx, &mut show, &mut state, events, time, &mut commands);
        }
        let titles: Vec<&str> = state.services.front().items.iter().map(Item::title).collect();
        assert_eq!(titles, ["Beta", "Delta", "Epsilon", "Alpha", "Gamma"]);
        assert_eq!(state.item_picks.places, [4, 3], "still picked out where they went");
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn songs_in_a_library_are_picked_as_in_the_finder() {
        let (mut show, mut state) = picking_setting();
        state.playlist_open = false;
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        let row = |title: &str| drawn.left(title).center();
        let mut click = |state: &mut HomeState, title: &str, mods: Modifiers| {
            click_holding(&ctx, (&mut show, state), row(title), mods, &mut time, &mut commands);
        };
        click(&mut state, "Beta", Modifiers::NONE);
        click(&mut state, "Delta", Modifiers::SHIFT);
        assert_eq!(state.picked_songs, [1, 2, 3]);
        click(&mut state, "Alpha", Modifiers::SHIFT);
        assert_eq!(state.picked_songs, [0, 1]);
        click(&mut state, "Epsilon", Modifiers::COMMAND);
        click(&mut state, "Alpha", Modifiers::COMMAND);
        assert_eq!(state.picked_songs, [1, 4]);
        click(&mut state, "Gamma", Modifiers::SHIFT);
        assert_eq!(state.picked_songs, [0, 1, 2], "counting from Alpha, Command-clicked last");
        time += 0.1;
        frame_commands(&ctx, &show, &mut state, pressed(Key::A, Modifiers::COMMAND), time);
        assert_eq!(state.picked_songs.len(), 5, "Command-A picks every song listed");
        click_holding(&ctx, (&mut show, &mut state), row("Delta"), Modifiers::NONE, &mut time, &mut commands);
        assert_eq!((state.picked_songs.clone(), show.selected()), (vec![3], 3));
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn delete_takes_picked_items_out_of_the_playlist_at_once_and_undo_puts_them_back() {
        use crate::service::Item;
        let titles = |state: &HomeState| -> Vec<String> {
            state.services.front().items.iter().map(|item| item.title().to_owned()).collect()
        };
        for key in [Key::Delete, Key::Backspace] {
            let (mut show, mut state) = picking_setting();
            let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
            let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
            let row = |title: &str| drawn.left(title).center();
            click_holding(&ctx, (&mut show, &mut state), row("Beta"), Modifiers::NONE, &mut time, &mut commands);
            click_holding(&ctx, (&mut show, &mut state), row("Delta"), Modifiers::COMMAND, &mut time, &mut commands);
            found_step(&ctx, &mut show, &mut state, pressed(key, Modifiers::NONE), time + 0.1, &mut commands);
            found_step(&ctx, &mut show, &mut state, vec![], time + 0.2, &mut commands);
            assert_eq!(titles(&state), ["Alpha", "Gamma", "Epsilon"], "{key:?}");
            assert!(state.confirm_delete_songs.is_empty() && !state.alert_open(), "nothing is asked");
            assert!(state.songs_to_delete.is_empty(), "no song is deleted");
            assert!(state.toast.is_none(), "and nothing is said: Undo is the way back");
            // The app keeps what went in its one history; Undo puts it back in place, picked out
            // again, and Redo takes it out again.
            let removed = state.removed.pop().expect("kept for Undo");
            let crate::removed::Removed::Items(items) = &removed else { panic!("items: {removed:?}") };
            assert_eq!(
                items.items.iter().map(|(at, item)| (*at, item.title())).collect::<Vec<_>>(),
                [(1, "Beta"), (3, "Delta")]
            );
            assert!(removed.redo(&mut state, true));
            assert_eq!(removed.said(true), "Put 2 items back in Service.");
            assert_eq!(titles(&state), ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"]);
            assert_eq!(state.item_picks.places, [1, 3]);
            assert!(removed.redo(&mut state, false));
            assert_eq!(titles(&state), ["Alpha", "Gamma", "Epsilon"]);
            nothing_went_live(&show, &commands);
        }
        // The item on screen (Alpha, opened with a click) is taken out; its slide stays on screen.
        let (mut show, mut state) = picking_setting();
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        let alpha = drawn.left("Alpha").center();
        click_holding(&ctx, (&mut show, &mut state), alpha, Modifiers::NONE, &mut time, &mut commands);
        found_step(&ctx, &mut show, &mut state, pressed(Key::Delete, Modifiers::NONE), time + 0.1, &mut commands);
        assert!(!state.services.front().items.iter().any(|item| item.title() == "Alpha"));
        assert!(matches!(state.services.front().items.first(), Some(Item::Song { title, .. }) if title == "Beta"));
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn delete_takes_no_item_out_while_the_keyboard_is_elsewhere() {
        let (mut show, mut state) = picking_setting();
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        click_holding(
            &ctx,
            (&mut show, &mut state),
            drawn.left("Beta").center(),
            Modifiers::NONE,
            &mut time,
            &mut commands,
        );
        assert_eq!((state.services.chosen, state.picking), (Some(1), Picking::Items), "Beta is open");
        // On the Bible page, with the chooser open, or under the list of keys, Backspace and
        // Delete leave the playlist alone.
        let places: [fn(&mut HomeState, bool); 3] =
            [|state, on| state.bible_open = on, |state, on| state.chooser_open = on, |state, on| state.key_sheet = on];
        for (n, place) in places.into_iter().enumerate() {
            place(&mut state, true);
            for key in [Key::Backspace, Key::Delete] {
                time += 0.1;
                found_step(&ctx, &mut show, &mut state, pressed(key, Modifiers::NONE), time, &mut commands);
                assert_eq!(state.services.front().items.len(), 5, "place {n}, {key:?}");
            }
            place(&mut state, false);
        }
        assert!(state.removed.is_empty());
        // Back in the Show view with the playlist listed, Delete takes Beta out.
        found_step(&ctx, &mut show, &mut state, pressed(Key::Delete, Modifiers::NONE), time + 0.1, &mut commands);
        assert_eq!(state.services.front().items.len(), 4);
    }

    #[test]
    fn delete_in_a_library_asks_before_deleting_the_songs_picked() {
        let (mut show, mut state) = picking_setting();
        state.playlist_open = false;
        let (ctx, mut commands, mut time) = (egui::Context::default(), Vec::new(), 0.0);
        let drawn = found_step(&ctx, &mut show, &mut state, vec![], time, &mut commands);
        let row = |title: &str| drawn.left(title).center();
        click_holding(&ctx, (&mut show, &mut state), row("Beta"), Modifiers::NONE, &mut time, &mut commands);
        click_holding(&ctx, (&mut show, &mut state), row("Delta"), Modifiers::SHIFT, &mut time, &mut commands);
        found_step(&ctx, &mut show, &mut state, pressed(Key::Delete, Modifiers::NONE), time + 0.1, &mut commands);
        assert_eq!(state.confirm_delete_songs, [1, 2, 3], "it asks about the three songs picked");
        assert!(state.songs_to_delete.is_empty(), "and deletes nothing until told to");
        assert_eq!(state.services.front().items.len(), 5, "the playlist is as it was");
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn songs_and_the_bible_in_the_find_box_each_have_their_mark_and_tint() {
        use crate::studio::{SONG_TINT, TEXT, TEXT_DIM};
        assert_eq!(crate::studio::found_look(false), (crate::icons::Icon::Note, SONG_TINT));
        assert_eq!(crate::studio::found_look(true), (crate::icons::Icon::Book, TEXT));
        // Drawn: each tag in its kind's tint, and the marks (drawn in lines) in the same tints.
        let ctx = egui::Context::default();
        let show = Show::new(vec![
            Arc::new(parse_song_text("Shepherd Song", "[Verse 1]\nWords for a test\n")),
            Arc::new(parse_song_text("Hills", "[Chorus]\nThe shepherd leads me on\n")),
        ]);
        let mut state = HomeState { library_search: Some(("shepherd".to_owned(), 0)), ..HomeState::default() };
        let mut tags = Vec::new();
        let mut marks = Vec::new();
        for frame in 0..2 {
            let raw = RawInput { time: Some(f64::from(frame) * 0.1), ..input(None) };
            let output = ctx.run_ui(raw, |ui| {
                home_ui(ui, &show, &mut state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
            });
            fn walk(shape: &egui::Shape, tags: &mut Vec<(String, Color32)>, marks: &mut Vec<Color32>) {
                match shape {
                    egui::Shape::Text(text) => tags.push((text.galley.text().to_owned(), text.fallback_color)),
                    egui::Shape::Path(path) => {
                        if let egui::epaint::ColorMode::Solid(colour) = path.stroke.color {
                            marks.push(colour);
                        }
                    }
                    egui::Shape::LineSegment { stroke, .. } => marks.push(stroke.color),
                    egui::Shape::Circle(circle) => marks.push(circle.fill),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, tags, marks)),
                    _ => {}
                }
            }
            tags.clear();
            marks.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut tags, &mut marks));
            output.drop_without_applying_deltas();
        }
        let tint_of = |words: &str| tags.iter().find(|(text, _)| text == words).map(|(_, tint)| *tint);
        assert_eq!(tint_of("SONG"), Some(SONG_TINT), "a song found by its title");
        assert_eq!(tint_of("SONG \u{b7} CHORUS"), Some(SONG_TINT), "a slide found by its words, as its song");
        let verse = tags.iter().find(|(text, _)| text.starts_with("BIBLE \u{b7} ")).expect("a verse is found");
        assert_eq!(verse.1, TEXT_DIM, "the Bible's tag is dim, its title first");
        assert!(marks.contains(&SONG_TINT) && marks.contains(&TEXT), "each kind has its mark");
        // The titles stay in ink.
        assert_eq!(tint_of("Shepherd Song"), Some(crate::studio::TEXT));
    }

    #[test]
    fn the_slide_waiting_for_enter_is_drawn_in_ink_never_blue_or_orange() {
        use crate::studio::{CARD, TALLY, TEXT, TEXT_DIM};
        let blue = Color32::from_rgb(0x6a, 0xa5, 0xff);
        // Each outline and fill drawn in the slides, with its width.
        let drawn = |state: &mut HomeState, show: &Show| -> Vec<(f32, Color32, Color32)> {
            fn walk(shape: &egui::Shape, into: &mut Vec<(f32, Color32, Color32)>) {
                match shape {
                    egui::Shape::Rect(rect) => into.push((rect.stroke.width, rect.stroke.color, rect.fill)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                    _ => {}
                }
            }
            let ctx = egui::Context::default();
            let mut rects = Vec::new();
            for frame in 0..2 {
                let raw = RawInput { time: Some(f64::from(frame) * 0.1), ..input(None) };
                let output = ctx.run_ui(raw, |ui| {
                    home_ui(ui, show, state, &Theme::builtin(), &mut |_| None, Preview::default(), None);
                });
                rects.clear();
                output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut rects));
                output.drop_without_applying_deltas();
            }
            rects
        };
        let show = show(3);
        let mut state = HomeState::default();
        state.set_pick(&show, SlideRef { presentation: 0, slide: 1 }, false);
        let grid = drawn(&mut state, &show);
        assert!(grid.iter().any(|(width, ink, _)| *width == 2.0 && *ink == TEXT), "a 2-point ink ring round the tile");
        assert!(!grid.iter().any(|(_, ink, fill)| [*ink, *fill].iter().any(|c| *c == blue || *c == TALLY)));
        // As words only, or a table: the well, with a fine dim ring inside.
        for view in [SlideView::Easy, SlideView::Table] {
            let mut state = HomeState { slide_view: view, ..HomeState::default() };
            state.set_pick(&show, SlideRef { presentation: 0, slide: 1 }, false);
            let rows = drawn(&mut state, &show);
            assert!(rows.iter().any(|(_, _, fill)| *fill == CARD), "{view:?}: the well");
            assert!(rows.iter().any(|(width, ink, _)| *width == 1.5 && *ink == TEXT_DIM), "{view:?}: the dim ring");
            assert!(!rows.iter().any(|(_, ink, fill)| *ink == blue || *fill == blue.gamma_multiply(0.2)));
        }
    }

    #[test]
    fn a_press_on_a_result_that_barely_moves_still_opens_it() {
        let (mut show, mut state) = find_drag_setting();
        let ctx = egui::Context::default();
        let press = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut commands = Vec::new();
        state.library_search = Some(("gamma".to_owned(), 0));
        found_step(&ctx, &mut show, &mut state, vec![], 0.0, &mut commands);
        found_step(&ctx, &mut show, &mut state, vec![], 0.1, &mut commands);
        let at = state.find_rows[0].center();
        found_step(&ctx, &mut show, &mut state, vec![Event::PointerMoved(at)], 0.2, &mut commands);
        found_step(&ctx, &mut show, &mut state, vec![press(at, true)], 0.3, &mut commands);
        let moved = at + vec2(2.0, 1.0);
        found_step(&ctx, &mut show, &mut state, vec![Event::PointerMoved(moved)], 0.35, &mut commands);
        found_step(&ctx, &mut show, &mut state, vec![press(moved, false)], 0.4, &mut commands);
        found_step(&ctx, &mut show, &mut state, vec![], 0.5, &mut commands);
        assert!(!egui::DragAndDrop::has_any_payload(&ctx), "nothing was carried");
        // Opened as a click opens it: the song in the Show view, the playlist as it was.
        assert_eq!(show.selected(), 2);
        assert!(state.library_search.is_none());
        assert_eq!(state.services.front().items.len(), 2);
        nothing_went_live(&show, &commands);
    }

    #[test]
    fn up_and_down_go_down_a_playlist_step_by_step_as_the_app_runs_it() {
        use crate::service::Item;
        let songs = ["Alpha", "Beta", "Gamma"].map(|title| Arc::new(parse_song_text(title, "Words\n")));
        let mut show = Show::new(songs.to_vec());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        for title in ["Alpha", "Beta", "Gamma"] {
            state.services.add(Item::song(format!("title:{title}"), title.to_owned()));
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
        assert_eq!(found.iter().map(|(index, _, _)| *index).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(found[1].2.as_deref(), Some("You are the light on the farther side"));
        assert!(search_songs(&show, |_| true, "  ").is_empty());
        assert!(search_songs(&show, |index| index != 0, "harbour").is_empty(), "a song not listed is left out");
        // Words found on two slides of a song: each slide, by its place, with its line.
        let show = Show::new(vec![std::sync::Arc::new(sp_model::parse_song_text(
            "Lanterns",
            "[Verse 1]\nA lamp in the window\n\n[Chorus]\nThe lamp is lit\n\n[Verse 2]\nNothing here\n",
        ))]);
        let found = search_songs(&show, |_| true, "lamp");
        assert_eq!(
            found,
            vec![(0, 0, Some("A lamp in the window".to_owned())), (0, 1, Some("The lamp is lit".to_owned()))]
        );
    }

    #[test]
    fn a_slide_picked_in_the_find_list_is_brought_into_view_and_lit_and_nothing_goes_on_screen() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![
            Arc::new(parse_song_text("Alpha", "Words\n")),
            Arc::new(parse_song_text("Lanterns", "[Verse 1]\nA lamp\n\n[Chorus]\nThe chorus words\n")),
        ]);
        let mut state = HomeState { library_search: Some(("chorus words".to_owned(), 0)), ..HomeState::default() };
        state.song_keys = vec!["one".to_owned(), "two".to_owned()];
        frame_commands(&ctx, &show, &mut state, vec![], 0.0);
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let commands = frame_commands(&ctx, &show, &mut state, vec![enter], 0.1);
        assert!(commands.contains(&Command::Select(1)), "{commands:?}");
        assert!(!commands.iter().any(|c| matches!(c, Command::GoTo(_) | Command::GoToStep { .. })));
        assert_eq!(state.spotlight.map(|(at, _)| at), Some(SlideRef { presentation: 1, slide: 1 }));
        assert!(state.library_search.is_none());
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
    fn every_question_about_what_undo_cant_bring_back_says_so() {
        assert_eq!(cant_undo(""), "This can\u{2019}t be undone.");
        assert_eq!(
            cant_undo("The file stays where it is on your Mac."),
            "The file stays where it is on your Mac. This can\u{2019}t be undone."
        );
    }

    #[test]
    fn removing_a_screen_asks_first_and_return_removes_it() {
        let ctx = egui::Context::default();
        let screens =
            vec![Screen::new("Main".to_owned(), Output::Automatic), Screen::new("Lobby".to_owned(), Output::Window)];
        let mut state =
            HomeState { settings_open: true, screen_selected: 1, screens: screens.clone(), ..HomeState::default() };
        // The − asked about the lobby: the question is open, and nothing has gone yet.
        ctx.data_mut(|data| data.insert_temp(removing_screen_id(), screens[1].id));
        let frame = |state: &mut HomeState, key| {
            let mut drawn = ctx.run_ui(input(key), |ui| {
                screens_ui(ui, state, true);
            });
            drawn.textures_delta.clear();
        };
        frame(&mut state, None);
        assert_eq!((state.screens.len(), state.screens_changed), (2, false), "asked, not removed");
        frame(&mut state, Some(Key::Escape));
        assert_eq!(state.screens.len(), 2, "Escape keeps it");
        ctx.data_mut(|data| data.insert_temp(removing_screen_id(), screens[1].id));
        frame(&mut state, Some(Key::Enter));
        assert_eq!(state.screens, screens[..1], "Return removes it");
        assert!(state.screens_changed);
        assert_eq!(state.screen_selected, 0);
    }

    #[test]
    fn a_passage_added_from_the_bible_page_goes_in_once_or_into_a_new_playlist() {
        let saved = |bibles: &[&str]| crate::service::SavedPassage {
            lookup: "John 3:16".to_owned(),
            numbering: Some("english".to_owned()),
            rows: bibles
                .iter()
                .map(|bible| crate::service::SavedRow { bible: (*bible).to_owned(), verse: None, reference: None })
                .collect(),
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: String::new(),
            show_with: None,
        };
        let mut state = HomeState::default();
        state.services.list[0].name = "Sunday".to_owned();
        add_passage(&mut state, Some(0), saved(&["kjv"]), "John 3:16");
        assert_eq!(state.notice.as_deref(), Some("Added John 3:16 to Sunday."));
        add_passage(&mut state, Some(0), saved(&["kjv"]), "John 3:16");
        assert_eq!(state.notice.as_deref(), Some("John 3:16 is already in Sunday."));
        assert_eq!(state.services.front().items.len(), 1);
        // In other translations it goes in again.
        add_passage(&mut state, Some(0), saved(&["kjv", "asv"]), "John 3:16");
        assert_eq!(state.notice.as_deref(), Some("Added John 3:16 to Sunday."));
        assert_eq!(state.services.front().items.len(), 2);
        // New Playlist…: a playlist of its own, in front, its name ready to type in the Show view.
        add_passage(&mut state, None, saved(&["kjv"]), "John 3:16");
        assert_eq!(state.services.list.len(), 2);
        assert_eq!((state.services.front_at(), state.services.front().items.len()), (1, 1));
        assert_eq!(state.notice.as_deref(), Some("Added John 3:16 to Playlist 1."));
        assert_eq!(state.renaming_service, Some((1, "Playlist 1".to_owned())));
        assert!(state.chooser_open && state.playlist_open && state.view_request == Some(0));
    }

    #[test]
    fn use_on_for_the_main_theme_greys_the_ticks_that_cant_be_cleared() {
        use crate::screens::{Output, Screen};
        let (main_id, other_id) = (sp_model::Uuid::new_v4(), sp_model::Uuid::new_v4());
        let mut state = HomeState { theme_ids: vec![main_id, other_id], theme_for_songs: 0, ..HomeState::default() };
        state.screens = vec![
            Screen::new("Left".to_owned(), Output::Off),
            Screen { theme: Some(other_id), ..Screen::new("Lobby".to_owned(), Output::Off) },
            Screen { theme: Some(main_id), ..Screen::new("Right".to_owned(), Output::Off) },
        ];
        let ctx = crate::studio::tests::styled();
        let click_on = |state: &mut HomeState, theme: usize, name: &str| {
            let mut at = None;
            for (round, events) in [vec![], vec![]].into_iter().enumerate() {
                let output = crate::studio::tests::drawn(&ctx, events, |ui| use_on_menu(ui, state, theme));
                if round == 1 {
                    at = crate::studio::tests::place_of(&output, name);
                }
                output.drop_without_applying_deltas();
            }
            let at = at.expect("the screen is listed");
            let press = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            for events in [vec![Event::PointerMoved(at), press(true)], vec![press(false)]] {
                crate::studio::tests::drawn(&ctx, events, |ui| use_on_menu(ui, state, theme))
                    .drop_without_applying_deltas();
            }
        };
        // The main theme: Left (no theme of its own) and Right (set to it) are ticked and greyed.
        for name in ["Left", "Right"] {
            click_on(&mut state, 0, name);
        }
        assert_eq!(state.screens[0].theme, None);
        assert_eq!(state.screens[2].theme, Some(main_id));
        assert!(!state.screens_changed, "a greyed tick changes nothing");
        // Lobby, on another theme, can be ticked: it goes back to the main theme.
        click_on(&mut state, 0, "Lobby");
        assert_eq!(state.screens[1].theme, None);
        assert!(state.screens_changed);
        // Another theme's ticks come and go.
        click_on(&mut state, 1, "Left");
        assert_eq!(state.screens[0].theme, Some(other_id));
        click_on(&mut state, 1, "Left");
        assert_eq!(state.screens[0].theme, None);
    }

    #[test]
    fn identify_shows_under_the_preview_and_stop_ends_it() {
        let ctx = egui::Context::default();
        let show = three_songs();
        // Started from the menu bar, with a test pattern on too: Identify is what the row shows.
        let mut state =
            HomeState { identify: true, test_pattern: Some(crate::props::TestPattern::Grid), ..HomeState::default() };
        frame_texts(&ctx, &show, &mut state, vec![], 0.0);
        let texts = frame_texts(&ctx, &show, &mut state, vec![], 0.1);
        assert!(drawn_at(&texts, "Identifying the screens:").is_some());
        assert!(drawn_at(&texts, "the room sees it").is_some());
        assert!(drawn_at(&texts, "Test pattern:").is_none(), "one thing at a time in the row");
        let rect_of = |words: &str| texts.iter().rfind(|(text, _)| text == words).map(|(_, rect)| *rect).unwrap();
        assert!(rect_of("the room sees it").max.x < rect_of("Stop").min.x - 8.0, "its words fit before Stop");
        let stop = drawn_at(&texts, "Stop").expect("its Stop key");
        let press = |pressed| Event::PointerButton {
            pos: stop,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame_texts(&ctx, &show, &mut state, vec![Event::PointerMoved(stop), press(true)], 0.2);
        frame_texts(&ctx, &show, &mut state, vec![press(false)], 0.3);
        assert!(!state.identify, "Stop ends it");
        let texts = frame_texts(&ctx, &show, &mut state, vec![], 0.4);
        assert!(drawn_at(&texts, "Identifying the screens:").is_none());
        assert!(drawn_at(&texts, "Test pattern:").is_some(), "the row goes back to what plays");
    }

    #[test]
    fn a_timer_renamed_takes_its_messages_along_once_names_are_its_own() {
        let mut state = HomeState::default();
        // Two timers of one name, as an older file may have: a token shows the first.
        for name in ["Sermon", "Sermon", "Walk-in"] {
            state.timers.push(crate::timers::Timer::new(name.to_owned()));
        }
        state.messages.push(crate::messages::Message::new("Ends".to_owned()));
        state.messages[0].text = "Ends in {Timer: Sermon}".to_owned();
        assert_eq!(state.rename_timer(1, "Second"), Ok("Sermon".to_owned()));
        assert_eq!(state.messages[0].text, "Ends in {Timer: Sermon}", "the second Sermon wasn't the one shown");
        assert!(!state.messages_changed && state.timers_changed);
        assert_eq!(state.rename_timer(0, " Preaching "), Ok("Sermon".to_owned()));
        assert_eq!(
            (state.timers[0].name.as_str(), state.messages[0].text.as_str()),
            ("Preaching", "Ends in {Timer: Preaching}")
        );
        assert!(state.messages_changed);
        assert_eq!(state.rename_timer(0, "WALK-IN"), Err("There is a timer called WALK-IN already.".to_owned()));
        assert_eq!(state.rename_timer(0, "  "), Err("A timer needs a name.".to_owned()));
        assert_eq!(state.rename_timer(9, "Other"), Err("There is no such timer.".to_owned()));
        assert_eq!(state.timers[0].name, "Preaching");
    }

    #[test]
    fn a_slide_the_app_makes_has_no_menu_and_modifier_o_says_so() {
        let ctx = egui::Context::default();
        let mut show = Show::new(vec![Arc::new(parse_song_text("Captions", "[Verse 1]\nOne\n\nTwo\n"))]);
        let click = |at: egui::Pos2, pressed: bool| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for (round, made) in [false, true].into_iter().enumerate() {
            // The song by itself, as VoxLive's captions are shown, or as a song of the library.
            let time = round as f64;
            let mut state = HomeState { voxlive_slot: made.then_some(0), ..HomeState::default() };
            frame_texts(&ctx, &show, &mut state, vec![], time);
            let texts = frame_texts(&ctx, &show, &mut state, vec![], time + 0.1);
            let at = drawn_at(&texts, "VERSE 1").expect("the second slide's strip");
            frame_texts(&ctx, &show, &mut state, vec![Event::PointerMoved(at), click(at, true)], time + 0.2);
            frame_texts(&ctx, &show, &mut state, vec![click(at, false)], time + 0.3);
            frame_texts(&ctx, &show, &mut state, vec![], time + 0.4);
            assert_eq!(egui::Popup::is_any_open(&ctx), !made, "made {made}: a right-click opens a menu");
            let escape = Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            frame_texts(&ctx, &show, &mut state, vec![escape], time + 0.5);
        }
        // On screen, modifier-O says the made slide has no menu.
        let mut state = HomeState { voxlive_slot: Some(0), ..HomeState::default() };
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        frame_texts(&ctx, &show, &mut state, vec![], 3.0);
        let command_o = vec![
            Event::ModifiersChanged(Modifiers::COMMAND),
            Event::Key { key: Key::O, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND },
        ];
        frame_texts(&ctx, &show, &mut state, command_o, 3.1);
        assert!(state.live_menu.is_none());
        assert_eq!(
            state.toast.as_ref().map(|(words, _)| words.as_str()),
            Some("The verse or caption on screen has no menu.")
        );
    }

    #[test]
    fn deleting_a_song_on_the_screens_says_it_comes_off() {
        let ctx = egui::Context::default();
        let mut show = three_songs();
        let note = |show: &Show, songs: Vec<usize>| {
            let mut state = HomeState { confirm_delete_songs: songs, ..HomeState::default() };
            frame_texts(&ctx, show, &mut state, vec![], 0.0);
            let texts = frame_texts(&ctx, show, &mut state, vec![], 0.1);
            texts.into_iter().map(|(text, _)| text).find(|text| text.contains("Trash")).expect("the question shows")
        };
        assert!(!note(&show, vec![1]).contains("screens"), "nothing on screen: nothing said");
        show.apply(Command::GoTo(SlideRef { presentation: 1, slide: 0 }));
        assert!(note(&show, vec![1]).starts_with("It\u{2019}s on the screens now; it comes off. "));
        assert!(note(&show, vec![0, 1]).starts_with("One of them is on the screens now; it comes off. "));
        assert!(!note(&show, vec![0, 2]).contains("screens"), "another song on screen: nothing said");
    }

    #[test]
    fn key_mappings_list_every_key_the_list_of_keys_has() {
        fn words(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| words(shape, into)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let mut state = HomeState { preferences_pane: 3, ..HomeState::default() };
        let mut drawn = Vec::new();
        for _ in 0..2 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 6000.0))),
                ..RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| settings_ui(ui, &mut state));
            drawn.clear();
            output.shapes.iter().for_each(|clipped| words(&clipped.shape, &mut drawn));
            output.drop_without_applying_deltas();
        }
        for (group, rows) in crate::keys::every_key() {
            for (keys, what) in rows {
                assert!(drawn.contains(&what), "{group}: {what}");
                assert!(drawn.contains(&keys.join("  ")), "{group}: {keys:?}");
            }
        }
        // Among them the audio's F5 off a Mac, and on a Mac "Button only" for clearing one
        // layer; and Delete as it is.
        assert_eq!(drawn.iter().any(|words| words.contains("F5")), crate::keys::f_keys());
        assert_eq!(drawn.iter().any(|words| words.contains(crate::keys::BUTTON_ONLY)), !crate::keys::f_keys());
        assert!(drawn.iter().any(|words| words.contains("what Undo brings back goes at once")));
    }

    #[test]
    fn escape_puts_the_chooser_away_and_cancels_the_reflow_editor() {
        use crate::studio::tests::key;
        let ctx = egui::Context::default();
        let show = three_songs();
        // The chooser: Escape puts it away, and nothing is cleared.
        let mut state = HomeState { playlist_open: true, chooser_open: true, ..HomeState::default() };
        frame_events(&ctx, &show, &mut state, vec![], 0.0);
        let commands = frame_commands(&ctx, &show, &mut state, vec![key(Key::Escape)], 0.1);
        assert!(!state.chooser_open, "Escape puts the chooser away");
        assert!(commands.is_empty(), "and clears nothing: {commands:?}");
        // Retyping a playlist's name in it, Escape ends the retyping and the chooser stays.
        state.chooser_open = true;
        state.renaming_service = Some((0, "Sunday".to_owned()));
        state.rename_started = true;
        frame_events(&ctx, &show, &mut state, vec![], 0.2);
        frame_events(&ctx, &show, &mut state, vec![key(Key::Escape)], 0.3);
        assert!(state.renaming_service.is_none() && state.chooser_open, "the retyping ends, the chooser stays");
        assert_eq!(state.services.front().name, "Service", "and the name is as it was");
        // The Reflow editor: Escape cancels, keeping nothing typed, whether or not its words
        // have the keyboard.
        state.chooser_open = false;
        let middle = pos2(700.0, 420.0);
        let click = |pressed: bool| Event::PointerButton {
            pos: middle,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for typing in [false, true] {
            state.reflow = Some((0, "Words".to_owned()));
            frame_events(&ctx, &show, &mut state, vec![], 0.4);
            if typing {
                frame_events(&ctx, &show, &mut state, vec![Event::PointerMoved(middle), click(true)], 0.45);
                frame_events(&ctx, &show, &mut state, vec![click(false)], 0.5);
                frame_events(&ctx, &show, &mut state, vec![Event::Text(" more".to_owned())], 0.55);
                assert!(state.reflow.as_ref().is_some_and(|(_, words)| words.contains("more")), "typed in");
            }
            assert_eq!(ctx.text_edit_focused(), typing);
            let commands = frame_commands(&ctx, &show, &mut state, vec![key(Key::Escape)], 0.6);
            assert!(state.reflow.is_none(), "typing {typing}: Escape closes the Reflow editor");
            assert!(state.reflow_done.is_none(), "typing {typing}: without rebuilding the slides");
            assert!(commands.is_empty(), "typing {typing}: and clears nothing: {commands:?}");
        }
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
