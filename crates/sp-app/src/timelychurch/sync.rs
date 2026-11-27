//! Keeping in step with TimelyChurch: the connection's state on the window's thread, and the
//! checks and sends it starts on threads of their own.
//!
//! - **Asking:** while connected, the services list is asked for soon after the app starts and
//!   then every few minutes, for the next four weeks, with the ETag of the list last seen: an
//!   unchanged list is a short "nothing new" (304), and the list kept from before stands.
//! - **Which songs are fetched:** a song is fetched when it isn't in the library yet (or was
//!   never fetched), when the list says it changed since it was fetched (if the list says when
//!   songs last changed), when a service it is in changed, and otherwise about once a day. Each
//!   at most once a check, however many services it is in. A song TimelyChurch no longer has
//!   (404) is noted as gone.
//! - **Applying** is the app's (`apply`), on the window's thread: new songs and new playlists at
//!   once; changes to songs and playlists only while nothing is on screen.
//! - **Problems:** a refused token (401, 403) means the computer was disconnected in
//!   TimelyChurch: nothing more is asked until it is connected again. "Wait" (429) is waited out.
//!   No network: the playlists and songs already made work as they are, and it asks again later.
//! - **Song uses** queued on disk are sent while connected, each with its own idempotency key;
//!   one is taken off the queue only when TimelyChurch has it.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use super::client::{self, Paired, Problem, Sent};
use super::store::{Cached, Files, Settings, UseBook};
use super::wire::{Id, PairRequest, ServicesReply, Song};
use crate::notices::{Notices, Outage};

/// How long after the app starts the first check is made.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// How often the services are asked for while connected.
const CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
/// How often a song that seems unchanged is fetched anyway.
const SONG_EVERY: i64 = 24 * 60 * 60;
/// How many days ahead services are asked for.
pub const DAYS_AHEAD: i64 = 28;
/// How soon queued song uses are tried again after they couldn't be sent.
const SEND_AGAIN: Duration = Duration::from_secs(60);

/// How the connection stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Not offered in this build (the testing switch isn't set): nothing is ever asked.
    #[default]
    Off,
    /// The kept token is being read.
    Reading,
    NotConnected,
    /// A pairing code is being exchanged.
    Pairing,
    Connected,
    /// TimelyChurch refused the token: the computer was disconnected there.
    Disconnected,
}

/// What a check found, for the app to apply.
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    /// The services list: as sent now, or the one kept when nothing changed.
    pub list: ServicesReply,
    /// The list as sent and its ETag, to keep once it is applied (`None` when it was the one
    /// kept already).
    pub keep: Option<Cached>,
    /// The list was answered now (not only the one kept): services missing from it inside the
    /// dates asked for were removed in TimelyChurch.
    pub fresh: bool,
    /// The first date asked for.
    pub from: String,
    /// Songs fetched: what was asked for, and the song.
    pub songs: Vec<((Id, Option<Id>), Song)>,
    /// Songs TimelyChurch no longer has.
    pub gone: Vec<(Id, Option<Id>)>,
    /// Songs that couldn't be fetched this time (the network, or an answer that wasn't a song).
    pub failed: Vec<(Id, Option<Id>)>,
    /// Problems with single songs, for the log.
    pub problems: Vec<String>,
}

/// What a finished thread found.
enum Answer {
    /// The token kept, read at the start.
    TokenRead(u64, Option<String>),
    /// Pairing: the address it was made with and the token kept, or why not.
    Paired(u64, String, Result<Paired, String>),
    Checked(u64, Result<Fetched, Problem>),
    /// Song uses sent (their keys), and the problem that stopped the rest, if one did.
    Sent(u64, Vec<String>, Option<Problem>),
}

