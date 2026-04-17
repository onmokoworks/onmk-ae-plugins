# ParticleLab Self Review

Date: 2026-04-01

## External reference points

- Furikake positions itself as a lightweight and high-performance particle generator for After Effects, with MFR, DOF, and 32-bit quality as selling points.
- Stardust positions itself around a modular node-based workflow, multiple emitters and forces in one shared space, presets, physics, and model/text/spline driven workflows.

## Current strengths

- Very fast direct-evaluation particle simulation.
- AE-native parameter workflow with grouped controls.
- MFR flags enabled again.
- Point parameter for emitter position with native crosshair UI.
- Image particles implemented with proxyable sprite cache.
- Child particle support, blend modes, and basic DOF/motion-blur style controls.

## Current weak points

- The image cache key is still pragmatic rather than rigorous. It is designed for speed first.
- Image source controls are functionally correct but still live under `Emitter`; they should move under `Rendering` in a compatibility-safe cleanup pass.
- Physics is still intentionally simplified. It is good for motion graphics, not yet for high-fidelity simulation.
- No 32-bit path yet.
- No layer/text/mask/model driven emitters yet.
- No preset system yet.
- No real node graph yet.

## Competitive gaps vs. higher-end tools

- No multi-operator graph on one layer.
- No source graph for emitter -> force -> deform -> render chaining.
- No text/mask/model/spline emitter family.
- No field/deformer stack.
- No particle-to-particle interaction or collision system.
- No preset browser or scene templates.

## Changes completed in this pass

- Switched image usage from image-emitter to image-particle sprite rendering.
- Added `Shape = Image`.
- Added image sprite proxy cache with `Full`, `/2`, `/4`, `/8`.
- Added cache refresh button.
- Added image tint/source color modes.
- Added image fit modes (`Contain`, `Stretch`).
- Added sprite rotation in the renderer so image particles follow particle rotation.

## Stability pass on 2026-04-02

- Added a panic boundary around AE command handling so Rust panics now return an AE error instead of hard-crashing the host.
- Added output buffer size guards so oversized SmartFX regions fail safely instead of attempting pathological allocations.
- Replaced the fixed `2000px` SmartPreRender expansion with a bounded heuristic based on particle travel distance.
- Removed threaded-rendering and flattened-sequence capability declarations to reduce cache / invalidation risk while the plugin still has no sequence data model.
- Forced rerender on any user parameter edit and explicitly invalidate source-derived caches when source layer or proxy settings change.

## Current stability findings

- The plugin is now more defensive at the host boundary, but SmartFX invalidation still deserves suspicion before any new feature work.
- Cache correctness is stronger than before, but the source-layer signature is still heuristic rather than exact and could theoretically collide.
- `src/lib.rs` has grown too large. The next cleanup should split AE command handling, cache helpers, and parameter extraction into separate modules without changing behavior.
- There is still no 16/32-bpc render path, so format assumptions remain narrower than a production AE effect should allow.
- `cargo clippy` could not be run in this environment because `clippy-driver.exe` returned `os error 5` (access denied), so lint-driven cleanup is still pending.

## Recommended next implementation order

1. Move image source controls to the Rendering group and hide them when `Shape != Image`.
2. Add `Use Source Alpha / Premultiply / Clamp` controls for image particles.
3. Add text and mask emitters.
4. Add force/deformer stack abstraction without UI graph first.
5. Add a compact node-graph data model and serialization format.
6. Only after the above, build a custom graph UI.

## Decision

The current plugin should continue as a fast AE-native particle system first, then grow into a node-based system in a controlled way.
Going directly to a large custom graph UI before the internal graph model is stable would create unnecessary churn.
