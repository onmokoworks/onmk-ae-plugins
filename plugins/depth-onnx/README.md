# Depth ONNX

Fixed per-frame normalization generating NaN from finite extreme ranges (for
example `[-f32::MAX, 0, f32::MAX]`). Normal ranges retain the previous f32 formula;
overflowing range differences or reciprocals use f64 arithmetic. Constant ranges
produce zero. Nonfinite, reversed or inconsistent extrema/data are rejected
before mutation. Direct regressions cover wide/subnormal ranges and ordinary
formula equivalence. This does not implement the unresolved Fixed range option
or prove model inference.

Synthetic aligned PF_LayerDef buffers now exercise the same CPU read/write loops
with format supplied explicitly: 18 layouts (8/16/32bit, thin/odd shapes, positive
and negative padded strides) retain input channels, including low-bit u16 and
signed/HDR float. All 36 constant-depth writes, including inversion, preserve row
padding and surrounding sentinels. This bypasses the host format suite and model
inference; it is not native AE world or arbitrary depth-field certification.

Native CPU world conversion now validates dimensions, non-null/aligned pointers,
signed stride, row capacity and checked address span before typed pixel access.
Valid negative row strides remain supported. Invalid layout and allocation failure
return errors instead of successful passthrough. Direct tests cover padded and
negative strides, thin/odd sizes and malformed/overflow metadata. These checks
cannot prove the host pointer's actual allocation size. The rebuilt DLL retains
the three input-identical missing-model outputs, still classified as notification
unverified rather than successful inference.

The direct missing-model contract regression feeds an empty catalog into model
selection, generates the actual PF_OutData notification, and checks fallback-copy
and checkin success/failure combinations. All four combinations preserve the
message flag and copy-error precedence; successful synthetic u16 copying retains
low bits. This composes the helpers with a synthetic copy callback: it does not
execute Smart Render in AE, prove native copy behavior, or turn the three
headless `blocked_error_notification_unverified` cases into passes.

[日本語](./README.ja.md) | [English](./README.md)

![Depth ONNX UI](./docs/depth-onnx-ui.png)

Depth ONNX is an Adobe After Effects SmartFX plug-in that runs monocular depth estimation through ONNX Runtime.

