//! Every song and service here is made up for these tests, in the shapes TimelyChurch described
//! for its Worship API; none of the words are a real song's.

use std::collections::HashMap;

use sp_model::{Presentation, Role};

use super::plan::{Planned, linked_setlist};
use super::song::{MadeSong, translation_for};
use super::wire::{PairReply, Refusal, ServicesReply, Song, Translation};
use super::*;
use crate::catalog::{self, Verdict};
use crate::service::Item as PlaylistItem;

const CHURCH: &str = "https://Test-Church.example.org/";

fn song(value: serde_json::Value) -> Song {
    serde_json::from_value(value).expect("a song reads")
}

/// A two-language song: four sections sent once each, the chorus sung three times, and a
/// Spanish translation with one line nobody translated.
fn two_languages() -> Song {
    song(serde_json::json!({
        "id": 101,
        "title": "Morning Test Song",
        "authors": ["A. Writer", "B. Writer"],
        "ccli_number": "CCLI 0001234",
        "copyright": "© 2001 Test Music",
        "key": "G",
        "arrangement": {"id": 1011, "name": "Default", "key": "G", "is_default": true},
        "sections": [
            {"name": "Verse 1", "lines": ["[G]First line of the [D]verse", "Second line of the verse", "",
                                          "Third line after a break", "Fourth line after a break"]},
            {"name": "Chorus", "lines": ["Sing the test chorus", "Sing it once again"]},
            {"name": "Verse 2", "lines": ["Another verse begins", "And it ends here"]},
            {"name": "Bridge", "lines": ["A bridge line"]}
        ],
        "order": ["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"],
        "translations": [{
            "language": "ES",
            "title": "Canción de prueba",
            "sections": [
                {"name": "Verse 1", "lines": ["Primera línea", "Segunda línea", null, "Tercera línea", null]},
                {"name": "Chorus", "lines": ["Canta el coro", "Cántalo otra vez"]},
                {"name": "Verse 2", "lines": [null, null]},
                {"name": "Bridge", "lines": ["Un puente"]}
            ]
        }],
        "updated_at": "2026-11-20T10:00:00Z"
    }))
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

fn make(song: &Song) -> MadeSong {
    make_song(CHURCH, song, sp_model::MOST_LINES, 0)
}

#[test]
fn sections_become_groups_and_the_order_sung_becomes_the_arrangement() {
    let made = make(&two_languages()).song;
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
            ("Bridge".to_owned(), vec!["A bridge line".to_owned()]),
        ],
        "each section once; chords out; a blank line starts a slide"
    );
    assert_eq!(
        sung(&made),
        ["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"],
        "a repeat is the same group again"
    );
    let arrangement = made.active_arrangement().expect("an arrangement, chosen to play");
    assert_eq!(arrangement.name, "TimelyChurch", "TimelyChurch's \"Default\" names no arrangement in particular");
    assert_eq!(arrangement.groups[1], arrangement.groups[3]);
}

#[test]
fn an_aligned_translation_goes_under_the_words_line_for_line() {
    let made = make(&two_languages());
    assert_eq!(
        translations(&made.song),
        [
            Some("Primera línea\nSegunda línea".to_owned()),
            Some("Tercera línea".to_owned()),
            Some("Canta el coro\nCántalo otra vez".to_owned()),
            None,
            Some("Un puente".to_owned()),
        ],
        "each slide's own lines in order; an untranslated line is left out; a slide with none has no box"
    );
    assert_eq!(made.second.as_deref(), Some("es"));
    assert_eq!(made.untranslated, 3, "the fourth line of verse 1 and both lines of verse 2");
    assert_eq!(made.song.catalog.as_ref().and_then(|m| m.second.clone()).as_deref(), Some("es"));
}

#[test]
fn a_translation_lines_up_by_place_and_by_name_whatever_its_length() {
    let mut song = two_languages();
    // Sections sent in another order than the song's, one short of lines, one too long.
    song.translations[0].sections = serde_json::from_value(serde_json::json!([
        {"name": "Chorus", "lines": ["Canta el coro"]},
        {"name": "Verse 1", "lines": ["Uno", "Dos", null, "Tres", "Cuatro", "sobra"]},
        {"name": null, "lines": ["Otra estrofa"]}
    ]))
    .expect("reads");
    let made = make(&song);
    assert_eq!(
        translations(&made.song),
        [
            Some("Uno\nDos".to_owned()),
            Some("Tres\nCuatro".to_owned()),
            Some("Canta el coro".to_owned()),
            Some("Otra estrofa".to_owned()),
            None,
        ],
        "a section named elsewhere is found by its name; one with no name by its place"
    );
}

#[test]
fn a_translation_with_nothing_translated_puts_nothing_under_the_words() {
    let mut song = two_languages();
    song.translations = serde_json::from_value(serde_json::json!([
        {"language": "ru", "sections": [{"name": "Verse 1", "lines": [null, "  ", null]}]}
    ]))
    .expect("reads");
    let made = make(&song);
    assert!(translations(&made.song).iter().all(Option::is_none));
    assert_eq!((made.second, made.untranslated), (None, 0));
}

