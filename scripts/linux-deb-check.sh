#!/bin/sh
# Tries the installed Linux package: the files are where the app looks for them, and the app
# from /usr/bin starts as a quiet test copy, answers sp, says its version, shows a song, plays
# a video through the package's own FFmpeg, and quits cleanly.
#
#   sudo apt install ./target/linux-deb/softpresenter_0.9.0~dev.18_amd64.deb
#   scripts/linux-deb-check.sh 0.9.0-dev.18             # the version the app should say
#   xvfb-run -a scripts/linux-deb-check.sh 0.9.0-dev.18 # on a computer with no display
#
# The copy is a test copy of its own (AGENTS.md): SOFTPRESENTER_QUIET, and a data folder and
# home folder in a new folder under /tmp, with every other SOFTPRESENTER_ variable taken out,
# so it never reaches anyone's library, settings or running app. It is started as the menu
# entry starts it, through /usr/bin/softpresenter: on Linux its socket is then in its own data
# folder, and every sp command here names that socket (SOFTPRESENTER_CONTROL_SOCKET), so sp
# never starts an app of its own. It is always quit through that socket (killed if it hasn't
# gone 30 seconds later). The folder is removed when every check passes and kept, with its
# path said, when one fails.
set -eu
expected=${1:-}
lib=/usr/lib/softpresenter

for name in $(env | sed -n 's/^\(SOFTPRESENTER_[A-Z0-9_]*\)=.*/\1/p'); do
    unset "$name"
done
work=$(mktemp -d /tmp/spdeb.XXXXXX)
export SOFTPRESENTER_QUIET=1 SOFTPRESENTER_LANGUAGE=en
export SOFTPRESENTER_DATA="$work/data" SOFTPRESENTER_CONTROL_SOCKET="$work/data/control/ctl.sock"
app=""
passed=0

fail() {
    echo "FAILED: $*" >&2
    if [ -f "$work/data/SoftPresenter.log" ]; then
        echo "--- the end of the copy's log:" >&2
        tail -n 30 "$work/data/SoftPresenter.log" >&2
    fi
    if [ -s "$work/app.err" ]; then
        echo "--- the end of its standard error:" >&2
        tail -n 20 "$work/app.err" >&2
    fi
    exit 1
}
# However it ends: the copy quit through its own socket, killed if it won't go.
finish() {
    if [ -n "$app" ] && kill -0 "$app" 2>/dev/null; then
        /usr/bin/sp menu Quit >/dev/null 2>&1 || true
        for _ in $(seq 1 60); do kill -0 "$app" 2>/dev/null || break; sleep 0.5; done
        kill -9 "$app" 2>/dev/null || true
    fi
    if [ "$passed" = 1 ]; then rm -rf "$work"; else echo "the test's folder is kept: $work" >&2; fi
}
trap finish EXIT
ok() { echo "ok: $*"; }

# The files: the program and its links, FFmpeg, the helper, the backgrounds through their link,
# the menu entry, the icon and the AppStream file.
[ "$(readlink -f /usr/bin/sp)" = "$lib/softpresenter" ] || fail "/usr/bin/sp doesn't lead to $lib/softpresenter"
[ "$(readlink -f /usr/bin/softpresenter)" = "$lib/softpresenter" ] || fail "/usr/bin/softpresenter doesn't lead to the program"
for program in softpresenter softpresenter-mirror ffmpeg ffprobe; do
    [ -x "$lib/$program" ] || fail "$lib/$program isn't there"
done
[ -f "$lib/Backgrounds/Drifting Light.mp4" ] || fail "the sample backgrounds aren't found beside the program"
for file in /usr/share/applications/softpresenter.desktop /usr/share/icons/hicolor/256x256/apps/softpresenter.png \
    /usr/share/icons/hicolor/scalable/apps/softpresenter.svg /usr/share/metainfo/com.softpresenter.app.metainfo.xml \
    /usr/share/man/man1/sp.1.gz /usr/share/doc/softpresenter/copyright; do
    [ -f "$file" ] || fail "$file isn't there"
done
ok "the package's files are in place"
"$lib/ffmpeg" -hide_banner -version | head -1 | grep -q '^ffmpeg version' || fail "the package's ffmpeg doesn't run here"
if "$lib/ffmpeg" -hide_banner -buildconf | grep -qE 'enable-(gpl|nonfree)'; then fail "the package's FFmpeg isn't LGPL"; fi
ok "$("$lib/ffmpeg" -hide_banner -version | head -1 | cut -d' ' -f1-3) runs, LGPL"

# sp with the test copy's socket and nothing there: says so, exits 1, and starts nothing.
if /usr/bin/sp status >"$work/none.out" 2>&1; then fail "sp status answered with no copy running"; fi
[ ! -e "$SOFTPRESENTER_CONTROL_SOCKET" ] || fail "sp status started a copy"
ok "/usr/bin/sp status with nothing running: $(head -1 "$work/none.out")"

