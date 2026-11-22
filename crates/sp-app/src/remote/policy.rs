//! Which commands a remote page may send. The page sends the same commands `sp` does, through
//! the same handler (`control.rs`), but `sp` is the computer's own user, who could change the
//! app's files anyway, and the page is anyone on the church's network who knows the password.
//! So the page runs the show, and nothing that reaches the computer's files, the services'
//! keys, the menu bar or Remote Control itself.
//!
//! Default deny: a command whose first word isn't listed here is refused, so a new command
//! stays closed to the page until it is looked at and listed (a test reads `control.rs` for
//! every command it knows and fails while one isn't). Within the commands listed, the shapes
//! that name a file or folder, take a key, or wait on the main thread are refused by their
//! words alone: never by looking at the disk, which would tell the page what is there.

/// What a signed-in page may do. Only Control so far; a view-only page, and others, come later
/// as more arms of `refusal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Runs the show: everything `sp` can do but the refused shapes below.
    Control,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::Control => "control",
        }
    }
}

/// The first words of the commands open to Control. Every command `control.rs` knows is either
/// here or in [`CLOSED`].
pub const OPEN: &[&str] = &[
    "help",
    "status",
    "songs",
    "slides",
    "slide",
    "playlists",
    "libraries",
    "library",
    "playlist",
    "media",
    "media-playlist",
    "voxlive",
    "canva",
    "themes",
    "screens",
    "timers",
    "messages",
    "props",
    "macros",
    "audio",
    "next",
    "previous",
    "prev",
    "back",
    "first",
    "last",
    "go",
    "select",
    "clear",
    "audience",
    "stage",
    "identify",
    "view",
    "message",
    "prop",
    "timer",
    "macro",
    "announce",
    "bible",
    "song",
    "job",
    "search",
    "group",
    "hotkey",
    "continuous",
    "video",
    "test",
    "bibles",
    "screen",
    "settings",
    "groups",
    "displays",
    "decklink",
    "theme",
    "version",
    "undo",
    "redo",
    "menu",
];

/// Commands never open to a page: Remote Control's own (switching it, its password).
pub const CLOSED: &[&str] = &["remote"];

/// Why a page in this role may not send this command, in words to show it; `None` when it may.
pub fn refusal(role: Role, args: &[String]) -> Option<String> {
    match role {
        Role::Control => control_refusal(args),
    }
}

fn control_refusal(args: &[String]) -> Option<String> {
    let word = |at: usize| args.get(at).map(|w| w.to_lowercase()).unwrap_or_default();
    let first = word(0);
    let second = word(1);
    let after = |at: usize| args.len().saturating_sub(at);
    let at_computer = |what: &str| Some(format!("{what} is done at the computer, not from the remote page."));
    if first.is_empty() {
        return Some("Which command?".to_owned());
    }
    if CLOSED.contains(&first.as_str()) || !OPEN.contains(&first.as_str()) {
        return Some(format!("The remote page can't use the command \u{201c}{}\u{201d}.", args[0]));
    }
    match (first.as_str(), second.as_str()) {
        // The page is sent every change as it happens; a wait would hold a place on the app's
        // main thread for nothing.
        ("status", _) if args.iter().any(|w| w == "--wait") => {
            Some("The remote page is sent each change as it happens; it doesn't wait for one.".to_owned())
        }
        // The menu bar's items include Quit and the ones that open the computer's own windows.
        ("menu", _) if args.len() > 1 => at_computer("Choosing from the menu bar"),
        // Keys and sign-ins for services.
        ("voxlive", "connect" | "disconnect") => at_computer("Connecting VoxLive"),
        ("canva", "connect" | "import" | "disconnect") => at_computer("Signing in to Canva"),
        // Files and folders on the computer: reading them in, writing them out, following them.
        ("song", "import") => at_computer("Importing songs"),
        ("song", "export") => at_computer("Exporting a song pack"),
        ("media", "add") => at_computer("Adding pictures and videos"),
        ("audio", "add") => at_computer("Adding audio tracks"),
        ("media-playlist", "follow") if !(args.len() == 4 && word(3) == "off") => at_computer("Following a folder"),
        ("bible", "install") if !(args.len() == 3 && offered_bible(&args[2])) => {
            at_computer("Installing a Bible from a file")
        }
        // A title, name or song given as one word (spaces and all) is never read as a file; more
        // words than that could end in one (`song new TITLE FILE.txt`, `prop new NAME PICTURE`,
        // `song text SONG FILE`).
        ("song", "new" | "text") | ("prop", "new") if after(2) > 1 => Some(format!(
            "From the remote page, give the {} as one word (quoted, spaces and all): {} {} \u{201c}…\u{201d}",
            if second == "text" { "song" } else { "name" },
            first,
            second
        )),
        // A web page on a slide: https only. Plain http reaches only this computer's own
        // services, which the page has no business putting on screen.
        ("slide", "web") if args.last().is_some_and(|address| !web_address_open(address)) => {
            Some("From the remote page, a web page on a slide must be an https:// address.".to_owned())
        }
        _ => None,
    }
}

