//! Reading the rows of a table out of an SQLite database file, and nothing else: no SQL, no
//! indexes, no writing. Written from the published description of the file
//! (sqlite.org/fileformat2.html), so that a songbook kept as an SQLite file is read the same
//! way on every system, with no SQLite on the computer.
//!
//! What it reads: the header (pages of 512 bytes to 65536, bytes kept back at the end of each
//! page, text as UTF-8 or UTF-16 either way round), the list of tables, a table's tree of pages
//! down to its rows, rows that run on into overflow pages, and each value (nothing, a whole
//! number of any size, a number with a fraction, text, bytes). A column declared
//! `INTEGER PRIMARY KEY` is the row's own number, as SQLite has it.
//!
//! What it refuses, with a sentence saying so: a `WITHOUT ROWID` table and a virtual table
//! (their rows are kept another way), a table with computed columns, and a file made for a
//! newer SQLite than the format described. It leaves alone: indexes, free pages, and a journal
//! or write-ahead log beside the file (only the main file is read, so changes not yet moved
//! into it are not seen). A column added to a table after its rows were written reads as
//! nothing in the older rows, whatever default it was given.
//!
//! The file is not trusted. Every place in it is checked before it is read, a page is never
//! visited twice in one walk (so a tree or a chain of overflow pages that loops, or shares
//! pages, is refused), a tree is at most [`DEEPEST`] levels, and the pages visited, the rows and
//! the bytes kept are bounded by [`Limits`]. Nothing here panics whatever the file holds; every
//! error is a sentence.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

/// How an SQLite file starts.
const MAGIC: &[u8; 16] = b"SQLite format 3\0";
/// The deepest tree of pages followed. SQLite itself makes none deeper.
const DEEPEST: usize = 20;
/// The most columns a table may declare (SQLite's own usual limit).
const MOST_COLUMNS: usize = 2000;

/// Whether the bytes start as an SQLite database does.
pub fn is_database(head: &[u8]) -> bool {
    head.starts_with(MAGIC)
}

/// One value of a row.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// One row: its own number in the table, and the values of the columns asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub rowid: i64,
    pub values: Vec<Value>,
}

/// How much may be read from one database.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The most pages one walk through a table may visit.
    pub pages: usize,
    /// The most rows one table may give.
    pub rows: usize,
    /// The most bytes of rows read from one table (the columns asked for and those before them).
    pub bytes: u64,
}

impl Limits {
    /// Room for the largest songbooks there are, many times over.
    pub const NORMAL: Limits = Limits { pages: 1 << 20, rows: 1_000_000, bytes: 512 * 1024 * 1024 };
}

/// How text is kept in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// One column of a table.
#[derive(Clone, Debug)]
struct Column {
    name: String,
    /// Declared `INTEGER PRIMARY KEY`: its value is the row's own number.
    is_rowid: bool,
}

/// One table the file lists.
#[derive(Clone, Debug)]
struct Table {
    name: String,
    /// The page its tree starts at, and its columns; or why it can't be read.
    shape: Result<(u32, Vec<Column>), String>,
}

/// An SQLite database file, open for reading tables.
pub struct Database<R> {
    source: R,
    page_size: usize,
    /// The bytes of each page that hold anything (the page less what is kept back at its end).
    usable: usize,
    /// How many whole pages the file has.
    pages: u32,
    /// The file's length.
    length: u64,
    encoding: Encoding,
    limits: Limits,
    tables: Vec<Table>,
}

fn be16(bytes: &[u8], at: usize) -> Option<usize> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(usize::from(u16::from_be_bytes([*pair.first()?, *pair.get(1)?])))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let four: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(four))
}

/// A number written in one to nine bytes, and the place after it.
fn varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for step in 0..9 {
        let byte = u64::from(*bytes.get(at.checked_add(step)?)?);
        if step == 8 {
            return Some(((value << 8) | byte, at + 9));
        }
        value = (value << 7) | (byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, at + step + 1));
        }
    }
    None
}

const DAMAGED: &str = "the database is damaged";

fn damaged(what: &str) -> String {
    format!("{DAMAGED} ({what})")
}

