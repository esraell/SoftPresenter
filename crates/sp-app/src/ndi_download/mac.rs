//! NDI's runtime for a Mac, taken out of NDI's own package without installing it.
//!
//! NDI publishes its Mac runtime as an installer package (`libNDI_for_Mac.pkg`, 4.8 MB in
//! November 2026), signed with NDI's Developer ID and notarized by Apple. Its installer would
//! put `libndi.dylib` in `/usr/local/lib`, which needs an administrator. SoftPresenter doesn't
//! run it: once the package's signature is checked, macOS's own `pkgutil --expand-full` lays
//! its contents out in a scratch folder (no installer runs, no administrator is asked), and
//! the one `libndi.dylib` in it, itself signed by the same team and built for this processor,
//! is copied into the app's own folder with the notices beside it.
//!
//! What is trusted, in order: the fixed HTTPS address on NDI's server; the package's signature
//! (a Developer ID Installer certificate of NDI's team, as `pkgutil --check-signature` reads
//! it, which also checks the package is whole); then the library's own signature
//! (`codesign --verify --strict`, and its team). Nothing from the package is run, nothing is
//! written under a name the package gives, and links in it are never followed.
//!
//! The package as NDI ships it holds the library and `libndi_licenses.txt` only: NDI's licence
//! text is not in it, so [`Contents::licence`] is empty until it is decided where a Mac reads
//! the licence from.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::sdk::{Problem, write_whole};

/// NDI's fixed address for its Mac runtime (where `ndi.link/NDIRedistV6Apple` leads).
pub const ADDRESS: &str = "https://downloads.ndi.tv/SDK/NDI_SDK_Mac/libNDI_for_Mac.pkg";
/// The team NDI signs its Mac software with ("NewTek, Inc."), read from NDI's package and its
/// library on 27 November 2026.
pub const TEAM: &str = "W8U66ET244";
/// The name the runtime has in the package, and is kept under.
pub const RUNTIME_FILE: &str = "libndi.dylib";
pub const NOTICES_FILE: &str = "libndi_licenses.txt";
/// The largest package read. NDI's was 4,794,396 bytes in November 2026.
pub const LARGEST_DOWNLOAD: u64 = 256 * 1024 * 1024;
const SMALLEST_RUNTIME: u64 = 64 * 1024;
const LARGEST_RUNTIME: u64 = 512 * 1024 * 1024;
const LARGEST_NOTICES: u64 = 4 * 1024 * 1024;
/// How deep, and through how many entries, the expanded package is looked through.
const DEEPEST: usize = 6;
const MOST_ENTRIES: usize = 5000;
/// How long one of macOS's tools may take.
const TOOL_TIME: Duration = Duration::from_secs(120);

/// macOS's own tools, by their full paths (a test gives stand-ins).
pub struct Tools {
    pub pkgutil: PathBuf,
    pub codesign: PathBuf,
}

impl Tools {
    pub fn system() -> Tools {
        Tools { pkgutil: PathBuf::from("/usr/sbin/pkgutil"), codesign: PathBuf::from("/usr/bin/codesign") }
    }
}

/// What the package holds that SoftPresenter takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    /// NDI's licence, where the package has it (NDI's doesn't).
    pub licence: Option<String>,
    notices: Option<String>,
    /// Where the runtime lies in the scratch folder, with its size and SHA-256 as read.
    runtime: PathBuf,
    runtime_size: u64,
    runtime_sha256: [u8; 32],
    pub version: String,
}

/// Runs one of macOS's tools with the words given and nothing of this program's environment,
/// and gives what it printed (both streams, at most 64 KB) and whether it ended well.
fn run(
    tool: &Path,
    words: &[&std::ffi::OsStr],
    scratch: &Path,
    cancel: &AtomicBool,
) -> Result<(bool, String), Problem> {
    let said = scratch.join(".tool-said");
    let out = std::fs::File::create(&said).map_err(Problem::disk)?;
    let err = out.try_clone().map_err(Problem::disk)?;
    let mut child = Command::new(tool)
        .args(words)
        .env_clear()
        .current_dir(scratch)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| Problem::Layout(format!("{} could not be run: {e}", tool.display())))?;
    let began = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if cancel.load(Ordering::Relaxed) || began.elapsed() > TOOL_TIME => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&said);
                return Err(if cancel.load(Ordering::Relaxed) {
                    Problem::Cancelled
                } else {
                    Problem::Layout(format!("{} took too long", tool.display()))
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(Problem::Layout(e.to_string())),
        }
    };
    let mut text = Vec::new();
    let _ = std::fs::File::open(&said).map(|file| file.take(64 * 1024).read_to_end(&mut text));
    let _ = std::fs::remove_file(&said);
    Ok((status.success(), String::from_utf8_lossy(&text).into_owned()))
}

