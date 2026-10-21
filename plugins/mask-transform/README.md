# MaskTransform

After Effectsレイヤーのマスクパスを水平・垂直反転、回転するためのエフェクトプラグインです。

> マスク形状そのものを変更する実験的な開発版です。適用前にプロジェクトのバックアップを推奨します。

## 主な機能

描画の素通し処理は、入力と出力の寸法・画素形式・行幅を検査してからコピーします。負の行幅と行末の余白を考慮し、短い出力へ一部だけ書いて成功扱いにしません。行コピーの単体テストは4/8/16バイト画素を扱いますが、対応深度の宣言を拡張したものではありません。

- 対象マスクのインデックス指定
- 水平反転
- 垂直反転
- 回転
- Applyスイッチによる適用

Applyには変更通知を有効にし、適用を試みた後はチェックを戻します。AEXCompatで通知設定の登録を検査していますが、実マスクへの適用やUI上の再クリックは未検証です。

## ビルド

開いたパスでは最後の頂点を含めて重心を計算し、全頂点を変換します。閉じたパスでは重複する終端を数えません。[SDKの頂点・セグメント定義](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_maskoutlinesuite3)に基づく単体テストで検証していますが、実AE上のApply動作は別途確認が必要です。

通常のエラー経路でも開始済みのUndoグループを閉じ、取得済みのAEGPスイートを逆順で返却します。取得済みの静止マスク・キーフレーム値も、変形や書き戻しに失敗しても一度だけ解放します。座標変換と後始末の単体テストはありますが、AEXCompatの描画テストは画素の素通しだけで、Applyによる実マスク・キーフレームの更新を保証しません。実ホストの障害注入やpanic時の後始末は別途検証が必要です。

Adobe After Effects SDKを用意し、環境変数 `AESDK_ROOT` を設定してから実行します。

```powershell
cargo build --release
```

生成されたDLLを `MaskTransform.aex` として配置します。ビルド生成物はGitには含めません。

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

Passthrough validates matching dimensions/formats and safe row layout before copying. Negative strides and row padding are respected; short output is rejected before any partial write. Row-helper tests cover4/8/16-byte pixels, not an expansion of advertised depth support.

Apply is registered as a supervised control and resets after an apply attempt. AEXCompat checks its registered notification flag; actual mask mutation and repeated UI clicks remain unverified.

Open paths include the terminal vertex in centroid calculation and transformation; closed paths omit their duplicate endpoint. Unit-tested against the SDK vertex/segment convention; actual AE Apply integration remains unverified.

MaskTransform is an experimental Adobe After Effects plug-in for flipping and rotating a selected mask path. Back up the project before applying destructive path changes.

Normal error paths close started Undo groups and release acquired AEGP suites in reverse order. Acquired static/keyframe stream values are disposed once even after transform or writeback errors. Unit tests cover transform math and cleanup ordering. AEXCompat render checks cover passthrough only, not Apply/mask/keyframe mutation. Native host fault injection and panic cleanup need separate validation.

Build with `cargo build --release` after setting `AESDK_ROOT`. See [LICENSE](./LICENSE) for licensing details.
