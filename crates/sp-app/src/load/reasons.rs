//! Why a file of songs couldn't be read, in the language set: each reason the readers give
//! (`sp_model::Reason`, whose `Display` is the English) with its key and its words, and the
//! import's own reasons ([`Why`]). The window says them in its language; `sp` and the log keep
//! the English ([`Why::english`], and `i18n::english_of` for a sentence already said).

use crate::i18n::{Language, t, tf, tn, with_language};
use sp_model::{Damage, Reason};

/// Why a file of many songs gave none, or a place couldn't be looked in, kept as what it is
/// until it is said, so the window says it in the language set and `sp` in English.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// Not a SoftProjector songbook SoftPresenter can read.
    NotSongbook,
    /// A songbook with no songs in it.
    NoSongs,
    /// A zip with no songbook in it SoftPresenter can read.
    NoSongbookInZip,
    /// A songbook in a zip that unpacks to more than any.
    ZipTooMuch,
    /// A damaged zip.
    ZipDamaged,
    /// A place SoftPresenter isn't allowed to look in.
    NotAllowed,
    /// The system couldn't open the file or look in the place: its own words.
    System(String),
    /// What the songbook's reader said.
    Reader(Reason),
}

impl Why {
    /// In the language set, as the end of a sentence.
    pub fn words(&self) -> String {
        match self {
            Why::NotSongbook => {
                t("notices.import.why.not_songbook", "it isn't a SoftProjector songbook SoftPresenter can read")
                    .to_owned()
            }
            Why::NoSongs => t("notices.import.why.no_songs", "it has no songs in it").to_owned(),
            Why::NoSongbookInZip => t(
                "notices.import.why.zip_no_songbook",
                "it has no SoftProjector songbook in it that SoftPresenter can read",
            )
            .to_owned(),
            Why::ZipTooMuch => t("notices.import.why.zip_too_much", "it unpacks to more than any songbook").to_owned(),
            Why::ZipDamaged => t("notices.import.zip_damaged", "the zip is damaged").to_owned(),
            Why::NotAllowed if cfg!(target_os = "macos") => t(
                "settings.find.denied.mac",
                "SoftPresenter isn\u{2019}t allowed to look there. Allow it in System Settings \u{203a} Privacy & Security \u{203a} Files and Folders",
            )
            .to_owned(),
            Why::NotAllowed => t("settings.find.denied", "SoftPresenter isn\u{2019}t allowed to look there").to_owned(),
            Why::System(said) => said.clone(),
            Why::Reader(reason) => words(reason),
        }
    }

    /// In English, for `sp` and the log.
    pub fn english(&self) -> String {
        with_language(Language::English, || self.words())
    }
}

