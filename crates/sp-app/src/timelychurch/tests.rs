//! Every song, setlist and plan here is made up for these tests, in the shapes TimelyChurch's
//! tools give (with fields about people left empty); none of the words are a real song's.

use std::collections::HashMap;

use sp_model::{Presentation, Role};

use super::*;
use crate::catalog::{self, Verdict};
use crate::service::Item;

const CHURCH: &str = "https://Test-Church.example.org/";

fn song_json(id: u64, title: &str) -> Song {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "title": title,
        "artist": "Test Singers",
        "composer": null,
        "ccli_number": null,
        "copyright_info": null,
        "default_key": "G",
        "tempo": null,
        "bpm": 70,
        "status": "approved",
        "tags": [],
        "themes": ["test"],
        "use_count": 0,
        "notes": null,
        "lyrics": "Not used: the words come from the chart",
        "arrangements": [
            {"id": id * 10, "name": "Default Arrangement", "key": "G", "available_keys": [], "bpm": 70,
             "duration_seconds": null, "is_default": true, "chart_format": "chordpro"}
        ],
        "attachments": [],
        "recent_usages": [{"used_at": "2026-01-01", "key_used": "G", "service_order_id": null}],
        "result_row_count": 2,
        "entity_ids": [id]
    }))
    .expect("a song reads")
}

fn chart(song: u64, sections: serde_json::Value) -> Chart {
    serde_json::from_value(serde_json::json!({
        "song_id": song,
        "title": "ignored",
        "arrangement_id": song * 10,
        "key": "G",
        "sections": sections,
        "result_row_count": 1,
        "entity_ids": [song]
    }))
    .expect("a chart reads")
}

/// The groups' names, and each group's slides as their main words.
fn shape(song: &Presentation) -> Vec<(String, Vec<String>)> {
    song.groups
        .iter()
        .map(|group| {
            let slides =
                group.slides.iter().map(|slide| slide.text_for(&Role::primary()).unwrap_or("").to_owned()).collect();
            (group.name.clone(), slides)
        })
        .collect()
}

/// The groups' names in the order sung.
fn sung(song: &Presentation) -> Vec<String> {
    let name =
        |id: &sp_model::Uuid| song.groups.iter().find(|g| g.id == *id).map(|g| g.name.clone()).unwrap_or_default();
    match song.active_arrangement() {
        Some(arrangement) => arrangement.groups.iter().map(name).collect(),
        None => song.groups.iter().map(|g| g.name.clone()).collect(),
    }
}

fn translations(song: &Presentation) -> Vec<Option<String>> {
    song.groups
        .iter()
        .flat_map(|g| g.slides.iter().map(|s| s.text_for(&Role::translation()).map(str::to_owned)))
        .collect()
}

fn make(chart: &Chart) -> MadeSong {
    make_song(CHURCH, &song_json(chart.song_id, "Morning Test Song"), chart, None, sp_model::MOST_LINES, 0)
}

#[test]
fn a_chart_becomes_groups_and_slides_and_a_chorus_written_again_is_sung_again() {
    let chart = chart(
        1,
        serde_json::json!([
            {"name": null, "lines": ["[Intro:] [G] [D] [Em] [C]"]},
            {"name": "Verse 1", "lines": [
                "[G]First line of the [D]verse", "Second line of the verse", "",
                "Third line [Em]after a break", "Fourth line [C]after a break", ""]},
            {"name": "Chorus", "lines": ["[C]Sing the test [G]chorus", "Sing it [D]once again", ""]},
            {"name": "Verse 2", "lines": ["Another verse begins", "And it ends here", ""]},
            {"name": "Chorus", "lines": ["[C]Sing the test [G]chorus", "Sing it [D]once again", ""]}
        ]),
    );
    let made = make(&chart).song;
    assert_eq!(
        shape(&made),
        [
            (
                "Verse 1".to_owned(),
                vec![
                    "First line of the verse\nSecond line of the verse".to_owned(),
                    "Third line after a break\nFourth line after a break".to_owned()
                ]
            ),
            ("Chorus".to_owned(), vec!["Sing the test chorus\nSing it once again".to_owned()]),
            ("Verse 2".to_owned(), vec!["Another verse begins\nAnd it ends here".to_owned()]),
        ],
        "chords and the chord-only intro are gone; a blank line starts a slide; the chorus is one group"
    );
    assert_eq!(sung(&made), ["Verse 1", "Chorus", "Verse 2", "Chorus"], "the order sung is the song's arrangement");
    assert_eq!(made.active_arrangement().map(|a| a.name.as_str()), Some("TimelyChurch"));
}

