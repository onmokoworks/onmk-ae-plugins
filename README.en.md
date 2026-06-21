# MinimaxMap

[日本語](./README.md) | [English](./README.en.md)

MinimaxMap is an Adobe After Effects effect plug-in for signed, linear morphological minimum/maximum filtering with optional radius-map control.

![MinimaxMap screenshot](./docs/minimaxmap-screenshot.png)

Negative `Amount` values erode with Minimum, positive values dilate with Maximum, and `0.00` is a pass-through. Fractional values are blended between adjacent integer radii, so small values such as `0.01` produce smooth, gradual changes.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `MinimaxMap`
- After Effects match name: `MinimaxMap`
- Category: `Channel`
- Plugin file name:
  - Windows: `MinimaxMap.aex`
  - macOS: `MinimaxMap.plugin`

## Main Features

- Signed linear Minimax amount
- Negative Amount for Minimum, positive Amount for Maximum
- `0.01` precision for subtle morphology control
- Horizontal, vertical, or both-direction processing
- Independent X/Y amount ratio controls
- Color, alpha, or color-and-alpha channel modes
- Repeat-edge option
- Radius map layer support
- Invert map and mix controls
- Smart Render support

## Validation Status

- Local build and MediaCore install tested on Adobe After Effects 2026 / Windows
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
rust\target\release\MinimaxMap.aex
```

On macOS, run this on an Apple Silicon Mac:

```bash
bash ./scripts/build_macos_release.sh
```

Output:

```text
rust/target/release/MinimaxMap.plugin
```

## Installation

### Using a release build

On Windows, download `MinimaxMap.aex` from GitHub Releases, close After Effects, then copy it to the plug-ins folder:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

On macOS, download `MinimaxMap-macos-arm64.plugin.zip`, extract it, close After Effects, then copy `MinimaxMap.plugin` to:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

If a downloaded `.plugin` is blocked by macOS Gatekeeper, remove the quarantine attribute if needed:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/MinimaxMap.plugin"
```

Restart After Effects and apply the effect from `Channel > MinimaxMap`.

### Installing a local Windows build

Close After Effects, then run PowerShell as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_minimaxmap_admin.ps1
```

Generated `.aex` / `.plugin` files are not committed to Git.

## Parameters

- `Amount`: signed minimax amount. Negative values run Minimum, positive values run Maximum, and `0.00` is no effect.
- `Ratio X` / `Ratio Y`: horizontal and vertical amount multipliers
- `Direction`: Horizontal, Vertical, or Both
- `Channel`: Color, Alpha, or Color and Alpha
- `Repeat Edge Pixels`: repeats edge pixels instead of using neutral outside values
- `Mix with Original`: blends the result back toward the input
- `Radius Map`: optional layer that scales absolute Amount per pixel
- `Invert Map`: inverts radius-map influence

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
