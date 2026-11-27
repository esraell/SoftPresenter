//! Why a file of songs couldn't be read: what the SQLite reader (`sqlite_read`), SoftProjector's
//! songbooks, the song document reader (`pro_file`) and SoftPresenter's own song files refuse a
//! file with. It is kept as what it is, not as a sentence, so that the app can say it in the
//! language its window speaks (`sp-app`'s `load/reasons.rs` gives each its key and its words),
//! while the log and `sp` keep the English, which is what `Display` writes.

use std::fmt;

/// Why a file of songs couldn't be read. `Display` writes it in English as the end of a
/// sentence, in lower case with no full stop: "the database is damaged (page 7 is used twice)".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The system couldn't read the file: its own words.
    NotRead(String),

    // An SQLite database.
    /// It doesn't start as an SQLite database does.
    NotDatabase,
    /// The file ends inside its header.
    EndsInHeader,
    /// The file ends inside its first page.
    EndsInFirstPage,
    /// A page past the file's end is pointed to: that page, and how many pages the file has.
    PastTheEnd { page: u32, pages: u32 },
    /// A row is longer than the whole file.
    RowLongerThanFile,
    /// Made for a newer SQLite than the one described.
    NewerSqlite,
    /// Damaged, in the way said.
    Damaged(Damage),
    /// The table's rows are kept as an index.
    IndexRows,
    /// More pages than the limit lets one walk through a table visit.
    TooManyPages,
    /// More bytes of rows than the limit.
    TooMuch,
    /// More rows than the limit.
    TooManyRows,
    /// No table of that name.
    NoTable(String),
    /// The table has no column of that name.
    NoColumn { table: String, column: String },
    /// That table can't be read, and why.
    Table { table: String, why: Box<Reason> },
    /// A virtual table, whose rows are kept another way.
    VirtualTable,
    /// A `WITHOUT ROWID` table, whose rows are kept another way.
    WithoutRowid,
    /// A table with computed columns.
    ComputedColumns,
    /// A table made by a statement the reader can't follow.
    UnclearTable,
    /// A table with more columns than any.
    TooManyColumns,

    // SoftProjector's songbooks.
    /// A songbook saved as XML.
    XmlSongbook,
    /// Larger than any songbook: a songbook kept as text, or what a file's songs come to.
    TextTooLarge,
    /// More songs than any songbook.
    TooManySongs,
    /// One of its songs has more parts than any song has.
    SongTooLong,

    // Song documents (`.pro`).
    /// Larger than a song document is.
    DocumentTooLarge,
    /// Damaged, or not a song document.
    DocumentDamaged,
    /// No slides in it.
    NoSlides,
    /// Neither words nor pictures in it.
    NoWordsOrPictures,
    /// Pictures or videos, but no words.
    OnlyPictures,

    // SoftPresenter's own song files.
    /// Not JSON at all: what the JSON reader said.
    NotJson(String),
    /// JSON, but not a song file.
    NotSongFile,
    /// It doesn't say which version it is.
    NoVersion,
    /// Made by a newer SoftPresenter: its version, and the newest this one reads.
    NewerSongFile { version: u64, reads: u32 },
    /// A song file that doesn't read as one: what the JSON reader said.
    SongFileDamaged(String),
}

