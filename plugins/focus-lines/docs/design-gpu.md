# ONMK FocusLines — GPU 対応設計（v0.1 実装仕様）

2026-10-21。先行設計 `_focus-lines-design/design-opus-5.5.md` と、それに優先する
`review-and-decisions.md`（以下「レビュー」）を受けた実装仕様。ここに書いた内容が
このディレクトリの実装の正本。性能値は計測値として明記したもの以外は目標。

## 1. 全体構成

```
AE (Render / SmartRender, 8/16/32 bpc, CPU バッファ)
  └ lib.rs           パラメータ読取 → Settings + Mapping + frame index（1か所で座標・時間を確定）
      └ layout.rs    CPU: 線の生成（安定ID・乱数・束ね・空白・テーパー）→ 凸多角形プリミティブ
      └ raster.rs    CPU: タイル／バンドへのビニング（共通）と CPU 基準ラスタライザ
      └ gpu.rs       wgpu 26 compute: 同じプリミティブ・ビンから coverage を計算し、ダウンロード
      └ backend.rs   Auto / GPU / CPU の選択とフォールバック
      └ pixel.rs     coverage → straight ARGB（Lines Only / Over Source）、8/16/32 書き込み
```

- 形状生成（線ID・乱数・幾何）は常に CPU。coverage 生成だけを CPU か GPU で行い、
  ピクセル書き込み（アルファ契約）は両バックエンド共通の `pixel.rs`。
- **AE ネイティブ GPU（`SupportsGpuRenderF32` / `SmartRenderGpu`）は宣言しない。**
  本プラグインは AE の CPU ワールドを受け取り、私有の wgpu デバイスで coverage を計算して
  CPU へ戻す（ダウンロード）。AE から見ると通常の CPU エフェクトであり、AE の GPU
  フレームバッファとは無関係。PiPL / GlobalSetup の GPU フラグを立てると AE が GPU
  ワールドを渡してくる契約になるため、実装していない機能を宣言しないことになる。
  ラベルは「GPU (private wgpu device)」とし、Mercury GPU 加速とは呼ばない。

## 2. 被覆率の定義（CPU/GPU 共通の品質ポリシー）

レビュー2・7に従い、**サブサンプルごとに図形の和集合と空白楕円の除外を評価してから**
画素内で平均する。coverage 平均後の 1−Π(1−c) は使わない。

- 各画素行 y（絶対バッファ座標）に S 本の水平走査線 `ys = y + (s + 0.5)/S`
  （S = 8 Standard / 16 High）。
- 各走査線で、候補プリミティブ（凸多角形）と走査線の交差区間を求める。辺の判定は半開区間
  `(a.y <= ys < b.y) || (b.y <= ys < a.y)` で、隣接するセグメントが共有する辺を二重に
  数えず、サンプルが辺上にあっても一意に割り当てる。
- 区間を **和集合** にしてから、同じ走査線上の空白楕円区間を差し引き、画素 `[x, x+1)`
  との重なり長を解析的に求める（水平方向は厳密、垂直方向は S サンプル）。
- coverage = Σ_s 長さ_s / S。**厳密な面積ではない**（垂直方向は S 点サンプリング）。
  垂直な辺は厳密、水平に近い辺は最大 1/(2S) 程度の誤差。1/S px より薄い先端は欠けうる。
- 同じ線を何本重ねても coverage は1本と同じ（和集合）。テストで固定する。
- GPU は画素ごとに区間を画素範囲へクリップしてから最大8区間の局所和集合を保持する。
  8区間を超える場合は、全候補の端点を順に走査する容量制限のない和集合処理へ切り替える。
  隙間を埋める近似はしない。12個の離れた細区間のGPUテストで検証する。
- 計算量: 画素 × S × （その画素のタイルに入る候補数）。全線を総当りしない。

## 3. 線の生成（CPU、`layout.rs`）

座標は「表示空間」= 全解像度レイヤー px を PAR で正方化した空間
（X = x·par, Y = y, y 下向き）。角度は 0° = 上、時計回り。基準長 R = H/2（全解像度高さ）。

