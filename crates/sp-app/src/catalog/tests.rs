use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::read::{self, LIMITS, Refused};
use super::*;

/// A song file written for these tests.
fn xml(title: &str, sections: &[(&str, &str)], versions: &[(&str, &[(&str, &str)])]) -> String {
    let part = |sections: &[(&str, &str)]| {
        sections
            .iter()
            .map(|(name, words)| {
                format!("<section title=\"{name}\" capitalise=\"true\"><lyrics>{words}\n</lyrics></section>")
            })
            .collect::<String>()
    };
    let more: String = versions
        .iter()
        .map(|(lang, sections)| {
            format!("<translationoptions><lang>{lang}</lang><lyrics>{}</lyrics></translationoptions>", part(sections))
        })
        .collect();
    format!("<song><title>{title}</title><author>Автор</author><lyrics>{}</lyrics>{more}</song>", part(sections))
}

/// A song pack (a zip) of these files.
fn pack(files: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            zip.start_file(format!("uk/{name}"), options).expect("start");
            zip.write_all(body.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");
    }
    out
}

fn simple(title: &str, first: &str) -> (String, String) {
    (format!("{title}.xml"), xml(title, &[("Verse 1", first), ("Chorus 1", "общий припев")], &[]))
}

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("softpresenter-test-catalog-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

fn catalog(name: &str) -> Catalog {
    Catalog::at(folder(name))
}

#[test]
fn a_song_pack_is_read_into_sections_versions_and_details() {
    let file = xml(
        "Свет над рекой",
        &[("Verse 1", "первая строка\nвторая строка"), ("Chorus 1", "припев здесь")],
        &[
            ("ru", &[("Verse 1", "1 куплет:\nпервая строка по-русски"), ("Chorus 1", "припев")]),
            ("en", &[("Verse 1", "first line")]),
        ],
    );
    let song = read::read_song(file.as_bytes()).expect("well formed").expect("words");
    assert_eq!(song.title, "Свет над рекой");
    assert_eq!(song.author, "Автор");
    assert_eq!(song.sections.len(), 2);
    assert_eq!(song.sections[0].title, "Verse 1");
    assert!(song.sections[0].capitalise);
    assert_eq!(song.sections[0].text, "первая строка\nвторая строка");
    assert_eq!(song.versions.iter().map(|v| v.lang.as_str()).collect::<Vec<_>>(), ["ru", "en"]);
    assert_eq!(
        song.versions[0].sections[0].text, "первая строка по-русски",
        "the line that only names the section goes"
    );
    assert!(!song.chords);
}

#[test]
fn chord_lines_and_naming_lines_come_out_and_real_lines_stay() {
    let (text, chords) = {
        let file = xml("Песня", &[("Verse 1", ".G  Am7  C/E\nпервая строка\nПриспів:\nДомой, домой")], &[]);
        let song = read::read_song(file.as_bytes()).expect("ok").expect("words");
        (song.sections[0].text.clone(), song.chords)
    };
    assert!(chords);
    // "Приспів:" is in the middle: only a first line that names the section is dropped.
    assert_eq!(text, "первая строка\nПриспів:\nДомой, домой");
    let file = xml("Песня", &[("Verse 1", "...и дальше слова\n. Ещё слово\nкак Ты нас любишь:")], &[]);
    let song = read::read_song(file.as_bytes()).expect("ok").expect("words");
    assert_eq!(song.sections[0].text.lines().count(), 3, "lines that merely start with dots are words");
    assert!(!song.chords);
}

#[test]
fn a_pack_with_a_bad_file_is_read_without_it_and_counts_what_it_left_out() {
    let mut files = vec![simple("Первая", "слова один"), simple("Вторая", "слова два")];
    files.push((
        "broken.xml".to_owned(),
        "<song><title>Сломана</title><lyrics><section>слова <<>></section></lyrics></song>".to_owned(),
    ));
    files.push(("empty.xml".to_owned(), "<song><title>Пустая</title><lyrics></lyrics></song>".to_owned()));
    files.push(("notes.txt".to_owned(), "not a song".to_owned()));
    files.push(("other.xml".to_owned(), "<document><title>x</title></document>".to_owned()));
    // The same song twice (as a pack's "Title" and "Title 2" are), and a repeat with other words.
    files.push(("Первая 2.xml".to_owned(), simple("Первая", "слова один").1));
    files.push(("Первая 3.xml".to_owned(), simple("Первая", "слова один").1.replace("общий припев", "другой припев")));
    let read = read::read_pack(&pack(&files)).expect("read");
    assert_eq!(read.songs.len(), 3);
    assert_eq!(read.malformed, 2, "a broken file and one that isn't a song");
    assert_eq!(read.empty, 1);
    assert_eq!(read.duplicates, 1);
    let titles: Vec<&str> = read.songs.iter().map(|(_, _, s)| s.title.as_str()).collect();
    assert_eq!(titles, ["Первая", "Вторая", "Первая"]);
}

#[test]
fn files_that_are_dangerous_xml_are_skipped_not_followed() {
    let entity = "<!DOCTYPE song [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;\">]>";
    let bomb = format!(
        "{entity}<song><title>&b;</title><lyrics><section title=\"V\"><lyrics>&b;</lyrics></section></lyrics></song>"
    );
    assert!(read::read_song(bomb.as_bytes()).is_err(), "a document type is refused");
    let deep = format!("<song>{}{}</song>", "<a>".repeat(5_000), "</a>".repeat(5_000));
    assert!(read::read_song(deep.as_bytes()).is_err(), "too deep");
    let wide = format!(
        "<song><title>x</title><lyrics>{}</lyrics></song>",
        "<section title=\"V\"><lyrics>a</lyrics></section>".repeat(600)
    );
    assert!(read::read_song(wide.as_bytes()).is_err(), "too many sections");
    let long = format!(
        "<song><title>x</title><lyrics><section title=\"V\"><lyrics>{}</lyrics></section></lyrics></song>",
        "a".repeat(70_000)
    );
    assert!(read::read_song(long.as_bytes()).is_err(), "a section far too long");
    assert!(read::read_song(&[0xff, 0xfe, 0x00]).is_err(), "not text");
}

#[test]
fn a_zip_that_is_too_big_or_too_odd_is_refused_whole() {
    assert_eq!(read::read_pack(b"not a zip at all").unwrap_err(), Refused::NotAZip);
    assert_eq!(read::read_pack(&[]).unwrap_err(), Refused::NotAZip);
    // Too many entries.
    let many: Vec<(String, String)> = (0..LIMITS.entries + 1).map(|n| (format!("{n}.xml"), String::new())).collect();
    assert_eq!(read::read_pack(&pack(&many)).unwrap_err(), Refused::TooMany);
    // One file bigger than the limit for one file, unpacked (it packs small).
    let big = "a".repeat(LIMITS.entry as usize + 1);
    assert_eq!(read::read_pack(&pack(&[("big.xml".to_owned(), big)])).unwrap_err(), Refused::TooBig);
    // A zip with no songs in it.
    assert_eq!(read::read_pack(&pack(&[("readme.txt".to_owned(), "hello".to_owned())])).unwrap_err(), Refused::NoSongs);
    // A zip inside the zip is not opened.
    let inner = pack(&[simple("Внутри", "слова")]);
    let mut outer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut outer));
        zip.start_file("inner.xml", zip::write::SimpleFileOptions::default()).expect("start");
        zip.write_all(&inner).expect("write");
        zip.finish().expect("finish");
    }
    assert_eq!(read::read_pack(&outer).unwrap_err(), Refused::NoSongs);
}

