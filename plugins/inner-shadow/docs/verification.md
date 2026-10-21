# Verification — 2026-10-21

Implementation: onmk Inner Shadow 0.1.0, new `plugins/inner-shadow` directory in the existing monorepo. Existing plugins and shared tooling were not edited. Opus 5.5 revised the design and authored the main implementation and tests; Codex completed build/fixture tooling, documentation, corrected GPU wait timeout behaviour, and ran the checks below.

## Completed

- `cargo test --locked -- --nocapture`, with `INNER_SHADOW_REQUIRE_GPU=1`: **46 passed** (26 unit tests, 15 independent CPU/oracle tests, 5 hardware GPU tests), no skips.
- Hardware: **NVIDIA GeForce RTX 4070 Ti**, Vulkan, DiscreteGpu, driver NVIDIA 610.88. GPU tests assert the GPU route and dispatched work; CPU fallback is not counted as GPU success.
- Maximum absolute error: CPU vs independent f64 oracle `1.8521190980891333e-7`; GPU vs oracle `1.8111572885892713e-7`; GPU vs CPU `1.1920928955078125e-7`.
- Partial alpha ladder, deliberate premultiplied negative control, fractional displacement, angle direction, hole boundaries, HDR/negative RGB, tiled/nonzero-origin rendering, downsample/PAR, failure fallback, and concurrent backend calls covered. Pixel adapters separately verify ARGB order, 32768 normalisation, integer precision and negative-stride indexing.
- `cargo fmt --check` passes. `build.ps1` builds the Windows x64 AEX successfully against the pinned after-effects 0.3.0 revision.
- `cargo run --locked --release --example render_fixture -- verification`: actual CPU/GPU renders saved as raw RGBA f32 and PPM. A checkerboard comparison was inspected visually: inward shadow, hole edges and CPU/GPU appearance agree. Fixture maximum CPU/GPU error `1.1920929e-7`.
- No install, AE launch or AE restart performed.

The first-use GPU example included initialisation costs and was slower than CPU. No real-time or general speedup claim is made. Actual production frame sizes, parameter ranges and warmed GPU timing need separate measurement.

## Artifact identity

`dist/InnerShadow.aex`, 5,238,272 bytes, SHA256:

```text
15C7715AB866F62F6B178CC95755AC27EC38559FB6ED48F85D174A733E4151D4
```

`verification/source-sha256.json` records hashes of the build inputs and tests. Raw fixtures and comparison images are local ignored outputs in `verification/`; `dist/` and `target/` are also ignored.

## Host validation boundary

The available AEXCompat `audit-harness-release-20260724/release/aexcompat-harness.exe` failed during its own worker audit on `--inspect-experimental`, exit 1:

```text
worker module audit validation failed: secure worker module audit schema is invalid
```

Harness SHA256: `ABB802CB07D5A54A936B46B7424CDE4F7BE7E12859F9011DE3C8EEBBD8FC7586`.

This is a host validation failure, not successful AEX loading, parameter enumeration or rendering. No host guards were bypassed and the host was not modified.

| Route | Implemented | Verified here |
|---|---|---|
| CPU mathematical backend | yes | independent expected results |
| wgpu hardware backend | yes | actual RTX 4070 Ti execution and oracle |
| ARGB8/16/32F pixel adapters | yes | codec/stride unit tests |
| Legacy Render 8/16 | yes | compiled, host rendering unverified |
| SmartRender 8/16/32 | yes | compiled, host rendering unverified |
| AE native SmartRenderGPU | no | not advertised |
| AE real host / MFR / crop / PAR / preview scaling | intended | unverified |
| Adobe Inner Shadow pixel equivalence | no claim | unverified |

An AE check should include ordinary and cropped input worlds, full/half resolution, non-square pixels, alpha edges, all supported depths and Auto/GPU/CPU modes. The mathematical tests do not establish host buffer-origin behaviour.

## Crisp edge settings

Set Choke=100% to remove Gaussian softness. For an inward outline use Distance=0, Size=desired thickness, Opacity=100%. The original input alpha is retained, so antialiased/partial-alpha edges are not forcibly binarised. A threshold control is not implemented.

## Deployment — 2026-10-21

At the user's explicit request, InnerShadow.aex was copied to the Adobe Common Plug-ins/7.0/MediaCore/onmk directory. Source and destination SHA256 both match the artifact identity above. AE was not started or restarted; actual AE loading remains unverified.
