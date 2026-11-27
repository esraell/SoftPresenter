//! NDI's row on its card (Settings › Integrations › NDI) and the rows under it: one row at one
//! height on every system, saying how NDI stands, with one key that keeps one width. An update
//! runs in the row itself, with a bar along the foot of its words; the first install has the
//! Install NDI window ([`super::window`]). Then Where it's used, Remove NDI, and NDI's
//! trademark line.

use egui::{Rect, pos2, vec2};

use super::sdk::Problem;
use super::{NdiDownload, Note, Place, Stage, way};
use crate::home::settings::parts::{self, Key};
use crate::i18n::{t, tf};
use crate::studio::{BUTTON, Flat, TEXT};

/// What the row's key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Opens the Install NDI window (Install NDI, and Show… while it is under way).
    Open,
    Check,
    /// Install Update, and Try Again after an update that failed.
    Update,
    /// Cancel, while an update downloads.
    Cancel,
}

/// The row, as it stands now.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub line: String,
    pub under: String,
    pub key: &'static str,
    /// Amber for the key that brings an update in.
    pub amber: bool,
    /// What the key does; none while it is greyed.
    pub act: Option<Act>,
    /// How far an update's download has got, 0 to 1, while it is downloading.
    pub bar: Option<f32>,
}

/// Megabytes, as the row and the window say them.
pub fn megabytes(bytes: u64) -> String {
    crate::i18n::count(bytes.div_ceil(1_000_000))
}

/// Which sentence says a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sentence {
    Download,
    Save,
    Install,
}

/// Why it failed, as the words that end "NDI couldn't be downloaded: ….", and which sentence
/// they end.
fn why(problem: &Problem) -> (Sentence, String) {
    match problem {
        Problem::Offline => {
            (Sentence::Download, t("integrations.ndi.why.network", "there is no internet connection").to_owned())
        }
        Problem::Blocked => (
            Sentence::Download,
            t("integrations.ndi.why.blocked", "this network checks secure connections and blocked it").to_owned(),
        ),
        Problem::DiskFull => {
            (Sentence::Save, t("integrations.ndi.why.disk", "this computer\u{2019}s disk is full").to_owned())
        }
        Problem::Disk(why) => (
            Sentence::Save,
            tf("integrations.ndi.why.disk_other", "it couldn\u{2019}t be written ({why})", &[("why", why)]),
        ),
        Problem::NoRuntime(processor) => (
            Sentence::Download,
            tf(
                "integrations.ndi.why.processor",
                "NDI\u{2019}s download has no NDI for this computer\u{2019}s processor ({processor})",
                &[("processor", processor)],
            ),
        ),
        Problem::Network(_) => (
            Sentence::Download,
            t("integrations.ndi.why.server", "NDI\u{2019}s server didn\u{2019}t answer as it should").to_owned(),
        ),
        Problem::Closed => (
            Sentence::Install,
            t("integrations.ndi.why.closed", "NDI\u{2019}s installer was closed before it finished").to_owned(),
        ),
        Problem::Layout(_) | Problem::Damaged(_) | Problem::TooLarge | Problem::Unsigned(_) | Problem::Cancelled => (
            Sentence::Download,
            t("integrations.ndi.why.file", "the download wasn\u{2019}t what NDI usually sends").to_owned(),
        ),
    }
}

/// What the operator is told when an install (or, in the row, an update) failed.
pub fn failed_line(problem: &Problem, update: bool) -> String {
    let (sentence, why) = why(problem);
    let why = [("why", why.as_str())];
    if update {
        return tf("integrations.ndi.fail.update", "NDI couldn\u{2019}t be updated: {why}.", &why);
    }
    match sentence {
        Sentence::Download => tf("integrations.ndi.fail.download", "NDI couldn\u{2019}t be downloaded: {why}.", &why),
        Sentence::Save => tf("integrations.ndi.fail.save", "NDI couldn\u{2019}t be saved: {why}.", &why),
        Sentence::Install => tf("integrations.ndi.fail.install", "NDI wasn\u{2019}t installed: {why}.", &why),
    }
}