#[test]
fn a_song_sung_as_written_has_no_arrangement() {
    let chart = chart(
        2,
        serde_json::json!([
            {"name": "Verse 1", "lines": ["One line", ""]},
            {"name": "Verse 2", "lines": ["Two line", ""]}
        ]),
    );
    let made = make(&chart).song;
    assert!(made.arrangements.is_empty());
    assert_eq!(made.arrangement, None);
}

#[test]
fn repeats_are_read_however_the_chart_writes_them() {
    let chart = chart(
        3,
        serde_json::json!([
            {"name": "Verse 1 - 2x", "lines": ["[G]A verse sung twice", ""]},
            {"name": "Chorus", "lines": ["The chorus line", ""]},
            {"name": "Verse 2", "lines": ["||: [G]Sing this twice", "Then [D]on we go :||", ""]},
            {"name": "Chorus", "lines": []},
            {"name": "Bridge (3x)", "lines": ["A bridge line"]},
            {"name": "Chorus x2", "lines": []},
            {"name": "Tag", "lines": ["Last words", "", "Chorus: 3x", "The chorus line"]}
        ]),
    );
    let made = make(&chart).song;
    assert_eq!(
        sung(&made),
        ["Verse 1", "Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Bridge", "Bridge", "Chorus", "Chorus", "Tag"]
            .into_iter()
            .chain(["Chorus"; 3])
            .collect::<Vec<_>>()
    );
    let names: Vec<String> = made.groups.iter().map(|g| g.name.clone()).collect();
    assert_eq!(names, ["Verse 1", "Chorus", "Verse 2", "Bridge", "Tag"], "a count is not part of the name");
    // Repeat signs are for the musicians: the words stay, the signs go.
    assert_eq!(shape(&made)[2].1, ["Sing this twice\nThen on we go"]);
}

#[test]
fn a_chart_with_its_sections_named_in_the_words_is_read_by_those_names() {
    // How a chart with the chords above the words comes: one section with no name, the
    // sections' names as lines of their own, musicians' notes among the words.
    let chart = chart(
        4,
        serde_json::json!([{"name": null, "lines": [
            "Words before any name", "Second opening line", "",
            "VERSE 1", "Verse words here", "",
            "Chorus", "Chorus words here", "",
            "Play Verse 1", "",
            "Chorus 2x", "",
            "Ending", "Final words", "End"
        ]}]),
    );
    let made = make(&chart).song;
    let names: Vec<String> = made.groups.iter().map(|g| g.name.clone()).collect();
    assert_eq!(names, ["", "Verse 1", "Chorus", "Ending"]);
    assert_eq!(sung(&made), ["", "Verse 1", "Chorus", "Chorus", "Chorus", "Ending"]);
    assert_eq!(shape(&made)[3].1, ["Final words"], "\"End\" is a note to the band, not a line to sing");
}

#[test]
fn sections_named_in_russian_and_unbracketed_chord_lines() {
    let chart = chart(
        5,
        serde_json::json!([
            {"name": null, "lines": ["INTRO: Am – F – C/E – G", "куплет 1", "Свет над рекой для проверки", "",
                                     "ПРИПЕВ", "Пой, проверка, пой", "F-----------"]},
            {"name": "Припев 2х", "lines": []},
            {"name": "Outro", "lines": ["OUTRO: Am F G C x2"]}
        ]),
    );
    let made = make(&chart).song;
    assert_eq!(
        shape(&made),
        [
            ("Куплет 1".to_owned(), vec!["Свет над рекой для проверки".to_owned()]),
            ("Припев".to_owned(), vec!["Пой, проверка, пой".to_owned()]),
        ],
        "a line of chords is never words; an outro of only chords is not a group"
    );
    assert_eq!(sung(&made), ["Куплет 1", "Припев", "Припев", "Припев"]);
}

