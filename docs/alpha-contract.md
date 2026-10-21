# AE のアルファ処理と実装時の確認

AE エフェクトの入出力は **ストレート RGB + alpha** とする。8 bit は255、AE 16 bit は32768が1.0。float の RGB は HDR・負値を理由なく丸めない。チャンネル順（AE ARGB / 転送 RGBA）とアルファ形式は別の仕様。

根拠: [ProEXR 開発元マニュアル](https://www.fnord.com/ProEXR/ProEXR_Manual.pdf) PDF 23ページの AE 内部表現と抽出バッファの説明、および Adobe SDK `AE_EffectCB.h` の `PF_BEGIN_SAMPLING` / `PF_END_SAMPLING` にある `PF_MF_Alpha_STRAIGHT`。一般的な合成ライブラリの規約を AE にそのまま当てはめない。

## 実装の順序

1. 実装前に、入力・内部処理・出力それぞれのアルファ形式を明記する。内部で premultiplied に変換してぼかす場合は、戻す境界と alpha=0 の扱いも決める。
2. エフェクトの役割から期待値を決める。生成色の RGB は coverage / opacity に依存しない。マスクのみなら RGB を維持して alpha だけを変える。色調補正は同じ RGB なら alpha によらず同じ補正結果。複数画像の over 合成は、内部の乗算済み合成結果を最終 alpha で戻して AE に返す。
3. 不透明画像だけでなく alpha=0,1/255,0.125,0.5,1 を含む入力を使う。白・黄・色付きの半透明画素、透明部の色、16 bit の低ビット、float HDR/負値を含める。
4. GPU と CPU の一致だけで合格にしない。同じ誤りを共有できるため、仕様から独立に計算した期待値にも照合する。変換の往復テストだけでなく、読み取りと書き込みを別々に検査する。
5. ぼかし・変形は「不透明な白＋透明な黒／別色」で確認する。内部で alpha-weighted な処理が必要。単に RGB の乗算を全部消す修正はしない。
6. AEXCompat で実装済みの Render / SmartRender × 8/16/32 bit の生データを比較する。未対応の経路は明示し、合格件数に含めない。PNG の表示だけでは判定しない。
7. ストレートとして白・黒・灰色・市松に合成する。生成色の白なら、白背景上で部分透明の縁が暗くならない。テスト自身が誤った premultiplied 出力を確実に落とすことも確認する。
8. 成果物 SHA256、ソース状態、入力・パラメータ、深度・経路、期待値、実行コマンドを残す。古いバイナリの合格を今のソースの証拠にしない。

## 共通の検査

```powershell
python -m unittest discover -s tools -p test_alpha_contract.py
python tools/alpha_contract.py --raw output.rgba16le --format rgba16le --contract constant-color --color 1 0.8666666666666667 0.2
python tools/alpha_contract.py --raw output.rgba32f-le --format rgba32f-le --contract identity --reference input.rgba32f-le
python tools/alpha_contract.py --raw output.rgba16le --format rgba16le --contract alpha-only --reference input.rgba16le
```

この検査はパックされた RGBA 生データが対象。AE の stride 付き ARGB バッファは、チャンネル順を変えてから渡す。幅・高さ・stride の正しさ、期待する alpha 形状は各プラグインの検査で別途確認する。`constant-color` は生成エフェクト用であり、任意のグロー・色調補正にそのまま適用しない。`alpha-only` は RGB 不変条件だけを検査する。空画像や不透明のみの画像では成功しない。

## テンプレートとモノレポの役割

- 新規実装の仕様と検査ツールの正本: `onmk-aeplugin-rust-template` の `docs/alpha-contract.md` / `scripts/alpha_contract.py`。
- モノレポは同じツールを `tools/alpha_contract.py` に固定して保持し、実装ごとの入力・期待値・AEXCompat 実行方法を追加する。Git submodule や全プラグインの一括移行は不要。
- 仕様変更時は両リポジトリへ明示的に反映し、`--compare-tool <もう一方のスクリプト>` で一致を確認する。GitHub から最新コードを毎回自動取得して実行しない。
- 共通ツールの CI 成功は、そのツールのテスト成功にすぎない。各 AEX の透明画素検査の代わりにしない。

現状の未修正箇所は [2026-10-16 の監査](alpha-audit-20261016.md) を参照。既存の premultiplied 前提のテストは、正しい期待値に置き換えてから修正する。
