# Mac 対応 & GPU 対応 タスク整理

現状: Windows CPU (rayon 並列) のみ。`build.rs` には Mac / GPU フラグは入っているが、実装・検証は未着手。

---

## 1. Mac 対応

### 前提確認
- [ ] ビルド対象 Mac のアーキテクチャ (Intel x86_64 / Apple Silicon arm64 / 両方 universal binary?)
- [ ] 手元に実機またはビルド用 Mac はあるか (クロスコンパイルだけだと署名ができない)
- [ ] AE のバージョン (macOS 版 AE 2024 / 2025 のどれを最低ラインにするか)
- [ ] 配布方法 (自分用ローカルインストールだけか、他人に配るか)
  - 他人に配る場合は Apple Developer ID 署名 + 公証 (notarization) が必要

### 既に対応済みの箇所
- `build.rs`: `Property::CodeMacIntel64("EffectMain")`, `Property::CodeMacARM64("EffectMain")` は設定済み
- `justfile` → `AdobePlugin.just` を import している。この共通 justfile が .plugin バンドル生成レシピを持っているはず (要確認)

### コード側の作業
- [ ] `AdobePlugin.just` の内容を読み、`just build-mac` 相当のレシピがあるか確認
- [ ] rayon は macOS でそのまま動くので変更不要
- [ ] `#[cfg(target_os = "macos")]` 条件分岐が必要な箇所は今のところない想定 (要ビルド時に確認)
- [ ] `Cargo.toml` の `[profile.release]` は macOS でも共通で OK

### ビルド・パッケージング作業
- [ ] `rustup target add x86_64-apple-darwin aarch64-apple-darwin` (Mac 上で)
- [ ] `cargo build --release --target aarch64-apple-darwin` / `x86_64-apple-darwin`
- [ ] 両アーキの `.dylib` を `lipo -create -output` で universal binary に
- [ ] `.plugin` バンドル構造を作る:
    ```
    RefractionDispersion.plugin/
      Contents/
        Info.plist
        MacOS/
          RefractionDispersion      ← universal dylib
        Resources/
          RefractionDispersion.rsrc ← pipl から生成される
    ```
- [ ] `Info.plist` のテンプレート確認 (AdobePlugin.just が生成するはず)
- [ ] `PkgInfo` の確認 (`eFKTFXTC` など)

### 署名・公証 (配布する場合のみ)
- [ ] Apple Developer ID Application 証明書の取得
- [ ] `codesign --force --deep --sign "Developer ID Application: NAME" RefractionDispersion.plugin`
- [ ] 公証用 zip を作って `xcrun notarytool submit ... --wait`
- [ ] `xcrun stapler staple RefractionDispersion.plugin`

### 動作確認
- [ ] インストール先: `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/`
- [ ] AE 起動 → Effects & Presets にプラグインが出るか
- [ ] マスクシェイプ参照・移動が Windows と同じ挙動か
- [ ] 6ch / Per-axis chroma / Edge Blur が同じ見た目か
- [ ] rayon スレッド数が論理コア数に追従しているか (`RAYON_NUM_THREADS` で検証)
- [ ] Rosetta 経由の AE (Intel 版 on M1/M2) でも動くか

### リスクポイント
- `PointerOwnership` / `origin_x, origin_y` の符号が macOS で異なる挙動をしないか
- 行ストライド (`rowbytes`) が Windows と同じ符号・単位で返るか (AE SDK 的には同じはずだが実測推奨)
- Apple Silicon AE で CPU バックエンドが使われたとき、`f32` アライン不整合がないか

---

## 2. GPU 対応

### 戦略選択 (最初の決断)
3 つの方針から 1 つ選ぶ必要がある。

| 方針 | Pros | Cons | 工数感 |
|---|---|---|---|
| **A. ネイティブカーネル (CUDA + Metal + OpenCL)** | AE SDK の王道。性能最大。対応プラットフォーム広い | 3 種類のカーネルを別々に書く。ビルド・配布が重い | 3〜5日 |
| **B. wgpu + WGSL 1本化** | 単一シェーダソースで Win/Mac 両対応。Rust で完結 | AE の GPU device を直接使わず、自分で wgpu device を立てる → メモリ二重コピー発生 | 2〜3日 |
| **C. AE SDK GPU Suite に繋ぐ + 手書きシェーダ** | AE が管理する GPU device をそのまま使うのでゼロコピー。A の簡易版 | AE の `PF_GPUDeviceSuite` の Rust ラッパが未整備な可能性。低レベル FFI を書く必要 | 3〜4日 |

