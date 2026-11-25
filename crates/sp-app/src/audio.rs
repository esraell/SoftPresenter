//! Audio: playlists of tracks to play, such as walk-in music, and the player that plays them
//! (crossfading from one to the next if asked, round and round with Loop). The playlists are
//! kept in `audio.json`; the files stay where they are.

mod deck;
mod resample;
mod ring;
mod stream;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sp_model::Uuid;

use crate::applog;
use crate::i18n::{t, tf, tg, tgf};

/// The file endings of audio the player can play, as the file chooser lists them: those the app
/// decodes itself, and those FFmpeg decodes (Opus, WMA, CAF, AC-3, MP2, audiobooks), with the
/// videos whose sound can be played. `.m4p` (bought or kept from Apple's shops) is taken too,
/// so that one that is locked says so.
pub const AUDIO_ENDINGS: [&str; 19] = [
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "opus", "aif", "aiff", "aifc", "caf", "wma", "ac3", "mp2", "m4b",
    "m4p", "mp4", "mov",
];

/// True for a file whose name says it is a sound.
pub fn is_audio(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| AUDIO_ENDINGS.iter().any(|ending| ext.eq_ignore_ascii_case(ending)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
}

impl Track {
    pub fn of(path: PathBuf) -> Track {
        let name = path.file_stem().map_or_else(
            || tg("tools.audio.track_name", "Track").to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        Track { id: Uuid::new_v4(), name, path }
    }
}

/// A named list of tracks, such as "Walk-in" or "Offering".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: Uuid,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn new(name: String) -> Playlist {
        Playlist { id: Uuid::new_v4(), name, tracks: Vec::new() }
    }
}

/// Every audio playlist, the one shown, and how tracks follow one another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub current: usize,
    /// When a track ends, a random other track of its playlist plays next.
    #[serde(default)]
    pub shuffle: bool,
    /// When a track ends, the next one plays.
    #[serde(default)]
    pub through: bool,
    /// After the last track, the first plays again (with shuffle, a fresh order); without
    /// Play through, the track playing plays again. A file from before it had this takes it from
    /// Play through, which went back to the first track after the last then.
    #[serde(default, rename = "loop")]
    pub looping: bool,
    /// Seconds the next track fades in over the end of the one before; 0 for none.
    #[serde(default)]
    pub crossfade: f32,
    /// The track playing, when it has been taken out of its playlist: it plays on to its end.
    /// Not kept between runs.
    #[serde(skip)]
    pub leaving: Option<Leaving>,
    /// The tracks played this time round, for shuffle. Not kept between runs.
    #[serde(skip)]
    pub round: Round,
}

/// The tracks of one playlist played since its shuffled order began, so that each plays once
/// before any plays again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Round {
    pub playlist: Option<Uuid>,
    pub played: Vec<Uuid>,
}

/// A track taken out of its playlist (or with its playlist) while it played. It plays on to its
/// end; then the playlist goes on as it now stands, from the place it had.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaving {
    pub track: Track,
    /// The playlist it was in, and its place there, kept up as tracks above it are taken out.
    pub playlist: Uuid,
    pub at: usize,
    /// The track that came after it (none after the last), which comes next wherever it has
    /// moved to; while it is in the playlist, its place stands for the leaving track's.
    pub next: Option<Uuid>,
}

impl Default for Library {
    fn default() -> Library {
        Library {
            playlists: vec![Playlist::new(first_name())],
            current: 0,
            shuffle: false,
            through: false,
            looping: false,
            crossfade: 0.0,
            leaving: None,
            round: Round::default(),
        }
    }
}

impl Library {
    /// Reads the file's words. A file from before playlists is a plain list of tracks.
    pub fn from_json(text: &str) -> Option<Library> {
        if let Ok(tracks) = serde_json::from_str::<Vec<Track>>(text) {
            return Some(Library {
                playlists: vec![Playlist { tracks, ..Playlist::new(first_name()) }],
                ..Library::default()
            });
        }
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let has_loop = value.get("loop").is_some();
        let mut library: Library = serde_json::from_value(value).ok()?;
        if !has_loop {
            library.looping = library.through;
        }
        if library.playlists.is_empty() {
            library.playlists.push(Playlist::new(first_name()));
        }
        library.current = library.current.min(library.playlists.len() - 1);
        Some(library)
    }

    /// The playlist shown. There is always one.
    pub fn shown(&self) -> &Playlist {
        &self.playlists[self.current.min(self.playlists.len() - 1)]
    }

    pub fn shown_mut(&mut self) -> &mut Playlist {
        let at = self.current.min(self.playlists.len() - 1);
        &mut self.playlists[at]
    }

    /// A track in any playlist, or the one playing that has been taken out of its playlist.
    pub fn track(&self, id: Uuid) -> Option<&Track> {
        self.playlists
            .iter()
            .flat_map(|playlist| playlist.tracks.iter())
            .chain(self.leaving.as_ref().map(|leaving| &leaving.track))
            .find(|track| track.id == id)
    }

    /// The playlist a track is in and its place there. For the one playing that has been taken
    /// out, the playlist it was in and the place it had, where the track after it now is.
    pub fn place_of(&self, id: Uuid) -> Option<(&Playlist, usize)> {
        let listed = self
            .playlists
            .iter()
            .find_map(|playlist| playlist.tracks.iter().position(|track| track.id == id).map(|at| (playlist, at)));
        listed.or_else(|| {
            let leaving = self.leaving.as_ref().filter(|leaving| leaving.track.id == id)?;
            let playlist = self.playlists.iter().find(|playlist| playlist.id == leaving.playlist)?;
            let next = leaving.next.and_then(|next| playlist.tracks.iter().position(|track| track.id == next));
            Some((playlist, next.unwrap_or(leaving.at)))
        })
    }

    /// The track to play when `id` has played to its end by itself, leaving out those in
    /// `failed` (they could not be played just now). With Play through, the next in its
    /// playlist ([`following`](Self::following)), and after the last the first again only with
    /// Loop; without Play through, the same track again with Loop, else nothing. `pick` chooses
    /// a number below the one given, for shuffle.
    pub fn after_end(&self, id: Uuid, pick: impl FnOnce(usize) -> usize, failed: &[Uuid]) -> Option<&Track> {
        match (self.through, self.looping) {
            (true, wrap) => self.following(id, pick, wrap, failed),
            // The track goes round only while it is in a playlist: one taken out ends there.
            (false, true) => self
                .playlists
                .iter()
                .flat_map(|playlist| playlist.tracks.iter())
                .find(|track| track.id == id && !failed.contains(&track.id)),
            (false, false) => None,
        }
    }

