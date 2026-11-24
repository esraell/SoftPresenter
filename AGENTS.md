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
- **Quit a test copy through its own socket** (`sp menu Quit` with its variables set), never by
  name or path (`osascript -e 'quit app "SoftPresenter"'`): macOS may send that to the owner's
  copy, or to another session's test copy.
- **Run one instance at a time,** and close it when you are done. Don't leave builds running
  that you haven't measured.
- **Before leaving a build running,** watch it idle for a few seconds:
  `top -l 5 -pid <pid> -stats pid,cpu,mem` (top's MEM includes GPU memory). Expect about 0% CPU
  and flat memory. Then run a few seconds with `SOFTPRESENTER_STRESS=1` (continuous redrawing)
  and check that memory stays flat. A redraw loop once leaked a gigabyte a second.
- **You may not be able to see the screen.** `--capture-home FILE` and the
  `SOFTPRESENTER_CAPTURE_*` switches save pictures of the windows, so you can look at what you
  built.

## Testing switches

The app reads these environment variables so that a test, or an agent, can drive it without a
person:

| Switch | What it does |
|---|---|
| `SOFTPRESENTER_QUIET=1` | Windows open behind everything, no full screen, no NDI, no DeckLink card; Remote Control, if switched on, listens on 127.0.0.1 only |
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
| `SOFTPRESENTER_MENU=<ids>` | Choose menu bar items by id, one every few frames |
| `SOFTPRESENTER_WINDOW_SIZE=1100x700` | Open the window at another size |
| `SOFTPRESENTER_STRESS=1` | Redraw continuously, to check memory stays flat |
| `SOFTPRESENTER_SCREENS_FILL=<display name>` | A quiet run's screens fill that one named display (never the main one), through the real window code, instead of opening in a window. For a virtual display made by `spikes/virtual-display` and nothing anyone looks at; with `SOFTPRESENTER_SCREENS_ON_TOP=1` the windows also take the real level above the menu bar |
| `SOFTPRESENTER_SCREENS_ON_TOP=1` | Keep the screens' windows above all others, so a timing run on one display really draws them (in a quiet run too, where they still don't take the keyboard) |
| `SOFTPRESENTER_RESIZE_AID=<from>x<to>` | Resize the operator's window in 40 steps, 50 ms apart, from that width (points) to the other and back, saying on stderr for each step the window's size, the size its surface was drawn at and how many frames were drawn at the new size, then quit |
| `SOFTPRESENTER_STOP_DRAWING_AFTER=<s>` | Stop drawing the window after a while, as minimising does |
| `SOFTPRESENTER_TRACE_REPAINT=1` | Log what keeps asking for the window to be redrawn |
| `SOFTPRESENTER_GPU_TOO_SLOW=<n>` or `<n>@<seconds>` | The screens give up the next n pictures they must wait for the GPU for, as when it is too slow (from that many seconds after the start) |
| `SOFTPRESENTER_HOLD_WINDOW_PICTURE=<ms>@<seconds>` | The first picture a screen's window asks for that many seconds after the start comes that many milliseconds late, once, as when the window system is slow to give a window its next picture (works in a quiet run) |
| `SOFTPRESENTER_FADE_CACHE=0` (or `off`) | The way back to the old drawing, with no new build: a fade draws both slides again at every step, and a slide over a playing video is drawn again for each picture of the video, as before. By default a fade on a solid screen draws each slide once into a texture and then only blends the two (`Output::cache_sides`), and the slide over a video is drawn once into a texture that each picture of the video is laid under (`Output::steady_scene`). A file named `fade-cache-off` in the data folder does the same without setting anything. Read once as the app starts |
| `SOFTPRESENTER_OUTPUT_ROOM=<lines>` | The screens' painter has room for that many lines whatever is drawn, so a fuller slide is too heavy for them |
| `SOFTPRESENTER_LANGUAGE=<es\|ru\|uk\|en>` | Stands in for the computer's language, so a test copy starts in that language (the app's own words, and the Bible view's reading language), unless the Language setting chose one; for captures and tests |
| `SOFTPRESENTER_HIDE_NDI=1` | Behave as on a computer without NDI installed |
| `SOFTPRESENTER_SLOW_PAGES=<ms>` | Each page of a PDF being made into slides takes that many milliseconds longer, to try a quit part way through (the pictures are drawn at the next start) or to see "Page N…" |
| `SOFTPRESENTER_HIDE_PICTURE_APPS=1` | Behave as on a Mac where no app can open a presentation to draw it (the import sheet greys As Pictures) |
| `SOFTPRESENTER_VIDEO_COPY=1` | Videos the system plays reach the GPU as copies of their pixels, as on a computer where they can't go as the decoder's own memory (to compare the two ways) |
| `SOFTPRESENTER_NDI_LOSE=<n>` or `<n>@<place>` | The next n NDI pictures somebody is watching for count as lost on their way back from the GPU (with a place, only that picture of each drawing: 1 is the first) |
| `SOFTPRESENTER_NDI=<name>` | Also send over NDI under this name, this run only |
| `SOFTPRESENTER_HIDE_DECKLINK=1` | Behave as on a computer without Desktop Video (the DeckLink driver) |
| `SOFTPRESENTER_PRETEND_DECKLINK=1` | List made-up DeckLink devices ("Pretend DeckLink (1)"…) in place of real ones; what they are sent goes nowhere |
| `SOFTPRESENTER_PRETEND_DECKLINK_STALL=<s>` | Each pretend DeckLink card stops taking frames that many seconds after it starts, as a card taken away does |

