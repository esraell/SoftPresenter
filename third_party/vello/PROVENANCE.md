# Where this copy of Vello comes from, and what was changed

This folder is the `vello` crate, version 0.12.0, as published on crates.io on 2026-11-11
(<https://crates.io/crates/vello/0.12.0>), built from commit
`b7e8f523b45c07a5edb806bb7241ef5afe34a882` of <https://github.com/linebender/vello>, where it
lives in `research/vello_research`. `.cargo_vcs_info.json` is the record crates.io ships with it.

Vello is copyright the Vello Authors and licensed under Apache-2.0 OR MIT; both licences are
here as `LICENSE-APACHE` and `LICENSE-MIT`, unchanged. SoftPresenter's changes are offered
under the same two licences, so the folder stays usable on Vello's own terms.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. `vello_encoding` and `vello_shaders` are still the published 0.12.0
crates, unchanged; no shader's source is changed. The shaders are compiled differently, though:
with bounds checks (see below).

## Why

Vello's compute stages write into scratch buffers (lines, tiles, segment counts, segments,
per-tile command lists) whose sizes must be set before a frame goes to the GPU. Vello 0.12
gives them constant sizes, "hand picked to accommodate the vello test scenes as well as
paris-30k" (`vello_encoding/src/config.rs`): about 165 MB, which every `Renderer` keeps for
good. It also never checks whether a frame outgrew them: such a frame is simply not drawn, and
`render_to_texture` still returns `Ok`.

For SoftPresenter that was wrong both ways. An ordinary slide needs a small part of that
memory, and the app has two renderers. And a slide full of small outlined text on a 4K screen
needs more lines and segments than the constants allow, so it was not drawn at all.

