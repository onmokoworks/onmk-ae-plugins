# onmk Inner Shadow — 設計（GPU 対応改訂版）

状態: 実装済みソースの仕様書。ここに書いた数式が CPU / GPU 共通の正本であり、独立オラクル（`tests/oracle.rs`）もこの文書から直接書いている。
Adobe の Layer Style「Inner Shadow」とのピクセル互換は検証しておらず、主張しない。

表記: **[証拠]** はソースで確認した事実（パス付き）、**[決定]** は本設計の決定、**[未検証]** はホスト（AE / AEXCompat）での確認が残る事項。

## 1. 識別子

| 項目 | 値 |
|---|---|
| 表示名 / PiPL Name | `onmk Inner Shadow` |
| Match Name | `ONMK_InnerShadow`（変更禁止） |
| Category | `onmk`（`edge-smith`, `celglow`, `onmk-film` と同じ文字列） |
| crate / binary | `inner-shadow` / `inner_shadow.dll` → `dist/InnerShadow.aex` |

## 2. パラメータと安定 ID

**[証拠]** `plugins/depth-onnx/vendor/after-effects/src/pf/parameters.rs:1244-1293`（crates.io 0.4.0 の vendored コピー）では、パラメータのディスク ID は `format!("{:?}", variant)` の Murmur3 ハッシュ、UI インデックスは `add` の呼び出し順。したがって **enum のバリアント名を改名しないこと** と **末尾追加のみ** が互換性の条件。固定 rev `0e0b197` 自体のソースはこのセッションで読めていない（§12）。

| # | バリアント | UI | 型 | 範囲 | 既定 |
|---|---|---|---|---|---|
| 1 | `BlendMode` | Blend Mode | Popup Normal/Multiply | 1..2 | 2 Multiply |
| 2 | `Color` | Color | Color（32bpc は `float_value()`） | — | 黒 |
| 3 | `Opacity` | Opacity (%) | Float | 0–100 | 75 |
| 4 | `Angle` | Angle | Angle | 任意 | 120° |
| 5 | `Distance` | Distance (px) | Float | 0–1000（slider 0–100） | 5 |
| 6 | `Choke` | Choke (%) | Float | 0–100 | 0 |
| 7 | `Size` | Size (px) | Float | 0–250（slider 0–100） | 5 |
| 8 | `GpuMode` | Render Device | Popup Auto/GPU/CPU | 1..3 | 1 Auto |

`GpuMode` は元設計の 7 個の後ろに追加した。

## 3. 座標・単位

- 画素中心は整数格子。SmartFX の rect、`output_origin`、入力 world の位置はすべて **ダウンサンプル後のレイヤー座標**（バッファ px）。
- **[証拠]** `in_data.downsample_x/y()` と `in_data.pixel_aspect_ratio()`（幅/高さ）が `RationalScale{num,den}` を返す: `vendor/after-effects/src/pf/in_data.rs:197-216`。`downsample` を SmartPreRender で使う前例: `plugins/taper-stroke/src/lib.rs:149`。
- **[決定]** ユーザー長さ（Distance, Size）は「正方ピクセル換算 px」。x 方向バッファ px = 長さ × dsx / PAR、y 方向 = 長さ × dsy。PAR は getter の値を使う。`den == 0` や `num <= 0` の不正値のときだけ 1.0 とし、その扱いを README に明記する。PAR は [0.05, 20]、downsample は (0, 16] の範囲外を `BadCallbackParameter` とする。

## 4. シャドウマスク（規範）

入力 α を clamp(α, 0, 1) で読み、NaN は 0 とする。

1. **外側マスク** `E(q) = 1 − α(q)`。入力 world の外は **E = 1**（画像外 = 透明）。穴も外側として扱う。
2. **オフセット**（Layer Style 規約: 光の来る方向、0° = 右、反時計回り、y 下向き）
   `ox = Distance·(−cos θ)·dsx/PAR`, `oy = Distance·(sin θ)·dsy`。
   いずれも 2⁻¹⁶ 格子に丸める（三角関数の誤差除去）。`ix = floor(ox)`, `fx = ox − ix ∈ [0,1)`。
