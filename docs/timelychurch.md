# TimelyChurch

[TimelyChurch](https://timelychurch.com) is a church-management app. Its worship side holds a
church's songs, its setlists (the songs a team sings at a service, in order) and its service
plans (the whole running order). It also pairs devices with a PIN, under the name Song Sync.

SoftPresenter will connect to a church's TimelyChurch as a Song Sync device. The church's
upcoming services will become playlists, and their songs will become library songs, in named
groups, in the order sung, with the translation under each slide. SoftPresenter will keep them
in step as the plans change during the week, and will never overwrite a change the church made
in SoftPresenter.

**Where it stands (24 November 2026):** this page is the design. The mapping, from TimelyChurch's
data to SoftPresenter's songs and playlists, is built and tested in `crates/sp-app/src/timelychurch/`.
It is not wired into the app yet. The connection itself waits for TimelyChurch's description of
its device interface. [What waits on TimelyChurch](#what-waits-on-timelychurch) lists each
question by the ID it has in the questions sent to TimelyChurch's developer (P1, R3…).

## What TimelyChurch holds

We read this read-only through TimelyChurch's assistant connector, on one church's account. Only
the shapes are recorded here. The device interface may name or shape things differently.
`crates/sp-app/src/timelychurch/wire.rs` models the fields the mapping uses.

| Resource | Fields seen | What we learnt |
|---|---|---|
| Song Sync devices | name, status (`paired`), last seen; for the church: stand link issued, PIN required, PIN mode `self`, enrolment open and until when, 180 days of lookahead | No address, code or token is shown. |
| Setlist | id, name, date, team, song count, linked calendar event, next rehearsal, active | A church can have several setlists per week, one for each team. |
| Setlist song | order, song id, title, arrangement, target key, capo, Nashville flag, notes, worship spot | It has **no id of its own**: its place and its song identify it. `arrangement` was `null` every time (meaning the song's default). |
| Service plan | id, title, series, type, date, time, status, event, item count, total duration | Titles may carry HTML entities (`&amp;`): decode them before showing. |
| Plan item | order, type, title, description, song id and title, key, capo, duration, leader, scripture reference, notes, worship spot | Types seen: `song`, `worship`, `section`, `scripture`, `prayer`, `sermon`, `offering`, `announcement`. A plan can be a template whose song slots name no song; then **the songs are in the setlists**. A `worship` item carries a worship-spot label. |
| Song | id, title, artist, composer, CCLI number, copyright, default key, tempo, BPM, time signature, status, tags, themes, notes, plain lyrics, arrangements, attachments, recent use | The plain `lyrics` have no section names. They can be missing, can carry ChordPro directives (`{title: …}`). |
| Arrangement | id, name, key, available keys, BPM, default flag, chart format (`chordpro` or `chords_above`) | An arrangement is **a chart in a key, not an order of sections**. A song usually has one default arrangement. |
| Chart | the arrangement's sections in order: a name and lines with chords in square brackets; as stored, `[Verse 1]` lines start sections | Section names are free text (Verse 1, Chorus, Bridge 1, Final Chorus, Half Chorus, Припев, куплет 1). See [how a chart writes a song](#how-a-chart-writes-a-song). |
| Translation | language, status, translated title, sections of `{source, translated}` rows, languages | Its sections are **pieces of at most six rows** (the slide style's lines per slide): a section's name can repeat, and a piece can cross a verse's blank line. The rows have no chords. A line nobody translated has `translated: ""`. |
| Congregation slide style | background, original and translation colours, sizes, gap, offsets, layout (`original_first`), lines per slide (6), and whether the church ever saved one | A church may never have saved one (`has_default` false): then SoftPresenter's own look applies. |
| Song use | when, the key used, the plan (`null` every time) | Several uses can fall on one day. |

**Identity and change:** every id is an integer, unique in one church. No resource carries an
`updated_at`, a version or an ETag. Setlist songs and plan items have no ids.

**Setlists and plans:** a setlist and a plan can be for the same calendar event; one pair was.
Another setlist and plan were on the same date, for different events.

### How a chart writes a song

A chart writes every section where it is sung. Repeats come in several forms:

- the section written out in full again, word for word;
- the section named again with no lines ("Chorus" with nothing under it);
- a count in the name ("Chorus - 2x", "Chorus x2", "Припев 2х", "Ending (2x)");
- a count on a line of its own inside another section ("Chorus: 3x"), followed by the chorus;
- repeat signs inside a section (`||: two lines :||`), sometimes not closed;
- the same name with slightly different words (a last chorus with a line added).

Charts also hold things that are not words to sing. These are lines of chords alone (`[Intro:]
[G] [D]`, `INTRO: Am – F – C`, `F------`) and notes to the band ("End", "Play Verse 1"). A
`chords_above` chart arrives as one section with no name. Its section names come as lines of
their own ("Verse 1", "Chorus 2x").

## How it maps

### A song

`timelychurch::make_song` turns a song, one arrangement's chart and, if there is one, a
translation into a SoftPresenter song.

| TimelyChurch | SoftPresenter |
|---|---|
| The song's title (HTML escapes undone) | The song's title. A song sung in an arrangement that isn't its default is a library song of its own, "Title (Arrangement)". |
| The chart's sections | Groups, named as the church named them, without a count. Lines before any named section form a group with no name. |
| A blank line in a section | A new slide. A stretch longer than the most lines a slide takes is shared out evenly, as for songs imported from text. That most is the church's saved lines per slide, or SoftPresenter's usual four when the church never saved one. TimelyChurch's own default of six is not the church's choice. |
| Chords, lines of chords alone, repeat signs, and a few notes to the band ("End", "Fine", "Play …", "Repeat …" followed by a section name) | Left out: they are for the musicians. A line that only might be a note is kept as words. |
| The same section again (same name and words), a section named with no lines, a count | The same group again, as many times as the count says. A name with no lines means the first group of that name, or failing that its first numbered one ("Chorus 1" for "Chorus"). A name with no lines that names nothing sung before it (an instrumental intro, or a chorus named before the chorus is written) is no group; such names are listed, so the window can say so. |
| The same name with other words | Another group of the same name: SoftPresenter allows repeated names. |
| The order sung | An arrangement, chosen to play, named after TimelyChurch's arrangement (or "TimelyChurch" for one called "Default Arrangement"). There is none when the order sung is simply the groups as written. |
| A translation | Lined up **line by line, by the line's words** rather than by section, because the translation's sections are only pieces. Each slide gets a translation box holding the translated lines of its own lines, in order. A line translated once is translated wherever it comes again. A line nobody translated is left out of the box and counted, so the window can say so. A slide with no translated line has no box, and a translation with none at all puts nothing under the words. |
| The translation's status | Used when it is approved or not given. A draft is used only if the church chooses (once R4 says what the statuses are). |
| The translated title | Not used yet: a song has one title. |
| Artist, composer, CCLI number, copyright line | Artist, Author, CCLI Song #, and the year and publisher read from the copyright line. These go on the copyright line on the screens. |
| The song's notes | The first slide's notes. |
| Key, capo, BPM, Nashville flag, tags, themes, scripture reference | Not shown on slides, and not kept for now. A key for the stage screen could come later. |
| Plain lyrics | Not used. |
| Attachments | Not used (R7). |

### A playlist

`timelychurch::from_setlist` and `from_plan` say what a service asks for. `to_service` then makes
the playlist once its songs are in the library.

- **A setlist** becomes a playlist with the setlist's name, holding its songs in order. A heading
  is added wherever the songs' worship-spot label changes ("Opening", "Communion").
- **A plan** becomes a playlist with the plan's title, holding its items in order:
  - a `section` becomes a heading;
  - a `worship` item becomes a heading with its label, followed by the setlist's songs that
    carry the same label (or all of them, at the first such item, when no song carries one);
  - a `song` item with a song becomes that song. Without one, it takes the setlist's next song
    when the plan has no `worship` item; otherwise it stays a placeholder ("Song");
  - a `scripture` item becomes a placeholder named with its passage ("Reading: Psalm 100"). Once
    R6 gives the format, it will become a Bible passage in the Bible view's translations;
  - every other type becomes a placeholder with the item's title (Sermon, Offering, Prayer…).
    Durations, leaders' names, descriptions and notes are not used.
- **A setlist and a plan for the same calendar event** make one playlist: the plan's order, with
  the setlist's songs. Any songs no place took follow the last song. A setlist and a plan on the
  same day for different events are not joined, and nothing is matched by date.
- **Which services appear:** every upcoming setlist inside the lookahead window. A plan replaces
  its setlist when it is linked to one, and stands alone when it holds songs of its own. A plan
  with neither is a template and is left out.
- A song not in the library yet (it couldn't be fetched) keeps its place as a placeholder with
  its title, and a song comes once, as in any playlist.
- A setlist song's notes, target key and capo are not used: a playlist's song item has no place
  for them, and the key and capo are for the musicians. If they are wanted on the stage screen
  later, the playlist item needs a field for them.
- Upcoming services with the same name ("Sunday Service" each week) get their date added:
  "Sunday Service (2026-12-06)".

## Identity, and the church's own changes

**Songs** carry the same mark as songs from the song catalog (`CatalogOrigin` in `sp-model`),
with the source `timelychurch`. The catalog's update pass picks its songs by source, so it
never touches TimelyChurch's songs, and the sync will pick its own the same way. The mark holds:

- **key:** `church.example.org/song/1201/arrangement/1305`. This is the address the church was
  connected with, then TimelyChurch's song and arrangement ids, so two churches on one computer
  never collide;
- **second:** the language of the translation under the words;
- **made_hash:** the fingerprint of the song as SoftPresenter made it. That is the whole song,
  with its ids numbered afresh and the mark left out. **Any change the church makes in
  SoftPresenter** gives the song another fingerprint: words, title, details, theme,
  arrangements, or a slide's look;
- **text_hash:** what TimelyChurch had. TimelyChurch's songs carry no version, so this is the
  fingerprint of the song as SoftPresenter would make it from TimelyChurch's data. A change at
  TimelyChurch that changes nothing shown (a new BPM) is then no change. The other side of
  this: anything that changes how songs are made changes every song's fingerprint at once.
  That covers the church's lines per slide, its second language, or a later release's
  mapping. Untouched songs are then made again, and songs the church changed are each offered
  the newer version;
- **newer** and **gone**, as for the catalog.

At each check, the song is made again from what TimelyChurch sends, and the catalog's rules
decide (`catalog::judge`):

| TimelyChurch | The song in SoftPresenter | What happens |
|---|---|---|
| Unchanged | Any | Nothing. |
| Changed | Untouched | Replaced by the new version. It keeps its id, theme, transition and looping, so playlists, macros and links still find it. |
| Changed | Changed by the church | Kept. The newer version is offered ("Use the newer version" or "Keep mine", as for the catalog), never forced. |
| The song is gone | Any | Noted as gone; the song stays. |

Songs that join the library go into a library named TimelyChurch. A library song of the same
title that didn't come from TimelyChurch is never merged on its own. On the first sync, such
songs are listed, and the church can choose to use the song it already has for that
TimelyChurch song. That choice is remembered by TimelyChurch's song id, and the church's song is
then never updated.

**Playlists** have no id or origin today. A synced playlist will carry a mark of the same kind,
a small addition to the services file that older builds drop on saving. The mark holds the key
(`church.example.org/setlist/42` or `…/plan/57`), the fingerprint of what TimelyChurch asked
for (`PlannedService::fingerprint`), and the fingerprint of the items as made. A playlist the
church hasn't touched is rebuilt when TimelyChurch's setlist or plan changes. One the church has
changed (a video added, a song moved) is left alone, and the change is offered.

## Keeping in step

**What is asked for, with what exists today.** Nothing tells SoftPresenter what changed, so it
compares fingerprints. It fetches in tiers, so that checking often stays cheap:

1. **Every check:** the upcoming setlists and plans inside the lookahead window, as two lists.
   Then each one in full, which is a request for each service.
2. **When a song is first seen, or when a setlist or plan it is in has changed:** the song, its
   arrangement's chart, and its translation into the church's chosen second language. That is
   three requests for each song, made once per check even when the song is in two services.
3. **Otherwise, about once a day, and never while anything is on screen:** every song again, to
   catch a change to a song's words that left its setlists as they were.

With three upcoming services, a check that finds nothing costs about five requests. A full
fetch of fifteen songs costs about fifty.

**What we asked TimelyChurch for:** a "changed since" query, or `updated_at` and ETags (S1).
With those, a check that finds nothing costs one small request, and a changed setlist or song
fetches only itself. The daily pass over every song is then no longer needed.

**How often:** every five minutes while the app is open, and every minute in the hour before a
service (the plan's time, or the setlist's date). This is what S3 proposes; TimelyChurch may
ask for less. Requests run on a thread of their own and never on the thread that draws the
screens. A "please wait" (429 with Retry-After, S4) is waited out.

**Applying what changed:** fetching happens at any time, but nothing changes on screen.

- A new service appears at once as a new playlist.
- A change to a song that is on screen or open in the editor waits until nothing is live and
  the editor is closed, as the catalog's updates do.
- A change to the playlist in front waits until nothing is live. A notice says what changed.

**Deletions, today:** a setlist or plan missing from the list while still inside the window is
noted on its playlist as removed in TimelyChurch. The playlist is never deleted on its own. A
service whose date has passed drops out of the window and simply stops being followed. A song
TimelyChurch no longer has is noted as gone. With tombstones or a list of removed ids (S2),
removal would no longer have to be guessed from absence.

## Offline

A service never depends on the network. Synced songs are ordinary library songs and synced
playlists ordinary playlists, saved as every other is. Every song in every service inside the
window is fetched ahead, so the church can run any of them with no network. A check that fails
changes nothing, and the Integrations page says when TimelyChurch was last reached. Song use
(W1) will be kept in a queue on disk, each with its own key so a retry never counts twice, and
sent when the network is back.

## Connecting: Settings › Integrations

TimelyChurch gets a row beside VoxLive and Canva Sync, with how it stands. Its page follows
VoxLive's:

- **Not connected:** the church's TimelyChurch address and the Song Sync PIN, and **Connect**.
  SoftPresenter pairs as a device named after the computer, and sends its version (P2). The
  token goes into the computer's store for secrets (`secrets.rs`: the Mac's Keychain, Windows'
  Credential Manager, or the keyring on Linux, where connecting is refused if there is none
  rather than keeping the token in a plain file). A test copy keeps its own in its own folder. The token is never written to a settings file, the log, a command's reply
  or a song pack. It is sent only to the church's TimelyChurch address, over HTTPS, with
  redirects off.
- **Connected:** the church, when it was last checked, and the upcoming services (date, name,
  songs, and whether each is followed, changed here, or has a newer version waiting). The page
  also has **Check Now**, the second language for translations (from the languages the
  church's songs have), and **Disconnect**, which asks first.
- **Disconnected in TimelyChurch** (P5): the page says "This computer was disconnected in
  TimelyChurch" and stops asking until it is connected again.
- `sp timelychurch status | connect | check | disconnect` do the same from the command line. The
  PIN is piped in, as `sp voxlive connect` takes its key.

## What waits on TimelyChurch

| ID | Question | What the connector already showed | Still needed |
|---|---|---|---|
| P1–P6 | Pairing, base address, scopes, revoking, several devices | Devices have a name, status and last seen; PIN mode `self`; an enrolment window; 180 days of lookahead; a church can have more than one device paired | The whole pairing exchange, the token's form and life, the address rule, the error when revoked |
| R1 | Setlists, plans or both | Songs live in setlists; plans can be templates with song places and worship places; the two meet by calendar event | Whether a plan's `worship` item is meant to take a setlist's songs, and how churches use the two |
| R2 | The read endpoints | The fields above | The device's own routes, fields and paging |
| R3 | Sections and order | The chart holds named sections in the order sung, with repeats written as above; names are free text; the plain lyrics have no sections | Whether the device gets the chart, and whether an arrangement order (V1 C V2 C) could be given instead of repeats written into the words |
| R4 | Translations | Rows of source and translated lines, in six-row pieces; `""` for a missing line; status and translated title `null`; no song had a translation | A filled example, the list of a song's languages, what the statuses are |
| R5 | Chords | ChordPro with chords in square brackets; `chords_above` charts too; the plain lyrics can carry directives | Whether a chord-free form exists for the device |
| R6 | Items that aren't songs | The eight types above; no scripture reference seen | The reference's format |
| R7 | Attachments | Type, file name, size, and whether it is a link elsewhere | Whether a device can fetch a background picture or video |
| R8 | Identity and versions | Integer ids; no `updated_at` anywhere; no ids for setlist songs or plan items | Whether ids last, and a version on each resource |
| S1–S4 | Changes, deletions, push or poll, limits | Nothing for any of them | All four |
| W1 | Song use | Uses are recorded with a date and key | The endpoint, and an idempotency key |
| W2, W3 | What's live; heartbeat | Devices show a last-seen time | Both |
| X1–X6 | Test church, versions, errors, plans, naming, licensing | — | All six; a test church with translated songs matters most |

## Build plan

| Step | What | Size | Waits on |
|---|---|---|---|
| 1 | The mapping: songs (groups, slides, order sung, translation line by line, details, mark) and playlists (setlists, plans, linking, placeholders). `crates/sp-app/src/timelychurch/`, with tests on made-up data | Done | — |
| 2 | The device client: pairing, the token in the system's store, the reads, errors, a stand-in for tests (`SOFTPRESENTER_TIMELYCHURCH_API`, local addresses only, as VoxLive's) | M, 2–3 days | P, R2, X2, X3 |
| 3 | Keeping in step: the background check, fetching each song once, fingerprints, the update rules, the TimelyChurch library, the playlists' mark in the services file, notices | M, about 3 days | S1, S2 (works without them, at a higher cost) |
| 4 | Settings › Integrations page and `sp timelychurch` | S–M, 1–2 days | P2, X5 |
| 5 | Song use, queued offline | S, 1 day | W1 |
| 6 | Readings as Bible passages | S, half a day | R6 |
| 7 | Stands following the screens | M | W2, after the rest |

Steps 2 to 5 are about a week once the answers arrive. After that, a development build goes to
TimelyChurch's team to try.
