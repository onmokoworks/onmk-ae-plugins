# CelGlow — 要件定義 (v0.1 ドラフト) 【凍結: `design-v2.md` で置き換え】

> **2026-07-10: バンド/複製/カスタム UI 方式の要件は廃止。**
> 現行の仕様は `docs/design-v2.md` を参照。

> イラスト/アニメ撮影向け After Effects プラグイン。
> 発生源から放射する **階調バンド型グロー**(境界がカキッと見える、
> セル塗り風の多色グロー)を生成。複製/回転/ランダムで
> 放射状の多重グローを作れる。
> 仮称 **CelGlow** / 2026-04-23 初版。
>
> 姉妹プロジェクト **LayerGlow** (ソフト多層グラデ版) と
> コア基盤を共通化する方針(§5, §11)。

---

## 1. 背景 / 目的

- アニメ/イラスト調の作品でよく見る「階調グロー」
  (例: 呪術廻戦の気エフェクト、ヒロアカの発光、ゲーム UI のキラキラ)
  は AE 標準の Glow だとソフトすぎて雰囲気が出ない。
- やりたいのは距離に応じて **ハードエッジのバンド** が重なる見た目:
  中心=白 / 内側=黄 / 中間=橙 / 外側=赤 など。
  バンド境界がカキッと見え、各バンドの色と幅を個別に制御したい。
- さらに十字星 / 花型バースト / 太陽フレア的に
  **グロー全体を複製して放射状に回転配置** できると一発で
  イラスト風の派手な発光が作れる。
- 個別にスライダを並べるよりは「カーブ / バンドリスト」を
  直接編集する UI が欲しい。

## 2. スコープ

### 対象ユーザー
- アニメ/MV 撮影、イラスト系モーショングラフィッカー、ゲーム UI 動画。
- AE 2025 の Effect パネルから適用 → キーフレームで動かす前提。

### In-scope (v1)
- RGBA 2D レイヤに適用し、グロー成分を生成して合成。
- 発生源: ① レイヤ輝度しきい値 / ② 外部マットレイヤ入力 /
  ③ 色/色域ピック の 3 方式(切替 + 併用可)。
- **階調バンド**: 可変数 N (1..16) のハードエッジバンド。
  各バンドは (内半径%, 外半径%, RGBA, 不透明度, エッジ softness,
  **減衰カーブ**) を持つ。減衰カーブは **v1 からバンドごとに独立**。
- **複製 (Copies)**: 1..16 コピーを放射(等角度)/ リング(同心円)/
  直線 分布で配置、各コピーに回転・スケール・色相オフセット。
- **ランダム化**: 回転 / スケール / 色相 / 不透明度に seed 付き jitter。
- **カスタム UI**: 自前描画のバンドエディタ + 減衰カーブエディタ
  (AE の Effect Controls 内、ArbitraryData Param)。
- 8bpc / 16bpc / 32bpc-float 全対応、SmartFX。
- macOS (arm64 / x86_64 ユニバーサル) + Windows x64。
- Metal / DirectCompute GPU パスを第一優先、CPU フォールバック。

### Out-of-scope (v1)
- 時間方向の一貫性(プリコンプ全体を見たフリッカー抑制)。
- 3D レイヤ / カメラ空間処理。
- グラデーション(色ストップが連続的に滑らかなやつ)→ **LayerGlow** 側。
- AEGP / 独自フローティングパネル / スクリプト連携。
- AE 標準の Masks 解析(マスクの輪郭から光らせる、等)。

## 3. 機能要件

### 3.1 発生源 (Source)
ユーザが 3 方式から選択(併用可、OR 合成)。

| パラメータ | 型 | デフォルト | 備考 |
| --- | --- | --- | --- |
| Source Mode | popup (multi-select via checkboxes) | Luma | Luma / Matte / Color を個別 ON/OFF |
| — Luma Threshold | slider 0..255 | 200 | この値以上の輝度を源に。 |
| — Luma Softness | slider 0..50 | 10 | 閾値付近の羽根化幅。 |
| Matte Layer | layer param | (None) | 任意レイヤをマスクソースに。 |
| — Matte Channel | popup | Luminance | Luminance / Alpha / Red / Green / Blue |
| — Invert Matte | checkbox | OFF | |
| Color Pick | color | #FFFFFF | 光らせたい色の中心。 |
| — Color Tolerance | slider 0..100 | 25 | HSL 距離 (%) での許容範囲。 |
| — Eyedropper | eyedropper | — | プレビューから 1 点指定。 |
| Combine Mode | popup | Max | 複数 ON 時の合成: Max / Add / Intersect |
| Source Dilation | slider -4..+8 px | 0 | 源マスクの膨張/収縮。 |

