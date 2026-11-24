//! PDF pages drawn on Windows, by Windows' own PDF reader (`Windows.Data.Pdf`, part of every
//! Windows 10 and 11), asked through Windows PowerShell, as the Mac asks its own frameworks. Each
//! page is drawn as a PNG in a folder of its own and read back, so nothing of another program's
//! runs inside SoftPresenter.
//!
//! The script is fixed text, passed as `-EncodedCommand` (so no quoting can change it), and every
//! value it uses (the PDF's path, the folder, the page sizes) goes to it in an environment
//! variable, never into its text. PowerShell is started from Windows' own folder, without a
//! console window.

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use crate::Frame;

/// The PowerShell script. `SP_PDF_MODE` is `sizes` (print "pages N", then each page's width and
/// height in device-independent pixels, a 96th of an inch) or `draw` (draw each page at the size
/// given in `SP_PDF_SIZES`, "w,h;w,h…", into `SP_PDF_OUT` as 1.png, 2.png…, printing "page N" as
/// each is done). A PDF locked with a password ends with "locked" on stderr and exit code 3.
#[cfg(any(windows, test))]
const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
try {
  Add-Type -AssemblyName System.Runtime.WindowsRuntime
  $methods = [System.WindowsRuntimeSystemExtensions].GetMethods()
  $asTask = $methods | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1
  $asTaskAction = $methods | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction' } | Select-Object -First 1
  function Await($operation, [Type]$type) { $task = $asTask.MakeGenericMethod($type).Invoke($null, @($operation)); [void]$task.Wait(-1); $task.Result }
  function AwaitAction($action) { $task = $asTaskAction.Invoke($null, @($action)); [void]$task.Wait(-1) }
  [void][Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
  [void][Windows.Storage.StorageFolder, Windows.Storage, ContentType = WindowsRuntime]
  [void][Windows.Storage.Streams.IRandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
  [void][Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType = WindowsRuntime]
  [void][Windows.Data.Pdf.PdfPageRenderOptions, Windows.Data.Pdf, ContentType = WindowsRuntime]
  $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($env:SP_PDF_IN)) ([Windows.Storage.StorageFile])
  try {
    $document = Await ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($file)) ([Windows.Data.Pdf.PdfDocument])
  } catch {
    if ($_.Exception.ToString() -match '8007052B|password') { [Console]::Error.WriteLine('locked'); exit 3 }
    throw
  }
  $count = [Math]::Min([int]$document.PageCount, [int]$env:SP_PDF_MOST)
  $invariant = [Globalization.CultureInfo]::InvariantCulture
  if ($env:SP_PDF_MODE -eq 'sizes') {
    [Console]::Out.WriteLine("pages $count")
    for ($at = 0; $at -lt $count; $at++) {
      $page = $document.GetPage($at)
      [Console]::Out.WriteLine($page.Size.Width.ToString($invariant) + ' ' + $page.Size.Height.ToString($invariant))
      $page.Dispose()
    }
  } else {
    $sizes = $env:SP_PDF_SIZES -split ';'
    $folder = Await ([Windows.Storage.StorageFolder]::GetFolderFromPathAsync($env:SP_PDF_OUT)) ([Windows.Storage.StorageFolder])
    for ($at = 0; $at -lt $count -and $at -lt $sizes.Count; $at++) {
      $size = $sizes[$at] -split ','
      $page = $document.GetPage($at)
      $made = Await ($folder.CreateFileAsync("$($at + 1).png", [Windows.Storage.CreationCollisionOption]::ReplaceExisting)) ([Windows.Storage.StorageFile])
      $stream = Await ($made.OpenAsync([Windows.Storage.FileAccessMode]::ReadWrite)) ([Windows.Storage.Streams.IRandomAccessStream])
      $options = New-Object Windows.Data.Pdf.PdfPageRenderOptions
      $options.DestinationWidth = [uint32]$size[0]
      $options.DestinationHeight = [uint32]$size[1]
      AwaitAction ($page.RenderToStreamAsync($stream, $options))
      $stream.Dispose()
      $page.Dispose()
      [Console]::Out.WriteLine("page $($at + 1)")
      [Console]::Out.Flush()
    }
  }
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;

/// The script as PowerShell's `-EncodedCommand` takes it: UTF-16LE, in base64.
#[cfg(any(windows, test))]
fn encoded(script: &str) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for three in bytes.chunks(3) {
        let n = (u32::from(three[0]) << 16)
            | (u32::from(three.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(three.get(2).copied().unwrap_or(0));
        for (place, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if place <= three.len() {
                out.push(char::from(LETTERS[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A page's size in pixels: as large as fits in the box, keeping its shape, as on a Mac.
#[cfg(any(windows, test))]
fn fitted(width: f64, height: f64, max_width: u32, max_height: u32) -> (u32, u32) {
    if !(width > 0.0 && height > 0.0) {
        return (max_width, max_height);
    }
    let scale = (f64::from(max_width) / width).min(f64::from(max_height) / height);
    (((width * scale).round() as u32).max(1), ((height * scale).round() as u32).max(1))
}

/// The pages' sizes from what the script printed in its `sizes` mode, fitted to the box.
#[cfg(any(windows, test))]
fn sizes_from(printed: &str, max_width: u32, max_height: u32) -> Result<Vec<(u32, u32)>, String> {
    let mut lines = printed.lines().map(str::trim).filter(|line| !line.is_empty());
    let count: usize = lines
        .next()
        .and_then(|line| line.strip_prefix("pages "))
        .and_then(|n| n.trim().parse().ok())
        .ok_or("Windows' PDF reader said nothing about the pages.")?;
    let sizes: Vec<(u32, u32)> = lines
        .take(count)
        .map(|line| {
            let mut parts = line.split_whitespace().map(|part| part.parse::<f64>().unwrap_or(0.0));
            fitted(parts.next().unwrap_or(0.0), parts.next().unwrap_or(0.0), max_width, max_height)
        })
        .collect();
    if sizes.is_empty() {
        return Err("has no pages.".to_owned());
    }
    Ok(sizes)
}

#[cfg(windows)]
fn shown_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// The full path, as Windows' own file calls take it: absolute, without the `\\?\` that
/// `canonicalize` puts in front.
#[cfg(windows)]
fn plain_full(path: &Path) -> std::path::PathBuf {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |here| here.join(path))
    };
    let text = full.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => std::path::PathBuf::from(rest),
        _ => full,
    }
}

/// Windows PowerShell, from Windows' own folder, ready to run a fixed script given as
/// `-EncodedCommand` (so no quoting can change it), with no console window. Values the script
/// needs go to it in environment variables set on the command, never into its text.
#[cfg(windows)]
pub fn powershell(script: &str) -> Result<std::process::Command, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot")
        .filter(|root| !root.is_empty())
        .ok_or("could not find Windows' own folder (SystemRoot is not set)")?;
    let program = Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = std::process::Command::new(program);
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded(script)])
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

/// What the script said when it failed, in plain words.
#[cfg(windows)]
fn failed(path: &Path, code: Option<i32>, said: &str) -> String {
    let name = shown_name(path);
    if code == Some(3) || said.trim() == "locked" {
        return format!("{name} is locked with a password. Save a copy without one, then try again.");
    }
    let said = said.lines().map(str::trim).rfind(|line| !line.is_empty()).unwrap_or("");
    if said.is_empty() {
        format!("{name} is not a PDF SoftPresenter can read.")
    } else {
        format!("{name} could not be read ({said}).")
    }
}

/// The size of each page, as large as fits in the box (at most `most_pages` of them).
#[cfg(windows)]
pub fn pdf_sizes(path: &Path, max_width: u32, max_height: u32, most_pages: usize) -> Result<Vec<(u32, u32)>, String> {
    let full = plain_full(path);
    let out = powershell(SCRIPT)?
        .env("SP_PDF_MODE", "sizes")
        .env("SP_PDF_IN", &full)
        .env("SP_PDF_MOST", most_pages.to_string())
        .output()
        .map_err(|e| format!("{}: Windows' PDF reader could not be started ({e}).", shown_name(path)))?;
    if !out.status.success() {
        return Err(failed(path, out.status.code(), &String::from_utf8_lossy(&out.stderr)));
    }
    sizes_from(&String::from_utf8_lossy(&out.stdout), max_width, max_height)
        .map_err(|why| format!("{} {why}", shown_name(path)))
}

/// Draws the pages of a PDF, each as [`pdf_sizes`] says, on white, and hands each to `each` with
/// its number (from 1), in order; `each` returns `false` to stop. Windows' reader draws them one
/// after another, so `threads` isn't used. Returns how many pages were drawn.
#[cfg(windows)]
pub fn pdf_draw(
    path: &Path,
    max_width: u32,
    max_height: u32,
    most_pages: usize,
    _threads: usize,
    each: impl Fn(usize, Frame) -> bool + Sync,
) -> Result<usize, String> {
    use std::io::BufRead;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    // A PDF may have many pages; each takes well under a second.
    const TIME_ALLOWED: Duration = Duration::from_secs(15 * 60);
    let sizes = pdf_sizes(path, max_width, max_height, most_pages)?;
    let full = plain_full(path);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    let folder = std::env::temp_dir().join(format!("softpresenter-pdf-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|e| format!("Could not make a folder to draw the pages in: {e}"))?;
    let listed: Vec<String> = sizes.iter().map(|(w, h)| format!("{w},{h}")).collect();
    let started = Instant::now();
    let child = powershell(SCRIPT)?
        .env("SP_PDF_MODE", "draw")
        .env("SP_PDF_IN", &full)
        .env("SP_PDF_OUT", &folder)
        .env("SP_PDF_MOST", most_pages.to_string())
        .env("SP_PDF_SIZES", listed.join(";"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(format!("{}: Windows' PDF reader could not be started ({e}).", shown_name(path)));
        }
    };
    let errors = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut said = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::Read::take(&mut stderr, 16 * 1024), &mut said);
            said
        })
    });
    let mut drawn = 0;
    let mut stopped = false;
    if let Some(stdout) = child.stdout.take() {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Some(number) = line.trim().strip_prefix("page ").and_then(|n| n.parse::<usize>().ok()) else {
                continue;
            };
            let file = folder.join(format!("{number}.png"));
            let picture = image::open(&file).map(image::DynamicImage::into_rgba8);
            let _ = std::fs::remove_file(&file);
            if let Ok(picture) = picture {
                drawn += 1;
                let frame = Frame { width: picture.width(), height: picture.height(), rgba: picture.into_raw() };
                if !each(number, frame) {
                    stopped = true;
                    break;
                }
            }
            if started.elapsed() > TIME_ALLOWED {
                stopped = true;
                break;
            }
        }
    }
    if stopped {
        let _ = child.kill();
    }
    let status = child.wait();
    let said = errors.and_then(|reader| reader.join().ok()).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&folder);
    match status {
        Ok(status) if !stopped && !status.success() && drawn == 0 => Err(failed(path, status.code(), &said)),
        _ => Ok(drawn),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_goes_to_powershell_as_utf16_in_base64() {
        // PowerShell's own documentation: "dir 'c:\program files' " in UTF-16LE.
        assert_eq!(
            encoded("dir 'c:\\program files' "),
            "ZABpAHIAIAAnAGMAOgBcAHAAcgBvAGcAcgBhAG0AIABmAGkAbABlAHMAJwAgAA=="
        );
        assert_eq!(encoded("a"), "YQA=");
        assert_eq!(encoded("ab"), "YQBiAA==");
        assert!(encoded(SCRIPT).len() < 30_000, "fits on a Windows command line");
    }

    #[test]
    fn pages_are_fitted_to_the_box_keeping_their_shape() {
        // A 16:9 slide fills the box; a letter page stands in the middle of it, as on a Mac.
        assert_eq!(fitted(960.0, 540.0, 1920, 1080), (1920, 1080));
        assert_eq!(fitted(816.0, 1056.0, 1920, 1080), (835, 1080));
        assert_eq!(fitted(0.0, 540.0, 1920, 1080), (1920, 1080), "a page with no size takes the box");
    }

    #[test]
    fn the_sizes_the_script_prints_are_read_in_any_locale() {
        let printed = "pages 2\r\n960 540\r\n612.5 792\r\n";
        assert_eq!(sizes_from(printed, 1920, 1080), Ok(vec![(1920, 1080), (835, 1080)]));
        assert!(sizes_from("pages 0\n", 1920, 1080).is_err());
        assert!(sizes_from("", 1920, 1080).is_err());
    }
}
