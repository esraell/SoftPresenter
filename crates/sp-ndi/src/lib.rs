//! Sends a screen's picture over the network as an NDI® source.
//!
//! NDI® is a registered trademark of Vizrt NDI AB. The NDI runtime is not part of SoftPresenter
//! and is not needed to build it or to start it: it is looked for on the computer when the first
//! NDI source starts, on that source's own thread. Without it, the source says NDI isn't
//! installed and where to get it, and everything else carries on. See `docs/licences/ndi.md`.
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
//! A source counts again, there and then, when it is handed a picture or told to forget while
//! it believes nobody is watching: a receiver may have connected since the last count. If one
//! has, the picture is sent to it rather than dropped, and no black picture is sent over what
//! it is looking at.

#[allow(unsafe_code)]
mod runtime;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use runtime::NOT_INSTALLED;

/// Where NDI can be downloaded, shown wherever NDI is chosen.
pub const DOWNLOAD_URL: &str = "https://ndi.video/";

/// How often the current picture is sent again while nothing changes. A receiver that joins
/// late, or a mixer that expects a steady source, gets a picture within this time. Each send is
/// compressed again by NDI, so this is kept slow: every 100 ms, two idle sources took about a
/// tenth of the computer's time; every second, about a fortieth.
const RESEND_EVERY: Duration = Duration::from_millis(1000);

/// How often a source asks NDI how many receivers it has. A receiver that connects to a source
/// nobody was watching gets its first picture within about this time.
const COUNT_EVERY: Duration = Duration::from_millis(1000);

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
}

impl Picture {
    fn new(width: u32, height: u32, rgba: Arc<Vec<u8>>, sent: Sent) -> Picture {
        Picture { width, height, rgba, sent, ready: matches!(sent, Sent::Opaque | Sent::Straight) }
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
}

impl Shared {
    fn new() -> Shared {
        Shared {
            state: Mutex::new(State::Starting),
            receivers: AtomicU32::new(UNKNOWN),
            arrivals: AtomicU32::new(0),
            longest_send: AtomicU64::new(0),
        }
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
        self.longest_send.fetch_max(u64::try_from(took.as_micros()).unwrap_or(u64::MAX), Ordering::Relaxed);
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
}

/// A running NDI source. Dropping it stops the source.
pub struct NdiOutput {
    name: String,
    shared: Arc<Shared>,
    tx: Option<Sender<Told>>,
    /// True from `forget` until the next picture is handed over: the source's thread is told
    /// once, however often it is said.
    forgotten: AtomicBool,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// Starts an NDI source with this name. Never waits for NDI: the source starts on its own
    /// thread, and `state` says how that went. The error is a plain sentence for the operator.
    /// Nobody is told when receivers connect: use `start_with_wake` to hand them a picture.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        NdiOutput::start_with_wake(name, || {})
    }

    /// Starts an NDI source as `start` does, which also calls `wake` each time the count of
    /// receivers is found to have changed, so that a picture can be drawn for receivers that
    /// connect after a spell with none (the source sends them nothing until one is handed
    /// over), and so that whoever shows the count can show the new one. `wake` runs on the source's own
    /// thread once the new count can be read from `receivers`, never on the caller's; it
    /// should only pass the word on and return.
    pub fn start_with_wake(name: &str, wake: impl FnMut() + Send + 'static) -> Result<NdiOutput, String> {
        spawn(name, |name| runtime::get().and_then(|runtime| runtime::Source::new(runtime, name)), wake)
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

/// Where a source's pictures go: NDI, or a stand-in in the tests.
trait Sink {
    /// How many receivers are connected, or `None` when that can't be known. Never waits.
    fn receivers(&self) -> Option<u32>;
    /// Sends one picture, see-through with `alpha` (else solid); may wait until it is time for
    /// the next.
    fn send(&self, width: u32, height: u32, rgba: &[u8], alpha: bool);
}

impl Sink for runtime::Source {
    fn receivers(&self) -> Option<u32> {
        runtime::Source::receivers(self)
    }

    fn send(&self, width: u32, height: u32, rgba: &[u8], alpha: bool) {
        runtime::Source::send(self, width, height, rgba, alpha);
    }
}

/// Starts a source's thread, which opens where its pictures go with `open` and then sends.
fn spawn<S: Sink + 'static>(
    name: &str,
    open: impl FnOnce(&str) -> Result<S, String> + Send + 'static,
    mut wake: impl FnMut() + Send + 'static,
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
                theirs.set_state(State::Sending);
                serve(&sink, &rx, &theirs, &mut wake);
                theirs.set_receivers(None);
            }
            Err(reason) => theirs.set_state(State::Failed(reason)),
        })
        .map_err(|e| format!("Could not start the NDI thread: {e}"))?;
    Ok(NdiOutput {
        name: name.to_owned(),
        shared,
        tx: Some(tx),
        forgotten: AtomicBool::new(false),
        thread: Some(thread),
    })
}