/// The line under a failure in the window: what to try, or that nothing changed.
pub fn failed_under(problem: &Problem) -> &'static str {
    match problem {
        Problem::Blocked => {
            t("integrations.ndi.fail.hotspot", "Try another network, such as a phone\u{2019}s hotspot.")
        }
        Problem::Closed => {
            t("integrations.ndi.fail.closed", "Nothing was changed. Try Again opens NDI\u{2019}s installer again.")
        }
        _ => t("integrations.ndi.fail.nothing", "Nothing was installed."),
    }
}

/// "NDI 6.1.1 is installed.", or without the version where nothing says it.
pub fn installed_line(version: Option<&str>) -> String {
    match version {
        Some(version) => tf("integrations.ndi.installed", "NDI {version} is installed.", &[("version", version)]),
        None => t("integrations.ndi.installed.unversioned", "NDI is installed.").to_owned(),
    }
}

/// Every wording the row's key can have, so it keeps one width.
fn key_wordings() -> [&'static str; 6] {
    [
        t("integrations.ndi.key.install", "Install NDI"),
        t("integrations.ndi.key.check", "Check for Updates"),
        t("integrations.ndi.key.update", "Install Update"),
        t("integrations.ndi.key.show", "Show\u{2026}"),
        t("integrations.ndi.key.cancel", "Cancel"),
        t("integrations.ndi.key.again", "Try Again"),
    ]
}

