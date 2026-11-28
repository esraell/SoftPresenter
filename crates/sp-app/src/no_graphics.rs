//! What a start says when this computer's graphics can't run SoftPresenter: no graphics device
//! could be made, or the graphics refused a part the app needs to draw (a shader's pipeline).
//! The words are the design's, in the app's language: what happened, what may fix it on this
//! system, and where to tell us. They go to the standard error, and, in a run that isn't a
//! test's, into a message window of the system's own with Quit and Report….

use crate::i18n::{t, tf};

/// Where problems are told.
const ISSUES: &str = "github.com/esraell/SoftPresenter/issues";

/// Why nothing here can draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No graphics device could be made.
    NoDevice,
    /// A device was made, and it refused this part the app needs to draw.
    Refused(String),
}

/// The system the advice is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    /// A Mac, with macOS' first number (11, 12, 13…) when it could be read: System Preferences
    /// became System Settings, with Software Update under General, in macOS 13.
    Mac(Option<u32>),
    Windows,
    Linux,
}

impl System {
    /// The system this program runs on. On a Mac its version is asked of `sw_vers`.
    pub fn here() -> System {
        if cfg!(target_os = "macos") {
            let said = std::process::Command::new("sw_vers").arg("-productVersion").output().ok();
            System::Mac(said.and_then(|out| macos_major(&String::from_utf8_lossy(&out.stdout))))
        } else if cfg!(windows) {
            System::Windows
        } else {
            System::Linux
        }
    }
}

/// macOS' first number, from what `sw_vers -productVersion` says ("11.6", "12.7.6", "26.0").
fn macos_major(said: &str) -> Option<u32> {
    said.trim().split('.').next()?.parse().ok()
}

impl Failure {
    /// The part of the app the graphics refused, as the graphics library's error names it (the
    /// pipeline's label, `vello.fine_area`): the one technical word the message carries, because
    /// a report needs it. The graphics' own name when the error names no part.
    pub fn refused(error: &str, graphics: &str) -> Failure {
        let part = error.split_once("label = '").and_then(|(_, rest)| rest.split_once('\'')).map(|(label, _)| label);
        Failure::Refused(part.filter(|label| !label.is_empty()).unwrap_or(graphics).to_owned())
    }

    /// The first line: what happened.
    pub fn first_line(&self) -> String {
        match self {
            Failure::NoDevice => t(
                "dialogs.no_graphics.none",
                "This computer's graphics can't run SoftPresenter. No graphics device was found that it can use.",
            )
            .to_owned(),
            Failure::Refused(name) => tf(
                "dialogs.no_graphics.refused",
                "This computer's graphics can't run SoftPresenter. They refused a part it needs to draw: {name}.",
                &[("name", name)],
            ),
        }
    }

    /// The last line: where to tell us.
    fn last_line(&self) -> String {
        match self {
            Failure::NoDevice => tf(
                "dialogs.no_graphics.tell",
                "If you think it should work, tell us which computer this is: {address}",
                &[("address", ISSUES)],
            ),
            Failure::Refused(_) => tf(
                "dialogs.no_graphics.tell_name",
                "If you think it should work, tell us which computer this is: {address} and include the name above.",
                &[("address", ISSUES)],
            ),
        }
    }

    /// The whole message for a system: what happened, what may fix it there, where to tell us.
    pub fn words(&self, system: System) -> String {
        format!("{}\n\n{}\n\n{}", self.first_line(), advice(system), self.last_line())
    }

    /// The address of a new issue with only the version, the system and the first line (in
    /// English) filled in.
    pub fn report_url(&self, system: System) -> String {
        let system = match system {
            System::Mac(Some(major)) => format!("macOS {major}"),
            System::Mac(None) => "macOS".to_owned(),
            System::Windows => "Windows".to_owned(),
            System::Linux => "Linux".to_owned(),
        };
        let line = crate::i18n::english_of(&self.first_line());
        let body = format!("SoftPresenter {}\n{system}\n{line}\n\n", crate::updates::VERSION);
        format!("https://{ISSUES}/new?body={}", percent_encoded(&body))
    }