内部的に `float32 1ch` の **Source Mask** を生成。

### 3.2 階調バンド (Bands)
`ArbitraryData` で可変長リストとして保持。既定は 3 バンド。

各バンドのフィールド:

| フィールド | 型 | 範囲 | 備考 |
| --- | --- | --- | --- |
| Inner Radius | % of layer shorter side | 0..200 | 源マスクからの距離(後述) |
| Outer Radius | % of layer shorter side | 0..200 | Inner < Outer を強制 |
| Color | RGBA | — | 内側ピーク色。 |
| Opacity | 0..100% | | |
| Edge Softness | 0..8 px | 0 | 内外エッジの AA 量。 |
| Falloff Curve | curve (バンドごと) | — | §3.3 参照 |

バンドは **リストとして並び替え可能** (ドラッグ & ドロップ)。
バンドは最大 16。UI の Band List Pane は縦スクロール可能。

### 3.3 減衰カーブ (Falloff Curve, バンドごと)
各バンドの不透明度プロファイルを、独立した自前編集カーブで定義。

- X 軸 = バンド内相対距離 (0=内側、1=外側)
- Y 軸 = 不透明度 (0..1)、バンドの Opacity に乗算
- 制御点: 追加 / 削除 / ドラッグ、Bezier ハンドル付き
- **v1 からバンドごとに独立**。Band List で選択中のバンドの
  カーブが Curve Editor Pane に表示される。
- プリセット: Flat / Linear / Smooth / Ease-In / Ease-Out / S-Curve
- 「このカーブを全バンドへコピー」ボタンで一括適用可。

内部表現は最大 8 制御点、各点 (x, y, inTanX, inTanY, outTanX, outTanY)。
256-entry LUT にベイクして GPU へ渡す(バンドごとに 1 LUT、
最大 16 バンド × 256 RGBA = 16 KB / インスタンス、GPU に一括転送)。

### 3.4 距離計算 (Distance Field)
- 源マスクのピクセル値 `> 0.5` を源とし、**ユークリッド距離変換 (EDT)**
  を 2-pass Felzenszwalb で計算 (O(W×H))。
- 結果 `D[px]` を「レイヤ短辺長 × 半径%」でスケールして
  各バンドの内外半径と比較 → バンド帰属を決定。
- エッジ softness は `(D - radius)` の絶対値から AA 係数を計算。

### 3.5 複製 / 変換 (Copies)
1 個の「源マスク + バンド集合」を 1 インスタンスとし、
同じ構成をコピーして配置する。

| パラメータ | 型 | デフォルト | 備考 |
| --- | --- | --- | --- |
| Copies | int 1..16 | 1 | 原本含む枚数 |
| Distribution | popup | Radial | Radial / Ring / Linear |
| Origin | point | layer center | 分布の中心点 |
| Radial Sweep | angle | 360° | Radial 時、全周に対しこの角度内に配置 |
| Ring Radius | slider 0..50% | 10% | Ring 時、同心円の半径 |
| Line Vector | point | (100, 0) | Linear 時、次コピーまでの変位 |
| Per-copy Rotation Step | angle | 0° | 各コピーに加算される回転 |
| Per-copy Scale Step | 0.5..2.0 | 1.0 | 各コピーの内外半径に乗算される係数 |
| Per-copy Hue Step | ±180° | 0° | 各コピーに加算される色相回転 |
| Per-copy Opacity Step | ±100% | 0% | 各コピーに加算される不透明度 |
| Blend (copies) | popup | Add | 複製同士の合成: Add / Screen / Normal |

### 3.6 ランダム化 (Randomize)
各コピーに決定論的ジッタを加える。seed 固定でフレーム間再現性を担保。

