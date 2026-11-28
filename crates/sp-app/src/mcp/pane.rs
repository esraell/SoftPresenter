//! Settings › AI assistants (MCP): how much an assistant may do, over the two switches the app
//! checks every request from an assistant against (`policy`), and a tile for each company that
//! adds SoftPresenter to its apps found on this computer with one click (`connect`), or copies a
//! message to paste into the assistant: how to connect, with this computer's path to `sp` filled
//! in, and how to use SoftPresenter.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use egui::{Rect, Sense, pos2, vec2};

use super::Switches;
use super::connect::{self, App, Company, Done, Place, Stands};
use crate::cli_install::Tone;
use crate::home::settings::parts::{self, Pick, Picture};
use crate::i18n::{t, tf};
use crate::logos::{self, Logo, Mark};
use crate::quick_help::Hint;

/// The card's state: the switches (kept in `ai-assistants.json`), the assistants' tiles, and
/// what was said last.
#[derive(Debug, Default)]
pub struct View {
    pub switches: Switches,
    /// What the last click said (Copy setup, Add or Remove, or a switch that couldn't be
    /// saved): its tone, and the words.
    said: Option<(Tone, String)>,
    /// The company whose tile's Copy setup has just copied its message (its key says Copied
    /// until the page comes into view again or another key is pressed).
    copied: Option<Company>,
    /// The company whose message What gets copied shows (Claude's until one is copied).
    shown: Option<Company>,
    /// Open What gets copied when next drawn (once, after a copy).
    reveal_copied: bool,
    /// The program an assistant starts, found again each time the card comes into view.
    program: Option<PathBuf>,
    /// The pass the card was last drawn in, to know when it has just come into view.
    drawn: Option<u64>,
    /// Scroll the page to its top when next drawn.
    pub reveal: bool,
    /// The part of the page to open and bring into view when next drawn (a capture run's,
    /// `SOFTPRESENTER_CAPTURE_PREFERENCES=mcp-tiles`, `mcp-apps` or `mcp-copied`).
    pub show_part: Option<Part>,
    /// One tile for each company (Claude, ChatGPT, Gemini, Grok), then one for each other app
    /// (under More).
    tiles: Vec<Tile>,
}

/// A part of the page a capture run brings into view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The assistants' tiles (Connect one).
    Tiles,
    /// Other apps, opened.
    OtherApps,
    /// What gets copied, opened.
    WhatIsCopied,
}

/// A tile: a company's apps, or one other app.
#[derive(Debug)]
struct Tile {
    company: Option<Company>,
    /// The apps it covers that are made for this system.
    apps: Vec<App>,
    /// Those found on this computer, each with where SoftPresenter stands once looked at.
    found: Vec<(App, Place, Option<Stands>)>,
    /// What the last click did, in place of where it stands.
    said: Option<(Tone, String)>,
    /// A look or a click under way, on a thread of its own (never the window's: an app's own
    /// command can take a moment): what it will find.
    job: Option<Receiver<Answer>>,
}

/// What a tile's thread found.
#[derive(Debug)]
enum Answer {
    /// Where SoftPresenter stands in each app found, when the card came into view.
    Stands(Vec<Stands>),
    /// What a click did to each app, and where it stands in each after it.
    Clicked(Vec<(App, Result<Done, String>)>, Vec<Stands>),
}

/// A click on one of a tile's keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    Add,
    Replace,
    Remove,
    /// Copy setup: the message to paste into the assistant.
    Copy,
    /// Get Claude Code ↗: its page, in the browser.
    Get,
}

impl View {
    /// The switches as last saved, both off when nothing was.
    pub fn load() -> View {
        View { switches: Switches::load(), ..View::default() }
    }
}