    /// The track the Next key goes to from `id`: the next, round to the first after the last,
    /// whatever Loop says (with shuffle, the next of this time round's order).
    pub fn next_by_hand(&self, id: Uuid, pick: impl FnOnce(usize) -> usize) -> Option<&Track> {
        self.following(id, pick, true, &[])
    }

    /// The track after `id` in its playlist, leaving out `failed`: the next, or with shuffle one
    /// not yet played this time round. After the last (with shuffle, once every track has
    /// played), round to the first again (a fresh order, never the track that just played
    /// unless it is the only one left) only when `wrap` says so. After the one playing that has
    /// been taken out, the playlist goes on as it now stands: from the track now at its place.
    fn following(&self, id: Uuid, pick: impl FnOnce(usize) -> usize, wrap: bool, failed: &[Uuid]) -> Option<&Track> {
        let listed = self.playlists.iter().any(|playlist| playlist.tracks.iter().any(|track| track.id == id));
        let (playlist, at) = self.place_of(id)?;
        let tracks = &playlist.tracks;
        if self.shuffle {
            let round: &[Uuid] = if self.round.playlist == Some(playlist.id) { &self.round.played } else { &[] };
            let open = |track: &&Track| track.id != id && !failed.contains(&track.id);
            let fresh: Vec<&Track> = tracks.iter().filter(open).filter(|track| !round.contains(&track.id)).collect();
            if !fresh.is_empty() {
                return Some(fresh[pick(fresh.len()).min(fresh.len() - 1)]);
            }
            if !wrap {
                return None;
            }
            let again: Vec<&Track> = tracks.iter().filter(open).collect();
            if !again.is_empty() {
                return Some(again[pick(again.len()).min(again.len() - 1)]);
            }
            return tracks.iter().find(|track| track.id == id && !failed.contains(&track.id));
        }
        // In order: from the next place (from its own, for one taken out), round past the end
        // only with `wrap`; the track itself comes last, once all the others have failed.
        let from = if listed { at + 1 } else { at };
        let count = tracks.len();
        for step in 0..count {
            let place = from + step;
            if place >= count && !wrap {
                return None;
            }
            let track = &tracks[place % count];
            if !failed.contains(&track.id) {
                return Some(track);
            }
        }
        None
    }

    /// Says that a track has begun to play, for shuffle's order: `fresh` when nothing played
    /// just before, which begins a new order. A track that has played this time round (a new
    /// time round, or one chosen again) begins a new one too.
    pub fn started(&mut self, id: Uuid, fresh: bool) {
        let Some(playlist) = self.place_of(id).map(|(playlist, _)| playlist.id) else { return };
        if fresh || self.round.playlist != Some(playlist) || self.round.played.contains(&id) {
            self.round = Round { playlist: Some(playlist), played: Vec::new() };
        }
        self.round.played.push(id);
    }

    /// Takes a track out of the playlist shown. The one playing (`playing`) plays on to its end,
    /// and the playlist then goes on from its place (see [`after`](Self::after)).
    pub fn remove_track(&mut self, id: Uuid, playing: Option<Uuid>) {
        let shown = self.shown_mut();
        let Some(at) = shown.tracks.iter().position(|track| track.id == id) else { return };
        let track = shown.tracks.remove(at);
        let playlist = shown.id;
        let next = shown.tracks.get(at).map(|track| track.id);
        if playing == Some(id) {
            self.leaving = Some(Leaving { track, playlist, at, next });
        } else if let Some(leaving) = self.leaving.as_mut().filter(|leaving| leaving.playlist == playlist) {
            // A track above the place of the one leaving: that place moves up with the rest.
            if at < leaving.at {
                leaving.at -= 1;
            }
        }
    }

    /// Deletes an audio playlist (by its place), unless it is the last. A track of it that is
    /// playing plays on to its end, and nothing follows it.
    pub fn remove_playlist(&mut self, at: usize, playing: Option<Uuid>) -> bool {
        if self.playlists.len() <= 1 || at >= self.playlists.len() {
            return false;
        }
        let gone = self.playlists.remove(at);
        if let Some(place) = gone.tracks.iter().position(|track| Some(track.id) == playing) {
            self.leaving =
                Some(Leaving { track: gone.tracks[place].clone(), playlist: gone.id, at: place, next: None });
        }
        true
    }

    /// Lets go of the track taken out while it played once the player no longer plays it.
    pub fn forget_leaving(&mut self, playing: Option<Uuid>) {
        if self.leaving.as_ref().is_some_and(|leaving| Some(leaving.track.id) != playing) {
            self.leaving = None;
        }
    }

    /// A name no audio playlist has: "New Audio Playlist", "New Audio Playlist 2", and so on
    /// ("Playlist" alone is the running order).
    pub fn new_name(&self) -> String {
        let taken = |name: &str| self.playlists.iter().any(|playlist| playlist.name == name);
        std::iter::once(tg("tools.audio.new_name", "New Audio Playlist").to_owned())
            .chain(
                (2..).map(|n| tgf("tools.audio.new_name.numbered", "New Audio Playlist {n}", &[("n", &n.to_string())])),
            )
            .find(|name| !taken(name))
            .unwrap_or_default()
    }
}

/// What the player is doing, for the Audio tab.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Playing {
    pub track: Option<Uuid>,
    pub position: Duration,
    pub length: Option<Duration>,
    pub paused: bool,
}

/// Something that happened to the player since it was last asked.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A track began to be heard: `fresh` when nothing played just before it, `followed` when
    /// it followed another by itself (with Play through or Loop).
    Began { track: Uuid, fresh: bool, followed: bool, said: String },
    /// A track played to its end, and nothing followed it.
    Ended { track: Uuid },
    /// A track could not be played: one asked for (what played goes on), or one lined up to
    /// follow (`lined_up`).
    Failed { track: Uuid, problem: String, lined_up: bool },
}

/// A track the player has asked to play.
struct Asked {
    track: Uuid,
    stream: Arc<stream::Shared>,
    /// Asked for by hand (or a seek), not lined up to follow.
    fresh: bool,
    /// When it was asked for.
    at: Instant,
}

