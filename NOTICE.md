# Notices

## SoftPresenter

Copyright (C) 2026 Esrael Nate and the SoftPresenter contributors.

SoftPresenter is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version, with the additional permission in
[ADDITIONAL-PERMISSION.md](ADDITIONAL-PERMISSION.md). It is distributed in the hope that it will
be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License in [LICENSE](LICENSE) for
more details.

### The SoftPresenter name

The GNU GPL gives no rights to the name "SoftPresenter" (GPL section 7(e)). If you pass on a
changed version, give it a name of its own. Saying plainly what it is, such as "based on
SoftPresenter", is fine.

## Parts that are not SoftPresenter's own

### NDI®

The NDI runtime library and the NDI SDK are not part of SoftPresenter and are not covered by
the GNU GPL. SoftPresenter never ships them: the person using it installs NDI themselves, and
SoftPresenter opens it while running. They are copyright Vizrt NDI AB and its licensors (NDI's
SDK licence names the copyright owner as NDI, Inc.) and are licensed on Vizrt NDI AB's own
terms. For those NDI components only, and for nothing else in SoftPresenter:

- do not modify, reverse engineer, disassemble or recompile them, or the protocols they use,
  and do not attempt to; do not get around their technical limits;
- do not remove, obscure or alter their proprietary notices or labels;
- Vizrt NDI AB and its licensors give no warranty, and, as far as the law allows, accept no
  liability for any damages, direct, indirect, incidental or consequential, arising from the
  use of this product;
- follow the export laws and regulations of the United States.

Vizrt NDI AB's own terms ask anyone who builds software with the NDI SDK to accept its licence.
That is between them and Vizrt NDI AB; it places no condition on SoftPresenter's own code, which
stays under the GNU GPL. NDI® is a registered trademark of Vizrt NDI AB. NDI Tools, which
installs the runtime, is free from <https://ndi.video/>.

The declarations in `crates/sp-ndi/src/runtime.rs` follow the NDI SDK's header files, which
carry this licence:

> Copyright (C) 2023-2026 Vizrt NDI AB. All rights reserved.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the "Software"), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
> BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
> DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Blackmagic Design DeckLink

SoftPresenter can play its screens out of Blackmagic Design DeckLink cards (SoftPresenter for
Blackmagic Design DeckLink). The DeckLink driver, Desktop Video, is not part of SoftPresenter
and is not covered by the GNU GPL: the person using SoftPresenter installs it from Blackmagic
Design, and SoftPresenter opens it while running; it is never shipped with SoftPresenter or
linked with it. DeckLink is a trademark of Blackmagic Design Pty. Ltd., which has nothing to do
with SoftPresenter.

SoftPresenter is compiled with header files from Blackmagic Design's DeckLink SDK, version
16.0, copied unchanged into `third_party/decklink-sdk` (`PROVENANCE.md` there lists them). No
other file of the SDK, and none of its sample code, is in SoftPresenter. The headers carry this
notice, which stays in every copy of them:

> Copyright (c) 2026 Blackmagic Design
>
> Permission is hereby granted, free of charge, to any person or organization obtaining a copy
> of the software and accompanying documentation (the "Software") to use, reproduce, display,
> distribute, sub-license, execute, and transmit the Software, and to prepare derivative works
> of the Software, and to permit third-parties to whom the Software is furnished to do so, in
> accordance with:
>
> (1) if the Software is obtained from Blackmagic Design, the End User License Agreement for
> the Software Development Kit ("EULA") available at
> https://www.blackmagicdesign.com/EULA/DeckLinkSDK; or
>
> (2) if the Software is obtained from any third party, such licensing terms as notified by
> that third party,
>
> and all subject to the following:
>
> (3) the copyright notices in the Software and this entire statement, including the above
> license grant, this restriction and the following disclaimer, must be included in all copies
> of the Software, in whole or in part, and all derivative works of the Software, unless such
> copies or derivative works are solely in the form of machine-executable object code generated
> by a source language processor.
>
> (4) THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
> INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
> PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT SHALL THE COPYRIGHT HOLDERS OR ANYONE
> DISTRIBUTING THE SOFTWARE BE LIABLE FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT,
> TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
> OTHER DEALINGS IN THE SOFTWARE.
>
> A copy of the Software is available free of charge at
> https://www.blackmagicdesign.com/desktopvideo_sdk under the EULA.

