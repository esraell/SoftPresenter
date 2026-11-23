//! Presentation files from other programs turned into a PDF by an app on this Mac that can
//! open them, so their slides come in as pictures that look exactly as that app draws them (the
//! "As Pictures" way of bringing a presentation in; the other way reads a `.pptx` directly, see
//! [`crate::pptx`]). Which apps are tried, in order, depends on the kind of file: a `.pptx`
//! (or `.ppt`, `.pps`, `.ppsx`) goes to the apps that open those, a `.key` to the one that does,
//! an `.odp` to the ones that open OpenDocument.
//!
//! Two of the three are asked through AppleScript, which macOS lets SoftPresenter do once the
//! person allows it (it asks the first time). They are started with `launch` (hidden, with no
//! window and not brought to the front) and never `activate`d; the file is opened and exported
//! without a window being shown where the app allows it. The third is run headless. An app
//! SoftPresenter started is quit again afterwards; one that was already open is left alone. Each
//! attempt gets two minutes.
//!
//! What the person is told never names an app: the apps are named here, in code, only because
//! they have to be started by the names they are installed under.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// An app that can turn presentations into PDFs, by how it is asked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    /// Asked by script to open the file and save a PDF in its own folder.
    ScriptSaves,
    /// Asked by script to open the file and export a PDF where it is told.
    ScriptExports,
    /// Run without a window, to convert the file.
    Headless,
}

impl Program {
    /// Where the app is when it is installed.
    fn app(self) -> PathBuf {
        let name = match self {
            Program::ScriptSaves => "Microsoft PowerPoint.app",
            Program::ScriptExports => "Keynote.app",
            Program::Headless => "LibreOffice.app",
        };
        let system = Path::new("/Applications").join(name);
        if system.exists() {
            return system;
        }
        std::env::var_os("HOME").map_or(system.clone(), |home| PathBuf::from(home).join("Applications").join(name))
    }

    /// The name the app runs under, for asking whether it is running.
    fn process(self) -> &'static str {
        match self {
            Program::ScriptSaves => "Microsoft PowerPoint",
            Program::ScriptExports => "Keynote",
            Program::Headless => "soffice",
        }
    }

    /// Whether the program is running now.
    fn running(self) -> bool {
        Command::new("/usr/bin/pgrep")
            .args(["-x", self.process()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Asks a program SoftPresenter started to quit, when its script didn't get to (it was
    /// stopped for taking too long, or failed part way).
    fn quit(self) {
        if self != Program::Headless {
            let script = format!("tell application \"{}\" to quit", self.process());
            let _ = run_for(osascript(&script, &[]), Duration::from_secs(15));
        }
    }

    fn installed(self) -> bool {
        cfg!(target_os = "macos") && self.app().exists()
    }
}

/// How long one program gets to make the PDF.
const TIME_ALLOWED: Duration = Duration::from_secs(120);

/// The programs that can open a presentation file, best first.
pub fn programs_for(path: &Path) -> &'static [Program] {
    let ending = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ending.as_str() {
        // The exporting one first: it has been tried and works; the one that saves is asked when
        // it isn't there, until it has been tried.
        "pptx" | "ppt" | "pps" | "ppsx" => &[Program::ScriptExports, Program::ScriptSaves, Program::Headless],
        "key" => &[Program::ScriptExports],
        "odp" => &[Program::Headless, Program::ScriptSaves],
        _ => &[],
    }
}

/// The installed program that will turn this file into slides, if there is one.
pub fn program_for(path: &Path) -> Option<Program> {
    // Testing aid: SOFTPRESENTER_HIDE_PICTURE_APPS=1 behaves as on a computer where no app can
    // open a presentation to draw it (the greyed As Pictures card, and the sheet with neither).
    if std::env::var_os("SOFTPRESENTER_HIDE_PICTURE_APPS").is_some() {
        return None;
    }
    programs_for(path).iter().copied().find(|program| program.installed())
}

/// What to say when no app on this Mac can open the file to draw it, in plain words.
pub fn none_installed(path: &Path) -> String {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let ending = path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
    let instead = if matches!(crate::deck::kind_of(path), "pptx") {
        " Bring it in as slides instead (it needs no app), or export it as a PDF and drop that here."
    } else {
        " Saved as .pptx it could come in as slides, or export it as a PDF and drop that here."
    };
    format!(
        "No app on this computer can open {ending} files to draw them, so \u{201c}{name}\u{201d} can\u{2019}t come in as pictures.{instead}"
    )
}

/// Turns a presentation file into a PDF in `folder`, with the installed program best for it,
/// trying the next when one fails. Returns the PDF and the program that made it.
pub fn to_pdf(path: &Path, folder: &Path) -> Result<(PathBuf, Program), String> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let full = path.canonicalize().map_err(|e| format!("{name} could not be found ({e})."))?;
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make a folder to work in: {e}"))?;
    let mut problems = Vec::new();
    for program in programs_for(path).iter().copied().filter(|p| p.installed()) {
        let was_running = program.running();
        let made = match program {
            Program::ScriptExports => by_exporting(&full, folder),
            Program::ScriptSaves => by_saving(&full),
            Program::Headless => by_headless(&full, folder),
        };
        match made {
            Ok(pdf) if pdf.is_file() => return Ok((pdf, program)),
            Ok(_) => problems.push("it made no PDF".to_owned()),
            Err(problem) => problems.push(problem),
        }
        // A program this started stays out of the way: closed again if it didn't close itself.
        if !was_running && program.running() {
            program.quit();
        }
    }
    if problems.is_empty() {
        return Err(none_installed(path));
    }
    Err(format!(
        "{name} could not be drawn as pictures (the app that opened it said: {}). Export it as a PDF and drop that here.",
        problems.join("; ")
    ))
}

