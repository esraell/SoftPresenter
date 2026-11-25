# What's new in SoftPresenter

What changed in SoftPresenter, newest first, for the volunteers who run services and the people testing it.
Versions are numbered 0.x until version 1, and until then things may still change from one build to the next.

## Unreleased

- The Edit view no longer shows the preview and the clear buttons on the right: the settings there (Shape, Text, Build, and a theme's or a stage layout's own) take the whole column, with far more room. While editing, the keys still clear the screens (Escape, and off a Mac F1 to F6); the buttons are in the Service and Bible views as before.
- The clearing keys work in the Edit view too: off a Mac F1 to F6, as in the Service view, and Escape once nothing is picked, being typed or open there (a first Escape still lets go of the picked box, puts back words being typed or closes a menu; the next one clears the screens). Space, the arrows and the letters still never change the screens from the Edit view.
- The six clear buttons on the right are in the order the screens stack the layers, from the back: Media, Slide, Announce, then Props, Messages and Audio. Their keys are the same.
- The phone and tablet Remote Control page speaks the app's language: Spanish when the app is in Spanish, from the sign-in on, with the same words as the app.
- Claude and other AI assistants can help prepare services through `sp mcp`: find songs and Bible passages, make songs and playlists. Switch it on in Settings › Integrations; changing the screens needs a second switch, and the assistant asks you each time, showing the words that will go up.
- Unofficial: an iPhone, iPad or Mac can show its screen in a box on a slide, through its own Screen Mirroring. Switch it on in Settings › Phone Screens; it is off until then, a device shows only where you put a Phone or iPad Screen box, and you're asked before it does. It isn't made or supported by Apple, and an Apple update may stop it working. On a Mac only for now, and without sound yet.
- Claude and other AI assistants can work with SoftPresenter: find songs and Bible passages, make a song from lyrics in one or two languages, and prepare a service's playlist. They can change what is on the screens only if you allow it in Settings › Integrations › AI assistants, and they ask you each time first. Both are off until you switch them on.

## 0.9.0-dev.14 — 24 November 2026 (development build)

- The screens no longer wait for the computer's disk to write the log, which once held them still for half a second.
- A video behind the words no longer freezes on NDI and DeckLink for each slide change: those outputs now show the dissolve the TVs show, and the video keeps moving through it.
- Each NDI screen has a frame rate in the Screens window: 29.97 unless you choose 30, 59.94, 60, 25 or 50. Sending exactly 30 to 29.97 equipment, as before, made a small pause every half-minute or so.
- The stage screen's clock no longer makes the other NDI screens redraw once a second.
- Keying a DeckLink screen is one choice in the Screens window now: Keyer Off, Fill and key, or Over the card's input. Keying makes the screen see-through with premultiplied soft edges, the usual for a switcher's keyer, so only the words are keyed; a keyed screen left solid before is put right when the app opens. When the screen's format isn't the one Desktop Video Setup has, the window says so in amber, with a button to use it.

## 0.9.0-dev.13 — 24 November 2026 (development build)

The first numbered build, with the same number on Mac and Windows. This is everything SoftPresenter could do on 24 November 2026.

### Songs and lyrics

- Songs are slides in groups (Verse, Chorus, Bridge), each with its colour and a letter key (A for Verse 1, C for the chorus).
- Two languages on every slide: the main words, with the translation under them.
- New Song (Command-N) takes pasted words: `[Verse 1]` starts a group, a blank line a new slide, a `--` line the translation.
- Arrangements set the order the groups are sung in; a chorus sung twice shows as ×2, and the slides stand in that order.
- Songs come in from text, rich text, ChordPro, lyric and (on a Mac) Word files, and whole libraries from other presentation apps.
- A .pptx presentation comes in as slides you can edit, or as pictures; PDFs and folders of pictures become picture slides.
- After an import, slides that may need a look (a chart that came in as a picture) wear a Check tag until marked as checked.
- Song packs carry songs, or a whole playlist with its pictures and videos, to another computer in one file.

