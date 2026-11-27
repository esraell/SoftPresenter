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
    /// The German name, as the Luther Bible names the book in a reference ("1. Mose",
    /// "Johannes"), and the usual abbreviations (the Loccum guidelines' and the Luther Bible's:
    /// `1mo`, `joh`, `offb`), with an umlaut also written out (`röm`, `roem`).
    pub german: &'static str,
    pub german_short: &'static [&'static str],
}

macro_rules! book {
    (
        $code:literal,
        $en:literal,
        [$($ens:literal),*],
        $ru:literal,
        [$($rus:literal),*],
        $uk:literal,
        [$($uks:literal),*],
        $de:literal,
        [$($des:literal),*]
    ) => {
        Book {
            code: $code,
            english: $en,
            english_short: &[$($ens),*],
            russian: $ru,
            russian_short: &[$($rus),*],
            ukrainian: $uk,
            ukrainian_short: &[$($uks),*],
            german: $de,
            german_short: &[$($des),*],
        }
    };
}

/// Every book, in the usual order.
pub const BOOKS: [Book; 66] = [
    book!(
        "GEN",
        "Genesis",
        ["gen", "gn"],
        "Бытие",
        ["быт"],
        "Буття",
        ["1м", "бут"],
        "1. Mose",
        ["1mo", "1mose", "gen"]
    ),
    book!(
        "EXO",
        "Exodus",
        ["exo", "ex", "exod"],
        "Исход",
        ["исх"],
        "Вихід",
        ["2м", "вих"],
        "2. Mose",
        ["2mo", "2mose", "ex"]
    ),
    book!(
        "LEV",
        "Leviticus",
        ["lev", "lv"],
        "Левит",
        ["лев"],
        "Левит",
        ["3м", "лев"],
        "3. Mose",
        ["3mo", "3mose", "lev"]
    ),
    book!(
        "NUM",
        "Numbers",
        ["num", "nm"],
        "Числа",
        ["чис"],
        "Числа",
        ["4м", "чис"],
        "4. Mose",
        ["4mo", "4mose", "num"]
    ),
    book!(
        "DEU",
        "Deuteronomy",
        ["deu", "deut", "dt"],
        "Второзаконие",
        ["втор"],
        "Повторення Закону",
        ["5м", "повт"],
        "5. Mose",
        ["5mo", "5mose", "dtn"]
    ),
    book!(
        "JOS",
        "Joshua",
        ["jos", "josh"],
        "Иисус Навин",
        ["нав", "навин"],
        "Ісуса Навина",
        ["єг", "нав"],
        "Josua",
        ["jos"]
    ),
    book!("JDG", "Judges", ["jdg", "judg"], "Судьи", ["суд"], "Суддів", ["суд"], "Richter", ["ri"]),
    book!("RUT", "Ruth", ["rut", "ru"], "Руфь", ["руф"], "Рут", ["рут"], "Rut", ["rut", "ruth"]),
    book!(
        "1SA",
        "1 Samuel",
        ["1sa", "1sam", "1sm"],
        "1 Царств",
        ["1цар"],
        "1 Самуїлова",
        ["1сам"],
        "1. Samuel",
        ["1sam"]
    ),
    book!(
        "2SA",
        "2 Samuel",
        ["2sa", "2sam", "2sm"],
        "2 Царств",
        ["2цар"],
        "2 Самуїлова",
        ["2сам"],
        "2. Samuel",
        ["2sam"]
    ),
    book!(
        "1KI",
        "1 Kings",
        ["1ki", "1kgs", "1kin"],
        "3 Царств",
        ["3цар"],
        "1 Царів",
        ["1цар"],
        "1. Könige",
        ["1kön", "1kon"]
    ),
    book!(
        "2KI",
        "2 Kings",
        ["2ki", "2kgs", "2kin"],
        "4 Царств",
        ["4цар"],
        "2 Царів",
        ["2цар"],
        "2. Könige",
        ["2kön", "2kon"]
    ),
    book!(
        "1CH",
        "1 Chronicles",
        ["1ch", "1chr", "1chron"],
        "1 Паралипоменон",
        ["1пар"],
        "1 Хронік",
        ["1хр"],
        "1. Chronik",
        ["1chr"]
    ),
    book!(
        "2CH",
        "2 Chronicles",
        ["2ch", "2chr", "2chron"],
        "2 Паралипоменон",
        ["2пар"],
        "2 Хронік",
        ["2хр"],
        "2. Chronik",
        ["2chr"]
    ),
    book!("EZR", "Ezra", ["ezr"], "Ездра", ["езд"], "Ездри", ["езд"], "Esra", ["esr"]),
    book!("NEH", "Nehemiah", ["neh"], "Неемия", ["неем"], "Неемії", ["неем"], "Nehemia", ["neh"]),
    book!("EST", "Esther", ["est", "esth"], "Есфирь", ["есф"], "Естер", ["ест"], "Ester", ["est", "esther"]),
    book!("JOB", "Job", ["jb"], "Иов", [], "Йова", ["йов"], "Hiob", ["hi", "ijob"]),
    book!(
        "PSA",
        "Psalms",
        ["psa", "ps", "psalm", "pss"],
        "Псалтирь",
        ["пс", "псалом", "псалмы"],
        "Псалом",
        ["пс", "псалми"],
        "Psalmen",
        ["ps", "psalm", "psalter"]
    ),
    book!(
        "PRO",
        "Proverbs",
        ["pro", "prov", "pr"],
        "Притчи",
        ["прит", "притч"],
        "Приповісті",
        ["пр", "прип"],
        "Sprüche",
        ["spr"]
    ),
    book!(
        "ECC",
        "Ecclesiastes",
        ["ecc", "eccl", "eccles"],
        "Екклесиаст",
        ["еккл", "екк"],
        "Екклезіяст",
        ["екл"],
        "Prediger",
        ["pred", "koh", "kohelet"]
    ),
    book!(
        "SOL",
        "Song of Solomon",
        ["sol", "song", "songofsongs", "sos", "sng"],
        "Песня Песней",
        ["песн", "песнь"],
        "Пісня над піснями",
        ["пісн"],
        "Hoheslied",
        ["hld"]
    ),
    book!("ISA", "Isaiah", ["isa", "is"], "Исаия", ["ис"], "Ісаї", ["іс"], "Jesaja", ["jes"]),
    book!("JER", "Jeremiah", ["jer"], "Иеремия", ["иер"], "Єремії", ["єр"], "Jeremia", ["jer"]),
    book!(
        "LAM",
        "Lamentations",
        ["lam"],
        "Плач Иеремии",
        ["плач"],
        "Плач Єремії",
        ["плач"],
        "Klagelieder",
        ["klgl", "klag"]
    ),
    book!(
        "EZE",
        "Ezekiel",
        ["eze", "ezek", "ezk"],
        "Иезекииль",
        ["иез"],
        "Єзекіїля",
        ["єз"],
        "Hesekiel",
        ["hes", "ezechiel"]
    ),
    book!("DAN", "Daniel", ["dan", "dn"], "Даниил", ["дан"], "Даниїла", ["дан"], "Daniel", ["dan"]),
    book!("HOS", "Hosea", ["hos"], "Осия", ["ос"], "Осії", ["ос"], "Hosea", ["hos"]),
    book!("JOE", "Joel", ["joe", "jl"], "Иоиль", ["иоил"], "Йоіла", ["йоіл"], "Joel", ["joel"]),
    book!("AMO", "Amos", ["amo", "am"], "Амос", ["ам"], "Амоса", ["ам"], "Amos", ["am"]),
    book!("OBA", "Obadiah", ["oba", "obad", "ob"], "Авдий", ["авд"], "Овдія", ["овд"], "Obadja", ["ob", "obd"]),
    book!("JON", "Jonah", ["jon", "jnh"], "Иона", ["ион"], "Йони", ["йона"], "Jona", ["jon"]),
    book!("MIC", "Micah", ["mic"], "Михей", ["мих"], "Михея", ["мих"], "Micha", ["mi"]),
    book!("NAH", "Nahum", ["nah", "na"], "Наум", [], "Наума", ["наум"], "Nahum", ["nah"]),
    book!("HAB", "Habakkuk", ["hab"], "Аввакум", ["авв"], "Авакума", ["ав"], "Habakuk", ["hab"]),
    book!(
        "ZEP",
        "Zephaniah",
        ["zep", "zeph"],
        "Софония",
        ["соф"],
        "Софонії",
        ["соф"],
        "Zefanja",
        ["zef", "zeph", "zephanja"]
    ),
    book!("HAG", "Haggai", ["hag"], "Аггей", ["агг"], "Огія", ["ог"], "Haggai", ["hag"]),
    book!("ZEC", "Zechariah", ["zec", "zech"], "Захария", ["зах"], "Захарія", ["зах"], "Sacharja", ["sach"]),
    book!("MAL", "Malachi", ["mal"], "Малахия", ["мал"], "Малахії", ["мал"], "Maleachi", ["mal"]),
    book!(
        "MAT",
        "Matthew",
        ["mat", "matt", "mt"],
        "От Матфея",
        ["мф", "матфея", "мат"],
        "Від Матвія",
        ["мт", "матвія"],
        "Matthäus",
        ["mt", "matth"]
    ),
    book!(
        "MAR",
        "Mark",
        ["mar", "mk", "mrk"],
        "От Марка",
        ["мк", "марка", "мар"],
        "Від Марка",
        ["мр", "мк", "марка"],
        "Markus",
        ["mk", "mark"]
    ),
    book!(
        "LUK",
        "Luke",
        ["luk", "lk"],
        "От Луки",
        ["лк", "луки", "лук"],
        "Від Луки",
        ["лк", "луки"],
        "Lukas",
        ["lk", "luk"]
    ),
    book!(
        "JOH",
        "John",
        ["joh", "jn", "jhn"],
        "От Иоанна",
        ["ин", "иоанна", "иоан"],
        "Від Івана",
        ["ів", "івана"],
        "Johannes",
        ["joh"]
    ),
    book!("ACT", "Acts", ["act", "ac"], "Деяния", ["деян"], "Дії", ["дії"], "Apostelgeschichte", ["apg"]),
    book!(
        "ROM",
        "Romans",
        ["rom", "ro", "rm"],
        "К Римлянам",
        ["рим", "римлянам"],
        "До римлян",
        ["рим"],
        "Römer",
        ["röm"]
    ),
    book!(
        "1CO",
        "1 Corinthians",
        ["1co", "1cor"],
        "1 Коринфянам",
        ["1кор"],
        "1 до коринтян",
        ["1кор"],
        "1. Korinther",
        ["1kor"]
    ),
    book!(
        "2CO",
        "2 Corinthians",
        ["2co", "2cor"],
        "2 Коринфянам",
        ["2кор"],
        "2 до коринтян",
        ["2кор"],
        "2. Korinther",
        ["2kor"]
    ),
    book!("GAL", "Galatians", ["gal"], "Галатам", ["гал"], "До галатів", ["гал"], "Galater", ["gal"]),
    book!("EPH", "Ephesians", ["eph"], "Ефесянам", ["еф"], "До ефесян", ["еф"], "Epheser", ["eph"]),
    book!(
        "PHI",
        "Philippians",
        ["phi", "phil", "php"],
        "Филиппийцам",
        ["флп", "фил"],
        "До филип\u{2019}ян",
        ["фил"],
        "Philipper",
        ["phil"]
    ),
    book!("COL", "Colossians", ["col"], "Колоссянам", ["кол"], "До колосян", ["кол"], "Kolosser", ["kol"]),
    book!(
        "1TH",
        "1 Thessalonians",
        ["1th", "1thess", "1thes"],
        "1 Фессалоникийцам",
        ["1фес"],
        "1 до солунян",
        ["1сол"],
        "1. Thessalonicher",
        ["1thess", "1thes", "1th"]
    ),
    book!(
        "2TH",
        "2 Thessalonians",
        ["2th", "2thess", "2thes"],
        "2 Фессалоникийцам",
        ["2фес"],
        "2 до солунян",
        ["2сол"],
        "2. Thessalonicher",
        ["2thess", "2thes", "2th"]
    ),
    book!(
        "1TI",
        "1 Timothy",
        ["1ti", "1tim"],
        "1 Тимофею",
        ["1тим"],
        "1 до Тимофія",
        ["1тим"],
        "1. Timotheus",
        ["1tim"]
    ),
    book!(
        "2TI",
        "2 Timothy",
        ["2ti", "2tim"],
        "2 Тимофею",
        ["2тим"],
        "2 до Тимофія",
        ["2тим"],
        "2. Timotheus",
        ["2tim"]
    ),
    book!("TIT", "Titus", ["tit"], "Титу", ["тит"], "До Тита", ["тит"], "Titus", ["tit"]),
    book!(
        "PHM",
        "Philemon",
        ["phm", "philem"],
        "Филимону",
        ["флм"],
        "До Филимона",
        ["филим"],
        "Philemon",
        ["phlm", "philem"]
    ),
    book!("HEB", "Hebrews", ["heb"], "Евреям", ["евр"], "До євреїв", ["євр"], "Hebräer", ["hebr"]),
    book!("JAM", "James", ["jam", "jas"], "Иакова", ["иак"], "Якова", ["як"], "Jakobus", ["jak"]),
    book!(
        "1PE",
        "1 Peter",
        ["1pe", "1pet", "1pt"],
        "1 Петра",
        ["1пет", "1петр"],
        "1 Петра",
        ["1петр"],
        "1. Petrus",
        ["1petr", "1pet"]
    ),
    book!(
        "2PE",
        "2 Peter",
        ["2pe", "2pet", "2pt"],
        "2 Петра",
        ["2пет", "2петр"],
        "2 Петра",
        ["2петр"],
        "2. Petrus",
        ["2petr", "2pet"]
    ),
    book!(
        "1JO",
        "1 John",
        ["1jo", "1jn", "1jhn"],
        "1 Иоанна",
        ["1ин", "1иоан"],
        "1 Івана",
        ["1ів"],
        "1. Johannes",
        ["1joh"]
    ),
    book!(
        "2JO",
        "2 John",
        ["2jo", "2jn", "2jhn"],
        "2 Иоанна",
        ["2ин", "2иоан"],
        "2 Івана",
        ["2ів"],
        "2. Johannes",
        ["2joh"]
    ),
    book!(
        "3JO",
        "3 John",
        ["3jo", "3jn", "3jhn"],
        "3 Иоанна",
        ["3ин", "3иоан"],
        "3 Івана",
        ["3ів"],
        "3. Johannes",
        ["3joh"]
    ),
    book!("JUD", "Jude", ["jud", "jd"], "Иуды", ["иуд"], "Юди", ["юд"], "Judas", ["jud"]),
    book!(
        "REV",
        "Revelation",
        ["rev", "rv", "revelations"],
        "Откровение",
        ["откр", "отк"],
        "Об\u{2019}явлення",
        ["об", "обявлення"],
        "Offenbarung",
        ["offb", "offenb", "off"]
    ),
];

