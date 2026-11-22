# Where this copy of Fontique comes from, and what was changed

This folder is the `fontique` crate, version 0.13.0, as published on crates.io
(<https://crates.io/crates/fontique/0.13.0>), built from commit
`32a137efbb228ad0b1c6a46b979dfb55d4584eae` of <https://github.com/linebender/parley>, where it
lives in `fontique`. `.cargo_vcs_info.json` is the record crates.io ships with it. Fontique finds
the computer's fonts and picks the one each letter is drawn in, for Parley, which lays out
SoftPresenter's slide text.

Fontique is copyright the Parley Authors and licensed under Apache-2.0 OR MIT; both licences are
here as `LICENSE-APACHE` and `LICENSE-MIT`, unchanged. SoftPresenter's changes are offered under
the same two licences, so the folder stays usable on Fontique's own terms.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. Parley and every other crate are the published ones, unchanged.

## Why

Fontique 0.13 maps every font file it hands to the shaper and the renderer
(`memmap2::Mmap::map` in `SourceCache`'s `load_blob`, and in `FontInfo::from_source`). A mapped
file that another program cuts short, or writes over in place, while it is mapped kills the
process with a bus error (SIGBUS) the next time a page past the file's new end is read. Safe code
can neither prevent that nor catch it.

That happened to SoftPresenter in use: another program rewrote an installed font file, shorter,
while the app had it mapped, and some minutes later the app was killed in the middle of shaping
a slide's text (in pair kerning, reading a part of the font not read before). A presentation
app is often left running for a whole service with fonts in use, while font managers, system
updates and font installers rewrite font files.

There is no option in 0.13 to read instead of map: `SourceCache` always maps a font that comes
from a file. Registering the fonts SoftPresenter uses from data it reads itself
(`Collection::register_fonts`) was looked at and not taken: the fonts Fontique falls back on for
other alphabets, the generic families, alternative family names and the last-resort fonts all
come from the computer's own collection, by file, and would still be mapped, or would have to be
done again outside Fontique, with a risk of slides looking different.

## What was changed

Everything is marked `SoftPresenter:` in the source. Removed from the published crate:
`Cargo.lock`, `Cargo.toml.orig` and `.cargo-ok`, which only crates.io's packaging uses.

- **`src/source_cache.rs`:**
  - `load_blob` reads the file into memory of its own (`std::fs::read`) where it was mapped.
    Only the fonts the shaper asks for are loaded, as before; each now costs its file's size
    in memory, where mapped pages could be dropped by the system. Measured on a Mac
    (2026-11-21): a song in three families and the Bible template's came to about 7 MB, read
    once for every thread. But a slide with one emoji reads Apple Color Emoji whole (188 MB),
    and a sign the theme's font lacks (a cross, ✝) can lead the shaper on to a large fallback
    font (STHeiti Medium, 54 MB); SoftPresenter keeps each until it quits.
  - New: the type `SourceLoader` and `SourceCache::set_loader`. A cache given a loader has it
    read font files in place of reading them itself, and a loader may say no (`None`), which the
    cache remembers as a failure until `prune` is asked to forget failures, as before.
    SoftPresenter uses it to keep one copy of each font file for every thread, to check the data
    is still a font before it is used, and to refuse to read on the thread that draws the
    screens (a loader thread reads the file meanwhile).
- **`src/font.rs`:** `FontInfo::from_source` reads a font file where it mapped it. The
  DirectWrite (Windows) and fontconfig (Linux) backends call it the first time a family is
  looked up.
- **`src/scan.rs`:** not changed, only commented. Looking for the computer's fonts still maps
  each file while its names and a few small tables are read, and lets the mapping go before the
  next file: there are about a gigabyte of font files on a Mac, too many to read. Nothing kept
  from a scan holds any of the file's data. A file cut short during the moment it is being
  scanned can still kill the process; SoftPresenter scans once, as it starts, on a thread that
  doesn't draw the screens.
- **`src/lib.rs`:** exports `SourceLoader`.

Changing this copy removes two of upstream's `unsafe` blocks (the two maps above) and adds none.
The crate keeps upstream's other `unsafe` code: the scan's map, and the calls into Core Text,
DirectWrite and fontconfig that look for the computer's fonts.

## Checking it

`cargo test -p sp-render --lib fonts` runs SoftPresenter's tests of this: text is laid out and
drawn in a copy of a font file; the copy is then cut short, written over, or taken away, and the
same text, with letters and kerning pairs not used before, is laid out and drawn again, in a
process of its own, which must neither be killed nor draw anything different. With the published
crate, cutting the file short or writing over it kills that process with SIGBUS.

## Updating

To move to a newer Fontique, check first whether it can read font files instead of mapping them
and take a loader (then this copy can go). If not, copy the new crate here and carry the
changes above across.
