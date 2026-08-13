# Changelog

このファイルは [Keep a Changelog](https://keepachangelog.com/) と
[Semantic Versioning](https://semver.org/lang/ja/) に概ね従います。

## [Unreleased]

### Added
- 初期実装

## [1.0.0] - 2026-06-05

### Added
- UVProject 初版リリース
- UV / ST マップ（R=U, G=V）を読み、テクスチャレイヤーを UV 座標へ投影
- V 軸原点の上/下（AE / Nuke・3D）切替
- Wrap モード（Clamp / Repeat / Mirror）
- サンプリング（Bilinear / Nearest）
- UV マップのアルファをカバレッジマスクとして使用するオプション
- Opacity
- Smart Render 対応（8-bit ARGB）
