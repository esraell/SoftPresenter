//! The sound the app plays, gathered into one mix for whoever sends it on beside the pictures
//! (the NDI screens).
//!
//! What the speakers play is not changed, and nothing is decoded twice: every sound output the
//! app opens ([`Output`], for the videos FFmpeg plays and for the audio playlists) is tapped
//! where its samples leave for the device, after the volume, pauses and fades the operator
//! chose, and the videos the Mac's own player plays hand over the samples that player is about
//! to play (a [`Feed`] of their own). Each is an *input* of the mix.
//!
//! The mix is stereo at [`RATE`] samples a second, made once in blocks of [`BLOCK`] samples
//! (20 ms) on a thread of its own, and each block is handed to every listener ([`listen`]) at
//! the moment its sound is heard, with the time its first sample is heard: a receiver lines
//! sound and pictures up by when each was sent or by the times they carry, so a block must
//! not go early or late, and its time must not move when the thread is held up a moment. While any listener wants sound the blocks
//! never stop: silence is sent when nothing plays, so a receiver's sound is ready the moment
//! something does. While none wants it (no NDI screen with its sound on, or nobody watching
//! one) nothing is copied, mixed or sent, and the thread sleeps.
//!
//! # Keeping in step
//!
//! Every chunk of samples comes with the time it is heard. An input's samples are laid end to
//! end from where its first chunk belongs, so nothing is lost or doubled between chunks,
//! however unevenly they come (a sound card asks for them in bursts). The sound card's clock
//! and the computer's are not quite the same, though, so sample by sample an input creeps
//! away from the times its chunks say. The mix follows it: about once a second it moves its
//! own clock by however much the input's chunks have come late or early, so the blocks go out
//! at the sound card's pace and no sample is ever dropped or made up to catch up. Only a
//! real break (a pause, a seek, sound that stopped and started, an input that ran out) lays
//! an input's samples down afresh at the time they say.

mod ring;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// Samples a second, of each of the mix's two channels.
pub const RATE: u32 = 48_000;

/// Samples of each channel in one block of the mix: 20 ms.
pub const BLOCK: usize = 960;

const BLOCK_FRAMES: i64 = BLOCK as i64;

/// A chunk that comes this much later than its place in its input says is the start of
/// something new (the sound stopped for a while), not a late piece of the same sound: 100 ms.
const BREAK: i64 = RATE as i64 / 10;

/// A chunk this far ahead of its place means the clock jumped: one second.
const AHEAD: i64 = RATE as i64;

/// How much sound, at most, waits in an input for its time: three seconds.
const WAITING: usize = RATE as usize * 3;

/// How often the mix's clock is moved to an input's pace: once for each second of its sound.
const PACE_EVERY: usize = RATE as usize;

/// The mix falls this far behind the clock (its thread was held up) before it gives up what
/// it missed rather than send it late: five blocks.
const BEHIND: f64 = BLOCK as f64 * 5.0;

/// How long after a rodio output hands samples to the sound card they count as heard. The
/// card plays them a little later than it takes them, and the pictures reach NDI a little
/// after they are drawn; a block also can't go out before it is full. Measured on a Mac with
/// a clip that beeps on a flash and sp-ndi's `sync` example: at this, the beep reaches a
/// receiver within a few milliseconds of its flash on average (a picture's time either way,
/// from one flash to the next). Another system's pictures may take longer or less.
const OUTPUT_DELAY: Duration = Duration::from_millis(60);

/// How long a chunk from a tapped output is: 10 ms.
const CHUNKS_A_SECOND: u32 = 100;

/// A tapped output that has played nothing for this many chunks (a second) stops handing its
/// silence over, until it plays again.
const QUIET_CHUNKS: u32 = CHUNKS_A_SECOND;

/// Makes one input's samples stereo at [`RATE`]: one channel goes to both sides, the first two
/// of several are kept, and another rate is brought to the mix's by drawing a straight line
/// between each two samples (as the sound output itself does for the speakers).
#[derive(Debug)]
struct Conform {
    rate: u32,
    channels: u16,
    /// The frame before the next to come, and how far past it the next output sample lies,
    /// as a part of the way to that next frame.
    before: Option<[f32; 2]>,
    along: f64,
}

impl Conform {
    fn new(rate: u32, channels: u16) -> Conform {
        Conform { rate, channels, before: None, along: 0.0 }
    }

    fn feed(&mut self, samples: &[f32], out: &mut VecDeque<[f32; 2]>) {
        let channels = usize::from(self.channels.max(1));
        let frames = samples.chunks_exact(channels).map(|frame| [frame[0], *frame.get(1).unwrap_or(&frame[0])]);
        if self.rate == RATE {
            out.extend(frames);
            return;
        }
        let step = f64::from(self.rate) / f64::from(RATE);
        for frame in frames {
            let Some(before) = self.before.replace(frame) else { continue };
            while self.along < 1.0 {
                let along = self.along as f32;
                out.push_back([before[0] + (frame[0] - before[0]) * along, before[1] + (frame[1] - before[1]) * along]);
                self.along += step;
            }
            self.along -= 1.0;
        }
    }
}

/// One source of sound in the mix: its samples waiting for their time.
#[derive(Debug)]
struct Input {
    id: u64,
    /// How loud it goes into the mix (an `f32`'s bits), read as each block is made.
    gain: Arc<AtomicU32>,
    conform: Conform,
    waiting: VecDeque<[f32; 2]>,
    /// Where in the mix the first waiting sample goes; with none waiting, where the next goes.
    front: i64,
    /// False until its first chunk, and again once it has run out or been flushed: the next
    /// chunk starts afresh, at the time it says.
    running: bool,
    /// How late its chunks have come at the worst since the mix's clock was last moved, in
    /// samples (negative: all early), and how much sound that was.
    latest: Option<i64>,
    since_paced: usize,
}

/// The mix, apart from the threads and the clock: told the time, so it can be tested alone.
/// Times are counted from when the mix was made.
#[derive(Debug)]
pub struct Mix {
    inputs: Vec<Input>,
    /// How many samples have gone out: where the next block starts.
    sent: i64,
    /// The sample that goes out at time zero: a moment's place is its seconds times [`RATE`]
    /// plus this, which moves as the mix follows the pace of the sound card.
    shift: f64,
}

