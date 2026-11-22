//! Canva Sync: keeps the church's announcements video fresh from its Canva presentation, through
//! Canva's Connect API (`https://api.canva.com/rest/v1`), with an integration the church makes in
//! Canva's developer portal (its own client ID and secret: an open-source app can't ship one).
//!
//! - **Sign-in** is OAuth with PKCE in the system's browser, its answer coming back to a listener
//!   on `127.0.0.1` only, which takes one answer carrying the `state` it sent and closes.
//! - **What is kept where.** The client ID and secret and the tokens are one item in the Mac's
//!   Keychain (a test copy, started with `SOFTPRESENTER_DATA`, keeps its own in its own folder and
//!   never reads the Keychain); the design and its divider page are in `canva-sync.json`. No
//!   secret, token, sign-in answer or download address goes into a settings file, the log, a
//!   notice, a command's reply or an error: errors are written here in plain words.
//! - **Tokens rotate.** Canva's refresh tokens work once: each refresh gives a new one, which is
//!   stored (and read back) before anything else is asked of Canva, under a lock no other copy
//!   of the app refreshes past; the access token is kept with its expiry and refreshed only when
//!   it is due. A refresh, once sent, is never cancelled before its answer is stored. A sign-in
//!   Canva no longer takes stops the asking until the operator signs in again.
//! - **A sync** lists the design's pages, exports those above the divider page as a 1080p video,
//!   polls the export until it gives an address (a render takes minutes), downloads it outside
//!   the library, checks it (size, and an MP4 header), and only then puts it in the library's
//!   `Media/Canva Sync` folder under a name volunteers can read. Anything that goes wrong
//!   leaves the video there as it was. The app puts the new file in place of the old in the
//!   media bin, and a video playing takes it over at the end of its pass.
//! - Every request runs on a thread of its own, one at a time, and only while the app is open.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

/// Canva's Connect API, and where its sign-in page is.
pub const LIVE_API: &str = "https://api.canva.com/rest/v1";
pub const LIVE_AUTHORIZE: &str = "https://www.canva.com/api/oauth/authorize";

/// Where Canva sends the sign-in's answer: a fixed address, which the church adds to its
/// integration's redirect URLs (Canva matches it exactly).
pub const REDIRECT_PORT: u16 = 51793;
pub const REDIRECT_PATH: &str = "/canva-sync";

/// What the integration is asked for: reading a design's pages and exporting it.
pub const SCOPE: &str = "design:content:read";

/// The folder in the library's `Media` the video goes in, and the media playlist that follows it.
pub const FOLDER: &str = "Canva Sync";
pub const PLAYLIST: &str = "Announcements";

/// A downloaded video smaller than this is not one.
const SMALLEST_VIDEO: u64 = 1_000_000;
/// Nor larger than this.
const LARGEST_VIDEO: u64 = 4 * 1024 * 1024 * 1024;
/// The most an answer from the API may be, in bytes.
const LARGEST_REPLY: u64 = 4 * 1024 * 1024;
/// How long a sign-in waits for its answer from the browser.
const SIGN_IN_WAIT: Duration = Duration::from_secs(5 * 60);
/// How long before an access token runs out it is refreshed.
const REFRESH_AHEAD: i64 = 5 * 60;

/// Where everything is, and how long things wait: Canva's, or a stand-in's for tests.
#[derive(Clone, Debug)]
pub struct Places {
    pub api: String,
    pub authorize: String,
    /// The port the sign-in's answer comes to (0 in tests: any free one).
    pub redirect_port: u16,
    pub store: Store,
    /// The old sync script's folder (its `creds.env` and `tokens.json`).
    pub old_script: PathBuf,
    /// Folders the old script's background jobs and menu bar plugin may be in.
    pub old_jobs: Vec<(OldKind, PathBuf)>,
    /// Where a download is put while it comes, outside the library.
    pub staging: PathBuf,
    /// The file two copies of the app lock while one refreshes or keeps the sign-in.
    pub lock: PathBuf,
    /// True when the API is a stand-in on this computer (a test copy's), where plain http is
    /// allowed.
    pub stand_in: bool,
    /// The settings file (design, divider page, last video).
    pub settings: PathBuf,
    /// The folder the video goes in.
    pub folder: PathBuf,
    pub poll_every: Duration,
    pub export_for: Duration,
}

impl Places {
    /// Canva's own, for the app. A test copy (one started with `SOFTPRESENTER_DATA`) keeps
    /// everything in its own folder. Testing aid: in a test copy only, `SOFTPRESENTER_CANVA_API`
    /// points at a stand-in for Canva on this computer (plain http to 127.0.0.1 only, so nothing
    /// secret can go elsewhere); the stand-in serves the API under `/rest/v1` and the sign-in page
    /// at `/api/oauth/authorize`. The owner's app never takes it, so it can't reach the Keychain
    /// item or the old script's real folder.
    pub fn for_app() -> Places {
        let data = crate::bible_store::data_dir().unwrap_or_else(std::env::temp_dir);
        let test = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        let stand_in = std::env::var("SOFTPRESENTER_CANVA_API").ok().filter(|base| test && is_stand_in(base));
        let (api, authorize) = match &stand_in {
            Some(base) => {
                let base = base.trim().trim_end_matches('/');
                (format!("{base}/rest/v1"), format!("{base}/api/oauth/authorize"))
            }
            None => (LIVE_API.to_owned(), LIVE_AUTHORIZE.to_owned()),
        };
        // The account's home folder from the system's records, not from `HOME`; without it the
        // old script isn't looked for.
        let home = sp_video::home_folder();
        let (store, old_script, old_jobs, local) = if test {
            let jobs = data.join("old-canva-sync-jobs");
            (
                Store::File(data.join("canva-test-sign-in.json")),
                data.join("old-canva-sync"),
                vec![
                    (OldKind::Daemon, jobs.join("LaunchDaemons")),
                    (OldKind::Agent, jobs.join("LaunchAgents")),
                    (OldKind::Plugin, jobs.join("Plugins")),
                ],
                data.join("Canva staging"),
            )
        } else {
            let local = local_cache(home.as_deref(), &data);
            match &home {
                Some(home) => (
                    Store::Keychain,
                    home.join("Library/Application Support/canva-sync"),
                    vec![
                        (OldKind::Daemon, PathBuf::from("/Library/LaunchDaemons")),
                        (OldKind::Agent, home.join("Library/LaunchAgents")),
                        (OldKind::Plugin, home.join("Library/Application Support/SwiftBar/Plugins")),
                    ],
                    local,
                ),
                None => (Store::Keychain, PathBuf::new(), Vec::new(), local),
            }
        };
        Places {
            api,
            authorize,
            redirect_port: REDIRECT_PORT,
            store,
            old_script,
            old_jobs,
            // In the data folder, which every copy of the app using this library shares.
            lock: data.join(".canva-sync.lock"),
            stand_in: stand_in.is_some(),
            staging: local,
            settings: data.join("canva-sync.json"),
            folder: data.join("Media").join(FOLDER),
            poll_every: Duration::from_secs(5),
            export_for: Duration::from_secs(25 * 60),
        }
    }

    /// The address the sign-in's answer comes back to.
    pub fn redirect(&self, port: u16) -> String {
        format!("http://127.0.0.1:{port}{REDIRECT_PATH}")
    }
}

/// True for plain http to 127.0.0.1 and a port, the only place a stand-in may be: after the
/// colon, one to five digits, then the end or a `/` (nothing that could make it another host).
fn is_stand_in(base: &str) -> bool {
    let Some(rest) = base.trim().strip_prefix("http://127.0.0.1:") else { return false };
    let port = rest.split_once('/').map_or(rest, |(port, _)| port);
    (1..=5).contains(&port.len())
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u32>().is_ok_and(|port| (1..=65535).contains(&port))
}

/// A folder of the computer's own (not the library, which may be synced to the cloud) for a
/// download while it comes, on the same disk as the library as a rule; the library's own hidden
/// folder where there is no such place.
fn local_cache(home: Option<&Path>, data: &Path) -> PathBuf {
    if cfg!(target_os = "macos")
        && let Some(home) = home
    {
        return home.join("Library/Caches/SoftPresenter/Canva Sync");
    }
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).filter(|local| local.is_absolute())
    {
        return local.join("SoftPresenter").join("Canva Sync");
    }
    data.join(".canva-staging")
}

// ---------------------------------------------------------------------------------------------
// The sign-in, kept.

/// The church's integration and its sign-in. Never printed: its `Debug` shows only the client
/// ID's first characters.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Secrets {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default)]
    pub access_token: Option<String>,
    /// When the access token runs out, in seconds since 1970.
    #[serde(default)]
    pub access_until: Option<i64>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secrets {{ client: {}, .. }}", hint_of(&self.client_id))
    }
}

impl Secrets {
    /// The access token, if it has more than `needed` seconds left.
    fn fresh_access(&self, needed: i64) -> Option<&str> {
        let until = self.access_until?;
        (until - now_seconds() > needed).then_some(self.access_token.as_deref()).flatten()
    }
}

/// A client ID's first characters, to show which integration it is.
pub fn hint_of(client_id: &str) -> String {
    format!("{}…", client_id.chars().take(6).collect::<String>())
}

fn now_seconds() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}

/// Where the sign-in is kept: the Mac's Keychain, or (a test copy) a file of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Store {
    Keychain,
    File(PathBuf),
}

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "SoftPresenter Canva Sync";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "church sign-in";

