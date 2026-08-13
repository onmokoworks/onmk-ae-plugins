# CelGlow — 設計書 (v0.1 ドラフト) 【凍結: `design-v2.md` で置き換え】

> **2026-07-10: 本書の EDT 距離場 + バンド方式は廃止。**
> 現行の設計は `docs/design-v2.md` (ブラー等高線リング + 筆質感) を参照。

> `docs/requirements.md` v0.2 (2026-04-23 確定) を受けた実装設計。
> アルゴリズムの疑似コード / データレイアウト / GPU シェーダ骨格 /
> カスタム UI のヒット判定までを詰める。実装直前に v0.2 へ上げる。

---

## 0. 用語

| 略 | 意味 |
| --- | --- |
| **W/H** | 入力レイヤのピクセル幅 / 高さ |
| **S** | `min(W, H)` — 半径% のスケール基準 |
| **D[p]** | 源マスクからピクセル `p` までのユークリッド距離 (px) |
| **LUT** | 256 エントリの `float` 1D テーブル (バンドの減衰カーブをベイク) |
| **ECW** | Effect Controls Window — AE のパラメータパネル |

---

## 1. データモデル

### 1.1 Band と Curve

```cpp
// src/Bands.h
namespace celglow {

struct CurvePoint {
    float x, y;              // 0..1
    float inTanX, inTanY;    // -1..+1 (bezier handle relative)
    float outTanX, outTanY;
};

struct Curve {
    uint8_t  numPoints;          // 2..8
    CurvePoint points[8];
    uint8_t  presetId;           // 0=Flat,1=Linear,2=Smooth,3=EaseIn,4=EaseOut,5=SCurve,0xFF=Custom
    // 256 LUT は別途キャッシュ (serialize しない)
};

struct Band {
    float   innerRadiusPct;  // 0..200
    float   outerRadiusPct;  // innerRadius 以上を強制、0..200
    float   rgba[4];         // linear, premul なし
    float   opacity;         // 0..1
    float   edgeSoftnessPx;  // 0..8
    Curve   falloff;         // per-band (v0.2 で合意)
    uint8_t enabled;         // 1 byte
    uint8_t _pad[3];
};

struct BandList {
    uint8_t numBands;  // 1..16
    Band    bands[16];
};

} // namespace celglow
```

`Band` は 32-bit float 揃えで約 `4+4+16+4+4 + (1+8*24+1+3) = 228` B。
16 個で `~3.6 KB`。

### 1.2 ArbitraryData 直列化 (TLV)

AE のプロジェクトファイル跨ぎ永続化、コピー/ペースト、undo に対応する
ため、**エンディアン非依存の TLV 形式**で直列化。

```
magic     : 'CGB0' (4B)
version   : uint16 LE = 1
flags     : uint16 LE = 0 (reserved)
payloadLen: uint32 LE
payload   : TLV streams

TLV:
  tag : uint16 LE
  len : uint32 LE
  val : len bytes

tags:
  0x0001 BANDS_V1      — numBands + Band[] (エンディアン正規化、下記参照)
  0x0080 END           — len=0 (optional termination)
```

Band のフィールドは IEEE 754 float を **リトルエンディアン** で書く。
読み出し側でホストエンディアンに変換する。未知 tag は skip。

未知バージョン: `version > 1` は拒否し、AE にエラーを返す
(ユーザにプラグインアップデート促す)。

### 1.3 LUT キャッシュ

LUT は `Curve` のハッシュ (FNV-1a 32bit) をキーに
`std::unordered_map<uint32_t, std::array<float, 256>>` でキャッシュ。
最大 64 エントリ、LRU。

LUT ベイク:

```cpp
void bakeLUT(const Curve& c, float out[256]) {
    for (int i = 0; i < 256; ++i) {
        float t = i / 255.0f;
        out[i] = evalCurve(c, t);  // bezier 区分評価
    }
}
```

### 1.4 パラメータの揮発性