impl Default for Mix {
    fn default() -> Mix {
        Mix::new()
    }
}

impl Mix {
    pub fn new() -> Mix {
        Mix { inputs: Vec::new(), sent: 0, shift: 0.0 }
    }

    /// Where in the mix the sound heard at a moment goes.
    fn place(&self, at: Duration) -> f64 {
        at.as_secs_f64() * f64::from(RATE) + self.shift
    }

    /// Begins at a moment, with nothing waiting: for when somebody starts to listen.
    pub fn begin(&mut self, now: Duration) {
        self.inputs.clear();
        self.shift = 0.0;
        self.sent = self.place(now).floor() as i64;
    }

    /// When the next block is due: when the last of its sound is heard.
    pub fn due(&self) -> Duration {
        Duration::from_secs_f64((((self.sent + BLOCK_FRAMES) as f64 - self.shift) / f64::from(RATE)).max(0.0))
    }

    /// Takes a chunk of an input's sound: `samples` with its channels side by side, at `rate`,
    /// the first of them heard at `heard`. `gain` is how loud the input goes into the mix,
    /// which may change afterwards.
    pub fn feed(&mut self, id: u64, gain: &Arc<AtomicU32>, rate: u32, channels: u16, samples: &[f32], heard: Duration) {
        if rate == 0 || channels == 0 {
            return;
        }
        let want = self.place(heard).round() as i64;
        let sent = self.sent;
        let at = match self.inputs.iter().position(|input| input.id == id) {
            Some(at) => at,
            None => {
                self.inputs.push(Input {
                    id,
                    gain: gain.clone(),
                    conform: Conform::new(rate, channels),
                    waiting: VecDeque::new(),
                    front: want,
                    running: false,
                    latest: None,
                    since_paced: 0,
                });
                self.inputs.len() - 1
            }
        };
        // The first input with sound sets the pace; the others follow it.
        let leads = !self.inputs[..at].iter().any(|input| input.running);
        let input = &mut self.inputs[at];
        if input.conform.rate != rate || input.conform.channels != channels {
            input.conform = Conform::new(rate, channels);
        }
        let next = input.front + input.waiting.len() as i64;
        let late = want - next;
        if !input.running || !(-AHEAD..=BREAK).contains(&late) {
            // Afresh, at the time it says. What waited before a jump back is let go; a gap
            // before a jump on is left silent.
            if want < next {
                input.waiting.clear();
                input.front = want.max(sent);
            } else if input.waiting.is_empty() {
                input.front = want.max(sent);
            } else {
                input.waiting.extend(std::iter::repeat_n([0.0; 2], (want - next) as usize));
            }
            input.conform = Conform::new(rate, channels);
            input.running = true;
            input.latest = None;
            input.since_paced = 0;
        } else if leads {
            input.latest = Some(input.latest.map_or(late, |latest| latest.max(late)));
        } else {
            // It follows another's pace for now; should it come to lead, it starts counting then.
            input.latest = None;
            input.since_paced = 0;
        }
        let before = input.waiting.len();
        input.conform.feed(samples, &mut input.waiting);
        input.since_paced += input.waiting.len() - before;
        if input.waiting.len() > WAITING {
            let over = input.waiting.len() - WAITING;
            input.waiting.drain(..over);
            input.front += over as i64;
        }
        if leads
            && input.since_paced >= PACE_EVERY
            && let Some(latest) = input.latest.take()
        {
            // The clock moves to where this input's latest chunk was exactly on time: the
            // blocks then go out at the pace its samples come, and none is lost or made up.
            input.since_paced = 0;
            self.shift -= latest as f64;
        }
    }

    /// Lets go of what waits of an input's sound: it was paused or moved, and what was handed
    /// over ahead of its time will not be heard.
    pub fn flush(&mut self, id: u64) {
        if let Some(input) = self.inputs.iter_mut().find(|input| input.id == id) {
            input.waiting.clear();
            input.running = false;
        }
    }

    /// An input has gone.
    pub fn forget(&mut self, id: u64) {
        self.inputs.retain(|input| input.id != id);
    }

    /// Only the inputs `still` says are still there are kept.
    pub fn keep(&mut self, still: impl Fn(u64) -> bool) {
        self.inputs.retain(|input| still(input.id));
    }

    /// The next block, once its time has come: when its first sample is heard, and the left
    /// channel's [`BLOCK`] samples and then the right's, every input's sound for that time
    /// added together, silence where there is none. `None` while it is not due yet.
    pub fn block(&mut self, now: Duration) -> Option<(Duration, Vec<f32>)> {
        // (A thousandth of a sample's leeway, for a time that is the block's end to the last
        // figure a clock keeps.)
        let reached = self.place(now) + 0.001;
        if reached < (self.sent + BLOCK_FRAMES) as f64 {
            return None;
        }
        if reached - self.sent as f64 > BEHIND {
            self.sent = reached.floor() as i64 - BLOCK_FRAMES;
        }
        let sent = self.sent;
        let mut planar = vec![0.0f32; BLOCK * 2];
        for input in &mut self.inputs {
            // What should have gone out already is too late to send.
            let stale = usize::try_from(sent - input.front).unwrap_or(0).min(input.waiting.len());
            input.waiting.drain(..stale);
            input.front += stale as i64;
            let Ok(skip) = usize::try_from(input.front - sent) else { continue };
            if skip >= BLOCK {
                continue;
            }
            let take = (BLOCK - skip).min(input.waiting.len());
            let gain = f32::from_bits(input.gain.load(Ordering::Relaxed));
            for (at, [left, right]) in input.waiting.drain(..take).enumerate() {
                planar[skip + at] += left * gain;
                planar[BLOCK + skip + at] += right * gain;
            }
            input.front += take as i64;
            // Run out: it has stopped, or fallen behind. Whatever comes next starts afresh,
            // and meanwhile another input sets the pace.
            if input.waiting.is_empty() {
                input.running = false;
            }
        }
        let heard = Duration::from_secs_f64(((sent as f64 - self.shift) / f64::from(RATE)).max(0.0));
        self.sent += BLOCK_FRAMES;
        Some((heard, planar))
    }
}

