# DeckLink: what the licence asks of SoftPresenter

A summary, in our own words, of the End User License Agreement for Blackmagic Design's DeckLink
SDK and of the licence statement in its header files, read on 2026-11-21 for SDK version 16.0.
This is a working note, not legal advice. Read the agreement itself before publishing or
selling anything that includes DeckLink support.

## Three kinds of file, three sets of terms

- **The header files** in the SDK's `Mac/include`, `Linux/include` and `Win/include` folders
  (`DeckLinkAPI.h` and its companions, and `DeckLinkAPIDispatch.cpp`). The agreement's clause
  0.1 says its clauses 1 (the grant and its limits), 4.3 and 4.4 (no copying, changing or
  passing on), 5, 7 and 8 do not apply to them. What does apply is the statement at the top of
  each file: anyone may use, copy, change, distribute and sublicense them, as long as the
  copyright notice and that whole statement stay in every copy (a compiled program needs
  neither), and they come with no warranty. Blackmagic keeps its rights in them (clause 4.2),
  gives no warranty (clause 2) and limits its liability (clause 3). So the headers SoftPresenter
  needs may be in its repository: they are in `third_party/decklink-sdk`, copied unchanged with
  their notices, and `PROVENANCE.md` there lists them with checksums.
- **The sample code** (the SDK's `Examples` and samples). Clause 1.7: it may be used only
  internally, to learn how to write software that works with Blackmagic's products. None of it
  is in SoftPresenter: the bridge and everything else were written for SoftPresenter, from the
  headers and the SDK's manual.
- **Everything else in the SDK** (the manual, the tools). The whole agreement applies: it may be
  used to make software compatible with Blackmagic's products, and passed on only in full under
  terms like Blackmagic's own (clause 1.4). None of it is in SoftPresenter.

## The driver

Desktop Video, the driver that comes with the cards and runs them, is not part of the SDK and
not part of SoftPresenter. The person using SoftPresenter installs it, from Blackmagic Design;
SoftPresenter opens it while running (on a Mac `/Library/Frameworks/DeckLinkAPI.framework`, on
Linux `libDeckLinkAPI.so`) and is not linked with it. Building SoftPresenter needs nothing from
Blackmagic but the headers already in the repository. Without the driver there are no DeckLink
outputs to choose, and everything else works. The additional permission in
[`ADDITIONAL-PERMISSION.md`](../../ADDITIONAL-PERMISSION.md) covers combining SoftPresenter with
the driver and the SDK.

## Names

Clause 6: "DeckLink" and Blackmagic's other names may not be used in SoftPresenter's own name or
in a way that suggests it is Blackmagic's. While the agreement lasts it allows "SoftPresenter
compatible with Blackmagic Design DeckLink" and "SoftPresenter for Blackmagic Design DeckLink".
Inside the app a device is called what the driver calls it ("DeckLink Duo (1)"), and the
outputs are described in plain words ("a DeckLink output"), never as Blackmagic's feature.
DeckLink is a trademark of Blackmagic Design Pty. Ltd.

## How SoftPresenter uses it

- `crates/sp-decklink/bridge/bridge.cpp`, SoftPresenter's own C++, compiled with the headers by
  the `cc` crate on a Mac and on Linux, gives the Rust side a handful of plain C functions:
  open the driver from a path, list the devices and their modes, be told when a device comes
  or goes, and play frames out with the card's clock. `crates/sp-decklink/src/ffi.rs` is the
  only Rust module that calls them, and the only one with `unsafe` code.
- The bridge looks up the driver's entry points by the names SDK 16.0's
  `DeckLinkAPIDispatch.cpp` uses; that file itself is not used or copied, because it opens the
  driver once, from one fixed place, and never looks again.
- On Windows the DeckLink interface comes as an IDL file that Microsoft's MIDL compiler turns
  into headers, so for now SoftPresenter is built there without DeckLink and offers none.
