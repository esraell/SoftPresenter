//! Which language the operator reads the Bible in, kept apart from what the audience sees.
//!
//! The slides made from a passage follow the translations chosen (which Bible goes in which
//! box), whatever language the reference was typed in. What the operator reads follows the
//! language typed: a reference written with an English book name is named back in English and
//! quoted from the first English Bible in use; one written in Russian or Ukrainian, in that.
//! With nothing typed to go by (a book clicked in the lists, the Bible view opened fresh) it is
//! the language last typed this session, or at its start the computer's. A language with no
//! Bible in use falls back to the first Bible in the translations, as before.
//!
//! Chapter and verse numbers are always the first translation's: only the book's name, and the
//! words quoted, are the operator's. So a name written back in the field reads again as the
//! same passage.
//!
//! Each Bible's language is the `language` its file gives (every Bible offered says one: `en`,
//! `ru`, `uk`, `de`…). A Bible that gives none is of no language: it is never chosen for one,
//! and only the fallback to the first Bible reaches it.

/// A Bible's language as a short code: the first part of what its file says, in lower case
/// (`en`, `ru`, `uk`); empty when it says nothing.
pub fn language_of(given: &str) -> String {
    let first = given.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    match first.as_str() {
        "eng" => "en".to_owned(),
        "rus" => "ru".to_owned(),
        "ukr" => "uk".to_owned(),
        _ => first,
    }
}

/// The Bible the operator reads when nothing typed says which: the first in use of the
/// session's language, or the first in the translations. `languages` are the Bibles in use, in
/// their order, by [`language_of`]; the answer is a place among them.
pub fn session_bible(session: &str, languages: &[String]) -> usize {
    languages.iter().position(|language| language == session).unwrap_or(0)
}

/// The Bible the operator reads a typed reference in, and the language the session goes on in.
/// `typed` are the languages its book's name was typed in (from `sp_bible::read_reference`):
/// the session's language when it is one of them and has a Bible in use (so "Пс 23", Russian
/// and Ukrainian alike, stays with whichever the operator was reading), else the first Bible in
/// use of any of them, else the first Bible of all. The session then goes on in the language
/// chosen, or, when none has a Bible, in the one typed.
pub fn typed_bible(typed: &[&str], session: &str, languages: &[String]) -> (usize, Option<String>) {
    if typed.is_empty() {
        return (session_bible(session, languages), None);
    }
    if typed.contains(&session)
        && let Some(at) = languages.iter().position(|language| language == session)
    {
        return (at, Some(session.to_owned()));
    }
    if let Some(at) = languages.iter().position(|language| typed.contains(&language.as_str())) {
        return (at, Some(languages[at].clone()));
    }
    let kept = if typed.contains(&session) { session } else { typed[0] };
    (0, Some(kept.to_owned()))
}

/// The alphabets words and Bibles are written in, for searching the Bibles in the alphabet of
/// the words first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
    Hebrew,
}

/// The alphabet most of the letters typed are in; `None` for no letters (only figures), or as
/// many of one as of another.
pub fn script_of_words(words: &str) -> Option<Script> {
    let mut counts = [0usize; 4];
    for c in words.chars().filter(|c| c.is_alphabetic()) {
        let script = match c as u32 {
            0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
            0x0590..=0x05FF | 0xFB1D..=0xFB4F => Script::Hebrew,
            _ if c.is_ascii_alphabetic() => Script::Latin,
            0x00C0..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
            _ => continue,
        };
        counts[script as usize] += 1;
    }
    let most = *counts.iter().max()?;
    let mut best = counts.iter().enumerate().filter(|(_, count)| **count == most && most > 0);
    let (at, _) = best.next()?;
    if best.next().is_some() {
        return None;
    }
    [Script::Latin, Script::Cyrillic, Script::Greek, Script::Hebrew].get(at).copied()
}

/// The alphabet a Bible's language is written in (by [`language_of`]); `None` when it gives no
/// language. Any language not known to be Cyrillic, Greek or Hebrew is taken as Latin.
pub fn script_of_language(language: &str) -> Option<Script> {
    match language {
        "" => None,
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" => Some(Script::Cyrillic),
        "el" | "grc" => Some(Script::Greek),
        "he" | "hbo" => Some(Script::Hebrew),
        _ => Some(Script::Latin),
    }
}

