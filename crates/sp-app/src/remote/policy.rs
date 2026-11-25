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
//!
//! A refusal is said in the window's language, as everything the app says to the page is.

use crate::i18n::{quoted, t, tf};

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
    "notices",
    "catalog",
];

/// Commands never open to a page: Remote Control's own (switching it, its password),
/// TimelyChurch's (its connection is made and kept at the computer), and the switch that lets
/// devices on the network show their screens.
pub const CLOSED: &[&str] = &["remote", "timelychurch", "mirror"];

/// For the open commands made of sub-commands, the sub-commands open to Control (`""` for the
/// command alone); any other is closed, so a sub-command added later, or one taking a key, a
/// link or a file, stays closed until it is looked at and listed. (`bible` is not here: a word
/// after it that isn't one of its sub-commands is a reference to look up.)
pub const OPEN_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("voxlive", &["", "screens", "show", "look", "refresh"]),
    ("canva", &["", "status", "sync", "pages", "divider"]),
    ("catalog", &["", "status", "languages", "search", "list", "add", "newer", "use-newer", "keep-mine"]),
    ("settings", &["", "set"]),
    ("screen", &["add", "set", "delete"]),
    (
        "song",
        &[
            "new",
            "delete",
            "rename",
            "duplicate",
            "theme",
            "transition",
            "info",
            "arrangements",
            "arrangement",
            "text",
            "reload",
            "keep",
        ],
    ),
    ("media", &["", "show", "delete", "next", "previous", "prev", "back", "set"]),
    // The page's status button mirrors the app's: the list, seen, Clear All, and a question's
    // own answers (which the app checks against the question).
    ("notices", &["", "seen", "clear", "answer"]),
];

/// The sub-commands of those that are closed on purpose, with why (for the test that checks
/// every sub-command `sp help` shows is one or the other).
#[cfg(test)]
pub const CLOSED_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("voxlive", &["connect", "disconnect"]),
    ("canva", &["connect", "import", "disconnect", "presentation"]),
    ("catalog", &["download", "check", "remove", "sizes"]),
    ("song", &["import", "export"]),
    ("media", &["add", "samples"]),
];

/// Why a page in this role may not send this command, in words to show it; `None` when it may.
pub fn refusal(role: Role, args: &[String]) -> Option<String> {
    let args = as_carried_out(args);
    match role {
        Role::Control => control_refusal(&args),
    }
}

/// A command's words as its handler goes on to read them, where a handler takes words out
/// before it looks at the rest: `song … --library NAME …` is carried out as `song …` without
/// that pair (as many times as it is there), so `song --library X import FILE` is an import.
fn as_carried_out(args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|w| w.eq_ignore_ascii_case("song")) {
        while let Some(at) = args.iter().position(|w| w == "--library") {
            let end = (at + 2).min(args.len());
            args.drain(at..end);
        }
    }
    args
}

/// What is done at the computer only, never from a remote page: refused by the policy's words,
/// and again where each would open a file (`App::only_at_the_computer`), saying so in the
/// window's language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtComputer {
    MenuBar,
    AudioTracks,
    FollowFolder,
    BibleFile,
    WebPage,
    Ndi,
    VoxLive,
    Canva,
    Catalog,
    DownloadSongs,
    CheckNewer,
    RemoveSongs,
    ImportSongs,
    ExportPack,
    AddMedia,
    Samples,
    SongWordsFile,
}