| パラメータ | 型 | デフォルト | 備考 |
| --- | --- | --- | --- |
| Enable Random | checkbox | OFF | |
| Seed | int 0..65535 | 1 | 変えると違う分布 |
| Rotation Jitter | ±180° | 0° | |
| Scale Jitter | 0..100% | 0% | 乗算に ±jitter% を適用 |
| Hue Jitter | ±180° | 0° | |
| Opacity Jitter | 0..100% | 0% | |
| Position Jitter | 0..50% (of min side) | 0% | 各コピーの中心を揺らす |
| Animate over Time | checkbox | OFF | ON 時は時刻を seed に混ぜて揺らす |

※ `Animate over Time` ON 時はフレームごとに乱数が更新される(ちらつき用途)。
OFF 時は同一シーンで固定。

### 3.7 出力 / ブレンド
| パラメータ | 型 | デフォルト | 備考 |
| --- | --- | --- | --- |
| Output | popup | Glow over Source | Glow over Source / Glow only / Source only / Distance / Mask |
| Global Blend | popup | Add | グロー全体と元素材の合成 |
| Amount | slider 0..200% | 100% | ブースト可 |
| Preserve Alpha | checkbox | OFF | ON でレイヤα外を塗らない |

### 3.8 カスタム UI (ArbitraryData)
`PF_Param_ARBITRARY_DATA` + `PF_Event_DRAW / DO_CLICK / DRAG / KEY_DOWN`
で実装。ECW 内に固定高さで描画。

- **Band List Pane** (左 60%)
  - 各バンドを横長スロットとして積み、左右ドラッグで内/外半径を変更。
  - 上下ドラッグで並び替え。
  - クリックで選択(選択中バンドのカーブが右ペインに出る)。
  - ダブルクリックで色選択ダイアログ。
  - +/- ボタンでバンド追加/削除。
  - 16 バンド入るので縦スクロール対応。
- **Curve Editor Pane** (右 40%)
  - 選択中バンドの減衰カーブを表示・編集。
  - 制御点のドラッグ、Alt+クリックで Bezier ハンドル引き出し。
  - プリセットボタン列 (Flat / Linear / Smooth / S-Curve / ...)。
  - "Apply to all bands" ボタンで全バンドへ現在のカーブをコピー。
- undo/redo は AE の一般 undo stack に乗せる (`PF_SetCurrentExtent` 系)。
- コピー/ペースト: 右クリックメニュー(バンド単位 / カーブ単位)。
- 高 DPI 対応(Retina / Windows 200%)。

### 3.9 パフォーマンス目標
- 1920×1080 / 16bpc / Copies=4 / Bands=3 で
  - CPU: 200ms/frame 以下(プレビュー許容)
  - GPU: 33ms/frame 以下(ほぼリアルタイム)
- EDT は 1080p で 10..20ms (CPU SIMD) / 2..4ms (GPU)。
- バンド合成は LUT 参照 → GPU で 1..2ms。

### 3.10 プレビュー戦略
- `(layerId, time, paramHash, sourceHash) → output` の LRU (32f) キャッシュ。
- カーブ / バンドリストのハッシュは ArbitraryData のバイト列で計算。

## 4. 非機能要件

- **対応 AE**: After Effects 2025 (SDK 25.2) 基準。最低 2023 まで動作確認。
- **OS**: macOS 13+ (arm64 優先 / x86_64 併載)、Windows 10 22H2 以上 x64。
- **言語**: C++17。UI 描画は AE の drawbot API を経由(OS 直描画は避ける)。
- **ライセンス**: 依存 OSS は MIT / Apache-2.0 / BSD のみ。
- **配布**: `.plugin` (macOS) / `.aex` (Windows) 単体。
- **カテゴリ**: AE の Effect メニュー下に `onmk > CelGlow` として追加。
- **ログ**: `~/Library/Logs/CelGlow/CelGlow.log` /
  `%LOCALAPPDATA%\CelGlow\CelGlow.log`。エラー時のみ。

## 5. アーキテクチャ概要

