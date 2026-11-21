# Fontique: the changed copy in `third_party/fontique`

SoftPresenter lays out slide text with Parley, and Parley finds the computer's fonts and picks
the one each letter is drawn in with Fontique (<https://github.com/linebender/parley>, in its
`fontique` folder). Parley comes from crates.io like any other library. Fontique 0.13.0 is kept
in this repository with changes, in [`third_party/fontique`](../../third_party/fontique), and
built in place of the published one (`[patch.crates-io]` in the workspace's `Cargo.toml`).

## The licence

Fontique is copyright the Parley Authors and licensed under Apache-2.0 OR MIT, the user's choice.
Both can be combined with the GNU GPL version 3. The copy keeps both licence files
(`LICENSE-APACHE`, `LICENSE-MIT`), every file keeps its copyright line, and each change is
marked in the source, as the Apache licence asks of changed files. SoftPresenter's changes to
that folder are offered under the same two licences, so they can go back to Fontique.

It ships as part of the app like the rest of the code, so [NOTICE.md](../../NOTICE.md) names it.

## What was changed, and why

[`third_party/fontique/PROVENANCE.md`](../../third_party/fontique/PROVENANCE.md) has the exact
source and the list of changes. In short: Fontique 0.13 maps font files into memory, and a font
file that another program rewrites while it is mapped kills the app with a bus error. The copy
reads font files instead, and lets SoftPresenter read them itself: once for every thread, and
never on the thread that draws the screens.
