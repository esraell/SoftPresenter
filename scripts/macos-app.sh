#!/bin/sh
# Builds SoftPresenter and wraps it as a Mac app, target/release/SoftPresenter.app, so the
# menu bar, the Dock and Activity Monitor call it "SoftPresenter". Any arguments are passed to
# cargo.
#
#   scripts/macos-app.sh
#   open target/release/SoftPresenter.app
set -eu
cd "$(dirname "$0")/.."
# The paths of the source and of the libraries are written into the program (for messages about
# where it stopped); the builder's home folder is shortened to ~ in them, so a build that is shared
# says nothing about whose computer made it.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
cargo build --release -p sp-app "$@"

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
cp target/release/softpresenter "$app/Contents/MacOS/softpresenter"
# The app's icon, for the Dock, the Finder and the app switcher.
cp assets/icons/SoftPresenter.icns "$app/Contents/Resources/SoftPresenter.icns"
# SoftPresenter Sans, the slides' typeface, is built into the program; its licence goes with it.
mkdir -p "$app/Contents/Resources/SoftPresenterSans"
cp crates/sp-render/fonts/OFL.txt crates/sp-render/fonts/AUTHORS.txt "$app/Contents/Resources/SoftPresenterSans/"
# The sample backgrounds (pictures and a looping video, CC0), for the screen that will offer them.
mkdir -p "$app/Contents/Resources/Backgrounds"
cp assets/backgrounds/*.jpg assets/backgrounds/*.mp4 assets/backgrounds/README.md "$app/Contents/Resources/Backgrounds/"
# FFmpeg, which plays the videos macOS's own player can't (MKV, WebM, AVI and others): an LGPL
# build, made once by scripts/build-ffmpeg.sh, beside the app's program with its licence and a
# note of its source. Without it (a computer that can't download it, say) the app still builds,
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
# NSAppSleepDisabled: macOS must not slow the app down (App Nap) while its windows are hidden;
# timed slides and timers run by the clock during a service.
# NSAppleEventsUsageDescription: why macOS asks to let SoftPresenter use Keynote or PowerPoint,
# which turn a presentation file into slides on import.
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
    <string>13.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSAppSleepDisabled</key>
    <true/>
    <key>NSAppleEventsUsageDescription</key>
    <string>SoftPresenter asks Keynote or PowerPoint to turn a presentation into slides when you import one.</string>
</dict>
</plist>
PLIST
# A local signature, so macOS runs the app as built on this computer.
codesign --force --deep --sign "$sign" "$app" >/dev/null 2>&1 || echo "note: could not sign the app; it may still run"
if [ "$sign" = "-" ]; then
    echo "built $app (signed ad hoc)"
else
    echo "built $app (signed as \"$sign\")"
fi
