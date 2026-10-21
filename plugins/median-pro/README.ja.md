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
- `Radius`: フィルター半径（`0`で無変化、最大`100`）
- `Weight Falloff`: Weighted Median の空間重み。Median選択時は無効
- `Iterations`: フィルターの反復回数。半径と同時に上げると処理負荷が大きくなります
- `Mix with Original`: 元画像とのブレンド量（`0%`で無変化）
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

## マップの検証

空または画素数が不足した輝度マップは、不正な画素を参照せず、既存のマップなし処理に戻します。マップの寸法とメモリ確保を検査します。正常な黒マップは半径0として扱い、反転も維持します。これらは単体テストで検証しています。AEXCompatの全画面テストだけでは、部分領域描画や不正なホストバッファの動作までは保証しません。

Smart Renderでは、出力取得失敗など通常のエラー経路でも取得済みの入力・マップ画素を返却します。エラーの優先順位を単体テストしています。panic・メモリ枯渇や実ホストのコールバック障害注入は未検証です。

最後のMixには反復処理で使い終わったバッファを再利用し、事前乗算も同じバッファ内で行います。計算式を変えず、フィルター処理中のARGB8作業バッファを2枚削減します。1920×1080では16,588,800バイト分です。これは確保領域の削減量であり、ホスト全体のメモリ使用量や上限を測定した値ではありません。

中核処理は画素数を検査し、空画像では近傍処理を行いません。作業メモリの確保失敗をエラーとして返します。ホスト画像の変換では8bit形式・ポインター・行幅を検査し、全行の範囲を確認してから書き込みます。パディングと逆順の行も扱います。主入力や出力の不正はエラーにし、不正な補助マップはマップなし処理に戻します。出力寸法の不一致を拒否するもので、部分領域描画への対応ではありません。不正な実ホストバッファやメモリ枯渇の注入試験、メモリ総量の制限は未実施です。

## ライセンス

MIT License. See [LICENSE](LICENSE).
