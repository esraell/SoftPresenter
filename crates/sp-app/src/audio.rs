//! Audio: playlists of tracks to play, such as walk-in music, and the player that plays one at a
//! time (crossfading from one to the next if asked). The playlists are kept in `audio.json`;
//! the files stay where they are.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sp_model::Uuid;

use crate::i18n::{t, tf, tg, tgf};

/// The file endings of audio the player can play, as the file chooser lists them.
pub const AUDIO_ENDINGS: [&str; 9] = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "mp4", "aif", "aiff"];

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

/// Plays one track at a time. The sound output is opened the first time a track is played, so a
/// computer that never plays audio never opens it.
pub struct Player {
    /// The sound output, tapped for the screens that send sound (see `sp_video::sound`).
    stream: Option<sp_video::sound::Output>,
    sink: Option<rodio::Sink>,
    track: Option<(Uuid, Option<Duration>)>,
    volume: f32,
    /// Tracks fading out under the one now playing: each with when its fade began, and how long.
    fading: Vec<(rodio::Sink, Instant, Duration)>,
    /// The track after which nothing could be played: it is not tried again as it ends.
    gave_up: Option<Uuid>,
}

impl Default for Player {
    fn default() -> Player {
        Player { stream: None, sink: None, track: None, volume: 1.0, fading: Vec::new(), gave_up: None }
    }
}

/// What came of playing on after a track: the track that plays now, if one does, and what was
/// wrong with each that could not be played.
#[derive(Debug, Default)]
pub struct FollowedOn {
    pub played: Option<Uuid>,
    pub problems: Vec<String>,
}

impl Player {
    /// Plays a track from its start, fading it in over `fade` while the one playing fades out.
    /// A track that can't be played leaves what plays as it was.
    pub fn play_fading(&mut self, track: &Track, fade: Duration) -> Result<(), String> {
        use rodio::Source;
        if self.stream.is_none() {
            let stream = sp_video::sound::Output::open().map_err(|e| {
                tf(
                    "tools.audio.no_output",
                    "Could not open the computer's sound output: {error}",
                    &[("error", &e.to_string())],
                )
            })?;
            self.stream = Some(stream);
        }
        let decoder = open(track)?;
        let length = decoder.total_duration();
        match self.sink.take().filter(|sink| !fade.is_zero() && !sink.empty() && !sink.is_paused()) {
            Some(old) => self.fading.push((old, Instant::now(), fade)),
            None => self.stop(),
        }
        let Some(stream) = &self.stream else {
            return Err(t("tools.audio.no_output.none", "No sound output.").to_owned());
        };
        let sink = rodio::Sink::connect_new(stream.mixer());
        sink.set_volume(self.volume);
        if fade.is_zero() {
            sink.append(decoder);
        } else {
            sink.append(decoder.fade_in(fade));
        }
        self.sink = Some(sink);
        self.track = Some((track.id, length));
        self.gave_up = None;
        Ok(())
    }

    /// Plays what follows `from` in `library` ([`Library::after_end`]), fading in over `fade`:
    /// a track that can't be played is passed over, each tried once, so a playlist of tracks
    /// that all fail stops rather than trying them again and again. Once nothing could be
    /// played after `from`, it is not tried again ([`may_follow`](Self::may_follow)).
    pub fn follow_on(&mut self, library: &Library, from: Uuid, fade: Duration) -> FollowedOn {
        let pick = |count: usize| (Uuid::new_v4().as_u128() % count as u128) as usize;
        let mut failed = Vec::new();
        let mut problems = Vec::new();
        while let Some(next) = library.after_end(from, pick, &failed) {
            match self.play_fading(next, fade) {
                Ok(()) => return FollowedOn { played: Some(next.id), problems },
                Err(problem) => {
                    failed.push(next.id);
                    problems.push(problem);
                }
            }
        }
        self.gave_up = Some(from);
        FollowedOn { played: None, problems }
    }

    /// False once nothing could be played after this track.
    pub fn may_follow(&self, from: Uuid) -> bool {
        self.gave_up != Some(from)
    }