/// The connection to TimelyChurch, kept on the window's thread.
pub struct Connection {
    /// The testing switch's address; `None`: nothing is offered and nothing is ever asked.
    gate: Option<String>,
    files: Option<Files>,
    pub settings: Settings,
    token: Option<String>,
    pub status: Status,
    /// The last problem, in a sentence, for the card (cleared by the next good answer).
    pub trouble: Option<String>,
    /// When the last check failed (it counts while connected with trouble), and what the
    /// notices have said of it (`tell_outage`).
    pub failed_at: Option<Instant>,
    outage: Outage,
    next_check: Option<Instant>,
    checking: bool,
    /// A check was asked for (Check Now) while one was under way.
    check_again: bool,
    /// Check Now: a check starts even while what the last one found waits to be applied.
    forced: bool,
    /// What the last check found, until the app has applied all of it.
    pub waiting: Option<Fetched>,
    /// When to try applying what waits again (it waited because something was on screen);
    /// `None`: as soon as the app looks.
    pub apply_at: Option<Instant>,
    /// The services list last applied (or kept from before), for what a song use says: the
    /// service and the key it was sung in.
    pub list: ServicesReply,
    /// The slide on screen when song uses were last looked for: its song, the playlist item it
    /// is from and the playlist in front (see `app`).
    pub looked: Option<(usize, Option<usize>, usize)>,
    pub uses: UseBook,
    sending: bool,
    next_send: Option<Instant>,
    /// Counts connections (each pairing or disconnection), so an answer that comes back for an
    /// earlier one is let go.
    generation: u64,
    /// What the window should say, once each (taken by the app).
    pub said: Vec<Said>,
    tx: Sender<Answer>,
    rx: Receiver<Answer>,
}

/// Something to say in the window's notices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    Connected(String),
    Disconnected,
}

impl Connection {
    /// The connection as it was left. With `gate` (the testing switch's address) unset, it is
    /// off: no thread, no token read, no request.
    pub fn new(gate: Option<String>, files: Option<Files>) -> Connection {
        let (tx, rx) = channel();
        let settings = files.as_ref().map(Files::settings).unwrap_or_default();
        let uses = files.as_ref().map(Files::uses).unwrap_or_default();
        let mut connection = Connection {
            gate,
            files,
            settings,
            token: None,
            status: Status::Off,
            trouble: None,
            failed_at: None,
            outage: Outage::default(),
            next_check: None,
            checking: false,
            check_again: false,
            forced: false,
            waiting: None,
            apply_at: None,
            list: ServicesReply::default(),
            looked: None,
            uses,
            sending: false,
            next_send: None,
            generation: 0,
            said: Vec::new(),
            tx,
            rx,
        };
        if connection.gate.is_none() {
            return connection;
        }
        // Disconnected in TimelyChurch before the app was last closed: still said.
        connection.status =
            if connection.settings.disconnected_there { Status::Disconnected } else { Status::NotConnected };
        connection.list = connection
            .files
            .as_ref()
            .and_then(Files::cached)
            .and_then(|cached| ServicesReply::read(&cached.body))
            .unwrap_or_default();
        if connection.settings.connected && !connection.settings.address.is_empty() {
            // The token is read on a thread of its own: macOS may first ask whether the app may
            // have it, and the window must not wait for that answer.
            let (tx, address) = (connection.tx.clone(), connection.settings.address.clone());
            let started = std::thread::Builder::new().name("timelychurch token".to_owned()).spawn(move || {
                let _ = tx.send(Answer::TokenRead(0, super::token::load(&address)));
            });
            if started.is_ok() {
                connection.status = Status::Reading;
            }
        }
        connection
    }

    /// Offered in this build: the testing switch is set.
    pub fn offered(&self) -> bool {
        self.gate.is_some()
    }

    /// The address the card offers to connect to: the one connected with before, else the
    /// testing switch's (or TimelyChurch's one address, if it has one for every church).
    pub fn suggested_address(&self) -> String {
        if !self.settings.address.is_empty() {
            return self.settings.address.clone();
        }
        self.gate.clone().or_else(|| client::ONE_ADDRESS.map(str::to_owned)).unwrap_or_default()
    }

    pub fn connected(&self) -> bool {
        self.status == Status::Connected
    }

    /// A check is under way.
    pub fn checking(&self) -> bool {
        self.checking
    }

    /// What the notices say of checks that can't reach TimelyChurch (`notices::Outage`): once an
    /// outage, after a while (nothing of TimelyChurch's on screen needs it, so never at once),
    /// and taken away when a check reaches it again. True when the notices changed.
    pub fn tell_outage(&mut self, notices: &mut Notices) -> bool {
        let failing = self.failed_at.filter(|_| self.status == Status::Connected && self.trouble.is_some());
        self.outage.follow(failing, false, || self.trouble.clone().unwrap_or_default(), notices)
    }

    /// A song as TimelyChurch last sent it, kept on this computer.
    pub fn kept_song(&self, song: &Id, arrangement: Option<&Id>) -> Option<Song> {
        self.files.as_ref()?.song(song, arrangement)
    }

