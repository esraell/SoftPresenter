---
name: preparing-a-service
description: Prepare a church service in SoftPresenter from a set list - find the songs or make them from lyrics (in one language or two), add Bible readings, headers and placeholders to a playlist in the order of the service, and check it before the service. Use when someone asks to prepare Sunday, make or fill a playlist, add songs or readings, or turn lyrics into a song.
---

# Preparing a service in SoftPresenter

SoftPresenter is the church's presentation app: it puts song words, Bible verses, pictures and
videos on the screens the congregation sees. A service is prepared as a **playlist**: its items
in the order of the service. Preparing never changes what is on the screens; it only changes the
library and the playlists, which the operator sees in SoftPresenter's window.

## Before you start

1. Call `get_status`. If something is on the screens, a service or a rehearsal may be running:
   say so, keep to preparing, and never touch the screens unless the person asks you to.
2. If a tool says AI assistants are switched off, tell the person where to switch them on
   (SoftPresenter's Settings › AI assistants, Prepare) and stop.

## The order of work

1. **The playlist.** Look at `list_playlists` first: the playlist may be there already. Make a
   new one with `create_playlist`, named the way the church names them ("Sunday 24 November").
2. **Each song.** Find it with `search_songs`, using a few words of its title or its first line.
   From then on name it by the **id** the search gives, never by a number in a list (numbers
   change as songs are added). Several songs can share a title: pick by id, and if you can't
   tell which is meant, ask.
3. **A song that isn't in the library.** Make it with `create_song` from the words the person
   gives you or sends you. Never write lyrics from memory or fill in lines you aren't given: ask
   for the words. Only songs the church may show belong on its screens.
4. **Fill the playlist** with `add_to_playlist`, in the order of the service. The items go at the
   end, in the order you give them:
   - `{"header": "Worship"}` groups the items after it ("Worship", "The Word", "Sermon",
     "Response"); headers make a long playlist easy to follow.
   - `{"song": "<id>"}` for each song. A playlist holds a song once.
   - `{"bible": "John 3:16-18"}` for a reading.
   - `{"media": "Countdown.mp4"}` for a picture or video already in the media bin
     (`list_media` lists them).
   - `{"placeholder": "Sermon slides"}` keeps a place for something that comes later.
5. **Check it.** `read_playlist` and read it back to the person in a short list. Point out
   anything missing (a song you couldn't find, a reading not yet chosen).

## Songs in two languages

Many churches show each slide in two languages: the main language on top, its translation
under it, in the theme's translation box. In `create_song`, give each slide's `lines` in the
main language and its `translation` with the same number of lines in the second language, line
for line. Leave `translation` out for a song in one language.

- **Slides:** two lines each is usual, four at most; a long verse becomes several slides of the
  same section, in order.
- **Sections** are named as the church names them: Verse 1 to Verse 6, Chorus (or Chorus 1 to 3),
  Bridge, Pre-chorus, Tag, Intro, Ending. These names get jump keys in SoftPresenter (C for the
  chorus), so use them rather than inventing others.
- **A chorus sung after every verse** is written out again each time it is sung, so the slides
  run in the order of singing.
- Leave out singing directions ("x2", "Repeat", "(Men)") and chords.
- Check the title isn't taken (`search_songs`) before making a song; a new title that is the
  same as an old one makes two songs that are hard to tell apart.

## Bible readings

- A reference is typed as on SoftPresenter's Bible page: `John 3:16-18`, `Psalm 23` (a whole
  chapter), `1 Corinthians 13:4-7`. Russian and Ukrainian names work too (`Ин 3:16`, `Пс 22`).
- English and Russian Bibles number some chapters differently (the Psalms most of all): type
  the reference in the language whose numbering it comes from.
- `look_up_bible` shows the verses as they will be shown, in the translations the Bible page
  uses (often two). Check a reading with it before adding it, and tell the person if the verses
  aren't what they expect.
- `list_bibles` shows the Bibles installed and those that can be downloaded; `install_bible`
  downloads one of those offered (it takes a little while).

## What you can't do here

Deleting songs or playlists, importing files, changing themes, settings or the screens' setup
are done by the person at the computer: say so plainly. Taking an item out of a playlist asks the
person first, and SoftPresenter's Undo puts it back.
