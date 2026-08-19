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
