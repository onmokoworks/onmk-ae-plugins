# FrameSlice

[English](./README.md) | [日本語](./README.ja.md)

FrameSlice is an Adobe After Effects effect plug-in for creating slit-scan style time offsets inside image layers.

It samples each pixel from a different point in time, using horizontal, vertical, radial, or map-layer driven time maps.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `FrameSlice`
- After Effects match name: `FrameSlice`
- Plugin file name:
  - Windows: `FrameSlice.aex`
  - macOS: `FrameSlice.plugin`

## Main Features

- Time-based slit-scan sampling of the input layer
- Horizontal, vertical, radial, and map-layer driven time maps
- Past, future, and bidirectional time ranges
- Nearest or linear interpolation between sampled frames
- Blend control for mixing the result back toward the original frame
- Internal GPU processing through `wgpu` with CPU fallback
- After Effects Smart Render support

## Validation Status

- Tested on Adobe After Effects 2025 / Windows
- Current implementation supports 8-bit ARGB only
- Deep Color / 32-bit float pixel formats are not supported
- After Effects native `SmartRenderGpu` path is not implemented
- No automated tests ship with this repository

## Build

On Windows, run this from the repository root:

```powershell
cargo build --release
```

Output:

```text
target\release\frame_slice.dll
```

Rename or copy the built DLL to `FrameSlice.aex` before installing it into After Effects.

## Installation

### Using a local Windows build

Close After Effects, then run PowerShell as administrator:

```powershell
cargo build
Copy-Item -Force target\debug\frame_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\FrameSlice.aex"
```

For a release build:

```powershell
cargo build --release
Copy-Item -Force target\release\frame_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\FrameSlice.aex"
```

Destination:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

Restart After Effects and apply the effect from `Time > FrameSlice`.

Generated `.aex` / `.dll` files are not committed to Git.

## Parameters

- `Time Frames`: frame range sampled along the time axis
- `Frame Step`: interval between sampled frames
- `Time Direction`: chooses `Past`, `Future`, or `Both`
- `Slice Mode`: chooses `Horizontal`, `Vertical`, `Radial`, or `Map Layer`
- `Gradient Phase`: offsets the generated time map
- `Center`: center point used by `Radial` mode
- `Interpolation`: chooses `Nearest` or `Linear` sampling
- `Mix with Original`: blends the processed result with the original frame
- `Time Map`: reference layer used by `Map Layer` mode
- `Invert Map`: inverts the time map

## Development Checks

```powershell
cargo fmt
cargo check
cargo test
cargo build --release
```

`cargo test` currently runs 0 tests.

## Environment Variables

The Adobe After Effects SDK is required at build time.

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
```

Set `AESDK_ROOT` for your local environment before building.

## Limitations

- 8-bit ARGB only
- Deep Color / 32-bit float rendering is not supported
- After Effects native GPU render path is not supported
- The legacy `Render` path does not yet implement the same slit-scan processing as Smart Render
- The current validation environment is Windows / After Effects 2025
- No automated tests yet

## License

MIT License. See [LICENSE](LICENSE).
