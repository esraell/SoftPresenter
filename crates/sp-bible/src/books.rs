//! The sixty-six books: their codes in Bible files, and the names people type for them.

/// One book of the Bible.
pub struct Book {
    /// The code used in Bible files, such as `JOH`.
    pub code: &'static str,
    pub english: &'static str,
    /// Other things people type for it in English, without spaces or full stops, lower case.
    pub english_short: &'static [&'static str],
    pub russian: &'static str,
    pub russian_short: &'static [&'static str],
    /// The Ukrainian name, short as it is written in a reference, and the usual abbreviations
    /// (from the list of the books of the Bible on Ukrainian Wikipedia, in the Ohienko
    /// tradition).
    pub ukrainian: &'static str,
    pub ukrainian_short: &'static [&'static str],
}

macro_rules! book {
    ($code:literal, $en:literal, [$($ens:literal),*], $ru:literal, [$($rus:literal),*], $uk:literal, [$($uks:literal),*]) => {
        Book {
            code: $code,
            english: $en,
            english_short: &[$($ens),*],
            russian: $ru,
            russian_short: &[$($rus),*],
            ukrainian: $uk,
            ukrainian_short: &[$($uks),*],
        }
    };
}

/// Every book, in the usual order.
pub const BOOKS: [Book; 66] = [
    book!("GEN", "Genesis", ["gen", "gn"], "Бытие", ["быт"], "Буття", ["1м", "бут"]),
    book!("EXO", "Exodus", ["exo", "ex", "exod"], "Исход", ["исх"], "Вихід", ["2м", "вих"]),
    book!("LEV", "Leviticus", ["lev", "lv"], "Левит", ["лев"], "Левит", ["3м", "лев"]),
    book!("NUM", "Numbers", ["num", "nm"], "Числа", ["чис"], "Числа", ["4м", "чис"]),
    book!("DEU", "Deuteronomy", ["deu", "deut", "dt"], "Второзаконие", ["втор"], "Повторення Закону", ["5м", "повт"]),
    book!("JOS", "Joshua", ["jos", "josh"], "Иисус Навин", ["нав", "навин"], "Ісуса Навина", ["єг", "нав"]),
    book!("JDG", "Judges", ["jdg", "judg"], "Судьи", ["суд"], "Суддів", ["суд"]),
    book!("RUT", "Ruth", ["rut", "ru"], "Руфь", ["руф"], "Рут", ["рут"]),
    book!("1SA", "1 Samuel", ["1sa", "1sam", "1sm"], "1 Царств", ["1цар"], "1 Самуїлова", ["1сам"]),
    book!("2SA", "2 Samuel", ["2sa", "2sam", "2sm"], "2 Царств", ["2цар"], "2 Самуїлова", ["2сам"]),
    book!("1KI", "1 Kings", ["1ki", "1kgs", "1kin"], "3 Царств", ["3цар"], "1 Царів", ["1цар"]),
    book!("2KI", "2 Kings", ["2ki", "2kgs", "2kin"], "4 Царств", ["4цар"], "2 Царів", ["2цар"]),
    book!("1CH", "1 Chronicles", ["1ch", "1chr", "1chron"], "1 Паралипоменон", ["1пар"], "1 Хронік", ["1хр"]),
    book!("2CH", "2 Chronicles", ["2ch", "2chr", "2chron"], "2 Паралипоменон", ["2пар"], "2 Хронік", ["2хр"]),
    book!("EZR", "Ezra", ["ezr"], "Ездра", ["езд"], "Ездри", ["езд"]),
    book!("NEH", "Nehemiah", ["neh"], "Неемия", ["неем"], "Неемії", ["неем"]),
    book!("EST", "Esther", ["est", "esth"], "Есфирь", ["есф"], "Естер", ["ест"]),
    book!("JOB", "Job", ["jb"], "Иов", [], "Йова", ["йов"]),
    book!(
        "PSA",
        "Psalms",
        ["psa", "ps", "psalm", "pss"],
        "Псалтирь",
        ["пс", "псалом", "псалмы"],
        "Псалом",
        ["пс", "псалми"]
    ),
    book!("PRO", "Proverbs", ["pro", "prov", "pr"], "Притчи", ["прит", "притч"], "Приповісті", ["пр", "прип"]),
    book!("ECC", "Ecclesiastes", ["ecc", "eccl", "eccles"], "Екклесиаст", ["еккл", "екк"], "Екклезіяст", ["екл"]),
    book!(
        "SOL",
        "Song of Solomon",
        ["sol", "song", "songofsongs", "sos", "sng"],
        "Песня Песней",
        ["песн", "песнь"],
        "Пісня над піснями",
        ["пісн"]
    ),
    book!("ISA", "Isaiah", ["isa", "is"], "Исаия", ["ис"], "Ісаї", ["іс"]),
    book!("JER", "Jeremiah", ["jer"], "Иеремия", ["иер"], "Єремії", ["єр"]),
    book!("LAM", "Lamentations", ["lam"], "Плач Иеремии", ["плач"], "Плач Єремії", ["плач"]),
    book!("EZE", "Ezekiel", ["eze", "ezek", "ezk"], "Иезекииль", ["иез"], "Єзекіїля", ["єз"]),
    book!("DAN", "Daniel", ["dan", "dn"], "Даниил", ["дан"], "Даниїла", ["дан"]),
    book!("HOS", "Hosea", ["hos"], "Осия", ["ос"], "Осії", ["ос"]),
    book!("JOE", "Joel", ["joe", "jl"], "Иоиль", ["иоил"], "Йоіла", ["йоіл"]),
    book!("AMO", "Amos", ["amo", "am"], "Амос", ["ам"], "Амоса", ["ам"]),
    book!("OBA", "Obadiah", ["oba", "obad", "ob"], "Авдий", ["авд"], "Овдія", ["овд"]),
    book!("JON", "Jonah", ["jon", "jnh"], "Иона", ["ион"], "Йони", ["йона"]),
    book!("MIC", "Micah", ["mic"], "Михей", ["мих"], "Михея", ["мих"]),
    book!("NAH", "Nahum", ["nah", "na"], "Наум", [], "Наума", ["наум"]),
    book!("HAB", "Habakkuk", ["hab"], "Аввакум", ["авв"], "Авакума", ["ав"]),
    book!("ZEP", "Zephaniah", ["zep", "zeph"], "Софония", ["соф"], "Софонії", ["соф"]),
    book!("HAG", "Haggai", ["hag"], "Аггей", ["агг"], "Огія", ["ог"]),
    book!("ZEC", "Zechariah", ["zec", "zech"], "Захария", ["зах"], "Захарія", ["зах"]),
    book!("MAL", "Malachi", ["mal"], "Малахия", ["мал"], "Малахії", ["мал"]),
    book!(
        "MAT",
        "Matthew",
        ["mat", "matt", "mt"],
        "От Матфея",
        ["мф", "матфея", "мат"],
        "Від Матвія",
        ["мт", "матвія"]
    ),
    book!("MAR", "Mark", ["mar", "mk", "mrk"], "От Марка", ["мк", "марка", "мар"], "Від Марка", ["мр", "мк", "марка"]),
    book!("LUK", "Luke", ["luk", "lk"], "От Луки", ["лк", "луки", "лук"], "Від Луки", ["лк", "луки"]),
    book!("JOH", "John", ["joh", "jn", "jhn"], "От Иоанна", ["ин", "иоанна", "иоан"], "Від Івана", ["ів", "івана"]),
    book!("ACT", "Acts", ["act", "ac"], "Деяния", ["деян"], "Дії", ["дії"]),
    book!("ROM", "Romans", ["rom", "ro", "rm"], "К Римлянам", ["рим", "римлянам"], "До римлян", ["рим"]),
    book!("1CO", "1 Corinthians", ["1co", "1cor"], "1 Коринфянам", ["1кор"], "1 до коринтян", ["1кор"]),
    book!("2CO", "2 Corinthians", ["2co", "2cor"], "2 Коринфянам", ["2кор"], "2 до коринтян", ["2кор"]),
    book!("GAL", "Galatians", ["gal"], "Галатам", ["гал"], "До галатів", ["гал"]),
    book!("EPH", "Ephesians", ["eph"], "Ефесянам", ["еф"], "До ефесян", ["еф"]),
    book!("PHI", "Philippians", ["phi", "phil", "php"], "Филиппийцам", ["флп", "фил"], "До филип\u{2019}ян", ["фил"]),
    book!("COL", "Colossians", ["col"], "Колоссянам", ["кол"], "До колосян", ["кол"]),
    book!(
        "1TH",
        "1 Thessalonians",
        ["1th", "1thess", "1thes"],
        "1 Фессалоникийцам",
        ["1фес"],
        "1 до солунян",
        ["1сол"]
    ),
    book!(
        "2TH",
        "2 Thessalonians",
        ["2th", "2thess", "2thes"],
        "2 Фессалоникийцам",
        ["2фес"],
        "2 до солунян",
        ["2сол"]
    ),
    book!("1TI", "1 Timothy", ["1ti", "1tim"], "1 Тимофею", ["1тим"], "1 до Тимофія", ["1тим"]),
    book!("2TI", "2 Timothy", ["2ti", "2tim"], "2 Тимофею", ["2тим"], "2 до Тимофія", ["2тим"]),
    book!("TIT", "Titus", ["tit"], "Титу", ["тит"], "До Тита", ["тит"]),
    book!("PHM", "Philemon", ["phm", "philem"], "Филимону", ["флм"], "До Филимона", ["филим"]),
    book!("HEB", "Hebrews", ["heb"], "Евреям", ["евр"], "До євреїв", ["євр"]),
    book!("JAM", "James", ["jam", "jas"], "Иакова", ["иак"], "Якова", ["як"]),
    book!("1PE", "1 Peter", ["1pe", "1pet", "1pt"], "1 Петра", ["1пет", "1петр"], "1 Петра", ["1петр"]),
    book!("2PE", "2 Peter", ["2pe", "2pet", "2pt"], "2 Петра", ["2пет", "2петр"], "2 Петра", ["2петр"]),
    book!("1JO", "1 John", ["1jo", "1jn", "1jhn"], "1 Иоанна", ["1ин", "1иоан"], "1 Івана", ["1ів"]),
    book!("2JO", "2 John", ["2jo", "2jn", "2jhn"], "2 Иоанна", ["2ин", "2иоан"], "2 Івана", ["2ів"]),
    book!("3JO", "3 John", ["3jo", "3jn", "3jhn"], "3 Иоанна", ["3ин", "3иоан"], "3 Івана", ["3ів"]),
    book!("JUD", "Jude", ["jud", "jd"], "Иуды", ["иуд"], "Юди", ["юд"]),
    book!(
        "REV",
        "Revelation",
        ["rev", "rv", "revelations"],
        "Откровение",
        ["откр", "отк"],
        "Об\u{2019}явлення",
        ["об", "обявлення"]
    ),
];

