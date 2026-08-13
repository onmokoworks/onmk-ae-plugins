# Agent guide — UVProject

このリポジトリの方針・レイアウト・ビルド手順・規約は **[`CLAUDE.md`](./CLAUDE.md)** に
集約しています。Codex / 他のコーディングエージェントもまずそちらを参照してください。

要点だけ:

- 公開仕様は `README.md` / `README.ja.md`（バイリンガル）。変更時は両方更新
- PiPL（プラグイン名・カテゴリ・Support URL）の真実は `build.rs`
- 実装は CPU のみ（8-bit ARGB）。wgpu には依存しない
- 作業ログ・調査メモなどの内部ドキュメントは `_internal/`（Git 管理外）に書く
- コミットしないもの: `*.aex` `*.dll` `target/` `_internal/`

ビルド:

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
cargo build --release
```