There are more, each beside the code that reads it with a `// Testing aid:` comment. List them
all with:

    git grep -ohE 'SOFTPRESENTER_[A-Z_]+' crates | sort -u

A new switch gets the same `// Testing aid:` comment, and a `Testing aid:` line in its commit
message.

## Rules

- **`unsafe` code** is allowed only in `sp-video`'s two macOS modules (`mac` for videos and
  pictures, with its `gpu` part that hands a decoded picture's memory to wgpu through wgpu's
  Metal interface and its `cover` part that puts a screen's window on its display and listens for
  the displays changing, `web_mac` for web pages), `sp-ndi`'s `runtime` module, `sp-winapi`'s `windows` module (a few Windows calls: joining
  the console a command was typed in, asking a monitor for its work area, and asking Windows the
  names the monitors give themselves) and `sp-decklink`'s `ffi`
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
  carrying it, with `unsafe` code of its own, on 2026-11-21. `sp-decklink`'s `ffi` module calls
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
  the song importer reads SoftProjector's song databases and songbooks, whose layout was
  learnt from SoftProjector's GPL-3.0 source (table and column names, how a song's text is
  split); none of its code is used, and SoftProjector may be named where the importer says
  what it reads.
- **Content.** No song lyrics or Bible text that isn't public domain, in tests or anywhere else.
  Tests use text written for them.
- **Privacy.** No secrets, tokens, local paths (`/Users/…`), links to private sessions or
  documents, or personal details of real people.
- **Third-party code.** Before linking anything new, read its licence and record it in
  `docs/licences/`, and in `NOTICE.md` if it ships with the app.
- **Tests that need a GPU** go in `crates/sp-render/tests/golden.rs`, which the automatic checks
  leave out, or are marked `#[ignore]` with the reason.
- **The docs follow the code.** When what works changes, update
  [docs/features.md](docs/features.md) in the same commit (and the README's one-line overview
  if a whole area changes). When a planned feature is built, take it off
  [docs/roadmap.md](docs/roadmap.md).
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
- **A build that leaves this computer** (sent to a tester, released, copied to another machine)
  carries no one's home folder: `scripts/macos-app.sh` shortens it to `~` with
  `--remap-path-prefix`; for any other build to share (Windows, Linux), set
  `RUSTFLAGS="--remap-path-prefix=$HOME=~"` yourself. Check with
  `strings <program> | grep -c "$HOME"`, which must be 0.