- [ ] どの方針で行くか決定する (推奨: まず **B (wgpu)** で動かして速度を測る → 足りなければ A/C に切り替え)

### 前提確認
- [ ] 対象 GPU (NVIDIA / AMD / Intel Arc / Apple Silicon GPU どれを想定するか)
- [ ] 最低動作 AE バージョン (GPU パイプラインは AE CC 2019 以降)
- [ ] CPU フォールバックは残すか (強く推奨: 残す)
- [ ] GPU が来るピクセル形式は何か: AE の GPU パイプラインは **BGRA f32 のみ**。 現状の ARGB u8 経路と別ルートが必要

### 共通の前作業 (どの方針でも必要)
- [ ] `after-effects` crate の `src/pf/gpu.rs` を読み、`GpuDeviceSetup` / `GpuDeviceSetdown` / `SmartRenderGpu` の Rust 側の API 形状を把握
- [ ] サンプル `examples/rust_gpu/` (wgpu + WGSL + rust-gpu) を読み、`SmartRenderGpu` 時の入力取り出し方・出力書き戻し方を確認
- [ ] `build.rs` の `OutFlags2::SupportsGpuRenderF32` は既に ON になっているので追加不要
- [ ] `lib.rs` で `Command::SmartRenderGpu { extra }` を別ハンドラに分岐 (現在は CPU にフォールバック)

### アルゴリズム GPU 化の分解
refract.rs の処理段階を GPU kernel 単位に分ける:

1. **Mask → height field** (`height = luma * alpha`)
   - 単純 per-pixel. 1 kernel
2. **Height blur (separable box blur 3-pass)**
   - 分離可能ブラー: 水平 pass → 垂直 pass × 3 回 = 6 dispatch
   - Ping-pong テクスチャ 2 枚で可
3. **Edge blur (mask_a 用)**
   - 2 と同じロジックを mask_a に適用
4. **Normals 生成 (height gradient → N)**
   - 1 kernel. per-pixel
5. **メイン refract ループ**
   - ここが一番重い。per-pixel ループで samples*3 or samples*6 回サンプル
   - 1 kernel。uniform buffer に IOR, 光源方向, strength 配列 (最大 samples=64) を積む
6. **Mask composite + mix + u8 書き戻し** (5 と統合可)

### Uniform / Storage バッファ設計
- [ ] uniform buffer: `ior_r/g/b`, `refract_power`, `chroma_x`, `chroma_y`, `samples`, `fresnel_power`, `shininess`, `diffuseness`, `light_dir` (vec3), `saturation`, `use_6ch` (u32), `mix` — 約 80 バイト
- [ ] storage buffer: `strength_r/g/b (x,y)` × samples (最大 64) = 6 * 64 * 4 = 1.5KB
- [ ] 6ch 用: `strengths_6_x/y[6][64]` = 3KB
- [ ] テクスチャ: input RGBA f32, bg RGBA f32, mask luminance R32F, height R32F (ping-pong), normals RG32F, edge_mask R32F, output RGBA f32

### 方針 A (ネイティブカーネル) を選んだ場合のタスク
- [ ] `kernels/refract.cu` (CUDA) を書く
- [ ] `kernels/refract.metal` (Metal) を書く
- [ ] `kernels/refract.cl` (OpenCL) を書く (macOS Intel 用。AMD/Intel GPU 用)
- [ ] `build.rs` を拡張:
  - Windows: `nvcc` 経由で .cu → .ptx または .cubin
  - macOS: `xcrun metal` → .air → .metallib
  - OpenCL: そのまま文字列として埋め込み
- [ ] シェーダバイナリを `include_bytes!` で埋め込み
- [ ] `GpuDeviceSetup` で現在のデバイス kind (CUDA/Metal/OpenCL) を判定し、対応する kernel をロード
- [ ] `SmartRenderGpu` で: 入力 world → GPU ポインタ取得 → kernel dispatch → 出力 world へ
- [ ] AE が渡してくる GPU device ポインタ (`PF_GPUDeviceInfo`) を使う。Rust ラッパがなければ `after-effects-sys` 経由で FFI 呼び出し

