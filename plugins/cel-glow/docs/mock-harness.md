# CelGlow v2 — スタンドアロン確認ハーネス設計 (mock-harness)

> 2026-07-10 起草。`design-v2.md` の補遺。
> AE を介さずにアルゴリズムだけを確認・開発するための構成。
> 「AE プラグインは薄いガワ、アルゴリズムは独立 crate」に再編し、
> CLI で PNG 入出力 + パラメータファイルで回せるようにする。

---

## 1. 狙い

- **今**: design-v2 のアルゴリズム (等高線リング + 筆質感) を
  AE のビルド/インストール/再起動サイクルなしで検証する。
  パラメータを変えて数秒で PNG を見比べたい。
- **今後**: ゴールデンテストの生成器・回帰テストのランナーを兼ねる。
  グレイン定数のチューニング (design-v2 §12) もここでやる。
- AE 固有のもの (SmartFX、checkout、ビット深度変換、パラメータ登録)
  は一切持ち込まない。

## 2. crate 再編 (workspace 化)

**v1 とは完全分離**: 旧 `rust/celglow` は凍結して一切変更しない。
v2 用に AE プラグイン crate も新規 (`celglow2`、match name
`ANTH CelGlow2`) とし、v1 の使える部分はコピーして持ち込む。

```
rust/
  Cargo.toml            # [workspace] members = celglow-core, celglow-cli, celglow2
                        #   (旧 celglow は members に入れない = 凍結)
  celglow-core/         # ★ 新規: 純アルゴリズム。AE 依存ゼロ
    src/
      lib.rs            # pub API: render(), Params, DebugView
      params.rs         # Params 構造体 (serde derive)
      field.rs          # S1: 強度抽出 + box blur ×3
      noise.rs          # hash / value noise / fbm (CPU 参照実装)
      render.rs         # S2〜S6 per-pixel 評価
  celglow-cli/          # ★ 新規: bin。画像 I/O + パラメータ読込
    src/main.rs
  celglow2/             # ★ 新規: AE プラグイン v2 (cdylib)。core の薄いガワ
    src/lib.rs          #   AE パラメータ ⇄ core::Params 変換 + SmartFX
    src/gpu.rs, shader.wgsl
    build.rs            #   PiPL: 名称 CelGlow 2 / match name ANTH CelGlow2
  celglow/              # 旧 v1: 凍結。変更・削除しない
```

- **celglow-core** は `image` にも `after-effects` にも依存しない。
  入出力は素の `&[f32]` バッファ (RGBA, straight, row-major)。
  **色空間変換は行わず、渡された値の空間のまま処理する**
  (design-v2 §2 冒頭。既定チューニングはガンマ空間前提):

  ```rust
  pub struct FrameBuf<'a> { pub w: usize, pub h: usize, pub rgba: &'a [f32] }

  pub fn render(
      input: FrameBuf,
      input_origin: (i32, i32), // 入力バッファ左上のレイヤ座標 (フル解像度)
      output_origin: (i32, i32),// 出力バッファ左上のレイヤ座標 (フル解像度)
      output_size: (usize, usize), // 出力の (w, h)。入力以下でよい
      params: &Params,          // downsample: (f32, f32) を含む
      view: DebugView,          // Result / GlowOnly / Field / Rings / TextureMask / Source
      out: &mut [f32],          // output_w*output_h*4、呼び出し側確保
  ) -> Result<(), RenderError>;
  ```

  - `input_origin` / `output_origin` は必須。AE の出力要求 rect は
    ROI・ズームでフレームごとに
    変わるため、ノイズ・リング位相は必ずレイヤ座標
    `p_layer = output_origin + p_out` で評価し、入力参照時は
    `p_in = p_layer - input_origin` に変換する (design-v2 §2/§4.4)。
    CLI は両 origin = (0,0)、output_size = input size、downsample = 1.0 固定。
  - 出力 rect 拡張 (reach) は core では扱わない。CLI は
    「入力 PNG に十分な余白を持たせる」運用とし、AE 側だけが
    PreRender で rect を拡張して大きい入力バッファを core に渡す。
    core は拡張済み入力と要求された出力 rect を別々に受け取り、出力 rect
    だけをレンダする。`output rect` が入力 rect に収まらない場合、または
    `out.len() != output_w * output_h * 4` の場合は `RenderError` を返す。
  - チャンネル順は core は RGBA。v1 の AE 変換ヘルパ
    (`layer_to_rgba_f32`) は実際には **ARGB 順**で詰めているため、
    celglow2 のアダプタでスウィズルすること (M3.5 の事故ポイント)。

