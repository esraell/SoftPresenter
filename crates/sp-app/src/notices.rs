//! Notices: one system for everything the window has to tell the operator (the owner's choice,
//! 22 November 2026: "it saves on real estate and has all messages in one place").
//!
//! A status button in the top bar opens the list (`studio::notices_button`). Three kinds of
//! entry go in it:
//! - **trouble**, counted in amber: the app stopped answering, an output stopped, a file that
//!   couldn't be read or written;
//! - **questions**, counted in amber with "?" until answered (today a song or theme changed
//!   outside SoftPresenter, with Reload and Keep mine); they never open the list by themselves;
//! - **information** about what happened out of sight (added to another playlist, a file read
//!   or written, a Bible installed), with a quiet dot.
//!
//! A **refusal** (why nothing happened: "Press Enter to put the picked verse on screen") is not an entry. It is a cue
//! under the place of action for a moment, and never kept. A confirmation of what the
//! operator just did and can see is never said at all.
//!
//! The list is plain data ([`Notice`]: kind, text, time, seen, actions), so it can be sent as it
//! is to a web page. Every entry and every refusal goes in the log as well.

use crate::i18n::t;
use crate::outside::Waiting;
use crate::quick_help::Hint;
use crate::studio::{CARD, CARE, LINE, TEXT, TEXT_DIM, TEXT_FAINT};
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use serde::Serialize;
use std::time::{Duration, Instant};

/// How many entries the list keeps, besides questions waiting for an answer (which stay until
/// answered).
pub const KEPT: usize = 40;

/// How long a refusal's cue stays, in seconds, and how long it takes to come and to go.
pub const CUE_FOR: f64 = 2.4;
pub const CUE_FADE: f64 = 0.12;

/// How soon after a click a refusal counts as that click's, in seconds: its cue then goes under
/// what was clicked, else under the preview's readout (a key was pressed).
pub const CLICK_NEAR: f64 = 0.6;

/// The answers a question about a file changed outside SoftPresenter offers.
pub const RELOAD: &str = "outside:reload";
pub const KEEP_MINE: &str = "outside:keep";

/// The answers a question about a device that wants to show its screen offers.
pub const ALLOW: &str = "mirror:allow";
pub const DONT_ALLOW: &str = "mirror:deny";

/// The answers the one-time notice that anonymous reports are on offers.
pub const KEEP_REPORTS: &str = "reports:keep";
pub const REPORTS_OFF: &str = "reports:off";

/// What kind of entry a notice is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Something that happened out of sight: a quiet dot on the button.
    Info,
    /// Something went wrong: counted in amber on the button.
    Trouble,
    /// Something waits for an answer: amber, with "?", until answered. Sent as "ask", the
    /// design's word for it.
    #[serde(rename = "ask")]
    Question,
}

/// A button on an entry, which answers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Action {
    /// What the answer is called where it is carried out ([`RELOAD`], [`KEEP_MINE`]).
    pub id: String,
    /// The words on the button.
    pub label: String,
}

/// One entry in the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// Its own number, never used again in a run: what an answer names it by.
    pub id: u64,
    pub kind: Kind,
    /// The whole text, never cut short.
    pub text: String,
    /// When it was said, in seconds since 1970 (UTC).
    pub time: i64,
    /// Seen once the list it was in has been closed. A question is never seen: it waits.
    pub seen: bool,
    /// The buttons that answer it (a question's).
    pub actions: Vec<Action>,
    /// What a question is about: the answer goes there.
    #[serde(skip)]
    pub about: Option<Waiting>,
    /// Something about security (a new certificate key, wrong passwords coming again and
    /// again): a remote page can't mark it seen or clear it; only the operator, in the app.
    #[serde(skip)]
    pub secure: bool,
}

/// A refusal's cue: a small dark chip under the place of action, for [`CUE_FOR`] seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub text: String,
    /// When it was first drawn (on the window's clock), once it has been.
    pub since: Option<f64>,
    /// What it goes under, chosen when it is first drawn.
    pub under: Option<Rect>,
}

impl Cue {
    /// A refusal said now, for [`draw_cue`]; noted in the log, as [`Notices::refuse`] notes one.
    pub fn new(text: &str) -> Cue {
        crate::applog::event("refused", text);
        Cue { text: text.to_owned(), since: None, under: None }
    }
}

/// How the status button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Nothing new: a quiet list icon.
    Quiet,
    /// New information: a small grey dot.
    New,
    /// Trouble not yet seen, or questions with it: amber, with how many.
    Trouble(usize),
    /// Only questions: amber, with "?".
    Question,
}

/// The list of notices, newest first, and the cue of the last refusal.
#[derive(Debug, Default)]
pub struct Notices {
    list: Vec<Notice>,
    next: u64,
    /// The list was open when last drawn: closing it marks everything seen.
    pub was_open: bool,
    /// The last refusal's cue, while it shows.
    pub cue: Option<Cue>,
    /// What the pointer last clicked (where it is, and when, on the window's clock), and where
    /// the preview's readout is this frame: where a refusal's cue goes.
    pub clicked: Option<(Rect, f64)>,
    pub readout: Option<Rect>,
    /// How many things have been said (entries and refusals alike), and the last of them, so the
    /// command line (and the setup's page) can say what came of something it asked for.
    said: u64,
    last_said: Option<String>,
}

