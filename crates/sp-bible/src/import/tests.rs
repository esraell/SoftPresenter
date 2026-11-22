//! Each format read from a small sample written for these tests (no published Bible's words),
//! and the ways an untrusted file could misbehave.

use std::io::Write;

use super::*;
use crate::{book_by_code, convert};

fn read(text: &str, file_name: &str) -> Imported {
    read_bytes(text.as_bytes(), file_name, &Limits::NORMAL).unwrap_or_else(|e| panic!("{file_name}: {e}"))
}

fn at(code: &str, chapter: u16, verse: u16) -> VerseRef {
    VerseRef { book: book_by_code(code).unwrap(), chapter, verse }
}

/// A zip holding the files given, deflated.
fn zipped(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Checks that a Bible made from an import reads back as the app's own file.
fn round_trip(imported: &Imported) -> Bible {
    let bible = imported.bible("test", &imported.prefilled());
    let again = Bible::from_bytes(&bible.to_bytes()).unwrap();
    assert_eq!(again.all().collect::<Vec<_>>(), bible.all().collect::<Vec<_>>());
    again
}

const ZEFANIA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<XMLBIBLE xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" biblename="Test Zefania Bible">
  <INFORMATION>
    <title>Another title</title>
    <identifier>TZB</identifier>
    <language>ENG</language>
    <rights>Written for a test.</rights>
  </INFORMATION>
  <BIBLEBOOK bnumber="1" bname="Genesis">
    <CHAPTER cnumber="1">
      <CAPTION vref="1">A heading that is not the text</CAPTION>
      <VERS vnumber="1">First <gr str="7225">words</gr> of the <STYLE css="color:#ff0000">test</STYLE>.<NOTE type="x-studynote">A note.</NOTE></VERS>
      <VERS vnumber="2">Second verse,<BR art="x-nl"/>on two lines.</VERS>
      <VERS vnumber="3">Half of a verse</VERS>
      <VERS vnumber="3">and its other half.</VERS>
      <VERS vnumber="4"><NOTE>Only a note.</NOTE></VERS>
    </CHAPTER>
  </BIBLEBOOK>
  <BIBLEBOOK bnumber="43" bname="John">
    <CHAPTER cnumber="3">
      <VERS vnumber="16">Words of a <STYLE css="color:#FF0000">teacher</STYLE> in red.<XREF fscope="x">Gen 1:1</XREF></VERS>
    </CHAPTER>
  </BIBLEBOOK>
  <BIBLEBOOK bnumber="67" bname="Tobit">
    <CHAPTER cnumber="1"><VERS vnumber="1">A book the app leaves out.</VERS></CHAPTER>
  </BIBLEBOOK>
</XMLBIBLE>"#;

#[test]
fn a_zefania_bible_reads_back_with_its_details_and_without_its_notes() {
    let imported = read(ZEFANIA, "test.xml");
    assert_eq!(imported.format, Format::Zefania);
    assert_eq!(
        imported.about,
        About {
            name: "Test Zefania Bible".to_owned(),
            short: "TZB".to_owned(),
            language: "en".to_owned(),
            terms: "Written for a test.".to_owned()
        }
    );
    assert!(imported.complete());
    assert_eq!(imported.verse(at("GEN", 1, 1)), Some("First words of the test."));
    assert_eq!(imported.verse(at("GEN", 1, 2)), Some("Second verse, on two lines."));
    assert_eq!(imported.verse(at("GEN", 1, 3)), Some("Half of a verse and its other half."), "fragments are joined");
    assert_eq!(imported.verse(at("GEN", 1, 4)), None, "a verse with only a note holds no words");
    assert_eq!(imported.verse(at("JOH", 3, 16)), Some("Words of a teacher in red."));
    assert_eq!((imported.book_count(), imported.verse_count(), imported.empty), (2, 4, 1));
    assert_eq!(imported.left_out, ["Tobit"]);
    let said = imported.summary("TZB");
    assert!(said.starts_with("Installed TZB (2 books, 4 verses), numbered as English Bibles are"), "{said}");
    assert!(said.contains("1 verse was empty") && said.contains("(Tobit)"), "{said}");
    let bible = round_trip(&imported);
    assert_eq!(
        (bible.info.name.as_str(), bible.info.short.as_str(), bible.info.language.as_str()),
        ("Test Zefania Bible", "TZB", "en")
    );
    assert_eq!(bible.info.source, "test.xml (Zefania XML)");
}

#[test]
fn an_osis_bible_reads_verses_held_or_marked_and_leaves_out_titles_and_notes() {
    let osis = r#"<?xml version="1.0" encoding="UTF-8"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
<osisText osisIDWork="TOB" xml:lang="ru">
  <header>
    <work osisWork="TOB"><title>Тестовая Библия</title><rights>Для проверки.</rights></work>
  </header>
  <div type="book" osisID="Gen">
    <chapter osisID="Gen.1">
      <title type="chapter">Глава 1</title>
      <verse osisID="Gen.1.1">Первые <w lemma="strong:H7225">слова</w> проверки.<note type="crossReference"><reference osisRef="John.1.1">Ин 1:1</reference></note></verse>
      <verse osisID="Gen.1.2 Gen.1.3">Два стиха вместе.</verse>
    </chapter>
  </div>
  <div type="book" osisID="John">
    <chapter sID="John.3" osisID="John.3"/>
    <p><verse sID="John.3.16" osisID="John.3.16"/>Он сказал: <q who="Jesus" marker="">слова учителя</q>,<lb/>и это конец.<verse eID="John.3.16"/></p>
    <title type="x-heading">Заголовок</title>
    <p><verse sID="John.3.17" osisID="John.3.17"/><transChange type="added">Стих</transChange> в двух <l>строках</l><l>стихов.</l><verse eID="John.3.17"/></p>
    <chapter eID="John.3"/>
  </div>
</osisText>
</osis>"#;
    let imported = read(osis, "test.osis.xml");
    assert_eq!(imported.format, Format::Osis);
    assert_eq!(
        (imported.about.name.as_str(), imported.about.short.as_str(), imported.about.language.as_str()),
        ("Тестовая Библия", "TOB", "ru")
    );
    assert_eq!(imported.about.terms, "Для проверки.");
    assert_eq!(imported.verse(at("GEN", 1, 1)), Some("Первые слова проверки."));
    assert_eq!(imported.verse(at("GEN", 1, 2)), Some("Два стиха вместе."));
    assert_eq!(imported.verse(at("JOH", 3, 16)), Some("Он сказал: слова учителя, и это конец."));
    assert_eq!(imported.verse(at("JOH", 3, 17)), Some("Стих в двух строках стихов."));
    assert_eq!(imported.verse_count(), 4);
    assert!(imported.complete());
    round_trip(&imported);
}

#[test]
fn a_bible_tagged_b_c_v_reads_books_by_their_names_in_any_language() {
    let tags = r#"<?xml version="1.0" encoding="UTF-8"?>
<bible>
  <b n="Genesis"><c n="1"><v n="1">Words for a test.</v><v n="2">More words.</v></c></b>
  <b n="Деяния апостолов"><c n="2"><v n="1">Слова для проверки.</v></c></b>
  <b n="Revelation of John"><c n="22"><v n="21">The last words.</v></c></b>
</bible>"#;
    let imported = read(tags, "My_Test_Bible.xml");
    assert_eq!(imported.format, Format::Tags);
    assert_eq!(imported.verse(at("GEN", 1, 2)), Some("More words."));
    assert_eq!(imported.verse(at("ACT", 2, 1)), Some("Слова для проверки."));
    assert_eq!(imported.verse(at("REV", 22, 21)), Some("The last words."));
}

#[test]
fn a_file_that_does_not_say_what_it_is_asks_with_the_file_name_offered() {
    let tags = r#"<bible><b n="Genesis"><c n="1"><v n="1">The first words of a test.</v><v n="2">And the second, of the test.</v></c></b></bible>"#;
    let imported = read(tags, "RST.xml");
    assert!(!imported.complete());
    assert_eq!(imported.missing_flags(&About::default()), "--name, --short and --language");
    let offered = imported.prefilled();
    assert_eq!((offered.name.as_str(), offered.short.as_str(), offered.language.as_str()), ("RST", "RST", "en"));
    // Nothing given: it can't be made yet; given, it can.
    assert_eq!(imported.details(&About::default()), None);
    let given = About {
        name: "Russian Test".to_owned(),
        short: "RT".to_owned(),
        language: "Russian".to_owned(),
        ..About::default()
    };
    let details = imported.details(&given).unwrap();
    assert_eq!((details.name.as_str(), details.language.as_str()), ("Russian Test", "ru"));
    // A Beblia file names itself but not its short name or language.
    let beblia = r#"<?xml version="1.0" encoding="UTF-8"?>
<bible translation="Test Translation" status="Written for a test">
  <testament name="New"><book number="43"><chapter number="3"><verse number="16">Words for a test.</verse></chapter></book></testament>
</bible>"#;
    let imported = read(beblia, "TestBible.xml");
    assert_eq!(imported.format, Format::Beblia);
    assert_eq!(
        (imported.about.name.as_str(), imported.about.terms.as_str()),
        ("Test Translation", "Written for a test")
    );
    assert_eq!(imported.missing_flags(&About::default()), "--short and --language");
    assert_eq!(imported.prefilled().short, "TT", "the first letters of its name");
    assert_eq!(imported.verse(at("JOH", 3, 16)), Some("Words for a test."));
}

const GENESIS_USFM: &str = "\\id GEN A test Bible\n\\h Genesis\n\\toc1 The Book of Beginnings\n\\mt1 Genesis\n\\c 1\n\
\\s1 A heading\n\\p\n\\v 1 The \\w first|strong=\"H7225\"\\w* words\\f + \\fr 1:1 \\ft A note.\\f* of a test.\n\
\\v 2 Words of the \\wj teacher\\wj* \\x - \\xo 1:2 \\xt Jn 1:1\\x*in red.\n\\q1\n\\v 3-4 Two verses \\va 4\\va* joined,\n\\q2 on two lines.\n\
\\v 5 \\qt-s |who=\"Someone\"\\*Said aloud.\\qt-e\\*\n";
const PSALMS_USFM: &str =
    "\\id PSA\n\\c 3\n\\d A song for a test.\n\\d Its second line.\n\\q1\n\\v 1 The first verse.\n\\v 2 The second.\n";

#[test]
fn usfm_books_in_a_zip_read_with_what_ebible_says_beside_them() {
    let copr = "<!DOCTYPE html>\n<html lang=\"en\">\n<head><title>A Test Translation  </title></head>\n<body><a href='https://creativecommons.org/licenses/by-sa/4.0/'>CC</a></body></html>";
    let bytes = zipped(&[
        ("02-GENtest.usfm", GENESIS_USFM.as_bytes()),
        ("20-PSAtest.usfm", PSALMS_USFM.as_bytes()),
        ("copr.htm", copr.as_bytes()),
        ("keys.asc", b"not read"),
    ]);
    let imported = read_bytes(&bytes, "test_usfm.zip", &Limits::NORMAL).unwrap();
    assert_eq!(imported.format, Format::Usfm);
    assert_eq!(imported.verse(at("GEN", 1, 1)), Some("The first words of a test."));
    assert_eq!(imported.verse(at("GEN", 1, 2)), Some("Words of the teacher in red."));
    assert_eq!(imported.verse(at("GEN", 1, 3)), Some("Two verses joined, on two lines."));
    assert_eq!(imported.verse(at("GEN", 1, 5)), Some("Said aloud."));
    assert_eq!(
        imported.verse(at("PSA", 3, 1)),
        Some("A song for a test. Its second line. The first verse."),
        "a psalm's title stays at the start of its first verse"
    );
    assert_eq!(
        imported.about,
        About {
            name: "A Test Translation".to_owned(),
            short: String::new(),
            language: "en".to_owned(),
            terms: "CC BY-SA 4.0".to_owned()
        }
    );
    assert_eq!(imported.prefilled().short, "ATT", "the first letters of its name");
    round_trip(&imported);
}

#[test]
fn a_folder_of_usx_books_reads_with_its_bundles_details() {
    let folder = std::env::temp_dir().join(format!("softpresenter-usx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("release/USX_1")).unwrap();
    let usx = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0">
  <book code="JHN" style="id">A description, not the text</book>
  <para style="h">John</para>
  <chapter number="3" style="c" sid="JHN 3"/>
  <para style="s1">A heading</para>
  <para style="p"><verse number="16" style="v" sid="JHN 3:16"/>Words of a <char style="wj">teacher</char><note caller="+" style="f"><char style="ft">A note.</char></note> for a test.<verse eid="JHN 3:16"/>
  <verse number="17" style="v" sid="JHN 3:17"/><char style="w" strong="G1">More</char> words.<verse eid="JHN 3:17"/></para>
  <chapter eid="JHN 3"/>
</usx>"#;
    std::fs::write(folder.join("release/USX_1/JHN.usx"), usx).unwrap();
    let metadata = r#"<?xml version="1.0" encoding="utf-8"?>
<DBLMetadata id="x" revision="1">
  <identification><name>Test Bible</name><nameLocal>Тестова Біблія</nameLocal><abbreviation>TB</abbreviation><abbreviationLocal>ТБ</abbreviationLocal></identification>
  <language><iso>ukr</iso></language>
  <copyright><fullStatement><statementContent type="xhtml"><p>Written for a test.</p></statementContent></fullStatement></copyright>
</DBLMetadata>"#;
    std::fs::write(folder.join("metadata.xml"), metadata).unwrap();
    std::fs::write(folder.join(".hidden.usx"), "not read").unwrap();
    assert!(looks_like_bible(&folder.join("release/USX_1")));
    let imported = read_path(&folder, &Limits::NORMAL).unwrap();
    assert_eq!(imported.format, Format::Usx);
    assert_eq!(imported.verse(at("JOH", 3, 16)), Some("Words of a teacher for a test."));
    assert_eq!(imported.verse(at("JOH", 3, 17)), Some("More words."));
    assert_eq!(
        (imported.about.name.as_str(), imported.about.short.as_str(), imported.about.language.as_str()),
        ("Тестова Біблія", "ТБ", "uk")
    );
    assert_eq!(imported.about.terms, "Written for a test.");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_zip_bomb_and_a_zip_of_too_many_files_are_refused() {
    let small = Limits { file: 1 << 20, entry: 1 << 20, unpacked: 1 << 20, entries: 10 };
    // Four megabytes of the same letter packs into a few kilobytes.
    let bomb = zipped(&[("bible.xml", &vec![b'a'; 4 << 20])]);
    assert!(bomb.len() < 64 * 1024);
    let refused = read_bytes(&bomb, "bomb.zip", &small).unwrap_err();
    assert!(refused.contains("unpacks to more than any Bible"), "{refused}");
    // Each entry small, but together too much.
    let half = vec![b'a'; 600 * 1024];
    let many = zipped(&[("a.xml", &half), ("b.xml", &half)]);
    assert!(read_bytes(&many, "many.zip", &small).unwrap_err().contains("unpacks to more"));
    let names: Vec<String> = (0..11).map(|n| format!("{n}.usfm")).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|name| (name.as_str(), b"\\id GEN".as_slice())).collect();
    assert!(read_bytes(&zipped(&files), "files.zip", &small).unwrap_err().contains("too many files"));
    // A file too large is never read whole.
    assert!(read_bytes(&vec![b' '; (1 << 20) + 1], "big.xml", &small).unwrap_err().contains("larger than any Bible"));
    // A zip inside the zip is not opened.
    let inner = zipped(&[("02-GENtest.usfm", GENESIS_USFM.as_bytes())]);
    let outer = zipped(&[("inner.zip", &inner)]);
    assert!(read_bytes(&outer, "outer.zip", &Limits::NORMAL).is_err());
}

#[test]
fn entities_a_file_declares_are_never_expanded() {
    let file = r#"<?xml version="1.0"?>
<!DOCTYPE XMLBIBLE [
  <!ENTITY word "EXPANDED">
  <!ENTITY outside SYSTEM "file:///etc/passwd">
  <!ENTITY a "&word;&word;&word;&word;&word;&word;&word;&word;">
  <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
]>
<XMLBIBLE biblename="&word; Bible">
  <BIBLEBOOK bnumber="1"><CHAPTER cnumber="1">
    <VERS vnumber="1">Before &word;&outside;&b; after &amp; &#1040;&#x0411; &lt;end&gt;.</VERS>
  </CHAPTER></BIBLEBOOK>
</XMLBIBLE>"#;
    let imported = read(file, "entities.xml");
    assert_eq!(imported.verse(at("GEN", 1, 1)), Some("Before after & АБ <end>."));
    assert!(!imported.about.name.contains("EXPANDED"), "{}", imported.about.name);
}

#[test]
fn deep_nesting_and_files_that_are_not_bibles_are_refused_in_plain_words() {
    let deep = format!(
        "<XMLBIBLE><BIBLEBOOK bnumber=\"1\"><CHAPTER cnumber=\"1\"><VERS vnumber=\"1\">{}x{}</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>",
        "<i>".repeat(5000),
        "</i>".repeat(5000)
    );
    let refused = read_bytes(deep.as_bytes(), "deep.xml", &Limits::NORMAL).unwrap_err();
    assert!(refused.contains("nested too deeply"), "{refused}");
    for (text, name) in [
        ("Words of a song\n\nThe next slide", "song.txt"),
        ("<song><title>A song</title></song>", "song.xml"),
        ("<XMLBIBLE><BIBLEBOOK bnumber=\"1\"><CHAPTER cnumber=\"1\"><VERS vnumber=\"1\">broken", "broken.xml"),
        ("<XMLBIBLE biblename=\"Empty\"></XMLBIBLE>", "empty.xml"),
    ] {
        assert!(read_bytes(text.as_bytes(), name, &Limits::NORMAL).is_err(), "{name}");
    }
}

#[test]
fn a_synodal_numbered_psalter_is_found_out_and_lines_up_with_english_psalms() {
    // The Synodal's chapter lengths for Psalms 1 to 23 (its psalm titles are verse 1).
    let lengths = [6, 12, 9, 9, 13, 11, 18, 10, 39, 7, 9, 6, 7, 5, 11, 15, 51, 15, 10, 14, 32, 6, 10];
    let mut usfm = String::from("\\id PSA\n");
    for (psalm, length) in lengths.iter().enumerate() {
        usfm.push_str(&format!("\\c {}\n\\q1\n", psalm + 1));
        for verse in 1..=*length {
            usfm.push_str(&format!("\\v {verse} Псалом {}, стих {verse}.\n", psalm + 1));
        }
    }
    let imported = read(&usfm, "psalms.usfm");
    assert_eq!(imported.fit.numbering, Numbering::Synodal);
    assert!(imported.fit.by_psalms && !imported.fit.loose);
    assert!(imported.summary("ПС").contains("numbered as the Russian Synodal Bible is, judged by its Psalms"));
    let bible = round_trip(&imported);
    assert_eq!(bible.info.numbering, Numbering::Synodal);
    // The English Psalm 23 is the Synodal's 22nd; the English 10:1 is the Synodal 9:22.
    let english = at("PSA", 23, 1);
    assert_eq!(bible.verse(convert(english, Numbering::English, bible.info.numbering)), Some("Псалом 22, стих 1."));
    let english = at("PSA", 10, 1);
    assert_eq!(bible.verse(convert(english, Numbering::English, bible.info.numbering)), Some("Псалом 9, стих 22."));
    // The same psalms numbered as English Bibles number them are taken for English.
    let english_lengths = [6, 12, 8, 8, 12, 10, 17, 9, 20, 18, 7, 8, 6, 7, 5, 11, 15, 50, 14, 9, 13, 31, 6];
    let mut usfm = String::from("\\id PSA\n");
    for (psalm, length) in english_lengths.iter().enumerate() {
        usfm.push_str(&format!("\\c {}\n", psalm + 1));
        for verse in 1..=*length {
            usfm.push_str(&format!("\\v {verse} Psalm {}, verse {verse}.\n", psalm + 1));
        }
    }
    let imported = read(&usfm, "psalms.usfm");
    assert_eq!((imported.fit.numbering, imported.fit.loose), (Numbering::English, false));
}

#[test]
fn what_a_file_says_cannot_break_the_bible_file_made_from_it() {
    let file = "<XMLBIBLE biblename=\"Name&#10;id: kjv&#10;&#10;GEN 1:1 Planted\"><INFORMATION><identifier>T&#10;B</identifier><language>en</language></INFORMATION>\
<BIBLEBOOK bnumber=\"1\"><CHAPTER cnumber=\"1\"><VERS vnumber=\"1\">Words&#10;GEN 1:2 planted&#13;too</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>";
    let imported = read(file, "x.xml");
    let bible = Bible::from_bytes(&imported.bible("mine", &imported.prefilled()).to_bytes()).unwrap();
    assert_eq!(bible.info.id, "mine");
    assert_eq!(bible.info.name, "Name id: kjv GEN 1:1 Planted");
    assert_eq!(bible.info.short, "T B");
    assert_eq!(bible.verse_count(), 1);
    assert_eq!(bible.verse(at("GEN", 1, 1)), Some("Words GEN 1:2 planted too"));
}

#[test]
fn files_are_told_apart_by_what_they_hold() {
    let folder = std::env::temp_dir().join(format!("softpresenter-sniff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("books")).unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = folder.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    // The content decides, whatever the ending.
    assert!(looks_like_bible(&write("bible", ZEFANIA.as_bytes())));
    assert!(looks_like_bible(&write("bible.txt", GENESIS_USFM.as_bytes())));
    assert!(looks_like_bible(&write("bible.zip", &zipped(&[("02-GENtest.usfm", GENESIS_USFM.as_bytes())]))));
    assert!(looks_like_bible(&write("named.zip", &zipped(&[("bible", ZEFANIA.as_bytes())]))));
    let tags = br#"<bible><b n="John"><c n="3"><v n="16">Words for a test.</v></c></b></bible>"#;
    assert!(looks_like_bible(&write("bible.xmm", tags)));
    assert!(looks_like_bible(&write("odd.zip", &zipped(&[("Bible.module", tags.as_slice()), ("a.jpg", b"picture")]))));
    std::fs::write(folder.join("books/GEN.SFM"), GENESIS_USFM).unwrap();
    assert!(looks_like_bible(&folder.join("books")));
    assert!(!looks_like_bible(&write("song.txt", b"Words of a song\n\nThe next slide")));
    assert!(!looks_like_bible(&write("song.xml", b"<song><title>A song</title><lyrics/></song>")));
    assert!(!looks_like_bible(&write("other.zip", &zipped(&[("a.jpg", b"picture")]))));
    assert!(!looks_like_bible(&folder.join("missing.xml")));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn short_names_for_files_and_languages_are_made_plain() {
    assert_eq!(id_from("KJV", "King James"), "kjv");
    assert_eq!(id_from("НПУ", ""), "npu");
    assert_eq!(id_from("", "Синодальный перевод"), "sinodalnyy-perevod");
    assert_eq!(id_from("../../etc", ""), "etc");
    assert_eq!(id_from("ΒΓ", "Η Καινή"), "bible");
    assert_eq!(language_code("RUS"), "ru");
    assert_eq!(language_code("en-GB"), "en");
    assert_eq!(language_code(" Ukrainian "), "uk");
    assert_eq!(language_code("ron"), "ron");
    assert_eq!(grouped(31102), "31,102");
    assert_eq!(grouped(999), "999");
}

#[test]
fn books_are_found_by_the_codes_and_names_files_use() {
    let code = |found: Option<usize>| found.map(|book| BOOKS[book].code);
    assert_eq!(code(crate::book_by_standard_code("JHN")), Some("JOH"));
    assert_eq!(code(crate::book_by_standard_code("sng")), Some("SOL"));
    assert_eq!(code(crate::book_by_standard_code("TOB")), None);
    assert_eq!(code(crate::book_by_osis("Ps")), Some("PSA"));
    assert_eq!(code(crate::book_by_osis("1John")), Some("1JO"));
    assert_eq!(code(crate::book_by_osis("Tob")), None);
    assert_eq!(code(crate::book_by_number(66)), Some("REV"));
    assert_eq!(code(crate::book_by_number(67)), None);
    for (name, book) in [
        ("Revelation of John", Some("REV")),
        ("II Kings", Some("2KI")),
        ("1 John", Some("1JO")),
        ("Song of Songs", Some("SOL")),
        ("Песнь песней", Some("SOL")),
        ("От Иоанна", Some("JOH")),
        ("Від Івана", Some("JOH")),
        ("Judith", None),
        ("Jude", Some("JUD")),
    ] {
        assert_eq!(code(crate::book_by_written_name(name)), book, "{name}");
    }
}

// Files made to keep the reader busy: each must be read (or refused) in a moment.

/// Fails if `work` takes longer than `most`; a whole Bible takes far less even in a slow build.
fn quickly<T>(most: std::time::Duration, what: &str, work: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let done = work();
    assert!(started.elapsed() < most, "{what} took {:?}", started.elapsed());
    done
}

#[test]
fn a_huge_book_name_is_looked_up_in_a_moment() {
    let name = "q.".repeat(500_000);
    let found = quickly(std::time::Duration::from_millis(100), "a 1 MB name", || crate::book_by_written_name(&name));
    assert_eq!(found, None);
    // As the `<b n>` of a Bible tagged b/c/v, zipped small.
    let file = format!(
        r#"<bible><b n="{name}"><c n="1"><v n="1">Words.</v></c></b><b n="John"><c n="3"><v n="16">More words.</v></c></b></bible>"#
    );
    let zip = zipped(&[("bible.xml", file.as_bytes())]);
    let imported = quickly(std::time::Duration::from_millis(500), "a Bible with a 1 MB book name", || {
        read_bytes(&zip, "names.zip", &Limits::NORMAL).unwrap()
    });
    assert_eq!(imported.verse(at("JOH", 3, 16)), Some("More words."));
    assert_eq!(imported.left_out.len(), 1);
    assert!(imported.left_out[0].chars().count() <= 60);
    // A long name of a real book still finds it by its first words.
    assert_eq!(crate::book_by_written_name(&format!("Revelation {}", "of John ".repeat(10_000))), book_by_code("REV"));
}

#[test]
fn the_same_verse_begun_over_and_over_costs_no_more_each_time() {
    let usfm = format!("\\id PSA\n{}", "\\c 1\n\\d A title.\n\\v 1 Words.\n".repeat(100_000));
    let imported = quickly(std::time::Duration::from_millis(100), "100,000 repeats in USFM", || read(&usfm, "x.usfm"));
    let verse = imported.verse(at("PSA", 1, 1)).unwrap();
    assert!(verse.starts_with("A title.") && verse.chars().count() <= LONGEST_VERSE, "{}", verse.len());
    let usx = format!(
        "<usx version=\"3.0\"><book code=\"PSA\" style=\"id\"/>{}</usx>",
        "<chapter number=\"1\" style=\"c\"/><para style=\"d\">A title.</para><para style=\"q1\"><verse number=\"1\" style=\"v\"/>Words.</para>"
            .repeat(100_000)
    );
    // Thirteen megabytes of XML: the time is reading them, once.
    let imported = quickly(std::time::Duration::from_millis(1000), "100,000 repeats in USX", || read(&usx, "x.usx"));
    assert!(imported.verse(at("PSA", 1, 1)).unwrap().chars().count() <= LONGEST_VERSE);
    // A title that never ends, and one verse's words in one enormous piece, stay within bounds.
    let long = format!("\\id PSA\n\\c 2\n\\d {}\n\\v 1 {}\n", "title ".repeat(200_000), "word ".repeat(200_000));
    let imported = quickly(std::time::Duration::from_millis(1500), "a 2 MB title and verse", || read(&long, "x.usfm"));
    assert!(imported.verse(at("PSA", 2, 1)).unwrap().chars().count() <= LONGEST_VERSE);
}

/// A zip whose second entry is the first one again under another name: their packed bytes
/// overlap.
fn overlapping_zip() -> Vec<u8> {
    let mut bytes = zipped(&[("a.usfm", GENESIS_USFM.as_bytes()), ("b.usfm", GENESIS_USFM.as_bytes())]);
    let u16_at = |bytes: &[u8], at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
    let u32_at = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let end = bytes.len() - 22;
    assert_eq!(&bytes[end..end + 4], b"PK\x05\x06");
    let first = u32_at(&bytes, end + 16);
    let second = first + 46 + u16_at(&bytes, first + 28) + u16_at(&bytes, first + 30) + u16_at(&bytes, first + 32);
    assert_eq!(&bytes[second..second + 4], b"PK\x01\x02");
    let offset = bytes[first + 42..first + 46].to_vec();
    bytes[second + 42..second + 46].copy_from_slice(&offset);
    bytes
}

#[test]
fn a_zip_whose_files_overlap_is_refused() {
    let refused = read_bytes(&overlapping_zip(), "overlap.zip", &Limits::NORMAL).unwrap_err();
    assert!(refused.contains("overlap"), "{refused}");
    // The same two files, honestly packed, are read.
    let honest = zipped(&[("a.usfm", GENESIS_USFM.as_bytes()), ("b.usfm", GENESIS_USFM.as_bytes())]);
    assert!(read_bytes(&honest, "honest.zip", &Limits::NORMAL).is_ok());
}

#[test]
fn text_that_grows_when_decoded_counts_against_what_may_be_unpacked() {
    let small = Limits { file: 1 << 20, entry: 1 << 20, unpacked: 1 << 20, entries: 10 };
    // 600 KB of a byte that is "€" (three bytes of UTF-8) in the old Western encoding.
    let refused = read_bytes(&zipped(&[("bible.xml", &vec![0x80; 600 * 1024])]), "euro.zip", &small).unwrap_err();
    assert!(refused.contains("unpacks to more than any Bible"), "{refused}");
    assert!(read_bytes(&vec![0x80; 600 * 1024], "euro.xml", &small).unwrap_err().contains("unpacks to more"));
}

#[test]
fn a_folder_of_too_many_entries_is_refused_whatever_they_are() {
    let small = Limits { file: 1 << 20, entry: 1 << 20, unpacked: 1 << 20, entries: 10 };
    let folder = std::env::temp_dir().join(format!("softpresenter-entries-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    for n in 0..20 {
        std::fs::write(folder.join(format!("{n}.jpg")), b"picture").unwrap();
    }
    std::fs::write(folder.join("GEN.usfm"), GENESIS_USFM).unwrap();
    let refused = read_path(&folder, &small).unwrap_err();
    assert!(refused.contains("too many files"), "{refused}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn only_the_first_attributes_of_an_element_are_taken() {
    let many: String = (0..40).map(|n| format!(" a{n}=\"x\"")).collect();
    let file = format!(
        "<XMLBIBLE biblename=\"T\"><BIBLEBOOK bnumber=\"1\"><CHAPTER cnumber=\"1\"><VERS vnumber=\"1\">Kept.</VERS><VERS{many} vnumber=\"2\">Not kept.</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>"
    );
    let imported = read(&file, "attributes.xml");
    assert_eq!(imported.verse(at("GEN", 1, 1)), Some("Kept."));
    assert_eq!(imported.verse(at("GEN", 1, 2)), None, "vnumber comes after the first 32 attributes");
}

#[cfg(unix)]
#[test]
fn a_pipe_is_never_opened() {
    let folder = std::env::temp_dir().join(format!("softpresenter-pipe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let pipe = folder.join("bible.xml");
    assert!(std::process::Command::new("mkfifo").arg(&pipe).status().unwrap().success());
    // Opening a pipe nothing writes to would wait for ever: the answer must come at once.
    let (tx, rx) = std::sync::mpsc::channel();
    let path = pipe.clone();
    std::thread::spawn(move || {
        let _ = tx.send((looks_like_bible(&path), read_path(&path, &Limits::NORMAL).map(|_| ())));
    });
    let (looks, read) = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("reading a pipe waited");
    assert!(!looks);
    assert!(read.unwrap_err().contains("isn't a file or folder"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_file_name_is_said_on_one_line_and_never_names_a_device() {
    let imported = read(ZEFANIA, "two\nlines\u{7}.xml");
    assert_eq!(imported.file_name, "two lines .xml");
    let refused = read_bytes(b"not a bible", "a\nname.txt", &Limits::NORMAL).unwrap_err();
    assert!(!refused.contains('\n'), "{refused}");
    assert_eq!(id_from("CON", ""), "con-bible");
    assert_eq!(id_from("Nul", ""), "nul-bible");
    assert_eq!(id_from("COM1", ""), "com1-bible");
    assert_eq!(id_from("com", ""), "com");
    assert_eq!(id_from("COM0", ""), "com0");
    assert!(is_device_name("LPT9") && is_device_name("aux.spbible") && !is_device_name("kjv"));
}
