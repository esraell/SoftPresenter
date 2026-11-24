//! VoxLive: live translation of the sermon, shown as captions. SoftPresenter talks to a church's
//! VoxLive account through its church API (`https://voxlive.app/api/v1`), with an API key the
//! church makes in VoxLive and pastes into Settings › Integrations.
//!
//! - The key is kept in the Mac's Keychain, and is never written to a settings file, the log,
//!   a reply on the command line, a song pack, or anywhere else. A test copy of the app (one
//!   started with `SOFTPRESENTER_DATA`) keeps its own key in its own folder instead, so it never
//!   reads, changes or asks for the owner's.
//! - The key is sent only to VoxLive's address (or, for tests, to a stand-in on this computer),
//!   with redirects off, so it never follows one elsewhere.
//! - Every request runs on a thread of its own, never on the thread that draws.
//! - The screens are asked for every 30 seconds while connected. A key VoxLive refuses stops
//!   the asking until another is given (VoxLive locks out an address that keeps trying a bad
//!   key, which would stop the church's other devices too); "wait" answers are waited out.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// VoxLive's church API.
pub const LIVE_API: &str = "https://voxlive.app/api/v1";

/// How often the screens are asked for while connected.
const POLL_EVERY: Duration = Duration::from_secs(30);

/// The most a reply may be, in bytes.
const LARGEST_REPLY: u64 = 2 * 1024 * 1024;

/// The API's address. Testing aid: `SOFTPRESENTER_VOXLIVE_API` points at a stand-in for it on
/// this computer (plain http to 127.0.0.1 or localhost only, so a key can't be sent elsewhere).
pub fn api_base() -> String {
    if let Ok(base) = std::env::var("SOFTPRESENTER_VOXLIVE_API") {
        let base = base.trim().trim_end_matches('/');
        let on_this_computer = ["http://127.0.0.1:", "http://localhost:", "http://127.0.0.1/", "http://localhost/"]
            .iter()
            .any(|start| base.starts_with(start) || format!("{base}/").starts_with(start));
        if on_this_computer {
            return base.to_owned();
        }
    }
    LIVE_API.to_owned()
}

/// One of the church's VoxLive screens: a caption display, in one language.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Screen {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub language: String,
    /// `captions` (the sermon as it is spoken or translated) or `reference` (a Bible passage).
    #[serde(default)]
    pub mode: String,
    /// The live display: it joins the captions and can start a paid translation, so it is
    /// opened only on a real screen.
    #[serde(default)]
    pub url: String,
    /// The test pattern: sample text in the screen's look, free, for previews.
    #[serde(default)]
    pub demo_url: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub style: serde_json::Value,
}

impl Screen {
    /// Captions or Scripture, in words; another mode as VoxLive names it.
    pub fn kind(&self) -> &str {
        match self.mode.as_str() {
            "reference" => "Scripture",
            "captions" | "" => "captions",
            other => other,
        }
    }

    /// What it is called, with its language.
    pub fn label(&self) -> String {
        let name = if self.name.trim().is_empty() { "VoxLive screen" } else { self.name.trim() };
        if self.language.is_empty() { name.to_owned() } else { format!("{name} · {}", self.language.to_uppercase()) }
    }

    /// Cleans what VoxLive sent: no control characters in the names, and only addresses at
    /// VoxLive's own display (or the test stand-in's) kept.
    fn cleaned(mut self, base: &str) -> Screen {
        for text in [&mut self.id, &mut self.name, &mut self.language, &mut self.mode, &mut self.version] {
            text.retain(|c| !c.is_control());
        }
        let display = display_prefix(base);
        for address in [&mut self.url, &mut self.demo_url] {
            if !address.starts_with(&display) || address.chars().any(char::is_control) {
                address.clear();
            }
        }
        self
    }
}

/// Where VoxLive's caption displays are: `https://voxlive.app/listen/`, or the test stand-in's.
pub(crate) fn display_prefix(base: &str) -> String {
    match base.strip_suffix("/api/v1") {
        Some(site) if site.starts_with("http://") => format!("{site}/listen/"),
        _ => "https://voxlive.app/listen/".to_owned(),
    }
}