/// How an SQLite database is damaged: the part of "the database is damaged (…)" in brackets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    PageSize,
    Header,
    NoRoomForRows,
    Encoding,
    PageUsedTwice(u32),
    NestedTooDeep,
    NotTablePage(u32),
    PageTooShort,
    TooManyCells(u32),
    RowOutsidePage,
    PointsToFirstPage,
    ValuePastRow,
    RowEndsEarly,
    RowIntoFirstPage,
    ValueList,
    UnknownKind,
    TableOutsideFile,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = match self {
            Reason::NotRead(said) => return write!(f, "it could not be read ({said})"),
            Reason::NotDatabase => "it isn't an SQLite database",
            Reason::EndsInHeader => "the file is cut short (it ends inside its header)",
            Reason::EndsInFirstPage => "the file is cut short (it ends inside its first page)",
            Reason::PastTheEnd { page, pages } => {
                let noun = if *pages == 1 { "page" } else { "pages" };
                return write!(
                    f,
                    "the file is cut short or damaged (it points to page {page}, and has {pages} {noun})"
                );
            }
            Reason::RowLongerThanFile => "the file is cut short or damaged (a row is longer than the file)",
            Reason::NewerSqlite => "it was made for a newer SQLite than SoftPresenter reads",
            Reason::Damaged(damage) => return write!(f, "the database is damaged ({damage})"),
            Reason::IndexRows => "its rows are kept as an index, which isn't read",
            Reason::TooManyPages => "the database is larger than any songbook",
            Reason::TooMuch => "the database holds more than any songbook",
            Reason::TooManyRows => "the database has more rows than any songbook",
            Reason::NoTable(table) => return write!(f, "it has no table {table}"),
            Reason::NoColumn { table, column } => return write!(f, "table {table} has no column {column}"),
            Reason::Table { table, why } => return write!(f, "table {table} can't be read: {why}"),
            Reason::VirtualTable => "it is a virtual table, which isn't read",
            Reason::WithoutRowid => "it is a WITHOUT ROWID table, which isn't read",
            Reason::ComputedColumns => "it has computed columns, which aren't read",
            Reason::UnclearTable => "how it was made isn't a statement SoftPresenter can follow",
            Reason::TooManyColumns => "it has more columns than any table",
            Reason::XmlSongbook => "it is a songbook saved as XML, which SoftPresenter doesn't read yet",
            Reason::TextTooLarge => "it is larger than any songbook",
            Reason::TooManySongs => "it has more songs than any songbook",
            Reason::SongTooLong => "one of its songs is too long to be a song",
            Reason::DocumentTooLarge => "the file is too large to be a song",
            Reason::DocumentDamaged => "the file is damaged or isn't a song document",
            Reason::NoSlides => "it has no slides in it, or isn't a song document",
            Reason::NoWordsOrPictures => "it has no words or pictures in it, or isn't a song document",
            Reason::OnlyPictures => "it has pictures or videos but no words, and those don't come across yet",
            Reason::NotJson(said) => return write!(f, "this is not a SoftPresenter song file ({said})"),
            Reason::NotSongFile => "this is not a SoftPresenter song file",
            Reason::NoVersion => "the song file does not say which version it is",
            Reason::NewerSongFile { version, reads } => {
                return write!(
                    f,
                    "the song file was made by a newer SoftPresenter (format {version}; this one reads up to {reads})"
                );
            }
            Reason::SongFileDamaged(said) => return write!(f, "the song file is damaged ({said})"),
        };
        f.write_str(plain)
    }
}

impl fmt::Display for Damage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = match self {
            Damage::PageSize => "its page size isn't one a database has",
            Damage::Header => "its header isn't one a database has",
            Damage::NoRoomForRows => "its pages have no room for rows",
            Damage::Encoding => "its text is in an encoding no database has",
            Damage::PageUsedTwice(page) => return write!(f, "page {page} is used twice"),
            Damage::NestedTooDeep => "its pages are nested too deep",
            Damage::NotTablePage(page) => return write!(f, "page {page} isn't part of a table"),
            Damage::PageTooShort => "a page is too short",
            Damage::TooManyCells(page) => return write!(f, "page {page} lists more rows than fit on it"),
            Damage::RowOutsidePage => "a row lies outside its page",
            Damage::PointsToFirstPage => "a table points back to the first page",
            Damage::ValuePastRow => "a value runs past the end of its row",
            Damage::RowEndsEarly => "a row ends before its values do",
            Damage::RowIntoFirstPage => "a row runs on into the first page",
            Damage::ValueList => "a row's list of values is wrong",
            Damage::UnknownKind => "a value is of a kind no database file holds",
            Damage::TableOutsideFile => "a table starts on a page the file doesn't have",
        };
        f.write_str(plain)
    }
}