#[test]
fn the_first_translation_with_words_is_the_one_under_the_words() {
    let mut song = two_languages();
    let spanish = song.translations[0].clone();
    let empty: Translation =
        serde_json::from_value(serde_json::json!({"language": "uk", "sections": []})).expect("reads");
    let russian = Translation { language: "ru".to_owned(), ..spanish.clone() };
    song.translations = vec![empty, russian, spanish];
    assert_eq!(translation_for(&song).map(|t| t.language.as_str()), Some("ru"));
    assert_eq!(make(&song).second.as_deref(), Some("ru"));
}

#[test]
fn an_order_naming_nothing_is_listed_and_one_as_written_is_no_arrangement() {
    let mut song = two_languages();
    song.order = ["Intro", "Verse 1", "chorus", "Verse 2", "Instrumental", "CHORUS"].map(str::to_owned).to_vec();
    let made = make(&song);
    assert_eq!(sung(&made.song), ["Verse 1", "Chorus", "Verse 2", "Chorus"]);
    assert_eq!(made.unmatched, ["Intro", "Instrumental"], "parts with no words are no group");
    song.order = ["Verse 1", "Chorus", "Verse 2", "Bridge"].map(str::to_owned).to_vec();
    let as_written = make(&song).song;
    assert!(as_written.arrangements.is_empty() && as_written.arrangement.is_none());
    song.order.clear();
    assert!(make(&song).song.arrangements.is_empty(), "no order: as written");
}

#[test]
fn two_sections_with_one_name_are_sung_in_turn() {
    let made = make(&song(serde_json::json!({
        "id": 5, "title": "T",
        "sections": [
            {"name": "Chorus", "lines": ["The chorus line"]},
            {"name": "Verse", "lines": ["A verse line"]},
            {"name": "Chorus", "lines": ["The chorus line", "With a last line added"]}
        ],
        "order": ["Chorus", "Verse", "Chorus", "Chorus"]
    })))
    .song;
    let ids: Vec<sp_model::Uuid> = made.active_arrangement().map(|a| a.groups.clone()).unwrap_or_default();
    assert_eq!(ids, [made.groups[0].id, made.groups[1].id, made.groups[2].id, made.groups[0].id]);
}

#[test]
fn long_stretches_are_shared_out() {
    let lines: Vec<String> = (1..=8).map(|n| format!("Line number {n}")).collect();
    let long = song(serde_json::json!({"id": 7, "title": "T", "sections": [{"name": "Verse 1", "lines": lines}]}));
    let sizes = |most: usize| {
        let made = make_song(CHURCH, &long, most, 0).song;
        made.groups[0]
            .slides
            .iter()
            .map(|s| s.text_for(&Role::primary()).unwrap_or("").lines().count())
            .collect::<Vec<_>>()
    };
    assert_eq!(sizes(sp_model::MOST_LINES), [4, 4]);
    assert_eq!(sizes(3), [3, 3, 2]);
}

#[test]
fn the_songs_details_title_and_mark() {
    let made = make_song(CHURCH, &two_languages(), 4, 86_400 * 365).song;
    assert_eq!(made.title, "Morning Test Song");
    assert_eq!(made.copyright.author, "A. Writer, B. Writer");
    assert_eq!((made.copyright.year.as_str(), made.copyright.publisher.as_str()), ("2001", "Test Music"));
    assert_eq!(made.copyright.ccli_song, "0001234");
    let mark = made.catalog.as_ref().expect("marked");
    assert_eq!(mark.source, "timelychurch");
    assert_eq!(mark.key, "test-church.example.org/song/101/arrangement/1011");
    assert_eq!(
        parse_song_key(&mark.key),
        Some(("test-church.example.org".to_owned(), Id::new("101"), Some(Id::new("1011"))))
    );
    assert_eq!(mark.added, "1971-01-01");
    assert_eq!(mark.made_hash, catalog::fingerprint(&made));
    assert_eq!(mark.text_hash, mark.made_hash);
    // An arrangement that isn't the song's default is a song of its own, named for it.
    let mut youth = two_languages();
    youth.arrangement =
        serde_json::from_value(serde_json::json!({"id": "a-7", "name": "Youth", "is_default": false})).expect("reads");
    let made = make(&youth).song;
    assert_eq!(made.title, "Morning Test Song (Youth)");
    assert_eq!(made.active_arrangement().map(|a| a.name.as_str()), Some("Youth"));
    assert!(made.catalog.as_ref().is_some_and(|m| m.key.ends_with("/song/101/arrangement/a-7")));
    // No arrangement named at all: the song's own key.
    let mut plain_song = two_languages();
    plain_song.arrangement = None;
    assert_eq!(make(&plain_song).song.catalog.map(|m| m.key).as_deref(), Some("test-church.example.org/song/101"));
}