/// What someone typed for a book, made comparable: lower case, no spaces or full stops.
/// What was typed, or a name, for matching: no spaces, full stops or apostrophes (typed as
/// ', ’ or ʼ), in lower case.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '.' | '\'' | '\u{2019}' | '\u{2bc}'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether a book's name starts with what was typed (already plain), also without the word
/// that joins it, at its start or after its number: "Иоанна" finds "От Иоанна", "Івана" finds
/// "Від Івана" and "1 корин" finds "1 до коринтян".
fn name_starts(name: &str, joins: &[&str], typed: &str) -> bool {
    let name = plain(name);
    if name.starts_with(typed) {
        return true;
    }
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    let rest = &name[digits.len()..];
    joins.iter().any(|join| rest.strip_prefix(join).is_some_and(|after| format!("{digits}{after}").starts_with(typed)))
}

/// Books whose names start with what was typed, best first, each with its name in the
/// language it matched in: an exact name or short form, then names in `language`, then names
/// in the other languages, each in Bible order. For the suggestions under Scripture Lookup:
/// "jo" gives John, Joshua, Job, Joel, Jonah; "Ів" gives Від Івана.
pub fn suggest_books(typed: &str, language: &str, limit: usize) -> Vec<(usize, &'static str)> {
    let typed = plain(typed);
    if typed.is_empty() {
        return Vec::new();
    }
    let names =
        |book: &'static Book, language: &str| -> (&'static str, &'static [&'static str], &'static [&'static str]) {
            match language {
                "ru" => (book.russian, book.russian_short, &["от", "к"]),
                "uk" => (book.ukrainian, book.ukrainian_short, &["від", "до"]),
                _ => (book.english, book.english_short, &[]),
            }
        };
    let mut order: Vec<&str> = vec![language];
    order.extend(["en", "ru", "uk"].into_iter().filter(|other| *other != language));
    let mut found: Vec<(usize, &'static str)> = Vec::new();
    let mut add = |book: usize, name: &'static str| {
        if !found.iter().any(|(have, _)| *have == book) {
            found.push((book, name));
        }
    };
    // Exact names and short forms first ("jn" is John before anything starting with "jn").
    for language in &order {
        for (at, book) in BOOKS.iter().enumerate() {
            let (name, shorts, _) = names(book, language);
            if plain(name) == typed || shorts.contains(&typed.as_str()) {
                add(at, name);
            }
        }
    }
    for language in &order {
        for (at, book) in BOOKS.iter().enumerate() {
            let (name, _, joins) = names(book, language);
            if name_starts(name, joins, &typed) {
                add(at, name);
            }
        }
    }
    found.truncate(limit);
    found
}