#[test]
fn the_same_name_with_other_words_is_another_group() {
    let chart = chart(
        6,
        serde_json::json!([
            {"name": "Chorus", "lines": ["The chorus line", ""]},
            {"name": "Verse", "lines": ["A verse line", ""]},
            {"name": "Chorus", "lines": ["The chorus line", "With a last line added"]}
        ]),
    );
    let made = make(&chart).song;
    assert_eq!(made.groups.len(), 3);
    assert_eq!(sung(&made), ["Chorus", "Verse", "Chorus"]);
    assert_ne!(made.groups[0].id, made.groups[2].id);
    // As written: no arrangement is needed.
    assert!(made.arrangements.is_empty());
}

#[test]
fn long_stretches_are_shared_out_and_the_churchs_lines_per_slide_count() {
    let lines: Vec<String> = (1..=8).map(|n| format!("Line number {n}")).collect();
    let chart = chart(7, serde_json::json!([{"name": "Verse 1", "lines": lines}]));
    let sizes = |most: usize| {
        let made = make_song(CHURCH, &song_json(7, "T"), &chart, None, most, 0).song;
        made.groups[0]
            .slides
            .iter()
            .map(|s| s.text_for(&Role::primary()).unwrap_or("").lines().count())
            .collect::<Vec<_>>()
    };
    assert_eq!(sizes(lines_per_slide(None)), [4, 4]);
    let saved: StyleReply = serde_json::from_value(
        serde_json::json!({"style": {"lines_per_slide": 6, "layout": "original_first"}, "has_default": true}),
    )
    .expect("a style reads");
    assert_eq!(lines_per_slide(Some(&saved)), 6);
    let never_saved = StyleReply { has_default: false, ..saved.clone() };
    assert_eq!(
        lines_per_slide(Some(&never_saved)),
        sp_model::MOST_LINES,
        "TimelyChurch's own default isn't the church's"
    );
    let three = StyleReply { style: SlideStyle { lines_per_slide: 3, ..saved.style.clone() }, has_default: true };
    assert_eq!(sizes(lines_per_slide(Some(&three))), [3, 3, 2]);
}

/// A translation as TimelyChurch gives it: in pieces of at most six lines, crossing the
/// song's own breaks, every line of the song again (chords out, repeat signs kept).
fn translation(rows: &[(&str, &str)]) -> Translation {
    let sections: Vec<serde_json::Value> = rows
        .chunks(6)
        .map(|piece| {
            serde_json::json!({"name": "Verse 1", "caption": "Verse 1",
                "rows": piece.iter().map(|(s, t)| serde_json::json!({"source": s, "translated": t})).collect::<Vec<_>>()})
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "song_id": 8, "title": "ignored", "language": "EN", "status": null, "title_translated": null,
        "sections": sections, "languages": [], "result_row_count": 1, "entity_ids": [8]
    }))
    .expect("a translation reads")
}

fn bilingual_chart() -> Chart {
    chart(
        8,
        serde_json::json!([
            {"name": "Куплет 1", "lines": ["[Am]Первая строка", "||: Вторая [F]строка", "", "Третья строка :||", ""]},
            {"name": "Припев", "lines": ["Строка припева", "Ещё строка припева", ""]},
            {"name": "Куплет 2", "lines": ["Строка припева", "Новая строка", ""]},
            {"name": "Припев", "lines": ["Строка припева", "Ещё строка припева"]}
        ]),
    )
}

#[test]
fn a_translation_goes_under_the_words_line_by_line() {
    let rows = [
        ("Первая строка", "First line"),
        ("||: Вторая строка", "Second line"),
        ("Третья строка :||", ""),
        ("Строка припева", "Chorus line"),
        ("Ещё строка припева", "Another chorus line"),
        ("Строка припева", ""),
        ("Новая строка", "New line"),
        ("Строка припева", ""),
        ("Ещё строка припева", ""),
    ];
    let made = make_song(CHURCH, &song_json(8, "Т"), &bilingual_chart(), Some(&translation(&rows)), 4, 0);
    assert_eq!(
        translations(&made.song),
        [
            Some("First line\nSecond line".to_owned()),
            None,
            Some("Chorus line\nAnother chorus line".to_owned()),
            Some("Chorus line\nNew line".to_owned()),
        ],
        "each slide's own lines, in order; a line translated once is translated wherever it comes again"
    );
    assert_eq!(made.second.as_deref(), Some("en"));
    assert_eq!(made.untranslated, 1, "the one line nobody translated");
    assert_eq!(made.song.catalog.as_ref().and_then(|m| m.second.clone()).as_deref(), Some("en"));
}

