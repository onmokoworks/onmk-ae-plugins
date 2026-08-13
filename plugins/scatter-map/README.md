# ScatterMap

画像のピクセルをランダムに散らし、別レイヤーをマップとして散らばり方を制御できるAfter Effectsエフェクトプラグインです。

> 現在は実験的な開発版です。UIやパラメータ、描画結果は今後変更される可能性があります。

## 主な機能

- 散乱量と方向の調整
- ランダムシード
- Scatter Mapレイヤーによる強度制御
- マップ反転
- 端のピクセルのリピート
- 元画像とのミックス

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

ScatterMap is an experimental Adobe After Effects effect plug-in for scattering pixels with optional layer-map control. It provides amount, direction, seed, edge, inversion, and original-mix controls.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