/// The book someone means by what they typed, as its place in [`BOOKS`]. An exact name or
/// short form wins; otherwise the first book whose name starts with what was typed.
pub fn find_book(typed: &str) -> Option<usize> {
    find_book_in(typed, "")
}

/// The same, reading what was typed in a language first (`en`, `ru` or `uk`): the same short
/// form can name different books ("1 Цар" is 1 Samuel in Russian but 1 Kings in Ukrainian).
pub fn find_book_in(typed: &str, language: &str) -> Option<usize> {
    find_book_typed(typed, language).map(|(book, _)| book)
}

/// The languages a book's names are written in, in the order they are tried when none is
/// preferred.
const LANGUAGES: [&str; 3] = ["en", "ru", "uk"];

/// [`find_book_in`], also saying which languages what was typed names the book in (`en`, `ru`,
/// `uk`, in that order). More than one when a name or short form is the same in two: "Пс",
/// "Лев" and "1 Кор" are Russian and Ukrainian alike. An exact name or short form decides the
/// languages when there is one; otherwise every language whose name starts with what was typed.
pub fn find_book_typed(typed: &str, language: &str) -> Option<(usize, Vec<&'static str>)> {
    let typed = plain(typed);
    if typed.is_empty() {
        return None;
    }
    let exact_in = |book: &Book, language: &str| match language {
        "en" => plain(book.english) == typed || book.english_short.contains(&typed.as_str()),
        "ru" => plain(book.russian) == typed || book.russian_short.contains(&typed.as_str()),
        "uk" => plain(book.ukrainian) == typed || book.ukrainian_short.contains(&typed.as_str()),
        _ => false,
    };
    let starts_in = |book: &Book, language: &str| match language {
        "en" => name_starts(book.english, &[], &typed),
        "ru" => name_starts(book.russian, &["от", "к"], &typed),
        "uk" => name_starts(book.ukrainian, &["від", "до"], &typed),
        _ => false,
    };
    let exact_languages = |book: &Book| -> Vec<&'static str> {
        let mut found: Vec<&'static str> = LANGUAGES.into_iter().filter(|language| exact_in(book, language)).collect();
        // A book's code is written in Latin letters, as English is.
        if found.is_empty() && book.code.eq_ignore_ascii_case(&typed) {
            found.push("en");
        }
        found
    };
    if let Some(found) = BOOKS.iter().position(|book| exact_in(book, language)) {
        return Some((found, exact_languages(&BOOKS[found])));
    }
    let exact = BOOKS.iter().position(|book| {
        book.code.eq_ignore_ascii_case(&typed) || LANGUAGES.iter().any(|language| exact_in(book, language))
    });
    if let Some(found) = exact {
        return Some((found, exact_languages(&BOOKS[found])));
    }
    let found = BOOKS.iter().position(|book| LANGUAGES.iter().any(|language| starts_in(book, language)))?;
    Some((found, LANGUAGES.into_iter().filter(|language| starts_in(&BOOKS[found], language)).collect()))
}

