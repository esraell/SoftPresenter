//! Sends a screen's picture over the network as an NDI® source.
//!
//! NDI® is a registered trademark of Vizrt NDI AB. The NDI runtime is not part of SoftPresenter
//! and is not needed to build it or to start it: it is looked for on the computer when the first
//! NDI source starts, on that source's own thread. Without it, the source says NDI isn't
//! installed and where to get it, and everything else carries on. On Linux, where NDI Tools
//! doesn't exist, the app can download NDI's runtime into a folder of its own
//! ([`set_own_folder`]), which is looked in too, and found without a restart. See
//! `docs/licences/ndi.md`.
//!
//! The work happens on each source's own thread, so handing over a picture never waits on NDI
//! or the network.
//!
//! # Sending only while someone is watching
//!
//! About once a second, each source asks NDI how many receivers are connected to it. While
//! there are none, it sends nothing and keeps no picture: the one it had is let go when the
//! last receiver leaves, and any handed over meanwhile is dropped. A source whose NDI runtime
//! can't count receivers keeps its picture and sends all the time, as before.
//!
//! A receiver that connects must never be shown a slide that has gone. Two things could show
//! it one, and neither does:
//!
//! - The source itself sends nothing from before the receiver came. The receiver gets the
//!   first picture handed over after it connected, and it is for whoever draws the pictures to
//!   hand one over then (see `start_with_wake` below).
//! - NDI keeps the last picture a source sent, and gives it to a receiver the moment it
//!   connects, before the source can know of it. While that picture is still what is on the
//!   screens, that is welcome: the receiver has its picture at once. When it no longer is
//!   (something changed, and nobody was watching to be sent it), the source is told to
//!   [`NdiOutput::forget`], and sends one black picture to take its place (all clear, for a
//!   see-through screen: [`NdiOutput::forget_as`]). A receiver that connects then sees black
//!   (or nothing, over its camera) until its picture is drawn, within about a second.
//!
//! What the thread drawing the screens needs, none of which ever makes it wait:
//!
//! - [`NdiOutput::receivers`] (or [`Watch::receivers`]): `Some(0)` means nobody is watching, so
//!   the picture for that source need not be drawn or read back from the GPU at all. `None`
//!   means the count isn't known (the source is still starting, or its runtime can't count):
//!   draw for it as usual.
//! - [`NdiOutput::forget`]: said whenever the newest picture is not going to reach a source:
//!   for a source nobody is watching, when a picture that may differ from the last is not
//!   drawn for it; and for one somebody is watching, when its picture was lost on the way. The
//!   first is sent black in place of what it last sent. The second goes on sending the picture
//!   it has (its receivers are looking at it, and nothing newer exists), and is sent black
//!   only if they all leave before a newer picture comes.
//! - [`NdiOutput::start_with_wake`]: `wake` is called each time the count of receivers is
//!   found to have changed, the first time it is known included. It runs on the source's own
//!   thread, after the new count can be read, and should only pass the word on (send a message
//!   on a channel, say) and return. Whoever it wakes shows the new count, and looks at
//!   `arrivals` to see whether to draw. A source started with [`NdiOutput::start`] has nobody
//!   to wake, so a receiver that connects to it waits for the next picture handed over.
//! - [`NdiOutput::arrivals`]: how many times receivers have come to the source after a spell
//!   with none. Each time, the source has no picture and waits to be handed a fresh one.
//!   Whoever draws remembers the number it last drew for and draws again when it has moved
//!   on. The count of receivers can't say this by itself: read after the receivers left and
//!   others came, it is the same number.
//! - [`NdiOutput::show_shared`]: one picture for several sources, handed to each without
//!   copying it.
//!
//! # Sound
//!
//! A source sends sound beside its pictures: [`NdiOutput::sound`] gives a [`Sound`] that any
//! thread may hand blocks of samples to ([`Sound::play`]), stereo at 48,000 samples a second.
//! They go out at once, on the thread that hands them over, never on the one sending the
//! pictures (which waits for each picture's time, and would hold the sound up). Each block
//! carries the time its sound is heard, by the computer's clock, which NDI makes the
//! pictures' timecodes from too; so a receiver plays them in step, whether it goes by the
//! timecodes or by when each arrives, as long as each block is handed over when its sound
//! is due. Sound, like pictures, is only for
//! somebody watching: [`Sound::wanted`] says whether anyone is.
//!
//! A source counts again, there and then, when it is handed a picture or told to forget while
//! it believes nobody is watching: a receiver may have connected since the last count. If one
//! has, the picture is sent to it rather than dropped, and no black picture is sent over what
//! it is looking at.

#[allow(unsafe_code)]
mod runtime;

mod receive;
pub use receive::{Finder, FinderState, Incoming, IncomingState, Video};

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use runtime::{NOT_INSTALLED, opened_from, set_own_folder};

/// NDI's website, linked wherever NDI is chosen (where NDI Tools is downloaded on a Mac or on
/// Windows).
pub const DOWNLOAD_URL: &str = "https://ndi.video/";

/// How often the current picture is sent again while nothing changes. A receiver that joins
/// late, or a mixer that expects a steady source, gets a picture within this time. Each send is
/// compressed again by NDI, so this is kept slow: every 100 ms, two idle sources took about a
/// tenth of the computer's time; every second, about a fortieth.
const RESEND_EVERY: Duration = Duration::from_millis(1000);

/// How often a source asks NDI how many receivers it has. A receiver that connects to a source
/// nobody was watching gets its first picture within about this time.
const COUNT_EVERY: Duration = Duration::from_millis(1000);

/// The pictures a second a source says it sends, as the fraction NDI and video equipment
/// give them by (29.97 is 30000/1001). NDI paces the source's sending to it (each send waits
/// for its picture's time), and a receiver that plays out at the same rate shows every picture
/// once: one that plays at another (a 59.94 Hz decoder given 30 a second) drops or repeats one
/// every so often, which is seen as a small pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub n: u32,
    pub d: u32,
}

impl Rate {
    pub const R29_97: Rate = Rate { n: 30_000, d: 1001 };
    pub const R30: Rate = Rate { n: 30, d: 1 };
    pub const R59_94: Rate = Rate { n: 60_000, d: 1001 };
    pub const R60: Rate = Rate { n: 60, d: 1 };
    pub const R25: Rate = Rate { n: 25, d: 1 };
    pub const R50: Rate = Rate { n: 50, d: 1 };

    /// One picture's time at this rate.
    pub fn frame(self) -> Duration {
        Duration::from_nanos(1_000_000_000 * u64::from(self.d.max(1)) / u64::from(self.n.max(1)))
    }

    /// The rate as it is said: "29.97", "30", "59.94".
    pub fn name(self) -> String {
        if self.d == 1 { self.n.to_string() } else { format!("{:.2}", f64::from(self.n) / f64::from(self.d)) }
    }
}

/// 29.97: what most video equipment where the mains are 60 Hz plays at.
impl Default for Rate {
    fn default() -> Rate {
        Rate::R29_97
    }
}

/// One picture's time at thirty a second: what the screens' thread counts a long step by,
/// whatever each source's own rate.
pub const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 30);

/// How many times more, and how long apart, a source that NDI would not create is tried: two
/// seconds in all, for the source of the same name before it to be gone.
const START_TRIES: u32 = 20;
const START_AGAIN_AFTER: Duration = Duration::from_millis(100);

/// How often a source says in the log how its pictures went out.
const MINUTE: Duration = Duration::from_secs(60);

/// The slowest the pictures' own pace is taken to be (see `Pace::frame`): fifteen a second.
/// Pictures slower than that never count as moving, so nothing is judged late among them.
const SLOWEST_FRAME: Duration = Duration::from_nanos(1_000_000_000 / 15);

/// Pictures count as moving once this many gaps in a row between new ones were on time (within
/// one and a half pictures' times at their own pace): a playing video. Fewer (a slide changed,
/// then the stage screens' time a moment later) say nothing of when the next should come.
const MOVING_AFTER: u32 = 3;

/// Late gaps while pictures move wait for the gaps after them to say whether the pictures went
/// on moving (see [`Late`]). A gap up to this long says they did: one more gap after a late gap
/// of up to this long, or [`MOVING_AFTER`] more after a longer one, and the late gaps count. Two
/// gaps longer than this say they had stopped (the video ended and a slide was changed a moment
/// later; a burst of pictures for a screen that shows nothing moving, then its time once a
/// second), and the late gaps count nothing. So a stretch of pictures coming late counts every
/// one, and a hitch is said a picture or three after it ends.
const STILL_MOVING_UP_TO: Duration = Duration::from_millis(250);

/// A gap longer than this is pictures having stopped, not one coming late: a video ended, a
/// slide stayed. Nothing is counted for it.
const STOPPED_AFTER: Duration = Duration::from_secs(2);

/// A source's hitches are told to the log at most this often; the next line says how many
/// more there were.
const TELL_EVERY: Duration = Duration::from_secs(5);

/// Said to the app's log, from a source's own thread: a line, and whether it is a problem.
pub type Log = Arc<dyn Fn(&str, bool) + Send + Sync>;

/// How a source sends the pictures it is handed. A picture is handed over as the screens'
/// renderer leaves it: tightly packed RGBA rows whose fourth byte says how solid each pixel is,
/// with the colours as they are (not multiplied by it), which is how NDI's own documentation
/// says its RGBA pictures are: "not pre-multiplied". Any change is made on the source's own
/// thread, once for each new picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sent {
    /// Solid: the fourth byte is not looked at (NDI's RGBX). Sent as it is.
    Opaque,
    /// See-through, the colours as they are (NDI's RGBA). Sent as it is.
    Straight,
    /// See-through, each colour multiplied by how solid its pixel is first (NDI's RGBA), for a
    /// receiver that expects that.
    Premultiplied,
    /// How solid each pixel is, as a grey: white where solid, black where clear, the shades
    /// between at soft edges (NDI's RGBX). The key of a see-through picture, for a video
    /// switcher that takes it apart from the colours.
    Key,
}

impl Sent {
    /// True when the picture goes with its fourth byte as alpha.
    fn alpha(self) -> bool {
        matches!(self, Sent::Straight | Sent::Premultiplied)
    }
}

/// Each colour multiplied by how solid its pixel is, rounded to the nearest: what a receiver
/// that expects premultiplied colours wants. A clear pixel becomes black and clear; a solid one
/// is left as it is.
pub fn premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for &[red, green, blue, alpha] in rgba.as_chunks::<4>().0 {
        // (c × a + 127) / 255 rounds c × a / 255 to the nearest: with 255 odd, it is never half-way.
        let times = |colour: u8| ((u16::from(colour) * u16::from(alpha) + 127) / 255) as u8;
        out.extend_from_slice(&[times(red), times(green), times(blue), alpha]);
    }
    out
}

/// How solid each pixel is, as a solid grey: the key of a see-through picture.
pub fn key(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for &[.., alpha] in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&[alpha, alpha, alpha, 255]);
    }
    out
}

/// One picture to send: tightly packed RGBA rows, which other sources may be sending too, and
/// how this source sends it.
struct Picture {
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
    sent: Sent,
    /// True once `rgba` is as it is sent: any change `sent` asks for has been made.
    ready: bool,
    /// When it was handed over, for saying what held up one that came late.
    handed: Instant,
}

impl Picture {
    fn new(width: u32, height: u32, rgba: Arc<Vec<u8>>, sent: Sent) -> Picture {
        let ready = matches!(sent, Sent::Opaque | Sent::Straight);
        Picture { width, height, rgba, sent, ready, handed: Instant::now() }
    }

    /// Makes the change `sent` asks for, once: into a copy of its own, so other sources
    /// sending the same picture are not touched.
    fn prepare(&mut self) {
        if std::mem::replace(&mut self.ready, true) {
            return;
        }
        self.rgba = Arc::new(match self.sent {
            Sent::Premultiplied => premultiplied(&self.rgba),
            Sent::Key => key(&self.rgba),
            Sent::Opaque | Sent::Straight => return,
        });
    }
}

/// What a source's thread is told by whoever draws its pictures.
enum Told {
    /// The picture to send from now on.
    Picture(Picture),
    /// The picture last handed over is out of date, and no newer one is coming for now; true
    /// when the screen is see-through, so what covers it is all clear rather than black.
    Forget(bool),
}

/// How many samples a second a source's sound has.
pub const SOUND_RATE: u32 = 48_000;

/// How many channels a source's sound has: left and right.
pub const SOUND_CHANNELS: u32 = 2;

/// What a source is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Looking for the NDI runtime and starting the source.
    Starting,
    /// On the network, sending whenever anyone is watching.
    Sending,
    /// It could not start, or stopped; the reason in plain words.
    Failed(String),
}

/// `Shared::receivers` when the count isn't known.
const UNKNOWN: u32 = u32::MAX;

