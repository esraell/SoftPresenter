# A map of SoftPresenter

What the app looks like, what every part of it does, which command-line command does the same,
where it keeps its files, and how the code is laid out. Written for people and AI agents
finding their way around. For the command line itself see [cli.md](cli.md).

## The main window

One window: a bar along the top, and under it three round panels, the left column, the middle
and the right column, which is the same in every view. Three views are chosen in the bar (Show, Bible and Edit). The
operator's window never shows on the audience screens; the screens are separate windows (or NDI
sources) drawn by their own thread.

```
  ▬ SoftPresenter [Show|Bible|Edit] (Find a song or a line)         12:00:00 [● Audience ON] [● Stage ON] ? ⚙
╭──────────────────╮ ╭─────────────────────────────────────────╮ ╭────────────────────────────────╮
│ PLAYLIST       ⤓ │ │ ● Song title  (i) ≡ ∞ ▤ ⋯ ────────────── │ │ ● [AUDIENCE|STAGE]    ON SCREEN│
│ ≡ Sunday       ⌄ │ │ ┌──────┐ ┌──────┐ ┌──────┐                │ │ ┌────────────────────────────┐ │
│ 01 Countdown   ⋯ │ │ │slide │ │slide │ │slide │   slide grid   │ │ │  what the screens show     │ │
│ WORSHIP          │ │ └──────┘ └──────┘ └──────┘   (or words    │ │ └────────────────────────────┘ │
│ 02 Song one    ⋯ │ │ 01 ●VERSE 1  02 ●VERSE 1  03 ●CHORUS only,│ │ 03/08  CHORUS · SONG TITLE     │
│ 03 Song two  ● ⋯ │ │ Next item ──────────────────  or a table) │ │ (a row for what is playing)    │
│ THE WORD         │ │                                          │ │ [Slide]   [Media]  [Props]     │
│ 04 John 3:16   ⋯ │ ├─────────────────────────────────────────┤ │ [Messages][Audio]  [Announce]  │
│                  │ │ [Cut|Dissolve|Advanced▾] [Continuous]     │ │ [          Clear All         ] │
│ [+ Add to        │ │ [Undo][Redo]     [grid|words|table] size │ │ Timers Messages Props Macros   │
│    playlist    ] │ │ MEDIA  (All Media ▾)    (Dissolve ▾) ⌄ + │ │ Stage  Audio  (tabs)           │
│ (Filter)         │ │ [pic] [pic] [video ▶]                    │ │   …the open tab's list…        │
╰──────────────────╯ ╰─────────────────────────────────────────╯ ╰────────────────────────────────╯
```

### Top bar

| Part | Does | Command line |
|---|---|---|
| Show / Bible / Edit | Switches the view. Edit has four kinds, chosen at the top of its left column: Slides, Themes, Props and Stage | `view show\|edit\|bible`; `view themes` opens Edit on the themes |
| Find box | Finds as you type, listed under it: a Bible passage first when the words are a reference (`John 3:16`, `Ин 3:16-18`, `ps 23`), named and quoted in the language it was typed in; then songs: by title, then each slide whose words have them (tagged with its group, `SONG · CHORUS`, the line found under it; picking one brings the view to that slide and rings it in ink for a moment); then verses that hold the words, from the Bibles in use written in their alphabet (each tagged with its Bible, `BIBLE · KJV`). Each result has a mark at its left and its tag in the same tint, so songs and the Bible read apart: a note in the libraries' lilac for a song or a slide of one (`SONG_TINT`), an open book in sea green for the Bible (`SCRIPTURE_TINT`, also the dot on Bible slides); the titles stay in ink, the words grey. Return or a click opens the one picked (a song in the Show view, a passage or verse in the Bible view; nothing goes on the screens) with its first slide or verse picked (a single verse opens in its whole chapter, that verse picked), drawn in ink with a small key cap for Enter (`paint_enter_key` in `home/slides.rs`), and the keyboard back with the show, so Enter puts it on screen; Command-Return (Ctrl on other computers) adds it to the playlist in front, Escape or a click elsewhere ends the search. A result can be dragged (the chooser shows above the left column's list while it is carried): onto the open playlist's items, where the ink line shows (or onto a placeholder); onto a playlist in the chooser, at its end; a song onto a library (row or song list); a passage or verse never into a library. A dropped result ends the search and is opened in the playlist listed; nothing goes on screen (`library_search` in `home.rs`; the drops in `studio.rs`, `Carried`). `/` goes to it, and so does Command-F. It does not search playlists or media (`find.rs`) | |
| Running timer | Just before the clock while a timer runs: its name and time, in green. It takes only room nothing else uses, so nothing moves when it starts or stops | `timers` |
| Clock | The time of day, on a 12-hour or a 24-hour clock as the computer is set (`clock.rs`) | |
| Audience ON/OFF | The audience screens on or off (Shift-Command-1; Ctrl+Shift+1 on other computers) | `audience on\|off` |
| Stage ON/OFF | The stage screens on or off (Shift-Command-2) | `stage on\|off` |
| ? | The list of every key (the `?` key opens it too; Escape, Enter or Done close it) (`keys.rs`) | |
| ⚙ | Settings: General, Screens, Groups, Key Mappings, Integrations (Command-comma) | `menu Settings` |