impl Store {
    /// The sign-in kept, if there is one. The error is a sentence.
    pub fn load(&self) -> Result<Option<Secrets>, String> {
        let bytes = match self {
            Store::File(path) => match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("The Canva sign-in kept for this test copy could not be read.".to_owned()),
            },
            #[cfg(target_os = "macos")]
            Store::Keychain => {
                match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
                    Ok(bytes) => bytes,
                    Err(e) if e.code() == -25300 => return Ok(None),
                    Err(_) => return Err("The Canva sign-in could not be read from the Keychain.".to_owned()),
                }
            }
            #[cfg(not(target_os = "macos"))]
            Store::Keychain => return Ok(None),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "The Canva sign-in kept is damaged; sign in again.".to_owned())
    }

    /// Keeps the sign-in, as one write, and reads it back to be sure it is there. The error is a
    /// sentence.
    pub fn save(&self, secrets: &Secrets) -> Result<(), String> {
        let bytes = serde_json::to_vec(secrets).map_err(|_| "The Canva sign-in could not be written.".to_owned())?;
        match self {
            Store::File(path) => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let partial = path.with_extension("json.part");
                write_private(&partial, &bytes)
                    .and_then(|()| std::fs::rename(&partial, path))
                    .map_err(|e| format!("The Canva sign-in could not be kept ({e})."))?;
            }
            #[cfg(target_os = "macos")]
            Store::Keychain => {
                security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &bytes)
                    .map_err(|_| "The Canva sign-in could not be kept in the Keychain.".to_owned())?;
            }
            #[cfg(not(target_os = "macos"))]
            Store::Keychain => {
                return Err("Keeping the Canva sign-in needs macOS for now (on Windows it would go in the \
                            Credential Manager, which SoftPresenter doesn't use yet)."
                    .to_owned());
            }
        }
        match self.load() {
            Ok(Some(kept)) if kept == *secrets => Ok(()),
            _ => Err("The Canva sign-in was written but couldn't be read back.".to_owned()),
        }
    }

    /// Forgets the sign-in on this computer.
    pub fn forget(&self) {
        match self {
            Store::File(path) => {
                let _ = std::fs::remove_file(path);
            }
            #[cfg(target_os = "macos")]
            Store::Keychain => {
                let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
            }
            #[cfg(not(target_os = "macos"))]
            Store::Keychain => {}
        }
    }
}

/// Writes a new file only its owner may read. Anything left at that name (a half-written file,
/// a link) is removed first, and the file is made new, so it is never written through.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The choices kept in `canva-sync.json`: nothing secret.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The design, by its id.
    pub design: Option<String>,
    /// The divider page, by its id (which stays as pages move), and its number when chosen.
    pub divider: Option<String>,
    pub divider_number: Option<u32>,
    /// The last video put in place: when, and its name.
    pub updated_at: Option<String>,
    pub updated_file: Option<String>,
    /// True from bringing in the old sync script's sign-in until the operator says it is off.
    pub old_script_warning: bool,
    /// True when the old script's token file couldn't be taken away, so it could still refresh
    /// the sign-in: nothing is synced until the operator says it is off.
    pub hold_syncs: bool,
    /// The videos Canva Sync put in its folder, by name: the only files it ever deletes there.
    pub made: Vec<String>,
}

impl Settings {
    /// The settings as kept. The design's id is checked as a typed one is (it goes into an
    /// address), and only plain file names are taken as videos the app made.
    pub fn load(path: &Path) -> Settings {
        let mut settings: Settings =
            std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        settings.design = settings.design.and_then(|design| design_id(&design).filter(|id| *id == design));
        settings.made.retain(|name| is_plain_name(name));
        settings
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, serde_json::to_string_pretty(self).unwrap_or_default())
            .and_then(|()| std::fs::rename(&partial, path))
            .map_err(|e| format!("Canva Sync's settings could not be saved: {e}"))
    }
}

// ---------------------------------------------------------------------------------------------
// Talking to Canva.

/// What went wrong, in words for the operator; `SignInGone` stops the asking until the
/// operator signs in again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    SignInGone(String),
    /// Canva answered a refresh but its answer couldn't be read: the sign-in may be used up.
    Doubtful(String),
    Cancelled,
    Other(String),
}

impl Failure {
    pub fn words(&self) -> String {
        match self {
            Failure::SignInGone(why) | Failure::Doubtful(why) | Failure::Other(why) => why.clone(),
            Failure::Cancelled => "Stopped.".to_owned(),
        }
    }
}

/// What a refresh answered but couldn't be read says.
const MAYBE_USED_UP: &str =
    "Canva may have used up the sign-in (its answer couldn't be read): sign in again if the next sync fails.";

fn other(words: impl Into<String>) -> Failure {
    Failure::Other(words.into())
}

/// An HTTP request's failure in words, without the address (a download's address is a key).
fn plain(e: &ureq::Error) -> &'static str {
    match e {
        ureq::Error::Timeout(_) => "it took too long",
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "it couldn't be reached (is this computer online?)"
        }
        ureq::Error::Tls(_) => "the secure connection failed",
        _ => "the request failed",
    }
}

/// A client for the API: no redirects (a token never follows one elsewhere), https unless the
/// API is a stand-in on this computer.
fn api_agent(places: &Places, timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .https_only(!places.stand_in)
        .timeout_global(Some(timeout))
        .build()
        .into()
}

/// An API call with the access token: its status and its answer as JSON (Canva's error bodies
/// are never shown as they are).
fn call(
    places: &Places,
    token: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(u16, serde_json::Value), Failure> {
    let agent = api_agent(places, Duration::from_secs(30));
    let url = format!("{}{path}", places.api);
    let auth = format!("Bearer {token}");
    let sent = match body {
        Some(body) => agent
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .send(body.to_string()),
        None => agent.get(&url).header("Authorization", &auth).header("Accept", "application/json").call(),
    };
    let mut response = sent.map_err(|e| other(format!("Canva couldn't be asked: {}.", plain(&e))))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .with_config()
        .limit(LARGEST_REPLY)
        .read_to_string()
        .map_err(|_| other("Canva's answer broke off; the video stays as it was."))?;
    Ok((status, serde_json::from_str(&text).unwrap_or_default()))
}

/// The error code Canva gave, if one of the few it documents; nothing else of its answer.
fn canva_code(reply: &serde_json::Value) -> &str {
    reply.get("code").or_else(|| reply.get("error")).and_then(serde_json::Value::as_str).unwrap_or("")
}

/// The token endpoint, with the integration's ID and secret as HTTP Basic: the new tokens, or
/// why not.
fn token_request(places: &Places, secrets: &Secrets, form: &[(&str, &str)]) -> Result<serde_json::Value, Failure> {
    // Longer than other calls: a refresh given up on here may have used the token up there.
    let agent = api_agent(places, TOKEN_WAIT);
    let basic =
        format!("Basic {}", base64(format!("{}:{}", secrets.client_id, secrets.client_secret).as_bytes(), STANDARD));
    let sent = agent
        .post(&format!("{}/oauth/token", places.api))
        .header("Authorization", &basic)
        .send_form(form.iter().copied());
    let mut response = sent.map_err(|e| other(format!("Canva's sign-in couldn't be reached: {}.", plain(&e))))?;
    let status = response.status().as_u16();
    let text = response.body_mut().with_config().limit(LARGEST_REPLY).read_to_string();
    // A refresh Canva answered may have used the token up: an answer that can't be read isn't a
    // passing problem.
    let reply: serde_json::Value = match (&text, status) {
        (Ok(text), _) => match serde_json::from_str(text) {
            Ok(reply) => reply,
            Err(_) if status == 200 => return Err(Failure::Doubtful(MAYBE_USED_UP.to_owned())),
            Err(_) => serde_json::Value::Null,
        },
        (Err(_), 200) => return Err(Failure::Doubtful(MAYBE_USED_UP.to_owned())),
        (Err(_), code) => return Err(other(format!("Canva's sign-in answer broke off ({code})."))),
    };
    match (status, canva_code(&reply)) {
        (200, _) => Ok(reply),
        (_, "invalid_grant") => Err(Failure::SignInGone(
            "Canva no longer takes this sign-in (it was used elsewhere, or taken back). Sign in again \
             (Settings › Integrations › Canva Sync)."
                .to_owned(),
        )),
        (_, "invalid_client" | "unauthorized_client" | "invalid_field") | (401, _) => Err(Failure::SignInGone(
            "Canva doesn't know that client ID and secret: check them in Canva's developer portal and sign in again."
                .to_owned(),
        )),
        (429, _) => Err(other("Canva asked to wait a moment before signing in again; try again shortly.")),
        (code, _) => Err(other(format!("Canva's sign-in answered with a problem ({code})."))),
    }
}

/// Takes a token answer into the sign-in: the new refresh token (each works once), the access
/// token and when it runs out.
fn take_tokens(secrets: &mut Secrets, reply: &serde_json::Value) -> Result<(), Failure> {
    let field = |name: &str| reply.get(name).and_then(serde_json::Value::as_str).filter(|v| !v.is_empty());
    let (Some(access), Some(refresh)) = (field("access_token"), field("refresh_token")) else {
        return Err(Failure::Doubtful(MAYBE_USED_UP.to_owned()));
    };
    let lasts = reply.get("expires_in").and_then(serde_json::Value::as_i64).unwrap_or(4 * 60 * 60);
    secrets.access_token = Some(access.to_owned());
    secrets.refresh_token = refresh.to_owned();
    secrets.access_until = Some(now_seconds() + lasts.clamp(60, 24 * 60 * 60));
    Ok(())
}

/// The newest sign-in and an access token with time left: the one kept if it has, otherwise a
/// refreshed one. The refresh happens under a lock (no other copy of the app refreshes the same
/// token meanwhile), from the sign-in as it is kept now (another copy may have refreshed it), and
/// its new tokens are stored and read back before this returns: a refresh token works once, so
/// losing the new one would lose the sign-in. `secrets` is the sign-in as this copy knows it,
/// updated with whatever is newest, kept or not. `unsaved` says the sign-in in `secrets` is
/// newer than the one kept (a store that failed before), and is cleared once it is kept.
fn access_token(
    places: &Places,
    secrets: &mut Option<Secrets>,
    unsaved: &mut bool,
    force: bool,
) -> Result<String, Failure> {
    let lock = take_lock(&places.lock).map_err(other)?;
    // A sign-in this copy couldn't store before is stored first; otherwise the kept one is newest.
    if *unsaved && let Some(mine) = secrets.as_ref() {
        places
            .store
            .save(mine)
            .map_err(|why| other(format!("{why} The sign-in is held until SoftPresenter quits.")))?;
        *unsaved = false;
    } else if let Ok(Some(kept)) = places.store.load() {
        *secrets = Some(kept);
    }
    let Some(current) = secrets.as_mut() else {
        return Err(Failure::SignInGone("Canva Sync isn't signed in.".to_owned()));
    };
    // It must last as long as a sync can (an export is polled for up to `export_for`).
    let needed = REFRESH_AHEAD + places.export_for.as_secs() as i64;
    if !force && let Some(access) = current.fresh_access(needed) {
        return Ok(access.to_owned());
    }
    let refresh = current.refresh_token.clone();
    // From here until it is kept, quitting waits (see `Client::shut_down`).
    let _refreshing = Refreshing::begin();
    let reply = token_request(places, current, &[("grant_type", "refresh_token"), ("refresh_token", &refresh)])?;
    take_tokens(current, &reply)?;
    if let Err(why) = places.store.save(current) {
        *unsaved = true;
        return Err(other(format!(
            "{why} Canva gave a new sign-in, which works only here until SoftPresenter quits: fix the Keychain, \
             then Sync Now keeps it."
        )));
    }
    crate::applog::event("canva", "refreshed the sign-in and kept it");
    let access = current.access_token.clone().unwrap_or_default();
    drop(lock);
    Ok(access)
}

/// How long a token request may take: longer than other calls, since a refresh given up on here
/// may have used the token up at Canva's end.
const TOKEN_WAIT: Duration = Duration::from_secs(90);

/// Takes the lock two copies of the app share for refreshing and keeping the sign-in, waiting a
/// while for another copy that holds it. The error is a sentence: nothing is refreshed or kept
/// without the lock.
fn take_lock(path: &Path) -> Result<std::fs::File, String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file =
        std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(|_| {
            "Canva Sync's lock in the library folder couldn't be opened, so nothing was done.".to_owned()
        })?;
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => {
                return Err(
                    "Another copy of SoftPresenter is using the Canva sign-in; try again in a moment.".to_owned()
                );
            }
        }
    }
}

