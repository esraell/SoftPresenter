#!/bin/sh
# Builds the FFmpeg that goes inside SoftPresenter.app: the ffmpeg and ffprobe programs, which
# play the videos macOS's own player can't (MKV, WebM, AVI and others).
#
# - LGPL: configured without --enable-gpl and without --enable-nonfree, from FFmpeg's own code
#   only (no outside libraries), so its licence is LGPL 2.1 or later.
# - Linked statically, needing nothing but macOS itself.
# - For both kinds of Mac: built once for Apple silicon (arm64) and once for Intel (x86_64), each
#   for the oldest macOS the app runs on (MACOSX_DEPLOYMENT_TARGET in .cargo/config.toml), and
#   joined with lipo into programs that run on either. The Intel build uses FFmpeg's x86
#   assembly, which needs nasm (brew install nasm); without it the Intel programs are slower.
# - Only what SoftPresenter asks of it: every decoder and file format to read, but writing only
#   the raw pictures and sound it pipes to the app, and no network.
# - The source is downloaded once from ffmpeg.org and checked against its SHA-256 (the same as
#   Homebrew's for this release) before anything is built.
#
#   scripts/build-ffmpeg.sh
#
# The programs land in target/ffmpeg/bin, with FFmpeg's licence and a note of the exact source
# in target/ffmpeg; scripts/macos-app.sh puts them in the app. A second run does nothing, unless
# the programs there are an older build: not for both processors, or for another macOS.
#
#   scripts/build-ffmpeg.sh windows
#
# The same, cross-built on a Mac for 64-bit Windows with the mingw-w64 toolchain (Homebrew's
# x86_64-w64-mingw32-gcc): ffmpeg.exe and ffprobe.exe in target/ffmpeg-windows/bin, linked
# statically so they need nothing but Windows itself. They can decode on the graphics chip
# through Direct3D 11 or DXVA2 where the codec allows. FFmpeg's x86 assembly is used if nasm is
# installed, and left out (slower) if not. This build is published once, as the zip that
# crates/sp-app/src/ffmpeg_fetch.rs names, and scripts/windows-package.sh takes the programs from
# that zip (scripts/ffmpeg-windows-published.sh), the build the app checks for; it puts these in
# its package only with SP_FFMPEG_LOCAL=1.
#
#   scripts/build-ffmpeg.sh linux
#
# The same, built on Linux for Linux (scripts/linux-deb.sh builds it on Ubuntu 22.04 for the .deb):
# ffmpeg and ffprobe in target/ffmpeg-linux/bin, with FFmpeg's own code linked in and nothing
# needed but the C library, zlib and bzip2, which every Linux has, so a video plays the same
# whichever distribution's FFmpeg is installed, or none. FFmpeg's x86 assembly needs nasm here
# (sudo apt install nasm): a build for others to use must not be the slow one. No hardware
# decoding: SoftPresenter asks for none on Linux.
#
# JOBS=8 builds with 8 jobs instead of one for each processor.
set -eu
cd "$(dirname "$0")/.."

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
URL="https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
# The oldest macOS the app runs on, the one every Mac build is made for.
MACOS=$(sed -n 's/^MACOSX_DEPLOYMENT_TARGET = "\(.*\)"/\1/p' .cargo/config.toml)
if [ -z "$MACOS" ]; then
    echo "error: .cargo/config.toml doesn't say MACOSX_DEPLOYMENT_TARGET" >&2
    exit 1
fi
# The processors a Mac build is for: Apple silicon and Intel.
MAC_ARCHS="arm64 x86_64"

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
linux)
    out=target/ffmpeg-linux
    exe=""
    work=$PWD/target/ffmpeg-build-linux
    if [ "$(uname -s)" != Linux ]; then
        echo "error: the Linux FFmpeg is built on Linux" >&2
        exit 1
    fi
    ;;
*)
    echo "usage: scripts/build-ffmpeg.sh [macos|windows|linux]" >&2
    exit 2
    ;;
