# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion は、マスクまたはシェイプレイヤーからガラス風の屈折、色分散、ハイライト表現を作る Adobe After Effects 用エフェクトプラグインです。

選択したレイヤーから高さ場を作り、疑似法線を計算し、入力または任意の背景レイヤーを RGB / 6ch 分散でサンプリングしてから元画像へ合成します。CPU レンダラーと任意の `wgpu` レンダラーを持っています。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

## 名前

- リポジトリ名: `RefractionDispersion-Ae`
- 表示名: `RefractionDispersion`
- After Effects match name: `RefractionDispersion`
- プラグインファイル名:
  - Windows: `RefractionDispersion.aex`
  - macOS: `RefractionDispersion.plugin`

## 主な機能

- シェイプ/マスクレイヤー駆動の屈折
- 任意の背景レイヤー参照
- RGB Split / Base IOR モード
- RGB または 6ch の色分散
- マスター分散量、チャンネル別 IOR、軸別 chromatic aberration
- Height Source、Coverage Source、Height Blur、Map Blur、Edge Mode
- Fresnel、Diffuse、Specular、Saturation、Brightness、Contrast、Affected Blur、Mix
- Output / Input / Mask Map / Delta Map / Debug 出力
- Smart Render 対応
- 任意の `wgpu` レンダリング経路と CPU フォールバック

## 検証状況

- Windows のリリースビルドスクリプトを開発中に使用しています。
- 開発環境で `wgpu` smoke test が通っています。
- CPU レンダラーを基準実装として扱っています。
- 現在のピクセル経路は ARGB 8-bit buffer です。
- macOS Apple Silicon 向けビルドスクリプトはありますが、署名と notarization は未提供です。

## ビルド

Adobe After Effects SDK と Rust toolchain が必要です。Rust プロジェクトは `rust/` にあります。

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

### リリースビルドを使う場合

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

ダウンロードした `.plugin` が macOS Gatekeeper でブロックされる場合は、必要に応じて quarantine 属性を外します。

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/RefractionDispersion.plugin"
```

After Effects を再起動し、`Distort > RefractionDispersion` から適用します。

### ローカルビルドをインストールする場合

After Effects を終了してから、管理者権限の PowerShell で実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_refractiondispersion_admin.ps1
```

macOS のローカルビルドでは、生成された `rust/target/release/RefractionDispersion.plugin` bundle を MediaCore フォルダへ手動コピーします。

生成された `.aex` / `.plugin` は Git にコミットしません。配布ビルドは GitHub Release の artifacts として添付します。

macOS リリース artifact は Apple Silicon / arm64 向けです。現時点では ad-hoc 署名のみで、Developer ID 署名と notarization は未対応です。

## パラメータ

- `IOR Mode`: RGB Split / Base IOR の切り替え
- `Base IOR`: Base IOR モードで使う屈折率
- `Refract Power`: 屈折サンプリングのオフセット量
- `Samples`: ピクセルあたりのサンプル数
- `Edge Mode`: 画面外サンプリングの扱い
- `Dispersion`: 色分散のマスター量
- `IOR Red`, `IOR Green`, `IOR Blue`: RGB チャンネル別の屈折率
- `Chromatic Aberration`: 共通の色収差量
- `Per-axis Chroma`: X/Y 個別の色収差を有効化
- `Chromatic Aberration X`, `Chromatic Aberration Y`: 軸別の色収差量
- `Use 6ch Dispersion (rygcbv)`: 6ch 分散モード
- `Mask (shape)`: 高さ/マスクとして使うレイヤー
- `Map Blur`: 高さ抽出前のマップブラー
- `Height Source`: 高さ場に使うチャンネル
- `Invert Height`: 高さ場を反転
- `Coverage Source`: 影響範囲に使うチャンネル
- `Height Strength`, `Height Blur`: 疑似法線の強さと滑らかさ
- `Edge Blur`: 出力マスクの柔らかさ
- `Fresnel Power`, `Shininess`, `Diffuseness`: ハイライトとライティング調整
- `Light Angle X`, `Light Angle Y`: ライト方向
- `Saturation`, `Affected Brightness`, `Affected Contrast`: 影響範囲の色調整
- `Affected Box Blur`: 影響範囲内のブラー
- `Background`: 背景としてサンプリングする任意レイヤー
- `Mix with Original`: 元画像とのブレンド
- `Output Mode`: Output / Input / Mask Map / Delta Map / Debug Regions の切り替え
- `Use GPU`: 任意の `wgpu` レンダリング経路を有効化

## 開発チェック

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## 参考・謝辞

初期の表現設計では、Maxime Heckel 氏の記事を参考にしました。

- [Refraction, Dispersion, and Other Shader Light Effects](https://blog.maximeheckel.com/posts/refraction-dispersion-and-other-shader-light-effects/)

## メモ

- CPU レンダラーは `rust/src/refract.rs` にあります。
- `wgpu` レンダラーは `rust/src/gpu.rs` と `rust/shader.wgsl` にあります。
- macOS と GPU 対応の作業は `docs/mac-gpu-roadmap.md` に整理しています。

## ライセンス

ライセンスはまだコミットしていません。公開前に意図したライセンスを追加してください。