impl AtComputer {
    /// Why a page can't do it: a whole sentence, in the language set.
    pub fn words(self) -> &'static str {
        match self {
            AtComputer::MenuBar => t(
                "remote.at_computer.menu",
                "Choosing from the menu bar is done at the computer, not from the remote page.",
            ),
            AtComputer::AudioTracks => {
                t("remote.at_computer.audio", "Adding audio tracks is done at the computer, not from the remote page.")
            }
            AtComputer::FollowFolder => {
                t("remote.at_computer.folder", "Following a folder is done at the computer, not from the remote page.")
            }
            AtComputer::BibleFile => t(
                "remote.at_computer.bible",
                "Installing a Bible from a file is done at the computer, not from the remote page.",
            ),
            AtComputer::WebPage => t(
                "remote.at_computer.web",
                "Putting a web page on a slide is done at the computer, not from the remote page.",
            ),
            AtComputer::Ndi => t(
                "remote.at_computer.ndi",
                "Sending a screen over NDI is done at the computer, not from the remote page.",
            ),
            AtComputer::VoxLive => {
                t("remote.at_computer.voxlive", "Connecting VoxLive is done at the computer, not from the remote page.")
            }
            AtComputer::Canva => t(
                "remote.at_computer.canva",
                "Canva Sync's sign-in and presentation are set at the computer, not from the remote page.",
            ),
            AtComputer::Catalog => t(
                "remote.at_computer.catalog",
                "Downloading, checking for and removing catalog songs is done at the computer, not from the remote page.",
            ),
            AtComputer::DownloadSongs => {
                t("remote.at_computer.download", "Downloading songs is done at the computer, not from the remote page.")
            }
            AtComputer::CheckNewer => t(
                "remote.at_computer.newer",
                "Checking for newer songs is done at the computer, not from the remote page.",
            ),
            AtComputer::RemoveSongs => {
                t("remote.at_computer.remove", "Removing songs is done at the computer, not from the remote page.")
            }
            AtComputer::ImportSongs => {
                t("remote.at_computer.import", "Importing songs is done at the computer, not from the remote page.")
            }
            AtComputer::ExportPack => t(
                "remote.at_computer.export",
                "Exporting a song pack is done at the computer, not from the remote page.",
            ),
            AtComputer::AddMedia => t(
                "remote.at_computer.media",
                "Adding pictures and videos is done at the computer, not from the remote page.",
            ),
            AtComputer::Samples => t(
                "remote.at_computer.samples",
                "The sample backgrounds are added at the computer, not from the remote page.",
            ),
            AtComputer::SongWordsFile => t(
                "remote.at_computer.words_file",
                "Reading a song's words from a file is done at the computer, not from the remote page.",
            ),
        }
    }
}

