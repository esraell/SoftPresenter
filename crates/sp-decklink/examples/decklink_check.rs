//! Lists the DeckLink devices on this computer, their formats and keying, and with `play` sends
//! a test picture to one of them: `cargo run -p sp-decklink --example decklink_check -- help`.
//! The app has the same check as `softpresenter decklink-check`, for a computer without the Rust
//! tools.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sp_decklink::check::run(&args, "cargo run -p sp-decklink --example decklink_check --")
}
