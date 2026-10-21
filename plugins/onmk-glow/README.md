# onmkGlow

onmkGlowは、Adobe After Effects向けの実験的なマルチパス・グローエフェクトです。

しきい値、フォールオフ、バイアス、RGB別の幅、After Glow、水平・垂直ストリーク、合成モードを備えています。

- 表示名: `onmkGlow`
- After Effects Match Name: `ONMK_Glow`
- 状態: 実験的

`Show`の`Result`と`Threshold`の処理が逆になっていた問題と、16 / 32 bpc画像を8bitとして読み書きしていた問題を修正しています。内部のぼかし計算は維持し、入出力をネイティブ深度に対応させました。floatのRGBは負値・HDR値を保持します。

2026-10-04のAEXCompat検査31件と単体テスト6件が通過しています。16bitの全32,769段階、HDR、周囲へ広がる微弱なぼかしを検査し、既存8bit出力20件の画像ハッシュも一致しました。AEXCompat入力はPNG8由来のため、下位ビット保持は別のネイティブ入力テストで確認しています。ROI・大解像度でのメモリ負荷・全パラメータの組み合わせは未検証であり、全条件の動作保証ではありません。本実装はCPU処理です。

## Build

After Glowとストリークを無効にし、元画像を除いたグローで30描画・6比較が通過しました。
8/16/32bit・Classic/Smartで、明るさ0のRGBゼロ、0.25→0.5の倍増、閾値を上げた場合の
減光、幅変更による出力差、アルファ不変を確認しています。整数の倍増差は1階調以内、
floatは0.000002以内です。任意の合成・After Glow・同一インスタンスの再描画は別の検査対象です。

ぼかし半径は、NaN・無限大・整数演算の範囲を超える値をエラーとして拒否します。
異常値が暗黙に小さい半径へ置き換わったり、整数計算で桁あふれしたりする経路を防ぎます。
これは内部関数への異常値を使った回帰検査であり、AEのUIから同じ値を設定できるという意味ではありません。

CPU処理は画像サイズと入力長を検査し、画素数・対角長のサイズ計算を桁あふれ検査付きで行います。
しきい値・ぼかし・After Glow・出力配列の確保失敗は、Classic／Smart Renderへエラーとして返します。
不正なサイズと確保容量のオーバーフローをテスト済みですが、実際のメモリ枯渇や全パラメータ範囲の保証ではありません。

Smart Renderの入力レイヤーは、出力取得・描画の通常エラーとRustのpanic時にも返却します。
テストでは成功／失敗の組み合わせ6条件で返却が1回だけ実行され、元のエラーまたはpanicが
保持されることを確認しています。実AEの障害注入やプロセス強制終了への保証ではありません。

ぼかしの境界処理は、全タップを直接加算する独立した参照計算とも比較しています。
1×1、1×31、17×1、17×31、9×7で、半径0/1/2/8/33/65の水平・垂直60条件と、
異方性・片軸無効を含む3パス25条件が、負値・HDR入力で誤差0.000005未満でした。
半径を選ぶGaussian近似式自体は共通であり、その式の正しさや任意の半径・入力を保証する検査ではありません。

```powershell
cargo build --release
```

生成された`.aex`、`.dll`、`target/`はGitへコミットしません。

## License

MIT License. See [LICENSE](./LICENSE).

---

## English

An isolated-core host fixture disables source/after-glow/streak contributions.
30 renders and six comparisons pass across three depths and Classic/Smart:
zero brightness produces zero RGB, doubling 0.25 to 0.5 doubles RGB (within one
integer code or 2e-6 float), higher threshold reduces it, width changes output,
and alpha stays fixed. This does not cover arbitrary compositing, after-glow or
same-instance redraw/invalidation.

Blur-radius validation rejects NaN/infinity and values outside safe integer
arithmetic before radius selection. Regression inputs exercise internal functions;
they do not establish that these values can be entered through AE's normal UI.

Smart Render now checks in its input after output/render errors and Rust unwinding.
Six injected work/cleanup-result combinations verify exactly-once cleanup and
preservation of the original error or panic. This is not native AE fault injection
or process-abort recovery.

CPU rendering validates dimensions and exact input length, checks pixel/diagonal
size arithmetic, and reserves threshold/blur/after-glow/output vectors fallibly.
Errors propagate through Classic and Smart Render. Tests cover invalid geometry
and capacity overflow, not physical memory exhaustion or every numeric parameter.

onmkGlow is an experimental multi-pass glow effect for Adobe After Effects. It provides threshold, falloff, bias, per-channel width, after-glow shaping, horizontal and vertical streaks, and compositing controls.

The reversed `Result`/`Threshold` mapping and byte-only 16/32-bpc transport are fixed. Native channel codecs retain deep precision and signed/HDR float RGB without changing the blur algorithm. The implementation runs on CPU.

An independent direct-tap reference now checks clamped-edge rolling blur: five
shapes (including1x1,1x31,17x1), six radii0/1/2/8/33/65 and both axes give60
comparisons; five anisotropic/disabled-axis three-pass settings add25. Signed/HDR
input agrees within5e-6. Radius selection uses the same Gaussian approximation,
so this validates accumulation/boundaries, not that approximation or arbitrary inputs.

The 2026-10-04 audit passes 31 headless fixtures and six unit tests, including all 32769 nominal u16 levels, HDR and sub-byte glow spreading to neighboring pixels. Twenty existing byte fixtures remain hash-identical. PNG8-derived host fixtures alone do not establish native low-bit precision; separate native-input tests cover it. ROI, large-image memory pressure and exhaustive parameter combinations remain unverified.
