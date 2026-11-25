# A map of SoftPresenter

What the app looks like, what every part of it does, which command-line command does the same,
where it keeps its files, and how the code is laid out. Written for people and AI agents
finding their way around. For the command line itself see [cli.md](cli.md).

## The main window

One window: a bar along the top, and under it three round panels, the left column, the middle
and the right column, the same in the Service and Bible views (in the Edit view, the inspector alone). Three views are chosen in the bar (Service, Bible and Edit). The
operator's window never shows on the audience screens; the screens are separate windows (or NDI
sources) drawn by their own thread.

```
  ▬ SoftPresenter [Show|Bible|Edit] (Find a song or a line) (≡)     12:00:00 [● Audience ON] [● Stage ON] ? ⚙
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
| Service / Bible / Edit | Switches the view. Edit has four kinds, chosen at the top of its left column: Slides, Themes, Props and Stage | `view service\|edit\|bible` (`show` still works); `view themes` opens Edit on the themes |
| Find box | Finds as you type, listed under it: a Bible passage first when the words are a reference (`John 3:16`, `Ин 3:16-18`, `ps 23`), named and quoted in the language it was typed in; then songs: by title, then each slide whose words have them (tagged with its group, `SONG · CHORUS`, the line found under it; picking one brings the view to that slide and rings it in ink for a moment); then verses that hold the words, from the Bibles in use written in their alphabet (each tagged with its Bible, `BIBLE · KJV`). Each result has a mark at its left, so songs and the Bible read apart: a note in the libraries' lilac for a song or a slide of one, its tag lilac too (`SONG_TINT`), an open book in ink for the Bible, its tag dim (`found_look` in `studio.rs`; slides the app makes, Bible passages and live captions, have an ink dot); the titles stay in ink, the words grey. Return or a click opens the one picked (a song in the Service view, a passage or verse in the Bible view; nothing goes on the screens) with its first slide or verse picked (a single verse is the passage, its slides its chapter's with a slide a verse), drawn with an ink ring, and the keyboard back with the show, so Enter puts it on screen; Command-Return (Ctrl on other computers) adds it to the playlist in front, Escape or a click elsewhere ends the search. A result can be dragged (the chooser shows above the left column's list while it is carried): onto the open playlist's items, where the ink line shows (or onto a placeholder); onto a playlist in the chooser, at its end; a song onto a library (row or song list); a passage or verse never into a library. A dropped result ends the search and is opened in the playlist listed; nothing goes on screen (`library_search` in `home.rs`; the drops in `studio.rs`, `Carried`). `/` goes to it, and so does Command-F. It does not search playlists or media (`find.rs`) | |
| Notices (≡) | The status button, 52 × 30 after the find box, always there and the same size: a quiet list icon, a grey dot for new information, amber with a count for trouble, "?" for a question waiting. A click opens the list under it (an egui pop-up): Clear All (a question stays), then questions first and the rest newest first, each with its mark, whole text, time and buttons (Reload and Keep mine on a question). Closing it marks all seen; Escape closes only it. It keeps the last 40, never opens by itself, and is plain data (`notices.rs`: kind, text, time, seen, actions) for a web page to take as it is. Everything the window tells the operator goes through `state.notices` (`info`, `trouble`, `ask`, and `refuse` for a refusal, which is no entry but a brief cue under what was clicked, or under the preview's readout for a key, `notices::paint_cue`) | `notices`, `notices answer N ANSWER`, `notices seen`, `notices clear`; the replies say the same in words |
| Running timer | Just before the clock while a timer runs: its name and time, in green. It takes only room nothing else uses, so nothing moves when it starts or stops | `timers` |
| Canva Sync line | In the same room while no timer runs: what a sync is doing (exporting with the time so far, downloading with how far; a click opens Canva Sync's page). How it ended goes in the notices (the video put in place, or why it failed) | `canva` |
| Remote Control line | In the same room while no timer runs and Canva Sync has nothing to say, while Remote Control is on: REMOTE ON, and how many pages are connected (pointing at it shows the address to open; a click opens Settings › Remote Control) | `remote` |
| Clock | The time of day, on a 12-hour or a 24-hour clock as the computer is set, figures only, no AM or PM (`clock.rs`) | |
| Audience ON/OFF | The audience screens on or off (Shift-Command-1; Ctrl+Shift+1 on other computers) | `audience on\|off` |
| Stage ON/OFF | The stage screens on or off (Shift-Command-2) | `stage on\|off` |
| ? | The list of every key (the `?` key opens it too; Escape, Enter or Done close it) (`keys.rs`) | |
| ⚙ | Settings, its pages in four groups (`home/settings.rs`): The room (Screens: Configure Screens…, Identify and the test patterns; Phone Screens), On screen (Slides, Groups, Language), Connect (Integrations, AI assistants, Phone remote) and Advanced (Keys with Show keys on buttons, Command line, Song files with Reload songs, About with the version, What's new, the guide, Report an issue…, Licences…); each row ends in how its page stands; Set Up SoftPresenter… under the list (Command-comma) | `menu Settings` |

### Left column: a playlist or a library

| Part | Does | Command line |
|---|---|---|
| PLAYLIST or LIBRARY, with the import button | Says which kind of thing is listed; the button imports songs, a library folder, or slides | `song import FILE…` |
| The name of what is listed | A click opens the chooser, or puts it away | |
| The chooser | Only things to open: the playlists (with how many items), then the song libraries under a line (All Songs, then each library); the open one is ticked; + makes one; ⋯ or right-click to rename, export or delete; drop songs (or a picture or video, or a result of the find box) on a row to add them, from the window or the Finder; a row that can't take what is carried shows the no-drop pointer, and All Songs takes nothing from the window (it shows above the list while song files are dragged over the window; the row under the pointer gets an ink outline, `file_drop.rs`) | `playlists`, `playlist open NAME`, `libraries`, `library open NAME` (or `all`) |
| Song list (a library open) | Rows are as tall as their titles (`studio::list_row`, `row_shape`: up to two lines, 42 or 48 high, a version's pill adds 18; the song list keeps each row's height in `SongRowHeights` and lays out only the rows in view); Song files dropped from the Finder go into this library (outlined while they are over the window); click opens a song; Command-click adds or takes away one, Shift-click (or Shift-↑/↓) picks a run from the one last clicked, Command-A picks all, dragging picks several, with the song's name (or how many) beside the pointer from the first move; drag picked songs onto a playlist or library, in the list's order (the chooser shows once they leave the list); Delete asks, then deletes; ⋯ or right-click for Add to Playlist (every song picked with it), Duplicate (the copy beside it: next in the list, after it in its libraries, after its item in a playlist; `HomeState::copy_beside`), Theme, Transition, Edit Words, Play on Announcements Layer, Delete… | `songs`, `select SONG`, `song delete SONG --yes`, `announce SONG` |
| Order: As listed, A–Z, Longest; New song (a library open) | The order the songs are listed in; a new song | `song new TITLE` |
| Playlist items (a playlist open) | A click opens an item (the slides scroll to it; the screens don't change); Command-click, Shift-click, Shift-↑/↓ and Command-A pick several as in the song list (`HomeState::item_picks`), lit in the chosen grey. Songs, Bible passages, pictures and videos, media playlists, placeholders and headers in running order, each with its number, a mark for its kind, its number in orange while it is on screen, and ⋯ for its menu (headers too); an item has one menu, the same from its row and from its title bar over the slides (`studio::item_menu`, the only one: Add Slide and Song ▸ for a song, Play on Announcements Layer for media, Fill with Song, Rename, the Version section, Move Up, Move Down, Remove from Playlist, Delete Song from Library…); drag to reorder (several picked move together, in their order); Delete or Backspace takes the items picked (or the one open) out at once, asking nothing and saying nothing, and Command-Z puts them back (`HomeState::take_out_items` in `removed.rs`, where every delete Undo covers is kept; the step goes in `ShowHistory` in `main.rs`); a picture or video dragged from the bin, or a result dragged from the find box, goes in where the line shows, or fills a placeholder; song files, pictures and videos dropped from the Finder go after the item under the pointer (the playlist is outlined, with a line where they go); a placeholder's menu has Fill with Song; a picture's, video's or media playlist's has Play on Announcements Layer; a song's or passage's has a Version section once a theme has a version (the usual look or a version, Keep It After This Item; `studio::show_with_menu`). An item set to a version has the version's name in a small pill on a second line under its title, lit orange while it is on screen in it | `playlist [NAME]`, `slides item N [in PLAYLIST]`, `playlist fill PLACEHOLDER with SONG`, `playlist remove ITEM [from PLAYLIST]`, `playlist item N show-with NAME\|usual`, `undo` |
| Add to playlist (a playlist open) | Adds a song, a header, a placeholder, a picture or video, a media playlist or the VoxLive item after the open item; A Bible Passage… opens the Bible page. Every add in the window goes the same way (`studio::add_items`: this, a song's Add to the playlist in front, the find box's Command-Return, the Bible page's + Add): after the open item, which it becomes; an entry in the notices only when it went out of sight, a refusal's cue when it was there already. The song pickers mark "In this playlist"; a placeholder is never filled by moving a song the playlist has (`studio::fill_placeholder`) | `playlist add SONG to PLAYLIST`, `playlist header\|media\|media-playlist\|placeholder NAME`, `bible REF --add` |
| Filter | Filters the list (the find box in the top bar finds songs by title and words, and Bible verses) | |

### Middle: the slides

| Part | Does | Command line |
|---|---|---|
| The scroll | With a playlist listed on the left: every item of it, one after another, each under a bar with its name that stays at the top while its slides scroll under it. With a library listed: the song chosen, the same way | `playlist go ITEM [SLIDE]`, `slides SONG [--full]` |
| An item's bar | (Under a song changed outside SoftPresenter, while it waits for an answer: an amber-tinted strip, "This song was changed outside SoftPresenter. Read it again from its file, or keep SoftPresenter's version?" with Reload and Keep mine, the same question as the notices' (`notices::ask_strip`).) A dot when it is the open item; the name (a song's renames on a double click); for a song its tools: details (writer, CCLI), Edit Words, arrangement (groups in another order: a row of pills under the bar, one per piece, ×2 for a group sung twice running, 2/3 while on screen; the slides then stand in the order sung, with what it leaves out under "Not in this arrangement"), the theme version button (once a theme has a version; for a passage too, after its tag: what the item shows in when it goes on screen, in the "on" grey while one is set, its icon orange while the item is on screen in it; the item's tiles are drawn in that version), notes, ⋯ (the song's menu, acting on this song alone, Add Slide, its place in the playlist, then Remove from Playlist and, last, Delete Song from Library…); for anything else a small label saying what it is | `song info`, `song text`, `song arrangement`, `playlist item N show-with NAME`, `song reload SONG`, `song keep SONG` |
| Slides | Tiles (the picture, and a strip with the number, the group's dot and name, tags, a pencil for the editor and ⋯), words only, or a table. Click puts a slide on screen; ⋯ or a right-click opens its menu, which acts on the slides of its song picked with it (`SlideRequest::Edit`, `Group`, `DeleteMany`: one step for Undo each) (Edit, Cut, Copy, Paste, Duplicate, Delete, Move Up, Move Down, Group, Media, Transition, Actions, Go to Next, Hot Key, Copy Style, Paste Style, Label, Disable, Template; nothing set is "None", and following what is set above it is "Usual (…)", naming it: "Usual (Dissolve, 0.4 s)", "Usual (the theme picks)"); slides the app makes (a Bible passage, VoxLive's captions) have no pencil, no ⋯ and no right-click menu, as nothing in it applies to them; Command/Shift-click and dragging a box pick several without going live; Delete deletes the picked slides of one song at once (`home::delete_slides`, `picks_of_one_song`), and Command-Z puts them back, picked | `go SONG SLIDE`, `next`, `previous`, `first`, `last` |
| Other items | A Bible passage's slides, drawn by the Bible theme (click one to put it on screen); a picture or video's tile (click to show it); a media playlist's tile (click to play it), each on the Announcements layer when it is one of the announcements; a placeholder's box (Choose a song…, or drop a picture or video on it); a header as a small capital line | `playlist go ITEM [SLIDE]` |
| The dock: the bar under the slides | The transition for all slides (Cut, Dissolve, Advanced ▾ with every other kind and how long it takes); Continuous; the theme version button once a theme has a version (lit while one is on, its tooltip naming it; its menu: the usual look or a version for every screen whose theme has it, Keep It Until Switched Back, Make or Change Versions…; `versions.rs` decides when it ends); Undo, Redo; Grid, Words Only or Table; slide size. Add Slide is in a song's ⋯ menu and the File menu | `continuous on\|off`, `version use NAME [--keep]`, `version off`, `undo`, `redo`, `menu NewSlide` |
| The dock: the media strip | Pictures and videos with their names, a video with its length at the lower right of its picture (read with its small copy by the bin's readers, `sp_video::length`; `studio::length_chip`, also on a playlist's video tile) (a file's name without its ending, kept for two files that differ only in it, one line ending in … when too long, the whole file name on hover and in menus: `media::label`, `media::ellipsize`); click shows one behind the words (it is ringed in orange), or one of the announcements (what plays or last played on the Announcements layer, what a macro plays there, the Canva Sync playlist's) on the Announcements layer (`announcements::for_media`); drag onto a slide (its media), onto a song's bar or under a song shown by itself (a slide of its own), or onto the playlist (an item, where the ink line shows) or a placeholder (it fills it); Command/Shift-click or a box picks several; Delete asks; ⋯ or right-click for Scaling, Loop, playlists, Play on Announcements Layer, Delete…; the menu beside MEDIA picks a media playlist (Play This Playlist, Play on Announcements Layer, Follow a Folder…, New Media Playlist…, then Add the Sample Backgrounds while any of the seven is not in the bin), whose view shows its items in the order played, drags them into another order, and has Remove from “<playlist>” (and Delete) in place of Delete… (not for one following a folder); the menu at the right is how one gives way to the next; + adds some (an empty bin also has Or use the sample backgrounds beside its big button), and so do pictures and videos dropped on it from the Finder (it opens and lights up for those only; song files dropped on it go to the songs); the chevron folds the strip away | `media`, `media show ITEM`, `media add FILE…`, `media samples`, `media delete ITEM --yes`, `media-playlist play NAME`, `announce media ITEM`, `announce media-playlist NAME` |

### Right column: what is on screen

This is the right column of the Service and Bible views. Nothing from the preview down to Clear
All moves, whatever is on screen. In the Edit view (slides, themes, props, stage layouts) the right column is the inspector
alone, from its top to its foot (`studio::edit_column`): no preview, layer keys or Clear All; the
keyboard clears there (see Keys in the Edit view).

| Part | Does | Command line |
|---|---|---|
| Preview block | What the audience screens show, drawn by the screens' own renderer (videos move in it); the switch at its head shows the stage picture instead; with more than two screens, a small arrow beside it opens a menu to look at one screen, drawn as that screen draws it (its own theme, layers and colour; a stage screen in its own layout; one switched off or not connected says so over the picture), and Audience or Stage goes back; ON SCREEN or CLEAR at the right. Under the picture, the slide's number in the song as played, its group and the song (with no slide on screen and the announcements playing, ANNOUNCEMENTS and what plays there); a second line only when there is something to say ("Looking at Stream", or how far the slide's click builds have got); then a row always there for what plays (Identify first, "Identifying the screens: the room sees it" with Stop, then a video's keys, a test pattern) | `status`, `status --wait SECONDS --since TOKEN` |
| The row under the readout | Whatever is playing: a video's keys (play or pause, start over, forward 10 seconds, a bar to drag through, the time left, loop, sound), for the video on the Announcements layer while it is over the slides, otherwise for the one on the media layer; or a test pattern with Off. Empty, but there, when nothing plays (the Announce key and Clear All take the announcements off) | |
| Slide, Media, Props, Messages, Audio, Announce | The six layer keys: each is lit while its layer shows, and clears that layer alone (off a Mac also F2 slide, F3 media, F4 props, F5 audio, F6 messages; on a Mac no F key is used) | `clear slide\|media\|props\|messages\|audio\|announcements` |
| Clear All | Takes everything off the screens (Escape; off a Mac F1 too); orange while anything shows | `clear all` |
| Tabs: Timers | A card each: count down, count up, to a time of day; Reset, Start or Stop; ⋯ (or right-click) for Edit… and Delete; + makes one | `timers`, `timer start\|stop\|reset NAME`, `timer new\|set\|rename\|delete` |
| Tabs: Messages | A card each, with fill-in tokens (the time, a timer), shown over the slides: Show or Hide; ⋯ for Edit…, Rename, Duplicate, Cut, Copy, Paste, Delete; Words once… shows typed words at once (kept as one message, "Quick message" when made in English, found by its `quick` mark, not its name: `messages::quick_message`) | `messages`, `message show NAME`, `message hide`, `message show --text WORDS` |
| Tabs: Props | Slides of their own laid over the show (logos, lower thirds), a card each with its picture: Show or Hide; ⋯ for Edit in the Editor, Clears Itself, Rename, Duplicate, Delete | `props`, `prop show\|hide NAME`, `prop new\|delete` |
| Tabs: Macros | Steps done in turn, a card each: Run (the name alone does nothing); the chevron opens its steps (each a button that changes it from the steps in groups, its own marked, dragged by its row into a new order, × to remove it; + Add a step offers the same groups: Clear, Messages, Props, Timers, Audience Screens, Go to a Song, Show a Picture or Video, Announcements, and Sync from Canva); ⋯ for Colour, Run When SoftPresenter Opens (start-up is a macro), Rename, Duplicate, Delete | `macros`, `macro run NAME`, `macro new\|add\|step\|move\|remove\|delete`, `macro startup NAME on\|off` |
| Tabs: Stage | Which stage screen (with more than one; all by default); every layout as a tile with a picture of the stage in it, the one in use ticked, a click switches; Edit layouts…; the stage message (Show on stage, Change it, Clear) | `stage on\|off`, `stage layout SCREEN LAYOUT`, `stage message TEXT\|clear`, `stage objects\|picture\|frame LAYOUT …` |
| Tabs: Audio | Audio playlists (chosen at its head; ⋯ for Rename, New Audio Playlist… and Delete…, + for a new one, whose name is then ready to type); the player: a line to drag, previous, −15, play or pause, +15, next, volume with mute, Play through, Shuffle, Loop, Crossfade (`audio.rs`: the playlists, what follows a track, and the player; `audio/stream.rs` decodes each track ahead on its own thread, by the app or FFmpeg; `audio/resample.rs` makes it stereo at the card's rate; `audio/ring.rs` hands it over; `audio/deck.rs` joins, fades and mixes on the sound card's thread); the tracks (click to play, drag to move, ⋯ for Move Up, Move Down, Rename, Remove; Add tracks…) | `audio`, `audio play\|pause\|stop\|next\|previous`, `audio volume N`, `audio shuffle\|through\|loop on\|off`, `audio seek TIME`, `audio add FILE…` |
| Tabs: VoxLive (once VoxLive is connected) | The church's VoxLive screens, a card each: Show puts that screen's captions on the screens in place of a verse (it reads On screen, on an orange-tinted card, while they show) | `voxlive show N\|NAME` |

### Keys in the Service view

| Key | Does |
|---|---|
| Space, → | Next slide. With another item opened (↑, ↓ or a click on its row), starts that item at its first slide; at the end of a song, with Continuous on, goes on into the next song or Bible passage. While a slide picked waits for Enter, goes on with what is on screen; with nothing on screen, Space puts the picked slide on and → / ← move the pick |
| ← | Previous slide; starts another item opened, as Space does; at a song's first slide, with Continuous on, goes back into the item before |
| ↑, ↓ | Opens the playlist item above or below (nothing, and no notice, at either end); in the Bible view under Slides, picks the verse before or after without putting it on screen; on the Bible page, as ← and → (with a verse picked and nothing on screen, moves the pick to the verse above or below; with something on screen, goes on with it) |
| Enter | The slide picked goes on screen (the first slide or verse of what the find box or the reference field just opened, or the verse ↑ or ↓ picked in the Bible view); with none, the open item from its first slide (with a library listed, the song chosen) |
| Home, End | First, last slide of the song |
| Escape (off a Mac, F1 too) | Clear all; with the chooser of playlists and libraries or Edit Words open, Escape closes it instead (Edit Words without keeping what was typed) and clears nothing |
| F2, F3, F4, F5, F6 (off a Mac) | Clear slide, media, props, audio, messages. A Mac's F keys are the system's, so there these are the layer keys' alone ("Button only" in the list of keys) |
| A letter | The slide with that hot key, or the group it stands for (A for the first verse, S for the second, C for the chorus, E for the ending…; the letter shows at the right of a group's bar). In the Bible view a letter or number starts the next reference in the reference field instead |
| Delete | Delete what is picked out: what Undo brings back goes at once (a playlist's items, slides, and in a media playlist's view its items, taken out of it); songs in a library and media bin items ask first, saying "This can't be undone."; in Edit, with nothing picked on the canvas, the list's open one (its Delete key) |
| Command-N | New song |
| Command-1, -2, -3 | Service, Bible (cursor in the reference), Edit (Ctrl on other computers, for all of these) |
| Command-B | Bible as well (Bold while editing a slide) |
| Command-P | The chooser of playlists and libraries |
| Option-1 … 7 (Alt elsewhere), then 1 … 9 | Opens that tool tab and numbers what is in it; a number then starts that timer, shows that message or prop, runs that macro, switches that stage layout or plays that track. The numbers go after four seconds; Escape puts them away and clears nothing |
| Shift-Command-M, then 1 … 9 | Shows that picture or video from the media strip |
| Shift-Command-1, -2 | Audience screens, stage screens, on or off |
| Command-O | The menu of the slide on screen (a verse or caption on screen has none, and a refusal's cue says so) |
| ? | Every key, as a list; the switch at its foot is Show keys on buttons |
| Show Keys on Buttons (the key list's foot and Settings › Keys) | A small amber tag beside everything that has a key, all the time; off, never (kept with the preferences; holding Command no longer shows them) (`keys.rs`) |
| SoftPresenter › Set Up SoftPresenter… | The setup a fresh install starts with: screens, songs, free songbooks (development builds only: `songbooks.rs`), Bibles, permissions |
| File › Import Songs › Song Files…, Words from Text… (or drop song files on the window) | Import songs from song files (SoftPresenter's own, `.pro` documents, ChordPro and lyric files, songbooks) or words in text, rich-text or Word files (`load.rs` lists each kind's endings) |
| File › Import Songs › A Song Bundle… | `.proBundle` bundles (every song in one, in its order; their pictures and videos go into the media bin, `bundle.rs`) and song packs |
| File › Import Songs › A Library Folder… (or drop a folder) | Bring a library folder across; each folder becomes a library; on a Mac song documents and bundles can be chosen too |
| File › Import Slides… and Import › Slides… beside the songs (`HomeState::slides_sheet`; or drop one, or a folder of pictures) | The sheet **Import Slides**: two cards, Pictures as slides (a picture dialog) and A presentation (a file dialog for `.pptx`, `.ppsx`, `.key`, `.odp`, `.ppt`, `.pps` and PDF). A presentation file then opens the sheet **Import “Sermon.pptx”** (`home/import_sheet.rs`, `HomeState::ask_import`; one file at a time, the rest in `AskImport::later`): As Slides (editable, read from the file: `pptx.rs`) or As Pictures (picture slides from an app on this computer that opens it: `convert.rs`); the ringed card is what that kind of file came in as last time (`HomeState::import_as`, kept with the settings), else As Slides; greyed cards give their reason; Enter, a click, Escape and Cancel as the sheet says; the sheets count in `alert_open`, so no key reaches the show. A PDF or pictures are one picture slide each, made on threads, their slides there at once and pictures filling in (`deck.rs`). A deck's pictures still to draw are written down first (`deck::write_job`, in `Pictures to draw` in the data folder) and taken up again at the next start if a quit came first (`Decks::resume`, from `handle_decks`); until a page is drawn its strip reads "Page N…" (`HomeState::pages_waiting`). After As Slides the notice names the slides to check, each wearing an amber Check tag in its strip (`Slide::check`, kept in the saved slide), whose menu has Mark as Checked |
| Import Songs › Free Songbooks… beside the songs (`HomeState::free_songbooks`, `open`) | The window **Free Songbooks** (`free_songbooks::window`): the songbooks shared by churches, by language (the app's language and the computer's first), each with its songs and size, Install (its download's per cent), "Downloaded · its songs come in once nothing is on screen", or Installed with Remove (asking first); the problem last met at the top; whose songs they are under the list. Its asks (`free_songbooks::Ask`) are carried out by `App::handle_free_songbooks` (`free_songbooks_cli.rs`): the download, its checks and the reading on a thread, the songs into the library through `take_songs` once nothing is on screen; `songbooks`, `songbook install|remove`. Escape or Done closes it; it counts in `window_over_the_window`, so no key reaches the show |
| Command-L | Back to the song on screen |

Every command, with its words, is listed by `sp menu`; any of them can be chosen with
`sp menu ITEM`. The Mac's menu bar (`menubar.rs`: `MENUS`) holds six menus of the standard items
(SoftPresenter, File, Edit, View, Window, Help); `COMMANDS` is the longer list `sp menu` uses, and
each of those has its place in the window. Windows and Linux have no menu bar.

### Keys in the Edit view

The keyboard is the editor's: Space, the arrows, Home, End, Enter and the letters never run the
show from here. Only the keys that clear reach the screens (`home_key_command` in `main.rs`):

| Key | Does |
|---|---|
| F1, F2, F3, F4, F5, F6 (off a Mac) | Clear all, slide, media, props, audio, messages, as in the Service view; not while a text box has the keyboard |
| Escape | First whatever it does in the editor: puts back the words being typed in a box, keeps the old name of one being retyped, lets go of the picked objects, closes a menu, a question or the sheet adding a theme version, stops a drag. With none of those (`HomeState::escape_edits`), it clears all, but never as it repeats while held down. On a Mac it is the one key that clears from the Edit view (one layer alone is cleared from the Service view) |

## The Edit view (slides, themes, props, stage)

```
┌──────────────────┬─────────────────────────────────────────────┬───────────────────────────┐
│ SONG             │ ruler ─────────────────────────────────────  │ [Shape] [Text] [Build]   │
│ ▤ Song title     │  [Add Text] [Shapes ▾] [Media…] [Web Page] [NDI Source] [Phone or iPad Screen]  (the open tab is light) │
│ SLIDES           │ ┌───────────────────────────────────────┐    │  font, size, colour,     │
│ Add Copy Del ↑ ↓ │ │                                       │    │  outline, shadow, fill,  │
│ ┌──────────────┐ │ │         the slide, full size          │    │  gradient, spacing,      │
│ └01 ● VERSE 1──┘ │ │   drag to move, handles to resize     │    │  linked text, builds…    │
│ Objects          │ └───────────────────────────────────────┘    │                          │
│   T Main         │ X  Y  W  H                     Grid   Fit    │ [Slide Notes] [Live Edit]│
│   ▢ photo        │                                              │                   [Show] │
└──────────────────┴──────────────────────────────────────────────┴───────────────────────────┘
```

- **Left**: at the top a switch, Slides · Themes · Props · Stage, for what is edited. Under it,
  for Themes the themes and the open theme's templates. Slides, Props and Stage share one look
  (`studio::list_heading`, `list_keys` and `list_picture`): SLIDES, PROPS or STAGE LAYOUTS, a
  row of keys acting on the one open (Add, Duplicate, Delete, ↑, ↓), then each as its picture
  with a strip under it (number and name); a click opens one, a double click renames it in
  place, a drag moves it to the ink line (`home::list_drag`, as the slides do; templates and
  themes too, the built-in theme staying first), a right-click has its other choices (a prop's:
  Clears Itself, Rename, Duplicate, Delete; a layout's: Rename, Duplicate, Delete). Delete
  with nothing picked on the canvas is the list's Delete key. A prop, a layout, a template or a
  version deleted goes at once and Command-Z puts it back, open again: it is kept in the app's
  one history (`removed.rs`, `ShowHistory`), with a step in the Edit view's own undo order
  (`EditStep::Deleted`) so Command-Z there reaches it among the slide's own changes; slides
  deleted ask first. Props with none yet shows the list empty, with Add
  (`home::empty_props_view`). The objects of what is open sit at the foot, front first (lock,
  hide, rename, drag to reorder).
- **Slides** (`home::slide_list_head` and `slide_list`): SONG over the songs to choose from
  (`studio::list_choice` rows: the playlist's songs, or the library's, and the song open;
  click to edit another), then SLIDES and its keys, which act on the slides picked out or the
  one open (Add a slide after it, Duplicate, Delete at once, ↑, ↓), then the slides in the order
  written. A strip has the number, the group's dot and the group's name (and label); the
  slide on screen has an orange strip, slides picked out a faint ring, a slide switched off is
  dimmed. A double click (or Rename Group on the right-click menu, with Duplicate, Delete,
  Move Up and Move Down) retypes the group's name in the strip; dragging moves a slide (and
  those picked out with it) to the ink line. The pictures are 224 wide, the props' and
  layouts' 204: the design's slides view sits 10 in from the column's sides, not 20.
- **Canvas**: Add Text, Shapes, Media… (the row is centred on the slide, or in the Theme Editor stands at the right beside the name of what is open; each tool is as wide as its words, and when they don't fit across the canvas they take a second row, which the slide goes under: `tool_bars` in `home.rs`) (a picture or a video, which plays in its box when the
  slide is on screen), Web Page (a box showing a live web page; its address is typed in the
  Shape tab), NDI Source (a 16:9 box showing a live NDI source, chosen in the Shape tab's NDI Source section: Source menu, Look again, a line saying what is so, Fits, Quality, Sound; `ndi_in.rs`, `inspector::ndi_source`; the picture is received on a thread of its own and put where every drawing thread finds it, `sp_render::show_ndi`, and the screens only look for news in `Output::take_video_picture`); pictures and videos can be dropped on it from the Finder. Double-click a
  text box to type in it. The letter keys add objects (T for a text box, one key per shape).
- **Inspector** (the whole right column): Shape (place, fill, outline, shadow, flip), Text (words, font, size, colour,
  alignment, spacing, outline, shadow, gradient, linked text such as a clock or a timer; its head says
  THE WHOLE BOX, or SELECTED WORDS and the words quoted while words are selected in a box being typed
  in, and font, size, B I U S and colour then change only those words, a half-lit key or "Mixed"
  showing where they differ; Clear word styles, beside Copy and Paste Style, takes every word's own
  look off), Build
  (how an object comes on and goes off).
- **Footer** (at the foot of the inspector): Slide Notes, Live Edit (changes reach the screens
  as they are made), Done.
- **Stage** (Edit › Stage; Screens › Edit Stage Layouts… and the Stage tab's Edit Layouts… open
  it): the layouts for the stage screens, edited with the same canvas, Objects list and
  inspector as a slide. Left: STAGE LAYOUTS, Add, Duplicate, Delete, ↑ and ↓, and each layout as
  a live picture of the stage in it (shared with the Stage tab, `live_tabs::StagePictures`),
  its right-click menu for Rename, Duplicate, Delete. Over the canvas: the layout's name, Add ▾ (Text, a shape, a
  picture or video, a web page, then what the stage shows live: Current Slide, Next Slide, Next
  Label, Clock, Timer, Slide Notes, Song Title, Group, Stage Message, and Screen Picture with
  the audience screens), Undo and Redo; under it two lines saying the keyboard edits here. The
  inspector, with nothing picked, is LAYOUT (its name, the stage screens using it); for an item
  the Text tab starts with what it shows and its own settings (which timer, the clock's form,
  lines of notes); for a screen picture the Shape tab starts with its screen, corners and
  label. The arrows move an object 10, Shift one pixel (on a slide 1, and Shift 10). While it
  is open the keyboard does not run the show, as in the rest of Edit: only the keys that clear
  reach it (see Keys in the Edit view).
- **Themes** (Edit › Themes; View › Themes, Control-T and `sp view themes` open it): the same canvas, tools and inspector, editing a theme's
  layouts (templates). A layout's text boxes say where each kind of words goes; its shapes,
  pictures and videos are drawn behind the words on every slide that uses it. The left column
  is in the slides view's look (`studio::section_head`, `plain_row`, `picture_actions`,
  `picture_card`): THEME with + and the themes as rows (MAIN on the main one; a right-click
  for Make It the Main Theme, Use On, Rename…, Duplicate, Delete…); then TEMPLATES with a
  switch, Usual | each version the theme has, and + Version (a sheet: the name, offered from
  other themes' versions, and which templates it starts as copies of); with a version shown,
  a line about it with ⋯ (Rename… in every theme, Add a Copy Of, Remove It From the theme);
  the row Add, Duplicate, Delete, ↑, ↓ acting on the templates shown; and the templates as
  pictures with a strip (number, name, what it is for; a right-click for Used For, Rename…,
  Duplicate, Delete). Over the canvas, at the left, the theme, the version and the template,
  with the tools at the right.

## The Bible view

```
╭─────────────────────────╮ ╭───────────────────────────────────────────────────────────────────╮
│ [Books|History]  [Hide] │ │ (John 3:16 ) [‹][›] [+ Add|▾] │ [Page|Slides] [Аа Auto] [Options] │
│ BOOK              [⊞|≡] │ │ TRANSLATIONS [SYN] [KJV] [Change]                                 │
│ [Gen][Exo][Lev][Num]…   │ │ John 3  [KJV + SYN]                                               │
│ [Mat][Mar][Luk][Joh]…   │ │  14  And as Moses lifted up the serpent…                          │
│ JOHN · CHAPTER          │ │      И как Моисей вознес змию…                                    │
│ [1][2][3][4][5][6]…     │ │ [16  For God so loved the world…   (picked: well, ring)]          │
│ VERSE · ⇧ FOR A PASSAGE │ │       Ибо так возлюбил Бог мир…                                   │
│ [1][2][3][4][5][6]…     │ │ MEDIA (All Media ▾)                       (Dissolve ▾) +          │
╰─────────────────────────╯ ╰───────────────────────────────────────────────────────────────────╯
```

- The left column, in place of the playlist (`bible_page.rs`, `left_column`): Books | History
  and Hide. Books: the Book heading with a switch at its right, Keypad (a grid icon, the first
  look) or Lists (lines), kept with the preferences (`bible_books`). The keypad: all 66 books as
  keys named short in the operator's language (`book_key`), then the book's chapters, then the
  chapter's verses, in one column that scrolls. The lists: book, chapter and verse side by side,
  each scrolling on its own, opened on what is chosen (the books in full under the testaments).
  Both dim what the first translation lacks and number chapters and verses as the page's Bible
  does (`BiblePage::page_bible`: the first in use of the language the page is read in); both hand
  a click to the one `choose`. History: the passages that were on screen, newest first, a click
  brings one back, and Clear history. Hide puts the column away and the middle takes its room;
  Books, beside the reference field, brings it back (kept with the preferences).
- The bar along the top (`home.rs`, `bible_bar`): the reference field (236 wide when there is
  room; a reference in English, Russian or Ukrainian, or words to find), ‹ and › (a verse longer
  at the start or end), + Add (the playlist in front; ▾ for another), Page | Slides, Аа (the
  language the page is read in: Auto, English, Русский, Українська; `bible_read_in`), Options. In
  a middle too narrow for one row, the right part takes a second.
- The translations line: TRANSLATIONS and each Bible's short name, Change. Opened: a row per
  translation (up to four), each a Bible (None frees it for another row), the box its verse goes
  in, the box its reference goes in, its licence (i) and ×; + Another translation, Manage
  Bibles…, Done.
- The passage (`BiblePage::shown`) is what was asked for: the title, Add and the lit verses
  and keys follow it. Its slides (`BiblePage::held`, one for each of the two places, with the
  passage's place among them) are, with a slide a verse and the passage in one chapter, the whole
  chapter's (`bible::page_deck`), so Space goes on through the chapter; otherwise its own. A
  playlist's passage runs as its own slides (`SlidesOf::Passage`); Open on the Bible Page opens
  it as the page opens any (`SlidesOf::Page`). The history notes the passage asked for
  (`passage_held`).
- Page (`bible_page.rs`, `page_view`): the passage's whole chapter as the page's Bible numbers it,
  a verse to a row with the same verse in the next Bible under it (with that Bible's own reference
  in small faint figures when it numbers it otherwise); the passage's verses bright, the verse on screen tinted
  orange, the verse picked in a well with a ring. A click on a verse with a slide puts that slide
  on screen (its slide is its place among the verses the slides were made from); a click on a
  verse with none looks it up and picks it. Laid out once per passage,
  Bibles and width; only the verses in view are drawn. Slides: the slides made for the passage
  as tiles drawn by the Bible theme, at the Service view's size (`slide_grid`): with a slide a verse
  the chapter's, the passage's in a 2-point faint ring (`SlideDeck::ringed`, from
  `BiblePage::ringed`). Which is kept with the preferences.
- Options: how verses become slides, the theme and layout for Bible slides, and the Bibles
  installed (more can be downloaded).
- The Bibles window (`bible.rs`, `manage_bibles`): a search box, Install from File… (the
  system's window for choosing a file, `handle_bible_file_request` in `main.rs`), Done; the
  Bibles installed, each with Remove (but for the four built in); those offered, by language,
  with Install. A Bible file dropped on the window (`file_drop::Kind::Bible`, found while it
  is held over the window; a file of any other ending is looked into when dropped) or chosen
  is read on a thread of its own (`App::start_bible_work` in `main.rs`, running
  `bible_store::run` with the shelf as it was, `Shelf::known`; `finish_bible_work` takes the
  result in): the app's own `.spbible` as it is (under a name of its own if it names a Bible
  built in or offered), another program's format read by `sp_bible::import` and made into one.
  The same Bible installed again replaces its earlier copy, which goes to the Trash. One that
  doesn't say its name, short name and language waits in the queue of sheets that ask
  (`bible::details_sheet`, a modal with Name, Short name, Language, Cancel and Install; several
  dropped together are asked about in turn), and its Install goes to a thread too.
  `sp bible install FILE --name --short --language` waits for its job.
- Add keeps the passage in a playlist, to open again with these settings.
- Keys: Return in the reference field (or in the find box) picks the passage's first verse and
  gives the keyboard back; ↑ and ↓ pick the verse before or after (never putting it on screen;
  on the page they follow `running::pick_key` as → and ← do, see `bible_pick_keys`);
  Enter puts the verse picked on screen (so does Space with nothing on screen, while → and ← move
  the pick); Space, → and ← then go verse by verse. The pick is `HomeState::picked` (`home.rs`),
  the rules for the keys `running.rs`.
- The lists: a click on a book loads its first chapter, on a chapter that chapter (the whole
  chapter the passage), on a verse that verse alone (Shift for a passage; a double-click opens the
  whole chapter with the verse picked), each with its first verse (or that verse) picked; nothing
  goes on screen from them (`bible_page.rs`, `choose`).
- Words (not a reference) typed in the reference field list the verses holding them as they are
  typed, in place of the page, found on a thread of their own (`find.rs`, `search_page` and
  `Finder`); `sp bible search` lists them too.
- Chapter and verse numbers follow the language: what is typed is read in the numbering of the
  language typed (`bible::numbering_of`: the first Bible in use in it, else English or Synodal,
  else the first translation's), turned into the first translation's for the slides
  (`bible::renumbered`, through the versification tables `sp_bible::convert` reads); a name the
  operator reads is numbered as its language numbers (`bible::named_in`). A playlist's passage
  and a history entry keep their numbering (`SavedPassage::numbering`, `HistoryEntry::numbering`).
- What the operator reads (the reference field after a lookup, the books in the keypad and the
  lists, the page, the verses found, the find box's Bible rows) is in the language chosen with Аа
  (kept with the preferences, `bible_language`), or on Auto in the language the reference was
  typed in, or with nothing typed the session's (at first the computer's); the slides follow the
  rows above whatever the language, and choosing one makes nothing again. Chapter and verse
  numbers are the first row's. The rule is in `bible_language.rs` (`Reading`); the computer's
  language is read once at start in `system_language.rs`.
- **The app's own language** (not the Bible's): `i18n.rs` holds the language in use
  (`Choice`: the computer's, or English, Spanish, Russian, Ukrainian; kept in `Prefs.language`),
  `t`/`ts`/`tf`/`tn` look words up by place key with the English as the fallback, and the tables
  are `crates/sp-app/locales/<language>/<area>.json` (listed by `build.rs`). The Language card is
  `home.rs` (`language_card`, `language_menu`), also used by the setup's first step.
  `SOFTPRESENTER_LANGUAGE=es` stands in for the computer's language in a test copy.
- Command line: `bible REFERENCE [--show]`, `bible view page|slides`, `bible books keypad|lists`,
  `bible language auto|en|ru|uk`.

## Other windows

| Window | Opened from | Holds |
|---|---|---|
| Screens | Settings, Screens; or Screens, Configure Screens… | Audience and stage screens: where each goes (a display, a window, NDI, a DeckLink output with its Keyer first, one choice for an audience screen (Off, Fill and key, Over the card's input; keyed, the screen is see-through with premultiplied soft edges by `Screen::set_keyer`, with a small Soft edges choice under it; Off makes it solid; a keyed screen saved solid is made see-through as it loads, `screens::settle_keying`, which also turns off a stage screen's keyer), then its format (starting in the card's own, as Desktop Video Setup sets it, and saying where the format comes from, or in amber that Desktop Video Setup has another, with Use FORMAT), in `screens/decklink_ui.rs`; `decklink`, `screen set SCREEN output=decklink:N mode=FORMAT keyer=external`), its name, its look (which layers, colour, covered edges); an audience screen's Background, Solid or See-through (clear where nothing is drawn, for lower thirds over a camera in a switcher or streaming app; Soft edges Straight or Premultiplied; not shown for a keyed DeckLink screen), and Key (shows another screen's key, white where solid, beside it on a second display or NDI name), in `screens/background_ui.rs`; an NDI screen's Sound, Send what plays (the sound of videos and audio playlists goes out with its picture; `sp screen set SCREEN sound=on|off`; the mix is made in `sp_video::sound`, which taps each sound output and, through `mac/tap.rs`, the Mac's own video player, and each block is handed to `sp_ndi::Sound` by `output.rs`'s `NdiSound`); what each is doing now (for NDI, how many receivers are watching, or that nobody is; why a key shows black); on Linux, under an NDI screen while NDI isn't installed, Download NDI… (NDI's SDK downloaded, its licence shown with Accept and Cancel, NDI's runtime installed in the data folder's `NDI/`; then the version and date, with Remove: `ndi_download.rs`, the download read in `ndi_download/sdk.rs`; the same under an NDI Source box's line in the inspector); Identify. `sp screen set SCREEN background=… alpha=… key=…`, `sp screens` |
| Settings | ⚙ or Command-comma | Its list in four groups, each row ending in how its page stands (`home/settings.rs`). The room: Screens (how many are going out; the check strip, Identify and the test patterns as a segmented switch; then a card with Configure Screens… and the stage screens On/Off; what happens at start is a macro run when SoftPresenter opens), Phone Screens. On screen: Slides (the copyright line drawn on a small slide, where it falls as four keys, the CCLI licence; the transition for all slides as keys with a picture and More…, Carry on to the next item; Grid, Words only or Table; `home/settings/parts.rs` has the pieces every page is built from), Groups (names and colours), Language (This computer's and each language in its own name with its flag, as keys). Advanced: Keys (every key the list of keys has, from the same list, then the menus' shortcuts), Command line (the sp command's row: whether `sp` is installed, with Install, Use This Copy and Remove), Song files (Read them again: Reload songs; what happens when a song was changed elsewhere), About (a row each for What's new, Help, Something wrong? and Licences). Groups is a card of each group's colour, name and key. Connect: Integrations (one card for each service, the same for all, with its logo (`logos.rs`), name, line, light, how it stands and one key, opening its page with the way back; Worship Leader App only where the song catalog is offered; TimelyChurch greyed as Coming soon until offered. VoxLive: its API key, its state, the church's screens; click a screen to change its look, with a preview, and save it to VoxLive; `voxlive …`, `voxlive look`. Canva Sync: connect with the church's integration or take over the old sync script's sign-in, the presentation and its divider page chosen from its pages' pictures, where the video goes, Sync Now; `canva …`. Worship Leader App, in development builds only: its songbooks on this computer with Remove, then the recommended ones and every other language with Download; `catalog …`.). AI assistants (`mcp/pane.rs`): How much it may do, Off, Prepare or Prepare and run the screens over the two switches, with what Prepare allows and what is never allowed; Connect one, a tile for Claude, ChatGPT, Gemini and Grok with its logo place, where SoftPresenter stands in its apps, Add (Remove once added, Replace where another program has the entry) and Copy setup (a message to paste into the assistant, What gets copied shows it), and Cursor and VS Code under Other apps; kept in `ai-assistants.json`; `sp mcp`, `sp mcp connect|disconnect|status`, docs/mcp.md). Phone remote: on or off, the port, the password, the address to open (by the computer's name, and as figures) and the key's fingerprint, Make a New Certificate, the pages connected with Disconnect, the devices remembered with Forget and Forget All (`remote …`) |
| Phone Screens | Settings, Phone Screens | Showing an iPhone's, iPad's or Mac's screen (its own Screen Mirroring; unofficial, and said so beside the switch): the switch (off until switched on), the name devices see, Ask me each time, who is connected with Stop, the ports. A device that asks is a question in the notices (Allow, Don't Allow). `mirror.rs` starts the separate program `softpresenter-mirror` (crate `sp-mirror`) only while it is on, and reads its events and pictures from pipes; the pictures show in Phone or iPad Screen boxes (`ObjectKind::Mirror`) through the renderer's door for NDI sources (`sp_render::show_mirror`) (`mirror …`) |

## Where the app keeps things

On a Mac, in `~/Documents/SoftPresenter/` (or the folder `SOFTPRESENTER_DATA` names). An
earlier version kept it in `~/Library/Application Support/SoftPresenter/`; the first start
of this one copies it to Documents, checks the copy, and keeps the old folder as
"SoftPresenter (moved to Documents)" (`library_folder.rs`).

| File or folder | Holds |
|---|---|
| `Songs/*.sppres` | Songs made or changed in the app (JSON), each named by its id |
| `Themes/*.sptheme` | Themes (JSON), each with its versions' names and templates under `versions`; `themes-in-use.txt` says which theme songs and Bible slides use |
| `Images/` | Pictures put on slides, and slides made from PDFs and presentations; any nothing uses goes to the Trash after a week (`sweep.rs`) |
| `Media/` | Videos brought in by song packs (a slide names one as `Media/<file>`), and the sample backgrounds when they are added to the media bin (`samples.rs`, under their own names); tidied as `Images/` is |
| `Bibles/` | Downloaded Bibles |
| `NDI/` | On Linux only, the NDI that Download NDI… installed (`ndi_download.rs`): NDI's runtime as `libndi.so.6`, `NDI SDK License Agreement.txt`, `libndi_licenses.txt` and `installed.json` (the version, and the day it was downloaded); `download/` holds NDI's SDK only until it is installed, cancelled or found wrong. sp-ndi looks for the runtime here right after `NDI_RUNTIME_DIR_V6` |
| `Catalog/worshipleaderapp/<language>/` | The song catalog, apart from the library: the song pack as downloaded (`songs-<hash>.qsp`), `index.json` (each song's key, title, first line, place in the pack and a fingerprint of its text; the ETag and Last-Modified the source gave; when it was fetched; the songs the source has dropped) and `status.json` (when it was last checked). `catalog/store.rs` |
| `services.json` | Playlists; a song in them is named by its id |
| `libraries.json` | Libraries, each a list of song ids |
| `media.json` | The media bin and media playlists |
| `timers.json`, `messages.json`, `macros.json` | Timers, messages and macros |
| `canva-sync.json` | Canva Sync's presentation, divider page, last video and the videos it made (nothing secret: the integration's secret and the sign-in are in the computer's store for secrets (the Keychain, Credential Manager on Windows, the keyring on Linux), under SoftPresenter Canva Sync) |
| `.canva-sync.lock` | The lock two copies of the app take to refresh or keep the Canva sign-in |
| `Media/Canva Sync/` | The announcements video Canva Sync made, one at a time, which the media playlist Announcements follows |
| `stage-layouts.json` | The stage layouts, each kept as a theme's template: `objects` are its objects in order from the back (an item such as the clock is a text object whose role says what it shows; a screen picture names its screen by id, or none for the main one), and `boxes` hold each item's look and place as earlier versions read them |
| `screens.json` | The screens and their looks |
| `preferences.json`, `bible-page.json`, `bible-history.json` | Small choices, how the Bible page was left, and the Bible passages looked up |
| `ai-assistants.json` | Settings › AI assistants: whether assistants may use SoftPresenter, and change the screens (both off unless switched on) |
| `.softpresenter-library` | Says the folder is SoftPresenter's library |
| `Not saved/` | SoftPresenter's version of a song or theme whose file changed outside the app, kept on quitting before Reload or Keep mine was chosen |
| `Backup before song ids/` | Files as they were before songs were named by id (only where that was done in place) |
| `Backup before tidying DATE/` | Each song's file as it was before `sp library tidy --apply` changed it (`tidy_cli.rs`); a second tidy the same day makes `… DATE 2/` |
| `.opening-pack-<id>/` | A song pack being opened; one a quit left behind goes to the Trash after a day |
| `.last-tidy` | When the library was last tidied (at most once a day: pictures and videos nothing uses, and files left half-saved, go to the Trash) |
| `remote/` | Remote Control, in a folder only the user may enter: `settings.json` (on or off, the port, the password's argon2id hash, and the devices remembered, each with only its token's SHA-256), `cert.der` and `key.der` (its certificate and key, readable by the user only) |
| `mirror.json` | Phone Screens: on or off, the name devices see, whether to ask each time, and a made-up device id (never the computer's real address). Nothing secret |

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

When the screens are held up, the log says where the time went. The screens' thread
(`output.rs`) times each pass of its loop (the work between two waits) step by step, with the
clock alone (`output/held.rs`). A pass held 100 ms or more writes one `[output]` line: when it
began, its longest step and what that step was for, the time and count of each kind of step,
and what was left over ("other"). The kinds are video fetch (asking the player for its newest
picture) and video copy, scene build, vello paint (with its wait for the GPU, and how many
times it ran the picture and ran it again for room), NDI readback and NDI rows (copying a
picture back from the GPU), hand-over (to NDI and DeckLink, which never waits), surface acquire
(a window's next picture from the window system), blit, present, window set-up, GPU poll,
painter rest, locks, waking the window, pauses taken on purpose, and capture. The line also
gives the time spent within those steps waiting for the locks every drawing thread shares and
writing the log, each DeckLink output's state and last try at opening its device, and each NDI
source's receivers and longest send lately (NDI and DeckLink send on threads of their own).
From a test copy laid out as the church's, its window held back for a second with
`SOFTPRESENTER_HOLD_WINDOW_PICTURE=1000@40` before that wait was taken off the thread:

    [output] The screens' thread was held 1021.7 ms from 11:13:24.243. Longest step: surface acquire for the window of "TV", 1005.0 ms. All steps: surface acquire 1005.0 ms ×1; vello paint 6.1 ms ×5 (GPU wait 2.7 ms, 5 runs, 0 again for room); scene build 0.1 ms ×5; video fetch 0.0 ms ×1; video copy 0.3 ms ×2; NDI readback 0.1 ms ×3; waking the window 0.0 ms ×1; pauses 10.0 ms ×1; other 0.0 ms. Within them: waiting for the pictures' and fonts' locks 0.0 ms, writing the log 0.0 ms. DeckLink "DeckLink Duo (1)": waiting for its device; last tried 1.0 s before the end, for 0.1 ms (not opened). DeckLink "DeckLink Duo (2)": waiting for its device; last tried 1.0 s before the end, for 0.1 ms (not opened). NDI "SPTEST Stage": 1 receiver, longest send lately 46.9 ms. NDI "SPTEST Main": 0 receivers, longest send lately 0.0 ms.

The per-minute `[output]` line adds the longest pass, how many passes were over 50 ms and over
100 ms, how many pictures reached the windows and how many didn't (a covered window shows
none), and how late the thread woke when that was 50 ms or more. Its "N NDI" is how many NDI
sources are switched on (one that could not start included); its "video pictures" are the new
pictures that came in to be shown (videos, and NDI Source boxes), never the pictures sent out
over NDI.

Each NDI source's own thread (`sp-ndi`, `Pace` in `lib.rs`) writes an `[ndi]` line each minute,
counted only while somebody watches it (or NDI can't count its receivers), like the
`[decklink]` lines: new pictures sent (and how many a second, against the rate the
source says it sends); repeated (the same picture sent again because nothing new came for a
second, as it is every second while a slide stays); late (a new picture more than one and a
half pictures' times after the one before, while pictures are moving, at the pictures' own
pace: a thirtieth of a second for a 30 fps video, a 24 fps video's own time for that, which
the source follows slowly from the gaps on time, so a slower video's steady pace is no
lateness); skipped (a picture
handed over that a newer one replaced before it could be sent, the source's thread busy; and
one put off on the screens' thread while the GPU still read back the one before, then passed
over); the longest gap between new pictures while moving; not drawn (the GPU too slow with
it, or unable to draw it); lost on the way back from the GPU, and the longest one took to
come back; the receivers; and the longest send (NDI paces its sending, so a send waits up to
a picture's time for its turn). Pictures count as moving once three gaps in a row were on
time (a playing video). Late gaps (each up to two seconds) then count
once the gaps after them show the pictures went on moving: one more gap of up to a quarter
of a second after a short late gap, three after a longer hold-up (a picture late beside a
hold-up is part of the same stutter, whose longest gap is the hitch); two gaps over a
quarter of a second say the pictures had stopped, and they count nothing. So a slide that
stays, a video that ends, the stage screens' time once a second and slides changed now and
then count nothing, and a hitch is known a picture or three after it ends. With
nobody watching all minute the line says only that. The screens' thread tells a source what
became of its pictures before they reached it (`NdiOutput::note`); the minute's figures are
kept for other code too (`sp_ndi::Watch::last_minute`, reached through the `NdiEntry` each
screen's source has in the window's statuses).

When a new picture comes more than two pictures' times after the one before while pictures
move (a hitch someone watching could see), the source's thread notes it, and the screens'
thread, which looks at each source's count of hitches at the end of every pass (one atomic
load), writes one `[ndi]` line at most every five seconds for each source (the next says how
many more there were): the gap and when it ended; how long after the picture before was sent
this one was handed over, how long it waited for the source's thread and how long it took to
send; and what the screens' thread was doing in that time, from `output/held.rs`, which keeps
the last four passes of a picture's time or more and the last sixteen of the other things
that can hold a picture up (a fade in the windows, a picture the GPU was too slow with, an
NDI picture lost or put off, the video giving no new picture): those that fell in the gap,
and the longest pass then, written as a held pass is. From a test copy playing a video
behind a song, with a receiver watching: a minute of the video, and the line a slide changed
with a 0.4 s dissolve gave before NDI was sent the fade itself (a hold-up reads the same way
now, with what the thread was doing in place of the fade):

    [ndi] NDI "SPTEST Main" at 1920x1080 29.97fps: 1796 pictures sent (29.9/s), 0 repeated, 0 late, 0 skipped, longest gap 46.1 ms, 0 not drawn, 0 lost on the way back from the GPU (back in up to 13.4 ms), receivers 1, send took up to 38.5 ms in the last minute
    [ndi] NDI "SPTEST Main" stood still: 424.2 ms between two new pictures, up to 20:58:26.792, while pictures were moving and somebody watched. The picture was handed over 417.8 ms after the one before was sent, waited 0.0 ms for NDI's thread and took 6.4 ms to send. On the screens' thread then: a fade ran in the windows for 368.0 ms of it; no pass took 33.3 ms or more.

A DeckLink output's thread says when opening its device took 100 ms or more, and the window
says when a screen's window is covered or uncovered and when the displays change (`[screens]`). Where a screen's window goes when displays change is decided by `window_plan.rs` (a pure function: displays, screens' targets and where each window is, in; open, keep, put back, hide out, with the notice for a display that went; tested with made-up displays) and carried out by `App::sync_screens` in `main.rs`. On a Mac the window is placed by `sp_video::cover_display` (`crates/sp-video/src/mac/cover.rs`: level, Spaces behaviour, exact frame), and `sp_video::DisplayWatch` wakes the loop when macOS says the displays changed; elsewhere `App::look_at_displays` looks once a second. Testing aids: `SOFTPRESENTER_SCREENS_FILL=<display name>` (a quiet run's screens fill that one display), with `SOFTPRESENTER_SCREENS_ON_TOP=1` for the real window level; `spikes/virtual-display` makes a virtual display to try it on. A screen's window that fills a display shows a large X in its top-right corner when the pointer keeps moving over it, which switches that screen off: the clock, the place and what a press does are `close_button.rs` (a pure state machine held on each `ScreenWindow`, fed by `App::window_event`'s pointer events and run by `App::tick_close_buttons` from `about_to_wait`, which also shows the pointer while it is up); the screens' thread is told how strongly to draw it (`OutputMsg::CloseButton`) and lays it over the window's picture as that goes into the window (`show_in_window_timed`, with `sp_render::CloseMark`, a small shader pass), never into the window's texture, so NDI, keys, stage screens, the preview and captures never have it; a change of the X alone shows the window's picture again without painting it (`Output::show_close_buttons`). Testing aids: `SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS=1` (screens in an ordinary window get it too) and `SOFTPRESENTER_CLOSE_BUTTON_PLAY=1` (a played pointer moves over the first such window and presses its X; `=move` only moves).

A screen window's next picture to draw into is asked for on a thread of the window's own
(`output/next_picture.rs`). The window system lends a window a few pictures and takes each back
once shown; when it isn't showing the window for a moment, asking for the next one can wait
about a second on a Mac. The screens' thread waits 50 ms at most, then passes the window over (it
keeps the picture it shows) and looks again for 8 ms at each drawing, so NDI, the stage,
DeckLink and the preview go on. The log says when a window was passed over and when its next
picture came:

    [output] The window of "TV" was not given its next picture by the system within 50 ms. The other screens go on; it keeps the picture it shows until the next comes.
    [output] The window of "TV" was given its next picture after 1005 ms.

## How the code is laid out

| Crate | Holds |
|---|---|
| `sp-model` | The data: songs (presentations, groups, slides, objects), themes and layouts, transitions, builds, actions |
| `sp-engine` | The show's state: what is selected, what is on screen, next and previous, clearing layers |
| `sp-render` | Drawing slides on the GPU with vello: screens, thumbnails, transitions |
| `third_party/vello` | Not a workspace crate: vello 0.12.0's main crate with SoftPresenter's change, which sizes the renderer's GPU working memory from what is drawn and checks that a picture fitted the first time it is drawn (a picture too heavy to draw is an error, and the one before it stays up) (`PROVENANCE.md` there, and `docs/licences/vello.md`) |
| `third_party/fontique` | Not a workspace crate: fontique 0.13.0, which finds the computer's fonts for the text layout, with SoftPresenter's change: a font file is mapped only where nothing else can change it (a Mac's read-only system volume, or a private copy left with no name, `mapped.rs`), and SoftPresenter has the rest read into memory itself (once for every thread, never on the screens' thread), so a font file rewritten while the app runs cannot crash it (`PROVENANCE.md` there, and `docs/licences/fontique.md`) |
| `sp-video` | Playing videos: macOS's own player (its pictures can go to the GPU as they are decoded, with no copy: `mac/gpu.rs`), FFmpeg for other formats (`ffmpeg.rs`, whose sound decoding, `decode_sound` and `sound_info`, the audio player shares); web pages drawn off-screen by macOS's web view (`web`, `web_mac`), copied into frames as a video's are; the sound output and the mix NDI sends (`sound.rs`) |
| `sp-ndi` | Sending screens over NDI, and finding and receiving NDI sources for slides (`receive.rs`: a `Finder` and an `Incoming` per source, each on its own thread; the runtime is opened when the first is needed) |
| `sp-decklink` | Playing screens out of Blackmagic Design DeckLink cards: finding the devices, their modes and each one's own default format on a thread of its own, choosing the format a new output starts in (`Device::start`), and SoftPresenter's C++ bridge to the driver (`bridge/bridge.cpp`, the only place it is called, through `src/ffi.rs`); the SDK's headers are in `third_party/decklink-sdk` |
| `sp-bible` | Bibles: references, verse numbering (and judging it from a Bible's chapters), search, and reading Bibles in other programs' formats (`import`: Zefania, OSIS, Beblia, `<b>`/`<c>`/`<v>` XML, USFM, USX, zipped or not, with limits for untrusted files) |
| `sp-app` | The app: the window (egui), the screens' thread (`output.rs`), the views (`home.rs`, `studio.rs`, `editor.rs`, `inspector.rs`), the stage layouts (`stage.rs` for what they hold and show, `stage_editor.rs` for their parts of the Edit view), the command line (`control.rs`, `slide_cli.rs` for changing slides, and `cli_install.rs` for putting `sp` where a terminal finds it: Settings' Command line card, `install-cli`, and once as the app starts), the song catalog (`catalog/`: the songbooks the source publishes and which to recommend (`books.rs`), reading the song pack (`read.rs`), the files (`store.rs`), search (`search.rs`, `fold.rs`), the conditional download (`fetch.rs`), making a song and judging an update (`present.rs`); whether the windows offer it (`catalog::offered`: development builds); `catalog_cli.rs` is the app's side: the `catalog` commands, joining the library, use the newer version, the check on the idle loop, and what the songbook windows ask; `songbooks.rs` is those windows: the setup's Free Songs step and the rows of Settings › Integrations › Worship Leader App; `catalog_songs.rs` is its songs in the window: the find box's part (on a thread of its own, through `find::Asker`), the two places for a song looked at, and the asks that make one join the library), the free songbooks shared by churches (`free_songbooks.rs`: the list with each file's size and SHA-256, the HTTPS-only download and its checks, reading a songbook with the import and marking its songs, which songbooks to recommend, the Free Songbooks window and the setup's part of the Free Songs step; `free_songbooks_cli.rs`: the downloads' threads, bringing the songs in, Remove, `songbooks` and `songbook`), TimelyChurch (`timelychurch/`, hidden until its Worship API ships (`SOFTPRESENTER_TIMELYCHURCH_URL`): the API's requests and replies (`wire.rs`), a song made from its sections, order sung and lined-up translation (`song.rs`), a setlist or plan made into a playlist (`plan.rs`), the requests (`client.rs`), the device token in the secure store (`token.rs`), what is kept on disk (`store.rs`), the connection and its checks and song-use sends on threads of their own (`sync.rs`), putting what a check found into the library and playlists and counting songs shown (`app.rs`), the card's state (`view.rs`, drawn in `integrations.rs`), `sp timelychurch` (`cli.rs`) and the demo church the tests run against (`mock.rs`); see `docs/timelychurch.md`), styled words in a box (`sp-model`'s `spans.rs`, and `text_map.rs` for following them through caps, lists and line changes; drawn in `sp-render`'s `scene.rs`), the menu bar (`menubar.rs`), theme versions while the show runs and their commands (`versions.rs`), what was deleted that Undo puts back (`removed.rs`), the sample backgrounds and how they are added to the media bin (`backgrounds.rs` finds the files, `samples.rs` adds them as one step for Undo), the notices and their status button (`notices.rs`), AI assistants (`mcp.rs` and `mcp/`: `sp mcp`'s server over its standard input and output, both eras of the protocol, questions and tasks (`protocol.rs`), its tools as `sp` commands and the plans of those that ask first (`tools.rs`), its two skills (`skills.rs`, `skills/*/SKILL.md`), which commands the app lets an assistant send and what each needs (`policy.rs`), adding SoftPresenter to assistant apps and `sp mcp connect|disconnect|status` (`connect.rs`), and Settings' card (`pane.rs`); see `docs/mcp.md`), Remote Control (`remote.rs` and `remote/`: the HTTPS and WebSocket server on its own thread (`server.rs`), sign-in and devices (`auth.rs`), the certificate (`cert.rs`), which commands a page may send (`policy.rs`), the versioned state (`hub.rs`, published by `state.rs`), its Settings pane (`pane.rs`) and the page and its sign-in built in (`page/`: `index.html`, `app.js`, `style.css`, `fonts/`, `login.html`, `login.js`, from the design work); the API is in `docs/remote-api.md`) |

### How a fade is drawn

A fade is two whole pictures (`sp_render::build_layers` for the screen's size, built once when the
new slide goes on) and a transition from `sp_render::transition`, which lays them one over the
other with a share each, a clip or a move. The screens' thread (`output.rs`) keeps each window's
fade as a `Fade`: the two scenes, and, when the fade is cached, a texture of each.

- **Cached** (the default): when a fade starts, `Output::cache_sides` paints each of the two
  scenes once into a texture of the window's size (`Painter::cache_whole`, a `CachedSide`). Each step
  then only blends the textures. The kinds that are a plain sum of the two pictures (Dissolve, Fade,
  Fade Black, Fade White: `sp_render::blend_weights`) are one pass over the two textures
  (`sp_render::FadeBlend`, no scene at all); the kinds that clip (Wipe, Split, Iris, Clock, Blinds,
  Checkerboard: `sp_render::fade_cacheable`) are drawn by the renderer from two image scenes
  (`CachedSide::scene`) through the same `transition`. Both textures are let go when the fade ends,
  and the last step is the real new scene, painted as before.
- **Drawn from the slides, as before:** every other kind (those that move or scale a picture would
  sample the texture between its texels); a see-through screen; a fade while a video, an NDI picture
  or a video box is on screen (they move during it); one the GPU was too slow for, or too heavy; and
  the operator's live preview, which is small. `SOFTPRESENTER_FADE_CACHE=0` (or a file
  `fade-cache-off` in the data folder) chooses this for everything.
- **A slide over a playing video** (`Output::steady_scene`): the screens used to lay out and draw the
  whole picture again for every picture of the video. Now, once only the video is moving (the
  drawing `draw_video` asks for with `moving_only`), the part above the media (`sp_render::build_layer_parts`:
  the words, outlines, shadows, props and message) is drawn once on clear into a texture
  (`Painter::cache_split`), and each picture of the video is the video's own part (`build_under`,
  which holds the video's moving picture and so shows each new picture as it comes) with that texture
  drawn over it. It is kept per screen and size, for the audience windows and the NDI and
  DeckLink pictures, until the next whole drawing (any message, a click, the second a clock ticks, a
  new size). Not used while the slide has a video, NDI or web box, the Announcements layer has a
  video, the slide builds or scrolls, screens are identified, or on stage, see-through and key
  screens.
- NDI and DeckLink outputs fade with the windows, by the same clock (`Output::begin_ndi_fades`,
  `NdiFade`): each picture drawn for them (`ndi_groups`) keeps the scene it was last sent
  (`ndi_shown`) and fades from it, cached like a window's when nothing moves. The fade's steps
  give them a picture when one is owed (`fade_owes_ndi`: each new picture of a playing video,
  or one each picture's time of the fastest NDI screen while only the fade moves) and none is on its way back
  from the GPU (`Output::step_ndi_fade`); `send_ndi_of` draws a fading picture as far as the
  fade has got whoever asks, and copies it straight from a window's texture when that window
  has just painted the same picture at NDI's size (`window_is_ndi_picture`). After the fade
  the whole new picture goes with the video's next picture, or at NDI's next picture time
  (`ndi_after_fade`). The fade's log line says how many of its pictures went to NDI and
  DeckLink. Stage screens change at once.
- The time on the stage screens draws only them (`Which::Stage`, and `Which::MovingAndStage`
  with a playing video's picture as a second begins: `video_drawing`), so the slides kept over
  a video stay and the other NDI sources are sent nothing for it.

`crates/sp-render/tests/golden.rs` compares each kind through five points of a fade, cached against
drawn from the slides, with outlined, shadowed words and a translation line over a picture.

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
  `_CAPTURE_EDITOR` (`=selected-typing`: the song in front, its words being typed in), `_CAPTURE_BIBLE` (with `_BIBLE_LOOKUP=REFERENCE`, and `_BIBLE_GO_LIVE=1` to put the verse picked on screen), `_CAPTURE_THEMES` (`=version` shows the theme's first
  version, `=add-version` opens the sheet that adds one), `_CAPTURE_SETTINGS`,
  `_PLAYLIST`, `_OPEN_ITEM=N`, `_MENU=ItemId,ItemId`, `_DROP=FILES` (files dropped on the
  window, separated as in PATH), `_HOVER=FILES` (files kept over the window, for a picture of
  where they would land), `_DROP_AT=X,Y` (where the pointer is for those, in the window's
  points), `_PICK=songs:0,2` (or `media:…`, `items:…` for the playlist in front),
  `_WINDOW_SIZE=WxH`, `_NDI=NAME`, `_HIDE_NDI=1`, `_SLOW_PAGES=MS` (each page of a PDF takes that long longer to draw, to try a quit part way, or to
  see "Page N…"), `_HIDE_PICTURE_APPS=1` (as on a Mac where no app can
  open a presentation to draw it: the greyed As Pictures card, and the sheet with neither), `_HIDE_DECKLINK=1`, `_PRETEND_DECKLINK=1` (made-up
  DeckLink devices that play into nothing), `_TRACE_PREVIEW=1`, `_LIVE_TAB=NAME` (open a
  tool tab), `_TAB_SCROLL=POINTS` (scroll the open tab down, for a picture of its foot),
  `_LANGUAGE=ru|uk|en` (as if the computer were set to that language, for the Bible view's start),
  `_FIND_RETURN=WORDS` (types the words in the find box and presses Return once something is
  listed, in any run, so `sp` can be tried on what it opens and picks), `_FIND_DRAG=WORDS@X,Y`
  (types the words, then drags the first result to X,Y in the window's points and holds it
  there, for a picture of a drag; `WORDS@X,Y,drop` lets go there; for a capture run, as macOS
  doesn't draw a quiet copy's window kept behind the others), `_RENAME=WHAT` (starts
  retyping a name where it is shown, as a double click does: `playlist`, `library`, `song`,
  `header`, `theme`, `template`, `stage-layout`, `group`, `object`, `message`, `prop` or
  `macro`, for a picture beside one taken without it), `_MACRO_STEPS=open` (the first macro's
  steps open; `menu=N` with its step N's menu open, `drag=N>M` with its step N carried over the
  lower half of step M and held there, for a capture run; steps count from 1), `_EDIT=props|stage` (opens the Edit view
  on the props, making one when there is none, or on the stage layouts, in any run, so a quiet
  copy can be watched idling there).
- A quiet copy never tidies the library (moves unused pictures to the Trash) unless
  `SOFTPRESENTER_SWEEP=DAYS` is set with a data folder of its own; it then tidies as if every file
  were that many days older. `touch -t` can't make a file look old to it: it goes by the newest of
  a file's modified, created and status-changed times, and a copied picture keeps its original's
  first two.
- The command line drives a quiet copy as it would the real one:
  `SOFTPRESENTER_DATA=… SOFTPRESENTER_CONTROL_SOCKET=… sp status` (see
  [cli.md](cli.md#trying-it-safely)).