3. **Choke / Size の配分** `r_m = Choke·Size`, `r_b = Size − r_m`。
4. **Choke = 楕円平坦構造要素による E の膨張**（= マットの縮小）。半径 r（ユーザー px）について `Rx = r·dsx/PAR`, `Ry = r·dsy`。行 `j ∈ [−⌊Ry⌋, ⌊Ry⌋]` の半幅 `w(j) = ⌊Rx·√max(0, 1−(j/Ry)²) + 10⁻⁶⌋`。
   `Dil_r E(x,y) = max_{j} max_{|i|≤w(j)} E(x+i, y+j)`。
   小数半径: `r0 = ⌊r_m⌋`, `f = r_m − r0`, `D = D_lo + (D_hi − D_lo)·f`。`D_lo = Dil_{r0}E`、`D_hi = Dil_{r0+1}E`。半径 0 は E そのもの。
5. **Blur** σ = r_b/3、`σx = σ·dsx/PAR`, `σy = σ·dsy`。σ ≤ 10⁻⁴ の軸は恒等（重み [1]）。それ以外は `R = ⌈3σ⌉`、`w_i ∝ exp(−i²/2σ²)`（f64 で正規化してから f32 化）。x → y の順に分離して適用する。範囲外の標本は 1。
6. **Shift**（最後）: `M(p) = bilinear(B, p − o)`。`fx > 0` なら `x0 = p − ix − 1`、重みは `B(x0)·fx + B(x0+1)·(1−fx)`。`fx = 0` なら `B(p − ix)` そのもの。重みは画素ごとに再計算せず全画素で共通なので、タイル分割しても結果が変わらない。
7. **影量** `t = Opacity·clamp(M, 0, 1)`。

### 4.1 Distance = Size = 0 の決定

**[決定]** 一貫した逆 alpha 式を特例なしで使う。d = S = 0 のとき M = 1 − α となり、α ∈ {0,1} の画素は変化しない。0 < α < 1 の画素は `t = Opacity·(1−α)` の縁影を受ける。

理由:
- Size → 0⁺ や Distance → 0⁺ の極限と連続である。ゼロだけ恒等にする特例は、アニメーション時に段差（ちらつき）を生む。
- 「重なりに相関なし」とする標準 Porter-Duff 合成と同じ前提である。

ビット単位の恒等は **Opacity = 0** のときのみ（数学的にも t ≡ 0）。この場合は GPU を初期化しない高速経路を通る。

## 5. AE 境界と合成

**[証拠]** world の画素は ARGB 順、16 bit の白は 32768、行は `row_bytes < 0` の逆順になりうる。`Layer::buffer()` は負 stride でも最下位アドレスから返す（`vendor/after-effects/src/pf/layer.rs:79-94`）。

- 入出力はストレート RGB。RGB の空間処理は行わず、ぼかしは α 由来のスカラーマスクだけにかかる。このため alpha 重み付けの問題はそもそも発生しない（`docs/alpha-audit-20261016.md` 40行の問題に該当しない）。
- 画素ごとの規則（K = 影色、`as_color().float_value()`。失敗時は 8 bit 値 / 255）:
  - α ≤ 0 または NaN → 入力画素をそのままコピー（隠れ RGB を保持）
  - t = 0 → 入力をそのままコピー
  - Normal: `C' = C + t·(K − C)`
  - Multiply: `C' = C·(1 − t·(1 − K))`
  - 有限でない入力 C チャンネル → そのまま
  - α' = 入力 α（float は clamp しない）
- 等価な内部表現: 乗算済み P = α·C に対し P' = α·C'、α' = α（レイヤーの上に影を atop 合成）。最終 α で割り戻した値が C' なので除算は不要。
- 書き戻し: 8 bit は round(C'·255) を [0,255] に、16 bit は round(C'·32768) を [0,32768] に収める。float は丸めない（HDR・負値保持）。
- 入力 world がない場合（`checkout_layer_pixels` が None）は出力を透明 0 にする。

## 6. SmartFX / Legacy Render

**[証拠]** `PreRenderExtra::{output_request, set_result_rect, set_max_result_rect, set_pre_render_data}`、`checkout_layer` が `PF_CheckoutResult{result_rect,max_result_rect}` を返すこと（`vendor/.../pf/render.rs:89-147,279-310`）。出力 world と入力 world の対応づけ（出力は `in_data.output_origin()`、入力は checkout の `result_rect.left/top`）は `plugins/edge-smith/src/lib.rs:255-265` と `plugins/cel-glow/rust/celglow2/src/lib.rs:737-738` の前例と同じ方式。

- **SmartPreRender**
  1. パラメータから Plan を作る。
  2. 出力要求 `req` に対し、`Ω(req) = [req.left − ix − ex − pad_x, req.right − ix + pad_x) × …` を計算する。ex = (fx > 0)、pad = blur 半径 + 膨張の最大半幅。
  3. `bbox(req, Ω)` を入力要求として checkout する。
  4. `result_rect = 入力 result_rect ∩ req`、`max_result_rect = 入力 max_result_rect` とする。alpha は不変なので出力は入力より広がらない。`returns_extra_pixels` は使わない。
  5. Plan、GpuMode、入力 rect を pre_render_data に保存する。