impl Notices {
    /// Something that happened out of sight: in the list, with a quiet dot.
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Kind::Info, text.into(), None, Vec::new());
    }

    /// Something that went wrong: in the list, counted in amber.
    pub fn trouble(&mut self, text: impl Into<String>) {
        self.push(Kind::Trouble, text.into(), None, Vec::new());
    }

    /// Trouble that lasts, said again each time it is tried (a screen's window that can't be
    /// opened is tried every frame): one entry, raised once; seen, it stays seen while it is in
    /// the list, so closing the list quiets the button.
    pub fn lasting_trouble(&mut self, text: impl Into<String>) {
        let text = crate::applog::plain_paths(&text.into()).into_owned();
        if !self.list.iter().any(|notice| notice.kind == Kind::Trouble && notice.text == text.trim()) {
            self.trouble(text);
        }
    }

    /// A lasting trouble that is over: its entry (the one with these words) leaves the list.
    pub fn withdraw(&mut self, text: &str) {
        let text = crate::applog::plain_paths(text).into_owned();
        self.list.retain(|notice| !(notice.kind == Kind::Trouble && notice.text == text.trim()));
    }

    /// Something about security, in the list like any other, but kept from a remote page's
    /// `notices seen` and `notices clear` until the operator has seen it in the app.
    pub fn secure(&mut self, kind: Kind, text: impl Into<String>) {
        let text = crate::applog::plain_paths(&text.into()).into_owned();
        self.push(kind, text.clone(), None, Vec::new());
        if let Some(notice) = self.list.iter_mut().find(|notice| notice.kind == kind && notice.text == text.trim()) {
            notice.secure = true;
        }
    }

    /// How a piece of work went: information when it went well, trouble when it didn't.
    pub fn outcome(&mut self, result: Result<String, String>) {
        match result {
            Ok(said) => self.info(said),
            Err(problem) => self.trouble(problem),
        }
    }

    /// Asks whether to take a file's version of something changed outside SoftPresenter
    /// (Reload) or to write SoftPresenter's over it (Keep mine). It waits at the head of the
    /// list, and the button stays amber, until answered; it never opens the list. Asked once
    /// however often it comes up.
    pub fn ask(&mut self, about: Waiting, text: impl Into<String>) {
        if self.asks_about(about) {
            return;
        }
        let actions = vec![
            Action { id: RELOAD.to_owned(), label: t("notices.ask.reload", "Reload").to_owned() },
            Action { id: KEEP_MINE.to_owned(), label: t("notices.ask.keep_mine", "Keep mine").to_owned() },
        ];
        self.push(Kind::Question, text.into(), Some(about), actions);
    }

    /// Asks whether a device may show its screen (Allow, Don't Allow). It waits at the head of
    /// the list, and the button stays amber, until answered or until the device gives up; it is
    /// in the operator's window only, never on a screen.
    pub fn ask_to_show(&mut self, device: u64, text: impl Into<String>) {
        let about = Waiting::Mirror(device);
        if self.asks_about(about) {
            return;
        }
        let actions = vec![
            Action { id: ALLOW.to_owned(), label: t("notices.ask.allow", "Allow").to_owned() },
            Action { id: DONT_ALLOW.to_owned(), label: t("notices.ask.dont_allow", "Don\u{2019}t Allow").to_owned() },
        ];
        self.push(Kind::Question, text.into(), Some(about), actions);
    }

    /// Says, once, that anonymous reports are on, with Turn off beside it (and Keep on). It
    /// waits at the head of the list until answered.
    pub fn ask_reports(&mut self, text: impl Into<String>) {
        let about = Waiting::Reports;
        if self.asks_about(about) {
            return;
        }
        let actions = vec![
            Action { id: REPORTS_OFF.to_owned(), label: t("reports.turn_off", "Turn off").to_owned() },
            Action { id: KEEP_REPORTS.to_owned(), label: t("reports.keep_on", "Keep on").to_owned() },
        ];
        self.push(Kind::Question, text.into(), Some(about), actions);
    }

    /// Whether a question about this waits for an answer.
    pub fn asks_about(&self, about: Waiting) -> bool {
        self.list.iter().any(|notice| notice.about == Some(about))
    }

    /// The question about this goes, with nothing said (it was settled some other way, or its
    /// answer is said by whoever carried it out).
    pub fn settle(&mut self, about: Waiting) {
        self.list.retain(|notice| notice.about != Some(about));
    }

    /// What the questions are about, after the things they name have moved (a theme before
    /// them deleted, say).
    pub fn renumber(&mut self, moved: impl Fn(Waiting) -> Waiting) {
        for notice in &mut self.list {
            notice.about = notice.about.map(&moved);
        }
    }

    /// What an answer to an entry asks for: the thing it is about, and true for Reload (or
    /// Allow). `None` when there is no such question or answer.
    pub fn answer(&self, id: u64, action: &str) -> Option<(Waiting, bool)> {
        let notice = self.list.iter().find(|notice| notice.id == id)?;
        let about = notice.about?;
        notice
            .actions
            .iter()
            .any(|offered| offered.id == action)
            .then_some((about, action == RELOAD || action == ALLOW || action == KEEP_REPORTS))
    }

    /// Why nothing happened ("Look a passage up first."), said at the place of action for a
    /// moment and never kept: under what was clicked, or under the preview's readout when it was
    /// a key. Better still is a control greyed so nothing is refused.
    pub fn refuse(&mut self, text: impl Into<String>) {
        let text = text.into();
        let text = crate::applog::plain_paths(&text).into_owned();
        crate::applog::event("refused", &text);
        self.said += 1;
        self.last_said = Some(text.clone());
        self.cue = Some(Cue { text, since: None, under: None });
    }

    /// The words of the refusal showing now, if one is.
    #[cfg(test)]
    pub fn refused(&self) -> Option<&str> {
        self.cue.as_ref().map(|cue| cue.text.as_str())
    }

    /// The refusal showing now, let go of (for tests).
    #[cfg(test)]
    pub fn take_refused(&mut self) -> Option<String> {
        self.cue.take().map(|cue| cue.text)
    }

    /// The newest entry's words, not a question's.
    #[cfg(test)]
    pub fn newest(&self) -> Option<&str> {
        self.list.iter().find(|notice| notice.kind != Kind::Question).map(|notice| notice.text.as_str())
    }

    /// The newest entry, not a question, taken out of the list (for tests).
    #[cfg(test)]
    pub fn take_newest(&mut self) -> Option<String> {
        let at = self.list.iter().position(|notice| notice.kind != Kind::Question)?;
        Some(self.list.remove(at).text)
    }

    /// A mark to ask [`Notices::said_since`] about later.
    pub fn mark(&self) -> u64 {
        self.said
    }

    /// The last thing said (an entry or a refusal) since `mark`, if anything was.
    pub fn said_since(&self, mark: u64) -> Option<&str> {
        (self.said > mark).then_some(self.last_said.as_deref()).flatten()
    }

    /// Clear All: the list empties, except for questions, which stay until answered.
    pub fn clear_all(&mut self) {
        self.list.retain(|notice| notice.kind == Kind::Question);
    }

    /// What a remote page's `notices clear` does: as Clear All, but what is about security stays
    /// until the operator has seen it in the app.
    pub fn clear_all_for_page(&mut self) {
        self.list.retain(|notice| notice.kind == Kind::Question || (notice.secure && !notice.seen));
    }

    /// What a remote page's `notices seen` does: as closing the list, but what is about security
    /// is left unseen.
    pub fn mark_seen_for_page(&mut self) {
        for notice in &mut self.list {
            notice.seen = notice.seen || (notice.kind != Kind::Question && !notice.secure);
        }
    }

    /// Whether Clear All has anything to take away.
    pub fn clearable(&self) -> bool {
        self.list.iter().any(|notice| notice.kind != Kind::Question)
    }

    /// Everything in the list has been seen (it was closed).
    pub fn mark_seen(&mut self) {
        for notice in &mut self.list {
            notice.seen = notice.kind != Kind::Question;
        }
    }

    /// The entries as the list shows them: questions first, then the rest, newest first.
    pub fn entries(&self) -> impl Iterator<Item = &Notice> {
        let questions = self.list.iter().filter(|notice| notice.kind == Kind::Question);
        questions.chain(self.list.iter().filter(|notice| notice.kind != Kind::Question))
    }

    /// The whole list as it is shown, to send as it is (to a web page, say).
    #[cfg_attr(not(test), allow(dead_code))] // For the web page, which takes the list as it is.
    pub fn to_vec(&self) -> Vec<Notice> {
        self.entries().cloned().collect()
    }

    /// How the status button looks now.
    pub fn look(&self) -> Look {
        let unseen = || self.list.iter().filter(|notice| !notice.seen);
        let questions = self.list.iter().filter(|notice| notice.kind == Kind::Question).count();
        let trouble = unseen().filter(|notice| notice.kind == Kind::Trouble).count();
        match (questions, trouble) {
            (0, 0) if unseen().next().is_some() => Look::New,
            (0, 0) => Look::Quiet,
            (_, 0) => Look::Question,
            (questions, trouble) => Look::Trouble(questions + trouble),
        }
    }

    /// Puts an entry at the head of the list. The same words as an entry not yet seen add
    /// nothing (so something said again and again, as a file that can't be opened may be, is one
    /// entry and one line in the log); seen, they come back as new, at the head. Past [`KEPT`],
    /// the oldest go (never a question).
    fn push(&mut self, kind: Kind, text: String, about: Option<Waiting>, actions: Vec<Action>) {
        let text = crate::applog::plain_paths(text.trim()).into_owned();
        if text.is_empty() {
            return;
        }
        let time = chrono::Utc::now().timestamp();
        self.said += 1;
        self.last_said = Some(text.clone());
        if kind != Kind::Question
            && let Some(at) = self.list.iter().position(|notice| notice.kind == kind && notice.text == text)
        {
            if !self.list[at].seen {
                return;
            }
            self.list.remove(at);
        }
        crate::applog::event(if kind == Kind::Question { "question" } else { "notice" }, &text);
        crate::reports::stats::notice(kind == Kind::Trouble);
        self.next += 1;
        self.list.insert(0, Notice { id: self.next, kind, text, time, seen: false, actions, about, secure: false });
        let mut kept = 0;
        self.list.retain(|notice| {
            if notice.kind == Kind::Question {
                return true;
            }
            kept += 1;
            kept <= KEPT
        });
    }
}

