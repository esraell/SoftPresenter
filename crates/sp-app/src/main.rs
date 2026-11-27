//! SoftPresenter: a home window with a slide grid, and a separate output for the audience.

// On Windows the app is a window program, so no console opens beside it. Typed commands still
// print: `main` joins the console they were typed in (see `sp_winapi::attach_parent_console`).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod announcements;
mod applog;
mod audio;
mod backgrounds;
mod bible;
mod bible_language;
mod bible_page;
mod bible_store;
mod bin_pictures;
mod bundle;
mod canva;
mod catalog;
mod catalog_cli;
mod catalog_songs;
mod cli_install;
mod clock;
mod close_button;
mod control;
mod convert;
mod deck;
mod dialogs;
mod displays;
mod editor;
mod end_look;
mod end_mark;
mod ffmpeg_fetch;
mod file_drop;
mod find;
mod find_songs;
mod find_songs_cli;
mod followed;
mod free_songbooks;
mod free_songbooks_cli;
mod home;
mod i18n;
mod icons;
mod images;
mod inspector;
mod integrations;
mod interface_size;
mod keep_awake;
mod kept;
mod key_table;
mod keys;
mod libraries;
mod library_folder;
mod links;
mod live_tabs;
mod load;
mod log_scrub;
mod logos;
mod macros;
mod make_cli;
mod mcp;
mod media;
mod menubar;
mod messages;
mod mirror;
mod ndi_download;
mod ndi_in;
mod no_graphics;
mod notices;
mod one_copy;
mod output;
mod outside;
mod pack;
mod pptx;
mod private_folder;
mod props;
mod remote;
mod removed;
mod renamed;
mod repaints;
mod reports;
mod running;
mod samples;
mod screen_cli;
mod screens;
mod secrets;
mod service;
mod service_cli;
mod setup;
mod show_cli;
mod slide_cli;
mod song_ids;
mod songbooks;
mod stage;
mod stage_editor;
mod stall;
mod studio;
mod surface_setup;
mod sweep;
mod system_language;
mod theme_cli;
mod theme_store;
mod thumbs;
mod tidy_cli;
mod timelychurch;
mod timers;
mod updates;
mod versions;
mod video_wait;
mod voxlive;
mod whats_new;
mod whole_file;
mod window_plan;
mod writer;

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
use winit::window::{Window, WindowId};

use crate::displays::{DisplayInfo, choose_output};
use crate::editor::EditDraft;
use crate::home::{HomeState, ThumbOf, home_ui};
use crate::key_table::{ActionGroup, KeyAction, KeyTable, Pressed, ShowKey};
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

Keys: Right or Space next, Left previous, Home first, End last, Escape clear all
      (off a Mac, F1 too, and F2 to F6 clear one layer each).
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

/// Says in the log how the video behind the words has reached the screens (pictures expected,
/// shown, dropped, repeated and late; see `sp_video::PaceMinute`): once a minute while it plays,
/// and for the part of a minute it played as it stops (`stopping`).
fn say_video_pace(player: &mut sp_video::Player, stopping: bool) {
    if let Some(minute) = player.take_pace(stopping) {
        applog::event("video", &minute.words(&file_name(player.path())));
    }
}

/// Waits, a second at most, until the video players let go have ended their FFmpeg, as the
/// app quits; the log says if some hadn't.
fn wait_for_videos_let_go() {
    let left = sp_video::wait_for_players_let_go(Duration::from_secs(1));
    if left > 0 {
        applog::event("video", &format!("{left} video players let go hadn't ended their FFmpeg a second on"));
    }
}

/// A video's pictures a second for the log, as `29.97 fps`, or that the file doesn't say.
fn fps_said(fps: f64) -> String {
    if fps > 0.0 { format!("{} fps", sp_video::fps_words(fps)) } else { "its pictures a second not said".to_owned() }
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
            parts.push(crate::i18n::tf("notices.reload.read", "Read again: {names}.", &[("names", &list(&self.read))]));
        }
        if !self.added.is_empty() {
            parts.push(crate::i18n::tf("notices.reload.added", "Added: {names}.", &[("names", &list(&self.added))]));
        }
        if !self.waiting.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.reload.waiting",
                "Waiting on Reload or Keep mine (in use, or with changes not saved): {names}.",
                &[("names", &list(&self.waiting))],
            ));
        }
        if !self.gone.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.reload.gone",
                "Their file is gone (Keep mine saves it again): {names}.",
                &[("names", &list(&self.gone))],
            ));
        }
        parts.extend(self.problems.iter().cloned());
        if parts.is_empty() {
            crate::i18n::t("notices.reload.nothing", "Every song is as its file has it.").to_owned()
        } else {
            parts.join(" ")
        }
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
    // Settings has its list of pages beside each page, as the home window has its columns, so it
    // opens wider and keeps room for both.
    let (size, least): ((f64, f64), (f64, f64)) = match kind {
        Panel::Screens => ((780.0, 640.0), (640.0, 440.0)),
        Panel::Settings => ((1040.0, 720.0), (820.0, 560.0)),
    };
    // It opens as much larger as the home window draws everything (its interface size), so it
    // has the room its pages were drawn for at that size, within the display it opens on.
    let zoom = f64::from(running.home_state.zoom_now);
    let display = running.home_window.current_monitor().map(|monitor| {
        let points = monitor.size().to_logical::<f64>(monitor.scale_factor());
        (points.width * 0.9, points.height * 0.9)
    });
    let grown = |points: f64, room: Option<f64>| (points * zoom).min(room.unwrap_or(f64::MAX)).max(points);
    let size = (grown(size.0, display.map(|d| d.0)), grown(size.1, display.map(|d| d.1)));
    // Testing aid: SOFTPRESENTER_SETTINGS_SIZE=1100x700 opens the Settings window at that size
    // (points), to see its pages at the sizes the design was drawn at.
    let size = match kind {
        Panel::Settings => std::env::var("SOFTPRESENTER_SETTINGS_SIZE")
            .ok()
            .and_then(|asked| {
                let (w, h) = asked.split_once('x')?;
                Some((w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?))
            })
            .unwrap_or(size),
        Panel::Screens => size,
    };
    let attrs = app_window(Window::default_attributes())
        .with_title(kind.title())
        .with_inner_size(winit::dpi::LogicalSize::new(size.0, size.1))
        .with_min_inner_size(winit::dpi::LogicalSize::new(least.0.min(size.0), least.1.min(size.1)));
    let attrs = if quiet {
        attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
    } else {
        attrs
    };
    let window = Arc::new(el.create_window(attrs).map_err(|e| {
        crate::i18n::tf(
            "notices.panel.not_opened",
            "Could not open the Screens window: {error}",
            &[("error", &e.to_string())],
        )
    })?);
    let surface = running.context.instance.create_surface(window.clone()).map_err(|e| {
        crate::i18n::tf(
            "notices.panel.not_drawn",
            "Could not draw the Screens window: {error}",
            &[("error", &e.to_string())],
        )
    })?;
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
    let longest_side = running.gpu.device.limits().max_texture_dimension_2d;
    let set_up = surface_setup::SetUp::new(config, longest_side, Instant::now(), |config| {
        surface_setup::set_up(&running.gpu.device, &surface, config, surface_setup::Of::Panel)
    });
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
    Ok(PanelWindow {
        kind,
        window,
        surface,
        set_up,
        ctx,
        state,
        renderer,
        next_repaint: None,
        frames: 0,
        least: egui::vec2(least.0 as f32, least.1 as f32),
        zoom_watch: interface_size::Watch::default(),
    })
}

/// Sets a window of its own's interface size: the home window's, as far as it has room for its
/// least size (see `interface_size::panel_zoom`), once a new size has settled. Returns how long
/// to wait before looking again while it settles.
fn follow_panel_zoom(panel: &mut PanelWindow, home: f32) -> Option<Duration> {
    let (size, scale) = (panel.window.inner_size(), panel.window.scale_factor());
    let seen = interface_size::Seen::panel((size.width, size.height), scale, home);
    match panel.zoom_watch.due(seen, Instant::now()) {
        interface_size::Due::Done => None,
        interface_size::Due::After(wait) => Some(wait),
        interface_size::Due::Now => {
            panel.zoom_watch.applied(seen);
            let points = size.to_logical::<f32>(scale);
            let zoom = interface_size::panel_zoom(home, egui::vec2(points.width, points.height), panel.least);
            panel.ctx.options_mut(|options| options.zoom_factor = zoom);
            None
        }
    }
}

/// Where the show is, for timing a slide that goes on by itself: the slide on screen, how many
/// of its click builds are done, and its place in the play order.
type AdvanceAt = (sp_engine::SlideRef, usize, Option<(usize, usize)>);

/// How many slides in a row can set each other going through their actions.
const MAX_ACTION_CHAIN: u8 = 3;

/// The slide on screen as the screens are sent it (`Frame::slide`): as it is, words and all,
/// for the stage screens keep them while they are hidden. The audience screens and the Program
/// preview draw it without them then (`Frame::words_hidden`, `output::drawn_slide`).
fn frame_slide(show: &Show) -> Option<Arc<Slide>> {
    show.live_slide().map(|(_, slide)| Arc::new(slide.clone()))
}

/// The theme the screens draw the slide on screen with: its presentation's (`theme_of`, by the
/// presentation's place), its words hidden or not; with nothing on screen, `songs`.
fn frame_theme<'a>(show: &Show, songs: &'a Arc<Theme>, theme_of: impl Fn(usize) -> &'a Arc<Theme>) -> &'a Arc<Theme> {
    show.live().map_or(songs, |at| theme_of(at.presentation))
}

/// The copyright line of the song on screen, on the slides Settings says (`when`), with the
/// church's CCLI licence number: none for what the app made (`app_made`, by the presentation's
/// place: a Bible passage, the VoxLive item, a song of the song catalog). The same while the
/// song's words are hidden: the audience screens leave it out with them
/// (`output::copyright_shown`), in the same dissolve, and the stage screens keep it.
///
/// "Last slide" is the song's last slide as sung (`last`, by the presentation's place:
/// `end_mark::last_sung`), where the end of a song's mark goes too, so the two always land
/// together: with an arrangement the slide its order ends on, and never a blank slide after
/// the last words. What has no song's words (a deck) keeps its written last slide.
fn copyright_line(
    show: &Show,
    app_made: impl Fn(usize) -> bool,
    when: service::CopyrightShow,
    licence: &str,
    last: impl FnOnce(usize) -> Option<end_mark::Last>,
) -> Option<String> {
    let at = show.live()?;
    if app_made(at.presentation) {
        return None;
    }
    let song = show.presentations().get(at.presentation)?;
    if song.copyright.is_empty() {
        return None;
    }
    let sung_last = || last(at.presentation).map(|last| show.step().is_some_and(|step| last == (step, at.slide)));
    let shown = match when {
        service::CopyrightShow::LastSlide => sung_last().unwrap_or_else(|| when.shows_on(at.slide, song.slide_count())),
        _ => when.shows_on(at.slide, song.slide_count()),
    };
    shown.then(|| song.copyright.line(&song.title, licence))
}

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
            ClearWhat::Audio => Command::Clear(sp_engine::Layer::Audio),
            ClearWhat::Announcements => Command::Clear(sp_engine::Layer::Announcements),
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
        | ShowAction::Announce { .. }
        | ShowAction::CanvaSync
        | ShowAction::Version { .. } => None,
    }
}

/// What a macro made from the start-up choice plays, in words: "the song …" or "the media
/// playlist …".
fn describe_opening(show: &Show, state: &home::HomeState, step: &sp_model::ShowAction) -> String {
    match step {
        sp_model::ShowAction::Announce { what: sp_model::Announced::Song { key } } => {
            let title = studio::playlist_song(show, state, key).map(|at| show.presentations()[at].title.clone());
            match title {
                Some(title) => crate::i18n::tf(
                    "notices.opening.song",
                    "the song {title}",
                    &[("title", &crate::i18n::quoted(&title))],
                ),
                None => crate::i18n::t(
                    "notices.opening.song_gone",
                    "a song that is no longer in the library, so nothing plays until you choose what it plays",
                )
                .to_owned(),
            }
        }
        sp_model::ShowAction::Announce { what: sp_model::Announced::MediaPlaylist { id } } => {
            let name = state.media_playlists.iter().find(|p| p.id == *id).map(|p| p.name.clone());
            match name {
                Some(name) => crate::i18n::tf(
                    "notices.opening.media_playlist",
                    "the media playlist {name}",
                    &[("name", &crate::i18n::quoted(&name))],
                ),
                None => crate::i18n::t(
                    "notices.opening.media_playlist_gone",
                    "a media playlist that is no longer there, so nothing plays until you choose what it plays",
                )
                .to_owned(),
            }
        }
        _ => crate::i18n::t("notices.opening.as_set", "what it was set to").to_owned(),
    }
}

/// Goes to a place in the track heard, playing or paused: a tenth of a second before its end at
/// the furthest, so it ends there by itself and what follows it follows.
fn seek_audio(player: &mut audio::Player, state: &HomeState, to: Duration) {
    let playing = player.playing();
    let Some(track) = playing.track.and_then(|id| state.audio.track(id)) else { return };
    let to = playing.length.map_or(to, |length| to.min(length.saturating_sub(Duration::from_millis(100))));
    applog::event("audio", &format!("go to {} in {}", audio::clock(to), track.name));
    player.seek(track, to);
}

/// Does what the operator asked of a playing video (on the media layer or the Announcements
/// layer), and keeps the window's view of it (`live`) in step.
fn ask_of_video(player: &mut sp_video::Player, live: &mut home::VideoLive, request: home::VideoRequest) {
    use home::VideoRequest;
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
                Announced::Song { key } => macros::announced_song(state, key).map(announcements::Announce::Song),
                Announced::Media { id } => Some(announcements::Announce::Media(*id)),
                Announced::MediaPlaylist { id } => Some(announcements::Announce::MediaPlaylist(*id)),
            };
            match asked {
                Some(asked) => state.announcement_request = Some(Some(asked)),
                None => {
                    // Name the macro, so the operator knows where to choose another.
                    let holder = state
                        .macros
                        .iter()
                        .find(|made| made.actions.contains(action))
                        .map(|made| {
                            crate::i18n::tf(
                                "notices.macro.in",
                                " in the macro {macro}",
                                &[("macro", &crate::i18n::quoted(&made.name))],
                            )
                        })
                        .unwrap_or_default();
                    state.notices.trouble(crate::i18n::tf(
                        "notices.macro.song_gone",
                        "Nothing plays on the Announcements layer: the song a step{holder} plays is no longer in the library. Open the macro in the Macros tab and choose what it plays.",
                        &[("holder", &holder)],
                    ));
                }
            }
        }
        // A sync from Canva, taken up by `tick_canva`.
        ShowAction::CanvaSync => state.canva.sync_request = true,
        // A theme version, switched as the button beside Continuous switches it.
        ShowAction::Version { id, keep } => {
            state.version_asks.push(versions::Ask::Use { version: *id, keep: *keep });
        }
        ShowAction::Clear { .. } | ShowAction::Audience { .. } | ShowAction::Song { .. } | ShowAction::Media { .. } => {
        }
    }
}

/// The app's one history, in the order Undo takes it back: songs changed (each step a song's
/// place and the song as it was: Undo puts the song back and keeps how it is now, to redo), and
/// things deleted that Undo can bring back (items taken out of a playlist, playlists, libraries,
/// audio playlists, cards; see `removed`): Undo puts them back, Redo deletes them again.
#[derive(Default)]
struct ShowHistory {
    /// Each step, and when it was taken (a count).
    undo: Vec<(ShowStep, u64)>,
    redo: Vec<(ShowStep, u64)>,
    next: u64,
}

/// One step of the app's history.
enum ShowStep {
    /// A song's place, and the song as it was (or, to redo, as it was before the undo).
    Song(usize, Arc<sp_model::Presentation>),
    /// Something deleted.
    Removed(Box<removed::Removed>),
}

impl ShowHistory {
    /// Keeps a song as it is before a change, and drops what could be redone.
    fn remember(&mut self, index: usize, song: Arc<sp_model::Presentation>) {
        self.keep(ShowStep::Song(index, song));
    }

    /// Keeps something just deleted, to put back, and drops what could be redone.
    fn remember_removed(&mut self, removed: removed::Removed) {
        self.keep(ShowStep::Removed(Box::new(removed)));
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

    /// Whether the next undo (or redo) puts back something deleted (or deletes it again).
    fn removed_next(&self, redo: bool) -> bool {
        matches!(if redo { self.redo.last() } else { self.undo.last() }, Some((ShowStep::Removed(_), _)))
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

    /// Takes the next undo (or redo) step when it is something deleted: it is to be put back
    /// (or deleted again).
    fn step_removed(&mut self, redo: bool) -> Option<removed::Removed> {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        match from.pop()? {
            (ShowStep::Removed(removed), when) => {
                to.push((ShowStep::Removed(removed.clone()), when));
                Some(*removed)
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
    /// Something Remote Control's server asks of the main thread: a command from a page (carried
    /// out as `sp`'s are), a slide to draw, the state to publish.
    Remote(remote::ToApp),
    /// The program that receives AirPlay said something, or a device's first picture came.
    Mirror,
    /// A deck of slides finished on its own thread.
    DeckDone,
    /// A song pack was written or read on its own thread.
    PackDone,
    /// A Bible file was read (and installed, if it said what it is) on a thread of its own.
    BibleRead,
    /// A media bin picture was read on a thread of its own.
    MediaRead,
    /// The followed folders were looked at on a thread of their own.
    FoldersRead,
    /// The library was tidied on its own thread: what to tell the operator.
    Tidied(String),
    /// The DeckLink devices found changed (one plugged in or taken away, the driver found).
    DeckLink,
    /// Canva Sync's thread has something to tell (see `canva`).
    Canva,
    /// A song catalog download or check finished on its thread (see `catalog_cli`).
    CatalogDone,
    /// A free songbook's download finished on its thread (see `free_songbooks_cli`).
    SongbookDone,
    /// A search for songs on the computer found something, or its import read some songs (see
    /// `find_songs_cli`).
    FindSongs,
    /// The library's writer wrote everything handed to it, or a file couldn't be written (see
    /// `writer`).
    Saved,
    /// Check for Updates has its answer (see `updates`).
    UpdateChecked,
    /// A fetch of the videos' FFmpeg began or ended (see `ffmpeg_fetch`).
    FfmpegFetch,
    /// The system says the displays changed (macOS: a display plugged in or out, the main one
    /// changed, a size or the arrangement changed). Only macOS sends it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    DisplaysChanged,
    /// A file window or question the app showed was answered (see `dialogs`).
    Dialog(dialogs::Answered),
    /// Files were dropped on the window: sent with the first, so it comes once the system has
    /// handed over all of that drop's files, one after another (see [`App::file_dropped`]).
    FilesDropped,
    /// A Wayland desktop said of files dragged over the window or dropped on it, which the
    /// window library doesn't say there (see `file_drop::Listening`).
    #[cfg(target_os = "linux")]
    FilesDragged(sp_wayland::Told),
}

impl AppEvent {
    /// Its name, for the log (`repaints`).
    fn name(&self) -> &'static str {
        match self {
            AppEvent::Menu(_) => "Menu",
            AppEvent::LivePreview => "LivePreview",
            AppEvent::Control(_) => "Control",
            AppEvent::Remote(_) => "Remote",
            AppEvent::Mirror => "Mirror",
            AppEvent::DeckDone => "DeckDone",
            AppEvent::PackDone => "PackDone",
            AppEvent::BibleRead => "BibleRead",
            AppEvent::MediaRead => "MediaRead",
            AppEvent::FoldersRead => "FoldersRead",
            AppEvent::Tidied(_) => "Tidied",
            AppEvent::DeckLink => "DeckLink",
            AppEvent::Canva => "Canva",
            AppEvent::CatalogDone => "CatalogDone",
            AppEvent::SongbookDone => "SongbookDone",
            AppEvent::FindSongs => "FindSongs",
            AppEvent::Saved => "Saved",
            AppEvent::UpdateChecked => "UpdateChecked",
            AppEvent::FfmpegFetch => "FfmpegFetch",
            AppEvent::DisplaysChanged => "DisplaysChanged",
            AppEvent::Dialog(_) => "Dialog",
            AppEvent::FilesDropped => "FilesDropped",
            #[cfg(target_os = "linux")]
            AppEvent::FilesDragged(_) => "FilesDragged",
        }
    }
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

/// A Bible file's work done on a thread of its own: the command line's job, if it started it,
/// and what came of it.
struct BibleWork {
    job: Option<u64>,
    result: Result<bible_store::Opened, String>,
    /// A file refused: its size and what its first bytes look like, for the log alone.
    kind: Option<String>,
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
    /// How presentation files come in (as slides, or as pictures), when it was said; otherwise
    /// as each kind was last brought in.
    pub deck_as: Option<deck::ImportAs>,
    /// Brought in by a download in the background (a free songbook): the window is left as it
    /// is, with no song chosen and nothing open closed.
    pub quiet: bool,
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

/// How an import knows a song the library has (`load::import_key`).
use load::import_key;

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
    /// How many files couldn't be read, and what the log says of each (its size and what its
    /// first bytes look like), by the sentence said of it.
    refused: usize,
    kinds: HashMap<String, String>,
}

impl Imported {
    /// Notes a file that couldn't be read, and what the log says of it.
    fn refuse(&mut self, problem: &str, path: &std::path::Path) {
        self.refused += 1;
        if let Some(kind) = applog::file_kind::of(path) {
            self.kinds.insert(problem.to_owned(), kind);
        }
    }
}

/// How the sentence saying a song imported is in the library already ends.
fn already() -> &'static str {
    crate::i18n::t("notices.import.already", " is already in the library.")
}

/// The log's line for an import: the songs that came in, from how many files, how long it took
/// and how many files couldn't be read; then any songs the library had already and files still
/// being read.
fn imported_line(songs: usize, files: usize, ms: u128, refused: usize, here: usize, reading: usize) -> String {
    let n = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
    let mut line = format!(
        "imported {} from {} in {ms} ms ({} not imported",
        n(songs, "song", "songs"),
        n(files, "file", "files"),
        n(refused, "file", "files")
    );
    if here > 0 {
        line.push_str(&format!(", {} already in the library", n(here, "song", "songs")));
    }
    if reading > 0 {
        line.push_str(&format!(", {} still being read", n(reading, "file", "files")));
    }
    line.push(')');
    line
}

/// Where the pointer is in the window, in its points, while files are dragged over it: asked of
/// the system, as the window library doesn't say (see `file_drop.rs`), or as a Wayland desktop
/// told `listening`. `None` where the system can't say.
fn file_drop_pointer(window: &Window, ctx: &egui::Context, listening: &file_drop::Listening) -> Option<egui::Pos2> {
    // Testing aid: SOFTPRESENTER_DROP_AT=X,Y puts the pointer there, in the window's points,
    // for files dragged over or dropped on the window.
    if let Ok(at) = std::env::var("SOFTPRESENTER_DROP_AT") {
        let (x, y) = at.split_once(',')?;
        return Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?));
    }
    if let Some(at) = listening.pointer() {
        return Some(file_drop::window_point(at, (0, 0), window.scale_factor(), ctx.pixels_per_point()));
    }
    let screen = file_drop::pointer_on_screen()?;
    let inner = window.inner_position().ok()?;
    Some(file_drop::window_point(screen, (inner.x, inner.y), window.scale_factor(), ctx.pixels_per_point()))
}

/// A song pack is read at once (the reply waits for it) when its file is no bigger than this;
/// a larger one, which holds videos, is read on a thread of its own.
const PACK_READ_AT_ONCE: u64 = 64 * 1024 * 1024;

/// What a theme calls its templates, in order.
fn template_names(theme: &Theme) -> Vec<(Uuid, String)> {
    theme.templates.iter().map(|template| (template.id, template.name.clone())).collect()
}

/// Whether Escape has something in the window to close (a pop-up, the Settings window, the
/// Bible page's Options or Bibles window, the sheet asking what a Bible file is, the setup, the
/// playlist chooser, the Reflow editor), which it closes instead of clearing the screens.
fn escape_closes_something(state: &home::HomeState, ctx: &egui::Context) -> bool {
    // A search left open after a drop gave the keys back closes too.
    something_over_the_window(state, ctx) || state.show_view_overlay_open() || state.library_search.is_some()
}

/// Whether something is open over the window: a pop-up, the Settings window, the Bible page's
/// Options or Bibles window, the sheet asking what a Bible file is, the setup.
fn something_over_the_window(state: &home::HomeState, ctx: &egui::Context) -> bool {
    window_over_the_window(state) || egui::Popup::is_any_open(ctx)
}

/// Whether a window or a sheet is open over the window (not a pop-up).
fn window_over_the_window(state: &home::HomeState) -> bool {
    state.settings_open
        || state.key_sheet
        || end_look::over_the_operators_window(state)
        || state.bible.options.open
        || state.bible.options.manage_open
        || !state.bible.options.details.is_empty()
        || state.setup.is_some()
        || state.typing_in_find()
        || state.free_songbooks.open
}

/// Whether a pop-up open now takes the keys that run the show (a menu moves with the arrows and
/// chooses with Space and Enter). The notices' list takes none of them: the show runs on while
/// it is open, and only Escape is the list's (it closes it).
fn popup_takes_show_keys(ctx: &egui::Context) -> bool {
    egui::Popup::is_any_open(ctx) && !notices::list_open(ctx)
}

/// What a key that runs the show does, as the table says (Settings › Keys), by the key's place on
/// the keyboard (`KeyTable::row_of`: a clicker's "." works on a Russian layout, where that key
/// types ю): `None` for a key with no row, and for an F key not added when `f_keys` is false (on
/// a Mac, where they are the system's: brightness, volume, Mission Control), whatever the table
/// says, since preferences brought from a PC may have changed them. The keys are read straight
/// from the window, before the interface draws, so a key press reaches the output without
/// waiting for the home window.
fn show_key_action(key: &Pressed, table: &KeyTable, f_keys: bool) -> Option<KeyAction> {
    table.row_of(key, f_keys).map(|row| table.action(row))
}

/// A letter that runs the show, by the character it types (a church typing in Russian sees its
/// own letters): a slide's own hot key puts it on screen; otherwise the letter jumps to the group
/// it stands for (A for the first verse, C for the chorus, and so on). A letter added in Settings
/// › Keys does its row instead (`show_key_press`).
fn letter_command(key: &Key, show: &Show) -> Option<Command> {
    // The key that opens find is never a slide's hot key or a group's letter.
    if opens_find(key) {
        return None;
    }
    let Key::Character(typed) = key else { return None };
    let mut letters = typed.chars();
    let (Some(letter), None) = (letters.next(), letters.next()) else { return None };
    show.hot_key_slide(letter).or_else(|| show.group_start(sp_model::groups_for_key(letter))).map(Command::GoTo)
}

/// What the show is asked to do for an action: the clears, Hide the Words, Show Again and the
/// slides' steps. `None` for the actions the app carries out itself (Show Selected, the
/// playlist's and the views'), and for Nothing.
fn action_command(action: KeyAction) -> Option<Command> {
    use sp_engine::Layer;
    Some(match action {
        KeyAction::ClearAll => Command::ClearAll,
        KeyAction::ClearSlide => Command::Clear(Layer::Slides),
        KeyAction::ClearMedia => Command::Clear(Layer::Media),
        KeyAction::ClearProps => Command::Clear(Layer::Props),
        KeyAction::ClearAudio => Command::Clear(Layer::Audio),
        KeyAction::ClearMessages => Command::Clear(Layer::Messages),
        KeyAction::HideWords => Command::HideWords,
        KeyAction::ShowAgain => Command::ShowAgain,
        KeyAction::Next => Command::Next,
        KeyAction::Previous => Command::Previous,
        KeyAction::First => Command::First,
        KeyAction::Last => Command::Last,
        KeyAction::ShowSelected
        | KeyAction::AddToPlaylist
        | KeyAction::RemoveFromPlaylist
        | KeyAction::GoToBible
        | KeyAction::GoToService
        | KeyAction::Nothing => return None,
    })
}

/// A key pressed that runs the show: one of the table's keys doing its action, or a letter
/// putting a slide up (`letter_command`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum KeyPress {
    Action(KeyAction),
    Letter(Command),
}

/// What a key does to the show, in the home window and the screens' windows alike: its action
/// from the table (`show_key_action`), or a letter's slide. A key set to Nothing is still the
/// show's, and does nothing (`App::key_action`), so Space set to Nothing never presses the control
/// that has the keyboard; only Escape set to Nothing is the window's (`None`), which closes a
/// menu or pop-up with it as ever.
fn show_key_press(key: &Pressed, table: &KeyTable, show: &Show, f_keys: bool) -> Option<KeyPress> {
    match show_key_action(key, table, f_keys) {
        Some(KeyAction::Nothing) if matches!(key.logical, Key::Named(NamedKey::Escape)) => None,
        Some(action) => Some(KeyPress::Action(action)),
        None => letter_command(&key.logical, show).map(KeyPress::Letter),
    }
}

/// A key that runs the show held down (`repeat`, its repeats): each does what the key does, but
/// for Show Selected, Add to Playlist and Remove from Playlist, whose repeats do nothing, as a
/// held Enter's do. Each first press changes what the next would act on: Show Selected took the
/// pick, so a repeat would put the open item up from its first slide; Add and Remove would go on
/// to the next song open or item picked.
fn held_down(press: KeyPress, repeat: bool) -> KeyPress {
    match press {
        KeyPress::Action(KeyAction::ShowSelected | KeyAction::AddToPlaylist | KeyAction::RemoveFromPlaylist)
            if repeat =>
        {
            KeyPress::Action(KeyAction::Nothing)
        }
        press => press,
    }
}

/// What a key that runs the show does from any of the app's windows, the home window's and the
/// screens' alike (`show_key_press`), with the rules every window keeps: nothing while a question
/// or the new-song sheet is open; a key held down as `held_down` says; and in the Edit view only
/// Escape, the F rows (an F key added on a Mac too) and "." reach the show, and only when they
/// clear or hide the words (`ActionGroup::Clear`), "." only while nothing is being typed or
/// renamed (`HomeState::typing_or_renaming`): Escape only when it has nothing to do in the editor
/// (`HomeState::escape_edits`: no words being typed, no name being retyped, nothing picked on the
/// canvas, no drag, nothing open over the window), and never when it is held down (`repeat`): the
/// press that let go of the picked box, or put back the words typed, must not go on to clear the
/// screens as it repeats. Space, the arrows, Page Down and Page Up, Home, End, the letters and
/// every key added stay the editor's, whatever they are set to. The home window holds more keys
/// back of its own (`home_key_command`).
fn show_key_command(
    state: &home::HomeState,
    ctx: &egui::Context,
    key: &Pressed,
    repeat: bool,
    show: &Show,
    f_keys: bool,
) -> Option<KeyPress> {
    if state.new_song_sheet.is_some() || state.alert_open() {
        return None;
    }
    let press = held_down(show_key_press(key, &state.keys, show, f_keys)?, repeat);
    if state.edit_kind().is_some() {
        let escape = matches!(key.logical, Key::Named(NamedKey::Escape));
        let clears = matches!(press, KeyPress::Action(action) if action.group() == ActionGroup::Clear);
        let escape_f_or_period = state
            .keys
            .row_of(key, f_keys)
            .is_some_and(|key| key == ShowKey::Escape || key == ShowKey::Period || key.is_f_key());
        // A key that types a character ("."), while words or a name are being typed, is the
        // typist's next key, whether or not their field still has the keyboard.
        let types = matches!(key.logical, Key::Character(_));
        if !clears
            || !escape_f_or_period
            || (escape && (repeat || state.escape_edits(ctx)))
            || (types && state.typing_or_renaming(ctx))
        {
            return None;
        }
    }
    Some(press)
}

/// Whether `key` is one of the F rows, F1 to F12 (Settings › Keys), by its place, else by the key
/// the window says it is.
fn f_row(key: &Pressed) -> bool {
    ShowKey::of(&key.physical).or_else(|| ShowKey::named(&key.logical)).is_some_and(ShowKey::is_f_key)
}

/// Whether the keyboard is in one of the two typing fields the F rows still reach: the find box
/// (open, before it has drawn with the keyboard too) or Scripture Lookup. In every other text
/// field (a song's words, a name, a title) every key is the field's, the F keys too.
fn typing_with_f_rows(state: &home::HomeState, ctx: &egui::Context) -> bool {
    state.typing_in_find() || ctx.memory(|memory| memory.has_focus(home::scripture_lookup_id()))
}

/// What an F row set to `action` does while the find box or Scripture Lookup has the keyboard
/// (`typing_with_f_rows`). Nothing leaves the typing alone: the field keeps the keyboard and its
/// words. Show Selected and Add to Playlist act on what is typed, as the field's own Return does,
/// which the field carries out as it next draws (`HomeState::typed_key`). For any other action
/// the field gives up the keyboard (the find box closes), and the action is returned, to be done
/// as anywhere: so in SoftPresenter's style F2 to F6 clear while typing too.
fn typed_key(state: &mut home::HomeState, ctx: &egui::Context, action: KeyAction) -> Option<KeyAction> {
    match action {
        KeyAction::Nothing => None,
        KeyAction::ShowSelected | KeyAction::AddToPlaylist => {
            state.typed_key = Some(action);
            None
        }
        other => {
            if state.typing_in_find() {
                home::end_search(ctx, state);
            } else {
                ctx.memory_mut(|memory| memory.surrender_focus(home::scripture_lookup_id()));
            }
            Some(other)
        }
    }
}

/// What a key set to one of the window's own actions does (Settings › Keys): Add to Playlist as
/// `add_key` says. Remove from Playlist takes out of the playlist listed only what can be seen
/// picked there: several items picked, which are lit, or a single pick when it is the open item
/// (a single pick isn't lit, and stays where it was when the open item moves on: Next going on
/// into the next item, a click on another item's slide). Undo puts them back, and nothing on the
/// screens changes. Anything else (a single pick left behind, none, the playlist not listed: the
/// Bible view, a library or the chooser in its place) takes nothing out, and a refusal's cue says
/// why. Go to Bible and Go to Service as Command-2 (Scripture Lookup ready for a reference) and
/// Command-1. Show Selected is Enter's (`App::enter_key`), and the show's own actions are the
/// show's (`action_command`).
fn window_key_action(show: &Show, state: &mut home::HomeState, action: KeyAction) {
    match action {
        KeyAction::AddToPlaylist => add_key(show, state),
        KeyAction::RemoveFromPlaylist => {
            state.item_picks.check(&state.services);
            let places = state.item_picks.places.clone();
            let seen = state.item_picks.several() || (places.len() == 1 && state.services.chosen == Some(places[0]));
            if !seen || !crate::studio::playlist_in_sight(state, state.services.front_at()) {
                state
                    .notices
                    .refuse(crate::i18n::t("keys.remove.none", "Pick an item in the playlist to take it out."));
            } else {
                state.take_out_items(state.services.current, &places);
            }
        }
        KeyAction::GoToBible => {
            state.view_request = Some(2);
            state.bible.focus_lookup = true;
        }
        KeyAction::GoToService => state.view_request = Some(0),
        _ => {}
    }
}

/// Add to Playlist's key, into the playlist in front: in the Bible view the passage looked up,
/// as Bible's Add puts it; in the Service view a library's song open, with the library listed and
/// no item of the playlist open, at the end, as its menu's Add to Playlist puts it. With an item
/// of the playlist open, of any kind, the song chosen may not be the one in sight (a passage, a
/// picture or a placeholder opened leaves the song chosen before), so nothing is added. Then, as
/// with a song the app makes or nothing looked up in the Bible view, a refusal's cue says what
/// to open.
fn add_key(show: &Show, state: &mut home::HomeState) {
    if state.bible_open {
        if let Some(saved) = state.bible.saved() {
            let front = state.services.front_at();
            home::add_passage(state, Some(front), saved);
            return;
        }
    } else {
        let open = show.selected();
        let library_listed = !state.playlist_open && !state.chooser_open;
        if library_listed
            && state.services.chosen.is_none()
            && state.song_keys.get(open).is_some()
            && !state.app_made(open)
        {
            crate::studio::SongAsks { add: vec![open], ..Default::default() }.carry_out(show, state);
            return;
        }
    }
    state.notices.refuse(home::nothing_to_add());
}

/// Keys that run the show but belong to something open in the window: under the list of keys
/// nothing runs the show; while the numbers show (after modifier-1 to 7), a number and Escape
/// are theirs, a number by the digit it types or by its place (a digit added does its row only
/// when no numbers show); in an open menu the arrows, Space and Enter move in it and choose,
/// whatever they are set to. In the Bible view a key typing a letter or a digit starts the next
/// reference in Scripture Lookup (see `bible_area` in `home.rs`), whatever its place's row says,
/// never a jump to a group of the song on screen: after Return has picked a verse and given the
/// keyboard back, typing "ps 23" must not put the song's second verse up with its "s", and on a
/// Russian or Ukrainian layout the "." key typing ю types it (a screen cleared by chance is seen
/// by the whole room; a stray letter in the reference box is not). A key typing anything else
/// with a row does its action there, as in the Service view ("." on a US keyboard clears), and
/// so do the keys that type nothing (the F rows, Space).
/// While the find box is being typed in every key is the box's, before the box has drawn with the
/// keyboard too: a letter typed straight after `/` is typed in it, never a slide's hot key or a
/// group's letter, and Escape closes it, clearing nothing. Only the F rows reach the show from it
/// (`typed_key`). A search left open after a drop has given the keys back to the show
/// (`HomeState::search_resting`).
fn show_key_held_back(state: &home::HomeState, ctx: &egui::Context, key: &Pressed, f_keys: bool) -> bool {
    if state.key_sheet || end_look::over_the_operators_window(state) || (state.typing_in_find() && !f_row(key)) {
        return true;
    }
    if state.bible_open
        && let Key::Character(typed) = &key.logical
        && !opens_find(&key.logical)
        && (typed.chars().next().is_some_and(char::is_alphanumeric) || state.keys.row_of(key, f_keys).is_none())
    {
        return true;
    }
    use winit::keyboard::{KeyCode as C, PhysicalKey};
    let digit = matches!(&key.logical, Key::Character(typed) if typed.chars().count() == 1 && typed.chars().all(|c| ('1'..='9').contains(&c)))
        || matches!(
            key.physical,
            PhysicalKey::Code(
                C::Digit1
                    | C::Digit2
                    | C::Digit3
                    | C::Digit4
                    | C::Digit5
                    | C::Digit6
                    | C::Digit7
                    | C::Digit8
                    | C::Digit9
            )
        );
    if state.numbers.is_some() && (digit || matches!(key.logical, Key::Named(NamedKey::Escape))) {
        return true;
    }
    popup_takes_show_keys(ctx)
        && matches!(
            key.logical,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::Space | NamedKey::Enter)
        )
}

/// What a key pressed in the home window does to the show, as the table in `state.keys` says
/// (with the F keys read or not, as `show_key_action`), or `None` when the key is the window's:
/// while a text box is typed in (but for an F row in the find box or Scripture Lookup,
/// `typing_with_f_rows`), the key is held back (`show_key_held_back`), or it is Escape set to
/// Nothing. Escape runs the show only when there is nothing else for it to close. The rules every
/// window keeps (a question or the new-song sheet open, a key held down, the Edit view) are
/// `show_key_command`'s.
fn home_key_command(
    state: &home::HomeState,
    ctx: &egui::Context,
    key: &Pressed,
    repeat: bool,
    show: &Show,
    f_keys: bool,
) -> Option<KeyPress> {
    let f_row_typed = f_row(key) && typing_with_f_rows(state, ctx);
    if (ctx.text_edit_focused() && !f_row_typed) || show_key_held_back(state, ctx, key, f_keys) {
        return None;
    }
    let press = show_key_command(state, ctx, key, repeat, show, f_keys)?;
    if matches!(key.logical, Key::Named(NamedKey::Escape)) && escape_closes_something(state, ctx) {
        return None;
    }
    Some(press)
}

/// What a key does to a slide picked (`running::pick_key`) while it is set to `action`, as the
/// prototype has it: only Next and Previous touch the pick. → doing Next moves it on; any other
/// key doing Next takes it, as Space and Enter do, since Next starts what is lined up when
/// nothing is on screen (so Page Down, a clicker's forward, puts the picked slide up); any key
/// doing Previous moves it back (Page Up, a clicker's back, too). A key set to anything else does
/// that, so Space set to Clear All clears, never putting the pick on screen.
fn pick_key_for(key: &Pressed, action: KeyAction, picked: bool, showing: bool, editing: bool) -> running::PickKey {
    let (next, previous) = (action == KeyAction::Next, action == KeyAction::Previous);
    if !next && !previous {
        return running::PickKey::Pass;
    }
    // → by what the window says it is: the number pad's 6 with Num Lock off is → too.
    let right = next && matches!(key.logical, Key::Named(NamedKey::ArrowRight));
    running::pick_key(next && !right, right, previous, picked, showing, editing)
}

/// What a key set to `action` does to the slide picked (`pick_key_for`), from the window's state
/// as it is: a pick that moves is moved here, to the next or the previous slide of its passage or
/// song in the order sung (at either end it stays, with no notice); taking it is the caller's, as
/// Enter takes it (`App::enter_key`).
fn pick_key_press(show: &Show, state: &mut home::HomeState, key: &Pressed, action: KeyAction) -> running::PickKey {
    let picked = state.pick(show);
    let decided = pick_key_for(key, action, picked.is_some(), show.live().is_some(), state.edit_kind().is_some());
    if let (running::PickKey::Move(forward), Some(at)) = (decided, picked) {
        let bible = state.picked.as_ref().is_some_and(|picked| picked.passage.is_some());
        let order = show.presentations().get(at.presentation).map(|song| song.order()).unwrap_or_default();
        if let Some(slide) = running::pick_beside(&order, at.slide, forward) {
            state.set_pick(show, sp_engine::SlideRef { presentation: at.presentation, slide }, bible);
        }
    }
    decided
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

/// Whether Enter is free to run the show (in the Service view and the Bible view alike): no
/// control has the keyboard, the Edit view is not open, and no sheet, question, menu or Options
/// is open over the window.
fn enter_key_is_free(state: &home::HomeState, ctx: &egui::Context) -> bool {
    !ctx.text_edit_focused()
        && ctx.memory(|memory| memory.focused().is_none())
        && state.edit_kind().is_none()
        && !window_over_the_window(state)
        && !popup_takes_show_keys(ctx)
        && state.new_song_sheet.is_none()
        && !state.alert_open()
}

/// Where a key pressed in the home window goes (see `window_event`), judged from the window as
/// it was last drawn.
#[derive(Clone, Debug, PartialEq)]
enum KeyGoes {
    /// `/`: the find box opens, empty, or with the words of a search a drop left open
    /// (`find_key_is_free`, `HomeState::open_find`).
    Find,
    /// A key that runs the show, acted on at once (`home_key_command`): Space, the arrows, a hot
    /// key, Escape, each doing what the table says.
    Show(KeyPress),
    /// An F row pressed while the find box or Scripture Lookup has the keyboard (off a Mac),
    /// doing what the table says there (`typed_key`).
    Typed(KeyAction),
    /// Enter: the slide picked goes on screen, or the open item of the playlist
    /// (`enter_key_is_free`).
    Enter,
    /// The window's: passed to the interface, which acts on it as it next draws.
    Window,
}

/// Where a key pressed in the home window goes, with `held` the modifiers held: `/` opens the
/// find box, from wherever the keyboard runs the show; the keys that run the show act at once
/// and are not passed on, so Space can never also press whatever control happens to have the
/// keyboard, set to Nothing too (only Escape set to Nothing is passed on, to close a menu or
/// pop-up; see `show_key_press`); an F row in the find box or Scripture Lookup does what it does
/// there (`KeyGoes::Typed`); Enter puts the slide picked on screen, or the open item, when no control has the
/// keyboard and nothing is open over the window, and not when held down, so a Return held in
/// the find box can't go on to put what it opened on screen. Anything else is the window's.
fn home_key_goes(
    state: &home::HomeState,
    ctx: &egui::Context,
    show: &Show,
    key: &Pressed,
    repeat: bool,
    f_keys: bool,
) -> KeyGoes {
    let (command, control, alt) = (key.held.super_key(), key.held.control_key(), key.held.alt_key());
    if opens_find(&key.logical) && find_key_is_free(state, ctx) && !command && !control && !alt {
        return KeyGoes::Find;
    }
    if !command
        && !control
        && let Some(asked) = home_key_command(state, ctx, key, repeat, show, f_keys)
    {
        return match asked {
            KeyPress::Action(action) if f_row(key) && typing_with_f_rows(state, ctx) => KeyGoes::Typed(action),
            asked => KeyGoes::Show(asked),
        };
    }
    if matches!(key.logical, Key::Named(NamedKey::Enter))
        && !repeat
        && enter_key_is_free(state, ctx)
        && !command
        && !control
        && !alt
    {
        return KeyGoes::Enter;
    }
    KeyGoes::Window
}

/// Whether a key pressed in the home window waits for the window to draw what it was given before
/// it (`waiting`, not drawn yet): when that may change where keys go or what they act on (a key
/// that isn't typing: Return, Escape, Tab, an arrow, Delete, one held with Command, Control or
/// Option; a mouse button), and the key is one `home_key_goes` may keep from the window (Enter,
/// `/`, a key that runs the show). Keys then act in the order they were pressed, each on the
/// window as the ones before it left it, however slowly the window draws (an Intel Mac's takes some
/// 60 ms a frame while the slides' pictures are made): Return in the find box and a quick
/// Enter after it put the slide picked on screen, as they do with a frame between them, and ↓
/// and a quick Enter start the item ↓ opened, not the one open before. While an F row's Show
/// Selected or Add to Playlist waits for the find box or Scripture Lookup to carry it out
/// (`asked`, `HomeState::typed_key`), every key waits: a Space straight after F5 goes on from
/// what it showed, and letters typed straight after F2 don't change what it adds. A key held down
/// goes on at once.
fn key_waits_for_drawing(
    waiting: &[egui::Event],
    asked: bool,
    key: &Pressed,
    repeat: bool,
    table: &KeyTable,
    show: &Show,
    f_keys: bool,
) -> bool {
    use egui::Key as K;
    let changes_keys = |event: &egui::Event| match event {
        egui::Event::Key { pressed: true, key, modifiers, .. } => {
            modifiers.command
                || modifiers.ctrl
                || modifiers.alt
                || modifiers.mac_cmd
                || matches!(
                    key,
                    K::Enter
                        | K::Escape
                        | K::Tab
                        | K::ArrowUp
                        | K::ArrowDown
                        | K::ArrowLeft
                        | K::ArrowRight
                        | K::Backspace
                        | K::Delete
                        | K::Insert
                        | K::Home
                        | K::End
                        | K::PageUp
                        | K::PageDown
                )
        }
        // A character typed may start what takes the keyboard (a reference in the Bible view):
        // "1." typed quickly there must type its "." as well, never clear the screen.
        egui::Event::Text(_)
        | egui::Event::PointerButton { .. }
        | egui::Event::Copy
        | egui::Event::Cut
        | egui::Event::Paste(_) => true,
        _ => false,
    };
    let kept = matches!(key.logical, Key::Named(NamedKey::Enter))
        || opens_find(&key.logical)
        || show_key_press(key, table, show, f_keys).is_some();
    !repeat && (asked || (kept && waiting.iter().any(changes_keys)))
}

/// Testing aid: SOFTPRESENTER_LOG_KEYS=1, in a copy with its own data folder
/// (SOFTPRESENTER_DATA), says in the log ("keys") where each key pressed in the home window went
/// and what was picked and on screen after it (`App::log_key`), and each drawing of the window
/// that took 40 ms or more: to see on a slow computer, with a person at the keyboard, that keys
/// pressed quickly one after another act in turn. A letter is said as "a letter", never which.
fn log_keys() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("SOFTPRESENTER_DATA").is_some()
            && std::env::var("SOFTPRESENTER_LOG_KEYS").is_ok_and(|on| on == "1")
    })
}

/// Waits, as the app ends, for the replies to `sp` it has handed over to be written: `menu Quit`'s
/// was handed over as the app began to leave, and the program could end before it was written.
/// A reply not written in two seconds (its thread stuck) is said in the log and left.
fn finish_replies() {
    let left = control::finish_replies(Duration::from_secs(2));
    if left > 0 {
        applog::problem("control", &format!("{left} replies to sp were still unwritten as the app ended"));
    }
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
        .ok_or_else(|| {
            i18n::t(
                "notices.screens.no_format",
                "This display does not offer a colour format SoftPresenter can draw to.",
            )
            .to_owned()
        })
}

/// A screen's name, by its id, for the log.
fn screen_name(state: &HomeState, id: Uuid) -> &str {
    state.screens.iter().find(|screen| screen.id == id).map_or("A screen", |screen| screen.name.as_str())
}

/// Switches a screen off whose X was pressed on its own window (see `close_button`), as closing
/// its window does: it stays in the list, and `sync_screens` saves the list and closes the
/// window. The operator is told how to bring it back.
fn switch_off_by_its_x(running: &mut Running, screen: Uuid) {
    let name = screen_name(&running.home_state, screen).to_owned();
    if let Some(closed) = running.home_state.screens.iter_mut().find(|have| have.id == screen) {
        closed.on = false;
    }
    running.home_state.screens_changed = true;
    applog::event("screens", &format!("{name}: switched off with the close button on its own window"));
    running.home_state.notices.info(i18n::tf(
        "screens.closed_from_display",
        "{screen} was turned off from its own display. Switch it on again in Settings \u{203a} Screens.",
        &[("screen", &i18n::quoted(&name))],
    ));
    running.home_window.request_redraw();
}

/// The connected displays as the window system lists them.
/// Notes in the log where a screen's window was asked to go and which display it is on, so a
/// screen on the wrong display can be seen from the log.
fn log_placement(screen: &str, place: &Placed, window: &Arc<Window>, displays: &[DisplayInfo]) {
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
    let on = on.as_deref().unwrap_or("a display not named yet");
    // Wayland gives no window a position: the line then says so instead of ending in "at ".
    let place = match window.outer_position() {
        Ok(p) => format!("{screen}: asked for {asked}; the window is on {on} at {},{}", p.x, p.y),
        Err(_) => {
            look_again_later(screen, asked, window);
            format!("{screen}: asked for {asked}; the window is on {on} (the window system doesn't say where)")
        }
    };
    applog::event("screens", &place);
}

/// Screens' windows whose place the window system didn't say when they were made (Wayland gives
/// a window no position, and says which display it is on only once it is up), with when.
type LateLook = (std::sync::Weak<Window>, String, String, Instant);
static LOOK_AGAIN: std::sync::Mutex<Vec<LateLook>> = std::sync::Mutex::new(Vec::new());

/// How long after a window is made its display is looked at again.
const LOOK_AGAIN_AFTER: Duration = Duration::from_millis(1500);

/// Notes a screen's window to be looked at again a moment from now, and wakes the event loop
/// then (a thread of its own asks the window to draw), so the log says where it really went.
fn look_again_later(screen: &str, asked: &str, window: &Arc<Window>) {
    let weak = Arc::downgrade(window);
    if let Ok(mut pending) = LOOK_AGAIN.lock() {
        pending.push((weak.clone(), screen.to_owned(), asked.to_owned(), Instant::now()));
    }
    std::thread::spawn(move || {
        std::thread::sleep(LOOK_AGAIN_AFTER);
        if let Some(window) = weak.upgrade() {
            window.request_redraw();
        }
    });
}

/// Logs where the windows noted by `look_again_later` are now that they are up.
fn log_late_placements(displays: &[DisplayInfo]) {
    let Ok(mut pending) = LOOK_AGAIN.try_lock() else { return };
    if pending.is_empty() {
        return;
    }
    pending.retain(|(window, screen, asked, made)| {
        if made.elapsed() < LOOK_AGAIN_AFTER {
            return true;
        }
        if let Some(window) = window.upgrade() {
            let on = window
                .current_monitor()
                .and_then(|m| {
                    let name = m.name();
                    displays
                        .iter()
                        .find(|d| d.x == m.position().x && d.y == m.position().y)
                        .or_else(|| displays.iter().find(|d| name.as_deref().is_some_and(|n| d.is_named(n))))
                        .map(|d| d.name.clone())
                })
                .unwrap_or_else(|| "a display the window system doesn't name".to_owned());
            applog::event("screens", &format!("{screen}: asked for {asked}; once up, the window is on {on}"));
        }
        false
    });
}

/// The program file this copy started from, as it was then (its size and time written).
static STARTED_FROM: std::sync::OnceLock<Option<(PathBuf, u64, std::time::SystemTime)>> = std::sync::OnceLock::new();

/// The program file's size and time now, for comparing with how it was at the start.
fn program_file_now(path: &std::path::Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Notes the program file as it is at the start (called early, once).
fn note_program_file() {
    STARTED_FROM.get_or_init(|| {
        let path = std::env::current_exe().ok()?;
        let (len, at) = program_file_now(&path)?;
        Some((path, len, at))
    });
}

/// Whether a different program file now stands where this copy started from: a newer build was
/// installed while this one runs (copied over it, or its app swapped for a new one). A file
/// gone and not replaced isn't counted.
fn newer_program_installed() -> bool {
    let Some(Some((path, len, at))) = STARTED_FROM.get() else { return false };
    program_file_now(path).is_some_and(|(now_len, now_at)| now_len != *len || now_at != *at)
}

/// Whether a screen's window that has to leave its display (it was switched off, or the display
/// went) is kept hidden to show again (macOS), or closed and made again when needed (the other
/// systems, where a window can't be moved off a display that has gone without being shown).
const KEEP_HIDDEN: bool = cfg!(target_os = "macos");

/// How often the displays are looked at though nothing has said they changed. On macOS the
/// system says so (`DisplayWatch`) and this is only a backstop; elsewhere it is how a display
/// plugged out is found (the window library doesn't pass on the system's word), within a second.
const DISPLAYS_LOOK_EVERY: Duration =
    if cfg!(target_os = "macos") { Duration::from_secs(5) } else { Duration::from_secs(1) };

/// A display's place and size as a rectangle.
fn monitor_rect(monitor: &winit::monitor::MonitorHandle) -> displays::Rect {
    displays::Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        w: monitor.size().width,
        h: monitor.size().height,
    }
}

/// Puts a screen's window on its display and shows it there, with no animation. On macOS it is
/// a plain borderless window put on the display's frame above the menu bar and Dock (see
/// `sp_video::cover_display`), never a full-screen Space, which slides in and out. Elsewhere it
/// is the window library's borderless full screen on that display, which Windows does at once.
/// `fresh` is a window just opened on the display.
fn put_on_display(
    window: &Window,
    monitor: &winit::monitor::MonitorHandle,
    above: bool,
    fresh: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::MonitorHandleExtMacOS;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let _ = fresh;
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return Err("Not a macOS window.".to_owned()) };
        sp_video::cover_display(handle.ns_view, monitor.native_id(), above)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = above;
        if !fresh {
            window.set_fullscreen(None);
        }
        window.set_outer_position(monitor.position());
        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(monitor.clone()))));
        window.set_visible(true);
        Ok(())
    }
}

/// How the GPU device hands out memory: wgpu's own way unless a test asks for another. On
/// DirectX 12 and Vulkan (Windows and Linux) wgpu places textures and buffers in blocks it
/// reserves ahead: by default 128 to 256 MB of the GPU's memory and 64 to 128 MB of memory the
/// processor can reach, one at least of each kind kept once made, whatever is in it, which on a
/// PC whose graphics share the computer's memory all counts as the app's. wgpu's small blocks
/// (8 to 64 MB and 4 to 32 MB) would hold less, but then a 4K screen's picture (33 MB) gets a
/// block of its own each time it is made, as the two pictures of every fade are, on the screens'
/// thread: so they are only tried, on a PC, before anything is decided. A Mac's Metal makes each
/// texture and buffer on its own and takes no notice of either.
fn gpu_memory_hints() -> wgpu::MemoryHints {
    // Testing aid: SOFTPRESENTER_GPU_MEMORY=small asks for wgpu's small memory blocks, with no
    // new build, to compare memory and the screens' timing on a PC.
    match std::env::var("SOFTPRESENTER_GPU_MEMORY").as_deref() {
        Ok("small") => wgpu::MemoryHints::MemoryUsage,
        _ => wgpu::MemoryHints::Performance,
    }
}

/// Makes the surface a window is drawn on and gives it to the output thread. False (with the
/// reason in the notices) when it can't be made.
fn add_surface(
    output: &OutputHandle,
    context: &RenderContext,
    device_id: usize,
    screen: &screens::Screen,
    window: &Arc<Window>,
    notices: &mut notices::Notices,
) -> bool {
    let surface = match context.instance.create_surface(window.clone()) {
        Ok(surface) => surface,
        Err(e) => {
            notices.lasting_trouble(i18n::tf(
                "notices.screens.not_drawn",
                "Could not draw to \"{name}\": {error}",
                &[("name", &screen.name), ("error", &e.to_string())],
            ));
            return false;
        }
    };
    let format = match surface_format(&surface, context.devices[device_id].adapter()) {
        Ok(format) => format,
        Err(e) => {
            notices.lasting_trouble(e);
            return false;
        }
    };
    let size = window.inner_size();
    output.send(OutputMsg::AddSurface {
        id: screen.id,
        name: screen.name.clone(),
        surface,
        format,
        w: size.width,
        h: size.height,
    });
    true
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

/// The part of a display a window may use: on Windows what the taskbar leaves, asked of the
/// system; elsewhere the display less room for a menu bar at the top and a dock at the bottom
/// (the window library doesn't say how much they take).
fn work_area(monitor: &winit::monitor::MonitorHandle, scale: f64) -> displays::Rect {
    let (pos, size) = (monitor.position(), monitor.size());
    if let Some((x, y, w, h)) =
        sp_winapi::work_area_at(pos.x + (size.width / 2) as i32, pos.y + (size.height / 2) as i32)
    {
        return displays::Rect { x, y, w, h };
    }
    let (top, bottom) = ((48.0 * scale) as u32, (96.0 * scale) as u32);
    displays::Rect { x: pos.x, y: pos.y + top as i32, w: size.width, h: size.height.saturating_sub(top + bottom) }
}

/// The connected displays, named as the system names them (on a Mac as System Settings does, on
/// Windows by the names the monitors give themselves, as its display settings show them), and
/// which is the main display (`primary`, with the menu bar or the taskbar).
pub(crate) fn display_list(
    monitors: impl Iterator<Item = winit::monitor::MonitorHandle>,
    primary: Option<winit::monitor::MonitorHandle>,
) -> Vec<DisplayInfo> {
    // On a Mac by display id; on Windows by device name, which is the window library's name for
    // the display; on Linux by the connector the window system names ("DP-1", "None-2").
    #[cfg(target_os = "macos")]
    let names = sp_video::display_names();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let names = {
        static LAST: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
        let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        displays::names_or_last(sp_winapi::monitor_names(), &mut last)
    };
    let mut list: Vec<DisplayInfo> = Vec::new();
    // On Wayland a window's own list of monitors holds each display it was told of at the start
    // twice (winit adds the display again when the system describes it), which listed every
    // display as "DP-1" and "DP-1 (2)". The event loop's list has each once, and in the same
    // order, so the places of the displays agree.
    #[cfg(target_os = "linux")]
    let monitors = {
        let mut seen: Vec<winit::monitor::MonitorHandle> = Vec::new();
        monitors.filter(move |m| {
            let new = !seen.contains(m);
            if new {
                seen.push(m.clone());
            }
            new
        })
    };
    let monitors: Vec<winit::monitor::MonitorHandle> = monitors.collect();
    #[cfg(target_os = "linux")]
    let names = {
        let window_names: Vec<String> = monitors.iter().filter_map(winit::monitor::MonitorHandle::name).collect();
        displays::linux_names(&window_names, &displays::connected_monitors())
    };
    for (index, m) in monitors.into_iter().enumerate() {
        let old_name = m.name().unwrap_or_else(|| "Display".to_owned());
        #[cfg(target_os = "macos")]
        let own = {
            use winit::platform::macos::MonitorHandleExtMacOS;
            displays::own_name(&names, &m.native_id())
        };
        #[cfg(not(target_os = "macos"))]
        let own = displays::own_name(&names, &old_name);
        let name = displays::listed_name(own, &old_name, &list);
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
    /// Which window it is.
    of: surface_setup::Of,
    surface: &'a wgpu::Surface<'static>,
    /// How the surface is set up. A frame is drawn only while it is set up at the size the
    /// frame is made for (see `surface_setup`).
    set_up: &'a mut surface_setup::SetUp,
    renderer: &'a mut egui_wgpu::Renderer,
    ctx: &'a egui::Context,
    label: &'static str,
    /// Testing aid: the size in pixels a capture run lays the window out at, drawn off the
    /// window (see `interface_size::capture_size`), instead of the window's own.
    offscreen: Option<[u32; 2]>,
}

/// Draws one frame of the interface in a window and returns what was drawn, for a capture.
/// Also says where the time went (for `repaints`).
fn paint_ui(
    gpu: &Gpu,
    target: UiTarget<'_>,
    mut full: egui::FullOutput,
) -> (Vec<egui::ClippedPrimitive>, egui_wgpu::ScreenDescriptor, repaints::PaintTimes) {
    let UiTarget { of, surface, set_up, renderer, ctx, label, offscreen } = target;
    let mut times = repaints::PaintTimes::default();
    let mut step = Instant::now();
    let mut lap = |into: &mut Duration| {
        let now = Instant::now();
        *into = now.duration_since(step);
        step = now;
    };
    let jobs = ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
    lap(&mut times.tessellate);
    for (id, deltas) in &full.textures_delta.set {
        for delta in deltas {
            renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
    }
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: offscreen.unwrap_or([set_up.size().0, set_up.size().1]),
        pixels_per_point: full.pixels_per_point,
    };
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let extra = renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
    lap(&mut times.upload);
    // A surface whose last set-up was lost still has the size before (or none): nothing is
    // drawn into it. Whoever draws the window sets the surface up again before a later drawing
    // and sets the time of that drawing (`SetUp::follow`, `SetUp::next_try`); nothing here asks
    // for the window to be drawn again at once, so a surface that keeps refusing costs little.
    let next = (offscreen.is_none() && set_up.drawable()).then(|| output::next_texture(surface));
    lap(&mut times.acquire);
    let frame = match next {
        // Drawn off the window (only into the capture's own picture), or not set up.
        None => None,
        Some(Some(wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame))) => {
            set_up.answered();
            Some(frame)
        }
        // Its pictures no longer fit the window, the system took it away, its set-up is not
        // valid (which is also all a graphics device that was lost says), or it was never set
        // up: set up again before a later drawing.
        Some(
            Some(
                wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost
                | wgpu::CurrentSurfaceTexture::Validation,
            )
            | None,
        ) => {
            set_up.refused();
            None
        }
        // The window is covered, or its picture didn't come in time: nothing is wrong.
        Some(Some(wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded)) => {
            set_up.answered();
            None
        }
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
        lap(&mut times.submit);
        gpu.queue.present(frame);
        lap(&mut times.present);
        surface_setup::race_aid_drew(of, screen.size_in_pixels);
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
    (jobs, screen, times)
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
            Panel::Screens => crate::i18n::t("notices.window.screens", "Screens"),
            Panel::Settings => crate::i18n::t("notices.window.settings", "Settings"),
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
            Panel::Settings => {
                state.preferences_open = false;
                state.keys_card.stop();
                crate::end_look::settings_closed(state);
            }
        }
    }
}

/// A window of its own with its own interface: Screens, or Settings.
struct PanelWindow {
    kind: Panel,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    /// How its surface is set up (see `surface_setup`).
    set_up: surface_setup::SetUp,
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    next_repaint: Option<Instant>,
    frames: u32,
    /// Its least size in points, which its interface size never lays it out under, and what
    /// that size is worked out from (see `interface_size::panel_zoom`).
    least: egui::Vec2,
    zoom_watch: interface_size::Watch,
}

/// Everything that exists once the windows are open.
struct Running {
    /// Listening for files dragged onto the operator's window on a Wayland desktop. This must
    /// stay the first field: fields are dropped in order, and the listening has to stop before
    /// the home window goes. It knows the window by the address of its surface, and once the
    /// window is gone that address may be given to another surface (another window's, say),
    /// which would then take drops meant for no one.
    file_drops: file_drop::Listening,
    home_window: repaints::HomeWindow,
    home_surface: wgpu::Surface<'static>,
    /// How the home window's surface is set up (see `surface_setup`).
    home_set_up: surface_setup::SetUp,
    /// A new size for the home window while its edge is being dragged, and when it last changed.
    /// Setting a window's drawing surface to a new size waits for everything the GPU is doing,
    /// the screens' pictures included, so it is done once the size has settled, never on each
    /// step of a drag: the audience must not see the operator resizing a window.
    home_resize: Option<(winit::dpi::PhysicalSize<u32>, Instant)>,
    /// Set when taking on a size live held the GPU for longer than `LIVE_RESIZE_WAIT`: the rest
    /// of that drag waits for the size to settle again, as before, until it ends.
    home_resize_held_back: bool,
    /// The windows audience screens are drawn in, one per screen that has one.
    screen_windows: Vec<ScreenWindow>,
    /// Windows of screens that were moved elsewhere or switched off, hidden, and dropped a
    /// little later once the output thread has let go of them.
    retired: Vec<(Arc<Window>, Instant)>,
    /// The NDI names the output thread was last told to send under, by screen.
    ndi_sent: Vec<(Uuid, String, screens::NdiRate)>,
    /// The screens the output thread was last told send no sound over NDI.
    ndi_silent_sent: Vec<Uuid>,
    /// The screens switched off that still send (on NDI or a DeckLink output) the output thread
    /// was last told to draw black.
    dark_sent: Vec<Uuid>,
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
    /// The end of a song's mark as the screens were last told of it with the slide
    /// (`Frame::end_mark`): when it comes to differ (Settings' switch, the song set to go round,
    /// its order changed), the slide is sent again.
    end_mark_sent: Option<f32>,
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
    /// The screens whose display is away, with the notice said for each (see `window_plan`).
    away: std::collections::BTreeMap<Uuid, String>,
    /// When the displays are next looked at though nothing has said they changed (a backstop to
    /// what the system tells us on macOS, the only way on the other systems), and the macOS
    /// watch that wakes the loop when they change.
    displays_look_at: Instant,
    #[cfg(target_os = "macos")]
    display_watch: Option<sp_video::DisplayWatch>,
    /// The live preview's two textures, as the window knows them.
    live_preview_ids: Vec<egui::TextureId>,
    /// The same two textures, for Remote Control to read the newest back from (never the
    /// output thread: it only draws into them).
    live_textures: Vec<wgpu::Texture>,
    /// The live preview's textures before they were made again at another size, shown until
    /// the output thread has drawn a picture since (it had drawn this many then), then let go.
    live_preview_before: Option<(Vec<egui::TextureId>, u64)>,
    /// What the home window's interface size is worked out from, and how much larger than
    /// usual its pictures of slides are drawn (see `interface_size`).
    zoom_watch: interface_size::Watch,
    picture_scale: f32,

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
    /// What the line under the preview saying what Next will put on screen was last worked out
    /// from (see `refresh_next_line`).
    next_line_from: Option<NextLineFrom>,
    frames: u32,
    /// The GPU instance, which outlives its surfaces and makes the surfaces of new screens,
    /// and which of its devices is in use.
    context: RenderContext,
    device_id: usize,
}

/// Everything what Next would do is worked out from (`App::next_up`), kept to tell when the
/// line under the preview must be worked out again: where the show is, the window's view, open
/// item and pick (`at`), the songs taken out, the media playlists, the playlist in front and
/// the passages the Bible page's places hold (which tell whether a reading on screen is a
/// playlist's passage; a passage looked up in a chapter already made changes no song).
/// Compared with what they are now on each frame without copying anything.
#[derive(Clone, PartialEq)]
struct NextLineFrom {
    at: NextLineAt,
    held: [home::Held; 2],
    removed: Vec<usize>,
    media_playlists: Vec<usize>,
    items: Vec<service::Item>,
}

/// The plain values of [`NextLineFrom`].
#[derive(Clone, Copy, PartialEq)]
struct NextLineAt {
    songs: u64,
    live: Option<sp_engine::SlideRef>,
    cursor: Option<sp_engine::SlideRef>,
    place: Option<(usize, usize)>,
    build: usize,
    // What Next puts on screen depends on it too (`Show::next_on_screen`).
    words_hidden: bool,
    selected: usize,
    picked: Option<sp_engine::SlideRef>,
    picked_word: Option<&'static str>,
    editing: bool,
    bible_open: bool,
    playlist_open: bool,
    playlist: usize,
    chosen: Option<usize>,
    live_item: Option<usize>,
    continuous: bool,
    keys: usize,
    media: usize,
    language: i18n::Language,
}

impl NextLineFrom {
    fn of(show: &Show, state: &HomeState) -> NextLineFrom {
        NextLineFrom {
            at: NextLineAt::of(show, state),
            held: state.bible.held.clone(),
            removed: state.removed_songs.clone(),
            media_playlists: state.media_playlists.iter().map(|playlist| playlist.files.len()).collect(),
            items: state.services.front().items.clone(),
        }
    }

    /// Whether everything is still as it was.
    fn holds(&self, show: &Show, state: &HomeState) -> bool {
        self.at == NextLineAt::of(show, state)
            && self.held == state.bible.held
            && self.removed == state.removed_songs
            && self.items == state.services.front().items
            && self
                .media_playlists
                .iter()
                .copied()
                .eq(state.media_playlists.iter().map(|playlist| playlist.files.len()))
    }
}

impl NextLineAt {
    fn of(show: &Show, state: &HomeState) -> NextLineAt {
        NextLineAt {
            songs: show.version(),
            live: show.live(),
            cursor: show.cursor(),
            place: show.position(),
            build: show.build_step(),
            words_hidden: show.words_hidden(),
            selected: show.selected(),
            picked: state.pick(show),
            picked_word: state.picked_word(show),
            editing: state.edit_kind().is_some(),
            bible_open: state.bible_open,
            playlist_open: state.playlist_open,
            playlist: state.services.current,
            chosen: state.services.chosen,
            live_item: state.live_item,
            continuous: state.continuous,
            keys: state.song_keys.len(),
            media: show.media().len(),
            language: i18n::current(),
        }
    }
}

/// A window an audience screen is drawn in.
struct ScreenWindow {
    screen: Uuid,
    window: Arc<Window>,
    placed: Placed,
    name: String,
    /// When the window was last put on its display.
    placed_at: Instant,
    /// Hidden (macOS keeps a hidden window to show again), with its picture let go.
    hidden: bool,
    /// The display's rectangle it was last put on, and how many times it has been put back since
    /// being found elsewhere (see `window_plan`).
    put_on: Option<displays::Rect>,
    tries: u8,
    /// Made as a macOS window that covers a display (see `put_on_display`).
    cover: bool,
    /// The X it shows when the pointer moves over it, while it fills a display, and how the
    /// screens' thread was last told to draw it (see `close_button`).
    close: close_button::CloseButton,
    close_sent: Option<close_button::Drawn>,
}

impl ScreenWindow {
    /// True when the window has the X: shown, and filling a display (or, with the testing aid,
    /// in an ordinary window).
    fn has_close_button(&self) -> bool {
        !self.hidden && close_button::has_close_button(&self.placed, close_button::in_windows_too())
    }
}

struct App {
    /// A song's last slide as sung (`end_mark::last_sung`), as last worked out: for which
    /// songs (`Show::version`), which presentation, and what it came to. Asked on every wake
    /// while the end of a song is marked or the copyright line shows on last slides.
    last_sung: std::cell::Cell<Option<(u64, usize, Option<end_mark::Last>)>>,
    options: Options,
    /// Wakes the event loop from other threads (the output thread, for a new live preview).
    waker: Option<winit::event_loop::EventLoopProxy<AppEvent>>,
    /// Song packs written or read on threads of their own report here.
    pack_tx: std::sync::mpsc::Sender<PackJob>,
    pack_rx: std::sync::mpsc::Receiver<PackJob>,
    /// Bible files read and installed on threads of their own report here.
    bible_tx: std::sync::mpsc::Sender<BibleWork>,
    bible_rx: std::sync::mpsc::Receiver<BibleWork>,
    /// Song packs being written or read in the background, by number, with how each went once
    /// done (`sp job N`, and `--wait`); the number of the last one started.
    jobs: Vec<(u64, Option<Result<String, String>>)>,
    last_job: Option<u64>,
    /// A word made at each start, which a job's replies carry as `run`: job numbers start again
    /// from 1 with the app, so job 1 of an earlier run (an assistant's task) can be told from
    /// this run's.
    run_word: String,
    /// While `sp song import … --library NAME|none` runs: the library its songs (a folder's
    /// included) go into, or none.
    import_library_override: Option<Option<Uuid>>,
    /// Files dropped on the window, waiting until all of one drop have come, and where the
    /// pointer was at the drop (see `finish_drop`).
    dropped: Vec<PathBuf>,
    drop_pointer: Option<egui::Pos2>,
    /// When the first file of the drop waiting came, and the time of day then (for the log).
    drop_came: Option<(Instant, chrono::DateTime<chrono::Local>)>,
    /// The file window or question open, if any, and what it is for (see `dialogs`).
    dialogs: dialogs::Dialogs<dialogs::For>,
    /// Bible downloads started from the command line: each job's number, the Bible's id and name.
    bible_jobs: Vec<(u64, String, String)>,
    /// `sp update check`s waiting for the check that is running: their job numbers.
    update_jobs: Vec<u64>,
    /// What the window has said of fetching the videos' FFmpeg, and the videos added while it
    /// was missing (see `ffmpeg_fetch`).
    ffmpeg_fetch: ffmpeg_fetch::Told,
    /// The socket `softpresenter ctl` reaches the app through, while it listens.
    _control: std::sync::Arc<std::sync::Mutex<Option<control::Listening>>>,
    /// `status --wait`s held until something changes or their time is up.
    waiting: Vec<control::Waiter>,
    /// AirPlay: the program that receives an iPhone's, iPad's or Mac's screen, while
    /// switched on.
    mirror: mirror::Mirror,
    /// The display kept awake while the screens are on (see `keep_awake`), told by
    /// `sync_screens` and let go for good by the quit.
    keep_awake: keep_awake::KeepAwake,
    /// Remote Control: the web page other devices run the show from, while switched on.
    remote: remote::Remote,
    /// Whether Remote Control, left on, has been switched on again since the app opened.
    remote_started: bool,
    /// How many pages were connected when Settings and the status line were last told.
    remote_pages_seen: usize,
    /// True while a command from a remote page is carried out: the handlers that would open a
    /// file or folder on the computer refuse then (`only_at_the_computer`), whatever the policy let by.
    remote_call: bool,
    /// The same while an AI assistant's command (`sp mcp`) is carried out (`mcp::policy`).
    mcp_call: bool,
    /// The jobs an AI assistant started (a Bible download), so the log says how each ended.
    mcp_jobs: mcp::Jobs,
    show: Show,
    /// The file each song came from, in the show's order; `None` for the bundled sample.
    sources: Vec<Option<PathBuf>>,
    /// The presentations kept for Bible passages.
    bible_slots: [usize; 2],
    /// Where the VoxLive item is among the presentations (see `voxlive::item`).
    voxlive_slot: usize,
    /// The two presentations kept for a song of the song catalog being looked at (see
    /// `catalog_songs`).
    catalog_slots: [usize; 2],
    /// The VoxLive screens the item in its place was last made from, and that item.
    voxlive_item_from: Option<(Vec<voxlive::Screen>, Arc<sp_model::Presentation>)>,
    /// The slide on screen, the item it was put on screen from and the open item when the
    /// theme version last followed them (see `follow_version`).
    version_seen: Option<(sp_engine::SlideRef, Option<usize>, Option<usize>)>,
    /// PDFs and folders of pictures being made into slides, each on a thread of its own.
    decks: deck::Decks,
    /// Whether the decks an earlier run left unfinished have been taken up (once, at the start).
    decks_resumed: bool,
    /// The media items' pictures, by item: a small copy of each for the window, and the
    /// full-size pictures of the item on screen and a few others for the screens.
    bin_pictures: bin_pictures::BinPictures,
    /// When the bin's small copies started being read at the start, and how many there are,
    /// until they all have been (for the log).
    bin_started: Option<(Instant, usize)>,
    /// Pictures and videos named on the command line, put in the media bin once the start has
    /// opened the window (`fill_media_bin`).
    media_named: Vec<PathBuf>,
    /// Videos added to the bin by hand this run whose picture is still being read by a reader
    /// (see `add_media_file`): one that can't be read says so as it leaves the bin.
    added_by_hand: std::collections::HashSet<Uuid>,
    /// Songs changed in the Edit view and not yet written to their files, and when the last
    /// change was made.
    unsaved: Vec<usize>,
    /// How each song and theme file was when the app last read or wrote it, so a file changed
    /// outside the app is never saved over (see `outside`).
    stamps: outside::Stamps,
    /// The thread that writes songs, playlists, libraries and the media bin, so the window never
    /// waits for the disk (see `writer`).
    writer: writer::Writer,
    /// Songs and themes with changes not saved because their file changed outside the app; they
    /// wait on Reload or Keep mine.
    held_songs: Vec<usize>,
    held_themes: Vec<usize>,
    /// The held songs and themes already written to "Not saved" while quitting (see
    /// `keep_held_aside`), so a later look writes only those held since.
    kept_aside: (Vec<usize>, Vec<usize>),
    /// The files whose failed save was written to "Not saved" while quitting (see
    /// `keep_failed_aside`), each once.
    kept_failed: Vec<PathBuf>,
    /// Songs and themes whose save failed, tried again after a wait (`writer::Retries`).
    retries: writer::Retries,
    /// The changes made to songs in the Service view, to undo and redo.
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
    /// The video that was on screen before it, still playing there: the screens keep it until
    /// the first picture of the one that took its place has come (see `refresh_output`).
    video_leaving: Option<sp_video::Player>,
    /// The picture behind the words the screens were last told to show, and how it fills them:
    /// what they keep while a video's first picture is still to come.
    media_on_screens: (Option<sp_render::Picture>, sp_model::ImageFit),
    /// The wait for the video on screen's first picture, from when it was opened: the screens
    /// keep what they showed until it comes, for at most `video_wait::LIMIT` (`None` for a video
    /// ready at once, or none).
    video_wait: Option<video_wait::FirstPicture>,
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
    /// The NDI sources slides on screen or being edited show: finding and receiving them.
    ndi: ndi_in::Inputs,
    /// `sp_render::ndi_changes` as the editor's picture was last drawn for it, and when.
    ndi_seen: u64,
    ndi_drawn: Option<Instant>,
    /// The connection to the church's VoxLive account.
    voxlive: voxlive::Client,
    /// Canva Sync: the church's Canva sign-in, its design, and a sync under way.
    canva: canva::Client,
    /// The song catalog: downloaded songs, kept apart from the library (see `catalog_cli`).
    catalog: catalog_cli::CatalogState,
    /// The free songbooks shared by churches being downloaded (see `free_songbooks_cli`).
    free_songbooks: free_songbooks_cli::Installs,
    /// Settings › Song files' Find songs on this computer: the search and the import under way.
    finding: find_songs_cli::Finding,
    /// The connection to the church's TimelyChurch: its services as playlists, its songs in the
    /// library, and the songs shown told to it (see `timelychurch`).
    timelychurch: timelychurch::Connection,
    /// The preview of a VoxLive screen's look being changed, while its page is open.
    voxlive_preview: Option<LookPreview>,
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
    /// What the media layer, and the Announcements layer, playing from a followed folder
    /// remember between looks (see `followed`).
    following: [followed::Following; 2],
    /// For each of those layers, the wait for a file lined up to follow its video to give its
    /// first picture, and what the operator was told when it didn't (see `video_wait::LinedUp`).
    lined_up_wait: [video_wait::LinedUp; 2],
    /// The video on the media layer that FFmpeg (or the system) said can't play, by its media
    /// bin item: a playing media playlist goes on from it.
    video_refused: Option<Uuid>,
    /// What the operator was told of videos a playing media playlist went on from because no
    /// picture came in time, by their media bin item: taken back once that item plays.
    skipped_videos: HashMap<Uuid, String>,
    /// How many items in a row the playing media playlist has gone on from without any of them
    /// playing (videos that wouldn't): a whole round of them, and it stops going round.
    cycle_failed: usize,
    /// The media playlists read at the start, handed to the window's state once it is made.
    media_playlists: Vec<media::Playlist>,
    /// The settings files the start couldn't read, kept aside (`whole_file`): told in the notices
    /// once the window's state is made.
    unread: Vec<whole_file::Unread>,
    /// The small settings files' saves, handed to the library's writer at most every
    /// `writer::PACE` each, so the window never waits for the disk.
    paced: writer::Paced,
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
    /// The start failed because nothing here can draw: no graphics device could be made, or
    /// the graphics refused the painter's shaders. Only that failure is shown in a message
    /// window (`no_graphics::Failure::show_window`); any other is said on the standard error,
    /// as before.
    no_graphics: Option<no_graphics::Failure>,
    /// Set when a test capture has been saved and the program should end.
    capture_done: bool,
    /// How often and how long the operator window is drawn, for the log.
    home_timing: applog::Timing,
    /// When quitting began, while it waits for Canva Sync to keep a sign-in it is refreshing.
    quitting: Option<Instant>,
    /// Windows is told the app is saving (`say_saving_to_windows`).
    #[cfg(windows)]
    saying_saving: bool,
    /// Testing aid: SOFTPRESENTER_END_SESSION_WHEN has been acted on (see `about_to_wait`).
    #[cfg(windows)]
    end_session_asked: bool,
    /// Testing aid: SOFTPRESENTER_TERMINATE_AFTER has been acted on (see `about_to_wait`).
    #[cfg(target_os = "macos")]
    terminate_asked: bool,
    /// The app's leaving has been done, by its own Quit or by `leave_at_once`: never twice.
    left: bool,
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
        start_steps::mark("event loop started");
        let monitors: Vec<_> = el.available_monitors().collect();
        let displays = display_list(monitors.iter().cloned(), el.primary_monitor());
        start_steps::mark("displays listed");
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
        // The window opens inside the work area of the display it opens on (the part the
        // taskbar or the menu bar leaves), no larger than it, and centred: the operating
        // system's own place for a new window cascades down and off a small screen.
        let first_monitor = el.primary_monitor().or_else(|| monitors.first().cloned());
        let scale = first_monitor.as_ref().map_or(1.0, |m| m.scale_factor());
        let work = first_monitor.as_ref().map(|m| work_area(m, scale));
        let (frame_w, frame_h) = ((16.0 * scale) as u32, (40.0 * scale) as u32);
        let wanted_inner = ((width * scale) as u32, (height * scale) as u32);
        let mut home_attrs = app_window(Window::default_attributes()).with_title("SoftPresenter");
        let min_inner = (1100.0 * scale, 700.0 * scale);
        match work {
            Some(work) => {
                let put = displays::place_home_window(
                    (wanted_inner.0 + frame_w, wanted_inner.1 + frame_h),
                    None,
                    work,
                    &[work],
                );
                home_attrs = home_attrs
                    .with_inner_size(winit::dpi::PhysicalSize::new(
                        put.w.saturating_sub(frame_w).max(1),
                        put.h.saturating_sub(frame_h).max(1),
                    ))
                    .with_min_inner_size(winit::dpi::PhysicalSize::new(
                        (min_inner.0 as u32).min(put.w.saturating_sub(frame_w).max(1)),
                        (min_inner.1 as u32).min(put.h.saturating_sub(frame_h).max(1)),
                    ))
                    .with_position(winit::dpi::PhysicalPosition::new(put.x, put.y));
            }
            None => {
                home_attrs = home_attrs
                    .with_inner_size(winit::dpi::LogicalSize::new(width, height))
                    .with_min_inner_size(winit::dpi::LogicalSize::new(1100.0, 700.0));
            }
        }
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
        start_steps::mark("window made");
        // On Windows, a log-off, a restart or a shut-down is heard on the operator's window, so
        // the app saves and stops before Windows may end it (`leave_for_session_end`).
        #[cfg(windows)]
        if !win32_handle(&home_window).is_some_and(sp_winapi::on_session_end) {
            applog::problem("quit", "could not listen for Windows ending the session: a log-off may not save first");
        }
        applog::event("start", &format!("steps: {}", start_steps::said()));
        // Where the videos' FFmpeg is, or where it was looked for: a tester's copy said FFmpeg
        // wasn't on the computer, from a folder the log couldn't name.
        match sp_video::ffmpeg_program() {
            Some(found) => applog::event("video", &format!("FFmpeg: {}", found.display())),
            None => {
                let places = sp_video::ffmpeg_places();
                let first = places.first().and_then(|place| place.parent()).map(|dir| dir.display().to_string());
                applog::event(
                    "video",
                    &format!(
                        "FFmpeg was not found: not beside the program ({}), nor in the {} other places looked in",
                        first.unwrap_or_default(),
                        places.len().saturating_sub(1)
                    ),
                );
            }
        }
        // The frame is known now: if the guess at it left the window off the work area, move it.
        if let (Some(work), Ok(at)) = (work, home_window.outer_position()) {
            let size = home_window.outer_size();
            let put = displays::place_home_window((size.width, size.height), Some((at.x, at.y)), work, &[work]);
            if (put.x, put.y) != (at.x, at.y) {
                home_window.set_outer_position(winit::dpi::PhysicalPosition::new(put.x, put.y));
            }
            let (w, h) = (size.width, size.height);
            applog::event(
                "window",
                &format!(
                    "home window {w}x{h} at {},{} (asked for {},{}); work area {}x{} at {},{}",
                    at.x, at.y, put.x, put.y, work.w, work.h, work.x, work.y
                ),
            );
        }
        let operator = operator_displays(&monitors, home_window.current_monitor(), el.primary_monitor());

        // A display asked for on the command line must exist. The screens' windows are opened
        // once the app has started.
        choose_output(&displays, &operator, self.options.output_display)?;

        let mut context = RenderContext::new();
        context.memory_hints = gpu_memory_hints();
        let home_surface = context
            .instance
            .create_surface(home_window.clone())
            .map_err(|e| format!("Could not draw to the home window: {e}"))?;
        let making = Instant::now();
        let Some(device_id) = pollster::block_on(context.device(Some(&home_surface))) else {
            // Why, for the log: the graphics that were tried, and what each said (its
            // compiler refusing a shader the graphics library builds for itself, say).
            let why = if context.refused.is_empty() {
                "this computer listed none that can draw to a window".to_owned()
            } else {
                context.refused.join("; ")
            };
            applog::problem(
                "gpu",
                &format!("No graphics device could be used, after {:.1} s: {why}", making.elapsed().as_secs_f32()),
            );
            let failure = no_graphics::Failure::NoDevice;
            let words = failure.words(no_graphics::System::here());
            self.no_graphics = Some(failure);
            return Err(words);
        };
        let handle = &context.devices[device_id];
        // Which graphics the app drew with, for the log of a computer that draws slowly: and
        // whether the painters take the early stages of every picture off them
        // (`sp_render::early_stages_on_processor`): Windows' software graphics, or an Intel
        // Mac's.
        let chosen = handle.adapter().get_info();
        let stages = if !sp_render::early_stages_on_processor(&chosen) {
            ""
        } else if chosen.backend == wgpu::Backend::Metal {
            "; an Intel Mac's graphics: pictures are worked out on the processor and filled in by the GPU"
        } else {
            "; software graphics: pictures are worked out on the processor"
        };
        applog::event(
            "gpu",
            &format!(
                "{} ({:?}, {:?}, driver {} {}; memory {:?}{stages})",
                chosen.name,
                chosen.backend,
                chosen.device_type,
                chosen.driver,
                chosen.driver_info,
                context.memory_hints
            ),
        );
        reports::gpu_chosen(&chosen);
        let gpu = Gpu { device: handle.device.clone(), queue: handle.queue.clone() };
        // By default a graphics error ends the program; in a service it is better to say so and carry on.
        gpu.device.on_uncaptured_error(Arc::new(|error| graphics_error(&error.to_string())));
        // A graphics device that is lost (the driver was restarted or stopped answering, the
        // graphics card was taken away) is not made again: nothing can be drawn with it any
        // more, in the windows or on the screens, until SoftPresenter is started again. The
        // graphics library keeps the errors that follow to itself, so it is said here, once.
        gpu.device.set_device_lost_callback(|reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                applog::problem(
                    "gpu",
                    &format!(
                        "The graphics device was lost ({message}). Nothing can be drawn until SoftPresenter is \
                         started again."
                    ),
                );
            }
        });
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
        // Testing aid: SOFTPRESENTER_SURFACE_RACE (see `surface_setup::race_aid_asked`).
        let (width, height) = surface_setup::race_aid_start((size.width, size.height));
        let size = winit::dpi::PhysicalSize::new(width, height);
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
        let longest_side = gpu.device.limits().max_texture_dimension_2d;
        let home_set_up = surface_setup::SetUp::new(home_config, longest_side, Instant::now(), |config| {
            surface_setup::set_up(&gpu.device, &home_surface, config, surface_setup::Of::Operator)
        });

        let egui_ctx = egui::Context::default();
        studio::apply_style(&egui_ctx);
        repaints::watch(&egui_ctx);
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
        // A GPU whose compiler won't take the painter's shaders: said now, by the pipeline's
        // name, where it would otherwise only show as pictures never drawn.
        let mut thumbs = match Thumbs::new(&gpu.device, &gpu.queue) {
            Ok(thumbs) => thumbs,
            Err(why) => {
                applog::problem(
                    "gpu",
                    &format!("The graphics ({}) can't run the painter's shaders: {why}", chosen.name),
                );
                let failure = no_graphics::Failure::refused(&why, &chosen.name);
                let words = failure.words(no_graphics::System::here());
                self.no_graphics = Some(failure);
                return Err(words);
            }
        };
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
        output.send(OutputMsg::PreviewTargets(live_textures.clone()));

        let (services, unread) = service::load();
        self.unread.extend(unread);
        Ok(Some(Running {
            file_drops: file_drop::Listening::start(&home_window, self.waker.clone()),
            home_window: repaints::HomeWindow::new(home_window),
            home_surface,
            home_set_up,
            home_resize: None,
            home_resize_held_back: false,
            screen_windows: Vec::new(),
            retired: Vec::new(),
            ndi_sent: Vec::new(),
            ndi_silent_sent: Vec::new(),
            dark_sent: Vec::new(),
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
            end_mark_sent: None,
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
            live_textures,
            live_preview_before: None,
            zoom_watch: interface_size::Watch::default(),
            picture_scale: 1.0,
            ndi_checked: None,
            displays_seen: None,
            away: std::collections::BTreeMap::new(),
            displays_look_at: Instant::now() + DISPLAYS_LOOK_EVERY,
            #[cfg(target_os = "macos")]
            display_watch: None,

            preview_before: None,
            preview_changed: Instant::now(),
            preview_shows_stage: false,
            preview_screen_shown: None,
            stage_preview_at: Instant::now(),
            home_state: HomeState {
                fonts: Arc::new(sp_render::Fonts::new().family_names()),
                theme_for_songs: self.songs_theme,
                theme_layouts: self.theme_layouts(),
                template_names: self.themes.iter().map(template_names).collect(),
                versions: sp_model::all_versions(&self.themes),
                song_templates: song_templates(&self.theme),
                bible_layout: self.bible_layout,
                services,
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
                        } else if let Some(place) = self.catalog_slots.iter().position(|slot| *slot == at) {
                            catalog_songs::SLOT_KEYS[place].to_owned()
                        } else {
                            song_ids::key_for(source.as_deref(), song.id, &song.title)
                        }
                    })
                    .collect(),
                voxlive_slot: Some(self.voxlive_slot),
                // The window looks in the song catalog where it is offered.
                catalog: catalog_songs::CatalogSongs::new(
                    self.catalog.catalog.clone().filter(|_| catalog::offered()),
                    self.catalog_slots,
                ),
                theme_for_bible: self.bible_theme,
                bible: home::BiblePage {
                    slots: Some(self.bible_slots),
                    shelf: bible::Shelf::with_installed(),
                    boxes: home::layout_boxes(&self.theme_for_bible, self.bible_layout),
                    box_labels: home::layout_labels(&self.theme_for_bible, self.bible_layout),
                    template: self.bible_template(),
                    ..home::BiblePage::default()
                },
                // The themes' names, ids and end marks, as every later change to them lists them.
                ..HomeState::listing_themes(&self.themes)
            },
            next_repaint: None,
            canvas: None,
            canvas_for: None,
            next_line_from: None,
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
        // A slide going on screen takes the announcements off every screen that shows slides, until
        // they are started again (told to the screens before the slide, so it comes with the
        // slide's own transition).
        if self.show.live().is_some() && self.show.live() != live_before {
            self.announcements_off_for_slide();
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
            && let Some(title) = self.show.presentations().get(at.presentation).map(|song| song.title.clone())
            && let Some(running) = self.running.as_mut()
        {
            // The passage asked for, as it is kept, not the chapter its slides were made from.
            let bible = &mut running.home_state.bible;
            let (passage, numbering) = bible.passage_held(at.presentation, &title);
            bible.note_shown(&passage, numbering, chrono::Local::now().naive_local());
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

    /// Says once, in the notices, when a newer build has been installed over this copy while it
    /// runs, so whoever runs it knows to quit and reopen it. Looked at about once a minute.
    fn say_if_newer_installed(&mut self) {
        static LAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if SAID.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let Ok(mut last) = LAST.lock() else { return };
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last = Some(Instant::now());
        if !newer_program_installed() {
            return;
        }
        SAID.store(true, std::sync::atomic::Ordering::Relaxed);
        applog::event("update", "a newer program file was installed while this copy runs");
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.info(i18n::t(
                "common.newer_installed",
                "A newer SoftPresenter has been installed. Quit and reopen SoftPresenter to use it.",
            ));
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
            repaints::wake_at(&mut running.next_repaint, due, repaints::Timer::Advance);
        }
    }

    /// Plays a media playlist item after item on the media layer, starting with its first.
    fn play_media_playlist(&mut self, id: Uuid) {
        self.check_folders(true);
        let Some(first) = self.playlist_media(id).first().copied() else {
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.refuse(crate::i18n::t(
                    "notices.media_playlist.empty",
                    "That media playlist has nothing in it to play.",
                ));
                running.home_window.request_redraw();
            }
            return;
        };
        self.media_cycle = Some(MediaCycle { playlist: id, at: 0, since: Instant::now(), shown: first });
        self.cycle_failed = 0;
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
        let next = items[media::next_in_cycle(&items, cycle.at, cycle.shown)];
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

    /// The media bin places of a media playlist's files, in its order (as its view shows them).
    fn playlist_media(&self, id: Uuid) -> Vec<usize> {
        let Some(running) = self.running.as_ref() else { return Vec::new() };
        let Some(playlist) = running.home_state.media_playlists.iter().find(|p| p.id == id) else { return Vec::new() };
        media::playlist_places(&playlist.files, self.show.media())
    }

    /// Whether the media playlist playing goes on from the video on screen if it won't play: it
    /// shows that video, has other items to go on to, and it wouldn't be the last of a whole
    /// round that wouldn't play (where the playlist stops going round instead,
    /// `none_would_play`). With `now`, only once the video has been up as long as a picture,
    /// when it goes on at once.
    fn cycle_goes_on_from_video(&self, now: bool) -> bool {
        let Some(cycle) = self.media_cycle.as_ref() else { return false };
        let items = self.playlist_media(cycle.playlist).len();
        self.show.live_media() == Some(cycle.shown)
            && self.show.media().get(cycle.shown).is_some_and(|item| media::is_video(&item.path))
            && items > 1
            && self.cycle_failed + 1 < items
            && (!now || cycle.since.elapsed() >= Duration::from_secs_f32(PICTURE_SECONDS))
    }

    /// Whether a media playlist playing shows the media on screen (and so comes back to it each
    /// round).
    fn playlist_shows_live_media(&self) -> bool {
        self.media_cycle.as_ref().is_some_and(|cycle| self.show.live_media() == Some(cycle.shown))
    }

    /// A whole round of the media playlist playing went by and none of its videos would play:
    /// it stops going round, on the last of them (its still, or nothing), and the operator is
    /// told once.
    fn none_would_play(&mut self, playlist: Uuid) {
        let name = self
            .running
            .as_ref()
            .and_then(|running| running.home_state.media_playlists.iter().find(|p| p.id == playlist))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        applog::event(
            "media",
            &format!(
                "a whole round of the media playlist {name} went by and none of its videos would play: it stops going round"
            ),
        );
        self.stop_media_playlist();
        self.cycle_failed = 0;
        let said = crate::i18n::tf(
            "notices.media_playlist.none_played",
            "None of the videos in \u{201c}{playlist}\u{201d} would play, so it stopped going round.",
            &[("playlist", &name)],
        );
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.lasting_trouble(said);
            running.home_window.request_redraw();
        }
    }

    /// Moves a playing media playlist on: a picture after 8 seconds, a video when it ends (or
    /// when it never starts or can't play: `media::video_done`). It stops if the operator puts
    /// other media on, or clears it. Says when to look again.
    fn tick_media_cycle(&mut self) -> Option<Instant> {
        let cycle = self.media_cycle.as_ref()?;
        let (id, at, since, shown) = (cycle.playlist, cycle.at, cycle.since, cycle.shown);
        if self.show.live_media() != Some(shown) {
            self.stop_media_playlist();
            return None;
        }
        let items = self.playlist_media(id);
        let is_video = self.show.media().get(shown).is_some_and(|item| media::is_video(&item.path));
        let dwelt = since.elapsed() >= Duration::from_secs_f32(PICTURE_SECONDS);
        let (done, wont_play) = if is_video {
            // One video on its own loops, so it never ends; with others, it plays to its end, or
            // is gone on from when it never starts or can't play, once it has been up as long
            // as a picture.
            let shown_id = self.show.media().get(shown).map(|item| item.id);
            let ended = self.video.as_ref().is_some_and(|(_, player)| player.ended());
            let overdue = self.video.as_ref().is_some_and(|(id, _)| Some(*id) == shown_id)
                && self.video_wait.as_ref().is_some_and(video_wait::FirstPicture::overdue);
            let refused = shown_id.is_some() && self.video_refused == shown_id;
            let wont_play = !ended && (overdue || refused);
            (media::video_done(items.len() > 1, ended, wont_play, dwelt), wont_play)
        } else {
            (dwelt, false)
        };
        if done && items.len() > 1 {
            // A whole round of items that wouldn't play: it stops going round, on the last of
            // them, and says so once.
            self.cycle_failed = if wont_play { self.cycle_failed + 1 } else { 0 };
            if self.cycle_failed >= items.len() {
                self.none_would_play(id);
                return None;
            }
            // After the one on screen, wherever it is now: its playlist may have been reordered,
            // or had things taken out, while it played.
            let next_at = media::next_in_cycle(&items, at, shown);
            let next = items[next_at];
            self.media_cycle = Some(MediaCycle { playlist: id, at: next_at, since: Instant::now(), shown: next });
            self.apply(vec![Command::ShowMedia(next)]);
            self.read_ahead_in_cycle();
        }
        Some(Instant::now() + Duration::from_millis(if is_video { 200 } else { 500 }))
    }

    /// Keeps the media playlists that follow folders up to date: has the folders looked at, on
    /// a thread of its own, every few seconds (every second while a layer waits for a new file,
    /// see `folder_look_every`), or at once when `now`. `finish_folder_look` takes in what was
    /// found.
    fn check_folders(&mut self, now: bool) {
        if !now && self.folders_checked.elapsed() < self.folder_look_every() {
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
    /// threads) and go in once they have been; one written over is read again (a video playing
    /// goes on to the end of its pass, then the new file takes over); the playlists list what
    /// their folders hold; and
    /// an item a folder brought in whose file has gone leaves the bin, unless something else
    /// uses it.
    fn finish_folder_look(&mut self) {
        let Some(listings) = self.folder_look.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        self.folder_look = None;
        let Some(running) = self.running.as_ref() else { return };
        // A playlist that stopped following that folder while it was looked at is left alone.
        let playlists = &running.home_state.media_playlists;
        // A Canva video a new one replaced, waiting to be deleted once nothing plays it, is not
        // brought back into the bin meanwhile.
        let replaced = &self.canva.old_files;
        let listings: Vec<media::Listing> = listings
            .into_iter()
            .filter(|(id, folder, _)| playlists.iter().any(|p| p.id == *id && p.folder.as_ref() == Some(folder)))
            .map(|(id, folder, files)| {
                let files = files.map(|files| files.into_iter().filter(|(path, _)| !replaced.contains(path)).collect());
                (id, folder, files)
            })
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
            // A video playing goes on to the end of its pass, and the new file takes over there:
            // nothing is cut or started again. One that can't be lined up starts again from
            // the new file at once, as before.
            if media::is_video(&path)
                && let Err(problem) = self.play_new_file_next(id, &path)
            {
                applog::problem("video", &problem);
                if live == Some(id) {
                    if let Some((_, mut player)) = self.video.take() {
                        say_video_pace(&mut player, true);
                    }
                    media_changed = true;
                }
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

    /// A media bin item's file has a new version (written over, or put in its place): wherever
    /// that item's video plays (on the media layer, or the Announcements layer), the new file is
    /// opened now and takes over at the end of the pass under way, where the video would go
    /// round or end, so the loop carries on with the new pictures. A video one of several in a
    /// media playlist going round is left alone: the playlist opens the new file next time round.
    /// The error is a sentence; the old file then plays on.
    fn play_new_file_next(&mut self, id: Uuid, path: &std::path::Path) -> Result<(), String> {
        let one_of_many = self.media_cycle.as_ref().is_some_and(|cycle| self.playlist_media(cycle.playlist).len() > 1);
        if !one_of_many && let Some((_, player)) = self.video.as_mut().filter(|(playing, _)| *playing == id) {
            player.play_next(path)?;
            applog::event("video", &format!("{} takes over at the end of the pass", file_name(path)));
        }
        self.play_next_on_layer(id, path)
    }

    /// Plays the video on screen, if the media on screen is one, and stops the one that was.
    fn sync_video(&mut self) {
        let live = self
            .show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .filter(|item| media::is_video(&item.path))
            .cloned();
        if self.video_refused.is_some() && self.video_refused != live.as_ref().map(|item| item.id) {
            self.video_refused = None;
        }
        // One that said it can't play isn't opened again while it stays on screen: each try
        // runs FFmpeg's prober, and the screens would be told something new each time.
        if self.video.is_none() && self.video_refused.is_some() {
            return;
        }
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
        if let Some((_, mut player)) = self.video.take() {
            say_video_pace(&mut player, true);
            applog::event("video", "stopped");
            // Another video takes its place: it plays on, on the screens, until that one's first
            // picture has come (one that never reached the screens itself is let go).
            if live.is_some() && player.ready() {
                self.video_leaving = Some(player);
            }
        }
        self.end_video_wait();
        if live.is_none() {
            self.video_leaving = None;
        }
        let Some(running) = self.running.as_mut() else { return };
        running.home_state.video_live = None;
        let Some(item) = live else { return };
        let started = Instant::now();
        let loops = self.loops_now(&item);
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else { return };
        // The video behind the slide goes to the GPU from this thread only (the screens'), so
        // FFmpeg may give its pictures as planes the GPU turns into colours. A video FFmpeg
        // plays is never waited for here: FFmpeg is asked what it is and started on the
        // player's own thread, which says in the log how long each step took (`tick_video`).
        match sp_video::Player::open(&item.path, sp_video::Lend::Planes) {
            Ok(mut player) => {
                let said = if player.uses_ffmpeg() {
                    format!("opening {} with FFmpeg", item.path.display())
                } else {
                    format!(
                        "opened {} with the system's player in {:?}, {}",
                        item.path.display(),
                        started.elapsed(),
                        fps_said(player.fps())
                    )
                };
                applog::event("video", &said);
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
                // The screens wait for its first picture from now, for so long at most.
                self.video_wait =
                    (!player.ready()).then(|| video_wait::FirstPicture::new(started, video_wait::limit()));
                self.video = Some((item.id, player));
            }
            Err(problem) => {
                applog::event("video", &format!("could not open {}: {problem}", item.path.display()));
                // A file just taken out of a followed folder: the layer goes on to what is left
                // (see `followed`), with nothing to tell the operator.
                if !self.gone_from_followed_folder(&item.path) {
                    self.video_refused = Some(item.id);
                    // FFmpeg missing: fetched again where it can be, and said so (once, for a
                    // media playlist coming back to it every round).
                    let problem = self.video_problem(&item.path, problem);
                    let in_playlist = self.playlist_shows_live_media();
                    if let Some(running) = self.running.as_mut() {
                        if in_playlist {
                            running.home_state.notices.lasting_trouble(problem);
                        } else {
                            running.home_state.notices.trouble(problem);
                        }
                    }
                }
            }
        }
    }

    /// The shapes, pictures, videos and web pages the slide on screen's layout adds to it, both
    /// those behind its words and those in front of them: its theme's template, or the version's
    /// while a version its theme has is on.
    fn live_layout_objects(&self) -> Option<Slide> {
        let (at, (_, slide)) = (self.show.live()?, self.show.live_slide()?);
        let theme = self.theme_of(at.presentation);
        let version = self.running.as_ref().and_then(|running| running.home_state.version_switch.version());
        let shown = version.and_then(|version| theme.slide_in_version(slide, version));
        sp_render::layout_objects(shown.as_ref().unwrap_or(slide), theme)
    }

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
        if let Some((_, slide)) = self.show.live_slide() {
            videos_in(&slide.objects, &mut wanted);
            if let Some(behind) = self.live_layout_objects() {
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
            match sp_video::Player::open(&path, sp_video::Lend::Rows) {
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
                    let problem = self.video_problem(&path, problem);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.notices.trouble(problem);
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
                let place = object.frame.unwrap_or(sp_model::WHOLE_SLIDE);
                let key = sp_render::web_key(url, place.w, place.h);
                match into.iter_mut().find(|(have, ..)| *have == key) {
                    Some(found) => found.4 |= live,
                    None => into.push((key, url.clone(), place.w.round() as u32, place.h.round() as u32, live)),
                }
            }
        }
        let mut wanted = Vec::new();
        if let Some((_, slide)) = self.show.live_slide() {
            boxes_in(&slide.objects, true, &mut wanted);
            if let Some(behind) = self.live_layout_objects() {
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
        let (ndi_first, ndi_fresh, ndi_at) = self.tick_ndi();
        let (mirror_first, mirror_fresh) = self.mirror_pictures();
        if self.web_pages.is_empty() && ndi_first.is_empty() && ndi_fresh.is_empty() && !mirror_first && !mirror_fresh {
            return ndi_at;
        }
        let now = Instant::now();
        // The addresses of the pages with a new picture, and of those with their first (an NDI
        // source's is `ndi:` and its name, as `thumbs::web_pages` says it).
        let mut fresh: Vec<String> = ndi_fresh.iter().map(|source| format!("ndi:{source}")).collect();
        let mut first: Vec<String> = ndi_first.iter().map(|source| format!("ndi:{source}")).collect();
        // A phone's screen in a box is `mirror:`, as `thumbs::web_pages` says it.
        if mirror_fresh {
            fresh.push("mirror:".to_owned());
        }
        if mirror_first {
            first.push("mirror:".to_owned());
        }
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
        let web_at = self.web_pages.iter().map(|web| web.asked.unwrap_or(now) + web.every()).min();
        [web_at, ndi_at].into_iter().flatten().min()
    }

    /// The NDI sources the audience screens show now (the slide on screen, its layout behind it,
    /// and the stage layouts), each with whether every box showing it wants the light stream.
    fn live_ndi_wanted(&self) -> Vec<(String, bool)> {
        let mut wanted = Vec::new();
        if let Some((_, slide)) = self.show.live_slide() {
            ndi_in::wanted_in(&slide.objects, &mut wanted);
            if let Some(behind) = self.live_layout_objects() {
                ndi_in::wanted_in(&behind.objects, &mut wanted);
            }
        }
        if let Some(running) = self.running.as_ref() {
            ndi_in::wanted_in(&stage::shown_objects(&running.home_state), &mut wanted);
        }
        wanted
    }

    /// Starts receiving the NDI sources the slide on screen or being edited shows, and stops
    /// receiving the rest; keeps what the inspector shows of NDI, and tells the operator when a
    /// source stops. Nothing is received (and nothing looked for) while no such slide is shown.
    fn sync_ndi(&mut self) -> ndi_in::Report {
        let mut wanted = self.live_ndi_wanted();
        let Some(running) = self.running.as_mut() else { return ndi_in::Report::default() };
        let mut choosing = false;
        if let Some(draft) = running.home_state.editing.as_ref() {
            ndi_in::wanted_in(&draft.slide.objects, &mut wanted);
            choosing = draft.selected_object().is_some_and(|o| matches!(o.kind, sp_model::ObjectKind::Ndi { .. }));
        }
        if std::mem::take(&mut running.home_state.ndi.look_again) {
            self.ndi.look_again();
        }
        let mut own: Vec<String> = running
            .home_state
            .screens
            .iter()
            .filter_map(|screen| match &screen.output {
                screens::Output::Ndi(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        // Testing aid: SOFTPRESENTER_NDI also sends under a name, which is no source to show.
        own.extend(std::env::var("SOFTPRESENTER_NDI").ok());
        let (view, report) = self.ndi.sync(&wanted, choosing, &own, Instant::now());
        running.home_state.ndi = view;
        for source in &report.stopped_now {
            applog::event("ndi", &format!("{} stopped sending", sp_model::ndi_short_name(source)));
            running.home_state.notices.trouble(ndi_in::stopped_notice(source));
        }
        if !report.stopped_now.is_empty() {
            // The notice shows at once, on a wake nothing else asked a drawing on.
            running.home_window.request_redraw();
        }
        for source in &report.first {
            applog::event("ndi", &format!("receiving {}", sp_model::ndi_short_name(source)));
        }
        for source in &report.dropped {
            applog::event("ndi", &format!("no longer receiving {}", sp_model::ndi_short_name(source)));
        }
        report
    }

    /// Keeps NDI sources going for the editor and the thumbnails: the sources whose first
    /// picture has come (their thumbnails are drawn again), the sources with a new picture the
    /// editor shows (at most thirty times a second), and when to look again.
    fn tick_ndi(&mut self) -> (Vec<String>, Vec<String>, Option<Instant>) {
        let report = self.sync_ndi();
        let now = Instant::now();
        let changes = sp_render::ndi_changes();
        let editing = self.running.as_ref().is_some_and(|running| {
            running.home_state.editing.as_ref().is_some_and(|draft| {
                draft
                    .slide
                    .objects
                    .iter()
                    .any(|o| matches!(&o.kind, sp_model::ObjectKind::Ndi { source, .. } if !source.is_empty()))
            })
        });
        let mut fresh = Vec::new();
        if changes != self.ndi_seen {
            if !editing {
                self.ndi_seen = changes;
            } else if self.ndi_drawn.is_none_or(|at| now.duration_since(at) >= Duration::from_millis(33)) {
                self.ndi_seen = changes;
                self.ndi_drawn = Some(now);
                fresh = self.ndi.receiving_sources();
            }
        }
        let at = match (self.ndi.receiving_any(), editing) {
            (false, _) => None,
            (true, true) => Some(now + Duration::from_millis(33)),
            (true, false) => Some(now + Duration::from_millis(200)),
        };
        (report.first, fresh, at)
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
                    if let Some(look) = running.home_state.voxlive.look.take() {
                        self.voxlive.look_closed(&look.screen);
                    }
                    self.voxlive_preview = None;
                }
            }
        }
        // A look closes with Settings, or when Settings shows another pane: its page goes with it.
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            let showing = state.preferences_open && state.preferences_pane == home::Pane::Integrations;
            if !showing && let Some(look) = state.voxlive.look.take() {
                self.voxlive.look_closed(&look.screen);
            }
            if state.voxlive.look.is_none() {
                self.voxlive_preview = None;
            }
        }
        let mut at = self.voxlive.tick();
        // How a save went, in the look if it is open (`sp voxlive` has it from the connection).
        if let Some((screen, outcome)) = self.voxlive.saved.take()
            && let Some(look) =
                self.running.as_mut().and_then(|r| r.home_state.voxlive.look.as_mut()).filter(|l| l.screen == screen)
        {
            look.saving = false;
            match outcome {
                Ok(saved) => {
                    look.take(&saved);
                    look.outcome = Some(crate::i18n::t("notices.voxlive.saved", "Saved to VoxLive.").to_owned());
                }
                Err(voxlive::Saving::Changed(now)) => look.changed_there = Some(*now),
                Err(why @ (voxlive::Saving::Unreachable | voxlive::Saving::Failed(_))) => {
                    look.outcome = Some(why.words())
                }
            }
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
        // A VoxLive screen is on screen: VoxLive out of reach is said at once then.
        let on_screen = self.show.live().is_some_and(|at| at.presentation == self.voxlive_slot);
        if let Some(running) = self.running.as_mut() {
            // A key refused, or VoxLive out of reach for a while, is trouble in the notices too
            // (`voxlive::Client::tell`); the card says so at once.
            if self.voxlive.tell(on_screen, &mut running.home_state.notices) {
                running.home_window.request_redraw();
            }
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
        let mut troubles: Vec<(String, String)> = Vec::new();
        for (file, player) in &mut self.slide_videos {
            wait = wait.min(player.tick());
            while let Some(note) = player.take_note() {
                applog::event("video", &note);
            }
            // A video FFmpeg can't play says so a moment after it was opened: the operator is
            // told once.
            if player.uses_ffmpeg()
                && let Some(problem) = player.problem()
                && self.slide_video_refused.insert(file.clone())
            {
                troubles.push((file.clone(), problem));
            }
            // A video FFmpeg couldn't take over either isn't tried again on every tick.
            if player.problem().is_none() || player.uses_ffmpeg() || self.slide_video_refused.contains(file) {
                continue;
            }
            let fallback = images::resolve(file)
                .map(|path| (sp_video::Player::open_with_ffmpeg(&path, sp_video::Lend::Rows), path));
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
        for (file, problem) in troubles {
            applog::event("video", &format!("could not play {file} in a box on the slide: {problem}"));
            let problem = self.video_problem(std::path::Path::new(&file), problem);
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.trouble(problem);
            }
        }
        if switched {
            self.refresh_output();
        }
        Some(Instant::now() + wait)
    }

    /// The wait for the video on screen's first picture ends because the layer has gone on to
    /// something else (another video or picture, a Clear, the video let go): what the operator
    /// was told when it hadn't started ("It will show as soon as it starts") is taken back, as it
    /// is no longer true.
    fn end_video_wait(&mut self) {
        let Some(wait) = self.video_wait.take() else { return };
        if let (Some(notice), Some(running)) = (wait.notice, self.running.as_mut()) {
            applog::event(
                "video",
                "the video the notice said hadn't started is no longer on screen: the notice is taken back",
            );
            running.home_state.notices.withdraw(&notice);
        }
    }

    /// Looks again whether the video on screen has given its first picture, and says what that
    /// changed (see `video_wait`). When the limit passes with none, the operator is told once,
    /// naming the file, and the log says why the screens went on without it; a picture that
    /// comes after that (or FFmpeg saying the video can't play) takes the notice back. A media
    /// playlist playing goes on from it instead (`tick_media_cycle`), and the operator is told
    /// that; once that item plays, on a later turn, that notice is taken back.
    fn look_at_first_picture(&mut self) -> video_wait::Change {
        use video_wait::Change;
        if self.video.is_none() {
            self.end_video_wait();
            return Change::Nothing;
        }
        let goes_on = self.cycle_goes_on_from_video(false);
        let Some((item, player)) = self.video.as_ref() else { return Change::Nothing };
        let Some(wait) = self.video_wait.as_mut() else { return Change::Nothing };
        let now = Instant::now();
        let change = wait.look(player.ready(), now);
        if change == Change::Nothing {
            return change;
        }
        // An item a playlist went on from before has played now: what was said of it is over.
        if player.problem().is_none()
            && matches!(change, Change::Came | Change::CameLate)
            && let Some(said) = self.skipped_videos.remove(item)
        {
            applog::event(
                "video",
                &format!("{} plays now: the notice that it hadn't started is taken back", file_name(player.path())),
            );
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.withdraw(&said);
            }
        }
        if change == Change::Came {
            return change;
        }
        let name = file_name(player.path());
        let waited = format!("{:.1} s", wait.waited(now).as_secs_f64());
        if change == Change::GaveUp && goes_on {
            let playlist = self
                .media_cycle
                .as_ref()
                .and_then(|cycle| {
                    let running = self.running.as_ref()?;
                    running.home_state.media_playlists.iter().find(|p| p.id == cycle.playlist).map(|p| p.name.clone())
                })
                .unwrap_or_default();
            applog::event(
                "video",
                &format!(
                    "no picture of {name} came in {waited}, the most a video's first picture is waited for: the media \
                     playlist {playlist} goes on to its next item"
                ),
            );
            let seconds = wait.limit().as_secs_f64().round().max(1.0) as u64;
            let text = format!(
                "{name}: {}",
                crate::i18n::tn(
                    "notices.media.video_not_started_skipped",
                    seconds,
                    "this video hasn\u{2019}t started after {n} second, so the media playlist \u{201c}{playlist}\u{201d} \
                     goes on to its next item.",
                    "this video hasn\u{2019}t started after {n} seconds, so the media playlist \u{201c}{playlist}\u{201d} \
                     goes on to its next item.",
                    &[("playlist", &playlist)],
                )
            );
            // Not taken back as the playlist goes on (it stays true), only once the item plays.
            let item = *item;
            self.skipped_videos.insert(item, text.clone());
            // Said once however many turns it doesn't start: an entry already in the list, seen
            // or not, isn't raised again (until it has been taken back).
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.lasting_trouble(text);
            }
        } else if change == Change::GaveUp {
            applog::event(
                "video",
                &format!(
                    "no picture of {name} came in {waited}, the most the screens wait for a video's first picture: \
                     they stop keeping what they showed and go on without it (its still, if it has one); FFmpeg \
                     goes on opening it, and its pictures go on screen if they come"
                ),
            );
            let seconds = wait.limit().as_secs_f64().round().max(1.0) as u64;
            let text = format!(
                "{name}: {}",
                crate::i18n::tn(
                    "notices.media.video_not_started",
                    seconds,
                    "this video hasn\u{2019}t started after {n} second, so the screens went on without it. It will \
                     show as soon as it starts.",
                    "this video hasn\u{2019}t started after {n} seconds, so the screens went on without it. It will \
                     show as soon as it starts.",
                    &[],
                )
            );
            wait.notice = Some(text.clone());
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.trouble(text);
            }
        } else {
            let said = if player.problem().is_some() {
                format!(
                    "FFmpeg said why {name} can't play {waited} after it was asked for, after the screens stopped \
                     waiting"
                )
            } else {
                format!(
                    "the first picture of {name} came {waited} after it was asked for, after the screens stopped \
                     waiting: it goes on screen as it comes"
                )
            };
            applog::event("video", &said);
            if let (Some(text), Some(running)) = (wait.notice.take(), self.running.as_mut()) {
                running.home_state.notices.withdraw(&text);
            }
        }
        change
    }

    /// Keeps the video on screen going (a looping one starts again at its end) and the
    /// operator's view of it up to date. Says when to look again.
    fn tick_video(&mut self) -> Option<Instant> {
        // The screens kept what they showed while the video opened: its first picture has
        // come (or it is known that none will), or it has been too long, so they are told of
        // it now, and the video that played on meanwhile is let go.
        let kept = self.video_wait.as_ref().is_some_and(video_wait::FirstPicture::keeps);
        let change = self.look_at_first_picture();
        // A media playlist playing goes on to its next item from one that never started
        // (`tick_media_cycle`, straight after): the screens go there, not to its still first.
        let goes_on = change == video_wait::Change::GaveUp && self.cycle_goes_on_from_video(true);
        if kept && !goes_on && (change != video_wait::Change::Nothing || self.video_wait.is_none()) {
            self.refresh_output();
        }
        // Looked at again by the end of the wait at the latest; once it is over with no picture
        // yet, a few times a second, not fifty (its pictures go on screen as they come, without
        // it).
        let looked = Instant::now();
        let due = self.video_wait.as_ref().and_then(|wait| Some((wait.due(looked)?, wait.overdue())));
        let (id, player) = self.video.as_mut()?;
        let wait = match (looked + player.tick(), due) {
            (at, Some((due, true))) => at.max(due),
            (at, Some((due, false))) => at.min(due),
            (at, None) => at,
        }
        .saturating_duration_since(looked);
        say_video_pace(player, false);
        while let Some(note) = player.take_note() {
            applog::event("video", &note);
        }
        let handed = player.take_handed_over();
        let id = *id;
        match handed {
            Some(Ok(path)) => {
                applog::event("video", &format!("{} took over at the end of the pass", file_name(&path)));
                let item = self.show.media().iter().find(|item| item.id == id);
                if item.is_some_and(|item| item.path != path) {
                    // Another item, from a followed folder: it is the one on screen now.
                    self.followed_took_over(followed::OnLayer::Media, &path);
                } else {
                    // The row under the preview names the new file (its media bin item has its
                    // name now).
                    let name = item.map(|item| item.name.clone());
                    if let (Some(name), Some(live)) =
                        (name, self.running.as_mut().and_then(|running| running.home_state.video_live.as_mut()))
                    {
                        live.name = name;
                    }
                }
            }
            Some(Err(problem)) => applog::problem("video", &format!("the new file couldn't take over: {problem}")),
            None => {}
        }
        let (_, player) = self.video.as_mut()?;
        let running = self.running.as_mut()?;
        if let Some(problem) = player.problem() {
            applog::event("video", &problem);
            // The system's player couldn't play it (an AV1 video on a Mac without AV1 hardware,
            // say): FFmpeg plays it instead, from where it was.
            if !player.uses_ffmpeg()
                && let Some((id, mut failed)) = self.video.take()
                && let Some(item) = self.show.media().iter().find(|item| item.id == id).cloned()
            {
                say_video_pace(&mut failed, true);
                let loops = self.loops_now(&item);
                let quiet = self.quiet();
                let running = self.running.as_mut()?;
                match sp_video::Player::open_with_ffmpeg(&item.path, sp_video::Lend::Planes) {
                    Ok(mut fallback) => {
                        applog::event("video", &format!("playing {} with FFmpeg instead", item.path.display()));
                        fallback.set_looping(loops);
                        if quiet {
                            fallback.set_volume(0.0);
                        }
                        fallback.play();
                        self.end_video_wait();
                        self.video_wait = (!fallback.ready())
                            .then(|| video_wait::FirstPicture::new(Instant::now(), video_wait::limit()));
                        self.video = Some((id, fallback));
                        // The screens take their pictures from the new player.
                        self.refresh_output();
                    }
                    Err(problem) => {
                        running.home_state.notices.trouble(problem);
                        running.home_state.video_live = None;
                    }
                }
                return Some(Instant::now() + Duration::from_millis(100));
            }
            running.home_state.video_live = None;
            running.home_window.request_redraw();
            if let Some((_, mut player)) = self.video.take() {
                say_video_pace(&mut player, true);
            }
            // A file just taken out of a followed folder: the layer goes on to what is left
            // (see `followed`), with nothing to tell the operator. Anything else is said (a
            // video FFmpeg plays says what is wrong with it here, a moment after it was shown,
            // not as it is opened).
            let path = self.show.media().iter().find(|item| item.id == id).map(|item| item.path.clone());
            if !path.is_some_and(|path| self.gone_from_followed_folder(&path)) {
                // A playing media playlist goes on from it (`tick_media_cycle`).
                self.video_refused = Some(id);
                let in_playlist = self.playlist_shows_live_media();
                if let Some(running) = self.running.as_mut() {
                    // A media playlist comes back to it every round: said once.
                    if in_playlist {
                        running.home_state.notices.lasting_trouble(problem);
                    } else {
                        running.home_state.notices.trouble(problem);
                    }
                }
            }
            return None;
        }
        if let Some(live) = running.home_state.video_live.as_mut() {
            let (position, duration, playing) = (player.position(), player.duration(), player.playing());
            // The window shows whole seconds, so it is drawn again only when one passes, and only
            // while it shows them (in the row under the Program preview, not in the Edit view).
            let changed =
                live.position.floor() != position.floor() || live.duration != duration || live.playing != playing;
            *live = home::VideoLive { position, duration, playing, ..live.clone() };
            if changed {
                if running.home_state.preview_painted {
                    running.home_window.request_redraw();
                }
                // Pages show a video's time and length too, in the Edit view as well.
                self.remote.stir();
            }
        }
        Some(Instant::now() + wait)
    }

    /// Carries out what the operator asked of the video on screen.
    fn handle_video(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let Some(request) = running.home_state.video_request.take() else { return };
        let (Some((_, player)), Some(live)) = (self.video.as_mut(), running.home_state.video_live.as_mut()) else {
            return;
        };
        ask_of_video(player, live, request);
        running.home_window.request_redraw();
    }

    /// Carries out what the Audio tab asked of the player, and tells the tab what is playing.
    fn handle_audio(&mut self) {
        use home::AudioRequest;
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        match state.audio_request.take() {
            Some(AudioRequest::Add) => {
                // The tracks chosen join the playlist shown now (`dialogs`).
                let playlist = state.audio.shown().id;
                let window = dialogs::Files::new(
                    dialogs::Choose::Files,
                    crate::i18n::t("notices.dialog.add_audio", "Add Audio"),
                )
                .filter(crate::i18n::t("notices.dialog.filter.audio", "Audio"), &audio::AUDIO_ENDINGS);
                self.ask(dialogs::For::AddAudio { playlist }, dialogs::Window::Files(window));
                return;
            }
            Some(AudioRequest::Play(id)) => {
                // A track chosen while another plays crossfades from it, if a crossfade is set.
                let playing = self.player.playing();
                let crossfade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
                let fade = if playing.track.is_some() && !playing.paused { crossfade } else { Duration::ZERO };
                if let Some(track) = state.audio.track(id) {
                    applog::event("audio", &format!("play {} (asked for, crossfade {fade:?})", track.path.display()));
                    if let Err(problem) = self.player.play_fading(track, fade) {
                        state.notices.trouble(problem);
                    }
                }
            }
            Some(AudioRequest::TogglePause) => self.player.toggle_pause(),
            Some(AudioRequest::Stop) => self.player.stop(),
            Some(AudioRequest::Skip(seconds)) => {
                let playing = self.player.playing();
                let to = Duration::from_secs_f32((playing.position.as_secs_f32() + seconds).max(0.0));
                seek_audio(&mut self.player, state, to);
            }
            Some(AudioRequest::Seek(to)) => seek_audio(&mut self.player, state, to),
            // The level the tab shows follows it too (from `sp audio volume`, say).
            Some(AudioRequest::Volume(volume)) => {
                state.audio_volume = volume;
                self.player.set_volume(volume);
            }
            // Mute keeps the level to come back to, in the place the Audio tab's Mute keeps it.
            Some(AudioRequest::Mute(on)) => {
                if on {
                    if state.audio_volume > 0.0 {
                        state.audio_muted_from = Some(state.audio_volume);
                    }
                    state.audio_volume = 0.0;
                } else if state.audio_volume == 0.0 {
                    state.audio_volume = state.audio_muted_from.take().unwrap_or(1.0);
                }
                self.player.set_volume(state.audio_volume);
            }
            // The track playing plays on to its end; then the playlist goes on as it now stands.
            Some(AudioRequest::Remove(id)) => {
                state.audio.remove_track(id, state.audio_playing.track);
                state.audio_changed = true;
            }
            None => {}
        }
        if std::mem::take(&mut state.audio_changed) {
            save_setting(&mut self.paced, &self.writer, &mut state.notices, audio::to_save(&state.audio));
        }
        self.tick_audio();
    }

    /// Keeps the audio going: takes in what the player has done (tracks begun, ended, or that
    /// could not be played), and with Play through or Loop on lines up what follows the track
    /// heard (the next, or the same one again) a few seconds before its end, so it starts on the
    /// frame after its last, or fades in over its end if a crossfade is set. Tells the Audio tab
    /// what is playing, and says when to look again.
    fn tick_audio(&mut self) -> Option<Instant> {
        let running = self.running.as_mut()?;
        let state = &mut running.home_state;
        let events = {
            let audio = &state.audio;
            self.player.tick(|id| audio.track(id).map_or_else(String::new, |track| track.name.clone()))
        };
        let name_of = |audio: &audio::Library, id: Uuid| audio.track(id).map_or_else(String::new, |t| t.name.clone());
        for event in events {
            match event {
                audio::Event::Began { track, fresh, followed, said } => {
                    applog::event("audio", &format!("{} {said}", if followed { "follows on:" } else { "playing" }));
                    // Shuffle's order counts each track that begins.
                    state.audio.started(track, fresh);
                }
                audio::Event::Ended { track } => {
                    applog::event("audio", &format!("ended {}", name_of(&state.audio, track)));
                }
                audio::Event::Failed { track, problem, lined_up } => {
                    applog::event("audio", &format!("could not play {}: {problem}", name_of(&state.audio, track)));
                    if lined_up {
                        // Said once however often it comes round.
                        state.notices.lasting_trouble(problem);
                    } else {
                        state.notices.trouble(problem);
                    }
                }
            }
        }
        // What follows the track heard.
        let crossfade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
        let goes_on = state.audio.through || state.audio.looping;
        if let Some(then) = self.player.lined_up() {
            // Lined up, and no longer to follow: Play through and Loop switched off, the track
            // taken out, or nothing to follow now.
            let from = self.player.playing().track;
            let still = goes_on
                && state.audio.track(then).is_some()
                && from.is_some_and(|from| state.audio.after_end(from, |_| 0, self.player.failed()).is_some());
            if !still {
                self.player.cancel_then();
            }
        } else if goes_on && let Some(from) = self.player.wants_next(crossfade) {
            let pick = |count: usize| (Uuid::new_v4().as_u128() % count as u128) as usize;
            // A track too short to crossfade from goes straight into the next.
            let length = self.player.playing().length;
            let fade = if length.is_some_and(|length| length > crossfade * 2) { crossfade } else { Duration::ZERO };
            match state.audio.after_end(from, pick, self.player.failed()) {
                Some(next) => {
                    if let Err(problem) = self.player.line_up(next, fade) {
                        state.notices.trouble(problem);
                    }
                }
                None => {
                    if !self.player.failed().is_empty() {
                        state.notices.lasting_trouble(crate::i18n::tf(
                            "tools.audio.stopped_none_played",
                            "The audio stopped: no track after {name} could be played.",
                            &[("name", &name_of(&state.audio, from))],
                        ));
                    }
                    self.player.give_up(from);
                }
            }
        }
        let playing = self.player.playing();
        // A track taken out while it played is let go once something else plays, or nothing.
        state.audio.forget_leaving(playing.track);
        let shown = |p: &audio::Playing| (p.track, p.position.as_secs(), p.paused);
        if shown(&playing) != shown(&state.audio_playing) {
            running.home_window.request_redraw();
        }
        state.audio_playing = playing;
        // While a track plays, or one is on its way, look again soon.
        ((playing.track.is_some() && !playing.paused) || self.player.busy())
            .then(|| Instant::now() + Duration::from_millis(100))
    }

    /// Waits a moment (a few seconds at most) to hear whether a track asked for can be played:
    /// the problem if it can't, said in the window too. For the command line and the remote,
    /// which answer with it.
    pub(crate) fn audio_outcome(&mut self, id: Uuid) -> Result<(), String> {
        let began = Instant::now();
        while began.elapsed() < Duration::from_secs(5) {
            match self.player.outcome(id) {
                Some(Ok(())) => break,
                Some(Err(problem)) => {
                    self.tick_audio();
                    return Err(problem);
                }
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        self.tick_audio();
        Ok(())
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
        if command != Command::Next {
            return vec![command];
        }
        match self.goes_on_into() {
            Some((item, stops)) => self.start_commands(item, false, &stops, true).unwrap_or_else(|| vec![command]),
            None => vec![command],
        }
    }

    /// The item of the playlist in front that the show's own Next goes on into with Continuous
    /// on (see `continue_playlist`), and the playlist as the keys see it; `None` when it stays in
    /// what is on screen.
    fn goes_on_into(&self) -> Option<(usize, Vec<running::Stop>)> {
        let continuous = self.running.as_ref().is_some_and(|running| running.home_state.continuous);
        // Only at the end of the song's play order, with the last slide's builds all done.
        if !continuous || self.show.live().is_none() || self.show.upcoming().is_some() || self.show.builds_left() {
            return None;
        }
        let (stops, _) = self.playlist_stops();
        let item = running::go_on(&stops, self.cursor_item(&stops), true)?;
        Some((item, stops))
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
                .filter(|at| {
                    matches!(&items[*at], service::Item::Passage(saved)
                        if title.is_some_and(|title| state.bible.holds_kept(cursor.presentation, title, &saved.lookup)))
                })
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
    fn step_keys(&mut self, forward: bool) -> Option<running::Said> {
        self.step_aimed(forward, None)
    }

    /// Next or Previous as `step_keys` does them, or, with `aim`, as if that item of the
    /// playlist in front were the open one and nothing were picked: for a remote page, which
    /// keeps its own open item and picks and must not move the window's. An item it starts goes
    /// on screen without becoming the window's open item.
    pub(crate) fn step_aimed(&mut self, forward: bool, aim: Option<usize>) -> Option<running::Said> {
        match self.decide_step(forward, aim)? {
            running::Do::Show(command) => self.apply(vec![command]),
            running::Do::Start { item, last } => self.start_item_as(item, last, aim.is_none()),
            running::Do::Say(words) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.refuse(words.clone());
                    running.home_window.request_redraw();
                }
                return Some(running::Said { words, at_end: false });
            }
            // At an end: nothing in the window, only words for a command's reply.
            running::Do::Stay(end) => return Some(running::Said { words: end.words(), at_end: true }),
        }
        None
    }

    /// What Next (`forward`) or Previous would do now, as `step_aimed` (with `aim`, for a remote
    /// page) carries it out; nothing is changed. The one place this is decided: the line under
    /// the preview and `sp status` say what Next would do from it too (`next_up`).
    fn decide_step(&self, forward: bool, aim: Option<usize>) -> Option<running::Do> {
        let command = if forward { Command::Next } else { Command::Previous };
        let running = self.running.as_ref()?;
        let state = &running.home_state;
        let picked = if aim.is_some() { None } else { state.picked_word(&self.show) };
        let showing = self.show.live().is_some();
        // In the editors the show steps as it always has; on the Bible page too, unless a verse
        // picked waits for Enter with nothing on screen. An aimed step is the playlist's.
        Some(if aim.is_none() && state.edit_kind().is_some() {
            running::Do::Show(command)
        } else if aim.is_none() && state.bible_open {
            running::held_for_enter(picked, showing).unwrap_or(running::Do::Show(command))
        } else {
            let open = aim.or(if state.playlist_open { state.services.chosen } else { None });
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
        })
    }

    /// What Next would put on screen now, for the line under the preview and `sp status`: what
    /// `decide_step` decides, with the show's own Next going on into the next item where
    /// Continuous takes it (`goes_on_into`, as `apply` does), and the slide the show would put
    /// on (`Show::next_on_screen`). So they never say other than the key does.
    pub(crate) fn next_up(&self) -> running::NextUp {
        let Some(decided) = self.decide_step(true, None) else { return running::NextUp::Nothing };
        let decided = match decided {
            running::Do::Show(Command::Next) => match self.goes_on_into() {
                Some((item, _)) => running::Do::Start { item, last: false },
                None => decided,
            },
            decided => decided,
        };
        let (stops, titles) = match decided {
            running::Do::Start { .. } => self.playlist_stops(),
            _ => (Vec::new(), Vec::new()),
        };
        running::next_up(&decided, &stops, &titles, || {
            let Some(at) = self.show.next_on_screen() else { return running::NextUp::Nothing };
            let Some(song) = self.show.presentations().get(at.presentation) else { return running::NextUp::Nothing };
            // Nothing shown yet: the song chosen starts.
            if self.show.cursor().is_none_or(|cursor| cursor.presentation != at.presentation) {
                return running::NextUp::Item { title: song.title.clone(), slides: true };
            }
            // Named as the readout over it names the slide on screen: by its group.
            let name = song.slide(at.slide).map(|(group, _)| group.name.trim()).filter(|name| !name.is_empty());
            running::NextUp::Slide { name: name.map(str::to_owned), number: at.slide + 1 }
        })
    }

    /// Works out again the line under the preview that says what Next will put on screen
    /// (`next_up`), when anything it is worked out from has changed since: not on every frame a
    /// video or a fade draws, as working it out goes through the playlist and the library.
    fn refresh_next_line(&mut self) {
        let Some(running) = self.running.as_ref() else { return };
        if running.next_line_from.as_ref().is_some_and(|from| from.holds(&self.show, &running.home_state)) {
            return;
        }
        let from = NextLineFrom::of(&self.show, &running.home_state);
        let line = self.next_up().words();
        if let Some(running) = self.running.as_mut() {
            running.home_state.next_line = line;
            running.next_line_from = Some(from);
        }
    }

    /// The keys doing Next or Previous while a slide is picked and nothing is on screen (the
    /// owner's rule of 2026-11-21, for Space and the arrows, and the prototype's for every key):
    /// a key doing Next puts the picked slide on, as Enter does, but → moves the pick on; a key
    /// doing Previous moves it back, to the previous slide of its passage or song in the order
    /// sung, putting nothing on. With something on screen they run it as ever (false: not taken
    /// here), so a song can be finished with a reading picked and waiting for Enter. A key set to
    /// anything else does that (`pick_key_for`).
    fn pick_key(&mut self, key: &Pressed, action: KeyAction) -> bool {
        let Some(running) = self.running.as_mut() else { return false };
        match pick_key_press(&self.show, &mut running.home_state, key, action) {
            running::PickKey::Pass => false,
            running::PickKey::TakeIt => {
                self.enter_key();
                true
            }
            running::PickKey::Move(_) => {
                running.home_window.request_redraw();
                true
            }
        }
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
            Some(running::Do::Say(_) | running::Do::Stay(_)) | None => {}
        }
    }

    /// Says in the log where a key pressed in the home window went (see `log_keys`): whether it
    /// waited for the window to draw what came before it, and how long that took; where it went;
    /// and, after it, the slide picked, the song chosen, the slide on screen (as song and slide,
    /// from 1), the find box and who has the keyboard. A letter is said as "a letter".
    fn log_key(&self, key: &winit::event::KeyEvent, waited: Option<Duration>, goes: &KeyGoes) {
        let Some(running) = self.running.as_ref() else { return };
        let state = &running.home_state;
        let name = match &key.logical_key {
            Key::Named(named) => format!("{named:?}"),
            key if opens_find(key) => "/".to_owned(),
            Key::Character(_) => "a letter".to_owned(),
            _ => "another key".to_owned(),
        };
        let held = if key.repeat { " (held down)" } else { "" };
        let waited = waited.map_or(String::new(), |took| {
            format!("waited {} ms for the window to draw what came before it, then ", took.as_millis())
        });
        let at = |at: Option<sp_engine::SlideRef>| {
            at.map_or_else(|| "nothing".to_owned(), |at| format!("song {} slide {}", at.presentation + 1, at.slide + 1))
        };
        let keyboard = if running.egui_ctx.text_edit_focused() {
            "a text box"
        } else if running.egui_ctx.memory(|memory| memory.focused().is_some()) {
            "a control"
        } else {
            "the show"
        };
        applog::event(
            "keys",
            &format!(
                "{name}{held}: {waited}went to {goes:?}; picked {}, chosen song {}, on screen {}, find box {}, keyboard with {keyboard}",
                at(state.pick(&self.show)),
                self.show.selected() + 1,
                at(self.show.live()),
                match (&state.library_search, state.search_resting) {
                    (Some(_), false) => "open",
                    (Some(_), true) => "open, keys given back",
                    (None, _) => "closed",
                },
            ),
        );
    }

    /// Carries out a command from `sp` or from a remote page (`from` names the page's device)
    /// with the same handler, logs it unless it only reads, sends the reply, and brings the
    /// screens up to date at once.
    fn run_control(
        &mut self,
        el: &ActiveEventLoop,
        args: &[String],
        from: Option<&str>,
        send: impl FnOnce(control::Reply),
    ) {
        let reply = self.control(el, args);
        // What it deleted goes in the history now, before any later change.
        self.keep_removed();
        // A command that only reads (status, a list) changes nothing worth a line: a page
        // asking every second or two would fill the log with them.
        if !control::only_reads(args) {
            let outcome = if reply.ok { "done" } else { "refused" };
            match from {
                Some(from) => {
                    // Counted for the anonymous reports: that a remote page did something.
                    reports::stats::bump(reports::stats::Count::RemoteActions);
                    applog::event("remote", &format!("{from}: {:?} -> {outcome}", control::shown_args(args)))
                }
                None => applog::event("control", &format!("{:?} -> {outcome}", control::shown_args(args))),
            }
        }
        send(reply);
        // A screen changed from the command line reaches the screens now, not at the window's
        // next redraw (which an idle or hidden window may not have for a while).
        self.sync_screens(el);
        self.sync_panels(el);
    }

    /// Puts an item of the playlist in front on screen: a song or a Bible passage at its first
    /// slide (its last when `last`), a picture or video shown, a media playlist played. It
    /// becomes the open item.
    fn start_item(&mut self, item: usize, last: bool) {
        self.start_item_as(item, last, true);
    }

    /// Puts an item of the playlist in front on screen as `start_item` does; it becomes the
    /// open item only when `open_it` (not when a remote page, which keeps its own, started it).
    fn start_item_as(&mut self, item: usize, last: bool, open_it: bool) {
        let (stops, _) = self.playlist_stops();
        // A picture, video or media playlist that belongs to the announcements plays there.
        if let Some((what, from)) = self.announced_item(item, &stops) {
            if let Some(running) = self.running.as_mut()
                && open_it
            {
                running.home_state.services.chosen = Some(item);
            }
            self.announce_clicked(what, from);
            return;
        }
        if let Some(running::Stop::MediaPlaylist(true)) = stops.get(item) {
            let id = self.running.as_ref().and_then(|r| match r.home_state.services.front().items.get(item) {
                Some(service::Item::MediaPlaylist { id, .. }) => Some(*id),
                _ => None,
            });
            if let Some(running) = self.running.as_mut()
                && open_it
            {
                running.home_state.services.chosen = Some(item);
            }
            if let Some(id) = id {
                self.play_media_playlist(id);
            }
            return;
        }
        if let Some(commands) = self.start_commands(item, last, &stops, open_it) {
            self.apply(commands);
        }
    }

    /// What an item of the playlist in front plays on the Announcements layer when it is
    /// started, if it is a picture, video or media playlist that belongs to the announcements
    /// (see `announcements::for_media`).
    fn announced_item(&self, item: usize, stops: &[running::Stop]) -> Option<(announcements::Announce, Option<Uuid>)> {
        let state = &self.running.as_ref()?.home_state;
        match (state.services.front().items.get(item)?, stops.get(item)?) {
            (service::Item::Media { .. }, running::Stop::Media(Some(at))) => {
                announcements::for_media(state, self.show.media().get(*at)?)
            }
            (service::Item::MediaPlaylist { id, .. }, running::Stop::MediaPlaylist(true)) => {
                announcements::for_media_playlist(state, *id)
            }
            _ => None,
        }
    }

    /// What puts a song, a passage or a picture of the playlist in front on screen; a passage is
    /// made into slides first. The item becomes the one on screen, and the open one when
    /// `open_it`.
    fn start_commands(
        &mut self,
        item: usize,
        last: bool,
        stops: &[running::Stop],
        open_it: bool,
    ) -> Option<Vec<Command>> {
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
        if open_it {
            running.home_state.services.chosen = Some(item);
        }
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
        state.bible.open_saved(&saved, live, home::SlidesOf::Passage);
        state.bible.go_live = None;
        let (slot, passage) = state.bible.pending.take()?;
        let count = passage.slide_count();
        if count == 0 {
            return None;
        }
        self.show.replace_presentation(slot, Arc::new(passage));
        Some(sp_engine::SlideRef { presentation: slot, slide: if last { count - 1 } else { 0 } })
    }

    /// What the slide on screen is from: its song and, when it was put on screen from the
    /// playlist in front, the item.
    fn showing(&self) -> Option<versions::Showing> {
        let live = self.show.live()?;
        let (stops, _) = self.playlist_stops();
        let item = match self.cursor_item(&stops) {
            running::Here::Item(at) => Some(at),
            running::Here::Elsewhere | running::Here::Nothing => None,
        };
        Some(versions::Showing { song: live.presentation, item })
    }

    /// Brings the theme version in step with the slide on screen: another item going on screen
    /// ends it unless it was kept, and an item set to a version brings it. Nothing is worked out
    /// while no theme has a version, nor while the same slide stays on screen.
    fn follow_version(&mut self) {
        let Some(running) = self.running.as_ref() else { return };
        let state = &running.home_state;
        let Some(live) = self.show.live().filter(|_| !state.versions.is_empty()) else { return };
        let seen = (live, state.live_item, state.services.chosen);
        if self.version_seen == Some(seen) {
            return;
        }
        self.version_seen = Some(seen);
        let Some(here) = self.showing() else { return };
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        let versions = &state.versions;
        let asked = here
            .item
            .and_then(|at| state.services.front().items.get(at))
            .and_then(service::Item::show_with)
            .filter(|asked| versions.iter().any(|version| version.id == asked.version));
        if state.version_switch.slide_on(here, asked) {
            let now = state.version_switch.version().and_then(|id| versions.iter().find(|v| v.id == id));
            applog::event("version", now.map_or("the usual look", |version| version.name.as_str()));
        }
    }

    /// Tells the output what is on each layer now, then redraws the operator's preview.
    fn refresh_output(&mut self) {
        // Whatever changes what the screens show changes what pages are sent: a message or prop
        // that leaves by itself is taken off on a pass nothing else stirred.
        self.remote.stir();
        self.follow_version();
        self.sync_video();
        // Whether the screens still keep what they showed for the video's first picture.
        self.look_at_first_picture();
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
        self.sync_ndi();
        let mut ndi: Vec<String> = self.live_ndi_wanted().into_iter().map(|(source, _)| source).collect();
        // A phone's screen in a box is watched as an NDI source is, under its own name.
        if self.live_mirror_wanted() {
            ndi.push(sp_render::MIRROR_SOURCE.to_owned());
        }
        let media_fit = self.live_media_fit();
        let theme = frame_theme(&self.show, &self.theme, |at| self.theme_of(at)).clone();
        let about = self.stage_about();
        let end_mark = self.end_mark_now();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.end_mark_sent = end_mark;
        let slide = frame_slide(&self.show);
        // The slide Next would show, in the song's play order, for the stage screens.
        let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide).map(|(_, slide)| Arc::new(slide.clone()))
        });
        // The screens get the item's full-size picture; until it is read, its small copy.
        let live_item = self.show.live_media().and_then(|index| self.show.media().get(index));
        let media = self.bin_pictures.for_screens(live_item.map(|item| (item.id, item.path.as_path())));
        // A video whose first picture is still to come (FFmpeg is being started, which on a
        // busy computer can take seconds) isn't shown yet: the screens keep the picture, or the
        // video still playing, that they had, and are told of the new one when its first
        // picture is there (`tick_video`), so they go from the one to the other with nothing
        // between. Not for ever: once `video_wait::LIMIT` has passed they go on without it
        // (its still, if it has one), and its pictures follow as they come.
        let waits = self.video_wait.as_ref().is_some_and(video_wait::FirstPicture::keeps);
        // The video that played on meanwhile is let go only once the screens have been told
        // what follows it.
        let mut let_go = None;
        let (video, media, media_fit) = if waits {
            let leaving = self.video_leaving.as_ref().map(sp_video::Player::frames);
            (leaving, self.media_on_screens.0.clone(), self.media_on_screens.1)
        } else {
            let_go = self.video_leaving.take();
            self.media_on_screens = (media.clone(), media_fit);
            (video, media, media_fit)
        };
        let next_media = next.as_deref().and_then(home::media_of);
        // The audience comes first: tell the output before drawing anything for the operator.
        if let Some(output) = &running.output {
            let frame = Frame {
                media: media.clone(),
                video,
                media_fit,
                slide_video,
                ndi,
                slide: slide.clone(),
                theme: theme.clone(),
                version: running.home_state.version_switch.version(),
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
                words_hidden: self.show.words_hidden(),
                end_mark,
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_has_stopped();
            }
        }
        drop(let_go);
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
        let end_mark = self.end_mark_now();
        // The preview shows a screen that shows slides: without announcements a slide took off.
        let announcement = self.announcing.as_ref().filter(|a| a.shown.on_slide_screens).map(|a| a.shown.clone());
        let theme = frame_theme(&self.show, &self.theme, |at| self.theme_of(at)).clone();
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
        // The audience's picture shows the slide in the version that is on, and without its words
        // while they are hidden, as the screens do; the stage's shows it as it is, words and all.
        // The end of a song's mark is the audience's too: the stage's picture never has it.
        let slide_in = |version: Option<Uuid>, words_hidden: bool, end_mark: Option<f32>| {
            self.show.live_slide().map(|(_, slide)| {
                let clock = sp_model::BuildClock::settled(step);
                output::drawn_slide(slide, &theme, version, clock, &values, words_hidden, end_mark)
            })
        };
        let audience_slide = slide_in(state.version_switch.version(), self.show.words_hidden(), end_mark);
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
        // The copyright line as last worked out (a change to it redraws the preview again),
        // gone with the words while they are hidden, as on the audience screens.
        let copyright = running.overlay_sent.as_ref().and_then(|sent| sent.copyright.as_deref());
        live_props.extend(output::copyright_shown(copyright, true, self.show.words_hidden()));
        let over = props::overlay(&live_props, message.as_ref());
        let audience = Layers {
            media_fit,
            media: media.as_ref(),
            slide: audience_slide.as_deref(),
            overlay: over.as_ref().filter(|_| self.show.output_enabled()),
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: announcement.as_ref().map(|shown| (&*shown.slide, &*shown.theme)),
        };
        running.stage_preview_at = Instant::now();
        running.preview_shows_stage = state.preview_stage;
        if state.preview_stage {
            // The stage screens' picture: the time, this slide, the next, a timer, the stage message.
            let slide = slide_in(None, false, None);
            let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
                let song = self.show.presentations().get(at.presentation)?;
                song.slide(at.slide).map(|(_, slide)| slide.clone())
            });
            let overlay = running.overlay_sent.clone().unwrap_or_default();
            // The layout of the stage screen looked at, or of the first, as the people on stage
            // see it, each picture of an audience screen in it naming the screen it shows.
            let stage_screens = || state.screens.iter().filter(|screen| screen.kind == screens::Kind::Stage);
            let wanted = stage_screens()
                .find(|screen| Some(screen.id) == state.preview_screen)
                .or_else(|| stage_screens().next())
                .and_then(|s| s.stage_layout);
            let template = stage::with_sources(&stage::layout_for(&state.stage_layouts, wanted), &state.screens);
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
            // A picture of an audience screen in the layout shows what the audience preview
            // shows, as the stage screens show that screen's own picture.
            let shown_screen = template.objects.iter().find_map(|object| match object.kind {
                sp_model::ObjectKind::Screen { screen: Some(screen), .. } if !screen.is_nil() => Some(screen),
                _ => None,
            });
            let layout = stage::theme_for(&template);
            let (now, before) = running.thumbs.preview_showing(
                &running.gpu,
                &mut running.egui_renderer,
                &layers,
                &layout,
                shown_screen.map(|screen| (screen, &audience, &*theme)),
            );
            running.preview = now;
            running.preview_before = before;
            let now = Instant::now();
            running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
            running.stage_preview_at = now;
            return;
        }
        let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &audience, &theme);
        running.preview = now;
        running.preview_before = before;
        let now = Instant::now();
        running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
    }

    /// Takes in what the system says of files dragged from another program: the window library's
    /// own words on macOS, Windows and X11, and what a Wayland desktop said, put the same way
    /// (`file_drop::Told::by_wayland`). Where the pointer is, the window asks as it draws (see
    /// file_drop.rs).
    fn file_drag_told(&mut self, told: file_drop::Told) {
        let path = match told {
            file_drop::Told::Dropped(path) => return self.file_dropped(path),
            file_drop::Told::Over(path) => Some(path),
            file_drop::Told::Gone => None,
        };
        let Some(running) = self.running.as_mut() else {
            return;
        };
        match path {
            Some(path) => {
                running.home_state.drop_hover = true;
                running.home_state.file_drag.add(path);
            }
            None => {
                running.home_state.drop_hover = false;
                running.home_state.file_drag = file_drop::FileDrag::default();
            }
        }
        running.home_window.request_redraw();
    }

    /// Notes a file dropped on the window from the Finder. The system hands over several
    /// dropped together one after another; they are taken in together once all have come (see
    /// [`App::finish_drop`]), each to the place the pointer was over at the drop.
    ///
    /// The first sends `AppEvent::FilesDropped`, which the loop hands back after the rest of
    /// that drop's files (they all come within the one call from the system), so the drop is
    /// taken in at once. Waiting for the loop to be about to sleep instead could wait for
    /// something else to happen on Windows, where a drop comes from inside the system's own
    /// handling of it; that is still looked at then, in case the event can't be sent.
    fn file_dropped(&mut self, path: PathBuf) {
        if self.dropped.is_empty() {
            self.drop_pointer = self
                .running
                .as_ref()
                .and_then(|running| file_drop_pointer(&running.home_window, &running.egui_ctx, &running.file_drops));
            self.drop_came = Some((Instant::now(), chrono::Local::now()));
            if let Some(waker) = &self.waker {
                let _ = waker.send_event(AppEvent::FilesDropped);
            }
        }
        self.dropped.push(path);
    }

    /// Takes in the files dropped on the window since the last look: songs to the songs (into the
    /// playlist or library dropped on, if any, else the library shown), never into the media bin;
    /// pictures and videos into the media bin (and the playlist dropped on), or onto the slide in
    /// the editor; presentations and PDFs as slides; a Bible installed. See `file_drop.rs`.
    fn finish_drop(&mut self, how: &str) {
        if self.dropped.is_empty() {
            return;
        }
        let paths = std::mem::take(&mut self.dropped);
        let pointer = self.drop_pointer.take();
        if let Some((came, at)) = self.drop_came.take() {
            applog::event("drop", &format!("received {} files at {}", paths.len(), at.format("%H:%M:%S%.3f")));
            applog::event("drop", &format!("taken in after {} ms ({how})", came.elapsed().as_millis()));
        }
        // What each file is was found while it was held over the window; a file the system
        // didn't say was coming is looked at now.
        let seen: Vec<(PathBuf, file_drop::Kind)> =
            self.running.as_ref().map(|running| running.home_state.file_drag.files.clone()).unwrap_or_default();
        let files: Vec<(PathBuf, file_drop::Kind)> = paths
            .into_iter()
            .map(|path| {
                let kind = seen
                    .iter()
                    .find(|(have, _)| *have == path)
                    .map_or_else(|| file_drop::kind_of(&path), |(_, kind)| *kind);
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
        for path in &route.bibles {
            self.install_bible(path);
        }
        for path in &route.other {
            self.add_dropped_file(path);
        }
        if !route.slides.is_empty() {
            self.import_slides_asking(&route.slides);
        }
        if !route.songs.is_empty() {
            let to = self.songs_to(route.songs_to.as_ref());
            self.import_songs_to(&route.songs, &to);
        }
    }

    /// Where songs dropped on a playlist or library go.
    fn songs_to(&self, target: Option<&file_drop::DropTarget>) -> SongsTo {
        match target {
            Some(file_drop::DropTarget::Library(library)) => SongsTo { library: Some(*library), ..SongsTo::default() },
            Some(file_drop::DropTarget::Playlist { index, after }) => {
                let name = self
                    .running
                    .as_ref()
                    .and_then(|r| r.home_state.services.list.get(*index))
                    .map(|playlist| playlist.name.clone());
                let playlist = name.map(|name| PlaylistSpot { index: *index, name, after: *after });
                SongsTo { playlist, ..SongsTo::default() }
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
                Err(problem) => state.notices.trouble(problem),
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
        // Said only when it went out of sight (dropped on a playlist that isn't listed).
        let name = services.list.get(index).map(|playlist| playlist.name.clone());
        if let Some(name) = name
            && !studio::playlist_in_sight(&running.home_state, index)
        {
            running.home_state.notices.info(crate::i18n::tf(
                "notices.added_to_playlist",
                "Added {item} to the playlist {list}.",
                &[("item", &item.name), ("list", &name)],
            ));
        }
    }

    /// Any other file dropped on the window: a Bible is installed; anything else is offered to
    /// the media bin, which says why it can't take it.
    fn add_dropped_file(&mut self, path: &std::path::Path) {
        let shown = self.running.as_ref().and_then(|running| running.home_state.media_playlist_shown);
        self.add_dropped_file_to(path, shown);
    }

    /// As [`App::add_dropped_file`], into the media playlist `playlist` (by its id) if it is
    /// still there: the one shown when the media bin's Add was clicked, say.
    fn add_dropped_file_to(&mut self, path: &std::path::Path, playlist: Option<Uuid>) {
        // Whatever its ending: a file nothing else took is looked into (its first few
        // kilobytes) in case it is a Bible.
        if sp_bible::import::looks_like_bible(path) {
            self.install_bible(path);
            return;
        }
        let _ = self.add_media_file_to(path, playlist);
    }

    /// Installs a Bible file the operator has (dropped, or chosen with the Bibles window's
    /// Install from File…), in the app's own format or another program's, on a thread of its
    /// own. One that doesn't say its name, short name or language opens the sheet that asks.
    fn install_bible(&mut self, path: &std::path::Path) {
        let work =
            bible_store::Work::Open { path: path.to_owned(), given: sp_bible::import::About::default(), ask: true };
        self.start_bible_work(work, None);
    }

    /// Starts a Bible file's work (reading, converting, installing) on a thread of its own, so
    /// the window never waits on it, however long a file makes it take: the notice says it is
    /// under way, and [`App::finish_bible_work`] takes in what came of it. `job` is the command
    /// line's, answered then. Returns what the notice says.
    pub(crate) fn start_bible_work(&mut self, work: bible_store::Work, job: Option<u64>) -> String {
        let shown = match &work {
            bible_store::Work::Open { path, .. } => sp_bible::import::shown_name(
                &path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            ),
            bible_store::Work::Install { imported, .. } => imported.file_name.clone(),
        };
        let said = crate::i18n::tf("notices.bible.reading", "Reading {name}…", &[("name", &shown)]);
        let known = self.running.as_ref().map(|running| running.home_state.bible.shelf.known()).unwrap_or_default();
        let (tx, waker) = (self.bible_tx.clone(), self.waker.clone());
        let file = match &work {
            bible_store::Work::Open { path, .. } => Some(path.clone()),
            bible_store::Work::Install { .. } => None,
        };
        let started = std::thread::Builder::new().name("bible file".to_owned()).spawn(move || {
            // A panic in reading a file (ours, or a library's) ends this work, never the app.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bible_store::run(work, &known)))
                .unwrap_or_else(|_| {
                    Err(crate::i18n::tf(
                        "notices.bible.unreadable",
                        "SoftPresenter couldn't read {name}.",
                        &[("name", &shown)],
                    ))
                });
            let kind = result.as_ref().err().and(file.as_deref()).and_then(applog::file_kind::of);
            let _ = tx.send(BibleWork { job, result, kind });
            if let Some(waker) = waker {
                let _ = waker.send_event(AppEvent::BibleRead);
            }
        });
        if started.is_err() {
            let problem = "Couldn't start reading the Bible file.".to_owned();
            self.finish_job(job, Err(problem.clone()));
            return problem;
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.info(said.clone());
            running.home_window.request_redraw();
        }
        said
    }

    /// Takes in the Bible files read on threads of their own since the last look: an installed
    /// Bible goes on the shelf (in place of its earlier copy), one that doesn't say what it is
    /// waits for the sheet to ask, and the notices (and the command line's job) say how it went:
    /// installed, or why not. The sheet asking is there to see, so the list says nothing of it.
    fn finish_bible_work(&mut self) {
        let _stall = stall::inside(stall::Doing::Bible);
        while let Ok(BibleWork { job, result, kind }) = self.bible_rx.try_recv() {
            let mut sheet = false;
            let said = match result {
                Ok(bible_store::Opened::Installed(kept, info, said)) => {
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.bible.shelf.put_installed(kept, info);
                    }
                    Ok(said)
                }
                Ok(bible_store::Opened::NeedsDetails(imported)) => {
                    sheet = true;
                    let asked = format!("{} doesn't say all of what it is: the sheet asks.", imported.file_name);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.bible.options.details.push_back(crate::bible::DetailsSheet::new(imported));
                    }
                    Ok(asked)
                }
                Err(problem) => Err(problem),
            };
            let logged =
                said.as_ref().map_or_else(|problem| applog::file_kind::noted(problem, kind.as_deref()), Clone::clone);
            applog::event("bible", &logged);
            if let Some(running) = self.running.as_mut() {
                if !sheet {
                    running.home_state.notices.outcome(said.clone());
                }
                running.home_window.request_redraw();
            }
            self.finish_job(job, said);
        }
        // Bibles the sheet's Install asked for.
        let asked = self.running.as_mut().map(|running| std::mem::take(&mut running.home_state.bible.options.installs));
        for (imported, details) in asked.unwrap_or_default() {
            self.start_bible_work(bible_store::Work::Install { imported: Box::new(imported), details }, None);
        }
    }

    /// Keeps how a command line job went, for `job N` and `--wait`; the log says how one an AI
    /// assistant started ended.
    fn finish_job(&mut self, job: Option<u64>, result: Result<String, String>) {
        let Some(job) = job else { return };
        if let Some(line) = self.mcp_jobs.ended(job, result.is_ok()) {
            applog::event("mcp", &line);
        }
        if let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job) {
            *kept = Some(result);
        }
    }

    /// The Bibles window's Install from File…: the system's window for choosing a file, then
    /// the Bible installed as a dropped one is.
    fn handle_bible_file_request(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        if !std::mem::take(&mut running.home_state.bible.options.file_request) {
            return;
        }
        // Any file may be a Bible, whatever its ending: what it holds decides.
        let window = dialogs::Files::new(
            dialogs::Choose::File,
            crate::i18n::t("notices.dialog.bible_file", "Choose a Bible File"),
        );
        self.ask(dialogs::For::BibleFile, dialogs::Window::Files(window));
    }

    /// Puts a picture or video in the media bin (and in the media playlist shown, if any).
    /// Ok(true) when it was added, Ok(false) when it was in the bin already; the error says
    /// why it couldn't be.
    fn add_media_file(&mut self, path: &std::path::Path) -> Result<bool, String> {
        let shown = self.running.as_ref().and_then(|running| running.home_state.media_playlist_shown);
        self.add_media_file_to(path, shown)
    }

    /// Puts a picture or video in the media bin, and in the media playlist `playlist` (by its
    /// id), if it is still there. As [`App::add_media_file`] says.
    fn add_media_file_to(&mut self, path: &std::path::Path, playlist: Option<Uuid>) -> Result<bool, String> {
        let _stall = stall::inside(stall::Doing::Media);
        // A file already in the bin isn't added twice; it joins the playlist shown, if any. One a
        // followed folder brought in counts from now on as added by hand, so it stays in the bin
        // when it leaves the folder.
        let already = self.show.media().iter().find(|item| item.path == path).map(|item| item.id);
        if let Some(id) = already {
            self.followed_items.remove(&id);
        }
        let already = already.is_some();
        let started = Instant::now();
        // A video that can't play here says so at once, in words for this system. The reply
        // carries no file name (the caller has it); the notice below names it.
        let no_ffmpeg = !already && media::needs_missing_ffmpeg(path);
        // A video's picture is read by the bin's readers, never here: its still comes from FFmpeg
        // (on Windows a program of its own, which the computer's virus checker may look over
        // first), which can take seconds, and the window must not stand still meanwhile (a tester
        // on Windows saw it freeze when a .mov was dropped on the bin). The tile shows its name
        // until its picture comes; one that can't be read leaves the bin with a notice.
        if !already && !no_ffmpeg && media::is_video(path) {
            let name = path.file_name().map_or_else(|| "Video".to_owned(), |n| n.to_string_lossy().into_owned());
            let item = MediaItem::new(name, path.to_owned());
            let id = item.id;
            self.bin_pictures.read(id, path);
            self.bin_pictures.read_length(id, path);
            self.show.add_media(item);
            self.added_by_hand.insert(id);
            applog::event("media", &format!("added {} (its picture is read on a reader)", path.display()));
            if let Some(running) = self.running.as_mut() {
                running.home_state.drop_hover = false;
                let state = &mut running.home_state;
                if let Some(playlist) = playlist.and_then(|id| state.media_playlists.iter_mut().find(|p| p.id == id))
                    && !playlist.has(path)
                {
                    playlist.toggle(path);
                }
                running.home_window.request_redraw();
            }
            self.save_media_list();
            return Ok(true);
        }
        // On Windows the app fetches FFmpeg (`ffmpeg_fetch`), and the video is added once it
        // is ready.
        let kept = no_ffmpeg && self.ffmpeg_for_added(path);
        // Only the small copy is kept; the full-size picture is read when it goes on screen.
        let result = if already {
            Ok(None)
        } else if no_ffmpeg {
            let advice = if kept { media::ffmpeg_advice_kept() } else { media::ffmpeg_advice() };
            Err(crate::i18n::tf(
                "notices.media.needs_ffmpeg",
                "This video needs FFmpeg to play, and FFmpeg isn't on this computer. {advice}",
                &[("advice", &advice)],
            ))
        } else {
            media::load_small(path).map(Some)
        };
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
                    self.bin_pictures.read_length(item.id, path);
                    self.show.add_media(item);
                    applog::event("media", &format!("added {} in {:?}", path.display(), started.elapsed()));
                } else {
                    running.home_state.notices.refuse(crate::i18n::t(
                        "notices.media.already_in_bin",
                        "That file is already in the media bin.",
                    ));
                }
                // Added while a media playlist is shown, it goes into that playlist, so it shows.
                let state = &mut running.home_state;
                if let Some(playlist) = playlist.and_then(|id| state.media_playlists.iter_mut().find(|p| p.id == id))
                    && !playlist.has(path)
                {
                    playlist.toggle(path);
                }
                self.save_media_list();
            }
            Err(message) => {
                let kind = if no_ffmpeg { None } else { applog::file_kind::of(path) };
                let logged = applog::file_kind::noted(&message, kind.as_deref());
                applog::event("media", &format!("could not add {}: {logged}", path.display()));
                if no_ffmpeg {
                    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    running.home_state.notices.trouble(format!("{name}: {message}"));
                } else {
                    running.home_state.notices.trouble(message);
                }
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
                Arrived::Full(_) | Arrived::Length(..) => None,
            };
            if let Some((id, (path, changed))) = from_folder {
                // (The reader logged why it was refused.)
                if matches!(arrived, Arrived::Refused(..)) {
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
                    self.added_by_hand.remove(&id);
                    // The screens take a new small copy only while it stands in for the full one.
                    if live == Some(id) {
                        refresh |= !self.bin_pictures.screens_have_full();
                        preview = true;
                    }
                    self.forget_thumbs_showing(id);
                }
                // A video's tile shows its length.
                Arrived::Length(id, length) => self.note_media_length(id, length),
                // (The reader logged why it was refused.)
                Arrived::Refused(id, problem) => {
                    // A video added by hand this run whose picture can't be read says so as it
                    // leaves the bin (one in the bin from before leaves quietly, as it did).
                    if self.added_by_hand.remove(&id)
                        && self.bin_pictures.small(id).is_none()
                        && let Some(running) = self.running.as_mut()
                    {
                        running.home_state.notices.trouble(problem.clone());
                    }
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

    /// Tells the window how long a media bin item is (`None` for a picture, or a video whose
    /// length couldn't be read), for its tile.
    fn note_media_length(&mut self, id: Uuid, length: Option<f64>) {
        let Some(running) = self.running.as_mut() else { return };
        let lengths = &mut running.home_state.media_lengths;
        let changed = match length {
            Some(seconds) => lengths.insert(id, seconds) != Some(seconds),
            None => lengths.remove(&id).is_some(),
        };
        if changed {
            running.home_window.request_redraw();
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
            state.media_lengths.remove(&id);
            running.thumbs.forget(id, &mut running.egui_renderer);
        }
        changed
    }

    /// Remembers what is in the media bin, how each item is shown, and the media playlists.
    fn save_media_list(&mut self) {
        match self.media_list_save() {
            Ok(save) => {
                self.writer.write(save);
            }
            Err(problem) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.trouble(problem);
                }
            }
        }
    }

    /// The media bin's file and what goes in it now, for the library's writer.
    fn media_list_save(&self) -> Result<writer::Save, String> {
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
                sample: item.sample.clone(),
            })
            .collect();
        let playlists = self.running.as_ref().map(|r| r.home_state.media_playlists.clone()).unwrap_or_default();
        media::list_to_save(&media::List { entries, playlists })
    }

    /// Keeps a song as it is now, so the change about to be made to it can be undone.
    fn remember_for_undo(&mut self, index: usize) {
        if let Some(song) = self.show.presentations().get(index).cloned() {
            self.history.remember(index, song);
        }
    }

    /// Undoes the last change made to a song in the Service view, or redoes the last one undone.
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
        self.keep_removed();
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
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
        // The slide being edited, when it is a song's (a template, a prop or a stage layout is
        // never followed as one).
        let followed =
            (editing.is_some()).then(|| state.editing.as_ref().map(|draft| (draft.at, draft.slide.id))).flatten();
        // The step the editor took: it is the newest of the other list now.
        let taken = if redo { state.edit_steps.last() } else { state.edit_redo.last() }.copied();
        // Something deleted from one of the Edit view's lists: put back (or deleted again),
        // opened where its list opens it, not a slide to follow. When the history's next step
        // isn't a delete, it was already taken from outside the editor (the Service view, `sp
        // undo`): the step is let go, and nothing else is taken back in its place.
        if taken == Some(home::EditStep::Deleted) {
            if self.history.removed_next(redo) {
                self.undo_show(redo);
            } else if let Some(running) = self.running.as_mut() {
                let state = &mut running.home_state;
                if redo {
                    state.edit_steps.pop()
                } else {
                    state.edit_redo.pop()
                };
            }
            return;
        }
        // A change to a song's slides; only a song's slide being edited is followed (a template,
        // a prop or a stage layout never is).
        if followed.is_none() {
            self.undo_show(redo);
            return;
        }
        self.undo_show(redo);
        // The slide being edited is found again where the change left it.
        self.follow_edited_slide(followed, followed.map_or(0, |(at, _)| at.slide));
    }

    /// Moves what the window and the command line have just deleted (and Undo can bring back)
    /// into the app's one history, in the order it went.
    fn keep_removed(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        for removed in std::mem::take(&mut running.home_state.removed) {
            self.history.remember_removed(removed);
        }
    }

    /// Whether Undo (or Redo) in the Service view has a step to take, as [`App::undo_show`] takes
    /// it: something deleted to put back (or delete again), or a change to a song that is still
    /// there. Something just deleted, not yet in the history, goes in first, which leaves nothing
    /// to redo.
    fn show_can_step(&self, redo: bool) -> bool {
        if self.running.as_ref().is_some_and(|r| !r.home_state.removed.is_empty()) {
            return !redo;
        }
        self.history.removed_next(redo)
            || self.history.peek(redo).is_some_and(|index| index < self.show.presentations().len())
    }

    /// Tells the window what the Service view's history can undo and redo now, so Undo and Redo are
    /// greyed when it has nothing. After the frame's work (`redraw`), a change asks for the window
    /// to be drawn again, so the buttons follow at once.
    fn note_show_history(&mut self, redraw: bool) {
        let now = (self.show_can_step(false), self.show_can_step(true));
        if let Some(running) = self.running.as_mut()
            && running.home_state.show_history != now
        {
            running.home_state.show_history = now;
            if redraw {
                running.home_window.request_redraw();
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
        self.keep_removed();
        // Something deleted, when it was the last change: put back (or deleted again), with no
        // notice, as it is there to see; a notice only says why it couldn't be.
        if self.history.removed_next(redo) {
            let done = self.undo_removed(redo);
            if let Some(running) = self.running.as_mut() {
                if let Some(Err(why)) = done {
                    running.home_state.notices.refuse(why);
                }
                running.home_window.request_redraw();
            }
            return;
        }
        let Some(current) = self.history.peek(redo).and_then(|index| self.show.presentations().get(index).cloned())
        else {
            return;
        };
        let had: std::collections::HashSet<Uuid> =
            current.groups.iter().flat_map(|group| group.slides.iter().map(|slide| slide.id)).collect();
        let Some((index, song)) = self.history.step(redo, current) else { return };
        // Slides deleted and put back by Undo (every slide there is still there, and more) are
        // picked again, where they are now; slides deleted again by Redo are let go.
        let now: Vec<Uuid> = song.groups.iter().flat_map(|group| group.slides.iter().map(|slide| slide.id)).collect();
        let only_added = had.iter().all(|id| now.contains(id));
        let back: Vec<usize> = (0..now.len()).filter(|at| only_added && !had.contains(&now[*at])).collect();
        let gone = now.len() < had.len();
        let changes = self.show.replace_presentation_following(index, song);
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            if !back.is_empty() {
                state.picked_slides =
                    back.iter().map(|slide| sp_engine::SlideRef { presentation: index, slide: *slide }).collect();
                state.picking = home::Picking::Slides;
            } else if gone {
                state.picked_slides.retain(|at| at.presentation != index);
            }
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
        if changes.slides {
            self.refresh_output();
        }
    }

    /// Puts back what was last deleted (or, to redo, deletes it again). Returns what was done,
    /// as `sp undo` says it, or why it couldn't be; `None` when the next step isn't a delete.
    fn undo_removed(&mut self, redo: bool) -> Option<Result<String, String>> {
        let removed = self.history.step_removed(redo)?;
        let done = if removed.in_a_theme() {
            self.redo_theme_removed(&removed, !redo)
        } else if let removed::Removed::Samples(added) = &removed {
            // An add: Undo takes it away, Redo puts it back.
            self.redo_samples(added, redo)
        } else {
            removed.redo(&mut self.running.as_mut()?.home_state, !redo)
        };
        Some(if done { Ok(removed.said(!redo)) } else { Err(removed.changed_since()) })
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

    /// Keeps a song's arrangements as changed in the Service view, and the one it is played in.
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
        let mut rebuilt = song.reflowed(&text);
        // Words edited as text: the chorus is sung after each verse (`chorus_after.rs`).
        rebuilt.follow_parts_as_text(&home::chorus_after_name());
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
        // What is chosen goes to the theme or slide open now, if it is still open then
        // (`dialogs`).
        if std::mem::take(&mut running.home_state.theme_picture_request) {
            let Some(theme) = dialogs::theme_asked(running.home_state.theme_edit.as_ref()) else { return };
            let window = dialogs::Files::new(
                dialogs::Choose::File,
                crate::i18n::t("notices.dialog.theme_picture", "Choose a Picture for the Theme"),
            )
            .filter(crate::i18n::t("notices.dialog.filter.pictures", "Pictures"), &media::PICTURES);
            self.ask(dialogs::For::ThemePicture { theme }, dialogs::Window::Files(window));
            return;
        }
        if !std::mem::take(&mut running.home_state.picture_request) {
            return;
        }
        let draft = running.home_state.editing.as_ref().map(|editing| editing.serial);
        let window = dialogs::Files::new(
            dialogs::Choose::File,
            crate::i18n::t("notices.dialog.picture_or_video", "Choose a Picture or Video"),
        )
        .filter(crate::i18n::t("notices.dialog.filter.media", "Pictures and Videos"), &media::PICTURE_ENDINGS);
        self.ask(dialogs::For::SlidePicture { draft }, dialogs::Window::Files(window));
    }

    /// A media playlist follows a folder (or none): its files come and go with the folder's.
    fn follow_folder(&mut self, id: Uuid, folder: Option<PathBuf>) {
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

    fn handle_media_request(&mut self) {
        use home::MediaRequest;
        if self.running.as_mut().is_some_and(|r| std::mem::take(&mut r.home_state.media_playlists_changed)) {
            self.save_media_list();
        }
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.media_request.take()) else { return };
        match request {
            MediaRequest::PlayPlaylist(Some(id)) => self.play_media_playlist(id),
            MediaRequest::PlayPlaylist(None) => self.stop_media_playlist(),
            MediaRequest::WatchFolder(id, true) => {
                let window = dialogs::Files::new(
                    dialogs::Choose::Folder,
                    crate::i18n::t("notices.dialog.follow_folder", "Choose a Folder to Follow"),
                );
                self.ask(dialogs::For::FollowFolder { playlist: id }, dialogs::Window::Files(window));
            }
            MediaRequest::WatchFolder(id, false) => self.follow_folder(id, None),
            MediaRequest::Choose => {
                // The system's own window for choosing files, as any Mac app has; the window
                // goes on meanwhile, and what is chosen comes in as a drop does (`dialogs`).
                let window = dialogs::Files::new(
                    dialogs::Choose::Files,
                    crate::i18n::t("notices.dialog.add_media", "Add Pictures and Videos"),
                )
                .filter(crate::i18n::t("notices.dialog.filter.media", "Pictures and Videos"), &media::PICTURE_ENDINGS);
                // What is chosen goes into the media playlist shown now, as a drop on it would.
                let playlist = self.running.as_ref().and_then(|running| running.home_state.media_playlist_shown);
                self.ask(dialogs::For::AddMedia { playlist }, dialogs::Window::Files(window));
            }
            MediaRequest::AddSamples => {
                if let Err(problem) = self.add_sample_backgrounds()
                    && let Some(running) = self.running.as_mut()
                {
                    running.home_state.notices.refuse(problem);
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
        // A change made in the Service view can be undone there. The Edit view keeps its own steps;
        // the Service view's steps for the song from before it opened are dropped, so they can't
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

    /// Writes every song and theme changed to its file (by way of the library's writer, which
    /// writes it a moment later on a thread of its own). The bundled sample has no file. A theme
    /// whose file changed outside the app since it was read is never saved over: it waits on
    /// Reload or Keep mine, holding its changes.
    fn save_unsaved(&mut self) {
        for index in std::mem::take(&mut self.unsaved) {
            self.save_song_file(index, None);
        }
        for index in std::mem::take(&mut self.themes_unsaved) {
            let Some(theme) = self.themes.get(index) else { continue };
            if theme_store::file_of(theme).is_some_and(|file| self.stamps.check(&file) != outside::Now::Same) {
                applog::event("outside", &format!("did not save over the theme {}: it changed outside", theme.name));
                self.hold_theme(index);
                continue;
            }
            self.write_theme(index, false);
        }
        self.unsaved_since = None;
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
    }

    /// A theme whose file changed outside the app: its changes are held, and it waits on Reload
    /// or Keep mine.
    fn hold_theme(&mut self, index: usize) {
        if !self.held_themes.contains(&index) {
            self.held_themes.push(index);
        }
        self.wait_on(outside::Waiting::Theme(index));
    }

    /// Hands a theme to the library's writer to be written to its file, only over the file as
    /// the app last saw it (see `outside`), or over whatever is there with `over_anything` (Keep
    /// mine). The file it had before the project was renamed goes once it is written.
    fn write_theme(&mut self, index: usize, over_anything: bool) {
        let Some(theme) = self.themes.get(index) else { return };
        let file = theme_store::file_of(theme);
        let guard = match &file {
            Some(_) if over_anything => writer::Guard::Any,
            Some(file) => self.stamps.guard(file),
            None => writer::Guard::Any,
        };
        let old = theme_store::old_file_of(theme);
        match theme_store::to_save(theme, guard) {
            Ok(save) => {
                let seq = self.writer.write(save);
                if let Some(file) = &file {
                    self.stamps.writing(file, seq);
                }
                if let Some(old) = old {
                    self.writer.then(move || {
                        if !whole_file::refuses(&old) {
                            let _ = std::fs::remove_file(&old);
                        }
                        Ok(())
                    });
                }
            }
            Err(problem) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.trouble(problem);
                }
            }
        }
    }

    /// Hands the library's writer the note of the main theme and the order of the list, for the
    /// next start.
    fn remember_themes_in_use(&mut self) {
        if let Some(save) = theme_store::in_use_to_save(&self.theme, &self.themes) {
            self.writer.write(save);
        }
    }

    /// The song (not deleted) or theme whose file is `path`.
    fn whose_file(&self, path: &std::path::Path) -> Option<Saved> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        let song = self.sources.iter().enumerate().position(|(at, source)| {
            !removed.contains(&at) && source.as_ref().is_some_and(|source| load::song_file_for(source) == path)
        });
        song.map(Saved::Song).or_else(|| {
            self.themes.iter().position(|theme| theme_store::file_of(theme).as_deref() == Some(path)).map(Saved::Theme)
        })
    }

    /// What a song or theme is called in a notice, by its file; any other file (settings, the
    /// playlists…) by its name.
    fn saved_name(&self, path: &std::path::Path) -> Option<String> {
        let Some(saved) = self.whose_file(path) else {
            return Some(crate::i18n::quoted(&file_name(path)));
        };
        match saved {
            Saved::Song(index) => self.show.presentations().get(index).map(|song| crate::i18n::quoted(&song.title)),
            Saved::Theme(index) => self.themes.get(index).map(|theme| {
                crate::i18n::tf(
                    "notices.ask.the_theme",
                    "The theme {name}",
                    &[("name", &crate::i18n::quoted(&theme_store::shown_theme_name(theme)))],
                )
            }),
        }
    }

    /// Hands the library's writer a save again, after one failed, made from what the app has now:
    /// a song's or theme's, keeping the file as it was in the folder the failed save was to keep
    /// it in first (the library tidy's backup: a save that can't make it is left undone); a
    /// settings file's (paced, as every settings save is), the playlists', the libraries', the
    /// media bin's or the note of the theme in use. A song or theme deleted since, or whose file
    /// changed outside meanwhile (held for Reload or Keep mine), is no longer tried.
    fn save_again(&mut self, due: writer::Due) {
        let writer::Due { path, kind, keep_old_in } = due;
        let path = path.as_path();
        if !matches!(kind, writer::Kind::Song | writer::Kind::Theme) {
            match self.current_save(kind) {
                Some(Ok(save)) if save.path == path => {
                    if matches!(kind, writer::Kind::Settings(_)) {
                        self.paced.save(&self.writer, save, Instant::now());
                    } else {
                        self.writer.write(save);
                    }
                }
                _ => {
                    self.retries.done(path);
                }
            }
            return;
        }
        match self.whose_file(path) {
            Some(Saved::Song(index)) => {
                self.save_song_file(index, keep_old_in);
                if self.held_songs.contains(&index) {
                    self.retries.done(path);
                }
            }
            Some(Saved::Theme(index)) if self.stamps.check(path) == outside::Now::Same => {
                self.write_theme(index, false)
            }
            Some(Saved::Theme(index)) => {
                self.hold_theme(index);
                self.retries.done(path);
            }
            None => {
                self.retries.done(path);
            }
        }
    }

    /// The save of a settings file, the playlists, the libraries, the media bin or the note of
    /// the theme in use, made from what the app has now (`None` for a song or theme, or with no
    /// window yet): for one tried again after a failed save, or kept in "Not saved" at quit.
    fn current_save(&self, kind: writer::Kind) -> Option<Result<writer::Save, String>> {
        use whole_file::What;
        use writer::Kind;
        let state = &self.running.as_ref()?.home_state;
        Some(match kind {
            Kind::Song | Kind::Theme => return None,
            Kind::Playlists | Kind::Settings(What::Playlists) => service::to_save(&state.services),
            Kind::Libraries | Kind::Settings(What::Libraries) => libraries::to_save(&state.libraries),
            Kind::MediaList | Kind::Settings(What::MediaList) => self.media_list_save(),
            Kind::ThemesInUse => Ok(theme_store::in_use_to_save(&self.theme, &self.themes)?),
            Kind::Settings(What::Screens) => screens::to_save(&state.screens),
            Kind::Settings(What::Settings) => service::prefs_to_save(&state.prefs()),
            Kind::Settings(What::Audio) => audio::to_save(&state.audio),
            Kind::Settings(What::Macros) => macros::to_save(&state.macros),
            Kind::Settings(What::Timers) => timers::to_save(&state.timers),
            Kind::Settings(What::Messages) => messages::to_save(&state.messages),
            Kind::Settings(What::Props) => props::to_save(&state.props),
            Kind::Settings(What::StageLayouts) => stage::layouts_to_save(&state.stage_layouts),
            Kind::Settings(What::BiblePage) => {
                service::bible_page_to_save(&service::SavedPassage { lookup: String::new(), ..state.bible.settings() })
            }
            Kind::Settings(What::BibleHistory) => service::bible_history_to_save(&state.bible.history),
        })
    }

    /// While quitting, a file whose save failed is written into the library's "Not saved" folder
    /// instead, once, so the change is kept: a song or theme named by its title and the time, any
    /// other (a settings file, the playlists…) by its own name and the time, as the app has it
    /// now. The log says where, or why it couldn't be.
    fn keep_failed_aside(&mut self, path: &std::path::Path, kind: writer::Kind) {
        let (title, ending, bytes) = match self.whose_file(path) {
            Some(Saved::Song(index)) => {
                let Some(song) = self.show.presentations().get(index) else { return };
                (song.title.clone(), load::SONG_EXTENSION.to_owned(), sp_model::to_json(song).into_bytes())
            }
            Some(Saved::Theme(index)) => {
                let Some(theme) = self.themes.get(index) else { return };
                (theme.name.clone(), "sptheme".to_owned(), theme.to_json().into_bytes())
            }
            None => {
                let Some(Ok(save)) = self.current_save(kind) else { return };
                let stem = path.file_stem().map_or_else(|| file_name(path), |stem| stem.to_string_lossy().into_owned());
                let ending = path.extension().map_or_else(|| "json".to_owned(), |e| e.to_string_lossy().into_owned());
                (stem, ending, (save.content)())
            }
        };
        if self.kept_failed.iter().any(|kept| kept == path) {
            return;
        }
        self.kept_failed.push(path.to_path_buf());
        match keep_in_not_saved(&title, &ending, &bytes) {
            Ok(kept) => applog::problem(
                "saving",
                &format!(
                    "{title}: could not be saved; SoftPresenter's version is kept in Not saved/{}",
                    file_name(&kept)
                ),
            ),
            Err(e) => applog::problem("saving", &format!("{title}: changes not saved ({e})")),
        }
    }

    /// Hands a song to the library's writer, to be written to its file; a song with no file yet
    /// (the sample) is given one in the app's Songs folder, so a change to it is kept like any
    /// other. Bible passages are never saved as songs. A file changed outside the app since it
    /// was read is never saved over: the song waits on Reload or Keep mine, holding its changes.
    /// With `keep_old_in`, the file as it was is copied into that folder first.
    fn save_song_file(&mut self, index: usize, keep_old_in: Option<PathBuf>) {
        let songs_folder = bible_store::data_dir().map(|dir| dir.join("Songs"));
        if !self.app_made(index)
            && let (Some(None), Some(song)) = (self.sources.get(index), self.show.presentations().get(index))
            && let Some(folder) = &songs_folder
        {
            self.sources[index] = Some(folder.join(format!("{}.{}", song.id, load::SONG_EXTENSION)));
        }
        let Some(Some(path)) = self.sources.get(index) else { return };
        let target = load::song_file_for(path);
        if self.stamps.check(&target) != outside::Now::Same {
            applog::event("outside", &format!("did not save over {}: it changed outside", file_name(&target)));
            self.hold_song(index);
            return;
        }
        let in_songs_folder = songs_folder.is_some_and(|folder| target.parent() == Some(folder.as_path()));
        self.write_song(index, target, in_songs_folder, keep_old_in);
    }

    /// Hands a song to the library's writer to be written to `file` (its folder made first if
    /// `make_folder`), from now on the file it lives in. Saved only over the file as the app last
    /// saw it (see `outside`).
    fn write_song(&mut self, index: usize, file: PathBuf, make_folder: bool, keep_old_in: Option<PathBuf>) {
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let mut save = writer::Save::new(file.clone(), writer::Kind::Song, self.stamps.guard(&file), move || {
            sp_model::to_json(&song).into_bytes()
        });
        save.make_folder = make_folder;
        save.keep_old_in = keep_old_in;
        let seq = self.writer.write(save);
        self.stamps.writing(&file, seq);
        if let Some(source) = self.sources.get_mut(index) {
            *source = Some(file);
        }
    }

    /// A song whose file changed outside the app: its changes are held, and it waits on Reload
    /// or Keep mine.
    fn hold_song(&mut self, index: usize) {
        if !self.held_songs.contains(&index) {
            self.held_songs.push(index);
        }
        self.wait_on(outside::Waiting::Song(index));
    }

    /// Takes in what the library's writer did since it was last asked: the files written (now as
    /// the app last saw them), a song kept from a file changed outside meanwhile (held, waiting on
    /// Reload or Keep mine), and what couldn't be written, said in the notices and the log.
    fn finish_saves(&mut self) {
        let mut problems = Vec::new();
        for done in self.writer.finished() {
            let said = match done {
                writer::Done::Written { path, seq, stamp } => {
                    self.stamps.written(&path, seq, Some(stamp));
                    // Saved at last, after saves of it failed: said, as their failure was.
                    if let Some(tries) = self.retries.done(&path) {
                        applog::event("saving", &format!("wrote {} after {tries} failed saves", file_name(&path)));
                        if let Some(name) = self.saved_name(&path)
                            && let Some(running) = self.running.as_mut()
                        {
                            running.home_state.notices.outcome(Ok(crate::i18n::tf(
                                "notices.save.saved_now",
                                "{name} is saved now.",
                                &[("name", &name)],
                            )));
                        }
                    }
                    continue;
                }
                writer::Done::ChangedOutside { path, seq } => {
                    self.stamps.written(&path, seq, None);
                    self.retries.done(&path);
                    applog::event("outside", &format!("did not save over {}: it changed outside", file_name(&path)));
                    let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
                    let song = self.sources.iter().enumerate().position(|(at, source)| {
                        !removed.contains(&at)
                            && source.as_ref().is_some_and(|source| load::song_file_for(source) == path)
                    });
                    if let Some(index) = song {
                        self.hold_song(index);
                    }
                    // A theme, found by its file (the places in the list may have moved since).
                    let theme =
                        self.themes.iter().position(|theme| theme_store::file_of(theme).as_ref() == Some(&path));
                    if let Some(index) = theme {
                        self.hold_theme(index);
                    }
                    continue;
                }
                writer::Done::Failed { path, seq, kind, error, keep_old_in } => {
                    self.stamps.written(&path, seq, None);
                    if self.quitting.is_some() {
                        // Quitting: never tried again (the quit would wait for ever), but kept
                        // in the library's "Not saved" folder, so the change isn't lost.
                        self.retries.done(&path);
                        self.keep_failed_aside(&path, kind);
                        kind.problem(&path, &error)
                    } else {
                        // Tried again after a wait (another program may have held the file for
                        // a moment); said the first time, logged each time.
                        if !self.retries.failed(&path, Instant::now(), kind, keep_old_in) {
                            continue;
                        }
                        format!(
                            "{} {}",
                            kind.problem(&path, &error),
                            crate::i18n::t(
                                "notices.save.trying_again",
                                "SoftPresenter tries again in a moment; if it still can't when it quits, it keeps the change in the library's Not saved folder."
                            )
                        )
                    }
                }
                // A settings file the start could neither read nor keep aside: said the first
                // time, unless the start's notice said so already.
                writer::Done::NotRead { path, seq, kind } => {
                    self.stamps.written(&path, seq, None);
                    match whole_file::refusal(&path) {
                        Some(why) => kind.problem(&path, &std::io::Error::other(why)),
                        None => continue,
                    }
                }
                writer::Done::Problem(said) => {
                    applog::problem("saving", &said);
                    said
                }
            };
            problems.push(said);
        }
        // Many at once (a full disk) are one notice; the log has each.
        let Some(first) = problems.first() else { return };
        let said = match problems.len() {
            1 => first.clone(),
            n => format!(
                "{}{}.",
                first.trim_end_matches('.'),
                crate::i18n::tf(
                    "notices.bundle.more_in_log",
                    " and {n} more (see the log)",
                    &[("n", &crate::i18n::count(n as u64 - 1))]
                )
            ),
        };
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.trouble(said);
            running.home_window.request_redraw();
        }
    }

    /// Asks about a song or theme changed outside SoftPresenter: Reload or Keep mine. The
    /// question waits at the head of the notices (and, for a song, under its title bar) until
    /// it is answered; it never opens the list by itself.
    fn wait_on(&mut self, what: outside::Waiting) {
        let name = match what {
            // Asked by `mirror`, with the device's name; and by `reports`, once.
            outside::Waiting::Mirror(_) | outside::Waiting::Reports => return,
            outside::Waiting::Song(index) => self.show.presentations().get(index).map_or_else(
                || crate::i18n::t("notices.ask.a_song", "A song").to_owned(),
                |song| crate::i18n::quoted(&song.title),
            ),
            outside::Waiting::Theme(index) => self.themes.get(index).map_or_else(
                || crate::i18n::t("notices.ask.a_theme", "A theme").to_owned(),
                |theme| {
                    crate::i18n::tf(
                        "notices.ask.the_theme",
                        "The theme {name}",
                        &[("name", &crate::i18n::quoted(&theme_store::shown_theme_name(theme)))],
                    )
                },
            ),
        };
        if let Some(running) = self.running.as_mut()
            && !running.home_state.notices.asks_about(what)
        {
            running.home_state.notices.ask(
                what,
                crate::i18n::tf(
                    "notices.ask.changed_outside",
                    "{name} was changed outside SoftPresenter. Read it again from its file, or keep SoftPresenter's version and write it over the file?",
                    &[("name", &name)],
                ),
            );
            running.home_window.request_redraw();
        }
    }

    fn stop_waiting(&mut self, what: outside::Waiting) {
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.settle(what);
            running.home_window.request_redraw();
        }
    }

    /// Testing aid: SOFTPRESENTER_NOTICES fills the notices for each look of the status button,
    /// a few frames after the start, in a quiet run as in a capture: `new` (information only),
    /// `trouble`, `ask` (the song shown changed outside SoftPresenter), `list` (all of them, with
    /// the list open), `refuse` (a key refused, again every two seconds so a picture finds its
    /// cue). Nothing else is changed: no file is read or written for it.
    fn notices_testing_aid(&mut self) {
        static LAST_REFUSAL: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        let Ok(look) = std::env::var("SOFTPRESENTER_NOTICES") else { return };
        let Some(frames) = self.running.as_ref().map(|r| r.frames) else { return };
        if look == "refuse" {
            let Ok(mut last) = LAST_REFUSAL.lock() else { return };
            let due = frames >= 5 && last.is_none_or(|at| at.elapsed() >= Duration::from_secs(2));
            if due && let Some(running) = self.running.as_mut() {
                *last = Some(Instant::now());
                running.home_state.notices.refuse("Press Enter to put the picked verse on screen.");
                running.home_window.request_redraw();
            }
            return;
        }
        if frames != 5 {
            return;
        }
        let selected = self.show.selected();
        let Some(running) = self.running.as_mut() else { return };
        let notices = &mut running.home_state.notices;
        if matches!(look.as_str(), "new" | "list") {
            notices.info("\u{201c}Harbour Lights\u{201d} added to Wednesday Evening");
            notices.info("Reading Ukrainian_Ogienko.xml\u{2026}");
            notices.info("Installed Ukrainian Ogienko");
        }
        // The notice a church sees when the playlists can't be written to a full disk.
        if matches!(look.as_str(), "trouble" | "list") {
            notices.trouble(service::not_saved(&std::io::Error::from(std::io::ErrorKind::StorageFull)));
        }
        if matches!(look.as_str(), "list") {
            egui::Popup::open_id(&running.egui_ctx, notices::list_id());
        }
        running.home_window.request_redraw();
        if matches!(look.as_str(), "ask" | "list") {
            self.wait_on(outside::Waiting::Song(selected));
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
                    running.home_state.notices.trouble(problem);
                }
                false
            }
        }
    }

    /// Takes a song's file's version in place of the app's: what was not saved goes, as does
    /// its undo history, and the editor on it starts again from the file. Returns its title.
    fn reload_song(&mut self, index: usize) -> Result<String, String> {
        let title = self.show.presentations().get(index).map(|song| song.title.clone()).unwrap_or_default();
        let path = self.sources.get(index).cloned().flatten().ok_or_else(|| {
            crate::i18n::tf("notices.reload.no_file", "{title} has no file to read.", &[("title", &title)])
        })?;
        let target = load::song_file_for(&path);
        // A save still on its way to the file (the app's own, or a song pack's version written
        // over it) is written first, so what is read is the file as it will be (that file alone,
        // ahead of anything else waiting). One that failed is no longer tried: the file's version
        // is taken.
        self.writer.flush(&target);
        self.finish_saves();
        self.retries.done(&target);
        if !target.is_file() {
            return Err(crate::i18n::tf(
                "notices.reload.file_gone",
                "{title}'s file is gone; Keep mine saves it again.",
                &[("title", &title)],
            ));
        }
        let (mut loaded, problems) = load::load_songs(std::slice::from_ref(&target));
        let song = loaded.pop().map(|loaded| loaded.song).ok_or_else(|| {
            let why = problems.join(" ").replace(&target.display().to_string(), &file_name(&target));
            crate::i18n::tf(
                "notices.reload.unreadable",
                "{title} could not be read again ({why}); SoftPresenter's version is kept.",
                &[("title", &title), ("why", &why)],
            )
        })?;
        self.stamps.note(&target);
        Ok(self.take_version(index, Arc::new(song), target))
    }

    /// Puts a version of a song from its file (`file`) in place of the app's: what was not saved
    /// goes, as does its undo history, and the editor on it starts again from this version.
    /// Returns its title.
    fn take_version(&mut self, index: usize, song: Arc<sp_model::Presentation>, file: PathBuf) -> String {
        let key = song_ids::key_for(Some(&file), song.id, &song.title);
        let title = song.title.clone();
        let changes = self.show.replace_presentation_following(index, song);
        self.sources[index] = Some(file);
        self.history.forget(index);
        self.unsaved.retain(|at| *at != index);
        self.held_songs.retain(|at| *at != index);
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            let state = &mut running.home_state;
            if let Some(have) = state.song_keys.get_mut(index) {
                *have = key;
            }
            state.notices.settle(outside::Waiting::Song(index));
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
        title
    }

    /// Writes the app's version of a song over its file, which changed outside the app.
    fn keep_song(&mut self, index: usize) -> Result<String, String> {
        if self.running.as_ref().is_some_and(|r| r.home_state.removed_songs.contains(&index)) {
            return Err(crate::i18n::t("notices.keep.song_deleted", "That song was deleted.").to_owned());
        }
        let (Some(Some(path)), Some(song)) = (self.sources.get(index), self.show.presentations().get(index)) else {
            return Err(crate::i18n::t("notices.keep.no_file", "That song has no file.").to_owned());
        };
        let title = song.title.clone();
        let file = load::song_file_for(path);
        // Over whatever is there now (a save that fails is said when the writer gets to it).
        let song = song.clone();
        let seq =
            self.writer.write(writer::Save::new(file.clone(), writer::Kind::Song, writer::Guard::Any, move || {
                sp_model::to_json(&song).into_bytes()
            }));
        self.stamps.writing(&file, seq);
        self.sources[index] = Some(file);
        self.held_songs.retain(|at| *at != index);
        self.unsaved.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Song(index));
        Ok(title)
    }

    /// Takes a theme's file's version in place of the app's.
    fn reload_theme(&mut self, index: usize) -> Result<String, String> {
        if self.running.as_ref().is_some_and(|r| r.home_state.theme_edit.is_some()) {
            return Err(crate::i18n::t(
                "notices.reload.theme_editor_open",
                "Close the Theme editor first, then reload the theme.",
            )
            .to_owned());
        }
        let theme = self.themes.get(index).ok_or(crate::i18n::t("notices.theme_gone", "That theme is gone."))?;
        let file = theme_store::file_of(theme)
            .ok_or(crate::i18n::t("notices.reload.theme_no_file", "The theme has no file."))?;
        // The app's own saves of it on their way are written first, so the file read is not
        // taken over by one of them a moment later; one that failed is no longer tried.
        self.writer.flush(&file);
        self.finish_saves();
        self.retries.done(&file);
        let theme = self.themes.get(index).ok_or(crate::i18n::t("notices.theme_gone", "That theme is gone."))?;
        let text = std::fs::read_to_string(&file).map_err(|e| {
            crate::i18n::tf(
                "notices.theme.unreadable",
                "The theme {name} could not be read: {error}",
                &[("name", &theme_store::shown_theme_name(theme)), ("error", &e.to_string())],
            )
        })?;
        let read = sp_model::Theme::from_json(&text).map_err(|e| {
            crate::i18n::tf(
                "notices.theme.unreadable",
                "The theme {name} could not be read: {error}",
                &[("name", &theme_store::shown_theme_name(theme)), ("error", &e.to_string())],
            )
        })?;
        if read.id != theme.id {
            return Err(crate::i18n::tf(
                "notices.theme.other_inside",
                "The file of the theme {name} now holds another theme.",
                &[("name", &theme_store::shown_theme_name(theme))],
            ));
        }
        let name = theme_store::shown_theme_name(&read).into_owned();
        self.themes[index] = read;
        self.stamps.note(&file);
        self.held_themes.retain(|at| *at != index);
        self.themes_unsaved.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Theme(index));
        self.themes_changed();
        self.refresh_output();
        Ok(name)
    }

    /// Writes the app's version of a theme over its file, which changed outside the app (by way
    /// of the library's writer).
    fn keep_theme(&mut self, index: usize) -> Result<String, String> {
        let theme = self.themes.get(index).ok_or(crate::i18n::t("notices.theme_gone", "That theme is gone."))?;
        let name = theme_store::shown_theme_name(theme).into_owned();
        self.write_theme(index, true);
        self.held_themes.retain(|at| *at != index);
        self.stop_waiting(outside::Waiting::Theme(index));
        Ok(name)
    }

    /// Carries out the choice made in the bar that offers Reload and Keep mine.
    fn choose_outside(&mut self, what: outside::Waiting, reload: bool) {
        // The one-time notice that anonymous reports are on: Keep on or Turn off.
        if what == outside::Waiting::Reports {
            reports::set_enabled(reload);
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.settle(what);
                if !reload {
                    running
                        .home_state
                        .notices
                        .info(crate::i18n::t("reports.turned_off", "Anonymous reports are off. Nothing was sent."));
                }
                running.home_window.request_redraw();
            }
            return;
        }
        // A device that wants to show its screen: Allow or Don't Allow.
        if let outside::Waiting::Mirror(device) = what {
            let said = self.mirror_answer(device, reload);
            if let (Err(problem), Some(running)) = (said, self.running.as_mut()) {
                running.home_state.notices.info(problem);
                running.home_window.request_redraw();
            }
            return;
        }
        let result = match (what, reload) {
            (outside::Waiting::Song(index), true) => self.reload_song(index),
            (outside::Waiting::Song(index), false) => self.keep_song(index),
            (outside::Waiting::Theme(index), true) => self.reload_theme(index),
            (outside::Waiting::Theme(index), false) => self.keep_theme(index),
            (outside::Waiting::Mirror(_) | outside::Waiting::Reports, _) => return,
        };
        // The answer is noted in the list (a problem with it too, and the question stays).
        let said = match result {
            Ok(name) if reload => {
                Ok(crate::i18n::tf("notices.outside.reloaded", "Read {name} again from its file.", &[("name", &name)]))
            }
            Ok(name) => Ok(crate::i18n::tf(
                "notices.outside.kept",
                "Saved SoftPresenter's version of {name}.",
                &[("name", &name)],
            )),
            Err(problem) => Err(problem),
        };
        applog::event("outside", said.as_ref().unwrap_or_else(|problem| problem));
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.outcome(said);
            running.home_window.request_redraw();
        }
    }

    /// On quitting: the changes held back because their files changed outside the app are
    /// written to the library's "Not saved" folder (named by title and time), so neither
    /// version is lost, and the log says where. Each once: quitting looks again after the
    /// library's writer has finished, for a song it held back only then.
    fn keep_held_aside(&mut self) {
        let Some(folder) = bible_store::data_dir().map(|dir| dir.join("Not saved")) else { return };
        let when = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
        let plain = pack::safe_file_name;
        for index in self.held_songs.clone() {
            if self.kept_aside.0.contains(&index) {
                continue;
            }
            self.kept_aside.0.push(index);
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
            if self.kept_aside.1.contains(&index) {
                continue;
            }
            self.kept_aside.1.push(index);
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
                .is_some_and(|r| r.home_state.notices.asks_about(outside::Waiting::Song(index)))
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
            let copy = self.show.presentations().iter().any(|have| have.id == song.id);
            if copy {
                song.id = Uuid::new_v4();
            }
            self.stamps.note(&path);
            let key = song_ids::key_for(Some(&path), song.id, &song.title);
            done.added.push(song.title.clone());
            let index = self.show.add_presentation(Arc::new(song));
            self.sources.push(Some(path.clone()));
            if let Some(running) = self.running.as_mut() {
                running.home_state.song_keys.push(key);
            }
            if copy {
                self.write_song(index, path, false, None);
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
                            Ok(name) => done.read.push(crate::i18n::tf(
                                "notices.reload.the_theme",
                                "the theme {name}",
                                &[("name", &name)],
                            )),
                            Err(problem) => done.problems.push(problem),
                        }
                    }
                    _ => {
                        done.waiting.push(crate::i18n::tf(
                            "notices.reload.the_theme",
                            "the theme {name}",
                            &[("name", &theme_store::shown_theme_name(&self.themes[index]))],
                        ));
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
        let running = self.running.as_ref().ok_or(crate::i18n::t("notices.not_ready", "The app isn't ready."))?;
        let state = &running.home_state;
        let mut indices: Vec<usize> = indices.to_vec();
        let playlist_file = match playlist {
            Some(at) => {
                let list = state
                    .services
                    .list
                    .get(at)
                    .ok_or(crate::i18n::t("notices.pack.no_playlist", "No such playlist."))?;
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
            return Err(crate::i18n::t("notices.pack.no_songs", "No songs to pack.").to_owned());
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
        Ok(crate::i18n::tf(
            "notices.pack.writing",
            "Writing {name}, with its videos ({size}); the window says when it is done.",
            &[("name", &name), ("size", &size)],
        ))
    }

    /// Makes a song pack from the window: asks whether to put the videos in (with their size)
    /// and where to save it, in windows that leave the app going meanwhile (`dialogs`).
    fn export_from_window(&mut self, indices: &[usize], playlist: Option<usize>) {
        let export = match self.plan_pack(indices, playlist) {
            Ok(export) => export,
            Err(problem) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.outcome(Err(problem));
                    running.home_window.request_redraw();
                }
                return;
            }
        };
        let name = match playlist {
            Some(at) => self.running.as_ref().and_then(|r| r.home_state.services.list.get(at)).map(|l| l.name.clone()),
            None if export.songs.len() == 1 => export.songs.first().map(|song| song.title.clone()),
            None => Some(crate::i18n::t("notices.export.file_name", "Songs").to_owned()),
        };
        let file = format!(
            "{}.{}",
            pack::safe_file_name(
                &name.unwrap_or_else(|| crate::i18n::t("notices.export.file_name", "Songs").to_owned())
            ),
            pack::ENDING
        );
        if export.videos.is_empty() {
            return self.ask_where_to_save(Box::new(export), file);
        }
        let size = pack::size_words(export.video_bytes());
        let videos = if export.videos.len() == 1 {
            crate::i18n::t("notices.export.a_video", "a video").to_owned()
        } else {
            crate::i18n::tf(
                "notices.export.videos",
                "{n} videos",
                &[("n", &crate::i18n::count(export.videos.len() as u64))],
            )
        };
        let question = dialogs::Question {
            title: crate::i18n::t("notices.export.title", "Export Songs").to_owned(),
            words: crate::i18n::tf(
                "notices.export.ask_videos",
                "The songs use {videos} ({size}). Put them in the song pack?",
                &[("videos", &videos), ("size", &size)],
            ),
            buttons: [
                crate::i18n::tf("notices.export.include", "Include Videos ({size})", &[("size", &size)]),
                crate::i18n::t("notices.export.leave_out", "Leave Them Out").to_owned(),
                crate::i18n::t("notices.dialog.cancel", "Cancel").to_owned(),
            ],
        };
        self.ask(dialogs::For::ExportVideos { export: Box::new(export), file }, dialogs::Window::Question(question));
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
            return Err(crate::i18n::t("notices.no_library_folder", "Could not find the library folder.").to_owned());
        };
        let staging = dir.join(format!(".opening-pack-{}", Uuid::new_v4()));
        let small = std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= PACK_READ_AT_ONCE);
        if now || small || self.running.is_none() {
            return match pack::open(&path, &staging) {
                // Some of its songs are here already: the window asks what to do with them, and
                // the songs come in with the answer.
                Ok(opened) if let Some(already) = self.pack_songs_to_ask(&opened, same) => {
                    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    self.ask_pack_same(opened, library, None, already);
                    Ok(crate::i18n::tf(
                        "notices.pack.opening",
                        "Opening {name}; the window says when its songs are in.",
                        &[("name", &name)],
                    ))
                }
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
        Ok(crate::i18n::tf(
            "notices.pack.opening",
            "Opening {name}; the window says when its songs are in.",
            &[("name", &name)],
        ))
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
            self.finish_job(Some(job), result);
        }
    }

    /// Finishes the song packs written or read on threads of their own since the last look.
    fn finish_packs(&mut self) {
        while let Ok(done) = self.pack_rx.try_recv() {
            // A bundle's songs are said as any import's are (`say_imported`), once.
            let mut said_already = false;
            let (job, result) = match done {
                PackJob::Wrote { job, result } => (job, result),
                PackJob::Opened { job, result, same, library } => match *result {
                    // Some of its songs are here already: the job ends with the answer.
                    Ok(opened) if let Some(already) = self.pack_songs_to_ask(&opened, same) => {
                        self.ask_pack_same(opened, library, Some(job), already);
                        continue;
                    }
                    Ok(opened) => (job, self.apply_pack(opened, same, library)),
                    Err(problem) => (job, Err(problem)),
                },
                PackJob::Bundle { job, result, path, to, library } => match *result {
                    Ok(opened) => {
                        let mut done = Imported::default();
                        self.take_songs(vec![SongRead::Bundle { opened, path, library }], &to, &mut done);
                        let said = self.say_imported(&done);
                        said_already = true;
                        (job, if done.landed.is_empty() { Err(said) } else { Ok(said) })
                    }
                    Err(problem) => (job, Err(problem)),
                },
            };
            self.pack_finished(Some(job), result, said_already);
        }
    }

    /// Says how a song pack's job went: in the log, to `job N`, and (unless it has been said)
    /// in the window.
    fn pack_finished(&mut self, job: Option<u64>, result: Result<String, String>, said_already: bool) {
        applog::event("pack", result.as_ref().unwrap_or_else(|problem| problem));
        if let Some((_, kept)) = job.and_then(|job| self.jobs.iter_mut().find(|(have, _)| *have == job)) {
            *kept = Some(result.clone());
        }
        if let Some(running) = self.running.as_mut() {
            if !said_already {
                running.home_state.notices.outcome(result);
            }
            running.home_window.request_redraw();
        }
    }

    /// How many of an opened pack's songs the library has already, when it isn't said what to
    /// do with them and the window is there to ask; `None` when there is nothing to ask.
    fn pack_songs_to_ask(&self, opened: &pack::Opened, same: Option<pack::Same>) -> Option<usize> {
        let running = self.running.as_ref()?;
        if same.is_some() {
            return None;
        }
        let removed = &running.home_state.removed_songs;
        let already =
            opened
                .songs
                .iter()
                .filter(|song| {
                    self.show.presentations().iter().enumerate().any(|(at, have)| {
                        have.id == song.id && !self.bible_slots.contains(&at) && !removed.contains(&at)
                    })
                })
                .count();
        (already > 0).then_some(already)
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
            self.bin_pictures.read_length(new, &item.path);
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
            return Err(crate::i18n::t("notices.no_library_folder", "Could not find the library folder.").to_owned());
        };
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        let existing = |show: &sp_engine::Show, id: Uuid, bible: &[usize; 2]| {
            show.presentations()
                .iter()
                .enumerate()
                .position(|(at, song)| song.id == id && !bible.contains(&at) && !removed.contains(&at))
        };
        // Songs the library has already: as asked (the window asks first, `pack_songs_to_ask`),
        // else skipped.
        let same = same.unwrap_or(pack::Same::Skip);
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
                    self.themes.push(theme);
                    self.write_theme(self.themes.len() - 1, false);
                    themes_in.insert(id, id);
                    new_themes += 1;
                    id
                }),
                None => None,
            };
            let title = song.title.clone();
            match here {
                // Replaced: written over its file, and taken as an outside change is: at once,
                // unless the song is on screen, being edited or holding changes, when it waits on a
                // choice in the bar (Reload takes the pack's version from the file).
                Some(index) => match self.sources.get(index).cloned().flatten() {
                    Some(path) => {
                        let file = load::song_file_for(&path);
                        let song = Arc::new(song);
                        let written = song.clone();
                        let save = writer::Save::new(file.clone(), writer::Kind::Song, writer::Guard::Any, move || {
                            sp_model::to_json(&written).into_bytes()
                        });
                        let in_use = self.song_in_use(index, true)
                            || self.unsaved.contains(&index)
                            || self.held_songs.contains(&index);
                        if in_use {
                            // Not the app's own save: once written, the file reads as changed. The
                            // app's own saves of it are written first (so none waits to be merged
                            // with this one, or is left counting as on its way); a save of the
                            // song after it finds the pack's version and waits on the choice.
                            self.writer.flush(&file);
                            self.finish_saves();
                            self.writer.write(save);
                            self.wait_on(outside::Waiting::Song(index));
                            waiting.push(title);
                        } else {
                            self.take_version(index, song, file.clone());
                            let seq = self.writer.write(save);
                            self.stamps.writing(&file, seq);
                            replaced.push(title);
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
                services.list.push(service::Service {
                    name: name.clone(),
                    items: playlist.items,
                    uids: Default::default(),
                    origin: None,
                });
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
            parts.push(crate::i18n::tn(
                "notices.pack.imported",
                added.len() as u64,
                "Imported {n} song: {names}.",
                "Imported {n} songs: {names}.",
                &[("names", &list(&added))],
            ));
        }
        if !replaced.is_empty() {
            parts.push(crate::i18n::tf("notices.pack.replaced", "Replaced: {names}.", &[("names", &list(&replaced))]));
        }
        if !skipped.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.pack.skipped",
                "Skipped, already here: {names}.",
                &[("names", &list(&skipped))],
            ));
        }
        if !waiting.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.pack.waiting",
                "Written, waiting on Reload or Keep mine (in use or holding changes): {names}.",
                &[("names", &list(&waiting))],
            ));
        }
        let playlist_in = playlist_name.is_some();
        match playlist_name {
            Some(name) if name.ends_with("(here already)") => {
                parts.push(crate::i18n::tf(
                    "notices.pack.playlist_here",
                    "The playlist {name} is here already.",
                    &[("name", name.trim_end_matches(" (here already)"))],
                ));
            }
            Some(name) => parts.push(crate::i18n::tf(
                "notices.pack.playlist_added",
                "Added the playlist {name}.",
                &[("name", &name)],
            )),
            None => {}
        }
        if new_themes > 0 {
            parts.push(crate::i18n::tn(
                "notices.pack.themes_added",
                new_themes as u64,
                "Added {n} theme.",
                "Added {n} themes.",
                &[],
            ));
        }
        if media_playlists_added > 0 {
            parts.push(crate::i18n::tn(
                "notices.pack.media_playlists_added",
                media_playlists_added as u64,
                "Added {n} media playlist.",
                "Added {n} media playlists.",
                &[],
            ));
        }
        let (pictures, videos) = (brought.pictures, brought.videos);
        let things: Vec<String> = [(pictures, "picture"), (videos, "video")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| match what {
                "picture" => crate::i18n::tn("notices.pack.pictures", n as u64, "{n} picture", "{n} pictures", &[]),
                _ => crate::i18n::tn("notices.pack.videos_plain", n as u64, "{n} video", "{n} videos", &[]),
            })
            .collect();
        if !things.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.pack.brought",
                "Brought {what}.",
                &[("what", &things.join(crate::i18n::t("notices.list.and", " and ")))],
            ));
        }
        if lost_actions > 0 {
            parts.push(if lost_actions == 1 {
                crate::i18n::t(
                    "notices.pack.lost_action",
                    "1 slide action points at a message, prop, timer or media item this computer doesn't have; kept, shown as missing.",
                )
                .to_owned()
            } else {
                crate::i18n::tf(
                    "notices.pack.lost_actions",
                    "{n} slide actions point at messages, props, timers or media items this computer doesn't have; kept, shown as missing.",
                    &[("n", &crate::i18n::count(lost_actions as u64))],
                )
            });
        }
        if !opened.ignored.is_empty() {
            parts.push(crate::i18n::tf(
                "notices.pack.left_out",
                "Left out of the pack: {names}.",
                &[("names", &list(&opened.ignored))],
            ));
        }
        // Nothing usable in the pack (only things left out) is a failure; songs skipped because
        // they are here already are not.
        let anything =
            !added.is_empty() || !replaced.is_empty() || !skipped.is_empty() || !waiting.is_empty() || playlist_in;
        let said = if parts.is_empty() {
            crate::i18n::t("notices.pack.empty", "The pack had no songs in it.").to_owned()
        } else {
            parts.join(" ")
        };
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
            // The arrangement that sings the chorus after each verse follows the new name.
            renamed.follow_parts_from(song);
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
        // gives its sections and details, and a song with a verse and a chorus sings the chorus
        // after each verse); with none, one empty slide in a first verse.
        let mut parsed = sp_model::import_song_text(&title, &asked.words);
        parsed.name_chorus_after(&home::chorus_after_name());
        let copyright = parsed.copyright.clone();
        let (groups, arrangements, arrangement) = if parsed.slide_count() > 0 {
            (parsed.groups, parsed.arrangements, parsed.arrangement)
        } else {
            let empty = sp_model::Group {
                id: Uuid::new_v4(),
                name: "Verse 1".to_owned(),
                color: sp_model::group_color("Verse 1"),
                slides: vec![Slide::new(vec![sp_model::Object::text(sp_model::Role::primary(), "")])],
            };
            (vec![empty], Vec::new(), None)
        };
        let theme = asked.theme.and_then(|at| self.themes.get(at)).map(|theme| theme.id);
        let song = sp_model::Presentation {
            id,
            title: title.clone(),
            groups,
            copyright,
            theme,
            arrangements,
            arrangement,
            loops: false,
            transition: None,
            catalog: None,
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
        let _stall = stall::inside(stall::Doing::Importing);
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
                let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                if to.deck_as == Some(deck::ImportAs::Slides) && !deck::reads_as_slides(path) {
                    done.problems.push(crate::i18n::tf(
                        "notices.import.pictures_only",
                        "{name} can only come in as pictures: it isn't a .pptx file.",
                        &[("name", &name)],
                    ));
                    continue;
                }
                match self.deck_mode(path, to.deck_as) {
                    deck::ImportAs::Slides => {
                        let source = deck::Source::Presentation { path: path.clone(), mode: deck::ImportAs::Slides };
                        done.decks.push(source.name());
                        self.start_deck(source);
                    }
                    deck::ImportAs::Pictures => match convert::program_for(path) {
                        Some(_) => {
                            let source =
                                deck::Source::Presentation { path: path.clone(), mode: deck::ImportAs::Pictures };
                            done.decks.push(crate::i18n::tf(
                                "notices.import.as_pictures",
                                "{name} (as pictures)",
                                &[("name", &source.name())],
                            ));
                            self.start_deck(source);
                        }
                        None => done.problems.push(convert::none_installed(path)),
                    },
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
                    Ok(book) => {
                        read.extend(book.songs.into_iter().map(|(song, book)| SongRead::Song {
                            song: Ok(song),
                            path: path.clone(),
                            library: book.or_else(|| library.clone()),
                        }));
                        // What the operator should know of it is said with the problems.
                        read.extend(book.cautions.into_iter().map(|caution| SongRead::Song {
                            song: Err(caution),
                            path: path.clone(),
                            library: None,
                        }));
                    }
                    Err(problem) => {
                        done.refuse(&problem, path);
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
            if let Err(problem) = &song {
                done.refuse(problem, path);
            }
            read.push(SongRead::Song { song, path: path.clone(), library: library.clone() });
        }
        self.take_songs(read, to, &mut done);
        let here = done.problems.iter().filter(|problem| problem.ends_with(already())).count();
        let ms = started.elapsed().as_millis();
        applog::event(
            "songs",
            &imported_line(done.titles.len(), files.len(), ms, done.refused, here, done.opening.len()),
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
            done.problems
                .push(crate::i18n::t("notices.no_library_folder", "Could not find the library folder.").to_owned());
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
                    done.refuse(&problem, path);
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

    /// The songs an import looks for a song being imported in, with their places: all but the
    /// songs deleted in the app (or a Bible page) and the app's own presentations. A deleted
    /// song isn't "already here": its file can be imported again.
    fn import_compares_with(&self) -> Vec<(usize, Arc<sp_model::Presentation>)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        self.show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, _)| !removed.contains(at) && !self.app_made(*at))
            .map(|(at, song)| (at, song.clone()))
            .collect()
    }

    /// Every id the library has given out, a deleted song's included: its id stays taken, so a
    /// song imported in its place gets an id of its own.
    fn import_ids(&self) -> std::collections::HashSet<Uuid> {
        self.show.presentations().iter().map(|s| s.id).collect()
    }

    /// Takes in the songs read for an import (see [`App::import_songs_to`]), noting in `done`
    /// what came in, what was here already and what couldn't come.
    fn take_songs(&mut self, read: Vec<SongRead>, to: &SongsTo, done: &mut Imported) {
        // What the library has, looked up once rather than song by song.
        let mut ids = self.import_ids();
        let mut have: HashMap<(String, String), usize> =
            self.import_compares_with().iter().map(|(at, s)| (import_key(s), *at)).collect();
        let chosen_library = to.library.or(self.import_library_override);
        if let Some(Some(id)) = to.library {
            done.library = self.running.as_ref().and_then(|r| {
                r.home_state.libraries.iter().find(|library| library.id == id).map(|library| library.name.clone())
            });
        }
        for read in read {
            match read {
                SongRead::Song { song: Ok(song), path, library } => {
                    self.take_song(song, &path, &library, None, chosen_library, (&mut have, &mut ids), false, done);
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
                            (&mut have, &mut ids),
                            false,
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
                        let more = if more > 0 {
                            crate::i18n::tf(
                                "notices.bundle.more_in_log",
                                " and {n} more (see the log)",
                                &[("n", &crate::i18n::count(more as u64))],
                            )
                        } else {
                            String::new()
                        };
                        done.notes.push(match opened.songs.as_slice() {
                            [one] => crate::i18n::tf(
                                "notices.bundle.left_out_song",
                                "{title}: left out of the bundle: {list}{more}.",
                                &[("title", &one.song.title), ("list", &lost.join(", ")), ("more", &more)],
                            ),
                            _ => crate::i18n::tf(
                                "notices.bundle.left_out",
                                "Left out of {file}: {list}{more}.",
                                &[("file", &file_name(&path)), ("list", &lost.join(", ")), ("more", &more)],
                            ),
                        });
                    }
                    // Its own folder goes once its songs are in (or aren't coming).
                    let _ = std::fs::remove_dir_all(&opened.staging);
                }
            }
        }
        self.add_imported_to_playlist(to, done);
        // A download that ended in the background leaves the window as the operator has it.
        if to.quiet {
            return;
        }
        if let Some(at) = done.first {
            self.apply(vec![Command::Select(at)]);
        }
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
        (have, ids): (&mut HashMap<(String, String), usize>, &mut std::collections::HashSet<Uuid>),
        only_new: bool,
        done: &mut Imported,
    ) {
        // A song from another program's file comes in tidied, before it is looked for in the
        // library, so the same file imported again is found there (`load::as_imported`).
        song = load::as_imported(song, path);
        let look = import_key(&song);
        if let Some(at) = already_here(&song, have, ids, self.show.presentations()) {
            done.problems.push(format!("{}{}", song.title, already()));
            done.first.get_or_insert(at);
            done.landed.push(at);
            // A song in two of the folders imported goes into both their libraries; one dropped
            // on a library goes into it. Settings › Song files' Find brings only new songs into
            // the library of their songbook (`only_new`).
            if !only_new && let Some(running) = self.running.as_mut() {
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
                pictures.read_length(id, path);
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
            bringing.failed.extend(failed.into_iter().map(|name| {
                crate::i18n::tf("notices.bundle.unreadable_media", "{name} (couldn't be read)", &[("name", &name)])
            }));
            done.pictures += added.0;
            done.videos += added.1;
        }
        if !read.missing.is_empty() {
            done.notes.push(crate::i18n::tf(
                "notices.bundle.missing",
                "{title}: not in the bundle: {list}.",
                &[("title", &title), ("list", &read.missing.join(", "))],
            ));
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
            .filter_map(|&at| Some(service::Item::song(state.song_keys.get(at)?.clone(), songs.get(at)?.title.clone())))
            .collect();
        let services = &mut state.services;
        let found = if services.list.get(spot.index).is_some_and(|playlist| playlist.name == spot.name) {
            Some(spot.index)
        } else {
            services.list.iter().position(|playlist| playlist.name == spot.name)
        };
        let Some(at) = found.filter(|_| !items.is_empty()) else { return };
        services.put_in(at, spot.after, items);
        done.playlist = Some(services.list[at].name.clone());
    }

    /// Says in the notices what an import did (trouble when a file couldn't be read, else information:
    /// files read are out of sight), and returns the words.
    fn say_imported(&mut self, done: &Imported) -> String {
        for problem in &done.problems {
            let logged = applog::file_kind::noted(problem, done.kinds.get(problem).map(String::as_str));
            applog::event("songs", &format!("not imported: {logged}"));
        }
        let into = match done.libraries_used.as_slice() {
            [] => done.library.as_ref().map_or_else(String::new, |name| {
                crate::i18n::tf("notices.import.into_library", " into the library {name}", &[("name", name)])
            }),
            [one] => crate::i18n::tf("notices.import.into_library", " into the library {name}", &[("name", one)]),
            many => crate::i18n::tf(
                "notices.import.into_libraries",
                " into {n} libraries",
                &[("n", &crate::i18n::count(many.len() as u64))],
            ),
        };
        let busy = !done.decks.is_empty() || !done.opening.is_empty() || !done.packs.is_empty();
        // Several songs here already (a bundle's, say) are said together.
        let all_here = done.problems.len() > 1 && done.problems.iter().all(|problem| problem.ends_with(already()));
        let said = match (done.titles.as_slice(), done.problems.as_slice()) {
            ([], []) if busy => None,
            ([], []) => {
                Some(crate::i18n::t("notices.import.nothing", "There were no songs to import there.").to_owned())
            }
            ([one], []) => Some(crate::i18n::tf(
                "notices.import.imported_one",
                "Imported {title}{into}.",
                &[("title", one), ("into", &into)],
            )),
            (many, []) => Some(crate::i18n::tf(
                "notices.import.imported_many",
                "Imported {n} songs{into}.",
                &[("n", &crate::i18n::count(many.len() as u64)), ("into", &into)],
            )),
            ([], [problem]) => Some(problem.clone()),
            ([], rest) if all_here => Some(crate::i18n::tf(
                "notices.import.all_here",
                "All {n} songs are already in the library.",
                &[("n", &crate::i18n::count(rest.len() as u64))],
            )),
            ([], rest) => Some(if rest.len() > 1 {
                crate::i18n::tf(
                    "notices.import.none_more",
                    "No songs imported. {problem} (and {more} more; see the log)",
                    &[("problem", &rest[0]), ("more", &crate::i18n::count(rest.len() as u64 - 1))],
                )
            } else {
                crate::i18n::tf("notices.import.none", "No songs imported. {problem}", &[("problem", &rest[0])])
            }),
            (imported, rest) if all_here => Some(crate::i18n::tf(
                "notices.import.some_here",
                "Imported {n} of {total} songs{into}; the other {rest} are already in the library.",
                &[
                    ("n", &crate::i18n::count(imported.len() as u64)),
                    ("total", &crate::i18n::count((imported.len() + rest.len()) as u64)),
                    ("into", &into),
                    ("rest", &crate::i18n::count(rest.len() as u64)),
                ],
            )),
            (imported, rest) => Some(if rest.len() > 1 {
                crate::i18n::tf(
                    "notices.import.some_more",
                    "Imported {n} of {total} songs{into}. {problem} (and {more} more; see the log)",
                    &[
                        ("n", &crate::i18n::count(imported.len() as u64)),
                        ("total", &crate::i18n::count((imported.len() + rest.len()) as u64)),
                        ("into", &into),
                        ("problem", &rest[0]),
                        ("more", &crate::i18n::count(rest.len() as u64 - 1)),
                    ],
                )
            } else {
                crate::i18n::tf(
                    "notices.import.some",
                    "Imported {n} of {total} songs{into}. {problem}",
                    &[
                        ("n", &crate::i18n::count(imported.len() as u64)),
                        ("total", &crate::i18n::count((imported.len() + rest.len()) as u64)),
                        ("into", &into),
                        ("problem", &rest[0]),
                    ],
                )
            }),
        };
        let mut words: Vec<String> = said.into_iter().collect();
        words.extend(done.packs.iter().cloned());
        let things: Vec<String> = [(done.pictures, "picture"), (done.videos, "video")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| match what {
                "picture" => crate::i18n::tn("notices.pack.pictures", n as u64, "{n} picture", "{n} pictures", &[]),
                _ => crate::i18n::tn("notices.pack.videos_plain", n as u64, "{n} video", "{n} videos", &[]),
            })
            .collect();
        if !things.is_empty() {
            words.push(crate::i18n::tf(
                "notices.import.brought",
                "Brought {what} into the media bin.",
                &[("what", &things.join(crate::i18n::t("notices.list.and", " and ")))],
            ));
        }
        words.extend(done.notes.iter().cloned());
        if let Some(name) = &done.playlist {
            words.push(crate::i18n::tf(
                "notices.import.to_playlist",
                "Added to the playlist {name}.",
                &[("name", name)],
            ));
        }
        // Songs here already, dropped on a library: they are in it now.
        if let Some(name) = done.library.as_ref().filter(|_| done.titles.is_empty() && !done.landed.is_empty()) {
            words.push(crate::i18n::tf("notices.import.to_library", "Put into the library {name}.", &[("name", name)]));
        }
        if !done.decks.is_empty() {
            words.push(crate::i18n::tf(
                "notices.import.making_slides",
                "Making slides from {names}\u{2026}",
                &[("names", &done.decks.join(", "))],
            ));
        }
        if !done.opening.is_empty() {
            words.push(crate::i18n::tn(
                "notices.import.opening",
                done.opening.len() as u64,
                "Opening {names}; the window says when its songs are in.",
                "Opening {names}; the window says when their songs are in.",
                &[("names", &done.opening.join(", "))],
            ));
        }
        let text = words.join(" ");
        if let Some(running) = self.running.as_mut() {
            if done.problems.iter().any(|problem| !problem.ends_with(already())) {
                running.home_state.notices.trouble(text.clone());
            } else {
                running.home_state.notices.info(text.clone());
            }
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
                Ok(made) => self.add_song(made.presentation, false),
                Err(problem) => applog::event("slides", &problem),
            }
            return;
        };
        // The event loop is woken directly: a repaint request alone doesn't reach a window that is
        // hidden or behind others, and the deck would wait for the next time it is drawn.
        let ctx = running.egui_ctx.clone();
        let waker = self.waker.clone();
        self.decks.start(source, move || match &waker {
            Some(waker) => {
                let _ = waker.send_event(AppEvent::DeckDone);
            }
            None => ctx.request_repaint(),
        });
    }

    /// Carries out what the setup asked: importing songs, which needs more than the window.
    fn handle_setup(&mut self) {
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.setup_request.take()) else { return };
        let mark = self.running.as_ref().map_or(0, |r| r.home_state.notices.mark());
        match request {
            setup::SetupRequest::ImportLibrary(folder) => {
                self.import_songs(&[folder]);
            }
            // The answer comes later: its page says how it went then (`import_songs_chosen`).
            setup::SetupRequest::ChooseSongFiles => return self.choose_songs_to_import(true),
        }
        if let Some(running) = self.running.as_mut() {
            // The setup's own page says how bringing songs in went, as the notices do.
            let said = running.home_state.notices.said_since(mark).map(str::to_owned);
            if let Some(setup) = running.home_state.setup.as_mut() {
                setup.songs_said = said;
            }
            running.home_window.request_redraw();
        }
    }

    /// Adds the decks laid out since the last look, says how each went, and puts into their
    /// slides the pictures drawn since (a slide's thumbnail is drawn again once its picture is
    /// there).
    fn handle_decks(&mut self) {
        // Decks an earlier run was quit in the middle of finish now, once the window is there.
        if !self.decks_resumed && self.running.is_some() {
            self.decks_resumed = true;
            let waker = self.waker.clone();
            let ctx = self.running.as_ref().map(|running| running.egui_ctx.clone());
            let waiting = self.decks.resume(move || match (&waker, &ctx) {
                (Some(waker), _) => {
                    let _ = waker.send_event(AppEvent::DeckDone);
                }
                (None, Some(ctx)) => ctx.request_repaint(),
                (None, None) => {}
            });
            if let Some(running) = self.running.as_mut() {
                running.home_state.pages_waiting.extend(waiting);
            }
        }
        for (path, made) in self.decks.finished() {
            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let said = match made {
                Ok(made) => {
                    let count = made.presentation.slide_count();
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.pages_waiting.extend(made.waiting.iter().copied());
                    }
                    applog::event("slides", &format!("made {count} slides from {name}"));
                    self.add_song(made.presentation, false);
                    Ok(made.said.unwrap_or_else(|| {
                        crate::i18n::tn(
                            "notices.deck.made",
                            count as u64,
                            "Made {n} slide from {name}.",
                            "Made {n} slides from {name}.",
                            &[("name", &name)],
                        )
                    }))
                }
                Err(problem) => {
                    applog::event("slides", &format!("could not make slides: {problem}"));
                    Err(problem)
                }
            };
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.outcome(said);
                running.home_window.request_redraw();
            }
        }
        for filled in self.decks.filled() {
            match filled {
                deck::Fill::Page { slide, file } => {
                    sp_render::forget_picture(&file);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.pages_waiting.remove(&slide);
                        running.thumbs.forget(slide, &mut running.egui_renderer);
                        running.home_window.request_redraw();
                    }
                }
                deck::Fill::Done { name, unfilled, .. } => {
                    let failed = unfilled.len();
                    applog::event("slides", &format!("drew the pictures of {name} ({failed} could not be drawn)"));
                    if let Some(running) = self.running.as_mut() {
                        for slide in &unfilled {
                            // Not coming: the strip stops saying so.
                            running.home_state.pages_waiting.remove(slide);
                        }
                        if failed > 0 {
                            running.home_state.notices.trouble(crate::i18n::tn(
                                "notices.deck.not_drawn",
                                failed as u64,
                                "{n} picture of {name} could not be drawn.",
                                "{n} pictures of {name} could not be drawn.",
                                &[("name", &name)],
                            ));
                        }
                        running.home_window.request_redraw();
                    }
                }
            }
        }
    }

    /// How a presentation file comes in: as the person said, else as its kind was last brought
    /// in, else as slides where the file can be read directly. A file that can't be read
    /// directly only comes in as pictures.
    fn deck_mode(&self, path: &std::path::Path, asked: Option<deck::ImportAs>) -> deck::ImportAs {
        if !deck::reads_as_slides(path) {
            return deck::ImportAs::Pictures;
        }
        asked.unwrap_or_else(|| {
            self.running
                .as_ref()
                .map_or(deck::ImportAs::Slides, |running| running.home_state.import_choice(deck::kind_of(path)))
        })
    }

    /// Imports files that make slides (from the menu or a drop). PDFs and folders of pictures
    /// come in at once; each presentation file is asked about in turn, by the sheet that says
    /// how it can come in: as slides, or as pictures.
    fn import_slides_asking(&mut self, files: &[PathBuf]) {
        let (presentations, others): (Vec<PathBuf>, Vec<PathBuf>) =
            files.iter().cloned().partition(|path| deck::is_presentation_file(path));
        if !others.is_empty() {
            self.import_songs(&others);
        }
        self.ask_next_import(presentations);
    }

    /// Opens the sheet for the first of these presentation files, the rest waiting behind it
    /// (behind the one the sheet is already asking about, if there is one).
    fn ask_next_import(&mut self, mut waiting: Vec<PathBuf>) {
        if waiting.is_empty() {
            return;
        }
        let Some(running) = self.running.as_mut() else {
            // No window to ask in: each comes in the way it was last, or as slides.
            self.import_songs(&waiting);
            return;
        };
        if let Some(open) = running.home_state.ask_import.as_mut() {
            open.later.append(&mut waiting);
            return;
        }
        let path = waiting.remove(0);
        running.home_state.ask_import = Some(home::AskImport {
            slides_ok: deck::reads_as_slides(&path),
            pictures_ok: convert::program_for(&path).is_some(),
            kept: running.home_state.kept_import(deck::kind_of(&path)),
            path,
            later: waiting,
        });
        running.home_window.request_redraw();
    }

    /// Carries out the answer to the question of how a presentation is to come in, then asks
    /// about the next file chosen with it.
    fn handle_import_answer(&mut self) {
        let Some(answer) = self.running.as_mut().and_then(|r| r.home_state.import_answer.take()) else {
            return;
        };
        if let Some(how) = answer.how {
            self.import_songs_to(&[answer.path], &SongsTo { deck_as: Some(how), ..SongsTo::default() });
        }
        self.ask_next_import(answer.later);
    }

    /// Carries out what Import › Slides… was answered with.
    fn handle_slides_choice(&mut self) {
        match self.running.as_mut().and_then(|r| r.home_state.slides_choice.take()) {
            Some(home::SlidesChoice::Pictures) => self.choose_pictures_to_import(),
            Some(home::SlidesChoice::Presentation) => self.choose_presentations_to_import(),
            None => {}
        }
    }

    /// Asks for pictures to make slides of: the pictures chosen together become one presentation.
    fn choose_pictures_to_import(&mut self) {
        let window = dialogs::Files::new(
            dialogs::Choose::Files,
            crate::i18n::t("notices.dialog.import_pictures", "Import Pictures as Slides"),
        )
        .filter(crate::i18n::t("notices.dialog.filter.pictures", "Pictures"), &media::PICTURES);
        self.ask(dialogs::For::ImportPictures, dialogs::Window::Files(window));
    }

    /// Asks for presentation files and PDFs: a PDF becomes a presentation at once, and each
    /// presentation file is asked about.
    fn choose_presentations_to_import(&mut self) {
        let mut endings = deck::DECK_ENDINGS.to_vec();
        endings.push(deck::PDF_ENDING);
        let window = dialogs::Files::new(
            dialogs::Choose::Files,
            crate::i18n::t("notices.dialog.import_presentation", "Import a Presentation"),
        )
        .filter(crate::i18n::t("notices.dialog.filter.presentations", "Presentations and PDFs"), &endings);
        self.ask(dialogs::For::ImportPresentations, dialogs::Window::Files(window));
    }

    /// Import Songs › Song Files…: asks which song files to import (songs saved by SoftPresenter,
    /// song documents, ChordPro and lyric files, songbooks), and imports them.
    fn choose_songs_to_import(&mut self, setup: bool) {
        self.choose_to_import(
            crate::i18n::t("notices.dialog.import_song_files", "Import Song Files"),
            crate::i18n::t("notices.dialog.filter.song_files", "Song files"),
            &load::song_file_endings(),
            setup,
        );
    }

    /// Import Songs › A Song Bundle…: asks for song bundles and song packs (songs with the
    /// pictures and videos they show), and imports them.
    fn choose_bundles_to_import(&mut self) {
        self.choose_to_import(
            crate::i18n::t("notices.dialog.import_bundle", "Import a Song Bundle"),
            crate::i18n::t("notices.dialog.filter.bundles", "Song bundles and song packs"),
            &load::bundle_endings(),
            false,
        );
    }

    /// Import Songs › Words from Text…: asks for text, rich-text and Word files, and makes a song
    /// of each one's words.
    fn choose_words_to_import(&mut self) {
        self.choose_to_import(
            crate::i18n::t("notices.dialog.import_words", "Import Words from Text"),
            crate::i18n::t("notices.dialog.filter.words", "Text, rich-text and Word files"),
            &load::words_endings(),
            false,
        );
    }

    /// Asks for files with these endings, under a window title and the kind of file the filter
    /// names (with the endings after it), and imports them; `setup` when the first-start setup
    /// asked.
    fn choose_to_import(&mut self, title: &str, kind: &str, endings: &[&str], setup: bool) {
        let window =
            dialogs::Files::new(dialogs::Choose::Files, title).filter(&load::filter_name(kind, endings), endings);
        self.ask(dialogs::For::ImportSongs { setup }, dialogs::Window::Files(window));
    }

    /// Imports the song files chosen; for the first-start setup, its page then says how it went,
    /// as the notices do.
    fn import_songs_chosen(&mut self, paths: &[PathBuf], setup: bool) {
        let mark = self.running.as_ref().map_or(0, |r| r.home_state.notices.mark());
        self.import_songs(paths);
        if setup && let Some(running) = self.running.as_mut() {
            let said = running.home_state.notices.said_since(mark).map(str::to_owned);
            if let Some(setup) = running.home_state.setup.as_mut() {
                setup.songs_said = said;
            }
            running.home_window.request_redraw();
        }
    }

    /// Import Songs › A Library Folder…: asks for a library folder or the folder of libraries,
    /// or for song documents and bundles, and imports them, each folder becoming a library.
    /// Opens at the usual place for such libraries, when that folder is there. A Mac's file
    /// window takes song documents and folders at once; elsewhere a file window takes one kind
    /// or the other, so first a question says which (as the song pack's question does).
    fn choose_library_folder_to_import(&mut self) {
        let documents = [load::PRO_ENDING, load::PRO_BUNDLE_ENDINGS[0], load::PRO_BUNDLE_ENDINGS[1]];
        #[cfg(target_os = "macos")]
        let choose = dialogs::Choose::FilesOrFolders;
        #[cfg(not(target_os = "macos"))]
        let choose = dialogs::Choose::Files;
        let mut window =
            dialogs::Files::new(choose, crate::i18n::t("notices.dialog.import_folder", "Import a Library Folder"))
                .filter(
                    &load::filter_name(
                        crate::i18n::t("notices.dialog.filter.documents", "Song documents and bundles"),
                        &documents,
                    ),
                    &documents,
                );
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let libraries = PathBuf::from(home).join("Documents").join("ProPresenter").join("Libraries");
            if libraries.is_dir() {
                window = window.directory(libraries);
            }
        }
        #[cfg(target_os = "macos")]
        self.ask(dialogs::For::ImportSongs { setup: false }, dialogs::Window::Files(window));
        #[cfg(not(target_os = "macos"))]
        {
            let question = dialogs::Question {
                title: crate::i18n::t("notices.dialog.import_folder", "Import a Library Folder").to_owned(),
                words: crate::i18n::t(
                    "notices.dialog.folder_or_files",
                    "Import a folder of song documents (each folder in it becomes a library), or song documents and bundles themselves?",
                )
                .to_owned(),
                buttons: [
                    crate::i18n::t("notices.dialog.a_folder", "A Folder\u{2026}").to_owned(),
                    crate::i18n::t("notices.dialog.files", "Files\u{2026}").to_owned(),
                    crate::i18n::t("notices.dialog.cancel", "Cancel").to_owned(),
                ],
            };
            self.ask(dialogs::For::LibraryFilesOrFolder(window), dialogs::Window::Question(question));
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

    /// Adds a song to the show with a file of its own in the Songs folder, handed to the
    /// library's writer at once (so it is on disk before a library or playlist naming it), and
    /// returns its place and the key the song list knows it by.
    fn store_song(&mut self, song: sp_model::Presentation) -> (usize, String) {
        let (id, title) = (song.id, song.title.clone());
        // New songs live in the app's Songs folder, each in a file named by its id.
        let path = song_ids::library_path(id);
        let index = self.show.add_presentation(Arc::new(song));
        self.sources.push(path.clone());
        if let Some(path) = path {
            self.write_song(index, path, true, None);
        }
        let key = song_ids::library_key(id, &title);
        if let Some(running) = self.running.as_mut() {
            running.home_state.song_keys.push(key.clone());
        }
        (index, key)
    }

    /// True for a presentation the app makes itself (a Bible passage, the VoxLive item, a song of
    /// the song catalog being looked at): shown, never saved, edited, read again or put in a pack.
    pub(crate) fn app_made(&self, presentation: usize) -> bool {
        self.bible_slots.contains(&presentation)
            || presentation == self.voxlive_slot
            || self.catalog_slots.contains(&presentation)
    }

    /// The end of a song's mark for the slide on screen: how far down the slide it may reach
    /// (`end_mark::floor`: the copyright line's top, on the slides that show one), or `None`
    /// when no mark is due (`end_mark::due`). What the screens are told with the slide
    /// (`Frame::end_mark`) and what the Program preview draws.
    fn end_mark_now(&self) -> Option<f32> {
        let state = &self.running.as_ref()?.home_state;
        // Off, as it is for most: nothing more is looked at.
        if !state.end_mark {
            return None;
        }
        let at = self.show.live()?;
        let song = self.show.presentations().get(at.presentation)?;
        let (app_made, hidden) = (self.app_made(at.presentation), self.show.words_hidden());
        if app_made || hidden || song.loops {
            return None;
        }
        let last = self.last_sung(at.presentation);
        let due = end_mark::due(true, app_made, hidden, song, last, at.slide, self.show.step());
        let copyright = || self.copyright_line_now(state);
        due.then(|| end_mark::floor(copyright().is_some()))
    }

    /// The copyright line of the song on screen, on the slides Settings says ([`copyright_line`]).
    fn copyright_line_now(&self, state: &HomeState) -> Option<String> {
        let (when, licence) = (state.copyright_show, state.ccli_licence.trim());
        copyright_line(&self.show, |at| self.app_made(at), when, licence, |at| self.last_sung(at))
    }

    /// A presentation's last slide as sung (`end_mark::last_sung`), worked out once for the
    /// songs as they stand: any change to a song makes them another version.
    fn last_sung(&self, presentation: usize) -> Option<end_mark::Last> {
        let songs = self.show.version();
        if let Some((have, of, last)) = self.last_sung.get()
            && (have, of) == (songs, presentation)
        {
            return last;
        }
        let last = self.show.presentations().get(presentation).and_then(|song| end_mark::last_sung(song));
        self.last_sung.set(Some((songs, presentation, last)));
        last
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
        // Remote pages' slide pictures have new addresses now; the old ones are let go.
        if let Some(shared) = self.remote.shared() {
            shared.forget_pictures();
        }
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            let state = &mut running.home_state;
            state.list_themes(&self.themes);
            state.theme_for_songs = self.songs_theme;
            state.theme_for_bible = self.bible_theme;
            state.theme_layouts = layouts;
            state.template_names = self.themes.iter().map(template_names).collect();
            // A version no theme has any more is gone: the screens go back to the usual look,
            // and so do the playlist items set to it.
            let versions = sp_model::all_versions(&self.themes);
            if versions != state.versions {
                let have = |id: Uuid| versions.iter().any(|version| version.id == id);
                if state.version_switch.forget_unless(have) {
                    applog::event("version", "the usual look: the version on is in no theme now");
                }
                versions::forget_in_playlists(&mut state.services, have);
                state.versions = versions;
            }
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
                self.remember_themes_in_use();
                // Bible slides now use this theme's Bible template: the translations are pointed
                // at its boxes, as a fresh start.
                if let Some(running) = self.running.as_mut() {
                    running.home_state.bible.assign_boxes();
                }
                if !editing {
                    self.refresh_output();
                }
            }
            // A theme's look for the end of a song's mark, from its window (opened in Settings or
            // in the Theme Editor, whose copy of the theme takes it too, so that a later change
            // there doesn't put the old look back).
            Some(ThemeRequest::EndLook(id, look)) => {
                let edit = running.home_state.theme_edit.as_mut();
                if let Some(index) = end_look::give(&mut self.themes, edit, id, look) {
                    if !self.themes_unsaved.contains(&index) {
                        self.themes_unsaved.push(index);
                    }
                    // The theme is written once the clicks pause for a second (as a theme
                    // changed in its editor is), and on quitting: not at every click of A+,
                    // on the window's thread, with every song that waits to be saved.
                    self.unsaved_since = Some(Instant::now());
                    self.themes_changed();
                    // A last slide on screen takes the look at once (not while the Theme Editor
                    // is open: what is changed there reaches the screens with the next slide,
                    // or when the editor is left).
                    if !editing {
                        self.refresh_output();
                    }
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
                // Named in the language set, after the name the window shows (a copy of the
                // built-in theme is "Incluido copia" in Spanish), and kept.
                let original = theme_store::shown_theme_name(&theme).into_owned();
                theme.id = Uuid::new_v4();
                theme.name = crate::i18n::tgf("notices.copy_name", "{name} copy", &[("name", &original)]);
                self.themes.push(theme);
                self.themes_unsaved.push(self.themes.len() - 1);
                self.unsaved_since = Some(Instant::now());
                self.themes_changed();
                self.open_theme_editor(self.themes.len() - 1);
            }
            Some(ThemeRequest::New) => {
                let mut theme = (*self.theme).clone();
                theme.id = Uuid::new_v4();
                theme.name = crate::i18n::tg("notices.theme.new_name", "New Theme").to_owned();
                self.themes.push(theme);
                self.themes_unsaved.push(self.themes.len() - 1);
                self.unsaved_since = Some(Instant::now());
                self.themes_changed();
                self.open_theme_editor(self.themes.len() - 1);
            }
            Some(ThemeRequest::Edit(index)) if index < count => self.open_theme_editor(index),
            // Dragged to another place in the list (the built-in theme stays first): the themes
            // are written first, so the places in the list still mean what they did, and the
            // order is kept for the next start.
            Some(ThemeRequest::Move(from, to)) if from > 0 && to > 0 && from < count && to < count && from != to => {
                self.save_unsaved();
                let theme = self.themes.remove(from);
                self.themes.insert(to, theme);
                self.songs_theme = home::place_after_move(self.songs_theme, from, to);
                if let Some(edit) = self.running.as_mut().and_then(|r| r.home_state.theme_edit.as_mut()) {
                    edit.index = home::place_after_move(edit.index, from, to);
                }
                self.themes_changed();
                self.remember_themes_in_use();
            }
            // The built-in theme, at the front of the list, is always kept.
            Some(ThemeRequest::Delete(index)) if index > 0 && index < count => {
                // Write what is waiting first, so the places in the list still mean what they did.
                self.save_unsaved();
                let gone = self.themes.remove(index);
                // Its file goes after its last save is written, never before.
                self.writer.then(move || theme_store::delete(&gone));
                // Whatever used it falls back to the built-in theme; later themes move up one.
                let moved = |at: usize| match at.cmp(&index) {
                    std::cmp::Ordering::Less => at,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => at - 1,
                };
                let main_gone = index == self.songs_theme;
                self.songs_theme = moved(self.songs_theme);
                // A question about the theme deleted goes; one about a theme after it follows it.
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.settle(outside::Waiting::Theme(index));
                    running.home_state.notices.renumber(|about| match about {
                        outside::Waiting::Theme(at) => outside::Waiting::Theme(moved(at)),
                        song => song,
                    });
                }
                self.themes_changed();
                self.remember_themes_in_use();
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
            }
            _ => {}
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // Asked to leave, or its draft gone without that (`HomeState::theme_editor_is_left`).
        let leave = running.home_state.theme_editor_is_left();
        let Some(edit) = running.home_state.theme_edit.as_mut() else {
            return;
        };
        let (changed, index) = (std::mem::take(&mut edit.changed), edit.index);
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
            let left = self.running.as_mut().and_then(|running| running.home_state.leave_theme_editor());
            // The theme is written either way. Left by its own way out or for another view, it
            // is finished with, and the audience sees it. With its draft put away to show
            // something else (Go to the Slide on Screen, a passage from `sp`), the slide that
            // is up is not drawn again with it, which would play its transition again in front
            // of the room: the theme reaches the screens with the next slide.
            self.save_unsaved();
            if left == Some(home::EditorLeft::Asked) {
                self.refresh_output();
            }
        }
    }

    /// Opens the Theme Editor on the theme at a place in the list.
    fn open_theme_editor(&mut self, index: usize) {
        let (Some(theme), Some(running)) = (self.themes.get(index), self.running.as_mut()) else {
            return;
        };
        let edit = home::ThemeEdit::of(index, theme.clone());
        running.home_state.editing = edit.theme.templates.first().map(|first| EditDraft::of_template(first, 0));
        running.home_state.theme_edit = Some(edit);
        running.home_state.bible_open = false;
        running.canvas_for = None;
        running.home_window.request_redraw();
    }

    /// Looks at the displays when it is time (see `DISPLAYS_LOOK_EVERY`), and says when it will
    /// be time again.
    fn look_at_displays(&mut self, el: &ActiveEventLoop) -> Option<Instant> {
        let due = self.running.as_ref()?.displays_look_at;
        if due > Instant::now() {
            return Some(due);
        }
        if let Some(running) = self.running.as_mut() {
            running.displays_look_at = Instant::now() + DISPLAYS_LOOK_EVERY;
            // Asked afresh: a display's size or the main display can change with the same list.
            running.displays_seen = None;
        }
        self.sync_screens(el);
        self.running.as_ref().map(|running| running.displays_look_at)
    }

    /// True for a run that only saves a picture, or is asked to stay out of the way.
    fn quiet(&self) -> bool {
        self.options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some()
    }

    /// Testing aid: SOFTPRESENTER_CLOSE_BUTTON_PLAY=1 plays the pointer over the first screen's
    /// window that has the X, through the same steps as the window's own events: moving across
    /// it for 1.2 seconds, resting on its X for 0.6, then pressing it, once (`=move` only moves,
    /// and leaves the X to go by itself). For a quiet run with
    /// SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS=1, where nobody moves the pointer there. When to play
    /// the next step.
    fn play_pointer(&mut self) -> Option<Instant> {
        static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        // Testing aid: SOFTPRESENTER_CLOSE_BUTTON_PLAY (see above).
        let asked = std::env::var_os("SOFTPRESENTER_CLOSE_BUTTON_PLAY")?;
        if DONE.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let running = self.running.as_mut()?;
        let have = running.screen_windows.iter_mut().find(|have| have.has_close_button())?;
        let now = Instant::now();
        let gone = now.saturating_duration_since(*STARTED.get_or_init(|| now)).as_secs_f64();
        let (size, scale) = (have.window.inner_size(), have.window.scale_factor());
        let square = close_button::square(size.width, scale);
        let on_x = f64::from(square.x + square.side / 2.0);
        let at = match gone {
            ..1.2 => (f64::from(size.width) * (0.1 + 0.4 * gone / 1.2), f64::from(size.height) / 2.0),
            _ if asked == "move" => {
                DONE.store(true, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
            ..1.8 => (on_x, f64::from(square.y + square.side / 2.0)),
            _ => {
                DONE.store(true, std::sync::atomic::Ordering::Relaxed);
                let press = have.close.pressed(now, close_button::SHOWS_ON_PRESS);
                applog::event("screens", &format!("{}: testing aid: the played pointer pressed: {press:?}", have.name));
                if press == close_button::Press::SwitchOff {
                    let screen = have.screen;
                    switch_off_by_its_x(running, screen);
                }
                return None;
            }
        };
        have.close.moved(now, at, size.width, scale);
        Some(now + close_button::STEP)
    }

    /// The X on each screen's window that fills a display (see `close_button`): lets it go once
    /// it has faded, tells the screens' thread how to draw it whenever that changes (nothing
    /// while it only stays), and shows the pointer over the window while it shows. When to look
    /// again: a step later while one comes or goes, when one starts to go.
    fn tick_close_buttons(&mut self) -> Option<Instant> {
        let played = self.play_pointer();
        let Running { screen_windows, output, .. } = self.running.as_mut()?;
        let now = Instant::now();
        let mut next: Option<Instant> = played;
        for have in screen_windows.iter_mut() {
            // A window hidden, or no longer filling a display, has no X.
            if !have.has_close_button() {
                have.close = close_button::CloseButton::default();
            }
            have.close.settle(now);
            let drawn = have.close.drawn(now, have.window.scale_factor());
            if drawn != have.close_sent {
                if let Some(output) = output.as_ref() {
                    output.send(OutputMsg::CloseButton { id: have.screen, drawn });
                }
                // The pointer shows over the window while the X does; a window that fills a
                // display hides it otherwise.
                match (have.close_sent.is_some(), drawn.is_some()) {
                    (false, true) => {
                        have.window.set_cursor_visible(true);
                        applog::event("screens", &format!("{}: the close button shows on its window", have.name));
                    }
                    (true, false) => {
                        have.window.set_cursor_visible(!matches!(have.placed, Placed::Fullscreen(_)));
                        applog::event("screens", &format!("{}: the close button has gone", have.name));
                    }
                    _ => {}
                }
                have.close_sent = drawn;
            }
            next = [next, have.close.next_change(now)].into_iter().flatten().min();
        }
        next
    }

    /// Brings the screens' windows and NDI sources in line with the list of screens and the
    /// displays connected now: opens, moves and closes windows, starts, renames and stops NDI,
    /// and says what each screen is doing. Cheap when nothing has changed, so it runs every
    /// frame and notices a display being plugged in or out.
    fn sync_screens(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let quitting = self.quitting.is_some();
        let audience_on = self.show.output_enabled();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // NDI downloaded or removed in the app (Linux): looked for again at once, and the
        // screens that found no NDI are started again (below).
        running.home_state.ndi_download.poll();
        if running.home_state.ndi_download.take_changed() {
            running.ndi_checked = None;
        }
        // Whether NDI is installed, looked at again every few seconds (a few file checks), so
        // installing it while the app runs is noticed.
        if running.ndi_checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(3)) {
            let found = sp_ndi::installed();
            if running.ndi_checked.is_none() || found != running.home_state.ndi_available {
                applog::event("ndi", if found { "the NDI runtime was found" } else { "the NDI runtime was not found" });
            }
            // (A made-up state shown for a picture says for itself whether NDI is here.)
            let found = running.home_state.ndi_download.demo_found().unwrap_or(found);
            running.home_state.ndi_available = found;
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
            // What was seen a moment ago was used: it is looked at again once it is a second
            // old, so a change seen in the meantime (a display's size, the window moved to
            // another display) is never left until something else happens. That look is the
            // loop's (`look_at_displays`), which draws nothing unless it finds a difference:
            // drawing the window again for it drew a window that shows the clock twice a second.
            let due = seen.at + DisplaysSeen::KEPT_FOR;
            running.displays_look_at = running.displays_look_at.min(due);
        }
        // Automatic screens given a display, and old display names made new, saved so.
        let mut differs = false;
        if screens::settle(&mut running.home_state.screens, &displays, &operator) {
            running.home_state.screens_changed = true;
            differs = true;
        }
        if running.home_state.operator_displays != operator {
            running.home_state.operator_displays.clone_from(&operator);
            differs = true;
        }
        if displays != running.home_state.displays {
            differs = true;
            // Said in the log: a display that goes or comes back, or changes size, can hold up the
            // screens' windows on it (see `output/held.rs`).
            let listed: Vec<String> = displays
                .iter()
                .map(|display| format!("{} {}x{} at {},{}", display.name, display.w, display.h, display.x, display.y))
                .collect();
            applog::event("screens", &format!("Displays now: {}", listed.join("; ")));
            running.home_state.displays = displays.clone();
        }
        // The window shows what was found (the screens' displays, the operator's): a look the
        // loop made, with no drawing, draws it once it differs.
        if differs {
            running.home_window.request_redraw();
        }
        if std::mem::take(&mut running.home_state.screens_changed) && !quiet {
            let save = screens::to_save(&running.home_state.screens);
            save_setting(&mut self.paced, &self.writer, &mut running.home_state.notices, save);
        }
        let list = running.home_state.screens.clone();
        let mut placed = screens::place_all(&list, &displays, &operator);
        // Which screens show or send, as a person's copy places them (worked out before a quiet
        // run moves them into windows), for keeping the display awake while one is on.
        let decklink_ready = matches!(running.home_state.decklink.driver, sp_decklink::Driver::Ready(_));
        let would_show: Vec<bool> = placed.iter().map(|place| keep_awake::shows(place, decklink_ready)).collect();
        // A capture never fills a display or sends over the network, whatever the list says.
        if quiet {
            // Testing aid: SOFTPRESENTER_PRETEND_NDI's sources send nothing anywhere.
            let ndi_aid = std::env::var_os("SOFTPRESENTER_NDI").is_some() || sp_ndi::pretending();
            for place in &mut placed {
                match place {
                    // Testing aid: SOFTPRESENTER_SCREENS_FILL=<display name> lets a quiet run's
                    // screens fill that one display (a virtual one made for the test, never a
                    // display anyone looks at), through the same window code as a real run. The
                    // display goes by either of its names, as a saved screen's does.
                    Placed::Fullscreen(index)
                        if std::env::var("SOFTPRESENTER_SCREENS_FILL").is_ok_and(|name| {
                            displays.iter().any(|d| d.index == *index && d.is_named(&name) && !d.main)
                        }) => {}
                    Placed::Fullscreen(_) => *place = Placed::Window,
                    Placed::Ndi(_) if !ndi_aid => {
                        let why = i18n::t("screens.status.quiet_ndi", "Not sent in a quiet run (a test or a capture).");
                        *place = Placed::Nothing(why.to_owned());
                    }
                    // Never a real card: it may be on a video chain in front of people.
                    // Pretend devices (a testing switch) play into nothing, so they go on.
                    Placed::DeckLink(_) if !sp_decklink::pretending() => {
                        let why = i18n::t(
                            "screens.status.quiet_decklink",
                            "Not played out in a quiet run (a test or a capture).",
                        );
                        *place = Placed::Nothing(why.to_owned());
                    }
                    _ => {}
                }
            }
        }
        // A kind of screen switched off closes its windows, so the display shows what is behind
        // them rather than black. NDI goes on sending (black), so a stream keeps its source. A
        // screen switched off by its own switch is treated the same way (its window kept hidden
        // to show again, and no notice when its display is away).
        let on_flags: Vec<bool> = list
            .iter()
            .map(|screen| {
                screen.on
                    && match screen.kind {
                        screens::Kind::Audience => audience_on,
                        screens::Kind::Stage => running.home_state.stage_on,
                    }
            })
            .collect();
        for ((_, place), on) in list.iter().zip(placed.iter_mut()).zip(&on_flags) {
            if !*on && matches!(place, Placed::Fullscreen(_) | Placed::Window) {
                *place = Placed::Nothing(i18n::t("screens.status.switched_off", "Switched off").to_owned());
            }
        }
        // The display is kept awake while a screen is on: asked of the system only when that
        // changes (the call is quick, and never made on the screens' thread). Never again once
        // the quit has begun, though the screens are still brought in line while it waits.
        let screens_on = !quitting && running.output.is_some() && keep_awake::any_on(&would_show, &on_flags);
        if self.keep_awake.set(screens_on, Instant::now()).is_some() {
            running.home_state.notices.trouble(keep_awake::notice_words());
        }
        // Without the driver, or while it is still looked for, a DeckLink screen plays nowhere.
        let driver = &running.home_state.decklink.driver;
        if !matches!(driver, sp_decklink::Driver::Ready(_)) {
            for place in &mut placed {
                if matches!(place, Placed::DeckLink(_)) {
                    *place = Placed::Nothing(screens::decklink_driver_words(driver));
                }
            }
        }
        running.retired.retain(|(_, since)| since.elapsed() < Duration::from_secs(2));
        // Which screens' displays are away (said once, and taken back when they return).
        let now_away = window_plan::away(&list, &on_flags, &displays);
        let (said, over) = window_plan::notice_changes(&running.away, &now_away);
        for text in said {
            applog::event("screens", &text);
            running.home_state.notices.lasting_trouble(text);
        }
        for text in over {
            running.home_state.notices.withdraw(&text);
        }
        running.away = now_away;
        let Running { output, screen_windows, retired, context, device_id, home_state, .. } = running;
        let Some(output) = output.as_ref() else { return };

        // Windows: what to do with each (see `window_plan`), then do it. A window that fills a
        // display is hidden at once when its display goes, and put back when it returns.
        let mut old = std::mem::take(screen_windows);
        let seen: Vec<window_plan::Seen> = old
            .iter()
            .map(|have| window_plan::Seen {
                screen: have.screen,
                shown: !have.hidden,
                on: have.window.current_monitor().map(|monitor| monitor_rect(&monitor)),
                put_on: have.put_on,
                tries: have.tries,
                settled: have.placed_at.elapsed() >= Duration::from_secs(2),
            })
            .collect();
        let steps = window_plan::plan(&list, &placed, &on_flags, &displays, &seen);
        let above = !quiet || std::env::var_os("SOFTPRESENTER_SCREENS_ON_TOP").is_some();
        let mut opened = false;
        for ((screen, place), step) in list.iter().zip(&placed).zip(&steps) {
            let existing = old.iter().position(|have| have.screen == screen.id);
            match step {
                window_plan::Step::Idle => {
                    // A window hidden for a while is kept (on a Mac) to show again; otherwise
                    // it goes with the rest below.
                    if KEEP_HIDDEN && let Some(at) = existing {
                        screen_windows.push(old.remove(at));
                    }
                    continue;
                }
                window_plan::Step::Close => continue,
                window_plan::Step::Hide(why) => {
                    let Some(at) = existing else { continue };
                    let mut kept = old.remove(at);
                    if kept.hidden {
                        // Already hidden (a stray that stays so): nothing more to do.
                        screen_windows.push(kept);
                        continue;
                    }
                    // At once: the picture is let go, and the window is out of the way.
                    output.send(OutputMsg::RemoveSurface(screen.id));
                    kept.window.set_visible(false);
                    applog::event("screens", &format!("{}: hidden, because {}", screen.name, why.in_words()));
                    // A stray is kept hidden on every system, so it is not made again and again.
                    if KEEP_HIDDEN || *why == window_plan::Why::Stray {
                        kept.hidden = true;
                        if *why != window_plan::Why::Stray {
                            kept.put_on = None;
                            kept.tries = 0;
                        }
                        screen_windows.push(kept);
                    } else {
                        retired.push((kept.window, Instant::now()));
                    }
                    continue;
                }
                window_plan::Step::Keep => {
                    let Some(at) = existing else { continue };
                    let mut kept = old.remove(at);
                    let on = kept.window.current_monitor().map(|monitor| monitor_rect(&monitor));
                    if kept.put_on.is_some() && on == kept.put_on && kept.placed_at.elapsed() >= Duration::from_secs(2)
                    {
                        kept.tries = 0;
                    }
                    if kept.name != screen.name {
                        kept.window.set_title(&format!("{} — SoftPresenter", screen.name));
                        kept.name = screen.name.clone();
                    }
                    screen_windows.push(kept);
                    continue;
                }
                window_plan::Step::Other => {
                    if !matches!(place, Placed::Window) {
                        continue;
                    }
                    if let Some(at) = existing {
                        let mut kept = old.remove(at);
                        if !kept.cover {
                            if kept.placed != *place {
                                kept.window.set_fullscreen(None);
                                log_placement(&screen.name, place, &kept.window, &displays);
                                kept.placed_at = Instant::now();
                            }
                            kept.placed = place.clone();
                            if kept.name != screen.name {
                                kept.window.set_title(&format!("{} — SoftPresenter", screen.name));
                                kept.name = screen.name.clone();
                            }
                            screen_windows.push(kept);
                            continue;
                        }
                        // A window that covered a display is not made an ordinary window again.
                        output.send(OutputMsg::RemoveSurface(screen.id));
                        kept.window.set_visible(false);
                        retired.push((kept.window, Instant::now()));
                    }
                }
                window_plan::Step::Place(rect) | window_plan::Step::Open(rect) => {
                    let Placed::Fullscreen(index) = place else { continue };
                    let Some(monitor) = monitors.get(*index) else { continue };
                    if let Some(at) = existing {
                        let mut kept = old.remove(at);
                        if !cfg!(target_os = "macos") || kept.cover {
                            let was_hidden = kept.hidden;
                            let again = kept.put_on == Some(*rect);
                            if let Err(problem) = put_on_display(&kept.window, monitor, above, false) {
                                applog::event("screens", &format!("{}: {problem}", screen.name));
                                output.send(OutputMsg::RemoveSurface(screen.id));
                                kept.window.set_visible(false);
                                retired.push((kept.window, Instant::now()));
                                continue;
                            }
                            kept.hidden = false;
                            kept.put_on = Some(*rect);
                            kept.tries = if again { kept.tries.saturating_add(1) } else { 0 };
                            kept.placed = place.clone();
                            kept.placed_at = Instant::now();
                            kept.window.set_cursor_visible(false);
                            log_placement(&screen.name, place, &kept.window, &displays);
                            if was_hidden {
                                applog::event("screens", &format!("{}: shown again on its display", screen.name));
                                opened |= add_surface(
                                    output,
                                    context,
                                    *device_id,
                                    screen,
                                    &kept.window,
                                    &mut home_state.notices,
                                );
                            }
                            screen_windows.push(kept);
                            continue;
                        }
                        // An ordinary window is made again as one that covers the display.
                        output.send(OutputMsg::RemoveSurface(screen.id));
                        kept.window.set_visible(false);
                        retired.push((kept.window, Instant::now()));
                    }
                }
            }
            // Open a window: for a screen with none, or one that couldn't be used again.
            let attrs = app_window(Window::default_attributes()).with_title(format!("{} — SoftPresenter", screen.name));
            let covering = matches!(place, Placed::Fullscreen(_));
            let attrs = match place {
                // Opened on the display first, as large as it, and put to fill it once it is
                // there (below): a window made full screen as it opens fills the main display on
                // macOS, whatever display is asked for, which put the audience on the operator's
                // monitor.
                Placed::Fullscreen(index) => match monitors.get(*index) {
                    Some(monitor) => {
                        media::note_screen(monitor.size().width, monitor.size().height);
                        attrs.with_position(monitor.position()).with_inner_size(monitor.size())
                    }
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
            // On macOS a window that covers a display is made without a frame and unseen, put on
            // its display's frame, and only then shown: it is never seen anywhere else.
            let attrs = if covering && cfg!(target_os = "macos") {
                attrs.with_decorations(false).with_visible(false)
            } else {
                attrs
            };
            let attrs = if std::env::var_os("SOFTPRESENTER_SCREENS_ON_TOP").is_some() {
                // Testing aid: SOFTPRESENTER_SCREENS_ON_TOP keeps the screens' windows above
                // every other window, so a timing run on one display draws them for real (a
                // window covered by the operator's is not drawn, and is paced differently). In a
                // quiet run too, where they still never take the keyboard.
                attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            } else if quiet {
                attrs.with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
            } else {
                attrs
            };
            let attrs = if quiet { attrs.with_active(false) } else { attrs };
            let window = match el.create_window(attrs) {
                Ok(window) => Arc::new(window),
                Err(e) => {
                    home_state.notices.lasting_trouble(i18n::tf(
                        "notices.screens.no_window",
                        "Could not open a window for \"{name}\": {error}",
                        &[("name", &screen.name), ("error", &e.to_string())],
                    ));
                    continue;
                }
            };
            let mut put_on = None;
            if let (Placed::Fullscreen(index), window_plan::Step::Place(rect) | window_plan::Step::Open(rect)) =
                (place, step)
                && let Some(monitor) = monitors.get(*index)
            {
                if let Err(problem) = put_on_display(&window, monitor, above, true) {
                    window.set_visible(false);
                    home_state.notices.lasting_trouble(i18n::tf(
                        "notices.screens.not_placed",
                        "Could not put \"{name}\" on its display: {problem}",
                        &[("name", &screen.name), ("problem", &problem)],
                    ));
                    continue;
                }
                put_on = Some(*rect);
            }
            log_placement(&screen.name, place, &window, &displays);
            window.set_cursor_visible(!matches!(place, Placed::Fullscreen(_)));
            if !add_surface(output, context, *device_id, screen, &window, &mut home_state.notices) {
                window.set_visible(false);
                continue;
            }
            screen_windows.push(ScreenWindow {
                screen: screen.id,
                window,
                placed: place.clone(),
                name: screen.name.clone(),
                placed_at: Instant::now(),
                hidden: false,
                put_on,
                tries: 0,
                cover: covering && cfg!(target_os = "macos"),
                close: close_button::CloseButton::default(),
                close_sent: None,
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
            ndi_silent_sent,
            dark_sent,
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

        // NDI: start, rename and stop sources. A source that found no NDI starts again once
        // NDI has been downloaded in the app.
        if home_state.ndi_download.take_restart() {
            ndi_sent.retain(|(id, ..)| {
                !matches!(output.ndi_status(*id), output::NdiStatus::Failed(reason) if reason == sp_ndi::NOT_INSTALLED)
            });
        }
        let wanted: Vec<(Uuid, String, screens::NdiRate)> = list
            .iter()
            .zip(&placed)
            .filter_map(|(screen, place)| match place {
                Placed::Ndi(name) => Some((screen.id, name.clone(), screen.rate)),
                _ => None,
            })
            .collect();
        let mut ndi_moved = false;
        // Another name or another rate starts the source again.
        for (id, name, rate) in &wanted {
            if !ndi_sent.contains(&(*id, name.clone(), *rate)) {
                output.send(OutputMsg::SetNdi { id: *id, name: Some(name.clone()), rate: rate.rate() });
                ndi_moved = true;
            }
        }
        for (id, ..) in ndi_sent.iter() {
            if !wanted.iter().any(|(still, ..)| still == id) {
                output.send(OutputMsg::SetNdi { id: *id, name: None, rate: sp_ndi::Rate::default() });
                ndi_moved = true;
            }
        }
        *ndi_sent = wanted;
        // A video opened before an NDI source has started still hands its sound over.
        sp_video::sound::expect(
            list.iter().zip(&placed).any(|(screen, place)| screen.sound && matches!(place, Placed::Ndi(_))),
        );
        // Which NDI sources send the app's sound: all but those whose sound is switched off,
        // and those of screens switched off, which send black and silence.
        let silent: Vec<Uuid> =
            list.iter().filter(|screen| !screen.sound || !screen.on).map(|screen| screen.id).collect();
        if silent != *ndi_silent_sent {
            output.send(OutputMsg::NdiSilent(silent.clone()));
            *ndi_silent_sent = silent;
        }
        // Screens switched off that still send (NDI, DeckLink) are drawn black: what they send
        // while their kind is switched off.
        let dark: Vec<Uuid> = list.iter().filter(|screen| screen.sends_while_off()).map(|screen| screen.id).collect();
        if dark != *dark_sent {
            output.send(OutputMsg::Dark(dark.clone()));
            output.send(OutputMsg::Redraw);
            *dark_sent = dark;
        }
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
                    output::NdiStatus::Failed(reason) if reason == sp_ndi::NOT_INSTALLED => (
                        i18n::t("screens.status.ndi_not_installed", "NDI isn't installed on this computer").to_owned(),
                        false,
                    ),
                    // NDI's own words for what went wrong, as it says them.
                    output::NdiStatus::Failed(reason) => (reason, false),
                    output::NdiStatus::Off => {
                        starting = true;
                        (i18n::t("screens.status.ndi_starting_now", "Starting NDI\u{2026}").to_owned(), false)
                    }
                },
                Placed::DeckLink(_) => match output.decklink_status(screen.id) {
                    output::DeckLinkStatus::Playing(what) => (screens::decklink_playing_words(&what), true),
                    // The driver's own words for why it waits, as it says them.
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
                home_state.notices.trouble(i18n::tf(
                    "notices.decklink.trying_again",
                    "{screen}: {doing} Trying again.",
                    &[("screen", &screen.name), ("doing", doing)],
                ));
            }
        }
        *decklink_playing = playing;
        // A screen switched off on NDI or a DeckLink output sends black: it isn't going out.
        // (Said after the DeckLink outputs playing are noted, so switching one off is never
        // taken for an output that stopped.)
        for (screen, (doing, shown)) in list.iter().zip(home_state.screen_status.iter_mut()) {
            if screen.sends_while_off() && *shown {
                *doing = i18n::t("screens.status.off_black", "Off \u{b7} sending black").to_owned();
                *shown = false;
            }
        }
        if home_state.screen_status != status_before {
            for panel in running.panels.iter().filter(|panel| panel.kind == Panel::Screens) {
                panel.window.request_redraw();
            }
        }
        // Starting NDI takes a moment on the output thread; look again shortly.
        if starting || ndi_moved {
            repaints::wake_after(
                &mut running.next_repaint,
                Duration::from_millis(400),
                repaints::Timer::OutputStarting,
            );
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

    /// Testing aid: SOFTPRESENTER_RESIZE_AID=<from>x<to> (window widths in points) resizes the
    /// operator's window in 20 steps, 50 ms apart, from that width to the other and back to the
    /// first, as a drag does, and says on stderr for each step the size asked for, the size the
    /// window has, the size its surface was drawn at in the last frame and how many frames were
    /// drawn at the new size before the next step. A drag is only followed live if each frame is.
    fn resize_testing_aid(&mut self) {
        struct Aid {
            step: u32,
            at: Instant,
            drawn: u32,
            matching: u32,
        }
        static AID: std::sync::Mutex<Option<Aid>> = std::sync::Mutex::new(None);
        let Ok(asked) = std::env::var("SOFTPRESENTER_RESIZE_AID") else { return };
        let Some((from, to)) =
            asked.split_once('x').and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        else {
            return;
        };
        let Some(running) = self.running.as_mut() else { return };
        let frames = running.frames;
        let inner = running.home_window.inner_size();
        let config = running.home_set_up.size();
        let Ok(mut guard) = AID.lock() else { return };
        // Starts after the first few frames, once the window has settled.
        let aid = guard.get_or_insert(Aid { step: 0, at: Instant::now(), drawn: 0, matching: 0 });
        if frames < 10 {
            aid.at = Instant::now();
            running.next_repaint = aid.at.checked_add(Duration::from_millis(50));
            return;
        }
        if aid.step > 0 {
            aid.drawn += 1;
            aid.matching += u32::from(config == (inner.width, inner.height));
        }
        if aid.at.elapsed() < Duration::from_millis(50) {
            // Nothing is asked to be drawn but the next step, so frames counted are only those
            // the resizing itself brought.
            running.next_repaint = aid.at.checked_add(Duration::from_millis(50));
            return;
        }
        if aid.step > 0 {
            eprintln!(
                "resize step {}: window {}x{}, surface {}x{}, {} frames drawn, {} at the window's size",
                aid.step, inner.width, inner.height, config.0, config.1, aid.drawn, aid.matching
            );
        }
        if aid.step == 40 {
            eprintln!("resize aid done");
            // What the library's writer holds is written first: the exit skips its own wait (and
            // the end of `main`, where the run is noted as quit).
            self.paced.flush(&self.writer);
            self.writer.drain();
            applog::last_run::ended();
            finish_replies();
            std::process::exit(0);
        }
        aid.step += 1;
        aid.at = Instant::now();
        aid.drawn = 0;
        aid.matching = 0;
        let fraction = f64::from(if aid.step <= 20 { aid.step } else { 40 - aid.step }) / 20.0;
        let width = from + (to - from) * fraction;
        let size = winit::dpi::LogicalSize::new(width, 700.0);
        let _ = running.home_window.request_inner_size(size);
    }

    /// Sets the home window's interface size from the setting and the window's size in points,
    /// once a new size or display scale has settled (see `interface_size`), and has the window's
    /// pictures of slides drawn at the size its pixels to the point then ask for. Returns how
    /// long to wait before looking again while a new size settles.
    fn follow_interface_size(&mut self) -> Option<Duration> {
        let capturing = self.options.capture_home.is_some();
        let running = self.running.as_mut()?;
        let (size, scale) = (running.home_window.inner_size(), running.home_window.scale_factor());
        // Testing aid: a capture run laid out as if the window were another size (see
        // `interface_size::capture_size`).
        let (points, pixels) = match capturing.then(interface_size::capture_size).flatten() {
            Some(points) => (points, ((points.x * scale as f32) as u32, (points.y * scale as f32) as u32)),
            None => {
                let points = size.to_logical::<f32>(scale);
                (egui::vec2(points.width, points.height), (size.width, size.height))
            }
        };
        let setting = running.home_state.interface_size;
        let seen = interface_size::Seen::home(pixels, scale, setting);
        match running.zoom_watch.due(seen, Instant::now()) {
            interface_size::Due::Done => return None,
            interface_size::Due::After(wait) => return Some(wait),
            interface_size::Due::Now => {}
        }
        running.zoom_watch.applied(seen);
        let zoom = interface_size::home_zoom(setting, points);
        if zoom != running.egui_ctx.zoom_factor() {
            // Set before the frame's input is taken, so this frame is laid out at it (the
            // toolkit's own way to set it takes effect a frame later).
            running.egui_ctx.options_mut(|options| options.zoom_factor = zoom);
            let (w, h) = (points.x, points.y);
            applog::event("window", &format!("interface size {:.0}% for {w:.0}x{h:.0} points", zoom * 100.0));
            // Settings and Screens follow it (and Settings says it).
            for panel in &running.panels {
                panel.window.request_redraw();
            }
        }
        running.home_state.zoom_now = zoom;
        let pictures = interface_size::picture_scale(scale as f32 * zoom);
        if pictures != running.picture_scale {
            self.scale_pictures(pictures);
        }
        None
    }

    /// Draws the window's thumbnails and both previews `scale` times their usual size (see
    /// `interface_size::picture_scale`). The live preview's two textures are made again at the
    /// new size and handed to the output thread, which draws the preview into them at once;
    /// the old ones are shown until it has.
    fn scale_pictures(&mut self, scale: f32) {
        let Some(running) = self.running.as_mut() else { return };
        running.picture_scale = scale;
        running.thumbs.set_scale(scale);
        let (w, h) = interface_size::scaled(output::LIVE_PREVIEW_SIZE, scale);
        if running.live_textures.first().is_some_and(|texture| (texture.width(), texture.height()) == (w, h)) {
            return;
        }
        let textures: Vec<wgpu::Texture> =
            (0..2).map(|_| sp_render::target_texture(&running.gpu.device, w, h)).collect();
        let ids: Vec<egui::TextureId> = textures
            .iter()
            .map(|texture| {
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                running.egui_renderer.register_native_texture(&running.gpu.device, &view, wgpu::FilterMode::Linear)
            })
            .collect();
        let drawn = running.output.as_ref().map_or(0, |output| output.live_preview().1);
        let before = std::mem::replace(&mut running.live_preview_ids, ids);
        if let Some((older, _)) = running.live_preview_before.replace((before, drawn)) {
            for id in older {
                running.egui_renderer.free_texture(&id);
            }
        }
        running.live_textures = textures.clone();
        if let Some(output) = &running.output {
            output.send(OutputMsg::PreviewTargets(textures));
        }
        // Remote Control reads the preview back from the new textures.
        self.remote_preview_source();
    }

    /// Draws one frame of the home window and returns what the operator asked for.
    fn draw_home(&mut self) -> Vec<Command> {
        let _stall = stall::inside(stall::Doing::Drawing);
        let settling = self.follow_interface_size();
        // Undo and Redo are drawn greyed when the history has nothing to take back.
        self.note_show_history(false);
        self.refresh_next_line();
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
            home_set_up,
            home_resize,
            home_resize_held_back,
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
        // Testing aid: SOFTPRESENTER_POINTER_DRAG drags from one point to another (see
        // `home::pointer_drag_events`).
        raw_input.events.extend(home::pointer_drag_events(egui_ctx));
        // Testing aid: SOFTPRESENTER_MACRO_STEPS=drag=N>M drags a macro's step (see
        // `live_tabs::steps_aid_events`).
        raw_input.events.extend(live_tabs::steps_aid_events(egui_ctx));
        // Where the pointer is while files are dragged over the window: asked of the system, as
        // the window library doesn't say (see file_drop.rs).
        if running.home_state.drop_hover {
            running.home_state.file_drag.pointer = file_drop_pointer(home_window, egui_ctx, &running.file_drops);
        }
        // A new size is taken on as it comes, so the window is drawn at the size it has while it
        // is dragged and macOS has nothing to stretch. Setting the surface up waits for what the
        // GPU is doing, the screens' pictures included, so if that ever takes longer than
        // `LIVE_RESIZE_WAIT` the rest of the drag falls back to taking the size on once it has
        // stopped changing for `SETTLE` (the window is stretched until then): the audience must
        // not see the operator resizing a window.
        const SETTLE: Duration = Duration::from_millis(120);
        const LIVE_RESIZE_WAIT: Duration = Duration::from_millis(8);
        // The surface always follows the window. A size the window took without a resize being
        // handled (moved onto the work area as it opened, on a slow computer) left egui drawing
        // for one size onto a surface of another, which the GPU refuses on every frame; the
        // surface is set up again at once, as for a size that has settled. So is one whose last
        // set-up was lost to work the screens' thread handed the GPU at that moment, or that
        // refused its picture: it still has the size before, or none, and nothing is drawn into
        // it until a set-up works. That one is tried again only when its turn has come, a frame
        // later at the soonest and less often the longer it fails (see `surface_setup`).
        let inner = home_window.inner_size();
        let window_size = (inner.width, inner.height);
        if home_resize.is_none() && home_set_up.due(window_size, Instant::now()) {
            *home_resize = Some((inner, Instant::now().checked_sub(SETTLE).unwrap_or_else(Instant::now)));
        }
        if let Some((size, changed)) = *home_resize {
            let settled = changed.elapsed() >= SETTLE;
            let size = (size.width, size.height);
            if !*home_resize_held_back || settled {
                let configuring = Instant::now();
                // Not yet its turn after a failure: kept, and looked at again at the next drawing.
                if !home_set_up.behind(size) || home_set_up.due(size, configuring) {
                    home_set_up.follow(size, configuring, |config| {
                        surface_setup::set_up(&gpu.device, home_surface, config, surface_setup::Of::Operator)
                    });
                    if settled {
                        *home_resize_held_back = false;
                    } else if configuring.elapsed() > LIVE_RESIZE_WAIT {
                        *home_resize_held_back = true;
                    }
                    *home_resize = None;
                }
            } else {
                home_window.request_redraw();
            }
        }
        // While the surface has another size than the window (a drag held back, a set-up still
        // to come, a window larger than a surface may be), the window is laid out for the
        // surface's size, and the system stretches the picture.
        if inner.width > 0 && inner.height > 0 && home_set_up.size() != window_size {
            let points = home_window.scale_factor() as f32 * egui_ctx.zoom_factor();
            let (w, h) = (home_set_up.size().0 as f32 / points, home_set_up.size().1 as f32 / points);
            raw_input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)));
        }
        // Testing aid: a capture run laid out as if the window were another size, drawn off the
        // window at that size (see `interface_size::capture_size`).
        let offscreen = self.options.capture_home.is_some().then(interface_size::capture_size).flatten();
        if let Some(points) = offscreen {
            let laid_out = points / egui_ctx.zoom_factor();
            raw_input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, laid_out));
        }
        let offscreen = offscreen.map(|points| {
            let scale = home_window.scale_factor() as f32;
            [(points.x * scale).round() as u32, (points.y * scale).round() as u32]
        });
        thumbs.begin_frame();
        running.home_state.bible.keep_read_ahead();
        // The output gone (not taken away for quitting) is trouble, said once.
        if running.output.as_ref().is_some_and(OutputHandle::is_finished) {
            running.home_state.output_has_stopped();
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
        // Its textures of the size before (see `scale_pictures`) are shown until a picture has
        // been drawn in the new ones, then let go.
        if let (Some((_, at)), Some((_, drawn))) = (&running.live_preview_before, live)
            && drawn > *at
            && let Some((before, _)) = running.live_preview_before.take()
        {
            for id in before {
                egui_renderer.free_texture(&id);
            }
        }
        let live_ids = running.live_preview_before.as_ref().map_or(&running.live_preview_ids, |(before, _)| before);
        // A screen that could not draw what is live still shows the picture before it.
        let undrawn = running.output.as_ref().and_then(OutputHandle::undrawn);
        let preview = match live.and_then(|(front, _)| live_ids.get(front)) {
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
        home_state.preview_painted = false;
        repaints::laying_out();
        let laying_out = Instant::now();
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
                // In a theme version: kept under an id made from the slide's and the version's,
                // and made only when it isn't kept; as usual when the theme hasn't the version.
                ThumbOf::InVersion(at, slide, version) => {
                    let theme = match at {
                        Some(at) => {
                            let own = show.presentations().get(at).and_then(|song| song.theme);
                            own.and_then(|id| theme_arcs.iter().find(|theme| theme.id == id)).unwrap_or(main_theme)
                        }
                        None => bible_theme,
                    };
                    let key = Uuid::from_u128(slide.id.as_u128() ^ version.as_u128());
                    match thumbs.kept(key) {
                        Some(kept) => Some(kept),
                        None => match theme.slide_in_version(slide, version) {
                            Some(mut shown) => {
                                shown.id = key;
                                thumbs.get_in_version(gpu, egui_renderer, &shown, slide.id, media_for(slide), theme)
                            }
                            None => thumbs.get(gpu, egui_renderer, slide, media_for(slide), theme),
                        },
                    }
                }
                ThumbOf::Layout(index, slide) => {
                    // A layout is made anew each frame; an id from its template's (a version's
                    // templates are at the same places as the usual ones) lets its thumbnail be
                    // kept until the theme changes, when every thumbnail is dropped.
                    let mut kept = slide.clone();
                    let template = slide.template.map_or(index as u128, |id| id.as_u128());
                    kept.id = Uuid::from_u128(0x7068_656d_655f_6c61_796f_7574_0000_0000 ^ template);
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
        repaints::laid_out(egui_ctx, laying_out.elapsed());
        egui_state.handle_platform_output(home_window, std::mem::take(&mut full.platform_output));

        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // A new size settling, or thumbnails still of the size before: drawn again soon.
        let (delay, why) = match settling {
            Some(wait) if wait < delay => (wait, repaints::Timer::Settling),
            _ => (delay, repaints::Timer::Toolkit),
        };
        let (delay, why) =
            if thumbs.stale_shown() { (Duration::ZERO, repaints::Timer::OldThumbnails) } else { (delay, why) };
        // An animation is drawn at most a frame a screen refresh apart. A window macOS isn't
        // showing does not wait for the screen, and would otherwise be drawn thousands of times
        // a second while the preview fades.
        let delay = delay.max(FRAME);
        // Testing aid: redraw continuously, to check that memory stays flat under load.
        let delay = if std::env::var_os("SOFTPRESENTER_STRESS").is_some() { Duration::ZERO } else { delay };
        repaints::wake_after(&mut running.next_repaint, delay, why);

        let target = UiTarget {
            of: surface_setup::Of::Operator,
            surface: home_surface,
            set_up: home_set_up,
            renderer: egui_renderer,
            ctx: egui_ctx,
            label: "home window",
            offscreen,
        };
        let (jobs, screen, times) = paint_ui(gpu, target, full);
        // A surface still to be set up (a set-up lost, or a picture it refused) has the window
        // drawn again when its next try is due, never sooner than a frame from now.
        if let Some(at) = home_set_up.next_try(window_size, Instant::now()) {
            repaints::wake_at(&mut running.next_repaint, at, repaints::Timer::Settling);
        }
        repaints::painted(times);

        // Testing aid: show a slide, let the thumbnails fill in, then save what was drawn.
        running.frames += 1;
        // Testing aid: SOFTPRESENTER_OPEN_DIALOG=add-media opens the media bin's Add window a few
        // frames in, as a click on Add does, to see that the app goes on while it is open.
        if running.frames == 5 && std::env::var("SOFTPRESENTER_OPEN_DIALOG").is_ok_and(|which| which == "add-media") {
            running.home_state.media_request = Some(home::MediaRequest::Choose);
        }
        if let Some(path) = &self.options.capture_home {
            const GO_LIVE_AT: u32 = 3;
            const CAPTURE_AT: u32 = 30;
            // A smoke run stays up this many seconds before its picture is taken and it quits.
            static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let started = *STARTED.get_or_init(Instant::now);
            let wait = std::env::var("SOFTPRESENTER_CAPTURE_AFTER").ok().and_then(|s| s.parse::<f32>().ok());
            // The picture waits for the media bin's pictures, read on their own threads, and for
            // the fade to what was put on screen to end (with a moment for the screens' last
            // picture of it), so the preview shows it whole: taken at the 30th frame, it often
            // caught the dissolve part way, the preview's picture dark.
            let faded = running.preview_changed.elapsed().as_secs_f32() >= running.home_state.dissolve_seconds() + 0.25;
            let waited = wait.is_none_or(|seconds| started.elapsed().as_secs_f32() >= seconds)
                && !self.bin_pictures.any_coming()
                && faded;
            if running.frames == GO_LIVE_AT {
                commands.push(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 2 }));
                // Testing aid: SOFTPRESENTER_CAPTURE_MEDIA=N puts the media bin's Nth item on
                // screen instead of the first (a video, to see its controls).
                let media = std::env::var("SOFTPRESENTER_CAPTURE_MEDIA").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
                commands.push(Command::ShowMedia(media));
            }
            // Testing aid: SOFTPRESENTER_NDI_DEMO=<state> shows the Install NDI window in the home
            // window in a made-up state (see `ndi_download::NdiDownload::demo`).
            if running.frames == GO_LIVE_AT + 2
                && let Ok(demo) = std::env::var("SOFTPRESENTER_NDI_DEMO")
            {
                let state = &mut running.home_state;
                state.ndi_available = state.ndi_download.demo(&demo, ndi_download::Place::Home);
            }
            // Testing aid: open the Settings window.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(pane) = std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES")
            {
                running.home_state.preferences_open = true;
                // Testing aid: a page by name (integrations, or voxlive for VoxLive's own page).
                let state = &mut running.home_state;
                match pane.as_str() {
                    // A service's page; with -end, scrolled to its end (`integrations`).
                    "voxlive" | "voxlive-look" | "voxlive-end" => {
                        state.preferences_pane = home::Pane::Integrations;
                        state.voxlive.open = true;
                    }
                    "canva" | "canva-end" => {
                        state.preferences_pane = home::Pane::Integrations;
                        state.canva.open = true;
                    }
                    // Testing aid: AI assistants scrolled on to a part of the page: its tiles
                    // (mcp-tiles), Other apps opened (mcp-apps), or What gets copied opened
                    // (mcp-copied).
                    "mcp-tiles" | "mcp-apps" | "mcp-copied" => {
                        use crate::mcp::pane::Part;
                        state.preferences_pane = home::Pane::AiAssistants;
                        state.mcp.show_part = Some(match pane.as_str() {
                            "mcp-tiles" => Part::Tiles,
                            "mcp-apps" => Part::OtherApps,
                            _ => Part::WhatIsCopied,
                        });
                    }
                    // NDI's page; with a state after it (ndi-newer, ndi-install, ndi-licence…),
                    // as `ndi_download::NdiDownload::demo` draws it, with made-up numbers.
                    name if name == "ndi" || name.starts_with("ndi-") => {
                        state.preferences_pane = home::Pane::Integrations;
                        state.ndi_card_open = true;
                        if let Some(demo) = name.strip_prefix("ndi-") {
                            state.ndi_available = state.ndi_download.demo(demo, ndi_download::Place::Settings);
                        }
                    }
                    // The songbooks' page, where the song catalog is offered.
                    "songbooks" => {
                        state.preferences_pane = home::Pane::Integrations;
                        state.songbooks.open = true;
                    }
                    // Slides scrolled on to In your window, with Interface size.
                    "interface-size" => {
                        state.preferences_pane = home::Pane::Slides;
                        state.show_interface_size = true;
                    }
                    // Settings › Song files with Find pressed at once (find-songs), and with Import
                    // pressed too once the search has ended (find-songs-import: see
                    // `find_songs_cli::end_search`), or Stop once it has found two
                    // (find-songs-stop: see `find_songs_cli::tick_find_songs`).
                    "find-songs" | "find-songs-import" | "find-songs-stop" => {
                        state.preferences_pane = home::Pane::SongFiles;
                        state.find_songs.asks.push(find_songs::Ask::Find);
                    }
                    // Song files before a search, with a folder added to where it looks: its
                    // name joins the line's sentence (find-songs-folder).
                    "find-songs-folder" => {
                        state.preferences_pane = home::Pane::SongFiles;
                        state.find_songs.added.push(std::path::PathBuf::from("Worship songs"));
                    }
                    // TimelyChurch's page, where it is offered.
                    "timelychurch" | "timelychurch-end" => {
                        state.preferences_pane = home::Pane::Integrations;
                        state.timelychurch.open = true;
                    }
                    // Testing aid: the What's new window over About (whats-new), or scrolled to
                    // its end (whats-new-end).
                    "whats-new" | "whats-new-end" => {
                        crate::whats_new::open_over_about(state);
                        if pane == "whats-new-end" {
                            state.whats_new.scroll_to_end();
                        }
                    }
                    // Any page by its name ("phone-remote", "key mappings", "mcp").
                    other => {
                        if let Some(found) = home::Pane::named(other) {
                            state.preferences_pane = found;
                        }
                    }
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
                    let mut arrangement = song.new_arrangement(home::arrangement_name);
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
            // Testing aid: SOFTPRESENTER_CAPTURE_HIDE_WORDS=1 hides the words of the slide the
            // capture run put on screen, as Hide the Words does, for a picture of what stays.
            if running.frames == GO_LIVE_AT + 10 && std::env::var_os("SOFTPRESENTER_CAPTURE_HIDE_WORDS").is_some() {
                commands.push(Command::HideWords);
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
            // Testing aid: SOFTPRESENTER_CAPTURE_MEDIA_PLAYLIST=1 shows the first media playlist in
            // the media strip.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_CAPTURE_MEDIA_PLAYLIST").is_some() {
                running.home_state.media_playlist_shown = running.home_state.media_playlists.first().map(|p| p.id);
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
            // Testing aid: SOFTPRESENTER_FREE_SONGBOOKS=1 opens the Free Songbooks window, as its
            // item in the import menu does.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("SOFTPRESENTER_FREE_SONGBOOKS").is_some() {
                running.home_state.free_songbooks.open = true;
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
                    running.home_state.pick_passage(&self.show);
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
            // Testing aid: SOFTPRESENTER_BIBLE_GO_LIVE=1 puts the verse picked on screen, as Enter
            // does, once SOFTPRESENTER_BIBLE_LOOKUP has looked its passage up.
            if running.frames == GO_LIVE_AT + 8
                && std::env::var_os("SOFTPRESENTER_BIBLE_GO_LIVE").is_some()
                && let Some(at) = running.home_state.pick(&self.show)
            {
                commands.push(Command::GoTo(at));
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
            // Testing aid: SOFTPRESENTER_CAPTURE_THEMES=version shows the theme's first version's
            // templates; =add-version opens the sheet that adds a version to it.
            if running.frames == GO_LIVE_AT + 4 {
                let what = std::env::var("SOFTPRESENTER_CAPTURE_THEMES").unwrap_or_default();
                let state = &mut running.home_state;
                if what == "version"
                    && let Some(edit) = state.theme_edit.as_mut()
                {
                    let first = edit.theme.versions.first().map(|version| version.id);
                    state.editing = edit.show(first, 0);
                } else if what == "add-version" && state.theme_edit.is_some() {
                    state.version_sheet = Some(home::VersionSheet::adding(state));
                } else if what == "end-look"
                    && let Some(theme) = state.theme_edit.as_ref().map(|edit| edit.index)
                {
                    // Testing aid: =end-look opens the theme's end mark's window over the
                    // editor, as Song end's Change… does.
                    end_look::open(state, theme, end_look::Door::Editor);
                }
            }
            // Testing aid: as if the theme's name and its first template's name had been
            // clicked to rename them.
            if running.frames == GO_LIVE_AT + 4
                && std::env::var("SOFTPRESENTER_CAPTURE_THEMES").as_deref() == Ok("rename")
                && let Some(edit) = running.home_state.theme_edit.as_ref()
            {
                let template =
                    edit.theme.templates.first().map(|t| theme_store::shown_name(t).into_owned()).unwrap_or_default();
                running.home_state.renaming_theme = Some(theme_store::shown_theme_name(&edit.theme).into_owned());
                running.home_state.renaming_template = Some((0, template));
            }
            // Testing aid: also open the slide editor, to check how it looks.
            // Testing aid: `selected` edits the first slide of the song open in the window (once
            // SOFTPRESENTER_OPEN_ITEM has opened it), with its top object selected; otherwise the
            // first song's third slide.
            // Testing aid: SOFTPRESENTER_CAPTURE_EDITOR=props opens the Edit view on the props
            // instead (on a prop made for the picture when there is none); `no-props` on the
            // props with none, their list empty.
            let props_capture = std::env::var("SOFTPRESENTER_CAPTURE_EDITOR").unwrap_or_default();
            let on_props = props_capture == "props" || props_capture == "no-props";
            if running.frames == GO_LIVE_AT + 2 && on_props {
                let state = &mut running.home_state;
                if props_capture == "no-props" {
                    state.props.clear();
                } else if state.props.is_empty() {
                    state.props.push(props::Prop::new("Logo".to_owned()));
                }
                state.edit_kind_request = Some(home::EditKind::Props);
            }
            // Testing aid: `selected-typing` is both: the song chosen, its words being typed in.
            let editor_capture = std::env::var("SOFTPRESENTER_CAPTURE_EDITOR").unwrap_or_default();
            let picked = editor_capture == "selected" || editor_capture == "selected-typing";
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
                    if editor_capture == "typing" || editor_capture == "selected-typing" {
                        draft.typing = draft.selected;
                        draft.typing_started = true;
                        // Testing aid: SOFTPRESENTER_CAPTURE_SELECT=FROM-TO also selects those
                        // characters of the words, for a picture of the inspector's head then.
                        draft.typing_range = std::env::var("SOFTPRESENTER_CAPTURE_SELECT").ok().and_then(|r| {
                            let (a, b) = r.split_once('-')?;
                            Some((a.parse().ok()?, b.parse().ok()?))
                        });
                        if draft.typing_range.is_some() {
                            draft.typing_started = false;
                        }
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
                // Testing aid: SOFTPRESENTER_CAPTURE_NDI=1 puts an NDI source box on the slide,
                // selected, as NDI Source in the tools does, for a picture of the inspector's NDI
                // Source section (with SOFTPRESENTER_INSPECTOR_TAB=shape).
                if std::env::var_os("SOFTPRESENTER_CAPTURE_NDI").is_some()
                    && let Some(draft) = running.home_state.editing.as_mut()
                {
                    draft.add_ndi();
                }
                // Testing aid: SOFTPRESENTER_CAPTURE_AIRPLAY=1 puts an AirPlay box on the slide,
                // selected, as AirPlay in the tools does.
                if std::env::var_os("SOFTPRESENTER_CAPTURE_AIRPLAY").is_some()
                    && let Some(draft) = running.home_state.editing.as_mut()
                {
                    draft.add_mirror();
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
                        state.renaming_service = Some((state.services.front_at(), state.services.front().name.clone()));
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
                    "theme" => {
                        state.renaming_theme = state
                            .theme_edit
                            .as_ref()
                            .map(|edit| theme_store::shown_theme_name(&edit.theme).into_owned())
                    }
                    "template" => {
                        let name = state.theme_edit.as_ref().and_then(|edit| edit.theme.templates.first());
                        state.renaming_template =
                            name.map(|template| (0, theme_store::shown_name(template).into_owned()));
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
                match capture(gpu, egui_renderer, &jobs, &screen, home_set_up.config().format, path) {
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
        // The What's new window goes with the Settings window it is drawn over.
        if !running.home_state.preferences_open {
            running.home_state.whats_new.close();
        }
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
                    running.home_state.notices.trouble(problem);
                }
            }
        }
        // A card in Settings › Screens brings the Screens window in front, where it may be
        // behind (never in a quiet run, whose windows stay behind everything).
        if std::mem::take(&mut running.home_state.screens_to_front)
            && !quiet
            && let Some(panel) = running.panels.iter().find(|panel| panel.kind == Panel::Screens)
        {
            panel.window.focus_window();
        }
    }

    /// Draws one frame of a panel window and returns what was asked for in it.
    fn draw_panel(&mut self, id: WindowId) -> Vec<Command> {
        let output_on = self.show.output_enabled();
        let show = &self.show;
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running { panels, gpu, home_state, .. } = running;
        let Some(panel) = panels.iter_mut().find(|panel| panel.window.id() == id) else {
            return Vec::new();
        };
        let settling = follow_panel_zoom(panel, home_state.zoom_now);
        // The surface follows the window, and one whose last set-up was lost is set up again
        // (see `surface_setup`); until that works nothing is drawn into it.
        let inner = panel.window.inner_size();
        let window_size = (inner.width, inner.height);
        let (surface, device) = (&panel.surface, &gpu.device);
        panel.set_up.follow(window_size, Instant::now(), |config| {
            surface_setup::set_up(device, surface, config, surface_setup::Of::Panel)
        });
        let mut raw_input = panel.state.take_egui_input(&panel.window);
        // Laid out for the surface's size while that is not the window's (see the home window).
        if inner.width > 0 && inner.height > 0 && panel.set_up.size() != window_size {
            let points = panel.window.scale_factor() as f32 * panel.ctx.zoom_factor();
            let (w, h) = (panel.set_up.size().0 as f32 / points, panel.set_up.size().1 as f32 / points);
            raw_input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)));
        }
        let mut commands = Vec::new();
        let kind = panel.kind;
        let mut full = panel.ctx.run_ui(raw_input, |ui| match kind {
            Panel::Screens => commands.extend(home::screens_ui(ui, home_state, output_on)),
            Panel::Settings => home::settings_ui(ui, home_state, show),
        });
        panel.state.handle_platform_output(&panel.window, std::mem::take(&mut full.platform_output));
        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        let delay = settling.map_or(delay, |wait| delay.min(wait));
        panel.next_repaint = Instant::now().checked_add(delay.max(FRAME));
        let target = UiTarget {
            of: surface_setup::Of::Panel,
            surface: &panel.surface,
            set_up: &mut panel.set_up,
            renderer: &mut panel.renderer,
            ctx: &panel.ctx,
            label: "panel window",
            offscreen: None,
        };
        let (jobs, screen, _) = paint_ui(gpu, target, full);
        // A surface still to be set up has the window drawn again when its next try is due.
        if let Some(at) = panel.set_up.next_try(window_size, Instant::now()) {
            panel.next_repaint = Some(panel.next_repaint.map_or(at, |next| next.min(at)));
        }
        panel.frames += 1;
        // Testing aid: a capture run with a panel asked for also saves that window, every
        // frame once it has settled, so the last one saved is the latest.
        let ending = match kind {
            Panel::Screens => "screens.png",
            Panel::Settings => "settings.png",
        };
        if let Some(path) = &self.options.capture_home
            && panel.frames >= 2
            && let Err(e) = capture(
                gpu,
                &panel.renderer,
                &jobs,
                &screen,
                panel.set_up.config().format,
                &path.with_extension(ending),
            )
        {
            eprintln!("Could not save the {} window: {e}", kind.title());
        }
        commands
    }

    /// Carries out a change asked for from a slide's right-click menu, the keys over the Edit
    /// view's slides, or Delete.
    fn handle_slide_request(&mut self) {
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
        // The slide being edited in this song, if one is: where it is and which it is, so the
        // editor stays on it wherever the change puts it.
        let followed = self.running.as_ref().and_then(|running| {
            let state = &running.home_state;
            let draft = state.editing.as_ref().filter(|_| state.theme_edit.is_none())?;
            (draft.at.presentation == at.presentation).then_some((draft.at, draft.slide.id))
        });
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
            // The slide being edited deleted: the one now in its place opens.
            let instead = followed.map_or(0, |(open, _)| home::place_after_deleting(open.slide, &slides));
            self.follow_edited_slide(followed, instead);
            return;
        }
        let (changed, shift) = match &request {
            SlideRequest::DeleteMany(_) | SlideRequest::Move { .. } => (None, None),
            SlideRequest::PasteAfter(slides) | SlideRequest::AddAfter(slides) => {
                (song.with_slides_after(at.slide, slides), Some(true))
            }
            SlideRequest::Group(slides, name) => (song.with_group_over(slides, name), None),
            SlideRequest::Edit(edits) => {
                let mut changed = Some((*song).clone());
                for (place, slide) in edits {
                    changed = changed.and_then(|song| song.with_slide(*place, slide.clone()));
                }
                // Their pictures are made again.
                if let Some(running) = self.running.as_mut() {
                    for (_, slide) in edits {
                        running.thumbs.forget(slide.id, &mut running.egui_renderer);
                    }
                }
                (changed, None)
            }
        };
        let Some(changed) = changed else { return };
        self.remember_for_undo(at.presentation);
        self.note_song_step();
        let mut changes = self.show.replace_presentation(at.presentation, Arc::new(changed));
        // As many places taken as slides were pasted.
        let added = match &request {
            SlideRequest::PasteAfter(slides) | SlideRequest::AddAfter(slides) => slides.len(),
            _ => 1,
        };
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
        match (&request, followed, self.running.as_mut()) {
            // The Edit view's Add and Copy: the first slide made opens.
            (SlideRequest::AddAfter(_), Some(_), Some(running)) => {
                let made = sp_engine::SlideRef { slide: at.slide + 1, ..at };
                running.home_state.editing = EditDraft::of(&self.show, made);
                running.home_state.picked_slides.clear();
            }
            _ => self.follow_edited_slide(followed, followed.map_or(0, |(open, _)| open.slide)),
        }
    }

    /// The slide being edited, found again by its id after its song's slides changed: `followed`
    /// is where it was and which it is. Gone, the slide now at `instead` (or the last) opens.
    fn follow_edited_slide(&mut self, followed: Option<(sp_engine::SlideRef, Uuid)>, instead: usize) {
        let Some((at, id)) = followed else { return };
        let Some(running) = self.running.as_mut() else { return };
        let song = self.show.presentations().get(at.presentation);
        match song.and_then(|song| home::edited_slide_place(song, id, instead)) {
            // Still there: its draft goes with it, with the changes it can undo.
            Some((slide, true)) => {
                if let Some(draft) = running.home_state.editing.as_mut() {
                    draft.at.slide = slide;
                }
            }
            Some((slide, false)) => {
                running.home_state.editing = EditDraft::of(&self.show, sp_engine::SlideRef { slide, ..at });
            }
            // No slides left (or no song): nothing to edit.
            None => running.home_state.editing = None,
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
            SongRequest::Duplicate { item } => {
                let mut copy = (*song).clone();
                copy.id = Uuid::new_v4();
                copy.title = crate::i18n::tf("notices.copy_name", "{name} copy", &[("name", &song.title)]);
                for group in &mut copy.groups {
                    group.id = Uuid::new_v4();
                    for slide in &mut group.slides {
                        slide.id = Uuid::new_v4();
                        for object in &mut slide.objects {
                            object.id = Uuid::new_v4();
                        }
                    }
                }
                let path = song_ids::library_path(copy.id);
                if let Some(folder) = path.as_ref().and_then(|path| path.parent()) {
                    let _ = std::fs::create_dir_all(folder);
                }
                let (id, title) = (copy.id, copy.title.clone());
                let at = self.show.add_presentation(Arc::new(copy));
                self.sources.push(path);
                self.unsaved.push(at);
                self.save_unsaved();
                if let Some(running) = self.running.as_mut() {
                    let key = song_ids::library_key(id, &title);
                    running.home_state.song_keys.push(key.clone());
                    running.home_state.copy_beside(index, at, &key, item, &title);
                }
                self.apply(vec![Command::Select(at)]);
            }
            SongRequest::Delete => {
                // A song made in the app goes to the Trash; one opened from elsewhere stays put.
                // Its file goes on the library writer's thread, after any save of it still on
                // its way (which would otherwise make the file again); a file that won't go is
                // said then.
                let own = bible_store::data_dir().map(|dir| dir.join("Songs"));
                if let (Some(Some(path)), Some(own)) = (self.sources.get(index), own)
                    && path.starts_with(&own)
                {
                    let path = path.clone();
                    self.writer.then(move || {
                        if !path.exists() {
                            return Ok(());
                        }
                        sweep::to_trash(&path).map_err(|e| {
                            crate::i18n::tf(
                                "notices.song.not_trashed",
                                "Could not move the song to the Trash: {error}",
                                &[("error", &e.to_string())],
                            )
                        })
                    });
                }
                if self.show.live().is_some_and(|live| live.presentation == index) {
                    self.apply(vec![Command::Clear(sp_engine::Layer::Slides)]);
                }
                self.unsaved.retain(|at| *at != index);
                // A deleted song can't be brought back by Undo, which would save it again.
                self.history.forget(index);
                if let Some(running) = self.running.as_mut() {
                    let state = &mut running.home_state;
                    state.removed_songs.push(index);
                    state.notices.settle(outside::Waiting::Song(index));
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
                let next = (0..self.show.presentations().len()).find(|at| !removed.contains(at) && !self.app_made(*at));
                if self.show.selected() == index
                    && let Some(next) = next
                {
                    self.apply(vec![Command::Select(next)]);
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
        // The end of a song's mark goes with the slide, not with what is over it: when it is
        // no longer as the screens were told (Settings' switch, the song set to go round, its
        // order or the copyright line's slides changed), the slide is sent again, and the mark
        // dissolves on or off under the words.
        if self.running.as_ref().is_some_and(|running| running.end_mark_sent != self.end_mark_now()) {
            self.refresh_output();
        }
        // Linked text showing the time or a timer keeps the preview ticking.
        let links_tick = self.show.live_slide().is_some_and(|(_, slide)| links::ticks(slide));
        // The copyright line of the song on screen, on the slides Settings says.
        let copyright_line = self.running.as_ref().and_then(|running| self.copyright_line_now(&running.home_state));
        self.hand_over_lists(false);
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &mut running.home_state;
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
        reports::stats::feature_of(&action);
        if action == A::Quit {
            return self.quit(el);
        }
        if action == A::ImportSongs {
            return self.choose_songs_to_import(false);
        }
        if action == A::ImportLibraryFolder {
            return self.choose_library_folder_to_import();
        }
        if action == A::ImportBundle {
            return self.choose_bundles_to_import();
        }
        if action == A::ImportWords {
            return self.choose_words_to_import();
        }
        if action == A::ImportSlides {
            if let Some(running) = self.running.as_mut() {
                running.home_state.slides_sheet = true;
                running.home_window.request_redraw();
            }
            return;
        }
        if action == A::ExportSongs {
            let picked = self.running.as_ref().map(|r| r.home_state.picked_songs.clone()).unwrap_or_default();
            let songs = if picked.is_empty() { vec![self.show.selected()] } else { picked };
            self.export_from_window(&songs, None);
            return;
        }
        if action == A::CheckForUpdates {
            // Settings › About shows that it is checking, and then the answer.
            if let Some(running) = self.running.as_mut() {
                running.home_state.preferences_open = true;
                running.home_state.preferences_pane = home::Pane::About;
                running.home_window.request_redraw();
            }
            return self.check_for_updates(None);
        }
        if action == A::ReloadSongs {
            let said = self.reload_all().summary();
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.info(said);
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
        // In the Service view and on the Bible page (which has no undo of its own), with nothing
        // being typed, Undo and Redo step through the changes made to songs.
        let in_show = self.running.as_ref().is_some_and(|r| {
            let state = &r.home_state;
            state.edit_kind().is_none() && !r.egui_ctx.text_edit_focused()
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
                // Written files, out of sight: said in the list (Settings' card says it in the
                // card instead), and the card reads afresh.
                state
                    .notices
                    .outcome(cli_install::install().map(|done| cli_install::installed_words(&done, cfg!(windows))));
                state.command_line = Default::default();
            }
            A::ConfigureScreens => state.settings_open = true,
            A::SetUp => state.setup = Some(setup::Setup::new()),
            A::StageLayouts => state.edit_kind_request = Some(home::EditKind::Stage),
            A::NewSong => state.new_song = true,
            // Handled above, before the window's state is borrowed.
            A::CheckForUpdates
            | A::ImportSongs
            | A::ImportLibraryFolder
            | A::ImportBundle
            | A::ImportWords
            | A::ImportSlides
            | A::ReloadSongs
            | A::ExportSongs => {}
            A::NewSlide => {
                let song = match (&state.editing, &state.theme_edit) {
                    (Some(draft), None) => draft.at.presentation,
                    // Edit › Props with no props: no song is open there.
                    (None, None) if state.props_editor_open => usize::MAX,
                    _ => self.show.selected(),
                };
                if !state.app_made(song) && song < self.show.presentations().len() {
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
                    state.confirm_delete_theme = None;
                    state.confirm_delete_songs.clear();
                    state.confirm_delete_media.clear();
                } else if let Some(screen) =
                    running.screen_windows.iter().find(|have| Some(have.window.id()) == self.focused).map(|w| w.screen)
                    && let Some(closed) = state.screens.iter_mut().find(|have| have.id == screen)
                {
                    closed.on = false;
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
            // The import menu over the left column, in the Service view.
            A::Import => {
                state.view_request = Some(0);
                state.import_menu_request = true;
            }
            A::KeysSheet => state.key_sheet = true,
            A::ShowView => state.view_request = Some(0),
            A::EditView => state.view_request = Some(1),
            // Control-B: the Bible page, with the cursor in Scripture Lookup and its words
            // selected, ready for the next reference.
            A::BibleView => {
                state.view_request = Some(2);
                state.bible.focus_lookup = true;
            }
            A::ThemesView => state.view_request = Some(3),
            A::Find => state.open_find(),
            A::ReflowView => {
                let song = self.show.selected();
                // A song of the song catalog being looked at joins the library to be changed.
                if let Some(held) = state.catalog.held_at(song).cloned() {
                    state.catalog.join(&held, catalog_songs::Then::Words);
                } else if !state.app_made(song)
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
            A::ShowLiveSong => {
                // The song on screen, chosen in the Service view.
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
            A::Website | A::ReportIssue => {
                let page = if action == A::ReportIssue { home::Page::ReportIssue } else { home::Page::Guide };
                home::open_page(state, page);
            }
            // The What's new window over Settings › About, where its key is (as Check for
            // Updates opens there).
            A::WhatsNew => crate::whats_new::open_over_about(state),
        }
        running.home_window.request_redraw();
        if !commands.is_empty() {
            self.apply(commands);
        }
        if let Some(forward) = step {
            self.step_keys(forward);
        }
    }

    /// A key that runs the show, from the home window or a screen's: a letter's slide goes up; an
    /// action from the table is carried out (`key_action`), once a key doing Next or Previous has
    /// had its say over a slide picked (`pick_key`).
    fn key_press(&mut self, key: &Pressed, press: KeyPress) {
        match press {
            KeyPress::Letter(command) => self.apply(vec![command]),
            KeyPress::Action(action) => {
                if !self.pick_key(key, action) {
                    self.key_action(action);
                }
            }
        }
    }

    /// An F row pressed while the find box or Scripture Lookup has the keyboard
    /// (`KeyGoes::Typed`): what it does there (`typed_key`), and what that leaves is done as
    /// anywhere.
    fn typed_key_press(&mut self, key: &Pressed, action: KeyAction) {
        let Some(running) = self.running.as_mut() else { return };
        let rest = typed_key(&mut running.home_state, &running.egui_ctx, action);
        running.home_window.request_redraw();
        if let Some(action) = rest {
            self.key_press(key, KeyPress::Action(action));
        }
    }

    /// What a key set to `action` does (Settings › Keys): Next and Previous act on the open item
    /// (see `step_keys`); the show's other actions go to it as they are (`action_command`); Show
    /// Selected is Enter, and the window's own act on it (`window_key_action`).
    fn key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Next => {
                self.step_keys(true);
            }
            KeyAction::Previous => {
                self.step_keys(false);
            }
            KeyAction::ClearAll
            | KeyAction::ClearSlide
            | KeyAction::ClearMedia
            | KeyAction::ClearProps
            | KeyAction::ClearAudio
            | KeyAction::ClearMessages
            | KeyAction::HideWords
            | KeyAction::ShowAgain
            | KeyAction::First
            | KeyAction::Last => {
                if let Some(command) = action_command(action) {
                    self.apply(vec![command]);
                }
            }
            // What Enter puts on screen: the slide picked, else the open item from its first slide.
            KeyAction::ShowSelected => self.enter_key(),
            // The window's own, each as the control it stands for (`window_key_action`).
            KeyAction::AddToPlaylist
            | KeyAction::RemoveFromPlaylist
            | KeyAction::GoToBible
            | KeyAction::GoToService => {
                if let Some(running) = self.running.as_mut() {
                    window_key_action(&self.show, &mut running.home_state, action);
                    running.home_window.request_redraw();
                }
            }
            // A key set to Nothing is kept from the window and does nothing (`show_key_press`).
            KeyAction::Nothing => {}
        }
    }

    /// Ticks the menu bar's items that are on now: the view, the slide view, the audience
    /// output and Identify Screens; and greys those that can't be chosen now (`menubar::enabled`).
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
            // A field being typed in, or a window of its own with the keyboard (Settings), has
            // an undo of its own: Undo and Redo stay on for it.
            let in_panel = running.panels.iter().any(|panel| Some(panel.window.id()) == self.focused);
            // The menus' words follow the language set (cheap when it hasn't changed).
            menu.follow_language();
            menu.set_enabled(&menubar::Greying {
                editing: state.editing.is_some(),
                typing: running.egui_ctx.text_edit_focused() || in_panel,
                can_undo: state.can_undo(false),
                can_redo: state.can_undo(true),
                style_copied: state.inspector.copied_style.is_some(),
                checking_updates: updates::status().checking,
                listening: home::settings::keys_card::listening(state),
            });
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

    /// The pictures that were in the media bin last time, then any named on the command line,
    /// as the start opens the window. Every item is in the bin from the start, in its place and
    /// with its id, so slide actions, playlists and what plays at the start find it; its small
    /// copy is read on a reader's thread, so the window opens at once and the bin fills in. One
    /// whose file can't be read leaves the bin then, as it was never let in before.
    fn fill_media_bin(&mut self) {
        let (saved, unread) = media::load_list();
        self.unread.extend(unread);
        self.media_playlists = saved.playlists;
        let mut pictures_to_load = saved.entries;
        for path in std::mem::take(&mut self.media_named) {
            if !pictures_to_load.iter().any(|entry| entry.path == path) {
                pictures_to_load.push(media::Entry {
                    id: Uuid::new_v4(),
                    path,
                    fit: sp_model::ImageFit::Fill,
                    looping: true,
                    followed: false,
                    sample: None,
                });
            }
        }
        let mut seen = std::collections::HashSet::new();
        for entry in &pictures_to_load {
            let path = &entry.path;
            // A file listed twice (as an earlier version could save it) is shown once.
            if !seen.insert(path.clone()) || self.show.media().iter().any(|item| item.id == entry.id) {
                continue;
            }
            let name = path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
            // Its id is kept between runs, so a slide's action that shows it still finds it.
            self.show.add_media(MediaItem {
                id: entry.id,
                fit: entry.fit,
                looping: entry.looping,
                sample: entry.sample.clone(),
                ..MediaItem::new(name, path.clone())
            });
            self.bin_pictures.read(entry.id, path);
            if entry.followed {
                self.followed_items.insert(entry.id);
            }
        }
        self.bin_started = Some((Instant::now(), self.show.media().len()));
    }

    /// Hands the library's writer the Bible page's set-up (translations, boxes, options) and the
    /// passages that were on screen, the small choices (transition, view, slide size) and the
    /// playlists, each the moment it changes, so it is as it was left at the next start. The
    /// small choices wait until the window has drawn a few times, so a start's own settling isn't
    /// saved; `quitting`, they don't: a quit hands over whatever changed a moment before (an `sp`
    /// command just answered, the window not drawn since). The playlists go after the songs a
    /// playlist has just been given.
    fn hand_over_choices(&mut self, quitting: bool) {
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        let now = state.bible.choices();
        if now != self.bible_page_saved {
            let page = service::SavedPassage { lookup: String::new(), ..state.bible.settings() };
            save_setting(&mut self.paced, &self.writer, &mut state.notices, service::bible_page_to_save(&page));
            self.bible_page_saved = now;
        }
        if std::mem::take(&mut state.bible.history_changed) {
            let save = service::bible_history_to_save(&state.bible.history);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        let now = state.prefs();
        if self.prefs_saved.as_ref() != Some(&now) && (self.frames_since_start > 10 || quitting) {
            save_setting(&mut self.paced, &self.writer, &mut state.notices, service::prefs_to_save(&now));
            self.prefs_saved = Some(now);
        }
        let services = &mut state.services;
        if std::mem::take(&mut services.changed) {
            services.fresh = false;
            match service::to_save(services) {
                Ok(save) => {
                    self.writer.write(save);
                }
                Err(problem) => state.notices.trouble(problem),
            }
        }
    }

    /// Hands the library's writer the timers, messages, props, macros, stage layouts and
    /// libraries changed since this was last done (each loop, and at quit); `quitting`, the
    /// screens too, which the loop hands over as it places them (`sync_screens`), and never in a
    /// test copy, which keeps the screens it was started with. The libraries go after the songs
    /// a library has just been given.
    fn hand_over_lists(&mut self, quitting: bool) {
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        if std::mem::take(&mut state.timers_changed) {
            let save = timers::to_save(&state.timers);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        if std::mem::take(&mut state.messages_changed) {
            let save = messages::to_save(&state.messages);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        if std::mem::take(&mut state.props_changed) {
            let save = props::to_save(&state.props);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        if std::mem::take(&mut state.macros_changed) {
            let save = macros::to_save(&state.macros);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        if std::mem::take(&mut state.stage_layouts_changed) {
            let save = stage::layouts_to_save(&state.stage_layouts);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
        if std::mem::take(&mut state.libraries_changed) {
            match libraries::to_save(&state.libraries) {
                Ok(save) => {
                    self.writer.write(save);
                }
                Err(problem) => state.notices.trouble(problem),
            }
        }
        if quitting && !quiet && std::mem::take(&mut state.screens_changed) {
            let save = screens::to_save(&state.screens);
            save_setting(&mut self.paced, &self.writer, &mut state.notices, save);
        }
    }

    /// Hands the library's writer everything changed, whatever its time: songs and themes (one
    /// whose save failed tried once more, now), the small choices, the lists and every settings
    /// file waiting its turn. What a quit does first, and what Windows asking the app to close
    /// does.
    fn hand_over_all(&mut self) {
        self.save_unsaved();
        for due in self.retries.due(Instant::now(), true) {
            self.save_again(due);
        }
        self.hand_over_choices(true);
        self.hand_over_lists(true);
        self.paced.flush(&self.writer);
    }

    /// The Restart Manager asked the app to close (Windows' way for an installer that needs a
    /// file the app holds, such as a driver or NDI update run in the room). Everything changed
    /// is saved now, waited for here as the program may be ended soon after, but the app stays
    /// open: the screens never go dark in the middle of a service for it, and the installer
    /// asks for a restart instead. A save that fails is tried again as usual.
    fn save_for_close_app(&mut self) {
        applog::event("quit", "Windows asked SoftPresenter to close, as an installer needs a file it uses: saving");
        let began = Instant::now();
        self.hand_over_all();
        self.say_saving_to_windows();
        self.finish_saves();
        self.writer.drain();
        self.finish_saves();
        self.stop_saying_saving_to_windows();
        applog::event(
            "quit",
            &format!(
                "everything is saved, in {:.0} ms; SoftPresenter stays open, as the screens may be in use",
                began.elapsed().as_secs_f64() * 1000.0
            ),
        );
    }

    /// Windows asked whether the session may end (a log-off, a restart or a shut-down may follow,
    /// or be called off): everything changed is handed to the library's writer now, so less is
    /// left to write if the session does end, and Windows is told why the app keeps it waiting
    /// while it writes. The app stays open, the screens on; nothing is waited for here.
    fn save_as_session_may_end(&mut self) {
        applog::event("quit", "Windows asked whether the session may end: handing every save over now");
        self.hand_over_all();
        self.say_saving_to_windows();
    }

    /// While the library's writer has files to write, Windows is told why the app keeps a
    /// log-off or shut-down waiting ("Saving…", in the language set), which it shows beside the
    /// app if the wait is long; taken back once everything is written (`about_to_wait`, and the
    /// leaving). On Windows only.
    fn say_saving_to_windows(&mut self) {
        #[cfg(windows)]
        if !self.saying_saving
            && !self.writer.is_idle()
            && let Some(window) = self.running.as_ref().and_then(|running| win32_handle(&running.home_window))
            && sp_winapi::block_shutdown(window, crate::i18n::t("notices.saving_for_windows", "Saving\u{2026}"))
        {
            self.saying_saving = true;
            applog::event("quit", "told Windows the app is saving, which it shows if it is kept waiting");
        }
    }

    /// Takes back what `say_saving_to_windows` told Windows, once the writer has nothing left to
    /// write.
    fn stop_saying_saving_to_windows(&mut self) {
        #[cfg(windows)]
        if self.saying_saving
            && self.writer.is_idle()
            && let Some(window) = self.running.as_ref().and_then(|running| win32_handle(&running.home_window))
        {
            sp_winapi::unblock_shutdown(window);
            self.saying_saving = false;
            applog::event("quit", "everything is written: Windows is no longer told the app is saving");
        }
    }

    /// What a quit does at once, however it came: what was changed is handed to be saved, and
    /// the screens close (their NDI sources stopped, see `NdiOutput::stop`).
    fn begin_quit(&mut self) {
        self.hand_over_all();
        self.keep_held_aside();
        // A sync under way is stopped, and the screens close first: nothing waits in front of
        // the audience.
        self.canva.stop();
        if let Some(output) = self.running.as_mut().and_then(|r| r.output.take()) {
            output.shutdown();
        }
        // With the screens gone, the display is no longer kept awake, and isn't again.
        self.keep_awake.end();
        if let Some((_, mut player)) = self.video.take() {
            say_video_pace(&mut player, true);
        }
        self.video_leaving = None;
        self.announcing = None;
        // Every video player is let go now, so its FFmpeg has ended by the time the app goes
        // (`quit` waits for them, briefly).
        self.slide_videos.clear();
        self.quitting = Some(Instant::now());
        // The last anonymous report, if reports are on, goes from its own thread while the
        // rest of the leaving is done.
        reports::quit_begin();
    }

    /// The app is being ended without its own Quit having finished: on a Mac, by the Dock's
    /// Quit, a log-out, a restart or a shut-down, where macOS says the app will terminate and
    /// then ends the program itself, with no more turns of the event loop. Everything the
    /// app's Quit does is done here, now, waiting where it must (the system waits for this to
    /// return): what was changed is saved and every file handed to the library's writer is
    /// written, the screens' thread ends with its NDI sources stopped (a program that ended
    /// while one was sending crashed as it went), the helpers leave, the control socket goes
    /// and the run is noted as ended. Without it none of that happened: the last second's
    /// changes were lost, and the next start said the app had not quit. A Canva sign-in being
    /// refreshed this very moment is not waited for, and the last anonymous report goes if it
    /// has time. Once only, whichever way the app goes.
    fn leave_at_once(&mut self) {
        self.left = true;
        let began = Instant::now();
        if self.quitting.is_none() {
            self.begin_quit();
        }
        // A settings change since the quit began is handed over too.
        self.hand_over_choices(true);
        self.hand_over_lists(true);
        self.paced.flush(&self.writer);
        self.finish_saves();
        self.writer.drain();
        self.finish_saves();
        // A song the writer refused as it finished (its file changed outside meanwhile) is held
        // only now: it is kept aside too, so no change is lost on the way out.
        self.keep_held_aside();
        self.canva.shut_down();
        self.remove_replaced_canva_videos();
        self.mirror.quit();
        wait_for_videos_let_go();
        if let Ok(mut listening) = self._control.lock() {
            listening.take();
        }
        applog::event(
            "quit",
            &format!(
                "The system ended the app without its Quit: saved and stopped first, in {:.0} ms",
                began.elapsed().as_secs_f64() * 1000.0
            ),
        );
        applog::last_run::ended();
    }

    /// Windows is ending the session (a log-off, a restart, a shut-down): it asked the operator's
    /// window to close, and may end the program as soon as this returns, so everything the app's
    /// Quit does is done now (`leave_at_once`), then the loop is told to end.
    fn leave_for_session_end(&mut self, el: &ActiveEventLoop) {
        let late = sp_winapi::session_end_late();
        applog::event(
            "quit",
            if late {
                "Windows is ending the session, and asked while the app was busy: saving and stopping now"
            } else {
                "Windows is ending the session: saving and stopping first"
            },
        );
        if !self.left {
            if self.quitting.is_none() {
                self.begin_quit();
            }
            // Windows is told why it waits while the writer writes what is left.
            self.say_saving_to_windows();
            self.leave_at_once();
            self.stop_saying_saving_to_windows();
        }
        sp_winapi::session_end_done();
        el.exit();
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        if self.quitting.is_none() {
            self.begin_quit();
        }
        // Every file handed to the library's writer is written before the app goes (a big import
        // on a slow disk can take minutes), here in the event loop, so the window still answers.
        // Said once, after a moment. A settings change since the quit began is handed over too.
        self.paced.flush(&self.writer);
        // Whether the writer is idle is looked at before what it did is taken in: it hands over
        // what it did and goes idle at once (`writer::work`), so once it is idle all it did is
        // there to take, a save that failed as the last one ended included (kept in Not saved),
        // never missed between the two.
        let idle = self.writer.is_idle();
        self.finish_saves();
        if !idle {
            static SAVING_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            let waited = self.quitting.map_or(Duration::ZERO, |since| since.elapsed());
            if waited >= Duration::from_secs(1)
                && let Some(running) = self.running.as_mut()
                && !SAVING_SAID.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                let left = self.writer.left();
                applog::event("saving", &format!("waiting for {left} files to be written before quitting"));
                running.home_state.notices.info(crate::i18n::tn(
                    "notices.saving_before_quit",
                    left as u64,
                    "Saving {n} file\u{2026} SoftPresenter closes as soon as it is saved.",
                    "Saving {n} files\u{2026} SoftPresenter closes as soon as they are saved.",
                    &[],
                ));
                running.home_window.request_redraw();
            }
            return;
        }
        // A song the writer refused as it finished (its file changed outside meanwhile) is held
        // only now: it is kept aside too, so no change is lost on the way out.
        self.keep_held_aside();
        // A refresh of the Canva sign-in in flight is waited for (its new token must be kept),
        // here in the event loop, so the window still answers (and isn't taken for hung).
        if !self.canva.may_close(self.quitting.unwrap_or_else(Instant::now)) {
            let keeping = crate::i18n::t(
                "notices.canva.keeping",
                "Keeping the Canva sign-in… SoftPresenter closes as soon as it is kept.",
            );
            // Said once, though the loop comes back here until the sign-in is kept.
            static KEEPING_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if let Some(running) = self.running.as_mut()
                && self.canva.refreshing()
                && !KEEPING_SAID.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                running.home_state.notices.info(keeping);
                running.home_window.request_redraw();
            }
            return;
        }
        self.canva.shut_down();
        // Canva videos replaced while they played go now that nothing plays them.
        self.remove_replaced_canva_videos();
        // The program that receives AirPlay leaves with the app.
        self.mirror.quit();
        // The last anonymous report is given two seconds from when the quit began, here in the
        // event loop too (most often they have passed, or it has gone, while the saves were made).
        if !reports::quit_done() {
            return;
        }
        // FFmpeg's programs end with their players (let go without waiting, see
        // `sp_video::wait_for_players_let_go`): waited for here, briefly, so none outlives the app.
        wait_for_videos_let_go();
        // All done: nothing is left for `exiting` to do.
        self.left = true;
        el.exit();
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        // The step is let go before the work (locals drop last first), so it is counted in it.
        let _stall = stall::busy();
        let _starting = stall::inside(stall::Doing::Starting);
        match self.start(el) {
            Ok(Some(mut running)) => {
                // The media bin as it was left, read only by a start that opened the window: a
                // list of the displays, or a start that failed, never keeps its file aside
                // unsaid.
                self.fill_media_bin();
                // The Bible page and the small choices are as they were left.
                let (page, unread) = service::load_bible_page();
                self.unread.extend(unread);
                if let Some(page) = page {
                    running.home_state.bible.apply_settings(&page);
                }
                let (history, unread) = service::load_bible_history();
                self.unread.extend(unread);
                running.home_state.bible.history = history;
                // The language a session of the Bible view starts in, read once on a thread.
                running.home_state.bible.system_language.start();
                // The Bibles in use, read on a thread of their own, not as the Bible view opens.
                running.home_state.bible.read_ahead();
                self.bible_page_saved = running.home_state.bible.choices();
                let (prefs, unread) = service::load_prefs();
                self.unread.extend(unread);
                running.home_state.apply_prefs(&prefs);
                // The setup opens once on every Mac that hasn't seen it yet, a fresh install or
                // one set up before there was a setup. Testing aid: SOFTPRESENTER_SETUP=1 shows
                // it in a quiet run too, and SOFTPRESENTER_SETUP=<step> (free-songs, bibles…)
                // opens it at that step.
                if let Ok(step) = std::env::var("SOFTPRESENTER_SETUP") {
                    running.home_state.setup = Some(setup::Setup::opened_at(&step));
                } else if !prefs.setup_done && !self.quiet() {
                    running.home_state.setup = Some(setup::Setup::new());
                }
                // From before start-up was a macro: whether Settings said to start with the
                // audience screens off (moved into the start-up macro below).
                let audience_off_before = prefs.start_audience_off;
                // The screens, as they were left. The first start after screens could be
                // listed makes the list from how the output was set up before: one screen
                // placed automatically, and one sent over NDI if NDI was on.
                let quiet = self.quiet();
                let (screens, unread) = screens::load();
                self.unread.extend(unread);
                running.home_state.screens = screens.unwrap_or_else(|| {
                    let ndi = (prefs.ndi_on && running.home_state.ndi_available).then_some(prefs.ndi_name.as_str());
                    let made = screens::starting(ndi);
                    if !quiet && let Ok(save) = screens::to_save(&made) {
                        self.paced.save(&self.writer, save, Instant::now());
                    }
                    made
                });
                // A keyed DeckLink screen saved solid, before keying and the background were one
                // choice, sent a key that was all white: made see-through (a stage screen's
                // keyer turned off) and saved so, once.
                for said in screens::settle_keying(&mut running.home_state.screens) {
                    applog::event("screens", &said);
                    running.home_state.screens_changed = true;
                }
                let opening = prefs.start_with.clone();
                self.prefs_saved = Some(prefs);
                let (timers, unread) = timers::load();
                self.unread.extend(unread);
                running.home_state.timers = timers;
                let (messages, unread) = messages::load();
                self.unread.extend(unread);
                running.home_state.messages = messages;
                let (props, unread) = props::load();
                self.unread.extend(unread);
                running.home_state.props = props;
                let (macros, unread) = macros::load();
                self.unread.extend(unread);
                running.home_state.macros = macros;
                let (libraries, unread) = libraries::load();
                self.unread.extend(unread);
                running.home_state.libraries = libraries;
                running.home_state.media_playlists = std::mem::take(&mut self.media_playlists);
                let (stage_layouts, unread) = stage::load_layouts();
                self.unread.extend(unread);
                running.home_state.stage_layouts = stage_layouts;
                let (audio, unread) = audio::load();
                self.unread.extend(unread);
                running.home_state.audio = audio;
                // A settings file that couldn't be read was kept aside and the start went on with
                // the usual settings: said once, in the language just set.
                whole_file::tell(&mut running.home_state.notices, self.unread.drain(..));
                // Start-up belongs in macros: what Settings once said to play when the app opened
                // becomes a macro that plays it (and runs below, as it did), said once.
                // The macro is named in the notice as it is called.
                let macro_name = |macros: &[macros::Macro], id: Uuid| {
                    crate::i18n::quoted(macros.iter().find(|made| made.id == id).map_or("", |made| made.name.as_str()))
                };
                let made = macros::from_opening_choice(&mut running.home_state.macros, &opening);
                if let Some((step, id)) = &made {
                    let save = macros::to_save(&running.home_state.macros);
                    save_setting(&mut self.paced, &self.writer, &mut running.home_state.notices, save);
                    let what = describe_opening(&self.show, &running.home_state, step);
                    let mut said = crate::i18n::tf(
                        "notices.opening.macro",
                        "What played when SoftPresenter opened is now the macro {macro} in the Macros tab, with Run When SoftPresenter Opens ticked. It plays {what} on the Announcements layer.",
                        &[("macro", &macro_name(&running.home_state.macros, *id)), ("what", &what)],
                    );
                    said.push_str(crate::i18n::t(
                        "notices.opening.goes_off",
                        " It goes off when a slide goes on screen, until it is started again.",
                    ));
                    if matches!(opening, service::StartWith::MediaPlaylist { .. }) {
                        said.push_str(crate::i18n::t(
                            "notices.opening.was_behind",
                            " (The media playlist used to play behind the slides.)",
                        ));
                    }
                    applog::event("macros", &said);
                    running.home_state.notices.info(said);
                }
                let opening_macro = made.map(|(_, id)| id);
                if let Some(id) =
                    macros::from_audience_off(&mut running.home_state.macros, audience_off_before, opening_macro)
                {
                    let said = crate::i18n::tf(
                        "notices.opening.audience_off",
                        "Starting with the audience screens off is now the step Audience Screens Off in the macro {macro} in the Macros tab, run when SoftPresenter opens.",
                        &[("macro", &macro_name(&running.home_state.macros, id))],
                    );
                    let save = macros::to_save(&running.home_state.macros);
                    save_setting(&mut self.paced, &self.writer, &mut running.home_state.notices, save);
                    applog::event("macros", &said);
                    running.home_state.notices.info(said);
                }
                // A start-up macro that switches the audience screens off does that before any
                // screen draws (below), so they never show anything first.
                let start_off = macros::starts_with_audience_off(&running.home_state.macros);
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
                    first.go_to(screens::Output::Display(display.name));
                }
                // Testing aid: also send over NDI under this name, this run only.
                if let Ok(name) = std::env::var("SOFTPRESENTER_NDI") {
                    let screen = screens::Screen::new("NDI".to_owned(), screens::Output::Ndi(name));
                    running.home_state.screens.push(screen);
                    running.home_state.screen_selected = running.home_state.screens.len() - 1;
                }
                // What became of the library's folder at this start (moved to Documents, or why not).
                if let Some(said) = library_folder::take_notice() {
                    running.home_state.notices.outcome(said);
                }
                // The sp command, put where a terminal finds it once for each place the app is
                // installed in (never by a test copy), said once.
                if let Some(said) = cli_install::at_start(self.quiet()) {
                    applog::event("command line", &said);
                    running.home_state.notices.info(said);
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
                // On Windows, FFmpeg's own folder in the user's AppData is checked to hold the
                // app's FFmpeg, and copied there from beside the program or fetched where it
                // doesn't, on a thread of its own, wherever the program runs from (never by a
                // test copy).
                self.install_ffmpeg_at_start();
                if start_off {
                    self.apply(vec![Command::SetOutputEnabled(false)]);
                }
                self.sync_screens(el);
                // macOS says when the displays change (a display plugged out, say): the screens'
                // windows are dealt with before anyone sees them out of place.
                #[cfg(target_os = "macos")]
                if let Some(waker) = self.waker.clone()
                    && let Some(running) = self.running.as_mut()
                {
                    let waker = std::sync::Mutex::new(waker);
                    running.display_watch = Some(sp_video::DisplayWatch::start(move || {
                        if let Ok(waker) = waker.lock() {
                            let _ = waker.send_event(AppEvent::DisplaysChanged);
                        }
                    }));
                }
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
            // Nothing was started (a question answered, or a start that failed): there is nothing
            // for `exiting` to save or stop, and the run isn't one that ended.
            Ok(None) => {
                self.left = true;
                el.exit();
            }
            Err(message) => {
                eprintln!("{}", i18n::english_of(&message));
                // Graphics that can't draw: someone who opened the app from the Dock or the Start
                // menu sees no console, and without this the app would only be gone. Any other
                // failed start (a display asked for that isn't there, say) is said above only.
                if let Some(failure) = self.no_graphics.take().filter(|_| !self.quiet()) {
                    failure.show_window(no_graphics::System::here());
                }
                self.failed = true;
                self.left = true;
                el.exit();
            }
        }
    }

    fn new_events(&mut self, _el: &ActiveEventLoop, cause: winit::event::StartCause) {
        let by_clock = matches!(cause, winit::event::StartCause::ResumeTimeReached { .. });
        repaints::woke(by_clock);
        // A clock's time may change what pages are shown (a slide that moves on by itself, a
        // timer that ends); a wake for nothing (the system's own, which macOS gives several of a
        // second) changes nothing, and publishes nothing.
        if by_clock || matches!(cause, winit::event::StartCause::Init | winit::event::StartCause::Poll) {
            self.remote.stir();
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: AppEvent) {
        let _busy = reports::stats::BusyGuard::new(reports::stats::Who::Ui);
        let _stall = stall::busy();
        repaints::event(|| format!("the app was told {}", event.name()));
        self.remote.stir();
        let id = match event {
            AppEvent::Menu(id) => id,
            // A command from `softpresenter ctl`: done now, and its outcome sent back.
            AppEvent::Control(incoming) => {
                let _stall = stall::inside(stall::Doing::Command);
                // An AI assistant's, through `sp mcp`: checked against Settings › Integrations ›
                // AI assistants and `mcp::policy` first (which never lets a wait through).
                if incoming.request.mcp.is_some() {
                    self.mcp_control(el, incoming);
                    return;
                }
                // A wait for a change, with nothing changed yet, is held and answered later
                // (`answer_waiting`, from `about_to_wait`).
                let Some(incoming) = self.hold_wait(incoming) else { return };
                let control::Incoming { request, reply } = incoming;
                self.run_control(el, &request.args, None, |answer| {
                    let _ = reply.send(answer);
                });
                return;
            }
            AppEvent::Remote(asked) => {
                let _stall = stall::inside(stall::Doing::Command);
                self.remote_event(el, asked);
                return;
            }
            AppEvent::LivePreview => String::new(),
            AppEvent::DeckDone => {
                self.handle_decks();
                return;
            }
            AppEvent::BibleRead => {
                self.finish_bible_work();
                return;
            }
            AppEvent::PackDone => {
                self.finish_packs();
                return;
            }
            AppEvent::CatalogDone => {
                self.tick_catalog();
                return;
            }
            AppEvent::SongbookDone => {
                self.tick_free_songbooks();
                return;
            }
            AppEvent::FindSongs => {
                // The idle loop that follows asks again when it wants to be called.
                self.tick_find_songs();
                return;
            }
            AppEvent::Saved => {
                self.finish_saves();
                return;
            }
            AppEvent::UpdateChecked => {
                self.finish_update_check();
                return;
            }
            AppEvent::FfmpegFetch => {
                self.tick_ffmpeg_fetch();
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
            AppEvent::Dialog(answered) => {
                self.dialog_answered(answered);
                return;
            }
            // All of a drop's files have come: taken in now, not when the loop next waits.
            AppEvent::FilesDropped => {
                self.finish_drop("on the drop's own event");
                return;
            }
            // Files dragged in on a Wayland desktop: taken in as the window library's own words
            // about them are (`window_event`), all of a drop's files within this one event.
            #[cfg(target_os = "linux")]
            AppEvent::FilesDragged(told) => {
                if let Some(said) = file_drop::Told::wayland_line(&told) {
                    applog::event("drop", &said);
                }
                file_drop::Told::by_wayland(told).into_iter().for_each(|told| self.file_drag_told(told));
                return;
            }
            // Taken in when the loop next waits (`tick_canva`).
            AppEvent::Canva => return,
            // Taken in when the loop next waits (`tick_mirror`, `tick_web`).
            AppEvent::Mirror => return,
            AppEvent::Tidied(said) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.info(said);
                    running.home_window.request_redraw();
                }
                return;
            }
            // The system says the displays changed: looked at afresh at once (a main display or a
            // size that changed doesn't change the list of displays), so a screen's window is
            // hidden or put back before anyone sees it where it should not be.
            AppEvent::DisplaysChanged => {
                if let Some(running) = self.running.as_mut() {
                    running.displays_seen = None;
                    running.displays_look_at = Instant::now() + DISPLAYS_LOOK_EVERY;
                }
                self.sync_screens(el);
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
            let mut screens_only = false;
            if let Some(running) = self.running.as_ref()
                && let Some(output) = running.output.as_ref()
            {
                output.live_preview_woken();
                // Drawn while the window shows the Program preview. Where it doesn't (the Edit
                // view), a video's pictures no longer draw the whole window thirty times a second
                // for nothing; what the screens' thread may have woken it for besides (how many
                // watch an NDI source, a DeckLink output's state) still reaches the Screens
                // window, which `sync_screens` keeps up to date.
                if running.home_state.preview_painted {
                    running.home_window.request_redraw_for(|| repaints::Why::LivePreview);
                } else {
                    screens_only = true;
                }
                // Remote pages watching the preview are sent the new picture (read back on the
                // server's own thread, at most ten a second).
                if let Some(shared) = self.remote.shared() {
                    shared.preview_drawn(output.live_preview().1);
                }
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
            if screens_only {
                let said = self.running.as_ref().map(|running| running.home_state.notices.mark());
                self.sync_screens(el);
                // A notice it gave (a DeckLink output trying again) is shown at once.
                if let Some(running) = self.running.as_ref()
                    && Some(running.home_state.notices.mark()) != said
                {
                    running.home_window.request_redraw();
                }
            }
            return;
        }
        if let Some(action) = menubar::action_of(&id) {
            self.menu_action(el, action);
            #[cfg(target_os = "macos")]
            if let Some(menu) = self.menu.as_mut() {
                menu.forget_ticks();
            }
            self.sync_panels(el);
            for panel in self.running.iter().flat_map(|r| r.panels.iter()) {
                panel.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let _busy = reports::stats::BusyGuard::new(reports::stats::Who::Ui);
        let _stall = stall::busy();
        repaints::event(|| {
            let which = match self.running.as_ref() {
                Some(running) if running.home_window.id() == id => "the operator's window",
                Some(running) if running.screen_windows.iter().any(|have| have.window.id() == id) => {
                    "a screen's window"
                }
                _ => "another window",
            };
            format!("{which} told {}", repaints::event_name(&event))
        });
        self.remote.stir();
        reports::ui::window_event(&event);
        match &event {
            WindowEvent::ModifiersChanged(now) => self.held = now.state(),
            WindowEvent::Focused(true) => self.focused = Some(id),
            _ => {}
        }
        // Add a Key (Settings › Keys) is listening: the Settings window's next key is its own, by
        // its place on the keyboard, and reaches nothing else in that window. The menus' items
        // are greyed meanwhile (`tick_menu`), so a shortcut does nothing but get its sentence.
        // It listens only while Settings is open on Keys (`keys_card::listening`): a page
        // changed by a command ends it, though the card is never drawn again to say so.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && let Some(running) = self.running.as_mut()
            && home::settings::keys_card::listening(&running.home_state)
            && let Some(panel) =
                running.panels.iter().find(|panel| panel.window.id() == id && panel.kind == Panel::Settings)
        {
            if key.state == ElementState::Pressed && !key.repeat {
                home::settings::keys_card::key_pressed(&mut running.home_state, &Pressed::of(key, self.held));
            }
            panel.window.request_redraw();
            // The menus' items come back as it stops listening.
            self.tick_menu();
            return;
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
        // Add a Key stops listening when the Settings window loses the keyboard, and the menus'
        // items come back.
        if matches!(event, WindowEvent::Focused(false))
            && let Some(running) = self.running.as_mut()
            && running.home_state.keys_card.listening
            && let Some(panel) =
                running.panels.iter().find(|panel| panel.window.id() == id && panel.kind == Panel::Settings)
        {
            running.home_state.keys_card.stop();
            panel.window.request_redraw();
            self.tick_menu();
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
                // Its surface takes the new size before the next frame (`draw_panel`).
                WindowEvent::Resized(_) => {
                    panel.window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let size_before = self.running.as_ref().map(|running| running.home_state.interface_size);
                    let sheet_before = self.running.as_ref().is_some_and(|running| running.home_state.key_sheet);
                    let commands = self.draw_panel(id);
                    self.apply(commands);
                    // Add a Key (Settings › Keys) greys the menus' items while it listens.
                    self.tick_menu();
                    // A theme's end mark changed in its window, opened from Settings › Slides.
                    if self.running.as_ref().is_some_and(|running| running.home_state.theme_request.is_some()) {
                        self.handle_themes();
                    }
                    // Every key (Settings › Keys) opens the list of keys, which is the operator's
                    // window's: that window comes forward to show it.
                    let quiet = self.quiet();
                    if let Some(running) = self.running.as_ref()
                        && running.home_state.key_sheet
                        && !sheet_before
                    {
                        running.home_window.request_redraw();
                        if !quiet {
                            running.home_window.focus_window();
                        }
                    }
                    // Another Interface size chosen in Settings: the main window takes it at once.
                    if let Some(running) = self.running.as_ref()
                        && size_before != Some(running.home_state.interface_size)
                    {
                        running.home_window.request_redraw();
                    }
                    // A key in Settings that does what a menu item does (Check for updates) is
                    // carried out as the main window draws, as its own menus' are.
                    if let Some(running) = self.running.as_ref()
                        && running.home_state.menu_asked.is_some()
                    {
                        running.home_window.request_redraw();
                    }
                    // Settings may have asked for a songbook.
                    self.handle_songbooks();
                    // Or for a search for songs on this computer, or what it found.
                    self.handle_find_songs();
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
            let mut check_displays = false;
            match event {
                // Also how a display going is noticed at once on Windows: the system moves its
                // windows to the main display, which moves or resizes them.
                WindowEvent::Resized(size) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Resized(screen, size.width, size.height));
                    }
                    running.displays_seen = None;
                    check_displays = true;
                }
                WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    running.displays_seen = None;
                    check_displays = true;
                }
                // Closing a screen's window switches that screen off; it stays in the list.
                WindowEvent::CloseRequested => {
                    if let Some(closed) = running.home_state.screens.iter_mut().find(|have| have.id == screen) {
                        closed.on = false;
                    }
                    running.home_state.screens_changed = true;
                    running.home_window.request_redraw();
                }
                // Uncovered again: a frame skipped while the window was hidden must not leave old
                // words on screen. Do not also redraw on RedrawRequested: presenting a frame makes
                // the system ask for another, which loops without end.
                WindowEvent::Occluded(false) => {
                    applog::event(
                        "screens",
                        &format!("{}: the window is uncovered", screen_name(&running.home_state, screen)),
                    );
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Redraw);
                    }
                }
                // Said in the log, as uncovering is: while macOS counts a screen's window as
                // covered it shows none of its pictures, and the moment it changes is when a
                // window's next picture can be slow to come (see `output/held.rs`).
                WindowEvent::Occluded(true) => {
                    applog::event(
                        "screens",
                        &format!("{}: the window is covered", screen_name(&running.home_state, screen)),
                    );
                }
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                    let pressed = Pressed::of(&event, self.held);
                    if !self.held.super_key()
                        && !self.held.control_key()
                        && let Some(press) = show_key_command(
                            &running.home_state,
                            &running.egui_ctx,
                            &pressed,
                            event.repeat,
                            &self.show,
                            keys::f_keys(),
                        )
                    {
                        self.key_press(&pressed, press);
                    }
                }
                // The pointer over a window that fills a display shows its X (see
                // `close_button`); `tick_close_buttons` draws it as its clock runs.
                WindowEvent::CursorMoved { position, .. } => {
                    if let Some(have) = running.screen_windows.iter_mut().find(|have| have.window.id() == id)
                        && have.has_close_button()
                    {
                        let (w, scale) = (have.window.inner_size().width, have.window.scale_factor());
                        have.close.moved(Instant::now(), (position.x, position.y), w, scale);
                    }
                }
                WindowEvent::CursorLeft { .. } => {
                    if let Some(have) = running.screen_windows.iter_mut().find(|have| have.window.id() == id) {
                        have.close.left(Instant::now());
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: winit::event::MouseButton::Left,
                    ..
                } => {
                    let press = match running.screen_windows.iter_mut().find(|have| have.window.id() == id) {
                        Some(have) if have.has_close_button() => {
                            have.close.pressed(Instant::now(), close_button::SHOWS_ON_PRESS)
                        }
                        _ => close_button::Press::Nothing,
                    };
                    if press == close_button::Press::SwitchOff {
                        switch_off_by_its_x(running, screen);
                        check_displays = true;
                    }
                }
                _ => {}
            }
            if check_displays {
                self.sync_screens(el);
            }
            return;
        }
        if id != running.home_window.id() {
            return;
        }
        // A key pressed while what the window was given before it may change where it goes, and
        // is still waiting to be drawn: the window draws it first, here, within the event (as a
        // resize does), so the key goes where the keys before it left the keyboard
        // (`key_waits_for_drawing`).
        let mut waited = None;
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && key_waits_for_drawing(
                &running.egui_state.egui_input().events,
                running.home_state.typed_key.is_some(),
                &Pressed::of(key, self.held),
                key.repeat,
                &running.home_state.keys,
                &self.show,
                keys::f_keys(),
            )
        {
            let drawing = Instant::now();
            self.window_event(el, id, WindowEvent::RedrawRequested);
            waited = Some(drawing.elapsed());
        }
        // A key pressed: `/` opens the find box (not passed on, so the box opens empty), a key
        // that runs the show acts at once, Enter puts the slide picked on screen; anything else
        // is the window's (`home_key_goes`).
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && let Some(running) = self.running.as_mut()
        {
            let pressed = Pressed::of(key, self.held);
            let goes =
                home_key_goes(&running.home_state, &running.egui_ctx, &self.show, &pressed, key.repeat, keys::f_keys());
            match &goes {
                KeyGoes::Find => {
                    running.home_state.open_find();
                    running.home_window.request_redraw();
                }
                KeyGoes::Show(press) => self.key_press(&pressed, *press),
                KeyGoes::Typed(action) => self.typed_key_press(&pressed, *action),
                KeyGoes::Enter => self.enter_key(),
                KeyGoes::Window => {}
            }
            if log_keys() {
                self.log_key(key, waited, &goes);
            }
            if goes != KeyGoes::Window {
                return;
            }
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The interface toolkit answers "repaint" to a redraw request itself. Asking for a redraw
        // in reply to a redraw never ends: the window then redraws as fast as it can for ever,
        // which is what filled the computer's memory. A redraw request is not passed to it.
        if !matches!(event, WindowEvent::RedrawRequested)
            && running.egui_state.on_window_event(&running.home_window, &event).repaint
        {
            running.home_window.request_redraw_for(|| repaints::Why::Told(repaints::event_name(&event)));
        }
        match event {
            // Files dragged over the window from the Finder, each told one after another.
            WindowEvent::HoveredFile(path) => self.file_drag_told(file_drop::Told::Over(path)),
            WindowEvent::HoveredFileCancelled => self.file_drag_told(file_drop::Told::Gone),
            WindowEvent::DroppedFile(path) => self.file_drag_told(file_drop::Told::Dropped(path)),
            // Windows asks the window to close when the session is ending, and may end the
            // program once this returns: everything is saved and stopped now, waiting where it
            // must. The Restart Manager asks the same when an installer needs a file the app
            // holds: everything is saved, and the app stays open.
            WindowEvent::CloseRequested => match sp_winapi::take_windows_ask() {
                Some(sp_winapi::WindowsAsk::Ending) => self.leave_for_session_end(el),
                Some(sp_winapi::WindowsAsk::CloseApp) => self.save_for_close_app(),
                Some(sp_winapi::WindowsAsk::MayEnd) => self.save_as_session_may_end(),
                None => self.quit(el),
            },
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    running.home_resize = Some((size, Instant::now()));
                }
                running.home_window.request_redraw();
                // While macOS runs its resize loop it waits for the picture at the new size and
                // stretches the old one until it has it, so the frame is drawn here, within the
                // event, rather than at the next redraw. (Only when the drag is taken on live.)
                if !running.home_resize_held_back && size.width > 0 && size.height > 0 {
                    self.window_event(el, id, WindowEvent::RedrawRequested);
                }
            }
            // Testing aid: stop drawing the operator window after a few seconds, as macOS does
            // when it is minimised, to check that what runs by the clock goes on without it.
            WindowEvent::RedrawRequested
                if std::env::var("SOFTPRESENTER_STOP_DRAWING_AFTER")
                    .ok()
                    .and_then(|s| s.parse::<f32>().ok())
                    .is_some_and(|after| self.started.elapsed().as_secs_f32() >= after) => {}
            WindowEvent::RedrawRequested => {
                self.notices_testing_aid();
                let drawing = Instant::now();
                let commands = self.draw_home();
                self.home_timing.add(drawing.elapsed());
                repaints::drawn(drawing.elapsed());
                if log_keys() && drawing.elapsed() >= Duration::from_millis(40) {
                    applog::event("keys", &format!("the window took {} ms to draw", drawing.elapsed().as_millis()));
                }
                reports::stats::ui_frame(drawing.elapsed());
                reports::stats::phase(reports::stats::Who::Ui, reports::stats::UI_DRAWN);
                self.resize_testing_aid();
                // A command chosen from a right-click menu in the window, done as the menu bar's.
                if let Some(action) = self.running.as_mut().and_then(|r| r.home_state.menu_asked.take()) {
                    self.menu_action(el, action);
                }
                let thumbnails = self.running.as_ref().map(|running| running.thumbs.spare_bytes() >> 20);
                self.home_timing.report_each_minute("window", || {
                    // What the app holds, and what the painter of the thumbnails keeps ready
                    // between pictures; either may be missing, and then leaves no gap.
                    let held = thumbnails.map(|held| format!("thumbnails painter holds {held} MB"));
                    [applog::memory_line(), held].into_iter().flatten().collect::<Vec<_>>().join(", ")
                });
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
                // Testing aid: SOFTPRESENTER_EDIT=props or =stage opens the Edit view on the
                // props (with one made when there is none) or on the stage layouts, in any run,
                // so a test copy can be watched while it sits there.
                if frame == 8
                    && let Ok(kind) = std::env::var("SOFTPRESENTER_EDIT")
                    && let Some(running) = self.running.as_mut()
                {
                    let state = &mut running.home_state;
                    match kind.as_str() {
                        "props" => {
                            if state.props.is_empty() {
                                state.props.push(props::Prop::new("Logo".to_owned()));
                            }
                            state.edit_kind_request = Some(home::EditKind::Props);
                        }
                        "stage" => state.edit_kind_request = Some(home::EditKind::Stage),
                        _ => eprintln!("SOFTPRESENTER_EDIT is props or stage"),
                    }
                    running.home_window.request_redraw();
                }
                if let Some((at, slide)) = self.running.as_mut().and_then(|r| r.home_state.pending_edit.take()) {
                    self.apply_edit(at, slide);
                }
                self.run_actions();
                self.tick_advance();
                self.handle_audio();
                self.handle_video();
                self.handle_announcement_video();
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
                self.handle_bible_file_request();
                self.handle_song_request();
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
                self.handle_versions();
                self.handle_songs();
                self.handle_decks();
                self.handle_import_answer();
                self.handle_slides_choice();
                self.handle_setup();
                self.handle_songbooks();
                self.handle_free_songbooks();
                self.handle_catalog_songs();
                // Show Selected pressed in the find box or Scripture Lookup: once what it opened
                // is in the show (the song chosen, the passage in its place, a catalog song in
                // its own), Enter puts it on screen.
                if self.running.as_mut().is_some_and(|running| std::mem::take(&mut running.home_state.enter_after)) {
                    self.enter_key();
                }
                // The Bible page, the small choices and the playlists, as they change.
                self.hand_over_choices(false);
                if let Some(running) = self.running.as_mut() {
                    self.frames_since_start = self.frames_since_start.saturating_add(1);
                    // Until the playlists change, the first playlist of a first start is named in
                    // the language set.
                    if running.home_state.services.follow_language() {
                        running.home_window.request_redraw();
                    }
                }
                // Export…, from a song's or a playlist's right-click menu.
                if let Some(request) = self.running.as_mut().and_then(|r| r.home_state.export_request.take()) {
                    match request {
                        home::ExportRequest::Songs(songs) => self.export_from_window(&songs, None),
                        home::ExportRequest::Playlist(at) => self.export_from_window(&[], Some(at)),
                    }
                }
                self.finish_bible_jobs();
                self.finish_bible_work();
                // Reload or Keep mine, chosen in the bar for a file changed outside the app.
                if let Some((what, reload)) = self.running.as_mut().and_then(|r| r.home_state.waiting_choice.take()) {
                    self.choose_outside(what, reload);
                }
                self.update_canvas();
                self.sync_screens(el);
                self.sync_panels(el);
                self.sync_overlay();
                // Undo and Redo follow what the work above did to the history at once.
                self.note_show_history(true);
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

    /// On Linux, the windows (and with them their clipboards) go while the window system's
    /// connection is still open. Each window's clipboard has a thread of its own that holds
    /// Wayland objects from the event loop's connection and destroys them when the clipboard
    /// is dropped; dropped after `run_app` returns, the connection is gone and that thread
    /// crashed the process on every quit (exit 139).
    ///
    /// On every system this is also where an end that did not come through the app's own Quit is
    /// caught (see `leave_at_once`). Once the app has left (its Quit, or the system ending it), a
    /// settings change made since (an `sp` command answered on the way out) is still handed over
    /// and written, every time.
    fn exiting(&mut self, _el: &ActiveEventLoop) {
        if self.left {
            self.hand_over_choices(true);
            self.hand_over_lists(true);
            self.paced.flush(&self.writer);
            self.writer.drain();
            // What the writer did meanwhile: a song or theme whose save failed is kept in Not
            // saved (after the app's Quit), never left unread.
            self.finish_saves();
        } else {
            self.leave_at_once();
        }
        #[cfg(target_os = "linux")]
        drop(self.running.take());
        finish_replies();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let _busy = reports::stats::BusyGuard::new(reports::stats::Who::Ui);
        let _stall = stall::busy();
        self.reports_tick();
        // Testing aid: SOFTPRESENTER_TERMINATE_AFTER=<seconds> has macOS terminate the app that
        // many seconds after it started, the way the Dock's Quit, a log-out, a restart and a
        // shut-down do (never through the app's own Quit), so that what the app does then can
        // be tried without a pointer. On a Mac only; read once, and looked at each time the loop
        // waits (once a second at least).
        #[cfg(target_os = "macos")]
        if !self.terminate_asked && {
            static AFTER: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
            *AFTER.get_or_init(|| {
                std::env::var("SOFTPRESENTER_TERMINATE_AFTER").ok().and_then(|s| s.trim().parse::<f32>().ok())
            })
        }
        .is_some_and(|after| self.started.elapsed().as_secs_f32() >= after)
        {
            self.terminate_asked = true;
            applog::event("quit", "Testing aid: asking macOS to terminate the app, as the Dock's Quit does");
            sp_video::terminate_app();
        }
        // Testing aid: SOFTPRESENTER_END_SESSION_WHEN=<file> has the operator's window sent, once
        // that file exists, what Windows sends it at a log-off (may the session end, then that it
        // is ending), or, when the file says `closeapp`, what the Restart Manager sends when an
        // installer needs a file the app holds, from a thread of its own as Windows sends it, so
        // what the app does then can be tried without logging off or installing anything, at a
        // moment the test chooses however slowly the copy started. On Windows only, for a copy
        // with its own data folder; looked at each time the loop waits, and the loop wakes for it
        // four times a second until then.
        #[cfg(windows)]
        let end_session_at = {
            static WHEN: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
            let when = WHEN.get_or_init(|| {
                let own_folder = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
                std::env::var_os("SOFTPRESENTER_END_SESSION_WHEN").filter(|_| own_folder).map(PathBuf::from)
            });
            match when {
                Some(file) if !self.end_session_asked => {
                    if file.exists()
                        && let Some(window) =
                            self.running.as_ref().and_then(|running| win32_handle(&running.home_window))
                    {
                        self.end_session_asked = true;
                        let close_app = std::fs::read_to_string(file).is_ok_and(|said| said.trim() == "closeapp");
                        applog::event(
                            "quit",
                            if close_app {
                                "Testing aid: sending the window what the Restart Manager sends for an installer"
                            } else {
                                "Testing aid: sending the window what Windows sends at a log-off"
                            },
                        );
                        std::thread::spawn(move || sp_winapi::pretend_session_end(window, close_app));
                    }
                    Some(Instant::now() + Duration::from_millis(250))
                }
                _ => None,
            }
        };
        #[cfg(not(windows))]
        let end_session_at: Option<Instant> = None;
        // Quitting, waiting for the library's writer to write what it holds, or for Canva Sync
        // to keep a sign-in it is refreshing: looked at again shortly (their threads also wake
        // the loop when they are done), and nothing else is run.
        if self.quitting.is_some() {
            self.quit(el);
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100)));
            return;
        }
        // What the library's writer has written since (its stamps), and what it couldn't; once it
        // is idle, Windows is no longer told the app is saving.
        self.finish_saves();
        self.stop_saying_saving_to_windows();
        // Files dropped on the window, now that all of the drop have come (if the drop's own
        // event hasn't taken them in already).
        self.finish_drop("as the loop waited");
        // What runs by the clock is done here too, not only when the operator window draws:
        // macOS stops drawing a window that is minimised or covered, and a timed slide, a
        // slide's actions, or a message that leaves by itself must not wait for it.
        self.run_actions();
        self.tick_advance();
        self.say_if_newer_installed();
        if let Some(running) = self.running.as_ref() {
            log_late_placements(&running.home_state.displays);
        }
        self.sync_overlay();
        // What plays from a followed folder is kept going as its files come and go, before the
        // layers' own clocks move them on.
        let followed_at = self.tick_followed();
        let video_at = self.tick_video();
        let box_video_at = self.tick_slide_video();
        let web_at = self.tick_web();
        let voxlive_at = self.tick_voxlive();
        let canva_at = self.tick_canva();
        let catalog_at = self.tick_catalog();
        let songbooks_at = self.tick_free_songbooks();
        let find_at = self.tick_find_songs();
        let timelychurch_at = self.tick_timelychurch();
        let audio_at = self.tick_audio();
        let announce_at = self.tick_announcements();
        self.show_announcement_in_window();
        let cycle_at = self.tick_media_cycle();
        self.check_folders(false);
        let folders_at = self
            .running
            .as_ref()
            .filter(|r| r.home_state.media_playlists.iter().any(|p| p.folder.is_some()))
            .map(|_| self.folders_checked + self.folder_look_every());
        // The displays, looked at now and then: the screens' windows must never be left on the
        // operator's display by a change nobody was told of (the loop sleeps while idle, and
        // macOS doesn't draw a window that is covered, so nothing else would look).
        let looked_at = self.look_at_displays(el);
        // The X on the screens' windows, as it comes and goes.
        let close_at = self.tick_close_buttons();
        // The display kept awake while the screens are on: looked at now and then.
        let keep_awake_at = self.tick_keep_awake();
        // A Theme Editor left with the window not being drawn (minimised, covered), where the
        // drawing that would close it never comes: closed here, and its theme saved.
        if self.running.as_ref().is_some_and(|running| running.home_state.theme_editor_is_left()) {
            self.handle_themes();
        }
        // Songs changed in the Edit view are written once typing has paused for a second.
        const SAVE_AFTER: Duration = Duration::from_secs(1);
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        if save_at.is_some_and(|at| at <= Instant::now()) {
            self.save_unsaved();
        }
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        // Settings files whose saves wait for their time (`writer::Paced`) are handed over.
        let settings_at = self.paced.tick(&self.writer, Instant::now());
        // Songs and themes whose save failed, tried again once their wait is up.
        for due in self.retries.due(Instant::now(), false) {
            self.save_again(due);
        }
        let retry_at = self.retries.next();
        // `status --wait`s held: answered now if anything changed, or their time is up.
        let waiting_at = self.answer_waiting();
        // Remote Control: what Settings asked, the state for pages (when something has happened,
        // at most twenty times a second, only while one is connected) and the slides they asked
        // to see.
        if repaints::redraw_asked() {
            self.remote.stir();
        }
        let remote_at = self.tick_remote();
        // AirPlay: what Settings asked, what the receiving program said, and a still
        // screen's picture kept from fading.
        let mirror_at = self.tick_mirror();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The interface asks to be drawn again after a delay (animations); otherwise sleep
        // until then, or until a save is due.
        if running.next_repaint.is_some_and(|at| at <= Instant::now()) {
            running.next_repaint = None;
            running.home_window.request_redraw_for(|| repaints::Why::Timer(repaints::timer()));
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
        // Each clock by the name the log gives it (`repaints`).
        let wake = [
            (running.next_repaint, "the window's next drawing"),
            (panel_at, "a panel window's next drawing"),
            (thumbs_rest_at, "the thumbnails' painter resting"),
            (save_at, "saving"),
            (settings_at, "settings to save"),
            (retry_at, "saves tried again"),
            (followed_at, "followed folders"),
            (video_at, "the video"),
            (box_video_at, "videos in boxes"),
            (web_at, "web pages"),
            (voxlive_at, "VoxLive"),
            (canva_at, "Canva"),
            (catalog_at, "the song catalog"),
            (songbooks_at, "songbooks"),
            (find_at, "finding songs"),
            (timelychurch_at, "TimelyChurch"),
            (audio_at, "audio"),
            (announce_at, "announcements"),
            (cycle_at, "a media playlist"),
            (folders_at, "media folders"),
            (waiting_at, "status waits"),
            (remote_at, "Remote Control"),
            (mirror_at, "phone screens"),
            (looked_at, "the displays"),
            (close_at, "the screens' close buttons"),
            (end_session_at, "a testing aid's log-off"),
            (keep_awake_at, "keeping the display awake"),
        ];
        let soonest = wake.into_iter().filter_map(|(at, name)| at.map(|at| (at, name))).min_by_key(|(at, _)| *at);
        let now = Instant::now();
        repaints::sleeping(soonest.map(|(_, name)| name), soonest.is_some_and(|(at, _)| at <= now));
        match soonest {
            Some((at, _)) => {
                // Everything due has just been done, so a time already past is a mistake in
                // one of the clocks above; waiting for it would wake the loop again at once,
                // round and round at full speed. A short wait keeps that from running away.
                let at = if at <= now { now + Duration::from_millis(15) } else { at };
                el.set_control_flow(ControlFlow::WaitUntil(at));
            }
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// The app's icon, for its windows' title bars and the taskbar where the system takes it from
/// the window (a Mac takes it from the app bundle instead): on Windows the icon built into the
/// program, every size of it; on Linux a 256-pixel picture.
fn window_icon() -> Option<winit::window::Icon> {
    #[cfg(windows)]
    {
        use winit::platform::windows::IconExtWindows;
        // Resource 1: the icon build.rs puts into the program.
        winit::window::Icon::from_resource(1, None).ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let picture = image::load_from_memory_with_format(
            include_bytes!("../../../assets/icons/SoftPresenter-256.png"),
            image::ImageFormat::Png,
        )
        .ok()?
        .into_rgba8();
        let (width, height) = picture.dimensions();
        winit::window::Icon::from_rgba(picture.into_raw(), width, height).ok()
    }
    #[cfg(target_os = "macos")]
    {
        None
    }
}

/// What every window of the app shares: its icon, the taskbar's icon on Windows, and on Linux
/// the name a desktop entry is matched by (`softpresenter`, as `softpresenter.desktop`).
fn app_window(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let attrs = attrs.with_window_icon(window_icon());
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_taskbar_icon(window_icon())
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let attrs = {
        use winit::platform::wayland::WindowAttributesExtWayland;
        WindowAttributesExtWayland::with_name(attrs, "softpresenter", "softpresenter")
    };
    attrs
}

/// A graphics error nobody asked to catch, written in the log (a window program on Windows has no
/// console for it to go to). One that comes on every frame is written once, then once a minute
/// with how many times it came since, so it can't fill the log.
fn graphics_error(error: &str) {
    use std::sync::Mutex;
    static SAID: Mutex<Vec<(String, u32, Instant)>> = Mutex::new(Vec::new());
    eprintln!("Graphics error: {error}");
    let Ok(mut said) = SAID.lock() else { return };
    // Each kind is counted by itself: one frame the GPU refuses brings two errors (as it is put
    // together, and as it is handed in), which said turn about were both written every time.
    if let Some(again) = output::said_again(&mut said, error, Instant::now()) {
        let repeated = if again > 0 { format!(" ({again} more times in the last minute)") } else { String::new() };
        applog::problem("gpu", &format!("Graphics error{repeated}: {error}"));
    }
}

/// How long each step of starting took, said in one log line once the window is up, so a slow
/// start on someone's computer shows where the time went.
mod start_steps {
    use std::sync::Mutex;
    use std::time::Instant;

    static STEPS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());

    /// Notes that a step has just finished.
    pub fn mark(step: &'static str) {
        if let Ok(mut steps) = STEPS.lock() {
            steps.push((step, Instant::now()));
        }
    }

    /// Each step with how long it took after the one before, in milliseconds.
    pub fn said() -> String {
        let Ok(steps) = STEPS.lock() else { return String::new() };
        steps
            .windows(2)
            .map(|pair| format!("{} {} ms", pair[1].0, pair[1].1.duration_since(pair[0].1).as_millis()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A window's handle as Windows knows it (an `HWND`), for the few Windows calls made on it.
#[cfg(windows)]
fn win32_handle(window: &Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// A song or theme, by its place, found by its file (`App::whose_file`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Saved {
    Song(usize),
    Theme(usize),
}

/// Writes `bytes` into the library's "Not saved" folder as `<name> <date time>.<ending>` (or
/// `… 2.<ending>` and on, never over a file there), flushed to the disk. Where it went.
fn keep_in_not_saved(name: &str, ending: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let folder = bible_store::data_dir()
        .map(|dir| dir.join("Not saved"))
        .ok_or_else(|| std::io::Error::other("no library folder"))?;
    std::fs::create_dir_all(&folder)?;
    let when = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
    let base = format!("{} {when}", pack::safe_file_name(name));
    for n in 1..=99 {
        let more = if n == 1 { String::new() } else { format!(" {n}") };
        let file = folder.join(format!("{base}{more}.{ending}"));
        match whole_file::write_new(&file, bytes) {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("99 copies of it are there already"))
}

/// Hands a settings file's save to the library's writer, paced (`writer::Paced`), so the window
/// never waits for the disk; one that can't even be made (no place to keep it) is said.
fn save_setting(
    paced: &mut writer::Paced,
    writer: &writer::Writer,
    notices: &mut notices::Notices,
    save: Result<writer::Save, String>,
) {
    match save {
        Ok(save) => paced.save(writer, save, Instant::now()),
        Err(problem) => notices.trouble(problem),
    }
}

fn main() -> ExitCode {
    // `softpresenter ctl …` (or just `softpresenter next`, `softpresenter status`…) controls
    // the running app, and nothing else: no window, no log.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Anything typed after the program's name may print an answer (or a mistake): on Windows
    // that needs the console it was typed in, unless the output already goes to a file or pipe.
    if !args.is_empty() {
        sp_winapi::attach_parent_console();
    }
    match args.first().map(String::as_str) {
        Some("ctl") => return control::run_client(args[1..].to_vec()),
        // `softpresenter mcp status|connect|disconnect …`: adding SoftPresenter to Claude Desktop
        // or Claude Code, as the card's buttons do, answered here in English without the app.
        Some("mcp") if args.len() > 1 => return mcp::connect::run_cli(&args[1..]),
        // `softpresenter mcp` (`sp mcp`): the MCP server an AI assistant starts, talking to it
        // over the standard input and output and to the running app over the control socket.
        Some("mcp") => return mcp::serve(),
        // `softpresenter decklink-check …`: lists the DeckLink devices, and with `play` sends a
        // test picture to one, from a terminal, without the app.
        Some("decklink-check") => return sp_decklink::check::run(&args[1..], "softpresenter decklink-check"),
        // `softpresenter install-cli` and `remove-cli`: what Settings' Install and Remove do,
        // from a terminal, answered in English as the command line is.
        Some(verb @ ("install-cli" | "remove-cli")) => {
            let said = i18n::with_language(i18n::Language::English, || {
                if verb == "install-cli" {
                    cli_install::install().map(|done| cli_install::installed_words(&done, cfg!(windows)))
                } else {
                    cli_install::remove().map(|done| cli_install::removed_words(&done))
                }
            });
            return match said {
                Ok(said) => {
                    println!("{said}");
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
    // One copy per data folder (`one_copy`): a start that finds another copy running on this
    // folder asks that one to come to the front and ends here, having opened nothing and touched
    // nothing in the library. Taken after the renaming above, which making the hold's folder
    // first would stop, and before anything else; kept until the program ends, when the system
    // lets it go however it ends. A list of the displays only prints it and quits.
    let (_one_copy, one_copy_said) = if options.list_displays {
        (None, None)
    } else {
        match one_copy::at_start(options.capture_home.is_some(), !options.paths.is_empty()) {
            one_copy::Start::Go(held, said) => (held, said),
            one_copy::Start::End(code) => return code,
        }
    };
    // On a Mac the library moves to Documents, once. Songs named by path or title in files from
    // before songs were found by id are named by id, once, in whichever library folder is in use.
    // Neither for a list of the displays, which isn't held and may run beside the copy running.
    let (to_documents, settled) = if options.list_displays {
        (Vec::new(), Vec::new())
    } else {
        let to_documents = library_folder::move_to_documents();
        (to_documents, bible_store::data_dir().map(|dir| song_ids::settle(&dir, true)).unwrap_or_default())
    };
    applog::start();
    // Says in the log how the last run ended if it didn't quit, and notes this one until the app
    // has quit (any return from here on). A list of the displays, which may run beside the copy
    // running on this folder, leaves that copy's note alone.
    let _last_run = (!options.list_displays).then(applog::last_run::begin);
    start_steps::mark("log opened");
    if let Some(said) = &one_copy_said {
        applog::event("start", said);
    }
    reports::start(options.capture_home.is_some());
    // Whether this copy keeps the videos' FFmpeg in a folder of its own (Windows), worked out
    // before anything looks for FFmpeg.
    ffmpeg_fetch::prepare(options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some());
    for done in &moved {
        applog::event("renamed", done);
    }
    for done in &to_documents {
        applog::event("library", done);
    }
    for done in &settled {
        applog::event("song ids", done);
    }
    // NDI downloaded in the app is kept in its own folder, looked in for NDI's runtime. Before
    // it is looked for: a new one unpacked while the old was in use takes its place, and a
    // download a quit left part of the way through is deleted.
    // (Not on Windows, where NDI's own installer puts NDI in its own place: nothing of the
    // app's is ever in that folder there, so nothing is looked for in it.)
    if !ndi_download::way().installer()
        && let Some(folder) = ndi_download::folder()
    {
        ndi_download::settle(&folder);
        sp_ndi::set_own_folder(folder);
    }
    // Picture objects on slides are read from the app's Images folder.
    sp_render::set_picture_loader(images::load);
    sp_render::set_small_picture_loader(images::load_at);
    // Large font files are mapped from private copies in the app's caches folder.
    sp_render::copy_large_fonts_into_caches();
    // The operator's window draws a placeholder for a web page box with no picture yet; the
    // screens, on their own threads, never do.
    sp_render::show_placeholders(true);
    // A web page box whose page costs to open (a VoxLive screen's live display) shows its free
    // test pattern in the operator's window instead.
    sp_render::set_web_stand_in(web_stand_in);
    sp_render::set_ndi_tag_words(ndi_in::tag_words);

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
            catalog: None,
        });
        sources.push(None);
    }
    // One more after them holds the VoxLive item: the church's VoxLive screens, a slide each,
    // made from what VoxLive says and kept in step with it. It has no file.
    let voxlive_slot = songs.len();
    songs.push(voxlive::item(&[]));
    sources.push(None);
    // Two more hold a song of the song catalog while it is looked at, before it joins the library
    // (if it ever does). They have no file.
    let catalog_slots = [songs.len(), songs.len() + 1];
    for _ in catalog_slots {
        songs.push(sp_model::Presentation {
            id: sp_model::Uuid::new_v4(),
            title: catalog::SOURCE_NAME.to_owned(),
            groups: Vec::new(),
            copyright: sp_model::Copyright::default(),
            theme: None,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
            catalog: None,
        });
        sources.push(None);
    }

    start_steps::mark("songs read");
    note_program_file();
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
    start_steps::mark("event loop made");
    let (mut themes, songs_theme, moved_in) = theme_store::load_all();
    start_steps::mark("themes read");
    // Testing aid: SOFTPRESENTER_END_LOOK gives the main theme a look for the end of a song's
    // mark for this run (`end_look::asked_for_the_run`).
    if let (Some(look), Some(main)) = (end_look::asked_for_the_run(), themes.get_mut(songs_theme)) {
        main.end_look = look;
    }
    let (bible_theme, bible_layout) = (songs_theme, themes[songs_theme].bible_template_index());
    // How each song and theme file is now, as it was just read.
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
    let (bible_tx, bible_rx) = std::sync::mpsc::channel();
    start_steps::mark("files noted");
    let remote = remote::Remote::open();
    start_steps::mark("remote control read");
    // Commands from `softpresenter ctl` come in on the control socket. Without it (a copy on
    // another data folder named the same socket, say) the app runs as usual. It is set up on a thread of its own, so the window never
    // waits for it (keeping its folder private once ran Windows' own tools, which took up to a
    // minute on a busy PC); commands are answered a moment after it opens.
    let control: std::sync::Arc<std::sync::Mutex<Option<control::Listening>>> = std::sync::Arc::default();
    {
        let kept = control.clone();
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        let set_up = std::thread::Builder::new().name("control setup".to_owned()).spawn(move || {
            let started = Instant::now();
            match control::listen(move |incoming| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(AppEvent::Control(incoming));
                }
            }) {
                Ok(listening) => {
                    applog::event(
                        "control",
                        &format!("listening for commands after {} ms", started.elapsed().as_millis()),
                    );
                    if let Ok(mut kept) = kept.lock() {
                        *kept = Some(listening);
                    }
                }
                Err(problem) => applog::event("control", &problem),
            }
        });
        if let Err(problem) = set_up {
            applog::event("control", &format!("Could not start listening for commands: {problem}"));
        }
    }
    start_steps::mark("command socket started");
    let show = Show::new(songs.into_iter().map(Arc::new).collect());
    start_steps::mark("show made");
    // A picture read on a reader's thread wakes the window's thread to take it in.
    let bin_pictures = {
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        bin_pictures::BinPictures::new(move || {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::MediaRead);
            }
        })
    };
    start_steps::mark("picture readers started");
    let voxlive = voxlive::Client::new();
    start_steps::mark("voxlive client made");
    let canva = {
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::Canva);
            }
        });
        canva::Client::new(canva::Places::for_app(), Some(wake))
    };
    start_steps::mark("canva client made");
    // A file window's answer comes back through the event loop.
    let dialogs = {
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        let deliver: dialogs::Deliver = Arc::new(move |answered| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::Dialog(answered));
            }
        });
        dialogs::Dialogs::new(Box::new(dialogs::System::default()), deliver)
    };
    // A run that only saves a picture, or is asked to stay out of the way, never announces
    // itself to phones (the same test as `App::quiet`, made before `options` is given away).
    let quiet_run = options.capture_home.is_some() || std::env::var_os("SOFTPRESENTER_QUIET").is_some();
    let mut app = App {
        options,
        waker: Some(event_loop.create_proxy()),
        pack_tx,
        pack_rx,
        bible_tx,
        bible_rx,
        jobs: Vec::new(),
        last_job: None,
        run_word: Uuid::new_v4().simple().to_string(),
        import_library_override: None,
        dropped: Vec::new(),
        drop_pointer: None,
        drop_came: None,
        dialogs,
        bible_jobs: Vec::new(),
        update_jobs: Vec::new(),
        ffmpeg_fetch: ffmpeg_fetch::Told::default(),
        waiting: Vec::new(),
        mirror: mirror::Mirror::open(quiet_run),
        keep_awake: keep_awake::KeepAwake::for_this_run(quiet_run),
        remote,
        remote_started: false,
        remote_pages_seen: 0,
        remote_call: false,
        mcp_call: false,
        mcp_jobs: mcp::Jobs::default(),
        _control: control,
        show,
        last_sung: std::cell::Cell::new(None),
        sources,
        bible_slots,
        voxlive_slot,
        catalog_slots,
        voxlive_item_from: None,
        version_seen: None,
        decks: deck::Decks::default(),
        decks_resumed: false,
        bin_pictures,
        bin_started: None,
        added_by_hand: std::collections::HashSet::new(),
        unsaved: Vec::new(),
        history: ShowHistory::default(),
        edit_song: None,
        edit_mark: 0,
        action_depth: None,
        live_since: None,
        advanced: false,
        started: Instant::now(),
        video: None,
        video_leaving: None,
        media_on_screens: (None, sp_model::ImageFit::Fill),
        video_wait: None,
        slide_videos: Vec::new(),
        slide_video_refused: std::collections::HashSet::new(),
        web_pages: Vec::new(),
        web_refused: Vec::new(),
        web_traced: None,
        ndi: ndi_in::Inputs::default(),
        ndi_seen: 0,
        ndi_drawn: None,
        voxlive,
        canva,
        catalog: catalog_cli::CatalogState::default(),
        free_songbooks: free_songbooks_cli::Installs::default(),
        finding: find_songs_cli::Finding::default(),
        timelychurch: timelychurch::Connection::new(timelychurch::gate(), timelychurch::Files::in_data_dir()),
        voxlive_preview: None,
        media_playlists: Vec::new(),
        unread: Vec::new(),
        paced: writer::Paced::default(),
        announcing: None,
        stamps,
        writer: {
            let proxy = event_loop.create_proxy();
            writer::Writer::start(move || {
                let _ = proxy.send_event(AppEvent::Saved);
            })
        },
        held_songs: Vec::new(),
        held_themes: Vec::new(),
        kept_aside: (Vec::new(), Vec::new()),
        retries: writer::Retries::default(),
        kept_failed: Vec::new(),
        media_cycle: None,
        folder_notes: media::FolderNotes::default(),
        folders_checked: Instant::now(),
        folder_look: None,
        look_again: false,
        followed_items: std::collections::HashSet::new(),
        following: Default::default(),
        lined_up_wait: Default::default(),
        video_refused: None,
        skipped_videos: HashMap::new(),
        cycle_failed: 0,
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
        media_named: media_paths,
        capture_selects_box: false,
        focused: None,
        #[cfg(target_os = "macos")]
        menu: None,
        running: None,
        failed: false,
        no_graphics: None,
        capture_done: false,
        home_timing: applog::Timing::new("operator window drawings"),
        quitting: None,
        #[cfg(target_os = "macos")]
        terminate_asked: false,
        #[cfg(windows)]
        saying_saving: false,
        #[cfg(windows)]
        end_session_asked: false,
        left: false,
    };
    // Themes changed while loading (a Bible layout brought into the main theme) are handed to
    // the library's writer at once, with the new note of what is in use, so it happens only
    // once; never by a list of the displays, which may run beside the copy running.
    if !moved_in.is_empty() && !app.options.list_displays {
        for at in moved_in {
            app.write_theme(at, false);
        }
        app.remember_themes_in_use();
    }
    start_steps::mark("app made");
    // The log says when the window's thread is held up, and with what.
    stall::watch();
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

    #[test]
    fn the_logs_line_for_an_import_counts_songs_and_files_each_in_its_own_words() {
        assert_eq!(
            super::imported_line(3095, 1, 170, 0, 0, 0),
            "imported 3095 songs from 1 file in 170 ms (0 files not imported)"
        );
        assert_eq!(
            super::imported_line(1, 3, 12, 1, 0, 0),
            "imported 1 song from 3 files in 12 ms (1 file not imported)"
        );
        assert_eq!(
            super::imported_line(0, 2, 40, 0, 3095, 1),
            "imported 0 songs from 2 files in 40 ms (0 files not imported, 3095 songs already in the library, 1 file still being read)"
        );
        assert_eq!(
            super::imported_line(4, 1, 9, 0, 1, 0),
            "imported 4 songs from 1 file in 9 ms (0 files not imported, 1 song already in the library)"
        );
    }

    use super::*;

    /// A key pressed as on a US keyboard, with nothing held (`Pressed::from`).
    fn us(key: &Key) -> Pressed {
        Pressed::from(key.clone())
    }

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
        assert!(enter_key_is_free(&state, &ctx), "the Service view");
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
        assert!(matches!(letter_command(&s, &show), Some(Command::GoTo(_))));
        // ...so in the Bible view it is held back from the show, for Scripture Lookup.
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        assert!(show_key_held_back(&state, &ctx, &us(&s), true));
        assert!(show_key_held_back(&state, &ctx, &us(&Key::Character("2".into())), true));
        assert!(
            !show_key_held_back(&state, &ctx, &us(&Key::Named(NamedKey::Space)), true),
            "Space still runs the show"
        );
        assert!(!show_key_held_back(&state, &ctx, &us(&Key::Named(NamedKey::Escape)), true), "and Escape clears");
        // Home and End go to the first and last slide there too, as before the table.
        for (key, action) in [(NamedKey::Home, KeyAction::First), (NamedKey::End, KeyAction::Last)] {
            let press = home_key_command(&state, &ctx, &us(&Key::Named(key)), false, &show, false);
            assert_eq!(press, Some(KeyPress::Action(action)), "{key:?} in the Bible view");
        }
        state.bible_open = false;
        assert!(!show_key_held_back(&state, &ctx, &us(&s), true), "the Service view keeps its group letters");
    }

    #[test]
    fn with_the_notices_open_escape_closes_them_and_the_show_keys_still_run() {
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("One", "Words\n"))]);
        let mut state = home::HomeState::default();
        state.notices.trouble("Couldn't save the screens.");
        let mut frame = |events: Vec<egui::Event>, time: f64| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                time: Some(time),
                events,
                ..egui::RawInput::default()
            };
            let theme = sp_model::Theme::builtin();
            let output = ctx.run_ui(raw, |ui| {
                home::home_ui(ui, &show, &mut state, &theme, &mut |_| None, home::Preview::default(), None);
            });
            output.drop_without_applying_deltas();
        };
        frame(Vec::new(), 0.1);
        let at = ctx.read_response(notices::button_id()).expect("the status button").rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at)], 0.2);
        frame(vec![press(true)], 0.3);
        frame(vec![press(false)], 0.4);
        assert!(notices::list_open(&ctx));
        let state = home::HomeState::default();
        assert!(escape_closes_something(&state, &ctx), "Escape closes the list instead of clearing the screens");
        for key in [NamedKey::Space, NamedKey::ArrowRight, NamedKey::ArrowLeft] {
            assert!(!show_key_held_back(&state, &ctx, &us(&Key::Named(key)), true), "{key:?} runs the show");
        }
        assert!(enter_key_is_free(&state, &ctx), "and so does Enter");
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
    fn the_empty_props_list_keeps_the_keyboard_from_running_the_show() {
        let ctx = egui::Context::default();
        let mut state = home::HomeState { props_editor_open: true, ..home::HomeState::default() };
        assert!(state.editing.is_none());
        assert_eq!(state.edit_kind(), Some(home::EditKind::Props), "it is the Edit view");
        assert!(!find_key_is_free(&state, &ctx) && !enter_key_is_free(&state, &ctx));
        state.props_editor_open = false;
        assert!(find_key_is_free(&state, &ctx) && enter_key_is_free(&state, &ctx));
    }

    #[test]
    fn a_macro_can_stop_the_audio_and_take_the_announcements_off() {
        use sp_model::{ClearWhat, ShowAction};
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("Song", "One\n"))]);
        let state = home::HomeState::default();
        // The same commands as the Audio and Announce keys and the Presentation menu's Clear
        // Audio and Clear Announcements, which `apply` stops the audio and takes the layer off for.
        let clear = |what| action_commands(&show, &state, &ShowAction::Clear { what });
        assert_eq!(clear(ClearWhat::Audio), Some(vec![Command::Clear(sp_engine::Layer::Audio)]));
        assert_eq!(clear(ClearWhat::Announcements), Some(vec![Command::Clear(sp_engine::Layer::Announcements)]));
        // Kept in a macro's file by their own words.
        let step = ShowAction::Clear { what: ClearWhat::Announcements };
        let text = serde_json::to_string(&step).unwrap();
        assert!(text.contains("\"announcements\""), "{text}");
        assert_eq!(serde_json::from_str::<ShowAction>(&text).unwrap(), step);
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
    fn what_was_deleted_takes_its_turn_in_the_one_history() {
        let a = Arc::new(sp_model::parse_song_text("A", "One\n"));
        let removed = removed::Removed::Items(removed::RemovedItems {
            playlist: 0,
            name: "Sunday".to_owned(),
            items: vec![(1, service::Item::Header { name: "Word".to_owned() })],
        });
        let timer = removed::Removed::Timer { at: 0, timer: timers::Timer::new("Countdown".to_owned()) };
        let mut history = ShowHistory::default();
        history.remember(0, a.clone());
        history.remember_removed(removed.clone());
        history.remember_removed(timer.clone());
        // The timer, deleted last, is put back first; then the items; then the song's change
        // is undone.
        assert!(history.removed_next(false));
        assert_eq!((history.peek(false), history.step(false, a.clone())), (None, None), "not a song's step");
        assert_eq!(history.step_removed(false), Some(timer.clone()));
        assert_eq!(history.step_removed(false), Some(removed.clone()));
        assert_eq!(history.peek(false), Some(0));
        history.step(false, a.clone());
        // Redone in the order they were made: the song, then the items, then the timer.
        assert_eq!(history.peek(true), Some(0));
        history.step(true, a.clone());
        assert!(history.removed_next(true));
        assert_eq!(history.step_removed(true), Some(removed));
        assert_eq!(history.step_removed(true), Some(timer));
        // Forgetting a song's steps leaves the deletes' steps.
        history.forget(0);
        assert!(history.removed_next(false));
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
        std::fs::write(load::song_file_for(std::path::Path::new(&path)), sp_model::to_json(&song)).unwrap();
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
        assert_eq!(letter_command(&slash, &show), None);
        assert_eq!(show_key_press(&us(&slash), &KeyTable::default(), &show, true), None);
        // Other keys are as they were: a hot key still goes to its slide, and is not find.
        let k = Key::Character("k".into());
        assert!(!opens_find(&k) && !opens_find(&Key::Named(NamedKey::Space)));
        assert_eq!(letter_command(&k, &show), Some(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 })));
        // It is free in the Service view, and not while the Edit view is open or a sheet is up.
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
        assert!(!show_key_held_back(&state, &ctx, &us(&space), true), "nothing open: Space runs the show");
        assert!(!show_key_held_back(&state, &ctx, &us(&digit), true), "a digit is a slide's hot key");
        state.numbers = Some(keys::Numbers { of: keys::NumbersOf::Tab, since: 0.0, picked: None });
        assert!(show_key_held_back(&state, &ctx, &us(&digit), true), "while the numbers show, a digit picks");
        assert!(show_key_held_back(&state, &ctx, &us(&escape), true), "and Escape only puts them away");
        assert!(!show_key_held_back(&state, &ctx, &us(&space), true), "Space still runs the show");
        state.numbers = None;
        state.key_sheet = true;
        assert!(show_key_held_back(&state, &ctx, &us(&space), true), "nothing runs the show under the list of keys");
    }

    #[test]
    fn in_the_edit_view_only_the_keys_that_clear_reach_the_show_and_escape_edits_first() {
        use KeyAction as A;
        let song = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        let ctx = egui::Context::default();
        let named = Key::Named;
        let key =
            |state: &home::HomeState, key: Key, f_keys| home_key_command(state, &ctx, &us(&key), false, &show, f_keys);
        let editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        let mut state = home::HomeState { editing, ..home::HomeState::default() };
        assert!(state.editing.is_some());
        let does = |action| Some(KeyPress::Action(action));
        // Off a Mac the F keys clear while editing, as in the Service view.
        for (f, action) in [
            (NamedKey::F1, A::ClearAll),
            (NamedKey::F2, A::ClearSlide),
            (NamedKey::F3, A::ClearMedia),
            (NamedKey::F4, A::ClearProps),
            (NamedKey::F5, A::ClearAudio),
            (NamedKey::F6, A::ClearMessages),
        ] {
            assert_eq!(key(&state, named(f), true), does(action), "{f:?}");
            assert_eq!(key(&state, named(f), false), None, "{f:?} on a Mac is the system's");
        }
        // With nothing picked, typed or open, Escape clears everything, on every computer.
        assert_eq!(key(&state, named(NamedKey::Escape), true), does(A::ClearAll));
        assert_eq!(key(&state, named(NamedKey::Escape), false), does(A::ClearAll));
        // The keys that run the slides stay the editor's: they never put a slide up from here.
        for other in [
            named(NamedKey::Space),
            named(NamedKey::ArrowRight),
            named(NamedKey::ArrowLeft),
            named(NamedKey::Home),
            named(NamedKey::End),
            named(NamedKey::Enter),
            Key::Character("c".into()),
            Key::Character("1".into()),
        ] {
            assert_eq!(key(&state, other.clone(), true), None, "{other:?} is the editor's");
        }
        // Escape has an editing meaning first, and then it clears nothing.
        let escape = |state: &home::HomeState| key(state, named(NamedKey::Escape), false);
        let first = state.editing.as_ref().and_then(|draft| draft.slide.objects.first().map(|object| object.id));
        assert!(first.is_some());
        let edit = |state: &mut home::HomeState, change: &dyn Fn(&mut EditDraft)| {
            if let Some(draft) = state.editing.as_mut() {
                change(draft);
            }
        };
        edit(&mut state, &|draft| draft.selected = first);
        assert_eq!(escape(&state), None, "an object picked: Escape lets go of it");
        assert_eq!(key(&state, named(NamedKey::F2), true), does(A::ClearSlide), "F2 still clears");
        edit(&mut state, &|draft| draft.typing = first);
        assert_eq!(escape(&state), None, "words being typed: Escape puts them back");
        edit(&mut state, &|draft| draft.selected = None);
        assert_eq!(escape(&state), None, "still typing, though nothing is picked");
        edit(&mut state, &|draft| draft.typing = None);
        edit(&mut state, &|draft| draft.renaming = Some((sp_model::Uuid::nil(), "Main".to_owned())));
        assert_eq!(escape(&state), None, "an object's name being retyped");
        edit(&mut state, &|draft| draft.renaming = None);
        state.stage_rename = Some((0, "Words".to_owned()));
        assert_eq!(escape(&state), None, "a stage layout's name being retyped");
        state.stage_rename = None;
        state.version_sheet = Some(home::VersionSheet::adding(&state));
        assert_eq!(escape(&state), None, "the sheet that adds a theme version");
        state.version_sheet = None;
        egui::Popup::open_id(&ctx, egui::Id::unique("a menu"));
        assert_eq!(escape(&state), None, "a menu open over the editor");
        assert_eq!(key(&state, named(NamedKey::F3), true), does(A::ClearMedia), "F3 still clears");
        egui::Popup::close_all(&ctx);
        assert_eq!(escape(&state), does(A::ClearAll), "nothing left for it to do");
        // Held down, Escape repeats: the repeats never clear in the Edit view (the first press
        // may have let go of a box, or put back words typed), and the F keys still do.
        let held = |key: NamedKey| home_key_command(&state, &ctx, &us(&Key::Named(key)), true, &show, true);
        assert_eq!(held(NamedKey::Escape), None, "a held Escape");
        assert_eq!(held(NamedKey::F2), does(A::ClearSlide), "a held F2");
        let service = home::HomeState::default();
        let held_in_service = home_key_command(&service, &ctx, &us(&Key::Named(NamedKey::Escape)), true, &show, false);
        assert_eq!(held_in_service, does(A::ClearAll), "the Service view as before");
        // A text box with the keyboard (an inspector field, a name) keeps every key, F keys too.
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
        frame(true);
        frame(true);
        assert_eq!(escape(&state), None, "a field has the keyboard");
        assert_eq!(key(&state, named(NamedKey::F2), true), None, "and the F keys wait, as in the Service view");
        ctx.memory_mut(|memory| memory.stop_text_input());
        frame(false);
        assert_eq!(escape(&state), does(A::ClearAll));
        // A drag going on (a slide being moved in the list, a box dragged round objects).
        let press = |pressed: bool| {
            let event = egui::Event::PointerButton {
                pos: egui::pos2(10.0, 10.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let raw = egui::RawInput {
                events: vec![egui::Event::PointerMoved(egui::pos2(10.0, 10.0)), event],
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |_| {}).drop_without_applying_deltas();
        };
        press(true);
        assert_eq!(escape(&state), None, "the pointer is held down: Escape stops the drag");
        press(false);
        assert_eq!(escape(&state), does(A::ClearAll));
        // Every kind of the Edit view is the same: the themes, the props, the stage layouts.
        let mut props = home::HomeState { props_editor_open: true, ..home::HomeState::default() };
        assert_eq!(key(&props, named(NamedKey::Escape), false), does(A::ClearAll), "the props, none yet");
        assert_eq!(key(&props, named(NamedKey::Space), false), None);
        props.props_editor_open = false;
        props.stage_editor_open = true;
        assert_eq!(key(&props, named(NamedKey::F4), true), does(A::ClearProps), "the stage layouts");
        // And the Service view runs the show with every key, as before.
        let service = home::HomeState::default();
        assert_eq!(key(&service, named(NamedKey::Space), false), does(A::Next));
        assert_eq!(key(&service, named(NamedKey::Escape), false), does(A::ClearAll));
        // With the keys changed (Settings › Keys), Escape and the F keys still reach the show from
        // here only when they clear or hide the words; Space and the arrows stay the editor's,
        // whatever they are set to.
        let mut changed = home::HomeState { editing: state.editing.clone(), ..home::HomeState::default() };
        changed.keys.set_style(key_table::KeyStyle::SoftProjector);
        changed.keys.set(ShowKey::F3, A::ClearMedia);
        changed.keys.set(ShowKey::Space, A::ClearAll);
        changed.keys.set(ShowKey::Right, A::HideWords);
        changed.keys.set(ShowKey::Home, A::ClearSlide);
        assert!(changed.editing.is_some());
        assert_eq!(key(&changed, named(NamedKey::Escape), false), does(A::HideWords), "Hide the Words is a clear");
        assert_eq!(key(&changed, named(NamedKey::F1), true), does(A::ClearAll));
        assert_eq!(key(&changed, named(NamedKey::F3), true), does(A::ClearMedia), "a changed F row");
        for f in [NamedKey::F2, NamedKey::F4, NamedKey::F5, NamedKey::F6, NamedKey::F7] {
            assert_eq!(key(&changed, named(f), true), None, "{f:?}: the playlist's, the views' and Show Again wait");
        }
        for other in [NamedKey::Space, NamedKey::ArrowRight, NamedKey::Home] {
            assert_eq!(key(&changed, named(other), false), None, "{other:?} nudges, set to a clear or not");
        }
        changed.keys.set(ShowKey::Escape, A::Next);
        assert_eq!(key(&changed, named(NamedKey::Escape), false), None, "Escape set to Next does nothing here");
        let service = home::HomeState { keys: changed.keys.clone(), ..home::HomeState::default() };
        assert_eq!(key(&service, named(NamedKey::Escape), false), does(A::Next), "it runs the show elsewhere");
        assert_eq!(key(&service, named(NamedKey::Space), false), does(A::ClearAll));
    }

    #[test]
    fn show_keys() {
        use KeyAction as A;
        use NamedKey as N;
        let song = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n\nThree\n\n[Verse 2]\nFour\n");
        let show = Show::new(vec![Arc::new(song)]);
        let softpresenter = KeyTable::default();
        let mut softprojector = KeyTable::default();
        softprojector.set_style(key_table::KeyStyle::SoftProjector);
        let action = |table: &KeyTable, key: N, f_keys: bool| show_key_action(&us(&Key::Named(key)), table, f_keys);
        // Both styles run the slides alike, on every computer.
        for table in [&softpresenter, &softprojector] {
            for f_keys in [true, false] {
                assert_eq!(action(table, N::ArrowRight, f_keys), Some(A::Next));
                assert_eq!(action(table, N::Space, f_keys), Some(A::Next));
                assert_eq!(action(table, N::ArrowLeft, f_keys), Some(A::Previous));
                assert_eq!(action(table, N::Home, f_keys), Some(A::First));
                assert_eq!(action(table, N::End, f_keys), Some(A::Last));
            }
        }
        // Escape clears everything in SoftPresenter's style and hides the words in SoftProjector's,
        // on a Mac too.
        for f_keys in [true, false] {
            assert_eq!(action(&softpresenter, N::Escape, f_keys), Some(A::ClearAll));
            assert_eq!(action(&softprojector, N::Escape, f_keys), Some(A::HideWords));
        }
        // The F keys: off a Mac each does its style's action (F1 clears everything in both); on a
        // Mac they are the system's and are never read.
        let f_rows = [N::F1, N::F2, N::F3, N::F4, N::F5, N::F6, N::F7, N::F8];
        let softpresenter_f = [
            A::ClearAll,
            A::ClearSlide,
            A::ClearMedia,
            A::ClearProps,
            A::ClearAudio,
            A::ClearMessages,
            A::Nothing,
            A::Nothing,
        ];
        let softprojector_f = [
            A::ClearAll,
            A::AddToPlaylist,
            A::RemoveFromPlaylist,
            A::ShowAgain,
            A::ShowSelected,
            A::GoToBible,
            A::GoToService,
            A::Nothing,
        ];
        for (at, f) in f_rows.into_iter().enumerate() {
            assert_eq!(action(&softpresenter, f, true), Some(softpresenter_f[at]), "{f:?}");
            assert_eq!(action(&softprojector, f, true), Some(softprojector_f[at]), "{f:?}");
            assert_eq!(action(&softpresenter, f, false), None, "{f:?} on a Mac");
            assert_eq!(action(&softprojector, f, false), None, "{f:?} on a Mac");
        }
        assert_eq!(
            show_key_action(&us(&Key::Named(N::F1)), &softpresenter, keys::f_keys()).is_some(),
            keys::f_keys(),
            "this computer's way"
        );
        // Keys with no row, and letters, are no action; F9 to F12 are rows doing Nothing.
        for other in [N::Tab, N::ArrowUp, N::ArrowDown, N::Enter, N::Insert, N::Delete] {
            assert_eq!(action(&softprojector, other, true), None, "{other:?}");
        }
        for f in [N::F9, N::F10, N::F11, N::F12] {
            assert_eq!(action(&softprojector, f, true), Some(A::Nothing), "{f:?}");
            assert_eq!(action(&softprojector, f, false), None, "{f:?} on a Mac");
        }
        assert_eq!(show_key_action(&us(&Key::Character("c".into())), &softpresenter, true), None);
        // What each action asks of the show; the app's own (Show Selected, the playlist's, the
        // views') ask nothing of it, and nor does Nothing.
        let layer = |layer| Some(Command::Clear(layer));
        assert_eq!(action_command(A::ClearAll), Some(Command::ClearAll));
        assert_eq!(action_command(A::ClearSlide), layer(sp_engine::Layer::Slides));
        assert_eq!(action_command(A::ClearMedia), layer(sp_engine::Layer::Media));
        assert_eq!(action_command(A::ClearProps), layer(sp_engine::Layer::Props));
        assert_eq!(action_command(A::ClearAudio), layer(sp_engine::Layer::Audio));
        assert_eq!(action_command(A::ClearMessages), layer(sp_engine::Layer::Messages));
        assert_eq!(action_command(A::HideWords), Some(Command::HideWords));
        assert_eq!(action_command(A::ShowAgain), Some(Command::ShowAgain));
        assert_eq!(action_command(A::Next), Some(Command::Next));
        assert_eq!(action_command(A::Previous), Some(Command::Previous));
        assert_eq!(action_command(A::First), Some(Command::First));
        assert_eq!(action_command(A::Last), Some(Command::Last));
        for app in [A::ShowSelected, A::AddToPlaylist, A::RemoveFromPlaylist, A::GoToBible, A::GoToService, A::Nothing]
        {
            assert_eq!(action_command(app), None, "{app:?}");
        }
        // With nothing changed, every key asks the show for what it asked before the table: off a
        // Mac F1 clears everything and F2 to F6 a layer each; F7 and F8 ask nothing of it.
        let asked = |key: N, f_keys: bool| match show_key_press(&us(&Key::Named(key)), &softpresenter, &show, f_keys) {
            Some(KeyPress::Action(action)) => action_command(action),
            Some(KeyPress::Letter(command)) => Some(command),
            None => None,
        };
        for (key, command) in [
            (N::ArrowRight, Command::Next),
            (N::Space, Command::Next),
            (N::ArrowLeft, Command::Previous),
            (N::Home, Command::First),
            (N::End, Command::Last),
            (N::Escape, Command::ClearAll),
            (N::F1, Command::ClearAll),
            (N::F2, Command::Clear(sp_engine::Layer::Slides)),
            (N::F3, Command::Clear(sp_engine::Layer::Media)),
            (N::F4, Command::Clear(sp_engine::Layer::Props)),
            (N::F5, Command::Clear(sp_engine::Layer::Audio)),
            (N::F6, Command::Clear(sp_engine::Layer::Messages)),
        ] {
            assert_eq!(asked(key, true), Some(command), "{key:?}");
        }
        for key in [N::F7, N::F8] {
            let press = show_key_press(&us(&Key::Named(key)), &softpresenter, &show, true);
            assert_eq!(press, Some(KeyPress::Action(A::Nothing)), "{key:?}");
            assert_eq!(asked(key, true), None, "{key:?}");
        }
        assert_eq!(asked(N::Escape, false), Some(Command::ClearAll), "Escape still clears on a Mac");
        // Unless the Bibles window or the setup is open: then Escape closes it.
        let ctx = egui::Context::default();
        let mut state = home::HomeState::default();
        assert!(!escape_closes_something(&state, &ctx));
        state.bible.options.manage_open = true;
        assert!(escape_closes_something(&state, &ctx));
        state.bible.options.manage_open = false;
        state.setup = Some(setup::Setup::new());
        assert!(escape_closes_something(&state, &ctx));
        state.setup = None;
        // The playlist chooser and the Reflow editor too, in the Service view where they are; Enter
        // still runs the show under them.
        let opens: [fn(&mut home::HomeState); 2] =
            [|state| state.chooser_open = true, |state| state.reflow = Some((0, "Words".to_owned()))];
        for open in opens {
            let mut state = home::HomeState::default();
            open(&mut state);
            assert!(escape_closes_something(&state, &ctx) && enter_key_is_free(&state, &ctx));
            // Not drawn on the Bible page: there Escape clears.
            state.bible_open = true;
            assert!(!escape_closes_something(&state, &ctx));
        }
        // Letters jump to the group they stand for, in either case.
        let key = |key: Key| letter_command(&key, &show);
        let jump = |slide| Some(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide }));
        assert_eq!(key(Key::Named(N::Tab)), None);
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
        let key = |key: Key| letter_command(&key, &show);
        assert_eq!(key(Key::Character("a".into())), jump(2));
        assert_eq!(key(Key::Character("7".into())), None);
    }

    #[test]
    fn esc_set_to_clear_slide_clears_the_slide_and_leaves_media_up() {
        let song = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n");
        let ctx = egui::Context::default();
        let escape = Key::Named(NamedKey::Escape);
        for editing in [false, true] {
            let mut show = Show::new(vec![Arc::new(song.clone())]);
            let picture = show.add_media(MediaItem::new("Morning.png".to_owned(), "Morning.png".into()));
            show.apply(Command::ShowMedia(picture));
            show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 }));
            let mut state = home::HomeState::default();
            state.keys.set(ShowKey::Escape, KeyAction::ClearSlide);
            if editing {
                state.editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
                assert!(state.editing.is_some());
            }
            let press = home_key_command(&state, &ctx, &us(&escape), false, &show, false);
            assert_eq!(press, Some(KeyPress::Action(KeyAction::ClearSlide)), "editing: {editing}");
            let Some(KeyPress::Action(action)) = press else { unreachable!() };
            let command = action_command(action).expect("the show is asked");
            assert_eq!(command, Command::Clear(sp_engine::Layer::Slides));
            show.apply(command);
            assert_eq!(show.live(), None, "the slide is off (editing: {editing})");
            assert_eq!(show.live_media(), Some(picture), "the picture behind it stays (editing: {editing})");
        }
    }

    #[test]
    fn a_key_set_to_nothing_does_nothing_and_a_mac_never_reads_f_rows() {
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\nTwo\n"))]);
        let ctx = egui::Context::default();
        let (space, escape) = (Key::Named(NamedKey::Space), Key::Named(NamedKey::Escape));
        let goes =
            |state: &home::HomeState, key: &Key, f_keys| home_key_goes(state, &ctx, &show, &us(key), false, f_keys);
        let nothing = Some(KeyPress::Action(KeyAction::Nothing));
        let mut state = home::HomeState::default();
        state.keys.set(ShowKey::Space, KeyAction::Nothing);
        state.keys.set(ShowKey::Escape, KeyAction::Nothing);
        // The table says Nothing: the key is kept from the window, as a key that runs the show
        // is, and does nothing (`App::key_action`); the show is asked nothing.
        assert_eq!(show_key_action(&us(&space), &state.keys, true), Some(KeyAction::Nothing));
        assert_eq!(show_key_press(&us(&space), &state.keys, &show, true), nothing);
        assert_eq!(home_key_command(&state, &ctx, &us(&space), false, &show, true), nothing);
        assert_eq!(goes(&state, &space, true), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        assert_eq!(action_command(KeyAction::Nothing), None);
        // Escape set to Nothing is the window's: it still closes a menu or pop-up.
        assert_eq!(show_key_press(&us(&escape), &state.keys, &show, true), None);
        assert_eq!(home_key_command(&state, &ctx, &us(&escape), false, &show, true), None);
        assert_eq!(goes(&state, &escape, true), KeyGoes::Window);
        let enter_waiting = [egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        assert!(
            key_waits_for_drawing(&enter_waiting, false, &us(&space), false, &state.keys, &show, true),
            "kept, so it waits"
        );
        assert!(!key_waits_for_drawing(&enter_waiting, false, &us(&escape), false, &state.keys, &show, true));
        assert!(key_waits_for_drawing(&enter_waiting, false, &us(&escape), false, &KeyTable::default(), &show, true));
        // In a typing field every key is the field's, a key set to Nothing too.
        let typing = |events: Vec<egui::Event>| {
            ctx.run_ui(egui::RawInput { events, ..egui::RawInput::default() }, |ui| {
                let mut words = String::new();
                ui.text_edit_singleline(&mut words).request_focus();
            })
            .drop_without_applying_deltas();
        };
        typing(Vec::new());
        typing(Vec::new());
        assert!(ctx.text_edit_focused());
        assert_eq!(goes(&state, &space, true), KeyGoes::Window, "Space types a space");
        ctx.memory_mut(|memory| memory.stop_text_input());
        ctx.run_ui(egui::RawInput::default(), |_| {}).drop_without_applying_deltas();
        assert!(!ctx.text_edit_focused());
        // In the Edit view Space is the editor's, set to Nothing or not: it nudges what is picked.
        let editing = home::HomeState {
            editing: EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 }),
            keys: state.keys.clone(),
            ..home::HomeState::default()
        };
        assert!(editing.editing.is_some());
        assert_eq!(goes(&editing, &space, true), KeyGoes::Window, "Space nudges in the Edit view");
        // F7 and F8 do nothing in SoftPresenter's style: kept from the window, as Space is.
        let fresh = home::HomeState::default();
        for f in [NamedKey::F7, NamedKey::F8] {
            assert_eq!(show_key_action(&us(&Key::Named(f)), &fresh.keys, true), Some(KeyAction::Nothing));
            assert_eq!(goes(&fresh, &Key::Named(f), true), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        }
        // A Mac never reads the F rows, even rows changed on a PC whose preferences came over: they
        // are the window's, as every F key there is.
        let mut from_a_pc = home::HomeState::default();
        from_a_pc.keys.set(ShowKey::F7, KeyAction::Next);
        from_a_pc.keys.set(ShowKey::F2, KeyAction::HideWords);
        let f_rows = [
            NamedKey::F1,
            NamedKey::F2,
            NamedKey::F3,
            NamedKey::F4,
            NamedKey::F5,
            NamedKey::F6,
            NamedKey::F7,
            NamedKey::F8,
        ];
        for f in f_rows {
            assert_eq!(show_key_action(&us(&Key::Named(f)), &from_a_pc.keys, false), None, "{f:?}");
            assert_eq!(goes(&from_a_pc, &Key::Named(f), false), KeyGoes::Window, "{f:?}");
        }
        assert_eq!(goes(&from_a_pc, &Key::Named(NamedKey::F7), true), KeyGoes::Show(KeyPress::Action(KeyAction::Next)));
        assert_eq!(show_key_action(&us(&Key::Named(NamedKey::F2)), &from_a_pc.keys, true), Some(KeyAction::HideWords));
    }

    /// A button that has the keyboard, drawn with `events`: whether it was pressed. Over it, as
    /// `open` says, a menu of its own, drawn as long as it is open; and whether it still is.
    fn button_with_the_keyboard(ctx: &egui::Context, events: Vec<egui::Event>, open: bool) -> (bool, bool) {
        let menu = egui::Id::unique("the button's menu");
        if open {
            egui::Popup::open_id(ctx, menu);
        }
        let mut pressed = false;
        ctx.run_ui(egui::RawInput { events, ..egui::RawInput::default() }, |ui| {
            let button = ui.button("Press me");
            if !button.has_focus() {
                button.request_focus();
            }
            pressed = button.clicked();
            egui::Popup::menu(&button).id(menu).show(|ui| ui.label("A choice"));
        })
        .drop_without_applying_deltas();
        (pressed, egui::Popup::is_id_open(ctx, menu))
    }

    #[test]
    fn space_set_to_nothing_never_presses_the_button_that_has_the_keyboard() {
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\nTwo\n"))]);
        let ctx = egui::Context::default();
        let mut state = home::HomeState::default();
        state.keys.set(ShowKey::Space, KeyAction::Nothing);
        state.keys.set(ShowKey::Escape, KeyAction::Nothing);
        // What the window is given of a key, as `window_event` passes it on: only a key that goes
        // to the window reaches the button.
        let given = |goes: &KeyGoes, key: egui::Key| -> Vec<egui::Event> {
            if *goes != KeyGoes::Window {
                return Vec::new();
            }
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        button_with_the_keyboard(&ctx, Vec::new(), false);
        button_with_the_keyboard(&ctx, Vec::new(), false);
        assert!(ctx.memory(|memory| memory.focused().is_some()) && !ctx.text_edit_focused(), "the button has it");
        let space = Key::Named(NamedKey::Space);
        let goes = home_key_goes(&state, &ctx, &show, &us(&space), false, true);
        assert_eq!(goes, KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        let (pressed, _) = button_with_the_keyboard(&ctx, given(&goes, egui::Key::Space), false);
        assert!(!pressed, "Space set to Nothing is kept from the window: the button is not pressed");
        // Given to the window, Space would have pressed it.
        let (pressed, _) = button_with_the_keyboard(&ctx, given(&KeyGoes::Window, egui::Key::Space), false);
        assert!(pressed, "a Space the window is given presses the button");
        // Escape set to Nothing still closes the menu open, as the window's.
        let (_, open) = button_with_the_keyboard(&ctx, Vec::new(), true);
        assert!(open);
        let (_, open) = button_with_the_keyboard(&ctx, Vec::new(), false);
        assert!(open, "a frame with no key leaves the menu open");
        let escape = Key::Named(NamedKey::Escape);
        let goes = home_key_goes(&state, &ctx, &show, &us(&escape), false, true);
        assert_eq!(goes, KeyGoes::Window);
        let (_, open) = button_with_the_keyboard(&ctx, given(&goes, egui::Key::Escape), false);
        assert!(!open, "Escape closed the menu");
    }

    #[test]
    fn the_screen_windows_follow_the_same_table() {
        let song = sp_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        let ctx = egui::Context::default();
        let mut table = KeyTable::default();
        table.set_style(key_table::KeyStyle::SoftProjector);
        table.set(ShowKey::Right, KeyAction::ClearMedia);
        // A key pressed in a screen's window (`show_key_press`, as `window_event` reads it there).
        let press = |key: NamedKey, f_keys| show_key_press(&us(&Key::Named(key)), &table, &show, f_keys);
        let does = |action| Some(KeyPress::Action(action));
        assert_eq!(press(NamedKey::Escape, false), does(KeyAction::HideWords));
        assert_eq!(press(NamedKey::ArrowRight, false), does(KeyAction::ClearMedia), "a changed row");
        assert_eq!(press(NamedKey::Space, false), does(KeyAction::Next));
        assert_eq!(press(NamedKey::F4, true), does(KeyAction::ShowAgain));
        assert_eq!(press(NamedKey::F1, true), does(KeyAction::ClearAll));
        assert_eq!(press(NamedKey::F4, false), None, "a Mac's F keys are the system's");
        assert_eq!(press(NamedKey::F8, true), does(KeyAction::Nothing), "kept from the window, and does nothing");
        let chorus = Some(KeyPress::Letter(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 })));
        assert_eq!(show_key_press(&us(&Key::Character("c".into())), &table, &show, true), chorus, "a group's letter");
        // The home window, in the Service view, gives every key the same.
        let state = home::HomeState { keys: table.clone(), ..home::HomeState::default() };
        let keys = [
            Key::Named(NamedKey::Escape),
            Key::Named(NamedKey::F1),
            Key::Named(NamedKey::F2),
            Key::Named(NamedKey::F4),
            Key::Named(NamedKey::F8),
            Key::Named(NamedKey::Space),
            Key::Named(NamedKey::ArrowRight),
            Key::Named(NamedKey::ArrowLeft),
            Key::Named(NamedKey::Home),
            Key::Named(NamedKey::End),
            Key::Character("c".into()),
        ];
        for key in keys {
            for f_keys in [true, false] {
                assert_eq!(
                    home_key_command(&state, &ctx, &us(&key), false, &show, f_keys),
                    show_key_press(&us(&key), &table, &show, f_keys),
                    "{key:?}, F keys {f_keys}"
                );
            }
        }
    }

    #[test]
    fn a_slide_picked_is_taken_or_moved_only_by_the_keys_doing_next_and_previous() {
        use running::PickKey;
        let (space, right, left) =
            (Key::Named(NamedKey::Space), Key::Named(NamedKey::ArrowRight), Key::Named(NamedKey::ArrowLeft));
        // A slide picked and nothing on screen: as before the table, Space takes it and the arrows
        // move it.
        let picked = |key: &Key, action| pick_key_for(&us(key), action, true, false, false);
        assert_eq!(picked(&space, KeyAction::Next), PickKey::TakeIt);
        assert_eq!(picked(&right, KeyAction::Next), PickKey::Move(true));
        assert_eq!(picked(&left, KeyAction::Previous), PickKey::Move(false));
        // Any other key doing Next takes it, as Space does (the prototype's way): Page Down, a
        // clicker's forward, and ← set to Next. Page Up, doing Previous, moves it back.
        assert_eq!(picked(&Key::Named(NamedKey::PageDown), KeyAction::Next), PickKey::TakeIt);
        assert_eq!(picked(&left, KeyAction::Next), PickKey::TakeIt);
        assert_eq!(picked(&Key::Named(NamedKey::PageUp), KeyAction::Previous), PickKey::Move(false));
        // → with Num Lock off on the number pad is → too.
        let pad_right = Pressed::at(Key::Named(NamedKey::ArrowRight), winit::keyboard::KeyCode::Numpad6);
        assert_eq!(pick_key_for(&pad_right, KeyAction::Next, true, false, false), PickKey::Move(true));
        // A key set to anything else does that, never touching the pick: Space set to Clear All
        // clears, putting nothing on screen.
        assert_eq!(picked(&space, KeyAction::ClearAll), PickKey::Pass);
        assert_eq!(picked(&right, KeyAction::HideWords), PickKey::Pass);
        // A key doing Previous moves the pick back, whichever key it is, so Space set to Previous
        // never puts it on screen; only → doing Next moves it on.
        assert_eq!(picked(&space, KeyAction::Previous), PickKey::Move(false));
        assert_eq!(picked(&right, KeyAction::Previous), PickKey::Move(false));
        assert_eq!(picked(&Key::Named(NamedKey::End), KeyAction::Next), PickKey::TakeIt);
        // With something on screen, or in the Edit view, the keys run the show as ever.
        assert_eq!(pick_key_for(&us(&space), KeyAction::Next, true, true, false), PickKey::Pass);
        assert_eq!(pick_key_for(&us(&space), KeyAction::Next, true, false, true), PickKey::Pass);
        assert_eq!(pick_key_for(&us(&space), KeyAction::Next, false, false, false), PickKey::Pass);
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
        // Each song is named by its entry's file name, whatever name it keeps inside.
        for (name, title) in [
            ("Anchor Song.pro", "Anchor Song"),
            ("Evening Lantern.pro", "Evening Lantern,"),
            ("anchor song.pro", "Anchor Song (copy)"),
            ("Dawn Road.pro", "Untitled"),
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
        let item = |at: usize| service::Item::song(format!("song-{at}"), songs[at].title.clone());
        let mut services = service::Services::default();
        services.list[0].items = vec![
            service::Item::Header { name: "Welcome".to_owned() },
            service::Item::Header { name: "Sermon".to_owned() },
        ];
        services.list.push(service::Service {
            name: "Evening".to_owned(),
            items: Vec::new(),
            uids: Default::default(),
            origin: None,
        });
        services.put_in(0, Some(0), landed.iter().map(|&at| item(at)).collect());
        services.put_in(1, Some(0), landed.iter().map(|&at| item(at)).collect());
        let names = |playlist: usize| -> Vec<String> {
            services.list[playlist].items.iter().map(|item| item.title().to_owned()).collect()
        };
        assert_eq!(names(0), ["Welcome", "Anchor Song", "Evening Lantern", "Dawn Road", "Sermon"]);
        assert_eq!(names(1), ["Anchor Song", "Evening Lantern", "Dawn Road"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The home window as `window_event` runs it, with no GPU or window: a key pressed waits for
    /// the window to draw first when `key_waits_for_drawing` says so, then goes where
    /// `home_key_goes` sends it, judged from the window as it was last drawn; a key passed on
    /// waits in the interface's input for the next frame, after which the frame's commands are
    /// carried out, as the app does. Enter is carried out as `enter_key` does with a library or
    /// the playlist listed (`entered` keeps the open item it saw). The show keys follow the table
    /// in `state.keys` (SoftPresenter's style unless a test changes it), and an action from it as
    /// `App::key_action` carries it out (`act`); the F rows are read only when `f_keys` says, as
    /// off a Mac (never `keys::f_keys`, which is false on the Mac these tests are built on).
    struct Keyboard {
        ctx: egui::Context,
        show: Show,
        state: home::HomeState,
        waiting: Vec<egui::Event>,
        time: f64,
        entered: Vec<Option<usize>>,
        f_keys: bool,
    }

    impl Keyboard {
        /// Alpha and Beta, three slides each, then the Bible view's two places, with Alpha's first
        /// slide on screen, as during a service; the window drawn once.
        fn new(state: home::HomeState) -> Keyboard {
            let words =
                |first: &str| format!("[Verse 1]\n{first}\n\n[Verse 2]\nSecond lines\n\n[Chorus]\nThird lines\n");
            let mut all = vec![
                Arc::new(parse_song_text("Alpha", &words("Morning lines"))),
                Arc::new(parse_song_text("Beta", &words("Evening lines"))),
            ];
            for _ in 0..2 {
                all.push(Arc::new(sp_model::Presentation {
                    id: Uuid::new_v4(),
                    title: "Bible".to_owned(),
                    groups: Vec::new(),
                    ..(*all[0]).clone()
                }));
            }
            let mut show = Show::new(all);
            show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
            let mut state = state;
            state.bible.slots = Some([2, 3]);
            state.song_keys = vec!["title:Alpha".to_owned(), "title:Beta".to_owned()];
            let mut keyboard = Keyboard {
                ctx: egui::Context::default(),
                show,
                state,
                waiting: Vec::new(),
                time: 0.0,
                entered: Vec::new(),
                f_keys: false,
            };
            keyboard.draw();
            keyboard
        }

        /// One frame of the window, with the input waiting, then its commands carried out.
        fn draw(&mut self) {
            self.time += 0.1;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                time: Some(self.time),
                events: std::mem::take(&mut self.waiting),
                ..egui::RawInput::default()
            };
            let (show, state) = (&self.show, &mut self.state);
            let mut commands = Vec::new();
            self.ctx
                .run_ui(raw, |ui| {
                    commands.extend(home_ui(
                        ui,
                        show,
                        state,
                        &Theme::builtin(),
                        &mut |_| None,
                        home::Preview::default(),
                        None,
                    ));
                })
                .drop_without_applying_deltas();
            for command in commands {
                self.show.apply(command);
            }
            // A passage just looked up goes into its place.
            if let Some((slot, passage)) = self.state.bible.pending.take() {
                self.show.replace_presentation(slot, Arc::new(passage));
            }
            // Show Selected typed in the find box or Scripture Lookup: Enter, once what it opened
            // is in the show, as `window_event` does after the frame.
            if std::mem::take(&mut self.state.enter_after) {
                self.enter();
            }
        }

        /// Enter, as `enter_key` does with the Bible view, a library or the playlist listed.
        fn enter(&mut self) {
            let picked = self.state.pick(&self.show);
            let listed = if self.state.bible_open {
                running::Listed::Bible
            } else if self.state.playlist_open {
                running::Listed::Playlist(self.state.services.chosen)
            } else {
                running::Listed::Library
            };
            self.entered.push(self.state.services.chosen);
            if let Some(running::Do::Show(command)) = running::enter(&[], listed, picked) {
                self.show.apply(command);
            }
        }

        /// What a key set to `action` does, as `App::key_action` carries it out: the show's
        /// actions go to it, Show Selected is Enter, and the window's own are
        /// `window_key_action`'s.
        fn act(&mut self, action: KeyAction) {
            if let Some(command) = action_command(action) {
                self.show.apply(command);
            } else if action == KeyAction::ShowSelected {
                self.enter();
            } else {
                window_key_action(&self.show, &mut self.state, action);
            }
        }

        /// A key set to `action`, as `App::key_press` carries it out: a slide picked first has its
        /// say (`pick_key_press`: taken as Enter takes it, or moved), else the action (`act`).
        fn key_action(&mut self, key: &Pressed, action: KeyAction) {
            match pick_key_press(&self.show, &mut self.state, key, action) {
                running::PickKey::Pass => self.act(action),
                running::PickKey::TakeIt => self.enter(),
                running::PickKey::Move(_) => {}
            }
        }

        /// A key pressed, as the window gets it: drawn first if it waits for what is waiting.
        fn press(&mut self, key: Key) {
            self.press_as(key, false);
        }

        /// A key pressed, or a repeat of one held down (`repeat`), as the window gets it.
        fn press_as(&mut self, key: Key, repeat: bool) {
            self.press_key(Pressed::from(key), repeat);
        }

        /// A key pressed at its place, typing what it types there, with what is held
        /// (`Pressed`), or a repeat of one held down (`repeat`), as the window gets it.
        fn press_key(&mut self, key: Pressed, repeat: bool) {
            let asked = self.state.typed_key.is_some();
            if key_waits_for_drawing(&self.waiting, asked, &key, repeat, &self.state.keys, &self.show, self.f_keys) {
                self.draw();
            }
            match home_key_goes(&self.state, &self.ctx, &self.show, &key, repeat, self.f_keys) {
                KeyGoes::Find => self.state.open_find(),
                KeyGoes::Show(KeyPress::Letter(command)) => {
                    self.show.apply(command);
                }
                KeyGoes::Show(KeyPress::Action(action)) => self.key_action(&key, action),
                // An F row typed in the find box or Scripture Lookup: what it leaves is done as
                // anywhere, as `App::typed_key_press` does.
                KeyGoes::Typed(action) => {
                    if let Some(action) = typed_key(&mut self.state, &self.ctx, action) {
                        self.key_action(&key, action);
                    }
                }
                KeyGoes::Enter => self.enter(),
                // As egui-winit passes it on: the key (as it is, else by its place), with the
                // modifiers held, and the letter it types (none with ⌥ held).
                KeyGoes::Window => {
                    let place = match key.physical {
                        winit::keyboard::PhysicalKey::Code(code) => {
                            let name = format!("{code:?}");
                            let name = name.strip_prefix("Key").or(name.strip_prefix("Digit")).unwrap_or(&name);
                            egui::Key::from_name(name)
                        }
                        winit::keyboard::PhysicalKey::Unidentified(_) => None,
                    };
                    let alt = key.held.alt_key();
                    let (named, typed) = match &key.logical {
                        // An F key (on a Mac, or one the window keeps): the key, typing nothing.
                        Key::Named(named) if ShowKey::named(&key.logical).is_some_and(ShowKey::is_f_key) => {
                            (egui::Key::from_name(&format!("{named:?}")).expect("an F key"), None)
                        }
                        Key::Named(NamedKey::Enter) => (egui::Key::Enter, None),
                        Key::Named(NamedKey::Escape) => (egui::Key::Escape, None),
                        Key::Named(NamedKey::ArrowDown) => (egui::Key::ArrowDown, None),
                        Key::Named(NamedKey::ArrowUp) => (egui::Key::ArrowUp, None),
                        Key::Named(NamedKey::PageDown) => (egui::Key::PageDown, None),
                        Key::Named(NamedKey::PageUp) => (egui::Key::PageUp, None),
                        Key::Named(NamedKey::Space) => (egui::Key::Space, Some(" ".to_owned())),
                        Key::Character(letter) => (
                            egui::Key::from_name(&letter.to_uppercase()).or(place).expect("a letter or its place"),
                            (!alt).then(|| letter.to_string()),
                        ),
                        other => panic!("not passed on in these tests: {other:?}"),
                    };
                    self.waiting.push(egui::Event::Key {
                        key: named,
                        physical_key: place.or(Some(named)),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers { alt, shift: key.held.shift_key(), ..egui::Modifiers::NONE },
                    });
                    self.waiting.extend(typed.map(egui::Event::Text));
                }
            }
        }

        fn live(&self) -> Option<(usize, usize)> {
            self.show.live().map(|at| (at.presentation, at.slide))
        }

        /// A click of the mouse at `at`, as the window gets it: over it, down, then up, a frame each.
        fn click(&mut self, at: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            for event in [egui::Event::PointerMoved(at), button(true), button(false)] {
                self.waiting.push(event);
                self.draw();
            }
        }
    }

    /// The find box open with these words typed, and drawn with the keyboard.
    fn found(words: &str) -> Keyboard {
        let state = home::HomeState { library_search: Some((words.to_owned(), 0)), ..home::HomeState::default() };
        let mut keyboard = Keyboard::new(state);
        keyboard.draw();
        assert!(keyboard.ctx.text_edit_focused(), "the find box has the keyboard");
        keyboard
    }

    #[test]
    fn return_then_enter_with_a_frame_between_picks_the_song_then_puts_it_on_screen() {
        let mut keyboard = found("beta");
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        assert_eq!(keyboard.show.selected(), 1, "Beta is open");
        assert_eq!(keyboard.state.pick(&keyboard.show), Some(sp_engine::SlideRef { presentation: 1, slide: 0 }));
        assert_eq!(keyboard.live(), Some((0, 0)), "Alpha stays on screen");
        keyboard.press(Key::Named(NamedKey::Enter));
        assert_eq!(keyboard.live(), Some((1, 0)), "Enter puts Beta's first slide on screen");
    }

    #[test]
    fn return_then_enter_pressed_before_the_window_draws_still_puts_the_picked_slide_on_screen() {
        // On a computer that draws the window slowly (an Intel Mac, whose painters work slides
        // out on the processor) the second press can come before the frame that acts on the
        // first: it must still be Enter on what the first opened, not a second Return in the find
        // box, which only opens it again.
        let mut keyboard = found("beta");
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        keyboard.draw();
        assert_eq!(keyboard.show.selected(), 1, "Beta is open");
        assert_eq!(keyboard.live(), Some((1, 0)), "Beta's first slide on screen");
        assert_eq!(keyboard.state.pick(&keyboard.show), None, "the pick has gone on screen");
        // By a line of its words: that slide.
        let mut keyboard = found("third lines");
        keyboard.press(Key::Named(NamedKey::ArrowDown));
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((1, 2)), "Beta's chorus on screen");
    }

    #[test]
    fn return_then_a_quick_enter_works_with_the_playlist_listed_with_nothing_on_screen_and_for_a_passage() {
        let enter = Key::Named(NamedKey::Enter);
        for (playlist, cleared) in [(true, false), (false, true), (true, true)] {
            let mut state = home::HomeState {
                library_search: Some(("beta".to_owned(), 0)),
                playlist_open: playlist,
                ..home::HomeState::default()
            };
            if playlist {
                state.services.add(crate::service::Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
                state.services.chosen = Some(0);
            }
            let mut keyboard = Keyboard::new(state);
            if cleared {
                keyboard.show.apply(Command::ClearAll);
            }
            keyboard.draw();
            keyboard.press(enter.clone());
            keyboard.press(enter.clone());
            keyboard.draw();
            let case = format!("playlist listed {playlist}, cleared {cleared}");
            assert_eq!(keyboard.live(), Some((1, 0)), "{case}: the slide picked, not the open item");
        }
        // A passage found by its reference: its first verse, among its chapter's slides.
        let mut keyboard = found("jn 3:16-18");
        keyboard.press(enter.clone());
        keyboard.press(enter);
        keyboard.draw();
        let slot = keyboard.state.bible.slots.map(|slots| slots[keyboard.state.bible.showing]);
        assert_eq!(keyboard.live(), slot.map(|slot| (slot, 15)), "John 3:16 on screen");
    }

    /// Alpha in the playlist listed, "beta" typed in the find box, and Beta's result carried into
    /// the playlist under Alpha's row and let go there.
    fn beta_dropped() -> Keyboard {
        let mut state = home::HomeState {
            playlist_open: true,
            library_search: Some(("beta".to_owned(), 0)),
            ..home::HomeState::default()
        };
        state.services.add(crate::service::Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        let mut keyboard = Keyboard::new(state);
        keyboard.draw();
        assert!(keyboard.ctx.text_edit_focused(), "the find box has the keyboard");
        let from = keyboard.state.find_rows.first().expect("Beta is found").center();
        let to = egui::pos2(120.0, 420.0);
        let button = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let moved = egui::Event::PointerMoved;
        for events in [
            vec![moved(from)],
            vec![button(from, true)],
            vec![moved(from + egui::vec2(-4.0, 12.0))],
            vec![moved(from + egui::vec2(-60.0, 40.0))],
            vec![moved(to)],
            vec![button(to, false)],
            vec![],
        ] {
            keyboard.waiting.extend(events);
            keyboard.draw();
        }
        let titles: Vec<&str> = keyboard.state.services.front().items.iter().map(|item| item.title()).collect();
        assert_eq!(titles, ["Alpha", "Beta"]);
        keyboard
    }

    #[test]
    fn after_a_find_result_is_dropped_the_keys_run_the_show_and_slash_gives_the_box_its_words_back() {
        let mut keyboard = beta_dropped();
        // The words and what they found stay, but the keyboard is the show's again.
        assert_eq!(find_words(&keyboard), Some("beta"));
        assert!(!keyboard.state.find_rows.is_empty() && !keyboard.ctx.text_edit_focused());
        // Space goes on to Alpha's next slide at once, and types nothing.
        keyboard.press(Key::Named(NamedKey::Space));
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 1)), "Space ran the show");
        assert_eq!(find_words(&keyboard), Some("beta"), "and typed nothing in the box");
        // `/` gives the box the keyboard again, with its words: what is typed now goes in it.
        keyboard.press(Key::Character("/".into()));
        keyboard.draw();
        keyboard.draw();
        assert!(keyboard.ctx.text_edit_focused(), "the find box has the keyboard again");
        assert_eq!(find_words(&keyboard), Some("beta"), "with its words");
        keyboard.press(Key::Named(NamedKey::Space));
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 1)), "Space is the box's again");
        let typed = find_words(&keyboard).unwrap_or_default();
        assert_eq!((typed.trim(), typed.len()), ("beta", 5), "a space typed in it");
    }

    #[test]
    fn after_a_find_result_is_dropped_escape_closes_the_search_and_clears_nothing() {
        let mut keyboard = beta_dropped();
        keyboard.press(Key::Named(NamedKey::Escape));
        keyboard.draw();
        assert_eq!(find_words(&keyboard), None, "the search has ended");
        assert_eq!(keyboard.live(), Some((0, 0)), "Alpha's slide is still on screen");
        // And then Escape is the show's again.
        keyboard.press(Key::Named(NamedKey::Escape));
        assert_eq!(keyboard.live(), None, "the screens cleared");
    }

    #[test]
    fn down_then_enter_in_the_playlist_before_the_window_draws_starts_the_item_moved_to() {
        let mut state = home::HomeState { playlist_open: true, ..home::HomeState::default() };
        for title in ["Alpha", "Beta"] {
            state.services.add(crate::service::Item::song(format!("title:{title}"), title.to_owned()));
        }
        state.services.chosen = Some(0);
        let mut keyboard = Keyboard::new(state);
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::ArrowDown));
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        assert_eq!(keyboard.state.services.chosen, Some(1), "\u{2193} opened Beta");
        assert_eq!(keyboard.entered, vec![Some(1)], "and Enter started Beta, the item \u{2193} opened");
    }

    #[test]
    fn letters_and_escape_typed_right_after_slash_are_the_find_boxs_never_the_shows() {
        // "c" is the chorus's letter: typed straight after `/`, before the box has drawn with
        // the keyboard, it must go into the box, not put Alpha's chorus on screen.
        let mut keyboard = Keyboard::new(home::HomeState::default());
        keyboard.press(Key::Character("/".into()));
        keyboard.press(Key::Character("c".into()));
        keyboard.draw();
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 0)), "Alpha's first slide stays on screen");
        assert_eq!(keyboard.state.library_search.as_ref().map(|(typed, _)| typed.as_str()), Some("c"));
        // Escape straight after `/` closes the box, and clears nothing (as before: kept so).
        let mut keyboard = Keyboard::new(home::HomeState::default());
        keyboard.press(Key::Character("/".into()));
        keyboard.press(Key::Named(NamedKey::Escape));
        keyboard.draw();
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 0)), "nothing cleared");
        assert!(keyboard.state.library_search.is_none(), "the find box closed");
    }

    /// A window whose keys are in SoftProjector's style, with the F rows read, as off a Mac.
    fn in_softprojector_style(mut state: home::HomeState) -> Keyboard {
        state.keys.set_style(key_table::KeyStyle::SoftProjector);
        let mut keyboard = Keyboard::new(state);
        keyboard.f_keys = true;
        keyboard
    }

    /// The same with the find box open with these words typed, and drawn with the keyboard.
    fn found_in_softprojector_style(words: &str) -> Keyboard {
        let state = home::HomeState { library_search: Some((words.to_owned(), 0)), ..home::HomeState::default() };
        let mut keyboard = in_softprojector_style(state);
        keyboard.draw();
        assert!(keyboard.ctx.text_edit_focused(), "the find box has the keyboard");
        keyboard
    }

    /// The Bible view with this reference typed in Scripture Lookup, which has the keyboard, the
    /// keys in SoftProjector's style.
    fn typed_in_scripture_lookup(reference: &str) -> Keyboard {
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        state.bible.lookup = reference.to_owned();
        state.bible.focus_lookup = true;
        let mut keyboard = in_softprojector_style(state);
        keyboard.draw();
        assert!(scripture_lookup_has_the_keyboard(&keyboard), "Scripture Lookup has the keyboard");
        assert!(keyboard.ctx.text_edit_focused());
        keyboard
    }

    /// The Theme Editor open on the usual theme, with its end mark's window open over it, drawn.
    fn end_mark_window_open_in_the_theme_editor() -> Keyboard {
        let theme = Theme::builtin();
        let draft = editor::EditDraft::of_slide(theme.templates[0].as_slide(editor::sample_words()), 0);
        let mut keyboard = Keyboard::new(home::HomeState {
            theme_names: vec![theme.name.clone()],
            theme_ids: vec![theme.id],
            theme_end_looks: vec![sp_model::EndLook::default()],
            theme_edit: Some(home::ThemeEdit::of(0, theme)),
            editing: Some(draft),
            ..home::HomeState::default()
        });
        end_look::open(&mut keyboard.state, 0, end_look::Door::Editor);
        keyboard.draw();
        assert!(keyboard.state.end_look.is_some(), "the window is open over the editor");
        keyboard
    }

    /// After a way out of the Theme Editor was taken with its end mark's window open, and
    /// before anything is drawn or closed: no key is held back by a window that is over
    /// nothing, and the keys that run the show act (Space goes on a slide, Escape clears the
    /// screens). Then the app's own close (`HomeState::leave_theme_editor`, which
    /// `App::handle_themes` runs after a drawing and from the idle loop) says how the editor
    /// was left, and leaves nothing of it.
    fn the_show_keys_run_the_show(keyboard: &mut Keyboard, said: &str, left: home::EditorLeft) {
        let (space, escape) = (us(&Key::Named(NamedKey::Space)), us(&Key::Named(NamedKey::Escape)));
        assert!(keyboard.state.theme_edit.is_some(), "{said}: nothing has closed the editor yet");
        assert_eq!(keyboard.state.edit_kind(), None, "{said}: the window is not in the Edit view");
        assert!(!show_key_held_back(&keyboard.state, &keyboard.ctx, &space, true), "{said}: Space is held back");
        assert!(!show_key_held_back(&keyboard.state, &keyboard.ctx, &escape, true), "{said}: Escape is held back");
        assert!(!escape_closes_something(&keyboard.state, &keyboard.ctx), "{said}: Escape has nothing to close");
        let commands = |key: NamedKey| {
            home_key_goes(&keyboard.state, &keyboard.ctx, &keyboard.show, &us(&Key::Named(key)), false, true)
        };
        for key in [NamedKey::Space, NamedKey::ArrowRight, NamedKey::ArrowLeft, NamedKey::PageDown, NamedKey::PageUp] {
            assert!(matches!(commands(key), KeyGoes::Show(_)), "{said}: {key:?} runs the show: {:?}", commands(key));
        }
        keyboard.press(Key::Named(NamedKey::Space));
        assert_eq!(keyboard.live(), Some((0, 1)), "{said}: Space went on a slide, with nothing drawn since");
        assert!(keyboard.state.theme_editor_is_left(), "{said}: the editor is to be closed");
        assert_eq!(keyboard.state.leave_theme_editor(), Some(left), "{said}: how it was left");
        assert!(keyboard.state.theme_edit.is_none() && keyboard.state.editing.is_none(), "{said}");
        assert_eq!(keyboard.state.end_look, None, "{said}: the window went with the editor");
        assert_eq!(keyboard.state.leave_theme_editor(), None, "{said}: closed once");
        keyboard.press(Key::Named(NamedKey::Escape));
        assert_eq!(keyboard.live(), None, "{said}: Escape cleared the screens");
    }

    /// The window opened in the editor holds nothing back once the editor's draft has gone,
    /// whatever is left of the editor: before any frame is drawn.
    fn the_window_is_over_nothing(keyboard: &Keyboard, said: &str) {
        let (space, escape) = (us(&Key::Named(NamedKey::Space)), us(&Key::Named(NamedKey::Escape)));
        assert!(keyboard.state.end_look.is_some(), "{said}: nothing has cleared it yet");
        assert!(!end_look::over_the_operators_window(&keyboard.state), "{said}");
        assert!(!show_key_held_back(&keyboard.state, &keyboard.ctx, &space, true), "{said}: Space is held back");
        assert!(!show_key_held_back(&keyboard.state, &keyboard.ctx, &escape, true), "{said}: Escape is held back");
        assert!(!escape_closes_something(&keyboard.state, &keyboard.ctx), "{said}: Escape closes nothing");
    }

    #[test]
    fn the_end_marks_window_in_the_theme_editor_keeps_the_show_keys_only_while_the_editor_is_there() {
        // Open over the editor, the keys wait for it and Escape is its own.
        let mut keyboard = end_mark_window_open_in_the_theme_editor();
        assert!(show_key_held_back(&keyboard.state, &keyboard.ctx, &us(&Key::Named(NamedKey::Space)), true));
        assert!(escape_closes_something(&keyboard.state, &keyboard.ctx));
        keyboard.press(Key::Named(NamedKey::Space));
        assert_eq!(keyboard.live(), Some((0, 0)), "Space ran nothing under the window");
        assert!(!keyboard.state.theme_editor_is_left(), "and the editor stays");
        assert_eq!(keyboard.state.leave_theme_editor(), None, "nothing closes an editor that is open");
        assert!(keyboard.state.theme_edit.is_some() && keyboard.state.editing.is_some());

        // The Service or the Bible view asked for (Command-1, Command-2, the View menu, `sp
        // view`, the phone, an assistant): the bar asks the app to leave the editor.
        for view in [0, 2] {
            let mut keyboard = end_mark_window_open_in_the_theme_editor();
            keyboard.state.view_request = Some(view);
            keyboard.draw();
            // Asked to leave, the editor stays until the app closes it, which shows the theme.
            assert!(keyboard.state.theme_edit.as_ref().is_some_and(|edit| edit.leave));
            assert_eq!(keyboard.state.leave_theme_editor(), Some(home::EditorLeft::Asked));
            assert_eq!(keyboard.state.end_look, None, "the window went with the editor");
            keyboard.press(Key::Named(NamedKey::Space));
            assert_eq!(keyboard.live(), Some((0, 1)), "view {view}: Space went on a slide");
        }

        // Go to the Slide on Screen (Command-L, and the Program preview's key), as `menu_action`
        // does it: the song on screen chosen in the Service view, the editor's draft put away.
        let mut keyboard = end_mark_window_open_in_the_theme_editor();
        keyboard.state.editing = None;
        keyboard.state.bible_open = false;
        the_window_is_over_nothing(&keyboard, "Go to the Slide on Screen");
        the_show_keys_run_the_show(&mut keyboard, "Go to the Slide on Screen", home::EditorLeft::DraftGone);

        // `sp bible John 3:16` (and the phone's and an assistant's passage), as `ctl_bible`
        // does it: the Bible view, the editor's draft put away.
        let mut keyboard = end_mark_window_open_in_the_theme_editor();
        keyboard.state.bible_open = true;
        keyboard.state.editing = None;
        the_window_is_over_nothing(&keyboard, "a passage asked for from outside");
        the_show_keys_run_the_show(&mut keyboard, "a passage asked for from outside", home::EditorLeft::DraftGone);

        // The app closed the editor between two frames (as it does when asked to leave) with
        // the window still noted as open: no key waits for a frame to clear it.
        let mut keyboard = end_mark_window_open_in_the_theme_editor();
        (keyboard.state.theme_edit, keyboard.state.editing) = (None, None);
        the_window_is_over_nothing(&keyboard, "the editor closed by the app");
        keyboard.press(Key::Named(NamedKey::Space));
        assert_eq!(keyboard.live(), Some((0, 1)), "Space went on a slide before any frame was drawn");
    }

    fn scripture_lookup_has_the_keyboard(keyboard: &Keyboard) -> bool {
        keyboard.ctx.memory(|memory| memory.has_focus(home::scripture_lookup_id()))
    }

    /// The words typed in the find box, while it is open.
    fn find_words(keyboard: &Keyboard) -> Option<&str> {
        keyboard.state.library_search.as_ref().map(|(typed, _)| typed.as_str())
    }

    /// The names of the playlist in front's items, in order.
    fn playlist_items(keyboard: &Keyboard) -> Vec<String> {
        keyboard.state.services.front().items.iter().map(|item| item.title().to_owned()).collect()
    }

    /// The place of the Bible view's passage now shown among the presentations.
    fn bible_place(keyboard: &Keyboard) -> Option<usize> {
        keyboard.state.bible.slots.map(|slots| slots[keyboard.state.bible.showing])
    }

    /// Alpha and Beta in the playlist in front, listed.
    fn alpha_and_beta_listed() -> home::HomeState {
        let mut state = home::HomeState { playlist_open: true, ..home::HomeState::default() };
        for title in ["Alpha", "Beta"] {
            state.services.add(crate::service::Item::song(format!("title:{title}"), title.to_owned()));
        }
        state
    }

    /// The playlist's item opened and picked, as a click on its row leaves them: it alone picked.
    fn open_and_pick(state: &mut home::HomeState, item: usize) {
        state.services.chosen = Some(item);
        state.item_picks.places = vec![item];
        state.item_picks.anchor = Some(item);
        state.item_picks.made_in(&state.services);
        state.picking = home::Picking::Items;
    }

    const NOTHING_TO_ADD: &str = "Open a song in a library, or a passage in Bible, to add it to the playlist.";
    const NOTHING_PICKED: &str = "Pick an item in the playlist to take it out.";

    #[test]
    fn f5_in_the_find_box_shows_the_first_thing_found_and_empties_the_box() {
        let f5 = Key::Named(NamedKey::F5);
        let mut keyboard = found_in_softprojector_style("beta");
        keyboard.press(f5.clone());
        keyboard.draw();
        assert_eq!(keyboard.show.selected(), 1, "Beta is open, as Return opens it");
        assert_eq!(keyboard.live(), Some((1, 0)), "and its first slide on screen, as Enter then puts it");
        assert_eq!(find_words(&keyboard), None, "the find box emptied, as its Return leaves it");
        // A reference typed: it is looked up and its first verse shown, among its chapter's slides.
        let mut keyboard = found_in_softprojector_style("jn 3:16-18");
        keyboard.press(f5);
        keyboard.draw();
        assert_eq!(keyboard.live(), bible_place(&keyboard).map(|slot| (slot, 15)), "John 3:16 on screen");
        assert_eq!(find_words(&keyboard), None);
    }

    #[test]
    fn keys_straight_after_f5_or_f2_in_the_find_box_wait_for_what_it_did() {
        // Pressed before the window has drawn what F5 asked of the find box: Space goes on from
        // Beta's first slide, rather than typing a space in the box still open.
        let mut keyboard = found_in_softprojector_style("beta");
        keyboard.press(Key::Named(NamedKey::F5));
        keyboard.press(Key::Named(NamedKey::Space));
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((1, 1)), "Beta's second slide");
        assert_eq!(find_words(&keyboard), None);
        // A letter straight after F2 doesn't change what it adds ("betaq" finds nothing).
        let mut keyboard = found_in_softprojector_style("beta");
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.press(Key::Character("q".into()));
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Beta"]);
        assert_eq!(keyboard.state.notices.refused(), None);
    }

    #[test]
    fn f2_in_the_find_box_adds_what_command_enter_adds() {
        let f2 = Key::Named(NamedKey::F2);
        let mut keyboard = found_in_softprojector_style("beta");
        keyboard.press(f2.clone());
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Beta"], "Beta in the playlist in front");
        assert_eq!(keyboard.live(), Some((0, 0)), "nothing else goes on screen");
        assert_eq!(keyboard.show.selected(), 0, "and nothing is opened");
        assert_eq!(find_words(&keyboard), None, "the find box emptied");
        // A reference: the passage, kept as Command-Return keeps it.
        let mut keyboard = found_in_softprojector_style("jn 3:16");
        keyboard.press(f2);
        keyboard.draw();
        let items = &keyboard.state.services.front().items;
        assert!(matches!(items.as_slice(), [crate::service::Item::Passage(_)]), "{items:?}");
        assert_eq!(keyboard.live(), Some((0, 0)));
        assert_eq!(find_words(&keyboard), None);
    }

    #[test]
    fn f5_in_the_bible_field_shows_the_passage_typed_and_keeps_it_in_the_field() {
        let mut keyboard = typed_in_scripture_lookup("jn 3:16-18");
        keyboard.press(Key::Named(NamedKey::F5));
        keyboard.draw();
        assert_eq!(keyboard.live(), bible_place(&keyboard).map(|slot| (slot, 15)), "John 3:16 on screen");
        // As Return leaves it: the passage in full, in the operator's language.
        assert_eq!(keyboard.state.bible.lookup, "John 3:16-18", "the reference stays in the field");
        assert!(!scripture_lookup_has_the_keyboard(&keyboard), "the keyboard is back with the show, as after Return");
        // F2 there: looked up, and added as Bible's Add adds it.
        let mut keyboard = typed_in_scripture_lookup("jn 3:16");
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        let saved = keyboard.state.bible.saved().expect("looked up");
        let items = &keyboard.state.services.front().items;
        assert!(matches!(items.as_slice(), [crate::service::Item::Passage(kept)] if *kept == saved), "{items:?}");
        assert_eq!(keyboard.live(), Some((0, 0)), "nothing else goes on screen");
        assert_eq!(keyboard.state.bible.lookup, "John 3:16");
        assert!(!scripture_lookup_has_the_keyboard(&keyboard));
    }

    #[test]
    fn f5_or_f2_with_nothing_found_shows_nothing_and_keeps_the_text() {
        for (key, said) in [(NamedKey::F5, None), (NamedKey::F2, Some(NOTHING_TO_ADD))] {
            let mut keyboard = found_in_softprojector_style("qqzzxq");
            keyboard.press(Key::Named(key));
            keyboard.draw();
            assert_eq!(keyboard.live(), Some((0, 0)), "{key:?}: nothing else on screen");
            assert_eq!(keyboard.show.selected(), 0, "{key:?}: nothing opened");
            assert_eq!(find_words(&keyboard), Some("qqzzxq"), "{key:?}: the words are kept");
            assert!(keyboard.ctx.text_edit_focused(), "{key:?}: and the box keeps the keyboard");
            assert!(playlist_items(&keyboard).is_empty(), "{key:?}: nothing added");
            assert_eq!(keyboard.state.notices.refused(), said, "{key:?}");
            // Scripture Lookup with a reference not understood: the same.
            let mut keyboard = typed_in_scripture_lookup("nosuchbook 9");
            keyboard.press(Key::Named(key));
            keyboard.draw();
            keyboard.draw();
            assert_eq!(keyboard.live(), Some((0, 0)), "{key:?} in Scripture Lookup: nothing else on screen");
            assert_eq!(keyboard.state.bible.lookup, "nosuchbook 9", "{key:?}: the reference is kept");
            assert!(scripture_lookup_has_the_keyboard(&keyboard), "{key:?}: to be put right");
            assert!(playlist_items(&keyboard).is_empty(), "{key:?}: nothing added");
            assert_eq!(keyboard.state.notices.refused(), said, "{key:?} in Scripture Lookup");
        }
    }

    #[test]
    fn f8_set_to_nothing_in_the_find_box_keeps_the_keyboard_and_the_words() {
        let mut keyboard = found_in_softprojector_style("beta");
        let f8 = Key::Named(NamedKey::F8);
        assert_eq!(keyboard.state.keys.action(ShowKey::F8), KeyAction::Nothing);
        // Kept from the window as a row set to Nothing is, and doing nothing.
        let goes = home_key_goes(&keyboard.state, &keyboard.ctx, &keyboard.show, &us(&f8), false, true);
        assert_eq!(goes, KeyGoes::Typed(KeyAction::Nothing));
        keyboard.press(f8);
        keyboard.draw();
        assert_eq!(find_words(&keyboard), Some("beta"), "the words are kept");
        assert!(keyboard.ctx.text_edit_focused(), "and the box keeps the keyboard");
        assert_eq!((keyboard.live(), keyboard.show.selected()), (Some((0, 0)), 0), "nothing opened or shown");
        // Typing goes on in the box.
        keyboard.press(Key::Character("x".into()));
        keyboard.draw();
        assert_eq!(find_words(&keyboard), Some("betax"));
    }

    #[test]
    fn in_softpresenters_style_f2_clears_the_slide_while_typing() {
        let mut keyboard = found("beta");
        keyboard.f_keys = true;
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(keyboard.live(), None, "F2 cleared the slide, as anywhere");
        assert_eq!(find_words(&keyboard), None, "and the find box gave the keyboard up");
        assert!(!keyboard.ctx.text_edit_focused());
        // In Scripture Lookup too: F3 clears the media, and the field gives up the keyboard.
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        state.bible.lookup = "jn 3:16".to_owned();
        state.bible.focus_lookup = true;
        let mut keyboard = Keyboard::new(state);
        keyboard.f_keys = true;
        let picture = keyboard.show.add_media(MediaItem::new("Morning.png".to_owned(), "Morning.png".into()));
        keyboard.show.apply(Command::ShowMedia(picture));
        keyboard.draw();
        assert!(scripture_lookup_has_the_keyboard(&keyboard));
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert_eq!(keyboard.show.live_media(), None, "F3 cleared the media");
        assert_eq!(keyboard.live(), Some((0, 0)), "and left the slide");
        assert!(!scripture_lookup_has_the_keyboard(&keyboard), "the field gave the keyboard up");
        assert_eq!(keyboard.state.bible.lookup, "jn 3:16");
    }

    #[test]
    fn f5_off_a_mac_puts_up_what_enter_would() {
        let mut keyboard = in_softprojector_style(home::HomeState::default());
        // Beta's chorus picked, as a click picks it: F5 puts it on screen, as Enter does.
        keyboard.show.apply(Command::Select(1));
        keyboard.state.set_pick(&keyboard.show, sp_engine::SlideRef { presentation: 1, slide: 2 }, false);
        keyboard.draw();
        let f5 = Key::Named(NamedKey::F5);
        let goes = home_key_goes(&keyboard.state, &keyboard.ctx, &keyboard.show, &us(&f5), false, true);
        assert_eq!(goes, KeyGoes::Show(KeyPress::Action(KeyAction::ShowSelected)));
        keyboard.press(f5.clone());
        assert_eq!(keyboard.live(), Some((1, 2)), "the picked slide");
        // With nothing picked, a library listed: the song open, from its first slide.
        keyboard.draw();
        keyboard.press(f5);
        assert_eq!(keyboard.live(), Some((1, 0)), "Beta from its first slide");
    }

    #[test]
    fn f2_in_the_service_view_with_a_library_song_open_adds_it_at_the_end() {
        let mut state = home::HomeState::default();
        state.services.add(crate::service::Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(crate::service::Item::Header { name: "Sermon".to_owned() });
        let mut keyboard = in_softprojector_style(state);
        // Beta opened from a library, as a click on its row does: no item of the playlist is open.
        keyboard.show.apply(Command::Select(1));
        keyboard.state.services.chosen = None;
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Alpha", "Sermon", "Beta"], "Beta at the end");
        assert_eq!(keyboard.live(), Some((0, 0)), "the screens untouched");
        assert_eq!(keyboard.state.notices.refused(), None);
    }

    #[test]
    fn f2_in_the_bible_view_adds_the_passage_as_bibles_add_does() {
        let mut keyboard = typed_in_scripture_lookup("jn 3:16");
        // Return looks it up and gives the keyboard back; then F2 is the show's.
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        keyboard.draw();
        assert!(!scripture_lookup_has_the_keyboard(&keyboard));
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        let saved = keyboard.state.bible.saved().expect("looked up");
        let items = &keyboard.state.services.front().items;
        assert!(matches!(items.as_slice(), [crate::service::Item::Passage(kept)] if *kept == saved), "{items:?}");
        assert_eq!(keyboard.live(), Some((0, 0)), "nothing else goes on screen");
    }

    #[test]
    fn f2_with_nothing_to_add_says_why() {
        // The playlist listed, its item open: no song from a library open to add.
        let mut state = home::HomeState { playlist_open: true, ..home::HomeState::default() };
        state.services.add(crate::service::Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.chosen = Some(0);
        let mut keyboard = in_softprojector_style(state);
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD));
        assert_eq!(playlist_items(&keyboard), ["Alpha"]);
        // The Bible view with nothing looked up.
        let mut keyboard = in_softprojector_style(home::HomeState { bible_open: true, ..home::HomeState::default() });
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD));
        assert!(playlist_items(&keyboard).is_empty());
        // Beta open from a library, but the chooser of playlists and libraries open in its place:
        // no library is listed, so it isn't a library's song open. (With the playlist listed, a
        // song not in it puts the library back in the list as the window draws.)
        let mut keyboard = in_softprojector_style(home::HomeState::default());
        keyboard.show.apply(Command::Select(1));
        keyboard.state.services.chosen = None;
        keyboard.draw();
        keyboard.state.chooser_open = true;
        keyboard.press(Key::Named(NamedKey::F2));
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD), "the chooser open");
        assert!(playlist_items(&keyboard).is_empty());
        // A song the app makes (the Bible's place) open in the Service view.
        let mut keyboard = in_softprojector_style(home::HomeState::default());
        keyboard.show.apply(Command::Select(2));
        keyboard.state.services.chosen = None;
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD));
        assert!(playlist_items(&keyboard).is_empty());
    }

    #[test]
    fn f3_takes_the_picked_playlist_item_out_and_undo_puts_it_back() {
        // Beta's item clicked, so open and picked (after the first drawing, which opens Alpha's,
        // on screen): F3 takes Beta out.
        let mut keyboard = in_softprojector_style(alpha_and_beta_listed());
        open_and_pick(&mut keyboard.state, 1);
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Alpha"], "Beta, picked and open, taken out");
        assert_eq!(keyboard.live(), Some((0, 0)), "the screens untouched");
        let removed = keyboard.state.removed.pop().expect("kept for Undo");
        assert!(removed.redo(&mut keyboard.state, true), "Undo");
        assert_eq!(playlist_items(&keyboard), ["Alpha", "Beta"], "Undo put it back");
        // None picked: the open item stays, and the cue says why.
        keyboard.state.item_picks = home::ItemPicks::default();
        keyboard.state.services.chosen = Some(0);
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_PICKED));
        assert_eq!(playlist_items(&keyboard), ["Alpha", "Beta"], "the open item, Alpha, stays");
        assert!(keyboard.state.removed.is_empty(), "nothing for Undo");
        // Both picked, which lights them: both go.
        keyboard.state.notices.take_refused();
        keyboard.state.item_picks.places = vec![0, 1];
        keyboard.state.item_picks.made_in(&keyboard.state.services);
        assert!(keyboard.state.item_picks.several());
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert!(playlist_items(&keyboard).is_empty(), "both lit, both taken out");
        assert_eq!(keyboard.state.notices.refused(), None);
    }

    #[test]
    fn f3_takes_nothing_when_the_open_item_has_moved_on_from_the_one_picked() {
        // Alpha's item clicked, so open and picked; then Beta's item becomes the open one while
        // the pick stays on Alpha, unlit. F3 takes nothing out: Alpha is neither open nor lit.
        for how in ["a click on Beta's slide", "Next carrying on into Beta"] {
            let mut keyboard = in_softprojector_style(alpha_and_beta_listed());
            open_and_pick(&mut keyboard.state, 0);
            keyboard.draw();
            keyboard.draw();
            if how.starts_with("Next") {
                // What Next going on into the next item puts on screen; the slides follow it.
                keyboard.show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 1, slide: 0 }));
                keyboard.draw();
            } else {
                let beta = sp_engine::SlideRef { presentation: 1, slide: 1 };
                let at = keyboard.state.slide_rects.iter().find(|(at, _)| *at == beta).map(|(_, rect)| rect.center());
                keyboard.click(at.expect("Beta's second slide drawn"));
                assert_eq!(keyboard.live(), Some((1, 1)), "{how}: Beta's second slide on screen");
            }
            let (open, picked) = (keyboard.state.services.chosen, keyboard.state.item_picks.places.clone());
            assert_eq!((open, picked.as_slice()), (Some(1), &[0][..]), "{how}: Beta open, the pick left on Alpha");
            keyboard.press(Key::Named(NamedKey::F3));
            keyboard.draw();
            assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_PICKED), "{how}");
            assert_eq!(playlist_items(&keyboard), ["Alpha", "Beta"], "{how}: nothing taken out");
            assert!(keyboard.state.removed.is_empty(), "{how}");
        }
    }

    #[test]
    fn f3_takes_nothing_out_of_a_playlist_not_listed() {
        // Beta picked in the playlist, then the playlist put away: in the Bible view, with a
        // library listed, or with the chooser open, F3 takes nothing out and says why.
        for view in ["the Bible view", "a library listed", "the chooser open"] {
            let mut state = alpha_and_beta_listed();
            match view {
                "the Bible view" => state.bible_open = true,
                "a library listed" => state.playlist_open = false,
                _ => state.chooser_open = true,
            }
            let mut keyboard = in_softprojector_style(state);
            // Beta open and picked, so only the listing refuses.
            open_and_pick(&mut keyboard.state, 1);
            keyboard.press(Key::Named(NamedKey::F3));
            keyboard.draw();
            assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_PICKED), "{view}");
            assert_eq!(playlist_items(&keyboard), ["Alpha", "Beta"], "{view}");
            assert!(keyboard.state.removed.is_empty(), "{view}");
        }
    }

    #[test]
    fn holding_f2_or_f3_adds_or_takes_out_nothing_more() {
        // Beta's item open and picked: F3 takes Beta out; held, its repeats take nothing.
        let mut keyboard = in_softprojector_style(alpha_and_beta_listed());
        open_and_pick(&mut keyboard.state, 1);
        let (f2, f3) = (Key::Named(NamedKey::F2), Key::Named(NamedKey::F3));
        let held = |keyboard: &Keyboard, key: &Key| {
            home_key_goes(&keyboard.state, &keyboard.ctx, &keyboard.show, &us(key), true, true)
        };
        assert_eq!(held(&keyboard, &f3), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        assert_eq!(held(&keyboard, &f2), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        keyboard.press(f3.clone());
        keyboard.press_as(f3.clone(), true);
        keyboard.press_as(f3.clone(), true);
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Alpha"], "Beta out, Alpha left");
        // F2 held in the find box: the song found goes in once, and nothing else after it.
        let mut keyboard = found_in_softprojector_style("beta");
        assert_eq!(held(&keyboard, &f2), KeyGoes::Typed(KeyAction::Nothing));
        keyboard.press(f2.clone());
        keyboard.press_as(f2.clone(), true);
        keyboard.draw();
        keyboard.press_as(f2.clone(), true);
        keyboard.draw();
        assert_eq!(playlist_items(&keyboard), ["Beta"]);
        // The screens' windows: the same.
        let state = in_softprojector_style(home::HomeState::default()).state;
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(parse_song_text("Alpha", "One\n\nTwo\n"))]);
        let from_a_screen = |key: &Key, repeat| show_key_command(&state, &ctx, &us(key), repeat, &show, true);
        let does = |action| Some(KeyPress::Action(action));
        assert_eq!(from_a_screen(&f3, true), does(KeyAction::Nothing));
        assert_eq!(from_a_screen(&f2, true), does(KeyAction::Nothing));
        assert_eq!(from_a_screen(&Key::Named(NamedKey::F5), true), does(KeyAction::Nothing));
        assert_eq!(from_a_screen(&f3, false), does(KeyAction::RemoveFromPlaylist));
        assert_eq!(from_a_screen(&f2, false), does(KeyAction::AddToPlaylist));
        assert_eq!(from_a_screen(&Key::Named(NamedKey::Space), true), does(KeyAction::Next), "Space held goes on");
    }

    #[test]
    fn the_screen_windows_keep_the_home_windows_rules_for_editing_and_questions() {
        let show = Show::new(vec![Arc::new(parse_song_text("Alpha", "One\n\nTwo\n"))]);
        let ctx = egui::Context::default();
        let does = |action| Some(KeyPress::Action(action));
        let mut state = in_softprojector_style(home::HomeState::default()).state;
        let from_a_screen = |state: &home::HomeState, key: NamedKey| {
            show_key_command(state, &ctx, &us(&Key::Named(key)), false, &show, true)
        };
        // The Edit view open: only Escape and the F rows, and only to clear or hide the words.
        state.editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        assert!(state.editing.is_some());
        for key in [NamedKey::F2, NamedKey::F3, NamedKey::F4, NamedKey::F5, NamedKey::F6, NamedKey::F7, NamedKey::Space]
        {
            assert_eq!(from_a_screen(&state, key), None, "{key:?} while editing");
        }
        assert_eq!(from_a_screen(&state, NamedKey::F1), does(KeyAction::ClearAll));
        assert_eq!(from_a_screen(&state, NamedKey::Escape), does(KeyAction::HideWords));
        state.editing = None;
        assert_eq!(from_a_screen(&state, NamedKey::F3), does(KeyAction::RemoveFromPlaylist), "the Service view");
        // A question, or the new-song sheet, open: nothing.
        state.confirm_delete_theme = Some(0);
        assert!(state.alert_open());
        assert_eq!(from_a_screen(&state, NamedKey::F2), None, "a question open");
        assert_eq!(from_a_screen(&state, NamedKey::Space), None);
        state.confirm_delete_theme = None;
        state.new_song_sheet = Some(home::NewSong::default());
        assert_eq!(from_a_screen(&state, NamedKey::F5), None, "the new-song sheet open");
        assert_eq!(from_a_screen(&state, NamedKey::Space), None);
    }

    #[test]
    fn f1_clears_all_while_typing_as_anywhere() {
        // SoftPresenter's style, the find box with the keyboard.
        let mut keyboard = found("beta");
        keyboard.f_keys = true;
        let picture = keyboard.show.add_media(MediaItem::new("Morning.png".to_owned(), "Morning.png".into()));
        keyboard.show.apply(Command::ShowMedia(picture));
        keyboard.press(Key::Named(NamedKey::F1));
        keyboard.draw();
        assert_eq!((keyboard.live(), keyboard.show.live_media()), (None, None), "everything cleared");
        assert_eq!(find_words(&keyboard), None, "and the find box gave the keyboard up");
        // And in Scripture Lookup.
        let mut state = home::HomeState { bible_open: true, ..home::HomeState::default() };
        state.bible.lookup = "jn 3:16".to_owned();
        state.bible.focus_lookup = true;
        let mut keyboard = Keyboard::new(state);
        keyboard.f_keys = true;
        keyboard.draw();
        assert!(scripture_lookup_has_the_keyboard(&keyboard));
        keyboard.press(Key::Named(NamedKey::F1));
        keyboard.draw();
        assert_eq!(keyboard.live(), None, "cleared");
        assert!(!scripture_lookup_has_the_keyboard(&keyboard), "the field gave the keyboard up");
    }

    #[test]
    fn f3_with_nothing_picked_says_why() {
        // Beta in the playlist, none of its items picked or open (Alpha, on screen, isn't in it).
        let mut state = home::HomeState { playlist_open: true, ..home::HomeState::default() };
        state.services.add(crate::service::Item::song("title:Beta".to_owned(), "Beta".to_owned()));
        state.services.chosen = None;
        let mut keyboard = in_softprojector_style(state);
        assert_eq!((keyboard.state.services.chosen, keyboard.state.item_picks.places.len()), (None, 0));
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_PICKED));
        assert_eq!(playlist_items(&keyboard), ["Beta"]);
        assert!(keyboard.state.removed.is_empty(), "nothing for Undo");
    }

    #[test]
    fn holding_f5_puts_nothing_more_up_as_a_held_enter_doesnt() {
        // Beta's chorus picked: F5 puts it up; its repeats would find the pick gone and put Beta
        // up from its first slide, so they do nothing, as Enter's do.
        let mut keyboard = in_softprojector_style(home::HomeState::default());
        keyboard.show.apply(Command::Select(1));
        keyboard.state.set_pick(&keyboard.show, sp_engine::SlideRef { presentation: 1, slide: 2 }, false);
        keyboard.draw();
        let f5 = Key::Named(NamedKey::F5);
        keyboard.press(f5.clone());
        assert_eq!(keyboard.live(), Some((1, 2)));
        let held =
            |keyboard: &Keyboard| home_key_goes(&keyboard.state, &keyboard.ctx, &keyboard.show, &us(&f5), true, true);
        assert_eq!(held(&keyboard), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        // In the find box too, before and after it has carried the first press out.
        let mut keyboard = found_in_softprojector_style("third lines");
        assert_eq!(held(&keyboard), KeyGoes::Typed(KeyAction::Nothing));
        keyboard.press(f5.clone());
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 2)), "the first found by its words: Alpha's chorus");
        assert_eq!(held(&keyboard), KeyGoes::Show(KeyPress::Action(KeyAction::Nothing)));
        // The screens' windows: the same rule (`held_down`); other keys go on as they are held.
        assert_eq!(held_down(KeyPress::Action(KeyAction::ShowSelected), true), KeyPress::Action(KeyAction::Nothing));
        assert_eq!(
            held_down(KeyPress::Action(KeyAction::ShowSelected), false),
            KeyPress::Action(KeyAction::ShowSelected)
        );
        assert_eq!(held_down(KeyPress::Action(KeyAction::Next), true), KeyPress::Action(KeyAction::Next));
    }

    #[test]
    fn f2_with_an_item_of_the_playlist_open_says_why() {
        // Alpha's item open; the find box opens Beta, the playlist's item staying the open one.
        // In the Service view F2 adds only a library's song open, so it says why.
        let mut state = home::HomeState { playlist_open: true, ..home::HomeState::default() };
        state.services.add(crate::service::Item::song("title:Alpha".to_owned(), "Alpha".to_owned()));
        state.services.add(crate::service::Item::Placeholder { name: "Sermon".to_owned() });
        state.services.chosen = Some(0);
        state.library_search = Some(("beta".to_owned(), 0));
        let mut keyboard = in_softprojector_style(state);
        keyboard.draw();
        keyboard.press(Key::Named(NamedKey::Enter));
        keyboard.draw();
        assert_eq!(
            (keyboard.show.selected(), keyboard.state.services.chosen),
            (1, Some(0)),
            "Beta open, Alpha's item too"
        );
        keyboard.press(Key::Named(NamedKey::F2));
        keyboard.draw();
        assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD));
        assert_eq!(playlist_items(&keyboard), ["Alpha", "Sermon"]);
        // The placeholder opened: no song is chosen with it, so the song chosen before (Beta) is
        // a stale one, never added; with the playlist listed, or a library listed after.
        for listed in [true, false] {
            keyboard.state.notices.take_refused();
            keyboard.state.playlist_open = listed;
            keyboard.draw();
            keyboard.state.services.chosen = Some(1);
            assert_eq!(keyboard.show.selected(), 1, "Beta, chosen before, still the show's");
            keyboard.press(Key::Named(NamedKey::F2));
            keyboard.draw();
            assert_eq!(keyboard.state.notices.refused(), Some(NOTHING_TO_ADD), "playlist listed: {listed}");
            assert_eq!(playlist_items(&keyboard), ["Alpha", "Sermon"], "playlist listed: {listed}");
        }
    }

    #[test]
    fn f6_and_f7_switch_to_the_bible_and_back() {
        let mut keyboard = in_softprojector_style(home::HomeState::default());
        keyboard.press(Key::Named(NamedKey::F6));
        keyboard.draw();
        keyboard.draw();
        assert!(keyboard.state.bible_open, "F6 opened the Bible view");
        assert!(
            scripture_lookup_has_the_keyboard(&keyboard),
            "with the keyboard in Scripture Lookup, as Control-B leaves it"
        );
        // F7, though Scripture Lookup has the keyboard: the field gives it up.
        keyboard.press(Key::Named(NamedKey::F7));
        keyboard.draw();
        assert!(!keyboard.state.bible_open, "F7 went back to the Service view");
        assert!(!scripture_lookup_has_the_keyboard(&keyboard));
        assert_eq!(keyboard.live(), Some((0, 0)), "the screens untouched");
    }

    #[test]
    fn escape_in_the_find_box_still_closes_it() {
        let mut keyboard = found_in_softprojector_style("beta");
        assert_eq!(keyboard.state.keys.action(ShowKey::Escape), KeyAction::HideWords);
        keyboard.press(Key::Named(NamedKey::Escape));
        keyboard.draw();
        assert_eq!(find_words(&keyboard), None, "Escape closed the find box");
        assert!(!keyboard.show.words_hidden(), "and hid nothing");
        assert_eq!(keyboard.live(), Some((0, 0)));
    }

    /// The slide on screen as an audience screen draws it from the frame (`output::drawn_slide`),
    /// as the Program preview's audience picture does too.
    fn audience(show: &Show) -> Option<Arc<Slide>> {
        let values =
            links::Values { timers: &[], title: None, group: None, next: None, stage_message: None, copyright: None };
        let clock = sp_model::BuildClock::settled(show.build_step());
        let theme = Theme::builtin();
        frame_slide(show)
            .map(|slide| output::drawn_slide(&slide, &theme, None, clock, &values, show.words_hidden(), None))
    }

    /// A slide's words, its boxes' words joined.
    fn words_of(slide: &Slide) -> String {
        slide.texts().into_iter().map(|(_, words)| words).collect::<Vec<_>>().join("/")
    }

    #[test]
    fn while_the_words_are_hidden_the_audience_draws_the_same_slide_in_its_songs_theme_without_them() {
        let mut show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n--\nUno\n\nTwo\n"))]);
        // The songs theme, and the song's own (`App::theme_of` gives it by the song's place).
        let songs = Arc::new(Theme::builtin());
        let own = Arc::new(Theme { name: "The song's own".to_owned(), ..Theme::builtin() });
        let theme_of = |at: usize| if at == 0 { &own } else { &songs };
        assert!(Arc::ptr_eq(frame_theme(&show, &songs, theme_of), &songs), "nothing up: the songs theme");
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let live = show.live_slide().expect("on screen").1.clone();
        let drawn = audience(&show).expect("on screen");
        assert_eq!((words_of(&drawn), drawn.words_hidden), ("One/Uno".to_owned(), false));

        show.apply(Command::HideWords);
        let drawn = audience(&show).expect("still on screen");
        assert!(drawn.words_hidden, "the audience screens draw it without its words");
        // The same slide, its words kept for laying it out (they aren't drawn).
        assert_eq!((drawn.id, words_of(&drawn)), (live.id, "One/Uno".to_owned()));
        // Both frame builders draw it in the song's theme still, as the slide is still up.
        assert!(Arc::ptr_eq(frame_theme(&show, &songs, theme_of), &own), "the song's theme");

        show.apply(Command::ShowAgain);
        assert!(!audience(&show).expect("on screen").words_hidden, "Show Again brings them back");
    }

    #[test]
    fn while_the_words_are_hidden_the_stage_screens_keep_them() {
        let mut show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n\nTwo\n"))]);
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        show.apply(Command::HideWords);
        // The frame's slide is what a stage screen's picture is made from (`content_for`).
        let sent = frame_slide(&show).expect("on screen");
        assert!(!sent.words_hidden);
        let layout = &stage::builtin_layouts()[0];
        let stage = stage::slide(Some(&sent), None, &output::Overlay::default(), &stage::About::default(), layout);
        assert!(stage.texts().iter().any(|(_, words)| words == "One"), "{:?}", stage.texts());
        assert!(!stage.words_hidden, "the stage screens show the words");
        assert!(audience(&show).expect("on screen").words_hidden, "the room doesn't");
    }

    #[test]
    fn hide_the_words_over_a_playing_video_keeps_the_video_and_next_brings_words_back() {
        let mut show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n\nTwo\n"))]);
        let video = show.add_media(MediaItem::new("Waves".to_owned(), PathBuf::from("waves.mp4")));
        assert!(media::is_video(&show.media()[video].path));
        show.apply(Command::ShowMedia(video));
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let changes = show.apply(Command::HideWords);
        // The media layer is not touched, so `sync_video` keeps the same player going.
        assert!(changes.slides && !changes.media, "{changes:?}");
        assert_eq!(show.live_media(), Some(video), "the video plays on behind");
        let drawn = audience(&show).expect("the slide stays up");
        assert!(drawn.words_hidden, "without its words");

        let changes = show.apply(Command::Next);
        assert!(!changes.media, "{changes:?}");
        assert_eq!(show.live_media(), Some(video), "still the same video");
        let drawn = audience(&show).expect("on screen");
        assert_eq!((words_of(&drawn), drawn.words_hidden), ("Two".to_owned(), false), "Next shows its words");
    }

    #[test]
    fn the_copyright_lines_last_slide_is_the_one_the_end_mark_is_on() {
        // Written verse, chorus, verse, then a blank slide; sung verse, chorus, verse, chorus,
        // blank: the end is the chorus sung the second time, not the written last slide.
        let mut song = parse_song_text("River Song", "[Verse 1]\nOne\n\n[Chorus]\nSing\n\n[Verse 2]\nTwo\n");
        song.copyright.author = "A. Writer".to_owned();
        song.groups.push(sp_model::Group {
            id: sp_model::Uuid::new_v4(),
            name: "Ending".to_owned(),
            slides: vec![Slide::new(Vec::new())],
            ..song.groups[0].clone()
        });
        let ids: Vec<sp_model::Uuid> = song.groups.iter().map(|group| group.id).collect();
        let sung = sp_model::Arrangement {
            id: sp_model::Uuid::new_v4(),
            name: "Sung".to_owned(),
            groups: vec![ids[0], ids[1], ids[2], ids[1], ids[3]],
            auto: false,
        };
        song.arrangement = Some(sung.id);
        song.arrangements.push(sung);
        assert_eq!(song.order(), [0, 1, 2, 1, 3]);
        let mut show = Show::new(vec![Arc::new(song)]);
        let last = |show: &Show| end_mark::last_sung(&show.presentations()[0]);
        let line = |show: &Show| {
            copyright_line(show, |_| false, service::CopyrightShow::LastSlide, "", |_| last(show)).is_some()
        };
        let marked = |show: &Show| {
            let at = show.live().expect("on screen");
            end_mark::due(true, false, false, &show.presentations()[0], last(show), at.slide, show.step())
        };
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let mut seen = Vec::new();
        loop {
            assert_eq!(line(&show), marked(&show), "the line and the stars land together");
            seen.push((show.live().map(|at| at.slide), line(&show)));
            if show.upcoming().is_none() {
                break;
            }
            show.apply(Command::Next);
        }
        assert_eq!(
            seen,
            [(Some(0), false), (Some(1), false), (Some(2), false), (Some(1), true), (Some(3), false)],
            "on the chorus sung last, never on the blank slide after it"
        );
        // What has no song's words keeps its written last slide for the line.
        let mut deck = parse_song_text("Deck", "One\n\nTwo\n");
        deck.copyright.author = "A. Writer".to_owned();
        for slide in &mut deck.groups[0].slides {
            slide.objects[0].role = None;
        }
        let mut show = Show::new(vec![Arc::new(deck)]);
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        assert!(!line(&show));
        show.apply(Command::Next);
        assert!(line(&show) && last(&show).is_none());
    }

    #[test]
    fn hide_the_words_leaves_the_copyright_line_as_it_is() {
        let mut song = parse_song_text("River Song", "One\n\nTwo\n");
        song.copyright.author = "A. Writer".to_owned();
        let mut show = Show::new(vec![Arc::new(song)]);
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let last = |show: &Show| end_mark::last_sung(&show.presentations()[0]);
        let line =
            |show: &Show| copyright_line(show, |_| false, service::CopyrightShow::EverySlide, "1234", |_| last(show));
        let shown = line(&show).expect("the copyright line on the slide on screen");
        assert!(shown.contains("A. Writer"), "{shown}");
        let app_made = copyright_line(&show, |_| true, service::CopyrightShow::EverySlide, "", |_| last(&show));
        assert_eq!(app_made, None, "app-made");

        // What `sync_overlay` sends over the slides stays as it is, so it sends nothing and
        // tells the screens nothing again: the audience screens leave the line out with the
        // words themselves (`output::copyright_shown`), and the stage keeps it.
        show.apply(Command::HideWords);
        assert_eq!(line(&show).as_ref(), Some(&shown), "Hide the Words");
        show.apply(Command::ShowAgain);
        assert_eq!(line(&show), Some(shown), "Show Again");
    }

    #[test]
    fn the_next_line_is_worked_out_again_when_the_words_are_hidden_or_shown_again() {
        let mut show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n\nTwo\n"))]);
        show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 1 }));
        let state = home::HomeState::default();
        let from = NextLineFrom::of(&show, &state);
        assert!(from.holds(&show, &state));
        show.apply(Command::HideWords);
        assert!(!from.holds(&show, &state), "Hide the Words changes what Next puts on screen");
        let from = NextLineFrom::of(&show, &state);
        show.apply(Command::ShowAgain);
        assert!(!from.holds(&show, &state), "and so does Show Again");
    }

    #[test]
    fn keys_go_by_position_so_period_works_on_a_russian_layout() {
        use key_table::KeyStyle;
        use winit::keyboard::KeyCode;
        // On a Russian layout the key at "." types ю; a clicker's blank-screen button is that place.
        let period = Pressed::at(Key::Character("\u{44e}".into()), KeyCode::Period);
        let ctx = egui::Context::default();
        let show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n\nTwo\n"))]);
        let goes =
            |state: &home::HomeState, key: &Pressed, f_keys| home_key_goes(state, &ctx, &show, key, false, f_keys);
        let does = |action| KeyGoes::Show(KeyPress::Action(action));
        let mut state = home::HomeState::default();
        assert_eq!(goes(&state, &period, true), does(KeyAction::ClearAll), "Clear All in Standard");
        assert_eq!(goes(&state, &period, false), does(KeyAction::ClearAll), "on a Mac too");
        state.keys.set_style(KeyStyle::SoftProjector);
        assert_eq!(goes(&state, &period, true), does(KeyAction::HideWords), "Hide the Words like SoftProjector");
        // In the Bible view, where a letter typed starts a reference, ю types; "." on a US
        // keyboard, which is no letter, still does its row there.
        state.bible_open = true;
        assert_eq!(goes(&state, &period, true), KeyGoes::Window, "ю starts a reference in the Bible view");
        let us_period = us(&Key::Character(".".into()));
        assert_eq!(goes(&state, &us_period, true), does(KeyAction::HideWords), "a US \".\" in the Bible view");
        state.bible_open = false;
        // The key at "/" types "." there: it is no row, and the window has it.
        assert_eq!(goes(&state, &Pressed::at(Key::Character(".".into()), KeyCode::Slash), true), KeyGoes::Window);
        // On a US keyboard "." is the same row.
        assert_eq!(goes(&state, &us(&Key::Character(".".into())), true), does(KeyAction::HideWords));
        // Through the window: the slide goes.
        let mut keyboard = Keyboard::new(home::HomeState::default());
        assert_eq!(keyboard.live(), Some((0, 0)));
        keyboard.press_key(period, false);
        assert_eq!(keyboard.live(), None, "Clear All took the slide off");
    }

    #[test]
    fn a_mapped_letter_beats_the_group_jump_in_service_but_types_in_bible() {
        use winit::keyboard::KeyCode;
        let c = Key::Character("c".into());
        let at_c = ShowKey::Other(KeyCode::KeyC);
        // Not added, C jumps to the chorus, by the letter it types.
        let mut keyboard = Keyboard::new(home::HomeState::default());
        keyboard.press(c.clone());
        assert_eq!(keyboard.live(), Some((0, 2)), "C: Alpha's chorus");
        // Added and set to Next, it does its row in place of the jump.
        let mut table = KeyTable::default();
        assert_eq!(table.add(&us(&c), true), Ok(at_c));
        table.set(at_c, KeyAction::Next);
        let mut keyboard = Keyboard::new(home::HomeState { keys: table.clone(), ..home::HomeState::default() });
        keyboard.press(c.clone());
        assert_eq!(keyboard.live(), Some((0, 1)), "C set to Next: the next slide, not the chorus");
        // By its place: the Russian letter at C's place does it too.
        keyboard.show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        keyboard.press_key(Pressed::at(Key::Character("\u{441}".into()), KeyCode::KeyC), false);
        assert_eq!(keyboard.live(), Some((0, 1)), "с at C's place");
        // In place of a slide's own hot key too.
        let mut keyed = parse_song_text("Song", "[Verse 1]\nOne\n\nTwo\n");
        keyed.groups[0].slides[1].hot_key = Some('C');
        let show = Show::new(vec![Arc::new(keyed)]);
        assert!(matches!(letter_command(&c, &show), Some(Command::GoTo(_))), "C is the slide's hot key");
        assert_eq!(show_key_press(&us(&c), &table, &show, true), Some(KeyPress::Action(KeyAction::Next)));
        // Set to Nothing it does nothing at all: no jump either.
        table.set(at_c, KeyAction::Nothing);
        assert_eq!(show_key_press(&us(&c), &table, &show, true), Some(KeyPress::Action(KeyAction::Nothing)));
        table.set(at_c, KeyAction::Next);
        // A field being typed in keeps it: the find box.
        let ctx = egui::Context::default();
        let finding = home::HomeState {
            keys: table.clone(),
            library_search: Some((String::new(), 0)),
            ..home::HomeState::default()
        };
        assert_eq!(
            home_key_goes(&finding, &ctx, &show, &us(&c), false, true),
            KeyGoes::Window,
            "typed in the find box"
        );
        // In the Bible view it types a reference, whatever its row says.
        let mut bible = Keyboard::new(home::HomeState { keys: table, bible_open: true, ..home::HomeState::default() });
        bible.press(c);
        bible.draw();
        assert_eq!(bible.state.bible.lookup, "c", "C started a reference");
        assert_eq!(bible.live(), Some((0, 0)), "and nothing moved on screen");
    }

    #[test]
    fn digits_pick_the_numbers_while_they_show() {
        use winit::keyboard::{KeyCode, ModifiersState};
        let one = Key::Character("1".into());
        let mut table = KeyTable::default();
        assert_eq!(table.add(&us(&one), true), Ok(ShowKey::Other(KeyCode::Digit1)));
        table.set(ShowKey::Other(KeyCode::Digit1), KeyAction::ClearAll);
        let mut keyboard = Keyboard::new(home::HomeState { keys: table, ..home::HomeState::default() });
        // While the numbers show (after ⌥1 … ⌥7 or ⇧⌘M), 1 picks the first thing numbered.
        let numbers =
            |keyboard: &Keyboard| keys::Numbers { of: keys::NumbersOf::Tab, since: keyboard.time, picked: None };
        keyboard.state.numbers = Some(numbers(&keyboard));
        keyboard.press(one.clone());
        keyboard.draw();
        assert!(keys::Numbers::picked(&keyboard.state.numbers, keys::NumbersOf::Tab, 0), "1 picked the first");
        assert_eq!(keyboard.live(), Some((0, 0)), "and cleared nothing");
        // At that place on a layout where it types another character (& on a French one) too.
        keyboard.press_key(Pressed::at(Key::Character("&".into()), KeyCode::Digit1), false);
        keyboard.draw();
        assert!(keys::Numbers::picked(&keyboard.state.numbers, keys::NumbersOf::Tab, 0), "& at 1's place");
        assert_eq!(keyboard.live(), Some((0, 0)));
        // With no numbers showing, the digit does its row.
        keyboard.state.numbers = None;
        keyboard.press(one);
        assert_eq!(keyboard.live(), None, "1 set to Clear All cleared");
        // ⌥1 is the window's still: it opens the first tool tab and numbers what is in it.
        keyboard.show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        let option_one =
            Pressed { held: ModifiersState::ALT, ..Pressed::at(Key::Character("\u{a1}".into()), KeyCode::Digit1) };
        keyboard.press_key(option_one, false);
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 0)), "⌥1 cleared nothing");
        assert!(keys::Numbers::showing(&keyboard.state.numbers, keys::NumbersOf::Tab), "⌥1 numbered the tab");
    }

    #[test]
    fn page_down_is_next_and_page_up_is_previous_by_default() {
        for style in [key_table::KeyStyle::SoftPresenter, key_table::KeyStyle::SoftProjector] {
            for f_keys in [false, true] {
                let mut state = home::HomeState::default();
                state.keys.set_style(style);
                let mut keyboard = Keyboard::new(state);
                keyboard.f_keys = f_keys;
                keyboard.press(Key::Named(NamedKey::PageDown));
                assert_eq!(keyboard.live(), Some((0, 1)), "{style:?}, F keys {f_keys}: Page Down, the next slide");
                keyboard.press(Key::Named(NamedKey::PageDown));
                assert_eq!(keyboard.live(), Some((0, 2)));
                keyboard.press(Key::Named(NamedKey::PageUp));
                assert_eq!(keyboard.live(), Some((0, 1)), "{style:?}, F keys {f_keys}: Page Up, the previous slide");
            }
        }
    }

    #[test]
    fn in_the_edit_view_period_clears_when_it_is_a_clear_and_keys_added_wait() {
        use KeyAction as A;
        use winit::keyboard::KeyCode;
        let song = parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        let ctx = egui::Context::default();
        let editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        let mut state = home::HomeState { editing, ..home::HomeState::default() };
        assert!(state.editing.is_some());
        let key =
            |state: &home::HomeState, key: &Pressed, f_keys| home_key_command(state, &ctx, key, false, &show, f_keys);
        let does = |action| Some(KeyPress::Action(action));
        let period = us(&Key::Character(".".into()));
        assert_eq!(key(&state, &period, false), does(A::ClearAll), "Clear All in Standard");
        state.keys.set_style(key_table::KeyStyle::SoftProjector);
        assert_eq!(key(&state, &period, false), does(A::HideWords), "Hide the Words is a clear");
        state.keys.set(ShowKey::Period, A::Next);
        assert_eq!(key(&state, &period, false), None, "set to Next it waits, as Space does");
        // A key added never reaches the show from here, set to a clear or not...
        let b = us(&Key::Character("b".into()));
        assert_eq!(state.keys.add(&b, false), Ok(ShowKey::Other(KeyCode::KeyB)));
        state.keys.set(ShowKey::Other(KeyCode::KeyB), A::ClearAll);
        assert_eq!(key(&state, &b, false), None, "B is the editor's");
        // ...but an F key added on a Mac is an F row, and clears here as they do.
        let f3 = us(&Key::Named(NamedKey::F3));
        assert_eq!(key(&state, &f3, false), None, "a Mac's F3, not added");
        assert_eq!(state.keys.add(&f3, false), Ok(ShowKey::F3));
        state.keys.set(ShowKey::F3, A::ClearMedia);
        assert_eq!(key(&state, &f3, false), does(A::ClearMedia), "F3 added on a Mac");
        // In the Service view the keys added run the show.
        let service = home::HomeState { keys: state.keys.clone(), ..home::HomeState::default() };
        assert_eq!(key(&service, &b, false), does(A::ClearAll));
        assert_eq!(key(&service, &period, false), does(A::Next));
    }

    #[test]
    fn an_f_key_added_on_a_mac_runs_the_show_there() {
        let f2 = Key::Named(NamedKey::F2);
        let mut state = home::HomeState::default();
        assert_eq!(state.keys.add(&us(&f2), false), Ok(ShowKey::F2));
        assert_eq!(state.keys.action(ShowKey::F2), KeyAction::ClearSlide, "as Standard gives it off a Mac");
        // The harness reads no F rows (`f_keys` false), as on a Mac.
        let mut keyboard = Keyboard::new(state);
        keyboard.press(f2);
        assert_eq!(keyboard.live(), None, "F2 cleared the slide");
        // F3, not added, is the system's.
        keyboard.show.apply(Command::GoTo(sp_engine::SlideRef { presentation: 0, slide: 0 }));
        keyboard.press(Key::Named(NamedKey::F3));
        keyboard.draw();
        assert_eq!(keyboard.live(), Some((0, 0)));
    }

    /// The keyboard, with Alpha's second slide picked and nothing on screen.
    fn picked_with_nothing_on_screen(state: home::HomeState) -> Keyboard {
        let mut keyboard = Keyboard::new(state);
        keyboard.show.apply(Command::ClearAll);
        let second = sp_engine::SlideRef { presentation: 0, slide: 1 };
        keyboard.state.set_pick(&keyboard.show, second, false);
        assert_eq!((keyboard.live(), keyboard.state.pick(&keyboard.show)), (None, Some(second)));
        keyboard
    }

    #[test]
    fn page_down_puts_the_picked_slide_up_when_nothing_is_on_screen() {
        // A clicker's forward, with Next starting what is lined up: the slide picked.
        for style in [key_table::KeyStyle::SoftPresenter, key_table::KeyStyle::SoftProjector] {
            let mut state = home::HomeState::default();
            state.keys.set_style(style);
            let mut keyboard = picked_with_nothing_on_screen(state);
            keyboard.press(Key::Named(NamedKey::PageDown));
            assert_eq!(keyboard.live(), Some((0, 1)), "{style:?}: Page Down put the picked slide up");
            keyboard.press(Key::Named(NamedKey::PageDown));
            assert_eq!(keyboard.live(), Some((0, 2)), "{style:?}: then on, as Next");
        }
        // A key added and set to Next does the same.
        let mut state = home::HomeState::default();
        let b = Key::Character("b".into());
        assert_eq!(state.keys.add(&us(&b), true), Ok(ShowKey::Other(winit::keyboard::KeyCode::KeyB)));
        state.keys.set(ShowKey::Other(winit::keyboard::KeyCode::KeyB), KeyAction::Next);
        let mut keyboard = picked_with_nothing_on_screen(state);
        keyboard.press(b);
        assert_eq!(keyboard.live(), Some((0, 1)), "B set to Next put the picked slide up");
    }

    #[test]
    fn only_the_right_arrow_moves_the_pick_on_and_previous_moves_it_back() {
        let mut keyboard = picked_with_nothing_on_screen(home::HomeState::default());
        let pick = |keyboard: &Keyboard| keyboard.state.pick(&keyboard.show).map(|at| (at.presentation, at.slide));
        keyboard.press(Key::Named(NamedKey::ArrowRight));
        assert_eq!((keyboard.live(), pick(&keyboard)), (None, Some((0, 2))), "→ moved the pick on, nothing up");
        keyboard.press(Key::Named(NamedKey::PageUp));
        assert_eq!((keyboard.live(), pick(&keyboard)), (None, Some((0, 1))), "Page Up moved it back");
        keyboard.press(Key::Named(NamedKey::ArrowLeft));
        assert_eq!((keyboard.live(), pick(&keyboard)), (None, Some((0, 0))), "← moved it back");
        keyboard.press(Key::Named(NamedKey::Space));
        assert_eq!(keyboard.live(), Some((0, 0)), "Space put it up");
    }

    #[test]
    fn in_the_bible_view_a_letter_typed_starts_the_reference_whatever_its_place() {
        use winit::keyboard::KeyCode;
        // A Ukrainian layout: the "." key types ю.
        let yu = || Pressed::at(Key::Character("\u{44e}".into()), KeyCode::Period);
        let mut bible = Keyboard::new(home::HomeState { bible_open: true, ..home::HomeState::default() });
        bible.press_key(yu(), false);
        bible.draw();
        assert_eq!(bible.state.bible.lookup, "\u{44e}", "ю started a reference");
        assert_eq!(bible.live(), Some((0, 0)), "and the screen was not cleared");
        // The same key in the Service view does its row: Clear All.
        let mut service = Keyboard::new(home::HomeState::default());
        service.press_key(yu(), false);
        assert_eq!(service.live(), None, "Clear All in the Service view");
        // "." on a US keyboard types no letter, so it does its row in the Bible view too.
        let mut bible = Keyboard::new(home::HomeState { bible_open: true, ..home::HomeState::default() });
        bible.press(Key::Character(".".into()));
        bible.draw();
        assert_eq!(bible.live(), None, "a US \".\" cleared in the Bible view");
        assert_eq!(bible.state.bible.lookup, "");
        // And б at the place of "," (added, set to Clear All) types, for "Буття" and "Бытие".
        let mut keys = KeyTable::default();
        assert_eq!(keys.add(&us(&Key::Character(",".into())), true), Ok(ShowKey::Other(KeyCode::Comma)));
        keys.set(ShowKey::Other(KeyCode::Comma), KeyAction::ClearAll);
        let mut bible = Keyboard::new(home::HomeState { keys, bible_open: true, ..home::HomeState::default() });
        bible.press_key(Pressed::at(Key::Character("\u{431}".into()), KeyCode::Comma), false);
        bible.draw();
        assert_eq!((bible.state.bible.lookup.as_str(), bible.live()), ("\u{431}", Some((0, 0))));
    }

    #[test]
    fn a_digit_with_a_row_starts_the_reference_in_the_bible_view() {
        use winit::keyboard::KeyCode;
        // For "1 Кор" and "2 Peter".
        let one = Key::Character("1".into());
        let mut keys = KeyTable::default();
        assert_eq!(keys.add(&us(&one), true), Ok(ShowKey::Other(KeyCode::Digit1)));
        keys.set(ShowKey::Other(KeyCode::Digit1), KeyAction::ClearAll);
        let mut bible =
            Keyboard::new(home::HomeState { keys: keys.clone(), bible_open: true, ..home::HomeState::default() });
        bible.press(one.clone());
        bible.draw();
        assert_eq!(bible.state.bible.lookup, "1", "1 started a reference");
        assert_eq!(bible.live(), Some((0, 0)), "and cleared nothing");
        // In the Service view it does its row.
        let mut service = Keyboard::new(home::HomeState { keys, ..home::HomeState::default() });
        service.press(one);
        assert_eq!(service.live(), None, "1 set to Clear All cleared");
    }

    #[test]
    fn in_the_edit_view_period_waits_while_words_or_a_name_are_being_typed() {
        use KeyAction as A;
        let song = parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        let ctx = egui::Context::default();
        let editing = EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        let mut state = home::HomeState { editing, ..home::HomeState::default() };
        let first = state.editing.as_ref().and_then(|draft| draft.slide.objects.first().map(|object| object.id));
        assert!(first.is_some());
        let period = us(&Key::Character(".".into()));
        let f2 = us(&Key::Named(NamedKey::F2));
        let home = |state: &home::HomeState, key: &Pressed| home_key_command(state, &ctx, key, false, &show, true);
        let screen = |state: &home::HomeState, key: &Pressed| show_key_command(state, &ctx, key, false, &show, true);
        let does = |action| Some(KeyPress::Action(action));
        assert_eq!((home(&state, &period), screen(&state, &period)), (does(A::ClearAll), does(A::ClearAll)));
        // A box's words being typed, the field having lost the keyboard to a click on the
        // inspector (Bold, a colour): "." is the typist's next key, never a clear of the room's
        // screen, from the home window or a screen's.
        assert!(!ctx.text_edit_focused());
        if let Some(draft) = state.editing.as_mut() {
            draft.typing = first;
        }
        assert_eq!(home(&state, &period), None, "typing: the home window");
        assert_eq!(screen(&state, &period), None, "typing: a screen's window");
        // The F rows type nothing, and still clear, as before.
        assert_eq!(screen(&state, &f2), does(A::ClearSlide));
        // An object's name being retyped, or a song's, a theme's, a stage layout's.
        if let Some(draft) = state.editing.as_mut() {
            draft.typing = None;
            draft.renaming = Some((sp_model::Uuid::nil(), "Main".to_owned()));
        }
        assert_eq!((home(&state, &period), screen(&state, &period)), (None, None), "an object's name");
        if let Some(draft) = state.editing.as_mut() {
            draft.renaming = None;
        }
        state.stage_rename = Some((0, "Words".to_owned()));
        assert_eq!((home(&state, &period), screen(&state, &period)), (None, None), "a stage layout's name");
        state.stage_rename = None;
        state.renaming_song = Some("Song".to_owned());
        assert_eq!(screen(&state, &period), None, "a song's name");
        state.renaming_song = None;
        // Nothing typed: it clears again.
        assert_eq!(screen(&state, &period), does(A::ClearAll));
    }

    #[test]
    fn a_letter_and_a_period_in_one_frame_in_the_bible_view_type_the_reference() {
        // German "1. Mose": the letters wait for the window to draw, so Scripture Lookup has the
        // keyboard before "." comes, which then types instead of clearing the screen.
        let mut bible = Keyboard::new(home::HomeState { bible_open: true, ..home::HomeState::default() });
        bible.press(Key::Character("x".into()));
        bible.press(Key::Character(".".into()));
        bible.draw();
        assert_eq!(bible.state.bible.lookup, "x.", "both typed");
        assert_eq!(bible.live(), Some((0, 0)), "the screen was not cleared");
        // A character waiting to be drawn holds back a key that may run the show.
        let show = Show::new(vec![Arc::new(parse_song_text("Song", "One\n"))]);
        let text = [egui::Event::Text("x".to_owned())];
        let period = us(&Key::Character(".".into()));
        assert!(key_waits_for_drawing(&text, false, &period, false, &KeyTable::default(), &show, true));
        assert!(!key_waits_for_drawing(&[], false, &period, false, &KeyTable::default(), &show, true));
    }
}
