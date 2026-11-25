# Translating SoftPresenter's own words

SoftPresenter's buttons, menus, tooltips and notices exist in English, Spanish, Russian and
Ukrainian (English and Spanish first). Songs, Bible text and Bible book names are content and keep
their own languages; the `sp` command line, log lines and dev reports stay in English; the phone's
remote page has its own tables. The rules for how words may grow are in the design's
`06-languages.md`; this page is how to do the work in the code.

## Wrapping a string

English stays in the code, as the source text and the fallback. The key says *where* the words
are, so that one English word needing two translations ("Back" for going back, "Back" for sending
an object behind others) gets two keys.

```rust
use crate::i18n::{t, tf, tn, ts};

ui.button(t("right.clear_all", "Clear All"))               // &'static str
ui.on_hover_text(t("left.new_song", "New song"))
```

- A key is `area.place.what`, lower case, and starts with the name of its area's table
  (`right.` lives in `locales/<language>/right.json`). The first word is the file; this keeps
  agents on different areas out of each other's files, and a test checks it.
- Use the same key only where the English *and* the meaning are the same. When you reuse one
  from another area (`left.untitled`), leave it in its own file.
- Write the English exactly as the window shows it, in Title Case where the app uses it. The
  other languages use sentence case in their tables.
- Never join pieces in code ("Added " + n + " songs"): write a whole sentence with placeholders.
- A string that is only content (a song's name, a group's name, a file) is not wrapped.
- `t` needs literals (`&'static str`) for both arguments. For a table of words kept before they
  are drawn, make each pair with `tk("key", "English")` (it is only the two together, so the
  scan below still finds them) and hand it on to `t` or `ts`.

## Placeholders

```rust
tf("left.already_in", "{item} is already in {list}.", &[("item", &song), ("list", &name)])
```

Every `{name}` of the English must be in each translation (a test checks it); word order may
change. A name with no value stays as written, so a mistake shows.

## Plurals

```rust
tn("left.songs", n, "{n} song", "{n} songs", &[])   // "1 song", "5 songs"
```

`{n}` is the number as the language writes it (13,613; 13.613; 13 613). In the table a plural is an
array: **two** forms for English and Spanish (one, other), **three** for Russian and Ukrainian
(one: 1, 21; few: 2 to 4, 22 to 24; many: 5 to 20, 11 to 14):

```json
"left.songs": ["{n} canción", "{n} canciones"]
"left.songs": ["{n} песня", "{n} песни", "{n} песен"]
```

## Numbers, times and quotation marks

`i18n::count(n)`, `i18n::number(x, places)`, `i18n::seconds(x)` ("0.4 s", "0,4 s") and
`i18n::quoted(text)` use the language's rules, from `locales/<language>/format.json`. The unit of
seconds is `common.seconds_unit`. Don't format a number or a time with `format!("{:.1}")` in a
place a person reads.

## Short forms and widths

Controls size to their words, and nothing in a control wraps or ends in "…" (rule 2 of 06). So:

- Measure, don't assume: `studio::widest(ui, &[words, …], &font)` is the longest of the wordings
  in the language set; use it for a width that must not change (Show / Hide, On / Off, a switch's
  pills). `studio::tab_switch` shares its row evenly and gives a tab the room its words need
  (`shared_widths`).
- A place with a fixed room (a layer key, the find box, All Songs) asks
  `studio::within(ui, key, english, font, spacing, room, capitals)`: the usual words if they fit,
  otherwise the table's short form, `"<key>.short"` (see `right.layer.slide.short`, "Diapo."). Add a
  `.short` row where a translation is over the budget. Budgets are in 06 (mode tabs 9 letters,
  layer keys 10 letters a line).
- Where a label swaps wording under the pointer (Show / Hide), keep the width of the longest.

## Where each area's table lives

`crates/sp-app/locales/<language>/<area>.json`: a flat JSON object, key to words (or to a plural
array). `build.rs` lists the files, so a new area is only a new file, no code to edit. Keys that
start with `_` (`"_note"`) are comments.

| Area (file) | What it holds |
|---|---|
| `topbar` | the bar along the top: views, find, Audience and Stage keys, the clock's neighbours |
| `keys` | the keys sheet and key names |
| `left` | the left column: libraries, playlists, import and add menus, messages about them |
| `slides` | the slides area: item bars, tiles, tags, empty states |
| `dock` | the bar under the slides: transitions, Continuous, Undo, Redo, views; theme versions |
| `right` | the right column: preview, layer keys, Clear All, tool tabs |
| `media` | the media strip under the slides: its menus, the empty strip, what adding pictures and videos says |
| `biblepage` | the Bible view's page and left column: Books, History, the keypad and lists |
| `bibles` | the Bibles window, installing and reading Bibles, and what they say |
| `setup` | the first-start setup's steps, in each system's wording |
| `integrations` | Settings › Integrations: VoxLive, Canva Sync and their pages, states and messages |
| `remote` | Settings › Remote Control and what Remote Control says in the window |
| `inspector` | the inspector at the right of the Edit view: its tabs, sections, fields and menus |
| `editor` | the Edit view's slide editing (`editor.rs`) and its stage layouts' parts (`stage_editor.rs`) |
| `import` | the Import Slides sheets |
| `tools` | the right column's tool tabs: Timers, Messages, Props, Macros, Stage, Audio and VoxLive; macros' steps and the stage layouts' items |
| `common` | words several areas share |
| `format` | numbers and quotation marks (not words) |

The next areas, each a file of its own for one agent: `editor` and `inspector`; `settings`,
`setup` and `screens`; `bible`; `notices`, `messages` and `dialogs`; `stage` and the remaining
views (props, timers, macros, announcements).

When you finish an area, add its file name to `SPANISH_DONE` in `i18n.rs`, so a string wrapped
later without its Spanish fails the tests.

## Spanish: which Spanish

The drafts follow the design's `lang-tables.js`: Spain's Spanish (ordenador, vídeo, pulsa), tú,
sentence case, “…” for quotation marks. A native speaker will review them; the first thing he
decides is Spain or Latin America, tú or usted. Where the design has no Spanish for a string, write
it in the same style. `"_note"` at the top of each Spanish file says so.

## Checking

- **What is missing:** `cargo test -p sp-app i18n::tests::spanish_lacks -- --ignored --nocapture`
  lists every wrapped key Spanish has no words for yet (`SOFTPRESENTER_LANGUAGE_LACKS=ru` for another
  language).
- **Tables agree with the code:** `cargo test -p sp-app i18n` checks that every table parses, that
  keys start with their file's name, that no table has a key nothing asks for, that placeholders and
  plural counts match, and that the finished areas lack nothing.
- **Nothing cut:** run a quiet test copy in the language and look at the pictures, in a large and
  the smallest window:

  ```
  SOFTPRESENTER_QUIET=1 SOFTPRESENTER_DATA=/tmp/some/empty/folder SOFTPRESENTER_LANGUAGE=es \
    SOFTPRESENTER_WINDOW_SIZE=1100x700 SOFTPRESENTER_CAPTURE_AFTER=3 \
    target/debug/softpresenter --capture-home /tmp/home-es.png
  ```

  Look for words cut off, a "…" in a control, wrapped button words, and things moving when a label
  changes. Other views have their own `SOFTPRESENTER_CAPTURE_*` switches (see AGENTS.md).
- In a unit test, `i18n::with_language(Language::Spanish, || …)` sets the language for that thread
  only; never change the real setting in a test but the one that already does.
