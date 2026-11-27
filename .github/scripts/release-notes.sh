#!/bin/sh
# The notes for a release's draft. They hold the CHANGELOG.md section for that version, with
# the downloads and the warnings people will see around it. The release workflow (release.yml)
# writes them into the draft, and the person who publishes the draft reads and edits them first.
#
#   .github/scripts/release-notes.sh <version>      # 0.9.0-dev.28, or 0.9.0
#
# It fails if CHANGELOG.md has no section for that version, or only an empty one.
set -eu
cd "$(dirname "$0")/../.."

if [ $# -ne 1 ] || ! printf '%s\n' "$1" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$'; then
    echo "usage: $0 <version>   (such as 0.9.0 or 0.9.0-dev.28)" >&2
    exit 2
fi
version=$1

# The section: the lines after "## <version>" (followed by a space or nothing, so 0.9.0-dev.2
# never takes 0.9.0-dev.27's) up to the next "## " heading, without blank lines at either end.
section=$(awk -v heading="## $version" '
    !inside && index($0, heading) == 1 && (length($0) == length(heading) || substr($0, length(heading) + 1, 1) == " ") { inside = 1; next }
    inside && /^## / { exit }
    inside { print }
' CHANGELOG.md | awk 'NF { started = 1 } started' | awk '{ lines[NR] = $0 } NF { last = NR } END { for (i = 1; i <= last; i++) print lines[i] }')
if [ -z "$section" ]; then
    echo "error: CHANGELOG.md has no section \"## $version — <date>\", or nothing under it" >&2
    exit 1
fi

# GitHub names a release's files as they are named, but for "~" in the .deb's, which becomes ".".
deb="softpresenter_$(printf '%s' "$version" | sed 's/-/./')_amd64.deb"
mac="SoftPresenter-$version-mac.zip"
windows="SoftPresenter-$version-windows.zip"

case "$version" in
*-*)
    cat <<'TEXT'
**Pre-release: not for Sunday use yet.** This is SoftPresenter as it stands on its way to version 1. Things may still change: the core of how SoftPresenter works and looks will stay, and bugs will be fixed in the versions that come next. Keep your previous copy until you've tried a new one; Check for Updates tells you when one is out.

TEXT
    ;;
esac
cat <<TEXT
## What's new

$section

The full list is in [CHANGELOG.md](https://github.com/esraell/SoftPresenter/blob/master/CHANGELOG.md).

## Downloads

- **Mac** (Apple silicon or Intel; macOS 11 or later): \`$mac\`. Unzip and drag SoftPresenter to Applications.
- **Windows** (64-bit, Windows 10 or 11): \`$windows\`. Unzip it, keep the folder together, and run SoftPresenter.exe.
- **Linux** (Ubuntu 22.04+, Linux Mint 21+, Debian 12+): \`$deb\`. Install with \`sudo apt install ./$deb\`; it appears in the applications menu, and \`sp\` works in a terminal. DeckLink cards need Blackmagic's Desktop Video.

## The warnings you will see (expected)

These builds aren't signed with a paid Apple or Microsoft certificate yet, so your computer warns you the first time. That is expected:

- **Mac:** "SoftPresenter can't be opened because Apple cannot check it for malicious software" (or "unidentified developer"). Click **Done**, then open **System Settings › Privacy & Security**, scroll down to the line about SoftPresenter and click **Open Anyway**, then **Open**. You only do this once.
- **Windows:** "Windows protected your PC" (SmartScreen). Click **More info**, then **Run anyway**. Windows may also ask whether to let SoftPresenter use the network (for NDI and the phone remote); allow it on private networks.

SoftPresenter is free and open source (GPL-3.0-or-later); this build is made from the source in this repository at the commit this release names.
TEXT
