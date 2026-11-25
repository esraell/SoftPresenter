//! The `sp` command (and its full name, `softpresenter`) put where a terminal finds it: from
//! Settings › Command line, the menu bar, `softpresenter install-cli`, and once
//! by itself when an installed copy of the app starts ([`at_start`]).
//!
//! On a Mac and on Linux it is two links to this program in a folder a terminal looks in, and,
//! when that folder is `~/.local/bin` and a new terminal wouldn't look there, one marked line in
//! the person's own shell startup file (`~/.zshrc` for zsh…). On Windows it is two small batch
//! files in a folder of the user's own, put on the user's PATH. Remove takes away all of that.
//! Nothing needs an administrator, and no system file is touched.

use std::path::{Path, PathBuf};

use crate::i18n::{t, tf};

/// The names the command is installed as on a Mac and Linux: the short one to type, and the
/// full one (Windows has the same as `.cmd` files).
#[cfg(unix)]
const COMMAND_NAMES: [&str; 2] = ["sp", "softpresenter"];

/// The line written above the one that puts `~/.local/bin` on the PATH, so that Remove (and a
/// person reading the file) can tell it is SoftPresenter's.
#[cfg(any(unix, test))]
const MARK: &str = "# Added by SoftPresenter: the sp command";

/// The lines SoftPresenter writes under [`MARK`]: for zsh and bash, and for fish.
#[cfg(any(unix, test))]
const PATH_LINES: [&str; 2] = ["export PATH=\"$HOME/.local/bin:$PATH\"", "fish_add_path ~/.local/bin"];

/// The file in the data folder that remembers the start-up install: the program it was done
/// for (it is done again when the app has moved), or [`REMOVED`] after Remove.
const REMEMBERED: &str = "sp-command.txt";
const REMOVED: &str = "removed";

/// Where the sp command stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not in any of the places SoftPresenter puts it.
    Missing,
    /// Installed, and runs this program. `findable` when a terminal opened now finds it (its
    /// folder is on the PATH).
    Here { link: PathBuf, findable: bool },
    /// Runs another copy of SoftPresenter, which is still there.
    Elsewhere { link: PathBuf, program: PathBuf },
    /// Runs a copy of SoftPresenter that has moved or been deleted.
    Gone { link: PathBuf },
}

/// What Install did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// Where the short name is (`sp`, or `sp.cmd` on Windows).
    pub link: PathBuf,
    /// The shell startup file a line was added to, so a terminal looks in the link's folder.
    pub shell_file: Option<PathBuf>,
    /// A terminal doesn't look in the link's folder and SoftPresenter couldn't set that up (a
    /// shell it doesn't know): the person adds the folder to their PATH.
    pub add_folder: bool,
    /// Anything changed: a link made or pointed here, a line added, the PATH changed.
    pub changed: bool,
}

/// What Remove did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    /// How many links (or batch files) went.
    pub commands: usize,
    /// The shell startup files SoftPresenter's line was taken out of.
    pub shell_files: Vec<PathBuf>,
    /// The folder came off the PATH (Windows).
    pub path_removed: bool,
}

/// How a line of the Command line card looks: its dot's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Plain,
    Care,
    Trouble,
}

/// What the Command line card shows: where sp stands (read when the card comes into view and
/// after each click), what the last click did, and the pass it was last drawn in.
#[derive(Default)]
pub struct Card {
    pub status: Option<Status>,
    pub said: Option<(Tone, String)>,
    pub drawn: Option<u64>,
}

/// Where the sp command stands now.
pub fn status() -> Status {
    #[cfg(unix)]
    {
        Place::here().map_or(Status::Missing, |place| status_in(&place))
    }
    #[cfg(windows)]
    {
        windows_status()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Status::Missing
    }
}

/// Installs the sp command, as Settings' Install does, and remembers it was done for this
/// program (so the start-up install leaves it be, and Remove is forgotten).
pub fn install() -> Result<Installed, String> {
    #[cfg(unix)]
    let done = Place::here().and_then(|place| install_in(&place));
    #[cfg(windows)]
    let done = windows_install();
    #[cfg(not(any(unix, windows)))]
    let done: Result<Installed, String> =
        Err(t("settings.cli.not_here", "Installing the command line tool works on macOS, Windows and Linux.")
            .to_owned());
    if done.is_ok()
        && let Some(program) = this_program()
    {
        remember(&program.display().to_string());
    }
    done
}

/// Removes the sp command and whatever SoftPresenter added for it, and remembers that it was
/// removed, so the app doesn't put it back when it starts.
pub fn remove() -> Result<Removed, String> {
    #[cfg(unix)]
    let done = Place::here().and_then(|place| remove_in(&place));
    #[cfg(windows)]
    let done = windows_remove();
    #[cfg(not(any(unix, windows)))]
    let done: Result<Removed, String> = Ok(Removed::default());
    if done.is_ok() {
        remember(REMOVED);
    }
    done
}

/// At start, once for each place the app is installed in: installs the sp command as Install
/// does, quietly, and returns what the notices say when that changed something. Never in a test
/// copy (`quiet`, its own data folder, a capture run), never from a place the app is only
/// passing through (see [`installed_place`]), never over an sp that runs another
/// copy still there (the card offers to point it here), and never again after Remove.
pub fn at_start(quiet: bool) -> Option<String> {
    let test_copy = quiet
        || std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty())
        || std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("SOFTPRESENTER_CAPTURE"));
    let program = this_program()?;
    let home = std::env::var("HOME").unwrap_or_default();
    let temp = resolved(std::env::temp_dir());
    let place_ok = installed_place(
        &program.display().to_string(),
        &home,
        &temp.display().to_string(),
        cfg!(target_os = "macos"),
        cfg!(windows),
    );
    let remembered = remembered_file().and_then(|file| std::fs::read_to_string(file).ok());
    // The status is read only when it may matter, not at every start.
    let decided = decide(test_copy, place_ok, remembered.as_deref(), &program, status);
    match decided {
        AtStart::Leave => None,
        AtStart::Remember => {
            remember(&program.display().to_string());
            None
        }
        AtStart::Install => match install() {
            Ok(done) => done.changed.then(|| ready_words(&done, cfg!(windows))),
            Err(problem) => {
                // Said in the log only: nobody asked for it, and the card shows how it stands.
                crate::applog::event("command line", &problem);
                remember(&program.display().to_string());
                None
            }
        },
    }
}

/// What the start-up install does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtStart {
    /// Nothing, and nothing remembered (it is looked at again next time).
    Leave,
    /// Nothing to do, and nothing to look at again until the app moves.
    Remember,
    /// Install (and remember).
    Install,
}

/// The start-up install's choice: `remembered` is the data folder's [`REMEMBERED`] file, and
/// `status` is asked only when it is needed.
fn decide(
    test_copy: bool,
    place_ok: bool,
    remembered: Option<&str>,
    program: &Path,
    status: impl FnOnce() -> Status,
) -> AtStart {
    if test_copy || !place_ok {
        return AtStart::Leave;
    }
    let remembered = remembered.map(str::trim);
    if remembered == Some(REMOVED) || remembered == Some(program.display().to_string().as_str()) {
        return AtStart::Leave;
    }
    match status() {
        // Another copy's: never taken from it.
        Status::Elsewhere { .. } => AtStart::Leave,
        Status::Here { findable: true, .. } => AtStart::Remember,
        Status::Here { findable: false, .. } | Status::Gone { .. } | Status::Missing => AtStart::Install,
    }
}

