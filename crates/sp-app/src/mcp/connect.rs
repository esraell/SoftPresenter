//! Connecting an assistant with one click (Settings › Integrations › AI assistants (MCP)) or one
//! command (`sp mcp connect APP`, `sp mcp disconnect APP`, `sp mcp status`): SoftPresenter put
//! in, or taken out of, the settings of the apps that start an MCP server on this computer.
//! Only ever when the person clicks or types it, never at install or at start: these are other
//! programs' settings.
//!
//! - **A settings file** (Claude Desktop, Cursor, VS Code and GitHub Copilot, Antigravity): only
//!   `mcpServers.softpresenter` changes, and every other byte of the file stays as it was
//!   (checked by reading the result back before anything is written). A file that isn't valid
//!   JSON is never written; the file as it was is kept beside it first (`….before-softpresenter`,
//!   replaced at each change); the new one is written to a file of its own in the same folder and
//!   renamed over it, so it is never half written; and a file or folder that is a link is left
//!   alone, never written through.
//! - **The app's own command** where it has one (Claude Code's `claude mcp`, Codex's `codex mcp`,
//!   which the ChatGPT app shares, Grok Build's `grok mcp`), started by its full path with each
//!   word apart (no shell). Its settings file is copied beside it first (not Claude Code's, which
//!   keeps its own copies), and never written by SoftPresenter.
//!
//! Apps that only reach servers on the internet (ChatGPT in a browser, the Grok and Gemini apps)
//! can't start one on this computer, and SoftPresenter isn't put on the internet for them.
//!
//! A test copy (quiet, its own data folder, a capture run) reaches none of them, unless the
//! testing aid gives it a stand-in home folder, where everything is then looked for.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::cli_install::{Tone, shown};
use crate::i18n::{t, tf};

/// The name SoftPresenter is added under, in each.
pub const NAME: &str = "softpresenter";

/// What follows the name of an app's settings file in the name of the copy kept of it.
const BACKUP: &str = ".before-softpresenter";

/// How long a run of an app's command may take before it is stopped.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Where Claude Code is offered to people who haven't got it.
pub const GET_CLAUDE_CODE: &str = "https://claude.com/claude-code";

/// The program an assistant is to start: on a Mac and Linux, the installed `sp` when it runs
/// this copy (so an update needs nothing changed); else, and on Windows (where `sp` is a batch
/// file an assistant can't start by itself), this program.
pub fn program() -> Option<PathBuf> {
    #[cfg(unix)]
    if let crate::cli_install::Status::Here { link, .. } = crate::cli_install::status() {
        return Some(link);
    }
    std::env::current_exe().ok()
}

/// A test copy, as the start-up install of `sp` tells one (quiet, its own data folder, a capture
/// run): it reaches another program's settings only through the testing aid, never the real
/// ones, so a capture or a test never reads, runs or writes anything of the owner's.
fn test_copy() -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    set("SOFTPRESENTER_QUIET")
        || set("SOFTPRESENTER_DATA")
        || std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("SOFTPRESENTER_CAPTURE"))
        || std::env::args().any(|word| word == "--capture-home")
}

fn var_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// A path as a settings file holds it: as text, which a path that isn't (rare, on a Mac or
/// Linux) can't be.
fn program_text(program: &Path) -> Result<&str, String> {
    program.to_str().ok_or_else(|| {
        t(
            "mcp.program_unwritable",
            "This program\u{2019}s path has characters a settings file can\u{2019}t hold, so add SoftPresenter yourself.",
        )
        .to_owned()
    })
}

/// Whether the program a setting starts is this one: the same text, and on Windows in any case
/// and with either slash, as Windows reads it.
fn same_program(written: &str, program: &str, windows: bool) -> bool {
    if windows {
        let plain = |path: &str| path.replace('/', "\\").to_lowercase();
        plain(written) == plain(program)
    } else {
        written == program
    }
}

/// Words from another program's file or output, as the window shows them: one line, without
/// format characters, cut short when long.
fn plain_words(words: &str, most: usize) -> String {
    let flat: String = words
        .chars()
        .filter(|c| !super::tools::formats_text(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > most {
        format!("{}\u{2026}", flat.chars().take(most).collect::<String>().trim_end())
    } else if flat.is_empty() {
        "?".to_owned()
    } else {
        flat
    }
}

// ---------------------------------------------------------------------------------------------
// The apps.

/// An app that starts an MCP server on this computer, which SoftPresenter can add itself to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum App {
    ClaudeDesktop,
    ClaudeCode,
    /// The ChatGPT app and Codex (its command line and editor extensions), which share
    /// `~/.codex/config.toml`.
    Codex,
    Cursor,
    /// VS Code's GitHub Copilot and the Copilot command line, which share
    /// `~/.copilot/mcp-config.json`.
    Copilot,
    /// Google's Antigravity, which took the Gemini command line's place.
    Antigravity,
    /// xAI's Grok Build, its command line.
    Grok,
}

impl App {
    pub const ALL: [App; 7] =
        [App::ClaudeDesktop, App::ClaudeCode, App::Codex, App::Cursor, App::Copilot, App::Antigravity, App::Grok];

    /// The app's name, as it calls itself (names aren't translated).
    pub fn name(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "Claude Desktop",
            App::ClaudeCode => "Claude Code",
            App::Codex => "ChatGPT / Codex",
            App::Cursor => "Cursor",
            App::Copilot => "VS Code / GitHub Copilot",
            App::Antigravity => "Antigravity",
            App::Grok => "Grok Build",
        }
    }

    /// The word `sp mcp connect` takes for it alone (a company's word takes all its apps);
    /// [`App::from_word`] takes a few more.
    pub fn word(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "desktop",
            App::ClaudeCode => "code",
            App::Codex => "codex",
            App::Cursor => "cursor",
            App::Copilot => "vscode",
            App::Antigravity => "antigravity",
            App::Grok => "grok-build",
        }
    }

    pub fn from_word(word: &str) -> Option<App> {
        match word.to_lowercase().as_str() {
            "desktop" | "claude-desktop" => Some(App::ClaudeDesktop),
            "code" | "claude-code" => Some(App::ClaudeCode),
            "codex" => Some(App::Codex),
            "cursor" => Some(App::Cursor),
            "vscode" | "vs-code" | "copilot" => Some(App::Copilot),
            "antigravity" => Some(App::Antigravity),
            "grok-build" => Some(App::Grok),
            _ => None,
        }
    }

    /// The company whose tile the app is under; `None` for the other apps (Cursor, VS Code).
    pub fn company(self) -> Option<Company> {
        match self {
            App::ClaudeDesktop | App::ClaudeCode => Some(Company::Claude),
            App::Codex => Some(Company::ChatGpt),
            App::Antigravity => Some(Company::Gemini),
            App::Grok => Some(Company::Grok),
            App::Cursor | App::Copilot => None,
        }
    }

    /// The other apps, under More, made for this system.
    pub fn others() -> Vec<App> {
        App::here().into_iter().filter(|app| app.company().is_none()).collect()
    }

    /// What Copy setup copies for the app: where it goes, and what.
    fn setup(self, program: &Path) -> String {
        let json = super::pane::claude_desktop_config(program);
        let toml = super::pane::toml_config(program);
        let file = |file: &str| tf("mcp.setup.file", "In {file}:", &[("file", file)]);
        match self {
            App::ClaudeDesktop => format!("{}\n{json}", file("claude_desktop_config.json")),
            App::ClaudeCode => format!(
                "{}\n{}",
                t("mcp.setup.terminal", "In a terminal:"),
                super::pane::claude_code_line(program, cfg!(windows))
            ),
            App::Codex => format!(
                "{}\n{toml}",
                tf(
                    "mcp.setup.codex",
                    "In {file}, or in the ChatGPT app\u{2019}s Settings \u{203a} MCP servers:",
                    &[("file", "~/.codex/config.toml")]
                )
            ),
            App::Grok => format!("{}\n{toml}", file("~/.grok/config.toml")),
            App::Antigravity => format!("{}\n{json}", file("~/.gemini/config/mcp_config.json")),
            App::Cursor => format!("{}\n{json}", file("~/.cursor/mcp.json")),
            App::Copilot => {
                let entry = entry_text(&self.fields(&program.to_string_lossy()), Some("    "), "  ", "\n");
                format!(
                    "{}\n{{\n  \"mcpServers\": {{\n    \"softpresenter\": {entry}\n  }}\n}}",
                    file("~/.copilot/mcp-config.json")
                )
            }
        }
    }

    /// Whether the app is made for this system at all (there is no Claude Desktop for Linux).
    fn made_for(self, system: System) -> bool {
        self != App::ClaudeDesktop || system != System::Other
    }

    /// The apps made for this system, in the card's order.
    pub fn here() -> Vec<App> {
        App::ALL.into_iter().filter(|app| app.made_for(System::here())).collect()
    }

    /// What goes in a settings file for SoftPresenter, in the order the app's own documents
    /// write it.
    fn fields(self, command: &str) -> Vec<(&'static str, Value)> {
        let mut fields = vec![("command", json!(command)), ("args", json!(["mcp"]))];
        match self {
            App::Cursor => fields.insert(0, ("type", json!("stdio"))),
            App::Copilot => {
                fields.insert(0, ("type", json!("stdio")));
                fields.push(("tools", json!(["*"])));
            }
            _ => {}
        }
        fields
    }
}

/// A company whose assistant apps the card shows together, as one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Company {
    Claude,
    ChatGpt,
    Gemini,
    Grok,
}

impl Company {
    pub const ALL: [Company; 4] = [Company::Claude, Company::ChatGpt, Company::Gemini, Company::Grok];

    pub fn name(self) -> &'static str {
        match self {
            Company::Claude => "Claude",
            Company::ChatGpt => "ChatGPT",
            Company::Gemini => "Gemini",
            Company::Grok => "Grok",
        }
    }

    /// The word `sp mcp connect` takes for all its apps.
    pub fn word(self) -> &'static str {
        match self {
            Company::Claude => "claude",
            Company::ChatGpt => "chatgpt",
            Company::Gemini => "gemini",
            Company::Grok => "grok",
        }
    }

    pub fn from_word(word: &str) -> Option<Company> {
        Company::ALL.into_iter().find(|company| company.word().eq_ignore_ascii_case(word))
    }

    /// Its apps SoftPresenter can add itself to, made for this system: Claude Desktop and
    /// Claude Code; Codex (which the ChatGPT app shares); Antigravity (which took the Gemini
    /// command line's place); Grok Build.
    pub fn apps(self) -> Vec<App> {
        App::here().into_iter().filter(|app| app.company() == Some(self)).collect()
    }

    /// What Copy setup copies: each of its apps' settings, with this program's path filled in.
    pub fn setup(self, program: &Path) -> String {
        self.apps().into_iter().map(|app| app.setup(program)).collect::<Vec<_>>().join("\n\n")
    }
}

/// What Copy setup copies for one app.
pub fn app_setup(app: App, program: &Path) -> String {
    app.setup(program)
}

/// Names as one list: "Claude Desktop and Claude Code".
fn and_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => tf("mcp.and", "{first} and {last}", &[("first", &rest.join(", ")), ("last", last)]),
    }
}