`ArbitraryData` は AE のシーケンス間で保持されるが、実行時に再計算できる
(LUT、ソースハッシュ、距離場バッファ、etc.) は全て **SequenceData** に
置く。`SequenceSetup / SequenceResetup / SequenceFlatten / SequenceSetdown`
で扱う。SDK の `AEFX_SuiteScoper` を活用。

---

## 2. パイプライン詳細 (SmartFX)

### 2.1 `PF_Cmd_SMART_PRE_RENDER`

```cpp
// CelGlow.cpp (抜粋、戻り値エラーハンドリング省略)
static PF_Err PreRender(PF_InData* in, PF_OutData* out, PF_PreRenderExtra* extra) {
    ParamsSnapshot p = snapshotParams(in, extra);

    // 1. 出力 rect = 入力 rect + バンド到達距離 + コピー分の変位
    PF_Rect req = in->layer->bounds;
    const float reach = maxReachPx(p);    // Σ(outerRadius × globalScale × perCopyScaleMax) × √2
    req = expand(req, std::ceil(reach + positionJitterPx(p)));

    extra->cb->checkout_layer(in->effect_ref, kParam_Input, kParam_Input, &req, in->current_time, in->time_step, in->time_scale, &in0);
    if (p.src.useMatte)
        extra->cb->checkout_layer(in->effect_ref, kParam_SrcMatteLayer, kParam_SrcMatteLayer, &req, ...);

    extra->output->result_rect = req;
    extra->output->max_result_rect = req;
    return PF_Err_NONE;
}
```

### 2.2 `PF_Cmd_SMART_RENDER`

```cpp
static PF_Err SmartRender(PF_InData* in, PF_OutData* out, PF_SmartRenderExtra* extra) {
    auto p = snapshotParams(in, extra);

    // 0. checkout
    PF_EffectWorld *inW, *matteW = nullptr;
    extra->cb->checkout_layer_pixels(..., kParam_Input, &inW);
    if (p.src.useMatte) extra->cb->checkout_layer_pixels(..., kParam_SrcMatteLayer, &matteW);
    PF_EffectWorld *outW; extra->cb->checkout_output(..., &outW);

    // 1. float32 RGBA に昇格
    FloatImage inF  = upcast(inW);
    FloatImage matF = matteW ? upcast(matteW) : FloatImage{};

    // 2. Source mask 生成
    FloatImage1 src = buildSourceMask(inF, matF, p);    // §3

    // 3. EDT (Felzenszwalb 2-pass)
    FloatImage1 dist = edt(src);                          // §4

    // 4. バンド LUT を準備 (キャッシュヒット or bake)
    std::array<const float*, 16> lut = getBandLUTs(p);

    // 5. コピー変換行列 M_i を生成 (決定論乱数込み)
    std::vector<Copy> copies = buildCopies(p, W, H);     // §6

    // 6. 合成: CPU SIMD or GPU
    FloatImage glow = zerosRGBA(W, H);
    if (gpuAvailable()) gpuRender(glow, src, dist, lut, copies, p);
    else                cpuRender(glow, src, dist, lut, copies, p);

    // 7. Global Blend with input + Amount + PreserveAlpha
    compose(glow, inF, p, outW);
    return PF_Err_NONE;
}
```

---

## 3. Source Mask 生成

出力 `src[p] ∈ [0, 1]`。3 方式を OR/AND 系で合成。

### 3.1 Luma 閾値
```
for each p:
    Y = 0.2126 R + 0.7152 G + 0.0722 B    // Rec.709 linear
    t  = p.src.lumaThreshold / 255
    sw = p.src.lumaSoftness / 255
    s_luma[p] = smoothstep(t - sw, t + sw, Y)
```

### 3.2 Matte
```
chan = {Luma, Alpha, R, G, B} のうち指定
s_matte[p] = invert ? (1 - v) : v
```

