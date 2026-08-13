# CelGlow v2 — 設計仕様書「筆致リンググロー」

> 2026-07-10 起草。AE 上で組んだリファレンスチェーン
> (Fast Box Blur → S_GlowRainbow → Curves → UnMult → Turbulent Displace →
> Roughen Edges ×2 → プリコンポーズ → Fractal Noise → Solid Composite →
> UnMult → Fill → 加算/不透明度) を単一エフェクトとして再現する。
>
> **v1 (EDT 距離場 + フラットバンド方式、`design.md` / `requirements.md`) は
> 本書で置き換える。** v1 の見た目 (均等なハードエッジバンド) と本リファレンス
> (ブラー等高線に沿った手描き風リング) はアルゴリズムの根本が異なるため、
> **v2 は完全に別プラグインとして開発する**: 旧 `rust/celglow` は凍結して
> 一切変更せず、新 crate 群 (`celglow-core` / `celglow-cli` / `celglow2`) を
> 新設する。match name も `ANTH CelGlow2` に変え、旧 v1 と同時インストール
> 可能・旧プロジェクト非干渉とする (§3, §9)。SmartFX 骨格 / wgpu 基盤 /
> ビット深度変換などのインフラは v1 実装から**コピーして**流用する。
>
> 2026-07-10 サブエージェントレビュー (Critical 3 / Major 7 / Minor ほか)
> を反映済み。

---

## 0. リファレンスチェーンの分析と対応表

リファレンス画像:

1. 最終出力 — 元素材の周囲に、筆でなぞったような同心リングのグローが
   多数重なって出る。リングは不均一な明るさで、ところどころ色ズレ
   (虹色のフリンジ) があり、境界はザラついた筆・鉛筆質感。
2. 入力 — 透明背景上のカラフルなストローク群。
3. Fast Box Blur 後 — 入力が大きくボケて滑らかな「密度の山」になる。
4. S_GlowRainbow (+Curves) 後 — ボケた山の**等高線**に沿った多数の
   同心リング。山頂 (輝度が飽和した中心) は黒く抜け、
   リングは中心寄りが明るく外側へ向かって減衰する。
5. Turbulent Displace + Roughen Edges ×2 後 — リングがぐにゃぐにゃと
   うねり、線が粒状に切れて筆・チョーク質感になる。

ここから導かれる本質: **リングはブラー後の輝度場のレベルセット
(等高線) である。** 距離場 (EDT) ではない。ブラー場の勾配が急な所は
リング間隔が詰まり、なだらかな所は広がる — これが「手描きの等高線」
らしさの正体で、EDT の等距離リングでは出ない。

| AE エフェクト | 役割 | プラグイン内の対応 (ステージ) |
| --- | --- | --- |
| Fast Box Blur | 滑らかな密度場を作る | S1: ソース場 F(p) (box blur ×3 反復) |
| S_GlowRainbow | 等高線状の多重リング | S3: リング波 `wave(F)` + 包絡線 |
| Curves (RGB ぐちゃぐちゃ) | リングごとの明るさ不均一 + 色ズレ | S3: リング番号ハッシュによる per-ring / per-channel ゲイン |
| UnMult | 黒→透明 | 内部表現 (輝度=α で保持、明示処理不要) |
| Turbulent Displace (Bulge 14 / 24) | リングのうねり | S2: サンプリング座標のノイズ変位 (ドメイン歪み) |
| Roughen Edges (Cut, scale12) | 線を粒状に切る | S4: グレイン A (減算 + 2値化) |
| Roughen Edges (Roughen, scale10, border2.2) | エッジの削れ | S4: グレイン B (エッジ侵食) |
| Fractal Noise + Solid Composite + UnMult | 全体のムラ (大域変調) | S5: 低周波 fBm による乗算マスク |
| Fill | 最終着色 | S6: カラーモード |
| 不透明度 / 加算 | 元素材との合成 | S6: Blend + Opacity + Placement |

---

## 1. 全体パイプライン

```
入力 RGBA (float32 に昇格)
  │
  ├─ S1  ソース場生成:  strength(p) → box blur ×3 → F(p) ∈ [0,1]
  │       (Source Color モード用に blur した色 C_blur(p) も併産)
  │
  └─ 最終パス (出力ピクセル p ごと、1 パスで S2〜S6):
       S2  変位:      q = p + wobble(p)
       S3  リング波:  f = F(q) → 位相 φ → 線プロファイル w
                       × per-ring ゲイン × 包絡線 E → I_rgb
       S4  筆テクスチャ: グレイン A (cut) → グレイン B (roughen)
       S5  大域ムラ:  × (1 − U·fbm_macro(p))
       S6  着色・合成: カラーモード適用 → 入力と Blend
```

