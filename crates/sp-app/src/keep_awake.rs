//! Keeping the display awake while the screens are on.
//!
//! A computer left at its usual settings dims its display, turns it off, starts its screen saver,
//! locks or goes to sleep when nobody has touched it for a while: on a laptop, a long sermon on
//! one slide ended with the projector black or showing the lock screen. While any screen is on,
//! the app asks the system not to; when the screens are switched off, or the app quits, it lets
//! go, and the computer's own settings apply again. Nothing in the system's settings is changed.
//!
//! One owner, [`KeepAwake`], held by the app. `App::sync_screens` tells it whether the screens
//! are on, and it acts only when that changes (never on the screens' thread); `about_to_wait`
//! lets it look, now and then, whether what holds the display is still holding it; the app's
//! quit lets go for good. How each system is asked is behind [`Hold`], so the tests try the
//! state without asking anything of the computer they run on:
//!
//! - **macOS:** Apple's own `caffeinate -d -i -w <the app's process>`, which holds the system's
//!   "prevent user idle display sleep" and "prevent user idle system sleep" assertions (what a
//!   video player holds; `pmset -g assertions` lists them under caffeinate) and ends by itself
//!   when the app ends, however it ends. Letting go stops it. No new library, and no `unsafe`.
//! - **Windows:** a power request (`PowerCreateRequest`, then `PowerSetRequest` for the display
//!   and for the computer), as browsers make while a video plays: `powercfg /requests` lists it
//!   under DISPLAY and SYSTEM with its reason. Made through `sp_winapi`; Windows clears it when
//!   the app ends, however it ends.
//! - **Linux:** `systemd-inhibit --what=idle`, which logind lists (`systemd-inhibit --list`) and
//!   KDE Plasma honours (it keeps the screen from dimming, turning off and locking, and the
//!   computer from sleeping), and `gnome-session-inhibit --inhibit=idle:suspend` where it is
//!   installed, which GNOME honours (GNOME takes no notice of logind's idle locks, so on GNOME
//!   the display counts as kept awake only while that one holds); each runs
//!   `cat`, whose input the app holds, so it ends when the app ends, however it ends. A desktop
//!   that answers neither (one that only takes `org.freedesktop.ScreenSaver` over D-Bus) is not
//!   kept awake: that needs a D-Bus client, which the app doesn't have.
//!
//! A test copy (a quiet run) holds a pretend hold that asks nothing of the computer, unless
//! `SOFTPRESENTER_KEEP_AWAKE` says otherwise (see [`KeepAwake::for_this_run`]).

use std::time::{Duration, Instant};

use crate::applog;
use crate::screens::Placed;

/// How long after asking the first look is: a helper program that can't hold (refused, or on a
/// desktop that doesn't answer it) has stopped by then.
const FIRST_LOOK: Duration = Duration::from_secs(2);
/// How often it is looked at again while it holds.
const LOOK_EVERY: Duration = Duration::from_secs(30);
/// How long after a failure it is tried again, while the screens stay on.
const TRY_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// The reason given to the system, which its own lists show (`powercfg /requests`,
/// `systemd-inhibit --list`, `gnome-session-inhibit --list`).
#[cfg(any(windows, target_os = "linux"))]
const REASON: &str = "The screens are on";

/// Whether a screen, placed where it is, shows or sends what the room or a stream sees: one that
/// fills a display, sends over NDI, plays out of a DeckLink output (with the driver there to
/// play it), or shows in a window. A window counts whether the person chose it (to capture it
/// for a stream, say) or an automatic screen opened in it for want of another display: that is
/// a laptop mirrored to a projector, which must not go dark in the middle of a sermon. The
/// person switches the screens off to let the computer sleep.
pub fn shows(place: &Placed, decklink_ready: bool) -> bool {
    match place {
        Placed::Fullscreen(_) | Placed::Window | Placed::Ndi(_) => true,
        Placed::DeckLink(_) => decklink_ready,
        Placed::Nothing(_) => false,
    }
}

/// Whether any screen that shows (see [`shows`]) is switched on, its kind too: the screens are
/// on, and the display is kept awake.
pub fn any_on(shows: &[bool], on: &[bool]) -> bool {
    shows.iter().zip(on).any(|(shows, on)| *shows && *on)
}

