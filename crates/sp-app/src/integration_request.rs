//! Request an Integration: the window that the dashed card at the end of Settings ›
//! Integrations opens, where someone names the app, service or device their church uses and
//! says what it should do in SoftPresenter, and sends it to the SoftPresenter project.
//!
//! **What is sent:** what was typed (the name, what it should do, and how the church uses it
//! when that is filled in), the app's version and the system's name (macOS, Windows or Linux),
//! exactly what the line under the fields says. That is one HTTPS POST of JSON to the project's
//! own server (the one the anonymous reports go to, with the app's usual TLS and no redirects),
//! from a thread of its own. Nothing else is sent: not the app's language, no name, no email, no
//! install id, no log, no cookie. Nobody writes back.
//!
//! **What is typed stays** until it is sent (or the app quits: it is never written down),
//! however the window is left (Cancel, ✕, Esc, a failure), and the window opens again as it was. Once a request is sent, the window opens
//! empty.
//!
//! **A test copy sends nowhere** (quiet, with a data folder of its own, or saving a picture):
//! Send says so instead. The one exception is the testing aid in [`target`].
//!
//! The design's measures (the prototype's `.ra` sheet): 460 × 476 in every state, the keys' row
//! keeping its height while it is sending.

use std::sync::mpsc;
use std::time::Duration;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use serde::Serialize;

use crate::i18n::{t, tf};
use crate::quick_help::Hint;
use crate::studio::{CARD, CARE, Flat, PANEL, RADIUS, TEXT, TEXT_DIM, TEXT_FAINT, flat_button, semibold};

/// The server's route, after the reports server's address.
const ROUTE: &str = "/v1/integration-request";
/// A new idea on GitHub (the repository's idea form), with nothing typed in it.
pub const ISSUE_URL: &str = "https://github.com/esraell/SoftPresenter/issues/new?template=idea.yml";
/// The most characters each field takes, as the design's fields do. The server takes more, and
/// at four bytes a character all three together stay well under its 8 KB.
pub const NAME_MOST: usize = 80;
pub const TEXT_MOST: usize = 600;

/// The window's size in every state, as the design's.
pub const SIZE: Vec2 = vec2(460.0, 476.0);
const SIDE: f32 = 18.0;
/// The title's row.
const HEAD: f32 = 58.0;
/// The keys' row.
const KEY: f32 = 30.0;
/// The body, in every state: what is left between the title's row and the keys, with 14 above
/// the keys.
const BODY: f32 = 356.0;
/// Between a field and the label of the next, and under a label.
const GAP: f32 = 10.0;
const UNDER_LABEL: f32 = 5.0;
/// The name's field.
const FIELD: f32 = 32.0;

/// Why a request couldn't be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The server couldn't be reached at all.
    Offline,
    /// It was reached and gave no answer in time (or the send's thread went away).
    NoAnswer,
    /// It answered, and didn't take the request (400, 413, 503 and the like).
    NotTaken,
    /// It answered that too many came from this network just now (429): a minute later it will.
    TooMany,
    /// This is a test copy, which sends nothing.
    TestCopy,
}

/// Where the window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The fields, to fill in.
    Form,
    /// Sent, waiting for the answer.
    Sending,
    /// It couldn't be sent; what was typed is kept.
    Failed(Why),
    /// It was taken.
    Sent,
}

/// Where requests go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    /// A test copy pointed at a server on this computer, which may use plain http.
    pub local: bool,
}

/// How a request goes out: [`post`], or a stand-in in the tests.
type Post = fn(&Target, &[u8]) -> Result<(), Why>;

/// The window, what is typed in it, and the request on its way.
pub struct IntegrationRequest {
    pub shown: bool,
    pub name: String,
    pub what: String,
    pub how: String,
    pub phase: Phase,
    /// The answer of the request on its way.
    waiting: Option<mpsc::Receiver<Result<(), Why>>>,
    /// Send refused (a field needed is empty), under Send for a moment.
    refusal: Option<crate::notices::Cue>,
    /// The name's field takes the keyboard at the next drawing.
    focus: bool,
    /// Where requests go, once asked; `None` inside in a test copy.
    target: Option<Option<Target>>,
    post: Post,
}

impl Default for IntegrationRequest {
    fn default() -> Self {
        IntegrationRequest {
            shown: false,
            name: String::new(),
            what: String::new(),
            how: String::new(),
            phase: Phase::Form,
            waiting: None,
            refusal: None,
            focus: false,
            target: None,
            post,
        }
    }
}

/// The system's name, as the window's line says it and as it is sent.
pub fn system_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    }
}

/// Where this copy sends requests, or nowhere. A test copy never sends: one with
/// `SOFTPRESENTER_QUIET`, with a data folder of its own (`SOFTPRESENTER_DATA`), or saving a
/// picture (`capture`). The one exception is the testing aid below.
fn target(capture: bool, var: impl Fn(&str) -> Option<String>) -> Option<Target> {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    // Testing aid: SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL=http://127.0.0.1:<port> sends a
    // test copy's requests to a server on this computer (the reports Worker run locally).
    // Honoured only together with SOFTPRESENTER_DATA, and only for 127.0.0.1 or localhost, with
    // a port at most and no path; with any other address the copy sends nowhere.
    if let Some(address) = var("SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL").filter(|address| !address.is_empty()) {
        let address = address.trim().trim_end_matches('/').to_owned();
        let local = crate::reports::is_local_test_address(&address);
        return (set("SOFTPRESENTER_DATA") && local).then(|| Target { url: format!("{address}{ROUTE}"), local: true });
    }
    if capture || set("SOFTPRESENTER_QUIET") || set("SOFTPRESENTER_DATA") {
        return None;
    }
    Some(Target { url: format!("{}{ROUTE}", crate::reports::SERVER), local: false })
}

/// Whether this run only saves a picture.
fn capture_run() -> bool {
    std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("SOFTPRESENTER_CAPTURE"))
        || std::env::args().any(|word| word == "--capture-home")
}

/// What is sent: these five fields, as JSON, and nothing else; the line under the fields names
/// each of them (the server would take the app's language too, and is never sent it).
#[derive(Serialize)]
struct Body<'a> {
    name: &'a str,
    what: &'a str,
    how: &'a str,
    version: &'a str,
    system: &'a str,
}

