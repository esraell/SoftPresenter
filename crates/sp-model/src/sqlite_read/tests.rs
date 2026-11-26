//! The reader against databases SQLite itself made (`tests/songbooks`, made by `make.sh` there),
//! against small ones put together here byte by byte, and against both of those damaged on
//! purpose.

use std::io::Cursor;

use super::*;

const DATABASE: &[u8] = include_bytes!("../../tests/songbooks/database.sqlite");
const SONGBOOK: &[u8] = include_bytes!("../../tests/songbooks/songbook.sps");
const UTF16_LE: &[u8] = include_bytes!("../../tests/songbooks/utf16le.sps");
const UTF16_BE: &[u8] = include_bytes!("../../tests/songbooks/utf16be.sps");

/// Opens a database, with the reason a refusal gives in English.
fn open(bytes: &[u8]) -> Result<Database<Cursor<&[u8]>>, String> {
    Database::open(Cursor::new(bytes), Limits::NORMAL).map_err(|why| why.to_string())
}

fn text_of(value: &Value) -> &str {
    match value {
        Value::Text(text) => text,
        other => panic!("{other:?} isn't text"),
    }
}

#[test]
fn a_database_sqlite_made_gives_its_tables_columns_and_rows() {
    let mut db = open(DATABASE).unwrap();
    assert_eq!(db.table_names(), ["Songbooks", "sqlite_sequence", "Songs"]);
    assert!(db.has_table("Songbooks") && !db.has_table("songbooks"));
    assert_eq!(db.column_names("songbooks").unwrap(), ["id", "name", "info"]);
    // The id is the row's own number, kept nowhere in the row.
    let books = db.rows("Songbooks", &["id", "name"]).unwrap();
    let ids: Vec<&Value> = books.iter().map(|row| &row.values[0]).collect();
    assert_eq!(ids, [&Value::Integer(1), &Value::Integer(2), &Value::Integer(4), &Value::Integer(5)]);
    assert_eq!(books[0].rowid, 1);
    assert_eq!(text_of(&books[1].values[1]), "Ранкові пісні");
    assert_eq!(books[3].values[1], Value::Null);
    // Three levels of pages, in the rows' order; names in any case, columns in any order.
    let songs = db.rows("songs", &["TITLE", "id", "number", "song_text", "added_later"]).unwrap();
    assert_eq!(songs.len(), 226);
    assert!(songs.windows(2).all(|pair| pair[0].rowid < pair[1].rowid));
    assert_eq!(songs.iter().map(|row| row.rowid).sum::<i64>(), 29720);
    assert!(songs.iter().all(|row| row.values[1] == Value::Integer(row.rowid)));
    // A song many pages long comes whole.
    let long = songs.iter().find(|row| row.values[2] == Value::Integer(600)).unwrap();
    assert_eq!(text_of(&long.values[3]).chars().count(), 7583);
    assert!(text_of(&long.values[3]).ends_with("Line 150 of the long song goes on and on\n"));
    // Numbers as they were kept: text that isn't a number, and a number with a fraction.
    assert!(songs.iter().any(|row| row.values[2] == Value::Text("12a".to_owned())));
    assert!(songs.iter().any(|row| row.values[2] == Value::Real(7.5)));
    assert!(songs.iter().any(|row| row.values[2] == Value::Integer(-3)));
    // A column added after a row was written reads as nothing there, and as written after.
    assert_eq!(songs[0].values[4], Value::Null);
    assert_eq!(songs.last().unwrap().values[4], Value::Text("y".to_owned()));
    assert!(db.rows("Songs", &["no_such"]).unwrap_err().to_string().contains("no column no_such"));
    assert!(db.rows("Nothing", &["a"]).unwrap_err().to_string().contains("no table Nothing"));
}

#[test]
fn only_as_much_of_a_row_is_read_as_the_columns_asked_for_need() {
    // The long song has 5000 bytes of picture after its words: asking for the words stays
    // inside a limit the picture would break.
    let tight = Limits { bytes: 10 * 1024, ..Limits::NORMAL };
    let mut db = Database::open(Cursor::new(SONGBOOK), tight).unwrap();
    let songs = db.rows("Songs", &["number", "song_text"]).unwrap();
    assert_eq!(songs.len(), 7);
    assert_eq!(text_of(&songs[6].values[1]).chars().count(), 6143);
    assert!(db.rows("Songs", &["background"]).unwrap_err().to_string().contains("more than any songbook"));
    let mut db = open(SONGBOOK).unwrap();
    let pictures = db.rows("Songs", &["background"]).unwrap();
    assert_eq!(pictures[6].values[0], Value::Blob(vec![0; 5000]));
    assert_eq!(pictures[0].values[0], Value::Null);
}