/// One page of a table's tree, part way through being gone over.
struct Frame {
    page: Vec<u8>,
    /// Where the page's own header starts (after the file's header, on page 1).
    header: usize,
    /// Where its list of cells ends.
    cells_end: usize,
    cells: usize,
    next: usize,
    leaf: bool,
}

/// One row as it lies in the file: the part of it on its page, and where the rest is.
struct Cell {
    rowid: i64,
    /// The length of the whole row.
    length: u64,
    /// As much of the row as has been read.
    data: Vec<u8>,
    /// The next overflow page holding more of it, or 0.
    overflow: u32,
}

/// One walk through a table, row by row in the table's order.
struct Walk<'a, R> {
    db: &'a mut Database<R>,
    visited: HashSet<u32>,
    stack: Vec<Frame>,
    /// Bytes of rows read so far.
    read: u64,
}

impl<R: Read + Seek> Walk<'_, R> {
    /// Reads a page, once: a page asked for again is a loop, or two rows sharing their bytes.
    fn page(&mut self, number: u32) -> Result<Vec<u8>, String> {
        if number == 0 || number > self.db.pages {
            return Err(format!(
                "the file is cut short or damaged (it points to page {number}, and has {} pages)",
                self.db.pages
            ));
        }
        if !self.visited.insert(number) {
            return Err(damaged(&format!("page {number} is used twice")));
        }
        if self.visited.len() > self.db.limits.pages {
            return Err("the database is larger than any songbook".to_owned());
        }
        self.db.read_page(number)
    }

    /// Starts on a page of the tree: the page a table starts at, or one a page above points to.
    fn enter(&mut self, number: u32) -> Result<(), String> {
        if self.stack.len() >= DEEPEST {
            return Err(damaged("its pages are nested too deep"));
        }
        let page = self.page(number)?;
        let header = if number == 1 { 100 } else { 0 };
        let usable = self.db.usable;
        let kind = page.get(header).copied().unwrap_or(0);
        let leaf = match kind {
            13 => true,
            5 => false,
            2 | 10 => return Err("its rows are kept as an index, which isn't read".to_owned()),
            _ => return Err(damaged(&format!("page {number} isn't part of a table"))),
        };
        let cells = be16(&page, header + 3).ok_or_else(|| damaged("a page is too short"))?;
        let cells_end = header + if leaf { 8 } else { 12 } + 2 * cells;
        if cells_end > usable {
            return Err(damaged(&format!("page {number} lists more rows than fit on it")));
        }
        self.stack.push(Frame { page, header, cells_end, cells, next: 0, leaf });
        Ok(())
    }

    /// The next row of the table, or `None` after the last.
    fn next(&mut self) -> Result<Option<Cell>, String> {
        loop {
            let usable = self.db.usable;
            let length = self.db.length;
            let Some(frame) = self.stack.last_mut() else { return Ok(None) };
            let at = frame.next;
            frame.next += 1;
            let bad = || damaged("a row lies outside its page");
            // Where the cell is: its place is in the list after the page's header.
            let cell_at = |frame: &Frame| -> Result<usize, String> {
                let list = frame.header + if frame.leaf { 8 } else { 12 };
                let place = be16(&frame.page, list + 2 * at).ok_or_else(bad)?;
                if place < frame.cells_end || place >= usable { Err(bad()) } else { Ok(place) }
            };
            if frame.leaf {
                if at >= frame.cells {
                    self.stack.pop();
                    continue;
                }
                let page = frame.page.get(..usable).ok_or_else(bad)?;
                let (row_length, after) = varint(page, cell_at(frame)?).ok_or_else(bad)?;
                let (rowid, after) = varint(page, after).ok_or_else(bad)?;
                // A row can't be longer than the file it is in.
                if row_length > length {
                    return Err("the file is cut short or damaged (a row is longer than the file)".to_owned());
                }
                let here = on_its_page(row_length, usable as u64);
                let end = after.checked_add(here).ok_or_else(bad)?;
                let data = page.get(after..end).ok_or_else(bad)?.to_vec();
                let overflow = if row_length > here as u64 { be32(page, end).ok_or_else(bad)? } else { 0 };
                return Ok(Some(Cell { rowid: rowid as i64, length: row_length, data, overflow }));
            }
            if at > frame.cells {
                self.stack.pop();
                continue;
            }
            // The pages below: one before each key, and one more after the last.
            let child = if at < frame.cells {
                let place = cell_at(frame)?;
                // The key after the page number has to be on the page too.
                varint(frame.page.get(..usable).ok_or_else(bad)?, place + 4).ok_or_else(bad)?;
                be32(&frame.page, place)
            } else {
                be32(&frame.page, frame.header + 8)
            }
            .ok_or_else(bad)?;
            if child == 1 {
                return Err(damaged("a table points back to the first page"));
            }
            self.enter(child)?;
        }
    }

    /// Counts bytes of rows read, up to the limit.
    fn count(&mut self, bytes: u64) -> Result<(), String> {
        self.read = self.read.saturating_add(bytes);
        if self.read > self.db.limits.bytes {
            return Err("the database holds more than any songbook".to_owned());
        }
        Ok(())
    }

    /// Reads on along the row's overflow pages until its first `need` bytes are in hand.
    fn fetch(&mut self, cell: &mut Cell, need: u64) -> Result<(), String> {
        if need > cell.length {
            return Err(damaged("a value runs past the end of its row"));
        }
        let have = cell.data.len() as u64;
        if need <= have {
            return Ok(());
        }
        self.count(need - have)?;
        let usable = self.db.usable;
        while (cell.data.len() as u64) < need {
            if cell.overflow == 0 {
                return Err(damaged("a row ends before its values do"));
            }
            if cell.overflow == 1 {
                return Err(damaged("a row runs on into the first page"));
            }
            let page = self.page(cell.overflow)?;
            let left = cell.length - cell.data.len() as u64;
            let take = left.min(usable as u64 - 4) as usize;
            let more = page.get(4..4 + take).ok_or_else(|| damaged("a page is too short"))?;
            cell.data.extend_from_slice(more);
            cell.overflow = be32(&page, 0).unwrap_or(0);
        }
        Ok(())
    }

    /// The values of the columns at `wanted` (their places in the table) of one row.
    fn values(&mut self, cell: &mut Cell, wanted: &[(usize, bool)]) -> Result<Vec<Value>, String> {
        let bad = || damaged("a row's list of values is wrong");
        self.count(cell.data.len() as u64)?;
        // The row starts with the length of its list of kinds, then the kind of each value.
        let (list_length, first) = varint(&cell.data, 0).ok_or_else(bad)?;
        if list_length < first as u64 {
            return Err(bad());
        }
        self.fetch(cell, list_length)?;
        let list = cell.data.get(..list_length as usize).ok_or_else(bad)?;
        let last_wanted = wanted.iter().map(|(place, _)| *place).max().unwrap_or(0);
        // Where each wanted value lies: its kind, its start and its length.
        let mut found: Vec<Option<(u64, u64, u64)>> = vec![None; wanted.len()];
        let (mut at, mut place, mut start) = (first, 0_usize, list_length);
        while at < list.len() && place <= last_wanted {
            let (kind, after) = varint(list, at).ok_or_else(bad)?;
            let length = match kind {
                0 | 8 | 9 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                5 => 6,
                6 | 7 => 8,
                10 | 11 => return Err(damaged("a value is of a kind no database file holds")),
                kind => (kind - 12) / 2,
            };
            for (slot, (wanted_place, _)) in found.iter_mut().zip(wanted) {
                if *wanted_place == place {
                    *slot = Some((kind, start, length));
                }
            }
            start = start.checked_add(length).ok_or_else(bad)?;
            (at, place) = (after, place + 1);
        }
        let need = found.iter().flatten().map(|(_, start, length)| start.saturating_add(*length)).max().unwrap_or(0);
        self.fetch(cell, need)?;
        let encoding = self.db.encoding;
        found
            .iter()
            .zip(wanted)
            .map(|(found, (_, is_rowid))| {
                // A column the row was written before reads as nothing.
                let Some((kind, start, length)) = *found else {
                    return Ok(if *is_rowid { Value::Integer(cell.rowid) } else { Value::Null });
                };
                let bytes = cell.data.get(start as usize..(start + length) as usize).ok_or_else(bad)?;
                Ok(match kind {
                    0 if *is_rowid => Value::Integer(cell.rowid),
                    0 => Value::Null,
                    8 => Value::Integer(0),
                    9 => Value::Integer(1),
                    7 => {
                        let number = f64::from_be_bytes(bytes.try_into().map_err(|_| bad())?);
                        // SQLite reads a stored not-a-number as nothing.
                        if number.is_nan() { Value::Null } else { Value::Real(number) }
                    }
                    1..=6 => {
                        // A whole number of 1, 2, 3, 4, 6 or 8 bytes, its sign in the first.
                        let mut number = if bytes.first().is_some_and(|b| b & 0x80 != 0) { -1_i64 } else { 0 };
                        for byte in bytes {
                            number = (number << 8) | i64::from(*byte);
                        }
                        Value::Integer(number)
                    }
                    kind if kind % 2 == 0 => Value::Blob(bytes.to_vec()),
                    _ => Value::Text(text(bytes, encoding)),
                })
            })
            .collect()
    }
}