#[test]
fn a_translation_with_nothing_translated_puts_nothing_under_the_words() {
    let rows = [("Первая строка", ""), ("||: Вторая строка", "  "), ("Строка припева", "")];
    let made = make_song(CHURCH, &song_json(8, "Т"), &bilingual_chart(), Some(&translation(&rows)), 4, 0);
    assert!(translations(&made.song).iter().all(Option::is_none));
    assert_eq!(made.second, None);
    assert_eq!(made.untranslated, 0);
}

#[test]
fn the_songs_details_title_notes_and_mark() {
    let mut song = song_json(9, "Faith &amp; Test");
    song.composer = Some("A. Composer".to_owned());
    song.ccli_number = Some(serde_json::json!("CCLI 0001234"));
    song.copyright_info = Some("© 2001 Test Music".to_owned());
    song.notes = Some("Start quietly".to_owned());
    let chart = chart(9, serde_json::json!([{"name": "Verse 1", "lines": ["A line"]}]));
    let made = make_song(CHURCH, &song, &chart, None, 4, 86_400 * 365).song;
    assert_eq!(made.title, "Faith & Test");
    assert_eq!(made.copyright.artist, "Test Singers");
    assert_eq!(made.copyright.author, "A. Composer");
    assert_eq!((made.copyright.year.as_str(), made.copyright.publisher.as_str()), ("2001", "Test Music"));
    assert_eq!(made.copyright.ccli_song, "0001234");
    assert_eq!(made.groups[0].slides[0].notes.as_deref(), Some("Start quietly"));
    let mark = made.catalog.as_ref().expect("marked");
    assert_eq!(mark.source, "timelychurch");
    assert_eq!(mark.key, "test-church.example.org/song/9/arrangement/90");
    assert_eq!(parse_song_key(&mark.key), Some(("test-church.example.org".to_owned(), 9, 90)));
    assert_eq!(mark.added, "1971-01-01");
    assert_eq!(mark.made_hash, catalog::fingerprint(&made));
    assert_eq!(mark.text_hash, mark.made_hash);
    // A second arrangement, not the default, is a song of its own, named for it.
    song.arrangements.push(
        serde_json::from_value(serde_json::json!({"id": 91, "name": "Youth", "is_default": false})).expect("reads"),
    );
    let youth = Chart { arrangement_id: 91, ..chart };
    assert_eq!(make_song(CHURCH, &song, &youth, None, 4, 0).song.title, "Faith & Test (Youth)");
}

#[test]
fn an_untouched_song_is_brought_up_to_date_and_a_changed_one_is_only_offered() {
    let words = |line: &str| chart(10, serde_json::json!([{"name": "Verse 1", "lines": [line]}]));
    let old = make(&words("The words as they were")).song;
    // Made again from the same data: the same fingerprint, whatever the new ids.
    let same = make(&words("The words as they were"));
    assert_eq!(catalog::judge(&old, &fresh_source(&same)), Verdict::Current);
    // TimelyChurch changed a line: a song nobody touched takes it, keeping its id.
    let newer = make(&words("The words as they are now"));
    assert_eq!(catalog::judge(&old, &fresh_source(&newer)), Verdict::Update);
    let updated = catalog::refreshed_song(&old, newer.song.clone());
    assert_eq!(updated.id, old.id);
    assert_eq!(shape(&updated)[0].1, ["The words as they are now"]);
    assert_eq!(catalog::judge(&updated, &fresh_source(&newer)), Verdict::Current);
    // The church changed the song in SoftPresenter: the newer version is only offered.
    let edited = old.with_slide_text(0, "The church's own words", "").expect("edits");
    let offered = catalog::judge(&edited, &fresh_source(&newer));
    assert!(matches!(offered, Verdict::Offer(_)), "{offered:?}");
}

