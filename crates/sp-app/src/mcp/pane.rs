//! Settings › Integrations › AI assistants (MCP): the two switches the app checks every request
//! from an assistant against (`policy`), and adding SoftPresenter to each assistant app found on
//! this computer with one click (`connect`), with the line and the settings to copy, this
//! computer's path to `sp` filled in, for doing it by hand.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use egui::{RichText, Sense, vec2};

use super::Switches;
use super::connect::{self, App, Done, Place, Stands};
use crate::cli_install::Tone;
use crate::i18n::{t, tf};

/// The card's state: the switches (kept in `ai-assistants.json`), the assistant apps, and what
/// was said last.
#[derive(Debug, Default)]
pub struct View {
    pub switches: Switches,
    /// What the last Copy (or a switch that couldn't be saved) said: whether it went wrong, and
    /// the words.
    said: Option<(bool, String)>,
    /// The program an assistant starts, found again each time the card comes into view.
    program: Option<PathBuf>,
    /// The pass the card was last drawn in, to know when it has just come into view.
    drawn: Option<u64>,
    /// Scroll the pane to the card when next drawn (a capture run's, `SOFTPRESENTER_CAPTURE_PREFERENCES=mcp`).
    pub reveal: bool,
    /// Scroll on to the list of apps when next drawn (`SOFTPRESENTER_CAPTURE_PREFERENCES=mcp-apps`).
    pub reveal_apps: bool,
    /// The apps found on this computer, each with where SoftPresenter stands in it.
    rows: Vec<Row>,
    /// The apps made for this system that aren't found on it.
    absent: Vec<App>,
}

/// An app found on this computer.
#[derive(Debug)]
struct Row {
    app: App,
    place: Place,
    /// Where SoftPresenter stands, once looked at.
    stands: Option<Stands>,
    /// What the last click did, in place of where it stands.
    said: Option<(Tone, String)>,
    /// A look or a click under way, on a thread of its own (never the window's: an app's own
    /// command can take a moment): what it will find.
    job: Option<Receiver<Answer>>,
}

/// What a row's thread found.
#[derive(Debug)]
enum Answer {
    /// Where SoftPresenter stands, when the card came into view.
    Stands(Stands),
    /// What a click did, and where it stands after it.
    Clicked(Result<Done, String>, Stands),
}

