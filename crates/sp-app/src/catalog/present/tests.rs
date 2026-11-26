use super::*;
use crate::catalog::read::{Section, Song, Version};

fn section(title: &str, capitalise: bool, text: &str) -> Section {
    Section { title: title.to_owned(), capitalise, text: text.to_owned() }
}

/// A song made up for these tests.
fn song() -> Song {
    Song {
        title: "Свет над рекой".to_owned(),
        author: "Автор Тестов".to_owned(),
        copyright: "Тестовое издательство".to_owned(),
        year: "1999".to_owned(),
        musical_key: "G".to_owned(),
        sections: vec![
            section("Verse 1", true, "свет над рекой\nпесня над домом\n\nвторая часть первого"),
            section("Chorus 1", false, "Домой, домой"),
            section("Verse 2", true, "вечер настал"),
        ],
        versions: vec![
            Version {
                lang: "en".to_owned(),
                sections: vec![
                    section("Chorus 1", false, "Home, home"),
                    section("Verse 1", true, "light over the river\nsong over the house\n\nsecond part of the first"),
                    section("Bridge 1", false, "only in english"),
                ],
            },
            Version {
                lang: "uk".to_owned(), sections: vec![section("Verse 1", false, "світло над річкою")]
            },
        ],
        chords: false,
    }
}

fn texts(song: &Presentation, role: &Role) -> Vec<Option<String>> {
    song.groups.iter().flat_map(|g| g.slides.iter().map(|s| s.text_for(role).map(str::to_owned))).collect()
}

#[test]
fn sections_become_groups_with_the_packs_names_and_capitals_as_the_pack_says() {
    let made = present("ru", "ru:key", &song(), None, 0);
    let names: Vec<&str> = made.song.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Verse 1", "Chorus 1", "Verse 2"]);
    // A blank line starts another slide; capitalise puts a capital at the start of each line.
    assert_eq!(made.song.groups[0].slides.len(), 2);
    assert_eq!(
        texts(&made.song, &Role::primary()),
        [
            Some("Свет над рекой\nПесня над домом".to_owned()),
            Some("Вторая часть первого".to_owned()),
            Some("Домой, домой".to_owned()),
            Some("Вечер настал".to_owned()),
        ]
    );
    assert_eq!(made.song.copyright.author, "Автор Тестов");
    assert_eq!(made.song.copyright.year, "1999");
    assert_eq!(made.credit.text, "From Worship Leader App");
    assert!(made.credit.site.starts_with("https://"));
    assert_eq!(made.pairing, Pairing::default());
}

#[test]
fn a_version_is_lined_up_group_by_group_by_name_and_the_unpaired_are_recorded() {
    let made = present("ru", "ru:key", &song(), Some("en"), 0);
    // Chorus 1 comes first in the English, Verse 1 second: names decide, not places.
    assert_eq!(
        texts(&made.song, &Role::translation()),
        [
            Some("Light over the river\nSong over the house".to_owned()),
            Some("Second part of the first".to_owned()),
            Some("Home, home".to_owned()),
            None,
        ]
    );
    assert_eq!(made.pairing.second.as_deref(), Some("en"));
    assert_eq!(made.pairing.only_main, ["Verse 2"], "Verse 2 shows one language");
    assert_eq!(made.pairing.only_second, ["Bridge 1"], "a group only the version has is left out");
    assert!(made.song.groups.iter().all(|g| g.name != "Bridge 1"));
    let mark = made.song.catalog.as_ref().expect("marked");
    assert_eq!(mark.second.as_deref(), Some("en"));
    // A language the song has no version in changes nothing.
    let none = present("ru", "ru:key", &song(), Some("fr"), 0);
    assert_eq!(none.pairing, Pairing::default());
    assert!(texts(&none.song, &Role::translation()).iter().all(Option::is_none));
}

#[test]
fn a_group_name_used_twice_pairs_the_first_with_the_first() {
    let mut twice = song();
    twice.sections =
        vec![section("Chorus", false, "one"), section("Verse 1", false, "two"), section("Chorus", false, "three")];
    twice.versions = vec![Version {
        lang: "en".to_owned(),
        sections: vec![section("Chorus", false, "uno"), section("Chorus", false, "tres")],
    }];
    let made = present("ru", "k", &twice, Some("en"), 0);
    assert_eq!(texts(&made.song, &Role::translation()), [Some("uno".to_owned()), None, Some("tres".to_owned())]);
    assert_eq!(made.pairing.only_main, ["Verse 1"]);
}