#[test]
fn a_songs_key_is_made_from_its_title_and_first_section_and_stays_put() {
    let song = |title: &str, first: &str| {
        read::read_song(xml(title, &[("Verse 1", first), ("Chorus 1", "другое")], &[]).as_bytes())
            .expect("ok")
            .expect("words")
    };
    let a = song("Свет над рекой", "первая строка");
    let key = read::song_key("ru", &a);
    assert!(key.starts_with("ru:свет-над-рекой~"), "{key}");
    assert_eq!(key.len(), "ru:свет-над-рекой~".len() + 8);
    // The same song, however it is read or written: case, ё/е, punctuation and spacing don't matter.
    let b = song("СВЕТ  над рекой!", "Первая   строка");
    assert_eq!(read::song_key("ru", &b), key);
    let yo = read::song_key("ru", &song("Всё хорошо", "слова"));
    assert_eq!(yo, read::song_key("ru", &song("все хорошо", "слова")));
    // The language is part of it; the words after the first section are not; the first section is.
    assert_ne!(read::song_key("uk", &a), key);
    let later = song("Свет над рекой", "первая строка");
    assert_eq!(read::song_key("ru", &later), key);
    assert_ne!(read::song_key("ru", &song("Свет над рекой", "совсем другая")), key);
    // Order in the pack makes no difference, and two songs that share a title and first section
    // but differ after it are told apart by their file names, which stay when their words are
    // corrected.
    let one = read::Song {
        sections: vec![
            a.sections[0].clone(),
            read::Section { title: "Verse 2".into(), capitalise: true, text: "один".into() },
        ],
        ..a.clone()
    };
    let two = read::Song {
        sections: vec![
            a.sections[0].clone(),
            read::Section { title: "Verse 2".into(), capitalise: true, text: "два".into() },
        ],
        ..a.clone()
    };
    let keys = |songs: Vec<(usize, String, read::Song)>| {
        let mut keyed: Vec<(String, usize)> =
            read::keyed("ru", songs).into_iter().map(|(key, at, _)| (key, at)).collect();
        keyed.sort_by_key(|(_, at)| *at);
        keyed
    };
    let forward = keys(vec![(0, "a.xml".into(), one.clone()), (1, "a 2.xml".into(), two.clone())]);
    let backward = keys(vec![(1, "a 2.xml".into(), two.clone()), (0, "a.xml".into(), one.clone())]);
    assert_eq!(forward, backward, "the key follows the song, not its place");
    assert_ne!(forward[0].0, forward[1].0);
    assert!(forward[0].0.contains('.'));
    // A correction to the words after the first section changes neither key.
    let corrected = read::Song {
        sections: vec![
            a.sections[0].clone(),
            read::Section { title: "Verse 2".into(), capitalise: true, text: "один, исправлено".into() },
        ],
        ..a.clone()
    };
    let after = keys(vec![(0, "a.xml".into(), corrected), (1, "a 2.xml".into(), two)]);
    assert_eq!(after, forward);
    // A song with no title still has a key.
    let nameless = read::Song { title: String::new(), ..a };
    assert!(read::song_key("ru", &nameless).starts_with("ru:song~"));
}

