# Particle Kit / ParticleLab

Adobe After Effects向けのパーティクルエフェクトと、その発展系を開発するRustワークスペースです。

既存エフェクトの表示名は`Particle Kit`、After Effects Match Nameは`ParticleKit`です。既存プロジェクトとの互換性を維持するため、ホストパラメータのABIは追加のみを原則としています。

## 構成

- ルートクレート — Particle KitのAfter Effectsホストプラグイン
- `crates/particlelab_engine_core/` — AEに依存しないシミュレーション・描画エンジン
- `crates/lattice_lab_plugin/` — 点・線・メッシュ表現を扱う独立エフェクトLattice Lab
- `tools/node-ui-shell/` — バージョン付きグラフドキュメントを編集するNode UIシェル
- `tools/` — ビルド、デプロイ、ABI互換性、AEスモークテスト用ツール
- `docs/` — アーキテクチャ、互換性、ビルド・移行資料

## 開発状況

Particle Kitの従来UIと互換性を保ちながら、AE非依存エンジン、ノードグラフ、Lattice Labを段階的に分離しています。仕様・UI・ノードスキーマは開発中です。

## 基本チェック

```powershell
cargo test --workspace
powershell -ExecutionPolicy Bypass -File .\tools\verify_architecture.ps1
powershell -ExecutionPolicy Bypass -File .\tools\verify_compatibility_contract.ps1
```

詳細は[`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)と[`docs/BUILD_AND_DEPLOY.md`](./docs/BUILD_AND_DEPLOY.md)を参照してください。

---

## English

This Rust workspace contains Particle Kit, an Adobe After Effects particle effect, and its evolving shared engine, node-graph workflow, and the separate Lattice Lab effect.

The shipped effect keeps the display name `Particle Kit` and After Effects match name `ParticleKit`. Its host parameter ABI is append-only to preserve compatibility with existing projects.

The workspace is under active development. See [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for product boundaries and [`docs/BUILD_AND_DEPLOY.md`](./docs/BUILD_AND_DEPLOY.md) for build and deployment instructions.
