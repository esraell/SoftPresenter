//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sp_model::{Slide, Theme, Uuid};
use sp_ndi::NdiOutput;

mod decklink;
mod held;
mod next_picture;
pub use decklink::DeckLinkStatus;
use held::{For, Happened, Held, Part, Totals};
use next_picture::{Next, NextPicture};
use sp_render::{
    Fonts, Layers, MovingPicture, Painter, Picture, Solid, SolidPass, build_layers, read_pixels,
    see_through_transition, target_texture, vello, wgpu,
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
    /// The NDI sources the slide on screen, its layout and the stage layouts show: their
    /// receivers put each new picture where every drawing thread finds it, so the screens only
    /// look for news (`sp_render::ndi_changes`).
    pub ndi: Vec<String>,
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// The theme version the screens are in (see `versions`): a screen whose theme has it draws
    /// the slide with that version's template. Only the slide: props, messages and the
    /// copyright line over it keep their own look.
    pub version: Option<Uuid>,
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
        self.video.is_some() || !self.slide_video.is_empty() || !self.ndi.is_empty()
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
    /// A window to draw a screen in. The thread takes ownership of its surface. The screen's
    /// name is for the log.
    AddSurface {
        id: Uuid,
        name: String,
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
    /// Send a screen over NDI under this name, at this many pictures a second, or stop with
    /// `None`. Sent again with another name or rate, the source starts again with them.
    SetNdi {
        id: Uuid,
        name: Option<String>,
        rate: sp_ndi::Rate,
    },
    /// The screens whose NDI sources send no sound (switched off for them); every other NDI
    /// source sends what the app plays beside its pictures.
    NdiSilent(Vec<Uuid>),
    /// An NDI source has found that its count of receivers changed: the first receivers want a
    /// picture drawn for them, and a source the last one left is drawn for no longer. Sent by
    /// the sources' own threads.
    NdiReceivers,
    /// Play a screen out of a DeckLink card, or stop with `None`. Sent again with other
    /// settings, the output changes (see `output/decklink.rs`).
    SetDeckLink {
        id: Uuid,
        wanted: Option<sp_decklink::Wanted>,
    },
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
    /// The screens switched off that go on sending (on NDI or a DeckLink output): each is drawn
    /// as every screen of its kind is while that kind is switched off, black (clear, for a
    /// see-through one), with no sound, so a stream or switcher keeps it as a source.
    Dark(Vec<Uuid>),
    /// The operator's live preview looks at one audience screen, drawn as that screen draws it
    /// (its own theme and layers), or with `None` at what the room sees. Only the preview is
    /// drawn again: it is the same one picture, drawn for another screen.
    PreviewScreen(Option<Uuid>),
    /// How a screen's window is to draw its X now (see `close_button`), or `None` for no X. Only
    /// that window is shown again, with the picture it has: nothing is painted, and nothing
    /// else (NDI, a key, a stage screen, the preview) ever has the X.
    CloseButton {
        id: Uuid,
        drawn: Option<crate::close_button::Drawn>,
    },
    /// Said by a window's own thread: its next picture, which the screens' thread had stopped
    /// waiting for, has come (see `output/next_picture.rs`). The window is then shown the
    /// picture it is owed.
    NextPictureCame,
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
    /// Screens window shows it on one line inside a small picture of the screen. In the window's
    /// language (`sp` puts it back into English); for the operator's thread only.
    pub fn sending_in_words(name: &str, receivers: Option<u32>) -> String {
        use crate::i18n::{tf, tn};
        let (w, h) = NDI_SIZE;
        match receivers {
            None => tf(
                "screens.status.ndi_sending",
                "Sending as \"{name}\" at {w} x {h}",
                &[("name", name), ("w", &w.to_string()), ("h", &h.to_string())],
            ),
            Some(0) => {
                tf("screens.status.ndi_unwatched", "On NDI as \"{name}\"; nobody is watching", &[("name", name)])
            }
            Some(count) => tn(
                "screens.status.ndi_receivers",
                u64::from(count),
                "Sending as \"{name}\" to {n} receiver",
                "Sending as \"{name}\" to {n} receivers",
                &[("name", name)],
            ),
        }
    }
}

/// An NDI picture being copied back from the GPU: the sources it is for, its place, the buffer it is
/// copied into (each row padded as the GPU requires), and whether the copy has come back (and
/// worked).
struct NdiReadback {
    ids: Vec<Uuid>,
    /// Those of `ids` that send its key (see `NdiGroup`); the rest send it as `sent` says.
    keys: Vec<Uuid>,
    sent: sp_ndi::Sent,
    /// Which picture of its drawing it is, from nought.
    place: usize,
    buffer: wgpu::Buffer,
    padded_row: usize,
    done: Arc<Mutex<Option<bool>>>,
    /// When the copy was asked for (after the drawing it copies), for the NDI sources'
    /// per-minute line: how long it took to come back.
    asked: Instant,
}

/// How soon the idle output thread looks at the GPU again while it still holds memory the
/// painter gave back.
const GPU_LOOK_AGAIN: Duration = Duration::from_millis(20);

/// After a drawing of the screens in which the GPU was too slow to work a picture out, they
/// are drawn again this long after, up to this many times one after another. Each try may
/// itself wait four tenths of a second for the GPU (more on a slow one, which is seen to
/// finish what it was given: see `Painter::paint`), so that is up to about three seconds in
/// all on a Mac, with the picture before on the screens meanwhile.
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

/// How long a window's X that couldn't be shown (the window wasn't given its next picture in
/// time, say) waits before it is shown again, and how many times it is tried: an X left behind
/// would stay on the room's screen until the next slide.
const CLOSE_AGAIN_AFTER: Duration = Duration::from_millis(100);
const CLOSE_TRIES: u8 = 5;

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

/// Which screens a drawing draws: all of them, or only those something changes by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    /// Every screen: anything may have changed.
    All,
    /// Those whose picture moves: a playing video's new picture, or a step of a fade.
    Moving,
    /// Those, and the stage screens for the time on them, at the start of a second.
    MovingAndStage,
    /// The stage screens alone, for the time on them.
    Stage,
}

impl Which {
    /// Whether the drawing draws a screen; `moving` says its picture moves, `stage` that it is
    /// a stage screen.
    fn takes(self, moving: bool, stage: bool) -> bool {
        match self {
            Which::All => true,
            Which::Moving => moving,
            Which::MovingAndStage => moving || stage,
            Which::Stage => stage,
        }
    }
}

/// What a playing video's new picture has drawn. `steady` says nothing else moves (no fade, no
/// build, nothing scrolling); `second` that a second has begun with something on the screens
/// showing the time; `audience` that an audience screen shows it (a message or linked text with
/// the time or a timer), not only the stage screens.
fn video_drawing(steady: bool, second: bool, audience: bool) -> Which {
    match (steady, second) {
        (false, _) => Which::All,
        (true, false) => Which::Moving,
        (true, true) if audience => Which::All,
        (true, true) => Which::MovingAndStage,
    }
}

/// Whether two screens show the same picture at the same size: one screen, or two that both
/// show the show as it is. `own_way` says a screen shows it its own way (a stage screen, one
/// being identified, one with a look or a theme of its own).
fn same_picture(one: Uuid, other: Uuid, own_way: impl Fn(Uuid) -> bool) -> bool {
    one == other || (!own_way(one) && !own_way(other))
}

/// Whether a step of a fade owes the NDI sources and DeckLink outputs a picture, so that they
/// get the fade at their own pace and not at the windows'. While a video gives pictures, each
/// new picture of it is one for them, as outside a fade (`moved`; `since_video` is how long ago
/// the last came). While nothing else moves, one is owed each time NDI's own picture time has
/// passed (`due`).
fn fade_owes_ndi(moved: bool, since_video: Option<Duration>, due: bool) -> bool {
    let video_moves = since_video.is_some_and(|since| since < 2 * sp_ndi::FRAME);
    moved || (!video_moves && due)
}

/// When the next picture of a fade is due to the NDI sources after one drawn `now`, the one
/// before having been due at `due`: one picture's time (`frame`, at the rate the fastest of the
/// sources sends) after that, so the pace holds whatever the windows' steps are; from now when
/// that has fallen a picture or more behind.
fn next_ndi_due(due: Option<Instant>, now: Instant, frame: Duration) -> Instant {
    match due {
        Some(due) if now < due + frame => due + frame,
        _ => now + frame,
    }
}

/// One NDI source as a drawing finds it: whether a picture is wanted for it, and whether it
/// shows the show its own way (a stage screen, one being identified, one with a look or a
/// theme of its own, one whose key another source sends) and so needs a picture of its own.
#[derive(Clone, Copy, Debug)]
struct NdiSeen {
    id: Uuid,
    wanted: bool,
    alone: bool,
    /// The NDI source whose key this one sends, made from that source's own picture.
    key_of: Option<Uuid>,
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

/// True when every NDI source has the last picture drawn for it and nothing is owed (`well`),
/// so that a drawing may leave out the sources a video's new picture does not change.
fn ndi_settled(sources: &[NdiSeen], drawn_for: &[(Uuid, u32)], well: bool) -> bool {
    well && !ndi_count_needs_drawing(sources, drawn_for, false)
}

/// The mix is made as NDI sources send it.
const _: () = assert!(sp_ndi::SOUND_RATE == sp_video::sound::RATE);

/// An NDI source being handed the app's sound, block by block, on the mix's thread.
struct NdiSound {
    to: sp_ndi::Sound,
    name: String,
    /// How many blocks in a row have had something to hear, or `None` while it is silent,
    /// and how many silent ones have followed: for the log.
    heard: std::sync::Mutex<(Option<u32>, u32)>,
}

impl sp_video::sound::Listener for NdiSound {
    fn wanted(&self) -> bool {
        self.to.wanted()
    }

    fn play(&self, planar: &[f32], heard: std::time::SystemTime) {
        self.to.play(planar, heard);
        // The log says when something is heard after a silence, and when a second's silence
        // has followed it (a block is 20 ms), with how long the sound was.
        let sounding = planar.iter().any(|sample| *sample != 0.0);
        let Ok(mut heard) = self.heard.lock() else { return };
        match (&mut *heard, sounding) {
            ((None, _), true) => {
                *heard = (Some(1), 0);
                crate::applog::event("ndi", &format!("{}: sound is going out", self.name));
            }
            ((Some(blocks), quiet), true) => (*blocks, *quiet) = (*blocks + 1 + *quiet, 0),
            ((Some(blocks), quiet), false) => {
                *quiet += 1;
                if *quiet >= 50 {
                    let seconds = f64::from(*blocks) / 50.0;
                    crate::applog::event(
                        "ndi",
                        &format!("{}: sound stopped after {seconds:.1} s (silence goes out)", self.name),
                    );
                    *heard = (None, 0);
                }
            }
            ((None, _), false) => {}
        }
    }
}

/// Whether a picture is to be drawn for an NDI source. Not for one that could not start (NDI
/// not installed, say), and not for one nobody is watching: it would be drawn, read back from
/// the GPU and thrown away. A count that isn't known (the source is still starting, or its
/// NDI can't count) is drawn for, as if someone were watching.
fn ndi_wanted(state: &sp_ndi::State, receivers: Option<u32>) -> bool {
    !matches!(state, sp_ndi::State::Failed(_)) && receivers != Some(0)
}

/// One picture to draw for NDI: the screen it is drawn for, the sources that send it as it is,
/// and the sources that send its key. A key is made from the very pixels its fill sends, read
/// back from the GPU once and handed to both together, so the two never send different moments.
#[derive(Debug, PartialEq)]
struct NdiGroup {
    of: Uuid,
    fills: Vec<Uuid>,
    keys: Vec<Uuid>,
}

/// The pictures to draw for the NDI sources, each with the sources it goes to: one for each
/// source that needs its own (with any sources that send its key), then one shared by all the
/// rest. Sources no picture is wanted for are in none, and a picture nobody wants is not there
/// at all; a picture whose key is wanted is drawn though nobody watches the picture itself.
fn ndi_groups(sources: &[NdiSeen]) -> Vec<NdiGroup> {
    let keys_of = |fill: Uuid| -> Vec<Uuid> {
        sources.iter().filter(|source| source.wanted && source.key_of == Some(fill)).map(|source| source.id).collect()
    };
    let mut groups: Vec<NdiGroup> = sources
        .iter()
        .filter(|source| source.alone && source.key_of.is_none())
        .map(|source| NdiGroup {
            of: source.id,
            fills: if source.wanted { vec![source.id] } else { Vec::new() },
            keys: keys_of(source.id),
        })
        .filter(|group| !group.fills.is_empty() || !group.keys.is_empty())
        .collect();
    let shared: Vec<Uuid> =
        sources.iter().filter(|source| source.wanted && !source.alone).map(|source| source.id).collect();
    if let Some(first) = shared.first() {
        groups.push(NdiGroup { of: *first, keys: Vec::new(), fills: shared });
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

/// The usual size of the live preview (larger where the operator's window draws at more pixels
/// to the point: `interface_size::picture_scale`; the size is the targets' own), and how often a
/// playing video's pictures are drawn in it at most: about thirty a second, so a 24 or 30 fps
/// video shows every picture and a 60 fps one every other (every change of slide or media is
/// drawn at once, and fades as the screens do).
/// Measured in quiet runs (release build, a 30 fps 720p video, two window screens; ten-second
/// averages, three runs each): about 23% CPU for the whole app at this cap, 21% at a 50 ms cap,
/// which shows only every other picture of a 30 fps video.
pub const LIVE_PREVIEW_SIZE: (u32, u32) = (640, 360);
const LIVE_PREVIEW_EVERY: Duration = Duration::from_millis(30);

/// The size of the live preview's textures (the two are made alike).
fn preview_size_of(targets: &[wgpu::Texture]) -> Option<(u32, u32)> {
    targets.first().map(|texture| (texture.width(), texture.height()))
}

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
    ndi_status: NdiStatuses,
    decklink_status: decklink::Statuses,
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
        let decklink_status: decklink::Statuses = Arc::default();
        let status = (ndi_status.clone(), decklink_status.clone());
        let live_preview: Arc<LivePreview> = Arc::default();
        let shared = live_preview.clone();
        // A picture read for the screens meanwhile has them drawn again once it is there.
        let wake = tx.clone();
        // The NDI sources tell the thread when their receivers come and go, on the same channel.
        let from_ndi = tx.clone();
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || {
                // The screens never wait for the disk to take a log line.
                crate::applog::never_wait_on_this_thread();
                sp_render::draw_kept_only(move || {
                    let _ = wake.send(OutputMsg::Redraw);
                });
                run(device, queue, theme, (from_ndi, rx), status, shared, waker)
            })
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread), ndi_status, decklink_status, live_preview })
    }

    /// Which live preview texture holds the newest picture, and how many have been drawn.
    pub fn live_preview(&self) -> (usize, u64) {
        self.live_preview.newest()
    }

    /// The same, shared with another thread (Remote Control reads the newest picture back on
    /// its own), costing the output thread nothing.
    pub fn live_preview_shared(&self) -> Arc<LivePreview> {
        self.live_preview.clone()
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

/// What the Announcements layer shows: a slide and the theme it is drawn with, and the video
/// playing in it, if one is: the name its box shows it under, and where its pictures come from.
#[derive(Clone)]
pub struct Announcement {
    pub slide: Arc<Slide>,
    pub theme: Arc<Theme>,
    pub video: Option<(String, sp_video::Frames)>,
    /// False once a slide has gone on screen: the screens that show slides leave the layer out
    /// (a lobby screen showing announcements and no slides keeps it) until it is started again.
    pub on_slide_screens: bool,
}

impl Announcement {
    /// The name the video's pictures are shown under, if one plays.
    fn video_name(&self) -> Option<&str> {
        self.video.as_ref().map(|(name, _)| name.as_str())
    }

    /// The same slide drawn with the same theme: a change of where the video's pictures come
    /// from (FFmpeg taking it over) is not a new announcement.
    fn same_picture(&self, other: &Announcement) -> bool {
        Arc::ptr_eq(&self.slide, &other.slide) && Arc::ptr_eq(&self.theme, &other.theme)
    }
}

/// A window a screen is drawn in.
/// A fade under way in one window: the two whole pictures it goes between, and, when the fade
/// is cached, a texture of each (see `Output::cache_sides`). A cached fade draws each slide once
/// and then only blends the two textures, with the same `transition` as one that is not cached,
/// which draws both slides again at every step.
struct Fade {
    from: vello::Scene,
    to: vello::Scene,
    cache: Option<(sp_render::CachedSide, sp_render::CachedSide)>,
}

/// The two pictures a fade goes between, for `Output::cache_sides`: the scenes, and the texture
/// the picture it starts from is painted in already, when there is one (a window's own).
struct Sides<'a> {
    from: &'a vello::Scene,
    painted: Option<&'a wgpu::Texture>,
    to: &'a vello::Scene,
}

/// The slide above a playing video, kept in a texture (see `Output::steady_scene`).
struct Steady {
    id: Uuid,
    size: (u32, u32),
    /// The picture of the video that the part below the slide was made with.
    video: Picture,
    side: sp_render::CachedSide,
}

/// What a window is shown: a scene painted by the renderer, or the plain blend of the two sides of
/// a cached fade (`sp_render::FadeBlend`).
enum Draw<'a> {
    Scene(&'a vello::Scene),
    Blend(&'a sp_render::CachedSide, &'a sp_render::CachedSide, sp_render::Weights),
}

impl Fade {
    /// The two textures and weights to blend, for a cached fade whose kind is a plain sum of its
    /// two pictures (`sp_render::blend_weights`); `None` when the renderer must draw it.
    fn blend(
        &self,
        look: sp_model::Transition,
        amount: f32,
    ) -> Option<(&sp_render::CachedSide, &sp_render::CachedSide, sp_render::Weights)> {
        let (from, to) = self.cache.as_ref()?;
        Some((from, to, sp_render::blend_weights(look.kind, amount)?))
    }

    /// The picture `amount` of the way through the fade, as a scene to paint.
    fn mixed(&self, look: sp_model::Transition, amount: f32, w: u32, h: u32) -> vello::Scene {
        match &self.cache {
            Some((from, to)) => {
                sp_render::transition(look.kind, look.direction, from.scene(), to.scene(), amount, w, h)
            }
            None => sp_render::transition(look.kind, look.direction, &self.from, &self.to, amount, w, h),
        }
    }
}

/// A fade under way in an NDI picture (one of `ndi_groups`), at NDI's size: the sources and the
/// DeckLink outputs it goes to are sent the fade the windows show.
struct NdiFade {
    /// The screen the picture is drawn for.
    of: Uuid,
    fade: Fade,
    /// `Output::video_moves` when the picture faded to was built: a new picture of a video since
    /// then builds it again, when it is next drawn.
    built: u64,
}

/// What a fade drew, for the log line it ends with: when it began, its steps in the windows,
/// whether any window's were from textures, and how many drawings went to NDI and DeckLink.
#[derive(Clone, Copy)]
struct FadeDrawn {
    since: Instant,
    steps: u32,
    cached: bool,
    sent: u32,
}

struct Target {
    id: Uuid,
    /// The screen's name when its window opened, for the log.
    name: String,
    /// The window's surface, and its next picture to draw into, asked for on a thread of its
    /// own (see `output/next_picture.rs`).
    next: NextPicture,
    config: wgpu::SurfaceConfiguration,
    /// What copies the painted picture into the window's, shared by the windows of its format.
    blitter: Arc<wgpu::util::TextureBlitter>,
    /// What a see-through screen's picture, or a key, is shown through in place of the
    /// blitter: made the first time one is shown, and again if it must show another way.
    pass: Option<SolidPass>,
    texture: wgpu::Texture,
    /// The picture now in the window in full, kept so the next one can fade in over it.
    shown: Option<vello::Scene>,
    /// A fade under way: the picture it started from and the one it ends at.
    fade: Option<Fade>,
    /// False when setting the window's surface up failed (the GPU too busy, say); it is tried
    /// again before each frame, and the window is skipped until it works.
    configured: bool,
    /// A picture for this window that could not be drawn: said in the log once while it lasts,
    /// not for every picture.
    trouble: crate::applog::Lasting,
    /// Whether the window shows what it was last asked to.
    state: Shown,
    /// Since when the window has been passed over, its next picture not having come in time
    /// (see `output/next_picture.rs`), and whether the log has been told: it is, once the
    /// picture is as late as a display keeping up never is.
    passed_over: Option<Instant>,
    passed_over_said: bool,
    /// The X the window is to show over its picture, the X it last showed, and what lays it
    /// there (see `close_button`), shared by the windows of its format.
    close: Option<crate::close_button::Drawn>,
    close_shown: Option<crate::close_button::Drawn>,
    close_mark: Arc<sp_render::CloseMark>,
    /// How many times the X as it is now couldn't be shown (see `CLOSE_TRIES`).
    close_tries: u8,
}