#[test]
fn text_kept_as_utf16_reads_the_same_either_way_round() {
    for bytes in [UTF16_LE, UTF16_BE] {
        let mut db = open(bytes).unwrap();
        assert_eq!(text_of(&db.rows("SongBook", &["title"]).unwrap()[0].values[0]), "Пісні 🎵 UTF-16");
        let songs = db.rows("Songs", &["title", "song_text"]).unwrap();
        assert_eq!(text_of(&songs[0].values[0]), "Друга");
        assert_eq!(text_of(&songs[0].values[1]), "Куплет 1\nСонце встає 🎵");
        assert!(text_of(&songs[4].values[1]).ends_with("Рядок 40 довгої пісні\n"));
    }
    // Sorted as SQLite sorts them: by the bytes in the file.
    use std::cmp::Ordering::{Greater, Less};
    assert_eq!(open(UTF16_LE).unwrap().text_order("ȁ", "Ă"), Less);
    assert_eq!(open(UTF16_BE).unwrap().text_order("ȁ", "Ă"), Greater);
    assert_eq!(open(DATABASE).unwrap().text_order("ȁ", "Ă"), Greater);
}

// ---- Databases put together here ----

/// How a database made here is laid out.
#[derive(Clone, Copy)]
struct Make {
    page_size: usize,
    kept_back: usize,
    /// 1 for UTF-8, 2 and 3 for UTF-16.
    encoding: u32,
}

const PLAIN: Make = Make { page_size: 512, kept_back: 0, encoding: 1 };

fn write_varint(value: u64) -> Vec<u8> {
    if value >> 56 != 0 {
        let mut bytes: Vec<u8> = (0..8).map(|i| ((value >> (57 - 7 * i)) as u8 & 0x7f) | 0x80).collect();
        bytes.push(value as u8);
        return bytes;
    }
    let mut bytes = vec![(value & 0x7f) as u8];
    let mut rest = value >> 7;
    while rest != 0 {
        bytes.insert(0, (rest & 0x7f) as u8 | 0x80);
        rest >>= 7;
    }
    bytes
}

/// A row's bytes: the list of kinds, then the values.
fn record(values: &[Value], make: Make) -> Vec<u8> {
    let (mut kinds, mut body) = (Vec::new(), Vec::new());
    for value in values {
        let (kind, bytes): (u64, Vec<u8>) = match value {
            Value::Null => (0, Vec::new()),
            Value::Integer(0) => (8, Vec::new()),
            Value::Integer(1) => (9, Vec::new()),
            Value::Integer(n) if i8::try_from(*n).is_ok() => (1, vec![*n as u8]),
            Value::Integer(n) if i16::try_from(*n).is_ok() => (2, (*n as i16).to_be_bytes().to_vec()),
            Value::Integer(n) if (-(1 << 23)..(1 << 23)).contains(n) => (3, n.to_be_bytes()[5..].to_vec()),
            Value::Integer(n) if i32::try_from(*n).is_ok() => (4, (*n as i32).to_be_bytes().to_vec()),
            Value::Integer(n) if (-(1_i64 << 47)..(1 << 47)).contains(n) => (5, n.to_be_bytes()[2..].to_vec()),
            Value::Integer(n) => (6, n.to_be_bytes().to_vec()),
            Value::Real(x) => (7, x.to_be_bytes().to_vec()),
            Value::Blob(bytes) => (12 + 2 * bytes.len() as u64, bytes.clone()),
            Value::Text(text) => {
                let bytes: Vec<u8> = match make.encoding {
                    2 => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                    3 => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                    _ => text.as_bytes().to_vec(),
                };
                (13 + 2 * bytes.len() as u64, bytes)
            }
        };
        kinds.extend(write_varint(kind));
        body.extend(bytes);
    }
    // The list's length counts its own bytes.
    let mut length = kinds.len() + 1;
    if write_varint(length as u64).len() > 1 {
        length += 1;
    }
    let mut row = write_varint(length as u64);
    row.extend(kinds);
    row.extend(body);
    row
}

/// One row's cell on a page of rows: the whole row when it fits, else its start and the page
/// the rest is on.
fn row_cell(rowid: i64, row: &[u8], make: Make, overflow: u32) -> Vec<u8> {
    let here = on_its_page(row.len() as u64, (make.page_size - make.kept_back) as u64);
    let mut cell = write_varint(row.len() as u64);
    cell.extend(write_varint(rowid as u64));
    cell.extend(&row[..here]);
    if here < row.len() {
        cell.extend(overflow.to_be_bytes());
    }
    cell
}

/// The overflow pages for the rest of a row, the first numbered `first`.
fn overflow_pages(row: &[u8], make: Make, first: u32) -> Vec<Vec<u8>> {
    let usable = make.page_size - make.kept_back;
    let here = on_its_page(row.len() as u64, usable as u64);
    let chunks: Vec<&[u8]> = row[here..].chunks(usable - 4).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(at, chunk)| {
            let next = if at + 1 < chunks.len() { first + at as u32 + 1 } else { 0 };
            let mut page = next.to_be_bytes().to_vec();
            page.extend(*chunk);
            page.resize(make.page_size, 0);
            page
        })
        .collect()
}

