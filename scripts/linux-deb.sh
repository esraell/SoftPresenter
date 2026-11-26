#!/bin/sh
# Builds the Linux package from nothing, on Ubuntu 22.04:
# target/linux-deb/softpresenter_<version>_amd64.deb, for Ubuntu 22.04 and later, Linux Mint 21
# and later, Debian 12 and later, and the systems made from them.
#
#   scripts/linux-deb.sh
#   SOFTPRESENTER_RELEASE_VERSION=0.9.0-dev.18 scripts/linux-deb.sh    # a pre-release
#
# Build it on the oldest system it is for, Ubuntu 22.04: a program built on a newer one needs
# that one's C library, and would not start on the older ones. It needs the packages
# .github/workflows/linux-package.yml installs, and Rust (rustup installs the version in
# rust-toolchain.toml by itself). JOBS=8 builds with 8 jobs instead of one for each processor.
#
# It builds the program and softpresenter-mirror with this computer's home folder shortened to
# "~", and the LGPL FFmpeg (scripts/build-ffmpeg.sh linux), and lays the package out:
#
#   /usr/lib/softpresenter/       softpresenter, softpresenter-mirror, ffmpeg and ffprobe, and
#                                 Backgrounds (a link to /usr/share/softpresenter/Backgrounds):
#                                 the app looks for all of them beside its own program
#   /usr/bin/softpresenter, sp    links to the program; Settings › Command line says sp came
#                                 with the package, and the app adds no sp of its own
#   /usr/share/                   the menu entry, the icons, the AppStream file, the manual page,
#                                 and in doc/softpresenter the licences (copyright) and changelog
#
# Its Depends are what dpkg-shlibdeps finds the programs linked against, and the libraries the
# app opens by itself as it starts (the window system's, and Vulkan's), which no tool can see.
# The script stops if a program needs a newer C library than Ubuntu 22.04's, or still has this
# computer's home folder written into it.
set -eu
cd "$(dirname "$0")/.."
umask 022

if [ "$(uname -s)" != Linux ]; then
    echo "error: the Linux package is built on Linux (Ubuntu 22.04)" >&2
    exit 1
fi
# A development build offers the song catalog, which must stay out of anything shared.
if [ -n "${SOFTPRESENTER_DEV_VERSION:-}" ]; then
    echo "error: SOFTPRESENTER_DEV_VERSION is set; a package to share is built with SOFTPRESENTER_RELEASE_VERSION" >&2
    exit 1
fi
missing=""
for tool in cargo dpkg-deb dpkg-shlibdeps objdump strings strip gzip rsvg-convert nasm make; do
    command -v "$tool" >/dev/null || missing="$missing $tool"
done
if [ -n "$missing" ]; then
    echo "error: missing:$missing (the packages in .github/workflows/linux-package.yml)" >&2
    exit 1
fi

# The version: a pre-release's (0.9.0-dev.18) or the one in Cargo.toml (0.9.0). In Debian's
# order a pre-release comes before its version, so the part after the dash goes after a tilde,
# which sorts before anything: 0.9.0~dev.18 < 0.9.0. The package has no revision of its own
# (no "-1"), so its file is named as the release is.
version=${SOFTPRESENTER_RELEASE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
debversion=$(printf '%s' "$version" | sed 's/-/~/')
if ! printf '%s\n' "$debversion" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(~[A-Za-z0-9.]+)?$'; then
    echo "error: \"$version\" isn't a version this script knows (0.9.0, or 0.9.0-dev.18)" >&2
    exit 1
fi
case "$version" in
*-*) channel=development ;;
*) channel=stable ;;
esac
arch=$(dpkg --print-architecture)
# Dates in the package are the commit's, so a package built twice from it is the same.
SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}
export SOURCE_DATE_EPOCH
commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)
day=$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%d)