### 3.3 Color
```
HSL 変換後、Source Color (H0, S0, L0) との距離:
    dH = min(|H-H0|, 1 - |H-H0|)      // ring distance 0..0.5
    dS = |S - S0|
    dL = |L - L0|
    d  = √((dH×2)² + dS² + dL²)       // 0..√3
    s_color[p] = smoothstep(tol, 0, d)  // tol は 0..tolerance%
```

### 3.4 Combine
```
enabled = [useLuma, useMatte, useColor]
values  = [s_luma, s_matte, s_color]   (disabled は無視)
switch Combine Mode:
    Max:       src = max_enabled
    Add:       src = clamp(Σ enabled, 0, 1)
    Intersect: src = min_enabled
```

Source Dilation はチェックボード分離カーネルで実装。
正ならダイレート、負ならエロード、サイズ ±4 px までだが
2-pass (水平 + 垂直) で O(W×H × kernelSize)。

### 3.5 2 値化

EDT に入れる前に `src[p] > 0.5` を seed とする。源のソフトネスは
EDT の前処理で保持するが、距離計算は 2 値から始める(Felzenszwalb の
入力は 2 値 or 距離初期値)。ソフトエッジ情報はバンド評価時の
`edgeSoftness` に回す。

---

## 4. 距離変換 (EDT)

### 4.1 アルゴリズム: Felzenszwalb & Huttenlocher (2012)

1D EDT を 2 段 (垂直 → 水平) に適用する O(W×H) アルゴリズム。

```cpp
// 無限大初期化
for each p: f[p] = src[p] > 0.5 ? 0.0f : +INF;

// 垂直 pass: 各列に対し parabola envelope を維持
for x in [0..W):
    dist1d(f.col(x), g.col(x));

// 水平 pass
for y in [0..H):
    dist1d(g.row(y), d2.row(y));

// 結果は d² なので最終的に sqrt
for each p: dist[p] = std::sqrt(d2[p]);
```

`dist1d` は標準実装 (Felzenszwalb のレクチャーノート通り)、
`v[]` (放物線の最小点), `z[]` (交点), `k` (有効放物線数) の三変数で
O(N) ループ。SIMD 化は難しいがレーンごと並列 (TBB-like task)
でスレッド並列化は容易。

### 4.2 SIMD / GPU

- CPU: 垂直 pass を列ごと、水平 pass を行ごとにタスク化。
  macOS は Grand Central Dispatch の `dispatch_apply`、
  Windows は `PPL` (Concurrency::parallel_for)、
  抽象化は `ParallelFor(N, [&](int i){...})` で自前 thin wrapper。
- GPU: Jump Flood Algorithm (JFA) を使う方が GPU 向け。
  v1 は CPU で EDT、v1.1 で GPU JFA 置換を検討。
  JFA は精度が落ちるが glow の見た目では無視できる。

### 4.3 レイヤ外の扱い

Source rect を出力 rect に拡張済み。レイヤ bounds の外は
`src = 0` 扱い、`dist = +INF` に初期化。バンド評価で
`dist > outer*scale` は無視されるので計算は無駄だが、
矩形でメモリアクセスは速い。

---

## 5. バンド評価

### 5.1 距離 → バンド帰属

バンドが重なる場合も扱う (v1 は **重なり OK**、下から順に加算)。

```cpp
float4 bandColor(int bandIdx, float d_px, const Band& b, const float lut[256]) {
    float inner = b.innerRadiusPct / 100.0f * S * globalScale;
    float outer = b.outerRadiusPct / 100.0f * S * globalScale;
    if (outer <= inner) return float4(0);
    float t = (d_px - inner) / (outer - inner);
    if (t < 0 || t > 1) return float4(0);
    // エッジ softness: t が 0 や 1 に近いとき AA
    float aaIn  = smoothstep(0,  b.edgeSoftnessPx / (outer-inner), t);
    float aaOut = 1 - smoothstep(1 - b.edgeSoftnessPx / (outer-inner), 1, t);
    float a = lut[int(t * 255)] * b.opacity * aaIn * aaOut;
    return float4(b.rgba.rgb, b.rgba.a) * a;
}
```

