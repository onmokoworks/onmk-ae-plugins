# Shape Plus

After Effects標準の「円」を、円・四角・角丸四角に拡張した生成エフェクトです。

## Parameters

- **Shape** — Circle / Rectangle / Rounded Rectangle
- **Center** — 図形の中心
- **Size X / Size Y** — 幅と高さ
- **Force Circle** — X/Yの小さい方に揃え、必ず真円または正方形にする
- **Roundness** — 角丸半径。短辺の50%でカプセル／真円になる
- **Edge** — Fill / Stroke
- **Stroke Width** — 線幅
- **Mask Expansion** — マスクを外側／内側へ拡張・縮小
- **Feather** — エッジのぼかし
- **Invert** — 図形の内外を反転
- **Color / Opacity** — 描画色と不透明度
- **Composite** — Over / Shape Only / Mask Source / Alpha Matte / Luma Matte
- **Source Alpha Operation** — Replace / Intersect / Add / Subtract / Difference

## Build

```sh
just build
```

現在はCPUレンダリング対応です。8/16/32 bpc、Smart Render、Multi-Frame Renderingに対応します。

## 回帰検査

16bitの実合成関数へ型付き画素を直接渡すテストがあります。アルファ0〜32768の
全値について元画像保持と半透明の白のOver合成を独立した整数計算と比較し、完全一致を
確認しています。さらに半分のマスク強度で5種類のアルファ演算を全値検査します。
PNG入力の拡張値に限らない精度検査ですが、任意の色・マスク強度や実AEのMFR動作を
網羅する保証ではありません。この検査追加では描画式を変更していません。

32 bpcはWorld Suiteの形式を確認し、float用のホスト描画ループを明示的に使用します。汎用ループの深度判定により出力の半分が未書き込みになる問題を修正しています。

`cargo test --release`に加え、モノレポの`tools/quality_smoke.py --profile shape-plus`で奇数・縦長・透明入力、Classic / Smart Renderの一致、各深度の透明度ゼロ出力と3種類の図形生成を検査できます。
