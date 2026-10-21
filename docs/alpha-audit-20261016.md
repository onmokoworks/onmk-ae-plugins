# アルファ境界監査 — 2026-10-16

対象は `plugins/` の25ディレクトリ。ソースの検索・境界処理の読み取りと、2関数の独立した実行検査を行った。全 AEX や GPU の動作保証ではない。他プラグインの本体はこの監査では変更・配布していない。既存の未コミット変更を保持した。

## TaperStroke

v0.3.1 を MediaCore に配置し、SHA256 `EC5BAA5502EB783617FC1C0E65B376D807010642259E5F1DF2EA56BA7D5E9577` の一致を確認。AE 操作・終了・再起動はしていない。読み込み済みの旧版はユーザーが AE を再起動するまで置き換わらない場合がある。

## 同じ誤前提がある12プラグイン

| プラグイン | 根拠となるソース・処理 | 影響／確認レベル |
|---|---|---|
| LumaNormal | `plugins/luma-normal/src/lib.rs:186` `normal_pixel` | 出力法線 RGB に alpha を乗算。実関数を抽出した実行検査で、期待 ARGB `[128,128,128,255]` が `[128,64,64,128]` になり失敗 |
| EdgeSmith | `plugins/edge-smith/src/lib.rs:437` `output_rgb` | 最終 RGB に alpha を乗算。実関数の検査で `RGB=1, alpha=.5` の期待1が0.5になり失敗 |
| MaskOffset | `plugins/mask-offset/src/lib.rs:451` と `multiply_alpha` / `write_pixel` | マスク被覆率で全チャンネルを減衰する誤前提。フェザーや半透明色に影響。静的確認 |
| ShapePlus | `plugins/shape-plus/src/lib.rs` `composite_values` | alpha変更でRGBを比率補正し、色の合成も乗算済み形式のまま出力。既存テストも同じ前提。静的確認 |
| LumaGrain | `plugins/luma-grain/src/lib.rs:172` `process_pixel` | 輝度を alpha で除算し、RGB上限も alpha に制限。半透明の明色・ノイズ量が変わる。静的確認 |
| CelGlow2 | `plugins/cel-glow/rust/celglow2/src/lib.rs:611,630,670` | 入力を alpha で除算、出力を乗算。非線形な処理と合成で相殺できない。旧 celglow ターゲットは別途要検証 |
| UVProject | `plugins/uv-project/src/uv.rs:173` | UVを alpha で除算し、出力全チャンネルに coverage を乗算。座標のずれと縁の減衰。静的確認 |
| MedianPro | `plugins/median-pro/rust/src/lib.rs:293` | AE入力を unpremultiply して処理後に premultiply。部分透明を含むフィルター結果に影響。静的確認 |
| MinimaxMap | `plugins/minimax-map/rust/src/minimax.rs:39,86` | 同じ入出力変換。alphaを変更するモルフォロジーで特に影響。静的確認 |
| RioGrade | `plugins/rio-de-janeiro-filter/src/lib.rs:135` | 色を alpha で除算してから非線形補正。半透明RGBの補正が不透明時と変わる。静的確認 |
| PathArray | `plugins/path-array/src/lib.rs:1410` | 入力サンプルを乗算済みと仮定して over 合成。重なり・opacity時の色が不正になる候補。静的確認 |
| ParticleKit | `crates/particlelab_engine_core/src/renderer.rs:625`（plugin配下）と `engine.rs:124` | AE入力はpremultipliedという説明と既定 `source_premultiplied=true`。明示的モードもあるので一律変更不可。AE境界・mip・内部合成を分けて検証が必要 |

単なるコメント検索の件数ではなく、読み取り／最終出力／合成に影響する計算を確認した。実機で12件すべての黒縁を再現したという意味ではない。優先順は生成・マスク系（LumaNormal、EdgeSmith、MaskOffset、ShapePlus）、色処理系、フィルター／合成系。各変更で元の失敗を再現する検査を追加し、バイナリを個別に検証する。

実行証拠: `python tools/alpha_source_probes.py`。本番ソースから2関数をそのまま抽出し、Rustでコンパイルして外部の期待値と比較する。現在は2件の検出により **終了コード1**。`tools/target/alpha-probes/results.json` にソースSHAと実際の失敗を記録。この検査を「成功」として集計しない。

## その他の12ディレクトリ

AdaptiveFilter、DepthONNX、FrameSlice、MaskTransform、onmk Film、onmkFlare、onmkGlow、PrismWarp、RefractionDispersion、ScatterMap、Starglow、TuiImage は今回の検索で同じ明示的な AE-premultiplied 前提を検出しなかった。これは合格判定ではない。

MaskTransform のピクセル経路は行コピー、onmkFlare の `pixel.rs` と ScatterMap / Starglow / onmkGlow の `pixels.rs` はチャンネルの直接変換。グロー生成・ぼかし・GPU・色付き透明画素については、アルファ仕様を決めた上でのネイティブ検証が必要。25ディレクトリと複数ターゲットの一覧は `tools/target/alpha-audit-20261016/inventory.json`。

## 指定テンプレート

確認した作業コピー: `D:/Projects/01_Project/04_Tools/Ae_Plugins/AeRustGpuLabRust`。origin は `https://github.com/onmokoworks/onmk-aeplugin-rust-template.git`。入出力 `frame.rs` はチャンネル順／精度のみ変換し、RGBへalphaを乗算していない。境界の読み／書きを独立に検査する2テスト、Copy／Colorの部分透明RGB検査1テストを追加して成功。

別の透明処理リスク: `ae_gpu_lab_core/src/cpu.rs::box_blur` はRGBAを独立に平均する。白の不透明画素と黒の透明画素を混ぜると、straight RGBの色まで暗くなる。これはベンチマーク経路の既存挙動であり、今回の出力乗算ミスとは別。GPUと同じ結果でも正しさの証拠にならない。透明入力対応の本番テンプレートとして採用する前に alpha-weighted filtering と各バックエンドの独立オラクルが必要。今回、ベンチマークの演算内容は変更していない。

## 再発防止

[共通手順](alpha-contract.md)、生データ検査 `tools/alpha_contract.py`、その negative-control テストを追加。TaperStrokeの修正前データを確実に不合格、修正後を合格と判定できる。モノレポCIではツールのテストを行い、テンプレートCIではこれに加えて実際の frame / CPU 境界テストを行う。すべての既存プラグインが自動認証される仕組みではなく、実装ごとのホスト検証を追加するための共通条件。
