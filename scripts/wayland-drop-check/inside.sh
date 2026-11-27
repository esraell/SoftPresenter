#!/bin/bash
# Runs inside the private session bus (see ../wayland-drop-check.sh).
set -u
# Whichever way this ends (done, a step failed, out of time, Ctrl-C), everything it started
# goes with it: the copy, the keyboard, the drags and their helpers, and the private shell last.
SHELL_PID=""
below() { local child; for child in $(pgrep -P "$1"); do below "$child"; echo "$child"; done; }
tidy() {
  trap - EXIT INT TERM HUP
  local all child
  all=$(below $$)
  for child in $all; do [ "$child" = "$SHELL_PID" ] || kill "$child" 2>/dev/null; done
  [ -z "$SHELL_PID" ] || { sleep 0.5; kill "$SHELL_PID" 2>/dev/null; wait "$SHELL_PID" 2>/dev/null; }
  for child in $all; do kill -9 "$child" 2>/dev/null; done
}
trap tidy EXIT
trap 'exit 130' INT TERM HUP
# The display is 1920x1200 after scaling, whatever the scale, so the places are the same.
SCALE=${SCALE:-1}
SIZE=$(python3 -c "import sys; s=float(sys.argv[1]); print('%dx%d' % (round(1920*s), round(1200*s)))" "$SCALE")
gnome-shell --headless --wayland-display=sp-test-0 --virtual-monitor "$SIZE" --no-x11 > "$T/shell.log" 2>&1 &
SHELL_PID=$!
for i in $(seq 1 60); do [ -S "$T/run/sp-test-0" ] && break; sleep 0.5; done
[ -S "$T/run/sp-test-0" ] || { echo "the private compositor did not start"; tail -5 "$T/shell.log"; exit 3; }
sleep 3
[ "$SCALE" = 1 ] || { python3 "$HERE/scale.py" "$SCALE" || exit 6; sleep 2; }
export WAYLAND_DISPLAY=sp-test-0
# KEYS=1: the seat has a keyboard before the copy starts, as at a real desk.
KEYS_PID=""
if [ -n "${KEYS:-}" ]; then python3 "$HERE/keyboard.py" > "$T/keys.out" 2>&1 & KEYS_PID=$!; sleep 2; fi
# Two small pictures, one with a space and one with letters outside ASCII in its name.
python3 - "$T/photos" <<'PY'
import sys, struct, zlib
def png(path, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * 8 for _ in range(8))
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 8, 8, 8, 2, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
png(sys.argv[1] + "/Easter morning.png", (200, 160, 60))
png(sys.argv[1] + "/Пасха été.png", (60, 120, 200))
png(sys.argv[1] + "/third.png", (60, 200, 120))
PY
# CMD=...: run that inside the private desktop instead of the drags (the end-to-end tests,
# say), with the real home folder for the build tools; its windows open here, not at the desk.
# (Started with & and waited for, here and for the drags, so that a signal is acted on at once.)
if [ -n "${CMD:-}" ]; then
  HOME="$REALHOME" bash -c "$CMD" & wait $!
  exit $?
fi
# TRACE=1 has the copy write every Wayland message it sends and is sent to its error file.
${TRACE:+env WAYLAND_DEBUG=1} env SOFTPRESENTER_QUIET=1 SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" SOFTPRESENTER_LANGUAGE=en \
  "$APP" > "$T/app.out" 2> "$T/app.err" &
APP_PID=$!
sp() { SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" "$APP" "$@"; }
for i in $(seq 1 60); do sp status >/dev/null 2>&1 && break; sleep 0.5; done
sp status >/dev/null 2>&1 || { echo "the copy did not answer"; tail -5 "$T/app.err"; exit 4; }
sleep 2
# CASE=panel: the Settings window is opened first (a second window of the app, with a
# clipboard of its own on the same connection).
if [ "${CASE:-}" = panel ]; then sp menu "Settings…"; sleep 2; fi
export -f sp 2>/dev/null
bash "$HERE/drags.sh" & wait $!
code=$?
sp menu Quit > /dev/null 2>&1
for i in $(seq 1 40); do kill -0 $APP_PID 2>/dev/null || break; sleep 0.5; done
if kill -0 $APP_PID 2>/dev/null; then echo "the copy did not quit; killed"; kill -9 $APP_PID; code=5; fi
wait $APP_PID; echo "the copy's exit code: $?"
exit $code
