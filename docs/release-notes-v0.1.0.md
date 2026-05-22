# TuiImage v0.1.0 Release Notes

![TuiImage demo](./tuiimage-demo.gif)

## English

Initial public preview release of TuiImage, an After Effects effect for creating ANSI-art-inspired / TUI-style cell renderings from image layers.

### Assets

- `TuiImage.aex`: Windows build.
- `TuiImage-macos-arm64.plugin.zip`: macOS Apple Silicon build.

### Highlights

- Added Windows `.aex` and macOS `.plugin` release artifacts.
- Tested on Adobe After Effects 2025 / Windows.
- Tested on Adobe After Effects 2026 / Apple Silicon Mac.
- Includes procedural ANSI-art-inspired, ASCII-like, block, braille-like, and TUI gradient-style rendering presets.
- Supports adjustable cell size, glyph scale, column/row spacing, foreground/background colors, source color mode, and two-color gradient mode.
- Includes source code, build scripts, MIT License, and bilingual README files.

### Notes

- GPU support is experimental and disabled by default.
- The macOS build is ad-hoc signed. Developer ID signing and notarization are not provided yet.
- This is an early v0.1.0 release, so parameter names, defaults, and rendering behavior may still change.

## 日本語

TuiImage の最初の公開プレビュー版です。After Effects の画像レイヤーから、ANSI art / TUI 風のセル表現を作るエフェクトです。

### 同梱ファイル

- `TuiImage.aex`: Windows 版ビルドです。
- `TuiImage-macos-arm64.plugin.zip`: macOS Apple Silicon 版ビルドです。

### 主な内容

- Windows 用 `.aex` と macOS 用 `.plugin` のリリースアーティファクトを追加しました。
- Adobe After Effects 2025 / Windows で動作確認しました。
- Adobe After Effects 2026 / Apple Silicon Mac で動作確認しました。
- ANSI art / TUI 風、ASCII 風、ブロック風、点字風、TUI グラデーション風のプリセットを含みます。
- セルサイズ、文字スケール、列/行の間隔、前景色/背景色、ソースカラー、2色グラデーションを調整できます。
- ソースコード、ビルドスクリプト、MIT License、日本語/英語 README を同梱しています。

### 注意点

- GPU 対応は実験的で、デフォルトでは無効です。
- macOS 版は ad-hoc 署名です。Developer ID 署名や notarization はまだ行っていません。
- v0.1.0 の初期リリースなので、今後パラメータ名、初期値、描画挙動が変わる可能性があります。