- **Params** は design-v2 §3 の全パラメータを素の Rust 型で持ち、
  `serde::{Serialize, Deserialize}` + `Default` (既定値 = 仕様の既定値)
  を実装。TOML がそのまま仕様書のパラメータ表と 1:1 対応する。

- GPU (wgpu/WGSL) は当面 celglow2(AE 側)に置く。CLI は CPU パスのみ。
  M4 で GPU も core に下ろすかは、その時の parity テスト都合で判断。

## 3. CLI 仕様 (celglow-cli)

依存: `image` (PNG I/O), `serde` + `toml`, `clap`。全部 MIT/Apache。

```
celglow-cli render -i input.png -o out.png [-p params.toml] [--view rings]
             [--set rings.ring_count=32 --set wobble.amount=20 ...]
celglow-cli dump-params > params.toml        # 既定値の雛形を吐く
celglow-cli contact -i input.png -o sheet/   # 全 View を一括出力
```

- **-p params.toml**: 省略時は既定値。TOML 構造は Params のグループと同じ:

  ```toml
  [source]
  channel = "luma_x_alpha"   # alpha / luma / luma_x_alpha
   spread = 260.0
  [rings]
  ring_count = 24
  line_width = 45.0
  color_scramble = 30.0
  seed = 1
  [wobble]
   amount = 7.0
  scale = 24.0
  # ... 省略キーは既定値 (serde default)
  ```

- **--set group.key=value**: TOML 読込後に上書き。パラメータ探索を
  シェルループで回せる (`for w in 10 20 40; do ... --set wobble.amount=$w`)。
- **--view**: DebugView 選択。`contact` サブコマンドは全 View を一括出力
  — design-v2 §10 の「View が参照画像 3〜5 に対応する」検証が
  コマンド 1 発でできる。ファイル名と View の対応:

  | ファイル | DebugView |
  | --- | --- |
  | result.png | Result |
  | glow.png | Glow Only |
  | field.png | Field |
  | rings.png | Rings (raw) |
  | texture.png | Texture Mask |
  | source.png | Source Strength |

- 色空間: **既定は無変換** — PNG をデコードした 0..1 値をそのまま
  f32 で core に渡し、出力もそのまま 8bit 化する (HDR 超えは clamp)。
  リファレンスチェーンは非リニアプロジェクトの見た目なので、
  照合・チューニングは非リニアのまま行う (design-v2 §2 冒頭)。
  `--srgb-to-linear` をオプトインで用意 (リニア環境の検証用。
  EXR は当面やらない)。
- 終了時に stderr へ各ステージの所要時間を出す (性能目標 §8 の計測用)。

### 3.2 watch モード (すぐ効く開発ループ)

```
celglow-cli watch -i input.png -o out.png -p params.toml
```

`notify` crate で `params.toml` を監視し、保存のたびに再レンダ。
エディタで TOML を書き換え → 画像ビューア (自動リロードするもの、
例: Windows フォトは不可なので IrfanView / vscode のプレビュー) で
即confirm。**GUI は作らない** — この用途には watch + TOML で十分で、
egui 等を入れると mock の保守コストが本体を食う。必要になったら
v2.1 で検討。

## 4. テストとの接続

- **ゴールデンテスト (回帰検知用)**: `celglow-core/tests/golden.rs` が
  `tests/fixtures/*.png` + `tests/fixtures/*.toml` を CLI と同じ経路
  (core API 直叩き) でレンダし、`tests/golden/*.png` と比較
  (許容差: per-channel ±2/255)。ゴールデン更新は
  `celglow-cli render` で再生成してコミット。
  ※ CPU/GPU parity の判定には使わない — parity は f32 バッファ直接比較
  (99.9 percentile |Δ| < 1e-3、design-v2 §7) で別途定義。
