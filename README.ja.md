# TuiImage

[日本語](./README.ja.md) | [English](./README.md)

![TuiImage demo](./docs/tuiimage-demo.gif)

TuiImage は、画像レイヤーから ANSI art / TUI 風のセル表現を作る After Effects エフェクトです。

入力画像をグリッド状にサンプリングし、明るさを手続き的な glyph 密度に変換して、Mono / Source / Gradient の色モードで再構成します。ANSI エスケープシーケンスを出力するものではなく、After Effects 上でその見た目を再現するためのエフェクトです。

> この README は作業中の簡易版です。仕様、UI、パラメータ名、既定値は今後変わる可能性があります。

## 名前

- 表示名: `TuiImage`
- After Effects match name: `TuiImage`
- プラグインファイル名: `TuiImage.aex`

## 主な機能

- After Effects 用ネイティブエフェクト
- 入力レイヤーのセル単位サンプリング
- ANSI art / TUI 風、ASCII / Block / Braille 風の手続き的な glyph 表現
- ASCII Classic / Block / Braille / TUI Gradient プリセット
- セルサイズ、glyph スケール、字間、行間の調整
- Mono / Source / Gradient の色モード
- CPU レンダリングを標準使用
- GPU 経路は実験的で、環境変数を設定した場合のみ有効

## 検証状況

- Adobe After Effects 2025 / Windows で検証中
- macOS ビルドと検証は別環境で実施予定
- リリース用バイナリは、同じ commit / tag から各プラットフォームごとに生成します

## ビルド

リポジトリルートで実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

生成物:

```text
rust\target\release\TuiImage.aex
```

macOS では、実験的なローカルビルド補助として以下を使います。

```bash
bash ./scripts/build_macos_release.sh
```

生成物:

```text
rust/target/release/TuiImage.plugin
```

## インストール

### リリース版を使う場合

GitHub Release から `TuiImage.aex` をダウンロードし、After Effects を閉じてから、以下の plug-ins フォルダへコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

After Effects を再起動し、`Stylize > TuiImage` から適用します。

PowerShell スクリプトの実行が分かりにくい場合は、この手動コピーで問題ありません。

### ローカルビルドをインストールする場合

After Effects を閉じ、管理者権限の PowerShell で実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install_tuiimage_admin.ps1
```

標準では以下にコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

生成した `.aex` は Git には含めません。配布時は GitHub Release の成果物として添付します。

macOS 用ビルド補助は Apple Silicon のローカル検証用です。After Effects 上でのホスト検証は別途必要です。

## パラメータ

- `Preset`: glyph スタイルと初期値を選びます
- `Cell Width` / `Cell Height`: グリッドの細かさを決めます
- `Uniform Cell Size`: Cell Width / Height を連動します
- `Scale`: セル内の glyph サイズを調整します
- `Column Gap %` / `Row Gap %`: 字間・行間に相当する余白を調整します
- `Color Mode`: Mono / Source / Gradient を選びます
- `Source Mix`: 生成結果を元レイヤーへ混ぜます

## 環境変数

GPU 経路は実験中で、標準では無効です。試す場合だけ設定します。

```powershell
$env:TUIIMAGE_ENABLE_GPU = "1"
```

## 制限

- 現状は本物のフォントを描画していません
- ASCII / Block / Braille は手続き的な glyph 風表現です
- 任意フォント選択は未実装です
- GPU 経路は実験的です

## License

MIT License です。詳細は [LICENSE](LICENSE) を参照してください。
