use std::sync::Arc;

use sp_model::parse_song_text;

use super::*;

fn show() -> Show {
    Show::new(vec![Arc::new(parse_song_text("One", "A\n\nB\n\nC\n")), Arc::new(parse_song_text("Two", "D\n\nE\n"))])
}

fn at(presentation: usize, slide: usize) -> Option<SlideRef> {
    Some(SlideRef { presentation, slide })
}

#[test]
fn starts_with_nothing_live() {
    let s = show();
    assert_eq!(s.live(), None);
    assert!(s.output_enabled());
    assert_eq!(s.selected(), 0);
}

#[test]
fn the_version_changes_when_a_song_is_added_or_replaced_and_only_then() {
    let mut s = show();
    let first = s.version();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    s.apply(Command::Select(1));
    assert_eq!(s.version(), first, "going live and selecting change no song");
    s.add_presentation(Arc::new(parse_song_text("Three", "F\n")));
    let added = s.version();
    assert_ne!(added, first);
    s.replace_presentation(2, Arc::new(parse_song_text("Three", "G\n")));
    assert_ne!(s.version(), added);
    // Another show never shares a number with this one.
    assert_ne!(show().version(), s.version());
}

#[test]
fn goto_reports_slide_change() {
    let mut s = show();
    let c = s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(c.slides);
    assert_eq!(s.live(), at(0, 1));
    assert_eq!(s.live_slide().unwrap().1.objects[0].as_text(), Some("B"));
}

#[test]
fn goto_the_live_slide_reports_nothing() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 })).slides);
}

#[test]
fn next_with_nothing_live_goes_to_first_of_selected() {
    let mut s = show();
    s.apply(Command::Select(1));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(1, 0));
}

#[test]
fn next_and_previous_stop_at_the_ends() {
    let mut s = show();
    for _ in 0..9 {
        s.apply(Command::Next);
    }
    assert_eq!(s.live(), at(0, 2));
    assert!(!s.apply(Command::Next).slides);
    for _ in 0..9 {
        s.apply(Command::Previous);
    }
    assert_eq!(s.live(), at(0, 0));
    assert!(!s.apply(Command::Previous).slides);
}

#[test]
fn clear_then_next_continues() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    assert!(s.apply(Command::Clear(Layer::Slides)).slides);
    assert_eq!(s.live(), None);
    assert_eq!(s.cursor(), at(0, 0));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn clear_on_the_last_slide_then_next_shows_it_again() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    s.apply(Command::ClearAll);
    assert!(s.apply(Command::Next).slides);
    assert_eq!(s.live(), at(0, 2));
}

#[test]
fn clear_then_previous_goes_back_one() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    s.apply(Command::ClearAll);
    s.apply(Command::Previous);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn previous_with_no_cursor_does_nothing() {
    let mut s = show();
    assert!(!s.apply(Command::Previous).slides);
    assert_eq!(s.live(), None);
}

#[test]
fn clear_when_already_clear_reports_nothing() {
    let mut s = show();
    assert!(!s.apply(Command::ClearAll).slides);
}

#[test]
fn clearing_another_layer_keeps_the_slide() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(!s.apply(Command::Clear(Layer::Media)).slides);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn select_does_not_change_live() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    let c = s.apply(Command::Select(1));
    assert!(c.selection && !c.slides);
    assert_eq!(s.live(), at(0, 2));
    assert_eq!(s.selected(), 1);
}

#[test]
fn next_follows_the_live_song_not_the_selected_one() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    s.apply(Command::Select(1));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn first_and_last_use_the_selected_presentation() {
    let mut s = show();
    s.apply(Command::Select(1));
    s.apply(Command::Last);
    assert_eq!(s.live(), at(1, 1));
    s.apply(Command::First);
    assert_eq!(s.live(), at(1, 0));
}

#[test]
fn out_of_range_is_ignored() {
    let mut s = show();
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 5, slide: 0 })).slides);
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 3 })).slides);
    assert!(!s.apply(Command::Select(9)).selection);
    assert_eq!(s.live(), None);
}

#[test]
fn output_toggle() {
    let mut s = show();
    assert!(s.apply(Command::SetOutputEnabled(false)).output_enabled);
    assert!(!s.apply(Command::SetOutputEnabled(false)).output_enabled);
    assert!(!s.output_enabled());
}

