# PrismWarp

PrismWarp は、画像や指定したレンズレイヤーの輝度勾配に沿って、屈折とプリズム状の色分散を生成する Adobe After Effects 用エフェクトプラグインです。

> 現在は実験的な開発版です。UI、パラメータ、描画結果は今後変更される可能性があります。

## 主な機能

一定の横勾配を持つレンズ画像で、分散・ぼかし・しきい値を0にした24条件も検査します。強度±2.5px、方向0/90度、3深度、Classic/Smartについて、鏡像境界とバイリニア補間を含む独立計算と一致します。許容差は整数出力で1コード、floatで2×10⁻⁶です。分散や任意勾配まで保証する検査ではありません。

同サイズの白黒チェック柄マットについても、反転ON/OFF×3深度×2描画経路の12条件を追加しています。白領域はマットなしの変形結果と完全一致し、黒領域は元画像と深度変換の許容誤差内で一致します。一定色検査と合わせて48描画が通過しています。中間グレーの変位量やマットのリサイズ補間を検証したものではありません。

ヘッドレス回帰検査では3×7の一定色レンズ（黒・中間グレー・白）と、黒マット／反転白マットを17×31画像へ適用します。8/16/32bitとClassic/Smartの36描画を検査し、補助なしでは変形する入力で、これらの補助画像を適用すると素通しになることを確認しています。変化する勾配や部分透明マップの検証ではありません。

Smart Renderでは取得した入力・レンズ・マットを成功／エラー／パニック時に返却します。補助画像の変換前に取得状態を記録し、返却失敗があっても残りを返却します。描画エラーを優先し、描画成功時の返却エラーは伝播します。単体検査は制御フローの検証であり、実AEへの障害注入ではありません。

- レンズ画像のエッジ方向に沿った屈折ワープ
- 赤から青までを分離するプリズム状の色分散
- レンズ用の別レイヤー指定
- Lens Blur、Edge Threshold、Edge Softnessによる勾配調整
- Matteレイヤーによる適用範囲の制御
- 8 / 16 / 32 bpc対応
- After Effects Smart Render対応

32 bpcの読み書きはWorld Suiteのピクセル形式から判定します。Deepフラグだけで16 bpcと誤認して出力の半分が未書き込みになる問題を修正しています。奇数サイズの効果量ゼロ出力を8 / 16 / 32 bpcのClassic / Smart Renderで検査しています。

Smart Renderでは、参照レイヤーのチェックアウトが成功しても有効な矩形が空なら未指定として扱います。未指定のLens / Matteを黒い画像として処理して効果が消える問題を防ぎます。

## パラメータ

- `Strength (px)`: 全チャンネル共通の屈折距離。ピクセル単位で指定します。
- `Dispersion (px)`: Strengthを中心に、赤と青が反対方向へ開く距離です。
- `Lens Blur`: 勾配を計算する前にレンズ画像をぼかします。
- `Edge Threshold`: ワープ対象にするエッジ強度の下限です。
- `Edge Softness`: Threshold境界のなじませ幅です。
- `Direction`: レンズ勾配から得たワープ方向を回転します。
- `Quality`: 色分散のサンプル数を Draft / Normal / High から選択します。
- `Lens`: ワープ方向を作る任意のレイヤー。未指定時は入力画像を使用します。
- `Matte`: 効果量を制御する任意のレイヤー。
- `Invert Matte`: Matteを反転します。
- `Mix with Original`: 元画像との合成率です。

## ビルド

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `PrismWarp.aex` としてAfter Effectsのプラグインフォルダへ配置します。ビルド生成物はGitには含めません。

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

An analytic horizontal-ramp lens suite covers24 cases: strength±2.5px, direction0/90 degrees,3 depths and Classic/Smart, with dispersion/blur/threshold disabled. Reflected borders and bilinear sampling agree with independent expected coordinates within1 integer code or2e-6 float. Arbitrary gradients and dispersion are outside this oracle.

A same-size checker matte adds12 cases (inversion on/off,3 depths,2 render paths). White regions exactly match the full-effect reference; black regions match source within native conversion tolerance. All48 auxiliary renders pass. Fractional displacement and matte resize interpolation remain separate requirements.

Headless auxiliary tests use3x7 constant lenses (black/gray/white) and black/inverted-white mattes on17x31 input across8/16/float and Classic/Smart:36 renders. A no-auxiliary reference must actually warp, while the constant lens/zero matte yields passthrough. Spatial gradients and partial-alpha maps are not covered by this fixture.

Smart Render tracks input/lens/matte acquisition before conversion and releases acquired layers on success, error or unwind. All releases are attempted even if one fails; render errors take precedence, while successful rendering propagates cleanup errors. Unit coverage tests control flow, not native AE fault injection.

PrismWarp is an experimental Adobe After Effects effect plug-in that creates lens-driven refraction and prismatic chromatic dispersion. Its normalized gradient direction keeps pixel-based Strength and Dispersion controls stable across lens images with different contrast.

Pixel access uses the explicit World Suite format, avoiding a flag-first 32-to-16 bpc misclassification that left half of each output row unwritten. Headless identity fixtures cover odd-sized 8 / 16 / 32 bpc Classic and Smart Render output.

Smart Render treats an empty auxiliary checkout rectangle as unavailable rather than a real black Lens or Matte image, preserving the no-layer fallback.

Native16 regression checks all32769 AE codes through the actual row codec.
A further32766 cases decode four adjacent-code pixels, sample their half-pixel
bilinear center, and encode back to16bit against an independent exact reference.
This directly tests low-bit input beyond PNG8 expansion. It does not certify
arbitrary displacement, blur/dispersion accumulation or native host input precision.
Production rendering formulas were not changed for these tests.

16bitの全32769値を実際の読み書き関数で検査しています。さらに1段階ずつ異なる
4画素の中央を補間する32766ケースで、読み込み・補間・書き戻しを通した精度を
確認しています。任意の変位やぼかし・色分散の累積、実ホストの入力精度は別の検証対象です。

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.
