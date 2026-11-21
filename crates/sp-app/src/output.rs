//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sp_model::{Slide, Theme, Uuid};
use sp_ndi::NdiOutput;
use sp_render::{
    Fonts, Layers, MovingPicture, Painter, Picture, build_layers, read_pixels, target_texture, vello, wgpu,
};

/// How often a playing video is looked at for a new picture: more often than any video's
/// frame rate.
const VIDEO_POLL: Duration = Duration::from_millis(8);
/// How often a video that has given no new picture for a second (paused, or ended and left on
/// screen) is looked at: a picture after it starts again is at most this late.
const VIDEO_IDLE_POLL: Duration = Duration::from_millis(50);

/// Everything the output needs to draw one picture. Immutable once sent.
#[derive(Clone)]
pub struct Frame {
    /// The picture behind the words, if any. For a video, a still from it, shown until its
    /// first picture comes.
    pub media: Option<Picture>,
    /// The video playing behind the words, where its pictures are taken from.
    pub video: Option<sp_video::Frames>,
    /// How the media fills the screen.
    pub media_fit: sp_model::ImageFit,
    /// A video in a box on the slide, by the file the box names, and where its pictures come
    /// from: each new one is shown in the box in place of its still.
    /// The videos in boxes on the slide, by their files, each with where its pictures come from.
    pub slide_video: Vec<(String, sp_video::Frames)>,
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// When false the output is black whatever the slide is.
    pub enabled: bool,
    /// How the picture before gives way to this one.
    pub transition: sp_model::Transition,
    /// How the media before gives way to this, when only the media changed.
    pub media_transition: sp_model::Transition,
    /// The slide after the one on screen, for stage screens.
    pub next: Option<Arc<Slide>>,
    /// The song on screen's title and the slide's group, for stage screens.
    pub about: crate::stage::About,
    /// How many of the slide's click builds have been set going.
    pub build_step: usize,
}

impl Frame {
    /// True while a video plays, as the media or in a box on the slide.
    fn plays_video(&self) -> bool {
        self.video.is_some() || !self.slide_video.is_empty()
    }
}

/// What is shown over the slides: a message on the audience screens; the stage message and the
/// timers on the stage screens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// The message's words, tokens and all (they are filled in as it is drawn), and its look.
    pub message: Option<(String, crate::messages::Look)>,
    pub stage_message: Option<String>,
    pub timers: Vec<crate::timers::Timer>,
    /// The props on screen, in the order they were put on.
    pub props: Vec<Slide>,
    /// The copyright line of the song on screen, when it is to be shown.
    pub copyright: Option<String>,
    /// A test pattern shown on the audience screens in place of everything else.
    pub test_pattern: Option<crate::props::TestPattern>,
}

pub enum OutputMsg {
    Show(Frame),
    /// A window to draw a screen in. The thread takes ownership of its surface.
    AddSurface {
        id: Uuid,
        surface: wgpu::Surface<'static>,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    },
    /// A screen's window changed size, in pixels.
    Resized(Uuid, u32, u32),
    /// The textures the live preview is drawn into, for the window to show while a video plays.
    PreviewTargets(Vec<wgpu::Texture>),
    /// Draw the current frame again: a window was uncovered or asked to be redrawn.
    Redraw,
    /// Send a screen over NDI under this name, or stop with `None`. Sent again with another
    /// name, it is renamed.
    SetNdi {
        id: Uuid,
        name: Option<String>,
    },
    /// An NDI source has found that its count of receivers changed: the first receivers want a
    /// picture drawn for them, and a source the last one left is drawn for no longer. Sent by
    /// the sources' own threads.
    NdiReceivers,
    /// Stop drawing in a screen's window.
    RemoveSurface(Uuid),
    /// What the Announcements layer shows now, or nothing.
    Announcement(Option<Announcement>),
    /// What is shown over the slides.
    Overlay(Overlay),
    /// How each audience screen that has a look of its own shows the show.
    Looks(Vec<(Uuid, crate::screens::Look)>),
    /// The audience screens drawn with a theme of their own, and the theme.
    Themes(Vec<(Uuid, Arc<Theme>)>),
    /// Which screens are stage screens and the layout each is drawn with, and whether stage
    /// screens are on.
    Stage {
        screens: Vec<(Uuid, Arc<Theme>)>,
        on: bool,
    },
    /// Show each screen's own name on it, so the operator can tell them apart; an empty list
    /// goes back to the show.
    Identify(Vec<(Uuid, String, sp_model::Rgba)>),
    Shutdown,
}

/// What an NDI output is doing, for the operator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NdiStatus {
    #[default]
    Off,
    /// On the network under this name, with this many receivers connected when NDI can count
    /// them. It sends only while there are some.
    On(String, Option<u32>),
    /// Asked for, but it could not start; the reason in plain words.
    Failed(String),
}

impl NdiStatus {
    /// What a source that is on says of itself in the Screens window and to `sp screens`. The
    /// number is NDI's own: its monitor app, for one, counts as two receivers. Kept short: the
    /// Screens window shows it on one line inside a small picture of the screen.
    pub fn sending_in_words(name: &str, receivers: Option<u32>) -> String {
        let (w, h) = NDI_SIZE;
        match receivers {
            None => format!("Sending as \"{name}\" at {w} x {h}"),
            Some(0) => format!("On NDI as \"{name}\"; nobody is watching"),
            Some(1) => format!("Sending as \"{name}\" to 1 receiver"),
            Some(count) => format!("Sending as \"{name}\" to {count} receivers"),
        }
    }
}

/// An NDI picture being copied back from the GPU: the sources it is for, its place, the buffer it is
/// copied into (each row padded as the GPU requires), and whether the copy has come back (and
/// worked).
struct NdiReadback {
    ids: Vec<Uuid>,
    /// Which picture of its drawing it is, from nought.
    place: usize,
    buffer: wgpu::Buffer,
    padded_row: usize,
    done: Arc<Mutex<Option<bool>>>,
}

/// How soon the idle output thread looks at the GPU again while it still holds memory the
/// painter gave back.
const GPU_LOOK_AGAIN: Duration = Duration::from_millis(20);

/// After a drawing of the screens in which the GPU was too slow to work a picture out, they
/// are drawn again this long after, up to this many times one after another. Each try may
/// itself wait four tenths of a second for the GPU, so that is up to about three seconds in
/// all, with the picture before on the screens meanwhile.
const DRAW_AGAIN_AFTER: Duration = Duration::from_millis(150);
const DRAW_AGAIN_TRIES: u8 = 5;

/// After those tries the screens are given up on for now and the operator is told, and they
/// are drawn again this often for as long as the picture is still wanted: a GPU that was stuck
/// may answer again. Each such try holds the thread for one wait of four tenths of a second
/// at most. They are not the only drawings, though: with a stage screen showing the time, the
/// stage screens are drawn at the start of every second, so behind a GPU that stays stuck the
/// thread waits four tenths of every second.
const GIVEN_UP_AGAIN_AFTER: Duration = Duration::from_secs(5);

/// How long the thread waits after a drawing in which a picture could not be painted and
/// nothing was shown in any window. Showing a picture waits for the screen's next refresh,
/// which is what paces a fade, a build or scrolling words; without it they would spin. About
/// one refresh of a 60 Hz screen.
const UNDRAWN_PAUSE: Duration = Duration::from_millis(16);

/// Why a screen is not showing what it was last asked to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Undrawn {
    /// The GPU did not work the picture out in time, every time it was tried. The screen shows
    /// whatever it last drew: the picture before, or a step of the fade towards this one.
    NotInTime = 1,
    /// The renderer could not draw it, for a reason the log has.
    Failed = 2,
    /// There is too much to draw (on a slide, or in a stage layout).
    TooMuch = 3,
}

impl Undrawn {
    fn from_code(code: u8) -> Option<Undrawn> {
        [Undrawn::NotInTime, Undrawn::Failed, Undrawn::TooMuch].into_iter().find(|undrawn| *undrawn as u8 == code)
    }

    /// What to tell the operator, by the Program preview. True of an audience screen, a stage
    /// screen, an NDI picture and the preview itself, and of a fade that stopped part-way.
    pub fn in_words(self) -> &'static str {
        match self {
            Undrawn::NotInTime => "A screen could not draw this in time, and is not showing it. It will try again.",
            Undrawn::Failed => "A screen could not draw this, and is not showing it.",
            Undrawn::TooMuch => "There is too much here for a screen to draw. It is not showing it.",
        }
    }
}

/// How one drawing of the screens (every window, NDI and the live preview) ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ended {
    /// No picture was given up for a slow GPU. (One may still have been refused as too heavy.)
    Drawn,
    /// The GPU was too slow with a picture: the screens are drawn again shortly.
    TryAgain,
    /// The GPU was too slow again, after every try allowed: the operator is told, and the
    /// screens are drawn again now and then.
    GivenUp,
}

/// What became of the pictures painted in one drawing of the screens, and what follows from
/// the drawings before it. One drawing paints many pictures, each of which may wait for the
/// GPU; this is what keeps a GPU that is stuck from holding the thread once for each of them,
/// what counts the tries, and what says when to draw again. A drawing is any of them: every
/// screen, a step of a fade, the stage screens at the start of a second.
#[derive(Debug, Default)]
struct Round {
    /// True once the GPU has been too slow with a picture of this drawing: the pictures after
    /// it are not asked for.
    slow: bool,
    /// True when any picture of this drawing was not painted.
    failed: bool,
    /// True when a picture of this drawing was shown in a window.
    presented: bool,
    /// True when this drawing is of some of the screens only, or of the NDI picture alone.
    part: bool,
    /// How many drawings one after another have been tried again for a slow GPU.
    tries: u8,
    /// When to draw the screens again, if nothing else draws them first.
    again_at: Option<Instant>,
}

impl Round {
    /// A drawing begins: whatever was to be drawn again is being drawn now.
    fn begin(&mut self) {
        *self = Round { tries: self.tries, ..Round::default() };
    }

    /// A drawing of some of what is drawn begins (the stage screens at the start of a second,
    /// an NDI picture that had to wait for the one before it): the time set for drawing
    /// everything again stands, and so does the count of tries, however this drawing goes
    /// well. It says nothing of the screens it did not draw.
    fn begin_part(&mut self) {
        *self = Round { part: true, tries: self.tries, again_at: self.again_at, ..Round::default() };
    }

    /// The picture on the screens is another one: it gets all its tries.
    fn new_picture(&mut self) {
        self.tries = 0;
    }

    /// Notes how painting one picture went.
    fn note(&mut self, painted: &Result<(), sp_render::PaintError>) {
        if let Err(error) = painted {
            self.failed = true;
            self.slow |= error.worth_trying_again();
        }
    }

    /// The drawing is over. After one the GPU was too slow in, a time is always set to draw
    /// again, whatever the screens are doing: a fade or a build that draws them sooner begins
    /// a drawing of its own, which takes the place of that one.
    fn end(&mut self, now: Instant) -> Ended {
        if !self.slow {
            if !self.part {
                self.tries = 0;
            }
            return Ended::Drawn;
        }
        let (ended, after) = if self.tries >= DRAW_AGAIN_TRIES {
            (Ended::GivenUp, GIVEN_UP_AGAIN_AFTER)
        } else {
            self.tries += 1;
            (Ended::TryAgain, DRAW_AGAIN_AFTER)
        };
        // A drawing of part of the screens never puts off the time already set for all of
        // them: stage screens drawn every second would put it off for good.
        let at = now + after;
        self.again_at = Some(self.again_at.filter(|_| self.part).map_or(at, |set| set.min(at)));
        ended
    }
}

/// Whether one thing the thread draws (a window, the NDI picture, the live preview) shows what
/// it was last asked to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shown {
    /// Why it does not, when the operator is to be told.
    undrawn: Option<Undrawn>,
    /// True while its picture is to be tried again after a slow GPU.
    waiting: bool,
}

impl Shown {
    /// Its picture was painted.
    fn painted(&mut self) {
        *self = Shown::default();
    }

    /// Its picture was not painted.
    fn failed(&mut self, error: &sp_render::PaintError) {
        match error {
            sp_render::PaintError::TooHeavy => *self = Shown { undrawn: Some(Undrawn::TooMuch), waiting: false },
            sp_render::PaintError::Failed(_) => *self = Shown { undrawn: Some(Undrawn::Failed), waiting: false },
            // Too slow: to be tried again. What the operator was told before still holds.
            sp_render::PaintError::NotNow(_) => self.waiting = true,
        }
    }

    /// Its picture was not asked for, the GPU having been too slow with one just before.
    fn skipped(&mut self) {
        self.waiting = true;
    }

    /// No picture is asked of it any more (the NDI sources, once nobody is watching any of
    /// them): nothing is owed, whatever went wrong before, and nothing is waited for.
    fn not_asked(&mut self) {
        *self = Shown::default();
    }

    /// The tries are used up: a picture still waiting is not coming for now.
    fn given_up(&mut self) {
        if std::mem::take(&mut self.waiting) {
            self.undrawn = Some(Undrawn::NotInTime);
        }
    }
}

/// The worst of what the things drawn are not showing, for telling the operator.
fn worst_undrawn(shown: impl IntoIterator<Item = Shown>) -> Option<Undrawn> {
    shown.into_iter().filter_map(|shown| shown.undrawn).max()
}

/// Testing aid: SOFTPRESENTER_GPU_TOO_SLOW=N (or N@S) has the output's painter give up the next
/// N pictures it has to wait for the GPU for (starting S seconds after the start), as it does
/// when the GPU is too slow; pictures it has drawn before and need not wait for are still
/// drawn.
#[derive(Debug, PartialEq)]
struct PretendSlow {
    left: u32,
    after: Duration,
}

impl PretendSlow {
    /// What the switch asks for; `None` for anything that is not a count, or a count and a
    /// number of seconds from nought up.
    fn parse(asked: &str) -> Option<PretendSlow> {
        let (count, after) = asked.split_once('@').unwrap_or((asked, "0"));
        let after = Duration::try_from_secs_f32(after.trim().parse().ok()?).ok()?;
        Some(PretendSlow { left: count.trim().parse().ok()?, after })
    }
}

/// Testing aid: SOFTPRESENTER_OUTPUT_ROOM=LINES keeps the output painter's working memory at
/// room for that many lines and segments whatever is drawn, so that a slide with more on it
/// is too heavy for the screens to draw (a few lines of large text make tens of thousands).
/// `None` for anything but a number from one to eight million, which is as much as fits in
/// what one picture may take.
fn output_room(asked: &str) -> Option<vello::ScratchSizes> {
    let lines = asked.trim().parse::<u32>().ok().filter(|lines| (1..=8_000_000).contains(lines))?;
    Some(vello::ScratchSizes { lines, seg_counts: lines, segments: lines, ..vello::ScratchSizes::RESTING })
}

