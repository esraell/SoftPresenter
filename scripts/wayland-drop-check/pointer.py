#!/usr/bin/env python3
"""Moves the private compositor's pointer and presses its button, through Mutter's
RemoteDesktop on the session bus given (never the desktop's own bus).
Reads commands on standard input:  home | move DX DY | press | release | key CODE down|up | sleep S"""
import sys, time
import gi
from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
def call(path, iface, method, args=None, reply=None):
    return bus.call_sync("org.gnome.Mutter.RemoteDesktop", path, iface, method, args, reply,
                         Gio.DBusCallFlags.NONE, 5000, None)
(session,) = call("/org/gnome/Mutter/RemoteDesktop", "org.gnome.Mutter.RemoteDesktop", "CreateSession",
                  None, GLib.VariantType("(o)")).unpack()
S = "org.gnome.Mutter.RemoteDesktop.Session"
call(session, S, "Start")
def move(dx, dy):
    call(session, S, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (float(dx), float(dy))))
print("ready", flush=True)
for line in sys.stdin:
    words = line.split()
    if not words:
        continue
    if words[0] == "home":
        for _ in range(8):
            move(-1000, -1000)
            time.sleep(0.02)
    elif words[0] == "move":
        dx, dy = float(words[1]), float(words[2])
        steps = max(1, int(max(abs(dx), abs(dy)) // 10))
        for _ in range(steps):
            move(dx / steps, dy / steps)
            time.sleep(0.01)
    elif words[0] == "press":
        call(session, S, "NotifyPointerButton", GLib.Variant("(ib)", (272, True)))
    elif words[0] == "release":
        call(session, S, "NotifyPointerButton", GLib.Variant("(ib)", (272, False)))
    elif words[0] == "rclick":
        for down in (True, False):
            call(session, S, "NotifyPointerButton", GLib.Variant("(ib)", (273, down)))
            time.sleep(0.05)
    elif words[0] == "key":
        call(session, S, "NotifyKeyboardKeycode", GLib.Variant("(ub)", (int(words[1]), words[2] == "down")))
    elif words[0] == "sleep":
        time.sleep(float(words[1]))
    print("did", line.strip(), flush=True)
call(session, S, "Stop")
