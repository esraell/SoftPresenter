//! The songbooks the source publishes: a song pack for each language, at `<language>.qsp`.
//!
//! The source has no list to fetch (its folder of packs refuses to be listed), only a table on
//! its download page, so the languages are built in here, as that table stood on 24 November
//! 2026, and nothing is read from the page while the app runs. Left out: the full database
//! (`all`, every language at once), the same songs in another script (`az-RU`, `kk-CN`, `uz-EN`
//! and the like, whose codes the catalog's folders don't take), and the packs the table lists
//! with about ten songs. The table's song counts are rounded and don't agree with the packs, so
//! they are not kept: a window says the size the source gives, and the songs once downloaded.

use super::fold::fold;

/// A songbook the source publishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Songbook {
    /// The language's code, as the pack's address and the catalog's folders name it.
    pub code: &'static str,
    /// The language's name in English, for the sentences `sp` says.
    pub english: &'static str,
    /// Its name written in itself, where the windows' fonts draw that script on every system
    /// (Latin and Cyrillic letters). Elsewhere (Arabic, Chinese, Korean, Thai…) the English name
    /// stands in, rather than letters some computers would draw as boxes.
    own: Option<&'static str>,
}

impl Songbook {
    /// What a window calls it: its own name, as the Language menu names languages.
    pub fn name(&self) -> &'static str {
        self.own.unwrap_or(self.english)
    }
}

const fn book(code: &'static str, english: &'static str, own: Option<&'static str>) -> Songbook {
    Songbook { code, english, own }
}

/// Every songbook offered, in the order of the source's table (by English name).
pub const SONGBOOKS: &[Songbook] = &[
    book("af", "Afrikaans", Some("Afrikaans")),
    book("ar", "Arabic", None),
    book("hy", "Armenian", None),
    book("az", "Azerbaijani", Some("Azərbaycanca")),
    book("ba", "Bashkir", Some("Башҡортса")),
    book("be", "Belarusian", Some("Беларуская")),
    book("bg", "Bulgarian", Some("Български")),
    book("zh", "Chinese", None),
    book("cho", "Choctaw", Some("Chahta")),
    book("crh", "Crimean Tatar", Some("Къырымтатарджа")),
    book("hr", "Croatian", Some("Hrvatski")),
    book("cs", "Czech", Some("Čeština")),
    book("prs", "Dari", None),
    book("en", "English", Some("English")),
    book("fi", "Finnish", Some("Suomi")),
    book("fr", "French", Some("Français")),
    book("gnb", "Gangte", Some("Gangte")),
    book("ka", "Georgian", None),
    book("de", "German", Some("Deutsch")),
    book("cnh", "Hakha Chin", Some("Laiholh")),
    book("ha", "Hausa", Some("Hausa")),
    book("hu", "Hungarian", Some("Magyar")),
    book("id", "Indonesian", Some("Bahasa Indonesia")),
    book("it", "Italian", Some("Italiano")),
    book("kaa", "Karakalpak", Some("Қарақалпақша")),
    book("krc", "Karachay-Balkar", Some("Къарачай-малкъар тил")),
    book("kk", "Kazakh", Some("Қазақша")),
    book("kv", "Komi", Some("Коми кыв")),
    book("ko", "Korean", None),
    book("kmr", "Kurdish", Some("Kurmancî")),
    book("ky", "Kyrgyz", Some("Кыргызча")),
    book("mg", "Malagasy", Some("Malagasy")),
    book("mn", "Mongolian", Some("Монгол")),
    book("no", "Norwegian", Some("Norsk")),
    book("ps", "Pashto", None),
    book("fa", "Persian", None),
    book("pl", "Polish", Some("Polski")),
    book("pt", "Portuguese", Some("Português")),
    book("ro", "Romanian", Some("Română")),
    book("ru", "Russian", Some("Русский")),
    book("ru-ca", "Russian (Central Asia)", Some("Русский (Центральная Азия)")),
    book("sk", "Slovak", Some("Slovenčina")),
    book("alt", "Southern Altai", Some("Алтай тил")),
    book("es", "Spanish", Some("Español")),
    book("sw", "Swahili", Some("Kiswahili")),
    book("sv", "Swedish", Some("Svenska")),
    book("tab", "Tabasaran", Some("Табасаран чӀал")),
    book("tg", "Tajik", Some("Тоҷикӣ")),
    book("ta", "Tamil", None),
    book("tt", "Tatar", Some("Татарча")),
    book("te", "Telugu", None),
    book("th", "Thai", None),
    book("aeb", "Tunisian Arabic", None),
    book("tr", "Turkish", Some("Türkçe")),
    book("tk", "Turkmen", Some("Түркменче")),
    book("tyv", "Tuvan", Some("Тыва дыл")),
    book("udm", "Udmurt", Some("Удмурт кыл")),
    book("uk", "Ukrainian", Some("Українська")),
    book("ug", "Uyghur", Some("Уйғурчә")),
    book("uz", "Uzbek", Some("Ўзбекча")),
];

