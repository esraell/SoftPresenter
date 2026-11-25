//! What this program tells SoftPresenter: one JSON object on a line of standard error for each
//! thing that happens. Standard output carries pictures and nothing else.
//!
//! Text that came from a sender (a device's name, its model) is cleaned before it is said, and
//! no key is ever said at all.

use std::io::Write;

/// The longest a sender's name or model is said at.
pub const MAX_NAME_CHARS: usize = 64;

/// A sender's own words made safe to show: control characters and the marks that turn text
/// round or hide it are dropped, the rest is cut to a length. Empty words become `fallback`.
pub fn clean(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        })
        .take(MAX_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() { fallback.to_owned() } else { cleaned.to_owned() }
}

/// Says one event.
pub fn say(event: serde_json::Value) {
    let mut line = event.to_string();
    line.push('\n');
    let mut out = std::io::stderr().lock();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
}

/// A line for SoftPresenter's log: a step of the protocol, in plain words, with no key in it.
pub fn log(text: impl AsRef<str>) {
    say(serde_json::json!({ "event": "log", "text": text.as_ref() }));
}

/// Something the operator should know, in plain words.
pub fn problem(text: impl AsRef<str>) {
    say(serde_json::json!({ "event": "problem", "text": text.as_ref() }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_senders_name_is_cleaned_and_cut() {
        assert_eq!(clean("Anna\u{2019}s iPhone", "A device"), "Anna\u{2019}s iPhone");
        assert_eq!(clean("  Bob\r\n{\"event\":\"x\"}\u{0}\u{1b}[31m ", "A device"), "Bob{\"event\":\"x\"}[31m");
        assert_eq!(clean("\u{202E}enohPi\u{200B}", "A device"), "enohPi");
        assert_eq!(clean("", "A device"), "A device");
        assert_eq!(clean("\n\t \u{7f}", "A device"), "A device");
        assert_eq!(clean(&"x".repeat(5000), "A device").chars().count(), MAX_NAME_CHARS);
        assert_eq!(clean(&"\u{1F600}".repeat(100), "A device").chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn a_name_can_never_break_out_of_its_line() {
        let event = serde_json::json!({ "event": "asks", "name": clean("a\"}\n{\"event\":\"connected\"", "x") });
        let line = event.to_string();
        assert!(!line.contains('\n'));
        let back: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(back["event"], "asks");
    }
}
