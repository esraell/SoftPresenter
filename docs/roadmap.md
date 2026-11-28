# Roadmap

These are the features planned for SoftPresenter, beyond what
[features.md](features.md) lists. Sizes and order may change as each one
is built, and nothing here is promised by a date. Ideas are welcome as
[issues](https://github.com/esraell/SoftPresenter/issues).

Size: S = under a day, M = days, L = a week or more. Sunday: how much it matters for a typical church service.

## Coming first

1. **Playlists and media from a church's existing presentation software.** Songs, libraries
   and song bundles (with their pictures and videos) come across already; playlists, and the
   pictures and videos a plain song document names on the computer it is on, are next. · M
2. **Fixing a slide's words from the Service view.** Mend a typo on the live slide without leaving
   the Service view. · S–M
3. **Corner pin (keystone).** Square up a projector that is off-axis. · M
4. **More slide and macro actions.** Audio tracks and playlists, clearing audio and
   announcements, changing a stage layout or a saved screen setup, running another macro,
   media playlists. · S each
5. **A whole song that moves on by itself.** Announcement loops with one setting. · S
6. **Clear buttons of your own.** Each clears a chosen set of layers. · M
7. **Slides shown on the stage screens only**, and an action that changes a stage layout. · M
8. **Saved screen setups switched live, and a theme for each screen.** Lyrics off the stream,
    lower thirds on NDI. · M
9. **Fill-in fields in messages**, for paging parents from the nursery. · S–M
10. **What a video does when it ends, in and out points, and video sound against background
    music.** · M
11. **More on stage.** A video countdown, and the current or next slide as a picture. · M

Next in line: the network API, with the whole app in a browser and a phone remote, spell check, CCLI usage
reporting, editable key mappings, adding time to a running timer, and Russian and Ukrainian
menus. Tested Windows and Linux builds are left out of the ranking because they are a port
rather than a feature, but no church on Windows can use the app without them.

## Running a service

- **Clear buttons of your own.** Each clears a chosen set of layers (for example words and props, keeping the background), and can be used from keys, macros and remotes. (started: a clear button for each layer, Clear All, a macro's Clear steps for every layer, and F1–F6 off a Mac) · M · Medium
- **Slides shown on the stage screens only.** A key or a slide action sends the slides that follow to the stage screens only, while the audience keeps its picture (band notes, sermon notes). · M · Medium
- **Fixing a slide's words from the Service view.** A small pop-up for a typo spotted mid-service, without opening the editor. (started: the editor, Edit Words and Live Edit) · S–M · Medium
- **A whole song that moves on by itself.** One setting in the song's header moves every slide on after 5, 10, 15 or 20 seconds or a time of your own, for announcement loops. (started: Go to Next on each slide, set one by one from fixed times of 3 to 60 seconds, and Loop to First Slide) · S · Medium
- **Text format pop-over.** Change the font, size, colour, capitals, alignment, outline and shadow of the selected slides or songs at once. (started: styling per box in the editor, and Copy Style and Paste Style) · M · Medium
- **Notes for a whole song.** One line of operator notes kept with the song and shown in its header. (started: the notes strip shows the notes of the slide on screen) · S · Low
- **Timeline.** Record slide timings and play a song's slides, media and audio together, with a loop; it can follow incoming timecode. (started: Go to Next on each slide, and Loop to First Slide) · L · Low
- **A song that always plays on the Announcements layer.** A setting kept with the song, so it need not be chosen each time. (started: Play on Announcements Layer in the song's menu) · S · Low
- **Transport for any layer.** Choose whether the transport drives slide media, announcements or audio; a scrub bar; a jump to 10, 30, 60 or 90 seconds before the end. (started: pause, seek, loop and mute under the Program preview; in the Audio tab a line to drag through the track, skip 15 seconds either way, previous, next and mute) · S · Low
- **More show shortcuts.** F7 to clear announcements off a Mac (a Mac's F keys are the system's), ⇧⌘L to show the live announcement, ⌥⌘L to find a song in its library, a key for the stage editor (⌘1 to ⌘7 are the tool tabs now), holding ~ to peek at Words Only, and Option-click to load a video paused. (started: Presentation › Clear Announcements is there without a key) · S · Low
- **Slide view settings.** Font, size, colour and truncation in Words Only, a notes column in Table view, slides shown by group, and background colours. (started: the three views and the size slider) · S · Low
- **Audio level meter by the Program preview.** · S · Low
- **Warnings for missing things.** A badge on macros whose target has gone and on media playlists whose folder is missing. (started: a badge on slides whose text overflows, missing songs dimmed in playlists, and a macro step whose target has gone saying so in amber) · S · Low
- **Removing actions from several slides at once.** (started: removing actions from one slide) · S · Low

## Slides and editing

- **Spell check.** Misspelt words underlined in lyrics and slides (⌘; and ⌘:). · M · Medium
- **More linked text.** Slide number and slides left, the next playlist item and header, time left before the slide moves on by itself, time left in the video, another box's text, a text file and an RSS feed. Slide count and the next item are the ones stage screens use. (started: ten sources: clock, date, timer, title, group, label, next slide, notes, stage message and copyright) · S–M · Medium
- **Slide sizes other than 1920 × 1080.** A size for a song or a theme (1280 × 720, 4:3 or your own), for LED walls and 4:3 projectors. · L · Low
- **More transitions.** Transitions with effects of their own (3D, ripples, burns), with their options (colour, scale, radius). (started: nineteen transitions with direction and time) · M–L · Low
- **Transition picker extras.** Favourite and recent transitions, a filter box and previews on hover. (started: categories, and a reset to the default settings) · S · Low
- **Song settings panel.** A background colour, slide size, album and artwork for each song, and whether it shows the copyright line. (started: Artist, Author, Publisher, Year and CCLI Song # in the Info pop-over, and the song's transition in its menu) · S–M · Low
- **Background colour for one slide**, and a setting to ignore slide background colours. (started: a background colour and picture for a whole theme) · S · Low
- **A text style applied to all slides at once.** (started: Copy Style and Paste Style) · S · Low
- **More fills.** Text as a cut-out, a media fill, background invert or blur; shapes filled with a picture, a video, a web page, a live camera, a background effect or another object on the slide. (started: colour, and linear and radial gradients) · M–L · Low
- **Custom shapes and shape settings.** Draw shapes of your own with a pen; set a star's points and a polygon's sides; Option-drag to duplicate; Shift keeps proportions. (started: ten fixed shapes, and ⌘D duplicates) · M · Low
- **Showing a slide from the editor.** Send the slide being edited to the screens. (started: Live Edit sends edits to the screens; Show in the editor goes back to the Service view) · S · Low
- **Outline and text details.** Dashed outlines, an underline colour, and for scrolling text its speed, start point, edge fade and gap. (started: outline width and colour, and scroll direction) · S · Low
- **Number fields that take sums and can be dragged to change the value.** · S · Low
- **Animated graphics.** A searchable library of animated graphics to drop on slides; the graphics have to be made first. · M · Low
- **Feathering.** · S · Low
- **Objects shown only under conditions.** An object shows only when all, any or none of some conditions hold (a timer running, a video countdown, another object). · M · Low
- **Richer builds.** Any transition as a build, builds chained one after another, a build order you drag, fill-in-the-blank words, and text that comes on by letter, word or line. (started: Dissolve, Fly, Zoom and By Paragraph) · M · Low
- **Edit Words with live slides beside the text.** (started: Edit Words rebuilds the slides on Done) · S–M · Low
- **Editor view switches and menus.** Checkerboard, snapping, rulers and guides on and off; zoom to actual size or to fit, with ⌘0; tighter or looser spacing, ligatures, and raising or lowering the baseline; ⌘T for fonts; ⌥Return for a slide break. (started: rulers, guides and snapping always on, a Grid switch, zoom in the status bar, and the same values under the gear) · S · Low
- **Undo that names what it undoes.** · S · Low
- **New-song sheet with size, library and playlist.** (started: title and theme; the song goes into the library shown) · S · Low
- **Groups and slide labels of your own.** Vamp, Turnaround and Blank groups; groups of your own with colours and keys; slide labels of your own with colours; all edited in Settings. (started: a fixed group list with Interlude, four fixed labels, groups renamed within a song, and a read-only Groups pane) · S–M · Low
- **Background colour for slide notes.** · S · Low
- **Emoji & Symbols and Dictation in the Edit menu.** · S · Low

## Media and video

- **A smoother web page engine.** Web pages on slides are drawn by macOS's web view, copied about 30 times a second, which shows moving content (a scrolling ticker, an animation) unevenly. Chromium drawn off-screen, frame by frame with the screens (as broadcast software does it), makes them smooth at 60 frames a second and brings web pages to Windows and Linux; it adds about 200 MB to the app. (started: web pages on slides through macOS's web view) · L · Medium
- **Video on Windows and Linux, and AV1 on older Macs.** Every video format plays without installing anything there too; the other systems are L, and AV1 on Macs before M3 needs the dav1d decoder in the FFmpeg build (S). (started: macOS's own player and an LGPL FFmpeg inside the Mac app) · L · High
- **What follows a video, and timing per item.** What comes after a video (the next, a random, the first or a chosen item), how it ends (stop, hold the last frame, black, clear, fade), looping for a count or a time, how long a picture stays, and whether clicking it again starts it over. (started: Loop on or off per item, and Play This Playlist, which goes round a media playlist with pictures for 8 seconds) · S–M · Medium
- **In and out points, duration and start delay.** · M · Medium
- **Media kept with the library.** Every imported file copied into the app's folder, found again when moved, and unused copies cleaned up. (started: pictures put on slides are copied to the Images folder, and copies nothing uses go to the Trash once a day; media bin items and videos stay where they are) · M · Medium
- **Live video inputs.** Cameras, capture cards, NDI sources and AirPlay as a layer, from the media bin, a slide action or a box on a slide. (started: an NDI source in a box on a slide, picture only) · L · Medium
- **AirPlay: what is left.** An iPhone, iPad or Mac shows its screen in a box on a slide
  today (picture only, one device at a time, on a Mac; unofficial). Still to come: the device's
  sound through the app's mixer; Windows and Linux; a code on the screen instead of a question,
  with remembered devices; several devices at once; a mirrored screen full screen as media.
  (M)
- **Media bin filter, list view and size slider.** Find media by name, which matters most, and a list with path, duration, transition and format. · S · Medium
- **Foreground and background media.** Each media cue is marked background (it stays under the slides as they change) or foreground (a one-off item such as a video, which shows even over slide background colours and has its own rule for the music; see Audio). Both play on the media layer under the words. (started: one kind of media cue; SoftPresenter has no slide background colours yet, so the difference matters less) · S–M · Low
- **Fit with blur.** Fit the picture and fill the bars with a blurred copy of it, for portrait phone photos. · S · Low
- **Media inspector.** Alignment, flip, crop, rotation, play rate, a thumbnail from any frame, and a transition for each item. (started: one transition for the whole media bin) · M · Low
- **Fading a video's sound in and out.** · S · Low
- **Video effects.** Filters such as colour adjustment, blur, colour key, sepia, vignette and old film. · L · Low
- **Media item housekeeping.** Rename, duplicate, cut, copy and paste, Show in Finder, and sorting by type, name or duration. · S · Low
- **Media playlist folders.** (started: media playlists in one flat list) · S · Low
- **Video and audio streams by web address (HLS).** · M · Low
- **Alpha type for media.** Straight or premultiplied. · S · Low
- **A store of motion backgrounds.** Paid motion backgrounds to get from inside the app; a free source could stand in. · L · Low

## Audio

- **Choosing the audio output device.** Plus a master level, a delay, and a separate device for previewing cues. (started: the system's default device and a volume slider) · S–M · Medium
- **Video sound against background music.** A video with sound stops, pauses or leaves the walk-in music. · S · Medium
- **Audio from a slide or macro.** Play an audio track or playlist, or clear audio, as an action (start the walk-in music from a macro). · S · Medium
- **What each track does at its end.** Stop, loop or go to the next, for each track. (started: Play Through and Shuffle for the whole playlist) · S · Medium
- **Track inspector.** In and out points, start delay, play rate, and fades in and out for each track. (started: a crossfade between tracks) · S–M · Low
- **Sound effects.** Short cues that play over the main track. · M · Low
- **Markers.** Points marked in a track or video; stage screens count down to the next one. · M · Low
- **Audio bin extras.** Playlist folders, a filter, and how long each track and the whole playlist are. (started: renaming a track, putting the tracks in another order, and how many there are) · S · Low
- **Audio over SDI.** What the app plays inside a DeckLink output's signal, as it is inside an NDI screen's. (started: NDI carries the sound; a DeckLink output the picture only) · M · Low
- **Channel routing.** Up to 16 channels, a routing grid, and mute, solo and test for each channel. · L · Low
- **Live audio inputs.** A mixer or microphone feed, switched on, off or automatically. · L · Low

## Bible

- **Modern licensed translations.** NIV, ESV, NKJV, NLT, RV1960 and others, which English-speaking churches expect; each needs a licence. · L · Medium
- **More free Bibles in more languages.** (started: 4 ship with the app and 77 more can be downloaded, in 29 languages in all; a church can install any other Bible file it has) · M · Low
- **A transition of its own for Bible slides.** (started: the shared slide transition) · S · Low
- **The reference straight after the verse.** The reference follows the last word, on the same line. (started: the reference can go on a line of its own in the same box) · S · Low
- **Red-letter text.** Needs the data for each Bible. · M · Low
- **More than four translations at once.** (started: four rows) · S · Low
- **A long passage split by what fits.** As many verses on each slide as its box holds, then
  the next slide. (started: one verse a slide, or the whole passage on one) · M · Medium
- **Every verse matched between numberings.** The table that matches verses between the English
  and the Synodal numberings has no Synodal verse for English 2 Corinthians 13:14 and Acts
  19:41, so with the Synodal first those verses can't be looked up from an English page (one
  click says there is no such verse). Fill such gaps in the table. · S · Medium

## Themes and screen setups

- **Saved screen setups switched live.** Named sets of which layers each screen shows, picked from a toolbar button, the Screens menu, a slide or a macro, with a fade between them. (started: one fixed setup for each screen) · M · Medium
- **A theme for each screen.** One screen draws the words with another theme (lower thirds on the stream); most useful to churches that stream. · M · Medium
- **Masks and a mask editor.** Shapes that hide part of a screen. (started: covered edges for each screen) · M · Low
- **Copyright line editor.** Place and style the copyright line. (started: a fixed line at the bottom left; a theme box linked to the copyright can be placed by hand) · S · Low
- **Theme picker pop-over.** Thumbnails with recent themes; open a theme to pick a template; apply it to the selected slides or songs; rename or delete from the tile. (started: the song's Theme menu, the slide's Template menu, and deleting themes in the Themes view) · S · Low
- **Theme housekeeping.** Saving chosen slides into a theme, exporting and importing themes as files, and theme slides that start media. · S each · Low

## Screens and outputs

- **Corner pin (keystone).** Pull each output's four corners to square up a projector that is off-axis. · M · Medium
- **Screen sizes, and NDI size and frame rate.** LED walls, a wide lyric banner, 720p or 4K NDI, 1080p at 60 frames a second. (started: everything at 1920 × 1080, and NDI at 30 frames a second) · L · Medium
- **Colour correction per output.** Red, green, blue, gamma and black level, to match projectors and TVs. · M · Low
- **Video delay per output.** · S–M · Low
- **Part of a display per screen.** Send half or a quarter of a display (2×1, 2×2, 3×1 or your own split) for splitters. · S–M · Low
- **Grouped screens and edge blending.** One picture across a video wall, or projectors blended into one. · L · Low
- **Syphon output (Mac).** · M · Low
- **Placeholder screen with a resolution.** Comes after other screen sizes. (started: a screen whose output is off stays in the list) · S · Low
- **Windowed output of a chosen screen.** (started: New Windowed Output makes a new audience screen) · S · Low
- **Richer Identify.** Each layer named on the screens, and each output's device, resolution and frame rate shown. (started: Identify Screens shows the name, a border and a colour) · S · Low
- **Test pattern window.** About ten patterns with options, a logo, and a choice of screen. (started: colour bars, a grid and white in the Screens menu) · S–M · Low
- **Screens settings.** A performance overlay, the timing source, 10-bit colour and HDR. · S–L · Low
- **Recording or streaming a screen.** Capture to disk, or stream over RTMP or to Resi, with a Live menu in the toolbar and capture actions. · L · Low
- **Blackmagic SDI and HDMI outputs.** Dissolves on a DeckLink output, a video's sound over SDI, and DeckLink on Windows. (started: any screen played out of a DeckLink output on the card's own clock, in the format chosen, with the card's keyer sending fill and key or keying over its input; checked here with pretend devices, a real card to be checked) · M · Low

## Stage

- **Video countdown on stage.** Time left in the playing video, and in the audio. · S · Medium
- **Slide pictures and screen previews on stage.** The current or next slide as a picture, or a live copy of any screen; several make a multiviewer for the booth. (started: the current and next slide as words; a live picture of any audience screen, as many as a layout holds) · M · Medium
- **Stage layout action.** A slide or macro switches a stage screen's layout (to a sermon timer, say). · S · Medium
- **Stage screens on phones, tablets and browsers.** Needs the network API. · M · Medium
- **Chord charts.** A chart attached to a slide, and chords shown on stage, for the band. · L · Low
- **More stage items.** The labels of the current and next slide, the next group in arrangement order, the group's colour, slide count, the next playlist item, time left before the slide moves on by itself, timecode, and whether recording is on. · S–M · Low
- **Stage layout from the Screens menu, and a stage screen added from the Stage tab.** (started: the layout is picked in the Stage tab) · S · Low

## Props, messages, timers and macros

- **More action kinds for slides and macros.** Clearing announcements or a video input; your own clear buttons; media playlists; slides shown on the stage screens only; switching a saved screen setup; running another macro; live video and audio inputs; recording; MIDI and other devices. Audio and stage layout actions are under Audio and Stage. (started: Clear (all, slide, media, props, messages), Message, Prop, Timer, Audience on and off, Go to Song, and Media) · S each · Medium
- **Fill-in fields in messages.** The operator types a value at show time ("Parents of child {Number}…", a name, a room); churches that page parents this way use it every week. (started: the time or a timer in a message) · S–M · Medium
- **Adding or taking time off a running timer.** "Five more minutes" for the preacher. (started: start, stop and reset) · S · Medium
- **Timer and clock formats.** Which units a timer shows, 12- or 24-hour and date formats for the clock, and colour changes as time runs out (red at zero on stage). (started: one fixed format each for the clock and for timers) · S–M · Medium
- **Message requests from a web page.** Someone on the network asks for a message, and the operator approves it. · M · Low
- **Messages styled by a theme, with their own transition, and taken off when a timer ends.** (started: five built-in styles; taken off by hand or after a number of seconds) · S–M · Low
- **Timers from a playlist header.** Reaching a header starts, stops or resets a timer. (started: headers have a name only) · S · Low
- **Prop collections and prop transitions.** Collections are S, transitions M. (started: one flat list, with a time for each prop to clear itself) · S–M · Low
- **A polished, dependable macro editor.** Editing a macro's steps should be clear and quick, and a macro should be able to do and trigger whatever a service needs. · M · High
- **Macro collections, icons, a filter and a list view.** (started: name, colour, actions, and running when the app starts) · S · Low
- **Calendar.** Scheduled events that start a playlist, song, macro or recording, with repeats. · M · Low
- **Action tiles to drag.** Drag an action onto a slide or macro from a panel of tiles. (started: Add Action menus) · S · Low

## Library, playlists and import/export

- **Choices when importing songs.** How slides are split (paragraph, line or marker, and how many lines), and the theme, library and playlist imported songs go to. (started: File › Import Songs, drops and `sp song import` read text, lyric, ChordPro, rich-text and Word files, find sections, and share long stretches into slides of up to four lines) · S · Medium
- **Presentation import, the rest.** `.pptx` files come in as editable slides or as pictures. Still to come: `.odp` and `.key` as editable slides, tables, text that mixes looks inside one box (a text box has one look), and arrow heads and other line ends. · M · Medium
- **Lighter screens for picture-heavy decks.** The drawing library keeps up to about thirty full-size pictures per screen (roughly half a gigabyte each) until its texture fills; let them go sooner. · S–M · Medium
- **Importing from a church's existing presentation software**, so a library can move over in one go: songs, playlists and their media. (started: ProPresenter 7 songs, library folders and song bundles, with groups, arrangements, labels, notes and copyright details, and a bundle's pictures and videos; not yet playlists, or the pictures and videos a plain song document names) · M · High
- **SongSelect inside the app.** Search, preview and import from the search panel (⌘F). May need CCLI's agreement. · L · Medium
- **Cue, stream and hidden items in a playlist.** A cue (an action on its own), a video or audio stream, and items hidden from the running order. (started: playlists hold songs, saved passages, headers, pictures and videos, media playlists and placeholders) · M · Low
- **CCLI usage reporting.** Each song shown is logged, then exported or reported automatically. (started: CCLI song numbers and the licence number) · M · Medium
- **Playlist templates.** Save a service's shape and start next week from it. · S–M · Medium
- **Arrangement and key chosen per playlist item.** The same song sung differently in two services. (started: one chosen arrangement per song) · S–M · Medium
- **Planning Center Services.** A plan becomes a playlist, its items match songs in the library, and its sequences become arrangements. · L · Medium
- **OpenLyrics and EasyWorship import.** OpenLyrics covers OpenLP, Quelea and FreeShow, and could be SoftPresenter's open export format too. · M each · Medium
- **OpenSong import.** (ChordPro is done) · S–M · Low
- **Exporting a song as text or slide pictures.** (done: song packs carry songs, or a playlist, with their themes, pictures and videos) · S · Low
- **Printing.** Lyrics, slides and playlists, with notes (lyric sheets for the band). · M · Low
- **Song menu extras.** Hiding an item, moving a song, copying and pasting songs between libraries, and showing a song in its library. (started: dragging to a library, and Duplicate) · S · Low
- **Playlist menu extras and folders.** Duplicating, exporting and printing a playlist; folders for playlists and libraries in the sidebar; coloured headers. · S–M · Low
- **Search extras.** A result previewed as text or slides. (started: ⌘F searches titles and words; Return opens a result and ⌘Return adds it to the playlist; a result is dragged to a place in the playlist, a playlist or a library) · S–M · Low
- **File menu items.** New Playlist, New Library, Import, Export and Print. (started: New Song and Add Slide; the + buttons make playlists and libraries) · S · Low
- **MultiTracks, and Amazing's APlay.** APlay brings children's-ministry curriculum playlists in, ready to run. · L · Low
- **`.key` import as editable slides.** · L · Low
- **Moving to another computer, workspaces and cloud sync.** (started: one data folder, which `SOFTPRESENTER_DATA` can point elsewhere) · M–L · Low
- **Sending presentations to digital signage screens.** · L · Low

## Remote control and integrations

- **Network API.** HTTP and WebSocket (and one JSON message per line over TCP) control of slides, playlists, clears, timers, messages, props, macros, screen setups and screens, with live status updates and slide thumbnails. It runs the same commands as `sp`, so the command line, the browser, remotes and AI agents always do the same thing. Off until it is switched on in Settings, on the local network only, with passwords for watching and for control. Most of this section needs it. (started: Remote Control serves HTTPS and a WebSocket from the app, switched on in Settings, behind a password with remembered devices, running `sp`'s commands with live state, slide pictures and the live preview (docs/remote-api.md); still to come: a watching-only password, one JSON message per line over TCP) · L · Medium
- **Phone or tablet remote.** Browse playlists, put slides up, clear, run macros; passwords for watching only and for control. M as a web page the app serves, L as an app. (started: the design work's page is built into Remote Control for computers, tablets and phones) · M–L · Medium
- **The whole app in a browser.** A web page the app serves, laid out like the Service view: libraries and playlists, the slides, what is on screen, the media and the live tools (timers, messages, props, macros, clears), updating as the show changes. The show can then be run from any computer or tablet on the network, and a second operator can help from another room. Needs the network API. · L · Medium
- **Running without a window (headless).** SoftPresenter on a computer with nobody at it: it drives the screens or sends NDI, and is run from the browser page, a remote or `sp`. For a booth or rack computer that only feeds the projectors or a stream, a second computer that sends the stage screens, and scripts and agents that make slides, packs or pictures with no one watching. Comes after the network API and the whole app in a browser, which are how it is run. · M · Low
- **Stream Deck and Bitfocus Companion.** Buttons that run the show, ideally through modules that already exist. Needs the network API. · M · Medium
- **MIDI in and out.** A note map (next slide, clear, pick by number) and slide actions that send notes. · M · Low
- **DMX and Art-Net.** · M · Low
- **Switcher and device protocols.** RossTalk, AMP, CITP, GlobalCache, GVG100, Sony BVS/BVW and VDCP. · L · Low
- **Several computers following one show.** · L · Low
- **Older remote and stage socket protocols**, for remote and stage apps that still use them. · M · Low
- **Planning Center Live.** Control of the plan, and its countdown on stage. · M · Low
- **Siri and Shortcuts.** Ask Siri for slides, songs and macros. · M · Low

## The app

- **Tested Windows and Linux builds.** · L · High
- **Editable key mappings.** Rebind keys, with filters for changed keys and clashes, a reset, and keys for your own clear buttons, macros and props. (started: a read-only table) · M · Medium
- **The app in other languages.** Russian or Ukrainian menus for operators, which matters for the two-language churches SoftPresenter is made for. · M–L · Medium
- **Updates pane.** Checking now, notices of new versions, an opt-in beta and release notes; it matters more once there are releases. (started: Settings › About › What's new shows each version's changes in a window; its Updates row checks when asked, offers a newer version's download page, and shows a download's progress with Cancel where a build's updater fetches versions) · M · Low
- **Media and Import panes.** Default scaling, layer, picture duration, video loop and what audio does at its end, for imported items. · S · Low
- **Network, Inputs and Devices panes.** Built with the features they set up. · S each · Low
- **First-run guide and sample content.** Pick the audience and stage screens, and get sample songs and media. (started: one sample song) · S–M · Low
- **Help menu extras.** A knowledge base, tutorials, status, reporting an issue with setup details, and release notes. (started: Help, Report an Issue, and What's new in Settings › About) · S · Low
- **Finding the mouse pointer** (⌥M). · S · Low
- **Showing or choosing the data folder in Settings.** · S · Low
- **A search field in Settings.** · S · Low
- **Opt-in crash reports.** · S · Low

## Also planned

Ideas of our own, beyond the lists above:

- TimelyChurch songs and schedules: a church's upcoming setlists and plans as playlists, and
  their songs with translations, kept in step ([design](timelychurch.md); built against the
  Worship API TimelyChurch described, hidden until it ships; waits for its OpenAPI file to
  check the field names, then a test with TimelyChurch's demo church).
- VoxLive live translation shown on a screen.
- An API over the network with a stream of changes. The command line `sp` already controls
  the running app ([cli.md](cli.md)), and `sp mcp` lets AI assistants use it
  ([mcp.md](mcp.md)).
- Fully customisable NDI.
- Audio routing in one place.
