# ONMK Starglow

複数方向の光条とスペクトル色を生成する、Adobe After Effects用の実験的なスターグローエフェクトです。

## 主な機能

- Star／Cross／X／水平／垂直などの光条形状
- 方向別の長さ調整
- Threshold、Softness、Decay、Boost
- カラーマップとSpectrum
- Shimmer
- Luminance Map
- Source／Starglow OpacityとTransfer Mode
- `Affect Alpha` による透明領域への光の拡張

`Affect Alpha` を有効にすると、元レイヤーのアルファ外へ伸びた光も合成結果に残ります。無効にすると従来どおり元画像のアルファを維持します。

## ビルド

CPU描画は入力サイズ・画素数・マップ長を検査し、作業配列の確保失敗をエラーとして返します。
光芒の走査順は配列を作らず算出します。マップ変換の確保も同様に扱います。
容量オーバーフローはテスト済みですが、実際のOSメモリ枯渇は注入していません。
Classicのマップ取得・変換エラーとPreRenderの取得エラーは上位へ返し、未指定の場合だけマップなしで描画します。
未指定・取得失敗・変換成功・変換失敗の4条件を区別する注入テストを追加しました。

Smart Renderは、描画エラーやpanicでもマップ→入力の順に画像を返却します。
マップ取得エラーを「未指定」として握りつぶさず、後処理のエラーより元の描画エラーを優先します。
6条件の注入テストで返却順・各1回の返却・元のエラー／panicの保持を確認しています。
実AEのコールバック失敗やメモリ枯渇を注入した検査ではありません。

模様付き7×11マップと、同じ最近傍規則で17×31へ事前拡大したマップも比較しました。
8/16/32bpc・Classic/Smart・反転有無の24描画と12比較が通過し、出力はバイト単位で一致します。
これはサイズ変換の整合性の検査であり、光芒全体の独立した正解画像、ROI、ネイティブ深度のマップ入力の保証ではありません。

AEXCompatで7×11の黒／白Luminance Mapを17×31入力に指定し、8/16/32bpc・Classic/Smartの
30描画と6比較を確認しました。Source Opacity 0では黒マップのRGBが0になり、白マップは未指定時と
完全一致します。Invert Mapで両者が入れ替わることも確認済みです。一様マップなので、空間的な補間・ROI・
ネイティブ16bitマップ入力の精度まで保証する検査ではありません。

GPU転送用の入力・マップ・出力配列は、確保失敗をエラーとして既存のCPUフォールバックへ
返すようにしました。入力変換配列は送信後に解放し、読み戻しバッファのunmapはスコープ終了時に
実行します。不正な長さ・サイズのオーバーフローを拒否し、16bit全階調と負値/HDRの変換を検査済みです。
実際のメモリ枯渇・デバイス喪失は注入しておらず、CPU側の確保やドライバー内部のメモリは別の検査対象です。

GPUのグラデーション終点が最後の色ではなく1つ手前の色を返す不具合を修正しました。
色の区間を確定してから補間率を計算し、終点では最後の色を選びます。
カラーマップ10種とShimmer有無の20条件を実GPUとCPUで比較し、差が0.000002未満と
確認しています。Spectrumも色相境界・周回・密度・色のランダム性・Shimmerを組み合わせた
96条件でCPUとの差が0.000002未満でした。これは17×9の内部GPU検査であり、全解像度・
全ノイズ・全合成設定や実AEの保証ではありません。

Shimmer有効時のGPUが、区間内の減衰率を正しく掛け合わせずCPUと異なる結果になる
不具合を修正しました。既存の作業バッファの未使用成分に区間の減衰率を保持するため、
追加バッファは不要です。Shimmer強度・位相を含む72条件でCPUとの差が0.000002未満と
確認できました。任意のノイズ設定・カラーマップ・全解像度の一致まで保証するものではありません。

GPUが見かけの光芒長で計算を打ち切り、CPUの減衰した裾と一致しない不具合を修正しました。
方向ごとの画面内の長さまで計算し、極小の光を除く判定もCPUとそろえています。
63×35の複数光源で、光芒4種・長さ5/19/41・マップ有無の24条件を実GPUとCPUで比較し、
差が0.000002未満であることを確認しています。任意のカラーマップ・全解像度の
一致や速度改善を保証する検査ではありません。

内部GPUキャッシュは最大8スレッド分、要求バッファ容量の合計1GiBまで保持します。
新規確保前に古いものとサイズ変更前のバッファを解放し、単体で上限を超える場合は
既存のCPUフォールバックへ返します。小さなテスト用上限で12回のスレッド交代と
8bit／float切り替えを行い、実バッファサイズと容量計算、出力保持を確認しています。
ドライバー・パイプラインのメモリや解放待ちは含まず、実VRAMの上限保証や並行MFRの検査ではありません。

2026-10-04のヘッドレス検査で、未指定のLuminance Mapを黒画像として扱うSmart Renderの不具合と、16／32bitを8bitとして読み書きする不具合を修正しました。CPUと内部GPU処理の両方でネイティブ階調を保持し、floatの負値・HDR値も維持します。GPUで未実装だった合成モードとAffect AlphaもCPUの式に合わせました。

