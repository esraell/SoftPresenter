# The song catalog: songs from Worship Leader App

A working note, not legal advice.

SoftPresenter can download free songs from [Worship Leader App](https://worshipleaderapp.com),
which publishes a songbook for each language as a song pack (a zip of one XML file per song).
The catalog is kept apart from the church's library (`Catalog/worshipleaderapp/<language>/` in
the data folder) and a song joins the library only when a person adds it to a playlist, drags it
in or edits it.

## Whose texts they are

The songs and their words are the source's. **SoftPresenter does not contain, ship or host any
of them.** Each church's copy of the app downloads the languages that church chooses, straight
from the source's own server, on that computer, and asks the same server about once a month
whether they changed. Nothing from the catalog is in this repository: the tests write their own
song packs with words written for them.

- The terms the source publishes the songs under have **not** been recorded here. Before the
  catalog is offered to anyone, the owner should read them (or ask the source) and put what
  they say in this note, and say there whether the source has agreed to the app naming and
  linking it.
- Every catalog song carries the credit "From Worship Leader App" and the source's site, and a
  copy in the library keeps the mark (`catalog` in the song file), so the windows can name the
  source wherever a catalog song is shown, and a song pack exported with it keeps the credit.
  Those windows are not built yet; `NOTICE.md` names the source now. Nothing is put on the
  audience's screens unless the church chooses it.
- The songs' authors and copyright holders, where the pack gives them, are kept as the song's
  details and shown in the copyright line as any song's are.
- A church that copies a song into its library is responsible for the licence it needs to show
  that song, as for any song.

## Offered in development builds only

Until the source has answered the owner's email (24 November 2026), the windows that offer the
catalog (the setup's Songs step, Settings › Integrations › Worship Leader App) are shown
only in development builds: those compiled with `SOFTPRESENTER_DEV_VERSION` set
(`catalog::offered`). A release shows none of them. The `sp catalog` commands are in every build.

## Which languages

The source publishes no list of its packs to fetch (its folder of packs refuses to be listed).
Its download page has a table of them, with English names and rounded song counts, and says
the files are brought up to date daily; it states no terms of use. The app's list
(`catalog/books.rs`) was written from that table as it stood on 24 November 2026, leaving out
the full database, the same songs in another script, and the packs listed with about ten songs;
the page is never read by the app. The table's counts don't agree with the packs, so they are
not kept: a window shows the size the source gives for a pack, and its songs once downloaded.

## What the app does with the source

- It asks for `<language>.qsp` with `If-None-Match` and `If-Modified-Since` from the last
  fetch, so a check that finds nothing new downloads nothing. It asks for the file as it is
  (`Accept-Encoding: identity`): asked for compressed, the source compresses the zip again as it
  sends and gives no length, so a window could say neither its size nor how far it has got.
- The file is untrusted: it is read through the same bounded zip reader as the other zip files
  the app opens, each song file is bounded in size and depth, a song that isn't well-formed is
  left out, and a pack that would empty most of the catalog is not believed.
- It sends nothing about the church, the library or the computer: a plain `GET` (or `HEAD`, to
  say how big a download is) of one address.

## Libraries

No new library. The catalog uses crates SoftPresenter already links: `zip` (see
[song-packs.md](song-packs.md)), `roxmltree` (reading the songs; documents with a type
declaration are refused), `ureq` (the request), `sha2`, `serde` and `chrono`.
