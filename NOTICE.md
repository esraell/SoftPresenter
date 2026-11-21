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

SoftPresenter does not use DeckLink yet. When it does, the Desktop Video drivers and DeckLink
SDK will be installed by the person using it, never shipped with SoftPresenter, in the same way
as NDI, and their notices will be added here from the DeckLink SDK's licence.

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

SoftPresenter plays some videos by running FFmpeg as a separate program. The Mac app includes
FFmpeg 9.0.2, built unchanged from FFmpeg's source, under the GNU Lesser General Public License,
version 2.1 or later; its licence and the exact source it was built from are in the app's
`Contents/Resources/FFmpeg`. No FFmpeg code is in or linked with SoftPresenter itself.
FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project.
[`docs/licences/ffmpeg.md`](docs/licences/ffmpeg.md) explains.

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

### Other libraries

SoftPresenter is built from many open-source Rust libraries, each under its own licence (MIT,
Apache 2.0, MPL 2.0, BSD, ISC, Zlib and similar), all of which can be combined with the GNU GPL
version 3. `cargo metadata` lists each library and its licence.
