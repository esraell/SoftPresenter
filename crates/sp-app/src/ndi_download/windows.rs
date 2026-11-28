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
//! the installer's Authenticode signature, which must be valid, name NDI's company and come
//! from its certificate authority. It is checked when it is downloaded and again right before
//! it runs, the file held against any change from that check until it has started; it is
//! never hidden. NDI's uninstaller carries no signature, so the one run is the one NDI's own
//! entry among the programs installed for every user names (a part of the registry only an
//! administrator can write), never one found beside a runtime or through the environment.

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

/// The certificate authority NDI's company has its signing certificate from, by the start of
/// its name (the issuer on 27 November 2026 was "DigiCert Trusted G4 Code Signing RSA4096
/// SHA384 2021 CA1"). Should NDI change authority, installing refuses until this is changed.
pub const ISSUER: &str = "DigiCert ";

/// True when what Windows said of a file's signature is a valid signature of NDI's company:
/// its status, the signer's name, the issuer's name and the signer's name parts, with bars
/// between them. The status must be "Valid" (the file whole, the certificate trusted by
/// Windows), the name exactly `signer`, the issuer one of `issuer`'s, and the organisation
/// among the name's parts exactly `signer` too.
pub fn signed_by(said: &str, signer: &str, issuer: &str) -> bool {
    let mut fields = said.trim().split('|');
    let (status, name, by, parts) = (fields.next(), fields.next(), fields.next(), fields.next());
    let organisation = format!("O={signer}");
    status == Some("Valid")
        && name == Some(signer)
        && by.is_some_and(|by| by.starts_with(issuer))
        && parts.is_some_and(|parts| parts.split(';').any(|part| part == organisation))
        && fields.next().is_none()
}

/// Windows' own PowerShell, by its full path.
pub fn powershell() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|windows| PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

/// Asks Windows (through PowerShell's `Get-AuthenticodeSignature`) who signed `file`, and
/// refuses it unless [`signed_by`] says it is NDI's company's. Each name is asked for by
/// itself, never searched for in a longer text. The file's name reaches PowerShell in a
/// variable of its environment, never in the command's words.
pub fn check_signature(powershell: &Path, file: &Path) -> Result<(), Problem> {
    const ASK: &str = "$s = Get-AuthenticodeSignature -LiteralPath $env:SOFTPRESENTER_NDI_FILE; $n = ''; $i = ''; $r = ''; $c = $s.SignerCertificate; if ($c) { $n = $c.GetNameInfo('SimpleName', $false); $i = $c.GetNameInfo('SimpleName', $true); $r = (($c.SubjectName.Format($true) -split \"`r?`n\") | ForEach-Object { $_.Trim() } | Where-Object { $_ }) -join ';' }; [Console]::Out.Write($s.Status.ToString() + '|' + $n + '|' + $i + '|' + $r)";
    let answer = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", ASK])
        .env("SOFTPRESENTER_NDI_FILE", file)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| Problem::Unsigned(format!("the signature couldn't be read: {e}")))?;
    let said = String::from_utf8_lossy(&answer.stdout);
    if signed_by(&said, SIGNER, ISSUER) { Ok(()) } else { Err(Problem::Unsigned(said.chars().take(300).collect())) }
}

/// Opens `file` so that, for as long as what is given back is kept, nothing can write to it,
/// replace it or delete it (Windows: no sharing but for reading). Elsewhere, just open.
pub fn hold(file: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ alone: others may read (Windows must, to start it), nothing else.
        options.share_mode(1);
    }
    options.open(file)
}

/// Checks NDI's installer once more, right before it runs, and opens it: the file is held
/// (see [`hold`]) from before that check until the installer has started, so what runs is
/// what was checked. Then as [`open_and_wait`].
pub fn check_and_open(powershell: &Path, installer: &Path, stop: &AtomicBool) -> Result<(), Problem> {
    let held = hold(installer).map_err(|e| Problem::Damaged(e.to_string()))?;
    check_signature(powershell, installer)?;
    open_and_wait(installer, &[], stop, Some(held))
}

