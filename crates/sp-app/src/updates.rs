//! Check for Updates: the app menu's item on a Mac, the Updates row of Settings › About on every
//! system, and `sp update check`. A check is made only when someone asks for one, never in the
//! background, and one at a time: asking again while one runs joins it.
//!
//! A build from the public source asks GitHub for this repository's releases (pre-releases
//! too, as the app is still 0.x), takes the highest version among them (a release whose tag
//! isn't a version, such as the Bibles' `bibles-4`, is passed over) and compares it with its own.
//! It downloads and installs nothing: a newer version is offered as its release page, opened in
//! the browser. Nothing that tells who is asking is sent, only "SoftPresenter/<version>" as the
//! User-Agent GitHub requires.
//!
//! A development build answers from its own updater instead, through [`set_hook`]: the hook is
//! one function that blocks until it knows, and returns what it found. The thread, the
//! one-at-a-time rule, what is shown and every sentence are this module's.

use std::cmp::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;

use crate::i18n::{t, tf};

/// A development build's own version ("0.9.0-dev.15"), set when it is built (an empty one counts
/// as none).
pub const DEV_VERSION: Option<&str> = match option_env!("SOFTPRESENTER_DEV_VERSION") {
    Some(version) if !version.is_empty() => Some(version),
    _ => None,
};

/// A pre-release's own version ("0.9.0-dev.16"), set when a build from the public source is made
/// for GitHub's releases (`SOFTPRESENTER_RELEASE_VERSION`; an empty one counts as none). Unlike a
/// development build's, it changes nothing but the version: without it a pre-release would call
/// itself 0.9.0, which comes after every 0.9.0-dev.N, and never be told of a newer one.
const RELEASE_VERSION: Option<&str> = match option_env!("SOFTPRESENTER_RELEASE_VERSION") {
    Some(version) if !version.is_empty() => Some(version),
    _ => None,
};

/// The version this build is, as it shows it everywhere (the About panel, Settings › About, the
/// log, a new issue's text, the MCP server's name) and compares with GitHub's releases: a
/// development build's own, else a pre-release's, else the package's ("0.9.0"). Values other
/// programs or files read as a plain version (a song pack's manifest, what TimelyChurch is told)
/// keep the package's.
pub const VERSION: &str = match (DEV_VERSION, RELEASE_VERSION) {
    (Some(version), _) | (None, Some(version)) => version,
    (None, None) => env!("CARGO_PKG_VERSION"),
};

/// Where the releases are listed (100 to a page: the Bibles' releases come between the app's).
const RELEASES: &str = "https://api.github.com/repos/esraell/SoftPresenter/releases?per_page=100";
/// Where a release's page must be, to be opened.
const PAGES: &str = "https://github.com/esraell/SoftPresenter/releases/";
/// The most of GitHub's answer that is read.
const MOST_BYTES: u64 = 8 * 1024 * 1024;
/// How long a check may take before it says it couldn't.
const WITHIN: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------------------------
// Versions

/// A version as releases are tagged: `0.9.0`, `v0.9.0-dev.16` (semantic versioning: three
/// numbers, then pre-release parts; build metadata after `+` is ignored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Part>,
}

/// One dot-separated part of a pre-release: a number sorts before a word, numbers by their value
/// and words by their letters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Number(u64),
    Word(String),
}

impl Version {
    /// Reads a version or a release's tag; `None` for anything else (`bibles-4`).
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(version, _)| version);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = core.split('.');
        let mut triple = [0u64; 3];
        for slot in &mut triple {
            *slot = number(numbers.next()?)?;
        }
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|part| {
                    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                        None
                    } else if let Some(number) = number(part) {
                        Some(Part::Number(number))
                    } else {
                        Some(Part::Word(part.to_owned()))
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Version { core: triple, pre })
    }
}

fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl Ord for Version {
    /// Semantic versioning's order: by the three numbers, then a pre-release before its release
    /// (0.9.0-dev.16 before 0.9.0), then pre-releases part by part (dev.9 before dev.10), more
    /// parts after fewer when the rest are the same.
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => self.pre.cmp(&other.pre),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------------------------
// GitHub's releases

/// What is read of a release.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
}

