# onmkFlare

After Effects向けのレンズフレアプラグインです。`blackhole-rt/flaresim` のMIT実装を基に、実レンズ処方から全ての有効な反射面ペアを3波長（650/550/450 nm）で追跡します。

## Human Eye Glare

`Human Eye Glare` はカメラのゴーストとは独立した肉眼グレア層です。

- 瞳孔径に応じて現れる非対称な ciliary corona
- 年齢補正を含む広域 veiling glare
- RGBで半径を分けた lenticular halo
- Eye Seedによる個人固有の光条パターン
- Tear Film Motion と決定論的な Eye Motion Phase
- 目を細めた際の長い扇状光条を作る Squint / Eyelash

添付例のような太陽の長い光条は、`Source Mode`をManualまたはManual + Image、`Human Eye Glare`をオン、`Squint / Eyelash`を100～220、`Squint Length`を80～140%から調整してください。通常の夜間点光源ではSquintを下げ、Ciliary CoronaとVeiling Glareを中心に使います。

## Physical Renderer

- Cooke Triplet、Double Gauss、ARRI/ZEISS Master Prime 50mm、Canon EF 200-400mmのレンズ処方
- ハイライト抽出と手動光源（単独または併用）
- 全有効面ペアの逐次屈折・2回反射・センサー交差
- Cauchy分散、Fresnel反射、薄膜ARコーティング
- Hullin方式の瞳レイ格子・三角形面再構成、面積正規化、複数回ゴーストブラー
- 最大6オクターブの暖色クロマティックブルーム
- CPUフォールバックとWGPU合成経路で同一の物理レイヤー

`Ray Grid` は品質と速度の主要設定です。40が標準、プレビューでは16〜24、最終出力では48〜64を推奨します。複雑なCanon処方は多数の面ペアを持つため、Cookeより計算量が大きくなります。

## Build

ぼかしは隣接32列をまとめて処理し、prefix作業領域を再利用します。光源核・回折・Hazeは最大4スレッドで行単位に計算します。画素ごとの加算順序、光学式、サンプル数、ぼかし半径・回数は維持しており、元のぼかし／単一スレッド光源計算とのビット一致テストがあります。

1080p・半径40・3パスのReleaseぼかし計測は約76〜79msから53〜55msでした。2026-10-03のAEXCompat純黒720p・既定設定の3回比較ではworker elapsedが約1.207秒から1.106秒へ短縮し、最終出力ハッシュが一致しました。この時間はworker初期化を含み、AEの常駐状態や複数フレーム同時処理の速度を保証する値ではありません。

ぼかし単体の比較計測は `cargo test --release benchmark_blur -- --ignored --nocapture` で実行できます。

GPU経路は直近1件の光学レイヤーを最大128MiBまで共有キャッシュします。入力から抽出した光源、解像度、レンズと光学設定が一致すると再利用し、明るさ・不透明度・合成方式・Flicker・Atmosphere・Edge Triggerだけの変更では再利用を維持します。光源位置やレンズの変更は再計算になります。キャッシュ一致・無効化の検証は `cargo test --release optical_cache_matches_uncached_render -- --ignored --nocapture` で実行できます。

キャッシュが使えないGPUフレームでは物理ゴーストとBloomを独立に計算します。Bloomは最大2オクターブを並行計算し、元の順序で加算します。追加のフルフレーム作業領域を抑えるため、2,097,152画素を超える画像ではオクターブを逐次処理します。`parallel_bloom_is_bit_exact` は1～6オクターブ、色分散のオン／オフ、複数のパス数と縦横1画素を含む寸法で逐次版とのビット一致を検証します。

2026-10-03の720p直接GPU計測では、キャッシュなし初回が従来約233msから約130ms、光源を動かした5フレームが130～138msでした。AEやworkerの起動を含まない内部レンダー時間であり、AE全体のフレーム時間とは異なります。

追加の行分割ラスタライズでは、物理ゴーストの三角形を最大4スレッドで描画します。同じ行への加算順序は維持し、262,144画素未満は逐次実行します。4種類のレンズ・2光源・奇数寸法で逐次版とのビット一致を検証しています。同じ720p計測で光源移動時は約89～92ms、Double Gaussのゴースト層は約290msから165～169msでした。計測用テストではゴースト層とBloomの時間も出力します。

ぼかしの内側ループは連続スライスで処理し、横方向の固定幅区間を端の可変幅区間から分離しています。RGBを連続配列として扱い、コンパイラが減算と除算をベクトル化しやすくしています。逆数乗算への置換や加算順序の変更はせず、参照版とのビット一致を維持します。この変更後、同じ720p光源移動の5フレームは75～80msでした。これは実機上の内部描画計測であり、他のCPUやAE全体での速度を保証するものではありません。

描画の起動コストは三角形を順序どおりにバッチ化して削減し、32行の帯を最大4ワーカーへ交互に割り当てて負荷を分散します。バッチは32,768三角形を目安に消化し、最大でも瞳格子1枚分の追加に制限します。画素の加算順序とサブピクセル処理は維持します。この変更の720p光源移動計測は67～72ms、Double Gaussのゴースト層は116～124msでした。条件による変動があるため、再計測には上記のReleaseテストを使用してください。

1080p向けにはBloomの非ゼロ領域を追跡し、各ぼかしパスで半径分だけ処理範囲を拡大します。閾値による暗部の切り捨てはせず、画面端の分母とprefix加算順序を維持します。負値・非有限値は従来処理へ戻します。1080pの4層ぼかしを逐次比較した実測は118～122msから48～52ms、GPU経路の光源移動5フレームの中央値は約159msから138msでした。後者はホスト初期化を含まない内部描画時間です。