/// The Bibles in use to search for words, in order: those in the words' alphabet (Cyrillic
/// words: the Russian and the Ukrainian Bibles, as many words are spelt alike in both), the
/// session language's first, then in the translations' order. `None` when no Bible in use is in
/// that alphabet, or the words have none, for the caller to fall back to the translations'
/// order.
pub fn search_order(words: &str, session: &str, languages: &[String]) -> Option<Vec<usize>> {
    let script = script_of_words(words)?;
    let mut order: Vec<usize> =
        (0..languages.len()).filter(|at| script_of_language(&languages[*at]) == Some(script)).collect();
    if order.is_empty() {
        return None;
    }
    order.sort_by_key(|at| languages[*at] != session);
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn languages(list: &[&str]) -> Vec<String> {
        list.iter().map(|language| (*language).to_owned()).collect()
    }

    #[test]
    fn a_bibles_language_is_read_from_what_its_file_says() {
        assert_eq!(language_of("en"), "en");
        assert_eq!(language_of("RU"), "ru");
        assert_eq!(language_of("uk-UA"), "uk");
        assert_eq!(language_of("rus"), "ru");
        assert_eq!(language_of(""), "");
    }

    #[test]
    fn the_operators_bible_follows_the_language_typed() {
        // The Russian Synodal first, then the King James Version, as the app starts.
        let in_use = languages(&["ru", "en"]);
        assert_eq!(typed_bible(&["en"], "ru", &in_use), (1, Some("en".to_owned())), "English: the English Bible");
        assert_eq!(typed_bible(&["ru"], "en", &in_use), (0, Some("ru".to_owned())), "Russian: the Russian Bible");
        // Ukrainian with no Ukrainian Bible in use: the first Bible, and the session goes on in
        // Ukrainian (still with no Bible of its own).
        assert_eq!(typed_bible(&["uk"], "en", &in_use), (0, Some("uk".to_owned())));
        // Spelt alike in Russian and Ukrainian: the Russian Bible, the only one of the two.
        assert_eq!(typed_bible(&["ru", "uk"], "en", &in_use), (0, Some("ru".to_owned())));
        // With both, the one the session is reading; failing that, the first in the translations.
        let both = languages(&["en", "uk", "ru"]);
        assert_eq!(typed_bible(&["ru", "uk"], "ru", &both), (2, Some("ru".to_owned())));
        assert_eq!(typed_bible(&["ru", "uk"], "uk", &both), (1, Some("uk".to_owned())));
        assert_eq!(typed_bible(&["ru", "uk"], "en", &both), (1, Some("uk".to_owned())));
        // Only a Ukrainian Bible: a Russian-and-Ukrainian name lands on it.
        assert_eq!(typed_bible(&["ru", "uk"], "en", &languages(&["uk"])), (0, Some("uk".to_owned())));
        // Two English Bibles: the first of them.
        assert_eq!(typed_bible(&["en"], "en", &languages(&["ru", "en", "en"])).0, 1);
        // A Bible that gives no language is never chosen for one.
        assert_eq!(typed_bible(&["en"], "en", &languages(&["ru", ""])), (0, Some("en".to_owned())));
    }

    #[test]
    fn with_nothing_typed_the_sessions_language_decides() {
        let in_use = languages(&["ru", "en"]);
        assert_eq!(session_bible("en", &in_use), 1);
        assert_eq!(session_bible("ru", &in_use), 0);
        assert_eq!(session_bible("uk", &in_use), 0, "no Ukrainian Bible: the first");
        assert_eq!(session_bible("en", &languages(&["uk", "ru"])), 0);
        assert_eq!(typed_bible(&[], "en", &in_use), (1, None));
    }

    #[test]
    fn words_are_in_the_alphabet_most_of_their_letters_are() {
        assert_eq!(script_of_words("shepherd"), Some(Script::Latin));
        assert_eq!(script_of_words("Пастырь мой"), Some(Script::Cyrillic));
        assert_eq!(script_of_words("любов і віра"), Some(Script::Cyrillic));
        assert_eq!(script_of_words("Dieu amour à"), Some(Script::Latin));
        assert_eq!(script_of_words("λόγος"), Some(Script::Greek));
        assert_eq!(script_of_words("Jesus Иисус Христос"), Some(Script::Cyrillic));
        assert_eq!(script_of_words("3:16"), None);
        assert_eq!(script_of_words("ab аб"), None, "as many of each");
        assert_eq!(script_of_language("uk"), Some(Script::Cyrillic));
        assert_eq!(script_of_language("de"), Some(Script::Latin));
        assert_eq!(script_of_language(""), None);
    }

    #[test]
    fn words_are_searched_for_in_the_bibles_of_their_alphabet_first() {
        let in_use = languages(&["ru", "en", "uk", "en"]);
        assert_eq!(search_order("shepherd", "ru", &in_use), Some(vec![1, 3]));
        assert_eq!(search_order("любовь", "en", &in_use), Some(vec![0, 2]), "both Cyrillic Bibles, in order");
        assert_eq!(search_order("любов", "uk", &in_use), Some(vec![2, 0]), "the session's language first");
        // No Bible in that alphabet, or no letters: the caller keeps the translations' order.
        assert_eq!(search_order("λόγος", "en", &in_use), None);
        assert_eq!(search_order("3:16", "en", &in_use), None);
        assert_eq!(search_order("shepherd", "en", &languages(&["ru"])), None);
    }
}