/// What the Integrations pane shows (a copy of the connection's state, made each frame it
/// changes) and asks for. The key typed is never kept here past Connect.
#[derive(Default)]
pub struct View {
    /// VoxLive's page is open (otherwise the list of services).
    pub open: bool,
    pub typed: String,
    pub status: Status,
    pub key_hint: Option<String>,
    pub screens: Vec<Screen>,
    /// Why the key given wasn't taken.
    pub problem: Option<String>,
    pub request: Option<crate::integrations::Request>,
    /// Every look setting, as VoxLive describes it.
    pub schema: Vec<(String, serde_json::Value)>,
    /// A screen's look being changed, if one is open.
    pub look: Option<LookEdit>,
}

/// A VoxLive screen's look, open to change: what VoxLive has, what is being made of it, and a
/// preview of it drawn by VoxLive's own display page in its preview mode (no connection, free).
pub struct LookEdit {
    pub screen: String,
    pub label: String,
    pub language: String,
    /// The look as VoxLive has it (when opened, or last saved).
    pub base: serde_json::Map<String, serde_json::Value>,
    /// The look as it is being changed.
    pub style: serde_json::Map<String, serde_json::Value>,
    /// VoxLive's version of the screen the changes were made from.
    pub version: String,
    /// The preview's newest picture, and whether it is still to be shown.
    pub preview: Option<sp_video::Frame>,
    pub fresh: bool,
    pub texture: Option<egui::TextureHandle>,
    pub saving: bool,
    /// How the last save went, in words.
    pub outcome: Option<String>,
    /// The screen as VoxLive has it now, when it changed there since the look was opened.
    pub changed_there: Option<Screen>,
}

impl LookEdit {
    /// Opens a screen's look.
    pub fn of(screen: &Screen) -> LookEdit {
        let style = screen.style.as_object().cloned().unwrap_or_default();
        LookEdit {
            screen: screen.id.clone(),
            label: screen.label(),
            language: screen.language.clone(),
            base: style.clone(),
            style,
            version: screen.version.clone(),
            preview: None,
            fresh: false,
            texture: None,
            saving: false,
            outcome: None,
            changed_there: None,
        }
    }

    /// The settings changed from what VoxLive has.
    pub fn changes(&self) -> serde_json::Map<String, serde_json::Value> {
        self.style
            .iter()
            .filter(|(key, value)| self.base.get(*key) != Some(value))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Takes the screen as VoxLive stored it (after a save, or to start again from its look).
    pub fn take(&mut self, screen: &Screen) {
        self.base = screen.style.as_object().cloned().unwrap_or_default();
        self.style = self.base.clone();
        self.version = screen.version.clone();
        self.changed_there = None;
    }
}

/// The address of a screen's preview: its display page in the preview mode for an app, which
/// connects to nothing (so it costs nothing) and draws whatever look it is given.
pub fn preview_address(url: &str) -> String {
    format!("{url}{}preview=1&host=app", if url.contains('?') { '&' } else { '?' })
}

/// The script that paints a look in the preview page: its own `vxApplyStyle`, given the whole
/// look and the screen's language (as JSON, so nothing in them can break out of the script).
pub fn apply_script(style: &serde_json::Map<String, serde_json::Value>, language: &str) -> String {
    let look = serde_json::json!({ "style": style, "lang": language });
    format!("(function(){{try{{window.vxApplyStyle({look});return 'painted'}}catch(e){{return 'not yet'}}}})()")
}

/// How a playlist names the VoxLive item.
pub const ITEM_KEY: &str = "voxlive";

/// The VoxLive item's id: always the same, so a playlist, a thumbnail or a slide on screen
/// keeps finding it as its screens change.
pub const ITEM_ID: sp_model::Uuid = sp_model::Uuid::from_u128(0x5f1d_0c0e_7a1b_4c55_9e3d_7f0a_b0c1_d2e3);

/// VoxLive's colour, for the item's groups.
pub const COLOUR: sp_model::Rgba = sp_model::Rgba { r: 44, g: 93, b: 124, a: 255 };

/// The VoxLive item: the church's VoxLive screens, a group of one slide each, named after the
/// screen. Each slide is a web page box filling the slide with the screen's live display, which
/// the screens open when the slide is shown (thumbnails show its free test pattern instead).
pub fn item(screens: &[Screen]) -> sp_model::Presentation {
    let full = sp_model::SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT };
    let groups = screens
        .iter()
        .filter(|screen| !screen.url.is_empty())
        .map(|screen| {
            let mut web = sp_model::Object::web(screen.url.clone());
            web.id = stable_id("object", &screen.id);
            web.name = screen.label();
            web.frame = Some(full);
            let mut slide = sp_model::Slide::new(vec![web]);
            slide.id = stable_id("slide", &screen.id);
            sp_model::Group {
                id: stable_id("group", &screen.id),
                name: screen.label(),
                color: COLOUR,
                slides: vec![slide],
            }
        })
        .collect();
    sp_model::Presentation {
        id: ITEM_ID,
        title: "VoxLive".to_owned(),
        groups,
        copyright: sp_model::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
        catalog: None,
    }
}

