# Translating SoftPresenter's own words

SoftPresenter's buttons, menus, tooltips and notices exist in English, Spanish, Russian and
Ukrainian (English, Spanish and Russian in full). Songs, Bible text and Bible book names are
content and keep their own languages; the `sp` command line, log lines and dev reports stay in
English; the phone's remote page has its own tables. The rules for how words may grow are in the design's
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

## What stays English

The log (developers read it) and the `sp` command line's replies (scripts read them) stay in
English whatever the window speaks, while the window, the notices list and the remote page speak
the language set. Nothing at a call site has to do anything for that: every `t`, `ts`, `tf` and
`tn` in another language remembers its English, and `i18n::english_of` puts each piece of a text
it recognises back into English. The log's writer (`applog`) and the command line's reply writer
(`control::write_reply`) pass everything through it. It replaces whole words and phrases only,
never part of a word, and nothing under three letters; names and song words stay as they are.
For a reply built from words of its own, `i18n::with_language(Language::English, …)` makes it
in English to begin with (the macros' `describe` does).

A remote page gets the same replies without `english_of`, so a refusal wrapped with `t`/`tf`
reaches it in the window's language. JSON data is never turned back: where `sp`'s data holds
words (a playlist item's `text`), make them in English for `sp` and in the language for a page
(`service_cli::item_entry` and `item_text`). The server's own sentences are made on its thread
with `Shared::said`, in the language the app last gave it.

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
| `remote` | Settings › Phone remote and what Remote Control says in the window; what its server says to the phone's page (sign-in answers, refusals, errors, a playlist item's words) |
| `screens` | the Screens window: the list, each screen's output, look, background, key and DeckLink rows, what each screen is doing (which `sp screens` gives in English); the NDI download on Linux |
| `inspector` | the inspector at the right of the Edit view: its tabs, sections, fields and menus |
| `editor` | the Edit view's slide editing (`editor.rs`) and its stage layouts' parts (`stage_editor.rs`) |
| `import` | the Import Slides sheets |
| `tools` | the right column's tool tabs: Timers, Messages, Props, Macros, Stage, Audio and VoxLive; macros' steps and the stage layouts' items |
| `settings` | the Settings window: its list of pages in four groups and what each row says, Screens, Slides, Groups, Language, Keys, Command line, Song files, About |
| `edit` | the Edit view: its kinds, the slides and props lists, the object tools, Edit Words, why a key is greyed |
| `themes` | the Themes view and the version sheet |
| `bible` | the Bible view's bar, its Options and translations, and its history's dates |
| `dialogs` | confirmation sheets and small windows: delete a song, media or theme, New Song, missing fonts |
| `find` | the find box's list of results |
| `notices` | the notices list, every notice and refusal the window says, errors shown in the window, window titles, import and export messages, a slide's Check reasons |
| `menubar` | the Mac's menu bar: the menus' titles and items as drawn (`sp menu` keeps the English names) |
| `mcp` | Settings › AI assistants (MCP): how much an assistant may do, the tiles that add SoftPresenter to each company's assistant apps (Add, Replace, Remove, Copy setup, where it stands, why a settings file was left alone) (what `sp mcp` says to an assistant, and the replies of `sp mcp connect`, stay English, as `sp` does); app names stay as they are |
| `common` | words several areas share |
| `format` | numbers and quotation marks (not words) |

Spanish and Russian have every area above. What stays in English is what other crates say in their own words
when something fails (NDI's, the DeckLink driver's and a video file's reasons, shown as they
come), and the few errors said at startup before the language is known (the home window can't
open, no graphics device).
Where another crate's words are drawn in the window, sp-app says them from the crate's own values
(`screens::decklink_driver_words`, `screens::decklink_format_words`, `screens::ndi_not_installed`)
or hands the crate its words (`sp_render::set_ndi_tag_words`), so the crate and `sp` keep their
English.

When you finish an area, add its file name to `SPANISH_DONE` in `i18n.rs`, so a string wrapped
later without its Spanish fails the tests. Russian is held to the same list (`RUSSIAN_DONE` is
`SPANISH_DONE`), so a string wrapped later needs its Russian as well as its Spanish.

## Spanish: which Spanish

The drafts follow the design's `lang-tables.js`: Spain's Spanish (ordenador, vídeo, pulsa), tú,
sentence case, “…” for quotation marks. A native speaker will review them; the first thing he
decides is Spain or Latin America, tú or usted. Where the design has no Spanish for a string, write
it in the same style. `"_note"` at the top of each Spanish file says so.

## Russian: style and glossary

Written for Russian-speaking evangelical (Baptist) churches, which sing from «Песнь Возрождения»:
plain and friendly, the way a church volunteer talks, in church words, not Orthodox ones and not
office jargon. Translated from the English in the code; the Spanish only helps with what a string
means. The Ukrainian tables follow the same glossary, so the two read alike.

- **Form.** Buttons, menu items and tabs are an infinitive or a noun («Очистить», «Добавить
  песню»); sentences speak to the person with «вы» («Нажмите…», «Выберите…», «ваша библиотека»),
  never «ты». Sentence case everywhere, Title Case of the English included («Очистить всё»).
  Capitals only where the English has them (the small captions the app sets in capitals are
  written in sentence case). ё is always written.
- **Punctuation.** «…» round a name, as `format.json` says, but never round a placeholder the code
  fills already quoted (`i18n::quoted`, as for `{title}` in «Удалить {title}?»): look at the call
  before adding them. The ellipsis … where the English has it; a dash is « — ».
- **Names in sentences never take a case ending.** A placeholder gets a head noun («Экран «{screen}»
  выключен», «в список «{list}»») or stands before a colon («{name}: не удалось прочитать»).
- **Plurals** have three forms, each with `{n}`: `["{n} песня", "{n} песни", "{n} песен"]`. A
  count that isn't a plural in the code goes in brackets or after a colon («Удалить выбранные
  песни ({n})?», «Нет шрифтов: {n}»).
