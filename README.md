# TimeSlice

TimeSlice is an Adobe After Effects effect plug-in written in Rust. It creates slit-scan style images by sampling each pixel from a different frame in time.

## Features

- Horizontal, vertical, radial, and map-layer driven time slicing
- Past, future, and bidirectional time ranges
- Nearest or linear interpolation between sampled frames
- Blend control for mixing the result with the original frame
- Optional GPU compute path with CPU fallback
- Smart Render and threaded rendering support

## Parameters

| Parameter | Description |
| --- | --- |
| Time Frames | Number of frames covered by the time scan. |
| Frame Step | Step size between sampled frames. |
| Time Direction | `Past`, `Future`, or `Both`. |
| Slice Mode | `Horizontal`, `Vertical`, `Radial`, or `Map Layer`. |
| Gradient Phase | Offsets the generated gradient map. |
| Center | Center point used by radial mode. |
| Interpolation | `Nearest` or `Linear` frame sampling. |
| Mix with Original | Blend amount between the processed result and the original frame. |
| Time Map | Optional layer used as a custom luminance map. |
| Invert Map | Inverts generated or layer-based time maps. |

## Build

```powershell
cargo build --release
```

The compiled plug-in binary is emitted under `target/release`.

## Development Checks

```powershell
cargo check
cargo test
```

## Notes

This project depends on the Rust `after-effects` crate and the Adobe After Effects SDK toolchain expected by that crate. GPU acceleration uses `wgpu`; unsupported environments fall back to CPU rendering.
