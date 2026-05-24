# Depth ONNX

After Effects 2025+ 向け SmartFX プラグイン。
ONNX Runtime を用いて深度マップを推論できます。

Depth Anything などの ONNX モデルは同梱しません。下記 `DepthONNX/models/` にモデルフォルダ（`manifest.json` と `.onnx`）を置いてください。  
リポジトリには [Depth Anything V2 Small](https://huggingface.co/depth-anything/Depth-Anything-V2-Small-hf) 向けの設定例（`model/depth_anything_v2_small/manifest.json`）が入っています。プラグインは Depth Anything 専用ではなく、各フォルダの `manifest.json` に書かれた ONNX を読み込んで推論する汎用の Depth 推論ホストです。

## モデルの置き場所

プラグイン横の `DepthONNX/models/<pack>/` をスキャンする（Browse Model Folder 指定時はそちらが優先）。

**macOS**

```
.../Plug-ins/7.0/MediaCore/
  DepthONNX.plugin
  DepthONNX/models/<pack>/
    manifest.json
    *.onnx
```

**Windows**

```
.../Plug-ins/Effects/DepthONNX/
  DepthONNX.aex
  models/<pack>/
    manifest.json
    *.onnx
```

## セットアップ

### プラグイン

```bash
# macOS
cd crates/depth-onnx-ae && just release-bundle
./scripts/install_dev.sh
```

```bat
REM Windows（管理者 cmd）
cd crates\depth-onnx-ae
just release
scripts\install_dev.bat
```

ビルド前提: Rust, `just`, AE SDK（`AESDK_ROOT`）, ONNX Runtime（`third_party/`）。詳細は [`model/README.md`](model/README.md)。

### モデル

リポジトリで ONNX を持っている場合（macOS）:

```bash
./scripts/setup_models.sh
```

ない場合は export:

```bash
pip install -r scripts/requirements.txt
python scripts/export_onnx.py --static-shape --size 518 --out model/depth_anything_v2_small/depth_anything_v2_small.onnx
python scripts/export_onnx.py --static-shape --size 266 --out model/depth_anything_v2_small/depth_anything_v2_small_266.onnx
python scripts/export_onnx.py --static-shape --size 392 --out model/depth_anything_v2_small/depth_anything_v2_small_392.onnx
```

生成したパックを上記 `models/` に置き、AE を再起動。  
エフェクト: **Effects → Depth → Depth ONNX**

## 補足

| 項目 | 値 |
|------|-----|
| Match name | `ANTH DepthONNX` |
| Resolution | 266 / 392 / 518（manifest の variants） |
| 追加スキャン | 環境変数 `AE_DEPTH_ONNX_MODEL_DIR` |

## ライセンス

- プラグイン: MIT
- Depth Anything V2: Apache-2.0（別途取得）
- ONNX Runtime / AE SDK: 各ライセンスに従う
