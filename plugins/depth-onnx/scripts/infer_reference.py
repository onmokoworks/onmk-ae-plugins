"""
Reference inference with the exported ONNX model — used to validate parity
before we call the same model from the AE plugin.

Usage:
    python scripts/infer_reference.py \
        --model model/depth_anything_v2_small.onnx \
        --image path/to/input.jpg \
        --out   out_depth.png
"""
from __future__ import annotations

import argparse
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image

# ImageNet normalization — Depth Anything V2 preprocessing
MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)
INPUT_SIZE = 518


def preprocess(img: Image.Image, size: int = INPUT_SIZE) -> np.ndarray:
    img = img.convert("RGB").resize((size, size), Image.BICUBIC)
    arr = np.asarray(img, dtype=np.float32) / 255.0
    arr = (arr - MEAN) / STD
    arr = arr.transpose(2, 0, 1)[None, ...]  # NCHW
    return np.ascontiguousarray(arr)


def postprocess(depth: np.ndarray, out_w: int, out_h: int) -> Image.Image:
    d = depth[0] if depth.ndim == 3 else depth
    d_min, d_max = float(d.min()), float(d.max())
    norm = (d - d_min) / max(d_max - d_min, 1e-8)
    img = Image.fromarray((norm * 255.0).astype(np.uint8), mode="L")
    return img.resize((out_w, out_h), Image.BICUBIC)


def run(model_path: Path, image_path: Path, out_path: Path, warmup: int = 2, iters: int = 5) -> None:
    providers = ort.get_available_providers()
    print(f"available providers: {providers}")
    sess = ort.InferenceSession(model_path.as_posix(), providers=providers)

    img = Image.open(image_path)
    x = preprocess(img)
    inp_name = sess.get_inputs()[0].name

    for _ in range(warmup):
        sess.run(None, {inp_name: x})

    times = []
    for _ in range(iters):
        t0 = time.perf_counter()
        out = sess.run(None, {inp_name: x})[0]
        times.append(time.perf_counter() - t0)
    print(f"inference: mean={np.mean(times)*1000:.1f}ms  min={np.min(times)*1000:.1f}ms")

    depth_img = postprocess(out, img.width, img.height)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    depth_img.save(out_path)
    print(f"saved: {out_path}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=Path, required=True)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--out", type=Path, default=Path("out_depth.png"))
    args = ap.parse_args()
    run(args.model, args.image, args.out)