# The programs. SOFTPRESENTER_RELEASE_VERSION, when set, is read by the build as it is.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
if [ -n "${JOBS:-}" ]; then export CARGO_BUILD_JOBS="$JOBS"; fi
cargo build --release --locked -p sp-app -p sp-mirror
scripts/build-ffmpeg.sh linux

out=target/linux-deb
root=$out/root
lib=$root/usr/lib/softpresenter
share=$root/usr/share
doc=$share/doc/softpresenter
rm -rf "$out"
mkdir -p "$lib" "$root/usr/bin" "$share/softpresenter/Backgrounds" "$share/applications" "$share/metainfo" \
    "$share/man/man1" "$doc" "$share/lintian/overrides" "$root/DEBIAN"

# The programs, without their symbols (as Debian's own are), beside each other.
for program in softpresenter softpresenter-mirror; do
    install -m 0755 "target/release/$program" "$lib/$program"
done
for program in ffmpeg ffprobe; do
    install -m 0755 "target/ffmpeg-linux/bin/$program" "$lib/$program"
done
for program in softpresenter softpresenter-mirror ffmpeg ffprobe; do
    strip --remove-section=.comment --remove-section=.note "$lib/$program"
    # Nothing of this computer's: no home folder written in.
    if strings "$lib/$program" | grep -qF "$HOME"; then
        echo "error: $program has a folder of this computer written into it" >&2
        exit 1
    fi
    # Runs on Ubuntu 22.04's C library (2.35) and later.
    newest=$(objdump -T "$lib/$program" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -t. -k2 -n -u | tail -1)
    echo "$program needs $newest at most"
    case "$newest" in
    GLIBC_2.[0-9] | GLIBC_2.[0-2][0-9] | GLIBC_2.3[0-5]) ;;
    *)
        echo "error: $program needs a C library newer than Ubuntu 22.04's ($newest); build it on Ubuntu 22.04" >&2
        exit 1
        ;;
    esac
done
ln -s ../lib/softpresenter/softpresenter "$root/usr/bin/softpresenter"
ln -s ../lib/softpresenter/softpresenter "$root/usr/bin/sp"