/// True while a refresh is between being sent and its new tokens kept: quitting waits for it.
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// Marks a refresh in flight, until dropped (however the refresh ends).
struct Refreshing;

impl Refreshing {
    fn begin() -> Refreshing {
        REFRESHING.store(true, Ordering::SeqCst);
        Refreshing
    }
}

impl Drop for Refreshing {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::SeqCst);
    }
}

/// One page of the design.
#[derive(Clone, PartialEq, Eq)]
pub struct Page {
    pub number: u32,
    pub id: Option<String>,
    /// Its small picture's address (signed for a few minutes: never logged or printed).
    pub thumbnail: Option<String>,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let picture = if self.thumbnail.is_some() { "…" } else { "none" };
        write!(f, "Page {{ number: {}, id: {:?}, thumbnail: {picture} }}", self.number, self.id)
    }
}

/// Every page of the design, in order.
fn pages(places: &Places, token: &str, design: &str) -> Result<Vec<Page>, Failure> {
    let mut all = Vec::new();
    let mut offset = 1;
    loop {
        let (status, reply) = call(places, token, &format!("/designs/{design}/pages?offset={offset}&limit=200"), None)?;
        match status {
            200 => {}
            401 => return Err(other("Canva refused the sign-in for that design.")),
            403 => return Err(other("Canva says this sign-in can't see that design (it isn't shared with it).")),
            404 => return Err(other("Canva doesn't know that design: check its link.")),
            400 if canva_code(&reply) == "page_not_found" => {
                return Err(other("That design has no pages (a Canva doc?): use a presentation."));
            }
            code => return Err(other(format!("Canva answered with a problem listing the pages ({code})."))),
        }
        let items = reply.get("items").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
        let count = items.len();
        for item in items {
            let number = item.get("page_number").or_else(|| item.get("index")).and_then(serde_json::Value::as_u64);
            let Some(number) = number.and_then(|n| u32::try_from(n).ok()) else { continue };
            fn text(value: Option<&serde_json::Value>) -> Option<&str> {
                value.and_then(serde_json::Value::as_str).filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
            }
            all.push(Page {
                number,
                id: text(item.get("id")).map(str::to_owned),
                thumbnail: text(item.get("thumbnail").and_then(|t| t.get("url"))).map(str::to_owned),
            });
        }
        if count < 200 || offset + count > 500 {
            break;
        }
        offset += count;
    }
    Ok(all)
}

/// The page numbers above the divider page, or why there are none to export.
pub fn pages_above(pages: &[Page], divider: &str) -> Result<Vec<u32>, String> {
    let Some(at) = pages.iter().find(|page| page.id.as_deref() == Some(divider)) else {
        return Err("The divider page isn't in the design any more, so the video wasn't made (it would take in the \
                    pages below it). The video in the media bin stays as it was. Choose the divider again in \
                    Settings › Integrations › Canva Sync."
            .to_owned());
    };
    if at.number <= 1 {
        return Err("There are no pages above the divider page; the video in the media bin stays as it was.".to_owned());
    }
    Ok((1..at.number).collect())
}

/// Exports the pages as a 1080p video and waits for the export, polling: the address to
/// download it from.
fn export(places: &Places, token: &str, design: &str, pages: &[u32], cancel: &AtomicBool) -> Result<String, Failure> {
    let body = serde_json::json!({
        "design_id": design,
        "format": {"type": "mp4", "quality": "horizontal_1080p", "pages": pages},
    });
    let (status, reply) = call(places, token, "/exports", Some(body))?;
    if status != 200 {
        return Err(other(match status {
            429 => "Canva asked to wait before another export; try again in a few minutes.".to_owned(),
            code => format!("Canva wouldn't start the export ({code})."),
        }));
    }
    let job = reply
        .pointer("/job/id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .ok_or_else(|| other("Canva's answer didn't say which export it started."))?
        .to_owned();
    crate::applog::event("canva", &format!("exporting {} pages", pages.len()));
    let started = Instant::now();
    loop {
        if wait_or_cancel(places.poll_every, cancel) {
            return Err(Failure::Cancelled);
        }
        if started.elapsed() > places.export_for {
            return Err(other("Canva's export didn't finish in time; the video stays as it was."));
        }
        let (status, reply) = call(places, token, &format!("/exports/{job}"), None)?;
        if status == 429 {
            continue;
        }
        if status != 200 {
            return Err(other(format!("Canva answered with a problem about the export ({status}).")));
        }
        match reply.pointer("/job/status").and_then(serde_json::Value::as_str) {
            Some("success") => {
                return reply
                    .pointer("/job/urls/0")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| other("Canva's finished export had no address to download it from."));
            }
            Some("failed") => {
                let why = match reply.pointer("/job/error/code").and_then(serde_json::Value::as_str) {
                    Some("license_required") => "a page uses something the church's Canva plan doesn't include",
                    Some("approval_required") => "the design is waiting for approval in Canva",
                    _ => "Canva couldn't make the video",
                };
                return Err(other(format!("The export failed: {why}. The video stays as it was.")));
            }
            _ => {}
        }
    }
}

/// Waits, waking often to see whether to stop. True when stopped.
fn wait_or_cancel(time: Duration, cancel: &AtomicBool) -> bool {
    let until = Instant::now() + time;
    while Instant::now() < until {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        std::thread::sleep((until - Instant::now()).min(Duration::from_millis(100)));
    }
    cancel.load(Ordering::Relaxed)
}

/// Downloads the export into `into` (no token goes with it: the address is its own key),
/// saying how far it has got. The error never names the address.
fn download(
    places: &Places,
    url: &str,
    into: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<u64, Failure> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(5)
        .https_only(!places.stand_in)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // A download that stalls for a minute has failed; a slow one has up to an hour.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
        .build()
        .into();
    let mut response =
        agent.get(url).call().map_err(|e| other(format!("The video couldn't be downloaded: {}.", plain(&e))))?;
    if response.status().as_u16() != 200 {
        return Err(other(format!("The video couldn't be downloaded ({}).", response.status().as_u16())));
    }
    let total = response.body().content_length();
    let mut reader = response.body_mut().with_config().limit(LARGEST_VIDEO).reader();
    // A new file (never through something already at that name).
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(into)
        .map_err(|e| other(format!("The video couldn't be saved ({e}).")))?;
    let (mut done, mut buffer) = (0u64, vec![0u8; 256 * 1024]);
    progress(0, total);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        let read = reader.read(&mut buffer).map_err(|_| other("The download broke off; the video stays as it was."))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| other(format!("The video couldn't be saved ({e}).")))?;
        done += read as u64;
        progress(done, total);
    }
    file.sync_all().map_err(|e| other(format!("The video couldn't be saved ({e}).")))?;
    Ok(done)
}

/// True for a file that is a video worth putting in place: large enough, with an MP4 header.
pub fn looks_like_video(path: &Path) -> Result<u64, String> {
    let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let mut head = [0u8; 64];
    let read = std::fs::File::open(path).and_then(|mut file| file.read(&mut head)).unwrap_or(0);
    let mp4 = head[..read].windows(4).any(|four| four == b"ftyp");
    if size < SMALLEST_VIDEO || !mp4 {
        return Err("What Canva sent isn't a whole video, so it wasn't used; the video stays as it was.".to_owned());
    }
    Ok(size)
}

/// The video's name, which volunteers read: the weekday and time first ("Friday 6.00 AM -
/// Announcements 11-20-26.mp4"; a colon can't be in a Mac file's name).
pub fn video_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("{} - Announcements {}.mp4", now.format("%A %-I.%M %p"), now.format("%m-%d-%y"))
}

/// Puts the checked download in the folder under its name (another name if one is there),
/// without ever showing a part-written file there: a rename, or on another disk a copy into a
/// hidden file there first.
fn put_in_place(from: &Path, folder: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("The Canva Sync folder couldn't be made ({e})."))?;
    let stem = name.strip_suffix(".mp4").unwrap_or(name);
    let to = std::iter::once(folder.join(name))
        .chain((2..100).map(|n| folder.join(format!("{stem} ({n}).mp4"))))
        .find(|path| !path.exists())
        .ok_or("The Canva Sync folder has too many videos of that name.")?;
    if std::fs::rename(from, &to).is_ok() {
        return Ok(to);
    }
    // On another disk: copied into a new hidden file there (a name of its own, made new, so
    // nothing already there is written through), then renamed.
    let hidden = folder.join(format!(".canva-sync-{}.part", base64(&random_bytes(9), URL_SAFE)));
    let copied = std::fs::File::open(from).and_then(|mut source| {
        let mut made = std::fs::OpenOptions::new().write(true).create_new(true).open(&hidden)?;
        std::io::copy(&mut source, &mut made)?;
        made.sync_all()
    });
    copied.and_then(|()| std::fs::rename(&hidden, &to)).map_err(|e| {
        let _ = std::fs::remove_file(&hidden);
        format!("The new video couldn't be put in the Canva Sync folder ({e}).")
    })?;
    let _ = std::fs::remove_file(from);
    Ok(to)
}

/// True for a plain file name: no folder in it, not hidden, nothing odd.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
        && Path::new(name).file_name().is_some_and(|only| only == name)
}