/// A key's first characters, to show which key it is.
fn hint_of(key: &str) -> String {
    format!("{}…", key.chars().take(12).collect::<String>())
}

/// True when `shown` is the hint of `key` (both none, or `hint_of` the key).
fn hint_is(key: Option<&str>, shown: Option<&str>) -> bool {
    match (key, shown) {
        (None, None) => true,
        (Some(key), Some(shown)) => shown.strip_suffix('…').is_some_and(|start| start.chars().eq(key.chars().take(12))),
        _ => false,
    }
}

/// An id made from a screen's id, the same each time, so the item's slides keep their ids.
fn stable_id(part: &str, screen: &str) -> sp_model::Uuid {
    // FNV-1a, 128 bits: not for secrecy, only to be the same every time.
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in part.bytes().chain([0]).chain(screen.bytes()) {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013B);
    }
    sp_model::Uuid::from_u128(hash)
}

/// How the connection stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    /// No key given.
    #[default]
    NoKey,
    /// A key given, its first answer awaited.
    Checking,
    Connected,
    /// VoxLive refused the key; the words say why. Nothing more is asked until another key.
    Refused(String),
    /// VoxLive couldn't be reached, or answered with a problem; it is tried again.
    Trouble(String),
}

impl Status {
    /// In a few words, for the window and the command line.
    pub fn words(&self) -> String {
        match self {
            Status::NoKey => "Not connected".to_owned(),
            Status::Checking => "Connecting…".to_owned(),
            Status::Connected => "Connected".to_owned(),
            Status::Refused(why) | Status::Trouble(why) => why.clone(),
        }
    }
}

/// What went wrong with a request.
enum Problem {
    /// The key: missing, unknown, revoked, expired, or without the permission.
    Refused(String),
    /// Asked to wait (too many requests, or VoxLive busy) for this long.
    Wait(Duration, String),
    Other(String),
}

/// What a finished request found.
enum Answer {
    /// The screens, for the connection numbered so (an answer for an earlier key is let go).
    Screens(u64, Result<Vec<Screen>, Problem>),
    /// The style schema: every look setting, as VoxLive describes it.
    Schema(u64, Result<Vec<(String, serde_json::Value)>, Problem>),
    /// A look saved (the screen as VoxLive stored it), or why not.
    Saved(String, Result<Screen, Saving>),
    /// The key kept in the Keychain, read at start-up (for the connection numbered so).
    KeyRead(u64, Option<String>),
}

/// Why a look wasn't saved.
#[derive(Clone, Debug, PartialEq)]
pub enum Saving {
    /// The screen changed in VoxLive since its look was opened: nothing was written; this is
    /// the screen as it is now.
    Changed(Box<Screen>),
    Failed(String),
}

/// The order look settings are offered in; any VoxLive adds later come after, by name.
const SETTING_ORDER: [&str; 22] = [
    "mode",
    "bg",
    "transparent",
    "text",
    "font",
    "size",
    "weight",
    "align",
    "valign",
    "padding",
    "maxLines",
    "lineHeight",
    "maxWidth",
    "letterSpacing",
    "uppercase",
    "shadow",
    "edgeSize",
    "boxOpacity",
    "fade",
    "refPosition",
    "fitHeight",
    "passage",
];

/// The connection to VoxLive: the key, the screens last found, and requests under way.
pub struct Client {
    key: Option<String>,
    pub status: Status,
    pub screens: Vec<Screen>,
    /// When the screens were last found.
    pub found_at: Option<Instant>,
    /// When to ask again.
    next: Option<Instant>,
    busy: bool,
    /// Every look setting, as VoxLive describes it (asked for once, when first connected).
    pub schema: Vec<(String, serde_json::Value)>,
    /// A request for the look settings under way, and when to ask (again) once connected.
    schema_asking: bool,
    schema_next: Option<Instant>,
    /// Counts connections (each key given or forgotten), so an answer that comes back after
    /// its key was replaced or forgotten is let go.
    generation: u64,
    /// The kept key is still being read from the Keychain.
    key_reading: bool,
    /// A look being saved, by its screen.
    pub saving: Option<String>,
    /// The last look saved or refused: its screen, and how it went.
    pub saved: Option<(String, Result<Screen, Saving>)>,
    tx: Sender<Answer>,
    rx: Receiver<Answer>,
}

