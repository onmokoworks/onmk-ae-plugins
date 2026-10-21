# EdgeSmith

EdgeSmith is a distance-field edge designer for Adobe After Effects. It is intended as a modern, deterministic alternative to Roughen Edges rather than a parameter-for-parameter clone.

## Features

- Signed-distance based expansion and erosion
- Multi-octave animated edge noise
- Independent edge balance, direction, stretch, sharpness, and detail protection
- Nearest-color propagation into newly expanded alpha
- 8/16/32-bpc CPU rendering
- wgpu compute path (Metal, Vulkan, or DX12) with automatic CPU fallback
- Smart Render, Multi-Frame Rendering, and expanded ROI support

## Regression checks

CPU pixel access queries the explicit World Suite format rather than inferring
16 bpc from the Deep flag. This fixes half-written 32 bpc output. The monorepo's
`tools/quality_smoke.py --profile edge-smith` covers black/transparent inputs,
odd and portrait sizes, repeatability, Classic/Smart agreement and zero-Mix
identity at 8/16/32 bpc. These checks do not certify native AE GPU-world access.

`cargo test --release` includes a GPU smoke test. With `-- --nocapture`, it
reports whether shader execution/readback ran or the adapter/device was unavailable.
Once a device exists, rendering errors fail the test rather than silently passing.

Float output preserves signed HDR RGB when premultiplying; only integer output
is clipped to its normalized range. Alpha remains bounded to 0–1. Regression
tests cover this write boundary and CPU/GPU zero-Mix agreement for partially
transparent HDR pixels. PNG-based host fixtures alone do not prove HDR input
transport or general CPU/GPU edge-shape equivalence.

## Build commands

The opt-in `correction_1080p_timing` test measures only RGB correction on a
1920x1080 rectangular mask, one warm-up and five repetitions. Removing the
duplicate alpha plane saved7.91MiB of requested scratch; observed means were
26.826ms before and27.598ms after. This is a memory reduction, not evidence of
a speedup or an end-to-end AE performance result.

専用の1080p補正ベンチでは、アルファ複製除去の前後で平均26.826ms／27.598msでした。
計算上7.91MiBの一時配列削減はできますが、高速化やAE全体の性能向上を示す結果ではありません。

```sh
./scripts/build_macos_release.sh
```

On Windows, run `scripts/build_release.ps1`. The packaged artifact is written under `target/release/`.

## GPU implementation

The reproduced5x5 four-corner RGB tie mismatch is repaired by restoring propagated
RGB from the existing CPU seed traversal after GPU readback. GPU alpha/noise is
unchanged; CPU rendering uses the same extracted traversal. Images containing
near-zero alpha need one CPU field pass and12 bytes/pixel of temporary arrays
(23.73MiB at1080p, arithmetic not measured RSS), allocated fallibly. Images without
such alpha skip this correction. The symmetric fixture now requires zero RGB
differences; four distinct-color patterns also match CPU with alpha untouched.

四隅の色からCPUが黄、GPUが青を選んでいた差は、GPU読み戻し後に補完RGBだけを
既存CPUの走査で揃えることで修正しました。GPUのアルファ・ノイズは変更しません。
ほぼ透明な画素がある場合はCPU走査と一時配列が増え、1080pで計算上約23.73MiBを要します。
確保失敗はエラーで返し、透明部分がない画像では補正を省きます。対称配置のRGB差はゼロとなり、
別の4配置でもCPUの色と一致し、補正前のアルファを維持することを検査しています。

GPU distance evaluation now uses the CPU's8-neighbor chamfer metric instead of
Euclidean distance. The reproduced single-seed alpha discrepancy0.263944 vs0.635474
is repaired without changing CPU output. Actual-GPU tests compare an analytical
single seed and48 multi-seed size/border cases against CPU alpha within3e-6.
These isolate noise-free alpha; arbitrary JFA seed selection, ties and noisy RGB
equivalence remain unverified. This is not full CPU/GPU parity certification.

A separate17x13 rectangular-mask test covers16 combinations of Seed0/17,
Evolution0/123.5, Direction0/37 and Stretch-99/99 with five noise octaves.
Observed maximum CPU/GPU alpha error is8.53e-6 on the tested GPU, below the
2e-4 regression threshold. It does not cover arbitrary geometry, RGB ties or
all parameter combinations; production noise math was unchanged.

GPUの距離をCPUと同じ縦横・斜めの近似距離へ変更し、再現したアルファ約0.264対0.635の差を
修正しました。CPUの出力は変えていません。実GPUで1点の解析値と、複数点・寸法・Borderの
48条件を比較しています。ノイズなしのアルファを対象とした検査であり、任意の点配置、
同距離の色選択、ノイズ込みの全面的な一致は未検証です。
別途、17×13の矩形でSeed・Evolution・角度・Stretch上下限の16条件を比較し、
ノイズ付きアルファの最大差は約0.0000085でした。任意形状や全パラメータ、
同距離のRGB選択まで保証するものではありません。この検査で本体の計算は変更していません。

GPU color propagation now uses the CPU's near-zero alpha threshold rather than
the0.5 inside/outside geometry threshold. A2x1 red/partially-transparent HDR-blue
fixture reproduced neighboring color replacement even at Mix0; CPU-reference GPU
tests cover Mix0/0.5/1 after the repair. Distance-field approximation differences
are separate from this color bug and remain unverified in general.

