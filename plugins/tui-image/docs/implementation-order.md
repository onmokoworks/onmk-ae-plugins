# Implementation Order

This file tracks the practical implementation sequence for the Rust AE effect.
It is intentionally narrower than the long-term product docs.

## Milestone Order

1. Rust AE effect scaffold
2. CPU block-mode renderer
3. GPU block-mode renderer
4. Braille renderer path
5. TUI-gradient color mode
6. ASCII-classic renderer path
7. Preset data loading / embedding
8. AE smoke-test in host
9. Companion export tooling

## Current Position

As of 2026-05-19, milestones 1 through 6 are implemented in code.

Implemented now:

- AE effect parameters and PiPL metadata
- CPU fallback renderer
- Optional GPU renderer via `wgpu`
- Preset switch for:
  - `ASCII Classic`
  - `Block`
  - `Braille`
  - `TUI Gradient`
- Cell sizing, contrast, gamma, edge boost, invert, color mode, source mix
- Optional source blur before color sampling

Not implemented yet:

- Preset JSON loading/embedding from `presets/*.json`
- Verified host smoke-test in After Effects
- Export-oriented text / ANSI / APNG tooling

## Why This Order

The first useful question is whether the renderer can produce stable raster
output inside After Effects. That makes preset-data plumbing and companion
export features strictly later than the live effect path.

## Next Recommended Step

Implement milestone 7 next:

- load or embed preset JSON files
- map preset defaults into the runtime parameter model
- keep preset names and behavior synchronized with `presets/*.json`

After that, do the real AE smoke-test so host issues are separated from renderer
issues.