/// The sound output, with the deck playing in it.
struct Out {
    /// Kept open for as long as the app runs, once a track has been played.
    _output: sp_video::sound::Output,
    orders: std::sync::mpsc::SyncSender<deck::Order>,
    returned: std::sync::mpsc::Receiver<Arc<stream::Shared>>,
    controls: Arc<deck::Controls>,
    rate: u32,
}

/// Plays the tracks: each decoded ahead on a thread of its own ([`stream`]), and joined, faded
/// and mixed on the sound card's thread ([`deck`]). The sound output is opened the first time
/// a track is played, so a computer that never plays audio never opens it.
pub struct Player {
    out: Option<Out>,
    volume: f32,
    /// The track being heard.
    now: Option<Asked>,
    /// A track asked to play in its place (or the same track from another place), until it has
    /// its first frames.
    asked: Option<Asked>,
    /// The track lined up to follow the one heard.
    then: Option<Asked>,
    /// A track lined up and then let go, until the deck hands it back: had it begun already
    /// as the order reached the deck, it is the one heard.
    dropped: Option<Asked>,
    /// Every track handed to the deck, kept until it hands it back, so the sound card's thread
    /// never lets go of one.
    held: Vec<stream::Stream>,
    /// The tracks after the one heard that could not be played, this time round.
    failed: Vec<Uuid>,
    /// The track after which nothing could be played: nothing is lined up after it again.
    gave_up: Option<Uuid>,
    /// The last track asked for that could not be played, and why.
    refused: Option<(Uuid, String)>,
    /// The minute's figures for the log.
    minute: Minute,
}

/// What the log is told each minute something plays.
struct Minute {
    began: Instant,
    underruns: u64,
    blocks: u64,
}

impl Default for Player {
    fn default() -> Player {
        Player {
            out: None,
            volume: 1.0,
            now: None,
            asked: None,
            then: None,
            dropped: None,
            held: Vec::new(),
            failed: Vec::new(),
            gave_up: None,
            refused: None,
            minute: Minute { began: Instant::now(), underruns: 0, blocks: 0 },
        }
    }
}

/// The time to line the next track up before the one heard ends, beside the crossfade: long
/// enough for any track to be opened and decoded ahead, FFmpeg's too.
const LEAD: Duration = Duration::from_secs(3);

impl Player {
    /// The sound output, opened the first time it is needed, with the deck in it.
    fn out(&mut self) -> Result<&Out, String> {
        if self.out.is_none() {
            let output = sp_video::sound::Output::open().map_err(|e| {
                tf("tools.audio.no_output", "Could not open the computer's sound output: {error}", &[("error", &e)])
            })?;
            let rate = output.rate();
            let (orders, orders_in) = std::sync::mpsc::sync_channel(32);
            let (returned_out, returned) = std::sync::mpsc::sync_channel(64);
            let controls = Arc::new(deck::Controls::default());
            controls.set_volume(self.volume);
            output.mixer().add(deck::Deck::new(rate, orders_in, returned_out, controls.clone()));
            applog::event(
                "audio",
                &format!("sound output opened: stereo at {rate} Hz, decoded ahead and joined to the frame"),
            );
            self.out = Some(Out { _output: output, orders, returned, controls, rate });
        }
        self.out.as_ref().ok_or_else(|| t("tools.audio.no_output.none", "No sound output.").to_owned())
    }

    /// Starts decoding a track from `from` and hands it to the deck with `order`.
    fn start(
        &mut self,
        track: &Track,
        from: Duration,
        order: impl FnOnce(Arc<stream::Shared>) -> deck::Order,
    ) -> Result<Arc<stream::Shared>, String> {
        let rate = self.out()?.rate;
        let decoding = stream::start(track.path.clone(), track.name.clone(), from, rate);
        let shared = decoding.shared.clone();
        // Never waits: a sound card that has stopped taking sound (taken away, say) leaves the
        // orders unread, and the window must not stop with it.
        let sent = self.out.as_ref().is_some_and(|out| out.orders.try_send(order(shared.clone())).is_ok());
        if !sent {
            return Err(t("tools.audio.output_stuck", "The computer's sound output isn't taking sound.").to_owned());
        }
        self.held.push(decoding);
        Ok(shared)
    }

    /// Plays a track from its start, in place of what plays (which fades out over `fade`, or
    /// a moment so it doesn't click, once this one has its first frames). A track that can't
    /// be played leaves what plays as it was, and says so ([`Event::Failed`]); only a sound
    /// output that can't be opened is an error here.
    pub fn play_fading(&mut self, track: &Track, fade: Duration) -> Result<(), String> {
        self.cancel_then();
        let shared = match self.start(track, Duration::ZERO, |stream| deck::Order::Play { stream, fade }) {
            Ok(shared) => shared,
            Err(problem) => {
                self.refused = Some((track.id, problem.clone()));
                return Err(problem);
            }
        };
        if let Some(out) = &self.out {
            out.controls.set_paused(false);
        }
        self.asked = Some(Asked { track: track.id, stream: shared, fresh: true, at: Instant::now() });
        self.failed.clear();
        self.gave_up = None;
        self.refused = None;
        Ok(())
    }

    /// Lines a track up to follow the one heard: fading in over its last `fade`, or from the
    /// frame after its last.
    pub fn line_up(&mut self, track: &Track, fade: Duration) -> Result<(), String> {
        let shared = self.start(track, Duration::ZERO, |stream| deck::Order::Then { stream, fade })?;
        self.then = Some(Asked { track: track.id, stream: shared, fresh: false, at: Instant::now() });
        Ok(())
    }

    /// Forgets the track lined up to follow.
    pub fn cancel_then(&mut self) {
        if let Some(then) = self.then.take()
            && let Some(out) = &self.out
        {
            let _ = out.orders.try_send(deck::Order::NoThen);
            self.dropped = Some(then);
        }
    }

    /// The track lined up to follow, if one is.
    pub fn lined_up(&self) -> Option<Uuid> {
        self.then.as_ref().map(|then| then.track)
    }

    /// True when the track heard is near enough its end for the next to be lined up (`fade`
    /// before it, and time to decode the next ahead), and none is.
    pub fn wants_next(&self, fade: Duration) -> Option<Uuid> {
        let now = self.now.as_ref().filter(|_| self.then.is_none() && self.asked.is_none())?;
        let near = now.stream.decoded() || now.stream.left().is_some_and(|left| left <= fade + LEAD);
        (near && self.gave_up != Some(now.track)).then_some(now.track)
    }