esac
# Whether the Mac programs in $out have a part for each processor, each made for $MACOS.
for_every_mac() {
    for program in ffmpeg ffprobe; do
        for arch in $MAC_ARCHS; do
            otool -arch "$arch" -l "$out/bin/$program" 2>/dev/null | grep -A4 LC_BUILD_VERSION | grep -qx " *minos $MACOS" || return 1
        done
    done
}
# Whether $out already has this version's programs (on a Mac, for every Mac).
built() {
    [ -x "$out/bin/ffmpeg$exe" ] && [ -x "$out/bin/ffprobe$exe" ] && [ "$(cat "$out/VERSION" 2>/dev/null)" = "$VERSION" ] || return 1
    [ "$platform" != macos ] || for_every_mac
}
if built; then
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
# shasum on a Mac; sha256sum on Linux, where shasum (Perl's) may not be installed.
if command -v sha256sum >/dev/null; then sha256=sha256sum; else sha256="shasum -a 256"; fi
if ! echo "$SHA256  $tarball" | $sha256 -c - >/dev/null 2>&1; then
    echo "error: $tarball doesn't match its SHA-256; it has been removed" >&2
    rm -f "$tarball"
    exit 1
fi

# Unpacks a fresh copy of the source into the folder named.
unpack() {
    rm -rf "$1"
    mkdir -p "$1"
    tar -xJf "$tarball" -C "$1" --strip-components 1
}
jobs=${JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu)}
# A Mac's two builds each have a folder of their own (below); the others are built in one.
if [ "$platform" != macos ]; then
    unpack "$work/src"
    cd "$work/src"
fi
# The programs are taken from where they are built, not installed, so no folder of this
# computer is written into them (FFmpeg records how it was configured).
if [ "$platform" = macos ]; then
    # One build for each processor; the one this Mac doesn't have is cross-compiled.
    for arch in $MAC_ARCHS; do
        unpack "$work/src-$arch"
        cross=
        if [ "$arch" != "$(uname -m)" ]; then cross=--enable-cross-compile; fi
        asm=
        if [ "$arch" = x86_64 ]; then
            if command -v nasm >/dev/null; then
                asm=--enable-x86asm
            else
                asm=--disable-x86asm
                echo "note: nasm isn't installed (brew install nasm), so the Intel FFmpeg is built without its x86 assembly: it plays the same videos, but scales and decodes several times slower in places" >&2
            fi
        fi
        (
            cd "$work/src-$arch"
            ./configure \
                $cross --arch="$arch" --target-os=darwin --cc="clang -arch $arch" \
                --enable-static --disable-shared \
                --disable-autodetect $asm \
                --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-bzlib \
                --disable-network --disable-avdevice \
                --disable-programs --enable-ffmpeg --enable-ffprobe \
                --disable-doc --disable-debug \
                --disable-encoders --enable-encoder=rawvideo,pcm_f32le \
                --disable-muxers --enable-muxer=rawvideo,pcm_f32le \
                --disable-protocols --enable-protocol=file,pipe \
                --extra-cflags="-mmacosx-version-min=$MACOS" \
                --extra-ldflags="-mmacosx-version-min=$MACOS" \
                >"$work/configure-$arch.log"
            make -j"$jobs" >"$work/make-$arch.log" 2>&1
        )
    done
elif [ "$platform" = linux ]; then
    if ! command -v nasm >/dev/null; then
        echo "error: nasm isn't installed (sudo apt install nasm); FFmpeg would be built without its x86 assembly" >&2
        exit 1
    fi
    # The same choices as the Mac's, without Apple's frameworks. zlib and bzip2 are the
    # system's own (zlib1g-dev and libbz2-dev to build), as on a Mac. Position-independent, and
    # its relocations read-only once loaded (RELRO, bound at start), as Debian builds its own.
    ./configure \
        --enable-static --disable-shared \
        --disable-autodetect --enable-x86asm --enable-pic \
        --enable-zlib --enable-bzlib \
        --disable-network --disable-avdevice \
        --disable-programs --enable-ffmpeg --enable-ffprobe \
        --disable-doc --disable-debug \
        --disable-encoders --enable-encoder=rawvideo,pcm_f32le \
        --disable-muxers --enable-muxer=rawvideo,pcm_f32le \
        --disable-protocols --enable-protocol=file,pipe \
        --extra-ldflags="-Wl,-z,relro -Wl,-z,now" \
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
if [ "$platform" != macos ]; then
    make -j"$jobs" >"$work/make.log" 2>&1
    cd - >/dev/null
