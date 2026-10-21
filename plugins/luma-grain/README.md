# LumaGrain

入力画像の輝度に応じて強さを変えられる、軽量なAdobe After Effects用グレインエフェクトです。

## 主な機能

- Shadows / Midtones / Highlightsごとのグレイン量
- Amount、Size、Seed
- モノクロ／カラーノイズ
- 時間に応じて変化する決定論的ノイズ
- CPU Smart Render

現在は8 bpc専用の実験版です。未実装の16/32 bpcやGPU対応は宣言していません。

半透明入力ではストレート輝度からグレイン量を求め、アルファに応じてRGBへの加算量を調整します。Amountが0なら入力をそのまま返し、それ以外では完全透明画素を透明黒にします。不透明画像の計算結果は維持しています。

画像アクセス前にARGB8形式・寸法・行幅・ポインタ・サイズ計算を検査します。空画像はアクセスせずに処理を終え、入出力サイズ不一致はエラーとして返します。負の行幅でも論理上の座標でノイズを計算します。単体テストで行末余白・逆順・不足バッファを検証していますが、実AEへの障害注入や部分領域描画への対応を保証するものではありません。

## ビルド

Smart Renderは描画成功・失敗・Rustのパニック時も取得済み入力の返却を試みます。描画と返却の両方が失敗した場合は元の描画エラーを返します。6条件の単体テストで確認しており、実AEへの障害注入試験ではありません。

`AESDK_ROOT` を設定して `cargo build --release` を実行します。MIT License。

---

LumaGrain is an experimental lightweight, luminance-weighted grain effect for Adobe After Effects. It currently supports 8 bpc CPU rendering.

For premultiplied input, luminance is evaluated in straight color and noise is alpha-scaled. Amount zero preserves input bytes; active processing clears fully transparent RGB. Opaque rendering retains its existing result.

Smart Render attempts input checkin after success, render error and Rust panic. When rendering and checkin both fail, the primary render error is preserved. Six helper fault-path conditions are tested; native AE failure injection is not covered.

World access validates ARGB8 format, dimensions, stride, pointer and checked sizes before obtaining slices. Empty images return without access; mismatched input/output sizes are errors. Negative strides preserve logical noise coordinates. Unit tests cover padding, reversed rows and truncated buffers; this is not native AE fault injection or ROI certification.