/// A book's name as a language writes it (`ru`, `uk`; English for any other).
pub fn book_name(book: usize, language: &str) -> &'static str {
    let names = &BOOKS[book.min(BOOKS.len() - 1)];
    match language {
        "ru" => names.russian,
        "uk" => names.ukrainian,
        _ => names.english,
    }
}

/// A book's place in [`BOOKS`] from its code in a Bible file.
pub fn book_by_code(code: &str) -> Option<usize> {
    BOOKS.iter().position(|book| book.code == code)
}

#[cfg(test)]
mod tests {

    #[test]
    fn an_apostrophe_typed_any_way_finds_the_book() {
        let revelation = BOOKS.iter().position(|book| book.code == "REV");
        let philippians = BOOKS.iter().position(|book| book.code == "PHI");
        for typed in ["Об'явлення", "Об\u{2019}явлення", "Об\u{2bc}явлення", "Обявлення"]
        {
            assert_eq!(find_book_in(typed, "uk"), revelation, "{typed}");
        }
        assert_eq!(find_book_in("До филип'ян", "uk"), philippians);
        assert_eq!(BOOKS[revelation.unwrap()].ukrainian, "Об\u{2019}явлення", "shown with a typographic apostrophe");
    }

    #[test]
    fn suggestions_rank_exact_forms_then_the_bibles_language_and_name_the_book_as_matched() {
        let names = |typed: &str, language: &str| -> Vec<&str> {
            suggest_books(typed, language, 6).into_iter().map(|(_, name)| name).collect()
        };
        assert_eq!(names("jo", "en"), ["Joshua", "Job", "Joel", "Jonah", "John"]);
        assert_eq!(names("jn", "en")[0], "John", "a short form comes first");
        assert_eq!(names("Ів", "uk")[0], "Від Івана");
        assert_eq!(names("1 ко", "uk"), ["1 до коринтян"]);
        assert_eq!(names("Ин", "ru")[0], "От Иоанна");
        // A Russian Bible reads "1 Цар" as 1 Samuel; a Ukrainian one as 1 Kings.
        let first = |typed: &str, language: &str| BOOKS[suggest_books(typed, language, 1)[0].0].code;
        assert_eq!(first("1 Цар", "ru"), "1SA");
        assert_eq!(first("1 Цар", "uk"), "1KI");
        assert!(suggest_books("", "en", 6).is_empty());
        assert!(suggest_books("zzz", "en", 6).is_empty());
    }