/// Opens a program (NDI's installer, or its uninstaller) as it is and waits for it to end:
/// well, or [`Problem::Closed`] if it ended any other way (closed, or the administrator's yes
/// not given). `stop` (Stop Waiting) gives up waiting and leaves the program alone. `held`,
/// the file kept from being changed, is let go once the program has started.
pub fn open_and_wait(
    program: &Path,
    words: &[&str],
    stop: &AtomicBool,
    held: Option<std::fs::File>,
) -> Result<(), Problem> {
    // Nothing of this program's is handed to it: it is its own program, with its own window.
    let started =
        Command::new(program).args(words).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    drop(held);
    let mut child = started.map_err(|_| Problem::Closed)?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => return Err(Problem::Closed),
            Ok(None) if stop.load(Ordering::Relaxed) => return Err(Problem::Cancelled),
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Where NDI's runtime installer notes itself among the programs installed for every user:
/// its own entry in the computer's part of the registry, which only an administrator can
/// write (read off a PC it had installed on, 27 November 2026).
const INSTALLED_ENTRY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{71AFF296-ED43-4166-8301-4649285EE712}_is1";

/// NDI's own uninstaller, as NDI's installer noted it for Windows: the program its entry among
/// the installed programs names, and nothing else. Where NDI is, or what the environment says
/// of it, is never used to find it: those a user's own programs can set, and NDI's uninstaller
/// carries no signature to tell it by (read on NDI's own, 27 November 2026). None where NDI's
/// runtime installer has installed nothing (NDI Tools, say, which SoftPresenter leaves alone).
pub fn uninstaller() -> Option<PathBuf> {
    let noted = sp_winapi::machine_text(INSTALLED_ENTRY, "UninstallString")?;
    uninstaller_noted(&noted).filter(|uninstaller| uninstaller.is_file())
}

/// The uninstaller an installed-programs entry names (`"C:\…\NDI 6 Runtime\unins000.exe"`,
/// in quotes or bare): a whole path from a drive's top, with no step back up (`..`) in it,
/// to a file named `unins000.exe` in any case. Anything else is refused.
pub fn uninstaller_noted(noted: &str) -> Option<PathBuf> {
    let noted = noted.trim();
    let path = match noted.strip_prefix('"') {
        Some(rest) => rest.split_once('"')?.0,
        None => noted,
    };
    let parts: Vec<&str> = path.split(['\\', '/']).collect();
    let from_a_drive = parts
        .first()
        .is_some_and(|drive| drive.len() == 2 && drive.ends_with(':') && drive.as_bytes()[0].is_ascii_alphabetic());
    let plain = parts.iter().skip(1).all(|part| !part.is_empty() && *part != ".." && *part != ".");
    let named = parts.last().is_some_and(|name| name.eq_ignore_ascii_case("unins000.exe"));
    (from_a_drive && parts.len() >= 2 && plain && named).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_valid_signature_of_ndis_company_from_its_authority_counts() {
        // NDI's installer on 27 November 2026: valid; subject "CN=Vizrt AG, O=Vizrt AG,
        // L=Zürich, C=CH"; issued by "DigiCert Trusted G4 Code Signing RSA4096 SHA384 2021 CA1".
        // (These four fields as PowerShell gives them one by one were not read off the PC.)
        let ndis = "Valid|Vizrt AG|DigiCert Trusted G4 Code Signing RSA4096 SHA384 2021 CA1|CN=Vizrt AG;O=Vizrt AG;L=Z\u{fc}rich;C=CH";
        assert!(signed_by(ndis, SIGNER, ISSUER));
        for other in [
            "NotSigned|||",
            &ndis.replace("Valid", "HashMismatch"),
            &ndis.replace("Valid", "NotTrusted"),
            // Another signer, even one whose name holds NDI's.
            &ndis.replace("|Vizrt AG|", "|Somebody Else|"),
            &ndis.replace("|Vizrt AG|", "|Not Vizrt AG|"),
            &ndis.replace("|Vizrt AG|", "|Vizrt AG Fan Club|"),
            // NDI's name on a certificate from some other authority, or of another organisation.
            &ndis.replace("DigiCert Trusted G4", "Some Other Authority"),
            &ndis.replace("O=Vizrt AG", "O=Somebody Else"),
            &ndis.replace("O=Vizrt AG", "O=Vizrt AG Fan Club"),
            "Valid|Vizrt AG",
            "Valid|Vizrt AG|DigiCert X|",
            &format!("{ndis}|more"),
            "",
        ] {
            assert!(!signed_by(other, SIGNER, ISSUER), "{other}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_installer_that_ends_well_is_done_and_one_closed_early_is_said_so() {
        let stop = AtomicBool::new(false);
        let shell = Path::new("/bin/sh");
        assert_eq!(open_and_wait(shell, &["-c", "exit 0"], &stop, None), Ok(()));
        // Inno Setup ends with 2 or 5 when its window is closed before or during the install.
        assert_eq!(open_and_wait(shell, &["-c", "exit 2"], &stop, None), Err(Problem::Closed));
        assert_eq!(open_and_wait(Path::new("/nonexistent/installer"), &[], &stop, None), Err(Problem::Closed));
        // Stop Waiting gives up, and leaves the installer to itself.
        stop.store(true, Ordering::Relaxed);
        assert_eq!(open_and_wait(shell, &["-c", "sleep 1"], &stop, None), Err(Problem::Cancelled));
    }

    /// A stand-in for PowerShell that says `said` whatever it is asked.
    #[cfg(unix)]
    fn powershell_saying(folder: &Path, said: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = folder.join("powershell");
        std::fs::write(&path, format!("#!/bin/sh\nprintf '%s' '{said}'\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn the_installer_is_checked_again_right_before_it_runs_and_not_run_if_it_fails() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-windows-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        // A stand-in installer that leaves a mark when it runs.
        let installer = folder.join("installer");
        let mark = folder.join("ran");
        std::fs::write(&installer, format!("#!/bin/sh\n: > '{}'\n", mark.display())).unwrap();
        std::fs::set_permissions(&installer, std::fs::Permissions::from_mode(0o755)).unwrap();
        let stop = AtomicBool::new(false);
        // Not NDI's signature at that moment: refused, and never started.
        let refuses = powershell_saying(&folder, "NotSigned|||");
        assert!(matches!(check_and_open(&refuses, &installer, &stop), Err(Problem::Unsigned(_))));
        assert!(!mark.exists(), "it was not run");
        // NDI's: started.
        let ndis =
            powershell_saying(&folder, "Valid|Vizrt AG|DigiCert Trusted G4 Code Signing|CN=Vizrt AG;O=Vizrt AG;C=CH");
        assert_eq!(check_and_open(&ndis, &installer, &stop), Ok(()));
        assert!(mark.exists(), "it ran");
        // Gone by the time it is to run: nothing is started.
        assert!(check_and_open(&ndis, &folder.join("missing"), &stop).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn only_the_uninstaller_ndis_own_entry_names_is_taken() {
        // As NDI's installer wrote it on a PC.
        let noted = r#""C:\Program Files\NDI\NDI 6 Runtime\unins000.exe""#;
        assert_eq!(uninstaller_noted(noted), Some(PathBuf::from(r"C:\Program Files\NDI\NDI 6 Runtime\unins000.exe")));
        // Bare, and in another case: the same file to Windows.
        assert!(uninstaller_noted(r"c:\PROGRAM FILES\ndi\NDI 6 Runtime\UNINS000.EXE").is_some());
        for refused in [
            // A step back up, which would lead out of where it says it is.
            r#""C:\Program Files\..\Users\someone\x\NDI 6 Runtime\unins000.exe""#,
            r"C:\Program Files\NDI\.\unins000.exe",
            // Not a whole path from a drive, another program, or nothing.
            r"unins000.exe",
            r"\\server\share\unins000.exe",
            r"..\unins000.exe",
            r#""C:\Program Files\NDI\NDI 6 Runtime\setup.exe""#,
            r#""C:\Program Files\NDI\NDI 6 Runtime\unins000.exe.cmd""#,
            r#""C:\Program Files\NDI"#,
            "",
        ] {
            assert_eq!(uninstaller_noted(refused), None, "{refused}");
        }
        // Words after the program (as some entries have) are left off, never run.
        let with_words = r#""C:\Program Files\NDI\NDI 6 Runtime\unins000.exe" /SILENT"#;
        assert_eq!(uninstaller_noted(with_words), uninstaller_noted(noted));
        // On a computer where NDI's runtime installer installed nothing, there is none.
        if !cfg!(windows) {
            assert_eq!(uninstaller(), None);
        }
    }
}
