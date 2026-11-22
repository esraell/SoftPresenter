# Controlling SoftPresenter from the command line

While SoftPresenter runs, `sp` controls it from a terminal, a script or an AI agent. Every
command is carried out by the same code as the window's own buttons, so what the command line
does and what a click does are always the same.

Every example on this page was run against a test copy of the app, with a small library made
for testing (songs written for it, and the public-domain hymn Blessed Assurance); the replies
shown are the ones it gave. Until version 1.0, the commands, their words and their replies may
change, as everything else may (see the README).

- [Install](#install) and [the basics](#the-basics): names, numbers, `--json`, exit codes
- [Trying it safely](#trying-it-safely): a test copy for agents and scripts
- Commands: [what is going on](#what-is-going-on) (and [waiting for a change](#waiting-for-a-change)) ·
  [running the show](#running-the-show) ·
  [media and video](#media-and-video) ·
  [messages, props, timers, macros](#messages-props-timers-and-macros) · [audio](#audio) ·
  [the Bible](#the-bible) · [songs](#songs) · [changing a slide](#changing-a-slide) ·
  [playlists and libraries](#playlists-and-libraries) · [themes](#themes) ·
  [screens and settings](#screens-and-settings) ·
  [undo, jobs and the menu bar](#undo-jobs-and-the-menu-bar)
- [Song text](#song-text) · [song files edited outside the app](#song-files-edited-outside-the-app) ·
  [song packs](#song-packs)
- [Recipes](#recipes) · [replies as JSON](#replies-as-json) ·
  [not on the command line yet](#not-on-the-command-line-yet) ·
  [how it works](#how-it-works-and-who-can-use-it)

## Install

Choose **SoftPresenter › Install Command Line Tool…** (or run the app's program with
`install-cli`). It puts `sp`, and its full name `softpresenter`, in `~/.local/bin` (or Homebrew's
`bin` folder) as links to the app; no administrator password is needed.

    sp                  # what is on screen; starts SoftPresenter first if it isn't running
    sp go "Blessed Assurance" 3
    sp next
    sp clear

`softpresenter COMMAND` and `sp ctl COMMAND` do the same as `sp COMMAND`. From the source,
`cargo run --release -- COMMAND`.

On Windows (which has no tested build yet) there is no installer for `sp`: the commands are
typed after the program itself, `softpresenter.exe COMMAND`, and `sp` alone does not start the
app.

## The basics

- **Names** can be typed in full or in part, in any case and any script: `sp go bless` finds
  "Blessed Assurance", `sp go бытие` finds "Бытие 1:1-31". A part matches anywhere in a name.
  A name that fits several things is never guessed:

      $ sp go hymn
      “hymn” fits several songs: Hymn One (8), Hymn Two (9). Type more of the name, or its number.
      $ sp go "River Song"
      Several songs are called “River Song”: River Song (3), River Song (11). Give its number instead.

- **Numbers** are places in the lists (`sp songs`, `sp slides SONG`, `sp playlist NAME`…), from 1.
  A list changes when things are added or deleted, so list again before using a number.
- **A song's id**, as `sp song info` and the JSON replies (`playlist`, `status`) give it, names
  it too, and never changes: `sp slides e6985576-8dfd-44d8-b81c-99a7dc6a9b54`. It tells apart
  songs that share a title.
- **Names with spaces** can be quoted, or typed as separate words where nothing else follows:
  `sp go blessed assurance 2`, `sp playlist add open fields to friday`. The words `to`, `from`,
  `in`, `at` and `as` part a name from what follows (at the last of them), so quote a name
  that contains one ("O Come to the Altar"). Quote names in `slide` commands, and `key=value`
  settings whose value has spaces (`"layout=Words Only"`).
- **`--json`**, anywhere on the line, prints the whole reply as JSON ([below](#replies-as-json)).
- **`--yes`** is needed by every command that deletes something.
- **`--wait`** makes a command that runs in the background (a pack with videos, a large pack
  import, a Bible download) wait until it is done; see [jobs](#undo-jobs-and-the-menu-bar).
  `status --wait SECONDS` is something else: it waits for a change
  ([below](#waiting-for-a-change)).
- **Files** can be given by full path or relative to the current folder.
- **Exit codes:** 0 when the command was done; 1 when it was refused or not done (an unknown
  name, a missing argument or `--yes`, a value out of range, a change that couldn't be saved,
  the app not running); 2 when the first word is neither a command nor a file.

      $ sp go nosuch
      No song called “nosuch”.
      $ sp nosuch
      sp: no command or file called 'nosuch'. See: sp help
      $ sp status            # with the app closed
      SoftPresenter isn't running (or isn't listening for commands). Open it, then try again.

- **Help:** `sp help` lists every command, and `sp help song` the ones starting with that word.
  Both work with the app closed.
- **Saving and undo:** changes are saved at once. A change to a song or a slide can be undone
  with `sp undo`; changes to playlists, libraries, themes and settings can't, as in the window.
  `sp undo` takes back the last change to a song even when a theme or playlist change came
  after it, and says which song it changed.
- Each command that changes something is written to the app's log with its result (`done` or
  `refused`). Commands that only read are not, so a script or page asking every second or two
  doesn't fill it: `status`, `songs`, `slides`, `slide SONG N` and `slide item …`, `playlists`,
  `playlist [NAME]`,
  `libraries`, `themes`, `screens`, `timers`, `messages`, `props`, `macros`, `groups`,
  `displays`, `bibles`, `search`, `job`, `help`, and `media`, `media-playlist`, `audio`,
  `audio playlists`, `settings`, `menu`, `voxlive` and `voxlive screens` with nothing after them.

## Trying it safely

`sp` drives whichever SoftPresenter is listening, and that may be on the screens in front of a
congregation. Scripts and agents that are trying things out use a test copy of their own:

    export SOFTPRESENTER_QUIET=1                       # windows behind everything, no NDI
    export SOFTPRESENTER_DATA=/path/to/test-library    # its own library, settings and log
    export SOFTPRESENTER_CONTROL_SOCKET=/tmp/sp-test/ctl.sock   # its own socket (keep it short)
    /Applications/SoftPresenter.app/Contents/MacOS/softpresenter &

With the same variables set, `sp` talks to that copy only. If `SOFTPRESENTER_CONTROL_SOCKET` is
set and nothing answers there, `sp` exits 1 and never starts an app. A small wrapper makes the
mistake impossible:

    #!/bin/sh
    SOCK=/tmp/sp-test/ctl.sock
    [ -S "$SOCK" ] || { echo "test copy not running" >&2; exit 99; }
    SOFTPRESENTER_DATA=/path/to/test-library SOFTPRESENTER_CONTROL_SOCKET=$SOCK exec sp "$@"

- An empty data folder starts with the sample song, a playlist and the four Bibles that come
  with the app. For real-looking data, copy a library into it (`~/Documents/SoftPresenter`),
  leaving out `control/` and `screens.json`.
- **Quit the test copy with `sp menu Quit`** through the wrapper. Never quit it by name or path
  (`osascript -e 'quit app "SoftPresenter"'`): macOS sends that to whichever copy it picks.
- A song file opened by path gets its `.sppres` saved beside it; keep test files in a folder
  copied fresh for each run. Songs and Bibles deleted from a test copy go to the owner's Trash.
- Run one test copy at a time; [AGENTS.md](../AGENTS.md) says how to measure it before leaving
  it running.

## What is going on

| Command | Gives |
|---|---|
| `sp` or `status` | What is on screen (song, slide, words), the next slide, the media, any message and props, the announcements layer, a media playlist playing, running timers, the audio, the playlist in front, whether the screens, Continuous Playlist and Identify are on, and the main theme |
| `status --wait SECONDS [--since TOKEN]` | The same, once something has changed since TOKEN (or, with no TOKEN, from now), or when SECONDS (0 to 30) are up; see [waiting for a change](#waiting-for-a-change) |
| `songs` | Every song in the library, numbered; saved Bible passages and slides made from presentations too. A song whose file changed outside the app is marked |
| `slides [SONG]` | A song's slides: number, group, words (lines joined with " / ", main language first), which is on screen, and marks for a slide that is off, labelled or has a hot key |
| `slides item N [in PLAYLIST]` | The slides of a playlist's item N (of the playlist in front if none is named): a song's, or a saved Bible passage's, made as opening it would make them but without opening it, so the Bible page stays as it is |
| `slides SONG --full`, `slides item N [in PLAYLIST] --full` | Every slide as `slide SONG N` gives it (its own settings, its objects, their words, places and looks), in one reply |
| `search WORDS` | Songs with those words in their title or slides, and on which slides |
| `libraries`, `library NAME` | The libraries with how many songs each has (and how many are missing); one library's songs |
| `playlists`, `playlist [NAME]` | The playlists; one playlist's items (the one in front if none is named) |
| `media` | The media bin's pictures and videos |
| `themes`, `timers`, `messages`, `props`, `macros`, `audio` | Each list, with its state: a timer's kind, time and whether it runs; a message's words and whether it shows; a prop on screen; a macro's steps |
| `screens`, `displays`, `decklink`, `stage layouts` | Each screen and what it is doing (for a screen on NDI, how many receivers are watching it, or that nobody is; for one on DeckLink, the format it plays in, or why it isn't playing), and whether it is see-through (with its alpha) or the key of another (and why it shows black, if it does); the connected displays; the DeckLink outputs, numbered, with their formats and keying (or why there are none); the stage layouts and the screens that use them |
| `groups` | The group names that have a jump key, with their colours |
| `settings` | The app's settings |
| `menu` | Every menu bar item: its id and its words |

    $ sp status
    On screen: Blessed Assurance, slide 2 of 7 (Verse 1).
    Words: Heir of salvation, purchase of God / Born of His Spirit, washed in His blood
    Next: slide 3
    Media: Bars.mp4
    Audience screens: on   Stage screens: on
    Song shown in the window: Blessed Assurance
    Announcements: Harbour Lights, slide 1
    Timers running: Timer 00:05:00
    Playlist in front: Sunday (listed in the left column); item last opened from it: Blessed Assurance
    Continuous playlist: off   Identify screens: off   Main theme: Built-in

    $ sp screens
      1. Screen 1: In a window
      2. Stream: Sending as "Sanctuary" to 2 receivers
      3. Lobby feed: On NDI as "Lobby"; nobody is watching

    $ sp slides "River Song"
    River Song
      1. [Verse 1] Down by the river the morning is new / Light on the water, dew on the grass / У реки утро снова встаёт / Свет на воде и роса на траве
      2. [Verse 2] A second verse arrives today / Второй куплет
      3. [Chorus] Sing it again, let the valley reply / Пой снова, пусть долина ответит   (label Instrumental, key Q)

    $ sp playlist friday
    Friday
      1. — Worship —
      2. River Song
      3. River Song
      4. Psalms 117:1-2
      5. media.png (picture)
      6. Sermon slides (placeholder)
      7. Harbour Lights
    $ sp slides "Psalms 117:1-2"
    No song called “Psalms 117:1-2”.
    $ sp slides item 4 in friday
    Psalms 117:1-2
      1. [Psalms 117:1] O praise the LORD, all ye nations: praise him, all ye people. / Praise the LORD, all you nations! Extol him, all you peoples! / Psalms 117:1 / Psalms 117:1
      2. [Psalms 117:2] For his merciful kindness is great toward us: and the truth of the LORD endureth for ever. Praise ye the LORD. / For his loving kindness is great toward us. The LORD’s faithfulness endures forever. Praise the LORD! / Psalms 117:2 / Psalms 117:2
    $ sp slides item 5 in friday
    media.png is a picture, not slides (sp playlist go 5 1 in Friday shows it).
    $ sp slides item 6 in friday
    Sermon slides is a placeholder; it has no slides yet (sp playlist fill Sermon slides with SONG in Friday).
    $ sp slides "Harbour Lights" --full
    Harbour Lights, slide 1 of 2 (Verse 1)
     1. Main: "Lanterns are lit along the shore"
        at x=120 y=240 w=1680 h=600 (the theme's)
        font Helvetica Neue, size 96, weight 700, italic off, colour #ffffff, align Center, scaling TextDown

    Harbour Lights, slide 2 of 2 (Verse 2)
     1. Main: "Harbour lights for evermore"
        at x=120 y=240 w=1680 h=600 (the theme's)
        font Helvetica Neue, size 96, weight 700, italic off, colour #ffffff, align Center, scaling TextDown

`item` followed by a number names a playlist item; a song whose title starts with "Item" and a
number is named by its number, its id, or its title in quotes (`sp slides "Item 3"`).

    $ sp search foretaste
      1. Blessed Assurance (slide 1)
    $ sp timers
      1. Sermon: counts down from 30:00, 00:30:00 (stopped)
      2. Service: counts down to 10:30, 00:00:00 (stopped)
      3. Timer: counts down from 5:00, 00:05:00 (running)
    $ sp macros
      1. Opening: Clear All; Show Prop: Logo; Start Timer: Sermon; Go to Song: Blessed Assurance
    $ sp groups
    A  Verse 1  #4285f4
    S  Verse 2  #4285f4
    …
    C  Chorus 1  #cc4eb4
    …
    $ sp displays
      1. Built-in Retina Display (3024x1964), the main display

An empty list says `(none)`.

### Waiting for a change

A page or script that shows what is going on needn't ask `status` every second.
`status --json` gives a `token` (three parts: what `status` says, the playlists and their
items, and the library: songs and their slides, libraries, the media bin and media playlists).
`status --wait SECONDS --since TOKEN` answers at once if anything has changed since that token,
and otherwise as soon as something does, or when SECONDS are up, with the status and a new
token. Its JSON says which parts changed, in `changed` (`status`, `playlists`, `library`; none
when the time was up), so the page knows whether to read the playlist or the songs again too.
The time a timer shows and how far the audio has played change every second and don't count
(a timer starting or stopping does).

    $ sp status --json | jq -r .data.token
    d86eed1f29b6730e-bfa496941899fa4c-b4a26a3cf80db679
    $ sp status --wait 20 --since d86eed1f29b6730e-bfa496941899fa4c-b4a26a3cf80db679
    On screen: Harbour Lights, slide 1 of 2 (Verse 1).      # two seconds later, after sp go "Harbour Lights"
    …
    $ TOKEN=$(sp status --json | jq -r .data.token)
    $ sp status --wait 4 --since "$TOKEN"     # a timer running, nothing else
    Nothing changed in 4 s.
    $ sp status --wait 31
    A wait is 0 to 30 seconds, not 31.

- Each wait is answered on a thread of its own, so other commands, from the same page or
  anyone else, are answered meanwhile; the app does nothing while it waits.
- At most 8 waits at once: a ninth is refused at once ("8 connections are already waiting for a
  change; ask again in a moment, or without --wait.").
- A wait with no `--since` waits for the next change from now; `--wait 0` answers at once.
- `sp` waits as long as asked. A program talking to the socket itself should allow SECONDS plus
  a little before giving up on the reply.

A page's loop, then: `status --json` once, then `status --wait 25 --since TOKEN` again and again,
each time with the token the last reply gave.

## Running the show

| Command | Does |
|---|---|
| `go SONG [SLIDE]` | Puts a slide on screen (the first if SLIDE is left out), opening the song from the library |
| `playlist go ITEM [SLIDE] [in PLAYLIST]` | Opens a playlist item as a click does: a song, or a passage on the Bible page, and with SLIDE puts that slide on screen. Continuous Playlist then follows the playlist. A picture or video, or a media playlist, has one thing to show: `1` shows it (or plays the media playlist round and round); without it, it is only opened |
| `next`, `previous` (or `back`), `first`, `last` | As the arrow keys, Home and End; a switched-off slide is passed over and an arrangement is followed. Like the keys, `next` and `previous` act on the open item: with another item of the playlist opened (`sp playlist go ITEM` without a slide, a click on its row), they start it at its first slide; at the end of a song, with Continuous on, they go on into the next song or Bible passage (or back into the item before); at either end of the playlist, or with Continuous off, the reply starts with why nothing changed. While a slide picked in the window waits for Enter (what the find box or Scripture Lookup opened), they go on with what is on screen, and with nothing on screen they put nothing on and reply "Press Enter to put the picked verse on screen." (or slide) |
| `group NAME\|LETTER [in SONG]` | A group's first slide on screen: `group chorus`, `group c`, `group "verse 2"` |
| `hotkey KEY` | The slide with that hot key, in the song on screen (or shown) |
| `select SONG` | Shows a song's slides in the window without putting anything on screen |
| `continuous on\|off` | Continuous Playlist: after a song's last slide, Next goes on to the playlist's next item |
| `clear [LAYER]` | Clears `slide`, `media`, `props`, `messages`, `audio`, `announcements`, or `all` (the default) |
| `audience on\|off`, `stage on\|off` | The audience or stage screens (off closes their windows; NDI sends black, or all clear for a see-through screen) |
| `identify on\|off` | Identify Screens |
| `stage message TEXT`, `stage message clear` | Words for the stage screens only (`\n` starts a new line) |
| `stage layout SCREEN LAYOUT` | Which stage layout a stage screen shows |
| `stage objects LAYOUT` | A stage layout's objects, back first as they are drawn: each one's number, name, what it shows (an item such as the clock, words of its own, a shape, a picture, or a screen's picture) and where it is |
| `stage picture LAYOUT main\|SCREEN\|off` | A picture of what an audience screen shows, live, on a stage layout: the layout's screen picture shows `main` (the first audience screen, whichever that is) or the audience screen named, kept by its id so renaming it changes nothing; a layout with none gets one at the top right. `off` takes the layout's screen pictures away. A stage screen is never shown |
| `stage frame LAYOUT OBJECT x= y= w= h=` | Where one of a stage layout's objects sits, in the stage's pixels (1920 × 1080). OBJECT is its number in `stage objects` or its name |
| `test bars\|grid\|white\|off` | A test pattern on the audience screens |
| `view show\|edit\|bible\|themes`, `view reflow [SONG]` | Switches the window's view (`themes` is the Edit view on the themes; `edit` is the Edit view on a song's slides) |

    $ sp go "River Song"
    On screen: River Song, slide 1 of 3 (Verse 1).
    $ sp next
    On screen: River Song, slide 2 of 3 (Verse 2).
    $ sp go "River Song" 9
    River Song has 3 slides; there is no slide 9.
    $ sp playlist go 3 2 in sunday
    On screen: Harbour Lights, slide 2 of 2 (Chorus).
    $ sp playlist go 4 2 in friday       # a saved Bible passage
    Psalms 117:1-2: On screen: Psalms 117:1-2, slide 2 of 2 (Psalms 117:2).
    $ sp go "Blessed Assurance"
    On screen: Blessed Assurance, slide 1 of 7 (Verse 1).
    $ sp group chorus
    On screen: Blessed Assurance, slide 3 of 7 (Chorus).
    $ sp group zzz
    Blessed Assurance has no group zzz. Its groups: Verse 1, Chorus, Verse 2, Verse 3.
    $ sp continuous on
    Continuous playlist on: after a song's last slide, Next goes on to the playlist's next item.
    $ sp clear bogus
    Clear what? “bogus” isn't a layer.
    $ sp stage message "Two minutes\nthen the sermon"
    On the stage screens: “Two minutes
    then the sermon”
    $ sp stage picture "Words and Next" "HDMI main"
    Words and Next shows what HDMI main shows, at x=1240 y=130 w=640 h=360.
    $ sp stage frame "Words and Next" "Screen: HDMI main" x=1200 y=300
    Words and Next: Screen: HDMI main at x=1200 y=300 w=640 h=360.
    $ sp stage picture "Words and Next" Band
    Band is a stage screen; a screen picture shows an audience screen: main, Screen 1, HDMI main.
    $ sp test bars
    Colour bars on the audience screens.

A song put on screen with `go` is opened from the library, so Next stops at its last slide even
with Continuous Playlist on; open it with `playlist go` to run through the playlist.

## Media and video

| Command | Does |
|---|---|
| `media add FILE…` | Adds pictures or videos to the media bin, as dropping them on the window does |
| `media show ITEM`, `media next`, `media previous` | A picture or video behind the words; the next or previous one in the bin (it wraps around) |
| `media set ITEM scaling=fit\|fill\|stretch loop=on\|off` | How an item fills the screen, and whether a video loops |
| `media delete ITEM --yes` | Takes an item out of the bin (the file stays where it is) |
| `video play\|pause\|restart`, `video seek SECONDS`, `video loop on\|off`, `video mute on\|off` | The video behind the words |
| `media-playlist play NAME`, `media-playlist stop` | Plays a media playlist item after item |
| `media-playlist new NAME`, `media-playlist add ITEM… to NAME`, `media-playlist remove ITEM… from NAME`, `media-playlist delete NAME --yes` | Makes and fills media playlists |
| `media-playlist follow NAME FOLDER\|off` | A media playlist that follows a folder: its pictures and videos come and go with the folder's (each joins the bin a moment later, once it has been read; one the folder brought in leaves the bin with its file unless something else uses it) |

    $ sp media add pics/Blue.png pics/Bars.mp4
    Added 2 to the media bin: Blue.png, Bars.mp4.
    $ sp media show bars
    Showing Bars.mp4.
    $ sp video seek 1.5                  # at once
    The video's length isn't known yet (it has only just started); try again in a moment.
    $ sp video seek 1.5                  # a moment later
    The video is at 1.5 seconds.
    $ sp video seek 99
    The video is 3 seconds long.
    $ sp media set blue loop=on
    Blue.png is a picture: loop is for videos.
    $ sp media-playlist play walk-in
    The media playlist Walk-in has nothing in it to play.

## Messages, props, timers and macros

| Command | Does |
|---|---|
| `message show NAME`, `message hide` | Shows a message over the slides, or takes it off |
| `message show --text WORDS` | Shows words at once, kept as the message "Quick message" |
| `message new NAME [WORDS]`, `message set NAME text=… leaves=SECONDS\|never name=…`, `message delete NAME --yes` | Makes and changes messages |
| `prop show NAME`, `prop hide NAME` | A prop (a logo, a lower third) over the show |
| `prop new NAME [PICTURE]`, `prop delete NAME --yes` | A prop showing a picture, or its name in words |
| `timer start\|stop\|reset NAME` | A timer |
| `timer new NAME countdown M:SS \| to HH:MM \| up [to M:SS]` | A timer counting down from a length (`30` alone is 30 minutes), down to a time of day (`10:30am`), or up |
| `timer set NAME countdown …\|to …\|up …\|overrun on\|off\|name NEW`, `timer delete NAME --yes` | Changes a timer, or deletes it |
| `macro run NAME` | Runs a macro's steps in turn |
| `macro new NAME`, `macro add NAME STEP [then STEP…]`, `macro remove NAME N`, `macro delete NAME --yes` | Makes a macro and gives it steps |
| `announce SONG`, `announce stop` | Plays a song's slides on the Announcements layer, or stops it |

Macro steps: `clear all|slide|media|props|messages`, `show|hide message NAME`,
`show|hide prop NAME`, `start|stop|reset timer NAME`, `audience on|off`, `song SONG`,
`media ITEM`. Steps are parted by `then`, or by `;` inside one quoted argument.

    $ sp message new Parking "Car with lights on: ABC 123"
    Made the message Parking: “Car with lights on: ABC 123”.
    $ sp message set Parking leaves=30
    The message Parking: leaves after 30 s.
    $ sp message show --text "Welcome, everyone"
    Showing “Welcome, everyone” (kept as the message Quick message).
    $ sp timer new Sermon countdown 30:00
    Made the timer Sermon: it counts down from 30:00.
    $ sp timer new Service to 10:30am
    Made the timer Service: it counts down to 10:30.
    $ sp timer new Bad countdown abc
    abc isn't a length of time: say 5:00 (minutes and seconds), 1:15:00, or 30 (minutes).
    $ sp prop new Logo pics/Blue.png
    Made the prop Logo, showing a picture.
    $ sp macro new Opening
    Made the macro Opening; give it steps with macro add Opening STEP.
    $ sp macro add Opening clear all then show prop Logo then start timer Sermon then song "Blessed Assurance"
    Opening: added Clear All; Show Prop: Logo; Start Timer: Sermon; Go to Song: Blessed Assurance.
    $ sp macro run opening
    Ran the macro Opening. On screen: Blessed Assurance, slide 1 of 7 (Verse 1).

## Audio

| Command | Does |
|---|---|
| `audio play TRACK`, `audio pause`, `audio stop`, `audio next`, `audio previous` | The audio player (next and previous stay in the playing track's playlist) |
| `audio volume 0-100`, `audio shuffle on\|off`, `audio through on\|off`, `audio crossfade SECONDS` | Volume; a random or the next track after one ends; a crossfade of 0 to 12 seconds |
| `audio add FILE… [to PLAYLIST]` | Adds sound files |
| `audio playlists`, `audio playlist new NAME`, `audio playlist delete NAME --yes` | Audio playlists (deleting one leaves the files) |

    $ sp audio playlist new Walk-in
    Made the audio playlist Walk-in.
    $ sp audio add Welcome.m4a two.aiff to Walk-in
    Added to Walk-in: Welcome, two.
    $ sp audio play welcome
    Playing Welcome.
    $ sp audio crossfade 4
    The next track fades in over the last 4 s of the one before.
    $ sp audio volume 150
    The volume goes from 0 to 100, not 150.

## The Bible

| Command | Does |
|---|---|
| `bible REFERENCE` | Looks a passage up on the Bible page, with the translations and options chosen there |
| `bible REFERENCE --show` | The same, and puts its first verse on screen; `next` and `previous` then go verse by verse |
| `bible REFERENCE --add [to PLAYLIST]` | Looks it up and keeps it in a playlist, as the page's Add to button does |
| `bible search WORDS` | Verses with those words, in the Bibles in use written in their alphabet (in the first translation when none is), each named in its Bible's language with the first translation's numbers; each line names its Bible when more than one has them |
| `bible rows [BIBLE…]` | The translations shown, first to last; with names (short or whole), sets them |
| `bible options [verses=on\|off break=on\|off translation=on\|off reference=verse\|passage\|last\|none]` | Verse numbers, a new slide for each verse, the translation's name, where the reference goes |
| `bible history` | The passages looked up |
| `bibles` | The Bibles installed, and those that can be downloaded |
| `bible install NAME\|FILE`, `bible remove NAME --yes` | Downloads an offered Bible as a job (or installs a `.spbible` file); removes one (the four that come with the app stay) |

References are typed as on the Bible page, in English, Russian or Ukrainian: `John 3:16-18`,
`jn 3:16`, `Psalm 23`, `Genesis 1` (the whole chapter), `Пс 22:1-3`, `Ів 3:16`. Chapter and
verse numbers are read in the first translation's numbering. Replies name the book in the
language the reference was typed in, as the Bible page then names it to the operator (in the
first translation's language when no Bible in use is in that language); the slides are made
from the translations chosen on the Bible page, whatever the language typed, and a playlist
keeps the passage as the first translation names it.

    $ sp bible "John 3:16-18"
    Looked up John 3:16-18; it is on the Bible page.
    $ sp bible "jn 3:16" --show
    John 3:16: On screen: John 3:16, slide 1 of 1 (John 3:16).
    $ sp bible "John 3:16-18" --add to sunday
    Added John 3:16-18 to Sunday.
    $ sp bible "John 99:1"
    John has 21 chapters.
    $ sp bible rows KJV "Синодальный перевод"
    The Bible page shows KJV, then SYN.
    $ sp bible options verses=on break=off reference=last
    Bible options: verse numbers on, a new slide for each verse off, the translation's name off, the last slide names the passage.
    $ sp bible install ASV --wait
    Installed the American Standard Version.
    $ sp bible remove KJV --yes
    The King James Version comes with the app and stays.

With the Russian Synodal first and the King James Version second (as the app starts), a reply
names the passage as it was typed, while the screens and the playlist follow the translations:

    $ sp bible "gen 1 1"
    Looked up Genesis 1:1; it is on the Bible page.
    $ sp bible "Быт 1 1"
    Looked up Бытие 1:1; it is on the Bible page.
    $ sp bible "Ів 3:16"                 # no Ukrainian Bible in use: the first translation's
    Looked up От Иоанна 3:16; it is on the Bible page.
    $ sp bible "jn 3:16" --show
    John 3:16: On screen: От Иоанна 3:16, slide 1 of 1 (От Иоанна 3:16).
    $ sp bible "gen 1 1-2" --add
    Added Genesis 1:1-2 to Service.
    $ sp playlist
    Service
      1. Бытие 1:1-2
    $ sp bible "Psalm 23"                # the Synodal 23rd psalm (the English 24th)
    Looked up Psalms 23:1-10; it is on the Bible page.
    $ sp bible search valley of the shadow
      1. Psalms 22:4: Yea, though I walk through the valley of the shadow of death, …

## Songs

| Command | Does |
|---|---|
| `song new TITLE [FILE.txt] [--library NAME\|none]` | A new song, with its words from a [song text](#song-text) file, or one empty slide |
| `song import FILE… \| FOLDER… [--library NAME\|none]` | Imports songs, as File › Import Songs… does (formats below). A folder becomes a library of its name (and each folder in it another), unless `--library` says where its songs go. A song already in the library is skipped |
| `song text SONG` | Prints the song as song text |
| `song text SONG FILE` | Replaces the song's words from a song text file, keeping the song (its id, theme, playlist places) and its slides' looks, labels and notes where they still match |
| `song rename SONG to TITLE`, `song duplicate SONG [as TITLE]` | Renames or copies a song |
| `song theme SONG THEME\|main`, `song transition SONG KIND [SECONDS]\|none` | The song's theme; its transition (0 to 5 seconds) |
| `song info SONG [artist=… author=… publisher=… year=… ccli=…]` | Reads or sets the song's details (a year is four digits, a CCLI number digits) |
| `song arrangements SONG`, `song arrangement SONG use NAME\|none`, `song arrangement SONG set NAME GROUP…`, `song arrangement SONG delete NAME` | The song's groups in another order; Next follows the one in use |
| `song delete SONG --yes` | Deletes a song (one made in SoftPresenter goes to the Trash) |
| `song reload [SONG [--now]]`, `song keep SONG` | [Song files edited outside the app](#song-files-edited-outside-the-app) |
| `song export …`, `song import FILE.sppack …` | [Song packs](#song-packs) |

New and imported songs go into the library shown in the song list, and the reply says which;
`--library NAME` or `--library none` chooses. Titles should be unique: `song new` and
`song rename` warn when another song has the title.

**What `song import` reads:** plain text in any common encoding (UTF-8, UTF-16, and the older
Windows encodings for Russian, Ukrainian and Western languages), with section names such as
"Verse 1" or "Chorus" on their own lines; song text; lyric files from song services (the
footer's song number, writers and copyright become the song's details); ChordPro (chords taken
out); rich text; Word and OpenDocument files; SoftPresenter song files; ProPresenter 7 `.pro`
documents and `.proBundle` bundles (the song, and the pictures and videos its slides show,
which go into the media bin; the reply names any the bundle doesn't hold, and a bundle over
64 MB is read in the background, with a job number); SoftProjector databases and songbooks. A PowerPoint, Keynote or OpenDocument
presentation, a PDF, or a folder of pictures becomes a presentation of picture slides, made in
the background. Anything else is refused, with the list of what is read.

    $ sp song new "River Song" river.txt
    Made the song River Song.
    $ sp song import "Plain Lyrics.txt" "Morning Star.cho" Harbour.rtf Lanterns.docx
    Imported 4 songs: Plain Lyrics, Morning Star, Harbour Lights, Lanterns.
    $ sp song import "Morning Star.cho"
    Morning Star is already in the library.
    $ sp song import ../lib/Hymns
    Imported 3 songs: Hymn One, Hymn Two, Old Hymn.
    $ sp song text "River Song" river-v2.txt
    River Song now has the words from river-v2.txt: 3 slides.
    $ sp song duplicate "River Song" as "River Song (short)"
    Copied River Song as River Song (short).
    $ sp song transition "River Song" fade black 1.2
    River Song's transition: Fade Black 1.2s.
    $ sp song info "River Song" "artist=QA Band" ccli=12345
    River Song: artist “QA Band”, ccli “12345”.
    $ sp song arrangement "Blessed Assurance" set Short "Verse 1" Chorus "Verse 3" Chorus
    Blessed Assurance: arrangement Short: Verse 1, Chorus, Verse 3, Chorus.
    $ sp song delete "Old Hymn" --yes
    Deleted Old Hymn.

## Changing a slide

| Command | Does |
|---|---|
| `slide SONG N` | A slide's settings (off, label, hot key, transition, template, actions, notes) and objects: number, name, role, words, place and look, with `*` beside settings that are the slide's own rather than the theme's |
| `slide item N SLIDE [in PLAYLIST]` | The same for a slide of a playlist's item N, a saved Bible passage included (read only: a passage's slides are made from the Bible, so `slide text` and the rest don't change them) |
| `slide text SONG N [OBJECT] WORDS` | Sets an object's words (`\n` starts a new line). OBJECT is its number, name or role (`main`, `translation`…), the main text if left out |
| `slide style SONG N\|all [OBJECT] key=value…` | An object's look on one slide or every slide; `key=reset`, or `reset` alone, goes back to the theme's. `slide style keys` lists the settings |
| `slide frame SONG N\|all OBJECT x= y= w= h=` | Where an object sits, in the slide's pixels (1920 × 1080); `reset` puts it back. A box partly or wholly off the slide is warned about |
| `slide add SONG [after N] [group NAME] WORDS` | A new slide, at the end unless after N; with a group, only the new slide is in it |
| `slide delete SONG N --yes`, `slide duplicate SONG N`, `slide move SONG N to M` | Deletes, copies or moves a slide (a moved slide joins the group where it lands) |
| `slide group SONG N NAME` | Slide N starts a group of that name (or renames the group it starts) |
| `slide label SONG N LABEL\|none`, `slide notes SONG N TEXT\|none`, `slide hotkey SONG N KEY\|none` | A slide's label, operator notes and hot key (one letter or digit) |
| `slide enable\|disable SONG N` | A switched-off slide is passed over by Next |
| `slide transition SONG N KIND [SECONDS]\|none`, `slide template SONG N TEMPLATE\|auto`, `slide media SONG N ITEM\|none` | A slide's own transition, template, and the media it puts behind the words |
| `slide web SONG N URL\|off` | A live web page on the slide: the address of its web page box, or a new box filling the slide, behind the words, if it has none (`off` takes it away). Only `https://` addresses (a bare name gets `https://` in front); `sp slide SONG N` lists the box as `Web page: URL`, and its JSON has `"kind": "web"` and `"url"` |

Every change is saved at once, can be undone step by step, redraws the slide's picture, reloads
it in an editor that has it open, and shows at once on a slide that is on screen.

    $ sp slide "River Song" 1
    River Song, slide 1 of 3 (Verse 1)
     1. Main: "Down by the river the morning is new / Light on the water and grass in the dew"
        at x=120 y=170 w=1680 h=400 (the theme's)
        font Helvetica Neue, size 96, weight 700, italic off, colour #ffffff, align Center, scaling TextDown
     2. Translation: "У реки утро снова встаёт / Свет на воде и роса на траве"
        at x=120 y=610 w=1680 h=320 (the theme's)
        font Helvetica Neue, size 64, weight 700, italic off, colour #ff9628, align Center, scaling TextDown

    $ sp slide text "River Song" 1 "Down by the river the morning is new\nLight on the water, dew on the grass"
    River Song, slide 1: Main now says "Down by the river the morning is new / Light on the water, dew on the grass".
    $ sp slide text "River Song" 1 nosuch x
    This slide has no object called nosuch. It has: 1 Main, 2 Translation.
    $ sp slide style "Blessed Assurance" all size=90
    Blessed Assurance, every slide: size 90.
    $ sp slide style "River Song" 1 size=0
    size goes from 1 to 1000, not 0.
    $ sp slide frame "River Song" 1 main x=-5000
    River Song, slide 1: Main at x=-5000 y=170 w=1680 h=400. It is off the slide (1920 x 1080), so it won't show.
    $ sp slide add "Blessed Assurance" after 1 group Pre-Chorus "A new slide"
    Blessed Assurance: added slide 2.
    $ sp slide move "Blessed Assurance" 2 to 5
    Blessed Assurance: moved slide 2 to 5; it is now in Verse 2.
    $ sp slide delete "Blessed Assurance" 5
    This deletes slide 5 of Blessed Assurance. Add --yes to do it (Undo brings it back).
    $ sp slide web "Blessed Assurance" 1 http://127.0.0.1:8765/
    Blessed Assurance: slide 1 shows the web page http://127.0.0.1:8765/.
    $ sp slide web "Blessed Assurance" 2 http://example.com
    http://example.com isn't an address SoftPresenter shows: it must start with https://.
    $ sp slide "Blessed Assurance" 1
    Blessed Assurance, slide 1 of 7 (Verse 1)
     1. Web page: http://127.0.0.1:8765/
        at x=0 y=0 w=1920 h=1080 (its own)
     2. Main: "Blessed assurance, Jesus is mine / Oh, what a foretaste of glory divine"
        at x=120 y=240 w=1680 h=600 (the theme's)
        font Helvetica Neue, size 96, weight 700, italic off, colour #ffffff, align Center, scaling TextDown
    $ sp slide web "Blessed Assurance" 1 off
    Blessed Assurance: slide 1 has no web page.
    $ sp slide item 4 2 in friday
    Psalms 117:1-2, slide 2 of 2 (Psalms 117:2)
     1. Main: "For his merciful kindness is great toward us: and the truth of the LORD endureth for ever. Praise ye the LORD."
        at x=65 y=18 w=1790 h=378 (the theme's)
        font Linux Biolinum, size 96, weight 400, italic off, colour #ffffff, align Left, scaling TextUpOrDown
     2. Translation: "For his loving kindness is great toward us. The LORD’s faithfulness endures forever. Praise the LORD!"
        at x=65 y=540 w=1790 h=378 (the theme's)
        font Linux Biolinum, size 96, weight 400, italic off, colour #ffffff, align Left, scaling TextUpOrDown
     3. Reference: "Psalms 117:2"
        at x=65 y=429 w=1790 h=110 (the theme's)
        font Linux Biolinum, size 44, weight 700, italic off, colour #ffffff, align Right, scaling TextUpOrDown
     4. Translation reference: "Psalms 117:2"
        at x=65 y=951 w=1790 h=110 (the theme's)
        font Linux Biolinum, size 44, weight 700, italic off, colour #ffffff, align Right, scaling TextUpOrDown

The settings `slide style keys` lists (`theme box` takes the same):

    font=NAME            size=1-1000          weight=100-1000|light|regular|medium|semibold|bold|heavy
    italic=on|off        underline=on|off     strike=on|off        shadow=on|off   (yes|no too)
    color=#RRGGBB[AA]    outline=0-100        outline_color=#RRGGBB[AA]
    align=left|center|right|justify           valign=top|middle|bottom
    caps=none|all_caps|small_caps|title_case|start_case
    scaling=none|text_down|text_up|text_up_or_down|container_up_or_down
    line_transform=none|remove_line_returns|line_returns_as_slashes
    line_height=0.1-10   letter_spacing=-50-200  paragraph_spacing=0-500   baseline=-500-500

A picture slide (from a PDF or a presentation) has a picture object and no words: `slide` lists
it and `slide frame` moves it, while `slide text` and `slide style` say "The slide has no words."

## Playlists and libraries

| Command | Does |
|---|---|
| `playlist open NAME` | Puts a playlist in front |
| `playlist add SONG [to PLAYLIST]` | Adds a song at the end (of the playlist in front if none is named); `VoxLive` adds the VoxLive item, once VoxLive is connected |
| `playlist new NAME`, `playlist rename OLD to NEW`, `playlist delete NAME --yes` | Makes, renames or deletes a playlist (the last one stays) |
| `playlist remove ITEM [from PLAYLIST]`, `playlist move ITEM to N [in PLAYLIST]` | Takes out or moves an item, by number or title |
| `playlist header TEXT [at N] [in PLAYLIST]` | A header that groups the items after it |
| `playlist media ITEM [at N] [in PLAYLIST]` | A picture or video from the media bin (the countdown video) |
| `playlist media-playlist NAME [at N] [in PLAYLIST]` | A media playlist (the announcement loop), to play from the playlist |
| `playlist placeholder NAME [at N] [in PLAYLIST]` | A place kept for something to come (the sermon slides) |
| `playlist fill PLACEHOLDER with SONG`, `… with media ITEM` `[in PLAYLIST]` | Puts a song, or a picture or video, in a placeholder's place (a song already in the playlist moves there) |

In `playlist header|media|media-playlist|placeholder`, a last `in` names a playlist only when
there is one called that, and a last `at` a place only when a number follows, so either word
can be part of the name: `playlist media Live at Church.mp4`, `playlist placeholder Sermon in
Romans`.
| `library open NAME\|all` | Shows a library's songs, or every song, in the song list |
| `library new NAME`, `library rename OLD to NEW`, `library delete NAME --yes` | Makes, renames or deletes a library (its songs stay) |
| `library add SONG… to LIBRARY`, `library remove SONG… from LIBRARY` | Puts songs in a library or takes them out (quote each name with spaces) |

    $ sp playlist new Sunday Evening
    Made the playlist Sunday Evening.
    $ sp playlist add "Blessed Assurance" to sunday evening
    Added Blessed Assurance to Sunday Evening.
    $ sp playlist header Opening songs at 1 in sunday evening
    Put the header Opening songs at place 1 in Sunday Evening.
    $ sp playlist "Sunday Evening"
    Sunday Evening
      1. — Opening songs —
      2. Blessed Assurance
      3. John 3:16-18
    $ sp playlist move 3 to 1 in sunday evening
    Moved John 3:16-18 to place 1 in Sunday Evening.
    $ sp playlist remove 9 from sunday evening
    No item called “9”. Sunday Evening has 3 items.
    $ sp library add "Blessed Assurance" "River Song" to Favourites
    Added to Favourites: Blessed Assurance, River Song.

    $ sp playlist media Bars.mp4 at 1 in sunday evening
    Put the video Bars.mp4 at place 1 in Sunday Evening.
    $ sp playlist media-playlist Announcements at 1 in sunday evening
    Put the media playlist Announcements at place 1 in Sunday Evening.
    $ sp playlist placeholder Sermon slides in sunday evening
    Put the placeholder Sermon slides at place 6 in Sunday Evening.
    $ sp playlist "Sunday Evening"
    Sunday Evening
      1. Announcements (media playlist)
      2. Bars.mp4 (video)
      3. John 3:16-18
      4. — Opening songs —
      5. Blessed Assurance
      6. Sermon slides (placeholder)
    $ sp playlist go Announcements 1 in sunday evening
    Playing the media playlist Announcements, round and round.
    $ sp playlist go "Sermon slides" 1 in sunday evening
    Sermon slides is a placeholder; it has nothing to show yet (sp playlist fill Sermon slides with SONG).
    $ sp playlist fill "Sermon slides" with "River Song" in sunday evening
    Filled the placeholder Sermon slides in Sunday Evening with River Song.

A playlist item or library entry whose song no longer exists is kept and listed as
"(missing)"; a picture, video or media playlist that has gone is listed as "(missing picture or
video)" or "(missing media playlist)". A placeholder, once filled, becomes the song, picture or
video that fills it, as in the window, so it is listed as that.

The JSON of `playlist` has an object for each item: its number (`item`), `kind` (`song`,
`passage`, `header`, `media`, `media_playlist`, `placeholder`, or `voxlive`), `title` as the
playlist shows it, `missing`, and `text`, the line above. A song has its `id` and its `number`
in `sp songs`; a picture or video its `number` in `sp media` and `media_type` (`picture` or
`video`); a media playlist its `number` in `sp media-playlist`; a passage the `bibles` it was
kept with:

```json
[
  {"item": 1, "kind": "header", "title": "Worship", "missing": false, "text": "— Worship —"},
  {"item": 3, "kind": "song", "title": "River Song", "missing": false, "text": "River Song",
   "id": "e6985576-8dfd-44d8-b81c-99a7dc6a9b54", "number": 3},
  {"item": 4, "kind": "passage", "title": "Psalms 117:1-2", "missing": false,
   "text": "Psalms 117:1-2", "bibles": ["kjv", "web"]},
  {"item": 5, "kind": "media", "title": "media.png", "missing": false,
   "text": "media.png (picture)", "number": 1, "media_type": "picture"},
  {"item": 6, "kind": "placeholder", "title": "Sermon slides", "missing": false,
   "text": "Sermon slides (placeholder)"}
]
```

## Themes

| Command | Does |
|---|---|
| `themes`, `theme templates THEME` | The themes; a theme's templates, what each is for, and their boxes (font, size, place) |
| `theme main THEME` | The theme songs and Bible slides use unless they set their own |
| `theme duplicate THEME [as NAME]`, `theme rename THEME to NAME`, `theme delete THEME --yes` | Copies, renames or deletes a theme (Built-in and the main theme stay) |
| `theme box THEME TEMPLATE BOX key=value…` | A template box's look (the `slide style` keys) and place (`x= y= w= h=`) |
| `theme template THEME TEMPLATE for=songs\|bible\|both\|none` | What a template is for (a theme has one Bible template) |

    $ sp theme templates Built-in
    Built-in
      1. Two languages (for songs)
           box 1: Main — Helvetica Neue 96pt, at x=120 y=170 w=1680 h=400
           box 2: Translation — Helvetica Neue 64pt, at x=120 y=610 w=1680 h=320
           box 3: Reference — Helvetica Neue 44pt, at x=120 y=950 w=1680 h=80
      2. One language (for songs)
           box 1: Main — Helvetica Neue 96pt, at x=120 y=240 w=1680 h=600
           box 2: Reference — Helvetica Neue 44pt, at x=120 y=950 w=1680 h=80
      3. Bible (for Bible)
           box 1: Main — Linux Biolinum 96pt, at x=65 y=18 w=1790 h=378
           …
    $ sp theme duplicate Built-in as Evening
    Copied Built-in as Evening.
    $ sp theme box Evening "One language" main size=90 color=#ffcc00 y=600 h=400
    Evening, One language, Main: size 90, color #ffcc00, at x=120 y=600 w=1680 h=400.
    $ sp theme main Evening
    Evening is the main theme: songs that don't set their own use it.
    $ sp theme delete Built-in --yes
    The Built-in theme stays; it can be changed but not deleted.

## Screens and settings

| Command | Does |
|---|---|
| `screen add audience\|stage NAME [on display NAME\|window\|ndi NAME\|decklink NAME\|automatic]` | A new screen (in a window unless told); a DeckLink output by its name or its number in `sp decklink` |
| `screen set SCREEN key=value…` | `name=`, `output=display:NAME\|window\|ndi:NAME\|decklink:NAME\|automatic\|off`, `mode=FORMAT` (a DeckLink screen's format: `1080i59.94`, `720p50`…), `keyer=off\|external\|internal` (a DeckLink card's keyer: fill and key on two connectors, or over the card's input), `kind=audience\|stage`, `layers=slides,media,props,messages,announcements` (or `all`), `color=#RRGGBB`, `edges=L,T,R,B` (0 to 540 each), `theme=THEME\|main`, `layout=STAGE-LAYOUT`; for an audience screen `background=solid\|see-through`, `alpha=straight\|premultiplied` (how a see-through screen sends its soft edges; straight unless told), `key=SCREEN\|none` (shows that screen's key) |
| `screen delete SCREEN --yes` | Deletes a screen (there is always one) |
| `settings set key=value…` | `transition=KIND[:SECONDS]`, `media_transition=…`, `copyright=never\|first\|last\|every`, `ccli_licence=DIGITS`, `start_audience=on\|off`, `start_with=nothing\|media-playlist:NAME\|announcements:SONG`, `continuous=on\|off`, `view=grid\|easy\|table`, `columns=1-12` |

    $ sp screen add stage Band
    Added the stage screen Band, on a window.
    $ sp screen add audience Stream on ndi Sanctuary
    Added the audience screen Stream, on NDI as Sanctuary.
    $ sp screen add audience Lobby on display "Lobby TV"
    Added the audience screen Lobby, on the display Lobby TV (not connected now; the screen waits for it; connected: Built-in Retina Display).
    $ sp screen set Band "layout=Words Only"
    The screen Band: stage layout Words Only.
    $ sp screen set Lobby layers=media color=#336699
    The screen Lobby: shows media, colour #336699.
    $ sp settings set copyright=last ccli_licence=1234567 view=easy columns=4
    Settings: copyright line on a song's last slide, CCLI licence 1234567, easy view, 4 slides across.
    $ sp settings set columns=40
    columns is 1 to 12.

A screen set to a display that isn't plugged in keeps it by name and waits for it. Displays are
named as macOS names them (`sp displays`); the main display, with the menu bar, is never filled.

A screen can play out of a Blackmagic Design DeckLink card, where Blackmagic's Desktop Video (16.0 or
later) is installed. `sp decklink` lists the outputs as the driver names them (a DeckLink Duo 2
shows four, one for each SDI connector), with the formats each plays in and whether it can key;
a new DeckLink screen starts in 1080i59.94 (or the nearest the device has), and `mode=` sets
the format the switcher or projector expects. Each output plays one screen. A screen keeps its
output while the card is away and plays out of it again when it is back, never out of another
card. A quiet test copy (`SOFTPRESENTER_QUIET`) never plays out of a card; with
`SOFTPRESENTER_PRETEND_DECKLINK=1` it lists made-up devices that play into nothing, to try this.
On such a test copy (a real card is listed by its own name, "DeckLink Duo (1)"):

    $ sp decklink
      1. Pretend DeckLink (1): SDI, no keying; formats 720p50, 720p59.94, 720p60, 1080i50, 1080i59.94, 1080i60, 1080p23.98, 1080p24, 1080p25, 1080p29.97, 1080p30, 1080p50, 1080p59.94, 1080p60; a new output starts in 1080i59.94
      …
      5. Pretend DeckLink Keyer: SDI, external and internal keying; formats 720p50, …; a new output starts in 1080i59.94
    $ sp screen add audience Switcher on decklink 1
    Added the audience screen Switcher, on Pretend DeckLink (1) at 1080i59.94.
    $ sp screen set Switcher mode=1080p29.97
    The screen Switcher: in 1080p29.97.
    $ sp screens
      1. Screen 1: In a window
      2. Switcher: Playing at 1080p29.97
    $ sp screen set Switcher mode=1080i47
    Pretend DeckLink (1) has no format 1080i47. Its formats: 720p50, 720p59.94, 720p60, 1080i50, 1080i59.94, 1080i60, 1080p23.98, 1080p24, 1080p25, 1080p29.97, 1080p30, 1080p50, 1080p59.94, 1080p60.

Where the card can key (`sp decklink` says so; a DeckLink Duo 2 can once Desktop Video Setup
pairs its connectors), the card itself can send a see-through screen's fill and key, in step:

    $ sp screen set Switcher keyer=external
    Pretend DeckLink (1) can't do external keying in its card's present profile (it can do no keying).
    $ sp screen set Switcher output=decklink:5
    The screen Switcher: on Pretend DeckLink Keyer at 1080i59.94.
    $ sp screen set Switcher background=see-through keyer=external
    The screen Switcher: fill and key from the card's keyer, see-through, straight alpha.
    $ sp screens
      1. Screen 1: In a window
      2. Switcher: Playing at 1080i59.94, fill and key; see-through, straight alpha

Without the driver, `sp decklink` says Desktop Video isn't installed (or is too old) and there
are no DeckLink outputs to choose; a screen saved with one says the same in `sp screens`.

### Checking a DeckLink card

`softpresenter decklink-check` is the app's own program run from a terminal, with no app
needed (on a Mac, `/Applications/SoftPresenter.app/Contents/MacOS/softpresenter decklink-check`,
or wherever the app is). It lists the devices, the formats each plays in (B: 8-bit BGRA, Y: 8-bit
YUV, K: can key) and whether each can key, and with `play` sends a test picture to one device:

    softpresenter decklink-check
    softpresenter decklink-check play 1 --seconds 10
    softpresenter decklink-check play "DeckLink Duo (2)" --format 1080p29.97
    softpresenter decklink-check play 1 --key external --seconds 20

The picture is colour bars with a white bar moving across them, so a freeze or a stutter shows;
with `--key`, the card's keyer is on and the bottom half is clear but for a white ramp from clear
to solid and a soft-edged moving bar, to check the key (`--alpha premultiplied` sends it
premultiplied). Nothing is sent unless a device is named; the check says what it will send
where, and waits for `yes` (or `--yes`), before it does. It plays through the same output the
app uses, stops after the seconds asked for (10 unless told), and says how many frames played,
late and dropped. From the source, `cargo run -p sp-decklink --example decklink_check -- …` is
the same.

A see-through screen is clear wherever nothing is drawn, for a video switcher or streaming
software to lay the words over its camera picture: the media, the themes' backgrounds and the
screen's colour are left out (features.md says what stays). Over NDI it carries its clear parts.
For a switcher that takes the words and their shape as two inputs, a second screen is made its
key, on a second display or NDI name of the same kind:

    $ sp screen set Stream background=see-through
    The screen Stream: see-through, straight alpha.
    $ sp screen set Stream alpha=premultiplied
    The screen Stream: premultiplied alpha.
    $ sp screen add audience Stream key on ndi Sanctuary key
    Added the audience screen Stream key, on NDI as Sanctuary key.
    $ sp screen set "Stream key" key=Stream
    The screen Stream key: the key of Stream.
    $ sp screens
      1. Screen 1: Filling "3DTV", 1920 x 1080
      2. Stream: Sending as "Sanctuary" to 1 receiver; see-through, premultiplied alpha
      3. Stream key: Sending as "Sanctuary key" to 1 receiver; key of "Stream"
    $ sp screen set "Stream key" output=window
    The screen Stream key: on a window.
    $ sp screens
      …
      3. Stream key: In a window; key of "Stream" ("Stream" is on NDI. A key goes out beside its fill on two displays or windows, or on two NDI names, so this shows black.)
    $ sp screen set Lobby alpha=straight
    Lobby is solid; add background=see-through to send it see-through with that alpha.
    $ sp screen set "Stream key" key=none
    The screen Stream key: not a key.

The JSON of `screens` gives each screen's `background` (`solid` or `see-through`), `alpha`
(`straight`, `premultiplied`, or null), `key_of` (the name of the screen whose key it shows, or
null) and `key_trouble` (why a key shows black, or null), beside `name`, `doing` and `shown`.

## VoxLive

[VoxLive](https://voxlive.app) translates the sermon live. Connected with an API key made in
VoxLive (Settings › API keys, with the screens permissions), SoftPresenter finds the church's
VoxLive screens.

| Command | Does |
|---|---|
| `voxlive` | Whether VoxLive is connected (the key's first characters, VoxLive's handle for it) and how many screens it has |
| `voxlive screens` | The church's VoxLive screens: name, language, captions or Scripture (JSON: `id`, `name`, `language`, `mode`) |
| `voxlive connect` | Connects with a key read from the standard input, never from an argument (an argument would stay in the shell's history): pipe it in or paste it. The key is checked in the background; `voxlive` then says how it went |
| `voxlive show N\|NAME` | Puts that VoxLive screen's captions on the screens, in place of what is there, as a click in the VoxLive tab does (`go` and `next` then move on as usual) |
| `voxlive look N\|NAME` | That screen's look: every setting VoxLive describes, with its value and its choices or range (JSON: the look's settings) |
| `voxlive look N\|NAME setting=value …` | Saves those settings of the screen's look to VoxLive (the screen changes at once, wherever it shows). Each is checked against VoxLive's description first: one of its values, a colour such as `#FFE14D`, `on` or `off`, or a number in its range. If the screen changed in VoxLive since it was last found, nothing is written and the screen is taken as VoxLive has it now (so `voxlive look N` shows it, and the same command again saves over it); `voxlive` says how it went |
| `voxlive refresh` | Finds the screens again now (they are also found every 30 seconds) |
| `voxlive disconnect --yes` | Forgets the key on this computer (it stays in VoxLive until revoked there) |

    $ echo "$VOXLIVE_KEY" | sp voxlive connect
    Checking the key with VoxLive; sp voxlive says how it went.
    $ sp voxlive
    VoxLive: Connected (key vxk_live_abc…), 2 screens.
    $ sp voxlive screens
      1. Spanish captions · ES (captions)
      2. Scripture · RU (Scripture)
    $ sp voxlive show spanish
    On screen: VoxLive, Spanish captions · ES.
    $ sp voxlive look spanish size=72 text=#FFE14D
    Saving the look of Spanish captions · ES to VoxLive; sp voxlive says how it went.
    $ sp voxlive
    VoxLive: Connected (key vxk_live_abc…), 2 screens. Saved the look of Spanish captions · ES to VoxLive.

The screens also show as the slides of the VoxLive item, under LIBRARY in the window; it isn't
in `sp songs`, and `playlist add VoxLive` puts it in a playlist. A screen whose page isn't at
VoxLive's own display is listed with "no page to show" (`"can_show": false` in the JSON), and
`voxlive show` says so. A mode VoxLive adds later shows by its own name in place of captions or
Scripture.

The key is kept in the Mac's Keychain, and never appears in the log, a reply or a file. A key
VoxLive refuses stops the asking until another is given: VoxLive pauses an address that keeps
trying a wrong key, which would stop the church's other devices too. A test copy (one started
with `SOFTPRESENTER_DATA`) keeps its own key in its own folder and never touches the owner's;
`SOFTPRESENTER_VOXLIVE_API` can point it at a stand-in for VoxLive on this computer (plain http
to 127.0.0.1 or localhost only).

## Undo, jobs and the menu bar

| Command | Does |
|---|---|
| `undo`, `redo` | Undoes or redoes the last change to a song or slide, whatever the window shows, and says what it was |
| `job N` | How a background job (a pack with videos, a large pack import, a Bible download) went, or "still going" |
| `menu`, `menu ITEM` | Lists the menu bar items; chooses one by id (`ClearAll`, `NewSong`) or words ("Clear All"), as a click does |

    $ sp undo
    Undid a change to River Song, slide 2.
    $ sp slide media "River Song" 1 Bars.mp4
    $ sp song export "River Song" --to river-video
    Writing river-video.sppack, with its videos (1 MB); the window says when it is done. (job 2; sp job 2 says how it went)
    $ sp job 2
    Saved river-video.sppack: 1 song, 1 video (1 MB).
    $ sp job 99
    There is no job 99.
    $ sp menu "Clear All"
    Chose ClearAll from the menu.

Any command that starts a job takes `--wait`, and then prints the job's result and exits with
its code. Menu items that open a window or a file chooser open it on the Mac, as a click does.

## Song text

The format `song new`, `song text` and the app's Reflow use:

    [Verse 1]
    Down by the river the morning is new
    Light on the water and grass in the dew
    --
    У реки утро снова встаёт
    Свет на воде и роса на траве

    [Chorus]
    Sing it again, let the valley reply

- A line in square brackets starts a group. Verse 1–6, Chorus 1–3, Bridge 1–3, Pre-chorus, Tag,
  Intro, Ending and Outro get jump keys (`sp groups` lists them).
- A blank line starts a new slide in the same group.
- A line with only `--` separates a slide's main language from its translation. Leave it out
  for one language.
- A group that comes back (a chorus after every verse) is written out again each time, or kept
  once and put in order with an [arrangement](#songs).

## Song files edited outside the app

Each song is a readable JSON file in `~/Documents/SoftPresenter/Songs/`, and a person, a script
or an agent may change one while the app runs:

- **The app never saves over a file that changed outside it.** A change made in the app to
  such a song is held, not written: the reply ends with "Not saved: … sp song reload SONG --now
  takes the file's version; sp song keep SONG saves this one." and exits 1. The window shows a
  bar with Reload and Keep mine.
- **A changed file is read again** when a command names the song, or when it is opened, unless
  the song is on screen, on the Announcements layer, in the editor, or holding changes not
  saved; then it waits for Reload or Keep mine. `songs` marks it "(changed outside
  SoftPresenter)".
- `sp song reload` reads every changed song (and themes, when the theme editor is closed), and
  adds song files new in the folder; `sp song reload SONG --now` takes the file's version even
  for a song in use; `sp song keep SONG` writes the app's version.
- A broken file is reported and the app keeps its version; `song keep` writes it back. A deleted
  file waits; `song keep` saves it again.
- **Nothing is lost on quitting:** a change still held is written to
  `~/Documents/SoftPresenter/Not saved/TITLE DATE TIME.sppres`.

      $ sp slide text "River Song" 2 "A change"
      River Song, slide 2: Main now says "A change".
      Not saved: River Song's file was changed outside SoftPresenter. sp song reload River Song --now takes the file's version; sp song keep River Song saves this one.
      $ sp song reload
      Waiting on Reload or Keep mine (in use, or with changes not saved): River Song. Their file is gone (Keep mine saves it again): Blessed Assurance.
      $ sp song keep "River Song"
      Saved SoftPresenter's version of River Song over its file.

## Song packs

A song pack (`.sppack`) carries chosen songs, or a playlist with its songs, to another computer
in one file: the songs, the themes they set for themselves, and the pictures and videos they
use, a picture or video set behind a slide's words included. It opens the same on every system
SoftPresenter runs on.

| Command | Does |
|---|---|
| `song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]` | Writes a pack (`.sppack` is added if missing). With videos it is written in the background as a job |
| `song import FILE.sppack [--skip\|--replace\|--keep-both] [--library NAME\|none]` | Opens a pack. A song already here (the same id) is skipped unless `--replace` (written over; one in use waits for Reload) or `--keep-both` (a copy with a new id) |

- Songs that use the main theme take on the receiving computer's main theme. A theme a song
  sets for itself comes along if the receiving computer doesn't have it; the receiving
  computer's own themes are never changed.
- A playlist comes in as a new playlist ("Sunday (2)" if the name is taken) unless the same one
  is already there. Its pictures and videos come into the media bin, and its media playlists
  with theirs (a media playlist the receiving computer has already, the same one, is used as it
  is there; a placeholder comes as it is). With `--no-videos`, a video in the playlist is left
  out and its item shows as missing. Pictures go into `Images/`, videos into `Media/`; a file
  already there with the same contents is reused.
- A pack whose playlist holds a picture or video, a media playlist or a placeholder is written
  as pack version 2, which an older SoftPresenter refuses with "update SoftPresenter to open
  it"; any other pack is still version 1.
- A pack holds no file paths or user names. It is checked as it is opened: entries outside its
  folders, links, unknown references and oversized files are left out and named in the reply,
  and a pack from a newer SoftPresenter is refused.

      $ sp song export "Blessed Assurance" "River Song" --playlist Sunday --to sunday --wait
      Saved sunday.sppack: 3 songs, and the playlist Sunday.
      $ sp song import sunday.sppack          # here, where they are already
      Skipped, already here: Blessed Assurance, River Song, Harbour Lights. The playlist Sunday is here already.
      $ sp song import sunday.sppack --keep-both
      Imported 3 songs: Blessed Assurance, River Song, Harbour Lights. Added the playlist Sunday (2).

## Recipes

### A song from lyrics someone sent

1. Write the words as [song text](#song-text): a `[Group]` line for each part, two lines to a
   slide with a blank line between slides, and singing directions such as "x2" left out.
2. Check the title isn't taken (`sp search TITLE`), make the song, and look at it:

       $ sp song new "Blessed Assurance" blessed.txt
       Made the song Blessed Assurance.
       $ sp slides "Blessed Assurance"

3. Put it in Sunday's service: `sp playlist add "Blessed Assurance" to sunday`.

A file in another format (a lyric file, ChordPro, Word) goes straight in with
`sp song import FILE`.

### Fixing a slide, even while it is on screen

    $ sp search foretast                 # which song and slide
    $ sp slide "Blessed Assurance" 1     # its objects
    $ sp slide text "Blessed Assurance" 1 "Blessed assurance, Jesus is mine\nO what a foretaste of glory divine"
    $ sp undo                            # if it was wrong

### Text too big, or a line that wraps

    $ sp slide style "Blessed Assurance" all size=90
    $ sp slide style "Blessed Assurance" 1 reset

### Preparing Sunday

    $ sp playlist new Sunday 24 November
    $ sp playlist header Worship in sunday 24 november
    $ sp playlist add "Blessed Assurance" to sunday 24 november
    $ sp playlist add "River Song" to sunday 24 november
    $ sp playlist header Reading in sunday 24 november
    $ sp bible "John 3:16-18" --add to sunday 24 november
    $ sp timer new Sermon countdown 30:00
    $ sp message new Nursery "Parent of child 12, please come to the nursery"

### Running it

    $ sp playlist open sunday 24 november
    $ sp continuous on
    $ sp playlist go 2                   # the first song
    $ sp next                            # …on through the playlist
    $ sp group chorus                    # back to the chorus
    $ sp playlist go 5                   # the reading, on the Bible page
    $ sp bible "John 3:16-18" --show
    $ sp timer start Sermon
    $ sp clear

### Songs prepared at home

    home$   sp song export "Blessed Assurance" "River Song" --playlist "Sunday 24 November" --to sunday --wait
    church$ sp song import sunday.sppack

### Checking from a script

    $ sp status --json | jq -r '.data.live.song // "nothing"'
    $ sp status --json | jq '.data.live | {id, playlist, item}'     # which song exactly, and from where
    $ sp slides "River Song" --json | jq '.data[] | select(.on_screen) | .slide'
    $ sp go "River Song" 9 >/dev/null || echo "refused"

## Replies as JSON

With `--json`, every reply is one JSON object:

```json
{ "ok": true, "text": "On screen: River Song, slide 2 of 2 (Chorus).", "data": null }
```

`ok` is false when the command was refused or not done, and `text` then says why. `data` holds
the command's facts where it has any:

| Command | `data` |
|---|---|
| `status` | `token`, for [`--wait --since`](#waiting-for-a-change) (with `--wait`, also `changed`: the parts that changed since the token); `live` (null when nothing is on screen): `song`, `slide`, `slides`, `words`, `kind` (`song`, `passage` or `voxlive`), the song's `id` and its `number` in `sp songs` (null for a passage), and the `playlist` and `item` number it was opened from (null when it wasn't); `next_slide`, `media`, `message`, `props`, `announcements`, `media_playlist`, `timers` (each `name`, `kind`, `shows`, `running`), `audio` (`track`, `paused`, `seconds`, `length`, `volume`), `playlist` (`name`, `item_last_opened` and its number `item`, `listed`), `audience_on`, `stage_on`, `continuous`, `identify`, `main_theme`, `selected_song` |
| `songs`, `playlists`, `media`, `themes`, `timers`, `messages`, `props`, `macros`, `audio` | An array of names, in their numbered order |
| `playlist [NAME]` | An object for each item: `item`, `kind`, `title`, `missing`, `text`, and `id`, `number`, `media_type` or `bibles` by its kind ([above](#playlists-and-libraries)) |
| `libraries` | `[{"name": "Hymns", "songs": 3, "missing": 1}]` |
| `screens` | `[{"name": "Screen 1", "doing": "In a window", "shown": true}]` |
| `displays` | `[{"name": "Built-in Retina Display", "width": 3024, "height": 1964, "main": true}]` |
| `stage layouts` | `[{"name": "Words Only", "screens": ["Band"]}]` |
| `stage objects LAYOUT` | `[{"object": 7, "name": "Screen: HDMI main", "shows": "what HDMI main shows", "item": null, "screen": {"chosen": "HDMI main", "shows": "HDMI main", "fell_back": false}, "frame": {"x": 1240, "y": 130, "w": 640, "h": 360}, "hidden": false, "locked": false}]`; `item` is the role of what an item shows (`clock`, `now`…), `screen` is null for anything but a screen picture, and `fell_back` is true when the screen it chose has gone and it shows the main one |
| `groups` | `[{"key": "C", "group": "Chorus", "names": ["chorus", "chorus 1"], "color": "#cc4eb4"}]` |
| `menu` | `[{"id": "ClearAll", "label": "Clear All"}]` |
| `slides [SONG]`, `slides item N` | `[{"slide": 1, "group": "Verse 1", "words": "…", "on_screen": false, "enabled": true, "label": null, "hot_key": null}]` |
| `slides … --full` | The same array, each slide also with what `slide SONG N` gives (`notes`, `transition`, `template`, `actions`, `advance`, `objects`), all but `song` |
| `slide SONG N`, `slide item N SLIDE` | `song`, `slide`, `group`, `enabled`, `label`, `notes`, `hot_key`, `transition`, `template`, `actions`, `advance`, and `objects`: each with `object` (its number), `name`, `role`, `kind`, `text`, `frame` (`x`, `y`, `w`, `h`, and `own`), `style` (the settings the slide sets itself) and `look` (every setting, the theme's filled in) |
| `song text SONG` | `{"song": "River Song", "text": "[Verse 1]\n…"}` |
| `song info SONG` | `id`, `title`, `artist`, `author`, `publisher`, `year`, `ccli`, `slides`, `theme`, `transition`, `loops` |
| `search WORDS` | `[{"song": "Blessed Assurance", "number": 1, "title": false, "slides": [1]}]` |
| `bible search WORDS` | `[{"verse": "John 3:16", "chapter": …, "words": "…", "bible": "KJV"}]` |
| `song import` | `imported`, `making` (presentations still being turned into slides), `opening` (song bundles still being read), `notes` (pictures or videos a bundle's song came without), `problems` |
| `settings` | Every setting, as the app keeps it |
| A command that starts a job | `job`: its number |

The other commands answer in `text` only.

## Not on the command line yet

These need the window for now; some can be reached with `sp menu`.

- Adding or deleting objects on a slide (text boxes, shapes, pictures, videos), builds, and the
  inspector's fills, gradients and shadow details beyond the `slide style` keys.
- Key mappings; a stage layout's objects other than its screen picture (adding, deleting and
  restyling them; `stage frame` moves any of them), and making, renaming or deleting stage
  layouts.
- A slide's other actions (clearing a layer, a message, a prop, a timer, another song,
  the audience screens; `slide media` sets only the media), and its "goes on after" time.
- Whether a song loops back to its first slide (`song info` shows it).
- Which boxes each Bible row's verse and reference go in (`bible rows` chooses the Bibles; the
  boxes follow the theme).
- Removing a track from an audio playlist.
- Editing a macro's steps in place (they can be added and removed), and a prop's contents
  beyond its picture or name.

## How it works, and who can use it

- When the app starts it listens on a socket file: on a Mac,
  `~/Library/Caches/SoftPresenter/control/ctl.sock` (outside the library, so `sp` needs no
  Documents permission and works over ssh); with `SOFTPRESENTER_DATA` set,
  `control/ctl.sock` in that folder; `SOFTPRESENTER_CONTROL_SOCKET` names another (keep it
  short: about 100 bytes at most). The folder can be entered, and the socket opened, only by
  the user the app runs as.
- On Windows the socket is the same kind of file, `SoftPresenter\control\ctl.sock` in the
  user's local app data folder (`%LOCALAPPDATA%`, which stays on that computer). At each start
  the app makes the user the folder's owner and gives the folder an access list that names
  only that user and takes nothing from the folder above, then reads the list back, and reads
  the socket file's own list once it is made. If either can't be set or read, or names anyone
  but that user, the system and the administrators, the app does not listen and its log says
  why. It does this with Windows' own `icacls` and `whoami`. A folder that is a link or a
  junction is refused, as a linked folder is on a Mac. A folder that was already there with
  entries for other people is not changed: the app says so and does not listen until the folder
  is deleted (it makes it again, for the user alone). The same goes for a socket file already
  there that others may open.
- Two things the app does not check, on any system. The folder above the socket's folder: a
  socket path pointed at a shared place with `SOFTPRESENTER_CONTROL_SOCKET` is only as safe as
  that place, where somebody else may be able to put a folder of their own in the way. And
  `sp` trusts the path it is given: it does not look at the folder before it sends.
- On Windows, a program the same user runs with fewer rights than usual (low integrity, as the
  sandboxed part of a browser is) is not kept out by anything the app sets. Windows keeps it
  out by itself: a test run on Windows shows such a program refused, beside one let into a
  socket marked for it.
- Nothing listens on the network: no other computer, and no web page in a browser, can reach
  it. A program running as the same user can, which is no more than that program could do by
  changing the app's files.
- A request is one line of JSON, `{"v": 1, "args": ["go", "River Song", "2"]}`, at most 64 KB
  (`sp` refuses a longer one before sending it); the reply is one line of JSON. `v` is the
  protocol's version: a command line from a different build is told so.
- The app answers one command at a time, each within 15 seconds. A
  [`status --wait`](#waiting-for-a-change) is the exception: its connection is held on a thread
  of its own (at most 8 at once, for at most 30 seconds each) while the app carries on.
- A second copy of the app leaves the first one's socket alone and runs without one, unless it
  is given its own with `SOFTPRESENTER_CONTROL_SOCKET`.
- The socket is removed when the app quits. With the app not running, `sp` says so.
- Each command that changes something is logged in the app's log
  (`~/Library/Logs/SoftPresenter/SoftPresenter.log`, or the data folder's `SoftPresenter.log` for
  a copy run with `SOFTPRESENTER_DATA`); commands that only read are not ([the basics](#the-basics)
  lists them).

## Still to come

An API over the network (HTTP and WebSocket, with passwords, switched on in Settings), the whole
app in a browser, a stream of changes as they happen, and an MCP server so AI agents can use
these commands as tools. All of them will run the same commands as `sp`.
