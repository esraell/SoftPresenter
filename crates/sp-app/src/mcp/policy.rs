//! Which commands an AI assistant may send through `sp mcp`, and what each needs. The server
//! sends the same commands `sp` does, through the same handler (`control.rs`), but `sp` is the
//! computer's own user, and the assistant is a program that reads song words, playlist names and
//! anything else it is given, any of which may be written to steer it. So the app decides, here,
//! not the server or the assistant:
//!
//! - nothing at all unless Settings › Integrations › AI assistants lets assistants in;
//! - reading, and making songs and playlists, once it does;
//! - taking something out only after the person said yes to the server's question;
//! - changing the screens only with the second switch on too, and after a yes.
//!
//! Default deny, by exact shape: only the commands, with the number of words and the flags the
//! server sends, are open; any other (a command added to `sp` later among them) is refused until
//! it is looked at and listed here. Each shape is read as its handler goes on to read it, so a
//! name or a reference (one word, whatever it holds) can never become a flag, a sub-command or a
//! file: a word in a place for names that starts with `--` is refused, and the files a handler
//! would look for are closed while the command runs (`App::outside_call`).

use crate::control::McpCall;

/// What a command does, and so what it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Reads, and changes nothing the operator sees.
    Reads,
    /// Makes or adds to songs and playlists, or downloads a Bible offered.
    Prepares,
    /// Takes something out (an item of a playlist; `sp undo` puts it back).
    Removes,
    /// Changes what the audience screens show.
    Live,
}

/// The switches of Settings › Integrations › AI assistants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Switches {
    /// Let AI assistants use SoftPresenter: read, and make songs and playlists.
    #[serde(default)]
    pub allowed: bool,
    /// Let them change what's on the screens (each time after a yes).
    #[serde(default)]
    pub screens: bool,
}

/// The refusal for everything while assistants are switched off.
pub const SWITCHED_OFF: &str = "AI assistants are switched off in SoftPresenter. To let them in, open SoftPresenter's Settings › Integrations › AI assistants and switch on \u{201c}Let AI assistants use SoftPresenter\u{201d}.";

/// The refusal for a change to the screens while the second switch is off.
pub const SCREENS_OFF: &str = "SoftPresenter doesn't let AI assistants change what's on the screens. To allow it, switch on \u{201c}Let them change what's on the screens\u{201d} in SoftPresenter's Settings › Integrations › AI assistants (the assistant still asks you each time).";

/// The refusal for a change to the screens or a removal the person hasn't said yes to.
pub const NO_YES: &str =
    "This needs a yes from the person at the computer first, and it wasn't asked; nothing was done.";

/// Why the app refuses a command from an assistant, or what it is: `decide` with the switches
/// and what the server said about the request.
pub fn decide(switches: Switches, args: &[String], call: &McpCall) -> Result<Kind, String> {
    if !switches.allowed {
        return Err(SWITCHED_OFF.to_owned());
    }
    let kind = kind_of(args)?;
    match kind {
        Kind::Live if !switches.screens => Err(SCREENS_OFF.to_owned()),
        Kind::Live | Kind::Removes if !call.asked => Err(NO_YES.to_owned()),
        _ => Ok(kind),
    }
}

/// A short word for the log: why a request was refused (never its words).
pub fn refusal_word(why: &str) -> &'static str {
    match why {
        SWITCHED_OFF => "assistants switched off",
        SCREENS_OFF => "screens switch off",
        NO_YES => "no yes",
        _ => "not open to assistants",
    }
}

/// The sub-commands of `bible` (a word after it that is none of these is a reference).
const BIBLE_WORDS: &[&str] =
    &["search", "rows", "options", "history", "install", "remove", "view", "books", "language", "text"];

/// The layers `clear` takes.
pub const LAYERS: &[&str] = &["all", "slide", "media", "props", "messages", "audio", "announcements"];