/// How the row stands: `found` says whether NDI is on this computer.
pub fn row(download: &mut NdiDownload, found: bool) -> Row {
    let [install, check, update_key, show, cancel, again] = key_wordings();
    let version = download.version();
    let plain = |line: String, under: String, key, act| Row { line, under, key, amber: false, act, bar: None };
    let busy = || {
        plain(
            t("integrations.ndi.busy", "NDI is being installed\u{2026}").to_owned(),
            t("integrations.ndi.busy.under", "Its window is open.").to_owned(),
            show,
            Some(Act::Open),
        )
    };
    match &download.stage {
        Stage::Fetching if download.update => {
            let shared = &download.shared;
            if shared.checking.load(std::sync::atomic::Ordering::Relaxed) {
                let line = t("integrations.ndi.installing", "Installing NDI\u{2026}").to_owned();
                return plain(line, String::new(), cancel, None);
            }
            let done = shared.done.load(std::sync::atomic::Ordering::Relaxed);
            let total = shared.total.load(std::sync::atomic::Ordering::Relaxed);
            let part = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
            let percent = crate::i18n::percent((part * 100.0).floor() as u64);
            let under = if total > 0 {
                tf(
                    "integrations.ndi.of",
                    "{done} of {total} MB",
                    &[("done", &megabytes(done)), ("total", &megabytes(total))],
                )
            } else {
                tf("integrations.ndi.of.unsized", "{done} MB", &[("done", &megabytes(done))])
            };
            Row {
                line: tf("integrations.ndi.updating", "Updating NDI\u{2026} {percent}", &[("percent", &percent)]),
                under,
                key: cancel,
                amber: false,
                act: Some(Act::Cancel),
                bar: Some(part),
            }
        }
        Stage::Installing if download.update => {
            if way().installer() {
                plain(
                    t("integrations.ndi.installer", "NDI\u{2019}s installer is open. Follow its steps.").to_owned(),
                    t("integrations.ndi.installer.row", "Windows asks for an administrator\u{2019}s permission.")
                        .to_owned(),
                    cancel,
                    None,
                )
            } else {
                plain(
                    t("integrations.ndi.installing", "Installing NDI\u{2026}").to_owned(),
                    String::new(),
                    cancel,
                    None,
                )
            }
        }
        Stage::Failed { problem, .. } if download.update => Row {
            line: failed_line(problem, true),
            under: match &version {
                Some(version) => {
                    tf("integrations.ndi.still", "NDI {version} is still installed.", &[("version", version)])
                }
                None => t("integrations.ndi.still.unversioned", "NDI is still installed.").to_owned(),
            },
            key: again,
            amber: true,
            act: Some(Act::Update),
            bar: None,
        },
        Stage::Fetching | Stage::Licence(_) | Stage::Installing | Stage::Failed { .. } => busy(),
        Stage::Removing => {
            plain(t("integrations.ndi.removing", "Removing NDI\u{2026}").to_owned(), String::new(), check, None)
        }
        Stage::Idle | Stage::About | Stage::Done { .. } => {
            let waiting = download.waiting().is_some();
            if !found && !waiting {
                let under = if way().installer() {
                    t(
                        "integrations.ndi.none.installer",
                        "SoftPresenter downloads NDI\u{2019}s own installer and opens it.",
                    )
                } else {
                    t("integrations.ndi.none.own", "SoftPresenter downloads it and keeps it in its own folder.")
                };
                let line = t("integrations.ndi.none", "NDI isn\u{2019}t installed on this computer.");
                return plain(line.to_owned(), under.to_owned(), install, Some(Act::Open));
            }
            if download.checking {
                return plain(
                    t("integrations.ndi.checking", "Checking\u{2026}").to_owned(),
                    String::new(),
                    check,
                    None,
                );
            }
            let checked = download.checked().cloned();
            if checked.as_ref().is_some_and(|checked| checked.newer) && !waiting {
                let line = match &version {
                    Some(version) => tf(
                        "integrations.ndi.newer",
                        "A newer NDI may be available. You have {version}.",
                        &[("version", version)],
                    ),
                    None => t("integrations.ndi.newer.unversioned", "A newer NDI may be available.").to_owned(),
                };
                return Row {
                    line,
                    under: String::new(),
                    key: update_key,
                    amber: true,
                    act: Some(Act::Update),
                    bar: None,
                };
            }
            let today = |at: i64| {
                chrono::DateTime::from_timestamp(at, 0)
                    .map(|at| at.with_timezone(&chrono::Local))
                    .filter(|at| at.date_naive() == chrono::Local::now().date_naive())
                    .map(|at| at.format("%-I:%M").to_string())
            };
            let under = if let Some(at) = &download.updated_at {
                tf("integrations.ndi.updated_at", "Updated at {time}.", &[("time", at)])
            } else if waiting {
                t("integrations.ndi.next_start", "Screens sent over NDI use it the next time SoftPresenter starts.")
                    .to_owned()
            } else if let Some(at) = checked.and_then(|checked| today(checked.at)) {
                tf("integrations.ndi.checked_at", "Checked at {time}. No newer NDI was found.", &[("time", &at)])
            } else {
                t("integrations.ndi.not_checked", "Not checked yet").to_owned()
            };
            plain(installed_line(version.as_deref()), under, check, Some(Act::Check))
        }
    }
}

/// What is said once, under the card's rows, after something ended.
fn said(note: &Note) -> (crate::cli_install::Tone, String) {
    use crate::cli_install::Tone;
    match note {
        Note::NotInstalled => (
            Tone::Plain,
            t(
                "integrations.ndi.said.not_installed",
                "NDI wasn\u{2019}t installed. What was downloaded has been deleted.",
            )
            .to_owned(),
        ),
        Note::UpdateCancelled => (
            Tone::Plain,
            t(
                "integrations.ndi.said.update_cancelled",
                "The update was cancelled. What was downloaded has been deleted.",
            )
            .to_owned(),
        ),
        Note::Updated(Some(version)) => (
            Tone::Good,
            tf(
                "integrations.ndi.said.updated",
                "NDI is updated to {version}. Screens sent over NDI use it from the next start.",
                &[("version", version)],
            ),
        ),
        Note::Updated(None) => (
            Tone::Good,
            t(
                "integrations.ndi.said.updated.unversioned",
                "NDI is updated. Screens sent over NDI use it from the next start.",
            )
            .to_owned(),
        ),
        Note::Removed => (Tone::Plain, t("integrations.ndi.said.removed", "NDI was removed.").to_owned()),
        Note::RemovedInUse => (
            Tone::Plain,
            t("integrations.ndi.said.removed_in_use", "NDI was removed. Restart SoftPresenter to stop using it.")
                .to_owned(),
        ),
        Note::NotRemoved(why) => (
            Tone::Care,
            tf("integrations.ndi.said.not_removed", "NDI couldn\u{2019}t all be removed: {why}.", &[("why", why)]),
        ),
        Note::NotChecked => (
            Tone::Care,
            t("integrations.ndi.said.not_checked", "NDI\u{2019}s server couldn\u{2019}t be asked just now.").to_owned(),
        ),
    }
}

