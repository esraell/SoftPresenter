//! Compiles the bridge to the DeckLink driver, on a Mac and on Linux only. Windows' DeckLink
//! interface comes as an IDL file that needs Microsoft's MIDL compiler, so there the crate is
//! built without the bridge and behaves as on a computer without the driver.

fn main() {
    println!("cargo::rerun-if-changed=bridge/bridge.cpp");
    println!("cargo::rerun-if-changed=build.rs");
    // The system being built for, not the one building: a check for Windows made on a Mac
    // must not compile the bridge.
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let headers = match target.as_str() {
        "macos" => "Mac",
        "linux" => "Linux",
        _ => return,
    };
    let include = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/decklink-sdk")
        .join(headers)
        .join("include");
    println!("cargo::rerun-if-changed={}", include.display());
    cc::Build::new().cpp(true).std("c++17").include(&include).file("bridge/bridge.cpp").compile("sp_decklink_bridge");
    if target == "macos" {
        println!("cargo::rustc-link-lib=framework=CoreFoundation");
    } else {
        println!("cargo::rustc-link-lib=dylib=dl");
        println!("cargo::rustc-link-lib=dylib=pthread");
    }
}
