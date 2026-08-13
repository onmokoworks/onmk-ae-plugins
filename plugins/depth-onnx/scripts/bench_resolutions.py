"""
Sweep ONNX inference latency across input resolutions and providers.

Usage:
    python scripts/bench_resolutions.py \
        --model model/depth_anything_v2_small.onnx \
        --image model/test_input.png \
        --sizes 266 392 518 \
        --providers coreml cpu

Sizes must be multiples of 14 (ViT-S/14 patch size). FP16 path requires
`--fp16-model model/depth_anything_v2_small_fp16.onnx` (build it with
scripts/export_fp16.py).
"""
from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from statistics import mean

import numpy as np
import onnxruntime as ort
from PIL import Image

MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)

PROVIDER_MAP = {
    "coreml": "CoreMLExecutionProvider",
    "cpu": "CPUExecutionProvider",
    "dml": "DmlExecutionProvider",
}


def preprocess(img: Image.Image, size: int, dtype: np.dtype) -> np.ndarray:
    if size % 14 != 0:
        raise ValueError(f"size {size} must be a multiple of 14 (ViT patch)")
    img = img.convert("RGB").resize((size, size), Image.BICUBIC)
    arr = np.asarray(img, dtype=np.float32) / 255.0
    arr = (arr - MEAN) / STD
    arr = arr.transpose(2, 0, 1)[None, ...]
    return np.ascontiguousarray(arr.astype(dtype))


def bench_one(model_path: Path, image: Image.Image, size: int, provider: str,
              dtype: np.dtype, warmup: int, iters: int) -> dict:
    sess = ort.InferenceSession(model_path.as_posix(), providers=[provider, "CPUExecutionProvider"])
    actual = sess.get_providers()
    x = preprocess(image, size, dtype)
    inp = sess.get_inputs()[0].name

    for _ in range(warmup):
        sess.run(None, {inp: x})

    times = []
    for _ in range(iters):
        t0 = time.perf_counter()
        sess.run(None, {inp: x})
        times.append((time.perf_counter() - t0) * 1000.0)

    return {
        "size": size,
        "dtype": str(dtype),
        "requested_provider": provider,
        "active_providers": actual,
        "iters": iters,
        "mean_ms": round(mean(times), 1),
        "min_ms": round(min(times), 1),
        "max_ms": round(max(times), 1),
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=Path, required=True)
    ap.add_argument("--fp16-model", type=Path, default=None)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--sizes", type=int, nargs="+", default=[266, 392, 518])
    ap.add_argument("--providers", nargs="+", default=["coreml", "cpu"],
                    choices=list(PROVIDER_MAP.keys()))
    ap.add_argument("--warmup", type=int, default=2)
    ap.add_argument("--iters", type=int, default=5)
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    image = Image.open(args.image)
    available = ort.get_available_providers()
    print(f"available providers: {available}")

    runs: list[dict] = []
    targets = [(args.model, np.float32, "fp32")]
    if args.fp16_model is not None:
        targets.append((args.fp16_model, np.float16, "fp16"))

    for model_path, dtype, label in targets:
        for prov_key in args.providers:
            ep = PROVIDER_MAP[prov_key]
            if ep not in available:
                print(f"skip: {ep} not available")
                continue
            for size in args.sizes:
                try:
                    r = bench_one(model_path, image, size, ep, dtype,
                                  args.warmup, args.iters)
                except Exception as e:
                    r = {"size": size, "requested_provider": ep, "dtype": label,
                         "error": repr(e)}
                r["model"] = model_path.name
                r["precision"] = label
                runs.append(r)
                short_eps = ",".join(p.replace("ExecutionProvider", "") for p in r.get("active_providers", []))
                if "error" in r:
                    print(f"  {label:>4} {ep:<28} size={size:<3} ERROR: {r['error']}")
                else:
                    print(f"  {label:>4} {ep:<28} size={size:<3} mean={r['mean_ms']:>6}ms  min={r['min_ms']:>6}ms  active=[{short_eps}]")

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(runs, indent=2))
        print(f"saved: {args.json_out}")


if __name__ == "__main__":
    main()