/// A path as a shell takes it: as it is when it holds nothing a shell reads, else quoted (in
/// single quotes on a Mac and Linux, double on Windows).
pub(super) fn shell_word(path: &Path, windows: bool) -> String {
    let text = path.to_string_lossy();
    let plain = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '\\' | ':'));
    match (plain, windows) {
        (true, _) => text.into_owned(),
        (false, true) => format!("\"{text}\""),
        (false, false) => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// The line that adds SoftPresenter to Claude Code, for every folder (what its Add runs, as
/// words to type).
pub fn claude_code_line(program: &Path, windows: bool) -> String {
    format!("claude mcp add --scope user softpresenter -- {} mcp", shell_word(program, windows))
}

/// What goes in Claude Desktop's configuration file (and Cursor's and Antigravity's), in the
/// order people write it (the command, then its arguments), the path written as a JSON string.
pub fn claude_desktop_config(program: &Path) -> String {
    let command = serde_json::to_string(&program.to_string_lossy()).unwrap_or_default();
    format!(
        "{{\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"command\": {command},\n      \"args\": [\"mcp\"]\n    }}\n  }}\n}}"
    )
}

/// What goes in Codex's `config.toml` (which the ChatGPT app shares) and Grok Build's: the path
/// as a TOML string (a JSON string is one).
pub fn toml_config(program: &Path) -> String {
    let command = serde_json::to_string(&program.to_string_lossy()).unwrap_or_default();
    format!("[mcp_servers.softpresenter]\ncommand = {command}\nargs = [\"mcp\"]")
}

/// Settings › AI assistants, as the design has it (the owner, 2026-11-25: "make it simple, like a
/// click to copy, and have claude chatgpt grok and gemini and the copy will copy the how to
/// connect mcp and explanation to the agent on how to use it"). How much an assistant may do, as
/// one choice of three over the two switches, with what Prepare allows and what is never allowed;
/// then a tile for each company with its logo, where SoftPresenter stands in its apps here, Add
/// (Remove once added, Replace where another program has the entry) and Copy setup; a line saying
/// what the last click did; the other apps under Other apps; and what Copy setup copies.
pub fn page(ui: &mut egui::Ui, view: &mut View) {
    // Just come into view: everything is looked at again, and what was said is forgotten.
    let pass = ui.ctx().cumulative_pass_nr();
    if view.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        came_into_view(ui.ctx(), view);
    }
    view.drawn = Some(pass);
    for tile in &mut view.tiles {
        if let Some(said) = collect(tile) {
            view.said = Some(said);
        }
    }
    if std::mem::take(&mut view.reveal) {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    parts::card(
        ui,
        t("mcp.level.title", "How much it may do"),
        t("mcp.level.about", "Nothing works until you choose"),
        None,
        |ui| level(ui, view),
    );
    if view.show_part == Some(Part::Tiles) {
        view.show_part = None;
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    parts::card(
        ui,
        t("mcp.connect.title", "Connect one"),
        t("mcp.connect.about", "Add it on this computer, or copy a message to paste in"),
        None,
        |ui| connect_one(ui, view),
    );
}

/// How much an assistant may do: Off, Prepare, or Prepare and run the screens, over the app's
/// two switches (Off turns both off, so switching on again never brings back the screens by
/// itself); then what Prepare allows, and what is never allowed.
fn level(ui: &mut egui::Ui, view: &mut View) {
    let levels = [
        (
            Picture::Level(0),
            t("mcp.level.off", "Off"),
            t("mcp.level.off.about", "Nothing can connect"),
            t("quickhelp.mcp.off", "No AI assistant can connect."),
        ),
        (
            Picture::Level(1),
            t("mcp.level.prepare", "Prepare"),
            t("mcp.level.prepare.about", "Songs, Bible and playlists. The screens stay as they are"),
            t(
                "quickhelp.mcp.prepare",
                "An assistant can find songs and passages and prepare playlists. It can\u{2019}t change the screens.",
            ),
        ),
        (
            Picture::Level(2),
            t("mcp.level.run", "Prepare and run the screens"),
            t("mcp.level.run.about", "It asks you first, every time"),
            t("quickhelp.mcp.run", "An assistant can also change what is on screen, asking you each time."),
        ),
    ]
    .map(|(picture, name, line, hint)| Pick {
        picture,
        name: name.to_owned(),
        line: line.to_owned(),
        hint: hint.to_owned(),
    });
    let now = match (view.switches.allowed, view.switches.screens) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => 2,
    };
    let before = view.switches;
    if let (Some(chosen), _) = parts::picks(ui, "ai level", &levels, Some(now), 112.0) {
        view.switches = Switches { allowed: chosen > 0, screens: chosen == 2 };
    }
    if view.switches != before {
        let words = |on: bool| if on { "on" } else { "off" };
        crate::applog::event(
            "mcp",
            &format!(
                "AI assistants switched {}; changing the screens {}",
                words(view.switches.allowed),
                words(view.switches.screens)
            ),
        );
        if let Err(problem) = view.switches.save() {
            let words = tf("mcp.save_failed", "Could not save the setting: {error}", &[("error", &problem)]);
            view.said = Some((Tone::Trouble, words));
        }
    }
    let can = [
        t("mcp.can.songs", "Find and read your songs"),
        t("mcp.can.bible", "Look up and search the Bible"),
        t("mcp.can.make", "Make songs and playlists, and add to a playlist"),
    ];
    let never = [
        t("mcp.never.delete", "Delete anything, or open files and web pages"),
        t("mcp.never.settings", "Change Settings, keys or the phone remote"),
        t("mcp.never.quit", "Quit SoftPresenter"),
    ];
    parts::two_lists(ui, (t("mcp.can", "With Prepare it can"), &can), (t("mcp.never", "It can never"), &never));
}

/// The Connect one card's insides: a tile for each company, the line under them, Other apps, and
/// what Copy setup copies.
fn connect_one(ui: &mut egui::Ui, view: &mut View) {
    const BETWEEN: f32 = 8.0;
    let ctx = ui.ctx().clone();
    let program = view.program.clone();
    let width = ui.available_width();
    // In the design's order: Claude, ChatGPT, Grok, Gemini.
    let mut companies: Vec<usize> = (0..view.tiles.len()).filter(|at| view.tiles[*at].company.is_some()).collect();
    let place_of = |company: Option<Company>| match company {
        Some(Company::Claude) => 0,
        Some(Company::ChatGpt) => 1,
        Some(Company::Grok) => 2,
        _ => 3,
    };
    companies.sort_by_key(|at| place_of(view.tiles[*at].company));
    // As many across as fit 170 each.
    let across = (((width + BETWEEN) / (170.0 + BETWEEN)).floor() as usize).clamp(1, companies.len().max(1));
    let each = (width - BETWEEN * (across - 1) as f32) / across as f32;
    let looks: Vec<TileLook> = companies.iter().map(|at| tile_look(&view.tiles[*at], view.copied)).collect();
    let tallest = looks.iter().map(|look| tile_height(ui, look, each)).fold(124.0, f32::max);
    let rows = companies.len().div_ceil(across);
    ui.add_space(4.0);
    let (grid, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (tallest + BETWEEN) - BETWEEN), Sense::hover());
    for (place, (at, look)) in companies.iter().zip(&looks).enumerate() {
        let corner =
            grid.min + vec2((place % across) as f32 * (each + BETWEEN), (place / across) as f32 * (tallest + BETWEEN));
        let clicked = draw_tile(ui, Rect::from_min_size(corner, vec2(each, tallest)), look, *at);
        match (clicked, view.tiles[*at].company, program.as_deref()) {
            (Some(Click::Copy), Some(company), Some(program)) => {
                ui.ctx().copy_text(setup_message(company, program));
                view.copied = Some(company);
                view.shown = Some(company);
                view.reveal_copied = true;
                let mut said =
                    tf("mcp.copied_into", "Copied. Paste it into {assistant}.", &[("assistant", company.name())]);
                if !view.switches.allowed {
                    said = format!("{said} {}", t("mcp.after_prepare", "It can connect once you choose Prepare."));
                }
                view.said = Some((Tone::Good, said));
            }
            (Some(click), _, Some(program)) => {
                view.copied = None;
                start_click(&ctx, &mut view.tiles[*at], program, click);
            }
            _ => {}
        }
    }
    ui.add_space(12.0);
    // What the last click did, and that only the apps on this computer can connect.
    let said = view.said.as_ref().map(|(tone, words)| (*tone, words.as_str()));
    parts::said_line(ui, said, connect::internet_only_words());
    // The other apps SoftPresenter can add itself to.
    let others: Vec<usize> = (0..view.tiles.len()).filter(|at| view.tiles[*at].company.is_none()).collect();
    let reveal = view.show_part == Some(Part::OtherApps);
    if reveal {
        view.show_part = None;
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    let mut pressed = None;
    parts::fold(ui, "mcp other apps", t("mcp.other_apps", "Other apps"), reveal, |ui| {
        // A row's keys 8 apart, as the design's rows have them.
        const BETWEEN: f32 = 8.0;
        for at in &others {
            let look = tile_look(&view.tiles[*at], view.copied);
            // Add (or Remove, or Replace) where it applies, and Copy setup: the settings to add
            // by hand, as these apps keep them. Each key as wide as its own words.
            let copy = t("mcp.copy_setup", "Copy setup");
            let widths: Vec<f32> = look.keys.iter().map(|(_, words)| parts::key_size(ui, words).x).collect();
            let copy_width = parts::key_size(ui, copy).x;
            let size = vec2(widths.iter().map(|wide| wide + BETWEEN).sum::<f32>() + copy_width, 30.0);
            parts::row(ui, &look.name, &look.state.1, size, |ui, place| {
                let mut x = place.min.x;
                for ((click, words), wide) in look.keys.iter().zip(&widths) {
                    let rect = Rect::from_min_size(pos2(x, place.min.y), vec2(*wide, 30.0));
                    let id = ui.scope_id().with(("other app", *at, *words));
                    if parts::key(ui, rect, id, words, *click == Click::Remove).hint(&look.state.1).clicked() {
                        pressed = Some((*at, *click));
                    }
                    x += wide + BETWEEN;
                }
                let rect = Rect::from_min_size(pos2(x, place.min.y), vec2(copy_width, 30.0));
                let hint = t(
                    "quickhelp.mcp.copy",
                    "Copy the setup that adds SoftPresenter to this app, to paste in yourself.",
                );
                if parts::key(ui, rect, ui.scope_id().with(("other app copy", *at)), copy, false).hint(hint).clicked() {
                    pressed = Some((*at, Click::Copy));
                }
            });
        }
    });
    match (pressed, program.as_deref()) {
        (Some((at, Click::Copy)), Some(program)) => {
            let setup = view.tiles[at]
                .apps
                .iter()
                .map(|app| connect::app_setup(*app, program))
                .collect::<Vec<_>>()
                .join("\n\n");
            ui.ctx().copy_text(setup);
            view.said = Some((Tone::Good, t("mcp.copied", "Copied.").to_owned()));
        }
        (Some((at, click)), Some(program)) => {
            view.copied = None;
            start_click(&ctx, &mut view.tiles[at], program, click);
        }
        _ => {}
    }
    // What Copy setup copies: the last one copied, or Claude's.
    let shown = view.shown.unwrap_or(Company::Claude);
    let asked = view.show_part == Some(Part::WhatIsCopied);
    if asked {
        view.show_part = None;
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    let open = std::mem::take(&mut view.reveal_copied) || asked;
    parts::fold(ui, "mcp what gets copied", t("mcp.what_copied", "What gets copied"), open, |ui| {
        if let Some(program) = program.as_deref() {
            parts::code(ui, &setup_message(shown, program));
        }
    });
}

/// How a tile looks now: its name and logo, the state line, and its keys.
struct TileLook {
    name: String,
    company: Option<Company>,
    state: (Tone, String),
    /// Added to every app found: the line has a green light.
    added: bool,
    /// The keys that fit where SoftPresenter stands, with their words: Replace where another
    /// program has the entry, Add while an app hasn't got it, Remove (quiet) once one has; none
    /// while a look or a click is under way, or where nothing is found.
    keys: Vec<(Click, &'static str)>,
    /// Copy setup has just copied this tile's message.
    copied: bool,
    /// Claude's tile while Claude Code isn't found here: a key to its page.
    get_claude_code: bool,
}

fn tile_look(tile: &Tile, copied: Option<Company>) -> TileLook {
    let busy = tile.job.is_some();
    let known: Option<Vec<Stands>> = tile.found.iter().map(|(_, _, stands)| stands.clone()).collect();
    let names: Vec<&str> = tile.found.iter().map(|(app, _, _)| app.name()).collect();
    let any = |wanted: fn(&Stands) -> bool| known.as_ref().is_some_and(|known| known.iter().any(wanted));
    let added = known.as_ref().is_some_and(|known| !known.is_empty() && known.iter().all(|s| *s == Stands::Added));
    let name = tile.company.map_or_else(|| tile.apps.first().map_or("", |app| app.name()), Company::name).to_owned();
    let state = if busy {
        (Tone::Plain, t("mcp.looking", "Looking\u{2026}").to_owned())
    } else if tile.found.is_empty() {
        (Tone::Plain, for_words(tile))
    } else if added {
        (Tone::Good, tf("mcp.added_to", "Added to {apps}", &[("apps", &connect::and_list(&names))]))
    } else if any(|s| matches!(s, Stands::Other { .. } | Stands::Trouble(_) | Stands::Added)) {
        // Partly added, an entry another program has, or trouble: said in the backend's words.
        let found: Vec<(App, Place, Stands)> = tile
            .found
            .iter()
            .filter_map(|(app, place, stands)| stands.clone().map(|stands| (*app, place.clone(), stands)))
            .collect();
        connect::group_words(&found, false)
    } else {
        (Tone::Plain, tf("mcp.found", "{apps} found", &[("apps", &connect::and_list(&names))]))
    };
    let mut keys = Vec::new();
    if !busy && !tile.found.is_empty() && known.is_some() {
        if any(|s| matches!(s, Stands::Other { .. })) {
            keys.push((Click::Replace, t("mcp.replace", "Replace")));
        }
        if any(|s| matches!(s, Stands::Missing | Stands::Trouble(_))) {
            keys.push((Click::Add, t("mcp.add", "Add")));
        }
        if any(|s| *s == Stands::Added) {
            keys.push((Click::Remove, t("mcp.remove", "Remove")));
        }
    }
    let get_claude_code =
        tile.company == Some(Company::Claude) && !busy && !tile.found.iter().any(|(app, _, _)| *app == App::ClaudeCode);
    let copied = tile.company.is_some() && tile.company == copied;
    TileLook { name, company: tile.company, state, added, keys, copied, get_claude_code }
}

/// What a tile says while none of its apps is on this computer, as the design words it: Claude's
/// and ChatGPT's say "the desktop app" where there is one for this system; the others name their
/// apps.
fn for_words(tile: &Tile) -> String {
    let desktop = cfg!(any(target_os = "macos", windows));
    match tile.company {
        Some(Company::Claude) if tile.apps.contains(&App::ClaudeDesktop) => {
            t("mcp.for.claude", "For the desktop app or Claude Code").to_owned()
        }
        Some(Company::ChatGpt) if desktop => t("mcp.for.chatgpt", "For the desktop app or Codex").to_owned(),
        Some(Company::ChatGpt) => tf("mcp.for", "For {apps}", &[("apps", "Codex")]),
        _ => {
            let apps: Vec<&str> = tile.apps.iter().map(|app| app.name()).collect();
            tf("mcp.for", "For {apps}", &[("apps", &connect::and_list(&apps))])
        }
    }
}

/// The room between two of a tile's keys, across and down, as the design's.
const KEYS_BETWEEN: f32 = 6.0;

/// Get Claude Code's words, with the arrow of the design's keys that open a web page: their
/// short form where the usual would make the key wider than `room` (a tile's inside).
fn get_claude_code_words(ui: &egui::Ui, room: f32) -> String {
    const ARROW: &str = " \u{2197}";
    let font = egui::FontId::proportional(12.5);
    let words_room = room - parts::key_size(ui, ARROW).x;
    let (key, english) = crate::i18n::tk("mcp.get_claude_code", "Get Claude Code");
    let words = crate::studio::within(ui, key, english, font, 0.0, words_room, false);
    format!("{words}{ARROW}")
}

/// How wide Copy setup is: as wide as Copied with its light, or Copy setup, whichever is longer.
fn copy_width(ui: &egui::Ui) -> f32 {
    let font = egui::FontId::proportional(12.5);
    let copied = crate::studio::widest(ui, &[t("mcp.copied_key", "Copied")], &font) + 14.0;
    crate::studio::widest(ui, &[t("mcp.copy_setup", "Copy setup")], &font).max(copied) + 26.0
}

/// A tile's keys, row by row: those that apply, then Copy setup, then on Claude's tile while
/// Claude Code isn't here, Get Claude Code ↗; side by side while they fit, the rest on the next
/// row, as the design's keys wrap. Each is as wide as its own words (Copy setup as the longer of
/// Copy setup and Copied with its light).
fn tile_keys(ui: &egui::Ui, look: &TileLook, room: f32) -> Vec<Vec<(Click, String, f32)>> {
    let copy = if look.copied { t("mcp.copied_key", "Copied") } else { t("mcp.copy_setup", "Copy setup") };
    let mut keys: Vec<(Click, String, f32)> =
        look.keys.iter().map(|(click, words)| (*click, (*words).to_owned(), parts::key_size(ui, words).x)).collect();
    keys.push((Click::Copy, copy.to_owned(), copy_width(ui)));
    if look.get_claude_code {
        let words = get_claude_code_words(ui, room);
        let wide = parts::key_size(ui, &words).x;
        keys.push((Click::Get, words, wide));
    }
    let mut rows: Vec<Vec<(Click, String, f32)>> = Vec::new();
    let mut used = 0.0;
    for key in keys {
        match rows.last_mut() {
            Some(row) if used + KEYS_BETWEEN + key.2 <= room => {
                used += KEYS_BETWEEN + key.2;
                row.push(key);
            }
            _ => {
                used = key.2;
                rows.push(vec![key]);
            }
        }
    }
    rows
}

/// How tall a tile must be at `width` for its words and keys.
fn tile_height(ui: &egui::Ui, look: &TileLook, width: f32) -> f32 {
    use crate::studio::{TEXT, TEXT_DIM, semibold};
    let text = width - 28.0;
    let name = parts::words(ui, &look.name, semibold(16.0), TEXT, 0.0, text);
    let state_room = text - if look.added { 14.0 } else { 0.0 };
    let state = parts::words(ui, &look.state.1, egui::FontId::proportional(12.0), TEXT_DIM, 0.0, state_room);
    // Room for two lines of it whatever it says now, so a tile doesn't grow when it changes.
    let state_height = state.size().y.max(2.0 * state.size().y / state.rows.len().max(1) as f32);
    let rows = tile_keys(ui, look, text).len() as f32;
    14.0 + 36.0 + 10.0 + name.size().y + 3.0 + state_height + 14.0 + rows * 30.0 + (rows - 1.0) * KEYS_BETWEEN + 14.0
}

/// A company's tile in `rect`: its logo (its initials until there is one), its name, where
/// SoftPresenter stands in its apps here, and its keys at the foot. The key clicked, if one was.
fn draw_tile(ui: &mut egui::Ui, rect: Rect, look: &TileLook, at: usize) -> Option<Click> {
    use crate::studio::{LINE, PANEL, RADIUS_CARD, READY, TEXT, TEXT_DIM, TEXT_FAINT, semibold};
    let hovered = ui.rect_contains_pointer(rect);
    ui.painter().rect_filled(rect, RADIUS_CARD, PANEL);
    let edge = if hovered { TEXT_FAINT } else { LINE };
    ui.painter().rect_stroke(rect, RADIUS_CARD, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    let inner = rect.shrink(14.0);
    let logo = Rect::from_min_size(inner.min, vec2(36.0, 36.0));
    let initials = Mark::initials_of(&look.name);
    let mark = match look.company.and_then(logo_of) {
        Some(logo) => Mark::Logo(logo),
        None => Mark::Initials(&initials),
    };
    logos::tile(ui, logo, mark, crate::studio::CARD);
    let name = parts::words(ui, &look.name, semibold(16.0), TEXT, 0.0, inner.width());
    let mut y = logo.max.y + 10.0;
    let name_height = name.size().y;
    ui.painter().galley(pos2(inner.min.x, y), name, TEXT);
    y += name_height + 3.0;
    let mut x = inner.min.x;
    if look.added {
        crate::studio::paint_light(ui.painter(), pos2(x + 3.5, y + 8.0), Some(READY));
        x += 14.0;
    }
    let colour = match look.state.0 {
        Tone::Trouble => crate::studio::TALLY,
        Tone::Care => crate::studio::CARE,
        _ => TEXT_DIM,
    };
    let state = parts::words(ui, &look.state.1, egui::FontId::proportional(12.0), colour, 0.0, inner.max.x - x);
    ui.painter().galley(pos2(x, y), state, colour);
    // The keys, from the foot up.
    let rows = tile_keys(ui, look, inner.width());
    let mut clicked = None;
    let mut top = inner.max.y - rows.len() as f32 * (30.0 + KEYS_BETWEEN) + KEYS_BETWEEN;
    for row in rows {
        let mut x = inner.min.x;
        for (click, words, wide) in row {
            let place = Rect::from_min_size(pos2(x, top), vec2(wide, 30.0));
            let id = ui.scope_id().with(("ai tile", at, click as u8));
            x += wide + KEYS_BETWEEN;
            let response = if click == Click::Copy && look.copied {
                parts::lit_key(ui, place, id, &words)
            } else {
                parts::key(ui, place, id, &words, click == Click::Remove)
            };
            // Claude Code isn't here: its page, where to get it.
            if click == Click::Get {
                if response.hint(connect::GET_CLAUDE_CODE).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(connect::GET_CLAUDE_CODE));
                }
                continue;
            }
            let hint = match click {
                Click::Add => t(
                    "mcp.add.hint",
                    "Adds SoftPresenter to its apps on this computer, changing nothing else in their settings",
                ),
                Click::Replace => t("mcp.replace.hint", "Points the entry at this copy of SoftPresenter"),
                Click::Remove => t("mcp.remove.hint", "Takes SoftPresenter out again, and nothing else"),
                Click::Copy => t(
                    "mcp.copy_setup.hint",
                    "Copies a message to paste into it: how to connect, and how to use SoftPresenter",
                ),
                Click::Get => connect::GET_CLAUDE_CODE,
            };
            if response.hint(hint).clicked() {
                clicked = Some(click);
            }
        }
        top += 30.0 + KEYS_BETWEEN;
    }
    clicked
}

/// The logo a company's tile shows, where it has one (docs/licences/logos.md); initials where not.
fn logo_of(company: Company) -> Option<Logo> {
    match company {
        Company::Claude => Some(Logo::Claude),
        Company::ChatGpt => Some(Logo::ChatGpt),
        Company::Grok => Some(Logo::Grok),
        // Google offers Gemini's icon only through a form behind a sign-in: initials until then.
        Company::Gemini => None,
    }
}

/// Starts a click's work on the tile's own thread (an app's own command can take a moment).
fn start_click(ctx: &egui::Context, tile: &mut Tile, program: &Path, click: Click) {
    // Copy setup copies and Get Claude Code opens a page; neither changes an app's settings.
    if matches!(click, Click::Copy | Click::Get) {
        return;
    }
    let places: Vec<(App, Place)> = tile.found.iter().map(|(app, place, _)| (*app, place.clone())).collect();
    let program = program.to_owned();
    tile.said = None;
    tile.job = Some(on_a_thread(ctx, move || {
        let done = match click {
            Click::Add => connect::add_all(&places, &program, false),
            Click::Copy | Click::Get => Vec::new(),
            Click::Replace => connect::add_all(&places, &program, true),
            Click::Remove => connect::remove_ours(&places, &program),
        };
        let stands = places.iter().map(|(app, place)| connect::status(*app, place, &program)).collect();
        Answer::Clicked(done, stands)
    }));
}

/// What Copy setup copies for a company: a message to paste into its assistant, written to the
/// assistant, saying how to connect (with this computer's path to `sp` filled in, quoted as a
/// shell takes it) and how to use SoftPresenter. In English, as everything an assistant is told is.
pub fn setup_message(company: Company, program: &Path) -> String {
    let word = shell_word(program, cfg!(windows));
    let json = claude_desktop_config(program);
    let connect = match company {
        Company::Claude => {
            let code = format!("In Claude Code, run:  {}", claude_code_line(program, cfg!(windows)));
            if company.apps().contains(&App::ClaudeDesktop) {
                format!(
                    "{code}\nIn Claude Desktop, which can't run that, tell me to add this to claude_desktop_config.json and open Claude Desktop again:\n{json}"
                )
            } else {
                code
            }
        }
        Company::ChatGpt => format!(
            "Run:  codex mcp add softpresenter -- {word} mcp\nThe ChatGPT desktop app, the Codex CLI and its IDE extension share that configuration."
        ),
        Company::Grok => format!("In Grok Build, run:  grok mcp add -s user softpresenter -- {word} mcp"),
        Company::Gemini => format!(
            "In Antigravity (which took Gemini CLI's place), add this under mcpServers in ~/.gemini/config/mcp_config.json, leaving the rest of the file as it is:\n{json}"
        ),
    };
    format!(
        "Connect to SoftPresenter, the church presentation app running on this computer. It has an MCP server.

To connect
{connect}
If that doesn't match your version: it is a local MCP server over stdio. The command is {word} and its one argument is mcp.

How to use it
- Its tools find and read songs, look up and search the Bible, list and make songs and playlists, and add to a playlist. Use them to get a service ready with me.
- Changing what is on the screens (showing a song, an item or a passage, next, previous, clear, a message) works only when I have allowed it in SoftPresenter's Settings. Ask me first, every time, and say the words that will go up.
- You can't delete anything, change its settings, open files or web pages, or quit it. If a tool is refused, tell me what it said.
- Start by asking it what is on screen and which playlist is open."
    )
}

/// The card has just come into view: the program and each app's place are found again (quick:
/// a few files looked at), and each tile's apps found are asked where SoftPresenter stands, on
/// a thread of its own.
fn came_into_view(ctx: &egui::Context, view: &mut View) {
    view.program = connect::program();
    view.said = None;
    view.copied = None;
    // A tile with a click under way keeps it, and the answer it will bring.
    let mut busy: Vec<Tile> = view.tiles.drain(..).filter(|tile| tile.job.is_some()).collect();
    let groups: Vec<(Option<Company>, Vec<App>)> = Company::ALL
        .into_iter()
        .map(|company| (Some(company), company.apps()))
        .chain(App::others().into_iter().map(|app| (None, vec![app])))
        .collect();
    for (company, apps) in groups {
        if let Some(at) = busy.iter().position(|tile| tile.company == company && tile.apps == apps) {
            view.tiles.push(busy.remove(at));
            continue;
        }
        let found: Vec<(App, Place, Option<Stands>)> =
            apps.iter().filter_map(|app| connect::place(*app).map(|place| (*app, place, None))).collect();
        let job = view.program.clone().filter(|_| !found.is_empty()).map(|program| {
            let places: Vec<(App, Place)> = found.iter().map(|(app, place, _)| (*app, place.clone())).collect();
            on_a_thread(ctx, move || {
                Answer::Stands(places.iter().map(|(app, place)| connect::status(*app, place, &program)).collect())
            })
        });
        view.tiles.push(Tile { company, apps, found, said: None, job });
    }
}

/// Runs `work` on a thread of its own, and redraws the window when it is done.
fn on_a_thread(ctx: &egui::Context, work: impl FnOnce() -> Answer + Send + 'static) -> Receiver<Answer> {
    let (send, receive) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = send.send(work());
        ctx.request_repaint();
    });
    receive
}

