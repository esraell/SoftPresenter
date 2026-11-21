//! The computer's language, for the Bible view: when nothing typed says which language the
//! operator reads, a session starts in the computer's (Russian if its first language is
//! Russian, Ukrainian if Ukrainian, otherwise English).
//!
//! Each system is asked in its own way, as the clock asks for its setting (`clock.rs`): on a
//! Mac the user's preferred languages (the global `AppleLanguages` default, read with
//! `defaults`); on Windows the user's interface language (PowerShell's `Get-UICulture`, which
//! Windows fills from the user's preferred UI languages); elsewhere `LC_ALL`, `LC_MESSAGES` or
//! `LANG`, the first one set, as POSIX orders them. Running a program is slow, so it is read
//! once, when the app starts, on a thread of its own; until it answers (and in tests, which
//! read nothing of the computer they run on) the language is not known.

use std::sync::{Arc, OnceLock};

/// The Bible language a language tag stands for: `ru` or `uk`, otherwise `en`. Takes the tags
/// systems write (`ru-RU`, `uk`, `ru_UA.UTF-8`, `en-US@calendar=…`).
pub fn bible_language(tag: &str) -> &'static str {
    let first = tag.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    match first.as_str() {
        "ru" | "rus" => "ru",
        "uk" | "ukr" => "uk",
        _ => "en",
    }
}

/// The first language in what `defaults read -g AppleLanguages` prints: a list such as
/// `(\n    "en-US",\n    "ru-US"\n)`, its entries quoted when they hold a hyphen and bare
/// when they don't (`ru`). `None` when there is none.
#[cfg(any(test, target_os = "macos"))]
pub fn from_apple_languages(printed: &str) -> Option<String> {
    printed
        .split([',', '\n', '(', ')'])
        .map(|entry| entry.trim().trim_matches('"').trim())
        .find(|entry| !entry.is_empty())
        .map(str::to_owned)
}

/// The language in what `(Get-UICulture).Name` prints: one tag such as `ru-RU`. `None` when it
/// printed nothing (or an error, which goes elsewhere).
#[cfg(any(test, target_os = "windows"))]
pub fn from_ui_culture(printed: &str) -> Option<String> {
    let line = printed.lines().map(str::trim).find(|line| !line.is_empty())?;
    line.chars().all(|c| c.is_ascii_alphanumeric() || c == '-').then(|| line.to_owned())
}

/// The language in the locale variables, the first set of `LC_ALL`, `LC_MESSAGES` and `LANG`
/// (POSIX's order). `C` and `POSIX` name no language.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub fn from_locale_variables(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Option<String> {
    let value = [lc_all, lc_messages, lang].into_iter().flatten().map(str::trim).find(|value| !value.is_empty())?;
    let name = value.split(['.', '@']).next().unwrap_or(value);
    (!matches!(name, "C" | "POSIX")).then(|| value.to_owned())
}

/// What a small program of the system's prints, or `None` when it can't be run or fails.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn printed(mut command: std::process::Command) -> Option<String> {
    let output = command.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The computer's first language, as the system writes it. Slow (it runs a program): never on
/// the thread that draws.
#[cfg(target_os = "macos")]
fn read_system() -> Option<String> {
    let mut command = std::process::Command::new("/usr/bin/defaults");
    command.args(["read", "-g", "AppleLanguages"]);
    from_apple_languages(&printed(command)?)
}