/// What a source's thread tells everyone else.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// How many receivers were connected when last counted, or `UNKNOWN`.
    receivers: AtomicU32,
    /// How many times receivers have been found after a count of none.
    arrivals: AtomicU32,
    /// The longest one picture took to send, in microseconds, since it was last asked
    /// (`NdiOutput::longest_send`).
    longest_send: AtomicU64,
    /// Where the source's sound goes while it runs: what its pictures go to, shared with
    /// whichever thread hands over the sound.
    sound: SoundTo,
    /// What whoever draws the pictures said became of them before they were handed over, this
    /// minute (see [`Before`]): not drawn, passed over, lost, and the longest one took to come
    /// back from the GPU, in microseconds. The source's thread takes them for its minute's line.
    not_drawn: AtomicU32,
    passed_over: AtomicU32,
    lost: AtomicU32,
    came_back: AtomicU64,
    /// How many hitches the source has had (see [`Hitch`]), and the latest, set before the
    /// count moves on.
    hitches: AtomicU32,
    hitch: Mutex<Option<Hitch>>,
    /// The last whole minute's counts, once there has been one.
    minute: Mutex<Option<Minute>>,
}

/// Where a running source's sound goes; nowhere before it has started and once it has stopped.
#[derive(Default)]
struct SoundTo(Mutex<Option<Arc<dyn Sink>>>);

impl std::fmt::Debug for SoundTo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.get().is_some() { "SoundTo(a source)" } else { "SoundTo(nothing)" })
    }
}

impl SoundTo {
    fn get(&self) -> Option<Arc<dyn Sink>> {
        self.0.lock().ok().and_then(|to| to.clone())
    }

    fn set(&self, to: Option<Arc<dyn Sink>>) {
        if let Ok(mut now) = self.0.lock() {
            *now = to;
        }
    }
}

impl Shared {
    fn new() -> Shared {
        Shared {
            state: Mutex::new(State::Starting),
            receivers: AtomicU32::new(UNKNOWN),
            arrivals: AtomicU32::new(0),
            longest_send: AtomicU64::new(0),
            sound: SoundTo::default(),
            not_drawn: AtomicU32::new(0),
            passed_over: AtomicU32::new(0),
            lost: AtomicU32::new(0),
            came_back: AtomicU64::new(0),
            hitches: AtomicU32::new(0),
            hitch: Mutex::new(None),
            minute: Mutex::new(None),
        }
    }

    /// Notes what became of a picture before it was handed over.
    fn note(&self, what: Before) {
        let count = match what {
            Before::NotDrawn => &self.not_drawn,
            Before::PassedOver => &self.passed_over,
            Before::Lost => &self.lost,
            Before::CameBack(took) => {
                self.came_back.fetch_max(micros(took), Ordering::Relaxed);
                return;
            }
        };
        count.fetch_add(1, Ordering::Relaxed);
    }

    /// Adds to a minute what whoever draws said of its pictures, and begins those counts again.
    fn take_before(&self, minute: &mut Minute) {
        minute.not_drawn = self.not_drawn.swap(0, Ordering::Relaxed);
        minute.passed_over = self.passed_over.swap(0, Ordering::Relaxed);
        minute.lost = self.lost.swap(0, Ordering::Relaxed);
        minute.came_back = Duration::from_micros(self.came_back.swap(0, Ordering::Relaxed));
    }

    /// A hitch: kept as the latest, then counted, so whoever sees the count move finds it.
    fn hitched(&self, hitch: Hitch) {
        if let Ok(mut latest) = self.hitch.lock() {
            *latest = Some(hitch);
        }
        self.hitches.fetch_add(1, Ordering::Release);
    }

    fn last_minute(&self) -> Option<Minute> {
        self.minute.lock().ok().and_then(|minute| *minute)
    }

    fn state(&self) -> State {
        self.state.lock().map_or_else(|_| State::Failed("NDI stopped.".to_owned()), |state| state.clone())
    }

    fn set_state(&self, now: State) {
        if let Ok(mut state) = self.state.lock() {
            *state = now;
        }
    }

    fn receivers(&self) -> Option<u32> {
        match self.receivers.load(Ordering::Relaxed) {
            UNKNOWN => None,
            count => Some(count),
        }
    }

    /// A picture took this long to send.
    fn sent_in(&self, took: Duration) {
        self.longest_send.fetch_max(micros(took), Ordering::Relaxed);
    }

    fn set_receivers(&self, count: Option<u32>) {
        self.receivers.store(count.map_or(UNKNOWN, |count| count.min(UNKNOWN - 1)), Ordering::Relaxed);
    }
}

/// What a source is doing, seen from anywhere.
#[derive(Clone, Debug)]
pub struct Watch(Arc<Shared>);

impl Watch {
    pub fn state(&self) -> State {
        self.0.state()
    }

    /// How many receivers are connected to the source, as last counted (about once a second).
    /// `None` while it is starting, once it has stopped, or when its NDI runtime can't count.
    /// Never waits.
    pub fn receivers(&self) -> Option<u32> {
        self.0.receivers()
    }

    /// How the source's pictures went out in its last whole minute, the figures of its
    /// per-minute line in the log; `None` before its first minute is over. Waits only for the
    /// source's thread to put a minute's figures in, which it does once a minute.
    pub fn last_minute(&self) -> Option<Minute> {
        self.0.last_minute()
    }
}

/// What became of a picture meant for a source before it was handed over, said by whoever
/// draws the pictures ([`NdiOutput::note`]) for the source's per-minute line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Before {
    /// It was not drawn: the GPU was too slow with it (it is drawn again later), or could not
    /// draw it.
    NotDrawn,
    /// A newer picture took its place before it was drawn, while the one before it was still
    /// on its way back from the GPU.
    PassedOver,
    /// It was drawn, and lost on its way back from the GPU.
    Lost,
    /// It came back from the GPU this long after it was asked for (drawing it included).
    CameBack(Duration),
}

/// A hitch someone watching a source could have seen: while pictures were moving, a new one
/// was sent more than two pictures' times after the one before, at the pictures' own pace (a
/// thirtieth of a second at 30 fps, a 24 fps video's own time for that), so the one before
/// stood still for a picture or more longer than the rest. Measured on the source's own
/// thread, from when each picture's send returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hitch {
    /// When the late picture's send returned.
    pub at: Instant,
    /// How long after the new picture before it.
    pub gap: Duration,
    /// How long after the picture before it was sent this one was handed over: time the
    /// source had nothing new to send.
    pub handed_after: Duration,
    /// How long it then waited for the source's thread, busy sending the one before.
    pub waited: Duration,
    /// How long it took to send, any change it needed (premultiplied, a key) included. NDI
    /// paces its sending, so up to about one picture's time is the wait for its turn.
    pub took: Duration,
}

/// How a source's pictures went out over one minute: the figures of its per-minute line
/// ([`Minute::words`]), counted only while somebody watched (or the count of receivers wasn't
/// known, when the source sends all the time).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Minute {
    /// How long the minute was (it ends at the first look after sixty seconds).
    pub length: Duration,
    /// How long of it somebody watched. Nothing else is counted when this is nought.
    pub watched: Duration,
    /// The size of the last picture sent.
    pub size: Option<(u32, u32)>,
    /// The pictures a second the source says it sends.
    pub rate: Rate,
    /// New pictures sent.
    pub sent: u32,
    /// The same picture sent again, because nothing new came for a second (as it always is
    /// while the slide stays as it is).
    pub repeated: u32,
    /// New pictures sent more than one and a half pictures' times after the one before, at the
    /// pictures' own pace, while pictures were moving (see [`Hitch`]).
    pub late: u32,
    /// Pictures handed over that a newer one took the place of before they were sent, the
    /// source's thread being busy sending (a 60 fps video on a source that sends 30 a second
    /// skips one in two).
    pub skipped: u32,
    /// The longest time between two new pictures while pictures were moving; `None` when
    /// nothing moved.
    pub longest_gap: Option<Duration>,
    /// The longest one send took.
    pub longest_send: Duration,
    /// The fewest and the most receivers counted while somebody watched; `None` when NDI
    /// couldn't count them, or nobody watched.
    pub receivers: Option<(u32, u32)>,
    /// How many hitches there were (every one is late too).
    pub hitches: u32,
    /// Said by whoever draws the pictures ([`Before`]).
    pub not_drawn: u32,
    pub passed_over: u32,
    pub lost: u32,
    pub came_back: Duration,
}

impl Minute {
    /// The minute's line for the log, for the source of this name.
    pub fn words(&self, name: &str) -> String {
        if self.watched.is_zero() {
            return format!("NDI \"{name}\": nobody watching in the last minute");
        }
        let ms = |took: Duration| format!("{:.1} ms", took.as_secs_f64() * 1000.0);
        let size = self.size.map_or(String::new(), |(w, h)| format!(" at {w}x{h} {}fps", self.rate.name()));
        let rate = f64::from(self.sent) / self.watched.as_secs_f64().max(0.001);
        let skipped = match self.passed_over {
            0 => format!("{} skipped", self.skipped),
            passed => format!(
                "{} skipped ({passed} of them before drawing, the GPU still reading back the one before)",
                self.skipped + passed
            ),
        };
        let gap = self.longest_gap.map_or("nothing moving".to_owned(), |gap| format!("longest gap {}", ms(gap)));
        let receivers = match self.receivers {
            None => "receivers not known".to_owned(),
            Some((fewest, most)) if fewest == most => format!("receivers {most}"),
            Some((fewest, most)) => format!("receivers {fewest} to {most}"),
        };
        // A minute watched from start to end says so; part of one says how much.
        let when = if self.watched + Duration::from_secs(1) >= self.length {
            "in the last minute".to_owned()
        } else {
            format!("in the {:.0} s watched of the last minute", self.watched.as_secs_f64())
        };
        format!(
            "NDI \"{name}\"{size}: {} pictures sent ({rate:.1}/s), {} repeated, {} late, {skipped}, {gap}, {} not \
             drawn, {} lost on the way back from the GPU (back in up to {}), {receivers}, send took up to {} {when}",
            self.sent,
            self.repeated,
            self.late,
            self.not_drawn,
            self.lost,
            ms(self.came_back),
            ms(self.longest_send),
        )
    }

    /// True when something went wrong that a receiver may have seen: worth a problem's line.
    pub fn trouble(&self) -> bool {
        self.late + self.skipped + self.passed_over + self.not_drawn + self.lost > 0
    }
}

/// Where to hand a source's sound, from any thread. It outlives its source: once that has
/// stopped, nobody wants the sound and what is handed over goes nowhere.
#[derive(Clone, Debug)]
pub struct Sound(Arc<Shared>);

impl Sound {
    /// Whether the source is running with somebody watching it (or can't count who is), so
    /// that sound handed over would be sent. Never waits.
    pub fn wanted(&self) -> bool {
        self.0.receivers() != Some(0) && self.0.sound.get().is_some()
    }

    /// Sends one block of sound, now: `planar` is the left channel's samples and then the
    /// right's, the same number of each, at [`SOUND_RATE`], and `heard` the time the first of
    /// them is heard, which goes with the block as its timecode (the pictures' are the time
    /// each is sent). Hand each block over when its sound is due: some receivers go by the
    /// timecodes, others by when each arrives. Returns as soon as NDI has taken its copy;
    /// nothing is sent while nobody is watching.
    pub fn play(&self, planar: &[f32], heard: std::time::SystemTime) {
        if self.0.receivers() == Some(0) {
            return;
        }
        if let Some(sink) = self.0.sound.get() {
            sink.sound(planar, heard);
        }
    }
}

