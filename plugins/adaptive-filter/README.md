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

The host spatial-map check passes 108 renders and 36 exact pixel comparisons
on the current audited DLL. A patterned 7x11 PNG map controls 17x31 output;
nearest coordinates and half-up radius selection are computed in the runner,
then pixels are selected from four no-map reference renders. This covers three
filters, three output depths, both render paths and inversion with one iteration.
It does not independently validate filter kernels, native deep input, translated
layer origins or ROI.

A direct spatial-map regression compares each output pixel with independently
selected constant-radius renders. Literal expected radii straddle the 0.375,
0.625 and 0.875 transitions for radius 4. All 36 comparisons pass across three
filters, one/two iterations, thin/odd shapes and native u16/signed-HDR f32 input.
This isolates radius selection and iterative map placement; the reference uses
the same filter kernels, so it is not an independent proof of those kernels or
host map resampling.

Classic map checkout and Smart assigned-map checkout errors now propagate.
PreRender determines optional-map presence before requesting pixels and passes
that state to Smart Render, so no-map rendering does not rely on swallowing a
missing-world error. The optional decode regression also checks acquisition
failure without calling the decoder. Actual native AE callback fault injection
remains outside the headless checks.

Explicit 7x11 black/white luminance maps on 17x31 input were checked across all
three filters, 8/16/32bpc, Classic/Smart and inversion: 108 renders and 18 comparisons
pass on the current audited DLL. White matches radius 4 without a map; black
matches radius 1, and inversion exchanges them. Radius 1 and 4 must produce
different output to avoid a vacuous pass. This proves constant-map routing,
not spatial resampling, native deep-map input precision, ROI or MFR. The historical
`adaptive-map-bounds` host run contains only no-map identity cases; its name is
not evidence of explicit-map host coverage.

- Current production render path is CPU based
- `SmartRenderGpu` falls back to CPU
- CPU pixel path retains native ARGB8, ARGB16 and ARGB32f channels (2026-10-04).
  All three filters use the original color-distance scale without reducing
  deep input to 8 bits. Float RGB is not clamped to SDR. Unit regressions cover
  all 32769 nominal 16-bit codes, active sub-byte filtering, HDR and negative
  values. Host fixtures use PNG8 sources and therefore do not alone certify
  full native input precision. Explicit map layers, ROI and resource stress
  remain separate audit work.
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

## Map validation

Filter processing validates exact frame/map lengths, safe dimensions and bounded
radius/iterations before allocating. Working arrays reserve fallibly and return
errors through Classic/Smart paths. Final Mix reuses the spare iteration buffer,
removing one full-frame allocation without changing iteration or filter math.
Nine16-bit filter/iteration comparisons cover the former separate-output path;
this is not a process memory cap or physical allocation-failure injection.

Map resampling rejects empty/malformed source images and overflowing destination
sizes before indexing. Destination reservation is fallible; nearest-neighbor index
products use wide integers and empty destinations return immediately. Tests cover
black/white resizing and inversion. This does not yet cover all filter working
allocations or native host fault injection; existing missing-map fallback is unchanged.
Luminance-map allocation is now fallible too. Once a map is acquired, Classic and
Smart paths propagate conversion/shape/allocation errors rather than treating them
as absent maps. Checkout failure still retains the old optional-map behavior;
its error classification requires separate work. Smart rendering checks in only
successfully acquired input/map worlds, on success/error/unwind. Nested cleanup
tests cover optional maps and checkin failures, preserving the primary work error;
these are closure tests, not native AE callback fault injection.

## License

MIT License. See [LICENSE](LICENSE).