### 5.2 全バンド合成

```cpp
float4 acc = 0;
for i in [0..numBands):
    if !bands[i].enabled: continue;
    acc = blendAdd(acc, bandColor(i, d, bands[i], lut[i]));
```

バンド同士は **Add** で重ねる(v1 固定、v1.1 で切替検討)。
これは「内側バンドの色が外側バンドに漏れない」という直感より
「重なりで光量が増える」の方が自然なため。

---

## 6. コピー変換 / ランダム

### 6.1 変換モデル

各コピー `i ∈ [0, copies)` に対し、**源マスク空間** の座標変換を作る。
グロー計算はコピーごとに繰り返し、結果を加算(Blend Copies に従う)。

パラメータから導かれるコピー `i` の変換 `T_i`:

```
θ_i = i × rotStep + jitter_i(θ)       // 回転 (rad)
s_i = pow(scaleStep, i) × jitter_i(s) // スケール
origin = userOrigin + jitter_i(pos)

// 位置分布: mode に応じて
if Radial:  c_i = origin + 0  (源は共通、回転で散らす)
if Ring:    c_i = origin + (cos(i × 2π/copies + θ_0), sin(...)) × ringRadius
if Linear:  c_i = origin + lineVector × i

// 最終アフィン: 座標 p (出力空間) → 源空間 q へ:
q = T_i^{-1}(p) = R(-θ_i) × (p - c_i) / s_i
```

**重要**: 距離場 `D` は源の空間で 1 枚だけ計算。各コピーは
`q` を使って `D[q]` を bilinear サンプリング、バンド評価に入れる。

### 6.2 決定論乱数

```cpp
uint32_t hash32(uint32_t seed, uint32_t idx, uint32_t salt) {
    uint32_t x = seed ^ (idx * 0x9E3779B1u) ^ (salt * 0x85EBCA77u);
    x ^= x >> 16; x *= 0x7FEB352Du;
    x ^= x >> 15; x *= 0x846CA68Bu;
    x ^= x >> 16; return x;
}

float uniform01(uint32_t seed, uint32_t idx, uint32_t salt) {
    return (hash32(seed, idx, salt) & 0xFFFFFF) / float(0x1000000);
}
```

`salt` は `{0: rot, 1: scale, 2: hue, 3: opacity, 4: posX, 5: posY}`。
`Animate over Time` ON 時は `seed' = seed ^ hashTime(in->current_time)`。

### 6.3 コピー単位の色相シフト

RGB → HSL → H を `hueShift_i = i * hueStep + jitter_i(hue)` 分回転 →
RGB に戻して LUT に反映。色相処理は **LUT ベイク時** に行い、
コピーごとに専用 LUT を持つ(メモリ: 16 コピー × 16 バンド × 1024 B
= 256 KB)。実装上は `LUT[copy][band]` の 2D 配列。

---

## 7. 合成

### 7.1 Blend Copies

```cpp
float4 blend(float4 a, float4 b, BlendMode m) {
    switch m:
        Add:    return a + b;
        Screen: return 1 - (1 - a) * (1 - b);
        Normal: return a * (1 - b.a) + b;
}
```

Add は clamp しない(HDR を許容、最後の composite で clamp)。

### 7.2 Global Blend / Amount

```cpp
glow *= amount;   // 0..2.0
out = blend(input, glow, globalBlend);
if (preserveAlpha) out.rgb *= input.a;  // 厳密には out.a = input.a も
```

### 7.3 Output View

Debug 用 popup:
- Result — 通常出力
- Glow only — `out = glow` (入力 0)
- Source only — `out = input`
- Distance — `out = vec3(dist / maxRange)` grayscale
- Mask — `out = vec3(src)` grayscale

---

## 8. GPU 実装

### 8.1 プラットフォーム抽象

