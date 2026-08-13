# MedianPro

[English](./README.md) | [Japanese](./README.ja.md)

MedianPro は、中央値系フィルターに絞った Adobe After Effects エフェクトプラグインです。

現在は Median と Weighted Median を提供します。Kuwahara / Bilateral などの別系統フィルターは、別プラグイン側へ分離しています。

> 仕様、UI、パラメータ名、デフォルト値は今後変更される可能性があります。

## 名前

- 表示名: `MedianPro`
- After Effects match name: `ONMK_MedianPro`
- カテゴリ: `Filter`
- プラグインファイル名:
  - Windows: `MedianPro.aex`
  - macOS: `MedianPro.plugin`

## 主な機能

- Median
- Weighted Median
- Luminance Map レイヤーによるピクセル単位の半径制御
- Invert Map
- Iterations と Mix
- Smart Render 対応

## 検証状況

- 現在の実用レンダー経路は CPU 実装
- `SmartRenderGpu` は CPU にフォールバック
- 現在のピクセル経路は ARGB 8-bit buffer
- macOS package script は用意済みですが、このマシンでは未検証

## ビルド

Adobe After Effects SDK と Rust/MSVC toolchain が必要です。Rust プロジェクトは `rust/` にあります。

Windows ではリポジトリルートから実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

出力:

```text
rust\target\release\MedianPro.aex
```

macOS では Apple Silicon Mac 上で実行します。

```bash
bash ./scripts/build_macos_release.sh
```

出力:

```text
rust/target/release/MedianPro.plugin
```

## インストール

Windows では After Effects を終了してから `MedianPro.aex` を以下へコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

macOS では After Effects を終了してから `MedianPro.plugin` を以下へコピーします。

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

Gatekeeper にブロックされる場合は、必要に応じて quarantine 属性を外します。

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/MedianPro.plugin"
```

After Effects を再起動し、`Filter > MedianPro` から適用します。

### ローカル Windows ビルドを入れる

After Effects を終了し、管理者 PowerShell で実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_medianpro_admin.ps1
```

生成された `.aex` / `.plugin` は Git にコミットしません。

## パラメータ

- `Filter Type`: Median または Weighted Median
- `Radius`: 基本フィルター半径
- `Edge Preserve`: Weighted Median の空間重み強度
- `Iterations`: フィルターの反復回数
- `Mix with Original`: 元画像とのブレンド量
- `Luminance Map`: 半径をピクセル単位で制御する任意レイヤー
- `Invert Map`: 輝度マップの影響を反転

## 開発チェック

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

## ライセンス

MIT License. See [LICENSE](LICENSE).
