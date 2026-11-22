//! A DeckLink output: a thread of its own that opens the device, keeps it fed with frames on
//! its own clock (see `player`), turns each picture it is handed into the card's format and
//! size, and opens the device again if it stops or goes away. Handing it a picture never
//! waits, so the thread that draws the screens is never held up by the card.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::convert::{Layout, PixelFormat, black, to_frame};
use crate::player::{Card, Counts, Player};
use crate::{Alpha, Keying, Mode};

/// Where a screen goes: a device, by its name and the driver's persistent id for it, the mode
/// it plays in, by SoftPresenter's name for it ("1080i59.94"), and how it uses the keyer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub device: String,
    pub id: Option<i64>,
    pub mode: String,
    pub keying: Keying,
}

/// What an output is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Opening the device.
    Starting,
    /// Playing out: the mode and the keyer, in words ("1080i59.94", "1080i50, fill and key").
    Playing(String),
    /// Not playing, and trying again every couple of seconds: the device isn't connected, is
    /// in use, or stopped. Why, in plain words.
    Waiting(String),
}

/// What everyone else can see of an output: its state and its counts of frames.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    played: AtomicU64,
    late: AtomicU64,
    dropped: AtomicU64,
}

impl Shared {
    fn state(&self) -> State {
        self.state
            .lock()
            .map_or_else(|_| State::Waiting("The DeckLink output stopped.".to_owned()), |state| state.clone())
    }

    fn set_state(&self, now: State) {
        if let Ok(mut state) = self.state.lock() {
            *state = now;
        }
    }

    fn counts(&self) -> Counts {
        Counts {
            played: self.played.load(Ordering::Relaxed),
            late: self.late.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
        }
    }

    /// Adds what a run of playing has counted since it last said.
    fn add(&self, more: Counts) {
        self.played.fetch_add(more.played, Ordering::Relaxed);
        self.late.fetch_add(more.late, Ordering::Relaxed);
        self.dropped.fetch_add(more.dropped, Ordering::Relaxed);
    }
}

/// An output's state and counts, from another thread.
#[derive(Clone, Debug)]
pub struct Watch(Arc<Shared>);

impl Watch {
    pub fn state(&self) -> State {
        self.0.state()
    }

    /// Frames played, late and dropped since the output started.
    pub fn counts(&self) -> Counts {
        self.0.counts()
    }
}

/// Said to the app's log: a line, and whether it is a problem.
pub type Log = Arc<dyn Fn(&str, bool) + Send + Sync>;

/// A picture handed over.
struct Picture {
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
    alpha: Alpha,
}

enum Told {
    Picture(Picture),
    Change(Wanted),
}

/// A running DeckLink output. Dropping it stops the output: the thread closes the device on
/// its own, without being waited for.
pub struct DeckLinkOutput {
    tx: Option<Sender<Told>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl DeckLinkOutput {
    /// Starts an output on its own thread. Never waits for the driver: how opening went is
    /// in `state`. `log` is told what happens (on the output's thread), in plain words.
    pub fn start(wanted: Wanted, log: Log) -> Result<DeckLinkOutput, String> {
        if crate::pretend::active() {
            return spawn(wanted, crate::pretend::PretendOpener, log);
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return spawn(wanted, RealOpener, log);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        Err(crate::Driver::Unsupported.in_words())
    }

    /// Hands over the picture to play from now on: tightly packed RGBA rows, which other
    /// outputs may be sending too, and how to read their alpha. Never waits, never copies.
    /// A picture whose size and pixel count disagree is ignored.
    pub fn show_shared(&self, width: u32, height: u32, rgba: Arc<Vec<u8>>, alpha: Alpha) {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(Told::Picture(Picture { width, height, rgba, alpha }));
        }
    }

    /// Plays somewhere else, or another way: the device is closed and opened again by this
    /// output's own thread, so it is never found still in use by itself. Never waits.
    pub fn change(&self, wanted: Wanted) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Told::Change(wanted));
        }
    }

    pub fn state(&self) -> State {
        self.shared.state()
    }

    pub fn counts(&self) -> Counts {
        self.shared.counts()
    }

    pub fn watch(&self) -> Watch {
        Watch(self.shared.clone())
    }
}

