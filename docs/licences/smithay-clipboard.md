# smithay-clipboard: the changed copy in `third_party/smithay-clipboard`

On a Linux Wayland desktop, copy and paste in SoftPresenter's windows go through
smithay-clipboard (<https://github.com/Smithay/smithay-clipboard>), which the interface toolkit
(egui-winit) brings with it. smithay-clipboard 0.7.3 is kept in this repository with changes,
in [`third_party/smithay-clipboard`](../../third_party/smithay-clipboard), and built in place
of the published one (`[patch.crates-io]` in the workspace's `Cargo.toml`). It is built for
Linux only.

## The licence

smithay-clipboard is copyright Lucas Timmins and Victor Berger and licensed under the MIT
licence, which can be combined with the GNU GPL version 3. The copy keeps the licence file
(`LICENSE`), and each change is marked in the source. SoftPresenter's changes to that folder
are offered under the same licence, so they can go back to smithay-clipboard.

It ships as part of the app on Linux like the rest of the code, so
[NOTICE.md](../../NOTICE.md) names it.

## What was changed, and why

[`third_party/smithay-clipboard/PROVENANCE.md`](../../third_party/smithay-clipboard/PROVENANCE.md)
has the exact source and the list of changes. In short: files dragged onto the window are told
to the app, on a Wayland desktop, through the same "data device" the clipboard uses, and GNOME
keeps one of those for a program. The copy has every window's clipboard share one, and lets
SoftPresenter answer drags through it, so files dragged from the file manager come in and the
clipboard keeps working in every window.

It adds no library: the copy uses what the published crate uses.
