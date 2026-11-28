#!/bin/sh
# Gets the project's own FFmpeg for Windows: the build the app checks for, as the project's
# release publishes it.
#
#   scripts/ffmpeg-windows-published.sh
#
# On Windows the app keeps its own copy of FFmpeg, and copies the one beside it only when the two
# programs have the SHA-256s it was built with (crates/sp-app/src/ffmpeg_fetch.rs); any other it
# leaves alone and downloads that zip instead. So the Windows package carries exactly that build.
# This downloads the zip ffmpeg_fetch.rs names (URL), checks its size and SHA-256 (SIZE, SHA256)
# before anything is unpacked, takes out only the two programs and FFmpeg's licence and note of
# its source, and checks the programs' SHA-256s (FFMPEG_SHA256, FFPROBE_SHA256). They land in
# target/ffmpeg-windows-published: bin/ffmpeg.exe, bin/ffprobe.exe, COPYING.LGPLv2.1, LICENSE.md
# and SOURCE.txt. The zip is kept in target/ffmpeg-windows-download and checked again on every
# run, so a second run needs no network. Anything that doesn't match stops it, with nothing left
# in target/ffmpeg-windows-published.
#
# scripts/windows-package.sh runs it, and so does the release workflow.
set -eu
cd "$(dirname "$0")/.."

src=crates/sp-app/src/ffmpeg_fetch.rs
download=target/ffmpeg-windows-download
out=target/ffmpeg-windows-published

fail() {
    if [ "${GITHUB_ACTIONS:-}" = true ]; then echo "::error title=FFmpeg for Windows::$*"; fi
    echo "error: $*" >&2
    exit 1
}

url=$(grep -oE 'https://github\.com/[^"]+\.zip' "$src" | head -1)
size=$(sed -n 's/^pub const SIZE: u64 = \([0-9_]*\);.*/\1/p' "$src" | tr -d _)
zip_sha=$(sed -n 's/^pub const SHA256: &str = "\([0-9a-f]*\)";.*/\1/p' "$src")
ffmpeg_sha=$(sed -n 's/^pub const FFMPEG_SHA256: &str = "\([0-9a-f]*\)";.*/\1/p' "$src")
ffprobe_sha=$(sed -n 's/^pub const FFPROBE_SHA256: &str = "\([0-9a-f]*\)";.*/\1/p' "$src")
for value in "$url" "$size" "$zip_sha" "$ffmpeg_sha" "$ffprobe_sha"; do
    if [ -z "$value" ]; then fail "couldn't read FFmpeg's address, size or SHA-256s from $src"; fi
done

sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
zip="$download/$(basename "$url")"
# Whether the zip kept here is the one the app expects, to the byte.
zip_matches() {
    [ -f "$zip" ] && [ "$(wc -c <"$zip" | tr -d ' ')" = "$size" ] && [ "$(sha256 "$zip")" = "$zip_sha" ]
}
mkdir -p "$download"
if ! zip_matches; then
    rm -f "$zip" "$zip.part"
    echo "downloading $url"
    curl -fsSL -o "$zip.part" "$url" || fail "couldn't download $url"
    mv "$zip.part" "$zip"
    if ! zip_matches; then
        have_size=$(wc -c <"$zip" | tr -d ' ')
        have_sha=$(sha256 "$zip")
        rm -f "$zip"
        fail "$url is $have_size bytes with SHA-256 $have_sha; expected $size bytes, $zip_sha"
    fi
fi

# Only the five files the app itself takes from the zip, by their exact names.
rm -rf "$out" "$out.part" "$download/unpacked"
unzip -q "$zip" ffmpeg/ffmpeg.exe ffmpeg/ffprobe.exe ffmpeg/COPYING.LGPLv2.1 ffmpeg/LICENSE.md ffmpeg/SOURCE.txt \
    -d "$download/unpacked" || fail "$zip doesn't hold FFmpeg's programs, licence and note of its source"
mkdir -p "$out.part/bin"
cp "$download/unpacked/ffmpeg/ffmpeg.exe" "$download/unpacked/ffmpeg/ffprobe.exe" "$out.part/bin/"
chmod 755 "$out.part/bin/ffmpeg.exe" "$out.part/bin/ffprobe.exe"
cp "$download/unpacked/ffmpeg/COPYING.LGPLv2.1" "$download/unpacked/ffmpeg/LICENSE.md" "$download/unpacked/ffmpeg/SOURCE.txt" "$out.part/"
rm -rf "$download/unpacked"
for pair in "ffmpeg.exe=$ffmpeg_sha" "ffprobe.exe=$ffprobe_sha"; do
    have=$(sha256 "$out.part/bin/${pair%%=*}")
    if [ "$have" != "${pair#*=}" ]; then
        fail "${pair%%=*} has SHA-256 $have; the app expects ${pair#*=}"
    fi
done
mv "$out.part" "$out"
echo "FFmpeg for Windows, as published: $url"