/// A reader's reason in the language set, as the end of a sentence: "the database is damaged
/// (page 7 is used twice)". Numbers that name something (a page, a format) are written as they
/// are; a count is written as the language writes numbers.
pub fn words(reason: &Reason) -> String {
    let fixed = |words: &str| words.to_owned();
    match reason {
        Reason::NotRead(said) => {
            tf("notices.import.reason.not_read", "it could not be read ({error})", &[("error", said)])
        }
        Reason::NotDatabase => fixed(t("notices.import.reason.not_database", "it isn't an SQLite database")),
        Reason::EndsInHeader => {
            fixed(t("notices.import.reason.ends_in_header", "the file is cut short (it ends inside its header)"))
        }
        Reason::EndsInFirstPage => fixed(t(
            "notices.import.reason.ends_in_first_page",
            "the file is cut short (it ends inside its first page)",
        )),
        Reason::PastTheEnd { page, pages } => tn(
            "notices.import.reason.past_the_end",
            u64::from(*pages),
            "the file is cut short or damaged (it points to page {page}, and has {n} page)",
            "the file is cut short or damaged (it points to page {page}, and has {n} pages)",
            &[("page", &page.to_string())],
        ),
        Reason::RowLongerThanFile => fixed(t(
            "notices.import.reason.row_longer_than_file",
            "the file is cut short or damaged (a row is longer than the file)",
        )),
        Reason::NewerSqlite => {
            fixed(t("notices.import.reason.newer_sqlite", "it was made for a newer SQLite than SoftPresenter reads"))
        }
        Reason::Damaged(damage) => tf(
            "notices.import.reason.damaged",
            "the database is damaged ({what})",
            &[("what", damage_words(*damage).as_str())],
        ),
        Reason::IndexRows => {
            fixed(t("notices.import.reason.index_rows", "its rows are kept as an index, which isn't read"))
        }
        Reason::TooManyPages => {
            fixed(t("notices.import.reason.too_many_pages", "the database is larger than any songbook"))
        }
        Reason::TooMuch => fixed(t("notices.import.reason.too_much", "the database holds more than any songbook")),
        Reason::TooManyRows => {
            fixed(t("notices.import.reason.too_many_rows", "the database has more rows than any songbook"))
        }
        Reason::NoTable(table) => tf("notices.import.reason.no_table", "it has no table {table}", &[("table", table)]),
        Reason::NoColumn { table, column } => tf(
            "notices.import.reason.no_column",
            "table {table} has no column {column}",
            &[("table", table), ("column", column)],
        ),
        Reason::Table { table, why } => tf(
            "notices.import.reason.table",
            "table {table} can't be read: {why}",
            &[("table", table), ("why", &words(why))],
        ),
        Reason::VirtualTable => {
            fixed(t("notices.import.reason.virtual_table", "it is a virtual table, which isn't read"))
        }
        Reason::WithoutRowid => {
            fixed(t("notices.import.reason.without_rowid", "it is a WITHOUT ROWID table, which isn't read"))
        }
        Reason::ComputedColumns => {
            fixed(t("notices.import.reason.computed_columns", "it has computed columns, which aren't read"))
        }
        Reason::UnclearTable => fixed(t(
            "notices.import.reason.unclear_table",
            "how it was made isn't a statement SoftPresenter can follow",
        )),
        Reason::TooManyColumns => {
            fixed(t("notices.import.reason.too_many_columns", "it has more columns than any table"))
        }
        Reason::XmlSongbook => fixed(t(
            "notices.import.reason.xml_songbook",
            "it is a songbook saved as XML, which SoftPresenter doesn't read yet",
        )),
        Reason::TextTooLarge => fixed(t("notices.import.reason.text_too_large", "it is larger than any songbook")),
        Reason::TooManySongs => fixed(t("notices.import.reason.too_many_songs", "it has more songs than any songbook")),
        Reason::SongTooLong => {
            fixed(t("notices.import.reason.song_too_long", "one of its songs is too long to be a song"))
        }
        Reason::DocumentTooLarge => {
            fixed(t("notices.import.reason.document_too_large", "the file is too large to be a song"))
        }
        Reason::DocumentDamaged => {
            fixed(t("notices.import.reason.document_damaged", "the file is damaged or isn't a song document"))
        }
        Reason::NoSlides => {
            fixed(t("notices.import.reason.no_slides", "it has no slides in it, or isn't a song document"))
        }
        Reason::NoWordsOrPictures => fixed(t(
            "notices.import.reason.no_words_or_pictures",
            "it has no words or pictures in it, or isn't a song document",
        )),
        Reason::OnlyPictures => fixed(t(
            "notices.import.reason.only_pictures",
            "it has pictures or videos but no words, and those don't come across yet",
        )),
        Reason::NotJson(said) => {
            tf("notices.import.reason.not_json", "this is not a SoftPresenter song file ({error})", &[("error", said)])
        }
        Reason::NotSongFile => fixed(t("notices.import.reason.not_song_file", "this is not a SoftPresenter song file")),
        Reason::NoVersion => {
            fixed(t("notices.import.reason.no_version", "the song file does not say which version it is"))
        }
        Reason::NewerSongFile { version, reads } => tf(
            "notices.import.reason.newer_song_file",
            "the song file was made by a newer SoftPresenter (format {version}; this one reads up to {reads})",
            &[("version", &version.to_string()), ("reads", &reads.to_string())],
        ),
        Reason::SongFileDamaged(said) => {
            tf("notices.import.reason.song_file_damaged", "the song file is damaged ({error})", &[("error", said)])
        }
    }
}

