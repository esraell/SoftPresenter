//! Checks on the Bibles that ship with the app.

use sp_bible::{BOOKS, Bible, Numbering, VerseRef, convert, parse_reference};

fn load(name: &str) -> Bible {
    let path = format!("{}/../../assets/bibles/{name}.spbible", env!("CARGO_MANIFEST_DIR"));
    Bible::from_bytes(&std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

fn name(v: VerseRef) -> String {
    format!("{} {}:{}", BOOKS[v.book].code, v.chapter, v.verse)
}

#[test]
fn each_shipped_bible_reads_and_knows_john_3_16() {
    for (id, words) in [
        ("kjv", "For God so loved"),
        ("web", "For God so loved"),
        ("bsb", "For God so loved"),
        ("syn", "Ибо так возлюбил"),
    ] {
        let bible = load(id);
        assert_eq!(bible.info.id, id);
        assert!(bible.verse_count() > 31_000, "{id} has {} verses", bible.verse_count());
        let found = bible.verses(parse_reference("John 3:16").unwrap());
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains(words), "{id}: {}", found[0].1);
    }
}

#[test]
fn the_king_james_text_carries_no_source_marks() {
    let kjv = load("kjv");
    assert!(kjv.all().all(|(_, words)| !words.contains(['[', ']', '¶']) && !words.contains("  ")));
    assert_eq!(
        kjv.verses(parse_reference("Psalm 23:1").unwrap())[0].1,
        "A Psalm of David. The LORD is my shepherd; I shall not want."
    );
}

#[test]
fn english_and_russian_psalms_line_up() {
    let (kjv, syn) = (load("kjv"), load("syn"));
    let english = parse_reference("Psalm 23:1").unwrap().start;
    let russian = convert(english, Numbering::English, Numbering::Synodal);
    assert!(syn.verse(russian).unwrap().contains("Пастырь"));
    let english = parse_reference("Psalm 51:1").unwrap().start;
    let russian = convert(english, Numbering::English, Numbering::Synodal);
    assert!(kjv.verse(english).unwrap().contains("Have mercy upon me"));
    assert!(syn.verse(russian).unwrap().contains("Помилуй меня"));
}

/// Every English verse should land on a verse the Russian text has, and the other way round.
/// The few that do not are the joined verses the numbering table says it does not cover.
#[test]
fn almost_every_verse_has_a_partner_in_the_other_numbering() {
    let (kjv, syn) = (load("kjv"), load("syn"));
    let lost: Vec<String> = kjv
        .all()
        .map(|(v, _)| v)
        .filter(|v| syn.verse(convert(*v, Numbering::English, Numbering::Synodal)).is_none())
        .map(name)
        .collect();
    let lost_back: Vec<String> = syn
        .all()
        .map(|(v, _)| v)
        .filter(|v| kjv.verse(convert(*v, Numbering::Synodal, Numbering::English)).is_none())
        .map(name)
        .collect();
    println!("English verses with no Russian partner: {lost:?}");
    println!("Russian verses with no English partner: {lost_back:?}");
    assert!(lost.len() <= 12, "{} English verses have no Russian partner: {lost:?}", lost.len());
    assert!(lost_back.len() <= 20, "{} Russian verses have no English partner: {lost_back:?}", lost_back.len());
}

/// Two English verses should not land on the same Russian verse, apart from the handful the
/// Russian text joins into one.
#[test]
fn english_verses_land_on_different_russian_verses() {
    let kjv = load("kjv");
    let mut seen = std::collections::HashMap::new();
    let mut shared = Vec::new();
    for (verse, _) in kjv.all() {
        let there = convert(verse, Numbering::English, Numbering::Synodal);
        if let Some(first) = seen.insert(there, verse) {
            shared.push(format!("{} and {} -> {}", name(first), name(verse), name(there)));
        }
    }
    println!("English verses sharing a Russian verse: {shared:?}");
    assert!(shared.len() <= 12, "{} English verses share a Russian verse: {shared:?}", shared.len());
}

/// The numbering judged from each shipped Bible's chapter lengths, as an imported Bible's is
/// judged, is the one its file names, and none is a loose fit.
#[test]
fn each_shipped_bibles_numbering_is_judged_from_its_chapters() {
    for id in ["kjv", "web", "bsb", "syn"] {
        let bible = load(id);
        let mut chapters = std::collections::BTreeMap::new();
        for (verse, _) in bible.all() {
            let last = chapters.entry((verse.book, verse.chapter)).or_insert(0);
            *last = verse.verse.max(*last);
        }
        let fit = sp_bible::fit(&chapters);
        assert_eq!((fit.numbering, fit.loose), (bible.info.numbering, false), "{id}: {fit:?}");
        assert!(fit.by_psalms, "{id}");
    }
}
