# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion is an Adobe After Effects effect plug-in that creates glass-like refraction and chromatic dispersion from a shape or mask layer.

The effect uses a luminance/alpha height field, surface-normal approximation, RGB or six-channel dispersion sampling, and Blinn-Phong style highlights. The production render path is CPU based and parallelized with Rayon. A CUDA path exists only as experimental plumbing and currently runs a passthrough kernel.

## Name

- Display name: `RefractionDispersion`
- After Effects match name: `RefractionDispersion`
- Plugin file name:
  - Windows: `RefractionDispersion.aex`
  - macOS: `RefractionDispersion.plugin`

## Main Features

- Shape/mask layer driven refraction
- Optional background layer sampling
- Independent red, green, and blue IOR controls
- RGB or six-channel dispersion mode
- Per-axis chromatic aberration
- Height blur, edge blur, Fresnel, diffuse, specular, saturation, and mix controls
- Smart Render support

## Validation Status

- `cargo check` passes on Windows with CUDA Toolkit 13.2 installed
- Current production render path is CPU/Rayon
- CUDA backend is experimental passthrough only; keep `Use GPU (CUDA)` disabled for real output
- Pixel path currently operates on ARGB 8-bit buffers
- macOS packaging and notarization are not validated

## Build

The Adobe After Effects SDK and a working Rust/MSVC toolchain are required. The Rust project lives in `rust/`. Windows builds also compile CUDA PTX in `rust/build.rs`, so `nvcc` must be available through a CUDA Toolkit installation or `PATH`.

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

The macOS release artifact is for Apple Silicon / arm64. It is currently ad-hoc signed; Developer ID signing and notarization are not provided yet. CUDA is disabled on macOS and the CPU renderer is used.

## Parameters

- `IOR Red`, `IOR Green`, `IOR Blue`: refractive index values per color channel
- `Refract Power`: offset amount for refracted background sampling
- `Chromatic Aberration`: shared channel separation amount
- `Per-axis Chroma`: enables independent X/Y chromatic aberration
- `Chromatic Aberration X`, `Chromatic Aberration Y`: axis-specific separation controls
- `Samples`: number of samples per pixel
- `Fresnel Power`, `Shininess`, `Diffuseness`: highlight and lighting shaping
- `Light Angle X`, `Light Angle Y`: lighting direction
- `Saturation`: saturation multiplier after refraction sampling
- `Height Strength`, `Height Blur`: shape-derived height and normal controls
- `Edge Blur`: output mask softness
- `Use 6ch Dispersion (rygcbv)`: expanded wavelength-style dispersion
- `Mask (shape)`: layer used as height/mask source
- `Background`: optional layer sampled behind the glass
- `Mix with Original`: blends processed and original pixels
- `Use GPU (CUDA)`: experimental passthrough backend; disabled by default

## Development Checks

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## Notes

- The CUDA file in `rust/kernels/refract.cu` is a passthrough kernel used to validate device setup and host/device transfer.
- CPU rendering is the reference implementation in `rust/src/refract.rs`.
- Mac and full GPU work are tracked in `docs/mac-gpu-roadmap.md`.

## License

License is not committed yet. Add the intended license before publishing.
