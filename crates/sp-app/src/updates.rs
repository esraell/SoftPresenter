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
//! one function that blocks until it knows, and returns what it found. An updater that fetches
//! a newer version itself says how far it is with [`report`] (so much of so much, ready, or why
//! it failed, as a [`Trouble`], which [`trouble_of`] makes from a network error), from any
//! thread and at any time, and gives the key that puts a ready version in what it does with
//! [`set_install`], and stops when the row's Cancel asks ([`cancel_asked`]). The thread, the one-at-a-time rule, the Updates row ([`row`]), what is shown
//! ([`view`]) and every sentence are this module's: an updater never writes words.
//!
//! Nothing here is the window's: a check and a download go on with Settings closed, and
//! [`take_notice`] has what the notices are to say once a version is ready.

use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
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

/// Whether `version` is a pre-release (`0.9.0-dev.16`, `1.0.0-beta.1`): a version with a
/// pre-release part after its three numbers. A release (`0.9.0`, `1.0.0`) isn't, nor is anything
/// that isn't a version.
pub fn is_pre_release(version: &str) -> bool {
    Version::parse(version).is_some_and(|version| !version.pre.is_empty())
}

/// What a pre-release says of itself where people see its version (the setup's first step,
/// Settings › About): that it is one, with its version, and what that means. `None` for a
/// release, so from 1.0 on nothing is said, with nothing to change here.
pub fn pre_release_words(version: &str) -> Option<(String, &'static str)> {
    is_pre_release(version).then(|| {
        (
            tf("settings.about.prerelease.title", "This is a pre-release ({version})", &[("version", version)]),
            t(
                "settings.about.prerelease.about",
                "Things may still change: the core of how SoftPresenter works and looks will stay, and bugs will be fixed in the versions that come next.",
            ),
        )
    })
}

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
        Err(error) => return Found::Unreached(trouble_of(&error)),
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
        Err(error) => Found::Unreached(trouble_of(&error)),
    }
}

// ---------------------------------------------------------------------------------------------
// Why it couldn't

/// Why a check couldn't be made, or a newer version couldn't be fetched or was refused: a kind,
/// so that the sentences stay here whoever found it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// No connection to the internet (no network, no route, the name not found, refused).
    NoNetwork,
    /// The network stands in the middle of secure connections with a certificate of its own
    /// (a school's, an office's or a hotel's filter), which is never trusted: nothing was
    /// fetched over it.
    Blocked,
    /// It took too long.
    Timeout,
    /// The secure connection failed some other way.
    Insecure,
    /// The server couldn't be reached, or the connection broke off.
    Unreached,
    /// What was fetched isn't signed as it must be. Nothing was changed.
    #[allow(dead_code)] // Found by an updater that fetches (a development build's) only.
    NotSigned,
    /// What was fetched isn't the file it should be. Nothing was changed.
    #[allow(dead_code)] // Found by an updater that fetches (a development build's) only.
    Damaged,
    /// Something else, in a few plain words ("the disk is full").
    #[allow(dead_code)] // Said by an updater that fetches (a development build's) only.
    Other(String),
}

impl Trouble {
    /// Why, in a few words, after "couldn't be downloaded: " (or, for a `check`, "Couldn't
    /// check: ", where it is GitHub that was asked).
    fn why(&self, check: bool) -> String {
        match (self, check) {
            (Trouble::NoNetwork, _) => t("settings.updates.why.no_network", "there is no internet connection"),
            (Trouble::Blocked, _) => {
                t("settings.updates.why.blocked", "this network checks secure connections and blocked it")
            }
            (Trouble::Timeout, true) => t("settings.updates.net.timeout", "GitHub took too long to answer"),
            (Trouble::Timeout, false) => t("settings.updates.why.timeout", "it took too long"),
            (Trouble::Insecure, true) => t("settings.updates.net.tls", "the secure connection to GitHub failed"),
            (Trouble::Insecure, false) => t("settings.updates.why.insecure", "the secure connection failed"),
            (Trouble::Unreached, true) => t("settings.updates.net.other", "GitHub could not be reached"),
            (Trouble::Unreached, false) => t("settings.updates.why.unreached", "the server could not be reached"),
            (Trouble::NotSigned, _) => t("settings.updates.why.not_signed", "it isn\u{2019}t signed by SoftPresenter"),
            (Trouble::Damaged, _) => t("settings.updates.why.damaged", "the download was damaged"),
            (Trouble::Other(why), _) => why,
        }
        .to_owned()
    }

    /// What was fetched was refused (nothing was changed), rather than not fetched.
    fn refused(&self) -> bool {
        matches!(self, Trouble::NotSigned | Trouble::Damaged)
    }
}

/// What kind of trouble a network error is, for a check or an updater's download. A
/// certificate nobody vouches for (rustls's `UnknownIssuer`, which reaches here inside an I/O
/// error from the handshake) is a network that checks secure connections ([`Trouble::Blocked`]),
/// not a missing connection: the connection was there, and was refused on purpose. The
/// connection is never retried with less checking.
pub fn trouble_of(error: &ureq::Error) -> Trouble {
    use std::io::ErrorKind as Kind;
    use tokio_rustls::rustls;
    let secure = |error: &rustls::Error| match error {
        rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer) => Trouble::Blocked,
        _ => Trouble::Insecure,
    };
    match error {
        ureq::Error::Timeout(_) => Trouble::Timeout,
        ureq::Error::HostNotFound => Trouble::NoNetwork,
        ureq::Error::Rustls(error) => secure(error),
        ureq::Error::Tls(_) => Trouble::Insecure,
        ureq::Error::Io(error) => match error.get_ref().and_then(|inner| inner.downcast_ref::<rustls::Error>()) {
            Some(error) => secure(error),
            None => match error.kind() {
                Kind::TimedOut => Trouble::Timeout,
                Kind::ConnectionRefused
                | Kind::NetworkUnreachable
                | Kind::HostUnreachable
                | Kind::NetworkDown
                | Kind::AddrNotAvailable
                | Kind::NotConnected => Trouble::NoNetwork,
                _ => Trouble::Unreached,
            },
        },
        _ => Trouble::Unreached,
    }
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
    /// A development build's updater found a newer development build and is downloading it
    /// (how far it is comes through [`report`]).
    #[allow(dead_code)] // Made by a development build's updater only.
    DevFetching { version: String },
    /// A development build's updater has a newer development build ready to put in.
    #[allow(dead_code)] // Made by a development build's updater only.
    DevReady { version: String },
    /// The check couldn't be made: why, in a few words.
    Failed(String),
    /// The check couldn't be made: whoever was asked couldn't be reached, and what kind of
    /// trouble that was.
    Unreached(Trouble),
}

impl Found {
    /// What it says, in the language set: the Updates row's sentence.
    pub fn words(&self) -> String {
        let status = Status { last: Some((self.clone(), 0)), ..Status::default() };
        view(&status, &crate::clock::TimeForm::default(), Build::now()).sentence
    }

    /// What the notices say: the same, and for a newer version where its download page is.
    pub fn notice(&self) -> String {
        match self {
            Found::Newer { .. } => format!(
                "{} {}",
                self.words(),
                t("settings.updates.newer_notice", "Settings \u{203a} About opens its download page.")
            ),
            _ => self.words(),
        }
    }

    /// What `sp update check` answers: the same, and for a newer version its page.
    pub fn reply(&self) -> String {
        match self {
            Found::Newer { page, .. } => format!("{} {page}", self.words()),
            _ => self.words(),
        }
    }

    /// Whether it is trouble (the check couldn't be made).
    pub fn trouble(&self) -> bool {
        matches!(self, Found::Failed(_) | Found::Unreached(_))
    }

    /// The release page to open, when a newer version was found on GitHub.
    pub fn page(&self) -> Option<&str> {
        match self {
            Found::Newer { page, .. } => Some(page),
            _ => None,
        }
    }