    use super::*;

    #[test]
    fn every_english_and_russian_short_form_still_finds_its_book() {
        for (at, book) in BOOKS.iter().enumerate() {
            for short in book.english_short.iter().chain(book.russian_short) {
                assert_eq!(find_book(short), Some(at), "{short} should find {}", book.english);
            }
            assert_eq!(find_book(book.english), Some(at));
            assert_eq!(find_book(book.russian), Some(at));
        }
    }

    #[test]
    fn ukrainian_names_and_short_forms_find_their_books() {
        let john = BOOKS.iter().position(|b| b.code == "JOH");
        assert_eq!(find_book_in("Ів", "uk"), john);
        assert_eq!(find_book("Від Івана"), john);
        assert_eq!(find_book("Івана"), john, "found without the \"від\" too");
        assert_eq!(find_book_in("Мт", "uk"), BOOKS.iter().position(|b| b.code == "MAT"));
        assert_eq!(find_book_in("1 Кор", "uk"), BOOKS.iter().position(|b| b.code == "1CO"));
        for (at, book) in BOOKS.iter().enumerate() {
            assert_eq!(find_book_in(book.ukrainian, "uk"), Some(at), "{}", book.ukrainian);
            for short in book.ukrainian_short {
                assert_eq!(find_book_in(short, "uk"), Some(at), "{short} should find {}", book.ukrainian);
            }
        }
    }