/// A cell on a page of pages: the page below, and the last row number on it.
fn child_cell(child: u32, key: i64) -> Vec<u8> {
    let mut cell = child.to_be_bytes().to_vec();
    cell.extend(write_varint(key as u64));
    cell
}

/// A page of a table: of rows (`right` is `None`) or of pages below (`right` is the last).
fn page(make: Make, number: u32, cells: &[Vec<u8>], right: Option<u32>) -> Vec<u8> {
    let mut page = vec![0_u8; make.page_size];
    let header = if number == 1 { 100 } else { 0 };
    page[header] = if right.is_some() { 5 } else { 13 };
    page[header + 3..header + 5].copy_from_slice(&(cells.len() as u16).to_be_bytes());
    let list = header + if right.is_some() { 12 } else { 8 };
    if let Some(right) = right {
        page[header + 8..header + 12].copy_from_slice(&right.to_be_bytes());
    }
    let mut end = make.page_size - make.kept_back;
    for (at, cell) in cells.iter().enumerate() {
        end -= cell.len();
        page[end..end + cell.len()].copy_from_slice(cell);
        page[list + 2 * at..list + 2 * at + 2].copy_from_slice(&(end as u16).to_be_bytes());
    }
    page[header + 5..header + 7].copy_from_slice(&(end as u16).to_be_bytes());
    page
}

/// The first page: the file's header, and the list of tables (name, first page, statement).
fn first_page(make: Make, tables: &[(&str, i64, &str)]) -> Vec<u8> {
    let cells: Vec<Vec<u8>> = tables
        .iter()
        .enumerate()
        .map(|(at, (name, root, sql))| {
            let text = |text: &str| Value::Text(text.to_owned());
            let row = record(&[text("table"), text(name), text(name), Value::Integer(*root), text(sql)], make);
            row_cell(at as i64 + 1, &row, make, 0)
        })
        .collect();
    let mut first = page(make, 1, &cells, None);
    first[..16].copy_from_slice(MAGIC);
    let size = if make.page_size == 65536 { 1 } else { make.page_size as u16 };
    first[16..18].copy_from_slice(&size.to_be_bytes());
    first[18..24].copy_from_slice(&[1, 1, make.kept_back as u8, 64, 32, 32]);
    first[56..60].copy_from_slice(&make.encoding.to_be_bytes());
    first
}

fn rows_cells(make: Make, rows: &[Vec<Value>]) -> Vec<Vec<u8>> {
    rows.iter().enumerate().map(|(at, row)| row_cell(at as i64 + 1, &record(row, make), make, 0)).collect()
}

/// A database of one table, `t (a, b)`, its rows on page 2.
fn one_table(make: Make, rows: &[Vec<Value>]) -> Vec<u8> {
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
    file.extend(page(make, 2, &rows_cells(make, rows), None));
    file
}

fn word(text: &str) -> Value {
    Value::Text(text.to_owned())
}

/// A table of pages below pages: page 2 points to pages 3 and 4, which hold the rows.
fn two_levels() -> Vec<u8> {
    let make = PLAIN;
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
    file.extend(page(make, 2, &[child_cell(3, 1)], Some(4)));
    file.extend(page(make, 3, &[row_cell(1, &record(&[word("one"), Value::Integer(1)], make), make, 0)], None));
    file.extend(page(make, 4, &[row_cell(2, &record(&[word("two"), Value::Integer(2)], make), make, 0)], None));
    file
}

/// A table with one row that runs on over two overflow pages (pages 3 and 4).
fn with_overflow() -> (Vec<u8>, String) {
    let make = PLAIN;
    let long = "long ".repeat(250);
    let row = record(&[word(&long), Value::Integer(7)], make);
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
    file.extend(page(make, 2, &[row_cell(1, &row, make, 3)], None));
    let overflow = overflow_pages(&row, make, 3);
    assert!(overflow.len() >= 2);
    file.extend(overflow.concat());
    (file, long)
}

fn read(file: &[u8]) -> Result<Vec<Row>, String> {
    open(file)?.rows("t", &["a", "b"]).map_err(|why| why.to_string())
}