Upstream has nothing newer to move to: on 2026-11-20, `main` had the same constants and the
same unfinished check ("TODO: apply logic to determine whether we need to rerun coarse"), and
Vello's README still lists it among its limitations
(<https://github.com/linebender/vello/issues/366>).

## What was changed

Everything is marked `SoftPresenter:` in the source. Removed from the published crate:
`Cargo.lock`, `Cargo.toml.orig` and `.cargo-ok`, which only crates.io's packaging uses.

- **`src/scratch.rs` (new, SoftPresenter's own):** the sizes of the scratch buffers for one
  frame (a resting size, what recent frames used, and a guess from the scene's path count for
  one not drawn yet); the one place every frame's sizes go through, which keeps each buffer
  within what the GPU allows and the segments buffer no shorter than the segment counts; how
  the counters the GPU kept are read and judged, counters it never wrote being told apart from
  a frame that fitted; how the sizes grow after a frame that ran out of room; the most one
  frame may have (1 GiB in all, counted as the buffers really made, for the first run of a
  frame as for the ones after, and never by refusing a scene for room that other scenes
  left behind); how long a heavy frame's sizes are remembered; and which scenes are known to
  fit, or to be too heavy, already, by a fingerprint of the scene.
- **`src/lib.rs`:**
  - `Renderer::render_to_texture` now draws in two submissions. The coarse stages run first
    with scratch sizes taken from what recent frames used; their counters are read back; if
    anything ran out of room the frame is run again with more, and only a frame that fitted
    goes on to the fine stage. An error means nothing was drawn: `Error::SceneTooHeavy` (the
    scene needs more than a frame may have; remembered, so the same scene is refused at once
    the next time without being sent to the GPU), `Error::GpuTooSlow` (see the wait, below) or
    `Error::FrameDropped` (the GPU never ran the frame: the counters were not written; the
    picture atlas is then let go and the resolver started again, so that the next frame sends
    its pictures again, since the resolver had counted them as sent). A frame still asking
    for more room after sixteen runs is given up with `Error::GpuTooSlow` as well, not
    refused.
  - `Ok` means the scene was sent to be drawn in full, with one case taken on trust: a scene
    that was drawn before at the same size and fitted (a playing video, frame after frame) is
    not read back again, since the same scene always counts the same. Nothing checks that
    frame; the bounds checks below are what keep a wrong guess there from doing harm.
  - Not checked at all: the fine stage. A frame the GPU drops after its counters were read
    (macOS drops the frames of every app when any app's GPU work faults) returns `Ok` and
    leaves the target as it was. Vello has no way to see that, and SoftPresenter does not
    make up for it: the screen is put right by the next thing that draws it, and nothing tells the operator
    meanwhile. Drawing still
    screens again every few seconds was tried and taken out: for a slide full of small text
    it took the slide's working memory again each time (footprint 179 MB at rest, 815 to
    852 MB every five seconds).
  - The wait for the counters only ever asks the device how far the GPU has got
    (`PollType::Poll`), again and again; the renderer never waits inside the device. A wait
    with a time limit for one submission, which an earlier version of this change made, trips
    an assertion inside wgpu 30 when another thread submits and polls the same device (tried
    by itself, within five hundred waits). A Mac's or a discrete GPU is given 0.4 seconds
    for each run of a frame (an integrated or other PC GPU four times that, one that runs on
    the processor sixteen times: an Intel UHD 770 took 0.54 seconds for a heavy 4K slide's
    first two runs and 1.4 seconds for one of four runs, with its shaders still being built),
    a run the GPU finishes gives the next its own wait, and a call as a whole ends after four
    waits, with `Error::GpuTooSlow`, keeping the room found so far for the next try. A call
    that gave up leaves a note with the queue (`Queue::on_submitted_work_done`); when the next
    call finds the GPU has finished that work (slow, not stuck), it is given twice the wait
    the one before had, up to four seconds. A GPU that has not finished it changes nothing,
    so behind a stuck one a call still gives up after the usual wait. The longest whole call seen in the tests, for a heavy 4K
    slide run again, with other threads working the same device as hard as they could, was
    about 100 ms (103 ms).
  - Vello's own single pass is kept as `Renderer::render_to_texture_unchecked`, and is still
    what a renderer with `use_cpu` runs.
  - New: `Renderer::rest` (give back what a heavy frame took, when drawing stops; it never
    waits, and says whether the GPU still holds any buffer given back, by `rest` or by a
    frame as it was drawn), `Renderer::set_fixed_scratch`,
    `Renderer::spare_bytes`, `Renderer::scratch_wanted`, `Renderer::scratch_reruns`,
    `Renderer::scratch_runs`, `Renderer::scratch_unwaited`, `Renderer::gpu_waited` (how long
    the renderer has waited for the GPU, in all, reading back counters: the app's log of a
    hold-up of the screens tells the GPU's time from the renderer's own by it); for tests,
    `Renderer::set_longest_wait`, `Renderer::set_most_runs` and `Renderer::drop_frames`; the
    errors above and
    `Error::GpuWait`; the types `ScratchSizes`, `ScratchPlan` and `ScratchRoom`.
- **`src/render.rs`:** `Render::render_encoding_coarse` takes the scratch room for the frame
  and writes it over the constants `RenderConfig::new` starts with, and takes the fingerprint
  of the scene as it is sent to the GPU; `Render::abandon_fine` frees a frame that will be
  run again; `pictures_only` keeps, of a frame that is refused before it is run, the commands
  that put its pictures in the atlas; `Render::scratch_plan` and `Render::scene_print`.
- **`src/wgpu_engine.rs`:**
  - The shaders are compiled with bounds checks (`ShaderRuntimeChecks::bounds_checks`), which
    upstream leaves off. Not every stage stops writing when its scratch buffer is full
    (`tile_alloc.wgsl` clears the tiles it could not allocate; `path_tiling.wgsl` trusts that
    the segments fit). With upstream's constant sizes that is rare; with sizes that follow
    the drawing a frame runs out of room as a matter of course, and an unchecked write past a
    buffer faults the GPU: macOS drops the frame, restarts the GPU under every app on the
    computer, and after a few faults ignores the process. Measured on an Apple GPU, the
    checks cost nothing that shows: a song slide and a full reading, at 1080p and 4K, took the
    same time with and without them to within 2%, which is the difference between two runs.
  - The buffer the counters are copied into is made for that one copy with every byte set to
    0xFF (`unread_download`), where upstream takes a zeroed buffer from the pool: a copy the
    GPU never made would otherwise read as "everything fitted".
  - The buffer pool notes when each spare buffer was handed back and can give buffers back to
    the GPU (`release_idle`, `release_larger_than`, `release_spare`, `spare_bytes`). Upstream
    keeps every buffer it ever made.
  - `SIZE_CLASS_BITS` is 2, not 1: a buffer is at most a quarter larger than asked for, not a
    half, which matters for buffers of tens of megabytes.
- **`src/util.rs`:** `RenderContext` asks the GPU for its largest storage buffer sizes (up to
  1 GiB) in place of the default 128 MiB, so that a heavy frame's buffers are allowed. On
  Windows it also chooses the adapter itself, unless `WGPU_BACKEND`, `WGPU_ADAPTER_NAME` or
  `WGPU_POWER_PREF` is set: the best kind (a discrete GPU, then an integrated one, the
  others, one that runs on the processor), and within a kind Vulkan, then DirectX 12, then
  OpenGL, trying the next adapter if one cannot make a device. Elsewhere wgpu's own choice
  is kept.
- **`src/lib.rs`, wait for the GPU:** see the account of `Renderer::render_to_texture` above
  (a wait for each run, longer for a GPU that is not a Mac's or a discrete one, and
  learnt from a GPU seen to finish what a call gave up on).

The crate has one `unsafe` block, upstream's own: creating the shader modules as trusted, in
`wgpu_engine.rs`. SoftPresenter's changes add none; the bounds checks are turned on inside
that block, which asks for more checking than upstream does, not less.

## Checking it

From the repository's root (inside this folder Cargo wants a lock file of the folder's own,
which is not kept, and so goes to the network and fails without one):

    cargo test -p vello --lib scratch

runs the tests of `src/scratch.rs`; one of them uses a GPU if there is one. The tests that
draw are in `crates/sp-render/tests/golden.rs`: pictures that outgrow their room many times
over, one too heavy to draw, more slides than are kept in mind, and several threads drawing
with one device.

## Updating

To move to a newer Vello, check first whether it sizes these buffers itself (then this copy
can go). If not, copy the new crate here and carry the changes above across; the picture tests
in `crates/sp-render/tests/golden.rs` say whether heavy slides still draw.
