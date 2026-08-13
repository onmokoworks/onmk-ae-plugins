"""
Export Depth Anything V2 Small to ONNX.

Usage:
    pip install torch onnx onnxruntime transformers pillow numpy
    python scripts/export_onnx.py --out model/depth_anything_v2_small.onnx
"""
from __future__ import annotations

import argparse
from pathlib import Path

import onnx
import torch
from transformers import AutoModelForDepthEstimation

MODEL_ID = "depth-anything/Depth-Anything-V2-Small-hf"
INPUT_SIZE = 518  # training resolution


def export(out_path: Path, size: int = INPUT_SIZE, opset: int = 17,
           static_shape: bool = False) -> None:
    model = AutoModelForDepthEstimation.from_pretrained(MODEL_ID)
    model.eval()

    dummy = torch.randn(1, 3, size, size, dtype=torch.float32)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    # The HF Depth Anything V2 model bakes the position-embedding grid into the
    # graph, so declared dynamic H/W axes don't actually work at runtime.
    # static_shape=True drops the dynamic axes and produces a model fixed to
    # `size`, which is what we need for per-resolution preview presets.
    dyn_axes = None if static_shape else {
        "pixel_values": {0: "batch", 2: "height", 3: "width"},
        "predicted_depth": {0: "batch", 1: "height", 2: "width"},
    }
    torch.onnx.export(
        model,
        (dummy,),
        out_path.as_posix(),
        input_names=["pixel_values"],
        output_names=["predicted_depth"],
        dynamic_axes=dyn_axes,
        opset_version=opset,
        do_constant_folding=True,
    )

    # Consolidate external data into a single .onnx file for easier distribution.
    data_path = out_path.with_suffix(out_path.suffix + ".data")
    if data_path.exists():
        model_proto = onnx.load(out_path.as_posix(), load_external_data=True)
        onnx.save(model_proto, out_path.as_posix(), save_as_external_data=False)
        data_path.unlink()

    print(f"exported: {out_path}  ({out_path.stat().st_size / 1e6:.1f} MB)")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=Path("model/depth_anything_v2_small.onnx"))
    ap.add_argument("--size", type=int, default=INPUT_SIZE)
    ap.add_argument("--opset", type=int, default=17)
    ap.add_argument("--static-shape", action="store_true",
                    help="bake the input H/W into the graph (no dynamic axes)")
    args = ap.parse_args()
    export(args.out, args.size, args.opset, args.static_shape)