1. 線 k = 0..ceil(N)-1（N ≤ 5000、小数可）。k = floor(N) の線は幅に frac(N) を掛けてフェード。
2. 乱数 `u(seed,k,ch,epoch)` は splitmix64 の純関数。グローバル状態・前フレーム依存なし。
3. 基準角 `θ = 2π·frac(k·0.6180339887498949 + o_seed)`（Weyl 列。N を変えても既存線不変）。
4. 揺らぎ ±AngleJitter°、束ね: 最近傍クラスタ中心（Weyl 列）+ spread へ Clustering で寄せる。
5. 空白扇形（レビュー5）: GapWidth ≤ 0 は無効、≥ 360 は全線非表示、Softness = 0 は段階関数
   （δ ≥ GapWidth/2 で表示）、それ以外は smoothstep。幅と長さ（内端を外へ）に適用。
6. 密度揺らぎ: 低周波 D(θ) で内端を外側へ押す（ON/OFF しないのでポップしない）。
7. 中心楕円: 縦半径 b = CenterSize%·R、横 a = b·Aspect、回転 Rotation。
   オフセット (ox, oy) は楕円半径に対する比で、**長さ √(ox²+oy²) を 0.95 に放射方向クランプ**
   （レビュー1）。楕円中心 E = F + Rot·(a·ox, b·oy)。サイズ0は空白なしの別分岐。
8. 内端 r0 = 楕円出口距離 + LengthRandom·u·R + 密度押し出し。外端は焦点から
   レイヤー矩形を出る距離（画面外焦点は光線と矩形の交差で判定、交差しなければ除外）。
9. テーパー（レビュー3）: 幅 w(d) = W_k·(tip + (1−tip)·min(d/L,1)^p)。L = 0 は定義上
   即座に一定幅（除算なし）。テーパー区間は推定誤差 0.02 px（バッファ空間）で適応分割、
   上限 64 区間（上限到達は統計 `capped_lines` に記録）。一定幅区間は別の1区間。
   p = 1 でも最低2区間（テーパー + 一定幅）。
10. 各区間は台形 → バッファ空間へアフィン写像 → **レイヤー矩形でクリップ**（f64、
    Sutherland–Hodgman、最大8頂点）。ROI に依存しない矩形でクリップするため
    プリミティブは ROI と無関係で、画面外焦点の巨大座標も f32 に入る前に除去される。
11. 空白の保証（レビュー7）: 線の側面が楕円内に入っても、各サブサンプルで楕円区間を
    差し引くため楕円内に線は描かれない。

## 4. 座標・時間（1か所で確定、`lib.rs` → `Mapping`）

- Mapping = {layer_w, layer_h（全解像度 px）, par, dsx, dsy}。
  表示 → バッファ: `x_buf = X/par·dsx`, `y_buf = Y·dsy`。ds はこの写像で **1回だけ** 適用
  （スライダー px 値に別途 ds を掛けない。レビュー6）。
- 出力画素 (i, j) は絶対座標 `[i+ox, i+ox+1) × [j+oy, j+oy+1)`、(ox, oy) = `output.origin()`
  （TaperStroke と同じ取り方）。GPU も絶対座標で評価するので ROI/タイル分割に依存しない。
- Focus Point: 固定 crate の doc（`in_data.rs`「Point はダウンサンプル補正済み」）に従い
  `x_full = value_x / dsx` とする。**AE 実機で未確認**（レビュー6）。
- レイヤー寸法: `in_data.width()/height()` を全解像度とみなし、ダウンサンプル範囲は
  `ceil(W·dsx)`（TaperStroke の SmartPreRender と同じ前提）。**AE 実機で未確認**。
- 時間（レビュー4）: frame = `current_time.div_euclid(time_step)`（i64 の有理演算、負時刻は floor）。
  Hold Frames: epoch_k = `(frame + stagger_k).div_euclid(hold)`。epsilon は使わない。
  time_step はレンダー時のレイヤーのフレーム長であり、タイムストレッチ等では
  コンポのフレームと一致しない場合がある（仕様）。Static は時間に依存しない。
  秒ベースのステップは v0.1 では実装しない。

## 5. ビニング（`raster.rs`、CPU/GPU 共通）