/// How many bytes of a row of that length lie on its own page; the rest are on overflow pages.
fn on_its_page(length: u64, usable: u64) -> usize {
    let most = usable - 35;
    if length <= most {
        return length as usize;
    }
    let least = (usable - 12) * 32 / 255 - 23;
    let fitted = least + (length - least) % (usable - 4);
    (if fitted <= most { fitted } else { least }) as usize
}

/// Text as the file keeps it; bytes that aren't text in its encoding become the replacement
/// character, and half a UTF-16 unit left at the end is dropped.
fn text(bytes: &[u8], encoding: Encoding) -> String {
    let units =
        |read: fn([u8; 2]) -> u16| -> Vec<u16> { bytes.as_chunks::<2>().0.iter().map(|pair| read(*pair)).collect() };
    match encoding {
        Encoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Encoding::Utf16Le => String::from_utf16_lossy(&units(u16::from_le_bytes)),
        Encoding::Utf16Be => String::from_utf16_lossy(&units(u16::from_be_bytes)),
    }
}

impl<R: Read + Seek> Database<R> {
    /// Opens a database: checks its header and reads its list of tables.
    pub fn open(mut source: R, limits: Limits) -> Result<Self, String> {
        let unreadable = |e: std::io::Error| format!("it could not be read ({e})");
        let length = source.seek(SeekFrom::End(0)).map_err(unreadable)?;
        source.seek(SeekFrom::Start(0)).map_err(unreadable)?;
        let mut header = Vec::with_capacity(100);
        source.by_ref().take(100).read_to_end(&mut header).map_err(unreadable)?;
        if !is_database(&header) {
            return Err("it isn't an SQLite database".to_owned());
        }
        if header.len() < 100 {
            return Err("the file is cut short (it ends inside its header)".to_owned());
        }
        let page_size = match be16(&header, 16) {
            Some(1) => 65536,
            Some(size) if (512..=32768).contains(&size) && size.is_power_of_two() => size,
            _ => return Err(damaged("its page size isn't one a database has")),
        };
        // A file made for a later kind of SQLite file than 1 (plain) or 2 (with a write-ahead log).
        if header.get(19).is_some_and(|version| *version > 2) {
            return Err("it was made for a newer SQLite than SoftPresenter reads".to_owned());
        }
        // How much of a row stays on its page is worked out from these three, which never vary.
        if header.get(21..24) != Some(&[64, 32, 32]) {
            return Err(damaged("its header isn't one a database has"));
        }
        let kept_back = header.get(20).map_or(0, |b| usize::from(*b));
        let usable = page_size - kept_back;
        if usable < 480 {
            return Err(damaged("its pages have no room for rows"));
        }
        let encoding = match be32(&header, 56) {
            // A database nothing was ever put in says 0.
            Some(0 | 1) => Encoding::Utf8,
            Some(2) => Encoding::Utf16Le,
            Some(3) => Encoding::Utf16Be,
            _ => return Err(damaged("its text is in an encoding no database has")),
        };
        let pages = u32::try_from(length / page_size as u64).unwrap_or(u32::MAX);
        let mut db = Database { source, page_size, usable, pages, length, encoding, limits, tables: Vec::new() };
        db.tables = db.read_tables()?;
        Ok(db)
    }