    /// The newer version it found, whoever fetches it.
    pub fn newer(&self) -> Option<&str> {
        match self {
            Found::Newer { version, .. } | Found::DevFetching { version } | Found::DevReady { version } => {
                Some(version)
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A newer version being fetched

/// Where an updater that fetches a newer version itself has got to with it. A build from the
/// public source never makes one: it only opens the release's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// Coming down: so many bytes of so many (`total` 0 while that isn't known).
    Downloading { version: String, done: u64, total: u64 },
    /// Fetched and checked: it goes in at quit, or now with the row's key.
    Ready { version: String },
    /// Not fetched, or fetched and refused: why, and whether the updater tries again by itself.
    Failed { version: String, why: Trouble, retries: bool },
    /// The download was cancelled with the row's Cancel ([`cancel_asked`]), and what had come
    /// down has been deleted.
    Cancelled { version: String },
}

/// Bytes as whole megabytes, to the nearest, as a download's size is said.
fn megabytes(bytes: u64) -> u64 {
    bytes / 1_000_000 + u64::from(bytes % 1_000_000 >= 500_000)
}

impl Fetch {
    /// How much of a download is done, in whole percent (rounded down, so 100 is all of it);
    /// none while its size isn't known.
    fn percent(&self) -> Option<u64> {
        match self {
            Fetch::Downloading { done, total, .. } if *total > 0 => {
                Some((u128::from(*done) * 100 / u128::from(*total)).min(100) as u64)
            }
            _ => None,
        }
    }

    /// What of it the row shows: a download by its whole percent and megabytes, so that a
    /// report that changes nothing to see wakes nothing.
    fn shown(&self) -> Fetch {
        match self {
            Fetch::Downloading { version, done, total } => Fetch::Downloading {
                version: version.clone(),
                done: megabytes(*done).saturating_mul(1000).saturating_add(self.percent().unwrap_or(0)),
                total: megabytes(*total),
            },
            other => other.clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// What the Updates row shows

/// What the row's key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Check now (Check Now, Try Again): what the Mac's Check for Updates… does.
    Check,
    /// Open a newer version's release page in the browser.
    Open(String),
    /// Put the version that is ready in now: the updater's own doing ([`set_install`]).
    Install,
    /// Stop the download and delete what came down: asked of the updater ([`cancel_asked`]).
    Cancel,
}

/// The row's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub words: &'static str,
    pub act: Act,
    /// Greyed when not.
    pub enabled: bool,
    /// The amber key, which brings a newer version in (Update and Restart, Open the Download
    /// Page).
    pub amber: bool,
}

/// What the Updates row shows in one state: its sentence, the line under it, its key, the bar
/// along the foot of its words while a version comes down, and the line under the key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub sentence: String,
    pub under: String,
    pub key: Option<Key>,
    /// The bar's part filled, 0 to 1, while a version comes down.
    pub bar: Option<f32>,
    pub under_key: &'static str,
}

fn check_key() -> &'static str {
    t("settings.updates.check", "Check Now")
}

fn again_key() -> &'static str {
    t("settings.updates.again", "Try Again")
}

fn open_key() -> &'static str {
    t("settings.updates.open", "Open the Download Page")
}

fn cancel_key() -> &'static str {
    t("settings.updates.cancel", "Cancel")
}

fn install_key() -> &'static str {
    t("settings.updates.install", "Update and Restart")
}

fn key(words: &'static str, act: Act) -> Option<Key> {
    Some(Key { words, act, enabled: true, amber: false })
}

/// The line under a failure: what else to do about it.
fn advice(why: &Trouble, retries: bool) -> String {
    match why {
        Trouble::Blocked => t(
            "settings.updates.advice.blocked",
            "Try another network, or get it from the download page: softpresenter.com.",
        )
        .to_owned(),
        Trouble::NotSigned | Trouble::Damaged => {
            t("settings.updates.advice.refused", "Nothing was changed.").to_owned()
        }
        Trouble::NoNetwork if retries => {
            t("settings.updates.advice.retries", "SoftPresenter tries again by itself.").to_owned()
        }
        Trouble::NoNetwork => String::new(),
        _ => t("settings.updates.advice.page", "Or get it from the download page: softpresenter.com.").to_owned(),
    }
}

/// The row while an updater fetches a newer version: coming down, ready, failed or cancelled.
fn fetch_view(fetch: &Fetch, build: Build) -> View {
    match fetch {
        Fetch::Downloading { version, done, total } => {
            let nothing = t("settings.updates.downloading.under", "Nothing changes until it is ready.");
            let (sentence, under) = match fetch.percent() {
                Some(percent) => (
                    tf(
                        "settings.updates.downloading",
                        "Downloading {version}\u{2026} {percent}%",
                        &[("version", version), ("percent", &percent.to_string())],
                    ),
                    tf(
                        "settings.updates.downloading.of",
                        "{done} of {total} MB. {nothing}",
                        &[
                            ("done", &megabytes(*done).to_string()),
                            ("total", &megabytes(*total).to_string()),
                            ("nothing", nothing),
                        ],
                    ),
                ),
                None => (
                    tf("settings.updates.downloading.none", "Downloading {version}\u{2026}", &[("version", version)]),
                    if *done > 0 {
                        tf(
                            "settings.updates.downloading.some",
                            "{done} MB. {nothing}",
                            &[("done", &megabytes(*done).to_string()), ("nothing", nothing)],
                        )
                    } else {
                        nothing.to_owned()
                    },
                ),
            };
            View {
                sentence,
                under,
                // Greyed once asked, until the updater has stopped.
                key: build.keys.then(|| Key {
                    words: cancel_key(),
                    act: Act::Cancel,
                    enabled: !build.cancelling,
                    amber: false,
                }),
                bar: Some(fetch.percent().unwrap_or(0) as f32 / 100.0),
                ..View::default()
            }
        }
        // With no key of the updater's to put it in now, it goes in at quit, and the row says
        // only that.
        Fetch::Ready { version } if !build.keys => View {
            sentence: tf(
                "settings.updates.ready_at_quit",
                "{version} is ready. It goes in when you quit.",
                &[("version", version)],
            ),
            ..View::default()
        },
        Fetch::Ready { version } => View {
            sentence: tf(
                "settings.updates.ready",
                "{version} is ready. It goes in when you quit, or now.",
                &[("version", version)],
            ),
            key: Some(Key { words: install_key(), act: Act::Install, enabled: !build.waits, amber: true }),
            under_key: if build.waits { t("settings.updates.waits", "Something is on screen") } else { "" },
            ..View::default()
        },
        Fetch::Failed { version, why, retries } => View {
            sentence: if why.refused() {
                tf(
                    "settings.updates.refused",
                    "{version} was refused: {why}.",
                    &[("version", version), ("why", &why.why(false))],
                )
            } else {
                tf(
                    "settings.updates.not_downloaded",
                    "{version} couldn\u{2019}t be downloaded: {why}.",
                    &[("version", version), ("why", &why.why(false))],
                )
            },
            under: advice(why, *retries),
            key: key(again_key(), Act::Check),
            ..View::default()
        },
        Fetch::Cancelled { .. } => View {
            sentence: t("settings.updates.cancelled", "The update was cancelled.").to_owned(),
            under: t("settings.updates.cancelled.under", "What was downloaded has been deleted.").to_owned(),
            key: key(check_key(), Act::Check),
            ..View::default()
        },
    }
}

/// What the row's words depend on besides where the checks stand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Build {
    /// An updater that fetches versions answers the checks (a development build's), rather
    /// than GitHub's releases.
    pub fetches: bool,
    /// Something is on screen, so nothing that restarts SoftPresenter can be pressed.
    pub waits: bool,
    /// The updater answers the row's own keys (Cancel, Update and Restart: [`set_install`]).
    pub keys: bool,
    /// Cancel was pressed, and the updater hasn't stopped yet.
    pub cancelling: bool,
}