重い処理はブラーのみ。S2〜S6 はブラー場テクスチャ 1 枚 (+色 1 枚) を
読むだけの純関数なので、GPU では 1 コンピュートパスに収まる。

---

## 2. ステージ仕様

以降、`S = min(W, H)`、`u01()` は決定論ハッシュ乱数 (§7)。
すべて f32 で計算するが、**色空間変換は行わない** — core は渡された値の
空間 (AE の作業色空間) のまま処理する。リファレンスチェーンは非リニアの
通常プロジェクトで組まれた見た目なので、既定の見た目・チューニング定数は
**ガンマ空間前提**で確定させる。リニア作業色空間プロジェクトでの見え方の
差は既知の制限とする (§12)。

座標系: 全ステージの評価座標 p は**レイヤ座標** (フル解像度、レイヤ原点
基準)。バッファ相対座標は使わない — AE の出力要求 rect は ROI やズームで
フレームごとに変わるため、バッファ相対で評価するとノイズ模様が泳ぐ。
core API は入力・出力それぞれの `origin` (バッファ左上のレイヤ座標) を
受け取り、`p_layer = output_origin + p_out` で評価する。入力参照時は
`p_in = p_layer - input_origin` に変換する (`mock-harness.md` §2)。

### S1: ソース場 F(p)

1. **強度抽出**
   ```
   strength(p) = channel_select(input):
       Alpha:        A
       Luma:         Y = 0.2126R + 0.7152G + 0.0722B   (straight RGB)
       Luma × Alpha: Y × A                              ← 既定
   strength = clamp01(pow(strength × srcGain, srcGamma))
   ```
   注意: `Luma` は α を無視するため、透明領域に隠れた straight RGB の
   ゴミを拾い得る。既定を `Luma × Alpha` にしているのはこのため。
2. **ブラー** — 半径 `r = Spread / 3` の box blur (separable H+V) を
   **3 反復** ≈ ガウシアン。総到達距離 ≈ Spread px。
   エッジは clamp-to-zero (レイヤ外 = 0)。
   - 品質最適化: `Quality = Draft` 時は 1/4 解像度、`Normal` 時は 1/2
     解像度でブラーし bilinear アップサンプル。場は滑らかなので
     見た目の劣化はほぼ無い。`Best` はフル解像度。
3. **場の整形** — `F(p) = clamp01(pow(blurred, fieldGamma))`。
   fieldGamma でリングの分布重心を内外に寄せられる。
4. Source Color モード用: 同じブラーを premultiplied RGB にも適用し
   `C_blur(p)` (正規化: α で除算、α≈0 は白フォールバック) を得る。

### S2: 変位 (Turbulent Displace 相当)

出力座標 p を歪ませてから F をサンプルする。後段のリングは F から
導かれるため、「リング画像に Turbulent Displace をかけた」のと同じ
効果になる。

```
n2(p) = ( fbm(p/S_w + o1, oct_w),  fbm(p/S_w + o2, oct_w) )   // 各 ∈ [-1,1]
q     = p + A_w · n2(p)
f     = bilinear_sample(F, q)
```

- `A_w` = Wobble Amount (px, 既定 3)、`S_w` = Wobble Scale (px, 既定 24)
- `oct_w` = Wobble Complexity (1..4, 既定 1)
- `o1, o2` は seed と Wobble Evolution から導くオフセット。
  Evolution は fbm の第 3 入力 (3D ノイズの z)。value noise は z の
  整数シフトで元に戻らないため**ループ再生は仕様外** (連続進化のみ)。
  ループが必要になったら z 方向周期格子 + Cycle パラメータを v2.1 で検討。
- fbm は正規化済みで厳密に [-1,1] (§7)。したがって変位の最大は
  ±A_w px で、rect 拡張 (§4.1) は WobbleAmount そのままで足りる。

### S3: リング波 + 包絡線 + 不均一化

