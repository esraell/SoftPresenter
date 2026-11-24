#!/bin/sh
# Builds the FFmpeg that goes inside SoftPresenter.app: the ffmpeg and ffprobe programs, which
# play the videos macOS's own player can't (MKV, WebM, AVI and others).
#
# - LGPL: configured without --enable-gpl and without --enable-nonfree, from FFmpeg's own code
#   only (no outside libraries), so its licence is LGPL 2.1 or later.
# - Linked statically, needing nothing but macOS itself.
# - Only what SoftPresenter asks of it: every decoder and file format to read, but writing only
#   the raw pictures and sound it pipes to the app, and no network.
# - The source is downloaded once from ffmpeg.org and checked against its SHA-256 (the same as
#   Homebrew's for this release) before anything is built.
#
#   scripts/build-ffmpeg.sh
#
# The programs land in target/ffmpeg/bin, with FFmpeg's licence and a note of the exact source
# in target/ffmpeg; scripts/macos-app.sh puts them in the app. A second run does nothing.
#
#   scripts/build-ffmpeg.sh windows
#
# The same, cross-built on a Mac for 64-bit Windows with the mingw-w64 toolchain (Homebrew's
# x86_64-w64-mingw32-gcc): ffmpeg.exe and ffprobe.exe in target/ffmpeg-windows/bin, linked
# statically so they need nothing but Windows itself. They can decode on the graphics chip
# through Direct3D 11 or DXVA2 where the codec allows. FFmpeg's x86 assembly is used if nasm is
# installed, and left out (slower) if not. scripts/windows-package.sh puts the programs in the zip.
set -eu
cd "$(dirname "$0")/.."

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
URL="https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
# The oldest macOS the app runs on (LSMinimumSystemVersion in scripts/macos-app.sh).
MACOS=13.0

platform=${1:-macos}
case "$platform" in
macos)
    out=target/ffmpeg
    exe=""
    work=$PWD/target/ffmpeg-build
    ;;
windows)
    out=target/ffmpeg-windows
    exe=".exe"
    work=$PWD/target/ffmpeg-build-windows
    ;;
*)
    echo "usage: scripts/build-ffmpeg.sh [macos|windows]" >&2
    exit 2
    ;;
esac
if [ -x "$out/bin/ffmpeg$exe" ] && [ -x "$out/bin/ffprobe$exe" ] && [ "$(cat "$out/VERSION" 2>/dev/null)" = "$VERSION" ]; then
    echo "FFmpeg $VERSION is built in $out"
    exit 0
fi

mkdir -p "$work"
tarball="$work/ffmpeg-$VERSION.tar.xz"
if [ ! -f "$tarball" ]; then
    echo "downloading $URL"
    curl -fsSL -o "$tarball.part" "$URL"
    mv "$tarball.part" "$tarball"
fi
if ! echo "$SHA256  $tarball" | shasum -a 256 -c - >/dev/null 2>&1; then
    echo "error: $tarball doesn't match its SHA-256; it has been removed" >&2
    rm -f "$tarball"
    exit 1
fi

rm -rf "$work/src"
mkdir -p "$work/src"
tar -xJf "$tarball" -C "$work/src" --strip-components 1
cd "$work/src"
# The programs are taken from where they are built, not installed, so no folder of this
# computer is written into them (FFmpeg records how it was configured).
if [ "$platform" = macos ]; then
    ./configure \
        --enable-static --disable-shared \
        --disable-autodetect \
        --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-bzlib \
        --disable-network --disable-avdevice \
        --disable-programs --enable-ffmpeg --enable-ffprobe \
        --disable-doc --disable-debug \
        --disable-encoders --enable-encoder=rawvideo,pcm_f32le \
        --disable-muxers --enable-muxer=rawvideo,pcm_f32le \
        --disable-protocols --enable-protocol=file,pipe \
        --extra-cflags="-mmacosx-version-min=$MACOS" \
        --extra-ldflags="-mmacosx-version-min=$MACOS" \
        >"$work/configure.log"
