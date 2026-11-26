#!/bin/sh
# Builds SoftPresenter and wraps it as a Mac app, target/release/SoftPresenter.app, so the
# menu bar, the Dock and Activity Monitor call it "SoftPresenter".
#
#   scripts/macos-app.sh                for this Mac's own processor only: the quickest, to try a change
#   scripts/macos-app.sh --universal    for Apple silicon and Intel Macs both, as a build to share must be
#   open target/release/SoftPresenter.app
#
# A universal build compiles everything twice, the second time for the other processor (its Rust
# target must be installed: rustup target add x86_64-apple-darwin, or aarch64-apple-darwin on an
# Intel Mac), and joins each program's two parts with lipo. A build with a version to share
# (SOFTPRESENTER_RELEASE_VERSION or SOFTPRESENTER_DEV_VERSION set) is universal without being
# asked; --native makes it this Mac's own only. Any other arguments are passed to cargo.
set -eu
cd "$(dirname "$0")/.."

universal=
if [ -n "${SOFTPRESENTER_RELEASE_VERSION:-}${SOFTPRESENTER_DEV_VERSION:-}" ]; then universal=1; fi
for arg in "$@"; do
    shift
    case "$arg" in
    --universal) universal=1 ;;
    --native) universal= ;;
    *) set -- "$@" "$arg" ;;
    esac
done
# The oldest macOS the app runs on (.cargo/config.toml): every program in the app is made for
# it, and the app says it in LSMinimumSystemVersion.
macos=$(sed -n 's/^MACOSX_DEPLOYMENT_TARGET = "\(.*\)"/\1/p' .cargo/config.toml)
if [ -z "$macos" ]; then
    echo "error: .cargo/config.toml doesn't say MACOSX_DEPLOYMENT_TARGET" >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET="$macos"
# The other processor's Rust target, for a universal build; this Mac's own is built as usual, into
# target/release.
host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
aarch64-apple-darwin) other=x86_64-apple-darwin ;;
*) other=aarch64-apple-darwin ;;
esac
if [ -n "$universal" ] && ! rustup target list --installed 2>/dev/null | grep -qx "$other"; then
    echo "error: a universal build needs Rust's $other target: rustup target add $other" >&2
    exit 1
fi
# The paths of the source and of the libraries are written into the program (for messages about
# where it stopped); the builder's home folder is shortened to ~ in them, so a build that is shared
# says nothing about whose computer made it.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
cargo build --release -p sp-app "$@"
# The program that receives an iPhone's, iPad's or Mac's screen (Screen Mirroring): a separate
# program under its own licence, started by the app only while that is switched on.
cargo build --release -p sp-mirror "$@"
if [ -n "$universal" ]; then
    cargo build --release --target "$other" -p sp-app "$@"
    cargo build --release --target "$other" -p sp-mirror "$@"
fi
# Puts a program into the app: this Mac's build of it, joined in a universal build with the other
# processor's.
place() {
    if [ -n "$universal" ]; then
        lipo -create "target/release/$1" "target/$other/release/$1" -output "$app/Contents/MacOS/$1"
    else
        cp "target/release/$1" "$app/Contents/MacOS/$1"
    fi
}

# What the app is signed with. A local build is signed with a code-signing identity of its
# own, when this Mac's keychain has one by this name (or the name in
# SOFTPRESENTER_SIGN_IDENTITY): macOS then sees each new build as the same app, so the
# Keychain's "Always Allow" for the VoxLive key lasts from build to build. Without one, it is
# signed "ad hoc", and macOS asks again after every build. No key or certificate is ever kept
# in this repository; CONTRIBUTING.md says how to make one for your own Mac.
identity="${SOFTPRESENTER_SIGN_IDENTITY:-SoftPresenter Local Signing}"
if security find-identity -p codesigning 2>/dev/null | grep -qF "\"$identity\""; then
    sign="$identity"