### Left column: a playlist or a library

| Part | Does | Command line |
|---|---|---|
| PLAYLIST or LIBRARY, with the import button | Says which kind of thing is listed; the button imports songs, a library folder, or slides | `song import FILE…` |
| The name of what is listed | A click opens the chooser, or puts it away | |
| The chooser | Only things to open: the playlists (with how many items), then the song libraries under a line (All Songs, then each library); the open one is ticked; + makes one; ⋯ or right-click to rename, export or delete; drop songs (or a picture or video, or a result of the find box) on a row to add them, from the window or the Finder; a row that can't take what is carried shows the no-drop pointer, and All Songs takes nothing from the window (it shows above the list while song files are dragged over the window; the row under the pointer gets an ink outline, `file_drop.rs`) | `playlists`, `playlist open NAME`, `libraries`, `library open NAME` (or `all`) |
| Song list (a library open) | Song files dropped from the Finder go into this library (outlined while they are over the window); click opens a song; Command-click adds or takes away one, Shift-click (or Shift-↑/↓) picks a run from the one last clicked, Command-A picks all, dragging picks several; drag picked songs onto a playlist or library, in the list's order (the chooser shows while they are carried); Delete asks, then deletes; ⋯ or right-click for Add to Playlist, Duplicate, Theme, Transition, Reflow, Play on Announcements Layer, Delete… | `songs`, `select SONG`, `song delete SONG --yes`, `announce SONG` |
| Order: As listed, A–Z, Longest; New song (a library open) | The order the songs are listed in; a new song | `song new TITLE` |
| Playlist items (a playlist open) | A click opens an item (the slides scroll to it; the screens don't change); Command-click, Shift-click, Shift-↑/↓ and Command-A pick several as in the song list (`HomeState::item_picks`), lit in the chosen grey. Songs, Bible passages, pictures and videos, media playlists, placeholders and headers in running order, each with its number, a mark for its kind, the orange dot while it is on screen, and ⋯ for its menu; drag to reorder (several picked move together, in their order); Delete or Backspace takes the items picked (or the one open) out at once, asking nothing, and Command-Z puts them back (`take_out_items` and `redo_removed` in `studio.rs`; the step is kept in `ShowHistory` in `main.rs`); a picture or video dragged from the bin, or a result dragged from the find box, goes in where the line shows, or fills a placeholder; song files, pictures and videos dropped from the Finder go after the item under the pointer (the playlist is outlined, with a line where they go); a placeholder's menu has Fill with Song | `playlist [NAME]`, `slides item N [in PLAYLIST]`, `playlist fill PLACEHOLDER with SONG`, `playlist remove ITEM [from PLAYLIST]`, `undo` |
| Add to playlist (a playlist open) | Adds a song, a header, a placeholder, a picture or video, a media playlist or the VoxLive item after the open item; A Bible Passage… opens the Bible page | `playlist add SONG to PLAYLIST`, `playlist header\|media\|media-playlist\|placeholder NAME`, `bible REF --add` |
| Filter | Filters the list (the find box in the top bar finds songs by title and words, and Bible verses) | |

### Middle: the slides

| Part | Does | Command line |
|---|---|---|
| The scroll | With a playlist listed on the left: every item of it, one after another, each under a bar with its name that stays at the top while its slides scroll under it. With a library listed: the song chosen, the same way | `playlist go ITEM [SLIDE]`, `slides SONG [--full]` |
| An item's bar | A dot when it is the open item; the name (a song's renames on a double click); for a song its tools: details (writer, CCLI), words as text, arrangement (groups in another order: a row of pills under the bar, one per piece, ×2 for a group sung twice running, 2/3 while on screen; the slides then stand in the order sung, with what it leaves out under "Not in this arrangement"), notes, ⋯ (the song's menu, Add Slide, its place in the playlist); for anything else a small label saying what it is | `song info`, `song text`, `song arrangement` |
| Slides | Tiles (the picture, and a strip with the number, the group's dot and name, tags, a pencil for the editor and ⋯), words only, or a table. Click puts a slide on screen; ⋯ or a right-click opens its menu (Edit, Cut, Copy, Paste, Duplicate, Delete…, Move Up, Move Down, Group, Media, Transition, Actions, Go to Next, Hot Key, Copy/Paste Text Style, Label, Disable, Template); Command/Shift-click and dragging a box pick several without going live; Delete asks, then deletes | `go SONG SLIDE`, `next`, `previous`, `first`, `last` |
| Other items | A Bible passage's slides, drawn by the Bible theme (click one to put it on screen); a picture or video's tile (click to show it); a media playlist's tile (click to play it); a placeholder's box (Choose a song…, or drop a picture or video on it); a header as a small capital line | `playlist go ITEM [SLIDE]` |
| The dock: the bar under the slides | The transition for all slides (Cut, Dissolve, Advanced ▾ with every other kind and how long it takes); Continuous; Undo, Redo; grid, words only or table; slide size. Add Slide is in a song's ⋯ menu and the File menu | `continuous on\|off`, `undo`, `redo`, `menu NewSlide` |
| The dock: the media strip | Pictures and videos with their names; click shows one behind the words (it is ringed in orange); drag onto a slide (its media), onto a song's bar or under a song shown by itself (a slide of its own), or onto the playlist (an item, where the ink line shows) or a placeholder (it fills it); Command/Shift-click or a box picks several; Delete asks; ⋯ or right-click for Scaling, Loop, playlists, Delete…; the menu beside MEDIA picks a media playlist (Play This Playlist, Follow a Folder…); the menu at the right is how one gives way to the next; + adds some, and so do pictures and videos dropped on it from the Finder (it opens and lights up for those only; song files dropped on it go to the songs); the chevron folds the strip away | `media`, `media show ITEM`, `media add FILE…`, `media delete ITEM --yes`, `media-playlist play NAME` |

### Right column: what is on screen

The same in every view. In the Edit view and the Theme Editor the inspector takes the place of
the tabs. Nothing from the preview down to Clear All moves, whatever is on screen.

| Part | Does | Command line |
|---|---|---|
| Preview block | What the audience screens show, drawn by the screens' own renderer (videos move in it); the switch at its head shows the stage picture instead; with more than two screens, a small arrow beside it opens a menu to look at one screen, drawn as that screen draws it (its own theme, layers and colour; a stage screen in its own layout; one switched off or not connected says so over the picture), and Audience or Stage goes back; ON SCREEN or CLEAR at the right. Under the picture, the slide's number in the song as played, its group and the song; a second line only when there is something to say ("Looking at Stream", or how far the slide's click builds have got) | `status`, `status --wait SECONDS --since TOKEN` |
| The row under the readout | Whatever is playing: a video's keys (play or pause, start over, forward 10 seconds, a bar to drag through, the time left, loop, sound), the song on the Announcements layer with Clear, or a test pattern with Off. Empty, but there, when nothing plays | |
| Slide, Media, Props, Messages, Audio, Announce | The six layer keys: each is lit while its layer shows, and clears that layer alone (F2 slide, F3 media, F4 props, F5 audio, F6 messages) | `clear slide\|media\|props\|messages\|audio\|announcements` |
| Clear All | Takes everything off the screens (Escape, F1); orange while anything shows | `clear all` |
| Tabs: Timers | A card each: count down, count up, to a time of day; Reset, Start or Stop; ⋯ (or right-click) for Edit… and Delete; + makes one | `timers`, `timer start\|stop\|reset NAME`, `timer new\|set\|delete` |
| Tabs: Messages | A card each, with fill-in tokens (the time, a timer), shown over the slides: Show or Hide; ⋯ for Edit…, Rename, Duplicate, Cut, Copy, Paste, Delete; Words once… shows typed words at once (kept as Quick message) | `messages`, `message show NAME`, `message hide`, `message show --text WORDS` |
| Tabs: Props | Slides of their own laid over the show (logos, lower thirds), a card each with its picture: Show or Hide; ⋯ for Edit in the Editor, Clears Itself, Rename, Duplicate, Delete | `props`, `prop show\|hide NAME`, `prop new\|delete` |
| Tabs: Macros | Steps done in turn, a card each: Run (the name alone does nothing); the chevron opens its steps (Earlier, Remove, Add a step…); ⋯ for Colour, Run When SoftPresenter Opens, Rename, Duplicate, Delete | `macros`, `macro run NAME`, `macro new\|add\|remove\|delete` |
| Tabs: Stage | Which stage screen (with more than one; all by default); every layout as a tile with a picture of the stage in it, the one in use ticked, a click switches; Edit layouts…; the stage message (Show on stage, Change it, Clear) | `stage on\|off`, `stage layout SCREEN LAYOUT`, `stage message TEXT\|clear`, `stage objects\|picture\|frame LAYOUT …` |
| Tabs: Audio | Audio playlists (chosen at its head; ⋯ and + to rename, make, delete); the player: a line to drag, previous, −15, play or pause, +15, next, volume with mute, Play through, Shuffle, Crossfade; the tracks (click to play, drag to move, ⋯ for Move Up, Move Down, Rename, Remove; Add tracks…) | `audio`, `audio play\|pause\|stop\|next\|previous`, `audio volume N`, `audio add FILE…` |
| Tabs: VoxLive (once VoxLive is connected) | The church's VoxLive screens, a card each: Show puts that screen's captions on the screens in place of a verse (it reads On screen, on an orange-tinted card, while they show) | `voxlive show N\|NAME` |

### Keys in the Show view

| Key | Does |
|---|---|
| Space, → | Next slide. With another item opened (↑, ↓ or a click on its row), starts that item at its first slide; at the end of a song, with Continuous on, goes on into the next song or Bible passage. While a slide picked waits for Enter, goes on with what is on screen; with nothing on screen, Space puts the picked slide on and → / ← move the pick |
| ← | Previous slide; starts another item opened, as Space does; at a song's first slide, with Continuous on, goes back into the item before |
| ↑, ↓ | Opens the playlist item above or below (a notice says so at either end); in the Bible view, picks the verse before or after without putting it on screen |
| Enter | The slide picked goes on screen (the first slide or verse of what the find box or Scripture Lookup just opened, or the verse ↑ or ↓ picked in the Bible view); with none, the open item from its first slide (with a library listed, the song chosen) |
| Home, End | First, last slide of the song |
| Escape (F1) | Clear all |
| F2, F3, F4, F5, F6 | Clear slide, media, props, audio, messages |
| A letter | The slide with that hot key, or the group it stands for (A for the first verse, S for the second, C for the chorus, E for the ending…; the letter shows at the right of a group's bar). In the Bible view a letter or number starts the next reference in Scripture Lookup instead |
| Delete | Delete what is picked out, after asking |
| Command-N | New song |
| Command-1, -2, -3 | Show, Bible (cursor in the reference), Edit (Ctrl on other computers, for all of these) |
| Command-B | Bible as well (Bold while editing a slide) |
| Command-P | The chooser of playlists and libraries |
| Option-1 … 7 (Alt elsewhere), then 1 … 9 | Opens that tool tab and numbers what is in it; a number then starts that timer, shows that message or prop, runs that macro, switches that stage layout or plays that track. The numbers go after four seconds; Escape puts them away and clears nothing |
| Shift-Command-M, then 1 … 9 | Shows that picture or video from the media strip |
| Shift-Command-1, -2 | Audience screens, stage screens, on or off |
| Command-O | The menu of the slide on screen |
| ? | Every key, as a list; the switch at its foot is Show keys on buttons |
| Help › Show Keys on Buttons (a Mac's menu bar; the key list's switch everywhere) | A small amber tag beside everything that has a key, all the time; off, never (kept with the preferences; holding Command no longer shows them) (`keys.rs`) |
| SoftPresenter › Set Up SoftPresenter… | The setup a fresh install starts with: screens, songs, Bibles, permissions |
| File › Import Songs… (or drop song files on the window) | Import songs from text, lyric, ChordPro, rich-text or Word files, `.pro` documents and `.proBundle` bundles (whose pictures and videos go into the media bin, `bundle.rs`) |
| File › Import from ProPresenter… (or drop a folder) | Bring a library folder across; each folder becomes a library; song documents and bundles can be chosen too |
| File › Import Slides (PowerPoint, Keynote, PDF or Pictures)… (or drop one, or a folder of pictures) | A presentation of picture slides, one per slide, page or picture |
| Command-L | Back to the song on screen |

Every menu bar item, with its shortcut, is listed by `sp menu`; any of them can be chosen with
`sp menu ITEM`.

## The Edit view (slides, themes, props, stage)

```
┌──────────────────┬─────────────────────────────────────────────┬───────────────────────────┐
│ Song title       │ ruler ─────────────────────────────────────  │ [Shape] [Text] [Build]   │
│ ┌──────────────┐ │  [Add Text] [Shapes ▾] [Media…] [Web Page]   │  (the open tab is light) │
│ │ slide 1      │ │ ┌───────────────────────────────────────┐    │  font, size, colour,     │
│ ├──────────────┤ │ │                                       │    │  outline, shadow, fill,  │
│ │ slide 2      │ │ │         the slide, full size          │    │  gradient, spacing,      │
│ └──────────────┘ │ │   drag to move, handles to resize     │    │  linked text, builds…    │
│ Objects          │ └───────────────────────────────────────┘    │                          │
│   T Main         │ X  Y  W  H                     Grid   Fit    │ [Slide Notes] [Live Edit]│
│   ▢ photo        │                                              │                   [Show] │
└──────────────────┴──────────────────────────────────────────────┴───────────────────────────┘
```

- **Left**: at the top a switch, Slides · Themes · Props · Stage, for what is edited. Under it,
  for Slides the song's slides (click to edit another); for Themes the themes and the open
  theme's templates; for Props the props (click to edit another; props are made, renamed and
  deleted in the Props tab of the Show view, and Props is faint until there is one); for Stage
  the stage layouts, with +, − and Duplicate at the foot. For a slide, a template or a prop, the
  objects come next, front first (lock, hide, rename, drag to reorder).
- **Canvas**: Add Text, Shapes, Media… (a picture or a video, which plays in its box when the
  slide is on screen), Web Page (a box showing a live web page; its address is typed in the
  Shape tab); pictures and videos can be dropped on it from the Finder. Double-click a
  text box to type in it. The letter keys add objects (T for a text box, one key per shape).
- **Inspector** (in the right column, under Clear All): Shape (place, fill, outline, shadow, flip), Text (words, font, size, colour,
  alignment, spacing, outline, shadow, gradient, linked text such as a clock or a timer), Build
  (how an object comes on and goes off).
- **Footer** (at the foot of the inspector): Slide Notes, Live Edit (changes reach the screens
  as they are made), Done.
- **Stage** (Edit › Stage; Screens › Edit Stage Layouts… and the Stage tab's Edit Layouts… open
  it): the layouts for the stage screens, edited with the same canvas, Objects list and
  inspector as a slide. Left: STAGE LAYOUTS with + (a new layout) and a row for each, ⋯ on it
  for Rename, Duplicate, Delete. Over the canvas: the layout's name, Add ▾ (Text, a shape, a
  picture or video, a web page, then what the stage shows live: Current Slide, Next Slide, Next
  Label, Clock, Timer, Slide Notes, Song Title, Group, Stage Message, and Screen Picture with
  the audience screens), Undo and Redo; under it two lines saying the keyboard edits here. The
  inspector, with nothing picked, is LAYOUT (its name, the stage screens using it); for an item
  the Text tab starts with what it shows and its own settings (which timer, the clock's form,
  lines of notes); for a screen picture the Shape tab starts with its screen, corners and
  label. The arrows move an object 10, Shift one pixel (on a slide 1, and Shift 10). While it
  is open the keyboard does not run the show, as in the rest of Edit.
- **Themes** (Edit › Themes; View › Themes, Control-T and `sp view themes` open it): the same canvas, tools and inspector, editing a theme's
  layouts (templates). A layout's text boxes say where each kind of words goes; its shapes,
  pictures and videos are drawn behind the words on every slide that uses it. The list on the
  left holds the themes (tagged Songs and Bible by what uses them) and the layouts.

## The Bible view

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ [John 3:16-18      ] [Books]  [Search          ]   [Add to "Sunday" ] [▾]   [Options] │
│ [King James ▾]  Verse [Main ▾]   Reference [Reference ▾]                 License  [+] │
│ [Synodal    ▾]  Verse [Translation ▾] Reference [None ▾]                 License  [−] │
├────────────────────────────────────────────────────────────────────────────────────────┤
│  verses as slides; click one to put it on screen                                       │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ [‹ Verse] [Verse ›]                    Cut Dissolve Advanced ▾    grid/easy/table  size │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- Up to four Bibles side by side; each row picks a Bible (None frees it for another row), the
  box its verse goes in, and the box its reference goes in.
- Options: how verses become slides, the theme and layout for Bible slides, and the Bibles
  installed (more can be downloaded).
- Add to "…" keeps the passage in a playlist, to open again with these settings.
- Keys: Return in Scripture Lookup (or in the find box) picks the passage's first verse and gives
  the keyboard back; ↑ and ↓ pick the verse before or after (never putting it on screen); Enter
  puts the verse picked on screen (so does Space with nothing on screen, while → and ← move the
  pick); Space, → and ← then go verse by verse. The pick is
  `HomeState::picked` (`home.rs`), the rules for the keys `running.rs`.
- The lists: a click on a book loads its first chapter, on a chapter that chapter, on a verse
  that verse (Shift for a passage; a double-click opens the chapter with the verse), each with its
  first verse (or that verse) picked; nothing goes on screen from them (`bible.rs`, `lists`).
- Search, and words (not a reference) typed in Scripture Lookup, list the verses holding them as
  they are typed, found on a thread of their own (`find.rs`, `search_page` and `Finder`).
- What the operator reads (Scripture Lookup after a lookup, the books in the lists, Search's
  verses, the find box's passage) is in the language the reference was typed in, or with
  nothing typed the session's (at first the computer's); the slides follow the rows above
  whatever the language. Chapter and verse numbers are the first row's. The rule is in
  `bible_language.rs`; the computer's language is read once at start in `system_language.rs`.
- Command line: `bible REFERENCE [--show]`.

## Other windows

| Window | Opened from | Holds |
|---|---|---|
| Screens | Settings, Screens; or Screens, Configure Screens… | Audience and stage screens: where each goes (a display, a window, NDI, a DeckLink output with its format (starting in the card's own, as Desktop Video Setup sets it, and saying where the format comes from, in `screens/decklink_ui.rs`) and keyer, off; `decklink`, `screen set SCREEN output=decklink:N mode=FORMAT keyer=external`), its name, its look (which layers, colour, covered edges); an audience screen's Background, Solid or See-through (clear where nothing is drawn, for a switcher to lay over a camera; Soft edges Straight or Premultiplied), and Key (shows another screen's key, white where solid, beside it on a second display or NDI name), in `screens/background_ui.rs`; what each is doing now (for NDI, how many receivers are watching, or that nobody is; why a key shows black); Identify. `sp screen set SCREEN background=… alpha=… key=…`, `sp screens` |
| Settings | ⚙ or Command-comma | General (copyright line, start-up choices), Screens, Groups (names and colours), Key Mappings, Integrations (VoxLive: its API key, its state, the church's screens; click a screen to change its look, with a preview, and save it to VoxLive; `voxlive …`, `voxlive look`) |

## Where the app keeps things

On a Mac, in `~/Documents/SoftPresenter/` (or the folder `SOFTPRESENTER_DATA` names). An
earlier version kept it in `~/Library/Application Support/SoftPresenter/`; the first start
of this one copies it to Documents, checks the copy, and keeps the old folder as
"SoftPresenter (moved to Documents)" (`library_folder.rs`).

| File or folder | Holds |
|---|---|
| `Songs/*.sppres` | Songs made or changed in the app (JSON), each named by its id |
| `Themes/*.sptheme` | Themes (JSON); `themes-in-use.txt` says which theme songs and Bible slides use |
| `Images/` | Pictures put on slides, and slides made from PDFs and presentations; any nothing uses goes to the Trash after a week (`sweep.rs`) |
| `Media/` | Videos brought in by song packs (a slide names one as `Media/<file>`); tidied as `Images/` is |
| `Bibles/` | Downloaded Bibles |
| `services.json` | Playlists; a song in them is named by its id |
| `libraries.json` | Libraries, each a list of song ids |
| `media.json` | The media bin and media playlists |
| `timers.json`, `messages.json`, `macros.json` | Timers, messages and macros |
| `stage-layouts.json` | The stage layouts, each kept as a theme's template: `objects` are its objects in order from the back (an item such as the clock is a text object whose role says what it shows; a screen picture names its screen by id, or none for the main one), and `boxes` hold each item's look and place as earlier versions read them |
| `screens.json` | The screens and their looks |
| `preferences.json`, `bible-page.json`, `bible-history.json` | Small choices, how the Bible page was left, and the Bible passages looked up |
| `.softpresenter-library` | Says the folder is SoftPresenter's library |
| `Not saved/` | SoftPresenter's version of a song or theme whose file changed outside the app, kept on quitting before Reload or Keep mine was chosen |
| `Backup before song ids/` | Files as they were before songs were named by id (only where that was done in place) |
| `.opening-pack-<id>/` | A song pack being opened; one a quit left behind goes to the Trash after a day |
| `.last-tidy` | When the library was last tidied (at most once a day: pictures and videos nothing uses, and files left half-saved, go to the Trash) |

The command line's socket, while the app runs, is `~/Library/Caches/SoftPresenter/control/ctl.sock`
(outside Documents, so `sp` reaches the app from any terminal or over ssh), or
`control/ctl.sock` in a folder `SOFTPRESENTER_DATA` names. On Windows it is
`%LOCALAPPDATA%\SoftPresenter\control\ctl.sock`.

Private copies of large font files are made in `~/Library/Caches/SoftPresenter/font copies`
(on Linux, `SoftPresenter/font copies` in the caches folder; with `SOFTPRESENTER_DATA`, `font
copies` in that folder), a folder only the user may enter. Each copy loses its name as soon as it
is opened, so the folder is empty almost all the time; one left by a run that ended at that
moment is cleared the next time a copy is made.

The log is `~/Library/Logs/SoftPresenter/SoftPresenter.log`: what went on screen, commands that
change something (not those that only read, such as `sp status`), problems, and once a minute
how long drawing took.

## How the code is laid out

| Crate | Holds |
|---|---|
| `sp-model` | The data: songs (presentations, groups, slides, objects), themes and layouts, transitions, builds, actions |
| `sp-engine` | The show's state: what is selected, what is on screen, next and previous, clearing layers |
| `sp-render` | Drawing slides on the GPU with vello: screens, thumbnails, transitions |
| `third_party/vello` | Not a workspace crate: vello 0.12.0's main crate with SoftPresenter's change, which sizes the renderer's GPU working memory from what is drawn and checks that a picture fitted the first time it is drawn (a picture too heavy to draw is an error, and the one before it stays up) (`PROVENANCE.md` there, and `docs/licences/vello.md`) |
| `third_party/fontique` | Not a workspace crate: fontique 0.13.0, which finds the computer's fonts for the text layout, with SoftPresenter's change: a font file is mapped only where nothing else can change it (a Mac's read-only system volume, or a private copy left with no name, `mapped.rs`), and SoftPresenter has the rest read into memory itself (once for every thread, never on the screens' thread), so a font file rewritten while the app runs cannot crash it (`PROVENANCE.md` there, and `docs/licences/fontique.md`) |
| `sp-video` | Playing videos: macOS's own player, FFmpeg for other formats; web pages drawn off-screen by macOS's web view (`web`, `web_mac`), copied into frames as a video's are |
| `sp-ndi` | Sending screens over NDI (the runtime is opened when an NDI screen starts) |
| `sp-decklink` | Playing screens out of Blackmagic Design DeckLink cards: finding the devices, their modes and each one's own default format on a thread of its own, choosing the format a new output starts in (`Device::start`), and SoftPresenter's C++ bridge to the driver (`bridge/bridge.cpp`, the only place it is called, through `src/ffi.rs`); the SDK's headers are in `third_party/decklink-sdk` |
| `sp-bible` | Bibles: references, verse numbering, search |
| `sp-app` | The app: the window (egui), the screens' thread (`output.rs`), the views (`home.rs`, `studio.rs`, `editor.rs`, `inspector.rs`), the stage layouts (`stage.rs` for what they hold and show, `stage_editor.rs` for their parts of the Edit view), the command line (`control.rs`, and `slide_cli.rs` for changing slides), the menu bar (`menubar.rs`) |

## Trying the app safely (for agents and tests)

- Every test run sets `SOFTPRESENTER_QUIET=1`: its windows open behind everything, its screens
  never go full screen, it sends no NDI and it opens no DeckLink card. Point it at a copy of the data with
  `SOFTPRESENTER_DATA`, and for the command line set `SOFTPRESENTER_CONTROL_SOCKET` to a short
  path, so nothing reaches the copy someone is using.
- `softpresenter --capture-home FILE.png` saves a picture of the window and quits;
  `SOFTPRESENTER_CAPTURE_AFTER=SECONDS` waits before taking it, and
  `SOFTPRESENTER_CAPTURE_OUTPUT=FILE.png` saves the first screen's picture.
- Testing aids (environment variables) open a view or do something at start:
  `SOFTPRESENTER_GO_LIVE=1` (a slide on screen), `_GO_LIVE_MEDIA=1` (the first media item),
  `_CAPTURE_EDITOR`, `_CAPTURE_BIBLE`, `_CAPTURE_THEMES`, `_CAPTURE_SETTINGS`,
  `_PLAYLIST`, `_OPEN_ITEM=N`, `_MENU=ItemId,ItemId`, `_DROP=FILES` (files dropped on the
  window, separated as in PATH), `_HOVER=FILES` (files kept over the window, for a picture of
  where they would land), `_DROP_AT=X,Y` (where the pointer is for those, in the window's
  points), `_PICK=songs:0,2` (or `media:…`, `items:…` for the playlist in front),
  `_WINDOW_SIZE=WxH`, `_NDI=NAME`, `_HIDE_NDI=1`, `_HIDE_DECKLINK=1`, `_PRETEND_DECKLINK=1` (made-up
  DeckLink devices that play into nothing), `_TRACE_PREVIEW=1`, `_LIVE_TAB=NAME` (open a
  tool tab), `_TAB_SCROLL=POINTS` (scroll the open tab down, for a picture of its foot),
  `_LANGUAGE=ru|uk|en` (as if the computer were set to that language, for the Bible view's start),
  `_FIND_RETURN=WORDS` (types the words in the find box and presses Return once something is
  listed, in any run, so `sp` can be tried on what it opens and picks), `_FIND_DRAG=WORDS@X,Y`
  (types the words, then drags the first result to X,Y in the window's points and holds it
  there, for a picture of a drag; `WORDS@X,Y,drop` lets go there; for a capture run, as macOS
  doesn't draw a quiet copy's window kept behind the others).
- A quiet copy never tidies the library (moves unused pictures to the Trash) unless
  `SOFTPRESENTER_SWEEP=DAYS` is set with a data folder of its own; it then tidies as if every file
  were that many days older. `touch -t` can't make a file look old to it: it goes by the newest of
  a file's modified, created and status-changed times, and a copied picture keeps its original's
  first two.
- The command line drives a quiet copy as it would the real one:
  `SOFTPRESENTER_DATA=… SOFTPRESENTER_CONTROL_SOCKET=… sp status` (see
  [cli.md](cli.md#trying-it-safely)).