    fn read_page(&mut self, number: u32) -> Result<Vec<u8>, String> {
        let unreadable = |e: std::io::Error| format!("it could not be read ({e})");
        let start = u64::from(number.saturating_sub(1)) * self.page_size as u64;
        self.source.seek(SeekFrom::Start(start)).map_err(unreadable)?;
        let mut page = vec![0_u8; self.page_size];
        self.source.read_exact(&mut page).map_err(unreadable)?;
        Ok(page)
    }

    /// Goes through the rows of the tree starting at `root`, reading the columns at `wanted`.
    fn walk(&mut self, root: u32, wanted: &[(usize, bool)]) -> Result<Vec<Row>, String> {
        let most = self.limits.rows;
        let mut walk = Walk { db: self, visited: HashSet::new(), stack: Vec::new(), read: 0 };
        walk.enter(root)?;
        let mut rows = Vec::new();
        while let Some(mut cell) = walk.next()? {
            if rows.len() >= most {
                return Err("the database has more rows than any songbook".to_owned());
            }
            let values = walk.values(&mut cell, wanted)?;
            rows.push(Row { rowid: cell.rowid, values });
        }
        Ok(rows)
    }

    /// The file's list of what it holds, kept as a table of its own starting on page 1: each
    /// table's name, the page it starts at, and the statement that made it (for its columns).
    fn read_tables(&mut self) -> Result<Vec<Table>, String> {
        if self.pages == 0 {
            return Err("the file is cut short (it ends inside its first page)".to_owned());
        }
        let listed = self.walk(1, &[(0, false), (1, false), (3, false), (4, false)])?;
        let pages = self.pages;
        Ok(listed
            .into_iter()
            .filter_map(|row| match row.values.as_slice() {
                [Value::Text(kind), Value::Text(name), root, sql] if kind == "table" => {
                    let shape = match (root, sql) {
                        (Value::Integer(root), Value::Text(sql)) => match u32::try_from(*root) {
                            Ok(root) if root >= 2 && root <= pages => columns(sql).map(|columns| (root, columns)),
                            Ok(0) => Err("it is a virtual table, which isn't read".to_owned()),
                            _ => Err(damaged("a table starts on a page the file doesn't have")),
                        },
                        _ => Err("it is a virtual table, which isn't read".to_owned()),
                    };
                    Some(Table { name: name.clone(), shape })
                }
                _ => None,
            })
            .collect())
    }

