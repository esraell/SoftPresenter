//! SoftPresenter: a home window with a slide grid, and a separate output for the audience.

mod announcements;
mod applog;
mod audio;
mod bible;
mod bible_language;
mod bible_page;
mod bible_store;
mod bin_pictures;
mod bundle;
mod clock;
mod control;
mod convert;
mod deck;
mod displays;
mod editor;
mod file_drop;
mod find;
mod home;
mod icons;
mod images;
mod inspector;
mod integrations;
mod kept;
mod keys;
mod libraries;
mod library_folder;
mod links;
mod live_tabs;
mod load;
mod macros;
mod make_cli;
mod media;
mod menubar;
mod messages;
mod output;
mod outside;
mod pack;
mod private_folder;
mod props;
mod renamed;
mod running;
mod screen_cli;
mod screens;
mod service;
mod service_cli;
mod setup;
mod show_cli;
mod slide_cli;
mod song_ids;
mod stage;
mod stage_editor;
mod studio;
mod sweep;
mod system_language;
mod theme_cli;
mod theme_store;
mod thumbs;
mod timers;
mod voxlive;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sp_engine::{Command, Show};
use sp_model::{MediaItem, ShowAction, Slide, Theme, Uuid, parse_song_text};
use sp_render::Layers;
use sp_render::vello::util::RenderContext;
use sp_render::wgpu;
use std::collections::HashMap;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::displays::{DisplayInfo, choose_output};
use crate::editor::EditDraft;
use crate::home::{HomeState, ThumbOf, home_ui};
use crate::output::{Frame, OutputHandle, OutputMsg};
use crate::screens::Placed;
use crate::thumbs::Thumbs;
use crate::timers::Timer;

/// The GPU device and its queue: one per process, shared with the output thread.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

const HELP: &str = "\
SoftPresenter

Usage: softpresenter [options] [song.txt | folder | picture]...

Songs are text files; pictures (PNG, JPEG, WebP, BMP, GIF) go into the media bin.
With no songs, a sample song is loaded. Pictures can also be dropped onto the window.

Options:
  --list-displays        Print the connected displays and quit
  --output-display N     Put the output fullscreen on display N
  --capture-home FILE    For testing: go to the third slide, save a picture of the
                         home window to FILE (PNG) and quit
  -h, --help             Show this help

Keys: Right or Space next, Left previous, Home first, End last, C clear slide,
      F1 clear all.
";

#[derive(Debug)]
struct Options {
    list_displays: bool,
    output_display: Option<usize>,
    capture_home: Option<PathBuf>,
    paths: Vec<PathBuf>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options { list_displays: false, output_display: None, capture_home: None, paths: Vec::new() };
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--list-displays" => options.list_displays = true,
            "--output-display" => {
                let value = args.next().ok_or("--output-display needs a display number")?;
                let index =
                    value.parse().map_err(|_| format!("--output-display: '{value}' is not a display number"))?;
                options.output_display = Some(index);
            }
            "--capture-home" => {
                options.capture_home = Some(PathBuf::from(args.next().ok_or("--capture-home needs a file name")?));
            }
            other if other.starts_with("--") => {
                return Err(format!("Unknown option {other}; try --help."));
            }
            path => options.paths.push(PathBuf::from(path)),
        }
    }
    Ok(Some(options))
}

/// A media playlist playing item after item: which, where in it, since when, and the media bin
/// place shown.
struct MediaCycle {
    playlist: Uuid,
    at: usize,
    since: Instant,
    shown: usize,
}

/// How long a picture stays up in a playing media playlist.
const PICTURE_SECONDS: f32 = 8.0;

/// A file's name alone, for the log and replies (no home folder in them).
fn file_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// What reloading every song from its file did.
#[derive(Default)]
struct Reloaded {
    read: Vec<String>,
    added: Vec<String>,
    waiting: Vec<String>,
    gone: Vec<String>,
    problems: Vec<String>,
}

impl Reloaded {
    /// In words, for the window and the command line.
    fn summary(&self) -> String {
        let list = |names: &[String]| names.join(", ");
        let mut parts = Vec::new();
        if !self.read.is_empty() {
            parts.push(format!("Read again: {}.", list(&self.read)));
        }
        if !self.added.is_empty() {
            parts.push(format!("Added: {}.", list(&self.added)));
        }
        if !self.waiting.is_empty() {
            parts.push(format!(
                "Waiting on Reload or Keep mine (in use, or with changes not saved): {}.",
                list(&self.waiting)
            ));
        }
        if !self.gone.is_empty() {
            parts.push(format!("Their file is gone (Keep mine saves it again): {}.", list(&self.gone)));
        }
        parts.extend(self.problems.iter().cloned());
        if parts.is_empty() { "Every song is as its file has it.".to_owned() } else { parts.join(" ") }
    }
}

/// The shortest time between two drawings of a window for an animation: one refresh of a
/// 60 Hz screen.
const FRAME: Duration = Duration::from_micros(16_667);

/// The id of the sample song the app starts with.
const SAMPLE_SONG_ID: Uuid = Uuid::from_u128(0x7361_6d70_6c65_5f73_6f6e_6700_0000_0001);

/// The templates of the theme songs use, by id and name, in order.
fn song_templates(theme: &Theme) -> Vec<(Uuid, String)> {
    theme.templates.iter().map(|template| (template.id, template.name.clone())).collect()
}

/// Opens the Screens window: a window of its own, drawn with its own interface.
fn open_panel_window(
    el: &ActiveEventLoop,
    running: &mut Running,
    quiet: bool,
    kind: Panel,
) -> Result<PanelWindow, String> {
    let attrs = Window::default_attributes()
        .with_title(kind.title())
        .with_inner_size(match kind {
            Panel::Screens => winit::dpi::LogicalSize::new(780.0, 640.0),
            Panel::Settings => winit::dpi::LogicalSize::new(760.0, 520.0),
        })
        .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 440.0));
    let attrs = if quiet {
        attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
    } else {
        attrs
    };
    let window = Arc::new(el.create_window(attrs).map_err(|e| format!("Could not open the Screens window: {e}"))?);
    let surface = running
        .context
        .instance
        .create_surface(window.clone())
        .map_err(|e| format!("Could not draw the Screens window: {e}"))?;
    let format = surface_format(&surface, running.context.devices[running.device_id].adapter())?;
    let size = window.inner_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        color_space: wgpu::SurfaceColorSpace::Auto,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    surface.configure(&running.gpu.device, &config);
    let ctx = egui::Context::default();
    studio::apply_style(&ctx);
    let state = egui_winit::State::new(
        ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
        Some(running.gpu.device.limits().max_texture_dimension_2d as usize),
    );
    let renderer = egui_wgpu::Renderer::new(&running.gpu.device, format, egui_wgpu::RendererOptions::default());
    Ok(PanelWindow { kind, window, surface, config, ctx, state, renderer, next_repaint: None, frames: 0 })
}

/// Where the show is, for timing a slide that goes on by itself: the slide on screen, how many
/// of its click builds are done, and its place in the play order.
type AdvanceAt = (sp_engine::SlideRef, usize, Option<(usize, usize)>);

/// How many slides in a row can set each other going through their actions.
const MAX_ACTION_CHAIN: u8 = 3;

/// What Clear All, Clear Messages and Clear Props take off: the message and the props. Returns
/// whether anything was showing.
fn clear_overlays(state: &mut home::HomeState, command: &Command) -> bool {
    let mut cleared = false;
    if matches!(command, Command::ClearAll | Command::Clear(sp_engine::Layer::Messages)) {
        cleared |= state.message_live.take().is_some();
    }
    if matches!(command, Command::ClearAll | Command::Clear(sp_engine::Layer::Props)) && !state.props_live.is_empty() {
        state.props_live.clear();
        cleared = true;
    }
    cleared
}

/// The show commands an action stands for, or `None` for an action that changes the window's
/// own state (a message, a prop, a timer).
fn action_commands(show: &Show, state: &home::HomeState, action: &sp_model::ShowAction) -> Option<Vec<Command>> {
    use sp_model::{ClearWhat, ShowAction};
    match action {
        ShowAction::Clear { what } => Some(vec![match what {
            ClearWhat::All => Command::ClearAll,
            ClearWhat::Slide => Command::Clear(sp_engine::Layer::Slides),
            ClearWhat::Media => Command::Clear(sp_engine::Layer::Media),
            ClearWhat::Props => Command::Clear(sp_engine::Layer::Props),
            ClearWhat::Messages => Command::Clear(sp_engine::Layer::Messages),
        }]),
        ShowAction::Audience { on } => Some(vec![Command::SetOutputEnabled(*on)]),
        ShowAction::Media { id } => {
            Some(show.media().iter().position(|item| item.id == *id).map(Command::ShowMedia).into_iter().collect())
        }
        ShowAction::Song { key } => {
            // The song's first slide in its play order that is not disabled; found as a playlist
            // finds it, so the two agree.
            let at = studio::playlist_song(show, state, key);
            let first = at.and_then(|at| {
                let song = show.presentations().get(at)?;
                song.order().into_iter().find(|slide| song.slide(*slide).is_some_and(|(_, s)| s.enabled))
            });
            Some(match (at, first) {
                (Some(at), Some(slide)) => {
                    vec![Command::Select(at), Command::GoTo(sp_engine::SlideRef { presentation: at, slide })]
                }
                (Some(at), None) => vec![Command::Select(at)],
                _ => Vec::new(),
            })
        }
        ShowAction::Message { .. }
        | ShowAction::Prop { .. }
        | ShowAction::Timer { .. }
        | ShowAction::Announce { .. } => None,
    }
}

/// What a macro made from the start-up choice plays, in words: "the song …" or "the media
/// playlist …".
fn describe_opening(show: &Show, state: &home::HomeState, step: &sp_model::ShowAction) -> String {
    match step {
        sp_model::ShowAction::Announce { what: sp_model::Announced::Song { key } } => {
            let title = studio::playlist_song(show, state, key).map(|at| show.presentations()[at].title.clone());
            format!("the song {}", title.unwrap_or_else(|| "it named (gone)".to_owned()))
        }
        sp_model::ShowAction::Announce { what: sp_model::Announced::MediaPlaylist { id } } => {
            let name = state.media_playlists.iter().find(|p| p.id == *id).map(|p| p.name.clone());
            format!("the media playlist {}", name.unwrap_or_else(|| "it named (gone)".to_owned()))
        }
        _ => "what it was set to".to_owned(),
    }
}

/// Does an action that changes the window's own state: shows or hides a message or a prop,
/// starts, stops or resets a timer.
fn do_state_action(state: &mut home::HomeState, action: &sp_model::ShowAction, now: Instant) {
    use sp_model::{Announced, ShowAction, TimerDo};
    match action {
        ShowAction::Message { id, show } => {
            if *show && state.messages.iter().any(|m| m.id == *id) {
                state.message_live = Some((*id, now));
            } else if !*show && state.message_live.is_some_and(|(live, _)| live == *id) {
                state.message_live = None;
            }
        }
        ShowAction::Prop { id, show } => {
            state.props_live.retain(|(live, _)| live != id);
            if *show && state.props.iter().any(|p| p.id == *id) {
                state.props_live.push((*id, now));
            }
        }
        ShowAction::Timer { id, act } => {
            if let Some(timer) = state.timers.iter_mut().find(|t| t.id == *id) {
                match act {
                    TimerDo::Start => timer.start(now),
                    TimerDo::Stop => timer.stop(now),
                    TimerDo::Reset => timer.reset(),
                }
            }
        }
        ShowAction::Announce { what } => {
            let asked = match what {
                Announced::Song { key } => {
                    state.song_keys.iter().position(|have| have == key).map(announcements::Announce::Song)
                }
                Announced::Media { id } => Some(announcements::Announce::Media(*id)),
                Announced::MediaPlaylist { id } => Some(announcements::Announce::MediaPlaylist(*id)),
            };
            match asked {
                Some(asked) => state.announcement_request = Some(Some(asked)),
                None => state.notice = Some("A step's song for the Announcements layer can't be found.".to_owned()),
            }
        }
        ShowAction::Clear { .. } | ShowAction::Audience { .. } | ShowAction::Song { .. } | ShowAction::Media { .. } => {
        }
    }
}

/// The changes made in the Show view, in the order Undo takes them back: songs changed (each
/// step a song's place and the song as it was: Undo puts the song back and keeps how it is now,
/// to redo), and items taken out of a playlist (Undo puts them back; Redo takes them out again).
#[derive(Default)]
struct ShowHistory {
    /// Each step, and when it was taken (a count).
    undo: Vec<(ShowStep, u64)>,
    redo: Vec<(ShowStep, u64)>,
    next: u64,
}

/// One step of the Show view's undo order.
enum ShowStep {
    /// A song's place, and the song as it was (or, to redo, as it was before the undo).
    Song(usize, Arc<sp_model::Presentation>),
    /// Items taken out of a playlist.
    Items(home::RemovedItems),
}

impl ShowHistory {
    /// Keeps a song as it is before a change, and drops what could be redone.
    fn remember(&mut self, index: usize, song: Arc<sp_model::Presentation>) {
        self.keep(ShowStep::Song(index, song));
    }

    /// Keeps items just taken out of a playlist, to put back, and drops what could be redone.
    fn remember_items(&mut self, removed: home::RemovedItems) {
        self.keep(ShowStep::Items(removed));
    }

    fn keep(&mut self, step: ShowStep) {
        self.undo.push((step, self.next));
        self.next += 1;
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Drops every step for a song, after it was changed in a way that isn't kept here.
    fn forget(&mut self, index: usize) {
        self.forget_before(index, u64::MAX);
    }

    /// Drops a song's steps taken before `mark` (see [`mark`](Self::mark)), keeping later ones.
    fn forget_before(&mut self, index: usize, mark: u64) {
        let keep =
            |(step, when): &(ShowStep, u64)| !matches!(step, ShowStep::Song(at, _) if *at == index) || *when >= mark;
        self.undo.retain(keep);
        self.redo.retain(keep);
    }

    /// Where the history is now, for [`forget_before`](Self::forget_before).
    fn mark(&self) -> u64 {
        self.next
    }

    /// Drops what could be redone, after a change the history doesn't keep.
    fn clear_redo(&mut self) {
        self.redo.clear();
    }

    /// The song the next undo (or redo) would change, when it changes a song.
    fn peek(&self, redo: bool) -> Option<usize> {
        match if redo { self.redo.last() } else { self.undo.last() } {
            Some((ShowStep::Song(index, _), _)) => Some(*index),
            _ => None,
        }
    }

    /// Whether the next undo (or redo) puts back items taken out of a playlist (or takes them
    /// out again).
    fn items_next(&self, redo: bool) -> bool {
        matches!(if redo { self.redo.last() } else { self.undo.last() }, Some((ShowStep::Items(_), _)))
    }

    /// Takes the next undo (or redo) step when it changes a song, keeping the song as it is now
    /// to step back to.
    fn step(
        &mut self,
        redo: bool,
        current: Arc<sp_model::Presentation>,
    ) -> Option<(usize, Arc<sp_model::Presentation>)> {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        match from.pop()? {
            (ShowStep::Song(index, song), when) => {
                to.push((ShowStep::Song(index, current), when));
                Some((index, song))
            }
            other => {
                from.push(other);
                None
            }
        }
    }

    /// Takes the next undo (or redo) step when it is items taken out of a playlist: they are to
    /// be put back (or taken out again).
    fn step_items(&mut self, redo: bool) -> Option<home::RemovedItems> {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        match from.pop()? {
            (ShowStep::Items(removed), when) => {
                to.push((ShowStep::Items(removed.clone()), when));
                Some(removed)
            }
            other => {
                from.push(other);
                None
            }
        }
    }
}

/// What wakes the app from outside its windows: a menu bar choice (by its item's id), the
/// output thread having drawn a new live preview, or a command from `softpresenter ctl`.
enum AppEvent {
    Menu(String),
    LivePreview,
    Control(control::Incoming),
    /// A deck of slides finished on its own thread.
    DeckDone,
    /// A song pack was written or read on its own thread.
    PackDone,
    /// A media bin picture was read on a thread of its own.
    MediaRead,
    /// The followed folders were looked at on a thread of their own.
    FoldersRead,
    /// The library was tidied on its own thread: what to tell the operator.
    Tidied(String),
    /// The DeckLink devices found changed (one plugged in or taken away, the driver found).
    DeckLink,
}

/// The most web pages kept running at once: those on screen, then those loaded ahead. Each is a
/// web process of its own (about 40 MB).
const MOST_WEB_PAGES: usize = 6;
/// How often a web page on screen is asked for a new picture: as often as the screens draw.
const WEB_LIVE_EVERY: Duration = Duration::from_millis(33);
/// How often a page on the screens that has looked the same for a second is asked for a new
/// picture: a change (new caption words) shows at most this late, then pictures come at the
/// screens' pace again.
const WEB_LIVE_STILL_EVERY: Duration = Duration::from_millis(100);
/// How often a web page loaded ahead is asked for one: enough to have a fresh picture ready,
/// and to show it in the editor.
const WEB_AHEAD_EVERY: Duration = Duration::from_millis(500);
/// How often a page resting off the screens (loaded ahead, with its first picture taken, and
/// not being edited) is asked for a new picture.
const WEB_RESTING_EVERY: Duration = Duration::from_secs(5);

/// The preview of a VoxLive screen's look: its display page in preview mode, the look last
/// painted in it and when, and when a picture was last asked for.
struct LookPreview {
    page: sp_video::WebPage,
    painted: String,
    painted_at: Option<Instant>,
    /// When the look last changed (pictures come faster for a moment after).
    changed_at: Option<Instant>,
    asked: Option<Instant>,
}

/// The page of a web page box, kept running while it is wanted.
struct WebBox {
    /// What its pictures are shown under (`sp_render::web_key`).
    key: String,
    page: sp_video::WebPage,
    /// On the screens now (they take its pictures); otherwise loaded ahead, for a slide that
    /// may come next.
    live: bool,
    /// Running (its view shown), or resting off the screens.
    shown: bool,
    /// When a picture was last asked for.
    asked: Option<Instant>,
    /// How many of its pictures went to the operator's window (for SOFTPRESENTER_TRACE_WEB).
    pictures: u32,
    /// A page that costs, once off the screens, is kept running until then.
    warm_until: Option<Instant>,
}

impl WebBox {
    /// How often it is asked for a new picture: as often as the screens show one while it is on
    /// them, twice a second while it runs ahead, and every few seconds while it rests.
    fn every(&self) -> Duration {
        if self.live {
            if self.page.still_for() >= Duration::from_secs(1) { WEB_LIVE_STILL_EVERY } else { WEB_LIVE_EVERY }
        } else if self.shown {
            WEB_AHEAD_EVERY
        } else {
            WEB_RESTING_EVERY
        }
    }
}

/// How long a page that costs (a VoxLive screen's live display) keeps running after it leaves
/// the screens, so going back to it is at once.
const WEB_WARM_FOR: Duration = Duration::from_secs(15 * 60);

/// The free page that stands in for one that costs to open: a VoxLive screen's test pattern for
/// its live display.
fn web_stand_in(url: &str) -> Option<String> {
    web_costs(url).then(|| format!("{url}{}demo=1", if url.contains('?') { '&' } else { '?' }))
}

/// The host part of a web address, for the log (the rest may hold a key or a name).
fn web_host(url: &str) -> &str {
    url.split_once("://").map_or("", |(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
}

/// True for a web address that costs money when it is open: any of VoxLive's own pages for
/// listening (`/listen/…`: a screen's live display joins the captions and can start a
/// translation). A screen's test pattern (`demo=1`) and preview (`preview=1`) cost nothing.
fn web_costs(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let host = web_host(&lower);
    // VoxLive's own pages for listening, or (for tests) the stand-in's. Not only a screen's
    // usual address: any page there, so a change in how VoxLive lays its addresses out can't
    // make a costly page look free (and be loaded ahead).
    let path = lower.split_once("://").and_then(|(_, rest)| rest.find('/').map(|at| &rest[at..])).unwrap_or("");
    let voxlive = ((host == "voxlive.app" || host.ends_with(".voxlive.app")) && path.starts_with("/listen/"))
        || lower.starts_with(&voxlive::display_prefix(&voxlive::api_base()));
    voxlive && !lower.contains("demo=1") && !lower.contains("preview=1")
}

/// Media bin items a pack brings in, as they come: the id each had where the pack was made, and
/// its id in this bin (`None` when the pack didn't have it); and how many were added to the bin.
#[derive(Default)]
struct PackMedia {
    came: HashMap<Uuid, Option<Uuid>>,
    added: usize,
}

/// A song pack written or read, or a song bundle read, on a thread of its own, for the app to
/// finish. A bundle carries where its songs go, and the library named after the folder it was
/// in, if any.
enum PackJob {
    Wrote {
        job: u64,
        result: Result<String, String>,
    },
    Opened {
        job: u64,
        result: Box<Result<pack::Opened, String>>,
        same: Option<pack::Same>,
        library: Option<Uuid>,
    },
    Bundle {
        job: u64,
        result: Box<Result<bundle::Opened, String>>,
        path: PathBuf,
        to: SongsTo,
        library: Option<String>,
    },
}

/// Where songs being imported go, besides the Songs folder: by default, the library shown (or,
/// for a folder's songs, the library named after it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongsTo {
    /// The library they go into instead: `Some(None)` for none (All Songs only).
    pub library: Option<Option<Uuid>>,
    /// A playlist they are added to as well.
    pub playlist: Option<PlaylistSpot>,
}

/// A place in a playlist: the playlist, by its place in the list and its name (to find it again
/// if the list has changed), and the item that what is added goes after (`None`: at its end).
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistSpot {
    pub index: usize,
    pub name: String,
    pub after: Option<usize>,
}

/// What was read for an import from `path`, with the library named after its folder (or
/// songbook): a song, or a song bundle's songs (whose pictures and videos are brought in with
/// them).
enum SongRead {
    Song { song: Result<sp_model::Presentation, String>, path: PathBuf, library: Option<String> },
    Bundle { opened: bundle::Opened, path: PathBuf, library: Option<String> },
}

/// A song bundle being taken in, song by song: what became of its pictures and videos brought
/// so far (each is brought once, by the first song taken in that shows it), and those that
/// couldn't be read.
struct Bringing<'a> {
    opened: &'a bundle::Opened,
    placed: bundle::Placed,
    failed: Vec<String>,
}

/// How an import knows a song the library has: its title, in any case, and its words.
fn import_key(song: &sp_model::Presentation) -> (String, String) {
    (song.title.trim().to_lowercase(), sp_model::to_song_text(song))
}

/// Where the library (`songs`) has a song being imported already: the song with its title and
/// words (`have`, by [`import_key`]), else the one with its id (`ids`).
fn already_here(
    song: &sp_model::Presentation,
    have: &HashMap<(String, String), usize>,
    ids: &std::collections::HashSet<Uuid>,
    songs: &[Arc<sp_model::Presentation>],
) -> Option<usize> {
    have.get(&import_key(song))
        .copied()
        .or_else(|| ids.contains(&song.id).then(|| songs.iter().position(|s| s.id == song.id)).flatten())
}

/// Puts items into playlist `at`, one after another in their order: after the item `after`
/// names when it is the playlist in front, else at its end. A song already in it isn't added
/// again.
fn put_in_playlist(services: &mut service::Services, at: usize, after: Option<usize>, items: Vec<service::Item>) {
    match after {
        Some(after) if at == services.current => {
            let mut slot = after + 1;
            for item in items {
                let before = services.front().items.len();
                services.insert(slot, item);
                if services.front().items.len() > before {
                    slot += 1;
                }
            }
        }
        _ => {
            for item in items {
                services.add_to(at, item);
            }
        }
    }
}

/// What an import did.
#[derive(Default)]
struct Imported {
    /// The titles of the songs imported.
    titles: Vec<String>,
    /// A sentence for each file or song that didn't come in (one here already among them).
    problems: Vec<String>,
    /// Presentations being made into slides.
    decks: Vec<String>,
    /// What each song pack opened brought in (or that it is being opened).
    packs: Vec<String>,
    /// Song bundles being read on a thread of their own.
    opening: Vec<String>,
    /// What a bundle's song came without (pictures or videos missing or left out).
    notes: Vec<String>,
    /// The libraries named after folders the songs went into.
    libraries_used: Vec<String>,
    /// The first song imported (or found here already), whose slides are shown.
    first: Option<usize>,
    /// Every song imported or found here already, by its place, in order.
    landed: Vec<usize>,
    /// The playlist they were added to.
    playlist: Option<String>,
    /// The library a drop on it put them into.
    library: Option<String>,
    /// Pictures and videos brought into the media bin from bundles.
    pictures: usize,
    videos: usize,
}

/// How the sentence saying a song imported is in the library already ends.
const ALREADY: &str = " is already in the library.";

/// Where the pointer is in the window, in its points, while files are dragged over it: asked of
/// the system, as the window library doesn't say (see `file_drop.rs`). `None` where the system
/// can't say.
fn file_drop_pointer(window: &Window, ctx: &egui::Context) -> Option<egui::Pos2> {
    // Testing aid: SOFTPRESENTER_DROP_AT=X,Y puts the pointer there, in the window's points,
    // for files dragged over or dropped on the window.
    if let Ok(at) = std::env::var("SOFTPRESENTER_DROP_AT") {
        let (x, y) = at.split_once(',')?;
        return Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?));
    }
    let screen = file_drop::pointer_on_screen()?;
    let inner = window.inner_position().ok()?;
    Some(file_drop::window_point(screen, (inner.x, inner.y), window.scale_factor(), ctx.pixels_per_point()))
}

/// A song pack is read at once (the reply waits for it) when its file is no bigger than this;
/// a larger one, which holds videos, is read on a thread of its own.
const PACK_READ_AT_ONCE: u64 = 64 * 1024 * 1024;

/// What a theme calls its templates, in order.
fn template_names(theme: &Theme) -> Vec<String> {
    theme.templates.iter().map(|template| template.name.clone()).collect()
}

/// Whether Escape has something in the window to close (a pop-up, the Settings window, the
/// Bible page's Options or Bibles window, the setup), which it closes instead of clearing the
/// screens.
fn escape_closes_something(state: &home::HomeState, ctx: &egui::Context) -> bool {
    state.settings_open
        || state.key_sheet
        || state.bible.options.open
        || state.bible.options.manage_open
        || state.setup.is_some()
        || state.library_search.is_some()
        || egui::Popup::is_any_open(ctx)
}

/// The keys that run the show. They are read straight from the window, before the interface
/// draws, so a key press reaches the output without waiting for the home window. A slide's own
/// hot key puts it on screen; otherwise a letter jumps to the group it stands for (A for the
/// first verse, C for the chorus, and so on).
fn show_key_command(key: &Key, show: &Show) -> Option<Command> {
    match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::Space) => Some(Command::Next),
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Previous),
        Key::Named(NamedKey::Home) => Some(Command::First),
        Key::Named(NamedKey::End) => Some(Command::Last),
        // Escape clears everything; F1 does too, for keyboards that have it to hand.
        Key::Named(NamedKey::Escape | NamedKey::F1) => Some(Command::ClearAll),
        Key::Named(NamedKey::F2) => Some(Command::Clear(sp_engine::Layer::Slides)),
        Key::Named(NamedKey::F3) => Some(Command::Clear(sp_engine::Layer::Media)),
        Key::Named(NamedKey::F4) => Some(Command::Clear(sp_engine::Layer::Props)),
        Key::Named(NamedKey::F5) => Some(Command::Clear(sp_engine::Layer::Audio)),
        Key::Named(NamedKey::F6) => Some(Command::Clear(sp_engine::Layer::Messages)),
        // The key that opens find is never a slide's hot key or a group's letter.
        key if opens_find(key) => None,
        Key::Character(typed) => {
            let mut letters = typed.chars();
            let (Some(letter), None) = (letters.next(), letters.next()) else { return None };
            show.hot_key_slide(letter).or_else(|| show.group_start(sp_model::groups_for_key(letter))).map(Command::GoTo)
        }
        _ => None,
    }
}

/// Keys that run the show but belong to something open in the window: under the list of keys
/// nothing runs the show; while the numbers show (after modifier-1 to 7), a number and Escape
/// are theirs; in an open menu the arrows, Space and Enter move in it and choose. In the Bible
/// view a letter or a number starts the next reference in Scripture Lookup (see `bible_area` in
/// `home.rs`), never a jump to a group of the song on screen: after Return has picked a verse and
/// given the keyboard back, typing "ps 23" must not put the song's second verse up with its "s".
fn show_key_held_back(state: &home::HomeState, ctx: &egui::Context, key: &Key) -> bool {
    if state.key_sheet {
        return true;
    }
    if state.bible_open && matches!(key, Key::Character(_)) && !opens_find(key) {
        return true;
    }
    let digit = matches!(key, Key::Character(typed) if typed.chars().count() == 1 && typed.chars().all(|c| ('1'..='9').contains(&c)));
    if state.numbers.is_some() && (digit || matches!(key, Key::Named(NamedKey::Escape))) {
        return true;
    }
    egui::Popup::is_any_open(ctx)
        && matches!(key, Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::Space | NamedKey::Enter))
}

/// Whether `/` is free to open the find box: the keyboard is not in a text field, the Edit view
/// is not open, and no sheet, question or menu is up.
fn find_key_is_free(state: &home::HomeState, ctx: &egui::Context) -> bool {
    !ctx.text_edit_focused()
        && !state.key_sheet
        && state.edit_kind().is_none()
        && state.new_song_sheet.is_none()
        && !state.alert_open()
        && !egui::Popup::is_any_open(ctx)
}

/// Whether Enter is free to run the show (in the Show view and the Bible view alike): no
/// control has the keyboard, the Edit view is not open, and no sheet, question, menu or Options
/// is open over the window.
fn enter_key_is_free(state: &home::HomeState, ctx: &egui::Context) -> bool {
    !ctx.text_edit_focused()
        && ctx.memory(|memory| memory.focused().is_none())
        && state.edit_kind().is_none()
        && !escape_closes_something(state, ctx)
        && state.new_song_sheet.is_none()
        && !state.alert_open()
}

/// True for the key that opens the find box in the bar along the top: `/`.
fn opens_find(key: &Key) -> bool {
    matches!(key, Key::Character(typed) if typed == "/")
}

fn surface_format(surface: &wgpu::Surface<'_>, adapter: &wgpu::Adapter) -> Result<wgpu::TextureFormat, String> {
    surface
        .get_capabilities(adapter)
        .formats
        .into_iter()
        .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm))
        .ok_or_else(|| "This display does not offer a colour format SoftPresenter can draw to.".to_owned())
}

/// The connected displays as the window system lists them.
/// Notes in the log where a screen's window was asked to go and which display it is on, so a
/// screen on the wrong display can be seen from the log.
fn log_placement(screen: &str, place: &Placed, window: &Window, displays: &[DisplayInfo]) {
    let asked = match place {
        Placed::Fullscreen(index) => {
            displays.iter().find(|d| d.index == *index).map_or("a display", |d| d.name.as_str())
        }
        Placed::Window => "a window",
        _ => return,
    };
    let on = window
        .current_monitor()
        .and_then(|m| displays.iter().find(|d| d.x == m.position().x && d.y == m.position().y).map(|d| d.name.clone()));
    let at = window.outer_position().map(|p| format!("{},{}", p.x, p.y)).unwrap_or_default();
    applog::event(
        "screens",
        &format!("{screen}: asked for {asked}; the window is on {} at {at}", on.as_deref().unwrap_or("?")),
    );
}

/// The displays as last asked of the system: when, the displays connected then, and what
/// `display_list` and `operator_displays` said of them.
struct DisplaysSeen {
    at: Instant,
    monitors: Vec<winit::monitor::MonitorHandle>,
    displays: Vec<DisplayInfo>,
    operator: Vec<usize>,
}

impl DisplaysSeen {
    /// How long what was seen is used before the system is asked again.
    const KEPT_FOR: Duration = Duration::from_secs(1);
}

/// The displays the operator works on, by their place in `monitors`: the one the home window is
/// on and the one with the menu bar (macOS's main display). Empty when neither is known yet,
/// as when the home window has just opened: no output then fills a display by guesswork.
fn operator_displays(
    monitors: &[winit::monitor::MonitorHandle],
    home: Option<winit::monitor::MonitorHandle>,
    primary: Option<winit::monitor::MonitorHandle>,
) -> Vec<usize> {
    let mut operator: Vec<usize> =
        [home, primary].into_iter().flatten().filter_map(|m| monitors.iter().position(|have| *have == m)).collect();
    operator.dedup();
    operator
}

/// The connected displays, named as macOS names them in System Settings, and which is the main
/// display (`primary`, with the menu bar).
pub(crate) fn display_list(
    monitors: impl Iterator<Item = winit::monitor::MonitorHandle>,
    primary: Option<winit::monitor::MonitorHandle>,
) -> Vec<DisplayInfo> {
    let names = sp_video::display_names();
    let mut list: Vec<DisplayInfo> = Vec::new();
    for (index, m) in monitors.enumerate() {
        let old_name = m.name().unwrap_or_else(|| "Display".to_owned());
        #[cfg(target_os = "macos")]
        let named = {
            use winit::platform::macos::MonitorHandleExtMacOS;
            let id = m.native_id();
            names.iter().find(|(have, _)| *have == id).map(|(_, name)| name.clone())
        };
        #[cfg(not(target_os = "macos"))]
        let named: Option<String> = {
            let _ = &names;
            None
        };
        let mut name = named.unwrap_or_else(|| old_name.clone());
        let alike = list.iter().filter(|d| d.name == name || d.name.starts_with(&format!("{name} ("))).count();
        if alike > 0 {
            name = format!("{name} ({})", alike + 1);
        }
        list.push(DisplayInfo {
            index,
            name,
            old_name,
            main: primary.as_ref() == Some(&m),
            x: m.position().x,
            y: m.position().y,
            w: m.size().width,
            h: m.size().height,
        });
    }
    list
}

/// Draws the home window's last frame again into a texture and saves it as a PNG.
fn capture(
    gpu: &Gpu,
    egui_renderer: &egui_wgpu::Renderer,
    jobs: &[egui::ClippedPrimitive],
    screen: &egui_wgpu::ScreenDescriptor,
    format: wgpu::TextureFormat,
    path: &std::path::Path,
) -> Result<(), String> {
    let [w, h] = screen.size_in_pixels;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("home capture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("home capture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        egui_renderer.render(&mut pass, jobs, screen);
    }
    gpu.queue.submit([encoder.finish()]);
    let mut pixels = sp_render::read_pixels(&gpu.device, &gpu.queue, &texture)?;
    if format == wgpu::TextureFormat::Bgra8Unorm {
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p.swap(0, 2));
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).map_err(|e| e.to_string())
}

/// A window drawn with the interface toolkit, and what is needed to draw one frame in it.
struct UiTarget<'a> {
    window: &'a Window,
    surface: &'a wgpu::Surface<'static>,
    config: &'a wgpu::SurfaceConfiguration,
    renderer: &'a mut egui_wgpu::Renderer,
    ctx: &'a egui::Context,
    label: &'static str,
}

/// Draws one frame of the interface in a window and returns what was drawn, for a capture.
fn paint_ui(
    gpu: &Gpu,
    target: UiTarget<'_>,
    mut full: egui::FullOutput,
) -> (Vec<egui::ClippedPrimitive>, egui_wgpu::ScreenDescriptor) {
    let UiTarget { window, surface, config, renderer, ctx, label } = target;
    let jobs = ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
    for (id, deltas) in &full.textures_delta.set {
        for delta in deltas {
            renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
    }
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [config.width, config.height],
        pixels_per_point: full.pixels_per_point,
    };
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let extra = renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
    let frame = match output::next_texture(surface) {
        Some(wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame)) => {
            Some(frame)
        }
        Some(wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) | None => {
            output::configure(&gpu.device, surface, config);
            window.request_redraw();
            None
        }
        _ => None,
    };
    if let Some(frame) = frame {
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        gpu.queue.present(frame);
    } else {
        // No frame to draw into (the window is hidden or covered). The interface's data for
        // this frame has already been handed to the GPU queue, and it stays in memory until
        // something is submitted, so submit it anyway. Skipping this leaked memory on every
        // frame drawn while the window could not be seen.
        gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
    }
    for id in &full.textures_delta.free {
        renderer.free_texture(id);
    }
    // What finished drawings used is let go only when the device is asked to look (see the
    // screens' thread, which shares this device and asks too); thumbnails and previews are
    // painted here even while the screens are still.
    let _ = gpu.device.poll(wgpu::PollType::Poll);
    // The toolkit insists on being told its texture changes were applied.
    full.textures_delta.clear();
    (jobs, screen)
}

/// Which window of its own a panel window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Screens,
    Settings,
}

impl Panel {
    const ALL: [Panel; 2] = [Panel::Screens, Panel::Settings];

    fn title(self) -> &'static str {
        match self {
            Panel::Screens => "Screens",
            Panel::Settings => "Settings",
        }
    }

    /// True while the window is asked for.
    fn wanted(self, state: &HomeState) -> bool {
        match self {
            Panel::Screens => state.settings_open,
            Panel::Settings => state.preferences_open,
        }
    }

    /// Says the window is no longer wanted, as closing it does.
    fn close(self, state: &mut HomeState) {
        match self {
            Panel::Screens => {
                state.settings_open = false;
                state.identify = false;
            }
            Panel::Settings => state.preferences_open = false,
        }
    }
}

/// A window of its own with its own interface: Screens, or Settings.
struct PanelWindow {
    kind: Panel,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    next_repaint: Option<Instant>,
    frames: u32,
}

/// Everything that exists once the windows are open.
struct Running {
    home_window: Arc<Window>,
    home_surface: wgpu::Surface<'static>,
    home_config: wgpu::SurfaceConfiguration,
    /// A new size for the home window while its edge is being dragged, and when it last changed.
    /// Setting a window's drawing surface to a new size waits for everything the GPU is doing,
    /// the screens' pictures included, so it is done once the size has settled, never on each
    /// step of a drag: the audience must not see the operator resizing a window.
    home_resize: Option<(winit::dpi::PhysicalSize<u32>, Instant)>,
    /// The windows audience screens are drawn in, one per screen that has one.
    screen_windows: Vec<ScreenWindow>,
    /// Windows of screens that were moved elsewhere or switched off, hidden, and dropped a
    /// little later once the output thread has let go of them.
    retired: Vec<(Arc<Window>, Instant)>,
    /// The NDI names the output thread was last told to send under, by screen.
    ndi_sent: Vec<(Uuid, String)>,
    /// The DeckLink devices, found on a thread of their own (none on a quiet run's own data,
    /// see `sync_screens`); the outputs the output thread was last told to play out of, by
    /// screen; and the screens whose output was playing when last looked at.
    decklink: Option<sp_decklink::Discovery>,
    decklink_sent: Vec<(Uuid, screens::DeckLinkOut)>,
    decklink_playing: Vec<Uuid>,
    /// The names the screens were last told to show while being identified.
    identify_sent: Option<Vec<(Uuid, String, sp_model::Rgba)>>,
    /// The screen looks last sent to the output.
    looks_sent: Vec<(Uuid, screens::Look)>,
    /// The stage screens the output was last told of, and whether they were on.
    stage_sent: Option<(Vec<(Uuid, sp_model::ThemeTemplate)>, bool)>,
    /// The audience screens' own themes last sent to the output.
    themes_sent: Vec<(Uuid, Arc<Theme>)>,
    /// What the output was last told to show over the slides.
    overlay_sent: Option<output::Overlay>,
    output: Option<OutputHandle>,
    /// The Screens and Settings windows, while they are open.
    panels: Vec<PanelWindow>,
    /// Testing aid: the first VoxLive look has been opened for a capture.
    capture_look_opened: bool,
    gpu: Gpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    thumbs: Thumbs,
    preview: Option<egui::TextureId>,
    /// When it was last looked whether NDI is installed.
    ndi_checked: Option<Instant>,
    /// The displays as last asked of the system (see `sync_screens`).
    displays_seen: Option<DisplaysSeen>,
    /// The live preview's two textures, as the window knows them.
    live_preview_ids: Vec<egui::TextureId>,

    /// The preview picture before the last change, and when that change was, so the preview
    /// fades as the output does.
    preview_before: Option<egui::TextureId>,
    preview_changed: Instant,
    /// True when the Program preview last drew the stage picture, and when it did.
    preview_shows_stage: bool,
    /// The one screen the Program preview was last told to look at.
    preview_screen_shown: Option<Uuid>,
    stage_preview_at: Instant,
    home_state: HomeState,
    next_repaint: Option<Instant>,
    /// The Edit view's picture of the slide being edited, and the words it was drawn from.
    canvas: Option<egui::TextureId>,
    canvas_for: Option<(sp_engine::SlideRef, Slide)>,
    frames: u32,
    /// The GPU instance, which outlives its surfaces and makes the surfaces of new screens,
    /// and which of its devices is in use.
    context: RenderContext,
    device_id: usize,
}

/// A window an audience screen is drawn in.
struct ScreenWindow {
    screen: Uuid,
    window: Arc<Window>,
    placed: Placed,
    name: String,
    /// When the window was last put on its display, and how many times it has been put back
    /// after being found on another (macOS can put a full-screen window on the main display).
    placed_at: Instant,
    put_back: u8,
}

struct App {
    options: Options,
    /// Wakes the event loop from other threads (the output thread, for a new live preview).
    waker: Option<winit::event_loop::EventLoopProxy<AppEvent>>,
    /// Song packs written or read on threads of their own report here.
    pack_tx: std::sync::mpsc::Sender<PackJob>,
    pack_rx: std::sync::mpsc::Receiver<PackJob>,
    /// Song packs being written or read in the background, by number, with how each went once
    /// done (`sp job N`, and `--wait`); the number of the last one started.
    jobs: Vec<(u64, Option<Result<String, String>>)>,
    last_job: Option<u64>,
    /// While `sp song import … --library NAME|none` runs: the library its songs (a folder's
    /// included) go into, or none.
    import_library_override: Option<Option<Uuid>>,
    /// Files dropped on the window, waiting until all of one drop have come, and where the
    /// pointer was at the drop (see `finish_drop`).
    dropped: Vec<PathBuf>,
    drop_pointer: Option<egui::Pos2>,
    /// Bible downloads started from the command line: each job's number, the Bible's id and name.
    bible_jobs: Vec<(u64, String, String)>,
    /// The socket `softpresenter ctl` reaches the app through, while it listens.
    _control: Option<control::Listening>,
    /// `status --wait`s held until something changes or their time is up.
    waiting: Vec<control::Waiter>,
    show: Show,
    /// The file each song came from, in the show's order; `None` for the bundled sample.
    sources: Vec<Option<PathBuf>>,
    /// The presentations kept for Bible passages.
    bible_slots: [usize; 2],
    /// Where the VoxLive item is among the presentations (see `voxlive::item`).
    voxlive_slot: usize,
    /// The VoxLive screens the item in its place was last made from, and that item.
    voxlive_item_from: Option<(Vec<voxlive::Screen>, Arc<sp_model::Presentation>)>,
    /// PDFs and folders of pictures being made into slides, each on a thread of its own.
    decks: deck::Decks,
    /// The media items' pictures, by item: a small copy of each for the window, and the
    /// full-size pictures of the item on screen and a few others for the screens.
    bin_pictures: bin_pictures::BinPictures,
    /// When the bin's small copies started being read at the start, and how many there are,
    /// until they all have been (for the log).
    bin_started: Option<(Instant, usize)>,
    /// Songs changed in the Edit view and not yet written to their files, and when the last
    /// change was made.
    unsaved: Vec<usize>,
    /// How each song and theme file was when the app last read or wrote it, so a file changed
    /// outside the app is never saved over (see `outside`).
    stamps: outside::Stamps,
    /// Songs and themes with changes not saved because their file changed outside the app; they
    /// wait on Reload or Keep mine.
    held_songs: Vec<usize>,
    held_themes: Vec<usize>,
    /// The changes made to songs in the Show view, to undo and redo.
    history: ShowHistory,
    /// The song open in the Edit view, if any, and where the song history was when it opened:
    /// steps from before then are dropped once the editor changes the song.
    edit_song: Option<usize>,
    edit_mark: u64,
    /// While actions are being done, how far along a chain of slides' actions they are.
    action_depth: Option<u8>,
    /// Where the show is (the slide on screen, its builds, its place in the play order) and
    /// since when, for a slide that goes on to the next by itself; and true once it has.
    live_since: Option<(AdvanceAt, Instant)>,
    advanced: bool,
    /// When the app started.
    started: Instant,
    /// The video on screen, by its media item, playing.
    video: Option<(Uuid, sp_video::Player)>,
    /// A video in a box on the slide on screen, by the file the box names, playing.
    slide_videos: Vec<(String, sp_video::Player)>,
    /// Box videos neither the system nor FFmpeg could play, so they aren't tried again.
    slide_video_refused: std::collections::HashSet<String>,
    /// The pages of web page boxes: those on screen, and those loaded ahead (see `WebBox`).
    web_pages: Vec<WebBox>,
    /// Web page boxes whose page couldn't be opened, not tried again until they change.
    web_refused: Vec<String>,
    /// Testing aid: when the web pages' state was last logged (SOFTPRESENTER_TRACE_WEB).
    web_traced: Option<Instant>,
    /// The connection to the church's VoxLive account.
    voxlive: voxlive::Client,
    /// The preview of a VoxLive screen's look being changed, while its page is open.
    voxlive_preview: Option<LookPreview>,
    /// How the last save of a VoxLive look went, for `sp voxlive`.
    voxlive_said: Option<String>,
    /// What plays on the Announcements layer: a song, a picture, a video or a media playlist
    /// (see `announcements`).
    announcing: Option<announcements::Announcing>,
    /// A media playlist playing.
    media_cycle: Option<MediaCycle>,
    /// What is known of the files in followed folders, and when the folders were last looked
    /// at. A look is taken on a thread of its own (a folder on a network share can take a
    /// while); the window takes in what it found when it is done, and one asked for meanwhile
    /// is taken after it.
    folder_notes: media::FolderNotes,
    folders_checked: Instant,
    folder_look: Option<std::sync::mpsc::Receiver<Vec<media::Listing>>>,
    look_again: bool,
    /// The media bin items a followed folder brought in (not added by hand).
    followed_items: std::collections::HashSet<Uuid>,
    /// The media playlists read at the start, handed to the window's state once it is made.
    media_playlists: Vec<media::Playlist>,
    unsaved_since: Option<Instant>,
    /// Whether the Edit view was open at the last drawing (leaving it writes what was changed).
    was_editing: bool,
    /// The main theme, which songs and Bible slides are drawn with: a copy of the theme at
    /// `songs_theme` in `themes`. `theme_for_bible` and `bible_theme` are the same theme, kept
    /// apart for the places that ask for Bible slides' theme.
    theme: Arc<Theme>,
    theme_for_bible: Arc<Theme>,
    themes: Vec<Theme>,
    /// The themes again, shared, for a song that names its own theme.
    theme_arcs: Vec<Arc<Theme>>,
    songs_theme: usize,
    bible_theme: usize,
    /// Which template of the main theme Bible slides use, by its place among them.
    bible_layout: usize,
    /// The themes with changes not yet written to their files, by place.
    themes_unsaved: Vec<usize>,
    /// The Bible page's set-up as last written to its file, to notice when it changes.
    bible_page_saved: String,
    /// The small choices as last written, and how many frames have been drawn: NDI takes a
    /// moment to start, and the choices are not written until it has had that moment.
    prefs_saved: Option<service::Prefs>,
    frames_since_start: u32,
    /// Plays the Audio tab's tracks.
    player: audio::Player,
    /// The modifier keys held down now. A letter pressed with Command or Control is a
    /// shortcut, not a jump to a group.
    held: winit::keyboard::ModifiersState,
    /// Testing aid: select a box once the Theme Editor has opened.
    capture_selects_box: bool,
    /// The window that has the keyboard, for Close Window.
    focused: Option<WindowId>,
    /// The menu bar along the top of the screen.
    #[cfg(target_os = "macos")]
    menu: Option<menubar::MenuBar>,
    running: Option<Running>,
    failed: bool,
    /// Set when a test capture has been saved and the program should end.
    capture_done: bool,
    /// How often and how long the operator window is drawn, and the last notice, for the log.
    home_timing: applog::Timing,
    notice_logged: Option<String>,
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> Result<Option<Running>, String> {
        // macOS opens a new window as a tab of a full-screen window (System Settings' "prefer
        // tabs in full screen"), which put the Settings and Screens windows over the whole
        // screen. SoftPresenter's windows are never tabs.
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::ActiveEventLoopExtMacOS;
            el.set_allows_automatic_window_tabbing(false);
        }
        let monitors: Vec<_> = el.available_monitors().collect();
        let displays = display_list(monitors.iter().cloned(), el.primary_monitor());
        if self.options.list_displays {
            for d in &displays {
                let main = if d.main { " (main display)" } else { "" };
                println!("display {}: {} {}x{} at {},{}{main}", d.index, d.name, d.w, d.h, d.x, d.y);
            }
            return Ok(None);
        }

        // Testing aid: open the window at another size, such as 1100x700.
        let (width, height) = std::env::var("SOFTPRESENTER_WINDOW_SIZE")
            .ok()
            .and_then(|size| {
                let (w, h) = size.split_once('x')?;
                Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?))
            })
            .unwrap_or((1440.0, 900.0));
        let home_attrs = Window::default_attributes()
            .with_title("SoftPresenter")
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_min_inner_size(winit::dpi::LogicalSize::new(1100.0, 700.0));
        // A run that only saves a picture of the window stays out of the way: its windows open
        // behind everything else and do not take the keyboard, so whoever is using the
        // computer is not interrupted.
        let quiet = self.options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some();
        let behind = |attrs: winit::window::WindowAttributes| {
            if quiet {
                attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
            } else {
                attrs
            }
        };
        let home_attrs = behind(home_attrs);
        let home_window =
            Arc::new(el.create_window(home_attrs).map_err(|e| format!("Could not open the home window: {e}"))?);
        let operator = operator_displays(&monitors, home_window.current_monitor(), el.primary_monitor());

        // A display asked for on the command line must exist. The screens' windows are opened
        // once the app has started.
        choose_output(&displays, &operator, self.options.output_display)?;

        let mut context = RenderContext::new();
        let home_surface = context
            .instance
            .create_surface(home_window.clone())
            .map_err(|e| format!("Could not draw to the home window: {e}"))?;
        let device_id = pollster::block_on(context.device(Some(&home_surface)))
            .ok_or("No graphics device was found that SoftPresenter can use.")?;
        let handle = &context.devices[device_id];
        let gpu = Gpu { device: handle.device.clone(), queue: handle.queue.clone() };
        // By default a graphics error ends the program; in a service it is better to say so and carry on.
        gpu.device.on_uncaptured_error(Arc::new(|error| eprintln!("Graphics error: {error}")));
        let home_format = surface_format(&home_surface, handle.adapter())?;

        let waker: Option<output::Waker> = self.waker.clone().map(|proxy| {
            let proxy = std::sync::Mutex::new(proxy);
            let wake: output::Waker = Arc::new(move || {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(AppEvent::LivePreview);
                }
            });
            wake
        });
        let output = OutputHandle::spawn(gpu.device.clone(), gpu.queue.clone(), self.theme.clone(), waker)?;

        let size = home_window.inner_size();
        let home_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: home_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        home_surface.configure(&gpu.device, &home_config);

        let egui_ctx = egui::Context::default();
        studio::apply_style(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &home_window,
            Some(home_window.scale_factor() as f32),
            None,
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let mut egui_renderer =
            egui_wgpu::Renderer::new(&gpu.device, home_format, egui_wgpu::RendererOptions::default());
        let mut thumbs = Thumbs::new(&gpu.device, &gpu.queue)?;
        let (preview, _) = thumbs.preview(&gpu, &mut egui_renderer, &Layers::default(), &self.theme);
        // The live preview: two small textures the output thread draws the audience picture
        // into, turn about, for the Program preview to show what the screens show.
        let (live_w, live_h) = output::LIVE_PREVIEW_SIZE;
        let live_textures: Vec<wgpu::Texture> =
            (0..2).map(|_| sp_render::target_texture(&gpu.device, live_w, live_h)).collect();
        let live_preview_ids: Vec<egui::TextureId> = live_textures
            .iter()
            .map(|texture| {
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear)
            })
            .collect();
        output.send(OutputMsg::PreviewTargets(live_textures));

        Ok(Some(Running {
            home_window,
            home_surface,
            home_config,
            home_resize: None,
            screen_windows: Vec::new(),
            retired: Vec::new(),
            ndi_sent: Vec::new(),
            // Looking for DeckLink devices may take a moment: on a thread of its own, which
            // wakes the window with what it found.
            decklink: Some(sp_decklink::Discovery::start({
                let proxy = self.waker.clone().map(std::sync::Mutex::new);
                move || {
                    if let Some(proxy) = &proxy
                        && let Ok(proxy) = proxy.lock()
                    {
                        let _ = proxy.send_event(AppEvent::DeckLink);
                    }
                }
            })),
            decklink_sent: Vec::new(),
            decklink_playing: Vec::new(),
            identify_sent: None,
            looks_sent: Vec::new(),
            stage_sent: None,
            themes_sent: Vec::new(),
            overlay_sent: None,
            output: Some(output),
            panels: Vec::new(),
            capture_look_opened: false,
            gpu,
            egui_ctx,
            egui_state,
            egui_renderer,
            thumbs,
            preview,
            live_preview_ids,
            ndi_checked: None,
            displays_seen: None,

            preview_before: None,
            preview_changed: Instant::now(),
            preview_shows_stage: false,
            preview_screen_shown: None,
            stage_preview_at: Instant::now(),
            home_state: HomeState {
                fonts: Arc::new(sp_render::Fonts::new().family_names()),
                theme_names: self.themes.iter().map(|theme| theme.name.clone()).collect(),
                themes_kept: self
                    .themes
                    .iter()
                    .enumerate()
                    .filter(|(_, theme)| theme.id.is_nil())
                    .map(|(at, _)| at)
                    .collect(),
                theme_ids: self.themes.iter().map(|theme| theme.id).collect(),
                theme_for_songs: self.songs_theme,
                theme_layouts: self.theme_layouts(),
                template_names: self.themes.iter().map(template_names).collect(),
                song_templates: song_templates(&self.theme),
                bible_layout: self.bible_layout,
                services: service::load(),
                // A song is found again by its file, without the ending (which changes when
                // it is first saved); one with no file, by its title.
                song_keys: self
                    .sources
                    .iter()
                    .zip(self.show.presentations())
                    .enumerate()
                    .map(|(at, (source, song))| {
                        if at == self.voxlive_slot {
                            voxlive::ITEM_KEY.to_owned()
                        } else {
                            song_ids::key_for(source.as_deref(), song.id, &song.title)
                        }
                    })
                    .collect(),
                voxlive_slot: Some(self.voxlive_slot),
                theme_for_bible: self.bible_theme,
                bible: home::BiblePage {
                    slots: Some(self.bible_slots),
                    shelf: bible::Shelf::with_installed(),
                    boxes: home::layout_boxes(&self.theme_for_bible, self.bible_layout),
                    box_labels: home::layout_labels(&self.theme_for_bible, self.bible_layout),
                    template: self.bible_template(),
                    ..home::BiblePage::default()
                },
                ..HomeState::default()
            },
            next_repaint: None,
            canvas: None,
            canvas_for: None,
            frames: 0,
            context,
            device_id,
        }))
    }

    /// Applies operator commands and tells the output and the home window what changed.
    fn apply(&mut self, commands: Vec<Command>) {
        // A song opened is read again first if its file changed outside the app.
        for command in &commands {
            if let Command::Select(index) = command {
                self.freshen(*index, false);
            }
        }
        let commands: Vec<Command> = commands.into_iter().flat_map(|command| self.continue_playlist(command)).collect();
        let live_before = self.show.live();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let mut any = false;
        let mut output_changed = false;
        if !commands.is_empty() {
            applog::event("command", &format!("{commands:?}"));
        }
        for command in commands {
            // Clear All, and Clear Audio, stop the audio.
            if matches!(command, Command::ClearAll | Command::Clear(sp_engine::Layer::Audio)) {
                self.player.stop();
            }
            // Clear All, and Clear Announcements, take the Announcements layer off.
            if matches!(command, Command::ClearAll | Command::Clear(sp_engine::Layer::Announcements))
                && self.announcing.take().is_some()
            {
                running.home_state.announcement_live = None;
                if let Some(output) = &running.output {
                    output.send(OutputMsg::Announcement(None));
                }
                applog::event("announcements", "cleared");
                output_changed = true;
            }
            if clear_overlays(&mut running.home_state, &command) {
                output_changed = true;
                any = true;
            }
            let changes = self.show.apply(command);
            output_changed |= changes.slides || changes.media || changes.output_enabled || changes.builds;

            any |= changes.slides || changes.media || changes.output_enabled || changes.selection || changes.builds;
        }
        if output_changed {
            self.refresh_output();
        }
        // A slide that has just gone on screen does what it says. One reached by another slide's
        // actions counts one step further along a chain, which stops after a few steps.
        let depth = self.action_depth.map_or(0, |depth| depth + 1);
        if self.show.live() != live_before {
            let what = self.show.live().and_then(|at| {
                let song = self.show.presentations().get(at.presentation)?;
                let group = song.slide(at.slide).map_or("", |(group, _)| group.name.as_str());
                Some(format!("{} slide {} ({group})", song.title, at.slide + 1))
            });
            applog::event("live", &what.unwrap_or_else(|| "cleared".to_owned()));
        }
        // A Bible passage that has just gone on screen goes in the Bible page's history.
        if self.show.live() != live_before
            && let Some(at) = self.show.live()
            && self.bible_slots.contains(&at.presentation)
            && let Some(passage) = self.show.presentations().get(at.presentation).map(|song| song.title.clone())
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.bible.note_shown(&passage, chrono::Local::now().naive_local());
        }
        if self.show.live() != live_before
            && let Some((_, slide)) = self.show.live_slide()
            && !slide.actions.is_empty()
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.actions_to_run.extend(slide.actions.iter().cloned());
            running.home_state.actions_depth = depth;
        }
        if any && let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// Goes on to the next slide when the one on screen has been there as long as it asks.
    fn tick_advance(&mut self) {
        // Any step counts: a new slide, a click build, or the same slide at another place in
        // the play order (a group an arrangement plays twice in a row).
        let here = self.show.live().map(|at| (at, self.show.build_step(), self.show.position()));
        if here != self.live_since.map(|(at, _)| at) {
            // Testing aid: say when each slide went on screen.
            if std::env::var_os("SOFTPRESENTER_TRACE_LIVE").is_some() {
                let drawn = self.running.as_ref().map_or(0, |r| r.frames);
                let slide = here.map(|(at, _, _)| at.slide);
                eprintln!("live {slide:?} at {:?}, operator window drawn {drawn} times", self.started.elapsed());
            }
            self.live_since = here.map(|at| (at, Instant::now()));
            self.advanced = false;
        }
        let Some((_, since)) = self.live_since else { return };
        let Some(seconds) = self.show.live_slide().and_then(|(_, slide)| slide.advance).filter(|s| *s > 0.0) else {
            return;
        };
        let due = since + Duration::from_secs_f32(seconds);
        if Instant::now() >= due {
            if !self.advanced {
                self.advanced = true;
                self.apply(vec![Command::Next]);
            }
        } else if let Some(running) = self.running.as_mut() {
            running.next_repaint = Some(running.next_repaint.map_or(due, |at| at.min(due)));
        }
    }

    /// Plays a media playlist item after item on the media layer, starting with its first.
    fn play_media_playlist(&mut self, id: Uuid) {
        self.check_folders(true);
        let Some(first) = self.playlist_media(id).first().copied() else {
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some("That media playlist has nothing in it to play.".to_owned());
            }
            return;
        };
        self.media_cycle = Some(MediaCycle { playlist: id, at: 0, since: Instant::now(), shown: first });
        if let Some(running) = self.running.as_mut() {
            running.home_state.media_playing = Some(id);
        }
        applog::event("media", &format!("playing media playlist {id}"));
        self.apply(vec![Command::ShowMedia(first)]);
        self.read_ahead_in_cycle();
    }

    /// Has the full-size picture of what a playing media playlist shows next read ahead, so it
    /// goes on screen sharp.
    fn read_ahead_in_cycle(&mut self) {
        let Some(cycle) = self.media_cycle.as_ref() else { return };
        let items = self.playlist_media(cycle.playlist);
        if items.len() < 2 {
            return;
        }
        let next = items[(cycle.at + 1) % items.len()];
        if let Some(item) = self.show.media().get(next) {
            self.bin_pictures.read_ahead(item.id, &item.path);
        }
    }

    fn stop_media_playlist(&mut self) {
        if self.media_cycle.take().is_some() {
            applog::event("media", "stopped the media playlist");
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.media_playing = None;
        }
    }

    /// The media bin places of a media playlist's files, in its order.
    fn playlist_media(&self, id: Uuid) -> Vec<usize> {
        let Some(running) = self.running.as_ref() else { return Vec::new() };
        let Some(playlist) = running.home_state.media_playlists.iter().find(|p| p.id == id) else { return Vec::new() };
        playlist.files.iter().filter_map(|file| self.show.media().iter().position(|item| &item.path == file)).collect()
    }

    /// Moves a playing media playlist on: a picture after 8 seconds, a video when it ends. It
    /// stops if the operator puts other media on, or clears it. Says when to look again.
    fn tick_media_cycle(&mut self) -> Option<Instant> {
        let cycle = self.media_cycle.as_ref()?;
        let (id, at, since, shown) = (cycle.playlist, cycle.at, cycle.since, cycle.shown);
        if self.show.live_media() != Some(shown) {
            self.stop_media_playlist();
            return None;
        }
        let items = self.playlist_media(id);
        let is_video = self.show.media().get(shown).is_some_and(|item| media::is_video(&item.path));
        let done = if is_video {
            // One video on its own loops, so it never ends; with others, it plays to its end.
            items.len() > 1 && self.video.as_ref().is_some_and(|(_, player)| player.ended())
        } else {
            since.elapsed() >= Duration::from_secs_f32(PICTURE_SECONDS)
        };
        if done && items.len() > 1 {
            let next_at = (at + 1) % items.len();
            let next = items[next_at];
            self.media_cycle = Some(MediaCycle { playlist: id, at: next_at, since: Instant::now(), shown: next });
            self.apply(vec![Command::ShowMedia(next)]);
            self.read_ahead_in_cycle();
        }
        Some(Instant::now() + Duration::from_millis(if is_video { 200 } else { 500 }))
    }

    /// Keeps the media playlists that follow folders up to date: has the folders looked at, on
    /// a thread of its own, every few seconds, or at once when `now`. `finish_folder_look` takes
    /// in what was found.
    fn check_folders(&mut self, now: bool) {
        if !now && self.folders_checked.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.folders_checked = Instant::now();
        // One look at a time; one asked for now while another is being taken follows it.
        if self.folder_look.is_some() {
            self.look_again |= now;
            return;
        }
        let Some(running) = self.running.as_ref() else { return };
        let followed: Vec<(Uuid, std::path::PathBuf)> = running
            .home_state
            .media_playlists
            .iter()
            .filter_map(|playlist| playlist.folder.clone().map(|folder| (playlist.id, folder)))
            .collect();
        if followed.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let waker = self.waker.clone();
        let started = std::thread::Builder::new().name("followed folders".to_owned()).spawn(move || {
            let listings: Vec<media::Listing> = followed
                .into_iter()
                .map(|(id, folder)| {
                    let files = media::folder_files(&folder);
                    (id, folder, files)
                })
                .collect();
            if tx.send(listings).is_ok()
                && let Some(waker) = waker
            {
                let _ = waker.send_event(AppEvent::FoldersRead);
            }
        });
        match started {
            Ok(_) => self.folder_look = Some(rx),
            Err(e) => applog::event("media", &format!("could not look at the followed folders: {e}")),
        }
    }

    /// Takes in a look at the followed folders: files new to the bin are read (on the readers'
    /// threads) and go in once they have been; one written over is read again (a video on
    /// screen starts again from the new file); the playlists list what their folders hold; and
    /// an item a folder brought in whose file has gone leaves the bin, unless something else
    /// uses it.
    fn finish_folder_look(&mut self) {
        let Some(listings) = self.folder_look.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        self.folder_look = None;
        let Some(running) = self.running.as_ref() else { return };
        // A playlist that stopped following that folder while it was looked at is left alone.
        let playlists = &running.home_state.media_playlists;
        let listings: Vec<media::Listing> = listings
            .into_iter()
            .filter(|(id, folder, _)| playlists.iter().any(|p| p.id == *id && p.folder.as_ref() == Some(folder)))
            .collect();
        let bin: Vec<(Uuid, std::path::PathBuf)> = self.show.media().iter().map(|i| (i.id, i.path.clone())).collect();
        let in_use = self.media_in_use();
        let playlists = &running.home_state.media_playlists;
        let changes = self.folder_notes.look(&listings, &bin, playlists, &self.followed_items, &in_use);
        let mut changed_list = false;
        let mut media_changed = false;
        for (path, changed) in changes.read_new {
            let id = Uuid::new_v4();
            self.bin_pictures.read(id, &path);
            self.folder_notes.coming.insert(id, (path, changed));
        }
        let live = self.show.live_media().and_then(|index| self.show.media().get(index)).map(|item| item.id);
        for id in changes.read_again {
            let Some(path) = self.show.media().iter().find(|item| item.id == id).map(|item| item.path.clone()) else {
                continue;
            };
            applog::event("media", &format!("{} changed in a followed folder", path.display()));
            // The old picture shows until the new one has been read.
            self.bin_pictures.read(id, &path);
            if live == Some(id) && media::is_video(&path) {
                self.video = None;
                media_changed = true;
            }
        }
        if let Some(running) = self.running.as_mut() {
            for (id, files) in changes.playlists {
                if let Some(playlist) = running.home_state.media_playlists.iter_mut().find(|p| p.id == id) {
                    playlist.files = files;
                    changed_list = true;
                }
            }
        }
        for id in changes.leave {
            if let Some(index) = self.show.media().iter().position(|item| item.id == id) {
                applog::event("media", &format!("{} left a followed folder", self.show.media()[index].path.display()));
                media_changed |= self.remove_bin_item(index);
                changed_list = true;
            }
        }
        if media_changed {
            self.refresh_output();
        }
        if changed_list {
            self.save_media_list();
            if let Some(running) = self.running.as_ref() {
                running.home_window.request_redraw();
            }
        }
        if std::mem::take(&mut self.look_again) {
            self.check_folders(true);
        }
    }

    /// A file new to the bin from a followed folder has been read: it goes in, as an item the
    /// folder brought, if a followed playlist still lists it and it isn't in the bin already.
    /// True when it went in.
    fn add_from_folder(&mut self, id: Uuid, path: std::path::PathBuf, changed: std::time::SystemTime) -> bool {
        let listed = self.running.as_ref().is_some_and(|running| {
            running.home_state.media_playlists.iter().any(|p| p.folder.is_some() && p.has(&path))
        });
        if !listed || self.show.media().iter().any(|item| item.path == path) {
            self.bin_pictures.forget(id);
            return false;
        }
        applog::event("media", &format!("{} appeared in a followed folder", path.display()));
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.show.add_media(MediaItem { id, ..MediaItem::new(name, path.clone()) });
        self.followed_items.insert(id);
        self.folder_notes.seen.insert(path, changed);
        true
    }

    /// The media bin items used other than by a media playlist: shown by a slide (in any song)
    /// or a prop, in a playlist of the service, in a macro, or on screen.
    fn media_in_use(&self) -> std::collections::HashSet<Uuid> {
        let shown = |actions: &[sp_model::ShowAction]| {
            actions
                .iter()
                .filter_map(|action| match action {
                    sp_model::ShowAction::Media { id }
                    | sp_model::ShowAction::Announce { what: sp_model::Announced::Media { id } } => Some(*id),
                    _ => None,
                })
                .collect::<Vec<Uuid>>()
        };
        let mut used: std::collections::HashSet<Uuid> = self
            .show
            .presentations()
            .iter()
            .flat_map(|song| song.slides().flat_map(|(_, slide)| shown(&slide.actions)).collect::<Vec<_>>())
            .collect();
        used.extend(self.show.live_media().and_then(|index| self.show.media().get(index)).map(|item| item.id));
        used.extend(self.announced_media());
        if let Some(running) = self.running.as_ref() {
            let state = &running.home_state;
            for service in &state.services.list {
                used.extend(service.items.iter().filter_map(|item| match item {
                    service::Item::Media { id, .. } => Some(*id),
                    _ => None,
                }));
            }
            used.extend(state.macros.iter().flat_map(|m| shown(&m.actions)));
            used.extend(state.props.iter().flat_map(|prop| shown(&prop.slide.actions)));
        }
        used
    }

    /// Whether a video loops now: as it is set, unless a media playlist of several items is
    /// playing, when it plays to its end so the next can follow.
    fn loops_now(&self, item: &MediaItem) -> bool {
        let many = self.media_cycle.as_ref().is_some_and(|cycle| self.playlist_media(cycle.playlist).len() > 1);
        item.looping && !many
    }

    /// How the media on screen fills the screen.
    fn live_media_fit(&self) -> sp_model::ImageFit {
        self.show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .map_or(sp_model::ImageFit::Fill, |m| m.fit)
    }

    /// Plays the video on screen, if the media on screen is one, and stops the one that was.
    fn sync_video(&mut self) {
        let live = self
            .show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .filter(|item| media::is_video(&item.path))
            .cloned();
        if self.video.as_ref().map(|(id, _)| *id) == live.as_ref().map(|item| item.id) {
            // The same video: it may have been told to loop or not.
            let loops = live.as_ref().is_some_and(|item| self.loops_now(item));
            if let (Some((_, player)), Some(running)) = (self.video.as_mut(), self.running.as_mut())
                && let Some(shown) = running.home_state.video_live.as_mut()
                && shown.looping != loops
            {
                player.set_looping(loops);
                shown.looping = loops;
            }
            return;
        }
        if self.video.take().is_some() {
            applog::event("video", "stopped");
        }
        let Some(running) = self.running.as_mut() else { return };
        running.home_state.video_live = None;
        let Some(item) = live else { return };
        let started = Instant::now();
        let loops = self.loops_now(&item);
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else { return };
        match sp_video::Player::open(&item.path) {
            Ok(mut player) => {
                let engine = if player.uses_ffmpeg() { "FFmpeg" } else { "the system's player" };
                applog::event(
                    "video",
                    &format!("opened {} with {engine} in {:?}", item.path.display(), started.elapsed()),
                );
                player.set_looping(loops);
                // A quiet run (a test) makes no sound: someone may be working at this computer.
                if quiet {
                    player.set_volume(0.0);
                }
                player.play();
                running.home_state.video_live = Some(home::VideoLive {
                    name: item.name.clone(),
                    position: 0.0,
                    duration: None,
                    playing: true,
                    looping: loops,
                    muted: false,
                });
                self.video = Some((item.id, player));
            }
            Err(problem) => {
                applog::event("video", &format!("could not open {}: {problem}", item.path.display()));
                running.home_state.notice = Some(problem);
            }
        }
    }

    /// Plays the video in a box on the slide on screen, if it has one, and stops the one that was.
    /// The video loops, with its sound; it is the first visible picture box naming a video.
    fn sync_slide_video(&mut self) {
        // Every video in a box on the slide on screen, and in its theme's layout behind it: each
        // plays, looping, while the slide is on screen.
        let videos_in = |objects: &[sp_model::Object], into: &mut Vec<String>| {
            for object in objects.iter().filter(|object| !object.hidden) {
                if let sp_model::ObjectKind::Image { file, .. } = &object.kind
                    && media::is_video(std::path::Path::new(file))
                    && !into.contains(file)
                {
                    into.push(file.clone());
                }
            }
        };
        let mut wanted: Vec<String> = Vec::new();
        if let (Some(at), Some((_, slide))) = (self.show.live(), self.show.live_slide()) {
            videos_in(&slide.objects, &mut wanted);
            if let Some(behind) = sp_render::layout_objects(slide, self.theme_of(at.presentation)) {
                videos_in(&behind.objects, &mut wanted);
            }
        }
        // A video on a stage screen's layout plays too, without its sound (the room hears the
        // slide's).
        let for_the_room = wanted.clone();
        let on_stage = self.running.as_ref().map(|running| stage::shown_objects(&running.home_state));
        videos_in(&on_stage.unwrap_or_default(), &mut wanted);
        let playing: Vec<&String> = self.slide_videos.iter().map(|(file, _)| file).collect();
        if playing.len() == wanted.len() && playing.iter().zip(&wanted).all(|(have, want)| *have == want) {
            return;
        }
        // Those no longer wanted stop; those still wanted go on from where they are.
        let before = self.slide_videos.len();
        self.slide_videos.retain(|(file, _)| wanted.contains(file));
        if self.slide_videos.len() < before {
            applog::event("video", "stopped a video on the slide");
        }
        let quiet = self.quiet();
        let mut kept = std::mem::take(&mut self.slide_videos);
        for file in wanted {
            if let Some(at) = kept.iter().position(|(have, _)| *have == file) {
                self.slide_videos.push(kept.remove(at));
                continue;
            }
            let Some(path) = images::resolve(&file) else { continue };
            match sp_video::Player::open(&path) {
                Ok(mut player) => {
                    applog::event("video", &format!("playing {} in a box on the slide", path.display()));
                    player.set_looping(true);
                    if quiet || !for_the_room.contains(&file) {
                        player.set_volume(0.0);
                    }
                    player.play();
                    self.slide_videos.push((file, player));
                }
                Err(problem) => {
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.notice = Some(problem);
                    }
                }
            }
        }
    }

    /// Opens the pages web page boxes want, and closes the rest. The boxes on the slide on screen
    /// (and in the layout behind it) are on screen; the boxes of the song open in the window and
    /// of the slide being edited are loaded ahead, so a page has loaded and drawn by the time its
    /// slide is shown, rather than flashing in as it loads. A page is kept while it is wanted,
    /// and never loaded again only because its slide is shown again. A VoxLive screen's live
    /// address (it starts a paid translation) is never loaded ahead.
    fn sync_web_pages(&mut self) {
        fn boxes_in(objects: &[sp_model::Object], live: bool, into: &mut Vec<(String, String, u32, u32, bool)>) {
            for object in objects.iter().filter(|object| !object.hidden) {
                let sp_model::ObjectKind::Web { url } = &object.kind else { continue };
                if !sp_video::web::allowed(url) {
                    continue;
                }
                // A page that costs to open is opened only on screen; ahead of that, its free
                // stand-in is, for the operator's window.
                let stand_in = (!live).then(|| web_stand_in(url)).flatten();
                let url = stand_in.as_ref().unwrap_or(url);
                let place = object.frame.unwrap_or(sp_model::SlideRect {
                    x: 0.0,
                    y: 0.0,
                    w: sp_model::SLIDE_WIDTH,
                    h: sp_model::SLIDE_HEIGHT,
                });
                let key = sp_render::web_key(url, place.w, place.h);
                match into.iter_mut().find(|(have, ..)| *have == key) {
                    Some(found) => found.4 |= live,
                    None => into.push((key, url.clone(), place.w.round() as u32, place.h.round() as u32, live)),
                }
            }
        }
        let mut wanted = Vec::new();
        if let (Some(at), Some((_, slide))) = (self.show.live(), self.show.live_slide()) {
            boxes_in(&slide.objects, true, &mut wanted);
            if let Some(behind) = sp_render::layout_objects(slide, self.theme_of(at.presentation)) {
                boxes_in(&behind.objects, true, &mut wanted);
            }
        }
        // The stage screens' layouts are on screen too.
        if let Some(running) = self.running.as_ref() {
            boxes_in(&stage::shown_objects(&running.home_state), true, &mut wanted);
        }
        if let Some(song) = self.show.presentations().get(self.show.selected()) {
            for (_, slide) in song.slides() {
                boxes_in(&slide.objects, false, &mut wanted);
            }
        }
        if let Some(draft) = self.running.as_ref().and_then(|running| running.home_state.editing.as_ref()) {
            boxes_in(&draft.slide.objects, false, &mut wanted);
        }
        // A page that costs, once shown, is kept running a while after it leaves the screens,
        // so going back to it (a verse, then the captions again) is at once: it is running for
        // the service anyway.
        let now = Instant::now();
        for web in &mut self.web_pages {
            // One that costs and has just left the screens starts its while.
            if web.live
                && web_costs(web.page.address())
                && !wanted.iter().any(|(key, .., live)| *live && *key == web.key)
            {
                web.warm_until = Some(now + WEB_WARM_FOR);
            }
            if web.warm_until.is_some_and(|until| until > now) && !wanted.iter().any(|(key, ..)| *key == web.key) {
                let (width, height) = web.page.size();
                wanted.push((web.key.clone(), web.page.address().to_owned(), width, height, false));
            }
        }
        // Those on screen first, then as many loaded ahead as there is room for.
        wanted.sort_by_key(|(.., live)| !*live);
        wanted.truncate(MOST_WEB_PAGES);
        let unchanged = wanted.len() == self.web_pages.len()
            && wanted.iter().zip(&self.web_pages).all(|((key, .., live), web)| *key == web.key && *live == web.live);
        if unchanged {
            return;
        }
        let mut kept = std::mem::take(&mut self.web_pages);
        for (key, url, width, height, live) in wanted {
            if let Some(at) = kept.iter().position(|web| web.key == key) {
                let mut web = kept.remove(at);
                if web.live && !live && web_costs(web.page.address()) {
                    web.warm_until = Some(now + WEB_WARM_FOR);
                } else if live {
                    web.warm_until = None;
                }
                web.live = live;
                self.web_pages.push(web);
                continue;
            }
            if self.web_refused.contains(&key) {
                continue;
            }
            match sp_video::WebPage::open(&url, width, height) {
                Ok(page) => {
                    applog::event("web", &format!("opened a web page box ({})", web_host(&url)));
                    self.web_pages.push(WebBox {
                        key,
                        page,
                        live,
                        shown: true,
                        asked: None,
                        pictures: 0,
                        warm_until: None,
                    });
                }
                Err(problem) => {
                    applog::problem("web", &problem);
                    self.web_refused.push(key);
                }
            }
        }
        for gone in kept {
            applog::event("web", &format!("closed a web page box ({})", web_host(gone.page.address())));
            sp_render::forget_picture(&gone.key);
        }
    }

    /// Keeps the pages of web page boxes drawing: one on screen asks for a new picture as often
    /// as the screens can show one, one loaded ahead twice a second (its picture goes to the
    /// operator's window: the editor and thumbnails). Says when to look again.
    fn tick_web(&mut self) -> Option<Instant> {
        self.sync_web_pages();
        if self.web_pages.is_empty() {
            return None;
        }
        let now = Instant::now();
        // The addresses of the pages with a new picture, and of those with their first.
        let mut fresh: Vec<String> = Vec::new();
        let mut first: Vec<String> = Vec::new();
        let editing = self.running.as_ref().is_some_and(|running| running.home_state.editing.is_some());
        for web in &mut self.web_pages {
            // A page runs while it is on the screens, until its first picture (for the
            // thumbnails), while a slide is being edited, and while one that costs is kept warm
            // (it stays connected for the service); otherwise it rests, loaded, and runs again
            // at once when it goes on screen.
            let warm = web.warm_until.is_some_and(|until| until > now);
            let runs = web.live || web.pictures == 0 || editing || warm;
            if web.shown != runs {
                web.page.set_shown(runs);
                web.shown = runs;
            }
            if web.asked.is_none_or(|asked| now.duration_since(asked) >= web.every()) {
                web.page.capture();
                web.asked = Some(now);
            }
            if !web.live
                && let Some(frame) = web.page.frames().next()
                && let Some(picture) = sp_render::Picture::from_rgba(frame.width, frame.height, frame.rgba)
            {
                sp_render::show_picture(&web.key, picture);
                web.pictures += 1;
                if web.pictures == 1 {
                    first.push(web.page.address().to_owned());
                }
                fresh.push(web.page.address().to_owned());
            }
        }
        // A box shows a page by its own address, or (in this window) by its stand-in's.
        let shows = |pages: &[String], url: &str| {
            pages.iter().any(|page| page == url || web_stand_in(url).is_some_and(|stand_in| stand_in == *page))
        };
        // The Edit view's picture of the slide is drawn again with each new picture of a page it
        // shows (its slide's or its layout's); a picture of another page leaves it be.
        let canvas_shows = !fresh.is_empty()
            && self.running.as_ref().and_then(|running| running.home_state.editing.as_ref()).is_some_and(|draft| {
                let edited = self.running.as_ref().and_then(|r| r.home_state.theme_edit.as_ref());
                // As `update_canvas` draws it: a theme's layout with that theme, a slide with its
                // song's.
                let theme: &Theme = match edited {
                    Some(edit) => self.themes.get(edit.index).unwrap_or(&self.theme),
                    None => self.theme_of(draft.at.presentation),
                };
                thumbs::web_pages(Some(&draft.slide), theme).iter().any(|url| shows(&fresh, url))
            });
        if let Some(running) = self.running.as_mut() {
            // A page's first picture replaces its placeholder in the thumbnails that show it,
            // drawn again; the others are kept.
            if !first.is_empty() {
                running.thumbs.forget_showing(&mut running.egui_renderer, |url| shows(&first, url));
                running.home_window.request_redraw();
            }
            if canvas_shows {
                running.canvas_for = None;
                running.home_window.request_redraw();
            }
        }
        // Testing aid: SOFTPRESENTER_TRACE_WEB logs, once a second, each page's state: whether
        // it is on screen, how far it has loaded, and whether the page thinks it can be seen.
        if std::env::var_os("SOFTPRESENTER_TRACE_WEB").is_some()
            && self.web_traced.is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1))
        {
            self.web_traced = Some(now);
            for web in &self.web_pages {
                let (live, loading, pictures) = (web.live, web.page.loading(), web.pictures);
                web.page.ask(
                    "JSON.stringify([document.visibilityState, Math.round(performance.now()), window.raf || 0])",
                    move |text| {
                        applog::event(
                            "web",
                            &format!(
                                "on screen {live}, {loading:?}, {pictures} pictures for the window, page says {}",
                                text.unwrap_or_default()
                            ),
                        );
                    },
                );
            }
        }
        self.web_pages.iter().map(|web| web.asked.unwrap_or(now) + web.every()).min()
    }

    /// Does what Settings › Integrations asked of VoxLive, keeps the connection going, and shows
    /// its state in the window. Says when to look again.
    fn tick_voxlive(&mut self) -> Option<Instant> {
        // Testing aid: SOFTPRESENTER_CAPTURE_PREFERENCES=voxlive-look opens the first screen's look
        // once the screens and the look settings have come.
        if let Some(running) = self.running.as_mut()
            && running.home_state.voxlive.look.is_none()
            && running.home_state.voxlive.request.is_none()
            && !self.voxlive.schema.is_empty()
            && std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").as_deref() == Ok("voxlive-look")
            && let Some(first) = self.voxlive.screens.iter().find(|screen| !screen.url.is_empty())
            && !std::mem::replace(&mut running.capture_look_opened, true)
        {
            running.home_state.voxlive.request = Some(integrations::Request::OpenLook(first.id.clone()));
        }
        if let Some(running) = self.running.as_mut()
            && let Some(request) = running.home_state.voxlive.request.take()
        {
            running.home_state.voxlive.problem = None;
            match request {
                integrations::Request::Connect(key) => match self.voxlive.connect(&key) {
                    Ok(()) => applog::event("voxlive", "a key was given; checking it"),
                    Err(problem) => running.home_state.voxlive.problem = Some(problem),
                },
                integrations::Request::Disconnect => {
                    self.voxlive.disconnect();
                    applog::event("voxlive", "the key was forgotten on this computer");
                }
                integrations::Request::Refresh => self.voxlive.refresh(),
                // A screen's look opens with a preview: its display page in the preview mode for
                // an app, which connects to nothing and costs nothing.
                integrations::Request::OpenLook(screen) => {
                    if let Some(screen) = self.voxlive.screens.iter().find(|have| have.id == screen).cloned() {
                        running.home_state.voxlive.look = Some(voxlive::LookEdit::of(&screen));
                        self.voxlive_preview =
                            match sp_video::WebPage::open(&voxlive::preview_address(&screen.url), 960, 540) {
                                Ok(page) => Some(LookPreview {
                                    page,
                                    painted: String::new(),
                                    painted_at: None,
                                    changed_at: None,
                                    asked: None,
                                }),
                                Err(problem) => {
                                    applog::problem("voxlive", &problem);
                                    None
                                }
                            };
                    }
                }
                integrations::Request::SaveLook { anyway } => {
                    if let Some(look) = running.home_state.voxlive.look.as_mut() {
                        let version = (!anyway && !look.version.is_empty()).then(|| look.version.clone());
                        match self.voxlive.save_look(&look.screen, version, look.changes()) {
                            Ok(()) => {
                                look.saving = true;
                                look.outcome = None;
                            }
                            Err(problem) => look.outcome = Some(problem),
                        }
                    }
                }
                integrations::Request::CloseLook => {
                    running.home_state.voxlive.look = None;
                    self.voxlive_preview = None;
                }
            }
        }
        // A look closes with Settings, or when Settings shows another pane: its page goes with it.
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            let showing = state.preferences_open && home::PANES.get(state.preferences_pane) == Some(&"Integrations");
            if !showing && state.voxlive.look.is_some() {
                state.voxlive.look = None;
            }
            if state.voxlive.look.is_none() {
                self.voxlive_preview = None;
            }
        }
        let mut at = self.voxlive.tick();
        // How a save went: in the look, if it is open, and for `sp voxlive`.
        if let Some((screen, outcome)) = self.voxlive.saved.take() {
            let said = match &outcome {
                Ok(saved) => format!("Saved the look of {} to VoxLive.", saved.label()),
                Err(voxlive::Saving::Changed(now)) => {
                    format!("The look of {} wasn't saved: it changed in VoxLive since it was opened.", now.label())
                }
                Err(voxlive::Saving::Failed(why)) => format!("The look wasn't saved: {why}"),
            };
            if let Some(look) =
                self.running.as_mut().and_then(|r| r.home_state.voxlive.look.as_mut()).filter(|l| l.screen == screen)
            {
                look.saving = false;
                match outcome {
                    Ok(saved) => {
                        look.take(&saved);
                        look.outcome = Some("Saved to VoxLive.".to_owned());
                    }
                    Err(voxlive::Saving::Changed(now)) => look.changed_there = Some(*now),
                    Err(voxlive::Saving::Failed(why)) => look.outcome = Some(why),
                }
            }
            self.voxlive_said = Some(said);
        }
        // The preview paints the look as it is changed (and again each second, in case the page
        // wasn't ready), and its pictures go to the look's page, about ten a second.
        if let (Some(preview), Some(running)) = (self.voxlive_preview.as_mut(), self.running.as_mut())
            && let Some(look) = running.home_state.voxlive.look.as_mut()
            && preview.page.loading() == sp_video::web::Loading::Loaded
        {
            let script = voxlive::apply_script(&look.style, &look.language);
            let now = Instant::now();
            if preview.painted != script {
                preview.changed_at = Some(now);
            }
            if preview.painted != script
                || preview.painted_at.is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1))
            {
                preview.page.ask(&script, |_| {});
                preview.painted = script;
                preview.painted_at = Some(now);
            }
            // Ten pictures a second for two seconds after a change (and while the page first
            // settles), one a second otherwise.
            let settling = preview.changed_at.is_none_or(|at| now.duration_since(at) < Duration::from_secs(2));
            let every = if settling { Duration::from_millis(100) } else { Duration::from_secs(1) };
            if preview.asked.is_none_or(|asked| now.duration_since(asked) >= every) {
                preview.page.capture();
                preview.asked = Some(now);
            }
            if let Some(frame) = preview.page.frames().next() {
                look.preview = Some(frame);
                look.fresh = true;
                for panel in &running.panels {
                    panel.window.request_redraw();
                }
            }
            let next = if settling { Duration::from_millis(100) } else { Duration::from_secs(1) };
            at = [at, Some(now + next)].into_iter().flatten().min();
        }
        // The VoxLive item follows the screens VoxLive lists. It is made again only when they
        // have changed, or something else has taken its place, not each time the app wakes.
        let in_place = self.show.presentations().get(self.voxlive_slot);
        let up_to_date = self.voxlive_item_from.as_ref().is_some_and(|(screens, item)| {
            *screens == self.voxlive.screens && in_place.is_some_and(|have| Arc::ptr_eq(have, item))
        });
        if !up_to_date {
            let item = Arc::new(voxlive::item(&self.voxlive.screens));
            if in_place.is_some_and(|have| **have != *item) {
                self.show.replace_presentation(self.voxlive_slot, item);
                if let Some(running) = self.running.as_mut() {
                    running.thumbs.forget(voxlive::ITEM_ID, &mut running.egui_renderer);
                }
                self.refresh_output();
            }
            let now = self.show.presentations().get(self.voxlive_slot).cloned();
            self.voxlive_item_from = now.map(|item| (self.voxlive.screens.clone(), item));
        }
        // A screen chosen in the VoxLive tab goes on screen, in place of what was there.
        let chosen = self.running.as_mut().and_then(|running| running.home_state.voxlive_go.take());
        if let Some(slide) = chosen
            && self.show.presentations().get(self.voxlive_slot).is_some_and(|item| slide < item.slide_count())
        {
            self.apply(vec![Command::GoTo(sp_engine::SlideRef { presentation: self.voxlive_slot, slide })]);
        }
        if let Some(running) = self.running.as_mut() {
            let view = &mut running.home_state.voxlive;
            if view.schema != self.voxlive.schema {
                view.schema = self.voxlive.schema.clone();
            }
            if view.status != self.voxlive.status
                || view.screens != self.voxlive.screens
                || !self.voxlive.key_hint_is(view.key_hint.as_deref())
            {
                view.status = self.voxlive.status.clone();
                view.screens = self.voxlive.screens.clone();
                view.key_hint = self.voxlive.key_hint();
                running.home_window.request_redraw();
                for panel in &running.panels {
                    panel.window.request_redraw();
                }
            }
        }
        at
    }

    /// Keeps the videos in boxes on the slide going; if the system's player can't play one,
    /// FFmpeg takes it over. Says when to look again.
    fn tick_slide_video(&mut self) -> Option<Instant> {
        if self.slide_videos.is_empty() {
            return None;
        }
        let quiet = self.quiet();
        let mut wait = Duration::from_secs(1);
        let mut switched = false;
        for (file, player) in &mut self.slide_videos {
            wait = wait.min(player.tick());
            // A video FFmpeg couldn't take over either isn't tried again on every tick (each try
            // starts FFmpeg's prober on this thread).
            if player.problem().is_none() || player.uses_ffmpeg() || self.slide_video_refused.contains(file) {
                continue;
            }
            let fallback = images::resolve(file).map(|path| (sp_video::Player::open_with_ffmpeg(&path), path));
            let Some((Ok(mut fallback), path)) = fallback else {
                self.slide_video_refused.insert(file.clone());
                continue;
            };
            {
                applog::event("video", &format!("playing {} in a box with FFmpeg instead", path.display()));
                fallback.set_looping(true);
                if quiet {
                    fallback.set_volume(0.0);
                }
                fallback.play();
                *player = fallback;
                switched = true;
            }
        }
        if switched {
            self.refresh_output();
        }
        Some(Instant::now() + wait)
    }

    /// Keeps the video on screen going (a looping one starts again at its end) and the
    /// operator's view of it up to date. Says when to look again.
    fn tick_video(&mut self) -> Option<Instant> {
        let (_, player) = self.video.as_mut()?;
        let wait = player.tick();
        let running = self.running.as_mut()?;
        if let Some(problem) = player.problem() {
            applog::event("video", &problem);
            // The system's player couldn't play it (an AV1 video on a Mac without AV1 hardware,
            // say): FFmpeg plays it instead, from where it was.
            if !player.uses_ffmpeg()
                && let Some((id, _)) = self.video.take()
                && let Some(item) = self.show.media().iter().find(|item| item.id == id).cloned()
            {
                let loops = self.loops_now(&item);
                let quiet = self.quiet();
                let running = self.running.as_mut()?;
                match sp_video::Player::open_with_ffmpeg(&item.path) {
                    Ok(mut fallback) => {
                        applog::event("video", &format!("playing {} with FFmpeg instead", item.path.display()));
                        fallback.set_looping(loops);
                        if quiet {
                            fallback.set_volume(0.0);
                        }
                        fallback.play();
                        self.video = Some((id, fallback));
                        // The screens take their pictures from the new player.
                        self.refresh_output();
                    }
                    Err(problem) => {
                        running.home_state.notice = Some(problem);
                        running.home_state.video_live = None;
                    }
                }
                return Some(Instant::now() + Duration::from_millis(100));
            }
            running.home_state.notice = Some(problem);
            running.home_state.video_live = None;
            self.video = None;
            running.home_window.request_redraw();
            return None;
        }
        if let Some(live) = running.home_state.video_live.as_mut() {
            let (position, duration, playing) = (player.position(), player.duration(), player.playing());
            // The window shows whole seconds, so it is drawn again only when one passes.
            let changed =
                live.position.floor() != position.floor() || live.duration != duration || live.playing != playing;
            *live = home::VideoLive { position, duration, playing, ..live.clone() };
            if changed {
                running.home_window.request_redraw();
            }
        }
        Some(Instant::now() + wait)
    }

    /// Carries out what the operator asked of the video on screen.
    fn handle_video(&mut self) {
        use home::VideoRequest;
        let Some(running) = self.running.as_mut() else { return };
        let Some(request) = running.home_state.video_request.take() else { return };
        let (Some((_, player)), Some(live)) = (self.video.as_mut(), running.home_state.video_live.as_mut()) else {
            return;
        };
        match request {
            VideoRequest::TogglePause if player.playing() => player.pause(),
            VideoRequest::TogglePause => player.play(),
            VideoRequest::Restart => {
                player.seek(0.0);
                player.play();
            }
            VideoRequest::Looping(on) => {
                player.set_looping(on);
                live.looping = on;
            }
            VideoRequest::Muted(on) => {
                player.set_volume(if on { 0.0 } else { 1.0 });
                live.muted = on;
            }
            VideoRequest::Seek(to) => player.seek(to),
            VideoRequest::Scrub(to) if !player.uses_ffmpeg() => player.seek(to),
            VideoRequest::Scrub(_) => {}
        }
        live.playing = player.playing();
        live.position = player.position();
        running.home_window.request_redraw();
    }

    /// Carries out what the Audio tab asked of the player, and tells the tab what is playing.
    fn handle_audio(&mut self) {
        use home::AudioRequest;
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        match state.audio_request.take() {
            Some(AudioRequest::Add) => {
                let chosen = rfd::FileDialog::new()
                    .set_title("Add Audio")
                    .add_filter("Audio", &audio::AUDIO_ENDINGS)
                    .pick_files()
                    .unwrap_or_default();
                let tracks = &mut state.audio.shown_mut().tracks;
                for path in chosen {
                    if !tracks.iter().any(|track| track.path == path) {
                        tracks.push(audio::Track::of(path));
                        state.audio_changed = true;
                    }
                }
            }
            Some(AudioRequest::Play(id)) => {
                // A track chosen while another plays crossfades from it, if a crossfade is set.
                let fade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
                if let Some(track) = state.audio.track(id) {
                    applog::event("audio", &format!("play {} (crossfade {fade:?})", track.path.display()));
                }
                if let Some(track) = state.audio.track(id)
                    && let Err(problem) = self.player.play_fading(track, fade)
                {
                    state.notice = Some(problem);
                }
            }
            Some(AudioRequest::TogglePause) => self.player.toggle_pause(),
            Some(AudioRequest::Stop) => self.player.stop(),
            Some(AudioRequest::Skip(seconds)) => self.player.skip(seconds),
            Some(AudioRequest::Volume(volume)) => self.player.set_volume(volume),
            Some(AudioRequest::Remove(id)) => {
                if state.audio_playing.track == Some(id) {
                    self.player.stop();
                }
                state.audio.shown_mut().tracks.retain(|track| track.id != id);
                state.audio_changed = true;
            }
            None => {}
        }
        if std::mem::take(&mut state.audio_changed)
            && let Err(problem) = audio::save(&state.audio)
        {
            state.notice = Some(problem);
        }
        self.tick_audio();
    }

    /// Keeps the audio going by the clock: fades finish, and with Play Through on the next track
    /// starts as one ends (fading in over its end, if a crossfade is set). Tells the Audio tab
    /// what is playing, and says when to look again.
    fn tick_audio(&mut self) -> Option<Instant> {
        let fading = self.player.tick();
        let running = self.running.as_mut()?;
        let state = &mut running.home_state;
        let fade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
        let pick = |count: usize| (Uuid::new_v4().as_u128() % count as u128) as usize;
        if let Some(ended) = self.player.ended() {
            let name = state.audio.track(ended).map_or("", |track| track.name.as_str());
            applog::event("audio", &format!("ended {name}"));
            if state.audio.through
                && let Some(next) = state.audio.after(ended, pick)
                && let Err(problem) = self.player.play(next)
            {
                state.notice = Some(problem);
            }
        } else if let playing = self.player.playing()
            && state.audio.through
            && !fade.is_zero()
            && !playing.paused
            && let Some(id) = playing.track
            // A track too short to crossfade from just plays to its end.
            && playing.length.is_some_and(|length| length > fade * 2)
            && self.player.left().is_some_and(|left| left <= fade)
            && let Some(next) = state.audio.after(id, pick)
            && let Err(problem) = self.player.play_fading(next, fade)
        {
            state.notice = Some(problem);
        }
        let playing = self.player.playing();
        let shown = |p: &audio::Playing| (p.track, p.position.as_secs(), p.paused);
        if shown(&playing) != shown(&state.audio_playing) {
            running.home_window.request_redraw();
        }
        state.audio_playing = playing;
        // While a track plays or fades, look again soon.
        ((playing.track.is_some() && !playing.paused) || fading).then(|| Instant::now() + Duration::from_millis(100))
    }

    /// Carries out the actions waiting to be done: a macro's, or a slide's.
    fn run_actions(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let actions = std::mem::take(&mut running.home_state.actions_to_run);
        let depth = std::mem::take(&mut running.home_state.actions_depth);
        // Two slides whose actions go to each other's songs would send the show back and forth
        // for ever; a chain of slides' actions stops after a few steps.
        if actions.is_empty() || depth >= MAX_ACTION_CHAIN {
            return;
        }
        self.action_depth = Some(depth);
        // Each action is done in its turn, so a later one sees what an earlier one did:
        // Clear All, then Show Prop, leaves the prop on screen.
        for action in actions {
            let Some(running) = self.running.as_mut() else { break };
            match action_commands(&self.show, &running.home_state, &action) {
                Some(commands) => self.apply(commands),
                None => do_state_action(&mut running.home_state, &action, Instant::now()),
            }
        }
        self.action_depth = None;
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// With Continuous Playlist on, Next on the last slide of a song or Bible passage of the
    /// playlist in front goes on into the next song or passage instead of staying put (a slide
    /// that goes on by itself, the command line). Anything else is left as it is.
    fn continue_playlist(&mut self, command: Command) -> Vec<Command> {
        let continuous = self.running.as_ref().is_some_and(|running| running.home_state.continuous);
        // Only at the end of the song's play order, with the last slide's builds all done.
        if command != Command::Next
            || !continuous
            || self.show.live().is_none()
            || self.show.upcoming().is_some()
            || self.show.builds_left()
        {
            return vec![command];
        }
        let (stops, _) = self.playlist_stops();
        let here = self.cursor_item(&stops);
        match running::go_on(&stops, here, true) {
            Some(item) => self.start_commands(item, false, &stops).unwrap_or_else(|| vec![command]),
            None => vec![command],
        }
    }

    /// The playlist in front as the keys that run the show see it, with each item's title.
    fn playlist_stops(&self) -> (Vec<running::Stop>, Vec<String>) {
        use running::Stop;
        let Some(running) = self.running.as_ref() else { return (Vec::new(), Vec::new()) };
        let state = &running.home_state;
        let items = &state.services.front().items;
        let has_slides = |at: usize| {
            self.show.presentations().get(at).is_some_and(|song| {
                song.order().into_iter().any(|slide| song.slide(slide).is_some_and(|(_, one)| one.enabled))
            })
        };
        let stops = items
            .iter()
            .map(|item| match item {
                service::Item::Song { key, .. } => match studio::playlist_song(&self.show, state, key) {
                    Some(at) => Stop::Song { at, slides: has_slides(at) },
                    None => Stop::Nothing,
                },
                service::Item::Passage(_) => Stop::Passage,
                service::Item::Media { id, .. } => Stop::Media(self.show.media().iter().position(|m| m.id == *id)),
                service::Item::MediaPlaylist { id, .. } => Stop::MediaPlaylist(!self.playlist_media(*id).is_empty()),
                service::Item::Header { .. } | service::Item::Placeholder { .. } => Stop::Nothing,
            })
            .collect();
        (stops, items.iter().map(|item| item.title().to_owned()).collect())
    }

    /// Which item of the playlist in front the slide on screen (or last on screen) is from. A
    /// song the playlist holds twice: the item it was put on screen from, then the open one,
    /// then the first.
    fn cursor_item(&self, stops: &[running::Stop]) -> running::Here {
        use running::Here;
        let Some(cursor) = self.show.cursor() else { return Here::Nothing };
        let Some(running) = self.running.as_ref() else { return Here::Nothing };
        let state = &running.home_state;
        let holds: Vec<usize> = if self.bible_slots.contains(&cursor.presentation) {
            let title = self.show.presentations().get(cursor.presentation).map(|p| p.title.as_str());
            let items = &state.services.front().items;
            (0..items.len())
                .filter(
                    |at| matches!(&items[*at], service::Item::Passage(saved) if Some(saved.lookup.as_str()) == title),
                )
                .collect()
        } else {
            (0..stops.len())
                .filter(|at| matches!(stops[*at], running::Stop::Song { at, .. } if at == cursor.presentation))
                .collect()
        };
        [state.live_item, state.services.chosen]
            .into_iter()
            .flatten()
            .find(|item| holds.contains(item))
            .or_else(|| holds.first().copied())
            .map_or(Here::Elsewhere, Here::Item)
    }

    /// Next (`forward`) or Previous, from a key, the menu or the command line: the keys act on
    /// the open item, and leave a slide picked for Enter (see `running`). Returns what was said
    /// instead, if anything was.
    fn step_keys(&mut self, forward: bool) -> Option<String> {
        let command = if forward { Command::Next } else { Command::Previous };
        let running = self.running.as_ref()?;
        let state = &running.home_state;
        let picked = state.picked_word(&self.show);
        let showing = self.show.live().is_some();
        // In the editors the show steps as it always has; on the Bible page too, unless a verse
        // picked waits for Enter with nothing on screen.
        let decided = if state.edit_kind().is_some() {
            running::Do::Show(command)
        } else if state.bible_open {
            running::held_for_enter(picked, showing).unwrap_or(running::Do::Show(command))
        } else {
            let open = if state.playlist_open { state.services.chosen } else { None };
            let continuous = state.continuous;
            let (stops, titles) = self.playlist_stops();
            let here = self.cursor_item(&stops);
            let builds_back = self.show.live().is_some() && self.show.build_step() > 0;
            let situation = running::Situation {
                stops: &stops,
                titles: &titles,
                open,
                here,
                at_end: self.show.cursor().is_some() && self.show.upcoming().is_none() && !self.show.builds_left(),
                at_start: self.show.cursor().is_some() && self.show.preceding().is_none() && !builds_back,
                continuous,
                picked,
                showing,
            };
            running::step(&situation, forward)
        };
        match decided {
            running::Do::Show(command) => self.apply(vec![command]),
            running::Do::Start { item, last } => self.start_item(item, last),
            running::Do::Say(words) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.say(words.clone());
                    running.home_window.request_redraw();
                }
                return Some(words);
            }
        }
        None
    }

    /// Space and the arrows while a slide is picked and nothing is on screen (the owner's rule,
    /// 2026-11-21): Space puts the picked slide on, as Enter does; → and ← move the pick to the
    /// next or the previous slide of its passage or song, in the order sung, putting nothing on.
    /// With something on screen they run it as ever (false: not taken here), so a song can be
    /// finished with a reading picked and waiting for Enter.
    fn pick_key(&mut self, key: &Key) -> bool {
        let Some(running) = self.running.as_ref() else { return false };
        let state = &running.home_state;
        let picked = state.pick(&self.show);
        let decided = running::pick_key(
            matches!(key, Key::Named(NamedKey::Space)),
            matches!(key, Key::Named(NamedKey::ArrowRight)),
            matches!(key, Key::Named(NamedKey::ArrowLeft)),
            picked.is_some(),
            self.show.live().is_some(),
            state.edit_kind().is_some(),
        );
        let (running::PickKey::Move(forward), Some(at)) = (decided, picked) else {
            if decided == running::PickKey::TakeIt {
                self.enter_key();
                return true;
            }
            return false;
        };
        let word = state.picked_word(&self.show).unwrap_or("slide");
        let bible = state.picked.as_ref().is_some_and(|picked| picked.passage.is_some());
        let order = self.show.presentations().get(at.presentation).map(|song| song.order()).unwrap_or_default();
        let next = running::pick_beside(&order, at.slide, forward);
        let Some(running) = self.running.as_mut() else { return true };
        match next {
            Some(slide) => running.home_state.set_pick(
                &self.show,
                sp_engine::SlideRef { presentation: at.presentation, slide },
                bible,
            ),
            None => running.home_state.say(format!("That is the {} {word}.", if forward { "last" } else { "first" })),
        }
        running.home_window.request_redraw();
        true
    }

    /// Enter: the slide picked goes on screen; otherwise the open item of the playlist from its
    /// first slide, with a library listed the song chosen, and in the Bible view nothing (see
    /// `running::enter`).
    fn enter_key(&mut self) {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return };
        let picked = state.pick(&self.show);
        let listed = if state.bible_open {
            running::Listed::Bible
        } else if state.playlist_open {
            running::Listed::Playlist(state.services.chosen)
        } else {
            running::Listed::Library
        };
        let stops = match listed {
            running::Listed::Playlist(Some(_)) if picked.is_none() => self.playlist_stops().0,
            _ => Vec::new(),
        };
        match running::enter(&stops, listed, picked) {
            Some(running::Do::Show(command)) => self.apply(vec![command]),
            Some(running::Do::Start { item, last }) => self.start_item(item, last),
            Some(running::Do::Say(_)) | None => {}
        }
    }

    /// Puts an item of the playlist in front on screen: a song or a Bible passage at its first
    /// slide (its last when `last`), a picture or video shown, a media playlist played. It
    /// becomes the open item.
    fn start_item(&mut self, item: usize, last: bool) {
        let (stops, _) = self.playlist_stops();
        if let Some(running::Stop::MediaPlaylist(true)) = stops.get(item) {
            let id = self.running.as_ref().and_then(|r| match r.home_state.services.front().items.get(item) {
                Some(service::Item::MediaPlaylist { id, .. }) => Some(*id),
                _ => None,
            });
            if let Some(running) = self.running.as_mut() {
                running.home_state.services.chosen = Some(item);
            }
            if let Some(id) = id {
                self.play_media_playlist(id);
            }
            return;
        }
        if let Some(commands) = self.start_commands(item, last, &stops) {
            self.apply(commands);
        }
    }

    /// What puts a song, a passage or a picture of the playlist in front on screen; a passage is
    /// made into slides first. The item becomes the open one, and the one on screen.
    fn start_commands(&mut self, item: usize, last: bool, stops: &[running::Stop]) -> Option<Vec<Command>> {
        let commands = match stops.get(item)? {
            running::Stop::Song { at, .. } => {
                let song = self.show.presentations().get(*at)?;
                let order = song.order();
                let enabled = |step: &usize| song.slide(order[*step]).is_some_and(|(_, slide)| slide.enabled);
                let step = if last { (0..order.len()).rev().find(enabled) } else { (0..order.len()).find(enabled) }?;
                vec![Command::GoToStep { presentation: *at, step }]
            }
            running::Stop::Passage => vec![Command::GoTo(self.open_passage_now(item, last)?)],
            running::Stop::Media(Some(at)) => vec![Command::ShowMedia(*at)],
            _ => return None,
        };
        let running = self.running.as_mut()?;
        running.home_state.services.chosen = Some(item);
        if !matches!(stops.get(item), Some(running::Stop::Media(_))) {
            running.home_state.live_item = Some(item);
        }
        Some(commands)
    }

    /// Makes a Bible passage the playlist in front keeps into slides now, as a click on one of
    /// its tiles does (with the translations and choices it was kept with), and puts it on the
    /// Bible page's place that is not on screen. Returns its first slide (its last when `last`).
    fn open_passage_now(&mut self, item: usize, last: bool) -> Option<sp_engine::SlideRef> {
        let live = self.show.live().map(|at| at.presentation);
        let running = self.running.as_mut()?;
        let state = &mut running.home_state;
        let service::Item::Passage(saved) = state.services.front().items.get(item)?.clone() else { return None };
        state.bible.open_saved(&saved, live);
        state.bible.go_live = None;
        let (slot, passage) = state.bible.pending.take()?;
        let count = passage.slide_count();
        if count == 0 {
            return None;
        }
        self.show.replace_presentation(slot, Arc::new(passage));
        Some(sp_engine::SlideRef { presentation: slot, slide: if last { count - 1 } else { 0 } })
    }

    /// Tells the output what is on each layer now, then redraws the operator's preview.
    fn refresh_output(&mut self) {
        self.sync_video();
        self.sync_slide_video();
        self.sync_web_pages();
        let video = self.video.as_ref().map(|(_, player)| player.frames());
        // Videos in boxes, and the pages of web page boxes on screen: the screens take each
        // one's newest picture as it comes.
        let slide_video: Vec<(String, sp_video::Frames)> = self
            .slide_videos
            .iter()
            .map(|(file, player)| (file.clone(), player.frames()))
            .chain(self.web_pages.iter().filter(|web| web.live).map(|web| (web.key.clone(), web.page.frames())))
            .collect();
        let media_fit = self.live_media_fit();
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
        let about = self.stage_about();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let slide = self.show.live_slide().map(|(_, slide)| Arc::new(slide.clone()));
        // The slide Next would show, in the song's play order, for the stage screens.
        let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide).map(|(_, slide)| Arc::new(slide.clone()))
        });
        // The screens get the item's full-size picture; until it is read, its small copy.
        let live_item = self.show.live_media().and_then(|index| self.show.media().get(index));
        let media = self.bin_pictures.for_screens(live_item.map(|item| (item.id, item.path.as_path())));
        let next_media = next.as_deref().and_then(home::media_of);
        // The audience comes first: tell the output before drawing anything for the operator.
        if let Some(output) = &running.output {
            let frame = Frame {
                media: media.clone(),
                video,
                media_fit,
                slide_video,
                slide: slide.clone(),
                theme: theme.clone(),
                enabled: self.show.output_enabled(),
                // A slide's own transition, if it has one; then its song's; otherwise the one
                // chosen for all.
                transition: self
                    .show
                    .live_slide()
                    .and_then(|(song, slide)| slide.transition.or(song.transition))
                    .unwrap_or(running.home_state.transition),
                media_transition: running.home_state.media_transition,
                next,
                about,
                build_step: self.show.build_step(),
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_stopped = true;
            }
        }
        self.refresh_preview(true);
        // The picture or video the slide Next would show is read ahead, so it goes on sharp.
        if let Some(item) = next_media.and_then(|id| self.show.media().iter().find(|item| item.id == id)) {
            self.bin_pictures.read_ahead(item.id, &item.path);
        }
    }

    /// The song on screen's title and its slide's group, for the stage screens.
    fn stage_about(&self) -> stage::About {
        match self.show.live_slide() {
            Some((song, _)) => {
                let group = self.show.live().and_then(|at| song.slide(at.slide)).map(|(group, _)| group.name.clone());
                stage::About { title: Some(song.title.clone()), group }
            }
            None => stage::About::default(),
        }
    }

    /// Redraws the Program preview: what the audience sees, or what the stage sees. `fade` lets
    /// it fade from the last picture as the screens do; a clock's tick doesn't fade.
    fn refresh_preview(&mut self, fade: bool) {
        let media_fit = self.live_media_fit();
        let announcement = self.announcing.as_ref().map(|a| a.shown.clone());
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
        let about = self.stage_about();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The preview is small: it takes the small copy.
        let media = self
            .show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .and_then(|item| self.bin_pictures.small(item.id))
            .cloned();
        let state = &running.home_state;
        // The slide as far as its builds have got, with nothing moving, and its linked text filled in.
        let step = self.show.build_step();
        let next_slide = self.show.live().and(self.show.upcoming()).and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide).map(|(_, slide)| slide.clone())
        });
        let copyright = running.overlay_sent.as_ref().and_then(|sent| sent.copyright.clone());
        let values = links::Values {
            timers: &state.timers,
            title: about.title.as_deref(),
            group: about.group.as_deref(),
            next: next_slide.as_ref(),
            stage_message: state.stage_message_live.then_some(state.stage_message.as_str()),
            copyright: copyright.as_deref(),
        };
        let slide = self.show.live_slide().map(|(_, slide)| {
            let built = slide.built(&theme, sp_model::BuildClock::settled(step)).0;
            Arc::new(links::fill(&built, &values).into_owned())
        });
        running.stage_preview_at = Instant::now();
        running.preview_shows_stage = state.preview_stage;
        if state.preview_stage {
            // The stage screens' picture: the time, this slide, the next, a timer, the stage message.
            let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
                let song = self.show.presentations().get(at.presentation)?;
                song.slide(at.slide).map(|(_, slide)| slide.clone())
            });
            let overlay = running.overlay_sent.clone().unwrap_or_default();
            // The layout of the stage screen looked at, or of the first, as the people on stage
            // see it.
            let stage_screens = || state.screens.iter().filter(|screen| screen.kind == screens::Kind::Stage);
            let wanted = stage_screens()
                .find(|screen| Some(screen.id) == state.preview_screen)
                .or_else(|| stage_screens().next())
                .and_then(|s| s.stage_layout);
            let template = stage::layout_for(&state.stage_layouts, wanted);
            let picture = stage::slide(slide.as_deref(), next.as_ref(), &overlay, &about, &template);
            let layers = Layers {
                media_fit: sp_model::ImageFit::Fill,
                media: None,
                slide: Some(&picture),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            };
            let layout = stage::theme_for(&template);
            let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &layout);
            running.preview = now;
            running.preview_before = before;
            let now = Instant::now();
            running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
            running.stage_preview_at = now;
            return;
        }
        // The operator's preview shows the props and a message over the slide, as the audience
        // sees them.
        let message = state.message_live.and_then(|(id, _)| {
            let message = state.messages.iter().find(|message| message.id == id)?;
            let words = messages::fill_in(&message.text, &state.timers, Instant::now(), timers::seconds_into_day());
            Some(messages::slide(&words, message.look))
        });
        let mut live_props: Vec<Slide> = state
            .props_live
            .iter()
            .filter_map(|(id, _)| state.props.iter().find(|prop| prop.id == *id).map(|prop| prop.slide.clone()))
            .collect();
        // The copyright line as last worked out; a change to it redraws the preview again.
        live_props.extend(
            running.overlay_sent.as_ref().and_then(|sent| sent.copyright.as_deref()).map(props::copyright_slide),
        );
        let over = props::overlay(&live_props, message.as_ref());
        let layers = Layers {
            media_fit,
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: over.as_ref().filter(|_| self.show.output_enabled()),
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: announcement.as_ref().map(|shown| (&*shown.slide, &*shown.theme)),
        };
        let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &theme);
        running.preview = now;
        running.preview_before = before;
        let now = Instant::now();
        running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
    }

    /// Notes a file dropped on the window from the Finder. The system hands over several
    /// dropped together one after another; they are taken in together once all have come (see
    /// [`App::finish_drop`]), each to the place the pointer was over at the drop.
    fn file_dropped(&mut self, path: PathBuf) {
        if self.dropped.is_empty() {
            self.drop_pointer =
                self.running.as_ref().and_then(|running| file_drop_pointer(&running.home_window, &running.egui_ctx));
        }
        self.dropped.push(path);
    }

    /// Takes in the files dropped on the window since the last look: songs to the songs (into the
    /// playlist or library dropped on, if any, else the library shown), never into the media bin;
    /// pictures and videos into the media bin (and the playlist dropped on), or onto the slide in
    /// the editor; presentations and PDFs as slides; a Bible installed. See `file_drop.rs`.
    fn finish_drop(&mut self) {
        if self.dropped.is_empty() {
            return;
        }
        let paths = std::mem::take(&mut self.dropped);
        let pointer = self.drop_pointer.take();
        let files: Vec<(PathBuf, file_drop::Kind)> = paths
            .into_iter()
            .map(|path| {
                let kind = file_drop::kind_of(&path);
                (path, kind)
            })
            .collect();
        let target = self.running.as_mut().and_then(|running| {
            let state = &mut running.home_state;
            let target = file_drop::target_at(&state.file_drag.places, &files, pointer);
            // The drag is over: the system says nothing more after a drop.
            state.drop_hover = false;
            state.file_drag = file_drop::FileDrag::default();
            running.home_window.request_redraw();
            target
        });
        applog::event("drop", &format!("{} dropped at {pointer:?} (window points) on {target:?}", files.len()));
        let route = file_drop::route(&files, target.as_ref());
        // Pictures first and songs last, so that what is said about the songs stays in sight.
        let mut media_to = route.media_playlist;
        for path in &route.media {
            self.add_dropped_media(path, &mut media_to);
        }
        for path in &route.other {
            self.add_dropped_file(path);
        }
        if !route.slides.is_empty() {
            self.import_songs(&route.slides);
        }
        if !route.songs.is_empty() {
            let to = self.songs_to(route.songs_to.as_ref());
            self.import_songs_to(&route.songs, &to);
        }
    }

    /// Where songs dropped on a playlist or library go.
    fn songs_to(&self, target: Option<&file_drop::DropTarget>) -> SongsTo {
        match target {
            Some(file_drop::DropTarget::Library(library)) => SongsTo { library: Some(*library), playlist: None },
            Some(file_drop::DropTarget::Playlist { index, after }) => {
                let name = self
                    .running
                    .as_ref()
                    .and_then(|r| r.home_state.services.list.get(*index))
                    .map(|playlist| playlist.name.clone());
                let playlist = name.map(|name| PlaylistSpot { index: *index, name, after: *after });
                SongsTo { library: None, playlist }
            }
            _ => SongsTo::default(),
        }
    }

    /// A picture or video dropped on the window: in the editor (a song's or a theme's), onto the
    /// slide or the layout, as Media… puts it there; else into the media bin, and into the
    /// playlist it was dropped on (`to`: the playlist, and the item it goes after, moved on past
    /// it for the next).
    fn add_dropped_media(&mut self, path: &std::path::Path, to: &mut Option<(usize, Option<usize>)>) {
        if let Some(running) = self.running.as_mut()
            && running.home_state.editing.is_some()
        {
            let state = &mut running.home_state;
            match images::import(path) {
                Ok(imported) => {
                    applog::event("editor", &format!("dropped {} on the slide", path.display()));
                    state.picture_to_add.push(imported);
                }
                Err(problem) => state.notice = Some(problem),
            }
            running.home_window.request_redraw();
            return;
        }
        if self.add_media_file(path).is_err() {
            return;
        }
        let Some((index, after)) = *to else { return };
        let Some(item) = self.show.media().iter().find(|item| item.path == path) else { return };
        let media = service::Item::Media { id: item.id, name: item.name.clone() };
        let Some(running) = self.running.as_mut() else { return };
        let services = &mut running.home_state.services;
        match after {
            Some(after) if index == services.current => {
                services.insert(after + 1, media);
                *to = Some((index, Some(after + 1)));
            }
            _ => {
                services.add_to(index, media);
            }
        }
        if let Some(playlist) = services.list.get(index) {
            running.home_state.notice = Some(format!("Added {} to the playlist {}.", item.name, playlist.name));
        }
    }

    /// Any other file dropped on the window: a Bible is installed; anything else is offered to
    /// the media bin, which says why it can't take it.
    fn add_dropped_file(&mut self, path: &std::path::Path) {
        if path.extension().is_some_and(|ext| ext == "spbible" || ext == "pcbible") {
            let Some(running) = self.running.as_mut() else {
                return;
            };
            match bible_store::install_file(path) {
                Ok((kept, info)) => {
                    running.home_state.notice = Some(format!("Installed the {}.", info.name));
                    running.home_state.bible.shelf.add_file(kept, info);
                }
                Err(message) => running.home_state.notice = Some(message),
            }
            running.home_window.request_redraw();
            return;
        }
        let _ = self.add_media_file(path);
    }

    /// Puts a picture or video in the media bin (and in the media playlist shown, if any).
    /// Ok(true) when it was added, Ok(false) when it was in the bin already; the error says
    /// why it couldn't be.
    fn add_media_file(&mut self, path: &std::path::Path) -> Result<bool, String> {
        // A file already in the bin isn't added twice; it joins the playlist shown, if any. One a
        // followed folder brought in counts from now on as added by hand, so it stays in the bin
        // when it leaves the folder.
        let already = self.show.media().iter().find(|item| item.path == path).map(|item| item.id);
        if let Some(id) = already {
            self.followed_items.remove(&id);
        }
        let already = already.is_some();
        let started = Instant::now();
        // Only the small copy is kept; the full-size picture is read when it goes on screen.
        let result = if already { Ok(None) } else { media::load_small(path).map(Some) };
        let Some(running) = self.running.as_mut() else {
            return Err("The window isn't open.".to_owned());
        };
        running.home_state.drop_hover = false;
        let outcome = match &result {
            Ok(loaded) => Ok(loaded.is_some()),
            Err(problem) => Err(problem.clone()),
        };
        match result {
            Ok(loaded) => {
                if let Some((picture, whole)) = loaded {
                    let name =
                        path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
                    let item = MediaItem::new(name, path.to_owned());
                    self.bin_pictures.set_small(item.id, picture, whole);
                    self.show.add_media(item);
                    running.home_state.notice = None;
                    applog::event("media", &format!("added {} in {:?}", path.display(), started.elapsed()));
                } else {
                    running.home_state.notice = Some("That file is already in the media bin.".to_owned());
                }
                // Added while a media playlist is shown, it goes into that playlist, so it shows.
                let state = &mut running.home_state;
                if let Some(playlist) =
                    state.media_playlist_shown.and_then(|id| state.media_playlists.iter_mut().find(|p| p.id == id))
                    && !playlist.has(path)
                {
                    playlist.toggle(path);
                }
                self.save_media_list();
            }
            Err(message) => {
                applog::event("media", &format!("could not add {}: {message}", path.display()));
                running.home_state.notice = Some(message);
            }
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
        outcome
    }

    /// Takes in the media bin pictures read on their threads since the last look. The item on
    /// screen goes to the screens again once its full-size picture (or a new small copy) is
    /// there.
    fn take_media_read(&mut self) {
        use bin_pictures::Arrived;
        let arrived = self.bin_pictures.take_arrived();
        if arrived.is_empty() {
            return;
        }
        let live = self.show.live_media().and_then(|index| self.show.media().get(index)).map(|item| item.id);
        let (mut refresh, mut preview, mut added) = (false, false, false);
        for arrived in arrived {
            // A file new to the bin from a followed folder goes in once it has been read; one
            // that can't be read is noted, and tried again only once it changes.
            let from_folder = match &arrived {
                Arrived::Small(id) | Arrived::Refused(id, _) => self.folder_notes.coming.remove(id).map(|f| (*id, f)),
                Arrived::Full(_) => None,
            };
            if let Some((id, (path, changed))) = from_folder {
                if let Arrived::Refused(_, problem) = &arrived {
                    applog::event("media", problem);
                    self.bin_pictures.forget(id);
                    self.folder_notes.refused.insert(path, changed);
                } else {
                    added |= self.add_from_folder(id, path, changed);
                }
                continue;
            }
            match arrived {
                Arrived::Full(id) => {
                    refresh |= live == Some(id);
                    if let Some(took) = self.bin_pictures.stood_in(id) {
                        applog::event("media", &format!("full-size picture on screen {took:?} after it was asked for"));
                    }
                }
                Arrived::Small(id) => {
                    // The screens take a new small copy only while it stands in for the full one.
                    if live == Some(id) {
                        refresh |= !self.bin_pictures.screens_have_full();
                        preview = true;
                    }
                    self.forget_thumbs_showing(id);
                }
                Arrived::Refused(id, problem) => {
                    applog::event("media", &problem);
                    // An item whose picture was never read (its file is gone, or isn't a picture)
                    // leaves the bin, as such a file was never let in; one read before keeps the
                    // copy it has.
                    if self.bin_pictures.small(id).is_none()
                        && let Some(index) = self.show.media().iter().position(|item| item.id == id)
                    {
                        eprintln!("{problem}");
                        // A media playlist that was to start with it starts with the next instead.
                        let playing =
                            self.media_cycle.as_ref().filter(|cycle| cycle.shown == index).map(|c| c.playlist);
                        refresh |= self.remove_bin_item(index);
                        if let Some(playlist) = playing {
                            self.play_media_playlist(playlist);
                        }
                    }
                }
            }
        }
        if let Some((started, count)) = self.bin_started
            && !self.bin_pictures.any_coming()
        {
            applog::event("media", &format!("read the bin's {count} pictures in {:?}", started.elapsed()));
            self.bin_started = None;
        }
        if added {
            self.save_media_list();
        }
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        if refresh {
            self.refresh_output();
        } else if preview {
            self.refresh_preview(false);
        }
    }

    /// Lets go of the thumbnails that show a media item (its own, and those of slides with it
    /// behind their words), so they are drawn again with its picture as it is now.
    fn forget_thumbs_showing(&mut self, id: Uuid) {
        let Some(running) = self.running.as_mut() else { return };
        running.thumbs.forget(id, &mut running.egui_renderer);
        for song in self.show.presentations() {
            for (_, slide) in song.slides() {
                if home::media_of(slide) == Some(id) {
                    running.thumbs.forget(slide.id, &mut running.egui_renderer);
                }
            }
        }
    }

    /// Takes the item at `index` out of the media bin, with its pictures, and moves what names
    /// bin items by their place (the media playlist playing, the items picked out) along with
    /// it. True when that took media off the screens.
    fn remove_bin_item(&mut self, index: usize) -> bool {
        let Some(id) = self.show.media().get(index).map(|item| item.id) else { return false };
        let changed = self.show.remove_media(index).media;
        self.bin_pictures.forget(id);
        self.followed_items.remove(&id);
        if let Some(cycle) = self.media_cycle.as_mut()
            && let Some(at) = media::place_after_removal(cycle.shown, index)
        {
            cycle.shown = at;
        }
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            state.picked_media =
                state.picked_media.iter().filter_map(|at| media::place_after_removal(*at, index)).collect();
            state.media_anchor = state.media_anchor.and_then(|at| media::place_after_removal(at, index));
            running.thumbs.forget(id, &mut running.egui_renderer);
        }
        changed
    }

    /// Remembers what is in the media bin, how each item is shown, and the media playlists.
    fn save_media_list(&mut self) {
        let entries = self
            .show
            .media()
            .iter()
            .map(|item| media::Entry {
                id: item.id,
                path: item.path.clone(),
                fit: item.fit,
                looping: item.looping,
                followed: self.followed_items.contains(&item.id),
            })
            .collect();
        let playlists = self.running.as_ref().map(|r| r.home_state.media_playlists.clone()).unwrap_or_default();
        if let Err(problem) = media::save_list(&media::List { entries, playlists })
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notice = Some(problem);
        }
    }

    /// Keeps a song as it is now, so the change about to be made to it can be undone.
    fn remember_for_undo(&mut self, index: usize) {
        if let Some(song) = self.show.presentations().get(index).cloned() {
            self.history.remember(index, song);
        }
    }

    /// Undoes the last change made to a song in the Show view, or redoes the last one undone.
    /// Keeps the Edit view's undo order and the song history in step: drops what can no longer
    /// be undone safely, notes when the editor opens and closes, and undoes or redoes a change of
    /// the song's slides asked for from the editor, the slide being edited following it.
    fn edit_undo_bookkeeping(&mut self) {
        // A song just opened in the editor is read again first if its file changed outside the
        // app (the editor then starts from the file's version).
        let opened = self.running.as_ref().and_then(|r| {
            let state = &r.home_state;
            state.editing.as_ref().filter(|_| state.theme_edit.is_none()).map(|draft| draft.at.presentation)
        });
        let own_slot = |index: usize| index == props::PROP_SLOT || index == stage::STAGE_SLOT;
        if let Some(index) = opened.filter(|index| Some(*index) != self.edit_song && !own_slot(*index)) {
            self.freshen(index, true);
        }
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        if let Some(index) = state.forget_song_history.take() {
            self.history.forget(index);
        }
        if std::mem::take(&mut state.clear_song_redo) {
            self.history.clear_redo();
        }
        // Items taken out of a playlist take their place in the Show view's undo order.
        for removed in std::mem::take(&mut state.removed_items) {
            self.history.remember_items(removed);
        }
        let editing = state
            .editing
            .as_ref()
            .filter(|_| state.theme_edit.is_none())
            .map(|draft| draft.at.presentation)
            .filter(|at| !own_slot(*at));
        if editing != self.edit_song {
            // The editor closed, or moved to another song: the slide's own steps went; the
            // song's go too if undoing them would lose a later change of the slide.
            if let Some(old) = self.edit_song
                && home::drop_slide_steps(&mut state.edit_steps)
            {
                self.history.forget(old);
            }
            state.edit_steps.clear();
            state.edit_redo.clear();
            state.edit_steps_draft = 0;
            self.edit_mark = self.history.mark();
            self.edit_song = editing;
        }
        let Some(redo) = state.song_undo.take() else { return };
        let followed = state.editing.as_ref().map(|draft| (draft.at, draft.slide.id));
        self.undo_show(redo);
        // The slide being edited is found again where the change left it.
        let Some((at, id)) = followed else { return };
        let Some(running) = self.running.as_mut() else { return };
        let song = self.show.presentations().get(at.presentation);
        let found = song.and_then(|song| song.slides().position(|(_, slide)| slide.id == id));
        match found {
            Some(slide) => {
                if let Some(draft) = running.home_state.editing.as_mut() {
                    draft.at.slide = slide;
                }
            }
            None => {
                let count = song.map_or(0, |song| song.slide_count());
                let slide = at.slide.min(count.saturating_sub(1));
                running.home_state.editing = EditDraft::of(&self.show, sp_engine::SlideRef { slide, ..at });
            }
        }
    }

    /// A change of a song's slides made from the Edit view goes in its undo order.
    fn note_song_step(&mut self) {
        if let Some(running) = self.running.as_mut()
            && running.home_state.editing.is_some()
        {
            running.home_state.edit_steps.push(home::EditStep::Song);
            running.home_state.edit_redo.clear();
        }
    }

    fn undo_show(&mut self, redo: bool) {
        // Items taken out of a playlist, when they were the last change: put back (or taken
        // out again).
        if self.history.items_next(redo) {
            if let Some(said) = self.undo_items(redo)
                && let Some(running) = self.running.as_mut()
            {
                running.home_state.say(said);
                running.home_window.request_redraw();
            }
            return;
        }
        let Some(current) = self.history.peek(redo).and_then(|index| self.show.presentations().get(index).cloned())
        else {
            return;
        };
        let Some((index, song)) = self.history.step(redo, current) else { return };
        let changes = self.show.replace_presentation_following(index, song);
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
        if changes.slides {
            self.refresh_output();
        }
    }

    /// Puts back the items last taken out of a playlist (or, to redo, takes them out again).
    /// Returns what to tell the operator: what was done, or why it couldn't be.
    fn undo_items(&mut self, redo: bool) -> Option<String> {
        let removed = self.history.step_items(redo)?;
        let state = &mut self.running.as_mut()?.home_state;
        Some(
            studio::redo_removed(state, &removed, !redo)
                .unwrap_or_else(|| format!("{} has changed since, so that can't be undone.", removed.name)),
        )
    }

    /// Turns a song's looping on or off, from its menu.
    /// Gives a song a transition of its own, or takes it away.
    fn handle_song_transition(&mut self) {
        let Some((index, transition)) = self.running.as_mut().and_then(|r| r.home_state.song_transition_request.take())
        else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        if song.transition == transition {
            return;
        }
        // Dragging the duration or turning the direction of the song's own transition is one
        // step to undo, not one for each frame of the drag.
        let same_kind = matches!((song.transition, transition), (Some(was), Some(now)) if was.kind == now.kind);
        if !(same_kind && self.history.peek(false) == Some(index)) {
            self.history.remember(index, song.clone());
        }
        let changed = sp_model::Presentation { transition, ..(*song).clone() };
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        // Written once the changes pause, not on every frame of a drag.
        self.unsaved_since = Some(Instant::now());
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    fn handle_song_loop(&mut self) {
        let Some(index) = self.running.as_mut().and_then(|r| r.home_state.song_loop_request.take()) else { return };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = sp_model::Presentation { loops: !song.loops, ..(*song).clone() };
        self.history.remember(index, song);
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if self.show.live().is_some_and(|live| live.presentation == index) {
            self.refresh_output();
        }
    }

    /// Keeps a song's arrangements as changed in the Show view, and the one it is played in.
    fn handle_arrangement(&mut self) {
        let Some((index, arrangements, chosen)) =
            self.running.as_mut().and_then(|r| r.home_state.arrangement_request.take())
        else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = sp_model::Presentation { arrangements, arrangement: chosen, ..(*song).clone() };
        if changed == *song {
            return;
        }
        // Typing an arrangement's name is one step to undo, not one for each letter.
        let only_named = |a: &sp_model::Presentation, b: &sp_model::Presentation| {
            a.arrangement == b.arrangement
                && a.arrangements.len() == b.arrangements.len()
                && a.arrangements.iter().zip(&b.arrangements).all(|(x, y)| x.id == y.id && x.groups == y.groups)
        };
        let renaming = only_named(&changed, &song) && self.history.peek(false) == Some(index);
        if !renaming {
            self.history.remember(index, song);
        }
        let changes = self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        // An arrangement's name being typed is written once typing pauses; anything else at once.
        if renaming {
            self.unsaved_since = Some(Instant::now());
        } else {
            self.save_unsaved();
        }
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        // The stage screens show the next slide in the new order.
        if changes.slides {
            self.refresh_output();
        }
    }

    /// Gives a song a theme of its own, or puts it back on the songs theme.
    fn handle_song_theme(&mut self) {
        let Some((index, theme)) = self.running.as_mut().and_then(|r| r.home_state.song_theme_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let id = theme.and_then(|at| self.themes.get(at)).map(|theme| theme.id);
        if song.theme == id {
            return;
        }
        let changed = sp_model::Presentation { theme: id, ..(*song).clone() };
        self.remember_for_undo(index);
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            running.home_window.request_redraw();
        }
        if self.show.live().is_some_and(|live| live.presentation == index) {
            self.refresh_output();
        }
    }

    /// Gives a song the copyright details typed in its Info pop-over.
    fn handle_copyright(&mut self) {
        let Some((index, details)) = self.running.as_mut().and_then(|r| r.home_state.copyright_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = sp_model::Presentation { copyright: details, ..(*song).clone() };
        self.history.remember(index, song.clone());
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.unsaved_since = Some(Instant::now());
    }

    /// Rebuilds a song from the words finished in the Reflow editor.
    fn handle_reflow(&mut self) {
        let Some((index, text)) = self.running.as_mut().and_then(|r| r.home_state.reflow_done.take()) else { return };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let rebuilt = song.reflowed(&text);
        if rebuilt == *song {
            return;
        }
        self.history.remember(index, song.clone());
        let changes = self.show.replace_presentation(index, Arc::new(rebuilt));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if changes.slides {
            self.refresh_output();
        }
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
    }

    /// Carries out a change to the media bin: choosing pictures to add, or removing one.
    /// Asks for a picture to put on the slide being edited (or behind the theme being edited),
    /// and copies it into the Images folder.
    fn handle_picture_request(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        if std::mem::take(&mut running.home_state.theme_picture_request) {
            let chosen = rfd::FileDialog::new()
                .set_title("Choose a Picture for the Theme")
                .add_filter("Pictures", &media::PICTURES)
                .pick_file();
            let state = &mut running.home_state;
            match chosen.map(|path| images::import(&path)) {
                Some(Ok(picture)) => {
                    if let Some(edit) = state.theme_edit.as_mut() {
                        edit.theme.background_picture = Some(picture.file);
                        edit.changed = true;
                    }
                }
                Some(Err(problem)) => state.notice = Some(problem),
                None => {}
            }
            running.home_window.request_redraw();
            return;
        }
        if !std::mem::take(&mut running.home_state.picture_request) {
            return;
        }
        let chosen = rfd::FileDialog::new()
            .set_title("Choose a Picture or Video")
            .add_filter("Pictures and Videos", &media::PICTURE_ENDINGS)
            .pick_file();
        let state = &mut running.home_state;
        match chosen.map(|path| images::import(&path)) {
            Some(Ok(picture)) => state.picture_to_add.push(picture),
            Some(Err(problem)) => state.notice = Some(problem),
            None => {}
        }
        running.home_window.request_redraw();
    }

    fn handle_media_request(&mut self) {
        use home::MediaRequest;
        if self.running.as_mut().is_some_and(|r| std::mem::take(&mut r.home_state.media_playlists_changed)) {
            self.save_media_list();
        }
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.media_request.take()) else { return };
        match request {
            MediaRequest::PlayPlaylist(Some(id)) => self.play_media_playlist(id),
            MediaRequest::PlayPlaylist(None) => self.stop_media_playlist(),
            MediaRequest::WatchFolder(id, follow) => {
                let folder = if follow {
                    let Some(folder) = rfd::FileDialog::new().set_title("Choose a Folder to Follow").pick_folder()
                    else {
                        return;
                    };
                    Some(folder)
                } else {
                    None
                };
                if let Some(running) = self.running.as_mut()
                    && let Some(playlist) = running.home_state.media_playlists.iter_mut().find(|p| p.id == id)
                {
                    applog::event("media", &format!("playlist {} follows {:?}", playlist.name, folder));
                    playlist.folder = folder;
                }
                self.folder_notes.seen.clear();
                self.check_folders(true);
                self.save_media_list();
            }
            MediaRequest::Choose => {
                // The system's own window for choosing files, as any Mac app has.
                let chosen = rfd::FileDialog::new()
                    .set_title("Add Pictures and Videos")
                    .add_filter("Pictures and Videos", &media::PICTURE_ENDINGS)
                    .pick_files()
                    .unwrap_or_default();
                for path in chosen {
                    self.add_dropped_file(&path);
                }
            }
            MediaRequest::AddAsSlide(index) => {
                let Some(item) = self.show.media().get(index).cloned() else { return };
                let song = self.show.selected();
                let Some(count) = self.show.presentations().get(song).map(|song| song.slide_count()) else { return };
                if count == 0 || self.app_made(song) {
                    return;
                }
                // After the slide picked out in the song shown, or at its end.
                let after = self.show.cursor().filter(|at| at.presentation == song).map_or(count - 1, |at| at.slide);
                let name = std::path::Path::new(&item.name)
                    .file_stem()
                    .map_or(item.name.clone(), |stem| stem.to_string_lossy().into_owned());
                let slide =
                    Slide { label: Some(name), ..home::slide_with_media(&Slide::new(Vec::new()), Some(item.id)) };
                if let Some(running) = self.running.as_mut() {
                    running.home_state.slide_request = Some((
                        sp_engine::SlideRef { presentation: song, slide: after },
                        home::SlideRequest::PasteAfter(vec![slide]),
                    ));
                    running.home_window.request_redraw();
                }
                applog::event("media", &format!("added {} to a song as a slide", item.path.display()));
            }
            MediaRequest::Look(index, fit, looping) => {
                let changes = self.show.set_media_look(index, fit, looping);
                if changes.media {
                    self.refresh_output();
                }
                self.save_media_list();
                if let Some(running) = self.running.as_mut() {
                    running.home_window.request_redraw();
                }
            }
            MediaRequest::Delete(mut items) => {
                // From the last to the first, so each place still names the same item.
                items.sort_unstable();
                items.dedup();
                let mut media_changed = false;
                for index in items.into_iter().rev() {
                    if let Some(item) = self.show.media().get(index) {
                        applog::event("media", &format!("deleted {} from the bin", item.path.display()));
                    }
                    media_changed |= self.remove_bin_item(index);
                }
                if media_changed {
                    self.refresh_output();
                }
                self.save_media_list();
                if let Some(running) = self.running.as_mut() {
                    running.home_window.request_redraw();
                }
            }
        }
    }

    /// Puts a slide changed in the Edit view into its song. The song's file is written a
    /// moment later (see `save_unsaved`). The output is left alone unless Live Edit is on, so
    /// editing never changes what the audience sees by surprise.
    fn apply_edit(&mut self, at: sp_engine::SlideRef, slide: Slide) {
        let Some(song) = self.show.presentations().get(at.presentation).cloned() else {
            return;
        };
        let slide_id = slide.id;
        let Some(edited) = song.with_slide(at.slide, slide) else {
            return;
        };
        // A change made in the Show view can be undone there. The Edit view keeps its own steps;
        // the Show view's steps for the song from before it opened are dropped, so they can't
        // undo its edits. Its own changes of the song's slides (moves, pastes) are kept.
        if self.running.as_ref().is_some_and(|r| r.home_state.editing.is_none()) {
            self.remember_for_undo(at.presentation);
        } else {
            self.history.forget_before(at.presentation, self.edit_mark);
        }
        let changes = self.show.replace_presentation(at.presentation, Arc::new(edited));
        if !self.unsaved.contains(&at.presentation) {
            self.unsaved.push(at.presentation);
        }
        self.unsaved_since = Some(Instant::now());
        let mut live_edit = false;
        if let Some(running) = self.running.as_mut() {
            running.thumbs.forget(slide_id, &mut running.egui_renderer);
            live_edit = running.home_state.live_edit;
            running.home_window.request_redraw();
        }
        if changes.slides && live_edit {
            self.refresh_output();
        }
        // The song is written once the changes pause for a second (not on every letter typed
        // or every frame of a drag, each a whole file written on this thread), on leaving the
        // Edit view, and on quitting.
    }

    /// Writes every song changed in the Edit view to its file. The bundled sample has no file.
    fn save_unsaved(&mut self) {
        let mut problem = None;
        for index in std::mem::take(&mut self.unsaved) {
            // A song with no file yet (the sample) is given one in the app's Songs folder, so
            // a change to it is kept like any other. Bible passages are never saved as songs.
            if !self.app_made(index)
                && let (Some(None), Some(song)) = (self.sources.get(index), self.show.presentations().get(index))
                && let Some(folder) = bible_store::data_dir().map(|dir| dir.join("Songs"))
            {
                let _ = std::fs::create_dir_all(&folder);
                self.sources[index] = Some(folder.join(format!("{}.{}", song.id, load::SONG_EXTENSION)));
            }
            let (Some(Some(path)), Some(song)) = (self.sources.get(index), self.show.presentations().get(index)) else {
                continue;
            };
            // A file changed outside the app since it was read is never saved over: the song
            // waits on Reload or Keep mine, holding its changes.
            let target = load::song_file_for(path);
            if self.stamps.check(&target) != outside::Now::Same {
                applog::event("outside", &format!("did not save over {}: it changed outside", file_name(&target)));
                if !self.held_songs.contains(&index) {
                    self.held_songs.push(index);
                }
                self.wait_on(outside::Waiting::Song(index));
                continue;
            }
            match load::save_song(path, song) {
                // From now on the song lives in the file that was just written.
                Ok(written) => {
                    self.stamps.note(&written);
                    self.sources[index] = Some(written);
                }
                Err(e) => problem = Some(format!("The change was not saved. {e}")),
            }
        }
        for index in std::mem::take(&mut self.themes_unsaved) {
            let Some(theme) = self.themes.get(index) else { continue };
            let file = theme_store::file_of(theme);
            if file.as_ref().is_some_and(|file| self.stamps.check(file) != outside::Now::Same) {
                applog::event("outside", &format!("did not save over the theme {}: it changed outside", theme.name));
                if !self.held_themes.contains(&index) {
                    self.held_themes.push(index);
                }
                self.wait_on(outside::Waiting::Theme(index));
                continue;
            }
            match theme_store::save(theme) {
                Ok(()) => {
                    if let Some(file) = file {
                        self.stamps.note(&file);
                    }
                }
                Err(e) => problem = Some(e),
            }
        }
        self.unsaved_since = None;
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = problem;
            running.home_window.request_redraw();
        }
    }

    /// Puts a song or theme in the bar that offers Reload and Keep mine.
    fn wait_on(&mut self, what: outside::Waiting) {
        if let Some(running) = self.running.as_mut()
            && !running.home_state.waiting.contains(&what)
        {
            running.home_state.waiting.push(what);
            running.home_window.request_redraw();
        }
    }

    fn stop_waiting(&mut self, what: outside::Waiting) {
        if let Some(running) = self.running.as_mut() {
            running.home_state.waiting.retain(|have| *have != what);
            running.home_window.request_redraw();
        }
    }

    /// Whether a song is in use where it can't be swapped for its file's version unasked: on
    /// screen, on the Announcements layer, or (unless `editor_too` is false) open in the editor.
    fn song_in_use(&self, index: usize, editor_too: bool) -> bool {
        let live = self.show.live().is_some_and(|at| at.presentation == index);
        let announcing = self.announcing.as_ref().is_some_and(|on| on.song() == Some(index));
        let editing = editor_too
            && self.running.as_ref().is_some_and(|r| {
                r.home_state.theme_edit.is_none()
                    && r.home_state.editing.as_ref().is_some_and(|draft| draft.at.presentation == index)
            });
        live || announcing || editing
    }

    /// Before a song is opened or used: if its file changed outside the app, reads it again,
    /// or, while the song is in use or holds changes not saved, offers Reload instead. With
    /// `opening_editor`, the song being opened in the editor doesn't count as in use there.
    /// True when the song was read again.
    fn freshen(&mut self, index: usize, opening_editor: bool) -> bool {
        let removed = self.running.as_ref().is_some_and(|r| r.home_state.removed_songs.contains(&index));
        if self.app_made(index) || removed {
            return false;
        }
        let Some(Some(path)) = self.sources.get(index) else { return false };
        let now = self.stamps.check(&load::song_file_for(path));
        if now == outside::Now::Same {
            return false;
        }
        let waiting = outside::Waiting::Song(index);
        if now == outside::Now::Gone
            || self.song_in_use(index, !opening_editor)
            || self.unsaved.contains(&index)
            || self.held_songs.contains(&index)
        {
            self.wait_on(waiting);
            return false;
        }
        match self.reload_song(index) {
            Ok(title) => {
                applog::event("outside", &format!("read {title} again: its file changed outside SoftPresenter"));
                true
            }
            Err(problem) => {
                self.wait_on(waiting);
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notice = Some(problem);
                }
                false
            }
        }
    }

    /// Takes a song's file's version in place of the app's: what was not saved goes, as does
    /// its undo history, and the editor on it starts again from the file. Returns its title.
    fn reload_song(&mut self, index: usize) -> Result<String, String> {
        let title = self.show.presentations().get(index).map(|song| song.title.clone()).unwrap_or_default();
        let path = self.sources.get(index).cloned().flatten().ok_or_else(|| format!("{title} has no file to read."))?;
        let target = load::song_file_for(&path);
        if !target.is_file() {
            return Err(format!("{title}'s file is gone; Keep mine saves it again."));
        }
        let (mut loaded, problems) = load::load_songs(std::slice::from_ref(&target));
        let song = loaded.pop().map(|loaded| loaded.song).ok_or_else(|| {
            let why = problems.join(" ").replace(&target.display().to_string(), &file_name(&target));
            format!("{title} could not be read again ({why}); SoftPresenter's version is kept.")
        })?;
        let key = song_ids::key_for(Some(&target), song.id, &song.title);
        let title = song.title.clone();
        let changes = self.show.replace_presentation_following(index, Arc::new(song));
        self.sources[index] = Some(target.clone());
        self.stamps.note(&target);
        self.history.forget(index);
        self.unsaved.retain(|at| *at != index);
        self.held_songs.retain(|at| *at != index);
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            let state = &mut running.home_state;
            if let Some(have) = state.song_keys.get_mut(index) {
                *have = key;
            }
            state.waiting.retain(|have| *have != outside::Waiting::Song(index));
            // The editor on the song starts again from the file's version.
            let open = state.editing.as_ref().map(|draft| draft.at).filter(|at| at.presentation == index);
            if let Some(open) = open {
                let count = self.show.presentations().get(index).map_or(0, |song| song.slide_count());
                let at = sp_engine::SlideRef { slide: open.slide.min(count.saturating_sub(1)), ..open };
                state.editing = EditDraft::of(&self.show, at);
                state.edit_steps.clear();
                state.edit_redo.clear();
            }
            running.home_window.request_redraw();
        }
        if changes.slides {
            self.refresh_output();
        }
        Ok(title)
    }

    /// Writes the app's version of a song over its file, which changed outside the app.
    fn keep_song(&mut self, index: usize) -> Result<String, String> {
        if self.running.as_ref().is_some_and(|r| r.home_state.removed_songs.contains(&index)) {
            return Err("That song was deleted.".to_owned());
        }
        let (Some(Some(path)), Some(song)) = (self.sources.get(index), self.show.presentations().get(index)) else {
            return Err("That song has no file.".to_owned());
        };
        let title = song.title.clone();
        let written = load::save_song(path, song)?;
        self.stamps.note(&written);
        self.sources[index] = Some(written);
        self.held_songs.retain(|at| *at != index);
        self.unsaved.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Song(index));
        Ok(title)
    }

    /// Takes a theme's file's version in place of the app's.
    fn reload_theme(&mut self, index: usize) -> Result<String, String> {
        if self.running.as_ref().is_some_and(|r| r.home_state.theme_edit.is_some()) {
            return Err("Close the Theme editor first, then reload the theme.".to_owned());
        }
        let theme = self.themes.get(index).ok_or("That theme is gone.")?;
        let file = theme_store::file_of(theme).ok_or("The theme has no file.")?;
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("The theme {} could not be read: {e}", theme.name))?;
        let read = sp_model::Theme::from_json(&text)
            .map_err(|e| format!("The theme {} could not be read: {e}", theme.name))?;
        if read.id != theme.id {
            return Err(format!("The file of the theme {} now holds another theme.", theme.name));
        }
        let name = read.name.clone();
        self.themes[index] = read;
        self.stamps.note(&file);
        self.held_themes.retain(|at| *at != index);
        self.themes_unsaved.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Theme(index));
        self.themes_changed();
        self.refresh_output();
        Ok(name)
    }

    /// Writes the app's version of a theme over its file, which changed outside the app.
    fn keep_theme(&mut self, index: usize) -> Result<String, String> {
        let theme = self.themes.get(index).ok_or("That theme is gone.")?;
        theme_store::save(theme)?;
        let name = theme.name.clone();
        if let Some(file) = theme_store::file_of(theme) {
            self.stamps.note(&file);
        }
        self.held_themes.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Theme(index));
        Ok(name)
    }

    /// Carries out the choice made in the bar that offers Reload and Keep mine.
    fn choose_outside(&mut self, what: outside::Waiting, reload: bool) {
        let result = match (what, reload) {
            (outside::Waiting::Song(index), true) => self.reload_song(index),
            (outside::Waiting::Song(index), false) => self.keep_song(index),
            (outside::Waiting::Theme(index), true) => self.reload_theme(index),
            (outside::Waiting::Theme(index), false) => self.keep_theme(index),
        };
        let said = match result {
            Ok(name) if reload => format!("Read {name} again from its file."),
            Ok(name) => format!("Saved SoftPresenter's version of {name}."),
            Err(problem) => problem,
        };
        applog::event("outside", &said);
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = Some(said);
        }
    }

    /// On quitting: the changes held back because their files changed outside the app are
    /// written to the library's "Not saved" folder (named by title and time), so neither
    /// version is lost, and the log says where.
    fn keep_held_aside(&mut self) {
        let Some(folder) = bible_store::data_dir().map(|dir| dir.join("Not saved")) else { return };
        let when = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
        let plain = pack::safe_file_name;
        for index in self.held_songs.clone() {
            let Some(song) = self.show.presentations().get(index) else { continue };
            let file = folder.join(format!("{} {when}.{}", plain(&song.title), load::SONG_EXTENSION));
            let kept = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&file, sp_model::to_json(song)));
            match kept {
                Ok(()) => applog::problem(
                    "outside",
                    &format!(
                        "{}: its file changed outside SoftPresenter; SoftPresenter's version is kept in Not saved/{}",
                        song.title,
                        file_name(&file)
                    ),
                ),
                Err(e) => applog::problem("outside", &format!("{}: changes not saved ({e})", song.title)),
            }
        }
        for index in self.held_themes.clone() {
            let Some(theme) = self.themes.get(index) else { continue };
            let file = folder.join(format!("{} {when}.sptheme", plain(&theme.name)));
            let kept = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&file, theme.to_json()));
            match kept {
                Ok(()) => applog::problem(
                    "outside",
                    &format!(
                        "the theme {}: its file changed outside SoftPresenter; SoftPresenter's version is kept in Not saved/{}",
                        theme.name,
                        file_name(&file)
                    ),
                ),
                Err(e) => applog::problem("outside", &format!("the theme {}: changes not saved ({e})", theme.name)),
            }
        }
    }

    /// Reads again every song (and, with the Theme editor closed, every theme) whose file
    /// changed outside the app, and adds songs put in the Songs folder since. A song in use or
    /// holding changes not saved is offered in the bar instead.
    fn reload_all(&mut self) -> Reloaded {
        let mut done = Reloaded::default();
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        for index in 0..self.show.presentations().len() {
            // A song deleted in the app, or a Bible page, is not looked at.
            if removed.contains(&index) || self.app_made(index) {
                continue;
            }
            let Some(Some(path)) = self.sources.get(index) else { continue };
            let title = self.show.presentations()[index].title.clone();
            let before = self.stamps.check(&load::song_file_for(path));
            if before == outside::Now::Same {
                continue;
            }
            if self.freshen(index, false) {
                done.read.push(title);
            } else if before == outside::Now::Gone {
                done.gone.push(title);
            } else if self
                .running
                .as_ref()
                .is_some_and(|r| r.home_state.waiting.contains(&outside::Waiting::Song(index)))
            {
                done.waiting.push(title);
            }
        }
        // Song files put in the Songs folder while the app runs.
        let known: std::collections::HashSet<PathBuf> =
            self.sources.iter().flatten().map(|path| load::song_file_for(path)).collect();
        let folder = bible_store::data_dir().map(|dir| dir.join("Songs"));
        let mut new_files: Vec<PathBuf> = folder
            .and_then(|folder| std::fs::read_dir(folder).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| load::is_song_file(path) && !known.contains(path))
            .collect();
        new_files.sort();
        for path in new_files {
            let (mut loaded, problems) = load::load_songs(std::slice::from_ref(&path));
            let Some(mut song) = loaded.pop().map(|loaded| loaded.song) else {
                let shown = path.display().to_string();
                done.problems.extend(problems.into_iter().map(|problem| problem.replace(&shown, &file_name(&path))));
                continue;
            };
            // A copy of a song already here gets an id of its own, written back to its file.
            if self.show.presentations().iter().any(|have| have.id == song.id) {
                song.id = Uuid::new_v4();
                if let Err(e) = load::save_song(&path, &song) {
                    done.problems.push(e);
                    continue;
                }
            }
            self.stamps.note(&path);
            let key = song_ids::key_for(Some(&path), song.id, &song.title);
            done.added.push(song.title.clone());
            self.show.add_presentation(Arc::new(song));
            self.sources.push(Some(path));
            if let Some(running) = self.running.as_mut() {
                running.home_state.song_keys.push(key);
            }
        }
        // Themes, with the Theme editor closed.
        if self.running.as_ref().is_some_and(|r| r.home_state.theme_edit.is_none()) {
            for index in 0..self.themes.len() {
                let Some(file) = theme_store::file_of(&self.themes[index]) else { continue };
                match self.stamps.check(&file) {
                    outside::Now::Same => {}
                    outside::Now::Changed
                        if !self.held_themes.contains(&index) && !self.themes_unsaved.contains(&index) =>
                    {
                        match self.reload_theme(index) {
                            Ok(name) => done.read.push(format!("the theme {name}")),
                            Err(problem) => done.problems.push(problem),
                        }
                    }
                    _ => {
                        done.waiting.push(format!("the theme {}", self.themes[index].name));
                        self.wait_on(outside::Waiting::Theme(index));
                    }
                }
            }
        }
        for line in done
            .read
            .iter()
            .map(|name| format!("read {name} again"))
            .chain(done.added.iter().map(|name| format!("added {name}")))
        {
            applog::event("outside", &line);
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
        done
    }

    /// The id of the song a key names (a "go to song" action's, or a playlist item's), if the
    /// app has it.
    fn song_id_for_key(&self, key: &str) -> Option<Uuid> {
        let running = self.running.as_ref()?;
        studio::playlist_song(&self.show, &running.home_state, key)
            .and_then(|index| self.show.presentations().get(index))
            .map(|song| song.id)
    }

    /// Plans a song pack of the songs at `indices`, or of a playlist's songs and the playlist.
    fn plan_pack(&self, indices: &[usize], playlist: Option<usize>) -> Result<pack::Export, String> {
        let running = self.running.as_ref().ok_or("The app isn't ready.")?;
        let state = &running.home_state;
        let mut indices: Vec<usize> = indices.to_vec();
        let playlist_file = match playlist {
            Some(at) => {
                let list = state.services.list.get(at).ok_or("No such playlist.")?;
                for item in &list.items {
                    if let service::Item::Song { key, .. } = item
                        && let Some(index) = studio::playlist_song(&self.show, state, key)
                        && !indices.contains(&index)
                    {
                        indices.push(index);
                    }
                }
                Some(pack::PlaylistFile { name: list.name.clone(), items: list.items.clone() })
            }
            None => None,
        };
        let songs: Vec<sp_model::Presentation> = indices
            .iter()
            .filter(|index| !self.app_made(**index) && !state.removed_songs.contains(index))
            .filter_map(|index| self.show.presentations().get(*index).map(|song| (**song).clone()))
            .collect();
        if songs.is_empty() && playlist_file.is_none() {
            return Err("No songs to pack.".to_owned());
        }
        let media_of = |id: Uuid| {
            self.show
                .media()
                .iter()
                .find(|item| item.id == id)
                .map(|item| (item.name.clone(), item.path.clone(), item.fit, item.looping))
        };
        // A media playlist's name and the bin items in it, in its order.
        let media_playlist_of = |id: Uuid| {
            state.media_playlists.iter().find(|playlist| playlist.id == id).map(|playlist| {
                let members = playlist
                    .files
                    .iter()
                    .filter_map(|file| self.show.media().iter().find(|item| &item.path == file).map(|item| item.id))
                    .collect();
                (playlist.name.clone(), members)
            })
        };
        Ok(pack::Export::plan(
            songs,
            &self.themes,
            playlist_file,
            |key| self.song_id_for_key(key),
            &media_of,
            media_playlist_of,
        ))
    }

    /// Writes a planned pack: at once when it holds no videos, else on a thread of its own (the
    /// window says when it is done). Returns what was done, or is being done.
    fn write_pack(&mut self, export: pack::Export, to: PathBuf) -> Result<String, String> {
        if export.videos.is_empty() {
            let said = export.write(&to)?;
            applog::event("pack", &said);
            return Ok(said);
        }
        let size = pack::size_words(export.video_bytes());
        let (tx, waker) = (self.pack_tx.clone(), self.waker.clone());
        let name = to.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let job = self.start_job();
        std::thread::spawn(move || {
            let _ = tx.send(PackJob::Wrote { job, result: export.write(&to) });
            if let Some(waker) = waker {
                let _ = waker.send_event(AppEvent::PackDone);
            }
        });
        Ok(format!("Writing {name}, with its videos ({size}); the window says when it is done."))
    }

    /// Makes a song pack from the window: asks whether to put the videos in (with their size)
    /// and where to save it.
    fn export_from_window(&mut self, indices: &[usize], playlist: Option<usize>) {
        let said = match self.plan_pack(indices, playlist) {
            Err(problem) => problem,
            Ok(mut export) => {
                if !export.videos.is_empty() {
                    let size = pack::size_words(export.video_bytes());
                    let keep = rfd::MessageDialog::new()
                        .set_title("Export Songs")
                        .set_description(format!(
                            "The songs use {} ({size}). Put them in the song pack?",
                            if export.videos.len() == 1 {
                                "a video".to_owned()
                            } else {
                                format!("{} videos", export.videos.len())
                            }
                        ))
                        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                            format!("Include Videos ({size})"),
                            "Leave Them Out".to_owned(),
                            "Cancel".to_owned(),
                        ))
                        .show();
                    let include = format!("Include Videos ({size})");
                    match pack::three_way(keep, [include.as_str(), "Leave Them Out", "Cancel"]) {
                        Some(0) => {}
                        Some(1) => export.leave_out_videos(),
                        _ => return,
                    }
                }
                let name = match playlist {
                    Some(at) => {
                        self.running.as_ref().and_then(|r| r.home_state.services.list.get(at)).map(|l| l.name.clone())
                    }
                    None if export.songs.len() == 1 => export.songs.first().map(|song| song.title.clone()),
                    None => Some("Songs".to_owned()),
                };
                let file =
                    format!("{}.{}", pack::safe_file_name(&name.unwrap_or_else(|| "Songs".to_owned())), pack::ENDING);
                let Some(to) = rfd::FileDialog::new()
                    .set_title("Export Songs")
                    .set_file_name(file)
                    .add_filter("Song pack", &[pack::ENDING])
                    .save_file()
                else {
                    return;
                };
                self.write_pack(export, to).unwrap_or_else(|problem| problem)
            }
        };
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = Some(said);
            running.home_window.request_redraw();
        }
    }

    /// Opens a song pack: at once when it is small (and returns what came in), else on a thread
    /// of its own (and returns that it is under way). `same` says what to do with songs the
    /// library has already; `None` asks. Songs go into `library`, if one is named.
    fn open_pack(
        &mut self,
        path: PathBuf,
        same: Option<pack::Same>,
        library: Option<Uuid>,
        now: bool,
    ) -> Result<String, String> {
        let Some(dir) = bible_store::data_dir() else {
            return Err("Could not find the library folder.".to_owned());
        };
        let staging = dir.join(format!(".opening-pack-{}", Uuid::new_v4()));
        let small = std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= PACK_READ_AT_ONCE);
        if now || small || self.running.is_none() {
            return match pack::open(&path, &staging) {
                Ok(opened) => {
                    let said = self.apply_pack(opened, same, library);
                    applog::event("pack", said.as_ref().unwrap_or_else(|problem| problem));
                    said
                }
                Err(problem) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    applog::event("pack", &problem);
                    Err(problem)
                }
            };
        }
        let (tx, waker) = (self.pack_tx.clone(), self.waker.clone());
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let job = self.start_job();
        std::thread::spawn(move || {
            let result = pack::open(&path, &staging);
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&staging);
            }
            let _ = tx.send(PackJob::Opened { job, result: Box::new(result), same, library });
            if let Some(waker) = waker {
                let _ = waker.send_event(AppEvent::PackDone);
            }
        });
        Ok(format!("Opening {name}; the window says when its songs are in."))
    }

    /// Numbers a song pack job started in the background (the last 50 are kept).
    fn start_job(&mut self) -> u64 {
        let job = self.jobs.last().map_or(1, |(last, _)| last + 1);
        self.jobs.push((job, None));
        if self.jobs.len() > 50 {
            self.jobs.remove(0);
        }
        self.last_job = Some(job);
        job
    }

    /// Marks Bible downloads started from the command line done once their Bible is installed
    /// (or the download has stopped without it).
    fn finish_bible_jobs(&mut self) {
        if self.bible_jobs.is_empty() {
            return;
        }
        let Some(running) = self.running.as_mut() else { return };
        let page = &mut running.home_state.bible;
        let mut done = Vec::new();
        self.bible_jobs.retain(|(job, id, name)| {
            if page.shelf.position(id).is_some() {
                done.push((*job, Ok(format!("Installed the {name}."))));
                false
            } else if !page.downloads.is_busy(id) {
                let why = page.options.problem.clone().unwrap_or_else(|| "the download stopped".to_owned());
                done.push((*job, Err(format!("The {name} wasn't installed: {why}"))));
                false
            } else {
                true
            }
        });
        for (job, result) in done {
            if let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job) {
                *kept = Some(result);
            }
        }
    }

    /// Finishes the song packs written or read on threads of their own since the last look.
    fn finish_packs(&mut self) {
        while let Ok(done) = self.pack_rx.try_recv() {
            let (job, result) = match done {
                PackJob::Wrote { job, result } => (job, result),
                PackJob::Opened { job, result, same, library } => match *result {
                    Ok(opened) => (job, self.apply_pack(opened, same, library)),
                    Err(problem) => (job, Err(problem)),
                },
                PackJob::Bundle { job, result, path, to, library } => match *result {
                    Ok(opened) => {
                        let mut done = Imported::default();
                        self.take_songs(vec![SongRead::Bundle { opened, path, library }], &to, &mut done);
                        let said = self.say_imported(&done);
                        (job, if done.landed.is_empty() { Err(said) } else { Ok(said) })
                    }
                    Err(problem) => (job, Err(problem)),
                },
            };
            let said = result.clone().unwrap_or_else(|problem| problem);
            if let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job) {
                *kept = Some(result);
            }
            applog::event("pack", &said);
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some(said);
                running.home_window.request_redraw();
            }
        }
    }

    /// Brings an opened pack's songs into the library, with the themes, pictures and videos
    /// those songs need (and only those), and its playlist as a new playlist. Says what came in.
    /// Where a media bin item a pack names (by the id it had on the computer it came from) is in
    /// this bin: the same item, if this bin has it; else its file brought from the pack (or the
    /// same file here already) as an item of this bin. `None` if the pack doesn't have it.
    fn pack_media_here(
        &mut self,
        id: Uuid,
        opened: &pack::Opened,
        dir: &std::path::Path,
        brought: &mut pack::Brought,
        seen: &mut PackMedia,
    ) -> Option<Uuid> {
        if self.show.media().iter().any(|item| item.id == id) {
            return Some(id);
        }
        if let Some(known) = seen.came.get(&id) {
            return *known;
        }
        let known = opened.media_items.iter().find(|entry| entry.id == id).and_then(|entry| {
            let path = pack::bring_media_item(entry, opened, dir, brought)?;
            // The same file in the bin already: that item.
            if let Some(have) = self.show.media().iter().find(|item| item.path == path) {
                return Some(have.id);
            }
            let (picture, whole) = media::load_small(&path).ok()?;
            let mut name = entry.name.clone();
            name.retain(|c| !c.is_control());
            let mut item = MediaItem::new(name, path);
            item.fit = entry.fit;
            item.looping = entry.looping;
            let new = item.id;
            self.bin_pictures.set_small(new, picture, whole);
            self.show.add_media(item);
            seen.added += 1;
            Some(new)
        });
        seen.came.insert(id, known);
        known
    }

    fn apply_pack(
        &mut self,
        opened: pack::Opened,
        same: Option<pack::Same>,
        library: Option<Uuid>,
    ) -> Result<String, String> {
        let Some(dir) = bible_store::data_dir() else {
            return Err("Could not find the library folder.".to_owned());
        };
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        let existing = |show: &sp_engine::Show, id: Uuid, bible: &[usize; 2]| {
            show.presentations()
                .iter()
                .enumerate()
                .position(|(at, song)| song.id == id && !bible.contains(&at) && !removed.contains(&at))
        };
        let already =
            opened.songs.iter().filter(|song| existing(&self.show, song.id, &self.bible_slots).is_some()).count();
        // Songs the library has already: as asked, else ask (once, for all of them).
        let same = match same {
            Some(same) => same,
            None if already == 0 => pack::Same::Skip,
            None => {
                let answer = rfd::MessageDialog::new()
                    .set_title("Import Song Pack")
                    .set_description(format!(
                        "{} in the pack {} already here. Replace them with the pack's, keep both, or skip them?",
                        if already == 1 { "1 song".to_owned() } else { format!("{already} songs") },
                        if already == 1 { "is" } else { "are" }
                    ))
                    .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                        "Replace".to_owned(),
                        "Keep Both".to_owned(),
                        "Skip".to_owned(),
                    ))
                    .show();
                match pack::three_way(answer, ["Replace", "Keep Both", "Skip"]) {
                    Some(0) => pack::Same::Replace,
                    Some(1) => pack::Same::KeepBoth,
                    _ => pack::Same::Skip,
                }
            }
        };
        // Which songs are taken, and the new ids of copies kept beside songs already here.
        let mut renamed: HashMap<Uuid, Uuid> = HashMap::new();
        let mut taken = Vec::new();
        let mut skipped = Vec::new();
        for song in &opened.songs {
            let here = existing(&self.show, song.id, &self.bible_slots);
            match (here, same) {
                (Some(_), pack::Same::Skip) => skipped.push(song.title.clone()),
                (Some(_), pack::Same::KeepBoth) => {
                    renamed.insert(song.id, Uuid::new_v4());
                    taken.push((song.clone(), None));
                }
                (here, _) => taken.push((song.clone(), here)),
            }
        }
        let remap = |key: &mut String| {
            if let Some(new) = Uuid::parse_str(key).ok().and_then(|id| renamed.get(&id)) {
                *key = new.to_string();
            }
        };
        // What this computer has, for slide actions that point at its things.
        let (timers, messages, props): (Vec<Uuid>, Vec<Uuid>, Vec<Uuid>) =
            self.running.as_ref().map_or_else(Default::default, |r| {
                let state = &r.home_state;
                (
                    state.timers.iter().map(|t| t.id).collect(),
                    state.messages.iter().map(|m| m.id).collect(),
                    state.props.iter().map(|p| p.id).collect(),
                )
            });
        let mut media_seen = PackMedia::default();
        let mut brought = pack::Brought::default();
        let mut themes_in: HashMap<Uuid, Uuid> = HashMap::new();
        let mut new_themes = 0;
        let mut lost_actions = 0;
        let mut added = Vec::new();
        let mut replaced = Vec::new();
        let mut waiting = Vec::new();
        for (mut song, here) in taken {
            if let Some(new) = renamed.get(&song.id) {
                song.id = *new;
            }
            // No control characters from a pack reach the window, the log or a terminal.
            song.title.retain(|c| !c.is_control());
            pack::bring_song_files(&mut song, &opened, &dir, &mut brought);
            for group in &mut song.groups {
                for slide in &mut group.slides {
                    for action in &mut slide.actions {
                        let found = match action {
                            ShowAction::Song { key }
                            | ShowAction::Announce { what: sp_model::Announced::Song { key } } => {
                                remap(key);
                                true
                            }
                            ShowAction::Timer { id, .. } => timers.contains(id),
                            ShowAction::Message { id, .. } => messages.contains(id),
                            ShowAction::Prop { id, .. } => props.contains(id),
                            ShowAction::Media { id }
                            | ShowAction::Announce { what: sp_model::Announced::Media { id } } => {
                                match self.pack_media_here(*id, &opened, &dir, &mut brought, &mut media_seen) {
                                    Some(here) => {
                                        *id = here;
                                        true
                                    }
                                    None => false,
                                }
                            }
                            _ => true,
                        };
                        lost_actions += usize::from(!found);
                    }
                }
            }
            // The song's own theme comes with it, as a theme of this computer's; a song using the
            // main theme takes on this computer's main theme.
            song.theme = match song.theme {
                Some(id) if themes_in.contains_key(&id) => themes_in.get(&id).copied(),
                // A theme this computer has (the same id) is used as it is here, never changed or
                // added again; one it doesn't have comes in, under its own id.
                Some(id) if self.themes.iter().any(|have| have.id == id) => Some(id),
                Some(id) => opened.themes.get(&id).map(|theme| {
                    let mut theme = theme.clone();
                    theme.name.retain(|c| !c.is_control());
                    pack::bring_theme_files(&mut theme, &opened, &dir, &mut brought);
                    let base = theme.name.clone();
                    let mut n = 2;
                    while self.themes.iter().any(|have| have.name == theme.name) {
                        theme.name = format!("{base} ({n})");
                        n += 1;
                    }
                    if theme_store::save(&theme).is_ok()
                        && let Some(file) = theme_store::file_of(&theme)
                    {
                        self.stamps.note(&file);
                    }
                    self.themes.push(theme);
                    themes_in.insert(id, id);
                    new_themes += 1;
                    id
                }),
                None => None,
            };
            let title = song.title.clone();
            match here {
                // Replaced: written over its file, then read again as an outside change is (so a
                // song on screen, being edited or holding changes waits on a choice in the bar).
                Some(index) => match self.sources.get(index).cloned().flatten() {
                    Some(path) => {
                        if let Err(problem) = load::save_song(&path, &song) {
                            waiting.push(format!("{title} ({problem})"));
                            continue;
                        }
                        if self.freshen(index, false) {
                            replaced.push(title);
                        } else {
                            waiting.push(title);
                        }
                    }
                    None => {
                        self.show.replace_presentation(index, Arc::new(song));
                        self.unsaved.push(index);
                        replaced.push(title);
                    }
                },
                None => {
                    let (_, key) = self.store_song(song);
                    if let Some(id) = library
                        && let Some(running) = self.running.as_mut()
                        && let Some(into) = running.home_state.libraries.iter_mut().find(|l| l.id == id)
                    {
                        into.songs.push(key);
                        running.home_state.libraries_changed = true;
                    }
                    added.push(title);
                }
            }
        }
        if new_themes > 0 {
            self.themes_changed();
        }
        // The playlist's pictures and videos, and its media playlists (with theirs), come into the
        // media bin and the media playlists, and the items point at them.
        let mut playlist = opened.playlist.clone();
        let mut new_media_playlists: Vec<media::Playlist> = Vec::new();
        for item in playlist.iter_mut().flat_map(|playlist| playlist.items.iter_mut()) {
            match item {
                service::Item::Song { key, .. } => remap(key),
                service::Item::Media { id, name } => {
                    name.retain(|c| !c.is_control());
                    if let Some(here) = self.pack_media_here(*id, &opened, &dir, &mut brought, &mut media_seen) {
                        *id = here;
                    }
                }
                service::Item::MediaPlaylist { id, name } => {
                    name.retain(|c| !c.is_control());
                    let have = self
                        .running
                        .as_ref()
                        .is_some_and(|r| r.home_state.media_playlists.iter().any(|playlist| playlist.id == *id));
                    // One this computer has (the same id) is used as it is here; one it doesn't
                    // comes in, with the pictures and videos the pack has of it.
                    if !have
                        && !new_media_playlists.iter().any(|made| made.id == *id)
                        && let Some(entry) = opened.media_playlists.iter().find(|entry| entry.id == *id)
                    {
                        let mut files = Vec::new();
                        for member in &entry.items {
                            if let Some(here) =
                                self.pack_media_here(*member, &opened, &dir, &mut brought, &mut media_seen)
                                && let Some(item) = self.show.media().iter().find(|item| item.id == here)
                            {
                                files.push(item.path.clone());
                            }
                        }
                        let mut made = media::Playlist::new(entry.name.chars().filter(|c| !c.is_control()).collect());
                        made.id = entry.id;
                        made.files = files;
                        new_media_playlists.push(made);
                    }
                }
                service::Item::Header { name } | service::Item::Placeholder { name } => {
                    name.retain(|c| !c.is_control());
                }
                service::Item::Passage(_) => {}
            }
        }
        if media_seen.added > 0 {
            self.save_media_list();
        }
        let media_playlists_added = new_media_playlists.len();
        if let Some(running) = self.running.as_mut()
            && !new_media_playlists.is_empty()
        {
            let state = &mut running.home_state;
            for mut made in new_media_playlists {
                let base = made.name.clone();
                let mut n = 2;
                while state.media_playlists.iter().any(|have| have.name == made.name) {
                    made.name = format!("{base} ({n})");
                    n += 1;
                }
                state.media_playlists.push(made);
            }
            state.media_playlists_changed = true;
        }
        self.save_unsaved();
        // The playlist, as a new playlist.
        let mut playlist_name = None;
        if let Some(mut playlist) = playlist
            && let Some(running) = self.running.as_mut()
        {
            playlist.name.retain(|c| !c.is_control());
            let services = &mut running.home_state.services;
            // The same playlist again (opening the pack a second time) isn't added twice.
            let here = services.list.iter().any(|have| have.name == playlist.name && have.items == playlist.items);
            if here {
                playlist_name = Some(format!("{} (here already)", playlist.name));
            } else {
                let mut name = playlist.name.clone();
                let mut n = 2;
                while services.list.iter().any(|have| have.name == name) {
                    name = format!("{} ({n})", playlist.name);
                    n += 1;
                }
                services.list.push(service::Service { name: name.clone(), items: playlist.items });
                services.changed = true;
                playlist_name = Some(name);
            }
        }
        let _ = std::fs::remove_dir_all(&opened.staging);
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
        // In words.
        let list = |names: &[String]| names.join(", ");
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!(
                "Imported {}: {}.",
                if added.len() == 1 { "1 song".to_owned() } else { format!("{} songs", added.len()) },
                list(&added)
            ));
        }
        if !replaced.is_empty() {
            parts.push(format!("Replaced: {}.", list(&replaced)));
        }
        if !skipped.is_empty() {
            parts.push(format!("Skipped, already here: {}.", list(&skipped)));
        }
        if !waiting.is_empty() {
            parts.push(format!(
                "Written, waiting on Reload or Keep mine (in use or holding changes): {}.",
                list(&waiting)
            ));
        }
        let playlist_in = playlist_name.is_some();
        match playlist_name {
            Some(name) if name.ends_with("(here already)") => {
                parts.push(format!("The playlist {} is here already.", name.trim_end_matches(" (here already)")));
            }
            Some(name) => parts.push(format!("Added the playlist {name}.")),
            None => {}
        }
        if new_themes > 0 {
            parts.push(format!("Added {} theme{}.", new_themes, if new_themes == 1 { "" } else { "s" }));
        }
        if media_playlists_added > 0 {
            parts.push(format!(
                "Added {} media playlist{}.",
                media_playlists_added,
                if media_playlists_added == 1 { "" } else { "s" }
            ));
        }
        let (pictures, videos) = (brought.pictures, brought.videos);
        let things: Vec<String> = [(pictures, "picture"), (videos, "video")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}{}", if n == 1 { "" } else { "s" }))
            .collect();
        if !things.is_empty() {
            parts.push(format!("Brought {}.", things.join(" and ")));
        }
        if lost_actions > 0 {
            parts.push(if lost_actions == 1 {
                "1 slide action points at a message, prop, timer or media item this computer doesn't have; kept, shown as missing.".to_owned()
            } else {
                format!("{lost_actions} slide actions point at messages, props, timers or media items this computer doesn't have; kept, shown as missing.")
            });
        }
        if !opened.ignored.is_empty() {
            parts.push(format!("Left out of the pack: {}.", list(&opened.ignored)));
        }
        // Nothing usable in the pack (only things left out) is a failure; songs skipped because
        // they are here already are not.
        let anything =
            !added.is_empty() || !replaced.is_empty() || !skipped.is_empty() || !waiting.is_empty() || playlist_in;
        let said = if parts.is_empty() { "The pack had no songs in it.".to_owned() } else { parts.join(" ") };
        if anything { Ok(said) } else { Err(said) }
    }

    /// Makes the new song that was asked for, and gives a song the title just typed for it.
    fn handle_songs(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if let Some((index, title)) = running.home_state.pending_title.take()
            && let Some(song) = self.show.presentations().get(index)
        {
            let renamed = sp_model::Presentation { title, ..(**song).clone() };
            self.history.remember(index, song.clone());
            self.show.replace_presentation(index, Arc::new(renamed));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
        }
        // A group renamed in the editor: its name changes, and its colour with it.
        if let Some((index, group, name)) = running.home_state.pending_group_name.take()
            && let Some(song) = self.show.presentations().get(index)
            && group < song.groups.len()
        {
            let mut renamed = (**song).clone();
            renamed.groups[group].color = sp_model::group_color(&name);
            renamed.groups[group].name = name;
            let ids: Vec<Uuid> = renamed.groups[group].slides.iter().map(|slide| slide.id).collect();
            self.history.forget(index);
            self.show.replace_presentation(index, Arc::new(renamed));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            for id in ids {
                running.thumbs.forget(id, &mut running.egui_renderer);
            }
            running.home_window.request_redraw();
        }
        // A slide added at the end of a song in the editor: it is made, and opened to edit.
        if let Some(index) = running.home_state.add_slide.take()
            && let Some(song) = self.show.presentations().get(index)
        {
            let (longer, slide) = song.with_slide_added();
            self.history.forget(index);
            self.show.replace_presentation(index, Arc::new(longer));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            let at = sp_engine::SlideRef { presentation: index, slide };
            running.home_state.editing = EditDraft::of(&self.show, at);
            running.home_window.request_redraw();
        }
        let Some(asked) = running.home_state.new_song_request.take() else {
            return;
        };
        let title = asked.title.clone();
        let id = Uuid::new_v4();
        // The words given, made into slides as an imported song's are (a lyric file pasted in
        // gives its sections and details); with none, one empty slide in a first verse.
        let parsed = sp_model::import_song_text(&title, &asked.words);
        let copyright = parsed.copyright.clone();
        let groups = if parsed.slide_count() > 0 {
            parsed.groups
        } else {
            vec![sp_model::Group {
                id: Uuid::new_v4(),
                name: "Verse 1".to_owned(),
                color: sp_model::group_color("Verse 1"),
                slides: vec![Slide::new(vec![sp_model::Object::text(sp_model::Role::primary(), "")])],
            }]
        };
        let theme = asked.theme.and_then(|at| self.themes.get(at)).map(|theme| theme.id);
        let song = sp_model::Presentation {
            id,
            title: title.clone(),
            groups,
            copyright,
            theme,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
        };
        self.add_new_song(song);
    }

    /// Puts a song made in the app into the library, gives it a file in the Songs folder, and
    /// shows its slides with its title ready to type over.
    fn add_new_song(&mut self, song: sp_model::Presentation) {
        self.add_song(song, true);
    }

    /// Imports songs from files and folders, as the File menu and the import button do (see
    /// [`App::import_songs_to`]). Returns the titles imported, a plain sentence for each file
    /// that wasn't, and what is being made into slides or opened in the background.
    fn import_songs(&mut self, paths: &[PathBuf]) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut done = self.import_songs_to(paths, &SongsTo::default());
        done.decks.append(&mut done.packs);
        (done.titles, done.problems, done.decks)
    }

    /// Imports songs from files and folders, as the File menu, a drop on the window and
    /// `sp song import` do. Each song goes into the Songs folder; a folder's songs go into a
    /// library named after it (made if there isn't one), and other songs into the library
    /// shown, if any, unless `to` names the library; `to` may also name a playlist they are
    /// added to. Each song in a song bundle comes in, in the bundle's order, and the pictures
    /// and videos they show into the library's Media folder and the media bin; a large bundle is
    /// read on a thread of its own, and its songs come in when it has been. The first song's
    /// slides are shown. A song the library already has, word for word under the same title,
    /// isn't added again (it still goes into the library or playlist named). The window's
    /// notice says what happened.
    fn import_songs_to(&mut self, paths: &[PathBuf], to: &SongsTo) -> Imported {
        let started = Instant::now();
        let mut done = Imported::default();
        // PDFs, presentation files and folders of pictures become slides, made on threads of
        // their own. A presentation file needs a program on this Mac that opens it.
        let mut songs = Vec::new();
        for path in paths {
            if pack::is_pack(path) {
                match self.open_pack(path.clone(), None, self.library_for(to), false) {
                    Ok(said) => done.packs.push(said),
                    Err(problem) => done.problems.push(problem),
                }
            } else if deck::is_presentation_file(path) {
                match convert::program_for(path) {
                    Some(program) => {
                        let source = deck::Source::File(path.clone());
                        done.decks.push(format!("{} (with {})", source.name(), program.name()));
                        self.start_deck(source);
                    }
                    None => done.problems.push(convert::none_installed(path)),
                }
            } else if deck::is_pdf(path) || (path.is_dir() && deck::pictures_in(path).is_some()) {
                let source = deck::Source::File(path.clone());
                done.decks.push(source.name());
                self.start_deck(source);
            } else {
                songs.push(path.clone());
            }
        }
        let files = load::files_to_import(&songs);
        // Each file is one song, but a songbook (SoftProjector's) is many, each going into a
        // library named after its songbook.
        let mut read: Vec<SongRead> = Vec::new();
        for (path, library) in &files {
            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            if load::is_songbook(path) {
                match load::import_songbook(path) {
                    Ok(book) => read.extend(book.into_iter().map(|(song, book)| SongRead::Song {
                        song: Ok(song),
                        path: path.clone(),
                        library: book.or_else(|| library.clone()),
                    })),
                    Err(problem) => {
                        read.push(SongRead::Song { song: Err(problem), path: path.clone(), library: None });
                    }
                }
                continue;
            }
            if bundle::is_bundle(path) {
                if let Some(opened) = self.open_bundle(path, to, library.clone(), &mut done) {
                    read.push(opened);
                }
                continue;
            }
            let song = if load::is_song_file(path) {
                load::read_song_file(path).map_err(|reason| format!("{name}: {reason}"))
            } else {
                load::import_file(path)
            };
            read.push(SongRead::Song { song, path: path.clone(), library: library.clone() });
        }
        self.take_songs(read, to, &mut done);
        applog::event(
            "songs",
            &format!(
                "imported {} of {} files in {} ms ({} not imported)",
                done.titles.len(),
                files.len(),
                started.elapsed().as_millis(),
                done.problems.len()
            ),
        );
        self.say_imported(&done);
        done
    }

    /// The library songs imported go into, by what `to` says: the one it names (or none), else
    /// the library shown.
    fn library_for(&self, to: &SongsTo) -> Option<Uuid> {
        match to.library {
            Some(chosen) => chosen,
            None => self.running.as_ref().and_then(|r| r.home_state.library_shown),
        }
    }

    /// Opens a song bundle to import: at once when it is small (and returns it, to be taken in
    /// with the other songs), else on a thread of its own, whose songs come in when it has been
    /// read (see [`App::finish_packs`]).
    fn open_bundle(
        &mut self,
        path: &std::path::Path,
        to: &SongsTo,
        library: Option<String>,
        done: &mut Imported,
    ) -> Option<SongRead> {
        let Some(dir) = bible_store::data_dir() else {
            done.problems.push("Could not find the library folder.".to_owned());
            return None;
        };
        // Beside the song packs being opened, so a folder left by a crash is tidied with theirs.
        let staging = dir.join(format!(".opening-pack-{}", Uuid::new_v4()));
        let small = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= PACK_READ_AT_ONCE);
        if small || self.running.is_none() {
            return match bundle::open(path, &staging) {
                Ok(opened) => Some(SongRead::Bundle { opened, path: path.to_owned(), library }),
                Err(problem) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    done.problems.push(problem);
                    None
                }
            };
        }
        let (tx, waker) = (self.pack_tx.clone(), self.waker.clone());
        let job = self.start_job();
        let (path, to) = (path.to_owned(), to.clone());
        done.opening.push(path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
        std::thread::spawn(move || {
            let result = bundle::open(&path, &staging);
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&staging);
            }
            let _ = tx.send(PackJob::Bundle { job, result: Box::new(result), path, to, library });
            if let Some(waker) = waker {
                let _ = waker.send_event(AppEvent::PackDone);
            }
        });
        None
    }

    /// Takes in the songs read for an import (see [`App::import_songs_to`]), noting in `done`
    /// what came in, what was here already and what couldn't come.
    fn take_songs(&mut self, read: Vec<SongRead>, to: &SongsTo, done: &mut Imported) {
        // What the library has, looked up once rather than song by song.
        let mut ids: std::collections::HashSet<Uuid> = self.show.presentations().iter().map(|s| s.id).collect();
        // A song deleted in the app (or a Bible page) isn't "already here": its file can be
        // imported again. Its id stays taken, so the one imported gets an id of its own.
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        let mut have: HashMap<(String, String), usize> = self
            .show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, _)| !removed.contains(at) && !self.app_made(*at))
            .map(|(at, s)| (import_key(s), at))
            .collect();
        let chosen_library = to.library.or(self.import_library_override);
        if let Some(Some(id)) = to.library {
            done.library = self.running.as_ref().and_then(|r| {
                r.home_state.libraries.iter().find(|library| library.id == id).map(|library| library.name.clone())
            });
        }
        for read in read {
            match read {
                SongRead::Song { song: Ok(song), path, library } => {
                    self.take_song(song, &path, &library, None, chosen_library, &mut have, &mut ids, done);
                }
                SongRead::Song { song: Err(problem), .. } => done.problems.push(problem),
                SongRead::Bundle { opened, path, library } => {
                    // Its songs one after another, in its order, sharing its pictures and videos.
                    let mut bringing = Bringing { opened: &opened, placed: bundle::Placed::new(), failed: Vec::new() };
                    for read in &opened.songs {
                        let from = Some((&mut bringing, read));
                        self.take_song(
                            read.song.clone(),
                            &path,
                            &library,
                            from,
                            chosen_library,
                            &mut have,
                            &mut ids,
                            done,
                        );
                    }
                    // What the bundle left out, said once for it.
                    let mut lost = opened.left_out.clone();
                    lost.extend(bringing.failed);
                    if !lost.is_empty() {
                        for each in &lost {
                            applog::event("songs", &format!("left out of {}: {each}", file_name(&path)));
                        }
                        const NAMED: usize = 10;
                        let more = lost.len().saturating_sub(NAMED);
                        lost.truncate(NAMED);
                        let more = if more > 0 { format!(" and {more} more (see the log)") } else { String::new() };
                        done.notes.push(match opened.songs.as_slice() {
                            [one] => format!("{}: left out of the bundle: {}{more}.", one.song.title, lost.join(", ")),
                            _ => format!("Left out of {}: {}{more}.", file_name(&path), lost.join(", ")),
                        });
                    }
                    // Its own folder goes once its songs are in (or aren't coming).
                    let _ = std::fs::remove_dir_all(&opened.staging);
                }
            }
        }
        if let Some(at) = done.first {
            self.apply(vec![Command::Select(at)]);
        }
        self.add_imported_to_playlist(to, done);
        if let Some(running) = self.running.as_mut()
            && !done.titles.is_empty()
        {
            running.home_state.editing = None;
            running.home_state.bible_open = false;
        }
    }

    /// Takes in one song read for an import: skipped when the library has it already (but put
    /// into the library named), else stored, with the pictures and videos it shows from its
    /// bundle (`bundle`: the bundle being taken in, and the song as read from it) brought in,
    /// and put into its library.
    #[allow(clippy::too_many_arguments)]
    fn take_song(
        &mut self,
        mut song: sp_model::Presentation,
        path: &std::path::Path,
        library: &Option<String>,
        bundle: Option<(&mut Bringing, &bundle::Song)>,
        chosen_library: Option<Option<Uuid>>,
        have: &mut HashMap<(String, String), usize>,
        ids: &mut std::collections::HashSet<Uuid>,
        done: &mut Imported,
    ) {
        let look = import_key(&song);
        if let Some(at) = already_here(&song, have, ids, self.show.presentations()) {
            done.problems.push(format!("{}{ALREADY}", song.title));
            done.first.get_or_insert(at);
            done.landed.push(at);
            // A song in two of the folders imported goes into both their libraries; one dropped
            // on a library goes into it.
            if let Some(running) = self.running.as_mut() {
                let state = &mut running.home_state;
                if let Some(key) = state.song_keys.get(at).cloned() {
                    let into = state.libraries.iter_mut().find(|l| match chosen_library {
                        Some(chosen) => Some(l.id) == chosen,
                        None => library.as_ref().is_some_and(|name| l.name == *name),
                    });
                    if let Some(into) = into
                        && !into.has(&key)
                    {
                        into.songs.push(key);
                        state.libraries_changed = true;
                    }
                }
            }
            return;
        }
        // A song made from another file gets an id of its own, so the two are never mixed up.
        if !load::is_song_file(path) {
            song.id = Uuid::new_v4();
        }
        // The pictures and videos it shows from its bundle come into the library, and its
        // slides show them.
        if let Some((bringing, read)) = bundle {
            self.bring_bundle_media(&mut song, bringing, read, done);
        }
        done.titles.push(song.title.clone());
        ids.insert(song.id);
        let (index, key) = self.store_song(song);
        have.insert(look, index);
        done.first.get_or_insert(index);
        done.landed.push(index);
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            let into = match (library, chosen_library) {
                // `--library NAME|none` on the command line, or a drop on a library, decides for a
                // folder's songs too.
                (_, Some(chosen)) => chosen.and_then(|id| state.libraries.iter().position(|l| l.id == id)),
                (Some(name), None) => {
                    if !done.libraries_used.contains(name) {
                        done.libraries_used.push(name.clone());
                    }
                    let at = match state.libraries.iter().position(|l| l.name == *name) {
                        Some(at) => at,
                        None => {
                            state.libraries.push(libraries::Library::new(name.clone()));
                            state.libraries.len() - 1
                        }
                    };
                    Some(at)
                }
                (None, None) => state.library_shown.and_then(|id| state.libraries.iter().position(|l| l.id == id)),
            };
            if let Some(at) = into {
                state.libraries[at].songs.push(key);
                state.libraries_changed = true;
            }
        }
    }

    /// Brings the pictures and videos a song imported from a bundle shows (`read`, as read from
    /// the bundle being taken in) into the library's Media folder and the media bin (the bin's
    /// own item when it has the same file already), each once for the whole bundle, and has the
    /// song's slides show them. The pictures the bundle doesn't hold are noted in `done`, by file
    /// name; those that couldn't be read, in `bringing`.
    fn bring_bundle_media(
        &mut self,
        song: &mut sp_model::Presentation,
        bringing: &mut Bringing,
        read: &bundle::Song,
        done: &mut Imported,
    ) {
        let title = song.title.clone();
        if let Some(dir) = bible_store::data_dir() {
            let mut brought = pack::Brought::default();
            // Items added to the bin, pictures and videos.
            let mut added = (0, 0);
            let (show, pictures) = (&mut self.show, &mut self.bin_pictures);
            let (opened, placed) = (bringing.opened, &mut bringing.placed);
            let failed = bundle::bring(song, &read.slides, opened, placed, &dir, &mut brought, |path, staged| {
                if let Some(have) = show.media().iter().find(|item| item.path == path) {
                    return Some(have.id);
                }
                let (picture, whole) = media::load_small(path).ok()?;
                let mut item = MediaItem::new(staged.name.clone(), path.to_owned());
                item.looping = staged.looping;
                let id = item.id;
                pictures.set_small(id, picture, whole);
                show.add_media(item);
                if staged.video {
                    added.1 += 1;
                } else {
                    added.0 += 1;
                }
                Some(id)
            });
            if added != (0, 0) {
                self.save_media_list();
            }
            bringing.failed.extend(failed.into_iter().map(|name| format!("{name} (couldn't be read)")));
            done.pictures += added.0;
            done.videos += added.1;
        }
        if !read.missing.is_empty() {
            done.notes.push(format!("{title}: not in the bundle: {}.", read.missing.join(", ")));
        }
    }

    /// Adds the songs an import brought in (or found here already) to the playlist `to` names,
    /// if any: after the item it names in the playlist in front, else at the playlist's end. The
    /// playlist is found again by its name if the list has changed since the drop.
    fn add_imported_to_playlist(&mut self, to: &SongsTo, done: &mut Imported) {
        let Some(spot) = &to.playlist else { return };
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        let songs = self.show.presentations();
        let items: Vec<service::Item> = done
            .landed
            .iter()
            .filter_map(|&at| {
                Some(service::Item::Song { key: state.song_keys.get(at)?.clone(), title: songs.get(at)?.title.clone() })
            })
            .collect();
        let services = &mut state.services;
        let found = if services.list.get(spot.index).is_some_and(|playlist| playlist.name == spot.name) {
            Some(spot.index)
        } else {
            services.list.iter().position(|playlist| playlist.name == spot.name)
        };
        let Some(at) = found.filter(|_| !items.is_empty()) else { return };
        put_in_playlist(services, at, spot.after, items);
        done.playlist = Some(services.list[at].name.clone());
    }

    /// Says in the window's notice what an import did, and returns the words.
    fn say_imported(&mut self, done: &Imported) -> String {
        for problem in &done.problems {
            applog::event("songs", &format!("not imported: {problem}"));
        }
        let into = match done.libraries_used.as_slice() {
            [] => done.library.as_ref().map_or_else(String::new, |name| format!(" into the library {name}")),
            [one] => format!(" into the library {one}"),
            many => format!(" into {} libraries", many.len()),
        };
        let busy = !done.decks.is_empty() || !done.opening.is_empty() || !done.packs.is_empty();
        // Several songs here already (a bundle's, say) are said together.
        let all_here = done.problems.len() > 1 && done.problems.iter().all(|problem| problem.ends_with(ALREADY));
        let said = match (done.titles.as_slice(), done.problems.as_slice()) {
            ([], []) if busy => None,
            ([], []) => Some("There were no songs to import there.".to_owned()),
            ([one], []) => Some(format!("Imported {one}{into}.")),
            (many, []) => Some(format!("Imported {} songs{into}.", many.len())),
            ([], [problem]) => Some(problem.clone()),
            ([], rest) if all_here => Some(format!("All {} songs are already in the library.", rest.len())),
            ([], rest) => Some(format!(
                "No songs imported. {}{}",
                rest[0],
                if rest.len() > 1 { format!(" (and {} more; see the log)", rest.len() - 1) } else { String::new() }
            )),
            (imported, rest) if all_here => Some(format!(
                "Imported {} of {} songs{into}; the other {} are already in the library.",
                imported.len(),
                imported.len() + rest.len(),
                rest.len()
            )),
            (imported, rest) => Some(format!(
                "Imported {} of {} songs{into}. {}{}",
                imported.len(),
                imported.len() + rest.len(),
                rest[0],
                if rest.len() > 1 { format!(" (and {} more; see the log)", rest.len() - 1) } else { String::new() }
            )),
        };
        let mut words: Vec<String> = said.into_iter().collect();
        words.extend(done.packs.iter().cloned());
        let things: Vec<String> = [(done.pictures, "picture"), (done.videos, "video")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}{}", if n == 1 { "" } else { "s" }))
            .collect();
        if !things.is_empty() {
            words.push(format!("Brought {} into the media bin.", things.join(" and ")));
        }
        words.extend(done.notes.iter().cloned());
        if let Some(name) = &done.playlist {
            words.push(format!("Added to the playlist {name}."));
        }
        // Songs here already, dropped on a library: they are in it now.
        if let Some(name) = done.library.as_ref().filter(|_| done.titles.is_empty() && !done.landed.is_empty()) {
            words.push(format!("Put into the library {name}."));
        }
        if !done.decks.is_empty() {
            words.push(format!("Making slides from {}\u{2026}", done.decks.join(", ")));
        }
        if !done.opening.is_empty() {
            words.push(format!(
                "Opening {}; the window says when {} songs are in.",
                done.opening.join(", "),
                if done.opening.len() == 1 { "its" } else { "their" }
            ));
        }
        let text = words.join(" ");
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = Some(text.clone()).filter(|text| !text.is_empty());
            running.home_window.request_redraw();
        }
        text
    }

    /// Starts making slides from a PDF or pictures, on a thread of its own; the presentation
    /// is added when it is done (see [`App::handle_decks`]).
    fn start_deck(&mut self, source: deck::Source) {
        applog::event("slides", &format!("making slides from {}", source.name()));
        let Some(running) = self.running.as_ref() else {
            // No window to wake: make it now.
            match deck::make(&source) {
                Ok(made) => self.add_song(made, false),
                Err(problem) => applog::event("slides", &problem),
            }
            return;
        };
        // The event loop is woken directly: a repaint request alone doesn't reach a window that is
        // hidden or behind others, and the deck would wait for the next time it is drawn.
        let ctx = running.egui_ctx.clone();
        let waker = self.waker.clone();
        self.decks.start(source, move || match waker {
            Some(waker) => {
                let _ = waker.send_event(AppEvent::DeckDone);
            }
            None => ctx.request_repaint(),
        });
    }

    /// Carries out what the setup asked: importing songs, which needs more than the window.
    fn handle_setup(&mut self) {
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.setup_request.take()) else { return };
        match request {
            setup::SetupRequest::ImportLibrary(folder) => {
                self.import_songs(&[folder]);
            }
            setup::SetupRequest::ChooseSongFiles => self.choose_songs_to_import(),
        }
        if let Some(running) = self.running.as_mut() {
            let said = running.home_state.notice.clone();
            if let Some(setup) = running.home_state.setup.as_mut() {
                setup.songs_said = said;
            }
            running.home_window.request_redraw();
        }
    }

    /// Adds the decks made since the last look, and says how each went.
    fn handle_decks(&mut self) {
        for (path, made) in self.decks.finished() {
            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let said = match made {
                Ok(made) => {
                    let count = made.slide_count();
                    applog::event("slides", &format!("made {count} slides from {name}"));
                    self.add_song(made, false);
                    format!("Made {count} slide{} from {name}.", if count == 1 { "" } else { "s" })
                }
                Err(problem) => {
                    applog::event("slides", &format!("could not make slides: {problem}"));
                    problem
                }
            };
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some(said);
                running.home_window.request_redraw();
            }
        }
    }

    /// Asks for presentations, PDFs or pictures to make slides of: each presentation file or
    /// PDF becomes a presentation, and the pictures chosen together become one.
    fn choose_slides_to_import(&mut self) {
        let mut endings = deck::DECK_ENDINGS.to_vec();
        endings.push(deck::PDF_ENDING);
        endings.extend(media::PICTURES);
        let Some(chosen) = rfd::FileDialog::new()
            .set_title("Import Slides")
            .add_filter("Presentations, PDFs and pictures", &endings)
            .pick_files()
        else {
            return;
        };
        let (pictures, files): (Vec<PathBuf>, Vec<PathBuf>) =
            chosen.into_iter().partition(|path| media::is_media(path) && !media::is_video(path));
        if !files.is_empty() {
            self.import_songs(&files);
        }
        if !pictures.is_empty() {
            self.start_deck(deck::chosen_pictures(pictures));
        }
    }

    /// Asks which song files to import, and imports them.
    fn choose_songs_to_import(&mut self) {
        let endings = load::import_endings();
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import Songs")
            .add_filter("Songs (text, lyric, ChordPro, rich-text, Word and song documents)", &endings)
            .pick_files()
        else {
            return;
        };
        self.import_songs(&paths);
    }

    /// Asks for song documents, a library folder, or the folder of libraries, and imports them,
    /// each folder becoming a library. Opens where ProPresenter keeps its libraries, when that
    /// folder is there.
    fn choose_library_folder_to_import(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose Songs or a Library Folder").add_filter(
            "ProPresenter songs (.pro, .proBundle)",
            &[load::PRO_ENDING, load::PRO_BUNDLE_ENDINGS[0], load::PRO_BUNDLE_ENDINGS[1]],
        );
        if let Some(home) = std::env::var_os("HOME") {
            let libraries = PathBuf::from(home).join("Documents").join("ProPresenter").join("Libraries");
            if libraries.is_dir() {
                dialog = dialog.set_directory(libraries);
            }
        }
        // A Mac's file window takes song documents and folders at once; elsewhere, a folder (of
        // one library, or of all of them) is chosen.
        #[cfg(target_os = "macos")]
        let chosen = dialog.pick_files_or_folders();
        #[cfg(not(target_os = "macos"))]
        let chosen = dialog.pick_folders();
        if let Some(chosen) = chosen {
            self.import_songs(&chosen);
        }
    }

    /// Puts a song into the library and gives it a file in the Songs folder; `rename` leaves
    /// its title ready to type over, as for a song just made.
    fn add_song(&mut self, song: sp_model::Presentation, rename: bool) {
        let (index, key) = self.store_song(song);
        self.apply(vec![Command::Select(index)]);
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            // A song made while a library is shown goes into that library.
            if let Some(library) = state.library_shown.and_then(|id| state.libraries.iter_mut().find(|l| l.id == id)) {
                library.toggle(&key);
                state.libraries_changed = true;
            }
            // Back to the slides of the new song.
            state.editing = None;
            state.bible_open = false;
            state.rename_started = rename;
            running.home_window.request_redraw();
        }
    }

    /// Adds a song to the show with a file of its own in the Songs folder, to be saved shortly,
    /// and returns its place and the key the song list knows it by.
    fn store_song(&mut self, song: sp_model::Presentation) -> (usize, String) {
        let (id, title) = (song.id, song.title.clone());
        // New songs live in the app's Songs folder, each in a file named by its id, so two
        // songs with one title never collide.
        let path = bible_store::data_dir().map(|dir| dir.join("Songs").join(format!("{id}.{}", load::SONG_EXTENSION)));
        if let Some(folder) = path.as_ref().and_then(|path| path.parent()) {
            let _ = std::fs::create_dir_all(folder);
        }
        let index = self.show.add_presentation(Arc::new(song));
        self.sources.push(path.clone());
        self.unsaved.push(index);
        self.unsaved_since = Some(Instant::now());
        let key = song_ids::key_for(path.as_deref(), id, &title);
        if let Some(running) = self.running.as_mut() {
            running.home_state.song_keys.push(key.clone());
        }
        (index, key)
    }

    /// True for a presentation the app makes itself (a Bible passage, the VoxLive item): shown,
    /// never saved, edited, read again or put in a pack.
    pub(crate) fn app_made(&self, presentation: usize) -> bool {
        self.bible_slots.contains(&presentation) || presentation == self.voxlive_slot
    }

    /// The theme a presentation's slides are drawn with: the Bible theme for a Bible passage,
    /// the song's own theme if it names one, otherwise the songs theme.
    pub(crate) fn theme_of(&self, presentation: usize) -> &Arc<Theme> {
        if self.bible_slots.contains(&presentation) {
            return &self.theme_for_bible;
        }
        let own = self.show.presentations().get(presentation).and_then(|song| song.theme);
        own.and_then(|id| self.theme_arcs.iter().find(|theme| theme.id == id)).unwrap_or(&self.theme)
    }

    /// Each theme's templates as boxes, for the pictures in the theme picker.
    fn theme_layouts(&self) -> Vec<Vec<Vec<(sp_model::Role, sp_model::SlideRect)>>> {
        let boxes = |theme: &Theme| (0..theme.templates.len()).map(|at| home::layout_boxes(theme, at)).collect();
        self.themes.iter().map(boxes).collect()
    }

    /// What Bible slides name as their template: the Bible template of whichever theme draws
    /// them.
    fn bible_template(&self) -> Option<Uuid> {
        Some(sp_model::BIBLE_TEMPLATE)
    }

    /// Brings everything that depends on the themes up to date after one was changed or
    /// chosen: the copies slides are drawn with, every thumbnail, and what the window lists.
    /// The output is left as it is; `refresh_output` brings it up to date.
    fn themes_changed(&mut self) {
        self.songs_theme = self.songs_theme.min(self.themes.len().saturating_sub(1));
        // Bible slides are drawn with the main theme's Bible template.
        self.bible_theme = self.songs_theme;
        // A theme that is as it was keeps the copy already made of it. Dragging a box in the
        // Theme Editor comes here on every frame: only the theme being changed is copied, and
        // a screen drawn with another theme of its own is not sent its theme (and drawn) again.
        let kept = std::mem::take(&mut self.theme_arcs);
        self.theme_arcs = self
            .themes
            .iter()
            .enumerate()
            .map(|(at, theme)| match kept.get(at) {
                Some(copy) if **copy == *theme => copy.clone(),
                _ => Arc::new(theme.clone()),
            })
            .collect();
        self.theme = self.theme_arcs[self.songs_theme].clone();
        self.theme_for_bible = self.theme.clone();
        self.bible_layout = self.theme.bible_template_index();
        let (layouts, pinned) = (self.theme_layouts(), self.bible_template());
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            let state = &mut running.home_state;
            state.theme_names = self.themes.iter().map(|theme| theme.name.clone()).collect();
            state.themes_kept =
                self.themes.iter().enumerate().filter(|(_, theme)| theme.id.is_nil()).map(|(at, _)| at).collect();
            state.theme_ids = self.themes.iter().map(|theme| theme.id).collect();
            state.theme_for_songs = self.songs_theme;
            state.theme_for_bible = self.bible_theme;
            state.theme_layouts = layouts;
            state.template_names = self.themes.iter().map(template_names).collect();
            state.song_templates = song_templates(&self.theme);
            state.bible_layout = self.bible_layout;
            state.bible.boxes = home::layout_boxes(&self.theme_for_bible, self.bible_layout);
            state.bible.box_labels = home::layout_labels(&self.theme_for_bible, self.bible_layout);
            state.bible.template = pinned;
            running.home_window.request_redraw();
        }
    }

    /// Carries out what the Theme menu, Bible Options and the Theme Editor asked for this frame.
    fn handle_themes(&mut self) {
        use home::ThemeRequest;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let request = running.home_state.theme_request.take();
        let editing = running.home_state.theme_edit.is_some();
        let count = self.themes.len();
        match request {
            Some(ThemeRequest::UseForSongs(index)) if index < count && index != self.songs_theme => {
                self.songs_theme = index;
                self.themes_changed();
                theme_store::remember_in_use(&self.theme);
                // Bible slides now use this theme's Bible template: the translations are pointed
                // at its boxes, as a fresh start.
                if let Some(running) = self.running.as_mut() {
                    running.home_state.bible.assign_boxes();
                }
                if !editing {
                    self.refresh_output();
                }
            }
            // A template of a theme made the one Bible slides use (from Bible Options; the Theme
            // Editor changes the theme it has open itself).
            Some(ThemeRequest::UseForBible(index, layout))
                if index < count && self.themes[index].templates.get(layout).is_some_and(|t| !t.bible) =>
            {
                self.themes[index].set_bible_template(layout);
                if let Some(edit) = running.home_state.theme_edit.as_mut().filter(|edit| edit.index == index) {
                    edit.theme.set_bible_template(layout);
                }
                if !self.themes_unsaved.contains(&index) {
                    self.themes_unsaved.push(index);
                }
                self.save_unsaved();
                self.themes_changed();
                if index == self.songs_theme {
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.bible.assign_boxes();
                    }
                    if !editing {
                        self.refresh_output();
                    }
                }
            }
            Some(ThemeRequest::Duplicate(index)) if index < count => {
                let mut theme = self.themes[index].clone();
                theme.id = Uuid::new_v4();
                theme.name = format!("{} copy", theme.name);
                self.themes.push(theme);
                self.themes_unsaved.push(self.themes.len() - 1);
                self.unsaved_since = Some(Instant::now());
                self.themes_changed();
                self.open_theme_editor(self.themes.len() - 1);
            }
            Some(ThemeRequest::New) => {
                let mut theme = (*self.theme).clone();
                theme.id = Uuid::new_v4();
                theme.name = "New Theme".to_owned();
                self.themes.push(theme);
                self.themes_unsaved.push(self.themes.len() - 1);
                self.unsaved_since = Some(Instant::now());
                self.themes_changed();
                self.open_theme_editor(self.themes.len() - 1);
            }
            Some(ThemeRequest::Edit(index)) if index < count => self.open_theme_editor(index),
            // The built-in theme, at the front of the list, is always kept.
            Some(ThemeRequest::Delete(index)) if index > 0 && index < count => {
                // Write what is waiting first, so the places in the list still mean what they did.
                self.save_unsaved();
                let gone = self.themes.remove(index);
                let problem = theme_store::delete(&gone).err();
                // Whatever used it falls back to the built-in theme; later themes move up one.
                let moved = |at: usize| match at.cmp(&index) {
                    std::cmp::Ordering::Less => at,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => at - 1,
                };
                let main_gone = index == self.songs_theme;
                self.songs_theme = moved(self.songs_theme);
                self.themes_changed();
                theme_store::remember_in_use(&self.theme);
                if main_gone && let Some(running) = self.running.as_mut() {
                    running.home_state.bible.assign_boxes();
                }
                let was_open =
                    self.running.as_ref().and_then(|r| r.home_state.theme_edit.as_ref().map(|edit| edit.index));
                if let Some(open) = was_open {
                    self.open_theme_editor(moved(open));
                } else {
                    self.refresh_output();
                }
                if let (Some(problem), Some(running)) = (problem, self.running.as_mut()) {
                    running.home_state.notice = Some(problem);
                }
            }
            _ => {}
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some(edit) = running.home_state.theme_edit.as_mut() else {
            return;
        };
        let (changed, leave, index) = (std::mem::take(&mut edit.changed), edit.leave, edit.index);
        if changed {
            let theme = edit.theme.clone();
            // In the main theme, another template for Bible slides (one marked, or the one
            // marked removed) points the translations at its boxes, as when one is chosen.
            let bible_id = |theme: &Theme| theme.templates.get(theme.bible_template_index()).map(|t| t.id);
            let bible_template_gone =
                index == self.songs_theme && self.themes.get(index).and_then(bible_id) != bible_id(&theme);
            if let Some(slot) = self.themes.get_mut(index) {
                *slot = theme;
            }
            if !self.themes_unsaved.contains(&index) {
                self.themes_unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            self.themes_changed();
            // The template Bible slides used was deleted: the translations are pointed at
            // the boxes of the one that took its place, as when a template is chosen.
            if bible_template_gone && let Some(running) = self.running.as_mut() {
                running.home_state.bible.assign_boxes();
            }
            // The theme is written once the changes pause for a second, on leaving the editor,
            // and on quitting.
        }
        if leave {
            if let Some(running) = self.running.as_mut() {
                running.home_state.theme_edit = None;
                running.home_state.editing = None;
            }
            // The theme is finished with: write it, and let the audience see it.
            self.save_unsaved();
            self.refresh_output();
        }
    }

    /// Opens the Theme Editor on the theme at a place in the list.
    fn open_theme_editor(&mut self, index: usize) {
        let (Some(theme), Some(running)) = (self.themes.get(index), self.running.as_mut()) else {
            return;
        };
        let edit = home::ThemeEdit::of(index, theme.clone());
        running.home_state.editing = edit.theme.templates.first().map(|first| EditDraft::of_slide(first.as_slide(), 0));
        running.home_state.theme_edit = Some(edit);
        running.home_state.bible_open = false;
        running.canvas_for = None;
        running.home_window.request_redraw();
    }

    /// True for a run that only saves a picture, or is asked to stay out of the way.
    fn quiet(&self) -> bool {
        self.options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some()
    }

    /// Brings the screens' windows and NDI sources in line with the list of screens and the
    /// displays connected now: opens, moves and closes windows, starts, renames and stops NDI,
    /// and says what each screen is doing. Cheap when nothing has changed, so it runs every
    /// frame and notices a display being plugged in or out.
    fn sync_screens(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let audience_on = self.show.output_enabled();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // Whether NDI is installed, looked at again every few seconds (a few file checks), so
        // installing it while the app runs is noticed.
        if running.ndi_checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(3)) {
            running.home_state.ndi_available = sp_ndi::installed();
            running.ndi_checked = Some(Instant::now());
        }
        // The DeckLink devices as last found (never waits on the driver).
        if let Some(found) = running.decklink.as_ref().map(sp_decklink::Discovery::found)
            && found != running.home_state.decklink
        {
            running.home_state.decklink = found;
        }
        // The displays connected, their names and places, and which the operator works on. The
        // list of displays is cheap to ask for and is looked at every time, so one plugged in or
        // out is seen at once; the rest (each display's name and size, which display the window
        // is on) costs more and is asked again only when that list changes, or after a second.
        let monitors: Vec<_> = el.available_monitors().collect();
        let fresh = running
            .displays_seen
            .as_ref()
            .is_some_and(|seen| seen.monitors == monitors && seen.at.elapsed() < DisplaysSeen::KEPT_FOR);
        if !fresh {
            let displays = display_list(monitors.iter().cloned(), el.primary_monitor());
            let operator = operator_displays(&monitors, running.home_window.current_monitor(), el.primary_monitor());
            running.displays_seen =
                Some(DisplaysSeen { at: Instant::now(), monitors: monitors.clone(), displays, operator });
        }
        let Some(seen) = running.displays_seen.as_ref() else { return };
        let (displays, operator) = (seen.displays.clone(), seen.operator.clone());
        if fresh {
            // A frame drawn with what was seen a moment ago: one more once that is a second old,
            // so a change seen in the meantime (a display's size, the window moved to another
            // display) is never left until something else happens.
            let due = seen.at + DisplaysSeen::KEPT_FOR;
            running.next_repaint = Some(running.next_repaint.map_or(due, |at| at.min(due)));
        }
        // A screen set up as Automatic (no longer offered) is given the first display that isn't
        // the main one and isn't taken; one saved under a display's old name ("Monitor #0") is
        // given the name System Settings shows ("3DTV"). Either way it is saved so.
        let mut given = false;
        for at in 0..running.home_state.screens.len() {
            let taken: Vec<String> = running
                .home_state
                .screens
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != at)
                .filter_map(|(_, screen)| match &screen.output {
                    screens::Output::Display(name) => Some(name.clone()),
                    _ => None,
                })
                .collect();
            let screen = &mut running.home_state.screens[at];
            let now = match &screen.output {
                screens::Output::Automatic => displays
                    .iter()
                    .find(|d| !d.main && !taken.iter().any(|name| d.is_named(name)))
                    .map(|d| d.name.clone()),
                screens::Output::Display(name) => {
                    displays.iter().find(|d| d.old_name == *name && d.name != *name).map(|d| d.name.clone())
                }
                _ => None,
            };
            if let Some(name) = now {
                screen.output = screens::Output::Display(name);
                given = true;
            }
        }
        if given {
            running.home_state.screens_changed = true;
        }
        if displays != running.home_state.displays {
            running.home_state.displays = displays.clone();
        }
        if std::mem::take(&mut running.home_state.screens_changed)
            && !quiet
            && let Err(problem) = screens::save(&running.home_state.screens)
        {
            running.home_state.notice = Some(problem);
        }
        let list = running.home_state.screens.clone();
        let mut placed = screens::place_all(&list, &displays, &operator);
        // A capture never fills a display or sends over the network, whatever the list says.
        if quiet {
            let ndi_aid = std::env::var_os("SOFTPRESENTER_NDI").is_some();
            for place in &mut placed {
                match place {
                    Placed::Fullscreen(_) => *place = Placed::Window,
                    Placed::Ndi(_) if !ndi_aid => {
                        *place = Placed::Nothing("Not sent in a quiet run (a test or a capture).".to_owned());
                    }
                    // Never a real card: it may be on a video chain in front of people.
                    // Pretend devices (a testing switch) play into nothing, so they go on.
                    Placed::DeckLink(_) if !sp_decklink::pretending() => {
                        *place = Placed::Nothing("Not played out in a quiet run (a test or a capture).".to_owned());
                    }
                    _ => {}
                }
            }
        }
        // A kind of screen switched off closes its windows, so the display shows what is behind
        // them rather than black. NDI goes on sending (black), so a stream keeps its source.
        for (screen, place) in list.iter().zip(placed.iter_mut()) {
            let on = match screen.kind {
                screens::Kind::Audience => audience_on,
                screens::Kind::Stage => running.home_state.stage_on,
            };
            if !on && matches!(place, Placed::Fullscreen(_) | Placed::Window) {
                *place = Placed::Nothing("Switched off".to_owned());
            }
        }
        // Without the driver, or while it is still looked for, a DeckLink screen plays nowhere.
        let driver = &running.home_state.decklink.driver;
        if !matches!(driver, sp_decklink::Driver::Ready(_)) {
            for place in &mut placed {
                if matches!(place, Placed::DeckLink(_)) {
                    *place = Placed::Nothing(driver.in_words());
                }
            }
        }
        running.retired.retain(|(_, since)| since.elapsed() < Duration::from_secs(2));
        let Running { output, screen_windows, retired, context, device_id, home_state, .. } = running;
        let Some(output) = output.as_ref() else { return };

        // Windows: keep, move, open or close each one.
        let mut old = std::mem::take(screen_windows);
        let mut opened = false;
        for (screen, place) in list.iter().zip(&placed) {
            if !matches!(place, Placed::Fullscreen(_) | Placed::Window) {
                continue;
            }
            if let Some(at) = old.iter().position(|have| have.screen == screen.id) {
                let mut kept = old.remove(at);
                if kept.placed != *place {
                    match place {
                        Placed::Fullscreen(index) => {
                            // Out of full screen, onto the display, then full screen there: macOS
                            // fills the display the window is on, whatever display is asked for.
                            if let Some(monitor) = monitors.get(*index) {
                                kept.window.set_fullscreen(None);
                                kept.window.set_outer_position(monitor.position());
                            }
                            kept.window.set_fullscreen(Some(Fullscreen::Borderless(monitors.get(*index).cloned())));
                        }
                        _ => kept.window.set_fullscreen(None),
                    }
                    log_placement(&screen.name, place, &kept.window, &displays);
                    kept.placed_at = Instant::now();
                    kept.put_back = 0;
                } else if let Placed::Fullscreen(index) = place
                    && !fresh
                    && let Some(monitor) = monitors.get(*index)
                    && kept.put_back < 3
                    && kept.placed_at.elapsed() >= Duration::from_secs(2)
                    && kept.window.current_monitor().is_some_and(|on| on != *monitor)
                {
                    // Found on another display than the one it fills: put it back.
                    kept.window.set_fullscreen(None);
                    kept.window.set_outer_position(monitor.position());
                    kept.window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor.clone()))));
                    kept.placed_at = Instant::now();
                    kept.put_back += 1;
                    applog::event("screens", &format!("{}: was on another display; put back", screen.name));
                    log_placement(&screen.name, place, &kept.window, &displays);
                    kept.window.set_cursor_visible(!matches!(place, Placed::Fullscreen(_)));
                    kept.placed = place.clone();
                }
                if kept.name != screen.name {
                    kept.window.set_title(&format!("{} — SoftPresenter", screen.name));
                    kept.name = screen.name.clone();
                }
                screen_windows.push(kept);
                continue;
            }
            let attrs = Window::default_attributes().with_title(format!("{} — SoftPresenter", screen.name));
            let attrs = match place {
                // Opened on the display first, as large as it, and made full screen once it is
                // there (below): a window made full screen as it opens fills the main display on
                // macOS, whatever display is asked for, which put the audience on the operator's
                // monitor.
                Placed::Fullscreen(index) => match monitors.get(*index) {
                    Some(monitor) => attrs.with_position(monitor.position()).with_inner_size(monitor.size()),
                    None => attrs,
                },
                // A screen in a window opens on a display the operator doesn't work on when there
                // is one, so it never lands on top of the operator's own screen.
                _ => {
                    let attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0));
                    let away = monitors.iter().enumerate().find(|(index, _)| !operator.contains(index));
                    match away {
                        Some((_, monitor)) => {
                            let (at, size, scale) = (monitor.position(), monitor.size(), monitor.scale_factor());
                            let (w, h) = ((960.0 * scale) as i32, (540.0 * scale) as i32);
                            let x = at.x + (size.width as i32 - w).max(0) / 2;
                            let y = at.y + (size.height as i32 - h).max(0) / 2;
                            attrs.with_position(winit::dpi::PhysicalPosition::new(x, y))
                        }
                        None => attrs,
                    }
                }
            };
            let attrs = if quiet {
                attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
            } else if std::env::var_os("SOFTPRESENTER_SCREENS_ON_TOP").is_some() {
                // Testing aid: SOFTPRESENTER_SCREENS_ON_TOP keeps the screens' windows above
                // every other window, so a timing run on one display draws them for real (a
                // window covered by the operator's is not drawn, and is paced differently).
                attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            } else {
                attrs
            };
            let window = match el.create_window(attrs) {
                Ok(window) => Arc::new(window),
                Err(e) => {
                    home_state.notice = Some(format!("Could not open a window for \"{}\": {e}", screen.name));
                    continue;
                }
            };
            if let Placed::Fullscreen(index) = place
                && let Some(monitor) = monitors.get(*index)
            {
                window.set_outer_position(monitor.position());
                window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor.clone()))));
            }
            log_placement(&screen.name, place, &window, &displays);
            window.set_cursor_visible(!matches!(place, Placed::Fullscreen(_)));
            let surface = match context.instance.create_surface(window.clone()) {
                Ok(surface) => surface,
                Err(e) => {
                    home_state.notice = Some(format!("Could not draw to \"{}\": {e}", screen.name));
                    continue;
                }
            };
            let format = match surface_format(&surface, context.devices[*device_id].adapter()) {
                Ok(format) => format,
                Err(e) => {
                    home_state.notice = Some(e);
                    continue;
                }
            };
            let size = window.inner_size();
            output.send(OutputMsg::AddSurface { id: screen.id, surface, format, w: size.width, h: size.height });
            screen_windows.push(ScreenWindow {
                screen: screen.id,
                window,
                placed: place.clone(),
                name: screen.name.clone(),
                placed_at: Instant::now(),
                put_back: 0,
            });
            opened = true;
        }
        for gone in old {
            output.send(OutputMsg::RemoveSurface(gone.screen));
            gone.window.set_visible(false);
            retired.push((gone.window, Instant::now()));
        }
        // A new window takes the keyboard; give it back to the operator.
        if opened && !quiet {
            running.home_window.focus_window();
        }
        let Some(running) = self.running.as_mut() else { return };
        let Running {
            output,
            ndi_sent,
            identify_sent,
            home_state,
            looks_sent,
            themes_sent,
            decklink_sent,
            decklink_playing,
            ..
        } = running;
        let Some(output) = output.as_ref() else { return };

        // Themes: the audience screens drawn with a theme of their own (one that still exists).
        let themes: Vec<(Uuid, Arc<Theme>)> = list
            .iter()
            .filter(|screen| screen.kind == screens::Kind::Audience)
            .filter_map(|screen| {
                let theme = self.theme_arcs.iter().find(|theme| Some(theme.id) == screen.theme)?;
                Some((screen.id, theme.clone()))
            })
            .collect();
        let same = themes.len() == themes_sent.len()
            && themes.iter().zip(themes_sent.iter()).all(|(a, b)| a.0 == b.0 && Arc::ptr_eq(&a.1, &b.1));
        if !same {
            output.send(OutputMsg::Themes(themes.clone()));
            output.send(OutputMsg::Redraw);
            *themes_sent = themes;
        }

        // Looks: the audience screens that show the show their own way.
        let looks: Vec<(Uuid, screens::Look)> = list
            .iter()
            .filter(|screen| screen.kind == screens::Kind::Audience && !screen.look.is_default())
            .map(|screen| (screen.id, screen.look))
            .collect();
        if looks != *looks_sent {
            output.send(OutputMsg::Looks(looks.clone()));
            output.send(OutputMsg::Redraw);
            *looks_sent = looks;
        }

        // NDI: start, rename and stop sources.
        let wanted: Vec<(Uuid, String)> = list
            .iter()
            .zip(&placed)
            .filter_map(|(screen, place)| match place {
                Placed::Ndi(name) => Some((screen.id, name.clone())),
                _ => None,
            })
            .collect();
        let mut ndi_moved = false;
        for (id, name) in &wanted {
            if !ndi_sent.contains(&(*id, name.clone())) {
                output.send(OutputMsg::SetNdi { id: *id, name: Some(name.clone()) });
                ndi_moved = true;
            }
        }
        for (id, _) in ndi_sent.iter() {
            if !wanted.iter().any(|(still, _)| still == id) {
                output.send(OutputMsg::SetNdi { id: *id, name: None });
                ndi_moved = true;
            }
        }
        *ndi_sent = wanted;
        // DeckLink: start, change and stop outputs.
        let wanted: Vec<(Uuid, screens::DeckLinkOut)> = list
            .iter()
            .zip(&placed)
            .filter_map(|(screen, place)| match place {
                Placed::DeckLink(out) => Some((screen.id, out.clone())),
                _ => None,
            })
            .collect();
        for (id, out) in &wanted {
            if !decklink_sent.contains(&(*id, out.clone())) {
                output.send(OutputMsg::SetDeckLink { id: *id, wanted: Some(out.wanted()) });
                ndi_moved = true;
            }
        }
        for (id, _) in decklink_sent.iter() {
            if !wanted.iter().any(|(still, _)| still == id) {
                output.send(OutputMsg::SetDeckLink { id: *id, wanted: None });
            }
        }
        *decklink_sent = wanted;
        if ndi_moved {
            // A new source needs the current picture at once.
            output.send(OutputMsg::Redraw);
        }

        // Stage screens: which they are, the layout each is drawn with, and whether they are on.
        let stage = (
            list.iter()
                .filter(|screen| screen.kind == screens::Kind::Stage)
                .map(|screen| {
                    // Each screen picture names the audience screen it shows now.
                    let layout = stage::layout_for(&home_state.stage_layouts, screen.stage_layout);
                    (screen.id, stage::with_sources(&layout, &list))
                })
                .collect::<Vec<_>>(),
            home_state.stage_on,
        );
        let stage_changed = running.stage_sent.as_ref() != Some(&stage);
        if stage_changed {
            let screens = stage.0.iter().map(|(id, layout)| (*id, Arc::new(stage::theme_for(layout)))).collect();
            output.send(OutputMsg::Stage { screens, on: stage.1 });
            running.stage_sent = Some(stage);
        }

        // Identify Screens: every screen shows its own name, in its own colour, while it is on.
        let names: Option<Vec<(Uuid, String, sp_model::Rgba)>> = home_state.identify.then(|| {
            list.iter()
                .enumerate()
                .map(|(place, screen)| (screen.id, screen.name.clone(), screens::identify_color(place)))
                .collect()
        });
        if names != *identify_sent {
            output.send(OutputMsg::Identify(names.clone().unwrap_or_default()));
            *identify_sent = names;
        }

        // What each screen is doing, for the Screens window.
        let mut starting = false;
        let status_before = std::mem::take(&mut home_state.screen_status);
        home_state.screen_status = list
            .iter()
            .zip(&placed)
            .map(|(screen, place)| match place {
                Placed::Ndi(_) => match output.ndi_status(screen.id) {
                    output::NdiStatus::On(name, receivers) => {
                        (output::NdiStatus::sending_in_words(&name, receivers), true)
                    }
                    output::NdiStatus::Failed(reason) if reason == sp_ndi::NOT_INSTALLED => {
                        ("NDI isn't installed on this computer".to_owned(), false)
                    }
                    output::NdiStatus::Failed(reason) => (reason, false),
                    output::NdiStatus::Off => {
                        starting = true;
                        ("Starting NDI…".to_owned(), false)
                    }
                },
                Placed::DeckLink(_) => match output.decklink_status(screen.id) {
                    output::DeckLinkStatus::Playing(what) => (format!("Playing at {what}"), true),
                    output::DeckLinkStatus::Waiting(why) => (why, false),
                    output::DeckLinkStatus::Off | output::DeckLinkStatus::Starting => {
                        starting = true;
                        (screens::describe(place, &displays), false)
                    }
                },
                Placed::Fullscreen(_) | Placed::Window => (screens::describe(place, &displays), true),
                Placed::Nothing(_) => (screens::describe(place, &displays), false),
            })
            .collect();
        // A DeckLink output that was playing and has stopped (its device taken away, say) is
        // told to the operator, once, beside the Screens window's own line.
        let playing: Vec<Uuid> = list
            .iter()
            .zip(&placed)
            .zip(&home_state.screen_status)
            .filter(|((_, place), (_, shown))| matches!(place, Placed::DeckLink(_)) && *shown)
            .map(|((screen, _), _)| screen.id)
            .collect();
        for ((screen, place), (doing, _)) in list.iter().zip(&placed).zip(&home_state.screen_status) {
            if matches!(place, Placed::DeckLink(_))
                && decklink_playing.contains(&screen.id)
                && !playing.contains(&screen.id)
            {
                home_state.notice = Some(format!("{}: {doing} Trying again.", screen.name));
            }
        }
        *decklink_playing = playing;
        if home_state.screen_status != status_before {
            for panel in running.panels.iter().filter(|panel| panel.kind == Panel::Screens) {
                panel.window.request_redraw();
            }
        }
        // Starting NDI takes a moment on the output thread; look again shortly.
        if starting || ndi_moved {
            running.next_repaint = Some(Instant::now() + Duration::from_millis(400));
        }
        // A video put on (or taken off) a stage layout starts (or stops) at once.
        if stage_changed {
            let before: Vec<String> = self.slide_videos.iter().map(|(file, _)| file.clone()).collect();
            self.sync_slide_video();
            if !self.slide_videos.iter().map(|(file, _)| file).eq(before.iter()) {
                self.refresh_output();
            }
        }
    }

    /// Redraws the Edit view's picture of the slide when the slide being edited changes.
    fn update_canvas(&mut self) {
        // The theme of the song being edited, worked out before the window's state is borrowed.
        let editing_song = self.running.as_ref().and_then(|r| r.home_state.editing.as_ref()).map(|d| d.at.presentation);
        let song_theme = editing_song.map(|song| self.theme_of(song).clone());
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let editing = running.home_state.editing.as_ref().map(|draft| (draft.at, draft.slide.clone()));
        if editing == running.canvas_for {
            return;
        }
        if let Some((at, slide)) = &editing {
            // A stage layout is drawn with the stage's plain look, as the editor works with it.
            let stage_theme = (at.presentation == stage::STAGE_SLOT && running.home_state.theme_edit.is_none())
                .then(|| running.home_state.stage_layouts.get(at.slide).map(stage::editor_theme))
                .flatten();
            // A theme's layout is drawn with that theme; a slide with its presentation's.
            let theme = match (&running.home_state.theme_edit, &stage_theme) {
                (Some(edit), _) => self.themes.get(edit.index).unwrap_or(&self.theme),
                (None, Some(stage)) => stage,
                (None, None) => song_theme.as_deref().unwrap_or(&self.theme),
            };
            running.canvas = running.thumbs.canvas(&running.gpu, &mut running.egui_renderer, slide, theme);
        }
        running.canvas_for = editing;
        running.home_window.request_redraw();
    }

    /// Draws one frame of the home window and returns what the operator asked for.
    fn draw_home(&mut self) -> Vec<Command> {
        // The song in front (being edited, or chosen) may have a theme of its own.
        let in_front = self
            .running
            .as_ref()
            .and_then(|r| r.home_state.editing.as_ref())
            .map_or(self.show.selected(), |draft| draft.at.presentation);
        let front_theme = self.theme_of(in_front).clone();
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running {
            home_window,
            home_surface,
            home_config,
            home_resize,
            gpu,
            egui_ctx,
            egui_state,
            egui_renderer,
            thumbs,
            ..
        } = running;
        let mut raw_input = egui_state.take_egui_input(home_window);
        // Testing aid: SOFTPRESENTER_FIND_DRAG drags a result of the find box (see
        // `home::find_drag_events`).
        raw_input.events.extend(home::find_drag_events(egui_ctx, &mut running.home_state));
        // Where the pointer is while files are dragged over the window: asked of the system, as
        // the window library doesn't say (see file_drop.rs).
        if running.home_state.drop_hover {
            running.home_state.file_drag.pointer = file_drop_pointer(home_window, egui_ctx);
        }
        // A new size is taken on once it has stopped changing for a moment. Until then the
        // window is drawn at the size it had, which macOS stretches to fit.
        const SETTLE: Duration = Duration::from_millis(120);
        if let Some((size, changed)) = *home_resize {
            if changed.elapsed() >= SETTLE {
                home_config.width = size.width;
                home_config.height = size.height;
                output::configure(&gpu.device, home_surface, home_config);
                *home_resize = None;
            } else {
                let points = home_window.scale_factor() as f32;
                let (w, h) = (home_config.width as f32 / points, home_config.height as f32 / points);
                raw_input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)));
                home_window.request_redraw();
            }
        }
        thumbs.begin_frame();
        if running.output.as_ref().is_none_or(OutputHandle::is_finished) {
            running.home_state.output_stopped = true;
        }
        let mut commands = Vec::new();
        // How far the preview's fade has got: the same time and easing as the output's.
        let seconds = running.home_state.dissolve_seconds();
        let amount = if seconds > 0.0 {
            let t = (running.preview_changed.elapsed().as_secs_f32() / seconds).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        } else {
            1.0
        };
        // The Program preview is what the screens show: the live preview the output thread draws,
        // once it has drawn one. (The stage picture, and the start, use the window's own.)
        let live = running.output.as_ref().map(OutputHandle::live_preview).filter(|(_, drawn)| *drawn > 0);
        // A screen that could not draw what is live still shows the picture before it.
        let undrawn = running.output.as_ref().and_then(OutputHandle::undrawn);
        let preview = match live.and_then(|(front, _)| running.live_preview_ids.get(front)) {
            Some(texture) if !running.home_state.preview_stage => {
                home::Preview { now: Some(*texture), before: None, amount: 1.0, undrawn }
            }
            _ => home::Preview { now: running.preview, before: running.preview_before, amount, undrawn },
        };
        let (show, pictures) = (&self.show, &self.bin_pictures);
        let (theme_arcs, main_theme) = (&self.theme_arcs, &self.theme);
        let (songs_theme, bible_theme): (&Theme, &Theme) = (&front_theme, &self.theme_for_bible);
        running.home_state.song_templates = song_templates(songs_theme);
        // The theme the part of the window in front works with: the one being edited, the
        // Bible's on the Bible page or when a Bible slide is being edited, otherwise the songs'.
        let state = &running.home_state;
        let edited = state.theme_edit.as_ref().and_then(|edit| self.themes.get(edit.index));
        let on_bible = match &state.editing {
            Some(draft) => self.bible_slots.contains(&draft.at.presentation),
            None => state.bible_open,
        };
        let theme = edited.unwrap_or(if on_bible { bible_theme } else { songs_theme });
        let canvas = running.canvas;
        let home_state = &mut running.home_state;
        let mut full = egui_ctx.run_ui(raw_input, |ui| {
            // The toolkit may run this closure twice in one frame, and input arrives only the
            // first time, so add to the list rather than replace it.
            // A slide that shows a media item has it behind its words in its thumbnail.
            let media_for = |slide: &Slide| {
                let id = home::media_of(slide)?;
                let item = show.media().iter().find(|item| item.id == id)?;
                Some((pictures.small(id)?, item.fit))
            };
            let mut thumb = |which: ThumbOf| match which {
                ThumbOf::Slide(slide) => thumbs.get(gpu, egui_renderer, slide, media_for(slide), songs_theme),
                // A song's slide among a playlist's: drawn as that song's own theme draws it.
                ThumbOf::SongSlide(at, slide) => {
                    let own = show.presentations().get(at).and_then(|song| song.theme);
                    let theme = own.and_then(|id| theme_arcs.iter().find(|theme| theme.id == id)).unwrap_or(main_theme);
                    thumbs.get(gpu, egui_renderer, slide, media_for(slide), theme)
                }
                ThumbOf::BibleSlide(slide) => thumbs.get(gpu, egui_renderer, slide, None, bible_theme),
                ThumbOf::Layout(index, slide) => {
                    // A layout is made anew each frame; a fixed id lets its thumbnail be kept
                    // until the theme changes, when every thumbnail is dropped.
                    let mut kept = slide.clone();
                    kept.id = Uuid::from_u128(0x7068_656d_655f_6c61_796f_7574_0000_0000 + index as u128);
                    thumbs.get(gpu, egui_renderer, &kept, None, theme)
                }
                ThumbOf::Own(slide, own, again) => {
                    if again {
                        thumbs.forget(slide.id, egui_renderer);
                    }
                    thumbs.get(gpu, egui_renderer, slide, None, own)
                }
                ThumbOf::Media(index) => {
                    let item = show.media().get(index)?;
                    match pictures.small(item.id) {
                        Some(picture) => thumbs
                            .get_media(gpu, egui_renderer, item.id, picture, songs_theme)
                            .map(|texture| (texture, false)),
                        // Still being read: a plain dark tile until it is there (the window is
                        // woken then), rather than asking to be drawn again and again.
                        None => thumbs.waiting(gpu, egui_renderer).map(|texture| (texture, false)),
                    }
                }
            };
            commands.extend(home_ui(ui, show, home_state, theme, &mut thumb, preview, canvas));
        });
        egui_state.handle_platform_output(home_window, std::mem::take(&mut full.platform_output));

        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // An animation is drawn at most a frame a screen refresh apart. A window macOS isn't
        // showing does not wait for the screen, and would otherwise be drawn thousands of times
        // a second while the preview fades.
        let delay = delay.max(FRAME);
        // Testing aid: say what keeps asking the window to be drawn again.
        if std::env::var_os("SOFTPRESENTER_TRACE_REPAINT").is_some() && running.frames % 100 == 0 {
            eprintln!("frame {}: delay {delay:?}, causes {:?}", running.frames, egui_ctx.repaint_causes());
        }
        // Testing aid: redraw continuously, to check that memory stays flat under load.
        let delay = if std::env::var_os("SOFTPRESENTER_STRESS").is_some() { Duration::ZERO } else { delay };
        running.next_repaint = Instant::now().checked_add(delay);

        let target = UiTarget {
            window: home_window,
            surface: home_surface,
            config: home_config,
            renderer: egui_renderer,
            ctx: egui_ctx,
            label: "home window",
        };
        let (jobs, screen) = paint_ui(gpu, target, full);

        // Testing aid: show a slide, let the thumbnails fill in, then save what was drawn.
        running.frames += 1;
        if let Some(path) = &self.options.capture_home {
            const GO_LIVE_AT: u32 = 3;
            const CAPTURE_AT: u32 = 30;
            // A smoke run stays up this many seconds before its picture is taken and it quits.
            static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let started = *STARTED.get_or_init(Instant::now);
            let wait = std::env::var("SOFTPRESENTER_CAPTURE_AFTER").ok().and_then(|s| s.parse::<f32>().ok());
            // The picture waits for the media bin's pictures, read on their own threads.
            let waited = wait.is_none_or(|seconds| started.elapsed().as_secs_f32() >= seconds)
                && !self.bin_pictures.any_coming();
            if running.frames == GO_LIVE_AT {
                commands.push(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 2 }));
                // Testing aid: SOFTPRESENTER_CAPTURE_MEDIA=N puts the media bin's Nth item on
                // screen instead of the first (a video, to see its controls).
                let media = std::env::var("SOFTPRESENTER_CAPTURE_MEDIA").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
                commands.push(Command::ShowMedia(media));
            }
            // Testing aid: open the Settings window.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(pane) = std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES")
            {
                running.home_state.preferences_open = true;
                // Testing aid: a pane by name (integrations, or voxlive for VoxLive's own page).
                match pane.as_str() {
                    "integrations" => running.home_state.preferences_pane = 4,
                    "voxlive" | "voxlive-look" => {
                        running.home_state.preferences_pane = 4;
                        running.home_state.voxlive.open = true;
                    }
                    _ => {}
                }
            }
            // Testing aid: open Settings.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_CAPTURE_SETTINGS").is_some() {
                running.home_state.settings_open = true;
            }
            // Testing aid: arrange the first song Verse, Chorus, Verse, Chorus, and show the
            // second chorus.
            if std::env::var_os("SOFTPRESENTER_ARRANGE").is_some() {
                if running.frames == GO_LIVE_AT + 1
                    && let Some(song) = self.show.presentations().first()
                    && song.groups.len() >= 2
                {
                    let (verse, chorus) = (song.groups[0].id, song.groups[1].id);
                    let mut arrangement = song.new_arrangement();
                    arrangement.name = "Sunday".to_owned();
                    arrangement.groups = vec![verse, chorus, verse, chorus];
                    let chosen = Some(arrangement.id);
                    running.home_state.arrangement_request = Some((0, vec![arrangement], chosen));
                }
                if running.frames == GO_LIVE_AT + 4
                    && let Some(song) = self.show.presentations().first()
                {
                    let chorus = song.groups.get(1).map(|group| group.id);
                    let in_chorus =
                        |slide: &usize| song.slide(*slide).is_some_and(|(group, _)| Some(group.id) == chorus);
                    if let Some(step) = song.order().iter().rposition(in_chorus) {
                        commands.push(Command::GoToStep { presentation: 0, step });
                    }
                }
            }
            // Testing aid: show a library, by its name.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(name) = std::env::var("SOFTPRESENTER_LIBRARY")
            {
                let state = &mut running.home_state;
                state.library_shown = state.libraries.iter().find(|library| library.name == name).map(|l| l.id);
            }
            // Testing aid: play a song, by its place, on the Announcements layer.
            if running.frames == GO_LIVE_AT + 2
                && let Some(song) = std::env::var("SOFTPRESENTER_ANNOUNCE").ok().and_then(|s| s.parse().ok())
            {
                running.home_state.announcement_request = Some(Some(announcements::Announce::Song(song)));
            }
            // Testing aid: pick out songs, media items or the playlist in front's items by their
            // places ("songs:0,2", "media:0,1" or "items:1,3"), as Command-clicks on them do; then
            // the Delete menu item (or key) asks.
            if running.frames == 2
                && let Ok(pick) = std::env::var("SOFTPRESENTER_PICK")
                && let Some((list, places)) = pick.split_once(':')
            {
                let places: Vec<usize> = places.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                let state = &mut running.home_state;
                if list == "media" {
                    state.picked_media = places;
                    state.picking = home::Picking::Media;
                } else if list == "items" {
                    state.item_picks.anchor = places.last().copied();
                    state.item_picks.places = places;
                    state.item_picks.made_in(&state.services);
                    state.picking = home::Picking::Items;
                } else {
                    state.picked_songs = places;
                    state.picking = home::Picking::Songs;
                }
            }
            // Testing aid: open an item of the playlist in front, by its place, as a click on it
            // in the left column does.
            if running.frames == GO_LIVE_AT + 3
                && let Some(item) = std::env::var("SOFTPRESENTER_OPEN_ITEM").ok().and_then(|s| s.parse::<usize>().ok())
            {
                let state = &mut running.home_state;
                state.playlist_open = true;
                let key = match state.services.front().items.get(item) {
                    Some(service::Item::Song { key, .. }) => Some(key.clone()),
                    // A picture or video, a media playlist or a placeholder opens as a click on
                    // it does: chosen, and shown with the rest of the playlist.
                    Some(
                        service::Item::Media { .. }
                        | service::Item::MediaPlaylist { .. }
                        | service::Item::Placeholder { .. }
                        | service::Item::Passage(_),
                    ) => {
                        state.services.chosen = Some(item);
                        state.slides_jump_to = Some(item);
                        state.bible_open = false;
                        None
                    }
                    _ => None,
                };
                if let Some(at) = key.and_then(|key| studio::playlist_song(&self.show, state, &key)) {
                    state.services.chosen = Some(item);
                    state.slides_jump_to = Some(item);
                    state.bible_open = false;
                    commands.push(Command::Select(at));
                }
            }
            // Testing aid: SOFTPRESENTER_CAPTURE_SLIDE=N puts the Nth slide of the song chosen
            // (the one SOFTPRESENTER_OPEN_ITEM opened, say) on screen, in place of the sample's.
            if running.frames == GO_LIVE_AT + 6
                && let Some(slide) =
                    std::env::var("SOFTPRESENTER_CAPTURE_SLIDE").ok().and_then(|s| s.parse::<usize>().ok())
            {
                let presentation = self.show.selected();
                commands.push(Command::GoTo(sp_engine::SlideRef { presentation, slide: slide.saturating_sub(1) }));
            }
            // Testing aid: SOFTPRESENTER_CAPTURE_STEP=N puts the song chosen on screen at the Nth
            // place of the order it is sung in (a chorus sung the second time, say).
            if running.frames == GO_LIVE_AT + 6
                && let Some(step) =
                    std::env::var("SOFTPRESENTER_CAPTURE_STEP").ok().and_then(|s| s.parse::<usize>().ok())
            {
                let presentation = self.show.selected();
                commands.push(Command::GoToStep { presentation, step: step.saturating_sub(1) });
            }
            // Testing aid: SOFTPRESENTER_LOOK_AT=NAME has the Program preview look at the screen of
            // that name, as its menu does.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(name) = std::env::var("SOFTPRESENTER_LOOK_AT")
                && let Some(screen) = running.home_state.screens.iter().find(|screen| screen.name == name)
            {
                let (id, stage) = (screen.id, screen.kind == screens::Kind::Stage);
                running.home_state.preview_screen = Some(id);
                running.home_state.preview_stage = stage;
            }
            // Testing aid: SOFTPRESENTER_START_TIMER=1 starts the first timer, for a picture of a
            // running timer in the top bar.
            if running.frames == GO_LIVE_AT + 2
                && std::env::var_os("SOFTPRESENTER_START_TIMER").is_some()
                && let Some(timer) = running.home_state.timers.first_mut()
            {
                timer.start(Instant::now());
            }
            // Testing aid: list the playlist in front in the left column.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_PLAYLIST").is_some() {
                running.home_state.playlist_open = true;
            }
            // Testing aid: SOFTPRESENTER_CAPTURE_CHOOSER=1 opens the chooser of playlists and
            // libraries in the left column.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_CAPTURE_CHOOSER").is_some() {
                running.home_state.chooser_open = true;
            }
            // Testing aid: open the Edit view on the stage layouts, with the front object picked;
            // SOFTPRESENTER_CAPTURE_STAGE_LAYOUTS=N picks the Nth from the front (from 0), and
            // =none nothing.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(pick) = std::env::var("SOFTPRESENTER_CAPTURE_STAGE_LAYOUTS")
            {
                running.home_state.edit_kind_request = Some(home::EditKind::Stage);
                running.home_state.stage_edit =
                    (0, if pick == "none" { None } else { Some(pick.parse().unwrap_or(0)) });
            }
            // Testing aid: show the stage picture in the Program preview.
            if running.frames == GO_LIVE_AT + 2 && std::env::var("SOFTPRESENTER_PREVIEW").as_deref() == Ok("stage") {
                running.home_state.preview_stage = true;
            }
            // Testing aid: open a tab under the Program panel, and show the first message.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(tab) = std::env::var("SOFTPRESENTER_LIVE_TAB")
            {
                let state = &mut running.home_state;
                state.live_tab = match tab.as_str() {
                    "messages" => 1,
                    "props" => 2,
                    "macros" => 3,
                    "stage" => 4,
                    "audio" => 5,
                    "voxlive" => 6,
                    _ => 0,
                };
                if let Some(first) = state.messages.first() {
                    state.message_selected = Some(first.id);
                    state.message_live = Some((first.id, Instant::now()));
                }
                if let Some(first) = state.props.first() {
                    state.props_live.push((first.id, Instant::now()));
                }
                if let Some(first) = state.timers.first_mut() {
                    state.timer_open = Some(first.id);
                    first.start(Instant::now() - Duration::from_secs(83));
                }
            }
            // Testing aid: open the library search with these words typed.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(words) = std::env::var("SOFTPRESENTER_SEARCH")
            {
                running.home_state.library_search = Some((words, 0));
            }
            // Testing aid: open the Reflow editor on the first song.
            if running.frames == GO_LIVE_AT + 2
                && std::env::var_os("SOFTPRESENTER_REFLOW").is_some()
                && let Some(song) = self.show.presentations().first()
            {
                running.home_state.reflow = Some((0, sp_model::to_song_text(song)));
            }
            // Testing aid: show the Bible page.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_CAPTURE_BIBLE").is_some() {
                running.home_state.bible_open = true;
                // Testing aid: look a reference up as if it had been typed.
                if let Ok(reference) = std::env::var("SOFTPRESENTER_BIBLE_LOOKUP") {
                    running.home_state.bible.lookup = reference;
                    running.home_state.bible.look_up(None);
                }
                // Testing aid: as if this had just been typed in Scripture Lookup, without Return.
                if let Ok(typed) = std::env::var("SOFTPRESENTER_BIBLE_TYPING") {
                    running.home_state.bible.lookup = typed;
                    running.home_state.bible.typed_at = Some(0.0);
                    running.home_state.bible.focus_lookup = true;
                }
                // Testing aid: open the Options pop-up, or with "bibles" the Bibles window, with
                // "language" the Аа menu of the language the page is read in, or with "history"
                // the History side of the left column.
                if let Ok(pane) = std::env::var("SOFTPRESENTER_BIBLE_OPTIONS") {
                    if pane == "bibles" {
                        running.home_state.bible.options.manage_open = true;
                    } else if pane == "language" {
                        running.home_state.bible.open_read_in = true;
                    } else if pane == "history" {
                        running.home_state.bible.side = home::BibleSide::History;
                    } else {
                        running.home_state.bible.options.open = true;
                    }
                }
            }
            // Testing aid: as if Command-N had been pressed.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_NEW_SONG").is_some() {
                running.home_state.new_song = true;
            }
            // Testing aid: show the slides in another view.
            if running.frames == GO_LIVE_AT + 2 {
                match std::env::var("SOFTPRESENTER_SLIDE_VIEW").as_deref() {
                    Ok("easy") => running.home_state.slide_view = home::SlideView::Easy,
                    Ok("table") => running.home_state.slide_view = home::SlideView::Table,
                    _ => {}
                }
            }
            // Testing aid: open the Theme Editor, optionally with its first box selected.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(what) = std::env::var("SOFTPRESENTER_CAPTURE_THEMES")
            {
                running.home_state.theme_request = Some(home::ThemeRequest::Edit(0));
                running.home_state.inspector.tab = inspector::InspectorTab::Text;
                self.capture_selects_box = what == "box";
            }
            if running.frames == GO_LIVE_AT + 4
                && self.capture_selects_box
                && let Some(draft) = running.home_state.editing.as_mut()
            {
                draft.selected = draft.slide.objects.first().map(|object| object.id);
            }
            // Testing aid: as if the theme's name and its first template's name had been
            // clicked to rename them.
            if running.frames == GO_LIVE_AT + 4
                && std::env::var("SOFTPRESENTER_CAPTURE_THEMES").as_deref() == Ok("rename")
                && let Some(edit) = running.home_state.theme_edit.as_ref()
            {
                let template = edit.theme.templates.first().map(|t| t.name.clone()).unwrap_or_default();
                running.home_state.renaming_theme = Some(edit.theme.name.clone());
                running.home_state.renaming_template = Some((0, template));
            }
            // Testing aid: also open the slide editor, to check how it looks.
            // Testing aid: `selected` edits the first slide of the song open in the window (once
            // SOFTPRESENTER_OPEN_ITEM has opened it), with its top object selected; otherwise the
            // first song's third slide.
            // Testing aid: SOFTPRESENTER_CAPTURE_EDITOR=props opens the Edit view on the props
            // instead (on a prop made for the picture when there is none).
            let on_props = std::env::var("SOFTPRESENTER_CAPTURE_EDITOR").as_deref() == Ok("props");
            if running.frames == GO_LIVE_AT + 2 && on_props {
                let state = &mut running.home_state;
                if state.props.is_empty() {
                    state.props.push(props::Prop::new("Logo".to_owned()));
                }
                state.edit_kind_request = Some(home::EditKind::Props);
            }
            let picked = std::env::var("SOFTPRESENTER_CAPTURE_EDITOR").as_deref() == Ok("selected");
            if running.frames == GO_LIVE_AT + if picked { 6 } else { 2 }
                && std::env::var_os("SOFTPRESENTER_CAPTURE_EDITOR").is_some()
                && !on_props
            {
                let at = if picked {
                    sp_engine::SlideRef { presentation: self.show.selected(), slide: 0 }
                } else {
                    sp_engine::SlideRef { presentation: 0, slide: 2 }
                };
                running.home_state.editing = EditDraft::of(&self.show, at);
                if let Some(draft) = running.home_state.editing.as_mut() {
                    draft.selected = if picked { draft.slide.objects.last() } else { draft.slide.objects.first() }
                        .map(|object| object.id);
                    // Testing aid: as if the box had been double-clicked.
                    if std::env::var("SOFTPRESENTER_CAPTURE_EDITOR").as_deref() == Ok("typing") {
                        draft.typing = draft.selected;
                        draft.typing_started = true;
                    }
                }
                // Testing aid: put a picture from a file on the slide, as Picture… does.
                if let Some(path) = std::env::var_os("SOFTPRESENTER_CAPTURE_PICTURE") {
                    match images::import(std::path::Path::new(&path)) {
                        Ok(picture) => running.home_state.picture_to_add.push(picture),
                        Err(problem) => eprintln!("{problem}"),
                    }
                }
                // Testing aid: give the selected object builds, to see the Build tab filled in.
                if std::env::var_os("SOFTPRESENTER_CAPTURE_BUILD").is_some()
                    && let Some(object) = running.home_state.editing.as_mut().and_then(|d| d.selected_object_mut())
                {
                    let fly = sp_model::Build { on_click: true, ..sp_model::Build::of(sp_model::BuildEffect::Fly) };
                    object.build_in = Some(fly);
                    object.build_out = Some(sp_model::Build::of(sp_model::BuildEffect::Dissolve));
                }
                // Testing aids: show the grid, and zoom in.
                running.home_state.edit_grid = std::env::var_os("SOFTPRESENTER_EDIT_GRID").is_some();
                if let Some(zoom) = std::env::var("SOFTPRESENTER_EDIT_ZOOM").ok().and_then(|z| z.parse().ok()) {
                    running.home_state.edit_zoom = zoom;
                }
                // Testing aid: which tab of the inspector to show.
                running.home_state.inspector.tab = match std::env::var("SOFTPRESENTER_INSPECTOR_TAB").as_deref() {
                    Ok("shape") => inspector::InspectorTab::Shape,
                    Ok("build") => inspector::InspectorTab::Build,
                    _ => inspector::InspectorTab::Text,
                };
            }
            // Testing aid: SOFTPRESENTER_RENAME=WHAT starts retyping a name where it is shown, as
            // a double click on it does, for a picture of it being retyped beside one without:
            // playlist, library, song (the one chosen), header, theme and template (in the
            // Theme Editor), stage-layout, group and object (in the slide editor), message,
            // prop or macro; the first of each, or the one open.
            if running.frames == GO_LIVE_AT + 8
                && let Ok(what) = std::env::var("SOFTPRESENTER_RENAME")
            {
                let state = &mut running.home_state;
                state.rename_started = true;
                let first = |names: Vec<(sp_model::Uuid, String)>| names.into_iter().next();
                match what.as_str() {
                    "playlist" => {
                        state.chooser_open = true;
                        state.renaming_service = Some(state.services.front().name.clone());
                    }
                    "library" => {
                        state.chooser_open = true;
                        state.renaming_library =
                            first(state.libraries.iter().map(|l| (l.id, l.name.clone())).collect());
                    }
                    "song" => {
                        state.renaming_song =
                            self.show.presentations().get(self.show.selected()).map(|s| s.title.clone());
                    }
                    "header" => {
                        let items = &state.services.front().items;
                        let at = items.iter().position(|item| matches!(item, service::Item::Header { .. }));
                        state.renaming_header = at.map(|at| (at, items[at].title().to_owned()));
                    }
                    "theme" => state.renaming_theme = state.theme_edit.as_ref().map(|edit| edit.theme.name.clone()),
                    "template" => {
                        let name = state.theme_edit.as_ref().and_then(|edit| edit.theme.templates.first());
                        state.renaming_template = name.map(|template| (0, template.name.clone()));
                    }
                    "stage-layout" => {
                        state.stage_rename = state.stage_layouts.first().map(|layout| (0, layout.name.clone()));
                        state.stage_rename_started = true;
                    }
                    "group" => {
                        let song = state
                            .editing
                            .as_ref()
                            .and_then(|draft| self.show.presentations().get(draft.at.presentation));
                        state.renaming_group =
                            song.and_then(|song| song.groups.first()).map(|group| (0, group.name.clone()));
                    }
                    "object" => {
                        if let Some(draft) = state.editing.as_mut() {
                            draft.renaming = draft.slide.objects.last().map(|object| (object.id, object.name.clone()));
                        }
                    }
                    "message" => {
                        state.message_rename = first(state.messages.iter().map(|m| (m.id, m.name.clone())).collect())
                    }
                    "prop" => state.prop_rename = first(state.props.iter().map(|p| (p.id, p.name.clone())).collect()),
                    "macro" => {
                        state.macro_rename = first(state.macros.iter().map(|m| (m.id, m.name.clone())).collect())
                    }
                    _ => eprintln!("SOFTPRESENTER_RENAME: nothing called \"{what}\" to rename"),
                }
            }
            if running.frames >= CAPTURE_AT && waited && !self.capture_done {
                match capture(gpu, egui_renderer, &jobs, &screen, home_config.format, path) {
                    Ok(()) => println!("saved {}", path.display()),
                    Err(e) => {
                        eprintln!("Could not save the picture: {e}");
                        self.failed = true;
                    }
                }
                self.capture_done = true;
            }
            home_window.request_redraw();
        }
        commands
    }

    /// Opens the Screens and Settings windows when they are asked for, and closes them when
    /// they are not.
    fn sync_panels(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &running.home_state;
        running.panels.retain(|panel| panel.kind.wanted(state));
        for kind in Panel::ALL {
            if !kind.wanted(&running.home_state) || running.panels.iter().any(|panel| panel.kind == kind) {
                continue;
            }
            match open_panel_window(el, running, quiet, kind) {
                Ok(panel) => {
                    panel.window.request_redraw();
                    running.panels.push(panel);
                }
                Err(problem) => {
                    kind.close(&mut running.home_state);
                    running.home_state.notice = Some(problem);
                }
            }
        }
    }

    /// Draws one frame of a panel window and returns what was asked for in it.
    fn draw_panel(&mut self, id: WindowId) -> Vec<Command> {
        let output_on = self.show.output_enabled();
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running { panels, gpu, home_state, .. } = running;
        let Some(panel) = panels.iter_mut().find(|panel| panel.window.id() == id) else {
            return Vec::new();
        };
        let raw_input = panel.state.take_egui_input(&panel.window);
        let mut commands = Vec::new();
        let kind = panel.kind;
        let mut full = panel.ctx.run_ui(raw_input, |ui| match kind {
            Panel::Screens => commands.extend(home::screens_ui(ui, home_state, output_on)),
            Panel::Settings => home::settings_ui(ui, home_state),
        });
        panel.state.handle_platform_output(&panel.window, std::mem::take(&mut full.platform_output));
        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        panel.next_repaint = Instant::now().checked_add(delay.max(FRAME));
        let target = UiTarget {
            window: &panel.window,
            surface: &panel.surface,
            config: &panel.config,
            renderer: &mut panel.renderer,
            ctx: &panel.ctx,
            label: "panel window",
        };
        let (jobs, screen) = paint_ui(gpu, target, full);
        panel.frames += 1;
        // Testing aid: a capture run with a panel asked for also saves that window, every
        // frame once it has settled, so the last one saved is the latest.
        let ending = match kind {
            Panel::Screens => "screens.png",
            Panel::Settings => "settings.png",
        };
        if let Some(path) = &self.options.capture_home
            && panel.frames >= 2
            && let Err(e) =
                capture(gpu, &panel.renderer, &jobs, &screen, panel.config.format, &path.with_extension(ending))
        {
            eprintln!("Could not save the {} window: {e}", kind.title());
        }
        commands
    }

    /// Carries out a change asked for from a slide's right-click menu.
    fn handle_slide_request(&mut self) {
        use home::SlideRequest;
        // Slides Delete was pressed on: each song's at once, so one Undo brings them back.
        let many =
            self.running.as_mut().map(|r| std::mem::take(&mut r.home_state.slides_to_delete)).unwrap_or_default();
        let mut songs: Vec<usize> = many.iter().map(|at| at.presentation).collect();
        songs.sort_unstable();
        songs.dedup();
        for song in songs {
            let slides: Vec<usize> = many.iter().filter(|at| at.presentation == song).map(|at| at.slide).collect();
            if let Some(running) = self.running.as_mut() {
                running.home_state.slide_request =
                    Some((sp_engine::SlideRef { presentation: song, slide: 0 }, SlideRequest::DeleteMany(slides)));
            }
            self.handle_one_slide_request();
        }
        self.handle_one_slide_request();
    }

    fn handle_one_slide_request(&mut self) {
        use home::SlideRequest;
        let Some((at, request)) = self.running.as_mut().and_then(|r| r.home_state.slide_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(at.presentation).cloned() else { return };
        // Slides moved to another place: the slide on screen, the one being edited and the ones
        // picked out follow them.
        if let SlideRequest::Move { slides, to, after } = &request {
            let Some((moved, new_of_old)) = song.with_slides_moved(slides, *to, *after) else { return };
            self.remember_for_undo(at.presentation);
            self.note_song_step();
            self.show.replace_presentation(at.presentation, Arc::new(moved));
            self.show.slides_reordered(at.presentation, &new_of_old);
            if !self.unsaved.contains(&at.presentation) {
                self.unsaved.push(at.presentation);
            }
            self.save_unsaved();
            // What comes next has changed, for the stage screens.
            self.refresh_output();
            if let Some(running) = self.running.as_mut() {
                let state = &mut running.home_state;
                let follow = |place: &mut sp_engine::SlideRef| {
                    if place.presentation == at.presentation
                        && let Some(new) = new_of_old.get(place.slide)
                    {
                        place.slide = *new;
                    }
                };
                if let Some(draft) = state.editing.as_mut() {
                    follow(&mut draft.at);
                }
                state.picked_slides.iter_mut().for_each(follow);
                if let Some(selected) = state.selected_slide.as_mut() {
                    follow(selected);
                }
                running.home_window.request_redraw();
            }
            return;
        }
        // Several slides: from the last to the first, so each place still names the same slide.
        if let SlideRequest::DeleteMany(slides) = &request {
            let mut slides = slides.clone();
            slides.sort_unstable();
            slides.dedup();
            let mut changed = (*song).clone();
            for slide in slides.iter().rev() {
                match changed.without_slide(*slide) {
                    Some(fewer) => changed = fewer,
                    None => break,
                }
            }
            if changed.slide_count() == song.slide_count() {
                return;
            }
            self.remember_for_undo(at.presentation);
            self.note_song_step();
            let mut changes = self.show.replace_presentation(at.presentation, Arc::new(changed));
            for slide in slides.iter().rev() {
                changes.slides |= self.show.slides_shifted(at.presentation, *slide, false).slides;
            }
            if !self.unsaved.contains(&at.presentation) {
                self.unsaved.push(at.presentation);
            }
            self.save_unsaved();
            if changes.slides {
                self.refresh_output();
            }
            if let Some(running) = self.running.as_mut() {
                running.home_state.selected_slide = None;
                running.home_window.request_redraw();
            }
            return;
        }
        let (changed, shift) = match &request {
            SlideRequest::DeleteMany(_) | SlideRequest::Move { .. } => (None, None),
            SlideRequest::PasteAfter(slides) => (song.with_slides_after(at.slide, slides), Some(true)),
            SlideRequest::Group(name) => (song.with_group_starting(at.slide, name), None),
            SlideRequest::Label(label) => {
                let slide = song.slide(at.slide).map(|(_, slide)| Slide { label: label.clone(), ..slide.clone() });
                (slide.and_then(|slide| song.with_slide(at.slide, slide)), None)
            }
        };
        let Some(changed) = changed else { return };
        self.remember_for_undo(at.presentation);
        self.note_song_step();
        let mut changes = self.show.replace_presentation(at.presentation, Arc::new(changed));
        // As many places taken as slides were pasted.
        let added = if let SlideRequest::PasteAfter(slides) = &request { slides.len() } else { 1 };
        match shift {
            Some(false) => changes.slides |= self.show.slides_shifted(at.presentation, at.slide, false).slides,
            Some(true) => {
                for _ in 0..added {
                    changes.slides |= self.show.slides_shifted(at.presentation, at.slide + 1, true).slides;
                }
            }
            None => {}
        }
        if !self.unsaved.contains(&at.presentation) {
            self.unsaved.push(at.presentation);
        }
        self.save_unsaved();
        if changes.slides {
            self.refresh_output();
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.selected_slide = None;
            running.home_window.request_redraw();
        }
    }

    /// Carries out a change asked for from a song's right-click menu in the library, and
    /// deletes songs Delete was pressed on, one by one, as a song's own Delete does.
    fn handle_song_request(&mut self) {
        let many = self.running.as_mut().map(|r| std::mem::take(&mut r.home_state.songs_to_delete)).unwrap_or_default();
        for index in many {
            if let Some(running) = self.running.as_mut() {
                running.home_state.song_request = Some((index, home::SongRequest::Delete));
            }
            self.handle_one_song_request();
        }
        self.handle_one_song_request();
    }

    fn handle_one_song_request(&mut self) {
        use home::SongRequest;
        let Some((index, request)) = self.running.as_mut().and_then(|r| r.home_state.song_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        match request {
            SongRequest::Duplicate => {
                let mut copy = (*song).clone();
                copy.id = Uuid::new_v4();
                copy.title = format!("{} copy", song.title);
                for group in &mut copy.groups {
                    group.id = Uuid::new_v4();
                    for slide in &mut group.slides {
                        slide.id = Uuid::new_v4();
                        for object in &mut slide.objects {
                            object.id = Uuid::new_v4();
                        }
                    }
                }
                let path = bible_store::data_dir()
                    .map(|dir| dir.join("Songs").join(format!("{}.{}", copy.id, load::SONG_EXTENSION)));
                if let Some(folder) = path.as_ref().and_then(|path| path.parent()) {
                    let _ = std::fs::create_dir_all(folder);
                }
                let (id, title) = (copy.id, copy.title.clone());
                let at = self.show.add_presentation(Arc::new(copy));
                self.sources.push(path.clone());
                self.unsaved.push(at);
                self.save_unsaved();
                self.apply(vec![Command::Select(at)]);
                if let Some(running) = self.running.as_mut() {
                    running.home_state.song_keys.push(song_ids::key_for(path.as_deref(), id, &title));
                }
            }
            SongRequest::Delete => {
                // A song made in the app goes to the Trash; one opened from elsewhere stays put.
                let own = bible_store::data_dir().map(|dir| dir.join("Songs"));
                let mut problem = None;
                if let (Some(Some(path)), Some(own)) = (self.sources.get(index), own)
                    && path.starts_with(&own)
                    && path.exists()
                    && let Err(e) = sweep::to_trash(path)
                {
                    problem = Some(format!("Could not move the song to the Trash: {e}"));
                }
                if problem.is_none() {
                    if self.show.live().is_some_and(|live| live.presentation == index) {
                        self.apply(vec![Command::Clear(sp_engine::Layer::Slides)]);
                    }
                    self.unsaved.retain(|at| *at != index);
                    // A deleted song can't be brought back by Undo, which would save it again.
                    self.history.forget(index);
                    if let Some(running) = self.running.as_mut() {
                        let state = &mut running.home_state;
                        state.removed_songs.push(index);
                        state.waiting.retain(|have| *have != outside::Waiting::Song(index));
                        // Nor is it in any library any more.
                        if let Some(key) = state.song_keys.get(index).cloned() {
                            for library in &mut state.libraries {
                                let before = library.songs.len();
                                library.songs.retain(|have| *have != key);
                                state.libraries_changed |= library.songs.len() != before;
                            }
                        }
                    }
                    // Another song is shown in its place.
                    let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
                    let next =
                        (0..self.show.presentations().len()).find(|at| !removed.contains(at) && !self.app_made(*at));
                    if self.show.selected() == index
                        && let Some(next) = next
                    {
                        self.apply(vec![Command::Select(next)]);
                    }
                }
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notice = problem;
                }
            }
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
    }

    /// Saves the timers and messages when they change, takes a message off when its time is up,
    /// and tells the output what to show over the slides when that changes.
    fn sync_overlay(&mut self) {
        // Linked text showing the time or a timer keeps the preview ticking.
        let links_tick = self.show.live_slide().is_some_and(|(_, slide)| links::ticks(slide));
        // The copyright line of the song on screen, on the slides Settings says.
        let copyright_line = self.running.as_ref().and_then(|running| {
            let state = &running.home_state;
            let at = self.show.live()?;
            if self.app_made(at.presentation) {
                return None;
            }
            let song = self.show.presentations().get(at.presentation)?;
            (!song.copyright.is_empty() && state.copyright_show.shows_on(at.slide, song.slide_count()))
                .then(|| song.copyright.line(&song.title, state.ccli_licence.trim()))
        });
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &mut running.home_state;
        if std::mem::take(&mut state.timers_changed)
            && let Err(problem) = timers::save(&state.timers)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.messages_changed)
            && let Err(problem) = messages::save(&state.messages)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.props_changed)
            && let Err(problem) = props::save(&state.props)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.macros_changed)
            && let Err(problem) = macros::save(&state.macros)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.stage_layouts_changed)
            && let Err(problem) = stage::save_layouts(&state.stage_layouts)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.libraries_changed)
            && let Err(problem) = libraries::save(&state.libraries)
        {
            state.notice = Some(problem);
        }
        // Props that clear themselves, once their time is up.
        let props = &state.props;
        state.props_live.retain(|(id, since)| {
            let after = props.iter().find(|prop| prop.id == *id).and_then(|prop| prop.auto_clear);
            props.iter().any(|prop| prop.id == *id)
                && after.is_none_or(|after| since.elapsed().as_secs() < u64::from(after))
        });
        // A message that leaves by itself, once its time is up.
        if let Some((id, since)) = state.message_live
            && let Some(after) = state.messages.iter().find(|message| message.id == id).and_then(|m| m.dismiss_after)
            && since.elapsed().as_secs() >= u64::from(after)
        {
            state.message_live = None;
        }
        // What is to be shown over the slides, compared with what was sent last as it stands,
        // so nothing is copied (props are whole slides) on the many wakes when it is the same.
        let message = state
            .message_live
            .and_then(|(id, _)| state.messages.iter().find(|message| message.id == id))
            .map(|message| (&message.text, message.look));
        let stage_message = state.stage_message_live.then_some(&state.stage_message);
        let props_live = || {
            state
                .props_live
                .iter()
                .filter_map(|(id, _)| state.props.iter().find(|prop| prop.id == *id).map(|p| &p.slide))
        };
        let unchanged = running.overlay_sent.as_ref().is_some_and(|sent| {
            sent.message.as_ref().map(|(text, look)| (text, *look)) == message
                && sent.stage_message.as_ref() == stage_message
                && sent.timers == state.timers
                && sent.props.iter().eq(props_live())
                && sent.copyright == copyright_line
                && sent.test_pattern == state.test_pattern
        });
        let overlay = (!unchanged).then(|| output::Overlay {
            message: message.map(|(text, look)| (text.clone(), look)),
            stage_message: stage_message.cloned(),
            timers: state.timers.clone(),
            props: props_live().cloned().collect(),
            copyright: copyright_line,
            test_pattern: state.test_pattern,
        });
        // While a timer runs, or a message counts down to leaving, the window shows it each second.
        let ticking = state.timers.iter().any(Timer::running)
            || state.message_live.is_some()
            || state.props_live.iter().any(|(id, _)| state.props.iter().any(|p| p.id == *id && p.auto_clear.is_some()));
        if ticking {
            let soon = Instant::now() + Duration::from_millis(500);
            running.next_repaint = Some(running.next_repaint.map_or(soon, |at| at.min(soon)));
        }
        // Looking at one screen: an audience screen's picture is drawn by the output thread, as
        // that screen draws it; a stage screen's, as all stage pictures, by the window.
        let mut looked_again = false;
        if running.preview_screen_shown != state.preview_screen {
            running.preview_screen_shown = state.preview_screen;
            let audience = state.preview_screen.filter(|id| {
                state.screens.iter().any(|screen| screen.id == *id && screen.kind != screens::Kind::Stage)
            });
            if let Some(output) = &running.output {
                output.send(OutputMsg::PreviewScreen(audience));
            }
            looked_again = true;
        }
        // The stage picture in the Program preview shows the time, so it is redrawn each second;
        // switching between the audience and the stage redraws it straight away.
        let switched = running.preview_shows_stage != state.preview_stage || looked_again;
        let ticking = state.preview_stage || links_tick;
        let stage_tick = ticking && running.stage_preview_at.elapsed() >= Duration::from_secs(1);
        if ticking {
            let soon = Instant::now() + Duration::from_millis(500);
            running.next_repaint = Some(running.next_repaint.map_or(soon, |at| at.min(soon)));
        }
        let Some(overlay) = overlay else {
            if switched || stage_tick {
                self.refresh_preview(switched);
            }
            return;
        };
        let message_changed = running.overlay_sent.as_ref().is_none_or(|sent| {
            sent.message != overlay.message || sent.props != overlay.props || sent.copyright != overlay.copyright
        });
        if let Some(output) = &running.output {
            output.send(OutputMsg::Overlay(overlay.clone()));
        }
        running.overlay_sent = Some(overlay);
        if message_changed {
            self.refresh_output();
        } else if switched || stage_tick || running.preview_shows_stage {
            // The stage picture shows timers and the stage message, so any change redraws it.
            self.refresh_preview(switched);
        }
    }

    /// Carries out a choice from the menu bar.
    fn menu_action(&mut self, el: &ActiveEventLoop, action: menubar::Action) {
        use menubar::Action as A;
        if action == A::Quit {
            return self.quit(el);
        }
        if action == A::ImportSongs {
            return self.choose_songs_to_import();
        }
        if action == A::ImportLibraryFolder {
            return self.choose_library_folder_to_import();
        }
        if action == A::ImportSlides {
            return self.choose_slides_to_import();
        }
        if action == A::ExportSongs {
            let picked = self.running.as_ref().map(|r| r.home_state.picked_songs.clone()).unwrap_or_default();
            let songs = if picked.is_empty() { vec![self.show.selected()] } else { picked };
            self.export_from_window(&songs, None);
            return;
        }
        if action == A::ReloadSongs {
            let said = self.reload_all().summary();
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some(said);
            }
            return;
        }
        // Typing actions in a window of its own (Settings, say) go to that window, not the main
        // one: Paste there pasted into nothing, and Undo undid a song.
        let focused = self.focused;
        if matches!(action, A::Cut | A::Copy | A::Paste | A::PasteMatchStyle | A::SelectAll | A::Undo | A::Redo)
            && let Some(running) = self.running.as_mut()
            && let Some(panel) = running.panels.iter_mut().find(|panel| Some(panel.window.id()) == focused)
        {
            let pasted = matches!(action, A::Paste | A::PasteMatchStyle)
                .then(|| panel.state.clipboard_text())
                .flatten()
                .filter(|text| !text.is_empty());
            let command = egui::Modifiers::COMMAND;
            let events = &mut panel.state.egui_input_mut().events;
            let key = |events: &mut Vec<egui::Event>, key: egui::Key, modifiers: egui::Modifiers| {
                for pressed in [true, false] {
                    events.push(egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers });
                }
            };
            match action {
                A::Cut => events.push(egui::Event::Cut),
                A::Copy => events.push(egui::Event::Copy),
                A::Paste | A::PasteMatchStyle => {
                    if let Some(text) = pasted {
                        events.push(egui::Event::Paste(text.replace("\r\n", "\n")));
                    }
                }
                A::SelectAll => key(events, egui::Key::A, command),
                A::Undo => key(events, egui::Key::Z, command),
                _ => key(events, egui::Key::Z, command | egui::Modifiers::SHIFT),
            }
            panel.window.request_redraw();
            return;
        }
        // In the Show view and on the Bible page (which has no undo of its own), with nothing
        // being typed, Undo and Redo step through the changes made to songs.
        let in_show = self.running.as_ref().is_some_and(|r| {
            let state = &r.home_state;
            state.editing.is_none() && state.theme_edit.is_none() && !r.egui_ctx.text_edit_focused()
        });
        if in_show && matches!(action, A::Undo | A::Redo) {
            return self.undo_show(action == A::Redo);
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &mut running.home_state;
        let mut commands = Vec::new();
        // Next Slide or Previous Slide: the keys' own rule, once the rest is done.
        let mut step = None;
        // Shortcuts the views act on are handed to them as the key press they stand for.
        let mut press = |key: egui::Key, modifiers: egui::Modifiers| {
            let events = &mut running.egui_state.egui_input_mut().events;
            for pressed in [true, false] {
                events.push(egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers });
            }
        };
        let command = egui::Modifiers::COMMAND;
        // A change to the selected objects is made by the editor, in its own frame.
        let request = |which| state.editing.is_some().then_some(which);
        match action {
            A::DeselectAll
            | A::Bold
            | A::Italic
            | A::Underline
            | A::Bigger
            | A::Smaller
            | A::AlignLeft
            | A::AlignCenter
            | A::AlignRight
            | A::Justify
            | A::UpperCase
            | A::LowerCase
            | A::Capitalize
            | A::CopyStyle
            | A::PasteStyle
            | A::BringForward
            | A::BringToFront
            | A::SendBackward
            | A::SendToBack
            | A::Lock
            | A::UnlockAll => {
                use home::EditRequest as E;
                state.edit_request = request(match action {
                    A::DeselectAll => E::DeselectAll,
                    A::Bold => E::Bold,
                    A::Italic => E::Italic,
                    A::Underline => E::Underline,
                    A::Bigger => E::Bigger,
                    A::Smaller => E::Smaller,
                    A::AlignLeft => E::Align(sp_model::Align::Left),
                    A::AlignCenter => E::Align(sp_model::Align::Center),
                    A::AlignRight => E::Align(sp_model::Align::Right),
                    A::Justify => E::Align(sp_model::Align::Justify),
                    A::UpperCase => E::UpperCase,
                    A::LowerCase => E::LowerCase,
                    A::Capitalize => E::Capitalize,
                    A::CopyStyle => E::CopyStyle,
                    A::PasteStyle => E::PasteStyle,
                    A::BringForward => E::BringForward,
                    A::BringToFront => E::BringToFront,
                    A::SendBackward => E::SendBackward,
                    A::SendToBack => E::SendToBack,
                    A::Lock => E::Lock,
                    _ => E::UnlockAll,
                });
            }
            A::InsertText => state.edit_request = Some(home::EditRequest::AddText),
            A::AlignObjects(edge) => state.edit_request = Some(home::EditRequest::AlignObjects(edge)),
            A::Flip(across) => state.edit_request = Some(home::EditRequest::Flip(across)),
            A::NewWindowedOutput => {
                // A new audience screen in a window of its own.
                let name = screens::next_name(&state.screens, screens::Kind::Audience);
                state.screens.push(screens::Screen::new(name, screens::Output::Window));
                state.screens_changed = true;
            }
            A::InsertShape(at) => {
                if let Some(shape) = sp_model::Shape::ALL.get(usize::from(at)) {
                    state.edit_request = Some(home::EditRequest::AddShape(*shape));
                }
            }
            A::InsertPicture => state.picture_request = state.editing.is_some() && state.theme_edit.is_none(),
            A::Quit => {}
            A::Settings => state.preferences_open = true,
            A::InstallCommandLine => {
                state.notice = Some(match control::install_command() {
                    Ok(link) => format!(
                        "Installed the sp command at {} (and softpresenter beside it). In Terminal, try: sp status",
                        link.display()
                    ),
                    Err(problem) => problem,
                });
            }
            A::ConfigureScreens => state.settings_open = true,
            A::SetUp => state.setup = Some(setup::Setup::new()),
            A::StageLayouts => state.edit_kind_request = Some(home::EditKind::Stage),
            A::NewSong => state.new_song = true,
            // Handled above, before the window's state is borrowed.
            A::ImportSongs | A::ImportLibraryFolder | A::ImportSlides | A::ReloadSongs | A::ExportSongs => {}
            A::NewSlide => {
                let song = match (&state.editing, &state.theme_edit) {
                    (Some(draft), None) => draft.at.presentation,
                    _ => self.show.selected(),
                };
                if !self.bible_slots.contains(&song)
                    && song != self.voxlive_slot
                    && song < self.show.presentations().len()
                {
                    state.add_slide = Some(song);
                }
            }
            A::CloseWindow => {
                // The front-most thing that can close: the Screens window, a question, or a
                // screen's own window, which switches that screen off.
                if state.settings_open {
                    state.settings_open = false;
                    state.identify = false;
                } else if state.alert_open() {
                    state.confirm_delete_template = None;
                    state.confirm_delete_theme = None;
                    state.confirm_delete_songs.clear();
                    state.confirm_delete_media.clear();
                    state.confirm_delete_slides.clear();
                } else if let Some(screen) =
                    running.screen_windows.iter().find(|have| Some(have.window.id()) == self.focused).map(|w| w.screen)
                    && let Some(closed) = state.screens.iter_mut().find(|have| have.id == screen)
                {
                    closed.output = screens::Output::Off;
                    state.screens_changed = true;
                }
            }
            A::Undo => press(egui::Key::Z, command),
            A::Redo => press(egui::Key::Z, command | egui::Modifiers::SHIFT),
            A::Duplicate => press(egui::Key::D, command),
            A::SelectAll => press(egui::Key::A, command),
            A::Delete => press(egui::Key::Delete, egui::Modifiers::NONE),
            A::Cut => running.egui_state.egui_input_mut().events.push(egui::Event::Cut),
            A::Copy => running.egui_state.egui_input_mut().events.push(egui::Event::Copy),
            A::Paste | A::PasteMatchStyle => {
                if let Some(text) = running.egui_state.clipboard_text().filter(|text| !text.is_empty()) {
                    running.egui_state.egui_input_mut().events.push(egui::Event::Paste(text.replace("\r\n", "\n")));
                }
            }
            A::ShowView => state.view_request = Some(0),
            A::EditView => state.view_request = Some(1),
            // Control-B: the Bible page, with the cursor in Scripture Lookup and its words
            // selected, ready for the next reference.
            A::BibleView => {
                state.view_request = Some(2);
                state.bible.focus_lookup = true;
            }
            A::ThemesView => state.view_request = Some(3),
            A::Find => state.library_search = Some((String::new(), 0)),
            A::ReflowView => {
                let song = self.show.selected();
                if !self.bible_slots.contains(&song)
                    && song != self.voxlive_slot
                    && let Some(presentation) = self.show.presentations().get(song)
                {
                    state.editing = None;
                    state.bible_open = false;
                    state.reflow = Some((song, sp_model::to_song_text(presentation)));
                }
            }
            A::TimersTab => state.live_tab = 0,
            A::MessagesTab => state.live_tab = 1,
            A::PropsTab => state.live_tab = 2,
            A::MacrosTab => state.live_tab = 3,
            A::StageTab => state.live_tab = 4,
            A::AudioTab => state.live_tab = 5,
            A::MediaBin => state.media_hidden = !state.media_hidden,
            A::NextBackground | A::PreviousBackground => {
                // The next (or last) picture in the media bin, round to the start (or end).
                let count = self.show.media().len();
                if count > 0 {
                    let at = match (self.show.live_media(), action == A::NextBackground) {
                        (Some(live), true) => (live + 1) % count,
                        (Some(live), false) => (live + count - 1) % count,
                        (None, true) => 0,
                        (None, false) => count - 1,
                    };
                    commands.push(Command::ShowMedia(at));
                }
            }
            A::ShowLiveSong => {
                // The song on screen, chosen in the Show view.
                if let Some(live) = self.show.live() {
                    commands.push(Command::Select(live.presentation));
                    state.editing = None;
                    state.bible_open = false;
                }
            }
            A::ClearAudio => commands.push(Command::Clear(sp_engine::Layer::Audio)),
            A::ClearAnnouncements => commands.push(Command::Clear(sp_engine::Layer::Announcements)),
            A::ClearProps => commands.push(Command::Clear(sp_engine::Layer::Props)),
            A::ClearMessages => commands.push(Command::Clear(sp_engine::Layer::Messages)),
            A::GridView => state.slide_view = home::SlideView::Grid,
            A::EasyView => state.slide_view = home::SlideView::Easy,
            A::TableView => state.slide_view = home::SlideView::Table,
            A::NextSlide | A::PreviousSlide => step = Some(action == A::NextSlide),
            A::ClearAll => commands.push(Command::ClearAll),
            A::ClearSlide => commands.push(Command::Clear(sp_engine::Layer::Slides)),
            A::ClearMedia => commands.push(Command::Clear(sp_engine::Layer::Media)),
            A::Audience => commands.push(Command::SetOutputEnabled(!self.show.output_enabled())),
            A::Stage => state.stage_on = !state.stage_on,
            A::Continuous => state.continuous = !state.continuous,
            A::TestBars | A::TestGrid | A::TestWhite => {
                let pattern = match action {
                    A::TestBars => props::TestPattern::ColorBars,
                    A::TestGrid => props::TestPattern::Grid,
                    _ => props::TestPattern::White,
                };
                state.test_pattern = if state.test_pattern == Some(pattern) { None } else { Some(pattern) };
            }
            A::IdentifyScreens => state.identify = !state.identify,
            A::KeysOnButtons => state.keys_on_buttons = !state.keys_on_buttons,
            A::Website | A::WhatsNew | A::ReportIssue => {
                let page = match action {
                    A::WhatsNew => "https://github.com/esraell/SoftPresenter/commits/master",
                    A::ReportIssue => "https://github.com/esraell/SoftPresenter/issues/new",
                    _ => "https://github.com/esraell/SoftPresenter#readme",
                };
                if let Err(e) = std::process::Command::new("open").arg(page).spawn() {
                    state.notice = Some(format!("Could not open the web page: {e}"));
                }
            }
        }
        running.home_window.request_redraw();
        if !commands.is_empty() {
            self.apply(commands);
        }
        if let Some(forward) = step {
            self.step_keys(forward);
        }
    }

    /// A key that runs the show: Next and Previous act on the open item (see `step_keys`); the
    /// rest go to the show as they are.
    fn show_key(&mut self, command: Command) {
        match command {
            Command::Next => {
                self.step_keys(true);
            }
            Command::Previous => {
                self.step_keys(false);
            }
            other => self.apply(vec![other]),
        }
    }

    /// Ticks the menu bar's items that are on now: the view, the slide view, the audience
    /// output and Identify Screens.
    fn tick_menu(&mut self) {
        #[cfg(target_os = "macos")]
        if let (Some(menu), Some(running)) = (self.menu.as_mut(), self.running.as_ref()) {
            use menubar::Action as A;
            let state = &running.home_state;
            let view = if state.theme_edit.is_some() {
                A::ThemesView
            } else if state.edit_kind().is_some() {
                A::EditView
            } else if state.bible_open {
                A::BibleView
            } else {
                A::ShowView
            };
            let slide_view = match state.slide_view {
                home::SlideView::Grid => A::GridView,
                home::SlideView::Easy => A::EasyView,
                home::SlideView::Table => A::TableView,
            };
            let output_on = self.show.output_enabled();
            menu.set_editing(state.editing.is_some());
            menu.set_checked(|action| match action {
                A::Audience => output_on,
                A::Stage => state.stage_on,
                A::Continuous => state.continuous,
                A::TestBars => state.test_pattern == Some(props::TestPattern::ColorBars),
                A::TestGrid => state.test_pattern == Some(props::TestPattern::Grid),
                A::TestWhite => state.test_pattern == Some(props::TestPattern::White),
                A::TimersTab => state.live_tab == 0,
                A::MessagesTab => state.live_tab == 1,
                A::PropsTab => state.live_tab == 2,
                A::MacrosTab => state.live_tab == 3,
                A::StageTab => state.live_tab == 4,
                A::AudioTab => state.live_tab == 5,
                A::MediaBin => !state.media_hidden,
                A::IdentifyScreens => state.identify,
                A::KeysOnButtons => state.keys_on_buttons,
                _ => action == view || action == slide_view,
            });
        }
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        self.save_unsaved();
        self.keep_held_aside();
        if let Some(output) = self.running.as_mut().and_then(|r| r.output.take()) {
            output.shutdown();
        }
        el.exit();
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match self.start(el) {
            Ok(Some(mut running)) => {
                // The Bible page and the small choices are as they were left.
                if let Some(page) = service::load_bible_page() {
                    running.home_state.bible.apply_settings(&page);
                }
                running.home_state.bible.history = service::load_bible_history();
                // The language a session of the Bible view starts in, read once on a thread.
                running.home_state.bible.system_language.start();
                self.bible_page_saved = running.home_state.bible.choices();
                let prefs = service::load_prefs();
                running.home_state.apply_prefs(&prefs);
                // The setup opens once on every Mac that hasn't seen it yet, a fresh install or
                // one set up before there was a setup. Testing aid: SOFTPRESENTER_SETUP=1 shows
                // it in a quiet run too.
                if (!prefs.setup_done && !self.quiet()) || std::env::var_os("SOFTPRESENTER_SETUP").is_some() {
                    running.home_state.setup = Some(setup::Setup::new());
                }
                // The audience screens start off when Settings says so.
                let start_off = prefs.start_audience_off;
                // The screens, as they were left. The first start after screens could be
                // listed makes the list from how the output was set up before: one screen
                // placed automatically, and one sent over NDI if NDI was on.
                let quiet = self.quiet();
                running.home_state.screens = screens::load().unwrap_or_else(|| {
                    let ndi = (prefs.ndi_on && running.home_state.ndi_available).then_some(prefs.ndi_name.as_str());
                    let made = screens::starting(ndi);
                    if !quiet {
                        let _ = screens::save(&made);
                    }
                    made
                });
                let opening = prefs.start_with.clone();
                self.prefs_saved = Some(prefs);
                running.home_state.timers = timers::load();
                running.home_state.messages = messages::load();
                running.home_state.props = props::load();
                running.home_state.macros = macros::load();
                running.home_state.libraries = libraries::load();
                running.home_state.media_playlists = std::mem::take(&mut self.media_playlists);
                running.home_state.stage_layouts = stage::load_layouts();
                running.home_state.audio = audio::load();
                // Start-up belongs in macros: what Settings once said to play when the app opened
                // becomes a macro that plays it (and runs below, as it did), said once.
                if let Some(step) = macros::from_opening_choice(&mut running.home_state.macros, &opening) {
                    let saved = macros::save(&running.home_state.macros);
                    let what = describe_opening(&self.show, &running.home_state, &step);
                    let mut said = format!(
                        "What played when SoftPresenter opened is now the macro \u{201c}{}\u{201d} in the Macros tab, \
                         with Run When SoftPresenter Opens ticked. It plays {what} on the Announcements layer.",
                        macros::OPENING_MACRO
                    );
                    if matches!(opening, service::StartWith::MediaPlaylist { .. }) {
                        said.push_str(" That layer is over the slides, where the media playlist played behind them.");
                    }
                    if let Err(problem) = saved {
                        said.push_str(&format!(" {problem}"));
                    }
                    applog::event("macros", &said);
                    running.home_state.notice = Some(said);
                }
                // Macros set to run when SoftPresenter starts.
                let startup: Vec<sp_model::ShowAction> = running
                    .home_state
                    .macros
                    .iter()
                    .filter(|m| m.on_startup)
                    .flat_map(|m| m.actions.iter().cloned())
                    .collect();
                running.home_state.actions_to_run.extend(startup);
                // A display asked for on the command line takes the first screen, this run only.
                if let Some(index) = self.options.output_display
                    && let Some(display) = display_list(el.available_monitors(), el.primary_monitor())
                        .into_iter()
                        .find(|d| d.index == index)
                    && let Some(first) = running.home_state.screens.first_mut()
                {
                    first.output = screens::Output::Display(display.name);
                }
                // Testing aid: also send over NDI under this name, this run only.
                if let Ok(name) = std::env::var("SOFTPRESENTER_NDI") {
                    let screen = screens::Screen::new("NDI".to_owned(), screens::Output::Ndi(name));
                    running.home_state.screens.push(screen);
                    running.home_state.screen_selected = running.home_state.screens.len() - 1;
                }
                // What became of the library's folder at this start (moved to Documents, or why not).
                if let Some(said) = library_folder::take_notice() {
                    running.home_state.notice = Some(said);
                }
                // Pictures, videos and leftovers nothing in the library uses go to the Trash, at
                // most once a day, on a thread of their own; the window says so if any went.
                let open_songs = self.sources.iter().flatten().map(|path| load::song_file_for(path)).collect();
                let waker = self.waker.clone();
                sweep::start(self.quiet(), open_songs, move |said| {
                    if let Some(waker) = waker {
                        let _ = waker.send_event(AppEvent::Tidied(said));
                    }
                });
                running.home_window.request_redraw();
                self.running = Some(running);
                if start_off {
                    self.apply(vec![Command::SetOutputEnabled(false)]);
                }
                self.sync_screens(el);
                // The menu bar, for a run someone is using.
                #[cfg(target_os = "macos")]
                if !self.quiet() {
                    match menubar::MenuBar::install() {
                        Ok(menu) => self.menu = Some(menu),
                        Err(problem) => eprintln!("{problem}"),
                    }
                }
                // Testing aid: start with the first slide on screen, and the first media item
                // behind it.
                if std::env::var_os("SOFTPRESENTER_GO_LIVE").is_some() {
                    self.apply(vec![Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 })]);
                }
                if std::env::var_os("SOFTPRESENTER_GO_LIVE_MEDIA").is_some() {
                    self.apply(vec![Command::ShowMedia(0)]);
                }
                // Followed folders are read (a macro run at the start may play one's media).
                self.check_folders(true);
            }
            Ok(None) => el.exit(),
            Err(message) => {
                eprintln!("{message}");
                self.failed = true;
                el.exit();
            }
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: AppEvent) {
        let id = match event {
            AppEvent::Menu(id) => id,
            // A command from `softpresenter ctl`: done now, and its outcome sent back.
            AppEvent::Control(incoming) => {
                // A wait for a change, with nothing changed yet, is held and answered later
                // (`answer_waiting`, from `about_to_wait`).
                let Some(incoming) = self.hold_wait(incoming) else { return };
                let reply = self.control(el, &incoming.request.args);
                // A command that only reads (status, a list) changes nothing worth a line: a
                // page asking every second or two would fill the log with them.
                if !control::only_reads(&incoming.request.args) {
                    applog::event(
                        "control",
                        &format!(
                            "{:?} -> {}",
                            control::shown_args(&incoming.request.args),
                            if reply.ok { "done" } else { "refused" }
                        ),
                    );
                }
                let _ = incoming.reply.send(reply);
                // A screen changed from the command line reaches the screens now, not at the
                // window's next redraw (which an idle or hidden window may not have for a while).
                self.sync_screens(el);
                self.sync_panels(el);
                return;
            }
            AppEvent::LivePreview => String::new(),
            AppEvent::DeckDone => {
                self.handle_decks();
                return;
            }
            AppEvent::PackDone => {
                self.finish_packs();
                return;
            }
            AppEvent::MediaRead => {
                self.take_media_read();
                return;
            }
            AppEvent::FoldersRead => {
                self.finish_folder_look();
                return;
            }
            AppEvent::Tidied(said) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notice = Some(said);
                    running.home_window.request_redraw();
                }
                return;
            }
            AppEvent::DeckLink => {
                self.sync_screens(el);
                for panel in self.running.iter().flat_map(|r| r.panels.iter()) {
                    panel.window.request_redraw();
                }
                return;
            }
        };
        // The output thread drew a new live preview: the window shows it.
        if id.is_empty() {
            if let Some(running) = self.running.as_ref()
                && let Some(output) = running.output.as_ref()
            {
                output.live_preview_woken();
                running.home_window.request_redraw();
                // Testing aid: say once a second how many new previews the window was woken for.
                if std::env::var_os("SOFTPRESENTER_TRACE_PREVIEW").is_some() {
                    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    static SINCE: std::sync::OnceLock<std::sync::Mutex<Instant>> = std::sync::OnceLock::new();
                    let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let since = SINCE.get_or_init(|| std::sync::Mutex::new(Instant::now()));
                    if let Ok(mut since) = since.lock()
                        && since.elapsed() >= Duration::from_secs(1)
                    {
                        let (_, drawn) = output.live_preview();
                        eprintln!("preview wakes {count} in {:.1}s, {drawn} drawn", since.elapsed().as_secs_f32());
                        COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
                        *since = Instant::now();
                    }
                }
            }
            return;
        }
        if let Some(action) = menubar::action_of(&id) {
            self.menu_action(el, action);
            self.sync_panels(el);
            for panel in self.running.iter().flat_map(|r| r.panels.iter()) {
                panel.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::ModifiersChanged(now) => self.held = now.state(),
            WindowEvent::Focused(true) => self.focused = Some(id),
            _ => {}
        }
        // A key that is a menu bar shortcut belongs to the menu, which acts on it once.
        #[cfg(target_os = "macos")]
        if self.menu.is_some()
            && let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && let winit::keyboard::PhysicalKey::Code(code) = key.physical_key
            && menubar::claims(code, self.held, self.running.as_ref().is_some_and(|r| r.home_state.editing.is_some()))
        {
            return;
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if let Some(at) = running.panels.iter().position(|panel| panel.window.id() == id) {
            let panel = &mut running.panels[at];
            match &event {
                WindowEvent::CloseRequested => {
                    panel.kind.close(&mut running.home_state);
                    running.panels.remove(at);
                    running.home_window.request_redraw();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        panel.config.width = size.width;
                        panel.config.height = size.height;
                        output::configure(&running.gpu.device, &panel.surface, &panel.config);
                    }
                    panel.window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let commands = self.draw_panel(id);
                    self.apply(commands);
                    self.sync_screens(el);
                }
                _ => {
                    if panel.state.on_window_event(&panel.window, &event).repaint {
                        panel.window.request_redraw();
                    }
                }
            }
            return;
        }
        if let Some(screen) = running.screen_windows.iter().find(|have| have.window.id() == id).map(|have| have.screen)
        {
            match event {
                WindowEvent::Resized(size) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Resized(screen, size.width, size.height));
                    }
                }
                // Closing a screen's window switches that screen off; it stays in the list.
                WindowEvent::CloseRequested => {
                    if let Some(closed) = running.home_state.screens.iter_mut().find(|have| have.id == screen) {
                        closed.output = screens::Output::Off;
                    }
                    running.home_state.screens_changed = true;
                    running.home_window.request_redraw();
                }
                // Uncovered again: a frame skipped while the window was hidden must not leave old
                // words on screen. Do not also redraw on RedrawRequested: presenting a frame makes
                // the system ask for another, which loops without end.
                WindowEvent::Occluded(false) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Redraw);
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                    if !self.held.super_key()
                        && !self.held.control_key()
                        && let Some(command) = show_key_command(&event.logical_key, &self.show)
                    {
                        let logical = event.logical_key.clone();
                        if !self.pick_key(&logical) {
                            self.show_key(command);
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        if id != running.home_window.id() {
            return;
        }
        // `/` opens the find box, from wherever the keyboard runs the show: not while something is
        // being typed, not in the Edit view (its keyboard is the editor's), not over a sheet, a
        // question or a menu. It is not passed on, so the box opens empty.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && opens_find(&key.logical_key)
            && find_key_is_free(&running.home_state, &running.egui_ctx)
            && !self.held.super_key()
            && !self.held.control_key()
            && !self.held.alt_key()
        {
            running.home_state.library_search = Some((String::new(), 0));
            running.home_window.request_redraw();
            return;
        }
        // Show keys act at once and are not passed on, so Space can never also press whatever
        // control happens to have the keyboard. Typing in a text box is left alone.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && !running.egui_ctx.text_edit_focused()
            && running.home_state.edit_kind().is_none()
            && running.home_state.new_song_sheet.is_none()
            && !running.home_state.alert_open()
            && !self.held.super_key()
            && !self.held.control_key()
            && !show_key_held_back(&running.home_state, &running.egui_ctx, &key.logical_key)
            && let Some(command) = show_key_command(&key.logical_key, &self.show)
        {
            // Escape clears the screen only when there is nothing else for it to close.
            let something_open = escape_closes_something(&running.home_state, &running.egui_ctx);
            let escape = matches!(key.logical_key, Key::Named(NamedKey::Escape));
            if !(escape && something_open) {
                let logical = key.logical_key.clone();
                if !self.pick_key(&logical) {
                    self.show_key(command);
                }
                return;
            }
        }
        // Enter puts the slide picked on screen, or the open item of the playlist from its first
        // slide, when no control has the keyboard and nothing is open over the window. Not when
        // held down, so a Return held in the find box can't go on to put what it opened on screen.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && matches!(key.logical_key, Key::Named(NamedKey::Enter))
            && !key.repeat
            && enter_key_is_free(&running.home_state, &running.egui_ctx)
            && !self.held.super_key()
            && !self.held.control_key()
            && !self.held.alt_key()
        {
            self.enter_key();
            return;
        }
        // The interface toolkit answers "repaint" to a redraw request itself. Asking for a redraw
        // in reply to a redraw never ends: the window then redraws as fast as it can for ever,
        // which is what filled the computer's memory. A redraw request is not passed to it.
        if !matches!(event, WindowEvent::RedrawRequested)
            && running.egui_state.on_window_event(&running.home_window, &event).repaint
        {
            running.home_window.request_redraw();
        }
        match event {
            // Files dragged over the window from the Finder, each told one after another; where
            // the pointer is, the window asks the system as it draws (see file_drop.rs).
            WindowEvent::HoveredFile(path) => {
                running.home_state.drop_hover = true;
                running.home_state.file_drag.add(path);
                running.home_window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                running.home_state.drop_hover = false;
                running.home_state.file_drag = file_drop::FileDrag::default();
                running.home_window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => self.file_dropped(path),
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    running.home_resize = Some((size, Instant::now()));
                }
                running.home_window.request_redraw();
            }
            // Testing aid: stop drawing the operator window after a few seconds, as macOS does
            // when it is minimised, to check that what runs by the clock goes on without it.
            WindowEvent::RedrawRequested
                if std::env::var("SOFTPRESENTER_STOP_DRAWING_AFTER")
                    .ok()
                    .and_then(|s| s.parse::<f32>().ok())
                    .is_some_and(|after| self.started.elapsed().as_secs_f32() >= after) => {}
            WindowEvent::RedrawRequested => {
                let drawing = Instant::now();
                let commands = self.draw_home();
                self.home_timing.add(drawing.elapsed());
                // A command chosen from a right-click menu in the window, done as the menu bar's.
                if let Some(action) = self.running.as_mut().and_then(|r| r.home_state.menu_asked.take()) {
                    self.menu_action(el, action);
                }
                self.home_timing.report_each_minute("window", || {
                    applog::memory_mb().map_or_else(String::new, |mb| format!("{mb} MB resident"))
                });
                // Every notice shown to the operator goes in the log too.
                let notice = self.running.as_ref().and_then(|r| r.home_state.notice.clone());
                if notice != self.notice_logged {
                    if let Some(notice) = &notice {
                        applog::event("notice", notice);
                    }
                    self.notice_logged = notice;
                }
                self.apply(commands);
                // Testing aid: drag files over the window (SOFTPRESENTER_HOVER, kept there, for a
                // picture of where they would land) or drop them on it a few frames after they
                // come over it (SOFTPRESENTER_DROP), as dragging them from the Finder does. Either
                // takes several files, separated as in PATH. SOFTPRESENTER_DROP_AT=X,Y puts the
                // pointer at that place in the window, in its points.
                let frame = self.running.as_ref().map_or(0, |r| r.frames);
                let testing =
                    std::env::var_os("SOFTPRESENTER_DROP").or_else(|| std::env::var_os("SOFTPRESENTER_HOVER"));
                if let Some(files) = testing.filter(|_| frame == 2)
                    && let Some(running) = self.running.as_mut()
                {
                    running.home_state.drop_hover = true;
                    std::env::split_paths(&files).for_each(|path| running.home_state.file_drag.add(path));
                    running.home_window.request_redraw();
                }
                if let Some(files) = std::env::var_os("SOFTPRESENTER_DROP").filter(|_| frame == 12) {
                    std::env::split_paths(&files).for_each(|path| self.file_dropped(path));
                    self.finish_drop();
                }
                // Testing aid: choose menu bar items by their ids, one every few frames, the way
                // a click on them reaches the app.
                if let Ok(ids) = std::env::var("SOFTPRESENTER_MENU") {
                    let frame = self.running.as_ref().map_or(0, |r| r.frames);
                    if let Some(id) =
                        frame.checked_sub(6).filter(|n| n % 4 == 0).and_then(|n| ids.split(',').nth((n / 4) as usize))
                    {
                        eprintln!("menu {id}");
                        self.user_event(el, AppEvent::Menu(id.to_owned()));
                    }
                }
                if let Some((at, slide)) = self.running.as_mut().and_then(|r| r.home_state.pending_edit.take()) {
                    self.apply_edit(at, slide);
                }
                self.run_actions();
                self.tick_advance();
                self.handle_audio();
                self.handle_video();
                self.handle_slide_request();
                self.handle_song_theme();
                self.handle_song_loop();
                self.handle_song_transition();
                self.handle_announcement();
                self.handle_arrangement();
                self.handle_copyright();
                self.handle_reflow();
                self.handle_media_request();
                self.handle_picture_request();
                self.handle_song_request();
                // A template chosen from a slide's menu: the slide is changed and, if it is on
                // screen, the screen too.
                if let Some((at, template)) = self.running.as_mut().and_then(|r| r.home_state.pending_template.take())
                    && let Some((_, slide)) =
                        self.show.presentations().get(at.presentation).and_then(|p| p.slide(at.slide))
                {
                    let slide = Slide { template, ..slide.clone() };
                    self.apply_edit(at, slide);
                    if self.show.live() == Some(at) {
                        self.refresh_output();
                    }
                }
                // A Bible passage just looked up goes into its slot. It is never the slot on
                // screen, so the output is untouched.
                if let Some((slot, passage)) = self.running.as_mut().and_then(|r| r.home_state.bible.pending.take()) {
                    self.show.replace_presentation(slot, Arc::new(passage));
                    // A verse double-clicked in the Bible lists goes on screen, in its chapter.
                    let go_live = self.running.as_mut().and_then(|r| r.home_state.bible.go_live.take());
                    if let Some(slide) = go_live {
                        self.apply(vec![Command::GoTo(sp_engine::SlideRef { presentation: slot, slide })]);
                    }
                    if let Some(running) = self.running.as_ref() {
                        running.home_window.request_redraw();
                    }
                }
                self.edit_undo_bookkeeping();
                self.handle_themes();
                self.handle_songs();
                self.handle_decks();
                self.handle_setup();
                // The Bible page's set-up (translations, boxes, options) is remembered the
                // moment it changes, so it is as it was left at the next start.
                if let Some(running) = self.running.as_mut() {
                    let now = running.home_state.bible.choices();
                    if now != self.bible_page_saved {
                        let page =
                            service::SavedPassage { lookup: String::new(), ..running.home_state.bible.settings() };
                        if let Err(problem) = service::save_bible_page(&page) {
                            running.home_state.notice = Some(problem);
                        }
                        self.bible_page_saved = now;
                    }
                }
                // And the passages that were on screen.
                if let Some(running) = self.running.as_mut()
                    && std::mem::take(&mut running.home_state.bible.history_changed)
                    && let Err(problem) = service::save_bible_history(&running.home_state.bible.history)
                {
                    running.home_state.notice = Some(problem);
                }
                // So are the small choices: transition, view, slide size.
                if let Some(running) = self.running.as_mut() {
                    let now = running.home_state.prefs();
                    if self.prefs_saved.as_ref() != Some(&now) && self.frames_since_start > 10 {
                        if let Err(problem) = service::save_prefs(&now) {
                            running.home_state.notice = Some(problem);
                        }
                        self.prefs_saved = Some(now);
                    }
                    self.frames_since_start = self.frames_since_start.saturating_add(1);
                }
                // The services are written as soon as they change; the file is small.
                if let Some(running) = self.running.as_mut()
                    && std::mem::take(&mut running.home_state.services.changed)
                    && let Err(problem) = service::save(&running.home_state.services)
                {
                    running.home_state.notice = Some(problem);
                }
                // Export…, from a song's or a playlist's right-click menu.
                if let Some(request) = self.running.as_mut().and_then(|r| r.home_state.export_request.take()) {
                    match request {
                        home::ExportRequest::Songs(songs) => self.export_from_window(&songs, None),
                        home::ExportRequest::Playlist(at) => self.export_from_window(&[], Some(at)),
                    }
                }
                self.finish_bible_jobs();
                // Reload or Keep mine, chosen in the bar for a file changed outside the app.
                if let Some((what, reload)) = self.running.as_mut().and_then(|r| r.home_state.waiting_choice.take()) {
                    self.choose_outside(what, reload);
                }
                self.update_canvas();
                self.sync_screens(el);
                self.sync_panels(el);
                self.sync_overlay();
                self.tick_menu();
                // Leaving the Edit view writes the songs changed there straight away (other changes
                // are written once they pause for a second).
                let editing = self.running.as_ref().is_some_and(|r| r.home_state.editing.is_some());
                if std::mem::replace(&mut self.was_editing, editing) && !editing && !self.unsaved.is_empty() {
                    self.save_unsaved();
                }
                if self.capture_done {
                    self.quit(el);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Files dropped on the window, now that all of the drop have come.
        self.finish_drop();
        // What runs by the clock is done here too, not only when the operator window draws:
        // macOS stops drawing a window that is minimised or covered, and a timed slide, a
        // slide's actions, or a message that leaves by itself must not wait for it.
        self.run_actions();
        self.tick_advance();
        self.sync_overlay();
        let video_at = self.tick_video();
        let box_video_at = self.tick_slide_video();
        let web_at = self.tick_web();
        let voxlive_at = self.tick_voxlive();
        let audio_at = self.tick_audio();
        let announce_at = self.tick_announcements();
        let cycle_at = self.tick_media_cycle();
        self.check_folders(false);
        let folders_at = self
            .running
            .as_ref()
            .filter(|r| r.home_state.media_playlists.iter().any(|p| p.folder.is_some()))
            .map(|_| self.folders_checked + Duration::from_secs(5));
        // Songs changed in the Edit view are written once typing has paused for a second.
        const SAVE_AFTER: Duration = Duration::from_secs(1);
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        if save_at.is_some_and(|at| at <= Instant::now()) {
            self.save_unsaved();
        }
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        // `status --wait`s held: answered now if anything changed, or their time is up.
        let waiting_at = self.answer_waiting();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The interface asks to be drawn again after a delay (animations); otherwise sleep
        // until then, or until a save is due.
        if running.next_repaint.is_some_and(|at| at <= Instant::now()) {
            running.next_repaint = None;
            running.home_window.request_redraw();
        }
        let mut panel_at: Option<Instant> = None;
        for panel in &mut running.panels {
            if panel.next_repaint.is_some_and(|at| at <= Instant::now()) {
                panel.next_repaint = None;
                panel.window.request_redraw();
            }
            panel_at = [panel_at, panel.next_repaint].into_iter().flatten().min();
        }
        // Once thumbnails and previews have stopped being drawn, their painter gives back the
        // GPU memory a heavy slide made it take.
        let thumbs_rest_at = running.thumbs.rest_when_settled(Instant::now());
        let wake = [
            running.next_repaint,
            panel_at,
            thumbs_rest_at,
            save_at,
            video_at,
            box_video_at,
            web_at,
            voxlive_at,
            audio_at,
            announce_at,
            cycle_at,
            folders_at,
            waiting_at,
        ];
        match wake.into_iter().flatten().min() {
            Some(at) => {
                // Everything due has just been done, so a time already past is a mistake in
                // one of the clocks above; waiting for it would wake the loop again at once,
                // round and round at full speed. A short wait keeps that from running away.
                let now = Instant::now();
                let at = if at <= now {
                    if std::env::var_os("SOFTPRESENTER_TRACE_REPAINT").is_some() {
                        applog::event("wake", "a clock asked for a wake-up already past");
                    }
                    now + Duration::from_millis(15)
                } else {
                    at
                };
                el.set_control_flow(ControlFlow::WaitUntil(at));
            }
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn main() -> ExitCode {
    // `softpresenter ctl …` (or just `softpresenter next`, `softpresenter status`…) controls
    // the running app, and nothing else: no window, no log.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ctl") => return control::run_client(args[1..].to_vec()),
        // `softpresenter decklink-check …`: lists the DeckLink devices, and with `play` sends a
        // test picture to one, from a terminal, without the app.
        Some("decklink-check") => return sp_decklink::check::run(&args[1..], "softpresenter decklink-check"),
        Some("install-cli") => {
            return match control::install_command() {
                Ok(link) => {
                    println!("Installed {} (and softpresenter beside it). Try: sp status", link.display());
                    ExitCode::SUCCESS
                }
                Err(problem) => {
                    eprintln!("{problem}");
                    ExitCode::from(1)
                }
            };
        }
        // A command always wins over a file of the same name (open that as ./name); one
        // after options, such as `sp --json status`, is a command too.
        _ if args.iter().find(|word| !word.starts_with('-')).is_some_and(|word| control::is_command(word)) => {
            return control::run_client(args);
        }
        // --json with no command after it.
        _ if args.iter().any(|word| word == "--json") => {
            eprintln!("sp: --json goes with a command, such as sp status --json. See: sp help");
            return ExitCode::from(2);
        }
        // A first word that is neither a command nor a file to open is a mistake, said so,
        // rather than handed to the app as a file it can't read.
        Some(first) if !first.starts_with('-') && !std::path::Path::new(first).exists() => {
            eprintln!("sp: no command or file called '{first}'. See: sp help");
            return ExitCode::from(2);
        }
        _ => {}
    }
    // Started through the installed command (a link outside the app), the app is opened as a
    // Mac app is, so it is named and placed properly; anything typed after it goes along.
    // `sp` alone, through any link (into the app, or to the program itself): what is on screen,
    // or the app started if nothing answers; never a second app beside one that answers.
    #[cfg(unix)]
    if args.is_empty() && control::started_through_link() {
        #[cfg(target_os = "macos")]
        let bundle = control::bundle_opened_through_link();
        #[cfg(not(target_os = "macos"))]
        let bundle: Option<PathBuf> = None;
        if let Some(code) = control::bare(bundle.as_deref()) {
            return code;
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(bundle) = control::bundle_opened_through_link() {
        let mut open = std::process::Command::new("/usr/bin/open");
        open.arg(&bundle);
        if !args.is_empty() {
            open.arg("--args").args(&args);
        }
        return match open.status() {
            Ok(status) if status.success() => ExitCode::SUCCESS,
            _ => ExitCode::from(1),
        };
    }
    // The options are read before anything is moved or changed: asking for help, or a mistyped
    // option, never moves the library.
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    // Folders kept under the project's old name are renamed to the new one, once, before the
    // log opens in its new place.
    let moved = renamed::move_old_folders();
    // On a Mac the library moves to Documents, once.
    let to_documents = library_folder::move_to_documents();
    // Songs named by path or title in files from before songs were found by id are named by
    // id, once, in whichever library folder is in use.
    let settled = bible_store::data_dir().map(|dir| song_ids::settle(&dir, true)).unwrap_or_default();
    applog::start();
    for done in &moved {
        applog::event("renamed", done);
    }
    for done in &to_documents {
        applog::event("library", done);
    }
    for done in &settled {
        applog::event("song ids", done);
    }
    // Picture objects on slides are read from the app's Images folder.
    sp_render::set_picture_loader(images::load);
    // Large font files are mapped from private copies in the app's caches folder.
    sp_render::copy_large_fonts_into_caches();
    // The operator's window draws a placeholder for a web page box with no picture yet; the
    // screens, on their own threads, never do.
    sp_render::show_placeholders(true);
    // A web page box whose page costs to open (a VoxLive screen's live display) shows its free
    // test pattern in the operator's window instead.
    sp_render::set_web_stand_in(web_stand_in);

    let (media_paths, song_paths): (Vec<PathBuf>, Vec<PathBuf>) =
        options.paths.iter().cloned().partition(|path| media::is_media(path));
    let songs = if song_paths.is_empty() {
        // The sample always has the same id, so that once it has been changed and saved the
        // saved one is known to be it.
        let mut sample = parse_song_text("Sample song", load::SAMPLE_SONG);
        sample.id = SAMPLE_SONG_ID;
        vec![(sample, None)]
    } else {
        let (songs, problems) = load::load_songs(&song_paths);
        for problem in &problems {
            eprintln!("{problem}");
        }
        if songs.is_empty() && !options.list_displays {
            eprintln!("Nothing to show.");
            return ExitCode::from(2);
        }
        songs.into_iter().map(|loaded| (loaded.song, Some(loaded.path))).collect()
    };
    let (mut songs, mut sources): (Vec<_>, Vec<_>) = songs.into_iter().unzip();
    // The songs made in the app are kept in its Songs folder; they are always loaded.
    let own: Vec<PathBuf> = bible_store::data_dir()
        .and_then(|dir| std::fs::read_dir(dir.join("Songs")).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| load::is_song_file(path))
        .collect();
    let (mut made_here, problems) = load::load_songs(&own);
    for problem in &problems {
        eprintln!("{problem}");
    }
    made_here.sort_by_key(|loaded| loaded.song.title.to_lowercase());
    // The sample song stays in the library. Once it has been changed it is saved like any
    // other song, and the saved one is shown in place of the one built in.
    if song_paths.is_empty() && made_here.iter().any(|loaded| loaded.song.id == SAMPLE_SONG_ID) {
        songs.clear();
        sources.clear();
    }
    let mut have: std::collections::HashSet<PathBuf> = sources.iter().flatten().cloned().collect();
    for loaded in made_here {
        if have.insert(loaded.path.clone()) {
            songs.push(loaded.song);
            sources.push(Some(loaded.path));
        }
    }
    // Two empty presentations after the songs hold Bible passages. They have no file.
    let bible_slots = [songs.len(), songs.len() + 1];
    for _ in bible_slots {
        songs.push(sp_model::Presentation {
            id: sp_model::Uuid::new_v4(),
            title: "Bible".to_owned(),
            groups: Vec::new(),
            copyright: sp_model::Copyright::default(),
            theme: None,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
        });
        sources.push(None);
    }
    // One more after them holds the VoxLive item: the church's VoxLive screens, a slide each,
    // made from what VoxLive says and kept in step with it. It has no file.
    let voxlive_slot = songs.len();
    songs.push(voxlive::item(&[]));
    sources.push(None);

    // Menu bar choices arrive as user events carrying the item's id.
    let mut builder = EventLoop::<AppEvent>::with_user_event();
    // A run that only saves a picture must not come to the front of whatever is in use.
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // The app puts up its own menu bar once it has started.
        builder.with_default_menu(false);
        if options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some() {
            builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
        }
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("Could not start the window system: {e}");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // A menu bar choice wakes the event loop with the item's id.
    #[cfg(target_os = "macos")]
    {
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::Menu(event.id().as_ref().to_owned()));
            }
        }));
    }
    let (themes, songs_theme, moved_in) = theme_store::load_all();
    // Themes changed while loading (a Bible layout brought into the main theme) are written
    // at once, with the new note of what is in use, so it happens only once.
    if !moved_in.is_empty() {
        for at in &moved_in {
            if let Err(problem) = theme_store::save(&themes[*at]) {
                eprintln!("{problem}");
            }
        }
        theme_store::remember_in_use(&themes[songs_theme]);
    }
    let (bible_theme, bible_layout) = (songs_theme, themes[songs_theme].bible_template_index());
    // How each song and theme file is now, as it was just read (and the themes saved above).
    let mut stamps = outside::Stamps::default();
    for source in sources.iter().flatten() {
        stamps.note(&load::song_file_for(source));
    }
    for theme in &themes {
        if let Some(file) = theme_store::file_of(theme) {
            stamps.note(&file);
        }
    }
    let (pack_tx, pack_rx) = std::sync::mpsc::channel();
    let mut app = App {
        options,
        waker: Some(event_loop.create_proxy()),
        pack_tx,
        pack_rx,
        jobs: Vec::new(),
        last_job: None,
        import_library_override: None,
        dropped: Vec::new(),
        drop_pointer: None,
        bible_jobs: Vec::new(),
        waiting: Vec::new(),
        // Commands from `softpresenter ctl` come in on the control socket. Without it (another
        // copy has it, say) the app runs as usual.
        _control: {
            let proxy = std::sync::Mutex::new(event_loop.create_proxy());
            match control::listen(move |incoming| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(AppEvent::Control(incoming));
                }
            }) {
                Ok(listening) => Some(listening),
                Err(problem) => {
                    applog::event("control", &problem);
                    None
                }
            }
        },
        show: Show::new(songs.into_iter().map(Arc::new).collect()),
        sources,
        bible_slots,
        voxlive_slot,
        voxlive_item_from: None,
        decks: deck::Decks::default(),
        // A picture read on a reader's thread wakes the window's thread to take it in.
        bin_pictures: {
            let proxy = std::sync::Mutex::new(event_loop.create_proxy());
            bin_pictures::BinPictures::new(move || {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(AppEvent::MediaRead);
                }
            })
        },
        bin_started: None,
        unsaved: Vec::new(),
        history: ShowHistory::default(),
        edit_song: None,
        edit_mark: 0,
        action_depth: None,
        live_since: None,
        advanced: false,
        started: Instant::now(),
        video: None,
        slide_videos: Vec::new(),
        slide_video_refused: std::collections::HashSet::new(),
        web_pages: Vec::new(),
        web_refused: Vec::new(),
        web_traced: None,
        voxlive: voxlive::Client::new(),
        voxlive_preview: None,
        voxlive_said: None,
        media_playlists: Vec::new(),
        announcing: None,
        stamps,
        held_songs: Vec::new(),
        held_themes: Vec::new(),
        media_cycle: None,
        folder_notes: media::FolderNotes::default(),
        folders_checked: Instant::now(),
        folder_look: None,
        look_again: false,
        followed_items: std::collections::HashSet::new(),
        unsaved_since: None,
        was_editing: false,
        theme: Arc::new(themes[songs_theme].clone()),
        theme_for_bible: Arc::new(themes[bible_theme].clone()),
        theme_arcs: themes.iter().cloned().map(Arc::new).collect(),
        themes,
        songs_theme,
        bible_theme,
        bible_layout,
        themes_unsaved: Vec::new(),
        bible_page_saved: String::new(),
        prefs_saved: None,
        frames_since_start: 0,
        player: audio::Player::default(),
        held: winit::keyboard::ModifiersState::empty(),
        capture_selects_box: false,
        focused: None,
        #[cfg(target_os = "macos")]
        menu: None,
        running: None,
        failed: false,
        capture_done: false,
        home_timing: applog::Timing::new("operator window drawings"),
        notice_logged: None,
    };
    // The pictures that were in the media bin last time, then any named on the command line.
    let saved = media::load_list();
    app.media_playlists = saved.playlists;
    let mut pictures_to_load = saved.entries;
    for path in &media_paths {
        if !pictures_to_load.iter().any(|entry| &entry.path == path) {
            pictures_to_load.push(media::Entry {
                id: Uuid::new_v4(),
                path: path.clone(),
                fit: sp_model::ImageFit::Fill,
                looping: true,
                followed: false,
            });
        }
    }
    // Every item is in the bin from the start, in its place and with its id, so slide actions,
    // playlists and what plays at the start find it; its small copy is read on a reader's
    // thread, so the window opens at once and the bin fills in. One whose file can't be read
    // leaves the bin then, as it was never let in before.
    let mut seen = std::collections::HashSet::new();
    for entry in &pictures_to_load {
        let path = &entry.path;
        // A file listed twice (as an earlier version could save it) is shown once.
        if !seen.insert(path.clone()) || app.show.media().iter().any(|item| item.id == entry.id) {
            continue;
        }
        let name = path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
        // Its id is kept between runs, so a slide's action that shows it still finds it.
        app.show.add_media(MediaItem {
            id: entry.id,
            fit: entry.fit,
            looping: entry.looping,
            ..MediaItem::new(name, path.clone())
        });
        app.bin_pictures.read(entry.id, path);
        if entry.followed {
            app.followed_items.insert(entry.id);
        }
    }
    app.bin_started = Some((Instant::now(), app.show.media().len()));
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("The window system stopped: {e}");
        return ExitCode::FAILURE;
    }
    if app.failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_voxlive_screens_live_address_counts_as_costing_and_its_test_pattern_doesnt() {
        assert!(super::web_costs("https://voxlive.app/listen/grace/screen/abc2345"));
        assert!(super::web_costs("https://VoxLive.app/listen/grace/screen/abc2345?x=1"));
        assert!(!super::web_costs("https://voxlive.app/listen/grace/screen/abc2345?demo=1"));
        assert!(!super::web_costs("https://voxlive.app/listen/grace/screen/abc2345?preview=1&host=app"));
        assert!(!super::web_costs("https://example.com/listen/grace/screen/abc2345"));
        assert!(!super::web_costs("https://voxlive.app/pricing"));
        assert!(super::web_costs("https://voxlive.app/listen/grace/scr1"), "any page for listening, however laid out");
        assert!(!super::web_costs("https://voxlive.app.example.com/listen/grace/screen/abc"));
        assert_eq!(super::web_host("https://example.com:8443/a?b#c"), "example.com:8443");
    }

    use super::*;

    #[test]
    fn enter_is_free_in_the_bible_view_too_but_not_while_typing_editing_or_under_the_key_list() {
        let ctx = egui::Context::default();
        let frame = |focus: bool| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut words = String::new();
                let field = ui.text_edit_singleline(&mut words);
                if focus {
                    field.request_focus();
                }
            })
            .drop_without_applying_deltas();
        };
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        frame(false);
        assert!(enter_key_is_free(&state, &ctx), "the Bible view, nothing with the keyboard");
        state.bible_open = false;
        assert!(enter_key_is_free(&state, &ctx), "the Show view");
        frame(true);
        frame(true);
        assert!(!enter_key_is_free(&state, &ctx), "a field has the keyboard (the find box, Scripture Lookup)");
        ctx.memory_mut(|memory| memory.stop_text_input());
        frame(false);
        assert!(enter_key_is_free(&state, &ctx));
        state.stage_editor_open = true;
        assert!(!enter_key_is_free(&state, &ctx), "the Edit view");
        state.stage_editor_open = false;
        state.key_sheet = true;
        assert!(!enter_key_is_free(&state, &ctx), "the list of keys");
    }

    #[test]
    fn in_the_bible_view_a_letter_never_jumps_the_song_on_screen_to_a_group() {
        let song = sp_model::parse_song_text("Alpha", "[Verse 1]\nOne\n\n[Verse 2]\nTwo\n\n[Chorus]\nThree\n");
        let mut show = Show::new(vec![Arc::new(song)]);
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let ctx = egui::Context::default();
        let s = Key::Character("s".into());
        // The key itself would put the song's second verse up...
        assert!(matches!(show_key_command(&s, &show), Some(Command::GoTo(_))));
        // ...so in the Bible view it is held back from the show, for Scripture Lookup.
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        assert!(show_key_held_back(&state, &ctx, &s));
        assert!(show_key_held_back(&state, &ctx, &Key::Character("2".into())));
        assert!(!show_key_held_back(&state, &ctx, &Key::Named(NamedKey::Space)), "Space still runs the show");
        assert!(!show_key_held_back(&state, &ctx, &Key::Named(NamedKey::Escape)), "and Escape clears");
        state.bible_open = false;
        assert!(!show_key_held_back(&state, &ctx, &s), "the Show view keeps its group letters");
    }

    fn args(list: &[&str]) -> Result<Option<Options>, String> {
        parse_args(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn no_arguments_means_the_sample() {
        let options = args(&[]).unwrap().unwrap();
        assert!(options.paths.is_empty() && !options.list_displays && options.output_display.is_none());
    }

    #[test]
    fn paths_and_options_mix() {
        let options = args(&["a.txt", "--output-display", "1", "songs"]).unwrap().unwrap();
        assert_eq!(options.paths, vec![PathBuf::from("a.txt"), PathBuf::from("songs")]);
        assert_eq!(options.output_display, Some(1));
    }

    #[test]
    fn help_and_bad_options() {
        assert!(args(&["--help"]).unwrap().is_none());
        assert!(args(&["--output-display"]).is_err());
        assert!(args(&["--output-display", "two"]).is_err());
        assert!(args(&["--wat"]).unwrap_err().contains("--wat"));
    }

    #[test]
    fn a_macros_actions_are_done_in_turn() {
        use sp_model::{ClearWhat, ShowAction};
        let mut state = home::HomeState::default();
        let logo = props::Prop::new("Logo".to_owned());
        let old = props::Prop::new("Old".to_owned());
        let now = Instant::now();
        state.props = vec![logo.clone(), old.clone()];
        state.props_live = vec![(old.id, now)];
        let song = sp_model::parse_song_text("Song", "One\n\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        // Clear All, then Show Prop: the old prop goes, the logo stays.
        for action in [ShowAction::Clear { what: ClearWhat::All }, ShowAction::Prop { id: logo.id, show: true }] {
            match action_commands(&show, &state, &action) {
                Some(commands) => {
                    for command in &commands {
                        clear_overlays(&mut state, command);
                    }
                }
                None => do_state_action(&mut state, &action, now),
            }
        }
        assert_eq!(state.props_live.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [logo.id]);
    }

    #[test]
    fn going_to_a_song_lands_on_its_first_slide_that_is_not_disabled() {
        let mut song = sp_model::parse_song_text("Song", "One\n\nTwo\n");
        song.groups[0].slides[0].enabled = false;
        let show = Show::new(vec![Arc::new(song)]);
        let state = home::HomeState { song_keys: vec!["song".to_owned()], ..home::HomeState::default() };
        let go = sp_model::ShowAction::Song { key: "song".to_owned() };
        assert_eq!(
            action_commands(&show, &state, &go),
            Some(vec![Command::Select(0), Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 })])
        );
        let gone = sp_model::ShowAction::Song { key: "nowhere".to_owned() };
        assert_eq!(action_commands(&show, &state, &gone), Some(Vec::new()));
        // A media action shows its picture from the media bin, found by its id.
        let mut show = show;
        let picture = sp_model::MediaItem::new("Sky".to_owned(), "/sky.png".into());
        let at = show.add_media(picture.clone());
        let media = sp_model::ShowAction::Media { id: picture.id };
        assert_eq!(action_commands(&show, &state, &media), Some(vec![Command::ShowMedia(at)]));
        let missing = sp_model::ShowAction::Media { id: Uuid::new_v4() };
        assert_eq!(action_commands(&show, &state, &missing), Some(Vec::new()));
    }

    #[test]
    fn the_editor_drops_only_a_songs_steps_from_before_it_opened() {
        let song = Arc::new(sp_model::parse_song_text("A", "One\n"));
        let mut history = ShowHistory::default();
        history.remember(0, song.clone());
        let opened = history.mark();
        history.remember(0, song.clone());
        history.forget_before(0, opened);
        assert_eq!(history.undo.len(), 1, "the step taken in the editor stays");
        history.forget(0);
        assert!(history.undo.is_empty());
    }

    #[test]
    fn items_taken_out_of_a_playlist_take_their_turn_in_the_show_views_undo() {
        let a = Arc::new(sp_model::parse_song_text("A", "One\n"));
        let removed = home::RemovedItems {
            playlist: 0,
            name: "Sunday".to_owned(),
            items: vec![(1, service::Item::Header { name: "Word".to_owned() })],
        };
        let mut history = ShowHistory::default();
        history.remember(0, a.clone());
        history.remember_items(removed.clone());
        // The items, taken out last, are put back first; then the song's change is undone.
        assert!(history.items_next(false));
        assert_eq!((history.peek(false), history.step(false, a.clone())), (None, None), "not a song's step");
        assert_eq!(history.step_items(false), Some(removed.clone()));
        assert_eq!(history.peek(false), Some(0));
        history.step(false, a.clone());
        // Redone in the order they were made: the song, then the items.
        assert_eq!(history.peek(true), Some(0));
        history.step(true, a.clone());
        assert!(history.items_next(true));
        assert_eq!(history.step_items(true), Some(removed));
        // Forgetting a song's steps leaves the items' step.
        history.forget(0);
        assert!(history.items_next(false));
    }

    #[test]
    fn undo_steps_go_back_and_forth_and_are_dropped_with_their_song() {
        let a = Arc::new(sp_model::parse_song_text("A", "One\n"));
        let b = Arc::new(sp_model::parse_song_text("B", "Two\n"));
        let mut history = ShowHistory::default();
        history.remember(0, a.clone());
        history.remember(1, b.clone());
        assert_eq!(history.peek(false), Some(1));
        let (index, song) = history.step(false, b.clone()).unwrap();
        assert_eq!((index, song.title.as_str()), (1, "B"));
        assert_eq!(history.peek(true), Some(1));
        // A song changed elsewhere (or deleted) loses its steps; another song keeps its own.
        history.forget(1);
        assert_eq!(history.peek(true), None);
        assert_eq!(history.peek(false), Some(0));
        // A new change drops what could be redone.
        history.step(false, a.clone());
        history.remember(0, a);
        assert_eq!(history.peek(true), None);
    }

    /// Writes a song that uses every moving part of the output: a build with the slide, a click
    /// build, a scrolling box, a clock, a slide that goes on by itself, and looping. For smoke
    /// runs: `SMOKE_SONG=/tmp/smoke.sppres cargo test -p sp-app write_smoke_song -- --ignored`.
    #[test]
    #[ignore = "writes a file, for smoke runs"]
    fn write_smoke_song() {
        let Ok(path) = std::env::var("SMOKE_SONG") else { return };
        let mut song = sp_model::parse_song_text("Smoke", "[Verse 1]\nOne\n\nTwo\n\nThree\n\n[Chorus]\nFour\n");
        song.loops = true;
        let slides: Vec<&mut Slide> = song.groups.iter_mut().flat_map(|group| group.slides.iter_mut()).collect();
        let [first, second, third, fourth] = <[&mut Slide; 4]>::try_from(slides).ok().unwrap();
        first.objects[0].build_in = Some(sp_model::Build::of(sp_model::BuildEffect::Fly));
        first.advance = Some(3.0);
        second.objects[0].build_in =
            Some(sp_model::Build { on_click: true, ..sp_model::Build::of(sp_model::BuildEffect::Zoom) });
        second.advance = Some(2.0);
        if let sp_model::ObjectKind::Text { style, .. } = &mut third.objects[0].kind {
            style.scroll = Some(sp_model::Scroll::Left);
        }
        let mut clock = sp_model::Object::text(sp_model::Role("clock".to_owned()), "[Clock]");
        clock.link = Some(sp_model::TextLink::Clock);
        clock.frame = Some(sp_model::SlideRect { x: 60.0, y: 40.0, w: 600.0, h: 120.0 });
        fourth.objects.push(clock);
        load::save_song(std::path::Path::new(&path), &song).unwrap();
    }

    #[test]
    fn slash_goes_to_find_and_is_never_a_hot_key() {
        // Even a slide that somehow carries `/` as its hot key is not reached by it.
        let mut keyed = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\nTwo\n");
        keyed.groups[0].slides[0].hot_key = Some('/');
        keyed.groups[0].slides[1].hot_key = Some('K');
        let show = Show::new(vec![Arc::new(keyed)]);
        let slash = Key::Character("/".into());
        assert!(opens_find(&slash));
        assert_eq!(show_key_command(&slash, &show), None);
        // Other keys are as they were: a hot key still goes to its slide, and is not find.
        let k = Key::Character("k".into());
        assert!(!opens_find(&k) && !opens_find(&Key::Named(NamedKey::Space)));
        assert_eq!(show_key_command(&k, &show), Some(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 })));
        // It is free in the Show view, and not while the Edit view is open or a sheet is up.
        let ctx = egui::Context::default();
        let mut state = home::HomeState::default();
        assert!(find_key_is_free(&state, &ctx));
        state.stage_editor_open = true;
        assert!(!find_key_is_free(&state, &ctx), "the Edit view's keyboard is the editor's");
        state.stage_editor_open = false;
        state.editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        assert!(state.editing.is_some() && !find_key_is_free(&state, &ctx));
        state.editing = None;
        state.new_song_sheet = Some(home::NewSong::default());
        assert!(!find_key_is_free(&state, &ctx));
    }

    #[test]
    fn show_keys_wait_under_the_list_of_keys_and_while_the_numbers_show() {
        let ctx = egui::Context::default();
        let digit = Key::Character("2".into());
        let space = Key::Named(NamedKey::Space);
        let escape = Key::Named(NamedKey::Escape);
        let mut state = home::HomeState::default();
        assert!(!show_key_held_back(&state, &ctx, &space), "nothing open: Space runs the show");
        assert!(!show_key_held_back(&state, &ctx, &digit), "a digit is a slide's hot key");
        state.numbers = Some(keys::Numbers { of: keys::NumbersOf::Tab, since: 0.0, picked: None });
        assert!(show_key_held_back(&state, &ctx, &digit), "while the numbers show, a digit picks");
        assert!(show_key_held_back(&state, &ctx, &escape), "and Escape only puts them away");
        assert!(!show_key_held_back(&state, &ctx, &space), "Space still runs the show");
        state.numbers = None;
        state.key_sheet = true;
        assert!(show_key_held_back(&state, &ctx, &space), "nothing runs the show under the list of keys");
    }

    #[test]
    fn show_keys() {
        let song = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n\nThree\n\n[Verse 2]\nFour\n");
        let show = Show::new(vec![Arc::new(song)]);
        let key = |key: Key| show_key_command(&key, &show);
        let jump = |slide| Some(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide }));
        assert_eq!(key(Key::Named(NamedKey::ArrowRight)), Some(Command::Next));
        assert_eq!(key(Key::Named(NamedKey::Space)), Some(Command::Next));
        assert_eq!(key(Key::Named(NamedKey::ArrowLeft)), Some(Command::Previous));
        assert_eq!(key(Key::Named(NamedKey::Home)), Some(Command::First));
        assert_eq!(key(Key::Named(NamedKey::End)), Some(Command::Last));
        assert_eq!(key(Key::Named(NamedKey::F1)), Some(Command::ClearAll));
        assert_eq!(key(Key::Named(NamedKey::Escape)), Some(Command::ClearAll));
        // Unless the Bibles window or the setup is open: then Escape closes it.
        let ctx = egui::Context::default();
        let mut state = home::HomeState::default();
        assert!(!escape_closes_something(&state, &ctx));
        state.bible.options.manage_open = true;
        assert!(escape_closes_something(&state, &ctx));
        state.bible.options.manage_open = false;
        state.setup = Some(setup::Setup::new());
        assert!(escape_closes_something(&state, &ctx));
        assert_eq!(key(Key::Named(NamedKey::Tab)), None);
        // Letters jump to the group they stand for, in either case.
        assert_eq!(key(Key::Character("a".into())), jump(0));
        assert_eq!(key(Key::Character("C".into())), jump(1));
        assert_eq!(key(Key::Character("s".into())), jump(3));
        // A group the song does not have, and a key that stands for none.
        assert_eq!(key(Key::Character("b".into())), None);
        assert_eq!(key(Key::Character("q".into())), None);
        // A slide's own hot key comes before a group's letter, and a disabled slide has none.
        let mut keyed = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n\nThree\n");
        keyed.groups[1].slides[1].hot_key = Some('A');
        keyed.groups[0].slides[0].hot_key = Some('7');
        keyed.groups[0].slides[0].enabled = false;
        let show = Show::new(vec![Arc::new(keyed)]);
        let key = |key: Key| show_key_command(&key, &show);
        assert_eq!(key(Key::Character("a".into())), jump(2));
        assert_eq!(key(Key::Character("7".into())), None);
    }

    /// A bundle's songs, taken in as an import takes them: one the library has already (or
    /// that came earlier in the bundle) is found, not added again, and all of them, in the
    /// bundle's order, go into a playlist where it was dropped on it.
    #[test]
    fn a_bundles_songs_go_into_a_playlist_in_order_and_one_here_already_isnt_added_twice() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("softpresenter-main-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let song = |title: &str| sp_model::pro_example::titled_song_showing(title, &[]);
        let bundle = dir.join("Service.proBundle");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
        for (name, title) in [
            ("Anchor Song.pro", "Anchor Song"),
            ("Evening Lantern.docx.pro", "Evening Lantern"),
            ("Anchor Song (copy).pro", "anchor song"),
            ("Dawn Road.pro", "Dawn Road"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(&song(title)).unwrap();
        }
        zip.finish().unwrap();
        let opened = bundle::open(&bundle, &dir.join("staging")).unwrap();
        // The library has Evening Lantern already, from another import (so another id).
        let earlier = bundle::open(&bundle, &dir.join("staging-2")).unwrap();
        let mut songs = vec![Arc::new(earlier.songs[1].song.clone())];
        let mut have: HashMap<(String, String), usize> = HashMap::from([(import_key(&songs[0]), 0)]);
        let mut ids = std::collections::HashSet::from([songs[0].id]);
        let mut landed = Vec::new();
        for read in &opened.songs {
            let at = already_here(&read.song, &have, &ids, &songs).unwrap_or_else(|| {
                songs.push(Arc::new(read.song.clone()));
                have.insert(import_key(&read.song), songs.len() - 1);
                ids.insert(read.song.id);
                songs.len() - 1
            });
            landed.push(at);
        }
        let titles: Vec<&str> = songs.iter().map(|song| song.title.as_str()).collect();
        assert_eq!(titles, ["Evening Lantern", "Anchor Song", "Dawn Road"], "each song once");
        assert_eq!(landed, [1, 0, 1, 2]);
        // Dropped on the playlist in front after its first item: in, in order, there; on
        // another playlist's row: at its end, in order.
        let item = |at: usize| service::Item::Song { key: format!("song-{at}"), title: songs[at].title.clone() };
        let mut services = service::Services::default();
        services.list[0].items = vec![
            service::Item::Header { name: "Welcome".to_owned() },
            service::Item::Header { name: "Sermon".to_owned() },
        ];
        services.list.push(service::Service { name: "Evening".to_owned(), items: Vec::new() });
        put_in_playlist(&mut services, 0, Some(0), landed.iter().map(|&at| item(at)).collect());
        put_in_playlist(&mut services, 1, Some(0), landed.iter().map(|&at| item(at)).collect());
        let names = |playlist: usize| -> Vec<String> {
            services.list[playlist].items.iter().map(|item| item.title().to_owned()).collect()
        };
        assert_eq!(names(0), ["Welcome", "Anchor Song", "Evening Lantern", "Dawn Road", "Sermon"]);
        assert_eq!(names(1), ["Anchor Song", "Evening Lantern", "Dawn Road"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
