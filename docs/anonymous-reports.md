# The anonymous report (version 1)

What SoftPresenter sends to `POST /v1/public/report` when anonymous reports are on: every
field, and the rules the app and the server both keep. [PRIVACY.md](../PRIVACY.md) says the same
in plain words, and Settings › About › See what's sent shows the next report exactly as it would
go.

This page is the format's description, and the one that is edited. The app builds a report only
from the closed types in `crates/sp-app/src/reports/format.rs`, and checks it with
`crates/sp-app/src/reports/check.rs` before it is kept for sending; the server makes the same
check again. `crates/sp-app/src/testdata/anonymous-report-fixtures.json` holds reports that must
be taken and reports that must be refused: the app's tests and the server's both read that
file, so the two can't drift apart.

## The rules the format follows

- **Closed.** A field that is not listed below, at any level, refuses the whole report (400).
  Adding a field means changing the server first.
- **No words.** Views, tools, events and what is on screen are *codes*: names the program made
  up (`^[a-z][a-z0-9_]{0,31}$`), such as `service`, `tool_bible`, `slow_pass`, `video_1080p30`.
  A code can't hold a title in another script, a path, an address or a sentence.
- **Three places hold text**: a crash's message and backtrace, and the problem lines. They may
  hold only plain ASCII (so nothing in Russian, Ukrainian or Spanish, which is what a song's
  title or a file's name would be in), and the server refuses the report if it finds in them a
  home or disk folder (`/Users/…`, `/home/…`, `C:\…`, `\\server\…`), an e-mail address, an
  IPv4 or MAC address, a link, or (message and problem lines only, because a backtrace is
  full of `::` and long names) an IPv6 address or a long run of mixed letters and digits that
  could be a key. Names in plain English letters and host names without a link pass this
  check: only the app, which knows what a line is about, can leave those out. The app must already have removed all of these, and must *drop*
  a log line that could hold a name rather than clean it; the server's check is the second
  one, for the app's mistakes.
- **Small.** 12 KB as sent (gzip), 64 KB uncompressed. The lists have limits (below); when the
  app has more than fits (a full report of unlike rows is about 9 KB as sent), it leaves the
  oldest out until the gzip fits and says how many in `dropped`.

## What the app must do (the server can't check these)

- Reports are **on unless the person turns them off**, but **nothing is sent, and nothing is
  queued for sending, until the person has seen** the setup's last step, Ready, whose Reports line says so (or, in an
  install updated from an earlier build, the one-time notice with its Turn off key). A first
  start that quits before then sends nothing.
- On a computer whose **region** (not its language) is in the European Union, the rest of the
  European Economic Area (Iceland, Liechtenstein, Norway) or the United Kingdom, reports are
  **off unless the person turns them on**: the Reports line of the setup's Ready step shows the
  switch Off with a line inviting them to turn it on, and there is no one-time notice (nothing is on). The region is
  asked of the system (`crates/sp-app/src/reports/region.rs`: a Mac's `AppleLocale`, Windows'
  `GetUserDefaultGeoName`, elsewhere the territory of `LC_ALL`, `LC_MESSAGES` or `LANG`), is
  never sent and is not kept; a region that can't be read counts as one where reports start
  on. A choice the person has made is kept and always wins over either starting point.