impl Client {
    /// The connection as it was left: with the key kept, if there is one, being checked.
    pub fn new() -> Client {
        let (tx, rx) = channel();
        // The key is read from the Keychain on a thread of its own: macOS may first ask the
        // operator whether the app may have it (it asks again for each new build until the app
        // is signed), and the window must not wait for that answer.
        let reader = tx.clone();
        let started = std::thread::Builder::new().name("voxlive key".to_owned()).spawn(move || {
            let _ = reader.send(Answer::KeyRead(0, store::load()));
        });
        let key_reading = started.is_ok();
        Client {
            key: None,
            status: if key_reading { Status::Checking } else { Status::NoKey },
            key_reading,
            screens: Vec::new(),
            found_at: None,
            next: None,
            busy: false,
            schema: Vec::new(),
            schema_asking: false,
            schema_next: Some(Instant::now()),
            generation: 0,
            saving: None,
            saved: None,
            tx,
            rx,
        }
    }

    /// True when a key is kept.
    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// The key's first characters (VoxLive's own handle for it), to show which key it is.
    pub fn key_hint(&self) -> Option<String> {
        self.key.as_deref().map(hint_of)
    }

    /// True when `shown` is what `key_hint` gives now, found without making a new string (it
    /// is asked each time the app wakes).
    pub fn key_hint_is(&self, shown: Option<&str>) -> bool {
        hint_is(self.key.as_deref(), shown)
    }

    /// Takes a key: keeps it, and checks it by asking for the screens. The error is a sentence.
    pub fn connect(&mut self, key: &str) -> Result<(), String> {
        let key = key.trim();
        if !looks_like_key(key) {
            return Err(
                "That isn't a VoxLive API key: a key starts with vxk_ (make one in VoxLive, Settings › API keys)."
                    .to_owned(),
            );
        }
        store::save(key)?;
        self.key = Some(key.to_owned());
        self.status = Status::Checking;
        self.screens.clear();
        self.found_at = None;
        self.next = Some(Instant::now());
        self.new_connection();
        Ok(())
    }

    /// Forgets the key (here, not in VoxLive) and the screens.
    pub fn disconnect(&mut self) {
        store::forget();
        self.key = None;
        self.status = Status::NoKey;
        self.screens.clear();
        self.found_at = None;
        self.next = None;
        self.new_connection();
    }

    /// Starts afresh: requests still under way for the key before are let go when they answer.
    fn new_connection(&mut self) {
        self.generation += 1;
        // A key given or forgotten meanwhile wins over the one still being read.
        self.key_reading = false;
        self.busy = false;
        self.schema_asking = false;
        self.schema_next = self.schema.is_empty().then(Instant::now);
    }

    /// Asks for the screens now (unless a key was refused, or a request is under way).
    pub fn refresh(&mut self) {
        if self.key.is_some() && !matches!(self.status, Status::Refused(_)) {
            self.next = Some(Instant::now());
        }
    }

    /// Saves changes to a screen's look in VoxLive (live at once, wherever the screen shows):
    /// only the settings changed, merged into what VoxLive has. With `version`, only if the
    /// screen hasn't changed since (otherwise it is handed back as it is now); without, whatever
    /// changed. The outcome comes in `saved`. The error is a sentence.
    pub fn save_look(
        &mut self,
        screen: &str,
        version: Option<String>,
        changes: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), String> {
        let key = self.key.clone().ok_or("VoxLive isn't connected.")?;
        if !screen.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || screen.is_empty() {
            return Err("That isn't a VoxLive screen.".to_owned());
        }
        if self.saving.is_some() {
            return Err("A look is being saved already.".to_owned());
        }
        self.saving = Some(screen.to_owned());
        self.saved = None;
        let (tx, base, screen) = (self.tx.clone(), api_base(), screen.to_owned());
        std::thread::spawn(move || {
            let outcome = patch_look(&base, &key, &screen, version.as_deref(), changes);
            let _ = tx.send(Answer::Saved(screen, outcome));
        });
        Ok(())
    }