else
    cross=x86_64-w64-mingw32-
    if ! command -v "${cross}gcc" >/dev/null; then
        echo "error: the mingw-w64 cross compiler (${cross}gcc) isn't installed (brew install mingw-w64)" >&2
        exit 1
    fi
    # FFmpeg's x86 assembly (its scaler and several decoders) needs nasm 2.13 or later. Without
    # nasm the build has none of it: it works the same, but scales and decodes in plain C, which
    # is several times slower in places.
    asm=--disable-x86asm
    if command -v nasm >/dev/null; then asm=--enable-x86asm; fi
    echo "x86 assembly: $asm"
    # The Windows hardware decoders (Direct3D 11 and DXVA2) need only Windows' own headers and
    # libraries. -static leaves out the toolchain's own DLLs. No zlib or bzip2: the toolchain
    # has none, and without them FFmpeg skips only PNG pictures inside videos and the few
    # Matroska files with compressed headers.
    ./configure \
        --enable-cross-compile --target-os=mingw32 --arch=x86_64 --cross-prefix="$cross" \
        --enable-static --disable-shared \
        --disable-autodetect "$asm" \
        --enable-d3d11va --enable-dxva2 \
        --disable-network --disable-avdevice \
        --disable-programs --enable-ffmpeg --enable-ffprobe \
        --disable-doc --disable-debug \
        --disable-encoders --enable-encoder=rawvideo,pcm_f32le \
        --disable-muxers --enable-muxer=rawvideo,pcm_f32le \
        --disable-protocols --enable-protocol=file,pipe \
        --extra-ldflags="-static" \
        >"$work/configure.log"
fi
make -j"$(sysctl -n hw.ncpu)" >"$work/make.log" 2>&1

cd - >/dev/null
rm -rf "$out"
mkdir -p "$out/bin"
for program in ffmpeg ffprobe; do
    cp "$work/src/$program$exe" "$out/bin/$program$exe"
    if [ "$platform" = macos ]; then
        strip "$out/bin/$program"
    else
        x86_64-w64-mingw32-strip "$out/bin/$program$exe"
    fi
done
if [ "$platform" = macos ]; then
    # A build that needs anything outside macOS (a Homebrew library, say) won't run elsewhere.
    if otool -L "$out/bin/ffmpeg" "$out/bin/ffprobe" | tail -n +2 | grep -v ':$' | grep -vE '^\s+(/usr/lib/|/System/Library/)'; then
        echo "error: the FFmpeg build needs libraries from outside macOS (above)" >&2
        exit 1
    fi
    configuration() { "$out/bin/ffmpeg" -hide_banner -buildconf; }
else
    # Only Windows' own DLLs may be needed: the toolchain's runtime DLLs wouldn't be there.
    dlls=$(x86_64-w64-mingw32-objdump -p "$out/bin/ffmpeg.exe" "$out/bin/ffprobe.exe" | sed -n 's/^.*DLL Name: //p' | sort -u)
    echo "needs: $(echo $dlls)"
    if echo "$dlls" | grep -iE 'pthread|libgcc|libstdc'; then
        echo "error: the FFmpeg build needs the toolchain's own libraries (above)" >&2
        exit 1
    fi
    # The programs can't run on this computer, so the configuration is read from the build's record.
    configuration() { sed -n 's/^#define FFMPEG_CONFIGURATION "\(.*\)"/\1/p' "$work/src/config.h" | sed 's/ --/\
  --/g'; }
fi
if configuration | grep -qE 'enable-(gpl|nonfree)'; then
    echo "error: the FFmpeg build isn't LGPL" >&2
    exit 1
fi
if strings "$out/bin/ffmpeg$exe" "$out/bin/ffprobe$exe" | grep -q "$HOME"; then
    echo "error: the FFmpeg build has a folder of this computer written into it" >&2
    exit 1
fi
cp "$work/src/COPYING.LGPLv2.1" "$out/COPYING.LGPLv2.1"
cp "$work/src/LICENSE.md" "$out/LICENSE.md"
cat >"$out/SOURCE.txt" <<NOTE
These copies of ffmpeg and ffprobe are FFmpeg $VERSION, licensed under the GNU Lesser General
Public License, version 2.1 or later (COPYING.LGPLv2.1, and LICENSE.md for the details).

They were built, unchanged, from the source FFmpeg publishes at
  $URL
(SHA-256 $SHA256), configured with:
$(configuration | sed -n 's/^ *--/  --/p')

FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project. SoftPresenter runs
these programs as separate programs; no FFmpeg code is built into SoftPresenter itself.
NOTE
echo "$VERSION" >"$out/VERSION"
echo "built FFmpeg $VERSION in $out: $(du -sh "$out/bin" | cut -f1)"
