# LumaNormal

入力画像の輝度勾配からタンジェントスペース風のRGBノーマルマップを生成する、Adobe After Effects用エフェクトです。

## 主な機能

- Strength、Pre-Blur、Detail Scale
- X／Y反転
- 不透明または元画像アルファ
- CPU Smart Render

出力は `R=X / G=Y / B=Z` です。現在は8 bpc専用の実験版です。

Source Alphaでは法線RGBを元のアルファで事前乗算して出力します。透明部分にはRGBを残しません。OpaqueのRGB値は変更しません。半透明入力・Strength 0の期待値をClassic／Smart両経路で検証しています。部分領域描画や同一インスタンスのキャッシュ動作を保証する検査ではありません。

## ビルド

Pre-Blurは元の輝度バッファを再利用します。ぼかし0では追加コピーせず、ぼかしありでも全画面のfloat32作業領域を1枚削減します。1080pでは8,294,400バイト分の確保を省きます。ホスト全体のメモリ使用量を測定した値ではありません。

描画前に画素形式・行幅・寸法・ポインターを検査します。空画像では画素に触れず、逆順の行も扱います。入力と出力の寸法不一致はエラーとし、一部だけ描画して成功扱いにしません。部分領域描画への対応ではなく、不正な実ホストバッファの注入試験も未実施です。

`AESDK_ROOT` を設定して `cargo build --release` を実行します。MIT License。

---

LumaNormal is an experimental Adobe After Effects effect that converts luminance gradients into RGB normal maps. It currently supports 8 bpc CPU rendering.

Source Alpha premultiplies encoded normal RGB by the source alpha, clearing RGB in fully transparent pixels. Opaque encoding is unchanged. Partial-alpha, zero-strength output is checked against expected pixels in Classic and Smart paths; this does not certify ROI or same-instance cache behavior.

Rendering validates pixel format, row geometry and data pointer before accessing pixels, skips empty images and handles reversed rows. Mismatched input/output extents return an error rather than partial success. This is not ROI support or actual malformed-host-world fault injection.

Pre-Blur reuses its input luminance allocation, including the zero-radius path. One full float32 frame allocation is removed (8,294,400 bytes at 1080p); this is not a whole-process memory measurement.