/// True when what `pkgutil --check-signature` printed says the package is signed with a
/// Developer ID Installer certificate of `team`, issued by Apple.
pub fn package_signed_by(said: &str, team: &str) -> bool {
    let apple = said.lines().any(|line| {
        line.trim().starts_with("Status:") && line.contains("signed by a developer certificate issued by Apple")
    });
    let ending = format!("({team})");
    let first = said.lines().map(str::trim).find(|line| line.starts_with("1. "));
    apple && first.is_some_and(|line| line.starts_with("1. Developer ID Installer: ") && line.ends_with(&ending))
}

/// The team in what `codesign -dv` printed.
pub fn library_team(said: &str) -> Option<&str> {
    said.lines().find_map(|line| line.trim().strip_prefix("TeamIdentifier="))
}

/// The version a package's `PackageInfo` gives: its first two to four numbers
/// ("6.4.0.2.261117", whose last is a date, is 6.4.0.2; a fourth that is 0 is left off).
pub fn package_version(info: &str) -> Option<String> {
    let rest = info.split_once("<pkg-info")?.1;
    let rest = rest.split_once(" version=\"")?.1;
    let whole = rest.split_once('"')?.0;
    let mut parts: Vec<&str> = whole.split('.').take(4).collect();
    let numbers =
        parts.iter().all(|part| !part.is_empty() && part.len() <= 5 && part.bytes().all(|b| b.is_ascii_digit()));
    if !numbers || parts.len() < 2 {
        return None;
    }
    if parts.len() == 4 && parts[3] == "0" {
        parts.pop();
    }
    Some(parts.join("."))
}

/// True when the bytes start a Mach-O shared library for `processor`: one for several
/// processors that has it, or one for it alone.
pub fn library_for(head: &[u8], processor: &str) -> bool {
    let wanted: u32 = match processor {
        "x86_64" => 0x0100_0007,
        "aarch64" => 0x0100_000c,
        _ => return false,
    };
    let be = |at: usize| head.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le = |at: usize| head.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    match be(0) {
        // Several processors: a count, then each one's kind in an entry of 20 bytes.
        Some(0xcafe_babe) => {
            let count = be(4).unwrap_or(0).min(8) as usize;
            (0..count).any(|n| be(8 + 20 * n) == Some(wanted))
        }
        // One 64-bit processor (little-endian): its kind, and file type 6, a shared library.
        Some(0xcffa_edfe) => le(4) == Some(wanted) && le(12) == Some(6),
        _ => false,
    }
}