    /// Takes in finished requests and starts those due. Says when to look again.
    pub fn tick(&mut self) -> Option<Instant> {
        while let Ok(answer) = self.rx.try_recv() {
            // An answer for a key since replaced or forgotten changes nothing.
            if let Answer::Screens(generation, _) | Answer::Schema(generation, _) | Answer::KeyRead(generation, _) =
                &answer
                && *generation != self.generation
            {
                continue;
            }
            if let Answer::KeyRead(_, key) = answer {
                self.key_reading = false;
                match key {
                    Some(key) => {
                        self.key = Some(key);
                        self.status = Status::Checking;
                        self.next = Some(Instant::now());
                    }
                    None => self.status = Status::NoKey,
                }
                continue;
            }
            match answer {
                Answer::Schema(_, Ok(schema)) => {
                    self.schema = schema;
                    self.schema_asking = false;
                    self.schema_next = None;
                    continue;
                }
                Answer::Schema(_, Err(_)) => {
                    // Asked again after a while, as the screens are; the editor waits meanwhile.
                    self.schema_asking = false;
                    self.schema_next = Some(Instant::now() + POLL_EVERY);
                    continue;
                }
                Answer::Saved(screen, outcome) => {
                    self.saving = None;
                    match &outcome {
                        Ok(saved) => {
                            crate::applog::event("voxlive", &format!("saved the look of {}", saved.label()));
                            if let Some(have) = self.screens.iter_mut().find(|have| have.id == saved.id) {
                                *have = saved.clone();
                            }
                        }
                        // The refusal carries the screen as VoxLive has it now: take it, so its
                        // look shows what is really there and a second save goes over it.
                        Err(Saving::Changed(now)) => {
                            crate::applog::event("voxlive", "a look wasn't saved: the screen changed in VoxLive");
                            if let Some(have) = self.screens.iter_mut().find(|have| have.id == now.id) {
                                *have = (**now).clone();
                            }
                        }
                        Err(Saving::Failed(why)) => crate::applog::problem("voxlive", why),
                    }
                    self.saved = Some((screen, outcome));
                    continue;
                }
                Answer::Screens(..) => self.busy = false,
                // Taken in above.
                Answer::KeyRead(..) => continue,
            }
            match answer {
                Answer::Screens(_, Ok(screens)) => {
                    self.screens = screens;
                    self.found_at = Some(Instant::now());
                    self.status = Status::Connected;
                    self.next = Some(Instant::now() + POLL_EVERY);
                }
                Answer::Screens(_, Err(Problem::Refused(why))) => {
                    crate::applog::problem("voxlive", &why);
                    self.status = Status::Refused(why);
                    self.next = None;
                }
                Answer::Screens(_, Err(Problem::Wait(wait, why))) => {
                    self.status = Status::Trouble(why);
                    self.next = Some(Instant::now() + wait);
                }
                Answer::Screens(_, Err(Problem::Other(why))) => {
                    crate::applog::problem("voxlive", &why);
                    self.status = Status::Trouble(why);
                    self.next = Some(Instant::now() + POLL_EVERY);
                }
                Answer::Schema(..) | Answer::Saved(..) | Answer::KeyRead(..) => {}
            }
        }
        // The look settings, once connected.
        if self.status == Status::Connected
            && !self.schema_asking
            && self.schema_next.is_some_and(|at| at <= Instant::now())
            && let Some(key) = self.key.clone()
        {
            self.schema_asking = true;
            self.schema_next = None;
            let (tx, base, generation) = (self.tx.clone(), api_base(), self.generation);
            std::thread::spawn(move || {
                let _ = tx.send(Answer::Schema(generation, get_schema(&base, &key)));
            });
        }
        let due = self.next.is_some_and(|at| at <= Instant::now());
        if due
            && !self.busy
            && let Some(key) = self.key.clone()
        {
            self.busy = true;
            self.next = None;
            let (tx, base, generation) = (self.tx.clone(), api_base(), self.generation);
            std::thread::spawn(move || {
                let _ = tx.send(Answer::Screens(generation, get_screens(&base, &key)));
            });
        }
        if self.busy || self.saving.is_some() || self.schema_asking || self.key_reading {
            Some(Instant::now() + Duration::from_millis(250))
        } else {
            let schema = self.schema_next.filter(|_| self.status == Status::Connected);
            // With no key, nothing is asked, so nothing is due (a time left over would be past).
            let next = self.next.filter(|_| self.key.is_some());
            [next, schema].into_iter().flatten().min()
        }
    }
}