### Bible

- Type a reference in English, Russian or Ukrainian; nothing goes on screen until you click a verse or press Enter.
- Up to four translations on one slide, lined up verse by verse even where they number verses differently.
- Four Bibles come with the app, and sixteen more install from the Bibles window.
- A Bible file the church already has installs when dropped on the window (Zefania, OSIS, USFM, USX and other formats).
- The page shows the whole chapter, a keypad of all 66 books and History of passages shown; Аа sets its reading language.
- + Add puts the passage in the playlist, and the find box (/ or Command-F) finds passages and verses as well as songs.

### Media and video

- Pictures and videos go in the media bin, stay between runs, and show behind the words or on their own.
- Videos play with their sound and can loop; MKV, WebM, AVI and other formats play with nothing to install on a Mac or Windows.
- Under the preview, a playing video has Play or Pause, Start Over, Forward 10 seconds, a bar to drag through, Loop and Sound.
- The Announcements layer loops pictures, videos or a song's slides before the service, and comes off as the first slide goes up.
- A media playlist can follow a folder: a fresh video put there takes over at the end of the loop, with no black between.
- The Audio tab plays walk-in music from playlists, with shuffle, a crossfade and its own volume.
- Seven sample backgrounds come with the app: Add the Sample Backgrounds, in the menu beside MEDIA.

### Screens, NDI and DeckLink

- Any number of audience screens, each with its own look: on a display, in a window, over NDI or out of a DeckLink card.
- Your own display is never filled; with only one, the audience screen opens in a window until a projector is plugged in.
- A screen takes over its display at once; when its projector goes off it hides, and it comes back when the display returns.
- Moving the pointer over a screen that fills a display shows an X that turns it off, so a wrong display never leaves you stuck.
- Identify Screens shows each screen's name on it; test patterns (colour bars, grid, white) are in Settings › Screens.
- NDI sends only while someone watches, with the sound of what plays; a screen can be see-through, or another's key, for a stream.
- A camera or another computer sending NDI on the network can be shown in a box on a slide.
- DeckLink cards play out over SDI or HDMI, with fill and key where the card can key (Desktop Video 16 or later).

### Stage screen

- Stage screens show the words now, the next slide, the clock, a running timer and a stage message typed in the Stage tab.
- Ready-made layouts: Words and Next, Words Only, Words and Notes, Clock and Timer; one click in the Stage tab switches.
- Edit › Stage designs your own layouts with the slide editor: words, shapes, pictures, videos, web pages and the slide's notes.
- A layout can show a live picture of an audience screen, so the band sees exactly what the room sees.
- With several stage screens, each can show a layout of its own.

### Themes and the editor

- The slide editor places text, shapes, pictures, videos and web pages (on a Mac only), with guides, rulers and undo.
- Words can be styled inside one box: a bold reference, an italic line, one word in a colour.
- SoftPresenter Sans, with Latin, Cyrillic and Greek letters, is built in, so slides look the same on every computer.
- A theme has templates for two-language songs, one-language songs and Bible slides; each audience screen can have its own theme.
- Theme versions: a named look for a moment in the service ("Offering") that one switch puts on every screen whose theme has it.
- Nineteen transitions, for all slides, one song or one slide; builds, scrolling text, and text that shows the clock or a timer.
- Props: a logo or a banner that stays over the slides until taken off.

### Playlists and services

- A playlist is the running order: songs, Bible passages, pictures, videos, media playlists, headers and placeholders.
- The whole playlist shows as one scroll of slides; a click on an item opens it without changing the screens.
- Space and the right arrow go forward, letters jump to a group, Escape (or F1 off a Mac) clears everything; ? lists every key.
- The Show view is now the Service view (Command-1); Bible is Command-2 and Edit Command-3. A Mac's menu bar is six short menus.
- Timers, messages, props, macros, audio and the stage each have a tab of cards under Clear All (Option-1 to Option-7).
- Macros do several things in turn; one can run as the app opens, playing the announcement loop with the audience screens off.
- Undo brings back what was deleted; the app asks first only when Undo can't bring something back.
- The notices button in the top bar keeps what needs your attention; nothing pops up over the show.
- Nothing needs saving: every change is written as you make it, on a Mac into Documents › SoftPresenter, easy to back up.