/// The videos Canva Sync made before (by the names it kept) that are still in the folder, which
/// the new one replaces. A file anyone else put there is never one of them.
fn others_in(folder: &Path, kept: &Path, made: &[String]) -> Vec<PathBuf> {
    let mut others: Vec<PathBuf> = made
        .iter()
        .filter(|name| is_plain_name(name))
        .map(|name| folder.join(name))
        .filter(|path| path != kept && path.is_file())
        .collect();
    others.sort();
    others.dedup();
    others
}

/// What a sync made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Synced {
    /// The new video, in the folder.
    pub path: PathBuf,
    pub bytes: u64,
    /// The videos it replaces, to go once nothing plays them.
    pub others: Vec<PathBuf>,
    /// How many pages it has.
    pub pages: usize,
}

/// How far a sync has got.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    SigningIn,
    /// Bringing in the old sync script's sign-in.
    Importing,
    ReadingPages,
    Starting,
    Exporting {
        since: Instant,
    },
    Downloading {
        done: u64,
        total: Option<u64>,
    },
}

impl Phase {
    pub fn busy(&self) -> bool {
        *self != Phase::Idle
    }

    /// In a few words: "Exporting… 1:23", "Downloading… 45%".
    pub fn words(&self) -> String {
        match self {
            Phase::Idle => String::new(),
            Phase::SigningIn => "Waiting for the sign-in in the browser…".to_owned(),
            Phase::Importing => "Bringing in the old script's sign-in…".to_owned(),
            Phase::ReadingPages => "Reading the design's pages…".to_owned(),
            Phase::Starting => "Starting…".to_owned(),
            Phase::Exporting { since } => {
                let seconds = since.elapsed().as_secs();
                format!("Exporting… {}:{:02}", seconds / 60, seconds % 60)
            }
            Phase::Downloading { done, total: Some(total) } if *total > 0 => {
                format!("Downloading… {}%", (done * 100 / total).min(100))
            }
            Phase::Downloading { done, .. } => format!("Downloading… {:.1} MB", *done as f64 / 1_000_000.0),
        }
    }
}

/// A sync: everything it needs on its thread.
struct SyncJob {
    places: Places,
    secrets: Option<Secrets>,
    unsaved: bool,
    design: String,
    divider: String,
    /// The videos the app made before, by name (see `Settings::made`).
    made: Vec<String>,
    cancel: Arc<AtomicBool>,
    progress: Sender<Event>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Runs a sync to its end: the video put in place, or why not (the video there stays as it
/// was). Hands back the sign-in as it now is.
fn run_sync(mut job: SyncJob) -> (Option<Secrets>, bool, Result<Synced, Failure>) {
    let outcome = sync_steps(&mut job);
    (job.secrets, job.unsaved, outcome)
}

fn sync_steps(job: &mut SyncJob) -> Result<Synced, Failure> {
    let say = |job: &SyncJob, phase: Phase| {
        let _ = job.progress.send(Event::Progress(phase));
        if let Some(wake) = &job.wake {
            wake();
        }
    };
    say(job, Phase::Starting);
    let mut token = access_token(&job.places, &mut job.secrets, &mut job.unsaved, false)?;
    // An access token Canva refuses (taken back early) is refreshed once.
    let listed = match pages(&job.places, &token, &job.design) {
        Err(Failure::Other(why)) if why.starts_with("Canva refused the sign-in") => {
            token = access_token(&job.places, &mut job.secrets, &mut job.unsaved, true)?;
            pages(&job.places, &token, &job.design)
        }
        listed => listed,
    }?;
    let above = pages_above(&listed, &job.divider).map_err(Failure::Other)?;
    if job.cancel.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    say(job, Phase::Exporting { since: Instant::now() });
    let url = export(&job.places, &token, &job.design, &above, &job.cancel)?;
    let staging = job.places.staging.join(format!("{DOWNLOADING}{}.mp4.part", base64(&random_bytes(9), URL_SAFE)));
    std::fs::create_dir_all(&job.places.staging).map_err(|e| other(format!("No room for the download ({e}).")))?;
    let mut last_said = Instant::now() - Duration::from_secs(1);
    let mut progress = |done: u64, total: Option<u64>| {
        if last_said.elapsed() >= Duration::from_millis(250) || total == Some(done) {
            last_said = Instant::now();
            say(job, Phase::Downloading { done, total });
        }
    };
    let downloaded = download(&job.places, &url, &staging, &job.cancel, &mut progress);
    let checked = downloaded.and_then(|_| looks_like_video(&staging).map_err(Failure::Other));
    let bytes = match checked {
        Ok(bytes) => bytes,
        Err(failure) => {
            let _ = std::fs::remove_file(&staging);
            return Err(failure);
        }
    };
    let name = video_name(chrono::Local::now());
    let path = put_in_place(&staging, &job.places.folder, &name).map_err(|why| {
        let _ = std::fs::remove_file(&staging);
        other(why)
    })?;
    let others = others_in(&job.places.folder, &path, &job.made);
    Ok(Synced { path, bytes, others, pages: above.len() })
}

/// How a download in the staging folder is named, before its random part.
const DOWNLOADING: &str = "download-";

/// Removes downloads left in the staging folder (by a quit or a crash part way through).
fn clear_staging(staging: &Path) {
    let Ok(entries) = std::fs::read_dir(staging) else { return };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(DOWNLOADING) && name.ends_with(".part") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Signing in.

/// URL-safe base64 with no padding, and the usual kind.
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let digits = chunk.len() + 1;
        for at in 0..4 {
            if at < digits {
                out.push(char::from(alphabet[(n >> (18 - 6 * at) & 63) as usize]));
            } else if alphabet == STANDARD {
                out.push('=');
            }
        }
    }
    out
}

/// Random bytes from the system's secure source (each version-4 id is 122 random bits from it).
fn random_bytes(count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count + 16);
    while bytes.len() < count {
        bytes.extend_from_slice(sp_model::Uuid::new_v4().as_bytes());
    }
    bytes.truncate(count);
    bytes
}

/// A sign-in's proof: the verifier (kept here), its challenge (sent), and the state.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl Pkce {
    pub fn new() -> Pkce {
        let verifier = base64(&random_bytes(48), URL_SAFE);
        Pkce { challenge: challenge_of(&verifier), verifier, state: base64(&random_bytes(24), URL_SAFE) }
    }
}

/// The S256 challenge for a verifier.
pub fn challenge_of(verifier: &str) -> String {
    use sha2::Digest;
    base64(&sha2::Sha256::digest(verifier.as_bytes()), URL_SAFE)
}

/// Percent-encodes a value for an address's query.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Undoes percent-encoding (and `+` for a space).
fn decoded(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' if at + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or(""), 16) {
                    Ok(b) => {
                        out.push(b);
                        at += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The address of Canva's sign-in page for this integration.
pub fn authorize_url(places: &Places, client_id: &str, pkce: &Pkce, redirect: &str) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        places.authorize,
        encoded(client_id),
        encoded(redirect),
        encoded(SCOPE),
        encoded(&pkce.challenge),
        encoded(&pkce.state)
    )
}

/// The query of a request line `GET /path?query HTTP/1.1`, if it is for `path`.
fn query_for<'a>(request_line: &'a str, path: &str) -> Option<&'a str> {
    let target = request_line.strip_prefix("GET ")?.split(' ').next()?;
    let (at, query) = target.split_once('?').unwrap_or((target, ""));
    (at == path).then_some(query)
}

/// One value of a query.
fn query_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (decoded(key) == name).then(|| decoded(value))
    })
}

/// The first line of a request (up to its line end), read in as many pieces as it comes in,
/// within a few seconds and 8 KB.
fn request_line(stream: &mut std::net::TcpStream) -> String {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let (until, mut head, mut piece) = (Instant::now() + Duration::from_secs(5), Vec::new(), [0u8; 1024]);
    while head.len() < 8192 && Instant::now() < until && !head.windows(2).any(|two| two == b"\r\n") {
        match stream.read(&mut piece) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(&piece[..read]),
        }
    }
    let line_end = head.windows(2).position(|two| two == b"\r\n").unwrap_or(head.len());
    String::from_utf8_lossy(&head[..line_end]).into_owned()
}

/// Waits for the browser to come back with the sign-in's answer: the one request to the
/// redirect path carrying our `state`. One with another state, or none, is told so and passed
/// over (it can't end the sign-in for anyone else); anything else asked of the listener is told
/// it isn't there. The listener is let go when this returns. The code, if the operator agreed.
fn wait_for_answer(listener: std::net::TcpListener, state: &str, cancel: &AtomicBool) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|_| "The sign-in couldn't listen for Canva's answer.".to_owned())?;
    let until = Instant::now() + SIGN_IN_WAIT;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("The sign-in was stopped.".to_owned());
        }
        if Instant::now() > until {
            return Err("No answer came from the browser in five minutes; try Sign In again.".to_owned());
        }
        let (mut stream, from) = match listener.accept() {
            Ok(accepted) => accepted,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            // Out of files, say: a pause, not a spin.
            Err(_) => {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
        };
        if !from.ip().is_loopback() {
            continue;
        }
        let line = request_line(&mut stream);
        let Some(query) = query_for(&line, REDIRECT_PATH) else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        };
        let reply = |stream: &mut std::net::TcpStream, status: &str, words: &str| {
            let page = format!(
                "<!doctype html><meta charset=utf-8><title>SoftPresenter</title>\
                 <body style=\"font:16px system-ui;margin:3em\"><p>{words}</p></body>"
            );
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
                     Cache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                )
                .as_bytes(),
            );
        };
        // Compared without stopping at the first difference.
        let given = query_value(query, "state").unwrap_or_default();
        let same =
            given.len() == state.len() && given.bytes().zip(state.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0;
        if !same {
            reply(
                &mut stream,
                "400 Bad Request",
                "This answer isn't for SoftPresenter's sign-in. Nothing was changed.",
            );
            crate::applog::problem("canva", "an answer to the sign-in came with the wrong state; it was passed over");
            continue;
        }
        if let Some(error) = query_value(query, "error") {
            reply(&mut stream, "200 OK", "Canva Sync wasn't signed in. You can close this tab.");
            return Err(match error.as_str() {
                "access_denied" => "Canva Sync wasn't allowed in Canva; nothing was changed.".to_owned(),
                _ => "Canva didn't sign Canva Sync in; nothing was changed.".to_owned(),
            });
        }
        let Some(code) = query_value(query, "code").filter(|code| !code.is_empty()) else {
            reply(&mut stream, "400 Bad Request", "Canva's answer had no sign-in in it.");
            return Err("Canva's answer had no sign-in in it; try again.".to_owned());
        };
        reply(&mut stream, "200 OK", "Signed in to Canva. You can close this tab and go back to SoftPresenter.");
        return Ok(code);
    }
}