/// True for something shaped like a VoxLive API key: `vxk_`, then letters, digits, `_` or `-`.
pub fn looks_like_key(key: &str) -> bool {
    key.strip_prefix("vxk_").is_some_and(|rest| {
        rest.len() >= 16 && rest.len() <= 128 && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    })
}

/// A request to VoxLive's API with the key: its status, its answer as JSON, and how long it
/// asked to wait (on a 429 or 503). Redirects are off, so the key never follows one elsewhere.
fn call(
    base: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
    if_match: Option<&str>,
) -> Result<(u16, serde_json::Value, Option<Duration>), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .https_only(base.starts_with("https://"))
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let url = format!("{base}{path}");
    let auth = format!("Bearer {key}");
    let sent = match (method, body) {
        ("PATCH", Some(body)) => {
            let mut request =
                agent.patch(&url).header("Authorization", &auth).header("Content-Type", "application/json");
            if let Some(version) = if_match {
                request = request.header("If-Match", version);
            }
            request.send(body.to_string())
        }
        _ => agent.get(&url).header("Authorization", &auth).header("Accept", "application/json").call(),
    };
    let mut response = sent.map_err(|e| format!("VoxLive couldn't be reached ({e})."))?;
    let status = response.status().as_u16();
    let wait = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.clamp(2, 3600)));
    let body = response.body_mut().with_config().limit(LARGEST_REPLY).read_to_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&body).unwrap_or_default(), wait))
}

/// The church's screens, as `GET /screens` gives them.
fn get_screens(base: &str, key: &str) -> Result<Vec<Screen>, Problem> {
    let (status, reply, wait) = call(base, key, "GET", "/screens", None, None).map_err(Problem::Other)?;
    let error = reply.get("error").and_then(serde_json::Value::as_str).unwrap_or("");
    match status {
        200 => {
            let screens: Vec<Screen> = reply
                .get("screens")
                .and_then(|screens| serde_json::from_value(screens.clone()).ok())
                .ok_or_else(|| Problem::Other("VoxLive's answer didn't list screens.".to_owned()))?;
            Ok(screens.into_iter().map(|screen| screen.cleaned(base)).filter(|screen| !screen.id.is_empty()).collect())
        }
        401 | 403 => Err(Problem::Refused(refused_words(error))),
        429 if error == "too_many_attempts" => Err(Problem::Refused(refused_words(error))),
        429 | 503 => {
            Err(Problem::Wait(wait.unwrap_or(Duration::from_secs(60)), "VoxLive asked to wait a moment.".to_owned()))
        }
        other => Err(Problem::Other(format!("VoxLive answered with a problem ({other})."))),
    }
}

/// Every look setting, as `GET /screen-style` describes them, in the order they are offered.
fn get_schema(base: &str, key: &str) -> Result<Vec<(String, serde_json::Value)>, Problem> {
    let (status, reply, _) = call(base, key, "GET", "/screen-style", None, None).map_err(Problem::Other)?;
    if status != 200 {
        return Err(Problem::Other(format!("VoxLive didn't describe the look settings ({status}).")));
    }
    let fields = reply.get("fields").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
    Ok(ordered(fields))
}

