# UVProject — After Effects effect plug-in

**One-line:** UV / ST マップ経由でテクスチャを投影する（STMap リマップ）。

Working dir: `D:\Projects\01_Project\04_Tools\Ae_Plugins\UVProject-Ae\`
（ローカル作業フォルダは `UVProjectRust`）

Sibling reference plug-ins (diff these when in doubt):
- `D:\Projects\01_Project\04_Tools\Ae_Plugins\ScatterMapRust\` — マップ用セカンダリ
  レイヤー入力 + CPU Smart Render の最も近い実例（このプラグインの土台）
- `D:\Projects\01_Project\04_Tools\Ae_Plugins\TimeSliceRust\` — single-input wgpu effect
- `D:\Projects\01_Project\04_Tools\Ae_Plugins\StarglowRust\` — multi-pass

## ローカルで作業を始めるときに参照するもの

1. **このファイル (`CLAUDE.md`)** — プラグイン固有の方針・状態・レイアウト
2. **`README.md` / `README.ja.md`** — 公開仕様（表示名・パラメータ・ビルド手順）。
   実装と README がずれたら README を更新する
3. **`_internal/`** — 個人メモ・調査ログ・スクラッチ（Git 管理外）
4. **`build.rs`** — PiPL 定義（match name・カテゴリ・Support URL）が真実のソース

## Status

CPU 実装・8-bit ARGB。Smart Render パスあり。ビルド未検証（要 cargo build）。

## どう動くか

- エフェクトは **UV マップレイヤー**（入力 layer 0）に適用する。これが出力解像度と
  座標を決める。
- **Texture**（param index 1, LayerDef）を UV 座標でサンプリングして投影する。
- 各出力ピクセル: `u = R/255`, `v = G/255` → V 原点トグルに応じて `v` を反転 →
  テクスチャを bilinear/nearest + wrap でサンプル → プリマルチ済みなので
  カバレッジ（`opacity * (use_uv_alpha ? uv_alpha : 1)`）を全チャンネルに掛ける。
- チャンネル順は AE の `[A, R, G, B]`（`uv.rs` / `lib.rs` の `layer_to_flat` 参照）。

## Layout

```
UVProject-Ae/
├── README.md / README.ja.md   公開ドキュメント（バイリンガル）
├── CLAUDE.md                   このファイル
├── AGENTS.md                   他エージェント向けポインタ
├── Cargo.toml                  after-effects 依存（wgpu なし、CPU のみ）
├── build.rs                    PiPL（Distort / UVProject / Support URL）
├── src/
│   ├── lib.rs                  params / Render / SmartPreRender / SmartRender
│   └── uv.rs                   UV リマップのコア（project / wrap / sample）
├── docs/                       公開アセット（デモ GIF 等）
└── _internal/                  個人メモ・調査ログ（gitignore 済み）
```

## Build (Windows, dev)

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
cargo build --release
Copy-Item -Force target\release\uv_project.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex"
```

After Effects を再起動し、`Distort > UVProject` から適用。

## 規約

- 8-bit ARGB を基本ターゲットにする（Deep Color / 32-bit float は未対応）
  - 注: UV マップは 8-bit だと段差が出やすい。将来 16/32-bit 対応が改善ポイント
- 現状 CPU のみ。GPU は当面入れない
- 公開仕様を変えたら `README.md` と `README.ja.md` の両方を更新する
- ビルド成果物（`*.aex` `*.dll` `target/`）と `_internal/` はコミットしない

## Memory pointers

- `[[reference_rust_ae_plugin_gotchas]]` — ロード拒否(ABI) / PiPL エラー /
  256 バイト panic の定番対処
- `[[feedback_careful_analysis]]` — ルック再現は対象の仕組みを徹底分析してから