/// Somebody the mix is handed to, block by block: an NDI screen's sound.
pub trait Listener: Send + Sync {
    /// Whether a block handed over now would be used. Asked often; must not wait.
    fn wanted(&self) -> bool;
    /// One block, at the moment its sound has been heard: the left channel's [`BLOCK`] samples
    /// and then the right's, at [`RATE`], and the time by the computer's clock at which the
    /// first of them was heard (20 ms ago, give or take how punctually the thread ran).
    /// Called on the mix's thread; must not wait long.
    fn play(&self, planar: &[f32], heard: SystemTime);
}

/// How often the mix's thread looks whether any listener wants sound, while none does.
const LOOK_EVERY: Duration = Duration::from_millis(500);

/// How often, at least, the mix's thread takes what the inputs have handed over: twice in
/// each block's time, well inside the time their sound is held before it is due.
const TAKE_EVERY: Duration = Duration::from_millis(10);

/// Room in each input's ring: about a second of stereo sound at 48,000 samples a second.
const RING_WORDS: usize = 1 << 17;

/// The moment the mix's times are counted from.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// What an input and the mix's thread share: the ring its samples cross in, how loud it goes
/// into the mix, and how many times it has been flushed.
#[derive(Debug)]
struct Crossing {
    id: u64,
    gain: Arc<AtomicU32>,
    flushes: AtomicU32,
    ring: ring::Ring,
}

/// The app's one mix: its inputs, its listeners, and the way to wake its thread.
struct Hub {
    wake: Sender<()>,
    /// The other end, until the thread is started (by the first listener) and takes it.
    woken: Mutex<Option<Receiver<()>>>,
    /// Whether any listener wants sound now: the taps copy nothing while it is false.
    wanted: AtomicBool,
    listeners: Mutex<Vec<(u64, Arc<dyn Listener>)>>,
    /// Every input there is. Added to and taken from where inputs are made and dropped,
    /// never on a thread that plays sound.
    inputs: Mutex<Vec<Arc<Crossing>>>,
    /// Whether a listener is expected, said ahead of its coming ([`expect`]).
    expected: AtomicBool,
    ids: AtomicU64,
}

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| {
        let (wake, woken) = channel();
        epoch();
        Hub {
            wake,
            woken: Mutex::new(Some(woken)),
            wanted: AtomicBool::new(false),
            listeners: Mutex::new(Vec::new()),
            inputs: Mutex::new(Vec::new()),
            expected: AtomicBool::new(false),
            ids: AtomicU64::new(1),
        }
    })
}

/// A listener being handed the mix; dropping it stops that.
#[must_use = "the listener is handed the mix only while this is kept"]
pub struct Listening(u64);

impl Drop for Listening {
    fn drop(&mut self) {
        let hub = hub();
        if let Ok(mut listeners) = hub.listeners.lock() {
            listeners.retain(|(id, _)| *id != self.0);
        }
        let _ = hub.wake.send(());
    }
}

/// Hands the mix to a listener from now on, until what this returns is dropped. The first
/// listener starts the mix's thread; before that, nothing here costs anything.
pub fn listen(listener: impl Listener + 'static) -> Listening {
    let hub = hub();
    let id = hub.ids.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut listeners) = hub.listeners.lock() {
        listeners.push((id, Arc::new(listener)));
    }
    if let Some(woken) = hub.woken.lock().ok().and_then(|mut woken| woken.take()) {
        let started = std::thread::Builder::new().name("sound mix".to_owned()).spawn(move || run(hub, &woken));
        if let Err(problem) = started {
            eprintln!("The sound for NDI could not be started: {problem}");
        }
    }
    let _ = hub.wake.send(());
    Listening(id)
}

/// Whether anybody is being handed the mix at all, wanted this moment or not (an NDI screen
/// has its sound on), or is expected to be. A player that must go out of its way to hand its
/// sound over only does so when this says there is someone to hand it to.
pub fn listened() -> bool {
    let hub = hub();
    hub.expected.load(Ordering::Relaxed) || hub.listeners.lock().is_ok_and(|listeners| !listeners.is_empty())
}

/// Says whether a listener is on its way (a screen is set to send sound, and its source is
/// being started on another thread), so that a player opened meanwhile hands its sound over.
pub fn expect(listener: bool) {
    hub().expected.store(listener, Ordering::Relaxed);
}

/// What the mix's thread knows of an input: how many flushes it has acted on, and how many
/// pieces it has seen left out of the ring.
struct Seen {
    id: u64,
    flushes: u32,
    lost: u64,
}

/// Takes what every input has handed over into the mix, in the order each handed it over;
/// lets go of what a flushed input had waiting, and of inputs that have gone. With `keep`
/// false everything waiting is let go instead (the mix is only beginning).
fn take_in(hub: &Hub, mix: &mut Mix, seen: &mut Vec<Seen>, samples: &mut Vec<f32>, keep: bool) {
    let inputs: Vec<Arc<Crossing>> = hub.inputs.lock().map(|inputs| inputs.clone()).unwrap_or_default();
    seen.retain(|seen| inputs.iter().any(|input| input.id == seen.id));
    mix.keep(|id| inputs.iter().any(|input| input.id == id));
    for input in &inputs {
        let flushes = input.flushes.load(Ordering::Relaxed);
        let at = seen.iter().position(|seen| seen.id == input.id).unwrap_or_else(|| {
            seen.push(Seen { id: input.id, flushes, lost: input.ring.lost() });
            seen.len() - 1
        });
        if !keep || seen[at].flushes != flushes {
            seen[at].flushes = flushes;
            input.ring.clear();
            mix.flush(input.id);
            continue;
        }
        while let Some(piece) = input.ring.take(samples) {
            mix.feed(input.id, &input.gain, piece.rate, piece.channels, samples, Duration::from_micros(piece.heard));
        }
        // Pieces left out (this thread was held up too long): the sound starts afresh after
        // the hole, rather than closing it up and running early.
        let lost = input.ring.lost();
        if lost != seen[at].lost {
            seen[at].lost = lost;
            mix.flush(input.id);
        }
    }
}