#[test]
fn every_kind_of_value_page_size_and_encoding_reads_back() {
    let rows = vec![
        vec![Value::Null, Value::Integer(0)],
        vec![Value::Integer(1), Value::Integer(-1)],
        vec![Value::Integer(127), Value::Integer(-128)],
        vec![Value::Integer(32767), Value::Integer(-32768)],
        vec![Value::Integer(8_388_607), Value::Integer(-8_388_608)],
        vec![Value::Integer(i64::from(i32::MAX)), Value::Integer(i64::from(i32::MIN))],
        vec![Value::Integer((1 << 47) - 1), Value::Integer(-(1 << 47))],
        vec![Value::Integer(i64::MAX), Value::Integer(i64::MIN)],
        vec![Value::Real(1.5), Value::Real(-0.25)],
        vec![word("Свет 🎵"), Value::Blob(vec![0, 255, 7])],
        vec![word(""), Value::Blob(Vec::new())],
    ];
    for page_size in [512, 1024, 4096, 32768, 65536] {
        for kept_back in [0, 12, 32] {
            for encoding in [1, 2, 3] {
                let make = Make { page_size, kept_back, encoding };
                let got = read(&one_table(make, &rows)).unwrap();
                let got: Vec<Vec<Value>> = got.into_iter().map(|row| row.values).collect();
                assert_eq!(got, rows, "{page_size} {kept_back} {encoding}");
            }
        }
    }
    // A number that isn't one reads as nothing, as SQLite reads it; a row shorter than its
    // table reads as nothing in the columns it hasn't.
    let odd = one_table(PLAIN, &[vec![Value::Real(f64::NAN)], vec![]]);
    let got = read(&odd).unwrap();
    assert_eq!(got[0].values, [Value::Null, Value::Null]);
    assert_eq!(got[1].values, [Value::Null, Value::Null]);
    // Half a UTF-16 unit at the end of a text is dropped.
    let mut half = one_table(Make { encoding: 2, ..PLAIN }, &[vec![Value::Blob(vec![0x41, 0, 0x42])]]);
    let kind = half.iter().rposition(|byte| *byte == 12 + 2 * 3).unwrap();
    half[kind] = 13 + 2 * 3;
    assert_eq!(read(&half).unwrap()[0].values[0], word("A"));
}

#[test]
fn pages_below_pages_and_rows_over_several_pages_read_back() {
    let got = read(&two_levels()).unwrap();
    assert_eq!(got.iter().map(|row| row.rowid).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(got[1].values, [word("two"), Value::Integer(2)]);
    let (file, long) = with_overflow();
    assert_eq!(read(&file).unwrap()[0].values, [word(&long), Value::Integer(7)]);
    // With bytes kept back at the end of each page, too.
    let make = Make { page_size: 1024, kept_back: 40, encoding: 3 };
    let long = "довга ".repeat(600);
    let row = record(&[word(&long), Value::Integer(7)], make);
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
    file.extend(page(make, 2, &[row_cell(1, &row, make, 3)], None));
    file.extend(overflow_pages(&row, make, 3).concat());
    assert_eq!(read(&file).unwrap()[0].values, [word(&long), Value::Integer(7)]);
}

#[test]
fn the_rows_own_number_stands_for_an_integer_primary_key() {
    let is_rowid = |sql: &str| -> Vec<bool> { columns(sql).unwrap().iter().map(|column| column.is_rowid).collect() };
    let names = |sql: &str| -> Vec<String> { columns(sql).unwrap().into_iter().map(|column| column.name).collect() };
    let sql = "CREATE TABLE 'Songs' ('id' INTEGER PRIMARY KEY  AUTOINCREMENT  NOT NULL , 'songbook_id' INTEGER, \
               'category' INTEGER DEFAULT 0, 'use_private' BOOL, 'background' BLOB)";
    assert_eq!(names(sql), ["id", "songbook_id", "category", "use_private", "background"]);
    assert_eq!(is_rowid(sql), [true, false, false, false, false]);
    assert_eq!(is_rowid("create table t (a text, b integer primary key)"), [false, true]);
    assert_eq!(is_rowid("CREATE TABLE t (a INTEGER, b, PRIMARY KEY (a))"), [true, false]);
    assert_eq!(is_rowid("CREATE TABLE t (a INTEGER, b, CONSTRAINT k PRIMARY KEY (\"a\" DESC))"), [true, false]);
    // Not the row's number: another type, a key of two columns, and SQLite's own exception.
    assert_eq!(is_rowid("CREATE TABLE t (a INT PRIMARY KEY, b)"), [false, false]);
    assert_eq!(is_rowid("CREATE TABLE t (a INTEGER, b, PRIMARY KEY (a, b))"), [false, false]);
    assert_eq!(is_rowid("CREATE TABLE t (a INTEGER PRIMARY KEY DESC, b)"), [false, false]);
    // Names in every kind of quote, comments, types with brackets, and constraints with commas.
    let sql = "CREATE TABLE IF NOT EXISTS \"main\".[t] -- one\n (`a b` VARCHAR(10, 2) DEFAULT 'x,y', /* two */ \
               [c,d] NUMERIC CHECK (\"c,d\" IN (1, 2)), \"e\"\"f\" TEXT, g, UNIQUE (g, \"e\"\"f\"), FOREIGN KEY (g) REFERENCES u (a, b))";
    assert_eq!(names(sql), ["a b", "c,d", "e\"f", "g"]);
    // Tables whose rows are kept another way are refused, in words.
    for (sql, why) in [
        ("CREATE TABLE t (a PRIMARY KEY, b) WITHOUT ROWID", "WITHOUT ROWID"),
        ("CREATE TABLE t (a, b) STRICT, without rowid", "WITHOUT ROWID"),
        ("CREATE VIRTUAL TABLE t USING fts5 (a, b)", "virtual"),
        ("CREATE TABLE t (a, b GENERATED ALWAYS AS (a + 1))", "computed"),
        ("CREATE TABLE t (a, b INT AS (a + 1) STORED)", "computed"),
        ("CREATE TABLE t", "can follow"),
        ("CREATE TABLE t (a, (b)", "can follow"),
        ("CREATE TABLE t ()", "can follow"),
        ("", "can follow"),
    ] {
        assert!(columns(sql).unwrap_err().to_string().contains(why), "{sql}");
    }
    // In a file: the value kept is nothing, and the row's number is read in its place.
    let make = PLAIN;
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a INTEGER PRIMARY KEY, b)")]);
    let cells = vec![
        row_cell(41, &record(&[Value::Null, word("x")], make), make, 0),
        row_cell(-5, &record(&[], make), make, 0),
    ];
    file.extend(page(make, 2, &cells, None));
    let got = read(&file).unwrap();
    assert_eq!(got[0].values, [Value::Integer(41), word("x")]);
    assert_eq!((got[1].rowid, &got[1].values), (-5, &vec![Value::Integer(-5), Value::Null]));
    // A table that can't be read doesn't stop the others being read.
    let mut file = first_page(
        make,
        &[("w", 3, "CREATE TABLE w (a PRIMARY KEY) WITHOUT ROWID"), ("t", 2, "CREATE TABLE t (a, b)")],
    );
    file.extend(page(make, 2, &rows_cells(make, &[vec![word("kept"), Value::Null]]), None));
    file.extend(page(make, 3, &[], None));
    let mut db = open(&file).unwrap();
    assert_eq!(db.rows("t", &["a"]).unwrap()[0].values, [word("kept")]);
    assert!(db.rows("w", &["a"]).unwrap_err().to_string().contains("WITHOUT ROWID"));
}