impl Drop for DeckLinkOutput {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop, and it closes the device itself. It isn't
        // waited for: the thread dropping this may be the one drawing the screens.
        self.tx = None;
        drop(self.thread.take());
    }
}

/// Opens a device for an output: the real driver, or a stand-in.
pub(crate) trait Opener: Send + 'static {
    type Card: Card;
    fn open(&mut self, wanted: &Wanted) -> Result<Opened<Self::Card>, String>;
}

/// A device opened, playing in a mode, its frames laid out so.
pub(crate) struct Opened<C> {
    pub card: C,
    pub device: String,
    pub mode: Mode,
    pub layout: Layout,
    pub keyed: Keying,
}

fn spawn<O: Opener>(wanted: Wanted, opener: O, log: Log) -> Result<DeckLinkOutput, String> {
    let (tx, rx) = channel();
    let shared = Arc::new(Shared {
        state: Mutex::new(State::Starting),
        played: AtomicU64::new(0),
        late: AtomicU64::new(0),
        dropped: AtomicU64::new(0),
    });
    let theirs = shared.clone();
    let thread = std::thread::Builder::new()
        .name("decklink".to_owned())
        .spawn(move || run(opener, wanted, &rx, &theirs, &log))
        .map_err(|e| format!("Could not start the DeckLink output: {e}"))?;
    Ok(DeckLinkOutput { tx: Some(tx), shared, thread: Some(thread) })
}

/// How long after a failure the device is opened again.
const TRY_AGAIN: Duration = Duration::from_secs(2);

/// How a run of playing ended.
enum Ended {
    /// The output was dropped.
    Closed,
    /// It is to play somewhere else or another way.
    Changed,
    /// The device stopped or went wrong: why, in plain words.
    Trouble(String),
}

