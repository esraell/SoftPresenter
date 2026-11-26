# FFmpeg: how SoftPresenter uses it, and what that asks

A working note, not legal advice. Read FFmpeg's own licence pages
(<https://ffmpeg.org/legal.html>) before publishing a build that includes FFmpeg.

## How it is used

SoftPresenter plays the videos macOS's own player can't (MKV, WebM, AVI and the like, or a
codec a Mac lacks) by running the `ffmpeg` and `ffprobe` programs as separate processes and
reading the decoded pictures and sound from them through pipes (`crates/sp-video/src/ffmpeg.rs`).
No FFmpeg code is compiled into or linked with SoftPresenter, and nothing from FFmpeg is in
this repository.

## In the app

The Mac app carries its own FFmpeg: `ffmpeg` and `ffprobe` in `SoftPresenter.app/Contents/MacOS`,
with FFmpeg's licence (`COPYING.LGPLv2.1`, `LICENSE.md`) and a note of the exact source and
configuration (`SOURCE.txt`) in `Contents/Resources/FFmpeg`. The app looks for FFmpeg there
first, then in `/opt/homebrew/bin`, `/usr/local/bin` and `/usr/bin`.

`scripts/build-ffmpeg.sh` makes it, and `scripts/macos-app.sh` runs it:

- **The source:** FFmpeg 9.0.2 as FFmpeg publishes it,
  <https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz>, checked against its SHA-256 (the same one
  Homebrew records for that file) before it is built. Nothing in it is changed.
- **LGPL 2.1 or later:** configured without `--enable-gpl`, `--enable-version3` or
  `--enable-nonfree`, and with `--disable-autodetect`, so no outside library (GPL or otherwise)
  is picked up. The script refuses a build whose configuration says `gpl` or `nonfree`.
- **Nothing but macOS:** linked statically; the only libraries it uses are macOS's own (the
  system library, zlib, bzip2, and the VideoToolbox, AudioToolbox and Core Media frameworks).
  The script refuses a build that needs anything else, or that has a folder of the building
  computer written into it.
- **Only what SoftPresenter asks of it:** every decoder, file format and parser FFmpeg has, for
  reading; for writing, only raw pictures and raw 32-bit float sound, which it pipes to the
  app; only the `file` and `pipe` protocols, and no network.
- **For every Mac:** built twice from the same source with the same options, for Apple silicon
  (arm64) and for Intel (x86_64, with FFmpeg's x86 assembly when `nasm` is installed), each for
  macOS 11 and later, and joined with `lipo` into programs that run on both. `SOURCE.txt` gives
  both configurations. A newer macOS call FFmpeg makes (one, in macOS 12) is checked for as it
  runs, by FFmpeg itself.

Decoding H.264, HEVC, VP8, VP9, MPEG-1, MPEG-2, MPEG-4, WMV, ProRes, AAC, MP3, Opus, Vorbis,
AC-3, WMA and the rest is FFmpeg's own code. AV1 is decoded by FFmpeg through the Mac's video
hardware, which M3 and later Macs have; an older Mac, and every Intel Mac, would need the dav1d
decoder (BSD licence) added to the build.

What the LGPL asks, and how the app meets it:

- **The licence goes with the programs:** `Contents/Resources/FFmpeg/COPYING.LGPLv2.1`.
- **The source is available:** `SOURCE.txt` names the exact file, its checksum and the
  configuration; FFmpeg's own site keeps its releases. A public release should also keep a copy
  of that file with the release, or offer it in writing, in case the link ever goes.
- **It can be replaced:** the programs are separate files, run as separate processes; anyone can
  put their own build of `ffmpeg` and `ffprobe` in their place.
- **No FFmpeg code is in SoftPresenter itself**, so SoftPresenter's licence is its own.

## In the Windows package

`scripts/windows-package.sh` builds the same FFmpeg 9.0.2 for 64-bit Windows with
`scripts/build-ffmpeg.sh windows`, cross-compiling on a Mac with the mingw-w64 toolchain, from
the same source file and checksum. `ffmpeg.exe` and `ffprobe.exe` sit beside `SoftPresenter.exe`,
which looks there first, and the zip's `Licences` folder holds `COPYING.LGPLv2.1`,
`FFmpeg-LICENSE.md` and `FFmpeg-SOURCE.txt` (the exact source, checksum and configuration).

- **LGPL 2.1 or later,** as on the Mac: no `--enable-gpl`, `--enable-version3` or
  `--enable-nonfree`, `--disable-autodetect`, and the script refuses a build that says otherwise.
- **Needs nothing but Windows:** linked statically (`-static`), so the only DLLs are Windows' own
  (the Universal C Runtime, `KERNEL32`, `bcrypt` and a few more). The script refuses a build that
  needs the toolchain's own DLLs.
- **Hardware decoding:** Direct3D 11 and DXVA2 decoders for H.264, HEVC, VP9 and AV1 are built
  in; they use Windows' own headers only.
- **Without nasm** (not installed on the building Mac) the build has no hand-written x86
  assembly (`--disable-x86asm`): it works the same, but scales and decodes more slowly. With
  nasm installed the script uses it.
- **No zlib or bzip2** (the toolchain has neither, and `--disable-autodetect` leaves outside
  libraries out), so the few Matroska files with compressed headers, and PNG pictures inside a
  video, don't play. Ordinary video and sound don't need them.

## In the Linux package

`scripts/linux-deb.sh` builds the same FFmpeg 9.0.2 on Ubuntu 22.04 with
`scripts/build-ffmpeg.sh linux`, from the same source file and checksum, and puts `ffmpeg` and
`ffprobe` beside the program in `/usr/lib/softpresenter`, which the app looks in first (before
`/usr/bin`, so a distribution's own FFmpeg, often a GPL build, is never the one used). The licence
and the note of the source are in `/usr/share/doc/softpresenter`: the machine-readable
`copyright` file (which names `/usr/share/common-licenses/LGPL-2.1`, Debian's copy of the
licence), `FFmpeg-LICENSE.md` and `FFmpeg-SOURCE.txt` (the exact source, checksum and
configuration).

- **LGPL 2.1 or later,** as on the Mac: no `--enable-gpl`, `--enable-version3` or
  `--enable-nonfree`, `--disable-autodetect`, and the script refuses a build that says otherwise.
- **Needs nothing but what every Linux has:** FFmpeg's own libraries are built into the two
  programs; they need only the C library, zlib and bzip2, from the system. The script refuses a
  build that needs anything else, and the package's script one that needs a newer C library than
  Ubuntu 22.04's.
- **With its x86 assembly** (`--enable-x86asm`; the script refuses to build without nasm), and
  position-independent with read-only relocations, as Debian builds its own.
- **No hardware decoding:** SoftPresenter asks for none on Linux; FFmpeg decodes on the
  processor.

## Rules

- Never copy Homebrew's FFmpeg into the app: it is a GPL build (with x264, x265 and others) and
  depends on Homebrew's own libraries.
- Adding an outside library to the build (dav1d, say) means reading its licence first, adding it
  here, and keeping the build LGPL.