    /// Shows the message in a window of the system's own, titled "SoftPresenter can't start",
    /// with Quit (the default) and Report…, and waits on the window's thread for it to be
    /// closed; Report… opens the new issue in the browser first. The program ends after that,
    /// not before. No screen is open yet and nothing is on show, so no picture waits on it. The
    /// rest of the start is up, though: the control socket's thread listens (a command sent
    /// meanwhile waits for an answer that never comes), the screens' thread may run, and the
    /// copy still holds its data folder. So another start while the message is open is turned
    /// away as from any copy running: it asks this one to come to the front, gets no answer for
    /// ten seconds, and ends, on Windows with the message that SoftPresenter is already running
    /// or still closing (`one_copy`), once for each such start. Not tried on a real screen.
    pub fn show_window(&self, system: System) {
        let quit = t("dialogs.no_graphics.quit", "Quit").to_owned();
        let report = t("dialogs.no_graphics.report", "Report…").to_owned();
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(t("dialogs.no_graphics.title", "SoftPresenter can't start"))
            .set_description(self.words(system))
            // The first is the default.
            .set_buttons(rfd::MessageButtons::OkCancelCustom(quit, report.clone()))
            .show();
        if report_chosen(&answer, &report)
            && let Err(e) = crate::canva::open_page(&self.report_url(system))
        {
            crate::applog::problem("start", &format!("The browser couldn't be opened for a report: {e}"));
        }
    }
}

/// Whether the window's answer is its Report… key: only that key by its own words. A window
/// closed any other way (Quit, Escape, a system that shows its own OK and Cancel) only quits, so
/// that nothing opens a browser the person didn't ask for.
fn report_chosen(answer: &rfd::MessageDialogResult, report: &str) -> bool {
    matches!(answer, rfd::MessageDialogResult::Custom(chosen) if chosen == report)
}

/// What may fix it, by system.
fn advice(system: System) -> &'static str {
    match system {
        // Unknown counts as a Mac of today.
        System::Mac(Some(major)) if major <= 12 => t(
            "dialogs.no_graphics.mac_before_13",
            "Updating macOS may fix it: System Preferences › Software Update. If this Mac is already up to date, its graphics are too old for SoftPresenter.",
        ),
        System::Mac(_) => t(
            "dialogs.no_graphics.mac",
            "Updating macOS may fix it: System Settings › General › Software Update. If this Mac is already up to date, its graphics are too old for SoftPresenter.",
        ),
        System::Windows => t(
            "dialogs.no_graphics.windows",
            "Updating the graphics driver may fix it: get the newest from the computer's maker, or from Intel, AMD or NVIDIA. If it is already the newest, this computer's graphics are too old for SoftPresenter.",
        ),
        System::Linux => t(
            "dialogs.no_graphics.linux",
            "Updating the graphics drivers may fix it (Mesa, or the graphics maker's own). If they are already the newest, this computer's graphics are too old for SoftPresenter.",
        ),
    }
}

