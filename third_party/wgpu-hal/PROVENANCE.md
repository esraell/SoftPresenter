# Where this copy of wgpu-hal comes from, and what was changed

This folder is the `wgpu-hal` crate, version 30.0.2, as published on crates.io
(<https://crates.io/crates/wgpu-hal/30.0.2>, SHA-256 of the published file
`a57452fe453658e141aafdb09de1028afa498805fab518238ddf372a518813ff`), from
<https://github.com/gfx-rs/wgpu>, where it lives in `wgpu-hal`. It is the layer of wgpu, the
graphics library SoftPresenter draws through, that speaks to each system's graphics (Metal on
a Mac, Vulkan, DirectX 12, OpenGL).

wgpu-hal is copyright the gfx-rs developers and licensed under MIT OR Apache-2.0; both licences
are here as `LICENSE.MIT` and `LICENSE.APACHE`, unchanged. SoftPresenter's changes are offered
under the same two licences, so they can go back to wgpu.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. Its dependencies are the published ones, at the versions the app already
had, but for `naga`, which is the copy in `third_party/naga`.

## Why

`third_party/naga/PROVENANCE.md` has the account: the compiler of Intel's older Mac graphics
aborts on a compute shader that carries `max_total_threads_per_threadgroup`, so on an Intel
Iris 5100 with macOS 11.6 no graphics device could be made and SoftPresenter did not start.
This layer decides how shaders are written for a device, so the other half of the change is
here.

## What was changed

Marked `SoftPresenter:` in the source, all of it in the Metal layer (`src/metal`). Removed from
the published crate: `Cargo.lock`, `Cargo.toml.orig` and `.cargo-ok`. One file of the published
crate has Windows line endings (`src/vulkan/descriptor.rs`); git keeps it with Unix ones, as the
repository's settings say, and it is otherwise unchanged.

- **`mod.rs`, `adapter.rs`:** `PrivateDisabilities` has a new field,
  `broken_workgroup_size_attribute`, true for every Metal device whose name starts with
  "Intel" (the test upstream's other Intel workarounds use). It is not found out by trying the
  compiler: the two ways of saying the number are meant to be the same to Metal, so Intel
  graphics whose compiler would have taken the attribute should lose nothing. That is an
  expectation: only the Iris 5100 below was tried.
- **`device.rs`:**
  - For such a device, shaders are written without the attribute (naga's
    `emit_workgroup_size_attribute`).
  - Its compute pipelines are given the workgroup's size as
    `MTLComputePipelineDescriptor.maxTotalThreadsPerThreadgroup` instead (macOS 10.14 and
    later, as the attribute is).
  - A pipeline Metal then makes with room for fewer threads than its shader's workgroup has
    is refused with an error that says both numbers, where its dispatches would otherwise do
    nothing and say nothing.

Shaders handed over as Metal text or a Metal library (wgpu's "passthrough") are left as they
are. Every other device, Apple's GPUs and AMD's included, is sent what the published crate
sends. The crate has `unsafe` code of its own throughout (calls into the systems' graphics);
the changes add none.

## Checked

On the Mac mini of 2014 (Intel Iris 5100, macOS 11.6, build 20G165): the device is made, every compute
pipeline the app builds is accepted (wgpu's own, Vello's last stage with a workgroup of 64,
SoftPresenter's two with 64), and no crash report of the compiler is left. `CHANGELOG.md` and
the commit say what was drawn with them.

## Updating

As for `third_party/naga`: when wgpu takes a fix for issue 5827, drop both copies.
