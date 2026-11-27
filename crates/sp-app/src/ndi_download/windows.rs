//! NDI's runtime for Windows, installed by NDI's own installer.
//!
//! NDI publishes its Windows runtime only as an installer (`NDI 6 Runtime.exe`, made with Inno
//! Setup, 11 MB in November 2026). Tried on a PC on 27 November 2026, it can't be kept in a
//! folder of the app's: asked to install hidden, for this user only, into a folder given, it
//! still installs as an administrator (Windows asks for the administrator's yes), points the
//! computer's own NDI settings (`NDI_RUNTIME_DIR_V2`…`V6`) at wherever it is put, registers a
//! DirectShow filter from there, and resets Windows' network settings (`netsh winsock reset`,
//! `netsh int tcp reset`). So on Windows SoftPresenter downloads the installer, checks who
//! signed it, and opens it as it is, in its own usual place; Windows asks for the
//! administrator's yes, never SoftPresenter. Remove opens NDI's own uninstaller the same way.
//!
//! What is trusted: the fixed HTTPS address on NDI's server, and then Windows' own check of
//! the installer's Authenticode signature, which must be valid and name NDI's company. The
//! installer is never run before that check, and never hidden.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::sdk::Problem;

/// NDI's fixed address for its Windows runtime (where `ndi.link/NDIRedistV6` leads).
pub const ADDRESS: &str = "https://downloads.ndi.tv/SDK/NDI_SDK/NDI%206%20Runtime.exe";
/// The name the download is kept under while it is checked and run.
pub const DOWNLOAD_FILE: &str = "NDI 6 Runtime.exe";
/// The company NDI's Windows software is signed by, read from NDI's installer on 27 November
/// 2026 (its certificate's name; the whole subject was "CN=Vizrt AG, O=Vizrt AG, L=Zürich,
/// C=CH", issued by DigiCert).
pub const SIGNER: &str = "Vizrt AG";
/// The largest installer run. NDI's was 11,277,792 bytes in November 2026.
pub const LARGEST_DOWNLOAD: u64 = 256 * 1024 * 1024;

/// True when what Windows said of a file's signature (its status, a bar, the signer's name)
/// is a valid signature by `signer`, the name exactly.
pub fn signed_by(said: &str, signer: &str) -> bool {
    said.trim().split_once('|').is_some_and(|(status, name)| status == "Valid" && name == signer)
}

/// Asks Windows (through PowerShell's `Get-AuthenticodeSignature`) who signed `file`, and
/// refuses it unless the signature is valid (the file whole, the certificate trusted by
/// Windows) and the signer's name is NDI's company. The name is the certificate's own name
/// field, asked for by itself, never searched for in a longer text. The file's name reaches
/// PowerShell in a variable of its environment, never in the command's words.
pub fn check_signature(powershell: &Path, file: &Path) -> Result<(), Problem> {
    const ASK: &str = "$s = Get-AuthenticodeSignature -LiteralPath $env:SOFTPRESENTER_NDI_FILE; $n = ''; if ($s.SignerCertificate) { $n = $s.SignerCertificate.GetNameInfo('SimpleName', $false) }; [Console]::Out.Write($s.Status.ToString() + '|' + $n)";
    let answer = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", ASK])
        .env("SOFTPRESENTER_NDI_FILE", file)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| Problem::Unsigned(format!("the signature couldn't be read: {e}")))?;
    let said = String::from_utf8_lossy(&answer.stdout);
    if signed_by(&said, SIGNER) { Ok(()) } else { Err(Problem::Unsigned(said.chars().take(200).collect())) }
}

/// Opens a program (NDI's installer, or its uninstaller) as it is and waits for it to end:
/// well, or [`Problem::Closed`] if it ended any other way (closed, or the administrator's yes
/// not given). `stop` (Stop Waiting) gives up waiting and leaves the program alone.
pub fn open_and_wait(program: &Path, words: &[&str], stop: &AtomicBool) -> Result<(), Problem> {
    // Nothing of this program's is handed to it: it is its own program, with its own window.
    let mut child = Command::new(program)
        .args(words)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Problem::Closed)?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => return Err(Problem::Closed),
            Ok(None) if stop.load(Ordering::Relaxed) => return Err(Problem::Cancelled),
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// NDI's own uninstaller for a runtime its installer put at `runtime`
/// (`…\NDI 6 Runtime\v6\Processing.NDI.Lib.x64.dll`): `unins000.exe` in the folder named for
/// the runtime. None for a runtime that came some other way (with NDI Tools, say), which
/// SoftPresenter leaves alone.
pub fn uninstaller_for(runtime: &Path) -> Option<PathBuf> {
    let folder = runtime.ancestors().skip(1).take(2).find(|folder| {
        folder
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("NDI ") && name.ends_with(" Runtime"))
    })?;
    let uninstaller = folder.join("unins000.exe");
    uninstaller.is_file().then_some(uninstaller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_valid_signature_of_ndis_company_counts() {
        // NDI's installer on 27 November 2026 was valid, its subject "CN=Vizrt AG, …": the name
        // asked for here is that CN. (The exact answer to this question was not read off the PC.)
        assert!(signed_by("Valid|Vizrt AG", SIGNER));
        for other in [
            "NotSigned|",
            "HashMismatch|Vizrt AG",
            "NotTrusted|Vizrt AG",
            "Valid|Somebody Else",
            // NDI's name inside another name proves nothing.
            "Valid|Not Vizrt AG",
            "Valid|Vizrt AG Fan Club",
            "Valid|Evil, O=Vizrt AG",
            "Valid|",
            "Valid",
            "",
        ] {
            assert!(!signed_by(other, SIGNER), "{other}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_installer_that_ends_well_is_done_and_one_closed_early_is_said_so() {
        let stop = AtomicBool::new(false);
        let shell = Path::new("/bin/sh");
        assert_eq!(open_and_wait(shell, &["-c", "exit 0"], &stop), Ok(()));
        // Inno Setup ends with 2 or 5 when its window is closed before or during the install.
        assert_eq!(open_and_wait(shell, &["-c", "exit 2"], &stop), Err(Problem::Closed));
        assert_eq!(open_and_wait(Path::new("/nonexistent/installer"), &[], &stop), Err(Problem::Closed));
        // Stop Waiting gives up, and leaves the installer to itself.
        stop.store(true, Ordering::Relaxed);
        assert_eq!(open_and_wait(shell, &["-c", "sleep 1"], &stop), Err(Problem::Cancelled));
    }

    #[test]
    fn only_the_runtimes_own_uninstaller_is_found() {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-windows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let runtime = folder.join("NDI").join("NDI 6 Runtime").join("v6");
        let tools = folder.join("NDI").join("NDI 6 Tools").join("Runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(folder.join("NDI/NDI 6 Runtime/unins000.exe"), b"stand-in").unwrap();
        std::fs::write(folder.join("NDI/NDI 6 Tools/unins000.exe"), b"stand-in").unwrap();
        assert_eq!(
            uninstaller_for(&runtime.join("Processing.NDI.Lib.x64.dll")),
            Some(folder.join("NDI/NDI 6 Runtime/unins000.exe"))
        );
        assert_eq!(uninstaller_for(&tools.join("Processing.NDI.Lib.x64.dll")), None, "NDI Tools is left alone");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