/// How a database is damaged, for the brackets of "the database is damaged (…)".
fn damage_words(damage: Damage) -> String {
    let page = |page: u32| page.to_string();
    match damage {
        Damage::PageSize => t("notices.import.damage.page_size", "its page size isn't one a database has").to_owned(),
        Damage::Header => t("notices.import.damage.header", "its header isn't one a database has").to_owned(),
        Damage::NoRoomForRows => {
            t("notices.import.damage.no_room_for_rows", "its pages have no room for rows").to_owned()
        }
        Damage::Encoding => {
            t("notices.import.damage.encoding", "its text is in an encoding no database has").to_owned()
        }
        Damage::PageUsedTwice(at) => {
            tf("notices.import.damage.page_used_twice", "page {page} is used twice", &[("page", &page(at))])
        }
        Damage::NestedTooDeep => t("notices.import.damage.nested_too_deep", "its pages are nested too deep").to_owned(),
        Damage::NotTablePage(at) => {
            tf("notices.import.damage.not_table_page", "page {page} isn't part of a table", &[("page", &page(at))])
        }
        Damage::PageTooShort => t("notices.import.damage.page_too_short", "a page is too short").to_owned(),
        Damage::TooManyCells(at) => tf(
            "notices.import.damage.too_many_cells",
            "page {page} lists more rows than fit on it",
            &[("page", &page(at))],
        ),
        Damage::RowOutsidePage => t("notices.import.damage.row_outside_page", "a row lies outside its page").to_owned(),
        Damage::PointsToFirstPage => {
            t("notices.import.damage.points_to_first_page", "a table points back to the first page").to_owned()
        }
        Damage::ValuePastRow => {
            t("notices.import.damage.value_past_row", "a value runs past the end of its row").to_owned()
        }
        Damage::RowEndsEarly => t("notices.import.damage.row_ends_early", "a row ends before its values do").to_owned(),
        Damage::RowIntoFirstPage => {
            t("notices.import.damage.row_into_first_page", "a row runs on into the first page").to_owned()
        }
        Damage::ValueList => t("notices.import.damage.value_list", "a row's list of values is wrong").to_owned(),
        Damage::UnknownKind => {
            t("notices.import.damage.unknown_kind", "a value is of a kind no database file holds").to_owned()
        }
        Damage::TableOutsideFile => {
            t("notices.import.damage.table_outside_file", "a table starts on a page the file doesn't have").to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::english_of;

    /// The names a translation keeps as they are.
    const KEPT: [&str; 5] = ["WITHOUT ROWID", "SoftPresenter", "SoftProjector", "SQLite", "XML"];

    /// What a reason was filled with (table and column names, the system's own words), which
    /// stay as they are in every language.
    fn filled_with(reason: &Reason) -> Vec<String> {
        match reason {
            Reason::NotRead(said) | Reason::NotJson(said) | Reason::SongFileDamaged(said) | Reason::NoTable(said) => {
                vec![said.clone()]
            }
            Reason::NoColumn { table, column } => vec![table.clone(), column.clone()],
            Reason::Table { table, why } => {
                let mut all = vec![table.clone()];
                all.extend(filled_with(why));
                all
            }
            _ => Vec::new(),
        }
    }

    #[test]
    fn every_reason_says_the_same_english_as_the_reader() {
        for reason in sp_model::reasons_one_of_each() {
            assert_eq!(with_language(Language::English, || words(&reason)), reason.to_string(), "{reason:?}");
            assert_eq!(Why::Reader(reason.clone()).english(), reason.to_string());
        }
    }

    #[test]
    fn every_reason_has_its_words_in_every_language_with_no_english_left_in_them() {
        // Words English sentences are made of, which Spanish and German have none of.
        const ENGLISH: [&str; 12] =
            ["the", "is", "it", "its", "has", "and", "of", "which", "isn't", "can't", "doesn't", "file"];
        for reason in sp_model::reasons_one_of_each() {
            let english = reason.to_string();
            for language in [Language::Spanish, Language::Russian, Language::Ukrainian, Language::German] {
                let said = with_language(language, || words(&reason));
                assert_ne!(said, english, "{language:?} has no words for {reason:?}");
                assert!(!said.contains('{') && !said.contains('}'), "{language:?}: {said}");
                // The log and `sp` get the English back from the words said.
                assert_eq!(english_of(&said), english, "{language:?}: {said}");
                let mut rest = said.clone();
                for kept in filled_with(&reason).iter().map(String::as_str).chain(KEPT) {
                    rest = rest.replace(kept, " ");
                }
                // Spanish and German are written in Latin letters too: no English words in them.
                if matches!(language, Language::Spanish | Language::German) {
                    let english_words: Vec<&str> = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '\'')
                        .filter(|word| ENGLISH.contains(&word.to_lowercase().as_str()))
                        .collect();
                    assert!(english_words.is_empty(), "English in the {language:?}: {said} ({english_words:?})");
                } else {
                    assert!(!rest.chars().any(|c| c.is_ascii_alphabetic()), "English in the {language:?}: {said}");
                }
            }
        }
    }

    #[test]
    fn a_count_of_pages_takes_its_plural_form() {
        let past = |pages| Reason::PastTheEnd { page: 160, pages };
        with_language(Language::Russian, || {
            assert_eq!(
                [1, 3, 12, 21].map(|pages| words(&past(pages))),
                [
                    "файл обрезан или повреждён (он ссылается на страницу 160, а в нём 1 страница)",
                    "файл обрезан или повреждён (он ссылается на страницу 160, а в нём 3 страницы)",
                    "файл обрезан или повреждён (он ссылается на страницу 160, а в нём 12 страниц)",
                    "файл обрезан или повреждён (он ссылается на страницу 160, а в нём 21 страница)",
                ]
            );
        });
        with_language(Language::English, || {
            assert_eq!(words(&past(1)), "the file is cut short or damaged (it points to page 160, and has 1 page)");
        });
    }

    #[test]
    fn the_imports_own_reasons_are_said_in_the_language_set_and_in_english_for_sp() {
        for why in
            [Why::NotSongbook, Why::NoSongs, Why::NoSongbookInZip, Why::ZipTooMuch, Why::ZipDamaged, Why::NotAllowed]
        {
            let english = why.english();
            for language in [Language::Spanish, Language::Russian, Language::Ukrainian, Language::German] {
                let said = with_language(language, || why.words());
                assert_ne!(said, english, "{language:?} {why:?}");
                assert_eq!(with_language(language, || why.english()), english);
            }
        }
        let system = Why::System("Permission denied (os error 13)".to_owned());
        assert_eq!(with_language(Language::Russian, || system.words()), "Permission denied (os error 13)");
    }
}
