# MaskTransform

After Effectsレイヤーのマスクパスを水平・垂直反転、回転するためのエフェクトプラグインです。

> マスク形状そのものを変更する実験的な開発版です。適用前にプロジェクトのバックアップを推奨します。

## 主な機能

- 対象マスクのインデックス指定
- 水平反転
- 垂直反転
- 回転
- Applyスイッチによる適用

## ビルド

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `MaskTransform.aex` として配置します。ビルド生成物はGitには含めません。

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

MaskTransform is an experimental Adobe After Effects plug-in for flipping and rotating a selected mask path. Back up the project before applying destructive path changes.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

