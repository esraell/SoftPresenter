# SoftPresenter Sans: the typeface slides are set in

SoftPresenter sets slide text in SoftPresenter Sans unless a theme or a slide names another
typeface. It is built into the program, four files (Regular, Bold, Italic and Bold Italic) in
[`crates/sp-render/fonts`](../../crates/sp-render/fonts), so Windows, Linux and the Mac all have
it with nothing to install, and slides wrap the same way on every computer.

## The licence

SoftPresenter Sans is a renamed copy of Libertinus Sans 7.051
(<https://github.com/alerque/libertinus>), copyright the Libertinus Project Authors, with one
addition: a Bold Italic, made by slanting the Bold. Libertinus Sans continues Linux Biolinum by
Philipp H. Poll. It is licensed under the SIL Open Font License 1.1, which allows the font to be
used, changed and shared, alone or bundled with software, and can be combined with the GNU GPL
version 3.

The licence asks three things, which are done here:

- **The notices stay.** [`OFL.txt`](../../crates/sp-render/fonts/OFL.txt) (the licence, with the
  copyright lines) and [`AUTHORS.txt`](../../crates/sp-render/fonts/AUTHORS.txt) sit beside the
  font files, and the Mac app carries both in `Contents/Resources/SoftPresenterSans`.
- **The name is not the original's.** The Reserved Font Names are "Linux Libertine", "Biolinum"
  and "STIX Fonts", so the family is called SoftPresenter Sans and never by those names.
- **It stays under the OFL.** The font files are not relicensed; the rest of SoftPresenter is
  under the GNU GPL.

[NOTICE.md](../../NOTICE.md) names it.

## How it is used

`sp_render::Fonts` registers the four files in the font collection it starts from, so the family
"SoftPresenter Sans" is there before the computer's own fonts are found (the screens draw with
it from their first picture) and on a computer that has no fonts at all. Its data is part of the
program, so nothing is read from a file. The picture tests in `crates/sp-render/tests/golden.rs`
use only this typeface (`Fonts::bundled_only`), so their stored pictures do not depend on the
computer's fonts.