- **決定論テスト**: 同 seed 2 回レンダで bit 一致 (design-v2 §10)。
- **原点非依存テスト**: 入力を N px パディングし origin を −N ずらして
  レンダしたとき、中央領域が非パディング版と bit 一致すること
  (ROI/ズーム非依存の回帰検知。design-v2 §10)。
- **ノイズ単体テスト**: `noise.rs` のハッシュ・fbm の既知値テスト。
  後で WGSL 版と突き合わせる基準値もここで固定する。
- リファレンス素材: ユーザー提供の入力 (画像 2 相当) を
  `tests/fixtures/reference_input.png` として置き、M1〜M3 の
  目視確認と M5 のチューニングに使う。

## 5. AE プラグイン側 (celglow2)

- `celglow2/src/lib.rs` は「AE パラメータ読み取り → `core::Params` 構築
  (downsample 込み) → f32 昇格済みバッファ + origin で `core::render()`
  → 出力変換 (ARGB↔RGBA スウィズル込み)」だけの薄いガワ。
  アルゴリズム変更で AE 側を触る必要がなくなる。
- 旧 v1 (`celglow`) はそのまま残り、AE には
  `CelGlow` (v1) と `CelGlow 2` (v2) が別エフェクトとして並ぶ。
- ビルドは workspace ルートで:
  ```powershell
  cargo build -p celglow-cli            # mock (数秒、AE 不要)
  cargo build -p celglow2 --release     # AE プラグイン v2 → CelGlow2.aex
  ```

## 6. 実装順 (design-v2 §11 への差し込み)

| MS | 内容 |
| --- | --- |
| **M0** (新設) | workspace 新設 + celglow-core 骨格 (Params/FrameBuf/origin/noise) + CLI の render/dump-params/watch。旧 celglow は workspace 外に凍結のまま残す (触らない) |
| M1〜M3 | **すべて CLI 上で開発・確認** (design-v2 の完了条件を contact シートで判定) |
| M3.5 (新設) | 新規 AE プラグイン `celglow2` を作成 (v1 の SmartFX 骨格をコピーして core 接続)、AE 上で実写確認 |
| M4〜M5 | GPU 移植 + parity (CLI の CPU 出力が基準)、チューニング |

M0 は半日規模。以降のイテレーションが「cargo run → PNG 確認」の
数秒ループになるので、初期投資として最優先で作る。

### 6.1 M0 受け入れ条件

M0 では S1〜S6 の画作りは実装しない。`render` は API・座標・I/O 経路を
検証するための **RGBA パススルー**とし、入力 rect から出力 rect を
切り出して返す。M1 でこの内部を実アルゴリズムへ置き換える。

- [ ] `rust/Cargo.toml` を workspace ルートとして新設し、members は
      `celglow-core` / `celglow-cli` のみとする。旧 `celglow` は members に
      含めず、配下のファイルを一切変更しない。
- [ ] `cargo build --workspace` と `cargo test --workspace` が成功する。
- [ ] `cargo run -p celglow-cli -- render -i input.png -o out.png` が成功し、
      M0 のパススルー出力が入力とピクセル一致する。
- [ ] `cargo run -p celglow-cli -- dump-params` が全パラメータの既定 TOML を
      stdout に出し、その TOML を `render -p` で再読込できる。
- [ ] `--set group.key=value` が TOML 読込後に上書きされ、不明な group/key、
      型不一致、範囲外値は説明付きエラーと非ゼロ終了になる。
- [ ] `watch` は params TOML の保存を検知して再レンダする。構文エラー時は
      直前の正常な出力を残し、エラー表示後も監視を継続する。
- [ ] `celglow-core` に Params/FrameBuf/DebugView/RenderError、origin を含む
      `render` API、hash/value noise/fbm の参照実装と既知値テストがある。
- [ ] 入力を N px 透明パディングして `input_origin=(-N,-N)`、
      `output_origin=(0,0)`、元画像サイズを出力指定した結果が、非パディング
      入力の結果と bit 一致する。
- [ ] 入出力長不一致、不正な output rect、PNG/TOML 読込失敗で panic せず、
      CLI が説明付きエラーと非ゼロ終了を返す。
- [ ] `cargo run -p celglow-cli` の引数なし実行は usage とサブコマンド一覧を
     表示する（PNG 入出力の受け入れ判定には上記 `render` コマンドを使う）。
