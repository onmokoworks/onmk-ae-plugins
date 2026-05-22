# Current Status

## Phase

Rust effect MVP compiles, and the renderer has reached the ASCII milestone.

## Confirmed Direction

- This is an After Effects plug-in project under `Ae_Plugins`.
- The near-term goal is a Rust native `TUI Image Renderer` effect.
- GPU processing is preferred for the main renderer.
- CPU fallback is required.
- The visual target is modern TUI aesthetics rather than pure legacy ASCII.
- The strongest current reference direction is:
  - Unicode block characters
  - half blocks
  - Braille cells
  - truecolor-like gradient thinking

## Implemented Milestones

The current codebase already contains:

1. Rust AE effect scaffold
2. CPU block-mode renderer
3. GPU block-mode renderer
4. Braille renderer path
5. TUI-gradient color mode
6. ASCII-classic renderer path

Current preset entries in the effect UI:

- `ASCII Classic`
- `Block`
- `Braille`
- `TUI Gradient`

## Why This Shape

Recent TUI aesthetics are usually not produced by classic ASCII ramps alone.
They combine:

1. glyph density selection
2. color mapping
3. cell-based resolution tricks
4. optional edge emphasis

That makes a TUI-oriented renderer a better first milestone than a narrowly
"classic ASCII only" plug-in.

## Rust References

The current implementation direction was based on existing Rust AE plug-in
experiments:

- `ColorfulEchoRust`: minimal Rust AE effect shape and SmartRender wiring.
- `OpticalFlareRust`: `wgpu` compute path and WGSL shader integration.
- `ParticleLabRust`: CPU renderer safety guards and heavy-render organization.
- repository build scripts: `.aex` output/install convention.

## Next Step

The next implementation step should be preset-data loading, then host
smoke-testing.

1. Wire `presets/*.json` into the Rust runtime so preset defaults are not
   duplicated only in code.
2. Keep UI preset labels and runtime defaults aligned with those JSON files.
3. Install and smoke-test the compiled `.aex` in After Effects:

   1. Run `scripts\build_release.ps1`.
   2. Run `scripts\install_tuiimage_admin.ps1` from an elevated shell.
   3. Start After Effects.
   4. Confirm the effect appears under `Stylize`.
   5. Apply it to footage/precomp and verify ASCII, block, Braille, and
      TUI-gradient output, plus cell sizing, color mode, invert, contrast,
      gamma, and source mix.

## 2026-05-17 Scaffold

Created the initial project scaffold:

- `README.md`
- `docs/architecture.md`
- `docs/mvp-spec.md`
- `docs/preset-contract.md`
- `docs/visual-direction.md`
- `presets/*.json`
- placeholder `cep/`, `jsx/`, and `renderer/` boundaries

This recommendation was superseded after inspecting the local Rust plug-ins.
The first real implementation should be Rust + GPU, with CEP/ExtendScript kept
as optional companion tooling rather than the main runtime.

## 2026-05-17 Compile Pass

Created the actual Rust effect crate under `rust/`:

- `Cargo.toml`
- `build.rs`
- `shader.wgsl`
- `src/lib.rs`
- `src/gpu.rs`

Implemented:

- `TuiImage` PiPL metadata.
- AE parameters for preset, cell size, contrast, gamma, edge boost, invert,
  color mode, foreground/background, and source mix.
- CPU block/Braille-like fallback renderer.
- `wgpu` compute path with WGSL shader and per-thread buffer cache.

Verified:

- `cargo fmt`
- `cargo check`
- `cargo build --release`

Built artifacts:

- `rust\target\release\TuiImage.aex`

Current caveat: AE host smoke-test has not been run yet, and the plug-in has
not been copied into the elevated Adobe MediaCore directory by this task.