impl Target {
    /// True when the window shows the picture it was last given: painted, and its window set up
    /// and not passed over. A drawing that leaves it out then leaves it right.
    fn up_to_date(&self) -> bool {
        self.configured && self.passed_over.is_none() && self.state == Shown::default() && self.shown.is_some()
    }
}

/// A copy of the picture a video lends, to draw: its pixels as they are, in whichever order
/// their bytes come (the GPU reads either), so copying is all the work there is.
fn copied(lent: &sp_video::Lent<'_>) -> Option<Picture> {
    Picture::from_rows(lent.width, lent.height, lent.bytes, lent.bytes_per_row, pixel_order(lent.order), lent.opaque)
}

/// The picture a playing video lends, sent into its texture on the GPU (made, or made again for
/// another size), to draw as the media: four-byte pixels as they are, a copy where that can't be
/// done, or a video's two planes, which the GPU turns into colours.
fn into_texture(
    lending: sp_video::Lending<'_>,
    painter: &mut Painter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    video_texture: &mut Option<MovingPicture>,
) -> Option<Picture> {
    let lent = match lending {
        sp_video::Lending::Rows(lent) => lent,
        sp_video::Lending::Planes(planes) => return planes_into_texture(planes, painter, device, queue, video_texture),
    };
    // The GPU draws each pixel's fourth byte as its alpha. A picture shown opaque whatever those
    // bytes say goes there as it is only when every one is full (as the system's player decodes
    // all but a video with clear parts); a picture where one isn't is drawn from a copy made
    // opaque.
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
}

/// A video's two planes, sent to the GPU, which turns them into colours in the moving picture's
/// texture (made, or made again for another size). `None` when it could not be done (planes
/// too large for a texture, which FFmpeg is never asked for).
fn planes_into_texture(
    planes: &sp_video::Planes<'_>,
    painter: &mut Painter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    video_texture: &mut Option<MovingPicture>,
) -> Option<Picture> {
    let (width, height) = (planes.width, planes.height);
    let order = sp_render::PixelOrder::Rgba;
    if !video_texture.as_ref().is_some_and(|kept| kept.fits(width, height, order)) {
        *video_texture = painter.moving_picture(device, width, height, order);
    }
    let sent = sp_render::YuvPlanes {
        width,
        height,
        luma: planes.luma,
        luma_row: width as usize,
        chroma: planes.chroma,
        chroma_row: planes.chroma_row(),
        colours: yuv_colours(planes.colours),
    };
    video_texture.as_ref().filter(|kept| painter.show_planes(device, queue, kept, &sent)).map(MovingPicture::picture)
}

/// The colours a video's planes stand for, as the GPU's pass takes them.
fn yuv_colours(colours: sp_video::Colours) -> sp_render::YuvColours {
    let matrix = match colours.matrix {
        sp_video::Matrix::Bt601 => sp_render::YuvMatrix::Bt601,
        sp_video::Matrix::Bt709 => sp_render::YuvMatrix::Bt709,
        sp_video::Matrix::Fcc => sp_render::YuvMatrix::Fcc,
        sp_video::Matrix::Smpte240 => sp_render::YuvMatrix::Smpte240,
        sp_video::Matrix::Bt2020 => sp_render::YuvMatrix::Bt2020,
    };
    sp_render::YuvColours { matrix, full_range: colours.full_range }
}

/// The picture a playing video's decoder made, as a GPU texture over the decoder's own memory,
/// drawn as the media: the GPU turns it into the moving picture's texture (made, or made again
/// for another size) without the CPU touching a pixel. `None` when the texture could not be
/// shown.
#[cfg(target_os = "macos")]
fn decoded_into_texture(
    texture: wgpu::Texture,
    painter: &mut Painter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    video_texture: &mut Option<MovingPicture>,
) -> Option<Picture> {
    let (width, height) = (texture.width(), texture.height());
    let order = sp_render::PixelOrder::Bgra;
    if !video_texture.as_ref().is_some_and(|kept| kept.fits(width, height, order)) {
        *video_texture = painter.moving_picture(device, width, height, order);
    }
    video_texture.as_ref().filter(|kept| painter.show_texture(device, queue, kept, texture)).map(MovingPicture::picture)
}

/// A copy of a playing video's newest picture, if a new one has come, with the time taken to
/// fetch it and to copy it noted apart.
fn copied_from(frames: &sp_video::Frames, held: &mut Held) -> Option<Picture> {
    let fetching = Instant::now();
    let mut copying = Duration::ZERO;
    // The videos copied this way are opened to lend rows (`sp_video::Lend::Rows`).
    let picture = frames.next_with(|lending| {
        let began = Instant::now();
        let picture = lending.rows().and_then(copied);
        copying = began.elapsed();
        picture
    });
    held.add(Part::VideoFetch, For::Nothing, fetching.elapsed().saturating_sub(copying));
    if picture.is_some() {
        held.add(Part::VideoCopy, For::Nothing, copying);
    }
    picture.flatten()
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

/// A screen's window's next picture to draw into, as [`next_texture`] gives it.
fn next_screen_texture(surface: &wgpu::Surface<'static>) -> Option<wgpu::CurrentSurfaceTexture> {
    hold_next_texture_once();
    next_texture(surface)
}

/// Testing aid: SOFTPRESENTER_HOLD_WINDOW_PICTURE=MS@SECONDS has the first picture a screen's
/// window asks for that many seconds after the start come MS milliseconds late, once, as when
/// the window system is slow to give a window back a picture it has shown (on a Mac it can
/// take a second). It works in a quiet run, whose windows are covered and otherwise answer at
/// once. The operator's window is never held.
fn hold_next_texture_once() {
    static ASKED: std::sync::OnceLock<Option<(Duration, Instant)>> = std::sync::OnceLock::new();
    static HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let asked = ASKED.get_or_init(|| {
        let asked = std::env::var("SOFTPRESENTER_HOLD_WINDOW_PICTURE").ok()?;
        held_picture(&asked).map(|(hold, after)| (hold, Instant::now() + after))
    });
    if let Some((hold, at)) = asked
        && Instant::now() >= *at
        && !HELD.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        std::thread::sleep(*hold);
    }
}

/// What SOFTPRESENTER_HOLD_WINDOW_PICTURE asks for: how long to hold the picture, and from how
/// long after the start. `None` for anything but milliseconds, and seconds from nought up.
fn held_picture(asked: &str) -> Option<(Duration, Duration)> {
    let (hold, after) = asked.split_once('@').unwrap_or((asked, "0"));
    let hold = Duration::from_millis(hold.trim().parse().ok()?);
    let after = Duration::try_from_secs_f32(after.trim().parse().ok()?).ok()?;
    Some((hold, after))
}

/// Testing aid: SOFTPRESENTER_SLOW_PRESENT=MS or MS@SCREEN has every picture a screen's window
/// asks for come MS milliseconds late (only the window of the screen of that name, with one;
/// every screen's without), as when the window system is slow to take a window's pictures and
/// give them back, time after time: a display behind a splitter or on a slow connection, a
/// window system busy with something else. The window is then shown fewer pictures; nothing
/// else should be held up by it. It works in a quiet run, as SOFTPRESENTER_HOLD_WINDOW_PICTURE
/// (which holds one picture, once) does. How late the pictures of the window of the screen
/// `name` come, if they do.
fn slow_present(asked: &str, name: &str) -> Option<Duration> {
    let (slow, screen) = match asked.split_once('@') {
        Some((slow, screen)) => (slow, Some(screen.trim())),
        None => (asked, None),
    };
    let slow = Duration::from_millis(slow.trim().parse().ok()?);
    let this_one = screen.is_none_or(|screen| screen.eq_ignore_ascii_case(name.trim()));
    (this_one && !slow.is_zero()).then_some(slow)
}

