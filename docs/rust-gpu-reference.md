# Rust/GPU Reference Notes

These notes record the existing local projects that should shape
`TuiImage`. They are more useful than abstract plug-in theory, because
they already match this machine, this SDK setup, and the user's current build
style.

## Original Reference Roots

```text
ColorfulEchoRust
OpticalFlareRust
ParticleLabRust
```

These were local reference projects used while designing the renderer. They are
not dependencies of this repository.

## ColorfulEchoRust

Use as the smallest practical AE effect model.

Relevant patterns:

- `Cargo.toml`
  - `crate-type = ["cdylib"]`
  - dependency: `after-effects` from `virtualritz/after-effects`
  - build dependency: `pipl` from the same repository
- `build.rs`
  - `PIPLType::AEEffect`
  - Windows entry point: `CodeWin64X86("EffectMain")`
  - match name and visible plug-in name separated cleanly
  - smart render and threaded render flags
- `src/lib.rs`
  - `ae::define_effect!(Plugin, (), Params)`
  - enum-backed AE parameter IDs
  - `params_setup`
  - `handle_command` with `Render`, `SmartPreRender`, `SmartRender`,
    `SmartRenderGpu`

TuiImage should copy this shape before getting clever.

## OpticalFlareRust

Use as the GPU model.

Relevant patterns:

- `wgpu = "26"` plus `pollster`, `futures-intrusive`, `parking_lot`
- WGSL loaded with `include_str!("../shader.wgsl")`
- `GpuProcessor` owns `Device`, `Queue`, `ComputePipeline`, and bind group
  layout
- per-thread buffer cache keyed by `std::thread::ThreadId`
- `#[repr(C, align(16))]` uniform struct for WGSL-safe layout
- storage-buffer source/output, staging-buffer readback
- workgroup dispatch over `(width + 15) / 16`, `(height + 15) / 16`

For TuiImage, the first shader should not render arbitrary fonts. It
should procedurally render block and Braille-style cell output. Font atlases can
come later.

## ParticleLabRust

Use as the safety model.

Relevant patterns:

- explicit render config structs
- bounded pixel work
- deadline/time-budget thinking
- clear separation between config, sampling, and draw functions

TuiImage can create huge per-cell work if cell size is too small. Guard
that from the start:

- minimum cell size
- maximum generated cell count
- fallback/skip behavior for pathological settings
- no unbounded AE text-layer generation in render path

## Install Target

Release builds should create `rust\target\release\TuiImage.aex`, then install
to the After Effects common plug-in directory, for example:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

## Recommended First Implementation Slice

1. Rust effect compiles and appears in AE as `TuiImage`.
2. Parameters exist but only block mode does real work.
3. CPU block renderer works for 8-bit RGBA.
4. GPU block renderer matches CPU output within acceptable tolerance.
5. Braille mode adds higher apparent resolution.
6. Gradient mode adds source/gradient color treatment.

Anything beyond that is ornament. Useful ornament, perhaps, but still ornament.
