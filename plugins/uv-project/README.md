# UVProject

[English](./README.md) | [日本語](./README.ja.md)

![UVProject demo](./docs/uv-project-demo.gif)

UVProject is an Adobe After Effects effect plug-in for projecting a texture
through a UV / ST map.

Apply the effect to either the **UV / ST map** or the **texture**. Choose the
applied layer's role with `Input Is`, then select the opposite role with
`Other Layer`. UVProject decodes U from red and V from green, samples the
texture at that coordinate, and bakes it into the UV layout. This is the
classic "STMap" remap used to re-project 3D UV passes back onto 2D artwork.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `UVProject`
- After Effects match name: `UVProject`
- Plugin file name:
  - Windows: `UVProject.aex`
  - macOS: `UVProject.plugin`

## How it works

1. When applying UVProject to the UV map, choose `UV Map` under `Input Is` and
   select the texture under `Other Layer`.
2. When applying it to the texture, choose `Texture` under `Input Is` and select
   the UV map under `Other Layer`.
3. For Blender UV passes, use 32-bpc OpenEXR footage in a 32-bpc AE project.

## Main Features

- STMap-style UV remap: project any texture layer through a UV / ST pass
- Apply the effect to either the UV map or texture
- 8, 16 and 32 bpc support
- Unpremultiplies antialiased UV-map edges before decoding coordinates
- V-axis origin toggle: Top (After Effects / image space) or Bottom (Nuke / 3D)
- Wrap modes for texture sampling: Clamp / Repeat / Mirror
- Bilinear or Nearest sampling
- Optional: use the UV map's own alpha as a coverage mask (transparent where
  there is no geometry)
- Opacity
- After Effects Smart Render support

## Validation Status

- Tested on Adobe After Effects 2025 / Windows
- Supports 8-bit ARGB, 16-bit ARGB and 32-bit float ARGB
- 32 bpc is recommended for Blender UV passes; 8 bpc limits each coordinate to
  256 levels
- CPU implementation; the native `SmartRenderGpu` path falls back to CPU
- Automated tests cover UV precision and premultiplied edge recovery

## Build

On Windows, run this from the repository root:

```powershell
cargo build --release
```

Output:

```text
target\release\uv_project.dll
```

Rename or copy the built DLL to `UVProject.aex` before installing it into After Effects.

## Installation

### Using a local Windows build

Close After Effects, then run PowerShell as administrator:

```powershell
cargo build
Copy-Item -Force target\debug\uv_project.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex"
```

For a release build:

```powershell
cargo build --release
Copy-Item -Force target\release\uv_project.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex"
```

Destination:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

Restart After Effects and apply the effect from `Distort > UVProject`.

Generated `.aex` / `.dll` files are not committed to Git.

## Parameters

- `Other Layer (Texture / UV Map)`: the role opposite the applied layer.
- `Input Is`: whether the applied layer is the UV map or the texture.
- `V Origin`: `Top (After Effects)` or `Bottom (Nuke / 3D)`. Most 3D/render UV
  passes use the bottom origin (the default); image-space maps use the top.
- `Wrap`: how texture coordinates outside 0..1 are handled — `Clamp`, `Repeat`,
  or `Mirror`.
- `Sampling`: `Bilinear` (smooth) or `Nearest` (no interpolation).
- `Use UV Alpha as Mask`: multiply output coverage by the UV map's alpha, so
  areas with no geometry stay transparent.
- `Opacity`: overall strength, 0–100%.

## Development Checks

```powershell
cargo fmt
cargo check
cargo build --release
```

## Environment Variables

The Adobe After Effects SDK is required at build time.

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
```

Set `AESDK_ROOT` for your local environment before building.

## Limitations

- 8-bpc UV maps can show coordinate quantization; 16/32 bpc is recommended
- CPU only (no native GPU render path)
- The current validation environment is Windows / After Effects 2025

## License

MIT License. See [LICENSE](LICENSE).