impl Build {
    /// This build, now.
    fn now() -> Build {
        Build {
            fetches: updater_set(),
            waits: waits(),
            keys: CHECKS.install.get().is_some() || staged().is_some(),
            cancelling: cancel_asked(),
        }
    }
}

/// What the Updates row shows for where Check for Updates stands: checking; else what an
/// updater is fetching; else what the last check found; else that none has been made.
pub fn view(status: &Status, form: &crate::clock::TimeForm, build: Build) -> View {
    if status.checking {
        return View {
            sentence: t("settings.updates.checking", "Checking\u{2026}").to_owned(),
            key: Some(Key { words: check_key(), act: Act::Check, enabled: false, amber: false }),
            ..View::default()
        };
    }
    if let Some(fetch) = status.fetching() {
        return fetch_view(&fetch, build);
    }
    let failed = |why: String, advice: String| View {
        sentence: tf("settings.updates.failed", "Couldn\u{2019}t check: {why}.", &[("why", &why)]),
        under: advice,
        key: key(again_key(), Act::Check),
        ..View::default()
    };
    match &status.last {
        None => View {
            sentence: if build.fetches {
                String::new()
            } else {
                t("settings.updates.asked_only", "Updates are checked only when you ask.").to_owned()
            },
            under: t("settings.updates.never", "Not checked yet").to_owned(),
            key: key(check_key(), Act::Check),
            ..View::default()
        },
        Some((Found::Newest, time)) => View {
            sentence: tf("settings.updates.newest", "SoftPresenter {version} is the newest.", &[("version", VERSION)]),
            under: tf(
                "settings.updates.checked",
                "Checked at {time}",
                &[("time", &crate::notices::time_of_day(*time, form))],
            ),
            key: key(check_key(), Act::Check),
            ..View::default()
        },
        Some((Found::Newer { version, page }, _)) => View {
            sentence: tf("settings.updates.out", "SoftPresenter {version} is out.", &[("version", version)]),
            under: t("settings.updates.cant", "This copy can\u{2019}t update itself yet.").to_owned(),
            key: Some(Key { words: open_key(), act: Act::Open(page.clone()), enabled: true, amber: true }),
            ..View::default()
        },
        // (An updater's answers are shown as what it is fetching, above.)
        Some((Found::DevFetching { .. } | Found::DevReady { .. }, _)) => View::default(),
        Some((Found::Failed(why), _)) => failed(why.clone(), String::new()),
        Some((Found::Unreached(why), _)) => failed(why.why(true), advice(why, false)),
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
    /// Where an updater that fetches a newer version has got to with it ([`report`]).
    pub fetch: Option<Fetch>,
}

impl Status {
    /// What an updater is fetching: what it last reported, else what its answer to the last
    /// check says (coming down, how far not yet said; or ready).
    pub fn fetching(&self) -> Option<Fetch> {
        match (&self.fetch, &self.last) {
            (Some(fetch), _) => Some(fetch.clone()),
            (None, Some((Found::DevFetching { version }, _))) => {
                Some(Fetch::Downloading { version: version.clone(), done: 0, total: 0 })
            }
            (None, Some((Found::DevReady { version }, _))) => Some(Fetch::Ready { version: version.clone() }),
            _ => None,
        }
    }
}

/// What an updater that fetches versions sets for the key that puts a ready one in: what the
/// key does, and whether it must wait now.
type Install = (fn(), fn() -> bool);

/// What the notices are told, once: the words, and whether it is trouble.
pub type Notice = (String, bool);

/// The checks: where they stand, and what an updater set, if there is one.
struct Checks {
    status: Mutex<Status>,
    hook: OnceLock<Hook>,
    /// What the key that puts a ready version in does, and whether it must wait now.
    install: OnceLock<Install>,
    /// What the notices have been told already (each thing once for each version).
    said: Mutex<Vec<String>>,
    /// Cancel was pressed: the updater is to stop, delete what came down and say so.
    cancel: AtomicBool,
    /// A download was cancelled, and the notices haven't said so yet.
    cancelled: AtomicBool,
}

impl Checks {
    const fn new() -> Checks {
        Checks {
            status: Mutex::new(Status { checking: false, last: None, fetch: None }),
            hook: OnceLock::new(),
            install: OnceLock::new(),
            said: Mutex::new(Vec::new()),
            cancel: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks a check begun, unless one is running: whether this call began it. A failure to
    /// fetch is let go, since this is Try Again; a download going on, or a version ready, stays.
    fn begin(&self) -> bool {
        let mut status = self.lock();
        let began = !std::mem::replace(&mut status.checking, true);
        if began && matches!(status.fetch, Some(Fetch::Failed { .. } | Fetch::Cancelled { .. })) {
            status.fetch = None;
        }
        began
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

    /// Keeps where an updater has got to (`None`: it has nothing in hand any more): whether
    /// what the row shows changed with it.
    fn report(&self, fetch: Option<Fetch>) -> bool {
        // Only a download can be cancelled: once it is anything else, nothing is asked any more,
        // and the row is drawn again for it.
        if !matches!(fetch, Some(Fetch::Downloading { .. })) {
            let asked = self.cancel.swap(false, Relaxed);
            let cancelled = matches!(fetch, Some(Fetch::Cancelled { .. }));
            if cancelled {
                self.cancelled.store(true, Relaxed);
            }
            if asked || cancelled {
                self.lock().fetch = fetch;
                return true;
            }
        }
        let mut status = self.lock();
        let changed = status.fetch.as_ref().map(Fetch::shown) != fetch.as_ref().map(Fetch::shown);
        status.fetch = fetch;
        changed
    }

    /// What the notices are to be told now, if they haven't been: that a version is ready, or
    /// that one was refused (never that there is no network: only the row carries that).
    fn take_notice(&self) -> Option<Notice> {
        if self.cancelled.swap(false, Relaxed) {
            let view = fetch_view(&Fetch::Cancelled { version: String::new() }, Build::default());
            return Some((format!("{} {}", view.sentence, view.under), false));
        }
        let fetch = self.lock().fetching()?;
        let (once, notice) = match &fetch {
            Fetch::Ready { version } => (
                format!("ready {version}"),
                (tf("settings.updates.ready_notice", "Version {version} is ready.", &[("version", version)]), false),
            ),
            Fetch::Failed { version, why, .. } if why.refused() => {
                let view = fetch_view(&fetch, Build::default());
                (format!("refused {version} {why:?}"), (format!("{} {}", view.sentence, view.under), true))
            }
            _ => return None,
        };
        let mut said = self.said.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if said.contains(&once) {
            return None;
        }
        said.push(once);
        Some(notice)
    }
}

static CHECKS: Checks = Checks::new();

/// Tells the window that what the row shows changed (set by the app once it runs).
static WAKER: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Sets the hook that makes checks instead of asking GitHub (a development build's updater). The
/// first one set stays.
#[allow(dead_code)] // Set by a development build's updater only.
pub fn set_hook(hook: Hook) {
    let _ = CHECKS.hook.set(hook);
}

/// An updater that fetches versions answers the row's own keys: it sets what Update and Restart
/// does (`install`: asked on the window's thread, it must not block) and whether it must wait
/// just now (`waits`: something is on screen; the key is then greyed, saying so), and by setting
/// them promises to look at [`cancel_asked`] between the pieces of a download. Without this the
/// row shows neither Cancel nor Update and Restart. The first set stay.
#[allow(dead_code)] // Set by a development build's updater only.
pub fn set_install(install: fn(), waits: fn() -> bool) {
    let _ = CHECKS.install.set((install, waits));
}

/// An updater says where it has got to with a newer version: so much of so much, ready, or
/// failed and why. From any thread, at any time, as often as it likes: the window is woken only
/// when what the row shows changes (a whole percent, a whole megabyte, another state), and the
/// notices say once when a version is ready.
#[allow(dead_code)] // Called by a development build's updater only.
pub fn report(fetch: Fetch) {
    if CHECKS.report(Some(fetch))
        && let Some(wake) = WAKER.get()
    {
        wake();
    }
}

/// Whether the row's Cancel was pressed while a version comes down. The updater looks between
/// the pieces of its download; once it is so, it stops, deletes what came down and reports
/// [`Fetch::Cancelled`] (which lets the asking go, as any report but a download's does).
pub fn cancel_asked() -> bool {
    CHECKS.cancel.load(Relaxed)
}

/// An updater has nothing in hand any more (what it fetched went in, or was dropped): the row
/// goes back to what the last check found.
#[allow(dead_code)] // Called by a development build's updater only.
pub fn forget() {
    if CHECKS.report(None)
        && let Some(wake) = WAKER.get()
    {
        wake();
    }
}

/// Whether an updater that fetches versions answers the checks here (a development build's),
/// rather than GitHub's releases: the states the row can reach differ.
fn updater_set() -> bool {
    CHECKS.hook.get().is_some() || staged().is_some_and(|(status, _)| status.fetch.is_some())
}

/// Whether Update and Restart must wait just now (something is on screen), as the updater says.
fn waits() -> bool {
    match staged() {
        Some((_, waits)) => *waits,
        None => CHECKS.install.get().is_some_and(|(_, waits)| waits()),
    }
}

/// Testing aid: SOFTPRESENTER_UPDATES_SHOW=<state> shows the Updates row in that state, in a test
/// copy (one with `SOFTPRESENTER_DATA`) only, for pictures of states no check on this computer
/// reaches: `never`, `checking`, `newest`, `found`, `check-failed`, `check-blocked`, and an
/// updater's `downloading`, `ready`, `ready-live` (something on screen), `failed`, `blocked`,
/// `refused`, `cancelled`. Nothing is asked of the network for it; Cancel and Update and
/// Restart do nothing.
fn staged() -> Option<&'static (Status, bool)> {
    static STAGED: OnceLock<Option<(Status, bool)>> = OnceLock::new();
    STAGED
        .get_or_init(|| {
            std::env::var_os("SOFTPRESENTER_DATA")?;
            stage(&std::env::var("SOFTPRESENTER_UPDATES_SHOW").ok()?)
        })
        .as_ref()
}

/// The row's state a testing aid's word names, and whether something is on screen in it.
fn stage(word: &str) -> Option<(Status, bool)> {
    // 2:07 in the afternoon of the day this was written, wherever the computer is.
    let at = {
        use chrono::TimeZone;
        chrono::Local.with_ymd_and_hms(2026, 11, 28, 14, 7, 0).single().map_or(0, |time| time.timestamp())
    };
    let newer = "0.9.1".to_owned();
    let found = |found: Found| Status { last: Some((found, at)), ..Status::default() };
    let fetch = |fetch: Fetch| Status { fetch: Some(fetch), ..Status::default() };
    let failed = |why: Trouble| fetch(Fetch::Failed { version: newer.clone(), why, retries: false });
    let status = match word {
        "never" => Status::default(),
        "checking" => Status { checking: true, ..Status::default() },
        "newest" => found(Found::Newest),
        "found" => found(Found::Newer { version: newer.clone(), page: format!("{PAGES}tag/v{newer}") }),
        "check-failed" => found(Found::Unreached(Trouble::NoNetwork)),
        "check-blocked" => found(Found::Unreached(Trouble::Blocked)),
        "downloading" => fetch(Fetch::Downloading { version: newer.clone(), done: 22_680_000, total: 54_000_000 }),
        "ready" | "ready-live" => fetch(Fetch::Ready { version: newer.clone() }),
        "cancelled" => fetch(Fetch::Cancelled { version: newer.clone() }),
        "failed" => failed(Trouble::NoNetwork),
        "blocked" => failed(Trouble::Blocked),
        "refused" => failed(Trouble::NotSigned),
        _ => return None,
    };
    Some((status, word == "ready-live"))
}

/// Where Check for Updates stands now. Never waits on a check.
pub fn status() -> Status {
    match staged() {
        Some((status, _)) => status.clone(),
        None => CHECKS.status(),
    }
}

/// What the notices are to be told now about a version an updater fetched, once each.
pub fn take_notice() -> Option<Notice> {
    CHECKS.take_notice()
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

// ---------------------------------------------------------------------------------------------
// The Updates row of Settings › About

/// The row's sizes: its words' lines, the key, the gap under it and the line under the key, the
/// bar along the foot of the words (and how far under them), and the room above and below.
const LABEL_LINE: f32 = 18.0;
const SENTENCE_LINE: f32 = 19.0;
const UNDER_LINE: f32 = 18.0;
const KEY_HIGH: f32 = 30.0;
const UNDER_KEY_GAP: f32 = 3.0;
const UNDER_KEY_LINE: f32 = 16.0;
const BAR_HIGH: f32 = 4.0;
const BAR_UNDER_WORDS: f32 = 4.0;
const ROOM: f32 = 12.0;

/// Every state the row can be in for this build, to size it by: a build from the public source
/// checks, finds and fails; one whose updater fetches also downloads, has one ready and fails
/// to fetch. With the version in hand, or this one's, in their sentences.
fn reachable(now: &Status, build: Build) -> Vec<Status> {
    let version = now
        .fetching()
        .map(|fetch| match fetch {
            Fetch::Downloading { version, .. }
            | Fetch::Ready { version }
            | Fetch::Failed { version, .. }
            | Fetch::Cancelled { version } => version,
        })
        .or_else(|| now.last.as_ref().and_then(|(found, _)| found.newer().map(str::to_owned)))
        .unwrap_or_else(|| VERSION.to_owned());
    let found = |found: Found| Status { last: Some((found, 0)), ..Status::default() };
    let mut all = vec![
        Status::default(),
        Status { checking: true, ..Status::default() },
        found(Found::Newest),
        found(Found::Unreached(Trouble::Blocked)),
        found(Found::Unreached(Trouble::Timeout)),
    ];
    if build.fetches {
        let fetch = |fetch: Fetch| Status { fetch: Some(fetch), ..Status::default() };
        all.push(fetch(Fetch::Downloading { version: version.clone(), done: 999_000_000, total: 999_000_000 }));
        all.push(fetch(Fetch::Ready { version: version.clone() }));
        all.push(fetch(Fetch::Cancelled { version: version.clone() }));
        for why in [Trouble::NoNetwork, Trouble::Blocked, Trouble::NotSigned, Trouble::Timeout] {
            all.push(fetch(Fetch::Failed { version: version.clone(), why, retries: true }));
        }
    } else {
        all.push(found(Found::Newer { version, page: String::new() }));
    }
    all
}

/// Settings › About's Updates row: one height and one key width in every state, so nothing on
/// the page moves as a check and a download go on. Its name, then the sentence and the line
/// under it (as tall as the tallest of the states this build can reach, laid out at the row's
/// width), with a thin bar along their foot while a version comes down, in the room under
/// them; at the right the key (as wide as its longest wording, amber when it brings a newer
/// version in) and a line under it, which says why the key is greyed. The same check as the Mac's app menu item, which opens this page; on
/// Windows and Linux, which have no menu bar, this is its place.
pub fn row(ui: &mut egui::Ui, state: &mut crate::home::HomeState) {
    draw_row(ui, state, &status(), Build::now());
}

/// The row for where the checks stand (`now`) in a build like `build`; where it was drawn.
fn draw_row(ui: &mut egui::Ui, state: &mut crate::home::HomeState, now: &Status, build: Build) -> egui::Rect {
    use crate::home::settings::parts;
    use crate::studio::{BUTTON, Flat, LINE, TEXT, TEXT_DIM, TEXT_FAINT, flat_button, semibold, widest};
    use egui::{Align2, FontId, Rect, Sense, Stroke, pos2, vec2};
    let form = state.clock.known().clone();
    let shown = view(now, &form, build);
    let all: Vec<View> =
        reachable(now, build).iter().map(|status| view(status, &form, Build { waits: true, ..build })).collect();
    let key_font = FontId::proportional(12.5);
    let mut wordings: Vec<&str> = all.iter().filter_map(|view| view.key.as_ref().map(|key| key.words)).collect();
    wordings.extend(shown.key.as_ref().map(|key| key.words));
    let key_wide = widest(ui, &wordings, &key_font) + 26.0;
    let width = ui.available_width();
    let room = (width - key_wide - 16.0).max(80.0);
    let sentence =
        |ui: &egui::Ui, words: &str| parts::lines(ui, words, FontId::proportional(13.0), TEXT_DIM, room, SENTENCE_LINE);
    let under =
        |ui: &egui::Ui, words: &str| parts::lines(ui, words, FontId::proportional(12.5), TEXT_FAINT, room, UNDER_LINE);
    // As tall as the tallest state's words, the line under kept even when it is empty.
    let words_high = all
        .iter()
        .chain(std::iter::once(&shown))
        .map(|view| {
            sentence(ui, &view.sentence).size().y.max(SENTENCE_LINE)
                + 1.0
                + under(ui, &view.under).size().y.max(UNDER_LINE)
        })
        .fold(0.0, f32::max);
    let text_high = LABEL_LINE + words_high;
    let control_high = KEY_HIGH + UNDER_KEY_GAP + UNDER_KEY_LINE;
    let inner = text_high.max(control_high);
    let (place, _) = ui.allocate_exact_size(vec2(width, inner + 2.0 * ROOM), Sense::hover());
    let painter = ui.painter();
    painter.hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, LINE));
    let top = place.min.y + ROOM + (inner - text_high) / 2.0;
    let label = parts::lines(ui, t("settings.updates.label", "Updates"), semibold(13.0), TEXT, room, LABEL_LINE);
    painter.galley(pos2(place.min.x, top), label, TEXT);
    let line = sentence(ui, &shown.sentence);
    let line_high = line.size().y.max(SENTENCE_LINE);
    painter.galley(pos2(place.min.x, top + LABEL_LINE), line, TEXT_DIM);
    painter.galley(pos2(place.min.x, top + LABEL_LINE + line_high + 1.0), under(ui, &shown.under), TEXT_FAINT);
    // The key, the bar in its place, and the line under it.
    let key_at = pos2(place.max.x - key_wide, place.min.y + ROOM + (inner - control_high) / 2.0);
    let key_place = Rect::from_min_size(key_at, vec2(key_wide, KEY_HIGH));
    state.updates_key = Some(key_place);
    if let Some(filled) = shown.bar {
        let at = pos2(place.min.x, top + text_high + BAR_UNDER_WORDS);
        let bar = Rect::from_min_size(at, vec2(room, BAR_HIGH));
        painter.rect_filled(bar, BAR_HIGH / 2.0, BUTTON);
        let done = Rect::from_min_size(bar.min, vec2(room * filled.clamp(0.0, 1.0), BAR_HIGH));
        painter.rect_filled(done, BAR_HIGH / 2.0, TEXT);
        state.updates_bar = Some(bar);
    } else {
        state.updates_bar = None;
    }
    if !shown.under_key.is_empty() {
        let at = pos2(key_place.center().x, key_place.max.y + UNDER_KEY_GAP + UNDER_KEY_LINE / 2.0);
        painter.text(at, Align2::CENTER_CENTER, shown.under_key, FontId::proportional(11.5), TEXT_FAINT);
    }
    let Some(key) = shown.key else { return place };
    let look = if key.amber { Flat::Amber } else { Flat::Plain };
    let id = ui.scope_id().with("updates key");
    if !flat_button(ui, key_place, id, key.words, key_font, look, key.enabled).clicked() {
        return place;
    }
    match key.act {
        Act::Check => state.menu_asked = Some(crate::menubar::Action::CheckForUpdates),
        Act::Open(page) => {
            if let Err(e) = crate::canva::open_page(&page) {
                state.notices.trouble(tf(
                    "settings.about.not_opened",
                    "Could not open the web page: {error}",
                    &[("error", &e.to_string())],
                ));
            }
        }
        Act::Install => {
            if let Some((install, _)) = CHECKS.install.get() {
                install();
            }
        }
        Act::Cancel => CHECKS.cancel.store(true, Relaxed),
    }
    place
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
        // A version an updater is fetching is said once it is ready, not here.
        let fetching = matches!(found, Found::DevFetching { .. } | Found::DevReady { .. });
        if let Some(running) = self.running.as_mut().filter(|_| !fetching) {
            if found.trouble() {
                running.home_state.notices.trouble(found.notice());
            } else {
                running.home_state.notices.info(found.notice());
            }
        }
        self.update_stepped();
    }

    /// Lets an updater's reports ([`report`]) wake the window, from whatever thread they come.
    pub(crate) fn wake_for_updates(&self) {
        let Some(waker) = self.waker.clone() else { return };
        let waker = Mutex::new(waker);
        let _ = WAKER.set(Box::new(move || {
            if let Ok(waker) = waker.lock() {
                let _ = waker.send_event(crate::AppEvent::UpdateStepped);
            }
        }));
    }

    /// What the Updates row shows changed (an updater got further with a newer version): the
    /// windows are drawn again, Settings open or not, and the notices say once when a version
    /// is ready or was refused.
    pub(crate) fn update_stepped(&mut self) {
        if let Some((words, trouble)) = take_notice()
            && let Some(running) = self.running.as_mut()
        {
            if trouble {
                running.home_state.notices.trouble(words);
            } else {
                running.home_state.notices.info(words);
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
                // As the Updates row says it, with a newer version's page and when it was checked.
                let said = match (&now.last, now.checking || now.fetch.is_some()) {
                    (Some((found, _)), false) => found.reply(),
                    _ => view(&now, &form, Build::now()).sentence,
                };
                let when = match &now.last {
                    Some((_, time)) => tf(
                        "settings.updates.checked",
                        "Checked at {time}",
                        &[("time", &crate::notices::time_of_day(*time, &form))],
                    ),
                    None => t("settings.updates.never", "Not checked yet").to_owned(),
                };
                let said = format!("{said} {when}").trim().to_owned();
                Ok(crate::control::Reply::with(
                    said,
                    serde_json::json!({
                        "version": VERSION,
                        "checking": now.checking,
                        "checked": now.last.as_ref().map(|(_, time)| time),
                        "newer": now.last.as_ref().and_then(|(found, _)| found.newer()),
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
    fn only_a_pre_release_says_it_is_one() {
        for version in ["0.9.0-dev.16", "0.9.0-dev.15", "v0.9.1-dev.2", "1.0.0-beta.1", "1.0.0-rc.1", "0.9.0-dev+mac"] {
            assert!(is_pre_release(version), "{version}");
            let words = crate::i18n::with_language(crate::i18n::Language::English, || pre_release_words(version));
            let (title, _) = words.unwrap_or_else(|| panic!("{version} says nothing"));
            assert_eq!(title, format!("This is a pre-release ({version})"));
        }
        for version in ["0.9.0", "1.0.0", "1.0.0+mac", "2.3.4", "", "bibles-4", "0.9"] {
            assert!(!is_pre_release(version), "{version}");
            assert_eq!(pre_release_words(version), None, "{version}");
        }
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
        assert_eq!(
            checks.status(),
            Status { checking: false, last: Some((Found::Newest, 1_764_000_000)), fetch: None }
        );
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
            None => assert_eq!(VERSION, RELEASE_VERSION.unwrap_or(env!("CARGO_PKG_VERSION"))),
            // Tests built as a development build (SOFTPRESENTER_DEV_VERSION set).
            Some(dev) => assert_eq!(VERSION, dev),
        }
        assert_eq!(Found::Newest.words(), format!("SoftPresenter {VERSION} is the newest."));
        assert!(Version::parse(VERSION).is_some(), "{VERSION} is a version the check can compare");
    }

    #[test]
    fn each_answer_has_its_words() {
        let newer = Found::Newer {
            version: "0.9.0-dev.16".to_owned(),
            page: "https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16".to_owned(),
        };
        assert_eq!(newer.words(), "SoftPresenter 0.9.0-dev.16 is out.");
        assert_eq!(
            newer.reply(),
            "SoftPresenter 0.9.0-dev.16 is out. https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16"
        );
        assert!(newer.notice().contains("download page"));
        assert_eq!(newer.page(), Some("https://github.com/esraell/SoftPresenter/releases/tag/v0.9.0-dev.16"));
        let failed = Found::Failed("GitHub took too long to answer".to_owned());
        assert_eq!(failed.words(), "Couldn\u{2019}t check: GitHub took too long to answer.");
        let blocked = Found::Unreached(Trouble::Blocked);
        assert_eq!(blocked.words(), "Couldn\u{2019}t check: this network checks secure connections and blocked it.");
        assert!(failed.trouble() && blocked.trouble() && !newer.trouble() && !Found::Newest.trouble());
        assert_eq!(Found::Newest.page(), None);
        assert_eq!((newer.newer(), Found::Newest.newer()), (Some("0.9.0-dev.16"), None));
    }

    /// 2:07 in the afternoon, on this computer.
    fn at_2_07() -> i64 {
        use chrono::TimeZone;
        chrono::Local.with_ymd_and_hms(2026, 11, 28, 14, 7, 0).single().expect("a time").timestamp()
    }

    const PUBLIC: Build = Build { fetches: false, waits: false, keys: false, cancelling: false };
    const FETCHES: Build = Build { fetches: true, waits: false, keys: true, cancelling: false };
    const LIVE: Build = Build { waits: true, ..FETCHES };

    fn found(found: Found) -> Status {
        Status { last: Some((found, at_2_07())), ..Status::default() }
    }

    fn fetching(fetch: Fetch) -> Status {
        Status { fetch: Some(fetch), ..Status::default() }
    }

    fn failed(why: Trouble, retries: bool) -> Status {
        fetching(Fetch::Failed { version: "0.9.1".to_owned(), why, retries })
    }

    /// Every state of the row with the build it is reached in: its sentence, the line under
    /// it, its key and the line under the key.
    fn states() -> Vec<(Status, Build, [&'static str; 4])> {
        let newest: &'static str = Box::leak(format!("SoftPresenter {VERSION} is the newest.").into_boxed_str());
        let page = format!("{PAGES}tag/v0.9.1");
        let downloading = Fetch::Downloading { version: "0.9.1".to_owned(), done: 22_680_000, total: 54_000_000 };
        let ready = Fetch::Ready { version: "0.9.1".to_owned() };
        vec![
            // A build from the public source: it checks, finds, and fails to check.
            (Status::default(), PUBLIC, ["Updates are checked only when you ask.", "Not checked yet", "Check Now", ""]),
            (Status { checking: true, ..Status::default() }, PUBLIC, ["Checking\u{2026}", "", "Check Now", ""]),
            (found(Found::Newest), PUBLIC, [newest, "Checked at 2:07", "Check Now", ""]),
            (
                found(Found::Newer { version: "0.9.1".to_owned(), page }),
                PUBLIC,
                [
                    "SoftPresenter 0.9.1 is out.",
                    "This copy can\u{2019}t update itself yet.",
                    "Open the Download Page",
                    "",
                ],
            ),
            (
                found(Found::Unreached(Trouble::NoNetwork)),
                PUBLIC,
                ["Couldn\u{2019}t check: there is no internet connection.", "", "Try Again", ""],
            ),
            (
                found(Found::Unreached(Trouble::Blocked)),
                PUBLIC,
                [
                    "Couldn\u{2019}t check: this network checks secure connections and blocked it.",
                    "Try another network, or get it from the download page: softpresenter.com.",
                    "Try Again",
                    "",
                ],
            ),
            (
                found(Found::Unreached(Trouble::Timeout)),
                PUBLIC,
                [
                    "Couldn\u{2019}t check: GitHub took too long to answer.",
                    "Or get it from the download page: softpresenter.com.",
                    "Try Again",
                    "",
                ],
            ),
            // A build whose updater fetches the version itself.
            (Status::default(), FETCHES, ["", "Not checked yet", "Check Now", ""]),
            (
                fetching(downloading),
                FETCHES,
                ["Downloading 0.9.1\u{2026} 42%", "23 of 54 MB. Nothing changes until it is ready.", "Cancel", ""],
            ),
            (
                found(Found::DevFetching { version: "0.9.1".to_owned() }),
                FETCHES,
                ["Downloading 0.9.1\u{2026}", "Nothing changes until it is ready.", "Cancel", ""],
            ),
            (
                fetching(ready.clone()),
                FETCHES,
                ["0.9.1 is ready. It goes in when you quit, or now.", "", "Update and Restart", ""],
            ),
            (
                found(Found::DevReady { version: "0.9.1".to_owned() }),
                FETCHES,
                ["0.9.1 is ready. It goes in when you quit, or now.", "", "Update and Restart", ""],
            ),
            (
                fetching(ready),
                LIVE,
                [
                    "0.9.1 is ready. It goes in when you quit, or now.",
                    "",
                    "Update and Restart",
                    "Something is on screen",
                ],
            ),
            (
                fetching(Fetch::Cancelled { version: "0.9.1".to_owned() }),
                FETCHES,
                ["The update was cancelled.", "What was downloaded has been deleted.", "Check Now", ""],
            ),
            (
                failed(Trouble::NoNetwork, true),
                FETCHES,
                [
                    "0.9.1 couldn\u{2019}t be downloaded: there is no internet connection.",
                    "SoftPresenter tries again by itself.",
                    "Try Again",
                    "",
                ],
            ),
            (
                failed(Trouble::NoNetwork, false),
                FETCHES,
                ["0.9.1 couldn\u{2019}t be downloaded: there is no internet connection.", "", "Try Again", ""],
            ),
            (
                failed(Trouble::Blocked, true),
                FETCHES,
                [
                    "0.9.1 couldn\u{2019}t be downloaded: this network checks secure connections and blocked it.",
                    "Try another network, or get it from the download page: softpresenter.com.",
                    "Try Again",
                    "",
                ],
            ),
            (
                failed(Trouble::Other("the disk is full".to_owned()), false),
                FETCHES,
                [
                    "0.9.1 couldn\u{2019}t be downloaded: the disk is full.",
                    "Or get it from the download page: softpresenter.com.",
                    "Try Again",
                    "",
                ],
            ),
            (
                failed(Trouble::NotSigned, false),
                FETCHES,
                [
                    "0.9.1 was refused: it isn\u{2019}t signed by SoftPresenter.",
                    "Nothing was changed.",
                    "Try Again",
                    "",
                ],
            ),
            (
                failed(Trouble::Damaged, false),
                FETCHES,
                ["0.9.1 was refused: the download was damaged.", "Nothing was changed.", "Try Again", ""],
            ),
        ]
    }

    #[test]
    fn every_state_has_its_words_and_its_key() {
        let form = crate::clock::TimeForm::default();
        for (status, build, [sentence, under, key, under_key]) in states() {
            let shown = view(&status, &form, build);
            assert_eq!(shown.sentence, sentence, "{status:?}");
            assert_eq!(shown.under.replace("14:07", "2:07"), under, "{status:?}");
            assert_eq!(shown.key.as_ref().map_or("", |key| key.words), key, "{status:?}");
            assert_eq!(shown.under_key, under_key, "{status:?}");
            // A bar only while a version comes down.
            assert_eq!(shown.bar.is_some(), sentence.starts_with("Downloading"), "{status:?}");
        }
        // Checking greys the key; something on screen greys Update and Restart.
        let key = |status: &Status, build| view(status, &form, build).key.expect("a key");
        assert!(!key(&Status { checking: true, ..Status::default() }, PUBLIC).enabled);
        let ready = fetching(Fetch::Ready { version: "0.9.1".to_owned() });
        // The keys that bring a newer version in are amber, and no other.
        let install = Key { words: "Update and Restart", act: Act::Install, enabled: true, amber: true };
        assert_eq!(key(&ready, FETCHES), install);
        assert_eq!(key(&ready, LIVE), Key { enabled: false, ..install });
        // A newer version's key opens its release page, and nothing else.
        let page = format!("{PAGES}tag/v0.9.1");
        let newer = found(Found::Newer { version: "0.9.1".to_owned(), page: page.clone() });
        let open = Key { words: "Open the Download Page", act: Act::Open(page), enabled: true, amber: true };
        assert_eq!(key(&newer, PUBLIC), open);
        // Try Again checks, as Check Now does, and neither is amber.
        let again = key(&failed(Trouble::Blocked, false), FETCHES);
        assert_eq!((again.act, again.amber), (Act::Check, false));
        // Cancel asks the updater to stop, and is greyed once it has been pressed.
        let downloading = fetching(Fetch::Downloading { version: "0.9.1".to_owned(), done: 1, total: 2 });
        let cancel = Key { words: "Cancel", act: Act::Cancel, enabled: true, amber: false };
        assert_eq!(key(&downloading, FETCHES), cancel);
        assert_eq!(key(&downloading, Build { cancelling: true, ..FETCHES }), Key { enabled: false, ..cancel });
        // While a check runs, the row says so whatever was found or fetched before.
        let checking = Status { checking: true, ..ready.clone() };
        assert_eq!(view(&checking, &form, FETCHES).sentence, "Checking\u{2026}");
        // An updater that hasn't said what its keys do gets none: no key that does nothing.
        let unset = Build { keys: false, ..FETCHES };
        let waiting = view(&ready, &form, unset);
        assert_eq!((waiting.sentence.as_str(), waiting.key), ("0.9.1 is ready. It goes in when you quit.", None));
        assert_eq!(view(&downloading, &form, unset).key, None);
    }

    #[test]
    fn the_row_shows_each_states_words_at_one_height_with_one_key_width() {
        use crate::studio::tests::{drawn, place_of};
        for build in [PUBLIC, FETCHES] {
            let mut places = Vec::new();
            for (status, of, [sentence, under, key, under_key]) in states() {
                if of.fetches != build.fetches {
                    continue;
                }
                let ctx = crate::studio::tests::styled();
                let mut state = crate::home::HomeState::default();
                let mut row = None;
                let output = drawn(&ctx, vec![], |ui| {
                    ui.set_width(700.0);
                    row = Some(draw_row(ui, &mut state, &status, of));
                });
                let under = under.replace("2:07", &crate::notices::time_of_day(at_2_07(), state.clock.known()));
                for words in [sentence, under.as_str(), key, under_key, "Updates"] {
                    assert!(
                        words.is_empty() || place_of(&output, words).is_some(),
                        "{words:?} is in the row: {status:?}"
                    );
                }
                let row = row.expect("drawn");
                // The bar runs along the foot of the words, in the row, left of the key.
                let key_place = state.updates_key.expect("the key's place");
                assert_eq!(state.updates_bar.is_some(), sentence.starts_with("Downloading"), "{status:?}");
                if let Some(bar) = state.updates_bar {
                    assert!(row.contains_rect(bar) && bar.max.x < key_place.min.x, "{bar:?} in {row:?}");
                    assert_eq!(bar.height(), 4.0);
                    let words = place_of(&output, under.as_str()).expect("the line under");
                    assert!(bar.min.y > words.y, "under the words");
                }
                output.drop_without_applying_deltas();
                places.push((row, key_place, status));
            }
            let (row, key, _) = &places[0];
            for (other_row, other_key, status) in &places {
                assert_eq!(other_row.height(), row.height(), "the row keeps its height: {status:?}");
                assert_eq!(other_key, key, "the key, or the bar in its place, stays where it is: {status:?}");
            }
            assert!(key.max.x <= row.max.x && key.min.y >= row.min.y && key.max.y <= row.max.y);
        }
    }

    #[test]
    fn in_every_language_the_row_is_one_height_whatever_its_state() {
        use crate::studio::tests::drawn;
        // No heights are named: only that, within a language and a width, no state's row is
        // taller or shorter than another's, so nothing on the page moves as the state changes.
        for language in crate::i18n::Language::ALL {
            crate::i18n::with_language(language, || {
                for width in [700.0, 440.0] {
                    for build in [PUBLIC, FETCHES] {
                        let rows: Vec<(egui::Rect, egui::Rect, Status)> = states()
                            .into_iter()
                            .filter(|(_, of, _)| of.fetches == build.fetches)
                            .map(|(status, of, _)| {
                                let ctx = crate::studio::tests::styled();
                                let mut state = crate::home::HomeState::default();
                                let mut row = None;
                                drawn(&ctx, vec![], |ui| {
                                    ui.set_width(width);
                                    row = Some(draw_row(ui, &mut state, &status, of));
                                })
                                .drop_without_applying_deltas();
                                (row.expect("drawn"), state.updates_key.expect("the key's place"), status)
                            })
                            .collect();
                        let (first, key, _) = &rows[0];
                        for (row, other_key, status) in &rows {
                            assert_eq!(
                                row.height(),
                                first.height(),
                                "{language:?} at {width}: the row changes height in {status:?}"
                            );
                            assert_eq!(other_key, key, "{language:?} at {width}: the key moves in {status:?}");
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn a_download_says_how_much_of_how_much() {
        let form = crate::clock::TimeForm::default();
        let say = |done: u64, total: u64| {
            let shown =
                view(&fetching(Fetch::Downloading { version: "0.9.1".to_owned(), done, total }), &form, FETCHES);
            (shown.sentence, shown.under, shown.bar)
        };
        let part = |percent: &str, amounts: &str, bar: f32| {
            (
                format!("Downloading 0.9.1\u{2026}{percent}"),
                format!("{amounts}Nothing changes until it is ready."),
                Some(bar),
            )
        };
        assert_eq!(say(22_680_000, 54_000_000), part(" 42%", "23 of 54 MB. ", 0.42));
        assert_eq!(say(0, 54_000_000), part(" 0%", "0 of 54 MB. ", 0.0));
        // Never 100% before all of it is there, and never more than all of it.
        assert_eq!(say(53_999_999, 54_000_000), part(" 99%", "54 of 54 MB. ", 0.99));
        assert_eq!(say(54_000_000, 54_000_000), part(" 100%", "54 of 54 MB. ", 1.0));
        assert_eq!(say(60_000_000, 54_000_000), part(" 100%", "60 of 54 MB. ", 1.0));
        // Its size not known (yet): no percent, and an empty bar.
        assert_eq!(say(0, 0), part("", "", 0.0));
        assert_eq!(say(7_400_000, 0), part("", "7 MB. ", 0.0));
        // A very big one doesn't overflow.
        assert_eq!(say(u64::MAX / 2, u64::MAX).2, Some(0.49));
    }

    #[test]
    fn cancel_asks_the_updater_to_stop_and_the_notices_say_it_once() {
        let checks = Checks::new();
        let downloading = |done| Fetch::Downloading { version: "0.9.1".to_owned(), done, total: 54_000_000 };
        checks.report(Some(downloading(1_000_000)));
        assert!(!checks.cancel.load(Relaxed));
        // The row's Cancel: asked, and still asked while the updater reports more of it.
        checks.cancel.store(true, Relaxed);
        checks.report(Some(downloading(2_000_000)));
        assert!(checks.cancel.load(Relaxed), "the updater hasn't stopped yet");
        assert_eq!(checks.take_notice(), None);
        // The updater stopped and deleted what came down: the row and the notices say so, once.
        assert!(checks.report(Some(Fetch::Cancelled { version: "0.9.1".to_owned() })));
        assert!(!checks.cancel.load(Relaxed), "nothing is asked any more");
        let said = "The update was cancelled. What was downloaded has been deleted.";
        assert_eq!(checks.take_notice(), Some((said.to_owned(), false)));
        assert_eq!(checks.take_notice(), None);
        let shown = view(&checks.status(), &crate::clock::TimeForm::default(), FETCHES);
        assert_eq!(format!("{} {}", shown.sentence, shown.under), said);
        // Check Now lets it go; a download cancelled again is said again.
        assert!(checks.begin());
        assert_eq!(checks.status().fetch, None);
        checks.finish(Found::Newest, 1);
        checks.report(Some(Fetch::Cancelled { version: "0.9.1".to_owned() }));
        assert_eq!(checks.take_notice(), Some((said.to_owned(), false)));
        // Cancel pressed just as the download ended: it is ready, and nothing is left asked.
        checks.report(Some(downloading(3_000_000)));
        checks.cancel.store(true, Relaxed);
        assert!(checks.report(Some(Fetch::Ready { version: "0.9.1".to_owned() })));
        assert!(!checks.cancel.load(Relaxed));
    }

    #[test]
    fn a_report_goes_on_with_settings_closed_and_wakes_the_window_only_for_something_to_see() {
        // No window is part of this: the checks stand where they stand whoever looks.
        let checks = Checks::new();
        let downloading = |done| Fetch::Downloading { version: "0.9.1".to_owned(), done, total: 54_000_000 };
        assert!(checks.report(Some(downloading(0))), "the first report shows");
        assert!(!checks.report(Some(downloading(100_000))), "a tenth of a megabyte more changes nothing to see");
        assert!(checks.report(Some(downloading(600_000))), "another megabyte, to the nearest");
        assert!(checks.report(Some(downloading(22_680_000))));
        assert_eq!(checks.status().fetch, Some(downloading(22_680_000)), "what was reported is what is kept");
        assert_eq!(checks.take_notice(), None, "nothing is said while it comes down");
        // A check meanwhile leaves the download alone.
        assert!(checks.begin());
        checks.finish(Found::Newest, 1);
        assert_eq!(checks.status().fetch, Some(downloading(22_680_000)));
        let ready = Fetch::Ready { version: "0.9.1".to_owned() };
        assert!(checks.report(Some(ready.clone())));
        assert!(!checks.report(Some(ready.clone())), "said again, nothing changes");
        // Ready: the notices say so once, however often it is asked or reported.
        assert_eq!(checks.take_notice(), Some(("Version 0.9.1 is ready.".to_owned(), false)));
        assert_eq!(checks.take_notice(), None);
        checks.report(Some(ready));
        assert_eq!(checks.take_notice(), None);
        // The next version is said too.
        checks.report(Some(Fetch::Ready { version: "0.9.2".to_owned() }));
        assert_eq!(checks.take_notice(), Some(("Version 0.9.2 is ready.".to_owned(), false)));
        // Nothing in hand any more: back to what the last check found.
        assert!(checks.report(None));
        assert_eq!(checks.status(), Status { checking: false, last: Some((Found::Newest, 1)), fetch: None });
    }

    #[test]
    fn a_refusal_is_said_in_the_notices_and_no_network_only_in_the_row() {
        let checks = Checks::new();
        let failed = |why| Fetch::Failed { version: "0.9.1".to_owned(), why, retries: true };
        for quiet in [Trouble::NoNetwork, Trouble::Blocked, Trouble::Timeout] {
            checks.report(Some(failed(quiet)));
            assert_eq!(checks.take_notice(), None);
        }
        checks.report(Some(failed(Trouble::NotSigned)));
        let said = "0.9.1 was refused: it isn\u{2019}t signed by SoftPresenter. Nothing was changed.";
        assert_eq!(checks.take_notice(), Some((said.to_owned(), true)));
        assert_eq!(checks.take_notice(), None, "once");
        // Try Again lets the failure go, so the row says Checking… and then what comes.
        assert!(checks.begin());
        assert_eq!(checks.status().fetch, None);
        // An updater's answer to a check that it has one ready is said the same way, once.
        checks.finish(Found::DevReady { version: "0.9.3".to_owned() }, 1);
        assert_eq!(checks.take_notice(), Some(("Version 0.9.3 is ready.".to_owned(), false)));
        checks.report(Some(Fetch::Ready { version: "0.9.3".to_owned() }));
        assert_eq!(checks.take_notice(), None);
    }

    /// A request to `address` with the app's own client settings: the error it ends in.
    fn asked(address: &str) -> ureq::Error {
        let agent: ureq::Agent =
            ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
        agent.get(address).call().expect_err("nothing answers there properly")
    }

    #[test]
    fn a_certificate_nobody_vouches_for_is_a_network_that_blocked_it_not_no_network() {
        use std::sync::Arc;
        use tokio_rustls::rustls;
        // A server on this computer with a certificate it signed itself, as a network's filter
        // answers for GitHub with a certificate of its own.
        let folder = std::env::temp_dir().join(format!("sp-updates-blocked-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        let names = crate::remote::cert::Names { names: vec!["localhost".to_owned()], until: 0 };
        let cert = crate::remote::cert::make(&folder, &names, true).expect("a certificate");
        let _ = std::fs::remove_dir_all(&folder);
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocols")
            .with_no_client_auth()
            .with_single_cert(cert.chain.clone(), cert.key.clone_key())
            .expect("the pair");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("its address").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a caller");
            let mut connection = rustls::ServerConnection::new(Arc::new(config)).expect("a connection");
            // The handshake, until the caller turns the certificate down.
            while connection.is_handshaking() {
                if connection.complete_io(&mut stream).is_err() {
                    break;
                }
            }
        });
        let error = asked(&format!("https://localhost:{port}/releases"));
        let _ = server.join();
        // As ureq hands it over today: an I/O error with rustls's inside, which used to be
        // taken for "no connection".
        assert!(format!("{error:?}").contains("UnknownIssuer"), "{error:?}");
        assert_eq!(trouble_of(&error), Trouble::Blocked, "{error:?}");
    }

    #[test]
    fn nothing_listening_is_no_network_and_other_errors_keep_their_kind() {
        // A port nothing listens on (taken, then let go).
        let port = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).expect("a port").port();
        let error = asked(&format!("http://127.0.0.1:{port}/releases"));
        assert_eq!(trouble_of(&error), Trouble::NoNetwork, "{error:?}");
        let io = |kind| ureq::Error::Io(std::io::Error::new(kind, "x"));
        assert_eq!(trouble_of(&io(std::io::ErrorKind::TimedOut)), Trouble::Timeout);
        assert_eq!(trouble_of(&io(std::io::ErrorKind::ConnectionReset)), Trouble::Unreached);
        assert_eq!(trouble_of(&ureq::Error::HostNotFound), Trouble::NoNetwork);
        assert_eq!(trouble_of(&ureq::Error::Tls("x")), Trouble::Insecure);
        // Another fault in a certificate is a secure connection that failed, not a filter.
        use tokio_rustls::rustls;
        let expired = rustls::Error::InvalidCertificate(rustls::CertificateError::Expired);
        assert_eq!(trouble_of(&ureq::Error::Rustls(expired.clone())), Trouble::Insecure);
        let inside = ureq::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, expired));
        assert_eq!(trouble_of(&inside), Trouble::Insecure);
    }

    #[test]
    fn a_testing_aids_word_names_a_state() {
        for word in [
            "never",
            "checking",
            "newest",
            "found",
            "check-failed",
            "check-blocked",
            "downloading",
            "ready",
            "ready-live",
            "failed",
            "blocked",
            "refused",
            "cancelled",
        ] {
            assert!(stage(word).is_some(), "{word}");
        }
        assert_eq!(stage("ready-live").map(|(_, waits)| waits), Some(true));
        assert_eq!(stage("nonsense"), None);
    }
}