/// The size of the picture sent over NDI, whatever size any window is.
const NDI_SIZE: (u32, u32) = (1920, 1080);

/// How the NDI pictures asked for now may differ from the ones last asked for. It decides
/// which sources nobody is watching are told that the picture they last sent is out of date
/// (see `Output::send_ndi`). In order: a later one covers the ones before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NdiChange {
    /// Not at all: they are drawn only because receivers came or went, or for a new source.
    Nothing,
    /// The stage screens' may: they are drawn again for the time, at the start of a second.
    Stage,
    /// Any of them may.
    Any,
}

impl NdiChange {
    /// Whether a source's picture may differ; `stage` says the source is a stage screen.
    fn touches(self, stage: bool) -> bool {
        match self {
            NdiChange::Nothing => false,
            NdiChange::Stage => stage,
            NdiChange::Any => true,
        }
    }

    /// With a drawing that was put off while a picture was on its way back from the GPU: as
    /// different from the last as the more different of the two. The one put off is not
    /// forgotten when the next is drawn, or not drawn, in its place.
    fn with(self, put_off: Option<NdiChange>) -> NdiChange {
        put_off.map_or(self, |before| before.max(self))
    }
}

/// One NDI source as a drawing finds it: whether a picture is wanted for it, and whether it
/// shows the show its own way (a stage screen, one being identified, one with a look or a
/// theme of its own) and so needs a picture of its own.
#[derive(Clone, Copy, Debug)]
struct NdiSeen {
    id: Uuid,
    wanted: bool,
    alone: bool,
    /// NDI says nobody is watching it (which is one reason no picture is wanted).
    unwatched: bool,
    /// How many times receivers have come to it after a spell with none.
    arrivals: u32,
    /// It is a stage screen.
    stage: bool,
}

/// The sources to tell that the picture they last sent is out of date: those nobody is
/// watching whose picture may have changed. They are not drawn for, so NDI would give that
/// picture to whoever connects next.
fn ndi_out_of_date(sources: &[NdiSeen], changed: NdiChange) -> Vec<Uuid> {
    sources.iter().filter(|source| source.unwatched && changed.touches(source.stage)).map(|source| source.id).collect()
}

/// Takes out of the list of sources a picture on its way back from the GPU is for those no
/// picture is wanted for any more, and gives them: they would throw it away, and it was newer
/// than the one they have, so they are told that one is out of date.
fn ndi_strip(ids: &mut Vec<Uuid>, sources: &[NdiSeen]) -> Vec<Uuid> {
    let unwanted = |id: &Uuid| sources.iter().any(|source| source.id == *id && !source.wanted);
    let stripped: Vec<Uuid> = ids.iter().copied().filter(unwanted).collect();
    ids.retain(|id| !unwanted(id));
    stripped
}

/// Whether a picture lost on its way back from the GPU is still owed to anyone: to a source
/// it was for that a picture is still wanted for. Lost for sources nobody is watching any
/// more, it is not undrawn, and not drawn again.
fn ndi_lost_is_owed(ids: &[Uuid], sources: &[NdiSeen]) -> bool {
    sources.iter().any(|source| source.wanted && ids.contains(&source.id))
}

/// How things stand with NDI pictures lost on their way back from the GPU, counted by whole
/// drawings: every picture of one is back, or lost, before the next is painted. The first
/// drawing to lose a picture somebody is watching for is drawn once more at once. If that
/// loses one too, the operator is told, and the pictures are drawn again every five seconds
/// (as a picture the GPU was too slow with is, once it was given up) until a drawing comes
/// back whole. One picture coming back says nothing of another that was lost with it: with
/// two pictures to a drawing and one of them lost every time, that is still one drawing more
/// and then one every five seconds, not one after another without end.
#[derive(Debug, Default, PartialEq)]
struct Lost {
    /// A picture of the drawing on its way back now was lost.
    in_this: bool,
    /// How many drawings one after another lost a picture; nought when the last came back whole.
    drawings: u32,
    /// When to draw the NDI pictures again, once more than one drawing has lost a picture.
    again_at: Option<Instant>,
}

impl Lost {
    /// The NDI pictures are being painted: a new drawing, and whatever was to be drawn again
    /// is being drawn now.
    fn painting(&mut self) {
        self.in_this = false;
        self.again_at = None;
    }

    /// A picture somebody is watching for was lost on its way back.
    fn one_lost(&mut self) {
        self.in_this = true;
    }

    /// Every picture of the drawing is back, or lost. True when the pictures are to be drawn
    /// once more now.
    fn drawing_back(&mut self, now: Instant) -> bool {
        if !std::mem::take(&mut self.in_this) {
            *self = Lost::default();
            return false;
        }
        self.drawings = self.drawings.saturating_add(1);
        if self.drawings == 1 {
            return true;
        }
        self.again_at = Some(now + GIVEN_UP_AGAIN_AFTER);
        false
    }

    /// True while a lost picture has not been made good by a drawing that came back whole.
    fn owed(&self) -> bool {
        self.in_this || self.drawings > 0
    }

    /// What the operator is told: nothing of a picture that is being drawn once more; that
    /// NDI is not showing what it was asked to, from when that one is lost too until a
    /// drawing has come back whole and been handed over.
    fn shown(&self) -> Shown {
        Shown { undrawn: (self.drawings >= 2).then_some(Undrawn::Failed), waiting: false }
    }

    /// True, once, when it is time to draw the pictures again.
    fn take_due(&mut self, now: Instant) -> bool {
        let due = self.again_at.is_some_and(|at| at <= now);
        if due {
            self.again_at = None;
        }
        due
    }
}

/// Testing aid: SOFTPRESENTER_NDI_LOSE=N (or N@P) has the next N NDI pictures that come back
/// from the GPU for a source somebody is watching count as lost: every such picture, or with
/// @P only the Pth picture of each drawing (1 for the first: a screen with its own look, or a
/// stage screen, is drawn before the picture the other sources share). Pictures drawn while
/// nobody is known to be watching (as a source starts, say) do not use the count up.
#[derive(Debug, PartialEq)]
struct PretendLost {
    left: u32,
    place: Option<usize>,
}

impl PretendLost {
    /// What the switch asks for; `None` for anything but a count, or a count and a place
    /// from one up.
    fn parse(asked: &str) -> Option<PretendLost> {
        let (count, place) = match asked.split_once('@') {
            Some((count, place)) => (count, Some(place.trim().parse::<usize>().ok()?.checked_sub(1)?)),
            None => (asked, None),
        };
        Some(PretendLost { left: count.trim().parse().ok()?, place })
    }

    /// Whether the picture in this place of its drawing (from nought) counts as lost, which
    /// uses one of the count up.
    fn takes(&mut self, place: usize) -> bool {
        let takes = self.left > 0 && self.place.is_none_or(|only| only == place);
        if takes {
            self.left -= 1;
        }
        takes
    }
}

/// Whether the NDI pictures are to be drawn because receivers came or went. Only when
/// receivers have come to a source since it was last drawn for (`drawn_for` has each source
/// with its count of arrivals then: a source whose receivers left and came back between two
/// looks shows the same number of receivers, but has let its picture go), or when a picture
/// is owed (it could not be drawn, which the last receiver leaving puts an end to). More
/// receivers, or fewer, of a source already drawn for change nothing to draw.
fn ndi_count_needs_drawing(sources: &[NdiSeen], drawn_for: &[(Uuid, u32)], owed: bool) -> bool {
    owed || sources.iter().any(|source| source.wanted && !drawn_for.contains(&(source.id, source.arrivals)))
}

/// Whether a picture is to be drawn for an NDI source. Not for one that could not start (NDI
/// not installed, say), and not for one nobody is watching: it would be drawn, read back from
/// the GPU and thrown away. A count that isn't known (the source is still starting, or its
/// NDI can't count) is drawn for, as if someone were watching.
fn ndi_wanted(state: &sp_ndi::State, receivers: Option<u32>) -> bool {
    !matches!(state, sp_ndi::State::Failed(_)) && receivers != Some(0)
}

/// The pictures to draw for the NDI sources, each with the sources it goes to: one for each
/// source that needs its own, then one shared by all the rest. Sources no picture is wanted
/// for are in none, and a picture nobody wants is not there at all.
fn ndi_groups(sources: &[NdiSeen]) -> Vec<Vec<Uuid>> {
    let wanted = || sources.iter().filter(|source| source.wanted);
    let mut groups: Vec<Vec<Uuid>> = wanted().filter(|source| source.alone).map(|source| vec![source.id]).collect();
    let shared: Vec<Uuid> = wanted().filter(|source| !source.alone).map(|source| source.id).collect();
    if !shared.is_empty() {
        groups.push(shared);
    }
    groups
}

/// Each screen's NDI source as the window sees it: started (by its name, and how it is doing
/// now), or refused before it could start.
#[derive(Clone)]
enum NdiEntry {
    Started(String, sp_ndi::Watch),
    Refused(String),
}

type NdiStatuses = Arc<Mutex<Vec<(Uuid, NdiEntry)>>>;

/// The operator's Program preview is what the screens show: the output thread draws the
/// audience picture a second time, small, into one of two textures the window shows, turn about: which one was drawn last, and how many
/// have been drawn (so the window knows when to show a new one). Nothing is read back from the
/// GPU, and the window only shows what is drawn.
#[derive(Default)]
pub struct LivePreview {
    front: std::sync::atomic::AtomicUsize,
    drawn: std::sync::atomic::AtomicU64,
    /// True while the window has been woken for a new picture and hasn't shown it yet, so a
    /// burst of pictures wakes it once.
    wake_pending: std::sync::atomic::AtomicBool,
    /// Why a screen is not showing what it was asked to, as [`Undrawn`]'s number; 0 when every
    /// screen is.
    undrawn: std::sync::atomic::AtomicU8,
}

/// Wakes the operator's window (from the output thread) to show a new live preview.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

impl LivePreview {
    /// Which texture holds the newest picture, and how many have been drawn.
    pub fn newest(&self) -> (usize, u64) {
        use std::sync::atomic::Ordering::Acquire;
        (self.front.load(Acquire), self.drawn.load(Acquire))
    }
}

/// The size of the live preview, and how often a playing video's pictures are drawn in it at
/// most: about thirty a second, so a 24 or 30 fps video shows every picture and a 60 fps one
/// every other (every change of slide or media is drawn at once, and fades as the screens do).
/// Measured in quiet runs (release build, a 30 fps 720p video, two window screens; ten-second
/// averages, three runs each): about 23% CPU for the whole app at this cap, 21% at a 50 ms cap,
/// which shows only every other picture of a 30 fps video.
pub const LIVE_PREVIEW_SIZE: (u32, u32) = (640, 360);
const LIVE_PREVIEW_EVERY: Duration = Duration::from_millis(30);

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
    ndi_status: NdiStatuses,
    live_preview: Arc<LivePreview>,
}

impl OutputHandle {
    /// Starts the thread. It shows nothing until it is given a window or an NDI name.
    pub fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        theme: Arc<Theme>,
        waker: Option<Waker>,
    ) -> Result<OutputHandle, String> {
        let (tx, rx) = channel();
        let ndi_status: NdiStatuses = Arc::default();
        let status = ndi_status.clone();
        let live_preview: Arc<LivePreview> = Arc::default();
        let shared = live_preview.clone();
        // A picture read for the screens meanwhile has them drawn again once it is there.
        let wake = tx.clone();
        // The NDI sources tell the thread when their receivers come and go, on the same channel.
        let from_ndi = tx.clone();
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || {
                sp_render::draw_kept_pictures_only(move || {
                    let _ = wake.send(OutputMsg::Redraw);
                });
                run(device, queue, theme, (from_ndi, rx), status, shared, waker)
            })
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread), ndi_status, live_preview })
    }

    /// Which live preview texture holds the newest picture, and how many have been drawn.
    pub fn live_preview(&self) -> (usize, u64) {
        self.live_preview.newest()
    }

    /// Why a screen (a window or an NDI source) is not showing what it was last asked to show,
    /// if one is not: the operator should know that the audience sees the picture before.
    pub fn undrawn(&self) -> Option<Undrawn> {
        Undrawn::from_code(self.live_preview.undrawn.load(std::sync::atomic::Ordering::Acquire))
    }

    /// The window has woken for the newest live preview: the next new one wakes it again.
    pub fn live_preview_woken(&self) {
        self.live_preview.wake_pending.store(false, std::sync::atomic::Ordering::Release);
    }

    /// Never blocks. Returns false when the thread has ended and the message went nowhere.
    pub fn send(&self, msg: OutputMsg) -> bool {
        self.tx.send(msg).is_ok()
    }

    /// What a screen's NDI output is doing now. `Off` while it is still starting.
    pub fn ndi_status(&self, screen: Uuid) -> NdiStatus {
        let entry = self
            .ndi_status
            .lock()
            .ok()
            .and_then(|all| all.iter().find(|(id, _)| *id == screen).map(|(_, entry)| entry.clone()));
        match entry {
            None => NdiStatus::Off,
            Some(NdiEntry::Refused(reason)) => NdiStatus::Failed(reason),
            Some(NdiEntry::Started(name, watch)) => match watch.state() {
                sp_ndi::State::Starting => NdiStatus::Off,
                sp_ndi::State::Sending => NdiStatus::On(name, watch.receivers()),
                sp_ndi::State::Failed(reason) => NdiStatus::Failed(reason),
            },
        }
    }

    /// True once the thread has stopped, which it only does on shutdown or a fatal error.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Stops the thread and waits for it.
    pub fn shutdown(mut self) {
        let _ = self.tx.send(OutputMsg::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What a screen shows: the picture behind, the slide, words over them, and the theme.
type Content = (Option<Picture>, Option<Arc<Slide>>, Option<Arc<Slide>>, Arc<Theme>, Option<Announcement>);

/// What the Announcements layer shows: a slide and the theme it is drawn with.
pub type Announcement = (Arc<Slide>, Arc<Theme>);

/// A window a screen is drawn in.
struct Target {
    id: Uuid,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    texture: wgpu::Texture,
    /// The picture now in the window in full, kept so the next one can fade in over it.
    shown: Option<vello::Scene>,
    /// A fade under way: the picture it started from and the one it ends at.
    fade: Option<(vello::Scene, vello::Scene)>,
    /// False when setting the window's surface up failed (the GPU too busy, say); it is tried
    /// again before each frame, and the window is skipped until it works.
    configured: bool,
    /// A picture for this window that could not be drawn: said in the log once while it lasts,
    /// not for every picture.
    trouble: crate::applog::Lasting,
    /// Whether the window shows what it was last asked to.
    state: Shown,
}

/// A copy of the picture a video lends, to draw: its pixels as they are, in whichever order
/// their bytes come (the GPU reads either), so copying is all the work there is.
fn copied(lent: &sp_video::Lent<'_>) -> Option<Picture> {
    Picture::from_rows(lent.width, lent.height, lent.bytes, lent.bytes_per_row, pixel_order(lent.order), lent.opaque)
}

fn pixel_order(order: sp_video::PixelOrder) -> sp_render::PixelOrder {
    match order {
        sp_video::PixelOrder::Rgba => sp_render::PixelOrder::Rgba,
        sp_video::PixelOrder::Bgra => sp_render::PixelOrder::Bgra,
    }
}

/// A window's next picture to draw into. The graphics library panics when the window's surface
/// isn't set up (its setup failed); that is caught here, so one window that can't be drawn this
/// moment doesn't stop the thread drawing every screen. `None` then: set it up and try again.
pub(crate) fn next_texture(surface: &wgpu::Surface<'static>) -> Option<wgpu::CurrentSurfaceTexture> {
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| surface.get_current_texture())).ok();
    if got.is_none() {
        crate::applog::event("output", "recovered from a window that could not be drawn; it is set up again");
    }
    got
}

