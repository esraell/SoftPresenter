# Free songbooks shared by churches

A working note, not legal advice.

SoftPresenter offers fifteen songbooks for download (Czech, English, German, Russian, Slovak and
Ukrainian; the list, with each file's size and SHA-256, is `crates/sp-app/src/free_songbooks.rs`).
They were put together by many people over many years and are shared free on a public page, for
any church to use. The owner decided on 25 November 2026 to offer them as the Bibles are offered:
"the songbooks are free and they were put together by many people over time, so there is nobody
to ask".

## Fetched from their publisher, not redistributed

**SoftPresenter does not contain, ship or host any of these songbooks, or any of their words.**
Nothing from them is in this repository: the tests make their own songbooks with words written
for them. Each church's copy of the app downloads a songbook only when someone there chooses
Install (or ticks it in the setup and goes on), straight from the site of the page that shares
them, on that computer. The address of each file is in the code; nothing else in the app or its
documentation names the site.

- The page links most of them as zipped files of an older kind, which the songbook import doesn't
  read; the same site lists the same songbooks, in the kind the import reads, for programs that
  download songbooks, and those are the files offered. They answer over HTTPS.
- Three Czech songbooks the page lists (Bratrské písně BJB, České chvály, Dětské písně) are not
  offered: their links lead nowhere, and the site has them under no other name.
- A file is installed only when it is byte for byte the one measured on 25 November 2026 (its
  size and SHA-256). If the site changes a file, the app refuses it and says so, and a church that
  trusts a copy it has can bring it in with Import Songs › Song Files…. A new version of the app,
  with the new file measured, offers the change.

## Whose songs they are

The songs belong to their writers. The window says so under the list: a church's licence (such as
CCLI) covers showing them in its services, as for any song. Where a songbook gives a song's
writers, they are kept as the song's details and shown in the copyright line as any song's are.

## What the app does with the site

- A plain `GET` of one file, over HTTPS only (redirects too), asked for as it is
  (`Accept-Encoding: identity`) so the length is known: nothing about the church, the library or
  the computer is sent. A test copy can be pointed at a local server instead
  (`SOFTPRESENTER_SONGBOOK_URL`); the size and checksum are checked all the same.
- The file is untrusted until checked: a length other than the one expected is refused before the
  body is read, nothing past the expected size is ever read (and never more than 16 MB), and the
  checksum is checked before anything is written. It is then read by the songbook import, read
  only, from a file in a folder of its own in the data folder, which is deleted once read.
- Its songs come into the library as File › Import Songs brings a songbook's, each marked with the
  songbook it came from, so Remove can take back out the songs nobody changed and nothing else
  uses.

## Libraries

No new library: `ureq` (the request), `sha2` (the checksum) and the songbook import already in the
app.
