# Working on SoftPresenter as an AI agent

This file is for AI coding agents (Claude, Codex, Cursor and others). People are welcome to read
it too.

**Start with the [README](README.md).** It says what SoftPresenter is and who it is for, how to
build and run it, the crate map and the checks to run before a change; what works today, in
full, is in [docs/features.md](docs/features.md). This file doesn't repeat any of that; it adds
what an agent needs on top. Two more pages are
written for agents as much as for people:

- [docs/app-map.md](docs/app-map.md): every view, panel and window, what each part does, the
  matching command-line command, where the data lives and how the code is laid out.
- [docs/cli.md](docs/cli.md): `sp` (full name `softpresenter`), which controls the running app from a
  terminal or a script: every command with the replies it gives, JSON, recipes for preparing
  and running a service, and how to try it safely.

## Running the app without getting in anyone's way

SoftPresenter opens full-screen windows on other displays and can send over the network. A
careless test run puts a slide in front of whoever is using the computer.

- **Every test run sets `SOFTPRESENTER_QUIET=1`** (or uses `--capture-home FILE`). Its windows
  then open behind everything else and don't take the keyboard, its screens never go full screen,
  and it sends nothing over NDI and opens no DeckLink card.
- **Use a data folder of your own:** `SOFTPRESENTER_DATA=/some/empty/folder`. Without it, the
  app reads and writes the owner's real library, Bibles and settings in
  `~/Documents/SoftPresenter/` (on a Mac whose library hasn't moved there yet, in
  `~/Library/Application Support/SoftPresenter/`, which the first start moves). A data folder
  of your own is never moved.