/// Sets a window's surface up, saying whether that worked. A failure is caught here, not left to
/// stop the thread: a screen that can't be drawn this moment is tried again next frame.
pub(crate) fn configure(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'static>,
    config: &wgpu::SurfaceConfiguration,
) -> bool {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    surface.configure(device, config);
    match pollster::block_on(scope.pop()) {
        None => true,
        Some(error) => {
            crate::applog::problem("output", &format!("A screen's window could not be set up yet: {error}"));
            false
        }
    }
}

struct Output {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
    fonts: Fonts,
    frame: Frame,
    targets: Vec<Target>,
    /// When the transition under way began, and which it is. Every window follows this one
    /// clock, so they stay in step.
    fading: Option<(Instant, sp_model::Transition)>,
    /// The NDI sources that are switched on, and the texture their picture is drawn into.
    ndi: Vec<(Uuid, NdiOutput)>,
    /// A way into this thread's own messages, a copy of which each NDI source is started with
    /// to say that its receivers have come or gone. (It keeps the channel open for as long as
    /// the thread runs, as the picture loader's copy does: the thread ends when it is told to
    /// shut down, and then drops the sources and this with them.)
    from_ndi: Sender<OutputMsg>,
    ndi_target: Option<wgpu::Texture>,
    /// NDI pictures on their way back from the GPU, buffers kept to copy the next ones into,
    /// and whether a newer picture is waiting for the round under way to finish: if one is,
    /// how it may differ from the last (see `send_ndi`).
    ndi_pending: Vec<NdiReadback>,
    ndi_buffers: Vec<wgpu::Buffer>,
    ndi_dirty: Option<NdiChange>,
    /// The sources the NDI pictures were last drawn for, each with how many times receivers
    /// had come to it by then.
    ndi_drawn_for: Vec<(Uuid, u32)>,
    /// NDI pictures lost on their way back from the GPU: whether to draw again, and what the
    /// operator is told. Said in the log once while it lasts.
    ndi_lost: Lost,
    ndi_lost_trouble: crate::applog::Lasting,
    /// Testing aid: SOFTPRESENTER_NDI_LOSE (see `PretendLost`).
    pretend_lost: Option<PretendLost>,
    ndi_status: NdiStatuses,
    /// While screens are being identified: the name each one shows instead of the show.
    /// While screens are identified: each screen's name (as a slide), its colour, and when it
    /// started, for the wash of colour that breathes over it.
    identify: Vec<(Uuid, Arc<Slide>, sp_model::Rgba)>,
    identify_since: Instant,
    identify_theme: Arc<Theme>,
    /// The audience screens with a look of their own.
    looks: Vec<(Uuid, crate::screens::Look)>,
    themes: Vec<(Uuid, Arc<Theme>)>,
    /// The stage screens with the layout each is drawn with, and whether they are on.
    stage: Vec<(Uuid, Arc<Theme>)>,
    stage_on: bool,
    /// When the slide on screen was fully on (after its transition), for builds that start with
    /// it; and when its last click build was set going.
    slide_on: Instant,
    step_at: Instant,
    overlay: Overlay,
    /// The playing video's latest picture, and the texture on the GPU its pixels are sent
    /// into: one for each video, so the picture a fade starts from keeps the old video's.
    video_picture: Option<Picture>,
    video_texture: Option<MovingPicture>,
    /// The Announcements layer's slide, if one is on.
    announcement: Option<Announcement>,
    /// How the next new picture fades in: the slides' transition, or the media's when only the
    /// media changed.
    look: sp_model::Transition,
    /// An NDI picture that could not be drawn: said in the log once while it lasts, not for
    /// every picture. (Each window has its own.)
    ndi_trouble: crate::applog::Lasting,
    /// Whether the NDI picture and the live preview show what they were last asked to.
    ndi_state: Shown,
    preview_state: Shown,
    /// The drawing of the screens under way or just done: what became of its pictures, and
    /// when to draw again after a slow GPU (the old picture stays up meanwhile).
    round: Round,
    /// Testing aids: pictures to give up as if the GPU were too slow, and when the thread
    /// started.
    pretend_slow: Option<PretendSlow>,
    started: Instant,
    /// How often and how long the screens are drawn, and how many video pictures came, for the log.
    timing: crate::applog::Timing,
    video_pictures: u64,
    /// The live preview: what the window is told, the two textures drawn into, and when the
    /// last was drawn.
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
    preview_targets: Vec<wgpu::Texture>,
    previewed_at: Instant,
    /// When a video last gave a new picture, or a message last came (so a video just shown is
    /// looked at often from the start).
    video_moved_at: Instant,
    /// The live preview's whole picture, and a fade under way in it: from, to.
    preview_shown: Option<vello::Scene>,
    preview_fade: Option<(vello::Scene, vello::Scene)>,
}