/// Opens an address in the computer's own browser.
fn open_in_browser(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(url).spawn().map(|_| ()).map_err(|_| "The browser couldn't be opened for the sign-in.".to_owned())
}

/// Testing aid: follows a stand-in's sign-in page as a browser would (its answer is a redirect
/// back to the listener), on a thread of its own so the sign-in can wait for it.
fn follow_like_a_browser(url: &str) -> Result<(), String> {
    let url = url.to_owned();
    std::thread::spawn(move || {
        let agent: ureq::Agent =
            ureq::Agent::config_builder().max_redirects(3).timeout_global(Some(Duration::from_secs(10))).build().into();
        let _ = agent.get(&url).call();
    });
    Ok(())
}

/// Signs in with the integration's ID and secret: Canva's sign-in page in the browser (`open`
/// opens it), the answer taken on 127.0.0.1, exchanged for tokens with the proof, and the whole
/// sign-in kept (and read back) before it counts. The error is a sentence.
fn sign_in(
    places: &Places,
    client_id: &str,
    client_secret: &str,
    open: impl FnOnce(&str) -> Result<(), String>,
    cancel: &AtomicBool,
) -> Result<Secrets, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", places.redirect_port)).map_err(|_| {
        format!(
            "Port {} on this computer is in use, so the sign-in can't hear Canva's answer. Quit what uses it, then \
             try again.",
            places.redirect_port
        )
    })?;
    let port = listener.local_addr().map(|at| at.port()).unwrap_or(places.redirect_port);
    let redirect = places.redirect(port);
    let pkce = Pkce::new();
    open(&authorize_url(places, client_id, &pkce, &redirect))?;
    let code = wait_for_answer(listener, &pkce.state, cancel)?;
    let mut secrets =
        Secrets { client_id: client_id.to_owned(), client_secret: client_secret.to_owned(), ..Secrets::default() };
    let reply = token_request(
        places,
        &secrets,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", &pkce.verifier),
            ("redirect_uri", &redirect),
        ],
    )
    .map_err(|failure| failure.words())?;
    take_tokens(&mut secrets, &reply).map_err(|failure| failure.words())?;
    let _lock = take_lock(&places.lock)?;
    places.store.save(&secrets)?;
    crate::applog::event("canva", "signed in, and the sign-in was kept");
    Ok(secrets)
}

// ---------------------------------------------------------------------------------------------
// The old sync script.

/// What runs the old script, or shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OldKind {
    /// A system background job (a LaunchDaemon).
    Daemon,
    /// A job in the user's session (a LaunchAgent).
    Agent,
    /// A menu bar plugin.
    Plugin,
}

/// One of the old script's jobs or plugins found on this computer, and a background job's
/// label as its file gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OldJob {
    pub kind: OldKind,
    pub path: PathBuf,
    pub label: Option<String>,
}

/// A background job's label, as its property list gives it (`<key>Label</key><string>…`), if
/// it is one launchd takes: letters, digits, dots, dashes and underscores.
fn plist_label(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let after = &text[text.find("<key>Label</key>")? + "<key>Label</key>".len()..];
    let value = after.trim_start().strip_prefix("<string>")?;
    let label = value[..value.find("</string>")?].trim();
    (!label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)))
        .then(|| label.to_owned())
}

/// True when the old sync script's sign-in is on this computer.
pub fn old_script_here(places: &Places) -> bool {
    places.old_script.join("creds.env").is_file() && places.old_script.join("tokens.json").is_file()
}

/// The old script's jobs and plugin: the files in their folders that name its script or folder.
pub fn old_jobs(places: &Places) -> Vec<OldJob> {
    let mut found = Vec::new();
    for (kind, dir) in &places.old_jobs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            let small = std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() < 256 * 1024);
            let Some(bytes) = std::fs::read(&path).ok().filter(|_| small) else { continue };
            if bytes.windows(10).any(|w| w == b"canva_sync" || w == b"canva-sync") {
                let label = if *kind == OldKind::Plugin { None } else { plist_label(&bytes) };
                found.push(OldJob { kind: *kind, path, label });
            }
        }
    }
    found
}

/// A word as the Terminal takes it whatever is in it: in single quotes, a quote in it written
/// `'\''`. (These lines are pasted with sudo.)
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// A path for a line to paste in the Terminal: whole, and quoted.
fn for_terminal(path: &Path) -> String {
    quoted(&path.to_string_lossy())
}

/// How to switch the old script off, a line each, from what was found on this computer. A job
/// whose label can't be read from its file is said so, not guessed from the file's name.
pub fn switch_off_steps(places: &Places, jobs: &[OldJob]) -> Vec<String> {
    let mut steps = Vec::new();
    for job in jobs {
        let file = for_terminal(&job.path);
        match (job.kind, &job.label) {
            (OldKind::Daemon, Some(label)) => {
                steps.push(format!("sudo launchctl bootout {}", quoted(&format!("system/{label}"))));
                steps.push(format!("sudo rm {file}"));
            }
            (OldKind::Agent, Some(label)) => {
                steps.push(format!("launchctl bootout \"gui/$(id -u)/\"{}", quoted(label)));
                steps.push(format!("rm {file}"));
            }
            (OldKind::Daemon | OldKind::Agent, None) => {
                steps.push(format!("(its label couldn't be read: unload it by hand before removing it) {file}"));
            }
            (OldKind::Plugin, _) => steps.push(format!("rm {file}")),
        }
    }
    if jobs.is_empty() {
        steps.push(
            "Switch off whatever starts the old sync script: its background job (a LaunchDaemon or \
                    LaunchAgent) and any menu bar plugin that runs it."
                .to_owned(),
        );
    }
    steps.push(format!(
        "When Canva Sync has made a video, delete {} (it still holds the integration's secret).",
        for_terminal(&places.old_script)
    ));
    steps
}

/// What bringing in the old script's sign-in did.
pub struct Imported {
    pub secrets: Secrets,
    /// True when its token file couldn't be taken away: the old script could still refresh the
    /// sign-in, so nothing is synced until it is switched off.
    pub token_file_left: bool,
    /// Anything the operator should know, in words.
    pub notes: Vec<String>,
}

/// Brings in the old sync script's sign-in: read here (nobody pastes it anywhere), kept and read
/// back, and only then its token file taken away, so the old script can't refresh the token
/// (which would break this one's). All of it is done holding the lock the old script takes while
/// it runs (as the same user), so it can't refresh meanwhile. The client ID and secret file is
/// left for the operator.
fn import_old(places: &Places) -> Result<Imported, String> {
    // The old script locks this file (making it if need be) for as long as it runs. It is made
    // here if it isn't there, so the script can't start part way through.
    let lock_path = places.old_script.join("lock");
    let opened = if lock_path.exists() {
        std::fs::OpenOptions::new().read(true).open(&lock_path)
    } else {
        std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path)
    };
    let script_lock = opened.map_err(|_| {
        "The old sync script's lock couldn't be opened, so its sign-in wasn't brought in (the script may be \
         running): try again in a few minutes."
            .to_owned()
    })?;
    if script_lock.try_lock().is_err() {
        return Err("The old sync script is running right now; try again in a few minutes.".to_owned());
    }
    let mut notes = Vec::new();
    // A file it was writing when it stopped is left alone (its token may be the one that works).
    if places.old_script.join("tokens.json.tmp").exists() {
        notes.push(
            "The old script left a half-saved tokens.json.tmp behind; it was left alone. If Canva refuses the \
             sign-in, sign in again."
                .to_owned(),
        );
    }
    let creds = std::fs::read_to_string(places.old_script.join("creds.env"))
        .map_err(|_| "The old script's creds.env couldn't be read.".to_owned())?;
    let value = |name: &str| {
        creds.lines().map(str::trim).filter(|line| !line.starts_with('#')).find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then(|| value.trim().trim_matches('"').to_owned())
        })
    };
    let (Some(client_id), Some(client_secret)) = (value("CANVA_CLIENT_ID"), value("CANVA_CLIENT_SECRET")) else {
        return Err("The old script's creds.env doesn't have a client ID and secret.".to_owned());
    };
    let tokens: serde_json::Value = std::fs::read(places.old_script.join("tokens.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("The old script's tokens.json couldn't be read.")?;
    let refresh = tokens.get("refresh_token").and_then(serde_json::Value::as_str).filter(|t| !t.is_empty());
    let Some(refresh) = refresh else {
        return Err("The old script's tokens.json has no refresh token.".to_owned());
    };
    if client_id.is_empty() || client_secret.is_empty() {
        return Err("The old script's creds.env doesn't have a client ID and secret.".to_owned());
    }
    let secrets = Secrets { client_id, client_secret, refresh_token: refresh.to_owned(), ..Secrets::default() };
    {
        let _lock = take_lock(&places.lock)?;
        places.store.save(&secrets)?;
    }
    let token_file_left = match std::fs::remove_file(places.old_script.join("tokens.json")) {
        Ok(()) => false,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            crate::applog::problem("canva", &format!("the old script's token file couldn't be taken away ({e})"));
            notes.push(format!(
                "Its token file couldn't be taken away ({e}), so the old script could still refresh the sign-in: \
                 nothing is synced until you switch it off and click It's Switched Off."
            ));
            true
        }
    };
    drop(script_lock);
    crate::applog::event("canva", "brought in the old sync script's sign-in, and kept it");
    Ok(Imported { secrets, token_file_left, notes })
}

// ---------------------------------------------------------------------------------------------
// The design's link.

