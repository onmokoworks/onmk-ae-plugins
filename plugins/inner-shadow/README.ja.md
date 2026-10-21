# onmk Inner Shadow

入力レイヤーの輪郭の内側に影を付けるAfter Effects用エフェクトです。穴のある形状にも対応し、元のアルファを保持します。Windows x64、バージョン0.1.0。

操作項目は合成モード（Normal／Multiply）、色、不透明度、角度、距離、チョーク、サイズ、Render Device（Auto／GPU／CPU）です。角度は光が来る方向（0°＝右、90°＝上）。距離とサイズは表示上の正方ピクセル換算で、縮小解像度とピクセル縦横比を反映します。

Autoはwgpuで実GPUを優先し、利用できなければCPUへ切り替えます。GPU指定時は失敗を明示します。通常のRender／SmartRender内部でGPUを使用する方式で、AE専用のSmartRenderGPU APIには対応していません。チョーク・ぼかし・影の移動と合成をGPUで計算します。

## ビルド

このフォルダーでRust/MSVCを使用します。

```powershell
./build.ps1
./verify.ps1
```

成果物は `dist/InnerShadow.aex` です。インストールは行いません。`verify.ps1` は実GPUを必須としてテストし、CPU/GPU比較画像を `verification/` に生成します。

## 仕様と制限

- AE境界はストレートRGB。16bitの白は32768、32bitのHDR・負のRGB値を保持します。
- 画像外は透明扱いのため、画面全体を覆う不透明レイヤーでも画像端に内側の影が付きます。
- 逆アルファを用いるため、半透明部分にも影が付きます。距離とサイズが両方ゼロでも半透明部分は暗くなります。不透明度ゼロなら元画像と同一です。
- GPU描画はデバイスごとに同期します。大きな処理領域はメモリを多く使用し、上限超過時はエラーになります。
- Adobe標準のインナーシャドウとのピクセル単位の一致は保証しません。
- Noise、Contour、Global Light、影のみの出力は未対応です。
- 実際に確認した経路と未検証範囲は[検証記録](docs/verification.md)を参照してください。

[GPU対応設計](docs/design.md) · [English](README.md)