    /// Exchanges a pairing code for a token, with the church at `typed` (its TimelyChurch
    /// address). The error is a sentence.
    pub fn pair(&mut self, typed: &str, code: &str) -> Result<(), String> {
        if self.gate.is_none() {
            return Err(off_words());
        }
        let address = client::address(typed)?;
        let code = code.trim().to_owned();
        if code.is_empty() || code.len() > 200 || code.chars().any(char::is_control) {
            return Err(crate::i18n::t(
                "integrations.timelychurch.no_code",
                "Type the pairing code from TimelyChurch (Setlists › Song Sync).",
            )
            .to_owned());
        }
        if self.status == Status::Pairing {
            return Err(crate::i18n::t("integrations.timelychurch.pairing_already", "Connecting already…").to_owned());
        }
        self.new_connection();
        self.status = Status::Pairing;
        self.trouble = None;
        let (tx, generation) = (self.tx.clone(), self.generation);
        std::thread::spawn(move || {
            let request = PairRequest {
                pairing_code: code,
                device_name: computer_name(),
                app_name: "SoftPresenter".to_owned(),
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
            };
            let outcome = match client::pair(&address, &request) {
                Ok(paired) => super::token::save(&address, &paired.token).map(|()| paired),
                Err(problem) => Err(problem_words(&problem)),
            };
            let _ = tx.send(Answer::Paired(generation, address, outcome));
        });
        Ok(())
    }

    /// Forgets the token here (TimelyChurch lists the computer until it is disconnected there)
    /// and stops following. The songs and playlists already made stay as they are.
    pub fn disconnect(&mut self) {
        if !self.settings.address.is_empty() {
            let address = self.settings.address.clone();
            std::thread::spawn(move || super::token::forget(&address));
        }
        self.token = None;
        self.settings.connected = false;
        self.settings.disconnected_there = false;
        if let Some(files) = &self.files {
            files.forget_cached();
        }
        self.save_settings();
        self.new_connection();
        if self.gate.is_some() {
            self.status = Status::NotConnected;
        }
        self.trouble = None;
        self.waiting = None;
    }

    /// Asks for the services now (Check Now). Says why not, in a sentence, when it can't.
    pub fn check_now(&mut self) -> Result<(), String> {
        match self.status {
            Status::Connected if self.checking => {
                self.check_again = true;
                Ok(())
            }
            Status::Connected => {
                self.next_check = Some(Instant::now());
                self.forced = true;
                Ok(())
            }
            Status::Off => Err(off_words()),
            _ => {
                Err(crate::i18n::t("integrations.timelychurch.not_connected", "TimelyChurch isn't connected.")
                    .to_owned())
            }
        }
    }

    /// Starts afresh: answers still to come for the connection before are let go.
    fn new_connection(&mut self) {
        self.generation += 1;
        self.checking = false;
        self.check_again = false;
        self.sending = false;
        self.next_check = None;
    }

    /// True when a check is due and may start: none under way, and nothing waiting to be
    /// applied (unless Check Now asked), so a long time on screen doesn't fetch the same songs
    /// again and again.
    pub fn check_due(&self) -> bool {
        self.status == Status::Connected
            && self.token.is_some()
            && !self.checking
            && (self.waiting.is_none() || self.forced)
            && self.next_check.is_some_and(|at| at <= Instant::now())
    }

    /// Starts a check on a thread of its own. `in_library` are the keys of the TimelyChurch
    /// songs in the library now; `today` is the computer's date, `YYYY-MM-DD`.
    pub fn start_check(&mut self, in_library: HashSet<String>, today: chrono::NaiveDate, now: i64) {
        let Some(token) = self.token.clone() else { return };
        self.checking = true;
        self.forced = false;
        self.next_check = None;
        let from = today.format("%Y-%m-%d").to_string();
        let to = (today + chrono::Duration::days(DAYS_AHEAD)).format("%Y-%m-%d").to_string();
        // The list kept is asked about only if it was for the same dates.
        let cached = self.files.as_ref().and_then(Files::cached).filter(|c| c.from == from && c.to == to);
        let input = CheckInput {
            base: self.settings.address.clone(),
            token,
            from,
            to,
            cached,
            settings: self.settings.clone(),
            in_library,
            now,
            files: self.files.clone(),
        };
        let (tx, generation) = (self.tx.clone(), self.generation);
        std::thread::spawn(move || {
            let _ = tx.send(Answer::Checked(generation, check(&input)));
        });
    }

    /// The app has applied what was waiting: the list and its ETag are kept now, with what was
    /// seen.
    pub fn applied(&mut self, fetched: &Fetched) {
        if let (Some(files), Some(keep)) = (&self.files, &fetched.keep) {
            let _ = files.save_cached(keep);
        }
        self.list = fetched.list.clone();
        self.waiting = None;
        self.save_settings();
    }