- Turning reports off stops sending at once and deletes the queue.
- A test copy never sends: one run with `SOFTPRESENTER_QUIET`, with a data folder of its own
  (`SOFTPRESENTER_DATA`), or only to save a picture. (The testing switch in
  [AGENTS.md](../AGENTS.md) sends a test copy's reports to a server on the same computer.)
- When: one report every fifteen minutes, one as the app quits (waited for for two seconds at
  the most) and one for a crash (at the next start if it could not go), none sooner than a
  minute after the person was told (in a later run, a crash that waits and the report at quit
  go at once). With no network they wait on disk, 256 KB at the
  most, the oldest going first.
- A problem line is sent only from the areas of the log that are about the computer and the
  drawing (`gpu`, `output`, `window`, `video`), and
  only if it holds no quotation, no file name, no path, nothing the log's cleaner would
  change and nothing outside plain ASCII. A line that fails is dropped whole, never cleaned.
  A crash's message goes by the same rule.
- `install_id` is random (not made from anything about the computer), kept beside the log
  (this computer's own place, not the library, which may be copied or synced to another), and replaced with a new random one when the person asks.
- A crash's `location` and the paths in its backtrace are the *build* computer's. They are
  cut down to the part inside the program's source (`crates/sp-app/src/deck.rs:412:17`,
  `wgpu-27.0.1/src/api/device.rs`), with `/` between the parts on every system, so no
  builder's home folder is sent either. Rust's own library keeps its usual
  `/rustc/<commit>/library/…`, which is the same in everyone's build. A `location` in any
  other shape refuses the whole crash report.
- Hardware names keep to the characters below; any other character is replaced with a space.

## Sending

    POST /v1/public/report
    Content-Type: application/json
    Content-Encoding: gzip

No key and no other header. Answers: `201 {"id":"…"}` taken; `400` not a valid report (drop
it); `413` too large (drop it); `429` too many, try again in a minute or later; `503` the
store is full for today (`Retry-After` says when; keep it or drop it); anything else, try
again later. The error text never repeats what was sent.

## The fields

Required: `v`, `kind`, `install_id`, `sent_at`, `app` (with `version` and `channel`), `os`
(with `name`). Everything else may be left out.

| Field | What it is | Limits |
|---|---|---|
| `v` | the format's version | `1` |
| `kind` | `report` (how it has been running) or `crash` | |
| `install_id` | a random number the app made up; says which reports came from the same install, not whose it is; the person can reset it | 32 lowercase hex |
| `session` | another random number, new each time the app starts | 32 lowercase hex |
| `sent_at` | when the report was made, unix seconds | whole number |
| `app.version` | the app's version | 40, letters digits `. + -` |
| `app.commit` | the commit it was built from | 7 to 40 hex, or `unknown` |
| `app.channel` | `release` or `dev` | |
| `os.name` | `macos`, `windows` or `linux` | |
| `os.version` | the system's version, as it names itself (`15.1`, `Windows 11 Pro (build 26100)`) | 80, hardware characters |
| `os.arch` | `x86_64`, `aarch64` | 16 |
| `cpu.model`, `cpu.cores` | the processor's name as its maker wrote it, and how many cores | 80, hardware characters; up to 4096 |
| `ram_gb` | memory, in whole gigabytes | up to 65536 |
| `gpu.name`, `gpu.driver` | the graphics card's and its driver's name | 80 each, hardware characters |
| `gpu.backend` | `metal`, `vulkan`, `dx12`, `gl`, `other` | |
| `gpu.type` | `integrated`, `discrete`, `virtual`, `cpu`, `other` | |
| `displays[]` | each display's `w`, `h` (pixels), `scale`, `hz`, `main`; never its name | 16 displays |
| `outputs` | counts: `audience`, `stage` screens set up, how many are `on`, and how many send to a `display`, a `window`, `ndi`, `decklink` | each up to 64 |
| `integrations` | whether `ndi`, `decklink`, `ffmpeg` are there | true or false |
| `language` | the app's language (`en`, `ru`, `uk`, `es`, `de`) | like `en` or `pt-BR` |
| `uptime_s` | seconds since the app started | |
| `minutes[]` | a row for each minute of running, below | 30 rows |
| `events[]` | things that happened, below | 200 |
| `features` | how many times each view or tool was used: `{"tool_bible": 7}` | 100 entries, names are codes |
| `problems[]` | lines from the app's log that report a problem, cleaned | 20 lines of 300, ASCII, tripwires |
| `crash` | only with kind `crash`, below | |
| `dropped` | how many `minutes`, `events`, `problems` were left out to fit | |

Hardware characters: letters, digits, space and `. _ ( ) @ / + , : # -`.

A minute row (`at` and `ticks` required):

| Field | What it is |
|---|---|
| `at`, `up_s` | when the minute ended (unix seconds), and seconds since the app started |
| `ticks` | how many five-second readings it holds (1 to 60) |
| `cpu.app`, `cpu.video`, `cpu.system` | processor use in percent, each `[average, highest]` |
| `mem_mb.app`, `mem_mb.video` | memory in megabytes, each `[average, highest]`; `mem_mb.free_least` the least free memory seen, or null |
| `ui` | the operator's window: `view` (a code), `frames` drawn, how long a frame took in ms (`p50`, `p95`, `max`), how many took `over50` and `over100` ms |
| `screens` | the same for the screens: `passes`, `p50`, `p95`, `max`, `over50`, `over100`, and frames `shown` and `not_shown` |
| `scene` | what kinds of thing were on screen, up to 12 codes (`song`, `bible`, `video_1080p30`, `player_ffmpeg`, `prop`) |
| `actions` | how many `keys` were pressed and `clicks` made, and actions from Remote Control (`remote`); counts only |
| `outputs_on` | how many screens were on |

An event (`t` and `kind` required): `t` seconds since the app started; `kind` a code
(`slow_pass`, `freeze`, `display_added`, `notice`); `id` a notice's id, never its words
(`media.missing`: codes joined by dots); `n` a count; `ms` a time.

A crash: `message` (500, ASCII, tripwires), `location` (a place in the program's source,
never from the root of a disk), `thread` (40), `backtrace` (16000, ASCII lines, tripwires
except the key one), `uptime_s`.

## What is never in it

Song words or titles, Bible text, the names of files, folders, displays, screens, NDI sources
or devices, people's or churches' names, the computer's name or model, the user name, IP or
MAC addresses, Remote Control devices, passwords, keys, the log itself, anything the person
typed. The server adds nothing of its own to what it keeps: no IP address and no country.

## What the server keeps

The report, written out again from the fields it checked (so nothing unchecked is stored),
with its install id, version, system, kind, size and the time it arrived, for 90 days; then a
daily clean-up deletes it. The server adds nothing of its own: no IP address and no country,
and its request logs are switched off. To have the reports of an install deleted sooner, send
its anonymous id (Settings › About, or `sp reports`) in an issue.
