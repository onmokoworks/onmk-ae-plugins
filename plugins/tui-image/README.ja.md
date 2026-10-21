# TuiImage

GPU出力の8bit変換を切り捨てからCPUと同じ四捨五入へ修正しました。色が場所ごとに異なる画像で、Mono・Source・GradientとSource Mix 0／0.37／1の9条件を実GPUで比較し、CPUとバイト単位で一致しています。全設定での一致を保証するものではありません。

実験的GPU経路は、キャッシュ全体の要求バッファ量を256MiB以内に制限し、古いものから解放して確保します。1件で上限を超える場合は既存のCPU経路へ戻します。ドライバ内部の管理領域・遅延解放・CPUメモリは含まず、実VRAM使用量の上限を保証するものではありません。

実GPUテストでは、テスト時だけ上限を2フレーム分にし、6つの別スレッドを順番に実行しました。キャッシュ件数が上限以内に収まり、各出力がCPUとバイト単位で一致することを確認しています。同時実行や実VRAMの解放まで確認したものではありません。

実GPUの回帰テストには、入力輝度OFF・入力色保持・字形サイズと間隔・4プリセットの9段階グレー・エッジ強調・解像度変更も含みます。さらに色付き画像でプリセットとコントラスト・ガンマ・反転の12条件を比較し、CPUへのフォールバックなしでバイト単位の一致を確認しました。全設定の組合せ・デバイス障害・AEのネイティブGPU画像対応を保証するものではなく、GPU経路は引き続き実験的です。シェーダーの予約語による起動エラーも修正しました。

画像変換はARGB8形式・寸法・行幅・ポインタを読書き前に検査します。行コピーは全範囲を検査してから書き込み、負の行幅にも対応します。出力サイズ不一致はエラーにしますが、部分領域描画への対応を意味しません。入力フレームの確保失敗もエラーで返します。

描画入口と最近傍リサイズは入力長・サイズ計算を検査し、リサイズ用メモリの確保失敗をエラーで返します。空画像はCPU／GPU描画前に処理を終えます。すべてのメモリ確保やAE画像バッファの検査が完了したという意味ではありません。

CPUの並列処理は読み取り専用の入力を共有し、同一画像2枚分の複製を省きます。1920×1080のARGB8では計算上16,588,800バイトの確保削減です。プロセス全体やGPUメモリの実測削減量ではありません。

Smart Renderは出力取得・パラメータ読取りの失敗やRustのパニック時も、取得済み入力画像の返却を試みます。描画と返却の両方が失敗した場合は描画エラーを優先します。単体テストは後始末の処理を検証するもので、実AEの障害注入試験ではありません。

[日本語](./README.ja.md) | [English](./README.md)

![TuiImage demo](./docs/tuiimage-demo.gif)

TuiImage は、画像レイヤーから ANSI art / TUI 風のセル表現を作る After Effects エフェクトです。

入力画像をグリッド状にサンプリングし、明るさを手続き的な glyph 密度に変換して、Mono / Source / Gradient の色モードで再構成します。ANSIエスケープシーケンスを出力するものではなく、After Effects 上で見た目を再現するためのエフェクトです。

> 仕様、UI、パラメータ名、既定値は今後変わる可能性があります。

## 名前

- 表示名: `TuiImage`
- After Effects match name: `TuiImage`
- プラグインファイル名:
  - Windows: `TuiImage.aex`
  - macOS: `TuiImage.plugin`

## 主な機能

- 入力レイヤーのセル単位サンプリング
- ANSI art / TUI 風、ASCII / Block / Braille 風の手続き的な glyph 表現
- ASCII Classic / Block / Braille / TUI Gradient プリセット
- セルサイズ、glyphスケール、字間、行間の調整
- Mono / Source / Gradient の色モード
- GPU レンダリング(実験的、環境変数を設定した場合のみ有効)

## 検証状況

- Adobe After Effects 2025 / Windows で検証
- Adobe After Effects 2026 / Apple silicon mac で検証
- リリース用バイナリは、同じ commit / tag から各プラットフォームごとに生成します

## ビルド

Windows では、リポジトリルートで実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

生成物:

```text
rust\target\release\TuiImage.aex
```

macOS では、Apple Silicon Mac 上で以下を使います。

```bash
bash ./scripts/build_macos_release.sh
```

生成物:

```text
rust/target/release/TuiImage.plugin
```

## インストール

### リリース版を使う場合

Windows では、GitHub Release から `TuiImage.aex` をダウンロードし、After Effects を閉じてから、以下の plug-ins フォルダへコピーします。

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

macOS では、GitHub Release から `TuiImage-macos-arm64.plugin.zip` をダウンロードして展開し、After Effects を閉じてから、`TuiImage.plugin` を以下の MediaCore フォルダへコピーします。

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

コマンドでコピーする場合:

```bash
unzip TuiImage-macos-arm64.plugin.zip
sudo cp -R TuiImage.plugin "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
```

GitHub からダウンロードした `.plugin` が macOS の Gatekeeper でブロックされる場合は、必要に応じて quarantine 属性を外します。

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/TuiImage.plugin"
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

macOS のローカルビルドを手動で入れる場合は、生成された `rust/target/release/TuiImage.plugin` を MediaCore フォルダへコピーします。

生成した `.aex` / `.plugin` は Git には含めません。配布時は GitHub Release の成果物として添付します。

macOS 用リリースアーティファクトは Apple Silicon / arm64 用です。現状は ad-hoc 署名で、Developer ID 署名や notarization は行っていません。

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

macOS / bash:

```bash
export TUIIMAGE_ENABLE_GPU=1
```

## 制限

明示実行する実GPUテストでは、同じサイズで入力と設定を変更し、元に戻す検査も行います。
セルサイズとプリセットの16組でCPUとの一致、設定を戻した後のバイト単位の一致を
確認しています。同じGPUインスタンスの逐次再利用の検証であり、AEの同時MFRや
ネイティブGPU画像形式の保証ではありません。実験的GPU経路の既定値は変更していません。

- 現状は本物のフォントを描画していません
- ASCII / Block / Braille は手続き的な glyph 風表現です
- 任意フォント選択は未実装です
- GPU 経路は実験的です

## License

MIT License です。詳細は [LICENSE](LICENSE) を参照してください。
