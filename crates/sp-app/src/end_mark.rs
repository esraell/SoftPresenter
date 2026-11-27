//! The end of a song's mark: three stars under the words of a song's last slide as it is sung,
//! on the audience screens, as SoftProjector draws them. Here is when the mark is due and how
//! far down the slide it may reach; the painter puts it under the words as each screen's theme
//! lays them out (`sp_model::Slide::end_mark`).

use sp_model::Presentation;

/// The mark is never drawn into the lowest 3% of the slide.
const FOOT: f32 = 0.03;

/// Whether the slide on screen gets the mark: Settings has it `on`, the slide is a song's (not
/// one the app made: a Bible passage, the VoxLive captions, a song of the song catalog), its
/// words aren't hidden, the song doesn't go round, and the slide is the last one sung
/// ([`last_sung`]): `step` is its place in the song's play order, so a chorus written once and
/// sung after each verse is marked only when it is sung last.
pub fn due(on: bool, app_made: bool, words_hidden: bool, song: &Presentation, step: Option<usize>) -> bool {
    on && !app_made && !words_hidden && !song.loops && step.is_some() && step == last_sung(song)
}

/// The place in a song's play order of the last slide with words: the blank slides many songs
/// end on (those brought in from other programs, often) are passed over, so the mark goes under
/// the song's real last words. `None` for a song with no words at all.
pub fn last_sung(song: &Presentation) -> Option<usize> {
    let worded = |slide: &usize| {
        song.slide(*slide).is_some_and(|(_, slide)| slide.texts().iter().any(|(_, words)| !words.trim().is_empty()))
    };
    song.order().iter().rposition(worded)
}

