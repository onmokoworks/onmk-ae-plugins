# Resolution investigation — 2026-10-16 (unresolved)

- User reports lower resolution shifts/extends the stroke left; initial complete-disappearance report was corrected.
- Normal artifact and installed MediaCore AEX remain v0.3.1, SHA256 EC5BAA5502EB783617FC1C0E65B376D807010642259E5F1DF2EA56BA7D5E9577. No AE operations or installation in this pass.
- Full/Half/Quarter 16-bpc classic + SmartRender and three cropped ROI cases pass with full-resolution mask fixtures. See `dist/resolution-diagnostic/summary.json`. This does not establish AE PF Path coordinate units.
- Read-only Claude consultation agrees units remain unproven; summary in `dist/resolution-diagnostic/consultation-summary.md`.
- Added opt-in `coordinate-trace` feature with separate effect/match name, geometry-only bounded logs, no default-feature file I/O. Diagnostic release in `dist/coordinate-trace/`; six native 16-bpc SmartRender cases match normal output byte-for-byte. 15 unit tests/default check passed. Diagnostic is not a confirmed fix.

# v0.3.1 alpha correction — 2026-10-16

- Artifact: `dist/ONMK_TaperStroke.aex`, SHA256 `EC5BAA5502EB783617FC1C0E65B376D807010642259E5F1DF2EA56BA7D5E9577`. Installed to `C:/Program Files/Adobe/Common/Plug-ins/7.0/MediaCore/ONMK_TaperStroke.aex` on 2026-10-16 at the user request; installed SHA256 matches the build. No AE interaction.
- Root implementation error: output used `RGB = color * alpha`. AE's straight-alpha effect pipeline requires unassociated RGB. The renderer now retains the selected color, including zero-coverage pixels, and writes coverage/opacity only into alpha. Geometry and parameter IDs are unchanged.
- Independent evidence: [ProEXR developer manual](https://www.fnord.com/ProEXR/ProEXR_Manual.pdf), PDF page 23, explicitly describes AE's internal straight representation and extraction into its pixel buffer. Adobe SDK `Examples/Headers/AE_EffectCB.h` standard `PF_BEGIN_SAMPLING` and `PF_END_SAMPLING` macros use `PF_MF_Alpha_STRAIGHT`. The SDK macro alone was not treated as sufficient proof of the output convention.
- Claude Code (Opus, read-only consultation) identified the circular assumption in our earlier tests and fitted screenshot edge colors to yellow/background/black mixtures. Full advisory: `dist/alpha-diagnostic/claude-fringe-report.txt`. It did not change code or run the build. Codex independently checked the SDK, fetched the developer manual, implemented and tested this fix. Claude's stronger claims excluding all color-management causes are not adopted: the user's exact live stack remains untested.
- Correction of previous investigation: the white `RGB=A` and yellow `RGB=color*A` tests confirmed our implementation, not AE's required convention. Calling the ordinary AEXCompat PNG path defective was unjustified. Straight output is correctly written directly to PNG; our plugin's premultiplied output caused the dark preview. Earlier `check_compositing.py` is retained as a historical premultiplied-model experiment, pinned to the old artifact, not an AE acceptance test.
- `dist/alpha-diagnostic/check_straight_fix.py`: 12 before/after comparisons (24 native renders): 8/16/32 bpc, classic/SmartRender, yellow, opacity 25/100%. Old artifact fails the straight-color contract and darkens on white; new artifact retains constant RGB within channel quantization and has no red deficit on white. Alpha buffers are exactly unchanged in every comparison. Transparent pixels retain the stroke RGB. Results and SHA: `dist/alpha-diagnostic/straight-fix/summary.json`.
- 15 geometry tests, clippy with documented SDK macro exceptions, Release build, formatter and exact linked PiPL validation passed.
- Old v0.3 saved at `dist/alpha-diagnostic/v03-premult-before.aex`. Native before/after composites: `straight-fix/before-checker.png`, `straight-fix/after-checker.png`. The old synthetic-color output is wrong under straight-alpha composition; this regression now fails before and passes after. Actual AE screenshot equivalence and Turbulent Displace behavior are not claimed verified; AE was not touched.

# v0.3 validation — 2026-10-14

- Built artifact: `dist/ONMK_TaperStroke.aex`, SHA256 `9C7562913307074D1BF163E10DC7D7AA4098764083B579DCE9B76D8A1F1D2BC0`.
- Build only, per user request. MediaCore remains v0.2, SHA256 `A3939D264005A013AA82F8F478AFDE04836445E5B006C11AFB4452A8EE3F93A5` (verified).
- Trim radii now use original full-path fractions. Round geometry is the union of tangent hulls of variable-radius discs, with analytic circle/ellipse scanline spans. Contained discs skip invalid tangent quads. Flat geometry keeps perpendicular cut boundaries.
- Regression before/after at original path x=360: v0.2 width 167.247px; expected original profile 68.8125px; v0.3 width 68.831px. Five locations checked, each within 0.55px of the analytic profile after coverage quantization.
- 15 core tests passed, including unchanged interior pixels under trimming, common-tangent orthogonality, negligible trim handling, and raster coverage compared to an independent quadratic minimization over continuous disc families (ordinary, contained, zero-tip, sine, tight arc and hairpin cases). Curved fixtures use unsimplified input segments as their oracle, with <0.1 per-pixel coverage error and <0.5% total area error; trimmed coverage cannot exceed full coverage by more than 0.02.
- Consulted Claude Code (Opus, read-only plan mode) via claude-code-delegation. Advisory report: `dist/v03-validation/claude-consultation.md`. Adopted original-path width anchoring and common-tangent geometry; kept analytically rasterized full discs plus tangent quads instead of the suggested unverified arc-wedge optimization. Claude did not run tests; all results here were independently verified by Codex. Flat joins on very tight curves retain their previous miter behavior and are outside this Round-cap fix.
- Clippy (same SDK macro exceptions), formatter, Release build and linked PiPL byte comparison passed.
- AEXCompat: 13 native cases passed, including trimmed curves through all 6 classic/SmartRender × 8/16/32bpc paths, identical classic/SmartRender decoded pixels per depth, near-empty round trim, empty trim, steep round taper, contained discs, zero-radius tip, and 4× geometry (2048×1280).
- Images inspected: `dist/v03-validation/zoom-round.png` and `comparison.png`. Reproducer, requests, native results, pixel measurements and artifact identity: `dist/v03-validation/validate.py` and `host-summary.json`.
- Core benchmark (same 1080p fixture, 5 runs, median): v0.2 straight 3.153ms / curve 7.229ms; v0.3 straight 2.898ms / curve 9.980ms. Added curved-round envelope work costs about 2.75ms in this fixture. Excludes AE host calls, output pixel writing, and UI.
- Compatibility: parameter IDs 1–10 unchanged. Trim and Round silhouettes intentionally change. Round Width describes the generating-disc diameter; steep taper can extend outside the old normal-offset outline. Exact empty trim is transparent; a near-empty round trim can be a finite disc. Arbitrary self-overlap/cusp topology is not globally C1. AE live interaction, cache invalidation and render-queue equivalence were not tested.

## Historical v0.2 validation (superseded artifact)

- Artifact SHA256: `A3939D264005A013AA82F8F478AFDE04836445E5B006C11AFB4452A8EE3F93A5`. Installed to MediaCore and verified by hash.
- 11 core tests passed; clippy passed with the same two SDK macro exclusions; packaged PiPL byte comparison passed.
- AEXCompat inspection returns 10 parameters. All 6 classic/SmartRender × 8/16/32 bpc routes passed, with exact classic/SmartRender pixel equality at each depth.
- Native controls passed: linear taper widths at 5 positions; flat-cap Trim 25–75% bounds exactly x=76..180 for a path x=24..232; equal trim boundaries produce transparent pixels; curved trim renders visibly.
- Reproduction: `dist/v02-validation/validate.py`, `host-summary.json`, request JSON and PNGs.
- Core benchmark: 1920×1080, 3601 vertices, width=300, median of 5 iterations including outline construction and row coverage. Straight: 123.547→2.944ms (42×); curve: 116.085→5.971ms (19×). Excludes AE host path evaluation, pixel writing and UI overhead. The two versions use their respective production profiles (v0.1 smoothstep, v0.2 linear).
- Benchmark source snapshots and results: `dist/v02-validation/`. AE interactive performance and animated cache invalidation were not measured in this revision.
- Trim appends IDs 9/10 with defaults 0/100; old IDs remain unchanged. Ease=1 intentionally changes existing instances to linear taper.

## Historical v0.1 validation (superseded artifact)

## Artifact

- Windows x64: `dist/ONMK_TaperStroke.aex`
- SHA256: `59C3422D663BF0BB8FE9DDAF52AA1BEB54CD2AE546F41EBD3F9F34BC8F051F9A`
- Version: 0.1.0
- Dependency revision: `after-effects` / `pipl` at `0e0b197850f28c1bb24e8c82fce2e3d4c120d1e9`; transitive versions locked in Cargo.lock.

## Local checks

- `cargo test --offline --locked`: 7 passed, 0 failed.
- `cargo clippy --offline --locked --all-targets -- -D warnings -A clippy::drop_non_drop -A clippy::question_mark`: passed. The two exceptions originate in the pinned SDK's `define_effect!` macro.
- `cargo build --release --locked`: passed.
- `python verify_pipl.py`: passed; 13 properties, 382 exact binary bytes. Requires `pefile`.
- `dist/preview.png`: inspected; pointed taper, retained round end, and flat start are visible. This is a core preview, not an AE/AEXCompat host render.

The tests cover width endpoints/monotonicity, flat/round caps, transparent background, duplicate points, self-overlap coverage, render tiles and nonuniform downsampling, distance-based taper independent of vertex density, and width changes on a curved path.

## Host checks

AEXCompat verification was performed in chat `019ff518-0ca6-7b62-b1fd-4be5dfc51e40`, limited to this AEX. The request CLI supports `host_context.mask_scene` and a Path parameter assignment.

- Inspection succeeded; all 8 parameters were returned.
- Open Bezier mask at 256×144: classic and SmartRender both succeeded at 8/16/32 bpc (6/6).
- Every output PNG decoded and contained 5,252 pixels with nonzero alpha.
- Classic/SmartRender decoded pixel hashes match at each bit depth.
- The SmartRender 8 bpc image was visually inspected: curved tapered stroke, rounded start, transparent background.
- Evidence: `dist/aexcompat-validation/20261014-pipl-fixed/six-route-summary.json`, the corresponding request, identity, per-route reports, PNGs, and raw 16/32-bit outputs.
- Final authoritative summary: `dist/aexcompat-validation/20261014-pipl-fixed/verified-summary.json` (supersedes the intermediate `final-summary.json`). 13 successful native frames: 6 depth/route cases, 3 Path=None cases, and 4 parameter controls.
- Path=0 with an empty mask scene returns all-zero transparent pixels at 8/16/32 bpc. Width=0 is also transparent. Round/Flat caps have the expected bounds; 25% opacity and changed color produce the expected premultiplied values.
- All six main routes preserve guard bytes and balance handle/world lifetimes. Classic render reports balanced Path lifetimes; the shipping SmartRender report omits dedicated Path lifetime counters, so those counters remain unverified.
- Three initial negative cases selected Path=1 in an empty mask scene and were rejected by host `session_launch_validation/mask_range` before rendering. These are retained as input-validation failures, not transparent-output successes. Deleted positive path IDs remain unverified.
- No host source changes or rebuilds were needed. GPU, animation, host downsampling, oversized paths, and AE reference equivalence were outside this bounded host test.

The first AEX was correctly rejected as `invalid_pipl`. The pinned pipl crate's RC string encoding converted flag byte `88` into `81 45` on this machine, shifting the following resource property. The local build now embeds an opaque binary resource, with metadata emitted from the same property list. `verify_pipl.py` validates all property boundaries and compares the PE resource against the generated file. No host parser relaxation was needed.

Actual Adobe After Effects UI, animated Mask Path cache invalidation, and AE Render Queue equivalence have not been tested. `ae-demo.jsx` creates a three-layer test composition with animated masks and width for those checks; it has not been run in this session.