#[test]
fn a_charts_text_splits_into_its_sections() {
    let text = "{title: Not a line}\n# a comment\nOpening words\n\n[Verse 1]\n[G]First [D]line\n[C]\n\n[Chorus]\nChorus line\n";
    let sections = sections_from_text(text);
    let names: Vec<Option<&str>> = sections.iter().map(|s| s.name.as_deref()).collect();
    assert_eq!(names, [None, Some("Verse 1"), Some("Chorus")], "a chord alone on its line is not a section");
    assert_eq!(sections[1].lines, ["[G]First [D]line", "[C]", ""]);
    let made = make(&Chart { sections, ..chart(11, serde_json::json!([])) }).song;
    assert_eq!(shape(&made)[1], ("Verse 1".to_owned(), vec!["First line".to_owned()]));
}

fn setlist(value: serde_json::Value) -> Setlist {
    serde_json::from_value(value).expect("a setlist reads")
}

fn setlist_song(order: u32, song: u64, title: &str, spot: Option<&str>) -> serde_json::Value {
    serde_json::json!({"order": order, "song_id": song, "title": title, "arrangement": null, "target_key": "G",
        "capo": null, "show_nashville_numbers": false, "notes": null, "worship_spot": spot})
}

fn sunday_setlist() -> Setlist {
    setlist(serde_json::json!({
        "id": 501, "name": "Test Sunday &amp; Communion", "description": null, "event_date": "2026-12-06",
        "team_name": "Test Team", "lead_names": [], "linked_event_id": 77, "is_active": true,
        "songs": [
            setlist_song(3, 103, "Third Test Song", Some("Communion")),
            setlist_song(1, 101, "First Test Song", Some("Opening")),
            setlist_song(2, 102, "Second Test Song", Some("Opening")),
            setlist_song(4, 101, "First Test Song", Some("Communion"))
        ],
        "rehearsals": [{"starts_at": null, "place": "Test room"}],
        "result_row_count": 4, "truncated": false, "entity_ids": [501]
    }))
}

/// The library: songs 101 and 103 are in it, 102 isn't yet.
fn library(song: u64, arrangement: Option<u64>) -> Option<String> {
    assert_eq!(arrangement, None);
    HashMap::from([(101, "id-101"), (103, "id-103")]).get(&song).map(|key| (*key).to_owned())
}

#[test]
fn a_setlist_becomes_a_playlist_in_its_order() {
    let planned = from_setlist(&sunday_setlist());
    assert_eq!(planned.name, "Test Sunday & Communion");
    assert_eq!(planned.date.as_deref(), Some("2026-12-06"));
    let service = to_service(&planned, library);
    assert_eq!(service.name, "Test Sunday & Communion");
    assert_eq!(
        service.items,
        [
            Item::Header { name: "Opening".to_owned() },
            Item::song("id-101", "First Test Song"),
            Item::Placeholder { name: "Second Test Song".to_owned() },
            Item::Header { name: "Communion".to_owned() },
            Item::song("id-103", "Third Test Song"),
        ],
        "in order; a song not in the library yet keeps its place; a song comes once"
    );
}

fn plan(items: serde_json::Value, event: Option<u64>) -> ServicePlan {
    serde_json::from_value(serde_json::json!({
        "id": 900, "title": "Test Service", "series": null, "service_type": "contemporary", "date": "2026-12-06",
        "time": "10:00:00", "status": "draft", "notes": null, "event_id": event, "items": items,
        "total_duration_seconds": 0, "result_row_count": 0, "truncated": false, "entity_ids": [900]
    }))
    .expect("a plan reads")
}

fn plan_item(order: u32, kind: &str, title: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut item = serde_json::json!({"order": order, "type": kind, "title": title, "description": null,
        "song_id": null, "song_title": null, "key": null, "capo": null, "duration_seconds": 300,
        "leader_name": null, "scripture_reference": null, "notes": null, "worship_spot": null});
    if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
        item.extend(extra.clone());
    }
    item
}

