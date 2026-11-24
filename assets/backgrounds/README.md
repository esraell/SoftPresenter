# Sample backgrounds

Seven backgrounds for slides, which SoftPresenter carries with it: five stills (Still, First
Light, Night Blue, Evening Hills, Olive), a stand-in for a camera shot behind a livestream's
lower third (Room Out of Focus) and a 16-second loop of two soft lights drifting over charcoal
(Drifting Light, 1920 × 1080, H.264, no sound).

They were drawn by the project's own script, [`make_backgrounds.py`](make_backgrounds.py): light
added to a dark field, then a fine grain. No photograph, stock picture or outside generator went
into them. Run the script again (it needs Python with numpy and Pillow, and ffmpeg for the video)
and it draws the same files.

## Licence

**CC0 1.0 Universal** (public domain dedication,
<https://creativecommons.org/publicdomain/zero/1.0/>). The owner of the project chose it so that
the files can ship in the app and churches can use them anywhere without asking. This applies to
the pictures and the video, and to `make_backgrounds.py`.
[docs/licences/backgrounds.md](../../docs/licences/backgrounds.md) has the note.