    /// True while a track is on its way (asked for, or lined up), or the deck has tracks to hand
    /// back: worth looking at again soon, playing or not.
    pub fn busy(&self) -> bool {
        self.asked.is_some() || self.held.len() > usize::from(self.now.is_some()) + usize::from(self.then.is_some())
    }

    /// The tracks after the one heard that could not be played this time round.
    pub fn failed(&self) -> &[Uuid] {
        &self.failed
    }

    /// Nothing could be played after this track: nothing more is lined up after it.
    pub fn give_up(&mut self, after: Uuid) {
        self.gave_up = Some(after);
    }

    /// Whether the track asked for has its first frames (`Some(Ok)`), can't be played
    /// (`Some(Err)`), or is still opening (`None`).
    pub fn outcome(&self, track: Uuid) -> Option<Result<(), String>> {
        let asked = self.asked.as_ref().filter(|asked| asked.track == track);
        let Some(asked) = asked else {
            // Already heard, or gone with its problem said.
            if let Some((_, problem)) = self.refused.as_ref().filter(|(refused, _)| *refused == track) {
                return Some(Err(problem.clone()));
            }
            return self.now.as_ref().filter(|now| now.track == track).map(|_| Ok(()));
        };
        if let Some(problem) = asked.stream.failed() {
            return Some(Err(problem));
        }
        asked.stream.ready().then_some(Ok(()))
    }

    /// Pauses, or goes on from where it was paused.
    pub fn toggle_pause(&mut self) {
        if let Some(out) = &self.out
            && (self.now.is_some() || self.asked.is_some())
        {
            out.controls.set_paused(!out.controls.paused());
        }
    }

    /// Stops, fading out over a moment; whatever was lined up goes too.
    pub fn stop(&mut self) {
        if let Some(out) = &self.out {
            let _ = out.orders.try_send(deck::Order::Stop);
            out.controls.set_paused(false);
        }
        self.now = None;
        self.asked = None;
        self.then = None;
        self.dropped = None;
        self.failed.clear();
    }

    /// Goes to a place in the track, playing or paused: the track is decoded again from there,
    /// and taken up where it is ready, with a moment's fade so it doesn't click.
    pub fn seek(&mut self, track: &Track, to: Duration) {
        let Some(current) = self.asked.as_ref().or(self.now.as_ref()).map(|asked| asked.track) else { return };
        if current != track.id {
            return;
        }
        self.cancel_then();
        if let Ok(shared) = self.start(track, to, |stream| deck::Order::Play { stream, fade: Duration::ZERO }) {
            self.asked = Some(Asked { track: track.id, stream: shared, fresh: false, at: Instant::now() });
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.5);
        if let Some(out) = &self.out {
            out.controls.set_volume(self.volume);
        }
    }

    /// What is heard now: a track asked for and still opening shows already, at its place.
    pub fn playing(&self) -> Playing {
        let paused = self.out.as_ref().is_some_and(|out| out.controls.paused());
        match self.asked.as_ref().or(self.now.as_ref()) {
            Some(asked) => {
                // A place asked for in the track heard: its length is known already.
                let heard = self.now.as_ref().filter(|now| now.track == asked.track);
                let length = asked.stream.length().or_else(|| heard.and_then(|now| now.stream.length()));
                Playing { track: Some(asked.track), position: asked.stream.position(), length, paused }
            }
            None => Playing::default(),
        }
    }

    /// Takes in what the deck has done since last asked: tracks begun, ended, or that could
    /// not be played; lets go of the tracks it has handed back; and tells the log once a minute
    /// how the sound kept up. `name` says a track's name, for the log.
    pub fn tick(&mut self, name: impl Fn(Uuid) -> String) -> Vec<Event> {
        let mut events = Vec::new();
        // Tracks the deck holds no longer: let go of here, where letting go of memory is safe.
        if let Some(out) = &self.out {
            while let Ok(done) = out.returned.try_recv() {
                self.held.retain(|held| !Arc::ptr_eq(&held.shared, &done));
            }
        }
        // A track asked for: begun, or not playable.
        if let Some(asked) = &self.asked {
            if let Some(problem) = asked.stream.failed() {
                self.refused = Some((asked.track, problem.clone()));
                events.push(Event::Failed { track: asked.track, problem, lined_up: false });
                self.asked = None;
            } else if asked.stream.began.load(Ordering::Acquire) {
                let asked = self.asked.take().expect("there is one");
                let seek = self.now.as_ref().is_some_and(|now| now.track == asked.track) && !asked.fresh;
                if seek {
                    // How soon a jump in the track was heard.
                    let ready = asked.stream.ready_after().map_or(0, |ready| ready.as_millis());
                    applog::event(
                        "audio",
                        &format!(
                            "heard from {} in {} after {} ms ({}, ready in {ready} ms)",
                            clock(asked.stream.from),
                            name(asked.track),
                            asked.stream.heard_after().unwrap_or_else(|| asked.at.elapsed()).as_millis(),
                            asked.stream.way().map_or("?", stream::Way::word),
                        ),
                    );
                } else {
                    let fresh = self.now.is_none();
                    events.push(Event::Began {
                        track: asked.track,
                        fresh,
                        followed: false,
                        said: said(&asked.stream, &name(asked.track)),
                    });
                }
                self.now = Some(asked);
            }
        }
        // The track lined up: begun (the one before it has gone), or not playable.
        if let Some(then) = &self.then {
            if let Some(problem) = then.stream.failed() {
                events.push(Event::Failed { track: then.track, problem, lined_up: true });
                self.failed.push(then.track);
                self.then = None;
            } else if then.stream.began.load(Ordering::Acquire) {
                let then = self.then.take().expect("there is one");
                events.push(Event::Began {
                    track: then.track,
                    fresh: false,
                    followed: true,
                    said: said(&then.stream, &name(then.track)),
                });
                self.failed.clear();
                self.now = Some(then);
            }
        }
        // A track let go just as it began in place of the one heard: it is heard now.
        if let Some(dropped) = self.dropped.take() {
            let began = dropped.stream.began.load(Ordering::Acquire);
            let heard_ended = self.now.as_ref().is_none_or(|now| now.stream.ended.load(Ordering::Acquire));
            if began && !dropped.stream.ended.load(Ordering::Acquire) && self.asked.is_none() && heard_ended {
                events.push(Event::Began {
                    track: dropped.track,
                    fresh: false,
                    followed: true,
                    said: said(&dropped.stream, &name(dropped.track)),
                });
                self.now = Some(dropped);
            } else if !dropped.stream.released.load(Ordering::Acquire) {
                self.dropped = Some(dropped);
            }
        }
        // The track heard has ended with nothing after it (one lined up comes in on the frame
        // after its last, so it is waited for).
        if let Some(now) = &self.now
            && now.stream.ended.load(Ordering::Acquire)
            && self.asked.is_none()
            && self.then.is_none()
        {
            events.push(Event::Ended { track: now.track });
            self.now = None;
        }
        self.minute_line(&name);
        events
    }