# The sample backgrounds (CC0): data, so in /usr/share, and found beside the program through a link.
install -m 0644 assets/backgrounds/*.jpg assets/backgrounds/*.mp4 "$share/softpresenter/Backgrounds/"
ln -s ../../share/softpresenter/Backgrounds "$lib/Backgrounds"

# The menu entry. The windows say they are "softpresenter" (their Wayland app id, and X11's
# class), which is how the desktop matches them to this file and its icon.
cat >"$share/applications/softpresenter.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=SoftPresenter
GenericName=Church Presentation
Comment=Show songs, Bible passages, pictures and videos on the screens in a church service
Exec=softpresenter
Icon=softpresenter
Terminal=false
Categories=Office;Presentation;
Keywords=church;worship;songs;lyrics;Bible;slides;projector;NDI;
StartupWMClass=softpresenter
DESKTOP

# The icon, at every size the desktop asks for, and drawn to any size.
for size in 16 22 24 32 48 64 128 256 512; do
    mkdir -p "$share/icons/hicolor/${size}x$size/apps"
    rsvg-convert -w "$size" -h "$size" assets/icons/icon.svg -o "$share/icons/hicolor/${size}x$size/apps/softpresenter.png"
done
mkdir -p "$share/icons/hicolor/scalable/apps"
install -m 0644 assets/icons/icon.svg "$share/icons/hicolor/scalable/apps/softpresenter.svg"

# What software centres show.
cat >"$share/metainfo/com.softpresenter.app.metainfo.xml" <<METAINFO
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>com.softpresenter.app</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-or-later</project_license>
  <name>SoftPresenter</name>
  <summary>Show songs, Bible passages and media in a church service</summary>
  <developer_name>The SoftPresenter contributors</developer_name>
  <description>
    <p>
      SoftPresenter is free presentation software for churches. It puts song words, Bible
      passages, pictures and videos on the screens during a service, in one language or two on
      every slide, and runs well on a modest computer.
    </p>
    <ul>
      <li>Songs with their verses and choruses, and a playlist for the service</li>
      <li>The Bible in many translations, side by side, with references typed in English, Russian or Ukrainian</li>
      <li>Pictures and videos behind the words, timers, messages and a stage screen</li>
      <li>Screens sent over NDI, and controlled from a phone, the command line (sp) or an AI assistant</li>
    </ul>
  </description>
  <launchable type="desktop-id">softpresenter.desktop</launchable>
  <icon type="stock">softpresenter</icon>
  <url type="homepage">https://softpresenter.com</url>
  <url type="bugtracker">https://github.com/esraell/SoftPresenter/issues</url>
  <provides>
    <binary>softpresenter</binary>
    <binary>sp</binary>
  </provides>
  <content_rating type="oars-1.1"/>
  <releases>
    <release version="$version" date="$day" type="$channel"/>
  </releases>
</component>
METAINFO

# The manual page, for softpresenter and sp alike.
cat >"$out/softpresenter.1" <<MANUAL
.TH SOFTPRESENTER 1 "$day" "SoftPresenter $version" "User Commands"
.nh
.SH NAME
softpresenter, sp \\- church presentation app, and its command line
.SH SYNOPSIS
.B softpresenter
.br
.B sp
.RI [ command " [" arguments ...]]
.RB [ \\-\\-json ]
.SH DESCRIPTION
.B softpresenter
alone opens SoftPresenter, which puts song words, Bible passages, pictures and videos on the
screens during a church service.
.PP
With a command after it,
.B softpresenter
(or its short name,
.BR sp )
controls the SoftPresenter that is running, from a terminal or a script, and prints its answer;
.B \\-\\-json
gives the answer as JSON.
.B sp
alone says what is on screen, and opens SoftPresenter first if it isn't running.
.SH EXAMPLES
.TP
.B sp status
What is on screen, the next slide, the media, timers and the screens.
.TP
.B sp go """Blessed Assurance""" 3
Shows a song's third slide.
.TP
.B sp next
The next slide.
.TP
.B sp clear all
Clears the screens.
.TP
.B sp help
Every command, and
.B sp help
.I command
for one.
.SH ENVIRONMENT
.TP
.B SOFTPRESENTER_DATA
Keep the library, Bibles, settings and log in this folder instead of
.IR ~/.local/share/softpresenter .
.TP
.B SOFTPRESENTER_CONTROL_SOCKET
Where the app and
.B sp
meet; set it to the same path for both to try things on a test copy.
.TP
.B SOFTPRESENTER_QUIET
Set to 1 for a test copy: its windows open behind the others, its screens never fill a
display, and it sends nothing over the network.
.SH FILES
.TP
.I ~/.local/share/softpresenter/
The songs, playlists, Bibles, settings and log (or
.IR \$XDG_DATA_HOME/softpresenter/ ).
.SH SEE ALSO
The full list of commands, with their answers:
.UR https://github.com/esraell/SoftPresenter/blob/master/docs/cli.md
.UE
MANUAL
gzip -9n <"$out/softpresenter.1" >"$share/man/man1/softpresenter.1.gz"
ln -s softpresenter.1.gz "$share/man/man1/sp.1.gz"

# The licences of everything in the package, in the machine-readable form, and the notices
# that go with them. Licences Debian keeps in /usr/share/common-licenses are named, not copied.
ffmpeg_version=$(cat target/ffmpeg-linux/VERSION)
{
    cat <<COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: SoftPresenter
Upstream-Contact: https://github.com/esraell/SoftPresenter/issues
Source: https://github.com/esraell/SoftPresenter
Comment: This package is built, unchanged, from the source in that repository at
 commit $commit.
 NOTICE.md in this folder has the notices for everything SoftPresenter
 carries from other projects, and for NDI and DeckLink, which it can use but
 never ships.

Files: *
Copyright: 2026 Esrael Nate and the SoftPresenter contributors
License: GPL-3+ with SoftPresenter exception
 SoftPresenter is free software: you can redistribute it and/or modify it
 under the terms of the GNU General Public License as published by the Free
 Software Foundation, either version 3 of the License, or (at your option) any
 later version, with the additional permission in ADDITIONAL-PERMISSION.md in
 this folder (for the NDI runtime and DeckLink driver, which are installed
 separately).
 .
 It is distributed in the hope that it will be useful, but WITHOUT ANY
 WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
 A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 .
 On Debian systems, the complete text of the GNU General Public License,
 version 3, is in /usr/share/common-licenses/GPL-3.
 .
 The GNU GPL gives no rights to the name "SoftPresenter" (GPL section 7(e)).
Comment: The program has the SoftPresenter Sans typeface built in, under the
 SIL Open Font License 1.1 (SoftPresenterSans-OFL.txt in this folder, with its
 authors in SoftPresenterSans-AUTHORS.txt), and the other fonts and tables
 NOTICE.md lists, each under its own licence.

Files: usr/lib/softpresenter/softpresenter-mirror
Copyright: 2026 Esrael Nate and the SoftPresenter contributors
License: GPL-3+
 This program is free software: you can redistribute it and/or modify it
 under the terms of the GNU General Public License as published by the Free
 Software Foundation, either version 3 of the License, or (at your option) any
 later version. Its FairPlay module is under version 3 of that License.
 .
 On Debian systems, the complete text of the GNU General Public License,
 version 3, is in /usr/share/common-licenses/GPL-3.
Comment: softpresenter-mirror receives an iPhone, iPad or Mac screen (unofficial;
 not made or supported by Apple). It follows UxPlay's lib/ (LGPL-2.1+;
 /usr/share/common-licenses/LGPL-2.1) and carries a port of playfair (GPL-3).
 SoftPresenter's additional permission does not extend to it.
 screen-mirroring.md in this folder says what it follows, with their notices.

Files: usr/lib/softpresenter/ffmpeg usr/lib/softpresenter/ffprobe
Copyright: the FFmpeg developers
License: LGPL-2.1+
 These programs are free software; you can redistribute them and/or modify
 them under the terms of the GNU Lesser General Public License as published by
 the Free Software Foundation; either version 2.1 of the License, or (at your
 option) any later version.
 .
 On Debian systems, the complete text of the GNU Lesser General Public
 License, version 2.1, is in /usr/share/common-licenses/LGPL-2.1.
Comment: FFmpeg $ffmpeg_version, built unchanged from the source FFmpeg publishes,
 configured as LGPL. FFmpeg-SOURCE.txt in this folder names the exact source
 file, its SHA-256 and the configuration; FFmpeg-LICENSE.md is FFmpeg's own
 note on its licence. SoftPresenter runs them as separate programs; no FFmpeg
 code is in SoftPresenter itself. FFmpeg is a trademark of Fabrice Bellard,
 originator of the FFmpeg project.

Files: usr/share/softpresenter/Backgrounds/*
Copyright: 2026 the SoftPresenter contributors
License: CC0-1.0
 To the extent possible under law, the SoftPresenter contributors have waived
 all copyright and related or neighbouring rights to the sample backgrounds.
 .
 On Debian systems, the complete text of the CC0 1.0 Universal public domain
 dedication is in /usr/share/common-licenses/CC0-1.0.
COPYRIGHT
} >"$doc/copyright"
cp ADDITIONAL-PERMISSION.md "$doc/ADDITIONAL-PERMISSION.md"
cp NOTICE.md "$doc/NOTICE.md"
cp docs/licences/screen-mirroring.md "$doc/screen-mirroring.md"
# The note names the licence files as the Mac app has them; here they are elsewhere.
sed 's|(COPYING.LGPLv2.1, and LICENSE.md for the details)|(/usr/share/common-licenses/LGPL-2.1, and FFmpeg-LICENSE.md\
for the details)|' target/ffmpeg-linux/SOURCE.txt >"$doc/FFmpeg-SOURCE.txt"
cp target/ffmpeg-linux/LICENSE.md "$doc/FFmpeg-LICENSE.md"
cp crates/sp-render/fonts/OFL.txt "$doc/SoftPresenterSans-OFL.txt"
cp crates/sp-render/fonts/AUTHORS.txt "$doc/SoftPresenterSans-AUTHORS.txt"
cp assets/backgrounds/README.md "$doc/Backgrounds.md"
# Debian's changelog of the package: one entry, for this build. What changed in the app is in
# its own CHANGELOG.md.
cat >"$out/changelog.Debian" <<CHANGELOG
softpresenter ($debversion) unstable; urgency=medium

  * SoftPresenter $version, built from the public source at commit
    $commit.
    What changed is in CHANGELOG.md:
    https://github.com/esraell/SoftPresenter/blob/master/CHANGELOG.md

 -- SoftPresenter <105669618+esraell@users.noreply.github.com>  $(date -u -R -d "@$SOURCE_DATE_EPOCH")
CHANGELOG
gzip -9n <"$out/changelog.Debian" >"$doc/changelog.Debian.gz"

# What lintian is told to accept, each with why. (A "*" stands for the file, which lintian
# writes with or without brackets depending on its version.)
cat >"$share/lintian/overrides/softpresenter" <<'OVERRIDES'
# FFmpeg's own programs, with FFmpeg's libraries built into them on purpose: the package brings
# one LGPL FFmpeg, built unchanged from FFmpeg's source, so that a video plays the same on every
# distribution and release it installs on, whatever FFmpeg (if any) the system has.
softpresenter: embedded-library libav* *usr/lib/softpresenter/ffmpeg*
softpresenter: embedded-library libav* *usr/lib/softpresenter/ffprobe*
softpresenter: embedded-library libsw* *usr/lib/softpresenter/ffmpeg*
softpresenter: embedded-library libsw* *usr/lib/softpresenter/ffprobe*
# The version has no Debian revision ("0.9.0~dev.18", not "0.9.0~dev.18-1"), so the package's
# file is named as the release is; changelog.Debian.gz is Debian's record of the build, kept
# apart from the app's own list of changes (Help › What's New, CHANGELOG.md).
softpresenter: wrong-name-for-changelog-of-native-package *usr/share/doc/softpresenter/changelog.Debian.gz*
# Rust programs: Rust's own code calls the C library's functions directly, never through the
# C headers' checked (_FORTIFY_SOURCE) forms, so there are none to find.
softpresenter: hardening-no-fortify-functions *usr/lib/softpresenter/softpresenter*
# Words in FFmpeg's and the Rust libraries' own messages, and in bytes that only look like
# words; none is SoftPresenter's.
softpresenter: spelling-error-in-binary *
# The sample backgrounds (pictures and a looping video, about 10 MB): one package for the one
# architecture SoftPresenter is built for on Linux.
softpresenter: arch-dep-package-has-big-usr-share *
OVERRIDES

# Every file and folder as Debian expects: root's, readable by all, the programs runnable.
find "$root" -type d -exec chmod 0755 {} +
find "$root" -type f -exec chmod 0644 {} +
chmod 0755 "$lib/softpresenter" "$lib/softpresenter-mirror" "$lib/ffmpeg" "$lib/ffprobe"

# Depends: what the programs are linked against (dpkg-shlibdeps reads that from them; it wants
# a debian/control, so it runs in a folder with a small one)...
shlibs=$out/shlibdeps
mkdir -p "$shlibs/debian"
printf 'Source: softpresenter\n\nPackage: softpresenter\nArchitecture: any\n' >"$shlibs/debian/control"
here=$PWD
linked=$(cd "$shlibs" && dpkg-shlibdeps -O \
    -e"$here/$lib/softpresenter" -e"$here/$lib/softpresenter-mirror" -e"$here/$lib/ffmpeg" -e"$here/$lib/ffprobe" |
    sed -n 's/^shlibs:Depends=//p')
rm -rf "$shlibs"
# ... with the name Ubuntu 24.04 and Debian 13 gave a library when its time values grew to 64
# bits (libasound2 became libasound2t64) as the first choice and the old name as the other ...
linked=$(printf '%s\n' "$linked" | sed -E 's/(^|, )(libasound2) (\([^)]*\))/\1\2t64 \3 | \2 \3/g')
# ... and what the app opens by itself as it starts, which no tool can see in it: the window
# system's libraries (Wayland's; X11's, its pointers and input; the keyboard maps for both),
# Vulkan's loader and EGL, through which it draws, and D-Bus's, through which it asks the
# desktop for its Open and Save windows. Found by reading which libraries a running copy had
# loaded, on Wayland and on X11, and in the code that opens them. The same names on Ubuntu
# 22.04 to 26.04 and Debian 12 and 13.
opened="libwayland-client0, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libvulkan1, libegl1, libdbus-1-3"
depends="$linked, $opened"
echo "Depends: $depends"

# Recommends: Mesa's Vulkan drivers, for a computer with no driver of its own that draws with
# Vulkan (the app draws in software with them, as on a machine with no graphics card); the
# DejaVu faces the app's own words are drawn in (DejaVu Sans Mono, for numbers and clocks, is a
# package of its own from Ubuntu 24.04 and Debian 13); the desktop's portal for the Open and
# Save windows (or zenity, which the app falls back on); xdg-open, for links and folders; and
# secret-tool, which keeps the integrations' keys in the desktop's keyring. Every desktop
# system has most of them already. Nothing for NDI (the app downloads NDI's runtime itself) or
# DeckLink (Blackmagic's Desktop Video, which the description mentions).
installed_size=$(du -sk --apparent-size "$root/usr" | cut -f1)
cat >"$root/DEBIAN/control" <<CONTROL
Package: softpresenter
Version: $debversion
Architecture: $arch
Maintainer: SoftPresenter <105669618+esraell@users.noreply.github.com>
Installed-Size: $installed_size
Depends: $depends
Recommends: mesa-vulkan-drivers, fonts-dejavu-core, fonts-dejavu-mono | fonts-dejavu-core, xdg-desktop-portal | zenity, xdg-utils, libsecret-tools
Section: graphics
Priority: optional
Homepage: https://softpresenter.com
Description: presentation app for churches: songs, the Bible and media
 SoftPresenter puts song words, Bible passages, pictures and videos on the
 screens during a church service, in one language or two on every slide. It
 has playlists for the service, the Bible in many translations side by side,
 timers, messages and a stage screen, and it can be controlled from a phone,
 from the command line (sp) or by an AI assistant.
 .
 Videos play through the LGPL build of FFmpeg that comes with the package.
 Screens can be sent over NDI: SoftPresenter downloads NDI's runtime itself
 when asked. Playing a screen out of a Blackmagic Design DeckLink card needs
 Blackmagic's Desktop Video driver, installed separately.
CONTROL
(cd "$root" && find usr -type f -print0 | LC_ALL=C sort -z | xargs -0 md5sum) >"$root/DEBIAN/md5sums"
chmod 0644 "$root/DEBIAN/control" "$root/DEBIAN/md5sums"

# The same dates throughout, so the same commit makes the same package.
find "$root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
deb=$out/softpresenter_${debversion}_$arch.deb
dpkg-deb --root-owner-group -Zxz --build "$root" "$deb"
echo "built $deb: $(du -h "$deb" | cut -f1)"