```
// Curves と同じく、ぼかし場の最大値を入力白点として正規化する
f_norm = clamp01(F(q) / max(F))

// 中空コア: f_norm が高い(=ソース中心)領域はリングを消す
fade  = min(fade_core, L_core)       // クランプ: 下端が負になり全域減光するのを防ぐ
core  = smoothstep(L_core − fade, L_core + fade, f_norm)   // 1 = コア内
u     = clamp01(f_norm / L_core)     // 0 = 遠方, 1 = コア境界

// 位相: コア境界で 0、外へ向かって増加 (phase = Ring Phase, 360°=1リング)
φ = N_rings · pow(1 − u, γ_dist) + phase
k = floor(φ)                         // リング番号 (整数)
t = φ − k                            // リング内位置 0..1

// 線プロファイル (アンチエイリアス幅は位相の解析勾配から。CPU/GPU 共通)
d      = |t − 0.5|
halfW  = lineWidth / 2                              // duty 比
aa     = max(0.5 · |∇φ|_px, (1 − hardness) · halfW)
w      = 1 − smoothstep(halfW − aa, halfW + aa, d)
// リング密集域 (aa > halfW) では被覆率が duty 比でなく 0.5 に収束して
// 明るさが跳ねるため、duty 比へブレンド補正する:
w      = mix(w, lineWidth, clamp01(aa / halfW − 1))

// per-ring / per-channel ゲイン (Curves ぐちゃぐちゃ相当)
  g_base   = 1 − variation · u01(bitcast_u32(k), seed, 0)
  g_c(ch)  = g_base · (1 − scramble · (u01(bitcast_u32(k), seed, 1+ch) − 0.5))
                                                              // ch = R,G,B

// 包絡線: 外側へ pow で減衰、コアで消す
E = pow(u, γ_falloff) · (1 − core)

I_ch = w · g_c(ch) · E        // チャンネルごとのリング強度
```

備考:

- `scramble > 0` のとき R/G/B のゲインがリングごとに割れて
  リファレンスの虹色フリンジが出る。`scramble = 0` なら純モノクロ。
- **AA 勾配の規約 (CPU/GPU parity のため固定)**:
  `|∇φ| = N·γ_dist·(1−u)^(γ_dist−1) / L_core · |∇F|`。
  `∇F` は **CPU/GPU とも** F テクスチャの中心差分を**変位後座標 q** で
  評価する。wobble のヤコビアン項 (∂q/∂p) は AA 目的では無視する
  (aa の下限項 `(1−hardness)·halfW` が吸収)。
  `fwidth` はコンピュートシェーダに存在しないため**使わない**。
- `Ring Phase` をアニメすると、リング全体の形を保ったまま
  リングが外側 (または内側) へ流れる脈動表現ができる。
- Ring Phase は負値を取り得るため k は signed i32 とする。乱数キーへ渡す
  ときだけ 2 の補数ビット列をそのまま u32 に bitcast する
  (Rust: `k as u32`、WGSL: `bitcast<u32>(k)`)。剰余や絶対値は取らない。
- リング番号 k は場 F に対して大域なので、複数の光源ブロブが
  あってもリングの明滅パターンは矛盾なく繋がる (リファレンス通り)。
  ただし k は phase を含む floor なので、Ring Phase アニメ中は
  per-ring ゲインが 1 リング分ずつ流れる (仕様とする)。

### S4: Alpha Roughen (Roughen Edges 相当)

S2〜S5 で得た Glow Only の premultiplied RGBA を一旦バッファ化し、alpha境界を
フラクタルで**内側へ侵食**する。RGB強度へノイズを掛ける方式は採らない。

```
A(p)     = max(glow.r, glow.g, glow.b)       // glow alpha
d(p)     = distance_to_outside(A > 1/255)    // alpha境界からの内側距離 px
n(p)     = clamp01(fbm(p / RoughenScale, complexity, evolution) · 0.5 + 0.5)
erosion  = RoughenInfluence · n              // Cut は n を高コントラスト化
cover    = smoothstep(erosion - aa, erosion + aa, d)
A'(p)    = A(p) · cover                      // d <= Border の範囲だけ適用
rgb'(p)  = rgb(p) · cover                    // premultiplied を維持
```

- `Border` はalpha境界から内側へ処理する最大幅。帯の中央は保たれる。
- `Roughen` は連続的に侵食し、`Cut` はノイズを閾値化して欠けを強くする。
- `Edge Sharpness` が高いほど侵食境界はジャギジャギで硬くなる。
- グローだけを処理し、元素材RGBはS6の合成まで変更しない。
- S4は入力checkout全体で評価してから要求rectへcropする。ROI境界を偽のalpha境界として
  削らないためである。