/// What has queued up comes to, `first` included: the newest picture, if there is one, and
/// whether the word to forget came after it (or with no picture at all), with whether the
/// screen was see-through then. Word to forget from before the newest picture is overtaken by it.
fn newest(rx: &Receiver<Told>, first: Told) -> (Option<Picture>, Option<bool>) {
    let (mut picture, mut forget) = (None, None);
    let mut next = Some(first);
    while let Some(told) = next {
        match told {
            Told::Picture(newer) => (picture, forget) = (Some(newer), None),
            Told::Forget(see_through) => forget = Some(see_through),
        }
        next = rx.try_recv().ok();
    }
    (picture, forget)
}

/// Sends the pictures handed over to `sink` until the output is dropped: each new one at once
/// and the current one again every second, while anyone is watching. Calls `wake` whenever the
/// count of receivers has changed.
fn serve(sink: &impl Sink, rx: &Receiver<Told>, shared: &Shared, wake: &mut impl FnMut()) {
    let mut gate = Gate::new(Instant::now());
    // Counts the receivers, and tells everyone the count before `wake`, so that whoever is
    // woken reads the new one.
    let mut count = |gate: &mut Gate| {
        let receivers = sink.receivers();
        let nobody_was = !gate.watched();
        let changed = gate.counted(Instant::now(), receivers);
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
                let (picture, forget) = newest(rx, first);
                // Word for a source nobody was watching when last counted. Someone may have
                // come since: counted again now, a picture is sent to them rather than
                // dropped, and a black one is not sent over what they are looking at.
                let watched_before = gate.watched();
                if !watched_before {
                    count(&mut gate);
                }
                gate.told(picture, forget, watched_before);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if gate.count_due(Instant::now()) {
            count(&mut gate);
        }
        // Black for a solid source; for a see-through one, all clear (zeros sent as RGBA), so
        // that a video switcher shows its camera rather than black where the words were.
        if let Some((width, height, alpha)) = gate.blank_due() {
            let began = Instant::now();
            sink.send(width, height, &vec![0; width as usize * height as usize * 4], alpha);
            shared.sent_in(began.elapsed());
        }
        if let Some(picture) = gate.picture_due(Instant::now()) {
            // Changed once, the first time it is sent, then sent again as it is each second.
            let began = Instant::now();
            picture.prepare();
            sink.send(picture.width, picture.height, &picture.rgba, picture.sent.alpha());
            shared.sent_in(began.elapsed());
        }
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
    /// time, and nothing while nobody is watching.
    fn picture_due(&mut self, now: Instant) -> Option<&mut Picture> {
        if !self.watched() || !(self.fresh || now >= self.next_resend) {
            return None;
        }
        let picture = self.current.as_mut()?;
        self.fresh = false;
        self.next_resend = now + RESEND_EVERY;
        self.given = Some((picture.width, picture.height, picture.sent.alpha()));
        Some(picture)
    }
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
        gate.picture_due(now).map(|picture| picture.width)
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
        let width = |(picture, forget): (Option<Picture>, Option<bool>)| (picture.map(|picture| picture.width), forget);
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
    }

    fn standing(receivers: u32, wake: impl FnMut() + Send + 'static) -> Standing {
        let receivers = Arc::new(AtomicU32::new(receivers));
        let ((sent_tx, sent), (black_tx, black), (bytes_tx, bytes)) = (channel(), channel(), channel());
        let theirs = receivers.clone();
        let open = move |_: &str| Ok(Stand { receivers: theirs, sent: sent_tx, black: black_tx, bytes: bytes_tx });
        let output = spawn("SoftPresenter test", open, wake).unwrap();
        Standing { output, receivers, sent, black, bytes }
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
