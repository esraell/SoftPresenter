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
│ LIBRARY        + │ │ Song title   8 slides     (i) ≡ ∞ ▤      │ │ ● [AUDIENCE|STAGE]    ON SCREEN│
│   All Songs      │ │ ┌──────┐ ┌──────┐ ┌──────┐                │ │ ┌────────────────────────────┐ │
│   Christmas      │ │ │slide │ │slide │ │slide │   slide grid   │ │ │  what the screens show     │ │
│ PLAYLISTS      + │ │ │1 V1 A│ │2     │ │3 Ch C│   (or Easy /   │ │ └────────────────────────────┘ │
│   Sunday         │ │ └──────┘ └──────┘ └──────┘   Table view)  │ │ 03/08  CHORUS · SONG TITLE     │
│ ──────────────── │ │                                          │ │ (a row for what is playing)    │
│ ALL SONGS    3 + │ │                                          │ │ [Slide]   [Media]  [Props]     │
│   Song one       │ │ [+ Add Slide]   Cut Dissolve Advanced ▾  │ │ [Messages][Audio]  [Announce]  │
│   Song two       │ │                 grid/easy/table  size ── │ │ [          Clear All         ] │
│                  │ ├─────────────────────────────────────────┤ │ Timers Messages Props Macros   │
│ [Search songs]   │ │ ▾ MEDIA  All Media ▾   Dissolve ▾   [+]  │ │ Stage  Audio  (tabs)           │
│                  │ │ [pic] [pic] [video ▶]                    │ │   …the open tab's list…        │
╰──────────────────╯ ╰─────────────────────────────────────────╯ ╰────────────────────────────────╯
```

### Top bar

| Part | Does | Command line |
|---|---|---|
| Show / Bible / Edit | Switches the view. Edit has four kinds, chosen at the top of its left column: Slides, Themes, Props and Stage | `view show\|edit\|bible`; `view themes` opens Edit on the themes |
| Find box | Searches the song library as you type: titles first, then lines of the songs' words. Return opens the song picked, Command-Return (Ctrl on other computers) adds it to the playlist in front, Escape or a click elsewhere ends the search. Command-F goes to it. It does not search the Bible, playlists or media | |
| Clock | The time of day, on a 12-hour or a 24-hour clock as the computer is set (`clock.rs`) | |
| Audience ON/OFF | The audience screens on or off (Command-1) | `audience on\|off` |
| Stage ON/OFF | The stage screens on or off (Command-2) | `stage on\|off` |
| ? | Opens Settings at Key Mappings | |
| ⚙ | Settings: General, Screens, Groups, Key Mappings, Integrations (Command-comma) | `menu Settings` |

### Left column: library and playlists

| Part | Does | Command line |
|---|---|---|
| LIBRARY rows | All Songs, and libraries of your own; + makes one; right-click to rename or delete | `songs`, `libraries`, `library open NAME` (or `all`) |
| PLAYLISTS rows | Playlists; + makes one; drop songs on a row to add them | `playlists`, `playlist open NAME` |
| Song list (a library chosen) | Click opens a song; Command/Shift-click and dragging pick several; drag picked songs onto a playlist or library; Delete asks, then deletes; right-click for Duplicate, Theme, Transition, Reflow, Play on Announcements Layer, Delete… | `select SONG`, `song delete SONG --yes`, `announce SONG` |
| VoxLive row (under LIBRARY, once VoxLive is connected) | The VoxLive item: the church's VoxLive screens, a slide each; right-click to add it to the playlist in front | `voxlive screens`, `voxlive show N\|NAME` |
| Playlist items (a playlist chosen) | Songs, Bible passages, pictures and videos, media playlists, placeholders and headers in running order; drag to reorder; + adds a header or a placeholder; a picture or video dragged from the bin goes in where the line shows, or fills a placeholder; a placeholder's right-click Fill with Song | `playlist [NAME]`, `slides item N [in PLAYLIST]`, `playlist add SONG to PLAYLIST`, `playlist media\|media-playlist\|placeholder NAME`, `playlist fill PLACEHOLDER with SONG` |
| Search | Filters the list (the find box in the top bar searches titles and words) | |

### Middle: the slides

| Part | Does | Command line |
|---|---|---|
| Song header | Title (double-click renames), slide count, and four icons: Info (writer, CCLI), Reflow (edit the words as text), Arrangement (groups in another order), Notes (operator notes) | `slides SONG [--full]` |
| Slide grid | Click puts a slide on screen; right-click picks it out and opens its menu (Edit, Cut, Copy, Paste, Delete…, Group, Media, Transition, Actions, Go to Next, Hot Key, Copy/Paste Text Style, Label, Disable, Template); Command/Shift-click and dragging a box pick several without going live; Delete asks, then deletes | `go SONG SLIDE`, `next`, `previous`, `first`, `last` |
| Playlist view | A song opened from a playlist shows the playlist's items one after another, each under a bar with its name: a song's slides; a picture or video's tile (click to show it); a media playlist's tile (click to play it); a placeholder's tile (Choose a Song…, or drop a picture or video on it) | `playlist go ITEM [SLIDE]` |
| Bottom bar | + Add Slide; the transition for all slides (Cut, Dissolve, Advanced ▾ with 19 kinds); grid, easy (words only) or table view; slide size | `menu NewSlide` |
| Media bin | Pictures and videos; click shows one behind the words; drag onto a slide (its media) or under the slides (a slide of its own); Command/Shift-click or a box picks several; Delete asks; right-click for Scaling, Loop, playlists, Delete…; MEDIA ▾ picks a media playlist (Play This Playlist, Follow a Folder…); the chevron folds the bin away | `media`, `media show ITEM`, `media add FILE…`, `media delete ITEM --yes`, `media-playlist play NAME` |

### Right column: what is on screen

The same in every view. In the Edit view and the Theme Editor the inspector takes the place of
the tabs. Nothing from the preview down to Clear All moves, whatever is on screen.

| Part | Does | Command line |
|---|---|---|
| Preview block | What the audience screens show, drawn by the screens' own renderer (videos move in it); the switch at its head shows the stage picture instead; ON SCREEN or CLEAR at the right. Under the picture, the slide's number in the song as played, its group and the song | `status`, `status --wait SECONDS --since TOKEN` |
| The row under the readout | Whatever is playing: a video's keys (play or pause, start over, forward 10 seconds, a bar to drag through, the time left, loop, sound), the song on the Announcements layer with Clear, or a test pattern with Off. Empty, but there, when nothing plays | |
| Slide, Media, Props, Messages, Audio, Announce | The six layer keys: each is lit while its layer shows, and clears that layer alone (F2 slide, F3 media, F4 props, F5 audio, F6 messages) | `clear slide\|media\|props\|messages\|audio\|announcements` |
| Clear All | Takes everything off the screens (Escape, F1); orange while anything shows | `clear all` |
| Tabs: Timers | Count down, count up, to a time of day; start, stop, reset | `timers`, `timer start\|stop\|reset NAME` |
| Tabs: Messages | Messages with fill-in tokens (the time, a timer), shown over the slides | `messages`, `message show NAME`, `message hide` |
| Tabs: Props | Slides of their own laid over the show (logos, lower thirds) | `props`, `prop show\|hide NAME` |
| Tabs: Macros | Lists of actions done in turn | `macros`, `macro run NAME` |
| Tabs: Stage | Each stage screen's layout, and the stage message | `stage on\|off` |
| Tabs: Audio | Audio playlists: play, pause, skip, volume, shuffle, crossfade | `audio`, `audio play\|pause\|stop`, `audio volume N` |
| Tabs: VoxLive (once VoxLive is connected) | The church's VoxLive screens as buttons: a click puts that screen's captions on the screens in place of a verse | `voxlive show N\|NAME` |

### Keys in the Show view

| Key | Does |
|---|---|
| Space, → | Next slide |
| ← | Previous slide |
| Home, End | First, last slide of the song |
| Escape (F1) | Clear all |
| F2, F3, F4, F5, F6 | Clear slide, media, props, audio, messages |
| A letter | The slide with that hot key, or the group it stands for (A for the first verse, S for the second, C for the chorus, E for the ending…; the letter shows at the right of a group's bar) |
| Delete | Delete what is picked out, after asking |
| Command-N | New song |
| SoftPresenter › Set Up SoftPresenter… | The setup a fresh install starts with: screens, songs, Bibles, permissions |
| File › Import Songs… (or drop song files on the window) | Import songs from text, lyric, ChordPro, rich-text or Word files |
| File › Import from ProPresenter… (or drop a folder) | Bring a library folder across; each folder becomes a library |
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
  deleted in the Props tab of the Show view, and Props is faint until there is one); Stage opens
  the Stage Layouts window. Then the objects of the slide, front first (lock, hide, rename, drag
  to reorder).
- **Canvas**: Add Text, Shapes, Media… (a picture or a video, which plays in its box when the
  slide is on screen), Web Page (a box showing a live web page; its address is typed in the
  Shape tab); pictures and videos can be dropped on it from the Finder. Double-click a
  text box to type in it. The letter keys add objects (T for a text box, one key per shape).
- **Inspector** (in the right column, under Clear All): Shape (place, fill, outline, shadow, flip), Text (words, font, size, colour,
  alignment, spacing, outline, shadow, gradient, linked text such as a clock or a timer), Build
  (how an object comes on and goes off).
- **Footer** (at the foot of the inspector): Slide Notes, Live Edit (changes reach the screens
  as they are made), Done.
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
- Command line: `bible REFERENCE [--show]`.

## Other windows

| Window | Opened from | Holds |
|---|---|---|
| Screens | Settings, Screens; or Screens, Configure Screens… | Audience and stage screens: where each goes (a display, a window, NDI, off), its name, its look (which layers, colour, covered edges); what each is doing now (for NDI, how many receivers are watching, or that nobody is); Identify |
| Settings | ⚙ or Command-comma | General (copyright line, start-up choices), Screens, Groups (names and colours), Key Mappings, Integrations (VoxLive: its API key, its state, the church's screens; click a screen to change its look, with a preview, and save it to VoxLive; `voxlive …`, `voxlive look`) |
| Stage Layouts | Screens, Edit Stage Layouts… | Layouts for the stage screens: now, next, notes, clock, timer, message |

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
| `sp-video` | Playing videos: macOS's own player, FFmpeg for other formats; web pages drawn off-screen by macOS's web view (`web`, `web_mac`), copied into frames as a video's are |
| `sp-ndi` | Sending screens over NDI (the runtime is opened when an NDI screen starts) |
| `sp-bible` | Bibles: references, verse numbering, search |
| `sp-app` | The app: the window (egui), the screens' thread (`output.rs`), the views (`home.rs`, `studio.rs`, `editor.rs`, `inspector.rs`), the command line (`control.rs`, and `slide_cli.rs` for changing slides), the menu bar (`menubar.rs`) |

## Trying the app safely (for agents and tests)

- Every test run sets `SOFTPRESENTER_QUIET=1`: its windows open behind everything, its screens
  never go full screen, and it sends no NDI. Point it at a copy of the data with
  `SOFTPRESENTER_DATA`, and for the command line set `SOFTPRESENTER_CONTROL_SOCKET` to a short
  path, so nothing reaches the copy someone is using.
- `softpresenter --capture-home FILE.png` saves a picture of the window and quits;
  `SOFTPRESENTER_CAPTURE_AFTER=SECONDS` waits before taking it, and
  `SOFTPRESENTER_CAPTURE_OUTPUT=FILE.png` saves the first screen's picture.
- Testing aids (environment variables) open a view or do something at start:
  `SOFTPRESENTER_GO_LIVE=1` (a slide on screen), `_GO_LIVE_MEDIA=1` (the first media item),
  `_CAPTURE_EDITOR`, `_CAPTURE_BIBLE`, `_CAPTURE_THEMES`, `_CAPTURE_SETTINGS`,
  `_PLAYLIST`, `_OPEN_ITEM=N`, `_MENU=ItemId,ItemId`, `_DROP=FILE`, `_PICK=songs:0,2`,
  `_WINDOW_SIZE=WxH`, `_NDI=NAME`, `_HIDE_NDI=1`, `_TRACE_PREVIEW=1`.
- A quiet copy never tidies the library (moves unused pictures to the Trash) unless
  `SOFTPRESENTER_SWEEP=DAYS` is set with a data folder of its own; it then tidies as if every file
  were that many days older. `touch -t` can't make a file look old to it: it goes by the newest of
  a file's modified, created and status-changed times, and a copied picture keeps its original's
  first two.
- The command line drives a quiet copy as it would the real one:
  `SOFTPRESENTER_DATA=… SOFTPRESENTER_CONTROL_SOCKET=… sp status` (see
  [cli.md](cli.md#trying-it-safely)).
