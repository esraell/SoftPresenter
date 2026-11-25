//! Settings › Integrations › AI assistants (MCP): the two switches the app checks every request
//! from an assistant against (`policy`), and adding SoftPresenter to each assistant app found on
//! this computer with one click (`connect`), with the line and the settings to copy, this
//! computer's path to `sp` filled in, for doing it by hand.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use egui::{RichText, Sense, vec2};

use super::Switches;
use super::connect::{self, App, Company, Done, Place, Stands};
use crate::cli_install::Tone;
use crate::i18n::{t, tf};

/// The card's state: the switches (kept in `ai-assistants.json`), the assistants' tiles, and
/// what was said last.
#[derive(Debug, Default)]
pub struct View {
    pub switches: Switches,
    /// What the last Copy setup (or a switch that couldn't be saved) said: whether it went
    /// wrong, and the words.
    said: Option<(bool, String)>,
    /// The program an assistant starts, found again each time the card comes into view.
    program: Option<PathBuf>,
    /// The pass the card was last drawn in, to know when it has just come into view.
    drawn: Option<u64>,
    /// Scroll the pane to the card when next drawn (a capture run's, `SOFTPRESENTER_CAPTURE_PREFERENCES=mcp`).
    pub reveal: bool,
    /// Scroll on to the tiles when next drawn (`SOFTPRESENTER_CAPTURE_PREFERENCES=mcp-apps`),
    /// with More open.
    pub reveal_apps: bool,
    /// One tile for each company (Claude, ChatGPT, Gemini, Grok), then one for each other app
    /// (under More).
    tiles: Vec<Tile>,
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

/// A click on one of a tile's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    Add,
    Replace,
    Remove,
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

/// Draws the card under the Command Line card.
pub fn card(ui: &mut egui::Ui, view: &mut View) {
    use crate::studio::{READY, TALLY, TEXT_DIM, TEXT_FAINT};
    ui.add_space(18.0);
    if std::mem::take(&mut view.reveal) {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    crate::studio::caption(ui, &t("mcp.caption", "AI Assistants (MCP)").to_uppercase());
    let room = ui.available_width().min(640.0);
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.label(t(
            "mcp.about",
            "Let Claude and other AI assistants work with SoftPresenter: find songs and Bible passages, prepare playlists and, if you allow it, run the screens.",
        ));
    });
    ui.add_space(4.0);
    // Just come into view: everything is looked at again, and what was said is forgotten.
    let pass = ui.ctx().cumulative_pass_nr();
    if view.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        came_into_view(ui.ctx(), view);
    }
    view.drawn = Some(pass);
    for tile in &mut view.tiles {
        collect(tile);
    }

    let before = view.switches;
    let mut allowed = view.switches.allowed;
    if ui.checkbox(&mut allowed, t("mcp.allow", "Let AI assistants use SoftPresenter")).changed() {
        view.switches.allowed = allowed;
        // Switched off, the second goes off too, so switching on again never brings back the
        // screens by itself.
        if !allowed {
            view.switches.screens = false;
        }
    }
    detail(
        ui,
        t(
            "mcp.allow.detail",
            "They can read your songs, Bibles and playlists, and make songs and playlists. Nothing on the screens changes.",
        ),
    );
    let mut screens = view.switches.screens;
    ui.add_enabled_ui(view.switches.allowed, |ui| {
        if ui
            .checkbox(&mut screens, t("mcp.screens", "Let them change what's on the screens"))
            .on_disabled_hover_text(t("mcp.screens.disabled", "Let AI assistants use SoftPresenter first"))
            .changed()
        {
            view.switches.screens = screens;
        }
    });
    detail(
        ui,
        t(
            "mcp.screens.detail",
            "The assistant asks you first, each time. Leave this off unless someone is watching it.",
        ),
    );
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
        view.said = view.switches.save().err().map(|problem| {
            (true, tf("mcp.save_failed", "Could not save the setting: {error}", &[("error", &problem)]))
        });
    }

    if let Some(program) = view.program.clone() {
        ui.add_space(6.0);
        if view.reveal_apps {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        // Adding an assistant lets nothing in by itself: the switch above does.
        if !view.switches.allowed {
            line(
                ui,
                Tone::Care,
                t(
                    "mcp.off_note",
                    "An assistant you add here can do nothing until \u{201c}Let AI assistants use SoftPresenter\u{201d} is on.",
                ),
            );
        }
        let ctx = ui.ctx().clone();
        let more = std::mem::take(&mut view.reveal_apps);
        for tile in view.tiles.iter_mut().filter(|tile| tile.company.is_some()) {
            tile_part(ui, &ctx, tile, &program, &mut view.said);
        }
        ui.add_space(8.0);
        faint(ui, connect::internet_only_words());
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new(t("mcp.more", "More")).size(12.0))
            .id_salt("mcp.more")
            .open(more.then_some(true))
            .show(ui, |ui| {
                for tile in view.tiles.iter_mut().filter(|tile| tile.company.is_none()) {
                    tile_part(ui, &ctx, tile, &program, &mut view.said);
                }
            });
    }
    if let Some((trouble, words)) = view.said.clone() {
        ui.add_space(2.0);
        ui.horizontal_top(|ui| {
            let (place, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
            ui.painter().circle_filled(place.center(), 3.5, if trouble { TALLY } else { READY });
            ui.label(RichText::new(words).size(12.0).color(if trouble { TEXT_DIM } else { TEXT_FAINT }));
        });
    }
}

