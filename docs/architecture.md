# Architecture

## Runtime Decision

The project should start as a Rust native After Effects effect, not as a CEP
export helper. The local machine already has several Rust AE plug-ins built with
the same constraints, so there is no reason to pretend this is unexplored.

Primary target:

```text
After Effects Effect
  Rust cdylib + after-effects crate + PiPL

Render Core
  CPU source-frame flattening
  wgpu compute shader
  CPU fallback renderer

Preset Contract
  JSON presets for glyph/color behavior
  compiled defaults embedded into the plug-in

Optional Companion Later
  CEP panel for preset editing, batch export, APNG helpers
```

## Reference Projects

This implementation was originally shaped by local Rust After Effects plug-in
experiments. Those reference projects are useful during development, but they
are not required to build this repository.

- `ColorfulEchoRust`
  - minimal Rust AE effect structure
  - `after-effects` crate usage
  - PiPL generation in `build.rs`
  - `SmartRender` / `SmartRenderGpu` command handling
- `OpticalFlareRust`
  - `wgpu` initialization
  - WGSL shader inclusion
  - per-thread GPU buffer cache
  - staging-buffer readback pattern
- `ParticleLabRust`
  - CPU renderer safety rules
  - pixel budgets and pathological-work guards
  - rayon/glam-style heavy rendering organization
- build helper scripts
  - repository-local `.aex` build and install convention

## Effect Boundary

The AE effect should expose a small, boring parameter surface:

- `Preset`: `ascii-classic`, `block`, `braille`, `tui-gradient`
- `Cell Width`
- `Cell Height`
- `Contrast`
- `Gamma`
- `Edge Boost`
- `Invert`
- `Color Mode`: `mono`, `source`, `gradient`
- `Foreground Color`
- `Background Color`
- `Source Mix`
- `Alpha Mode`

The first build can hard-code the four presets in Rust and keep the JSON preset
files as the design contract. Runtime JSON loading can come later; AE effects
should not fail just because an adjacent preset file moved.

## GPU Render Path

The first GPU path should be deliberately narrow:

1. Flatten AE source frame to contiguous RGBA.
2. Upload packed pixels to a `wgpu` storage buffer.
3. Upload effect parameters as a 16-byte-aligned uniform buffer.
4. In WGSL:
   - compute luma and optional edge response
   - group pixels into cells
   - choose block/Braille density
   - apply mono/source/gradient color
   - write raster output pixels
5. Read back and copy to AE output.

Do not start with font rasterization. GPU font drawing is a trap for the MVP.
`block` and `braille` can be rendered procedurally; `ascii-classic` can initially
share the same density model or use a baked glyph atlas later.

## CPU Fallback

The CPU fallback is not optional. AE plug-ins must degrade predictably when GPU
creation fails, a driver is unstable, or the host is running in a restricted
environment.

Fallback scope:

- same cell sampling model
- same luma/edge/color parameter interpretation
- block-mode output first
- no APNG/text export requirement

## Later Companion Panel

A CEP/UXP companion still makes sense later for:

- editing and saving presets
- batch rendering all comps to PNG/APNG
- exporting ANSI/plain text frames
- browsing produced 404 variants

But it is not the core renderer. The core renderer is the Rust effect.

## Hard Constraints

- Keep the effect name stable: `TuiImage`.
- Keep preset IDs stable even if the internal implementation changes.
- Keep source/output pixel conversions isolated from glyph selection.
- Keep GPU errors recoverable with CPU fallback.
- Avoid generating thousands of AE text layers in the live effect path.