fn run<O: Opener>(mut opener: O, mut wanted: Wanted, rx: &Receiver<Told>, shared: &Shared, log: &Log) {
    let mut picture: Option<Picture> = None;
    // The last problem said, so that one that lasts is said once.
    let mut said: Option<String> = None;
    loop {
        let trouble = match opener.open(&wanted) {
            Ok(mut opened) => {
                let what = playing_in_words(&opened);
                log(&format!("{} plays out at {what}, in {}", opened.device, opened.layout.format.in_words()), false);
                said = None;
                shared.set_state(State::Playing(what));
                match play(&mut opened, &mut wanted, rx, &mut picture, shared, log) {
                    Ended::Closed => return,
                    Ended::Changed => continue,
                    Ended::Trouble(why) => why,
                }
            }
            Err(why) => why,
        };
        shared.set_state(State::Waiting(trouble.clone()));
        if said.as_deref() != Some(trouble.as_str()) {
            log(&format!("{trouble} Trying again every {} seconds.", TRY_AGAIN.as_secs()), true);
            said = Some(trouble);
        }
        // Meanwhile the newest picture is kept, to play at once when the device is back.
        let until = Instant::now() + TRY_AGAIN;
        loop {
            match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(Told::Picture(newer)) => picture = Some(newer),
                Ok(Told::Change(changed)) => {
                    if changed != wanted {
                        wanted = changed;
                        said = None;
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

fn playing_in_words<C>(opened: &Opened<C>) -> String {
    let keyer = match opened.keyed {
        Keying::Off => String::new(),
        Keying::External => ", fill and key".to_owned(),
        Keying::Internal => ", keyed over its input".to_owned(),
    };
    format!("{}{keyer}", opened.mode.name)
}

/// How long the card may go without giving a frame back before it counts as stopped: ten
/// frames, and a second at least.
fn stalled_after(frame: Duration) -> Duration {
    (frame * 10).max(Duration::from_secs(1))
}

/// Plays until the output is dropped or changed, or the card stops.
fn play<C: Card>(
    opened: &mut Opened<C>,
    wanted: &mut Wanted,
    rx: &Receiver<Told>,
    picture: &mut Option<Picture>,
    shared: &Shared,
    log: &Log,
) -> Ended {
    let Opened { card, device, mode, layout, keyed } = opened;
    let (layout, keyed) = (*layout, *keyed != Keying::Off);
    let convert =
        |picture: &Picture| to_frame(&picture.rgba, picture.width, picture.height, layout, picture.alpha, keyed);
    let mut player: Player<C::Frame> = Player::new(black(layout), mode.frame_duration, mode.time_scale);
    if let Some(picture) = picture.as_ref() {
        player.set_picture(convert(picture));
    }
    if let Err(e) = player.top_up(card) {
        return Ended::Trouble(format!("{device} would not start playing: {e}."));
    }
    let frame = player.frame_time();
    let wait = frame.min(Duration::from_millis(20));
    let mut given_back = Instant::now();
    let mut said = Counts::default();
    let mut minute = Instant::now();
    let mut in_minute = Counts::default();
    loop {
        let (done, halt) = card.wait(wait);
        if !done.is_empty() {
            given_back = Instant::now();
        }
        for (key, how) in done {
            player.finished(key, how, card);
        }
        let counted = player.counts;
        let more = Counts {
            played: counted.played - said.played,
            late: counted.late - said.late,
            dropped: counted.dropped - said.dropped,
        };
        shared.add(more);
        in_minute = Counts {
            played: in_minute.played + more.played,
            late: in_minute.late + more.late,
            dropped: in_minute.dropped + more.dropped,
        };
        said = counted;
        if halt.profile {
            return Ended::Trouble(format!(
                "{device} stopped: the card's profile changed (in Blackmagic's Desktop Video Setup)."
            ));
        }
        if halt.stopped {
            return Ended::Trouble(format!("{device} stopped playing."));
        }
        // What the screens' thread has handed over meanwhile: only the newest picture counts.
        let mut newest = None;
        loop {
            match rx.try_recv() {
                Ok(Told::Picture(newer)) => newest = Some(newer),
                Ok(Told::Change(changed)) => {
                    if changed != *wanted {
                        *wanted = changed;
                        return Ended::Changed;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ended::Closed,
            }
        }
        // The card is kept fed first, with the picture it has, before the new picture is
        // turned into a frame (which takes a few milliseconds).
        if let Err(e) = player.top_up(card) {
            return Ended::Trouble(format!("{device} refused a frame: {e}."));
        }
        if let Some(newer) = newest {
            player.set_picture(convert(&newer));
            *picture = Some(newer);
            if let Err(e) = player.top_up(card) {
                return Ended::Trouble(format!("{device} refused a frame: {e}."));
            }
        }
        if given_back.elapsed() > stalled_after(frame) {
            return Ended::Trouble(format!("{device} stopped taking frames."));
        }
        if minute.elapsed() >= Duration::from_secs(60) {
            log(
                &format!(
                    "{device} at {}: {} frames played, {} late, {} dropped in the last minute",
                    mode.name, in_minute.played, in_minute.late, in_minute.dropped
                ),
                in_minute.late > 0 || in_minute.dropped > 0,
            );
            minute = Instant::now();
            in_minute = Counts::default();
        }
    }
}

/// The real driver.
#[cfg(any(target_os = "macos", target_os = "linux"))]
struct RealOpener;

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Opener for RealOpener {
    type Card = RealCard;

    fn open(&mut self, wanted: &Wanted) -> Result<Opened<RealCard>, String> {
        let found = crate::list_now();
        if !matches!(found.driver, crate::Driver::Ready(_)) {
            return Err(found.driver.in_words());
        }
        let device =
            found.find(wanted.id, &wanted.device).ok_or_else(|| format!("{} isn't connected.", wanted.device))?;
        let (mode, format) = choose(device, wanted)?;
        let name = device.name.clone();
        let mut out = crate::ffi::Out::open(device.id, &device.name).map_err(|error| match error {
            None => format!("{name} isn't connected."),
            Some(code) => format!("{name} could not be opened: {}.", crate::ffi::in_words(code)),
        })?;
        out.enable(mode.code)
            .map_err(|code| format!("{name} could not play {}: {}.", mode.name, crate::ffi::in_words(code)))?;
        if wanted.keying != Keying::Off {
            out.keyer(true, wanted.keying == Keying::External)
                .map_err(|code| format!("{name}'s keyer could not be turned on: {}.", crate::ffi::in_words(code)))?;
        }
        let row_bytes = out.row_bytes(format.code(), mode.width).unwrap_or_else(|| format.row_bytes(mode.width));
        let layout = Layout { width: mode.width, height: mode.height, format, row_bytes };
        Ok(Opened { card: RealCard { out, layout }, device: name, mode, layout, keyed: wanted.keying })
    }
}

/// The mode and pixel format a device is to play in, as wanted: BGRA where the mode takes it
/// (and always to key, which needs the alpha), else YUV. The error, in plain words, is why it
/// can't play so.
pub(crate) fn choose(device: &crate::Device, wanted: &Wanted) -> Result<(Mode, PixelFormat), String> {
    if !device.usable() {
        return Err(format!("{} can't play out in its card's present profile.", device.name));
    }
    let mode = device.mode(&wanted.mode).ok_or_else(|| format!("{} has no mode {}.", device.name, wanted.mode))?;
    let can_key = match wanted.keying {
        Keying::Off => true,
        Keying::External => device.external_keying,
        Keying::Internal => device.internal_keying,
    };
    if !can_key {
        return Err(format!("{} can't do {} keying in its card's present profile.", device.name, wanted.keying.name()));
    }
    if wanted.keying != Keying::Off && !mode.keying {
        return Err(format!("{} can't key in {}.", device.name, mode.name));
    }
    let format = if wanted.keying != Keying::Off || mode.bgra { PixelFormat::Bgra } else { PixelFormat::Yuv };
    Ok((mode.clone(), format))
}

/// The real card, through the bridge.
#[cfg(any(target_os = "macos", target_os = "linux"))]
struct RealCard {
    out: crate::ffi::Out,
    layout: Layout,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Card for RealCard {
    type Frame = crate::ffi::Frame;

    fn new_frame(&mut self) -> Result<crate::ffi::Frame, String> {
        let Layout { width, height, format, row_bytes } = self.layout;
        self.out
            .new_frame(width, height, row_bytes, format.code())
            .ok_or_else(|| "the card could not make a frame".to_owned())
    }

    fn write(&mut self, frame: &mut crate::ffi::Frame, bytes: &[u8]) -> Result<(), String> {
        crate::ffi::Out::write(frame, bytes).map_err(crate::ffi::in_words)
    }

    fn key(frame: &crate::ffi::Frame) -> usize {
        frame.key()
    }

    fn schedule(&mut self, frame: &crate::ffi::Frame, at: i64, duration: i64, scale: i64) -> Result<(), String> {
        self.out.schedule(frame, at, duration, scale).map_err(crate::ffi::in_words)
    }

    fn start(&mut self, at: i64, scale: i64) -> Result<(), String> {
        self.out.start(at, scale).map_err(crate::ffi::in_words)
    }

    fn stream_time(&mut self, scale: i64) -> Option<i64> {
        self.out.stream_time(scale)
    }

    fn wait(&mut self, timeout: Duration) -> (Vec<(usize, crate::player::Finished)>, crate::player::Halt) {
        let (done, flags) = self.out.wait(timeout);
        let halt = crate::player::Halt {
            stopped: flags & crate::ffi::STOPPED != 0,
            profile: flags & (crate::ffi::PROFILE_CHANGING | crate::ffi::PROFILE_CHANGED) != 0,
        };
        (done, halt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pretend::SimCard;

    /// Opens pretend devices, as the test says: which are there, and what each open gives.
    #[derive(Clone, Default)]
    struct Stand {
        absent: Arc<Mutex<bool>>,
        opened: Arc<Mutex<Vec<(String, String)>>>,
        cards: Arc<Mutex<Vec<crate::pretend::SimControl>>>,
    }

    impl Opener for Stand {
        type Card = SimCard;
        fn open(&mut self, wanted: &Wanted) -> Result<Opened<SimCard>, String> {
            if *self.absent.lock().unwrap() {
                return Err(format!("{} isn't connected.", wanted.device));
            }
            let devices = crate::pretend::devices();
            let device = devices.iter().find(|device| device.name == wanted.device).ok_or("no such device")?;
            let (mode, format) = choose(device, wanted)?;
            let layout =
                Layout { width: mode.width, height: mode.height, format, row_bytes: format.row_bytes(mode.width) };
            let card = SimCard::new();
            self.cards.lock().unwrap().push(card.control());
            self.opened.lock().unwrap().push((wanted.device.clone(), mode.name.clone()));
            Ok(Opened { card, device: device.name.clone(), mode, layout, keyed: wanted.keying })
        }
    }

    fn wanted(device: &str, mode: &str) -> Wanted {
        Wanted { device: device.to_owned(), id: None, mode: mode.to_owned(), keying: Keying::Off }
    }

    fn quiet() -> Log {
        Arc::new(|_: &str, _| {})
    }

    /// Waits up to five seconds for `ready`.
    fn soon(ready: impl Fn() -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if ready() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        ready()
    }

    #[test]
    fn a_picture_handed_over_is_played_out_converted_and_frames_keep_coming() {
        let stand = Stand::default();
        let output = spawn(wanted("Pretend DeckLink (1)", "720p59.94"), stand.clone(), quiet()).unwrap();
        assert!(soon(|| matches!(output.state(), State::Playing(_))), "{:?}", output.state());
        assert_eq!(output.state(), State::Playing("720p59.94".to_owned()));
        let red = Arc::new([255u8, 0, 0, 255].repeat(1920 * 1080));
        output.show_shared(1920, 1080, red, Alpha::Opaque);
        let card = stand.cards.lock().unwrap()[0].clone();
        assert!(
            soon(|| card.last_written().is_some_and(|(_, start)| start[..4] == [0, 0, 255, 255])),
            "scaled to 720p, in BGRA"
        );
        assert_eq!(card.last_written().map(|(len, _)| len), Some(1280 * 720 * 4));
        assert!(soon(|| output.counts().played >= 10), "the card's clock keeps taking frames: {:?}", output.counts());
        assert_eq!(output.counts().dropped, 0);
    }

    #[test]
    fn handing_over_a_picture_never_waits() {
        let output = spawn(wanted("Pretend DeckLink (1)", "1080i59.94"), Stand::default(), quiet()).unwrap();
        let picture = Arc::new(vec![9u8; 1920 * 1080 * 4]);
        let started = Instant::now();
        for _ in 0..100 {
            output.show_shared(1920, 1080, picture.clone(), Alpha::Opaque);
        }
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn a_change_of_mode_is_made_by_the_same_thread_closing_and_opening_again() {
        let stand = Stand::default();
        let output = spawn(wanted("Pretend DeckLink (1)", "1080i59.94"), stand.clone(), quiet()).unwrap();
        assert!(soon(|| matches!(output.state(), State::Playing(_))));
        output.change(wanted("Pretend DeckLink (1)", "1080p25"));
        assert!(soon(|| output.state() == State::Playing("1080p25".to_owned())), "{:?}", output.state());
        let opened = stand.opened.lock().unwrap().clone();
        assert_eq!(opened.iter().map(|(_, mode)| mode.as_str()).collect::<Vec<_>>(), ["1080i59.94", "1080p25"]);
        assert!(stand.cards.lock().unwrap()[0].closed(), "the first card was let go before the second was opened");
        // The same again changes nothing.
        output.change(wanted("Pretend DeckLink (1)", "1080p25"));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(stand.opened.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_device_that_isnt_there_is_waited_for_and_played_on_when_it_comes() {
        let stand = Stand::default();
        *stand.absent.lock().unwrap() = true;
        let (said_tx, said) = channel();
        let log: Log = Arc::new(move |line: &str, problem| {
            let _ = said_tx.send((line.to_owned(), problem));
        });
        let output = spawn(wanted("Pretend DeckLink (2)", "1080i50"), stand.clone(), log).unwrap();
        assert!(soon(|| matches!(output.state(), State::Waiting(_))));
        assert!(matches!(output.state(), State::Waiting(why) if why.contains("isn't connected")));
        let (line, problem) = said.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(problem && line.contains("Trying again"), "{line}");
        // A picture handed over meanwhile is played as soon as the device is there.
        output.show_shared(1, 1, Arc::new(vec![0, 255, 0, 255]), Alpha::Opaque);
        *stand.absent.lock().unwrap() = false;
        assert!(soon(|| matches!(output.state(), State::Playing(_))), "{:?}", output.state());
        let card = stand.cards.lock().unwrap()[0].clone();
        assert!(soon(|| card.last_written().is_some_and(|(_, start)| start[..4] == [0, 255, 0, 255])));
    }

    #[test]
    fn a_card_that_stops_taking_frames_is_opened_again() {
        let stand = Stand::default();
        let output = spawn(wanted("Pretend DeckLink (1)", "1080p50"), stand.clone(), quiet()).unwrap();
        assert!(soon(|| matches!(output.state(), State::Playing(_))));
        stand.cards.lock().unwrap()[0].stall();
        assert!(soon(|| stand.opened.lock().unwrap().len() >= 2), "opened again after the stall");
        assert!(soon(|| matches!(output.state(), State::Playing(_))));
    }

    #[test]
    fn a_card_whose_profile_changes_is_closed_and_opened_again() {
        let stand = Stand::default();
        let output = spawn(wanted("Pretend DeckLink (1)", "1080p50"), stand.clone(), quiet()).unwrap();
        assert!(soon(|| matches!(output.state(), State::Playing(_))));
        stand.cards.lock().unwrap()[0].change_profile();
        assert!(soon(|| stand.opened.lock().unwrap().len() >= 2));
        assert!(stand.cards.lock().unwrap()[0].closed());
    }

    #[test]
    fn dropping_the_output_lets_the_card_go() {
        let stand = Stand::default();
        let output = spawn(wanted("Pretend DeckLink (1)", "1080p50"), stand.clone(), quiet()).unwrap();
        assert!(soon(|| matches!(output.state(), State::Playing(_))));
        let started = Instant::now();
        drop(output);
        assert!(started.elapsed() < Duration::from_millis(50), "never waited for");
        let card = stand.cards.lock().unwrap()[0].clone();
        assert!(soon(|| card.closed()));
    }

    #[test]
    fn a_mode_or_keying_the_device_cant_do_is_refused_in_plain_words() {
        let devices = crate::pretend::devices();
        let plain = &devices[0];
        let keyer = devices.iter().find(|device| device.external_keying).unwrap();
        assert!(choose(plain, &wanted(&plain.name, "1080i47")).unwrap_err().contains("has no mode 1080i47"));
        let mut keyed = wanted(&plain.name, "1080i59.94");
        keyed.keying = Keying::External;
        assert!(choose(plain, &keyed).unwrap_err().contains("can't do external keying"));
        keyed.device = keyer.name.clone();
        assert_eq!(choose(keyer, &keyed).unwrap().1, PixelFormat::Bgra);
        let yuv_only = crate::Device {
            modes: plain.modes.iter().map(|mode| Mode { bgra: false, ..mode.clone() }).collect(),
            ..plain.clone()
        };
        assert_eq!(choose(&yuv_only, &wanted(&plain.name, "1080i59.94")).unwrap().1, PixelFormat::Yuv);
    }

    #[test]
    fn a_stall_is_ten_frames_and_a_second_at_least() {
        assert_eq!(stalled_after(Duration::from_millis(40)), Duration::from_secs(1));
        assert_eq!(stalled_after(Duration::from_millis(200)), Duration::from_secs(2));
    }
}