/// A catalog of `count` made-up Russian songs in a pack.
fn big_pack(count: usize) -> Vec<u8> {
    const WORDS: [&str; 12] =
        ["свет", "река", "дом", "вечер", "небо", "радость", "путь", "тихо", "слово", "берег", "ночь", "утро"];
    let files: Vec<(String, String)> = (0..count)
        .map(|n| {
            let (a, b, c) = (WORDS[n % 12], WORDS[(n / 12) % 12], WORDS[(n / 144) % 12]);
            let title = format!("{} {} {} {n}", a, b, c);
            (
                format!("{n}.xml"),
                xml(&title, &[("Verse 1", &format!("{b} над {c}ом\nвторая {a}")), ("Chorus 1", "общий припев")], &[]),
            )
        })
        .collect();
    pack(&files)
}

#[test]
fn search_finds_words_in_titles_and_first_lines_whatever_the_case_or_yo() {
    let catalog = catalog("search");
    let files = vec![
        ("a.xml".to_owned(), xml("Всё в Твоих руках", &[("Verse 1", "Ещё один день\nконец строки")], &[])),
        ("b.xml".to_owned(), xml("Благодать", &[("Verse 1", "Чудная благодать Твоя")], &[])),
        ("c.xml".to_owned(), xml("Другая песня", &[("Verse 1", "Слова про благодать")], &[])),
        ("d.xml".to_owned(), xml("Amazing Grace", &[("Verse 1", "Grace that found me")], &[])),
        ("e.xml".to_owned(), xml("Свет над рекой", &[("Verse 1", "Тихо над рекой")], &[])),
    ];
    catalog.install("ru", &pack(&files), None, None, 0).expect("install");
    let find = |typed: &str| catalog.search(typed, 10, &[], &HashSet::new());
    // Case and ё/е.
    assert_eq!(find("всё в твоих").hits[0].title, "Всё в Твоих руках");
    assert_eq!(find("ВСЕ В ТВОИХ РУКАХ").hits[0].title, "Всё в Твоих руках");
    assert_eq!(find("еще один").hits[0].title, "Всё в Твоих руках", "found by its first line, ё as е");
    // A title that is the words comes before a song that only holds them in a line.
    let grace = find("благодать");
    assert_eq!(grace.total, 2);
    assert_eq!(grace.hits[0].title, "Благодать");
    assert!(grace.hits[0].in_title);
    assert_eq!(grace.hits[1].title, "Другая песня");
    assert!(!grace.hits[1].in_title);
    assert_eq!(grace.hits[0].lang, "ru");
    // Latin, parts of words, several words in any order, punctuation.
    assert_eq!(find("AMAZING").hits[0].key.split(':').next(), Some("ru"));
    assert_eq!(find("grace found").hits[0].title, "Amazing Grace");
    assert_eq!(find("рекой, свет!").hits[0].title, "Свет над рекой");
    // Nothing, and nothing typed.
    assert!(find("несуществующее").hits.is_empty());
    assert_eq!(find("  ").total, 0);
    // Songs already in the library are left to the library.
    let key = grace.hits[0].key.clone();
    let skipped = catalog.search("благодать", 10, &[], &HashSet::from([key]));
    assert_eq!(skipped.total, 1);
    assert_eq!(skipped.hits[0].title, "Другая песня");
    // The limit says how many are returned and the total how many matched.
    let limited = catalog.search("благодать", 1, &[], &HashSet::new());
    assert_eq!((limited.hits.len(), limited.total), (1, 2));
}

