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

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

PathArray is an experimental Adobe After Effects effect plug-in that distributes copies of source layers along a mask path. It supports path orientation, transform and opacity controls, time offsets, and an optional second source layer.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.