fi

rm -rf "$out"
mkdir -p "$out/bin"
for program in ffmpeg ffprobe; do
    if [ "$platform" = macos ]; then
        # Each processor's program, stripped, then the two joined into one.
        set --
        for arch in $MAC_ARCHS; do
            strip -o "$work/$program-$arch" "$work/src-$arch/$program"
            set -- "$@" "$work/$program-$arch"
        done
        lipo -create "$@" -output "$out/bin/$program"
        continue
    fi
    cp "$work/src/$program$exe" "$out/bin/$program$exe"
    if [ "$platform" = windows ]; then
        x86_64-w64-mingw32-strip "$out/bin/$program$exe"
    else
        strip "$out/bin/$program"
    fi
done
# Where the build was made, and how, for SOURCE.txt (a Mac's says so for both of its builds).
source="$work/src"
described() {
    echo "(SHA-256 $SHA256), configured with:"
    configuration | sed -n 's/^ *--/  --/p'
}
if [ "$platform" = macos ]; then
    # A build that needs anything outside macOS (a Homebrew library, say) won't run elsewhere.
    if otool -arch all -L "$out/bin/ffmpeg" "$out/bin/ffprobe" | grep -v ':$' | grep -vE '^\s+(/usr/lib/|/System/Library/)'; then
        echo "error: the FFmpeg build needs libraries from outside macOS (above)" >&2
        exit 1
    fi
    if ! for_every_mac; then
        echo "error: the FFmpeg programs aren't for both processors ($MAC_ARCHS), each for macOS $MACOS" >&2
        exit 1
    fi
    # How each build was configured, one option a line, read from its record.
    recorded() { sed -n 's/^#define FFMPEG_CONFIGURATION "\(.*\)"/\1/p' "$work/src-$1/config.h" | sed 's/ --/\
  --/g'; }
    configuration() { for arch in $MAC_ARCHS; do recorded "$arch"; done; }
    source="$work/src-arm64"
    described() {
        echo "(SHA-256 $SHA256), once for each kind of Mac,"
        echo "each for macOS $MACOS and later, and joined into programs that run on both. For Apple silicon"
        echo "(arm64) they were configured with:"
        recorded arm64 | sed -n 's/^ *--/  --/p'
        echo "and for Intel (x86_64) with:"
        recorded x86_64 | sed -n 's/^ *--/  --/p'
    }
elif [ "$platform" = linux ]; then
    # Only libraries every Linux has: the C library's own, zlib and bzip2.
    needed=$(objdump -p "$out/bin/ffmpeg" "$out/bin/ffprobe" | sed -n 's/^ *NEEDED *//p' | sort -u)
    echo "needs: $(echo $needed)"
    if echo "$needed" | grep -vE '^(libc|libm|libpthread|libdl|libz|libbz2)\.so\.'; then
        echo "error: the FFmpeg build needs other libraries (above)" >&2
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
# -a: every part of a program (of a universal one, both processors' parts).
if strings -a "$out/bin/ffmpeg$exe" "$out/bin/ffprobe$exe" | grep -q "$HOME"; then
    echo "error: the FFmpeg build has a folder of this computer written into it" >&2
    exit 1
fi
cp "$source/COPYING.LGPLv2.1" "$out/COPYING.LGPLv2.1"
cp "$source/LICENSE.md" "$out/LICENSE.md"
cat >"$out/SOURCE.txt" <<NOTE
These copies of ffmpeg and ffprobe are FFmpeg $VERSION, licensed under the GNU Lesser General
Public License, version 2.1 or later (COPYING.LGPLv2.1, and LICENSE.md for the details).

They were built, unchanged, from the source FFmpeg publishes at
  $URL
$(described)

FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project. SoftPresenter runs
these programs as separate programs; no FFmpeg code is built into SoftPresenter itself.
NOTE
echo "$VERSION" >"$out/VERSION"
echo "built FFmpeg $VERSION in $out: $(du -sh "$out/bin" | cut -f1)"
