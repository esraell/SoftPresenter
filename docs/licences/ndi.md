# NDI: what the licence asks of SoftPresenter

A summary, in our own words, of the NDI SDK License Agreement that comes with the SDK
(`/Library/NDI SDK for Apple/NDI SDK License Agreement.pdf`), read on 2026-11-19 for SDK
version 6. This is a working note, not legal advice. Read the agreement itself before
publishing or selling anything that includes NDI.

## What we may do

- Use the SDK, free of royalties, to build software that runs on ordinary computers
  (desktops, laptops, servers the user controls). SoftPresenter is that kind of software.
  Fixed-purpose appliances, locked-down cloud services and embedded devices are excluded and
  need a commercial licence from NDI.
- Distribute the parts of the SDK that its documentation marks as redistributable (the NDI
  runtime), and only in the way that documentation says.

## Receiving

NDI sources in slides are received with the same runtime, opened the same way as for sending
(the user's own NDI Tools install; nothing from the SDK is in this repository). The few
receiving and finding functions are declared in `sp-ndi`'s `runtime` module following
`Processing.NDI.Find.h` and `Processing.NDI.Recv.h`, under the same MIT notice as the sending
ones. The app asks for the picture only, never audio, and shows it as it arrives.

## What we must not do

- Put any other SDK file in this repository or in a download: no headers, examples, tools or
  documentation. The agreement treats the SDK as confidential. The source here contains no
  SDK file: only declarations of the few functions it calls, following the headers, which
  carry their own MIT licence (see below).
- Modify, reverse engineer, decompile or work around the limits of the SDK or NDI's
  protocols, or let anyone else do so.
- Ship a version that is not compatible with other NDI products, or that falls behind: the
  agreement asks products to use the latest SDK available when they are built or distributed.
- Pass on NDI-enabled builds without NDI's agreement, for now. Clause 3(d)(viii) asks that
  anyone who builds on the product be required to follow NDI's SDK licence and to put the
  terms of clause 3(d) in their own licence. SoftPresenter's own code is under the GNU GPL,
  which forbids adding conditions like that to it, so the GPL can't carry that requirement.
  Until NDI agrees in writing to how SoftPresenter handles this (the additional permission
  and `NOTICE.md`), public releases are built without NDI.

## What a published SoftPresenter must carry

- **Licence terms for users.** Whatever licence SoftPresenter is distributed under has to
  include terms that: forbid modifying or reverse engineering the NDI parts and working around
  their limits; forbid removing NDI's notices; disclaim warranties and liability on NDI's
  behalf; follow United States export law; and name NDI as the copyright owner of its part.
  An open-source licence alone does not say these things, so they have to be added for the
  NDI part before publishing a build with NDI in it.
- **Trademark notes.** "NDI" may be used only to say the product is compatible with NDI
  products, with a clear note that NDI® is a registered trademark of Vizrt NDI AB. Nothing may
  suggest NDI sponsors or endorses SoftPresenter. The SDK documentation also asks for a link
  to <https://ndi.video/> wherever NDI is offered (the Screens window has one) and for the
  trademark line in the About box (not there yet; needed before a release).
- **How SoftPresenter carries these terms.** They are in [`NOTICE.md`](../../NOTICE.md),
  limited to NDI's own components, and the additional permission in
  [`ADDITIONAL-PERMISSION.md`](../../ADDITIONAL-PERMISSION.md) requires every NDI-enabled copy
  to carry them. They place no limits on what the GNU GPL allows for SoftPresenter's code.
- **Third-party notices.** The runtime's own licence list
  (`libndi_licenses.txt`, in the SDK's `lib` folder) goes beside the runtime wherever the
  runtime is shipped.

## Other points

- The agreement is governed by the laws of Sweden.
- NDI may test a product and end the licence if it does not work to their satisfaction; either
  side may end it with thirty days' notice.
- Patented codecs (H.264, H.265, AAC) are a separate matter: the SDK documentation says the
  product's maker is responsible for licensing them. SoftPresenter sends uncompressed frames
  to the SDK and uses none of those itself.

## How SoftPresenter uses it

- Building SoftPresenter needs nothing from NDI. `crates/sp-ndi/src/runtime.rs` declares the
  handful of functions and structures it calls, following the SDK's headers
  (`Processing.NDI.Lib.h`, `Processing.NDI.structs.h`, `Processing.NDI.Send.h`). Those
  headers are under the MIT licence, unlike the rest of the SDK, and their notice is kept in
  that file. No header or other SDK file is in the repository.
- The app is not linked with the NDI runtime. When an NDI screen is turned on, it looks for
  the runtime that NDI Tools (or the SDK) installed, the way the SDK's own "dynamic loading"
  header describes (the folder `NDI_RUNTIME_DIR_V6` names, then `/usr/local/lib`, then the
  SDK's copy), and on a Mac with NDI Tools but not NDI's runtime installer, the copy NDI ships
  inside its own apps (NDI Virtual Input, NDI Scan Converter; never another company's app),
  and opens it on that screen's own thread. Without it, the screen says NDI
  isn't installed and links to https://ndi.video/, and the rest of the app is unaffected.
  Nothing from NDI is copied into the app.
- The two examples in `crates/sp-ndi/examples` (which look for and receive NDI sources, to
  check SoftPresenter's own) use a third-party NDI library that needs the SDK; they build
  only with `--features sdk-examples`, never as part of the app or its tests.
