# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion は、シェイプまたはマスクレイヤーからガラス風の屈折と色分散を作る Adobe After Effects エフェクトプラグインです。

マスクの輝度/アルファから高さ場を作り、疑似法線、RGB または 6ch の分散サンプリング、Blinn-Phong 風のハイライトを合成します。実用レンダー経路は Rayon 並列の CPU 実装です。CUDA 経路は配線確認用の実験段階で、現在は passthrough kernel です。

## 名前

- 表示名: `RefractionDispersion`
- After Effects match name: `RefractionDispersion`
- プラグインファイル名:
  - Windows: `RefractionDispersion.aex`
  - macOS: `RefractionDispersion.plugin`

## 主な機能

- シェイプ/マスクレイヤー駆動の屈折
- 任意の背景レイヤー参照
- RGB 別の IOR 制御
- RGB または 6ch 分散モード
- X/Y 個別の色収差制御
- Height Blur、Edge Blur、Fresnel、Diffuse、Specular、Saturation、Mix
- Smart Render 対応

## 検証状態

- CUDA Toolkit 13.2 が入った Windows 環境で `cargo check` 済み
- 現在の実用レンダー経路は CPU/Rayon
- CUDA バックエンドは実験用 passthrough のため、実出力では `Use GPU (CUDA)` を無効にする
- 現在のピクセル経路は ARGB 8-bit buffer
- macOS のパッケージングと公証は未検証

## ビルド

Adobe After Effects SDK と Rust/MSVC toolchain が必要です。Rust プロジェクトは `rust/` にあります。Windows build では `rust/build.rs` が CUDA PTX をコンパイルするため、CUDA Toolkit の `nvcc` も必要です。

Windows ではリポジトリルートから実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

出力:

```text
rust\target\release\RefractionDispersion.aex
```

macOS では Apple Silicon Mac 上で実行します。

```bash
bash ./scripts/build_macos_release.sh
```

出力:

```text
rust/target/release/RefractionDispersion.plugin
```

## インストール

### リリースビルドを使う

Windows では GitHub Releases から `RefractionDispersion.aex` をダウンロードし、After Effects を終了してから plug-ins フォルダへコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

macOS では GitHub Releases から `RefractionDispersion-macos-arm64.plugin.zip` をダウンロードして展開し、After Effects を終了してから `RefractionDispersion.plugin` を MediaCore フォルダへコピーします。

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

コマンドラインでコピーする例:

```bash
unzip RefractionDispersion-macos-arm64.plugin.zip
sudo cp -R RefractionDispersion.plugin "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
```

ダウンロードした `.plugin` が Gatekeeper にブロックされる場合は、必要に応じて quarantine 属性を外します。

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/RefractionDispersion.plugin"
```

After Effects を再起動し、`Distort > RefractionDispersion` から適用します。

### ローカルビルドを入れる

After Effects を終了し、管理者 PowerShell で実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_refractiondispersion_admin.ps1
```

macOS のローカルビルドでは、生成された `rust/target/release/RefractionDispersion.plugin` bundle を MediaCore フォルダに手動コピーします。

生成された `.aex` / `.plugin` は Git にコミットしません。配布ビルドは GitHub Release の artifacts として添付します。

macOS リリース artifact は Apple Silicon / arm64 向けです。現時点では ad-hoc 署名のみで、Developer ID 署名と notarization は未対応です。macOS では CUDA を無効化し、CPU renderer を使います。

## パラメータ

- `IOR Red`, `IOR Green`, `IOR Blue`: チャンネル別の屈折率
- `Refract Power`: 背景サンプリングのずれ量
- `Chromatic Aberration`: 共通の色分散量
- `Per-axis Chroma`: X/Y 個別色収差を有効化
- `Chromatic Aberration X`, `Chromatic Aberration Y`: 軸別の色収差量
- `Samples`: ピクセルあたりのサンプル数
- `Fresnel Power`, `Shininess`, `Diffuseness`: ハイライトとライティング調整
- `Light Angle X`, `Light Angle Y`: ライト方向
- `Saturation`: 屈折サンプリング後の彩度
- `Height Strength`, `Height Blur`: マスク由来の高さと法線の調整
- `Edge Blur`: 合成マスクの柔らかさ
- `Use 6ch Dispersion (rygcbv)`: 6ch 分散モード
- `Mask (shape)`: 高さ/マスクとして使うレイヤー
- `Background`: 背景としてサンプリングする任意レイヤー
- `Mix with Original`: 元画像とのブレンド量
- `Use GPU (CUDA)`: 実験用 passthrough backend。デフォルト無効

## 開発チェック

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## メモ

- `rust/kernels/refract.cu` は CUDA device setup と転送を確認する passthrough kernel です。
- CPU 版の参照実装は `rust/src/refract.rs` です。
- macOS と本格 GPU 化の作業は `docs/mac-gpu-roadmap.md` に整理しています。

## ライセンス

ライセンスはまだコミットしていません。公開前に意図したライセンスを追加してください。