#[test]
fn empty_show_never_panics() {
    let mut s = Show::new(vec![]);
    for c in [Command::Next, Command::Previous, Command::First, Command::Last, Command::ClearAll, Command::Select(0)] {
        assert_eq!(s.apply(c), Changes::default());
    }
}

#[test]
fn a_song_without_slides_is_skipped() {
    let mut s = Show::new(vec![Arc::new(parse_song_text("Empty", ""))]);
    for c in [Command::Next, Command::First, Command::Last] {
        assert!(!s.apply(c).slides);
    }
}

fn picture(name: &str) -> sp_model::MediaItem {
    sp_model::MediaItem::new(name.to_owned(), name.into())
}

#[test]
fn media_is_added_in_order_and_nothing_is_live() {
    let mut s = show();
    assert_eq!(s.add_media(picture("a.png")), 0);
    assert_eq!(s.add_media(picture("b.png")), 1);
    assert_eq!(s.media().len(), 2);
    assert_eq!(s.live_media(), None);
}

#[test]
fn showing_media_reports_a_media_change_and_leaves_the_slide() {
    let mut s = show();
    s.add_media(picture("a.png"));
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    let c = s.apply(Command::ShowMedia(0));
    assert!(c.media && !c.slides);
    assert_eq!(s.live_media(), Some(0));
    assert_eq!(s.live(), at(0, 1));
    assert!(!s.apply(Command::ShowMedia(0)).media, "showing what is already showing changes nothing");
    assert!(!s.apply(Command::ShowMedia(7)).media, "there is no media 7");
}

#[test]
fn layers_clear_separately_and_clear_all_clears_both() {
    let mut s = show();
    s.add_media(picture("a.png"));
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    s.apply(Command::ShowMedia(0));

    let c = s.apply(Command::Clear(Layer::Media));
    assert!(c.media && !c.slides);
    assert_eq!(s.live_media(), None);
    assert_eq!(s.live(), at(0, 0));

    s.apply(Command::ShowMedia(0));
    let c = s.apply(Command::Clear(Layer::Slides));
    assert!(c.slides && !c.media);
    assert_eq!(s.live_media(), Some(0));

    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    let c = s.apply(Command::ClearAll);
    assert!(c.slides && c.media);
    assert_eq!((s.live(), s.live_media()), (None, None));
    assert_eq!(s.apply(Command::ClearAll), Changes::default());
}

#[test]
fn anything_live_says_whether_the_screen_is_empty() {
    let mut s = show();
    assert!(!s.anything_live());
    s.add_media(picture("a.png"));
    s.apply(Command::ShowMedia(0));
    assert!(s.anything_live());
    s.apply(Command::ClearAll);
    s.apply(Command::Next);
    assert!(s.anything_live());
}

#[test]
fn replacing_a_presentation_keeps_what_is_live_when_it_still_exists() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    // Same number of slides: the live slide stays, now with the new words.
    let c = s.replace_presentation(0, Arc::new(parse_song_text("One", "A\n\nB\n\nEdited\n")));
    assert!(c.slides, "the live slide's words changed");
    assert_eq!(s.live(), at(0, 2));
    assert_eq!(s.live_slide().unwrap().1.objects[0].as_text(), Some("Edited"));
    // Fewer slides: the live slide is gone, so the layer is cleared.
    let c = s.replace_presentation(0, Arc::new(parse_song_text("One", "A\n")));
    assert!(c.slides);
    assert_eq!(s.live(), None);
    assert_eq!(s.cursor(), None);
    // Out of range: nothing happens.
    assert_eq!(s.replace_presentation(9, Arc::new(parse_song_text("X", "A\n"))), Changes::default());
}

#[test]
fn replacing_a_presentation_that_is_not_live_changes_no_layer() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    let c = s.replace_presentation(1, Arc::new(parse_song_text("Two", "New\n")));
    assert!(!c.slides);
    assert_eq!(s.presentations()[1].slide(0).unwrap().1.objects[0].as_text(), Some("New"));
}

