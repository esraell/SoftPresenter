//! Settings › Integrations › AI assistants (MCP): the two switches the app checks every request
//! from an assistant against (`policy`), and how to connect Claude Code and Claude Desktop, with
//! this computer's path to `sp` filled in and a Copy button beside each.

use std::path::{Path, PathBuf};

use egui::{RichText, Sense, vec2};

use super::Switches;
use crate::i18n::{t, tf};

/// The card's state: the switches (kept in `ai-assistants.json`), and what it said last.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub switches: Switches,
    /// What the last click did: whether it went wrong, and the words.
    said: Option<(bool, String)>,
    /// The program an assistant starts, found again each time the card comes into view.
    program: Option<PathBuf>,
    /// The pass the card was last drawn in, to know when it has just come into view.
    drawn: Option<u64>,
    /// Scroll the pane to the card when next drawn (a capture run's, `SOFTPRESENTER_CAPTURE_PREFERENCES=mcp`).
    pub reveal: bool,
}

impl View {
    /// The switches as last saved, both off when nothing was.
    pub fn load() -> View {
        View { switches: Switches::load(), ..View::default() }
    }
}

/// The program an assistant is to start: on a Mac and Linux, the installed `sp` when it runs
/// this copy (so an update needs nothing changed); else, and on Windows (where `sp` is a batch
/// file an assistant can't start by itself), this program.
fn program() -> Option<PathBuf> {
    #[cfg(unix)]
    if let crate::cli_install::Status::Here { link, .. } = crate::cli_install::status() {
        return Some(link);
    }
    std::env::current_exe().ok()
}

/// A path as a shell takes it: as it is when it holds nothing a shell reads, else quoted (in
/// single quotes on a Mac and Linux, double on Windows).
fn shell_word(path: &Path, windows: bool) -> String {
    let text = path.to_string_lossy();
    let plain = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '\\' | ':'));
    match (plain, windows) {
        (true, _) => text.into_owned(),
        (false, true) => format!("\"{text}\""),
        (false, false) => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// The line that adds SoftPresenter to Claude Code.
pub fn claude_code_line(program: &Path, windows: bool) -> String {
    format!("claude mcp add softpresenter -- {} mcp", shell_word(program, windows))
}

/// What goes in Claude Desktop's configuration file, in the order people write it (the command,
/// then its arguments), the path written as a JSON string.
pub fn claude_desktop_config(program: &Path) -> String {
    let command = serde_json::to_string(&program.to_string_lossy()).unwrap_or_default();
    format!(
        "{{\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"command\": {command},\n      \"args\": [\"mcp\"]\n    }}\n  }}\n}}"
    )
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
    // Just come into view: the program is found again, and what was said is forgotten.
    let pass = ui.ctx().cumulative_pass_nr();
    if view.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        view.program = program();
        view.said = None;
    }
    view.drawn = Some(pass);

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
        let windows = cfg!(windows);
        ui.add_space(8.0);
        let code = claude_code_line(&program, windows);
        snippet(ui, view, t("mcp.claude_code", "Claude Code: type this in a terminal"), &code);
        let config = claude_desktop_config(&program);
        snippet(
            ui,
            view,
            t(
                "mcp.claude_desktop",
                "Claude Desktop: add this to its claude_desktop_config.json, then open Claude Desktop again",
            ),
            &config,
        );
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
        assert_eq!(claude_code_line(plain, false), "claude mcp add softpresenter -- /Users/someone/.local/bin/sp mcp");
        let spaced = Path::new("/Applications/Soft Presenter.app/Contents/MacOS/softpresenter");
        assert_eq!(
            claude_code_line(spaced, false),
            "claude mcp add softpresenter -- '/Applications/Soft Presenter.app/Contents/MacOS/softpresenter' mcp"
        );
        let quote = Path::new("/tmp/it's/sp");
        assert_eq!(claude_code_line(quote, false), "claude mcp add softpresenter -- '/tmp/it'\\''s/sp' mcp");
        let windows = Path::new(r"C:\Program Files\SoftPresenter\SoftPresenter.exe");
        assert_eq!(
            claude_code_line(windows, true),
            r#"claude mcp add softpresenter -- "C:\Program Files\SoftPresenter\SoftPresenter.exe" mcp"#
        );
        // The configuration is JSON whatever the path holds.
        let config = claude_desktop_config(Path::new("C:\\a \"b\"\\sp.exe"));
        let read: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(read["mcpServers"]["softpresenter"]["command"], "C:\\a \"b\"\\sp.exe");
        assert_eq!(read["mcpServers"]["softpresenter"]["args"], serde_json::json!(["mcp"]));
    }
}
