//! What Next, Previous and Enter do in a playlist: the rules for running a service by the keys.
//!
//! The keys act on the open item. While the open item is the one on screen, Next and Previous
//! step through it; once Up, Down or a click has opened another item, the next Next or Previous
//! starts that item at its first slide (a picture or video is shown, a media playlist plays).
//! At the end of a song Next goes on into the next song or Bible passage when Continuous is on,
//! passing over headers, placeholders, pictures and videos; Previous goes back the same way,
//! into the last slide of the item before. At either end of the playlist, or with Continuous
//! off, nothing changes and a notice says why.
//!
//! Nothing here knows about the window: the app says what the playlist holds and where the
//! show is, and carries out what comes back.

use sp_engine::Command;

/// One item of the playlist in front, as the keys see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// A song the show holds: its place among the show's presentations, and whether it has a
    /// slide that isn't switched off.
    Song { at: usize, slides: bool },
    /// A Bible passage the playlist keeps; its slides are made when it is gone into.
    Passage,
    /// A picture or video, by its place in the media bin while the bin has it.
    Media(Option<usize>),
    /// A media playlist, and whether it has anything to play.
    MediaPlaylist(bool),
    /// A header, a placeholder, a song the library doesn't have: nothing to show.
    Nothing,
}

impl Stop {
    /// Whether Next and Previous go on into it from the item before or after (a song or a
    /// passage with slides; never media, which would change what is behind the words).
    fn steps_into(self) -> bool {
        matches!(self, Stop::Song { slides: true, .. } | Stop::Passage)
    }

    /// Whether opening it and pressing a key has something to put on screen.
    fn startable(self) -> bool {
        self.steps_into() || matches!(self, Stop::Media(Some(_)) | Stop::MediaPlaylist(true))
    }
}

/// Where the slide on screen (or the one last on screen, after a clear) comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Here {
    /// Nothing has been on screen yet.
    Nothing,
    /// A slide from outside the playlist in front: a song from a library, the Bible page.
    Elsewhere,
    /// A slide of this item of the playlist.
    Item(usize),
}

/// What the app knows when a key that runs the show is pressed.
pub struct Situation<'a> {
    /// The items of the playlist in front, in order, and what each is called.
    pub stops: &'a [Stop],
    pub titles: &'a [String],
    /// The open item, while the left column lists the playlist in front.
    pub open: Option<usize>,
    pub here: Here,
    /// The show has nothing further to step to in that direction within what is on screen
    /// (no slide and no click build left).
    pub at_end: bool,
    pub at_start: bool,
    pub continuous: bool,
}

/// What to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// Give the show this command (a step within what is on screen, or nothing to step to).
    Show(Command),
    /// Put this item on screen: a song or passage at its first slide, or at its last when
    /// `last` (Previous going back into it); a picture or video shown; a media playlist played.
    Start { item: usize, last: bool },
    /// Change nothing, and say this.
    Say(String),
}

/// What Next (`forward`) or Previous does.
pub fn step(situation: &Situation, forward: bool) -> Do {
    let command = if forward { Command::Next } else { Command::Previous };
    let stops = situation.stops;
    let startable = |item: Option<usize>| item.filter(|at| stops.get(*at).is_some_and(|stop| stop.startable()));
    // The open item is not where the show is: it is started instead.
    match (situation.here, startable(situation.open)) {
        (Here::Item(here), Some(open)) if here != open => return Do::Start { item: open, last: false },
        (Here::Nothing, Some(open)) => return Do::Start { item: open, last: false },
        _ => {}
    }
    let edge = if forward { situation.at_end } else { situation.at_start };
    let Here::Item(here) = situation.here else {
        // Not in the playlist: the show steps, or stays at the end of what it shows.
        return if edge && situation.here == Here::Elsewhere {
            Do::Say(if forward { "That is the last slide." } else { "That is the first slide." }.to_owned())
        } else {
            Do::Show(command)
        };
    };
    if !edge {
        return Do::Show(command);
    }
    let title = situation.titles.get(here).map_or("", String::as_str);
    let (end, key) = if forward { ("last", "Next") } else { ("first", "Previous") };
    if !situation.continuous {
        return Do::Say(format!(
            "That is the {end} slide of {title}. Continuous is off, so {key} stays in it (Continuous is in the bar under the slides)."
        ));
    }
    match next_stop(stops, here, forward) {
        Some(item) => Do::Start { item, last: !forward },
        None => Do::Say(format!("That is the {end} slide of the playlist.")),
    }
}