/// How far down the slide the mark may reach, in slide units: 3% above the slide's bottom, or
/// the copyright line's top while that shows (`copyright`). With no room above it, the painter
/// leaves the mark out; the words are never moved or made smaller for it.
pub fn floor(copyright: bool) -> f32 {
    if copyright { crate::props::COPYRIGHT_TOP } else { sp_model::SLIDE_HEIGHT * (1.0 - FOOT) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_engine::{Command, Show, SlideRef};
    use sp_model::{Arrangement, Uuid, parse_song_text};
    use std::sync::Arc;

    /// Whether the mark is due on the slide on screen, with Settings' switch on and the song the
    /// library's own.
    fn due_on(show: &Show) -> bool {
        let at = show.live().expect("a slide is on screen");
        let step = show.position().map(|(step, _)| step);
        due(true, false, show.words_hidden(), &show.presentations()[at.presentation], step)
    }

    fn at(slide: usize) -> Command {
        Command::GoTo(SlideRef { presentation: 0, slide })
    }

    #[test]
    fn the_mark_is_due_on_the_sung_last_slide_only() {
        // Written verse, chorus, verse; sung verse, chorus, verse, chorus.
        let mut song = parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nSing\n\n[Verse 2]\nTwo\n");
        let ids: Vec<Uuid> = song.groups.iter().map(|group| group.id).collect();
        assert_eq!(ids.len(), 3);
        let sung = Arrangement {
            id: Uuid::new_v4(),
            name: "Sung".to_owned(),
            groups: vec![ids[0], ids[1], ids[2], ids[1]],
            auto: false,
        };
        song.arrangement = Some(sung.id);
        song.arrangements.push(sung);
        assert_eq!(song.order(), [0, 1, 2, 1]);
        let mut show = Show::new(vec![Arc::new(song.clone())]);
        show.apply(at(0));
        let mut marked = Vec::new();
        loop {
            marked.push((show.live().map(|at| at.slide), due_on(&show)));
            if show.upcoming().is_none() {
                break;
            }
            show.apply(Command::Next);
        }
        // The chorus after the first verse isn't the end; the same slide sung last is. The
        // written last slide (verse 2) never is.
        assert_eq!(marked, [(Some(0), false), (Some(1), false), (Some(2), false), (Some(1), true)]);
        // The chorus clicked takes its next place in the order, as Next would reach it: from
        // the first verse that is the chorus sung first, and after the second verse the end.
        let mut clicked = Show::new(vec![Arc::new(song.clone())]);
        clicked.apply(at(0));
        clicked.apply(at(1));
        assert!(!due_on(&clicked), "the chorus clicked after verse 1");
        clicked.apply(at(2));
        clicked.apply(at(1));
        assert!(due_on(&clicked), "the chorus clicked after verse 2");
        // Its words hidden, the mark goes with them, and comes back with them.
        show.apply(Command::HideWords);
        assert!(show.words_hidden() && !due_on(&show));
        show.apply(Command::ShowAgain);
        assert!(due_on(&show));
        // Settings' switch off: never.
        let last = Some(song.order().len() - 1);
        assert_eq!(last_sung(&song), last);
        assert!(due(true, false, false, &song, last));
        assert!(!due(false, false, false, &song, last));
        // A slide whose place in the order isn't known is not taken for the last.
        assert!(!due(true, false, false, &song, None));

        // Sung as written, the written last slide is the one.
        let plain = parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nSing\n\n[Verse 2]\nTwo\n");
        let mut show = Show::new(vec![Arc::new(plain)]);
        show.apply(at(1));
        assert!(!due_on(&show));
        show.apply(at(2));
        assert!(due_on(&show));

        // A song that ends on a blank slide, or two (as songs brought in from other programs
        // often do): its last slide with words is the one, and the blank ones never are.
        for blanks in [1, 2] {
            let mut ended = parse_song_text("Song", "[Verse 1]\nOne\n\n[Verse 2]\nTwo\n");
            let last = ended.groups.last_mut().expect("a group");
            // A slide with nothing on it, then one whose box has no words in it.
            last.slides.push(sp_model::Slide::new(Vec::new()));
            if blanks == 2 {
                last.slides.push(last.slides[0].empty_copy());
            }
            assert_eq!(ended.slide_count(), 2 + blanks);
            assert_eq!(last_sung(&ended), Some(1), "{blanks} blank at the end");
            let mut show = Show::new(vec![Arc::new(ended)]);
            let marked: Vec<bool> = (0..2 + blanks)
                .map(|slide| {
                    show.apply(at(slide));
                    due_on(&show)
                })
                .collect();
            let mut want = vec![false; 2 + blanks];
            want[1] = true;
            assert_eq!(marked, want, "{blanks} blank at the end");
        }
        // A blank slide in the middle is no end, and a song with no words has none.
        let mut gap = parse_song_text("Song", "One\n\nTwo\n");
        gap.groups[0].slides.insert(1, sp_model::Slide::new(Vec::new()));
        assert_eq!(last_sung(&gap), Some(2));
        let mut wordless = parse_song_text("Pictures", "One\n");
        wordless.groups[0].slides = vec![sp_model::Slide::new(Vec::new()); 2];
        assert_eq!(last_sung(&wordless), None);
        assert!(!due(true, false, false, &wordless, Some(1)));

        // A song of one slide: that slide.
        let mut show = Show::new(vec![Arc::new(parse_song_text("Short", "Only\n"))]);
        show.apply(at(0));
        assert!(due_on(&show));

        // A song that goes round has no end.
        let mut round = parse_song_text("Round", "One\n\nTwo\n");
        round.loops = true;
        let mut show = Show::new(vec![Arc::new(round)]);
        for slide in [0, 1] {
            show.apply(at(slide));
            assert!(!due_on(&show), "slide {slide} of a looping song");
        }

        // What the app made (a Bible passage, the VoxLive captions while they are up, a song of
        // the song catalog) is no song of the library's: never.
        let passage = parse_song_text("John 3", "For God so loved the world\n");
        assert!(due(true, false, false, &passage, Some(0)));
        assert!(!due(true, true, false, &passage, Some(0)), "a Bible passage, or VoxLive's captions");
    }

    #[test]
    fn the_end_mark_stops_3_percent_above_the_foot_or_at_the_copyright_line() {
        assert_eq!(floor(false), sp_model::SLIDE_HEIGHT * 0.97);
        let line = crate::props::copyright_slide("A. Writer");
        let top = line.objects[0].frame.expect("the line has its place").y;
        assert_eq!(floor(true), top);
        assert!(floor(true) < floor(false));
    }
}
