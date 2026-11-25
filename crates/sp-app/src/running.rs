//! What Next, Previous and Enter do in a playlist: the rules for running a service by the keys.
//!
//! The keys act on the open item. While the open item is the one on screen, Next and Previous
//! step through it; once Up, Down or a click has opened another item, the next Next or Previous
//! starts that item at its first slide (a picture or video is shown, a media playlist plays).
//! A slide from outside the playlist (a reading from the Bible page, a song from a library) is
//! the exception while it is on screen: they go on with it, and the open item waits; at its last
//! slide Next starts the open item (Previous at its first stays), and once it has been cleared,
//! Next and Previous start the open item. Next always moves what the audience sees; opening an
//! item only lines it up (the owner, 2026-11-25).
//! At the end of a song Next goes on into the next song or Bible passage when Continuous is on,
//! passing over headers, placeholders, pictures and videos; Previous goes back the same way,
//! into the last slide of the item before. At either end of the playlist, or with Continuous
//! off, nothing changes and a refusal's cue says why.
//!
//! A slide can be picked to go on screen next: what the find box opens (a passage's first verse,
//! or a song's first slide or the slide found), the passage Scripture Lookup looks up, and in the
//! Bible view the verse ↑ or ↓ moves to. Only Enter puts a picked slide on screen. While one
//! waits, Next and Previous go on with what is on screen (a song runs on to its end while the
//! reading waits), and with nothing on screen they say so and start nothing.
//!
//! Before Next is pressed, the line under the preview and `sp status` say what it would put on
//! screen ([`next_up`]), made from the same decision, so they never say other than the key does.
//!
//! Nothing here knows about the window: the app says what the playlist holds and where the
//! show is, and carries out what comes back.

use crate::i18n::{t, tf};
use sp_engine::{Command, SlideRef};

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
    /// A slide picked and waiting for Enter, by what it is (`verse` or `slide`).
    pub picked: Option<&'static str>,
    /// A slide is on screen now (not cleared).
    pub showing: bool,
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
    /// Change nothing: the show is at an end of what it steps through. Said only in a command's
    /// reply (to `sp` or a remote page), never in the window: reaching an end needs no notice
    /// (the owner, 2026-11-23).
    Stay(End),
}

/// Which end Next or Previous stayed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    /// The last (or first) slide of a slide from outside the playlist.
    Slide { last: bool },
    /// The last or first slide of an item of the playlist (by its title), with Continuous off.
    ContinuousOff { title: String, last: bool },
    /// The last or first slide of the playlist.
    Playlist { last: bool },
}

impl End {
    /// What a command's reply says.
    pub fn words(&self) -> String {
        let end = |last: bool| if last { "last" } else { "first" };
        match self {
            End::Slide { last } => format!("That is the {} slide.", end(*last)),
            End::ContinuousOff { title, last } => {
                format!("That is the {} slide of {title}, and Continuous is off.", end(*last))
            }
            End::Playlist { last } => format!("That is the {} slide of the playlist.", end(*last)),
        }
    }
}

/// What Next or Previous said instead of moving, for a command's reply: `at_end` when it stayed
/// at an end of what it steps through (said in the reply only).
pub struct Said {
    pub words: String,
    pub at_end: bool,
}

/// What Next or Previous does while a slide picked waits for Enter (`picked` says what it is):
/// with nothing on screen they put nothing on (not the picked slide, nor the song chosen) and
/// say how; with something on screen, `None`: they go on with it.
pub fn held_for_enter(picked: Option<&str>, showing: bool) -> Option<Do> {
    let what = picked.filter(|_| !showing)?;
    Some(Do::Say(match what {
        "verse" => t("notices.enter.verse", "Press Enter to put the picked verse on screen.").to_owned(),
        "slide" => t("notices.enter.slide", "Press Enter to put the picked slide on screen.").to_owned(),
        what => tf("notices.enter.other", "Press Enter to put the picked {what} on screen.", &[("what", what)]),
    }))
}

