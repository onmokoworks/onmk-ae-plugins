# onmk AE Plugins

onmkが開発・メンテナンスするAdobe After Effectsプラグインのモノレポです。

## リポジトリ構成

- `plugins/` — 各After Effectsプラグイン
- `crates/` — 複数プラグインで共有するRustクレート
- `tools/` — 共通のビルド・検証・開発ツール
- `templates/` — 再利用可能なプラグインテンプレート

実験中・未公開のプロジェクトはリポジトリ外で開発し、継続開発または公開の準備が整ったものから順次移行します。

## ビルド・導入

各プラグインは独立したRustパッケージまたはワークスペースです。対象ディレクトリのREADMEに従ってビルドしてください。必要なSDK・OS・GPUバックエンドはプラグインごとに異なります。生成したAEXやキャッシュはGitに含めません。

FocusLinesのWindows例（Rust/MSVC環境）:

```powershell
cd plugins/focus-lines
cargo fetch --locked
./build.ps1
```

生成先は `dist/ONMK_FocusLines.aex`。MediaCoreへのコピーは別操作です。設定例・GIF・テスト手順は [FocusLines README](plugins/focus-lines/README.md) を参照してください。

## 検証状況

各READMEと `VALIDATION.md` / 検証ドキュメントに、確認した版・描画経路・未確認事項を記録しています。単体テスト、GPU比較、AEXCompatでの検証、AE実機での確認は区別しています。`main`への収録は、全プラグインの全環境での動作保証を意味しません。

共通検証ツールのテスト:

```powershell
python -m unittest discover -s tools -p "test_*.py"
```

詳細は [tools](tools/README.md)。

## プラグイン

### カラー・ルック

| プラグイン | 概要 | 状態 |
|---|---|---|
| [onmk Film](./plugins/onmk-film/) | フィルムレスポンス、滲み、密度グレイン、光学（Metal） | 開発中・v0.5 |
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
| [onmk Inner Shadow](./plugins/inner-shadow/) | アルファ形状の内側に柔らかな影を生成、CPU/GPU対応 | 実験的・v0.1.0 |
| [CelGlow](./plugins/cel-glow/) | イラスト・アニメ向けの段階的なセルグロー | 実験的 |
| [onmkFlare](./plugins/onmk-flare/) | 実レンズ処方によるゴースト、回折・Haze、Flare Studio | 開発中 |
| [onmkGlow](./plugins/onmk-glow/) | After Glowとストリークを備えたマルチパス・グロー | 実験的 |
| [ONMK Starglow](./plugins/starglow/) | 複数方向の光条とスペクトル色を生成するスターグロー | 実験的 |
| [RefractionDispersion](./plugins/refraction-dispersion/) | ガラス風の屈折、色分散、ハイライト表現 | 開発中 |
| [PrismWarp](./plugins/prism-warp/) | レンズ勾配による屈折とプリズム状の色分散 | 実験的 |

### 変形・マップ・マスク

| プラグイン | 概要 | 状態 |
|---|---|---|
| [EdgeSmith](./plugins/edge-smith/) | 距離場による輪郭の拡張・侵食・ノイズ | 実験的 |
| [UVProject](./plugins/uv-project/) | UV／STマップによる投影と平面UVマップ生成 | 開発中 |
| [ScatterMap](./plugins/scatter-map/) | レイヤーマップで制御できるピクセル散乱 | 実験的 |
| [PathArray](./plugins/path-array/) | マスクパスに沿ってレイヤーの複製を配置 | 実験的 |
| [MaskTransform](./plugins/mask-transform/) | マスクパスの反転と回転 | 実験的 |
| [MaskOffset](./plugins/mask-offset/) | マスクパスのオフセット、塗り、フェザー | 実験的 |
| [TaperStroke](./plugins/taper-stroke/) | 開いたマスクパスから終点側が細くなるストロークを生成 | 実験的・v0.3.1 |
| [LumaNormal](./plugins/luma-normal/) | 輝度勾配からRGBノーマルマップを生成 | 実験的・8 bpc |
| [onmk DistanceGradation](./plugins/distance-gradation/) | AA切り替え・元画像保持に対応する距離グラデーション | 開発中・v0.1.0 |

### 時間・生成・スタイライズ