- **Never translated:** product names (SoftPresenter, NDI, DeckLink, VoxLive, Canva, Canva Sync,
  TimelyChurch, Worship Leader App, Claude, ChatGPT, Grok, Gemini, Bluetooth, iPhone, Mac,
  FFmpeg…), and keys as their caps print them (Esc, Shift, Enter, Delete, Home, End, ⌘); only
  Space, the arrows and «a letter», which have no printed name, are Russian (Пробел, Стрелки,
  Буква). The Mac's menu bar uses the Mac's own Russian (Правка, Справка, Завершить…).
- **Song group names stay content.** The New Song example keeps `[Verse 1]` and `[Chorus]`, as the
  Spanish does: those are the names the group keys (C for the chorus) and colours know.
- **Short forms** (`.short`) where Russian runs long in a fixed room: the layer keys Сообщ. and
  Объявл., Ветхий З. and Новый З. in the Bible lists, Найти for the find box, Песни for All
  Songs, Ещё for the Bible bar's Options when the bar is narrow, and the rest of the rows the
  Spanish has.
- **Words chosen to fit** (no short form is read there): the top bar's Audience and Stage keys
  say Вкл and Выкл without the dot, so the find box keeps its room in a 1100-point window; the
  sort row is Порядок; Identify's other wording in Settings › Screens is Остановить and the colour
  bars are Полосы, so the check strip fits the Settings window's least width; Dissolve is Плавно
  in the list too (the media strip's chip); the stage layout is Слова и далее; the media strip's
  empty box and the inspector's note on the whole box are written short.

| English | Russian |
|---|---|
| Service (the church service, the first playlist, the view's tab) | Служение |
| Bible (the view) / Edit (the view) | Библия / Редактор |
| playlist / the service's playlist | список / список служения |
| playlist item | пункт |
| header / divider / placeholder (in a playlist) | заголовок / разделитель / пустое место |
| library / All Songs | библиотека / Все песни |
| song / slide | песня / слайд |
| verse / chorus / pre-chorus / bridge / ending / intro / tag | куплет / припев / предприпев / бридж / окончание / вступление / тег |
| group (of a song) | часть |
| arrangement | порядок исполнения (short: порядок) |
| Bible verse / passage / reference / chapter / book | стих / отрывок / ссылка / глава / книга |
| Bible translation / reading | перевод / чтение |
| screen / display (a monitor) | экран / монитор |
| audience screens / Audience (key and tab) | экраны для зала / Зал |
| stage screens / Stage (key and tab) / stage layout | экраны для сцены / Сцена / макет сцены |
| the operator's window | окно оператора |
| Clear / Clear All / take off the screens | Очистить / Очистить всё / убрать с экранов |
| Show / Hide / On / Off | Показать / Скрыть / Вкл. / Выкл. (the top bar's keys: Вкл / Выкл) |
| layer keys | кнопки слоёв |
| key (keyboard) / hot key | клавиша / горячая клавиша |
| key (a video switcher's) / fill | ключ / заливка |
| media / media bin / media playlist | медиа / медиатека / медиасписок |
| picture / video / audio / track / audio playlist | картинка / видео / аудио / трек / аудиосписок |
| props / prop | накладки / накладка |
| messages / quick message / announcements | сообщения / быстрое сообщение / объявления |
| timers / macros | таймеры / макросы |
| theme / theme version / the usual look / template | тема / вариант темы / обычный вид / шаблон |
| transition / Cut / Dissolve / Continuous | переход / Сразу / Плавно / Подряд |
| box (on a slide) / object / shape | блок / объект / фигура |
| captions / test pattern | субтитры / тестовая картинка |
| preview / live (on the screens) | предпросмотр / на экране |
| Settings / Integrations / AI assistants | Настройки / Интеграции / ИИ-помощники |
| Settings groups: The room / On screen / Connect / Advanced | Зал / На экране / Подключения / Дополнительно |
| Phone remote / Remote Control / Phone Screens | Пульт на телефоне / Удалённое управление / Экраны телефонов |
| songbook / free songbooks / song pack / song bundle | сборник / бесплатные сборники / набор песен / пакет песен |
| notices / setup (first start) | уведомления / первая настройка |
| name (of a song, list, file) / name (NDI, a person) | название / имя |
| sign in / sign-in / password / sync | войти / вход / пароль / синхронизация |
| Import / Export / download / install | Импорт / Экспорт / скачать / установить |
| Undo / Redo / Cancel / Done | Отменить / Повторить / Отмена / Готово |
| Delete / Remove (from a list) | Удалить / Убрать |
| pick / picked | выбрать / выбранные |
| Trash | Корзина |
| Command line / terminal | Командная строка / терминал |
| Identify / Stop identifying | Опознать / Остановить (the setup: Остановить опознание) |

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