/// Where SoftPresenter stands in a tile's apps found here, in one line (`cli`: for `sp`).
pub fn group_words(found: &[(App, Place, Stands)], cli: bool) -> (Tone, String) {
    if found.is_empty() {
        return (Tone::Plain, not_found_words().to_owned());
    }
    if let Some((app, place, stands)) =
        found.iter().find(|(_, _, stands)| matches!(stands, Stands::Trouble(_) | Stands::Other { .. }))
    {
        let (tone, words) = stands_words(*app, stands, place, cli);
        return if matches!(stands, Stands::Trouble(_)) && found.len() > 1 {
            (tone, format!("{}: {words}", app.name()))
        } else {
            (tone, words)
        };
    }
    let names = |added: bool| -> Vec<&str> {
        found
            .iter()
            .filter(|(_, _, stands)| (*stands == Stands::Added) == added)
            .map(|(app, _, _)| app.name())
            .collect()
    };
    let (added, missing) = (names(true), names(false));
    match (added.is_empty(), missing.is_empty()) {
        (false, true) => (Tone::Good, tf("mcp.group.added", "Added to {apps}.", &[("apps", &and_list(&added))])),
        (false, false) => (
            Tone::Plain,
            tf(
                "mcp.group.partly",
                "Added to {added}; not yet to {missing}.",
                &[("added", &and_list(&added)), ("missing", &and_list(&missing))],
            ),
        ),
        _ => (Tone::Plain, tf("mcp.group.found", "Found: {apps}. Not added yet.", &[("apps", &and_list(&missing))])),
    }
}

/// Adds SoftPresenter to each app found (with `replace`, pointing entries that start another
/// program at this one): what each did.
pub fn add_all(found: &[(App, Place)], program: &Path, replace: bool) -> Vec<(App, Result<Done, String>)> {
    found.iter().map(|(app, place)| (*app, add(*app, place, program, replace))).collect()
}

/// Takes SoftPresenter's own entries (those starting this program) out of each app found; an
/// entry starting another program is left for that app: what each did.
pub fn remove_ours(found: &[(App, Place)], program: &Path) -> Vec<(App, Result<Done, String>)> {
    found
        .iter()
        .map(|(app, place)| {
            let done = match status(*app, place, program) {
                Stands::Added => remove(*app, place, program),
                _ => Ok(Done::NotThere),
            };
            (*app, done)
        })
        .collect()
}

/// What a click on a tile (or a command for a company) did to its apps, in one line.
pub fn group_done_words(done: &[(App, Result<Done, String>)], places: &[(App, Place)], cli: bool) -> (Tone, String) {
    let place = |app: App| places.iter().find(|(have, _)| *have == app).map(|(_, place)| place.clone());
    if let Some((app, Err(why))) = done.iter().find(|(_, done)| done.is_err()) {
        return (Tone::Trouble, if done.len() > 1 { format!("{}: {why}", app.name()) } else { why.clone() });
    }
    if let Some((app, Ok(taken @ Done::Taken { .. }))) =
        done.iter().find(|(_, done)| matches!(done, Ok(Done::Taken { .. })))
    {
        return done_words(*app, taken, &place(*app).unwrap_or(Place::File(PathBuf::new())), cli);
    }
    let named = |kinds: &[Done]| -> Vec<&str> {
        done.iter()
            .filter(|(_, done)| done.as_ref().is_ok_and(|done| kinds.contains(done)))
            .map(|(app, _)| app.name())
            .collect()
    };
    let changed = named(&[Done::Added, Done::Replaced]);
    let removed = named(&[Done::Removed]);
    let name = |names: &[&str]| [("app", and_list(names))];
    if !changed.is_empty() {
        let args = name(&changed);
        return (Tone::Good, tf("mcp.done.added", "Added. Quit and reopen {app} to use it.", &[("app", &args[0].1)]));
    }
    if !removed.is_empty() {
        let args = name(&removed);
        return (
            Tone::Plain,
            tf("mcp.done.removed", "Removed. Quit and reopen {app} for it to go.", &[("app", &args[0].1)]),
        );
    }
    let already = named(&[Done::Already]);
    if !already.is_empty() {
        let args = name(&already);
        return (
            Tone::Good,
            tf(
                "mcp.done.already",
                "It was there already. If {app} doesn\u{2019}t show SoftPresenter, quit and reopen it.",
                &[("app", &args[0].1)],
            ),
        );
    }
    (Tone::Plain, t("mcp.done.nothing", "There was nothing to remove.").to_owned())
}

/// Which system's places to use, as a value so that each system's rules can be tried on any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum System {
    Mac,
    Windows,
    Other,
}

impl System {
    fn here() -> System {
        if cfg!(target_os = "macos") {
            System::Mac
        } else if cfg!(windows) {
            System::Windows
        } else {
            System::Other
        }
    }
}

/// What finding an app's place reads from its surroundings, so that tests (and a test copy)
/// give their own.
struct Around {
    system: System,
    home: Option<PathBuf>,
    /// Windows' `%APPDATA%`.
    app_data: Option<PathBuf>,
    /// The PATH's folders, and Homebrew's and `/usr/local/bin`: looked in only for the real
    /// thing, never in a test copy.
    path: Option<OsString>,
    everywhere: bool,
    /// Where Copilot and Codex keep their settings, when moved (`COPILOT_HOME`, `CODEX_HOME`).
    copilot_home: Option<PathBuf>,
    codex_home: Option<PathBuf>,
}

impl Around {
    /// This run's: the person's own, or in a test copy the testing aid's stand-in home folder
    /// (and nothing at all without it).
    fn here() -> Option<Around> {
        if test_copy() {
            // Testing aid: SOFTPRESENTER_ASSISTANTS_HOME=<folder> is the home folder a test copy
            // looks for every assistant in (`Library/Application Support/Claude/`, `.cursor/`,
            // `.local/bin/claude`, `.grok/bin/grok`… under it, and `AppData/Roaming` for Windows'
            // own folder), and nowhere else: not the PATH, not the system's folders. A test copy
            // without it finds no assistant, so it never reads, runs or writes anything of the
            // owner's.
            let home = var_path("SOFTPRESENTER_ASSISTANTS_HOME")?;
            return Some(Around {
                system: System::here(),
                app_data: Some(home.join("AppData").join("Roaming")),
                home: Some(home),
                path: None,
                everywhere: false,
                copilot_home: None,
                codex_home: None,
            });
        }
        Some(Around {
            system: System::here(),
            home: if cfg!(windows) { var_path("USERPROFILE") } else { var_path("HOME") },
            app_data: var_path("APPDATA"),
            path: std::env::var_os("PATH"),
            everywhere: true,
            copilot_home: var_path("COPILOT_HOME"),
            codex_home: var_path("CODEX_HOME"),
        })
    }
}

/// Where an app keeps what SoftPresenter changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// Its settings file, changed by SoftPresenter.
    File(PathBuf),
    /// Its own program, which changes its settings (kept in the file named, copied first).
    Program { program: PathBuf, settings: Option<PathBuf> },
}

impl Place {
    /// The place as the window names it.
    pub fn shown(&self) -> String {
        match self {
            Place::File(file) => shown(file),
            Place::Program { program, .. } => shown(program),
        }
    }
}

/// Where the app's settings are on this computer, `None` when it isn't found here (its folder
/// isn't there, or its program).
pub fn place(app: App) -> Option<Place> {
    place_in(app, &Around::here()?)
}

fn place_in(app: App, around: &Around) -> Option<Place> {
    if !app.made_for(around.system) {
        return None;
    }
    let home = around.home.as_deref()?;
    let file_in = |folder: PathBuf, file: &[&str]| {
        folder.is_dir().then(|| Place::File(file.iter().fold(folder.clone(), |path, part| path.join(part))))
    };
    match app {
        App::ClaudeDesktop => {
            let folder = match around.system {
                System::Mac => home.join("Library").join("Application Support").join("Claude"),
                System::Windows => around.app_data.as_deref()?.join("Claude"),
                System::Other => return None,
            };
            file_in(folder, &["claude_desktop_config.json"])
        }
        App::Cursor => file_in(home.join(".cursor"), &["mcp.json"]),
        App::Copilot => {
            file_in(around.copilot_home.clone().unwrap_or_else(|| home.join(".copilot")), &["mcp-config.json"])
        }
        // Its own folders (the Gemini command line's `~/.gemini` alone isn't Antigravity).
        App::Antigravity => {
            let gemini = home.join(".gemini");
            ["antigravity", "antigravity-cli", "config"]
                .iter()
                .any(|folder| gemini.join(folder).is_dir())
                .then(|| Place::File(gemini.join("config").join("mcp_config.json")))
        }
        App::ClaudeCode | App::Codex | App::Grok => {
            let program = program_places(app, around).into_iter().find(|place| runnable(place))?;
            let settings = match app {
                App::Codex => {
                    Some(around.codex_home.clone().unwrap_or_else(|| home.join(".codex")).join("config.toml"))
                }
                App::Grok => Some(home.join(".grok").join("config.toml")),
                _ => None,
            };
            Some(Place::Program { program, settings })
        }
    }
}

/// Where an app's program may be, in order: the places its own installers put it under the
/// home folder, npm's global folder on Windows, then (for the real thing) Homebrew's folder,
/// `/usr/local/bin` and the PATH's folders written in full (a relative one would depend on
/// where the app started; an app opened from a Mac's Dock has a short PATH, so these come last).
fn program_places(app: App, around: &Around) -> Vec<PathBuf> {
    let (name, own): (&str, &[&[&str]]) = match app {
        App::ClaudeCode => ("claude", &[&[".local", "bin"], &[".claude", "local"]]),
        App::Codex => ("codex", &[&[".local", "bin"]]),
        App::Grok => ("grok", &[&[".grok", "bin"], &[".local", "bin"]]),
        _ => return Vec::new(),
    };
    let windows = around.system == System::Windows;
    let names: Vec<String> =
        if windows { vec![format!("{name}.exe"), format!("{name}.cmd")] } else { vec![name.to_owned()] };
    let mut places = Vec::new();
    if let Some(home) = &around.home {
        for folder in own {
            let folder = folder.iter().fold(home.clone(), |path, part| path.join(part));
            places.push(folder.join(&names[0]));
        }
    }
    if windows
        && app != App::Grok
        && let Some(dir) = &around.app_data
    {
        places.push(dir.join("npm").join(format!("{name}.cmd")));
    }
    if around.everywhere {
        if !windows {
            places.push(Path::new("/opt/homebrew/bin").join(name));
            places.push(Path::new("/usr/local/bin").join(name));
        }
        if let Some(path) = &around.path {
            for dir in std::env::split_paths(path).filter(|dir| dir.is_absolute()) {
                places.extend(names.iter().map(|name| dir.join(name)));
            }
        }
    }
    places
}

/// A file that can be started (on a Mac and Linux, one marked as a program).
fn runnable(place: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(place) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Where SoftPresenter stands in an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stands {
    /// Not there.
    Missing,
    /// There, starting this program.
    Added,
    /// There, starting another program (as written there, made plain).
    Other { command: String },
    /// Its settings can't be read, or aren't ones SoftPresenter would change, or its command
    /// answered in a way SoftPresenter doesn't know: why.
    Trouble(String),
}

/// What a click or a command did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    Added,
    /// It was there already, starting this program.
    Already,
    /// An entry starting another program now starts this one.
    Replaced,
    /// Nothing done: an entry by that name starts another program (as written there).
    Taken {
        command: String,
    },
    Removed,
    /// Nothing to remove.
    NotThere,
}

impl Done {
    /// The word for the log.
    pub fn log_word(&self) -> &'static str {
        match self {
            Done::Added => "added",
            Done::Already => "already there",
            Done::Replaced => "replaced",
            Done::Taken { .. } => "left alone (another program's entry)",
            Done::Removed => "removed",
            Done::NotThere => "nothing to remove",
        }
    }
}

/// Where SoftPresenter stands in the app (a command app's command is run: call it off the
/// window's thread).
pub fn status(app: App, place: &Place, program: &Path) -> Stands {
    let command = match program_text(program) {
        Ok(command) => command,
        Err(why) => return Stands::Trouble(why),
    };
    match place {
        Place::File(file) => file_status(file, command, cfg!(windows)),
        Place::Program { program: tool, .. } => command_status(app, tool, command, cfg!(windows)).0,
    }
}