fn run(
    device: wgpu::Device,
    queue: wgpu::Queue,
    theme: Arc<Theme>,
    (from_ndi, rx): (Sender<OutputMsg>, Receiver<OutputMsg>),
    ndi_status: NdiStatuses,
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
) {
    let mut painter = match Painter::new(&device) {
        Ok(painter) => painter,
        Err(e) => return eprintln!("The output cannot draw on this computer: {e}"),
    };
    // Testing aid: SOFTPRESENTER_OUTPUT_ROOM (see `output_room`).
    if let Some(room) = std::env::var("SOFTPRESENTER_OUTPUT_ROOM").ok().as_deref().and_then(output_room) {
        painter.keep_scratch_at(Some(room));
    }
    let mut output = Output {
        painter,
        fonts: Fonts::new(),
        frame: Frame {
            media: None,
            video: None,
            media_fit: sp_model::ImageFit::Fill,
            slide_video: Vec::new(),
            slide: None,
            theme,
            enabled: true,
            transition: sp_model::Transition::of(sp_model::TransitionKind::Cut),
            media_transition: sp_model::Transition::of(sp_model::TransitionKind::Cut),
            next: None,
            about: crate::stage::About::default(),
            build_step: 0,
        },
        targets: Vec::new(),
        fading: None,
        ndi: Vec::new(),
        from_ndi,
        ndi_target: None,
        ndi_pending: Vec::new(),
        ndi_buffers: Vec::new(),
        ndi_dirty: None,
        ndi_drawn_for: Vec::new(),
        ndi_lost: Lost::default(),
        ndi_lost_trouble: crate::applog::Lasting::default(),
        // Testing aid: SOFTPRESENTER_NDI_LOSE (see `PretendLost`).
        pretend_lost: std::env::var("SOFTPRESENTER_NDI_LOSE").ok().as_deref().and_then(PretendLost::parse),
        ndi_status,
        identify: Vec::new(),
        identify_since: Instant::now(),
        identify_theme: Arc::new(Theme::builtin()),
        stage: Vec::new(),
        stage_on: true,
        looks: Vec::new(),
        themes: Vec::new(),
        slide_on: Instant::now(),
        step_at: Instant::now(),
        overlay: Overlay::default(),
        video_picture: None,
        video_texture: None,
        announcement: None,
        look: sp_model::Transition::of(sp_model::TransitionKind::Cut),
        ndi_trouble: crate::applog::Lasting::default(),
        ndi_state: Shown::default(),
        preview_state: Shown::default(),
        round: Round::default(),
        pretend_slow: std::env::var("SOFTPRESENTER_GPU_TOO_SLOW").ok().as_deref().and_then(PretendSlow::parse),
        started: Instant::now(),
        timing: crate::applog::Timing::new("screen drawings"),
        video_pictures: 0,
        live_preview,
        waker,
        preview_targets: Vec::new(),
        previewed_at: Instant::now(),
        video_moved_at: Instant::now(),
        preview_shown: None,
        preview_fade: None,
        device,
        queue,
    };

    loop {
        // The GPU library lets go of what a finished drawing used (a bind group and a texture
        // view for every picture the renderer paints) only when the device is asked to look,
        // and nothing else asks: without this, each frame drawn left about a kilobyte behind,
        // for good (a few thousand objects for every half minute of video). It never waits.
        let _ = output.device.poll(wgpu::PollType::Poll);
        // What the operator has asked for meanwhile comes first: it is drawn below, in place
        // of a picture that was due to be drawn again.
        let waiting = match rx.try_recv() {
            Ok(msg) => Some(msg),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => return,
        };
        // A picture the GPU was too slow with a moment ago is drawn again when its time comes,
        // whatever else the thread is waiting for.
        if waiting.is_none() {
            output.draw_again_if_due();
        }
        // While a fade is under way, look for messages without waiting, so it keeps moving;
        // otherwise sleep until there is one.
        let first = if waiting.is_some() {
            waiting
        } else if output.fading.is_some() || output.building() || output.scrolling() || output.identify_breathing() {
            match rx.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else if output.ndi_waiting() {
            // NDI pictures on their way back from the GPU are looked for every couple of
            // milliseconds (never waited on), with a playing video kept going meanwhile.
            match rx.recv_timeout(Duration::from_millis(2)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // The picture that was waiting may be painted now: a drawing of its own,
                    // so that one the GPU is too slow with is drawn again, and the operator is
                    // told of one that cannot be drawn, or that all is well again.
                    output.round.begin_part();
                    output.finish_ndi();
                    output.end_round();
                    if output.frame.plays_video() && output.take_video_picture() {
                        output.draw(false);
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if capture_wanted() {
            // Testing aid: while a picture of the output is still to be taken, keep drawing, so
            // it is taken even where no window asks to be drawn (a quiet run's are hidden).
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // With a video's (or web page's) newest picture, as the screens show it.
                    output.take_video_picture();
                    output.draw(false);
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if output.ticks() {
            // Stage screens show the time: wake at the start of each second to redraw them.
            let into_second = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(Duration::ZERO, |since| Duration::from_nanos(u64::from(since.subsec_nanos())));
            // A playing video is looked at for a new picture far more often.
            let wait = if output.frame.plays_video() {
                output.video_poll()
            } else {
                Duration::from_secs(1).saturating_sub(into_second) + Duration::from_millis(5)
            };
            // No longer than until a picture the GPU was too slow with is due again.
            let wait = output.draw_again_in().map_or(wait, |due| wait.min(due));
            match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Woken for that picture: it redraws every screen, the time included.
                    if output.draw_again_if_due() {
                        continue;
                    }
                    // A new picture of the video redraws everything, the time included.
                    if output.take_video_picture() {
                        output.draw(false);
                        continue;
                    }
                    // Otherwise the time changes only as a second begins (a video, or a web page
                    // or video in a box, wakes this far more often than that).
                    let new_second =
                        !output.frame.plays_video() || into_second + output.video_poll() >= Duration::from_secs(1);
                    if !new_second {
                        continue;
                    }
                    if output.overlay.message.as_ref().is_some_and(|(text, _)| crate::messages::has_tokens(text))
                        || output.links_tick()
                    {
                        output.draw(false);
                    } else {
                        output.draw_stage();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if output.frame.plays_video() {
            match rx.recv_timeout(output.video_poll()) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if output.take_video_picture() {
                        output.draw(false);
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            // Nothing moves until the next message: memory a heavy picture took is given back.
            // That never waits here; while the GPU still holds some of it, or a picture it was
            // too slow with is to be drawn again, the thread comes back in a moment to look.
            let rested = output.painter.rest(&output.device, &output.queue);
            match output.look_again_in(rested) {
                None => match rx.recv() {
                    Ok(msg) => Some(msg),
                    Err(_) => return,
                },
                // The top of the loop looks at the GPU, and draws a picture again if it is due.
                Some(wait) => match rx.recv_timeout(wait) {
                    Ok(msg) => Some(msg),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                },
            }
        };
        let Some(first) = first else {
            // Drawing waits for the screen's next refresh, which paces the fade and the builds.
            if output.fading.is_some() {
                output.step_fade();
            } else {
                output.take_video_picture();
                output.draw(false);
                // With no window to wait for (only NDI), keep to about the screens' pace.
                if output.targets.is_empty() {
                    std::thread::sleep(Duration::from_millis(16));
                }
            }
            continue;
        };
        output.video_moved_at = Instant::now();
        // Take everything that has queued up, so a burst of changes costs one draw.
        let mut next = Some(first);
        let mut new_picture = false;
        // Whether NDI receivers came or went, and whether that is all these messages say.
        let (mut receivers_changed, mut nothing_else) = (false, true);
        while let Some(msg) = next {
            nothing_else &= matches!(msg, OutputMsg::NdiReceivers);
            match msg {
                OutputMsg::Show(frame) => {
                    let now = Instant::now();
                    // The next slide's pictures are read meanwhile, so it goes on at once.
                    if let Some(next) = frame.next.as_deref() {
                        sp_render::read_ahead(next);
                    }
                    let same_slide = output.frame.slide.as_deref() == frame.slide.as_deref()
                        && output.frame.enabled == frame.enabled;
                    if !same_slide {
                        // Builds that start with the slide wait for its transition to end.
                        output.slide_on = now + Duration::from_secs_f32(frame.transition.duration());
                        output.step_at = now;
                        new_picture = true;
                    } else if frame.build_step > output.frame.build_step {
                        // A click build is drawn moving on the slide already there, not faded to.
                        output.step_at = now;
                    } else if frame.build_step < output.frame.build_step {
                        // Stepping back shows the builds still done as they were, without moving
                        // them again.
                        output.step_at = now.checked_sub(Duration::from_secs(3600)).unwrap_or(now);
                    } else {
                        new_picture = true;
                    }
                    // Another video, or none: its last picture goes, and its still shows until
                    // the new one's first picture comes.
                    let same_video = match (&output.frame.video, &frame.video) {
                        (Some(was), Some(now)) => was.same(now),
                        (None, None) => true,
                        _ => false,
                    };
                    if !same_video {
                        output.video_picture = None;
                        output.video_texture = None;
                    }
                    // A video box that stops playing goes back to its still.
                    for (file, _) in &output.frame.slide_video {
                        if !frame.slide_video.iter().any(|(still, _)| still == file) {
                            sp_render::forget_picture(file);
                        }
                    }
                    let same_media = match (&output.frame.media, &frame.media) {
                        (Some(was), Some(now)) => was.same(now),
                        (None, None) => true,
                        _ => false,
                    };
                    // Another slide, video or picture behind it: should the GPU be slow with it,
                    // it gets all its tries. The same picture sent again does not: that would
                    // put off for good the telling of the operator.
                    if !same_slide || !same_media || !same_video {
                        output.round.new_picture();
                    }
                    // Only the media changed: it gives way by the media's own transition.
                    output.look = if same_slide && !same_media { frame.media_transition } else { frame.transition };
                    output.frame = frame;
                }
                OutputMsg::AddSurface { id, surface, format, w, h } => output.add_surface(id, surface, format, w, h),
                OutputMsg::PreviewTargets(targets) => output.preview_targets = targets,
                OutputMsg::Resized(id, w, h) => output.resize(id, w, h),
                OutputMsg::Redraw => {}
                OutputMsg::SetNdi { id, name } => output.set_ndi(id, name),
                OutputMsg::NdiReceivers => receivers_changed = true,
                OutputMsg::RemoveSurface(id) => output.targets.retain(|target| target.id != id),
                OutputMsg::Announcement(announcement) => {
                    // A new announcement dissolves in over what is there, and gets all its tries.
                    output.round.new_picture();
                    output.announcement = announcement;
                    output.look = sp_model::Transition::default();
                    new_picture = true;
                }
                OutputMsg::Overlay(overlay) => output.overlay = overlay,
                OutputMsg::Looks(looks) => output.looks = looks,
                OutputMsg::Themes(themes) => output.themes = themes,
                OutputMsg::Stage { screens, on } => {
                    output.stage = screens;
                    output.stage_on = on;
                }
                OutputMsg::Identify(names) => {
                    output.identify = names
                        .into_iter()
                        .map(|(id, name, color)| {
                            (id, Arc::new(crate::messages::slide(&name, crate::messages::Look::Centre)), color)
                        })
                        .collect();
                    output.identify_since = Instant::now();
                    // Names appear and go at once, whatever was fading.
                    output.fading = None;
                    for target in &mut output.targets {
                        target.fade = None;
                        target.shown = None;
                    }
                }
                OutputMsg::Shutdown => return,
            }
            next = rx.try_recv().ok();
        }
        // The Screens window says how many are watching each source: it looks again.
        if receivers_changed && let Some(wake) = &output.waker {
            wake();
        }
        if nothing_else {
            // Only the NDI pictures are drawn, and only if the change of count asks for it.
            // Drawing the windows too would cut short a fade under way in them, for something
            // the audience in the room has no part in.
            let owed = output.ndi_state != Shown::default() || output.ndi_lost.owed();
            if ndi_count_needs_drawing(&output.ndi_seen(), &output.ndi_drawn_for, owed) {
                output.draw_ndi();
            }
        } else {
            output.draw(new_picture);
        }
    }
}

/// How far in a fade is, from 0 to 1, eased so it starts and ends gently.
fn fade_amount(started: Instant, seconds: f32) -> f32 {
    let t = (started.elapsed().as_secs_f32() / seconds.max(0.001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Output {
    fn add_surface(&mut self, id: Uuid, surface: wgpu::Surface<'static>, format: wgpu::TextureFormat, w: u32, h: u32) {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        let configured = configure(&self.device, &surface, &config);
        // A screen has one window: a new one takes the old one's place.
        self.targets.retain(|target| target.id != id);
        self.targets.push(Target {
            id,
            blitter: wgpu::util::TextureBlitter::new(&self.device, format),
            texture: target_texture(&self.device, config.width, config.height),
            surface,
            config,
            shown: None,
            fade: None,
            configured,
            trouble: crate::applog::Lasting::default(),
            state: Shown::default(),
        });
    }

    fn resize(&mut self, id: Uuid, w: u32, h: u32) {
        let Some(target) = self.targets.iter_mut().find(|target| target.id == id) else { return };
        // A minimised window reports zero; keep the last real size until it comes back.
        if w == 0 || h == 0 || (w == target.config.width && h == target.config.height) {
            return;
        }
        target.config.width = w;
        target.config.height = h;
        target.configured = configure(&self.device, &target.surface, &target.config);
        target.texture = target_texture(&self.device, w, h);
        // Pictures built for the old size cannot be faded at the new one.
        target.shown = None;
        target.fade = None;
    }

    /// Starts, renames or stops a screen's NDI source and records how that went for the operator.
    fn set_ndi(&mut self, id: Uuid, name: Option<String>) {
        // Stop the old source first, so a renamed one does not appear twice.
        self.ndi.retain(|(have, _)| *have != id);
        // Starting never waits on NDI: the source starts on its own thread.
        let status = name.map(|name| {
            // Its thread says when its receivers come and go; it only passes the word on.
            let tell = self.from_ndi.clone();
            let started = NdiOutput::start_with_wake(&name, move || {
                let _ = tell.send(OutputMsg::NdiReceivers);
            });
            match started {
                Ok(ndi) => {
                    let entry = NdiEntry::Started(ndi.name().to_owned(), ndi.watch());
                    self.ndi.push((id, ndi));
                    entry
                }
                Err(reason) => NdiEntry::Refused(reason),
            }
        });
        if self.ndi.is_empty() {
            // Nothing is sent: whatever NDI picture could not be drawn is no longer owed.
            self.ndi_state = Shown::default();
            self.ndi_trouble = crate::applog::Lasting::default();
            self.ndi_target = None;
            self.ndi_pending.clear();
            self.ndi_buffers.clear();
            self.ndi_dirty = None;
            self.ndi_lost = Lost::default();
            self.ndi_lost_trouble = crate::applog::Lasting::default();
        }
        self.ndi_drawn_for.retain(|(have, _)| *have != id);
        // A new source gets the picture on screen now.
        if !self.ndi.is_empty() {
            let frame = self.frame.clone();
            self.send_ndi(&frame, NdiChange::Nothing);
        }
        if let Ok(mut shared) = self.ndi_status.lock() {
            shared.retain(|(have, _)| *have != id);
            shared.extend(status.map(|status| (id, status)));
        }
    }

    /// What a screen shows now. Cheap copies: the picture, the slide and the theme are shared.
    fn content_for(&self, id: Uuid, frame: &Frame) -> Content {
        if let Some((_, layout)) = self.stage.iter().find(|(have, _)| *have == id) {
            let words = self.stage_on.then(|| layout.templates.first()).flatten().map(|template| {
                let (now, next) = (frame.slide.as_deref(), frame.next.as_deref());
                Arc::new(crate::stage::slide(now, next, &self.overlay, &frame.about, template))
            });
            return (None, words, None, layout.clone(), None);
        }
        if let Some(pattern) = self.overlay.test_pattern {
            return (None, Some(Arc::new(pattern.slide())), None, self.identify_theme.clone(), None);
        }
        if !frame.enabled {
            return (None, None, None, frame.theme.clone(), None);
        }
        // A screen's look can leave layers out, and its own theme lays the words out its way.
        let look = self.look_of(id);
        let theme = screen_theme(&self.themes, id, &frame.theme);
        // A message shows over whatever is on the audience screens.
        let message = self.overlay.message.as_ref().filter(|_| look.messages).map(|(text, look)| {
            let words =
                crate::messages::fill_in(text, &self.overlay.timers, Instant::now(), crate::timers::seconds_into_day());
            crate::messages::slide(&words, *look)
        });
        // Props and the message share the layer over the slide.
        let copyright = self.overlay.copyright.as_deref().map(crate::props::copyright_slide);
        let values = crate::links::Values {
            timers: &self.overlay.timers,
            title: frame.about.title.as_deref(),
            group: frame.about.group.as_deref(),
            next: frame.next.as_deref(),
            stage_message: self.overlay.stage_message.as_deref(),
            copyright: self.overlay.copyright.as_deref(),
        };
        let mut under: Vec<Slide> = if look.props {
            self.overlay.props.iter().map(|prop| crate::links::fill(prop, &values).into_owned()).collect()
        } else {
            Vec::new()
        };
        // The copyright line goes with the slide's words.
        under.extend(copyright.filter(|_| look.slides));
        let over = crate::props::overlay(&under, message.as_ref()).map(Arc::new);
        // The slide as far as its builds have got, with its linked text filled in.
        let slide = frame.slide.as_ref().filter(|_| look.slides).map(|slide| {
            let (built, _) = slide.built(&theme, self.build_clock());
            Arc::new(crate::links::fill(&built, &values).into_owned())
        });
        let media = self.video_picture.clone().or_else(|| frame.media.clone());
        let announcement = self.announcement.clone().filter(|_| look.announcements);
        (media.filter(|_| look.media), slide, over, theme, announcement)
    }

    /// How an audience screen shows the show: its own look, or every layer.
    fn look_of(&self, id: Uuid) -> crate::screens::Look {
        self.looks.iter().find(|(have, _)| *have == id).map_or_else(crate::screens::Look::default, |(_, look)| *look)
    }

    /// Where the slide on screen is in its builds now.
    fn build_clock(&self) -> sp_model::BuildClock {
        let now = Instant::now();
        sp_model::BuildClock {
            step: self.frame.build_step,
            since_on: now.checked_duration_since(self.slide_on).map(|since| since.as_secs_f32()),
            since_step: now.duration_since(self.step_at).as_secs_f32(),
        }
    }

    /// True while a build on the slide on screen is moving, or waiting to, so it is drawn again
    /// at each refresh.
    fn building(&self) -> bool {
        self.frame.enabled
            && self.frame.slide.as_ref().is_some_and(|slide| {
                let clock = self.build_clock();
                // During the transition, builds wait for it to end: keep drawing until they're done.
                clock.since_on.is_none() && slide.objects.iter().any(|o| o.build_in.is_some_and(|b| !b.on_click))
                    || slide.built(&self.frame.theme, clock).1
            })
    }

    /// Builds what a screen shows now for a target of a size.
    fn scene_for(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> vello::Scene {
        let (media, slide, overlay, theme, announcement) = self.content_for(id, frame);
        // While screens are identified, each is marked over what it shows.
        let identified =
            self.identify.iter().find(|(have, ..)| *have == id).map(|(_, name, color)| (name.clone(), *color));
        // Scrolling text moves on from when the slide was fully on screen.
        let time = Instant::now().checked_duration_since(self.slide_on).map_or(0.0, |since| since.as_secs_f32());
        // A stage screen keeps the plain look.
        let look = if self.is_stage(id) { crate::screens::Look::default() } else { self.look_of(id) };
        let backdrop = (look.color != sp_model::Rgba::BLACK).then_some(look.color);
        let layers = Layers {
            media_fit: frame.media_fit,
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: overlay.as_deref(),
            time,
            backdrop,
            mask: look.mask,
            announcement: announcement.as_ref().map(|(slide, theme)| (&**slide, &**theme)),
        };
        let mut scene = build_layers(&mut self.fonts, &layers, &theme, w, h).scene;
        // Its colour and border, then its name on top, clear of the colour.
        if let Some((name, color)) = identified {
            identify_marks(&mut scene, color, identify_strength(self.identify_since.elapsed().as_secs_f32()), w, h);
            let label = sp_render::build_overlay(&mut self.fonts, &name, &self.identify_theme, w, h);
            scene.append(&label, None);
        }
        scene
    }

    /// True while the wash over identified screens is still breathing, so they are drawn at
    /// each refresh until it settles.
    fn identify_breathing(&self) -> bool {
        !self.identify.is_empty() && self.identify_since.elapsed().as_secs_f32() < IDENTIFY_BREATHS * IDENTIFY_BREATH
    }

    /// Draws the picture at NDI's size for every NDI source anyone is watching and starts
    /// copying it back from the GPU, without waiting for the copy: `finish_ndi` hands it over
    /// once it is there. One round of copies is under way at a time; a picture that comes while
    /// one is under way is not waited for, and the newest is drawn as soon as the round is
    /// done. This does not depend on any window: with none open, NDI still sends.
    ///
    /// A source nobody is watching gets nothing: no picture is drawn or read back for it. It
    /// says when a receiver connects (`OutputMsg::NdiReceivers`), which has this picture drawn
    /// for it then. Meanwhile NDI gives a receiver the last picture the source sent, the
    /// moment it connects. `changed` says which pictures asked for now may be other than the
    /// last ones asked for: the sources of those that nobody is watching are told to forget
    /// theirs, which puts black in its place. So a receiver that connects is never sent a
    /// slide that has gone: it sees black until its picture comes, or, when nothing has
    /// changed since the last receiver left, the picture sent then, which is still the one on
    /// the screens.
    fn send_ndi(&mut self, frame: &Frame, changed: NdiChange) {
        self.finish_ndi();
        if self.ndi.is_empty() {
            return;
        }
        let seen = self.ndi_seen();
        // A drawing put off while a picture was on its way back is drawn now, or not drawn
        // because nobody is watching any more: what it changed counts as changed now.
        let changed = changed.with(self.ndi_dirty);
        // Another picture is asked for, which a source nobody is watching will not get: the
        // one it had is out of date. (Said for every such picture; the source acts on it once.)
        let mut forget = ndi_out_of_date(&seen, changed);
        // A picture still on its way back from the GPU does not go to a source nobody is
        // watching any more, which would throw it away. It was newer than the one the source
        // has, so that one is out of date.
        for readback in &mut self.ndi_pending {
            forget.extend(ndi_strip(&mut readback.ids, &seen));
        }
        for (_, ndi) in self.ndi.iter().filter(|(id, _)| forget.contains(id)) {
            ndi.forget();
        }
        let groups = ndi_groups(&seen);
        if groups.is_empty() {
            // Nobody is watching (or no source could start): no picture is owed, so none is
            // undrawn, whatever went wrong while someone watched.
            self.ndi_dirty = None;
            self.ndi_drawn_for.clear();
            self.ndi_state.not_asked();
            self.ndi_trouble = crate::applog::Lasting::default();
            self.ndi_lost = Lost::default();
            self.ndi_lost_trouble = crate::applog::Lasting::default();
            return;
        }
        if !self.ndi_pending.is_empty() {
            self.ndi_dirty = Some(changed);
            return;
        }
        self.ndi_dirty = None;
        self.ndi_lost.painting();
        self.ndi_drawn_for =
            seen.iter().filter(|source| source.wanted).map(|source| (source.id, source.arrivals)).collect();
        let (w, h) = NDI_SIZE;
        let mut all_painted = true;
        for (place, ids) in groups.into_iter().enumerate() {
            // Not asked for after a picture the GPU was too slow with: it waits for the next
            // try, and so do the pictures after it.
            if self.round.slow {
                self.ndi_state.skipped();
                all_painted = false;
                break;
            }
            let scene = self.scene_for(ids[0], frame, w, h);
            let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h)).clone();
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            if let Err(e) = self.paint(&scene, &view, w, h) {
                let message = format!("The NDI picture could not be drawn: {e}");
                if self.ndi_trouble.is_news(&message) {
                    crate::applog::problem("output", &message);
                }
                // The other sources' pictures are still drawn, unless the GPU was too slow.
                self.ndi_state.failed(&e);
                all_painted = false;
                continue;
            }
            // The copy is queued after the drawing, so it takes this picture even though the next
            // group is drawn into the same texture straight after.
            let padded_row = (w as usize * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
            let buffer = self.ndi_buffers.pop().unwrap_or_else(|| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("NDI readback"),
                    size: (padded_row * h as usize) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row as u32),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit([encoder.finish()]);
            let done: Arc<Mutex<Option<bool>>> = Arc::default();
            let flag = done.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                if let Ok(mut flag) = flag.lock() {
                    *flag = Some(result.is_ok());
                }
            });
            self.ndi_pending.push(NdiReadback { ids, place, buffer, padded_row, done });
        }
        if !all_painted {
            return;
        }
        // Every picture was drawn: whatever went wrong before is over.
        self.ndi_state.painted();
        if let Some(more) = self.ndi_trouble.over() {
            crate::applog::event("output", &format!("The NDI picture is drawn again, after {more} more not drawn"));
        }
    }

    /// Each NDI source as a drawing finds it now.
    fn ndi_seen(&self) -> Vec<NdiSeen> {
        // The show is drawn once for every source that shows it as it is; a screen being
        // identified, a stage screen or one with its own look gets its own picture.
        self.ndi
            .iter()
            .map(|(id, ndi)| NdiSeen {
                id: *id,
                // Read before the count: receivers that come in between are drawn for twice,
                // not missed.
                arrivals: ndi.arrivals(),
                wanted: ndi_wanted(&ndi.state(), ndi.receivers()),
                alone: self.identify.iter().any(|(have, ..)| have == id)
                    || self.is_stage(*id)
                    || self.looks.iter().any(|(have, _)| have == id)
                    || self.themes.iter().any(|(have, _)| have == id),
                unwatched: ndi.receivers() == Some(0),
                stage: self.is_stage(*id),
            })
            .collect()
    }

    /// A picture for these sources was lost on its way back from the GPU. The sources are told
    /// that what they last sent is out of date: they go on sending it to whoever is watching,
    /// but once those leave, whoever connects next is not given it. If any of them still has
    /// receivers, those keep the picture before it, and the loss counts (see `Lost`); a
    /// picture nobody is watching for any more is owed to nobody. Said in the log once while
    /// it lasts.
    fn ndi_lost(&mut self, ids: &[Uuid]) {
        for (_, ndi) in self.ndi.iter().filter(|(id, _)| ids.contains(id)) {
            ndi.forget();
        }
        if ndi_lost_is_owed(ids, &self.ndi_seen()) {
            self.ndi_lost.one_lost();
            if self.ndi_lost_trouble.is_news("An NDI picture could not be read back.") {
                crate::applog::problem("output", "An NDI picture could not be read back.");
            }
        }
    }

    /// Draws the NDI pictures alone, as a drawing of its own (like one that had to wait for
    /// the picture before it): one the GPU is too slow with is drawn again, and the operator
    /// is told of one that cannot be drawn, or that all is well again.
    fn draw_ndi(&mut self) {
        self.round.begin_part();
        let frame = self.frame.clone();
        // The picture is the one last asked for. If that could not be drawn, though, the
        // sources still have the one before it, which is out of date for any nobody watches.
        let owed = if self.ndi_state == Shown::default() { NdiChange::Nothing } else { NdiChange::Any };
        self.send_ndi(&frame, owed);
        self.end_round();
    }

    /// True while pictures for NDI are on their way back from the GPU.
    fn ndi_waiting(&self) -> bool {
        !self.ndi_pending.is_empty()
    }

    /// Hands every NDI picture that has come back from the GPU to its sources, without
    /// waiting for any still on the way; then, if a newer picture came meanwhile, starts it.
    fn finish_ndi(&mut self) {
        if self.ndi_pending.is_empty() {
            return;
        }
        let _ = self.device.poll(wgpu::PollType::Poll);
        let (w, h) = NDI_SIZE;
        let row = w as usize * 4;
        let mut still = Vec::new();
        let mut lost: Vec<Uuid> = Vec::new();
        for readback in std::mem::take(&mut self.ndi_pending) {
            let mut done = readback.done.lock().ok().and_then(|done| *done);
            // Testing aid: SOFTPRESENTER_NDI_LOSE (see `PretendLost`).
            let watched = |id: &Uuid| self.ndi.iter().any(|(have, ndi)| have == id && ndi.receivers() > Some(0));
            if done == Some(true)
                && readback.ids.iter().any(watched)
                && self.pretend_lost.as_mut().is_some_and(|lose| lose.takes(readback.place))
            {
                readback.buffer.unmap();
                done = Some(false);
            }
            match done {
                None => still.push(readback),
                Some(false) => lost.extend(readback.ids),
                Some(true) => {
                    // The sources it is for that still want it. One that nobody is watching
                    // any more would only throw it away: it is told that the picture it had
                    // is out of date, which is all this one would have told it.
                    let mut sources: Vec<&NdiOutput> = Vec::new();
                    for (_, ndi) in self.ndi.iter().filter(|(id, _)| readback.ids.contains(id)) {
                        if ndi_wanted(&ndi.state(), ndi.receivers()) {
                            sources.push(ndi);
                        } else {
                            ndi.forget();
                        }
                    }
                    if !sources.is_empty() {
                        // The rows are copied out of the buffer once, without the padding, and
                        // every source sends those same pixels.
                        let pixels = {
                            let slice = readback.buffer.slice(..);
                            let Ok(mapped) = slice.get_mapped_range() else {
                                lost.extend(readback.ids);
                                continue;
                            };
                            let mut pixels = Vec::with_capacity(row * h as usize);
                            for padded in mapped.chunks(readback.padded_row) {
                                pixels.extend_from_slice(&padded[..row]);
                            }
                            Arc::new(pixels)
                        };
                        for ndi in sources {
                            ndi.show_shared(w, h, pixels.clone());
                        }
                    }
                    readback.buffer.unmap();
                    // Kept for the next picture, two at most.
                    if self.ndi_buffers.len() < 2 {
                        self.ndi_buffers.push(readback.buffer);
                    }
                }
            }
        }
        self.ndi_pending = still;
        if !lost.is_empty() {
            self.ndi_lost(&lost);
        }
        if self.ndi_pending.is_empty() {
            // The whole drawing is back, or lost: once more at once after the first loss,
            // every few seconds after that, and all well again once one comes back whole.
            if self.ndi_lost.drawing_back(Instant::now()) {
                self.ndi_dirty = Some(NdiChange::Nothing.with(self.ndi_dirty));
            }
            if !self.ndi_lost.owed()
                && let Some(more) = self.ndi_lost_trouble.over()
            {
                crate::applog::event("output", &format!("NDI pictures are read back again, after {more} more lost"));
            }
        }
        if self.ndi_pending.is_empty()
            && let Some(changed) = self.ndi_dirty
        {
            let frame = self.frame.clone();
            self.send_ndi(&frame, changed);
        }
    }

    /// Draws the current frame everywhere. A new picture fades in over the one on screen when
    /// the frame asks for a dissolve; anything else (a resize, an uncovered window) is drawn
    /// at once.
    fn draw(&mut self, new_picture: bool) {
        let started = Instant::now();
        self.draw_now(new_picture);
        self.timing.add(started.elapsed());
        let (videos, screens, ndi) = (self.video_pictures, self.targets.len(), self.ndi.len());
        let extra = || format!("{videos} video pictures, {screens} windows, {ndi} NDI");
        if self.timing.report_each_minute("output", extra) {
            self.video_pictures = 0;
        }
    }

    /// Draws the audience picture small for the operator's window, as the screens show it: a
    /// new picture fades in as theirs does; a playing video's pictures are drawn at most thirty
    /// times a second.
    fn draw_live_preview(&mut self, frame: &Frame, new_picture: bool) {
        if self.preview_targets.len() < 2 {
            return;
        }
        if !new_picture && frame.plays_video() && self.previewed_at.elapsed() < LIVE_PREVIEW_EVERY {
            return;
        }
        let (w, h) = LIVE_PREVIEW_SIZE;
        let scene = self.scene_for(Uuid::nil(), frame, w, h);
        // A video's new picture during a fade: the fade goes on, towards the newer picture.
        if !new_picture && let Some((from, _)) = self.preview_fade.take() {
            self.preview_fade = Some((from, scene));
            return self.step_live_preview(None);
        }
        let before = self.preview_fade.take().map(|(_, to)| to).or_else(|| self.preview_shown.take());
        match before {
            Some(from) if new_picture && self.fading.is_some() => {
                self.preview_fade = Some((from, scene));
                self.step_live_preview(Some(0.0));
            }
            // A picture that was not painted is not the one the preview shows.
            before => {
                let painted = self.paint_live_preview(&scene);
                self.preview_shown = if painted { Some(scene) } else { before };
            }
        }
    }

    /// Moves a fade in the live preview on to `amount` (as far as the screens' fade has got
    /// when `None`); at the end it shows the whole new picture.
    fn step_live_preview(&mut self, amount: Option<f32>) {
        let Some((from, to)) = self.preview_fade.take() else { return };
        let amount =
            amount.unwrap_or_else(|| self.fading.map_or(1.0, |(started, look)| fade_amount(started, look.duration())));
        if amount >= 1.0 {
            let painted = self.paint_live_preview(&to);
            self.preview_shown = Some(if painted { to } else { from });
            return;
        }
        let (w, h) = LIVE_PREVIEW_SIZE;
        let look = self.fading.map_or(sp_model::Transition::default(), |(_, look)| look);
        let mixed = sp_render::transition(look.kind, look.direction, &from, &to, amount, w, h);
        self.paint_live_preview(&mixed);
        self.preview_fade = Some((from, to));
    }

    /// Paints a picture into the live preview texture the window isn't showing, then tells the
    /// window to show it; true when it was painted.
    fn paint_live_preview(&mut self, scene: &vello::Scene) -> bool {
        use std::sync::atomic::Ordering::{Acquire, Release};
        self.previewed_at = Instant::now();
        if self.round.slow {
            self.preview_state.skipped();
            return false;
        }
        let (w, h) = LIVE_PREVIEW_SIZE;
        let back = 1 - self.live_preview.front.load(Acquire).min(1);
        let view = self.preview_targets[back].create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.paint(scene, &view, w, h) {
            self.preview_state.failed(&e);
            return false;
        }
        self.preview_state.painted();
        self.live_preview.front.store(back, Release);
        self.live_preview.drawn.fetch_add(1, Release);
        // Whatever moved (a video, a build, scrolling words, a timer), the window is woken
        // to show it; once, until it has.
        if !self.live_preview.wake_pending.swap(true, Acquire)
            && let Some(wake) = &self.waker
        {
            wake();
        }
        true
    }

    fn draw_now(&mut self, new_picture: bool) {
        self.round.begin();
        // Cheap copies: the picture and the slide are shared, not duplicated.
        let frame = self.frame.clone();
        self.send_ndi(&frame, NdiChange::Any);
        self.capture(&frame);
        // How far a fade already under way has got, before the clock is started again.
        let amount = self.fading.map(|(started, look)| fade_amount(started, look.duration()));
        let mut fading = false;
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            let scene = self.scene_for(id, &frame, w, h);
            let stage = self.is_stage(id);
            // What the next fade starts from: the last whole picture, or for a fade cut short,
            // one of its two whole pictures.
            let target = &mut self.targets[index];
            let before = match target.fade.take() {
                Some((from, to)) => Some(fade_start(from, to, amount.unwrap_or(1.0), new_picture)),
                None => target.shown.take(),
            };
            // Stage screens change at once: the people on stage need the words now.
            match before {
                Some(from) if new_picture && self.look.duration() > 0.0 && !stage => {
                    target.fade = Some((from, scene));
                    fading = true;
                }
                // A picture that could not be painted was never shown: the next fade starts
                // from the one that was.
                before => {
                    let painted = self.present(index, &scene);
                    self.targets[index].shown = if painted { Some(scene) } else { before };
                }
            }
        }
        self.fading = fading.then(|| (Instant::now(), self.look));
        // The preview follows, fading if the screens do.
        self.draw_live_preview(&frame, new_picture);
        if fading {
            self.fade_step();
        }
        self.end_round();
    }

    /// Paints one picture of the drawing under way, and notes how it went. Callers ask first
    /// whether the GPU has been too slow with a picture of this drawing (`round.slow`) and do
    /// not ask for theirs then: a GPU that is stuck holds the thread for one wait, not for one
    /// wait for every window.
    fn paint(
        &mut self,
        scene: &vello::Scene,
        view: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> Result<(), sp_render::PaintError> {
        // Testing aid: SOFTPRESENTER_GPU_TOO_SLOW (see `PretendSlow`).
        let pretending =
            self.pretend_slow.as_ref().is_some_and(|slow| slow.left > 0 && self.started.elapsed() >= slow.after);
        if self.pretend_slow.is_some() {
            self.painter.wait_at_most(pretending.then_some(Duration::ZERO));
        }
        let painted = self.painter.paint(&self.device, &self.queue, scene, view, w, h);
        // Only a picture that was given up counts: one drawn before is not waited for.
        if pretending
            && painted.as_ref().is_err_and(sp_render::PaintError::worth_trying_again)
            && let Some(slow) = &mut self.pretend_slow
        {
            slow.left -= 1;
        }
        self.round.note(&painted);
        painted
    }

    /// Ends a drawing, of all the screens or some: sets them to be drawn again if the GPU was
    /// too slow, tells the operator's window when something is not showing what it should, and
    /// keeps a drawing in which nothing could be shown from being followed by the next at once.
    fn end_round(&mut self) {
        // Given up: the pictures that were waiting for another try are not coming for now.
        if self.round.end(Instant::now()) == Ended::GivenUp {
            self.targets.iter_mut().for_each(|target| target.state.given_up());
            self.ndi_state.given_up();
            self.preview_state.given_up();
        }
        let worst = worst_undrawn(self.targets.iter().map(|target| target.state).chain([
            self.ndi_state,
            self.ndi_lost.shown(),
            self.preview_state,
        ]));
        let code = worst.map_or(0, |undrawn| undrawn as u8);
        if self.live_preview.undrawn.swap(code, std::sync::atomic::Ordering::AcqRel) != code {
            let told = worst.map_or("Every screen shows what it was asked to again.", Undrawn::in_words);
            crate::applog::event("output", &format!("The operator's window is told: {told}"));
            if let Some(wake) = &self.waker {
                wake();
            }
        }
        if self.round.failed && !self.round.presented {
            std::thread::sleep(UNDRAWN_PAUSE);
        }
    }

    /// Testing aid: saves what the audience screens show, once, when something is on them a few
    /// seconds after the start, to the file SOFTPRESENTER_CAPTURE_OUTPUT names
    /// (SOFTPRESENTER_CAPTURE_OUTPUT_AFTER seconds, 3 if not given).
    fn capture(&mut self, frame: &Frame) {
        static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let Some(path) = std::env::var_os("SOFTPRESENTER_CAPTURE_OUTPUT") else { return };
        let after = std::env::var("SOFTPRESENTER_CAPTURE_OUTPUT_AFTER").ok().and_then(|s| s.parse().ok());
        let started = *STARTED.get_or_init(Instant::now);
        // Taken once something is on screen.
        let showing = frame.slide.is_some() || frame.media.is_some();
        if started.elapsed().as_secs_f32() < after.unwrap_or(3.0)
            || !showing
            || CAPTURE_DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let (w, h) = NDI_SIZE;
        // The first screen's picture, as its window shows it (its look, its identify marks).
        let id = self.targets.first().map_or(Uuid::nil(), |target| target.id);
        let scene = self.scene_for(id, frame, w, h);
        let target = target_texture(&self.device, w, h);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let saved = self
            .painter
            .paint(&self.device, &self.queue, &scene, &view, w, h)
            .map_err(|e| e.to_string())
            .and_then(|()| read_pixels(&self.device, &self.queue, &target).map_err(|e| e.to_string()))
            .and_then(|pixels| {
                let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).map_err(|e| e.to_string())
            });
        match saved {
            Ok(()) => eprintln!("saved the output to {}", std::path::Path::new(&path).display()),
            Err(e) => eprintln!("could not save the output: {e}"),
        }
    }

    /// Takes the playing video's newest picture, if a new one has come. True when it has.
    /// How long to wait before looking at a playing video for a new picture: a refresh's
    /// worth while pictures come, longer once none has come for a second.
    fn video_poll(&self) -> Duration {
        if self.video_moved_at.elapsed() < Duration::from_secs(1) { VIDEO_POLL } else { VIDEO_IDLE_POLL }
    }

    fn take_video_picture(&mut self) -> bool {
        let mut new = false;
        // The video behind the slide: its pixels go straight from the player into a texture on
        // the GPU, the same one for every picture, with nothing copied or put in another order
        // on the way. Only this thread's painter draws it (the screens, the live preview, NDI).
        let Output { frame, painter, device, queue, video_texture, .. } = self;
        let shown = frame.video.as_ref().and_then(|frames| {
            frames.next_with(|lent| {
                // The GPU draws each pixel's fourth byte as its alpha. A picture shown opaque
                // whatever those bytes say goes there as it is only when every one is full
                // (as the system's player decodes all but a video with clear parts); a picture
                // where one isn't is drawn from a copy made opaque.
                if lent.opaque && !lent.fourth_bytes_full() {
                    return copied(lent);
                }
                let order = pixel_order(lent.order);
                // The video's first picture, or one of another size than its last: a new texture.
                if !video_texture.as_ref().is_some_and(|kept| kept.fits(lent.width, lent.height, order)) {
                    *video_texture = painter.moving_picture(device, lent.width, lent.height, order);
                }
                video_texture
                    .as_ref()
                    .filter(|kept| painter.show_rows(queue, kept, lent.bytes, lent.bytes_per_row))
                    .map(MovingPicture::picture)
                    // A picture too large for a texture is drawn from a copy.
                    .or_else(|| copied(lent))
            })
        });
        if let Some(shown) = shown {
            self.video_picture = shown;
            self.video_pictures += 1;
            new |= self.video_picture.is_some();
        }
        // Each video in a box on the slide: its newest picture stands in for the box's still.
        // It is a copy, because every thread that draws the slide (the operator's window too)
        // shows it.
        for (file, frames) in &self.frame.slide_video {
            if let Some(Some(picture)) = frames.next_with(copied) {
                sp_render::show_picture(file, picture);
                self.video_pictures += 1;
                new = true;
            }
        }
        if new {
            self.video_moved_at = Instant::now();
        }
        new
    }

    /// True when something on the screens changes by itself each second: the time on a stage
    /// screen, or a message with the time or a timer in it.
    fn ticks(&self) -> bool {
        self.has_stage()
            || self.overlay.message.as_ref().is_some_and(|(text, _)| crate::messages::has_tokens(text))
            || self.links_tick()
    }

    /// True while text on the audience screens scrolls, so it is drawn again at each refresh.
    fn scrolling(&self) -> bool {
        self.frame.enabled
            && self.overlay.test_pattern.is_none()
            && (self.frame.slide.as_deref().is_some_and(|slide| sp_render::scrolls(slide, &self.frame.theme))
                || self.overlay.props.iter().any(|prop| sp_render::scrolls(prop, &self.frame.theme)))
    }

    /// True when linked text on the audience screens shows the time or a timer.
    fn links_tick(&self) -> bool {
        self.frame.enabled
            && (self.frame.slide.as_deref().is_some_and(crate::links::ticks)
                || self.overlay.props.iter().any(crate::links::ticks))
    }

    /// True for a stage screen.
    fn is_stage(&self, id: Uuid) -> bool {
        self.stage.iter().any(|(have, _)| *have == id)
    }

    /// True when there is a stage screen to keep the time on.
    fn has_stage(&self) -> bool {
        self.stage_on
            && (self.targets.iter().any(|target| self.is_stage(target.id))
                || self.ndi.iter().any(|(id, _)| self.is_stage(*id)))
    }

    /// Draws the stage screens again, for the time, leaving the others as they are.
    fn draw_stage(&mut self) {
        self.round.begin_part();
        let frame = self.frame.clone();
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            if !self.is_stage(id) {
                continue;
            }
            let scene = self.scene_for(id, &frame, w, h);
            if self.present(index, &scene) {
                self.targets[index].shown = Some(scene);
            }
        }
        // A stage screen sent over NDI gets the new time too, the same way as every NDI picture
        // (never waited on).
        if self.ndi.iter().any(|(id, _)| self.is_stage(*id)) {
            self.send_ndi(&frame, NdiChange::Stage);
        }
        self.end_round();
    }

    /// Draws the next step of the fade under way in every window, and finishes it when its
    /// time is up.
    fn step_fade(&mut self) {
        self.round.begin();
        self.fade_step();
        self.end_round();
    }

    /// The step itself, as part of the drawing under way.
    fn fade_step(&mut self) {
        let Some((started, look)) = self.fading else { return };
        let amount = fade_amount(started, look.duration());
        // A video playing behind the new slide keeps moving through the fade: each new picture
        // of it builds the picture being faded to again.
        if self.take_video_picture() {
            let frame = self.frame.clone();
            for index in 0..self.targets.len() {
                let (id, w, h) = {
                    let target = &self.targets[index];
                    (target.id, target.config.width, target.config.height)
                };
                if self.targets[index].fade.is_some() {
                    let fresh = self.scene_for(id, &frame, w, h);
                    if let Some((_, to)) = self.targets[index].fade.as_mut() {
                        *to = fresh;
                    }
                }
            }
        }
        for index in 0..self.targets.len() {
            let Some((from, to)) = self.targets[index].fade.take() else { continue };
            if amount >= 1.0 {
                // A last picture that could not be painted was never shown: the window keeps
                // the one the fade started from, for the next fade to start from.
                let painted = self.present(index, &to);
                self.targets[index].shown = Some(if painted { to } else { from });
            } else {
                let (w, h) = (self.targets[index].config.width, self.targets[index].config.height);
                let scene = sp_render::transition(look.kind, look.direction, &from, &to, amount, w, h);
                self.present(index, &scene);
                self.targets[index].fade = Some((from, to));
            }
        }
        self.step_live_preview(Some(amount));
        if amount >= 1.0 || self.targets.iter().all(|target| target.fade.is_none()) {
            self.fading = None;
            // An NDI picture that could not be drawn as the fade began, or during it, had no
            // drawing to be tried again in: the steps of a fade draw the windows only, and the
            // time set for another try is dropped while one runs (whether it was still being
            // tried or had been given up for now). It is tried now, as part of this drawing,
            // which sets a time for the next try if it must.
            if self.ndi_state != Shown::default() {
                let frame = self.frame.clone();
                self.send_ndi(&frame, NdiChange::Any);
            }
        }
        // NDI pictures that have come back from the GPU are handed over at every step: NDI
        // shows the new slide at once, as the stage screens do, not when the windows' fade
        // ends. After the windows: this may paint an NDI picture that was put off, and one the
        // GPU is too slow with would keep the windows' step from being asked for. (An NDI
        // picture that could not be drawn is not tried again here: it waits for the end of
        // the fade, above, so that the room's fade is never held up for NDI.)
        self.finish_ndi();
    }

    /// How long the thread may sleep with nothing to draw before it must look again: not at
    /// all (`None`) unless the GPU still holds memory the painter gave back (`rested` is
    /// false), or a picture the GPU was too slow with is waiting to be drawn again.
    fn look_again_in(&self, rested: bool) -> Option<Duration> {
        let for_memory = (!rested).then_some(GPU_LOOK_AGAIN);
        [for_memory, self.draw_again_in()].into_iter().flatten().min()
    }

    /// How long until a picture the GPU was too slow with, or NDI pictures lost on their way
    /// back from it, are due to be drawn again, if any are.
    fn draw_again_in(&self) -> Option<Duration> {
        let at = [self.round.again_at, self.ndi_lost.again_at].into_iter().flatten().min()?;
        Some(at.saturating_duration_since(Instant::now()))
    }

    /// Draws the screens again if a picture the GPU was too slow with is due another try, and
    /// says whether it did. During a fade it does not: the fade's own steps draw the screens,
    /// and a drawing of the whole picture would cut the fade short.
    fn draw_again_if_due(&mut self) -> bool {
        if self.fading.is_some() {
            self.round.again_at = None;
            return false;
        }
        let now = Instant::now();
        if self.round.again_at.is_some_and(|at| at <= now) {
            self.draw(false);
            return true;
        }
        // NDI pictures lost on their way back are drawn again alone: the windows are well.
        // (After a fade, not during one: this waits as the drawing above does.)
        let lost_due = self.ndi_lost.take_due(now);
        if lost_due {
            self.draw_ndi();
        }
        lost_due
    }

    /// Paints a picture and shows it in one window; true when it was painted. A picture that
    /// cannot be painted leaves the one before it on the screen, and is said in the log once
    /// for as long as that lasts. (Too heavy stays too heavy; too slow may pass, and the end
    /// of the drawing sets the screens to be drawn again.)
    fn present(&mut self, index: usize, scene: &vello::Scene) -> bool {
        let Some(target) = self.targets.get(index) else { return false };
        let (w, h) = (target.config.width, target.config.height);
        let view = target.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Not asked for after a picture the GPU was too slow with: it waits for the next try,
        // and nothing is said of it, since nothing went wrong with it.
        if self.round.slow {
            self.targets[index].state.skipped();
            return false;
        }
        let painted = self.paint(scene, &view, w, h);
        let Output { device, queue, targets, round, .. } = self;
        let target = &mut targets[index];
        if let Err(e) = painted {
            let message = format!("The output could not draw a frame, and keeps what it shows: {e}");
            if target.trouble.is_news(&message) {
                crate::applog::problem("output", &message);
            }
            target.state.failed(&e);
            return false;
        }
        target.state.painted();
        if let Some(more) = target.trouble.over() {
            crate::applog::event("output", &format!("The output draws again, after {more} more frames not drawn"));
        }
        // A window whose surface isn't set up is set up first, or skipped this frame.
        if !target.configured {
            target.configured = configure(device, &target.surface, &target.config);
            if !target.configured {
                std::thread::sleep(Duration::from_millis(8));
                return true;
            }
        }
        // A lost or outdated surface is set up again and tried once more; anything else
        // (hidden window, timeout) skips this frame and the next message redraws.
        for attempt in 0..2 {
            let Some(next) = next_texture(&target.surface) else {
                target.configured = false;
                return true;
            };
            match next {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    target.blitter.copy(device, &mut encoder, &view, &surface_view);
                    queue.submit([encoder.finish()]);
                    queue.present(frame);
                    round.presented = true;
                    return true;
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                    target.configured = configure(device, &target.surface, &target.config);
                    if !target.configured {
                        return true;
                    }
                }
                // A hidden window shows nothing; wait a moment so a fade does not spin.
                _ => {
                    std::thread::sleep(Duration::from_millis(8));
                    return true;
                }
            }
        }
        true
    }
}

/// What a fade cut short by a new picture is followed from: the whole picture nearer to what the
/// screen shows (the old one before half-way, the new one after), never the part-way mix itself.
/// A mix would hold both pictures, and the next one cut short would hold that mix and another,
/// so an operator clicking on faster than fades finish would pile slide on slide into every
/// frame, each slower to draw than the last, and keep the memory for all of them.
fn fade_from<T>(from: T, to: T, amount: f32) -> T {
    if amount < 0.5 { from } else { to }
}

/// What the next fade starts from when one is cut short. By a new picture: the picture the fade
/// was going to, however far it had got, as the operator's preview does. Stepping on faster than
/// fades finish then shows each slide in turn; starting from the nearer picture instead kept
/// the first slide on the screen for as long as the steps came quickly, the others passing
/// behind it. By a redraw of the same picture: the nearer of the two, as `fade_from` gives.
fn fade_start<T>(from: T, to: T, amount: f32, new_picture: bool) -> T {
    if new_picture { to } else { fade_from(from, to, amount) }
}

/// How a screen being identified is marked: three quick breaths of its colour, from a tenth to a
/// fifth of the way over the picture and back, about 0.7 s each; then a steady tenth.
const IDENTIFY_BREATHS: f32 = 3.0;
const IDENTIFY_BREATH: f32 = 0.7;

/// How strongly an identified screen's colour lies over it, `seconds` after identifying began.
fn identify_strength(seconds: f32) -> f32 {
    if seconds >= IDENTIFY_BREATHS * IDENTIFY_BREATH {
        return 0.10;
    }
    0.15 - 0.05 * (seconds / IDENTIFY_BREATH * std::f32::consts::TAU).cos()
}

/// Lays an identified screen's marks over its picture: its colour over the whole picture at
/// `strength`, and a solid border of it round the edge.
fn identify_marks(scene: &mut vello::Scene, color: sp_model::Rgba, strength: f32, w: u32, h: u32) {
    use vello::kurbo::{Affine, Rect, Stroke};
    use vello::peniko::{Color, Fill};
    let (w, h) = (f64::from(w), f64::from(h));
    let solid = Color::from_rgba8(color.r, color.g, color.b, 255);
    scene.fill(Fill::NonZero, Affine::IDENTITY, solid.with_alpha(strength), None, &Rect::new(0.0, 0.0, w, h));
    let border = (h / 1080.0 * 16.0).max(6.0);
    let half = border / 2.0;
    scene.stroke(&Stroke::new(border), Affine::IDENTITY, solid, None, &Rect::new(half, half, w - half, h - half));
}

/// The theme a screen's slides are drawn with: its own, if it has one, or the one the slide
/// came with (the main theme, or a song's own).
fn screen_theme(themes: &[(Uuid, Arc<Theme>)], screen: Uuid, slide_theme: &Arc<Theme>) -> Arc<Theme> {
    themes.iter().find(|(have, _)| *have == screen).map_or_else(|| slide_theme.clone(), |(_, own)| own.clone())
}

/// Testing aid: whether the picture of the output SOFTPRESENTER_CAPTURE_OUTPUT asks for is
/// still to be taken.
static CAPTURE_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn capture_wanted() -> bool {
    std::env::var_os("SOFTPRESENTER_CAPTURE_OUTPUT").is_some()
        && !CAPTURE_DONE.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slow() -> Result<(), sp_render::PaintError> {
        Err(sp_render::PaintError::NotNow("too slow".to_owned()))
    }

    /// One drawing in which the GPU was too slow with a picture, and how it ended.
    fn slow_drawing(round: &mut Round, now: Instant) -> Ended {
        round.begin();
        round.note(&slow());
        round.end(now)
    }

    #[test]
    fn a_slow_gpu_holds_a_drawing_for_one_picture_and_is_tried_again_a_few_times() {
        let mut round = Round::default();
        let now = Instant::now();
        // A drawing in which nothing was slow: every picture is asked for, nothing follows.
        round.begin();
        round.note(&Ok(()));
        assert!(!round.slow);
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!(round.again_at, None);
        // Too heavy is not too slow: the other pictures are still asked for, and nothing is
        // tried again.
        round.begin();
        round.note(&Err(sp_render::PaintError::TooHeavy));
        assert!(!round.slow && round.failed);
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!(round.again_at, None);
        // The GPU too slow with one picture: the rest of that drawing's are not asked for, and
        // the drawing counts as one try however many pictures it had.
        for tried in 1..=DRAW_AGAIN_TRIES {
            round.begin();
            assert!(!round.slow, "a new drawing asks again");
            round.note(&slow());
            assert!(round.slow, "the pictures after it are not asked for");
            assert_eq!(round.end(now), Ended::TryAgain);
            assert_eq!(round.tries, tried);
            assert_eq!(round.again_at, Some(now + DRAW_AGAIN_AFTER));
        }
        // Slow again after every try: given up, and drawn again only now and then, for as long
        // as it goes on.
        for _ in 0..3 {
            assert_eq!(slow_drawing(&mut round, now), Ended::GivenUp);
            assert_eq!(round.again_at, Some(now + GIVEN_UP_AGAIN_AFTER));
        }
        // The same picture sent again does not get new tries; another picture does.
        assert_eq!(slow_drawing(&mut round, now), Ended::GivenUp);
        round.new_picture();
        assert_eq!(slow_drawing(&mut round, now), Ended::TryAgain);
        assert_eq!(round.again_at, Some(now + DRAW_AGAIN_AFTER));
        // And a drawing that went well ends the count.
        round.begin();
        round.note(&Ok(()));
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!((round.tries, round.again_at), (0, None));
    }

    #[test]
    fn a_drawing_given_up_in_a_fade_or_at_the_end_of_a_build_is_still_drawn_again() {
        let mut round = Round::default();
        let now = Instant::now();
        // Steps of a fade, each a drawing of its own, the GPU too slow in every one: each
        // takes the place of the one set before it, and the last one, tried or given up,
        // leaves a time set, though nothing is moving any more to draw the screens.
        for step in 0..12 {
            let ended = slow_drawing(&mut round, now);
            let after = if step < DRAW_AGAIN_TRIES { DRAW_AGAIN_AFTER } else { GIVEN_UP_AGAIN_AFTER };
            assert_eq!(ended == Ended::GivenUp, step >= DRAW_AGAIN_TRIES, "step {step}");
            assert_eq!(round.again_at, Some(now + after), "step {step}");
        }
        // The last picture of a build, the first the GPU is slow with: the same.
        let mut round = Round::default();
        assert_eq!(slow_drawing(&mut round, now), Ended::TryAgain);
        assert_eq!(round.again_at, Some(now + DRAW_AGAIN_AFTER));
        // Any drawing of all the screens begun is that drawing again...
        round.begin();
        assert_eq!(round.again_at, None);
        // ...but one of the stage screens alone, at the start of a second, is not: the time
        // set stands, and the tries are not counted afresh for its having gone well.
        assert_eq!(slow_drawing(&mut round, now), Ended::TryAgain);
        let tries = round.tries;
        round.begin_part();
        round.note(&Ok(()));
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!((round.again_at, round.tries), (Some(now + DRAW_AGAIN_AFTER), tries));
        // So stage screens that go on drawing do not keep the operator from being told: five
        // tries of all the screens with a stage drawing after each, then given up, and then
        // one try every five seconds, not five quick ones.
        let mut round = Round::default();
        for step in 0..8 {
            let ended = slow_drawing(&mut round, now);
            assert_eq!(ended == Ended::GivenUp, step >= DRAW_AGAIN_TRIES, "step {step}");
            round.begin_part();
            round.note(&Ok(()));
            assert_eq!(round.end(now), Ended::Drawn);
        }
        assert_eq!(round.again_at, Some(now + GIVEN_UP_AGAIN_AFTER));
        // Nor do stage screens the GPU is too slow with, second after second, put off the
        // time set for all the screens.
        let later = now + Duration::from_secs(1);
        for _ in 0..4 {
            round.begin_part();
            round.note(&slow());
            assert_eq!(round.end(later), Ended::GivenUp);
            assert_eq!(round.again_at, Some(now + GIVEN_UP_AGAIN_AFTER));
        }
    }

    /// An NDI picture painted between two drawings of the screens (it had to wait for the one
    /// before it to come back from the GPU) is a drawing of its own.
    #[test]
    fn an_ndi_picture_painted_between_drawings_is_tried_again_and_told_like_any_other() {
        let now = Instant::now();
        // The GPU too slow with it: a time is set to draw again, where nothing else would.
        let (mut round, mut ndi) = (Round::default(), Shown::default());
        round.begin_part();
        let painted = slow();
        round.note(&painted);
        ndi.failed(&painted.unwrap_err());
        assert_eq!(round.end(now), Ended::TryAgain);
        assert_eq!(round.again_at, Some(now + DRAW_AGAIN_AFTER));
        assert_eq!(worst_undrawn([ndi]), None, "not said while it is being tried");
        // Too heavy: the operator is told at the end of that drawing, and nothing is set.
        let (mut round, mut ndi) = (Round::default(), Shown::default());
        round.begin_part();
        let too_heavy = sp_render::PaintError::TooHeavy;
        ndi.failed(&too_heavy);
        round.note(&Err(too_heavy));
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!((round.again_at, worst_undrawn([ndi])), (None, Some(Undrawn::TooMuch)));
        // Painted there after it was given up: nothing is left to tell, and the count of tries
        // for all the screens is as it was.
        let mut round = Round::default();
        for _ in 0..=DRAW_AGAIN_TRIES {
            slow_drawing(&mut round, now);
        }
        ndi = Shown::default();
        ndi.skipped();
        ndi.given_up();
        assert_eq!(worst_undrawn([ndi]), Some(Undrawn::NotInTime));
        round.begin_part();
        round.note(&Ok(()));
        ndi.painted();
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!(worst_undrawn([ndi]), None);
        assert_eq!(round.tries, DRAW_AGAIN_TRIES);
    }

    fn seen(wanted: bool, alone: bool) -> NdiSeen {
        NdiSeen { id: Uuid::new_v4(), wanted, alone, unwatched: !wanted, stage: false, arrivals: 0 }
    }

    #[test]
    fn ndi_sources_nobody_watches_are_told_when_their_picture_goes_out_of_date() {
        let (watched, unwatched) = (seen(true, false), seen(false, false));
        let stage = NdiSeen { stage: true, ..seen(false, true) };
        let failed = NdiSeen { unwatched: false, ..seen(false, false) };
        let all = [watched, unwatched, stage, failed];
        // Receivers coming or going change no picture: nobody is told.
        assert!(ndi_out_of_date(&all, NdiChange::Nothing).is_empty());
        // The time on the stage screens: only a stage screen nobody is watching.
        assert_eq!(ndi_out_of_date(&all, NdiChange::Stage), vec![stage.id]);
        // Anything else: every source nobody is watching; never one somebody is watching, nor
        // one that could not start.
        assert_eq!(ndi_out_of_date(&all, NdiChange::Any), vec![unwatched.id, stage.id]);
        // A drawing put off (a picture was on its way back) counts when the next comes, even
        // if that one changes nothing, or is not drawn because the last receiver has left.
        assert_eq!(NdiChange::Nothing.with(Some(NdiChange::Any)), NdiChange::Any);
        assert_eq!(NdiChange::Nothing.with(Some(NdiChange::Stage)), NdiChange::Stage);
        assert_eq!(NdiChange::Any.with(Some(NdiChange::Nothing)), NdiChange::Any);
        assert_eq!(NdiChange::Stage.with(None), NdiChange::Stage);
        assert_eq!(ndi_out_of_date(&all, NdiChange::Nothing.with(Some(NdiChange::Any))), vec![unwatched.id, stage.id]);
    }

    #[test]
    fn an_ndi_picture_on_its_way_back_does_not_go_to_a_source_nobody_watches_any_more() {
        let (watched, left, failed) = (seen(true, false), seen(false, false), seen(false, false));
        let gone = Uuid::new_v4();
        let mut ids = vec![watched.id, left.id, gone, failed.id];
        // The sources no picture is wanted for are taken out and given, to be told that the
        // picture they have is out of date; one switched off meanwhile is left (nothing is
        // handed to it anyway), and the one still watched keeps its picture.
        assert_eq!(ndi_strip(&mut ids, &[watched, left, failed]), vec![left.id, failed.id]);
        assert_eq!(ids, vec![watched.id, gone]);
        assert!(ndi_strip(&mut ids, &[watched, left, failed]).is_empty(), "once");
    }

    #[test]
    fn only_a_sources_first_receivers_have_the_ndi_pictures_drawn() {
        let (one, two) = (seen(true, false), seen(true, false));
        let drawn = |source: NdiSeen| (source.id, source.arrivals);
        // The first receivers of a source not drawn for: draw.
        assert!(ndi_count_needs_drawing(&[one, two], &[drawn(one)], false));
        assert!(ndi_count_needs_drawing(&[one], &[], false));
        // More receivers, or fewer, of sources already drawn for: nothing to draw.
        assert!(!ndi_count_needs_drawing(&[one, two], &[drawn(one), drawn(two)], false));
        // The last receiver leaving: nothing to draw...
        let left = seen(false, false);
        assert!(!ndi_count_needs_drawing(&[one, left], &[drawn(one), drawn(left)], false));
        assert!(!ndi_count_needs_drawing(&[left], &[drawn(left)], false));
        // ...unless a picture was owed, which that drawing puts an end to.
        assert!(ndi_count_needs_drawing(&[left], &[drawn(left)], true));
        assert!(ndi_count_needs_drawing(&[one], &[drawn(one)], true));
        // Receivers left and others came between two looks: the count is what it was, but
        // the source let its picture go when they left, and is drawn for again.
        let back = NdiSeen { arrivals: one.arrivals + 1, ..one };
        assert!(ndi_count_needs_drawing(&[back, two], &[drawn(one), drawn(two)], false));
        assert!(!ndi_count_needs_drawing(&[back, two], &[drawn(back), drawn(two)], false));
    }

    #[test]
    fn an_ndi_picture_lost_is_owed_only_to_sources_still_watched() {
        let (watched, left, other) = (seen(true, false), seen(false, false), seen(true, false));
        let all = [watched, left, other];
        assert!(ndi_lost_is_owed(&[watched.id, left.id], &all));
        assert!(!ndi_lost_is_owed(&[left.id], &all), "its receivers have left: nobody is told, nothing is drawn");
        assert!(!ndi_lost_is_owed(&[Uuid::new_v4()], &all), "its source was switched off");
        assert!(!ndi_lost_is_owed(&[], &all));
    }

    /// One drawing of the NDI pictures, of which one was lost or none, as `send_ndi` and
    /// `finish_ndi` tell it; true when the pictures are to be drawn once more now.
    fn ndi_drawing(lost: &mut Lost, loses: bool, now: Instant) -> bool {
        lost.painting();
        if loses {
            lost.one_lost();
        }
        lost.drawing_back(now)
    }

    #[test]
    fn an_ndi_picture_lost_is_drawn_once_more_then_every_few_seconds_and_told_until_one_comes_back() {
        let now = Instant::now();
        let mut lost = Lost::default();
        // A drawing that comes back whole: nothing to do or tell.
        assert!(!ndi_drawing(&mut lost, false, now));
        assert_eq!(lost, Lost::default());
        // A picture lost: drawn once more at once, and nothing said yet (it is being put right).
        assert!(ndi_drawing(&mut lost, true, now));
        assert!(lost.owed());
        assert_eq!((worst_undrawn([lost.shown()]), lost.again_at), (None, None));
        // Lost again: the operator is told, and it is drawn again in five seconds, not now.
        assert!(!ndi_drawing(&mut lost, true, now));
        assert_eq!(worst_undrawn([lost.shown()]), Some(Undrawn::Failed));
        assert_eq!(lost.again_at, Some(now + GIVEN_UP_AGAIN_AFTER));
        assert!(!lost.take_due(now + GIVEN_UP_AGAIN_AFTER / 2));
        assert!(lost.take_due(now + GIVEN_UP_AGAIN_AFTER));
        assert!(!lost.take_due(now + GIVEN_UP_AGAIN_AFTER), "due once");
        // Painting again (that try, or a new slide) does not take the warning away: only a
        // picture handed over does.
        lost.painting();
        assert_eq!(worst_undrawn([lost.shown()]), Some(Undrawn::Failed));
        // The drawing comes back whole: all is well, and the next loss is drawn once more.
        assert!(!lost.drawing_back(now));
        assert_eq!(lost, Lost::default());
        assert!(ndi_drawing(&mut lost, true, now));
        // Another drawing that paints while one is to be drawn again takes its place.
        assert!(!ndi_drawing(&mut lost, true, now));
        assert!(lost.again_at.is_some());
        lost.painting();
        assert_eq!(lost.again_at, None);
    }

    /// Two pictures to a drawing (a screen with its own look, and the one the others share),
    /// and one of them is lost every time while the other comes back.
    #[test]
    fn one_ndi_picture_lost_every_time_beside_one_that_comes_back_is_not_drawn_without_end() {
        let now = Instant::now();
        let mut lost = Lost::default();
        let at_once = (0..100).filter(|_| ndi_drawing(&mut lost, true, now)).count();
        assert_eq!(at_once, 1, "drawn once more at once, and never again at once");
        assert_eq!(worst_undrawn([lost.shown()]), Some(Undrawn::Failed), "the operator is told throughout");
        assert_eq!(lost.again_at, Some(now + GIVEN_UP_AGAIN_AFTER), "and it is drawn again now and then");
    }

    #[test]
    fn the_switch_for_losing_ndi_pictures_takes_a_count_and_a_place() {
        assert_eq!(PretendLost::parse("3"), Some(PretendLost { left: 3, place: None }));
        assert_eq!(PretendLost::parse(" 40 @ 2 "), Some(PretendLost { left: 40, place: Some(1) }));
        for bad in ["", "x", "-1", "3@", "3@0", "3@x", "@2", "1@2@3", "4294967296"] {
            assert_eq!(PretendLost::parse(bad), None, "{bad:?}");
        }
        // Any picture, three times.
        let mut any = PretendLost::parse("3").unwrap();
        assert_eq!([0, 1, 0, 1].map(|place| any.takes(place)), [true, true, true, false]);
        // Only the first picture of each drawing, twice: the second always comes back.
        let mut first = PretendLost::parse("2@1").unwrap();
        assert_eq!([0, 1, 0, 1, 0, 1].map(|place| first.takes(place)), [true, false, true, false, false, false]);
    }

    #[test]
    fn no_ndi_picture_is_drawn_for_a_source_nobody_is_watching() {
        let (sending, starting) = (sp_ndi::State::Sending, sp_ndi::State::Starting);
        assert!(!ndi_wanted(&sending, Some(0)), "nobody watching: nothing drawn");
        assert!(ndi_wanted(&sending, Some(1)) && ndi_wanted(&sending, Some(2)));
        assert!(ndi_wanted(&sending, None), "a count that isn't known is drawn for, as before");
        assert!(ndi_wanted(&starting, None), "and so is a source still starting");
        assert!(!ndi_wanted(&sp_ndi::State::Failed("NDI stopped.".to_owned()), None));
        assert!(!ndi_wanted(&sp_ndi::State::Failed("NDI stopped.".to_owned()), Some(3)));
        // Two sources showing the show as it is share one picture; one with a look of its own
        // has its own, drawn first.
        let (one, two, lower) = (seen(true, false), seen(true, false), seen(true, true));
        assert_eq!(ndi_groups(&[one, lower, two]), vec![vec![lower.id], vec![one.id, two.id]]);
        // A source nobody is watching is in no picture's list...
        let unwatched = seen(false, false);
        assert_eq!(ndi_groups(&[one, unwatched]), vec![vec![one.id]]);
        // ...and a picture nobody is watching is not drawn at all: not the shared one, not a
        // source's own.
        assert_eq!(ndi_groups(&[unwatched, seen(false, false), lower]), vec![vec![lower.id]]);
        assert_eq!(ndi_groups(&[one, seen(false, true)]), vec![vec![one.id]]);
        assert!(ndi_groups(&[unwatched, seen(false, true)]).is_empty());
        assert!(ndi_groups(&[]).is_empty());
    }

    /// With nobody watching, a drawing asks for no NDI picture (as `send_ndi` does when
    /// `ndi_groups` is empty): that is neither undrawn nor waited for.
    #[test]
    fn ndi_nobody_is_watching_is_not_undrawn_and_owes_nothing_from_before() {
        let now = Instant::now();
        // A drawing of the windows with the NDI sources unwatched: no picture of theirs is
        // noted, nothing is said, and nothing is set to be drawn again.
        let (mut round, mut ndi) = (Round::default(), Shown::default());
        round.begin();
        round.note(&Ok(()));
        ndi.not_asked();
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!((round.again_at, round.failed, worst_undrawn([ndi])), (None, false, None));
        // Refused as too heavy while someone watched, then the last receiver leaves: the
        // operator is no longer told of a picture nobody is owed.
        ndi.failed(&sp_render::PaintError::TooHeavy);
        assert_eq!(worst_undrawn([ndi]), Some(Undrawn::TooMuch));
        ndi.not_asked();
        assert_eq!(worst_undrawn([ndi]), None);
        // Too slow and given up, then unwatched: the same.
        ndi.failed(&slow().unwrap_err());
        ndi.given_up();
        assert_eq!(worst_undrawn([ndi]), Some(Undrawn::NotInTime));
        ndi.not_asked();
        assert_eq!(ndi, Shown::default());
        // Still waiting for another try when the receiver left: it is not waited for any
        // more, so when the tries of the windows' drawing are used up, nothing is said of NDI.
        ndi.skipped();
        ndi.not_asked();
        ndi.given_up();
        assert_eq!(worst_undrawn([ndi]), None);
    }

    /// A receiver comes back and the picture drawn for it (a drawing of the NDI pictures
    /// alone, as `draw_ndi` makes) cannot be painted: tried again and told like any other.
    #[test]
    fn an_ndi_picture_drawn_for_a_receiver_that_came_back_is_tried_again_and_told() {
        let now = Instant::now();
        let (mut round, mut ndi) = (Round::default(), Shown::default());
        ndi.not_asked();
        // The GPU too slow with it: a time is set for another try, and nothing said yet.
        round.begin_part();
        let painted = slow();
        round.note(&painted);
        ndi.failed(&painted.unwrap_err());
        assert_eq!(round.end(now), Ended::TryAgain);
        assert_eq!(round.again_at, Some(now + DRAW_AGAIN_AFTER));
        assert_eq!(worst_undrawn([ndi]), None);
        // The tries that follow are drawings of everything; slow every time, it is given up
        // and the operator told, and it goes on being tried now and then.
        let mut ended = Ended::TryAgain;
        while ended == Ended::TryAgain {
            round.begin();
            round.note(&slow());
            ndi.skipped();
            ended = round.end(now);
        }
        ndi.given_up();
        assert_eq!(worst_undrawn([ndi]), Some(Undrawn::NotInTime));
        assert_eq!(round.again_at, Some(now + GIVEN_UP_AGAIN_AFTER));
        // Painted at last: nothing is left to tell.
        round.begin();
        round.note(&Ok(()));
        ndi.painted();
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!((worst_undrawn([ndi]), round.again_at), (None, None));
        // Too heavy for the receiver that came back: told at the end of that drawing.
        round.begin_part();
        let too_heavy = sp_render::PaintError::TooHeavy;
        ndi.failed(&too_heavy);
        round.note(&Err(too_heavy));
        assert_eq!(round.end(now), Ended::Drawn);
        assert_eq!(worst_undrawn([ndi]), Some(Undrawn::TooMuch));
    }

    #[test]
    fn only_the_ndi_pictures_that_may_have_changed_are_out_of_date_for_whoever_was_not_watching() {
        // Receivers coming and going change no picture.
        assert!(!NdiChange::Nothing.touches(false) && !NdiChange::Nothing.touches(true));
        // The time on the stage screens changes theirs, not the audience screens'.
        assert!(NdiChange::Stage.touches(true) && !NdiChange::Stage.touches(false));
        assert!(NdiChange::Any.touches(false) && NdiChange::Any.touches(true));
        // Of drawings put off while a picture was on its way back, the most different counts.
        assert_eq!(NdiChange::Nothing.max(NdiChange::Stage), NdiChange::Stage);
        assert_eq!(NdiChange::Any.max(NdiChange::Stage), NdiChange::Any);
        assert_eq!(NdiChange::Nothing.max(NdiChange::Nothing), NdiChange::Nothing);
    }

    #[test]
    fn an_ndi_source_says_how_many_are_watching_it() {
        let words = |receivers| NdiStatus::sending_in_words("Sanctuary", receivers);
        assert_eq!(words(None), "Sending as \"Sanctuary\" at 1920 x 1080");
        assert_eq!(words(Some(0)), "On NDI as \"Sanctuary\"; nobody is watching");
        assert_eq!(words(Some(1)), "Sending as \"Sanctuary\" to 1 receiver");
        assert_eq!(words(Some(2)), "Sending as \"Sanctuary\" to 2 receivers");
    }

    #[test]
    fn the_operator_is_told_the_worst_of_what_is_not_shown_and_only_while_it_lasts() {
        let mut window = Shown::default();
        let mut ndi = Shown::default();
        let told = |window: Shown, ndi: Shown| worst_undrawn([window, ndi]);
        assert_eq!(told(window, ndi), None);
        // Too slow: nothing is said while it is still being tried, then that it was not in time.
        window.failed(&slow().unwrap_err());
        ndi.skipped();
        assert_eq!(told(window, ndi), None);
        window.given_up();
        ndi.given_up();
        assert_eq!((window.undrawn, ndi.undrawn), (Some(Undrawn::NotInTime), Some(Undrawn::NotInTime)));
        // Giving up again changes nothing for one that was not waiting.
        let mut drawn = Shown::default();
        drawn.given_up();
        assert_eq!(drawn, Shown::default());
        // Too heavy is said at once, and is worse.
        ndi.failed(&sp_render::PaintError::TooHeavy);
        assert_eq!(told(window, ndi), Some(Undrawn::TooMuch));
        // The window draws again; NDI is refused still.
        window.painted();
        assert_eq!(told(window, ndi), Some(Undrawn::TooMuch));
        // NDI is switched off after a refusal (as `set_ndi` does): nothing is owed any more.
        ndi = Shown::default();
        assert_eq!(told(window, ndi), None);
        // A picture slow again after it was said to be too heavy: what was said stands.
        ndi.failed(&sp_render::PaintError::TooHeavy);
        ndi.failed(&slow().unwrap_err());
        assert_eq!(ndi.undrawn, Some(Undrawn::TooMuch));
        for undrawn in [Undrawn::NotInTime, Undrawn::Failed, Undrawn::TooMuch] {
            assert_eq!(Undrawn::from_code(undrawn as u8), Some(undrawn));
            assert!(undrawn.in_words().contains("not showing it"));
        }
        assert_eq!(Undrawn::from_code(0), None);
    }

    #[test]
    fn the_testing_switches_take_only_what_they_can_use() {
        let slow = |left, seconds| Some(PretendSlow { left, after: Duration::from_secs_f32(seconds) });
        assert_eq!(PretendSlow::parse("3"), slow(3, 0.0));
        assert_eq!(PretendSlow::parse("40@24"), slow(40, 24.0));
        assert_eq!(PretendSlow::parse(" 6 @ 1.5 "), slow(6, 1.5));
        for bad in ["", "x", "-1", "3@", "3@-1", "3@nan", "3@inf", "3@-inf", "3@1e40", "@2", "1@2@3", "4294967296"] {
            assert_eq!(PretendSlow::parse(bad), None, "{bad:?}");
        }
        let room = output_room("20000").expect("room");
        assert_eq!((room.lines, room.seg_counts, room.segments), (20_000, 20_000, 20_000));
        assert_eq!(room.tiles, vello::ScratchSizes::RESTING.tiles);
        assert!(output_room(" 1 ").is_some() && output_room("8000000").is_some());
        for bad in ["", "0", "-5", "8000001", "4294967295", "99999999999", "many", "2e4", "20000 lines"] {
            assert!(output_room(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn a_screen_with_a_theme_of_its_own_draws_with_it_and_the_rest_with_the_slides() {
        let main = Arc::new(Theme::builtin());
        let mut lower = Theme::builtin();
        lower.id = Uuid::new_v4();
        lower.name = "Lower third".to_owned();
        let lower = Arc::new(lower);
        let (stream, audience) = (Uuid::new_v4(), Uuid::new_v4());
        let themes = vec![(stream, lower.clone())];
        assert_eq!(screen_theme(&themes, stream, &main).name, "Lower third");
        assert_eq!(screen_theme(&themes, audience, &main).name, "Built-in");
    }

    #[test]
    fn slides_clicked_through_faster_than_fades_never_pile_up_in_a_frame() {
        use vello::kurbo::{Affine, Rect};
        use vello::peniko::{Color, Fill};
        let slide = |shade: u8| {
            let mut scene = vello::Scene::new();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(shade, 0, 0),
                None,
                &Rect::new(0.0, 0.0, 10.0, 10.0),
            );
            scene
        };
        let size = |scene: &vello::Scene| scene.encoding().draw_tags.len();
        let kind = sp_model::TransitionKind::Dissolve;
        let one = size(&sp_render::transition(kind, sp_model::Direction::default(), &slide(0), &slide(1), 0.3, 10, 10));
        // A hundred fades, each cut short by the next picture, some before half-way, some after.
        let mut from = slide(0);
        let mut frame = vello::Scene::new();
        for n in 1..=100u8 {
            let to = slide(n);
            let amount = if n % 2 == 0 { 0.3 } else { 0.7 };
            frame = sp_render::transition(kind, sp_model::Direction::default(), &from, &to, amount, 10, 10);
            from = fade_from(from, to, amount);
        }
        assert_eq!(size(&frame), one, "each frame holds two slides, however many fades were cut short");
        assert_eq!(fade_from(1, 2, 0.49), 1);
        assert_eq!(fade_from(1, 2, 0.5), 2);
    }

    #[test]
    fn stepping_on_quickly_leaves_no_slide_stuck_on_the_screen() {
        // Five slides stepped through, each before the fade to it is a fifth done: each new
        // fade starts from the slide before, never from the first one again.
        let mut from = 1;
        let mut starts = Vec::new();
        for to in 2..=5 {
            starts.push(from);
            from = fade_start(from, to, 0.2, true);
        }
        assert_eq!(starts, [1, 2, 3, 4]);
        // The same picture drawn again part-way keeps the nearer of the two.
        assert_eq!(fade_start(1, 2, 0.2, false), 1);
        assert_eq!(fade_start(1, 2, 0.8, false), 2);
    }
}
