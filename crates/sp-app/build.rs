//! Puts the app's icon into the Windows program, so the file, the taskbar and the window show
//! it. Needs `windres` (the mingw-w64 toolchain's, or a native GNU one); without it the program
//! is built without an icon and says so. Does nothing for any other system.

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

fn main() {
    list_locales();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icons/SoftPresenter.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu")
    {
        return;
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let icon = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons/SoftPresenter.ico");
    let script = out.join("softpresenter.rc");
    let object = out.join("softpresenter-icon.o");
    // Backslashes would be taken for escapes in the resource script.
    let icon = icon.to_string_lossy().replace('\\', "/");
    if std::fs::write(&script, format!("1 ICON \"{icon}\"\n")).is_err() {
        println!("cargo:warning=could not write the icon script; building without an icon");
        return;
    }
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
    println!("cargo:warning=windres was not found; building without an icon");
}
