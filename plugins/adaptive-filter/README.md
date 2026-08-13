# AdaptiveFilter

[English](./README.md) | [Japanese](./README.ja.md)

AdaptiveFilter is a working-name Adobe After Effects effect plug-in for edge-aware filters split out from MedianPro.

The name is temporary. This repository currently carries the non-median filter set: Kuwahara, Generalized Kuwahara, and Bilateral filtering with optional luminance-map control.

> Specifications, UI, parameter names, defaults, and the final product name may change.

## Name

- Display name: `AdaptiveFilter`
- After Effects match name: `ONMK_AdaptiveFilter`
- Category: `Filter`
- Plugin file name:
  - Windows: `AdaptiveFilter.aex`
  - macOS: `AdaptiveFilter.plugin`

## Main Features

- Kuwahara filtering
- Generalized Kuwahara filtering
- Bilateral filtering
- Luminance map layer support for per-pixel radius scaling
- Invert map option
- Iteration count and mix controls
- Smart Render support

## Validation Status

- Current production render path is CPU based
- `SmartRenderGpu` falls back to CPU
- Pixel path currently operates on ARGB 8-bit buffers
- macOS packaging is scripted but not validated on this machine

## Build

The Adobe After Effects SDK and a working Rust/MSVC toolchain are required. The Rust project lives in `rust/`.

On Windows, run this from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Output:

```text
rust\target\release\AdaptiveFilter.aex
```

On macOS, run this on an Apple Silicon Mac:

```bash
bash ./scripts/build_macos_release.sh
```

Output:

```text
rust/target/release/AdaptiveFilter.plugin
```

## Installation

On Windows, close After Effects, then copy `AdaptiveFilter.aex` to:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

On macOS, close After Effects, then copy `AdaptiveFilter.plugin` to:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

If a downloaded `.plugin` is blocked by macOS Gatekeeper, remove the quarantine attribute if needed:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/AdaptiveFilter.plugin"
```

Restart After Effects and apply the effect from `Filter > AdaptiveFilter`.

### Installing a local Windows build

Close After Effects, then run PowerShell as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_adaptivefilter_admin.ps1
```

Generated `.aex` / `.plugin` files are not committed to Git.

## Parameters

- `Filter Type`: Kuwahara, Generalized Kuwahara, or Bilateral
- `Radius`: base filtering radius
- `Edge Preserve`: shape/edge sensitivity for edge-aware filters
- `Iterations`: number of repeated filter passes
- `Mix with Original`: blends the result back toward the input
- `Luminance Map`: optional layer that scales radius per pixel
- `Invert Map`: inverts luminance-map influence

## Development Checks

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## License

MIT License. See [LICENSE](LICENSE).