/// Whether the program is in a place the app is installed in, rather than one it only passes
/// through. On a Mac: inside `/Applications` or `~/Applications` (not a disk image, Downloads, or
/// a build folder; a Mac runs an app it has moved aside from somewhere else, too). Elsewhere:
/// anywhere but the temporary folder, a mounted disk or image (`/media`, `/run/media`) and a
/// folder Cargo builds in. Paths as text, so each system's rules can be tried on any.
fn installed_place(program: &str, home: &str, temp: &str, mac: bool, windows: bool) -> bool {
    let plain = |path: &str| {
        let path = path.replace('\\', "/");
        let path = path.trim_end_matches('/').to_owned();
        if windows { path.to_lowercase() } else { path }
    };
    let program = plain(program);
    let under = |folder: &str| {
        let folder = plain(folder);
        !folder.is_empty() && program.starts_with(&format!("{folder}/"))
    };
    if mac {
        return under("/Applications") || (!home.is_empty() && under(&format!("{home}/Applications")));
    }
    let passing = under(temp) || (!windows && ["/tmp", "/var/tmp", "/media", "/run/media"].into_iter().any(under));
    let parts: Vec<&str> = program.split('/').collect();
    let built = parts
        .iter()
        .position(|part| *part == "target")
        .is_some_and(|at| parts[at + 1..].iter().any(|part| *part == "debug" || *part == "release"));
    !passing && !built
}

fn this_program() -> Option<PathBuf> {
    std::env::current_exe().ok().map(resolved)
}

/// A path with its links resolved, on a Mac and Linux (where the app's program is reached
/// through them, and `/tmp` is one). Windows' own form of a resolved path (`\\?\C:\…`) is no
/// help, so there it stays as it is.
fn resolved(path: PathBuf) -> PathBuf {
    if cfg!(windows) { path } else { std::fs::canonicalize(&path).unwrap_or(path) }
}

fn remembered_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join(REMEMBERED))
}

/// Writes the [`REMEMBERED`] file. A data folder that can't be written to only means the
/// start-up install looks again next time.
fn remember(what: &str) {
    if let Some(file) = remembered_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, format!("{what}\n"));
    }
}

/// A path as the window shows it: the home folder as `~` (on a Mac and Linux).
pub(crate) fn shown(path: &Path) -> String {
    #[cfg(unix)]
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from) {
        // As written, or with its links resolved, as a program's own path is.
        let resolved = resolved(home.clone());
        if let Some(rest) = [home, resolved].iter().find_map(|home| path.strip_prefix(home).ok()) {
            return if rest.as_os_str().is_empty() { "~".to_owned() } else { format!("~/{}", rest.display()) };
        }
    }
    path.display().to_string()
}

/// The folder a link is in, as the window shows it.
fn shown_folder(link: &Path) -> String {
    shown(link.parent().unwrap_or(link))
}

/// The copy of the app a program is part of, as the window names it: on a Mac the `.app`.
fn shown_program(program: &Path) -> String {
    let bundle = program.ancestors().find(|dir| dir.extension().is_some_and(|ext| ext == "app"));
    shown(bundle.unwrap_or(program))
}

/// What the card says about where sp stands (`windows`: in Windows' words, `cfg!(windows)`).
pub fn status_words(status: &Status, windows: bool) -> (Tone, String) {
    match status {
        Status::Missing => (
            Tone::Plain,
            t(
                "settings.cli.state.missing",
                "sp isn\u{2019}t installed yet. Install puts it where a terminal finds it.",
            )
            .to_owned(),
        ),
        Status::Here { link, findable: true } => (
            Tone::Good,
            if windows {
                tf(
                    "settings.cli.state.here_windows",
                    "sp is installed ({path}). In a Command Prompt, type: sp status (in PowerShell: softpresenter status)",
                    &[("path", &shown(link))],
                )
            } else {
                tf(
                    "settings.cli.state.here",
                    "sp is installed ({path}). In a terminal, type: sp status",
                    &[("path", &shown(link))],
                )
            },
        ),
        Status::Here { link, findable: false } => (
            Tone::Care,
            tf(
                "settings.cli.state.not_found",
                "sp is installed ({path}), but a terminal doesn\u{2019}t look in its folder yet. Install sets that up.",
                &[("path", &shown(link))],
            ),
        ),
        Status::Elsewhere { program, .. } => (
            Tone::Care,
            tf(
                "settings.cli.state.elsewhere",
                "sp opens another copy of SoftPresenter ({copy}). Use This Copy points it here.",
                &[("copy", &shown_program(program))],
            ),
        ),
        Status::Gone { .. } => (
            Tone::Care,
            t(
                "settings.cli.state.gone",
                "sp opens a copy of SoftPresenter that has moved or been deleted. Use This Copy points it here.",
            )
            .to_owned(),
        ),
    }
}

/// What is said once Install has run (in the card, or in the notices after the menu bar's
/// Install Command Line Tool…, or in the terminal after `install-cli`).
pub fn installed_words(done: &Installed, windows: bool) -> String {
    let folder = shown_folder(&done.link);
    if windows {
        tf(
            "settings.cli.installed_windows",
            "Installed in {folder}, which is on your PATH. Open a new Command Prompt and type: sp status (in PowerShell: softpresenter status)",
            &[("folder", &folder)],
        )
    } else if let Some(file) = &done.shell_file {
        tf(
            "settings.cli.installed_file",
            "Installed in {folder}, and SoftPresenter added a line for it to {file}. Open a new terminal window and type: sp status",
            &[("folder", &folder), ("file", &shown(file))],
        )
    } else if done.add_folder {
        tf(
            "settings.cli.installed_add_folder",
            "Installed in {folder}, but a terminal doesn\u{2019}t look there yet: add {folder} to your PATH, then type: sp status",
            &[("folder", &folder)],
        )
    } else {
        tf(
            "settings.cli.installed",
            "Installed in {folder}. Open a new terminal window and type: sp status",
            &[("folder", &folder)],
        )
    }
}

/// What the notices say when the app installed sp by itself as it started.
pub fn ready_words(done: &Installed, windows: bool) -> String {
    let path = shown(&done.link);
    if windows {
        tf(
            "notices.command.ready_windows",
            "The sp command is ready for the terminal ({path}), and its folder is on your PATH. In PowerShell, type softpresenter instead. Settings \u{203a} Command line can remove it.",
            &[("path", &path)],
        )
    } else if let Some(file) = &done.shell_file {
        tf(
            "notices.command.ready_file",
            "The sp command is ready for the terminal ({path}). SoftPresenter added a line for it to {file}. Settings \u{203a} Command line can remove it.",
            &[("path", &path), ("file", &shown(file))],
        )
    } else if done.add_folder {
        tf(
            "notices.command.ready_add_folder",
            "The sp command is in {folder}; add that folder to your PATH to use it in a terminal. Settings \u{203a} Command line can remove it.",
            &[("folder", &shown_folder(&done.link))],
        )
    } else {
        tf(
            "notices.command.ready",
            "The sp command is ready for the terminal ({path}). Settings \u{203a} Command line can remove it.",
            &[("path", &path)],
        )
    }
}