/// How a system is asked to keep the display awake. Each call is quick and never waits; the
/// state ([`KeepAwake`]) decides when each is made.
pub trait Hold {
    /// Asks the system to keep the display awake: the way it holds it, in words for the log and
    /// `sp status`, or why it couldn't.
    fn take(&mut self) -> Result<String, String>;
    /// Looks whether the hold is still in place (a helper program may have stopped): the way
    /// it holds it now, or why it holds nothing any more.
    fn look(&mut self) -> Result<String, String>;
    /// Lets go of whatever is held.
    fn release(&mut self);
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// Nothing is held: no screen is on.
    Off,
    /// Asked, in this way, and not yet looked at (see `FIRST_LOOK`).
    Asked(String),
    /// Held, in this way.
    Held(String),
    /// The screens are on, but the system couldn't be asked or let go; why. Tried again later.
    Failed(String),
    /// The app is quitting: nothing is held, or ever taken again.
    Ended,
}

/// The display kept awake while the screens are on (see the module's words).
pub struct KeepAwake {
    hold: Box<dyn Hold>,
    state: State,
    /// Whether the screens are on, as last told.
    wanted: bool,
    /// When to look at the hold again, or try again after a failure.
    next_look: Option<Instant>,
    /// The last failure the log said, so one that comes back each minute is said once.
    said: Option<String>,
    /// Whether the operator has been told of a failure (once a run).
    told: bool,
}

impl KeepAwake {
    pub fn new(hold: Box<dyn Hold>) -> KeepAwake {
        KeepAwake { hold, state: State::Off, wanted: false, next_look: None, said: None, told: false }
    }