    pub fn save_settings(&self) {
        if let Some(files) = &self.files {
            let _ = files.save_settings(&self.settings);
        }
    }

    /// Counts a song shown in a service (see [`UseBook::count`]); it is sent soon.
    pub fn count_use(&mut self, counted_as: &str, used: super::wire::SongUse) -> bool {
        if self.gate.is_none() || !self.uses.count(counted_as, used) {
            return false;
        }
        if let Some(files) = &self.files {
            let _ = files.save_uses(&self.uses);
        }
        self.next_send = Some(Instant::now());
        true
    }

    /// Takes in finished threads' answers and starts the song uses' sending when due. Says when
    /// to look again.
    pub fn tick(&mut self) -> Option<Instant> {
        self.gate.as_ref()?;
        while let Ok(answer) = self.rx.try_recv() {
            self.take(answer);
        }
        if self.status == Status::Connected
            && !self.sending
            && !self.uses.queued.is_empty()
            && self.next_send.is_none_or(|at| at <= Instant::now())
            && let Some(token) = self.token.clone()
        {
            self.sending = true;
            self.next_send = None;
            let (tx, generation, base, queued) =
                (self.tx.clone(), self.generation, self.settings.address.clone(), self.uses.queued.clone());
            std::thread::spawn(move || {
                let mut sent = Vec::new();
                let mut stopped = None;
                for one in &queued {
                    match client::song_use(&base, &token, &one.used, &one.key) {
                        Ok(Sent::Recorded) => sent.push(one.key.clone()),
                        Ok(Sent::Refused(why)) => {
                            crate::applog::problem("timelychurch", &format!("a song use was refused: {why}"));
                            sent.push(one.key.clone());
                        }
                        Err(problem) => {
                            stopped = Some(problem);
                            break;
                        }
                    }
                }
                let _ = tx.send(Answer::Sent(generation, sent, stopped));
            });
        }
        let busy = self.checking || self.sending || matches!(self.status, Status::Pairing | Status::Reading);
        let due = [self.next_check, self.next_send.filter(|_| !self.uses.queued.is_empty())];
        let due = due.into_iter().flatten().filter(|_| self.status == Status::Connected).min();
        if busy { Some(Instant::now() + Duration::from_millis(250)) } else { due }
    }

    fn take(&mut self, answer: Answer) {
        let generation = match &answer {
            Answer::TokenRead(g, _) | Answer::Paired(g, ..) | Answer::Checked(g, _) | Answer::Sent(g, ..) => *g,
        };
        if generation != self.generation {
            return;
        }
        match answer {
            Answer::TokenRead(_, token) => match token {
                Some(token) => {
                    self.token = Some(token);
                    self.status = Status::Connected;
                    self.next_check = Some(Instant::now() + FIRST_CHECK_AFTER);
                }
                None => {
                    self.status = Status::NotConnected;
                    self.settings.connected = false;
                    self.save_settings();
                }
            },
            Answer::Paired(_, address, Ok(paired)) => {
                let same_church = super::church_part(&address) == super::church_part(&self.settings.address);
                if !same_church {
                    // Another church: nothing seen of the last one counts for this one, and the
                    // last one's songs shown are never told to this one (its ids are its own).
                    self.settings = Settings::default();
                    self.uses = UseBook::default();
                    if let Some(files) = &self.files {
                        let _ = files.save_uses(&self.uses);
                    }
                }
                if let Some(files) = &self.files {
                    files.forget_cached();
                }
                self.settings.address = address;
                self.settings.church = paired.church.clone();
                self.settings.church_id = paired.church_id.clone();
                self.settings.connected = true;
                self.settings.disconnected_there = false;
                self.save_settings();
                self.token = Some(paired.token);
                self.status = Status::Connected;
                self.trouble = None;
                self.next_check = Some(Instant::now());
                crate::applog::event("timelychurch", &format!("connected to {}", paired.church));
                self.said.push(Said::Connected(paired.church));
            }
            Answer::Paired(_, _, Err(why)) => {
                crate::applog::problem("timelychurch", &why);
                self.status = Status::NotConnected;
                self.trouble = Some(why);
            }
            Answer::Checked(_, outcome) => {
                self.checking = false;
                match outcome {
                    Ok(fetched) => {
                        self.trouble = None;
                        self.settings.last_checked = Some(chrono::Utc::now().timestamp());
                        for problem in &fetched.problems {
                            crate::applog::problem("timelychurch", problem);
                        }
                        // What a check before found and isn't applied yet: its songs not
                        // fetched again this time are kept with this check's.
                        let mut fetched = fetched;
                        if let Some(before) = self.waiting.take() {
                            for (asked, song) in before.songs {
                                if !fetched.songs.iter().any(|(have, _)| *have == asked)
                                    && !fetched.gone.contains(&asked)
                                {
                                    fetched.songs.push((asked, song));
                                }
                            }
                            for asked in before.gone {
                                if !fetched.gone.contains(&asked)
                                    && !fetched.songs.iter().any(|(have, _)| *have == asked)
                                {
                                    fetched.gone.push(asked);
                                }
                            }
                        }
                        self.waiting = Some(fetched);
                        self.apply_at = None;
                        self.save_settings();
                        self.next_check = Some(Instant::now() + CHECK_EVERY);
                    }
                    Err(problem) => self.trouble_with(problem, true),
                }
                if std::mem::take(&mut self.check_again) && self.status == Status::Connected {
                    self.next_check = Some(Instant::now());
                }
            }
            Answer::Sent(_, keys, stopped) => {
                self.sending = false;
                if !keys.is_empty() {
                    self.uses.sent(&keys);
                    if let Some(files) = &self.files {
                        let _ = files.save_uses(&self.uses);
                    }
                    crate::applog::event("timelychurch", &format!("told TimelyChurch of {} song use(s)", keys.len()));
                }
                match stopped {
                    Some(problem) => self.trouble_with(problem, false),
                    None => self.next_send = None,
                }
            }
        }
    }