/// Look settings in the order they are offered: the known ones first, then any others by name.
fn ordered(fields: serde_json::Map<String, serde_json::Value>) -> Vec<(String, serde_json::Value)> {
    let mut out: Vec<(String, serde_json::Value)> = SETTING_ORDER
        .iter()
        .filter_map(|name| fields.get(*name).map(|field| ((*name).to_owned(), field.clone())))
        .collect();
    let mut rest: Vec<(String, serde_json::Value)> =
        fields.into_iter().filter(|(name, _)| !SETTING_ORDER.contains(&name.as_str())).collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    out.extend(rest.into_iter().filter(|(name, _)| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')));
    out
}

/// Saves changed look settings with `PATCH /screens/:id` (the style merges field by field).
fn patch_look(
    base: &str,
    key: &str,
    screen: &str,
    version: Option<&str>,
    changes: serde_json::Map<String, serde_json::Value>,
) -> Result<Screen, Saving> {
    let body = serde_json::json!({ "style": changes });
    let (status, reply, _) =
        call(base, key, "PATCH", &format!("/screens/{screen}"), Some(body), version).map_err(Saving::Failed)?;
    let screen_in = |reply: &serde_json::Value| {
        reply
            .get("screen")
            .and_then(|screen| serde_json::from_value::<Screen>(screen.clone()).ok())
            .map(|s| s.cleaned(base))
    };
    let error = reply.get("error").and_then(serde_json::Value::as_str).unwrap_or("");
    match status {
        200 => screen_in(&reply).ok_or_else(|| Saving::Failed("VoxLive's answer didn't have the screen.".to_owned())),
        412 => screen_in(&reply)
            .map(|now| Saving::Changed(Box::new(now)))
            .ok_or_else(|| Saving::Failed("The screen changed in VoxLive since it was opened.".to_owned()))
            .and_then(Err),
        400 => {
            let field = reply.get("field").and_then(serde_json::Value::as_str).unwrap_or("a setting");
            Err(Saving::Failed(format!("VoxLive didn't take {field} ({error}).")))
        }
        401 | 403 => Err(Saving::Failed(refused_words(error))),
        404 => Err(Saving::Failed("That screen is no longer in VoxLive.".to_owned())),
        429 | 503 => Err(Saving::Failed("VoxLive asked to wait a moment; try again.".to_owned())),
        other => Err(Saving::Failed(format!("VoxLive answered with a problem ({other})."))),
    }
}

/// Why VoxLive refused the key, in words.
fn refused_words(error: &str) -> String {
    match error {
        "key_revoked" => "VoxLive says that key was revoked: make a new one in VoxLive, Settings › API keys.",
        "key_expired" => "VoxLive says that key has expired: make a new one in VoxLive, Settings › API keys.",
        "insufficient_scope" => {
            "That key can't read screens: make one in VoxLive with the screens permissions (read and write)."
        }
        "too_many_attempts" => {
            "VoxLive has paused requests from this network after too many wrong keys; it lifts within the hour."
        }
        _ => "VoxLive doesn't know that key: check it in VoxLive, Settings › API keys.",
    }
    .to_owned()
}

/// Where the key is kept.
mod store {
    /// The Keychain item's service.
    #[cfg(target_os = "macos")]
    const SERVICE: &str = "SoftPresenter VoxLive";

    /// A test copy's own key file, in its own folder; `None` for the owner's app.
    fn test_file() -> Option<std::path::PathBuf> {
        std::env::var_os("SOFTPRESENTER_DATA").map(|dir| std::path::PathBuf::from(dir).join("voxlive-test-key"))
    }

    pub fn load() -> Option<String> {
        if let Some(file) = test_file() {
            return std::fs::read_to_string(file).ok().map(|key| key.trim().to_owned()).filter(|key| !key.is_empty());
        }
        #[cfg(target_os = "macos")]
        return security_framework::passwords::get_generic_password(SERVICE, "church API key")
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
        #[cfg(not(target_os = "macos"))]
        None
    }

    pub fn save(key: &str) -> Result<(), String> {
        if let Some(file) = test_file() {
            std::fs::write(&file, key).map_err(|e| format!("The key could not be kept: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        return security_framework::passwords::set_generic_password(SERVICE, "church API key", key.as_bytes())
            .map_err(|e| format!("The key could not be kept in the Keychain: {e}"));
        #[cfg(not(target_os = "macos"))]
        Err("Keeping a VoxLive key needs macOS for now.".to_owned())
    }

    pub fn forget() {
        if let Some(file) = test_file() {
            let _ = std::fs::remove_file(file);
            return;
        }
        #[cfg(target_os = "macos")]
        let _ = security_framework::passwords::delete_generic_password(SERVICE, "church API key");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keys_hint_is_recognised_without_being_made_again() {
        for key in ["vxk_abcdefghijklmnopqrstuvwx", "vxk_short", "", "ключ_абвгдежзийклмн"] {
            assert!(hint_is(Some(key), Some(&hint_of(key))), "{key}");
            assert!(!hint_is(Some(key), Some(&hint_of("vxk_another_key_entirely"))), "{key}");
            assert!(!hint_is(Some(key), None));
            assert!(!hint_is(None, Some(&hint_of(key))));
        }
        assert!(hint_is(None, None));
    }

    #[test]
    fn a_key_is_shaped_like_voxlives_and_nothing_else_is_taken() {
        assert!(looks_like_key("vxk_live_0123456789abcdefABCDEF-_xyz"));
        assert!(looks_like_key("vxk_test_dummydummydummy"));
        for bad in [
            "",
            "vxk_",
            "vxk_short",
            "sk_live_0123456789abcdef",
            "vxk_live_0123456789abc def",
            "vxk_live_ünï0123456789",
        ] {
            assert!(!looks_like_key(bad), "{bad}");
        }
    }

    #[test]
    fn only_voxlives_own_display_addresses_are_kept_from_a_reply() {
        let screen = Screen {
            id: "abc2345".to_owned(),
            name: "Main\u{1b}[31m".to_owned(),
            url: "https://voxlive.app/listen/grace/screen/abc2345".to_owned(),
            demo_url: "https://evil.example/listen/grace/screen/abc2345?demo=1".to_owned(),
            ..Screen::default()
        }
        .cleaned(LIVE_API);
        assert_eq!(screen.name, "Main[31m", "no control characters");
        assert_eq!(screen.url, "https://voxlive.app/listen/grace/screen/abc2345");
        assert!(screen.demo_url.is_empty(), "an address elsewhere is dropped");
        let test = Screen {
            id: "x".to_owned(),
            url: "http://127.0.0.1:9100/listen/t/screen/x".to_owned(),
            ..Screen::default()
        }
        .cleaned("http://127.0.0.1:9100/api/v1");
        assert_eq!(test.url, "http://127.0.0.1:9100/listen/t/screen/x", "the test stand-in's own display");
    }

    #[test]
    fn the_item_has_a_slide_for_each_screen_and_keeps_its_ids() {
        let screens = vec![
            Screen {
                id: "abc2345".to_owned(),
                name: "Spanish".to_owned(),
                language: "es".to_owned(),
                url: "https://voxlive.app/listen/g/screen/abc2345".to_owned(),
                ..Screen::default()
            },
            Screen { id: "nourl00".to_owned(), name: "No display".to_owned(), ..Screen::default() },
        ];
        let first = item(&screens);
        let again = item(&screens);
        assert_eq!(first.id, ITEM_ID);
        assert_eq!(first.groups.len(), 1, "a screen with no display address has no slide");
        assert_eq!(first.groups[0].name, "Spanish · ES");
        assert_eq!(first, again, "the same screens make the same item, ids and all");
        let object = &first.groups[0].slides[0].objects[0];
        assert!(matches!(&object.kind, sp_model::ObjectKind::Web { url } if url.ends_with("/screen/abc2345")));
    }

    #[test]
    fn the_api_is_voxlives_unless_a_stand_in_on_this_computer_is_named() {
        assert_eq!(LIVE_API, "https://voxlive.app/api/v1");
        assert_eq!(display_prefix(LIVE_API), "https://voxlive.app/listen/");
    }

    #[test]
    fn a_look_previews_free_and_sends_only_what_changed() {
        assert_eq!(preview_address("https://voxlive.app/listen/x"), "https://voxlive.app/listen/x?preview=1&host=app");
        assert_eq!(preview_address("https://voxlive.app/s/x?a=1"), "https://voxlive.app/s/x?a=1&preview=1&host=app");
        let screen = Screen {
            id: "abc".into(),
            version: "v3".into(),
            language: "es".into(),
            style: serde_json::json!({"size": 64, "text": "#FFFFFF"}),
            ..Screen::default()
        };
        let mut look = LookEdit::of(&screen);
        assert!(look.changes().is_empty());
        look.style.insert("size".into(), serde_json::json!(72));
        look.style.insert("shadow".into(), serde_json::json!("soft"));
        let changes = look.changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes.get("size"), Some(&serde_json::json!(72)));
        // The look goes into the page as JSON, so a quote in it can't end the script.
        look.style.insert("font".into(), serde_json::json!("x'});alert(1);({'"));
        let script = apply_script(&look.style, "es\"");
        assert!(script.contains(r#""x'});alert(1);({'""#) && script.contains(r#""lang":"es\"""#));
    }

    #[test]
    fn look_settings_come_in_a_steady_order_with_odd_names_left_out() {
        let fields = serde_json::json!({"zeta": {}, "size": {}, "mode": {}, "alpha": {}, "bad name": {}});
        let names: Vec<String> =
            ordered(fields.as_object().cloned().unwrap()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["mode", "size", "alpha", "zeta"]);
    }
}
