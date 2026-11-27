#!/usr/bin/env python3
"""Gives the private compositor's seat a keyboard from the start, as a real desktop's has
(a headless one has none until a key is first pressed): taps Shift, then stays."""
import time
from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
def call(path, iface, method, args=None, reply=None):
    return bus.call_sync("org.gnome.Mutter.RemoteDesktop", path, iface, method, args, reply,
                         Gio.DBusCallFlags.NONE, 5000, None)
(session,) = call("/org/gnome/Mutter/RemoteDesktop", "org.gnome.Mutter.RemoteDesktop", "CreateSession",
                  None, GLib.VariantType("(o)")).unpack()
S = "org.gnome.Mutter.RemoteDesktop.Session"
call(session, S, "Start")
for down in (True, False):
    call(session, S, "NotifyKeyboardKeycode", GLib.Variant("(ub)", (42, down)))
    time.sleep(0.1)
print("keyboard there", flush=True)
while True:
    time.sleep(60)