The SDK's End User License Agreement leaves these header files out of its limits on copying and
passing on (its clause 0.1). [`docs/licences/decklink.md`](docs/licences/decklink.md) explains.

### Fonts built into the app

The interface toolkit (egui) builds these fonts into the app. Each keeps its own licence:

- **Hack:** copyright Source Foundry Authors, MIT licence; based on Bitstream Vera Sans Mono,
  copyright Bitstream Inc., under the Bitstream Vera licence (reserved font names "Bitstream"
  and "Vera"), and DejaVu (public domain).
- **Ubuntu Light:** Ubuntu Font Licence 1.0.
- **Noto Emoji:** SIL Open Font Licence 1.1.
- **emoji-icon-font** (and egui's small subset of it): copyright John Slegers, MIT licence.

### Verse-numbering tables

`crates/sp-bible/data/*.vrs` are copied unchanged from SIL's libpalaso and built into the app:

> Copyright (c) 2007-2026 SIL Global
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the "Software"), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
> BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
> DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Bibles

The Bible texts that come with SoftPresenter, and those it offers to download, are not under
the GNU GPL. Each keeps its own terms: public domain, or an open licence that lets anyone pass
it on. [`docs/licences/bibles.md`](docs/licences/bibles.md) lists each one and any conditions
it carries.

### FFmpeg

SoftPresenter plays some videos by running FFmpeg as a separate program. The Mac app, the Windows
package and the Linux package include FFmpeg 9.0.2, built unchanged from FFmpeg's source, under
the GNU Lesser General Public License, version 2.1 or later; its licence and the exact source it
was built from are in the Mac app's `Contents/Resources/FFmpeg`, the Windows package's `Licences`
folder and the Linux package's `/usr/share/doc/softpresenter`. No FFmpeg code is in or linked with SoftPresenter itself.
FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project.
[`docs/licences/ffmpeg.md`](docs/licences/ffmpeg.md) explains.

### Showing an iPhone, iPad or Mac screen

SoftPresenter can show the screen of a device that picks it in Screen Mirroring (AirPlay). This
is unofficial: it is not made, licensed, certified or supported by Apple, and an Apple update
may stop it working. AirPlay, iPhone, iPad, Mac and Apple TV are trademarks of Apple Inc.
SoftPresenter is not made, licensed or endorsed by Apple.

It is done by a separate program, `softpresenter-mirror`, which SoftPresenter starts and talks
to through pipes; none of its code is in or linked with SoftPresenter itself, and it uses
neither NDI nor DeckLink. `softpresenter-mirror` is under the GNU General Public License,
version 3 or later, **without** the additional permission in
[ADDITIONAL-PERMISSION.md](ADDITIONAL-PERMISSION.md): that permission does not extend to it. It
is written in Rust following these works, whose notices it keeps:

- UxPlay's `lib/` ([github.com/FDH2/UxPlay](https://github.com/FDH2/UxPlay), which descends
  from shairplay, AirplayServer and RPiPlay): copyright (C) 2011-2012, 2018 Juho Vähä-Herttua;
  (c) 2019 dsafa22; (C) 2020 Jaslo Ziska; 2014 Joakim Plate; Florian Draschbacher; (C) 2026 Omar
  Roth; modified 2021-2026 by F. Duncanh. GNU Lesser General Public License, version 2.1 or
  later ([`docs/licences/LGPL-2.1.txt`](docs/licences/LGPL-2.1.txt)).
