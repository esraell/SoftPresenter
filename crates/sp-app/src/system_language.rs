//! The computer's language, for the Bible view: when nothing typed says which language the
//! operator reads, a session starts in the computer's (Russian if its first language is
//! Russian, Ukrainian if Ukrainian, German if German, otherwise English).
//!
//! Each system is asked in its own way, as the clock asks for its setting (`clock.rs`): on a
//! Mac the user's preferred languages (the global `AppleLanguages` default, read with
//! `defaults`); on Windows the user's interface language (PowerShell's `Get-UICulture`, which
//! Windows fills from the user's preferred UI languages), then the user's language list
//! (`Get-WinUserLanguageList`, in the same run); elsewhere `LC_ALL`, `LC_MESSAGES` or `LANG`,
//! the first one set, as POSIX orders them, then the languages `LANGUAGE` lists. The first is
//! the computer's language (the app's own words, the Bible view); the rest are its other
//! preferred languages, which the Bibles window lists Bibles in first too. Running a program is
//! slow, so it is read once, when the app starts, on a thread of its own; until it answers (and
//! in tests, which read nothing of the computer they run on) the language is not known.

use std::sync::{Arc, OnceLock};

/// The Bible language a language tag stands for: `ru`, `uk` or `de`, otherwise `en`. Takes the
/// tags systems write (`ru-RU`, `uk`, `ru_UA.UTF-8`, `de-AT`, `en-US@calendar=…`).
pub fn bible_language(tag: &str) -> &'static str {
    let first = tag.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    match first.as_str() {
        "ru" | "rus" => "ru",
        "uk" | "ukr" => "uk",
        "de" | "deu" | "ger" => "de",
        _ => "en",
    }
}