/// What was typed, or a name or short form, for matching: no spaces, full stops or apostrophes
/// (typed as ', ’ or ʼ), in lower case, and German's ä, ö, ü and ß written out (ae, oe, ue, ss),
/// as a keyboard without them types them: "Roem" is "Röm", "Roemer" "Römer".
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().filter(|c| !c.is_whitespace() && !matches!(c, '.' | '\'' | '\u{2019}' | '\u{2bc}')) {
        for c in c.to_lowercase() {
            match c {
                'ä' => out.push_str("ae"),
                'ö' => out.push_str("oe"),
                'ü' => out.push_str("ue"),
                'ß' => out.push_str("ss"),
                c => out.push(c),
            }
        }
    }
    out
}

/// Whether what was typed (already plain) is one of a book's short forms.
fn is_short(shorts: &[&str], typed: &str) -> bool {
    shorts.iter().any(|short| plain(short) == typed)
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
                "de" => (book.german, book.german_short, &[]),
                _ => (book.english, book.english_short, &[]),
            }
        };
    let mut order: Vec<&str> = vec![language];
    order.extend(LANGUAGES.into_iter().filter(|other| *other != language));
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
            if plain(name) == typed || is_short(shorts, &typed) {
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

/// The same, reading what was typed in a language first (`en`, `ru`, `uk` or `de`): the same
/// short form can name different books ("1 Цар" is 1 Samuel in Russian but 1 Kings in Ukrainian).
pub fn find_book_in(typed: &str, language: &str) -> Option<usize> {
    find_book_typed(typed, language).map(|(book, _)| book)
}

/// The languages a book's names are written in, in the order they are tried when none is
/// preferred.
const LANGUAGES: [&str; 4] = ["en", "ru", "uk", "de"];

/// [`find_book_in`], also saying which languages what was typed names the book in (`en`, `ru`,
/// `uk`, `de`, in that order). More than one when a name or short form is the same in two: "Пс",
/// "Лев" and "1 Кор" are Russian and Ukrainian alike, "Joh" and "Dan" English and German. An
/// exact name or short form decides the languages when there is one; otherwise every language
/// whose name starts with what was typed. German starts are tried first only when German is
/// preferred, and otherwise after the other three, so a start typed in English finds what it
/// always found ("He" is still Hebrews, not Hesekiel).
pub fn find_book_typed(typed: &str, language: &str) -> Option<(usize, Vec<&'static str>)> {
    let typed = plain(typed);
    if typed.is_empty() {
        return None;
    }
    let exact_in = |book: &Book, language: &str| match language {
        "en" => plain(book.english) == typed || is_short(book.english_short, &typed),
        "ru" => plain(book.russian) == typed || is_short(book.russian_short, &typed),
        "uk" => plain(book.ukrainian) == typed || is_short(book.ukrainian_short, &typed),
        "de" => plain(book.german) == typed || is_short(book.german_short, &typed),
        _ => false,
    };
    let starts_in = |book: &Book, language: &str| match language {
        "en" => name_starts(book.english, &[], &typed),
        "ru" => name_starts(book.russian, &["от", "к"], &typed),
        "uk" => name_starts(book.ukrainian, &["від", "до"], &typed),
        "de" => name_starts(book.german, &[], &typed),
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
    let starts_in_german = || BOOKS.iter().position(|book| starts_in(book, "de"));
    let found = (language == "de")
        .then(starts_in_german)
        .flatten()
        .or_else(|| BOOKS.iter().position(|book| ["en", "ru", "uk"].iter().any(|language| starts_in(book, language))))
        .or_else(starts_in_german)?;
    Some((found, LANGUAGES.into_iter().filter(|language| starts_in(&BOOKS[found], language)).collect()))
}

/// A book's name as a language writes it (`ru`, `uk`, `de`; English for any other).
pub fn book_name(book: usize, language: &str) -> &'static str {
    let names = &BOOKS[book.min(BOOKS.len() - 1)];
    match language {
        "ru" => names.russian,
        "uk" => names.ukrainian,
        "de" => names.german,
        _ => names.english,
    }
}

/// A book's name as people read it in a reference to part of it, in a language (`en`, `ru`,
/// `uk`, `de`; English for any other): the Psalms named as one psalm when the reference is all in
/// one ("Psalm 23", "Псалом 22", German "Psalm 23"), and as psalms when it runs across several
/// ("Psalms 23–24"; in Ukrainian "Псалми", in German "Psalmen"; Russian keeps the book's name,
/// "Псалтирь"); any other book as
/// [`book_name`] has it. Each of these names reads back as the same book. For what people read
/// (a title, the reference on a slide); what is kept and read back again is named by
/// `label_in`, with the books' own names.
pub fn reading_book_name(book: usize, language: &str, one_chapter: bool) -> &'static str {
    if BOOKS[book.min(BOOKS.len() - 1)].code != "PSA" {
        return book_name(book, language);
    }
    match (language, one_chapter) {
        ("ru", true) => "Псалом",
        ("uk", false) => "Псалми",
        ("ru" | "uk", _) => book_name(book, language),
        (_, true) => "Psalm",
        _ => book_name(book, language),
    }
}

