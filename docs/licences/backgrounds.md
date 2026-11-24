# Sample backgrounds

A working note, not legal advice.

SoftPresenter carries seven sample backgrounds in [`assets/backgrounds`](../../assets/backgrounds):
`Still.jpg`, `First Light.jpg`, `Night Blue.jpg`, `Evening Hills.jpg`, `Olive.jpg`,
`Room Out of Focus.jpg` and `Drifting Light.mp4`.

## Where they come from

The project drew them itself, with [`make_backgrounds.py`](../../assets/backgrounds/make_backgrounds.py)
in the same folder: light added to a dark field, then a fine grain, and for the video two soft
lights moving on closed paths, encoded with ffmpeg. No photograph, stock picture, clip or
outside generator was used, and nothing was copied from another program's artwork.

## The licence

**CC0 1.0 Universal**, a public domain dedication (<https://creativecommons.org/publicdomain/zero/1.0/>).
The owner chose it, so the files can ship in the app and churches can use them anywhere without
asking, and the same goes for the script. It asks nothing of anyone who uses them, and it can be
combined with the GNU GPL version 3 that the rest of SoftPresenter is under.

## Where they go

- The Mac app copies the folder into `Contents/Resources/Backgrounds` (`scripts/macos-app.sh`).
- Nothing adds them to anyone's media bin; a church picks one where the app offers them.
- The program finds the folder with `crates/sp-app/src/backgrounds.rs`.

[NOTICE.md](../../NOTICE.md) names them.