/// The highest version among GitHub's list of releases, as its tag writes it (without the `v`),
/// and its page: the release's own when it is this repository's, else the page its tag would
/// have. `None` when no release is a version.
pub fn newest_release(json: &str) -> Result<Option<(String, String)>, String> {
    let releases: Vec<Release> = serde_json::from_str(json)
        .map_err(|_| t("settings.updates.net.not_releases", "GitHub's answer wasn't a list of releases").to_owned())?;
    let newest = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| Version::parse(&release.tag_name).map(|version| (version, release)))
        .max_by(|a, b| a.0.cmp(&b.0));
    Ok(newest.map(|(_, release)| {
        let tag = release.tag_name.trim();
        let shown = tag.strip_prefix(['v', 'V']).unwrap_or(tag).to_owned();
        let safe = release.html_url.starts_with(PAGES)
            && release.html_url.chars().all(|c| c.is_ascii_graphic())
            && !release.html_url.contains("/../");
        let page = if safe { release.html_url } else { format!("{PAGES}tag/{tag}") };
        (shown, page)
    }))
}

/// What GitHub's list of releases says to a build of version `own`.
pub fn verdict(own: &str, json: &str) -> Found {
    match newest_release(json) {
        Err(why) => Found::Failed(why),
        Ok(Some((version, page)))
            if Version::parse(own).is_some_and(|own| Version::parse(&version).is_some_and(|newest| newest > own)) =>
        {
            Found::Newer { version, page }
        }
        Ok(_) => Found::Newest,
    }
}

/// A server on this computer, for a test: `http` to 127.0.0.1 or localhost only.
fn on_this_computer(address: &str) -> bool {
    ["http://127.0.0.1:", "http://127.0.0.1/", "http://localhost:", "http://localhost/"]
        .iter()
        .any(|start| address.to_ascii_lowercase().starts_with(start))
}

/// Asks GitHub, and says what its answer means for this build.
fn ask_github() -> Found {
    // Testing aid: SOFTPRESENTER_RELEASES_URL=http://127.0.0.1:<port>/<path> asks a server on this
    // computer for the list of releases instead of GitHub (any other address is ignored), to see
    // a newer version offered, or a broken answer, without a release being made.
    let local = std::env::var("SOFTPRESENTER_RELEASES_URL").ok().filter(|address| on_this_computer(address));
    let address = local.as_deref().unwrap_or(RELEASES);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(WITHIN))
        .https_only(local.is_none())
        .max_redirects(2)
        .http_status_as_error(false)
        .user_agent(format!("SoftPresenter/{VERSION}"))
        .build()
        .into();
    let mut response = match agent
        .get(address)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
    {
        Ok(response) => response,
        Err(error) => return Found::Failed(unreachable(&error)),
    };
    match response.status().as_u16() {
        200 => {}
        403 | 429 => {
            return Found::Failed(
                t("settings.updates.net.busy", "GitHub is taking fewer checks just now; try again later").to_owned(),
            );
        }
        code => {
            return Found::Failed(tf(
                "settings.updates.net.answered",
                "GitHub answered {code}",
                &[("code", &code.to_string())],
            ));
        }
    }
    match response.body_mut().with_config().limit(MOST_BYTES).read_to_string() {
        Ok(json) => verdict(VERSION, &json),
        Err(error) => Found::Failed(unreachable(&error)),
    }
}

/// Why GitHub couldn't be asked, in a few words.
fn unreachable(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => t("settings.updates.net.timeout", "GitHub took too long to answer"),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            t("settings.updates.net.offline", "there is no connection to GitHub")
        }
        ureq::Error::Tls(_) => t("settings.updates.net.tls", "the secure connection to GitHub failed"),
        _ => t("settings.updates.net.other", "GitHub could not be reached"),
    }
    .to_owned()
}

