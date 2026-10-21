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

パラメータの意味と初期値は[`docs/PARAMETERS.md`](./docs/PARAMETERS.md)を参照してください。開発資料は[`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)と[`docs/BUILD_AND_DEPLOY.md`](./docs/BUILD_AND_DEPLOY.md)にあります。

---

画像キャッシュの内容判定では、8×8点の抜き取りではなくARGB8の全有効画素をハッシュします。抜き取り点以外の編集が反映されない不具合を回帰テストで再現・修正しました。行末のパディングは判定から除外します。複数時刻のスプライトは各時刻の画像を取得・照合し、変更のない画像の加工結果を再利用します。AE上の再描画イベントの検証は別途必要です。

スプライトとエミッタ点のキャッシュは、それぞれ保持データ128 MiB・256件を上限とします。スプライトはミップと確保済み容量も計上します。上限到達時は保存分を解放し、単独で上限を超えるデータは描画には使用しますが保持しません。描画中の参照、作業用メモリ、マップの管理領域はこの上限とは別です。

## English

This Rust workspace contains Particle Kit, an Adobe After Effects particle effect, and its evolving shared engine, node-graph workflow, and the separate Lattice Lab effect.

The shipped effect keeps the display name `Particle Kit` and After Effects match name `ParticleKit`. Its host parameter ABI is append-only to preserve compatibility with existing projects.

The workspace is under active development. See [`docs/PARAMETERS.md`](./docs/PARAMETERS.md) for the parameter reference, [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for product boundaries, and [`docs/BUILD_AND_DEPLOY.md`](./docs/BUILD_AND_DEPLOY.md) for build and deployment instructions.

Source-image cache signatures hash all active ARGB8 pixels, excluding row padding. A regression reproduced stale-cache key reuse when edits fell between the former 8x8 sample points. Multi-time sprites now check out and validate each requested frame before reusing its preprocessing, rather than trusting the current frame as a key for the entire sequence. Direct cache tests cover a changed second frame and restoration; native AE temporal checkout/redraw integration remains unverified.

Sprite and emitter caches each retain at most 128 MiB of accounted payload and 256 entries. Sprite accounting includes mip levels and allocated vector capacities. Full caches are cleared before admission; oversized values remain usable for rendering but are not retained. Active render references, temporary working memory and map/allocator bookkeeping are outside this retained-payload budget.