- **SmartRender**
  - 入力 world の寸法が保存した rect と違えば `BadCallbackParameter` を返す（黙ってずれた結果を出さない）。
  - 出力 world の位置は `output_origin`、大きさは world の寸法から決める。
  - check-in は guard で 1 回だけ行う（panic 時も）。panic は `InternalStructDamaged` に変換する。crate の `catch_panics` は panic を PF_Err_NONE にしてしまうため、その前に自前で捕まえる。
- **Legacy Render**: 入力はレイヤー全体（原点 0,0）、出力位置は `output_origin`。8/16 bit。32 bit は SmartFX のみ（SDK の想定）。
- PiPL: `DeepColorAware`、`FloatColorAware | SupportsSmartRender | SupportsThreadedRendering | SupportsGetFlattenedSequenceData`。**[証拠]** crate が GlobalSetup で PiPL の out_flags を自動設定する（`vendor/.../plugin_base.rs:426-431`）。
- **AE ネイティブ GPU（SmartRenderGPU / `SupportsGpuRenderF32`）は宣言しない。** wgpu は通常の SmartRender 内で CPU メモリ ↔ GPU を往復する内部アクセラレーションにすぎない。GPU suite 連携は実装していないので、`SmartRenderGpu` コマンドが来た場合はエラーを返す。edge-smith はこのフラグを宣言しているが、本プラグインはその構成を引き継がない。

## 7. バックエンドと選択ポリシー

```
identity (Opacity=0)          → 入力コピー。GPU を初期化しない。
CPU                           → CPU。GPU を初期化しない。
GPU                           → GPU。失敗すると明示的なエラー（ae::Error::Generic）。CPU で黙って代替しない。
Auto                          → GPU を試し、未対応・不在・失敗なら CPU。経路と理由は Route として記録する。
```

GPU の条件: wgpu 25 で HighPerformance アダプタを要求し、`DeviceType::Cpu`（WARP / llvmpipe 等）は拒否する。デバイス limits はアダプタの値をそのまま要求する。

### 7.1 GPU パイプライン（`shader.wgsl`、workgroup 16×16）

| pass | 内容 |
|---|---|
| `level` | 疎テーブルの次段 `L_k(x) = max(L_{k−1}(x), L_{k−1}(x+2^{k−1}))`（範囲外は 1） |
| `accum` | 段 k に属する行スパン `(j, w)` について `max(L_k(x−w, y+j), L_k(x+w−2^k+1, y+j))` を D に max 集約 |
| `blur_h` | `D_lo + (D_hi − D_lo)·f` を x 方向に畳み込み |
| `blur_v` | y 方向に畳み込み |
| `composite` | bilinear shift → t → Normal/Multiply。出力は `(r', g', b', t)` |

- **膨張のコスト:** 1 画素あたり O(段数 + 2⌊Ry⌋+1)。段数は ⌈log₂(2·w_max+1)⌉ 以下で、2D 半径の二重ループは使わない。CPU も同じアルゴリズムを使う。max 演算は丸め誤差がないので、膨張結果は CPU と GPU でビット一致する。
- **ぼかし:** 分離型で O(2R+1)/画素/軸。重みは CPU で計算した同じ f32 配列を両バックエンドで使う。
- **GPU とホストの分担:** E 平面の構築（O(N)）、raw コピー規則、深度ごとの読み書きは CPU 側の共通コードで行う。GPU 出力の α チャンネルには t が入っており、これを使って finalize する。NaN の payload や denormal を GPU がどう扱っても、コピー規則でビット単位に保護される。
- **キャッシュ:** `Mutex<Slot>` に `Ready(Arc<Ctx>)` / `Failed(時刻, 理由)` を持つ。失敗後 10 秒は再試行しない。デバイス・検証エラーが起きたら Ctx を破棄し、次回再初期化する。pipeline、bind group layout、dummy buffer は Ctx に 1 度だけ作る。
- **フレームごとの処理:** buffer はフレームごとに作り、フレーム終了で解放する。エンコーダ 1 個、submit 1 回にまとめる。
- **MFR と GPU:** Ctx 内の `Mutex` で GPU フレームを直列化する（error scope と poll を確定させるため）。CPU 経路は直列化しない。
- **上限:** Ω ≤ 2²⁶ 画素（CPU / GPU 共通。超えたら `OutOfMemory`）。GPU は各 buffer を `max_buffer_size` と `max_storage_buffer_binding_size`、ディスパッチ数を `max_compute_workgroups_per_dimension` と事前比較し、超える場合は GPU エラー（Auto なら CPU）とする。
- **エラー処理:** Validation と OutOfMemory の error scope でフレーム全体を囲む。readback は 非ブロッキングpollと30秒の期限。マップに成功したときだけ unmap する（edge-smith の「失敗時も unmap」は直す）。
- **GPU を使う根拠:** 性能値は計測するまで記載しない。`examples/render_fixture.rs` が CPU と GPU の時間とアダプタ情報を出力する。

