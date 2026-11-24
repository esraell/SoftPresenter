#!/bin/sh
# Tries a screen's window across a display going and returning, on a virtual display (never a
# real one), with a quiet test copy of the app. Short and bounded: the display ends on its own
# timer and the app is killed by a watchdog if the script goes wrong.
#
#   spikes/virtual-display/try.sh PATH-TO-softpresenter [real-level]
#
# Build the helper first:  clang -fobjc-arc -framework Foundation -framework CoreGraphics \
#                            -o /tmp/vdisplay spikes/virtual-display/vdisplay.m
set -u
APP="$1"
LEVEL="${2:-}"
VD=/tmp/vdisplay
NAME="SP Test Display"
WORK=$(mktemp -d /tmp/spvd.XXXXXX)
SOCK="$WORK/c.sock"
export SOFTPRESENTER_QUIET=1
export SOFTPRESENTER_DATA="$WORK/data"
export SOFTPRESENTER_CONTROL_SOCKET="$SOCK"
export SOFTPRESENTER_SCREENS_FILL="$NAME"
[ "$LEVEL" = real-level ] && export SOFTPRESENTER_SCREENS_ON_TOP=1
mkdir -p "$SOFTPRESENTER_DATA"
sp() { "$APP" ctl "$@"; }
now() { python3 -c 'import time; print(time.time())'; }
shown() { "$VD" windows softpresenter $APP_PID | grep -c 'onscreen=1 bounds=1512,0 1920x1080'; }

"$VD" run "$NAME" 12 >"$WORK/vd1.out" 2>&1 &
sleep 2
"$VD" displays
"$APP" >"$WORK/app.out" 2>&1 &
APP_PID=$!
( sleep 110; kill -9 $APP_PID 2>/dev/null ) &
i=0; while [ ! -S "$SOCK" ] && [ $i -lt 100 ]; do sleep 0.2; i=$((i+1)); done
echo "--- displays and screens"; sp displays; sp screens
echo "--- audience on"
t0=$(now); sp audience on >/dev/null
i=0; while [ "$(shown)" = 0 ] && [ $i -lt 100 ]; do sleep 0.02; i=$((i+1)); done
t1=$(now); echo "window on the display after $(python3 -c "print(round(($t1-$t0)*1000))") ms"
"$VD" windows softpresenter $APP_PID
ids() { "$VD" windows softpresenter $APP_PID | grep "Screen 1" | awk '{print $2}' | tr '\n' ' '; }
sp test grid >/dev/null
echo "--- audience off (should hide at once)"
before=$(ids)
sp audience off >/dev/null; sleep 0.3; echo "shown on the display: $(shown)"
sp audience on >/dev/null; sleep 1; echo "shown again: $(shown)"
echo "window numbers before and after: [$before] [$(ids)] (the same: the window was kept and shown again)"
screencapture -x -R1512,0,640,360 "$WORK/after-off-on.png" && echo "picture saved: $WORK/after-off-on.png"
echo "--- waiting for the display to go (about 12 s after it was made)"
while [ "$(shown)" != 0 ]; do sleep 0.1; done
t2=$(now)
echo "window left the display ($(python3 -c "print(round(($t2-$t0)*1000))") ms into the run); its windows over the next 2 s (none should be on screen):"
for wait in 0.1 0.2 0.4 0.8 1.0; do
  sleep $wait
  echo "  +$(python3 -c "print(round(($(now)-$t2)*1000))") ms: $("$VD" windows softpresenter $APP_PID | tr '\n' ';')"
done
sleep 1; sp notices 2>&1 | head -5
echo "--- display back"
"$VD" run "$NAME" 8 >"$WORK/vd2.out" 2>&1 &
t3=$(now)
i=0; while [ "$(shown)" = 0 ] && [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done
t4=$(now); echo "window back after $(python3 -c "print(round(($t4-$t3)*1000))") ms"
sleep 1; sp notices 2>&1 | head -5
echo "window numbers: [$(ids)]"
screencapture -x -R1512,0,640,360 "$WORK/after-return.png" && echo "picture saved: $WORK/after-return.png"
echo "--- app log"
grep -h "screens" "$SOFTPRESENTER_DATA"/*.log "$SOFTPRESENTER_DATA"/log/* 2>/dev/null | tail -20
sp menu Quit >/dev/null 2>&1
sleep 1
kill -9 $APP_PID 2>/dev/null
wait 2>/dev/null
echo "done; $WORK"