/// Takes what a tile's thread found, once it is done: what a click did, in words, when it was a
/// click.
fn collect(tile: &mut Tile) -> Option<(Tone, String)> {
    let Some(job) = &tile.job else { return None };
    let stands = match job.try_recv() {
        Ok(Answer::Stands(stands)) => stands,
        Ok(Answer::Clicked(done, stands)) => {
            for (app, done) in &done {
                let word = match done {
                    Ok(done) => done.log_word(),
                    Err(_) => "not changed",
                };
                crate::applog::event("mcp", &format!("{}: {word}", app.name()));
            }
            let places: Vec<(App, Place)> = tile.found.iter().map(|(app, place, _)| (*app, place.clone())).collect();
            tile.said = Some(connect::group_done_words(&done, &places, false));
            stands
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => return None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
    };
    for ((_, _, now), stands) in tile.found.iter_mut().zip(stands) {
        *now = Some(stands);
    }
    tile.job = None;
    tile.said.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_setup_tells_each_assistant_how_to_connect_and_how_to_use_it() {
        let program = Path::new("/Users/someone/.local/bin/sp");
        for company in Company::ALL {
            let message = setup_message(company, program);
            assert!(message.starts_with("Connect to SoftPresenter"), "{message}");
            assert!(message.contains("The command is /Users/someone/.local/bin/sp and its one argument is mcp."));
            assert!(message.contains("Ask me first, every time"), "the screens need a yes: {message}");
        }
        let line = |company| setup_message(company, program);
        assert!(
            line(Company::Claude)
                .contains("claude mcp add --scope user softpresenter -- /Users/someone/.local/bin/sp mcp")
        );
        assert!(line(Company::ChatGpt).contains("codex mcp add softpresenter -- /Users/someone/.local/bin/sp mcp"));
        assert!(line(Company::Grok).contains("grok mcp add -s user softpresenter -- /Users/someone/.local/bin/sp mcp"));
        assert!(line(Company::Gemini).contains("~/.gemini/config/mcp_config.json"));
        // A path with a space is quoted as this computer's shell takes it: in single quotes on a
        // Mac and Linux, in double quotes on Windows (cmd and PowerShell).
        let (spaced, quoted) = if cfg!(windows) {
            (
                r"C:\Program Files\SoftPresenter\softpresenter.exe",
                r#""C:\Program Files\SoftPresenter\softpresenter.exe""#,
            )
        } else {
            (
                "/Applications/Soft Presenter.app/Contents/MacOS/sp",
                "'/Applications/Soft Presenter.app/Contents/MacOS/sp'",
            )
        };
        let said = setup_message(Company::Grok, Path::new(spaced));
        assert!(said.contains(&format!("grok mcp add -s user softpresenter -- {quoted} mcp")), "{said}");
        assert!(said.contains(&format!("The command is {quoted} and its one argument is mcp.")), "{said}");
    }

    #[test]
    fn a_path_is_quoted_only_when_it_must_be_and_as_each_systems_shell_takes_it() {
        let word = |path: &str, windows| shell_word(Path::new(path), windows);
        assert_eq!(word("/Users/someone/.local/bin/sp", false), "/Users/someone/.local/bin/sp");
        assert_eq!(word(r"C:\Users\someone\sp.exe", true), r"C:\Users\someone\sp.exe");
        assert_eq!(word("/Soft Presenter/sp", false), "'/Soft Presenter/sp'");
        assert_eq!(word("/it's/sp", false), r"'/it'\''s/sp'");
        assert_eq!(word(r"C:\Program Files\sp.exe", true), r#""C:\Program Files\sp.exe""#);
    }

    #[test]
    fn the_page_shows_the_three_choices_and_a_tile_for_each_company() {
        let ctx = crate::studio::tests::styled();
        // Tiles made here, and the page counted as in view, so nothing looks for (or asks) any
        // assistant app on this computer.
        let tiles = Company::ALL
            .into_iter()
            .map(|company| Tile {
                company: Some(company),
                apps: company.apps(),
                found: Vec::new(),
                said: None,
                job: None,
            })
            .collect();
        let mut view = View { tiles, drawn: Some(ctx.cumulative_pass_nr()), ..View::default() };
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| page(ui, &mut view));
        for words in [
            "Off",
            "Prepare",
            "Prepare and run the screens",
            "WITH PREPARE IT CAN",
            "Claude",
            "ChatGPT",
            "Grok",
            "Gemini",
        ] {
            assert!(crate::studio::tests::place_of(&drawn, words).is_some(), "{words}");
        }
        drawn.drop_without_applying_deltas();
    }

    #[test]
    fn a_tiles_keys_are_as_wide_as_their_words_and_share_a_row_where_they_fit() {
        let ctx = crate::studio::tests::styled();
        let look = |keys: Vec<(Click, &'static str)>, get_claude_code| TileLook {
            name: "Claude".to_owned(),
            company: Some(Company::Claude),
            state: (Tone::Plain, String::new()),
            added: false,
            keys,
            copied: false,
            get_claude_code,
        };
        let mut checked = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let add = parts::key_size(ui, "Add").x;
            assert!(add < parts::key_size(ui, "Replace").x, "Add is as wide as its own word");
            // Add and Copy setup side by side where they fit; narrower, one above the other.
            let both = add + KEYS_BETWEEN + copy_width(ui);
            let rows = tile_keys(ui, &look(vec![(Click::Add, "Add")], false), both);
            assert_eq!(rows.len(), 1, "{rows:?}");
            assert_eq!(rows[0][0].2, add);
            assert_eq!(tile_keys(ui, &look(vec![(Click::Add, "Add")], false), both - 1.0).len(), 2);
            // A tile at 1100 has 152 inside, where the design has them side by side. The design
            // is drawn in the Mac's face; a wider one (Linux's) puts them one above the other.
            if cfg!(target_os = "macos") {
                assert!(both <= 152.0, "Add and Copy setup take {both}");
            }
            // Remove alone where it applies, and Get Claude Code ↗ a key of its own after Copy setup.
            let rows = tile_keys(ui, &look(vec![(Click::Remove, "Remove")], true), 152.0);
            let words: Vec<&str> = rows.iter().flatten().map(|(_, words, _)| words.as_str()).collect();
            assert_eq!(words, ["Remove", "Copy setup", "Get Claude Code \u{2197}"]);
            // No key is wider than the tile, in Spanish too (Get Claude Code takes its short form).
            let spanish = crate::i18n::with_language(crate::i18n::Language::Spanish, || {
                tile_keys(ui, &look(vec![(Click::Add, "A\u{f1}adir")], true), 152.0)
            });
            for (_, words, wide) in spanish.iter().flatten() {
                assert!(*wide <= 152.0, "{words} is {wide} wide");
            }
            checked = true;
        })
        .drop_without_applying_deltas();
        assert!(checked);
    }

    #[test]
    fn the_lines_to_copy_name_the_program_safely() {
        let plain = Path::new("/Users/someone/.local/bin/sp");
        assert_eq!(
            claude_code_line(plain, false),
            "claude mcp add --scope user softpresenter -- /Users/someone/.local/bin/sp mcp"
        );
        let spaced = Path::new("/Applications/Soft Presenter.app/Contents/MacOS/softpresenter");
        assert_eq!(
            claude_code_line(spaced, false),
            "claude mcp add --scope user softpresenter -- '/Applications/Soft Presenter.app/Contents/MacOS/softpresenter' mcp"
        );
        let quote = Path::new("/tmp/it's/sp");
        assert_eq!(
            claude_code_line(quote, false),
            "claude mcp add --scope user softpresenter -- '/tmp/it'\\''s/sp' mcp"
        );
        let windows = Path::new(r"C:\Program Files\SoftPresenter\SoftPresenter.exe");
        assert_eq!(
            claude_code_line(windows, true),
            r#"claude mcp add --scope user softpresenter -- "C:\Program Files\SoftPresenter\SoftPresenter.exe" mcp"#
        );
        // The settings are JSON (and TOML) whatever the path holds.
        let config = claude_desktop_config(Path::new("C:\\a \"b\"\\sp.exe"));
        let read: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(read["mcpServers"]["softpresenter"]["command"], "C:\\a \"b\"\\sp.exe");
        assert_eq!(read["mcpServers"]["softpresenter"]["args"], serde_json::json!(["mcp"]));
        assert_eq!(
            toml_config(Path::new("C:\\a \"b\"\\sp.exe")),
            "[mcp_servers.softpresenter]\ncommand = \"C:\\\\a \\\"b\\\"\\\\sp.exe\"\nargs = [\"mcp\"]"
        );
    }
}