// ---------------------------------------------------------------------------------------------
// What a check found

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// This build is the newest there is.
    Newest,
    /// A newer version on GitHub, and its release page.
    Newer { version: String, page: String },
    /// A development build's updater found a newer development build and is downloading it.
    #[allow(dead_code)] // Made by a development build's updater only.
    DevFetching { version: String },
    /// A development build's updater has a newer development build ready to put in.
    #[allow(dead_code)] // Made by a development build's updater only.
    DevReady { version: String },
    /// The check couldn't be made: why, in a few words.
    Failed(String),
}

impl Found {
    /// What it says, in the language set: the Updates row's line.
    pub fn words(&self) -> String {
        match self {
            Found::Newest if DEV_VERSION.is_some() => tf(
                "settings.updates.newest_dev",
                "You have the newest development version ({version})",
                &[("version", VERSION)],
            ),
            Found::Newest => {
                tf("settings.updates.newest", "You have the newest version ({version})", &[("version", VERSION)])
            }
            Found::Newer { version, .. } => {
                tf("settings.updates.newer", "SoftPresenter {version} is available", &[("version", version)])
            }
            Found::DevFetching { version } => tf(
                "settings.updates.dev_fetching",
                "A new development version ({version}) is downloading; Settings \u{203a} About offers it once it is ready",
                &[("version", version)],
            ),
            Found::DevReady { version } => tf(
                "settings.updates.dev_ready",
                "A new development version is ready ({version}): it goes in when you next quit SoftPresenter, or now with Update and restart in Settings \u{203a} About",
                &[("version", version)],
            ),
            Found::Failed(why) => tf("settings.updates.failed", "Couldn\u{2019}t check: {why}", &[("why", why)]),
        }
    }

    /// What the notices say: the same, and for a newer version where its download page is.
    pub fn notice(&self) -> String {
        match self {
            Found::Newer { version, .. } => tf(
                "settings.updates.newer_notice",
                "SoftPresenter {version} is available. Settings \u{203a} About opens its download page.",
                &[("version", version)],
            ),
            _ => self.words(),
        }
    }

    /// What `sp update check` answers: the same, and for a newer version its page.
    pub fn reply(&self) -> String {
        match self {
            Found::Newer { page, .. } => format!("{}: {page}", self.words()),
            _ => self.words(),
        }
    }

    /// Whether it is trouble (the check couldn't be made).
    pub fn trouble(&self) -> bool {
        matches!(self, Found::Failed(_))
    }

