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
3. To create a UV map for a flat layer, choose `Generate Planar UV Map`. This
   mode does not require `Other Layer`.
4. For Blender UV passes, use 32-bpc OpenEXR footage in a 32-bpc AE project.

## Main Features

- STMap-style UV remap: project any texture layer through a UV / ST pass
- Apply the effect to either the UV map or texture
- Generate a pixel-center-aligned UV map for a flat layer
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

- Before typed SDK pixel access, the adapter validates dimensions, signed stride,
  row length, addressable extent, nonnull data and channel alignment. Output image
  dimensions must match the host world exactly instead of silently copying only
  their intersection. Layout tests cover positive/negative padded strides and
  rejected boundary values, not validity of host-owned memory behind a pointer.
- Core image creation checks dimensions, addressable size and allocation failure.
  Projection rejects inconsistent source lengths before sampling. Acquired input
  images are returned on Smart Render errors/unwind; conversion errors from an
  available auxiliary layer are propagated rather than treated as missing input.
  Unit tests cover arithmetic/shape failures and cleanup precedence, not physical
  allocator exhaustion or native AE callback fault injection.
- Pixel indices stay 64-bit until wrapping. Large finite UVs previously saturated
  32-bit indices and could corrupt bilinear output. A direct core regression uses
  a 40000x1 texture, UV coordinates +/-65536, all three wrap modes and both filters,
  checking signed/HDR channels at half opacity. This does not certify native AE
  maximum dimensions, nonfinite UV inputs or arbitrary float precision.
- Tested on Adobe After Effects 2025 / Windows
- Supports 8-bit ARGB, 16-bit ARGB and 32-bit float ARGB
- 32 bpc is recommended for Blender UV passes; 8 bpc limits each coordinate to
  256 levels
- CPU implementation; the native `SmartRenderGpu` path falls back to CPU
- Automated tests cover UV precision and premultiplied edge recovery
- Core tests also cover negative/past-end wrap indices and hand-computed
  bilinear seam colors for Clamp, Repeat and Mirror. Headless auxiliary-layer
  fixtures check sampling, role selection, opacity and partial UV alpha at
  8/16/32 bpc. PNG fixtures do not establish native HDR/sub-8-bit input precision
  or arbitrary ROI/same-instance behavior.
- World pixel format is queried explicitly before reading/writing channels.
  This avoids interpreting ARGB128 float worlds as 16-bit when a host also sets
  the deep-color flag. Headless AEXCompat regression fixtures check planar UV
  coordinates across the full odd-sized frame in Classic and Smart Render.

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
- `Input Is`: whether the applied layer is the UV map or texture, or whether
  UVProject should generate a planar UV map.
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
