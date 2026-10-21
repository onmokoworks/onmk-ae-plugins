# onmk Film

GPUバックエンドは現在Metal専用です。Windowsでは「Use GPU」がONでもCPUへ戻ります。8/16/32bitのヘッドレス比較で、ON/OFFの出力一致を確認しています。これはWindowsでのGPU動作確認ではありません。

The GPU backend is currently Metal-only. On Windows, Use GPU falls back to CPU; headless 8/16/32-bit comparisons verify identical on/off output. This is fallback evidence, not Windows GPU execution.

GPU用バッファの確保前に容量・ストレージバインディング・実行グループ数・シェーダーの座標範囲を検査し、制限超過はCPUフォールバック用のエラーで返します。境界条件はGPUなしの単体テストで検証しています。Metal実機でのメモリ枯渇やデバイス喪失の検証ではありません。

Before GPU allocation, buffer/binding sizes, dispatch counts and shader coordinate bounds are checked; oversized requests return an error for CPU fallback. Device-independent boundary tests cover these guards, not actual Metal exhaustion or device loss.

GPU読み戻しは待機エラーを伝播し、マッピング解除を成功・失敗・パニック時に行います。結果用ベクタはマッピング前に失敗可能な方法で確保します。解除の制御フローは単体テスト済みですが、Metal実機への障害注入は未実施です。

GPU readback propagates poll errors and unmaps on success, failure or unwind. Output capacity is reserved fallibly before mapping. Unit tests cover cleanup control flow; Metal device fault injection remains unverified.

AE側の待機中フレームバッファは、全スレッド合計の容量を256MiBまで保持します。上限に入らないバッファは描画後に破棄します。解像度・画質は変更しません。この上限には描画中のバッファ、コア/GPU内部、アロケータの管理領域は含まれません。

Idle AE work-frame caches retain at most 256 MiB of vector capacity across threads. Buffers that do not fit are dropped after rendering without reducing resolution or quality. Active buffers, core/GPU storage and allocator overhead are excluded.

小さなテスト用上限で4スレッド・64回の同時確保と返却を検証し、上限内の保持と終了後の計数ゼロを確認しています。実AEのスレッド終了やプロセスの実メモリ使用量を測定したものではありません。

A small-budget test exercises four concurrent workers over 64 admission/release rounds, checking bounded retention and zero accounting after teardown. It does not measure process memory or native AE thread shutdown.

CPU画像アクセス前に寸法・行幅・ポインタ整列・オフセット範囲を検査し、AE側の作業フレーム確保は失敗をエラーで返します。部分領域の座標整合や全キャッシュのメモリ上限まで検証したものではありません。

CPU world access validates dimensions, stride, pointer alignment and offset bounds. AE-side work-frame allocation failures propagate as errors. This does not certify ROI alignment or an aggregate cache-memory bound.

Smart Renderは描画成功・失敗・Rustパニック時も取得済み入力を返却します。描画と返却が両方失敗した場合は元の描画エラーを優先します。6条件のヘルパーテストと通常のヘッドレス描画で検証しており、実AEへの障害注入は未実施です。

Smart Render attempts acquired-input checkin after success, error or Rust panic, preserving the primary render error if cleanup also fails. Six helper fault conditions and normal headless rendering are tested; native AE fault injection is not covered.

macOS After Effects 向け SmartFX。独自のフィルムレスポンス、乳剤滲み、密度空間ラティスグレイン、光学系をひとつのパイプラインで処理します。

| | |
|---|---|
| Effect | `onmk Film` |
| Category | `onmk` |
| Match name | `ONMK_Film` |
| Bundle | `onmkFilm.plugin` |
| Version | 0.5.0 |

## Layout

```
plugins/onmk-film/
  src/                    # onmk-film-core sources
  crates/onmk-film-core/  # host-independent engine
  crates/onmk-film-gpu/   # wgpu Metal blur + grain
  crates/onmk-film-ae/    # AE SmartFX host
  presets/                # JSON look notes
  docs/pipeline.md
  scripts/install_dev.sh
```

## Build (macOS)

```sh
export AESDK_ROOT="/path/to/AfterEffectsSDK"
cargo test -p onmk-film-core
cargo test -p onmk-film-gpu
NO_INSTALL=1 just build
./scripts/install_dev.sh
```

Installs to:

`~/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/onmkFilm.plugin`

Restart After Effects. Effect path: **Effect → onmk → onmk Film**.

## Features (v0.5)

- Input CS: Rec.709 / sRGB / Linear / Bypass / S-Log3 / LogC3 / V-Log / C-Log3
- 13 film stocks, 6 looks + Custom
- Exposure, character, print contrast, shoulder, saturation, signature, skin
- Halation, bloom/diffusion, emulsion bleed
- Density lattice grain (gauge, clump, S/M/H, print grain)
- MTF / acutance, CA, vignette, light leak, gate weave
- Crossover + mottle
- 8 / 16 / 32 bpc SmartFX
- Metal GPU blur + grain with CPU fallback

## Notes

- Windows headless audit (2026-10-04): added the missing Win64 PiPL entry.
  Explicit World Suite format detection fixes 32-bit bypass corruption caused
  by interpreting float worlds with the Deep flag as 16-bit. The CPU core has
  nine tests and the adapter has a pixel-format regression test. This is not
  native AE or macOS GPU certification; Metal execution remains untested here.
- `Cargo.lock` is intentionally ignored in this workspace. The audit generated
  a local lockfile from the existing offline cache, then used `--locked` builds.

- GPU path uses internal wgpu (Metal), not AE `SmartRenderGpu`
- Look popup overwrites related controls (except Neutral / Custom)
- Preset JSON under `presets/` is offline documentation of look values
