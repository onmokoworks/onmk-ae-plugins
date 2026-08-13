# onmk AE Plugins

onmkが開発・メンテナンスするAfter Effectsプラグインのモノレポです。

## リポジトリ構成

- `plugins/` — 各After Effectsプラグイン
- `crates/` — 複数のプラグインで共有するRustクレート
- `tools/` — 共通のビルド・検証・開発ツール
- `templates/` — 再利用可能なプラグインテンプレート

実験中・未公開のプロジェクトはこのリポジトリの外で開発し、継続開発または公開の準備が整ったものから順次移管します。

## 現在の状況

現在、リポジトリ構成を整備中です。既存のプラグインは、これまでのGit履歴を維持しながら段階的に移管する予定です。

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

The repository structure is currently being prepared. Existing plugins will be migrated incrementally while preserving their Git history.