/// A running NDI source. Dropping it stops the source.
pub struct NdiOutput {
    name: String,
    rate: Rate,
    shared: Arc<Shared>,
    tx: Option<Sender<Told>>,
    /// True from `forget` until the next picture is handed over: the source's thread is told
    /// once, however often it is said.
    forgotten: AtomicBool,
    /// For telling hitches at most every [`TELL_EVERY`]: the count of hitches last looked at,
    /// the count when one was last told, and when. Only its owner's thread looks.
    hitches_seen: Cell<u32>,
    hitches_told: Cell<u32>,
    told_at: Cell<Option<Instant>>,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// Starts an NDI source with this name, at the usual rate. Never waits for NDI: the source starts on its own
    /// thread, and `state` says how that went. The error is a plain sentence for the operator.
    /// Nobody is told when receivers connect (use `start_with_wake` to hand them a picture),
    /// and its per-minute line goes nowhere.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        NdiOutput::start_with_wake(name, Rate::default(), || {}, Arc::new(|_: &str, _: bool| {}))
    }

    /// Starts an NDI source as `start` does, whose pictures come at `rate`, and which calls `wake` each time the count of
    /// receivers is found to have changed, so that a picture can be drawn for receivers that
    /// connect after a spell with none (the source sends them nothing until one is handed
    /// over), and so that whoever shows the count can show the new one. `wake` runs on the source's own
    /// thread once the new count can be read from `receivers`, never on the caller's; it
    /// should only pass the word on and return. `log` is given, on the source's own thread,
    /// a line each minute saying how its pictures went out ([`Minute::words`]), and whether
    /// that is a problem.
    pub fn start_with_wake(
        name: &str,
        rate: Rate,
        wake: impl FnMut() + Send + 'static,
        log: Log,
    ) -> Result<NdiOutput, String> {
        // A source started again under the name it had (for another rate) may find the one
        // before it still being taken down, on that one's own thread, and NDI refuses a second
        // of the name: it is tried again for a moment, here on the new source's thread.
        let open = move |name: &str| {
            let runtime = runtime::get()?;
            let mut tries = 0;
            loop {
                match runtime::Source::new(runtime, name, rate) {
                    Err(_) if tries < START_TRIES => {
                        tries += 1;
                        std::thread::sleep(START_AGAIN_AFTER);
                    }
                    started => return started,
                }
            }
        };
        spawn(name, rate, open, wake, log, MINUTE)
    }

    /// The pictures a second the source says it sends.
    pub fn rate(&self) -> Rate {
        self.rate
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the source is doing now.
    pub fn state(&self) -> State {
        self.shared.state()
    }

    /// How many receivers are connected, as [`Watch::receivers`] says. Never waits.
    pub fn receivers(&self) -> Option<u32> {
        self.shared.receivers()
    }

    /// How many times receivers have come to the source after a spell with none. Each time,
    /// the source has no picture and waits to be handed one; whoever draws them remembers the
    /// number it last drew for, and draws again when it has moved on. (The count of receivers
    /// alone can't say this: read after they left and others came, it looks unchanged.) It
    /// moves on before `wake` is called for that change. Never waits.
    pub fn arrivals(&self) -> u32 {
        self.shared.arrivals.load(Ordering::Relaxed)
    }

    /// The longest one picture has taken to send (on the source's own thread, which may wait
    /// for NDI there) since this was last asked, and the count begun again. For the log of the
    /// thread drawing the screens, which never waits on a send. Never waits.
    pub fn longest_send(&self) -> Duration {
        Duration::from_micros(self.shared.longest_send.swap(0, Ordering::Relaxed))
    }

    /// A way to see what the source is doing from another thread.
    pub fn watch(&self) -> Watch {
        Watch(self.shared.clone())
    }

    /// How the source's pictures went out in its last whole minute (see [`Watch::last_minute`]).
    pub fn last_minute(&self) -> Option<Minute> {
        self.shared.last_minute()
    }

    /// Says what became of a picture meant for this source before it was handed over, for its
    /// per-minute line. Said only for a source somebody is watching (or whose count isn't
    /// known). Never waits.
    pub fn note(&self, what: Before) {
        self.shared.note(what);
    }

    /// The source's latest hitch, if it has had one since this was last asked, and it is
    /// [`TELL_EVERY`] since the last one told: with how many more there were since that one,
    /// not told one by one. A hitch not told when it is new is never told (whatever made it is
    /// long gone by the time telling is allowed again); it is counted in the next told. For
    /// the thread drawing the pictures to say in the log what it was doing then: one atomic
    /// load, unless there is a hitch to tell. Never waits for long.
    pub fn hitch_to_tell(&self, now: Instant) -> Option<(Hitch, u32)> {
        let count = self.shared.hitches.load(Ordering::Acquire);
        if count == self.hitches_seen.replace(count) {
            return None;
        }
        if self.told_at.get().is_some_and(|told| now.saturating_duration_since(told) < TELL_EVERY) {
            return None;
        }
        let hitch = self.shared.hitch.lock().ok().and_then(|latest| *latest)?;
        let more = count.saturating_sub(self.hitches_told.replace(count)).saturating_sub(1);
        self.told_at.set(Some(now));
        Some((hitch, more))
    }

    /// Where to hand the sound that goes with the source's pictures, from any thread.
    pub fn sound(&self) -> Sound {
        Sound(self.shared.clone())
    }

    /// Hands over the picture to send from now on, sent as `sent` says. Never blocks, and
    /// never copies the pixels here (a change `sent` asks for is made on the source's own
    /// thread). A picture whose size and pixel count disagree is ignored, and so is one handed
    /// over while nobody is watching.
    pub fn show(&self, width: u32, height: u32, rgba: Vec<u8>, sent: Sent) {
        self.show_shared(width, height, Arc::new(rgba), sent);
    }

    /// Hands over the picture to send from now on, as `show` does, for a picture that several
    /// sources send: each is handed the same pixels, and none copies them unless it sends them
    /// changed (premultiplied, or as a key), which it does on its own thread. Never blocks. A
    /// picture whose size and pixel count disagree is ignored, and so is one handed over
    /// while nobody is watching.
    pub fn show_shared(&self, width: u32, height: u32, rgba: Arc<Vec<u8>>, sent: Sent) {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        self.forgotten.store(false, Ordering::Relaxed);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Told::Picture(Picture::new(width, height, rgba, sent)));
        }
    }

    /// Says that the picture last handed over is no longer what should be seen, and that no
    /// newer one is being handed over for now: nobody is watching, so it was not drawn, or it
    /// was lost on the way. NDI itself keeps the last picture a source sent and gives it to a
    /// receiver the moment it connects; after this, a receiver that connects to a source nobody
    /// was watching is given a black picture instead, until one is handed over for it. A source
    /// somebody is watching goes on sending the picture it has, once a second as always; the
    /// black picture follows only if its receivers all leave before a newer one is handed over.
    /// Never blocks. Said again before the next picture is handed over, it does nothing, so it
    /// can be said for every picture that is not drawn.
    pub fn forget(&self) {
        self.forget_as(false);
    }

    /// Says what [`NdiOutput::forget`] says, for a screen that is see-through when `see_through`
    /// is true: then what takes the old picture's place is all clear, not black, whatever the
    /// old picture was, so a switcher shows its camera rather than black where the words were.
    pub fn forget_as(&self, see_through: bool) {
        if !self.forgotten.swap(true, Ordering::Relaxed)
            && let Some(tx) = &self.tx
        {
            let _ = tx.send(Told::Forget(see_through));
        }
    }
}

impl Drop for NdiOutput {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop, and it takes its source down on its own.
        // It isn't waited for: it may be part-way through sending a picture (paced to the
        // frame rate) or still starting NDI, and the thread dropping this is the one drawing
        // the screens, which must never wait on NDI.
        self.tx = None;
        drop(self.thread.take());
    }
}

/// Whether the NDI runtime is on this computer. Only looks for its file (a few quick checks),
/// so it can be asked from anywhere.
pub fn installed() -> bool {
    runtime::locate().is_some()
}

/// Where a source's pictures and sound go: NDI, or a stand-in in the tests. Its pictures are
/// sent from the source's own thread and its sound from whichever thread hands that over, at
/// the same time.
trait Sink: Send + Sync {
    /// How many receivers are connected, or `None` when that can't be known. Never waits.
    fn receivers(&self) -> Option<u32>;
    /// Sends one picture, see-through with `alpha` (else solid); may wait until it is time for
    /// the next.
    fn send(&self, width: u32, height: u32, rgba: &[u8], alpha: bool);
    /// Sends one block of sound, the left channel's samples and then the right's, at
    /// [`SOUND_RATE`], the first of them heard at `heard`; never waits for its time.
    fn sound(&self, planar: &[f32], heard: std::time::SystemTime);
}

impl Sink for runtime::Source {
    fn receivers(&self) -> Option<u32> {
        runtime::Source::receivers(self)
    }

    fn send(&self, width: u32, height: u32, rgba: &[u8], alpha: bool) {
        runtime::Source::send(self, width, height, rgba, alpha);
    }

    fn sound(&self, planar: &[f32], heard: std::time::SystemTime) {
        self.send_sound(SOUND_RATE, SOUND_CHANNELS, planar, heard);
    }
}

/// Starts a source's thread, which opens where its pictures go with `open` and then sends,
/// telling `log` how that went every `minute`.
fn spawn<S: Sink + 'static>(
    name: &str,
    rate: Rate,
    open: impl FnOnce(&str) -> Result<S, String> + Send + 'static,
    mut wake: impl FnMut() + Send + 'static,
    log: Log,
    minute: Duration,
) -> Result<NdiOutput, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The NDI source needs a name.".to_owned());
    }
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::new());
    let (source, theirs) = (name.to_owned(), shared.clone());
    let thread = std::thread::Builder::new()
        .name("ndi".to_owned())
        .spawn(move || match open(&source) {
            Ok(sink) => {
                let sink = Arc::new(sink);
                theirs.sound.set(Some(sink.clone()));
                theirs.set_state(State::Sending);
                let report = Report { name: &source, log: &*log, minute, rate };
                serve(&*sink, &rx, &theirs, &mut wake, &report);
                // Sound handed over from now on goes nowhere; the source itself is taken down
                // when a block being sent this moment, if there is one, has gone.
                theirs.sound.set(None);
                theirs.set_receivers(None);
            }
            Err(reason) => theirs.set_state(State::Failed(reason)),
        })
        .map_err(|e| format!("Could not start the NDI thread: {e}"))?;
    Ok(NdiOutput {
        name: name.to_owned(),
        rate,
        shared,
        tx: Some(tx),
        forgotten: AtomicBool::new(false),
        hitches_seen: Cell::new(0),
        hitches_told: Cell::new(0),
        told_at: Cell::new(None),
        thread: Some(thread),
    })
}

/// What has queued up comes to, `first` included: the newest picture, if there is one, and
/// whether the word to forget came after it (or with no picture at all), with whether the
/// screen was see-through then; and how many pictures the newest overtook, which are never
/// sent. Word to forget from before the newest picture is overtaken by it.
fn newest(rx: &Receiver<Told>, first: Told) -> (Option<Picture>, Option<bool>, u32) {
    let (mut picture, mut forget, mut overtaken) = (None, None, 0);
    let mut next = Some(first);
    while let Some(told) = next {
        match told {
            Told::Picture(newer) => {
                overtaken += u32::from(picture.is_some());
                (picture, forget) = (Some(newer), None);
            }
            Told::Forget(see_through) => forget = Some(see_through),
        }
        next = rx.try_recv().ok();
    }
    (picture, forget, overtaken)
}

/// Who a source's thread tells how its pictures went out, and how often.
struct Report<'a> {
    name: &'a str,
    log: &'a (dyn Fn(&str, bool) + Send + Sync),
    minute: Duration,
    rate: Rate,
}