/// A book's place in [`BOOKS`] from its code in a Bible file.
pub fn book_by_code(code: &str) -> Option<usize> {
    BOOKS.iter().position(|book| book.code == code)
}

/// A book's place in [`BOOKS`] from its standard three-letter code (as USFM, USX and the
/// numbering tables write it, in any case), or from the code our own files use where the two
/// differ.
pub fn book_by_standard_code(code: &str) -> Option<usize> {
    let code = code.trim().to_ascii_uppercase();
    let ours = match code.as_str() {
        "SNG" => "SOL",
        "EZK" => "EZE",
        "JOL" => "JOE",
        "NAM" => "NAH",
        "MRK" => "MAR",
        "JHN" => "JOH",
        "PHP" => "PHI",
        "JAS" => "JAM",
        "1JN" => "1JO",
        "2JN" => "2JO",
        "3JN" => "3JO",
        other => other,
    };
    book_by_code(ours)
}

/// The sixty-six books' names in OSIS files (`Gen`, `Exod`… `Rev`), in the order of [`BOOKS`].
const OSIS_NAMES: [&str; 66] = [
    "Gen", "Exod", "Lev", "Num", "Deut", "Josh", "Judg", "Ruth", "1Sam", "2Sam", "1Kgs", "2Kgs", "1Chr", "2Chr",
    "Ezra", "Neh", "Esth", "Job", "Ps", "Prov", "Eccl", "Song", "Isa", "Jer", "Lam", "Ezek", "Dan", "Hos", "Joel",
    "Amos", "Obad", "Jonah", "Mic", "Nah", "Hab", "Zeph", "Hag", "Zech", "Mal", "Matt", "Mark", "Luke", "John", "Acts",
    "Rom", "1Cor", "2Cor", "Gal", "Eph", "Phil", "Col", "1Thess", "2Thess", "1Tim", "2Tim", "Titus", "Phlm", "Heb",
    "Jas", "1Pet", "2Pet", "1John", "2John", "3John", "Jude", "Rev",
];