fn control_refusal(args: &[String]) -> Option<String> {
    let word = |at: usize| args.get(at).map(|w| w.to_lowercase()).unwrap_or_default();
    let first = word(0);
    let second = word(1);
    let after = |at: usize| args.len().saturating_sub(at);
    let at_computer = |what: AtComputer| Some(what.words().to_owned());
    if first.is_empty() {
        return Some(t("remote.refused.which", "Which command?").to_owned());
    }
    if CLOSED.contains(&first.as_str()) || !OPEN.contains(&first.as_str()) {
        return Some(tf(
            "remote.refused.command",
            "The remote page can't use the command {command}.",
            &[("command", &quoted(&args[0]))],
        ));
    }
    // Sub-commands: only those listed as open.
    if let Some((_, open)) = OPEN_SUBCOMMANDS.iter().find(|(word, _)| *word == first)
        && !open.contains(&second.as_str())
    {
        return match (first.as_str(), second.as_str()) {
            ("voxlive", _) => at_computer(AtComputer::VoxLive),
            ("canva", _) => at_computer(AtComputer::Canva),
            ("catalog", _) => at_computer(AtComputer::Catalog),
            ("song", "import") => at_computer(AtComputer::ImportSongs),
            ("song", "export") => at_computer(AtComputer::ExportPack),
            ("media", "add") => at_computer(AtComputer::AddMedia),
            ("media", "samples") => at_computer(AtComputer::Samples),
            _ => Some(tf(
                "remote.refused.command",
                "The remote page can't use the command {command}.",
                &[("command", &quoted(&format!("{} {}", args[0], args.get(1).map_or("", String::as_str))))],
            )),
        };
    }
    match (first.as_str(), second.as_str()) {
        // The page is sent every change as it happens; a wait would hold a place on the app's
        // main thread for nothing.
        ("status", _) if args.iter().any(|w| w == "--wait") => Some(
            t("remote.refused.wait", "The remote page is sent each change as it happens; it doesn't wait for one.")
                .to_owned(),
        ),
        // The menu bar's items include Quit and the ones that open the computer's own windows.
        ("menu", _) if args.len() > 1 => at_computer(AtComputer::MenuBar),
        // Files and folders on the computer: reading them in, following them.
        ("audio", "add") => at_computer(AtComputer::AudioTracks),
        ("media-playlist", "follow") if !(args.len() == 4 && word(3) == "off") => at_computer(AtComputer::FollowFolder),
        ("bible", "install") if !(args.len() == 3 && offered_bible(&args[2])) => at_computer(AtComputer::BibleFile),
        // A title, name or song given as one word (spaces and all) is never read as a file; more
        // words than that could end in one (`song new TITLE FILE.txt`, `prop new NAME PICTURE`,
        // `song text SONG FILE`).
        ("song", "text") if after(2) > 1 => Some(
            t(
                "remote.refused.one_word_song",
                "From the remote page, give the song as one word (quoted, spaces and all): song text “…”",
            )
            .to_owned(),
        ),
        ("song", "new") | ("prop", "new") if after(2) > 1 => Some(tf(
            "remote.refused.one_word_name",
            "From the remote page, give the name as one word (quoted, spaces and all): {command} “…”",
            &[("command", &format!("{first} {second}"))],
        )),
        // A web page on a slide runs its own scripts in the computer's web view, on the
        // screens: put up at the computer only. Taking one off is open.
        ("slide", "web")
            if !args
                .last()
                .is_some_and(|word| word.eq_ignore_ascii_case("off") || word.eq_ignore_ascii_case("none")) =>
        {
            at_computer(AtComputer::WebPage)
        }
        // NDI sends a screen to anyone on the network who asks, with no password: turned on at
        // the computer only.
        ("screen", "add" | "set")
            if args.iter().skip(2).any(|word| {
                let word = word.to_lowercase();
                word == "ndi" || word.starts_with("output=ndi")
            }) =>
        {
            at_computer(AtComputer::Ndi)
        }
        // The window's language is the operator's: a phone never changes it.
        ("settings", "set") if args.iter().skip(2).any(|word| word.to_lowercase().starts_with("language=")) => Some(
            t("remote.at_computer.language", "The app's language is chosen at the computer, not from the remote page.")
                .to_owned(),
        ),
        // Reading every song again from its file: one song at a time from a page. The command
        // reads what is left of the words, less `--now`, as the song's name: nothing, or "all",
        // reads them all (so `--now` alone, a blank word and "ALL" are refused too).
        ("song", "reload")
            if {
                let typed = args.iter().skip(2).filter(|w| *w != "--now").cloned().collect::<Vec<_>>().join(" ");
                typed.trim().is_empty() || typed.trim().eq_ignore_ascii_case("all")
            } =>
        {
            Some(
                t("remote.refused.reload_all", "From the remote page, name the song to read again: song reload SONG")
                    .to_owned(),
            )
        }
        _ => None,
    }
}