    /// The release page to open, when a newer version was found on GitHub.
    pub fn page(&self) -> Option<&str> {
        match self {
            Found::Newer { page, .. } => Some(page),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// One check at a time

/// A check made some other way than asking GitHub: it blocks until it knows, on the check's own
/// thread, and returns what it found. A development build's updater sets one.
pub type Hook = fn() -> Found;

/// Where Check for Updates stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// A check is running.
    pub checking: bool,
    /// What the last check found, and when (seconds since 1970).
    pub last: Option<(Found, i64)>,
}

/// The checks: where they stand, and the hook, if one is set.
struct Checks {
    status: Mutex<Status>,
    hook: OnceLock<Hook>,
}

impl Checks {
    const fn new() -> Checks {
        Checks { status: Mutex::new(Status { checking: false, last: None }), hook: OnceLock::new() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks a check begun, unless one is running: whether this call began it.
    fn begin(&self) -> bool {
        let mut status = self.lock();
        !std::mem::replace(&mut status.checking, true)
    }

    /// Keeps what a check found, and when, and marks it over.
    fn finish(&self, found: Found, time: i64) {
        let mut status = self.lock();
        status.checking = false;
        status.last = Some((found, time));
    }

    fn status(&self) -> Status {
        self.lock().clone()
    }

    /// Makes the check: through the hook when one is set, else by `ask`.
    fn run(&self, ask: impl FnOnce() -> Found) -> Found {
        match self.hook.get() {
            Some(hook) => hook(),
            None => ask(),
        }
    }
}

static CHECKS: Checks = Checks::new();

/// Sets the hook that makes checks instead of asking GitHub (a development build's updater). The
/// first one set stays.
#[allow(dead_code)] // Set by a development build's updater only.
pub fn set_hook(hook: Hook) {
    let _ = CHECKS.hook.set(hook);
}

/// Where Check for Updates stands now. Never waits on a check.
pub fn status() -> Status {
    CHECKS.status()
}

/// Starts a check on a thread of its own, unless one is running; `done` is called on that
/// thread once it has found something ([`status`] then has it). Returns whether this call
/// started one (false: it joins the one running, whose `done` says when it is over).
pub fn start(done: impl FnOnce() + Send + 'static) -> bool {
    if !CHECKS.begin() {
        return false;
    }
    let spawned = std::thread::Builder::new().name("check for updates".to_owned()).spawn(move || {
        let found = CHECKS.run(ask_github);
        CHECKS.finish(found, chrono::Utc::now().timestamp());
        done();
    });
    if let Err(error) = spawned {
        CHECKS.finish(
            Found::Failed(tf(
                "settings.updates.no_thread",
                "it could not be started ({error})",
                &[("error", &error.to_string())],
            )),
            chrono::Utc::now().timestamp(),
        );
    }
    true
}

/// The Updates row's line: what the last check found and when, or that none has been made.
pub fn line(status: &Status, form: &crate::clock::TimeForm) -> String {
    match &status.last {
        None => t("settings.updates.never", "Not checked yet").to_owned(),
        Some((found, time)) => tf(
            "settings.updates.checked",
            "{answer} \u{b7} checked at {time}",
            &[("answer", &found.words()), ("time", &crate::notices::time_of_day(*time, form))],
        ),
    }
}

impl crate::App {
    /// Check for Updates, from the menu, Settings › About or `sp update check` (`job`): starts a
    /// check, or joins the one running. Its answer comes as `AppEvent::UpdateChecked`.
    pub(crate) fn check_for_updates(&mut self, job: Option<u64>) {
        if let Some(job) = job {
            self.update_jobs.push(job);
        }
        let waker = self.waker.clone();
        start(move || {
            if let Some(waker) = waker {
                let _ = waker.send_event(crate::AppEvent::UpdateChecked);
            }
        });
        self.redraw_for_updates();
    }

    /// A check has its answer: said in the notices (trouble when it couldn't be made), given to
    /// the `sp update check`s waiting for it, and shown in Settings › About.
    pub(crate) fn finish_update_check(&mut self) {
        let Some((found, _)) = status().last else { return };
        let reply = found.reply();
        for job in std::mem::take(&mut self.update_jobs) {
            if let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job) {
                *kept = Some(if found.trouble() { Err(reply.clone()) } else { Ok(reply.clone()) });
            }
        }
        if let Some(running) = self.running.as_mut() {
            if found.trouble() {
                running.home_state.notices.trouble(found.notice());
            } else {
                running.home_state.notices.info(found.notice());
            }
        }
        self.redraw_for_updates();
    }

    /// The main window and Settings drawn again, for the key and the line that follow a check.
    fn redraw_for_updates(&self) {
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
            for panel in &running.panels {
                panel.window.request_redraw();
            }
        }
    }

    /// `update [status]`: what the last check found and when; `update check`: checks now (or
    /// joins the check running), and the command line waits for the answer.
    pub(crate) fn ctl_update(&mut self, args: &[String]) -> Result<crate::control::Reply, String> {
        const USAGE: &str = "update [status] | update check";
        match args.get(1).map(|word| word.to_lowercase()).as_deref() {
            None | Some("status") if args.len() <= 2 => {
                let now = status();
                let form = self.running.as_ref().map(|r| r.home_state.clock.known().clone()).unwrap_or_default();
                let mut said = match &now.last {
                    // As the Updates row says it, with a newer version's page.
                    Some((found, time)) => tf(
                        "settings.updates.checked",
                        "{answer} \u{b7} checked at {time}",
                        &[("answer", &found.reply()), ("time", &crate::notices::time_of_day(*time, &form))],
                    ),
                    None => line(&now, &form),
                };
                if now.checking {
                    said = format!("{} {said}", t("settings.updates.checking", "Checking\u{2026}"));
                }
                Ok(crate::control::Reply::with(
                    said,
                    serde_json::json!({
                        "version": VERSION,
                        "checking": now.checking,
                        "checked": now.last.as_ref().map(|(_, time)| time),
                        "newer": now.last.as_ref().and_then(|(found, _)| match found {
                            Found::Newer { version, .. } | Found::DevFetching { version } | Found::DevReady { version } => Some(version),
                            _ => None,
                        }),
                        "page": now.last.as_ref().and_then(|(found, _)| found.page()),
                    }),
                ))
            }
            Some("check") if args.len() == 2 => {
                let job = self.start_job();
                self.check_for_updates(Some(job));
                Ok(crate::control::Reply::with(
                    t("settings.updates.checking", "Checking\u{2026}"),
                    serde_json::json!({ "wait": true }),
                ))
            }
            _ => Err(USAGE.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} is a version"))
    }

    #[test]
    fn versions_are_ordered_as_semantic_versioning_orders_them() {
        // Development builds of one version, by their number (not their letters: dev.10 is
        // after dev.9).
        assert!(v("0.9.0-dev.15") > v("0.9.0-dev.14"));
        assert!(v("0.9.0-dev.10") > v("0.9.0-dev.9"));
        assert_eq!(v("v0.9.0-dev.14"), v("0.9.0-dev.14"));
        // Plain versions.
        assert!(v("0.9.1") > v("0.9.0"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        // A pre-release comes before its release, and after the version before.
        assert!(v("0.9.0") > v("0.9.0-dev.16"));
        assert!(v("0.9.1-dev.1") > v("0.9.0"));
        // More parts after fewer; a number before a word.
        assert!(v("0.9.0-dev.1.1") > v("0.9.0-dev.1"));
        assert!(v("0.9.0-rc.1") > v("0.9.0-dev.1"));
        assert!(v("0.9.0-dev") > v("0.9.0-1"));
        // Build metadata is no part of the order.
        assert_eq!(v("0.9.0+mac").cmp(&v("0.9.0")), Ordering::Equal);
    }

    #[test]
    fn a_tag_that_isnt_a_version_is_none() {
        for tag in
            ["bibles-4", "bibles", "", "v", "0.9", "0.9.0.1", "0.9.x", "0.9.0-", "0.9.0-dev..1", "0.9.0-dé.1", "latest"]
        {
            assert_eq!(Version::parse(tag), None, "{tag:?}");
        }
    }

    /// GitHub's list as it is now: only the Bibles' releases, newest first.
    const ONLY_BIBLES: &str = include_str!("testdata/releases-bibles.json");
    /// The same with the app's own releases among them, not in version order.
    const WITH_VERSIONS: &str = include_str!("testdata/releases-versions.json");

    #[test]
    fn the_releases_list_is_read_for_its_highest_version() {
        assert_eq!(newest_release(ONLY_BIBLES), Ok(None), "the Bibles' releases are passed over");
        assert_eq!(verdict("0.9.0", ONLY_BIBLES), Found::Newest);
        assert_eq!(verdict("0.9.0-dev.14", ONLY_BIBLES), Found::Newest);
        let newest = newest_release(WITH_VERSIONS).unwrap();
        assert_eq!(
            newest,
            Some((
                "0.9.0-dev.16".to_owned(),
                "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16".to_owned()
            )),
            "the highest version, not the first listed, and never the draft"
        );
        assert_eq!(
            verdict("0.9.0-dev.14", WITH_VERSIONS),
            Found::Newer {
                version: "0.9.0-dev.16".to_owned(),
                page: "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16".to_owned()
            }
        );
        assert_eq!(verdict("0.9.0-dev.16", WITH_VERSIONS), Found::Newest, "the same is not newer");
        assert_eq!(verdict("0.9.0", WITH_VERSIONS), Found::Newest, "a release is newer than its pre-releases");
        assert!(matches!(verdict("0.9.0", "{\"message\": \"Not Found\"}"), Found::Failed(_)));
        assert!(matches!(verdict("0.9.0", "<html>"), Found::Failed(_)));
    }

    #[test]
    fn a_page_elsewhere_is_never_opened() {
        let json = r#"[{"tag_name": "v0.9.1", "html_url": "https://example.org/SoftPresenter/releases/tag/v0.9.1"},
                      {"tag_name": "v0.9.0", "html_url": "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0"}]"#;
        assert_eq!(
            newest_release(json),
            Ok(Some(("0.9.1".to_owned(), "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.1".to_owned())))
        );
        let json =
            r#"[{"tag_name": "v1.0.0", "html_url": "https://github.com/esraell/SoftPresenter/releases/../../other"}]"#;
        assert_eq!(
            newest_release(json).unwrap().unwrap().1,
            "https://github.com/esraell/SoftPresenter/releases/tag/v1.0.0"
        );
    }

    #[test]
    fn only_a_server_on_this_computer_stands_in_for_github() {
        assert!(on_this_computer("http://127.0.0.1:4330/releases"));
        assert!(on_this_computer("http://localhost/releases"));
        for address in
            ["http://localhost.example.org/releases", "http://127.0.0.10/", "https://example.org/", "ftp://127.0.0.1/"]
        {
            assert!(!on_this_computer(address), "{address}");
        }
    }

    #[test]
    fn one_check_at_a_time_and_its_answer_is_kept() {
        let checks = Checks::new();
        assert!(checks.begin(), "the first begins");
        assert!(!checks.begin(), "a second joins it");
        assert!(checks.status().checking);
        checks.finish(Found::Newest, 1_764_000_000);
        assert_eq!(checks.status(), Status { checking: false, last: Some((Found::Newest, 1_764_000_000)) });
        assert!(checks.begin(), "once it is over, another can begin");
        assert_eq!(checks.status().last, Some((Found::Newest, 1_764_000_000)), "the last answer stays meanwhile");
    }

    #[test]
    fn github_is_asked_unless_a_hook_is_set() {
        let checks = Checks::new();
        let asked = || Found::Failed("asked GitHub".to_owned());
        assert_eq!(checks.run(asked), Found::Failed("asked GitHub".to_owned()), "the default asks GitHub");
        fn hook() -> Found {
            Found::DevReady { version: "0.9.0-dev.15".to_owned() }
        }
        let _ = checks.hook.set(hook);
        assert_eq!(checks.run(asked), hook(), "a hook answers instead");
    }

    #[test]
    fn the_version_shown_is_a_development_builds_own_or_the_packages() {
        match DEV_VERSION {
            // A build from the public source (a pre-release's version, or the package's).
            None => {
                assert_eq!(VERSION, RELEASE_VERSION.unwrap_or(env!("CARGO_PKG_VERSION")));
                assert_eq!(Found::Newest.words(), format!("You have the newest version ({VERSION})"));
            }
            // Tests built as a development build (SOFTPRESENTER_DEV_VERSION set).
            Some(dev) => {
                assert_eq!(VERSION, dev);
                assert_eq!(Found::Newest.words(), format!("You have the newest development version ({dev})"));
            }
        }
        assert!(Version::parse(VERSION).is_some(), "{VERSION} is a version the check can compare");
    }

    #[test]
    fn each_answer_has_its_words() {
        let newer = Found::Newer {
            version: "0.9.0-dev.16".to_owned(),
            page: "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16".to_owned(),
        };
        assert_eq!(newer.words(), "SoftPresenter 0.9.0-dev.16 is available");
        assert_eq!(
            newer.reply(),
            "SoftPresenter 0.9.0-dev.16 is available: https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16"
        );
        assert!(newer.notice().contains("download page"));
        assert_eq!(newer.page(), Some("https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16"));
        let failed = Found::Failed("GitHub took too long to answer".to_owned());
        assert_eq!(failed.words(), "Couldn\u{2019}t check: GitHub took too long to answer");
        assert!(failed.trouble() && !newer.trouble() && !Found::Newest.trouble());
        assert_eq!(Found::Newest.page(), None);
    }
}
