# RefractionDispersion

[English](./README.md) | [日本語](./README.ja.md)

RefractionDispersion は、マスクまたはシェイプレイヤーからガラス風の屈折、色分散、ハイライト表現を作る Adobe After Effects 用エフェクトプラグインです。

選択したレイヤーから高さ場を作り、疑似法線を計算し、入力または任意の背景レイヤーを RGB / 6ch 分散でサンプリングしてから元画像へ合成します。CPU レンダラーと任意の `wgpu` レンダラーを持っています。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

AEXCompatで補助レイヤーを指定し、マスクのアルファ縞と別の一様色背景が正しく使われることを、
8/16/32bpc・Classic/Smartの12条件で確認しています。背景の比較では拡散照明と屈折を無効にしています。
同サイズ・原点0・PNG8由来の入力での検査であり、ネイティブ下位ビット、変形・位置ずれ、部分ROIは別の検査対象です。

さらに17×31の入力に対して7×11・23×37の補助画像を使う24条件を追加しました。
空間パターンのあるアルファマスクとRGB背景が、拡大縮小されず左上配置・ゼロ埋め・切り取りされることを
全3深度・Classic/Smartで確認しています。原点は0であり、位置移動・変形・部分ROIは含みません。

配置関数は別途、正負・画面外を含む49オフセットを16bitと負値HDRの2種類で検査し、
出力側から求める参照計算と98条件で完全一致しました。現行AEXCompatの宣言式フィクスチャには
補助画像の原点や要求ROIの指定欄がありません。この関数単体の検査で、実ホストの原点・ROI受け渡しや変形を検証済みとは扱いません。

CPUぼかしの半径が画像の幅・高さを超えると、端の画素の加算が不足して暗くなる不具合を修正しました。
小さい画像・細長い画像でも一様な明るさを維持します。画像5形状・半径5種・一様／負値HDRパターンの
50条件を全タップ直接加算の参照計算と比較し、差が0.000005未満であることを確認しています。
CPUのボックスぼかし単体の検査であり、任意のROIやGPUとの一致まで保証するものではありません。

CPUエフェクト全体でも、1×1・1×31・17×1・9×7の一様な画像に、マップ・背景・高さ・境界・適用領域の
ぼかしをsigma 0/2/40で同時に適用する24条件を検査しました。16bit値は完全一致、負値・HDRのfloatは
差0.000005未満です。照明・屈折を中立にしてぼかしの保持を確認したもので、任意の照明・色分散・模様付き画像の保証ではありません。

補助レイヤーの取得・変換・返却エラーを「レイヤーなし」として処理せず、呼び出し元へ返すようにしました。
配置処理では入力の長さと出力サイズを検査し、確保失敗を通知します。極端な配置座標も計算途中で桁あふれしません。
エラーの優先順位、不正なサイズ、ネイティブ精度を保った配置をテストしています。
CPU本体も処理前に画像・補助画像のサイズを検査し、画像・ぼかし・距離・法線・サンプル用配列の
確保失敗をClassic／Smart Renderへ返します。並列ぼかしのエラーも伝え、ぼかしの一時配列は3パスで再利用します。
不正な入力長と容量の桁あふれに加え、ぼかし有効・Glass Edgeの2方式・出力5方式で有限値と再現性を検査しています。
実際のメモリ枯渇やRayon・ドライバー内部の確保、任意の数値パラメータまで保証する検査ではありません。

Smart Renderの入力・補助レイヤーは、通常エラーとRustのpanicによる巻き戻しのどちらでも返却し、
元のエラーを保ちます。テストでは返却が1回だけであることと、補助→入力の返却順序を確認しています。
レイヤー選択の照会エラーも未選択扱いにせず通知します。実AEの障害、プロセス強制終了、
実際のメモリ枯渇を再現した検査ではありません。

## 名前

- リポジトリ: [`onmokoworks/onmk-ae-plugins`](https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/refraction-dispersion)
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

- GPUアップロードの変換前に配列長を検査し、確保失敗を返せるようにしました。
  入力のみの出力モードも同様です。入力・マスク・背景の変換用配列を3枚同時に
  保持せず、1枚ずつ転送して解放します。転送キュー内部のメモリは別であり、
  プロセスメモリやVRAM全体の削減を実測した結果ではありません。
- GPU読み戻しは中間の整数配列を作らず、確保失敗を返せる出力配列へ直接変換します。
  一時配列を8bitで4バイト/画素、16/32bitで16バイト/画素削減します。マップ済みの
  ビューを解放した後、途中終了やパニック時もガードがマッピングを解除します。
  16bit全階調・負値・HDR・読み戻し長の不整合を検査します。速度や実メモリ量の
  実測結果、全確保箇所やデバイス喪失の保証ではなく、CPU描画やドライバー側の確保は別課題です。
- 内部GPUキャッシュは最大8スレッド分、要求バッファ容量の合計1GiBまで保持します。
  新規確保前に古いバッファとサイズ変更前のバッファを解放し、1フレームで上限を
  超える場合は既存のCPU経路へ戻します。小さなテスト用上限と12回のスレッド交代で、
  実バッファサイズと容量計算の一致、負値・HDRの保持、過大要求の事前拒否を確認します。
  ドライバーやパイプラインのメモリは含まず、実VRAM・プロセスメモリの上限保証や
  並行MFRの検証ではありません。
- 同じサイズのままGPU処理器の入力・部分透明マスク・背景・ぼかし・明るさ・Mix・
  出力モード4種を変更し、新しく作った処理器とバイト単位で一致することを検査します。
  元の入力と設定へ戻した結果も初回と完全一致します。内部wgpuの古い状態の混入を
  調べる検査であり、CPUとの一致・AEネイティブGPU・並行MFRの保証ではありません。
- 2026-10-04のヘッドレス検査で、Smart Renderが未指定の補助レイヤーを黒画像として扱う不具合を修正しました。初期値で通常／Smart Renderの出力が一致しました。指定済みの画面外マスクを空マスクとする処理は維持しています。補助レイヤー指定時とROIは別途検証が必要です。
- 16／32bitを8bitとして読み書きする不具合を、AE・CPU・GPUの各経路で修正しました。AEXCompatの33件が通過し、以前失敗した4件も解消しています。既存8bit出力20件は画像ハッシュが一致しました。単体・実GPUテスト9件では下位ビット・HDR・補間・ぼかし・補助画像の配置座標・深度と解像度の切り替えを確認しています。PNG8由来のホスト検査だけでは下位ビット保持の証明にならず、GPU指定だけでは実GPU実行の証明にならないため、別々に検査しています。
- GPU回帰テストでは同一デバイスで16×16、13×7、1×1、16×16とサイズを切り替え、Mix=0の出力が入力と完全一致することも確認します。内部wgpuの検査であり、AEネイティブGPU経路の検証ではありません。

- Windows のリリースビルドスクリプトを開発中に使用しています。
- 開発環境で `wgpu` smoke test が通っています。
- CPU レンダラーを基準実装として扱っています。
- ARGB8／16／32fのネイティブ深度に対応しています。全機能でのCPU/GPU一致、補助レイヤー・ROI、デバイス喪失・メモリ負荷の検証は残っています。
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