/// A book's place in [`BOOKS`] from its name in an OSIS file (`Gen`, `Ps`, `1John`), in any case.
pub fn book_by_osis(name: &str) -> Option<usize> {
    OSIS_NAMES.iter().position(|osis| osis.eq_ignore_ascii_case(name.trim()))
}

/// A book's place in [`BOOKS`] from its number in the usual order, 1 (Genesis) to 66
/// (Revelation). Numbers past 66 are the books some Bibles add, which the app leaves out.
pub fn book_by_number(number: u32) -> Option<usize> {
    (1..=66).contains(&number).then(|| number as usize - 1)
}

/// A book's place in [`BOOKS`] from its name as a Bible file writes it, in English, Russian,
/// Ukrainian or German: a name, short form or code exactly; else the most words at its start that are
/// one ("Revelation of John", "Деяния апостолов"). Books some Bibles add ("Judith") find none.
///
/// A file's name for a book is untrusted and may be any length: only its first
/// [`WRITTEN_NAME_CHARS`] characters and [`WRITTEN_NAME_WORDS`] words are looked at, against
/// every book's names made plain once, so it takes the same short time whatever it is.
pub fn book_by_written_name(name: &str) -> Option<usize> {
    let known = written_names();
    let exactly = |typed: &str| known.get(typed).copied();
    let name: String = name.chars().take(WRITTEN_NAME_CHARS).collect();
    let mut words: Vec<String> =
        name.split([' ', '.', '\u{a0}']).map(plain).filter(|w| !w.is_empty()).take(WRITTEN_NAME_WORDS).collect();
    // "II Kings" is 2 Kings.
    let several = words.len() > 1;
    if let Some(first) = words.first_mut()
        && let Some(number) = ["i", "ii", "iii"].iter().position(|roman| first == roman)
        && several
    {
        *first = (number + 1).to_string();
    }
    (1..=words.len()).rev().find_map(|count| exactly(&words[..count].concat()))
}