- playfair ([github.com/EstebanKubata/playfair](https://github.com/EstebanKubata/playfair),
  2015, as carried in UxPlay's `lib/playfair/`): GNU General Public License, version 3
  ([LICENSE](LICENSE)). The helper's FairPlay module is a port of it.

[`docs/licences/screen-mirroring.md`](docs/licences/screen-mirroring.md) says which part follows
which file, and lists the libraries the helper is built with.

### Reading ProPresenter 7 song documents

SoftPresenter's importer for `.pro` song documents uses the field numbers from greyshirtguy's
unofficial ProPresenter7-Proto definitions
([github.com/greyshirtguy/ProPresenter7-Proto](https://github.com/greyshirtguy/ProPresenter7-Proto)),
under this licence. ProPresenter is a trademark of Renewed Vision, LLC, which has nothing to do
with SoftPresenter; [`docs/licences/song-documents.md`](docs/licences/song-documents.md) explains.

> MIT License
>
> Copyright (c) 2023 greyshirtguy
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the "Software"), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
> BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
> DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Vello

SoftPresenter draws with Vello (<https://github.com/linebender/vello>), copyright the Vello
Authors, licensed under Apache-2.0 OR MIT. Its `vello` crate, version 0.12.0, is built from the
copy in `third_party/vello`, which SoftPresenter has changed: the copy keeps Vello's licence
files, marks each change, and says what was changed and why in its `PROVENANCE.md`.
[`docs/licences/vello.md`](docs/licences/vello.md) explains.

### Fontique

SoftPresenter finds fonts with Fontique (<https://github.com/linebender/parley>), copyright the
Parley Authors, licensed under Apache-2.0 OR MIT. Its `fontique` crate, version 0.13.0, is built
from the copy in `third_party/fontique`, which SoftPresenter has changed: the copy keeps
Fontique's licence files, marks each change, and says what was changed and why in its
`PROVENANCE.md`. [`docs/licences/fontique.md`](docs/licences/fontique.md) explains.

### smithay-clipboard

On Linux, SoftPresenter's clipboard on Wayland desktops is smithay-clipboard
(<https://github.com/Smithay/smithay-clipboard>), copyright Lucas Timmins and Victor Berger,
licensed under the MIT licence. Its `smithay-clipboard` crate, version 0.7.3, is built from the
copy in `third_party/smithay-clipboard`, which SoftPresenter has changed: the copy keeps
smithay-clipboard's licence file, marks each change, and says what was changed and why in its
`PROVENANCE.md`. [`docs/licences/smithay-clipboard.md`](docs/licences/smithay-clipboard.md)
explains.

### SoftPresenter Sans

SoftPresenter sets slides in SoftPresenter Sans, a typeface built into the program: Libertinus
Sans 7.051 under another name, copyright 2012-2024 the Libertinus Project Authors, with SoftPresenter's
Bold Italic added, licensed under the SIL Open Font License 1.1. The licence and the authors
are kept beside the font files in `crates/sp-render/fonts` and in the Mac app's Resources.
[`docs/licences/softpresenter-sans.md`](docs/licences/softpresenter-sans.md) explains.

### Sample backgrounds

SoftPresenter carries seven sample slide backgrounds, five pictures, a stand-in for a camera shot
and a looping video, in `assets/backgrounds` and, in the Mac app, in `Contents/Resources/Backgrounds`.
The project drew them with its own script, `assets/backgrounds/make_backgrounds.py`, and released
them and the script under CC0 1.0 Universal (public domain).
[`docs/licences/backgrounds.md`](docs/licences/backgrounds.md) explains.

### Logos in Settings

The logos on Settings' cards for the services and assistants SoftPresenter connects with are
trademarks of their owners, used only to identify the apps SoftPresenter connects with. They are
not covered by the GNU GPL, their permission can't be passed on, and they are removed on request.
They are kept as their companies provide them in `assets/logos/third-party`, apart from the rest:
the Canva icon (for Canva Sync, under canva.dev's brand guidelines) and Grok's logomark (under
xAI's brand guidelines, implying no endorsement); and Claude's spark (Anthropic), OpenAI's
Blossom (for ChatGPT) and Worship Leader App's note, which are used without the owner's approval.
VoxLive's and TimelyChurch's are the owner's own. [`docs/licences/logos.md`](docs/licences/logos.md)
gives each one's source, date and rules.

### Song catalog

Song catalog: Worship Leader App, [worshipleaderapp.com](https://worshipleaderapp.com). The
songs are the source's, not SoftPresenter's, and none comes with the app: a church downloads
the languages it chooses from the source's own server
([docs/licences/song-catalog.md](docs/licences/song-catalog.md)).

### Other libraries

SoftPresenter is built from many open-source Rust libraries, each under its own licence (MIT,
Apache 2.0, MPL 2.0, BSD, ISC, Zlib and similar), all of which can be combined with the GNU GPL
version 3. `cargo metadata` lists each library and its licence.