### 方針 B (wgpu) を選んだ場合のタスク
- [ ] `Cargo.toml` に `wgpu`, `pollster`, `bytemuck` を追加
- [ ] `kernels/refract.wgsl` を書く (height blur / edge blur / normals / refract main の 4 entry point)
- [ ] プラグインロード時 (GlobalSetup) に wgpu instance / adapter / device を 1 回初期化してキャッシュ (per-effect 再初期化は遅い)
- [ ] `SmartRenderGpu` (あるいは `SmartRender` 内で条件分岐) で:
  1. AE の input world 内容を staging buffer 経由で wgpu texture に upload
  2. ブラー → 法線 → メインの dispatch chain
  3. 出力 texture を staging buffer 経由で AE の output world に download
- [ ] ゼロコピーは不可能 (AE GPU device と別 wgpu device のため)。計測して帯域律速なら方針変更
- [ ] Metal バックエンド: `wgpu` は macOS で自動的に Metal を選ぶので追加作業なし
- [ ] `examples/rust_gpu/src/wgpu_proc.rs` のコードを流用できる部分は流用

### 方針 C (AE SDK GPU Suite ネイティブ) を選んだ場合のタスク
- [ ] `after-effects-sys` の `PF_GPUDeviceSuite1` bindings を確認。存在しなければ自力で bindgen
- [ ] `PF_GPU_Framework_CUDA` / `_METAL` / `_OPENCL` の enum で分岐
- [ ] CUDA/Metal コンテキストは AE が作ったものを受け取る → 自前 device 初期化不要
- [ ] 後は方針 A と同じ (kernel 本体を 3 種類書く)

### 動作確認 (共通)
- [ ] 小さい comp (256x256) で CPU 結果と GPU 結果が差分 < 1 bit の範囲で一致するか
- [ ] 1920x1080 / 3840x2160 の render 時間を CPU と比較
- [ ] samples を 16 → 64 に上げたときの GPU スケーリング (GPU は samples 大でこそ効くはず)
- [ ] GPU OOM 時の CPU フォールバックが動くか
- [ ] AE の GPU preview (Fast Draft, Fast Previews) で問題が出ないか
- [ ] メモリリーク: 100 frame ループして VRAM が単調増加しないか
- [ ] `height_blur` / `edge_blur` を大きく振ってもアーティファクトが出ないか

### リスクポイント
- **浮動小数差**: CPU (scalar f32) と GPU (IEEE 754 準拠だが FMA 融合の有無が違う) で最下位ビットが違う → 許容差を決めておく
- **bilinear サンプリング**: CPU は手書きの 4-tap。GPU は texture sampler (linear filter) で自動。UV 座標オフセット (+0.5) に注意しないと半ピクセルずれる
- **ブラー境界**: CPU はクランプ境界、GPU は sampler の address mode (`clamp-to-edge`) に揃える
- **最大 samples 64** の制約を uniform buffer 設計時に守る (可変長にすると dispatch 毎 re-upload でコスト増)
- **行ストライド**: AE の world は `rowbytes` を持ち、必ずしも `width*4*4` ではない。GPU texture と変換するときに要配慮

---

## 3. 実装順序の推奨

1. **Mac CPU 対応を先に終わらせる** (数時間〜1日)
   - GPU を入れる前に「rayon だけで Mac で動く」状態を作り、CPU レファレンス実装を 2 プラットフォームで確定
2. **GPU は wgpu (方針 B) で Win/Mac 同時対応** (2〜3日)
   - 単一シェーダで両対応できるのが最大の利点
   - 速度が足りなければ方針 C に移行
3. **方針 C への移行は必要なら** (+ 2日)
   - wgpu のコピーオーバーヘッドがボトルネックになった場合のみ

---

## 4. 未決定事項 (ユーザ確認が必要)

- [ ] Mac: Intel / Apple Silicon / universal、どれを作る?
- [ ] Mac: 配布するか (= 署名・公証をやるか)
- [ ] GPU: A/B/C どの方針で行くか
- [ ] GPU: CUDA 非搭載の Windows マシン (Intel Arc, AMD) も対象にするか
- [ ] GPU: CPU フォールバックを残すか (強く推奨: 残す)
- [ ] `SupportsGetFlattenedSequenceData` フラグを追加するか (現状の build warning 対応)
