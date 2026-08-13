# UVProject

[English](./README.md) | [日本語](./README.ja.md)

![UVProject demo](./docs/uv-project-demo.gif)

UVProject は、UV / ST マップを使ってテクスチャを投影する Adobe After Effects 用エフェクトです。

エフェクトは **UV / ST マップ**と**テクスチャ**のどちら側にも適用できます。
`Input Is` で適用先の役割を選び、`Other Layer` にもう一方のレイヤーを指定します。
UV マップの赤チャンネルを U、緑チャンネルを V として各ピクセルの座標を読み取り、
テクスチャをその位置からサンプリングしてUVレイアウトへ焼き込みます。
3D の UV パスを 2D の素材へ貼り戻す、いわゆる「STMap」リマップと同じ仕組みです。

> 仕様、UI、パラメータ名、初期値は今後変更される可能性があります。

## Name

- 表示名: `UVProject`
- After Effects match name: `UVProject`
- プラグインファイル名:
  - Windows: `UVProject.aex`
  - macOS: `UVProject.plugin`

## 使い方

1. UV マップ側に適用する場合は `Input Is` を `UV Map` にし、`Other Layer` に
   テクスチャを指定します。
2. テクスチャ側に適用する場合は `Input Is` を `Texture` にし、`Other Layer` に
   UV マップを指定します。
3. Blender由来のUVパスでは32 bpcのOpenEXRとAEの32 bpcプロジェクトを推奨します。

## Main Features

- STMap 方式の UV リマップ: 任意のテクスチャレイヤーを UV / ST パス経由で投影
- UVマップ側／テクスチャ側のどちらにエフェクトを適用するか切替可能
- 8 / 16 / 32 bpc対応
- アンチエイリアスされたUVマップの半透明エッジをアンプリマルチして座標を復元
- V 軸原点の切替: 上原点（After Effects / 画像座標）または 下原点（Nuke / 3D）
- テクスチャサンプリングの Wrap: Clamp / Repeat / Mirror
- Bilinear / Nearest サンプリング
- UV マップ自身のアルファをカバレッジマスクとして使用（ジオメトリの無い箇所は透明）
- Opacity
- After Effects Smart Render 対応

## Validation Status

- Adobe After Effects 2025 / Windows で動作確認
- 8-bit ARGB、16-bit ARGB、32-bit float ARGBに対応
- 8 bpcではUV座標が256段階になるため、BlenderのUVパスには32 bpcを推奨
- CPU 実装。ネイティブ `SmartRenderGpu` パスは CPU にフォールバック
- UV精度と半透明エッジ復元の自動テストあり

## Build

Windows では、リポジトリルートで以下を実行します。

```powershell
cargo build --release
```

出力:

```text
target\release\uv_project.dll
```

After Effects にインストールする場合は、生成された DLL を `UVProject.aex` としてコピーします。

## Installation

### Using a local Windows build

After Effects を閉じてから、管理者権限の PowerShell で以下を実行します。

```powershell
cargo build
Copy-Item -Force target\debug\uv_project.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex"
```

Release build を配置する場合:

```powershell
cargo build --release
Copy-Item -Force target\release\uv_project.dll "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex"
```

コピー先:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

After Effects を再起動し、`Distort > UVProject` からエフェクトを適用します。

生成された `.aex` / `.dll` などのビルド成果物は Git にコミットしません。

## Parameters

- `Other Layer (Texture / UV Map)`: `Input Is` で選んだ適用先と反対側のレイヤー。
- `Input Is`: エフェクトの適用先がUVマップかテクスチャかを指定。
- `V Origin`: `Top (After Effects)` または `Bottom (Nuke / 3D)`。3D・レンダー由来の
  UV パスは下原点（既定）、画像座標系のマップは上原点を使います。
- `Wrap`: 0..1 を超えるテクスチャ座標の扱い — `Clamp` / `Repeat` / `Mirror`。
- `Sampling`: `Bilinear`（補間あり）または `Nearest`（補間なし）。
- `Use UV Alpha as Mask`: UV マップのアルファをカバレッジに掛け、ジオメトリの無い
  箇所を透明に保ちます。
- `Opacity`: 全体の強さ 0–100%。

## Development Checks

```powershell
cargo fmt
cargo check
cargo build --release
```

## Environment Variables

ビルド時には Adobe After Effects SDK が必要です。

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
```

ビルド前に、自分の環境に合わせて `AESDK_ROOT` を設定してください。

## Limitations

- 8 bpcのUVマップは座標量子化による段差が出るため16/32 bpcを推奨
- CPU のみ（ネイティブ GPU render path は未対応）
- 現時点の確認環境は Windows / After Effects 2025 です

## License

MIT License. See [LICENSE](LICENSE).