/// The mix's thread: sleeps while nobody wants sound, and otherwise hands every listener that
/// does a block each 20 ms.
fn run(hub: &Hub, woken: &Receiver<()>) {
    let began = epoch();
    let mut mix = Mix::new();
    let mut mixing = false;
    let (mut seen, mut samples) = (Vec::new(), Vec::new());
    loop {
        // The listeners as they are now, asked and handed their blocks without holding the
        // list: whoever adds or takes one away (the thread drawing the screens) never waits
        // for a block being sent.
        let listeners: Vec<Arc<dyn Listener>> = hub
            .listeners
            .lock()
            .map(|listeners| listeners.iter().map(|(_, listener)| listener.clone()).collect())
            .unwrap_or_default();
        let (any, wanted) = (!listeners.is_empty(), listeners.iter().any(|listener| listener.wanted()));
        hub.wanted.store(wanted, Ordering::Relaxed);
        if !wanted {
            mixing = false;
            // With no listener at all there is nothing to look for until one is added.
            let told = if any { woken.recv_timeout(LOOK_EVERY).map_err(|_| ()) } else { woken.recv().map_err(|_| ()) };
            if told.is_ok() {
                while woken.try_recv().is_ok() {}
            }
            continue;
        }
        if !mixing {
            mixing = true;
            mix.begin(began.elapsed());
            // Whatever the inputs handed over when sound was last wanted is long gone by.
            take_in(hub, &mut mix, &mut seen, &mut samples, false);
        }
        match woken.recv_timeout(mix.due().saturating_sub(began.elapsed()).min(TAKE_EVERY)) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        take_in(hub, &mut mix, &mut seen, &mut samples, true);
        while let Some((heard, block)) = mix.block(began.elapsed()) {
            // The block's time by the computer's clock, worked out from what that clock says
            // now: the pictures' times are read off the same clock as they are sent.
            let (now, clock) = (began.elapsed(), SystemTime::now());
            let heard = if heard <= now { clock - (now - heard) } else { clock + (heard - now) };
            for listener in listeners.iter().filter(|listener| listener.wanted()) {
                listener.play(&block, heard);
            }
        }
    }
}

/// One input of the mix: something that plays sound hands its samples over here as it plays
/// them, from the very thread that plays them. Cheap to keep while nobody listens. Dropping
/// it takes its sound out.
///
/// [`Feed::wanted`] and [`Feed::push`] are made for a sound card's thread: they never wait,
/// take no lock, ask for no memory and call nothing in the system but its clock's reading.
/// What is pushed crosses to the mix's thread in a ring of fixed size (see `ring`); should
/// the ring be full, the piece is left out of the mix and the thread goes straight on.
/// One thread at a time pushes to a feed. Making, flushing and dropping one are for other
/// threads.
#[derive(Debug)]
pub struct Feed {
    crossing: Arc<Crossing>,
}

impl Default for Feed {
    fn default() -> Feed {
        Feed::new()
    }
}

impl Feed {
    pub fn new() -> Feed {
        let hub = hub();
        let crossing = Arc::new(Crossing {
            id: hub.ids.fetch_add(1, Ordering::Relaxed),
            gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            flushes: AtomicU32::new(0),
            ring: ring::Ring::new(RING_WORDS),
        });
        if let Ok(mut inputs) = hub.inputs.lock() {
            inputs.push(crossing.clone());
        }
        Feed { crossing }
    }

    /// Whether anybody wants sound now. While nobody does, there is no need to copy samples
    /// to hand over. Never waits: asked from the threads that play the sound.
    pub fn wanted() -> bool {
        hub().wanted.load(Ordering::Relaxed)
    }

    /// Hands over samples, their channels side by side, at `rate`, the first of them heard at
    /// `heard` (now, or a little ahead). Never waits (see [`Feed`]). They are added to the
    /// mix as they are but for [`Feed::set_gain`]: hand over what is heard, after the
    /// operator's volume.
    pub fn push(&self, rate: u32, channels: u16, heard: Instant, samples: impl ExactSizeIterator<Item = f32>) {
        if !Feed::wanted() {
            return;
        }
        let heard = u64::try_from(heard.saturating_duration_since(epoch()).as_micros()).unwrap_or(u64::MAX);
        self.crossing.ring.put(ring::Piece { rate, channels, heard }, samples);
    }

    /// How loud this input goes into the mix from now on (1 as it is, 0 silent), for a
    /// player whose volume is applied after its samples are handed over. It takes hold at
    /// once, for samples handed over ahead of their time too.
    pub fn set_gain(&self, gain: f32) {
        self.crossing.gain.store(gain.max(0.0).to_bits(), Ordering::Relaxed);
    }

    /// What was handed over ahead of its time will not be heard after all (the player was
    /// paused or moved): it is let go.
    pub fn flush(&self) {
        self.crossing.flushes.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        if let Ok(mut inputs) = hub().inputs.lock() {
            inputs.retain(|input| input.id != self.crossing.id);
        }
    }
}

/// The computer's sound output, opened for something to play through, and tapped: what it
/// plays is also an input of the mix. The speakers get the same samples as without the tap.
pub struct Output {
    /// Kept open for as long as this is.
    _stream: rodio::OutputStream,
    mixer: rodio::mixer::Mixer,
    rate: u32,
}

impl Output {
    /// Opens the default sound output. The error is the system's reason.
    pub fn open() -> Result<Output, String> {
        let mut stream = rodio::OutputStreamBuilder::open_default_stream().map_err(|problem| problem.to_string())?;
        stream.log_on_drop(false);
        // What plays is mixed here first, in the output's own form (so nothing is converted
        // that wasn't before), and goes on to the output through the tap.
        let (channels, rate) = (stream.config().channel_count(), stream.config().sample_rate());
        let (mixer, mixed) = rodio::mixer::mixer(channels, rate);
        // A test copy (`SOFTPRESENTER_QUIET`) is never heard in the room.
        let speakers_silent = std::env::var_os("SOFTPRESENTER_QUIET").is_some();
        stream.mixer().add(Tap::new(mixed, channels, rate, speakers_silent));
        Ok(Output { _stream: stream, mixer, rate })
    }

    /// Where to play: connect a `rodio::Sink` here.
    pub fn mixer(&self) -> &rodio::mixer::Mixer {
        &self.mixer
    }

    /// The sound card's rate: samples a second. A source made at this rate is played as it is,
    /// with nothing converted on the sound card's thread.
    pub fn rate(&self) -> u32 {
        self.rate
    }
}