/// Adds SoftPresenter to the app, or with `replace`, points an entry of its name that starts
/// another program at this one (off the window's thread, for a command app).
pub fn add(app: App, place: &Place, program: &Path, replace: bool) -> Result<Done, String> {
    let command = program_text(program)?;
    match place {
        Place::File(file) => file_add(file, &app.fields(command), replace, cfg!(windows)),
        Place::Program { program: tool, settings } => {
            command_add(app, tool, settings.as_deref(), command, replace, cfg!(windows))
        }
    }
}

/// Takes SoftPresenter's entry (whatever it starts) out of the app.
pub fn remove(app: App, place: &Place, program: &Path) -> Result<Done, String> {
    let command = program_text(program)?;
    match place {
        Place::File(file) => file_remove(file),
        Place::Program { program: tool, settings } => {
            command_remove(app, tool, settings.as_deref(), command, cfg!(windows))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A settings file.

/// An app's settings file as read: its bytes (for the copy kept), its text without a byte order
/// mark, whether it had one, what it holds, and its permissions (on a Mac and Linux).
struct Config {
    raw: Vec<u8>,
    text: String,
    bom: bool,
    value: Value,
    mode: Option<u32>,
}

fn file_status(file: &Path, command: &str, windows: bool) -> Stands {
    match read_config(file) {
        Err(why) => Stands::Trouble(why),
        Ok(None) => Stands::Missing,
        Ok(Some(config)) => entry_state(&config.value, command, windows),
    }
}

fn entry_state(value: &Value, command: &str, windows: bool) -> Stands {
    let Some(entry) = value.get("mcpServers").and_then(|servers| servers.get(NAME)) else {
        return Stands::Missing;
    };
    let written = entry.get("command").and_then(Value::as_str).unwrap_or_default();
    if entry.get("args") == Some(&json!(["mcp"])) && same_program(written, command, windows) {
        Stands::Added
    } else {
        Stands::Other { command: plain_words(written, 120) }
    }
}

/// Refuses a file, or the folder it is in, that is a link (written through, it would change
/// another file; renamed over, the person's link would become a plain file), and a file that
/// isn't one.
fn refuse_links(file: &Path) -> Result<(), String> {
    for place in [Some(file), file.parent().filter(|folder| !folder.as_os_str().is_empty())].into_iter().flatten() {
        if std::fs::symlink_metadata(place).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(tf(
                "mcp.file.link",
                "{path} is a link to somewhere else, so SoftPresenter left it as it is. Add SoftPresenter to it yourself.",
                &[("path", &shown(place))],
            ));
        }
    }
    match std::fs::symlink_metadata(file) {
        Ok(meta) if !meta.is_file() => Err(tf(
            "mcp.file.not_file",
            "{path} isn\u{2019}t a file, so SoftPresenter left it as it is.",
            &[("path", &shown(file))],
        )),
        _ => Ok(()),
    }
}

/// An app's settings file, `None` when there is none yet. Refused, with why, when it is a link,
/// isn't valid JSON, or doesn't hold an object with `mcpServers` (if there) an object. A file
/// with nothing in it but spaces counts as `{}`.
fn read_config(file: &Path) -> Result<Option<Config>, String> {
    refuse_links(file)?;
    let raw = match std::fs::read(file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(tf(
                "mcp.file.not_read",
                "Could not read {path}: {error}",
                &[("path", &shown(file)), ("error", &e.to_string())],
            ));
        }
    };
    let not_json = |error: &str| {
        tf(
            "mcp.file.not_json",
            "{path} isn\u{2019}t valid JSON ({error}), so SoftPresenter left it as it is. Correct it, or add SoftPresenter to it yourself.",
            &[("path", &shown(file)), ("error", error)],
        )
    };
    let text = String::from_utf8(raw.clone()).map_err(|_| not_json("not UTF-8 text"))?;
    let (bom, text) = match text.strip_prefix('\u{feff}') {
        Some(rest) => (true, rest.to_owned()),
        None => (false, text),
    };
    let value: Value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&text).map_err(|e| not_json(&e.to_string()))?
    };
    let laid_out = value.as_object().is_some_and(|root| root.get("mcpServers").is_none_or(Value::is_object));
    if !laid_out {
        return Err(tf(
            "mcp.file.shape",
            "{path} isn\u{2019}t laid out as SoftPresenter expects (an object, with mcpServers an object in it), so SoftPresenter left it as it is. Add SoftPresenter to it yourself.",
            &[("path", &shown(file))],
        ));
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(file).ok().map(|meta| meta.permissions().mode())
    };
    #[cfg(not(unix))]
    let mode = None;
    Ok(Some(Config { raw, text, bom, value, mode }))
}

fn file_add(file: &Path, fields: &[(&str, Value)], replace: bool, windows: bool) -> Result<Done, String> {
    let command = fields.iter().find(|(key, _)| *key == "command").and_then(|(_, value)| value.as_str()).unwrap_or("");
    let config = read_config(file)?;
    let state = config.as_ref().map_or(Stands::Missing, |config| entry_state(&config.value, command, windows));
    let done = match state {
        Stands::Added => return Ok(Done::Already),
        Stands::Other { command } if !replace => return Ok(Done::Taken { command }),
        Stands::Other { .. } => Done::Replaced,
        Stands::Missing | Stands::Trouble(_) => Done::Added,
    };
    let text = match &config {
        Some(config) if !config.text.trim().is_empty() => changed(config, Some(fields))?,
        _ => fresh_text(fields),
    };
    write_config(file, config.as_ref(), &text)?;
    Ok(done)
}

fn file_remove(file: &Path) -> Result<Done, String> {
    let Some(config) = read_config(file)? else { return Ok(Done::NotThere) };
    if config.value.get("mcpServers").and_then(|servers| servers.get(NAME)).is_none() {
        return Ok(Done::NotThere);
    }
    let text = changed(&config, None)?;
    write_config(file, Some(&config), &text)?;
    Ok(Done::Removed)
}

/// A new settings file holding only SoftPresenter.
fn fresh_text(fields: &[(&str, Value)]) -> String {
    let name = serde_json::to_string(NAME).unwrap_or_default();
    format!("{{\n  \"mcpServers\": {{\n    {name}: {}\n  }}\n}}\n", entry_text(fields, Some("    "), "  ", "\n"))
}

/// The file's text with `mcpServers.softpresenter` set to `fields`, or taken out (`None`), and
/// every other byte as it was. The result is read back and must hold exactly what the file held
/// with only that changed; anything else (two entries of the same name, say) is refused rather
/// than written.
fn changed(config: &Config, fields: Option<&[(&str, Value)]>) -> Result<String, String> {
    let mut wanted = config.value.clone();
    if let Some(root) = wanted.as_object_mut() {
        match fields {
            Some(fields) => {
                let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
                if let Some(servers) = servers.as_object_mut() {
                    let entry: serde_json::Map<String, Value> =
                        fields.iter().map(|(key, value)| ((*key).to_owned(), value.clone())).collect();
                    servers.insert(NAME.to_owned(), Value::Object(entry));
                }
            }
            None => {
                if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
                    servers.remove(NAME);
                }
            }
        }
    }
    let spliced = splice(&config.text, fields);
    let same =
        spliced.as_deref().and_then(|text| serde_json::from_str::<Value>(text).ok()).is_some_and(|v| v == wanted);
    match spliced {
        Some(text) if same => Ok(if config.bom { format!("\u{feff}{text}") } else { text }),
        _ => Err(t(
            "mcp.file.not_safely",
            "SoftPresenter couldn\u{2019}t change the settings file without changing something else in it, so it left it as it is. Add SoftPresenter to it yourself.",
        )
        .to_owned()),
    }
}

/// Keeps the file as it was beside it (when there was one), then puts the new text in its place,
/// each written whole to a new file in the same folder and renamed over the old, with the
/// file's own permissions (only the person's for a new one).
fn write_config(file: &Path, before: Option<&Config>, text: &str) -> Result<(), String> {
    if let Some(folder) = file.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).map_err(|e| {
            tf(
                "mcp.file.not_made",
                "Could not make {path}: {error}",
                &[("path", &shown(folder)), ("error", &e.to_string())],
            )
        })?;
    }
    // Again, now that the folder is surely there.
    refuse_links(file)?;
    let mode = before.and_then(|config| config.mode).unwrap_or(0o600);
    if let Some(before) = before {
        keep_copy(file, &before.raw, mode)?;
    }
    replace_file(file, text.as_bytes(), mode).map_err(|e| {
        tf(
            "mcp.file.not_written",
            "Could not change {path}: {error}",
            &[("path", &shown(file)), ("error", &e.to_string())],
        )
    })
}

/// Keeps a copy of a settings file as it was, beside it.
fn keep_copy(file: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let backup = backup_of(file);
    replace_file(&backup, bytes, mode).map_err(|e| {
        tf(
            "mcp.file.no_backup",
            "Could not keep a copy of the file as it was ({path}): {error}. Nothing was changed.",
            &[("path", &shown(&backup)), ("error", &e.to_string())],
        )
    })
}

/// Where the copy of an app's settings file as it was is kept.
pub fn backup_of(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(BACKUP);
    file.with_file_name(name)
}

/// Writes `bytes` whole to a new file of its own (never one already there, nor through a link)
/// in `target`'s folder, then renames it over `target`: a reader sees the old file or the new,
/// never part of one, and a link at `target` is replaced, not followed.
fn replace_file(target: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let folder = target.parent().filter(|folder| !folder.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.subsec_nanos());
    for attempt in 0..16 {
        let part = folder.join(format!(".{name}.{}-{stamp}-{attempt}.part", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut out = match options.open(&part) {
            Ok(out) => out,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let written = out.write_all(bytes).and_then(|()| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                out.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))?;
            }
            #[cfg(not(unix))]
            let _ = mode;
            out.sync_all()
        });
        drop(out);
        let done = written.and_then(|()| std::fs::rename(&part, target));
        if done.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        return done;
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free name for a new file"))
}

// The splice: JSON text changed in one place, every other byte kept. The text has been read as
// JSON by serde_json already (so it is valid and not nested too deep); this finds where things
// are in it.

/// A member of an object: its key, and where its key starts and its value starts and ends.
struct Member {
    key: String,
    key_start: usize,
    value_start: usize,
    value_end: usize,
}

/// An object: where its braces are, and its members in order.
struct Object {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

struct Scan<'a> {
    text: &'a [u8],
    at: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn string(&mut self) -> Option<()> {
        (self.peek()? == b'"').then_some(())?;
        self.at += 1;
        loop {
            match self.peek()? {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return Some(());
                }
                _ => self.at += 1,
            }
        }
    }

    fn value(&mut self) -> Option<()> {
        match self.peek()? {
            b'"' => self.string(),
            b'{' => self.object().map(|_| ()),
            b'[' => {
                self.at += 1;
                self.space();
                if self.peek()? == b']' {
                    self.at += 1;
                    return Some(());
                }
                loop {
                    self.value()?;
                    self.space();
                    match self.peek()? {
                        b',' => {
                            self.at += 1;
                            self.space();
                        }
                        b']' => {
                            self.at += 1;
                            return Some(());
                        }
                        _ => return None,
                    }
                }
            }
            _ => {
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.')) {
                    self.at += 1;
                }
                (self.at > start).then_some(())
            }
        }
    }

    fn object(&mut self) -> Option<Object> {
        (self.peek()? == b'{').then_some(())?;
        let open = self.at;
        self.at += 1;
        let mut members = Vec::new();
        self.space();
        if self.peek()? == b'}' {
            self.at += 1;
            return Some(Object { open, close: self.at - 1, members });
        }
        loop {
            self.space();
            let key_start = self.at;
            self.string()?;
            let key: String = serde_json::from_slice(&self.text[key_start..self.at]).ok()?;
            self.space();
            (self.peek()? == b':').then_some(())?;
            self.at += 1;
            self.space();
            let value_start = self.at;
            self.value()?;
            members.push(Member { key, key_start, value_start, value_end: self.at });
            self.space();
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(Object { open, close: self.at - 1, members });
                }
                _ => return None,
            }
        }
    }
}