| プラグイン | 概要 | 状態 |
|---|---|---|
| [ONMK FocusLines](./plugins/focus-lines/) | 集中線生成、丸い先端、短い線の範囲・割合、CPU/GPU対応 | 実験的・v0.1.2 |
| [Shape Plus](./plugins/shape-plus/) | 円・四角・角丸四角の塗りと線を生成 | 実験的 |
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

### Build and validation

Build each plugin from its own directory following its README; there is no single repository-wide build command. SDK, OS, and GPU requirements differ. Generated binaries are not tracked. For FocusLines on Windows with Rust/MSVC, run `cargo fetch --locked` followed by `./build.ps1` in `plugins/focus-lines`; installation is a separate step.

Plugin READMEs and validation records distinguish unit tests, GPU comparisons, AEXCompat runs, and native AE checks. Inclusion on `main` does not certify every plugin or platform. Run shared tool tests with `python -m unittest discover -s tools -p "test_*.py"`.

### Repository layout

- `plugins/` — Individual After Effects plugins
- `crates/` — Rust crates shared across plugins
- `tools/` — Shared build, verification, and development tools
- `templates/` — Reusable plugin templates

Experimental and unpublished projects are developed outside this repository and moved here once they are ready for continued development or publication.

### Color and Looks

| Plugin | Description | Status |
|---|---|---|
| [onmk Film](./plugins/onmk-film/) | Film response, emulsion scatter, density grain, optics (Metal) | In development / v0.5 |
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
| [onmk Inner Shadow](./plugins/inner-shadow/) | Soft inner shadows from source alpha, with CPU/GPU rendering | Experimental / v0.1.0 |
| [CelGlow](./plugins/cel-glow/) | Stepped cel glow for illustration and anime compositing | Experimental |
| [onmkFlare](./plugins/onmk-flare/) | Lens-prescription ghosts, diffraction, haze, and Flare Studio | In development |
| [onmkGlow](./plugins/onmk-glow/) | Multi-pass glow with after-glow shaping and streaks | Experimental |
| [ONMK Starglow](./plugins/starglow/) | Multi-direction spectral star glow | Experimental |
| [RefractionDispersion](./plugins/refraction-dispersion/) | Glass-like refraction, chromatic dispersion, and highlights | In development |
| [PrismWarp](./plugins/prism-warp/) | Lens-driven refraction and prismatic chromatic dispersion | Experimental |

### Distortion, Maps, and Masks

| Plugin | Description | Status |
|---|---|---|
| [EdgeSmith](./plugins/edge-smith/) | Distance-field expansion, erosion, and edge noise | Experimental |
| [UVProject](./plugins/uv-project/) | UV/ST projection and planar UV-map generation | In development |
| [ScatterMap](./plugins/scatter-map/) | Map-controlled pixel scattering | Experimental |
| [PathArray](./plugins/path-array/) | Distributes layer copies along a mask path | Experimental |
| [MaskTransform](./plugins/mask-transform/) | Flips and rotates mask paths | Experimental |
| [MaskOffset](./plugins/mask-offset/) | Offsets mask paths with fill and feather controls | Experimental |
| [TaperStroke](./plugins/taper-stroke/) | Variable-width tapered strokes from an open mask path | Experimental / v0.3.1 |
| [LumaNormal](./plugins/luma-normal/) | Generates RGB normal maps from luminance gradients | Experimental / 8 bpc |
| [onmk DistanceGradation](./plugins/distance-gradation/) | Antialiased distance gradients composited over the original layer | In development / v0.1.0 |

### Time, Generation, and Stylization

| Plugin | Description | Status |
|---|---|---|
| [ONMK FocusLines](./plugins/focus-lines/) | Radial lines, rounded tips, short-line ranges and mixing, CPU/GPU | Experimental / v0.1.2 |
| [Shape Plus](./plugins/shape-plus/) | Filled or stroked circles, rectangles, and rounded rectangles | Experimental |
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

## アルファ処理の品質確認

実装前に [AE アルファ契約と検査手順](docs/alpha-contract.md) を確認してください。[2026-10-16時点の監査](docs/alpha-audit-20261016.md) は当時の問題と未検証範囲の記録です。現行版の検証状況は各プラグインの記録を参照してください。
