# ScatterMap

画像のピクセルをランダムに散らし、別レイヤーをマップとして散らばり方を制御できるAfter Effectsエフェクトプラグインです。

> 現在は実験的な開発版です。UIやパラメータ、描画結果は今後変更される可能性があります。

## 主な機能

散乱処理本体も入力・マップの長さとサイズ計算を検査し、出力の確保失敗をエラーで返します。座標加算は64bitで行います。補助マップ変換は同サイズなら中間マップを再利用します。実際のメモリ枯渇を起こす試験は行っていません。

画像アクセス前に寸法・行幅・符号付き長さの上限を検査します。行幅の最小整数値や過大なバッファ長はエラーにします。これらは単体テストで検証していますが、実AEへの不正バッファ注入試験ではありません。

- 散乱量と方向の調整
- ランダムシード
- Scatter Mapレイヤーによる強度制御
- マップ反転
- 端のピクセルのリピート
- 元画像とのミックス
- 8 / 16 / 32 bpcのネイティブ精度で処理（floatのHDR値・負値を保持）

## 回帰検査

16bitネイティブ入力のミックスを直接検査しています。全32769コードに対し6種類の相手値と0/25/50/75/100%のミックスを整数演算の期待値と比較します。17×31の部分透明入力でも、方向3種・端リピートON/OFF・5段階のミックスで下位桁と透明な画面外サンプルとの合成を検査します。サンプル位置は別の既存検査が担当します。これはPNG経由のホスト検査ではなく、任意のミックス率や実AEの入力精度を証明するものではありません。

上記の座標検査にミックス率37%の36条件を追加し、計78レンダリングを検証しています。元画像と散乱画像のARGB合成を別途計算し、整数深度の切り捨てとfloat出力を含めて完全一致を確認します。端リピートOFFでは透明なサンプルとの合成も含みます。入力自体が部分透明な条件は含みません。

中間グレー64/192の3×7マップを17×31へ拡大する回帰検査を追加しています。8/16/32bitとClassic/Smartの各組み合わせで、方向3種と「端リピートOFF・反転OFF／両方ON」を検査し、独立計算した散乱座標と全ピクセル一致を確認します。42レンダリング中36件が座標比較、6件が元画像参照です。部分透明マップや同一インスタンスの変更検査は別課題です。

`cargo test --release`で16bitの下位桁、float値、深度間のサンプリング位置の一致を検査します。リポジトリの`tools/quality_smoke.py --profile scatter-map`では、黒・透明・奇数サイズ、再現性、Classic / Smart Renderの一致、各深度での効果量ゼロ出力を検査します。8bit固定の読み書きによって16 / 32 bpc出力が欠ける問題を修正しています。

## ビルド

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `ScatterMap.aex` として配置します。ビルド生成物はGitには含めません。

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

Direct native16 mix tests compare all32769 input codes, six partner values and quarter-step mix weights against integer truncation references. A17x31 partial-alpha input also exercises three directions, both edge modes and five weights, including transparent out-of-bounds samples. Sampling positions are covered separately. These are typed-buffer unit tests, not PNG-derived host precision evidence or proof for arbitrary mix weights/native AE input.

The coordinate suite also includes36 cases at37% mix, for78 renders total. Independently calculated ARGB blending matches exactly, including integer truncation and float output. Disabled edge repeat exercises blending with transparent out-of-bounds samples; partially transparent source fixtures are not covered here.

The fractional-map regression resizes a 3x7 gray64/192 map to 17x31 across native8/16/float and Classic/Smart. Three directions and paired edge-repeat/inversion off/off or on/on settings match independently computed sampling coordinates exactly:36 oracle renders plus6 source references. This does not cover partial-alpha maps or same-instance parameter changes.

The scatter core validates source/map lengths and checked sizes, propagates output allocation failures, and adds offsets in 64-bit coordinates. Equal-size auxiliary maps reuse their luminance buffer. Actual allocator exhaustion has not been injected.

Image access checks signed dimension bounds, stride and slice-length limits before obtaining host buffers. Tests cover invalid geometry and arithmetic overflow, not malformed native AE callback injection.

ScatterMap is an experimental Adobe After Effects effect plug-in for scattering pixels with optional layer-map control. It provides amount, direction, seed, edge, inversion, and original-mix controls.

Processing preserves native 8 / 16 / 32 bpc channels, including HDR and negative float values. Unit tests cover codec precision and matching scatter coordinates across depths. Headless regression fixtures cover identity at each depth, odd sizes, black/transparent inputs and Classic/Smart agreement. This fixes incomplete deep/float output caused by the former fixed 4-byte-per-pixel access.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.