```
CelGlow/
  docs/
    requirements.md          # 本書
    design.md                # アルゴリズム詳細と疑似コード (次ステップ)
  plugin/
    CMakeLists.txt
    cmake/                   # FindAfterEffectsSDK.cmake (FillLine から流用)
    src/
      CelGlow.cpp            # AE エントリ (PF_Cmd_* ディスパッチ)
      CelGlow.h
      CelGlowPiPL.r          # PiPL リソース
      Params.h               # パラメータ ID / 構造
      Source.cpp/.h          # 発生源検出 (Luma / Matte / Color)
      DistanceField.cpp/.h   # EDT (Felzenszwalb)
      Bands.cpp/.h           # バンド評価 + LUT
      Copies.cpp/.h          # 複製・変換 (Radial/Ring/Linear + Random)
      ui/
        BandEditor.cpp/.h    # ArbitraryData の描画 + 入力
        CurveEditor.cpp/.h
        ArbitraryData.h      # 直列化
      gpu/
        CelGlow.metal        # macOS
        CelGlow.hlsl         # Windows
        GpuDispatcher.cpp/.h
      FrameCache.h
    Mac/Info.plist
    Win/CelGlow.rc
  third_party/
    AfterEffectsSDK/         # シンボリックリンク → /Users/onmk/Documents/After Effects SDK/ae25.2_20.64bit.AfterEffectsSDK/
  scripts/
    build_mac.sh
    build_win.ps1
```

### 処理パイプライン (SmartFX)
1. `PF_Cmd_SMART_PRE_RENDER`:
   - 入力レイヤ + Matte レイヤ(指定時)を request。
   - 出力 rect は入力レイヤ bound を `Σ Outer Radius × GlobalScale × √2` だけ拡張。
2. `PF_Cmd_SMART_RENDER`:
   1. 入力 / matte を float32 に昇格。
   2. Source mask 生成(Luma / Matte / Color の OR 合成)。
   3. Source mask から EDT → `D`。
   4. 各コピーについて: 回転・スケール・ジッタを適用した後
      各バンドを LUT で評価、RGBA を累加。
   5. Global Blend と Amount で元素材と合成。
   6. 指定 bit depth に戻して出力チェックアウト。

## 6. パラメータ ID と既定値 (v1 案)

```
enum {
  kParam_Input = 0,
  // Source
  kParam_SrcGroup,
  kParam_SrcModeLuma,           // checkbox
  kParam_SrcLumaThreshold,
  kParam_SrcLumaSoftness,
  kParam_SrcModeMatte,          // checkbox
  kParam_SrcMatteLayer,
  kParam_SrcMatteChannel,
  kParam_SrcMatteInvert,
  kParam_SrcModeColor,          // checkbox
  kParam_SrcColor,
  kParam_SrcColorTolerance,
  kParam_SrcColorPick,          // eyedropper-like color param
  kParam_SrcCombine,
  kParam_SrcDilation,
  // Bands
  kParam_BandsGroup,
  kParam_BandsData,             // ArbitraryData: N bands (各バンドに curve を内包)
  kParam_GlobalScale,           // % multiplies all radii
  // Copies
  kParam_CopiesGroup,
  kParam_Copies,
  kParam_Distribution,
  kParam_Origin,
  kParam_RadialSweep,
  kParam_RingRadius,
  kParam_LineVector,
  kParam_RotStep,
  kParam_ScaleStep,
  kParam_HueStep,
  kParam_OpacityStep,
  kParam_BlendCopies,
  // Randomize
  kParam_RandGroup,
  kParam_RandEnable,
  kParam_RandSeed,
  kParam_RandRotJitter,
  kParam_RandScaleJitter,
  kParam_RandHueJitter,
  kParam_RandOpacityJitter,
  kParam_RandPosJitter,
  kParam_RandAnimate,
  // Output
  kParam_OutputView,
  kParam_GlobalBlend,
  kParam_Amount,
  kParam_PreserveAlpha,
  kNumParams,
};
```

プリセット (既定で同梱):
- `Starburst Yellow` — 4 バンド白→黄→橙→赤、Copies=8 Radial。
- `Holy Cross` — 2 バンド、Copies=2 Linear (十字)。
- `Soft Flare` — 1 バンド、Copies=6 Radial + Random。
- `Neon Ring` — 1 バンド、Copies=12 Ring、色相 30°ステップ。

## 7. 成功基準 (受け入れテスト)

- [ ] 1920×1080 8bpc で `Starburst Yellow` プリセットがデフォルトで雰囲気出る。
- [ ] 16bpc / 32bpc で色のバンディング / α 汚れがない。
- [ ] Copies=4 + Bands=3 で GPU 30fps を達成 (M2 Pro, 1080p)。
- [ ] バンドエディタで 100 回マウス操作 → AE 無反応 / クラッシュなし。
- [ ] カーブエディタのプリセット切替でカーブが即座に反映される。
- [ ] macOS / Windows で同一プロジェクトが ΔE < 1 で一致。
- [ ] Random Seed 固定時、フレーム A→B→A で同一出力。