#[test]
fn disabled_slides_are_stepped_over_and_cannot_be_shown() {
    let mut song = parse_song_text("One", "[Verse 1]\nA\n\nB\n\nC\n\n[Chorus]\nD\n\nE\n");
    // B and D are disabled.
    song.groups[0].slides[1].enabled = false;
    song.groups[1].slides[0].enabled = false;
    let mut s = Show::new(vec![Arc::new(song)]);
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 0));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 2), "B is skipped");
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 4), "D is skipped");
    assert!(!s.apply(Command::Next).slides, "the end of the song");
    s.apply(Command::Previous);
    assert_eq!(s.live(), at(0, 2));
    // Clicking a disabled slide does nothing.
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 })).slides);
    // The chorus key lands on the chorus's first slide that is not disabled.
    assert_eq!(s.group_start(&["chorus"]), at(0, 4));
}

#[test]
fn removing_or_adding_a_slide_before_the_live_one_keeps_it_live() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    s.slides_shifted(0, 0, false);
    assert_eq!(s.live(), at(0, 1));
    s.slides_shifted(0, 0, true);
    assert_eq!(s.live(), at(0, 2));
    // Another song's slides do not move it.
    s.slides_shifted(1, 0, false);
    assert_eq!(s.live(), at(0, 2));
    // Removing the live slide takes it off the screen.
    assert!(s.slides_shifted(0, 2, false).slides);
    assert_eq!(s.live(), None);
}

#[test]
fn removing_a_picture_keeps_the_one_on_screen_or_takes_it_off() {
    let mut s = show();
    for name in ["a", "b", "c"] {
        s.add_media(sp_model::MediaItem::new(name.to_owned(), name.into()));
    }
    s.apply(Command::ShowMedia(2));
    s.remove_media(0);
    assert_eq!((s.live_media(), s.media().len()), (Some(1), 2));
    assert!(s.remove_media(1).media);
    assert_eq!(s.live_media(), None);
}

#[test]
fn a_replaced_presentation_keeps_the_live_slide_by_its_id() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    let before = s.presentations()[0].clone();
    // The first slide is removed, then put back: B stays live through both.
    let shorter = Arc::new(before.without_slide(0).unwrap());
    assert!(s.replace_presentation_following(0, shorter).slides);
    assert_eq!(s.live(), at(0, 0));
    assert_eq!(s.live_slide().unwrap().1.objects[0].as_text(), Some("B"));
    s.replace_presentation_following(0, before.clone());
    assert_eq!(s.live(), at(0, 1));
    assert_eq!(s.live_slide().unwrap().1.objects[0].as_text(), Some("B"));
    // A version without the live slide takes it off the screen.
    let without_b = Arc::new(before.without_slide(1).unwrap());
    s.replace_presentation_following(0, without_b);
    assert_eq!(s.live(), None);
}

/// Verse 1, Chorus, Verse 2 (one slide each), arranged Verse 1, Chorus, Verse 2, Chorus.
fn arranged() -> Show {
    let mut song = parse_song_text("Song", "[Verse 1]\nA\n\n[Chorus]\nC\n\n[Verse 2]\nD\n");
    let (v1, chorus, v2) = (song.groups[0].id, song.groups[1].id, song.groups[2].id);
    let mut arrangement = song.new_arrangement(|n| format!("Arrangement {n}"));
    arrangement.groups = vec![v1, chorus, v2, chorus];
    song.arrangement = Some(arrangement.id);
    song.arrangements.push(arrangement);
    Show::new(vec![Arc::new(song)])
}

#[test]
fn next_follows_the_arrangement_and_knows_which_chorus_it_is_on() {
    let mut s = arranged();
    s.apply(Command::Next);
    assert_eq!((s.live(), s.position()), (at(0, 0), Some((0, 4))));
    s.apply(Command::Next);
    assert_eq!((s.live(), s.position()), (at(0, 1), Some((1, 4))), "the first chorus");
    assert_eq!(s.upcoming(), at(0, 2));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 2));
    s.apply(Command::Next);
    assert_eq!((s.live(), s.position()), (at(0, 1), Some((3, 4))), "the second chorus");
    assert_eq!(s.upcoming(), None, "the end of the arrangement");
    assert!(!s.apply(Command::Next).slides);
    s.apply(Command::Previous);
    assert_eq!(s.live(), at(0, 2));
    // Clicking the chorus from Verse 2 lands on the chorus after it, and Last on the last one.
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert_eq!(s.position(), Some((3, 4)));
    s.apply(Command::First);
    s.apply(Command::GoToStep { presentation: 0, step: 1 });
    assert_eq!((s.live(), s.position()), (at(0, 1), Some((1, 4))));
    s.apply(Command::Last);
    assert_eq!(s.position(), Some((3, 4)));
}