/// The JSON sent for these words, with this copy's version and system.
fn body(name: &str, what: &str, how: &str) -> Vec<u8> {
    let body = Body { name, what, how, version: crate::updates::VERSION, system: system_name() };
    serde_json::to_vec(&body).unwrap_or_default()
}

/// The words as they are sent: the ends trimmed; a carriage return (pasted from Windows) made a
/// new line; the characters the server refuses or nobody types left out (control characters,
/// and those that reorder text on screen), keeping new lines and tabs in the texts; and at most
/// `most` characters, whatever the field let in, so the request stays within the server's
/// limits and its 8 KB.
fn cleaned(text: &str, one_line: bool, most: usize) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let kept: String = text
        .chars()
        .filter(|&c| {
            let reorders = matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
            let allowed_control = !one_line && matches!(c, '\n' | '\t');
            !reorders && (!c.is_control() || allowed_control)
        })
        .collect();
    let cut: String = kept.trim().chars().take(most).collect();
    cut.trim_end().to_owned()
}

/// What the server's answer means: taken (2xx), too many just now (429), or not taken.
fn answer_of(status: u16) -> Result<(), Why> {
    match status {
        200..=299 => Ok(()),
        429 => Err(Why::TooMany),
        _ => Err(Why::NotTaken),
    }
}

/// Sends one request: the JSON and the header saying so, and nothing else (no key, no cookie,
/// no install id). The app's usual TLS; no redirects; connecting may take five seconds and the
/// whole exchange fifteen.
pub fn post(target: &Target, body: &[u8]) -> Result<(), Why> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(15)))
        .https_only(!target.local)
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("SoftPresenter")
        .build()
        .into();
    match agent.post(&target.url).header("Content-Type", "application/json").send(body) {
        // The server's words are never shown, only what its answer means.
        Ok(response) => answer_of(response.status().as_u16()),
        Err(ureq::Error::Timeout(_)) => Err(Why::NoAnswer),
        Err(_) => Err(Why::Offline),
    }
}

impl IntegrationRequest {
    /// Shows the window as it was left (or with the answer that came while it was shut).
    pub fn open(&mut self) {
        self.poll();
        self.shown = true;
        self.focus = matches!(self.phase, Phase::Form | Phase::Failed(_));
    }

    /// Puts the window away, keeping what is typed; once sent, it opens empty next time.
    pub fn close(&mut self) {
        self.shown = false;
        self.refusal = None;
        if self.phase == Phase::Sent {
            self.phase = Phase::Form;
        }
    }

    /// Takes the answer of the request on its way, if it has come.
    pub fn poll(&mut self) {
        let Some(waiting) = &self.waiting else { return };
        let answer = match waiting.try_recv() {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(Why::NoAnswer),
        };
        self.waiting = None;
        self.phase = match answer {
            Ok(()) => {
                self.name.clear();
                self.what.clear();
                self.how.clear();
                Phase::Sent
            }
            Err(why) => Phase::Failed(why),
        };
    }

    /// Where requests go, asked once.
    fn target(&mut self) -> Option<Target> {
        self.target.get_or_insert_with(|| target(capture_run(), |name| std::env::var(name).ok())).clone()
    }

    /// Send (or Try Again): refused while the name or what it should do is empty; in a test
    /// copy, says it sends nothing; otherwise sends from a thread of its own and calls `wake`
    /// when the answer is in (the window also looks for it while it waits).
    pub fn send(&mut self, wake: impl Fn() + Send + 'static) {
        if self.waiting.is_some() || matches!(self.phase, Phase::Sending | Phase::Sent) {
            return;
        }
        let (name, what, how) = (
            cleaned(&self.name, true, NAME_MOST),
            cleaned(&self.what, false, TEXT_MOST),
            cleaned(&self.how, false, TEXT_MOST),
        );
        if name.is_empty() || what.is_empty() {
            self.refusal = Some(crate::notices::Cue::new(refused_words()));
            return;
        }
        self.refusal = None;
        let Some(target) = self.target() else {
            crate::applog::event("request", "Not sent: a test copy sends nothing");
            self.phase = Phase::Failed(Why::TestCopy);
            return;
        };
        let body = body(&name, &what, &how);
        let (tx, rx) = mpsc::channel();
        let post = self.post;
        let started = std::thread::Builder::new().name("integration request".to_owned()).spawn(move || {
            let answer = post(&target, &body);
            match answer {
                Ok(()) => crate::applog::event("request", "Integration request sent"),
                Err(why) => crate::applog::event("request", &format!("Integration request not sent: {why:?}")),
            }
            let _ = tx.send(answer);
            wake();
        });
        match started {
            Ok(_) => {
                self.waiting = Some(rx);
                self.phase = Phase::Sending;
            }
            Err(_) => self.phase = Phase::Failed(Why::NoAnswer),
        }
    }