/// A click on one of an app's buttons.
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
    for row in &mut view.rows {
        collect(row);
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
        if std::mem::take(&mut view.reveal_apps) {
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
        for row in &mut view.rows {
            row_part(ui, &ctx, row, &program);
        }
        others(ui, view);
        by_hand(ui, view, &program);
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
/// a few files looked at), and each app found is asked where SoftPresenter stands, on a thread
/// of its own.
fn came_into_view(ctx: &egui::Context, view: &mut View) {
    view.program = connect::program();
    view.said = None;
    // A row with a click under way keeps it, and the answer it will bring.
    let mut busy: Vec<Row> = view.rows.drain(..).filter(|row| row.job.is_some()).collect();
    view.absent.clear();
    for app in App::here() {
        if let Some(at) = busy.iter().position(|row| row.app == app) {
            view.rows.push(busy.remove(at));
            continue;
        }
        let Some(place) = connect::place(app) else {
            view.absent.push(app);
            continue;
        };
        let job = view.program.clone().map(|program| {
            let place = place.clone();
            on_a_thread(ctx, move || Answer::Stands(connect::status(app, &place, &program)))
        });
        view.rows.push(Row { app, place, stands: None, said: None, job });
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

/// Takes what a row's thread found, once it is done.
fn collect(row: &mut Row) {
    let Some(job) = &row.job else { return };
    match job.try_recv() {
        Ok(Answer::Stands(stands)) => {
            row.stands = Some(stands);
            row.job = None;
        }
        Ok(Answer::Clicked(done, stands)) => {
            let word = match &done {
                Ok(done) => done.log_word(),
                Err(_) => "not changed",
            };
            crate::applog::event("mcp", &format!("{}: {word}", row.app.name()));
            row.said = Some(match done {
                Ok(done) => connect::done_words(row.app, &done, &row.place, false),
                Err(why) => (Tone::Trouble, why),
            });
            row.stands = Some(stands);
            row.job = None;
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
        Err(std::sync::mpsc::TryRecvError::Disconnected) => row.job = None,
    }
}

/// An app found: its name and the buttons that fit where SoftPresenter stands (Add; Replace when
/// an entry by its name starts another program; Remove), and a line saying where it stands, or
/// what the last click did.
fn row_part(ui: &mut egui::Ui, ctx: &egui::Context, row: &mut Row, program: &Path) {
    let busy = row.job.is_some();
    let stands = row.stands.clone();
    let present = matches!(stands, Some(Stands::Added | Stands::Other { .. }));
    let other = matches!(stands, Some(Stands::Other { .. }));
    let add_hint = match connect::add_line(row.app, &row.place, program) {
        Some(line) => tf("mcp.add.runs", "Runs: {command}", &[("command", &line)]),
        None => tf(
            "mcp.add.file",
            "Adds SoftPresenter to {file}, changing nothing else in it, and keeps the file as it was beside it",
            &[("file", &row.place.shown())],
        ),
    };
    let mut clicked = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(row.app.name()).strong());
        ui.add_space(6.0);
        ui.add_enabled_ui(!busy, |ui| {
            if !present && stands.is_some() && ui.button(t("mcp.add", "Add")).on_hover_text(add_hint).clicked() {
                clicked = Some(Click::Add);
            }
            if other
                && ui
                    .button(t("mcp.replace", "Replace"))
                    .on_hover_text(t("mcp.replace.hint", "Points the entry at this copy of SoftPresenter"))
                    .clicked()
            {
                clicked = Some(Click::Replace);
            }
            if present
                && ui
                    .add(
                        egui::Button::new(RichText::new(t("mcp.remove", "Remove")).color(crate::studio::TEXT_DIM))
                            .frame(false),
                    )
                    .on_hover_text(t("mcp.remove.hint", "Takes SoftPresenter out again, and nothing else"))
                    .clicked()
            {
                clicked = Some(Click::Remove);
            }
        });
    });
    if let Some(click) = clicked {
        let (app, place, program) = (row.app, row.place.clone(), program.to_owned());
        row.said = None;
        row.job = Some(on_a_thread(ctx, move || {
            let done = match click {
                Click::Add => connect::add(app, &place, &program, false),
                Click::Replace => connect::add(app, &place, &program, true),
                Click::Remove => connect::remove(app, &place, &program),
            };
            Answer::Clicked(done, connect::status(app, &place, &program))
        }));
    }
    let said = if row.job.is_some() {
        Some((Tone::Plain, t("mcp.looking", "Looking\u{2026}").to_owned()))
    } else {
        row.said
            .clone()
            .or_else(|| stands.as_ref().map(|stands| connect::stands_words(row.app, stands, &row.place, false)))
    };
    if let Some((tone, words)) = said {
        line(ui, tone, &words);
    }
}

/// The apps made for this system that aren't on it, and those that only reach the internet, in
/// one faint line each.
fn others(ui: &mut egui::Ui, view: &View) {
    ui.add_space(8.0);
    if !view.absent.is_empty() {
        let names: Vec<&str> = view.absent.iter().map(|app| app.name()).collect();
        faint(ui, &tf("mcp.absent", "Not found on this computer: {apps}.", &[("apps", &names.join(", "))]));
        if view.absent.contains(&App::ClaudeCode) {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.hyperlink_to(
                    RichText::new(t("mcp.get_claude_code", "Get Claude Code")).size(11.5),
                    connect::GET_CLAUDE_CODE,
                );
            });
        }
    }
    faint(ui, connect::internet_only_words());
}

/// The line to type and the settings to paste, with Copy: open by itself when no app is found
/// to add SoftPresenter to with a click.
fn by_hand(ui: &mut egui::Ui, view: &mut View, program: &Path) {
    ui.add_space(6.0);
    egui::CollapsingHeader::new(RichText::new(t("mcp.by_hand", "Add it yourself")).size(12.0))
        .id_salt("mcp.by_hand")
        .default_open(view.rows.is_empty())
        .show(ui, |ui| {
            let code = claude_code_line(program, cfg!(windows));
            snippet(ui, view, t("mcp.claude_code", "Claude Code: type this in a terminal"), &code);
            let config = claude_desktop_config(program);
            snippet(
                ui,
                view,
                t(
                    "mcp.json_config",
                    "Claude Desktop, Cursor, Antigravity and most others: add this to the app\u{2019}s MCP settings file (mcpServers), then open the app again",
                ),
                &config,
            );
            snippet(
                ui,
                view,
                t(
                    "mcp.toml_config",
                    "ChatGPT and Codex (~/.codex/config.toml), Grok Build (~/.grok/config.toml): add this, then open the app again",
                ),
                &toml_config(program),
            );
        });
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

/// A line to copy: what it is for, the words in a fixed-width face, and Copy.
fn snippet(ui: &mut egui::Ui, view: &mut View, label: &str, words: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(label).size(12.0).color(crate::studio::TEXT_DIM));
    ui.horizontal_top(|ui| {
        let room = (ui.available_width() - 80.0).clamp(160.0, 560.0);
        ui.scope(|ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(RichText::new(words).monospace().size(11.5)).wrap());
        });
        if ui.button(t("mcp.copy", "Copy")).clicked() {
            ui.ctx().copy_text(words.to_owned());
            view.said = Some((false, t("mcp.copied", "Copied.").to_owned()));
        }
    });
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