/// What Next (`forward`) or Previous does.
pub fn step(situation: &Situation, forward: bool) -> Do {
    let command = if forward { Command::Next } else { Command::Previous };
    if let Some(said) = held_for_enter(situation.picked, situation.showing) {
        return said;
    }
    let stops = situation.stops;
    let startable = |item: Option<usize>| item.filter(|at| stops.get(*at).is_some_and(|stop| stop.startable()));
    // A slide picked waits for Enter: the open item is not started over it.
    let open = situation.open.filter(|_| situation.picked.is_none());
    // The open item is not where the show is: it is started instead. A slide from elsewhere still
    // on screen (a reading from the Bible page, a song from a library) goes on, so it can be
    // finished while the next item is opened, and at its last slide Next starts the item opened;
    // once it has been cleared, the item opened is what Next and Previous mean (the owner,
    // 2026-11-25).
    match (situation.here, startable(open)) {
        (Here::Item(here), Some(open)) if here != open => return Do::Start { item: open, last: false },
        (Here::Nothing, Some(open)) => return Do::Start { item: open, last: false },
        (Here::Elsewhere, Some(open)) if !situation.showing => return Do::Start { item: open, last: false },
        (Here::Elsewhere, Some(open)) if forward && situation.at_end => return Do::Start { item: open, last: false },
        _ => {}
    }
    let edge = if forward { situation.at_end } else { situation.at_start };
    let Here::Item(here) = situation.here else {
        // Not in the playlist: the show steps, or stays at the end of what it shows.
        return if edge && situation.here == Here::Elsewhere {
            Do::Stay(End::Slide { last: forward })
        } else {
            Do::Show(command)
        };
    };
    if !edge {
        return Do::Show(command);
    }
    if !situation.continuous {
        let title = situation.titles.get(here).cloned().unwrap_or_default();
        return Do::Stay(End::ContinuousOff { title, last: forward });
    }
    match next_stop(stops, here, forward) {
        Some(item) => Do::Start { item, last: !forward },
        None => Do::Stay(End::Playlist { last: forward }),
    }
}

/// What Next would put on screen, said before it is pressed: the line under the preview and
/// `sp status`'s Next. Made from what Next decides ([`next_up`]), so it never says other than
/// the key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NextUp {
    /// A slide of what is on screen (or was last): by its group's name, or by its number (from
    /// 1) when its group has none.
    Slide { name: Option<String>, number: usize },
    /// An item started, or a song with nothing shown yet: a song or passage at its first slide
    /// (`slides`), or a picture, a video or a media playlist shown; by its title.
    Item { title: String, slides: bool },
    /// Nothing: the playlist's last slide is on screen.
    PlaylistEnd,
    /// Nothing: an item's last slide is on screen, and Continuous is off.
    ContinuousOff,
    /// Nothing worth saying: Next would change nothing, set a click build going, or only say
    /// why it doesn't.
    Nothing,
}

impl NextUp {
    /// The line's words, in the window's language; `None` when it says nothing.
    pub fn words(&self) -> Option<String> {
        Some(match self {
            NextUp::Slide { name: Some(name), .. } => tf("right.next.what", "Next: {what}", &[("what", name)]),
            NextUp::Slide { name: None, number } => {
                tf("right.next.slide", "Next: slide {n}", &[("n", &number.to_string())])
            }
            NextUp::Item { title, slides: true } => tf("right.next.start", "Next: {item}, slide 1", &[("item", title)]),
            NextUp::Item { title, slides: false } => tf("right.next.what", "Next: {what}", &[("what", title)]),
            NextUp::PlaylistEnd => {
                t("right.next.playlist_end", "Next: nothing \u{2014} end of the playlist").to_owned()
            }
            NextUp::ContinuousOff => {
                t("right.next.continuous_off", "Next: nothing \u{2014} Continuous is off").to_owned()
            }
            NextUp::Nothing => return None,
        })
    }
}

