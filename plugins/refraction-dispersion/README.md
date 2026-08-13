# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion is an Adobe After Effects effect plug-in for creating glass-like refraction, chromatic dispersion, and highlight shaping from a mask or shape layer.

It builds a height field from a selected layer, derives pseudo surface normals, samples the input or optional background layer through RGB or six-channel dispersion, then composites the result back over the original image. The plug-in includes a CPU renderer and an optional `wgpu` renderer.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Repository: [`onmokoworks/onmk-ae-plugins`](https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/refraction-dispersion)
- Display name: `RefractionDispersion`
- After Effects match name: `RefractionDispersion`
- Plugin file name:
  - Windows: `RefractionDispersion.aex`
  - macOS: `RefractionDispersion.plugin`

## Main Features

- Shape/mask layer driven refraction
- Optional background layer sampling
- RGB Split and Base IOR modes
- RGB or six-channel dispersion
- Master dispersion, per-channel IOR, and per-axis chromatic aberration controls
- Height source, coverage source, height blur, map blur, and edge mode controls
- Fresnel, diffuse, specular, saturation, brightness, contrast, affected blur, and mix controls
- Output, input, mask map, delta map, and debug output modes
- Smart Render support
- Optional `wgpu` rendering path with CPU fallback

## Validation Status

- Windows release build script is used during development.
- `wgpu` smoke test passes on the development machine.
- CPU rendering remains the reference path.
- Pixel path currently operates on ARGB 8-bit buffers.
- macOS build script exists for Apple Silicon, but signing/notarization are not provided yet.

## Build

The Adobe After Effects SDK and a working Rust toolchain are required. The Rust project lives in `rust/`.

On Windows, run this from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Output:

```text
rust\target\release\RefractionDispersion.aex
```

On macOS, run this on an Apple Silicon Mac:

```bash
bash ./scripts/build_macos_release.sh
```

Output:

```text
rust/target/release/RefractionDispersion.plugin
```

## Installation

### Using a release build

On Windows, download `RefractionDispersion.aex` from GitHub Releases, close After Effects, then copy it to the plug-ins folder:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

On macOS, download `RefractionDispersion-macos-arm64.plugin.zip` from GitHub Releases, extract it, close After Effects, then copy `RefractionDispersion.plugin` to the MediaCore folder:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

Command-line copy example:

```bash
unzip RefractionDispersion-macos-arm64.plugin.zip
sudo cp -R RefractionDispersion.plugin "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
```

If a downloaded `.plugin` is blocked by macOS Gatekeeper, remove the quarantine attribute if needed:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/RefractionDispersion.plugin"
```

Restart After Effects and apply the effect from `Distort > RefractionDispersion`.

### Installing a local build

Close After Effects, then run PowerShell as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_refractiondispersion_admin.ps1
```

For a local macOS build, manually copy the generated `rust/target/release/RefractionDispersion.plugin` bundle into the MediaCore folder.

Generated `.aex` / `.plugin` files are not committed to Git. Distribution builds are attached as GitHub Release artifacts.

The macOS release artifact is for Apple Silicon / arm64. It is currently ad-hoc signed; Developer ID signing and notarization are not provided yet.

## Parameters

- `IOR Mode`: selects RGB Split or Base IOR sampling
- `Base IOR`: refractive index used by Base IOR mode
- `Refract Power`: offset amount for refracted background sampling
- `Samples`: number of samples per pixel
- `Edge Mode`: controls out-of-frame sampling behavior
- `Dispersion`: master color separation amount
- `IOR Red`, `IOR Green`, `IOR Blue`: refractive index values per RGB channel
- `Chromatic Aberration`: shared channel separation amount
- `Per-axis Chroma`: enables independent X/Y chromatic aberration
- `Chromatic Aberration X`, `Chromatic Aberration Y`: axis-specific separation controls
- `Use 6ch Dispersion (rygcbv)`: expanded wavelength-style dispersion
- `Mask (shape)`: layer used as height/mask source
- `Map Blur`: blur applied to the mask map before height extraction
- `Height Source`: channel used to build the height field
- `Invert Height`: inverts the height field
- `Coverage Source`: channel used for the affected region
- `Height Strength`, `Height Blur`: shape-derived normal controls
- `Edge Blur`: output mask softness
- `Fresnel Power`, `Shininess`, `Diffuseness`: highlight and lighting shaping
- `Light Angle X`, `Light Angle Y`: lighting direction
- `Saturation`, `Affected Brightness`, `Affected Contrast`: affected-region color correction
- `Affected Box Blur`: blur applied inside the affected region
- `Background`: optional layer sampled behind the glass
- `Mix with Original`: blends processed and original pixels
- `Output Mode`: selects output, input, mask map, delta map, or debug regions
- `Use GPU`: enables the optional `wgpu` render path

## Development Checks

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## References

Special thanks to Maxime Heckel for the shader article that inspired the original effect direction:

- [Refraction, Dispersion, and Other Shader Light Effects](https://blog.maximeheckel.com/posts/refraction-dispersion-and-other-shader-light-effects/)

## Notes

- CPU rendering is implemented in `rust/src/refract.rs`.
- The `wgpu` renderer is implemented in `rust/src/gpu.rs` and `rust/shader.wgsl`.
- Mac and full GPU work are tracked in `docs/mac-gpu-roadmap.md`.

## License

License is not committed yet. Add the intended license before publishing.