```cpp
// src/gpu/GpuDispatcher.h
struct GpuBackend {
    virtual bool init() = 0;
    virtual bool uploadMask(const FloatImage1& src) = 0;
    virtual bool runEDT() = 0;                          // v1.1 JFA
    virtual bool uploadLUTs(const LUTSet& luts) = 0;
    virtual bool render(const ParamsSnapshot& p, FloatImage& out) = 0;
};
// Metal / DirectCompute 実装を ifdef で分岐
```

v1 は **EDT だけ CPU / 合成を GPU** の分担。EDT は 1080p で
10..20 ms、合成は GPU 1..3 ms。

### 8.2 Metal シェーダ骨格

```metal
// CelGlow.metal
#include <metal_stdlib>
using namespace metal;

struct Copy {
    float2 center;
    float  cos_th, sin_th;
    float  inv_scale;
    float4 hueRotMat;   // 色相回転はシェーダ内で O(4) MAD
};

kernel void celglow_render(
    texture2d<float, access::sample> distTex    [[texture(0)]],
    texture2d<float, access::sample> lutAtlas   [[texture(1)]],  // 16 copies × 16 bands × 256 pixels, 1D stacked
    constant Band*  bands    [[buffer(0)]],
    constant Copy*  copies   [[buffer(1)]],
    constant Params& p       [[buffer(2)]],
    texture2d<float, access::write>  outTex    [[texture(2)]],
    uint2 tid [[thread_position_in_grid]])
{
    if (tid.x >= p.W || tid.y >= p.H) return;
    float2 pix = float2(tid);
    float4 acc = 0;
    for (uint c = 0; c < p.numCopies; ++c) {
        Copy C = copies[c];
        float2 q  = rot(C.cos_th, -C.sin_th, pix - C.center) * C.inv_scale;
        float  d  = sampleLinear(distTex, q);
        for (uint b = 0; b < p.numBands; ++b) {
            Band B = bands[b];
            if (!B.enabled) continue;
            float4 col = bandColor(d, B, lutAtlas, c, b, p);
            acc = blend(acc, col, p.blendCopies);
        }
    }
    outTex.write(acc, tid);
}
```

スレッドグループは 16×16、M2 Pro で 1080p ~2ms。

### 8.3 HLSL (DirectCompute)

構造は Metal と 1:1 対応、記述は HLSL SM 5.1。
`StructuredBuffer<Band>`, `Texture2D`, `RWTexture2D`。
シェーダのコードは `gpu/CelGlow.hlsl` に別管理、
`CelGlow.metal` との **単一真実のソースを持たない**
(ビルド対象 OS のみコンパイル)。アルゴリズム変更時は両方手動同期、
`scripts/check_shader_parity.py` で定数・構造体のサイズ比較テスト。

### 8.4 LUT テクスチャ配置

`256 × (copies × bands)` の 2D テクスチャ 1 枚 (R16F)。
`y = copy * numBands + band` でアクセス。
16×16 で 256 行 × 256 列 = 128 KB (16bit float 1 ch)。

---

## 9. カスタム UI (ArbitraryData + drawbot)

### 9.1 レイアウト

ECW 内での描画は `PF_Event_DRAW` 内で AE の **drawbot API**
(`PF_SupplyDrawbotSuite1` 経由) を使う。OS 直描画は避ける。

```
+--------------------------------------------------------+
| Band List Pane (60%)          | Curve Editor Pane (40%)|
| [1] [innerR▶][outerR◀] [●]    |     ╱──╲              |
| [2] [        ][        ] [●]   |    ╱    ╲             |
| [3] [        ][        ] [●]   |   ╱      ╲____        |
| [+][-]      [↑][↓]            | [Flat][Lin][Smo][S][Ease]|
|                                | [Apply to all]          |
+--------------------------------------------------------+
```

縦は 180 px 固定、高 DPI で自動 2x。

### 9.2 ヒット判定

`PF_Event_DO_CLICK` で座標を受け取り、領域定義:

```cpp
struct HitTest {
    enum Kind { None, BandRow, BandInnerHandle, BandOuterHandle,
                BandColorSwatch, BandReorderGrip, AddBand, RemoveBand,
                CurvePoint, CurveInTan, CurveOutTan, CurveBg,
                PresetFlat, ..., ApplyToAll } kind;
    int idx;   // band index or curve point index
};

HitTest hit(Point p, UiState& ui) {
    if (inBandList(p)) {
        int row = (p.y - listTop) / rowHeight + ui.scrollOffset;
        if (row >= ui.bandList.num) return {AddBand, 0};
        float localX = (p.x - listLeft) / listW;
        if (localX < 0.08) return {BandReorderGrip, row};
        if (localX < 0.15) return {BandColorSwatch, row};
        // 半径範囲スライダ
        float inner = ui.bandList.bands[row].innerRadiusPct / 200;
        float outer = ui.bandList.bands[row].outerRadiusPct / 200;
        if (abs(localX - inner) < 0.02) return {BandInnerHandle, row};
        if (abs(localX - outer) < 0.02) return {BandOuterHandle, row};
        return {BandRow, row};
    }
    if (inCurveEditor(p)) { ... }
    return {None, 0};
}
```

### 9.3 ドラッグ

`PF_Event_DRAG` を受けて、直前の `HitTest` 状態 + マウス差分で更新。
内外半径のクロスは `inner ≤ outer` を enforce。

ドラッグ開始時に snapshot、ドラッグ中は ArbitraryData を
**一時バッファ** に書き、`PF_ChangeFlag_CHANGED_VALUE` を都度立てて
AE にプレビューさせる。ドロップ時に最終値を「正」として
`AEGP_SetArbitraryDataValue` 相当を呼ぶ(undo ポイントは 1 つ)。

### 9.4 undo / redo

AE の `PF_Cmd_USER_CHANGED_PARAM` が arbitrary data にも来るので
値変化のたびに AE が undo stack に push する。細かすぎる変化を
防ぐため、**ドラッグ中は AE 側 undo を抑制** する工夫が必要。
具体的には:

- ドラッグ開始時に `PF_UTIL_BEGIN_NO_UNDO_MODE` (疑似、SDK サンプル参照)
- ドラッグ終了時に `END_NO_UNDO_MODE` + 最終値を 1 つの undo 単位で commit

実装詳細は SDK の `ArbSample/` サンプルを読み込んでから確定(TODO)。

### 9.5 高 DPI

drawbot API は論理座標。OS の DPI スケールは AE が吸収してくれる
(ハズ)。実機確認項目 (リスク §8):

- Retina 2x / 3x
- Windows 125% / 150% / 200%

---

## 10. ロギング / エラー

- `~/Library/Logs/CelGlow/CelGlow.log` / `%LOCALAPPDATA%\CelGlow\CelGlow.log`
- **エラー時のみ書き込み** (通常パスで IO しない)。
- 書き込みは `std::ofstream` を 1 MB 超えたらローテート、最大 4 世代。
- スレッドセーフにするため `std::mutex` 1 個 + バッファリング。

エラーコード:
- `CG_ERR_ARB_VERSION_TOO_NEW` (プロジェクトが新バージョンで保存)
- `CG_ERR_GPU_INIT_FAILED`
- `CG_ERR_OUT_OF_MEMORY`
- `CG_ERR_INVALID_SOURCE`

AE 側には `PF_Err_OUT_OF_MEMORY` / `PF_Err_INTERNAL_STRUCT_DAMAGED`
などにマップして返す。

---

## 11. テスト戦略

### 11.1 ユニットテスト

`tests/` ディレクトリに Catch2 (header-only) で配置、
AE 依存のない純アルゴリズム部のみ対象:

- `test_edt.cpp` — 2値入力 vs リファレンス実装 (naive O(N²))
- `test_curve.cpp` — プリセットの LUT 値がリファレンス値と一致
- `test_arbdata.cpp` — 直列化 → 復元で bit 一致、エンディアン逆転テスト
- `test_random.cpp` — 決定論性 (同 seed で同値)、分布の均一性

