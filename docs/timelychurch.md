# TimelyChurch

[TimelyChurch](https://timelychurch.com) is a church-management app. Its worship side holds a
church's songs, its setlists (the songs a team sings at a service, in order) and its service
plans (the whole running order). It pairs devices with a code, under the name Song Sync.

SoftPresenter connects to a church's TimelyChurch as a device. The church's upcoming services
become playlists, and their songs become library songs, in named groups, in the order sung,
with the translation under each slide. SoftPresenter keeps them in step as the plans change
during the week, never overwrites a change the church made in SoftPresenter, and tells
TimelyChurch which songs were shown in each service, for the church's song use report (which
feeds its CCLI report).

**Where it stands (24 November 2026):** TimelyChurch agreed to build a read-only **Worship API,
version 1**, for presentation apps, planned for one of its December releases, and will send an
OpenAPI file and example JSON for every request first. SoftPresenter's side is built against
the API as TimelyChurch described it in its answer, and tested against a stand-in of our own
(`crates/sp-app/src/timelychurch/mock.rs`). It is **hidden until the API ships**: the card and
`sp timelychurch` are offered only when `SOFTPRESENTER_TIMELYCHURCH_URL` is set, and without it
nothing is ever asked of the network or the secure store. When the OpenAPI file comes, the
field names marked `assumed:` in `wire.rs` are checked against it ([the list](#names-assumed)).

## The Worship API, as described

| Request | What it does |
| --- | --- |
| `POST /api/v1/worship/devices/pair` | The pairing code an administrator made in TimelyChurch (Setlists › Song Sync), the computer's name, and the app's name and version. Gives back the device token and the church's name. |
| `GET /api/v1/worship/services?from=…&to=…` | The setlists and service plans between two dates, each with its items in order (a song, a reading, the sermon or another placeholder; song and arrangement ids; key; notes) and when it last changed. ETag and `If-None-Match`: a short 304 when nothing changed. A removed service is gone from the list. |
| `GET /api/v1/worship/songs/{id}?arrangement=…` | A song: title, authors, CCLI number, copyright, key, its sections (a name and lines, each section once), the order they are sung in as a list of section names (a repeat is the name again), each **published** translation by language code, lined up with the sections line for line (`null` for a line nobody translated), and when it last changed. A deleted song is a 404. |
| `POST /api/v1/worship/song-uses` | One song shown in a service (song, arrangement, date and time, service, key), with an `Idempotency-Key` header so a retry never counts twice. |

The token goes only in the `Authorization: Bearer` header, never in an address. A computer
disconnected in TimelyChurch, or a church that turned the feature off, gets a 401 or 403, which
SoftPresenter shows as "This computer was disconnected from TimelyChurch".

TimelyChurch asked that songs be kept for that church's use only (never shared between
churches or sent anywhere else), the token be kept in the system's secure store (never in a
plain file or a log), and the church's own edits be kept with its newer version offered.
Stands following the screens come after version 1 is in use.

## How it maps

### A song

`timelychurch::make_song` (`song.rs`) turns a song, in one arrangement, into a SoftPresenter
song.

| TimelyChurch | SoftPresenter |
|---|---|
| The title (HTML escapes undone) | The title. A song sung in an arrangement that isn't its default is a library song of its own, "Title (Arrangement)". |
| Each section, once | A group, named as the church named it. |
| A blank line in a section | A new slide. A stretch longer than four lines (the app's usual for songs from text) is shared out evenly. |
| Chords in square brackets, repeat signs (`\|\|:` `:\|\|`), should a line carry any | Left out: they are for the musicians. A line left with no words is no line. |
| The order sung | An arrangement of the groups, chosen to play, named after TimelyChurch's arrangement (or "TimelyChurch" for one called "Default" or "Default Arrangement"). None when the order is simply the sections as sent, or there is no order. Names are matched whatever their case and spacing; two sections sent with one name are taken in turn. A name that names no section with words (an instrumental part) is no group, and is said in the log. |
| A published translation | Lined up by place: the translation's section at the song's section's place (or, if that one names another section, the one of this section's name), and its line *n* under the section's line *n*, counted as sent, before anything is left out. Each slide gets a translation box with the translated lines of its own lines, in order. A `null` line is left out and counted; a slide with none translated has no box; a translation with nothing translated puts nothing under the words. |
| Several translations | The first, in the order sent, with a line translated (`song::translation_for`, the one place to change if the church is to choose). |
| The translated title | Not used: a song has one title. |
| Authors, CCLI number, copyright line | Author (the names joined with commas), CCLI Song #, and the year and publisher read from the copyright line. |
| Key | Not on slides, not kept. The song use sends the key the service gave. |

