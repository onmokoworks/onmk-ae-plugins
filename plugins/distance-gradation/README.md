# onmk DistanceGradation

アルファ輪郭からの距離でグラデーションを生成し、元画像に重ねる After Effects プラグイン。
Windows x64: `dist/ONMK_DistanceGradation.aex`。エフェクト名は **onmk DistanceGradation**、カテゴリは **ONMK**。

## 今回の動作

- **Anti-aliasing: ON（初期値）**。入力の半透明アルファをサブピクセル距離の種として利用し、Inside/Outside の境界にも元の coverage を利用する。OFF は alpha >= 0.5 の二値輪郭。元画像自身のアンチエイリアスは変更しない。
- **Composite Original: ON（初期値）**。グラデーションを元画像へストレートアルファで over 合成する。効果のない部分は元画像を保持し、透明部分からコンポジションの下層が見える。OFF はグラデーションのみ。
- **Use Background Color: OFF（初期値）**。これを ON にすると指定領域を背景色との二色グラデーションで埋める。Both と併用すれば画面全体を覆う設定になるため、下層を見せる用途では OFF を使う。
- **Gradation Opacity (%): 100**。0 なら元画像がそのまま出力される（Composite Original ON）。

## 設定

| 設定 | 内容／初期値 |
|---|---|
| Invert | グラデーションの反転／OFF |
| In/Out | Inside / Outside / Both、初期値 Inside |
| Inside / Outside Threshold | 内側／外側の距離、フル解像度のレイヤーピクセル単位／128 |
| Render Mode | RGB は指定色、Layer はその位置の入力RGB／RGB |
| Gradation Color / BG Color | 赤／黒 |
| Interpolation Mode | Constant / Linear / Sphere / Power、初期値 Linear |
| Power | Power 補間の指数／1 |
| Blur Mode / Blur Size | No Blur / Box / Gaussian、初期値 No Blur・0、最大500 px |
| Anti-aliasing | ON |
| Composite Original | ON |
| Gradation Opacity (%) | 100 |

輪郭の周囲へ出す場合は Both または Outside。Layer モードの透明領域では元画像に有効なRGBがないことがあるため、外側に指定色を出す場合は RGB を使う。
反転を有効にすると輪郭から遠い領域が濃くなる。空の入力には輪郭がないため、反転しても何も生成しない。

## 実装と互換性

参考: [OLM-for-mac の Distance Gradation](https://github.com/onmokoworks/OLM-for-mac/tree/main/mac/OLMDistanceGradation)。UI、距離の正規化、補間の意味を参考にした Rust の独立実装。OLM のバイナリ互換品・完全一致版ではない。OLM の C++ ソースは同梱していない。

距離は separable squared Euclidean distance transform。AA は fractional-alpha seed costs を利用する近似で、画像から失われたベクター輪郭を復元するものではない。入力／出力は straight RGBA、合成の積算時のみ RGB にアルファを掛け、出力時に合成アルファで割り戻す。8 bpc は255、16 bpc は32768、32 bpc はfloat。floatの元画像RGBのHDR／負値を不必要に切り捨てない。

距離は `min(指定距離, 実際の最大距離)` で正規化。原版の深度ごとの丸め・隠れRGB・特殊ケースは再現しない。縮小率はX/Y別に距離計算へ反映。効果は入力レイヤーの矩形内で生成し、レイヤーの外へキャンバスは拡張しない。SmartRenderでは距離の正規化を安定させるため全入力を要求し、出力は要求された部分だけを返す。

CPUのみ。8/16/32 bpc、Render / SmartRender対応。macOSのソースビルド経路はあるが、この成果物はWindows版のみでmacOSビルド未検証。AE実機操作・MediaCoreへの導入はこの作業では実施していない。

## ライセンス

元プロジェクトと同じ **Apache License 2.0** を適用します。[LICENSE](LICENSE) に原文と OLM Digital の著作権表記を保持し、[NOTICE](NOTICE) に出典・参照リビジョン・onmk版の変更点を記載しています。OLM Digital の提携・承認を受けた製品ではありません。

配布時は `.aex` と一緒に `LICENSE` と `NOTICE` を同梱してください。`build.ps1` はこれらを `dist/` へコピーします。この指定は `plugins/distance-gradation/` に適用し、モノレポ内の別プラグインや依存ライブラリのライセンスを変更しません。

## ビルドと検証

```powershell
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings -A clippy::drop_non_drop -A clippy::question_mark
./build.ps1
python verify_pipl.py
python verify_host.py
python verify_resolution.py
```

後ろの3つは Python の pefile / numpy / Pillow、およびローカルAEXCompatを使用。ハーネスパスは検証スクリプト内に明示。常駐プロセスは作成しない。詳しくは [VALIDATION.md](VALIDATION.md)。