### S5: 大域ムラ (Fractal Noise + Solid Composite 相当)

```
h  = smoothstep(0.25, 0.75, clamp01(fbm(p / S_U, oct_U) · 0.5 + 0.5))
M  = 1 − U · h
I₃ = I₂ · clamp01(M)
```

- `U` = Unevenness Amount (0..1, 既定 0.60)、`S_U` = 既定 500 px、
  `oct_U` = 3。Evolution 対応。
- 十分に低周波なので「筆圧が場所によって違う」ように見える。既定は
  1080pフレームより大きいスケールとし、局所的な粒ではなく大きなムラにする。

### S6: 着色・合成

```
// カラーモード
Fill:          rgb = fillColor.rgb · I₃_mono      (I₃_mono = max(I₃.rgb))
               ただし scramble による色割れを残すため
               rgb = fillColor.rgb · I₃.rgb を既定とする
Inner/Outer:   rgb = mix(outerColor, innerColor, u) · I₃.rgb
Rainbow:       hue = k / N_rings · rainbowCycles + hueOffset
               rgb = hsv2rgb(hue, rainbowSat, 1) · I₃_mono
Source Color:  rgb = C_blur(q) · I₃.rgb

glow.a   = clamp01(max(I₃.r, I₃.g, I₃.b)) · opacity
glow.rgb = rgb · opacity                      // premultiplied

// 合成域の規約: AE の入力は straight alpha。
//   1. input を premultiply する
//   2. premul 空間で合成する (Normal/over は premul 必須。
//      Add は premul/straight で結果が変わるため premul 側で統一)
//   3. 出力前に unpremultiply して straight で書き戻す
in_p = premul(input)
if placement == Behind:
    out_p = over(in_p, blend_sub(glow))   // グローが下。Add/Screen は可換
                                          // なので Behind でも適用され、
                                          // Normal のみ over 順序が変わる
else /* In Front */:
    out_p = blend(in_p, glow)             // Add / Screen / Normal
out = unpremul(out_p)

if !writeGlowAlpha: out.a = input.a     // RGB は straight のまま変更しない
```

- **Behind + Blend Mode の意味論**: Add と Screen は可換なので
  Placement に関わらず同じ式を適用する。Normal のみ「glow が input の
  下 (Behind) / 上 (In Front)」で over の順序が変わる。
  死にパラメータの組合せは存在しない。
- Add は clamp しない (32bpc HDR 許容、8/16bpc 変換時に clamp)。
  unpremul の α≈0 は 0 除算ガード (rgb=0)。
- 中心の中空化は S3 の `Core Level / Core Softness` で行う。評価前に
  `f_norm = F(q) / max(F)` と正規化して、Curves の入力白点をぼかし場の最大値へ
  合わせる。これにより Spread や入力の絶対輝度に関わらず、最も明るい中心を
  曲線の白点として黒へ落とせる。
- Glow Alpha がOFFなら、合成後のstraight RGBを維持したままαのみ入力値へ戻す。
  ONならグロー帯のalphaも出力へ書き込む。RGBにinput.aを再乗算しない
  （それを行うとstraight出力規約に反する）。
- View (デバッグ popup): `Result / Glow Only / Field / Rings (raw) /
  Texture Mask / Source Strength`。開発中の各ステージ検証にそのまま使う。

---

## 3. パラメータ定義

グループ構成と既定値。ordinal は AE のストリーム位置なので
**リリース後は末尾追加のみ** (現行実装の教訓を踏襲)。
v2 はパラメータ全面刷新であり、**match name を `ANTH CelGlow2` に変更した
別プラグインとして出す** (エフェクト名: `onmk > CelGlow 2`、
成果物: `CelGlow2.aex`)。同一 match name のままだと旧プロジェクトの
ストリーム値が ordinal 順で新パラメータに黙って再マッピングされ、
エラーなしで壊れた設定になるため。旧プロジェクトでは v2 のみ導入時に
「エフェクト欠落」と明示表示され、v1 と並行インストールも可能。

### Source
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 0 | Source Channel | popup | Alpha / Luma / Luma × Alpha | Luma × Alpha |
| 1 | Source Gain | slider % | 0..400 | 100 |
| 2 | Source Gamma | slider | 0.2..3.0 | 1.0 |
| 3 | Spread | slider px | 0..500 | 260 |
| 4 | Field Gamma | slider | 0.2..3.0 | 1.0 |