/// Sets a window's surface up, saying whether that worked. A failure is caught here, not left to
/// stop the thread: a screen that can't be drawn this moment is tried again next frame.
///
/// The operator's window and the screens share one device, and setting a surface up waits for
/// the GPU to come idle: work the other thread hands in at that moment makes it fail ("Failed to
/// wait for GPU to come idle"), mostly as the app starts. That is only a race, so it is tried
/// again a few times, a moment apart, and if it still loses, the next frame tries again; it is
/// written as an event, not a problem.
pub(crate) fn configure(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'static>,
    config: &wgpu::SurfaceConfiguration,
) -> bool {
    const TRIES: u32 = 4;
    for attempt in 1..=TRIES {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        surface.configure(device, config);
        let Some(error) = pollster::block_on(scope.pop()) else { return true };
        let raced = error.to_string().contains("come idle");
        if raced && attempt < TRIES {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        if raced {
            crate::applog::event(
                "output",
                "A window's surface met other work on the GPU as it was set up; trying again",
            );
        } else {
            crate::applog::problem("output", &format!("A screen's window could not be set up yet: {error}"));
        }
        return false;
    }
    false
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
    /// Whether a fade draws each slide once into a texture and then only blends (see
    /// `cache_sides`); false draws both slides again at every step, as it always did.
    fade_cache: bool,
    /// The slides kept in textures over a playing video, for the screens that show one.
    steady: Vec<Steady>,
    /// What the fade under way has drawn so far, for the log line each fade ends with.
    fade_drawn: Option<FadeDrawn>,
    /// Blends the textures of a cached fade (see `Draw`).
    blend: sp_render::FadeBlend,
    /// The NDI sources that are switched on, and the texture their picture is drawn into.
    ndi: Vec<(Uuid, NdiOutput)>,
    /// The screens whose NDI sources send no sound, and the sources being handed the app's
    /// sound now: each for as long as its entry is kept.
    ndi_silent: Vec<Uuid>,
    ndi_sound: Vec<(Uuid, sp_video::sound::Listening)>,
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
    /// True while `ndi_dirty` is a drawing put off (not one asked for again after a picture was
    /// lost): another put off before it is drawn passes it over, which the sources count.
    ndi_put_off: bool,
    /// The sources the NDI pictures were last drawn for, each with how many times receivers
    /// had come to it by then.
    ndi_drawn_for: Vec<(Uuid, u32)>,
    /// The whole picture last drawn for each NDI picture, by the screen it is drawn for, kept so
    /// that the next can fade in over it; and the fades under way in them, which follow the
    /// windows' clock (`fading`).
    ndi_shown: Vec<(Uuid, vello::Scene)>,
    ndi_fades: Vec<NdiFade>,
    /// In a fade: whether a step owes the sources a picture it could not draw yet, and when the
    /// next is due while nothing but the fade moves (see `fade_owes_ndi`).
    ndi_fade_owed: bool,
    ndi_fade_due: Option<Instant>,
    /// After a fade: when the sources are owed the whole new picture, if no drawing of them all
    /// has come by then (the fade's last picture for them may be a step short of its end).
    ndi_after_fade: Option<Instant>,
    /// How many times a video (or a box's video or NDI picture) has given a new picture.
    video_moves: u64,
    /// NDI pictures lost on their way back from the GPU: whether to draw again, and what the
    /// operator is told. Said in the log once while it lasts.
    ndi_lost: Lost,
    ndi_lost_trouble: crate::applog::Lasting,
    /// Testing aid: SOFTPRESENTER_NDI_LOSE (see `PretendLost`).
    pretend_lost: Option<PretendLost>,
    ndi_status: NdiStatuses,
    /// The screens played out of DeckLink cards, which are given the NDI pictures as if always
    /// watched (see `output/decklink.rs`), and what each is doing, for the window.
    decklink: Vec<(Uuid, sp_decklink::DeckLinkOutput)>,
    decklink_status: decklink::Statuses,
    /// While screens are being identified: the name each one shows instead of the show.
    /// While screens are identified: each screen's name (as a slide), its colour, and when it
    /// started, for the wash of colour that breathes over it.
    identify: Vec<(Uuid, Arc<Slide>, sp_model::Rgba)>,
    identify_since: Instant,
    identify_theme: Arc<Theme>,
    /// The screens switched off that go on sending: drawn black (see `OutputMsg::Dark`).
    dark: Vec<Uuid>,
    /// The audience screens with a look of their own.
    looks: Vec<(Uuid, crate::screens::Look)>,
    themes: Vec<(Uuid, Arc<Theme>)>,
    /// Themes without their background colour and picture, for see-through screens, each with
    /// the theme it was made from (see `plain_theme`).
    plain_themes: Vec<(Arc<Theme>, Arc<Theme>)>,
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
    /// Makes GPU textures of the memory the system's player decodes into, so those pictures
    /// aren't copied: made when the first video plays; `Some(None)` where that can't be done.
    #[cfg(target_os = "macos")]
    video_import: Option<Option<sp_video::GpuImport>>,
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
    /// The second (since 1970) in which the screens were last drawn whole for the time on them
    /// (see `draw_video`).
    ticked_second: u64,
    /// Where the pass under way spends its time, written to the log when it is held up (see
    /// `output/held.rs`).
    held: Held,
    /// The live preview: what the window is told, the two textures drawn into, and when the
    /// last was drawn.
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
    preview_targets: Vec<wgpu::Texture>,
    previewed_at: Instant,
    /// When a video last gave a new picture, or a message last came (so a video just shown is
    /// looked at often from the start).
    video_moved_at: Instant,
    /// When a video (or a box's video or NDI picture) last gave a new picture.
    video_came_at: Option<Instant>,
    /// `sp_render::ndi_changes` as last drawn, and how visible each NDI source then was.
    ndi_seen: u64,
    ndi_visible_seen: Vec<f32>,
    /// The live preview's whole picture, and a fade under way in it: from, to.
    preview_shown: Option<vello::Scene>,
    preview_fade: Option<Fade>,
    /// The audience screen the live preview is drawn as, when the operator looks at one.
    preview_screen: Option<Uuid>,
    /// True in a drawing of every screen until the windows other than the stage's are drawn:
    /// meanwhile a stage screen's picture of one of them is made from the frame, not taken from
    /// the window, which still holds the last drawing's.
    windows_behind: bool,
    /// What draws the windows' X, one for each format of window (see `add_surface`), and when
    /// an X that couldn't be shown is tried again.
    close_marks: Vec<Arc<sp_render::CloseMark>>,
    close_again_at: Option<Instant>,
    /// What copies a painted picture into a window's, one for each format of window, made with
    /// the first window of that format (see `add_surface`).
    blitters: Vec<(wgpu::TextureFormat, Arc<wgpu::util::TextureBlitter>)>,
}

fn run(
    device: wgpu::Device,
    queue: wgpu::Queue,
    theme: Arc<Theme>,
    (from_ndi, rx): (Sender<OutputMsg>, Receiver<OutputMsg>),
    (ndi_status, decklink_status): (NdiStatuses, decklink::Statuses),
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
) {
    let mut painter = match Painter::new(&device) {
        Ok(painter) => painter,
        Err(e) => return eprintln!("The output cannot draw on this computer: {e}"),
    };
    // Videos the system plays reach the GPU through a small compute pass, and the planes FFmpeg
    // gives are turned into colours by another: compiled now, not when the first video's first
    // picture comes. So is all else a video's first picture would be the first to need, up to
    // the size FFmpeg gives the screens' videos (1080p).
    #[cfg(target_os = "macos")]
    painter.prepare_texture_copy(&device);
    let preparing = Instant::now();
    match painter.prepare_video(&device, &queue, 1920, 1080) {
        Ok(()) => crate::applog::event(
            "output",
            &format!("The screens' painter was readied for a 1920 x 1080 video in {:?}", preparing.elapsed()),
        ),
        Err(e) => crate::applog::event("output", &format!("The screens' painter couldn't be readied for video: {e}")),
    }
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
            ndi: Vec::new(),
            slide: None,
            theme,
            version: None,
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
        ndi_silent: Vec::new(),
        ndi_sound: Vec::new(),
        from_ndi,
        ndi_target: None,
        ndi_pending: Vec::new(),
        ndi_buffers: Vec::new(),
        ndi_dirty: None,
        ndi_put_off: false,
        ndi_drawn_for: Vec::new(),
        ndi_shown: Vec::new(),
        ndi_fades: Vec::new(),
        ndi_fade_owed: false,
        ndi_fade_due: None,
        ndi_after_fade: None,
        video_moves: 0,
        ndi_lost: Lost::default(),
        ndi_lost_trouble: crate::applog::Lasting::default(),
        // Testing aid: SOFTPRESENTER_NDI_LOSE (see `PretendLost`).
        pretend_lost: std::env::var("SOFTPRESENTER_NDI_LOSE").ok().as_deref().and_then(PretendLost::parse),
        ndi_status,
        decklink: Vec::new(),
        decklink_status,
        identify: Vec::new(),
        identify_since: Instant::now(),
        identify_theme: Arc::new(Theme::builtin()),
        dark: Vec::new(),
        stage: Vec::new(),
        stage_on: true,
        looks: Vec::new(),
        themes: Vec::new(),
        plain_themes: Vec::new(),
        slide_on: Instant::now(),
        step_at: Instant::now(),
        overlay: Overlay::default(),
        video_picture: None,
        video_texture: None,
        #[cfg(target_os = "macos")]
        video_import: None,
        announcement: None,
        look: sp_model::Transition::of(sp_model::TransitionKind::Cut),
        ndi_trouble: crate::applog::Lasting::default(),
        ndi_state: Shown::default(),
        preview_state: Shown::default(),
        round: Round::default(),
        pretend_slow: std::env::var("SOFTPRESENTER_GPU_TOO_SLOW").ok().as_deref().and_then(PretendSlow::parse),
        fade_cache: fade_cache_wanted(),
        steady: Vec::new(),
        fade_drawn: None,
        blend: sp_render::FadeBlend::new(&device),
        started: Instant::now(),
        timing: crate::applog::Timing::new("screen drawings"),
        video_pictures: 0,
        ticked_second: 0,
        held: Held::new(Instant::now()),
        live_preview,
        waker,
        preview_targets: Vec::new(),
        previewed_at: Instant::now(),
        video_moved_at: Instant::now(),
        video_came_at: None,
        ndi_seen: 0,
        ndi_visible_seen: Vec::new(),
        preview_shown: None,
        preview_fade: None,
        preview_screen: None,
        windows_behind: false,
        close_marks: Vec::new(),
        close_again_at: None,
        blitters: Vec::new(),
        device,
        queue,
    };

    output.held.begin(Instant::now(), output.totals());
    loop {
        // Each time round is a pass of its own, as is the work after each wait below: one held
        // up is written to the log with where its time went.
        output.end_pass();
        output.held.begin(Instant::now(), output.totals());
        // The GPU library lets go of what a finished drawing used (a bind group and a texture
        // view for every picture the renderer paints) only when the device is asked to look,
        // and nothing else asks: without this, each frame drawn left about a kilobyte behind,
        // for good (a few thousand objects for every half minute of video). It never waits.
        let polled = Instant::now();
        let _ = output.device.poll(wgpu::PollType::Poll);
        output.held.since(Part::Poll, For::Nothing, polled);
        // A window's X that couldn't be shown a moment ago is tried again.
        if output.close_again_at.is_some_and(|at| at <= Instant::now()) {
            output.show_windows_again();
        }
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
            let look = Duration::from_millis(2);
            match output.idle(Some(look), || rx.recv_timeout(look)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // The picture that was waiting may be painted now: a drawing of its own,
                    // so that one the GPU is too slow with is drawn again, and the operator is
                    // told of one that cannot be drawn, or that all is well again.
                    output.round.begin_part();
                    output.finish_ndi();
                    output.end_round();
                    if output.plays_video() && output.take_video_picture() {
                        output.draw_video();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if capture_wanted() {
            // Testing aid: while a picture of the output is still to be taken, keep drawing, so
            // it is taken even where no window asks to be drawn (a quiet run's are hidden).
            let look = Duration::from_millis(200);
            match output.idle(Some(look), || rx.recv_timeout(look)) {
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
            let wait = if output.plays_video() {
                output.video_poll()
            } else {
                Duration::from_secs(1).saturating_sub(into_second) + Duration::from_millis(5)
            };
            // No longer than until a picture the GPU was too slow with is due again.
            let wait = output.draw_again_in().map_or(wait, |due| wait.min(due));
            match output.idle(Some(wait), || rx.recv_timeout(wait)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Woken for that picture: it redraws every screen, the time included.
                    if output.draw_again_if_due() {
                        continue;
                    }
                    // A new picture of the video redraws what shows it, and the time on the
                    // screens that keep it, once in each second.
                    if output.take_video_picture() {
                        output.draw_video();
                        continue;
                    }
                    // Otherwise the time changes only as a second begins (a video, or a web page
                    // or video in a box, wakes this far more often than that).
                    let new_second =
                        !output.plays_video() || into_second + output.video_poll() >= Duration::from_secs(1);
                    // While a video gives pictures, the one that comes as a second begins draws
                    // the time with it (`draw_video`).
                    let video_draws_it =
                        output.video_came_at.is_some_and(|came| came.elapsed() < Duration::from_secs(1));
                    if !new_second || video_draws_it {
                        continue;
                    }
                    if output.audience_ticks() {
                        output.draw(false);
                    } else {
                        output.draw_stage();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if output.plays_video() {
            let wait = output.video_poll();
            match output.idle(Some(wait), || rx.recv_timeout(wait)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if output.take_video_picture() {
                        output.draw_video();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            // Nothing moves until the next message: memory a heavy picture took is given back.
            // That never waits here; while the GPU still holds some of it, or a picture it was
            // too slow with is to be drawn again, the thread comes back in a moment to look.
            let resting = Instant::now();
            let rested = output.painter.rest(&output.device, &output.queue);
            output.held.since(Part::Rest, For::Nothing, resting);
            match output.look_again_in(rested) {
                None => match output.idle(None, || rx.recv()) {
                    Ok(msg) => Some(msg),
                    Err(_) => return,
                },
                // The top of the loop looks at the GPU, and draws a picture again if it is due.
                Some(wait) => match output.idle(Some(wait), || rx.recv_timeout(wait)) {
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
                // With no window to wait for (only NDI or DeckLink fade), a pause paces the
                // steps, as a covered window's does.
                if output.targets.is_empty() {
                    let pausing = Instant::now();
                    std::thread::sleep(next_picture::STILL_WAIT);
                    output.held.since(Part::Pause, For::Nothing, pausing);
                }
            } else {
                output.take_video_picture();
                output.draw(false);
                // With no window to wait for (only NDI), keep to about the screens' pace.
                if output.targets.is_empty() {
                    let pausing = Instant::now();
                    std::thread::sleep(Duration::from_millis(16));
                    output.held.since(Part::Pause, For::Nothing, pausing);
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
        // Whether the live preview now looks at another screen, whether that or the NDI receivers
        // were asked about at all, and whether a window's X is to look different.
        let (mut preview_again, mut ndi_or_preview, mut close_changed) = (false, false, false);
        // Whether a window passed over has been given its next picture: it is shown what it
        // is owed, and nothing is drawn for that.
        let mut window_came = false;
        while let Some(msg) = next {
            // The live preview's textures made again at another size (the operator's window
            // draws at another scale) ask for the preview alone, as another screen to look at
            // does.
            let preview_resized = match &msg {
                OutputMsg::PreviewTargets(targets) => {
                    !output.preview_targets.is_empty()
                        && preview_size_of(targets) != preview_size_of(&output.preview_targets)
                }
                _ => false,
            };
            nothing_else &= preview_resized
                || matches!(
                    msg,
                    OutputMsg::NdiReceivers
                        | OutputMsg::PreviewScreen(_)
                        | OutputMsg::CloseButton { .. }
                        | OutputMsg::NextPictureCame
                );
            ndi_or_preview |= matches!(msg, OutputMsg::NdiReceivers | OutputMsg::PreviewScreen(_));
            match msg {
                OutputMsg::Show(frame) => {
                    let now = Instant::now();
                    // The next slide's pictures are read meanwhile, so it goes on at once.
                    if let Some(next) = frame.next.as_deref() {
                        let asking = Instant::now();
                        sp_render::read_ahead(next);
                        output.held.since(Part::Locks, For::Nothing, asking);
                    }
                    // Another version over the same slide gives way to it as another slide does.
                    let same_slide = output.frame.slide.as_deref() == frame.slide.as_deref()
                        && output.frame.enabled == frame.enabled
                        && output.frame.version == frame.version;
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
                            let forgetting = Instant::now();
                            sp_render::forget_picture(file);
                            output.held.since(Part::Locks, For::Nothing, forgetting);
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
                OutputMsg::AddSurface { id, name, surface, format, w, h } => {
                    let setting_up = Instant::now();
                    output.add_surface(id, name, surface, format, w, h);
                    output.held.since(Part::Setup, For::Window(id), setting_up);
                }
                OutputMsg::PreviewTargets(targets) => {
                    // What the preview showed, and any fade under way in it, were the old size's.
                    if preview_resized {
                        output.preview_fade = None;
                        output.preview_shown = None;
                        preview_again = true;
                    }
                    output.preview_targets = targets;
                }
                OutputMsg::Resized(id, w, h) => {
                    let setting_up = Instant::now();
                    output.resize(id, w, h);
                    output.held.since(Part::Setup, For::Window(id), setting_up);
                }
                OutputMsg::Redraw => {}
                OutputMsg::SetNdi { id, name, rate } => output.set_ndi(id, name, rate),
                OutputMsg::NdiSilent(silent) => {
                    output.ndi_silent = silent;
                    output.set_ndi_sound();
                }
                OutputMsg::NdiReceivers => receivers_changed = true,
                OutputMsg::SetDeckLink { id, wanted } => output.set_decklink(id, wanted),
                OutputMsg::RemoveSurface(id) => {
                    let letting_go = Instant::now();
                    output.targets.retain(|target| target.id != id);
                    output.held.since(Part::Setup, For::Window(id), letting_go);
                }
                OutputMsg::Announcement(announcement) => {
                    // A video that stops playing on the layer goes back to its still (unless a
                    // box on the slide plays the same file).
                    let was = output.announcement.as_ref().and_then(Announcement::video_name).map(str::to_owned);
                    let now = announcement.as_ref().and_then(Announcement::video_name);
                    if let Some(was) = was
                        && now != Some(was.as_str())
                        && !output.frame.slide_video.iter().any(|(file, _)| *file == was)
                    {
                        let forgetting = Instant::now();
                        sp_render::forget_picture(&was);
                        output.held.since(Part::Locks, For::Nothing, forgetting);
                    }
                    let same = match (&output.announcement, &announcement) {
                        (Some(before), Some(after)) => before.same_picture(after),
                        _ => false,
                    };
                    output.announcement = announcement;
                    // A new announcement dissolves in over what is there, and gets all its tries.
                    if !same {
                        output.round.new_picture();
                        output.look = sp_model::Transition::default();
                        new_picture = true;
                    }
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
                    output.set_fading(None);
                    for target in &mut output.targets {
                        target.fade = None;
                        target.shown = None;
                    }
                    output.ndi_fades.clear();
                    output.ndi_shown.clear();
                }
                OutputMsg::Dark(dark) => {
                    // What each screen last sent is forgotten, so the change goes out at once.
                    if output.dark != dark {
                        output.dark = dark;
                        for target in &mut output.targets {
                            target.shown = None;
                        }
                        output.ndi_shown.clear();
                    }
                }
                OutputMsg::PreviewScreen(screen) => {
                    if output.preview_screen != screen {
                        output.preview_screen = screen;
                        preview_again = true;
                    }
                }
                OutputMsg::CloseButton { id, drawn } => {
                    if let Some(target) = output.targets.iter_mut().find(|target| target.id == id) {
                        target.close = drawn;
                        target.close_tries = 0;
                        close_changed = true;
                    }
                }
                OutputMsg::NextPictureCame => window_came = true,
                OutputMsg::Shutdown => {
                    // The app is closing: each NDI source finishes the picture it is sending
                    // and is taken down before the program goes (see `NdiOutput::stop`). The
                    // sound stops being handed to them first.
                    output.ndi_sound.clear();
                    for (_, ndi) in output.ndi.drain(..) {
                        let (name, stopping) = (ndi.name().to_owned(), Instant::now());
                        ndi.stop();
                        crate::applog::event(
                            "ndi",
                            &format!(
                                "NDI \"{name}\" was stopped before closing, in {:.0} ms",
                                stopping.elapsed().as_secs_f64() * 1000.0
                            ),
                        );
                    }
                    return;
                }
            }
            next = rx.try_recv().ok();
        }
        // The Screens window says how many are watching each source: it looks again.
        if receivers_changed && let Some(wake) = &output.waker {
            let waking = Instant::now();
            wake();
            output.held.since(Part::Wake, For::Nothing, waking);
        }
        if nothing_else && preview_again {
            // Only the preview is drawn, at once, for the screen it now looks at.
            let frame = output.frame.clone();
            output.preview_fade = None;
            output.preview_shown = None;
            output.draw_live_preview(&frame, true);
        }
        if nothing_else {
            // Only the NDI pictures are drawn, and only if the change of count asks for it.
            // Drawing the windows too would cut short a fade under way in them, for something
            // the audience in the room has no part in.
            let owed = output.ndi_state != Shown::default() || output.ndi_lost.owed();
            if ndi_or_preview && ndi_count_needs_drawing(&output.ndi_seen(), &output.ndi_drawn_for, owed) {
                output.draw_ndi();
            }
        } else {
            output.draw(new_picture);
        }
        // A window whose X is to look different, and wasn't drawn just now, is shown again;
        // so is one that was passed over, now that its next picture has come.
        if close_changed || window_came {
            output.show_windows_again();
        }
    }
}

/// Which videos are playing, for [`changes_with_video`].
#[derive(Clone, Copy)]
struct Playing {
    /// One plays behind the words.
    behind: bool,
    /// One plays in a box on the slide, its layout or a stage layout.
    in_boxes: bool,
    /// One plays on the Announcements layer, which this screen shows.
    announced: bool,
}

/// Whether an audience screen with this look (see-through or not) shows a playing video.
fn changes_with_video(look: &crate::screens::Look, see_through: bool, playing: Playing) -> bool {
    // A see-through screen leaves out the media behind the words.
    (look.media && !see_through && playing.behind) || playing.in_boxes || (look.announcements && playing.announced)
}

/// Whether fades draw each slide once and then blend (see `Output::cache_sides`).
///
/// Testing aid, and the way back to the old drawing if a fade ever looks wrong:
/// SOFTPRESENTER_FADE_CACHE=0 (or `off`) draws both slides again at every step of a fade, as
/// the output did before; a file named `fade-cache-off` in the data folder does the same without
/// setting anything. Read once, as the screens' thread starts.
fn fade_cache_wanted() -> bool {
    if let Ok(asked) = std::env::var("SOFTPRESENTER_FADE_CACHE") {
        return !matches!(asked.trim().to_ascii_lowercase().as_str(), "0" | "off" | "no" | "false");
    }
    !crate::bible_store::data_dir().is_some_and(|folder| folder.join("fade-cache-off").exists())
}

/// How far in a fade is, from 0 to 1, eased so it starts and ends gently.
fn fade_amount(started: Instant, seconds: f32) -> f32 {
    let t = (started.elapsed().as_secs_f32() / seconds.max(0.001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Output {
    /// The running totals a pass's share of is told when it is held up.
    fn totals(&self) -> Totals {
        Totals {
            gpu_wait: self.painter.gpu_waited(),
            runs: self.painter.runs(),
            reruns: self.painter.reruns(),
            shared_locks: sp_render::lock_waits(),
            log: crate::applog::time_writing(),
            work: self.painter.work(),
        }
    }

    /// Ends the pass under way; one held up is written to the log, with where its time went and
    /// what the DeckLink and NDI threads were doing. One long enough to hold up an NDI picture
    /// is kept a while, and an NDI source's hitch is told with what this thread was doing then.
    fn end_pass(&mut self) {
        let now = Instant::now();
        if self.held.is_long(now) {
            let totals = self.totals();
            self.held.keep(now, totals);
        }
        self.tell_ndi_hitches(now);
        let Some(took) = self.held.end(now) else { return };
        let began_at = chrono::Local::now() - chrono::Duration::from_std(took).unwrap_or_default();
        let began_at = began_at.format("%H:%M:%S%.3f").to_string();
        let mut line = self.held.words(took, &began_at, self.totals(), |what| self.step_for(what));
        for (_, output) in &self.decklink {
            let Some(tried) = output.last_try() else { continue };
            let state = match output.state() {
                sp_decklink::State::Starting => "starting",
                sp_decklink::State::Playing(_) => "playing",
                sp_decklink::State::Waiting(_) => "waiting for its device",
            };
            line.push_str(&format!(
                " DeckLink \"{}\": {state}; last tried {:.1} s before the end, for {:.1} ms ({}).",
                tried.device,
                now.saturating_duration_since(tried.began).as_secs_f64(),
                tried.took.as_secs_f64() * 1000.0,
                if tried.opened { "opened" } else { "not opened" },
            ));
        }
        for (_, ndi) in &self.ndi {
            let receivers = match ndi.receivers() {
                None => "receivers not known".to_owned(),
                Some(1) => "1 receiver".to_owned(),
                Some(count) => format!("{count} receivers"),
            };
            line.push_str(&format!(
                " NDI \"{}\": {receivers}, longest send lately {:.1} ms.",
                ndi.name(),
                ndi.longest_send().as_secs_f64() * 1000.0
            ));
        }
        crate::applog::event("output", &line);
    }

    /// Says in the log, for each NDI source that has had a hitch since this last looked (a new
    /// picture sent more than two pictures' times after the one before while pictures were
    /// moving, which somebody watching could see), what held it up: on the source's thread,
    /// and on this one then. At most one line every few seconds for each source (see
    /// `NdiOutput::hitch_to_tell`); otherwise one atomic load per source.
    fn tell_ndi_hitches(&self, now: Instant) {
        for (_, ndi) in &self.ndi {
            let Some((hitch, more)) = ndi.hitch_to_tell(now) else { continue };
            let ms = |took: Duration| format!("{:.1} ms", took.as_secs_f64() * 1000.0);
            let more = match more {
                0 => String::new(),
                1 => " (1 more since the last such line)".to_owned(),
                more => format!(" ({more} more since the last such line)"),
            };
            let then = self
                .held
                .then(hitch.at.checked_sub(hitch.gap).unwrap_or(hitch.at), hitch.at, |what| self.step_for(what));
            crate::applog::problem(
                "ndi",
                &format!(
                    "NDI \"{}\" stood still: {} between two new pictures, up to {}, while pictures were moving and \
                     somebody watched{more}. The picture was handed over {} after the one before was sent, waited {} \
                     for NDI's thread and took {} to send. On the screens' thread then: {then}",
                    ndi.name(),
                    ms(hitch.gap),
                    held::clock(hitch.at),
                    ms(hitch.handed_after),
                    ms(hitch.waited),
                    ms(hitch.took),
                ),
            );
        }
    }

    /// Waits as `wait` does (for at most `most`, when it says), as the end of one pass and the
    /// start of the next; a wait that went on well past `most` is noted for the minute.
    fn idle<T>(&mut self, most: Option<Duration>, wait: impl FnOnce() -> T) -> T {
        self.end_pass();
        let asked = Instant::now();
        let got = wait();
        let now = Instant::now();
        if let Some(most) = most {
            self.held.woke_late(now.saturating_duration_since(asked).saturating_sub(most));
        }
        self.held.begin(now, self.totals());
        got
    }

    /// What a step was for, in words for the log.
    fn step_for(&self, what: For) -> String {
        let screen = |id: Uuid| -> String {
            if let Some(target) = self.targets.iter().find(|target| target.id == id) {
                return format!("\"{}\"", target.name);
            }
            if let Some((_, ndi)) = self.ndi.iter().find(|(have, _)| *have == id) {
                return format!("\"{}\" (NDI)", ndi.name());
            }
            if let Some(tried) = self.decklink.iter().find(|(have, _)| *have == id).and_then(|(_, out)| out.last_try())
            {
                return format!("\"{}\" (DeckLink)", tried.device);
            }
            "a screen".to_owned()
        };
        match what {
            For::Nothing => String::new(),
            For::Window(id) => format!("the window of {}", screen(id)),
            For::Ndi(id) => format!("the NDI and DeckLink picture of {}", screen(id)),
            For::Screen(id) => format!("what {} shows", screen(id)),
            For::Preview => "the operator's preview".to_owned(),
        }
    }

    fn add_surface(
        &mut self,
        id: Uuid,
        name: String,
        surface: wgpu::Surface<'static>,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    ) {
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
        // Testing aid: SOFTPRESENTER_SLOW_PRESENT (see `slow_present`).
        let slow = std::env::var("SOFTPRESENTER_SLOW_PRESENT").ok().and_then(|asked| slow_present(&asked, &name));
        if let Some(slow) = slow {
            crate::applog::event(
                "output",
                &format!(
                    "Testing aid: every picture the window of \"{name}\" asks for comes {} ms late.",
                    slow.as_millis()
                ),
            );
        }
        // The window's thread says when a picture the screens' thread stopped waiting for has
        // come; it only passes the word on.
        let tell = self.from_ndi.clone();
        let came = move || {
            let _ = tell.send(OutputMsg::NextPictureCame);
        };
        let next = NextPicture::new(Arc::new(surface), slow, came);
        // What draws the X over the window's picture (see `close_button`): made with the first
        // window of each format, as it is set up, never as the X first shows (making it holds
        // the thread as long as setting a window up does).
        let close_mark = match self.close_marks.iter().find(|mark| mark.format() == format) {
            Some(mark) => mark.clone(),
            None => {
                let mark = Arc::new(sp_render::CloseMark::new(&self.device, format));
                self.close_marks.push(mark.clone());
                mark
            }
        };
        // Likewise what copies the painted picture into the window's: making it compiles a
        // shader and a pipeline, which takes longer on a PC's DirectX 12 or Vulkan than on a Mac
        // (a window's whole set-up held a PC's screens' thread 33 ms as it opened). A window of a
        // format seen before (the next display's, or a screen switched on again while others
        // show) uses the one made then.
        let blitter = match self.blitters.iter().find(|(made_for, _)| *made_for == format) {
            Some((_, blitter)) => blitter.clone(),
            None => {
                let blitter = Arc::new(wgpu::util::TextureBlitter::new(&self.device, format));
                self.blitters.push((format, blitter.clone()));
                blitter
            }
        };
        // A screen has one window: a new one takes the old one's place.
        self.targets.retain(|target| target.id != id);
        self.targets.push(Target {
            id,
            name,
            blitter,
            pass: None,
            texture: target_texture(&self.device, config.width, config.height),
            next,
            config,
            shown: None,
            fade: None,
            configured,
            trouble: crate::applog::Lasting::default(),
            state: Shown::default(),
            passed_over: None,
            passed_over_said: false,
            close: None,
            close_shown: None,
            close_mark,
            close_tries: 0,
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
        // Not while a picture asked for is still to come: setting the surface up would wait for
        // it. It is set up before the next picture is shown, once that one has come.
        target.configured = !target.next.waiting() && configure(&self.device, target.next.surface(), &target.config);
        target.texture = target_texture(&self.device, w, h);
        // Pictures built for the old size cannot be faded at the new one.
        target.shown = None;
        target.fade = None;
    }

    /// Starts, renames or stops a screen's NDI source, which sends `rate` pictures a second,
    /// and records how that went for the operator.
    fn set_ndi(&mut self, id: Uuid, name: Option<String>, rate: sp_ndi::Rate) {
        // Stop the old source first, so a renamed one does not appear twice.
        self.ndi.retain(|(have, _)| *have != id);
        self.ndi_sound.retain(|(have, _)| *have != id);
        // Starting never waits on NDI: the source starts on its own thread.
        let status = name.map(|name| {
            // Its thread says when its receivers come and go; it only passes the word on. It
            // says in the log each minute how its pictures went out.
            let tell = self.from_ndi.clone();
            let wake = move || {
                let _ = tell.send(OutputMsg::NdiReceivers);
            };
            let log: sp_ndi::Log = Arc::new(|line: &str, problem: bool| {
                if problem {
                    crate::applog::problem("ndi", line);
                } else {
                    crate::applog::event("ndi", line);
                }
            });
            let started = NdiOutput::start_with_wake(&name, rate, wake, log);
            match started {
                Ok(ndi) => {
                    let entry = NdiEntry::Started(ndi.name().to_owned(), ndi.watch());
                    self.ndi.push((id, ndi));
                    entry
                }
                Err(reason) => NdiEntry::Refused(reason),
            }
        });
        self.set_ndi_sound();
        if self.ndi.is_empty() && self.decklink.is_empty() {
            self.forget_readbacks();
        }
        self.ndi_drawn_for.retain(|(have, _)| *have != id);
        // A new source gets the picture on screen now.
        if !self.ndi.is_empty() {
            let frame = self.frame.clone();
            self.send_ndi(&frame, NdiChange::Nothing);
        }
        let locking = Instant::now();
        if let Ok(mut shared) = self.ndi_status.lock() {
            shared.retain(|(have, _)| *have != id);
            shared.extend(status.map(|status| (id, status)));
        }
        self.held.since(Part::Locks, For::Nothing, locking);
    }

    /// Hands the app's sound to every NDI source but those switched silent, and stops handing
    /// it to those. Never waits: the sound goes from the mix's own thread (`sp_video::sound`).
    fn set_ndi_sound(&mut self) {
        let silent = &self.ndi_silent;
        self.ndi_sound.retain(|(id, _)| !silent.contains(id));
        for (id, ndi) in &self.ndi {
            if !silent.contains(id) && !self.ndi_sound.iter().any(|(have, _)| have == id) {
                let listener = NdiSound { to: ndi.sound(), name: ndi.name().to_owned(), heard: Default::default() };
                self.ndi_sound.push((*id, sp_video::sound::listen(listener)));
            }
        }
    }

    /// Nothing is read back for NDI or DeckLink any more: whatever such picture could not be
    /// drawn is no longer owed.
    fn forget_readbacks(&mut self) {
        self.ndi_state = Shown::default();
        self.ndi_trouble = crate::applog::Lasting::default();
        self.ndi_target = None;
        self.ndi_pending.clear();
        self.ndi_buffers.clear();
        self.ndi_dirty = None;
        self.ndi_put_off = false;
        self.ndi_lost = Lost::default();
        self.ndi_lost_trouble = crate::applog::Lasting::default();
    }

    /// What a screen shows now. Cheap copies: the picture, the slide and the theme are shared.
    fn content_for(&self, id: Uuid, frame: &Frame) -> Content {
        // Switched off, it shows what a screen of its kind shows while the kind is off.
        if self.dark.contains(&id) {
            let stage = self.stage.iter().find(|(have, _)| *have == id).map(|(_, layout)| layout);
            return switched_off(stage, &frame.theme);
        }
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
            return switched_off(None, &frame.theme);
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
        // The slide as far as its builds have got, with its linked text filled in; in the
        // version that is on, when this screen's theme has it for the slide's kind.
        let slide = frame.slide.as_ref().filter(|_| look.slides).map(|slide| {
            let in_version = frame.version.and_then(|version| theme.slide_in_version(slide, version));
            let (built, _) = in_version.as_ref().unwrap_or(slide).built(&theme, self.build_clock());
            Arc::new(crate::links::fill(&built, &values).into_owned())
        });
        let media = self.video_picture.clone().or_else(|| frame.media.clone());
        let announcement =
            self.announcement.clone().filter(|shown| look.announcements && (shown.on_slide_screens || !look.slides));
        (media.filter(|_| look.media), slide, over, theme, announcement)
    }

    /// Whether a screen's picture changes when a playing video gives a new picture: the video
    /// is behind its words or in a box on its slide, in its announcements, or (for a stage
    /// screen) in a screen's picture it shows. When unsure, true: a picture drawn that did not
    /// change costs time, one not drawn that did is wrong on the screen.
    fn shows_video(&self, id: Uuid) -> bool {
        if self.dark.contains(&id) {
            return false;
        }
        if let Some((_, layout)) = self.stage.iter().find(|(have, _)| *have == id) {
            // A stage layout's own videos are among the slide's (see `sync_slide_video`).
            return self.stage_on
                && (!self.frame.slide_video.is_empty()
                    || !self.frame.ndi.is_empty()
                    || layout
                        .templates
                        .first()
                        .is_some_and(|t| crate::stage::sources(t).iter().any(|screen| self.shows_video(*screen))));
        }
        // Black, a test pattern, or a key made from the picture of its fill (drawn with it).
        if self.key_of(id).is_some() || self.overlay.test_pattern.is_some() || !self.frame.enabled {
            return false;
        }
        // A screen being identified is drawn whole.
        if self.identify.iter().any(|(have, ..)| *have == id) {
            return true;
        }
        let look = self.look_of(id);
        let playing = Playing {
            behind: self.frame.video.is_some() || self.video_picture.is_some(),
            in_boxes: !self.frame.slide_video.is_empty() || !self.frame.ndi.is_empty(),
            announced: self
                .announcement
                .as_ref()
                .is_some_and(|shown| shown.video.is_some() && (shown.on_slide_screens || !look.slides)),
        };
        changes_with_video(&look, self.see_through(id).is_some(), playing)
    }

    /// How an audience screen shows the show: its own look, or every layer.
    fn look_of(&self, id: Uuid) -> crate::screens::Look {
        self.looks.iter().find(|(have, _)| *have == id).map_or_else(crate::screens::Look::default, |(_, look)| *look)
    }

    /// How a screen is see-through, if it is: an audience screen that asks to be, and is not a
    /// key (a key shows nothing of its own).
    fn see_through(&self, id: Uuid) -> Option<crate::screens::Alpha> {
        let look = self.look_of(id);
        look.see_through.filter(|_| look.key_of.is_none() && !self.is_stage(id))
    }

    /// The screen whose key this one shows, if it is a key (only an audience screen can be).
    fn key_of(&self, id: Uuid) -> Option<Uuid> {
        self.look_of(id).key_of.filter(|_| !self.is_stage(id))
    }

    /// The screen a key follows, when it can have a key: another audience screen that is not
    /// a key itself. (While the key is being identified it shows its own name instead.)
    fn fill_of(&self, key: Uuid) -> Option<Uuid> {
        let fill = self.key_of(key)?;
        let identified = self.identify.iter().any(|(have, ..)| *have == key);
        // A key switched off, or of a fill switched off, follows nothing: it is black.
        let dark = self.dark.contains(&key) || self.dark.contains(&fill);
        (fill != key && !self.is_stage(fill) && self.key_of(fill).is_none() && !identified && !dark).then_some(fill)
    }

    /// For a key in a window, the window of its fill, whose picture it is made from: a key goes
    /// out beside its fill only when both are in windows (or on displays), or both on NDI, so
    /// that the two always show the same picture (see `screens::key_trouble`). Any other key
    /// draws black, which keys nothing in.
    fn window_fill(&self, key: Uuid) -> Option<usize> {
        let fill = self.fill_of(key)?;
        self.targets.iter().position(|target| target.id == fill)
    }

    /// For a key on NDI or DeckLink, its fill's NDI source or DeckLink output, whose picture it
    /// is made from: both are given the same pictures read back from the GPU (see
    /// `window_fill`).
    fn ndi_fill(&self, key: Uuid) -> Option<Uuid> {
        let fill = self.fill_of(key)?;
        (self.ndi.iter().any(|(id, _)| *id == fill) || self.decklink.iter().any(|(id, _)| *id == fill)).then_some(fill)
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
        let building = self.held.scene_begins();
        let scene = self.build_scene_for(id, frame, w, h);
        self.held.scene_built(building, For::Screen(id));
        scene
    }

    fn build_scene_for(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> vello::Scene {
        // While screens are identified, each is marked over what it shows; one switched off
        // shows nothing.
        let identified = self
            .identify
            .iter()
            .find(|(have, ..)| *have == id && !self.dark.contains(&id))
            .map(|(_, name, color)| (name.clone(), *color));
        // A key shows nothing of its own: its picture is made from its fill's, where that is
        // painted (`present_keys_of`, `send_ndi`). One with no fill to follow is black, which
        // keys nothing in, so a switcher shows its camera untouched.
        let mut scene = if self.key_of(id).is_some() { black(w, h) } else { self.show_for(id, frame, w, h) };
        // Its colour and border, then its name on top, clear of the colour.
        if let Some((name, color)) = identified {
            identify_marks(&mut scene, color, identify_strength(self.identify_since.elapsed().as_secs_f32()), w, h);
            let label = sp_render::build_overlay(&mut self.fonts, &name, &self.identify_theme, w, h);
            scene.append(&label, None);
        }
        scene
    }

    /// The show as a screen shows it, for a target of a size: its layers, its theme, its
    /// colour and its covered edges; for a see-through screen, without what would cover the
    /// whole picture (see `see_through_content`), clear where nothing is drawn.
    fn show_for(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> vello::Scene {
        let see_through = self.see_through(id).is_some();
        let mask = if self.is_stage(id) { crate::screens::Look::default().mask } else { self.look_of(id).mask };
        let scene = self.show_with(id, frame, |fonts, layers, theme| build_layers(fonts, layers, theme, w, h).scene);
        // Covered with clear is not covered at all: a see-through screen's edges are cut away.
        if see_through { cut_covered_edges(scene, mask, w, h) } else { scene }
    }

    /// Lays out what a screen shows now for a target of a size, as [`Output::show_for`] does, and
    /// has `build` make of it what it needs: the whole picture, or only a part of it.
    fn show_with<R>(&mut self, id: Uuid, frame: &Frame, build: impl FnOnce(&mut Fonts, &Layers, &Theme) -> R) -> R {
        let content = self.content_for(id, frame);
        let see_through = self.see_through(id).is_some();
        let (media, slide, overlay, theme, announcement) = if see_through {
            let mut kept = std::mem::take(&mut self.plain_themes);
            let content = see_through_content(content, |theme| plain_theme(&mut kept, theme));
            self.plain_themes = kept;
            content
        } else {
            content
        };
        // Scrolling text moves on from when the slide was fully on screen.
        let time = Instant::now().checked_duration_since(self.slide_on).map_or(0.0, |since| since.as_secs_f32());
        // A stage screen keeps the plain look.
        let look = if self.is_stage(id) { crate::screens::Look::default() } else { self.look_of(id) };
        let backdrop =
            if see_through { Some(CLEAR) } else { (look.color != sp_model::Rgba::BLACK).then_some(look.color) };
        let layers = Layers {
            media_fit: frame.media_fit,
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: overlay.as_deref(),
            time,
            backdrop,
            mask: look.mask,
            announcement: announcement.as_ref().map(|shown| (&*shown.slide, &*shown.theme)),
        };
        // A stage screen's screen pictures show those screens' own pictures of this moment.
        let pictures = self.screen_pictures(id, frame);
        let fonts = &mut self.fonts;
        sp_render::with_screen_pictures(pictures, || build(fonts, &layers, &theme))
    }

    /// Whether the slide on a screen can be drawn once into a texture and laid over the video
    /// behind it by each of the video's pictures (see `steady_scene`): an ordinary audience
    /// screen with a video playing behind a slide that nothing else moves on.
    fn steady_wanted(&self, id: Uuid, frame: &Frame) -> bool {
        self.fade_cache
            && !self.round.slow
            && self.fading.is_none()
            && frame.enabled
            && frame.video.is_some()
            && self.video_picture.is_some()
            && frame.slide_video.is_empty()
            && frame.ndi.is_empty()
            && self.announcement.as_ref().is_none_or(|shown| shown.video.is_none())
            && self.overlay.test_pattern.is_none()
            && self.identify.is_empty()
            && !self.dark.contains(&id)
            && !self.is_stage(id)
            && self.key_of(id).is_none()
            && self.see_through(id).is_none()
    }

    /// What an audience screen shows while a video plays behind its slide and nothing else has
    /// changed since the last whole drawing (`draw_video` says so with `moving_only`): the slide
    /// above the video is drawn once into a texture (`Painter::cache_split`), and each picture of
    /// the video is that texture laid over the video's, instead of laying out and drawing the
    /// slide's words, outlines and shadows again. `None` when the screen must be drawn whole.
    ///
    /// Valid until the next whole drawing (which is made for anything that could change the
    /// slide: a new slide, a message, a build step, the time on a clock, a new size), when the
    /// textures are let go (`draw_now`).
    fn steady_scene(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> Option<vello::Scene> {
        if !self.steady_wanted(id, frame) {
            return None;
        }
        let picture = self.video_picture.clone()?;
        if let Some(at) = self.steady.iter().position(|steady| steady.id == id && steady.size == (w, h)) {
            if !self.steady[at].video.same_as(&picture) {
                // The video was given another picture to draw (its texture was made again, or its
                // pictures are copies): the part below the slide is made again.
                let under = self.show_with(id, frame, |_, layers, theme| sp_render::build_under(layers, theme, w, h));
                let entry = &mut self.steady[at];
                entry.side.set_under(under);
                entry.video = picture;
            }
            return Some(self.steady[at].side.scene().clone());
        }
        let building = self.held.scene_begins();
        let parts =
            self.show_with(id, frame, |fonts, layers, theme| sp_render::build_layer_parts(fonts, layers, theme, w, h));
        self.held.scene_built(building, For::Screen(id));
        let sp_render::Parts { under, over, .. } = parts;
        let side = self.cache_with(For::Screen(id), |painter, device, queue| {
            painter.cache_split(device, queue, under, &over, w, h)
        })?;
        let scene = side.scene().clone();
        crate::applog::event(
            "output",
            &format!(
                "The slide over the playing video is drawn once into a texture ({w} x {h}); each picture of the video is laid under it"
            ),
        );
        self.steady.push(Steady { id, size: (w, h), video: picture, side });
        Some(scene)
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
        self.send_ndi_of(frame, changed, Which::All, &[]);
    }

    /// [`Output::send_ndi`] for a drawing of some of the screens (`which`): a playing video's
    /// new picture or a step of a fade (only the pictures that move are drawn, and only the
    /// sources that show them have the one they sent put out of date), or the time on the
    /// stage screens (only theirs).
    ///
    /// A picture that is fading (`ndi_fades`) is drawn as far as the fade has got, whoever asks:
    /// the two slides' textures blended, or the fade drawn from the slides, as a window's is.
    /// `windows` are the windows a step of the fade has just painted: one that shows the very
    /// picture at NDI's size has it copied back from its own texture, with nothing painted again.
    fn send_ndi_of(&mut self, frame: &Frame, changed: NdiChange, which: Which, windows: &[usize]) {
        self.finish_ndi();
        if self.ndi.is_empty() && self.decklink.is_empty() {
            return;
        }
        // The slides kept over a video are good for as long as the slide stays as it is.
        let kept = which != Which::All;
        if !kept {
            self.steady.clear();
        }
        let seen = self.ndi_seen();
        // Only some of the pictures are drawn when all is as it should be: every source has the
        // last picture drawn for it, none is owed or put off. Anything else is drawn whole, so
        // what is owed to a source is never counted done by a drawing that left it out.
        let settled = ndi_settled(
            &seen,
            &self.ndi_drawn_for,
            self.ndi_state == Shown::default()
                && !self.ndi_lost.owed()
                && self.ndi_dirty.is_none()
                && self.ndi_after_fade.is_none(),
        );
        let which = if settled { which } else { Which::All };
        // A whole drawing is the one owed after a fade, now or once the picture on its way
        // back is here.
        if which == Which::All {
            self.ndi_after_fade = None;
        }
        // A drawing put off while a picture was on its way back is drawn now, or not drawn
        // because nobody is watching any more: what it changed counts as changed now.
        let changed = changed.with(self.ndi_dirty);
        // Another picture is asked for, which a source nobody is watching will not get: the
        // one it had is out of date. (Said for every such picture; the source acts on it once.)
        let mut forget = ndi_out_of_date(&seen, changed);
        if which != Which::All {
            forget.retain(|id| which.takes(self.ndi_moves(*id), self.is_stage(*id)));
        }
        // A picture still on its way back from the GPU does not go to a source nobody is
        // watching any more, which would throw it away. It was newer than the one the source
        // has, so that one is out of date.
        for readback in &mut self.ndi_pending {
            forget.extend(ndi_strip(&mut readback.ids, &seen));
        }
        // A see-through screen is covered with clear, not black (see `NdiOutput::forget_as`).
        for (id, ndi) in self.ndi.iter().filter(|(id, _)| forget.contains(id)) {
            ndi.forget_as(self.see_through(*id).is_some());
        }
        let groups = ndi_groups(&seen);
        if groups.is_empty() {
            // Nobody is watching (or no source could start): no picture is owed, so none is
            // undrawn, whatever went wrong while someone watched.
            self.ndi_dirty = None;
            self.ndi_put_off = false;
            self.ndi_drawn_for.clear();
            self.ndi_shown.clear();
            self.ndi_fades.clear();
            self.ndi_state.not_asked();
            self.ndi_trouble = crate::applog::Lasting::default();
            self.ndi_lost = Lost::default();
            self.ndi_lost_trouble = crate::applog::Lasting::default();
            return;
        }
        if !self.ndi_pending.is_empty() {
            // Drawn when the picture before it is back. One already put off is passed over: its
            // sources never get it, and count it.
            if std::mem::replace(&mut self.ndi_put_off, true) {
                let wanted: Vec<Uuid> = seen.iter().filter(|source| source.wanted).map(|source| source.id).collect();
                self.ndi_note(&wanted, sp_ndi::Before::PassedOver);
            }
            self.held.happened(Instant::now(), Happened::NdiPutOff);
            self.ndi_dirty = Some(changed);
            return;
        }
        self.ndi_dirty = None;
        self.ndi_put_off = false;
        self.ndi_lost.painting();
        // A picture no longer drawn is not one for the next to fade in over.
        if which == Which::All {
            self.ndi_shown.retain(|(of, _)| groups.iter().any(|group| group.of == *of));
        }
        self.ndi_drawn_for =
            seen.iter().filter(|source| source.wanted).map(|source| (source.id, source.arrivals)).collect();
        let (w, h) = NDI_SIZE;
        let mut all_painted = true;
        let mut groups = groups.into_iter().enumerate();
        while let Some((place, group)) = groups.next() {
            // Not asked for after a picture the GPU was too slow with: it waits for the next
            // try, and so do the pictures after it, which their sources count as not drawn.
            if self.round.slow {
                self.ndi_state.skipped();
                all_painted = false;
                for (_, group) in std::iter::once((place, group)).chain(groups.by_ref()) {
                    self.ndi_note(&group.fills, sp_ndi::Before::NotDrawn);
                    self.ndi_note(&group.keys, sp_ndi::Before::NotDrawn);
                }
                break;
            }
            // Its picture is the one its sources are sending.
            if !which.takes(self.ndi_moves(group.of), self.is_stage(group.of)) {
                continue;
            }
            // A fade that is only beginning (the windows are still to be drawn, and its clock
            // to be started) is drawn by its first step, in this same pass: what the sources
            // have until then is the picture it fades from.
            let fading = self.ndi_fades.iter().position(|fade| fade.of == group.of);
            if fading.is_some() && self.windows_behind {
                continue;
            }
            // A see-through screen's picture is painted on clear, and goes with its alpha; its
            // keys are made from these same pixels, on their sources' own threads.
            let see_through = self.see_through(group.of);
            let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h)).clone();
            // A window that has just painted this step of the fade at this size holds the
            // picture already.
            let window =
                fading.and_then(|_| windows.iter().copied().find(|index| self.window_is_ndi_picture(*index, group.of)));
            let painted = match (window, fading) {
                (Some(_), _) => Ok(()),
                (None, Some(at)) => self.paint_ndi_fade(at, frame, &target),
                (None, None) => {
                    let steady = if kept { self.steady_scene(group.of, frame, w, h) } else { None };
                    let scene = steady.unwrap_or_else(|| self.scene_for(group.of, frame, w, h));
                    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
                    let painted = self.paint(&scene, &view, w, h, see_through.is_some(), For::Ndi(group.of));
                    // Kept for the next picture to fade in over.
                    if painted.is_ok() {
                        self.ndi_shown.retain(|(of, _)| *of != group.of);
                        self.ndi_shown.push((group.of, scene));
                    }
                    painted
                }
            };
            let target = window.and_then(|index| self.targets.get(index)).map_or(target, |shown| shown.texture.clone());
            if let Err(e) = painted {
                let message = format!("The NDI picture could not be drawn: {e}");
                if self.ndi_trouble.is_news(&message) {
                    crate::applog::problem("output", &message);
                }
                // The other sources' pictures are still drawn, unless the GPU was too slow.
                self.ndi_state.failed(&e);
                self.ndi_note(&group.fills, sp_ndi::Before::NotDrawn);
                self.ndi_note(&group.keys, sp_ndi::Before::NotDrawn);
                all_painted = false;
                continue;
            }
            // The copy is queued after the drawing, so it takes this picture even though the next
            // group is drawn into the same texture straight after.
            let reading_back = Instant::now();
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
            let asked = self.held.since(Part::Readback, For::Ndi(group.of), reading_back);
            let ids = group.fills.into_iter().chain(group.keys.iter().copied()).collect();
            let sent = ndi_sent(see_through);
            let readback = NdiReadback { ids, keys: group.keys, sent, place, buffer, padded_row, done, asked };
            self.ndi_pending.push(readback);
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
        // The NDI sources that send another's key, and whose.
        let keys: Vec<(Uuid, Uuid)> = (self.ndi.iter().map(|(id, _)| *id))
            .chain(self.decklink.iter().map(|(id, _)| *id))
            .filter_map(|id| self.ndi_fill(id).map(|fill| (id, fill)))
            .collect();
        let alone = |id: &Uuid| {
            self.identify.iter().any(|(have, ..)| have == id)
                || self.is_stage(*id)
                || self.looks.iter().any(|(have, _)| have == id)
                || self.themes.iter().any(|(have, _)| have == id)
                || keys.iter().any(|(_, fill)| fill == id)
        };
        let ndi = self.ndi.iter().map(|(id, ndi)| NdiSeen {
            id: *id,
            // Read before the count: receivers that come in between are drawn for twice,
            // not missed.
            arrivals: ndi.arrivals(),
            wanted: ndi_wanted(&ndi.state(), ndi.receivers()),
            alone: alone(id),
            key_of: keys.iter().find(|(key, _)| key == id).map(|(_, fill)| *fill),
            unwatched: ndi.receivers() == Some(0),
            stage: self.is_stage(*id),
        });
        // A DeckLink output is always watched: the card plays all the time, and keeps the
        // picture to play when its device comes back.
        let decklink = self.decklink.iter().map(|(id, _)| NdiSeen {
            id: *id,
            arrivals: 0,
            wanted: true,
            alone: alone(id),
            key_of: keys.iter().find(|(key, _)| key == id).map(|(_, fill)| *fill),
            unwatched: false,
            stage: self.is_stage(*id),
        });
        ndi.chain(decklink).collect()
    }

    /// A picture for these sources was lost on its way back from the GPU. The sources are told
    /// that what they last sent is out of date: they go on sending it to whoever is watching,
    /// but once those leave, whoever connects next is not given it. If any of them still has
    /// receivers, those keep the picture before it, and the loss counts (see `Lost`); a
    /// picture nobody is watching for any more is owed to nobody. Said in the log once while
    /// it lasts.
    fn ndi_lost(&mut self, ids: &[Uuid]) {
        // Each source still watched counts it, every time.
        self.ndi_note(ids, sp_ndi::Before::Lost);
        for (id, ndi) in self.ndi.iter().filter(|(id, _)| ids.contains(id)) {
            ndi.forget_as(self.see_through(*id).is_some());
        }
        if ndi_lost_is_owed(ids, &self.ndi_seen()) {
            self.held.happened(Instant::now(), Happened::NdiLost);
            self.ndi_lost.one_lost();
            if self.ndi_lost_trouble.is_news("An NDI picture could not be read back.") {
                crate::applog::problem("output", "An NDI picture could not be read back.");
            }
        }
    }

    /// Tells the NDI sources among `ids` that somebody is watching (or whose count isn't known)
    /// what became of a picture meant for them, for their per-minute line. Never waits.
    fn ndi_note(&self, ids: &[Uuid], what: sp_ndi::Before) {
        for (_, ndi) in self.ndi.iter().filter(|(id, _)| ids.contains(id)) {
            if ndi_wanted(&ndi.state(), ndi.receivers()) {
                ndi.note(what);
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
        let polling = Instant::now();
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.held.since(Part::Poll, For::Nothing, polling);
        let (w, h) = NDI_SIZE;
        let row = w as usize * 4;
        // Whether anything on the screens moves by itself now: a fade, a build, scrolling
        // words, a screen being identified.
        let moving = self.fading.is_some() || self.building() || self.scrolling() || self.identify_breathing();
        let mut still = Vec::new();
        let mut lost: Vec<Uuid> = Vec::new();
        for readback in std::mem::take(&mut self.ndi_pending) {
            let locking = Instant::now();
            let mut done = readback.done.lock().ok().and_then(|done| *done);
            self.held.since(Part::Locks, For::Nothing, locking);
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
                    let mut sources: Vec<(&NdiOutput, sp_ndi::Sent, bool)> = Vec::new();
                    let came_back = sp_ndi::Before::CameBack(readback.asked.elapsed());
                    for (id, ndi) in self.ndi.iter().filter(|(id, _)| readback.ids.contains(id)) {
                        if ndi_wanted(&ndi.state(), ndi.receivers()) {
                            // A key and its fill are handed these same pixels together.
                            let sent = if readback.keys.contains(id) { sp_ndi::Sent::Key } else { readback.sent };
                            ndi.note(came_back);
                            // Whether another picture follows this one by itself: said to the
                            // source, which then doesn't take the operator's rest on a slide
                            // for a picture that stood still.
                            sources.push((ndi, sent, moving || self.ndi_moves(*id)));
                        } else {
                            ndi.forget_as(self.see_through(*id).is_some());
                        }
                    }
                    let cards = self.decklink_for(&readback.ids, &readback.keys, readback.sent);
                    // Timed apart, then noted: the sources borrow the list the times go in.
                    let (mut rows, mut handed) = (Duration::ZERO, Duration::ZERO);
                    if !sources.is_empty() || !cards.is_empty() {
                        // The rows are copied out of the buffer once, without the padding, and
                        // every source sends those same pixels.
                        let copying = Instant::now();
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
                        let handing = Instant::now();
                        rows = handing - copying;
                        for (ndi, sent, moves) in sources {
                            if moves {
                                ndi.show_shared(w, h, pixels.clone(), sent);
                            } else {
                                ndi.show_shared_last(w, h, pixels.clone(), sent);
                            }
                        }
                        for (card, alpha) in cards {
                            card.show_shared(w, h, pixels.clone(), alpha);
                        }
                        handed = handing.elapsed();
                    }
                    let unmapping = Instant::now();
                    readback.buffer.unmap();
                    rows += unmapping.elapsed();
                    self.held.add(Part::Rows, For::Nothing, rows);
                    if handed > Duration::ZERO {
                        self.held.add(Part::HandOver, For::Nothing, handed);
                    }
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
        self.draw_with(new_picture, Which::All);
    }

    /// Draws what a new picture of a playing video changes, and nothing else: a screen that
    /// does not show the video (one without the media layer, a see-through one, a stage screen
    /// that follows no screen showing it) is not painted, shown, read back from the GPU or sent
    /// again, for its picture is the one it has. As a second begins, the stage screens are
    /// drawn with them for the time they show, and every screen when an audience screen shows
    /// the time (see `video_drawing`); all are drawn whole while anything else moves or fades.
    fn draw_video(&mut self) {
        let second =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
        let time_moves = self.ticks() && second != self.ticked_second;
        if time_moves {
            self.ticked_second = second;
        }
        let steady = self.fading.is_none()
            && self.targets.iter().all(|target| target.fade.is_none())
            && !self.building()
            && !self.scrolling()
            && !self.identify_breathing();
        self.draw_with(false, video_drawing(steady, time_moves, self.audience_ticks()));
    }

    /// Draws the screens: all of them, or just those `which` takes.
    fn draw_with(&mut self, new_picture: bool, which: Which) {
        let started = Instant::now();
        self.draw_now(new_picture, which);
        self.timing.add(started.elapsed());
        // How each NDI source's pictures went out is its own line, from its own thread.
        let (videos, screens, ndi) = (self.video_pictures, self.targets.len(), self.ndi.len());
        let spare = self.painter.spare_bytes() >> 20;
        let held = &mut self.held;
        let extra = || {
            format!(
                "{videos} video pictures, {screens} windows, {ndi} NDI, painter holds {spare} MB; {}",
                held.minute_words()
            )
        };
        if self.timing.report_each_minute("output", extra) {
            self.video_pictures = 0;
        }
    }

    /// The size of the live preview's textures.
    fn preview_size(&self) -> (u32, u32) {
        preview_size_of(&self.preview_targets).unwrap_or(LIVE_PREVIEW_SIZE)
    }

    /// Draws the audience picture small for the operator's window, as the screens show it: a
    /// new picture fades in as theirs does; a playing video's pictures are drawn at most thirty
    /// times a second.
    fn draw_live_preview(&mut self, frame: &Frame, new_picture: bool) {
        if self.preview_targets.len() < 2 {
            return;
        }
        if !new_picture && self.plays_video() && self.previewed_at.elapsed() < LIVE_PREVIEW_EVERY {
            return;
        }
        let (w, h) = self.preview_size();
        let scene = self.scene_for(self.preview_screen.unwrap_or(Uuid::nil()), frame, w, h);
        // A video's new picture during a fade: the fade goes on, towards the newer picture.
        if !new_picture && let Some(fade) = self.preview_fade.take() {
            // (Nothing is cached while a video plays, so this fade draws both pictures again.)
            self.preview_fade = Some(Fade { from: fade.from, to: scene, cache: None });
            return self.step_live_preview(None);
        }
        let before = self.preview_fade.take().map(|fade| fade.to).or_else(|| self.preview_shown.take());
        match before {
            Some(from) if new_picture && self.fading.is_some() => {
                let looking = self.preview_screen.unwrap_or(Uuid::nil());
                let sides = Sides { from: &from, painted: None, to: &scene };
                let cache = self.cache_sides(For::Preview, self.see_through(looking).is_some(), sides, w, h);
                self.preview_fade = Some(Fade { from, to: scene, cache });
                self.step_live_preview(Some(0.0));
            }
            // A picture that was not painted is not the one the preview shows.
            before => {
                let painted = self.paint_live_preview(Draw::Scene(&scene));
                self.preview_shown = if painted { Some(scene) } else { before };
            }
        }
    }

    /// Moves a fade in the live preview on to `amount` (as far as the screens' fade has got
    /// when `None`); at the end it shows the whole new picture.
    fn step_live_preview(&mut self, amount: Option<f32>) {
        let Some(fade) = self.preview_fade.take() else { return };
        let amount =
            amount.unwrap_or_else(|| self.fading.map_or(1.0, |(started, look)| fade_amount(started, look.duration())));
        if amount >= 1.0 {
            let Fade { from, to, .. } = fade;
            let painted = self.paint_live_preview(Draw::Scene(&to));
            self.preview_shown = Some(if painted { to } else { from });
            return;
        }
        let (w, h) = self.preview_size();
        let look = self.fading.map_or(sp_model::Transition::default(), |(_, look)| look);
        if let Some((from, to, weights)) = fade.blend(look, amount) {
            self.paint_live_preview(Draw::Blend(from, to, weights));
        } else {
            let mixing = Instant::now();
            let mixed = fade.mixed(look, amount, w, h);
            self.held.since(Part::Scene, For::Preview, mixing);
            self.paint_live_preview(Draw::Scene(&mixed));
        }
        self.preview_fade = Some(fade);
    }

    /// Paints a picture into the live preview texture the window isn't showing, then tells the
    /// window to show it; true when it was painted.
    fn paint_live_preview(&mut self, draw: Draw<'_>) -> bool {
        use std::sync::atomic::Ordering::{Acquire, Release};
        self.previewed_at = Instant::now();
        if self.round.slow {
            self.preview_state.skipped();
            return false;
        }
        let (w, h) = self.preview_size();
        let back = 1 - self.live_preview.front.load(Acquire).min(1);
        let view = self.preview_targets[back].create_view(&wgpu::TextureViewDescriptor::default());
        let painted = match draw {
            Draw::Scene(scene) => self.paint(scene, &view, w, h, false, For::Preview),
            Draw::Blend(from, to, weights) => {
                let blending = Instant::now();
                let drawn = self.blend.draw(&self.device, &self.queue, from, to, weights, &self.preview_targets[back]);
                self.held.since(Part::Paint, For::Preview, blending);
                if drawn {
                    Ok(())
                } else {
                    Err(sp_render::PaintError::Failed("the fade's pictures are not the preview's size".to_owned()))
                }
            }
        };
        if let Err(e) = painted {
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
            let waking = Instant::now();
            wake();
            self.held.since(Part::Wake, For::Preview, waking);
        }
        true
    }

    fn draw_now(&mut self, new_picture: bool, which: Which) {
        // A drawing of the screens a video changes says nothing of the others.
        if which == Which::All {
            self.round.begin();
        } else {
            self.round.begin_part();
        }
        // Cheap copies: the picture and the slide are shared, not duplicated.
        let frame = self.frame.clone();
        self.windows_behind = true;
        // A whole drawing is made for anything that could change a slide: the slides kept over
        // a video are made again from what is shown now.
        if which == Which::All {
            self.steady.clear();
        }
        // How far a fade already under way has got, before the clock is started again.
        let amount = self.fading.map(|(started, look)| fade_amount(started, look.duration()));
        // The NDI and DeckLink pictures fade as the windows do: a new picture's fades are set
        // up here, and its steps draw them (`fade_step`); what changes at once is drawn now.
        let ndi_fading = which == Which::All && self.begin_ndi_fades(&frame, new_picture, amount);
        self.send_ndi_of(&frame, NdiChange::Any, which, &[]);
        self.capture(&frame);
        let mut fading = false;
        // The stage screens are drawn after the others, with the fade's clock started: a stage
        // screen may show another screen's picture, which is then the one of this drawing.
        let mut order: Vec<usize> = (0..self.targets.len()).collect();
        order.sort_by_key(|&index| self.is_stage(self.targets[index].id));
        let mut clock_started = false;
        for index in order {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            if !clock_started && self.is_stage(id) {
                self.set_fading((fading || ndi_fading).then(|| (Instant::now(), self.look)));
                self.windows_behind = false;
                clock_started = true;
            }
            // A key in a window is shown when its fill's window is, from that picture, fade
            // and all (see `present_keys_of`); it keeps nothing of its own.
            if self.window_fill(id).is_some() {
                let target = &mut self.targets[index];
                (target.shown, target.fade) = (None, None);
                continue;
            }
            if !which.takes(self.shows_video(id), self.is_stage(id)) && self.targets[index].up_to_date() {
                continue;
            }
            let steady = if which == Which::All { None } else { self.steady_scene(id, &frame, w, h) };
            let scene = steady.unwrap_or_else(|| self.scene_for(id, &frame, w, h));
            // Stage screens change at once: the people on stage need the words now. So does a
            // key with no fill to follow, which is black.
            let at_once = self.is_stage(id) || self.key_of(id).is_some();
            // What the next fade starts from: the last whole picture, or for a fade cut short,
            // one of its two whole pictures.
            let target = &mut self.targets[index];
            // With no fade under way, the window's texture holds that last whole picture as it
            // was painted (a picture that could not be painted left it as it was, and so did
            // one that reached no window), and a cached fade starts from a copy of it.
            let painted = (target.fade.is_none() && target.shown.is_some()).then(|| target.texture.clone());
            let before = match target.fade.take() {
                Some(fade) => Some(fade_start(fade.from, fade.to, amount.unwrap_or(1.0), new_picture)),
                None => target.shown.take(),
            };
            match before {
                Some(from) if new_picture && self.look.duration() > 0.0 && !at_once => {
                    let see_through = self.see_through(id).is_some();
                    let sides = Sides { from: &from, painted: painted.as_ref(), to: &scene };
                    let cache = self.cache_sides(For::Window(id), see_through, sides, w, h);
                    self.targets[index].fade = Some(Fade { from, to: scene, cache });
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
        if !clock_started {
            self.set_fading((fading || ndi_fading).then(|| (Instant::now(), self.look)));
        }
        self.windows_behind = false;
        // The preview follows, fading if the screens do.
        if which == Which::All || self.shows_video(self.preview_screen.unwrap_or(Uuid::nil())) {
            self.draw_live_preview(&frame, new_picture);
        }
        if fading || ndi_fading {
            self.fade_step();
        }
        self.end_round();
    }

    /// Paints one picture of the drawing under way, on clear for a see-through screen (else on
    /// black), and notes how it went. Callers ask first whether the GPU has been too slow with
    /// a picture of this drawing (`round.slow`) and do not ask for theirs then: a GPU that is
    /// stuck holds the thread for one wait, not for one wait for every window.
    fn paint(
        &mut self,
        scene: &vello::Scene,
        view: &wgpu::TextureView,
        w: u32,
        h: u32,
        see_through: bool,
        what: For,
    ) -> Result<(), sp_render::PaintError> {
        // Testing aid: SOFTPRESENTER_GPU_TOO_SLOW (see `PretendSlow`).
        let pretending =
            self.pretend_slow.as_ref().is_some_and(|slow| slow.left > 0 && self.started.elapsed() >= slow.after);
        if self.pretend_slow.is_some() {
            self.painter.wait_at_most(pretending.then_some(Duration::ZERO));
        }
        let painting = Instant::now();
        let painted = if see_through {
            self.painter.paint_see_through(&self.device, &self.queue, scene, view, w, h)
        } else {
            self.painter.paint(&self.device, &self.queue, scene, view, w, h)
        };
        self.held.since(Part::Paint, what, painting);
        self.paint_noted(pretending, &painted);
        painted
    }

    /// Notes how one painting went: a picture that was given up counts against a pretended slow
    /// GPU, is kept for saying what held up an NDI picture, and the drawing under way hears of
    /// any trouble.
    fn paint_noted<T>(&mut self, pretending: bool, painted: &Result<T, sp_render::PaintError>) {
        let too_slow = painted.as_ref().is_err_and(sp_render::PaintError::worth_trying_again);
        if too_slow {
            self.held.happened(Instant::now(), Happened::GpuTooSlow);
        }
        // Only a picture that was given up counts: one drawn before is not waited for.
        if pretending
            && too_slow
            && let Some(slow) = &mut self.pretend_slow
        {
            slow.left -= 1;
        }
        self.round.note(&painted.as_ref().map(|_| ()).map_err(Clone::clone));
    }

    /// Starts, starts again or ends the fade under way (in the windows, and in the NDI and
    /// DeckLink pictures), each kept for saying what a late NDI picture came during.
    fn set_fading(&mut self, fading: Option<(Instant, sp_model::Transition)>) {
        let now = Instant::now();
        if self.fading.is_some() {
            self.held.happened(now, Happened::FadeEnded);
        }
        if fading.is_some() {
            self.held.happened(now, Happened::FadeBegan);
        }
        self.fading = fading;
    }

    /// Draws the two sides of a fade once each into textures of their own, so that the fade's
    /// steps only blend them (see `Fade`); `None` when the fade should draw both slides at
    /// every step as it always did:
    /// - the switch is off (`fade_cache_wanted`);
    /// - the GPU was too slow with a picture of this drawing (the wait is not asked for again);
    /// - the screen is see-through (its pictures keep their transparency);
    /// - something on the screens moves by itself: a playing video, a video or NDI picture in a
    ///   box on the slide or on the Announcements layer;
    /// - the GPU could not draw one of the pictures into a texture.
    ///
    /// The side the fade starts from is copied from the texture it is painted in already, when
    /// `sides` gives one of the size (a window's own picture), rather than painted again.
    ///
    /// A GPU that was too slow is said to the drawing under way as for any picture.
    fn cache_sides(
        &mut self,
        what: For,
        see_through: bool,
        sides: Sides<'_>,
        w: u32,
        h: u32,
    ) -> Option<(sp_render::CachedSide, sp_render::CachedSide)> {
        let Sides { from, painted, to } = sides;
        if !self.fade_cache
            || self.round.slow
            || see_through
            || !sp_render::fade_cacheable(self.look.kind)
            || self.plays_video()
            || self.video_picture.is_some()
        {
            return None;
        }
        self.cache_with(what, |painter, device, queue| {
            let old = match painted.filter(|painted| (painted.width(), painted.height()) == (w, h)) {
                Some(painted) => painter.cache_copy(device, queue, painted)?,
                None => painter.cache_whole(device, queue, from, w, h)?,
            };
            Ok((old, painter.cache_whole(device, queue, to, w, h)?))
        })
    }

    /// Has the painter draw pictures into textures, as `paint` has it draw a picture: a GPU
    /// pretended to be slow, the time held, and how it went told to the drawing under way. `None`
    /// when it could not be done; the caller draws the picture as it always did.
    fn cache_with<T>(
        &mut self,
        what: For,
        make: impl FnOnce(&mut Painter, &wgpu::Device, &wgpu::Queue) -> Result<T, sp_render::PaintError>,
    ) -> Option<T> {
        let pretending =
            self.pretend_slow.as_ref().is_some_and(|slow| slow.left > 0 && self.started.elapsed() >= slow.after);
        if self.pretend_slow.is_some() {
            self.painter.wait_at_most(pretending.then_some(Duration::ZERO));
        }
        let caching = Instant::now();
        let made = make(&mut self.painter, &self.device, &self.queue);
        self.held.since(Part::Paint, what, caching);
        self.paint_noted(pretending, &made);
        made.ok()
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
                let waking = Instant::now();
                wake();
                self.held.since(Part::Wake, For::Nothing, waking);
            }
        }
        if self.round.failed && !self.round.presented {
            let pausing = Instant::now();
            std::thread::sleep(UNDRAWN_PAUSE);
            self.held.since(Part::Pause, For::Nothing, pausing);
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
        let showing = frame.slide.is_some() || frame.media.is_some() || self.announcement.is_some();
        if started.elapsed().as_secs_f32() < after.unwrap_or(3.0)
            || !showing
            || CAPTURE_DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let (w, h) = NDI_SIZE;
        // The first screen's picture, as its window shows it (its look, its identify marks).
        // Testing aid: SOFTPRESENTER_CAPTURE_OUTPUT_STAGE=1 takes the first stage screen's, with
        // its pictures of other screens as their windows hold them.
        let stage = std::env::var_os("SOFTPRESENTER_CAPTURE_OUTPUT_STAGE").is_some();
        let first = self.targets.iter().find(|target| !stage || self.is_stage(target.id));
        let id = first.map_or(Uuid::nil(), |target| target.id);
        let behind = self.windows_behind;
        self.windows_behind = behind && !stage;
        let scene = self.scene_for(id, frame, w, h);
        self.windows_behind = behind;
        let capturing = Instant::now();
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
        self.held.since(Part::Capture, For::Nothing, capturing);
        match saved {
            Ok(()) => eprintln!("saved the output to {}", std::path::Path::new(&path).display()),
            Err(e) => eprintln!("could not save the output: {e}"),
        }
    }

    /// True while a video plays: as the media, in a box on the slide, or on the Announcements
    /// layer.
    fn plays_video(&self) -> bool {
        self.frame.plays_video() || self.announcement.as_ref().is_some_and(|shown| shown.video.is_some())
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
        #[cfg(target_os = "macos")]
        let Output { frame, painter, device, queue, video_texture, video_import, held, .. } = self;
        #[cfg(not(target_os = "macos"))]
        let Output { frame, painter, device, queue, video_texture, held, .. } = self;
        if let Some(frames) = frame.video.as_ref() {
            let fetching = Instant::now();
            let mut copying = Duration::ZERO;
            // A video the system plays goes to the GPU as the decoder's own memory; one whose
            // pictures can't (clear parts, FFmpeg's, another system) is copied.
            #[cfg(target_os = "macos")]
            let by_texture =
                video_import.get_or_insert_with(|| sp_video::GpuImport::new(device)).as_ref().and_then(|import| {
                    match frames.next_texture(import, device) {
                        sp_video::GpuNext::Picture(texture) => {
                            let began = Instant::now();
                            let shown = decoded_into_texture(texture, painter, device, queue, video_texture);
                            copying = began.elapsed();
                            if shown.is_none() {
                                // The GPU side couldn't take it: this picture is lost, and the
                                // ones after it are copied, as when the decoder's memory can't be
                                // made a texture, rather than each failing the same way.
                                frames.copy_from_now();
                                crate::applog::event(
                                    "video",
                                    "A picture couldn't go to the GPU without a copy; this video's pictures are copied from now on.",
                                );
                            }
                            Some(Some(shown))
                        }
                        sp_video::GpuNext::Nothing => Some(None),
                        sp_video::GpuNext::Unavailable => None,
                    }
                });
            let mut by_copy = || {
                frames.next_with(|lending| {
                    let began = Instant::now();
                    let shown = into_texture(lending, painter, device, queue, video_texture);
                    copying = began.elapsed();
                    shown
                })
            };
            #[cfg(target_os = "macos")]
            let shown = by_texture.unwrap_or_else(&mut by_copy);
            #[cfg(not(target_os = "macos"))]
            let shown = by_copy();
            held.add(Part::VideoFetch, For::Nothing, fetching.elapsed().saturating_sub(copying));
            if let Some(shown) = shown {
                held.add(Part::VideoCopy, For::Nothing, copying);
                self.video_picture = shown;
                self.video_pictures += 1;
                new |= self.video_picture.is_some();
            }
        }
        // Each video in a box on the slide: its newest picture stands in for the box's still.
        // It is a copy, because every thread that draws the slide (the operator's window too)
        // shows it.
        for (file, frames) in &self.frame.slide_video {
            if let Some(picture) = copied_from(frames, &mut self.held) {
                let showing = Instant::now();
                sp_render::show_picture(file, picture);
                self.held.since(Part::VideoCopy, For::Nothing, showing);
                self.video_pictures += 1;
                new = true;
            }
        }
        // An NDI source's receiver has already put its newest picture where the drawing threads
        // find it; all that is left is to see whether a new one has come, or a fade has moved
        // on (a source that stopped holds its picture two seconds, then fades out over half a
        // second, which no picture announces).
        if !self.frame.ndi.is_empty() {
            let changes = sp_render::ndi_changes();
            let visible = sp_render::ndi_visibility(&self.frame.ndi);
            if changes != self.ndi_seen || visible != self.ndi_visible_seen {
                self.ndi_seen = changes;
                self.ndi_visible_seen = visible;
                self.video_pictures += 1;
                new = true;
            }
        }
        // A video on the Announcements layer, the same way: its box shows its newest picture.
        if let Some((name, frames)) = self.announcement.as_ref().and_then(|shown| shown.video.as_ref())
            && let Some(picture) = copied_from(frames, &mut self.held)
        {
            let showing = Instant::now();
            sp_render::show_picture(name, picture);
            self.held.since(Part::VideoCopy, For::Nothing, showing);
            self.video_pictures += 1;
            new = true;
        }
        if new {
            let now = Instant::now();
            self.video_moves += 1;
            self.video_moved_at = now;
            // A video that stood still for more than two pictures (and less than a stop) is kept
            // for saying what held up an NDI picture.
            let still = self.video_came_at.replace(now).map(|came| now.saturating_duration_since(came));
            if let Some(still) = still.filter(|still| *still > 2 * sp_ndi::FRAME && *still <= Duration::from_secs(2)) {
                self.held.happened(now, Happened::VideoStill(still));
            }
        }
        new
    }

    /// True when something on the screens changes by itself each second: the time on a stage
    /// screen, or a message with the time or a timer in it.
    fn ticks(&self) -> bool {
        self.has_stage() || self.audience_ticks()
    }

    /// True when an audience screen shows the time or a timer: in a message, or in linked text.
    fn audience_ticks(&self) -> bool {
        self.overlay.message.as_ref().is_some_and(|(text, _)| crate::messages::has_tokens(text)) || self.links_tick()
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

    /// For a stage screen, what each screen its layout's screen pictures show is showing now:
    /// that screen's window's own picture (in a fade, the mix it shows), or, for a screen with
    /// no window (on NDI only, or off) or whose window this drawing has still to draw, its
    /// picture made from the frame. Never a stage screen's: no screen shows itself.
    fn screen_pictures(&mut self, id: Uuid, frame: &Frame) -> Vec<sp_render::ScreenPicture> {
        let Some(template) = self.stage.iter().find(|(have, _)| *have == id).and_then(|(_, l)| l.templates.first())
        else {
            return Vec::new();
        };
        if !self.stage_on || self.dark.contains(&id) {
            return Vec::new();
        }
        let mut pictures = Vec::new();
        for screen in crate::stage::sources(template) {
            if self.is_stage(screen) {
                continue;
            }
            let window = self.targets.iter().find(|target| target.id == screen).filter(|_| !self.windows_behind);
            let shown = window.and_then(|target| {
                let (w, h) = (target.config.width, target.config.height);
                let scene = match (&target.fade, self.fading) {
                    (Some(fade), Some((started, look))) => {
                        let amount = fade_amount(started, look.duration());
                        fade.mixed(look, amount, w, h)
                    }
                    (Some(fade), None) => fade.from.clone(),
                    (None, _) => target.shown.clone()?,
                };
                Some((scene, w, h))
            });
            let (scene, width, height) = match shown {
                Some(shown) => shown,
                None => {
                    let (w, h) = NDI_SIZE;
                    (self.scene_for(screen, frame, w, h), w, h)
                }
            };
            pictures.push(sp_render::ScreenPicture { screen, scene, width, height });
        }
        pictures
    }

    /// The stage screens' windows whose layouts show a picture of a screen that is fading.
    fn stage_following_fades(&self) -> Vec<usize> {
        (0..self.targets.len()).filter(|&index| self.follows_fade(self.targets[index].id)).collect()
    }

    /// Whether a stage screen's layout shows a picture of a screen whose window is fading.
    fn follows_fade(&self, id: Uuid) -> bool {
        let fading = |screen: &Uuid| self.targets.iter().any(|target| target.id == *screen && target.fade.is_some());
        let layout = self.stage.iter().find(|(have, _)| *have == id).filter(|_| self.stage_on);
        layout
            .and_then(|(_, layout)| layout.templates.first())
            .is_some_and(|template| crate::stage::sources(template).iter().any(fading))
    }

    /// Whether the picture drawn for a screen on NDI moves by itself: it shows a playing video,
    /// it is fading, or (a stage screen) it shows a picture of a screen that is.
    fn ndi_moves(&self, id: Uuid) -> bool {
        self.shows_video(id) || self.ndi_fades.iter().any(|fade| fade.of == id) || self.follows_fade(id)
    }

    /// Whether a window holds, at NDI's size, the very picture the NDI sources are sent for
    /// the screen `of`: that screen's own window, or the window of a screen that shows the show
    /// as it is when `of` does too.
    fn window_is_ndi_picture(&self, index: usize, of: Uuid) -> bool {
        let Some(target) = self.targets.get(index) else { return false };
        let own_way = |id: Uuid| {
            self.is_stage(id)
                || self.identify.iter().any(|(have, ..)| *have == id)
                || self.looks.iter().any(|(have, _)| *have == id)
                || self.themes.iter().any(|(have, _)| *have == id)
        };
        (target.texture.width(), target.texture.height()) == NDI_SIZE && same_picture(target.id, of, own_way)
    }

    /// Sets up the fades of the NDI and DeckLink pictures for a drawing of every screen, and
    /// says whether any is under way: a new picture fades in over the one each was last sent,
    /// as it does in the windows, by the same clock. Stage screens change at once, and so does a
    /// picture with none before it. Any other whole drawing ends the fades there were, as it
    /// does the windows': each then starts from what `fade_start` gives.
    ///
    /// The two sides are drawn once into textures when nothing moves on them (`cache_sides`),
    /// unless a window of NDI's size shows the same picture: its steps are copied back instead.
    fn begin_ndi_fades(&mut self, frame: &Frame, new_picture: bool, amount: Option<f32>) -> bool {
        for NdiFade { of, fade, .. } in std::mem::take(&mut self.ndi_fades) {
            let Fade { from, to, .. } = fade;
            self.ndi_shown.retain(|(have, _)| *have != of);
            self.ndi_shown.push((of, fade_start(from, to, amount.unwrap_or(1.0), new_picture)));
        }
        self.ndi_fade_owed = false;
        self.ndi_fade_due = None;
        if !new_picture || self.look.duration() <= 0.0 || (self.ndi.is_empty() && self.decklink.is_empty()) {
            return false;
        }
        let (w, h) = NDI_SIZE;
        for group in ndi_groups(&self.ndi_seen()) {
            let of = group.of;
            // The people on stage need the words now; a key with no fill to follow is black.
            if self.is_stage(of) || self.key_of(of).is_some() {
                continue;
            }
            let Some(at) = self.ndi_shown.iter().position(|(have, _)| *have == of) else { continue };
            let (_, from) = self.ndi_shown.swap_remove(at);
            let to = self.scene_for(of, frame, w, h);
            let in_a_window = (0..self.targets.len()).any(|index| self.window_is_ndi_picture(index, of));
            let cache = if in_a_window {
                None
            } else {
                let sides = Sides { from: &from, painted: None, to: &to };
                self.cache_sides(For::Ndi(of), self.see_through(of).is_some(), sides, w, h)
            };
            self.ndi_fades.push(NdiFade { of, fade: Fade { from, to, cache }, built: self.video_moves });
        }
        !self.ndi_fades.is_empty()
    }

    /// Draws the step the fade has reached in one NDI picture into NDI's texture: the blend of
    /// its two textures, or the fade drawn from the two slides, the one faded to built again
    /// when a playing video has given a new picture since (which moves through the fade).
    fn paint_ndi_fade(
        &mut self,
        at: usize,
        frame: &Frame,
        target: &wgpu::Texture,
    ) -> Result<(), sp_render::PaintError> {
        let (w, h) = NDI_SIZE;
        let of = self.ndi_fades[at].of;
        let see_through = self.see_through(of).is_some();
        if self.ndi_fades[at].built != self.video_moves {
            let fresh = self.scene_for(of, frame, w, h);
            let moves = self.video_moves;
            let fading = &mut self.ndi_fades[at];
            // The sides are cached only while nothing moves.
            fading.fade.cache = None;
            fading.fade.to = fresh;
            fading.built = moves;
        }
        let (amount, look) = match self.fading {
            Some((started, look)) => (fade_amount(started, look.duration()), look),
            None => (1.0, self.look),
        };
        let mixing = Instant::now();
        let fade = &self.ndi_fades[at].fade;
        let scene = if amount >= 1.0 {
            fade.to.clone()
        } else if see_through {
            see_through_transition(look.kind, &fade.from, &fade.to, amount, w, h)
        } else if let Some((from, to, weights)) = fade.blend(look, amount) {
            let drawn = self.blend.draw(&self.device, &self.queue, from, to, weights, target);
            self.held.since(Part::Paint, For::Ndi(of), mixing);
            return if drawn {
                Ok(())
            } else {
                Err(sp_render::PaintError::Failed("the fade's pictures are not NDI's size".to_owned()))
            };
        } else {
            fade.mixed(look, amount, w, h)
        };
        self.held.since(Part::Scene, For::Ndi(of), mixing);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        self.paint(&scene, &view, w, h, see_through, For::Ndi(of))
    }

    /// Gives the NDI sources and DeckLink outputs their picture of a step of the fade, when one
    /// is owed them (`fade_owes_ndi`) and can be drawn: not while the one before is still on its
    /// way back from the GPU (it is drawn by a later step, and nothing counts as put off), nor
    /// after one could not be drawn (it is tried again when the fade ends, so that the room's
    /// fade is never held up for NDI twice). True when the fade's last step, which draws the
    /// whole new pictures, was drawn for them, so nothing more is owed once the fade is over.
    fn step_ndi_fade(&mut self, amount: f32, moved: bool, windows: &[usize]) -> bool {
        if self.ndi_fades.is_empty() {
            return false;
        }
        let now = Instant::now();
        let due = self.ndi_fade_due.is_none_or(|due| due <= now);
        let since_video = self.video_came_at.map(|came| now.saturating_duration_since(came));
        self.ndi_fade_owed |= fade_owes_ndi(moved, since_video, due);
        let well = self.ndi_state == Shown::default() && !self.round.slow;
        if !self.ndi_fade_owed || !self.ndi_pending.is_empty() || !well {
            return false;
        }
        let frame = self.frame.clone();
        self.send_ndi_of(&frame, NdiChange::Any, Which::Moving, windows);
        self.ndi_fade_owed = false;
        // At the pace of the source that sends the most pictures a second (a DeckLink card
        // plays the newest it has at its own).
        let frame = self.ndi.iter().map(|(_, ndi)| ndi.rate().frame()).min().unwrap_or(sp_ndi::FRAME);
        self.ndi_fade_due = Some(next_ndi_due(self.ndi_fade_due, now, frame));
        if let Some(drawn) = &mut self.fade_drawn {
            drawn.sent += u32::from(!self.ndi_pending.is_empty());
        }
        amount >= 1.0 && self.ndi_state == Shown::default()
    }

    /// Ends the fades of the NDI pictures with the windows': each picture is now the one it
    /// faded to. Unless the last step drew that (`whole`), the sources are owed it: with the
    /// next picture of a playing video, or, when nothing moves, at their next picture's time.
    fn end_ndi_fades(&mut self, whole: bool) {
        if self.ndi_fades.is_empty() {
            return;
        }
        for NdiFade { of, fade, .. } in std::mem::take(&mut self.ndi_fades) {
            self.ndi_shown.retain(|(have, _)| *have != of);
            self.ndi_shown.push((of, fade.to));
        }
        let now = Instant::now();
        if !whole {
            let video_moves =
                self.video_came_at.is_some_and(|came| now.saturating_duration_since(came) < 2 * sp_ndi::FRAME);
            let wait = if video_moves { 2 * sp_ndi::FRAME } else { Duration::ZERO };
            self.ndi_after_fade = Some(self.ndi_fade_due.map_or(now, |due| due.max(now)) + wait);
        }
        self.ndi_fade_owed = false;
        self.ndi_fade_due = None;
    }

    /// True when there is a stage screen to keep the time on.
    fn has_stage(&self) -> bool {
        self.stage_on
            && (self.targets.iter().any(|target| self.is_stage(target.id))
                || self.ndi.iter().any(|(id, _)| self.is_stage(*id))
                || self.decklink.iter().any(|(id, _)| self.is_stage(*id)))
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
        // (never waited on); the other sources' pictures are left as they are.
        if self.ndi.iter().any(|(id, _)| self.is_stage(*id)) || self.decklink.iter().any(|(id, _)| self.is_stage(*id)) {
            self.send_ndi_of(&frame, NdiChange::Stage, Which::Stage, &[]);
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
        let cached = self.targets.iter().any(|target| target.fade.as_ref().is_some_and(|fade| fade.cache.is_some()))
            || self.ndi_fades.iter().any(|fading| fading.fade.cache.is_some());
        let mut drawn = self.fade_drawn.filter(|drawn| drawn.since == started).unwrap_or(FadeDrawn {
            since: started,
            steps: 0,
            cached: false,
            sent: 0,
        });
        drawn.steps += 1;
        drawn.cached |= cached;
        self.fade_drawn = Some(drawn);
        // A video playing behind the new slide keeps moving through the fade: each new picture
        // of it builds the picture being faded to again.
        let moved = self.take_video_picture();
        if moved {
            let frame = self.frame.clone();
            for index in 0..self.targets.len() {
                let (id, w, h) = {
                    let target = &self.targets[index];
                    (target.id, target.config.width, target.config.height)
                };
                if self.targets[index].fade.is_some() {
                    let fresh = self.scene_for(id, &frame, w, h);
                    if let Some(fade) = self.targets[index].fade.as_mut() {
                        // A video came after the fade began (the sides are cached only while
                        // nothing moves): it draws both slides again at each step, as before.
                        fade.cache = None;
                        fade.to = fresh;
                    }
                }
            }
        }
        // Stage screens showing a picture of a screen that is fading are drawn again with it.
        let following = self.stage_following_fades();
        // A stage screen on NDI or DeckLink that shows such a picture is owed it whole once the
        // fade is over: its last step is not known to it as one.
        let followed_on_ndi = (self.ndi.iter().map(|(id, _)| *id))
            .chain(self.decklink.iter().map(|(id, _)| *id))
            .any(|id| self.follows_fade(id));
        // The windows whose step was painted, whose pictures NDI may be given as they are.
        let mut stepped = Vec::new();
        for index in 0..self.targets.len() {
            let Some(fade) = self.targets[index].fade.take() else { continue };
            if amount >= 1.0 {
                // The last picture is the new one whole. A cached fade that is a plain sum of
                // its two pictures holds it in a texture already, painted from the same scene,
                // and its last step is nothing but that texture: it is shown from there, where
                // painting the scene again took a 4K screen with a photo behind the verse 1.4 to
                // 3 ms of the GPU on an M3 Max, and several times that on a slower one, for each
                // window at every verse. A last picture that could not be shown never was: the
                // window keeps the one the fade started from, for the next fade to start from.
                // (The textures of a cached fade go with it.)
                let id = self.targets[index].id;
                let whole = if self.see_through(id).is_some() { None } else { fade.blend(look, 1.0) };
                let painted = match whole {
                    Some((from, to, weights)) => self.present_draw(index, Draw::Blend(from, to, weights)),
                    None => self.present(index, &fade.to),
                };
                let Fade { from, to, .. } = fade;
                stepped.extend(painted.then_some(index));
                self.targets[index].shown = Some(if painted { to } else { from });
            } else {
                let (id, w, h) =
                    (self.targets[index].id, self.targets[index].config.width, self.targets[index].config.height);
                // On a see-through screen, a true cross-fade (see `see_through_transition`).
                // A cached fade that is a plain sum of its two pictures is one pass over their
                // textures; any other is drawn by the renderer (from the textures if cached).
                let plain = if self.see_through(id).is_some() { None } else { fade.blend(look, amount) };
                let painted = if let Some((from, to, weights)) = plain {
                    self.present_draw(index, Draw::Blend(from, to, weights))
                } else {
                    let mixing = Instant::now();
                    let scene = if self.see_through(id).is_some() {
                        see_through_transition(look.kind, &fade.from, &fade.to, amount, w, h)
                    } else {
                        fade.mixed(look, amount, w, h)
                    };
                    self.held.since(Part::Scene, For::Window(id), mixing);
                    self.present(index, &scene)
                };
                stepped.extend(painted.then_some(index));
                self.targets[index].fade = Some(fade);
            }
        }
        for index in following {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            let frame = self.frame.clone();
            let scene = self.scene_for(id, &frame, w, h);
            if self.present(index, &scene) {
                self.targets[index].shown = Some(scene);
            }
        }
        self.step_live_preview(Some(amount));
        // NDI pictures that have come back from the GPU are handed over at every step, and the
        // sources and DeckLink outputs are given the step itself when one is owed them. After
        // the windows: this may paint an NDI picture, and one the GPU is too slow with would
        // keep the windows' step from being asked for.
        self.finish_ndi();
        let whole = self.step_ndi_fade(amount, moved, &stepped) && !followed_on_ndi;
        let windows_done = self.targets.iter().all(|target| target.fade.is_none());
        if amount >= 1.0 || (windows_done && self.ndi_fades.is_empty()) {
            if let Some(drawn) = self.fade_drawn.take().filter(|drawn| drawn.since == started) {
                let seconds = drawn.since.elapsed().as_secs_f32().max(0.001);
                let sent = if self.ndi.is_empty() && self.decklink.is_empty() {
                    String::new()
                } else {
                    format!("; {} of its pictures went to NDI and DeckLink", drawn.sent)
                };
                crate::applog::event(
                    "output",
                    &format!(
                        "A {} fade of {seconds:.1} s drew {} pictures ({:.0} a second), {}{sent}",
                        look.kind.name(),
                        drawn.steps,
                        drawn.steps as f32 / seconds,
                        if drawn.cached { "from textures of the two slides" } else { "from the slides" }
                    ),
                );
            }
            self.set_fading(None);
            self.end_ndi_fades(whole);
            // An NDI picture that could not be drawn as the fade began, or during it, had no
            // drawing to be tried again in: a fade's steps give NDI nothing more once one of
            // its pictures could not be drawn, and the time set for another try is dropped
            // while a fade runs (whether it was still being tried or had been given up for
            // now). It is tried now, as part of this drawing, which sets a time for the next
            // try if it must.
            if self.ndi_state != Shown::default() {
                let frame = self.frame.clone();
                self.send_ndi(&frame, NdiChange::Any);
            }
        }
    }

    /// How long the thread may sleep with nothing to draw before it must look again: not at
    /// all (`None`) unless the GPU still holds memory the painter gave back (`rested` is
    /// false), a picture the GPU was too slow with is waiting to be drawn again, or a window's
    /// X that couldn't be shown is to be tried again.
    fn look_again_in(&self, rested: bool) -> Option<Duration> {
        let for_memory = (!rested).then_some(GPU_LOOK_AGAIN);
        [for_memory, self.draw_again_in(), self.close_again_in()].into_iter().flatten().min()
    }

    /// How long until a window's X that couldn't be shown is tried again, if one is to be.
    fn close_again_in(&self) -> Option<Duration> {
        self.close_again_at.map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// How long until a picture the GPU was too slow with, NDI pictures lost on their way back
    /// from it, or the NDI pictures owed after a fade are due to be drawn, if any are.
    fn draw_again_in(&self) -> Option<Duration> {
        let at = [self.round.again_at, self.ndi_lost.again_at, self.ndi_after_fade].into_iter().flatten().min()?;
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
        // So is the whole picture the sources are owed after a fade.
        let after_fade = self.ndi_after_fade.take_if(|at| *at <= now).is_some();
        if lost_due || after_fade {
            self.draw_ndi();
        }
        lost_due || after_fade
    }

    /// Until when this pass may wait for its windows' next pictures: `None` when only windows
    /// are drawn for (each is then waited for as long as it always was, which paces a fade to
    /// the display). While an NDI source somebody watches (or whose count isn't known) or a
    /// DeckLink card is drawn for too, the windows of one pass are waited for
    /// `next_picture::FED_WAIT` in all, from when the pass began: a window whose pictures come
    /// slowly then misses pictures of its own and never makes theirs late.
    fn windows_until(&self) -> Option<Instant> {
        let fed =
            !self.decklink.is_empty() || self.ndi.iter().any(|(_, ndi)| ndi_wanted(&ndi.state(), ndi.receivers()));
        fed.then(|| self.held.began() + next_picture::FED_WAIT)
    }

    /// Paints a picture and shows it in one window; true when it was painted. A picture that
    /// cannot be painted leaves the one before it on the screen, and is said in the log once
    /// for as long as that lasts. (Too heavy stays too heavy; too slow may pass, and the end
    /// of the drawing sets the screens to be drawn again.)
    fn present(&mut self, index: usize, scene: &vello::Scene) -> bool {
        self.present_draw(index, Draw::Scene(scene))
    }

    /// Paints or blends a picture and shows it in one window, as `present` says.
    fn present_draw(&mut self, index: usize, draw: Draw<'_>) -> bool {
        let Some(target) = self.targets.get(index) else { return false };
        let (id, w, h) = (target.id, target.config.width, target.config.height);
        let view = target.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Not asked for after a picture the GPU was too slow with: it waits for the next try,
        // and nothing is said of it, since nothing went wrong with it.
        if self.round.slow {
            self.targets[index].state.skipped();
            return false;
        }
        // A see-through screen is painted on clear, and shown solid as its alpha says.
        let see_through = self.see_through(id);
        let painted = match draw {
            Draw::Scene(scene) => self.paint(scene, &view, w, h, see_through.is_some(), For::Window(id)),
            Draw::Blend(from, to, weights) => {
                let blending = Instant::now();
                let drawn = self.blend.draw(&self.device, &self.queue, from, to, weights, &self.targets[index].texture);
                self.held.since(Part::Paint, For::Window(id), blending);
                if drawn {
                    Ok(())
                } else {
                    Err(sp_render::PaintError::Failed("the fade's pictures are not this window's size".to_owned()))
                }
            }
        };
        let until = self.windows_until();
        let Output { device, queue, targets, round, held, .. } = self;
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
        if show_in_window(device, queue, target, &view, see_through.map(solid_fill), held, until) {
            round.presented = true;
        }
        // The keys of this picture, made from it now, so they never show another moment. (A
        // picture not painted leaves its keys as they are, showing the picture before, as this
        // window does.)
        self.present_keys_of(index);
        true
    }

    /// Shows again, in each window that is owed it, the picture it already has: one copy into
    /// the window, nothing painted. A window is owed it when its X is to look different from
    /// the X it shows (see `close_button`), or when it was passed over, its next picture not
    /// having come in time (see `output/next_picture.rs`): the picture painted for it then, or
    /// a newer one since, is in its texture and never reached it. Without this a window passed
    /// over at a fade's last step went on showing a step of the fade, and one passed over at a
    /// cut the slide before, until something else was drawn. Not in a window that is fading,
    /// whose steps show it; nor in one with no picture painted yet, which shows it with its
    /// first. An X that couldn't be shown is tried again a few times, a moment apart; a window
    /// still waiting for its next picture is shown when that comes (its thread says so).
    fn show_windows_again(&mut self) {
        self.close_again_at = None;
        let until = self.windows_until();
        for index in 0..self.targets.len() {
            let target = &self.targets[index];
            let close_due = target.close != target.close_shown && target.close_tries < CLOSE_TRIES;
            if !(close_due || target.passed_over.is_some()) || target.fade.is_some() {
                continue;
            }
            // A key shows its fill's picture as a key; any other window its own picture, solid
            // if it is see-through.
            let id = target.id;
            let (from, solid) = match self.window_fill(id) {
                Some(fill) => (fill, Some(Solid::Key)),
                None => (index, self.see_through(id).map(solid_fill)),
            };
            let Some(source) = self.targets.get(from) else { continue };
            if source.shown.is_none() || source.state != Shown::default() {
                continue;
            }
            let picture = source.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let Output { device, queue, targets, held, close_again_at, .. } = self;
            let target = &mut targets[index];
            // (One still waiting for its next picture costs no try: it is shown when that comes.)
            if !show_in_window(device, queue, target, &picture, solid, held, until)
                && close_due
                && target.passed_over.is_none()
            {
                target.close_tries += 1;
                if target.close_tries < CLOSE_TRIES {
                    *close_again_at = Some(Instant::now() + CLOSE_AGAIN_AFTER);
                }
            }
        }
    }

    /// Shows, in the windows of the screens that are its key, the key of the picture just
    /// painted for the window at `fill`: how solid each of its pixels is, as a grey, made from
    /// that very picture on the GPU (scaled, should their sizes differ).
    fn present_keys_of(&mut self, fill: usize) {
        let keys: Vec<usize> = (0..self.targets.len())
            .filter(|key| *key != fill && self.window_fill(self.targets[*key].id) == Some(fill))
            .collect();
        if keys.is_empty() {
            return;
        }
        let picture = self.targets[fill].texture.create_view(&wgpu::TextureViewDescriptor::default());
        let until = self.windows_until();
        let Output { device, queue, targets, round, held, .. } = self;
        for key in keys {
            let target = &mut targets[key];
            target.state.painted();
            if show_in_window(device, queue, target, &picture, Some(Solid::Key), held, until) {
                round.presented = true;
            }
        }
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

/// Nothing: a see-through screen's colour where nothing is drawn.
const CLEAR: sp_model::Rgba = sp_model::Rgba { r: 0, g: 0, b: 0, a: 0 };

/// What a screen shows while it is switched off, or while its kind is: no media, no slide,
/// nothing over it and no announcement; its own background only (black, or clear for a
/// see-through screen), with a stage screen's layout, or the main theme.
fn switched_off(stage_layout: Option<&Arc<Theme>>, theme: &Arc<Theme>) -> Content {
    (None, None, None, stage_layout.unwrap_or(theme).clone(), None)
}

/// Black, filling a target of this size.
fn black(w: u32, h: u32) -> vello::Scene {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::{Color, Fill};
    let mut scene = vello::Scene::new();
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    scene
}

/// A see-through screen's covered edges are clear: what is drawn there is cut away, where a
/// solid screen covers it with its colour.
fn cut_covered_edges(scene: vello::Scene, mask: [f32; 4], w: u32, h: u32) -> vello::Scene {
    use vello::kurbo::{Affine, Rect};
    let [left, top, right, bottom] = mask.map(|edge| f64::from(edge.max(0.0)));
    if left + top + right + bottom <= 0.0 {
        return scene;
    }
    let (x, y, fitted, _) = sp_render::fit_rect(w, h);
    let (slide_w, slide_h) = (f64::from(sp_model::SLIDE_WIDTH), f64::from(sp_model::SLIDE_HEIGHT));
    let to_target = Affine::translate((x, y)) * Affine::scale(fitted / slide_w);
    let mut cut = vello::Scene::new();
    cut.push_clip_layer(
        vello::peniko::Fill::NonZero,
        to_target,
        &Rect::new(left, top, slide_w - right, slide_h - bottom),
    );
    cut.append(&scene, None);
    cut.pop_layer();
    cut
}

/// A theme without its background colour and picture, for a see-through screen. Those made
/// are kept in `kept` with the theme each was made from, so a theme is copied once rather than
/// at every drawing (a few are kept: a theme changed is a new one); one with neither is itself.
fn plain_theme(kept: &mut Vec<(Arc<Theme>, Arc<Theme>)>, theme: &Arc<Theme>) -> Arc<Theme> {
    if theme.background.is_none() && theme.background_picture.is_none() {
        return theme.clone();
    }
    if let Some((_, plain)) = kept.iter().find(|(of, _)| Arc::ptr_eq(of, theme)) {
        return plain.clone();
    }
    let plain = Arc::new(Theme { background: None, background_picture: None, ..(**theme).clone() });
    if kept.len() >= 8 {
        kept.remove(0);
    }
    kept.push((theme.clone(), plain.clone()));
    plain
}

/// What a see-through screen draws of what a screen shows. It leaves out what would cover the
/// whole picture and hide the camera a switcher lays it over: the media behind the words, and
/// the background colour and picture of the slides' theme and of the announcements' (`plain`
/// gives a theme without them). Everything placed on a slide stays: the words, the layout's and
/// the slide's own shapes and pictures (a lower third's bar, say), the props, the message, the
/// copyright line, and the announcements' words. (A graphic meant to go over the camera belongs
/// in a prop, which can hold a picture or a video with clear parts.)
fn see_through_content(content: Content, mut plain: impl FnMut(&Arc<Theme>) -> Arc<Theme>) -> Content {
    let (_media, slide, overlay, theme, announcement) = content;
    let announcement = announcement.map(|shown| Announcement { theme: plain(&shown.theme), ..shown });
    (None, slide, overlay, plain(&theme), announcement)
}

/// Shows a painted picture in a screen's window: through a solid pass when `solid` asks (a
/// see-through picture, which a window shows solid, or a key), else as it is. True when it
/// reached the window; false when the window could not take it this time (its surface is set
/// up again, it is hidden, or its next picture has not come), and the next picture tries
/// again. `until` is how long this pass may wait for its windows' next pictures while NDI or
/// DeckLink are drawn for too (`Output::windows_until`).
fn show_in_window(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut Target,
    picture: &wgpu::TextureView,
    solid: Option<Solid>,
    held: &mut Held,
    until: Option<Instant>,
) -> bool {
    let shown = show_in_window_timed(device, queue, target, picture, solid, held, until);
    held.window_shown(shown);
    shown
}

/// [`show_in_window`]'s work, each step timed for the log.
fn show_in_window_timed(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut Target,
    picture: &wgpu::TextureView,
    solid: Option<Solid>,
    held: &mut Held,
    until: Option<Instant>,
) -> bool {
    let window = For::Window(target.id);
    // A window whose surface isn't set up is set up first, or skipped this frame. One resized
    // while its next picture was still to come is set up once that has come: the picture is
    // of the old size, and is let go.
    if !target.configured {
        if target.next.waiting() {
            let acquiring = Instant::now();
            let came = target.next.take(next_picture::wait_for(true, until, acquiring));
            held.since(Part::Acquire, window, acquiring);
            if matches!(came, Next::NotYet) {
                return false;
            }
        }
        let setting_up = Instant::now();
        target.configured = configure(device, target.next.surface(), &target.config);
        let pausing = held.since(Part::Setup, window, setting_up);
        if !target.configured {
            std::thread::sleep(Duration::from_millis(8));
            held.since(Part::Pause, window, pausing);
            return false;
        }
    }
    if let Some(shown) = solid
        && target.pass.as_ref().is_none_or(|pass| pass.shown() != shown || pass.format() != target.config.format)
    {
        let setting_up = Instant::now();
        target.pass = Some(SolidPass::new(device, target.config.format, shown));
        held.since(Part::Setup, window, setting_up);
    }
    // A lost or outdated surface is set up again and tried once more; anything else
    // (hidden window, timeout) skips this frame and the next message redraws.
    for attempt in 0..2 {
        // The window's next picture comes from its own thread, waited for a short while at
        // most: one that doesn't come (the window system isn't taking the window's pictures
        // back for the moment) is looked for again, briefly, at the next drawing, and the
        // window keeps the picture it shows meanwhile.
        let acquiring = Instant::now();
        let came = target.next.take(next_picture::wait_for(target.next.waiting(), until, acquiring));
        let after = held.since(Part::Acquire, window, acquiring);
        let next = match came {
            Next::Came(next) => {
                // Said in the log, so a window that stood still says for how long. (One passed
                // over for less than a display keeping up could take, while NDI or DeckLink
                // were drawn for, says nothing: that is every picture of a 30 Hz display.)
                if let Some(since) = target.passed_over.take()
                    && (std::mem::take(&mut target.passed_over_said) || since.elapsed() >= next_picture::ASKED_WAIT)
                {
                    crate::applog::event(
                        "output",
                        &format!(
                            "The window of \"{}\" was given its next picture after {:.0} ms.",
                            target.name,
                            since.elapsed().as_secs_f64() * 1000.0
                        ),
                    );
                }
                next
            }
            Next::NotYet => {
                let since = *target.passed_over.get_or_insert(acquiring);
                if !target.passed_over_said && since.elapsed() >= next_picture::ASKED_WAIT {
                    target.passed_over_said = true;
                    crate::applog::event(
                        "output",
                        &format!(
                            "The window of \"{}\" was not given its next picture by the system within {} ms. \
                             The other screens go on; it keeps the picture it shows until the next comes.",
                            target.name,
                            next_picture::ASKED_WAIT.as_millis()
                        ),
                    );
                }
                return false;
            }
        };
        let Some(next) = next else {
            target.configured = false;
            return false;
        };
        match next {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                match (solid, &target.pass) {
                    (Some(_), Some(pass)) => pass.draw(device, &mut encoder, picture, &surface_view),
                    _ => target.blitter.copy(device, &mut encoder, picture, &surface_view),
                }
                // The X over it, in this window's picture only: never in the texture the
                // picture came from, which keys, stage screens and fades are made from.
                if let Some(drawn) = target.close {
                    let size = (target.config.width, target.config.height);
                    let square = crate::close_button::square(size.0, drawn.scale);
                    target.close_mark.draw(queue, &mut encoder, &surface_view, size, square, &drawn.look());
                }
                queue.submit([encoder.finish()]);
                let presenting = held.since(Part::Blit, window, after);
                queue.present(frame);
                held.since(Part::Present, window, presenting);
                // Said once each time the X comes, so a test of it can see it reached the window.
                if target.close_shown.is_none() && target.close.is_some() {
                    crate::applog::event(
                        "output",
                        &format!("The window of \"{}\" shows its close button.", target.name),
                    );
                }
                target.close_shown = target.close;
                return true;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                target.configured = configure(device, target.next.surface(), &target.config);
                held.since(Part::Setup, window, after);
                if !target.configured {
                    return false;
                }
            }
            // A hidden window shows nothing; wait a moment so a fade does not spin.
            _ => {
                std::thread::sleep(Duration::from_millis(8));
                held.since(Part::Pause, window, after);
                return false;
            }
        }
    }
    false
}

/// How a see-through picture of this alpha is shown in a window: solid, its colours straight
/// or premultiplied.
fn solid_fill(alpha: crate::screens::Alpha) -> Solid {
    match alpha {
        crate::screens::Alpha::Straight => Solid::Straight,
        crate::screens::Alpha::Premultiplied => Solid::Premultiplied,
    }
}

/// How a picture of a screen this see-through (or solid) is sent over NDI.
fn ndi_sent(see_through: Option<crate::screens::Alpha>) -> sp_ndi::Sent {
    match see_through {
        None => sp_ndi::Sent::Opaque,
        Some(crate::screens::Alpha::Straight) => sp_ndi::Sent::Straight,
        Some(crate::screens::Alpha::Premultiplied) => sp_ndi::Sent::Premultiplied,
    }
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

    #[test]
    fn a_screen_switched_off_sends_its_background_only() {
        // What a screen switched off on NDI or a DeckLink output is drawn with: what every
        // screen shows while its kind is switched off, so black (clear, for a see-through one).
        let theme = Arc::new(Theme::builtin());
        let (media, slide, over, drawn_with, announcement) = switched_off(None, &theme);
        assert!(media.is_none() && slide.is_none() && over.is_none() && announcement.is_none());
        assert!(Arc::ptr_eq(&drawn_with, &theme));
        let layout = Arc::new(Theme::builtin());
        let (.., stage_with, _) = switched_off(Some(&layout), &theme);
        assert!(Arc::ptr_eq(&stage_with, &layout), "a stage screen keeps its layout, with no words");
    }

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
        NdiSeen { id: Uuid::new_v4(), wanted, alone, key_of: None, unwatched: !wanted, stage: false, arrivals: 0 }
    }

    /// Who is sent each picture as it is, picture by picture.
    fn fills(groups: &[NdiGroup]) -> Vec<Vec<Uuid>> {
        groups.iter().map(|group| group.fills.clone()).collect()
    }

    #[test]
    fn an_ndi_key_rides_with_its_fills_picture_whoever_is_watching() {
        let (fill, other) = (seen(true, true), seen(true, false));
        let key = NdiSeen { key_of: Some(fill.id), ..seen(true, true) };
        // One picture for the fill, sent as it is to the fill and as a key to the key; the
        // others share theirs. The key is never drawn on its own.
        let groups = ndi_groups(&[key, other, fill]);
        assert_eq!(
            groups,
            vec![
                NdiGroup { of: fill.id, fills: vec![fill.id], keys: vec![key.id] },
                NdiGroup { of: other.id, fills: vec![other.id], keys: vec![] },
            ]
        );
        // Nobody watching the fill: its picture is still drawn, for the key alone.
        let unwatched = NdiSeen { wanted: false, unwatched: true, ..fill };
        assert_eq!(ndi_groups(&[unwatched, key]), vec![NdiGroup { of: fill.id, fills: vec![], keys: vec![key.id] }]);
        // Nobody watching the key: the fill goes alone; nobody watching either: nothing.
        let unwatched_key = NdiSeen { wanted: false, unwatched: true, ..key };
        assert_eq!(fills(&ndi_groups(&[fill, unwatched_key])), vec![vec![fill.id]]);
        assert!(ndi_groups(&[unwatched, unwatched_key]).is_empty());
        // A key whose fill is not on NDI has its own picture (black, `scene_for` draws it).
        let lone_key = NdiSeen { key_of: None, ..key };
        assert_eq!(ndi_groups(&[lone_key]), vec![NdiGroup { of: key.id, fills: vec![key.id], keys: vec![] }]);
    }

    #[test]
    fn a_see_through_screen_leaves_out_the_media_and_the_themes_backgrounds_and_keeps_the_rest() {
        let mut theme = Theme::builtin();
        theme.background = Some(sp_model::Rgba::rgb(0, 0, 90));
        theme.background_picture = Some("hills.jpg".to_owned());
        let theme = Arc::new(theme);
        let mut own = Theme::builtin();
        own.background = Some(sp_model::Rgba::rgb(90, 0, 0));
        let own = Arc::new(own);
        let words = Arc::new(Slide::new(vec![sp_model::Object::text(sp_model::Role::primary(), "Light rises")]));
        let message = Arc::new(Slide::new(vec![sp_model::Object::text(sp_model::Role::primary(), "Welcome")]));
        let media = Picture::from_rgba(1, 1, vec![9, 9, 9, 255]);
        let shown = Announcement { slide: words.clone(), theme: own.clone(), video: None, on_slide_screens: true };
        let content: Content = (media, Some(words.clone()), Some(message.clone()), theme.clone(), Some(shown));
        let mut kept = Vec::new();
        let (media, slide, over, plain, announcement) = see_through_content(content, |t| plain_theme(&mut kept, t));
        assert!(media.is_none(), "the media behind the words would hide the camera");
        assert!(plain.background.is_none() && plain.background_picture.is_none());
        assert_eq!(plain.templates, theme.templates, "the theme lays the words out as it did");
        let announced = announcement.expect("the announcements stay");
        assert!(announced.theme.background.is_none(), "without the background that covers the screen");
        assert!(Arc::ptr_eq(&announced.slide, &words));
        assert!(slide.is_some_and(|slide| Arc::ptr_eq(&slide, &words)), "the words stay");
        assert!(over.is_some_and(|over| Arc::ptr_eq(&over, &message)), "the props and the message stay");
        // Each theme is copied once and kept; one with no background is used as it is.
        assert_eq!(kept.len(), 2);
        assert!(Arc::ptr_eq(&plain_theme(&mut kept, &theme), &plain));
        let bare = Arc::new(Theme::builtin());
        assert!(Arc::ptr_eq(&plain_theme(&mut kept, &bare), &bare));
        assert_eq!(kept.len(), 2);
        for _ in 0..20 {
            let mut changed = Theme::builtin();
            changed.background = Some(sp_model::Rgba::BLACK);
            plain_theme(&mut kept, &Arc::new(changed));
        }
        assert!(kept.len() <= 8, "only a few are kept");
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
        assert_eq!(fills(&ndi_groups(&[one, lower, two])), vec![vec![lower.id], vec![one.id, two.id]]);
        // A source nobody is watching is in no picture's list...
        let unwatched = seen(false, false);
        assert_eq!(fills(&ndi_groups(&[one, unwatched])), vec![vec![one.id]]);
        // ...and a picture nobody is watching is not drawn at all: not the shared one, not a
        // source's own.
        assert_eq!(fills(&ndi_groups(&[unwatched, seen(false, false), lower])), vec![vec![lower.id]]);
        assert_eq!(fills(&ndi_groups(&[one, seen(false, true)])), vec![vec![one.id]]);
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
        let english = crate::i18n::Language::English;
        let words =
            |receivers| crate::i18n::with_language(english, || NdiStatus::sending_in_words("Sanctuary", receivers));
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
        let ms = Duration::from_millis;
        assert_eq!(held_picture("1000"), Some((ms(1000), Duration::ZERO)));
        assert_eq!(held_picture(" 1000 @ 90 "), Some((ms(1000), Duration::from_secs(90))));
        for bad in ["", "x", "-5", "1000@", "1000@-1", "1000@nan", "@3", "1.5@3"] {
            assert_eq!(held_picture(bad), None, "{bad:?}");
        }
        assert_eq!(slow_present("150", "Screen 1"), Some(ms(150)));
        assert_eq!(slow_present(" 150 @ screen 1 ", "Screen 1"), Some(ms(150)));
        assert_eq!(slow_present("150@Stage", "Screen 1"), None, "another screen's window");
        for bad in ["", "0", "x", "-5", "1.5", "0@Screen 1", "@Screen 1"] {
            assert_eq!(slow_present(bad, "Screen 1"), None, "{bad:?}");
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

    #[test]
    fn a_new_video_picture_changes_only_the_screens_that_show_the_video() {
        use crate::screens::Look;
        let behind = Playing { behind: true, in_boxes: false, announced: false };
        let nothing = Playing { behind: false, in_boxes: false, announced: false };
        let usual = Look::default();
        assert!(changes_with_video(&usual, false, behind), "a screen with every layer shows the video behind");
        let words_only = Look { media: false, ..Look::default() };
        assert!(!changes_with_video(&words_only, false, behind), "one without the media layer does not");
        assert!(!changes_with_video(&usual, true, behind), "a see-through one leaves the media out");
        assert!(!changes_with_video(&usual, false, nothing), "no video, no change");
        // A video in a box on the slide is shown with the slide, whatever the media layer.
        let boxed = Playing { in_boxes: true, ..nothing };
        assert!(changes_with_video(&words_only, false, boxed) && changes_with_video(&usual, true, boxed));
        // The Announcements layer's video, for a screen that shows it.
        let announced = Playing { announced: true, ..nothing };
        assert!(changes_with_video(&words_only, false, announced));
        assert!(!changes_with_video(&Look { announcements: false, ..words_only }, false, announced));
    }

    #[test]
    fn a_drawing_leaves_out_ndi_sources_only_when_all_have_their_pictures_and_none_is_owed() {
        let one = seen(true, false);
        let drawn = [(one.id, one.arrivals)];
        assert!(ndi_settled(&[one], &drawn, true));
        // Something owed, put off or undrawn: drawn whole.
        assert!(!ndi_settled(&[one], &drawn, false));
        // A receiver that came since the last drawing has not been drawn for.
        let back = NdiSeen { arrivals: one.arrivals + 1, ..one };
        assert!(!ndi_settled(&[back], &drawn, true));
        // A source never drawn for (one just started) is drawn for first.
        assert!(!ndi_settled(&[one], &[], true));
        // One nobody is watching has nothing drawn for it, so it holds nothing back.
        assert!(ndi_settled(&[seen(false, false)], &[], true));
    }

    #[test]
    fn the_time_on_the_stage_screens_draws_only_the_stage_screens() {
        // (its picture moves, it is a stage screen)
        let (main, words_only, stage) = ((true, false), (false, false), (false, true));
        // The stage screens' clock alone: no audience screen, window or NDI source, is touched.
        assert!(Which::Stage.takes(stage.0, stage.1));
        assert!(!Which::Stage.takes(main.0, main.1) && !Which::Stage.takes(words_only.0, words_only.1));
        // With a new picture of a playing video: what shows the video, and the stage screens.
        assert!(Which::MovingAndStage.takes(main.0, main.1) && Which::MovingAndStage.takes(stage.0, stage.1));
        assert!(!Which::MovingAndStage.takes(words_only.0, words_only.1));
        // A video's new picture alone leaves a stage screen that does not show it as it is.
        assert!(Which::Moving.takes(main.0, main.1) && !Which::Moving.takes(stage.0, stage.1));
        assert!(Which::All.takes(words_only.0, words_only.1));
        // Only the stage screens' own sources are told the time may have changed.
        assert!(NdiChange::Stage.touches(true) && !NdiChange::Stage.touches(false));
    }

    #[test]
    fn a_second_beginning_while_a_video_plays_draws_every_screen_only_when_an_audience_screen_shows_the_time() {
        // Nothing but the video moves: only what shows it.
        assert_eq!(video_drawing(true, false, false), Which::Moving);
        assert_eq!(video_drawing(true, false, true), Which::Moving);
        // A second begins with only the stage screens showing the time: they are drawn with it,
        // and the slides kept over the video stay.
        assert_eq!(video_drawing(true, true, false), Which::MovingAndStage);
        // A message or linked text with the time on the audience screens: everything.
        assert_eq!(video_drawing(true, true, true), Which::All);
        // A fade, a build, scrolling words: everything, whatever the time.
        assert_eq!(video_drawing(false, false, false), Which::All);
        assert_eq!(video_drawing(false, true, false), Which::All);
    }

    #[test]
    fn a_fade_gives_ndi_a_picture_for_each_picture_of_a_video_or_at_ndis_own_pace() {
        let ms = Duration::from_millis;
        // A video's new picture is one for NDI, due or not.
        assert!(fade_owes_ndi(true, Some(ms(0)), false));
        // Between two pictures of a video that is giving them, a step of the windows' fade is
        // not: NDI would be handed two in one picture's time and pass one over.
        assert!(!fade_owes_ndi(false, Some(ms(16)), true));
        assert!(!fade_owes_ndi(false, Some(ms(40)), true));
        // Nothing but the fade moves (no video, or one that stopped): one each picture's time.
        assert!(fade_owes_ndi(false, None, true));
        assert!(!fade_owes_ndi(false, None, false));
        assert!(fade_owes_ndi(false, Some(ms(500)), true));
        assert!(!fade_owes_ndi(false, Some(ms(500)), false));
    }

    #[test]
    fn a_fades_pictures_for_ndi_keep_ndis_pace_whatever_the_windows_steps_are() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let frame = sp_ndi::Rate::R29_97.frame();
        // The first is due a picture's time after it is drawn.
        assert_eq!(next_ndi_due(None, start, frame), start + frame);
        // Steps every 8 ms (a covered window) or 16.7 ms: the one drawn a little after it was
        // due does not push the next back, so 29.97 a second go out, not fewer.
        let due = start + frame;
        assert_eq!(next_ndi_due(Some(due), due + ms(7), frame), due + frame);
        // Fallen a picture or more behind (the thread was held): from now, not two at once.
        assert_eq!(next_ndi_due(Some(due), due + ms(50), frame), due + ms(50) + frame);
        // A source that sends 59.94 a second is given the fade twice as often.
        let fast = sp_ndi::Rate::R59_94.frame();
        assert_eq!(next_ndi_due(Some(start + fast), start + fast + ms(3), fast), start + 2 * fast);
    }

    #[test]
    fn a_windows_picture_is_ndis_only_when_both_show_the_same() {
        let (plain, other_plain, own) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let own_way = |id: Uuid| id == own;
        // The screen's own window, however it shows the show.
        assert!(same_picture(own, own, own_way) && same_picture(plain, plain, own_way));
        // Two screens that show the show as it is.
        assert!(same_picture(plain, other_plain, own_way));
        // One with a look, a theme, a stage layout or its name on it shows another picture.
        assert!(!same_picture(plain, own, own_way) && !same_picture(own, other_plain, own_way));
    }

    /// How long the screens' thread takes over a verse change at 3840 × 2160 with a large
    /// picture behind the verse, drawn as a dissolve is drawn on a solid screen: the picture the
    /// fade starts from copied from the window's (`cache_copy`), the one it ends at painted once
    /// into a texture of its own (`cache_whole`), and the last step shown from that texture; and,
    /// for comparison, the starting picture and the last one painted again, as they were before. Each step is timed
    /// until the GPU has finished it, with the painter's wait for the GPU and its runs. Prints the
    /// figures. Run by hand:
    /// `cargo test --release -p sp-app verse_over_a_picture_at_4k -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a GPU, and is for measuring"]
    fn verse_over_a_picture_at_4k_drawing_time() {
        use sp_render::vello::util::RenderContext;
        use sp_render::{Fonts, Layers, Painter, build_layers, target_texture, wgpu};
        let (w, h) = (3840, 2160);
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let blend = sp_render::FadeBlend::new(device);
        let mut fonts = Fonts::new();
        let target = target_texture(device, w, h);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        // A photograph as the screens load one for a 4K screen: 3840 pixels on its longer side.
        let (pw, ph) = (3840, 2560);
        let pixels: Vec<u8> =
            (0..pw * ph).flat_map(|i| [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255]).collect();
        let photo = sp_render::Picture::from_rgba(pw, ph, pixels).expect("a picture");
        let mut shelf = crate::bible::Shelf::shipped();
        let kjv = shelf.get(shelf.position("kjv").expect("the KJV")).expect("read");
        let rows = [crate::bible::RowChoice {
            bible: kjv,
            verse: Some(sp_model::Role::primary()),
            reference: Some(sp_model::Role::reference()),
        }];
        let options = crate::bible::BibleOptions::default();
        let passage = crate::bible::build_passage("John 3:1-36", &rows, &options, None).expect("John 3");
        let theme = sp_model::Theme::builtin_bible();
        let mut scene_of = |at: usize, picture: bool| {
            let layers = Layers {
                media: picture.then_some(&photo),
                media_fit: sp_model::ImageFit::Fill,
                slide: passage.slide(at).map(|(_, slide)| slide),
                ..Default::default()
            };
            build_layers(&mut fonts, &layers, &theme, w, h).scene
        };
        let step = |painter: &mut Painter, name: &str, run: &mut dyn FnMut(&mut Painter)| {
            let (waited, runs) = (painter.gpu_waited(), painter.runs());
            let started = std::time::Instant::now();
            run(painter);
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            println!(
                "  {name:<36} {:>6.2} ms (GPU wait {:>5.2} ms, {} runs)",
                started.elapsed().as_secs_f64() * 1000.0,
                (painter.gpu_waited() - waited).as_secs_f64() * 1000.0,
                painter.runs() - runs
            );
        };
        for picture in [true, false] {
            println!("{} picture behind:", if picture { "A" } else { "No" });
            let mut shown = scene_of(0, picture);
            step(&mut painter, "first verse (its picture sent)", &mut |p| {
                p.paint(device, queue, &shown, &view, w, h).expect("paint");
            });
            for at in 1..5 {
                let next = scene_of(at, picture);
                let (mut from, mut to) = (None, None);
                step(&mut painter, "fade starts: from, copied", &mut |p| {
                    from = Some(p.cache_copy(device, queue, &target).expect("from"));
                });
                step(&mut painter, "fade starts: from, painted (before)", &mut |p| {
                    drop(p.cache_whole(device, queue, &shown, w, h).expect("from"));
                });
                step(&mut painter, "fade starts: to, into a texture", &mut |p| {
                    to = Some(p.cache_whole(device, queue, &next, w, h).expect("to"));
                });
                let (from, to) = (from.expect("from"), to.expect("to"));
                let weights = sp_render::blend_weights(sp_model::TransitionKind::Dissolve, 1.0).expect("a sum");
                step(&mut painter, "fade ends: from the new texture", &mut |_| {
                    assert!(blend.draw(device, queue, &from, &to, weights, &target));
                });
                step(&mut painter, "fade ends: painted again (before)", &mut |p| {
                    p.paint(device, queue, &next, &view, w, h).expect("paint");
                });
                shown = next;
            }
        }
    }
}