// ---- Damaged on purpose ----

/// Every refusal is a sentence's worth of words, never a crash.
fn refused(file: &[u8]) -> String {
    let why = read(file).expect_err("a damaged database was read");
    assert!(why.len() > 10 && !why.contains("panicked"), "{why}");
    why
}

fn set32(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn set16(file: &mut [u8], at: usize, value: u16) {
    file[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn a_header_that_is_wrong_is_refused() {
    let good = one_table(PLAIN, &[vec![word("a"), Value::Integer(1)]]);
    assert_eq!(read(&good).unwrap().len(), 1);
    assert!(refused(b"").contains("isn't an SQLite database"));
    assert!(refused(b"PK\x03\x04 a zip").contains("isn't an SQLite database"));
    assert!(refused(&good[..60]).contains("cut short"));
    assert!(refused(&good[..100]).contains("cut short"));
    // Page sizes no database has: not a power of two, too small, nothing at all.
    for size in [0_u16, 2, 3, 256, 500, 1000, 1023, 0xffff] {
        let mut file = good.clone();
        set16(&mut file, 16, size);
        assert!(refused(&file).contains("page size"), "{size}");
    }
    // A page size other than the file's own: the pages aren't where it says.
    for size in [1024_u16, 4096, 1] {
        let mut file = good.clone();
        set16(&mut file, 16, size);
        refused(&file);
    }
    let mut file = good.clone();
    file[20] = 40;
    assert!(refused(&file).contains("no room for rows"));
    let mut file = good.clone();
    file[19] = 3;
    assert!(refused(&file).contains("newer SQLite"));
    let mut file = good.clone();
    file[21] = 65;
    assert!(refused(&file).contains("header"));
    let mut file = good.clone();
    set32(&mut file, 56, 4);
    assert!(refused(&file).contains("encoding"));
}

#[test]
fn a_file_cut_short_anywhere_is_refused_or_read_in_part_but_never_crashes() {
    for whole in [two_levels(), with_overflow().0] {
        let all = read(&whole).unwrap();
        for length in 0..whole.len() {
            match read(&whole[..length]) {
                Ok(rows) => assert!(rows.len() <= all.len()),
                Err(why) => assert!(why.len() > 10, "{why}"),
            }
        }
        // Without its last page, it says the file is short.
        assert!(refused(&whole[..whole.len() - 512]).contains("cut short"));
    }
}

#[test]
fn pages_that_point_in_circles_or_share_pages_are_refused() {
    let page_at = |number: usize| (number - 1) * 512;
    // A page of pages that points to itself, and one that points back up to where it came from.
    let mut file = two_levels();
    set32(&mut file, page_at(2) + 8, 2);
    assert!(refused(&file).contains("used twice"));
    let mut file = two_levels();
    file[page_at(4)] = 5;
    set16(&mut file, page_at(4) + 3, 0);
    set32(&mut file, page_at(4) + 8, 2);
    assert!(refused(&file).contains("used twice"));
    // Two places pointing to the same page, with no circle: that page's rows would come twice.
    let mut file = two_levels();
    set32(&mut file, page_at(2) + 8, 3);
    assert!(refused(&file).contains("used twice"));
    // Pointing to the first page, to no page, and past the end of the file.
    for (to, why) in [(1, "first page"), (0, "has 4 pages"), (5, "has 4 pages"), (u32::MAX, "has 4 pages")] {
        let mut file = two_levels();
        set32(&mut file, page_at(2) + 8, to);
        assert!(refused(&file).contains(why), "{to}");
    }
    // The table itself starting on a page the file hasn't, or on the first.
    for root in [0, 1, 9, -4, i64::MAX] {
        let mut file = first_page(PLAIN, &[("t", root, "CREATE TABLE t (a, b)")]);
        file.extend(page(PLAIN, 2, &[], None));
        refused(&file);
    }
    // A chain of overflow pages that comes back to itself, or to the row's own page.
    let (good, _) = with_overflow();
    for to in [3, 2] {
        let mut file = good.clone();
        set32(&mut file, page_at(3), to);
        assert!(refused(&file).contains("used twice"), "{to}");
    }
    // One that stops early, runs into the first page, or off the end.
    for (to, why) in [(0, "ends before"), (1, "first page"), (77, "has 4 pages")] {
        let mut file = good.clone();
        set32(&mut file, page_at(3), to);
        assert!(refused(&file).contains(why), "{to}");
    }
}

#[test]
fn pages_nested_too_deep_are_refused() {
    // Each page points to the next, thirty deep, with rows only on the last.
    let make = PLAIN;
    let deep = |levels: u32| {
        let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
        for level in 0..levels {
            file.extend(page(make, 2 + level, &[], Some(3 + level)));
        }
        file.extend(page(make, 2 + levels, &rows_cells(make, &[vec![word("deep"), Value::Null]]), None));
        file
    };
    assert_eq!(read(&deep(DEEPEST as u32 - 1)).unwrap().len(), 1);
    assert!(refused(&deep(DEEPEST as u32)).contains("too deep"));
    assert!(refused(&deep(30)).contains("too deep"));
}

#[test]
fn rows_that_lie_about_where_or_how_long_they_are_are_refused() {
    let page_at = |number: usize| (number - 1) * 512;
    let good = one_table(PLAIN, &[vec![word("first"), Value::Integer(1)], vec![word("second"), Value::Integer(2)]]);
    let list = page_at(2) + 8;
    // A row's place outside the page, inside the page's own header, or right at its end.
    for place in [0_u16, 4, 9, 511, 512, 600, 0xffff] {
        let mut file = good.clone();
        set16(&mut file, list, place);
        assert!(refused(&file).contains("outside its page"), "{place}");
    }
    // More rows listed than the page has room to list.
    let mut file = good.clone();
    set16(&mut file, page_at(2) + 3, 400);
    assert!(refused(&file).contains("more rows than fit"));
    let mut file = good.clone();
    set16(&mut file, page_at(2) + 3, 0xffff);
    assert!(refused(&file).contains("more rows than fit"));
    // A page that isn't a table's, and an index where a table should be.
    for (kind, why) in [(0_u8, "isn't part of a table"), (1, "isn't part of a table"), (200, "isn't part of a table")] {
        let mut file = good.clone();
        file[page_at(2)] = kind;
        assert!(refused(&file).contains(why), "{kind}");
    }
    for kind in [2_u8, 10] {
        let mut file = good.clone();
        file[page_at(2)] = kind;
        assert!(refused(&file).contains("index"), "{kind}");
    }
    // A row that says it is longer than the page, than the file, than anything.
    let place = usize::from(u16::from_be_bytes([good[list], good[list + 1]])) + page_at(2);
    for length in [200_u64, 5000, 1 << 40, u64::MAX] {
        let mut file = good.clone();
        let written = write_varint(length);
        // Room is made for the longer number by moving the row's place back.
        let start = place + 1 - written.len();
        file[start..=place].copy_from_slice(&written);
        set16(&mut file, list, (start - page_at(2)) as u16);
        refused(&file);
    }
    // Numbers of nine bytes where a small one belongs: the row's own number may be anything.
    let make = PLAIN;
    let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
    file.extend(page(make, 2, &[row_cell(i64::MIN, &record(&[word("far"), Value::Null], make), make, 0)], None));
    assert_eq!(read(&file).unwrap()[0].rowid, i64::MIN);
    // A row cut off in the middle of a nine-byte number at the very end of the page.
    let mut file = good.clone();
    file[page_at(3) - 3..page_at(3)].copy_from_slice(&[0xff, 0xff, 0xff]);
    set16(&mut file, list, 509);
    assert!(refused(&file).contains("outside its page"));
}

#[test]
fn a_rows_list_of_values_that_is_wrong_is_refused() {
    let make = PLAIN;
    let with_row = |row: Vec<u8>| {
        let mut file = first_page(make, &[("t", 2, "CREATE TABLE t (a, b)")]);
        file.extend(page(make, 2, &[row_cell(1, &row, make, 0)], None));
        file
    };
    // Kinds no file holds.
    for kind in [10_u8, 11] {
        assert!(refused(&with_row(vec![3, kind, 1, 0])).contains("kind no database file holds"), "{kind}");
    }
    // A list longer than the row, a list of no length, a row of no bytes at all.
    assert!(refused(&with_row(vec![9, 1, 1])).contains("past the end of its row"));
    assert!(refused(&with_row(vec![0, 1, 1])).contains("list of values"));
    assert!(refused(&with_row(Vec::new())).contains("list of values"));
    // A text said to be longer than the row, and one of a length no computer holds.
    assert!(refused(&with_row(vec![2, 13 + 2 * 50, b'a'])).contains("past the end of its row"));
    let mut huge = vec![10];
    huge.extend(write_varint(u64::MAX));
    assert!(refused(&with_row(huge.clone())).contains("past the end of its row"));
    // Two such, whose lengths together don't fit in a number.
    huge.extend(write_varint(u64::MAX));
    huge[0] = 19;
    refused(&with_row(huge));
    // A kind's number cut off at the end of the list.
    assert!(refused(&with_row(vec![3, 1, 0x81, 5])).contains("list of values"));
}

#[test]
fn the_limits_hold() {
    let rows: Vec<Vec<Value>> = (0..20).map(|n| vec![word("row"), Value::Integer(n)]).collect();
    let file = one_table(PLAIN, &rows);
    let with = |limits: Limits| {
        Database::open(Cursor::new(&file[..]), limits)
            .and_then(|mut db| db.rows("t", &["a"]))
            .map_err(|why| why.to_string())
    };
    assert_eq!(with(Limits::NORMAL).unwrap().len(), 20);
    assert_eq!(with(Limits { rows: 20, ..Limits::NORMAL }).unwrap().len(), 20);
    assert!(with(Limits { rows: 19, ..Limits::NORMAL }).unwrap_err().contains("more rows"));
    assert!(with(Limits { pages: 0, ..Limits::NORMAL }).unwrap_err().contains("larger than any songbook"));
    assert!(with(Limits { bytes: 100, ..Limits::NORMAL }).unwrap_err().contains("more than any songbook"));
    // A long row isn't read at all once it would pass the limit.
    let (file, _) = with_overflow();
    let tight = Limits { bytes: 600, ..Limits::NORMAL };
    assert!(Database::open(Cursor::new(&file[..]), tight).unwrap().rows("t", &["a"]).is_err());
}

/// Numbers that look random and are the same every run.
struct Dice(u64);

impl Dice {
    fn roll(&mut self, below: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % below.max(1) as u64) as usize
    }
}

/// Reads everything a file has, whatever it turns out to hold.
fn read_everything(file: &[u8]) {
    let limits = Limits { pages: 4096, rows: 10_000, bytes: 4 * 1024 * 1024 };
    let Ok(mut db) = Database::open(Cursor::new(file), limits) else { return };
    let tables: Vec<String> = db.table_names().into_iter().map(str::to_owned).collect();
    for table in tables {
        let Ok(columns) = db.column_names(&table) else { continue };
        let columns: Vec<&str> = columns.iter().map(String::as_str).collect();
        if let Err(why) = db.rows(&table, &columns) {
            assert!(why.to_string().len() > 10, "{why}");
        }
    }
}

#[test]
fn files_damaged_at_random_never_crash_the_reader() {
    let built = [
        two_levels(),
        with_overflow().0,
        one_table(Make { page_size: 1024, kept_back: 12, encoding: 2 }, &[vec![word("a"), Value::Null]]),
    ];
    let files: Vec<&[u8]> =
        [DATABASE, SONGBOOK, UTF16_LE, UTF16_BE].into_iter().chain(built.iter().map(Vec::as_slice)).collect();
    let mut dice = Dice(0x5eed_1234_abcd_ef01);
    for whole in files {
        for round in 0..1500 {
            let mut file = whole.to_vec();
            // A few bytes changed (often to values that matter: nothing, one, everything), a
            // number of four bytes pointed somewhere else, or the end cut off.
            for _ in 0..=dice.roll(4) {
                let at = dice.roll(file.len());
                match dice.roll(6) {
                    0 => file[at] = 0,
                    1 => file[at] = 0xff,
                    2 => file[at] = dice.roll(256) as u8,
                    3 => file[at] ^= 1 << dice.roll(8),
                    4 if at + 4 <= file.len() => {
                        let pages = (file.len() / 512) as u32;
                        set32(&mut file, at, dice.roll(pages as usize + 2) as u32);
                    }
                    _ => file[at] = file[at].wrapping_add(1),
                }
            }
            if round % 7 == 0 {
                file.truncate(dice.roll(file.len()));
            }
            read_everything(&file);
        }
    }
}

#[test]
fn every_byte_of_a_small_database_changed_never_crashes_the_reader() {
    // Every byte of the two pages a table's rows are found through, set to each of a few values.
    let whole = two_levels();
    for at in 0..whole.len() {
        for value in [0, 1, 2, 5, 13, 0x7f, 0x80, 0xff] {
            let mut file = whole.clone();
            file[at] = value;
            read_everything(&file);
        }
    }
}

/// Compares every table, column and row of real files with what the system's SQLite gives.
/// Testing aid: `SOFTPRESENTER_SQLITE_FILES` names the files (separated as in PATH); run on a
/// Mac with `cargo test -p sp-model the_systems_sqlite -- --ignored --nocapture`.
#[test]
#[ignore = "needs files to compare, and /usr/bin/sqlite3"]
fn every_row_is_what_the_systems_sqlite_reads() {
    let files = std::env::var_os("SOFTPRESENTER_SQLITE_FILES").expect("SOFTPRESENTER_SQLITE_FILES names the files");
    let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02X}")).collect::<String>();
    let quoted = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    for path in std::env::split_paths(&files) {
        let mut db = Database::open(std::fs::File::open(&path).unwrap(), Limits::NORMAL).unwrap();
        let tables: Vec<String> = db.table_names().into_iter().map(str::to_owned).collect();
        let (mut rows_compared, mut values_compared) = (0, 0);
        for table in &tables {
            let columns = db.column_names(table).unwrap();
            let asked: Vec<&str> = columns.iter().map(String::as_str).collect();
            let ours = db.rows(table, &asked).unwrap();
            let each: Vec<String> = columns
                .iter()
                .map(|column| {
                    let c = quoted(column);
                    format!(
                        "typeof({c}), CASE typeof({c}) WHEN 'null' THEN '' WHEN 'integer' THEN {c} WHEN 'real' THEN quote({c}) ELSE hex({c}) END"
                    )
                })
                .collect();
            let query = format!("SELECT rowid, {} FROM {} ORDER BY rowid", each.join(", "), quoted(table));
            let output = std::process::Command::new("/usr/bin/sqlite3")
                .args(["-readonly", "-list", "-separator", "|"])
                .arg(&path)
                .arg(&query)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let theirs = String::from_utf8(output.stdout).unwrap();
            let theirs: Vec<Vec<&str>> = theirs.lines().map(|line| line.split('|').collect()).collect();
            assert_eq!(ours.len(), theirs.len(), "{table}: rows");
            for (row, their) in ours.iter().zip(&theirs) {
                assert_eq!(row.rowid.to_string(), their[0], "{table}: a row's number");
                for (at, value) in row.values.iter().enumerate() {
                    let (kind, written) = (their[1 + 2 * at], their[2 + 2 * at]);
                    let column = &columns[at];
                    let place = format!("{table}.{column} of row {}", row.rowid);
                    match value {
                        // A column added later reads as nothing here, and as its default there:
                        // said below, not failed.
                        Value::Null => {}
                        Value::Integer(n) => {
                            assert_eq!(("integer", n.to_string().as_str()), (kind, written), "{place}")
                        }
                        Value::Real(x) => assert_eq!(("real", Ok(*x)), (kind, written.parse::<f64>()), "{place}"),
                        Value::Blob(bytes) => assert_eq!(("blob", hex(bytes).as_str()), (kind, written), "{place}"),
                        Value::Text(text) => {
                            let utf16 = |write: fn(u16) -> [u8; 2]| {
                                hex(&text.encode_utf16().flat_map(write).collect::<Vec<u8>>())
                            };
                            let same = written == hex(text.as_bytes())
                                || written == utf16(u16::to_le_bytes)
                                || written == utf16(u16::to_be_bytes);
                            assert!(kind == "text" && same, "{place}");
                        }
                    }
                    if !matches!(value, Value::Null) || kind == "null" {
                        values_compared += 1;
                    } else {
                        println!("{place}: nothing here, {kind} {written} there (a column added later)");
                    }
                }
                rows_compared += 1;
            }
        }
        println!(
            "{}: {} tables, {rows_compared} rows, {values_compared} values the same",
            path.file_name().unwrap().to_string_lossy(),
            tables.len()
        );
    }
}
