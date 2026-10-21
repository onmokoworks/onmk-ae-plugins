# onmk Inner Shadow

An After Effects effect that casts a soft shadow inside the input alpha silhouette, including holes. The original alpha is preserved. Windows x64, version 0.1.0.

Controls: Blend Mode (Normal/Multiply), Color, Opacity, Angle, Distance, Choke, Size, and Render Device (Auto/GPU/CPU). Angle denotes the light direction: 0° right, 90° up. Distance and Size use square-pixel display units and account for downsample and pixel aspect ratio.

Auto prefers a hardware GPU through wgpu and falls back to CPU on GPU failure. GPU mode fails explicitly if GPU rendering is unavailable. CPU mode and zero opacity do not initialise the GPU. This is internal GPU acceleration of ordinary Render/SmartRender, not AE's native SmartRenderGPU API. The GPU handles choke, blur, shift, and colour compositing; CPU code handles world conversion and alpha preservation.

## Build and verification

From this directory, using Rust/MSVC:

```powershell
./build.ps1
./verify.ps1
```

`build.ps1` produces `dist/InnerShadow.aex` without installing it. `verify.ps1` requires a real hardware GPU, runs the tests, builds the AEX, and writes CPU/GPU comparison fixtures under `verification/`. Dependencies are pinned by Cargo.lock. No AE installation or restart is performed.

## Behaviour and limitations

- AE straight RGB at the boundary; 16-bit white is 32768. Float HDR and negative RGB are preserved. Normal and Multiply operate in the supplied working colour space.
- Image exterior is transparent: a full-frame opaque input also receives an inset shadow at its edges.
- Partial-alpha pixels can shadow themselves under the inverse-alpha model. Distance=Size=0 is therefore not identity on partial alpha; Opacity=0 is identity.
- GPU rendering is serialised per device; frames have independent buffers. Very large working domains fail explicitly rather than truncate the shadow. The total memory ceiling can exceed 1 GB; Auto can fall back to CPU for device-specific limits.
- Gaussian and choke are defined by this plugin; Adobe layer-style pixel identity is not claimed.
- No Noise, Contour, Global Light, or shadow-only output in this version.
- See [verification](docs/verification.md) for actual tested routes, artifact identity and remaining host checks. Passing mathematical/GPU tests does not establish AE validation.

[Design](docs/design.md) · [日本語](README.ja.md)
