# FrameSlice

[English](./README.md) | [日本語](./README.ja.md)

![FrameSlice demo](./docs/frameslice-demo.gif)

FrameSlice は、映像を時間方向にスリットスキャンする Adobe After Effects 用エフェクトです。

各ピクセルごとに異なる時刻のフレームを参照し、横方向・縦方向・放射状・マップレイヤーによる時間のずれを作ります。

参照時刻の丸めを整数演算に変更しました。従来は64フレーム範囲・7サンプル・過去方向のindex5で、
半フレーム境界の誤差により本来−11の箇所を−10としていました。方向・サンプル数・ホスト時刻の上下限を
含む1,200条件を整数の参照式と比較しています。現行AEXCompatの宣言式フィクスチャは単一画像と時刻情報のみで、
時刻ごとの画像列を指定できません。静止画のホスト検査を、実際の動画からの時間参照の保証とは扱いません。

時刻に応じて異なる画素を返す疑似フレームを使い、修正した時刻からCPU・実GPUの画素選択までつなぐ検査も追加しました。
3方向と最近傍／線形補間のCPU6条件・GPU6条件で、明示した参照時刻・画素と完全一致しています。
実AEの時間方向のレイヤー取得やGPUワールドの受け渡しを検証したものではありません。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

## Name

- 表示名: `FrameSlice`
- After Effects match name: `FrameSlice`
- プラグインファイル名:
  - Windows: `FrameSlice.aex`
  - macOS: `FrameSlice.plugin`

## Main Features

- 入力レイヤーを時間方向にサンプリングするスリットスキャン処理
- 横方向、縦方向、放射状、マップレイヤーによる時間マップ
- 過去方向、未来方向、過去と未来の両方向に対応
- 最近傍または線形補間によるフレーム間サンプリング
- 元映像とのミックス量調整
- `wgpu` による内部 GPU 処理と CPU フォールバック
- After Effects Smart Render 対応

## Validation Status

- Adobe After Effects 2025 / Windows で動作確認済み
- 現在の実装は 8-bit ARGB のみ対応
- Deep Color / 32-bit float pixel format は未対応
- After Effects ネイティブの `SmartRenderGpu` パスは未実装
- 時間方向・端点・時刻上限・異なるフレームの選択と補間・切り抜きフレームの配置・空マップを単体テスト5件で検査しています。未来の時刻が負値へ巻き戻る不具合を修正し、時間依存をWideTimeInputで宣言しました。サイズが異なる参照フレームは原点を合わせ、外側を透明にして処理します。マップ未指定・空領域の場合は横方向の時間グラデーションへ戻し、指定された黒画像とは区別します。
- 静止入力のAEXCompat検査22件が通過しています。ホストから時刻別に異なる画像を取得する処理、時間依存キャッシュ、補助マップ、フレーム間の領域変化は未検証です。

## Build

Windows では、リポジトリルートで以下を実行します。

```powershell
cargo build --release
```

出力:

```text
target\release\frame_slice.dll
```

After Effects にインストールする場合は、生成された DLL を `FrameSlice.aex` としてコピーします。

## Installation

### Using a local Windows build

After Effects を閉じてから、管理者権限の PowerShell で以下を実行します。

```powershell
cargo build
Copy-Item -Force target\debug\frame_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\FrameSlice.aex"
```

Release build を配置する場合:

```powershell
cargo build --release
Copy-Item -Force target\release\frame_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\FrameSlice.aex"
```

コピー先:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

After Effects を再起動し、`Time > FrameSlice` からエフェクトを適用します。

生成された `.aex` / `.dll` などのビルド成果物は Git にコミットしません。

## Parameters

- `Time Frames`: 時間方向に参照するフレーム範囲、2-1000
- `Frame Step`: サンプリングするフレーム間隔、1-100
- `Time Direction`: `Past` / `Future` / `Both` を選択
- `Slice Mode`: `Horizontal` / `Vertical` / `Radial` / `Map Layer` を選択
- `Gradient Phase`: 自動生成される時間マップのオフセット
- `Center`: `Radial` モードの中心位置
- `Interpolation`: `Nearest` / `Linear` の補間方式
- `Mix with Original`: 処理結果と元映像のミックス量
- `Time Map`: `Map Layer` モードで使用する参照レイヤー
- `Invert Map`: 時間マップの白黒反転

## Development Checks

```powershell
cargo fmt
cargo check
cargo test
cargo build --release
```

`cargo test` は現在 0 件です。

## Environment Variables

ビルド時には Adobe After Effects SDK が必要です。

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
```

ビルド前に、自分の環境に合わせて `AESDK_ROOT` を設定してください。

## Limitations

- 8-bit ARGB のみ対応
- Deep Color / 32-bit float rendering は未対応
- After Effects ネイティブ GPU render path は未対応
- 旧`Render`経路でも時間方向の画像とTime Mapを取得し、Smart Renderと同じ時刻計算・位置合わせ・合成処理を使います。画像がない場合は現在の入力で補い、取得エラーは返します。異なる時刻の画素を使う回帰テストはありますが、静止画のAEXCompat検査だけではAE実機の時間方向の取得は証明できません。
- GPU単体テストは`cargo test --release --lib -- --test-threads=1`で順次実行してください。監査では既定の並列実行が停滞し、順次実行では全件成功しました。GPUモジュールの8件を2並列で実行した検査も成功していますが、既定の並列実行が止まった原因やAEのMFR動作は未確認です。
- 現時点の確認環境は Windows / After Effects 2025 です
- 単体・実GPUテスト8件と静止入力のAEXCompat検査を追加しました。GPUバッファ・ディスパッチ上限を割り当て前に確認し、超過時はCPUへ戻します。参照枚数が減った場合は大きなGPUキャッシュを縮小します。Nearest補間の中間値でCPUとGPUが別フレームを選ぶ問題と、補間後のMixで丸める段階が異なる問題も修正しました。総VRAM・CPUメモリ上限、デバイス喪失、時間変化するホスト入力の検証は残っています。

## License

MIT License. See [LICENSE](LICENSE).