# The command line card: the package's sp is the one, and nothing is added to the home folder.
mkdir -p "$work/home"
said=$(HOME="$work/home" /usr/bin/softpresenter install-cli 2>&1) || fail "install-cli: $said"
case "$said" in
"Installed in /usr/bin."*) ;;
*) fail "install-cli said: $said" ;;
esac
[ ! -e "$work/home/.local" ] && [ -z "$(ls -A "$work/home")" ] || fail "install-cli wrote in the home folder"
ok "install-cli: $said"

# The app, as the menu entry starts it (Exec=softpresenter, through /usr/bin's link: with
# nothing answering, it opens), with a home folder of its own too. Its socket is the one in its
# data folder, which every sp command below names.
env -u SOFTPRESENTER_CONTROL_SOCKET HOME="$work/home" /usr/bin/softpresenter >"$work/app.out" 2>"$work/app.err" &
app=$!
started=$(date +%s)
until /usr/bin/sp status --json >"$work/status.json" 2>/dev/null; do
    kill -0 "$app" 2>/dev/null || fail "the app stopped before it answered"
    [ $(($(date +%s) - started)) -lt 120 ] || fail "the app didn't answer within 2 minutes"
    sleep 0.5
done
ok "the app answered sp status after $(($(date +%s) - started)) s"
line=$(grep -m1 -o '\[start\] SoftPresenter .*' "$work/data/SoftPresenter.log" || true)
[ -n "$line" ] || fail "the log has no start line"
if [ -n "$expected" ]; then
    echo "$line" | grep -qF "SoftPresenter $expected on linux x86_64" || fail "the log says \"$line\", not $expected"
fi
ok "the log says: $line"

# A song on screen.
printf 'Test Hymn\n\nVerse 1\nFirst line of the test verse\nSecond line of the test verse\n\nChorus\nThe test chorus line\n' >"$work/test-hymn.txt"
/usr/bin/sp song import "$work/test-hymn.txt" >/dev/null || fail "sp song import"
for _ in $(seq 1 20); do /usr/bin/sp go "Test Hymn" 1 >/dev/null 2>&1 && break; sleep 0.5; done
/usr/bin/sp go "Test Hymn" 1 >/dev/null || fail "sp go"
/usr/bin/sp status --json >"$work/status.json" || fail "sp status"
grep -q '"song": "Test Hymn"' "$work/status.json" || fail "the song isn't on screen: $(cat "$work/status.json")"
ok "a song is on screen: $(/usr/bin/sp status | head -1)"

# A video behind it, from the sample backgrounds (found through the package's link), played by
# the package's own FFmpeg, not one the computer may have in /usr/bin.
/usr/bin/sp media samples >/dev/null || fail "sp media samples"
/usr/bin/sp media show "Drifting Light" >/dev/null || fail "sp media show"
seen=""
for _ in $(seq 1 60); do
    for child in $(ps -o pid= --ppid "$app"); do
        exe=$(readlink "/proc/$child/exe" 2>/dev/null || true)
        case "$exe" in */ffmpeg) seen=$exe ;; esac
    done
    [ -n "$seen" ] && /usr/bin/sp video seek 2 >/dev/null 2>&1 && break
    sleep 0.5
done
[ "$seen" = "$lib/ffmpeg" ] || fail "the video isn't played by $lib/ffmpeg (found: ${seen:-no ffmpeg})"
/usr/bin/sp video seek 2 >/dev/null 2>&1 || fail "the video's length never came (ffprobe)"
/usr/bin/sp status --json | grep -q '"media": "Drifting Light' || fail "the video isn't behind the words"
ok "a video plays through $seen: $(/usr/bin/sp video seek 2)"

# Quit, as a person would, and the copy leaves nothing wrong behind.
/usr/bin/sp clear all >/dev/null || fail "sp clear all"
/usr/bin/sp menu Quit >/dev/null || fail "sp menu Quit"
for _ in $(seq 1 60); do kill -0 "$app" 2>/dev/null || break; sleep 0.5; done
if kill -0 "$app" 2>/dev/null; then fail "the app hadn't quit 30 seconds after menu Quit"; fi
status=0
wait "$app" || status=$?
app=""
[ "$status" = 0 ] || fail "the app quit with exit code $status"
if grep -E '\[crash\]|panicked|Graphics error' "$work/data/SoftPresenter.log" "$work/app.err"; then
    fail "the log or the standard error says something went wrong (above)"
fi
[ ! -e "$SOFTPRESENTER_CONTROL_SOCKET" ] || fail "the control socket was left behind"
ok "the app quit cleanly (exit code 0, nothing wrong in the log, the socket gone)"
passed=1