ONNX model packs are not bundled. Place folders that contain `manifest.json` and `.onnx` files under the `models/` directory described below. This repository ships a sample manifest for [Depth Anything V2 Small](https://huggingface.co/depth-anything/Depth-Anything-V2-Small-hf) at `model/depth_anything_v2_small/manifest.json`.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `Depth ONNX`
- After Effects match name: `ANTH DepthONNX`
- Plugin file name:
  - Windows: `DepthONNX.aex`
  - macOS: `DepthONNX.plugin`

## Main Features

- Monocular depth inference via ONNX Runtime (DirectML on Windows, CPU on macOS)
- Generic model packs driven by `manifest.json` (not limited to Depth Anything)
- Resolution presets 266 / 392 / 518 (depends on pack `variants`)
- Per-frame normalization, invert, and temporal smoothing
- Optional model root via Browse Model Folder
- After Effects Smart Render support

## Validation Status

Acquired Smart Render input is checked in before propagating an unwinding render panic. A failure-first helper regression verifies exactly-once checkin and preservation of the original panic; this does not cover process abort, driver faults or AE-level panic recovery.

- Tested on Adobe After Effects 2025 / 2026 / Windows
- End-to-end validation with Depth Anything V2 Small (exported ONNX)
- macOS build recipes are included; on-machine validation is not documented here
- Adapter unit tests cover pixel formats, depth shape/finite checks, interpolation, smoothing-history reuse/release and input checkin on success/failure. Run `cargo test --release --locked --offline -p depthonnxae` from this directory.
- Model/runtime/inference failures set the existing error notification and copy input to output. The latest headless audit confirmed passthrough at8/16/32 bpc but did not observe the expected error notification. This is not an inference or complete error-contract pass.
- Smoothing history retains at most256 entries and256MiB of vector capacity. Growth beyond either limit returns OutOfMemory without evicting history or changing the current frame; allocations are fallible. Small-budget unit tests cover rejection, retained history and capacity reuse after disabling an entry. This is not a process RSS or peak-allocation bound (replacement can temporarily retain old and new storage).
- SequenceSetdown removes that effect's smoothing history; GlobalSetdown clears all histories and map storage. Unit tests verify repeated removal, isolation of other keys and a fresh history when a key is reused. Actual AE lifecycle/MFR scheduling is not certified by these tests.
- Actual model inference and nonsequential/MFR history behavior remain unverified in this audit.

## Build

### Windows

Requires Visual Studio 2022 (MSVC), Rust, optional `just`, and the ONNX Runtime GPU package under `third_party/onnxruntime-win-x64-gpu-*`.

`just release` may fail because `AdobePlugin.just` points `AESDK_ROOT` at a non-existent `sdk/AfterEffectsSDK`. The reliable flow is:

```powershell
cd crates\depth-onnx-ae
Remove-Item Env:AESDK_ROOT -ErrorAction SilentlyContinue
$env:CARGO_TARGET_DIR = "..\..\target"
cargo build --release
Copy-Item -Force ..\..\target\release\depthonnxae.dll ..\..\target\release\DepthONNX.aex
Copy-Item -Force ..\..\third_party\onnxruntime-win-x64-gpu-1.24.4\lib\*.dll ..\..\target\release\
```

Output:

```text
target\release\DepthONNX.aex
target\release\onnxruntime.dll
target\release\onnxruntime_providers_shared.dll
```

To build against a live AE SDK with bindgen, set `AESDK_ROOT` to your SDK root before `cargo build`.

### macOS

```bash
cd crates/depth-onnx-ae
export CARGO_TARGET_DIR=../../target
# export AESDK_ROOT=/path/to/AfterEffectsSDK  # optional; vendored bindings used when unset
just release-bundle
```

Place `third_party/onnxruntime-osx-arm64/lib/libonnxruntime.1.24.4.dylib` before running `release-bundle` (the `post-bundle` step copies it into `Contents/Frameworks/`).

## Installation

### Windows (MediaCore)

Close After Effects, then run PowerShell as administrator:

```powershell
$dst = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
$repo = (Get-Location).Path

Copy-Item -Force "$repo\target\release\DepthONNX.aex" $dst
Copy-Item -Force "$repo\target\release\onnxruntime.dll" $dst
Copy-Item -Force "$repo\target\release\onnxruntime_providers_shared.dll" $dst

$models = "$dst\models\depth_anything_v2_small"
New-Item -ItemType Directory -Force -Path $models | Out-Null
Copy-Item -Force "$repo\model\depth_anything_v2_small\*" $models
```

Destination:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
  DepthONNX.aex
  onnxruntime.dll
  onnxruntime_providers_shared.dll
  models\<pack>\
    manifest.json
    *.onnx
```

Restart After Effects and apply the effect from `Effects > Depth > Depth ONNX`.

Helper script: [`scripts/install_dev.bat`](scripts/install_dev.bat) targets `Effects\DepthONNX\`. For MediaCore installs, prefer the copy steps above.

### macOS

```bash
./scripts/install_dev.sh
```

Destination:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
  DepthONNX.plugin
  DepthONNX/models/<pack>/
    manifest.json
    *.onnx
```

Generated `.aex` / `.plugin` / `.dll` artifacts are not committed to Git.

## Models

ONNX weights are not in the repository (`.gitignore`). Generate them with the export script:

```bash
pip install -r scripts/requirements.txt
pip install onnxscript

python scripts/export_onnx.py --static-shape --size 518 --out model/depth_anything_v2_small/depth_anything_v2_small.onnx
python scripts/export_onnx.py --static-shape --size 266 --out model/depth_anything_v2_small/depth_anything_v2_small_266.onnx
python scripts/export_onnx.py --static-shape --size 392 --out model/depth_anything_v2_small/depth_anything_v2_small_392.onnx
```

Weights are downloaded from [Hugging Face: Depth-Anything-V2-Small-hf](https://huggingface.co/depth-anything/Depth-Anything-V2-Small-hf) on first export (cache: `~/.cache/huggingface/`).

See [`model/README.md`](model/README.md) for pack layout.

## Parameters

- `Model`: select a scanned model pack
- `Browse Model Folder`: pick an extra model root (`Choose…`)
- `Resolution`: `266 (preview)` / `392 (preview HQ)` / `518 (final)`
- `Normalization`: `Per-frame` / `Fixed range`
- `Invert`: invert the depth output
- `Temporal Smoothing`: exponential smoothing over time (0.00–0.95)

## Development Checks

```powershell
cargo fmt
cargo check
cargo build --release
```

```bash
# macOS
cd crates/depth-onnx-ae && just release-bundle
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `AESDK_ROOT` | AE SDK root for bindgen builds |
| `CARGO_TARGET_DIR` | Cargo output directory (default: `target/`) |
| `ORT_DYLIB_PATH` | Explicit ONNX Runtime library path (dev) |
| `AE_DEPTH_ONNX_MODEL_DIR` | Extra model scan root |
| `AE_DEPTHANYTHING_MODEL_DIR` | Legacy alias of the above |

## Limitations

- Engine configuration compares the entire manifest, not only its ID: revised variant filenames or tensor names update the configuration and invalidate cached sessions. Model-free regression tests cover five manifest fields, repeated configuration, and directory changes. Actual session destruction/reload with ONNX Runtime is not tested here; unchanged files replaced in place and automatic catalog rescans are not covered.
- The `Fixed range` Normalization selection is currently ignored: rendering always uses per-frame extrema. No fixed-range bounds are defined in the manifest. This remains an unresolved parameter contract, not verified functionality.
- Preprocessing clamps continuous resize coordinates before bilinear interpolation, preserving source edge pixels when enlarging small inputs. A model-independent 2x2-to-4x4 regression checks corners/interior in ARGB and RGBA order; real-model output is not certified by this test.
- Error notifications are bounded to 255 UTF-8 bytes, explicitly NUL-terminated, and sanitize embedded NULs. Adapter regression tests exercise long ASCII/Unicode messages and reused output-message storage; this does not certify host error display or real inference.
- Each model pack must match its `manifest.json` IO names and preprocess mode
- Windows: `onnxruntime.dll` must sit next to `DepthONNX.aex`
- macOS: ORT dylib is bundled under `Contents/Frameworks/` via `release-bundle`
- Depth inference is expensive; choose resolution / preview presets carefully
- Adapter unit tests cover pixel-format dispatch, depth shape/finiteness, sampling, smoothing storage reuse/release, and input checkin on success/error paths. Run `cargo test --release --locked --offline -p depthonnxae` from this directory.
- Model/runtime/inference failures copy the input to the output while reporting the existing user error. The headless audit compares raw Smart Render input checkpoints against output at 8/16/32 bpc, including matching dimensions, format, channel order, origin and stride. All three are byte-identical, but error notification and execution of the intended model-failure branch remain unobserved. These cases are classified `blocked_error_notification_unverified`, never a passing inference or negative-path test; the audit command still exits nonzero. The former expectation that the render command itself must fail did not match this notification-plus-passthrough contract.
- Direct smoothing-cache tests run eight independent instance keys concurrently for 200 updates each, with four dimensions, repeated history removal, and exact comparison to per-instance recurrence. They check retained vector capacity against a reduced test budget on every update and release map storage at the end. Existing tests reject over-budget growth without changing the frame or evicting other histories. This covers cache internals, not process RSS, model inference, actual AE MFR, or concurrent/nonsequential requests for the same instance; those remain unverified.

## License

- Plug-in: MIT License. See [LICENSE](LICENSE).
- Depth Anything V2: Apache-2.0 (obtain separately)
- ONNX Runtime / AE SDK: follow their respective licenses
