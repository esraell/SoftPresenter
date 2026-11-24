# virtual-display (test tool, never part of the app)

Makes a virtual display on a Mac with no second monitor, so a screen's window can be tried
across a display going and coming back. It uses `CGVirtualDisplay`, a private CoreGraphics
interface that open-source virtual display tools use, so it may stop working with a macOS
update. Nothing in the app uses it.

    clang -fobjc-arc -framework Foundation -framework CoreGraphics -o /tmp/vdisplay spikes/virtual-display/vdisplay.m
    /tmp/vdisplay run "SP Test Display" 30       # a 1920x1080 display for 30 s, then it goes
    /tmp/vdisplay displays                       # active displays and their frames
    /tmp/vdisplay windows softpresenter [pid]    # that program's windows: layer, on screen, bounds

`try.sh PATH-TO-softpresenter [real-level]` runs a quiet test copy against it (its own data
folder and socket, `SOFTPRESENTER_SCREENS_FILL` naming the virtual display, and with
`real-level` the screens' windows take the level above the menu bar): audience on, off, on, the
display removed, the display made again, saying where the windows are at each step. It ends the
app itself and the display ends on its own timer. The virtual display is nobody's monitor, so a
run covers nothing anyone looks at; keep runs short all the same.