#[test]
fn an_untouched_song_is_brought_up_to_date_and_a_changed_one_is_only_offered() {
    let words = |line: &str| {
        song(serde_json::json!({"id": 10, "title": "T", "sections": [{"name": "Verse 1", "lines": [line]}]}))
    };
    let old = make(&words("The words as they were")).song;
    // Made again from the same data: the same fingerprint, whatever the new ids.
    assert_eq!(catalog::judge(&old, &fresh_source(&make(&words("The words as they were")))), Verdict::Current);
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
fn a_song_saved_and_opened_again_is_still_untouched() {
    let made = make(&two_languages()).song;
    assert!(made.active_arrangement().is_some() && translations(&made).iter().any(Option::is_some));
    let opened = sp_model::from_json(&sp_model::to_json(&made)).expect("opens");
    let mark = opened.catalog.as_ref().expect("still marked");
    assert_eq!(catalog::fingerprint(&opened), mark.made_hash, "a song nobody touched must still look untouched");
    assert_eq!(catalog::judge(&opened, &fresh_source(&make_song(CHURCH, &two_languages(), 4, 9))), Verdict::Current);
}

fn services(value: serde_json::Value) -> Vec<Service> {
    ServicesReply::read(&value.to_string()).expect("a list reads").services
}

fn song_item(position: u32, song: u64, title: &str, spot: Option<&str>) -> serde_json::Value {
    serde_json::json!({"position": position, "type": "song", "title": title, "song_id": song,
        "arrangement_id": null, "key": "G", "notes": null, "worship_spot": spot})
}

fn item(position: u32, kind: &str, title: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut item = serde_json::json!({"position": position, "type": kind, "title": title});
    if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
        item.extend(extra.clone());
    }
    item
}

fn sunday_setlist() -> serde_json::Value {
    serde_json::json!({
        "id": 501, "type": "setlist", "name": "Test Sunday &amp; Communion", "date": "2026-12-06",
        "event_id": 77, "updated_at": "2026-11-20T10:00:00Z",
        "items": [
            song_item(3, 103, "Third Test Song", Some("Communion")),
            song_item(1, 101, "First Test Song", Some("Opening")),
            song_item(2, 102, "Second Test Song", Some("Opening")),
            song_item(4, 101, "First Test Song", Some("Communion"))
        ]
    })
}

/// The library: songs 101 and 103 are in it, 102 isn't yet.
fn library(song: &Id, arrangement: Option<&Id>) -> Option<String> {
    assert_eq!(arrangement, None);
    HashMap::from([("101", "id-101"), ("103", "id-103")]).get(song.as_str()).map(|key| (*key).to_owned())
}

#[test]
fn a_setlist_becomes_a_playlist_in_its_order() {
    let list = services(serde_json::json!({"services": [sunday_setlist()]}));
    let follow = to_follow(&list);
    assert_eq!(follow.len(), 1);
    let planned = from_service(follow[0].0, follow[0].1);
    assert_eq!(planned.name, "Test Sunday & Communion");
    assert_eq!(planned.date.as_deref(), Some("2026-12-06"));
    let songs = planned.items.iter().filter(|item| matches!(item, Planned::Song { .. })).count();
    assert_eq!(songs, 4, "as the setlist sings them; the playlist takes each once");
    let service = to_service(&planned, library);
    assert_eq!(
        service.items,
        [
            PlaylistItem::Header { name: "Opening".to_owned() },
            PlaylistItem::song("id-101", "First Test Song"),
            PlaylistItem::Placeholder { name: "Second Test Song".to_owned() },
            PlaylistItem::Header { name: "Communion".to_owned() },
            PlaylistItem::song("id-103", "Third Test Song"),
        ],
        "in order; a song not in the library yet keeps its place; a song comes once"
    );
}

fn plan(items: serde_json::Value, event: Option<u64>) -> serde_json::Value {
    serde_json::json!({"id": 900, "type": "plan", "title": "Test Service", "date": "2026-12-06", "time": "10:00",
        "event_id": event, "updated_at": "2026-11-21T09:00:00Z", "items": items})
}

#[test]
fn a_plan_takes_its_setlists_songs_into_its_places_for_worship() {
    let plan = plan(
        serde_json::json!([
            item(1, "heading", "Welcome", serde_json::json!({})),
            item(2, "worship", "OPENING", serde_json::json!({"worship_spot": "Opening"})),
            item(3, "reading", "Reading", serde_json::json!({"reference": "Psalm 100"})),
            item(4, "sermon", "Message", serde_json::json!({})),
            item(5, "worship", "Communion", serde_json::json!({"worship_spot": "communion"})),
            item(6, "prayer", "", serde_json::json!({}))
        ]),
        Some(77),
    );
    let other_day = serde_json::json!({"id": 1, "type": "setlist", "name": "Other day", "event_id": 76});
    let list = services(serde_json::json!([other_day, sunday_setlist(), plan]));
    let follow = to_follow(&list);
    let names: Vec<&str> = follow.iter().map(|(s, _)| s.name.as_str()).collect();
    assert_eq!(names, ["Other day", "Test Service"], "the setlist joined to the plan is no playlist of its own");
    let (plan, linked) = follow[1];
    assert_eq!(linked.map(|s| s.id.as_str()), Some("501"), "the setlist for the same event");
    crate::i18n::with_language(crate::i18n::Language::English, || {
        let service = to_service(&from_service(plan, linked), library);
        assert_eq!(
            service.items,
            [
                PlaylistItem::Header { name: "Welcome".to_owned() },
                PlaylistItem::Header { name: "Opening".to_owned() },
                PlaylistItem::song("id-101", "First Test Song"),
                PlaylistItem::Placeholder { name: "Second Test Song".to_owned() },
                PlaylistItem::Placeholder { name: "Reading: Psalm 100".to_owned() },
                PlaylistItem::Placeholder { name: "Message".to_owned() },
                PlaylistItem::Header { name: "communion".to_owned() },
                PlaylistItem::song("id-103", "Third Test Song"),
                PlaylistItem::Placeholder { name: "Prayer".to_owned() },
            ]
        );
    });
}