/// What the card says once Remove has run.
pub fn removed_words(done: &Removed) -> String {
    if done.commands == 0 && done.shell_files.is_empty() && !done.path_removed {
        return t("settings.cli.removed_nothing", "There was nothing to remove.").to_owned();
    }
    if done.path_removed {
        return t("settings.cli.removed_windows", "Removed sp and softpresenter, and took their folder off your PATH.")
            .to_owned();
    }
    if done.shell_files.is_empty() {
        return t("settings.cli.removed", "Removed sp and softpresenter.").to_owned();
    }
    let files: Vec<String> = done.shell_files.iter().map(|file| shown(file)).collect();
    tf(
        "settings.cli.removed_file",
        "Removed sp and softpresenter, and the line SoftPresenter added to {file}.",
        &[("file", &files.join(", "))],
    )
}

// ---------------------------------------------------------------------------------------------
// A Mac and Linux: links, and a line in the shell's startup file.

/// Everything the Mac and Linux install reads from its surroundings, so that tests can give
/// their own.
#[cfg(unix)]
pub struct Place {
    /// This program, with every link resolved.
    pub program: PathBuf,
    pub home: PathBuf,
    /// Where sp is looked for, in order: `~/.local/bin`, where a new install goes, then
    /// Homebrew's folder and `/usr/local/bin`, which earlier versions used when they could.
    pub folders: Vec<PathBuf>,
    /// This program's own PATH.
    pub path: Option<std::ffi::OsString>,
    /// The person's shell, as `$SHELL` names it.
    pub shell: String,
    /// A Mac's terminals start login shells; Linux's don't.
    pub mac: bool,
    /// Where zsh keeps its startup files, when not the home folder.
    pub zdotdir: Option<PathBuf>,
    /// Where fish keeps its configuration (`$XDG_CONFIG_HOME`), when not `~/.config`.
    pub config_home: Option<PathBuf>,
}

#[cfg(unix)]
impl Place {
    fn here() -> Result<Place, String> {
        let program = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| {
            tf("settings.cli.no_program", "Could not find this program: {error}", &[("error", &e.to_string())])
        })?;
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
        let home =
            var("HOME").ok_or_else(|| t("settings.cli.no_home", "Could not find the home folder.").to_owned())?;
        Ok(Place {
            program,
            folders: vec![home.join(".local/bin"), PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")],
            path: std::env::var_os("PATH"),
            shell: std::env::var("SHELL").unwrap_or_default(),
            mac: cfg!(target_os = "macos"),
            zdotdir: var("ZDOTDIR"),
            config_home: var("XDG_CONFIG_HOME"),
            home,
        })
    }

    fn local(&self) -> PathBuf {
        self.home.join(".local/bin")
    }
}

/// What is at the place of a link.
#[cfg(unix)]
enum Found {
    Nothing,
    /// A link to a program called softpresenter: one SoftPresenter made. `exists` when that
    /// program is still there.
    Ours {
        target: PathBuf,
        exists: bool,
    },
    /// Something else: never changed.
    Other,
}

#[cfg(unix)]
fn look(link: &Path) -> Found {
    let Ok(meta) = std::fs::symlink_metadata(link) else { return Found::Nothing };
    if !meta.file_type().is_symlink() {
        return Found::Other;
    }
    let Ok(target) = std::fs::read_link(link) else { return Found::Other };
    let target = match link.parent() {
        Some(dir) if target.is_relative() => dir.join(target),
        _ => target,
    };
    if !target.file_name().is_some_and(|name| name.eq_ignore_ascii_case("softpresenter")) {
        return Found::Other;
    }
    Found::Ours { exists: target.exists(), target }
}

#[cfg(unix)]
fn status_in(place: &Place) -> Status {
    for folder in &place.folders {
        let link = folder.join(COMMAND_NAMES[0]);
        if let Found::Ours { target, exists } = look(&link) {
            if !exists {
                return Status::Gone { link };
            }
            let program = std::fs::canonicalize(&target).unwrap_or(target);
            return if program == place.program {
                Status::Here { findable: findable(place, folder), link }
            } else {
                Status::Elsewhere { link, program }
            };
        }
    }
    Status::Missing
}

/// Puts the links in: beside an earlier install of SoftPresenter's (which may be in Homebrew's
/// folder or `/usr/local/bin`), else in `~/.local/bin`, the person's own, made if it isn't
/// there. Nothing new goes in a folder shared with other programs. A link to another copy is
/// pointed here; anything else by those names is left alone. Then, when the folder is
/// `~/.local/bin` and a new terminal wouldn't look there, a line in the shell's startup file.
#[cfg(unix)]
fn install_in(place: &Place) -> Result<Installed, String> {
    let ours = |dir: &&PathBuf| matches!(look(&dir.join(COMMAND_NAMES[0])), Found::Ours { .. });
    let folder = match place.folders.iter().find(ours) {
        Some(dir) => dir.clone(),
        None => {
            let local = place.local();
            std::fs::create_dir_all(&local).map_err(|e| {
                tf(
                    "settings.cli.not_made",
                    "Could not make {path}: {error}",
                    &[("path", &shown(&local)), ("error", &e.to_string())],
                )
            })?;
            local
        }
    };
    let mut done = Installed { link: folder.join(COMMAND_NAMES[0]), ..Installed::default() };
    for name in COMMAND_NAMES {
        let link = folder.join(name);
        match look(&link) {
            Found::Other => {
                return Err(tf(
                    "settings.cli.taken",
                    "{path} is another program, so SoftPresenter left it alone.",
                    &[("path", &shown(&link))],
                ));
            }
            Found::Ours { target, .. } => {
                if std::fs::canonicalize(&target).is_ok_and(|program| program == place.program) {
                    continue;
                }
                std::fs::remove_file(&link).map_err(|e| {
                    tf(
                        "settings.cli.not_replaced",
                        "Could not replace {path}: {error}",
                        &[("path", &shown(&link)), ("error", &e.to_string())],
                    )
                })?;
            }
            Found::Nothing => {}
        }
        std::os::unix::fs::symlink(&place.program, &link).map_err(|e| {
            tf(
                "settings.cli.not_made",
                "Could not make {path}: {error}",
                &[("path", &shown(&link)), ("error", &e.to_string())],
            )
        })?;
        done.changed = true;
    }
    if !findable(place, &folder) {
        let files = shell_files(place);
        match files.write {
            Some(file) => {
                add_line(&file, files.line)?;
                done.shell_file = Some(file);
                done.changed = true;
            }
            None => done.add_folder = true,
        }
    }
    Ok(done)
}

