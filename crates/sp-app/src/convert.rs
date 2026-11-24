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
//! attempt gets two minutes (five on Windows, where saving a long deck takes longer).
//!
//! On Windows the saving app is asked through Windows PowerShell instead (its own automation,
//! opened read-only and without a window, hidden slides left out, saved as a PDF and closed), and
//! the headless app is run the same way as on a Mac; their PDFs are drawn by Windows' own PDF
//! reader (`sp_video::pdf_draw`).
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
    #[cfg(target_os = "macos")]
    fn app(self) -> Option<PathBuf> {
        let name = match self {
            Program::ScriptSaves => "Microsoft PowerPoint.app",
            Program::ScriptExports => "Keynote.app",
            Program::Headless => "LibreOffice.app",
        };
        let system = Path::new("/Applications").join(name);
        if system.exists() {
            return Some(system);
        }
        let own = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications").join(name))?;
        own.exists().then_some(own)
    }

    /// Where the app's program is when it is installed: the saving one where Windows' own list
    /// of programs says, or where it installs by default; the headless one in Program Files.
    #[cfg(windows)]
    fn app(self) -> Option<PathBuf> {
        let program_files = |name: &str| {
            ["ProgramFiles", "ProgramFiles(x86)"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|root| PathBuf::from(root).join(name))
                .find(|place| place.is_file())
        };
        match self {
            Program::ScriptSaves => registered("POWERPNT.EXE")
                .or_else(|| program_files(r"Microsoft Office\root\Office16\POWERPNT.EXE"))
                .or_else(|| program_files(r"Microsoft Office\Office16\POWERPNT.EXE")),
            Program::Headless => program_files(r"LibreOffice\program\soffice.exe"),
            Program::ScriptExports => None,
        }
    }

    /// The headless app, where Linux packages put it; the others don't run there.
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    fn app(self) -> Option<PathBuf> {
        match self {
            Program::Headless => ["/usr/bin/soffice", "/usr/bin/libreoffice", "/usr/local/bin/soffice"]
                .iter()
                .map(PathBuf::from)
                .find(|place| place.is_file()),
            Program::ScriptSaves | Program::ScriptExports => None,
        }
    }

    /// The name the app runs under, for asking whether it is running.
    fn process(self) -> &'static str {
        match self {
            Program::ScriptSaves if cfg!(windows) => "POWERPNT.EXE",
            Program::ScriptSaves => "Microsoft PowerPoint",
            Program::ScriptExports => "Keynote",
            Program::Headless if cfg!(windows) => "soffice.bin",
            Program::Headless => "soffice",
        }
    }

    /// Whether the program is running now.
    #[cfg(not(windows))]
    fn running(self) -> bool {
        Command::new("/usr/bin/pgrep")
            .args(["-x", self.process()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    fn running(self) -> bool {
        let mut command = windows_tool("tasklist.exe");
        command.args(["/FI", &format!("IMAGENAME eq {}", self.process()), "/NH", "/FO", "CSV"]);
        command.output().is_ok_and(|out| {
            String::from_utf8_lossy(&out.stdout).to_lowercase().contains(&self.process().to_lowercase())
        })
    }

    /// Asks a program SoftPresenter started to quit, when its script didn't get to (it was
    /// stopped for taking too long, or failed part way).
    fn quit(self) {
        if self == Program::Headless {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let script = format!("tell application \"{}\" to quit", self.process());
            let _ = run_for(osascript(&script, &[]), Duration::from_secs(15));
        }
        #[cfg(windows)]
        {
            let mut command = windows_tool("taskkill.exe");
            command.args(["/IM", self.process()]);
            let _ = run_for(command, Duration::from_secs(15));
        }
    }

    /// Installed, on a system whose PDFs SoftPresenter can draw (not Linux yet).
    fn installed(self) -> bool {
        cfg!(any(target_os = "macos", windows)) && self.app().is_some()
    }
}

/// One of Windows' own programs, from its own folder, with no console window.
#[cfg(windows)]
fn windows_tool(name: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let mut command = Command::new(root.join("System32").join(name));
    command.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
    command
}

/// Where Windows' own list of installed programs ("App Paths") says a program is.
#[cfg(windows)]
fn registered(program: &str) -> Option<PathBuf> {
    let key = format!(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{program}");
    let mut command = windows_tool("reg.exe");
    command.args(["query", &key, "/ve"]);
    let out = command.output().ok().filter(|out| out.status.success())?;
    path_from_reg(&String::from_utf8_lossy(&out.stdout))
}

/// The path in what `reg query … /ve` prints: "(Default)    REG_SZ    C:\…\POWERPNT.EXE".
#[cfg(any(windows, test))]
fn path_from_reg(printed: &str) -> Option<PathBuf> {
    let line = printed.lines().find(|line| line.contains("REG_SZ") || line.contains("REG_EXPAND_SZ"))?;
    let (_, value) = line.split_once("REG_SZ").or_else(|| line.split_once("REG_EXPAND_SZ"))?;
    let value = value.trim().trim_matches('"');
    let path = PathBuf::from(value);
    (!value.is_empty() && (cfg!(test) || path.is_file())).then_some(path)
}

/// How long one program gets to make the PDF. Windows' saving app takes about a second and a
/// half a slide on an ordinary office PC (20 slides: 45 seconds, starting it included), so a long
/// deck gets longer there.
const TIME_ALLOWED: Duration = Duration::from_secs(if cfg!(windows) { 300 } else { 120 });

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
    let full = plain(&path.canonicalize().map_err(|e| format!("{name} could not be found ({e})."))?);
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make a folder to work in: {e}"))?;
    let mut problems = Vec::new();
    for program in programs_for(path).iter().copied().filter(|p| p.installed()) {
        let was_running = program.running();
        let made = match program {
            Program::ScriptExports => by_exporting(&full, folder),
            Program::ScriptSaves if cfg!(windows) => by_saving_on_windows(&full, folder),
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
///
/// What `open` gives back is never used: for a file it converts (a `.pptx`), that is not the
/// document it made, and exporting it made the app quit with an error. Instead the documents open
/// before are noted, and the one that appears after (the one named after the file, if several
/// do) is exported and closed. A document the person had open is never exported or closed.
fn by_exporting(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let pdf = folder.join("slides.pdf");
    let script = r#"on run argv
    set inFile to POSIX file (item 1 of argv)
    set outFile to POSIX file (item 2 of argv)
    set baseName to item 3 of argv
    set wasOpen to application "Keynote" is running
    if not wasOpen then launch application "Keynote"
    tell application "Keynote"
        set openBefore to id of every document
        open inFile
        set newId to missing value
        repeat 500 times
            set openNow to id of every document
            repeat with place from 1 to count of openNow
                set docId to item place of openNow
                if openBefore does not contain {docId} then
                    if newId is missing value or (name of document id docId) starts with baseName then set newId to docId
                end if
            end repeat
            if newId is not missing value then exit repeat
            delay 0.2
        end repeat
        if newId is missing value then error "The file didn't open (if it is open there already, close it and try again)."
        set theDocument to document id newId
        export theDocument to outFile as PDF
        close theDocument saving no
    end tell
    if not wasOpen then tell application "Keynote" to quit
end run"#;
    // The file's name without its ending, to tell its document from others that open at once.
    let stem = Path::new(path.file_stem().unwrap_or_default());
    run_script(script, &[path, &pdf, stem])?;
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

/// The saving app on Windows, asked through its own automation from Windows PowerShell: the file
/// opened read-only and without a window, its hidden slides left out (in that copy, never the
/// file), saved as a PDF in `folder`, closed. The app is quit again only if it wasn't running and
/// has nothing else open. The paths go to the script in environment variables, never its text.
#[cfg(windows)]
fn by_saving_on_windows(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
try {
  $wasRunning = [bool](Get-Process -Name POWERPNT -ErrorAction SilentlyContinue)
  $app = New-Object -ComObject PowerPoint.Application
  try {
    # Open(FileName, ReadOnly, Untitled, WithWindow): read-only, keeping its name, no window.
    $presentation = $app.Presentations.Open($env:SP_PPT_IN, -1, 0, 0)
    try {
      for ($at = $presentation.Slides.Count; $at -ge 1; $at--) {
        $slide = $presentation.Slides.Item($at)
        if ($slide.SlideShowTransition.Hidden -eq -1) { $slide.Delete() }
      }
      if ($presentation.Slides.Count -eq 0) { throw 'every slide in it is hidden' }
      # 32: save as PDF.
      $presentation.SaveAs($env:SP_PPT_OUT, 32)
    } finally {
      $presentation.Close()
    }
  } finally {
    if (-not $wasRunning -and $app.Presentations.Count -eq 0) { $app.Quit() }
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($app)
  }
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;
    let pdf = folder.join("slides.pdf");
    let mut command = sp_video::powershell(SCRIPT)?;
    command.env("SP_PPT_IN", path).env("SP_PPT_OUT", &pdf);
    run(command)?;
    Ok(pdf)
}

#[cfg(not(windows))]
fn by_saving_on_windows(_path: &Path, _folder: &Path) -> Result<PathBuf, String> {
    Err("it runs on Windows only".to_owned())
}

/// A path as other programs take it: on Windows without the `\\?\` that `canonicalize` puts in
/// front (a program asked by automation may not read that form).
fn plain(path: &Path) -> PathBuf {
    if cfg!(windows) {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && !rest.starts_with(r"UNC\")
        {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// The headless app, without a window.
fn by_headless(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let app = Program::Headless.app().ok_or("it isn't installed")?;
    let soffice = if cfg!(target_os = "macos") { app.join("Contents/MacOS/soffice") } else { app };
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(soffice);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
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
    fn windows_list_of_programs_gives_the_saving_apps_path() {
        let printed = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\POWERPNT.EXE\r\n    (Default)    REG_SZ    C:\\Program Files\\Microsoft Office\\Root\\Office16\\POWERPNT.EXE\r\n";
        assert_eq!(
            path_from_reg(printed),
            Some(PathBuf::from("C:\\Program Files\\Microsoft Office\\Root\\Office16\\POWERPNT.EXE"))
        );
        assert_eq!(path_from_reg("ERROR: The system was unable to find the specified registry key"), None);
        assert_eq!(path_from_reg("    (Default)    REG_SZ    \r\n"), None, "an empty value is no program");
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
