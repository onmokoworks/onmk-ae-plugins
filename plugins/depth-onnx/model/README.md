# Model packs

The plugin does **not** hard-code a specific architecture. Each model pack is a
directory with a `manifest.json` plus one or more ONNX files.

## Layout

```
model/
└── depth_anything_v2_small/     # example pack (not committed if *.onnx gitignored)
    ├── manifest.json
    ├── depth_anything_v2_small.onnx
    ├── depth_anything_v2_small_266.onnx
    └── depth_anything_v2_small_392.onnx
```

At install time, copy model packs into the plug-in’s user **`models/`** directory
(next to the binary, **not** inside the signed macOS bundle):

- **macOS:** `.../MediaCore/DepthONNX/models/<pack>/`
- **Windows:** `.../Plug-ins/Effects/DepthONNX/models/<pack>/`

`scripts/install_dev.sh` / `install_dev.bat` はフォルダ作成と `manifest.json` テンプレ配置まで行います。  
**`.onnx` は `./scripts/setup_models.sh`（macOS）でリポジトリからミラーするか、各自 export してください。**

## manifest.json

```json
{
  "id": "depth_anything_v2_small",
  "label": "Depth Anything V2 Small",
  "preprocess": "imagenet_rgb",
  "input_name": "pixel_values",
  "output_name": "predicted_depth",
  "variants": {
    "266": "depth_anything_v2_small_266.onnx",
    "392": "depth_anything_v2_small_392.onnx",
    "518": "depth_anything_v2_small.onnx"
  }
}
```

| Field | Meaning |
|-------|---------|
| `id` | Stable identifier (used internally) |
| `label` | Shown in the **Model** popup |
| `preprocess` | Currently only `imagenet_rgb` |
| `input_name` / `output_name` | ONNX Runtime tensor names |
| `variants` | Map of input size → ONNX filename **in the same folder** |

The **Resolution** effect control (266 / 392 / 518) picks which variant file to load.

## Exporting Depth Anything V2 Small (reference)

```bash
python -m venv .venv && source .venv/bin/activate
pip install -r scripts/requirements.txt

# 518 (final quality)
python scripts/export_onnx.py --static-shape --size 518 \
  --out model/depth_anything_v2_small/depth_anything_v2_small.onnx

# preview sizes
python scripts/export_onnx.py --static-shape --size 266 \
  --out model/depth_anything_v2_small/depth_anything_v2_small_266.onnx
python scripts/export_onnx.py --static-shape --size 392 \
  --out model/depth_anything_v2_small/depth_anything_v2_small_392.onnx
```

Weights come from Hugging Face `depth-anything/Depth-Anything-V2-Small-hf` (Apache-2.0).
ONNX files are large and gitignored; generate them locally or ship separately.

## Adding another model

1. Create `model/my_model/manifest.json` (+ ONNX files).
2. Match the IO contract or extend `preprocess` in `Inference.cpp`.
3. Rebuild or copy the folder into the installed `models/` directory.
4. Restart After Effects (model list is scanned at plug-in load).
