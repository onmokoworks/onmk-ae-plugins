# CelGlow Current Status

Date: 2026-05-13

## Direction

CelGlow is now treated as a Rust-only After Effects plug-in. The previous C++
implementation tree was removed to avoid duplicate match-name loading and
checkout-ID drift.

Design/implementation rationale is tracked in `docs/implementation-notes.md`.

## Active Source

```text
rust/celglow/
  Cargo.toml
  build.rs
  src/lib.rs
  src/edt.rs
```

The implementation follows the same pattern as `C:\Users\optim\PathArrayRust`:

- `virtualritz/after-effects`
- `pipl`
- `SmartPreRender`
- `SmartRender`
- `checkout_layer(0, 0)`
- `pre_render_data`

## Build

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow\rust\celglow
cargo build --release
```

Release artifact:

```text
rust/celglow/target/release/celglow.dll
```

For After Effects on Windows, install it as:

```text
CelGlow.aex
```

## AE Identity

- Plug-in name: `CelGlow`
- Category: `onmk`
- Match name: `ANTH CelGlow`
- SmartFX: enabled
- Float color: enabled
- Threaded rendering: enabled
- Flattened sequence data support: enabled

Only one plug-in with match name `ANTH CelGlow` should be visible to After
Effects at a time.

## Implemented

- Luma threshold source mask
- Color source mode and luma/color combine modes
- Luma Softness now affects the source-mask boundary used by EDT instead of only
  affecting the mask debug view
- Source Box Blur softens the generated source mask before distance-field
  generation; this uses CPU mask blur and then continues through CPU EDT / GPU
  compositing when available
- Felzenszwalb 2-pass EDT
- Generated 1-8 hard-edge/additive glow bands
- Flat band opacity for storybook/cel-like glow; no gradual outer fade inside a
  band, and adjacent band boundaries do not fade to transparent
- Band ranges are generated from `Band Count`, `Glow Start`, `Total Size`, and
  `Size Mode`, keeping adjacent band boundaries linked without exposing
  per-band radius controls
- Amount control
- Global blend: Add, Screen, Normal
- Preserve Alpha
- Basic copies: radial, ring, and linear distribution
- GPU compute path for source-mask generation, approximate distance-field
  generation via Jump Flooding Algorithm (JFA), and per-pixel band/copy
  compositing, with CPU fallback when GPU initialization or execution is
  unavailable
- Debug views: result, glow only, source only, distance, mask
- Pre-render output extent expansion based on outer band radius
- 8/16/32 bpc buffer conversion path

## Not Yet Implemented

- Matte source
- Presets
- Custom band/curve UI
- Exact GPU EDT; the GPU path currently uses approximate JFA
- AE project serialization for arbitrary custom band data