/// A design's id from its link (`https://www.canva.com/design/DAF…/…/edit`) or the id itself.
pub fn design_id(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let id = match typed.find("/design/") {
        Some(at) => typed[at + "/design/".len()..].split(['/', '?', '#']).next().unwrap_or(""),
        None => typed,
    };
    (id.len() >= 6 && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .then(|| id.to_owned())
}

// ---------------------------------------------------------------------------------------------
// The connection, on the main thread.

/// A page as the window shows it, to choose the divider: its number, id and small picture.
#[derive(Clone, Debug, PartialEq)]
pub struct PageView {
    pub number: u32,
    pub id: Option<String>,
    pub picture: Option<Arc<sp_video::Frame>>,
}

/// What a thread hands back.
enum Event {
    SecretsRead(Result<Option<Secrets>, String>),
    OldScript(bool, Vec<OldJob>),
    SignedIn(Result<Secrets, String>),
    Imported(Result<Imported, String>),
    /// A job's thread stopped without finishing (it panicked).
    Broke,
    Pages(Option<Secrets>, bool, Result<Vec<PageView>, Failure>),
    Progress(Phase),
    Synced(Option<Secrets>, bool, Result<Synced, Failure>),
}

/// How Canva Sync stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// The kept sign-in is being read.
    #[default]
    Reading,
    NotConnected,
    Connected,
    /// Canva answered a refresh in a way that couldn't be read: the sign-in may be used up. Syncs
    /// are still tried; one that works puts it back to Connected, one Canva refuses to Refused.
    Doubtful(String),
    /// Canva no longer takes the sign-in; nothing is asked until the operator signs in again.
    Refused(String),
}

/// The connection to Canva, its choices, and the job under way (one at a time).
pub struct Client {
    pub places: Places,
    secrets: Option<Secrets>,
    /// The sign-in held here is newer than the one kept (keeping it failed).
    unsaved: bool,
    pub status: Status,
    pub phase: Phase,
    pub settings: Settings,
    pub pages: Vec<PageView>,
    /// The old script's sign-in is on this computer, and what runs it.
    pub old_here: bool,
    pub old_jobs: Vec<OldJob>,
    /// The last thing that went wrong, or was done, in words, and when.
    pub said: Option<(String, bool, chrono::DateTime<chrono::Local>)>,
    /// A sync asked for before the kept sign-in was read: it starts once it has been.
    sync_waiting: bool,
    /// A finished sync, for the app to take in (the media bin, the playing video).
    pub finished: Option<Result<Synced, String>>,
    /// The `sp canva sync` job waiting on the sync under way, and old videos to go once nothing
    /// plays them.
    pub job: Option<u64>,
    pub old_files: Vec<PathBuf>,
    /// Counts changes, so the window copies what it shows only when something changed.
    pub version: u64,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Client {
    /// The connection as it was left: the kept sign-in is read on a thread of its own (macOS may
    /// first ask whether the app may have it), and the old script looked for.
    pub fn new(places: Places, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Client {
        let (tx, rx) = channel();
        let settings = Settings::load(&places.settings);
        let (reader, store, look) = (tx.clone(), places.store.clone(), places.clone());
        let wake_reader = wake.clone();
        // Videos it made that a quit or a crash left behind beside the newest go once nothing
        // uses them (see `remove_replaced_canva_videos`).
        let old_files = settings
            .made
            .iter()
            .filter(|name| Some(*name) != settings.updated_file.as_ref())
            .map(|name| places.folder.join(name))
            .filter(|path| path.is_file())
            .collect();
        let _ = std::thread::Builder::new().name("canva sign-in".to_owned()).spawn(move || {
            let _ = reader.send(Event::SecretsRead(store.load()));
            let _ = reader.send(Event::OldScript(old_script_here(&look), old_jobs(&look)));
            if let Some(wake) = wake_reader {
                wake();
            }
        });
        Client {
            places,
            secrets: None,
            unsaved: false,
            status: Status::Reading,
            phase: Phase::Idle,
            settings,
            pages: Vec::new(),
            old_here: false,
            old_jobs: Vec::new(),
            said: None,
            sync_waiting: false,
            finished: None,
            job: None,
            old_files,
            version: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            thread: None,
            tx,
            rx,
            wake,
        }
    }

    /// The client ID's first characters, while signed in.
    pub fn client_hint(&self) -> Option<String> {
        self.secrets.as_ref().map(|secrets| hint_of(&secrets.client_id))
    }

    fn say(&mut self, words: impl Into<String>, good: bool) {
        let words = words.into();
        if good {
            crate::applog::event("canva", &words);
        } else {
            crate::applog::problem("canva", &words);
        }
        self.said = Some((words, good, chrono::Local::now()));
        self.version += 1;
    }

    /// Says something that went wrong, on the page and in the log.
    pub fn say_problem(&mut self, words: String) {
        self.say(words, false);
    }

    /// Starts a job on a thread of its own, unless one is under way. The error is a sentence.
    fn start(
        &mut self,
        phase: Phase,
        work: impl FnOnce(Sender<Event>, Arc<AtomicBool>) + Send + 'static,
    ) -> Result<(), String> {
        if self.phase.busy() {
            return Err(format!("Canva Sync is busy: {}", self.phase.words()));
        }
        self.cancel = Arc::new(AtomicBool::new(false));
        let (tx, cancel, wake) = (self.tx.clone(), self.cancel.clone(), self.wake.clone());
        let started = std::thread::Builder::new().name("canva sync".to_owned()).spawn(move || {
            // A job that panics still ends: Canva Sync isn't left busy for good.
            let broke = tx.clone();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(tx, cancel))).is_err() {
                let _ = broke.send(Event::Broke);
            }
            if let Some(wake) = wake {
                wake();
            }
        });
        match started {
            Ok(thread) => {
                self.thread = Some(thread);
                self.phase = phase;
                self.version += 1;
                Ok(())
            }
            Err(_) => Err("Canva Sync couldn't start.".to_owned()),
        }
    }

    /// Signs in in the browser with the integration's client ID and secret.
    pub fn sign_in(&mut self, client_id: &str, client_secret: &str) -> Result<(), String> {
        let (id, secret) = (client_id.trim().to_owned(), client_secret.trim().to_owned());
        if id.is_empty()
            || secret.is_empty()
            || id.chars().any(char::is_whitespace)
            || secret.chars().any(char::is_whitespace)
        {
            return Err("Give the integration's client ID and its secret, from Canva's developer portal.".to_owned());
        }
        let places = self.places.clone();
        // Testing aid: SOFTPRESENTER_CANVA_BROWSER=fetch, with a stand-in for Canva on this
        // computer, has the app follow the sign-in page itself, as a browser would, so a test copy
        // signs in without opening the computer's browser.
        let fetch = places.stand_in && std::env::var("SOFTPRESENTER_CANVA_BROWSER").as_deref() == Ok("fetch");
        self.start(Phase::SigningIn, move |tx, cancel| {
            let signed = if fetch {
                sign_in(&places, &id, &secret, follow_like_a_browser, &cancel)
            } else {
                sign_in(&places, &id, &secret, open_in_browser, &cancel)
            };
            let _ = tx.send(Event::SignedIn(signed));
        })
    }

    /// Brings in the old sync script's sign-in.
    pub fn import_old(&mut self) -> Result<(), String> {
        let places = self.places.clone();
        self.start(Phase::Importing, move |tx, _| {
            let _ = tx.send(Event::Imported(import_old(&places)));
        })
    }

    /// Stops what is under way (a sign-in waiting for the browser, a sync).
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Forgets the sign-in on this computer (it stays in Canva until taken back there). Not
    /// while a job runs: it may be keeping a refreshed sign-in, which would come back.
    pub fn disconnect(&mut self) -> Result<(), String> {
        if self.phase.busy() {
            return Err(format!("Canva Sync is busy ({}): stop it first, then disconnect.", self.phase.words()));
        }
        self.places.store.forget();
        self.secrets = None;
        self.unsaved = false;
        self.status = Status::NotConnected;
        self.pages.clear();
        self.say("Canva Sync's sign-in was forgotten on this computer.", true);
        Ok(())
    }

    /// The operator says the old script is switched off: the warning goes, and syncs held back
    /// because its token file couldn't be taken away may run.
    pub fn old_script_off(&mut self) {
        self.settings.old_script_warning = false;
        self.settings.hold_syncs = false;
        let _ = self.settings.save(&self.places.settings);
        self.version += 1;
    }

    /// Takes a design's link (or id), and reads its pages to choose the divider from.
    pub fn set_design(&mut self, typed: &str) -> Result<(), String> {
        let design =
            design_id(typed).ok_or("That isn't a Canva design's link: copy it from the design's Share menu.")?;
        if self.settings.design.as_deref() != Some(design.as_str()) {
            self.settings.design = Some(design);
            self.settings.divider = None;
            self.settings.divider_number = None;
            self.settings.save(&self.places.settings)?;
        }
        self.read_pages()
    }

