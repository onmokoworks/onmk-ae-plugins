# PrismWarp

PrismWarp は、画像や指定したレンズレイヤーの輝度勾配に沿って、屈折とプリズム状の色分散を生成する Adobe After Effects 用エフェクトプラグインです。

> 現在は実験的な開発版です。UI、パラメータ、描画結果は今後変更される可能性があります。

## 主な機能

- レンズ画像のエッジ方向に沿った屈折ワープ
- 赤から青までを分離するプリズム状の色分散
- レンズ用の別レイヤー指定
- Lens Blur、Edge Threshold、Edge Softnessによる勾配調整
- Matteレイヤーによる適用範囲の制御
- 8 / 16 / 32 bpc対応
- After Effects Smart Render対応

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

PrismWarp is an experimental Adobe After Effects effect plug-in that creates lens-driven refraction and prismatic chromatic dispersion. Its normalized gradient direction keeps pixel-based Strength and Dispersion controls stable across lens images with different contrast.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