/// What a command is, by its exact shape; a refusal, in words for the assistant, for any shape
/// not open to assistants.
pub fn kind_of(args: &[String]) -> Result<Kind, String> {
    let word = |at: usize| args.get(at).map(|w| w.as_str()).unwrap_or_default();
    // A name, title, reference or words: one word, which is never empty and never a flag.
    let name = |at: usize| args.get(at).is_some_and(|w| !w.trim().is_empty() && !w.trim_start().starts_with("--"));
    let number = |at: usize| args.get(at).is_some_and(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit()));
    let count = args.len();
    let shape = match (word(0), count) {
        // Reading.
        ("status" | "screens" | "songs" | "playlists" | "media" | "messages" | "bibles", 1) => Some(Kind::Reads),
        ("job", 2) if number(1) => Some(Kind::Reads),
        ("search", 2) if name(1) => Some(Kind::Reads),
        ("song", 3) if matches!(word(1), "text" | "info") && name(2) => Some(Kind::Reads),
        ("bible", 3) if matches!(word(1), "text" | "search") && name(2) => Some(Kind::Reads),
        // A song's slides, or a playlist item's (whatever the words, `slides` only reads).
        ("slides", 2) if name(1) && !word(1).trim().eq_ignore_ascii_case("item") => Some(Kind::Reads),
        ("slides", 5) if word(1) == "item" && number(2) && word(3) == "in" && name(4) => Some(Kind::Reads),
        ("playlist", 1) => Some(Kind::Reads),
        // `playlist NAME`, read only when the name isn't one of the words that change one.
        ("playlist", 2) if name(1) && !changes_a_playlist(word(1)) => Some(Kind::Reads),
        // Making songs and playlists.
        ("song", 5) if word(1) == "new" && name(2) && word(3) == "--words" && name(4) => Some(Kind::Prepares),
        ("playlist", 3) if word(1) == "new" && name(2) => Some(Kind::Prepares),
        ("playlist", 5) if word(1) == "add" && name(2) && word(3) == "to" && name(4) => Some(Kind::Prepares),
        ("playlist", 5)
            if matches!(word(1), "header" | "media" | "placeholder" | "passage")
                && name(2)
                && word(3) == "in"
                && name(4) =>
        {
            Some(Kind::Prepares)
        }
        // A Bible SoftPresenter offers, by its name alone: never a file.
        ("bible", 3) if word(1) == "install" && offered_bible(word(2)) => Some(Kind::Prepares),
        // Taking an item out of a playlist.
        ("playlist", 5) if word(1) == "remove" && name(2) && word(3) == "from" && name(4) => Some(Kind::Removes),
        // The screens.
        ("next" | "previous", 1) => Some(Kind::Live),
        ("clear", 2) if LAYERS.contains(&word(1)) => Some(Kind::Live),
        ("go", 2) if name(1) => Some(Kind::Live),
        ("go", 3) if name(1) && number(2) => Some(Kind::Live),
        ("playlist", 5) if word(1) == "go" && name(2) && word(3) == "in" && name(4) => Some(Kind::Live),
        ("playlist", 6) if word(1) == "go" && name(2) && number(3) && word(4) == "in" && name(5) => Some(Kind::Live),
        // `bible REFERENCE --show`: the reference is never one of `bible`'s own words.
        ("bible", 3)
            if name(1) && !BIBLE_WORDS.contains(&word(1).trim().to_lowercase().as_str()) && word(2) == "--show" =>
        {
            Some(Kind::Live)
        }
        ("message", 3) if word(1) == "show" && name(2) => Some(Kind::Live),
        ("message", 4) if word(1) == "show" && word(2) == "--text" && name(3) => Some(Kind::Live),
        _ => None,
    };
    shape.ok_or_else(|| refusal(args))
}

/// Whether a playlist's name, typed after `playlist`, would be read as one of the words that
/// change a playlist (`playlist new`, `playlist go`…).
fn changes_a_playlist(typed: &str) -> bool {
    let typed = typed.trim().to_lowercase();
    crate::control::PLAYLIST_CHANGES.contains(&typed.as_str())
}

/// Whether a Bible's name is one SoftPresenter offers to download (`bibles` lists them).
fn offered_bible(typed: &str) -> bool {
    crate::bible::OFFERED.iter().any(|b| b.short.eq_ignore_ascii_case(typed) || b.id.eq_ignore_ascii_case(typed))
}