/// The spaces before `at` on its line, when nothing else is before it there.
fn line_indent(text: &str, at: usize) -> Option<&str> {
    let start = text[..at].rfind('\n')? + 1;
    let indent = &text[start..at];
    indent.chars().all(|c| c == ' ' || c == '\t').then_some(indent)
}

/// The spaces a line starts with, for the line `at` is on.
fn own_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |at| at + 1);
    let line = &text[start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// SoftPresenter's entry as the file writes it: over lines at that indent (ended as the file
/// ends its lines), or on one.
fn entry_text(fields: &[(&str, Value)], indent: Option<&str>, unit: &str, nl: &str) -> String {
    let field = |(key, value): &(&str, Value)| {
        format!(
            "{}: {}",
            serde_json::to_string(key).unwrap_or_default(),
            serde_json::to_string(value).unwrap_or_default()
        )
    };
    match indent {
        Some(indent) => {
            let lines: Vec<String> = fields.iter().map(|pair| format!("{indent}{unit}{}", field(pair))).collect();
            format!("{{{nl}{}{nl}{indent}}}", lines.join(&format!(",{nl}")))
        }
        None => format!("{{{}}}", fields.iter().map(field).collect::<Vec<_>>().join(", ")),
    }
}

/// The text with a member added at the end of an object, the member written by `member` for the
/// indent its line has (`None` on one line with the rest).
fn add_member(text: &str, object: &Object, unit: &str, nl: &str, member: impl Fn(Option<&str>) -> String) -> String {
    match object.members.last() {
        Some(last) => {
            let indent = line_indent(text, last.key_start);
            let gap = indent.map_or(" ".to_owned(), |indent| format!("{nl}{indent}"));
            format!("{},{gap}{}{}", &text[..last.value_end], member(indent), &text[last.value_end..])
        }
        None if text.contains('\n') => {
            let own = own_indent(text, object.open);
            let indent = format!("{own}{unit}");
            format!("{}{nl}{indent}{}{nl}{own}{}", &text[..=object.open], member(Some(&indent)), &text[object.close..])
        }
        None => format!("{}{}{}", &text[..=object.open], member(None), &text[object.close..]),
    }
}

/// The text with `mcpServers.softpresenter` set to `fields` (or taken out), else as it was;
/// `None` when it can't be read as an object.
fn splice(text: &str, fields: Option<&[(&str, Value)]>) -> Option<String> {
    let bytes = text.as_bytes();
    let mut scan = Scan { text: bytes, at: 0 };
    scan.space();
    let root = scan.object()?;
    // The file's own line ending (Windows' Notepad writes \r\n), and step of indent, from the
    // first member of the outer object on a line of its own (two spaces when none is).
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let unit = root
        .members
        .iter()
        .find_map(|member| line_indent(text, member.key_start).filter(|indent| !indent.is_empty()))
        .unwrap_or("  ")
        .to_owned();
    let name = serde_json::to_string(NAME).unwrap_or_default();
    // serde_json keeps the last of two members with the same key; so does the splice.
    let Some(servers) = root.members.iter().rev().find(|member| member.key == "mcpServers") else {
        let Some(fields) = fields else { return Some(text.to_owned()) };
        return Some(add_member(text, &root, &unit, nl, |indent| match indent {
            Some(indent) => format!(
                "\"mcpServers\": {{{nl}{indent}{unit}{name}: {}{nl}{indent}}}",
                entry_text(fields, Some(&format!("{indent}{unit}")), &unit, nl)
            ),
            None => format!("\"mcpServers\": {{{name}: {}}}", entry_text(fields, None, &unit, nl)),
        }));
    };
    let object = Scan { text: bytes, at: servers.value_start }.object()?;
    let ours = object.members.iter().rposition(|member| member.key == NAME);
    Some(match (ours, fields) {
        (Some(at), Some(fields)) => {
            let member = &object.members[at];
            let indent = line_indent(text, member.key_start);
            format!(
                "{}{}{}",
                &text[..member.value_start],
                entry_text(fields, indent, &unit, nl),
                &text[member.value_end..]
            )
        }
        (None, Some(fields)) => {
            add_member(text, &object, &unit, nl, |indent| format!("{name}: {}", entry_text(fields, indent, &unit, nl)))
        }
        (Some(at), None) => {
            let member = &object.members[at];
            match (at.checked_sub(1).map(|before| &object.members[before]), object.members.get(at + 1)) {
                // From its key to the next one's.
                (_, Some(next)) => format!("{}{}", &text[..member.key_start], &text[next.key_start..]),
                // The last of several: from the end of the one before.
                (Some(before), None) => format!("{}{}", &text[..before.value_end], &text[member.value_end..]),
                // The only one: the object is left empty.
                (None, None) => format!("{}{}", &text[..=object.open], &text[object.close..]),
            }
        }
        (None, None) => text.to_owned(),
    })
}

// ---------------------------------------------------------------------------------------------
// An app's own command.

/// What a run of an app's command said: whether it went well, what it wrote on its standard
/// output (where its answers in JSON are), and both outputs together (for its words).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    ok: bool,
    out: String,
    words: String,
}

/// A pipe of a program's, read to its end on a thread of its own (the first 64 KB kept), so
/// that neither of its two outputs filling up holds it.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = std::io::Read::read_to_end(&mut std::io::Read::take(&mut pipe, 64 * 1024), &mut kept);
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        }
        let _ = send.send(kept);
    });
    receive
}

/// Runs an app's program with these words, each its own argument (no shell reads them), with
/// nothing to read, from the home folder, and stops it after [`COMMAND_TIMEOUT`].
fn run(app: App, tool: &Path, args: &[&str]) -> Result<Said, String> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(tool);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Not wherever SoftPresenter was started from (`/` for a Mac app): the settings it changes
    // for every folder are the same wherever it starts.
    if let Some(home) = Around::here().and_then(|around| around.home).filter(|home| home.is_dir()) {
        command.current_dir(home);
    }
    // A program that is a script for Node (npm's) finds the Node beside it, even with an app's
    // short PATH.
    if let Some(dir) = tool.parent() {
        let mut dirs = vec![dir.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        if let Ok(path) = std::env::join_paths(dirs) {
            command.env("PATH", path);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| {
        tf(
            "mcp.command.not_started",
            "Could not start {app} ({path}): {error}",
            &[("app", app.name()), ("path", &shown(tool)), ("error", &e.to_string())],
        )
    })?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let until = Instant::now() + COMMAND_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let words = |pipe: std::sync::mpsc::Receiver<Vec<u8>>| {
        String::from_utf8_lossy(&pipe.recv_timeout(Duration::from_secs(2)).unwrap_or_default()).into_owned()
    };
    let out = words(out);
    let words = format!("{out}\n{}", words(err)).trim().to_owned();
    match status {
        Some(status) => Ok(Said { ok: status.success(), out, words }),
        None => Err(tf(
            "mcp.command.too_long",
            "{app} didn\u{2019}t answer within 30 seconds, so SoftPresenter stopped waiting.",
            &[("app", app.name())],
        )),
    }
}

/// What an app's command said, for the window: its first lines, made plain.
fn said_words(app: App, said: &Said) -> String {
    let first: Vec<&str> = said.words.lines().map(str::trim).filter(|line| !line.is_empty()).take(2).collect();
    tf(
        "mcp.command.said",
        "{app} said: {words}",
        &[("app", app.name()), ("words", &plain_words(&first.join(" "), 240))],
    )
}

/// Where SoftPresenter stands, as the app's command reads it, and the scope of the app's
/// settings its entry is in (for taking it out of the same place).
fn command_status(app: App, tool: &Path, command: &str, windows: bool) -> (Stands, &'static str) {
    let asked: &[&str] = match app {
        App::ClaudeCode => &["mcp", "get", NAME],
        App::Codex => &["mcp", "get", NAME, "--json"],
        _ => &["mcp", "list", "--json"],
    };
    match run(app, tool, asked) {
        Ok(said) => read_answer(app, &said, command, windows),
        Err(why) => (Stands::Trouble(why), "user"),
    }
}

/// Reads an app's answer to where SoftPresenter stands (the forms each wrote when tried, in
/// November 2026). Anything else is [`Stands::Trouble`] with its words, never a guess.
fn read_answer(app: App, said: &Said, command: &str, windows: bool) -> (Stands, &'static str) {
    let unknown = || (Stands::Trouble(said_words(app, said)), "user");
    let judged = |written: &str, args_ok: bool| {
        if args_ok && same_program(written, command, windows) {
            Stands::Added
        } else {
            Stands::Other { command: plain_words(written, 120) }
        }
    };
    if !said.ok {
        // "No MCP server named …" (Claude Code), "No MCP server named '…' found." (Codex).
        return if said.words.contains("No MCP server named") { (Stands::Missing, "user") } else { unknown() };
    }
    match app {
        // Lines: "Scope: User config (…)", "Command: …", "Args: mcp".
        App::ClaudeCode => {
            let field = |name: &str| said.words.lines().find_map(|line| line.trim().strip_prefix(name)).map(str::trim);
            let scope = match field("Scope:").and_then(|scope| scope.split_whitespace().next()).map(str::to_lowercase) {
                Some(word) if word == "local" => "local",
                Some(word) if word == "project" => "project",
                _ => "user",
            };
            match field("Command:") {
                Some(written) => (judged(written, field("Args:") == Some("mcp")), scope),
                None => unknown(),
            }
        }
        // {"name": …, "transport": {"type": "stdio", "command": …, "args": […]}, …}
        App::Codex => {
            let Ok(read) = serde_json::from_str::<Value>(&said.out) else { return unknown() };
            let transport = &read["transport"];
            match transport["command"].as_str() {
                Some(written) => (judged(written, transport["args"] == json!(["mcp"])), "user"),
                None => (Stands::Other { command: plain_words(&transport["url"].to_string(), 120) }, "user"),
            }
        }
        // [{"name": …, "command": …, "args": […], "scope": "user"|"project", …}, …]
        _ => {
            let Ok(Value::Array(servers)) = serde_json::from_str::<Value>(&said.out) else { return unknown() };
            let ours: Vec<&Value> = servers.iter().filter(|server| server["name"] == NAME).collect();
            let Some(entry) = ours.iter().find(|server| server["scope"] == "user").or(ours.first()) else {
                return (Stands::Missing, "user");
            };
            let scope = if entry["scope"] == "project" { "project" } else { "user" };
            let written = entry["command"].as_str().unwrap_or_default();
            (judged(written, entry["args"] == json!(["mcp"])), scope)
        }
    }
}

/// Copies an app's settings file beside it before its own command changes it, when there is
/// one (a link is read, and its copy is a file of its own).
fn copy_settings(settings: Option<&Path>) -> Result<(), String> {
    let Some(file) = settings else { return Ok(()) };
    match std::fs::read(file) {
        Ok(bytes) => {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(file).map_or(0o600, |meta| meta.permissions().mode())
            };
            #[cfg(not(unix))]
            let mode = 0o600;
            keep_copy(file, &bytes, mode)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(tf(
            "mcp.file.not_read",
            "Could not read {path}: {error}",
            &[("path", &shown(file)), ("error", &e.to_string())],
        )),
    }
}

/// The words that add SoftPresenter for every folder: `claude mcp add --scope user softpresenter
/// -- PROGRAM mcp`, `codex mcp add softpresenter -- PROGRAM mcp` (Codex has one place for every
/// folder), `grok mcp add -s user softpresenter -- PROGRAM mcp`.
fn add_words(app: App, command: &str) -> Vec<&str> {
    let mut words = match app {
        App::ClaudeCode => vec!["mcp", "add", "--scope", "user", NAME],
        App::Grok => vec!["mcp", "add", "-s", "user", NAME],
        _ => vec!["mcp", "add", NAME],
    };
    words.extend(["--", command, "mcp"]);
    words
}

fn command_add(
    app: App,
    tool: &Path,
    settings: Option<&Path>,
    command: &str,
    replace: bool,
    windows: bool,
) -> Result<Done, String> {
    let (state, scope) = command_status(app, tool, command, windows);
    let done = match state {
        Stands::Added => return Ok(Done::Already),
        Stands::Other { command } if !replace => return Ok(Done::Taken { command }),
        // Codex's and Grok Build's `add` replace an entry without asking, so when where
        // SoftPresenter stands couldn't be read, nothing is added unless Replace was chosen
        // (Claude Code's refuses to add over one).
        Stands::Trouble(why) if !replace && app != App::ClaudeCode => return Err(why),
        Stands::Other { .. } => Done::Replaced,
        Stands::Missing | Stands::Trouble(_) => Done::Added,
    };
    copy_settings(settings)?;
    // Claude Code refuses to add over an entry; Codex and Grok Build replace it.
    if done == Done::Replaced && app == App::ClaudeCode {
        remove_with(app, tool, scope)?;
    }
    let said = run(app, tool, &add_words(app, command))?;
    if said.ok { Ok(done) } else { Err(said_words(app, &said)) }
}

fn command_remove(
    app: App,
    tool: &Path,
    settings: Option<&Path>,
    command: &str,
    windows: bool,
) -> Result<Done, String> {
    let scope = match command_status(app, tool, command, windows) {
        (Stands::Missing, _) => return Ok(Done::NotThere),
        (_, scope) => scope,
    };
    copy_settings(settings)?;
    remove_with(app, tool, scope)?;
    Ok(Done::Removed)
}

/// `claude mcp remove --scope SCOPE softpresenter`, `codex mcp remove softpresenter`, `grok mcp
/// remove -s SCOPE softpresenter`.
fn remove_with(app: App, tool: &Path, scope: &str) -> Result<(), String> {
    let words: Vec<&str> = match app {
        App::ClaudeCode => vec!["mcp", "remove", "--scope", scope, NAME],
        App::Grok => vec!["mcp", "remove", "-s", scope, NAME],
        _ => vec!["mcp", "remove", NAME],
    };
    let said = run(app, tool, &words)?;
    if said.ok { Ok(()) } else { Err(said_words(app, &said)) }
}

// ---------------------------------------------------------------------------------------------
// Words.

/// Where SoftPresenter stands in an app, in words (`cli`: for `sp`, which names the command
/// where the card has a button).
pub fn stands_words(app: App, stands: &Stands, place: &Place, cli: bool) -> (Tone, String) {
    match stands {
        Stands::Missing => (Tone::Plain, t("mcp.missing", "Not added yet.").to_owned()),
        Stands::Added => match place {
            Place::File(file) => (Tone::Good, tf("mcp.added_in", "Added, in {file}.", &[("file", &shown(file))])),
            Place::Program { .. } => (Tone::Good, t("mcp.added", "Added.").to_owned()),
        },
        Stands::Other { command } if cli => (
            Tone::Care,
            tf(
                "mcp.other_cli",
                "{app} has a \u{201c}softpresenter\u{201d} entry that starts another program ({command}). sp mcp connect {word} --replace points it at this copy.",
                &[("app", app.name()), ("command", command), ("word", app.word())],
            ),
        ),
        Stands::Other { command } => (
            Tone::Care,
            tf(
                "mcp.other",
                "{app} has a \u{201c}softpresenter\u{201d} entry that starts another program ({command}). Replace points it at this copy.",
                &[("app", app.name()), ("command", command)],
            ),
        ),
        Stands::Trouble(why) => (Tone::Trouble, why.clone()),
    }
}

/// What a click or a command did, in words.
pub fn done_words(app: App, done: &Done, place: &Place, cli: bool) -> (Tone, String) {
    let name = [("app", app.name())];
    match done {
        Done::Added => (Tone::Good, tf("mcp.done.added", "Added. Quit and reopen {app} to use it.", &name)),
        Done::Already => (
            Tone::Good,
            tf(
                "mcp.done.already",
                "It was there already. If {app} doesn\u{2019}t show SoftPresenter, quit and reopen it.",
                &name,
            ),
        ),
        Done::Replaced => (Tone::Good, tf("mcp.done.replaced", "Replaced. Quit and reopen {app} to use it.", &name)),
        Done::Taken { command } => stands_words(app, &Stands::Other { command: command.clone() }, place, cli),
        Done::Removed => (Tone::Plain, tf("mcp.done.removed", "Removed. Quit and reopen {app} for it to go.", &name)),
        Done::NotThere => (Tone::Plain, t("mcp.done.nothing", "There was nothing to remove.").to_owned()),
    }
}

/// What the window and `sp` say about an app made for this system that isn't found on it.
pub fn not_found_words() -> &'static str {
    t("mcp.not_found", "Not found on this computer.")
}