- 16 px のタイル（絶対座標グリッド）。プリミティブごとに、交差するタイル行ごとの
  スラブ内 x 範囲を求め、該当タイルにだけ登録（長い斜め線の bbox 過剰登録を回避）。
- タイルリストはプリミティブ番号昇順（決定的）。CPU はタイル行（バンド）単位のリスト、
  GPU はタイル単位のリストを使う。項目数上限 32M（超過はエラー → Auto は CPU）。

## 6. GPU（`gpu.rs`, `shader.wgsl`）

- `OnceLock<Result<Ctx, String>>` に私有デバイスを遅延初期化（CPU バックエンドは GPU を
  一切初期化しない）。`Mutex<Scratch>` で再利用バッファを保持し、MFR の並行レンダーは
  このロックで直列化。スクラッチ合計が 256 MB を超えたら解放。
- 出力は行ストリップ単位（1ストリップ ≤ 64 MB）で dispatch → staging へコピー → map。
- 未捕捉エラーハンドラを登録（wgpu 既定の panic を防ぐ）、error scope と poll の失敗、
  panic（catch_unwind）をエラーとして返す。デバイスレベルの失敗後は以後 GPU を使わない
  （AE 再起動まで）。
- Backend パラメータ: Auto = GPU を試し、失敗時 CPU（理由を記録、About に表示）。
  GPU = 失敗時はレンダーエラー（黙って CPU にしない）。CPU = GPU を初期化しない。
- Auto でも同一レンダー内で CPU と GPU を混ぜない。CPU/GPU の結果は f32/f64 の差で
  ビット一致しない（許容差は §8）。

## 7. アルファ契約（`docs/alpha-contract.md` 準拠）

- AE 境界は straight RGB。8 bit = 255、16 bit = 32768、32 bit float は RGB を clamp しない。
- Lines Only: RGB = Color·Intensity（全画素一定、α=0 でも同色）、α = coverage·Opacity。
- Over Source: 入力 straight (Rs, As) を内部で乗算済み合成し、最終 α で戻す。
  `P = C·a + Rs·As·(1−a)`, `Ao = a + As·(1−a)`, `RGB = P/Ao`。a = 0 の画素は入力の
  生ピクセルをそのままコピー（HDR・負値・透明部の色を保持）。
- テスト: 独立した期待値（解析・点サンプリング）、意図的な premultiplied 出力が
  受入れ関数で不合格になること、白背景で縁が暗くならないこと。

## 8. 検証層と受入れ基準（v0.1）

1. コア（`cargo test --offline`）: 和集合（同一線3本 = 1本）、空白楕円内 0、Weyl 安定性、
   Hold Frames の有理・負時刻、ゼロ／極端値、ROI 一致（CPU ビット一致）、解像度一貫性、
   アルファ契約と負の対照。
2. GPU（`tests/gpu.rs`、アダプタがある場合）: GPU を独立期待値と CPU の両方に照合。
   CPU との許容差: 平均 |Δ| ≤ 1e-4、画素の 99.9% で |Δ| ≤ 1e-3、最大 |Δ| ≤ 1/S + 1e-3。
   アダプタがない場合は SKIP と表示し合格に数えない
   （`FOCUSLINES_REQUIRE_GPU=1` で失敗扱い）。
3. `examples/smoke.rs`: アダプタ名・実バックエンド・時間（形状、ビニング、GPU 転送込み）・
   CPU/GPU 差分を出力し、白背景合成とアルファの PPM/PGM を書く。
4. AEXCompat / AE 実機: 本ディレクトリでは未実施。座標規約（Point, ds, PAR, origin）、
   6経路の生データ、キャッシュ挙動は親作業での検証対象。

## 9. v0.1 で実装しないもの（明示的な延期）

- 曲線、マスク範囲、複数焦点、線ごとの色、外端フェード、秒ベースのステップ、
  プリセット popup（数値表は README）、AE ネイティブ GPU 経路、CPU 内部並列化。
- Cluster Spread 以外の UI は先行設計の表に従う。`Steps per Second` は `Hold Frames` に
  置き換え（レビュー4）。Render グループ（Backend, Quality）を追加。