/// Why a command an assistant may not send is refused, in words for it.
fn refusal(args: &[String]) -> String {
    let first = args.first().map(|w| w.to_lowercase()).unwrap_or_default();
    let second = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
    let at_computer = |what: &str| format!("{what} at the computer, never by an AI assistant.");
    match (first.as_str(), second.as_str()) {
        ("", _) => "Which command?".to_owned(),
        ("menu", _) => at_computer("Quitting SoftPresenter and choosing from the menu bar are done"),
        ("settings" | "screen" | "theme" | "version" | "audience" | "stage" | "identify" | "test", _) => {
            at_computer("Settings, the screens' setup and themes are changed")
        }
        ("remote", _) => at_computer("Remote Control and its password are set"),
        ("voxlive" | "canva" | "timelychurch" | "catalog", _) => {
            at_computer("Connections to other services, and their keys and sign-ins, are made")
        }
        ("song" | "library" | "playlist" | "bible" | "media" | "media-playlist", "delete" | "remove")
            if !(first == "playlist" && second == "remove") =>
        {
            at_computer("Deleting songs, libraries, playlists, Bibles and media is done")
        }
        ("song", "import" | "export") | ("media", "add" | "samples") | ("audio", "add") | ("prop", "new") => {
            at_computer("Files on the computer are opened")
        }
        ("slide", "web") => at_computer("Web pages are put on slides"),
        _ => format!(
            "AI assistants can't use \u{201c}{}\u{201d} in SoftPresenter: only the shapes its tools send are open to them.",
            args.iter().take(2).cloned().collect::<Vec<_>>().join(" ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| (*w).to_owned()).collect()
    }

    fn on() -> Switches {
        Switches { allowed: true, screens: true }
    }

    fn yes() -> McpCall {
        McpCall { tool: "test".to_owned(), asked: true, check: false }
    }

    fn not_asked() -> McpCall {
        McpCall { tool: "test".to_owned(), asked: false, check: false }
    }

    #[test]
    fn nothing_is_open_while_assistants_are_switched_off() {
        let off = Switches { allowed: false, screens: true };
        for shape in [&["status"][..], &["songs"], &["next"], &["playlist", "new", "Sunday"]] {
            assert_eq!(decide(off, &words(shape), &yes()), Err(SWITCHED_OFF.to_owned()), "{shape:?}");
        }
    }

    #[test]
    fn the_screens_need_their_switch_and_a_yes_and_removing_needs_a_yes() {
        let reading = Switches { allowed: true, screens: false };
        assert_eq!(decide(reading, &words(&["next"]), &yes()), Err(SCREENS_OFF.to_owned()));
        assert_eq!(decide(reading, &words(&["status"]), &not_asked()), Ok(Kind::Reads));
        assert_eq!(decide(on(), &words(&["next"]), &not_asked()), Err(NO_YES.to_owned()));
        assert_eq!(decide(on(), &words(&["next"]), &yes()), Ok(Kind::Live));
        let remove = words(&["playlist", "remove", "3", "from", "Sunday"]);
        assert_eq!(decide(reading, &remove, &not_asked()), Err(NO_YES.to_owned()));
        assert_eq!(decide(reading, &remove, &yes()), Ok(Kind::Removes));
        // Making things needs neither.
        let make = words(&["playlist", "new", "Sunday"]);
        assert_eq!(decide(reading, &make, &not_asked()), Ok(Kind::Prepares));
    }

    #[test]
    fn each_shape_the_tools_send_has_its_kind() {
        let cases: &[(&[&str], Kind)] = &[
            (&["status"], Kind::Reads),
            (&["screens"], Kind::Reads),
            (&["songs"], Kind::Reads),
            (&["search", "grace"], Kind::Reads),
            (&["song", "text", "Amazing Grace"], Kind::Reads),
            (&["song", "info", "e6985576-8dfd-44d8-b81c-99a7dc6a9b54"], Kind::Reads),
            (&["bible", "text", "John 3:16-18"], Kind::Reads),
            (&["bible", "search", "valley of the shadow"], Kind::Reads),
            (&["bibles"], Kind::Reads),
            (&["playlists"], Kind::Reads),
            (&["playlist"], Kind::Reads),
            (&["playlist", "2"], Kind::Reads),
            (&["slides", "e6985576-8dfd-44d8-b81c-99a7dc6a9b54"], Kind::Reads),
            (&["slides", "item", "3", "in", "Sunday"], Kind::Reads),
            (&["playlist", "Sunday Evening"], Kind::Reads),
            (&["media"], Kind::Reads),
            (&["messages"], Kind::Reads),
            (&["job", "4"], Kind::Reads),
            (&["song", "new", "River Song", "--words", "[Verse 1]\nDown by the river"], Kind::Prepares),
            (&["playlist", "new", "Sunday"], Kind::Prepares),
            (&["playlist", "add", "River Song", "to", "Sunday"], Kind::Prepares),
            (&["playlist", "header", "Worship", "in", "Sunday"], Kind::Prepares),
            (&["playlist", "media", "Bars.mp4", "in", "Sunday"], Kind::Prepares),
            (&["playlist", "placeholder", "Sermon slides", "in", "Sunday"], Kind::Prepares),
            (&["playlist", "passage", "John 3:16", "in", "Sunday"], Kind::Prepares),
            (&["bible", "install", "ASV"], Kind::Prepares),
            (&["playlist", "remove", "7d2c6f2e-0000-4000-8000-000000000001", "from", "Sunday"], Kind::Removes),
            (&["next"], Kind::Live),
            (&["previous"], Kind::Live),
            (&["clear", "all"], Kind::Live),
            (&["clear", "messages"], Kind::Live),
            (&["go", "e6985576-8dfd-44d8-b81c-99a7dc6a9b54", "2"], Kind::Live),
            (&["playlist", "go", "7d2c6f2e-0000-4000-8000-000000000001", "in", "Sunday"], Kind::Live),
            (&["playlist", "go", "7d2c6f2e-0000-4000-8000-000000000001", "3", "in", "Sunday"], Kind::Live),
            (&["bible", "John 3:16", "--show"], Kind::Live),
            (&["message", "show", "Parking"], Kind::Live),
            (&["message", "show", "--text", "Welcome, everyone"], Kind::Live),
        ];
        for (shape, kind) in cases {
            assert_eq!(kind_of(&words(shape)), Ok(*kind), "{shape:?}");
        }
    }

    /// A name, title or reference is one word in its place; whatever it holds, it is never read
    /// as a flag, a sub-command or a file, and a reading or making shape never becomes one that
    /// changes the screens or takes something out.
    #[test]
    fn hostile_names_never_turn_a_shape_into_another() {
        for hostile in ["--yes", "--aim", "--show", "--library", "--now", "--wait", " --yes", ""] {
            for shape in [
                &["search", hostile][..],
                &["song", "text", hostile],
                &["song", "info", hostile],
                &["bible", "text", hostile],
                &["playlist", hostile],
                &["song", "new", hostile, "--words", "[Verse 1]\nWords"],
                &["song", "new", "Title", "--words", hostile],
                &["playlist", "new", hostile],
                &["playlist", "add", hostile, "to", "Sunday"],
                &["playlist", "add", "River Song", "to", hostile],
                &["go", hostile],
                &["message", "show", hostile],
                &["bible", hostile, "--show"],
            ] {
                assert!(kind_of(&words(shape)).is_err(), "{shape:?} should be refused");
            }
        }
        // A playlist's name that is one of the words that change one is not read as a name.
        for word in ["new", "go", "remove", "delete", "add", "passage", "GO", " go "] {
            assert!(kind_of(&words(&["playlist", word])).is_err(), "playlist {word}");
        }
        // A reference that is one of `bible`'s own words is never shown as a passage.
        for word in ["remove", "install", "search", "TEXT"] {
            assert!(kind_of(&words(&["bible", word, "--show"])).is_err(), "bible {word} --show");
        }
        // Words with spaces, keywords or quotes in one word stay one name.
        for name in ["O Come to the Altar", "to", "in", "from", "at", "\u{201c}Amazing\u{201d} Grace", "3; clear all"] {
            assert_eq!(kind_of(&words(&["playlist", "add", name, "to", "Sunday"])), Ok(Kind::Prepares), "{name}");
            assert_eq!(kind_of(&words(&["search", name])), Ok(Kind::Reads), "{name}");
        }
    }

    #[test]
    fn extra_words_or_flags_close_a_shape() {
        for shape in [
            &["status", "--wait", "5"][..],
            &["song", "text", "Amazing", "/etc/hosts"],
            &["song", "info", "Amazing", "ccli=1"],
            &["song", "new", "Title", "/etc/hosts"],
            &["song", "new", "Title"],
            &["song", "new", "Title", "--words", "x", "--library", "Hymns"],
            &["song", "--library", "Hymns", "new", "Title", "--words", "x"],
            &["playlist", "add", "River Song"],
            &["playlist", "remove", "3"],
            &["playlist", "go", "3"],
            &["playlist", "go", "3", "1", "in", "Sunday", "extra"],
            &["go", "Amazing", "--aim", "2"],
            &["go", "--aim", "2"],
            &["go", "Amazing", "two"],
            &["next", "--aim", "3"],
            &["clear"],
            &["clear", "everything"],
            &["bible", "John 3:16", "--add"],
            &["bible", "John 3:16", "--show", "--add"],
            &["bible", "install", "/tmp/bible.xml"],
            &["bible", "install", "ASV", "--name", "x"],
            &["message", "show", "--text"],
            &["job", "x"],
        ] {
            assert!(kind_of(&words(shape)).is_err(), "{shape:?} should be refused");
        }
    }

    #[test]
    fn what_is_never_open_says_why() {
        let said = |shape: &[&str]| kind_of(&words(shape)).unwrap_err();
        assert!(said(&["menu", "Quit"]).contains("menu bar"));
        assert!(said(&["menu"]).contains("menu bar"));
        assert!(said(&["settings", "set", "continuous=on"]).contains("Settings"));
        assert!(said(&["remote", "password"]).contains("Remote Control"));
        assert!(said(&["timelychurch", "connect"]).contains("Connections"));
        assert!(said(&["voxlive", "connect"]).contains("Connections"));
        assert!(said(&["song", "delete", "Amazing", "--yes"]).contains("Deleting"));
        assert!(said(&["library", "delete", "Hymns"]).contains("Deleting"));
        assert!(said(&["theme", "delete", "Evening", "--yes"]).contains("themes"));
        assert!(said(&["playlist", "delete", "Sunday"]).contains("Deleting"));
        assert!(said(&["bible", "remove", "KJV", "--yes"]).contains("Deleting"));
        assert!(said(&["song", "import", "/etc/hosts"]).contains("Files"));
        assert!(said(&["song", "export", "Amazing", "--to", "/tmp/x"]).contains("Files"));
        assert!(said(&["slide", "web", "Amazing", "1", "https://example.org"]).contains("Web pages"));
        assert!(said(&["undo"]).contains("only the shapes"));
    }

    /// Every command `control.rs` knows is closed to assistants unless one of its shapes is
    /// listed above: a new command stays closed until it is looked at here.
    #[test]
    fn a_command_the_app_knows_is_closed_unless_a_shape_of_it_is_listed() {
        let source = include_str!("../control.rs");
        let from = source.find("let result = match word.as_str() {").expect("the command match");
        let to = from + source[from..].find("other => Err(").expect("the end of the match");
        let mut known = Vec::new();
        for line in source[from..to].lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            let arm = line.split("=>").next().unwrap_or("");
            let arm = arm.split(" if ").next().unwrap_or("");
            for part in arm.split('|') {
                let word = part.trim().trim_matches('"');
                if !word.is_empty() && part.trim().starts_with('"') {
                    known.push(word.to_owned());
                }
            }
        }
        assert!(known.len() > 40, "found only {known:?}");
        // The commands with a shape open to assistants; each of the others is refused however
        // it is worded (alone, or with a word after it).
        let listed = [
            "status",
            "screens",
            "songs",
            "playlists",
            "media",
            "messages",
            "bibles",
            "job",
            "search",
            "song",
            "bible",
            "playlist",
            "next",
            "previous",
            "clear",
            "go",
            "message",
            "slides",
        ];
        for word in known {
            if listed.contains(&word.as_str()) {
                continue;
            }
            for shape in [vec![word.clone()], vec![word.clone(), "x".to_owned()], vec![word.clone(), "on".to_owned()]] {
                assert!(kind_of(&shape).is_err(), "{shape:?} is open to assistants without being listed");
            }
        }
    }
}