/// What Next would put on screen, from what it decides (`decided`: [`step`]'s answer, or the
/// app's on the Bible page and in the editors, with Continuous going on), the playlist's items
/// and their titles, and, when Next only steps the show, the slide the show would put on
/// (`slide`, asked only then).
pub fn next_up(decided: &Do, stops: &[Stop], titles: &[String], slide: impl FnOnce() -> NextUp) -> NextUp {
    match decided {
        Do::Show(Command::Next) => slide(),
        Do::Start { item, .. } => NextUp::Item {
            title: titles.get(*item).cloned().unwrap_or_default(),
            slides: stops.get(*item).is_some_and(|stop| stop.steps_into()),
        },
        Do::Stay(End::Playlist { .. }) => NextUp::PlaylistEnd,
        Do::Stay(End::ContinuousOff { .. }) => NextUp::ContinuousOff,
        Do::Show(_) | Do::Say(_) | Do::Stay(End::Slide { .. }) => NextUp::Nothing,
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

/// Where Enter is pressed, as it matters to what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listed {
    /// The Service view, with the playlist in front listed, and its open item.
    Playlist(Option<usize>),
    /// The Service view, with a library listed.
    Library,
    /// The Bible view.
    Bible,
}

/// What Enter does: a slide picked goes on screen. Otherwise, in the Service view, the open item
/// goes on screen from its first slide (with a library listed, the song chosen); in the Bible
/// view nothing does until a verse is picked.
pub fn enter(stops: &[Stop], listed: Listed, picked: Option<SlideRef>) -> Option<Do> {
    if let Some(slide) = picked {
        return Some(Do::Show(Command::GoTo(slide)));
    }
    match listed {
        Listed::Playlist(open) => {
            let open = open?;
            stops.get(open).filter(|stop| stop.startable()).map(|_| Do::Start { item: open, last: false })
        }
        Listed::Library => Some(Do::Show(Command::First)),
        Listed::Bible => None,
    }
}

/// Where ↑ (`down` false) or ↓ moves the pick in the Bible view's passage of `count` slides: to
/// the verse before or after the one picked, or with none picked the one on screen (`live`, by
/// its place in the passage), or with neither to the first (↓) or the last (↑). At either end
/// it stays (`None`), saying nothing.
pub fn move_pick(
    count: usize,
    picked: Option<usize>,
    live: Option<usize>,
    down: bool,
) -> Result<Option<usize>, &'static str> {
    if count == 0 {
        return Err(t("notices.bible.look_up_first", "Look a passage up first."));
    }
    Ok(match picked.or(live).filter(|at| *at < count) {
        Some(at) if down => (at + 1 < count).then_some(at + 1),
        Some(at) => at.checked_sub(1),
        None if down => Some(0),
        None => Some(count - 1),
    })
}

/// What Space or an arrow does while a slide is picked (the owner's rule, 2026-11-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKey {
    /// Put the picked slide on screen, as Enter does.
    TakeIt,
    /// Move the pick to the next (true) or the previous slide, putting nothing on.
    Move(bool),
    /// Not the pick's: run what is on screen as ever.
    Pass,
}

/// Which: with nothing on screen, Space takes the pick and → / ← move it; with something on
/// screen (or no pick, or in an editor) they run the show as ever, so a song can be finished
/// with a reading picked and waiting for Enter.
pub fn pick_key(space: bool, right: bool, left: bool, picked: bool, showing: bool, editing: bool) -> PickKey {
    if !picked || showing || editing {
        return PickKey::Pass;
    }
    if space {
        PickKey::TakeIt
    } else if right || left {
        PickKey::Move(right)
    } else {
        PickKey::Pass
    }
}

