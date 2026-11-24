//! Puts the app's icon into the Windows program, so the file, the taskbar and the window show
//! it. Needs `windres` (the mingw-w64 toolchain's, or a native GNU one); without it the program
//! is built without an icon and says so. Does nothing for any other system.

use std::path::PathBuf;
use std::process::Command;

fn main() {
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
