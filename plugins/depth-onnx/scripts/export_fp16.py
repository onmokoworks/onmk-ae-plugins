"""
Convert the FP32 ONNX model to FP16 using onnxconverter_common.

Usage:
    python scripts/export_fp16.py \
        --in  model/depth_anything_v2_small.onnx \
        --out model/depth_anything_v2_small_fp16.onnx
"""
from __future__ import annotations

import argparse
from pathlib import Path

import onnx
from onnxconverter_common import float16


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--in", dest="src", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--keep-io-fp32", action="store_true",
                    help="leave input/output in fp32 (cast inside the graph)")
    args = ap.parse_args()

    model = onnx.load(args.src.as_posix())
    fp16_model = float16.convert_float_to_float16(
        model,
        keep_io_types=args.keep_io_fp32,
        disable_shape_infer=True,
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(fp16_model, args.out.as_posix())
    print(f"saved: {args.out}  ({args.out.stat().st_size / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