/// Takes away every link to a SoftPresenter program by those names in the folders, and the
/// marked line from every shell startup file SoftPresenter may have written to.
#[cfg(unix)]
fn remove_in(place: &Place) -> Result<Removed, String> {
    let mut done = Removed::default();
    for folder in &place.folders {
        for name in COMMAND_NAMES {
            let link = folder.join(name);
            if matches!(look(&link), Found::Ours { .. }) {
                std::fs::remove_file(&link).map_err(|e| {
                    tf(
                        "settings.cli.not_removed",
                        "Could not remove {path}: {error}",
                        &[("path", &shown(&link)), ("error", &e.to_string())],
                    )
                })?;
                done.commands += 1;
            }
        }
    }
    for file in every_shell_file(place) {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        if let Some(shorter) = without_line(&text) {
            // Written in place, so a startup file that is a link (to a folder of dotfiles)
            // stays one.
            std::fs::write(&file, shorter).map_err(|e| {
                tf(
                    "settings.cli.not_written",
                    "Could not change {path}: {error}",
                    &[("path", &shown(&file)), ("error", &e.to_string())],
                )
            })?;
            done.shell_files.push(file);
        }
    }
    Ok(done)
}

/// Whether a terminal opened now looks in the folder. Homebrew's folder and `/usr/local/bin`
/// are on a terminal's PATH already (macOS puts `/usr/local/bin` there itself); `~/.local/bin`
/// is when this program's PATH has it (an app opened from a Mac's Dock gets a short PATH of its
/// own, so that alone doesn't say no) or a startup file the shell reads puts it there.
#[cfg(unix)]
fn findable(place: &Place, folder: &Path) -> bool {
    if folder != place.local() {
        return true;
    }
    on_path(place.path.as_deref(), folder)
        || shell_files(place).read.iter().any(|file| {
            std::fs::read(file).is_ok_and(|text| puts_on_path(&String::from_utf8_lossy(&text), folder, &place.home))
        })
}

#[cfg(unix)]
fn on_path(path: Option<&std::ffi::OsStr>, folder: &Path) -> bool {
    path.is_some_and(|path| std::env::split_paths(path).any(|dir| dir == folder))
}

/// The shells SoftPresenter knows how to set up.
#[cfg(any(unix, test))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Shell {
    Zsh,
    Bash,
    Fish,
    /// Another shell (tcsh, nushell…): nothing is written for it.
    Other,
}

/// The shell `$SHELL` names. Without one, the system's usual: zsh on a Mac, bash on Linux.
#[cfg(any(unix, test))]
fn shell_of(shell: &str, mac: bool) -> Shell {
    match Path::new(shell).file_name().and_then(|name| name.to_str()).unwrap_or("") {
        "zsh" => Shell::Zsh,
        "bash" => Shell::Bash,
        "fish" => Shell::Fish,
        "" if mac => Shell::Zsh,
        "" => Shell::Bash,
        _ => Shell::Other,
    }
}

/// The startup files that matter for a terminal opened now: the one SoftPresenter writes its
/// line to (none for a shell it doesn't know), every one the shell reads (to see whether the
/// folder is on the PATH already), and the line.
#[cfg(unix)]
struct ShellFiles {
    write: Option<PathBuf>,
    read: Vec<PathBuf>,
    line: &'static str,
}

/// A Mac's Terminal starts each window as a login shell: zsh then reads `.zshenv`, `.zprofile`,
/// `.zshrc` and `.zlogin`; bash reads the first of `.bash_profile`, `.bash_login` and `.profile`
/// that is there (and not `.bashrc`, unless that one reads it), so the line goes in that one,
/// and `.bash_profile` is made only when none is there (making it beside a `.profile` would stop
/// bash reading the `.profile`). Linux's terminals start shells that aren't login shells: zsh
/// reads `.zshenv` and `.zshrc`, bash reads `.bashrc` (a line in `.profile` counts only from the
/// next sign-in, which is why it isn't looked at). Fish reads `config.fish` and `conf.d`.
#[cfg(unix)]
fn shell_files(place: &Place) -> ShellFiles {
    let home = &place.home;
    match shell_of(&place.shell, place.mac) {
        Shell::Zsh => {
            let dir = place.zdotdir.clone().unwrap_or_else(|| home.clone());
            let names: &[&str] =
                if place.mac { &[".zshenv", ".zprofile", ".zshrc", ".zlogin"] } else { &[".zshenv", ".zshrc"] };
            ShellFiles {
                write: Some(dir.join(".zshrc")),
                read: names.iter().map(|name| dir.join(name)).collect(),
                line: PATH_LINES[0],
            }
        }
        Shell::Bash if place.mac => {
            let login = [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .map(|name| home.join(name))
                .find(|file| file.exists())
                .unwrap_or_else(|| home.join(".bash_profile"));
            let mut read = vec![login.clone()];
            if std::fs::read(&login).is_ok_and(|text| String::from_utf8_lossy(&text).contains(".bashrc")) {
                read.push(home.join(".bashrc"));
            }
            ShellFiles { write: Some(login), read, line: PATH_LINES[0] }
        }
        Shell::Bash => {
            ShellFiles { write: Some(home.join(".bashrc")), read: vec![home.join(".bashrc")], line: PATH_LINES[0] }
        }
        Shell::Fish => {
            let dir = place.config_home.clone().unwrap_or_else(|| home.join(".config")).join("fish");
            let mut read = vec![dir.join("config.fish")];
            if let Ok(entries) = std::fs::read_dir(dir.join("conf.d")) {
                read.extend(
                    entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| path.extension().is_some_and(|e| e == "fish")),
                );
            }
            ShellFiles { write: Some(dir.join("config.fish")), read, line: PATH_LINES[1] }
        }
        Shell::Other => ShellFiles { write: None, read: Vec::new(), line: PATH_LINES[0] },
    }
}

/// Every startup file SoftPresenter may have written its line to, whichever shell is used now.
#[cfg(unix)]
fn every_shell_file(place: &Place) -> Vec<PathBuf> {
    let home = &place.home;
    let mut files: Vec<PathBuf> =
        [".zshrc", ".bashrc", ".bash_profile", ".bash_login", ".profile"].iter().map(|name| home.join(name)).collect();
    if let Some(dir) = &place.zdotdir {
        files.push(dir.join(".zshrc"));
    }
    files.push(place.config_home.clone().unwrap_or_else(|| home.join(".config")).join("fish/config.fish"));
    let mut seen = Vec::new();
    files.retain(|file| {
        let new = !seen.contains(file);
        seen.push(file.clone());
        new
    });
    files
}