/// The card has just come into view: the program and each app's place are found again (quick:
/// a few files looked at), and each tile's apps found are asked where SoftPresenter stands, on
/// a thread of its own.
fn came_into_view(ctx: &egui::Context, view: &mut View) {
    view.program = connect::program();
    view.said = None;
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

/// Takes what a tile's thread found, once it is done.
fn collect(tile: &mut Tile) {
    let Some(job) = &tile.job else { return };
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
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
    };
    for ((_, _, now), stands) in tile.found.iter_mut().zip(stands) {
        *now = Some(stands);
    }
    tile.job = None;
}

/// A tile: its name and the buttons that fit where SoftPresenter stands in its apps found (Add
/// while one hasn't got it; Replace when an entry by its name starts another program; Remove
/// once one has it), Copy setup always, and a line saying what was found and where it stands,
/// or what the last click did.
fn tile_part(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    tile: &mut Tile,
    program: &Path,
    copied: &mut Option<(bool, String)>,
) {
    let busy = tile.job.is_some();
    let known: Option<Vec<(App, Place, Stands)>> = tile
        .found
        .iter()
        .map(|(app, place, stands)| stands.clone().map(|stands| (*app, place.clone(), stands)))
        .collect();
    let any = |wanted: fn(&Stands) -> bool| known.as_ref().is_some_and(|known| known.iter().any(|(_, _, s)| wanted(s)));
    let add = !tile.found.is_empty() && any(|s| matches!(s, Stands::Missing | Stands::Trouble(_)));
    let replace = any(|s| matches!(s, Stands::Other { .. }));
    let remove = any(|s| *s == Stands::Added);
    let name = tile.company.map_or_else(|| tile.apps.first().map_or("", |app| app.name()), Company::name);
    let setup = match tile.company {
        Some(company) => company.setup(program),
        None => tile.apps.iter().map(|app| connect::app_setup(*app, program)).collect::<Vec<_>>().join("\n\n"),
    };
    let mut clicked = None;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).strong());
        ui.add_space(6.0);
        ui.add_enabled_ui(!busy, |ui| {
            let names: Vec<&str> = tile.found.iter().map(|(app, _, _)| app.name()).collect();
            if add
                && ui
                    .button(t("mcp.add", "Add"))
                    .on_hover_text(tf(
                        "mcp.add.hint",
                        "Adds SoftPresenter to {apps}, changing nothing else in their settings",
                        &[("apps", &names.join(", "))],
                    ))
                    .clicked()
            {
                clicked = Some(Click::Add);
            }
            if replace
                && ui
                    .button(t("mcp.replace", "Replace"))
                    .on_hover_text(t("mcp.replace.hint", "Points the entry at this copy of SoftPresenter"))
                    .clicked()
            {
                clicked = Some(Click::Replace);
            }
            if remove
                && ui
                    .button(t("mcp.remove", "Remove"))
                    .on_hover_text(t("mcp.remove.hint", "Takes SoftPresenter out again, and nothing else"))
                    .clicked()
            {
                clicked = Some(Click::Remove);
            }
        });
        if ui
            .add(
                egui::Button::new(RichText::new(t("mcp.copy_setup", "Copy setup")).color(crate::studio::TEXT_DIM))
                    .frame(false),
            )
            .on_hover_text(t(
                "mcp.copy_setup.hint",
                "Copies the settings to add by hand, with this computer\u{2019}s path to sp",
            ))
            .clicked()
        {
            ui.ctx().copy_text(setup);
            *copied = Some((false, t("mcp.copied", "Copied.").to_owned()));
        }
    });
    if let Some(click) = clicked {
        let places: Vec<(App, Place)> = tile.found.iter().map(|(app, place, _)| (*app, place.clone())).collect();
        let program = program.to_owned();
        tile.said = None;
        tile.job = Some(on_a_thread(ctx, move || {
            let done = match click {
                Click::Add => connect::add_all(&places, &program, false),
                Click::Replace => connect::add_all(&places, &program, true),
                Click::Remove => connect::remove_ours(&places, &program),
            };
            let stands = places.iter().map(|(app, place)| connect::status(*app, place, &program)).collect();
            Answer::Clicked(done, stands)
        }));
    }
    let said = if tile.job.is_some() {
        Some((Tone::Plain, t("mcp.looking", "Looking\u{2026}").to_owned()))
    } else if tile.found.is_empty() {
        Some((Tone::Plain, connect::not_found_words().to_owned()))
    } else {
        tile.said.clone().or_else(|| known.as_ref().map(|known| connect::group_words(known, false)))
    };
    if let Some((tone, words)) = said {
        line(ui, tone, &words);
    }
    if tile.company == Some(Company::Claude) && !tile.found.iter().any(|(app, _, _)| *app == App::ClaudeCode) {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.hyperlink_to(
                RichText::new(t("mcp.get_claude_code", "Get Claude Code")).size(11.5),
                connect::GET_CLAUDE_CODE,
            );
        });
    }
}

