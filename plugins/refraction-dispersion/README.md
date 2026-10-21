# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion is an Adobe After Effects effect plug-in for creating glass-like refraction, chromatic dispersion, and highlight shaping from a mask or shape layer.

It builds a height field from a selected layer, derives pseudo surface normals, samples the input or optional background layer through RGB or six-channel dispersion, then composites the result back over the original image. The plug-in includes a CPU renderer and an optional `wgpu` renderer.

> Specifications, UI, parameter names, and defaults may change in future versions.

AEXCompat auxiliary-layer fixtures now verify selected mask-alpha stripes and a
distinct constant background at8/16/32bpc in Classic and Smart Render (12 cases).
Background comparison disables diffuse lighting/refraction to isolate layer
routing. Inputs are PNG8-derived, same-size and zero-origin; this does not certify
native low-bit input, transformed/offset layers or partial ROI.

An additional24 host cases use7x11 and23x37 auxiliary images against17x31 input:
spatial alpha masks and RGB backgrounds match unscaled top-left placement with
zero-fill/clipping in all three depths and both render paths. Origins remain zero;
translation, transforms and partial ROI are not covered by these cases.

Direct native helper tests separately check49 positive/negative/off-screen offsets
for u16 and signed/HDR float against destination-driven sampling (98 comparisons,
exact equality). The current AEXCompat declarative fixture schema has no auxiliary
origin or requested-ROI field. These helper tests do not replace host-origin/ROI
verification and do not test transformed layers.

CPU box blur now includes repeated far-edge samples when its radius reaches past
the image width/height. Previously, small or thin images could lose brightness
even for a constant field. An independent direct-clamped-tap reference covers
five shapes, five radii and constant/signed-HDR patterns (50 comparisons, error
below5e-6). This validates the CPU box pass, not arbitrary ROI or GPU parity.

Complete CPU rendering is also checked with constant native-depth fields at
1x1,1x31,17x1 and9x7, with map/background/height/coverage/affected blur enabled
at sigma0/2/40. All24 depth/shape/sigma cases retain u16 codes exactly or signed
HDR float within5e-6. Neutral lighting/refraction settings isolate blur preservation;
this is not an oracle for arbitrary lighting, dispersion or nonconstant images.

Auxiliary-layer checkout, conversion and checkin errors now propagate instead of
being treated as an absent layer. Offset fitting validates source length and uses
checked, fallible output allocation; extreme offsets cannot wrap coordinate math.
Tests cover error precedence, malformed dimensions and native-depth placement.
The CPU renderer also validates image/auxiliary dimensions before processing and
uses fallible reservations for its image, blur, distance, normal and sampling
arrays. Errors propagate through Classic and Smart Render; parallel channel blurs
propagate allocation errors as well. Box-blur scratch is reused across three passes.
Tests cover invalid input lengths, capacity overflow and finite/repeatable output
for both Glass Edge bevel modes and all five output modes with blurs enabled.
These checks do not inject physical memory exhaustion or cover Rayon/driver-owned
allocations, nor certify arbitrary numerical parameters.

Smart Render input/auxiliary pixel checkouts now run checkin on returned errors
and Rust unwinding, preserving the original failure. Closure-injection tests
verify exactly-once cleanup and auxiliary-before-input ordering. Layer-assignment
query errors also propagate rather than being interpreted as unassigned. These
tests do not simulate native AE faults, process aborts or allocator exhaustion.

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

- GPU upload packing validates exact source length and allocates fallibly, including
  input-only output mode. Source/mask/background conversions are uploaded and released
  one at a time instead of retaining three packed vectors through readback. Queue-owned
  staging is separate; this does not establish a process-memory or VRAM reduction.
- GPU readback decodes mapped bytes directly into a fallibly allocated native
  output vector, removing the intermediate packed-word copy (4 bytes/pixel for
  byte output, 16 bytes/pixel for deep/float). A drop guard unmaps after the mapped
  view is released, including early return/unwind. Tests cover all nominal deep
  codes, signed/HDR samples and malformed readback lengths. This is an allocation
  reduction, not a measured speed/RSS result or full allocator/device-loss testing;
  CPU-renderer and driver allocations remain separate.
- The internal GPU cache retains at most eight worker states and 1 GiB of requested
  buffer storage. Least-recently-used states and replaced-size buffers are released
  before allocation; an oversized single frame returns the existing CPU fallback.
  Actual-GPU tests use a reduced budget and twelve successive workers, compare
  accounting against buffer sizes, preserve signed/HDR identity and reject oversized
  requests before allocation. This budget excludes driver/pipeline memory and is
  not a measured VRAM or process-RSS ceiling; worker turnover is not concurrent MFR.
- Same-size GPU state replay changes source, partial-alpha mask, background,
  blur, brightness, mix and four output modes on one processor. Each changed
  result matches a newly created processor byte-for-byte, then restoring the
  original inputs/parameters reproduces the baseline exactly. This checks stale
  internal wgpu state, not CPU parity, native AE GPU-worlds or concurrent MFR.
- Headless audit (2026-10-04): fixed absent auxiliary layers being treated as
  black images in Smart Render. Default Classic/Smart output now matches.
  Assigned off-frame masks retain the empty-mask fallback; explicit auxiliary
  layers and ROI still need separate host fixtures.
- Fixed the byte-only 16/32-bit host/CPU/GPU transport. The headless sweep now
  passes 33 checks, including four previously failing deep identities and active
  deep Classic/Smart agreement. Twenty old byte fixtures remain hash-identical.
  Nine unit tests cover native low bits, HDR, auxiliary offsets, interpolation,
  blur and real GPU format/size changes. PNG8 host fixtures alone do not prove
  native input precision; GPU-requested fixtures alone do not prove GPU execution.
- The GPU regression also checks exact zero-mix output across same-device
  16x16, 13x7, 1x1 and 16x16 size changes; this is internal wgpu, not native AE GPU.

- Windows release build script is used during development.
- `wgpu` smoke test passes on the development machine.
- CPU rendering remains the reference path.
- Native ARGB8/16/32f transport is implemented. Full effect CPU/GPU parity,
  explicit auxiliary/ROI fixtures, device-loss and memory stress remain open.
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