/// The plain files named `name` under `folder`, never following a link, looking no deeper than
/// [`DEEPEST`] and at no more than [`MOST_ENTRIES`] entries.
fn files_named(folder: &Path, name: &str) -> Result<Vec<PathBuf>, Problem> {
    let mut found = Vec::new();
    let mut seen = 0usize;
    let mut waiting = vec![(folder.to_path_buf(), 0usize)];
    while let Some((at, depth)) = waiting.pop() {
        for entry in std::fs::read_dir(&at).map_err(|e| Problem::Layout(e.to_string()))?.flatten() {
            seen += 1;
            if seen > MOST_ENTRIES {
                return Err(Problem::Layout("the package holds far more than NDI's does".to_owned()));
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() && depth < DEEPEST {
                waiting.push((entry.path(), depth + 1));
            } else if kind.is_file() && entry.file_name() == name {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

/// A file's size and SHA-256, read through once.
fn measure(file: &Path) -> Result<(u64, [u8; 32]), Problem> {
    let mut hasher = Sha256::new();
    let mut reader = std::fs::File::open(file).map_err(|e| Problem::Damaged(e.to_string()))?;
    let size = std::io::copy(&mut reader, &mut hasher).map_err(|e| Problem::Damaged(e.to_string()))?;
    Ok((size, hasher.finalize().into()))
}

/// Checks NDI's package and lays it out in `scratch` (a new, empty folder of the app's own),
/// and finds the runtime for `processor` in it. Installs nothing.
pub fn read(
    tools: &Tools,
    package: &Path,
    scratch: &Path,
    processor: &str,
    cancel: &AtomicBool,
) -> Result<Contents, Problem> {
    let word = std::ffi::OsStr::new;
    let (well, said) = run(&tools.pkgutil, &[word("--check-signature"), package.as_os_str()], scratch, cancel)?;
    if !well || !package_signed_by(&said, TEAM) {
        let first = said.lines().nth(1).unwrap_or("").trim().to_owned();
        return Err(Problem::Unsigned(format!("the package: {first}")));
    }
    let expanded = scratch.join("expanded");
    let (well, said) =
        run(&tools.pkgutil, &[word("--expand-full"), package.as_os_str(), expanded.as_os_str()], scratch, cancel)?;
    if !well {
        return Err(Problem::Layout(format!(
            "the package couldn't be laid out: {}",
            said.lines().next().unwrap_or("")
        )));
    }
    let runtimes = files_named(&expanded, RUNTIME_FILE)?;
    let [runtime] = runtimes.as_slice() else {
        return Err(Problem::Layout(format!("{} files named {RUNTIME_FILE}", runtimes.len())));
    };
    let mut head = [0u8; 256];
    let read = std::fs::File::open(runtime)
        .and_then(|mut file| file.read(&mut head))
        .map_err(|e| Problem::Damaged(e.to_string()))?;
    if !library_for(&head[..read], processor) {
        return Err(Problem::NoRuntime(processor.to_owned()));
    }
    let (well, _) = run(&tools.codesign, &[word("--verify"), word("--strict"), runtime.as_os_str()], scratch, cancel)?;
    let (_, said) = run(&tools.codesign, &[word("-dv"), runtime.as_os_str()], scratch, cancel)?;
    if !well || library_team(&said) != Some(TEAM) {
        return Err(Problem::Unsigned(format!("the library: team {}", library_team(&said).unwrap_or("none"))));
    }
    let (runtime_size, runtime_sha256) = measure(runtime)?;
    if !(SMALLEST_RUNTIME..=LARGEST_RUNTIME).contains(&runtime_size) {
        return Err(Problem::Layout(format!("{RUNTIME_FILE} is {runtime_size} bytes")));
    }
    let notices = runtime
        .parent()
        .map(|beside| beside.join(NOTICES_FILE))
        .filter(|file| {
            std::fs::symlink_metadata(file).is_ok_and(|about| about.is_file() && about.len() <= LARGEST_NOTICES)
        })
        .and_then(|file| std::fs::read(file).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let version = runtime
        .parent()
        .and_then(Path::parent)
        .map(|component| component.join("PackageInfo"))
        .filter(|file| std::fs::symlink_metadata(file).is_ok_and(|about| about.is_file() && about.len() < 64 * 1024))
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|info| package_version(&info))
        .unwrap_or_default();
    Ok(Contents { licence: None, notices, runtime: runtime.clone(), runtime_size, runtime_sha256, version })
}

/// Copies what [`read`] found into `into`: the notices, then the runtime as [`RUNTIME_FILE`],
/// last and in one step, and only if it is still the file that was checked. Only ever writes
/// those names, in `into`.
pub fn install(contents: &Contents, into: &Path) -> Result<(), Problem> {
    std::fs::create_dir_all(into).map_err(Problem::disk)?;
    let part = into.join(format!(".{RUNTIME_FILE}.part"));
    let _ = std::fs::remove_file(&part);
    let copied = (|| {
        let mut from = std::fs::File::open(&contents.runtime).map_err(|e| Problem::Damaged(e.to_string()))?;
        // A new file (never written through something already at that name).
        let mut to = std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(Problem::disk)?;
        let mut hasher = Sha256::new();
        let (mut size, mut buffer) = (0u64, vec![0u8; 256 * 1024]);
        loop {
            let read = from.read(&mut buffer).map_err(|e| Problem::Damaged(e.to_string()))?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > contents.runtime_size {
                break;
            }
            hasher.update(&buffer[..read]);
            to.write_all(&buffer[..read]).map_err(Problem::disk)?;
        }
        let same: [u8; 32] = hasher.finalize().into();
        if size != contents.runtime_size || same != contents.runtime_sha256 {
            return Err(Problem::Damaged("the library changed after it was checked".to_owned()));
        }
        to.sync_all().map_err(Problem::disk)?;
        if let Some(notices) = &contents.notices {
            write_whole(into, NOTICES_FILE, notices.as_bytes())?;
        }
        std::fs::rename(&part, into.join(RUNTIME_FILE)).map_err(Problem::disk)
    })();
    if copied.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `pkgutil --check-signature` printed for NDI's package on 27 November 2026 (the
    /// fingerprints left out).
    const NDIS_SIGNATURE: &str = "Package \"libNDI_for_Mac.pkg\":\n   Status: signed by a developer certificate issued by Apple for distribution\n   Notarization: trusted by the Apple notary service\n   Signed with a trusted timestamp on: 2026-11-17 20:15:34 +0000\n   Certificate Chain:\n    1. Developer ID Installer: NewTek, Inc. (W8U66ET244)\n       Expires: 2028-01-04 21:27:55 +0000\n    2. Developer ID Certification Authority\n    3. Apple Root CA\n";

    #[test]
    fn only_a_package_signed_by_ndis_team_counts() {
        assert!(package_signed_by(NDIS_SIGNATURE, TEAM));
        let other = NDIS_SIGNATURE.replace("W8U66ET244", "ABCDE12345");
        assert!(!package_signed_by(&other, TEAM));
        // The team named further down the chain, or in the package's name, proves nothing.
        let named = other
            .replace("libNDI_for_Mac.pkg", "(W8U66ET244)")
            .replace("2. Developer ID Certification Authority", "2. Developer ID Installer: NewTek, Inc. (W8U66ET244)");
        assert!(!package_signed_by(&named, TEAM));
        assert!(!package_signed_by("Package \"x.pkg\":\n   Status: no signature\n", TEAM));
        let untrusted = NDIS_SIGNATURE.replace("issued by Apple for distribution", "that is not trusted");
        assert!(!package_signed_by(&untrusted, TEAM));
        // An application's certificate is not an installer's.
        assert!(!package_signed_by(
            &NDIS_SIGNATURE.replace("Developer ID Installer", "Developer ID Application"),
            TEAM
        ));
    }

    #[test]
    fn the_librarys_team_and_the_packages_version_are_read() {
        let said = "Executable=/x/libndi.dylib\nIdentifier=libndi\nFormat=Mach-O universal (x86_64 arm64)\nAuthority=Developer ID Application: NewTek, Inc. (W8U66ET244)\nTeamIdentifier=W8U66ET244\n";
        assert_eq!(library_team(said), Some("W8U66ET244"));
        assert_eq!(library_team("Identifier=libndi\nTeamIdentifier=not set\n"), Some("not set"));
        assert_eq!(library_team("code object is not signed at all\n"), None);
        let info = "<?xml version=\"1.0\"?>\n<pkg-info overwrite-permissions=\"true\" identifier=\"com.newtek.libndi\" version=\"6.4.0.2.261117\" format-version=\"2\">";
        assert_eq!(package_version(info).as_deref(), Some("6.4.0.2"));
        assert_eq!(package_version(&info.replace("6.4.0.2.261117", "6.3.2.0")).as_deref(), Some("6.3.2"));
        assert_eq!(package_version(&info.replace("6.4.0.2.261117", "six")), None);
        assert_eq!(package_version("<installer-gui-script/>"), None);
    }

    /// A made-up library for two processors (x86_64 and arm64), `size` bytes long.
    fn library(size: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; size];
        bytes[..4].copy_from_slice(&0xcafe_babe_u32.to_be_bytes());
        bytes[4..8].copy_from_slice(&2u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&0x0100_0007_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&0x0100_000c_u32.to_be_bytes());
        for (at, byte) in bytes.iter_mut().enumerate().skip(64) {
            *byte = (at % 249) as u8;
        }
        bytes
    }

    #[test]
    fn a_library_must_be_for_this_processor() {
        let both = library(4096);
        assert!(library_for(&both, "x86_64") && library_for(&both, "aarch64"));
        assert!(!library_for(&both, "riscv64"));
        let mut one = vec![0u8; 64];
        one[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        one[4..8].copy_from_slice(&0x0100_000c_u32.to_le_bytes());
        one[12..16].copy_from_slice(&6u32.to_le_bytes());
        assert!(library_for(&one, "aarch64") && !library_for(&one, "x86_64"));
        // A program, not a shared library; and something that isn't Mach-O at all.
        one[12..16].copy_from_slice(&2u32.to_le_bytes());
        assert!(!library_for(&one, "aarch64"));
        assert!(!library_for(b"\x7fELF and more", "x86_64") && !library_for(&[], "x86_64"));
    }

    /// Stand-ins for macOS's two tools, as shell scripts: `pkgutil` prints `signature` for
    /// `--check-signature` and copies `payload` to where `--expand-full` is told; `codesign`
    /// says `team`.
    #[cfg(unix)]
    fn stand_ins(folder: &Path, signature: &str, payload: &Path, team: &str) -> Tools {
        use std::os::unix::fs::PermissionsExt;
        let script = |name: &str, body: String| {
            let path = folder.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        std::fs::write(folder.join("signature.txt"), signature).unwrap();
        let pkgutil = script(
            "pkgutil",
            format!(
                "#!/bin/sh\nif [ \"$1\" = --check-signature ]; then /bin/cat '{}'; exit 0; fi\nif [ \"$1\" = --expand-full ]; then /bin/cp -R '{}' \"$3\"; exit $?; fi\nexit 1\n",
                folder.join("signature.txt").display(),
                payload.display()
            ),
        );
        let codesign = script(
            "codesign",
            format!(
                "#!/bin/sh\nif [ \"$1\" = -dv ]; then echo 'Identifier=libndi' >&2; echo 'TeamIdentifier={team}' >&2; fi\nexit 0\n"
            ),
        );
        Tools { pkgutil, codesign }
    }

    #[cfg(unix)]
    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-mac-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("scratch")).unwrap();
        folder
    }

    /// A package laid out as NDI's is once expanded, made up for the tests.
    #[cfg(unix)]
    fn payload(folder: &Path) -> PathBuf {
        let payload = folder.join("payload");
        let component = payload.join("libNDIComponent.pkg");
        std::fs::create_dir_all(component.join("Payload")).unwrap();
        std::fs::write(payload.join("Distribution"), "<installer-gui-script/>").unwrap();
        std::fs::write(
            component.join("PackageInfo"),
            "<pkg-info identifier=\"com.newtek.libndi\" version=\"6.4.0.2.261117\" install-location=\"/usr/local/lib\">",
        )
        .unwrap();
        std::fs::write(component.join("Payload").join(RUNTIME_FILE), library(100_000)).unwrap();
        std::fs::write(component.join("Payload").join(NOTICES_FILE), "Test notices.\n").unwrap();
        payload
    }

    #[cfg(unix)]
    #[test]
    fn a_signed_package_is_laid_out_and_its_library_installed_under_fixed_names() {
        let folder = folder("whole");
        let tools = stand_ins(&folder, NDIS_SIGNATURE, &payload(&folder), TEAM);
        let package = folder.join("libNDI_for_Mac.pkg");
        std::fs::write(&package, b"a stand-in package").unwrap();
        let none = AtomicBool::new(false);
        let contents = read(&tools, &package, &folder.join("scratch"), "aarch64", &none).unwrap();
        assert_eq!(contents.version, "6.4.0.2");
        assert_eq!(contents.licence, None, "NDI's package holds no licence text");
        let into = folder.join("NDI");
        install(&contents, &into).unwrap();
        let mut names: Vec<String> =
            std::fs::read_dir(&into).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, [RUNTIME_FILE, NOTICES_FILE]);
        assert_eq!(std::fs::read(into.join(RUNTIME_FILE)).unwrap(), library(100_000));
        // Changed after it was checked, it is not installed, and what was there stays.
        std::fs::write(&contents.runtime, library(100_001)).unwrap();
        assert!(matches!(install(&contents, &into), Err(Problem::Damaged(_))));
        assert_eq!(std::fs::read(into.join(RUNTIME_FILE)).unwrap(), library(100_000));
        assert!(!into.join(format!(".{RUNTIME_FILE}.part")).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_package_or_library_not_signed_by_ndi_is_refused_before_anything_is_taken() {
        let none = AtomicBool::new(false);
        // Another team's package: refused before it is even laid out.
        let folder_a = folder("other-team");
        let other = NDIS_SIGNATURE.replace("W8U66ET244", "ABCDE12345");
        let tools = stand_ins(&folder_a, &other, &payload(&folder_a), TEAM);
        let package = folder_a.join("x.pkg");
        std::fs::write(&package, b"x").unwrap();
        let problem = read(&tools, &package, &folder_a.join("scratch"), "aarch64", &none).err().unwrap();
        assert!(matches!(problem, Problem::Unsigned(_)), "{problem:?}");
        assert!(!folder_a.join("scratch/expanded").exists());
        // NDI's package, but a library some other team signed.
        let folder_b = folder("other-library");
        let tools = stand_ins(&folder_b, NDIS_SIGNATURE, &payload(&folder_b), "ABCDE12345");
        let problem = read(&tools, &package, &folder_b.join("scratch"), "aarch64", &none).err().unwrap();
        assert!(matches!(problem, Problem::Unsigned(_)), "{problem:?}");
        // A library that is only a link to one elsewhere is never followed: none is found.
        let folder_c = folder("link");
        let laid = payload(&folder_c);
        let inside = laid.join("libNDIComponent.pkg/Payload").join(RUNTIME_FILE);
        std::fs::rename(&inside, folder_c.join("elsewhere.dylib")).unwrap();
        std::os::unix::fs::symlink(folder_c.join("elsewhere.dylib"), &inside).unwrap();
        let tools = stand_ins(&folder_c, NDIS_SIGNATURE, &laid, TEAM);
        let problem = read(&tools, &package, &folder_c.join("scratch"), "aarch64", &none).err().unwrap();
        assert!(matches!(problem, Problem::Layout(_)), "{problem:?}");
        // A library for another processor.
        let folder_d = folder("processor");
        let tools = stand_ins(&folder_d, NDIS_SIGNATURE, &payload(&folder_d), TEAM);
        let problem = read(&tools, &package, &folder_d.join("scratch"), "riscv64", &none).err().unwrap();
        assert_eq!(problem, Problem::NoRuntime("riscv64".to_owned()));
        for folder in [folder_a, folder_b, folder_c, folder_d] {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// With macOS's own tools and NDI's own package: `SOFTPRESENTER_NDI_MAC_PACKAGE=<the
    /// package> cargo test -p sp-app ndis_real_package -- --ignored --nocapture`, on a Mac.
    /// Installs nothing outside a folder of its own in the temporary folder.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs NDI's own package, downloaded from NDI by whoever runs it"]
    fn ndis_real_package_is_checked_laid_out_and_its_library_taken() {
        let package = PathBuf::from(std::env::var_os("SOFTPRESENTER_NDI_MAC_PACKAGE").expect("the package's path"));
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-mac-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("scratch")).unwrap();
        let none = AtomicBool::new(false);
        let contents =
            read(&Tools::system(), &package, &folder.join("scratch"), std::env::consts::ARCH, &none).unwrap();
        println!(
            "version {}; library {} bytes; notices: {}",
            contents.version,
            contents.runtime_size,
            contents.notices.is_some()
        );
        assert_eq!(contents.licence, None);
        let into = folder.join("NDI");
        install(&contents, &into).unwrap();
        let mut names: Vec<String> =
            std::fs::read_dir(&into).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        println!("installed: {names:?}");
        assert_eq!(names, [RUNTIME_FILE, NOTICES_FILE]);
        // A package that is not NDI's (this one with a byte changed) is refused.
        let mut bytes = std::fs::read(&package).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        let changed = folder.join("changed.pkg");
        std::fs::write(&changed, bytes).unwrap();
        std::fs::create_dir_all(folder.join("scratch2")).unwrap();
        let refused = read(&Tools::system(), &changed, &folder.join("scratch2"), std::env::consts::ARCH, &none);
        println!("a changed package: {refused:?}");
        assert!(refused.is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
