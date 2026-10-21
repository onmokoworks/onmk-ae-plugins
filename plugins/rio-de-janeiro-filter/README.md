# Rio de Janeiro Filter for After Effects

Rio de Janeiroフィルター風の色調を再現するAfter Effectsプラグインです。

非公式の独自実装で、InstagramおよびMetaとは関係ありません。

## ビルド

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
cargo build --release
```

---

An After Effects plug-in that recreates a look inspired by the Rio de Janeiro filter.

This is an unofficial, independent implementation and is not affiliated with Instagram or Meta.

## Rendering and validation

The current implementation is an 8-bit CPU Classic Render effect; it does not advertise 16/32-bit, Smart Render or GPU support. The checked-in shader is a placeholder, not an active rendering path.

Colour correction operates on unpremultiplied RGB and premultiplies the result before blending, preserving source alpha. Strength 0 preserves the input bytes exactly. The alpha regression test compares partial-alpha results with an opaque reference; AEXCompat additionally checks transparent RGB and partial-alpha Strength 0/50/100. Full-frame checks do not certify cropped regions, same-instance lifecycle or memory limits. See [Japanese notes](README.ja.md).

Row copies validate the complete geometry before writing and preserve row padding. Unit tests cover truncated buffers, overflow and reversed rows. Incompatible output extents return an error instead of silently producing a partial image; this is not cropped-region support. Input allocation is fallible. Grading reuses that single packed ARGB8 buffer, avoiding another full-frame allocation (8,294,400 bytes at 1920×1080). Total host/concurrent rendering memory is not yet budgeted.

## Build

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
cargo build --release
```
