# Agent guide 窶・RioGradeRust

縺薙・繝ｪ繝昴ず繝医Μ縺ｮ譁ｹ驥昴・繝ｬ繧､繧｢繧ｦ繝医・繝薙Ν繝画焔鬆・・隕冗ｴ・・ **[`CLAUDE.md`](./CLAUDE.md)** 縺ｫ
髮・ｴ・＠縺ｦ縺・∪縺吶・odex / 莉悶・繧ｳ繝ｼ繝・ぅ繝ｳ繧ｰ繧ｨ繝ｼ繧ｸ繧ｧ繝ｳ繝医ｂ縺ｾ縺壹◎縺｡繧峨ｒ蜿ら・縺励※縺上□縺輔＞縲・
隕∫せ縺縺・

- 蜈ｬ髢倶ｻ墓ｧ倥・ `README.md` / `README.ja.md`・医ヰ繧､繝ｪ繝ｳ繧ｬ繝ｫ・峨ょ､画峩譎ゅ・荳｡譁ｹ譖ｴ譁ｰ
- PiPL・医・繝ｩ繧ｰ繧､繝ｳ蜷阪・繧ｫ繝・ざ繝ｪ繝ｻSupport URL・峨・逵溷ｮ溘・ `build.rs`
- GPU 縺ｯ `wgpu` + CPU 繝輔か繝ｼ繝ｫ繝舌ャ繧ｯ蠢・医√ち繝ｼ繧ｲ繝・ヨ縺ｯ 8-bit ARGB
- 菴懈･ｭ繝ｭ繧ｰ繝ｻ隱ｿ譟ｻ繝｡繝｢縺ｪ縺ｩ縺ｮ蜀・Κ繝峨く繝･繝｡繝ｳ繝医・ `_internal/`・・it 邂｡逅・､厄ｼ峨↓譖ｸ縺・- 繧ｳ繝溘ャ繝医＠縺ｪ縺・ｂ縺ｮ: `*.aex` `*.dll` `target/` `_internal/`

繝薙Ν繝・

```powershell
$env:AESDK_ROOT = "C:\path\to\AfterEffectsSDK"
cargo build --release
```
