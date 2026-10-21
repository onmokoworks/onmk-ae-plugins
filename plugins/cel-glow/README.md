# CelGlow

## Headless quality audit (2026-10-04)

Legacy CPU distance transform now preserves original1D costs while writing output.
A failure-first7x9 case returned sqrt(5) instead of sqrt(8);24 small-mask cases
now agree with exhaustive nearest-seed distances. This is a correctness repair,
not a look redesign. EDT validates exact buffer lengths/checked dimensions and
reserves all temporary storage fallibly before writing output. Invalid geometry
leaves output untouched. GPU parity and aggregate memory limits remain unverified.

旧CPU距離変換では、出力で上書きした距離を後続の計算が参照する不具合を修正しました。
7×9画像で正しい√8が√5になることを再現し、修正後は24種類の小画像で全シードとの
距離を直接比較した結果に一致します。見た目の再設計ではなく計算誤りの修正です。
距離変換の寸法・配列長を検査し、出力前に全作業領域を失敗可能な方法で確保します。
不正な寸法では出力を変更しません。総メモリ上限やGPUとの一致は別途検証が必要です。

Legacy CelGlow now routes every exit after primary input acquisition through a
single checkin wrapper, including output-checkout/parameter failures and unwind.
Render errors take precedence; successful work reports checkin failure. This
cleanup also receives errors from legacy world-layout validation. Legacy input/output
conversion now checks formats, dimensions, stride and null pointers before SDK
buffer access, handles negative rows, and reserves input conversion storage fallibly.
Legacy color math is unchanged. The adapter's full-frame output/mask/distance arrays
and blur workspace now use checked, fallible allocation. Blur reuses one prefix
array instead of allocating per row/column; direct-average unit tests cover narrow
images and large radii. GPU allocations, aggregate memory and ROI remain separate gaps.

旧CelGlowは入力取得後の終了経路を一本化し、出力取得・パラメータ取得の失敗や
パニックでも入力を返却します。描画エラーを優先し、描画成功時の返却失敗は伝播します。
旧版の画像変換も形式・寸法・行幅・nullポインタを検査し、負の行幅を補正します。
入力変換の確保失敗をエラーで返します。旧版固有の色計算は変更していません。
アダプター側の出力・マスク・距離配列とぼかし作業領域も確保失敗をエラーで返します。
ぼかしは行・列ごとの配列確保をやめ、1本を再利用します。細長い画像や大きい半径は
直接平均との単体比較で検証しています。GPU内部の確保、総メモリ量、
部分領域の座標整合は別の残課題です。

CelGlow2 validates CPU world dimensions/stride/null pointers before SDK buffer access,
handles negative row orientation, and reserves input conversion storage fallibly.
Output dimensions and channel count must match before writing. Geometry unit tests
and22 normal headless checks pass;21 prior output hashes are unchanged. This does
not certify native malformed-pointer injection, ROI alignment or total memory limits.

CelGlow2はCPU画像アクセス前に寸法・行幅・nullポインタを検査し、負の行幅を補正します。
入力変換用メモリの確保失敗をエラーで返し、出力の寸法・チャンネル数も書き込み前に検査します。
単体検査とヘッドレス22項目が通過し、従来21画像は同一です。実AEへの不正ポインタ注入、
部分領域の座標整合、総メモリ上限まで検証したものではありません。

Both the legacy `rust/celglow` and newer `rust/celglow2` adapters are inspected
separately. The legacy crate has its own lockfile and is explicitly excluded
from the newer Rust workspace so its standalone locked build works.

Both adapters now obtain CPU pixel formats from the World Suite instead of
inferring depth from the Deep flag. Float worlds can also carry that flag;
treating them as 16-bit corrupted their output. Regression fixtures cover
8/16/32-bit no-effect output, black/transparent inputs, odd and portrait sizes,
and repeatability. CelGlow2 identity uses Result with Glow Opacity at zero:
its Input debug view displays the extracted source mask, not the original image.

These fixtures do not certify native AE GPU worlds, HDR range preservation,
same-instance cache lifetime, or all parameter combinations. Those remain
separate audit items; no MediaCore deployment is part of this audit.