/// How long a service the app keeps asking (VoxLive's screens, TimelyChurch's services) may stay
/// out of reach before the notices say so: a network that drops for a moment (a Wi-Fi change, a
/// move between networks) is nothing to tell the operator about.
pub const OUTAGE_GRACE: Duration = Duration::from_secs(120);

/// A gap between two failed answers longer than this (and than twice the time between asks)
/// starts an outage's clock again from the later one: the computer slept between them, and the
/// one before says nothing of how long the service has been out of reach.
pub const OUTAGE_GAP: Duration = Duration::from_secs(5 * 60);

/// What was last heard from a service the app keeps asking, for [`Outage::follow`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    /// It couldn't be reached, or answered with a problem: its last failed answer came then.
    Failed(Instant),
    /// It answered, or refused what it was given, or it isn't asked any more (disconnected): an
    /// outage is over.
    Ended,
    /// Nothing to go by yet (a new key or code being tried): an outage stands as it is.
    Nothing,
}

/// What the notices say of a service the app keeps asking while it can't be reached. Its card in
/// Settings says so at once; the notices say it once an outage (from its first failed answer
/// until it answers or refuses, or is disconnected): when a failed answer comes
/// [`OUTAGE_GRACE`] or more after the first, or at once while what the service does is on screen.
/// When the outage ends, the entry goes.
#[derive(Debug)]
pub struct Outage {
    /// When this outage's clock started: its first failed answer, or the first after a gap.
    since: Option<Instant>,
    /// When its last failed answer came.
    last: Option<Instant>,
    /// A longer gap between failed answers starts the clock again.
    gap: Duration,
    /// The entry said for this outage, once it has been: its words, to take it away by.
    said: Option<String>,
}

impl Outage {
    /// For a service asked every `every` while it fails.
    pub fn asked_every(every: Duration) -> Outage {
        Outage { since: None, last: None, gap: OUTAGE_GAP.max(every * 2), said: None }
    }

    /// Follows the service, each time the app wakes: `heard` is what was last heard from it;
    /// `in_use` is true while what it does is on screen; `words` are the entry's. True when the
    /// list changed.
    pub fn follow(
        &mut self,
        heard: Heard,
        in_use: bool,
        words: impl FnOnce() -> String,
        notices: &mut Notices,
    ) -> bool {
        let at = match heard {
            Heard::Failed(at) => at,
            Heard::Nothing => return false,
            Heard::Ended => {
                self.since = None;
                self.last = None;
                let said = self.said.take();
                if let Some(said) = &said {
                    notices.withdraw(said);
                }
                return said.is_some();
            }
        };
        let slept = self.last.is_some_and(|last| at.saturating_duration_since(last) > self.gap);
        if self.since.is_none() || slept {
            self.since = Some(at);
        }
        self.last = Some(at);
        let since = self.since.unwrap_or(at);
        if self.said.is_some() || !(in_use || at.saturating_duration_since(since) >= OUTAGE_GRACE) {
            return false;
        }
        let words = words();
        notices.trouble(words.clone());
        self.said = Some(words);
        true
    }
}

/// What a service's card and the notices say while the app can't reach it and keeps asking. Why
/// (a lookup that failed, a timeout) goes in the log only.
pub fn unreachable(service: &str) -> String {
    crate::i18n::tf(
        "notices.unreachable",
        "{service} can't be reached. SoftPresenter keeps trying. Check this computer's internet connection.",
        &[("service", service)],
    )
}

/// What the window says when something the operator asked of a service (saving, connecting)
/// couldn't reach it. Why goes in the log only.
pub fn unreachable_try_again(service: &str) -> String {
    crate::i18n::tf(
        "notices.unreachable_try_again",
        "{service} can't be reached. Check this computer's internet connection and try again.",
        &[("service", service)],
    )
}

/// When a notice was said, for the list: hours and minutes on the window's clock (12-hour or
/// 24-hour as the computer is set).
pub fn time_of_day(time: i64, form: &crate::clock::TimeForm) -> String {
    use chrono::{TimeZone, Timelike};
    let Some(local) = chrono::Local.timestamp_opt(time, 0).single() else { return String::new() };
    form.hours_minutes(local.hour(), local.minute())
}

/// Why a file couldn't be written, for the end of a notice: the usual reasons in plain words in
/// the language set (a full disk, a place SoftPresenter may not write in, a disk that can only be
/// read, a file another program has open), and any other as the system gives it, without the
/// error's number ("(os error 32)"), which the log keeps.
pub fn write_problem(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    // Windows' "The process cannot access the file because it is being used by another process"
    // (32) and "another process has locked a portion of the file" (33).
    let in_use = cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33));
    match error.kind() {
        _ if in_use => t("notices.reason.in_use", "another program has the file open").to_owned(),
        ErrorKind::ResourceBusy => t("notices.reason.in_use", "another program has the file open").to_owned(),
        ErrorKind::StorageFull => t("notices.reason.disk_full", "the disk is full").to_owned(),
        ErrorKind::PermissionDenied => {
            t("notices.reason.not_allowed", "SoftPresenter isn't allowed to write there").to_owned()
        }
        ErrorKind::ReadOnlyFilesystem => t("notices.reason.read_only", "the disk can only be read").to_owned(),
        _ => without_error_number(&error.to_string()).to_owned(),
    }
}

/// A system's error sentence without the number Rust adds at its end: "Access is denied." for
/// "Access is denied. (os error 5)".
fn without_error_number(said: &str) -> &str {
    match said.rfind(" (os error ") {
        Some(at) if said.ends_with(')') => said[..at].trim_end(),
        _ => said,
    }
}

/// The status button's size, in points: it never changes size or place.
pub const BUTTON: egui::Vec2 = egui::vec2(52.0, 30.0);

/// How wide the list is, and how tall it grows before it scrolls.
const LIST_WIDE: f32 = 380.0;
const LIST_HIGH: f32 = 460.0;
/// The list's own grey, a step above the columns.
const LIST_FILL: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
/// The line between two entries.
const ROW_LINE: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
/// The amber mixed 20% into a card's grey (the button with trouble), a little more under the
/// pointer; and 9% into the list's grey (a question's row).
const AMBER_WELL: Color32 = Color32::from_rgb(0x4f, 0x43, 0x1c);
const AMBER_WELL_HOVER: Color32 = Color32::from_rgb(0x5a, 0x4c, 0x1d);
const AMBER_ROW: Color32 = Color32::from_rgb(0x33, 0x2e, 0x1c);
/// The dark words on amber, and the grey of an information mark.
const ON_AMBER: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
const INFO_MARK: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x56);

/// The status button's id.
pub fn button_id() -> egui::Id {
    egui::Id::unique("notices button")
}

/// The list's id: it is a pop-up kept open in egui's memory, so Escape closes it (and only it)
/// as it closes any pop-up, before it can clear the screens.
pub fn list_id() -> egui::Id {
    button_id().with("list")
}

/// Whether the list is open.
pub fn list_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, list_id())
}