#[test]
fn a_plan_takes_its_setlists_songs_into_its_places_for_worship() {
    let plan = plan(
        serde_json::json!([
            plan_item(1, "section", "Welcome", serde_json::json!({})),
            plan_item(2, "worship", "OPENING", serde_json::json!({"worship_spot": "Opening"})),
            plan_item(3, "scripture", "Reading", serde_json::json!({"scripture_reference": "Psalm 100"})),
            plan_item(4, "sermon", "Message", serde_json::json!({})),
            plan_item(5, "worship", "Communion", serde_json::json!({"worship_spot": "communion"})),
            plan_item(6, "prayer", "", serde_json::json!({}))
        ]),
        Some(77),
    );
    let setlists =
        [setlist(serde_json::json!({"id": 1, "name": "Other day", "linked_event_id": 76})), sunday_setlist()];
    let linked = linked_setlist(&plan, &setlists);
    assert_eq!(linked.map(|s| s.id), Some(501), "the setlist for the same event");
    let service = to_service(&from_plan(&plan, linked), library);
    assert_eq!(
        service.items,
        [
            Item::Header { name: "Welcome".to_owned() },
            Item::Header { name: "Opening".to_owned() },
            Item::song("id-101", "First Test Song"),
            Item::Placeholder { name: "Second Test Song".to_owned() },
            Item::Placeholder { name: "Reading: Psalm 100".to_owned() },
            Item::Placeholder { name: "Message".to_owned() },
            Item::Header { name: "communion".to_owned() },
            Item::song("id-103", "Third Test Song"),
            Item::Placeholder { name: "Prayer".to_owned() },
        ]
    );
}