/// Whether a startup file's text puts `folder` on the PATH: a line, not a comment, that
/// mentions the PATH and the folder, however the home folder is written in it (`$HOME`,
/// `${HOME}`, `"$HOME"`, `~` or in full).
#[cfg(any(unix, test))]
fn puts_on_path(text: &str, folder: &Path, home: &Path) -> bool {
    let wanted = match folder.strip_prefix(home) {
        Ok(rest) => format!("$HOME/{}", rest.display()),
        Err(_) => folder.display().to_string(),
    };
    let home = home.display().to_string();
    text.lines().map(str::trim).filter(|line| !line.starts_with('#')).any(|line| {
        if !line.to_ascii_lowercase().contains("path") {
            return false;
        }
        let mut line = line.replace("${HOME}", "$HOME").replace("\"$HOME\"", "$HOME").replace("~/", "$HOME/");
        if home.len() > 1 {
            line = line.replace(&home, "$HOME");
        }
        line.match_indices(&wanted).any(|(at, _)| {
            let after = &line[at + wanted.len()..];
            let after = after.strip_prefix('/').unwrap_or(after);
            after.chars().next().is_none_or(|c| c.is_whitespace() || matches!(c, ':' | '"' | '\'' | ')' | ';' | '}'))
        })
    })
}

/// Adds SoftPresenter's marked line at the end of a startup file (made if it isn't there),
/// after an empty line. Added to, not written anew, so a file that is a link stays one.
#[cfg(unix)]
fn add_line(file: &Path, line: &str) -> Result<(), String> {
    use std::io::Write;
    let not_written = |e: std::io::Error| {
        tf(
            "settings.cli.not_written",
            "Could not change {path}: {error}",
            &[("path", &shown(file)), ("error", &e.to_string())],
        )
    };
    let before = match std::fs::read(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(not_written(e)),
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(not_written)?;
    }
    let mut out = std::fs::OpenOptions::new().create(true).append(true).open(file).map_err(not_written)?;
    out.write_all(with_line_after(&before, line).as_bytes()).map_err(not_written)
}

/// What [`add_line`] adds to a file that holds `before`.
#[cfg(any(unix, test))]
fn with_line_after(before: &[u8], line: &str) -> String {
    let gap = match before.last() {
        None => "",
        Some(b'\n') => "\n",
        Some(_) => "\n\n",
    };
    format!("{gap}{MARK}\n{line}\n")
}

/// The text without SoftPresenter's marked lines (and the empty line before each), or `None`
/// when it has none. A mark whose next line was changed by hand is left, with that line.
#[cfg(any(unix, test))]
fn without_line(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut at = 0;
    let mut found = false;
    while at < lines.len() {
        let ours =
            lines[at].trim_end() == MARK && lines.get(at + 1).is_some_and(|next| PATH_LINES.contains(&next.trim_end()));
        if ours {
            if kept.last().is_some_and(|last| last.trim().is_empty()) {
                kept.pop();
            }
            found = true;
            at += 2;
        } else {
            kept.push(lines[at]);
            at += 1;
        }
    }
    found.then(|| kept.concat())
}

// ---------------------------------------------------------------------------------------------
// Windows: batch files, and the folder on the user's PATH.

/// The folder of the user's own the batch files go in.
#[cfg(windows)]
fn windows_folder() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|folder| !folder.is_empty())
        .map(|local| PathBuf::from(local).join("SoftPresenter").join("bin"))
        .ok_or_else(|| {
            t("settings.cli.no_local", "Could not find the folder for this user\u{2019}s programs (LOCALAPPDATA).")
                .to_owned()
        })
}

#[cfg(windows)]
fn windows_status() -> Status {
    let Ok(folder) = windows_folder() else { return Status::Missing };
    let link = folder.join("sp.cmd");
    let Some(program) = std::fs::read_to_string(&link).ok().and_then(|text| program_in_command_file(&text)) else {
        return Status::Missing;
    };
    let here = std::env::current_exe().is_ok_and(|exe| same_windows_path(&exe, &program));
    if here {
        Status::Here { findable: sp_winapi::on_user_path(&folder.to_string_lossy()), link }
    } else if program.exists() {
        Status::Elsewhere { link, program }
    } else {
        Status::Gone { link }
    }
}

/// `sp.cmd` and `softpresenter.cmd` in the user's own folder (`%LOCALAPPDATA%\SoftPresenter\bin`),
/// each starting this program with what was typed after it, and that folder put on the user's
/// PATH. Pressing Install again points them at this program, wherever it now is.
#[cfg(windows)]
fn windows_install() -> Result<Installed, String> {
    let program = std::env::current_exe().map_err(|e| {
        tf("settings.cli.no_program", "Could not find this program: {error}", &[("error", &e.to_string())])
    })?;
    let folder = windows_folder()?;
    std::fs::create_dir_all(&folder).map_err(|e| {
        tf(
            "settings.cli.not_made",
            "Could not make {path}: {error}",
            &[("path", &shown(&folder)), ("error", &e.to_string())],
        )
    })?;
    let mut done = Installed { link: folder.join("sp.cmd"), ..Installed::default() };
    let wanted = command_file(&program);
    for name in ["sp.cmd", "softpresenter.cmd"] {
        let file = folder.join(name);
        if std::fs::read_to_string(&file).is_ok_and(|text| text == wanted) {
            continue;
        }
        std::fs::write(&file, &wanted).map_err(|e| {
            tf(
                "settings.cli.not_made",
                "Could not make {path}: {error}",
                &[("path", &shown(&file)), ("error", &e.to_string())],
            )
        })?;
        done.changed = true;
    }
    if sp_winapi::add_to_user_path(&folder.to_string_lossy()).map_err(|e| {
        tf(
            "settings.cli.path_not_set",
            "Made the sp command in {folder}, but {error}.",
            &[("folder", &shown(&folder)), ("error", &e)],
        )
    })? {
        done.changed = true;
    }
    Ok(done)
}

#[cfg(windows)]
fn windows_remove() -> Result<Removed, String> {
    let folder = windows_folder()?;
    let mut done = Removed::default();
    for name in ["sp.cmd", "softpresenter.cmd"] {
        let file = folder.join(name);
        if file.exists() {
            std::fs::remove_file(&file).map_err(|e| {
                tf(
                    "settings.cli.not_removed",
                    "Could not remove {path}: {error}",
                    &[("path", &shown(&file)), ("error", &e.to_string())],
                )
            })?;
            done.commands += 1;
        }
    }
    // The folder goes too when nothing else is in it.
    let _ = std::fs::remove_dir(&folder);
    done.path_removed = sp_winapi::remove_from_user_path(&folder.to_string_lossy())
        .map_err(|e| tf("settings.cli.path_not_removed", "Removed the sp command, but {error}.", &[("error", &e)]))?;
    Ok(done)
}

/// Two Windows paths name the same file: Windows doesn't mind the case.
#[cfg(any(windows, test))]
fn same_windows_path(a: &Path, b: &Path) -> bool {
    let plain = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
    plain(a) == plain(b)
}

/// The batch file behind `sp` on Windows: a command goes to the program and its answer and exit
/// code come back; `sp` alone opens the app (through `start`, so the terminal isn't held until
/// it quits). A batch file reads `%` as the start of a variable, so the path's are doubled.
#[cfg(any(windows, test))]
fn command_file(program: &Path) -> String {
    let program = program.display().to_string().replace('%', "%%");
    format!(
        "@echo off\r\nif \"%~1\"==\"\" goto open\r\n\"{program}\" %*\r\nexit /b %errorlevel%\r\n:open\r\nstart \"\" \"{program}\"\r\n"
    )
}