### 11.2 インテグレーションテスト

- 固定プロジェクト (.aep) を `aerender` で書き出し、
  ゴールデン PNG と ΔE で比較。
- プリセット 4 種 × 8bpc/16bpc/32bpc × CPU/GPU で
  24 ケース生成。
- macOS / Windows で同一出力を確認 (ΔE < 1)。

### 11.3 手動テスト (チェックリスト)

受け入れ時の手動チェックは requirements.md §7 と同じ。

---

## 12. ビルド / 依存

### 12.1 CMake 構成

```
plugin/
  CMakeLists.txt
  cmake/
    FindAfterEffectsSDK.cmake
    CelGlowMacApp.cmake     # bundle生成
    CelGlowWinApp.cmake     # .aex 生成 + PiPL
```

トップ `CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.24)
project(CelGlow CXX)
set(CMAKE_CXX_STANDARD 17)
find_package(AfterEffectsSDK REQUIRED)

add_library(CelGlow MODULE
    src/CelGlow.cpp src/Source.cpp src/DistanceField.cpp
    src/Bands.cpp src/Copies.cpp
    src/ui/BandEditor.cpp src/ui/CurveEditor.cpp
    src/gpu/GpuDispatcher.cpp
    # Metal / HLSL は CMake add_custom_command でビルド
)
target_link_libraries(CelGlow PRIVATE AfterEffectsSDK::Base)
if(APPLE)
  target_link_libraries(CelGlow PRIVATE "-framework Metal" "-framework MetalKit")
  set_target_properties(CelGlow PROPERTIES BUNDLE TRUE BUNDLE_EXTENSION "plugin")
elseif(WIN32)
  set_target_properties(CelGlow PROPERTIES SUFFIX ".aex")
  # PiPL: Rez or python → .rsrc embed (FillLine 流儀)
endif()
```

### 12.2 PiPL のバージョン

```
AE_Effect_Version { 0x8001 }  // 0.1 DEVELOP build 1 == (1<<15) | 1
// feedback_ae_pipl_version.md 参照
```

リリースごとに `PF_VERSION(maj, min, bug, stage, build)` を手計算して
ここに書く。FillLine でこの罠を踏んでいるので要注意。

### 12.3 Metal / HLSL のビルド

- Metal: `xcrun metal` + `xcrun metallib` でコンパイル、`.metallib`
  を Resources に埋め込み。実行時に `newDefaultLibrary` でロード。
- HLSL: Windows では `fxc` or `dxc` で `.cso` を生成、
  `.rc` に埋め込み。

CMake の `add_custom_command(OUTPUT ... COMMAND ...)` で統一。

---

## 13. 未決 TODO (design から実装前までに詰める)

- [ ] drawbot API の実使用サンプル (SDK `ArbSample/`) を読み込み、
      ヒット判定 / drag state の詳細パターンを確定。
- [ ] ArbitraryData の flatten (Paramsize 制限) と現実サイズの照合。
      AE 側のサイズ上限を確認、必要なら外部ファイル参照に切替。
- [ ] EDT の SIMD 最適化(まず素朴実装で M2 測定、15ms 超えたら優先)。
- [ ] GPU EDT (JFA) を v1.1 送りで OK か、開発速度次第で再判断。
- [ ] HSL→OKLab 化検討(色相シフトの質。LayerGlow と揃えるか)。
- [ ] プリセット JSON 同梱方式: Effect > Animation Preset に変換するか、
      プラグイン内蔵の popup 選択肢として持つか。

---

## 14. 次のアクション

1. 本書の方針に OK / 修正指示をもらう。
2. OK 後、`plugin/` スケルトンを作成(CMake + Empty SmartFX + PiPL
   + パラメータ登録骨格、AE で "onmk > CelGlow (stub)" が出るところまで)。
3. その後 M1 の EDT プロトタイプ / LUT ベイク検証へ。