#[test]
fn without_a_place_for_worship_the_songs_fill_the_plans_song_places_and_the_rest_follow() {
    let plan = plan(
        serde_json::json!([
            plan_item(1, "song", "Song", serde_json::json!({})),
            plan_item(2, "announcement", "Announcements", serde_json::json!({})),
            plan_item(3, "song", "Song", serde_json::json!({})),
            plan_item(4, "sermon", "Message", serde_json::json!({}))
        ]),
        Some(77),
    );
    let unlabelled = setlist(serde_json::json!({"id": 2, "name": "Plain", "linked_event_id": 77, "songs": [
        setlist_song(1, 101, "First Test Song", None),
        setlist_song(2, 103, "Third Test Song", None),
        setlist_song(3, 104, "Fourth Test Song", None)
    ]}));
    let planned = from_plan(&plan, Some(&unlabelled));
    let titles: Vec<String> = planned
        .items
        .iter()
        .map(|item| match item {
            Planned::Song { title, .. } => format!("song {title}"),
            Planned::Placeholder { name } => format!("place {name}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        titles,
        [
            "song First Test Song",
            "place Announcements",
            "song Third Test Song",
            "song Fourth Test Song",
            "place Message"
        ]
    );
    // Without its setlist, the plan's song places are kept for songs to come.
    let alone = to_service(&from_plan(&plan, None), library);
    assert_eq!(alone.items[0], Item::Placeholder { name: "Song".to_owned() });
}

#[test]
fn a_setlist_and_a_plan_on_the_same_day_for_other_events_are_not_joined() {
    let plan = plan(serde_json::json!([]), Some(78));
    assert!(linked_setlist(&plan, &[sunday_setlist()]).is_none());
    let no_event = ServicePlan { event_id: None, ..plan };
    assert!(linked_setlist(&no_event, &[sunday_setlist()]).is_none());
}

#[test]
fn services_with_the_same_name_are_told_apart_by_their_dates() {
    let service = |name: &str, date: &str| PlannedService {
        name: name.to_owned(),
        date: Some(date.to_owned()),
        items: Vec::new(),
    };
    let mut services = [
        service("Sunday Service", "2026-12-06"),
        service("sunday service", "2026-12-13"),
        service("Concert", "2026-12-05"),
    ];
    distinct_names(&mut services);
    let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Sunday Service (2026-12-06)", "sunday service (2026-12-13)", "Concert"]);
}

#[test]
fn a_services_fingerprint_changes_with_what_it_asks_for() {
    let one = from_setlist(&sunday_setlist());
    assert_eq!(one.fingerprint(), from_setlist(&sunday_setlist()).fingerprint());
    let mut moved = sunday_setlist();
    moved.songs[0].order = 0;
    assert_ne!(one.fingerprint(), from_setlist(&moved).fingerprint());
    assert_eq!(one.fingerprint().len(), 16);
}

#[test]
fn keys_name_the_church_and_titles_lose_their_escapes() {
    assert_eq!(service_key(CHURCH, Made::Setlist(5)), "test-church.example.org/setlist/5");
    assert_eq!(service_key("test-church.example.org", Made::Plan(6)), "test-church.example.org/plan/6");
    assert_eq!(parse_song_key("x/song/1/arrangement/two"), None);
    assert_eq!(plain(" Faith &amp; Hope &#39;test&#x27; &lt;b&gt; "), "Faith & Hope 'test' <b>");
    assert_eq!(plain("Fish & Chips &unknown; &"), "Fish & Chips &unknown; &");
}

#[test]
fn a_song_saved_and_opened_again_is_still_untouched() {
    // Everything a made song can hold: words before any name, an arrangement, a translation,
    // notes, the details and the mark.
    let chart = chart(
        12,
        serde_json::json!([
            {"name": null, "lines": ["Opening words", ""]},
            {"name": "Куплет 1", "lines": ["Первая строка", "", "Третья строка"]},
            {"name": "Припев", "lines": ["Строка припева"]},
            {"name": "Припев - 2x", "lines": []}
        ]),
    );
    let mut song = song_json(12, "Saved &amp; Opened");
    song.notes = Some("A note".to_owned());
    song.copyright_info = Some("2020 Test Music".to_owned());
    let rows = [("Первая строка", "First line"), ("Строка припева", "Chorus line")];
    let made = make_song(CHURCH, &song, &chart, Some(&translation(&rows)), 4, 0).song;
    assert!(made.active_arrangement().is_some() && translations(&made).iter().any(Option::is_some));
    let opened = sp_model::from_json(&sp_model::to_json(&made)).expect("opens");
    let mark = opened.catalog.as_ref().expect("still marked");
    assert_eq!(catalog::fingerprint(&opened), mark.made_hash, "a song nobody touched must still look untouched");
    assert_eq!(
        catalog::judge(&opened, &fresh_source(&make_song(CHURCH, &song, &chart, Some(&translation(&rows)), 4, 9))),
        Verdict::Current
    );
}

#[test]
fn null_fields_read_as_empty() {
    let song: Song = serde_json::from_value(serde_json::json!({
        "id": 13, "title": null, "artist": null, "ccli_number": null, "arrangements": null
    }))
    .expect("a song with nulls reads");
    assert_eq!((song.title.as_str(), song.arrangements.len(), song.ccli().as_str()), ("", 0, ""));
    let row: TranslationRow =
        serde_json::from_value(serde_json::json!({"source": "A line", "translated": null})).expect("reads");
    assert_eq!(row.translated, "");
    let setlist: Setlist = serde_json::from_value(serde_json::json!({
        "id": 14, "name": null, "event_date": "2026-12-06", "is_active": null,
        "songs": [{"order": null, "song_id": 15, "title": null, "worship_spot": null}]
    }))
    .expect("a setlist with nulls reads");
    assert_eq!(from_setlist(&setlist).name, "2026-12-06", "a service with no name is named by its date");
    let plan: ServicePlan =
        serde_json::from_value(serde_json::json!({"id": 16, "title": null, "items": [{"type": null, "title": null}]}))
            .expect("a plan with nulls reads");
    assert_eq!(from_plan(&plan, None).items, [Planned::Placeholder { name: "Item".to_owned() }]);
    let chart: Chart = serde_json::from_value(serde_json::json!({"song_id": 17, "arrangement_id": null,
        "sections": [{"name": "Verse", "lines": null}]}))
    .expect("a chart with nulls reads");
    assert_eq!(chart.sections[0].lines.len(), 0);
}

#[test]
fn a_bare_name_finds_its_numbered_group_and_one_naming_nothing_is_listed() {
    let chart = chart(
        18,
        serde_json::json!([
            {"name": "Intro", "lines": ["[G] [D] [C]"]},
            {"name": "Chorus", "lines": []},
            {"name": "Verse 1", "lines": ["A verse line"]},
            {"name": "Chorus 1", "lines": ["The chorus line"]},
            {"name": "Verse 2", "lines": ["Another verse line"]},
            {"name": "Chorus", "lines": []}
        ]),
    );
    let made = make(&chart);
    assert_eq!(sung(&made.song), ["Verse 1", "Chorus 1", "Verse 2", "Chorus 1"]);
    assert_eq!(made.unmatched, ["Intro", "Chorus"], "the intro has no words; the first chorus came before any");
}