    /// Once a minute while something plays, a line for the log: how many blocks the sound card
    /// played and in how many the decoding hadn't kept up (0 is right), and the least sound
    /// that was ready ahead.
    fn minute_line(&mut self, name: &impl Fn(Uuid) -> String) {
        if self.minute.began.elapsed() < Duration::from_secs(60) {
            return;
        }
        let Some(out) = &self.out else { return };
        let underruns = out.controls.underruns.load(Ordering::Relaxed);
        let blocks = out.controls.blocks.load(Ordering::Relaxed);
        let lowest = out.controls.lowest.swap(u64::MAX, Ordering::Relaxed);
        let (new_underruns, new_blocks) = (underruns - self.minute.underruns, blocks - self.minute.blocks);
        self.minute = Minute { began: Instant::now(), underruns, blocks };
        if new_blocks == 0 {
            return;
        }
        let played = new_blocks as f64 * deck::BLOCK as f64 / f64::from(out.rate);
        let ahead = if lowest == u64::MAX {
            String::new()
        } else {
            format!("; the least ready ahead {:.2} s", lowest as f64 / f64::from(out.rate))
        };
        let heard = self.now.as_ref().map_or_else(String::new, |now| format!("; playing {}", name(now.track)));
        applog::event(
            "audio",
            &format!(
                "last minute: {new_underruns} underruns in {new_blocks} blocks ({played:.1} s played){ahead}{heard}"
            ),
        );
    }
}

/// How a track is being played, for the log: how it is decoded, from what, and how soon it
/// was ready.
fn said(stream: &stream::Shared, name: &str) -> String {
    let way = stream.way().map_or("?", stream::Way::word);
    let ready = stream.ready_after().map_or_else(String::new, |ready| format!(", ready in {} ms", ready.as_millis()));
    format!("{name} ({way}, from {}, stereo at {} Hz{ready})", clock(stream.from), stream.rate)
}

/// Whether a file can be played, and how (for adding tracks), in words for the operator when
/// it can't.
pub fn check(path: &std::path::Path) -> Result<stream::Way, String> {
    let name = path.file_stem().map_or_else(|| path.display().to_string(), |stem| stem.to_string_lossy().into_owned());
    stream::check(path, &name)
}
/// The name of the audio playlist there always is, when none has been made: the app names it
/// and saves it, in the language set.
pub fn first_name() -> String {
    tg("tools.audio.first_name", "Audio").to_owned()
}

/// Minutes and seconds: "3:07".
pub fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Where `sp audio seek TIME` goes in a track at `position`, of `length` when it is known:
/// `1:23` (or `1:02:03`), `83` or `83.5` seconds, or `+15` and `-15` from where it is (back
/// past the start goes to the start). A place past the end is refused, with the track's length.
pub fn seek_to(typed: &str, position: Duration, length: Option<Duration>) -> Result<Duration, String> {
    let typed = typed.trim();
    let usage = || format!("Not a time: {typed}. For example: 1:23, 83 (seconds), +15 or -15.");
    let (sign, number) = match typed.as_bytes().first() {
        Some(b'+') => (1.0, &typed[1..]),
        Some(b'-') => (-1.0, &typed[1..]),
        _ => (0.0, typed),
    };
    let mut seconds = 0.0f64;
    let parts: Vec<&str> = number.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(usage());
    }
    for (at, part) in parts.iter().enumerate() {
        let value: f64 = part.parse().map_err(|_| usage())?;
        // Only the last part may have a fraction, and the parts after the first count to 60.
        if !value.is_finite() || value < 0.0 || (at + 1 < parts.len() && value.fract() != 0.0) {
            return Err(usage());
        }
        if at > 0 && value >= 60.0 {
            return Err(usage());
        }
        seconds = seconds * 60.0 + value;
    }
    let to = if sign == 0.0 { seconds } else { (position.as_secs_f64() + sign * seconds).max(0.0) };
    let to = Duration::from_secs_f64(to);
    match length {
        Some(length) if to > length => {
            Err(format!("The track is {} long; {} is past its end.", clock(length), clock(to)))
        }
        _ => Ok(to),
    }
}

fn file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("audio.json"))
}

pub fn load() -> Library {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| Library::from_json(&text))
        .unwrap_or_default()
}