#[test]
fn a_joined_copy_is_marked_with_where_it_came_from() {
    let made = present("ru", "ru:key~1", &song(), Some("en"), 1_700_000_000);
    let mark = made.song.catalog.clone().expect("marked");
    assert_eq!(mark.source, "worshipleaderapp");
    assert_eq!(mark.key, "ru:key~1");
    assert_eq!(mark.language, "ru");
    assert_eq!(mark.text_hash, read::text_hash(&song()));
    assert_eq!(mark.made_hash, fingerprint(&Presentation { catalog: None, ..made.song.clone() }));
    assert_eq!(mark.added, "2023-11-14");
    assert!(mark.newer.is_none() && !mark.gone);
}

#[test]
fn the_fingerprint_ignores_ids_and_the_mark_and_sees_every_edit() {
    let one = present("ru", "k", &song(), None, 0).song;
    let two = present("ru", "k", &song(), None, 99).song;
    assert_ne!(one.id, two.id);
    assert_eq!(fingerprint(&one), fingerprint(&two), "made twice from one text, the same");
    let edit = |change: &dyn Fn(&mut Presentation)| {
        let mut changed = one.clone();
        change(&mut changed);
        assert_ne!(fingerprint(&changed), fingerprint(&one));
    };
    edit(&|p| p.title.push('!'));
    edit(&|p| p.copyright.year = "2001".to_owned());
    edit(&|p| p.groups[0].name = "Intro".to_owned());
    edit(&|p| p.theme = Some(Uuid::new_v4()));
    edit(&|p| p.loops = true);
    edit(&|p| {
        let id = p.groups[1].id;
        p.arrangements.push(Arrangement {
            id: Uuid::new_v4(),
            name: "Short".to_owned(),
            groups: vec![id],
            auto: false,
        });
    });
    edit(&|p| p.groups[0].slides[0].label = Some("Opening".to_owned()));
    edit(&|p| p.groups[0].slides[0].set_words("Другие слова", ""));
}

#[test]
fn an_arrangement_is_fingerprinted_by_its_groups_places_not_their_ids() {
    let make = || {
        let mut p = present("ru", "k", &song(), None, 0).song;
        let order = vec![p.groups[1].id, p.groups[0].id];
        let arrangement =
            Arrangement { id: Uuid::new_v4(), name: "Chorus first".to_owned(), groups: order, auto: false };
        p.arrangement = Some(arrangement.id);
        p.arrangements.push(arrangement);
        p
    };
    assert_eq!(fingerprint(&make()), fingerprint(&make()));
}