#[test]
fn a_disabled_slide_is_passed_over_in_the_arrangement() {
    let mut s = arranged();
    let mut song = (*s.presentations()[0]).clone();
    song.groups[2].slides[0].enabled = false;
    s.replace_presentation(0, Arc::new(song));
    s.apply(Command::GoToStep { presentation: 0, step: 1 });
    // Verse 2 is disabled: Next goes from the first chorus to the second, the same slide.
    assert!(!s.apply(Command::Next).slides, "the same slide stays on screen");
    assert_eq!(s.position(), Some((3, 4)));
}

#[test]
fn a_deleted_group_or_an_undo_keeps_the_place_in_the_arrangement() {
    let mut s = arranged();
    s.apply(Command::GoToStep { presentation: 0, step: 3 });
    let before = s.presentations()[0].clone();
    // Verse 2 is deleted: the arrangement is Verse 1, Chorus, Chorus, and the second chorus is on.
    let without_v2 = Arc::new(before.without_slide(2).unwrap());
    s.replace_presentation_following(0, without_v2);
    assert_eq!((s.live(), s.position()), (at(0, 1), Some((2, 3))));
    // Undo puts Verse 2 back; the second chorus is still the one on screen.
    s.replace_presentation_following(0, before);
    assert_eq!((s.live(), s.position()), (at(0, 1), Some((3, 4))));
}

#[test]
fn next_sets_a_slides_click_builds_going_before_moving_on() {
    let mut song = parse_song_text("Song", "A\n\nB\n");
    let click = sp_model::Build { on_click: true, ..sp_model::Build::of(sp_model::BuildEffect::Dissolve) };
    song.groups[0].slides[0].objects[0].build_in = Some(click);
    let mut s = Show::new(vec![Arc::new(song)]);
    s.apply(Command::Next);
    assert_eq!((s.live(), s.build_step(), s.builds_left()), (at(0, 0), 0, true));
    let changes = s.apply(Command::Next);
    assert!(changes.builds && !changes.slides);
    assert_eq!((s.live(), s.build_step(), s.builds_left()), (at(0, 0), 1, false));
    // Previous takes the build back before going to the slide before.
    assert!(s.apply(Command::Previous).builds);
    assert_eq!(s.build_step(), 0);
    s.apply(Command::Next);
    s.apply(Command::Next);
    assert_eq!((s.live(), s.build_step()), (at(0, 1), 0), "a new slide starts with no builds done");
}

#[test]
fn a_song_that_loops_goes_back_to_its_first_slide() {
    let mut song = parse_song_text("Notices", "A\n\nB\n\nC\n");
    song.loops = true;
    let mut s = Show::new(vec![Arc::new(song)]);
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    assert_eq!(s.upcoming(), at(0, 0));
    assert!(s.apply(Command::Next).slides);
    assert_eq!(s.live(), at(0, 0));
}

#[test]
fn the_slide_on_screen_follows_its_slide_when_slides_are_moved() {
    let mut show = show();
    show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    // Slide 0 went to place 2; the others moved up.
    show.slides_reordered(0, &[2, 0, 1]);
    assert_eq!(show.live(), at(0, 2));
    // Another presentation's slides moving leaves it alone.
    show.slides_reordered(1, &[1, 0]);
    assert_eq!(show.live(), at(0, 2));
}

#[test]
fn preceding_is_the_slide_previous_would_show_and_none_at_the_start() {
    let mut s = show();
    assert_eq!(s.preceding(), None, "nothing shown yet");
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    assert_eq!(s.preceding(), at(0, 1));
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    assert_eq!(s.preceding(), None, "the start of the song");
    s.apply(Command::ClearAll);
    assert_eq!(s.preceding(), None, "still the start after a clear");
}
