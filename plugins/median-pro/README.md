# MedianPro

[English](./README.md) | [Japanese](./README.ja.md)

MedianPro is an Adobe After Effects effect plug-in focused on median-family filtering with optional luminance-map control.

It currently provides Median and Weighted Median. The broader Kuwahara / Bilateral filter set has been split into a separate working-name plug-in.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `MedianPro`
- After Effects match name: `ONMK_MedianPro`
- Category: `Filter`
- Plugin file name:
  - Windows: `MedianPro.aex`
  - macOS: `MedianPro.plugin`

## Main Features

- Median filtering
- Weighted Median filtering
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
rust\target\release\MedianPro.aex
```

On macOS, run this on an Apple Silicon Mac:

```bash
bash ./scripts/build_macos_release.sh
```

Output:

```text
rust/target/release/MedianPro.plugin
```

## Installation

On Windows, close After Effects, then copy `MedianPro.aex` to:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

On macOS, close After Effects, then copy `MedianPro.plugin` to:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

If a downloaded `.plugin` is blocked by macOS Gatekeeper, remove the quarantine attribute if needed:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/MedianPro.plugin"
```

Restart After Effects and apply the effect from `Filter > MedianPro`.

### Installing a local Windows build

Close After Effects, then run PowerShell as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_medianpro_admin.ps1
```

Generated `.aex` / `.plugin` files are not committed to Git.

## Parameters

- `Filter Type`: Median or Weighted Median
- `Radius`: filter radius (`0` is a no-op, maximum `100`)
- `Weight Falloff`: spatial weighting for Weighted Median; disabled in Median mode
- `Iterations`: repeated filter passes; large radii combined with multiple passes can be expensive
- `Mix with Original`: blends the result back toward the input (`0%` is a no-op)
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