    /// The way this run keeps the display awake: the system's own, or in a quiet run (a test or
    /// a capture) a pretend one that asks nothing of the computer. The testing aid is read once,
    /// here, and only in a test copy (quiet, or with a data folder of its own).
    pub fn for_this_run(quiet: bool) -> KeepAwake {
        let test_copy = quiet || std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|data| !data.is_empty());
        // Testing aid: SOFTPRESENTER_KEEP_AWAKE=1 (or `real`) has a quiet run keep the display
        // awake the system's own way, as a person's copy does, to see it in the system's lists
        // (`pmset -g assertions`, `powercfg /requests`, `systemd-inhibit --list`); `pretend`
        // asks nothing of the computer in any test copy; `fail` is refused every time, to see
        // the log's line and the notice. Honoured only in a test copy.
        let asked = if test_copy { std::env::var("SOFTPRESENTER_KEEP_AWAKE").ok() } else { None };
        let hold: Box<dyn Hold> = match asked.as_deref().map(str::trim) {
            Some("1" | "real") => system(),
            Some("pretend") => Box::new(Pretend),
            Some("fail") => Box::new(Refused("SOFTPRESENTER_KEEP_AWAKE=fail, a testing aid")),
            _ if quiet => Box::new(Pretend),
            _ => system(),
        };
        if let Some(asked) = &asked {
            applog::event("power", &format!("Testing aid: SOFTPRESENTER_KEEP_AWAKE={asked}"));
        }
        KeepAwake::new(hold)
    }

    /// Tells it whether the screens are on now (every frame, from `sync_screens`): it acts only
    /// when that changed, taking hold or letting go. A failure is given back, the first time in
    /// a run, for the operator's notices.
    pub fn set(&mut self, on: bool, now: Instant) -> Option<String> {
        if self.state == State::Ended || on == self.wanted {
            return None;
        }
        self.wanted = on;
        if on {
            self.take(now)
        } else {
            self.let_go();
            None
        }
    }

    /// Looks whether the hold is still in place, or tries again after a failure, when that is
    /// due (see [`KeepAwake::next_look`]); otherwise does nothing. A failure is given back as
    /// `set` gives it.
    pub fn look(&mut self, now: Instant) -> Option<String> {
        if self.next_look.is_none_or(|at| now < at) {
            return None;
        }
        self.next_look = None;
        match std::mem::replace(&mut self.state, State::Off) {
            State::Asked(_) if self.wanted => self.looked(None, now),
            State::Held(how) if self.wanted => self.looked(Some(how), now),
            State::Failed(_) if self.wanted => self.take(now),
            other => {
                self.state = other;
                None
            }
        }
    }

    /// When the hold is next looked at, or tried again: for the event loop to wake then.
    pub fn next_look(&self) -> Option<Instant> {
        self.next_look
    }

    /// The app is quitting: lets go, and takes nothing again, whatever the screens do while the
    /// quit waits for files to be written.
    pub fn end(&mut self) {
        match std::mem::replace(&mut self.state, State::Ended) {
            State::Held(_) => {
                self.hold.release();
                applog::event("power", "the display is no longer kept awake: SoftPresenter is quitting");
            }
            State::Asked(_) => self.hold.release(),
            State::Off | State::Failed(_) | State::Ended => {}
        }
        self.wanted = false;
        self.next_look = None;
    }

    /// For `sp status`: whether the display is kept awake now, and how (or why not), in words.
    pub fn status(&self) -> (bool, String) {
        match &self.state {
            State::Off => (false, "no screen is on".to_owned()),
            State::Asked(how) => (true, format!("{how}; asked a moment ago")),
            State::Held(how) => (true, how.clone()),
            State::Failed(why) => (false, format!("couldn't: {why}")),
            State::Ended => (false, "quitting".to_owned()),
        }
    }

    fn take(&mut self, now: Instant) -> Option<String> {
        match self.hold.take() {
            Ok(how) => {
                self.state = State::Asked(how);
                self.next_look = Some(now + FIRST_LOOK);
                None
            }
            Err(why) => self.failed(why, now),
        }
    }

    /// Looks at the hold. `held` is how the last look found it holding (none when it has only
    /// been asked): the log says when it first holds, and when the way it holds changes (a
    /// helper of two that stopped).
    fn looked(&mut self, held: Option<String>, now: Instant) -> Option<String> {
        match self.hold.look() {
            Ok(how) => {
                match &held {
                    None => {
                        applog::event("power", &format!("keeping the display awake while screens are on ({how})"));
                    }
                    Some(before) if *before != how => {
                        applog::event("power", &format!("the display is kept awake now by {how}"));
                    }
                    Some(_) => {}
                }
                self.state = State::Held(how);
                self.next_look = Some(now + LOOK_EVERY);
                // A failure that comes back after this is said in the log again.
                self.said = None;
                None
            }
            Err(why) => self.failed(why, now),
        }
    }

    fn let_go(&mut self) {
        match std::mem::replace(&mut self.state, State::Off) {
            State::Held(_) => {
                self.hold.release();
                applog::event("power", "the display is no longer kept awake: no screen is on");
            }
            State::Asked(_) => self.hold.release(),
            State::Off | State::Failed(_) => {}
            State::Ended => self.state = State::Ended,
        }
        self.next_look = None;
    }

    /// The system couldn't be asked, or let go: whatever is left is let go, the log says why
    /// (once for the same reason), it is tried again in a minute, and the operator is told,
    /// once a run.
    fn failed(&mut self, why: String, now: Instant) -> Option<String> {
        self.hold.release();
        if self.said.as_deref() != Some(why.as_str()) {
            applog::event("power", &format!("could not keep the display awake while screens are on: {why}"));
            self.said = Some(why.clone());
        }
        self.state = State::Failed(why.clone());
        self.next_look = Some(now + TRY_AGAIN_AFTER);
        if self.told {
            return None;
        }
        self.told = true;
        Some(why)
    }
}

impl crate::App {
    /// Looks, when that is due, whether the display is still kept awake (a helper program may
    /// have stopped), or tries again after a failure, and tells the operator of a failure (once
    /// a run); when to look next, for the event loop to wake then. Nothing is drawn for it.
    pub(crate) fn tick_keep_awake(&mut self) -> Option<Instant> {
        if self.keep_awake.look(Instant::now()).is_some()
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notices.trouble(notice_words());
            running.home_window.request_redraw();
        }
        self.keep_awake.next_look()
    }
}

/// What the operator is told, once a run, when the display couldn't be kept awake (the log says
/// why).
pub fn notice_words() -> String {
    crate::i18n::t(
        "notices.keep_awake.failed",
        "SoftPresenter couldn't keep the display awake while the screens are on: it may dim, go dark or lock if nobody touches the computer for a while. Check the computer's sleep and screen saver settings.",
    )
    .to_owned()
}

/// What a pretend hold says it holds with.
const PRETEND: &str = "pretend: a test copy asks the computer nothing";

/// Asks nothing of the computer: a test copy's.
struct Pretend;