#[test]
fn languages_asked_for_come_first_and_each_hit_says_its_language() {
    let catalog = catalog("languages");
    for lang in ["uk", "ru"] {
        let file = xml("Благодать", &[("Verse 1", "слова")], &[("en", &[("Verse 1", "words")])]);
        catalog.install(lang, &pack(&[("a.xml".to_owned(), file)]), None, None, 0).expect("install");
    }
    let found = catalog.search("благодать", 10, &["ru".to_owned()], &HashSet::new());
    assert_eq!(found.hits.iter().map(|h| h.lang.as_str()).collect::<Vec<_>>(), ["ru", "uk"]);
    let found = catalog.search("благодать", 10, &["uk".to_owned()], &HashSet::new());
    assert_eq!(found.hits[0].lang, "uk");
    assert_eq!(found.hits[0].versions, ["en"], "the languages it is also in");
}

#[test]
fn searching_fourteen_thousand_songs_is_fast() {
    let catalog = catalog("speed");
    let bytes = big_pack(14_000);
    let started = std::time::Instant::now();
    let done = catalog.install("ru", &bytes, None, None, 0).expect("install");
    let installing = started.elapsed();
    assert_eq!(done.songs, 14_000);
    // A new Catalog reads the index from disk, as the app does after a start.
    let again = Catalog::at(catalog.root().to_owned());
    let started = std::time::Instant::now();
    let index = again.index("ru").expect("index").expect("there");
    let loading = started.elapsed();
    assert_eq!(index.entries.len(), 14_000);
    let mut slowest = std::time::Duration::ZERO;
    for typed in ["свет", "радость над небо", "ВЕЧЕР", "берег ночь утро 13", "слово слово", "нет такого слова"]
    {
        let started = std::time::Instant::now();
        let found = again.search(typed, 20, &[], &HashSet::new());
        slowest = slowest.max(started.elapsed());
        assert!(found.hits.len() <= 20);
    }
    eprintln!(
        "14,000 songs: pack {} KB, index files {} KB on disk; install {installing:?}, load index {loading:?}, slowest search {slowest:?}",
        bytes.len() / 1024,
        again.disk_bytes("ru") / 1024
    );
    assert!(slowest < std::time::Duration::from_millis(100), "a search took {slowest:?}");
}

