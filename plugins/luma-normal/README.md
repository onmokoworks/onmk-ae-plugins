# LumaNormal

入力画像の輝度勾配からタンジェントスペース風のRGBノーマルマップを生成する、Adobe After Effects用エフェクトです。

## 主な機能

- Strength、Pre-Blur、Detail Scale
- X／Y反転
- 不透明または元画像アルファ
- CPU Smart Render

出力は `R=X / G=Y / B=Z` です。現在は8 bpc専用の実験版です。

## ビルド

`AESDK_ROOT` を設定して `cargo build --release` を実行します。MIT License。

---

LumaNormal is an experimental Adobe After Effects effect that converts luminance gradients into RGB normal maps. It currently supports 8 bpc CPU rendering.
