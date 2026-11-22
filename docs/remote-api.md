# Remote Control: the API

Remote Control lets any computer, tablet or phone on the church's network run the show from a
web page the app serves. This page is for whoever writes that page (the design work's
prototype, wired to it) and for anyone checking how it is kept safe. What is built is marked
**step 1**; what is designed but not built yet is marked **step 2** at the end, so a page
written now won't need rewriting.

In short: sign in once with the password; open **one WebSocket**; say `hello`; you are sent the
whole state as a **snapshot**, then each **change** with a **version** number; you send the
**same commands as `sp`** ([cli.md](cli.md)) with a request id and get the **same replies**;
slide pictures come from plain URLs; the live preview comes over the WebSocket as JPEGs.

- [Connecting](#connecting) · [signing in](#signing-in-and-remembered-devices) ·
  [the rules every request goes through](#the-rules-every-request-goes-through) ·
  [endpoints](#endpoints)
- [The WebSocket](#the-websocket): [hello](#hello), [snapshot](#snapshot),
  [changes and versions](#changes-and-versions), [reconnecting](#reconnecting),
  [commands and replies](#commands-and-replies), [the live preview](#the-live-preview),
  [heartbeats](#heartbeats), [close codes](#close-codes)
- [The state, part by part](#the-state-part-by-part) · [times](#times)
- [The page's own state: open item, picks, cue](#the-pages-own-state-open-item-picks-cue)
- [Slide pictures](#slide-pictures) · [roles](#roles-and-who-you-are) · [limits](#limits) ·
  [the security model](#the-security-model) · [step 2](#step-2-designed-not-built-yet)

## Connecting

Switched on in **Settings › Remote Control** (or `sp remote on`), the app listens on one port
(4316 unless changed; 1024 up) on every network the computer is on, **HTTPS only**, and
answers only devices on a local network (private and link-local addresses, and the shared
100.64/10 range some networks and VPNs use): a connection from the internet is closed at once. The pane
shows the address to open, such as `https://192.168.1.20:4316`. Nothing listens while it is
off, and switching it off closes every connection.

The certificate is made on the computer itself the first time Remote Control is switched on
(ECDSA P-256, valid for ten years, naming no address so it survives the computer's address
changing). Nobody else vouches for it, so **the browser warns the first time**. The pane shows
its SHA-256 fingerprint; the operator compares it with the one the browser shows (in Chrome:
the warning page › Advanced, or the padlock › Certificate) and goes on only if they match. Make
a New Certificate replaces it (every device then sees a new fingerprint, and a warning again).

A test copy started with `SOFTPRESENTER_QUIET=1` listens on 127.0.0.1 only.

## Signing in, and remembered devices

`GET /` without a signed-in device answers `303` to `/login`, the sign-in page. It posts:

    POST /api/login
    Origin: https://192.168.1.20:4316
    Content-Type: application/json

    {"password": "…"}

- **Right:** `200 {"ok": true, "device": "Safari on an iPad"}` and a cookie:
  `Set-Cookie: __Host-sp_device=<token>; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=31536000`.
  The token is 256 random bits from the system's random source; the computer keeps only its
  SHA-256, with the device's name (made from the browser's user agent: browser and system,
  nothing more), when it was first and last seen, and its address. The page can't read the
  cookie (HttpOnly); the browser sends it with the page's own requests only.
- **Wrong:** `401 {"ok": false, "text": "That password isn't right."}`.
- **Too many wrong:** after three wrong passwords in a row from one address, each must wait:
  1 s, 2 s, 4 s… up to a minute; ten in a row shut the address out for 15 minutes; past 30
  wrong passwords a minute from everywhere together, everyone waits out that minute. While an
  address must wait, its password isn't even checked:
  `429 {"ok": false, "text": "Too many wrong passwords. Try again in 4 s.", "wait": 4}` with
  `Retry-After: 4`. A right password clears its address.
- **No password set yet:** `503` with text saying to set one in Settings.

`POST /api/logout` (same Origin rule) forgets this device: its pages close, and the cookie is
cleared.

A device stays signed in until it is **forgotten** (Settings › Remote Control › Forget, Forget
All, or `sp remote forget`), the **password changes** (which forgets every device), or it signs
out. Its open WebSockets close at once with code **4401**: go to `/login`. At most 50 devices
are remembered at once.

## The rules every request goes through

1. **Host.** The `Host` header must be the address the connection came in on, as figures with
   the port (`192.168.1.20:4316`; IPv6 in brackets), or `localhost:PORT` on the computer
   itself. Anything else is `421`. So a web page elsewhere that gets its own name pointed at
   the computer (DNS rebinding) is refused. Open the page by its address, as the pane shows it.
2. **A signed-in device**, for everything except `/login`, `/login.js`, `/style.css` and
   `/favicon.ico`: otherwise `401` (or `303` to `/login` for `/`).
3. **Origin** must be `https://` + that Host on `POST /api/login`, `POST /api/logout` and the
   WebSocket upgrade; otherwise `403`. Sign-in also needs `Content-Type: application/json`
   (a plain form is `415`). Together with SameSite=Strict, no other site's page can use a
   signed-in browser.
4. **No file paths.** Nothing is served from a path in a request: the pages are compiled into
   the app, and pictures are found by a slide's id. Unknown paths are `404`.
5. **Headers on every answer:**
   `Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self' wss://HOST; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`,
   `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`,
   `Cross-Origin-Opener-Policy` and `Cross-Origin-Resource-Policy: same-origin`, and
   `Cache-Control: no-store` unless a picture says otherwise.

**For the page:** scripts and styles must be files served by the app (no inline `<script>`,
no `onclick=` attributes, no `eval`); pictures may be from the app or `blob:` URLs. Every
word that comes from the app (song titles, slide words, names, notices) is somebody's typing:
put it in with `textContent`, never `innerHTML`.

## Endpoints

| Method and path | Signed in | What |
|---|---|---|
| `GET /` | yes (else 303 to /login) | The page |
| `GET /login`, `/login.js`, `/style.css` | no | The sign-in page, its script, the style sheet |
| `GET /app.js` | yes | The page's script |
| `GET /favicon.ico` | no | 204 |
| `POST /api/login` | no | Sign in (above) |
| `POST /api/logout` | yes | Forget this device |
| `GET /api/ws` | yes | The WebSocket (Origin checked) |
| `GET /thumb/{slide}/{version}/{look}.jpg` | yes | A slide's picture ([below](#slide-pictures)) |

The built-in page's files are in `crates/sp-app/src/remote/page/`; the design work's page will
take their place there (same names, or more files added to the server's routes).

## The WebSocket

One socket per page: `wss://HOST/api/ws`, opened with the cookie and the page's Origin (a
browser does both by itself). Messages are JSON text, except the live preview's binary
pictures. The page sends at most 64 KB a message.

### hello

The first thing to send, within 10 seconds:

    {"type": "hello", "preview": true, "resume": {"epoch": "Hk3…", "version": 41}}

`preview` (optional) starts the live preview; `resume` (optional) is the epoch and version the
page last had. The answer is a snapshot, or (resuming) a change with exactly what was missed.

### snapshot

    {"type": "snapshot", "epoch": "Hk3…", "version": 41, "now": 1795370000123,
     "you": {"client": 3, "device": "Safari on an iPad", "role": "control"},
     "parts": {"live": {…}, "playlist": {…}, "playlists": […], "library": {…}, "bin": {…},
               "media": {…}, "audio": {…}, "timers": […], "messages": […], "props": […],
               "macros": […], "screens": {…}, "notices": {…}, "versions": {…}}}

`now` is the server's clock in milliseconds since 1970 when it was sent ([times](#times)).
`you` is who this page is ([roles](#roles-and-who-you-are)).

### Changes and versions

    {"type": "change", "version": 42, "now": 1795370000456, "parts": {"live": {…}}}

Each change carries **whole parts** (replace the part, don't merge into it), only the parts
that changed, and the version they changed at. Versions go up by one per change, counting from
1 each time Remote Control is switched on, under a new **epoch** (a random word). Changes are
coalesced: at most about twenty a second, and none while nothing changes. The state is taken
from what the app already keeps, never by looking at its window.

### Reconnecting

Reconnect with `resume` set to the epoch and the last version applied:

- same epoch, version not ahead: a `change` with every part that changed since (possibly
  `"parts": {}` when nothing did), with the current version;
- another epoch (the app restarted, or Remote Control was switched off and on) or a version
  ahead of the server's: a fresh `snapshot`.

Reconnect after a growing pause (half a second, doubling, up to eight seconds), except after the
close codes that say not to.

### Commands and replies

    {"type": "command", "id": 7, "args": ["next", "--aim", "3", "--rev", "9f1c…"]}

`args` are the words typed after `sp` ([cli.md](cli.md) lists every command, its words and its
reply), at most 64 of them; `id` is a number or a word of at most 64 characters, echoed in the
reply:

    {"type": "reply", "id": 7, "reply": {"ok": true, "text": "On screen: River Song, slide 1 of 6 (Verse 1).", "data": …}}

`reply` is byte for byte the reply `sp --json` gets: carried out on the app's main thread by the
same handler as `sp` and the window's own buttons. `ok: false` is a **refusal**; `text` is the
app's own words for why (show it as the cue chip), such as "That is the last slide." or "River
Song has 6 slides; there is no slide 9.". Every command gets exactly one reply, within 15
seconds ("SoftPresenter did not answer in time." otherwise).

Refused before they reach the app, with words saying so (read as the app would read them:
`song --library NAME import FILE` is an import):

- commands that reach the **computer's files or folders**: `song import`, `song export`,
  `media add`, `audio add`, `media-playlist follow FOLDER`, `bible install FILE` (a Bible
  `bibles` offers, by name, is fine), and `song new`, `song text`, `prop new` with more than one
  word after them (send the title as **one** word, spaces and all: `["song", "new", "Morning
  Hymn"]`, never read as a file);
- **keys and sign-ins**: `voxlive connect|disconnect`, `canva connect|import|disconnect`;
- the **menu bar**: `menu ITEM` (`menu` alone lists it);
- a web page on a slide other than `https://` (`slide web`);
- `status --wait` (the page is sent changes anyway);
- **Remote Control itself**: `remote …`;
- anything the server doesn't know (a command added to the app later is closed to pages until
  it is looked at).

And to keep the app responsive: at most four commands waiting for the app at once, twenty a
second (forty in a burst) from one page; past that, a refusal saying to slow down.

A message the server can't read gets `{"type": "error", "text": "…"}`.

### The live preview

With `"preview": true` in the hello (or `{"type": "preview", "on": true}` later; `false` stops
it), the page is sent what the audience sees as **binary messages**: one byte saying what it is,
then the picture. Byte `1` is the live preview: a 640 × 360 JPEG. It is the picture the
operator's own Program preview shows, read back on the server's own thread, so it costs the
screens nothing; sent when it changes, at most ten a second (a playing video, a fade), only the
newest (a slow page skips pictures rather than falling behind), and nothing while nobody
watches.

    socket.binaryType = "blob";
    // on a Blob message: first byte 1 → URL.createObjectURL(blob.slice(1, blob.size, "image/jpeg"))

### Heartbeats

Every 15 seconds the server sends a WebSocket ping and `{"type": "beat", "now": …}` (use `now`
to keep the clock offset fresh, and to notice a dead connection: no beat for 30 seconds, close
and reconnect). The server closes a page it has heard nothing from (no message, no pong; a
browser answers pings by itself) for 45 seconds.

### Close codes

| Code | Why | The page should |
|---|---|---|
| 4401 | Signed out: the device was forgotten, or the password changed | go to `/login` |
| 4000 | Disconnected at the computer (Settings › Remote Control › Disconnect) | stay disconnected until the person asks to connect again |
| 1001 | Remote Control was switched off | reconnect with the usual pauses |
| 4008 | Too slow to take what it was sent, silent too long, or no hello in time | reconnect and resume |
| 1013 | Too many pages connected | try again later |

## The state, part by part

Each part is what the app already keeps, mostly the same JSON `sp` gives for the matching
command. Numbers are counted from 1, as `sp` counts them.

**`live`** — what is on screen, and the switches beside it: `sp status --json`'s `data` less what
ticks (`timers`, `audio`, `token`), plus:

    {"live": {"song": "River Song", "slide": 2, "slides": 6, "words": "…", "kind": "song|passage|voxlive",
              "id": "<song id>", "number": 4, "playlist": "Sunday", "item": 3},
     "next_slide": 3, "media": "Waves.mp4", "message": "Welcome", "props": ["Logo"],
     "audience_on": true, "stage_on": true, "selected_song": "…", "announcements": "…",
     "media_playlist": "…", "playlist": {"name": "Sunday", "item": 3, "item_last_opened": "…", "listed": true},
     "continuous": false, "identify": false, "main_theme": "Built-in",
     "version": {"name": "Offering", "kept": false, "waiting": false},
     "build": 0, "slide_id": "<uuid>", "thumb": "/thumb/<uuid>/usual/<look>.jpg",
     "live_item": 3, "advance_at": 1795370004000,
     "layers": {"slides": true, "media": true, "props": true, "messages": true, "audio": false, "announcements": false},
     "test_pattern": "ColorBars", "stage_message": {"text": "…", "on": false}, "captions": false,
     "version_id": "<uuid>"}

`live.item`/`live.playlist` say which item of the playlist in front it was opened from (as the
window would), `live_item` which item went on screen (also when a page started it with
`--aim`). `build` is the click build step; `advance_at` when the slide goes on by itself (null
if it doesn't); `captions` whether VoxLive's captions are on screen; `version_id` the theme
version the screens draw in. A Bible passage on screen is `kind: "passage"`, its title in
`song`.

**`playlist`** — the playlist in front: `{"name", "number", "rev", "live_item", "items": […]}`,
the items as `sp playlist --json` lists them. Each has `item` (its place in the playlist: the
number `--aim` takes), `title`, `text` (as `sp` shows it), `kind`
(`song|passage|header|media|media_playlist|placeholder`) and `missing`; a song adds its `id` and
`number` (its place in the library, not the playlist), a picture or video its `media_type` and
`number` (in the media bin). For example:

    {"item": 1, "kind": "header", "title": "Worship", "text": "— Worship —", "missing": false}
    {"item": 2, "kind": "song", "id": "58a9…", "number": 2, "title": "Test One", "text": "Test One", "missing": false}
    {"item": 4, "kind": "media", "media_type": "picture", "number": 2, "title": "pic001.png", "text": "pic001.png (picture)", "missing": false}

`rev` changes whenever the items do; send it with `--aim` ([below](#the-pages-own-state-open-item-picks-cue)).

**`playlists`** — `[{"number", "name", "items", "rev", "front"}]`. A playlist's items come on
demand: `["playlist", "NAME"]`.

**`library`** — `{"songs": 214, "rev": "…", "libraries": […]}`. The songs come on demand:
`["songs"]` (all, with ids; fetch again when `rev` changes), `["library", "NAME"]`,
`["search", "WORDS"]`. A song's slides: `["slides", "<song id>"]`; a playlist item's:
`["slides", "item", "3"]` (each slide with `id` and `thumb`).

**`bin`** — the media bin: `{"items": [{"number", "id", "name", "kind": "picture|video", "fit",
"loop", "poster": "/thumb/<id>/usual/<look>.jpg"}], "playlists": [{"id", "name"}]}`.

**`media`** — what plays on the media layer:

    {"showing": {"id", "name"},
     "video": {"name", "playing", "loop", "mute", "duration_ms", "started_at", "position_ms"},
     "media_playlist": {"id", "name", "step", "since"},
     "announcements": {"words", "step", "item", "since", "on_slide_screens"}}

**`audio`** — `{"track", "track_id", "playing", "paused", "length_ms", "started_at",
"position_ms", "volume" (0–100), "mute", "shuffle", "through", "crossfade" (seconds)}`.

**`timers`** — `[{"id", "name", "kind", "words", "overrun", "running", "counted_ms", "started_at"}]`.
`kind` is one of `{"type": "countdown", "seconds": 300}`, `{"type": "countdown_to", "hour": 10,
"minute": 30, "second": 0}` (a time of day, the computer's), `{"type": "elapsed", "start": 0,
"end": 600}` (`end` null for none); `words` is how `sp timers` says it ("counts down from 5:00").

**`messages`**, **`props`**, **`macros`** — as `sp messages|props|macros --json` give them
(which is on screen included).

**`screens`** — `{"screens": [ … as sp screens --json, each with "tone": "good|trouble" ],
"audience_on", "stage_on", "identify"}`. `doing` is the screen's status in words.

**`notices`** — `{"notice": "…", "toast": "…", "questions": [{"kind", "text", "actions":
[{"label", "args"}]}]}`. A question's actions are commands: send `args` as a command to answer
it (for a song changed outside the app: `song reload SONG --now` or `song keep SONG`).

**`versions`** — `{"versions": [{"id", "name"}], "on": {"id", "keep", "waiting"}}`.

## Times

Nothing that ticks is sent. Times are **milliseconds since 1970 on the server's clock**, and
every snapshot, change and beat carries the server's `now`. Keep
`offset = server_now - Date.now()` (fresh at each message) and compute:

- a **running timer**: `counted = counted_ms + (Date.now() + offset - started_at)`; then a
  countdown shows `seconds*1000 - counted` (below 0 only with `overrun`), a count up
  `start*1000 + counted` (stopping at `end`), a countdown to a time of day the time left to it;
  a stopped timer has `started_at: null` and shows `counted_ms`;
- a **playing video or track**: position `= Date.now() + offset - started_at`; paused, it is
  `position_ms` (and `started_at` is null);
- the slide going on by itself at `advance_at`; a media playlist's step began at `since`.

A seek or a pause sends a new `started_at` or `position_ms`; while it plays on, nothing is
sent.

## The page's own state: open item, picks, cue

The open item, picks, the cue (the slide picked for Enter), find, the chooser and scrolling
are **the page's own**, per operator; the app holds only what is on screen. So commands that
act "on the open item" are told which item:

- `["next", "--aim", "3", "--rev", REV]`, `["previous", "--aim", "3", "--rev", REV]`: Next or
  Previous as if item 3 of the playlist in front were open and nothing picked (Continuous
  followed as the window follows it). An item it starts goes on screen (it becomes
  `live_item`) without becoming the window's open item.
- `["go", "--aim", "3", "2", "--rev", REV]`: slide 2 of item 3 on screen (its first slide
  without the number; a picture, video or media playlist with `1` or nothing). **Enter** is a
  `go` to the page's cue.
- `["go", "<song id>", "2"]`: a song's slide from the library.

`first`, `last`, `group NAME` and `hotkey KEY` still act on the song the window shows; from a
page use `go --aim ITEM SLIDE` (or `go SONG SLIDE`), and `group NAME in SONG`.

The window's open item, picks and chosen song stay as they are. `--rev` is the playlist's `rev`
from the `playlist` part: if the playlist changed since the page saw it, the command is refused
("Sunday has changed since it was last looked at; look again and try once more.") rather than
the wrong item shown. Items are named by their number in the playlist in front; to run from
another playlist, put it in front first (`["playlist", "open", "NAME"]`, which the window shows
too).

## Slide pictures

    GET /thumb/{slide id}/{version id | usual}/{look}.jpg

Every slide in `slides` replies and the `live` part comes with its `thumb` address; media bin
items come with a `poster`. The picture (480 × 270 JPEG) is drawn when first asked for, by the
same painter as the operator window's thumbnails (the slide's own theme, its media behind it,
in a theme version when the address names one: an item set to a version draws its tiles in
it), and kept. `look` is a hash of everything that makes the picture what it is (the slide's
objects, its theme, the version, the media behind it): when any of it changes, the slide's
`thumb` address changes, so a page fetches the new picture and keeps the old address's for good.

- An address whose look is current: `Cache-Control: private, max-age=31536000, immutable`, an
  `ETag`, and `304` to `If-None-Match`.
- An address with an older look still gets the picture as it is now, with `no-cache`.
- `404` for a slide that isn't there (a Bible passage only listed, not opened, has no pictures
  yet; see step 2), and for a media bin item whose picture hasn't been read yet (the `bin` part
  gives it a new `poster` address once it has); `503` when the app is busy (try again).

## Roles, and who you are

The snapshot's `you` says this page's client number, its device's name and its **role**. The
only role so far is `control`: everything above. The server refuses what a role doesn't allow
before the app sees it (`crates/sp-app/src/remote/policy.rs`), so a view-only role (step 2) is
one more arm there. Settings › Remote Control lists the pages connected (device, address,
since), with Disconnect.

## Limits

| What | Limit |
|---|---|
| Connections | 64 at once; 16 from one address |
| TLS handshake | 10 s |
| Request headers | 10 s, 16 KB, 64 headers; HTTP/1.1 only |
| A connection (not a WebSocket) | 2 minutes |
| Sign-in body | 4 KB |
| Pages (WebSockets) | 16 at once; 4 from one device |
| A message from a page | 64 KB (more closes the socket) |
| Commands (and hellos, and preview switches) | 4 waiting, 20 a second (40 in a burst) per page; 15 s for an answer |
| Hello | within 10 s |
| Silence | 45 s, then closed |
| Writing to a slow page | 10 s, then closed (it reconnects and catches up) |
| Slide pictures | 2 drawn at once (more wait up to 10 s); 24 MB kept |
| Live preview | 10 a second at most, newest only |
| Devices remembered | 50 |

## The security model

Who is who: the operator at the computer (trusted with everything); a person on the network
with the password (trusted to run the show); anyone else on the network; and web pages in any
browser on the network (trusted with nothing).

| Threat | How it is met |
|---|---|
| Someone on the network reads or changes the traffic, or steals a session | HTTPS only (rustls, TLS 1.2 and 1.3), with a certificate made on the computer; the operator checks its fingerprint once per device. The cookie is `Secure` |
| Guessing the password | argon2id (19 MiB, two passes), checked one at a time off the server's thread; per-address waits that grow, a 15-minute shutout, and a cap for everyone together; an address that must wait isn't checked at all. Settings asks for at least 8 characters |
| The password or tokens leaking from the computer | The password is kept only as an argon2id hash with a random salt, tokens only as SHA-256, in `remote/settings.json` in a folder only the user may enter (0700, files 0600); neither is ever logged (the log has sign-ins by device and address, wrong passwords by address, and the commands pages sent). The data folder may be in Documents, which a Mac can sync: the hashes and the certificate's key go with it (see residual risks) |
| A stolen or old device | Forget (or a new password) revokes at once, closing its open pages |
| Another site's page using a signed-in browser (cross-site requests, cross-site WebSocket) | SameSite=Strict, HttpOnly cookie; `Origin` checked on the WebSocket and every POST; sign-in needs JSON |
| DNS rebinding | `Host` must be the address the connection came in on |
| Someone on the internet, if the computer is reachable from it (a forwarded port) | Only local-network addresses are answered |
| Reading or writing the computer's files through the page | No path from a request is ever served; commands that name files or folders are refused for pages by their words, and again where each would open a file, whatever words got there (a title or name is then all words; the disk is never looked at for a page); pictures are found by id |
| Reaching the computer's own web services | `slide web` with anything but https is refused for pages |
| Keys, sign-ins, the menu bar (Quit, Install Command Line Tool), Remote Control's own settings | Closed to pages |
| Script injection into the page | Strict CSP (no inline script, no eval), `textContent` only, no framing |
| Flooding the app or the server | The limits above; commands are carried out on the main thread one at a time as `sp`'s are, never blocking the screens' thread; pictures drawn a few per turn of the event loop; the preview read back on the server's own thread from the window's own textures |
| A page left open forever | Heartbeats, and closing after 45 s of silence |

Residual risks, known and accepted for step 1: someone on the network can hold connection
slots open (up to the limits above) and keep a new device from signing in for a while
(remembered devices already connected carry on); the cap on wrong passwords from everywhere
together means a flood of wrong ones holds up everyone's sign-in for that minute; the first
visit trusts the certificate on the operator's word (the fingerprint check is theirs to do);
anyone with the password runs the show
(as anyone at the computer can), including deleting songs (to the Trash) and changing screens;
the data folder (and so the password's hash, the device hashes and the certificate's key) may
be synced by the Mac with the rest of Documents; the `Host` check allows the computer's
addresses as figures and `localhost` only, so a name such as `church-mac.local` is refused
(open the page by its address).

## Step 2 (designed, not built yet)

Planned so the page can be written against them now. Each keeps the rules above (one socket,
`sp`'s commands, whole parts, versions, refusals in words).

**4b. More pictures, and a choice of preview.** The same `/thumb/{id}/…` route for a theme's
templates (`id` = template id), props, stage layouts (as the stage screen draws them) and every
media bin picture. A page may choose its live preview: `{"type": "preview", "on": true, "of":
"audience" | "screen:<id>" | "stage:<layout id>"}`; byte `1` stays the audience picture, byte `2`
is a chosen screen, byte `3` a stage layout.

**5. The Bible, on the server.** Commands, each with a JSON reply:
`["bible", "parse", "WORDS", "--language", "ru"]` (a reference in the language typed, with the
book named in it); `["bible", "chapter", "John 3", "--in", "KJV,RST"]` (a chapter's text for the
chosen translations, each verse under each translation's own numbering); `["bible", "books",
"--language", "uk"]` (book names by language); passage slides as pictures (a passage looked up
is kept in a slot so its slides have ids and `thumb`s); `["bible", "search", "WORDS"]` exists
today.

**6. Find.** `["find", "WORDS"]`: songs, slides (the line found) and verses, ranked, each with a
kind tag (`song`, `slide`, `verse`) and what to send to open it, as the window's find box ranks
them.

**7. Uploads.** `POST /api/upload?kind=media|song|bundle|bible|audio` with the file as the body
(Origin checked; size capped per kind; streamed to a private staging folder, never to a path
from the request), answered with a job number; progress and the result come as notices
(`notices.jobs: [{"job", "kind", "name", "done", "of", "result"}]`). Each file goes through the
same importer, with the same limits, as a file dropped on the window (Bible files as the Bible
importer reads untrusted files).

**8. Undo for everything the page changes.** Every edit a page makes goes through the app's own
Undo, app-wide (`["undo"]`, `["redo"]`, with what was undone in the reply): playlist items,
songs and slides, renames, screens, settings, macro steps, and lastly the Edit page's objects.
The `notices` part gains `undo: {"can": true, "what": "…"}, redo: {…}`.

**9. Reliability, further.** A command may carry `"expect": VERSION`: refused if the state moved
on since (for edits more than for Next).

**10. Roles.** A `view` role (state, pictures and the preview; only commands that read), signed
in with a second password; later, roles per device (`devices[].role`), chosen in Settings.

**11. Settings the page draws from.** A `settings` part: key mappings (the Mac's
no-function-key rule included), groups with their colours and keys, transitions, Continuous,
the colours of each kind, the operator's reading language, and the hardware lists (displays,
NDI receivers, DeckLink devices with the formats each can key).

## Wiring the prototype: a checklist

1. Serve its files from `crates/sp-app/src/remote/page/` (scripts and styles as files).
2. On load: open the socket; send `hello` with `preview` and the `resume` from the last
   snapshot or change (keep epoch and version in memory, or in `sessionStorage`).
3. Replace parts whole on `snapshot` and `change`; draw from them.
4. Keep the open item, picks and cue in the page; send `--aim ITEM --rev REV` with Next and
   Previous, and `go --aim ITEM SLIDE` for Enter and clicks.
5. Send every action as a command with an id; show `reply.text` (always the app's words) when
   `ok` is false.
6. Compute times from `started_at` and `now` (never poll for them).
7. Load songs and slides on demand (`songs`, `slides item N`), pictures from `thumb` addresses.
8. Handle close codes 4401 (sign in again) and 4000 (stay disconnected).
