# ONMK PathArray — macOS ビルド手順

## 前提条件

1. **Xcode Command Line Tools**
   ```bash
   xcode-select --install
   ```

2. **Rust**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   # Intel + Apple Silicon 両ターゲット追加
   rustup target add x86_64-apple-darwin
   rustup target add aarch64-apple-darwin
   ```

3. **LLVM / libclang** (bindgen が必要とする)
   ```bash
   brew install llvm
   # .zshrc に追加:
   export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
   ```

4. **just** (タスクランナー)
   ```bash
   brew install just
   ```

5. **After Effects SDK**
   デフォルトでは `../../sdk/AfterEffectsSDK` (プロジェクトの2階層上) を参照。
   別の場所に置く場合は環境変数で指定:
   ```bash
   export AESDK_ROOT="/path/to/AfterEffectsSDK"
   ```

## ビルド

### リリースビルド (Universal Binary: Intel + Apple Silicon)

```bash
cd PathArrayRust
just release
```

これで以下が実行される:
1. `cargo build --release` で x86_64 と aarch64 を個別ビルド
2. `lipo` で Universal Binary を作成
3. `.plugin` バンドルを構築 (`target/release/PathArray.plugin/`)
4. PiPL リソース・PkgInfo・Info.plist をバンドル内に配置
5. コード署名 (Apple Development 証明書があれば使用、なければ ad-hoc)
6. `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/` にインストール

### デバッグビルド (ネイティブアーキのみ)

```bash
just build
```

### インストールをスキップ

```bash
NO_INSTALL=1 just release
```

出力先: `target/release/PathArray.plugin/`

## 出力ファイル構造

```
PathArray.plugin/
  Contents/
    Info.plist           (バンドル情報, CFBundleIdentifier: com.onmk.PathArray)
    PkgInfo
    MacOS/
      PathArray           (Universal Binary: x86_64 + arm64)
    Resources/
      PathArray.rsrc      (PiPL リソース)
```

## コード署名について

- AE/PR 25.2 以降、macOS では署名済みプラグインが必要
- Apple Development 証明書がキーチェーンにあれば自動で使用される
- なければ ad-hoc 署名 (`codesign --sign -`) が適用される
- 配布用には Apple Developer Program の証明書が必要

## トラブルシューティング

### `AESDK_ROOT` が見つからない
```bash
export AESDK_ROOT="/path/to/AfterEffectsSDK"
```

### `libclang` が見つからない
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

### `lipo` エラー
両ターゲットが追加されているか確認:
```bash
rustup target list --installed | grep apple
# x86_64-apple-darwin と aarch64-apple-darwin の両方が必要
```

### 署名エラーで AE がプラグインを読み込まない
```bash
# ad-hoc 再署名
codesign --options runtime --timestamp -strict --sign - target/release/PathArray.plugin
# インストール
sudo cp -rf target/release/PathArray.plugin "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
```

