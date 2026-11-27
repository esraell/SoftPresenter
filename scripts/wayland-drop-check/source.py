#!/usr/bin/env python3
"""A window that is a drag source, as a file manager is: press on it and drag.
  source.py SPEC      SPEC is a file read as each drag begins: "files" then a path a line
                      (handed over as a GdkFileList, as GNOME Files does), or "raw" then one
                      file, whose bytes are handed over as text/uri-list as they are
Prints the pointer's place on it, and how each drag ended."""
import sys
import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gio, GLib, GObject, Gtk

spec = sys.argv[1]

def provider():
    # What to hand over is read as each drag begins: the first line "files" or "raw", then
    # the paths (for "raw", one file whose bytes are handed over as text/uri-list as they are).
    with open(spec, encoding="utf-8") as f:
        mode, *args = f.read().splitlines()
    if mode == "raw":
        with open(args[0], "rb") as f:
            return Gdk.ContentProvider.new_for_bytes("text/uri-list", GLib.Bytes.new(f.read()))
    files = Gdk.FileList.new_from_list([Gio.File.new_for_path(p) for p in args])
    value = GObject.Value(Gdk.FileList, files)
    return Gdk.ContentProvider.new_for_value(value)

def activate(app):
    win = Gtk.ApplicationWindow(application=app, title="drag source")
    win.set_default_size(260, 200)
    label = Gtk.Label(label="drag from here")
    label.set_hexpand(True)
    label.set_vexpand(True)
    src = Gtk.DragSource()
    src.set_actions(Gdk.DragAction.COPY)
    src.connect("prepare", lambda s, x, y: provider())
    src.connect("drag-begin", lambda s, d: print("drag-begin", flush=True))
    src.connect("drag-end", lambda s, d, delete: print("drag-end", flush=True))
    src.connect("drag-cancel", lambda s, d, reason: print("drag-cancel", reason.value_nick, flush=True) or False)
    label.add_controller(src)
    motion = Gtk.EventControllerMotion()
    motion.connect("motion", lambda c, x, y: print("motion %.1f %.1f" % (x, y), flush=True))
    label.add_controller(motion)
    # The clipboard: a right click puts the words of the file beside the spec ("clip") on
    # it; whenever the window comes to the front it says what the clipboard holds.
    clipboard = Gdk.Display.get_default().get_clipboard()
    def put(gesture, n, x, y):
        with open(spec + ".clip", encoding="utf-8") as f:
            words = f.read().strip()
        clipboard.set(words)
        print("clipboard set: " + words, flush=True)
    right = Gtk.GestureClick()
    right.set_button(3)
    right.connect("pressed", put)
    label.add_controller(right)
    def said(source, result):
        try:
            print("clipboard holds: " + str(clipboard.read_text_finish(result)), flush=True)
        except GLib.Error as e:
            print("clipboard holds: (unreadable: %s)" % e.message, flush=True)
    def active(window, _):
        if window.is_active():
            clipboard.read_text_async(None, said)
    win.connect("notify::is-active", active)
    win.set_child(label)
    win.present()
    print("shown", flush=True)

app = Gtk.Application(application_id="org.softpresenter.DragSource", flags=Gio.ApplicationFlags.NON_UNIQUE)
app.connect("activate", activate)
app.run([])