#[test]
fn an_untouched_song_is_updated_a_changed_one_is_only_offered_and_never_overwritten() {
    let old = present("ru", "k", &song(), None, 0).song;
    let mut newer_text = song();
    newer_text.sections[1].text = "Домой, домой, домой".to_owned();
    let newer_hash = read::text_hash(&newer_text);
    let has = Source::Has(newer_hash.clone());

    // Untouched: replaced.
    assert_eq!(judge(&old, &has), Verdict::Update);
    let fresh = present("ru", "k", &newer_text, None, 5);
    let updated = refreshed(&old, fresh);
    assert_eq!(updated.id, old.id, "the same song in the library");
    assert!(texts(&updated, &Role::primary()).contains(&Some("Домой, домой, домой".to_owned())));
    let mark = updated.catalog.as_ref().expect("marked");
    assert_eq!(mark.text_hash, newer_hash);
    assert_eq!(mark.added, old.catalog.as_ref().expect("marked").added, "when it joined stays");
    assert_eq!(judge(&updated, &has), Verdict::Current, "now up to date, and unchanged again");

    // Changed by the church (any edit counts): offered, the song left as it is.
    let mut mine = old.clone();
    mine.groups[0].slides[0].set_words("Мои слова", "");
    let verdict = judge(&mine, &has);
    assert_eq!(verdict, Verdict::Offer(newer_hash.clone()));
    let offered = marked(&mine, &verdict).expect("a mark");
    assert_eq!(offered.groups, mine.groups, "never overwritten");
    assert_eq!(offered.catalog.as_ref().and_then(|m| m.newer.clone()), Some(newer_hash.clone()));
    // The same offer isn't made twice.
    assert_eq!(judge(&offered, &has), Verdict::Current);
    // A theme chosen is a change too.
    let mut themed = old.clone();
    themed.theme = Some(Uuid::new_v4());
    assert!(matches!(judge(&themed, &has), Verdict::Offer(_)));

    // Use the newer version: the whole song is the catalog's again, keeping the theme chosen.
    let taken = refreshed(&offered, present("ru", "k", &newer_text, None, 6));
    assert_eq!(taken.id, mine.id);
    assert_eq!(texts(&taken, &Role::primary())[0], Some("Свет над рекой\nПесня над домом".to_owned()));
    assert_eq!(judge(&taken, &has), Verdict::Current);
    let taken_themed = refreshed(&themed, present("ru", "k", &newer_text, None, 6));
    assert_eq!(taken_themed.theme, themed.theme);
    assert_eq!(judge(&taken_themed, &has), Verdict::Current);

    // Keep mine: the same newer version is not offered again, and the song stays changed.
    let kept_song = kept(&offered).expect("had an offer");
    assert!(kept_song.catalog.as_ref().is_some_and(|m| m.newer.is_none() && m.text_hash == newer_hash));
    assert_eq!(judge(&kept_song, &has), Verdict::Current);
    assert_eq!(kept(&kept_song), None, "nothing offered, nothing to keep");
    // Yet a still newer one is offered.
    assert!(matches!(judge(&kept_song, &Source::Has("0123456789abcdef".to_owned())), Verdict::Offer(_)));
}

#[test]
fn a_song_the_source_dropped_is_noted_on_its_copy_which_stays() {
    let copy = present("ru", "k", &song(), None, 0).song;
    assert_eq!(judge(&copy, &Source::Gone), Verdict::Gone);
    let noted = marked(&copy, &Verdict::Gone).expect("a mark");
    assert!(noted.catalog.as_ref().is_some_and(|m| m.gone));
    assert_eq!(noted.groups, copy.groups);
    assert_eq!(judge(&noted, &Source::Gone), Verdict::Current, "said once");
    // If it comes back unchanged, the note is taken off.
    let back = Source::Has(read::text_hash(&song()));
    assert_eq!(judge(&noted, &back), Verdict::Tidy);
    assert!(marked(&noted, &Verdict::Tidy).is_some_and(|p| p.catalog.is_some_and(|m| !m.gone)));
    // A language not downloaded says nothing; the church's own songs have nothing to judge.
    assert_eq!(judge(&copy, &Source::Unknown), Verdict::Current);
    let mut own = copy.clone();
    own.catalog = None;
    assert_eq!(judge(&own, &Source::Gone), Verdict::Current);
}

#[test]
fn undoing_the_newer_version_puts_the_song_back_with_its_mark() {
    use std::sync::Arc;
    let mine = {
        let mut p = present("ru", "k", &song(), None, 0).song;
        p.groups[0].slides[0].set_words("Мои слова", "");
        p
    };
    let mut newer_text = song();
    newer_text.title = "Свет над рекой".to_owned();
    newer_text.sections[2].text = "вечер настал и прошёл".to_owned();
    let offered = marked(&mine, &Verdict::Offer(read::text_hash(&newer_text))).expect("a mark");
    // The app's history keeps the song as it was before the change and gives it back on Undo.
    let mut history = crate::ShowHistory::default();
    let before = Arc::new(offered.clone());
    history.remember(0, before.clone());
    let after = Arc::new(refreshed(&offered, present("ru", "k", &newer_text, None, 1)));
    assert_ne!(after.groups, before.groups);
    let (index, back) = history.step(false, after.clone()).expect("a step to undo");
    assert_eq!(index, 0);
    assert_eq!(*back, offered, "my words, and the offer still showing");
    // And Redo takes the newer version again.
    let (_, again) = history.step(true, back).expect("a step to redo");
    assert_eq!(*again, *after);
}
