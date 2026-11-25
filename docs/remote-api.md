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
- [The state, part by part](#the-state-part-by-part) · [built-in names](#built-in-names) ·
  [the app's language](#the-apps-language) · [times](#times)
- [The page's own state: open item, picks, cue](#the-pages-own-state-open-item-picks-cue)
- [Slide pictures](#slide-pictures) · [roles](#roles-and-who-you-are) · [limits](#limits) ·
  [the security model](#the-security-model) · [step 2](#step-2-designed-not-built-yet)

## Connecting

Switched on in **Settings › Remote Control** (or `sp remote on`), the app listens on one port
(4316 unless changed; 1024 up) on every network the computer is on, **HTTPS only**, and
answers only devices on a local network (private and link-local addresses, and the shared
100.64/10 range some networks and VPNs use): a connection from the internet is closed at once. The pane
shows the address to open, by the computer's own name (`https://church-mac.local:4316`), and
under it the address as figures (`https://192.168.1.20:4316`), which works too but may warn
again whenever the network gives the computer another address. Nothing listens while it is
off, and switching it off closes every connection.

The certificate is made on the computer itself the first time Remote Control is switched on:
ECDSA P-256, for serving (the server-authentication purpose), for 396 days, naming only
`localhost`, 127.0.0.1 and the computer's own name on its network (`church-mac.local`, the
Bonjour name), **never its address**: a new lease, Wi-Fi instead of Ethernet or a VPN doesn't
touch it, and neither does being offline when it is looked at. (Opened by its address as
figures, the connection works, as the `Host` check accepts the address it came in on, but the
browser warns that the certificate is for another name.) It is made again a month before it
runs out and when the computer's own name changes (looked at when Remote Control is switched on,
and every ten minutes while no page is connected), and **with the same private key each
time**. Nobody else vouches for it, so **the browser warns the first time**.

What the operator verifies is the **key's fingerprint**: the SHA-256 of the key's public half
(its DER `SubjectPublicKeyInfo`), shown in Settings › Remote Control and by `sp remote`, 32
pairs. It does not change when the certificate is made again, so a renewal asks nothing of
anyone. Only Make a New Certificate makes a new key (every device then sees a new fingerprint
and a warning again, and the operator is told).

### How a new device trusts it

1. Open the address Settings shows (`https://church-mac.local:4316`).
2. The browser warns that the certificate isn't trusted. Browsers show the certificate's own
   SHA-256 fingerprint, which changes at each renewal, not the key's; to check the key's, from
   a computer on the network (or the browser's own certificate export):

       openssl s_client -connect church-mac.local:4316 -servername church-mac.local </dev/null 2>/dev/null \
         | openssl x509 -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -hex

   and compare with the one in Settings › Remote Control. On the computer itself, the same
   command with `localhost` reads the same key. A browser alone doesn't show the key's
   fingerprint; without a terminal the first visit rests on being on the computer's own network.
3. Go on (Chrome: Proceed; Safari: visit this website). Sign in. A certificate made again later
   with the same key (a month before it runs out) warns once more in some browsers; the key
   fingerprint is still the same, and nothing needs checking again.
4. A new key (Settings says so) means checking once more on each device.

**Needs checking on an iPad or iPhone (step 1 hasn't been tried there):** Safari may not let the
page's WebSocket connect to a certificate it only let through on a warning. The certificate is
made the way Apple asks of server certificates (at most 397 days, the server purpose, the
names it is opened at) so that, if need be, it can be installed and trusted on the
device instead (Settings › General › About › Certificate Trust Settings); whether that is
needed, and works for a certificate that signs itself, is still to be tried.

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

Each `text` is in the app's language ([below](#the-apps-language)): in Spanish, `"Esa contraseña
no es correcta."` and `"Demasiadas contraseñas incorrectas. Inténtalo de nuevo dentro de 4 s."`.
Show it as it comes; `wait` is the seconds as a number, for a page that counts down itself.

`POST /api/logout` (same Origin rule) forgets this device: its pages close, and the cookie is
cleared.

A device stays signed in until it is **forgotten** (Settings › Remote Control › Forget, Forget
All, or `sp remote forget`), the **password changes** (which forgets every device), or it signs
out. Its open WebSockets close at once with code **4401**: go to `/login`. At most 50 devices
are remembered at once.

## The rules every request goes through

1. **Host.** The `Host` header must be the address the connection came in on, as figures with
   the port (`192.168.1.20:4316`; IPv6 in brackets), the computer's own name on the network
   with the port (`church-mac.local:4316`), or `localhost:PORT` on the computer itself. (The
   certificate names the computer's own name and `localhost`, not the figures, so a browser
   warns about the figures once more each time the address changes.)
   Anything else is `421`. So a web page elsewhere that gets its own name pointed at the
   computer (DNS rebinding) is refused.
2. **A signed-in device**, for everything except `/login`, `/login.js`, `/style.css`, the `/next/` redirects and
   `/favicon.ico`: otherwise `401` (or `303` to `/login` for `/`).
3. **Origin** must be `https://` + that Host on `POST /api/login`, `POST /api/logout` and the
   WebSocket upgrade; otherwise `403`. Sign-in also needs `Content-Type: application/json`
   (a plain form is `415`). Together with SameSite=Strict, no other site's page can use a
   signed-in browser.
4. **No file paths.** Nothing is served from a path in a request: the pages are compiled into
   the app, and pictures are found by a slide's id. Unknown paths are `404`.
5. **Headers on every answer:**
   `Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; font-src 'self'; connect-src 'self' wss://HOST; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'`
   (so a page can't make a string into script or markup, even by a slip),
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
| `GET /fonts/SoftPresenterSans-{Regular,Bold,Italic,BoldItalic}.woff2`, `/fonts/OFL.txt` | yes (else 401) | The slide face, SoftPresenter Sans, and its licence; these names only |
| `GET /next/`, `/next/app.js`, `/next/style.css`, `/next/fonts/…` (the same names as above) | no | `307` to the same file at `/` (the page used to be tried out under `/next/`; bookmarks still work). Any other name under `/next/` is `404`; the new address is never made from the request's text |
| `GET /favicon.ico` | no | 204 |
| `POST /api/login` | no | Sign in (above) |
| `POST /api/logout` | yes | Forget this device |
| `GET /api/ws` | yes | The WebSocket (Origin checked) |
| `GET /thumb/{slide}/{version}/{look}.jpg` | yes | A slide's picture ([below](#slide-pictures)) |

The page's files are in `crates/sp-app/src/remote/page/` (`index.html`, `app.js`, `style.css`,
`fonts/`, and the sign-in page `login.html` with `login.js`), compiled in and served from a fixed
table of exact names (`page_file` in `server.rs`); nothing is looked up by a path from a
request. Every file carries an `ETag` from its bytes and `Cache-Control: no-cache`, so a browser
asks once whether it changed (304), and fetches the fonts once. The page names its script and
style by relative address, and its fonts from the style sheet by `fonts/…`. The sign-in
page's script posts to `/api/login` as above (the Sign In button stays disabled until the script
runs), and its form is `method="post"`, so a Return pressed early can't send the password in the
address. The sign-in page and the style sheet are open to anyone; the page, its script and the
fonts are for a signed-in device only.

**The language, before anything else comes.** The app writes the language its words are in
(`en`, `es`, `ru` or `uk`, as [`app.language`](#the-apps-language)) into four files as it sends
them: `<html lang="en"` becomes `<html lang="es"` in `index.html` and `login.html`, and a line
`window.SP_LANGUAGE = "es";` goes into `app.js` right after `window.SP_REMOTE = true;` and into
`login.js` right after its `"use strict";` (so the sign-in page can speak the language before
anyone signs in, and the page before its first snapshot). A bundle must keep those marks: a test
fails if one goes. Nothing else in the files changes, and each language has its own `ETag`, so a
browser that has the English page fetches the Spanish one afresh. After the first snapshot,
follow `app.language`, which changes while the page is open.

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
     "parts": {"app": {…}, "live": {…}, "playlist": {…}, "playlists": […], "library": {…}, "bin": {…},
               "media": {…}, "audio": {…}, "timers": […], "messages": […], "props": […],
               "macros": […], "screens": {…}, "notices": […], "versions": {…}}}

`now` is the server's clock in milliseconds since 1970 when it was sent ([times](#times)).
`you` is who this page is ([roles](#roles-and-who-you-are)).

### Changes and versions

    {"type": "change", "version": 42, "now": 1795370000456, "parts": {"live": {…}}}

Each change carries **whole parts** (replace the part, don't merge into it), only the parts
that changed, and the version they changed at. Versions go up by one per change, counting from
1 each time Remote Control is switched on, under a new **epoch** (a random word). Changes are
coalesced: at most about twenty a second, and none while nothing changes. The state is taken
from what the app already keeps, never by looking at its window.

**When the app's language changes** (Settings › General › Language, the setup's first step,
`sp settings set language=es` at the computer, or the computer's language answering while the
setting follows it; a page can't change it: `settings set language=…` from a page is refused),
one change carries `app` with the new language **and every part whose words changed with
it**: each part with a [built-in name](#built-in-names) (`live`, `playlist`, `playlists` today)
and the `playlist` part's item `text`s, all under one version. Apply the whole change before
drawing, and switch the page's own words by `app.language` at the same time, so the page never
shows two languages at once. A part whose words are the same in both languages (no label in
either) isn't sent: what the page has is already right. Notices keep the words they were said in.

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

`reply` is the reply `sp --json` gets, byte for byte, but for two things: any absolute path in it
(a picture's file, a folder) is cut to its file name, so the computer's folders and the user's
name in them stay on the computer (the state is sent the same way); and its words are in the
app's language where the app has them in it, where `sp` always gets English
([below](#the-apps-language)). It is carried out on the
app's main thread by the same handler as `sp` and the window's own buttons. `ok: false` is a **refusal**; `text` is the
app's own words for why (show it as the cue chip), such as "River Song has 6 slides; there is no
slide 9.". Next or Previous at an end is not a refusal: `ok: true`, with `data` `{"at_end": true}`,
and the window shows nothing for it, so a page shouldn't either. Every command gets exactly one reply, within 15
seconds ("SoftPresenter did not answer in time." otherwise).

Refused before they reach the app, with words saying so (read as the app would read them:
`song --library NAME import FILE` is an import):

- commands that reach the **computer's files or folders**: `song import`, `song export`,
  `media add`, `audio add`, `media-playlist follow FOLDER`, `bible install FILE` (a Bible
  `bibles` offers, by name, is fine), and `song new`, `song text`, `prop new` with more than one
  word after them (send the title as **one** word, spaces and all: `["song", "new", "Morning
  Hymn"]`, never read as a file); the app refuses them again where each would open a file;
- **keys, sign-ins and links**: `voxlive connect|disconnect`, `canva connect|import|disconnect`,
  `canva presentation`;
- **downloads and removals of the song catalog**: `catalog download|check|remove|sizes` (each
  fetches from the internet or takes songs off the computer); `catalog search`, `list`, `add`,
  `newer`, `use-newer` and `keep-mine` are open;
- the **menu bar**: `menu ITEM` (`menu` alone lists it);
- **an NDI source on a slide** can't be added from a page: there is no command for it (a
  box already on a slide is drawn in the slide's pictures as the screens draw it; `slide SONG N
  --json` lists it with `"kind": "ndi"`, `source`, `scaling`, `quality` and `sound`);
- **a web page on a slide** (`slide web … URL`): a page runs its own scripts in the computer's
  web view, on the screens, so it is put up at the computer; `slide web SONG N off` is open;
- **NDI**: `screen add … on ndi NAME`, `screen set … output=ndi:…` (NDI sends to anyone on the
  network who asks, with no password);
- `song reload` without a song (reads every song again), and `status --wait`;
- **Remote Control itself**: `remote …`;
- anything the server doesn't know: a command, **or a sub-command** of `voxlive`, `canva`,
  `settings`, `screen`, `song`, `catalog` or `media`, added to the app later is closed to pages until it is
  looked at and listed (`crates/sp-app/src/remote/policy.rs`).

And to keep the app responsive: every message a page sends counts, whatever it is: twenty a
second (forty in a burst) from one page, past which a command is refused with its id and
anything else is not answered; at most four commands from a page waiting for the app at once;
forty commands a second (eighty in a burst) from every page together; a device connects at most
twelve pages a minute (ten at once).

A message the server can't read gets `{"type": "error", "text": "…"}` (in the app's language:
`"Eso no es JSON."`).

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

**`app`** — the app itself: the language its words are in, and how that language writes numbers
and quotation marks (its `locales/<language>/format.json`):

    {"app": {"language": "es",
             "format": {"decimal": ",", "group": ".", "group_from": 5, "quote_open": "“", "quote_close": "”"}}}

`language` is `en`, `es`, `ru` or `uk`: the one chosen in Settings › General › Language, or, with
the setting on Automatic, the computer's (English when the computer's isn't one of these); never
`auto`. `group_from` is how many figures a whole number needs before it is grouped (Spanish writes
1234 but 13.613; English 1,234). It is sent again, in the same change as every part whose words
change with it, whenever the language changes ([above](#changes-and-versions)).

**`live`** — what is on screen, and the switches beside it: `sp status --json`'s `data` less what
ticks (`timers`, `audio`, `token`), plus:

    {"live": {"song": "River Song", "slide": 2, "slides": 6, "words": "…", "kind": "song|passage|voxlive|catalog",
              "id": "<song id>", "number": 4, "playlist": "Sunday", "item": 3},
     "next_slide": 3, "media": "Waves.mp4", "message": "Welcome", "props": ["Logo"],
     "audience_on": true, "stage_on": true, "selected_song": "…", "announcements": "…",
     "media_playlist": "…", "playlist": {"name": "Sunday", "item": 3, "item_last_opened": "…", "listed": true},
     "continuous": false, "identify": false, "main_theme": "Built-in",
     "main_theme_built_in": true, "main_theme_label": "Incluido",
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
version the screens draw in. A Bible passage on screen is `kind: "passage"`, and a song of the
song catalog shown from the find box and not in the library `kind: "catalog"` (with no `id` or
`number`), each with its title in `song`. `main_theme` is the main theme's name as kept; the
built-in theme under the name the app gave it has `main_theme_built_in: true` and, when the
window calls it otherwise, `main_theme_label` ([built-in names](#built-in-names)).
`live.playlist` (the playlist in front) carries the same `name`, `built_in` and `label` as the
`playlist` part.

**`playlist`** — the playlist in front: `{"name", "number", "rev", "live_item", "items": […]}`
(and `built_in`, `label` for the untouched first playlist: [built-in names](#built-in-names)),
the items as `sp playlist --json` lists them. Each has `item` (its place in the playlist: the
number `--aim` takes), `uid` (the item's own id, [below](#the-pages-own-state-open-item-picks-cue)), `title`, `text` (as `sp` shows it), `kind`
(`song|passage|header|media|media_playlist|placeholder`) and `missing`; a song adds its `id` and
`number` (its place in the library, not the playlist), a picture or video its `media_type` and
`number` (in the media bin). For example:

    {"item": 1, "uid": "6148368d-7262-44cb-9cbf-389667f98e5b", "kind": "header", "title": "Worship", "text": "— Worship —", "missing": false}
    {"item": 2, "uid": "032fb9a3-4bc7-4f35-9cc6-23ca08c68153", "kind": "song", "id": "58a9…", "number": 2, "title": "Test One", "text": "Test One", "missing": false}
    {"item": 4, "uid": "9f75b90a-22d8-4c2a-907d-cab4589f9399", "kind": "media", "media_type": "picture", "number": 2, "title": "pic001.png", "text": "pic001.png (picture)", "missing": false}

`text` is in the app's language (`"pic001.png (imagen)"`, `"Sermón (hueco)"`, `"Bars.mp4
(falta la imagen o el vídeo)"`), here and in the reply to `["playlist", "NAME"]`, and sent again
when the language changes; `sp` has it in English. `title` is the item's own name, and `kind`
and `media_type` are words for the page to read, never translated.

`uid` belongs to the item in the playlist, not to the song or picture it shows (the same song in
two playlists is two items, and a song's `id` is its own). It stays with the item when the items
are moved about, when the playlist is saved and read again and when the app is opened again; an
item put in, or one that is taken out and put back, has a new one. An item changed in place (a
header renamed, a theme version set) keeps its uid. Two identical items (two headers of one
name) are told apart by their order only.

`rev` changes whenever the items do; send it with `--aim` ([below](#the-pages-own-state-open-item-picks-cue)).

**`playlists`** — `[{"number", "name", "items", "rev", "front"}]` (and `built_in`, `label` for the
untouched first playlist). A playlist's items come on demand: `["playlist", "NAME"]`, or
`["playlist", "2"]` by its number.

**`library`** — `{"songs": 214, "rev": "…", "libraries": […]}`. The songs come on demand:
`["songs"]` (all; fetch again when `rev` changes; each `{"number": 2, "title": "Harbour Lights", "id": "829a…", "changed_outside": false}`, where `id` is the song's stable id), `["library", "NAME"]`,
`["search", "WORDS"]`. A song's slides: `["slides", "<song id>"]` (`go` and `slide` take the id too); a playlist item's:
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

**`screens`** — `{"screens": [ … as sp screens --json (each with `"kind": "audience|stage"`), each with "tone": "good|trouble" ],
"audience_on", "stage_on", "identify"}`. `doing` is the screen's status in words.

**`notices`** — the list under the app's status button, as the app keeps it (`notices.rs`):
`[{"id", "kind": "info|trouble|ask", "text", "time" (seconds since 1970), "seen", "actions":
[{"id", "label"}]}]`, questions first, then newest first. `text` and each action's `label` are
the window's own words, in the language the app spoke when it said them (a notice said in
Spanish stays in Spanish, in the window too, after a switch to English). A page's own button mirrors it with
the same commands as `sp`: `["notices", "answer", "ID", "ACTION_ID"]` answers a question with one
of its own actions (`outside:reload`, `outside:keep`; any other is refused), `["notices",
"seen"]` marks every entry seen (as closing the list does), `["notices", "clear"]` is Clear All
(a question stays until answered). Entries about security (a new certificate key, wrong
passwords tried several times in the hour) are the page's to read but not to dismiss: from a
page, `seen` leaves them unseen and `clear` keeps them until the operator has seen them in the
app (closing the list there marks them seen).

**`versions`** — `{"versions": [{"id", "name"}], "on": {"id", "keep", "waiting"}}`.

## Built-in names

Some names are the app's, not the church's: the built-in theme ("Built-in"), and the first
playlist of a first start while nothing has changed it ("Service"). Their files keep the English,
and the window shows them in its language ("Incluido", "Culto"). Wherever such a name reaches the
page it comes with two more fields:

- `built_in: true` while it is the app's name (a playlist renamed, given an item or written to
  its file, or a theme renamed, is the church's, and has neither field);
- `label`: the name as the window shows it now, **only when that differs** from the name (so
  never in English).

The name itself stays in `name` (the English the app gave it). **Show `label` when there is one,
else `name`.** Today's places:

    "live":      {"main_theme": "Built-in", "main_theme_built_in": true, "main_theme_label": "Incluido",
                  "playlist": {"name": "Service", "built_in": true, "label": "Culto", "item": null, …}, …}
    "playlist":  {"name": "Service", "built_in": true, "label": "Culto", "number": 1, "items": [], …}
    "playlists": [{"number": 1, "name": "Service", "built_in": true, "label": "Culto", "items": 0, "front": true, …}]

`main_theme` is a string, so its pair are siblings (`main_theme_built_in`, `main_theme_label`).
The untouched first playlist has no items, so nothing on screen comes from it (`live.live.playlist`
never names it). No template's or box role's name is in the state yet; when one comes (step 2),
it gets the same pair beside its `id`. Names made in a language and saved ("Arreglo 2",
"Mensaje rápido", "Texto 3") are the church's from then on and come as they are.

**In commands, name a built-in playlist by its `number`** (`["playlist", "open", "1"]`): the
app's name for it while the window speaks Spanish is "Culto", so `["playlist", "open",
"Service"]` is refused. A number always works for a playlist.

## The app's language

The page has its own words in its own tables; the app tells it which language to use (`app`, and
`window.SP_LANGUAGE` in the page's script before the first snapshot) and sends what it says
itself in that language, as the window says it:

- the sign-in answers (`401`, `429`, `503`);
- refusals (`ok: false`) the app has words for: those the policy gives (what is done at the
  computer: "Elegir en la barra de menús se hace en el ordenador, no desde la página remota.",
  a command closed to pages), the server's own (too many commands, busy, no answer in time, a
  device signed out), a playlist changed since it was looked at, an item no longer there, a slide
  that isn't there ("River Song tiene 6 diapositivas; no hay diapositiva 9."). Other refusals that
  only the command line words (most usage lines, "No item called …") are still in English;
  show `text` whatever language it is in;
- `{"type": "error"}` texts;
- the notices and their answers' labels (as the window said them);
- a playlist item's `text`, in the `playlist` part and in `["playlist", NAME]` replies.

What stays as it is: names and words the church typed (songs, slides, playlists, headers),
words for the page to read (`kind`, `media_type`, `test_pattern`, `tone`, ids), and an
`ok: true` reply's `text`, which is as the command made it. `sp` and the log are always in
English. `["settings", "set", "language=es"]` changes the app's language from a page too.

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
- `["next", "--aim", "<uid>"]`, `["go", "--aim", "<uid>", "2"]`: the item named by its `uid` (the
  `uid` of the `playlist` part's item) in place of its number. It is found wherever it has been
  moved to, so `--rev` isn't needed (and is ignored); if the item is no longer in the playlist in
  front the command is refused: "That item is no longer in Sunday; look again and try once more."
- `["go", "<song id>", "2"]`: a song's slide from the library.

`first`, `last`, `group NAME` and `hotkey KEY` still act on the song the window shows; from a
page use `go --aim ITEM SLIDE` (or `go SONG SLIDE`), and `group NAME in SONG`.

The window's open item, picks and chosen song stay as they are. `--rev` is the playlist's `rev`
from the `playlist` part (a page that names its open item by `uid` can leave it out): if the playlist changed since the page saw it, the command is refused
("Sunday has changed since it was last looked at; look again and try once more.") rather than
the wrong item shown. Items are named by their number in the playlist in front; to run from
another playlist, put it in front first (`["playlist", "open", "NAME"]`, which the window shows
too).

## Words styled inside a box

The snapshot carries each slide's words, not its objects; a page that wants a text object's
looks asks with `["slide", SONG, N, "--json"]`, and the reply's objects carry the box's own look
and, for a text object with styled words, `spans`:

    {"kind": "text", "text": "Eph 5:25 Husbands, love your wives", "spans":
      [{"start": 0, "end": 8, "weight": 700, "italic": false}, {"start": 9, "end": 17, "fill": {"r": 192, "g": 0, "b": 0, "a": 255}}]}

**Offsets count characters** (Unicode scalar values, from 0, `end` not included), not UTF-8
bytes and not UTF-16 units: in JavaScript, `Array.from(text)` makes the matching index, and a
letter with a separate combining mark is two steps. A span sets only what differs from the
box (`font_family`, `size` in slide units, `weight`, `italic`, `underline`, `strike`, `fill`);
a word's look is the theme's, then the box's own, then the span's. Spans are sorted, apart,
inside the text and at most 512; `slides` marks a slide that has some with `styled_words`
(each text object's number and spans). `["slide", "style", SONG, N, OBJECT, "weight=bold",
"--chars", "0-8"]` styles words (see [cli.md](cli.md)).

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
- An address whose look isn't the current one (an old look, or one made up) draws nothing: the
  app compares first, and answers `307` to the address with the current look (`no-store`). So
  only the look a slide really has can make the app draw, and a stored picture is never served
  under another look.
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
| Messages from a page (any kind) | 20 a second, 40 in a burst; past that a command is refused with its id, and every other message is left unanswered |
| Commands | 4 waiting per page; 40 a second (80 in a burst) from all pages together; 15 s for an answer |
| Connecting | 12 pages a minute per device (10 at once) |
| Hello | within 10 s |
| Silence | 45 s, then closed |
| Writing to a slow page | 10 s, then closed (it reconnects and catches up) |
| Slide pictures | 2 drawn at once (more wait up to 10 s); 24 MB and 4,096 pictures kept; a version no theme has is drawn as the usual look |
| Live preview | 10 a second at most, newest only |
| Devices remembered | 50; one not seen for 180 days is let in no more, and is forgotten (with its pages closed) when Remote Control is switched on, when anyone signs in and every ten minutes |

## The security model

Who is who: the operator at the computer (trusted with everything); a person on the network
with the password (trusted to run the show); anyone else on the network; and web pages in any
browser on the network (trusted with nothing).

| Threat | How it is met |
|---|---|
| Someone on the network reads or changes the traffic, or steals a session | HTTPS only (rustls, TLS 1.2 and 1.3), with a certificate made on the computer; the operator checks its key's fingerprint once per device (the key is kept when the certificate is made again). The cookie is `Secure` |
| Guessing the password | argon2id (19 MiB, two passes), checked one at a time off the server's thread; per-address waits that grow, a 15-minute shutout, and a cap for everyone together; an address that must wait isn't checked at all. Settings asks for at least 10 characters (a password saved before that was asked for, if one signs a device in, is noted as too short: Settings and `sp remote` say so and ask for a new one); and past ten wrong passwords lately from everywhere, every sign-in waits (1 s, doubling every five more, up to 2 minutes) |
| The password or tokens leaking from the computer | The password is kept only as an argon2id hash with a random salt, tokens only as SHA-256, in `remote/settings.json` in a folder only the user may enter (0700, files 0600); neither is ever logged (the log has sign-ins by device and address, wrong passwords by address, and the commands pages sent). The data folder may be in Documents, which a Mac can sync: the hashes and the certificate's key go with it (see residual risks) |
| A stolen or old device | Forget (or a new password) revokes at once, closing its open pages |
| Another site's page using a signed-in browser (cross-site requests, cross-site WebSocket) | SameSite=Strict, HttpOnly cookie; `Origin` checked on the WebSocket and every POST; sign-in needs JSON |
| DNS rebinding | `Host` must be the address the connection came in on |
| Someone on the internet, if the computer is reachable from it (a forwarded port) | Only local-network addresses are answered |
| Reading or writing the computer's files through the page | No path from a request is ever served; commands that name files or folders are refused for pages by their words, and again where each would open a file, whatever words got there (a title or name is then all words; the disk is never looked at for a page); pictures are found by id |
| Reaching the computer's own web services | Pages can't put web pages on slides; and no web page on a slide (a page's, or a song pack's) can redirect, post or frame its way to plain http on this computer, only one opened at such an address itself (a test's) |
| Sending the screens to anyone on the network | Pages can't turn on NDI output |
| The computer's folders and user name | Absolute paths in replies and state are cut to file names; VoxLive's key is shown to pages only as `vxk_…`; Canva's switch-off steps (which name home folder places) aren't sent |
| Somebody guessing at the password | Wrong passwords in the last hour are shown in Settings, and from three on in the status line, in red |
| The log | Readable by the user only (it names devices and addresses) |
| Keys, sign-ins, the menu bar (Quit, Install Command Line Tool), Remote Control's own settings | Closed to pages |
| Script injection into the page | Strict CSP (no inline script, no eval), `textContent` only, no framing |
| Flooding the app or the server | The limits above; commands are carried out on the main thread one at a time as `sp`'s are, never blocking the screens' thread; pictures drawn a few per turn of the event loop; the preview read back on the server's own thread from the window's own textures |
| A page left open forever | Heartbeats, and closing after 45 s of silence |

**Needs checking at each church (for the owner):** "only the local network is answered" looks at
the address a connection comes from. A router that forwards a port to the computer with its own
address in place of the sender's (source NAT, "hairpin" forwarding), a reverse proxy in front of
it, or a VPN that hands out addresses in 100.64/10 (Tailscale does) makes devices outside look
local, and then only the password stands in their way. **Don't forward Remote Control's port on
the church's router**; reach it from outside, if ever, through a VPN the church controls.

Residual risks, known and accepted for step 1: someone on the network can hold connection
slots open (up to the limits above) and keep a new device from signing in for a while
(remembered devices already connected carry on); the cap on wrong passwords from everywhere
together means a flood of wrong ones holds up everyone's sign-in for that minute; the first
visit trusts the certificate on the operator's word (the key fingerprint check is theirs to do);
anyone with the password runs the show
(as anyone at the computer can), including deleting songs (to the Trash) and changing screens;
the data folder (and so the password's hash, the device hashes and the certificate's key) may
be synced by the Mac with the rest of Documents.

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
(entries in the `notices` list, with the job's progress). Each file goes through the
same importer, with the same limits, as a file dropped on the window (Bible files as the Bible
importer reads untrusted files).

**8. Undo for everything the page changes.** Every edit a page makes goes through the app's own
Undo, app-wide (`["undo"]`, `["redo"]`, with what was undone in the reply): playlist items,
songs and slides, renames, screens, settings, macro steps, and lastly the Edit page's objects.
An `undo` part gives `{"can": true, "what": "…"}` and the same for redo.

**9. Reliability, further.** A command may carry `"expect": VERSION`: refused if the state moved
on since (for edits more than for Next).

**10. Roles.** A `view` role (state, pictures and the preview; only commands that read), signed
in with a second password; later, roles per device (`devices[].role`), chosen in Settings.

**11. Settings the page draws from.** A `settings` part: key mappings (the Mac's
no-function-key rule included), groups with their colours and keys, transitions, Continuous,
the colours of each kind, the operator's reading language, and the hardware lists (displays,
NDI receivers, DeckLink devices with the formats each can key).

## Wiring the prototype: a checklist

1. Its files are served from `crates/sp-app/src/remote/page/` (scripts and styles as files).
2. On load: open the socket; send `hello` with `preview` and the `resume` from the last
   snapshot or change (keep epoch and version in memory, or in `sessionStorage`).
3. Replace parts whole on `snapshot` and `change`; draw from them. Speak `app.language`
   (`window.SP_LANGUAGE` until the first snapshot), switching with the change that brings a new
   one, and show `label || name` for a name with `built_in`.
4. Keep the open item, picks and cue in the page; send `--aim ITEM --rev REV` with Next and
   Previous, and `go --aim ITEM SLIDE` for Enter and clicks.
5. Send every action as a command with an id; show `reply.text` (always the app's words) when
   `ok` is false.
6. Compute times from `started_at` and `now` (never poll for them).
7. Load songs and slides on demand (`songs`, `slides item N`), pictures from `thumb` addresses.
8. Handle close codes 4401 (sign in again) and 4000 (stay disconnected).