#[test]
fn a_song_is_read_from_the_kept_pack_by_its_key() {
    let catalog = catalog("read-back");
    let files = vec![
        ("a.xml".to_owned(), xml("Первая", &[("Verse 1", "один")], &[("en", &[("Verse 1", "one")])])),
        ("b.xml".to_owned(), xml("Вторая", &[("Verse 1", "два")], &[])),
    ];
    catalog.install("ru", &pack(&files), None, None, 0).expect("install");
    let key = catalog.search("вторая", 5, &[], &HashSet::new()).hits[0].key.clone();
    let (lang, entry, song) = catalog.song(&key).expect("song");
    assert_eq!((lang.as_str(), song.title.as_str()), ("ru", "Вторая"));
    assert_eq!(entry.key, key);
    let key = catalog.search("первая", 5, &[], &HashSet::new()).hits[0].key.clone();
    let made = catalog.present(&key, Some("en"), 0).expect("present");
    assert_eq!(made.pairing.second.as_deref(), Some("en"));
    assert!(catalog.song("ru:нет-такой~00000000").is_err());
    assert!(catalog.song("../etc:x").is_err());
    assert!(catalog.song("nonsense").is_err());
    // The listing is in title order.
    let listed = catalog.list("ru", 0, 10);
    assert_eq!(listed.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["Вторая", "Первая"]);
}

#[test]
fn language_codes_are_checked_before_they_reach_a_folder_or_an_address() {
    for good in ["ru", "uk", "en", "kk-en", "pt-br", "ru-ca"] {
        assert!(valid_language(good), "{good}");
    }
    for bad in ["", "r", "RU", "russian", "../x", "ru/x", "ru..", "ru-", "-ru", "ru-ABC", "ru x", "ру", "ru\0", "a/b"]
    {
        assert!(!valid_language(bad), "{bad:?}");
    }
    let catalog = catalog("bad-lang");
    assert!(catalog.download_from("http://127.0.0.1:1", "../x", 0, None).is_err());
    assert!(catalog.remove("../x").is_err());
    assert!(catalog.index("../x").is_err());
}

#[test]
fn installing_notes_the_songs_that_are_gone_and_a_pack_that_empties_the_catalog_is_not_believed() {
    let catalog = catalog("gone");
    let files: Vec<(String, String)> = (0..60).map(|n| simple(&format!("Песня {n}"), &format!("слова {n}"))).collect();
    catalog.install("ru", &pack(&files), Some("\"v1\"".to_owned()), None, 1_700_000_000).expect("install");
    let before = catalog.index("ru").expect("index").expect("there");
    let dropped = before.entries.iter().find(|e| e.title == "Песня 7").expect("song 7").key.clone();
    // The next pack lacks one song and changes another.
    let mut next: Vec<(String, String)> = files.iter().filter(|(name, _)| name != "Песня 7.xml").cloned().collect();
    next.iter_mut().find(|(name, _)| name == "Песня 8.xml").expect("8").1 =
        xml("Песня 8", &[("Verse 1", "слова 8"), ("Chorus 1", "новый припев")], &[]);
    let done = catalog.install("ru", &pack(&next), Some("\"v2\"".to_owned()), None, 1_702_000_000).expect("install");
    assert_eq!((done.songs, done.gone, done.changed, done.added), (59, 1, 1, 0));
    let after = catalog.index("ru").expect("index").expect("there");
    assert!(after.entry(&dropped).is_none());
    assert_eq!(after.is_gone(&dropped).map(|g| g.title.as_str()), Some("Песня 7"));
    assert_eq!(after.is_gone(&dropped).map(|g| g.since.as_str()), Some("2023-12-08"));
    assert_eq!(after.info().etag.as_deref(), Some("\"v2\""));
    assert!(matches!(catalog.source_of("ru", &dropped), Source::Gone));
    assert!(matches!(catalog.source_of("uk", &dropped), Source::Unknown), "a language not here says nothing");
    // A pack with a handful of songs where there were 59 is not believed: the old one stays.
    let few: Vec<(String, String)> = files.iter().take(5).cloned().collect();
    let refused = catalog.install("ru", &pack(&few), None, None, 1_703_000_000).unwrap_err();
    assert!(refused.contains("not believed"), "{refused}");
    assert_eq!(catalog.index("ru").expect("index").expect("there").entries.len(), 59);
    // A damaged download changes nothing either.
    assert!(catalog.install("ru", b"PK\x03\x04 broken", None, None, 1_703_000_000).is_err());
    assert!(catalog.install("ru", &pack(&[("x.xml".into(), "<song>".into())]), None, None, 0).is_err());
    assert_eq!(catalog.index("ru").expect("index").expect("there").entries.len(), 59);
    // A song that comes back is no longer gone.
    catalog.install("ru", &pack(&files), None, None, 1_704_000_000).expect("install");
    assert!(catalog.index("ru").expect("index").expect("there").is_gone(&dropped).is_none());
}