/// The songbook of a language code (`ru`, or a tag such as `en-US` by its first part).
pub fn songbook(code: &str) -> Option<&'static Songbook> {
    let code = code.trim().to_ascii_lowercase();
    let base = code.split(['-', '_']).next().unwrap_or("");
    SONGBOOKS.iter().find(|book| book.code == code).or_else(|| SONGBOOKS.iter().find(|book| book.code == base))
}

/// The songbooks in the order a list shows them: by their names as they are written, Latin
/// letters before Cyrillic, a letter with a mark beside the same letter without (Čeština among
/// the C's).
pub fn in_name_order() -> Vec<&'static Songbook> {
    let mut books: Vec<&'static Songbook> = SONGBOOKS.iter().collect();
    books.sort_by_cached_key(|book| fold(book.name()).chars().map(unmarked).collect::<String>());
    books
}

/// A Latin letter without its mark, for putting names in order.
fn unmarked(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' => 'a',
        'ç' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ə' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ı' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'š' | 'ș' | 'ş' => 's',
        'ț' | 'ţ' => 't',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ž' => 'z',
        other => other,
    }
}

/// The songbooks to recommend, ticked to start with: the language the operator chose for
/// SoftPresenter itself (`app`, such as `es`), then the languages the church shows, the one on
/// top first; each once, and only those the source publishes.
pub fn recommended(app: &str, church: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for code in std::iter::once(app).chain(church.iter().map(String::as_str)) {
        if let Some(book) = songbook(code)
            && !out.contains(&book.code)
        {
            out.push(book.code);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyrillic(c: char) -> bool {
        ('\u{400}'..='\u{52f}').contains(&c)
    }

    #[test]
    fn every_songbook_has_a_code_the_catalog_takes_and_a_name() {
        let mut seen = std::collections::HashSet::new();
        for book in SONGBOOKS {
            assert!(super::super::valid_language(book.code), "{} isn't a code the catalog takes", book.code);
            assert!(seen.insert(book.code), "{} is listed twice", book.code);
            assert!(!book.name().is_empty() && !book.english.is_empty(), "{}", book.code);
            // A name written in itself is in Latin or Cyrillic letters, which every system draws.
            let drawn = |c: char| c.is_ascii() || ('\u{a0}'..='\u{2af}').contains(&c) || cyrillic(c);
            assert!(book.name().chars().all(drawn), "{} is named in letters some computers can't draw", book.code);
        }
        // The app's own languages are all there.
        for language in crate::i18n::Language::ALL {
            assert!(songbook(language.code()).is_some(), "{language:?}");
        }
        assert!(songbook("all").is_none() && songbook("az-RU").is_none_or(|book| book.code == "az"));
    }

    #[test]
    fn the_operators_language_is_recommended_first_then_the_churchs() {
        let langs = |codes: &[&str]| codes.iter().map(|code| code.to_string()).collect::<Vec<_>>();
        // A Spanish speaker, before the setup asks which languages the church shows.
        assert_eq!(recommended("es", &[]), ["es"]);
        // A church showing Russian on top and English below, its operator reading Russian.
        assert_eq!(recommended("ru", &langs(&["ru", "en"])), ["ru", "en"]);
        // Its English-reading operator: the same two, theirs first.
        assert_eq!(recommended("en", &langs(&["ru", "en"])), ["en", "ru"]);
        // A Ukrainian operator in a Russian and English church: all three.
        assert_eq!(recommended("uk", &langs(&["ru", "en"])), ["uk", "ru", "en"]);
        // Tags with a region, upper case, and languages the source doesn't publish.
        assert_eq!(recommended("es-ES", &langs(&["EN", "xx", "en_GB", "ro"])), ["es", "en", "ro"]);
        assert!(recommended("", &[]).is_empty());
    }

    #[test]
    fn a_list_shows_latin_names_before_cyrillic_each_in_order() {
        let names: Vec<&str> = in_name_order().iter().map(|book| book.name()).collect();
        assert_eq!(names.len(), SONGBOOKS.len());
        let first_cyrillic = names.iter().position(|name| name.starts_with(cyrillic)).unwrap_or(0);
        assert!(names[first_cyrillic..].iter().all(|name| name.starts_with(cyrillic)), "{names:?}");
        assert!(names[..first_cyrillic].contains(&"English") && names[..first_cyrillic].contains(&"Español"));
        assert!(names[first_cyrillic..].contains(&"Русский") && names[first_cyrillic..].contains(&"Українська"));
        let at = |name: &str| names.iter().position(|n| *n == name);
        assert!(at("Bahasa Indonesia") < at("Čeština") && at("Čeština") < at("Chinese"));
        assert!(at("Chinese") < at("Deutsch") && at("Deutsch") < at("English"));
    }
}