    #[test]
    fn the_languages_a_book_was_typed_in_are_said() {
        let typed = |text: &str, language: &str| {
            find_book_typed(text, language).map(|(book, languages)| (BOOKS[book].code, languages))
        };
        assert_eq!(typed("gen", ""), Some(("GEN", vec!["en"])));
        assert_eq!(typed("Genesis", "ru"), Some(("GEN", vec!["en"])), "a preference never changes the language");
        assert_eq!(typed("Быт", "en"), Some(("GEN", vec!["ru"])));
        assert_eq!(typed("Бут", "ru"), Some(("GEN", vec!["uk"])));
        assert_eq!(typed("Ин", ""), Some(("JOH", vec!["ru"])));
        assert_eq!(typed("Ів", "ru"), Some(("JOH", vec!["uk"])));
        // Spelt alike in Russian and Ukrainian: both.
        assert_eq!(typed("Пс", "ru"), Some(("PSA", vec!["ru", "uk"])));
        assert_eq!(typed("1 Кор", ""), Some(("1CO", vec!["ru", "uk"])));
        assert_eq!(typed("Левит", "uk"), Some(("LEV", vec!["ru", "uk"])));
        // "1 Цар" is a different book in each; the one read is the one said.
        assert_eq!(typed("1 Цар", "ru"), Some(("1SA", vec!["ru"])));
        assert_eq!(typed("1 Цар", "uk"), Some(("1KI", vec!["uk"])));
        // The start of a name, and a book's code.
        assert_eq!(typed("Gene", ""), Some(("GEN", vec!["en"])));
        assert_eq!(typed("Быти", ""), Some(("GEN", vec!["ru"])));
        assert_eq!(typed("JHN", ""), Some(("JOH", vec!["en"])));
        assert_eq!(typed("3:16", ""), None);
        assert_eq!(typed("", "en"), None);
    }

    #[test]
    fn every_full_name_finds_its_book_whichever_language_is_preferred() {
        // The Bible page writes a passage's name back in the operator's language and reads it
        // again with the first Bible's language preferred: it must come back the same book.
        for (at, book) in BOOKS.iter().enumerate() {
            for name in [book.english, book.russian, book.ukrainian] {
                for preferred in ["", "en", "ru", "uk"] {
                    assert_eq!(find_book_in(name, preferred), Some(at), "{name} with {preferred:?} preferred");
                }
            }
            for language in LANGUAGES {
                assert_eq!(find_book_in(book_name(at, language), language), Some(at));
            }
        }
        assert_eq!(book_name(0, "fr"), "Genesis", "a language without names of its own is English");
    }

    #[test]
    fn a_short_form_is_read_in_the_language_of_the_bible_first() {
        let samuel = BOOKS.iter().position(|b| b.code == "1SA");
        let kings = BOOKS.iter().position(|b| b.code == "1KI");
        assert_eq!(find_book_in("1 Цар", "ru"), samuel);
        assert_eq!(find_book_in("1 Цар", "uk"), kings);
        assert_eq!(find_book("1 Цар"), samuel, "with no language, as before");
    }
}