    /// What a problem means for the connection.
    pub(super) fn trouble_with(&mut self, problem: Problem, checking: bool) {
        let unreachable = matches!(problem, Problem::Unreachable(_));
        match problem {
            Problem::Disconnected => {
                crate::applog::problem("timelychurch", "the token was refused: disconnected in TimelyChurch");
                let address = self.settings.address.clone();
                std::thread::spawn(move || super::token::forget(&address));
                self.token = None;
                self.settings.connected = false;
                self.settings.disconnected_there = true;
                if let Some(files) = &self.files {
                    files.forget_cached();
                }
                self.save_settings();
                self.new_connection();
                self.status = Status::Disconnected;
                self.trouble = Some(disconnected_words());
                self.said.push(Said::Disconnected);
            }
            Problem::Wait(wait) => {
                let at = Some(Instant::now() + wait);
                if checking { self.next_check = at } else { self.next_send = at }
            }
            Problem::Unreachable(why) | Problem::Other(why) => {
                crate::applog::problem("timelychurch", &why);
                if checking {
                    // In the card at once; in the notices once an outage, after a while
                    // (`tell_outage`). Why it couldn't be reached (a lookup that failed, a
                    // timeout) is the log's only.
                    self.trouble = Some(if unreachable { crate::notices::unreachable("TimelyChurch") } else { why });
                    self.failed_at = Some(Instant::now());
                    self.next_check = Some(Instant::now() + CHECK_EVERY);
                } else {
                    self.next_send = Some(Instant::now() + SEND_AGAIN);
                }
            }
        }
    }
}

/// "This computer was disconnected from TimelyChurch", as TimelyChurch asked it to be said.
pub fn disconnected_words() -> String {
    crate::i18n::t("integrations.timelychurch.disconnected", "This computer was disconnected from TimelyChurch.")
        .to_owned()
}

fn off_words() -> String {
    crate::i18n::t("integrations.timelychurch.off", "TimelyChurch isn't offered in this build yet.").to_owned()
}

/// A problem in a sentence.
fn problem_words(problem: &Problem) -> String {
    match problem {
        Problem::Disconnected => disconnected_words(),
        Problem::Wait(_) => {
            crate::i18n::t("integrations.timelychurch.wait", "TimelyChurch asked to wait a moment; try again shortly.")
                .to_owned()
        }
        Problem::Unreachable(why) | Problem::Other(why) => why.clone(),
    }
}

/// The computer's name, as TimelyChurch's list of devices shows it: the name its owner gave it.
fn computer_name() -> String {
    let named = if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        std::env::var("COMPUTERNAME").ok().or_else(|| std::fs::read_to_string("/etc/hostname").ok())
    };
    let name: String = named.unwrap_or_default().chars().filter(|c| !c.is_control()).take(100).collect();
    let name = name.trim();
    if name.is_empty() { "SoftPresenter".to_owned() } else { name.to_owned() }
}