/// The slide after (`forward`) or before `at` in a song's or passage's order (as sung), or its
/// first when `at` is not in it; `None` past either end.
pub fn pick_beside(order: &[usize], at: usize, forward: bool) -> Option<usize> {
    match order.iter().position(|slide| *slide == at) {
        Some(place) if forward => order.get(place + 1).copied(),
        Some(place) => place.checked_sub(1).and_then(|place| order.get(place).copied()),
        None => order.first().copied(),
    }
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
        Situation { stops, titles, open, here, at_end, at_start, continuous: true, picked: None, showing: true }
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
        assert_eq!(step(&s, true), Do::Stay(End::Playlist { last: true }));
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(2), false, true);
        assert_eq!(step(&s, false), Do::Stay(End::Playlist { last: false }));
    }

    #[test]
    fn with_continuous_off_next_stays_in_the_song_and_says_so() {
        let playlist = service();
        let mut s = situation(&playlist, Here::Item(2), Some(2), true, false);
        s.continuous = false;
        let Do::Stay(end) = step(&s, true) else { panic!("it stays") };
        assert_eq!(end.words(), "That is the last slide of Alpha, and Continuous is off.");
    }

    #[test]
    fn a_song_from_a_library_stays_at_its_end_and_says_so() {
        // Nothing startable open (a placeholder, or a library listed): it stays.
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(6), true, false);
        assert!(s.showing, "still on screen");
        assert_eq!(step(&s, true), Do::Stay(End::Slide { last: true }));
        s.open = None;
        assert_eq!(step(&s, true), Do::Stay(End::Slide { last: true }));
        let playlist = service();
        let s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        assert!(s.showing, "still on screen");
        assert_eq!(step(&s, true), Do::Show(Command::Next), "the open item isn't started over a song from elsewhere");
        assert_eq!(step(&s, false), Do::Show(Command::Previous), "nor by Previous");
    }

    #[test]
    fn at_a_readings_last_slide_next_starts_the_song_lined_up() {
        // A verse from the Bible page on screen at its last slide, Beta opened in the playlist
        // (lined up): Next starts Beta at its first slide (the owner, 2026-11-25). Continuous
        // has nothing to do with it: the item was opened.
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), true, false);
        assert!(s.showing, "still on screen");
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        s.continuous = false;
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        // Previous at the reading's first slide stays, as before.
        (s.at_end, s.at_start) = (false, true);
        assert_eq!(step(&s, false), Do::Stay(End::Slide { last: false }));
        // At a slide that is both its first and its last, Next starts Beta and Previous stays.
        s.at_end = true;
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        assert_eq!(step(&s, false), Do::Stay(End::Slide { last: false }));
    }

    #[test]
    fn at_a_readings_last_slide_next_starts_a_passage_or_a_video_lined_up() {
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(5), true, false);
        assert_eq!(step(&s, true), Do::Start { item: 5, last: false }, "the passage");
        s.open = Some(0);
        assert_eq!(step(&s, true), Do::Start { item: 0, last: false }, "a video is shown");
        s.open = Some(8);
        assert_eq!(step(&s, true), Do::Start { item: 8, last: false }, "a media playlist plays");
    }

    #[test]
    fn at_a_readings_last_slide_a_slide_picked_still_holds_the_keys() {
        // Beta open with a verse of the reading picked and waiting for Enter: Next stays at the
        // reading's last slide, as before; the pick waits.
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), true, false);
        s.picked = Some("verse");
        assert_eq!(step(&s, true), Do::Stay(End::Slide { last: true }));
    }

    #[test]
    fn once_a_reading_from_elsewhere_is_cleared_next_starts_the_song_opened_in_the_playlist() {
        // A verse from the Bible page went on screen and was cleared; then Beta was opened in the
        // playlist (a click, `sp playlist go`): Next starts Beta at its first slide, not the
        // reading's next verse (the owner, 2026-11-25). Previous too, as with nothing shown yet.
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        s.showing = false;
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        assert_eq!(step(&s, false), Do::Start { item: 3, last: false });
        // Cleared at the reading's last verse, or its first: the same.
        (s.at_end, s.at_start) = (true, true);
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false });
        assert_eq!(step(&s, false), Do::Start { item: 3, last: false });
    }

    #[test]
    fn once_a_reading_from_elsewhere_is_cleared_next_starts_a_passage_opened_in_the_playlist() {
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(5), false, false);
        s.showing = false;
        assert_eq!(step(&s, true), Do::Start { item: 5, last: false });
        // A video opened is shown, as with nothing on screen yet.
        s.open = Some(0);
        assert_eq!(step(&s, true), Do::Start { item: 0, last: false });
    }

    #[test]
    fn once_a_reading_from_elsewhere_is_cleared_with_nothing_startable_open_it_goes_on_as_before() {
        let playlist = service();
        let mut s = situation(&playlist, Here::Elsewhere, Some(6), false, false);
        s.showing = false;
        assert_eq!(step(&s, true), Do::Show(Command::Next), "a placeholder open: the reading goes on");
        s.open = None;
        assert_eq!(step(&s, true), Do::Show(Command::Next), "no item open (a library listed): the same");
        s.at_end = true;
        assert_eq!(step(&s, true), Do::Stay(End::Slide { last: true }));
        // A verse picked and waiting for Enter still holds the keys.
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        (s.showing, s.picked) = (false, Some("verse"));
        assert_eq!(step(&s, true), Do::Say("Press Enter to put the picked verse on screen.".to_owned()));
    }

    #[test]
    fn after_a_clear_the_item_on_screen_last_goes_on_where_it_was() {
        // Alpha cleared part way and still the open item: Next goes on in Alpha, as ever.
        let playlist = service();
        let mut s = situation(&playlist, Here::Item(2), Some(2), false, false);
        s.showing = false;
        assert_eq!(step(&s, true), Do::Show(Command::Next));
        assert_eq!(step(&s, false), Do::Show(Command::Previous));
    }

    /// What the line under the preview says, as `App::next_up` makes it: from what `step`
    /// decides, with `slide` standing in for the show's own next slide.
    fn line(s: &Situation) -> NextUp {
        let slide = || NextUp::Slide { name: Some("Verse 2".to_owned()), number: 2 };
        next_up(&step(s, true), s.stops, s.titles, slide)
    }

    #[test]
    fn the_next_line_says_what_next_would_put_on_screen() {
        let playlist = service();
        let item = |title: &str, slides: bool| NextUp::Item { title: title.to_owned(), slides };
        // The item on screen open: its next slide, as the show has it.
        let mut s = situation(&playlist, Here::Item(2), Some(2), false, false);
        assert_eq!(line(&s), NextUp::Slide { name: Some("Verse 2".to_owned()), number: 2 });
        // Another item opened: it starts; a video is shown, a media playlist plays.
        s.open = Some(3);
        assert_eq!(line(&s), item("Beta", true));
        s.open = Some(0);
        assert_eq!(line(&s), item("Walk-in", false));
        s.open = Some(8);
        assert_eq!(line(&s), item("Loop", false));
        // A placeholder opened: the song on screen goes on.
        s.open = Some(6);
        assert_eq!(line(&s), NextUp::Slide { name: Some("Verse 2".to_owned()), number: 2 });
        // At the end of a song, with Continuous on: the next song or passage, over a header.
        let s = situation(&playlist, Here::Item(3), Some(3), true, false);
        assert_eq!(line(&s), item("John 3:16-18", true));
        // At the end of the playlist, and with Continuous off: nothing, and why.
        let mut s = situation(&playlist, Here::Item(7), Some(7), true, false);
        assert_eq!(line(&s), NextUp::PlaylistEnd);
        s.here = Here::Item(2);
        s.open = Some(2);
        s.continuous = false;
        assert_eq!(line(&s), NextUp::ContinuousOff);
        // Nothing on screen yet, Beta open: Beta.
        let s = situation(&playlist, Here::Nothing, Some(3), false, false);
        assert_eq!(line(&s), item("Beta", true));
    }

    #[test]
    fn the_next_line_follows_a_reading_from_elsewhere() {
        let playlist = service();
        // On screen, part way: its next verse, even with Beta opened.
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        assert_eq!(line(&s), NextUp::Slide { name: Some("Verse 2".to_owned()), number: 2 });
        // At its last slide with Beta lined up: Beta (the owner's rule, 2026-11-25).
        s.at_end = true;
        assert_eq!(line(&s), NextUp::Item { title: "Beta".to_owned(), slides: true });
        // With nothing lined up: nothing to say.
        s.open = Some(6);
        assert_eq!(line(&s), NextUp::Nothing);
        // Cleared, with Beta opened: Beta.
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), false, false);
        s.showing = false;
        assert_eq!(line(&s), NextUp::Item { title: "Beta".to_owned(), slides: true });
        // A verse picked and waiting for Enter, with nothing on screen: Next only says so.
        s.picked = Some("verse");
        assert_eq!(line(&s), NextUp::Nothing);
        // With the reading on screen at its last slide, a pick waiting holds the keys: nothing.
        let mut s = situation(&playlist, Here::Elsewhere, Some(3), true, false);
        s.picked = Some("verse");
        assert_eq!(line(&s), NextUp::Nothing);
    }

    #[test]
    fn the_next_line_takes_the_show_s_slide_only_when_next_only_steps_it() {
        // A click build left, the end of what is on screen with the slide still there: the show
        // has no slide to put on, and the line says nothing.
        let playlist = service();
        let s = situation(&playlist, Here::Item(2), Some(2), false, false);
        assert_eq!(next_up(&step(&s, true), s.stops, s.titles, || NextUp::Nothing), NextUp::Nothing);
        // On the Bible page and in the editors the app decides a plain step: the show's slide.
        let slide = NextUp::Slide { name: None, number: 4 };
        assert_eq!(next_up(&Do::Show(Command::Next), &[], &[], || slide.clone()), slide);
    }

    #[test]
    fn the_next_lines_words_in_english_and_back_in_english_for_sp() {
        use crate::i18n::{Language, english_of, with_language};
        let item = |title: &str, slides: bool| NextUp::Item { title: title.to_owned(), slides };
        let said = |up: NextUp| with_language(Language::English, || up.words());
        assert_eq!(
            said(NextUp::Slide { name: Some("Verse 2".to_owned()), number: 4 }).as_deref(),
            Some("Next: Verse 2")
        );
        assert_eq!(said(NextUp::Slide { name: None, number: 4 }).as_deref(), Some("Next: slide 4"));
        assert_eq!(said(item("Amazing Grace", true)).as_deref(), Some("Next: Amazing Grace, slide 1"));
        assert_eq!(said(item("Walk-in.mp4", false)).as_deref(), Some("Next: Walk-in.mp4"));
        assert_eq!(said(NextUp::PlaylistEnd).as_deref(), Some("Next: nothing \u{2014} end of the playlist"));
        assert_eq!(said(NextUp::ContinuousOff).as_deref(), Some("Next: nothing \u{2014} Continuous is off"));
        assert_eq!(said(NextUp::Nothing), None);
        // The window in Russian: its words; `sp status` gives them back in English (the song's
        // own names stay as they are).
        let russian = with_language(Language::Russian, || item("Благодать", true).words()).unwrap_or_default();
        assert_eq!(russian, "Далее: Благодать, слайд 1");
        assert_eq!(english_of(&russian), "Next: Благодать, slide 1");
        let russian = with_language(Language::Russian, || NextUp::PlaylistEnd.words()).unwrap_or_default();
        assert_eq!(english_of(&russian), "Next: nothing \u{2014} end of the playlist");
    }

    #[test]
    fn an_end_stayed_at_is_said_as_before() {
        assert_eq!(End::Slide { last: true }.words(), "That is the last slide.");
        assert_eq!(End::Slide { last: false }.words(), "That is the first slide.");
        assert_eq!(End::Playlist { last: true }.words(), "That is the last slide of the playlist.");
        let off = End::ContinuousOff { title: "Alpha".to_owned(), last: false };
        assert_eq!(off.words(), "That is the first slide of Alpha, and Continuous is off.");
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
        let playlist = |open| enter(&stops, Listed::Playlist(open), None);
        assert_eq!(playlist(Some(3)), Some(Do::Start { item: 3, last: false }));
        assert_eq!(playlist(Some(0)), Some(Do::Start { item: 0, last: false }), "a video is shown");
        assert_eq!(playlist(Some(6)), None, "a placeholder has nothing");
        assert_eq!(playlist(None), None);
        assert_eq!(enter(&stops, Listed::Library, None), Some(Do::Show(Command::First)), "the song chosen");
        assert_eq!(enter(&stops, Listed::Bible, None), None, "in the Bible view, nothing until a verse is picked");
    }

    #[test]
    fn enter_puts_a_picked_slide_on_screen_wherever_it_is_pressed() {
        let (stops, _) = service();
        let picked = SlideRef { presentation: 9, slide: 3 };
        for listed in [Listed::Playlist(Some(3)), Listed::Playlist(None), Listed::Library, Listed::Bible] {
            assert_eq!(enter(&stops, listed, Some(picked)), Some(Do::Show(Command::GoTo(picked))), "{listed:?}");
        }
    }

    #[test]
    fn while_a_slide_is_picked_next_and_previous_go_on_with_what_is_on_screen() {
        // Alpha on screen, Beta opened from the find box with a slide of it picked: the keys
        // run Alpha on, and leave Beta for Enter.
        let playlist = service();
        let mut s = situation(&playlist, Here::Item(2), Some(3), false, false);
        s.picked = Some("slide");
        assert_eq!(step(&s, true), Do::Show(Command::Next));
        assert_eq!(step(&s, false), Do::Show(Command::Previous));
        s.at_end = true;
        assert_eq!(step(&s, true), Do::Start { item: 3, last: false }, "Continuous goes on into the next song");
        // A song from a library on screen: it goes on as ever.
        let mut s = situation(&playlist, Here::Elsewhere, None, false, false);
        s.picked = Some("verse");
        assert_eq!(step(&s, true), Do::Show(Command::Next));
    }

    #[test]
    fn while_a_slide_is_picked_with_nothing_on_screen_next_puts_nothing_on() {
        let said = Do::Say("Press Enter to put the picked verse on screen.".to_owned());
        let playlist = service();
        for (here, open) in [(Here::Nothing, Some(3)), (Here::Nothing, None), (Here::Item(2), Some(3))] {
            let mut s = situation(&playlist, here, open, false, false);
            (s.picked, s.showing) = (Some("verse"), false);
            assert_eq!(step(&s, true), said, "{here:?}");
            assert_eq!(step(&s, false), said, "{here:?}");
        }
        assert_eq!(held_for_enter(Some("verse"), false), Some(said));
        assert_eq!(held_for_enter(Some("verse"), true), None, "something on screen goes on");
        assert_eq!(held_for_enter(None, false), None, "nothing picked: as ever");
    }

    #[test]
    fn up_and_down_move_the_pick_through_the_passage_and_stay_at_its_ends() {
        // Five verses. From the one picked.
        assert_eq!(move_pick(5, Some(1), None, true), Ok(Some(2)));
        assert_eq!(move_pick(5, Some(1), None, false), Ok(Some(0)));
        assert_eq!(move_pick(5, Some(4), Some(0), true), Ok(None), "it stays at the last verse");
        assert_eq!(move_pick(5, Some(0), None, false), Ok(None), "and at the first");
        // None picked: from the verse on screen.
        assert_eq!(move_pick(5, None, Some(2), true), Ok(Some(3)));
        assert_eq!(move_pick(5, None, Some(2), false), Ok(Some(1)));
        // Neither: the first, or the last.
        assert_eq!(move_pick(5, None, None, true), Ok(Some(0)));
        assert_eq!(move_pick(5, None, None, false), Ok(Some(4)));
        assert!(move_pick(0, None, None, true).is_err(), "no passage");
    }

    #[test]
    fn with_nothing_on_screen_space_takes_the_pick_and_the_arrows_move_it() {
        // Nothing on screen, a verse picked: Space puts it on, → and ← move the pick.
        assert_eq!(pick_key(true, false, false, true, false, false), PickKey::TakeIt);
        assert_eq!(pick_key(false, true, false, true, false, false), PickKey::Move(true));
        assert_eq!(pick_key(false, false, true, true, false, false), PickKey::Move(false));
        // A song on screen: Space and the arrows go on running it; Enter takes the pick.
        assert_eq!(pick_key(true, false, false, true, true, false), PickKey::Pass);
        assert_eq!(pick_key(false, true, false, true, true, false), PickKey::Pass);
        // No pick, or in an editor: as ever.
        assert_eq!(pick_key(true, false, false, false, false, false), PickKey::Pass);
        assert_eq!(pick_key(true, false, false, true, false, true), PickKey::Pass);
    }

    #[test]
    fn the_pick_moves_in_the_order_sung_and_stops_at_the_ends() {
        // An arrangement that sings slide 2 (the chorus) twice.
        let order = [0, 2, 1, 2, 3];
        assert_eq!(pick_beside(&order, 0, true), Some(2));
        assert_eq!(pick_beside(&order, 1, true), Some(2));
        assert_eq!(pick_beside(&order, 1, false), Some(2));
        assert_eq!(pick_beside(&order, 3, true), None, "the last");
        assert_eq!(pick_beside(&order, 0, false), None, "the first");
        assert_eq!(pick_beside(&order, 9, true), Some(0), "not in it: the first");
    }
}