`cargo test --release benchmark_sparse_1080p -- --ignored --nocapture` で同一入力の処理時間とビット一致を比較し、AEXCompat用の黒い1080p入力も生成します。`cargo test --release optical_cache_1080p -- --ignored --nocapture` は1080pの光源移動・レンズ切替・キャッシュ無効化を検証します。

### 1080p・サブエージェントレビュー付き5回の改善（2026-10-03）

画質設定を固定し、各指摘を実装した後、Cooke／Double Gauss各5フレームの参照画像と比較しました。

| 回 | 変更 | 判定・実測 |
| --- | --- | --- |
| 1 | 三角形の前処理をバッチ内で共有 | 単独ではほぼ横ばい。次のSIMD処理の共通土台として使用 |
| 2 | 三角形の画素判定・補間をSSE2で4画素並列 | Double Gaussの全体中央値232→194ms |
| 3 | 光源層のsin／cos／expの同値計算を共有 | 層単体の同一実行内比較89～94→84～86ms |
| 4 | 光学レイヤー合成を最大4分割 | 合成単体9.1～9.6→6.1～6.6ms。小画像は呼び出し元で逐次処理 |
| 5 | AVX対応CPUで8画素並列、SSE2／スカラーへフォールバック | 最終2回の全体中央値：Cooke117ms前後、Double Gauss163～167ms |

開始時の中央値はCooke133ms、Double Gauss232msでした。これらは1080p・黒入力・Manual光源移動・Ray Grid 26の内部GPU経路計測で、AE起動やホスト側の時間は含みません。FMA・近似除算・サンプル削減は使用していません。旧三角形描画とSSE2／AVXの比較、非有限値を含む合成テスト、および改善前10フレームとの全画素一致を検証しています。

再計測は `cargo test --release benchmark_five_rounds_1080p -- --ignored --nocapture`。参照画像は `target/perf-five-rounds-reference` に保持します。参照のない環境では検証を失敗させるため、最初の作成時だけ改善前ビルドで環境変数 `ONMK_CAPTURE_PERF_REFERENCE=1` を指定してください。既存参照は上書きしません。層単位の比較は `benchmark_source_math_reuse` と `benchmark_combine` を同様に実行できます。

### 追加5回の改善（2026-10-03）

同じ画質設定で、さらに次の5項目を実装・レビューしました。

| 回 | 変更 |
| --- | --- |
| 1 | 最初の反射までの光線状態を描画内で共有。状態データは最大16MiB |
| 2 | f32で厳密にゼロになるGaussianのexp計算を省略 |
| 3 | Bloomの非ゼロ範囲を各オクターブで共有し、加算範囲も限定 |
| 4 | ARGB8をGPUへ直接転送し、CPUでの詰め替えを削除 |
| 5 | 同じ光学レイヤーを使う場合、GPUバッファへの再転送も省略 |

1080p・光源移動の中央値は開始時Cooke118ms／Double Gauss163ms、完了後の複数実行でCooke100～109ms／Double Gauss141～150msでした。キャッシュ利用時は約13msから4～6msへ短縮しました。後者は光源移動やレンズ変更で再計算するフレームとは別の値です。いずれも内部GPU経路の実測で、AE全体の時間ではありません。

通常14テストに加え、改善前の1080p画像10枚、色・アルファ・合成方式の12条件を全画素一致で検証しました。バッファ再作成と光源・レンズ・Bloom変更時の再転送も検証します。Bloomの負値・負のゼロ・非有限値・極端な値は全面処理へ戻し、NaNテストは双方NaNであることを確認します。ARGB直接転送はlittle-endianホストに限定しています。

色付き参照比較は `cargo test --release colored_byte_layout_references -- --ignored --nocapture`。参照画像はローカルの `target/perf-colored-reference` に保存し、上記と同じ明示的な初回作成方式を使います。生成画像とビルド成果物はリポジトリに含めません。

```powershell
cargo test
cargo build --release
```

比較レンダーは `cargo test write_comparison_preview -- --ignored` でPPMとして生成できます。

## Recovery

全面移植前の状態は、元の独立したonmkFlareリポジトリのGitタグ `before-full-flaresim-port` に保存されています。以下はその独立リポジトリ内だけで使えます。モノレポへのソース取り込みには、このタグと過去の履歴は含まれません。

```powershell
git switch --detach before-full-flaresim-port
```

ライセンスと原実装への表記は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。

## Flare Studio

エフェクト先頭の `Open Flare Studio...` ボタンから、Optical Flares風の専用ルックエディターを開けます。AEXへ自己完結で埋め込まれるため、別途Webサーバーやインストールは不要です。

- 5種類の統合Characterと、従来ルックを保存した8種類のLegacyプリセット
- Hotspot、Glow、Streak、Stripe、Ring、Ghost、Bloom、Starburst、Ocularのレイヤー表示
- ドラッグ可能な光源とリアルタイムCanvasプレビュー
- レンズモデル、強度、半径、色収差、ブレード数などの編集
- プリセットJSONの保存・読込と設定値のクリップボードコピー

AEでは `Character / Intensity / Scale / Ghost Complexity / Diffraction` を主要操作として使います。すべて同じ物理反射・回折・ブルームのパイプラインを共有します。以前の見た目は `Legacy / ...` として選択できます。
