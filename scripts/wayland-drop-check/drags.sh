#!/bin/bash
# The drags themselves (see ../wayland-drop-check.sh). The pointer starts at the screen's top
# left corner. Places are in the screen's points, on a 1920x1200 display, for the windows as
# they open in a quiet copy: the operator's window's corner at 240,201, its media strip around
# 900,950 to 900,1050, its Filter field at 48,862 in the window, Settings' Close at 520,900 and
# the Screens window's Name field at 545,299 in that window.
set -u
LOG="$T/data/SoftPresenter.log"
coproc PTR { python3 "$HERE/pointer.py"; }
read -r ready <&"${PTR[0]}"
ptr() { echo "$*" >&"${PTR[1]}"; read -r did <&"${PTR[0]}"; }

# The source window stays for all the drags (a window opened after a click elsewhere is not
# put in front). It is found by sweeping the pointer until it says the pointer is on it.
: > "$T/spec"
python3 "$HERE/source.py" "$T/spec" > "$T/source.out" 2> "$T/source.err" &
SRC=$!
for i in $(seq 1 40); do grep -q shown "$T/source.out" && break; sleep 0.25; done
sleep 1.5
ptr home
X=0; Y=0; SX=""; SY=""
for gy in 560 660 460 360 760 260 860 160 960 60 1060 1160 1260 1360 1460 1560 1660 1760; do
  for gx in $(seq 60 100 2860); do
    ptr move $((gx - X)) $((gy - Y)); X=$gx; Y=$gy
    sleep 0.05
    if grep -q "^motion" "$T/source.out"; then SX=$gx; SY=$gy; break 2; fi
  done
done
[ -n "$SX" ] || { echo "the source window was never under the pointer"; cat "$T/source.err"; kill $SRC; exit 1; }
# How far the pointer goes for one unit asked of it: on a scaled display it is asked in the
# display's own pixels, and the places below are in the screen's points.
at() { grep -a '^motion' "$T/source.out" | tail -1 | cut -d' ' -f2; }
A=$(at); ptr move 30 0; sleep 0.3; B=$(at); ptr move -30 0
K=$(python3 -c "import sys; print(round(30 / (float(sys.argv[2]) - float(sys.argv[1])), 4))" "$A" "$B")
units() { python3 -c "import sys; print(round(float(sys.argv[1]) * $K))" "$1"; }
echo "the source window is under the pointer at screen $(python3 -c "print(round($SX / $K), round($SY / $K))") (in it: $(grep -a '^motion' "$T/source.out" | tail -1)); $K pointer units to a point"

# One drag from the source window to a place on the screen, in its pixels.
drag() { # mode, to-x, to-y, how (drop|leave), files...
  local mode=$1 tx=$(units "$2") ty=$(units "$3") how=$4; shift 4
  { echo "$mode"; printf '%s\n' "$@"; } > "$T/spec"
  : > "$T/source.out"
  ptr move $((SX - X)) $((SY - Y)); X=$SX; Y=$SY
  sleep 0.3
  ptr press
  ptr move 12 12
  ptr move 12 12
  X=$((X + 24)); Y=$((Y + 24))
  sleep 0.3
  ptr move $((tx - X)) $((ty - Y)); X=$tx; Y=$ty
  sleep 0.5
  ptr move 6 0; ptr move -6 0
  sleep 1.0
  if [ "$how" = leave ]; then
    # Up onto the top bar and let go there: the files leave the app's window undropped.
    ptr move 0 $((0 - Y)); Y=0
    sleep 0.5
  fi
  ptr release
  sleep 1.5
  echo "  the source: $(grep -av '^motion' "$T/source.out" | tr -d '\000' | tr '\n' ' ')"
  echo "  the app:    $(sed -n "$((SEEN + 1)),\$p" "$LOG" | grep -c '\[drop\]') new [drop] lines"
  sed -n "$((SEEN + 1)),\$p" "$LOG" | grep '\[drop\]\|\[media\]' | sed 's/^[^[]*/    /'
  SEEN=$(wc -l < "$LOG")
}
SEEN=$(wc -l < "$LOG")