/// Whether a Bible's name is one SoftPresenter offers to download (`bibles` lists them).
fn offered_bible(typed: &str) -> bool {
    crate::bible::OFFERED.iter().any(|b| b.short.eq_ignore_ascii_case(typed) || b.id.eq_ignore_ascii_case(typed))
}

/// Whether a slide's web address may be set from a page: `off`, a bare host (made https), or
/// https.
fn web_address_open(address: &str) -> bool {
    let address = address.trim();
    if address.eq_ignore_ascii_case("off") || address.eq_ignore_ascii_case("none") {
        return true;
    }
    match address.split_once("://") {
        Some((scheme, _)) => scheme.eq_ignore_ascii_case("https"),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn refused(line: &str) -> bool {
        refusal(Role::Control, &words(line)).is_some()
    }

    #[test]
    fn running_the_show_is_open() {
        for line in [
            "status",
            "next",
            "next --aim 3 --rev 00ff",
            "go --aim 2 4",
            "go Amazing 3",
            "clear all",
            "playlist go 2 1",
            "playlist add Amazing",
            "timer start Walk",
            "message show Welcome",
            "slides item 2",
            "songs",
            "song new Title",
            "song text Amazing",
            "prop new Banner",
            "bible install ASV",
            "bible John 3:16 --show",
            "media-playlist follow Loop off",
            "slide web Amazing 2 https://example.org",
            "slide web Amazing 2 example.org",
            "slide web Amazing 2 off",
            "menu",
            "voxlive",
            "canva sync",
            "undo",
        ] {
            assert!(!refused(line), "{line} should be open");
        }
    }

    #[test]
    fn files_keys_the_menu_and_waits_are_closed() {
        for line in [
            "song import /etc/hosts",
            "song export Amazing --to /tmp/x.sppack",
            "song new Title /etc/hosts",
            "song new Two words",
            "song text Amazing /etc/hosts",
            "prop new Banner /tmp/x.png",
            "media add /tmp/x.png",
            "audio add /tmp/x.mp3",
            "media-playlist follow Loop /Users",
            "bible install /tmp/bible.xml",
            "bible install ASV --name x",
            "voxlive connect KEY",
            "voxlive disconnect --yes",
            "canva connect ID SECRET",
            "canva import",
            "canva disconnect --yes",
            "menu Quit",
            "status --wait 5",
            "slide web Amazing 2 http://127.0.0.1:8080",
            "slide web Amazing 2 file:///etc/hosts",
            "remote off",
            "remote password",
            "install-cli",
            "",
        ] {
            assert!(refused(line), "{line} should be refused");
        }
    }

    #[test]
    fn a_title_sent_as_one_word_is_never_a_file() {
        let args = vec!["song".to_owned(), "new".to_owned(), "Two words /etc/hosts".to_owned()];
        assert_eq!(refusal(Role::Control, &args), None);
    }

    /// Every command `control.rs` dispatches is either open or closed to the page, so a new
    /// one is refused until it is looked at here.
    #[test]
    fn every_command_the_app_knows_is_open_or_closed() {
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
        for word in known {
            assert!(
                OPEN.contains(&word.as_str()) || CLOSED.contains(&word.as_str()),
                "{word} is neither open nor closed to the remote page"
            );
        }
    }

    /// Each command line `sp help` shows with a FILE, FOLDER or PICTURE in it is refused with
    /// a path in that place.
    #[test]
    fn every_help_line_that_names_a_file_is_refused() {
        let mut checked = 0;
        for line in crate::control::HELP.lines() {
            let shape = line.trim_start();
            if !line.starts_with("  ") || line.starts_with("   ") {
                continue;
            }
            let shape = shape.split("   ").next().unwrap_or("");
            for one in shape.split(" | ") {
                if !(one.contains("FILE") || one.contains("FOLDER") || one.contains("PICTURE")) {
                    continue;
                }
                let filled: Vec<String> = one
                    .split_whitespace()
                    .filter(|w| !w.starts_with('['))
                    .map(|w| match w.trim_end_matches(['…', ']']) {
                        w if w.contains("FILE") || w.contains("FOLDER") => "/etc/hosts".to_owned(),
                        "PICTURE" => "/tmp/x.png".to_owned(),
                        "SONG" | "TITLE" | "NAME" => "Amazing".to_owned(),
                        "N" => "2".to_owned(),
                        w if w.contains('|') => w.split('|').next().unwrap_or(w).to_owned(),
                        w => w.to_owned(),
                    })
                    .collect();
                // `song new TITLE [FILE.txt]` and the like: the file is in brackets, so add it.
                let mut filled = filled;
                if !filled.iter().any(|w| w.starts_with('/')) {
                    filled.push("/etc/hosts".to_owned());
                }
                checked += 1;
                assert!(refusal(Role::Control, &filled).is_some(), "{filled:?} (from \u{201c}{one}\u{201d}) is open");
            }
        }
        assert!(checked >= 6, "only {checked} help lines name a file");
    }
}
