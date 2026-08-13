# Design Notes

## Model
- **Depth Anything V2 Small** — ViT-S/14, ~25M params, Apache-2.0
- HF: `depth-anything/Depth-Anything-V2-Small-hf`
- Input: RGB, 518×518, ImageNet normalized
- Output: relative inverse depth map (1/disparity-like), needs per-frame min-max normalization for display

## Runtime
- ONNX Runtime, **one session per (resolution) preset** per AE instance.
  The HF Depth Anything V2 export bakes position-embedding shape into the
  graph (see `docs/benchmark.md`), so we ship one ONNX file per supported
  input size instead of using dynamic axes.
- **Providers** (priority order, revised after 2026-04-20 bench):
  - Windows: `DmlExecutionProvider` (DirectML) → `CPUExecutionProvider`
  - macOS: `CPUExecutionProvider` (default). CoreML EP regresses at 518 due
    to partial offload — only re-enable once we have a native `.mlpackage`
    via `coremltools`.
- Session IO binding with pre-allocated NCHW float32 tensors.

## Plugin (AE SmartFX)
- Effect class: `PF_OutFlag_PIX_INDEPENDENT`, SmartFX (`PF_OutFlag2_SUPPORTS_SMART_RENDER`)
- 16bpc path: convert to float32 NCHW on input, write 16bpc grayscale on output
- Parameters:
  - `Resolution` (popup): 518 / 392 / 266 — lower = faster preview
  - `Normalization` (popup): Per-frame / Fixed range
  - `Invert` (checkbox)
  - ~~`Precision`~~ — dropped. Bench shows FP16 gives no win on the ORT path
    (CPU EP up-casts; CoreML loses partition coverage). Revisit when DirectML
    or native CoreML is in.
- Threading: inference off the AE worker thread via a task queue; AE blocks only on the result it needs for the current render.

## Preview Strategy
1. When parameters/time change, kick inference for the displayed frame
2. Cache: `(clipId, time, resolution) → depth buffer`, LRU ~64 frames
3. For scrub/preview, downscale input to 266 or 392
4. For final render, force 518

## File Layout
```
model/                  # ONNX weights (gitignored)
scripts/                # export + reference inference
plugin/
  src/
    DepthAnything.cpp   # AE entry points
    Inference.cpp/.h    # ORT wrapper
    FrameCache.h
  cmake/                # FindORT.cmake etc.
  CMakeLists.txt
third_party/
  onnxruntime/          # prebuilt, per-platform
  AfterEffectsSDK/      # symlink to SDK install
```