/// One of each reason, for the app's tests that every one has its words in every language and
/// says the same English as `Display`. A reason added to [`Reason`] or [`Damage`] belongs here
/// too: the match at the end doesn't build until it is named there, and the test beside it
/// counts the kinds.
#[cfg(any(test, feature = "test-documents"))]
#[doc(hidden)]
pub fn one_of_each() -> Vec<Reason> {
    let damages = [
        Damage::PageSize,
        Damage::Header,
        Damage::NoRoomForRows,
        Damage::Encoding,
        Damage::PageUsedTwice(7),
        Damage::NestedTooDeep,
        Damage::NotTablePage(12),
        Damage::PageTooShort,
        Damage::TooManyCells(3),
        Damage::RowOutsidePage,
        Damage::PointsToFirstPage,
        Damage::ValuePastRow,
        Damage::RowEndsEarly,
        Damage::RowIntoFirstPage,
        Damage::ValueList,
        Damage::UnknownKind,
        Damage::TableOutsideFile,
    ];
    let mut all = vec![
        Reason::NotRead("Input/output error (os error 5)".to_owned()),
        Reason::NotDatabase,
        Reason::EndsInHeader,
        Reason::EndsInFirstPage,
        Reason::PastTheEnd { page: 160, pages: 12 },
        Reason::PastTheEnd { page: 2, pages: 1 },
        Reason::PastTheEnd { page: 9, pages: 3 },
        Reason::RowLongerThanFile,
        Reason::NewerSqlite,
        Reason::IndexRows,
        Reason::TooManyPages,
        Reason::TooMuch,
        Reason::TooManyRows,
        Reason::NoTable("Songs".to_owned()),
        Reason::NoColumn { table: "Songs".to_owned(), column: "song_text".to_owned() },
        Reason::Table { table: "Songs".to_owned(), why: Box::new(Reason::VirtualTable) },
        Reason::Table { table: "Songbooks".to_owned(), why: Box::new(Reason::Damaged(Damage::TableOutsideFile)) },
        Reason::VirtualTable,
        Reason::WithoutRowid,
        Reason::ComputedColumns,
        Reason::UnclearTable,
        Reason::TooManyColumns,
        Reason::XmlSongbook,
        Reason::TextTooLarge,
        Reason::TooManySongs,
        Reason::SongTooLong,
        Reason::DocumentTooLarge,
        Reason::DocumentDamaged,
        Reason::NoSlides,
        Reason::NoWordsOrPictures,
        Reason::OnlyPictures,
        Reason::NotJson("expected value at line 1 column 1".to_owned()),
        Reason::NotSongFile,
        Reason::NoVersion,
        Reason::NewerSongFile { version: 9, reads: 4 },
        Reason::SongFileDamaged("missing field `groups`".to_owned()),
    ];
    all.extend(damages.into_iter().map(Reason::Damaged));
    // Every kind is here: a kind left out of the lists above fails to build in this match.
    for reason in &all {
        match reason {
            Reason::NotRead(_)
            | Reason::NotDatabase
            | Reason::EndsInHeader
            | Reason::EndsInFirstPage
            | Reason::PastTheEnd { .. }
            | Reason::RowLongerThanFile
            | Reason::NewerSqlite
            | Reason::IndexRows
            | Reason::TooManyPages
            | Reason::TooMuch
            | Reason::TooManyRows
            | Reason::NoTable(_)
            | Reason::NoColumn { .. }
            | Reason::Table { .. }
            | Reason::VirtualTable
            | Reason::WithoutRowid
            | Reason::ComputedColumns
            | Reason::UnclearTable
            | Reason::TooManyColumns
            | Reason::XmlSongbook
            | Reason::TextTooLarge
            | Reason::TooManySongs
            | Reason::SongTooLong
            | Reason::DocumentTooLarge
            | Reason::DocumentDamaged
            | Reason::NoSlides
            | Reason::NoWordsOrPictures
            | Reason::OnlyPictures
            | Reason::NotJson(_)
            | Reason::NotSongFile
            | Reason::NoVersion
            | Reason::NewerSongFile { .. }
            | Reason::SongFileDamaged(_) => {}
            Reason::Damaged(
                Damage::PageSize
                | Damage::Header
                | Damage::NoRoomForRows
                | Damage::Encoding
                | Damage::PageUsedTwice(_)
                | Damage::NestedTooDeep
                | Damage::NotTablePage(_)
                | Damage::PageTooShort
                | Damage::TooManyCells(_)
                | Damage::RowOutsidePage
                | Damage::PointsToFirstPage
                | Damage::ValuePastRow
                | Damage::RowEndsEarly
                | Damage::RowIntoFirstPage
                | Damage::ValueList
                | Damage::UnknownKind
                | Damage::TableOutsideFile,
            ) => {}
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_of_each_has_every_kind_once_at_least() {
        let all = one_of_each();
        let kinds: std::collections::HashSet<String> = all
            .iter()
            .map(|reason| match reason {
                Reason::Damaged(damage) => format!("damage {:?}", std::mem::discriminant(damage)),
                other => format!("reason {:?}", std::mem::discriminant(other)),
            })
            .collect();
        // 33 kinds of reason besides Damaged, and 17 kinds of damage.
        assert_eq!(kinds.len(), 33 + 17, "{all:#?}");
        assert!(all.iter().all(|reason| !reason.to_string().is_empty()));
    }
}
