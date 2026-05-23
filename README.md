# TimeSlice

TimeSlice は、映像を時間方向にスリットスキャンする Adobe After Effects 用エフェクトです。

各ピクセルごとに異なる時刻のフレームを参照し、横方向・縦方向・放射状・マップレイヤーによる時間のずれを作ります。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

## Name

- 表示名: `TimeSlice`
- After Effects match name: `TimeSlice`
- プラグインファイル名:
  - Windows: `TimeSlice.aex`
  - macOS: `TimeSlice.plugin`

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
- 自動テストはまだありません

## Build

Windows では、リポジトリルートで以下を実行します。

```powershell
cargo build --release
```

出力:

```text
target\release\time_slice.dll
```

`just` が利用できる場合は、以下のレシピも使えます。

```powershell
just build
just release
```

`just release` は `target\release\TimeSlice.aex` も生成します。

macOS 用の bundle 作成レシピも `AdobePlugin.just` に含まれていますが、現時点では Windows / After Effects 2025 での確認を優先しています。

## Installation

### Using a local Windows build

After Effects を閉じてから、管理者権限の PowerShell で以下を実行します。

```powershell
cargo build
Copy-Item -Force target\debug\time_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\TimeSlice.aex"
```

Release build を配置する場合:

```powershell
cargo build --release
Copy-Item -Force target\release\time_slice.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\TimeSlice.aex"
```

コピー先:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

After Effects を再起動し、`Time > TimeSlice` からエフェクトを適用します。

### Using just

`just` が入っている場合は、以下でもビルドとインストールを実行できます。

```powershell
just build
```

Release build:

```powershell
just release
```

インストールを行わず、ビルドだけ実行したい場合:

```powershell
$env:NO_INSTALL = "1"
just release
```

生成された `.aex` / `.dll` などのビルド成果物は Git にコミットしません。

## Parameters

- `Time Frames`: 時間方向に参照するフレーム範囲
- `Frame Step`: サンプリングするフレーム間隔
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

`AESDK_ROOT` が未設定の場合、`AdobePlugin.just` は `../../sdk/AfterEffectsSDK` を参照します。通常の fresh checkout では存在しないため、自分の環境に合わせて `AESDK_ROOT` を設定してください。

## Limitations

- 8-bit ARGB のみ対応
- Deep Color / 32-bit float rendering は未対応
- After Effects ネイティブ GPU render path は未対応
- レガシー `Render` 経路では Smart Render と同等のスリットスキャン処理はまだ実装していません
- macOS bundle recipe は含まれていますが、現時点の確認環境は Windows / After Effects 2025 です
- 自動テストはまだありません

## License

MIT License. See [LICENSE](LICENSE).