/// What an [`Output`] plays, on its way to the sound card: every sample is passed on as it
/// is, and copied for the mix, a chunk at a time, while anybody wants it. It runs on the
/// sound card's own thread, so the copying never waits, takes no lock and asks for no memory:
/// each sample goes into room kept from the start, and each full chunk into its feed's ring.
struct Tap {
    mixed: rodio::mixer::MixerSource,
    channels: u16,
    rate: u32,
    feed: Feed,
    chunk: Vec<f32>,
    chunk_len: usize,
    /// How many samples of the chunk have passed, and when its first did.
    at: usize,
    began: Instant,
    /// Whether this chunk is being copied: decided at its first sample.
    copying: bool,
    /// How many chunks in a row have been silent (counted up to [`QUIET_CHUNKS`]).
    quiet: u32,
    /// A test copy (`SOFTPRESENTER_QUIET`) sends silence to the speakers: what plays is still
    /// mixed, timed and copied for the mix (and so for NDI), but nothing is heard in the room.
    speakers_silent: bool,
}

impl Tap {
    fn new(mixed: rodio::mixer::MixerSource, channels: u16, rate: u32, speakers_silent: bool) -> Tap {
        Tap {
            mixed,
            channels,
            rate,
            feed: Feed::new(),
            // Room for a whole chunk from the start: it is filled again and again, and never
            // grows on the sound card's thread.
            chunk: Vec::with_capacity((rate / CHUNKS_A_SECOND).max(1) as usize * usize::from(channels)),
            chunk_len: (rate / CHUNKS_A_SECOND).max(1) as usize * usize::from(channels),
            at: 0,
            began: Instant::now(),
            copying: false,
            quiet: QUIET_CHUNKS,
            speakers_silent,
        }
    }
}

impl Iterator for Tap {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // With nothing playing the mixer has no sample to give; the output stays open, silent.
        let sample = self.mixed.next().unwrap_or(0.0);
        if self.at == 0 {
            self.copying = Feed::wanted();
            if self.copying {
                self.began = Instant::now();
                self.chunk.clear();
            }
        }
        if self.copying {
            self.chunk.push(sample);
        }
        self.at += 1;
        if self.at == self.chunk_len {
            self.at = 0;
            if self.copying {
                let silent = self.chunk.iter().all(|sample| *sample == 0.0);
                self.quiet = if silent { (self.quiet + 1).min(QUIET_CHUNKS) } else { 0 };
                // An output playing nothing hands nothing over: the mix is silent without it.
                if self.quiet < QUIET_CHUNKS {
                    let heard = self.began + OUTPUT_DELAY;
                    self.feed.push(self.rate, self.channels, heard, self.chunk.iter().copied());
                }
            }
        }
        Some(if self.speakers_silent { 0.0 } else { sample })
    }
}

impl rodio::Source for Tap {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain(of: f32) -> Arc<AtomicU32> {
        Arc::new(AtomicU32::new(of.to_bits()))
    }

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// Every block due by `until`, each taken as it falls due, end to end: the left channel
    /// and the right.
    fn blocks(mix: &mut Mix, until: Duration) -> (Vec<f32>, Vec<f32>) {
        let (mut left, mut right) = (Vec::new(), Vec::new());
        while mix.due() <= until {
            let (heard, block) = mix.block(mix.due()).unwrap();
            // Each block's time is where the one before it ended: the times advance with
            // the samples (but for the clock following the sound card, a few samples a second).
            let since = heard.as_secs_f64() - (mix.due().as_secs_f64() - 0.040);
            assert!(since.abs() < 1e-6, "a block's time is 20 ms before it is due");
            assert_eq!(block.len(), BLOCK * 2);
            left.extend_from_slice(&block[..BLOCK]);
            right.extend_from_slice(&block[BLOCK..]);
        }
        (left, right)
    }

    #[test]
    fn a_block_is_20_ms_of_both_channels_and_goes_out_when_its_sound_has_been_heard() {
        let mut mix = Mix::new();
        mix.begin(ms(1000));
        // Not before its last sample's time; then one block for each 20 ms, never two at once.
        assert_eq!(mix.due(), ms(1020));
        assert!(mix.block(ms(1019)).is_none());
        let (heard, first) = mix.block(ms(1020)).unwrap();
        assert_eq!(heard, ms(1000), "its time is when its first sample is heard");
        assert_eq!(first.len(), BLOCK * 2);
        assert!(mix.block(ms(1020)).is_none());
        assert_eq!(mix.due(), ms(1040));
        // Silence when nothing plays, and it keeps coming: a block for every 20 ms that pass.
        assert!(first.iter().all(|sample| *sample == 0.0));
        let (left, _) = blocks(&mut mix, ms(2020));
        assert_eq!(left.len(), BLOCK * 50);
        assert_eq!(mix.due(), ms(2040));
    }