### Remote control and the command line

- Remote Control runs the show from a computer, tablet or phone on the church's network (Settings › Remote Control).
- It is off until switched on, uses HTTPS and a password of ten characters or more, and answers only the church's own network.
- The page shows what is on screen with a live picture, the playlist, the slides and the tabs; Settings can forget a device.
- `sp` runs the app from a terminal or a script (`sp status`, `sp next`), and installs itself as the app first starts.

### Languages

- The app can be used in Spanish: choose it in Settings › General › Language or in the setup, and it changes at once.
- A few messages that come straight from the video, NDI and DeckLink drivers are still in English.
- Russian and Ukrainian can be chosen too, but the window's words aren't translated into them yet.
- Names the app gives (the first playlist, a new box) follow the language; names the church gave never change.
- The Bible takes references in English, Russian and Ukrainian; imported songs read Cyrillic, in older Windows encodings too.

### Windows

- Windows 10 and 11 (64-bit) builds are in testing; nothing is released yet.
- The package is a zip with the program and its own FFmpeg; it isn't signed yet, so Windows asks the first time ("Run anyway").
- Videos play through that FFmpeg, decoded on the graphics chip where it can be, without a console window opening.
- Screens are listed by the monitors' own names and cover the taskbar on their monitor.
- PDFs come in, and so do presentations as pictures when an app that opens them is installed.
- NDI is found through NDI Tools; the firewall asks once, when NDI or Remote Control is first used.
- The `sp` command installs itself; VoxLive's key and Canva Sync's sign-in are kept in Credential Manager.
- Not yet on Windows: DeckLink outputs, web pages on slides, and Word or OpenDocument files.

### Linux

- Linux builds are in testing; nothing is released yet.
- Displays are listed by the monitors' own names, each once.
- Download NDI… in the Screens window installs NDI for SoftPresenter once you accept NDI's licence, with no admin password.
- VoxLive's key and Canva Sync's sign-in are kept in the system keyring (GNOME Keyring or KWallet).
- Videos need FFmpeg installed on the computer (for example `sudo apt install ffmpeg`).
- Not yet on Linux: PDFs, presentations as pictures, Word or OpenDocument files, and web pages on slides.

### Integrations

- VoxLive: connect with an API key in Settings › Integrations; the church's caption screens become slides and a VoxLive tab.
- A VoxLive screen's look can be changed from Settings, with a preview that starts no translation.
- Canva Sync makes the announcements video from the church's Canva presentation, from the pages above a divider page.
- Sync Now, a macro step or a start-up macro fetches it, and the new video takes over at the end of the loop playing.
- Free songbooks in many languages (development builds only, for now) download in the setup or Settings › Integrations.
- The find box searches them; a song joins the library once put in a playlist, and a church's edits are never overwritten.
- Keys and sign-ins are kept in the computer's secure store (Keychain, Credential Manager or keyring), never in a plain file.

### Fixes worth knowing

- Early builds used more and more memory while they were open; they no longer do.
- Changing or removing a font while SoftPresenter was open could crash it in the middle of a song; it no longer does.
- Clicking through slides faster than the fades could leave a slide stuck or slow the screens; both are fixed.
- When a projector was switched off, its screen could land on the operator's display; now it hides until the projector is back.
- Playlists lost their songs if the library folder moved; songs are now found wherever the folder goes.
- Some looping videos froze on their last picture instead of going round; they loop now.
- Escape in the Bibles window, the playlist chooser or Edit Words cleared the screens; now it just closes them.
- Linux crashed every time it quit; it now quits cleanly.