#[cfg(target_os = "windows")]
fn read_system() -> Option<String> {
    use std::os::windows::process::CommandExt;
    // Run without a console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-UICulture).Name"])
        .creation_flags(CREATE_NO_WINDOW);
    from_ui_culture(&printed(command)?)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_system() -> Option<String> {
    let var = |name: &str| std::env::var(name).ok();
    from_locale_variables(var("LC_ALL").as_deref(), var("LC_MESSAGES").as_deref(), var("LANG").as_deref())
}

/// The computer's language, read once.
#[derive(Clone, Debug, Default)]
pub struct SystemLanguage {
    /// The Bible language it stands for, once read.
    read: Arc<OnceLock<&'static str>>,
}

impl SystemLanguage {
    /// Starts reading the computer's language, on a thread of its own. Called once, when the app
    /// starts; nothing waits for it. Never in a test.
    pub fn start(&self) {
        if cfg!(test) {
            return;
        }
        // Testing aid: SOFTPRESENTER_LANGUAGE=ru (or uk, en) stands in for the computer's
        // language, so a test copy can start as a Russian or Ukrainian computer would.
        if let Ok(tag) = std::env::var("SOFTPRESENTER_LANGUAGE") {
            let _ = self.read.set(bible_language(&tag));
            return;
        }
        let read = self.read.clone();
        let _ = std::thread::Builder::new().name("system language".to_owned()).spawn(move || {
            let _ = read.set(bible_language(&read_system().unwrap_or_default()));
        });
    }

    /// `en`, `ru` or `uk`, once the computer has answered.
    pub fn get(&self) -> Option<&'static str> {
        self.read.get().copied()
    }

    /// As if the computer had answered this (for tests).
    #[cfg(test)]
    pub fn answered(tag: &str) -> SystemLanguage {
        let language = SystemLanguage::default();
        let _ = language.read.set(bible_language(tag));
        language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_tag_becomes_a_bible_language() {
        for (tag, language) in [
            ("ru-RU", "ru"),
            ("ru", "ru"),
            ("ru_UA.UTF-8", "ru"),
            ("uk-UA", "uk"),
            ("uk", "uk"),
            ("uk_UA.UTF-8@euro", "uk"),
            ("en-US", "en"),
            ("de-DE", "en"),
            ("be-BY", "en"),
            ("", "en"),
        ] {
            assert_eq!(bible_language(tag), language, "{tag}");
        }
    }

    #[test]
    fn a_macs_first_preferred_language_is_read() {
        // As `defaults read -g AppleLanguages` printed it on the Mac this was written on.
        assert_eq!(from_apple_languages("(\n    \"en-US\",\n    \"ru-US\"\n)\n").as_deref(), Some("en-US"));
        // An entry without a hyphen is printed bare.
        assert_eq!(from_apple_languages("(\n    ru,\n    \"en-RU\"\n)\n").as_deref(), Some("ru"));
        assert_eq!(from_apple_languages("(\n    \"uk-UA\"\n)").as_deref(), Some("uk-UA"));
        assert_eq!(from_apple_languages("(\n)\n"), None);
        assert_eq!(from_apple_languages(""), None);
        assert_eq!(bible_language(&from_apple_languages("(\n    ru,\n    \"en-RU\"\n)\n").unwrap()), "ru");
    }

    #[test]
    fn windows_interface_language_is_read() {
        assert_eq!(from_ui_culture("ru-RU\r\n").as_deref(), Some("ru-RU"));
        assert_eq!(from_ui_culture("\r\nuk-UA\r\n").as_deref(), Some("uk-UA"));
        assert_eq!(from_ui_culture(""), None);
        assert_eq!(from_ui_culture("The term 'Get-UICulture' is not recognized"), None);
    }

    #[test]
    fn the_locale_variables_are_read_in_posix_order() {
        assert_eq!(from_locale_variables(None, None, Some("ru_RU.UTF-8")).as_deref(), Some("ru_RU.UTF-8"));
        assert_eq!(
            from_locale_variables(Some("uk_UA.UTF-8"), None, Some("en_US.UTF-8")).as_deref(),
            Some("uk_UA.UTF-8")
        );
        assert_eq!(from_locale_variables(Some(""), Some("ru_RU"), Some("en_US")).as_deref(), Some("ru_RU"));
        assert_eq!(from_locale_variables(None, None, Some("C.UTF-8")), None);
        assert_eq!(from_locale_variables(Some("POSIX"), None, Some("ru_RU")), None, "LC_ALL wins, and names none");
        assert_eq!(from_locale_variables(None, None, None), None);
    }

    /// Not a check: says what the computer the test runs on answers (it only reads).
    /// `cargo test -p sp-app system_language::tests::this_computer -- --ignored --nocapture`
    #[test]
    #[ignore = "reads the computer it runs on"]
    fn this_computer_answers() {
        let started = std::time::Instant::now();
        let read = read_system();
        let language = bible_language(read.as_deref().unwrap_or(""));
        eprintln!("the system says {read:?}, read in {:?}: the Bible view starts in {language}", started.elapsed());
    }

    #[test]
    fn nothing_is_read_in_a_test() {
        let language = SystemLanguage::default();
        language.start();
        assert_eq!(language.get(), None);
        assert_eq!(SystemLanguage::answered("ru-RU").get(), Some("ru"));
    }
}