/// Where Next goes on from the end of an item without anyone pressing a key (a slide that
/// goes on by itself, the command line's `next`): into the next song or passage when
/// Continuous is on, and nowhere otherwise.
pub fn go_on(stops: &[Stop], here: Here, continuous: bool) -> Option<usize> {
    let Here::Item(here) = here else { return None };
    continuous.then(|| next_stop(stops, here, true)).flatten()
}

/// The first item after (or before) `here` that Next goes on into.
fn next_stop(stops: &[Stop], here: usize, forward: bool) -> Option<usize> {
    if forward {
        (here + 1..stops.len()).find(|at| stops[*at].steps_into())
    } else {
        (0..here.min(stops.len())).rev().find(|at| stops[*at].steps_into())
    }
}

/// What Enter does: the open item goes on screen from its first slide.
pub fn enter(stops: &[Stop], open: Option<usize>) -> Option<Do> {
    let open = open?;
    stops.get(open).filter(|stop| stop.startable()).map(|_| Do::Start { item: open, last: false })
}

/// What ↑ or ↓ says when there is no item above or below to open.
pub fn no_item_beyond(down: bool) -> &'static str {
    if down { "That is the last item in the playlist." } else { "That is the first item in the playlist." }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The playlist the tests run: a video, a header, two songs, a header, a passage, a
    /// placeholder, a song and a media playlist, as a service might be.
    fn service() -> (Vec<Stop>, Vec<String>) {
        let stops = vec![
            Stop::Media(Some(0)),
            Stop::Nothing,
            Stop::Song { at: 0, slides: true },
            Stop::Song { at: 1, slides: true },
            Stop::Nothing,
            Stop::Passage,
            Stop::Nothing,
            Stop::Song { at: 2, slides: true },
            Stop::MediaPlaylist(true),
        ];
        let titles = ["Walk-in", "Worship", "Alpha", "Beta", "The Word", "John 3:16-18", "Sermon", "Gamma", "Loop"]
            .map(str::to_owned)
            .to_vec();
        (stops, titles)
    }

    fn situation<'a>(
        (stops, titles): &'a (Vec<Stop>, Vec<String>),
        here: Here,
        open: Option<usize>,
        at_end: bool,
        at_start: bool,
    ) -> Situation<'a> {
        Situation { stops, titles, open, here, at_end, at_start, continuous: true }
    }

    #[test]
    fn with_the_item_on_screen_open_the_keys_step_through_it() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(2), false, false);
        assert_eq!(step(&s, true), Do::Show(Command::Next));
        assert_eq!(step(&s, false), Do::Show(Command::Previous));
    }

    #[test]
    fn another_item_opened_is_started_at_its_first_slide_by_next_or_previous() {
        // Down opened Beta while Alpha is on screen: Right, and Left too, start Beta.
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(3), false, false);
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        assert_eq!(step(&s, false), Do::Start { item: 3, last: false });
        // Up opened the passage: it is started, not passed over.
        let playlist = service();
        let s = situation(&playlist, Here::Item(7), Some(5), false, false);
        assert_eq!(step(&s, true), Do::Start { item: 5, last: false });
    }

    #[test]
    fn an_open_video_is_shown_and_a_media_playlist_played_by_next() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(0), false, false);
        assert_eq!(step(&s, true), Do::Start { item: 0, last: false });
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(8), false, false);
        assert_eq!(step(&s, true), Do::Start { item: 8, last: false });
    }

    #[test]
    fn an_open_item_with_nothing_to_show_leaves_the_keys_on_the_item_on_screen() {
        // A header or a placeholder opened: Next goes on in the song on screen.
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(6), false, false);
        assert_eq!(step(&s, true), Do::Show(Command::Next));
    }

    #[test]
    fn before_anything_is_on_screen_next_starts_the_open_item() {
        let playlist = service();
        let s = situation(&playlist, Here::Nothing, Some(3), false, false);
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        let playlist = service();
        let s = situation(&playlist, Here::Nothing, None, false, false);
        assert_eq!(step(&s, true), Do::Show(Command::Next), "no item open: the show starts the song chosen");
    }

    #[test]
    fn at_the_end_of_a_song_next_goes_on_into_the_next_song() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(2), true, false);
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
    }

    #[test]
    fn next_steps_into_a_bible_passage_over_a_header() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(3), Some(3), true, false);
        assert_eq!(step(&s, true), Do::Start { item: 5, last: false });
    }

    #[test]
    fn next_passes_over_a_placeholder_and_previous_goes_back_into_the_last_slide_before() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(5), Some(5), true, false);
        assert_eq!(step(&s, true), Do::Start { item: 7, last: false }, "over the placeholder into Gamma");
        let playlist = service();
        let s = situation(&playlist, Here::Item(7), Some(7), false, true);
        assert_eq!(step(&s, false), Do::Start { item: 5, last: true }, "back into the passage, at its last slide");
        let playlist = service();
        let s = situation(&playlist, Here::Item(5), Some(5), false, true);
        assert_eq!(step(&s, false), Do::Start { item: 3, last: true }, "and on back to Beta");
    }

    #[test]
    fn at_either_end_of_the_playlist_it_stays_and_says_so() {
        let playlist = service();
        let s = situation(&playlist, Here::Item(7), Some(7), true, false);
        assert_eq!(step(&s, true), Do::Say("That is the last slide of the playlist.".to_owned()));
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(2), false, true);
        assert_eq!(step(&s, false), Do::Say("That is the first slide of the playlist.".to_owned()));
    }

    #[test]
    fn with_continuous_off_next_stays_in_the_song_and_says_so() {
        let playlist = service();
        let mut s = situation(&playlist, Here::Item(2), Some(2), true, false);
        s.continuous = false;
        let Do::Say(said) = step(&s, true) else { panic!("it stays") };
        assert!(said.starts_with("That is the last slide of Alpha. Continuous is off"), "{said}");
    }

    #[test]
    fn a_song_from_a_library_stays_at_its_end_and_says_so() {
        let playlist = service();
        let s = situation(&playlist, Here::Elsewhere, Some(3), true, false);
        assert_eq!(step(&s, true), Do::Say("That is the last slide.".to_owned()));
        let playlist = service();
        let s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        assert_eq!(step(&s, true), Do::Show(Command::Next), "the open item isn't started over a song from elsewhere");
    }

    #[test]
    fn going_on_by_itself_follows_continuous_and_never_says_anything() {
        let (stops, _) = service();
        assert_eq!(go_on(&stops, Here::Item(3), true), Some(5));
        assert_eq!(go_on(&stops, Here::Item(3), false), None);
        assert_eq!(go_on(&stops, Here::Item(7), true), None);
        assert_eq!(go_on(&stops, Here::Elsewhere, true), None);
    }

    #[test]
    fn enter_puts_the_open_item_on_screen() {
        let (stops, _) = service();
        assert_eq!(enter(&stops, Some(3)), Some(Do::Start { item: 3, last: false }));
        assert_eq!(enter(&stops, Some(0)), Some(Do::Start { item: 0, last: false }), "a video is shown");
        assert_eq!(enter(&stops, Some(6)), None, "a placeholder has nothing");
        assert_eq!(enter(&stops, None), None);
    }
}
