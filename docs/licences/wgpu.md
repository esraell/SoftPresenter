# wgpu: the changed copies in `third_party/naga` and `third_party/wgpu-hal`

SoftPresenter draws through wgpu, the gfx-rs developers' graphics library
(<https://github.com/gfx-rs/wgpu>). Most of it comes from crates.io like any other library. Two
of its crates, `naga` 30.0.2 (which writes shaders in the language of the computer's graphics)
and `wgpu-hal` 30.0.2 (which speaks to the system's graphics), are kept in this repository with
changes, in [`third_party/naga`](../../third_party/naga) and
[`third_party/wgpu-hal`](../../third_party/wgpu-hal), and built in place of the published ones
(`[patch.crates-io]` in the workspace's `Cargo.toml`).

## The licence

Both are copyright the gfx-rs developers and licensed under MIT OR Apache-2.0, the user's
choice. Both licences can be combined with the GNU GPL version 3. Each copy keeps both licence
files (`LICENSE.MIT`, `LICENSE.APACHE`), and each change is marked in the source, as the Apache
licence asks of changed files. SoftPresenter's changes to those folders are offered under the
same two licences, so they can go back to wgpu.

They ship as part of the app like the rest of the code, so [NOTICE.md](../../NOTICE.md) names
them.

## What was changed, and why

[`third_party/naga/PROVENANCE.md`](../../third_party/naga/PROVENANCE.md) and
[`third_party/wgpu-hal/PROVENANCE.md`](../../third_party/wgpu-hal/PROVENANCE.md) have the exact
source and the list of changes. In short: the compiler of Intel's older Mac graphics aborts on
a line wgpu writes into every compute shader for Metal (how many threads a workgroup has), so
on such a Mac no graphics device could be made and the app did not start. For Intel's graphics
on a Mac the copies leave the line out and give Metal the same number another way. Nothing
changes for any other graphics.

wgpu has the fault as its issue 5827, with a fix proposed; when a version with a fix is out,
SoftPresenter moves to it and the copies go.