/// The exporting app: open, export as a PDF (skipped slides left out, as in the show), close. The file
/// names go to the script as arguments, never into its text.
fn by_exporting(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let pdf = folder.join("slides.pdf");
    let script = r#"on run argv
    set inFile to POSIX file (item 1 of argv)
    set outFile to POSIX file (item 2 of argv)
    set wasOpen to application "Keynote" is running
    if not wasOpen then launch application "Keynote"
    tell application "Keynote"
        set theDocument to open inFile
        export theDocument to outFile as PDF
        close theDocument saving no
    end tell
    if not wasOpen then tell application "Keynote" to quit
end run"#;
    run_script(script, &[path, &pdf])?;
    Ok(pdf)
}

/// The saving app: open, save as a PDF, close. It may only write inside its own folder
/// without asking the person, so the PDF is made there and read from there.
fn by_saving(path: &Path) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("no home folder")?;
    let folder =
        PathBuf::from(home).join("Library/Containers/com.microsoft.Powerpoint/Data/Library/Caches/SoftPresenter");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let pdf = folder.join(format!("{}.pdf", sp_model::Uuid::new_v4()));
    let script = r#"on run argv
    set inFile to POSIX file (item 1 of argv)
    set outFile to (item 2 of argv)
    set wasOpen to application "Microsoft PowerPoint" is running
    if not wasOpen then launch application "Microsoft PowerPoint"
    tell application "Microsoft PowerPoint"
        open inFile
        set thePresentation to active presentation
        save thePresentation in outFile as save as PDF
        close thePresentation saving no
    end tell
    if not wasOpen then tell application "Microsoft PowerPoint" to quit
end run"#;
    run_script(script, &[path, &pdf])?;
    Ok(pdf)
}

/// The headless app, without a window.
fn by_headless(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let soffice = Program::Headless.app().join("Contents/MacOS/soffice");
    let mut command = Command::new(soffice);
    command.args(["--headless", "--norestore", "--convert-to", "pdf", "--outdir"]).arg(folder).arg(path);
    run(command)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(folder.join(format!("{stem}.pdf")))
}

fn run_script(script: &str, paths: &[&Path]) -> Result<(), String> {
    run(osascript(script, paths))
}

fn osascript(script: &str, paths: &[&Path]) -> Command {
    let mut command = Command::new("/usr/bin/osascript");
    command.arg("-e").arg(script);
    for path in paths {
        command.arg(path);
    }
    command
}

/// Runs a program for at most [`TIME_ALLOWED`], and says in plain words why it failed.
fn run(command: Command) -> Result<(), String> {
    run_for(command, TIME_ALLOWED)
}

/// Runs a program for at most `allowed`, and says in plain words why it failed.
fn run_for(mut command: Command, allowed: Duration) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not be started ({e})"))?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut said = String::new();
        if let Some(stderr) = stderr.as_mut() {
            use std::io::Read;
            let _ = stderr.take(16 * 1024).read_to_string(&mut said);
        }
        said
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() > allowed => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it took too long".to_owned());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let said = reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    // macOS refuses with -1743 until the person allows SoftPresenter to control the program.
    if said.contains("-1743") || said.contains("Not authorized") {
        return Err(
            "SoftPresenter isn't allowed to use it yet. Allow it in System Settings › Privacy & Security › Automation"
                .to_owned(),
        );
    }
    let said = said.lines().last().unwrap_or("").trim().to_owned();
    Err(if said.is_empty() { format!("it stopped ({status})") } else { said })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_file_has_its_programs_best_first() {
        let all = &[Program::ScriptExports, Program::ScriptSaves, Program::Headless];
        assert_eq!(programs_for(Path::new("a.PPTX")), all);
        assert_eq!(programs_for(Path::new("a.key")), &[Program::ScriptExports]);
        assert_eq!(programs_for(Path::new("a.odp")), &[Program::Headless, Program::ScriptSaves]);
        assert!(programs_for(Path::new("a.pdf")).is_empty());
    }

    #[test]
    fn with_no_app_the_message_says_what_would_do_and_names_no_product() {
        let said = none_installed(Path::new("/x/Sermon.pptx"));
        assert!(said.contains("\u{201c}Sermon.pptx\u{201d}") && said.contains(".pptx files") && said.contains("PDF"));
        assert!(said.contains("as slides instead"));
        let key = none_installed(Path::new("Talk.key"));
        assert!(key.contains("open .key files to draw them") && key.contains("as .pptx") && key.contains("PDF"));
        for words in [said, key] {
            for product in ["Keynote", "PowerPoint", "LibreOffice", "Microsoft", "Office"] {
                assert!(!words.contains(product), "{words}");
            }
        }
    }
}