GPUが半透明画素の色を隣の画素で置き換えてしまう問題を修正しました。
輪郭判定のアルファ0.5ではなく、CPUと同じほぼ透明の閾値で色を補います。
赤と半透明のHDR青の2画素で、Mix0でも色が変わる不具合を再現し、
修正後はMix0・0.5・1をCPUと比較しています。距離場の近似差は別の残課題です。

Readback reserves host memory before GPU buffer creation and validates the mapped
byte count before decoding floats without an alignment-dependent cast. A scoped
guard unmaps after the mapped view drops, including errors and unwinding. Tests
cover unaligned bytes, signed/HDR/NaN bit preservation and cleanup ordering;
these are not actual device-loss or physical OOM injection.

読み戻し先はGPU処理前に確保し、返却バイト数を検査します。アラインメントに依存せず
floatのビットを復元し、エラー・パニック時も参照を破棄してからマッピングを解除します。
符号付きHDR・NaNのビット保持と解除順序は単体テストで確認します。
実デバイス喪失や実際のメモリ枯渇への障害注入ではありません。

Before buffer creation, GPU rendering checks exact RGBA length, checked byte counts,
storage/buffer/dispatch limits and the shader's signed neighbor-coordinate range.
Unsupported geometry returns an error for the adapter's CPU fallback. Boundary
tests cover invalid lengths/overflow and valid rendering after rejection; these
do not prove native AE GPU-world support, aggregate memory limits or device-loss safety.

GPUバッファ作成前にRGBA長・容量・デバイス上限・距離計算の整数範囲を検査します。
扱えない寸法はエラーで返し、アダプターのCPU経路へ戻します。異常寸法の拒否後も
通常描画できることを検査しています。実AEのGPU画像形式、総メモリ上限や
デバイス喪失への安全性は別途検証が必要です。

PreRender expansion uses checked coordinate arithmetic and rejects inverted
rectangles or overflowing expansion before checkout. Normal padding is unchanged;
unit tests cover all four coordinate endpoints and padding 0/8/1024. Native host
ROI behavior and nonfinite parameter policy remain separate validation gaps.

PreRenderの領域拡張で整数オーバーフローや逆転した矩形を検出し、画像取得前に拒否します。
通常の余白量は変更せず、四辺の整数端点と余白0・8・1024を単体テストで確認しています。
実ホストのROI動作と非有限パラメータの扱いは別の検証対象です。

Classic/Smart output crops share checked dimensions and fallible allocation.
Origin differences are computed in i64 before subtraction; only intersecting row
spans are copied, leaving the rest transparent. Unit tests cover positive/negative
offsets, opposing i32 limits, short source and empty/overflow output dimensions.
This does not certify native ROI checkout; PreRender bounds are tested separately.

出力の切り出しでは寸法と配列長を検査し、座標差を64bitで計算します。
交差する行だけをコピーし、範囲外は透明に保ちます。正負の位置ずれと整数の端点、
不正な寸法を単体テストで確認しています。実AEのROI取得やPreRenderの拡張処理は別の検証対象です。

World conversion checks dimensions, signed-stride overflow, minimum row length,
addressable extent and null data before acquiring a flat SDK buffer. Output shape
and channel count must match; input float allocation is fallible. Unit checks cover
short/padded/negative rows and arithmetic limits, not actual host allocation faults.

画像変換前に寸法・行幅・アドレス範囲・データポインターを検査し、出力の寸法と
配列長の不一致を拒否します。入力変換用メモリの確保失敗もAEへ返します。
境界値は単体テストで確認していますが、実ホストでのメモリ枯渇試験ではありません。

Signed row-stride mapping now preserves logical row order independently for input
and output. A regression reproduces the former upside-down mixed-orientation copy;
36 depth/height/orientation combinations check distinct rows and untouched padding.
These use the adapter row mapping and codecs, not native AE world fault injection.

入力と出力で別々に行方向を補正します。従来の処理で上下が逆になる条件を再現し、
8/16/32bit・高さ1/3/7・入出力の向きを組み合わせた36条件で、行順と余白の保持を
検査しています。実AEの画像オブジェクトへの障害注入ではありません。

Native integer codec tests call the same decode/encode helpers as host rendering.
All 16-bit RGB codes 0..32768 round-trip exactly at opaque alpha; additional
alpha values 1,129,16384 exercise every valid channel value up to that alpha.
All nonzero 8-bit alpha values and corresponding channel values also round-trip.
This proves codec precision, not the entire filter, ROI, signed row-stride or native host contract.

実描画と共通の変換関数で、16bitの全RGB値0～32768の往復一致を確認しています。
アルファ1・129・16384と、8bitの全非ゼロアルファ値でも有効な色値を検査します。
これは変換精度の検証であり、フィルター全体・ROI・負の行幅・実AEの保証ではありません。

Smart Render returns an acquired input exactly once after success, error or a caught
render panic. A secondary checkin error no longer hides the original rendering
error; checkin failure is reported when rendering succeeded. Six injected closure
combinations cover this precedence, not actual native host callback fault injection.

Smart Renderは描画の成功・エラー・捕捉したパニックのいずれでも、取得済み入力を
一度返却します。返却も失敗した場合、元の描画エラーを優先します。描画が成功した
場合は返却エラーを報告します。6条件の単体テストで確認しており、実AEでの障害注入ではありません。

The GPU renderer builds inside/outside distance fields using Jump Flood passes, then evaluates the same deterministic fractal edge model as the CPU renderer. `SmartRenderGpu` is advertised only when GPU Acceleration is enabled. If adapter creation or shader execution fails, the render falls back to CPU.