## 8. メモリ

- CPU: E、L_a、L_b、D_lo、D_hi、ぼかし作業用（f32 × Ω、最大 6 面）と、出力 RGBA（16 B/px）。
- GPU: E、L_a、L_b、D_lo、D_hi（Ω 平面 5 面）と、RGBA（in-place）および readback。
- 確保は `try_reserve_exact` で行い、失敗したら `OutOfMemory` を返す。

## 9. 検証

- `tests/oracle.rs`: 仕様から直接書いた f64 の素朴実装（2D 列挙による膨張、2D 直接畳み込み、一般 bilinear、Ω を持たない遅延評価）と、閉形式の fixture。部分 alpha（0, 1/255, .125, .5, 1）、隠れ RGB、16 bit 低ビット、HDR・負値、角度方向、小数オフセット、穴、画像端、タイル分割と全体の一致（非ゼロ原点・入力のクロップ）、downsample / PAR、退化入力、並行実行、乗算済み出力の拒否を含む。
- `tests/gpu.rs`: GPU をオラクルと閉形式で照合し、アダプタ名、backend、device type、ディスパッチ数を表示する。`INNER_SHADOW_REQUIRE_GPU=1` のときにハードウェア GPU がなければ失敗する。
- `verify.ps1`: テスト（GPU 必須モードあり）、release ビルド、fixture 画像（PPM）と raw RGBA の生成、成果物とソースの SHA256 記録。
- **これらは AE の検証ではない。** AEXCompat による Render / SmartRender × 8/16/32 の生データ比較は、別の証拠ゲートとして未実施。

## 10. 範囲外

Noise、Contour、Global Light、Anti-aliased トグル、AE ネイティブ GPU 経路。

## 11. 実装ファイル

```
Cargo.toml build.rs shader.wgsl build.ps1 verify.ps1 README.md README.ja.md docs/design.md
src/lib.rs     AE glue（params, PreRender, SmartRender, Render, check-in guard）
src/world.rs   ARGB world ↔ RGBA f32（深度, stride, 行順, 境界チェック）
src/spec.rs    Plan（共通数式）
src/cpu.rs     CPU バックエンド
src/gpu.rs     wgpu バックエンド（キャッシュ, limits, エラー処理）
src/engine.rs    Ω, E 構築, バックエンド選択, finalize
tests/oracle.rs tests/gpu.rs examples/render_fixture.rs
```

## 12. 残る未検証事項

- 固定 rev `0e0b197`（after-effects 0.3.0）でコンパイルと46件のCPU/GPUテストが成功。初回調査はvendored 0.4.0を参照したが、実装の型・APIは固定版でビルド確認済み。
- `output_origin` と入力 `result_rect` による対応づけ（repo の前例どおりだが、AEXCompat / AE で未確認）。
- PAR ≠ 1 や downsample のときのホスト上の見え方。
- AE の 32 bit Legacy Render の有無。
- 実機 AE の MFR 下での GPU 直列化の性能。

## 0.2.0: Keep Source / macOS

`KeepSource` checkbox is appended after `GpuMode`, default true. The original alpha-preserving compositing and identity rules above apply with Keep Source enabled. Existing parameter variant names and indices remain unchanged.

When disabled, output straight RGB is the selected shadow color and output alpha is `clamp(input_alpha, 0, 1) * clamp(shadow_amount, 0, 1)`. Shadow amount includes opacity. Blend Mode has no effect in this mode. Zero opacity returns transparent output without initializing the GPU. CPU and wgpu share this finalization; the GPU still computes the shadow mask.

The pinned wgpu dependency enables Metal. `build_macos_release.sh` packages a native Apple Silicon or Intel `.plugin` with PiPL, Info.plist, executable and ad-hoc signature. macOS compilation, Metal execution and AE loading remain unverified; see [macOS instructions](macos.md).