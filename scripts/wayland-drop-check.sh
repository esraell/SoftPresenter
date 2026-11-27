#!/bin/bash
# Checks that files dragged from another program onto SoftPresenter's window come in on a
# Wayland desktop, and that copy and paste still work in its windows: the two things the changes
# carried in third_party/smithay-clipboard are there for (its PROVENANCE.md). Run it on Linux
# after an update of egui, winit or that copy.
#
#   scripts/wayland-drop-check.sh [path to the softpresenter program]
#                                 (default: target/debug/softpresenter)
#
# Everything it starts runs at the lowest priority (nice 19), and is stopped and removed
# however a run ends. No one needs to be at the desk and nothing of the desktop's own session is used: each run
# starts a private headless GNOME Shell (its own runtime folder, home, session bus and Wayland
# socket, all under a folder in /tmp that is removed afterwards), a quiet test copy of the app
# with its own data folder, a GTK 4 window as the drag source, and moves the pointer and
# presses keys through that shell's own RemoteDesktop. It needs gnome-shell (Mutter 46 or
# newer, for --headless and --virtual-monitor), dbus-run-session, python3 with PyGObject and
# GTK 4.
#
# Three runs, about a minute each:
#   drags      two pictures in one drop, a list with a comment and a web address, files held
#              over the window and taken away, a web address alone, drops down the window
#   windows    drops before Settings is opened, while it is open and after it is closed
#   clipboard  copy and paste both ways in the operator's window and in the Screens window,
#              then a drop
# The places the pointer goes to are for the 1920x1200 display the private shell is given
# and the windows as they open today; if a window's layout changes, drags.sh says where.
#
# More by hand (see --one below): SCALE=1.5 for a scaled display, KEEP=1 to keep the folder
# in /tmp, where app.err holds every Wayland message the copy sent and was sent (the runs read
# it to find a window and to count the copy's data devices).
set -u
HERE=$(cd "$(dirname "$0")" && pwd)/wayland-drop-check
APP=${1:-$(cd "$HERE/../.." && pwd)/target/debug/softpresenter}
[ -x "$APP" ] || { echo "no program at $APP: build it first (cargo build -p sp-app), or give its path"; exit 2; }
APP=$(cd "$(dirname "$APP")" && pwd)/$(basename "$APP")
for need in gnome-shell dbus-run-session python3; do
  command -v "$need" > /dev/null || { echo "$need is needed and was not found"; exit 2; }
done

# One run in a private desktop: its output on standard output.
one() { # the case ("" for the plain drags)
  local T code
  T=$(mktemp -d /tmp/sp-wl.XXXXXX) || return 2
  # The folder goes however this ends; a signal waits for the run below to tidy up first.
  [ -n "${KEEP:-}" ] || trap "rm -rf '$T'" EXIT
  trap 'exit 130' INT TERM HUP
  mkdir -m 700 "$T/run" "$T/home" "$T/data" "$T/photos"
  env -i PATH="$PATH" HOME="$T/home" XDG_RUNTIME_DIR="$T/run" LANG=C.UTF-8 T="$T" HERE="$HERE" APP="$APP" \
    DOWN="${DOWN:-}" DOWNX="${DOWNX:-}" SCALE="${SCALE:-1}" CASE="$1" TRACE=1 KEYS=1 CMD="${CMD:-}" \
    REALHOME="$HOME" nice -n 19 dbus-run-session -- timeout --foreground "${WITHIN:-300}" bash "$HERE/inside.sh" 2>&1 \
    | grep -v 'dbus-daemon\|portal\|evolution\|calendar-server\|goa-\|SpiRegistry\|^$'
  code=${PIPESTATUS[0]}
  if [ -n "${KEEP:-}" ]; then echo "kept: $T"; else rm -rf "$T"; trap - EXIT; fi
  return "$code"
}
# `wayland-drop-check.sh APP --one CASE` is one run with its whole output (CASE: drags,
# panel-after, clipboard, focus, panel); CMD=... runs a command in the private desktop instead.
if [ "${2:-}" = --one ]; then
  case=${3:-drags}; [ "$case" = drags ] && case=""
  one "$case"; exit $?
fi

failed=0
judge() { # name, the run's output, then pairs: a pattern and how many lines must hold it
  local name=$1 out=$2 bad="" pattern want got; shift 2
  while [ $# -gt 0 ]; do
    pattern=$1 want=$2; shift 2
    got=$(printf '%s\n' "$out" | grep -c -- "$pattern")
    [ "$got" = "$want" ] || bad="$bad
    wanted $want of \"$pattern\", saw $got"
  done
  if [ -z "$bad" ]; then echo "PASSED  $name"; else echo "FAILED  $name$bad"; printf '%s\n' "$out" | sed 's/^/    | /'; failed=1; fi
}

out=$(one "")
judge "drags" "$out" \
  "2 dropped at" 1  "left out 1 of the 2 things dragged" 1  "1 dropped at" 7 \
  "on Some(MediaStrip)" 3  "they left the window without being dropped" 3 \
  "crashes in the copy's log: 0" 1  "the copy's exit code: 0" 1
out=$(one panel-after)
judge "windows" "$out" \
  "dropped at" 5  "on Some(MediaStrip)" 1  "crashes in the copy's log: 0" 1  "the copy's exit code: 0" 1
out=$(one clipboard)
judge "clipboard" "$out" \
  "PASSED both ways" 3  "FAILED" 0  "1 dropped at" 1  "crashes in the copy's log: 0" 1  "the copy's exit code: 0" 1
[ "$failed" = 0 ] && echo "all three passed" || echo "something failed: see above"
exit $failed