/// The status button in the top bar's slot, `rect` ([`BUTTON`] in size), and the list it
/// opens. Drawn with the painter only, it takes no room in any layout, and nothing on it moves
/// or asks for the window to be drawn again. Closing the list (the button again, a click
/// elsewhere, or Escape) marks everything seen.
pub fn button(ui: &mut egui::Ui, state: &mut crate::home::HomeState, rect: Rect, form: &crate::clock::TimeForm) {
    let response = ui.interact(rect, button_id(), Sense::click());
    // Escape closes the list, and it is taken here, so nothing drawn after (the playlist
    // chooser, the Reflow editor) closes on the same press.
    if list_open(ui.ctx()) && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        egui::Popup::close_id(ui.ctx(), list_id());
    }
    let open = list_open(ui.ctx());
    let look = state.notices.look();
    let lit = open || response.hovered();
    let (fill, ink) = match look {
        Look::Trouble(_) | Look::Question => (if lit { AMBER_WELL_HOVER } else { AMBER_WELL }, CARE),
        _ if lit => (CARD, TEXT),
        _ => (Color32::TRANSPARENT, TEXT_FAINT),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, rect.height() / 2.0, fill);
    // The icon and what goes with it (a dot, or a badge), side by side in the middle.
    let badge = match look {
        Look::Trouble(count) => Some(count.min(99).to_string()),
        Look::Question => Some("?".to_owned()),
        _ => None,
    };
    let badge = badge.map(|words| painter.layout_no_wrap(words, crate::studio::fixed_semibold(11.5), ON_AMBER));
    let beside = match (&badge, look) {
        (Some(galley), _) => (galley.size().x + 8.0).max(18.0),
        (None, Look::New) => 7.0,
        _ => 0.0,
    };
    let icon = 15.0;
    let wide = icon + if beside > 0.0 { 5.0 + beside } else { 0.0 };
    let left = rect.center().x - wide / 2.0;
    let icon_rect = Rect::from_center_size(pos2(left + icon / 2.0, rect.center().y), egui::Vec2::splat(icon));
    crate::icons::paint(painter, crate::icons::Icon::Notices, icon_rect, ink);
    let beside_at = pos2(icon_rect.max.x + 5.0, rect.center().y);
    if let Some(galley) = badge {
        let pill = Rect::from_min_size(pos2(beside_at.x, beside_at.y - 9.0), vec2(beside, 18.0));
        painter.rect_filled(pill, 9.0, CARE);
        // The figure's own ink in the middle of the pill, not its line's box: a line keeps room
        // below for letters that hang, which figures don't have, so a figure centred by its box
        // sat high in the pill (on a Windows PC at 175% plainly so).
        let ink = galley.mesh_bounds;
        let at = if ink.is_positive() {
            pill.center() - ink.center().to_vec2()
        } else {
            pill.center() - galley.size() / 2.0
        };
        painter.galley(at, galley, ON_AMBER);
    } else if look == Look::New {
        painter.circle_filled(pos2(beside_at.x + 3.5, beside_at.y), 3.5, TEXT_DIM);
    }
    let hint = match look {
        Look::Question => t("notices.button.question", "A question is waiting: click to answer it"),
        Look::Trouble(_) => t("notices.button.trouble", "Something went wrong: click to see"),
        Look::New => t("notices.button.new", "New notices"),
        Look::Quiet => t("notices.title", "Notices"),
    };
    let response = if open { response } else { response.hint(hint) };
    let frame = egui::Frame::new()
        .fill(LIST_FILL)
        .corner_radius(14.0)
        .stroke(Stroke::new(1.0, LINE))
        .shadow(egui::Shadow { offset: [0, 24], blur: 60, spread: 0, color: Color32::from_black_alpha(150) });
    egui::Popup::from_toggle_button_response(&response)
        .id(list_id())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(egui::RectAlign::BOTTOM)
        .gap(8.0)
        .width(LIST_WIDE)
        .frame(frame)
        .show(|ui| list(ui, state, form));
    let open = list_open(ui.ctx());
    if state.notices.was_open && !open {
        state.notices.mark_seen();
    }
    state.notices.was_open = open;
}

/// The list: "Notices" and Clear All, then every entry, questions first, then newest first,
/// each with its mark, its whole text, its time and its buttons.
fn list(ui: &mut egui::Ui, state: &mut crate::home::HomeState, form: &crate::clock::TimeForm) {
    ui.set_width(LIST_WIDE);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let (head, _) = ui.allocate_exact_size(vec2(LIST_WIDE, 46.0), Sense::hover());
    ui.painter().text(
        pos2(head.min.x + 14.0, head.center().y),
        Align2::LEFT_CENTER,
        t("notices.title", "Notices"),
        crate::studio::semibold(14.0),
        TEXT,
    );
    let font = FontId::proportional(12.0);
    let clear_all = t("notices.clear_all", "Clear All");
    let wide = crate::studio::flat_button_width(ui, &[clear_all], &font, 10.0);
    let clear = Rect::from_min_size(pos2(head.max.x - 14.0 - wide, head.center().y - 12.0), vec2(wide, 24.0));
    let clearable = state.notices.clearable();
    let flat = crate::studio::Flat::Plain;
    if crate::studio::flat_button(ui, clear, list_id().with("clear"), clear_all, font, flat, clearable)
        .hint(t("notices.clear_all.hint", "Empty the list (a question stays until it is answered)"))
        .clicked()
    {
        state.notices.clear_all();
    }
    ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, LINE));
    let entries: Vec<Notice> = state.notices.entries().cloned().collect();
    let mut answered = None;
    egui::ScrollArea::vertical().max_height(LIST_HIGH - head.height()).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if entries.is_empty() {
            let words = t(
                "notices.empty",
                "Nothing to tell you. Trouble, questions, and what happens out of sight (added to another playlist, a file read or written) come here.",
            );
            let galley =
                ui.painter().layout(words.to_owned(), FontId::proportional(12.5), TEXT_FAINT, LIST_WIDE - 28.0);
            let (room, _) = ui.allocate_exact_size(vec2(LIST_WIDE, galley.size().y + 28.0), Sense::hover());
            ui.painter().galley(room.min + vec2(14.0, 14.0), galley, TEXT_FAINT);
        }
        for (place, notice) in entries.iter().enumerate() {
            if let Some(action) = row(ui, notice, place > 0, form) {
                answered = Some((notice.id, action));
            }
        }
        ui.add_space(4.0);
    });
    if let Some((id, action)) = answered
        && let Some(choice) = state.notices.answer(id, &action)
    {
        state.waiting_choice = Some(choice);
    }
}