After Effects SmartFX plug-in for illustration and anime-style compositing.
The current implementation is Rust-only and generates flat stepped hard-edge
glow bands from a luma or color mask. It uses a GPU compute path for source-mask
generation, approximate distance-field generation via JFA, and per-pixel
band/copy compositing when a GPU adapter is available.

## Current Implementation

- Source: `rust/celglow`
- Crate: `cdylib`
- AE SDK wrapper: `virtualritz/after-effects`
- PiPL: generated from `rust/celglow/build.rs`
- Effect menu: `Effects > onmk > CelGlow`
- Match name: `ANTH CelGlow`
- Render path: SmartPreRender / SmartRender
- Output artifact on Windows: `rust/celglow/target/release/celglow.dll`

For After Effects on Windows, install the release DLL as `CelGlow.aex`.

## Build

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow\rust\celglow
cargo build --release
```

## Install for Local AE Testing on Windows

Run PowerShell as Administrator if your After Effects plug-in directory requires
elevated permissions.

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow
.\scripts\install_dev.ps1
```

By default, the script copies:

```text
rust\celglow\target\release\celglow.dll
```

to:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\CelGlow.aex
```

You can override the install directory:

```powershell
.\scripts\install_dev.ps1 -InstallDir "D:\AePlugins"
```

## C++ Version

The old C++ plug-in tree has been removed. Keep only one CelGlow plug-in visible
to After Effects at a time, because duplicate plug-ins with the same match name
can cause AE to load only one of them.

## Notes

- CelGlow2 CPU adapter no longer reports arbitrary core errors/panics as a
  successful source passthrough. Allocation, invalid radius, bad buffer geometry
  and panic map to AE error codes. Only the existing clipped-ROI fallback remains.
  Classification unit tests are not native AE failure injection.
- CelGlow2 CPU/GPU adapter output arrays use checked RGBA/byte counts and
  fallible reservation. Invalid geometry returns an error before allocation;
  failed reservation returns OutOfMemory. This is not an aggregate memory cap.
- CelGlow2のCPU/GPU出力配列は要素数・バイト数を検査してから確保し、
  サイズ異常と確保失敗をエラーで返します。総メモリ量の上限ではありません。
- CelGlow2 clipped-ROI source fallback and GPU cropping copy the global-coordinate
  intersection, with transparent pixels outside it and 64-bit origin differences.
  Offset/extreme-coordinate unit tests cover this helper; native AE ROI remains unverified.
- CelGlow2のROI代替コピーとGPU切り出しは原点を考慮し、重なる部分だけを転送します。
  範囲外は透明にし、座標差は64bitで計算します。座標付き単体テストは通過していますが、
  実AEでの部分領域描画全体は未検証です。
- CelGlow2のCPU側は、コアの確保失敗やパニックを入力コピーの正常終了で隠さず、
  AEエラーへ変換します。従来のROI切り詰め時の代替処理だけを維持しています。
  エラー分類の単体テストであり、AE実機への障害注入ではありません。

- Shared v2 glow/roughen buffers reserve fallibly; roughening validates exact
  geometry before mutation, including its disabled path. It retains alpha and
  distance snapshots but updates RGBA in place, eliminating one full RGBA copy
  (31.6 MiB at 1920x1080 by allocation arithmetic, not measured process RSS).
- v2共有コアの合成・輪郭処理でも確保失敗を返し、不正な寸法では画素を書き換えません。
  不要なRGBA複製を除去し、1080pで計算上31.6 MiBの一時配列を削減します。
  実際のRSSや総メモリ上限は別途検証が必要です。

- Shared v2 `Field::build` now returns a typed error for malformed dimensions,
  input length, invalid/overflowing blur radius, or working-buffer allocation
  failure. CPU render, AE GPU render and CLI GPU preview propagate that error.
  Blur math remains zero-padded with the same summation order. Capacity-overflow
  tests do not simulate physical OOM; other core working allocations remain.
- v2共有コアの距離場構築は、寸法・入力長・ぼかし半径・作業領域確保の異常をエラーで
  返します。CPU描画、AEのGPU描画、CLIプレビューへ伝搬します。実メモリ枯渇や
  コア全体の確保処理を保証するものではありません。

- CelGlow2 GPU now transports Ring Phase and adds its degree/360 offset before
  ring selection, matching CPU semantics including negative/out-of-range bands.
  An isolated actual-GPU reference test covers -180, 0, 180, and 360 degrees.
- CelGlow2のGPUでもRing Phaseを反映します。負の位相と輪の範囲外を含め、
  −180・0・180・360度をCPU参照と実GPUで比較しています。

- CelGlow2 GPU core softness matches CPU at zero-width steps and clamps fade
  to the core level. Actual-GPU white-field tests reproduce and guard both
  boundaries with wobble/texture/unevenness disabled to isolate the calculation.
- CelGlow2のGPUコアぼかしは、幅ゼロの境界判定とコア値による幅の制限をCPUに揃えました。
  揺らぎ等を無効にした白入力で、両方の境界を実GPU比較しています。

- CelGlow2 GPU glow alpha now follows CPU compositing: maximum tinted RGB,
  combined with source alpha, unless glow-alpha output is disabled. Actual-GPU
  tests compare CPU compositing across three source alphas and two dimensions.
  Blend mode/placement fields are currently unused by CPU as well as GPU;
  those are shared implementation gaps, not GPU-only mismatches.
- CelGlow2のGPUアルファをCPUと同じ着色後RGBの最大値に揃えました。入力アルファ3種・
  2解像度で実GPU比較を通しています。合成モード・前後関係はCPUでも未使用のため、
  GPU固有ではなく共通の未実装として扱います。

- CelGlow2 GPU now rejects ring indices outside the configured count, matching
  the CPU's range test. An actual-GPU regression reproduced unwanted glow on
  transparent black at 100% line width before the fix and now matches CPU black.
  GPU feature parity (wobble, color modes, blend/placement and related controls)
  is still incomplete; this repair does not certify the draft path as equivalent.
- CelGlow2のGPUで輪の範囲外を除外し、線幅100%時に透明黒から余分な光が出る不具合を
  修正しました。実GPUでCPUの透明黒と一致しています。ただし揺らぎ・色モード・合成
  などのGPU対応は不完全で、描画全体の同値性は未達です。

- CelGlow2 readback reserves output fallibly before mapping, checks exact byte
  length, and decodes float bits without requiring mapped-pointer alignment.
  Scoped cleanup unmaps on success, returned error, and unwinding. Unit fault
  tests and actual-GPU normal transport pass; real device-loss/OOM injection
  and shared-core allocation safety remain separate gaps.
- CelGlow2の読み戻しは確保失敗・バイト数を検査し、成功・エラー・巻き戻し時に解除します。
  模擬異常系と実GPUの通常読み戻しは検証済みですが、実デバイス喪失・メモリ枯渇や
  共通CPUコアの確保処理までは検証していません。

- CelGlow2 GPU rejects empty/mismatched/overflowing frames and storage, buffer,
  or dispatch-limit violations before building its CPU field or GPU buffers.
  Tests cover dimension rejection and valid actual-GPU color transport. This
  is not allocator-exhaustion, aggregate memory, or CPU-fallback certification.
- CelGlow2のGPU入口は、空画像・入力長不一致・桁あふれ・GPU上限超過を距離場の構築前に
  拒否します。実際のメモリ枯渇、総メモリ上限、CPUフォールバック全体は未検証です。

- CelGlow2 GPU uniform layout now explicitly pads before the fill color to match
  WGSL's 16-byte vector alignment (fill offset48, alpha flag64, total96 bytes).
  A failure-first layout regression and an opt-in actual-GPU fill/alpha transport
  test cover the repair. This does not establish full CPU/GPU rendering parity.
- CelGlow2のGPUパラメータ配置を修正しました。色の開始位置をWGSLに合わせて48バイト目、
  アルファ設定を64バイト目に揃え、実GPUで色・アルファの伝達を検証しています。
  CPUとの描画全体の同値性を保証するものではありません。

- Legacy GPU cache retains at most eight buffer sets and 1 GiB of requested
  buffer bytes per processor. Old resolutions are removed before replacement;
  oldest entries are evicted before allocating, and a single oversized request
  returns to the existing CPU fallback. This is not a driver VRAM/RSS cap.
  An opt-in actual-GPU test checks small-budget eviction, entry count, resize,
  and oversize rejection across sequential worker threads, not concurrent MFR.
- 旧GPUキャッシュは最大8組・要求バッファ合計1 GiBまで保持します。解像度変更や
  上限超過時は新規確保前に古い組を除き、単体で上限を超える要求はCPUへ返します。
  実GPUテストは小さい予算で退避・組数・サイズ変更を確認します。ドライバのVRAMや
  RSS全体の上限ではなく、同時MFRの検証でもありません。

- Legacy CPU source-view and compositing coordinates widen to i64 before
  origin/plane arithmetic, preventing i32 wrap into the input at extreme origins.
  Output world layout is validated before working allocation. Unit tests cover
  extreme coordinate differences and normal-range equivalence; native AE ROI
  fixtures are still required to establish full ROI behavior.
- 旧CPU経路の入力表示・合成では、原点と入力領域の差を64bitで計算し、32bitの
  折り返しを防ぎます。出力レイアウトも作業領域確保前に検証します。極端な座標と
  通常範囲の同値性は単体検証済みですが、AE実機のROI全体の動作保証ではありません。

- Legacy GPU readback now propagates polling/mapping failures and unmaps on
  every work exit, including unwinding. The output allocation is fallible and
  mapped bytes are length-checked and decoded without a typed pointer cast.
  Cleanup fault tests use closures; they do not inject an actual device loss.
- 旧GPU経路の読み戻しは待機・マッピング失敗を返し、途中終了時もバッファを解除します。
  読み戻し先の確保失敗・バイト数も確認します。後始末の異常系テストは模擬処理であり、
  実デバイス喪失の検証ではありません。
- Both legacy GPU paths validate dimensions, source lengths, shader index and
  coordinate ranges, band count, and device storage/buffer/dispatch limits before
  buffer creation. Unsupported requests return to the existing fallback path.
  Tests cover boundary rejection and valid actual-GPU renders after rejection;
  actual driver memory and real allocation failures remain unverified.
- 旧GPUの両経路で、確保前に寸法・入力長・座標と添字・帯数・デバイス上限を検査します。
  範囲外は既存のフォールバックへ返します。拒否後の実GPU正常描画も検査していますが、
  ドライバの実メモリ使用量や実際の確保失敗は未検証です。

- Legacy CelGlow has an opt-in actual-GPU regression test,
  `gpu_single_seed_distance_and_resize` (`cargo test ... -- --ignored`).
  It checks both uploaded CPU distances and GPU JFA against analytic single-seed
  distances across six resize steps, including one-pixel-wide and odd frames.
  This does not establish multi-seed JFA exactness, transformed-copy parity,
  complete glow compositing parity, or GPU memory/failure safety.
- 旧CelGlowには実GPU必須の距離場テストがあります。1点光源について、細長い画像・
  奇数サイズを含む6回のサイズ切替を数式の正解と比較します。複数光源、変形コピー、
  合成結果の完全一致やGPUメモリ・障害時の安全性までは保証しません。

- CelGlow2 CPU output retains finite signed/HDR RGB at 32 bpc; non-finite
  values are still sanitized and alpha is bounded during premultiplication.
  The integer output policy is unchanged. Adapter regression tests cover both
  boundaries; this does not certify native GPU-world HDR transport or change
  the legacy CelGlow blend modes.

- The Rust version follows the same `PathArrayRust` pattern:
  `SmartPreRender`, `SmartRender`, `checkout_layer(0, 0)`, and pre-render data.
- The current feature set is generated 1-8 band glow, color source mode, basic
  copies, blend controls, GPU compute, and debug views.
- Band opacity is flat across each band for a storybook/cel-like look; only the
  overall glow start/end edges are softened, so adjacent bands do not create
  transparent gaps.
- `Luma Softness` affects the source-mask boundary used by the distance field.
- `Source Box Blur` softens the source mask before distance-field generation for
  a gentler storybook-style glow.
- `Band Count`, `Glow Start`, `Total Size`, and `Size Mode` generate adjacent
  band ranges automatically, so outer boundaries follow the previous band
  without exposing per-band radius controls.
- GPU compute falls back to CPU if initialization or execution fails.
- Planned work such as matte sources, presets, custom UI, and exact GPU EDT
  remains tracked in `docs/`.
