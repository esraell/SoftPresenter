#!/bin/sh
# Builds the Windows package from nothing, on a Mac: target/windows-package/SoftPresenter-Windows.zip.
#
#   scripts/windows-package.sh
#
# It cross-builds the program for 64-bit Windows (the mingw-w64 toolchain, Homebrew's
# x86_64-w64-mingw32-gcc, and the Rust target x86_64-pc-windows-gnu), takes the project's own
# LGPL FFmpeg for Windows as its release publishes it (scripts/ffmpeg-windows-published.sh), and
# gathers them with the sample backgrounds, the licences and a READ ME into a folder that unzips
# anywhere and runs from where it is. Nothing is installed on the computer building it. The
# program is built with this computer's home folder shortened to "~", and the script stops if
# any file of the package still has it.
#
# The FFmpeg must be exactly that build: the app copies the FFmpeg beside it only when the two
# programs have the SHA-256s it was built with (crates/sp-app/src/ffmpeg_fetch.rs), and otherwise
# downloads it at its first start. The script stops if the package's programs have any other.
#
#   SP_FFMPEG_LOCAL=1 scripts/windows-package.sh
#
# builds FFmpeg here instead (scripts/build-ffmpeg.sh windows), for when the published zip can't
# be reached. That build has other SHA-256s, so the app doesn't use it, and the script says so.
set -eu
cd "$(dirname "$0")/.."