#[test]
fn the_catalog_keeps_one_pack_per_language_in_a_folder_of_its_own_and_can_be_removed() {
    let catalog = catalog("layout");
    catalog.install("ru", &pack(&[simple("Песня", "слова")]), None, None, 5).expect("install");
    catalog.install("ru", &pack(&[simple("Песня", "слова"), simple("Ещё", "слова")]), None, None, 6).expect("again");
    let mut names: Vec<String> = std::fs::read_dir(catalog.root().join("ru"))
        .expect("folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.contains(&"index.json".to_owned()) && names.contains(&"status.json".to_owned()));
    assert!(names.iter().any(|n| n.starts_with("songs-") && n.ends_with(".qsp")));
    let languages = catalog.languages();
    assert_eq!(languages.len(), 1);
    assert_eq!((languages[0].songs, languages[0].fetched), (2, 6));
    assert!(languages[0].disk_bytes >= languages[0].pack_bytes);
    // A pack damaged on disk is said to need downloading again, not trusted.
    let pack_file = std::fs::read_dir(catalog.root().join("ru"))
        .expect("folder")
        .flatten()
        .find(|e| e.file_name().to_string_lossy().ends_with(".qsp"))
        .expect("pack");
    std::fs::write(pack_file.path(), b"short").expect("write");
    let fresh = Catalog::at(catalog.root().to_owned());
    assert!(fresh.index("ru").unwrap_err().contains("Download it again"));
    assert!(fresh.remove("ru").expect("removed"));
    assert!(fresh.installed().is_empty());
    assert!(!fresh.remove("ru").expect("nothing to remove"));
}

#[test]
fn an_old_data_folder_without_a_catalog_loads_fine() {
    let nowhere = catalog("none");
    assert!(nowhere.installed().is_empty());
    assert!(nowhere.languages().is_empty());
    assert!(nowhere.index("ru").expect("no error").is_none());
    assert_eq!(nowhere.search("свет", 5, &[], &HashSet::new()), Found::default());
    assert!(nowhere.due(1_700_000_000).is_empty());
    assert!(nowhere.list("ru", 0, 5).is_empty());
    // A song file saved before songs had a catalog mark opens, with none; one with a mark keeps it.
    let plain = sp_model::parse_song_text("Old", "[Verse 1]\nWords\n");
    let text = sp_model::to_json(&plain);
    assert!(!text.contains("catalog"));
    assert_eq!(sp_model::from_json(&text).expect("opens").catalog, None);
    let marked = present(
        "ru",
        "ru:k~1",
        &read::read_song(xml("Т", &[("Verse 1", "с")], &[]).as_bytes()).expect("ok").expect("words"),
        None,
        0,
    )
    .song;
    let back = sp_model::from_json(&sp_model::to_json(&marked)).expect("opens");
    assert_eq!(back.catalog, marked.catalog);
    // The catalog's folder is beside the library's, never inside Songs.
    assert!(Catalog::in_data_dir().is_none_or(|c| c.root().ends_with("Catalog/worshipleaderapp")));
}

#[test]
fn what_an_update_does_about_each_copy_in_the_library() {
    let catalog = catalog("plan");
    let v1 = vec![simple("Первая", "слова один"), simple("Вторая", "слова два"), simple("Третья", "слова три")];
    catalog.install("ru", &pack(&v1), None, None, 0).expect("install");
    let join = |title: &str| {
        let key = catalog.search(title, 1, &[], &HashSet::new()).hits[0].key.clone();
        catalog.present(&key, None, 0).expect("present").song
    };
    let (untouched, mut touched, dropped) = (join("первая"), join("вторая"), join("третья"));
    touched.title = "Вторая, моя".to_owned();
    let own = sp_model::parse_song_text("Моя", "[Verse 1]\nслова\n");
    // The next pack: the first and second change their text, the third is gone.
    let v2 = vec![
        ("Первая.xml".to_owned(), xml("Первая", &[("Verse 1", "слова один"), ("Chorus 1", "новый припев")], &[])),
        ("Вторая.xml".to_owned(), xml("Вторая", &[("Verse 1", "слова два"), ("Chorus 1", "тоже новый")], &[])),
        simple("Четвёртая", "слова четыре"),
        simple("Пятая", "слова пять"),
    ];
    // Four songs of five-sixths of three... the shrink check needs a bigger pack, so this is
    // judged against what the first install and this plan give: install it as the next pack.
    catalog.install("ru", &pack(&v2), None, None, 100).expect("install");
    let songs = [untouched.clone(), touched.clone(), dropped.clone(), own];
    let plan = catalog.plan(songs.iter().enumerate());
    assert_eq!(plan.len(), 3, "{plan:?}");
    assert_eq!(plan[0], (0, Verdict::Update));
    assert!(matches!(plan[1], (1, Verdict::Offer(_))), "the church changed it: only offered");
    assert_eq!(plan[2], (2, Verdict::Gone));
    // Run each, as the app does: the changed song is never overwritten.
    let key = untouched.catalog.as_ref().expect("marked").key.clone();
    let fresh = catalog.present(&key, None, 200).expect("present");
    let updated = refreshed(&untouched, fresh);
    assert!(
        updated
            .groups
            .iter()
            .any(|g| g.slides.iter().any(|s| s.text_for(&sp_model::Role::primary()) == Some("Новый припев")))
    );
    assert_eq!(judge(&updated, &catalog.source_of("ru", &key)), Verdict::Current);
    let offered = marked(&touched, &plan[1].1).expect("mark");
    assert_eq!(offered.title, "Вторая, моя");
    assert_eq!(catalog.plan([(0, &offered)]).len(), 0, "offered once, not again");
    // A language that isn't downloaded is left alone.
    let other = Catalog::at(folder("plan-empty"));
    assert!(other.plan(songs.iter().enumerate()).is_empty());
}

/// A web server on this computer that answers requests for packs, and says what it was asked.
struct Server {
    address: String,
    asked: Arc<Mutex<Vec<String>>>,
}

fn serve(etag: &'static str, modified: &'static str, body: Vec<u8>) -> Server {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let text = String::from_utf8_lossy(&request).into_owned();
            let line = text.lines().next().unwrap_or("").to_owned();
            let has =
                |name: &str, value: &str| text.lines().any(|l| l.to_lowercase().starts_with(name) && l.contains(value));
            let unchanged = has("if-none-match:", etag);
            log.lock().expect("log").push(format!(
                "{line} | none-match: {} | modified-since: {}",
                has("if-none-match:", etag),
                has("if-modified-since:", modified)
            ));
            let head = if line.contains("zz.qsp") {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
            } else if line.starts_with("HEAD") {
                format!(
                    "HTTP/1.1 200 OK\r\nETag: {etag}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            } else if unchanged {
                format!(
                    "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nLast-Modified: {modified}\r\nConnection: close\r\n\r\n"
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nETag: {etag}\r\nLast-Modified: {modified}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            };
            let _ = stream.write_all(head.as_bytes());
            if line.starts_with("GET") && !unchanged && !line.contains("zz.qsp") {
                let _ = stream.write_all(&body);
            }
        }
    });
    Server { address, asked }
}