### A playlist

`plan.rs` turns each service into what it asks for, and `to_service` into the playlist once its
songs are in the library.

- **A setlist** becomes a playlist with its name and its songs in order, with a heading wherever
  the songs' worship-spot label changes ("Opening", "Communion"), if the list carries labels.
- **A plan** becomes a playlist with its items in order: a `heading` (or `section`) item is a
  heading; a `song` item is that song; a `reading` is a placeholder named with its passage
  ("Reading: Psalm 23"); any other item a placeholder with its title (Sermon, Offering…).
- **A setlist and a plan for the same calendar event**, if the list says which event each is for,
  make one playlist: the plan's order, the setlist's songs in its `worship` places (by label) or
  its empty song places; the setlist is then no playlist of its own.
- A song not in the library (not fetched yet, or deleted in TimelyChurch) keeps its place as a
  placeholder with its title. A song comes once, as in any playlist.
- Services with the same name get their date added: "Sunday Service (2026-12-06)".
- Names the app gives (a placeholder with no title: Song, Reading, Sermon…) are in the language
  set.

## Identity, and the church's own changes

**Songs** carry the song catalog's mark (`CatalogOrigin` in `sp-model`) with the source
`timelychurch`, so the catalog's update pass never touches them:

- **key:** `church.example.org/song/1201/arrangement/1305` (the address the church was
  connected with, then TimelyChurch's ids), so two churches on one computer never collide;
- **second:** the language of the translation under the words;
- **made_hash:** the fingerprint of the song as made; any change the church makes (words,
  title, details, theme, arrangements, a slide's look) gives another;
- **text_hash:** the fingerprint of the song as SoftPresenter makes it from what TimelyChurch
  sent, so a change there that changes nothing shown is no change;
- **newer** and **gone**, as for the catalog.

At each fetch the song is made again and the catalog's rules (`catalog::judge`) decide:
unchanged, nothing; changed and untouched, replaced (keeping its id, theme, transition and
looping, so playlists and macros still find it); changed and changed by the church, kept, with
the newer version offered (a notice, and `sp timelychurch newer`, `use-newer SONG`, which can
be undone, and `keep-mine SONG`); deleted (404), noted as gone and kept. New songs go into a
library named TimelyChurch.

**Playlists** carry a mark in the services file (`ServiceOrigin`: the key
`church.example.org/setlist/42` or `…/plan/57`, the fingerprint of the playlist's name and
items as made, the service's date, a newer version's fingerprint, and whether TimelyChurch
removed it). Older builds drop the mark when they save. At each check:

| TimelyChurch's service | The playlist | What happens |
|---|---|---|
| Makes the playlist as it was made | Any | Nothing (an offer taken back is withdrawn). |
| Changed | Untouched | Made again. |
| Changed | Changed by the church | Kept; TimelyChurch's version offered, said once (`sp timelychurch use-newer PLAYLIST` takes it). |
| New | — | A new playlist, at the end of the list. |
| Gone from a list answered now, its date still inside the dates asked for | Any | Noted as removed in TimelyChurch (the card says so); never deleted. |
| Its date has passed | Any | It has only dropped out of the dates asked for: left as it is. |
| Any | Deleted by the church | Not made again. |

## Keeping in step

**When:** while connected, ten seconds after the start and then every five minutes (and on
Check Now), for the next 28 days. Requests run on threads of their own, never on the window's
or the screens'.

**What is asked:**

1. The services list, with the ETag of the list last applied. A 304 means the list kept on disk
   (`TimelyChurch/services.json`, with its ETag) stands. The ETag is kept only together with
   the list it belongs to, and only once what the list made has been applied; it is dropped on
   Disconnect, on pairing again and when the token is refused.
2. Each song that may have changed, once per check however many services it is in: one never
   fetched or not in the library any more; one whose last-changed time in the list differs
   from when it was fetched (if the list says when songs last changed); every song of a
   service whose last-changed time changed; and otherwise each song about once a day (a song
   TimelyChurch deleted too, in case it comes back). Each song's reply is kept as sent
   (`TimelyChurch/songs/`), so a newer version offered can be taken later with no network.

With nothing changed, a check is one short request.

**Applying** happens on the window's thread. New songs and new playlists come at once. Changes
to songs and playlists wait while anything is on screen (as the catalog's updates do), a song
open in the editor waits until it is closed, and no new check starts while a check's findings
wait (Check Now still does). Notices say what came, what was brought up to date, what is offered
and what was removed.

**Problems:** a 401 or 403 means the computer was disconnected in TimelyChurch: the token is
forgotten, the card and the notices say "This computer was disconnected from TimelyChurch", and
nothing more is asked until it is connected again. A 429 (or 503 with Retry-After) is waited
out. No network: the playlists and songs already made work as they are (they are ordinary
songs and playlists, saved as every other is), the card says TimelyChurch can't be reached, and
it asks again five minutes later.

## Song use

A TimelyChurch song **counts as shown** when it goes on screen (the slide on screen, whatever
the screens show) from the playlist in front, when that playlist was made from a TimelyChurch
service, on the service's date (the computer's date), and the song is of the church connected
now. It counts **once per song per service**: the sound check and the service on the day are
one use, and showing the song on another day (a rehearsal in the week) is none. The count is
remembered on disk (`TimelyChurch/song-uses.json`), so starting the app again doesn't count it
again.

Each use is queued on disk with its own idempotency key, made once when it is queued from what
it is (the service and the song: the same use always has the same key), and sent while
connected; it leaves the queue only when TimelyChurch has it (a 2xx, or a 409), or when
TimelyChurch will never take it as it is (400, 404, 410, 422, said in the log). A network
problem leaves it queued and it is tried again a minute later; a retry never counts twice. The
use says the song and arrangement, when it was shown (RFC 3339 with the computer's offset), the
service and its kind, and the key the service gave.

## Connecting: Settings › Integrations › TimelyChurch

TimelyChurch has a row beside VoxLive and Canva Sync ("Your upcoming services as playlists,
with their songs"), shown only while it is offered. Its page:

- **Not connected:** CONNECT TO TIMELYCHURCH, with "In TimelyChurch, an administrator opens
  Setlists › Song Sync and makes a pairing code. Type your church's TimelyChurch address and the
  code here. The connection is kept in this computer's secure store and used only with your
  church's TimelyChurch.", the fields Address and Pairing code, and **Connect**. If the OpenAPI
  file shows one address for every church, `client::ONE_ADDRESS` is set and the Address field
  goes. The address is taken as typed in a browser (`https://` added); plain http only for this
  computer, for the stand-in. SoftPresenter pairs as a device named after the computer (its
  name in Sharing on a Mac), as "SoftPresenter" with its version.
- **Connected:** "Connected to <church> · last checked <time>" (or "checking now", "not
  checked yet"), the address, **Check Now** and **Disconnect**, how many songs shown are waiting
  to be told to TimelyChurch, and UPCOMING SERVICES: each playlist made, with its date, its
  songs, and Up to date, Changed here, Changed here; newer in TimelyChurch, or Removed in
  TimelyChurch.
- **Disconnect** asks first ("Disconnect TimelyChurch? This computer stops following
  TimelyChurch and forgets its connection. The songs and playlists it made stay as they are.")
  and forgets the token on this computer.
- **Disconnected in TimelyChurch:** "This computer was disconnected from TimelyChurch." over the
  connect form, after a restart too, until it is connected again.
- The logo comes later (TimelyChurch will send its files); until then a "T" mark.

The token goes into the Mac's Keychain (a test copy started with `SOFTPRESENTER_DATA` keeps its
own in its own folder). `token.rs` holds a stand-in for `crate::secrets` (the store over the
Keychain, Windows Credential Manager and Linux Secret Service being written now), with the same
`get`, `set` and `delete`, to be replaced by it; until then only a Mac keeps a token. The token
is never written to a settings file, the log, a notice, a command's reply or a song pack.

`sp timelychurch` does the same from the command line: `sp timelychurch` (how it stands),
`echo "$CODE" | sp timelychurch connect [ADDRESS]`, `check`, `services`, `newer`, `use-newer
SONG|PLAYLIST`, `keep-mine SONG`, `disconnect --yes`. A remote page can't use it.

## The stand-in

`timelychurch/mock.rs` is a small HTTP server (std::net only) serving a demo church in the
described shapes: a setlist today and a plan in a week, a two-language song whose chorus is
sung three times, a one-language song, and a deleted song (404); it answers 304 to its ETag,
records each song use once by its idempotency key, and can be changed while it runs
(`POST /_mock/next`: a song's words changed, the plan removed, a new setlist; and again: the
two-language song's bridge changed; `/_mock/disconnect`;
`/_mock/wait`; `GET /_mock/uses`). The tests run against it; to try the app by hand,
`cargo test -p sp-app timelychurch::mock::serve -- --ignored --nocapture` serves it at
`http://127.0.0.1:4320` (pairing code `DEMO-2026`), and a test copy is started with
`SOFTPRESENTER_TIMELYCHURCH_URL=http://127.0.0.1:4320`.

## Names assumed

TimelyChurch's answer gave the paths and what each request carries, not the field names. Each
name below is marked `assumed:` in `wire.rs`; the likeliest other spellings are read too.

| Where | Assumed | Also read |
|---|---|---|
| Services query | `from`, `to` as `YYYY-MM-DD`, both days included | — |
| Song query | `arrangement` (left out for the default) | — |
| Pair request | `pairing_code`, `device_name`, `app_name`, `app_version` | — |
| Pair reply | `token`; `church` object with `id` and `name` | `device_token`, `access_token`; `church_name` |
| Services reply | an object with `services` | `data`, `items`, or the list itself |
| Service | `id`, `type` (`setlist` or `plan`), `name`, `date`, `time`, `event_id`, `updated_at`, `items` | `kind`; `title`; `linked_event_id`; `last_changed`, `changed_at`, `last_changed_at` |
| Item | `position`, `type` (`song`, `reading`, `sermon`, `heading`, `worship`, other), `title`, `song_id`, `arrangement_id`, `key`, `notes`, `worship_spot`, `reference`, `song_updated_at` | `order`; `kind`; `name`; `spot`; `scripture_reference`, `passage`; `song_last_changed`, `song_changed_at` |
| Song | `id`, `title`, `authors` (a list), `ccli_number`, `copyright`, `key`, `arrangement` (`id`, `name`, `key`, `is_default`), `sections` (`name`, `lines`), `order` (strings), `translations`, `updated_at` | `name`; `author`, or one line with commas; `ccli`, `ccli_song_number`; `copyright_info`, `copyright_line`; `default`; `label`; `sequence`, `section_order`, `order_sung`, objects with a `name`; `last_changed`… |
| Translation | `language`, `title`, `sections` (`name`, `lines` with `null`) | `language_code`, `lang` |
| Song use | `song_id`, `arrangement_id`, `used_at`, `service_id`, `service_type`, `key`; header `Idempotency-Key` | — |
| Errors | `error` (a string or an object with `message`), `message` | — |
| Ids | integers | strings (letters, figures, `-`, `_`, `.` only go into an address) |

## What waits on TimelyChurch

- **The OpenAPI file and examples:** the names above; the per-device rate limit (five minutes
  between checks is assumed to be well inside it); whether there is one address for every
  church.
- **How the services list says a song changed:** a last-changed time per item, a service's
  last-changed time moving when its songs change, or neither. All three are handled; with
  neither, a changed song's words arrive within a day, or at once when its service changes.
- **Worship spots and events:** whether items carry a worship-spot label and services an event,
  and so whether a setlist and a plan still need joining.
- **Lines:** whether section lines can carry chords (they are taken out if they do).
- **A choice of translation**, if churches publish more than one.
- **The logo files**, and a development build for TimelyChurch's own demo church before any
  church sees it.
- **Later:** stands following the screens (which song and section is on screen).

## Build plan

| Step | What | Where it stands |
|---|---|---|
| 1 | The mapping: songs (sections, order sung, translation lined up, details, mark) and playlists | Done, for the described API |
| 2 | The client: pairing, the token in the system's store, the reads, errors, the stand-in | Done (the store is a stand-in until `secrets.rs`) |
| 3 | Keeping in step: the background check, fetching only what changed, the update rules, the TimelyChurch library, the playlists' mark, notices | Done |
| 4 | Settings › Integrations › TimelyChurch and `sp timelychurch` | Done, hidden behind `SOFTPRESENTER_TIMELYCHURCH_URL`; the logo when it comes |
| 5 | Song use, queued offline | Done |
| 6 | Readings as Bible passages | Waits on how a passage is written |
| 7 | Stands following the screens | After version 1 is in use |

Once the OpenAPI file is in: check the names, try it against TimelyChurch's demo church with a
development build, then take away the switch.