# The version the READ ME names: a release's (SOFTPRESENTER_RELEASE_VERSION, which the build
# reads too), or the one in Cargo.toml.
version=${SOFTPRESENTER_RELEASE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
target=x86_64-pc-windows-gnu
root=target/windows-package
folder=$root/SoftPresenter-Windows

# FFmpeg first, so a zip that can't be had stops the build in seconds, not after the program.
if [ "${SP_FFMPEG_LOCAL:-}" = 1 ]; then
    scripts/build-ffmpeg.sh windows
    ffmpeg=target/ffmpeg-windows
else
    if ! scripts/ffmpeg-windows-published.sh; then
        echo "error: the project's FFmpeg for Windows couldn't be had (above). SP_FFMPEG_LOCAL=1 builds one here instead, which the app won't use: it downloads its own at its first start." >&2
        exit 1
    fi
    ffmpeg=target/ffmpeg-windows-published
fi

# The program is built in a folder of its own outside the home folder. The import libraries the
# linker makes for Windows' DLLs are named after the folder they are made in, and those names stay
# in the program ("_head__Users_<name>_…"), where --remap-path-prefix doesn't reach them. The
# folder must be this user's own (made private here), or the build stops. Each checkout has its
# own (named by a hash of where it is, which says nothing of the path), as two must never share one.
build=${SOFTPRESENTER_WINDOWS_BUILD:-/private/tmp/softpresenter-windows-build-$(id -u)-$(pwd | shasum | cut -c1-8)}
mkdir -p "$build"
chmod 700 "$build"
if [ "$(stat -f %u "$build")" != "$(id -u)" ]; then
    echo "error: $build belongs to another user; set SOFTPRESENTER_WINDOWS_BUILD to a folder of your own" >&2
    exit 1
fi
RUSTFLAGS="--remap-path-prefix=$HOME=~" CARGO_TARGET_DIR="$build" cargo build --release --target "$target" -p sp-app

rm -rf "$folder" "$root/SoftPresenter-Windows.zip"
mkdir -p "$folder/Backgrounds" "$folder/Licences"
# The program's own name for itself isn't relied on; it is called what people will see.
cp "$build/$target/release/softpresenter.exe" "$folder/SoftPresenter.exe"
cp "$ffmpeg/bin/ffmpeg.exe" "$ffmpeg/bin/ffprobe.exe" "$folder/"
cp assets/backgrounds/*.jpg assets/backgrounds/*.mp4 "$folder/Backgrounds/"

# The licences: the app's own, the notices, and those of what ships beside it.
cp LICENSE "$folder/Licences/SoftPresenter-GPL-3.0.txt"
cp NOTICE.md ADDITIONAL-PERMISSION.md "$folder/Licences/"
cp "$ffmpeg/COPYING.LGPLv2.1" "$folder/Licences/COPYING.LGPLv2.1"
cp "$ffmpeg/LICENSE.md" "$folder/Licences/FFmpeg-LICENSE.md"
cp "$ffmpeg/SOURCE.txt" "$folder/Licences/FFmpeg-SOURCE.txt"
cp crates/sp-render/fonts/OFL.txt "$folder/Licences/SoftPresenterSans-OFL.txt"
cp crates/sp-render/fonts/AUTHORS.txt "$folder/Licences/SoftPresenterSans-AUTHORS.txt"
cp assets/backgrounds/README.md "$folder/Licences/Backgrounds-CC0.md"

cat >"$folder/READ ME.txt" <<TEXT
SoftPresenter $version for Windows
==================================

What you need: Windows 10 or 11, 64-bit.

To start
--------
1. Unzip the whole SoftPresenter-Windows folder anywhere you like (the Desktop is fine).
   Keep the files in it together: SoftPresenter.exe, ffmpeg.exe and ffprobe.exe (they play
   videos) and the Backgrounds folder.
2. Open the folder and double-click SoftPresenter.exe.

"Windows protected your PC"
---------------------------
SoftPresenter isn't signed yet, so the first time Windows may show a blue box saying
"Windows protected your PC". Click "More info", then "Run anyway". Windows asks only once
for each copy of the program.

NDI (sending slides to other computers on the network)
------------------------------------------------------
SoftPresenter uses the NDI runtime that comes with NDI Tools (version 6 or 5), which is
free from https://ndi.video. Install NDI Tools first if you want to send a screen over NDI.
Without it, everything else works and the NDI choices say it isn't installed.

The Windows firewall
--------------------
The first time you switch on NDI output or Remote Control, Windows Firewall asks whether to
allow SoftPresenter. Choose "Allow" for private networks. NDI, and phones on the church
network, can't reach SoftPresenter without it.

Where your things are kept
--------------------------
Songs, Bibles, settings and everything you make are kept in your own folder:
  %APPDATA%\\SoftPresenter
To open it, press the Windows key and R together, type  %APPDATA%\\SoftPresenter  and press
Enter. To move to another computer, or to keep a copy, copy that folder. Deleting the
SoftPresenter-Windows folder doesn't delete it.

If something goes wrong
-----------------------
SoftPresenter writes a log of what happens, in the same folder:
  %APPDATA%\\SoftPresenter\\SoftPresenter.log
Send that file with a few words about what you were doing and the time it happened. It
stays on your computer; nothing is sent anywhere unless you send it.

Licences
--------
SoftPresenter is free software under the GNU General Public License. The Licences folder has
that licence and the notices for the parts that come with it: FFmpeg (LGPL, with a note of
its exact source), the SoftPresenter Sans typeface (Open Font Licence) and the sample
backgrounds (CC0).
TEXT
# Windows' Notepad shows line ends properly with either, but older tools want CR LF.
sed 's/$/\r/' "$folder/READ ME.txt" >"$folder/READ ME.crlf"
mv "$folder/READ ME.crlf" "$folder/READ ME.txt"

# A package that has this computer's home folder written into it must not leave: as a path, or
# with its slashes made underscores, as names made from a path are.
home_named=$(printf %s "$HOME" | tr / _)
for file in SoftPresenter.exe ffmpeg.exe ffprobe.exe; do
    count=$(strings "$folder/$file" | grep -c -e "$HOME" -e "$home_named" || true)
    echo "home folder in $file: $count"
    if [ "$count" != 0 ]; then
        echo "error: $file has a folder of this computer written into it" >&2
        exit 1
    fi
done

# The FFmpeg beside the program must be the app's own, or the app never uses it (above).
src=crates/sp-app/src/ffmpeg_fetch.rs
ffmpeg_sha=$(sed -n 's/^pub const FFMPEG_SHA256: &str = "\([0-9a-f]*\)";.*/\1/p' "$src")
ffprobe_sha=$(sed -n 's/^pub const FFPROBE_SHA256: &str = "\([0-9a-f]*\)";.*/\1/p' "$src")
for pair in "ffmpeg.exe=$ffmpeg_sha" "ffprobe.exe=$ffprobe_sha"; do
    file=${pair%%=*}
    have=$(shasum -a 256 "$folder/$file" | cut -d' ' -f1)
    if [ -n "${pair#*=}" ] && [ "$have" = "${pair#*=}" ]; then
        echo "$file: the app's own FFmpeg (SHA-256 $have)"
    elif [ "${SP_FFMPEG_LOCAL:-}" = 1 ]; then
        echo "warning: $file was built here (SP_FFMPEG_LOCAL=1), with SHA-256 $have, not the app's own (${pair#*=}). SoftPresenter won't use it: at its first start on each computer it downloads its own (23 MB, over the internet)." >&2
    else
        echo "error: $file in the package has SHA-256 $have; the app expects ${pair#*=} ($src)" >&2
        exit 1
    fi
done

(cd "$root" && zip -qr -X SoftPresenter-Windows.zip SoftPresenter-Windows)
echo "built $root/SoftPresenter-Windows.zip: $(du -h "$root/SoftPresenter-Windows.zip" | cut -f1)"