/// The languages in what `defaults read -g AppleLanguages` prints, in the user's order: a list
/// such as `(\n    "en-US",\n    "ru-US"\n)`, its entries quoted when they hold a hyphen and
/// bare when they don't (`ru`). Empty when there is none.
#[cfg(any(test, target_os = "macos"))]
pub fn from_apple_languages(printed: &str) -> Vec<String> {
    printed
        .split([',', '\n', '(', ')'])
        .map(|entry| entry.trim().trim_matches('"').trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The languages in what `(Get-UICulture).Name` and then `(Get-WinUserLanguageList).LanguageTag`
/// print: a tag a line, such as `ru-RU`, the interface language first. Empty when the first line
/// is no tag (nothing printed, or an error, which goes elsewhere); a later line that is no tag is
/// passed over.
#[cfg(any(test, target_os = "windows"))]
pub fn from_ui_culture(printed: &str) -> Vec<String> {
    let tag = |line: &&str| line.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let mut lines = printed.lines().map(str::trim).filter(|line| !line.is_empty());
    match lines.next() {
        Some(first) if tag(&first) => std::iter::once(first).chain(lines.filter(tag)).map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// The language in the locale variables, the first set of `LC_ALL`, `LC_MESSAGES` and `LANG`
/// (POSIX's order). `C` and `POSIX` name no language.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub fn from_locale_variables(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Option<String> {
    let value = [lc_all, lc_messages, lang].into_iter().flatten().map(str::trim).find(|value| !value.is_empty())?;
    let name = value.split(['.', '@']).next().unwrap_or(value);
    (!matches!(name, "C" | "POSIX")).then(|| value.to_owned())
}

/// The languages `LANGUAGE` lists, as GNU programs read it: codes separated by colons
/// (`uk:ru:en_GB`), the one preferred first.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub fn from_language_variable(language: Option<&str>) -> Vec<String> {
    language.unwrap_or("").split(':').map(str::trim).filter(|tag| !tag.is_empty()).map(str::to_owned).collect()
}

/// What a small program of the system's prints, or `None` when it can't be run or fails.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn printed(mut command: std::process::Command) -> Option<String> {
    let output = command.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The computer's preferred languages, as the system writes them, its own language first; empty
/// when they can't be read. Slow (it runs a program): never on the thread that draws.
#[cfg(target_os = "macos")]
fn read_system() -> Vec<String> {
    let mut command = std::process::Command::new("/usr/bin/defaults");
    command.args(["read", "-g", "AppleLanguages"]);
    printed(command).map(|printed| from_apple_languages(&printed)).unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn read_system() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    // Run without a console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // The interface language first, as before, then the user's language list. Should the list
    // not be there to read, its error is caught and the run still ends well, so the interface
    // language is never lost with it.
    const SCRIPT: &str =
        "(Get-UICulture).Name; try { (Get-WinUserLanguageList -ErrorAction Stop).LanguageTag } catch {}; exit 0";
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]).creation_flags(CREATE_NO_WINDOW);
    printed(command).map(|printed| from_ui_culture(&printed)).unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_system() -> Vec<String> {
    let var = |name: &str| std::env::var(name).ok();
    // With no language set (the C locale), GNU programs pass over LANGUAGE too.
    let Some(first) =
        from_locale_variables(var("LC_ALL").as_deref(), var("LC_MESSAGES").as_deref(), var("LANG").as_deref())
    else {
        return Vec::new();
    };
    std::iter::once(first).chain(from_language_variable(var("LANGUAGE").as_deref())).collect()
}

/// The computer's language, read once.
#[derive(Clone, Debug, Default)]
pub struct SystemLanguage {
    /// The computer's first language as the system wrote it (`es-ES`), once read.
    read: Arc<OnceLock<String>>,
}

impl SystemLanguage {
    /// Starts reading the computer's language, on a thread of its own. Called once, when the app
    /// starts; nothing waits for it. Never in a test.
    pub fn start(&self) {
        if cfg!(test) {
            return;
        }
        // Testing aid: SOFTPRESENTER_LANGUAGE=ru (or uk, es, de, en) stands in for the computer's
        // language, so a test copy can start as a Russian, Ukrainian, Spanish or German computer would
        // (the Bible view and the app's own words both follow it, unless the Language setting
        // chose one). Several, separated by commas (ru,uk,en), stand in for its preferred
        // languages, its own first (the Bibles window lists Bibles in them first).
        if let Ok(tags) = std::env::var("SOFTPRESENTER_LANGUAGE") {
            let tags: Vec<&str> = tags.split(',').map(str::trim).filter(|tag| !tag.is_empty()).collect();
            crate::i18n::set_computer(&tags);
            let _ = self.read.set(tags.first().copied().unwrap_or_default().to_owned());
            return;
        }
        let read = self.read.clone();
        let _ = std::thread::Builder::new().name("system language".to_owned()).spawn(move || {
            let tags = read_system();
            crate::i18n::set_computer(&tags);
            let _ = read.set(tags.into_iter().next().unwrap_or_default());
        });
    }

    /// `en`, `ru`, `uk` or `de`, once the computer has answered.
    pub fn get(&self) -> Option<&'static str> {
        self.read.get().map(|tag| bible_language(tag))
    }

    /// As if the computer had answered this (for tests).
    #[cfg(test)]
    pub fn answered(tag: &str) -> SystemLanguage {
        let language = SystemLanguage::default();
        let _ = language.read.set(tag.to_owned());
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
            ("de-DE", "de"),
            ("de-CH", "de"),
            ("de_AT.UTF-8", "de"),
            ("fr-FR", "en"),
            ("be-BY", "en"),
            ("", "en"),
        ] {
            assert_eq!(bible_language(tag), language, "{tag}");
        }
    }

    #[test]
    fn a_macs_preferred_languages_are_read_in_order() {
        // As `defaults read -g AppleLanguages` printed it on the Mac this was written on.
        assert_eq!(from_apple_languages("(\n    \"en-US\",\n    \"ru-US\"\n)\n"), ["en-US", "ru-US"]);
        // An entry without a hyphen is printed bare.
        assert_eq!(from_apple_languages("(\n    ru,\n    \"en-RU\"\n)\n"), ["ru", "en-RU"]);
        assert_eq!(from_apple_languages("(\n    \"uk-UA\"\n)"), ["uk-UA"]);
        assert!(from_apple_languages("(\n)\n").is_empty());
        assert!(from_apple_languages("").is_empty());
        assert_eq!(bible_language(&from_apple_languages("(\n    ru,\n    \"en-RU\"\n)\n")[0]), "ru");
    }

    #[test]
    fn windows_interface_language_and_language_list_are_read() {
        assert_eq!(from_ui_culture("ru-RU\r\n"), ["ru-RU"]);
        assert_eq!(from_ui_culture("\r\nuk-UA\r\n"), ["uk-UA"]);
        assert!(from_ui_culture("").is_empty());
        assert!(from_ui_culture("The term 'Get-UICulture' is not recognized").is_empty());
        // The interface language, then the user's language list, as the one run prints them.
        assert_eq!(from_ui_culture("en-US\r\nen-US\r\nru\r\nuk-UA\r\n"), ["en-US", "en-US", "ru", "uk-UA"]);
        // A line that is no tag after the first is passed over; the first stays.
        assert_eq!(from_ui_culture("ru-RU\r\nSomething went wrong.\r\nuk\r\n"), ["ru-RU", "uk"]);
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

    #[test]
    fn the_language_variable_lists_languages_in_order() {
        assert_eq!(from_language_variable(Some("uk:ru:en_GB")), ["uk", "ru", "en_GB"]);
        assert_eq!(from_language_variable(Some(" ru :: ")), ["ru"]);
        assert!(from_language_variable(Some("")).is_empty());
        assert!(from_language_variable(None).is_empty());
    }

    /// Not a check: says what the computer the test runs on answers (it only reads).
    /// `cargo test -p sp-app system_language::tests::this_computer -- --ignored --nocapture`
    #[test]
    #[ignore = "reads the computer it runs on"]
    fn this_computer_answers() {
        let started = std::time::Instant::now();
        let read = read_system();
        let language = bible_language(read.first().map_or("", String::as_str));
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
