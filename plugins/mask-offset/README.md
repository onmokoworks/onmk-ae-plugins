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

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

MaskOffset is an experimental Adobe After Effects effect plug-in for offsetting mask paths with separate X/Y expansion, corner rounding, fill, feather, and inversion controls.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