else
    sign="-"
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=target/release/SoftPresenter.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
place softpresenter
# The app's icon, for the Dock, the Finder and the app switcher.
cp assets/icons/SoftPresenter.icns "$app/Contents/Resources/SoftPresenter.icns"
# SoftPresenter Sans, the slides' typeface, is built into the program; its licence goes with it.
mkdir -p "$app/Contents/Resources/SoftPresenterSans"
cp crates/sp-render/fonts/OFL.txt crates/sp-render/fonts/AUTHORS.txt "$app/Contents/Resources/SoftPresenterSans/"
# The sample backgrounds (pictures and a looping video, CC0), for the screen that will offer them.
mkdir -p "$app/Contents/Resources/Backgrounds"
cp assets/backgrounds/*.jpg assets/backgrounds/*.mp4 assets/backgrounds/README.md "$app/Contents/Resources/Backgrounds/"
# FFmpeg, which plays the videos macOS's own player can't (MKV, WebM, AVI and others): an LGPL
# build, made once by scripts/build-ffmpeg.sh (for both processors, whichever this build is for),
# beside the app's program with its licence and a note of its source. Without it (a computer that can't download it, say) the app still builds,
# and uses an FFmpeg installed on the computer, if there is one.
if scripts/build-ffmpeg.sh >/dev/null; then
    cp target/ffmpeg/bin/ffmpeg target/ffmpeg/bin/ffprobe "$app/Contents/MacOS/"
    mkdir -p "$app/Contents/Resources/FFmpeg"
    cp target/ffmpeg/COPYING.LGPLv2.1 target/ffmpeg/LICENSE.md target/ffmpeg/SOURCE.txt "$app/Contents/Resources/FFmpeg/"
    for program in ffmpeg ffprobe; do
        codesign --force --sign "$sign" "$app/Contents/MacOS/$program" >/dev/null 2>&1 || true
    done
else
    echo "note: FFmpeg isn't in this build (scripts/build-ffmpeg.sh failed); MP4 and MOV still play"
fi
# softpresenter-mirror, which receives phone screens, beside the app's program as FFmpeg is, with
# its notices: it is GPL-3.0-or-later with a GPL-3.0 part, and the additional permission for NDI
# and DeckLink does not extend to it (docs/licences/screen-mirroring.md).
place softpresenter-mirror
codesign --force --sign "$sign" "$app/Contents/MacOS/softpresenter-mirror" >/dev/null 2>&1 || true
mkdir -p "$app/Contents/Resources/ScreenMirroring"
cp LICENSE "$app/Contents/Resources/ScreenMirroring/GPL-3.0.txt"
for notice in docs/licences/screen-mirroring.md docs/licences/LGPL-2.1.txt; do
    if [ -f "$notice" ]; then
        cp "$notice" "$app/Contents/Resources/ScreenMirroring/"
    else
        echo "note: $notice is missing; the app is built without that notice"
    fi
done
# NSAppSleepDisabled: macOS must not slow the app down (App Nap) while its windows are hidden;
# timed slides and timers run by the clock during a service.
# NSAppleEventsUsageDescription: why macOS asks to let SoftPresenter use Keynote or PowerPoint,
# which turn a presentation file into slides on import.
# NSLocalNetworkUsageDescription and NSBonjourServices: why macOS asks to let SoftPresenter find
# and be found by devices on the local network, and every Bonjour service it announces or looks
# for. Once the list is there macOS blocks any service not in it, so it must name them all: NDI's
# (screens sent over NDI, and NDI Source boxes finding sources) and the two a phone looks for when
# it shows its screen. Leaving NDI's out stopped every NDI screen being found.
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>SoftPresenter</string>
    <key>CFBundleDisplayName</key>
    <string>SoftPresenter</string>
    <key>CFBundleIdentifier</key>
    <string>com.softpresenter.app</string>
    <key>CFBundleIconFile</key>
    <string>SoftPresenter</string>
    <key>CFBundleExecutable</key>
    <string>softpresenter</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>$macos</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSAppSleepDisabled</key>
    <true/>
    <key>NSAppleEventsUsageDescription</key>
    <string>SoftPresenter asks Keynote or PowerPoint to turn a presentation into slides when you import one.</string>
    <key>NSLocalNetworkUsageDescription</key>
    <string>SoftPresenter uses the local network so an iPhone, iPad or Mac can show its screen, and for NDI and Remote Control.</string>
    <key>NSBonjourServices</key>
    <array>
        <string>_ndi._tcp</string>
        <string>_airplay._tcp</string>
        <string>_raop._tcp</string>
    </array>
</dict>
</plist>
PLIST
# A local signature, so macOS runs the app as built on this computer.
codesign --force --deep --sign "$sign" "$app" >/dev/null 2>&1 || echo "note: could not sign the app; it may still run"
# A universal build is checked before it is shared: every program in it has a part for each
# processor, made for the oldest macOS the app says it runs on, and none has this computer's home
# folder written into it.
kind="for this Mac's processor"
if [ -n "$universal" ]; then
    kind="universal: Apple silicon and Intel"
    for file in "$app"/Contents/MacOS/*; do
        for arch in arm64 x86_64; do
            if ! otool -arch "$arch" -l "$file" 2>/dev/null | grep -A4 LC_BUILD_VERSION | grep -qx " *minos $macos"; then
                echo "error: $file has no part for $arch made for macOS $macos" >&2
                exit 1
            fi
        done
        if strings -a -arch all "$file" | grep -q "$HOME"; then
            echo "error: $file has this computer's home folder written into it" >&2
            exit 1
        fi
    done
fi
if [ "$sign" = "-" ]; then
    echo "built $app, $kind, for macOS $macos and later (signed ad hoc)"
else
    echo "built $app, $kind, for macOS $macos and later (signed as \"$sign\")"
fi