/// A line saying where something stands, or what a click did, beside a dot of its colour.
fn line(ui: &mut egui::Ui, tone: Tone, words: &str) {
    use crate::studio::{CARE, READY, TALLY, TEXT_DIM, TEXT_FAINT};
    let colour = match tone {
        Tone::Good => READY,
        Tone::Plain => TEXT_FAINT,
        Tone::Care => CARE,
        Tone::Trouble => TALLY,
    };
    ui.add_space(2.0);
    let room = (ui.available_width() - 14.0).min(626.0);
    ui.horizontal_top(|ui| {
        let (place, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
        ui.painter().circle_filled(place.center(), 3.5, colour);
        ui.vertical(|ui| {
            ui.set_max_width(room);
            ui.label(RichText::new(words).size(12.0).color(TEXT_DIM));
        });
    });
}

/// Smaller, fainter words, wrapped.
fn faint(ui: &mut egui::Ui, words: &str) {
    let room = ui.available_width().min(640.0);
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.label(RichText::new(words).size(11.5).color(crate::studio::TEXT_FAINT));
    });
    ui.add_space(2.0);
}

/// The smaller words under a switch, lined up with its words and wrapped.
fn detail(ui: &mut egui::Ui, words: &str) {
    let room = (ui.available_width() - 24.0).min(616.0);
    ui.horizontal_top(|ui| {
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_max_width(room);
            ui.label(RichText::new(words).size(11.5).color(crate::studio::TEXT_DIM));
        });
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

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