AEXCompatの31件と単体テスト11件が通過しています。実GPUでは16bitの全32,769段階、微弱な光芒、HDR、解像度・深度切り替え、短い光芒の13合成モードを検査しました。既存の8bit検査20件は修正前と画像ハッシュが一致しています。これは全条件の保証ではありません。マップ指定時・ROI、長い光芒やShimmerのCPU/GPU差、デバイス喪失・メモリ上限、AEのネイティブGPUワールドは引き続き検査対象です。AEXCompatの深度別入力はPNG8由来であり、下位ビットの保持は別の実GPUテストで確認しています。

```powershell
cargo build --release
```

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

Fixed the GPU gradient endpoint returning the penultimate color at t=1. Clamp
the segment before calculating its fraction, matching the CPU endpoint. Ten
gradient/preset choices with shimmer off/on give20 actual-GPU/CPU comparisons
below2e-6. Another96 Spectrum comparisons cover hue boundaries/wrap, density,
random hue offsets and shimmer, also below2e-6. These use17x9 internal-GPU
fixtures, not all resolutions/noise/compositing settings or native AE certification.

Fixed spatially varying shimmer propagation: prefix-scan segments now carry the
product of per-pixel decay in the existing unused working-buffer component.
No additional GPU buffers are allocated. Extending the CPU comparison with three
shimmer amount/phase pairs gives72 cases below2e-6. Arbitrary noise settings,
colormaps and resolutions remain outside this bounded proof.

Fixed GPU streak-tail truncation: visual length controls decay, not the scan extent.
Prefix passes now cover the image along each direction, and the negligible-streak
cutoff matches CPU. A 63x35 multi-source regression covers four shapes, lengths
5/19/41 and map off/on: all24 actual-GPU/CPU comparisons stay below2e-6 without
widening tolerance. Arbitrary colormaps/resolutions and performance remain
separate; additional tail passes are required where the old calculation stopped early.

The internal GPU cache retains at most eight worker states and 1 GiB of requested
buffer storage. Replaced and least-recently-used states are dropped before allocation;
an oversized frame returns the existing CPU fallback. Actual-GPU tests cover twelve
successive workers with byte/float switching, reduced limits, exact buffer-size
accounting and preserved output. Driver/pipeline memory and pending device frees are
excluded: this is not measured VRAM or simultaneous MFR certification.

ONMK Starglow is an experimental spectral star-glow effect for Adobe After Effects with multi-direction rays, color maps, threshold controls, shimmer, and glow compositing.

CPU rendering validates dimensions, exact source/map lengths and reserves its
workspaces fallibly. Streak traversal computes indices without index vectors;
map conversion also reserves fallibly. Capacity overflow is tested, not actual
OS memory exhaustion. Classic map checkout/decode and PreRender checkout errors
now propagate; only an absent layer means no map. Four injected cases distinguish
absence, checkout failure, successful decoding and decoding failure.

Smart Render returns checked-out map/input pixels in reverse order on errors
and unwinding. Map checkout errors propagate instead of silently disabling the
map; the original work error or panic takes precedence over cleanup errors.
Six injected helper cases verify order, exactly-once cleanup and error/panic
preservation, not native AE callback failures or actual memory exhaustion.

Spatial 7x11 maps and equivalent nearest-expanded 17x31 maps produce identical
raw output: 24 renders and 12 comparisons pass across three depths, both render
paths and inversion states. This checks resize equivalence, not an independent
full-glow model, ROI transport or native deep map-input precision.

Selected7x11 black/white luminance maps against17x31 input pass30headless renders
and6role comparisons across8/16/32bpc and Classic/Smart. Black suppresses glow RGB
with source opacity zero; white is byte-identical to no map, and inversion swaps
the roles. Constant PNG8-derived maps do not certify spatial resampling, ROI or
native deep map-input precision.

GPU source/map/output transport vectors now reserve fallibly and return errors to
the existing CPU fallback. Source conversion storage is dropped after upload;
readback unmapping is scope-bound, after the mapped view is dropped. Tests cover
invalid lengths, capacity overflow, all nominal u16 codes and signed/HDR float
transport. Actual memory exhaustion/device loss, CPU allocations and driver-owned
storage remain outside this validation; no measured memory or speedup claim.

The 2026-10-04 audit fixed absent-map handling and byte-only 16/32-bit host
transport. CPU and internal GPU paths retain native precision, including signed
HDR float RGB. Missing GPU blend modes and Affect Alpha now follow CPU formulas.
All 31 headless fixtures and 11 unit tests pass; 20 existing byte fixtures remain
hash-identical. Actual GPU tests cover all 32769 nominal u16 levels, weak glow,
HDR, format/size switches, and 13 blend modes on a short streak. PNG8-derived
host fixtures alone do not establish native low-bit input precision.
Explicit maps/ROI, long-streak and shimmer parity, device loss, memory bounds,
and AE native GPU-world integration remain open; this is not full certification.

`Affect Alpha` extends output alpha with the generated glow so rays remain visible outside the source layer's original alpha. Disable it to preserve source alpha.