    /// Testing aid: `SOFTPRESENTER_CAPTURE_PREFERENCES=integration-request` (`-filled`,
    /// `-sending`, `-sent`, `-failed`, `-refused`) opens the window in that state with made-up
    /// words, to see it in a picture. Nothing is sent.
    pub fn demo(&mut self, state: &str) {
        *self = IntegrationRequest::default();
        self.target = Some(None);
        self.shown = true;
        if state.is_empty() {
            return;
        }
        self.name = "Our church planner".to_owned();
        self.what = "Bring this week\u{2019}s plan in as a playlist, with its songs in order.".to_owned();
        self.how = "Our worship leader plans every Sunday there.".to_owned();
        self.phase = match state {
            "sending" => Phase::Sending,
            "sent" => Phase::Sent,
            "failed" => Phase::Failed(Why::Offline),
            _ => Phase::Form,
        };
        if state == "refused" {
            self.what.clear();
            self.refusal = Some(crate::notices::Cue::new(refused_words()));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The window

fn refused_words() -> &'static str {
    t("integrations.ask.refused", "Say which app or device, and what it should do")
}

/// The line under the fields, always shown: what is sent, with this copy's version and system.
fn note_words() -> String {
    tf(
        "integrations.ask.note",
        "Only this is sent: what you typed, SoftPresenter {version} and {system}. No name, no email, no logs, no songs. Nobody writes back: if it\u{2019}s built, What\u{2019}s new says so.",
        &[("version", crate::updates::VERSION), ("system", system_name())],
    )
}

/// The line for what is happening: nothing, Sending…, or why it couldn't be sent, in amber.
fn say_words(phase: Phase) -> Option<(&'static str, Color32)> {
    match phase {
        Phase::Sending => Some((t("integrations.ask.sending", "Sending\u{2026}"), TEXT_DIM)),
        Phase::Failed(Why::Offline) => Some((
            t(
                "integrations.ask.failed.offline",
                "It couldn\u{2019}t be sent: there is no internet connection. What you typed is kept.",
            ),
            CARE,
        )),
        Phase::Failed(Why::NoAnswer) => Some((
            t(
                "integrations.ask.failed.server",
                "It couldn\u{2019}t be sent: the server didn\u{2019}t answer. What you typed is kept.",
            ),
            CARE,
        )),
        Phase::Failed(Why::NotTaken) => Some((
            t(
                "integrations.ask.failed.refused",
                "It couldn\u{2019}t be sent: the server didn\u{2019}t take it. What you typed is kept.",
            ),
            CARE,
        )),
        Phase::Failed(Why::TooMany) => Some((
            t(
                "integrations.ask.failed.busy",
                "It couldn\u{2019}t be sent: the server didn\u{2019}t take it just now. Try again in a minute. What you typed is kept.",
            ),
            CARE,
        )),
        Phase::Failed(Why::TestCopy) => Some((
            t(
                "integrations.ask.failed.test",
                "It wasn\u{2019}t sent: a test copy sends nothing. What you typed is kept.",
            ),
            CARE,
        )),
        Phase::Form | Phase::Sent => None,
    }
}

/// What a key of the window asks for, done once the drawing is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Issue,
    Cancel,
    Send,
    Done,
}

/// What a key of the window's foot does, for Quick Help.
fn foot_hint(asks: Asked) -> &'static str {
    match asks {
        Asked::Issue => {
            t("quickhelp.request.issue", "Open a new issue on GitHub with what you typed, in your browser.")
        }
        Asked::Cancel => t("quickhelp.request.cancel", "Close this window without sending."),
        Asked::Send => t("quickhelp.request.send", "Send the request to the SoftPresenter project."),
        Asked::Done => t("quickhelp.sheet.close", "Close this window."),
    }
}

/// A key of the window's foot: its words, whether it is the white one, what it asks for.
struct FootKey {
    words: &'static str,
    main: bool,
    asks: Asked,
}

/// The keys at the left of the foot, and those at the right, left to right.
fn foot_keys(phase: Phase) -> (Vec<FootKey>, Vec<FootKey>) {
    let issue = FootKey {
        words: t("integrations.ask.issue", "Open an Issue on GitHub \u{2197}"),
        main: false,
        asks: Asked::Issue,
    };
    let cancel = FootKey { words: t("integrations.ask.cancel", "Cancel"), main: false, asks: Asked::Cancel };
    match phase {
        Phase::Form => (
            vec![issue],
            vec![cancel, FootKey { words: t("integrations.ask.send", "Send"), main: true, asks: Asked::Send }],
        ),
        Phase::Failed(_) => (
            vec![issue],
            vec![cancel, FootKey { words: t("integrations.ask.again", "Try Again"), main: true, asks: Asked::Send }],
        ),
        Phase::Sending => (Vec::new(), Vec::new()),
        Phase::Sent => {
            (Vec::new(), vec![FootKey { words: t("integrations.ask.done", "Done"), main: true, asks: Asked::Done }])
        }
    }
}

/// How wide a key of the foot is.
fn key_width(ui: &egui::Ui, words: &str) -> f32 {
    crate::studio::widest(ui, &[words], &FontId::proportional(12.5)) + 26.0
}