/// Why the apps that only reach the internet aren't listed.
pub fn internet_only_words() -> &'static str {
    t(
        "mcp.internet_only",
        "ChatGPT in a browser and the Grok and Gemini apps only reach servers on the internet, and SoftPresenter stays on this computer, so they can\u{2019}t use it.",
    )
}

// ---------------------------------------------------------------------------------------------
// `sp mcp status|connect|disconnect`: answered by this program, in English, without the app.

const USAGE: &str = "Use: sp mcp status [--json] | sp mcp connect WHICH [--replace] | sp mcp disconnect WHICH\n\
     WHICH: claude (Claude Desktop and Claude Code), chatgpt (Codex, which the ChatGPT app shares),\n\
     gemini (Antigravity), grok (Grok Build), cursor, vscode (VS Code / GitHub Copilot),\n\
     or one app alone: desktop, code, codex, antigravity, grok-build";

/// Carries out `sp mcp …` (the words after `mcp`; `mcp` alone is the server, and any other word
/// after it a mistake), printing the answer.
pub fn run_cli(args: &[String]) -> ExitCode {
    let answer = crate::i18n::with_language(crate::i18n::Language::English, || answer(args));
    match answer {
        Ok(words) => {
            println!("{words}");
            ExitCode::SUCCESS
        }
        Err((code, words)) => {
            eprintln!("{words}");
            ExitCode::from(code)
        }
    }
}

/// The apps a word names: a company's (those made for this system), or one app.
fn apps_named(word: &str) -> Option<(String, Vec<App>)> {
    match (Company::from_word(word), App::from_word(word)) {
        (Some(company), _) => Some((company.name().to_owned(), company.apps())),
        (None, Some(app)) => Some((app.name().to_owned(), vec![app])),
        _ => None,
    }
}

fn answer(args: &[String]) -> Result<String, (u8, String)> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let mistake = |what: String| -> Result<String, (u8, String)> { Err((2, format!("{what}\n{USAGE}"))) };
    let program = program().ok_or_else(|| (1, "Could not find this program.".to_owned()))?;
    let (verb, rest) = words.split_first().map_or(("", &[][..]), |(verb, rest)| (*verb, rest));
    match verb {
        "status" if rest.is_empty() => Ok(status_text(&program)),
        "status" if rest == ["--json"] => Ok(serde_json::to_string_pretty(&status_json(&program)).unwrap_or_default()),
        "connect" | "disconnect" => {
            let replace = rest.contains(&"--replace");
            let which: Vec<&str> = rest.iter().copied().filter(|word| *word != "--replace").collect();
            let Some((name, apps)) = (if let [word] = which.as_slice() { apps_named(word) } else { None }) else {
                return mistake(format!("sp mcp {verb}: which assistant?"));
            };
            if replace && verb == "disconnect" {
                return mistake("sp mcp disconnect takes no --replace.".to_owned());
            }
            let found: Vec<(App, Place)> =
                apps.iter().filter_map(|app| place(*app).map(|place| (*app, place))).collect();
            if found.is_empty() {
                return Err((1, format!("{name}: {}", not_reached(&apps))));
            }
            // Each settings file, and whether it was there before (so a copy of it is kept).
            let files: Vec<Option<(PathBuf, bool, bool)>> = found
                .iter()
                .map(|(_, place)| match place {
                    Place::File(file) => Some((file.clone(), file.exists(), true)),
                    Place::Program { settings, .. } => {
                        settings.clone().map(|file| (file.clone(), file.exists(), false))
                    }
                })
                .collect();
            let done =
                if verb == "connect" { add_all(&found, &program, replace) } else { remove_ours(&found, &program) };
            let (tone, mut said) = group_done_words(&done, &found, true);
            let changed: Vec<App> = done
                .iter()
                .filter(|(_, done)| matches!(done, Ok(Done::Added | Done::Replaced | Done::Removed)))
                .map(|(app, _)| *app)
                .collect();
            for ((app, _), file) in found.iter().zip(&files).filter(|((app, _), _)| changed.contains(app)) {
                let Some((file, existed, ours)) = file else { continue };
                let what = if *ours { "Changed" } else { "Its settings are in" };
                said.push_str(&if *existed {
                    format!(
                        "\n{}: {what} {}; the file as it was is beside it, as {}.",
                        app.name(),
                        shown(file),
                        backup_of(file).file_name().unwrap_or_default().to_string_lossy()
                    )
                } else {
                    format!("\n{}: {what} {} (new).", app.name(), shown(file))
                });
            }
            if matches!(tone, Tone::Care | Tone::Trouble) { Err((1, said)) } else { Ok(said) }
        }
        "" => mistake("sp mcp: which command?".to_owned()),
        other => mistake(format!("sp mcp: no command called '{other}'.")),
    }
}

/// Why `sp` can't reach these apps here.
fn not_reached(apps: &[App]) -> String {
    if test_copy() && var_path("SOFTPRESENTER_ASSISTANTS_HOME").is_none() {
        "a test copy looks for assistants only under SOFTPRESENTER_ASSISTANTS_HOME.".to_owned()
    } else if apps.is_empty() {
        "not made for this system.".to_owned()
    } else {
        not_found_words().to_lowercase()
    }
}

/// Each tile (the companies, then the other apps), with its apps found here and where
/// SoftPresenter stands in each.
/// A tile for `sp mcp status`: its name, its company (none for an other app), its apps made for
/// this system, and those found here with where SoftPresenter stands in each.
type StatusTile = (String, Option<Company>, Vec<App>, Vec<(App, Place, Stands)>);

fn tiles(program: &Path) -> Vec<StatusTile> {
    let found = |apps: &[App]| -> Vec<(App, Place, Stands)> {
        apps.iter()
            .filter_map(|app| place(*app).map(|place| (*app, place.clone(), status(*app, &place, program))))
            .collect()
    };
    let mut tiles: Vec<_> = Company::ALL
        .into_iter()
        .map(|company| {
            let apps = company.apps();
            (company.name().to_owned(), Some(company), apps.clone(), found(&apps))
        })
        .collect();
    for app in App::others() {
        tiles.push((app.name().to_owned(), None, vec![app], found(&[app])));
    }
    tiles
}

