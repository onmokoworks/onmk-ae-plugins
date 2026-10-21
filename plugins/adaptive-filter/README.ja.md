# AdaptiveFilter

[English](./README.md) | [Japanese](./README.ja.md)

AdaptiveFilter は、MedianPro から分離した非Median系フィルター用の Adobe After Effects エフェクトプラグインです。

名前は仮です。現在は Kuwahara、Generalized Kuwahara、Bilateral を収めています。Luminance Map レイヤーを使うと、明るい部分だけ半径を強くするようなピクセル単位の制御ができます。

> 仕様、UI、パラメータ名、デフォルト値、最終的な名前は今後変更される可能性があります。

## 名前

- 表示名: `AdaptiveFilter`
- After Effects match name: `ONMK_AdaptiveFilter`
- カテゴリ: `Filter`
- プラグインファイル名:
  - Windows: `AdaptiveFilter.aex`
  - macOS: `AdaptiveFilter.plugin`

## 主な機能

- Kuwahara
- Generalized Kuwahara
- Bilateral filtering
- Luminance Map レイヤーによるピクセル単位の半径制御
- Invert Map
- Iterations と Mix
- Smart Render 対応

## 検証状況

ホスト経由の模様付きマップも、現行の検査用DLLで108描画と36比較が通過しました。
7×11のPNGマップから17×31の画素位置と半径をテスト側で計算し、マップなしの半径1～4の
描画から選んだ画素と完全一致します。3フィルター・3深度・両描画経路・反転有無を1反復で
検査しています。フィルター数式の独立検証、ネイティブ深度入力、原点移動やROIの保証ではありません。

模様付きマップの直接テストでは、半径4の切替境界0.375・0.625・0.875の前後に
期待半径を明記し、半径1～4の個別描画から画素を選んだ結果と比較します。
3フィルター・1/2反復・縦横1pxと奇数サイズ・ネイティブ16bitと負値/HDR floatの
36比較が完全一致しました。半径選択と反復時の位置対応の検査であり、比較側も同じ
フィルター本体を使うため、フィルター数式そのものやホストのマップ拡大の独立検証ではありません。

ClassicとSmartのマップ取得エラーを上位へ返すよう修正しました。PreRenderでマップの
指定有無を調べ、その情報をSmart Renderへ渡します。未指定時には画素を要求しないため、
取得失敗を無視して描画を続ける必要がありません。取得失敗時に変換処理を呼ばない回帰テストも
追加しています。実AEのコールバック失敗の注入検査ではありません。

17×31入力に7×11の白黒Luminance Mapを指定し、3種類のフィルター・8/16/32bpc・
Classic/Smart・反転を検査しました。現行の検査用DLLで108描画と18比較が通過しています。
白はマップなしの半径4、黒は半径1の出力と一致し、反転で入れ替わります。
半径1と4の出力が異なることも検査条件です。空間的な補間、ネイティブ深度のマップ入力、
ROI、MFRの保証ではありません。過去の`adaptive-map-bounds`のホスト検査はマップなしの
恒等変換だけであり、実際にマップを指定した検査の証拠には数えません。

- 現在の実用レンダー経路は CPU 実装
- `SmartRenderGpu` は CPU にフォールバック
- CPU経路はARGB8／ARGB16／ARGB32fの精度を維持します（2026-10-04）。3種類のフィルターは従来の色差の尺度を使い、16bit入力を8bitへ落としません。浮動小数点RGBはSDR範囲に制限しません。単体テストで16bitの32769階調、8bit未満の差を持つ値へのフィルター処理、HDR・負値を検査します。ホスト検査のPNG8入力だけではネイティブ入力の全精度を証明できないため、区別して記録しています。マップ指定・ROI・メモリ負荷の検査は別途残っています。
- macOS package script は用意済みですが、このマシンでは未検証

## ビルド

Adobe After Effects SDK と Rust/MSVC toolchain が必要です。Rust プロジェクトは `rust/` にあります。

Windows ではリポジトリルートから実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

出力:

```text
rust\target\release\AdaptiveFilter.aex
```

macOS では Apple Silicon Mac 上で実行します。

```bash
bash ./scripts/build_macos_release.sh
```

出力:

```text
rust/target/release/AdaptiveFilter.plugin
```

## インストール

Windows では After Effects を終了してから `AdaptiveFilter.aex` を以下へコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

macOS では After Effects を終了してから `AdaptiveFilter.plugin` を以下へコピーします。

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

Gatekeeper にブロックされる場合は、必要に応じて quarantine 属性を外します。

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/AdaptiveFilter.plugin"
```

After Effects を再起動し、`Filter > AdaptiveFilter` から適用します。

### ローカル Windows ビルドを入れる

After Effects を終了し、管理者 PowerShell で実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\install_adaptivefilter_admin.ps1
```

生成された `.aex` / `.plugin` は Git にコミットしません。

## パラメータ

- `Filter Type`: Kuwahara、Generalized Kuwahara、Bilateral
- `Radius`: 基本フィルター半径
- `Edge Preserve`: edge-aware 系フィルターの形状/エッジ感度
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

## マップ検証

フィルター処理前に画像・マップ長、寸法、半径・反復回数を検査し、作業配列の確保失敗を
Classic／Smartへ返します。最後のMixは反復用の余り配列を再利用し、画像1枚分の
追加確保を除去しました。16bitで3フィルター×反復1～3回の旧処理との一致を検査しています。
フィルターの計算は変えていません。総メモリ上限や実メモリ枯渇の検証ではありません。

マップ拡大縮小は空・不正な入力と出力サイズのオーバーフローを検査し、出力配列の
確保失敗を返します。添字積は広い整数型で計算し、空出力は即座に返します。
白黒マップの補間・反転を単体検査していますが、フィルター全体の
確保処理や実ホストへの障害注入は未完了です。既存のマップ未取得時の代替処理は維持します。
輝度マップ本体も確保失敗を返します。取得済みマップの変換・寸法・確保エラーは、
Classic／Smartとも「マップなし」として隠さず返します。取得そのものが失敗した場合の
任意入力扱いは別の残課題です。Smart側は取得済みの入力・マップだけを、成功・エラー・
パニック時に返却します。マップ有無と返却失敗を組み合わせた模擬テストで順序を確認し、
元の処理エラーを優先します。実AEへのコールバック障害注入ではありません。

## ライセンス

MIT License. See [LICENSE](LICENSE).