    fn table(&self, name: &str) -> Option<&Table> {
        // The name as written, else without regard to the case of English letters, as SQLite.
        self.tables
            .iter()
            .find(|table| table.name == name)
            .or_else(|| self.tables.iter().find(|table| table.name.eq_ignore_ascii_case(name)))
    }

    /// The names of the tables in the file, as they are written.
    pub fn table_names(&self) -> Vec<&str> {
        self.tables.iter().map(|table| table.name.as_str()).collect()
    }

    /// Whether the file has a table of exactly that name.
    pub fn has_table(&self, name: &str) -> bool {
        self.tables.iter().any(|table| table.name == name)
    }

    /// Which of two texts SQLite puts first when it sorts them: by their bytes as this file
    /// keeps them (so a UTF-16 file sorts by its UTF-16 bytes, not by UTF-8's).
    pub fn text_order(&self, a: &str, b: &str) -> std::cmp::Ordering {
        let bytes =
            |text: &str, write: fn(u16) -> [u8; 2]| -> Vec<u8> { text.encode_utf16().flat_map(write).collect() };
        match self.encoding {
            Encoding::Utf8 => a.as_bytes().cmp(b.as_bytes()),
            Encoding::Utf16Le => bytes(a, u16::to_le_bytes).cmp(&bytes(b, u16::to_le_bytes)),
            Encoding::Utf16Be => bytes(a, u16::to_be_bytes).cmp(&bytes(b, u16::to_be_bytes)),
        }
    }