/// One entry of the list. Returns the answer clicked, if one was.
fn row(ui: &mut egui::Ui, notice: &Notice, line_above: bool, form: &crate::clock::TimeForm) -> Option<String> {
    const SIDE: f32 = 14.0;
    const MARK: f32 = 16.0;
    const BETWEEN: f32 = 10.0;
    const BUTTON_HIGH: f32 = 24.0;
    let time_font = crate::studio::fixed(11.0);
    let time = time_of_day(notice.time, form);
    let time_wide = ui.painter().layout_no_wrap("00:00".to_owned(), time_font.clone(), TEXT_FAINT).size().x;
    let text_wide = LIST_WIDE - 2.0 * SIDE - MARK - 2.0 * BETWEEN - time_wide;
    let ink = match (notice.kind, notice.seen) {
        (_, true) => TEXT_FAINT,
        (Kind::Info, false) => TEXT_DIM,
        _ => TEXT,
    };
    let mut job = egui::text::LayoutJob::simple(notice.text.clone(), FontId::proportional(13.0), ink, text_wide);
    if let Some(section) = job.sections.first_mut() {
        section.format.line_height = Some(13.0 * 1.45);
    }
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let buttons = if notice.actions.is_empty() { 0.0 } else { 8.0 + BUTTON_HIGH };
    let high = 10.0 + galley.size().y.max(MARK) + buttons + 10.0;
    let (rect, _) = ui.allocate_exact_size(vec2(LIST_WIDE, high), Sense::hover());
    let painter = ui.painter();
    if notice.kind == Kind::Question {
        painter.rect_filled(rect, 0.0, AMBER_ROW);
    }
    if line_above {
        painter.hline(rect.x_range(), rect.min.y + 0.5, Stroke::new(1.0, ROW_LINE));
    }
    let top = rect.min.y + 10.0;
    let mark_x = rect.min.x + SIDE + MARK / 2.0;
    paint_mark(painter, pos2(mark_x, top + 9.5), notice.kind);
    let text_at = pos2(rect.min.x + SIDE + MARK + BETWEEN, top);
    let text_high = galley.size().y;
    painter.galley(text_at, galley, ink);
    painter.text(pos2(rect.max.x - SIDE, top + 2.0), Align2::RIGHT_TOP, time, time_font, TEXT_FAINT);
    let mut clicked = None;
    let mut x = text_at.x;
    let y = top + text_high.max(MARK) + 8.0;
    for (place, action) in notice.actions.iter().enumerate() {
        let font = FontId::proportional(12.0);
        let wide = crate::studio::flat_button_width(ui, &[action.label.as_str()], &font, 10.0);
        let at = Rect::from_min_size(pos2(x, y), vec2(wide, BUTTON_HIGH));
        // The first answer is the main one (Reload): the ink, with dark words.
        let look = if place == 0 { crate::studio::Flat::Main } else { crate::studio::Flat::Plain };
        let id = list_id().with((notice.id, place));
        if crate::studio::flat_button(ui, at, id, &action.label, font, look, true).clicked() {
            clicked = Some(action.id.clone());
        }
        x += wide + 8.0;
    }
    clicked
}

/// What a song changed outside SoftPresenter asks under its title bar, with room for all of it.
pub fn changed_outside() -> &'static str {
    t(
        "notices.changed_outside",
        "This song was changed outside SoftPresenter. Read it again from its file, or keep SoftPresenter's version?",
    )
}

/// The strip asking under a song's title bar: the room above and below it, its padding, and
/// its fill (the amber mixed 12% into a card's grey).
const STRIP_ABOVE: f32 = 6.0;
const STRIP_BELOW: f32 = 10.0;
const STRIP_PAD: egui::Vec2 = egui::vec2(12.0, 9.0);
const STRIP_FILL: Color32 = Color32::from_rgb(0x3d, 0x36, 0x1f);
const STRIP_BUTTON_HIGH: f32 = 24.0;

/// The strip's words, laid out to wrap in what its buttons leave of `width`, and the buttons'
/// widths (Reload, Keep mine).
fn strip_layout(ui: &egui::Ui, width: f32) -> (std::sync::Arc<egui::Galley>, [f32; 2]) {
    let font = FontId::proportional(12.0);
    let buttons = [t("notices.ask.reload", "Reload"), t("notices.ask.keep_mine", "Keep mine")]
        .map(|words| crate::studio::flat_button_width(ui, &[words], &font, 10.0));
    let text_wide = (width - 2.0 * STRIP_PAD.x - 16.0 - 10.0 - buttons[0] - buttons[1] - 2.0 * 10.0).max(120.0);
    let mut job = egui::text::LayoutJob::simple(
        changed_outside().to_owned(),
        FontId::proportional(13.0),
        Color32::PLACEHOLDER,
        text_wide,
    );
    if let Some(section) = job.sections.first_mut() {
        section.format.line_height = Some(13.0 * 1.4);
    }
    (ui.fonts_mut(|fonts| fonts.layout_job(job)), buttons)
}

/// How tall the strip asking under a song's title bar is, `width` wide (the same at every
/// width the song's slides are laid out in, so the slides under it never overlap it).
pub fn ask_strip_height(ui: &egui::Ui, width: f32) -> f32 {
    let (galley, _) = strip_layout(ui, width);
    STRIP_ABOVE + galley.size().y.max(STRIP_BUTTON_HIGH) + 2.0 * STRIP_PAD.y + STRIP_BELOW
}

/// The strip asking about a song changed outside SoftPresenter, under its title bar, in `band`
/// ([`ask_strip_height`] tall): the question whole, with Reload (the ink) and Keep mine. The
/// answer is carried out as one given in the list is.
pub fn ask_strip(ui: &egui::Ui, state: &mut crate::home::HomeState, band: Rect, about: Waiting) {
    let (galley, [reload_wide, keep_wide]) = strip_layout(ui, band.width());
    let strip = Rect::from_min_max(band.min + vec2(0.0, STRIP_ABOVE), band.max - vec2(0.0, STRIP_BELOW));
    let painter = ui.painter();
    painter.rect_filled(strip, 11.0, STRIP_FILL);
    let middle = strip.center().y;
    paint_mark(painter, pos2(strip.min.x + STRIP_PAD.x + 8.0, middle), Kind::Question);
    let text_at = pos2(strip.min.x + STRIP_PAD.x + 16.0 + 10.0, middle - galley.size().y / 2.0);
    painter.galley(text_at, galley, TEXT);
    let font = FontId::proportional(12.0);
    let keep = Rect::from_min_size(
        pos2(strip.max.x - STRIP_PAD.x - keep_wide, middle - STRIP_BUTTON_HIGH / 2.0),
        vec2(keep_wide, STRIP_BUTTON_HIGH),
    );
    let reload =
        Rect::from_min_size(pos2(keep.min.x - 10.0 - reload_wide, keep.min.y), vec2(reload_wide, STRIP_BUTTON_HIGH));
    let main = crate::studio::Flat::Main;
    if crate::studio::flat_button(
        ui,
        reload,
        strip_answer_id(about, RELOAD),
        t("notices.ask.reload", "Reload"),
        font.clone(),
        main,
        true,
    )
    .hint(t("notices.ask.reload.hint", "Take the file's version (SoftPresenter's changes to it go)"))
    .clicked()
    {
        state.waiting_choice = Some((about, true));
    }
    let plain = crate::studio::Flat::Plain;
    if crate::studio::flat_button(
        ui,
        keep,
        strip_answer_id(about, KEEP_MINE),
        t("notices.ask.keep_mine", "Keep mine"),
        font,
        plain,
        true,
    )
    .hint(t("notices.ask.keep_mine.hint", "Save SoftPresenter's version over the file"))
    .clicked()
    {
        state.waiting_choice = Some((about, false));
    }
}

/// The id of one of the strip's answers ([`RELOAD`] or [`KEEP_MINE`]) for what it asks about.
pub fn strip_answer_id(about: Waiting, answer: &str) -> egui::Id {
    button_id().with(("strip", about, answer))
}

/// At the end of every frame of the main window: notes what was clicked (or where something was
/// dropped), then draws the last refusal's cue, a small dark chip under the place of action:
/// under what was clicked when the refusal came within [`CLICK_NEAR`] of the click, else under
/// the preview's readout (a key was pressed), else under the top bar. It shows for [`CUE_FOR`]
/// seconds; the window is drawn again only while it comes and goes, and once when it is to fade.
pub fn paint_cue(ctx: &egui::Context, notices: &mut Notices) {
    let now = ctx.input(|input| input.time);
    let (clicked, dropped) = ctx.interaction_snapshot(|snapshot| (snapshot.clicked, snapshot.drag_stopped));
    if let Some(rect) = clicked.and_then(|id| ctx.read_response(id)).map(|response| response.rect) {
        notices.clicked = Some((rect, now));
    } else if dropped.is_some()
        && let Some(at) = ctx.input(|input| input.pointer.latest_pos())
    {
        notices.clicked = Some((Rect::from_center_size(at, vec2(1.0, 18.0)), now));
    }
    let readout = notices.readout.take();
    let clicked = notices.clicked;
    draw_cue(ctx, &mut notices.cue, || {
        let recent = clicked.filter(|(_, at)| now - at <= CLICK_NEAR).map(|(rect, _)| rect);
        recent.or(readout).unwrap_or_else(|| {
            let screen = ctx.content_rect();
            Rect::from_center_size(pos2(screen.center().x, crate::studio::TOP_BAR), vec2(0.0, 0.0))
        })
    });
}

