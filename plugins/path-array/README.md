# PathArray

After Effects上で、指定したマスクパスに沿ってレイヤーの複製を配置するエフェクトプラグインです。

> 現在は実験的な開発版です。UIやパラメータ、描画結果は今後変更される可能性があります。

## 主な機能

- マスクパスに沿った複数コピーの配置
- パス方向への自動回転と固定回転
- オフセット、拡張、角の丸め、スケール、透明度
- フレーム単位の時間オフセット
- 2つ目のソースレイヤーの合成

## ビルド

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `PathArray.aex` としてAfter Effectsのプラグインフォルダへ配置します。ビルド生成物はGitには含めません。

マスク読取りでは開いたパスの終端頂点を含め、閉じたパスの重複終端は除外します。開閉状態・頂点数・頂点の取得に失敗した場合、不完全なパスを使いません。単体テストで読取り規則を検証していますが、実マスクを使うAE統合動作は別途検証が必要です。

Smart Renderは取得した画像だけを返却します。補助画像はコピー後すぐに返却し、出力取得失敗・描画エラー・Rustのパニック時も主入力の返却を試みます。描画エラーを握りつぶさず、返却エラーより優先して通知します。単体テストは実AEの障害注入を代替するものではありません。

コピーの合成ではプリマルチプライ済みRGBにアルファを二重に掛けず、指定した不透明度だけを掛けます。半透明素材が不必要に暗くなる問題を単体テストで再現・修正しています。

画像端の補間では隣接位置を画像内に収め、右端・下端や1ピクセル幅の素材が消える問題を修正しています。等倍コピーの全画素一致と端の補間を単体テストで検証しています。

合成関数は画像サイズの乗算とバッファ長を検査し、不正な値や非正の倍率では描画しません。描画範囲は整数化前に出力範囲へ収めます。これらは合成関数の防御策であり、AEから取得する画像バッファ全体の検証は別の課題です。

画像変換・余白追加・合成用フレームの確保ではサイズの積を検査し、確保失敗を描画エラーとして返します。総メモリ使用量の上限管理やAE画像バッファの検査を完了したという意味ではありません。

画像変換は画素形式・寸法・行幅・ポインタを検査してからバッファへアクセスし、負の行幅を扱います。出力サイズと平坦画像の長さが一致しない場合はエラーを返します。内部の8bit変換は残っており、高精度対応や部分領域の座標対応を完了したものではありません。

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

PathArray is an experimental Adobe After Effects effect plug-in that distributes copies of source layers along a mask path. It supports path orientation, transform and opacity controls, time offsets, and an optional second source layer.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

Outline extraction includes open-path terminal vertices and excludes duplicated closed endpoints. Failed metadata or vertex reads discard the whole path. Unit tests verify extraction rules, not actual AE mask integration.

既存のAEXCompatヘッドレス経路で、17×31の配置元画像と開いた直線マスクを与え、1枚を中点に置く場合とパスを移動した場合を検証しました。両方とも期待する平行移動画像との画素差は0でした。曲線・回転・時刻別素材・実AEの再描画は、この検査の対象外です。

The existing headless AEXCompat vector-mask request also verifies actual one-copy placement with an assigned 17x31 source layer. Open straight paths place one copy at the midpoint; two translated paths exactly match independent clipped-translation images. This is narrower than arbitrary curved/rotated paths, temporal sources or native AE redraw certification.

Time Offsetは有効なレイヤー2の取得時刻に適用します。時刻の桁あふれはエラーとして返し、PreRenderの失敗を成功扱いにしません。無効なレイヤー2は取得しません。AEXCompatで境界エラーの伝達を確認していますが、時刻ごとに異なる素材の取得は未検証です。

Time Offset applies to enabled Layer 2. Checked time arithmetic rejects overflow, and PreRender failures propagate instead of being discarded. Disabled Layer 2 is not checked out. AEXCompat verifies overflow rejection versus disabled-layer success; differing-frame temporal content remains unverified.

拡張は重心からの頂点移動ではなく、サンプリングした輪郭の辺からの距離で計算します。閉じたパスは外向き、開いたパスは進行方向の右側を正方向とします。拡張0の配置は従来どおりです。矩形の2px拡張を実レンダーで検証しています。大きな収縮・凹形状の分裂・自己交差の解消は保証しません。

Expansion offsets sampled contour edges and recomputes spacing along the expanded contour. Positive offsets go outward for closed paths and to the right of vertex order for open paths. Zero-expansion sampling remains unchanged. Unit tests cover winding, separate X/Y, open endpoints and spacing; a one-pixel host render verifies the rectangle's 2px displacement. This is not a topology solver for large contractions, concave splits or self-intersection cleanup.

Smart Render checks in only acquired images, releasing auxiliary images immediately after copying. Output-checkout failures, render errors and Rust panics still attempt primary input cleanup. Render errors are propagated with precedence over cleanup errors. Unit fault tests are not native AE fault injection.

Compositing applies user opacity to premultiplied source RGB without multiplying source alpha a second time. Unit tests cover translucent source-over on transparent and translucent destinations.

Bilinear neighbors clamp to image edges, preserving the last row/column and single-pixel axes. Unit tests verify exact identity copies and border/interior interpolation.

The compositor checks image-size products and buffer lengths, skips non-finite transforms/non-positive scales, and clips bounds before integer conversion. These core guards do not certify host-world buffer access.

Conversion, padded canvases and compositing frames use checked size products and fallible reservations. Failures propagate through render cleanup; this does not establish an aggregate memory budget or validate host worlds.

World conversion validates explicit formats, dimensions, stride and nonnull data before buffer access, honors reversed rows and rejects output-size/flat-length mismatches. Internal 8-bit conversion remains; native precision and ROI coordinate correctness are not certified.
