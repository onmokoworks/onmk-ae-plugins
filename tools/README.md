# Repository tools

共通のビルド・検証・監査ツールです。プラグインのビルド方法は各READMEを参照してください。

| ツール | 用途 |
|---|---|
| `alpha_contract.py` | 生RGBAデータをアルファ契約に照らして検査 |
| `alpha_source_probes.py` | 本番ソースから抽出した関数の独立検査 |
| `quality_audit.py` | パッケージ・ソース一覧を収集（ビルドや合格判定はしない） |
| `quality_smoke.py` | 品質スモーク検査。対象・引数は `--help` を参照 |
| `quality_evidence.py` | 検証証拠の取り扱いを支援 |
| `quality_coverage.json` | 品質検査の対象情報 |

```powershell
python -m unittest discover -s tools -p "test_*.py"
```

このコマンドはツール自体のテストです。AE実機での全プラグイン検証ではありません。検出用プローブの非ゼロ終了は、実際の検出内容を確認して扱ってください。

[AEアルファ契約](../docs/alpha-contract.md) / [2026-10-16時点の監査](../docs/alpha-audit-20261016.md)。監査の記述は当時の状態であり、現在の修正・検証は各プラグインの記録を参照してください。

These tools support source inventory and evidence-based quality checks. Passing tool tests or source probes is not a substitute for native per-plugin AE validation.