#[test]
fn without_a_place_for_worship_the_songs_fill_the_plans_song_places_and_the_rest_follow() {
    let plan = plan(
        serde_json::json!([
            item(1, "song", "Song", serde_json::json!({})),
            item(2, "announcement", "Announcements", serde_json::json!({})),
            item(3, "song", "Song", serde_json::json!({})),
            item(4, "sermon", "Message", serde_json::json!({}))
        ]),
        Some(77),
    );
    let unlabelled = serde_json::json!({"id": 2, "type": "setlist", "name": "Plain", "event_id": 77, "items": [
        song_item(1, 101, "First Test Song", None),
        song_item(2, 103, "Third Test Song", None),
        song_item(3, 104, "Fourth Test Song", None)
    ]});
    let list = services(serde_json::json!([plan, unlabelled]));
    let planned = from_service(&list[0], linked_setlist(&list[0], &list));
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
    let alone = to_service(&from_service(&list[0], None), library);
    assert_eq!(alone.items[0], PlaylistItem::Placeholder { name: "Song".to_owned() });
}

#[test]
fn a_setlist_and_a_plan_without_a_shared_event_are_not_joined() {
    let list = services(serde_json::json!([plan(serde_json::json!([]), Some(78)), sunday_setlist()]));
    assert!(linked_setlist(&list[0], &list).is_none());
    assert_eq!(to_follow(&list).len(), 2);
    let no_event = services(serde_json::json!([plan(serde_json::json!([]), None), sunday_setlist()]));
    assert!(linked_setlist(&no_event[0], &no_event).is_none());
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
fn keys_name_the_church_and_titles_lose_their_escapes() {
    assert_eq!(service_key(CHURCH, &Made::Setlist(Id::new("5"))), "test-church.example.org/setlist/5");
    assert_eq!(service_key("test-church.example.org", &Made::Plan(Id::new("6"))), "test-church.example.org/plan/6");
    assert_eq!(parse_song_key("x/song/1/arrangement/two words"), None);
    assert_eq!(parse_song_key("x/song/1"), Some(("x".to_owned(), Id::new("1"), None)));
    assert_eq!(plain(" Faith &amp; Hope &#39;test&#x27; &lt;b&gt; "), "Faith & Hope 'test' <b>");
    assert_eq!(plain("Fish & Chips &unknown; &"), "Fish & Chips &unknown; &");
}

#[test]
fn null_fields_and_other_spellings_read() {
    let song: Song = serde_json::from_value(serde_json::json!({
        "id": "abc-13", "title": null, "authors": "One Writer; Two Writer", "ccli_number": null,
        "sections": null, "order": [{"name": "Chorus"}, null, "Verse"], "translations": null,
        "last_changed": "2026-11-01T00:00:00Z"
    }))
    .expect("a song with nulls reads");
    assert_eq!(song.id, Id::new("abc-13"));
    assert_eq!((song.title.as_str(), song.sections.len(), song.ccli().as_str()), ("", 0, ""));
    assert_eq!(song.authors, ["One Writer", "Two Writer"]);
    assert_eq!(song.order, ["Chorus", "Verse"]);
    assert_eq!(song.updated_at.as_deref(), Some("2026-11-01T00:00:00Z"));
    let list = services(serde_json::json!({"data": [
        {"id": 14, "kind": "setlist", "title": null, "date": "2026-12-06", "items": [
            {"order": null, "type": null, "song_id": 15, "title": null}
        ]},
        "not a service"
    ]}));
    assert_eq!(list.len(), 1, "what doesn't read is left out, not the whole list");
    let read = ServicesReply::read(r#"{"services": [{"id": 1}, 7, "x"]}"#).expect("a list");
    assert_eq!((read.services.len(), read.dropped), (1, 2), "and counted, so nothing is taken for removed");
    assert_eq!(from_service(&list[0], None).name, "2026-12-06", "a service with no name is named by its date");
    let planned = from_service(&list[0], None);
    assert!(matches!(&planned.items[0], Planned::Song { song, .. } if song.as_str() == "15"));
    assert!(ServicesReply::read("not json").is_none());
    let pair: PairReply =
        serde_json::from_value(serde_json::json!({"device_token": "t", "church_name": "Grace &amp; Peace"}))
            .expect("reads");
    assert_eq!((pair.token.as_str(), pair.church_name().as_str()), ("t", "Grace & Peace"));
    assert!(!format!("{pair:?}").contains("\"t\""), "the token never shows");
    assert_eq!(Refusal::words(r#"{"error": {"code": "x", "message": "Turned off"}}"#).as_deref(), Some("Turned off"));
}

#[test]
fn only_plain_ids_go_into_an_address() {
    assert!(Id::new("1201").is_safe() && Id::new("a-b_c.9").is_safe());
    for bad in ["", "../x", "a/b", "a?b=c", "a b", "é"] {
        assert!(!Id::new(bad).is_safe(), "{bad}");
    }
    let list = services(serde_json::json!([{"id": 1, "type": "setlist", "items": [song_item(1, 0, "x", None)]}]));
    let mut evil = list[0].clone();
    evil.items[0].song_id = Some(Id::new("../../admin"));
    assert!(
        matches!(&from_service(&evil, None).items[0], Planned::Placeholder { .. }),
        "a song whose id can't go into an address is only a placeholder"
    );
}

/// The client, the checks and the song uses, against the demo church (`mock`).
mod with_the_demo_church {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use super::super::client::{self, Problem, Sent, Services};
    use super::super::mock::{CODE, MockChurch};
    use super::super::store::{Files, ServiceSeen, Settings, SongSeen, UseBook, idempotency_key};
    use super::super::sync::{Connection, Said, Status, songs_to_fetch};
    use super::super::wire::{Id, PairRequest, SongUse};
    use super::*;

    fn pairing(code: &str) -> PairRequest {
        PairRequest {
            pairing_code: code.to_owned(),
            device_name: "Test Mac".to_owned(),
            app_name: "SoftPresenter".to_owned(),
            app_version: "0.0.0".to_owned(),
        }
    }

    /// The dates a check asks for, from today.
    fn dates() -> (String, String) {
        let today = chrono::Local::now().date_naive();
        (today.format("%Y-%m-%d").to_string(), (today + chrono::Duration::days(28)).format("%Y-%m-%d").to_string())
    }

    /// A folder of this test's own, empty.
    fn folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("softpresenter-test-timelychurch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Ticks the connection until `done` says so, for at most ten seconds.
    fn until(connection: &mut Connection, done: impl Fn(&Connection) -> bool) {
        let started = Instant::now();
        while !done(connection) {
            assert!(started.elapsed() < Duration::from_secs(10), "waited too long: {:?}", connection.status);
            connection.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn an_address_is_https_unless_it_is_this_computer() {
        assert_eq!(client::address("Grace.TimelyChurch.com/").as_deref(), Ok("https://grace.timelychurch.com"));
        assert_eq!(
            client::address(" https://grace.example.org/setlists?x=1 ").as_deref(),
            Ok("https://grace.example.org")
        );
        assert_eq!(client::address("http://127.0.0.1:4320").as_deref(), Ok("http://127.0.0.1:4320"));
        assert_eq!(client::address("http://localhost:8080/").as_deref(), Ok("http://localhost:8080"));
        for refused in [
            "http://grace.example.org",
            "ftp://grace.example.org",
            "",
            "grace",
            "user@grace.example.org",
            "https://grace example.org",
        ] {
            assert!(client::address(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn pairing_takes_only_the_right_code_and_the_token_goes_only_in_its_header() {
        let church = MockChurch::start();
        let wrong = crate::i18n::with_language(crate::i18n::Language::English, || {
            client::pair(&church.address, &pairing("WRONG")).expect_err("refused")
        });
        assert!(matches!(&wrong, Problem::Other(why) if why.contains("pairing code")), "{wrong:?}");
        let paired = client::pair(&church.address, &pairing(CODE)).expect("paired");
        assert_eq!(paired.church, "Demo Church");
        assert_eq!(paired.church_id, Some(Id::new("7")));
        assert!(!format!("{paired:?}").contains(&paired.token), "the token never shows");
        let (from, to) = dates();
        client::services(&church.address, &paired.token, &from, &to, None).expect("listed");
        let asked = format!("GET /api/v1/worship/services?from={from}&to={to}");
        church.with(|demo| {
            assert!(demo.requests.iter().all(|line| !line.contains(&paired.token)), "{:?}", demo.requests);
            assert!(demo.requests.iter().any(|line| line.starts_with(&asked)), "{:?}", demo.requests);
        });
    }

    #[test]
    fn the_services_list_answers_nothing_new_to_its_etag() {
        let church = MockChurch::start();
        let token = client::pair(&church.address, &pairing(CODE)).expect("paired").token;
        let (from, to) = dates();
        let first = client::services(&church.address, &token, &from, &to, None).expect("listed");
        let Services::Fresh { list, etag, .. } = first else { panic!("a list") };
        assert_eq!(list.services.len(), 2);
        assert_eq!(etag.as_deref(), Some("\"demo-v0\""));
        let again = client::services(&church.address, &token, &from, &to, etag.as_deref()).expect("asked");
        assert_eq!(again, Services::Unchanged);
        church.with(|demo| demo.version += 1);
        let changed = client::services(&church.address, &token, &from, &to, etag.as_deref()).expect("asked");
        assert!(matches!(changed, Services::Fresh { etag: Some(ref e), .. } if e == "\"demo-v1\""), "{changed:?}");
    }

    #[test]
    fn a_song_reads_with_its_translation_and_a_deleted_one_is_none() {
        let church = MockChurch::start();
        let token = client::pair(&church.address, &pairing(CODE)).expect("paired").token;
        let (song, body) =
            client::song(&church.address, &token, &Id::new("101"), None).expect("asked").expect("a song");
        assert!(body.contains("Morning light"));
        let made = make_song(&church.address, &song, sp_model::MOST_LINES, 0);
        assert_eq!(made.second.as_deref(), Some("es"));
        assert_eq!(made.song.active_arrangement().map(|a| a.groups.len()), Some(6), "the chorus three times");
        assert_eq!(client::song(&church.address, &token, &Id::new("103"), None), Ok(None), "deleted: 404");
        assert_eq!(client::song(&church.address, &token, &Id::new("../x"), None), Ok(None), "never asked for");
    }

    #[test]
    fn a_refused_token_a_wait_and_no_network_are_told_apart() {
        let church = MockChurch::start();
        let token = client::pair(&church.address, &pairing(CODE)).expect("paired").token;
        let (from, to) = dates();
        church.with(|demo| demo.wait_next = 1);
        let waited = client::services(&church.address, &token, &from, &to, None);
        assert_eq!(waited, Err(Problem::Wait(Duration::from_secs(1))));
        church.with(|demo| demo.tokens.clear());
        assert_eq!(client::services(&church.address, &token, &from, &to, None), Err(Problem::Disconnected));
        assert_eq!(client::song(&church.address, &token, &Id::new("101"), None), Err(Problem::Disconnected));
        // Nothing listening there: the network is down, as far as the app can tell.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = format!("http://{}", closed.local_addr().expect("address"));
        drop(closed);
        assert!(matches!(client::services(&address, &token, &from, &to, None), Err(Problem::Unreachable(_))));
    }

    #[test]
    fn a_song_use_counts_once_however_often_it_is_sent() {
        let church = MockChurch::start();
        let token = client::pair(&church.address, &pairing(CODE)).expect("paired").token;
        let used = SongUse {
            song_id: Id::new("101"),
            arrangement_id: None,
            used_at: "2026-12-06T10:15:00+00:00".to_owned(),
            service_id: Some(Id::new("501")),
            service_type: Some("setlist".to_owned()),
            key: Some("G".to_owned()),
        };
        let key = idempotency_key("church/setlist/501|church/song/101/arrangement/1011");
        assert_eq!(client::song_use(&church.address, &token, &used, &key), Ok(Sent::Recorded));
        assert_eq!(client::song_use(&church.address, &token, &used, &key), Ok(Sent::Recorded), "a retry");
        church.with(|demo| {
            assert_eq!((demo.uses.len(), demo.use_posts), (1, 2), "sent twice, counted once");
            assert_eq!(demo.uses[0].1["song_id"], serde_json::json!("101"));
            assert_eq!(demo.uses[0].1["service_id"], serde_json::json!("501"));
        });
    }

    #[test]
    fn the_use_book_counts_a_song_once_per_service_with_a_lasting_key() {
        let mut book = UseBook::default();
        assert!(book.count("svc/1|song/9", SongUse::default()));
        assert!(!book.count("svc/1|song/9", SongUse::default()), "once per service");
        assert!(book.count("svc/2|song/9", SongUse::default()), "another service counts again");
        assert_eq!(book.queued.len(), 2);
        let key = book.queued[0].key.clone();
        assert_eq!(key, idempotency_key("svc/1|song/9"), "the same use always has the same key");
        assert_eq!(key.len(), 36);
        let kept: UseBook = serde_json::from_str(&serde_json::to_string(&book).expect("writes")).expect("reads");
        assert_eq!(kept, book, "kept on disk as it is");
        book.sent(&[key]);
        assert_eq!(book.queued.len(), 1);
        assert!(!book.count("svc/1|song/9", SongUse::default()), "sent, and still counted");
    }

    #[test]
    fn only_songs_that_may_have_changed_are_fetched() {
        let list = ServicesReply::read(&serde_json::json!([super::sunday_setlist()]).to_string()).expect("reads");
        let church = "test.example.org";
        let settings = |seen: Vec<SongSeen>, updated: Option<&str>| Settings {
            address: church.to_owned(),
            songs: seen,
            services: vec![ServiceSeen {
                key: format!("{church}/setlist/501"),
                updated_at: updated.map(str::to_owned),
                date: None,
            }],
            ..Settings::default()
        };
        let seen = |song: &str, fetched_at: i64, gone: bool| SongSeen {
            song: Id::new(song),
            arrangement: None,
            key: format!("{church}/song/{song}"),
            updated_at: None,
            fetched_at,
            gone,
        };
        let now = 100_000;
        let ids = |wanted: Vec<(Id, Option<Id>)>| wanted.into_iter().map(|(song, _)| song.0).collect::<Vec<_>>();
        let library: HashSet<String> = ["101", "102", "103"].iter().map(|s| format!("{church}/song/{s}")).collect();
        // Never fetched: each, once, in the setlist's order.
        assert_eq!(ids(songs_to_fetch(&list, &settings(Vec::new(), None), &library, now)), ["101", "102", "103"]);
        // Fetched an hour ago, in the library, the setlist as it was: none.
        let fresh = vec![seen("101", now - 3600, false), seen("102", now - 3600, false), seen("103", now - 3600, true)];
        let unchanged = settings(fresh.clone(), Some("2026-11-20T10:00:00Z"));
        assert!(songs_to_fetch(&list, &unchanged, &library, now).is_empty());
        // The setlist changed: its songs again (not the one TimelyChurch deleted).
        let changed = settings(fresh, Some("2026-11-01T00:00:00Z"));
        assert_eq!(ids(songs_to_fetch(&list, &changed, &library, now)), ["101", "102"]);
        // A day later: every one again, the deleted one too (it may be back).
        assert_eq!(ids(songs_to_fetch(&list, &unchanged, &library, now + 86_400)), ["101", "102", "103"]);
        // A song gone from the library is fetched again.
        let without: HashSet<String> = library.iter().filter(|key| !key.ends_with("/102")).cloned().collect();
        assert_eq!(ids(songs_to_fetch(&list, &unchanged, &without, now)), ["102"]);
        // The list says when a song last changed.
        let mut told = list.clone();
        told.services[0].items[1].song_updated_at = Some("2026-11-23T00:00:00Z".to_owned());
        assert_eq!(ids(songs_to_fetch(&told, &unchanged, &library, now)), ["101"]);
    }

    #[test]
    fn a_connection_pairs_checks_and_sends_song_uses() {
        let church = MockChurch::start();
        let root = folder("connection");
        let files = Files::at(root.clone());
        let mut connection = Connection::new(Some(church.address.clone()), Some(files.clone()));
        assert_eq!(connection.status, Status::NotConnected);
        assert!(connection.pair("not an address", CODE).is_err());
        connection.pair(&church.address, "WRONG").expect("started");
        until(&mut connection, |c| c.status != Status::Pairing);
        assert_eq!(connection.status, Status::NotConnected);
        assert!(connection.trouble.is_some(), "why, in a sentence (in the language set)");
        connection.pair(&church.address, CODE).expect("started");
        until(&mut connection, |c| c.status != Status::Pairing);
        assert_eq!(connection.status, Status::Connected);
        assert_eq!(connection.settings.church, "Demo Church");
        assert!(connection.said.contains(&Said::Connected("Demo Church".to_owned())));
        assert_eq!(files.settings().church, "Demo Church");
        let kept = std::fs::read_to_string(root.join("settings.json")).unwrap_or_default();
        assert!(kept.contains("Demo Church") && !kept.contains("demo-token"), "the token is never in a file");
        let today = chrono::Local::now().date_naive();
        // A check: the list and every song in it, the deleted one noted.
        connection.check_now().expect("asked");
        assert!(connection.check_due());
        connection.start_check(HashSet::new(), today, chrono::Utc::now().timestamp());
        until(&mut connection, |c| c.waiting.is_some());
        let fetched = connection.waiting.clone().expect("found");
        assert!(fetched.fresh);
        let songs: Vec<&str> = fetched.songs.iter().map(|((song, _), _)| song.as_str()).collect();
        assert_eq!(songs, ["101", "102"]);
        assert_eq!(fetched.gone, [(Id::new("103"), None)]);
        assert!(files.song(&Id::new("101"), None).is_some(), "each song kept as sent");
        assert!(!connection.check_due(), "no check while what was found waits");
        connection.applied(&fetched);
        assert_eq!(files.cached().and_then(|c| c.etag).as_deref(), Some("\"demo-v0\""), "kept once applied");
        // The next check: nothing changed, a short answer, and the list kept stands for it.
        connection.check_now().expect("asked");
        connection.start_check(HashSet::new(), today, chrono::Utc::now().timestamp());
        until(&mut connection, |c| c.waiting.is_some());
        assert!(!connection.waiting.as_ref().is_some_and(|f| f.fresh));
        assert_eq!(connection.waiting.as_ref().map(|f| f.list.services.len()), Some(2));
        church.with(|demo| assert!(demo.requests.iter().any(|line| line.contains("if-none-match: \"demo-v0\""))));
        // A song shown: counted once, sent once.
        let used = SongUse { song_id: Id::new("101"), ..SongUse::default() };
        assert!(connection.count_use("s|101", used.clone()));
        assert!(!connection.count_use("s|101", used));
        until(&mut connection, |c| c.uses.queued.is_empty());
        church.with(|demo| assert_eq!((demo.uses.len(), demo.use_posts), (1, 1)));
        assert!(files.uses().queued.is_empty() && files.uses().counted == ["s|101"]);
        // Disconnected in TimelyChurch: said, and nothing more is asked.
        church.with(|demo| demo.tokens.clear());
        connection.waiting = None;
        connection.check_now().expect("asked");
        connection.start_check(HashSet::new(), today, chrono::Utc::now().timestamp());
        until(&mut connection, |c| c.status != Status::Connected);
        assert_eq!(connection.status, Status::Disconnected);
        assert!(connection.said.contains(&Said::Disconnected));
        assert!(connection.trouble.is_some());
        assert!(!files.settings().connected && files.settings().disconnected_there);
        assert!(files.cached().is_none(), "the list's ETag goes with the connection");
        assert!(connection.check_now().is_err());
        // Another church: the last one's songs shown are never told to it.
        let other = MockChurch::start();
        connection.pair(&other.address, CODE).expect("started");
        until(&mut connection, |c| c.status != Status::Pairing);
        assert_eq!(connection.status, Status::Connected);
        assert!(connection.uses.counted.is_empty() && files.uses().counted.is_empty());
        assert!(connection.settings.songs.is_empty(), "nothing seen of the last church counts");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_computer_disconnected_in_timelychurch_still_says_so_after_a_restart() {
        let root = folder("restart");
        let files = Files::at(root.clone());
        let left =
            Settings { address: "https://x.example.org".to_owned(), disconnected_there: true, ..Settings::default() };
        files.save_settings(&left).expect("saved");
        let mut connection = Connection::new(Some("https://x.example.org".to_owned()), Some(files));
        assert_eq!(connection.status, Status::Disconnected);
        assert!(connection.check_now().is_err(), "nothing is asked until it is connected again");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_is_offered_or_asked_without_the_switch() {
        let mut connection = Connection::new(None, None);
        assert_eq!(connection.status, Status::Off);
        assert!(!connection.offered());
        assert!(connection.pair("http://127.0.0.1:9", CODE).is_err());
        assert!(connection.check_now().is_err());
        assert!(!connection.count_use("s|1", SongUse::default()));
        assert_eq!(connection.tick(), None);
    }

    #[test]
    fn a_check_that_cant_reach_timelychurch_is_said_plainly_and_once_an_outage() {
        // Connected, with no token and no files: nothing here asks the network or the system's
        // store for secrets.
        let mut connection = Connection::new(Some("http://127.0.0.1:9".to_owned()), None);
        connection.status = Status::Connected;
        let mut notices = crate::notices::Notices::default();
        let texts = |notices: &crate::notices::Notices| -> Vec<String> {
            notices.to_vec().into_iter().map(|notice| notice.text).collect()
        };
        let out_of_reach = |reason: &str| Problem::Unreachable(format!("TimelyChurch couldn't be reached ({reason})."));
        connection.trouble_with(out_of_reach("io: No route to host (os error 65)"), true);
        let card = connection.trouble.clone().expect("the card says so at once");
        assert!(!card.contains("io:") && !card.contains("os error") && !card.contains('('), "plain words: {card}");
        assert!(connection.said.is_empty(), "nothing said for the notices as it comes: {:?}", connection.said);
        connection.tell_outage(&mut notices);
        assert!(texts(&notices).is_empty(), "one check that failed says nothing: {:?}", texts(&notices));
        // The next check, five minutes and a timeout on, can't reach it either: said once, in
        // the card's words (checks five minutes apart are no sleep).
        connection.trouble_with(out_of_reach("timeout: global"), true);
        connection.failed_at = connection.failed_at.map(|at| at + Duration::from_secs(320));
        assert!(connection.tell_outage(&mut notices));
        assert!(!connection.tell_outage(&mut notices));
        assert_eq!(texts(&notices), std::slice::from_ref(&card));
        // A pairing code given meanwhile: the same outage, the entry stays.
        connection.status = Status::Pairing;
        assert!(!connection.tell_outage(&mut notices));
        assert_eq!(texts(&notices), [card]);
        // Connected again, and a check that reaches it (it clears the trouble): the entry goes.
        connection.status = Status::Connected;
        connection.trouble = None;
        assert!(connection.tell_outage(&mut notices));
        assert!(texts(&notices).is_empty());
    }

    #[test]
    fn a_disconnect_while_timelychurch_is_out_of_reach_ends_the_outage() {
        // Connected, with no token and no files: nothing here asks the network or the system's
        // store for secrets.
        let mut connection = Connection::new(Some("http://127.0.0.1:9".to_owned()), None);
        connection.status = Status::Connected;
        let mut notices = crate::notices::Notices::default();
        let out_of_reach = || Problem::Unreachable("TimelyChurch couldn't be reached (timeout: global).".to_owned());
        // Two checks five minutes apart that can't reach it: told.
        let fail_twice = |connection: &mut Connection, notices: &mut crate::notices::Notices| {
            connection.trouble_with(out_of_reach(), true);
            assert!(!connection.tell_outage(notices));
            connection.trouble_with(out_of_reach(), true);
            connection.failed_at = connection.failed_at.map(|at| at + Duration::from_secs(320));
            connection.tell_outage(notices);
        };
        fail_twice(&mut connection, &mut notices);
        assert_eq!(notices.to_vec().len(), 1);
        // Disconnected here: nothing is asked any more, so the entry goes and the button is quiet.
        connection.disconnect();
        assert!(connection.tell_outage(&mut notices));
        assert!(notices.to_vec().is_empty(), "{:?}", notices.to_vec());
        assert_eq!(notices.look(), crate::notices::Look::Quiet);
        // Connected again into a new outage: told again, at its own second failed check.
        connection.status = Status::Connected;
        fail_twice(&mut connection, &mut notices);
        assert_eq!(notices.to_vec().len(), 1, "the new outage is told: {:?}", notices.to_vec());
    }

    #[test]
    fn connecting_when_timelychurch_cant_be_reached_says_so_plainly() {
        let words = super::super::sync::problem_words(&Problem::Unreachable(
            "TimelyChurch couldn't be reached (io: failed to lookup address information: nodename nor servname provided, or not known).".to_owned(),
        ));
        assert!(words.starts_with("TimelyChurch"), "{words}");
        assert!(!words.contains("io:") && !words.contains("lookup") && !words.contains('('), "{words}");
    }
}
