# Where this copy of naga comes from, and what was changed

This folder is the `naga` crate, version 30.0.2, as published on crates.io
(<https://crates.io/crates/naga/30.0.2>, SHA-256 of the published file
`fd0dd362bf26c0ffdd143df6da66a25d287e196eff482f117771010b207ad163`), from
<https://github.com/gfx-rs/wgpu>, where it lives in `naga`. It is the shader translator of wgpu,
the graphics library SoftPresenter draws through: it writes each shader in the language of the
computer's graphics (Metal's, on a Mac).

naga is copyright the gfx-rs developers and licensed under MIT OR Apache-2.0; both licences are
here as `LICENSE.MIT` and `LICENSE.APACHE`, unchanged. SoftPresenter's changes are offered under
the same two licences, so they can go back to naga.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. Its dependencies are the published ones, at the versions the app already
had. It goes with the copy of `wgpu-hal` in `third_party/wgpu-hal`, which is what uses the
change.

## Why

Since a change merged in April 2026 (<https://github.com/gfx-rs/wgpu/pull/9327>), naga writes
`[[max_total_threads_per_threadgroup(N)]]` on every compute shader for Metal's language 2.1 and
later. The compiler of Intel's older Mac graphics does not take it: on an Intel Iris 5100 (a
Mac mini of 2014, macOS 11.6) Metal's compiler service aborts on any compute shader that
carries it (`IGILErrorMessageHandlers::reportUnsupportedMaxWorkGroupSizeMetadataError`, in
`libIGIL-Metal.dylib`), whatever the number: the first to meet it is one wgpu builds for
itself as a device is made (its check of indirect dispatches, a workgroup of 1), so no device
could be made and SoftPresenter did not start, after about 13 seconds and three crash reports
of `MTLCompilerService`. With that check switched off (`WGPU_VALIDATION_INDIRECT_CALL=0`) the
device was made, and then every compute pipeline the app asked for was refused the same way
(`vello.fine_area`, `sp-render texture copy`, `sp-render video colours`).

Upstream has it as <https://github.com/gfx-rs/wgpu/issues/5827>, with a fix proposed and not
merged on 2026-11-27 (<https://github.com/gfx-rs/wgpu/pull/10731>); the change here is that
proposal's first half, without its test of the compiler at every start (a test that fails
costs seconds and a crash report each time).

## What was changed

Marked `SoftPresenter:` in the source. Removed from the published crate: `Cargo.lock`,
`Cargo.toml.orig` and `.cargo-ok`, which only crates.io's packaging uses.

- **`src/back/msl/mod.rs`:** `Options` has a new field, `emit_workgroup_size_attribute`, true
  unless the caller says otherwise.
- **`src/back/msl/writer.rs`:** the attribute is written only when that field is true.

Nothing else is changed: with the field true, the Metal text is what the published crate
writes, byte for byte. naga has no `unsafe` code, and the change adds none.

## Checking it

`cargo test -p sp-render --lib workgroup_attribute` (on a Mac) writes a compute shader both
ways and checks that the attribute is the only difference.

## Updating

When wgpu takes a fix for issue 5827, move to that version and drop this copy and
`third_party/wgpu-hal`. Until then, for a newer wgpu, copy the new crates here and carry the
changes across; the two copies must be the version the `wgpu` in `Cargo.lock` asks for.
