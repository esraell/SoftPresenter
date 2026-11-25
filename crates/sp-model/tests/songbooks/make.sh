#!/bin/sh
# Makes the small databases the SQLite reader's tests read, and what the system's own SQLite
# gives for the importer's two queries on each (the `.json` beside it), so the tests can check
# the reader against SQLite on a computer that has no SQLite. Run on a Mac, from this folder:
#
#     sh make.sh
#
# Every word in them was written for these tests. The table and column names are the ones
# SoftProjector's database and exported songbooks have.
set -eu
SQLITE=/usr/bin/sqlite3
cd "$(dirname "$0")"
rm -f ./*.sqlite ./*.sps ./*.json

DATABASE_QUERY="SELECT s.number AS number, s.title AS title, s.words AS words, s.music AS music, s.song_text AS text, b.name AS book FROM Songs s LEFT JOIN Songbooks b ON b.id = s.songbook_id ORDER BY b.name, s.number"
SONGBOOK_QUERY="SELECT s.number AS number, s.title AS title, s.words AS words, s.music AS music, s.song_text AS text, (SELECT title FROM SongBook LIMIT 1) AS book FROM Songs s ORDER BY s.number"

SONGS_COLUMNS="'number' INTEGER, 'title' TEXT, 'category' INTEGER DEFAULT 0, 'tune' TEXT, 'words' TEXT, 'music' TEXT, 'song_text' TEXT, 'notes' TEXT, 'use_private' BOOL, 'alignment_v' INTEGER, 'alignment_h' INTEGER, 'color' INTEGER, 'font' TEXT, 'info_color' INTEGER, 'info_font' TEXT, 'ending_color' INTEGER, 'ending_font' TEXT, 'use_background' BOOL, 'background_name' TEXT, 'background' BLOB, 'count' INTEGER DEFAULT 0, 'date' TEXT"

# A whole database: several songbooks, songs that name them by id (the id is the row's own
# number, kept nowhere in the row), small pages so the tables are three levels deep, a song
# longer than a page, rows deleted and changed, a column added later, and numbers of every kind.
$SQLITE database.sqlite <<SQL
PRAGMA page_size = 512;
CREATE TABLE 'Songbooks' ('id' INTEGER PRIMARY KEY  AUTOINCREMENT  NOT NULL , 'name' TEXT, 'info' TEXT);
CREATE TABLE 'Songs' ('id' INTEGER PRIMARY KEY  AUTOINCREMENT  NOT NULL , 'songbook_id' INTEGER, $SONGS_COLUMNS);
INSERT INTO Songbooks (name, info) VALUES ('Вечерние песни', 'Написано для проверки'), ('Ранкові пісні', ''), ('Gone', ''), ('Evening Hymns', 'Written for the tests'), (NULL, 'no name');
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 240)
INSERT INTO Songs (songbook_id, number, title, category, tune, words, music, song_text, notes, use_private, count)
SELECT 1 + (i % 4), i, 'Песня ' || i, 0, '', CASE WHEN i % 3 = 0 THEN 'Анна' ELSE '' END, CASE WHEN i % 5 = 0 THEN 'Борис' ELSE '' END,
  'Куплет 1' || char(10) || 'Свет над рекой, строка ' || i || char(10) || 'Песня над тобой' || char(10) || char(10) ||
  'Припев' || char(10) || 'Домой, домой, ' || i || char(10) || char(10) || 'Куплет 2' || char(10) || 'Вечер настал',
  '', 0, 0 FROM n;
INSERT INTO Songs (songbook_id, number, title, words, music, song_text) VALUES
  (2, 7, 'Ранок', 'Олена', 'Олена', 'Куплет 1' || char(10) || 'Сонце встає над полем' || char(10) || char(10) || 'Приспів' || char(10) || 'Радій, радій'),
  (2, 7, 'Ранок удруге', '', '', 'Куплет 1' || char(10) || 'Той самий номер'),
  (4, NULL, 'No number', NULL, NULL, 'Verse 1' || char(10) || 'A lantern on the hill'),
  (4, '12a', 'A number with a letter', '', '', 'Verse 1' || char(10) || 'Twelve and a little more'),
  (4, 7.5, 'Half a number', '', '', 'Verse 1' || char(10) || 'Seven and a half'),
  (4, ' 15 ', 'Spaces round its number', '', '', 'Verse 1' || char(10) || 'Fifteen'),
  (99, 3, 'No such songbook', '', '', 'Verse 1' || char(10) || 'Nobody''s book'),
  (NULL, 4, 'No songbook at all', '', '', 'Verse 1' || char(10) || 'On its own'),
  (5, 1, 'In the songbook without a name', '', '', 'Verse 1' || char(10) || 'Nameless'),
  (1, 500, 'Quotes "and" \\ back', 'A "B"', '', 'Verse 1' || char(10) || 'Tab' || char(9) || 'and emoji 🎵'),
  (1, 501, 'No words', '', '', ''),
  (1, 502, 'Null words', '', '', NULL),
  (1, 503, 12345, 678, 9.5, 'Verse 1' || char(10) || 'A title that was a number'),
  (1, -3, 'Below nothing', '', '', 'Verse 1' || char(10) || 'Minus three');
INSERT INTO Songs (songbook_id, number, title, words, music, song_text)
SELECT 4, 600, 'A very long song', '', '', group_concat('Verse ' || i || char(10) || 'Line ' || i || ' of the long song goes on and on' || char(10), char(10))
FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 150) SELECT i FROM n);
DELETE FROM Songbooks WHERE name = 'Gone';
DELETE FROM Songs WHERE number % 7 = 3 AND number < 200;
UPDATE Songs SET song_text = song_text || char(10) || char(10) || 'Окончание' || char(10) || 'Аминь, аминь, аминь, аминь' WHERE number % 11 = 0 AND number < 240;
ALTER TABLE Songs ADD COLUMN 'added_later' TEXT DEFAULT 'x';
INSERT INTO Songs (songbook_id, number, title, words, music, song_text, added_later) VALUES (2, 700, 'After the new column', '', '', 'Куплет 1' || char(10) || 'Новий рядок', 'y');
SQL

# A songbook as SoftProjector exports one: its title in SongBook, no ids, a song of several
# pages with a picture's worth of bytes after its words.
$SQLITE songbook.sps <<SQL
PRAGMA page_size = 1024;
CREATE TABLE 'SongBook' ('title' TEXT, 'info' TEXT);
CREATE TABLE 'Songs' ($SONGS_COLUMNS);
INSERT INTO SongBook VALUES ('Zpěvník pro zkoušku', 'Napsáno pro testy');
INSERT INTO Songs (number, title, category, tune, words, music, song_text) VALUES
  (2, 'Druhá píseň', 0, 'D', 'Jana', 'Petr', 'Verš 1' || char(10) || 'Žluťoučký kůň běží polem' || char(10) || char(10) || 'Refrén' || char(10) || 'Zpívej, zpívej'),
  (1, 'První píseň', 0, '', '', '', 'Verš 1' || char(10) || 'Ráno svítá nad řekou' || char(10) || char(10) || '&Verš 1' || char(10) || 'A den začíná'),
  (10, 'The tenth', 0, '', 'Ann', 'Ann', 'Verse 1' || char(13) || char(10) || 'A lamp in the window' || char(13) || char(10) || char(13) || char(10) || 'Chorus' || char(13) || char(10) || 'Come home'),
  (3, 'Третья', 0, '', '', '', 'Куплет 1' || char(10) || 'Тихий вечер'),
  (3, 'Третя, ще раз', 0, '', '', '', 'Куплет 1' || char(10) || 'Той самий номер'),
  (NULL, 'Без номера', 0, '', '', '', 'Куплет 1' || char(10) || 'Без номера');
INSERT INTO Songs (number, title, category, tune, words, music, song_text, background)
SELECT 20, 'Dlouhá píseň', 0, '', '', '', group_concat('Verš ' || i || char(10) || 'Řádek ' || i || ' dlouhé písně pokračuje dál a dál' || char(10), char(10)), zeroblob(5000)
FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 120) SELECT i FROM n);
SQL

# The same kind of songbook kept as UTF-16, either way round (which changes the order of two
# songs whose numbers are letters).
for kind in le be; do
$SQLITE utf16$kind.sps <<SQL
PRAGMA page_size = 512;
PRAGMA encoding = 'UTF-16$kind';
CREATE TABLE 'SongBook' ('title' TEXT, 'info' TEXT);
CREATE TABLE 'Songs' ($SONGS_COLUMNS);
INSERT INTO SongBook VALUES ('Пісні 🎵 UTF-16', '');
INSERT INTO Songs (number, title, category, tune, words, music, song_text) VALUES
  (2, 'Друга', 0, '', 'Олена', '', 'Куплет 1' || char(10) || 'Сонце встає 🎵'),
  ('ȁ', 'A number that is a letter', 0, '', '', '', 'Verse 1' || char(10) || 'Sorted by its UTF-16 bytes'),
  ('Ă', 'Another letter', 0, '', '', '', 'Verse 1' || char(10) || 'Before it one way round, after it the other'),
  (1, 'Перша', 0, '', '', 'Іван', 'Куплет 1' || char(10) || 'Ранок над полем' || char(10) || char(10) || 'Приспів' || char(10) || 'Радій');
INSERT INTO Songs (number, title, category, tune, words, music, song_text)
SELECT 3, 'Довга', 0, '', '', '', group_concat('Куплет ' || i || char(10) || 'Рядок ' || i || ' довгої пісні' || char(10), char(10))
FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 40) SELECT i FROM n);
SQL
done

# Three that give no songs: a database of something else, a songbook with none in it, and one
# whose songs have no text.
$SQLITE other.sqlite "PRAGMA page_size = 512; CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT); INSERT INTO notes (body) VALUES ('Not a songbook');"
$SQLITE empty.sps "PRAGMA page_size = 512; CREATE TABLE 'SongBook' ('title' TEXT, 'info' TEXT); CREATE TABLE 'Songs' ($SONGS_COLUMNS); INSERT INTO SongBook VALUES ('Empty', '');"
$SQLITE narrow.sps "PRAGMA page_size = 512; CREATE TABLE 'SongBook' ('title' TEXT, 'info' TEXT); CREATE TABLE 'Songs' ('number' INTEGER, 'title' TEXT, 'words' TEXT, 'music' TEXT); INSERT INTO SongBook VALUES ('Narrow', ''); INSERT INTO Songs VALUES (1, 'No text', '', '');"

# A database no program would make: songbooks sharing an id, and ids that aren't whole numbers.
$SQLITE shared.sqlite "PRAGMA page_size = 512; CREATE TABLE Songbooks (id, name); CREATE TABLE Songs (songbook_id, number, title, words, music, song_text);
INSERT INTO Songbooks VALUES (1, 'Second of the same id'), (1, 'First of the same id'), (2.5, 'Half an id'), ('x', 'A letter for an id'), (NULL, 'No id');
INSERT INTO Songs VALUES (1, 1, 'In two songbooks', '', '', 'Verse 1' || char(10) || 'Twice'), (1.0, 2, 'Its id has a fraction of nothing', '', '', 'Verse 1' || char(10) || 'Twice too'),
  (2.5, 3, 'Half', '', '', 'Verse 1' || char(10) || 'Once'), ('x', 4, 'Letter', '', '', 'Verse 1' || char(10) || 'Once'), ('1', 5, 'A one that is text', '', '', 'Verse 1' || char(10) || 'In none'),
  (NULL, 6, 'None', '', '', 'Verse 1' || char(10) || 'In none'), (1, 2.5, 'A number between', '', '', 'Verse 1' || char(10) || 'Between two and three'), (1, 9007199254740993, 'A very large number', '', '', 'Verse 1' || char(10) || 'Large'),
  (1, 9007199254740992.0, 'Nearly as large', '', '', 'Verse 1' || char(10) || 'Large');"
$SQLITE -readonly -json shared.sqlite "$DATABASE_QUERY" > shared.json

$SQLITE -readonly -json database.sqlite "$DATABASE_QUERY" > database.json
for book in songbook utf16le utf16be; do
  $SQLITE -readonly -json $book.sps "$SONGBOOK_QUERY" > $book.json
done
ls -l
