# onmk AE Plugins

onmkが開発・メンテナンスするAfter Effectsプラグインのモノレポです。

## リポジトリ構成

- `plugins/` — 各After Effectsプラグイン
- `crates/` — 複数のプラグインで共有するRustクレート
- `tools/` — 共通のビルド・検証・開発ツール
- `templates/` — 再利用可能なプラグインテンプレート

実験中・未公開のプロジェクトはこのリポジトリの外で開発し、継続開発または公開の準備が整ったものから順次移管します。

## 現在の状況

既存のプラグインを、これまでのGit履歴を維持しながら段階的に移管しています。

## プラグイン

- [Rio de Janeiro Filter](./plugins/rio-de-janeiro-filter/) — Rio de Janeiroフィルター風の色調を再現するエフェクト
- [RefractionDispersion](./plugins/refraction-dispersion/) — ガラス風の屈折、色分散、ハイライト表現を作るエフェクト
- [FrameSlice](./plugins/frame-slice/) — 画像内にスリットスキャン風の時間差を作るエフェクト
- [TuiImage](./plugins/tui-image/) — 画像をANSIアート／TUI風のセル表現に変換するエフェクト
- [Depth ONNX](./plugins/depth-onnx/) — ONNX Runtimeによる単眼深度推定エフェクト
- [MinimaxMap](./plugins/minimax-map/) — 符号付きのMinimum／Maximumモルフォロジーを行うエフェクト

---

## English

This monorepo contains After Effects plugins developed and maintained by onmk.

### Repository layout

- `plugins/` — Individual After Effects plugins
- `crates/` — Rust crates shared across multiple plugins
- `tools/` — Shared build, verification, and development tools
- `templates/` — Reusable plugin templates

Experimental and unpublished projects are developed outside this repository and moved here once they are ready for continued development or publication.

### Status

Existing plugins are being migrated incrementally while preserving their Git history.

### Plugins

- [Rio de Janeiro Filter](./plugins/rio-de-janeiro-filter/) — Recreates a look inspired by the Rio de Janeiro filter
- [RefractionDispersion](./plugins/refraction-dispersion/) — Creates glass-like refraction, chromatic dispersion, and highlight shaping
- [FrameSlice](./plugins/frame-slice/) — Creates slit-scan style time offsets inside image layers
- [TuiImage](./plugins/tui-image/) — Converts images into ANSI art / TUI-style cell renderings
- [Depth ONNX](./plugins/depth-onnx/) — Performs monocular depth estimation with ONNX Runtime
- [MinimaxMap](./plugins/minimax-map/) — Performs signed Minimum / Maximum morphology