pub fn save(library: &Library) -> Result<(), String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            tf(
                "media.no_folder",
                "Could not make the app's folder: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(library).unwrap_or_default()).map_err(|e| {
        tf(
            "tools.audio.not_saved",
            "Could not save the audio list: {error}",
            &[("error", &crate::notices::write_problem(&e))],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_named_by_their_file_and_a_missing_file_says_so() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let track = Track::of(PathBuf::from("/music/Walk In.mp3"));
            assert_eq!(track.name, "Walk In");
            assert_eq!(clock(Duration::from_secs(187)), "3:07");
            // Nothing is played, so no sound output is opened; a missing file fails in words.
            let player = Player::default();
            assert_eq!(player.playing(), Playing::default());
            assert!(player.out.is_none());
            let missing = check(std::path::Path::new("/nowhere/none.mp3"));
            assert!(missing.err().unwrap_or_default().contains("Could not open none"));
        });
    }

    #[test]
    fn an_old_list_loads_as_one_playlist_and_a_file_from_before_loop_keeps_going_round() {
        // The words are checked in English, whatever another test sets.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let one = Track::of(PathBuf::from("/m/One.mp3"));
            let two = Track::of(PathBuf::from("/m/Two.mp3"));
            let old = serde_json::to_string(&vec![one, two]).unwrap();
            let mut library = Library::from_json(&old).unwrap();
            assert_eq!((library.playlists.len(), library.shown().tracks.len()), (1, 2));
            assert!(!library.through && !library.looping);
            library.playlists.push(Playlist::new(library.new_name()));
            assert_eq!(library.playlists[1].name, "New Audio Playlist");
            assert_eq!(Library::from_json(&serde_json::to_string(&library).unwrap()), Some(library.clone()));
            // Saved before Loop: Play through went back to the first track after the last
            // then, so it keeps doing so; and the other way round.
            library.through = true;
            let mut saved: serde_json::Value = serde_json::to_value(&library).unwrap();
            saved.as_object_mut().unwrap().remove("loop");
            assert!(Library::from_json(&saved.to_string()).unwrap().looping);
            saved["through"] = serde_json::json!(false);
            assert!(!Library::from_json(&saved.to_string()).unwrap().looping);
            // Saved with it: as it was saved.
            library.looping = false;
            let saved: serde_json::Value = serde_json::to_value(&library).unwrap();
            assert_eq!(saved["loop"], serde_json::json!(false));
            assert!(!Library::from_json(&saved.to_string()).unwrap().looping);
        });
    }

    /// A playlist of these tracks, with Play through, Loop and Shuffle as given. The same
    /// names make tracks of the same ids.
    fn walk_in(names: &[&str], through: bool, looping: bool, shuffle: bool) -> Library {
        let tracks = names
            .iter()
            .enumerate()
            .map(|(at, name)| Track {
                id: Uuid::from_u128(at as u128 + 1),
                ..Track::of(PathBuf::from(format!("/m/{name}.mp3")))
            })
            .collect();
        Library {
            playlists: vec![Playlist { tracks, ..Playlist::new("Walk-in".to_owned()) }],
            through,
            looping,
            shuffle,
            ..Library::default()
        }
    }

    #[test]
    fn at_the_last_track_loop_goes_round_and_without_it_the_music_stops() {
        let names = ["One", "Two", "Three"];
        for through in [false, true] {
            for looping in [false, true] {
                for shuffle in [false, true] {
                    let case = format!("through {through}, loop {looping}, shuffle {shuffle}");
                    let mut library = walk_in(&names, through, looping, shuffle);
                    let ids: Vec<Uuid> = library.shown().tracks.iter().map(|track| track.id).collect();
                    let (first, last) = (ids[0], ids[2]);
                    // With shuffle, the other two have played this time round, so Three is last.
                    library.started(first, true);
                    library.started(ids[1], false);
                    library.started(last, false);
                    for pick in 0..2 {
                        let next = library.after_end(last, |_| pick, &[]).map(|track| track.id);
                        match (through, looping, shuffle) {
                            (_, false, _) => assert_eq!(next, None, "{case}"),
                            // Without Play through, Loop plays the same track again.
                            (false, true, _) => assert_eq!(next, Some(last), "{case}"),
                            (true, true, false) => assert_eq!(next, Some(first), "{case}"),
                            // A fresh order, never the track that has just played.
                            (true, true, true) => assert!(next.is_some() && next != Some(last), "{case}"),
                        }
                    }
                    // Before the last, Play through goes on whatever Loop says; without it the
                    // track ends there unless Loop plays it again.
                    let mut library = walk_in(&names, through, looping, shuffle);
                    library.started(first, true);
                    let next = library.after_end(first, |_| 0, &[]).map(|track| track.id);
                    match (through, looping, shuffle) {
                        (true, _, false) => assert_eq!(next, Some(ids[1]), "{case}"),
                        (true, _, true) => assert!(next.is_some() && next != Some(first), "{case}"),
                        (false, true, _) => assert_eq!(next, Some(first), "{case}"),
                        (false, false, _) => assert_eq!(next, None, "{case}"),
                    }
                    // The Next key goes on round to the first, Loop or not.
                    let mut library = walk_in(&names, through, looping, false);
                    library.started(last, true);
                    assert_eq!(library.next_by_hand(last, |_| 0).map(|track| track.id), Some(first), "{case}");
                }
            }
        }
    }

    #[test]
    fn shuffle_plays_each_track_once_a_time_round_and_never_one_twice_in_a_row() {
        let names = ["One", "Two", "Three", "Four", "Five"];
        let seed = std::cell::Cell::new(7usize);
        let pick = |count: usize| {
            seed.set(seed.get().wrapping_mul(1_103_515_245).wrapping_add(12_345));
            (seed.get() >> 8) % count
        };
        let mut library = walk_in(&names, true, true, true);
        let mut playing = library.shown().tracks[0].id;
        library.started(playing, true);
        let mut heard = vec![playing];
        for _ in 0..(names.len() * 6 - 1) {
            let next = library.after_end(playing, pick, &[]).map(|track| track.id).expect("goes round");
            assert_ne!(next, playing, "the same track twice in a row");
            library.started(next, false);
            heard.push(next);
            playing = next;
        }
        // Six times round: each time, every track once.
        for round in heard.chunks(names.len()) {
            let mut once = round.to_vec();
            once.sort();
            once.dedup();
            assert_eq!(once.len(), names.len(), "{round:?}");
        }
        // Without Loop, the order ends after every track has played once.
        let mut library = walk_in(&names, true, false, true);
        let mut playing = library.shown().tracks[2].id;
        library.started(playing, true);
        let mut count = 1;
        while let Some(next) = library.after_end(playing, pick, &[]).map(|track| track.id) {
            library.started(next, false);
            playing = next;
            count += 1;
            assert!(count <= names.len());
        }
        assert_eq!(count, names.len());
        // A track that has played already, chosen again, begins a new order; so does playing
        // from silence.
        let mut library = walk_in(&names, true, false, true);
        let (one, two) = (library.shown().tracks[0].id, library.shown().tracks[1].id);
        library.started(one, true);
        library.started(two, false);
        library.started(one, false);
        assert_eq!(library.round.played, vec![one]);
        library.started(two, true);
        assert_eq!(library.round.played, vec![two]);
    }

    #[test]
    fn a_playlist_of_one_track_goes_round_only_with_loop() {
        for shuffle in [false, true] {
            for through in [false, true] {
                let mut library = walk_in(&["Solo"], through, true, shuffle);
                let solo = library.shown().tracks[0].id;
                library.started(solo, true);
                assert_eq!(library.after_end(solo, |_| 0, &[]).map(|track| track.id), Some(solo));
                library.looping = false;
                assert!(library.after_end(solo, |_| 0, &[]).is_none());
            }
        }
        // An empty playlist has nothing to play.
        let library = walk_in(&[], true, true, false);
        assert!(library.after_end(Uuid::new_v4(), |_| 0, &[]).is_none());
    }

    #[test]
    fn tracks_that_could_not_be_played_are_passed_over_and_when_all_fail_nothing_follows() {
        for shuffle in [false, true] {
            let library = walk_in(&["One", "Two", "Three"], true, true, shuffle);
            let ids: Vec<Uuid> = library.shown().tracks.iter().map(|track| track.id).collect();
            // Two fails: after One, Three.
            assert_eq!(library.after_end(ids[0], |_| 0, &[ids[1]]).map(|track| track.id), Some(ids[2]));
            // Every other track fails: the one that played plays again.
            assert_eq!(library.after_end(ids[0], |_| 0, &[ids[1], ids[2]]).map(|track| track.id), Some(ids[0]));
            // All fail: nothing, so the player stops rather than trying them again and again.
            assert!(library.after_end(ids[0], |_| 0, &ids).is_none());
        }
        // Each try leaves one more out, so trying the next until one plays always ends, after
        // each track has been tried once.
        let library = walk_in(&["One", "Two", "Three", "Four"], true, true, false);
        let mut failed = Vec::new();
        let mut from = library.shown().tracks[3].id;
        while let Some(next) = library.after_end(from, |_| 0, &failed).map(|track| track.id) {
            assert!(!failed.contains(&next));
            failed.push(next);
            from = next;
            assert!(failed.len() <= 4);
        }
        assert_eq!(failed.len(), 4);
        // Without Play through, Loop plays the same track again only if it can be played.
        let library = walk_in(&["One"], false, true, false);
        let one = library.shown().tracks[0].id;
        assert!(library.after_end(one, |_| 0, &[one]).is_none());
    }

    #[test]
    fn a_track_taken_out_while_it_plays_plays_on_and_the_playlist_goes_on_as_it_now_stands() {
        let tracks: Vec<Track> = ["One", "Two", "Three", "Four"]
            .iter()
            .map(|name| Track::of(PathBuf::from(format!("/m/{name}.mp3"))))
            .collect();
        let id = |at: usize| tracks[at].id;
        let fresh = || Library {
            playlists: vec![Playlist { tracks: tracks.clone(), ..Playlist::new("Walk-in".to_owned()) }],
            through: true,
            looping: true,
            ..Library::default()
        };
        let after = |library: &Library, from: usize| library.after_end(id(from), |_| 0, &[]).map(|t| t.id);
        // Two plays and is taken out: it is still known by name, and Three comes after it.
        let mut library = fresh();
        library.remove_track(id(1), Some(id(1)));
        assert_eq!(library.shown().tracks.len(), 3);
        assert_eq!(library.track(id(1)).map(|t| t.name.as_str()), Some("Two"));
        assert_eq!(after(&library, 1), Some(id(2)));
        let (playlist, at) = library.place_of(id(1)).expect("the place it had");
        assert_eq!((playlist.name.as_str(), at), ("Walk-in", 1));
        // Taken out after it too: the playlist as it now stands goes on from that place.
        library.remove_track(id(2), Some(id(1)));
        assert_eq!(after(&library, 1), Some(id(3)));
        // With shuffle, any track left may follow.
        library.shuffle = true;
        assert_eq!(after(&library, 1), Some(id(0)));
        // Once something else plays (or nothing), it is let go.
        library.forget_leaving(Some(id(1)));
        assert!(library.leaving.is_some());
        library.forget_leaving(Some(id(0)));
        assert!(library.leaving.is_none() && library.track(id(1)).is_none());
        // Tracks above it taken out, or the one after it moved, while it plays: the track that
        // came after it still comes next.
        let mut library = fresh();
        library.remove_track(id(2), Some(id(2)));
        library.remove_track(id(0), Some(id(2)));
        assert_eq!(after(&library, 2), Some(id(3)));
        library.shown_mut().tracks.swap(0, 1);
        assert_eq!(after(&library, 2), Some(id(3)));
        assert_eq!(library.place_of(id(2)).map(|(_, at)| at), Some(0), "Four stands where it was");
        // And that one taken out too: the track now at its place.
        library.remove_track(id(3), Some(id(2)));
        assert_eq!(after(&library, 2), Some(id(1)));
        // The last one taken out while it plays: the first follows with Loop, and nothing
        // without it.
        let mut library = fresh();
        library.remove_track(id(3), Some(id(3)));
        assert_eq!(after(&library, 3), Some(id(0)));
        library.looping = false;
        assert_eq!(after(&library, 3), None);
        // Every track taken out: nothing is left to play.
        library.looping = true;
        for at in 0..3 {
            library.remove_track(id(at), Some(id(3)));
        }
        assert!(library.shown().tracks.is_empty());
        assert_eq!(after(&library, 3), None);
        // Without Play through, Loop doesn't bring back a track taken out: it ends there.
        let mut library = fresh();
        library.through = false;
        library.remove_track(id(1), Some(id(1)));
        assert_eq!(after(&library, 1), None);
        // A track that isn't playing just goes.
        let mut library = fresh();
        library.remove_track(id(0), Some(id(2)));
        assert!(library.leaving.is_none());
        // Its whole playlist deleted while it plays: it plays on, and nothing follows.
        let mut library = fresh();
        library.playlists.push(Playlist::new("Offering".to_owned()));
        assert!(library.remove_playlist(0, Some(id(2))));
        assert_eq!(library.track(id(2)).map(|t| t.name.as_str()), Some("Three"));
        assert_eq!(after(&library, 2), None);
        assert!(!library.remove_playlist(0, None), "there is always one");
        // Not kept between runs.
        let again = Library::from_json(&serde_json::to_string(&library).unwrap()).unwrap();
        assert!(again.leaving.is_none());
    }

    /// Ticks the player until `done` says so (or five seconds pass), gathering what happened.
    fn tick_until(player: &mut Player, done: impl Fn(&Player, &[Event]) -> bool) -> Vec<Event> {
        let mut events = Vec::new();
        let began = Instant::now();
        while began.elapsed() < Duration::from_secs(5) {
            events.extend(player.tick(|_| "track".to_owned()));
            if done(player, &events) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        events
    }

    /// Plays a quiet file at no volume through the real sound output: the player moves through
    /// it, holds its place while paused, goes to a place asked for, lines up the same track to
    /// follow (Loop) and joins it, and stops. Needs a sound output, so it is run by hand:
    /// `cargo test -p sp-app plays_ -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn plays_pauses_seeks_loops_and_stops_a_track() {
        let path = std::env::temp_dir().join(format!("pc-play-{}.wav", Uuid::new_v4()));
        std::fs::write(&path, quiet_wav(8000 * 3)).unwrap();
        let track = Track::of(path.clone());
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play_fading(&track, Duration::ZERO).unwrap();
        let events = tick_until(&mut player, |_, events| events.iter().any(|e| matches!(e, Event::Began { .. })));
        assert!(matches!(events.first(), Some(Event::Began { fresh: true, followed: false, .. })), "{events:?}");
        std::thread::sleep(Duration::from_millis(500));
        let playing = player.playing();
        assert_eq!(playing.track, Some(track.id));
        assert!(playing.position > Duration::from_millis(300), "{:?}", playing.position);
        assert!(playing.length.is_some_and(|length| (length.as_secs_f32() - 3.0).abs() < 0.1));
        player.toggle_pause();
        std::thread::sleep(Duration::from_millis(50));
        let paused_at = player.playing().position;
        std::thread::sleep(Duration::from_millis(300));
        assert!(player.playing().paused);
        assert_eq!(player.playing().position, paused_at, "the place is held");
        // A seek while paused: there at once, and still paused, taken up where it is once
        // played on.
        player.seek(&track, Duration::from_millis(500));
        let _ = tick_until(&mut player, |player, _| player.asked.as_ref().is_some_and(|asked| asked.stream.ready()));
        assert!(player.playing().paused);
        assert_eq!(player.playing().position, Duration::from_millis(500));
        player.toggle_pause();
        // A seek while playing: heard from there within 150 ms.
        player.seek(&track, Duration::from_millis(2000));
        let asked = Instant::now();
        let _ = tick_until(&mut player, |player, _| player.asked.is_none());
        // From the seek to its first frame played on the sound card's thread.
        let took = player.now.as_ref().and_then(|now| now.stream.heard_after()).unwrap_or_else(|| asked.elapsed());
        assert!(player.playing().position.abs_diff(Duration::from_millis(2000)) < Duration::from_millis(60));
        assert!(took < Duration::from_millis(150), "the seek took {took:?}");
        // Loop: the same track lined up, and joined without a stop.
        let fade = Duration::ZERO;
        let _ = tick_until(&mut player, |player, _| player.wants_next(fade).is_some());
        player.line_up(&track, fade).unwrap();
        let events = tick_until(&mut player, |_, events| events.iter().any(|e| matches!(e, Event::Began { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::Began { followed: true, .. })), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, Event::Ended { .. })), "{events:?}");
        assert!(player.playing().position < Duration::from_secs(1));
        if let Some(out) = &player.out {
            assert_eq!(out.controls.underruns.load(Ordering::Relaxed), 0, "no block went short");
        }
        player.stop();
        assert_eq!(player.playing(), Playing::default());
        std::fs::remove_file(path).unwrap();
    }

    /// A track that can't be played, asked for while another plays: the other plays on, and
    /// the problem is said. Needs a sound output: `cargo test -p sp-app a_broken -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn a_broken_track_leaves_the_one_playing() {
        let path = std::env::temp_dir().join(format!("pc-play-{}.wav", Uuid::new_v4()));
        std::fs::write(&path, quiet_wav(8000 * 3)).unwrap();
        let broken = std::env::temp_dir().join(format!("pc-broken-{}.mp3", Uuid::new_v4()));
        std::fs::write(&broken, b"not audio at all").unwrap();
        let (good, bad) = (Track::of(path.clone()), Track::of(broken.clone()));
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play_fading(&good, Duration::ZERO).unwrap();
        let _ = tick_until(&mut player, |_, events| !events.is_empty());
        player.play_fading(&bad, Duration::ZERO).unwrap();
        let events = tick_until(&mut player, |_, events| events.iter().any(|e| matches!(e, Event::Failed { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::Failed { lined_up: false, .. })), "{events:?}");
        assert_eq!(player.playing().track, Some(good.id));
        player.stop();
        for path in [path, broken] {
            std::fs::remove_file(path).unwrap();
        }
    }

    /// A wave file of silence: one channel, 16 bits, 8000 samples a second.
    fn quiet_wav(samples: u32) -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        wav.resize(wav.len() + samples as usize * 2, 0);
        wav
    }

    #[test]
    fn a_track_refused_before_it_was_decoded_says_why_when_asked() {
        // No sound output, so no track was started: what was refused is still the answer, not
        // "Playing".
        let track = Uuid::new_v4();
        let mut player = Player::default();
        assert_eq!(player.outcome(track), None);
        player.refused = Some((track, "No sound output.".to_owned()));
        assert_eq!(player.outcome(track), Some(Err("No sound output.".to_owned())));
        assert_eq!(player.outcome(Uuid::new_v4()), None);
    }

    #[test]
    fn a_place_to_seek_to_is_read_as_minutes_seconds_or_a_step_and_never_past_the_end() {
        let s = Duration::from_secs;
        let length = Some(s(187));
        let at = |typed: &str| seek_to(typed, s(60), length);
        assert_eq!(at("1:23"), Ok(s(83)));
        assert_eq!(at("83"), Ok(s(83)));
        assert_eq!(at("83.5"), Ok(Duration::from_millis(83_500)));
        assert_eq!(at("0:00"), Ok(s(0)));
        assert_eq!(at("+15"), Ok(s(75)));
        assert_eq!(at("-15"), Ok(s(45)));
        assert_eq!(at("+1:00"), Ok(s(120)));
        assert_eq!(seek_to("-15", s(5), length), Ok(s(0)), "back past the start: the start");
        assert_eq!(seek_to("1:02:03", s(0), None), Ok(s(3723)));
        // The end itself is a place; past it is refused, with the track's length.
        assert_eq!(at("3:07"), Ok(s(187)));
        assert_eq!(at("3:08"), Err("The track is 3:07 long; 3:08 is past its end.".to_owned()));
        assert_eq!(seek_to("+15", s(180), length), Err("The track is 3:07 long; 3:15 is past its end.".to_owned()));
        assert_eq!(at("500").unwrap_err(), "The track is 3:07 long; 8:20 is past its end.");
        // Not times.
        for typed in ["", "abc", "1:", ":30", "1:60", "1.5:00", "1:2:3:4", "--15", "+", "-1e3x", "nan", "inf"] {
            assert!(at(typed).is_err(), "{typed:?}");
        }
        // A track of no known length takes any place.
        assert_eq!(seek_to("99:00", s(0), None), Ok(s(5940)));
    }
}
