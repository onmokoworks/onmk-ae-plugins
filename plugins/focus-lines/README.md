# ONMK FocusLines（集中線）

漫画・アニメの集中線を生成する After Effects エフェクト（v0.1.2、開発中）。
焦点へ向かって鋭く細くなる線、楕円の空白、線ごとの固定乱数、束ね、空白扇形、
Hold Frames による時間変化を持つ。出力は透明背景（Lines Only）または元レイヤー上の
straight 合成（Over Source）。設計: [docs/design-gpu.md](docs/design-gpu.md)。

## バックエンド

`Render > Backend`:

| 値 | 動作 |
|---|---|
| Auto（既定） | 私有 wgpu デバイスで coverage を計算。GPU が使えない・エラー時は CPU に切り替え、理由を About に表示 |
| GPU | 私有 wgpu デバイスのみ。失敗時はレンダーエラー（黙って CPU にしない） |
| CPU | CPU 基準実装。GPU を初期化しない |

AE ネイティブ GPU レンダー（SmartRenderGpu）は宣言していない。AE からは CPU エフェクトとして
扱われ、GPU 計算は本プラグイン内部で完結してアップロード／ダウンロードを伴う。

`Render > Antialiasing`: Standard = 1画素あたり8走査線、High = 16走査線。水平方向は解析的、
垂直方向はサンプリング（厳密な面積ではない）。重なりは図形の和集合で評価する。

## 主なパラメータ（単位）

- Focus: Focus Point（レイヤー px、画面外可）、Center Size（H/2 に対する %）、Center Aspect、
  Center Rotation、Center Offset X/Y（楕円半径に対する %、長さ 95% に放射クランプ）。
- Lines: Line Count（0–5000、小数で最後の1本がフェード）、Line Width（全解像度 px）、
  Width Randomness、Tip Width（%）、Taper Length（H/2 に対する %、0 で一定幅）、Taper Shape、
  Length Randomness（H/2 に対する %）。
- Distribution: Random Seed、Angle Jitter（°）、Clustering、Cluster Count、Cluster Spread、
  Density Variation、Gap Direction（0° = 上、時計回り）、Gap Width（0 = 無効、360 = 全非表示）、
  Gap Softness（0 = 段階）。
- Evolution: Static / Hold Frames（整数フレーム、レンダー時刻 ÷ フレーム長の floor）、
  Change Amount、Stagger。
- Color & Composite: Color、Intensity（32 bpc では RGB を clamp しない）、Opacity、Composite。

Seed、Cluster Count、Evolution、Hold Frames、Backend、Antialiasing はキーフレーム不可
（補間による意図しないちらつきを防ぐ）。

## 調整の出発点（1920×1080 基準、以下の表は見た目未調整）

| 名前 | 主な値 |
|---|---|
| 標準集中線 | 既定値（Count 220, Width 9, Center 45%, Aspect 1.6） |
| 強烈フォーカス | Count 480, Width 6, WidthRand 60, Center 28, Aspect 1.4, LenRand 20, Clustering 35, Clusters 16 |
| まばら・静寂 | Count 70, Width 14, WidthRand 30, Center 55, LenRand 50, Clustering 0, Density 50 |
| アニメ白フラッシュ | Color 白, Count 300, Width 7, Center 50, Aspect 1.78, Hold Frames 2, Change 100 |
| ガタつき3コマ | Hold Frames 3, Change 35 |
| 片側抜き | Gap Direction 180, Gap Width 120, Gap Softness 30 |

4K では Line WidthとShort Linesの長さを約2倍にする（線幅は px、構図系は % 単位）。

## ビルドと検証（このディレクトリ内）

```powershell
cargo fetch --locked
cargo fmt --check
cargo test --offline --locked
cargo run --release --offline --locked --example smoke -- --backend both --out out/smoke
powershell -ExecutionPolicy Bypass -File .\build.ps1
python verify_pipl.py
```

`build.ps1` は `dist/ONMK_FocusLines.aex` を作り SHA256 を表示する。インストールはしない。
smoke はコアのみの描画で、AE ホストの証拠ではない。検証状況は [VALIDATION.md](VALIDATION.md)。

## 丸い先端と短い線

`Tip Cap: Round` は中心側の先端を丸める。Tip Widthで先端幅、Taper Lengthで外側へ太くなる区間を調整。

`Short Lines` は通常線の一部を、画面端から指定した長さの線に置き換える。

| パラメータ | 内容 |
|---|---|
| Minimum Length (px) | 短い線の最短 |
| Maximum Length (px) | 短い線の最長 |
| Mix (%) | 短い線にする確率。既定値0で従来描画 |

最小・最大が逆なら内部で並べ替える。線の選択はSeedとIDに固定し、Hold Framesで長さを再抽選。中央の空白・Gapを優先するので線が短縮・非表示になる場合があり、可視本数の比率はMixと厳密には一致しない。短い線にはLength Randomness・Density Variationの代わりに専用範囲を使い、太さは共通設定。

[調整済みパラメータ](docs/presets/parameters.md) / [GPU描画GIF](docs/presets/short-lines.gif)。

## 導入と検証範囲

Windowsで生成した `dist/ONMK_FocusLines.aex` をAdobeの共有MediaCoreフォルダーに配置します。標準パスは `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore` です。管理者権限が必要な場合があります。AE起動後は `ONMK > FocusLines` を探してください。同じプラグインの重複配置は避けてください。

v0.1.2はCPU/GPUの自動テストとAEXCompat Smart32を確認済みです。AE実機での読み込み・描画、macOSは未検証です。詳しい範囲は [VALIDATION.md](VALIDATION.md)。PiPL検査スクリプトにはPythonの `pefile` が必要です。

## Preview

![短い線を混ぜた集中線](docs/presets/short-lines.gif)

[パラメータ](docs/presets/parameters.md)。最短30px・最長120px・Mix 40%、3フレームごとに更新。