/// What a check works from.
struct CheckInput {
    base: String,
    token: String,
    from: String,
    to: String,
    cached: Option<Cached>,
    settings: Settings,
    in_library: HashSet<String>,
    now: i64,
    files: Option<Files>,
}

/// One check: the services list, then each song that needs fetching. Runs on a thread of its
/// own.
fn check(input: &CheckInput) -> Result<Fetched, Problem> {
    let etag = input.cached.as_ref().and_then(|cached| cached.etag.as_deref());
    let (list, keep, fresh) = match client::services(&input.base, &input.token, &input.from, &input.to, etag)? {
        client::Services::Unchanged => match input.cached.as_ref().and_then(|c| ServicesReply::read(&c.body)) {
            Some(list) => (list, None, false),
            // "Nothing changed" with nothing kept to stand for it: ask for the whole list.
            None => match client::services(&input.base, &input.token, &input.from, &input.to, None)? {
                client::Services::Fresh { list, body, etag } => {
                    let keep = Cached { etag, from: input.from.clone(), to: input.to.clone(), body };
                    (list, Some(keep), true)
                }
                client::Services::Unchanged => (ServicesReply::default(), None, false),
            },
        },
        client::Services::Fresh { list, body, etag } => {
            let keep = Cached { etag, from: input.from.clone(), to: input.to.clone(), body };
            (list, Some(keep), true)
        }
    };
    let wanted = songs_to_fetch(&list, &input.settings, &input.in_library, input.now);
    let mut fetched = Fetched { list, keep, fresh, from: input.from.clone(), ..Fetched::default() };
    for (song, arrangement) in wanted {
        match client::song(&input.base, &input.token, &song, arrangement.as_ref()) {
            Ok(Some((made, body))) => {
                if let Some(files) = &input.files {
                    files.save_song(&song, arrangement.as_ref(), &body);
                }
                fetched.songs.push(((song, arrangement), made));
            }
            Ok(None) => fetched.gone.push((song, arrangement)),
            // A refused token or a "wait" stops the check: what was fetched is let go.
            Err(problem @ (Problem::Disconnected | Problem::Wait(_))) => return Err(problem),
            Err(Problem::Unreachable(why) | Problem::Other(why)) => {
                fetched.problems.push(why);
                fetched.failed.push((song, arrangement));
            }
        }
    }
    Ok(fetched)
}

/// The songs a check fetches (see the module's notes), each once, in the order the services ask
/// for them.
pub fn songs_to_fetch(
    list: &ServicesReply,
    settings: &Settings,
    in_library: &HashSet<String>,
    now: i64,
) -> Vec<(Id, Option<Id>)> {
    let mut wanted: Vec<(Id, Option<Id>)> = Vec::new();
    let church = settings.address.as_str();
    for (service, setlist) in super::to_follow(&list.services) {
        let key = super::service_key(church, &super::Made::of(service));
        let seen = settings.services.iter().find(|seen| seen.key == key);
        let service_changed = seen.is_none_or(|seen| seen.updated_at != service.updated_at)
            || setlist.is_some_and(|setlist| {
                let key = super::service_key(church, &super::Made::of(setlist));
                settings.services.iter().find(|seen| seen.key == key).is_none_or(|s| s.updated_at != setlist.updated_at)
            });
        let mut items: Vec<&super::wire::Item> = service.items.iter().filter(|item| item.is_song()).collect();
        items.sort_by_key(|item| item.position);
        let mut joined: Vec<&super::wire::Item> =
            setlist.into_iter().flat_map(|s| &s.items).filter(|item| item.is_song()).collect();
        joined.sort_by_key(|item| item.position);
        for item in items.into_iter().chain(joined) {
            let Some(song) = item.song_id.clone().filter(Id::is_safe) else { continue };
            let arrangement = item.arrangement_id.clone().filter(Id::is_safe);
            let asked = (song, arrangement);
            if wanted.contains(&asked) {
                continue;
            }
            let fetch = match settings.seen_song(&asked.0, asked.1.as_ref()) {
                None => true,
                Some(seen) => {
                    let stale = now - seen.fetched_at >= SONG_EVERY;
                    let changed = item.song_updated_at.is_some() && item.song_updated_at != seen.updated_at;
                    let missing = !seen.gone && !in_library.contains(&seen.key);
                    stale || changed || missing || (service_changed && !seen.gone)
                }
            };
            if fetch {
                wanted.push(asked);
            }
        }
    }
    wanted
}
