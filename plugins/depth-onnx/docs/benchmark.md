# Baseline Benchmark

## 2026-04-20 — First export, M-series Mac

- **Model**: Depth Anything V2 Small, exported via `transformers` + `torch.onnx.export`
- **File**: `model/depth_anything_v2_small.onnx` — 100.8 MB (FP32, consolidated)
- **Input**: 518×518 NCHW, ImageNet normalization
- **ORT**: 1.24.4
- **Provider**: CoreMLExecutionProvider (partial: 512/725 nodes offloaded)

### Latency (5 iters, 2 warmup)
- mean: **986 ms**
- min:  **772 ms**

### Observations
- Roughly 1 fps at 518×518 — too slow for live preview.
- CoreML EP partial offload is costly; need to investigate:
  - Export with coremltools directly (full CoreML model) instead of ORT CoreML EP
  - FP16 quantization
  - Lower input resolution (266 → likely ~3-4× faster)
- On Windows the target is DirectML EP; expect similar ballpark on mid-range dGPU, faster on high-end.

---

## 2026-04-20 — Resolution × precision sweep, M-series Mac

Run with `scripts/bench_resolutions.py`. Results saved at `docs/bench_results.json`.

### Pre-finding: dynamic axes are a lie
The HF Depth Anything V2 export bakes the position-embedding grid (`1370 = (518/14)² + 1`) into the graph as constants. Declared `dynamic_axes` are accepted by ONNX but fail at runtime with broadcast errors when H/W differ from the export-time value.

**Workaround:** export one ONNX per target size (`--static-shape`). Files:
- `depth_anything_v2_small_266.onnx` (100.6 MB)
- `depth_anything_v2_small_392.onnx` (100.6 MB)
- `depth_anything_v2_small.onnx` (518, original, 100.8 MB)
- `*_fp16.onnx` variants — ~51 MB each

### Latency (5 iters, 2 warmup)

| size | provider | precision | mean ms | min ms | fps  |
|------|----------|-----------|---------|--------|------|
| 266  | CoreML   | fp32      |   86.6  |  82.9  | 11.5 |
| 266  | CPU      | fp32      |   89.2  |  88.2  | 11.2 |
| 266  | CoreML   | fp16      |   96.7  |  94.4  | 10.3 |
| 266  | CPU      | fp16      |   97.8  |  94.6  | 10.2 |
| 392  | CPU      | fp32      |  219.6  | 205.6  |  4.6 |
| 392  | CoreML   | fp32      |  288.2  | 243.3  |  3.5 |
| 392  | CoreML   | fp16      |  278.9  | 254.5  |  3.6 |
| 392  | CPU      | fp16      |  272.6  | 226.0  |  3.7 |
| 518  | CPU      | fp32      |  414.6  | 412.4  |  2.4 |
| 518  | CoreML   | fp32      | 1086.4  | 810.0  |  0.9 |
| 518  | CPU      | fp16      |  463.0  | 448.7  |  2.2 |
| 518  | CoreML   | fp16      |  456.7  | 452.3  |  2.2 |

### Conclusions
1. **Use CPU EP as the default on Mac** with the current ORT-only path. CoreML EP either matches CPU (266) or regresses (518: 1086 vs 414 ms). Partial-offload round-trips dominate at higher resolution.
2. **FP16 gives no benefit on this path.** CPU EP up-casts back to fp32. CoreML loses partition coverage on the fp16 graph (48/735 vs 512/725 supported nodes) and regresses everywhere except 518 where it ties CPU.
3. **Real GPU acceleration on Mac requires bypassing ORT** — convert directly with `coremltools` to a fully native `.mlpackage` (no per-op offload). Deferred to Task #4-b.
4. **Windows DirectML** path is untested; expected to be the dominant production target since DirectML covers full ViT graphs without partial offload.

### Preview strategy (revised)
- **Scrub / interactive**: 266×266, CPU EP → ~90 ms (≈11 fps). Fine for live param tweaking.
- **RAM preview**: 392×392, CPU EP → ~220 ms (≈4.5 fps). Reasonable when paired with the LRU frame cache.
- **Final render**: 518×518, CPU EP → ~415 ms/frame.
- Drop the FP16 popup from the param schema until we have a backend that actually exploits it (CoreML native or DirectML).

### Next perf work
1. Native CoreML conversion via `coremltools` — should eliminate the partial-offload tax at 518.
2. Validate on Windows + DirectML once the SmartFX skeleton is up.
3. Consider INT8 quantization (`onnxruntime.quantization`) for the CPU fallback path.
