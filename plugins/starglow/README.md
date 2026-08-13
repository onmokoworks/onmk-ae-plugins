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

```powershell
cargo build --release
```

## ライセンス

MIT License。詳細は [LICENSE](./LICENSE) を参照してください。

---

## English

ONMK Starglow is an experimental spectral star-glow effect for Adobe After Effects with multi-direction rays, color maps, threshold controls, shimmer, and glow compositing.

`Affect Alpha` extends output alpha with the generated glow so rays remain visible outside the source layer's original alpha. Disable it to preserve source alpha.
