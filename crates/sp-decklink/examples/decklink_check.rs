//! Lists the DeckLink devices on this computer, their modes and keying:
//! `cargo run -p sp-decklink --example decklink_check`. The app has the same check as
//! `softpresenter decklink-check`, for a computer without the Rust tools.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sp_decklink::check::run(&args, "cargo run -p sp-decklink --example decklink_check --")
}
