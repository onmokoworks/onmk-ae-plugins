# MaskOffset

After Effectsのマスクパスを内側・外側へオフセットし、塗りやフェザーを生成するエフェクトプラグインです。

> 現在は実験的な開発版です。UIやパラメータ、描画結果は今後変更される可能性があります。

## 主な機能

- 対象マスクのインデックス指定
- X/Y拡張量の個別指定
- 角の丸め
- 塗り色
- フェザー
- 反転

## ビルド

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `MaskOffset.aex` として配置します。ビルド生成物はGitには含めません。

マスク読取りでは開いたパスの終端頂点を含め、閉じたパスの重複終端を除外します。頂点・開閉状態・セグメント数の取得に失敗した場合、不完全な頂点列で描画せず既存のマスクなし処理に戻します。単体テストで読取り規則を検証しています。AEXCompatの基本描画は実マスク統合の検証ではありません。

Smart Renderでは出力取得失敗・描画エラー・Rustのパニック時も取得済み入力を返却します。描画と返却の両方が失敗した場合は描画エラーを優先し、呼出し元へ通知します。単体テストはエラー経路の検証であり、AEの障害注入試験ではありません。

Mask Alphaと内側・外側表示のマスク適用では、プリマルチプライ済みRGBとアルファを同じ割合で下げます。画素演算の単体テストは8bit・16bit・浮動小数点を検証していますが、プラグインの対応宣言は現在8bitのみです。

画像バッファの読書き前に画素形式・寸法・行幅を検査し、負の行幅に対応します。入力と出力のサイズ不一致はエラーとして返します。これは部分領域描画への対応を意味しません。

## ライセンス

拡張は重心からの放射移動ではなく、輪郭の辺からの距離で計算します。17×31の矩形マスクをAEXCompatで描画し、2px拡張で上下左右が2px広がることと、拡張0の出力が変わらないことを画素比較で確認しています。複雑な凹形状の分裂・自己交差の解消や、任意の曲線・アニメーションの正しさは未検証です。

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

MaskOffset is an experimental Adobe After Effects effect plug-in for offsetting mask paths with separate X/Y expansion, corner rounding, fill, feather, and inversion controls.

Outline extraction includes open-path terminal vertices and excludes duplicated closed endpoints. Failed metadata/vertex reads discard the entire outline rather than rendering a partial shape. In addition to extraction unit tests, the existing headless `--render-experimental-smart-request` route accepts a vector mask through `host_context.mask_scene`: a closed rectangle on a 17x31 input retained exactly its 144 interior pixels and cleared all exterior RGBA channels. This verifies one real host-mask render, not arbitrary curves, animated masks, ROI or Mask Transform's Apply event.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

Smart Render checks acquired input pixels back in on output-checkout failure, render errors and Rust panics. Render errors take precedence over checkin errors and are propagated to the caller. Unit fault-path tests are not native AE fault injection.

Mask Alpha and inside/outside visibility scale premultiplied RGB together with alpha. Pixel-helper tests cover 8-bit, native 16-bit and float HDR values; the plugin currently advertises only 8-bit support.

World access validates explicit pixel formats, dimensions and stride, and honors reversed rows. Mismatched input/output sizes return an error instead of silently copying a partial image; this is not ROI support.

Expansion uses edge offsets instead of radial displacement from the centroid. A headless 17x31 rectangle render with 2px expansion exactly matches the expected 264-pixel interior; zero expansion stays unchanged. Unit regressions cover both winding orders, contraction, separate X/Y, a rotated diamond and duplicate vertices. Concave topology changes/self-intersection cleanup and arbitrary curved or animated masks remain unverified.