/// Whether a Bible's name is one SoftPresenter offers to download (`bibles` lists them).
fn offered_bible(typed: &str) -> bool {
    crate::bible::OFFERED.iter().any(|b| b.short.eq_ignore_ascii_case(typed) || b.id.eq_ignore_ascii_case(typed))
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
    fn reading_every_song_again_is_refused_however_it_is_worded() {
        let to_args = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
        for list in [
            &["song", "reload", ""][..],
            &["song", "reload", " "],
            &["song", "reload", "all"],
            &["song", "reload", " All "],
            &["song", "reload", "", "--now"],
            &["song", "reload", "--now", "ALL"],
            &["song", "reload", "--now", ""],
        ] {
            assert!(refusal(Role::Control, &to_args(list)).is_some(), "{list:?}");
        }
        for list in [
            &["song", "reload", "Amazing"][..],
            &["song", "reload", "--now", "Amazing"],
            &["song", "reload", "all", "the"],
        ] {
            assert!(refusal(Role::Control, &to_args(list)).is_none(), "{list:?}");
        }
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
            "slide web Amazing 2 off",
            "screen add audience Front on display Projector",
            "screen set Front output=window",
            "song reload Amazing",
            "settings set continuous=on",
            "media next",
            "voxlive show 1",
            "notices",
            "notices seen",
            "notices clear",
            "notices answer 3 outside:reload",
            "menu",
            "voxlive",
            "canva sync",
            "catalog",
            "catalog search grace",
            "catalog list ru 0 20",
            "catalog add ru:song~1a2b3c4d --second en",
            "catalog newer",
            "catalog use-newer Amazing",
            "catalog keep-mine Amazing",
            "undo",
        ] {
            assert!(!refused(line), "{line} should be open");
        }
    }

    #[test]
    fn files_keys_the_menu_and_waits_are_closed() {
        for line in [
            "song import /etc/hosts",
            "song --library Hymns import /etc/hosts",
            "song --library Hymns --library none export Amazing --to /tmp/x.sppack",
            "song new Title --library Hymns /etc/hosts",
            "SONG IMPORT /etc/hosts",
            "song export Amazing --to /tmp/x.sppack",
            "song new Title /etc/hosts",
            "song new Two words",
            "song text Amazing /etc/hosts",
            "prop new Banner /tmp/x.png",
            "media add /tmp/x.png",
            "media samples",
            "audio add /tmp/x.mp3",
            "media-playlist follow Loop /Users",
            "bible install /tmp/bible.xml",
            "bible install ASV --name x",
            "voxlive connect KEY",
            "voxlive disconnect --yes",
            "canva connect ID SECRET",
            "canva import",
            "canva disconnect --yes",
            "catalog download ru",
            "catalog check",
            "catalog remove ru --yes",
            "catalog sizes --ask",
            "catalog frobnicate",
            "menu Quit",
            "status --wait 5",
            "slide web Amazing 2 http://127.0.0.1:8080",
            "slide web Amazing 2 file:///etc/hosts",
            "slide web Amazing 2 https://example.org",
            "slide web Amazing 2 example.org",
            "screen add audience Stream on ndi Church",
            "screen set Front output=ndi:Church",
            "screen set Front OUTPUT=NDI:Church",
            "song reload",
            "song reload --now",
            "song reload all",
            "song reload ALL",
            "song reload --now all",
            "canva presentation https://www.canva.com/design/DAFxxxx/edit",
            "voxlive key",
            "song frobnicate x",
            "media import x",
            "settings reset",
            "settings set language=es",
            "settings set continuous=on LANGUAGE=en",
            "notices forget 3",
            "remote off",
            "remote password",
            "install-cli",
            "",
        ] {
            assert!(refused(line), "{line} should be refused");
        }
    }

    /// `song --library NAME` is the one command whose handler takes words out and reads the
    /// rest again; the policy reads it the same way. A handler that starts doing so too must be
    /// mirrored in `as_carried_out`.
    #[test]
    fn the_only_handler_that_reads_its_words_again_is_mirrored() {
        let mut found = Vec::new();
        for (name, source) in [
            ("control.rs", include_str!("../control.rs")),
            ("make_cli.rs", include_str!("../make_cli.rs")),
            ("service_cli.rs", include_str!("../service_cli.rs")),
            ("show_cli.rs", include_str!("../show_cli.rs")),
            ("slide_cli.rs", include_str!("../slide_cli.rs")),
            ("screen_cli.rs", include_str!("../screen_cli.rs")),
            ("theme_cli.rs", include_str!("../theme_cli.rs")),
        ] {
            for line in source.lines().filter(|line| line.contains("self.ctl_") && line.contains("(&rest)")) {
                found.push(format!("{name}: {}", line.trim()));
            }
        }
        assert_eq!(found, ["control.rs: let result = self.ctl_song(&rest);"]);
        assert!(refusal(Role::Control, &words("song --library Hymns new Title")).is_none());
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

    /// Every sub-command `sp help` shows for the commands made of them is listed as open or
    /// closed, so a new one is looked at before a page can use it.
    #[test]
    fn every_sub_command_the_help_shows_is_open_or_closed() {
        let mut seen = 0;
        for line in crate::control::HELP.lines() {
            if !line.starts_with("  ") || line.starts_with("   ") {
                continue;
            }
            let shape = line.trim_start().split("   ").next().unwrap_or("");
            for one in shape.split(" | ") {
                let mut parts = one.split_whitespace();
                let (Some(first), second) = (parts.next(), parts.next()) else { continue };
                let Some((_, open)) = OPEN_SUBCOMMANDS.iter().find(|(word, _)| *word == first) else { continue };
                let sub = second.unwrap_or("");
                // A placeholder (SONG, N…) or an option isn't a sub-command.
                if sub.chars().any(|c| c.is_ascii_uppercase()) || sub.starts_with('[') || sub.starts_with('-') {
                    continue;
                }
                let sub = sub.split('|').next().unwrap_or("");
                let closed = CLOSED_SUBCOMMANDS.iter().find(|(word, _)| *word == first).map_or(&[][..], |(_, c)| *c);
                assert!(open.contains(&sub) || closed.contains(&sub), "{first} {sub} is neither open nor closed");
                seen += 1;
            }
        }
        assert!(seen >= 20, "only {seen} sub-commands seen");
        // Closed and open never overlap.
        for (word, closed) in CLOSED_SUBCOMMANDS {
            let open = OPEN_SUBCOMMANDS.iter().find(|(have, _)| have == word).map_or(&[][..], |(_, o)| *o);
            assert!(closed.iter().all(|sub| !open.contains(sub)), "{word}");
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

    #[test]
    fn a_refusal_is_said_in_the_windows_language_and_given_back_in_english_for_the_log() {
        use crate::i18n::{Language, english_of, with_language};
        let menu = words("menu Quit");
        let english = with_language(Language::English, || refusal(Role::Control, &menu)).unwrap();
        assert_eq!(english, "Choosing from the menu bar is done at the computer, not from the remote page.");
        let spanish = with_language(Language::Spanish, || refusal(Role::Control, &menu)).unwrap();
        assert_eq!(spanish, "Elegir en la barra de menús se hace en el ordenador, no desde la página remota.");
        assert_eq!(english_of(&spanish), english);
        let unknown = with_language(Language::Spanish, || refusal(Role::Control, &words("frobnicate now"))).unwrap();
        assert_eq!(unknown, "La página remota no puede usar el comando “frobnicate”.");
        let sub = with_language(Language::English, || refusal(Role::Control, &words("song frob x"))).unwrap();
        assert_eq!(sub, "The remote page can't use the command “song frob”.");
        let one_word = with_language(Language::Spanish, || refusal(Role::Control, &words("prop new Big Banner")));
        assert_eq!(
            one_word.as_deref(),
            Some(
                "Desde la página remota, da el nombre como una sola palabra (entre comillas, con espacios y todo): prop new “…”"
            )
        );
        // Every reason a page is sent to the computer has its own Spanish sentence.
        let all = [
            AtComputer::MenuBar,
            AtComputer::AudioTracks,
            AtComputer::FollowFolder,
            AtComputer::BibleFile,
            AtComputer::WebPage,
            AtComputer::Ndi,
            AtComputer::VoxLive,
            AtComputer::Canva,
            AtComputer::Catalog,
            AtComputer::DownloadSongs,
            AtComputer::CheckNewer,
            AtComputer::RemoveSongs,
            AtComputer::ImportSongs,
            AtComputer::ExportPack,
            AtComputer::AddMedia,
            AtComputer::Samples,
            AtComputer::SongWordsFile,
        ];
        for what in all {
            let (english, spanish) =
                (with_language(Language::English, || what.words()), with_language(Language::Spanish, || what.words()));
            assert_ne!(english, spanish, "{what:?}");
            assert!(spanish.ends_with("no desde la página remota."), "{what:?}: {spanish}");
        }
    }
}
