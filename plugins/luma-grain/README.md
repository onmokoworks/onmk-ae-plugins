# LumaGrain

入力画像の輝度に応じて強さを変えられる、軽量なAdobe After Effects用グレインエフェクトです。

## 主な機能

- Shadows / Midtones / Highlightsごとのグレイン量
- Amount、Size、Seed
- モノクロ／カラーノイズ
- 時間に応じて変化する決定論的ノイズ
- CPU Smart Render

現在は8 bpc専用の実験版です。未実装の16/32 bpcやGPU対応は宣言していません。

## ビルド

`AESDK_ROOT` を設定して `cargo build --release` を実行します。MIT License。

---

LumaGrain is an experimental lightweight, luminance-weighted grain effect for Adobe After Effects. It currently supports 8 bpc CPU rendering.
