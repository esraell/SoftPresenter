#!/usr/bin/env python3
"""Sets the private compositor's one display to a scale (2, 1.5...), through Mutter's
DisplayConfig on the session bus given, and prints what it is afterwards."""
import sys
from gi.repository import Gio, GLib

want = float(sys.argv[1])
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
def call(method, args=None):
    return bus.call_sync("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig",
                         "org.gnome.Mutter.DisplayConfig", method, args, None, Gio.DBusCallFlags.NONE, 5000, None)
serial, monitors, logical, props = call("GetCurrentState").unpack()
(connector, *_), modes, _ = monitors[0]
mode = next(m for m in modes if m[6].get("is-current"))
scale = min(mode[5], key=lambda s: abs(s - want))
config = [(0, 0, scale, 0, True, [(connector, mode[0], {})])]
call("ApplyMonitorsConfig", GLib.Variant("(uua(iiduba(ssa{sv}))a{sv})", (serial, 2, config, {})))
_, _, logical, _ = call("GetCurrentState").unpack()
print("display %s: mode %s, scale %s (asked %s), of %s" % (connector, mode[0], logical[0][2], want, list(mode[5])))