/// Sends the pictures handed over to `sink` until the output is dropped: each new one at once
/// and the current one again every second, while anyone is watching. Calls `wake` whenever the
/// count of receivers has changed, and tells the log each minute how the pictures went out.
fn serve(sink: &impl Sink, rx: &Receiver<Told>, shared: &Shared, wake: &mut impl FnMut(), report: &Report<'_>) {
    let mut gate = Gate::new(Instant::now());
    let mut pace = Pace::at(Instant::now(), report.minute, report.rate);
    // Counts the receivers, and tells everyone the count before `wake`, so that whoever is
    // woken reads the new one.
    let mut count = |gate: &mut Gate, pace: &mut Pace| {
        let receivers = sink.receivers();
        let nobody_was = !gate.watched();
        let now = Instant::now();
        let changed = gate.counted(now, receivers);
        pace.counted(now, receivers);
        if nobody_was && gate.watched() {
            shared.arrivals.fetch_add(1, Ordering::Relaxed);
        }
        shared.set_receivers(receivers);
        if changed {
            wake();
        }
    };
    loop {
        match rx.recv_timeout(gate.wait(Instant::now())) {
            Ok(first) => {
                let (picture, forget, overtaken) = newest(rx, first);
                // Word for a source nobody was watching when last counted. Someone may have
                // come since: counted again now, a picture is sent to them rather than
                // dropped, and a black one is not sent over what they are looking at.
                let watched_before = gate.watched();
                if !watched_before {
                    count(&mut gate, &mut pace);
                }
                pace.skipped(overtaken);
                gate.told(picture, forget, watched_before);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if gate.count_due(Instant::now()) {
            count(&mut gate, &mut pace);
        }
        // Black for a solid source; for a see-through one, all clear (zeros sent as RGBA), so
        // that a video switcher shows its camera rather than black where the words were.
        if let Some((width, height, alpha)) = gate.blank_due() {
            let began = Instant::now();
            sink.send(width, height, &vec![0; width as usize * height as usize * 4], alpha);
            shared.sent_in(began.elapsed());
        }
        if let Some((picture, fresh)) = gate.picture_due(Instant::now()) {
            // Changed once, the first time it is sent, then sent again as it is each second.
            let began = Instant::now();
            picture.prepare();
            sink.send(picture.width, picture.height, &picture.rgba, picture.sent.alpha());
            let ended = Instant::now();
            shared.sent_in(ended - began);
            if !fresh {
                pace.sent_again(ended - began);
            } else if let Some(hitch) = pace.sent_new((picture.width, picture.height), picture.handed, began, ended) {
                shared.hitched(hitch);
            }
        }
        if let Some(mut minute) = pace.minute_over(Instant::now()) {
            shared.take_before(&mut minute);
            if let Ok(mut last) = shared.minute.lock() {
                *last = Some(minute);
            }
            (report.log)(&minute.words(report.name), minute.trouble());
        }
    }
}

/// How a source's pictures go out over the minute under way, counted on the source's own
/// thread (nothing in it is shared, so nothing is waited for), and whether pictures are moving
/// now: a playing video, as against a slide that stays, which is sent again once a second.
/// Told the time rather than reading the clock, so it can be tested on its own.
///
/// A gap is on time when it is within one and a half pictures' times at the pictures' own pace
/// (`frame`): a thirtieth of a second for a 30 fps video (never less than the source's own
/// picture time: NDI's pacing brings a faster video to that), a 24 fps video's own time for that, whose steady pace is no stutter though a
/// receiver shows every fourth picture twice. Later than that is late; later than two
/// pictures' times, a hitch.
///
/// Late pictures and hitches are judged only while pictures move: once [`MOVING_AFTER`] gaps
/// in a row between new pictures were on time. Late gaps (each up to [`STOPPED_AFTER`]) then
/// count once the gaps after them show the pictures went on moving ([`STILL_MOVING_UP_TO`]);
/// otherwise they had stopped, and count nothing. So a slide that stays, a video that ends,
/// the stage screens' time once a second and a slide changed now and then count nothing.
struct Pace {
    began: Instant,
    length: Duration,
    /// Since when somebody has been watching, while they are, and how long they watched
    /// before that in this minute.
    watched_from: Option<Instant>,
    watched: Duration,
    /// The count of receivers last made, which the next minute's fewest and most start from.
    last_count: Option<Option<u32>>,
    receivers: Option<(u32, u32)>,
    count_unknown: bool,
    sent: u32,
    repeated: u32,
    late: u32,
    skipped: u32,
    hitches: u32,
    longest_gap: Option<Duration>,
    longest_send: Duration,
    size: Option<(u32, u32)>,
    /// When the last new picture's send returned, while somebody watches, how many gaps in a
    /// row between new pictures were on time, and the late gaps waiting for the gaps after
    /// them to say whether they count.
    last_new: Option<Instant>,
    run: u32,
    pending: Option<Late>,
    /// The pictures' own pace: the time between them, followed slowly from the gaps on time,
    /// from the source's own rate (NDI sends no faster) to [`SLOWEST_FRAME`].
    frame: Duration,
    /// The gap before the last new picture, for telling a steady pace from a hold-up.
    last_gap: Option<Duration>,
    /// The pictures a second the source says it sends.
    rate: Rate,
}

/// Late gaps while pictures move, one after another or close together (a hold-up with a
/// picture late beside it is one stutter), waiting for the gaps after them to say whether the
/// pictures went on moving: the longest, how many there are, and how many more gaps up to
/// [`STILL_MOVING_UP_TO`] they wait for.
#[derive(Clone, Copy, Debug)]
struct Late {
    worst: Hitch,
    gaps: u32,
    waiting: u32,
}

impl Late {
    /// A late gap, the first: one more gap is waited for after a short one, more after a
    /// hold-up.
    fn new(gap: Hitch) -> Late {
        let waiting = if gap.gap <= STILL_MOVING_UP_TO { 1 } else { MOVING_AFTER };
        Late { worst: gap, gaps: 1, waiting }
    }

    /// Another late gap, before the pictures were seen moving again.
    fn add(&mut self, gap: Hitch) {
        self.gaps += 1;
        if gap.gap > self.worst.gap {
            self.worst = gap;
        }
    }

    /// Whether a gap longer than [`STILL_MOVING_UP_TO`] is among them.
    fn held(&self) -> bool {
        self.worst.gap > STILL_MOVING_UP_TO
    }
}

impl Pace {
    /// For a source that sends thirty a second.
    #[cfg(test)]
    fn new(now: Instant, length: Duration) -> Pace {
        Pace::at(now, length, Rate::R30)
    }

    /// For a source that sends at `rate`.
    fn at(now: Instant, length: Duration, rate: Rate) -> Pace {
        Pace {
            began: now,
            length,
            watched_from: None,
            watched: Duration::ZERO,
            last_count: None,
            receivers: None,
            count_unknown: false,
            sent: 0,
            repeated: 0,
            late: 0,
            skipped: 0,
            hitches: 0,
            longest_gap: None,
            longest_send: Duration::ZERO,
            size: None,
            last_new: None,
            run: 0,
            pending: None,
            frame: rate.frame(),
            last_gap: None,
            rate,
        }
    }

    /// A gap longer than this is late: one and a half pictures' times at their own pace.
    fn late_after(&self) -> Duration {
        self.frame * 3 / 2
    }

    fn watching(&self) -> bool {
        self.watched_from.is_some()
    }

    /// The receivers were counted (`None`: NDI can't count them, so someone may be watching).
    fn counted(&mut self, now: Instant, receivers: Option<u32>) {
        self.last_count = Some(receivers);
        if receivers == Some(0) {
            if let Some(from) = self.watched_from.take() {
                self.watched += now.saturating_duration_since(from);
            }
            // Whatever moved is over: the next receivers start afresh.
            (self.last_new, self.run, self.pending) = (None, 0, None);
            return;
        }
        self.watched_from.get_or_insert(now);
        self.add_count(receivers);
    }

    fn add_count(&mut self, receivers: Option<u32>) {
        match receivers {
            Some(count) => {
                let (fewest, most) = self.receivers.unwrap_or((count, count));
                self.receivers = Some((fewest.min(count), most.max(count)));
            }
            None => self.count_unknown = true,
        }
    }

    /// Pictures handed over were overtaken by a newer one before they could be sent.
    fn skipped(&mut self, overtaken: u32) {
        if self.watching() {
            self.skipped += overtaken;
        }
    }

    /// The current picture was sent again, its send taking `took`.
    fn sent_again(&mut self, took: Duration) {
        if self.watching() {
            self.repeated += 1;
            self.longest_send = self.longest_send.max(took);
        }
    }

    /// A new picture of this size was sent: handed over at `handed`, its send begun at `began`
    /// and returned at `ended`. The hitch this picture says there was: the longest of the late
    /// gaps before it, now that the pictures are seen to have gone on moving after them (see
    /// [`STILL_MOVING_UP_TO`]), when it was long enough to be one.
    fn sent_new(&mut self, size: (u32, u32), handed: Instant, began: Instant, ended: Instant) -> Option<Hitch> {
        if !self.watching() {
            return None;
        }
        self.sent += 1;
        self.size = Some(size);
        self.longest_send = self.longest_send.max(ended.saturating_duration_since(began));
        let last = self.last_new.replace(ended)?;
        let gap = ended.saturating_duration_since(last);
        let this = Hitch {
            at: ended,
            gap,
            handed_after: handed.saturating_duration_since(last),
            waited: began.saturating_duration_since(handed),
            took: ended.saturating_duration_since(began),
        };
        let on_time = gap <= self.late_after();
        if on_time {
            if self.run >= MOVING_AFTER {
                self.longest_gap = Some(self.longest_gap.map_or(gap, |longest| longest.max(gap)));
            }
            self.run = self.run.saturating_add(1);
        }
        // The pictures' own pace follows the gaps, slowly (a sixteenth of the way): those on
        // time, and late ones that are steady (within a quarter of the gap before) and no
        // slower than the slowest pace, so that a video with fewer pictures a second than the
        // source sends (a 29.97 fps video on a 59.94 screen), or one that follows a faster
        // one, is found to be steady within half a second, not counted late for ever. A
        // hold-up is not steady, and moves nothing.
        let steady = self.last_gap.is_some_and(|before| gap <= SLOWEST_FRAME && gap.abs_diff(before) * 4 <= gap);
        self.last_gap = Some(gap);
        if on_time || steady {
            let (frame, gap) = (self.frame.as_secs_f64(), gap.as_secs_f64());
            let followed = frame + (gap - frame) / 16.0;
            self.frame =
                Duration::from_secs_f64(followed.clamp(self.rate.frame().as_secs_f64(), SLOWEST_FRAME.as_secs_f64()));
        }
        let mut hitch = None;
        match self.pending.take() {
            // Not moving (yet): only a run of pictures on time counts.
            None if self.run < MOVING_AFTER || gap > STOPPED_AFTER => {
                if !on_time {
                    self.run = 0;
                }
            }
            None => {
                if !on_time {
                    self.pending = Some(Late::new(this));
                }
            }
            // Moving on after late gaps: on time, or late but soon.
            Some(mut late) if gap <= STILL_MOVING_UP_TO => {
                late.waiting -= 1;
                if late.waiting > 0 {
                    if !on_time {
                        late.add(this);
                    }
                    self.pending = Some(late);
                } else {
                    hitch = self.count(late);
                    if !on_time {
                        self.pending = Some(Late::new(this));
                    }
                }
            }
            // A hold-up after a picture or two late: one stutter, waiting to see the video move.
            Some(mut late) if gap <= STOPPED_AFTER && !late.held() => {
                late.add(this);
                late.waiting = MOVING_AFTER;
                self.pending = Some(late);
            }
            // A second long gap: the pictures had stopped.
            Some(_) => self.run = 0,
        }
        hitch
    }

    /// Late gaps the pictures went on moving after count: the longest of them is a hitch, when
    /// it is longer than two pictures' times at their own pace.
    fn count(&mut self, late: Late) -> Option<Hitch> {
        self.late += late.gaps;
        let worst = late.worst;
        self.longest_gap = Some(self.longest_gap.map_or(worst.gap, |longest| longest.max(worst.gap)));
        (worst.gap > self.frame * 2).then(|| {
            self.hitches += 1;
            worst
        })
    }

    /// The minute's figures, once it is over at `now`, and the counts begun again; `None`
    /// until then. What whoever draws said is added by the caller.
    fn minute_over(&mut self, now: Instant) -> Option<Minute> {
        let length = now.saturating_duration_since(self.began);
        if length < self.length {
            return None;
        }
        if let Some(from) = self.watched_from.as_mut() {
            self.watched += now.saturating_duration_since(*from);
            *from = now;
        }
        let watched = self.watched > Duration::ZERO;
        let minute = Minute {
            length,
            watched: self.watched,
            size: self.size.filter(|_| watched),
            rate: self.rate,
            sent: self.sent,
            repeated: self.repeated,
            late: self.late,
            skipped: self.skipped,
            longest_gap: self.longest_gap,
            longest_send: self.longest_send,
            receivers: self.receivers.filter(|_| !self.count_unknown),
            hitches: self.hitches,
            ..Minute::default()
        };
        let (last_new, run, pending, last_count) = (self.last_new, self.run, self.pending, self.last_count);
        let (watched_from, frame, last_gap) = (self.watched_from, self.frame, self.last_gap);
        *self = Pace {
            watched_from,
            last_new,
            run,
            pending,
            frame,
            last_gap,
            last_count,
            ..Pace::at(now, self.length, self.rate)
        };
        // The next minute's receivers start from the count there is now, if somebody watches.
        if self.watching()
            && let Some(receivers) = last_count
        {
            self.add_count(receivers);
        }
        Some(minute)
    }
}

/// What a source's thread remembers between one wake and the next, and its rule for what to
/// send when. Told the time rather than reading the clock, so it can be tested on its own.
struct Gate {
    /// The newest picture; none while nobody is watching.
    current: Option<Picture>,
    /// How many receivers were connected when last counted; `None`, sent to as if someone
    /// were watching, when that isn't known.
    receivers: Option<u32>,
    /// The current picture is new, and still to be sent at once.
    fresh: bool,
    /// The size of the last picture sent, and whether it was see-through, while NDI still has
    /// it to give to a receiver the moment one connects; `None` before any is sent and once a
    /// black (or clear) one has taken its place.
    given: Option<(u32, u32, bool)>,
    /// That picture is out of date: a newer one was dropped or never drawn, nobody watching.
    stale: bool,
    /// Whether what made it out of date was see-through, when that is known: the black picture
    /// due then goes all clear for a see-through one, whatever the picture it covers was (a
    /// screen made see-through while nobody watched, say).
    cover: Option<bool>,
    next_count: Instant,
    next_resend: Instant,
}

impl Gate {
    /// Receivers are counted straight away, before anything is sent.
    fn new(now: Instant) -> Gate {
        Gate {
            current: None,
            receivers: None,
            fresh: false,
            given: None,
            stale: false,
            cover: None,
            next_count: now,
            next_resend: now,
        }
    }

    fn watched(&self) -> bool {
        self.receivers != Some(0)
    }

    /// How long the thread may wait for a new picture before it has something else to do.
    fn wait(&self, now: Instant) -> Duration {
        let next = if self.watched() && self.current.is_some() {
            self.next_count.min(self.next_resend)
        } else {
            self.next_count
        };
        next.saturating_duration_since(now)
    }

    /// A new picture: it takes the current one's place and goes at once. While nobody is
    /// watching it is dropped: by the time anyone watches, it may be a slide long gone. The
    /// one NDI was last sent is then out of date too.
    fn picture(&mut self, picture: Picture) {
        if self.watched() {
            self.current = Some(picture);
            self.fresh = true;
            self.stale = false;
            self.cover = None;
        } else {
            self.stale = true;
            self.cover = Some(picture.sent.alpha());
        }
    }

    /// What has queued up: the newest picture, and whether word to forget came after it.
    /// `watched_before` says somebody was watching when the source last counted, before it
    /// counted again for this. A picture followed by word to forget is still the newest there
    /// is for a source that was being watched all along (a newer one was lost on the way): it
    /// is sent, and marked out of date. For a source nobody was watching, the word means a
    /// newer picture was not drawn: that picture is from before whoever has come since, and
    /// is not sent to them. The word says whether the screen is see-through.
    fn told(&mut self, picture: Option<Picture>, forget: Option<bool>, watched_before: bool) {
        if let Some(picture) = picture
            && (forget.is_none() || watched_before)
        {
            self.picture(picture);
        }
        if let Some(see_through) = forget {
            self.forget();
            self.cover = Some(see_through);
        }
    }

    /// The picture last handed over is out of date, and no newer one is coming for now.
    /// With somebody watching it is still kept and sent again each second: it is what they
    /// are looking at, and a mixer expects a steady source. (With nobody watching none is
    /// kept anyway.) Either way it is marked, so that black takes its place once nobody is.
    fn forget(&mut self) {
        if !self.watched() {
            self.current = None;
            self.fresh = false;
        }
        self.stale = true;
    }

    /// The size of the black picture to send now, if one is due, and whether it goes
    /// see-through (all clear): as what made the picture out of date was, if that is known,
    /// else as the picture it covers. Due when nobody is watching and the last picture sent is
    /// out of date. NDI gives a receiver the last picture a source sent the moment it connects,
    /// before the source can know of it; this puts black (or nothing) in the place of a slide
    /// that has gone, once. While someone is watching none is due: they were given the
    /// picture when it was the newest, and a new one is on its way to them. Should they leave
    /// before it comes (it could not be drawn, say), black is due then.
    fn blank_due(&mut self) -> Option<(u32, u32, bool)> {
        if !self.stale || self.watched() {
            return None;
        }
        self.stale = false;
        let cover = self.cover.take();
        self.given.take().map(|(width, height, alpha)| (width, height, cover.unwrap_or(alpha)))
    }

    /// Whether it is time to count the receivers again.
    fn count_due(&self, now: Instant) -> bool {
        now >= self.next_count
    }

    /// The receivers have been counted. True when the count has changed, which whoever draws
    /// the pictures is told. With nobody watching, the picture kept is let go, so receivers
    /// that come later are sent nothing from before they came: only what is handed over then.
    fn counted(&mut self, now: Instant, receivers: Option<u32>) -> bool {
        self.next_count = now + COUNT_EVERY;
        let changed = self.receivers != receivers;
        self.receivers = receivers;
        if !self.watched() {
            let dropped = self.current.take();
            // A picture not sent yet goes unsent, so the one sent before it is out of date.
            if std::mem::take(&mut self.fresh) {
                self.stale = true;
                self.cover = dropped.map(|picture| picture.sent.alpha());
            }
        }
        changed
    }

    /// The picture to send now, if any: a fresh one at once, the current one again when it is
    /// time, and nothing while nobody is watching. True with it when it is fresh, not sent
    /// before.
    fn picture_due(&mut self, now: Instant) -> Option<(&mut Picture, bool)> {
        if !self.watched() || !(self.fresh || now >= self.next_resend) {
            return None;
        }
        let picture = self.current.as_mut()?;
        let fresh = std::mem::take(&mut self.fresh);
        self.next_resend = now + RESEND_EVERY;
        self.given = Some((picture.width, picture.height, picture.sent.alpha()));
        Some((picture, fresh))
    }
}

/// A length of time in whole microseconds, for keeping in an atomic.
fn micros(took: Duration) -> u64 {
    u64::try_from(took.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);
    /// The longest a test waits for a source's thread: far longer than it should take.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn picture(width: u32) -> Picture {
        Picture::new(width, 1, Arc::new(vec![9; width as usize * 4]), Sent::Opaque)
    }

    /// The width of the picture the gate sends now, which tells the tests' pictures apart.
    fn sent(gate: &mut Gate, now: Instant) -> Option<u32> {
        gate.picture_due(now).map(|(picture, _)| picture.width)
    }

    #[test]
    fn colours_are_multiplied_by_how_solid_they_are_rounded_to_the_nearest() {
        // Clear is black and clear; solid is left as it is, whatever the colour.
        assert_eq!(premultiplied(&[200, 100, 7, 0]), [0, 0, 0, 0]);
        assert_eq!(premultiplied(&[255, 128, 1, 255]), [255, 128, 1, 255]);
        assert_eq!(premultiplied(&[0, 0, 0, 255]), [0, 0, 0, 255]);
        // White half solid: 255 × 128 / 255 is 128 exactly.
        assert_eq!(premultiplied(&[255, 255, 255, 128]), [128, 128, 128, 128]);
        // Rounded, not cut short: 100 × 128 / 255 = 50.2 → 50; 101 × 128 / 255 = 50.7 → 51;
        // 1 × 128 / 255 = 0.502 → 1; 1 × 127 / 255 = 0.498 → 0.
        assert_eq!(premultiplied(&[100, 101, 1, 128]), [50, 51, 1, 128]);
        assert_eq!(premultiplied(&[1, 1, 1, 127]), [0, 0, 0, 127]);
        // Every colour and alpha against the exact sum: never more than half a step out, and
        // the alpha never changed.
        for alpha in 0..=255u8 {
            for colour in 0..=255u8 {
                let out = premultiplied(&[colour, 0, 255, alpha]);
                let exact = f64::from(colour) * f64::from(alpha) / 255.0;
                assert!((f64::from(out[0]) - exact).abs() <= 0.5, "{colour} at {alpha}: {} for {exact}", out[0]);
                assert_eq!((out[1], out[2], out[3]), (0, alpha, alpha));
            }
        }
        // Several pixels, each on its own.
        assert_eq!(premultiplied(&[255, 0, 0, 255, 0, 255, 0, 0]), [255, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn a_key_is_how_solid_each_pixel_is_as_a_solid_grey() {
        // Clear is black, solid is white, a soft edge a grey, whatever the colour was.
        assert_eq!(key(&[200, 100, 7, 0]), [0, 0, 0, 255]);
        assert_eq!(key(&[0, 0, 0, 255]), [255, 255, 255, 255]);
        assert_eq!(key(&[255, 255, 255, 255, 30, 60, 90, 77]), [255, 255, 255, 255, 77, 77, 77, 255]);
    }

    #[test]
    fn a_picture_is_changed_once_into_its_own_copy_and_only_when_it_must_be() {
        let shared = Arc::new(vec![255, 255, 255, 128]);
        // Solid and straight pictures go as they are, the same pixels as every other source.
        for sent in [Sent::Opaque, Sent::Straight] {
            let mut picture = Picture::new(1, 1, shared.clone(), sent);
            picture.prepare();
            assert!(Arc::ptr_eq(&picture.rgba, &shared), "{sent:?}");
        }
        assert!(!Sent::Opaque.alpha() && Sent::Straight.alpha() && Sent::Premultiplied.alpha() && !Sent::Key.alpha());
        // Premultiplied and a key are made into a copy of their own, once, leaving the shared one.
        let mut premultiplied = Picture::new(1, 1, shared.clone(), Sent::Premultiplied);
        premultiplied.prepare();
        assert_eq!(*premultiplied.rgba, [128, 128, 128, 128]);
        premultiplied.prepare();
        assert_eq!(*premultiplied.rgba, [128, 128, 128, 128], "not multiplied again when sent again");
        let mut key = Picture::new(1, 1, shared.clone(), Sent::Key);
        key.prepare();
        assert_eq!((*key.rgba).clone(), [128, 128, 128, 255]);
        assert_eq!(*shared, [255, 255, 255, 128]);
    }

    #[test]
    fn a_see_through_picture_gone_out_of_date_is_covered_with_clear_not_black() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(Picture::new(3, 1, Arc::new(vec![9; 12]), Sent::Straight));
        assert_eq!(sent(&mut gate, t), Some(3));
        gate.counted(t + SECOND, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), Some((3, 1, true)), "all clear, so a switcher shows its camera");
        // A key covers with black, which keys nothing in.
        gate.counted(t + 2 * SECOND, Some(1));
        gate.picture(Picture::new(4, 1, Arc::new(vec![9; 16]), Sent::Key));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(4));
        gate.counted(t + 3 * SECOND, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), Some((4, 1, false)));
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert!(NdiOutput::start("   ").err().is_some_and(|e| e.contains("needs a name")));
    }

    #[test]
    fn starting_never_waits_for_ndi() {
        let started = std::time::Instant::now();
        let output = NdiOutput::start("SoftPresenter test").unwrap();
        assert!(started.elapsed() < Duration::from_millis(50), "start returns at once");
        drop(output);
    }

    #[test]
    fn the_newest_queued_picture_wins() {
        let (tx, rx) = channel();
        let width =
            |(picture, forget, _): (Option<Picture>, Option<bool>, u32)| (picture.map(|picture| picture.width), forget);
        for width in [2, 3] {
            tx.send(Told::Picture(picture(width))).unwrap();
        }
        assert_eq!(width(newest(&rx, Told::Picture(picture(1)))), (Some(3), None));
        assert!(rx.try_recv().is_err(), "the older ones are gone");
        // Word to forget after a picture: both are kept, for the gate to weigh.
        tx.send(Told::Forget(false)).unwrap();
        assert_eq!(width(newest(&rx, Told::Picture(picture(1)))), (Some(1), Some(false)));
        // A picture after it overtakes it; with no picture, there is only the word, the newest
        // saying whether the screen is see-through.
        tx.send(Told::Picture(picture(4))).unwrap();
        assert_eq!(width(newest(&rx, Told::Forget(false))), (Some(4), None));
        tx.send(Told::Forget(true)).unwrap();
        assert_eq!(width(newest(&rx, Told::Forget(false))), (None, Some(true)));
    }

    #[test]
    fn a_picture_and_then_word_to_forget_is_sent_only_to_those_who_were_watching_all_along() {
        let t = Instant::now();
        // Somebody watching: picture 2 is handed over, then a newer one is lost on its way
        // and the source told to forget, before it has looked at either. Picture 2 is the
        // newest there is, and goes; had only the word been kept, picture 1 would go on
        // being sent each second.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        gate.told(Some(picture(2)), Some(false), true);
        assert_eq!(sent(&mut gate, t), Some(2));
        assert_eq!(sent(&mut gate, t + SECOND), Some(2), "and again each second");
        // It is marked out of date: black, if they leave before a newer one comes.
        gate.counted(t + 2 * SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((2, 1, false)));
        // Nobody was watching: the picture was handed over, then the next was not drawn. A
        // receiver has come since (counted again, as the thread does): the picture is from
        // before it came, and is not sent to it.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.counted(t + SECOND / 2, Some(1));
        gate.told(Some(picture(3)), Some(false), false);
        assert_eq!(sent(&mut gate, t + SECOND), None);
        // A picture alone, or after the word, goes as always.
        gate.told(Some(picture(4)), None, false);
        assert_eq!(sent(&mut gate, t + SECOND), Some(4));
    }

    #[test]
    fn a_screen_made_see_through_while_nobody_watches_is_covered_with_clear() {
        let t = Instant::now();
        // A solid picture was sent; nobody watches; the screen is made see-through, and its
        // picture is not drawn: what covers the solid one is all clear, not black.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t), Some(3));
        gate.counted(t + SECOND, Some(0));
        gate.told(None, Some(true), false);
        assert_eq!(gate.blank_due(), Some((3, 1, true)));
        // Made solid again, the other way about: black.
        gate.counted(t + 2 * SECOND, Some(1));
        gate.picture(Picture::new(4, 1, Arc::new(vec![9; 16]), Sent::Straight));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(4));
        gate.counted(t + 3 * SECOND, Some(0));
        gate.told(None, Some(false), false);
        assert_eq!(gate.blank_due(), Some((4, 1, false)));
        // A see-through picture handed over just as the last receiver left, so never sent:
        // clear covers the solid one it would have replaced.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.picture(Picture::new(5, 1, Arc::new(vec![9; 20]), Sent::Premultiplied));
        gate.counted(t + SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((2, 1, true)));
        // And one handed over after they left, dropped: the same.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.counted(t + SECOND, Some(0));
        gate.picture(Picture::new(5, 1, Arc::new(vec![9; 20]), Sent::Straight));
        assert_eq!(gate.blank_due(), Some((2, 1, true)));
    }

    #[test]
    fn a_picture_gone_out_of_date_while_nobody_watches_is_covered_with_black_once() {
        let t = Instant::now();
        // NDI gives a receiver the last picture a source sent the moment it connects. While
        // that is still the newest, it is left alone.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t), Some(3));
        assert_eq!(gate.blank_due(), None);
        gate.counted(t + SECOND, Some(0));
        assert_eq!(gate.blank_due(), None, "the last receiver leaving puts nothing out of date");
        // The slide changes and is not drawn, nobody watching: black, the size of that picture.
        gate.forget();
        assert_eq!(gate.blank_due(), Some((3, 1, false)));
        gate.forget();
        assert_eq!(gate.blank_due(), None, "once: NDI has the black one now");
        // A receiver comes and is drawn a picture, leaves, and the slide changes again.
        gate.counted(t + 2 * SECOND, Some(1));
        gate.picture(picture(4));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(4));
        gate.counted(t + 3 * SECOND, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), Some((4, 1, false)));
        // A picture handed over for nobody is dropped, which puts the last one sent out of
        // date as well.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.counted(t + SECOND, Some(0));
        gate.picture(picture(5));
        assert_eq!(gate.blank_due(), Some((2, 1, false)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        // So does a picture handed over while someone watched that was still to be sent when
        // the last receiver was found gone.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.picture(picture(6));
        gate.counted(t + SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((2, 1, false)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        // A source that never sent a picture has nothing to cover.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
    }

    #[test]
    fn black_is_never_sent_over_what_someone_is_watching() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.counted(t + SECOND, Some(0));
        // Told to forget, but counted again first (as the thread does) there is a receiver:
        // it was given that picture as it connected, and a new one is being drawn for it.
        assert!(gate.counted(t + SECOND + SECOND / 2, Some(1)), "wake whoever draws");
        gate.forget();
        assert_eq!(gate.blank_due(), None);
        assert_eq!(sent(&mut gate, t + 2 * SECOND), None, "and the picture from before they came is not sent");
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(3));
        // Nor later, when that receiver leaves: what NDI has then is the newest.
        gate.counted(t + 3 * SECOND, Some(0));
        assert_eq!(gate.blank_due(), None);
        // Told to forget with a receiver there, and no new picture comes (it could not be
        // drawn, say) before the receiver leaves: black then, for whoever comes next.
        gate.counted(t + 4 * SECOND, Some(1));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t + 4 * SECOND), Some(3));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
        assert_eq!(sent(&mut gate, t + 5 * SECOND), Some(3), "what they are looking at is still sent each second");
        gate.counted(t + 5 * SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((3, 1, false)));
        assert_eq!(gate.blank_due(), None);
        // With the count unknown, someone may always be watching.
        let mut gate = Gate::new(t);
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
    }

    #[test]
    fn with_the_count_unknown_pictures_go_as_they_always_have() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.count_due(t), "counted before anything is sent");
        assert!(!gate.counted(t, None));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1), "a new picture goes at once");
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + SECOND), Some(1), "and again after a second");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 10), Some(2));
    }

    #[test]
    fn with_receivers_pictures_go_as_they_always_have() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.counted(t, Some(2)), "the first count known is told");
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + SECOND), Some(1));
        assert!(gate.counted(t + SECOND, Some(1)), "fewer receivers are told");
        assert!(!gate.counted(t + SECOND, Some(1)), "the same count again is not");
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 2), None, "and change nothing of what is sent");
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(1));
    }

    #[test]
    fn with_nobody_watching_nothing_goes_and_no_picture_is_kept() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.counted(t, Some(0)));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), None);
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + 5 * SECOND), None, "nor is anything sent again");
        assert!(gate.current.is_none(), "a picture handed over for nobody is dropped");
    }

    #[test]
    fn a_receiver_connecting_is_sent_nothing_from_before_it_came_and_the_drawing_is_woken() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        // A picture sent while someone watched, and two handed over after they left.
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert!(gate.counted(t + SECOND, Some(0)));
        assert!(gate.current.is_none(), "the picture kept is let go with the last receiver");
        gate.picture(picture(2));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 2), None);
        // A receiver connects: whoever draws is woken, and none of those pictures goes, then
        // or when it would be time to send again.
        assert!(gate.count_due(t + 2 * SECOND));
        assert!(gate.counted(t + 2 * SECOND, Some(1)), "wake whoever draws");
        assert_eq!(sent(&mut gate, t + 2 * SECOND), None, "nothing from before it came");
        assert_eq!(sent(&mut gate, t + 4 * SECOND), None, "however long the fresh one takes");
        // The picture drawn for it goes at once, then as always.
        gate.picture(picture(4));
        assert_eq!(sent(&mut gate, t + 4 * SECOND), Some(4));
        assert_eq!(sent(&mut gate, t + 4 * SECOND + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + 5 * SECOND), Some(4));
        assert!(gate.counted(t + 5 * SECOND, Some(2)), "more receivers are told too");
        assert_eq!(sent(&mut gate, t + 6 * SECOND), Some(4), "and are sent the picture kept");
    }

    #[test]
    fn a_receiver_connecting_before_any_picture_wakes_the_drawing_and_sends_nothing() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        assert!(gate.counted(t + SECOND, Some(1)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t + SECOND), Some(1), "the first picture goes when it comes");
    }

    #[test]
    fn the_count_becoming_unknown_after_none_sends_again() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.picture(picture(1));
        assert!(gate.counted(t + SECOND, None), "wake whoever draws");
        assert_eq!(sent(&mut gate, t + SECOND), None, "nothing from before");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND), Some(2));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(2), "kept and sent again, as always");
        assert!(!gate.counted(t + 2 * SECOND, None), "unknown to unknown wakes nobody");
    }

    #[test]
    fn the_last_receiver_leaving_stops_the_sending() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert!(gate.counted(t + SECOND / 2, Some(0)), "told, so the drawing for it can stop");
        assert_eq!(sent(&mut gate, t + SECOND), None, "not sent again");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND), None, "nor a new one");
    }

    #[test]
    fn pictures_coming_all_the_time_never_put_off_counting() {
        // A playing video hands over thirty pictures a second while nobody watches: the
        // receivers are still counted each second, and the thread never waits past that.
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        let mut counts = Vec::new();
        for frame in 1..=90 {
            let now = t + Duration::from_millis(33 * frame);
            gate.picture(picture(1));
            assert!(gate.wait(now) <= SECOND);
            if gate.count_due(now) {
                counts.push(now - t);
                gate.counted(now, Some(0));
            }
            assert_eq!(sent(&mut gate, now), None);
        }
        assert_eq!(counts.len(), 2, "counted at about one and two seconds: {counts:?}");
        assert!(counts.iter().zip([SECOND, 2 * SECOND]).all(|(at, due)| *at >= due && *at - due < SECOND / 10));
    }

    #[test]
    fn the_thread_waits_for_whichever_comes_first_of_counting_and_sending_again() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert_eq!(gate.wait(t), Duration::ZERO, "count at once");
        gate.counted(t, Some(0));
        gate.picture(picture(1));
        assert_eq!(gate.wait(t), SECOND, "nobody watching: only the next count");
        gate.counted(t + SECOND / 2, Some(1));
        assert_eq!(gate.wait(t + SECOND / 2), SECOND, "watched, with no picture yet: the next count");
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t + SECOND / 2), Some(1));
        assert_eq!(gate.wait(t + SECOND / 2), SECOND, "the next count and the next send coincide");
        gate.counted(t + SECOND, Some(1));
        assert_eq!(gate.wait(t + SECOND), SECOND / 2, "the next send comes first");
    }

    /// Stands in for NDI: counts as many receivers as the test says (`UNKNOWN` for a runtime
    /// that can't count), and says what it was sent: the pictures, and apart from them the
    /// black ones.
    struct Stand {
        receivers: Arc<AtomicU32>,
        sent: Sender<(u32, usize)>,
        black: Sender<u32>,
        /// Every picture's bytes and whether it went see-through.
        bytes: Sender<(Vec<u8>, bool)>,
        /// Every block of sound.
        sound: Sender<Vec<f32>>,
    }

    impl Sink for Stand {
        fn receivers(&self) -> Option<u32> {
            match self.receivers.load(Ordering::Relaxed) {
                UNKNOWN => None,
                count => Some(count),
            }
        }

        fn send(&self, width: u32, _height: u32, rgba: &[u8], alpha: bool) {
            let _ = self.bytes.send((rgba.to_vec(), alpha));
            if rgba.iter().all(|byte| *byte == 0) {
                let _ = self.black.send(width);
            } else {
                let _ = self.sent.send((width, rgba.as_ptr().addr()));
            }
        }

        fn sound(&self, planar: &[f32], _heard: std::time::SystemTime) {
            let _ = self.sound.send(planar.to_vec());
        }
    }

    /// An output sending to a stand-in, with the count it gives and what it sent: each
    /// picture's width and where its pixels are, each black picture's width, and every
    /// picture's bytes with whether it went see-through.
    struct Standing {
        output: NdiOutput,
        receivers: Arc<AtomicU32>,
        sent: Receiver<(u32, usize)>,
        black: Receiver<u32>,
        bytes: Receiver<(Vec<u8>, bool)>,
        sound: Receiver<Vec<f32>>,
    }

    fn standing(receivers: u32, wake: impl FnMut() + Send + 'static) -> Standing {
        let receivers = Arc::new(AtomicU32::new(receivers));
        let ((sent_tx, sent), (black_tx, black), (bytes_tx, bytes)) = (channel(), channel(), channel());
        let (sound_tx, sound) = channel();
        let theirs = receivers.clone();
        let open = move |_: &str| {
            Ok(Stand { receivers: theirs, sent: sent_tx, black: black_tx, bytes: bytes_tx, sound: sound_tx })
        };
        let quiet: Log = Arc::new(|_: &str, _: bool| {});
        let output = spawn("SoftPresenter test", Rate::default(), open, wake, quiet, MINUTE).unwrap();
        Standing { output, receivers, sent, black, bytes, sound }
    }

    /// Waits until the source has started and counted its receivers.
    fn counted(source: &Standing) {
        let began = Instant::now();
        while source.output.state() != State::Sending || source.output.receivers().is_none() {
            assert!(began.elapsed() < PATIENCE, "the source never started");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn sound_goes_out_at_once_from_the_thread_that_hands_it_over_while_somebody_watches() {
        let source = standing(1, || {});
        let sound = source.output.sound();
        counted(&source);
        assert!(sound.wanted());
        let block: Vec<f32> = (0..1920).map(|at| at as f32 / 1920.0).collect();
        sound.play(&block, std::time::SystemTime::now());
        // Already sent when `play` returns: no other thread is waited for.
        assert_eq!(source.sound.try_recv().unwrap(), block);
    }

    #[test]
    fn no_sound_is_sent_while_nobody_watches_or_once_the_source_has_stopped() {
        let source = standing(0, || {});
        let sound = source.output.sound();
        counted(&source);
        assert!(!sound.wanted());
        sound.play(&[0.5; 1920], std::time::SystemTime::now());
        assert!(source.sound.try_recv().is_err(), "nobody is watching");
        // Somebody comes: the next count finds them.
        source.receivers.store(2, Ordering::Relaxed);
        let began = Instant::now();
        while !sound.wanted() {
            assert!(began.elapsed() < PATIENCE);
            std::thread::sleep(Duration::from_millis(5));
        }
        sound.play(&[0.25; 1920], std::time::SystemTime::now());
        assert_eq!(source.sound.try_recv().unwrap(), [0.25; 1920]);
        // The source stops; the sound's handle is still held, and now leads nowhere.
        let Standing { output, sound: heard, .. } = source;
        drop(output);
        let began = Instant::now();
        while sound.wanted() {
            assert!(began.elapsed() < PATIENCE);
            std::thread::sleep(Duration::from_millis(5));
        }
        sound.play(&[0.75; 1920], std::time::SystemTime::now());
        assert!(heard.try_recv().is_err());
    }

    #[test]
    fn one_picture_goes_to_a_fill_and_its_key_each_as_it_asks_on_their_own_threads() {
        // The screens hand the same pixels to a see-through source and to the source that is
        // its key: one sends them as they are with their alpha, the other as greys, solid.
        let (fill, key, premultiplied) = (standing(1, || {}), standing(1, || {}), standing(1, || {}));
        let pixels = Arc::new(vec![255, 255, 255, 255, 200, 100, 50, 128, 9, 9, 9, 0]);
        fill.output.show_shared(3, 1, pixels.clone(), Sent::Straight);
        key.output.show_shared(3, 1, pixels.clone(), Sent::Key);
        premultiplied.output.show_shared(3, 1, pixels.clone(), Sent::Premultiplied);
        assert_eq!(fill.bytes.recv_timeout(PATIENCE).unwrap(), ((*pixels).clone(), true));
        assert_eq!(
            key.bytes.recv_timeout(PATIENCE).unwrap(),
            (vec![255, 255, 255, 255, 128, 128, 128, 255, 0, 0, 0, 255], false)
        );
        assert_eq!(
            premultiplied.bytes.recv_timeout(PATIENCE).unwrap(),
            (vec![255, 255, 255, 255, 100, 50, 25, 128, 0, 0, 0, 0], true)
        );
        assert_eq!(fill.sent.recv_timeout(PATIENCE).unwrap().1, pixels.as_ptr().addr(), "straight is not copied");
    }

    #[test]
    fn one_picture_shown_on_two_sources_is_sent_by_both_from_the_same_pixels() {
        let (one, two) = (standing(UNKNOWN, || {}), standing(1, || {}));
        let pixels = Arc::new(vec![9; 4 * 2 * 4]);
        let at = pixels.as_ptr().addr();
        one.output.show_shared(4, 2, pixels.clone(), Sent::Opaque);
        two.output.show_shared(4, 2, pixels.clone(), Sent::Opaque);
        assert_eq!(one.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
        assert_eq!(two.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
    }

    #[test]
    fn a_picture_shown_as_its_own_pixels_is_sent_without_a_copy() {
        let source = standing(UNKNOWN, || {});
        let pixels = vec![9; 2 * 2 * 4];
        let at = pixels.as_ptr().addr();
        source.output.show(2, 2, pixels, Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap(), (2, at));
    }

    #[test]
    fn a_picture_whose_size_and_pixels_disagree_is_ignored() {
        let source = standing(UNKNOWN, || {});
        source.output.show_shared(3, 2, Arc::new(vec![9; 5]), Sent::Opaque);
        source.output.show(0, 2, Vec::new(), Sent::Opaque);
        source.output.show(1, 1, vec![9; 4], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1, "only the good picture goes");
    }

    #[test]
    fn a_receiver_connecting_to_a_quiet_source_is_sent_only_what_is_drawn_for_it() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1), "woken for the first count, which can be read");
        source.output.show(1, 1, vec![9; 4], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        while source.sent.try_recv().is_ok() {}
        // A picture handed over for nobody is dropped; the black one says that it has been.
        source.output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 1);
        // How soon is the gate tests' to say; a busy computer may take longer here.
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1), "the new count is there for whoever is woken");
        assert_eq!(source.output.arrivals(), 1, "and that receivers have come after none");
        // Whoever was woken hands over a fresh picture. Had either picture from before been
        // kept, the thread would have sent it as it woke us, ahead of this one.
        source.output.show(3, 1, vec![9; 12], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3, "the fresh picture, and nothing before it");
    }

    #[test]
    fn a_picture_handed_over_just_as_a_receiver_connects_is_sent_not_dropped() {
        let (woken_tx, woken) = channel();
        let source = standing(0, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        // A receiver connects, and before the source next counts it is handed a picture: it
        // counts there and then, and the picture goes.
        source.receivers.store(1, Ordering::Relaxed);
        source.output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        woken.recv_timeout(PATIENCE).unwrap();
        assert!(source.black.try_recv().is_err());
    }

    #[test]
    fn a_source_is_woken_when_its_last_receiver_leaves_and_when_more_come() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(1, 1, vec![9; 4], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1);
        for receivers in [2, 0] {
            source.receivers.store(receivers, Ordering::Relaxed);
            woken.recv_timeout(PATIENCE).unwrap();
            assert_eq!(source.output.receivers(), Some(receivers));
            assert_eq!(source.output.arrivals(), 0, "more receivers, or none, are not an arrival after none");
        }
        // Woken for the count of none, the picture it had is gone: nothing more of it is sent,
        // to nobody or to whoever comes next.
        while source.sent.try_recv().is_ok() {}
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2, "only the picture drawn for the newcomer");
    }

    #[test]
    fn a_source_told_to_forget_sends_black_once_and_a_receiver_that_comes_gets_only_a_fresh_picture() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        // (The thread wakes us before it would send black, so this waits a while for it.)
        assert!(source.black.recv_timeout(SECOND / 3).is_err(), "the picture NDI has is still the newest");
        // The slide changes with nobody watching, three times: said each time, done once.
        for _ in 0..3 {
            source.output.forget();
        }
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 2, "black, the size of the picture it covers");
        while source.sent.try_recv().is_ok() {}
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(3, 1, vec![9; 12], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3);
        assert!(source.black.try_recv().is_err(), "black went once");
        // After a picture, forgetting is said and done anew.
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.forget();
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 3);
    }

    /// A new picture sent through a pace, its send returning at `at`: handed over 2 ms before
    /// and begun 1 ms before.
    fn new_at(pace: &mut Pace, at: Instant) -> Option<Hitch> {
        pace.sent_new((1920, 1080), at - Duration::from_millis(2), at - Duration::from_millis(1), at)
    }

    /// A video's pictures through a pace, one every picture's time from `from` for `count`
    /// pictures; when the last was sent.
    fn video(pace: &mut Pace, from: Instant, count: u32) -> Instant {
        let mut at = from;
        for _ in 0..count {
            assert_eq!(new_at(pace, at), None, "a picture on time is no hitch");
            at += FRAME;
        }
        at - FRAME
    }

    #[test]
    fn a_video_sent_on_time_for_a_minute_counts_nothing_late() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(2));
        let last = video(&mut pace, t + FRAME, 1799);
        assert_eq!(pace.minute_over(last), None, "not a minute yet");
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.sent, minute.repeated, minute.late, minute.skipped, minute.hitches), (1799, 0, 0, 0, 0));
        assert_eq!(minute.longest_gap, Some(FRAME));
        assert_eq!((minute.watched, minute.receivers, minute.size), (MINUTE, Some((2, 2)), Some((1920, 1080))));
        assert_eq!(minute.longest_send, Duration::from_millis(1));
        assert!(!minute.trouble());
        // The next minute starts afresh, with the receivers still there.
        let next = pace.minute_over(t + 2 * MINUTE).expect("another minute");
        assert_eq!((next.sent, next.receivers, next.watched), (0, Some((2, 2)), MINUTE));
    }

    #[test]
    fn a_slide_that_stays_is_sent_again_each_second_and_nothing_counts_as_moving() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        new_at(&mut pace, t);
        for _ in 1..60 {
            pace.sent_again(Duration::from_millis(3));
        }
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.sent, minute.repeated, minute.late, minute.hitches), (1, 59, 0, 0));
        assert_eq!(minute.longest_gap, None, "nothing moved");
        assert!(minute.words("Main").contains("nothing moving"));
    }

    #[test]
    fn a_video_that_ends_and_slides_changed_now_and_then_count_nothing_late() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        // The video plays for ten seconds and ends; a second later the slide is changed (and
        // the stage screen's time drawn a moment after it), then the slides are changed every
        // few seconds, with the time once a second.
        let ended = video(&mut pace, t, 300);
        let mut at = ended + Duration::from_secs(1);
        for _ in 0..5 {
            assert_eq!(new_at(&mut pace, at), None);
            assert_eq!(new_at(&mut pace, at + Duration::from_millis(20)), None);
            at += Duration::from_secs(1);
        }
        for _ in 0..5 {
            assert_eq!(new_at(&mut pace, at), None);
            at += Duration::from_secs(4);
        }
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.late, minute.hitches), (0, 0));
        assert_eq!(minute.longest_gap, Some(FRAME), "only the video's gaps were judged");
    }

    #[test]
    fn pictures_late_while_moving_are_counted_and_a_long_wait_is_a_hitch_with_what_held_it() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        let mut at = video(&mut pace, t, 10);
        // 60 ms: late (more than a picture and a half), but not a hitch; counted once the
        // next picture comes on time.
        at += Duration::from_millis(60);
        assert_eq!(new_at(&mut pace, at), None);
        at = video(&mut pace, at + FRAME, 5);
        // 200 ms: the picture was handed over 190 ms after the one before was sent, waited
        // 1 ms for the source's thread and took 9 ms to send. A hitch, said when the next
        // picture shows the video went on moving.
        let before = at;
        at += Duration::from_millis(200);
        let handed = before + Duration::from_millis(190);
        assert_eq!(pace.sent_new((1920, 1080), handed, handed + Duration::from_millis(1), at), None);
        assert_eq!(
            new_at(&mut pace, at + FRAME),
            Some(Hitch {
                at,
                gap: Duration::from_millis(200),
                handed_after: Duration::from_millis(190),
                waited: Duration::from_millis(1),
                took: Duration::from_millis(9),
            })
        );
        video(&mut pace, at + 2 * FRAME, 5);
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.late, minute.hitches), (2, 1));
        assert_eq!(minute.longest_gap, Some(Duration::from_millis(200)));
        assert_eq!(minute.longest_send, Duration::from_millis(9));
        assert!(minute.trouble());
    }

    #[test]
    fn a_stretch_of_late_pictures_counts_every_one_and_a_long_stop_ends_the_moving() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        let mut at = video(&mut pace, t, 10);
        // Five pictures 80 ms apart (the screens drawing at 12 a second): each late, and each
        // a hitch, said as the next comes.
        for picture in 0..5 {
            at += Duration::from_millis(80);
            assert_eq!(new_at(&mut pace, at).is_some(), picture > 0);
        }
        at += FRAME;
        assert!(new_at(&mut pace, at).is_some(), "the last of them, said as the video goes on");
        // Held up 400 ms (a fade, say): late and a hitch once the video is seen moving again,
        // three pictures on.
        at = video(&mut pace, at + FRAME, 10) + Duration::from_millis(400);
        assert_eq!(new_at(&mut pace, at), None);
        for picture in 1..=3 {
            assert_eq!(new_at(&mut pace, at + picture * FRAME).is_some(), picture == 3);
        }
        // A burst of pictures, then nothing for 450 ms, then a picture once a second (a stage
        // screen's time, after pictures drawn for it with a video's): it had stopped moving.
        at = video(&mut pace, at + 4 * FRAME, 10) + Duration::from_millis(450);
        assert_eq!(new_at(&mut pace, at), None);
        for _ in 0..3 {
            at += Duration::from_secs(1);
            assert_eq!(new_at(&mut pace, at), None);
        }
        // Nor does a video that ends, and a slide changed a second later with the time drawn
        // just after it.
        at = video(&mut pace, at + FRAME, 10) + Duration::from_secs(1);
        assert_eq!(new_at(&mut pace, at), None);
        assert_eq!(new_at(&mut pace, at + Duration::from_millis(20)), None);
        assert_eq!(new_at(&mut pace, at + Duration::from_secs(1)), None);
        // A gap of more than two seconds counts nothing at all: the video stopped.
        at = video(&mut pace, at + Duration::from_secs(2), 10);
        assert_eq!(new_at(&mut pace, at + Duration::from_millis(2500)), None);
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.late, minute.hitches), (6, 6));
        assert_eq!(minute.longest_gap, Some(Duration::from_millis(400)));
    }

    #[test]
    fn a_slower_video_is_judged_at_its_own_pace() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        // A 24 fps video for ten seconds, each picture up to 8 ms either side of its time (the
        // screens look for a new one every 8 ms): its gaps run from 34 to 50 ms, past one and
        // a half thirtieths of a second, and none is late.
        let picture = Duration::from_nanos(1_000_000_000 / 24);
        let wobble = |at: u32| Duration::from_millis(u64::from(at * 7 % 9));
        let start = t + SECOND;
        for at in 0..240 {
            assert_eq!(new_at(&mut pace, start + picture * at + wobble(at)), None);
        }
        // Held up 120 ms: late at that pace, and a hitch once the video goes on.
        let held = start + picture * 239 + wobble(239) + Duration::from_millis(120);
        assert_eq!(new_at(&mut pace, held), None);
        let hitch = (1..=3).find_map(|after| new_at(&mut pace, held + picture * after));
        assert_eq!(hitch.map(|hitch| hitch.gap), Some(held - (start + picture * 239 + wobble(239))));
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.late, minute.hitches), (1, 1));
        assert!(pace.frame > Duration::from_millis(40), "its pace is followed: {:?}", pace.frame);
    }

    #[test]
    fn a_hold_up_with_a_late_picture_beside_it_is_still_one_hitch() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(1));
        // A picture 60 ms late, then the video held up 400 ms, then on time again.
        let mut at = video(&mut pace, t, 10) + Duration::from_millis(60);
        assert_eq!(new_at(&mut pace, at), None);
        at += Duration::from_millis(400);
        let held = at;
        assert_eq!(new_at(&mut pace, at), None);
        assert_eq!(new_at(&mut pace, at + FRAME), None);
        assert_eq!(new_at(&mut pace, at + 2 * FRAME), None);
        let hitch = new_at(&mut pace, at + 3 * FRAME).expect("the hold-up, once the video is seen moving");
        assert_eq!((hitch.at, hitch.gap), (held, Duration::from_millis(400)));
        // Held up 400 ms, then a picture 60 ms late among the three on time after it.
        at = video(&mut pace, at + 4 * FRAME, 10) + Duration::from_millis(400);
        let held = at;
        assert_eq!(new_at(&mut pace, at), None);
        at += FRAME;
        assert_eq!(new_at(&mut pace, at), None);
        at += Duration::from_millis(60);
        assert_eq!(new_at(&mut pace, at), None);
        at += FRAME;
        assert_eq!(new_at(&mut pace, at).map(|hitch| (hitch.at, hitch.gap)), Some((held, Duration::from_millis(400))));
        video(&mut pace, at + FRAME, 5);
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.late, minute.hitches), (4, 2), "every late gap, and one hitch for each hold-up");
        assert_eq!(minute.longest_gap, Some(Duration::from_millis(400)));
    }

    #[test]
    fn nothing_is_counted_while_nobody_watches_and_the_line_says_so() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(0));
        pace.skipped(3);
        assert_eq!(new_at(&mut pace, t + SECOND), None);
        pace.sent_again(SECOND);
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!(minute, Minute { length: MINUTE, rate: Rate::R30, ..Minute::default() });
        assert_eq!(minute.words("Main"), "NDI \"Main\": nobody watching in the last minute");
        assert!(!minute.trouble());
    }

    #[test]
    fn part_of_a_minute_watched_is_counted_and_said() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, Some(0));
        // A receiver comes after 18 s, and another for a while; then they leave, and one
        // comes back. What moved before they left counts nothing after.
        pace.counted(t + Duration::from_secs(18), Some(1));
        let last = video(&mut pace, t + Duration::from_secs(18), 300);
        pace.counted(t + Duration::from_secs(30), Some(2));
        pace.counted(t + Duration::from_secs(40), Some(0));
        pace.skipped(4);
        pace.counted(t + Duration::from_secs(50), Some(1));
        assert!(last < t + Duration::from_secs(40));
        assert_eq!(new_at(&mut pace, t + Duration::from_millis(50_100)), None, "afresh after nobody watched");
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!(minute.watched, Duration::from_secs(32));
        assert_eq!((minute.sent, minute.skipped, minute.receivers), (301, 0, Some((1, 2))));
        let words = minute.words("Main");
        assert!(words.ends_with("in the 32 s watched of the last minute"), "{words}");
        assert!(words.contains("301 pictures sent (9.4/s)") && words.contains("receivers 1 to 2"), "{words}");
    }

    #[test]
    fn a_count_ndi_cannot_make_is_watched_and_said_not_known() {
        let t = Instant::now();
        let mut pace = Pace::new(t, MINUTE);
        pace.counted(t, None);
        pace.skipped(2);
        new_at(&mut pace, t);
        let minute = pace.minute_over(t + MINUTE).expect("a minute");
        assert_eq!((minute.watched, minute.skipped, minute.receivers), (MINUTE, 2, None));
        assert!(minute.words("Main").contains("receivers not known"));
    }

    #[test]
    fn a_rate_is_sent_as_its_exact_fraction_and_said_as_people_say_it() {
        assert_eq!(Rate::default(), Rate { n: 30_000, d: 1001 });
        let said: Vec<String> =
            [Rate::R29_97, Rate::R30, Rate::R59_94, Rate::R60, Rate::R25, Rate::R50].map(Rate::name).to_vec();
        assert_eq!(said, ["29.97", "30", "59.94", "60", "25", "50"]);
        // One picture's time: 33.367 ms at 29.97, not the 33.333 ms of thirty a second.
        assert_eq!(Rate::R29_97.frame(), Duration::from_nanos(33_366_666));
        assert_eq!(Rate::R59_94.frame(), Duration::from_nanos(16_683_333));
        assert_eq!(Rate::R30.frame(), FRAME);
        assert_eq!(Rate::R50.frame(), Duration::from_millis(20));
    }

    #[test]
    fn late_is_judged_at_the_sources_own_rate() {
        // A source that sends 59.94 a second, given a 59.94 fps video: its pace is the video's,
        // and a picture a thirtieth of a second after the one before (one was missed) is late.
        let t = Instant::now();
        let mut pace = Pace::at(t, MINUTE, Rate::R59_94);
        pace.counted(t, Some(1));
        let frame = Rate::R59_94.frame();
        let mut at = t;
        for _ in 0..120 {
            at += frame;
            assert_eq!(pace.sent_new((1920, 1080), at, at, at), None);
        }
        at += 2 * frame;
        pace.sent_new((1920, 1080), at, at, at);
        for _ in 0..10 {
            at += frame;
            pace.sent_new((1920, 1080), at, at, at);
        }
        let minute = pace.minute_over(t + MINUTE).expect("the minute is over");
        assert_eq!((minute.rate, minute.late), (Rate::R59_94, 1));
        // The same gaps on a source that sends 30 a second are its steady pace.
        let mut pace = Pace::at(t, MINUTE, Rate::R30);
        pace.counted(t, Some(1));
        let mut at = t;
        for _ in 0..60 {
            at += 2 * frame;
            pace.sent_new((1920, 1080), at, at, at);
        }
        assert_eq!(pace.minute_over(t + MINUTE).expect("the minute is over").late, 0);
    }

    #[test]
    fn a_slower_video_on_a_faster_source_is_found_steady_and_its_hold_ups_are_told() {
        // A 29.97 fps video on a source that sends 59.94 a second: every gap is two of the
        // source's picture times. Its pace is found, and a hold-up is a hitch.
        let t = Instant::now();
        let mut pace = Pace::at(t, MINUTE, Rate::R59_94);
        pace.counted(t, Some(1));
        let frame = Rate::R29_97.frame();
        let mut at = t;
        for _ in 0..60 {
            at += frame;
            pace.sent_new((1920, 1080), at, at, at);
        }
        at += Duration::from_millis(400);
        let mut hitches = u32::from(pace.sent_new((1920, 1080), at, at, at).is_some());
        for _ in 0..30 {
            at += frame;
            hitches += u32::from(pace.sent_new((1920, 1080), at, at, at).is_some());
        }
        assert_eq!(hitches, 1, "the hold-up is told once the video is seen moving again");
        let minute = pace.minute_over(t + MINUTE).expect("the minute is over");
        assert_eq!(minute.longest_gap, Some(Duration::from_millis(400)));
        assert!(minute.late < 20, "only while its pace was being found, and the hold-up: {}", minute.late);
        // A 59.94 fps video, then a 29.97 fps one on the same source: late for a moment while
        // the pace follows, then steady.
        let mut pace = Pace::at(t, MINUTE, Rate::R59_94);
        pace.counted(t, Some(1));
        let mut at = t;
        for _ in 0..120 {
            at += Rate::R59_94.frame();
            pace.sent_new((1920, 1080), at, at, at);
        }
        for _ in 0..60 {
            at += frame;
            pace.sent_new((1920, 1080), at, at, at);
        }
        let before = pace.late + pace.pending.map_or(0, |late| late.gaps);
        for _ in 0..300 {
            at += frame;
            pace.sent_new((1920, 1080), at, at, at);
        }
        assert_eq!(pace.late + pace.pending.map_or(0, |late| late.gaps), before, "steady once followed");
        assert!(before < 20, "{before}");
    }

    #[test]
    fn the_minutes_line_says_every_figure() {
        let minute = Minute {
            length: MINUTE + Duration::from_millis(400),
            watched: MINUTE,
            size: Some((1920, 1080)),
            rate: Rate::R29_97,
            sent: 1795,
            repeated: 1,
            late: 3,
            skipped: 2,
            longest_gap: Some(Duration::from_millis(412)),
            longest_send: Duration::from_micros(34_120),
            receivers: Some((2, 2)),
            hitches: 1,
            not_drawn: 4,
            passed_over: 1,
            lost: 5,
            came_back: Duration::from_micros(6_240),
        };
        assert_eq!(
            minute.words("Main Screen NDI"),
            "NDI \"Main Screen NDI\" at 1920x1080 29.97fps: 1795 pictures sent (29.9/s), 1 repeated, 3 late, 3 skipped (1 \
             of them before drawing, the GPU still reading back the one before), longest gap 412.0 ms, 4 not drawn, 5 \
             lost on the way back from the GPU (back in up to 6.2 ms), receivers 2, send took up to 34.1 ms in the \
             last minute"
        );
        let clean = Minute { late: 0, skipped: 0, hitches: 0, not_drawn: 0, passed_over: 0, lost: 0, ..minute };
        assert!(minute.trouble() && !clean.trouble());
        assert!(clean.words("Main").contains(", 0 skipped, longest gap"));
    }

    #[test]
    fn pictures_overtaken_in_the_queue_are_counted() {
        let (tx, rx) = channel();
        for width in [2, 3, 4] {
            tx.send(Told::Picture(picture(width))).unwrap();
        }
        tx.send(Told::Forget(false)).unwrap();
        let (kept, forget, overtaken) = newest(&rx, Told::Picture(picture(1)));
        assert_eq!((kept.map(|picture| picture.width), forget, overtaken), (Some(4), Some(false), 3));
        let (_, _, overtaken) = newest(&rx, Told::Forget(true));
        assert_eq!(overtaken, 0, "word to forget overtakes no picture");
    }

    #[test]
    fn hitches_are_told_at_most_every_five_seconds_with_how_many_more_there_were() {
        let source = standing(1, || {});
        let output = &source.output;
        let t = Instant::now();
        let hitch = |ms: u64| Hitch {
            at: t,
            gap: Duration::from_millis(ms),
            handed_after: Duration::ZERO,
            waited: Duration::ZERO,
            took: Duration::ZERO,
        };
        assert_eq!(output.hitch_to_tell(t), None);
        output.shared.hitched(hitch(100));
        assert_eq!(output.hitch_to_tell(t), Some((hitch(100), 0)));
        assert_eq!(output.hitch_to_tell(t), None, "told once");
        // Two more within five seconds: not told, and not told later either.
        output.shared.hitched(hitch(200));
        assert_eq!(output.hitch_to_tell(t + SECOND), None);
        output.shared.hitched(hitch(300));
        assert_eq!(output.hitch_to_tell(t + 2 * SECOND), None);
        assert_eq!(output.hitch_to_tell(t + 6 * SECOND), None, "they are long gone");
        // The next one after that is told, with the two before it.
        output.shared.hitched(hitch(400));
        assert_eq!(output.hitch_to_tell(t + 7 * SECOND), Some((hitch(400), 2)));
    }

    #[test]
    fn a_source_says_each_minute_how_its_pictures_went_out_and_keeps_the_figures() {
        let (lines_tx, lines) = channel();
        let log: Log = Arc::new(move |line: &str, problem: bool| {
            let _ = lines_tx.send((line.to_owned(), problem));
        });
        let receivers = Arc::new(AtomicU32::new(1));
        let (sent_tx, sent) = channel();
        let (black_tx, _black) = channel();
        let (bytes_tx, _bytes) = channel();
        let (sound_tx, _sound) = channel();
        let theirs = receivers.clone();
        let open = move |_: &str| {
            Ok(Stand { receivers: theirs, sent: sent_tx, black: black_tx, bytes: bytes_tx, sound: sound_tx })
        };
        let output = spawn("Main", Rate::R59_94, open, || {}, log, SECOND).unwrap();
        output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(sent.recv_timeout(PATIENCE).unwrap().0, 2);
        // What whoever draws says of the pictures goes in the line too.
        output.note(Before::Lost);
        output.note(Before::CameBack(Duration::from_millis(7)));
        let began = Instant::now();
        let (line, problem) = loop {
            let (line, problem) = lines.recv_timeout(PATIENCE).expect("a line each minute");
            if line.contains("1 lost") || began.elapsed() > PATIENCE {
                break (line, problem);
            }
        };
        assert!(line.starts_with("NDI \"Main\" at 2x1 59.94fps: 1 pictures sent"), "{line}");
        assert!(line.contains("1 lost on the way back from the GPU (back in up to 7.0 ms), receivers 1"), "{line}");
        assert!(problem, "a picture lost is a problem");
        let kept = output.last_minute().expect("the figures are kept");
        assert_eq!((kept.sent, kept.lost, kept.receivers), (1, 1, Some((1, 1))));
        // Nobody watching: the next minutes say only that.
        receivers.store(0, Ordering::Relaxed);
        let began = Instant::now();
        while !lines
            .recv_timeout(PATIENCE)
            .expect("a line each minute")
            .0
            .ends_with("nobody watching in the last minute")
        {
            assert!(began.elapsed() < PATIENCE);
        }
    }

    #[test]
    fn told_to_forget_just_as_a_receiver_connects_a_source_counts_again_and_sends_no_black() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        // A receiver connects, and before the source next counts it is told to forget: it
        // counts there and then, wakes whoever draws, and sends no black picture to the
        // receiver, now or after.
        source.receivers.store(1, Ordering::Relaxed);
        source.output.forget();
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1));
        source.output.show(3, 1, vec![9; 12], Sent::Opaque);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3, "the fresh picture, and not the old one");
        assert!(source.black.try_recv().is_err());
    }
}