## 8. リスク / 未決事項

- **ArbitraryData の永続化**: プロジェクトファイル (.aep) 跨ぎで
  バンドリスト / カーブが正しく復元されるか。flatten 形式を
  design.md で詰める。エンディアン / バージョン ID 必須。
- **カスタム UI の工数**: 自前バンドエディタ + バンドごとカーブエディタは
  描画・入力・undo 統合で **3..4 週**見込み(カーブ個別化で +数日)。
  v1 で間に合わなければ **v1 は固定 3 バンド(通常パラメータ)+
  プリセットカーブ選択**でリリースし、v1.1 でカスタム UI + 可変バンド
  を追加する段階リリース案を残す。
- **高 DPI 座標**: AE drawbot の座標系が OS ごとに微妙に違う。
  Retina / Win 150%/200% で早期に目視確認。
- **レイヤ境界の扱い**: バンドが源マスクから遠く離れる場合、
  出力バッファが足りない。v1 は自動拡張 + `PF_OutFlag_PIX_INDEPENDENT`
  の整合性を確認。
- **Copies × Bands の爆発**: 最悪 16 × 16 = 256 回の評価になるが、
  GPU ならシェーダ 1 パスで収まる(バンド LUT を texture2D で束ねる)。
  CPU フォールバックは距離 → バンド決定を二分探索で O(log N) に抑える。
- **色空間**: ワーキング RGB が linear か sRGB かで発光の見た目が
  変わる。AE の「リニア作業色空間」有効/無効で差分が出ないか検証。

## 9. マイルストーン (暫定)

| MS | 内容 | 想定 |
| --- | --- | --- |
| M0 | 要件定義(本書)レビュー完了 | 今週 |
| M1 | `design.md` + EDT プロトタイプ + LUT 評価 | +1週 |
| M2 | AE スケルトン(パラメータ登録、源マスク Luma のみ、1 バンド、1 コピー) | +1週 |
| M3 | 3 バンド固定 + 減衰プリセット + 複製 Radial で「とりあえず動く」 | +2週 |
| M4 | Matte / Color 源 + Ring/Linear + Random | +1週 |
| M5 | カスタム UI (バンドエディタ + カーブエディタ) | +3週 |
| M6 | GPU パス(Metal 優先) | +2週 |
| M7 | Windows ビルド + 両 OS パリティ | +1週 |
| M8 | β配布 + 試験運用 | 以降 |

## 10. 依存と SDK

- AE SDK: `/Users/onmk/Documents/After Effects SDK/ae25.2_20.64bit.AfterEffectsSDK/`
  (FillLine と同じパス。`third_party/AfterEffectsSDK` からシンボリックリンク)
- CMake 3.24+、Xcode 15+ (mac)、Visual Studio 2022 (win)。
- 外部ライブラリは基本不要。乱数は `<random>` 標準で足りる。

## 11. LayerGlow との関係

姉妹プロジェクト **LayerGlow** (ソフト多層グラデ版) と共通化できる要素:

- AE スケルトン / PiPL / パラメータ登録の骨格
- `Source.*` 発生源検出
- `DistanceField.*` EDT
- `Copies.*` 複製・ランダム・変換
- `ui/CurveEditor.*` / `ui/ArbitraryData.*`
- `gpu/GpuDispatcher.*`
- `FrameCache.h`

v1 は各プロジェクト内に独立して置き、API が安定してから
`glow-core` 共通スタティックライブラリに切り出す方針
(**M4 完了時点で判断**)。

---

## 次のアクション
1. v0.2 確定済み (2026-04-23):
   - バンド数上限 = **16**
   - 複製上限 = **16** のまま
   - 減衰カーブ = **v1 からバンドごと個別**
   - Source Combine Mode 既定 = **Max**
2. `docs/design.md` で
   ArbitraryData 直列化フォーマット / バンドエディタのヒット判定 /
   EDT の細部 / LUT レイアウト / GPU シェーダ骨格を詰める。
3. 並行して `plugin/` スケルトン(CMake + Empty SmartFX + PiPL)を作成。