/// The window, over the window of `ctx` (Settings), while it is shown.
pub fn window(ctx: &egui::Context, ask: &mut IntegrationRequest) {
    ask.poll();
    if !ask.shown {
        return;
    }
    let screen = ctx.content_rect();
    let sheet = Rect::from_center_size(screen.center(), SIZE);
    let (left_keys, right_keys) = foot_keys(ask.phase);
    let mut asked = None;
    // Esc puts the window away, keeping what is typed. Enter is a new line in the texts, and
    // never sends.
    let mut hide = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    let mut send_key = None;
    let area = egui::Area::new(egui::Id::unique("request an integration"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false);
    area.show(ctx, |ui| {
        ui.set_clip_rect(screen);
        // The window under it takes the pointer, so nothing there is clicked.
        ui.allocate_rect(screen, Sense::click()).no_hint();
        let painter = ui.painter().clone();
        crate::studio::paint_sheet(&painter, screen, sheet);
        let title = t("integrations.ask.window", "Request an Integration");
        painter.text(pos2(sheet.min.x + SIDE, sheet.min.y + 29.0), Align2::LEFT_CENTER, title, semibold(15.0), TEXT);
        let cross = Rect::from_min_size(pos2(sheet.max.x - 42.0, sheet.min.y + 14.0), vec2(30.0, 30.0));
        let cross_response = ui.interact(cross, ui.scope_id().with("close request"), Sense::click());
        if cross_response.hovered() {
            painter.rect_filled(cross, 7.0, CARD);
        }
        crate::icons::paint(
            &painter,
            crate::icons::Icon::Close,
            Rect::from_center_size(cross.center(), vec2(14.0, 14.0)),
            TEXT_DIM,
        );
        if cross_response.hint(t("integrations.ask.close", "Close (Esc)")).clicked() {
            hide = true;
        }
        let inner = Rect::from_min_max(
            pos2(sheet.min.x + SIDE, sheet.min.y + HEAD),
            pos2(sheet.max.x - SIDE, sheet.max.y - SIDE),
        );
        let body = Rect::from_min_size(inner.min, vec2(inner.width(), BODY));
        if ask.phase == Phase::Sent {
            sent_body(ui, body);
        } else {
            form_body(ui, body, ask);
        }
        // The foot: one key at the left, the others at the right; the row kept while sending.
        let top = inner.max.y - KEY;
        let font = FontId::proportional(12.5);
        let mut x = inner.min.x;
        for (index, key) in left_keys.iter().enumerate() {
            let at = Rect::from_min_size(pos2(x, top), vec2(key_width(ui, key.words), KEY));
            let look = if key.main { Flat::Main } else { Flat::Plain };
            let id = ui.scope_id().with(("request key left", index));
            if flat_button(ui, at, id, key.words, font.clone(), look, true).hint(foot_hint(key.asks)).clicked() {
                asked = Some(key.asks);
            }
            x = at.max.x + 8.0;
        }
        let mut right = inner.max.x;
        for (index, key) in right_keys.iter().enumerate().rev() {
            let at =
                Rect::from_min_size(pos2(right - key_width(ui, key.words), top), vec2(key_width(ui, key.words), KEY));
            let look = if key.main { Flat::Main } else { Flat::Plain };
            let id = ui.scope_id().with(("request key right", index));
            if flat_button(ui, at, id, key.words, font.clone(), look, true).hint(foot_hint(key.asks)).clicked() {
                asked = Some(key.asks);
            }
            if key.asks == Asked::Send {
                send_key = Some(at);
            }
            right = at.min.x - 8.0;
        }
    });
    let wake = {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    };
    match asked {
        Some(Asked::Issue) => ctx.open_url(egui::OpenUrl::new_tab(ISSUE_URL)),
        Some(Asked::Cancel | Asked::Done) => ask.close(),
        Some(Asked::Send) => ask.send(wake),
        None if hide => ask.close(),
        None => {}
    }
    // Send refused: said under it for a moment, the usual way.
    let under = send_key.unwrap_or_else(|| Rect::from_center_size(sheet.center_bottom(), Vec2::ZERO));
    crate::notices::draw_cue(ctx, &mut ask.refusal, || under);
    // The Settings window draws only when asked to, and the send's thread can't wake it: while
    // the answer is awaited, it looks again five times a second.
    if ask.waiting.is_some() {
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

/// A field's label, with "Optional" after it in the fainter grey where it is.
fn label(ui: &egui::Ui, words: &str, optional: bool, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    let format = |ink| TextFormat { font_id: FontId::proportional(12.5), color: ink, ..TextFormat::default() };
    job.append(words, 0.0, format(TEXT_DIM));
    if optional {
        job.append(t("integrations.ask.optional", "Optional"), 6.0, format(TEXT_FAINT));
    }
    job.wrap.max_width = width;
    ui.painter().layout_job(job)
}

/// How the form's parts stand in the body: each label's top, the name's field, the two text
/// boxes, the note and the line for what is happening.
struct FormPlan {
    labels: [(f32, std::sync::Arc<egui::Galley>); 3],
    name: Rect,
    what: Rect,
    how: Rect,
    note: (f32, std::sync::Arc<egui::Galley>),
    say: Rect,
}

/// Lays the form out in `body`: the labels, the name's field and the note as tall as they
/// need, the line for what is happening two lines tall at the foot, and the two text boxes
/// sharing what is left (the first a little more).
fn plan_form(ui: &egui::Ui, body: Rect) -> FormPlan {
    let width = body.width();
    let labels = [
        label(ui, t("integrations.ask.name", "App, service or device"), false, width),
        label(ui, t("integrations.ask.what", "What should it do in SoftPresenter?"), false, width),
        label(ui, t("integrations.ask.how", "How does your church use it?"), true, width),
    ];
    let note = ui.painter().layout(note_words(), FontId::proportional(12.0), TEXT_FAINT, width);
    let say_height = 2.0 * ui.fonts_mut(|fonts| fonts.row_height(&FontId::proportional(13.0))) + 4.0;
    let fixed: f32 = labels.iter().map(|label| label.size().y + UNDER_LABEL).sum::<f32>()
        + FIELD
        + 4.0 * GAP
        + note.size().y
        + say_height;
    let boxes = (body.height() - fixed).max(72.0);
    let what_height = (boxes * 0.58).round();
    let how_height = boxes - what_height;
    let mut y = body.min.y;
    let place_label = |y: &mut f32, galley: &std::sync::Arc<egui::Galley>| {
        let top = *y;
        *y += galley.size().y + UNDER_LABEL;
        top
    };
    let name_label = place_label(&mut y, &labels[0]);
    let name = Rect::from_min_size(pos2(body.min.x, y), vec2(width, FIELD));
    y = name.max.y + GAP;
    let what_label = place_label(&mut y, &labels[1]);
    let what = Rect::from_min_size(pos2(body.min.x, y), vec2(width, what_height));
    y = what.max.y + GAP;
    let how_label = place_label(&mut y, &labels[2]);
    let how = Rect::from_min_size(pos2(body.min.x, y), vec2(width, how_height));
    y = how.max.y + GAP;
    let note_top = y;
    y += note.size().y + GAP;
    let say = Rect::from_min_size(pos2(body.min.x, y), vec2(width, say_height));
    let [first, second, third] = labels;
    FormPlan {
        labels: [(name_label, first), (what_label, second), (how_label, third)],
        name,
        what,
        how,
        note: (note_top, note),
        say,
    }
}

/// The fields, the line saying what is sent, and the line for what is happening. While it is
/// sending the fields are greyed and take nothing.
fn form_body(ui: &mut egui::Ui, body: Rect, ask: &mut IntegrationRequest) {
    let plan = plan_form(ui, body);
    let open = ask.phase != Phase::Sending;
    for (top, galley) in &plan.labels {
        ui.painter().galley(pos2(body.min.x, *top), galley.clone(), TEXT_DIM);
    }
    let focus = std::mem::take(&mut ask.focus);
    line_field(ui, plan.name, &mut ask.name, open, focus);
    text_box(
        ui,
        plan.what,
        &mut ask.what,
        t("integrations.ask.what.hint", "For example: bring this week\u{2019}s set list in as a playlist"),
        "request what",
        open,
    );
    text_box(ui, plan.how, &mut ask.how, "", "request how", open);
    let (note_top, note) = plan.note;
    ui.painter().galley(pos2(body.min.x, note_top), note, TEXT_FAINT);
    if let Some((words, ink)) = say_words(ask.phase) {
        let line = ui.painter().layout(words.to_owned(), FontId::proportional(13.0), ink, plan.say.width());
        ui.painter().with_clip_rect(plan.say).galley(plan.say.min, line, ink);
    }
}

/// The ring a field shows while it has the keyboard.
fn ring(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Inside);
}

/// The name's field: one line in the panel's dark, "Its name" faint in it while empty.
fn line_field(ui: &mut egui::Ui, rect: Rect, text: &mut String, open: bool, focus: bool) {
    ui.painter().rect_filled(rect, RADIUS, PANEL);
    let font = FontId::proportional(13.5);
    let inside =
        Rect::from_center_size(rect.center(), vec2(rect.width() - 20.0, ui.fonts_mut(|f| f.row_height(&font))));
    let mut place = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !open {
        place.set_opacity(0.5);
    }
    let edit = egui::TextEdit::singleline(text)
        .id_salt("request name")
        .frame(egui::Frame::NONE)
        .font(font)
        .text_color(TEXT)
        .margin(egui::Margin::ZERO)
        .desired_width(inside.width())
        .char_limit(NAME_MOST)
        .interactive(open)
        .hint_text(egui::RichText::new(t("integrations.ask.name.hint", "Its name")).color(TEXT_FAINT));
    let field = place
        .put(inside, edit)
        .hint(t("quickhelp.request.name", "The name of the app, service or device your church uses."));
    if focus && open {
        field.request_focus();
    }
    if field.has_focus() {
        ring(ui, rect);
    }
}

/// A box for several lines, filling `rect`: Enter starts a new line, and what doesn't fit
/// scrolls inside it.
fn text_box(ui: &mut egui::Ui, rect: Rect, text: &mut String, hint: &str, salt: &str, open: bool) {
    ui.painter().rect_filled(rect, RADIUS, PANEL);
    let inside = rect.shrink2(vec2(10.0, 7.0));
    let mut place = ui.new_child(egui::UiBuilder::new().max_rect(inside));
    place.set_clip_rect(inside.intersect(ui.clip_rect()));
    if !open {
        place.set_opacity(0.5);
    }
    let mut focused = false;
    egui::ScrollArea::vertical()
        .id_salt((salt, "scroll"))
        .auto_shrink([false, false])
        .max_height(inside.height())
        .show(&mut place, |ui| {
            let mut edit = egui::TextEdit::multiline(text)
                .id_salt(salt)
                .frame(egui::Frame::NONE)
                .font(FontId::proportional(13.5))
                .text_color(TEXT)
                .margin(egui::Margin::ZERO)
                .desired_width(inside.width())
                .min_size(inside.size())
                .char_limit(TEXT_MOST)
                .interactive(open);
            if !hint.is_empty() {
                edit = edit.hint_text(egui::RichText::new(hint).color(TEXT_FAINT));
            }
            let what = if salt == "request what" {
                t("quickhelp.request.what", "What SoftPresenter should do with it.")
            } else {
                t("quickhelp.request.how", "Anything else that would help: how your church uses it, a link.")
            };
            focused = ui.add(edit).hint(what).has_focus();
        });
    if focused {
        ring(ui, rect);
    }
}

/// Once it is sent: a round tick, "Sent. Thank you.", what happens to it, and a box saying
/// what happens next.
fn sent_body(ui: &mut egui::Ui, body: Rect) {
    let mut inside =
        ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    inside.set_clip_rect(body);
    inside.spacing_mut().item_spacing.y = 6.0;
    inside.add_space(8.0);
    crate::ndi_download::window::done_tick(&mut inside);
    inside.add_space(6.0);
    inside.add(
        egui::Label::new(
            egui::RichText::new(t("integrations.ask.sent", "Sent. Thank you.")).font(semibold(16.0)).color(TEXT),
        )
        .wrap(),
    );
    inside.add(
        egui::Label::new(
            egui::RichText::new(t(
                "integrations.ask.sent.about",
                "We read every request. If it\u{2019}s built, What\u{2019}s new will say so.",
            ))
            .size(13.0)
            .color(TEXT_DIM),
        )
        .wrap(),
    );
    inside.add_space(8.0);
    let room = inside.available_width() - 24.0;
    let name =
        inside.painter().layout(t("integrations.ask.next", "What happens next").to_owned(), semibold(13.5), TEXT, room);
    let how = inside.painter().layout(
        t(
            "integrations.ask.next.about",
            "Requests are weighed together: the more churches ask for one, the sooner it comes.",
        )
        .to_owned(),
        FontId::proportional(12.5),
        TEXT_DIM,
        room,
    );
    let (place, _) = inside
        .allocate_exact_size(vec2(inside.available_width(), name.size().y + 2.0 + how.size().y + 20.0), Sense::hover());
    inside.painter().rect_filled(place, 9.0, PANEL);
    let name_height = name.size().y;
    inside.painter().galley(pos2(place.min.x + 12.0, place.min.y + 10.0), name, TEXT);
    inside.painter().galley(pos2(place.min.x + 12.0, place.min.y + 10.0 + name_height + 2.0), how, TEXT_DIM);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// The stand-ins for sending. Whether one was called shows in `waiting` (a send's thread
    /// is started only with it) and then in the phase.
    fn taken(_: &Target, _: &[u8]) -> Result<(), Why> {
        Ok(())
    }
    fn offline(_: &Target, _: &[u8]) -> Result<(), Why> {
        Err(Why::Offline)
    }
    fn refused(_: &Target, _: &[u8]) -> Result<(), Why> {
        Err(Why::NotTaken)
    }

    /// A window that would send to `post`, never to the network.
    fn ready(post: Post) -> IntegrationRequest {
        IntegrationRequest {
            target: Some(Some(Target { url: "http://127.0.0.1:9/v1/integration-request".to_owned(), local: true })),
            post,
            ..IntegrationRequest::default()
        }
    }

    /// Waits for the send's thread to answer (it answers at once: the stand-ins don't wait).
    fn settle(ask: &mut IntegrationRequest) {
        for _ in 0..200 {
            ask.poll();
            if ask.waiting.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the send never answered");
    }

    fn key(key: Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// The words the window draws, after `events`; the first frame only measures it, so it is
    /// drawn twice and the events go to the second.
    fn drawn(ask: &mut IntegrationRequest, events: Vec<egui::Event>) -> Vec<String> {
        let ctx = egui::Context::default();
        let input = |events: Vec<egui::Event>| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0))),
            events,
            ..Default::default()
        };
        let mut first = ctx.run_ui(input(Vec::new()), |ui| window(ui.ctx(), ask));
        first.textures_delta.clear();
        let mut output = ctx.run_ui(input(events), |ui| window(ui.ctx(), ask));
        output.textures_delta.clear();
        let mut words = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut words);
        }
        words
    }

    fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }

    fn has(words: &[String], wanted: &str) -> bool {
        words.iter().any(|said| said.contains(wanted))
    }

    fn english<R>(body: impl FnOnce() -> R) -> R {
        crate::i18n::with_language(crate::i18n::Language::English, body)
    }

    #[test]
    fn the_form_has_its_fields_the_line_saying_what_is_sent_and_its_keys() {
        english(|| {
            let mut ask = ready(taken);
            ask.open();
            let words = drawn(&mut ask, Vec::new());
            for wanted in [
                "Request an Integration",
                "App, service or device",
                "Its name",
                "What should it do in SoftPresenter?",
                "For example: bring this week\u{2019}s set list in as a playlist",
                "How does your church use it?",
                "Optional",
                "Only this is sent: what you typed, SoftPresenter ",
                "No name, no email, no logs, no songs.",
                "Open an Issue on GitHub \u{2197}",
                "Cancel",
                "Send",
            ] {
                assert!(has(&words, wanted), "{wanted}: {words:?}");
            }
            let line = format!("SoftPresenter {} and {}.", crate::updates::VERSION, system_name());
            assert!(has(&words, &line), "{line}: {words:?}");
        });
    }

    #[test]
    fn send_needs_the_name_and_what_it_should_do_and_sends_nothing_without_them() {
        english(|| {
            let mut ask = ready(taken);
            ask.open();
            for (name, what) in
                [("", ""), ("Planner", ""), ("", "Bring the plan in"), ("   ", "\n\t "), ("\u{202e}", "x")]
            {
                ask.name = name.to_owned();
                ask.what = what.to_owned();
                ask.send(|| {});
                assert_eq!(ask.phase, Phase::Form, "{name:?} {what:?}");
                assert!(ask.waiting.is_none(), "nothing was sent");
                assert_eq!(ask.refusal.as_ref().map(|cue| cue.text.as_str()), Some(refused_words()));
            }
            // How the church uses it isn't needed.
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.send(|| {});
            assert!(ask.refusal.is_none());
            assert_eq!(ask.phase, Phase::Sending);
            settle(&mut ask);
            assert_eq!(ask.phase, Phase::Sent);
        });
    }

    #[test]
    fn a_refused_send_is_said_under_send_in_the_window() {
        english(|| {
            let mut ask = ready(taken);
            ask.open();
            ask.refusal = Some(crate::notices::Cue::new(refused_words()));
            let words = drawn(&mut ask, Vec::new());
            assert!(has(&words, "Say which app or device, and what it should do"), "{words:?}");
        });
    }

    #[test]
    fn what_is_typed_stays_after_cancel_the_cross_esc_and_a_failure() {
        english(|| {
            let mut ask = ready(offline);
            ask.open();
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.how = "Every Sunday".to_owned();
            // Esc.
            drawn(&mut ask, vec![key(Key::Escape)]);
            assert!(!ask.shown);
            ask.open();
            assert_eq!(
                (ask.name.as_str(), ask.what.as_str(), ask.how.as_str()),
                ("Planner", "Bring the plan in", "Every Sunday")
            );
            // Cancel and the cross put it away the same way.
            ask.close();
            ask.open();
            assert_eq!(ask.what, "Bring the plan in");
            // A failure: words kept, Try Again, and the window opens again as it was left.
            ask.send(|| {});
            settle(&mut ask);
            assert_eq!(ask.phase, Phase::Failed(Why::Offline));
            let words = drawn(&mut ask, Vec::new());
            assert!(has(&words, "there is no internet connection. What you typed is kept."), "{words:?}");
            assert!(has(&words, "Try Again") && has(&words, "Cancel") && has(&words, "Open an Issue on GitHub"));
            ask.close();
            ask.open();
            assert_eq!(ask.phase, Phase::Failed(Why::Offline));
            assert_eq!(
                (ask.name.as_str(), ask.what.as_str(), ask.how.as_str()),
                ("Planner", "Bring the plan in", "Every Sunday")
            );
            // Try Again goes through.
            ask.post = taken;
            ask.send(|| {});
            settle(&mut ask);
            assert_eq!(ask.phase, Phase::Sent);
        });
    }

    #[test]
    fn once_sent_it_says_so_and_opens_empty() {
        english(|| {
            let mut ask = ready(taken);
            ask.open();
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.send(|| {});
            settle(&mut ask);
            let words = drawn(&mut ask, Vec::new());
            for wanted in [
                "Sent. Thank you.",
                "We read every request. If it\u{2019}s built, What\u{2019}s new will say so.",
                "What happens next",
                "Requests are weighed together",
                "Done",
            ] {
                assert!(has(&words, wanted), "{wanted}: {words:?}");
            }
            assert!(!has(&words, "Send") && !has(&words, "Its name"), "no fields once sent: {words:?}");
            ask.close();
            ask.open();
            assert_eq!(ask.phase, Phase::Form);
            assert!(ask.name.is_empty() && ask.what.is_empty() && ask.how.is_empty());
            // Sending again needs new words.
            ask.send(|| {});
            assert_eq!(ask.phase, Phase::Form);
        });
    }

    #[test]
    fn while_sending_the_fields_are_greyed_there_are_no_keys_and_it_cant_go_twice() {
        english(|| {
            let (tx, rx) = mpsc::channel();
            let mut ask = ready(taken);
            ask.open();
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.phase = Phase::Sending;
            ask.waiting = Some(rx);
            let words = drawn(&mut ask, Vec::new());
            assert!(has(&words, "Sending\u{2026}"), "{words:?}");
            for gone in ["Send", "Cancel", "Open an Issue on GitHub", "Try Again"] {
                assert!(!words.iter().any(|said| said == gone), "{gone}: {words:?}");
            }
            ask.send(|| {});
            assert_eq!(ask.phase, Phase::Sending, "a second send while one is on its way");
            ask.poll();
            assert!(ask.waiting.is_some(), "the first is still the one waited for");
            // Shut while sending: the answer that comes is waiting when it opens again.
            drawn(&mut ask, vec![key(Key::Escape)]);
            assert!(!ask.shown);
            tx.send(Ok(())).unwrap();
            ask.open();
            assert_eq!(ask.phase, Phase::Sent);
            assert!(has(&drawn(&mut ask, Vec::new()), "Sent. Thank you."));
        });
    }

    #[test]
    fn while_the_answer_is_awaited_the_window_looks_again_soon() {
        let (_tx, rx) = mpsc::channel();
        let mut ask = ready(taken);
        ask.open();
        ask.phase = Phase::Sending;
        ask.waiting = Some(rx);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| window(ui.ctx(), &mut ask));
        output.textures_delta.clear();
        let delay = output.viewport_output.get(&egui::ViewportId::ROOT).map(|viewport| viewport.repaint_delay);
        assert!(delay.is_some_and(|delay| delay <= Duration::from_millis(200)), "{delay:?}");
    }

    #[test]
    fn a_server_that_refuses_is_said_in_plain_words() {
        english(|| {
            let mut ask = ready(refused);
            ask.open();
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.send(|| {});
            settle(&mut ask);
            assert_eq!(ask.phase, Phase::Failed(Why::NotTaken));
            assert!(has(&drawn(&mut ask, Vec::new()), "the server didn\u{2019}t take it. What you typed is kept."));
            // Too many just now says when to try again; no answer at all says so.
            ask.phase = Phase::Failed(Why::TooMany);
            assert!(has(&drawn(&mut ask, Vec::new()), "Try again in a minute."));
            ask.phase = Phase::Failed(Why::NoAnswer);
            assert!(has(&drawn(&mut ask, Vec::new()), "the server didn\u{2019}t answer. What you typed is kept."));
        });
    }

    #[test]
    fn what_the_servers_answer_means() {
        for taken in [200, 201, 204] {
            assert_eq!(answer_of(taken), Ok(()), "{taken}");
        }
        assert_eq!(answer_of(429), Err(Why::TooMany));
        for refused in [301, 400, 404, 405, 413, 500, 503] {
            assert_eq!(answer_of(refused), Err(Why::NotTaken), "{refused}");
        }
    }

    #[test]
    fn a_test_copy_sends_nothing_and_never_says_sent() {
        english(|| {
            let mut ask = IntegrationRequest { target: Some(None), post: taken, ..IntegrationRequest::default() };
            ask.open();
            ask.name = "Planner".to_owned();
            ask.what = "Bring the plan in".to_owned();
            ask.send(|| {});
            assert_eq!(ask.phase, Phase::Failed(Why::TestCopy));
            assert!(ask.waiting.is_none(), "nothing was sent");
            let words = drawn(&mut ask, Vec::new());
            assert!(has(&words, "a test copy sends nothing"), "{words:?}");
            assert!(!has(&words, "Sent. Thank you."));
            assert_eq!(ask.name, "Planner", "the words are kept");
        });
    }

    #[test]
    fn where_requests_go() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_owned())
        };
        let public = target(false, vars(&[]));
        assert_eq!(
            public,
            Some(Target { url: "https://reports.softpresenter.com/v1/integration-request".to_owned(), local: false })
        );
        // A test copy sends nowhere.
        assert_eq!(target(false, vars(&[("SOFTPRESENTER_QUIET", "1")])), None);
        assert_eq!(target(false, vars(&[("SOFTPRESENTER_DATA", "/tmp/x")])), None);
        assert_eq!(target(true, vars(&[])), None);
        // The testing aid: a local server, with a data folder of its own only.
        assert_eq!(
            target(
                false,
                vars(&[
                    ("SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL", "http://127.0.0.1:8797/"),
                    ("SOFTPRESENTER_DATA", "/tmp/x"),
                ])
            ),
            Some(Target { url: "http://127.0.0.1:8797/v1/integration-request".to_owned(), local: true })
        );
        assert_eq!(
            target(
                false,
                vars(&[
                    ("SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL", "http://localhost"),
                    ("SOFTPRESENTER_DATA", "/tmp/x")
                ])
            ),
            Some(Target { url: "http://localhost/v1/integration-request".to_owned(), local: true })
        );
        assert_eq!(
            target(false, vars(&[("SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL", "http://127.0.0.1:8797")])),
            None
        );
        for elsewhere in [
            "https://reports.softpresenter.com",
            "http://example.org",
            "http://127.0.0.1.example.org:1",
            "http://127.0.0.1:1@example.org",
            "http://localhost@example.org",
            "http://127.0.0.1:80/elsewhere",
            "http://127.0.0.1:",
            "http://127.0.0.1:123456",
        ] {
            let pairs: &'static [(&'static str, &'static str)] = Box::leak(Box::new([
                ("SOFTPRESENTER_INTEGRATION_REQUEST_TEST_URL", elsewhere),
                ("SOFTPRESENTER_DATA", "/tmp/x"),
            ]));
            assert_eq!(target(false, vars(pairs)), None, "{elsewhere}");
        }
    }

    #[test]
    fn the_body_is_what_was_typed_and_the_version_and_system_the_line_names() {
        english(|| {
            let sent: serde_json::Value = serde_json::from_slice(&body("Planner", "Bring it in", "")).unwrap();
            let object = sent.as_object().unwrap();
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            // Nothing the line doesn't name: not the app's language, nor anything about the person.
            assert_eq!(keys, ["how", "name", "system", "version", "what"]);
            assert_eq!(sent["name"], "Planner");
            assert_eq!(sent["what"], "Bring it in");
            assert_eq!(sent["how"], "");
            // What the line under the fields names is what goes.
            let version = sent["version"].as_str().unwrap();
            let system = sent["system"].as_str().unwrap();
            assert!(note_words().contains(&format!("SoftPresenter {version} and {system}.")), "{}", note_words());
            assert!(["macOS", "Windows", "Linux"].contains(&system));
            assert!(crate::reports::check::is_version(version), "the server takes it: {version}");
        });
    }

    #[test]
    fn the_words_are_cleaned_as_the_server_wants_them_and_fit_its_8_kb() {
        assert_eq!(cleaned("  Planner\u{202e} \u{1b}[2J ", true, NAME_MOST), "Planner [2J");
        assert_eq!(cleaned("a\r\nb\rc\td\u{0}e\u{85}f", false, TEXT_MOST), "a\nb\nc\tdef");
        assert_eq!(cleaned("one\ntwo", true, NAME_MOST), "onetwo");
        // At most so many characters, whatever the field let in, counted after the ends are
        // trimmed, with no space left at the cut.
        assert_eq!(cleaned(&"w".repeat(5000), false, TEXT_MOST).chars().count(), TEXT_MOST);
        assert_eq!(cleaned(&format!("  {}", "\u{1f64f}".repeat(200)), true, NAME_MOST).chars().count(), NAME_MOST);
        assert_eq!(cleaned("abc def", true, 4), "abc");
        // Fields far past their limits, of four-byte characters, quotes and new lines: what is
        // sent is still under 8 KB.
        let name = "\u{1f64f}".repeat(NAME_MOST * 10);
        let what = "\u{1f64f}\"\n".repeat(TEXT_MOST * 3);
        let how = "\u{1f64f}".repeat(TEXT_MOST * 10);
        let sent =
            body(&cleaned(&name, true, NAME_MOST), &cleaned(&what, false, TEXT_MOST), &cleaned(&how, false, TEXT_MOST));
        assert!(sent.len() < 8 * 1024, "{} bytes", sent.len());
        // Within the server's limits.
        const { assert!(NAME_MOST <= 100 && TEXT_MOST <= 2000) };
    }

    #[test]
    fn the_github_key_opens_the_idea_form_with_nothing_typed_in_it() {
        assert_eq!(ISSUE_URL, "https://github.com/esraell/SoftPresenter/issues/new?template=idea.yml");
        let mut ask = ready(taken);
        ask.open();
        ask.name = "Secret planner".to_owned();
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0));
        ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            window(ui.ctx(), &mut ask);
        })
        .drop_without_applying_deltas();
        // The key at the foot's left, 20 points in.
        let sheet = Rect::from_center_size(screen.center(), SIZE);
        let at = pos2(sheet.min.x + SIDE + 20.0, sheet.max.y - SIDE - KEY / 2.0);
        let click = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            },
            |ui| window(ui.ctx(), &mut ask),
        )
        .drop_without_applying_deltas();
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(screen), events: vec![click(true), click(false)], ..Default::default() },
            |ui| window(ui.ctx(), &mut ask),
        );
        output.textures_delta.clear();
        let opened = output.platform_output.commands.iter().find_map(|command| match command {
            egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        });
        assert_eq!(opened.as_deref(), Some(ISSUE_URL));
        assert!(ask.shown, "the window stays");
        assert_eq!(ask.name, "Secret planner", "and the words stay");
    }

    #[test]
    fn it_fits_its_size_in_every_language_and_every_state() {
        // The interface is drawn with each system's own font (SF on a Mac, Segoe UI on Windows,
        // DejaVu Sans on Linux: `studio::load_fonts`), and `styled` loads this system's, so each
        // system's run checks its own; the Mac, Linux and Windows checks all run it. Every
        // language and state is looked at before the test fails, so one run lists them all.
        let ctx = crate::studio::tests::styled();
        let mut problems = Vec::new();
        for language in crate::i18n::Language::ALL {
            crate::i18n::with_language(language, || {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    let body = Rect::from_min_size(pos2(SIDE, HEAD), vec2(SIZE.x - 2.0 * SIDE, BODY));
                    let plan = plan_form(ui, body);
                    if plan.say.max.y > body.max.y + 0.5 {
                        problems.push(format!(
                            "{language:?}: the form overflows its body by {}",
                            plan.say.max.y - body.max.y
                        ));
                    }
                    if plan.what.height() < 44.0 || plan.how.height() < 28.0 {
                        problems.push(format!(
                            "{language:?}: boxes too small ({}, {})",
                            plan.what.height(),
                            plan.how.height()
                        ));
                    }
                    // Each failure fits the two lines kept for it.
                    for why in [Why::Offline, Why::NoAnswer, Why::NotTaken, Why::TooMany, Why::TestCopy] {
                        let (words, _) = say_words(Phase::Failed(why)).unwrap();
                        let line =
                            ui.painter().layout(words.to_owned(), FontId::proportional(13.0), TEXT, body.width());
                        if line.size().y > plan.say.height() {
                            problems.push(format!("{language:?} {why:?}: more than two lines: {words}"));
                        }
                    }
                    // The keys of each state fit one row, 8 points at least between the key at
                    // the left and those at the right.
                    for phase in [Phase::Form, Phase::Failed(Why::Offline), Phase::Sent] {
                        let (left, right) = foot_keys(phase);
                        let all: f32 = left.iter().chain(&right).map(|key| key_width(ui, key.words) + 8.0).sum();
                        if all > body.width() + 8.0 {
                            problems
                                .push(format!("{language:?} {phase:?}: the keys need {all} of {}", body.width() + 8.0));
                        }
                    }
                })
                .drop_without_applying_deltas();
            });
        }
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(HEAD + BODY + 14.0 + KEY + SIDE, SIZE.y);
    }

    #[test]
    fn a_real_post_sends_one_json_body_and_reads_the_answer() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let kept = seen.clone();
        let server = std::thread::spawn(move || {
            for answer in [
                "HTTP/1.1 204 No Content\r\n\r\n",
                "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n",
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 0\r\n\r\n",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 4096];
                while let Ok(n) = stream.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_owned())
                            })
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                kept.lock().unwrap().push_str(&String::from_utf8_lossy(&request));
                stream.write_all(answer.as_bytes()).unwrap();
            }
        });
        let local = Target { url: format!("http://127.0.0.1:{port}/v1/integration-request"), local: true };
        let sent = english(|| body("Planner", "Bring it in", "Sundays"));
        assert_eq!(post(&local, &sent), Ok(()));
        assert_eq!(post(&local, &sent), Err(Why::NotTaken));
        assert_eq!(post(&local, &sent), Err(Why::TooMany));
        server.join().unwrap();
        let request = seen.lock().unwrap().clone();
        assert!(request.starts_with("POST /v1/integration-request HTTP/1.1\r\n"), "{request}");
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/json\r\n"), "{request}");
        assert!(!lower.contains("cookie") && !lower.contains("x-sp-"), "{request}");
        assert!(request.contains("{\"name\":\"Planner\",\"what\":\"Bring it in\",\"how\":\"Sundays\""), "{request}");
        // Nothing listens there: no network.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let gone = Target {
            url: format!("http://127.0.0.1:{}/v1/integration-request", closed.local_addr().unwrap().port()),
            local: true,
        };
        drop(closed);
        assert_eq!(post(&gone, &sent), Err(Why::Offline));
        // Anything but a test copy's local server is asked over HTTPS only: plain http is
        // refused before connecting.
        let listening = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listening.set_nonblocking(true).unwrap();
        let plain = Target {
            url: format!("http://127.0.0.1:{}/v1/integration-request", listening.local_addr().unwrap().port()),
            local: false,
        };
        assert!(post(&plain, &sent).is_err());
        assert!(listening.accept().is_err(), "it connected over plain http");
    }
}
