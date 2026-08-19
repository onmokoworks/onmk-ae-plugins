# EdgeSmith

EdgeSmith is a distance-field edge designer for Adobe After Effects. It is intended as a modern, deterministic alternative to Roughen Edges rather than a parameter-for-parameter clone.

## Features

- Signed-distance based expansion and erosion
- Multi-octave animated edge noise
- Independent edge balance, direction, stretch, sharpness, and detail protection
- Nearest-color propagation into newly expanded alpha
- 8/16/32-bpc CPU rendering
- wgpu compute path (Metal, Vulkan, or DX12) with automatic CPU fallback
- Smart Render, Multi-Frame Rendering, and expanded ROI support

## Build

```sh
./scripts/build_macos_release.sh
```

On Windows, run `scripts/build_release.ps1`. The packaged artifact is written under `target/release/`.

## GPU implementation

The GPU renderer builds inside/outside distance fields using Jump Flood passes, then evaluates the same deterministic fractal edge model as the CPU renderer. `SmartRenderGpu` is advertised only when GPU Acceleration is enabled. If adapter creation or shader execution fails, the render falls back to CPU.
