//! Puts the app's icon and its manifest into the Windows program. The manifest asks Windows for
//! version 6 of its common controls, which the app's questions need: rfd shows them with
//! TaskDialogIndirect (its `common-controls-v6` feature), so they carry the app's own button
//! words ("A Folder…", "Files…") rather than Yes, No and Cancel, and that function exists only in
//! version 6. Without the manifest Windows wouldn't start the program at all, so it is never left
//! out: with the GNU toolchain both go in through `windres` (the mingw-w64 toolchain's, or a native
//! GNU one), and the build stops if there is none; with Microsoft's linker the manifest is handed
//! to it to embed (the icon isn't put in there yet). Does nothing for any other system.

use std::path::PathBuf;
use std::process::Command;

/// Lists the translation tables, `locales/<language>/<area>.json`, as a generated file that
/// `include_str!`s each one, so adding an area's table never means editing code (the `i18n`
/// module includes the result).
fn list_locales() {
    println!("cargo:rerun-if-changed=locales");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut rows = Vec::new();
    let mut languages: Vec<PathBuf> = std::fs::read_dir(manifest.join("locales"))
        .map(|dir| dir.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect())
        .unwrap_or_default();
    languages.sort();
    for language in languages {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&language)
            .map(|dir| dir.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        files.sort();
        for file in files.iter().filter(|file| file.extension().is_some_and(|ext| ext == "json")) {
            println!("cargo:rerun-if-changed={}", file.display());
            let code = language.file_name().unwrap_or_default().to_string_lossy();
            let area = file.file_stem().unwrap_or_default().to_string_lossy();
            let path = file.to_string_lossy().replace('\\', "/");
            rows.push(format!("    ({code:?}, {area:?}, include_str!({path:?})),\n"));
        }
    }
    let text = format!("pub const FILES: &[(&str, &str, &str)] = &[\n{}];\n", rows.concat());
    std::fs::write(out.join("locales.rs"), text).expect("write locales.rs");
}

/// The program's manifest: version 6 of the common controls, and nothing else (the window
/// library sets the program's DPI awareness itself, as it starts).
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
    list_locales();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icons/SoftPresenter.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let manifest = out.join("softpresenter.manifest");
    std::fs::write(&manifest, MANIFEST).expect("write the program's manifest");
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        // As Microsoft's linker takes a manifest: embedded, from this file, without the one it
        // would write itself (which says nothing this needs).
        Ok("msvc") => {
            let manifest = manifest.canonicalize().expect("the manifest just written");
            println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
            println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
            println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        }
        Ok("gnu") => resources_with_windres(&out, &manifest),
        other => panic!("no way to put the manifest into a Windows program built with {other:?}"),
    }
}

/// The icon and the manifest, as a resource object made by `windres` and linked into the program.
fn resources_with_windres(out: &std::path::Path, manifest: &std::path::Path) {
    let icon = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons/SoftPresenter.ico");
    let script = out.join("softpresenter.rc");
    let object = out.join("softpresenter-icon.o");
    // Backslashes would be taken for escapes in the resource script.
    let icon = icon.to_string_lossy().replace('\\', "/");
    let manifest = manifest.to_string_lossy().replace('\\', "/");
    // 1 ICON: the program's icon. 1 24: its manifest (CREATEPROCESS_MANIFEST_RESOURCE_ID,
    // RT_MANIFEST), which Windows reads as it starts the program.
    std::fs::write(&script, format!("1 ICON \"{icon}\"\n1 24 \"{manifest}\"\n")).expect("write the resource script");
    let tools = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64") {
        ["x86_64-w64-mingw32-windres", "windres"]
    } else {
        ["i686-w64-mingw32-windres", "windres"]
    };
    for tool in tools {
        let done = Command::new(tool).arg("-i").arg(&script).arg("-o").arg(&object).arg("-O").arg("coff").status();
        if done.is_ok_and(|status| status.success()) {
            println!("cargo:rustc-link-arg-bins={}", object.display());
            return;
        }
    }
    panic!(
        "windres was not found: it puts the manifest into the Windows program, without which Windows won't start it \
         (install the mingw-w64 toolchain)"
    );
}