    #[test]
    fn sound_goes_out_at_the_time_it_is_heard_left_and_right_apart() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // Stereo at the mix's rate, heard from 50 ms on: left counts up, right is its negative.
        let samples: Vec<f32> = (1..=480).flat_map(|n| [n as f32, -(n as f32)]).collect();
        mix.feed(1, &gain(1.0), RATE, 2, &samples, ms(50));
        let (left, right) = blocks(&mut mix, ms(100));
        let from = 50 * 48;
        assert!(left[..from].iter().all(|sample| *sample == 0.0));
        assert_eq!(left[from..from + 480], (1..=480).map(|n| n as f32).collect::<Vec<_>>());
        assert_eq!(right[from..from + 480], (1..=480).map(|n| -(n as f32)).collect::<Vec<_>>());
        assert!(left[from + 480..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn chunks_that_come_in_bursts_are_laid_end_to_end() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // A sound card that asks for 4,096 samples at a time: eight chunks at the same moment,
        // each saying it is heard then. They follow one another all the same.
        let mut n = 0;
        for burst in 0..4u64 {
            let now = Duration::from_secs_f64(0.1 + burst as f64 * 4096.0 / 48000.0);
            for _ in 0..8 {
                let chunk: Vec<f32> = (0..512)
                    .map(|_| {
                        n += 1;
                        n as f32
                    })
                    .collect();
                mix.feed(1, &gain(1.0), RATE, 1, &chunk, now);
            }
        }
        let (left, right) = blocks(&mut mix, ms(1000));
        let heard: Vec<f32> = left.iter().copied().filter(|sample| *sample != 0.0).collect();
        assert_eq!(heard, (1..=4 * 4096).map(|n| n as f32).collect::<Vec<_>>());
        // One channel goes to both sides.
        assert_eq!(left, right);
        // And with no gap inside: the first to the last are next to one another.
        let first = left.iter().position(|sample| *sample != 0.0).unwrap();
        assert_eq!(first, 100 * 48);
        assert_eq!(left[first + 4 * 4096 - 1], (4 * 4096) as f32);
    }

    /// Feeds ten minutes of a sound card whose clock is not the computer's, in 10 ms chunks
    /// as its samples are asked for, taking the blocks as they fall due. Gives every sample
    /// that came out.
    fn ten_minutes_at(card_rate: f64) -> Vec<f32> {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        let mut out = Vec::new();
        let mut n = 0u32;
        for chunk in 0..60_000u32 {
            // The card takes 480 samples every 480 of its own samples' time.
            let now = Duration::from_secs_f64(1.0 + f64::from(chunk) * 480.0 / card_rate);
            out.extend(blocks(&mut mix, now).0);
            let samples: Vec<f32> = (0..480)
                .map(|_| {
                    n += 1;
                    n as f32
                })
                .collect();
            // Heard a moment after the card takes them, as a tapped output's are.
            mix.feed(1, &gain(1.0), RATE, 1, &samples, now + OUTPUT_DELAY);
        }
        out.extend(blocks(&mut mix, Duration::from_secs(700)).0);
        out
    }

    #[test]
    fn a_sound_card_a_little_fast_or_slow_loses_no_sample_and_gains_none_in_ten_minutes() {
        // 48,000.5 and 47,999.5 samples a second: 300 samples out by the end, far more than a
        // block's room, and one a great deal worse (a cheap USB card: 100 parts in a million).
        for card_rate in [48_000.5, 47_999.5, 48_004.8, 47_995.2] {
            let out = ten_minutes_at(card_rate);
            let first = out.iter().position(|sample| *sample != 0.0).unwrap();
            let all = 60_000 * 480;
            for (at, sample) in out[first..first + all].iter().enumerate() {
                assert_eq!(*sample, (at + 1) as f32, "at {card_rate}, sample {at}");
            }
            assert!(out[first + all..].iter().all(|sample| *sample == 0.0));
        }
    }

    #[test]
    fn the_blocks_keep_to_the_sound_cards_pace() {
        // Ten minutes at a fast card, which plays 2,880 samples more than the computer's clock
        // counts: the mix sends as many as the card played, so what waits to go out is still
        // only the last few moments' (the 60 ms a tapped output's sound is held, and the block
        // being filled), not those and 2,880 more. The sound never falls behind its pictures,
        // which follow the same card.
        let mut mix = Mix::new();
        mix.begin(ms(0));
        let card_rate = 48_004.8;
        for chunk in 0..60_000u32 {
            let now = Duration::from_secs_f64(1.0 + f64::from(chunk) * 480.0 / card_rate);
            while mix.block(now).is_some() {}
            mix.feed(1, &gain(1.0), RATE, 1, &[0.5; 480], now + OUTPUT_DELAY);
            let waiting = mix.inputs[0].waiting.len();
            assert!(
                chunk < 100 || (2 * BLOCK..=5 * BLOCK).contains(&waiting),
                "{waiting} samples wait after chunk {chunk}"
            );
        }
    }

    #[test]
    fn an_input_that_has_run_out_leaves_the_pace_to_the_one_playing() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // The music stopped a while ago; its output is still open, handing nothing over.
        mix.feed(1, &gain(1.0), RATE, 1, &[0.5; 480], ms(20));
        let _ = blocks(&mut mix, ms(1000));
        assert!(!mix.inputs[0].running);
        // A video's sound, on a card a little fast (48,100 samples a second): the mix follows
        // it, though the music's input came first.
        for chunk in 0..300u32 {
            let now = Duration::from_secs_f64(1.0 + f64::from(chunk) * 480.0 / 48_100.0);
            while mix.block(now).is_some() {}
            mix.feed(2, &gain(1.0), RATE, 1, &[0.25; 480], now + OUTPUT_DELAY);
        }
        assert!(mix.shift > 150.0, "the clock moved {} samples on", mix.shift);
        assert!(mix.inputs[1].waiting.len() <= 5 * BLOCK);
    }