    /// Pauses, or goes on from where it was paused.
    pub fn toggle_pause(&mut self) {
        if let Some(sink) = &self.sink {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    /// Stops, and forgets the track; tracks fading out stop too.
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        for (sink, _, _) in self.fading.drain(..) {
            sink.stop();
        }
        self.track = None;
    }

    /// Turns down the tracks fading out, and lets go of the ones done. True while any fade.
    pub fn tick(&mut self) -> bool {
        let volume = self.volume;
        self.fading.retain(|(sink, began, fade)| {
            let t = began.elapsed().as_secs_f32() / fade.as_secs_f32().max(0.001);
            if t >= 1.0 || sink.empty() {
                sink.stop();
                return false;
            }
            sink.set_volume(volume * (1.0 - t));
            true
        });
        !self.fading.is_empty()
    }

    /// How long the track playing has left, when its length is known.
    pub fn left(&self) -> Option<Duration> {
        let (_, length) = self.track?;
        let sink = self.sink.as_ref()?;
        length.map(|length| length.saturating_sub(sink.get_pos()))
    }

    /// Jumps by some seconds, forward or back, within the track.
    pub fn skip(&mut self, seconds: f32) {
        if let Some(sink) = &self.sink {
            let at = sink.get_pos().as_secs_f32() + seconds;
            let _ = sink.try_seek(Duration::from_secs_f32(at.max(0.0)));
        }
    }

    /// Goes to a place in the track, playing or paused.
    pub fn seek(&mut self, to: Duration) {
        if let Some(sink) = &self.sink {
            let _ = sink.try_seek(to);
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.5);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    /// The track that has just played to its end, if one has; it is forgotten.
    pub fn ended(&mut self) -> Option<Uuid> {
        if self.sink.as_ref().is_some_and(rodio::Sink::empty) {
            let ended = self.track.map(|(id, _)| id);
            if let Some(sink) = self.sink.take() {
                sink.stop();
            }
            self.track = None;
            return ended;
        }
        None
    }

    /// What is playing now. A track that has played to its end is forgotten.
    pub fn playing(&mut self) -> Playing {
        if self.sink.as_ref().is_some_and(rodio::Sink::empty) {
            if let Some(sink) = self.sink.take() {
                sink.stop();
            }
            self.track = None;
        }
        match (&self.sink, self.track) {
            (Some(sink), Some((id, length))) => {
                Playing { track: Some(id), position: sink.get_pos(), length, paused: sink.is_paused() }
            }
            _ => Playing::default(),
        }
    }
}

type FileDecoder = rodio::Decoder<std::io::BufReader<std::fs::File>>;

/// Opens a track's file and reads its sound, saying in words what went wrong if it can't.
fn open(track: &Track) -> Result<FileDecoder, String> {
    let file = std::fs::File::open(&track.path).map_err(|e| {
        tf(
            "tools.audio.not_opened",
            "Could not open {name}: {error}",
            &[("name", &track.name), ("error", &e.to_string())],
        )
    })?;
    rodio::Decoder::new(std::io::BufReader::new(file)).map_err(|e| {
        tf(
            "tools.audio.not_playable",
            "{name} is not audio SoftPresenter can play ({error}).",
            &[("name", &track.name), ("error", &e.to_string())],
        )
    })
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
            let mut player = Player::default();
            assert_eq!(player.playing(), Playing::default());
            let missing = Track::of(PathBuf::from("/nowhere/none.mp3"));
            assert!(open(&missing).err().unwrap_or_default().contains("Could not open none"));
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

    /// Plays a quiet file at no volume and checks the player moves through it, pauses and stops.
    /// Needs a sound output, so it is run by hand: `cargo test -p sp-app plays_ -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn plays_pauses_and_stops_a_track() {
        let path = std::env::temp_dir().join(format!("pc-play-{}.wav", Uuid::new_v4()));
        std::fs::write(&path, quiet_wav(8000 * 3)).unwrap();
        let track = Track::of(path.clone());
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play_fading(&track, Duration::ZERO).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        let playing = player.playing();
        assert_eq!(playing.track, Some(track.id));
        assert!(playing.position > Duration::from_millis(200), "{:?}", playing.position);
        assert!(playing.length.is_some_and(|length| (length.as_secs_f32() - 3.0).abs() < 0.1));
        player.toggle_pause();
        let paused_at = player.playing().position;
        std::thread::sleep(Duration::from_millis(300));
        assert!(player.playing().paused);
        assert!(player.playing().position.abs_diff(paused_at) < Duration::from_millis(60));
        player.stop();
        assert_eq!(player.playing(), Playing::default());
        std::fs::remove_file(path).unwrap();
    }

    /// Plays a track, then crossfades into another: both play during the fade, then only the
    /// second. Needs a sound output: `cargo test -p sp-app crossfades -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn crossfades_into_the_next_track() {
        let write = |seconds: u32| {
            let path = std::env::temp_dir().join(format!("pc-fade-{}.wav", Uuid::new_v4()));
            std::fs::write(&path, quiet_wav(8000 * seconds)).unwrap();
            path
        };
        let (first, second) = (Track::of(write(4)), Track::of(write(4)));
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play_fading(&first, Duration::ZERO).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        player.play_fading(&second, Duration::from_millis(500)).unwrap();
        assert!(player.tick(), "the first is fading out");
        assert_eq!(player.playing().track, Some(second.id));
        std::thread::sleep(Duration::from_millis(650));
        assert!(!player.tick(), "the fade is over");
        assert_eq!(player.playing().track, Some(second.id));
        assert!(player.left().is_some_and(|left| left < Duration::from_secs(4)));
        player.stop();
        for track in [first, second] {
            std::fs::remove_file(track.path).unwrap();
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

    #[test]
    fn a_wave_file_is_read_with_its_length() {
        use rodio::Source;
        // Two seconds of silence.
        let wav = quiet_wav(16_000);
        let dir = std::env::temp_dir().join(format!("pc-audio-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Quiet.wav");
        std::fs::write(&path, wav).unwrap();
        let decoder = open(&Track::of(path)).unwrap();
        let length = decoder.total_duration().unwrap();
        assert!((length.as_secs_f32() - 2.0).abs() < 0.05, "{length:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