impl Hold for Pretend {
    fn take(&mut self) -> Result<String, String> {
        Ok(PRETEND.to_owned())
    }

    fn look(&mut self) -> Result<String, String> {
        Ok(PRETEND.to_owned())
    }

    fn release(&mut self) {}
}

/// Refused every time, saying why.
struct Refused(&'static str);

impl Hold for Refused {
    fn take(&mut self) -> Result<String, String> {
        Err(self.0.to_owned())
    }

    fn look(&mut self) -> Result<String, String> {
        Err(self.0.to_owned())
    }

    fn release(&mut self) {}
}

/// The system's own way of keeping the display awake.
#[cfg(target_os = "macos")]
fn system() -> Box<dyn Hold> {
    Box::new(mac::Caffeinate::default())
}

#[cfg(windows)]
fn system() -> Box<dyn Hold> {
    Box::new(win::PowerRequest::default())
}

#[cfg(target_os = "linux")]
fn system() -> Box<dyn Hold> {
    Box::new(linux::Inhibitors::default())
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn system() -> Box<dyn Hold> {
    Box::new(Refused("SoftPresenter knows no way to keep this system's display awake"))
}

/// The system's programs that hold the display awake for as long as they run (macOS, Linux).
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod helper {
    use std::io::Read;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::{Arc, Mutex};

    /// How much of what a helper says on its error output is kept, for the log.
    const WORDS_KEPT: usize = 2000;

    /// One of the system's programs, holding the display awake for as long as it runs.
    pub struct Helper {
        pub name: &'static str,
        child: Child,
        /// The other end of its input, held by the app. It closes when the app ends, however the
        /// app ends (a crash too), and `cat`, which a Linux helper runs for as long as it holds
        /// its lock, ends then, and the helper with it: none is left holding.
        input: Option<ChildStdin>,
        /// What it has said on its error output.
        words: Arc<Mutex<String>>,
    }

    impl Helper {
        /// Starts `program` with `args`. What it says on its error output is read on a thread of
        /// its own, so the window never waits for it.
        pub fn start(name: &'static str, program: &str, args: &[&str]) -> std::io::Result<Helper> {
            let mut child = Command::new(program)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
            let input = child.stdin.take();
            let words = Arc::new(Mutex::new(String::new()));
            if let Some(mut errors) = child.stderr.take() {
                let kept = words.clone();
                let _ = std::thread::Builder::new().name("keep awake helper".to_owned()).spawn(move || {
                    let mut buffer = [0u8; 512];
                    while let Ok(read) = errors.read(&mut buffer) {
                        if read == 0 {
                            break;
                        }
                        if let Ok(mut said) = kept.lock()
                            && said.len() < WORDS_KEPT
                        {
                            said.push_str(&String::from_utf8_lossy(&buffer[..read]));
                        }
                    }
                });
            }
            Ok(Helper { name, child, input, words })
        }

        /// `None` while it runs; once it has stopped, how, with what it said.
        pub fn stopped(&mut self) -> Option<String> {
            match self.child.try_wait() {
                Ok(None) => None,
                Ok(Some(status)) => {
                    let said = self
                        .words
                        .lock()
                        .map(|said| said.split_whitespace().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default();
                    Some(if said.is_empty() {
                        format!("{} stopped ({status})", self.name)
                    } else {
                        format!("{} stopped ({status}): {said}", self.name)
                    })
                }
                Err(error) => Some(format!("{} can't be looked at: {error}", self.name)),
            }
        }

        /// Stops it: its input closes and it is killed, then waited for on a thread of its own,
        /// so it leaves nothing behind and the window never waits.
        pub fn stop(self) {
            let Helper { mut child, input, .. } = self;
            drop(input);
            let _ = child.kill();
            let _ = std::thread::Builder::new().name("keep awake helper".to_owned()).spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// macOS: Apple's own `caffeinate`.
#[cfg(target_os = "macos")]
mod mac {
    use super::Hold;
    use super::helper::Helper;

    /// How `sp status` and the log say it holds.
    const HOW: &str = "caffeinate -d -i";

    /// `caffeinate`, running while held.
    #[derive(Default)]
    pub struct Caffeinate {
        helper: Option<Helper>,
    }

    impl Hold for Caffeinate {
        fn take(&mut self) -> Result<String, String> {
            self.release();
            // -d holds "prevent user idle display sleep": the display doesn't dim or turn off,
            // and the computer can't idle-sleep meanwhile. -i holds "prevent user idle system
            // sleep" too, for a screen that only sends (NDI, DeckLink). -w ends it by itself when
            // the app's process ends, however it ends. Neither stops a person putting the Mac to
            // sleep, or closing a laptop's lid.
            let app = std::process::id().to_string();
            match Helper::start("caffeinate", "/usr/bin/caffeinate", &["-d", "-i", "-w", &app]) {
                Ok(helper) => {
                    self.helper = Some(helper);
                    Ok(HOW.to_owned())
                }
                Err(error) => Err(format!("caffeinate couldn't start: {error}")),
            }
        }

        fn look(&mut self) -> Result<String, String> {
            let Some(helper) = self.helper.as_mut() else {
                return Err("caffeinate isn't running".to_owned());
            };
            match helper.stopped() {
                None => Ok(HOW.to_owned()),
                Some(why) => {
                    self.helper = None;
                    Err(why)
                }
            }
        }

        fn release(&mut self) {
            if let Some(helper) = self.helper.take() {
                helper.stop();
            }
        }
    }
}

/// Windows: a power request, made through `sp_winapi`.
#[cfg(windows)]
mod win {
    use super::{Hold, REASON};

    /// The power request, while held.
    #[derive(Default)]
    pub struct PowerRequest {
        held: Option<sp_winapi::DisplayKeptOn>,
    }

    /// How `sp status` and the log say it holds.
    fn how(held: &sp_winapi::DisplayKeptOn) -> String {
        if held.system_too() {
            "a power request: display and system required".to_owned()
        } else {
            "a power request: display required".to_owned()
        }
    }

    impl Hold for PowerRequest {
        fn take(&mut self) -> Result<String, String> {
            self.held = None;
            let held = sp_winapi::keep_display_on(REASON)?;
            let words = how(&held);
            self.held = Some(held);
            Ok(words)
        }

        fn look(&mut self) -> Result<String, String> {
            self.held.as_ref().map(how).ok_or_else(|| "no power request is held".to_owned())
        }

        fn release(&mut self) {
            self.held = None;
        }
    }
}

/// Linux: logind's idle lock, and GNOME's own where it is installed.
#[cfg(target_os = "linux")]
mod linux {
    use super::helper::Helper;
    use super::{Hold, REASON};
    use crate::applog;

    /// A helper to try: its program, how `sp status` and the log say it holds, and what it is
    /// given. Each runs `cat`, whose input the app holds (see `Helper`).
    struct Wanted {
        program: &'static str,
        how: &'static str,
        args: Vec<String>,
    }

    fn wanted() -> [Wanted; 2] {
        [
            // logind's idle lock, which KDE Plasma honours: no dimming, no turning the screen
            // off, no locking and no sleeping while it is held. Idle only: a sleep lock that
            // blocks would also stop the person putting the computer to sleep themselves, and
            // it needs an administrator's yes in a session that isn't at the computer, which
            // would cost the idle lock asked for with it.
            Wanted {
                program: "systemd-inhibit",
                how: "systemd-inhibit --what=idle",
                args: vec![
                    "--what=idle".to_owned(),
                    "--who=SoftPresenter".to_owned(),
                    format!("--why={REASON}"),
                    "--mode=block".to_owned(),
                    "cat".to_owned(),
                ],
            },
            // GNOME's own, which GNOME honours (it takes no notice of logind's idle lock): the
            // session doesn't go idle (no blanking, no screen saver, no locking) and isn't put
            // to sleep. On another desktop it stops at once, and logind's lock goes on holding.
            Wanted {
                program: GNOME_HELPER,
                how: "gnome-session-inhibit --inhibit=idle:suspend",
                args: vec![
                    "--app-id=softpresenter".to_owned(),
                    format!("--reason={REASON}"),
                    "--inhibit=idle:suspend".to_owned(),
                    "cat".to_owned(),
                ],
            },
        ]
    }

    /// GNOME's helper, the only one GNOME honours.
    const GNOME_HELPER: &str = "gnome-session-inhibit";

    /// Whether the desktop is GNOME (`XDG_CURRENT_DESKTOP` names it, as Ubuntu's `ubuntu:GNOME`
    /// does): there logind's idle lock alone keeps nothing awake, so the display is kept awake
    /// only while GNOME's own helper holds.
    fn on_gnome() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP")
            .is_ok_and(|desktops| desktops.split(':').any(|desktop| desktop.trim().eq_ignore_ascii_case("GNOME")))
    }

    /// The helpers running while held.
    #[derive(Default)]
    pub struct Inhibitors {
        running: Vec<(&'static str, Helper)>,
        /// What the log has said of helpers missing or stopped, so a try again says it once.
        said: Vec<String>,
    }

    impl Inhibitors {
        fn how(&self) -> String {
            self.running.iter().map(|(how, _)| *how).collect::<Vec<_>>().join(" and ")
        }

        /// On GNOME, why nothing GNOME honours holds (its helper isn't running), from what was
        /// said of it; elsewhere, or while it runs, nothing.
        fn gnome_missing(&self, said: &[String]) -> Option<String> {
            if !on_gnome() || self.running.iter().any(|(_, helper)| helper.name == GNOME_HELPER) {
                return None;
            }
            let why = said
                .iter()
                .find(|words| words.starts_with(GNOME_HELPER))
                .cloned()
                .unwrap_or_else(|| format!("{GNOME_HELPER} isn't running"));
            Some(format!("{why}; GNOME takes no notice of systemd-inhibit"))
        }

        fn say(&mut self, words: String) {
            if !self.said.contains(&words) {
                applog::event("power", &words);
                self.said.push(words);
            }
        }
    }

    impl Hold for Inhibitors {
        fn take(&mut self) -> Result<String, String> {
            self.release();
            let mut missing = Vec::new();
            for one in wanted() {
                let args: Vec<&str> = one.args.iter().map(String::as_str).collect();
                match Helper::start(one.program, one.program, &args) {
                    Ok(helper) => self.running.push((one.how, helper)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing.push(format!("{} isn't installed", one.program));
                    }
                    Err(error) => missing.push(format!("{} couldn't start: {error}", one.program)),
                }
            }
            if self.running.is_empty() {
                return Err(missing.join("; "));
            }
            if let Some(why) = self.gnome_missing(&missing) {
                self.release();
                return Err(why);
            }
            // GNOME's helper isn't installed away from GNOME: said once.
            for words in missing {
                self.say(words);
            }
            Ok(self.how())
        }

        fn look(&mut self) -> Result<String, String> {
            let mut stopped = Vec::new();
            self.running.retain_mut(|(_, helper)| match helper.stopped() {
                None => true,
                Some(why) => {
                    stopped.push(why);
                    false
                }
            });
            if self.running.is_empty() {
                return Err(if stopped.is_empty() { "no helper is running".to_owned() } else { stopped.join("; ") });
            }
            if let Some(why) = self.gnome_missing(&stopped) {
                return Err(why);
            }
            // One that stopped while another holds (GNOME's, on a desktop that isn't GNOME):
            // said once.
            for why in stopped {
                self.say(why);
            }
            Ok(self.how())
        }

        fn release(&mut self) {
            for (_, helper) in self.running.drain(..) {
                helper.stop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use super::{FIRST_LOOK, Hold, KeepAwake, LOOK_EVERY, TRY_AGAIN_AFTER, any_on, shows};
    use crate::screens::{DeckLinkOut, Keyer, Placed};

    /// What a made-up hold was asked, and what it gives next (it holds, as "fake", when told
    /// nothing).
    #[derive(Default)]
    struct Book {
        takes: usize,
        looks: usize,
        releases: usize,
        takes_give: VecDeque<Result<String, String>>,
        looks_give: VecDeque<Result<String, String>>,
    }

    struct Fake(Rc<RefCell<Book>>);

    impl Hold for Fake {
        fn take(&mut self) -> Result<String, String> {
            let mut book = self.0.borrow_mut();
            book.takes += 1;
            book.takes_give.pop_front().unwrap_or_else(|| Ok("fake".to_owned()))
        }

        fn look(&mut self) -> Result<String, String> {
            let mut book = self.0.borrow_mut();
            book.looks += 1;
            book.looks_give.pop_front().unwrap_or_else(|| Ok("fake".to_owned()))
        }

        fn release(&mut self) {
            self.0.borrow_mut().releases += 1;
        }
    }

    fn fake() -> (KeepAwake, Rc<RefCell<Book>>) {
        let book = Rc::new(RefCell::new(Book::default()));
        (KeepAwake::new(Box::new(Fake(book.clone()))), book)
    }

    /// Takes, looks and releases so far.
    fn asked(book: &Rc<RefCell<Book>>) -> (usize, usize, usize) {
        let book = book.borrow();
        (book.takes, book.looks, book.releases)
    }

    #[test]
    fn the_display_is_held_while_the_screens_are_on_and_let_go_when_they_go_off() {
        let (mut awake, book) = fake();
        let start = Instant::now();
        assert_eq!(awake.set(false, start), None);
        assert_eq!(asked(&book), (0, 0, 0), "off to begin with: nothing is asked");
        assert_eq!(awake.status(), (false, "no screen is on".to_owned()));
        // `sync_screens` says so every frame: the system is asked once.
        for frame in 0..100 {
            assert_eq!(awake.set(true, start + Duration::from_millis(frame * 16)), None);
        }
        assert_eq!(asked(&book), (1, 0, 0), "taken once, however many frames say so");
        assert!(awake.status().0);
        for frame in 0..100 {
            awake.set(false, start + Duration::from_secs(5) + Duration::from_millis(frame * 16));
        }
        assert_eq!(asked(&book), (1, 0, 1), "let go once");
        assert_eq!(awake.status(), (false, "no screen is on".to_owned()));
        assert_eq!(awake.next_look(), None, "nothing to look at while the screens are off");
        awake.set(true, start + Duration::from_secs(9));
        assert_eq!(asked(&book), (2, 0, 1), "on again: taken again");
    }

    #[test]
    fn the_hold_is_looked_at_only_when_a_look_is_due() {
        let (mut awake, book) = fake();
        let start = Instant::now();
        awake.set(true, start);
        assert_eq!(awake.status(), (true, "fake; asked a moment ago".to_owned()));
        assert_eq!(awake.next_look(), Some(start + FIRST_LOOK));
        assert_eq!(awake.look(start + Duration::from_secs(1)), None);
        assert_eq!(asked(&book), (1, 0, 0), "not due yet");
        assert_eq!(awake.look(start + FIRST_LOOK), None);
        assert_eq!(asked(&book), (1, 1, 0), "looked at once it was due");
        assert_eq!(awake.status(), (true, "fake".to_owned()), "found holding");
        assert_eq!(awake.next_look(), Some(start + FIRST_LOOK + LOOK_EVERY));
        awake.look(start + FIRST_LOOK + Duration::from_secs(1));
        assert_eq!(asked(&book), (1, 1, 0), "the next look is half a minute on");
        awake.look(start + FIRST_LOOK + LOOK_EVERY);
        assert_eq!(asked(&book), (1, 2, 0));
    }

    #[test]
    fn quitting_lets_go_and_nothing_takes_hold_again() {
        let (mut awake, book) = fake();
        let start = Instant::now();
        awake.set(true, start);
        awake.look(start + FIRST_LOOK);
        awake.end();
        assert_eq!(asked(&book), (1, 1, 1), "let go at the quit");
        assert_eq!(awake.next_look(), None);
        assert_eq!(awake.status(), (false, "quitting".to_owned()));
        // The screens are still brought in line while the quit waits for files to be written.
        for frame in 0..10 {
            let now = start + Duration::from_secs(3) + Duration::from_millis(frame * 16);
            awake.set(frame % 2 == 0, now);
            assert_eq!(awake.look(now + Duration::from_secs(120)), None);
        }
        awake.end();
        assert_eq!(asked(&book), (1, 1, 1), "nothing taken or let go after the quit");
    }

    #[test]
    fn quitting_with_the_screens_off_lets_nothing_go() {
        let (mut awake, book) = fake();
        awake.end();
        assert_eq!(asked(&book), (0, 0, 0));
        awake.set(true, Instant::now());
        assert_eq!(asked(&book), (0, 0, 0));
    }

    #[test]
    fn a_refusal_is_told_once_and_tried_again_a_minute_later() {
        let (mut awake, book) = fake();
        book.borrow_mut().takes_give.extend([Err("refused".to_owned()), Err("refused".to_owned())]);
        let start = Instant::now();
        assert_eq!(awake.set(true, start), Some("refused".to_owned()), "told");
        assert_eq!(awake.status(), (false, "couldn't: refused".to_owned()));
        assert_eq!(awake.next_look(), Some(start + TRY_AGAIN_AFTER));
        assert_eq!(awake.look(start + Duration::from_secs(30)), None);
        assert_eq!(book.borrow().takes, 1, "not tried again before the minute is up");
        assert_eq!(awake.look(start + TRY_AGAIN_AFTER), None, "refused again: not told again");
        assert_eq!(book.borrow().takes, 2);
        assert_eq!(awake.look(start + TRY_AGAIN_AFTER * 2), None);
        assert_eq!(book.borrow().takes, 3, "and taken the third time");
        assert_eq!(awake.status(), (true, "fake; asked a moment ago".to_owned()));
        // Each refusal let go of whatever was left (nothing, here), once.
        assert_eq!(book.borrow().releases, 2);
    }

    #[test]
    fn the_screens_going_off_stops_the_tries() {
        let (mut awake, book) = fake();
        book.borrow_mut().takes_give.push_back(Err("refused".to_owned()));
        let start = Instant::now();
        assert!(awake.set(true, start).is_some());
        awake.set(false, start + Duration::from_secs(1));
        assert_eq!(awake.next_look(), None);
        assert_eq!(awake.look(start + TRY_AGAIN_AFTER * 3), None);
        assert_eq!(book.borrow().takes, 1, "not tried again with the screens off");
        assert_eq!(awake.status(), (false, "no screen is on".to_owned()));
        // On again later: tried at once, and a failure isn't told twice in a run.
        book.borrow_mut().takes_give.push_back(Err("refused".to_owned()));
        assert_eq!(awake.set(true, start + Duration::from_secs(300)), None);
        assert_eq!(book.borrow().takes, 2);
    }

    #[test]
    fn a_hold_that_stops_is_found_at_a_look_and_taken_again() {
        let (mut awake, book) = fake();
        book.borrow_mut().looks_give.extend([Ok("fake".to_owned()), Err("it stopped".to_owned())]);
        let start = Instant::now();
        awake.set(true, start);
        assert_eq!(awake.look(start + FIRST_LOOK), None);
        let lost = start + FIRST_LOOK + LOOK_EVERY;
        assert_eq!(awake.look(lost), Some("it stopped".to_owned()), "told");
        assert_eq!(awake.status(), (false, "couldn't: it stopped".to_owned()));
        assert_eq!(asked(&book), (1, 2, 1));
        assert_eq!(awake.look(lost + TRY_AGAIN_AFTER), None);
        assert_eq!(asked(&book), (2, 2, 1), "taken again a minute later");
        assert_eq!(awake.status(), (true, "fake; asked a moment ago".to_owned()));
    }

    #[test]
    fn the_way_it_holds_may_change_while_it_holds() {
        let (mut awake, book) = fake();
        book.borrow_mut().takes_give.push_back(Ok("one and two".to_owned()));
        book.borrow_mut().looks_give.push_back(Ok("one".to_owned()));
        let start = Instant::now();
        awake.set(true, start);
        assert_eq!(awake.status(), (true, "one and two; asked a moment ago".to_owned()));
        awake.look(start + FIRST_LOOK);
        assert_eq!(awake.status(), (true, "one".to_owned()), "the other stopped at once");
    }

    #[test]
    fn screens_that_show_or_send_keep_the_display_awake() {
        let card = DeckLinkOut {
            device: "DeckLink Duo (1)".to_owned(),
            id: None,
            mode: "1080i59.94".to_owned(),
            keyer: Keyer::Off,
        };
        assert!(shows(&Placed::Fullscreen(1), false));
        assert!(shows(&Placed::Ndi("Main".to_owned()), false));
        assert!(shows(&Placed::DeckLink(card.clone()), true));
        assert!(!shows(&Placed::DeckLink(card), false), "no driver: nothing goes out");
        // A window, chosen or the one an automatic screen opens in on a laptop with no other
        // display (mirrored to a projector, say).
        assert!(shows(&Placed::Window, false));
        assert!(!shows(&Placed::Nothing(String::new()), false));
        assert!(!shows(&Placed::Nothing("Disconnected".to_owned()), true));
        assert!(any_on(&[false, true], &[true, true]));
        assert!(!any_on(&[true, false], &[false, true]), "switched off, or its kind is");
        assert!(!any_on(&[], &[]));
    }
}