#[test]
fn a_second_download_asks_with_the_servers_validators_and_a_304_costs_no_download() {
    let body = pack(&[simple("Песня", "слова"), simple("Ещё", "слова")]);
    let server = serve("\"abc-123\"", "Tue, 17 Nov 2026 20:43:49 GMT", body.clone());
    let catalog = catalog("conditional");
    let progress = Progress::new();
    // The first time: the whole pack, with its validators kept.
    let first = catalog.download_from(&server.address, "ru", 1_000, Some(&progress)).expect("download");
    assert!(matches!(first, Outcome::Installed(Installed { songs: 2, .. })), "{first:?}");
    assert_eq!(progress.per_cent(), Some(100));
    let language = &catalog.languages()[0];
    assert_eq!(language.etag.as_deref(), Some("\"abc-123\""));
    assert_eq!(language.last_modified.as_deref(), Some("Tue, 17 Nov 2026 20:43:49 GMT"));
    assert_eq!((language.fetched, language.checked, language.songs), (1_000, 1_000, 2));
    // The second: asked with both validators, answered 304, nothing installed, the check noted.
    let second = catalog.download_from(&server.address, "ru", 2_000, None).expect("check");
    assert!(matches!(second, Outcome::Unchanged { songs: 2 }), "{second:?}");
    let language = &catalog.languages()[0];
    assert_eq!((language.fetched, language.checked), (1_000, 2_000), "fetched stays, checked moves");
    let asked = server.asked.lock().expect("log").clone();
    assert_eq!(asked.len(), 2);
    assert!(
        asked[0].starts_with("GET /ru.qsp") && asked[0].ends_with("none-match: false | modified-since: false"),
        "{asked:?}"
    );
    assert!(asked[1].ends_with("none-match: true | modified-since: true"), "{asked:?}");
    // The size of a language not yet here comes from a request that downloads nothing.
    let sizes = catalog.sizes_from(&server.address, &["ru".to_owned(), "uk".to_owned(), "zz".to_owned()], true);
    assert_eq!((sizes[0].songs, sizes[0].download_bytes, sizes[0].installed), (Some(2), Some(body.len() as u64), true));
    assert_eq!((sizes[1].songs, sizes[1].download_bytes, sizes[1].installed), (None, Some(body.len() as u64), false));
    assert!(sizes[2].problem.as_deref().is_some_and(|p| p.contains("no zz songs")), "{:?}", sizes[2]);
    let asked = server.asked.lock().expect("log").clone();
    assert!(asked.iter().skip(2).all(|line| line.starts_with("HEAD")), "{asked:?}");
}