- **Never run `sp` (or `softpresenter ctl`) against the owner's app.** Set `SOFTPRESENTER_DATA` and
  `SOFTPRESENTER_CONTROL_SOCKET` to a test copy first. Without them, `sp` drives the owner's
  live app, which may be on the real screens in front of a congregation.
  [docs/cli.md](docs/cli.md#trying-it-safely) has a wrapper that refuses to run without the
  test copy's socket.
- **`sp mcp` is `sp` too** (the MCP server, [docs/mcp.md](docs/mcp.md)): try it with the test
  copy's variables set, and never add it to the owner's Claude Code, Claude Desktop or any other
  assistant's setup. The card's Add buttons and `sp mcp connect` change those apps' settings: a
  test copy finds no assistant unless `SOFTPRESENTER_ASSISTANTS_HOME` names a stand-in home
  folder, with stand-ins for `claude`, `codex` and `grok` that record their words. A new `sp`
  command stays closed to assistants until `crates/sp-app/src/mcp/policy.rs` lists it.
- **Quit a test copy through its own socket** (`sp menu Quit` with its variables set), never by
  name or path (`osascript -e 'quit app "SoftPresenter"'`): macOS may send that to the owner's
  copy, or to another session's test copy.
- **Run one instance at a time,** and close it when you are done. Don't leave builds running
  that you haven't measured.
- **Before leaving a build running,** watch it idle for a few seconds:
  `top -l 5 -pid <pid> -stats pid,cpu,mem` (top's MEM includes GPU memory). Expect about 0% CPU
  and flat memory. Then run a few seconds with `SOFTPRESENTER_STRESS=1` (continuous redrawing)
  and check that memory stays flat. A redraw loop once leaked a gigabyte a second.
- **A test copy on another Mac** (a test machine reached over SSH) never goes through `open`
  or anything else of LaunchServices, and is never left at a path that outlives a sign-out:
  remove the copy and its folders as soon as its check is done. macOS reopens, after a session
  restarts, the apps that were open in it, a test copy too, without its variables: on the
  owner's library, not quiet. It happened: a hung GPU restarted the session, and the copy that
  came back hung it again.
- **You may not be able to see the screen.** `--capture-home FILE` and the
  `SOFTPRESENTER_CAPTURE_*` switches save pictures of the windows, so you can look at what you
  built.

## Testing switches

The app reads these environment variables so that a test, or an agent, can drive it without a
person:

| Switch | What it does |
|---|---|
| `SOFTPRESENTER_RELEASE_VERSION=<version>` (when building) | A pre-release made from the public source for GitHub's releases calls itself this (`0.9.0-dev.16`) in About, the log and Check for Updates, so it is told of newer pre-releases; nothing else changes (unlike a development build, it offers no song catalog) |
| `SOFTPRESENTER_QUIET=1` | Windows open behind everything, no full screen, no NDI, no DeckLink card; no anonymous reports are sent, and the one-time notice about them is not shown (the same in any copy with `SOFTPRESENTER_DATA`, or one that only saves a picture); Remote Control, if switched on, listens on 127.0.0.1 only; so does AirPlay, which then announces nothing; sound plays, timed and mixed as usual (NDI included), but the speakers get silence |
| `SOFTPRESENTER_DATA=<folder>` | Keep the library, Bibles, settings and log in that folder instead of the owner's |
| `SOFTPRESENTER_CONTROL_SOCKET=<path>` | Where `sp` and the app meet; a short path for a test copy (a socket file on Windows too) |
| `SOFTPRESENTER_TEST_ROOT=<folder>` | For test programs built on one computer and run on another (Windows tests built on the Mac): the golden pictures and shipped Bibles are read from that copy of the repository instead of the build's own source folder |
| `--capture-home FILE` | Go to the third slide, save a PNG of the home window, quit |
| `SOFTPRESENTER_CAPTURE_AFTER=<seconds>` | How long a capture run waits before taking its picture |
| `SOFTPRESENTER_CAPTURE_*` | Capture a particular view or window instead (`_BIBLE`, `_EDITOR` (`=props` for the props), `_THEMES`, `_SETTINGS`, `_STAGE_LAYOUTS`, `_OUTPUT`…) |
| `SOFTPRESENTER_GO_LIVE=1` | Start with the first slide on screen and the first media item behind it |
| `SOFTPRESENTER_OPEN_ITEM=<n>` | Open the nth item of the playlist in front, as a click does |
| `SOFTPRESENTER_PICK=songs:0,2` | Pick out songs, media items (`media:0,1`) or the playlist's items (`items:1,3`) by place, as Command-clicks do |
| `SOFTPRESENTER_DROP=<files>` | Drop files on the window (several separated as in PATH), as dragging from the Finder does |
| `SOFTPRESENTER_DROP_AT=X,Y` | Where in the window (points) the dropped files land |
| `SOFTPRESENTER_HOVER=<files>` | Files held over the window, so the place they would land lights up |
| `SOFTPRESENTER_OPEN_DIALOG=add-media` | A few frames in, open the media bin's Add window (the system's file window) as a click on the empty bin does, to see that the window, `sp` and the screens go on while it is open. The log says `[dialog] Add Media opened`, and once it is closed how long it was open, how many files were chosen and how many times the window was drawn meanwhile |
| `SOFTPRESENTER_DIALOG_OWNED=1` | On Windows, a file window or question has the app's window as its owner, as the system's usual way has it, to compare with the app's own way (none): Windows then switches the app's window off while it is open, so it takes no clicks and no drops |
| `SOFTPRESENTER_MENU=<ids>` | Choose menu bar items by id, one every few frames |
| `SOFTPRESENTER_WINDOW_SIZE=1100x700` | Open the window at another size |
| `SOFTPRESENTER_CAPTURE_SIZE=2560x1440` | A capture run lays the home window out as if it were that big (points), whatever room the display has, at the size Interface size gives it, and saves the picture at that size at the display's scale (5120 × 2880 on a Retina display), drawn off the window: to see the window at sizes this computer's display can't hold |
| `SOFTPRESENTER_SURFACE_RACE=<n>` | The operator's window's surface starts at half its window's size, as on Windows where a new window takes its size a moment after it opens, and the next n set-ups of that surface are lost (not made, and said to have failed), as when the screens' thread hands the GPU work at that moment time after time; Screens and Settings are left alone. Each is tried again later, the wait doubling from a frame up to a second. The log says (`[window] Testing aid:`) the sizes it starts with, each lost set-up, the size the surface has when it follows its window again, and the first frame drawn after (`surface_setup.rs`) |
| `SOFTPRESENTER_STRESS=1` | Redraw continuously, to check memory stays flat |
| `SOFTPRESENTER_SCREENS_FILL=<display name>` | A quiet run's screens fill that one named display (never the main one), through the real window code, instead of opening in a window. For a virtual display made by `spikes/virtual-display` and nothing anyone looks at; with `SOFTPRESENTER_SCREENS_ON_TOP=1` the windows also take the real level above the menu bar |
| `SOFTPRESENTER_SCREENS_ON_TOP=1` | Keep the screens' windows above all others, so a timing run on one display really draws them (in a quiet run too, where they still don't take the keyboard) |
| `SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS=1` | Screens shown in an ordinary window get the X that a screen filling a display shows when the pointer moves over it (`close_button.rs`), so it can be tried without filling a display |
| `SOFTPRESENTER_CLOSE_BUTTON_PLAY=1` (or `move`) | Play the pointer over the first screen's window that has the X, through the same steps as real pointer events: moving across it for 1.2 s, resting on the X, then pressing it once, which switches that screen off (`move` only moves, and leaves the X to fade). With the switch above in a quiet run, where nobody can move the pointer there; the log says each step (`[screens]`) |
| `SOFTPRESENTER_RESIZE_AID=<from>x<to>` | Resize the operator's window in 40 steps, 50 ms apart, from that width (points) to the other and back, saying on stderr for each step the window's size, the size its surface was drawn at and how many frames were drawn at the new size, then quit |
| `SOFTPRESENTER_TERMINATE_AFTER=<s>` | On a Mac, that many seconds after it started the app asks macOS to terminate it the way the Dock's Quit, a log-out, a restart and a shut-down do (never through the app's own Quit), to try what it does then: it must save and stop as its own Quit does |
| `SOFTPRESENTER_STOP_DRAWING_AFTER=<s>` | Stop drawing the window after a while, as minimising does |
| `SOFTPRESENTER_LOG_REPAINTS=1` (or a file named `log-repaints` in the data folder) | Once a minute the log says (`[repaints]`) how many times the operator's window was drawn, how long each drawing took and where (laying out, tessellating, handing the GPU textures and triangles, waiting for the window's picture, submitting, presenting), and what asked for each: a call in the code by its place (`sp-app/src/main.rs:1234`), a new Program preview picture, an event the window was told (`told CursorMoved`), a time the window set itself and why, or, for a time the toolkit asked for, the place in the code that asked for the soonest one (`the toolkit for sp-app/src/studio.rs:1221`, the clock); and how often the app's loop woke and which of its clocks it slept until. The file is for a computer where setting a variable is a bother (a tester's PC). Read once as the app starts |
| `SOFTPRESENTER_LOG_KEYS=1` | With `SOFTPRESENTER_DATA` set: the log says, for each key pressed in the home window, whether it waited for the window to draw the keys before it (and how long), where it went (the find box, the show, Enter, the window) and what was picked, chosen and on screen after it, and each drawing of the window that took 40 ms or more; a letter is said as "a letter". For a person pressing keys in a test copy on a slow computer |
| `SOFTPRESENTER_GPU_TOO_SLOW=<n>` or `<n>@<seconds>` | The screens give up the next n pictures they must wait for the GPU for, as when it is too slow (from that many seconds after the start) |
| `SOFTPRESENTER_HOLD_WINDOW_PICTURE=<ms>@<seconds>` | The first picture a screen's window asks for that many seconds after the start comes that many milliseconds late, once, as when the window system is slow to give a window its next picture (works in a quiet run) |
| `SOFTPRESENTER_SLOW_PRESENT=<ms>` or `<ms>@<screen>` | Every picture a screen's window asks for comes that many milliseconds late (only the window of the screen of that name, with one), time after time, as when the window system is slow to take a window's pictures and give them back (a display behind a splitter, a busy window system): the window is shown fewer pictures, and nothing else should be held up by it (works in a quiet run) |
| `SOFTPRESENTER_FADE_CACHE=0` (or `off`) | The way back to the old drawing, with no new build: a fade draws both slides again at every step, and a slide over a playing video is drawn again for each picture of the video, as before. By default a fade on a solid screen draws each slide once into a texture and then only blends the two (`Output::cache_sides`), and the slide over a video is drawn once into a texture that each picture of the video is laid under (`Output::steady_scene`). A file named `fade-cache-off` in the data folder does the same without setting anything. Read once as the app starts |
| `SOFTPRESENTER_GPU_MEMORY=small` | On DirectX 12 and Vulkan, GPU memory comes from wgpu's small blocks (8 to 64 MB) instead of its usual 128 to 256 MB ones, which are kept once made: to compare, on a PC with no new build, the app's memory and the screens' timing (a 4K picture then gets a block of its own each time it is made, on the screens' thread). A Mac's Metal takes no notice. The log's `[gpu]` line says which was asked for. Read once as the app starts |
| `SOFTPRESENTER_OUTPUT_ROOM=<lines>` | The screens' painter has room for that many lines whatever is drawn, so a fuller slide is too heavy for them |
| `SOFTPRESENTER_SETUP=1` (or a step: `free-songs`, `bibles`…) | Show the first-start setup in a quiet run too, at that step |
| `SOFTPRESENTER_CATALOG_OFFERED=1` | Offer the song catalog's windows (the setup's Free Songs step, Settings › Integrations › Worship Leader App) in a build that isn't a development build. Development builds (compiled with `SOFTPRESENTER_DEV_VERSION` set) offer them anyway; releases don't, until Worship Leader App answers |
| `SOFTPRESENTER_CATALOG_URL=<address>` | Fetch song packs from there (a local test server; `http` allowed for 127.0.0.1) instead of Worship Leader App's own address |
| `SOFTPRESENTER_SONGBOOK_URL=<address>` | Fetch the free songbooks from there (a local test server; `http` allowed for 127.0.0.1 and localhost) instead of the site that shares them; each file must still have the size and SHA-256 the app expects |
| `SOFTPRESENTER_FFMPEG_FETCH_URL=http://127.0.0.1:<port>/<file>` | A test copy keeps the videos' FFmpeg in a folder of its own as a person's copy on Windows does (`ffmpeg_fetch.rs`), on any system: `ffmpeg/` in its own data folder (never `%LOCALAPPDATA%`, never beside the program), checked at start, copied from beside the program when the two programs there have the SHA-256s expected, or else fetched from a server on this computer instead of the project's GitHub release; once checked, the lookup runs that copy first. Honoured only together with `SOFTPRESENTER_DATA`, and only for `http://127.0.0.1` or `http://localhost`; set to anything else, nothing is copied or fetched at all. The zip and the programs must still be the real ones, to the byte and their SHA-256, unless `SOFTPRESENTER_FFMPEG_FETCH_EXPECT=<bytes>:<zip sha256>:<ffmpeg.exe sha256>:<ffprobe.exe sha256>` gives a test's own (honoured only with it). Without the switch a test copy (quiet, its own data folder, or a capture) never copies or fetches |
| `SOFTPRESENTER_FREE_SONGBOOKS=1` | Open the Free Songbooks window as the app starts, as Import Songs › Free Songbooks… does |
| `SOFTPRESENTER_TIMELYCHURCH_URL=<base address>` | Offer TimelyChurch (Settings › Integrations › TimelyChurch and `sp timelychurch`), hidden until its Worship API ships, with that address suggested for connecting; without it nothing is ever asked of the network. For the demo church in the tests (`timelychurch/mock.rs`: `cargo test -p sp-app timelychurch::mock::serve -- --ignored --nocapture` serves it at `http://127.0.0.1:4320`, pairing code `DEMO-2026`), `http` is allowed for 127.0.0.1 and localhost only. Never point it at TimelyChurch's real servers from a test copy |
| `SOFTPRESENTER_CAPTURE_PREFERENCES=<page>` | Open Settings at a page in a capture run, by its English name (`slides`, `phone-remote`, `ai-assistants`…), or a service's own page (`voxlive`, `canva`, `songbooks`, `timelychurch`; `voxlive-end`, `canva-end` or `timelychurch-end` scrolled to its end), or Slides scrolled on to In your window with Interface size (`interface-size`), or Phone remote with Port and certificate folded out (`remote-more`), or AI assistants scrolled on to its tiles (`mcp-tiles`), to Other apps opened (`mcp-apps`) or to What gets copied opened (`mcp-copied`), or Song files with Find songs' Find pressed (`find-songs`), and Import pressed too once it has found (`find-songs-import`), or Stop once it has found two (`find-songs-stop`, with `SOFTPRESENTER_FIND_SONGS_SLOW` so it hasn't ended first), or before a search with a folder added to where it looks (`find-songs-folder`, a folder named Worship songs), or About with the What's new window open over it (`whats-new`), scrolled to its end (`whats-new-end`); the Settings window is saved beside the picture as `<name>.settings.png` |
| `SOFTPRESENTER_FIND_SONGS_HOME=<folder>` | The home folder a test copy's Find songs on this computer (Settings › Song files, `sp songs find`) looks in: its `Documents`, `Downloads`, `Desktop`, and `SoftProjector` for SoftProjector's folder. A test copy (quiet, its own data folder, or a capture) without it looks in no usual place, only in folders added, so it never reads the owner's folders |
| `SOFTPRESENTER_FIND_SONGS_SLOW=<ms>` | In a test copy, each folder Find songs looks in, and each song its import reads, takes that many milliseconds longer, so a picture can catch the card looking and bringing songs in |
| `SOFTPRESENTER_FIND_SONGS_CLOUD=<words, by commas>` | In a test copy, Find songs takes a file whose name holds one of the words as only in the cloud, as iCloud Drive or OneDrive keeps one (nothing on this computer can make one), so a picture can show its rows for such files |
| `SOFTPRESENTER_FIND_SONGS_FAIL=<n>` | In a test copy, Find songs' search thread panics once it has listed n sources, so a picture can show a search that went wrong (the panic is caught, and logged as one) |
| `SOFTPRESENTER_SETTINGS_SIZE=1100x700` | Open the Settings window at that size (points), smaller than its least size too, to see its pages at the sizes the design was drawn at |
| `SOFTPRESENTER_ASSISTANTS_HOME=<folder>` | The home folder a test copy looks for assistant apps in (Settings › AI assistants, `sp mcp connect`): their settings files (`Library/Application Support/Claude/`, `.cursor/`, `.copilot/`, `.gemini/`, and `AppData/Roaming` for Windows' own) and their programs (`.local/bin/claude`, `.local/bin/codex`, `.grok/bin/grok`), and nowhere else, not the PATH. A test copy without it finds none, so it never reads or changes the owner's |
| `SOFTPRESENTER_LANGUAGE=<es\|ru\|uk\|en>` | Stands in for the computer's language, so a test copy starts in that language (the app's own words, and the Bible view's reading language), unless the Language setting chose one; for captures and tests. Several separated by commas (`ru,uk,en`) stand in for the computer's preferred languages, its own first (the Bibles window lists Bibles in them first) |
| `SOFTPRESENTER_REPORTS_TEST_URL=http://127.0.0.1:<port>` | A test copy sends its anonymous reports to a server on this computer (the reports Worker run locally), as a person's own copy sends them to the project's: the one-time notice is shown, the first report goes a minute later, then one every fifteen minutes and one at quit. Honoured only together with `SOFTPRESENTER_DATA`, and only for `http://127.0.0.1` or `http://localhost`; with any other address the copy sends nowhere. `sp reports`, `reports show`, `reports on|off` drive it without a pointer; `SOFTPRESENTER_SETUP=reports` opens the setup at its Reports step (`reports-open` with See what's sent opened) and `SOFTPRESENTER_CAPTURE_PREFERENCES=reports-sent` Settings › About with it opened |
| `SOFTPRESENTER_REGION=<DE>` | Stands in for the computer's region (a country's two-letter code), which decides whether anonymous reports start on or off (off in the EU, the EEA and the UK): a test copy then starts as a computer there does |
| `SOFTPRESENTER_HIDE_NDI=1` | Behave as on a computer without NDI installed |
| `SOFTPRESENTER_SLOW_PAGES=<ms>` | Each page of a PDF being made into slides takes that many milliseconds longer, to try a quit part way through (the pictures are drawn at the next start) or to see "Page N…" |
| `SOFTPRESENTER_SLOW_WRITES=<ms>` | Each file the library's writer writes (a song, the playlists, the libraries, the media bin) takes that many milliseconds longer, as on a computer whose disk or virus scanner is slow (Windows Defender looks at every new file): to see that a big import never holds up the window or `sp`, and that quitting waits until every file is written. Only with `SOFTPRESENTER_DATA` set |
| `SOFTPRESENTER_HIDE_PICTURE_APPS=1` | Behave as on a Mac where no app can open a presentation to draw it (the import sheet greys As Pictures) |
| `SOFTPRESENTER_VIDEO_COPY=1` | Videos the system plays reach the GPU as copies of their pixels, as on a computer where they can't go as the decoder's own memory (to compare the two ways) |
| `SOFTPRESENTER_VIDEO_RGBA=1` | The video FFmpeg plays behind the words comes as RGBA pixels FFmpeg converts on the processor, as before, instead of as planes the GPU turns into colours (to compare the two ways, or go back with no new build) |
| `SOFTPRESENTER_NDI_LOSE=<n>` or `<n>@<place>` | The next n NDI pictures somebody is watching for count as lost on their way back from the GPU (with a place, only that picture of each drawing: 1 is the first) |
| `SOFTPRESENTER_NDI=<name>` | Also send over NDI under this name, this run only |
| `SOFTPRESENTER_PRETEND_NDI=1` (or a number of seconds) | Every NDI source sends to a stand-in instead of NDI, in a quiet run too and on a computer without NDI: it always has one receiver, takes each picture at the source's rate as NDI's own pacing does, and sends nothing anywhere. With a number, each source says in the log how its pictures went out every that many seconds instead of every minute. With `SOFTPRESENTER_NDI=<name>`, a test has a watched NDI screen whose timing it can read |
| `SOFTPRESENTER_NDI_WATCH=<name>` or `<name>:<seconds>` (for `cargo test -p sp-ndi -- --ignored --nocapture a_named_source`) | That test watches the NDI source on the network whose name holds this, as a receiver does, for that many seconds (20 without), and says how its pictures arrived: how many, how many of them new, and the waits between them |
| `SOFTPRESENTER_MIRROR_PORT=<n>` | The port a quiet run's `softpresenter-mirror` listens on (any free one without it), so a test sender knows where to connect. A quiet run's helper listens on 127.0.0.1 only and never announces itself, whatever this says |
| `SOFTPRESENTER_MIRROR_MODEL=<model>` | `softpresenter-mirror` says it is this model instead of its own (`SoftPresenter1,1`), to find out with a real phone whether a name of our own is accepted. Only the owner decides whether a build may ever say another model |
| `SOFTPRESENTER_MIRROR_ANNOUNCE=system` | On a Mac, `softpresenter-mirror` announces itself through the system's own Bonjour (the `dns-sd` tool) instead of its own responder, to find out with a real phone which is seen more reliably. Never in a quiet run, which announces nothing |
| `SOFTPRESENTER_MIRROR_H264_ONLY=1` | `softpresenter-mirror` leaves out the feature bit that lets a sender choose HEVC, to tell a codec problem from a protocol one |
| `SOFTPRESENTER_HIDE_DECKLINK=1` | Behave as on a computer without Desktop Video (the DeckLink driver) |
| `SOFTPRESENTER_PRETEND_DECKLINK=1` | List made-up DeckLink devices ("Pretend DeckLink (1)"…) in place of real ones; what they are sent goes nowhere |
| `SOFTPRESENTER_PRETEND_DECKLINK_STALL=<s>` | Each pretend DeckLink card stops taking frames that many seconds after it starts, as a card taken away does |

There are more, each beside the code that reads it with a `// Testing aid:` comment. List them
all with:

    git grep -ohE 'SOFTPRESENTER_[A-Z_]+' crates | sort -u

A new switch gets the same `// Testing aid:` comment, and a `Testing aid:` line in its commit
message.

## End-to-end tests

`crates/sp-app/tests/e2e.rs` starts the app Cargo built as a quiet test copy and drives it
through `sp`, as a script would: songs, slides and the six clears, a Bible passage, a picture
behind the words, a playlist, audio, a 300-song import on a slow disk, Remote Control's sign-in,
snapshot and sign-out over HTTPS, Check for Updates against a stand-in on this computer, and a
`--capture-home` picture. It runs with `cargo test`, in about half a minute on a Mac. The
automatic checks run it on all three systems with cargo-nextest, apart from the other tests and
one test at a time (`--no-capture`: the harness's one-copy-at-a-time lock holds within one
process, and nextest gives each test a process of its own). One test alone:

    cargo test -p sp-app --test e2e remote_control -- --nocapture

- **Its own copy, always quit.** Each test starts a copy of its own, with
  `SOFTPRESENTER_QUIET=1`, `SOFTPRESENTER_DATA` and `SOFTPRESENTER_CONTROL_SOCKET` in a
  temporary folder named `spe2e-<process>-<n>` (under `/tmp` on a Mac and Linux), and every other
  `SOFTPRESENTER_*` variable taken out of its environment and `sp`'s, so it never reaches the
  owner's library, log or socket. Copies run one at a time. When a test ends, passed or failed,
  its guard sends `menu Quit` through the copy's own socket and kills the copy if it hasn't gone
  30 seconds later. A failed test keeps its folder (the copy's log, standard output and error),
  and says where.
- **What a quit is checked for:** exit code 0, no `[crash]`, `panicked` or `Graphics error` in
  the log or on the standard error, no reply to `sp` left unwritten, and the socket gone.
- **Skipped, out loud:** `SOFTPRESENTER_E2E=off` (or `off (why)`) skips them all, as does a Linux computer with
  no display; a part a computer can't do (sound, with no sound output) is skipped with the
  reason printed, and as a notice on a GitHub run.
  On GitHub (`GITHUB_ACTIONS`) only a chosen skip passes (`SOFTPRESENTER_E2E=off (why)`): a
  Linux job that lost its display fails, and so does a missing FFmpeg in sp-video's tests on
  the Linux job, which installs it.
- **Switches it uses:** `SOFTPRESENTER_SLOW_WRITES` (the import), `SOFTPRESENTER_SLOW_REPLIES` (each
  reply to `sp` written late, to see `menu Quit` still answered), `SOFTPRESENTER_RELEASES_URL`
  (the updates stand-in), `SOFTPRESENTER_LANGUAGE=en`.
- **On another computer** (the Mac's Intel build on an Intel Mac, which this Mac can't run):
  build the tests without running them (`cargo test --release --target x86_64-apple-darwin -p
  sp-app --test e2e --no-run` prints the test program's path), copy that program and the app
  there, and run it with `SOFTPRESENTER_E2E_APP=<the app's program>` (instead of the one Cargo
  built, whose path is this Mac's) and `SOFTPRESENTER_E2E_FOLDER=<a folder of its own>` (for the
  tests' folders, instead of `/tmp`).
- **The tests need a graphics device,** as the app does. CI gives them one: Metal on the Mac,
  WARP (DirectX 12 in software) on Windows, and on Linux Mesa's software Vulkan under `xvfb-run`.
- **To draw with WARP on a PC that has a graphics card** (as a PC with no graphics driver
  does), set wgpu's own `WGPU_BACKEND=dx12` and `WGPU_ADAPTER_NAME=Microsoft Basic Render Driver`
  for a test copy. The log's `[gpu]` line then ends "software graphics: pictures are worked out
  on the processor": the painters run the renderer's early stages themselves there
  (`sp_render::early_stages_on_processor`), because WARP faults on one of them.

## Rules

- **`unsafe` code** is allowed only in `sp-video`'s two macOS modules (`mac` for videos and
  pictures, with its `gpu` part that hands a decoded picture's memory to wgpu through wgpu's
  Metal interface and its `cover` part that puts a screen's window on its display and listens for
  the displays changing, `web_mac` for web pages), `sp-ndi`'s `runtime` module, `sp-winapi`'s `windows` module (the few Windows calls the
  app needs: joining the console a command was typed in, asking a monitor for its work area,
  renaming a file into place written through to the disk,
  asking Windows the names the monitors give themselves and the country the user's location is set to, reading how much processor time and
  memory the app, the programs it started and the computer use (its `stats` part, for the
  anonymous reports), putting a folder on the user's PATH and
  taking it off again, holding a named mutex that keeps the app to one copy per data folder,
  letting the copy running come to the front when another start asks and asking Windows the
  user's display languages for the message such a start shows when that copy doesn't answer, for the app's file windows, setting up OLE on the thread that shows one, bringing it back to the front and giving the operator's window the front back when it closes, in its `security` part, reading and setting who owns and may open the
  control socket's folder and finding the user the app runs as, and, in its `credentials` part,
  reading, writing and removing the user's entries in Credential Manager, where keys are kept) and
  `sp-decklink`'s `ffi`
  module, and every block says why it is sound. `web_mac` also calls two web view settings outside Apple's published interface
  (window occlusion detection and the page background), only when the web view answers to them;
  the owner agreed to that on 2026-11-20. Anywhere else, find a safe way or ask the owner.
  The copy of Vello in `third_party/vello` is not a workspace crate and carries the one
  `unsafe` block Vello itself has (it creates the shader modules); changes made to it here
  add none. The copy of Fontique in `third_party/fontique` is not a workspace crate either and
  carries Fontique's own `unsafe` code (calls into the systems' font services, and mapping
  font files while it looks for fonts); changes made to it here removed two of its maps and
  added one, which maps only a font file nothing else can change (a Mac's read-only system
  volume, or a private copy left with no name) and says why it is sound. The owner agreed to
  carrying it, with `unsafe` code of its own, on 2026-11-21. The copy of smithay-clipboard in
  `third_party/smithay-clipboard` (the clipboard on Linux Wayland desktops, which also takes files
  dragged onto the window there) is not a workspace crate either and carries smithay-clipboard's
  own `unsafe` code (joining the window library's connection, and reading and writing the
  clipboard's pipes); changes made to it here add none. The owner agreed to carrying it, with
  `unsafe` code of its own, on 2026-11-26. `sp-decklink`'s `ffi` module calls
  the DeckLink driver through SoftPresenter's own C++ bridge (`bridge/bridge.cpp`, compiled
  with the DeckLink SDK's headers in `third_party/decklink-sdk`), the same arrangement as NDI's;
  the owner agreed to that on 2026-11-21. The bridge never calls back into Rust, and its
  functions take only plain values and buffers with their lengths.
- **Live safety.** Nothing slow (reading files, decoding, network, waiting on NDI) runs on the
  thread that draws the screens. The audience must never see a freeze or the wrong slide because
  the operator's window was busy.
- **Clean room.** Behaviour familiar from other presentation software is fine. Their names,
  wording, names for features, icons, artwork and looks are not, anywhere: code, comments, the
  interface, file formats, commit messages, the README, issues and pull requests. Nothing comes
  from decompiling another product or from file formats extracted from one. One exception,
  agreed by the owner: the song importer reads ProPresenter 7 `.pro` documents using
  greyshirtguy's MIT-licensed definitions, and (asked for by the owner on 2026-11-21)
  `.proBundle` files, a ZIP of one or more `.pro` and the pictures and videos they show, whose
  layout was learnt from the MIT-licensed propresenter7-php-lib's notes and from the owner's
  own bundles; it may name ProPresenter as a plain fact where it says what it reads, and
  never writes those formats
  ([docs/licences/song-documents.md](docs/licences/song-documents.md)). A second, the same way:
  the song importer reads SoftProjector's song databases and songbooks, and the Bible importer
  its Bible files (`.spb`), whose layout was learnt from SoftProjector's GPL-3.0 source (table
  and column names, how a song's text is split, the lines of a Bible file); none of its code is
  used, and SoftProjector may be named where the importer says what it reads. Those are SQLite files, read by SoftPresenter's own reader
  (`crates/sp-model/src/sqlite_read.rs`), written from SQLite's published description of the
  file: no SQLite library is linked and no SQLite tool is run, on any system. A third, agreed by the owner on 2026-11-24: `softpresenter-mirror`
  (`crates/sp-mirror`), the separate program that receives an iPhone, iPad or Mac screen,
  follows UxPlay's `lib/` (LGPL-2.1-or-later) and carries a Rust port of playfair (GPL-3.0),
  whose tables came from someone else's study of Apple's own program; it may name Apple,
  AirPlay and those projects as plain facts where it says what it does, and it keeps their
  notices ([docs/licences/screen-mirroring.md](docs/licences/screen-mirroring.md)). It stays a
  separate program: `sp-app` never depends on that crate, the crate depends on no `sp-*` crate,
  and SoftPresenter's additional permission for NDI and DeckLink does not extend to it. The
  interface calls the feature AirPlay (in every language; its switch says what it does, "Show an
  iPhone, iPad or Mac screen") and says, wherever it is switched on, that it is unofficial.
- **Content.** No song lyrics or Bible text that isn't public domain, in tests or anywhere else.
  Tests use text written for them.
- **Privacy.** No secrets, tokens, local paths (`/Users/…`), links to private sessions or
  documents, or personal details of real people.
- **Anonymous reports** ([PRIVACY.md](PRIVACY.md), [docs/anonymous-reports.md](docs/anonymous-reports.md)).
  A report is built only from the closed types in `crates/sp-app/src/reports/format.rs`: views,
  tools, events and what is on screen are codes the program made up, never words, and nothing
  that could hold a name (a title, a file, a display, a person) is ever put in one. A new field
  changes the server, the format's page, the fixtures and PRIVACY.md together. Text for a
  report is dropped when in doubt, never cleaned (`reports/scrub.rs`). Nothing is counted,
  written or sent before the person has been told, and a test copy never sends.
- **Third-party code.** Before linking anything new, read its licence and record it in
  `docs/licences/`, and in `NOTICE.md` if it ships with the app.
- **Tests that need a GPU** go in `crates/sp-render/tests/golden.rs`, which the automatic checks
  leave out, or are marked `#[ignore]` with the reason. The one exception is the
  [end-to-end tests](#end-to-end-tests), which compare no pictures and run everywhere.
- **The docs follow the code.** When what works changes, update
  [docs/features.md](docs/features.md) in the same commit (and the README's one-line overview
  if a whole area changes). When a planned feature is built, take it off
  [docs/roadmap.md](docs/roadmap.md).
- **So does the changelog.** Every change a church would notice adds one line under
  `## Unreleased` in [CHANGELOG.md](CHANGELOG.md), in the same commit (the first takes the place
  of "Nothing yet."), as one line, `- **Name.** One plain sentence.` and any detail, in its part:
  `### New` (something you couldn't do before), `### Better` (it worked, and now works or looks
  better) or `### Fixed` (it was wrong), in that order, below the one plain sentence under the
  heading that says what the version is about. Write it for the volunteer running the service:
  what changed for them, with no commit ids, code names or file names. A fix says what was
  wrong as they saw it ("Some looping videos froze on their last picture instead of going
  round; they loop now.").
  Internal work (tests, refactoring, testing aids, docs) adds no line. When a version is made,
  its heading takes the place of Unreleased (`## 0.9.0 — 27 November 2026`) and a new Unreleased,
  saying "Nothing yet.", goes on top.
- **Words.** British spelling (colour, licence, behaviour). Plain words in the interface, the
  way a church volunteer would say them.

## Commits

Follow the commit standard in [CONTRIBUTING.md](CONTRIBUTING.md#commits). The three things that
matter most:

1. The subject line says what changed for the person running a service, in plain English, in
   at most 72 characters, with no `feat:`-style prefix.
2. The body ends by saying what was checked and what wasn't.
3. A commit you helped write ends with your co-author line (for Claude:
   `Co-Authored-By: Claude <model> <noreply@anthropic.com>`). Never add a `Signed-off-by:` line
   yourself, and never add links to your sessions.

## Workflow

- **Until 1.0,** the owner and the owner's agents commit straight to `master` and push.
  Everyone else forks and sends a pull request.
- **Never force-push or rewrite `master`.** To undo a pushed commit, revert it.
- **Several agents at once:** each works in its own git worktree. Don't write in another
  session's working tree; hand it your commits to cherry-pick instead.
- **A Mac build that leaves this computer** is universal, for Apple silicon and Intel Macs:
  `scripts/macos-app.sh --universal` (a build with `SOFTPRESENTER_RELEASE_VERSION` or
  `SOFTPRESENTER_DEV_VERSION` set is universal anyway). Every Mac program is made for macOS 11,
  the oldest the app runs on (`.cargo/config.toml`), so a call into anything macOS added later
  is asked for only when the system answers to it (`respondsToSelector:`, or a check of the
  version), never made unguarded.
- **A build that leaves this computer** (sent to a tester, released, copied to another machine)
  carries no one's home folder: `scripts/macos-app.sh` shortens it to `~` with
  `--remap-path-prefix`; for any other build to share (Windows, Linux), set
  `RUSTFLAGS="--remap-path-prefix=$HOME=~"` yourself. Check with
  `strings <program> | grep -c "$HOME"`, which must be 0.

## Releases

- **Only from green checks.** Nothing is built to share, whether a release or a development
  build, from a commit whose Checks, Windows and Linux runs haven't all passed on GitHub. The
  commit must be on `master` and the last commit of its push: GitHub runs a push's checks on
  its last commit only. `.github/scripts/checks-green.sh <owner/repo> <commit>` says whether
  a commit's checks are green.
- **A tag makes a draft.** Give the version its heading in CHANGELOG.md (see Rules), push
  that to `master`, then tag the commit and push the tag: `git tag v0.9.0-dev.28 <commit>`,
  then `git push origin v0.9.0-dev.28`. `.github/workflows/release.yml` waits for the commit's
  checks (up to 90 minutes), and stops if one is red. It then builds the Mac zip (universal,
  signed ad hoc), the Windows zip and the Linux .deb on GitHub's machines, and makes a draft
  pre-release for the tag with the three files and that CHANGELOG section as its notes.
- **A person publishes it,** by hand on the Releases page, after reading and editing the notes
  and trying the downloads. The workflow never publishes, and leaves alone a release already
  made for the tag.
- **Past red checks** only by running the workflow by hand (Actions › Release › Run workflow)
  with a written reason, which goes into the draft's notes.
