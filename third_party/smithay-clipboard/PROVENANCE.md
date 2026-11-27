# Where this copy of smithay-clipboard comes from, and what was changed

This folder is the `smithay-clipboard` crate, version 0.7.3, as published on crates.io
(<https://crates.io/crates/smithay-clipboard/0.7.3>), built from commit
`26c2f53f15f6bdc4f41a442d0ae2c2d63bbc617c` of <https://github.com/Smithay/smithay-clipboard>.
`.cargo_vcs_info.json` is the record crates.io ships with it. It is the clipboard the interface
toolkit (egui-winit) runs for each of SoftPresenter's windows on a Linux Wayland desktop.

smithay-clipboard is copyright Lucas Timmins and Victor Berger and licensed under the MIT
licence, here as `LICENSE`, unchanged. SoftPresenter's changes are offered under the same
licence, so they can go back to smithay-clipboard.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. Its dependencies are the published ones, at the versions the app already
had.

## Why

On a Wayland desktop the window library (winit 0.30) says nothing of files dragged onto a
window, so SoftPresenter has to hear of them itself. Both a clipboard and a drag's destination
work through a `wl_data_device` for the seat, and GNOME's compositor (Mutter 50.1, read in its
`src/wayland/meta-wayland-data-device.c`) keeps **one** data device for a program:

- A drag is told to one device only, the first of the program's it finds
  (`wl_resource_find_for_client`).
- A device made while the program doesn't have the keyboard takes the place of the one it had
  (`get_data_device` takes the earlier one off its list), and the earlier one hears of nothing
  again: not of drags, and not of what is on the clipboard.

smithay-clipboard 0.7.3 makes a thread, a queue and a data device for every `Clipboard`, and
egui-winit makes a `Clipboard` for every window. So a listener of SoftPresenter's own, with a
data device of its own, heard of drags only until the next window opened (seen: drops on the
operator's window were lost once Settings had been opened). And with only the published crate,
opening a second window from outside (a command, the remote) leaves the first window's
clipboard deaf, and closing it leaves the app with no data device at all.

The cure for both is the same: one data device for the app, which serves the clipboard of every
window and answers drags.

Upstream has neither: on 2026-11-26 its `master` still made a worker for each `Clipboard` and
ignored the four drag events.

## What was changed

Everything is marked `SoftPresenter:` in the source. Removed from the published crate:
`Cargo.lock`, `Cargo.toml.orig`, `.cargo-ok`, `.github`, `.gitignore` and `doc_index.html`,
which only its own repository and crates.io's packaging use; and `examples/clipboard.rs` with
the `[[example]]` and `[dev-dependencies]` entries of `Cargo.toml` that it alone used (Cargo
tests a crate outside the workspace only if it has no dev-dependencies).

- **`src/lib.rs`: one worker for every `Clipboard` of a display.** `Clipboard` is now a handle
  to a shared `Worker` (the thread, the queue, the channels), found through a static weak
  reference and made by the first `Clipboard::new` for a display; the worker stops, and its
  thread is waited for, when the last `Clipboard` is dropped (what `Drop for Clipboard` did for
  each). A request and its reply are made under a lock, as two windows now share the reply
  channel. `Clipboard`'s public interface and its safety rule are unchanged. No `unsafe` was
  added: the one call that joins the display's connection is the published one, moved.
  A worker whose thread has ended by itself (the compositor can't be spoken to any more, or
  something on the thread panicked) is not shared again: the next `Clipboard::new` starts
  another (`Worker::serves`, with a unit test, `cargo test -p smithay-clipboard --lib` on
  Linux). The `Clipboard`s made before keep the ended one, as a published `Clipboard` whose
  thread ended does.
- **`src/dnd.rs` (new, SoftPresenter's own): being a drag's destination.** A program registers
  one `DragDestination` for the process (`set_destination`). The worker tells it what happens
  to a drag over any of the program's surfaces (entered, with the surface's address, the
  pointer's place and the kinds of data offered; moved; left; dropped; data read) and makes
  the `Request`s it returns: accept as a copy of a kind of data, refuse, ask for the data (read
  on a short-lived thread of its own, at most 1 MiB, never on the worker, as a source may never
  finish writing), keep a dropped offer, and finish or discard it. `finish` is sent only for a
  drop whose copy action the compositor agreed, since anything else is a protocol error. With
  no destination registered, drags are ignored as before.
- **`src/state.rs`:** the four drag events of `DataDeviceHandler` and `selected_action` of
  `DataOfferHandler`, which did nothing, call into `dnd`; `State` holds `dnd`'s two fields.
- **`src/worker.rs`:** `Command::DragRead`, by which a reading thread hands the worker what it
  read, and the sending end of the worker's own channel passed in for it.

## Limits to know of

- **Drags are heard only while the seat has a keyboard.** The published crate makes its data
  device when the seat gains a keyboard and lets go of it when the keyboard goes ("selection
  sources are tied to the keyboard"), and that is unchanged, so on a seat with a pointer or a
  touch screen and no keyboard nothing dragged is heard of. A desk has one; a headless
  compositor has none until a key is first pressed (the check script gives it one).
- **Each `Peek` and each `Take` starts a thread** to read the pipe, and SoftPresenter peeks
  once whenever a drag of files comes over its window (to show where they would land). The
  thread ends when the source closes the pipe or after `MOST_READ` bytes; a source that
  never closes its pipe leaves one small thread and one descriptor waiting until the app
  quits. They are never waited for, so they can't hold up Quit.
- One destination for the whole process, and no way to be a drag's source.
- **One drag at a time.** The drag over a surface is kept in one place (`Drags::over` in
  `src/dnd.rs`), and motion, leave and drop don't look at which data device spoke, so two
  seats dragging over the program's windows at once would cross. Two seats were not built
  for.
- **A worker that ended is replaced for new windows only.** A `Clipboard` made before its
  worker's thread ended keeps the ended one: in that window what is copied is dropped and a
  paste says "clipboard is dead" until the window is opened again. A window opened afterwards
  gets a fresh worker, and drags are heard again from then. Mending the earlier windows
  would mean joining the display again from inside a request, which is more `unsafe` use;
  not done.
- **The list's reader, in `crates/sp-wayland/src/uri_list.rs`, has two small gaps:** lines
  that end in a carriage return alone are not split, and a `?query` or `#fragment` written
  without percent signs stays in the file's path. No file manager seen sends either.

## What uses it

`crates/sp-wayland` registers the destination for the operator's window: it takes a drag of
`text/uri-list` onto that window's surface and tells the app of the files.

## Checking

`crates/sp-wayland`'s tests cover the order of a drag and the list's reader, and this crate's
one test the choice of a worker. The rest of the carried code needs a compositor:
`scripts/wayland-drop-check.sh` (in the repository's root) checks it with real drags and real
copy and paste in a private headless GNOME Shell, with no one at the desk: drops of files, drops
before, while and after a second window of the app is open, and copy and paste both ways in
two windows. It passed on Ubuntu 26.04 (GNOME Shell 50.1) when this copy was added.

## Updating

To move to a newer smithay-clipboard, check first whether it shares one data device between
its clipboards and lets a program answer drags (then this copy can go, or shrink). If not,
copy the new crate here and carry the changes above across. egui-winit names the version it
wants; the copy must be that version. Then run `scripts/wayland-drop-check.sh` on a Linux
computer with GNOME, and again after any update of egui or winit: they decide how many
`Clipboard`s there are and whose connection they join.