/// `sp mcp status`: the two switches, where SoftPresenter stands in each tile's apps, and the
/// program they start.
fn status_text(program: &Path) -> String {
    let switches = super::Switches::load();
    let mut lines = vec![if switches.allowed {
        "AI assistants: on (Settings \u{203a} Integrations \u{203a} AI assistants (MCP))".to_owned()
    } else {
        "AI assistants: off. An assistant can do nothing until Settings \u{203a} Integrations \u{203a} AI assistants (MCP) lets them in.".to_owned()
    }];
    lines.push(format!("Changing the screens: {}", if switches.screens { "on" } else { "off" }));
    for (name, company, apps, found) in tiles(program) {
        let said = if found.is_empty() { not_reached(&apps) } else { group_words(&found, true).1 };
        let word = company.map_or_else(|| apps.first().map_or("", |app| app.word()), Company::word);
        lines.push(format!("{name} ({word}): {said}"));
        if found.len() > 1 {
            for (app, place, stands) in &found {
                lines.push(format!("  {} ({}): {}", app.name(), app.word(), stands_words(*app, stands, place, true).1));
            }
        }
    }
    lines.push(internet_only_words().to_owned());
    lines.push(format!("Assistants start: {} mcp", shown(program)));
    lines.join("\n")
}

/// The word for where SoftPresenter stands, in JSON.
fn stands_word(stands: &Stands) -> &'static str {
    match stands {
        Stands::Missing => "not_added",
        Stands::Added => "added",
        Stands::Other { .. } => "other_command",
        Stands::Trouble(_) => "trouble",
    }
}