    /// The names of a table's columns, in order.
    pub fn column_names(&self, table: &str) -> Result<Vec<String>, String> {
        let found = self.table(table).ok_or_else(|| format!("it has no table {table}"))?;
        let (_, columns) = found.shape.as_ref().map_err(|why| format!("table {table} can't be read: {why}"))?;
        Ok(columns.iter().map(|column| column.name.clone()).collect())
    }

    /// Every row of a table, in the table's own order (by the rows' numbers), with the values
    /// of the columns named, in the order named. Names are matched without regard to the case
    /// of English letters, as SQLite matches them.
    pub fn rows(&mut self, table: &str, columns: &[&str]) -> Result<Vec<Row>, String> {
        let found = self.table(table).ok_or_else(|| format!("it has no table {table}"))?;
        let (root, all) = found.shape.as_ref().map_err(|why| format!("table {table} can't be read: {why}"))?;
        let wanted = columns
            .iter()
            .map(|name| {
                all.iter()
                    .position(|column| column.name.eq_ignore_ascii_case(name))
                    .and_then(|place| Some((place, all.get(place)?.is_rowid)))
                    .ok_or_else(|| format!("table {table} has no column {name}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let root = *root;
        self.walk(root, &wanted)
    }
}

/// One piece of a `CREATE TABLE` statement.
#[derive(Debug, PartialEq)]
enum Piece {
    /// A bare word: a keyword, or a name written without quotes.
    Word(String),
    /// A name or a text in quotes of any kind.
    Quoted(String),
    Sign(char),
    Number,
}

impl Piece {
    fn is(&self, word: &str) -> bool {
        matches!(self, Piece::Word(w) if w.eq_ignore_ascii_case(word))
    }
}

/// The pieces of a statement, without its spaces and comments.
fn pieces(sql: &str) -> Vec<Piece> {
    let chars: Vec<char> = sql.chars().collect();
    let mut pieces = Vec::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        if c.is_whitespace() {
            at += 1;
        } else if c == '-' && next == Some('-') {
            while chars.get(at).is_some_and(|c| *c != '\n') {
                at += 1;
            }
        } else if c == '/' && next == Some('*') {
            at += 2;
            while at < chars.len() && !(chars.get(at) == Some(&'*') && chars.get(at + 1) == Some(&'/')) {
                at += 1;
            }
            at += 2;
        } else if matches!(c, '\'' | '"' | '`' | '[') {
            // Quoted: to the same quote again (a `[` to its `]`); the quote twice is itself.
            let close = if c == '[' { ']' } else { c };
            let mut inside = String::new();
            at += 1;
            while let Some(&c) = chars.get(at) {
                at += 1;
                if c == close {
                    if close != ']' && chars.get(at) == Some(&close) {
                        at += 1;
                    } else {
                        break;
                    }
                }
                inside.push(c);
            }
            pieces.push(Piece::Quoted(inside));
        } else if c.is_alphabetic() || c == '_' {
            let start = at;
            while chars.get(at).is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$')) {
                at += 1;
            }
            pieces.push(Piece::Word(chars.get(start..at).unwrap_or(&[]).iter().collect()));
        } else if c.is_ascii_digit() {
            while chars.get(at).is_some_and(|c| c.is_alphanumeric() || *c == '.') {
                at += 1;
            }
            pieces.push(Piece::Number);
        } else {
            pieces.push(Piece::Sign(c));
            at += 1;
        }
    }
    pieces
}

/// A table's columns, from the statement that made the table: their names in order, and which
/// one, if any, is the row's own number.
fn columns(sql: &str) -> Result<Vec<Column>, String> {
    let unclear = || "how it was made isn't a statement SoftPresenter can follow".to_owned();
    let pieces = pieces(sql);
    if pieces.iter().take(3).any(|piece| piece.is("VIRTUAL")) {
        return Err("it is a virtual table, which isn't read".to_owned());
    }
    let open = pieces.iter().position(|piece| *piece == Piece::Sign('(')).ok_or_else(unclear)?;
    // The definitions between the brackets, split at the commas that aren't inside brackets.
    let mut definitions: Vec<&[Piece]> = Vec::new();
    let (mut depth, mut start, mut close) = (0_usize, open + 1, None);
    for (at, piece) in pieces.iter().enumerate().skip(open) {
        match piece {
            Piece::Sign('(') => depth += 1,
            Piece::Sign(')') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    definitions.push(pieces.get(start..at).unwrap_or(&[]));
                    close = Some(at);
                    break;
                }
            }
            Piece::Sign(',') if depth == 1 => {
                definitions.push(pieces.get(start..at).unwrap_or(&[]));
                start = at + 1;
            }
            _ => {}
        }
    }
    let close = close.ok_or_else(unclear)?;
    let after = pieces.get(close + 1..).unwrap_or(&[]);
    if after.windows(2).any(|pair| matches!(pair, [a, b] if a.is("WITHOUT") && b.is("ROWID"))) {
        return Err("it is a WITHOUT ROWID table, which isn't read".to_owned());
    }
    const CONSTRAINTS: [&str; 10] =
        ["CONSTRAINT", "PRIMARY", "NOT", "NULL", "UNIQUE", "CHECK", "DEFAULT", "COLLATE", "REFERENCES", "GENERATED"];
    let mut columns: Vec<(Column, bool)> = Vec::new();
    // A key the table names after its columns: `PRIMARY KEY (one_column)`.
    let mut table_key: Option<String> = None;
    for definition in definitions {
        let first = definition.first().ok_or_else(unclear)?;
        if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"].iter().any(|word| first.is(word)) {
            let key = definition.iter().position(|piece| piece.is("PRIMARY"));
            if let Some(key) = key.filter(|key| definition.get(key + 1).is_some_and(|piece| piece.is("KEY"))) {
                let named: Vec<&Piece> = definition
                    .iter()
                    .skip(key + 2)
                    .filter(|piece| matches!(piece, Piece::Word(_) | Piece::Quoted(_)))
                    .collect();
                let more = definition.iter().skip(key + 2).any(|piece| *piece == Piece::Sign(','));
                if let (false, Some(Piece::Word(name) | Piece::Quoted(name))) = (more, named.first()) {
                    table_key = Some(name.clone());
                }
            }
            continue;
        }
        let (Piece::Word(name) | Piece::Quoted(name)) = first else { return Err(unclear()) };
        // Its type: the words after its name, up to the first that says something else of it.
        let rest = definition.get(1..).unwrap_or(&[]);
        let type_words: Vec<&str> = rest
            .iter()
            .take_while(|piece| !CONSTRAINTS.iter().any(|word| piece.is(word)) && !piece.is("AS"))
            .map_while(|piece| match piece {
                Piece::Word(word) | Piece::Quoted(word) => Some(word.as_str()),
                _ => None,
            })
            .collect();
        let whole_number = matches!(type_words.as_slice(), [one] if one.eq_ignore_ascii_case("INTEGER"));
        // Only what the column says of itself, not what is inside brackets (a CHECK, a default).
        let mut depth = 0_usize;
        let outside: Vec<&Piece> = rest
            .iter()
            .filter(|piece| {
                match piece {
                    Piece::Sign('(') => depth += 1,
                    Piece::Sign(')') => depth = depth.saturating_sub(1),
                    _ => return depth == 0,
                }
                false
            })
            .collect();
        if outside.iter().any(|piece| piece.is("GENERATED") || piece.is("AS")) {
            return Err("it has computed columns, which aren't read".to_owned());
        }
        // `INTEGER PRIMARY KEY` is the row's own number; with DESC after it, SQLite keeps the
        // value in the row like any other.
        let key = outside.windows(2).position(|pair| matches!(pair, [a, b] if a.is("PRIMARY") && b.is("KEY")));
        let is_rowid =
            whole_number && key.is_some_and(|key| !outside.get(key + 2).is_some_and(|piece| piece.is("DESC")));
        columns.push((Column { name: name.clone(), is_rowid }, whole_number));
        if columns.len() > MOST_COLUMNS {
            return Err("it has more columns than any table".to_owned());
        }
    }
    if columns.is_empty() {
        return Err(unclear());
    }
    Ok(columns
        .into_iter()
        .map(|(mut column, whole_number)| {
            let keyed = table_key.as_ref().is_some_and(|key| key.eq_ignore_ascii_case(&column.name));
            column.is_rowid |= keyed && whole_number;
            column
        })
        .collect())
}

#[cfg(test)]
mod tests;