    #[test]
    fn sound_that_stops_and_starts_again_is_heard_when_it_starts_again() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        mix.feed(1, &gain(1.0), RATE, 1, &[1.0; 480], ms(100));
        // Paused for two seconds: the next chunk is not a late piece of the first.
        let _ = blocks(&mut mix, ms(2000));
        mix.feed(1, &gain(1.0), RATE, 1, &[2.0; 480], ms(2100));
        let (left, _) = blocks(&mut mix, ms(2200));
        let from = 100 * 48;
        assert!(left[..from].iter().all(|sample| *sample == 0.0));
        assert!(left[from..from + 480].iter().all(|sample| *sample == 2.0));
    }

    #[test]
    fn two_inputs_are_added_together_each_at_its_own_loudness() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        let (video, music) = (gain(1.0), gain(0.5));
        mix.feed(1, &video, RATE, 2, &[0.25; 1920], ms(20));
        mix.feed(2, &music, RATE, 1, &[0.5; 960], ms(20));
        let (left, right) = blocks(&mut mix, ms(40));
        assert!(left[BLOCK..].iter().chain(&right[BLOCK..]).all(|sample| *sample == 0.5));
        // An input that goes takes its sound with it.
        mix.feed(1, &video, RATE, 2, &[0.25; 1920], ms(40));
        mix.feed(2, &music, RATE, 1, &[0.5; 960], ms(40));
        mix.forget(1);
        let (left, _) = blocks(&mut mix, ms(60));
        assert!(left.iter().all(|sample| *sample == 0.25));
    }

    #[test]
    fn muting_takes_hold_at_once_for_sound_handed_over_ahead_of_its_time() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // A player that hands its sound over half a second before it is heard.
        let volume = gain(1.0);
        mix.feed(1, &volume, RATE, 1, &vec![0.5; 48_000], ms(500));
        let (left, _) = blocks(&mut mix, ms(600));
        assert!(left[..500 * 48].iter().all(|sample| *sample == 0.0));
        assert!(left[500 * 48..].iter().all(|sample| *sample == 0.5));
        volume.store(0.0f32.to_bits(), Ordering::Relaxed);
        let (left, _) = blocks(&mut mix, ms(700));
        assert!(left.iter().all(|sample| *sample == 0.0), "muted");
        volume.store(0.5f32.to_bits(), Ordering::Relaxed);
        let (left, _) = blocks(&mut mix, ms(800));
        assert!(left.iter().all(|sample| *sample == 0.25), "half as loud");
        // Paused: what was handed over ahead is not heard.
        mix.flush(1);
        let (left, _) = blocks(&mut mix, ms(1000));
        assert!(left.iter().all(|sample| *sample == 0.0));
        // And what comes after the pause starts afresh at its own time.
        mix.feed(1, &volume, RATE, 1, &[1.0; 480], ms(1100));
        let (left, _) = blocks(&mut mix, ms(1200));
        assert_eq!(left.iter().position(|sample| *sample != 0.0), Some(100 * 48));
    }

    #[test]
    fn another_rate_is_brought_to_48000_keeping_its_pitch_and_loudness() {
        // One second of a 441 Hz tone at 44,100 samples a second: 100 samples a cycle.
        let tone: Vec<f32> = (0..44_100).map(|n| (n as f32 * std::f32::consts::TAU / 100.0).sin() * 0.5).collect();
        let mut conform = Conform::new(44_100, 1);
        let mut out = VecDeque::new();
        // In uneven pieces, as it would come.
        for piece in tone.chunks(441) {
            conform.feed(piece, &mut out);
        }
        // A second of sound is a second of sound: 48,000 samples, give or take the last one.
        assert!((47_998..=48_000).contains(&out.len()), "{} samples", out.len());
        // The same tone: 441 cycles in the second, as loud as it was.
        let left: Vec<f32> = out.iter().map(|frame| frame[0]).collect();
        let crossings = left.windows(2).filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0).count();
        assert!((440..=441).contains(&crossings), "{crossings} cycles");
        let peak = left.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!((0.49..=0.5).contains(&peak), "peak {peak}");
        assert!(out.iter().all(|frame| frame[0] == frame[1]));
        // Every sample close to the tone itself at that moment.
        for (n, sample) in left.iter().enumerate().step_by(97) {
            let exact = (n as f32 / 48_000.0 * 441.0 * std::f32::consts::TAU).sin() * 0.5;
            assert!((sample - exact).abs() < 0.002, "sample {n}: {sample} for {exact}");
        }
    }

    #[test]
    fn the_mix_takes_sound_at_another_rate_and_with_more_channels() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // Half a second at 44,100, six channels: the first two are kept.
        let samples: Vec<f32> = (0..22_050).flat_map(|_| [0.5, -0.5, 9.0, 9.0, 9.0, 9.0]).collect();
        mix.feed(1, &gain(1.0), 44_100, 6, &samples, ms(100));
        let (left, right) = blocks(&mut mix, ms(1000));
        let sounding = left.iter().filter(|sample| **sample == 0.5).count();
        assert!((23_998..=24_000).contains(&sounding), "{sounding} samples");
        assert_eq!(right.iter().filter(|sample| **sample == -0.5).count(), sounding);
    }

    #[test]
    fn a_mix_held_up_gives_up_what_it_missed_rather_than_send_it_late() {
        let mut mix = Mix::new();
        mix.begin(ms(0));
        // Nothing asked for two seconds (the computer slept): one block comes, the newest.
        assert!(mix.block(ms(2000)).is_some());
        assert!(mix.block(ms(2000)).is_none());
        assert_eq!(mix.due(), ms(2020));
    }

    #[test]
    fn the_tap_passes_every_sample_on_as_it_is_and_plays_silence_when_nothing_does() {
        // What an output plays goes through the tap on its way to the sound card: the very
        // same samples come out, in order, whether or not anybody wants a copy.
        let (mixer, mixed) = rodio::mixer::mixer(2, 44_100);
        let mut tap = Tap::new(mixed, 2, 44_100, false);
        // Nothing playing yet: silence, never the end of the sound.
        assert_eq!(tap.by_ref().take(10).collect::<Vec<f32>>(), [0.0; 10]);
        let played: Vec<f32> = (0..9000).map(|n| ((n % 200) as f32 - 100.0) / 128.0).collect();
        mixer.add(rodio::buffer::SamplesBuffer::new(2, 44_100, played.clone()));
        assert_eq!(tap.by_ref().take(9000).collect::<Vec<f32>>(), played);
        assert_eq!(tap.by_ref().take(2000).collect::<Vec<f32>>(), [0.0; 2000]);
        assert_eq!((rodio::Source::channels(&tap), rodio::Source::sample_rate(&tap)), (2, 44_100));
    }

    #[test]
    fn a_test_copys_speakers_get_silence_while_the_sound_still_plays_through() {
        // In a test copy the sound card hears nothing, though the sound plays on as before:
        // the mixer is read sample by sample, at the same pace, and nothing is left behind.
        let (mixer, mixed) = rodio::mixer::mixer(2, 44_100);
        let mut tap = Tap::new(mixed, 2, 44_100, true);
        let played: Vec<f32> = (0..9000).map(|n| ((n % 200) as f32 - 100.0) / 128.0).collect();
        mixer.add(rodio::buffer::SamplesBuffer::new(2, 44_100, played));
        assert_eq!(tap.by_ref().take(9000).collect::<Vec<f32>>(), vec![0.0; 9000]);
        assert_eq!(tap.mixed.next(), None, "every sample was taken from the mixer");
    }

    /// Runs `work`, and gives with what it returns how late, at the most, a timed wait of
    /// `wait` (on a channel, as the mix's thread waits) woke on another thread meanwhile: how
    /// late this computer wakes such a wait now, and so the mix's thread's. How late depends
    /// on how long the wait is, so the wait is the mix's.
    fn with_latest_wake<T>(wait: Duration, work: impl FnOnce() -> T) -> (T, Duration) {
        let done = Arc::new(AtomicBool::new(false));
        let theirs = done.clone();
        let waiting = std::thread::spawn(move || {
            let (_keep, never) = channel::<()>();
            let mut latest = Duration::ZERO;
            while !theirs.load(Ordering::Relaxed) {
                let began = Instant::now();
                let _ = never.recv_timeout(wait);
                latest = latest.max(began.elapsed().saturating_sub(wait));
            }
            latest
        });
        let done_with = work();
        done.store(true, Ordering::Relaxed);
        (done_with, waiting.join().unwrap_or_default())
    }

    /// Says that a part of a test was not judged here and why, where it is seen though the test
    /// passes (straight to the standard error, which the test harness doesn't hold back), and
    /// on GitHub as a notice on the run.
    fn say_skipped(what: &str, why: &str) {
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), "sp-video: skipped {what}: {why}");
        if std::env::var_os("GITHUB_ACTIONS").is_some() {
            let _ = writeln!(std::io::stdout(), "::notice title=Timing not judged::{what}: {why}");
        }
    }

    struct Heard {
        on: Arc<AtomicBool>,
        blocks: Sender<(Vec<f32>, SystemTime)>,
    }

    impl Listener for Heard {
        fn wanted(&self) -> bool {
            self.on.load(Ordering::Relaxed)
        }
        fn play(&self, planar: &[f32], heard: SystemTime) {
            let _ = self.blocks.send((planar.to_vec(), heard));
        }
    }

    #[test]
    fn listeners_are_handed_the_mix_while_they_want_it_and_nothing_is_copied_while_none_does() {
        // The app's one mix, with its thread: this is the only test that uses it.
        let (on, (tx, heard)) = (Arc::new(AtomicBool::new(false)), channel());
        let feed = Feed::new();
        assert!(!Feed::wanted(), "nobody listens yet");
        let listening = listen(Heard { on: on.clone(), blocks: tx });
        // Switched off: nothing is asked of the taps, and nothing comes.
        std::thread::sleep(ms(100));
        assert!(!Feed::wanted());
        feed.push(RATE, 1, Instant::now(), [0.5; 480].into_iter());
        assert!(heard.recv_timeout(ms(100)).is_err());
        // Switched on: blocks come, steadily, silent until something plays.
        let block = Duration::from_secs_f64(BLOCK as f64 / f64::from(RATE));
        let switched_on = SystemTime::now();
        on.store(true, Ordering::Relaxed);
        let (first, first_at) = heard.recv_timeout(ms(2000)).unwrap();
        let came = SystemTime::now();
        assert!(Feed::wanted());
        // Its time is the computer's clock's, when its first sample was heard: after it was
        // switched on, and a block's time before it came (it goes once its sound has been
        // heard), however late the thread woke to send it. (A couple of milliseconds for the
        // clock's own steps.)
        assert!(first_at + ms(2) >= switched_on, "its time {first_at:?} is before it was switched on, {switched_on:?}");
        assert!(
            first_at + block <= came + ms(2),
            "its time {first_at:?} is less than a block before it came, {came:?}"
        );
        assert_eq!(first.len(), BLOCK * 2);
        assert!(first.iter().all(|sample| *sample == 0.0));
        // A thread held up longer than this gives up what it missed (`Mix::block`, and
        // `a_mix_held_up_gives_up_what_it_missed_rather_than_send_it_late`).
        let behind = Duration::from_secs_f64(BEHIND / f64::from(RATE));
        let ((gave_up, took), late) = with_latest_wake(block, || {
            let began = Instant::now();
            let mut before = first_at;
            let mut gave_up = 0;
            for _ in 0..25 {
                let (_, at) = heard.recv_timeout(ms(2000)).unwrap();
                // Each block's time is 20 ms on from the one before, however late the thread
                // woke to send it (the blocks due then go at once, one after another); unless
                // it was held up past `behind`, when the next is the block of a moment ago.
                // Never anything between.
                let step = at.duration_since(before).unwrap_or_default();
                if step >= behind {
                    gave_up += 1;
                } else {
                    assert!(step > ms(15) && step < ms(25), "{step:?}");
                }
                before = at;
            }
            (gave_up, began.elapsed())
        });
        // 25 blocks are half a second of sound, and none goes before its sound has been heard.
        assert!(took > ms(400), "{took:?}");
        // Where the thread wakes in time they come at its pace, none given up. Each wake is a
        // timed wait of up to a block's time, and a block is given up only when one wakes more
        // than `behind` less a block late: where a wait of a block's time wakes over half that
        // late (on GitHub's virtual Macs up to 150 ms), the pace is not the mix's doing, and
        // isn't judged.
        let paced = took < ms(1500) && gave_up == 0;
        if !paced && late > (behind - block) / 2 {
            say_skipped(
                "listeners_are_handed_the_mix_while_they_want_it_and_nothing_is_copied_while_none_does: the pace",
                &format!(
                    "25 blocks took {took:.0?}, {gave_up} of them after a hold-up given up, the thread's waits waking late here (a wait of {block:.0?} woke up to {late:.0?} late)"
                ),
            );
        } else {
            assert!(
                paced,
                "25 blocks took {took:?}, {gave_up} given up (a wait of {block:?} woke up to {late:?} late here)"
            );
        }
        // What is handed over is heard, at its loudness: half a second of it, so that a
        // hold-up given up meanwhile can't take all of it.
        feed.set_gain(0.5);
        feed.push(RATE, 1, Instant::now() + ms(50), [0.5; 24_000].into_iter());
        let sounded = (0..100).any(|_| heard.recv_timeout(ms(2000)).unwrap().0.contains(&0.25));
        assert!(sounded);
        // Switched off again: once the thread has seen it, the blocks stop.
        on.store(false, Ordering::Relaxed);
        let began = Instant::now();
        while Feed::wanted() {
            assert!(began.elapsed() < ms(5000), "the mix's thread never saw nobody wants it");
            std::thread::sleep(ms(10));
        }
        while heard.try_recv().is_ok() {}
        assert!(heard.recv_timeout(ms(200)).is_err());
        drop(listening);
    }
}