/// A refusal's cue for a window of its own (a sheet in the Settings window, where the main
/// window's [`paint_cue`] doesn't draw): `Cue::new` it when the refusal comes, and draw it here
/// every frame, under what `under` gives the first time it is drawn.
pub fn draw_cue(ctx: &egui::Context, cue: &mut Option<Cue>, under: impl FnOnce() -> Rect) {
    let now = ctx.input(|input| input.time);
    let Some(shown_cue) = cue.as_mut() else { return };
    let since = *shown_cue.since.get_or_insert(now);
    let under = *shown_cue.under.get_or_insert_with(under);
    let age = now - since;
    if age >= CUE_FOR {
        *cue = None;
        return;
    }
    let Some(cue) = cue.as_ref() else { return };
    let shown = (age / CUE_FADE).min((CUE_FOR - age) / CUE_FADE).clamp(0.0, 1.0) as f32;
    if shown < 1.0 {
        ctx.request_repaint();
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(CUE_FOR - CUE_FADE - age));
    }
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("refusal cue")));
    let wide = 360.0_f32.min(screen.width() - 20.0);
    let galley = painter.layout(cue.text.clone(), FontId::proportional(12.5), Color32::PLACEHOLDER, wide - 24.0);
    let size = galley.size() + vec2(24.0, 14.0);
    let left = if under.width() < 1.0 { under.center().x - size.x / 2.0 } else { under.min.x };
    let left = left.clamp(screen.min.x + 10.0, (screen.max.x - size.x - 10.0).max(screen.min.x + 10.0));
    // It slides down a little as it comes.
    let top = (under.max.y + 6.0).min(screen.max.y - size.y - 10.0) - 4.0 * (1.0 - shown);
    let chip = Rect::from_min_size(pos2(left, top), size);
    painter.rect_filled(
        chip.expand(6.0).translate(vec2(0.0, 6.0)),
        15.0,
        Color32::from_black_alpha(60).gamma_multiply(shown),
    );
    painter.rect_filled(chip, 9.0, CUE_FILL.gamma_multiply(shown));
    painter.rect_stroke(chip, 9.0, Stroke::new(1.0, CUE_EDGE.gamma_multiply(shown)), egui::StrokeKind::Inside);
    painter.galley(chip.min + vec2(12.0, 7.0), galley, TEXT.gamma_multiply(shown));
}

/// A refusal's chip, and its edge.
const CUE_FILL: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
const CUE_EDGE: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);