### Rings
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 5 | Ring Count | slider int | 1..64 | 8 |
| 6 | Ring Distribution | slider | 0.3..3.0 | 1.0 |
| 7 | Line Width | slider % | 5..95 | 30 |
| 8 | Line Hardness | slider % | 0..100 | 80 |
| 9 | Core Level | slider % | 10..100 | 85 |
| 10 | Core Softness | slider % | 0..50 | 10 |
| 11 | Outer Falloff | slider | 0..4.0 | 0 |
| 12 | Brightness Variation | slider % | 0..100 | 0 |
| 13 | Color Scramble | slider % | 0..100 | 0 |
| 14 | Random Seed | slider int | 0..65535 | 1 |
| 15 | Ring Phase | angle | — | 0° |
| 15a | Outermost Ring | checkbox | 最外周バンドを描画 | OFF |

Ring Phase: 360° = 1 リング分。アニメすると形を保ったままリングが
内外へ流れる (アニメ撮影の脈動表現用。ordinal 凍結前に入れる)。

### Wobble (Turbulent Displace 相当)
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 16 | Wobble Amount | slider px | 0..100 | 3 |
| 17 | Wobble Scale | slider px | 2..200 | 24 |
| 18 | Wobble Complexity | slider int | 1..4 | 1 |
| 19 | Wobble Evolution | angle | — | 0° |

### Alpha Roughen (Roughen Edges 相当)
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 20 | Edge Type | popup | Roughen / Cut | Cut |
| 21 | Border | slider px | 0..50 | 4 |
| 22 | Fractal Influence | slider px | 0..50 | 2.5 |
| 23 | Roughen Scale | slider px | 2..200 | 4 |
| 24 | Edge Sharpness | slider | 0..20 | 20 |
| 25 | Roughen Complexity | slider int | 1..4 | 2 |
| 26 | Roughen Evolution | angle | — | 0° |

### Unevenness (Fractal Noise 相当)
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 27 | Unevenness Amount | slider % | 0..100 | 0 |
| 28 | Unevenness Scale | slider px | 50..20000 | 500 |
| 29 | Unevenness Evolution | angle | — | 0° |

### Color & Composite
| # | 名前 | 型 | 範囲 | 既定 |
| --- | --- | --- | --- | --- |
| 30 | Color Mode | popup | Fill / Inner-Outer / Rainbow / Source Color | Fill |
| 31 | Fill Color | color | — | 青灰 |
| 32 | Inner Color | color | — | 白 |
| 33 | Outer Color | color | — | 水色 |
| 34 | Rainbow Cycles | slider | 0.25..8 | 1 |
| 35 | Rainbow Saturation | slider % | 0..100 | 70 |
| 36 | Glow Opacity | slider % | 0..200 | 75 |
| 37 | Blend Mode | popup | Add / Screen / Normal | Add |
| 38 | Placement | popup | Behind Source / In Front | Behind Source |
| 39 | Glow Alpha | checkbox | OFF: ソースalpha維持 / ON: グローalphaを書込 | ON |
| 40 | Quality | popup | Draft / Normal / Best | Normal |
| 41 | View | popup | Result / Glow Only / Field / Rings / Texture Mask / Source | Result |