#[test]
fn a_check_is_due_about_monthly_and_sooner_after_a_failure() {
    let body = pack(&[simple("Песня", "слова")]);
    let server = serve("\"e\"", "Tue, 17 Nov 2026 20:43:49 GMT", body);
    let catalog = catalog("due");
    catalog.download_from(&server.address, "ru", 1_000_000, None).expect("download");
    assert!(catalog.due(1_000_000 + CHECK_EVERY - 1).is_empty());
    assert_eq!(catalog.due(1_000_000 + CHECK_EVERY), ["ru"]);
    // A source that can't be reached: the check fails, the songs stay, and it is tried again in a day.
    let failed = catalog.download_from("http://127.0.0.1:1", "ru", 5_000_000, None);
    assert!(failed.is_err());
    assert_eq!(catalog.languages()[0].songs, 1);
    assert!(catalog.languages()[0].problem.is_some());
    assert!(catalog.due(5_000_000 + RETRY_AFTER - 1).is_empty());
    assert_eq!(catalog.due(5_000_000 + RETRY_AFTER), ["ru"]);
    // A server that answers 404 is a failure said in words, not a crash.
    let missing = catalog.download_from(&server.address, "zz", 0, None).unwrap_err();
    assert!(missing.contains("404"), "{missing}");
}

#[test]
fn a_pack_bigger_than_allowed_or_cut_short_is_not_kept() {
    // A server that says it will send more than it does.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut chunk = [0_u8; 1024];
            let _ = stream.read(&mut chunk);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5000\r\nConnection: close\r\n\r\nshort");
        }
    });
    let catalog = catalog("cut-short");
    let cut = catalog.download_from(&address, "ru", 0, None).unwrap_err();
    assert!(cut.to_lowercase().contains("cut short") || cut.to_lowercase().contains("stopped"), "{cut}");
    assert!(catalog.installed().is_empty(), "nothing was kept");
    // A server that says the pack is enormous is not asked for the rest.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut chunk = [0_u8; 1024];
            let _ = stream.read(&mut chunk);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9999999999\r\nConnection: close\r\n\r\n");
        }
    });
    let big = catalog.download_from(&address, "ru", 0, None).unwrap_err();
    assert!(big.contains("bigger"), "{big}");
}