/// The question Remove… asks first.
fn remove_question() -> (&'static str, &'static str, &'static str) {
    let message = if way().installer() {
        t(
            "integrations.ndi.remove.question.installer",
            "This runs NDI\u{2019}s own uninstaller, which takes NDI off this PC for every program. Windows asks for an administrator\u{2019}s permission.",
        )
    } else {
        t(
            "integrations.ndi.remove.question.own",
            "This deletes the NDI that SoftPresenter downloaded. Screens sent over NDI stop the next time SoftPresenter starts.",
        )
    };
    (t("integrations.ndi.remove.question", "Remove NDI?"), message, t("integrations.ndi.remove.confirm", "Remove"))
}

/// The rows of NDI's card: its row, Where it's used, Remove NDI where the app has one to
/// remove, what the last thing done said, and the trademark line. True when Screens was asked
/// for.
pub fn rows(ui: &mut egui::Ui, download: &mut NdiDownload, found: bool) -> bool {
    download.poll();
    let wake = {
        let ctx = ui.ctx().clone();
        move || ctx.request_repaint()
    };
    download.ask_version(found, wake.clone());
    download.check_if_due(found, wake.clone());
    let now = row(download, found);
    let wordings = key_wordings();
    let key = Key {
        look: if now.amber { Flat::Amber } else { Flat::Plain },
        ..Key::plain(now.key).enabled(now.act.is_some()).as_wide_as(&wordings)
    };
    let width = ui.available_width();
    let beside = parts::room_beside_keys(ui, width, &[key]) >= parts::LABEL_ROOM;
    let note = if now.under.is_empty() { now.line.clone() } else { format!("{}\n{}", now.line, now.under) };
    let (clicked, words) = parts::keys_row_keeping(ui, "ndi", "NDI", (&note, 2), &[key], beside);
    // An update's bar: a thin line along the foot of the row's words, in room the row has.
    if let Some(part) = now.bar {
        let track = Rect::from_min_size(pos2(words.min.x, words.max.y + 4.0), vec2(words.width(), 4.0));
        ui.painter().rect_filled(track, 2.0, BUTTON);
        ui.painter().rect_filled(Rect::from_min_size(track.min, vec2(track.width() * part, 4.0)), 2.0, TEXT);
    }
    if clicked.is_some() {
        match now.act {
            Some(Act::Open) => download.open(Place::Settings),
            Some(Act::Check) => download.check(wake.clone()),
            Some(Act::Update) => download.update(wake.clone()),
            Some(Act::Cancel) => download.cancel(),
            None => {}
        }
    }
    // Where it's used: a key to Screens.
    let screens = t("integrations.ndi.where.key", "Screens");
    let to_screens = parts::keys_row(
        ui,
        "ndi where",
        t("integrations.ndi.where", "Where it\u{2019}s used"),
        t(
            "integrations.ndi.where.about",
            "A screen\u{2019}s \u{201c}Where does it go?\u{201d} can be NDI. In Edit, an NDI Source box shows a camera or another computer in a slide.",
        ),
        None,
        &[Key::plain(screens)],
    )
    .is_some();
    // Remove NDI: Remove…, with a question first.
    if download.removable() {
        let about = if way().installer() {
            t(
                "integrations.ndi.remove.about.installer",
                "Runs NDI\u{2019}s own uninstaller. Windows asks for an administrator\u{2019}s permission.",
            )
        } else {
            t("integrations.ndi.remove.about.own", "Deletes the NDI that SoftPresenter downloaded")
        };
        let remove = Key::plain(t("integrations.ndi.remove.key", "Remove\u{2026}")).enabled(!download.busy());
        let asking = egui::Id::unique("removing ndi");
        if parts::keys_row(ui, "ndi remove", t("integrations.ndi.remove", "Remove NDI"), about, None, &[remove])
            .is_some()
        {
            ui.data_mut(|data| data.insert_temp(asking, true));
        }
        if ui.data(|data| data.get_temp::<bool>(asking)).unwrap_or(false) {
            let (question, message, confirm) = remove_question();
            if let Some(answer) =
                crate::studio::alert(ui.ctx(), "remove ndi", crate::icons::Icon::Close, question, message, confirm)
            {
                ui.data_mut(|data| data.remove::<bool>(asking));
                if answer {
                    download.remove(wake);
                }
            }
        }
    }
    ui.add_space(12.0);
    let trademark = t("integrations.ndi.trademark", "NDI\u{ae} is a registered trademark of Vizrt NDI AB.");
    let told = download.note.as_ref().map(said);
    parts::said_line(ui, told.as_ref().map(|(tone, words)| (*tone, words.as_str())), trademark);
    if download.busy() || download.checking {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    to_screens
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::super::check::{Checked, Marks};
    use super::super::{Installed, Step};
    use super::*;

    /// A card on a computer with nothing of the app's on disk, in a folder of the test's own.
    fn fresh(name: &str) -> NdiDownload {
        let data = std::env::temp_dir().join(format!("softpresenter-ndi-row-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        NdiDownload {
            data: Some(data),
            installed: Some(None),
            waiting: Some(None),
            checked: Some(None),
            ..NdiDownload::default()
        }
    }

    fn installed(version: &str) -> Option<Option<Installed>> {
        Some(Some(Installed { version: version.to_owned(), ..Installed::default() }))
    }

    #[test]
    fn not_installed_it_offers_to_install() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("none");
            let row = row(&mut download, false);
            assert_eq!(row.line, "NDI isn\u{2019}t installed on this computer.");
            assert_eq!((row.key, row.act, row.amber, row.bar), ("Install NDI", Some(Act::Open), false, None));
            assert!(row.under.starts_with("SoftPresenter downloads"));
        });
    }

    #[test]
    fn installed_it_says_the_version_and_when_it_was_checked() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("installed");
            download.installed = installed("6.1.1");
            let now = row(&mut download, true);
            assert_eq!((now.line.as_str(), now.under.as_str()), ("NDI 6.1.1 is installed.", "Not checked yet"));
            assert_eq!((now.key, now.act), ("Check for Updates", Some(Act::Check)));
            // Installed some other way, before the runtime has said its version.
            let mut outside = fresh("outside");
            assert_eq!(row(&mut outside, true).line, "NDI is installed.");
            outside.asked_version = Some(Some("6.4.0.2".to_owned()));
            assert_eq!(row(&mut outside, true).line, "NDI 6.4.0.2 is installed.");
            // A runtime in the app's folder with no note of its version (put there by hand, or
            // its note lost) claims none: the runtime itself is asked, as for any other.
            let mut bare = fresh("bare");
            bare.installed = Some(Some(Installed::default()));
            assert_eq!(bare.version(), None);
            assert_eq!(row(&mut bare, true).line, "NDI is installed.");
            // Checked today: when, and that nothing newer was found.
            download.checked =
                Some(Some(Checked { at: chrono::Utc::now().timestamp(), newer: false, marks: Marks::default() }));
            let now = row(&mut download, true);
            assert!(
                now.under.starts_with("Checked at ") && now.under.ends_with(". No newer NDI was found."),
                "{}",
                now.under
            );
            // Checked on another day, it doesn't give a time as if it were today's.
            download.checked = Some(Some(Checked { at: 1_700_000_000, newer: false, marks: Marks::default() }));
            assert_eq!(row(&mut download, true).under, "Not checked yet");
            // While a check runs the key is greyed.
            download.checking = true;
            let now = row(&mut download, true);
            assert_eq!((now.line.as_str(), now.key, now.act), ("Checking\u{2026}", "Check for Updates", None));
        });
    }

    #[test]
    fn a_newer_ndi_is_only_ever_maybe_and_its_key_is_amber() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("newer");
            download.installed = installed("6.1.1");
            download.checked = Some(Some(Checked { at: 1, newer: true, marks: Marks::default() }));
            let now = row(&mut download, true);
            assert_eq!(now.line, "A newer NDI may be available. You have 6.1.1.");
            assert_eq!(now.under, "", "no version is claimed for the newer one, and nothing is under the line");
            assert_eq!((now.key, now.amber, now.act), ("Install Update", true, Some(Act::Update)));
        });
    }

    #[test]
    fn an_update_runs_in_the_row_with_its_bar_and_cancel() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("updating");
            download.installed = installed("6.1.1");
            download.update = true;
            download.stage = Stage::Fetching;
            download.shared.total.store(68_000_000, Ordering::Relaxed);
            download.shared.done.store(30_600_000, Ordering::Relaxed);
            let now = row(&mut download, true);
            assert_eq!((now.line.as_str(), now.under.as_str()), ("Updating NDI\u{2026} 45%", "31 of 68 MB"));
            assert_eq!((now.key, now.act), ("Cancel", Some(Act::Cancel)));
            assert!((now.bar.unwrap() - 0.45).abs() < 0.001);
            // Read through and unpacked: the key is greyed.
            download.shared.checking.store(true, Ordering::Relaxed);
            let now = row(&mut download, true);
            assert_eq!(
                (now.line.as_str(), now.key, now.act, now.bar),
                ("Installing NDI\u{2026}", "Cancel", None, None)
            );
            download.stage = Stage::Installing;
            assert_eq!(row(&mut download, true).act, None);
            // A failure stays in the row, with what is still installed and an amber Try Again.
            download.stage = Stage::Failed { problem: Problem::Blocked, step: Step::Download };
            let now = row(&mut download, true);
            assert_eq!(
                now.line,
                "NDI couldn\u{2019}t be updated: this network checks secure connections and blocked it."
            );
            assert_eq!(now.under, "NDI 6.1.1 is still installed.");
            assert_eq!((now.key, now.amber, now.act), ("Try Again", true, Some(Act::Update)));
            // Done: the installed row again, saying when.
            download.stage = Stage::Idle;
            download.update = false;
            download.installed = installed("6.2.0");
            download.updated_at = Some("2:07".to_owned());
            let now = row(&mut download, true);
            assert_eq!((now.line.as_str(), now.under.as_str()), ("NDI 6.2.0 is installed.", "Updated at 2:07."));
            assert_eq!(now.key, "Check for Updates");
        });
    }

    #[test]
    fn a_first_install_under_way_shows_its_window_again() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("busy");
            for stage in
                [Stage::Fetching, Stage::Installing, Stage::Failed { problem: Problem::Offline, step: Step::Download }]
            {
                download.stage = stage;
                let now = row(&mut download, false);
                assert_eq!(now.line, "NDI is being installed\u{2026}");
                assert_eq!(
                    (now.under.as_str(), now.key, now.act),
                    ("Its window is open.", "Show\u{2026}", Some(Act::Open))
                );
            }
            // The window's first and last steps are not "being installed".
            download.stage = Stage::About;
            assert_eq!(row(&mut download, false).key, "Install NDI");
        });
    }

    #[test]
    fn a_new_ndi_waiting_for_the_next_start_is_the_one_named() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("waiting");
            download.installed = installed("6.1.1");
            download.waiting = installed("6.2.0");
            download.checked = Some(Some(Checked { at: 1, newer: true, marks: Marks::default() }));
            let now = row(&mut download, true);
            assert_eq!(now.line, "NDI 6.2.0 is installed.");
            assert_eq!(now.under, "Screens sent over NDI use it the next time SoftPresenter starts.");
            assert_eq!(now.key, "Check for Updates", "nothing newer is offered over the one waiting");
        });
    }

    #[test]
    fn each_failure_has_its_own_sentence() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let line = |problem: Problem| failed_line(&problem, false);
            assert_eq!(line(Problem::Offline), "NDI couldn\u{2019}t be downloaded: there is no internet connection.");
            assert_eq!(
                line(Problem::Blocked),
                "NDI couldn\u{2019}t be downloaded: this network checks secure connections and blocked it."
            );
            assert_eq!(line(Problem::DiskFull), "NDI couldn\u{2019}t be saved: this computer\u{2019}s disk is full.");
            for wrong in [
                Problem::Layout(String::new()),
                Problem::Damaged(String::new()),
                Problem::TooLarge,
                Problem::Unsigned(String::new()),
            ] {
                assert_eq!(
                    line(wrong),
                    "NDI couldn\u{2019}t be downloaded: the download wasn\u{2019}t what NDI usually sends."
                );
            }
            assert_eq!(
                line(Problem::Closed),
                "NDI wasn\u{2019}t installed: NDI\u{2019}s installer was closed before it finished."
            );
            // What fits none of the five is said as what it is, never as "no internet".
            assert!(line(Problem::Network("NDI's server answered 503".to_owned())).contains("server"));
            assert!(line(Problem::NoRuntime("riscv64".to_owned())).contains("riscv64"));
            assert!(line(Problem::Disk("read-only".to_owned())).starts_with("NDI couldn\u{2019}t be saved"));
            assert_eq!(failed_under(&Problem::Blocked), "Try another network, such as a phone\u{2019}s hotspot.");
            assert_eq!(failed_under(&Problem::Offline), "Nothing was installed.");
            assert!(failed_under(&Problem::Closed).contains("Try Again opens NDI\u{2019}s installer again"));
        });
    }

    #[test]
    fn the_rows_draw_in_every_state_and_keep_their_height() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = egui::Context::default();
            let mut download = fresh("draw");
            let mut heights = Vec::new();
            let draw = |download: &mut NdiDownload, found: bool| {
                let mut height = 0.0;
                let mut drawn = ctx.run_ui(egui::RawInput::default(), |ui| {
                    ui.set_width(700.0);
                    super::rows(ui, download, found);
                    height = ui.min_rect().height();
                });
                drawn.textures_delta.clear();
                height
            };
            heights.push(draw(&mut download, false));
            download.installed = installed("6.1.1");
            heights.push(draw(&mut download, true));
            download.checked = Some(Some(Checked { at: 1, newer: true, marks: Marks::default() }));
            heights.push(draw(&mut download, true));
            download.update = true;
            download.stage = Stage::Fetching;
            download.shared.total.store(68_000_000, Ordering::Relaxed);
            download.shared.done.store(30_600_000, Ordering::Relaxed);
            heights.push(draw(&mut download, true));
            download.stage = Stage::Installing;
            heights.push(draw(&mut download, true));
            download.stage = Stage::Failed { problem: Problem::Offline, step: Step::Download };
            heights.push(draw(&mut download, true));
            // With NDI in the app's folder every one of these has the same rows under it, so the
            // whole is as tall whenever NDI's own row is: it keeps its height through an update.
            let installed_heights = &heights[1..];
            assert!(installed_heights.iter().all(|height| (height - installed_heights[0]).abs() < 0.5), "{heights:?}");
        });
    }

    #[test]
    fn the_cards_key_opens_the_window_in_settings() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = egui::Context::default();
            let mut download = fresh("open");
            let clicked = super::super::testing::click(&ctx, "Install NDI", &mut |ui| {
                super::rows(ui, &mut download, false);
            });
            assert!(clicked, "the key is drawn");
            assert_eq!(download.shown_in(), Some(Place::Settings));
            assert!(matches!(download.stage, Stage::About), "at its first step: nothing is downloaded yet");
        });
    }
}