/// An entry's mark, centred on `at`: an amber dot for trouble, a grey one for information, and
/// an amber disc with "?" for a question.
fn paint_mark(painter: &egui::Painter, at: egui::Pos2, kind: Kind) {
    match kind {
        Kind::Question => {
            painter.circle_filled(at, 8.0, CARE);
            painter.text(at, Align2::CENTER_CENTER, "?", crate::studio::fixed_semibold(10.0), ON_AMBER);
        }
        Kind::Trouble => {
            painter.circle_filled(at, 5.0, CARE);
        }
        Kind::Info => {
            painter.circle_filled(at, 5.0, INFO_MARK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_not_written_says_why_in_the_language_set() {
        use crate::i18n::{Language, with_language};
        use std::io::{Error, ErrorKind};
        let full = Error::from(ErrorKind::StorageFull);
        with_language(Language::English, || {
            assert_eq!(crate::service::not_saved(&full), "Could not save the playlists: the disk is full");
        });
        with_language(Language::Spanish, || {
            assert_eq!(crate::service::not_saved(&full), "No se pudieron guardar las listas: el disco está lleno");
            assert_eq!(
                write_problem(&Error::from(ErrorKind::PermissionDenied)),
                "SoftPresenter no tiene permiso para escribir ahí"
            );
            assert_eq!(write_problem(&Error::from(ErrorKind::ReadOnlyFilesystem)), "el disco es de solo lectura");
        });
        // Any other reason is the system's own.
        let other = Error::other("the cable came out");
        assert_eq!(write_problem(&other), "the cable came out");
    }

    #[test]
    fn a_file_another_program_holds_is_said_in_plain_words_and_no_error_number_is_shown() {
        use crate::i18n::{Language, with_language};
        use std::io::{Error, ErrorKind};
        // A file another program has open: on Windows a sharing or lock violation (the system's
        // own words end "(os error 32)"), elsewhere a busy file.
        let mut held = vec![Error::from(ErrorKind::ResourceBusy)];
        if cfg!(windows) {
            held.extend([Error::from_raw_os_error(32), Error::from_raw_os_error(33)]);
        }
        for error in &held {
            with_language(Language::English, || {
                assert_eq!(write_problem(error), "another program has the file open", "{error}");
            });
            with_language(Language::Russian, || assert_eq!(write_problem(error), "файл открыт в другой программе"));
        }
        // Any other system sentence is kept, without the number Rust adds to it.
        let numbered = Error::from_raw_os_error(if cfg!(windows) { 1117 } else { 5 });
        assert!(numbered.to_string().contains("(os error"), "{numbered}");
        let said = write_problem(&numbered);
        assert!(!said.contains("os error") && !said.is_empty(), "{said}");
        assert_eq!(without_error_number("Access is denied. (os error 5)"), "Access is denied.");
        assert_eq!(without_error_number("no number (here)"), "no number (here)");
    }

    #[test]
    fn the_button_is_quiet_then_dotted_then_amber_then_asks() {
        let mut notices = Notices::default();
        assert_eq!(notices.look(), Look::Quiet, "nothing new: a quiet list icon");
        notices.info("Reading Ukrainian.xml\u{2026}");
        assert_eq!(notices.look(), Look::New, "new information: a small grey dot");
        notices.trouble("Couldn't save the playlists.");
        notices.trouble("The output has stopped.");
        assert_eq!(notices.look(), Look::Trouble(2), "trouble: amber, counted");
        notices.mark_seen();
        assert_eq!(notices.look(), Look::Quiet, "all seen: quiet again");
        notices.ask(Waiting::Song(1), "Harbour Lights was changed outside SoftPresenter.");
        assert_eq!(notices.look(), Look::Question, "a question alone: ?");
        notices.mark_seen();
        assert_eq!(notices.look(), Look::Question, "a question is never seen: it waits for its answer");
        notices.trouble("Couldn't read Hymns.xml.");
        assert_eq!(notices.look(), Look::Trouble(2), "with trouble too, both are counted");
    }

    #[test]
    fn a_device_that_wants_to_show_its_screen_is_a_question_with_allow_and_dont_allow() {
        let mut notices = Notices::default();
        notices.ask_to_show(7, "Anna's iPhone wants to show its screen.");
        // Asked once, however often the device says it.
        notices.ask_to_show(7, "Anna's iPhone wants to show its screen.");
        assert_eq!(notices.entries().count(), 1);
        let question = notices.entries().next().unwrap().clone();
        assert_eq!(question.kind, Kind::Question);
        assert_eq!(
            question.actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(),
            ["Allow", "Don\u{2019}t Allow"]
        );
        assert_eq!(notices.answer(question.id, ALLOW), Some((Waiting::Mirror(7), true)));
        assert_eq!(notices.answer(question.id, DONT_ALLOW), Some((Waiting::Mirror(7), false)));
        // Not the answers of a file changed outside the app.
        assert_eq!(notices.answer(question.id, RELOAD), None);
        // Another device is another question; each goes by itself, and Clear All leaves them.
        notices.ask_to_show(8, "Tom's iPad wants to show its screen.");
        notices.clear_all();
        assert_eq!(notices.entries().count(), 2);
        notices.settle(Waiting::Mirror(7));
        assert!(!notices.asks_about(Waiting::Mirror(7)) && notices.asks_about(Waiting::Mirror(8)));
    }

    #[test]
    fn the_list_is_newest_first_with_questions_at_its_head() {
        let mut notices = Notices::default();
        notices.info("first");
        notices.ask(Waiting::Theme(0), "a question");
        notices.trouble("second");
        notices.info("third");
        let texts: Vec<&str> = notices.entries().map(|notice| notice.text.as_str()).collect();
        assert_eq!(texts, ["a question", "third", "second", "first"]);
        let question = notices.entries().next().unwrap();
        assert_eq!(question.actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Reload", "Keep mine"]);
        assert_eq!(notices.answer(question.id, RELOAD), Some((Waiting::Theme(0), true)));
        assert_eq!(notices.answer(question.id, KEEP_MINE), Some((Waiting::Theme(0), false)));
        assert_eq!(notices.answer(question.id, "other"), None);
    }

    #[test]
    fn closing_marks_all_seen_and_clear_all_keeps_questions() {
        let mut notices = Notices::default();
        notices.info("Added Harbour Lights to Evening.");
        notices.trouble("Couldn't save the timers.");
        notices.ask(Waiting::Song(3), "Steady Ground was changed outside SoftPresenter.");
        notices.mark_seen();
        assert!(notices.entries().filter(|n| n.kind != Kind::Question).all(|n| n.seen));
        assert!(notices.entries().filter(|n| n.kind == Kind::Question).all(|n| !n.seen));
        assert!(notices.clearable());
        notices.clear_all();
        assert_eq!(notices.entries().count(), 1, "only the question stays");
        assert!(notices.asks_about(Waiting::Song(3)));
        assert!(!notices.clearable(), "Clear All has nothing more to take");
        notices.settle(Waiting::Song(3));
        assert!(notices.entries().next().is_none() && notices.look() == Look::Quiet);
    }

    #[test]
    fn a_remote_page_cannot_see_off_or_clear_what_is_about_security() {
        let mut notices = Notices::default();
        notices.info("Added Harbour Lights to Evening.");
        notices.secure(Kind::Info, "A new certificate key.");
        notices.secure(Kind::Trouble, "Wrong passwords keep coming.");
        notices.mark_seen_for_page();
        let seen = |notices: &Notices, text: &str| notices.entries().find(|n| n.text == text).map(|n| n.seen);
        assert_eq!(seen(&notices, "Added Harbour Lights to Evening."), Some(true));
        assert_eq!(seen(&notices, "A new certificate key."), Some(false));
        assert_eq!(seen(&notices, "Wrong passwords keep coming."), Some(false));
        notices.clear_all_for_page();
        assert_eq!(notices.entries().count(), 2, "the ordinary one went, the two about security stay");
        // The operator closes the list in the app: seen, and now a page may clear them.
        notices.mark_seen();
        notices.clear_all_for_page();
        assert_eq!(notices.entries().count(), 0);
        // Said again while unseen, it is not doubled, and is still about security.
        notices.secure(Kind::Info, "A new certificate key.");
        notices.secure(Kind::Info, "A new certificate key.");
        assert_eq!(notices.entries().count(), 1);
        assert!(notices.entries().all(|n| n.secure));
    }

    #[test]
    fn the_list_keeps_the_last_forty_and_every_question() {
        let mut notices = Notices::default();
        notices.ask(Waiting::Song(0), "an old question");
        for n in 0..45 {
            notices.info(format!("notice {n}"));
        }
        let kept: Vec<&Notice> = notices.entries().collect();
        assert_eq!(kept.len(), KEPT + 1, "forty entries and the question");
        assert_eq!(kept[0].text, "an old question", "the question outlives the cap");
        assert_eq!(kept[1].text, "notice 44");
        assert_eq!(kept[KEPT].text, "notice 5", "the oldest five went");
    }

    #[test]
    fn a_question_is_asked_once_and_the_same_words_twice_are_one_entry() {
        let mut notices = Notices::default();
        notices.ask(Waiting::Song(2), "changed");
        notices.ask(Waiting::Song(2), "changed");
        notices.trouble("Couldn't save the screens.");
        notices.trouble("Couldn't save the screens.");
        assert_eq!(notices.entries().count(), 2);
        assert!(notices.asks_about(Waiting::Song(2)) && !notices.asks_about(Waiting::Song(1)));
        // Seen, the same trouble again is new again.
        notices.mark_seen();
        notices.trouble("Couldn't save the screens.");
        assert_eq!(notices.entries().count(), 2);
        assert_eq!(notices.look(), Look::Trouble(2));
    }

    #[test]
    fn a_refusal_makes_no_entry_but_the_command_line_can_say_it() {
        let mut notices = Notices::default();
        let mark = notices.mark();
        notices.refuse("Look a passage up first.");
        assert_eq!(notices.entries().count(), 0, "a refusal is a cue, never an entry");
        assert_eq!(notices.look(), Look::Quiet);
        assert_eq!(notices.refused(), Some("Look a passage up first."));
        assert_eq!(notices.said_since(mark), Some("Look a passage up first."));
        let mark = notices.mark();
        assert_eq!(notices.said_since(mark), None, "nothing since");
        notices.trouble("That media playlist has nothing in it.");
        assert_eq!(notices.said_since(mark), Some("That media playlist has nothing in it."));
    }

    #[test]
    fn lasting_trouble_tried_every_frame_stays_quiet_once_seen() {
        let mut notices = Notices::default();
        for _ in 0..60 {
            notices.lasting_trouble("Could not open a window for \u{201c}Left\u{201d}.");
        }
        assert_eq!(notices.look(), Look::Trouble(1));
        notices.mark_seen();
        for _ in 0..60 {
            notices.lasting_trouble("Could not open a window for \u{201c}Left\u{201d}.");
        }
        assert_eq!(notices.look(), Look::Quiet, "closing the list quiets the button");
        assert_eq!(notices.entries().count(), 1);
    }

    #[test]
    fn a_question_follows_the_theme_it_is_about() {
        let mut notices = Notices::default();
        notices.ask(Waiting::Theme(2), "The theme Evening was changed outside SoftPresenter.");
        notices.ask(Waiting::Song(2), "Two was changed outside SoftPresenter.");
        // The theme before it is deleted: it is the second theme now; the song's stays.
        notices.renumber(|about| match about {
            Waiting::Theme(at) => Waiting::Theme(at - 1),
            song => song,
        });
        assert!(notices.asks_about(Waiting::Theme(1)) && !notices.asks_about(Waiting::Theme(2)));
        assert!(notices.asks_about(Waiting::Song(2)));
    }

    #[test]
    fn something_said_again_and_again_is_one_entry_until_seen() {
        let mut notices = Notices::default();
        for _ in 0..100 {
            notices.trouble("Could not open a window for \u{201c}Left\u{201d}.");
            notices.trouble("Could not draw to \u{201c}Right\u{201d}.");
        }
        assert_eq!(notices.entries().count(), 2, "said every frame, still two entries");
        notices.mark_seen();
        notices.trouble("Could not open a window for \u{201c}Left\u{201d}.");
        let texts: Vec<&str> = notices.entries().map(|notice| notice.text.as_str()).collect();
        assert_eq!(texts.len(), 2, "seen and said again: back at the head, not twice");
        assert!(texts[0].starts_with("Could not open"));
        assert_eq!(notices.look(), Look::Trouble(1));
    }

    #[test]
    fn the_list_is_plain_data_to_send_as_it_is() {
        let mut notices = Notices::default();
        notices.ask(Waiting::Song(0), "Harbour Lights was changed outside SoftPresenter.");
        notices.info("Installed Ukrainian Ogienko");
        let sent = serde_json::to_value(notices.to_vec()).unwrap();
        assert_eq!(sent[0]["kind"], "ask");
        assert_eq!(sent[0]["actions"][0]["id"], "outside:reload");
        assert_eq!(sent[0]["actions"][0]["label"], "Reload");
        assert_eq!(sent[1]["kind"], "info");
        assert_eq!(sent[1]["text"], "Installed Ukrainian Ogienko");
        assert_eq!(sent[1]["seen"], false);
        assert!(sent[1]["time"].as_i64().is_some_and(|time| time > 0));
        assert!(sent[0].get("about").is_none());
    }

    /// The words of the trouble entries in the list, newest first.
    fn troubles(notices: &Notices) -> Vec<String> {
        notices.to_vec().into_iter().filter(|notice| notice.kind == Kind::Trouble).map(|notice| notice.text).collect()
    }

    /// A service asked every 30 seconds while it fails, as VoxLive is.
    fn outage() -> Outage {
        Outage::asked_every(Duration::from_secs(30))
    }

    #[test]
    fn a_service_out_of_reach_is_said_once_an_outage_after_two_minutes() {
        let (mut notices, mut outage) = (Notices::default(), outage());
        let start = Instant::now();
        let at = |seconds: u64| Heard::Failed(start + Duration::from_secs(seconds));
        let words = || "Choir Hub can't be reached.".to_owned();
        // Failed answers over 90 seconds, each with other words: nothing.
        for (seconds, said) in [(0, "timed out"), (45, "no address"), (90, "no route")] {
            assert!(!outage.follow(at(seconds), false, || said.to_owned(), &mut notices));
        }
        assert!(troubles(&notices).is_empty(), "a short outage says nothing");
        // Still failing two minutes after the first: said, once, whatever comes after.
        assert!(outage.follow(at(120), false, words, &mut notices));
        for seconds in [120, 150, 400] {
            assert!(!outage.follow(at(seconds), true, || "other words".to_owned(), &mut notices));
        }
        assert_eq!(troubles(&notices), [words()]);
        // It answers: the entry goes by itself, with nothing said.
        assert!(outage.follow(Heard::Ended, false, words, &mut notices));
        assert!(notices.to_vec().is_empty());
        assert!(!outage.follow(Heard::Ended, false, words, &mut notices));
        // Out of reach again: a new outage, said after its own two minutes.
        assert!(!outage.follow(at(500), false, words, &mut notices));
        assert!(!outage.follow(at(560), false, words, &mut notices));
        assert!(outage.follow(at(620), false, words, &mut notices));
        assert_eq!(troubles(&notices), [words()]);
    }

    #[test]
    fn nothing_new_heard_leaves_an_outage_as_it_stands() {
        let (mut notices, mut outage) = (Notices::default(), outage());
        let start = Instant::now();
        let at = |seconds: u64| Heard::Failed(start + Duration::from_secs(seconds));
        let words = || "Choir Hub can't be reached.".to_owned();
        // Said at once while on screen; then a new key is tried, and none is given for a while.
        assert!(outage.follow(at(0), true, words, &mut notices));
        for _ in 0..3 {
            assert!(!outage.follow(Heard::Nothing, true, words, &mut notices));
        }
        assert_eq!(troubles(&notices), [words()], "the entry stays");
        // Still out of reach with the new key: the same outage, nothing said again.
        assert!(!outage.follow(at(10), true, words, &mut notices));
        assert_eq!(troubles(&notices).len(), 1);
        // A clock that counted on across a key: not on screen, said two minutes after the first.
        let (mut notices, mut outage) = (Notices::default(), self::outage());
        assert!(!outage.follow(at(0), false, words, &mut notices));
        assert!(!outage.follow(Heard::Nothing, false, words, &mut notices));
        assert!(outage.follow(at(120), false, words, &mut notices));
    }

    #[test]
    fn a_long_gap_between_failures_starts_the_clock_again() {
        let (mut notices, mut outage) = (Notices::default(), outage());
        let start = Instant::now();
        let at = |seconds: u64| Heard::Failed(start + Duration::from_secs(seconds));
        let words = || "Choir Hub can't be reached.".to_owned();
        // One failure, then the computer sleeps for six minutes: waking fails again, said nothing.
        assert!(!outage.follow(at(0), false, words, &mut notices));
        assert!(!outage.follow(at(360), false, words, &mut notices));
        assert!(!outage.follow(at(420), false, words, &mut notices));
        assert!(troubles(&notices).is_empty(), "the clock started again at 6:00");
        assert!(outage.follow(at(480), false, words, &mut notices), "two minutes after waking");
        // A service asked every five minutes: a gap of five and a half is no sleep.
        let (mut notices, mut outage) = (Notices::default(), Outage::asked_every(Duration::from_secs(300)));
        assert!(!outage.follow(at(0), false, words, &mut notices));
        assert!(outage.follow(at(330), false, words, &mut notices));
    }

    #[test]
    fn a_service_on_screen_is_said_out_of_reach_at_once_and_once() {
        let (mut notices, mut outage) = (Notices::default(), outage());
        let start = Instant::now();
        let at = |seconds: u64| Heard::Failed(start + Duration::from_secs(seconds));
        let words = || "Choir Hub can't be reached.".to_owned();
        assert!(!outage.follow(at(0), false, words, &mut notices), "not on screen: it waits");
        // What it does goes on screen while the outage lasts: said then.
        assert!(outage.follow(at(30), true, words, &mut notices));
        assert!(!outage.follow(at(60), true, words, &mut notices));
        assert_eq!(troubles(&notices).len(), 1);
        // Cleared by the operator meanwhile: not said again in the same outage.
        notices.clear_all();
        assert!(!outage.follow(at(200), true, words, &mut notices));
        assert!(troubles(&notices).is_empty());
    }

    #[test]
    fn a_service_out_of_reach_is_said_in_plain_words() {
        use crate::i18n::{Language, with_language};
        for language in Language::ALL {
            let words = with_language(language, || unreachable("VoxLive"));
            assert!(words.starts_with("VoxLive") || words.contains(" VoxLive"), "{words}");
            assert!(words.contains("SoftPresenter") && !words.contains('{'), "{words}");
        }
        assert_eq!(
            with_language(Language::English, || unreachable("VoxLive")),
            "VoxLive can't be reached. SoftPresenter keeps trying. Check this computer's internet connection."
        );
        for language in Language::ALL {
            let words = with_language(language, || unreachable_try_again("TimelyChurch"));
            assert!(words.contains("TimelyChurch") && !words.contains('{'), "{words}");
        }
    }

    #[test]
    fn the_time_is_hours_and_minutes_on_the_window_clock() {
        use chrono::TimeZone;
        let time = chrono::Local.with_ymd_and_hms(2026, 11, 22, 15, 4, 0).unwrap().timestamp();
        let mut form = crate::clock::TimeForm::default();
        assert_eq!(time_of_day(time, &form), "15:04");
        form.twelve = true;
        form.pad_hour = false;
        assert_eq!(time_of_day(time, &form), "3:04");
    }
}