/// The most characters of a book's name in a Bible file that are looked at. The longest name
/// of a book is under forty.
const WRITTEN_NAME_CHARS: usize = 100;
/// The most words of a book's name in a Bible file that are looked at.
const WRITTEN_NAME_WORDS: usize = 12;

/// Every book's names, short forms and code, made plain, each with the book it names (the
/// first book, in the usual order, where two share one). Made once.
fn written_names() -> &'static std::collections::HashMap<String, usize> {
    static NAMES: std::sync::OnceLock<std::collections::HashMap<String, usize>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = std::collections::HashMap::new();
        for (at, book) in BOOKS.iter().enumerate() {
            let forms = [book.english, book.russian, book.ukrainian, book.german]
                .into_iter()
                .map(plain)
                .chain(
                    book.english_short
                        .iter()
                        .chain(book.russian_short)
                        .chain(book.ukrainian_short)
                        .chain(book.german_short)
                        .map(|short| plain(short)),
                )
                .chain(std::iter::once(book.code.to_lowercase()));
            for form in forms {
                names.entry(form).or_insert(at);
            }
        }
        names
    })
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
        assert_eq!(typed("gen", ""), Some(("GEN", vec!["en", "de"])), "Gen is German's too");
        assert_eq!(typed("Gn", ""), Some(("GEN", vec!["en"])));
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
    fn german_names_and_short_forms_find_their_books() {
        let code = |typed: &str, preferred: &str| find_book_in(typed, preferred).map(|book| BOOKS[book].code);
        for (typed, book) in [
            ("Joh", "JOH"),
            ("Johannes", "JOH"),
            ("1. Mose", "GEN"),
            ("1Mo", "GEN"),
            ("1 Mose", "GEN"),
            ("5. Mose", "DEU"),
            ("Ri", "JDG"),
            ("Hi", "JOB"),
            ("Spr", "PRO"),
            ("Pred", "ECC"),
            ("Hld", "SOL"),
            ("Jes", "ISA"),
            ("Hes", "EZE"),
            ("Sach", "ZEC"),
            ("Mt", "MAT"),
            ("Matthäus", "MAT"),
            ("Apg", "ACT"),
            ("Röm", "ROM"),
            ("Roem", "ROM"),
            ("Römer", "ROM"),
            ("1. Kor", "1CO"),
            ("2Kön", "2KI"),
            ("2Koe", "2KI"),
            ("Hebr", "HEB"),
            ("Jak", "JAM"),
            ("1. Petr", "1PE"),
            ("1Joh", "1JO"),
            ("Offb", "REV"),
            ("Offenbarung", "REV"),
        ] {
            assert_eq!(code(typed, "de"), Some(book), "{typed}");
            assert_eq!(code(typed, ""), Some(book), "{typed} with no language preferred");
        }
        for (at, book) in BOOKS.iter().enumerate() {
            assert_eq!(find_book_in(book.german, "de"), Some(at), "{}", book.german);
            for short in book.german_short {
                assert_eq!(find_book_in(short, "de"), Some(at), "{short} should find {}", book.german);
            }
        }
        // A start is read in German first when German is preferred, and otherwise as before.
        assert_eq!(code("He", "de"), Some("EZE"));
        assert_eq!(code("He", "en"), Some("HEB"));
        assert_eq!(code("He", ""), Some("HEB"));
        assert_eq!(code("Ho", ""), Some("HOS"));
        assert_eq!(code("Ho", "de"), Some("SOL"), "Hoheslied");
        assert_eq!(code("Offen", "en"), Some("REV"), "a German start nothing else has");
        assert_eq!(suggest_books("Joh", "de", 1)[0].1, "Johannes", "named in the language preferred");
    }

    #[test]
    fn no_name_or_short_form_names_two_books() {
        // Every exact form, in any language, names one book only: a German short form that is an
        // English, Russian or Ukrainian one too names the same book ("joh", "ps"), never another.
        let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        let mut clashes = Vec::new();
        for book in &BOOKS {
            let names = [book.english, book.russian, book.ukrainian, book.german].map(plain);
            let shorts = book
                .english_short
                .iter()
                .chain(book.russian_short)
                .chain(book.ukrainian_short)
                .chain(book.german_short)
                .map(|short| plain(short));
            for form in names.into_iter().chain(shorts) {
                if let Some(other) = seen.insert(form.clone(), book.code)
                    && other != book.code
                {
                    clashes.push(format!("{form}: {other} and {}", book.code));
                }
            }
        }
        // The two known before German came, and kept on purpose: a Russian Bible's 1 Samuel and 2
        // Samuel are "1 Царств" and "2 Царств", a Ukrainian Bible's 1 and 2 Kings "1 Царів" and
        // "2 Царів" (each read in the Bible's own language first).
        clashes.retain(|clash| !clash.starts_with("1цар") && !clash.starts_with("2цар"));
        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn every_full_name_finds_its_book_whichever_language_is_preferred() {
        // The Bible page writes a passage's name back in the operator's language and reads it
        // again with the first Bible's language preferred: it must come back the same book.
        for (at, book) in BOOKS.iter().enumerate() {
            for name in [book.english, book.russian, book.ukrainian, book.german] {
                for preferred in ["", "en", "ru", "uk", "de"] {
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
