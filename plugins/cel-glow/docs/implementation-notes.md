# CelGlow Implementation Notes

Date: 2026-05-13

## Current Direction

CelGlow is being shaped as a storybook/cel-like glow effect, not a standard
smooth photographic glow.

The important visual rule is:

- The glow should feel painted and separated into clear bands.
- Band opacity should not gradually fade outward inside each band.
- Adjacent bands should not create transparent gaps.
- Softness is still useful at the source-mask boundary and at the whole glow's
  start/end edges.

## Band UI Decision

The previous fixed per-band controls made the parameter list too large and hard
to reason about. The current direction is generated bands:

- `Band Count`: 1-8
- `Glow Start (%)`: inner offset from the source mask
- `Total Size (%)`: total width of the generated glow
- `Size Mode`: `Equal`, `Taper`, or `Expand`
- `Palette Mode`: `Single`, `Inner - Outer`, or `3 Color Ramp`
- `Opacity Inner (%)` / `Opacity Outer (%)`: opacity interpolation per band

Each band's outer boundary follows the next band's inner boundary
automatically. This keeps the useful "linked boundary" behavior without exposing
manual inner/outer controls for every band.

## Gap Fix

There was a visible gap risk between bands because each band faded in at its
inner edge and faded out at its outer edge. Even when the generated ranges were
adjacent, both sides of a boundary could drop opacity.

The current implementation changed this:

- Internal band boundaries do not fade to transparent.
- `Edge Softness` applies only to the whole glow start and whole glow end.
- Bands remain flat inside their ranges, producing a more cel-like result.

This change was applied to both:

- CPU path: `rust/celglow/src/lib.rs`
- GPU path: `rust/celglow/shader.wgsl`

## Source Softness

There are two separate softness controls:

- `Luma Softness`: softens threshold selection before the distance field.
- `Source Box Blur (px)`: blurs the generated source mask before EDT, useful
  for a softer storybook-like pickup from the source layer.

When `Source Box Blur` is greater than zero, GPU EDT is skipped because the GPU
JFA path does not currently include mask blur. The CPU computes the blurred mask
and EDT, then GPU compositing can still be used if available.

## GPU Status

GPU support currently uses `wgpu`:

- Source mask generation on GPU when blur is zero.
- Approximate distance field via Jump Flooding Algorithm.
- Per-pixel band/copy compositing.
- CPU fallback if GPU initialization or execution fails.

The GPU path is approximate because JFA is not exact EDT. Exact GPU EDT remains
future work.

## DeepSeek Review Notes

DeepSeek v4 Pro was run through OpenCode as a read-only review after the
variable-band redesign.

Main result:

- Rust/WGSL uniform layout was judged consistent.
- CPU/GPU view, blend, alpha, and band logic were judged consistent.
- Source Box Blur fallback behavior was judged intentional and robust.
- Main caution was AE parameter ordinal compatibility.

Follow-up applied:

- `Params` enum now uses explicit discriminants.
- A comment warns that AE stores parameters by ordinal stream position, so new
  params should be appended after real project use.

## Current Build Command

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow\rust\celglow
cargo build
cargo build --release
```

Release artifact:

```text
D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow\rust\celglow\target\release\celglow.dll
```

For After Effects on Windows, install/rename as:

```text
CelGlow.aex
```

## Open Questions

- Whether `Edge Softness` should be renamed to make clear it affects the whole
  glow edge, not every internal band boundary.
- Whether a future custom UI should expose optional manual band editing after
  the generated band setup.
- Whether presets should be added for common storybook looks.
- Whether exact GPU EDT is worth implementing or JFA is visually sufficient.