> **実装上の互換メモ (2026-07):** AE の既存プロジェクトを壊さないため、実装済みのパラメータ ordinal は変更・中間挿入しない。現行バイナリは 1-based slot 1..45（enum ordinal 0..44）で、旧設計表の Quality/View (#40/#41) に対して `Source Threshold` / `Threshold Softness` を slot 43/44、`View` を slot 45 に末尾追加している。今後の追加も必ず末尾に append し、並び替えが必要な場合は新しい match name/version として扱う。

UI 上は `ae::GroupDef` (Source / Rings / Wobble / Brush Texture /
Unevenness / Output) でツイスト折り畳みにする。ArbitraryData /
カスタム UI は **v2 では不要** (v1 設計の主要リスクを丸ごと回避)。

---

## 4. SmartFX 統合

### 4.1 PreRender

グローの到達距離 (downsample 補正後の px):

```
reach = ceil( Spread' + WobbleAmount' + GrainErodeScale' + 4 )
```

(box blur ×3 の総到達は Spread、変位は fbm 正規化済みで ±WobbleAmount、
グレイン侵食のマージン、+AA 余白。`'` は §4.4 の downsample 補正済み値)

rect は方向を混同しないよう 3 式で定義する:

```
入力 checkout rect  = extra.output.request.rect を reach 拡張
max_result_rect     = (入力の max_result_rect) を reach 拡張
result_rect         = ((入力の result_rect) を reach 拡張) ∩ request.rect
```

- **v1 実装からの変更点**: v1 は入力の result_rect をそのまま返しており、
  グローのはみ出し領域が result_rect に含まれず切れるバグを抱えている。
  v2 では必ず「入力 result + reach」を基準にする。
  `set_returns_extra_pixels(true)` を併せて設定する。
- `checkout_layer(Input, 拡張 rect)`。Matte レイヤは v2 スコープ外。
- `pre_render_data` に ParamsSnapshot (全パラメータの数値化 +
  downsample 係数 + バッファ原点) を格納。v1 のパターンを流用。

### 4.2 SmartRender

1. 入力 checkout → float32 RGBA 昇格 (現行の 8/16/32bpc 変換路を流用)。
2. GPU 利用可なら GPU パス (§6)、失敗時 CPU パス (§5)。
3. 結果を出力ビット深度に変換して checkout_output へ。

### 4.3 フラグ

v1 と同じ: SmartFX / float color / threaded rendering /
flattened sequence data。`PF_OutFlag2_I_USE_3D_CAMERA` 等は不要。
`NON_PARAM_VARY` は立てない — 全出力がパラメータ由来で、
時刻そのものには依存しない (Evolution もパラメータ経由)。

### 4.4 プレビュー解像度 (downsample) 補正

AE のプレビュー解像度を落とすと SmartRender には縮小フレームが来る。
補正しないと半解像度でグローが相対的に 2 倍太くなり、ノイズ模様も
別物になる (v1 実装はこの穴を持っている。v2 では必須対応)。

- `in_data.downsample_x/y` を core の `Params.downsample: (f32, f32)`
  に渡す。
- **全 px 単位パラメータ** (Spread, Wobble Amount/Scale,
  Grain Cut/Erode Scale, Unevenness Scale) に downsample 係数を乗算。
- **ノイズ・φ の評価座標はフル解像度レイヤ座標**で行う:
  `p_full = (origin + p_buf) / downsample`。
  これで「Quality/解像度を変えても模様が変わらない」(§10) が
  AE のプレビュー解像度切替にも適用される。
- CLI は downsample = 1.0 固定。

---

## 5. CPU 実装

crate 構成は `mock-harness.md` §2 を正とする (**v1 とは完全分離**。
旧 `rust/celglow` は凍結し、v2 は新規 crate で開発する):

```
rust/
  celglow-core/src/           # 純アルゴリズム (AE 非依存)
    lib.rs        — pub API: render(input, input/output rect, params, view, out)
    params.rs     — Params (serde、Default = 本書の既定値、downsample 込み)
    field.rs      — S1: 強度抽出 + separable box blur ×3
    noise.rs      — hash / value noise / fbm (CPU 参照実装)
    render.rs     — S2〜S6 の per-pixel 評価
  celglow-cli/                # PNG/TOML ハーネス (mock-harness.md)
  celglow2/                   # AE プラグイン v2 (新規 cdylib、core の薄いガワ)
    src/lib.rs    — AE パラメータ ⇄ core::Params、SmartFX、bpc 変換
    src/gpu.rs
    shader.wgsl   — GPU シェーダ (新規)
  celglow/                    # v1 (凍結。一切変更しない)
```

- per-pixel ループは行単位で `rayon` 並列化 (現行同様の構造なら
  std::thread チャンクでも可)。
- ブラーは sliding-window box blur (O(W×H)、半径非依存)。
- CPU は Draft/Normal でも同じダウンサンプル戦略を使う。

## 6. GPU 実装 (wgpu / WGSL)

パス構成:

| パス | 内容 | 備考 |
| --- | --- | --- |
| P0 | strength 抽出 (+premul 色) | input → R32F (+RGBA16F) |
| P1..P6 | box blur H/V ×3 反復 | ping-pong、Draft/Normal は縮小解像度 |
| P7 | 最終合成 (S2〜S6 全部) | F テクスチャ + input を読み 1 パス |

- ノイズは全て WGSL 内で解析評価 (テクスチャ不要)。
- P7 は 8×8 ワークグループ。1080p で数 ms を想定。
- **F テクスチャ (r32float) のサンプリング**: WebGPU コア仕様で
  `r32float` は non-filterable のため、sampler の bilinear は
  validation error になる (`FLOAT32_FILTERABLE` feature はハード依存で
  フォールバック方針と相性が悪い)。`textureLoad` 4 点 + 手動 bilinear
  で実装する。CPU 実装と bit レベルで揃うので parity にも有利。
  r16float 化は Ring Count 64 時の位相量子化 (~10bit 仮数) が
  リング縁のジッタになり得るため採らない。
- CPU/GPU で同一のハッシュ・ノイズ実装 (§7) を使い、
  parity テスト (§7 の f32 直接比較) で検証する。
- v1 gpu.rs の「初期化失敗 → CPU フォールバック」構造をコピーして踏襲。
- **MFR 対応**: SupportsThreadedRendering + MFR では SmartRender が
  並列に呼ばれる。GPU リソース (ping-pong テクスチャ等の内部状態) は
  Mutex で直列化する (将来必要ならフレームサイズ別プール化)。

## 7. ノイズ / 乱数仕様 (CPU・GPU 共通)

決定論・プラットフォーム非依存が絶対条件 (seed 固定でフレーム再現)。

```
// 整数ハッシュ: lowbias32
hash(x: u32) -> u32 {
    x ^= x >> 16; x *= 0x7feb352d;
    x ^= x >> 15; x *= 0x846ca68b;
    x ^= x >> 16; x
}
u01(k, seed, salt) = hash(k ^ hash(seed ^ salt·0x9E3779B9)) / 2^32
// 乗算はすべて wrapping (Rust: wrapping_mul。debug でのパニック防止)

// value noise 3D: 格子ハッシュ + quintic 補間 (6t⁵−15t⁴+10t³)
// fbm: 最大 4 オクターブ、lacunarity 2.0、gain 0.5
// **振幅和で正規化し、oct によらず厳密に [-1,1] に収める**
//   (正規化しないと 4oct で ±1.875 に達し、rect 拡張の不足と
//    グレイン amount の oct 依存を招く)
norm(oct)   = 1 / Σ_{i<oct} gain^i
fbm(p, oct) = norm(oct) · Σ_{i<oct} gain^i · (vnoise(p · 2^i) · 2 − 1)
```

- 全ノイズ入力は **フル解像度レイヤ座標系** (px、§4.4)。
  バッファ原点にもダウンサンプル解像度にも依存させない
  (Quality / プレビュー解像度を変えても模様が変わらないこと)。
- Evolution は vnoise の z 軸。角度 360° = z +1.0 (ループはしない、§2 S2)。
- **CPU/GPU parity の判定基準** (ゴールデンとは別物として定義):
  f32 出力バッファ同士の直接比較で、99.9 percentile |Δ| < 1e-3。
  AA 境界画素 (φ の格子境界近傍) は許容を緩和してよい。
  8bit PNG 経由の比較は量子化がノイズ床になるため parity には使わない。

## 8. パフォーマンス目標

1920×1080 / Spread 120 / Normal 品質:

| 処理 | CPU (目標) | GPU (目標) |
| --- | --- | --- |
| S1 ブラー (1/2 解像度) | 15 ms | 1 ms |
| S2〜S6 合成 | 80 ms | 3 ms |
| GPU 転送 (f32 RGBA 1080p ≈33MB up + down) | — | 3 ms |
| 合計 | ≤ 120 ms | ≤ 8 ms |

fbm 呼び出しは 1 ピクセルあたり: wobble 2 + grain 2 + macro 1 = 5 回
(オクターブ込みで vnoise 最大 ~14 回)。CPU で重ければ
grain/macro ノイズを事前ベイクしたタイルテクスチャに置換する
(品質差が出ないことを確認の上)。

## 9. v1 コードの扱い (完全分離)

**v1 (`rust/celglow`) は凍結し、一切変更・削除しない。** v2 は
`celglow-core` / `celglow-cli` / `celglow2` を新規に作り、v1 から
使える部分を**コピーして**流用する。v1 と v2 は match name が異なる
別プラグインとして AE に同時インストールできる。

| v1 の資産 | v2 での扱い |
| --- | --- |
| SmartFX 骨格 (PreRender/SmartRender/checkout/pre_render_data) | celglow2 へコピーして流用 (rect 式は §4.1 の修正込み) |
| 8/16/32bpc ↔ f32 変換 | celglow2 へコピーして流用 (チャンネル順は ARGB — mock-harness §2 参照) |
| wgpu 初期化 / フォールバック構造 | celglow2 へコピーして流用 (パイプライン定義は新規) |
| パラメータ登録の書式 | 書き方のみ参考 (定義内容は全面刷新) |
| edt.rs / JFA / バンド生成ロジック | v2 では使わない (v1 内にそのまま残る) |
| Copies / Distribution 系パラメータ | v2 スコープ外 (リファレンスに複製要素なし。必要になれば v2.1 で S2 の前に複製変換を挿す) |

`docs/design.md` / `docs/requirements.md` は先頭に
「v2 (`design-v2.md`) で置き換え」の注記を追加して凍結済み。

## 10. 受け入れ基準

- [ ] リファレンス入力 (画像 2 相当の素材) に既定値 + 微調整で
      画像 1 の見た目 (等高線リング / 筆質感 / ムラ / 色フリンジ) が
      再現できる。各中間 View が画像 3〜5 に対応する:
      `Field` ≈ 画像 3、`Rings` ≈ 画像 4、`Texture Mask` 適用後 ≈ 画像 5。
- [ ] Seed 固定・Evolution 固定でフレーム A→B→A が bit 再現。
- [ ] バッファ原点をずらしてレンダしても同一レイヤ座標の出力が
      bit 一致する (ROI/ズーム非依存の検証。padding テスト)。
- [ ] Quality Draft/Normal/Best・AE プレビュー解像度 1/2 で
      リング位置・ノイズ模様が変わらない (エイリアス差のみ)。
- [ ] CPU/GPU parity: f32 直接比較で 99.9 percentile |Δ| < 1e-3 (§7)。
- [ ] 8/16/32bpc すべてで α 汚れ・バンディングなし。
- [ ] 1080p GPU で 8 ms/frame 以下 (RTX クラス)。
- [ ] レイヤ境界の外までリングが正しく描画される (rect 拡張の検証)。

## 11. マイルストーン

> M0 / M3.5 と開発ループの詳細は `docs/mock-harness.md` を参照。
> M1〜M3 は AE を介さず CLI ハーネス上で開発・確認する。

| MS | 内容 | 完了条件 |
| --- | --- | --- |
| M0 | workspace 再編 + `celglow-core` 骨格 + CLI ハーネス | `mock-harness.md` §6.1 の全項目を満たす |
| M1 | パラメータ刷新 + S1 (場) + S3 (リング波, 均一) CPU | View=Rings で画像 4 の滑らか版が出る |
| M2 | S3 不均一化 + S2 wobble + S4 グレイン ×2 | 画像 5 相当が出る |
| M3 | S5 ムラ + S6 カラーモード/合成/デバッグ View | 画像 1 相当が出る |
| M3.5 | AE プラグインを core 呼び出しに書き換え | AE 上で CLI と同じ絵が出る |
| M4 | GPU 移植 + parity テスト + 性能計測 | 受け入れ基準の性能/ΔE 達成 |
| M5 | 微調整 (グレイン定数チューニング)・実素材テスト | 受け入れ基準全達成 |

## 12. 未決事項 (実装中に判断)

- グレイン A/B の整形定数 (`sharp_A_bias`, `borderBoost`) の最終値 —
  M2 でリファレンス画像 5 と目視比較して確定。
- Fill モードで scramble の色割れを残すか、完全単色にするか
  (現案: 残す。完全単色は Scramble=0 で得られる)。
- CPU の fbm が重い場合のベイク済みノイズタイル化 (§8)。
- Rainbow モードの色相基準をリング番号 k にするか場 u にするか
  (現案: k。リングごとに色が階段状に変わる方がセル調)。
- グレイン Cut を包絡線 E の適用前 (`w·g`) に掛ける代替案 (§2 S4) —
  外周リングが早く消えすぎる場合に M2 で切替判断。
- Turbulent Displace「Bulge」と独立 2ch fbm 変位は厳密には別物
  (Bulge は膨張性の変位、2ch fbm は「Turbulent」相当)。M2 の照合で
  リファレンスとうねり方が合わない場合の第一容疑者。
- リニア作業色空間プロジェクトでの見た目差 — 既定チューニングは
  ガンマ空間前提 (§2)。リニア環境向けの補正は v2.1 以降で検討。
- Evolution のループ再生 (z 方向周期格子 + Cycle パラメータ) — v2.1。