/// `sp mcp status --json`: the same, for a script (or another window) to read. Each tile has
/// its apps, each app whether it is found here, where SoftPresenter stands and why; a tile's
/// state is `not_found`, `not_added`, `added` (all found), `partly`, `other_command` or
/// `trouble`.
fn status_json(program: &Path) -> Value {
    let switches = super::Switches::load();
    let tiles: Vec<Value> = tiles(program)
        .into_iter()
        .map(|(name, company, apps, found)| {
            let state = if found.is_empty() {
                "not_found"
            } else if found.iter().any(|(_, _, stands)| matches!(stands, Stands::Trouble(_))) {
                "trouble"
            } else if found.iter().any(|(_, _, stands)| matches!(stands, Stands::Other { .. })) {
                "other_command"
            } else if found.iter().all(|(_, _, stands)| *stands == Stands::Added) {
                "added"
            } else if found.iter().any(|(_, _, stands)| *stands == Stands::Added) {
                "partly"
            } else {
                "not_added"
            };
            let listed: Vec<Value> = apps
                .iter()
                .map(|app| match found.iter().find(|(have, _, _)| have == app) {
                    Some((_, place, stands)) => json!({
                        "id": app.word(),
                        "name": app.name(),
                        "found": true,
                        "place": place.shown(),
                        "state": stands_word(stands),
                        "command": match stands { Stands::Other { command } => Value::from(command.clone()), _ => Value::Null },
                        "said": stands_words(*app, stands, place, true).1,
                    }),
                    None => json!({"id": app.word(), "name": app.name(), "found": false, "state": "not_found"}),
                })
                .collect();
            let word = company.map_or_else(|| apps.first().map_or("", |app| app.word()), Company::word);
            json!({
                "id": word,
                "name": name,
                "company": company.is_some(),
                "state": state,
                "said": if found.is_empty() { not_reached(&apps) } else { group_words(&found, true).1 },
                "apps": listed,
            })
        })
        .collect();
    json!({
        "allowed": switches.allowed,
        "screens": switches.screens,
        "program": program.to_string_lossy(),
        "tiles": tiles,
        "internet_only": internet_only_words(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for a test.
    fn scratch(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("sp-connect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("a folder");
        std::fs::canonicalize(&base).expect("the folder")
    }

    const SP: &str = "/home/someone/.local/bin/sp";

    /// Runs a test's checks with the window's words in English (another test may switch the
    /// language while this one runs).
    fn english(checks: fn()) {
        crate::i18n::with_language(crate::i18n::Language::English, checks);
    }

    fn read(file: &Path) -> String {
        std::fs::read_to_string(file).expect("the file")
    }

    fn value(file: &Path) -> Value {
        serde_json::from_str(&read(file)).expect("JSON")
    }

    fn desktop() -> Vec<(&'static str, Value)> {
        App::ClaudeDesktop.fields(SP)
    }

    fn config(text: &str) -> Config {
        Config {
            raw: text.as_bytes().to_vec(),
            text: text.to_owned(),
            bom: false,
            value: serde_json::from_str(text).unwrap(),
            mode: None,
        }
    }

    #[test]
    fn a_settings_file_is_made_when_missing() {
        let base = scratch("missing");
        let file = base.join("Claude/claude_desktop_config.json");
        assert_eq!(file_status(&file, SP, false), Stands::Missing);
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Added));
        assert_eq!(value(&file), json!({"mcpServers": {"softpresenter": {"command": SP, "args": ["mcp"]}}}));
        assert!(read(&file).ends_with("}\n"));
        // Nothing was there before, so no copy of it.
        assert!(!backup_of(&file).exists());
        // Only the person may read a new one (it may come to hold other servers' keys).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(file_status(&file, SP, false), Stands::Added);
        // Again: nothing written.
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Already));
        assert!(!backup_of(&file).exists());
        // Each app's own fields, in its documents' order.
        assert_eq!(
            fresh_text(&App::Copilot.fields(SP)),
            format!(
                "{{\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"type\": \"stdio\",\n      \"command\": \"{SP}\",\n      \"args\": [\"mcp\"],\n      \"tools\": [\"*\"]\n    }}\n  }}\n}}\n"
            )
        );
        assert_eq!(App::Cursor.fields(SP)[0], ("type", json!("stdio")));
        assert_eq!(App::Antigravity.fields(SP), desktop());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn every_other_byte_of_the_file_is_kept() {
        let base = scratch("kept");
        let file = base.join("claude_desktop_config.json");
        let before = "{\n    \"preferences\": {\"zoom\": 1.50, \"menuBarEnabled\": false},\n    \"mcpServers\": {\n        \"weather\": {\n            \"command\": \"node\",\n            \"args\": [\"/opt/weather/index.js\"],\n            \"env\": {\"KEY\": \"x\\u0041\"}\n        }\n    },\n    \"aaa\": [1, 2, 3e2]\n}\n";
        std::fs::write(&file, before).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Added));
        let after = read(&file);
        let ours = format!(
            ",\n        \"softpresenter\": {{\n            \"command\": \"{SP}\",\n            \"args\": [\"mcp\"]\n        }}"
        );
        let at = before.find("\n    },\n    \"aaa\"").unwrap();
        assert_eq!(after, format!("{}{ours}{}", &before[..at], &before[at..]));
        // The file as it was, beside it, with its permissions; and the new one has them too.
        assert_eq!(read(&backup_of(&file)), before);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o640);
            assert_eq!(std::fs::metadata(backup_of(&file)).unwrap().permissions().mode() & 0o777, 0o640);
        }
        // No new file left behind.
        let names: Vec<String> =
            std::fs::read_dir(&base).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        // Remove: back to exactly what it was.
        assert_eq!(file_remove(&file), Ok(Done::Removed));
        assert_eq!(read(&file), before);
        assert_eq!(read(&backup_of(&file)), after);
        assert_eq!(file_remove(&file), Ok(Done::NotThere));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn mcp_servers_is_added_where_there_is_none_in_the_files_own_layout() {
        let fields = desktop();
        // Two-space pretty, as Claude Desktop writes it.
        let text = "{\n  \"preferences\": {\n    \"zoom\": 1\n  }\n}\n";
        assert_eq!(
            changed(&config(text), Some(&fields)).unwrap(),
            format!(
                "{{\n  \"preferences\": {{\n    \"zoom\": 1\n  }},\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"command\": \"{SP}\",\n      \"args\": [\"mcp\"]\n    }}\n  }}\n}}\n"
            )
        );
        // Windows' line endings stay Windows'.
        let text = "{\r\n  \"a\": 1\r\n}\r\n";
        let got = changed(&config(text), Some(&fields)).unwrap();
        assert!(!got.replace("\r\n", "").contains('\n'), "{got:?}");
        assert!(got.starts_with("{\r\n  \"a\": 1,\r\n  \"mcpServers\": {\r\n    \"softpresenter\": {\r\n"), "{got:?}");
        // Tabs.
        let got = changed(&config("{\n\t\"a\": 1\n}"), Some(&fields)).unwrap();
        assert!(got.contains("\n\t\"mcpServers\": {\n\t\t\"softpresenter\": {\n\t\t\t\"command\""), "{got}");
        // On one line.
        assert_eq!(
            changed(&config("{\"a\":1}"), Some(&fields)).unwrap(),
            format!(
                "{{\"a\":1, \"mcpServers\": {{\"softpresenter\": {{\"command\": \"{SP}\", \"args\": [\"mcp\"]}}}}}}"
            )
        );
        // Empty, over lines and not.
        let got = changed(&config("{\n}\n"), Some(&fields)).unwrap();
        assert!(got.starts_with("{\n  \"mcpServers\": {\n    \"softpresenter\": {\n"), "{got}");
        let got = changed(&config("{}"), Some(&fields)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap()["mcpServers"]["softpresenter"]["command"], SP);
        // An empty mcpServers.
        let got = changed(&config("{\n  \"mcpServers\": {}\n}\n"), Some(&fields)).unwrap();
        assert_eq!(
            got,
            format!(
                "{{\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"command\": \"{SP}\",\n      \"args\": [\"mcp\"]\n    }}\n  }}\n}}\n"
            )
        );
        // Taken out again: the only one leaves mcpServers empty.
        assert_eq!(changed(&config(&got), None).unwrap(), "{\n  \"mcpServers\": {}\n}\n");
    }

    #[test]
    fn taking_it_out_keeps_the_servers_around_it() {
        let ours = "\"softpresenter\": {\"command\": \"/x/sp\", \"args\": [\"mcp\"]}";
        // First, middle and last of three.
        for (text, left) in [
            (
                format!("{{\"mcpServers\": {{{ours}, \"b\": {{}}, \"c\": 1}}}}"),
                "{\"mcpServers\": {\"b\": {}, \"c\": 1}}",
            ),
            (format!("{{\"mcpServers\": {{\"a\": [], {ours}, \"c\": 1}}}}"), "{\"mcpServers\": {\"a\": [], \"c\": 1}}"),
            (
                format!("{{\"mcpServers\": {{\"a\": [], \"b\": {{}}, {ours}}}}}"),
                "{\"mcpServers\": {\"a\": [], \"b\": {}}}",
            ),
            (
                format!("{{\n  \"mcpServers\": {{\n    \"a\": 1,\n    {ours}\n  }}\n}}"),
                "{\n  \"mcpServers\": {\n    \"a\": 1\n  }\n}",
            ),
        ] {
            assert_eq!(changed(&config(&text), None).unwrap(), left, "{text}");
        }
        // Replaced in place, with the indent of its line.
        let text = format!("{{\n  \"mcpServers\": {{\n    {ours},\n    \"z\": 2\n  }}\n}}");
        assert_eq!(
            changed(&config(&text), Some(&desktop())).unwrap(),
            format!(
                "{{\n  \"mcpServers\": {{\n    \"softpresenter\": {{\n      \"command\": \"{SP}\",\n      \"args\": [\"mcp\"]\n    }},\n    \"z\": 2\n  }}\n}}"
            )
        );
        // Keys written with escapes, and braces inside strings, are read as JSON reads them.
        let text = format!("{{\"x\": \"}}{{\\\"\", \"mcp\\u0053ervers\": {{\"a\\\\\": 1, {ours}}}}}");
        assert_eq!(changed(&config(&text), None).unwrap(), "{\"x\": \"}{\\\"\", \"mcp\\u0053ervers\": {\"a\\\\\": 1}}");
        // Two entries of the same name: refused, not guessed.
        let text = format!("{{\"mcpServers\": {{{ours}, {ours}}}}}");
        assert!(changed(&config(&text), None).is_err());
    }

    #[test]
    fn a_file_that_isnt_valid_json_is_left_untouched() {
        english(a_file_that_isnt_valid_json_is_left_untouched_in_english);
    }

    fn a_file_that_isnt_valid_json_is_left_untouched_in_english() {
        let base = scratch("invalid");
        let file = base.join("mcp.json");
        for text in [
            "{\"mcpServers\": {",
            "not json",
            "[1, 2]",
            "{\"mcpServers\": []}",
            "{\"mcpServers\": null}",
            "{// note\n}",
        ] {
            std::fs::write(&file, text).unwrap();
            assert!(matches!(file_status(&file, SP, false), Stands::Trouble(_)), "{text}");
            let refused = file_add(&file, &desktop(), false, false).expect_err(text);
            assert!(refused.contains("left it as it is"), "{refused}");
            assert!(file_remove(&file).is_err());
            assert_eq!(read(&file), text);
            assert!(!backup_of(&file).exists());
        }
        // Nothing but spaces counts as {}; a byte order mark is kept.
        std::fs::write(&file, "  \n").unwrap();
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Added));
        assert_eq!(value(&file)["mcpServers"]["softpresenter"]["command"], SP);
        std::fs::write(&file, "\u{feff}{\"a\": 1}").unwrap();
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Added));
        assert!(read(&file).starts_with("\u{feff}{\"a\": 1, \"mcpServers\""));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn an_entry_starting_another_program_is_asked_about_first() {
        english(an_entry_starting_another_program_is_asked_about_first_in_english);
    }

    fn an_entry_starting_another_program_is_asked_about_first_in_english() {
        let base = scratch("other");
        let file = base.join("claude_desktop_config.json");
        let before =
            "{\"mcpServers\": {\"softpresenter\": {\"command\": \"/Old/sp\", \"args\": [\"mcp\"], \"env\": {}}}}";
        std::fs::write(&file, before).unwrap();
        assert_eq!(file_status(&file, SP, false), Stands::Other { command: "/Old/sp".to_owned() });
        assert_eq!(file_add(&file, &desktop(), false, false), Ok(Done::Taken { command: "/Old/sp".to_owned() }));
        assert_eq!(read(&file), before);
        assert_eq!(file_add(&file, &desktop(), true, false), Ok(Done::Replaced));
        assert_eq!(value(&file), json!({"mcpServers": {"softpresenter": {"command": SP, "args": ["mcp"]}}}));
        assert_eq!(read(&backup_of(&file)), before);
        // The same program with other arguments is another entry too.
        std::fs::write(&file, format!("{{\"mcpServers\": {{\"softpresenter\": {{\"command\": \"{SP}\"}}}}}}")).unwrap();
        assert!(matches!(file_status(&file, SP, false), Stands::Other { .. }));
        // Windows reads a path in any case and with either slash.
        let windows = r"C:\Program Files\SoftPresenter\SoftPresenter.exe";
        let written = json!({"mcpServers": {"softpresenter": {"command": "c:/program files/softpresenter/SOFTPRESENTER.EXE", "args": ["mcp"]}}});
        assert_eq!(entry_state(&written, windows, true), Stands::Added);
        assert!(matches!(entry_state(&written, windows, false), Stands::Other { .. }));
        // What the window shows of another's command is one plain line.
        let odd = json!({"mcpServers": {"softpresenter": {"command": "/a\u{202e}b\nc"}}});
        assert_eq!(entry_state(&odd, SP, false), Stands::Other { command: "/ab c".to_owned() });
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_written_through() {
        english(a_link_is_never_written_through_in_english);
    }

    #[cfg(unix)]
    fn a_link_is_never_written_through_in_english() {
        let base = scratch("link");
        let real = base.join("dotfiles.json");
        std::fs::write(&real, "{}").unwrap();
        let file = base.join("claude_desktop_config.json");
        std::os::unix::fs::symlink(&real, &file).unwrap();
        assert!(matches!(file_status(&file, SP, false), Stands::Trouble(_)));
        let refused = file_add(&file, &desktop(), false, false).expect_err("refused");
        assert!(refused.contains("is a link"), "{refused}");
        assert!(file_remove(&file).is_err());
        assert_eq!(read(&real), "{}");
        assert!(std::fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
        // A folder that is a link: the same.
        let folder = base.join("Claude");
        std::os::unix::fs::symlink(&base, &folder).unwrap();
        assert!(file_add(&folder.join("x.json"), &desktop(), false, false).is_err());
        assert!(!base.join("x.json").exists());
        // A link where the copy goes is replaced, not followed.
        let plain = base.join("plain.json");
        std::fs::write(&plain, "{}").unwrap();
        std::os::unix::fs::symlink(&real, backup_of(&plain)).unwrap();
        assert_eq!(file_add(&plain, &desktop(), false, false), Ok(Done::Added));
        assert_eq!(read(&real), "{}");
        assert!(!std::fs::symlink_metadata(backup_of(&plain)).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(base);
    }

    fn around(system: System, home: &Path) -> Around {
        Around {
            system,
            home: Some(home.to_owned()),
            app_data: Some(home.join("AppData").join("Roaming")),
            path: None,
            everywhere: false,
            copilot_home: None,
            codex_home: None,
        }
    }

    #[test]
    fn each_app_is_found_in_its_own_place_and_only_there() {
        let home = scratch("places");
        // Nothing there: nothing found.
        for app in App::ALL {
            assert_eq!(place_in(app, &around(System::Mac, &home)), None, "{app:?}");
        }
        // The Gemini command line's folder alone isn't Antigravity.
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        assert_eq!(place_in(App::Antigravity, &around(System::Mac, &home)), None);
        for folder in [
            "Library/Application Support/Claude",
            ".cursor",
            ".copilot",
            ".gemini/antigravity",
            "AppData/Roaming/Claude",
        ] {
            std::fs::create_dir_all(home.join(folder)).unwrap();
        }
        let file = |app: App, system: System| match place_in(app, &around(system, &home)) {
            Some(Place::File(file)) => file.strip_prefix(&home).unwrap().to_string_lossy().into_owned(),
            other => panic!("{app:?}: {other:?}"),
        };
        assert_eq!(
            file(App::ClaudeDesktop, System::Mac),
            "Library/Application Support/Claude/claude_desktop_config.json"
        );
        assert_eq!(file(App::ClaudeDesktop, System::Windows), "AppData/Roaming/Claude/claude_desktop_config.json");
        assert_eq!(file(App::Cursor, System::Mac), ".cursor/mcp.json");
        assert_eq!(file(App::Copilot, System::Other), ".copilot/mcp-config.json");
        assert_eq!(file(App::Antigravity, System::Windows), ".gemini/config/mcp_config.json");
        // No Claude Desktop for Linux.
        assert_eq!(place_in(App::ClaudeDesktop, &around(System::Other, &home)), None);
        assert!(!App::ClaudeDesktop.made_for(System::Other) && App::Grok.made_for(System::Other));
        // The commands: where their installers put them, under the home folder.
        let mac = around(System::Mac, &home);
        assert_eq!(
            program_places(App::ClaudeCode, &mac),
            [home.join(".local/bin/claude"), home.join(".claude/local/claude")]
        );
        assert_eq!(program_places(App::Grok, &mac), [home.join(".grok/bin/grok"), home.join(".local/bin/grok")]);
        let windows = around(System::Windows, &home);
        assert_eq!(
            program_places(App::Codex, &windows),
            [home.join(".local").join("bin").join("codex.exe"), home.join("AppData/Roaming/npm/codex.cmd")]
        );
        // The real thing also looks in Homebrew's folder and the PATH (written in full only).
        let real =
            Around { everywhere: true, path: Some("/usr/bin:relative/bin".into()), ..around(System::Mac, &home) };
        assert_eq!(
            program_places(App::Codex, &real)[1..],
            [
                PathBuf::from("/opt/homebrew/bin/codex"),
                PathBuf::from("/usr/local/bin/codex"),
                PathBuf::from("/usr/bin/codex")
            ]
        );
        // A program found is a program with the settings file it changes.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let grok = home.join(".grok/bin/grok");
            std::fs::create_dir_all(grok.parent().unwrap()).unwrap();
            std::fs::write(&grok, "#!/bin/sh\n").unwrap();
            assert_eq!(place_in(App::Grok, &mac), None, "not marked as a program yet");
            std::fs::set_permissions(&grok, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(
                place_in(App::Grok, &mac),
                Some(Place::Program { program: grok, settings: Some(home.join(".grok/config.toml")) })
            );
        }
        let _ = std::fs::remove_dir_all(home);
    }

    /// The apps' answers, as Claude Code 2.1, Codex 0.169 and Grok Build 1.0 wrote them.
    #[test]
    fn each_apps_answers_are_read_and_anything_else_is_trouble() {
        english(each_apps_answers_are_read_and_anything_else_is_trouble_in_english);
    }

    fn each_apps_answers_are_read_and_anything_else_is_trouble_in_english() {
        let said = |ok: bool, words: &str| Said { ok, out: words.to_owned(), words: words.to_owned() };
        let claude = |scope: &str, command: &str| {
            said(
                true,
                &format!(
                    "softpresenter:\n  Scope: {scope}\n  Status: \u{2714} Connected\n  Type: stdio\n  Command: {command}\n  Args: mcp\n  Environment:\n\nTo remove this server, run: claude mcp remove softpresenter -s user"
                ),
            )
        };
        let read = |app: App, said: &Said| read_answer(app, said, SP, false);
        assert_eq!(
            read(App::ClaudeCode, &claude("User config (available in all your projects)", SP)),
            (Stands::Added, "user")
        );
        assert_eq!(
            read(App::ClaudeCode, &claude("Local config (private to you in this project)", SP)),
            (Stands::Added, "local")
        );
        assert_eq!(
            read(App::ClaudeCode, &claude("User config", "/some where/sp")),
            (Stands::Other { command: "/some where/sp".to_owned() }, "user")
        );
        let missing = said(false, "No MCP server named \"softpresenter\". Run `claude mcp add` to add one.");
        assert_eq!(read(App::ClaudeCode, &missing).0, Stands::Missing);
        assert!(matches!(read(App::ClaudeCode, &said(false, "error: unknown command 'get'")).0, Stands::Trouble(_)));
        assert!(matches!(read(App::ClaudeCode, &said(true, "something new")).0, Stands::Trouble(_)));
        // Codex.
        let codex = format!(
            "{{\n  \"name\": \"softpresenter\",\n  \"enabled\": true,\n  \"transport\": {{\n    \"type\": \"stdio\",\n    \"command\": \"{SP}\",\n    \"args\": [\n      \"mcp\"\n    ],\n    \"env\": null\n  }}\n}}"
        );
        assert_eq!(read(App::Codex, &said(true, &codex)).0, Stands::Added);
        assert_eq!(
            read(App::Codex, &said(true, &codex.replace(SP, "/old/sp"))).0,
            Stands::Other { command: "/old/sp".to_owned() }
        );
        assert_eq!(
            read(App::Codex, &said(false, "Error: No MCP server named 'softpresenter' found.")).0,
            Stands::Missing
        );
        assert!(matches!(read(App::Codex, &said(true, "softpresenter\n  enabled: true")).0, Stands::Trouble(_)));
        // Grok Build.
        let grok = format!(
            "[\n  {{\"command\": \"/x\", \"args\": [], \"enabled\": true, \"name\": \"other\", \"scope\": \"user\"}},\n  {{\"command\": \"{SP}\", \"args\": [\"mcp\"], \"enabled\": true, \"name\": \"softpresenter\", \"scope\": \"project\"}}\n]"
        );
        assert_eq!(read(App::Grok, &said(true, &grok)), (Stands::Added, "project"));
        assert_eq!(read(App::Grok, &said(true, "[]")), (Stands::Missing, "user"));
        // A notice on the error output doesn't spoil the answer on the standard one.
        let noticed = Said { ok: true, out: grok.clone(), words: format!("{grok}\nA new version is out") };
        assert_eq!(read_answer(App::Grok, &noticed, SP, false).0, Stands::Added);
        assert!(matches!(read(App::Grok, &said(true, "{}")).0, Stands::Trouble(_)));
        assert_eq!(
            said_words(
                App::ClaudeCode,
                &said(false, "\n MCP server softpresenter already exists in user config\nmore\nand more")
            ),
            "Claude Code said: MCP server softpresenter already exists in user config more"
        );
    }

    /// A stand-in for an app's command that keeps each run's words, one run a line, and answers
    /// as the app does (Claude Code's, Codex's or Grok Build's way), from a file of its own.
    #[cfg(unix)]
    fn stand_in(base: &Path, app: App) -> (PathBuf, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let program = base.join("tool");
        let log = base.join("runs.txt");
        let entry = base.join("entry.txt");
        let (get, missing, add) = match app {
            App::ClaudeCode => (
                "printf 'softpresenter:\\n  Scope: User config\\n  Command: %s\\n  Args: mcp\\n' \"$(cat \"$E\")\"",
                "echo 'No MCP server named \"softpresenter\".' >&2; exit 1",
                "if [ -f \"$E\" ]; then echo 'MCP server softpresenter already exists in user config' >&2; exit 1; fi; printf '%s' \"$7\" > \"$E\"",
            ),
            App::Codex => (
                "printf '{\"name\": \"softpresenter\", \"transport\": {\"type\": \"stdio\", \"command\": \"%s\", \"args\": [\"mcp\"]}}' \"$(cat \"$E\")\"",
                "echo \"Error: No MCP server named 'softpresenter' found.\" >&2; exit 1",
                "printf '%s' \"$5\" > \"$E\"",
            ),
            _ => (
                "printf '[{\"name\": \"softpresenter\", \"command\": \"%s\", \"args\": [\"mcp\"], \"scope\": \"user\"}]' \"$(cat \"$E\")\"",
                "echo '[]'",
                "printf '%s' \"$7\" > \"$E\"",
            ),
        };
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nE='{entry}'\nprintf '%s|' \"$@\" >> '{log}'\necho >> '{log}'\ncase \"$2\" in\n  get|list) if [ -f \"$E\" ]; then {get}; else {missing}; fi ;;\n  add) {add} ;;\n  remove) rm -f \"$E\" ;;\nesac\n",
                log = log.display(),
                entry = entry.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (program, log, entry)
    }

    #[cfg(unix)]
    #[test]
    fn each_apps_command_is_asked_with_each_word_apart() {
        english(each_apps_command_is_asked_with_each_word_apart_in_english);
    }

    #[cfg(unix)]
    fn each_apps_command_is_asked_with_each_word_apart_in_english() {
        let sp = "/opt/it's here/sp";
        for (app, add, remove, get) in [
            (
                App::ClaudeCode,
                "mcp|add|--scope|user|softpresenter|--|",
                "mcp|remove|--scope|user|softpresenter|",
                "mcp|get|softpresenter|",
            ),
            (App::Codex, "mcp|add|softpresenter|--|", "mcp|remove|softpresenter|", "mcp|get|softpresenter|--json|"),
            (App::Grok, "mcp|add|-s|user|softpresenter|--|", "mcp|remove|-s|user|softpresenter|", "mcp|list|--json|"),
        ] {
            let base = scratch(&format!("command-{}", app.word()));
            let (tool, log, entry) = stand_in(&base, app);
            let settings = base.join("config.toml");
            std::fs::write(&settings, "# mine\n").unwrap();
            let status = || command_status(app, &tool, sp, false).0;
            let added = |replace| command_add(app, &tool, Some(&settings), sp, replace, false);
            assert_eq!(status(), Stands::Missing, "{app:?}");
            assert_eq!(added(false), Ok(Done::Added), "{app:?}");
            // Its settings were copied first.
            assert_eq!(read(&backup_of(&settings)), "# mine\n");
            assert_eq!(status(), Stands::Added, "{app:?}");
            assert_eq!(added(false), Ok(Done::Already));
            assert_eq!(command_remove(app, &tool, Some(&settings), sp, false), Ok(Done::Removed));
            assert_eq!(command_remove(app, &tool, Some(&settings), sp, false), Ok(Done::NotThere));
            assert_eq!(
                read(&log),
                format!("{get}\n{get}\n{add}{sp}|mcp|\n{get}\n{get}\n{get}\n{remove}\n{get}\n"),
                "{app:?}"
            );
            // Another program's entry: asked about, then replaced.
            std::fs::write(&entry, "/Old/sp").unwrap();
            assert_eq!(added(false), Ok(Done::Taken { command: "/Old/sp".to_owned() }), "{app:?}");
            assert_eq!(added(true), Ok(Done::Replaced), "{app:?}");
            assert_eq!(read(&entry), sp);
            // An answer that can't be read: Codex and Grok Build (whose own add would replace
            // another program's entry without asking) add nothing unless Replace was chosen.
            std::fs::write(&entry, "/Old/sp").unwrap();
            let log_before = read(&log);
            std::fs::write(
                &tool,
                format!(
                    "#!/bin/sh\nprintf '%s|' \"$@\" >> '{}'\necho >> '{}'\necho 'not JSON'\n",
                    log.display(),
                    log.display()
                ),
            )
            .unwrap();
            if app == App::ClaudeCode {
                assert_eq!(added(false), Ok(Done::Added), "Claude Code's own add refuses over an entry");
            } else {
                assert!(matches!(added(false), Err(why) if why.contains("not JSON")), "{app:?}");
                assert!(!read(&log)[log_before.len()..].contains("|add|"), "{app:?}");
            }
            assert_eq!(read(&entry), "/Old/sp");
            // What it says when it refuses comes back plain.
            std::fs::write(&tool, "#!/bin/sh\necho 'Something \u{202e}went wrong' >&2\nexit 3\n").unwrap();
            assert_eq!(added(false), Err(format!("{} said: Something went wrong", app.name())), "{app:?}");
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn the_words_name_the_app_and_are_said_in_spanish_too() {
        english(the_words_name_the_app_and_are_said_in_spanish_too_in_english);
    }

    fn the_words_name_the_app_and_are_said_in_spanish_too_in_english() {
        let place = Place::File(PathBuf::from("/x/mcp.json"));
        let (_, words) = crate::i18n::with_language(crate::i18n::Language::Spanish, || {
            done_words(App::Cursor, &Done::Added, &place, false)
        });
        assert_ne!(words, "Added. Quit and reopen Cursor to use it.");
        assert!(words.contains("Cursor"), "{words}");
        let (tone, words) = done_words(App::Grok, &Done::Taken { command: "/Old/sp".to_owned() }, &place, true);
        assert_eq!(tone, Tone::Care);
        assert!(words.ends_with("sp mcp connect grok-build --replace points it at this copy."), "{words}");
        for app in App::ALL {
            assert_eq!(App::from_word(app.word()), Some(app));
        }
        for company in Company::ALL {
            assert_eq!(Company::from_word(company.word()), Some(company));
        }
        assert_eq!(Company::from_word("ChatGPT"), Some(Company::ChatGpt));
        assert_eq!(App::from_word("gemini"), None);
    }

    #[test]
    fn a_companys_tile_covers_its_apps_and_says_how_they_stand_together() {
        english(tile_words_in_english);
    }

    fn tile_words_in_english() {
        assert_eq!(Company::Claude.apps().last(), Some(&App::ClaudeCode));
        assert_eq!(Company::ChatGpt.apps(), [App::Codex]);
        assert_eq!(Company::Gemini.apps(), [App::Antigravity]);
        assert_eq!(Company::Grok.apps(), [App::Grok]);
        assert!(App::others().contains(&App::Cursor) && App::others().contains(&App::Copilot));
        let file = |name: &str| Place::File(PathBuf::from(format!("/x/{name}")));
        let tool = Place::Program { program: PathBuf::from("/x/claude"), settings: None };
        let desktop = (App::ClaudeDesktop, file("claude_desktop_config.json"));
        let code = (App::ClaudeCode, tool);
        let with = |pairs: &[((App, Place), Stands)]| -> Vec<(App, Place, Stands)> {
            pairs.iter().map(|((app, place), stands)| (*app, place.clone(), stands.clone())).collect()
        };
        assert_eq!(group_words(&[], false), (Tone::Plain, "Not found on this computer.".to_owned()));
        assert_eq!(
            group_words(&with(&[(desktop.clone(), Stands::Missing), (code.clone(), Stands::Missing)]), false),
            (Tone::Plain, "Found: Claude Desktop and Claude Code. Not added yet.".to_owned())
        );
        assert_eq!(
            group_words(&with(&[(desktop.clone(), Stands::Added), (code.clone(), Stands::Missing)]), false),
            (Tone::Plain, "Added to Claude Desktop; not yet to Claude Code.".to_owned())
        );
        assert_eq!(
            group_words(&with(&[(desktop.clone(), Stands::Added), (code.clone(), Stands::Added)]), false),
            (Tone::Good, "Added to Claude Desktop and Claude Code.".to_owned())
        );
        let other = group_words(
            &with(&[(desktop.clone(), Stands::Added), (code.clone(), Stands::Other { command: "/o".to_owned() })]),
            false,
        );
        assert_eq!(other.0, Tone::Care);
        assert!(other.1.starts_with("Claude Code has a \u{201c}softpresenter\u{201d} entry"), "{}", other.1);
        // What a click did to them, in one line.
        let places = [desktop.clone(), code.clone()];
        assert_eq!(
            group_done_words(
                &[(App::ClaudeDesktop, Ok(Done::Added)), (App::ClaudeCode, Ok(Done::Added))],
                &places,
                false
            )
            .1,
            "Added. Quit and reopen Claude Desktop and Claude Code to use it."
        );
        assert_eq!(
            group_done_words(
                &[(App::ClaudeDesktop, Ok(Done::Removed)), (App::ClaudeCode, Ok(Done::NotThere))],
                &places,
                false
            )
            .1,
            "Removed. Quit and reopen Claude Desktop for it to go."
        );
        assert_eq!(
            group_done_words(
                &[(App::ClaudeDesktop, Ok(Done::Added)), (App::ClaudeCode, Err("No.".to_owned()))],
                &places,
                false
            ),
            (Tone::Trouble, "Claude Code: No.".to_owned())
        );
        // Copy setup: each app's settings, with the path filled in.
        let setup = Company::Claude.setup(Path::new(SP));
        assert!(setup.contains(&format!("claude mcp add --scope user softpresenter -- {SP} mcp")), "{setup}");
        assert!(setup.contains("\"mcpServers\"") || cfg!(target_os = "linux"), "{setup}");
        assert!(Company::Grok.setup(Path::new(SP)).contains("[mcp_servers.softpresenter]"));
        assert!(app_setup(App::Copilot, Path::new(SP)).contains("\"tools\": [\"*\"]"));
    }
}
