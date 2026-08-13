# onmk AE Plugins

onmkが開発・メンテナンスするAdobe After Effectsプラグインのモノレポです。

## リポジトリ構成

- `plugins/` — 各After Effectsプラグイン
- `crates/` — 複数プラグインで共有するRustクレート
- `tools/` — 共通のビルド・検証・開発ツール
- `templates/` — 再利用可能なプラグインテンプレート

実験中・未公開のプロジェクトはリポジトリ外で開発し、継続開発または公開の準備が整ったものから順次移行します。

## プラグイン

### カラー・ルック

| プラグイン | 概要 | 状態 |
|---|---|---|
| [Rio de Janeiro Filter](./plugins/rio-de-janeiro-filter/) | Rio de Janeiroフィルター風の色調を再現 | 開発中 |

### ブラー・画像フィルター

| プラグイン | 概要 | 状態 |
|---|---|---|
| [MedianPro](./plugins/median-pro/) | アルファ対応のMedian／Weighted Medianフィルター | 実験的 |
| [AdaptiveFilter](./plugins/adaptive-filter/) | Kuwahara／Generalized Kuwahara／Bilateralフィルター | 実験的・仮称 |
| [MinimaxMap](./plugins/minimax-map/) | 符号付きMinimum／Maximumモルフォロジー | 開発中 |

### グロー・光学

| プラグイン | 概要 | 状態 |
|---|---|---|
| [CelGlow](./plugins/cel-glow/) | イラスト・アニメ向けの段階的なセルグロー | 実験的 |
| [onmkGlow](./plugins/onmk-glow/) | After Glowとストリークを備えたマルチパス・グロー | 実験的 |
| [ONMK Starglow](./plugins/starglow/) | 複数方向の光条とスペクトル色を生成するスターグロー | 実験的 |
| [RefractionDispersion](./plugins/refraction-dispersion/) | ガラス風の屈折、色分散、ハイライト表現 | 開発中 |
| [PrismWarp](./plugins/prism-warp/) | レンズ勾配による屈折とプリズム状の色分散 | 実験的 |

### 変形・マップ・マスク

| プラグイン | 概要 | 状態 |
|---|---|---|
| [UVProject](./plugins/uv-project/) | UV／STマップによる投影と平面UVマップ生成 | 開発中 |
| [ScatterMap](./plugins/scatter-map/) | レイヤーマップで制御できるピクセル散乱 | 実験的 |
| [PathArray](./plugins/path-array/) | マスクパスに沿ってレイヤーの複製を配置 | 実験的 |
| [MaskTransform](./plugins/mask-transform/) | マスクパスの反転と回転 | 実験的 |
| [MaskOffset](./plugins/mask-offset/) | マスクパスのオフセット、塗り、フェザー | 実験的 |
| [LumaNormal](./plugins/luma-normal/) | 輝度勾配からRGBノーマルマップを生成 | 実験的・8 bpc |

### 時間・生成・スタイライズ

| プラグイン | 概要 | 状態 |
|---|---|---|
| [FrameSlice](./plugins/frame-slice/) | 画像内にスリットスキャン風の時間差を生成 | 開発中 |
| [TuiImage](./plugins/tui-image/) | 画像をANSIアート／TUI風のセル表現に変換 | v0.1.0・開発中 |
| [LumaGrain](./plugins/luma-grain/) | 輝度帯ごとに強さを調整できる軽量グレイン | 実験的・8 bpc |

### 解析・ML

| プラグイン | 概要 | 状態 |
|---|---|---|
| [Depth ONNX](./plugins/depth-onnx/) | ONNX Runtimeによる単眼深度推定 | 開発中 |

### パーティクル・実験基盤

| プラグイン | 概要 | 状態 |
|---|---|---|
| [Particle Kit](./plugins/particle-kit/) | パーティクルエフェクト、共有エンジン、Node UI、Lattice Lab | 実験的 |

---

## English

This monorepo contains Adobe After Effects plugins developed and maintained by onmk.

### Repository layout

- `plugins/` — Individual After Effects plugins
- `crates/` — Rust crates shared across plugins
- `tools/` — Shared build, verification, and development tools
- `templates/` — Reusable plugin templates

Experimental and unpublished projects are developed outside this repository and moved here once they are ready for continued development or publication.

### Color and Looks

| Plugin | Description | Status |
|---|---|---|
| [Rio de Janeiro Filter](./plugins/rio-de-janeiro-filter/) | Recreates a look inspired by the Rio de Janeiro filter | In development |

### Blur and Image Filters

| Plugin | Description | Status |
|---|---|---|
| [MedianPro](./plugins/median-pro/) | Alpha-safe Median and Weighted Median filtering | Experimental |
| [AdaptiveFilter](./plugins/adaptive-filter/) | Kuwahara, Generalized Kuwahara, and Bilateral filtering | Experimental / working name |
| [MinimaxMap](./plugins/minimax-map/) | Signed Minimum / Maximum morphology | In development |

### Glow and Optics

| Plugin | Description | Status |
|---|---|---|
| [CelGlow](./plugins/cel-glow/) | Stepped cel glow for illustration and anime compositing | Experimental |
| [onmkGlow](./plugins/onmk-glow/) | Multi-pass glow with after-glow shaping and streaks | Experimental |
| [ONMK Starglow](./plugins/starglow/) | Multi-direction spectral star glow | Experimental |
| [RefractionDispersion](./plugins/refraction-dispersion/) | Glass-like refraction, chromatic dispersion, and highlights | In development |
| [PrismWarp](./plugins/prism-warp/) | Lens-driven refraction and prismatic chromatic dispersion | Experimental |

### Distortion, Maps, and Masks

| Plugin | Description | Status |
|---|---|---|
| [UVProject](./plugins/uv-project/) | UV/ST projection and planar UV-map generation | In development |
| [ScatterMap](./plugins/scatter-map/) | Map-controlled pixel scattering | Experimental |
| [PathArray](./plugins/path-array/) | Distributes layer copies along a mask path | Experimental |
| [MaskTransform](./plugins/mask-transform/) | Flips and rotates mask paths | Experimental |
| [MaskOffset](./plugins/mask-offset/) | Offsets mask paths with fill and feather controls | Experimental |
| [LumaNormal](./plugins/luma-normal/) | Generates RGB normal maps from luminance gradients | Experimental / 8 bpc |

### Time, Generation, and Stylization

| Plugin | Description | Status |
|---|---|---|
| [FrameSlice](./plugins/frame-slice/) | Creates slit-scan-style time offsets | In development |
| [TuiImage](./plugins/tui-image/) | Converts images into ANSI art / TUI-style cells | v0.1.0 / In development |
| [LumaGrain](./plugins/luma-grain/) | Lightweight grain with luminance-range weighting | Experimental / 8 bpc |

### Analysis and ML

| Plugin | Description | Status |
|---|---|---|
| [Depth ONNX](./plugins/depth-onnx/) | Monocular depth estimation with ONNX Runtime | In development |

### Particles and Experimental Systems

| Plugin | Description | Status |
|---|---|---|
| [Particle Kit](./plugins/particle-kit/) | Particle effects, shared engine, Node UI, and Lattice Lab | Experimental |