/// The program a batch file made by [`command_file`] starts.
#[cfg(any(windows, test))]
fn program_in_command_file(text: &str) -> Option<PathBuf> {
    text.lines()
        .find_map(|line| line.strip_prefix('"')?.strip_suffix("\" %*"))
        .map(|program| PathBuf::from(program.replace("%%", "%")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_sp_command_passes_commands_on_and_opens_the_app_alone() {
        let file = command_file(Path::new(r"C:\Users\a\100% Church\SoftPresenter.exe"));
        assert!(file.starts_with("@echo off\r\n"), "{file}");
        // A command, with its answer's exit code; the % in the path is doubled for the batch file.
        assert!(
            file.contains("\"C:\\Users\\a\\100%% Church\\SoftPresenter.exe\" %*\r\nexit /b %errorlevel%"),
            "{file}"
        );
        // Alone: opens the app without holding the terminal.
        assert!(file.contains("if \"%~1\"==\"\" goto open"), "{file}");
        assert!(file.ends_with(":open\r\nstart \"\" \"C:\\Users\\a\\100%% Church\\SoftPresenter.exe\"\r\n"), "{file}");
    }

    #[test]
    fn the_program_is_read_back_from_the_windows_sp_command() {
        let program = Path::new(r"C:\Users\a\100% Church\SoftPresenter.exe");
        assert_eq!(program_in_command_file(&command_file(program)).as_deref(), Some(program));
        assert_eq!(program_in_command_file("@echo off\r\necho hello\r\n"), None);
        assert!(same_windows_path(
            Path::new(r"C:\Program Files\SoftPresenter.exe"),
            Path::new(r"c:/program files/softpresenter.EXE")
        ));
        assert!(!same_windows_path(Path::new(r"C:\Old\SoftPresenter.exe"), Path::new(r"C:\New\SoftPresenter.exe")));
    }

    #[test]
    fn a_folder_already_on_the_path_is_found_however_home_is_written() {
        let home = Path::new("/Users/a");
        let folder = Path::new("/Users/a/.local/bin");
        for line in [
            "export PATH=\"$HOME/.local/bin:$PATH\"",
            "export PATH=$HOME/.local/bin:$PATH",
            "export PATH=\"${HOME}/.local/bin:$PATH\"",
            "export PATH=~/.local/bin:$PATH",
            "PATH=\"$PATH:/Users/a/.local/bin\"",
            "export PATH=\"$PATH:$HOME/.local/bin/\"",
            "  path=($HOME/.local/bin $path)",
            "fish_add_path ~/.local/bin",
            "set -gx PATH \"$HOME\"/.local/bin $PATH",
            // Fedora's own .bashrc.
            "    PATH=\"$HOME/.local/bin:$HOME/bin:$PATH\"",
        ] {
            assert!(puts_on_path(&format!("alias ll='ls -l'\n{line}\n"), folder, home), "{line}");
        }
        for line in [
            // A comment, a test for the folder, another folder, a program in it.
            "# export PATH=\"$HOME/.local/bin:$PATH\"",
            "if [ -d \"$HOME/.local/bin\" ] ; then",
            "export PATH=\"$HOME/.local/bin2:$PATH\"",
            "export PATH=\"$HOME/.local/binaries:$PATH\"",
            ". \"$HOME/.local/bin/env\"",
            "export PATH=\"/Users/ab/.local/bin:$PATH\"",
        ] {
            assert!(!puts_on_path(line, folder, home), "{line}");
        }
        // SoftPresenter's own line counts, so it is never added twice.
        assert!(puts_on_path(&with_line_after(b"", PATH_LINES[0]), folder, home));
        assert!(puts_on_path(&with_line_after(b"", PATH_LINES[1]), folder, home));
    }

    #[test]
    fn the_shell_is_known_by_its_name() {
        assert_eq!(shell_of("/bin/zsh", true), Shell::Zsh);
        assert_eq!(shell_of("/usr/local/bin/bash", true), Shell::Bash);
        assert_eq!(shell_of("/usr/bin/fish", false), Shell::Fish);
        assert_eq!(shell_of("/bin/tcsh", true), Shell::Other);
        // None named: the system's own.
        assert_eq!(shell_of("", true), Shell::Zsh);
        assert_eq!(shell_of("", false), Shell::Bash);
    }

    #[test]
    fn the_marked_line_comes_out_as_it_went_in() {
        let before = "alias ll='ls -l'\nexport EDITOR=vi\n";
        let after = format!("{before}{}", with_line_after(before.as_bytes(), PATH_LINES[0]));
        assert_eq!(after, format!("{before}\n{MARK}\n{}\n", PATH_LINES[0]));
        assert_eq!(without_line(&after).as_deref(), Some(before));
        // A file without a last line ending, and an empty one.
        assert_eq!(with_line_after(b"export X=1", PATH_LINES[0]), format!("\n\n{MARK}\n{}\n", PATH_LINES[0]));
        assert_eq!(with_line_after(b"", PATH_LINES[1]), format!("{MARK}\n{}\n", PATH_LINES[1]));
        assert_eq!(without_line(&with_line_after(b"", PATH_LINES[1])).as_deref(), Some(""));
        // Nothing of SoftPresenter's: nothing to do; a mark whose line was changed by hand stays.
        assert_eq!(without_line(before), None);
        let changed = format!("{MARK}\nexport PATH=\"$HOME/.local/bin:/opt/x:$PATH\"\n");
        assert_eq!(without_line(&changed), None);
    }

    #[test]
    fn the_installed_place_is_told_from_one_the_app_passes_through() {
        let mac = |program: &str| installed_place(program, "/Users/a", "/private/var/folders/x/T", true, false);
        assert!(mac("/Applications/SoftPresenter.app/Contents/MacOS/softpresenter"));
        assert!(mac("/Users/a/Applications/SoftPresenter.app/Contents/MacOS/softpresenter"));
        assert!(!mac("/Volumes/SoftPresenter/SoftPresenter.app/Contents/MacOS/softpresenter"));
        assert!(!mac("/Users/a/Downloads/SoftPresenter.app/Contents/MacOS/softpresenter"));
        assert!(!mac("/Users/a/dev/presentercraft/target/debug/softpresenter"));
        assert!(!mac("/private/var/folders/x/T/AppTranslocation/1/d/SoftPresenter.app/Contents/MacOS/softpresenter"));
        let linux = |program: &str| installed_place(program, "/home/a", "/tmp", false, false);
        assert!(linux("/usr/bin/softpresenter"));
        assert!(linux("/home/a/Apps/SoftPresenter/softpresenter"));
        assert!(!linux("/tmp/.mount_SoftPrXyz/usr/bin/softpresenter"));
        assert!(!linux("/media/a/USB/softpresenter"));
        assert!(!linux("/home/a/src/presentercraft/target/release/softpresenter"));
        assert!(!linux("/home/a/src/presentercraft/target/x86_64-unknown-linux-gnu/debug/softpresenter"));
        let windows = |program: &str| installed_place(program, "", r"C:\Users\a\AppData\Local\Temp", false, true);
        assert!(windows(r"C:\Users\a\SoftPresenter\SoftPresenter.exe"));
        assert!(windows(r"C:\Program Files\SoftPresenter\SoftPresenter.exe"));
        assert!(!windows(r"c:\users\A\appdata\local\temp\Temp1_SoftPresenter.zip\SoftPresenter.exe"));
        assert!(!windows(r"C:\dev\presentercraft\target\x86_64-pc-windows-gnu\release\SoftPresenter.exe"));
    }

    #[test]
    fn the_start_up_install_never_runs_in_a_test_copy_and_never_takes_another_copys_sp() {
        let program = Path::new("/Applications/SoftPresenter.app/Contents/MacOS/softpresenter");
        let link = PathBuf::from("/Users/a/.local/bin/sp");
        let missing = || Status::Missing;
        let unasked = || -> Status { panic!("the status needn't be read") };
        // A test copy, or a place the app only passes through: nothing, and nothing read.
        assert_eq!(decide(true, true, None, program, unasked), AtStart::Leave);
        assert_eq!(decide(false, false, None, program, unasked), AtStart::Leave);
        // Done before for this program, or removed by the person: nothing.
        let here = format!("{}\n", program.display());
        assert_eq!(decide(false, true, Some(&here), program, unasked), AtStart::Leave);
        assert_eq!(decide(false, true, Some("removed\n"), program, unasked), AtStart::Leave);
        // Never installed, done for a copy that has moved, or pointing at nothing: installed.
        assert_eq!(decide(false, true, None, program, missing), AtStart::Install);
        assert_eq!(decide(false, true, Some("/Users/a/Downloads/Old.app/x\n"), program, missing), AtStart::Install);
        assert_eq!(decide(false, true, None, program, || Status::Gone { link: link.clone() }), AtStart::Install);
        // Already this program's: remembered; this program's but not found by a terminal: set up.
        let found = || Status::Here { link: link.clone(), findable: true };
        assert_eq!(decide(false, true, None, program, found), AtStart::Remember);
        let unfound = || Status::Here { link: link.clone(), findable: false };
        assert_eq!(decide(false, true, None, program, unfound), AtStart::Install);
        // Another copy that is still there keeps it.
        let other =
            || Status::Elsewhere { link: link.clone(), program: PathBuf::from("/Users/a/Old/SoftPresenter.app") };
        assert_eq!(decide(false, true, None, program, other), AtStart::Leave);
    }

    #[test]
    fn the_words_say_where_and_what_to_type() {
        let done = Installed {
            link: PathBuf::from("/opt/x/.local/bin/sp"),
            shell_file: Some(PathBuf::from("/opt/x/.zshrc")),
            add_folder: false,
            changed: true,
        };
        assert_eq!(
            installed_words(&done, false),
            "Installed in /opt/x/.local/bin, and SoftPresenter added a line for it to /opt/x/.zshrc. Open a new terminal window and type: sp status"
        );
        assert_eq!(
            ready_words(&done, false),
            "The sp command is ready for the terminal (/opt/x/.local/bin/sp). SoftPresenter added a line for it to /opt/x/.zshrc. Settings \u{203a} Command line can remove it."
        );
        let already = Installed { shell_file: None, ..done.clone() };
        assert_eq!(
            installed_words(&already, false),
            "Installed in /opt/x/.local/bin. Open a new terminal window and type: sp status"
        );
        let unknown = Installed { shell_file: None, add_folder: true, ..done.clone() };
        assert_eq!(
            installed_words(&unknown, false),
            "Installed in /opt/x/.local/bin, but a terminal doesn\u{2019}t look there yet: add /opt/x/.local/bin to your PATH, then type: sp status"
        );
        // Windows: the Command Prompt, and softpresenter in PowerShell, where sp means something
        // else.
        let windows = Installed {
            link: PathBuf::from("C:/Users/a/AppData/Local/SoftPresenter/bin/sp.cmd"),
            changed: true,
            ..Installed::default()
        };
        assert_eq!(
            installed_words(&windows, true),
            "Installed in C:/Users/a/AppData/Local/SoftPresenter/bin, which is on your PATH. Open a new Command Prompt and type: sp status (in PowerShell: softpresenter status)"
        );
        assert_eq!(
            ready_words(&windows, true),
            "The sp command is ready for the terminal (C:/Users/a/AppData/Local/SoftPresenter/bin/sp.cmd), and its folder is on your PATH. In PowerShell, type softpresenter instead. Settings \u{203a} Command line can remove it."
        );
        let here = Status::Here { link: windows.link.clone(), findable: true };
        assert_eq!(
            status_words(&here, true),
            (
                Tone::Good,
                "sp is installed (C:/Users/a/AppData/Local/SoftPresenter/bin/sp.cmd). In a Command Prompt, type: sp status (in PowerShell: softpresenter status)".to_owned()
            )
        );
        let here = Status::Here { link: done.link.clone(), findable: true };
        assert_eq!(
            status_words(&here, false),
            (Tone::Good, "sp is installed (/opt/x/.local/bin/sp). In a terminal, type: sp status".to_owned())
        );
        // In Spanish, with the places filled in.
        let spanish = crate::i18n::with_language(crate::i18n::Language::Spanish, || installed_words(&done, false));
        assert_eq!(
            spanish,
            "Instalado en /opt/x/.local/bin, y SoftPresenter ha añadido una línea para él en /opt/x/.zshrc. Abre una ventana nueva del terminal y escribe: sp status"
        );
        let removed = Removed { commands: 2, shell_files: vec![PathBuf::from("/opt/x/.zshrc")], path_removed: false };
        assert_eq!(
            removed_words(&removed),
            "Removed sp and softpresenter, and the line SoftPresenter added to /opt/x/.zshrc."
        );
        let windows = Removed { commands: 2, shell_files: Vec::new(), path_removed: true };
        assert_eq!(removed_words(&windows), "Removed sp and softpresenter, and took their folder off your PATH.");
        assert_eq!(removed_words(&Removed::default()), "There was nothing to remove.");
    }

    /// A home folder of its own for a test, with a program called softpresenter in it.
    #[cfg(unix)]
    fn scratch(name: &str, shell: &str, mac: bool) -> Place {
        let base = std::env::temp_dir().join(format!("sp-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home")).expect("a home folder");
        std::fs::create_dir_all(base.join("app")).expect("an app folder");
        std::fs::write(base.join("app/softpresenter"), "").expect("a program");
        let base = std::fs::canonicalize(&base).expect("the folder");
        let home = base.join("home");
        Place {
            program: base.join("app/softpresenter"),
            // Only the home folder's own: nothing outside the test's folder is looked at.
            folders: vec![home.join(".local/bin")],
            path: Some("/usr/bin:/bin".into()),
            shell: shell.to_owned(),
            mac,
            zdotdir: None,
            config_home: None,
            home,
        }
    }

    #[cfg(unix)]
    fn marks(file: &Path) -> usize {
        std::fs::read_to_string(file).unwrap_or_default().matches(MARK).count()
    }

    #[cfg(unix)]
    #[test]
    fn installing_twice_makes_one_line_and_remove_takes_it_all_away() {
        let place = scratch("zsh", "/bin/zsh", true);
        let zshrc = place.home.join(".zshrc");
        std::fs::write(&zshrc, "alias ll='ls -l'\n").expect("a .zshrc");
        assert_eq!(status_in(&place), Status::Missing);
        // ~/.local/bin is made, the links put in, and the line added.
        let first = install_in(&place).expect("installed");
        let link = place.home.join(".local/bin/sp");
        assert_eq!(first.link, link);
        assert_eq!(first.shell_file.as_deref(), Some(zshrc.as_path()));
        assert!(first.changed);
        assert_eq!(std::fs::canonicalize(&link).expect("sp"), place.program);
        assert_eq!(
            std::fs::canonicalize(place.home.join(".local/bin/softpresenter")).expect("softpresenter"),
            place.program
        );
        assert_eq!(status_in(&place), Status::Here { link: link.clone(), findable: true });
        // Again: nothing changes.
        let second = install_in(&place).expect("installed again");
        assert_eq!(second, Installed { link: link.clone(), ..Installed::default() });
        assert_eq!(marks(&zshrc), 1);
        // Remove: both links and the line, and the file as it was.
        let removed = remove_in(&place).expect("removed");
        assert_eq!(removed, Removed { commands: 2, shell_files: vec![zshrc.clone()], path_removed: false });
        assert_eq!(std::fs::read_to_string(&zshrc).expect("the .zshrc"), "alias ll='ls -l'\n");
        assert_eq!(status_in(&place), Status::Missing);
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
    }

    #[cfg(unix)]
    #[test]
    fn a_startup_file_that_already_has_the_folder_is_left_alone() {
        for (shell, mac, file, line) in [
            ("/bin/zsh", true, ".zprofile", "export PATH=~/.local/bin:$PATH"),
            ("/bin/bash", false, ".bashrc", "PATH=\"${HOME}/.local/bin:$PATH\""),
            ("/bin/bash", true, ".bash_profile", "export PATH=\"$HOME/.local/bin:$PATH\""),
        ] {
            let place = scratch(&format!("has-{}", file.trim_start_matches('.')), shell, mac);
            std::fs::write(place.home.join(file), format!("{line}\n")).expect("a startup file");
            let done = install_in(&place).expect("installed");
            assert_eq!(done.shell_file, None, "{shell} {file}");
            assert!(!done.add_folder);
            assert_eq!(std::fs::read_to_string(place.home.join(file)).expect("the file"), format!("{line}\n"));
            let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn each_shell_gets_the_file_a_new_terminal_reads() {
        // Linux's bash: .bashrc, even though .profile adds the folder at the next sign-in.
        let place = scratch("bash-linux", "/bin/bash", false);
        std::fs::write(
            place.home.join(".profile"),
            "if [ -d \"$HOME/.local/bin\" ] ; then\n    PATH=\"$HOME/.local/bin:$PATH\"\nfi\n",
        )
        .expect("a .profile");
        assert_eq!(install_in(&place).expect("installed").shell_file, Some(place.home.join(".bashrc")));
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // A Mac's bash: the login file that is there (.profile here), never a new .bash_profile
        // beside it.
        let place = scratch("bash-mac", "/bin/bash", true);
        std::fs::write(place.home.join(".profile"), "export EDITOR=vi\n").expect("a .profile");
        assert_eq!(install_in(&place).expect("installed").shell_file, Some(place.home.join(".profile")));
        assert!(!place.home.join(".bash_profile").exists());
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // ... and .bash_profile, made, when there is none.
        let place = scratch("bash-mac-none", "/bin/bash", true);
        assert_eq!(install_in(&place).expect("installed").shell_file, Some(place.home.join(".bash_profile")));
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // Fish: its config.fish, made with its folder, with fish's own words; once.
        let place = scratch("fish", "/usr/local/bin/fish", true);
        let config = place.home.join(".config/fish/config.fish");
        assert_eq!(install_in(&place).expect("installed").shell_file, Some(config.clone()));
        assert_eq!(std::fs::read_to_string(&config).expect("config.fish"), format!("{MARK}\n{}\n", PATH_LINES[1]));
        assert_eq!(install_in(&place).expect("installed again").shell_file, None);
        assert_eq!(remove_in(&place).expect("removed").shell_files, vec![config.clone()]);
        assert_eq!(std::fs::read_to_string(&config).expect("config.fish"), "");
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // A shell SoftPresenter doesn't know: nothing written, and the person told what to add.
        let place = scratch("tcsh", "/bin/tcsh", true);
        let done = install_in(&place).expect("installed");
        assert!(done.add_folder && done.shell_file.is_none());
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // The folder already on this program's PATH: nothing written.
        let mut place = scratch("on-path", "/bin/zsh", true);
        place.path =
            Some(std::env::join_paths([place.home.join(".local/bin"), PathBuf::from("/usr/bin")]).expect("a PATH"));
        assert_eq!(install_in(&place).expect("installed").shell_file, None);
        assert!(!place.home.join(".zshrc").exists());
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
    }

    #[cfg(unix)]
    #[test]
    fn another_copys_sp_is_pointed_here_and_another_program_is_left_alone() {
        let place = scratch("other-copy", "/bin/zsh", true);
        let bin = place.home.join(".local/bin");
        std::fs::create_dir_all(&bin).expect("the folder");
        let old = place.home.parent().expect("the test's folder").join("Old.app/Contents/MacOS/softpresenter");
        std::fs::create_dir_all(old.parent().expect("its folder")).expect("an old copy's folder");
        std::fs::write(&old, "").expect("an old copy");
        std::os::unix::fs::symlink(&old, bin.join("sp")).expect("a link to it");
        let old = std::fs::canonicalize(&old).expect("the old copy");
        assert_eq!(status_in(&place), Status::Elsewhere { link: bin.join("sp"), program: old.clone() });
        // Moved away: the link points at nothing.
        std::fs::remove_file(&old).expect("moved");
        assert_eq!(status_in(&place), Status::Gone { link: bin.join("sp") });
        install_in(&place).expect("pointed here");
        assert_eq!(std::fs::canonicalize(bin.join("sp")).expect("sp"), place.program);
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
        // A program of someone else's called sp: never replaced, nor removed.
        let place = scratch("taken", "/bin/zsh", true);
        let bin = place.home.join(".local/bin");
        std::fs::create_dir_all(&bin).expect("the folder");
        std::fs::write(bin.join("sp"), "#!/bin/sh\n").expect("another sp");
        assert_eq!(status_in(&place), Status::Missing);
        assert!(
            install_in(&place).expect_err("refused").ends_with("is another program, so SoftPresenter left it alone.")
        );
        assert_eq!(remove_in(&place).expect("removed").commands, 0);
        assert_eq!(std::fs::read_to_string(bin.join("sp")).expect("still there"), "#!/bin/sh\n");
        let _ = std::fs::remove_dir_all(place.home.parent().expect("the test's folder"));
    }
}