# Brings the source window back in front after another window has come over it: the next
# window is asked for (Alt+Escape) until the source says the pointer is on it again.
to_the_source() {
  local try before
  for try in 1 2 3 4 5; do
    before=$(grep -ac '^motion' "$T/source.out")
    ptr move $((SX - X + 3)) $((SY - Y)); X=$((SX + 3)); Y=$SY
    ptr move -3 0; X=$SX
    sleep 0.4
    [ "$(grep -ac '^motion' "$T/source.out")" -gt "$before" ] && return 0
    ptr key 56 down; ptr key 1 down; ptr key 1 up; ptr key 56 up; sleep 1
  done
  echo "  the source window could not be brought back in front"
}
devices() { # which of the copy's data devices heard of the drags since the last look
  [ -n "${TRACE:-}" ] || return 0
  echo "  the copy's data devices: $(grep -a -o 'get_data_device(new id wl_data_device#[0-9]*' "$T/app.err" | grep -o '#[0-9]*' | tr '\n' ' ')"
  echo "  entered: $(sed -n "$((ERRSEEN + 1)),\$p" "$T/app.err" | grep -a -o 'wl_data_device#[0-9]*\.enter' | sort | uniq -c | tr '\n' ' ')"
  ERRSEEN=$(wc -l < "$T/app.err")
}
ERRSEEN=0
chord() { # keys held together, by their codes: chord 29 47 is Ctrl+V
  local k; for k in "$@"; do ptr key "$k" down; done
  local i; for ((i=$#; i>0; i--)); do ptr key "${!i}" up; done
  sleep 0.4
}
click() { ptr move $(( $(units "$1") - X )) $(( $(units "$2") - Y )); X=$(units "$1"); Y=$(units "$2"); sleep 0.2; ptr press; ptr release; sleep 0.6; }
# Copy and paste, both ways, in a text field of the app that has just been clicked: the source
# puts WORDS on the clipboard; pasted over what the field holds, an "x" typed after them, all
# of it copied; then the source, brought to the front, says what the clipboard holds.
round_trip() { # words, field-x, field-y
  echo "$1" > "$T/spec.clip"
  to_the_source
  : > "$T/source.out"
  ptr move $((SX - X)) $((SY - Y)); X=$SX; Y=$SY; sleep 0.2
  ptr rclick; sleep 0.6
  echo "  the source: $(grep -a 'clipboard set' "$T/source.out" | tr -d '\000')"
  click "$2" "$3"
  chord 29 30; chord 29 47; chord 45; chord 29 30; chord 29 46
  : > "$T/source.out"
  to_the_source; sleep 1
  local holds; holds=$(grep -a 'clipboard holds' "$T/source.out" | tail -1 | tr -d '\000')
  echo "  the source: $holds"
  [ "$holds" = "clipboard holds: ${1}x" ] && echo "  PASSED both ways" || echo "  FAILED"
}
if [ "${CASE:-}" = clipboard ]; then
  # The operator's window: its Filter field, at the lower left (48, 862 in the window).
  echo "== a: the operator's window"
  round_trip "from the source 1" $((240 + 48)) $((201 + 862))
  echo "== b: the Screens window opened; its Name field"
  SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" "$APP" menu "Configure Screens…" > /dev/null
  sleep 2.5
  to_the_source
  # Where the Screens window is: a drag held over it says where the pointer is on it.
  drag files 700 420 drop "$T/photos/third.png" > /dev/null
  AT=$(grep -a -o 'wl_data_device#[0-9]*\.motion([0-9]*, [0-9.]*, [0-9.]*' "$T/app.err" | tail -1 | sed 's/.*motion([0-9]*, //; s/,//')
  PX=$(python3 -c "import sys; print(round(700 - float(sys.argv[1])))" $AT); PY=$(python3 -c "import sys; print(round(420 - float(sys.argv[2])))" $AT)
  echo "  the Screens window's corner is at screen $PX,$PY"
  round_trip "from the source 2" $((PX + 545)) $((PY + 299))
  echo "  the screen is now called: $(SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" "$APP" screens 2>&1 | head -3 | tr '\n' ' ')"
  echo "== c: the operator's window again, the Screens window still open"
  round_trip "from the source 3" $((240 + 48)) $((201 + 862))
  echo "== d: a drop on the operator's window after all that"
  to_the_source
  drag files 300 700 drop "$T/photos/third.png"
  kill $SRC 2>/dev/null
  echo "== crashes in the copy's log: $(grep -c 'crash\|panicked\|Graphics error' "$LOG")"
  exec {PTR[1]}>&-
  wait
  exit 0
fi
if [ -n "${CASE:-}" ]; then
  echo "== a: two pictures dropped on the left of the operator's window ($CASE)"
  drag files 300 700 drop "$T/photos/Easter morning.png" "$T/photos/Пасха été.png"
  devices
  if [ "$CASE" = focus ]; then
    for round in 1 2; do
      echo "== b$round: the operator's window clicked (it takes the keyboard), Alt+Tab back, then a drop"
      ptr move $(( $(units 300) - X )) $(( $(units 400) - Y )); X=$(units 300); Y=$(units 400)
      ptr press; ptr release; sleep 1
      ptr key 56 down; ptr key 15 down; ptr key 15 up; ptr key 56 up; sleep 1.5
      drag files 300 700 drop "$T/photos/third.png"
      devices
    done
  fi
  if [ "$CASE" = panel ]; then
    echo "== b: the same again with the panel still open"
    drag files 300 760 drop "$T/photos/third.png"
    devices
  fi
  if [ "$CASE" = panel-after ]; then
    echo "== b: Settings opened (a second window, with a clipboard of its own), Alt+Tab back to the source, then a drop"
    SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" "$APP" menu "Settings…" > /dev/null
    sleep 2.5
    ptr key 56 down; ptr key 15 down; ptr key 15 up; ptr key 56 up; sleep 1.5
    drag files 300 760 drop "$T/photos/third.png"
    devices
    echo "== c: once more"
    drag files 300 700 drop "$T/photos/third.png"
    devices
    echo "== d: Settings closed (a click on its Close at the lower left), then a drop"
    ptr move $(( $(units 520) - X )) $(( $(units 900) - Y )); X=$(units 520); Y=$(units 900)
    ptr press; ptr release; sleep 0.7
    sleep 1.5
    to_the_source
    [ -z "${TRACE:-}" ] || echo "  windows closed so far: $(grep -a -c -- '-> xdg_toplevel#[0-9]*\.destroy' "$T/app.err"), data devices let go: $(grep -a -c -- '-> wl_data_device#[0-9]*\.release' "$T/app.err")"
    drag files 300 700 drop "$T/photos/third.png"
    devices
    echo "== e: and onto the media strip"
    drag files 900 1000 drop "$T/photos/third.png"
    devices
  fi
  kill $SRC 2>/dev/null
  echo "== crashes in the copy's log: $(grep -c 'crash\|panicked\|Graphics error' "$LOG")"
  exec {PTR[1]}>&-
  wait
  exit 0
fi
echo "== 1: two pictures dropped on the left of the window"
drag files 300 700 drop "$T/photos/Easter morning.png" "$T/photos/Пасха été.png"
echo "== 2: a raw list with a comment, a web address and a bare line feed"
printf '# two entries\r\nhttps://example.org/sky.png\r\nfile://%s\n' "$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$T/photos/third.png")" > "$T/list.txt"
drag raw 1500 400 drop "$T/list.txt"
echo "== 3: held over the window, then taken away undropped"
drag files 300 400 leave "$T/photos/third.png"
echo "== 4: a web address alone"
printf 'https://example.org/sky.png\r\n' > "$T/web.txt"
drag raw 300 400 drop "$T/web.txt"
echo "== 5: dropped down a line, to see the places under the pointer"
for y in ${DOWN:-400 900 950 1000 1050 1100 1150 1190}; do
  echo "screen ${DOWNX:-900},$y"
  drag files "${DOWNX:-900}" "$y" drop "$T/photos/third.png"
done
kill $SRC 2>/dev/null
grep '\[window\]\|scale' "$LOG" | head -3
echo "== crashes in the copy's log: $(grep -c 'crash\|panicked\|Graphics error' "$LOG")"
echo "== the media bin"
SOFTPRESENTER_DATA="$T/data" SOFTPRESENTER_CONTROL_SOCKET="$T/s" "$APP" media
exec {PTR[1]}>&-
wait
