# Depth ONNX

[日本語](./README.ja.md) | [English](./README.md)

![Depth ONNX UI](./docs/depth-onnx-ui.png)

Depth ONNX は、ONNX Runtime を用いて単眼深度マップを推論する Adobe After Effects 用 SmartFX プラグインです。

Depth Anything などの ONNX モデルは同梱しません。`manifest.json` と `.onnx` を含むモデルパックを所定の `models/` フォルダに置いて使用します。リポジトリには [Depth Anything V2 Small](https://huggingface.co/depth-anything/Depth-Anything-V2-Small-hf) 向けの設定例（`model/depth_anything_v2_small/manifest.json`）が入っています。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

## Name

- 表示名: `Depth ONNX`
- After Effects match name: `ANTH DepthONNX`
- プラグインファイル名:
  - Windows: `DepthONNX.aex`
  - macOS: `DepthONNX.plugin`

## Main Features

- ONNX Runtime による単眼深度推論（Windows: DirectML、macOS: CPU）
- `manifest.json` ベースの汎用モデルパック（Depth Anything 専用ではない）
- 解像度プリセット 266 / 392 / 518（モデルパックの variants に依存）
- フレームごとの正規化、反転、時間方向スムージング
- Browse Model Folder による任意フォルダのスキャン
- After Effects Smart Render 対応

## Validation Status

- Adobe After Effects 2025 / 2026 / Windows で動作確認済み
- Depth Anything V2 Small（ONNX export）でエンドツーエンド確認済み
- macOS ビルド手順はリポジトリ上で用意（実機検証は未記載）
- 自動テストはまだありません

## Build

### Windows

Visual Studio 2022（MSVC）、Rust、`just`（任意）、ONNX Runtime GPU パッケージ（`third_party/onnxruntime-win-x64-gpu-*`）が必要です。

`AdobePlugin.just` が存在しない `sdk/AfterEffectsSDK` を `AESDK_ROOT` に設定するため、`just release` は環境によって失敗します。次の手順が確実です。

```powershell
cd crates\depth-onnx-ae
Remove-Item Env:AESDK_ROOT -ErrorAction SilentlyContinue
$env:CARGO_TARGET_DIR = "..\..\target"
cargo build --release
Copy-Item -Force ..\..\target\release\depthonnxae.dll ..\..\target\release\DepthONNX.aex
Copy-Item -Force ..\..\third_party\onnxruntime-win-x64-gpu-1.24.4\lib\*.dll ..\..\target\release\
```

出力:

```text
target\release\DepthONNX.aex
target\release\onnxruntime.dll
target\release\onnxruntime_providers_shared.dll
```

AE SDK で bindgen ビルドする場合は、ビルド前に `AESDK_ROOT` を実在パスに設定してください。

### macOS

```bash
cd crates/depth-onnx-ae
export CARGO_TARGET_DIR=../../target
# export AESDK_ROOT=/path/to/AfterEffectsSDK  # 未設定時は同梱 bindings を使用
just release-bundle
```

`third_party/onnxruntime-osx-arm64/lib/libonnxruntime.1.24.4.dylib` を配置してから `release-bundle` を実行してください（`post-bundle` が Frameworks にコピーします）。

## Installation

### Windows（MediaCore）

After Effects を閉じてから、管理者権限の PowerShell で以下を実行します。

```powershell
$dst = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
$repo = (Get-Location).Path

Copy-Item -Force "$repo\target\release\DepthONNX.aex" $dst
Copy-Item -Force "$repo\target\release\onnxruntime.dll" $dst
Copy-Item -Force "$repo\target\release\onnxruntime_providers_shared.dll" $dst

$models = "$dst\models\depth_anything_v2_small"
New-Item -ItemType Directory -Force -Path $models | Out-Null
Copy-Item -Force "$repo\model\depth_anything_v2_small\*" $models
```

コピー先:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
  DepthONNX.aex
  onnxruntime.dll
  onnxruntime_providers_shared.dll
  models\<pack>\
    manifest.json
    *.onnx
```

After Effects を再起動し、`Effects > Depth > Depth ONNX` からエフェクトを適用します。

補助スクリプト: [`scripts/install_dev.bat`](scripts/install_dev.bat)（`Effects\DepthONNX\` 向け。MediaCore 直置きの場合は上記手動コピーを推奨）。

### macOS

```bash
./scripts/install_dev.sh
```

コピー先:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
  DepthONNX.plugin
  DepthONNX/models/<pack>/
    manifest.json
    *.onnx
```

生成された `.aex` / `.plugin` / `.dll` などのビルド成果物は Git にコミットしません。

## Models

ONNX ファイルはリポジトリに含まれません（`.gitignore`）。初回は export スクリプトで生成します。

```bash
pip install -r scripts/requirements.txt
pip install onnxscript

python scripts/export_onnx.py --static-shape --size 518 --out model/depth_anything_v2_small/depth_anything_v2_small.onnx
python scripts/export_onnx.py --static-shape --size 266 --out model/depth_anything_v2_small/depth_anything_v2_small_266.onnx
python scripts/export_onnx.py --static-shape --size 392 --out model/depth_anything_v2_small/depth_anything_v2_small_392.onnx
```

重みは [Hugging Face: Depth-Anything-V2-Small-hf](https://huggingface.co/depth-anything/Depth-Anything-V2-Small-hf) から `export_onnx.py` 実行時に自動取得されます（キャッシュ: `~/.cache/huggingface/`）。

詳細は [`model/README.md`](model/README.md)。

## Parameters

- `Model`: スキャンしたモデルパックを選択
- `Browse Model Folder`: 任意のモデルルートを指定（`Choose…`）
- `Resolution`: `266 (preview)` / `392 (preview HQ)` / `518 (final)`
- `Normalization`: `Per-frame` / `Fixed range`
- `Invert`: 深度の白黒反転
- `Temporal Smoothing`: 時間方向の指数移動平均（0.00–0.95）

## Development Checks

```powershell
cargo fmt
cargo check
cargo build --release
```

```bash
# macOS
cd crates/depth-onnx-ae && just release-bundle
```

## Environment Variables

| 変数 | 用途 |
|------|------|
| `AESDK_ROOT` | AE SDK パス（bindgen ビルド時） |
| `CARGO_TARGET_DIR` | ビルド出力先（既定: `target/`） |
| `ORT_DYLIB_PATH` | ONNX Runtime ライブラリの明示パス（開発用） |
| `AE_DEPTH_ONNX_MODEL_DIR` | モデルスキャンルートの追加 |
| `AE_DEPTHANYTHING_MODEL_DIR` | 上記の旧名（互換） |

## Limitations

- モデルパックごとに `manifest.json` の IO 名・前処理が一致している必要がある
- Windows: `onnxruntime.dll` を `.aex` と同じフォルダに配置する必要がある
- macOS: ORT dylib はバンドル `Contents/Frameworks/` に同梱（`release-bundle`）
- 深度推論は CPU/GPU（DirectML）負荷が高く、解像度・プレビュー設定に注意
- 自動テストはまだありません

## License

- プラグイン: MIT License. See [LICENSE](LICENSE).
- Depth Anything V2: Apache-2.0（別途取得）
- ONNX Runtime / AE SDK: 各ライセンスに従う
