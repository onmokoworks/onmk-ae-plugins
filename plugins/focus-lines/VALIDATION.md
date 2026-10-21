# v0.1.2 検証記録

2026-10-21。Windows x64 / RTX 4070 Ti / Vulkan / NVIDIA 610.88。

- GPU必須で40テスト成功（unit12、alpha6、GPU5、oracle17）。独立面積・点サンプリング・straight alpha、意図的なpremultiply誤り、端の欠け回帰、短い線の範囲・比率・時間を検証。
- releaseビルド、生成PiPLとリンク後リソースの一致確認成功。
- v0.1.2 AEXCompat Smart32でShort Linesを指定しCPU/GPU各1レンダー成功。最大差0.00002528。[証拠](validation/host-short-v012.json)。
- v0.1.0時点ではclassic/smartの8/16/32bpc、Over、縮小を検査。これは現行バイナリの全経路再検査ではない。
- [GPU描画GIF](docs/presets/short-lines.gif)は72フレーム、24fps、3秒。4時点の見た目を確認。
- Windows配布AEX SHA256: `8BB7F71278238ED9A67B04C2024D54A42C71D9F5C1BFC2C320AC06DC64F9E625`。MediaCore配置後のハッシュ一致確認済み。
- AE実機での読み込み・描画は未検証。macOSも未検証。バイナリはGit管理に含めない。

移行時のソース識別: [source-sha256.json](validation/source-sha256.json)。
