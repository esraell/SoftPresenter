//! Presentation files from other programs (PowerPoint, Keynote, OpenDocument) turned into a PDF
//! by a program on this Mac that can open them, so their slides come in as pictures that look
//! exactly as that program draws them (the "As Pictures" way of bringing a presentation in; the
//! other way reads a `.pptx` directly, see [`crate::pptx`]). Which programs are tried, in order:
//!
//! - PowerPoint files: Keynote (which opens them), then PowerPoint, then LibreOffice.
//! - Keynote files: Keynote.
//! - OpenDocument presentations: LibreOffice, then PowerPoint.
//!
//! PowerPoint and Keynote are asked through AppleScript, which macOS lets SoftPresenter do once
//! the person allows it (it asks the first time). They are started with `launch` (hidden, with
//! no window and not brought to the front) and never `activate`d; the file is opened and
//! exported without a window being shown where the program allows it. LibreOffice is run
//! headless. A program SoftPresenter started is quit again afterwards; one that was already
//! open is left alone. Each attempt gets two minutes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A program that can turn presentations into PDFs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    PowerPoint,
    Keynote,
    LibreOffice,
}

impl Program {
    pub fn name(self) -> &'static str {
        match self {
            Program::PowerPoint => "PowerPoint",
            Program::Keynote => "Keynote",
            Program::LibreOffice => "LibreOffice",
        }
    }

    /// Where the program is when it is installed.
    fn app(self) -> PathBuf {
        let name = match self {
            Program::PowerPoint => "Microsoft PowerPoint.app",
            Program::Keynote => "Keynote.app",
            Program::LibreOffice => "LibreOffice.app",
        };
        let system = Path::new("/Applications").join(name);
        if system.exists() {
            return system;
        }
        std::env::var_os("HOME").map_or(system.clone(), |home| PathBuf::from(home).join("Applications").join(name))
    }

    /// The name the program runs under, for asking whether it is running.
    fn process(self) -> &'static str {
        match self {
            Program::PowerPoint => "Microsoft PowerPoint",
            Program::Keynote => "Keynote",
            Program::LibreOffice => "soffice",
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
        if self != Program::LibreOffice {
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
        // Keynote first: it has been tried and works; PowerPoint is asked when Keynote isn't
        // there, until it has been tried.
        "pptx" | "ppt" | "pps" | "ppsx" => &[Program::Keynote, Program::PowerPoint, Program::LibreOffice],
        "key" => &[Program::Keynote],
        "odp" => &[Program::LibreOffice, Program::PowerPoint],
        _ => &[],
    }
}

/// The installed program that will turn this file into slides, if there is one.
pub fn program_for(path: &Path) -> Option<Program> {
    programs_for(path).iter().copied().find(|program| program.installed())
}

/// What to do when no program on this Mac can open the file, in plain words.
pub fn none_installed(path: &Path) -> String {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let names: Vec<&str> = programs_for(path).iter().map(|p| p.name()).collect();
    let needs = match names.as_slice() {
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        [] => "a presentation program".to_owned(),
    };
    let slides = if matches!(crate::deck::kind_of(path), "pptx") {
        " Bring it in as slides instead (it needs no program), or export it as a PDF and drop that here."
    } else {
        " Or export it as a PDF and drop that here."
    };
    format!(
        "To bring in {name} as pictures, SoftPresenter uses {needs}, and none is on this Mac (Keynote is free from the App Store).{slides}"
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
            Program::Keynote => by_keynote(&full, folder),
            Program::PowerPoint => by_powerpoint(&full),
            Program::LibreOffice => by_libreoffice(&full, folder),
        };
        match made {
            Ok(pdf) if pdf.is_file() => return Ok((pdf, program)),
            Ok(_) => problems.push(format!("{} made no PDF", program.name())),
            Err(problem) => problems.push(format!("{}: {problem}", program.name())),
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
        "{name} could not be turned into slides ({}). Export it as a PDF and drop that here.",
        problems.join("; ")
    ))
}

/// Keynote: open, export as a PDF (skipped slides left out, as in the show), close. The file
/// names go to the script as arguments, never into its text.
fn by_keynote(path: &Path, folder: &Path) -> Result<PathBuf, String> {
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

/// PowerPoint: open, save as a PDF, close. PowerPoint may only write inside its own folder
/// without asking the person, so the PDF is made there and read from there.
fn by_powerpoint(path: &Path) -> Result<PathBuf, String> {
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

/// LibreOffice, without a window.
fn by_libreoffice(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let soffice = Program::LibreOffice.app().join("Contents/MacOS/soffice");
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
        assert_eq!(programs_for(Path::new("a.PPTX")), &[Program::Keynote, Program::PowerPoint, Program::LibreOffice]);
        assert_eq!(programs_for(Path::new("a.key")), &[Program::Keynote]);
        assert_eq!(programs_for(Path::new("a.odp")), &[Program::LibreOffice, Program::PowerPoint]);
        assert!(programs_for(Path::new("a.pdf")).is_empty());
    }

    #[test]
    fn with_no_program_the_message_says_what_would_do() {
        let said = none_installed(Path::new("/x/Sermon.pptx"));
        assert!(
            said.contains("Sermon.pptx") && said.contains("Keynote, PowerPoint or LibreOffice") && said.contains("PDF")
        );
        assert!(said.contains("as slides instead") && !none_installed(Path::new("Talk.key")).contains("as slides"));
        assert!(none_installed(Path::new("Talk.key")).contains("uses Keynote,"));
    }
}