/// Text as it goes into an address: letters, digits and `-._~` as they are, every other byte as
/// `%XX`.
fn percent_encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    fn english<T>(run: impl FnOnce() -> T) -> T {
        with_language(Language::English, run)
    }

    const MAC_NOW: &str = "Updating macOS may fix it: System Settings › General › Software Update. If this Mac is already up to date, its graphics are too old for SoftPresenter.";
    const MAC_BEFORE_13: &str = "Updating macOS may fix it: System Preferences › Software Update. If this Mac is already up to date, its graphics are too old for SoftPresenter.";

    /// The words are the design's, to the letter (`handoff/03-behaviour.md`, "When the
    /// computer's graphics can't run SoftPresenter").
    #[test]
    fn each_system_gets_its_own_advice_between_what_happened_and_where_to_tell_us() {
        english(|| {
            let none =
                "This computer's graphics can't run SoftPresenter. No graphics device was found that it can use.";
            let tell =
                "If you think it should work, tell us which computer this is: github.com/esraell/SoftPresenter/issues";
            let windows = "Updating the graphics driver may fix it: get the newest from the computer's maker, or from Intel, AMD or NVIDIA. If it is already the newest, this computer's graphics are too old for SoftPresenter.";
            let linux = "Updating the graphics drivers may fix it (Mesa, or the graphics maker's own). If they are already the newest, this computer's graphics are too old for SoftPresenter.";
            for (system, advice) in [
                (System::Mac(Some(13)), MAC_NOW),
                (System::Mac(Some(11)), MAC_BEFORE_13),
                (System::Windows, windows),
                (System::Linux, linux),
            ] {
                assert_eq!(Failure::NoDevice.words(system), format!("{none}\n\n{advice}\n\n{tell}"), "{system:?}");
            }
        });
    }

    #[test]
    fn a_mac_is_sent_to_system_preferences_up_to_macos_12_and_to_system_settings_from_13() {
        english(|| {
            for (said, advice) in [
                ("10.15.7", MAC_BEFORE_13),
                ("11.6", MAC_BEFORE_13),
                ("11.7.10\n", MAC_BEFORE_13),
                ("12.7.6", MAC_BEFORE_13),
                ("13.0", MAC_NOW),
                ("15.8.1", MAC_NOW),
                ("26.0", MAC_NOW),
                // A version that couldn't be read is taken for today's.
                ("", MAC_NOW),
                ("unknown", MAC_NOW),
            ] {
                assert_eq!(super::advice(System::Mac(macos_major(said))), advice, "macOS {said:?}");
            }
        });
    }

    #[test]
    fn a_refused_part_is_named_and_asked_for_in_the_report() {
        english(|| {
            let error = "Validation Error Caused by: In Device::create_compute_pipeline, label = 'vello.fine_area' Internal error: …";
            let failure = Failure::refused(error, "Intel Iris Graphics");
            assert_eq!(failure, Failure::Refused("vello.fine_area".to_owned()));
            let words = failure.words(System::Mac(Some(11)));
            let lines: Vec<&str> = words.split("\n\n").collect();
            assert_eq!(
                lines,
                [
                    "This computer's graphics can't run SoftPresenter. They refused a part it needs to draw: vello.fine_area.",
                    MAC_BEFORE_13,
                    "If you think it should work, tell us which computer this is: github.com/esraell/SoftPresenter/issues and include the name above.",
                ]
            );
            // An error that names no part: the graphics' own name stands in.
            assert_eq!(
                Failure::refused("something else", "Intel Iris Graphics"),
                Failure::Refused("Intel Iris Graphics".to_owned())
            );
            // No "error", no code, no library's name: the part's name is the only technical word.
            for failure in [Failure::NoDevice, Failure::Refused("sp-render video colours".to_owned())] {
                for system in [System::Mac(Some(11)), System::Mac(None), System::Windows, System::Linux] {
                    let words = failure.words(system).to_lowercase();
                    for never in ["error", "wgpu", "vello", "metal", "vulkan", "directx", "shader", "pipeline"] {
                        assert!(!words.contains(never), "{never:?} in {words:?}");
                    }
                }
            }
        });
    }

    #[test]
    fn a_report_carries_only_the_version_the_system_and_the_first_line() {
        english(|| {
            let url = Failure::Refused("vello.fine_area".to_owned()).report_url(System::Mac(Some(11)));
            let body = format!(
                "SoftPresenter {}\nmacOS 11\nThis computer's graphics can't run SoftPresenter. They refused a part it needs to draw: vello.fine_area.\n\n",
                crate::updates::VERSION
            );
            assert_eq!(
                url,
                format!("https://github.com/esraell/SoftPresenter/issues/new?body={}", percent_encoded(&body))
            );
            assert!(url.is_ascii() && !url.contains(' ') && !url.contains('\n'), "{url}");
            assert_eq!(percent_encoded("a b\n'›"), "a%20b%0A%27%E2%80%BA");
            assert!(Failure::NoDevice.report_url(System::Windows).contains("%0AWindows%0A"));
        });
    }

    #[test]
    fn only_the_report_key_opens_a_report() {
        use rfd::MessageDialogResult as Answer;
        assert!(report_chosen(&Answer::Custom("Report…".to_owned()), "Report…"));
        for other in [Answer::Custom("Quit".to_owned()), Answer::Ok, Answer::Cancel, Answer::Yes, Answer::No] {
            assert!(!report_chosen(&other, "Report…"), "{other:?}");
        }
    }

    /// Every language has the whole message: nothing of it is left in English but the names.
    #[test]
    fn each_language_says_the_whole_message_in_its_own_words() {
        let in_english = english(|| Failure::Refused("x".to_owned()).words(System::Mac(Some(11))));
        for language in [Language::Spanish, Language::Russian, Language::Ukrainian] {
            with_language(language, || {
                for system in [System::Mac(Some(11)), System::Mac(Some(13)), System::Windows, System::Linux] {
                    for failure in [Failure::NoDevice, Failure::Refused("x".to_owned())] {
                        let words = failure.words(system);
                        let lines: Vec<&str> = words.split("\n\n").collect();
                        assert_eq!(lines.len(), 3, "{language:?} {system:?}: {words}");
                        for (line, was) in lines.iter().zip(in_english.split("\n\n")) {
                            assert_ne!(*line, was, "{language:?} {system:?}");
                            assert!(
                                !line.contains("may fix it") && !line.contains("can't run"),
                                "{language:?}: {line}"
                            );
                        }
                        assert!(words.contains("github.com/esraell/SoftPresenter/issues"), "{language:?}: {words}");
                    }
                }
                let before = advice(System::Mac(Some(12)));
                let now = advice(System::Mac(Some(13)));
                assert_ne!(before, now, "{language:?}: the two Mac paths");
                for key in ["dialogs.no_graphics.title", "dialogs.no_graphics.quit", "dialogs.no_graphics.report"] {
                    assert_ne!(t(key, "\u{0}"), "\u{0}", "{language:?} has {key}");
                }
            });
        }
    }
}