    /// Reads the design's pages, with their small pictures.
    pub fn read_pages(&mut self) -> Result<(), String> {
        let design = self.settings.design.clone().ok_or("Give the design's link first.")?;
        self.signed_in()?;
        let (places, mut secrets, mut unsaved) = (self.places.clone(), self.secrets.clone(), self.unsaved);
        self.start(Phase::ReadingPages, move |tx, cancel| {
            let listed = access_token(&places, &mut secrets, &mut unsaved, false)
                .and_then(|token| pages(&places, &token, &design))
                .and_then(|pages| {
                    if pages.iter().any(|page| page.id.is_none()) {
                        return Err(other(
                            "Canva doesn't give this design's pages ids, so a divider page can't be kept track of.",
                        ));
                    }
                    // Stopped (quitting, say) between two pictures, it stops there.
                    let mut views = Vec::with_capacity(pages.len());
                    for page in pages {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(Failure::Cancelled);
                        }
                        views.push(PageView {
                            number: page.number,
                            id: page.id.clone(),
                            picture: thumbnail(&places, &page),
                        });
                    }
                    Ok(views)
                });
            let _ = tx.send(Event::Pages(secrets, unsaved, listed));
        })
    }

    /// Makes a page the divider: the pages above it are exported.
    pub fn choose_divider(&mut self, id: &str) -> Result<(), String> {
        let page =
            self.pages.iter().find(|page| page.id.as_deref() == Some(id)).ok_or("That page isn't in the design.")?;
        self.settings.divider = page.id.clone();
        self.settings.divider_number = Some(page.number);
        self.settings.save(&self.places.settings)?;
        self.version += 1;
        Ok(())
    }

    /// The error when not signed in, or the sign-in was refused, or syncs are held back until
    /// the old script is switched off.
    fn signed_in(&self) -> Result<(), String> {
        if self.settings.hold_syncs {
            return Err(
                "The old sync script could still refresh the sign-in (its token file couldn't be taken away): \
                        switch it off, then click It's Switched Off in Settings › Integrations › Canva Sync."
                    .to_owned(),
            );
        }
        match &self.status {
            Status::Connected | Status::Doubtful(_) => Ok(()),
            Status::Refused(why) => Err(why.clone()),
            Status::Reading => Err("Canva Sync's sign-in is still being read.".to_owned()),
            Status::NotConnected => Err("Canva Sync isn't signed in: Settings › Integrations › Canva Sync.".to_owned()),
        }
    }

    /// Starts a sync, unless one is under way. One asked for while the kept sign-in is still
    /// being read starts once it has been. The error is a sentence.
    pub fn sync_now(&mut self) -> Result<(), String> {
        if self.status == Status::Reading {
            self.sync_waiting = true;
            return Ok(());
        }
        self.signed_in()?;
        let (Some(design), Some(divider)) = (self.settings.design.clone(), self.settings.divider.clone()) else {
            return Err(
                "Choose the design and its divider page first: Settings › Integrations › Canva Sync.".to_owned()
            );
        };
        let job = SyncJob {
            places: self.places.clone(),
            secrets: self.secrets.clone(),
            unsaved: self.unsaved,
            design,
            divider,
            made: self.settings.made.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: self.tx.clone(),
            wake: self.wake.clone(),
        };
        self.start(Phase::Starting, move |tx, cancel| {
            let job = SyncJob { cancel, ..job };
            let (secrets, unsaved, outcome) = run_sync(job);
            let _ = tx.send(Event::Synced(secrets, unsaved, outcome));
        })?;
        crate::applog::event("canva", "sync started");
        Ok(())
    }

    /// Takes in what the threads found. Says when to look again.
    pub fn tick(&mut self) -> Option<Instant> {
        while let Ok(event) = self.rx.try_recv() {
            self.version += 1;
            match event {
                Event::SecretsRead(read) => {
                    // Taken only while nothing newer is held: a sign-in made or refreshed meanwhile
                    // is never replaced by the older one read.
                    match read {
                        _ if self.secrets.is_some() => {}
                        Ok(Some(secrets)) => {
                            self.secrets = Some(secrets);
                            self.status = Status::Connected;
                        }
                        Ok(None) => self.status = Status::NotConnected,
                        Err(why) => {
                            self.status = Status::NotConnected;
                            self.say(why, false);
                        }
                    }
                    if std::mem::take(&mut self.sync_waiting)
                        && let Err(why) = self.sync_now()
                    {
                        self.say(why.clone(), false);
                        self.finished = Some(Err(why));
                    }
                }
                Event::OldScript(here, jobs) => {
                    self.old_here = here;
                    self.old_jobs = jobs;
                }
                Event::SignedIn(result) => {
                    self.thread = None;
                    self.phase = Phase::Idle;
                    match result {
                        Ok(secrets) => {
                            self.secrets = Some(secrets);
                            self.unsaved = false;
                            self.status = Status::Connected;
                            self.say("Signed in to Canva.", true);
                        }
                        Err(why) => self.say(why, false),
                    }
                }
                Event::Imported(result) => {
                    self.thread = None;
                    self.phase = Phase::Idle;
                    match result {
                        Ok(imported) => {
                            self.secrets = Some(imported.secrets);
                            self.unsaved = false;
                            self.status = Status::Connected;
                            self.old_here = false;
                            self.settings.old_script_warning = true;
                            self.settings.hold_syncs = imported.token_file_left;
                            let _ = self.settings.save(&self.places.settings);
                            let mut said =
                                "Brought in the old sync script's sign-in. Switch the old script off now.".to_owned();
                            for note in &imported.notes {
                                said.push(' ');
                                said.push_str(note);
                            }
                            self.say(said, !imported.token_file_left);
                        }
                        Err(why) => self.say(why, false),
                    }
                }
                Event::Broke => {
                    self.thread = None;
                    self.phase = Phase::Idle;
                    self.say("Canva Sync stopped part way through; the video stays as it was. Try again.", false);
                    if self.job.is_some() || self.sync_waiting {
                        self.finished = Some(Err("Canva Sync stopped part way through.".to_owned()));
                    }
                }
                Event::Pages(secrets, unsaved, listed) => {
                    self.thread = None;
                    self.phase = Phase::Idle;
                    self.take_secrets(secrets, unsaved);
                    match listed {
                        Ok(pages) => {
                            self.signed_in_again();
                            self.pages = pages;
                            // The divider's number follows the page as it moves.
                            if let Some(page) = self.pages.iter().find(|p| p.id == self.settings.divider) {
                                self.settings.divider_number = Some(page.number);
                                let _ = self.settings.save(&self.places.settings);
                            }
                        }
                        Err(failure) => self.failed(failure),
                    }
                }
                Event::Progress(phase) => {
                    if self.phase.busy() {
                        self.phase = phase;
                    }
                }
                Event::Synced(secrets, unsaved, outcome) => {
                    self.thread = None;
                    self.phase = Phase::Idle;
                    self.take_secrets(secrets, unsaved);
                    match outcome {
                        Ok(synced) => {
                            self.signed_in_again();
                            let name = crate::file_name(&synced.path);
                            self.settings.updated_at = Some(chrono::Local::now().to_rfc3339());
                            self.settings.updated_file = Some(name.clone());
                            // Only the videos it made are ever deleted from its folder.
                            let folder = self.places.folder.clone();
                            self.settings.made.retain(|made| folder.join(made).is_file());
                            if !self.settings.made.contains(&name) {
                                self.settings.made.push(name.clone());
                            }
                            let _ = self.settings.save(&self.places.settings);
                            self.say(
                                format!(
                                    "Updated: {name} ({} pages, {:.1} MB).",
                                    synced.pages,
                                    synced.bytes as f64 / 1_000_000.0
                                ),
                                true,
                            );
                            self.finished = Some(Ok(synced));
                        }
                        Err(failure) => {
                            let words = failure.words();
                            self.failed(failure);
                            self.finished = Some(Err(words));
                        }
                    }
                }
            }
        }
        // A job's thread that ended without a word (it couldn't even say it broke) frees Canva
        // Sync all the same.
        if self.phase.busy() && self.thread.as_ref().is_some_and(std::thread::JoinHandle::is_finished) {
            let _ = self.tx.send(Event::Broke);
            return Some(Instant::now());
        }
        match self.phase {
            Phase::Idle => None,
            // The elapsed time is shown by the second.
            _ => Some(Instant::now() + Duration::from_millis(500)),
        }
    }

    /// A sign-in in doubt that has just worked is trusted again.
    fn signed_in_again(&mut self) {
        if matches!(self.status, Status::Doubtful(_)) {
            self.status = Status::Connected;
        }
    }

    /// A video it made has been deleted: it is no longer one of its own.
    pub fn forget_made(&mut self, name: &str) {
        let before = self.settings.made.len();
        self.settings.made.retain(|made| made != name);
        if self.settings.made.len() != before {
            let _ = self.settings.save(&self.places.settings);
        }
    }

    /// Takes the sign-in as a thread left it (refreshed, perhaps not kept).
    fn take_secrets(&mut self, secrets: Option<Secrets>, unsaved: bool) {
        if secrets.is_some() && self.status != Status::NotConnected {
            self.secrets = secrets;
            self.unsaved = unsaved;
        }
    }

    fn failed(&mut self, failure: Failure) {
        match &failure {
            Failure::SignInGone(why) => self.status = Status::Refused(why.clone()),
            Failure::Doubtful(why) => self.status = Status::Doubtful(why.clone()),
            Failure::Cancelled | Failure::Other(_) => {}
        }
        if failure != Failure::Cancelled {
            self.say(failure.words(), false);
        }
    }

    /// Stops what is under way and waits for its thread, for quitting: a refresh of the sign-in
    /// in flight is waited out (its new token must be kept; at most as long as a token request
    /// may take), anything else is given a few seconds to stop. A sign-in held here that
    /// couldn't be kept before is kept now.
    pub fn shut_down(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let started = Instant::now();
            while !thread.is_finished() {
                let refreshing = REFRESHING.load(Ordering::SeqCst);
                let waited = started.elapsed();
                if waited > TOKEN_WAIT + Duration::from_secs(5) || (!refreshing && waited > Duration::from_secs(5)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
        while let Ok(event) = self.rx.try_recv() {
            if let Event::Synced(secrets, unsaved, _) | Event::Pages(secrets, unsaved, _) = event {
                self.take_secrets(secrets, unsaved);
            }
        }
        if self.unsaved
            && let Some(secrets) = &self.secrets
            && take_lock(&self.places.lock).and_then(|_lock| self.places.store.save(secrets)).is_ok()
        {
            self.unsaved = false;
        }
        clear_staging(&self.places.staging);
    }

    /// One line for the window's top bar, while it says something: what a sync is doing, a
    /// video just put in place, or one that failed. With whether it went wrong.
    pub fn quiet_line(&self) -> Option<(String, bool)> {
        if self.phase.busy() && self.phase != Phase::SigningIn && self.phase != Phase::Importing {
            return Some((format!("Canva: {}", self.phase.words().trim_end_matches('…').replace("… ", " ")), false));
        }
        let (words, good, at) = self.said.as_ref()?;
        let age = chrono::Local::now().signed_duration_since(*at);
        if *good && words.starts_with("Updated") && age < chrono::Duration::minutes(10) {
            return Some((format!("Canva: updated {}", at.format("%-I:%M %p")), false));
        }
        (!*good && age < chrono::Duration::hours(12)).then(|| ("Canva: failed".to_owned(), true))
    }
}

/// A page's small picture, read on the thread listing the pages (no token goes with it: its
/// address is its own key). Small, so the list of pages costs little.
fn thumbnail(places: &Places, page: &Page) -> Option<Arc<sp_video::Frame>> {
    let url = page.thumbnail.as_deref()?;
    if !(url.starts_with("https://") || places.stand_in) {
        return None;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(3)
        .https_only(!places.stand_in)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut response = agent.get(url).call().ok()?;
    let bytes = response.body_mut().with_config().limit(8 * 1024 * 1024).read_to_vec().ok()?;
    // A picture claiming to be huge isn't decoded: a page's picture is a few hundred pixels.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    reader.limits(limits);
    let picture = reader.decode().ok()?;
    let picture = if picture.width() > 320 || picture.height() > 180 { picture.thumbnail(320, 180) } else { picture };
    let picture = picture.to_rgba8();
    let (width, height) = picture.dimensions();
    Some(Arc::new(sp_video::Frame { width, height, rgba: picture.into_raw() }))
}

// ---------------------------------------------------------------------------------------------
// The window's part.

/// What Canva Sync's page asks of the app. (Not `Debug`: Sign In carries the secret.)
#[derive(Clone, PartialEq, Eq)]
pub enum Request {
    /// Sign in in the browser with the integration's client ID and secret.
    SignIn(String, String),
    /// Bring in the old sync script's sign-in.
    Import,
    /// Stop the sign-in waiting for the browser, or the sync under way.
    Stop,
    Disconnect,
    /// The design's link (or id): its pages are read.
    Design(String),
    ReadPages,
    /// The divider page, by its id.
    Divider(String),
    SyncNow,
    /// The operator says the old script is switched off.
    OldScriptOff,
}

/// What the page shows (copied from the connection when it changes) and asks for. What is
/// typed into the ID and secret is let go once Sign In takes it.
#[derive(Default)]
pub struct View {
    pub open: bool,
    pub typed_id: String,
    pub typed_secret: String,
    pub typed_link: String,
    pub request: Option<Request>,
    /// A macro's Sync from Canva step asked for a sync.
    pub sync_request: bool,
    pub version: u64,
    pub status: Status,
    pub phase: Phase,
    pub client_hint: Option<String>,
    pub settings: Settings,
    pub pages: Vec<PageView>,
    /// The pages' pictures, made into textures as they are first drawn, by page id.
    pub textures: Vec<(String, egui::TextureHandle)>,
    pub old_here: bool,
    pub old_steps: Vec<String>,
    pub said: Option<(String, bool)>,
    /// What the top bar says, and whether it went wrong.
    pub quiet: Option<(String, bool)>,
    pub redirect: String,
}

impl crate::App {
    /// Carries out what Canva Sync's page or a macro asked, takes in what its thread found (a
    /// finished sync goes into the media bin), and keeps the page up to date. Says when to look
    /// again.
    pub(crate) fn tick_canva(&mut self) -> Option<Instant> {
        let (request, sync_asked) = match self.running.as_mut() {
            Some(running) => {
                let view = &mut running.home_state.canva;
                (view.request.take(), std::mem::take(&mut view.sync_request))
            }
            None => (None, false),
        };
        if sync_asked && let Err(why) = self.canva.sync_now() {
            self.canva.say_problem(why);
        }
        if let Some(request) = request {
            let done = match request {
                Request::SignIn(id, secret) => self.canva.sign_in(&id, &secret),
                Request::Import => self.canva.import_old(),
                Request::Stop => {
                    self.canva.stop();
                    Ok(())
                }
                Request::Disconnect => self.canva.disconnect(),
                Request::Design(typed) => self.canva.set_design(&typed),
                Request::ReadPages => self.canva.read_pages(),
                Request::Divider(id) => self.canva.choose_divider(&id).map(|()| self.follow_canva_folder()),
                Request::SyncNow => self.canva.sync_now(),
                Request::OldScriptOff => {
                    self.canva.old_script_off();
                    Ok(())
                }
            };
            if let Err(why) = done {
                self.canva.say_problem(why);
            }
        }
        let at = self.canva.tick();
        if let Some(finished) = self.canva.finished.take() {
            self.canva_finished(finished);
        }
        self.remove_replaced_canva_videos();
        let quiet = self.canva.quiet_line();
        let client = &self.canva;
        if let Some(running) = self.running.as_mut() {
            let view = &mut running.home_state.canva;
            let changed = view.version != client.version || view.quiet != quiet;
            if view.version != client.version {
                view.version = client.version;
                view.status = client.status.clone();
                view.client_hint = client.client_hint();
                view.settings = client.settings.clone();
                if view.pages != client.pages {
                    view.pages = client.pages.clone();
                    view.textures.clear();
                }
                view.old_here = client.old_here;
                view.old_steps = switch_off_steps(&client.places, &client.old_jobs);
                view.said = client.said.as_ref().map(|(words, good, _)| (words.clone(), *good));
                view.redirect = client.places.redirect(client.places.redirect_port);
            }
            view.phase = client.phase.clone();
            view.quiet = quiet;
            if changed || client.phase.busy() {
                running.home_window.request_redraw();
                for panel in &running.panels {
                    panel.window.request_redraw();
                }
            }
        }
        let files_at = (!self.canva.old_files.is_empty()).then(|| Instant::now() + Duration::from_secs(1));
        [at, files_at].into_iter().flatten().min()
    }

    /// Takes in a finished sync: the new video goes in place of the old in the media bin (the
    /// same item, so playlists, macros and the screens keep finding it), a video playing takes
    /// it over at the end of its pass, and the old file goes once nothing plays it. A
    /// `sp canva sync` waiting is told how it went.
    fn canva_finished(&mut self, finished: Result<Synced, String>) {
        let said = match &finished {
            Ok(synced) => format!("Canva Sync put {} in place.", crate::file_name(&synced.path)),
            Err(why) => why.clone(),
        };
        if let Some(job) = self.canva.job.take()
            && let Some((_, outcome)) = self.jobs.iter_mut().find(|(have, _)| *have == job)
        {
            *outcome = Some(finished.clone().map(|_| said.clone()));
        }
        let Ok(synced) = finished else {
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some(format!("Canva Sync: {said}"));
            }
            return;
        };
        self.follow_canva_folder();
        let name = crate::file_name(&synced.path);
        let old = self.show.media().iter().position(|item| synced.others.contains(&item.path));
        if let Some(index) = old {
            let (id, was) = (self.show.media()[index].id, self.show.media()[index].path.clone());
            self.show.set_media_file(index, name, synced.path.clone());
            if let Some(running) = self.running.as_mut() {
                for playlist in &mut running.home_state.media_playlists {
                    if playlist.has(&synced.path) {
                        playlist.files.retain(|file| *file != was);
                    } else {
                        for file in playlist.files.iter_mut().filter(|file| **file == was) {
                            *file = synced.path.clone();
                        }
                    }
                }
            }
            // The followed folder sees the new file as the one it knows, not a new one.
            self.folder_notes.seen.remove(&was);
            if let Ok(changed) = std::fs::metadata(&synced.path).and_then(|meta| meta.modified()) {
                self.folder_notes.seen.insert(synced.path.clone(), changed);
            }
            self.bin_pictures.read(id, &synced.path);
            self.forget_thumbs_showing(id);
            if let Err(problem) = self.play_new_file_next(id, &synced.path) {
                // It can't be lined up: the screens go on with the old one until it is shown again.
                crate::applog::problem("canva", &format!("the new video couldn't be lined up: {problem}"));
            }
            self.save_media_list();
        }
        self.check_folders(true);
        self.canva.old_files.extend(synced.others);
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// The media playlist that follows the Canva Sync folder: made (named Announcements, or
    /// another name if that is taken by a playlist of the church's own) if none follows it.
    fn follow_canva_folder(&mut self) {
        let folder = self.canva.places.folder.clone();
        let _ = std::fs::create_dir_all(&folder);
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        if state.media_playlists.iter().any(|playlist| playlist.folder.as_deref() == Some(folder.as_path())) {
            return;
        }
        let taken = |name: &str| state.media_playlists.iter().any(|playlist| playlist.name == name);
        let name = [PLAYLIST.to_owned(), format!("{PLAYLIST} (Canva)")]
            .into_iter()
            .chain((2..).map(|n| format!("{PLAYLIST} (Canva {n})")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        let mut playlist = crate::media::Playlist::new(name.clone());
        playlist.folder = Some(folder);
        state.media_playlists.push(playlist);
        state.media_playlists_changed = true;
        crate::applog::event("canva", &format!("made the media playlist {name}, following the Canva Sync folder"));
        self.check_folders(true);
    }

    /// Deletes the Canva videos a new one replaced (only videos Canva Sync made), once nothing
    /// plays them: every player (the media layer, the Announcements layer, videos in boxes on the
    /// slide and on the stage screens, and a file lined up to follow in any of them). One still
    /// named by something that can play it later (a media bin item, a media playlist, a slide,
    /// a theme, a prop) is the church's now and is kept.
    pub(crate) fn remove_replaced_canva_videos(&mut self) {
        if self.canva.old_files.is_empty() {
            return;
        }
        let mut playing: Vec<PathBuf> = Vec::new();
        let players = self
            .video
            .iter()
            .map(|(_, player)| player)
            .chain(self.announcing.as_ref().and_then(|on| on.video.as_ref()))
            .chain(self.slide_videos.iter().map(|(_, player)| player));
        for player in players {
            playing.push(player.path().to_owned());
            playing.extend(player.next_file().map(Path::to_path_buf));
        }
        let old = std::mem::take(&mut self.canva.old_files);
        for path in old {
            if playing.contains(&path) {
                self.canva.old_files.push(path);
                continue;
            }
            let name = crate::file_name(&path);
            if let Some(user) = self.canva_video_named(&path) {
                crate::applog::event("canva", &format!("kept the old video {name}: {user} still names it"));
                self.canva.forget_made(&name);
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    crate::applog::event("canva", &format!("removed the old video {name}"));
                    self.canva.forget_made(&name);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.canva.forget_made(&name),
                Err(e) => crate::applog::problem(
                    "canva",
                    &format!("the old video {name} couldn't be removed ({e}); it is tried again at the next start"),
                ),
            }
        }
    }

    /// What still names a file and could play it later, in words, if anything does: a media bin
    /// item, a media playlist, a picture or video box on a slide, a theme's layout or a prop.
    fn canva_video_named(&self, path: &Path) -> Option<&'static str> {
        let names = |file: &str| crate::images::resolve(file).is_some_and(|resolved| resolved == path);
        let in_objects = |objects: &[sp_model::Object]| {
            objects.iter().any(|object| matches!(&object.kind, sp_model::ObjectKind::Image { file, .. } if names(file)))
        };
        if self.show.media().iter().any(|item| item.path == path) {
            return Some("a media bin item");
        }
        if self.show.presentations().iter().any(|song| song.slides().any(|(_, slide)| in_objects(&slide.objects))) {
            return Some("a slide");
        }
        if self.themes.iter().any(|theme| theme.templates.iter().any(|template| in_objects(&template.objects))) {
            return Some("a theme");
        }
        let state = &self.running.as_ref()?.home_state;
        if state.media_playlists.iter().any(|playlist| playlist.has(path)) {
            return Some("a media playlist");
        }
        if state.props.iter().any(|prop| in_objects(&prop.slide.objects)) {
            return Some("a prop");
        }
        None
    }
}

#[cfg(test)]
mod tests;
