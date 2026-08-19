# onmk Film

macOS After Effects 向け SmartFX。独自のフィルムレスポンス、乳剤滲み、密度空間ラティスグレイン、光学系をひとつのパイプラインで処理します。

| | |
|---|---|
| Effect | `onmk Film` |
| Category | `onmk` |
| Match name | `ONMK_Film` |
| Bundle | `onmkFilm.plugin` |
| Version | 0.5.0 |

## Layout

```
plugins/onmk-film/
  src/                    # onmk-film-core sources
  crates/onmk-film-core/  # host-independent engine
  crates/onmk-film-gpu/   # wgpu Metal blur + grain
  crates/onmk-film-ae/    # AE SmartFX host
  presets/                # JSON look notes
  docs/pipeline.md
  scripts/install_dev.sh
```

## Build (macOS)

```sh
export AESDK_ROOT="/path/to/AfterEffectsSDK"
cargo test -p onmk-film-core
cargo test -p onmk-film-gpu
NO_INSTALL=1 just build
./scripts/install_dev.sh
```

Installs to:

`~/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/onmkFilm.plugin`

Restart After Effects. Effect path: **Effect → onmk → onmk Film**.

## Features (v0.5)

- Input CS: Rec.709 / sRGB / Linear / Bypass / S-Log3 / LogC3 / V-Log / C-Log3
- 13 film stocks, 6 looks + Custom
- Exposure, character, print contrast, shoulder, saturation, signature, skin
- Halation, bloom/diffusion, emulsion bleed
- Density lattice grain (gauge, clump, S/M/H, print grain)
- MTF / acutance, CA, vignette, light leak, gate weave
- Crossover + mottle
- 8 / 16 / 32 bpc SmartFX
- Metal GPU blur + grain with CPU fallback

## Notes

- GPU path uses internal wgpu (Metal), not AE `SmartRenderGpu`
- Look popup overwrites related controls (except Neutral / Custom)
- Preset JSON under `presets/` is offline documentation of look values
