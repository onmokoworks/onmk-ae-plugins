# ParticleLabRust handoff memo for Claude
Date: 2026-04-02
Project root: `C:\Users\optim\ParticleLabRust`
Deploy target: `C:\Users\optim\AEPluginBuild\ONMK_ParticleLab.aex`

## Goal
Rust-based After Effects particle plugin. Recent work focused on:
- reducing silent AE crashes
- stabilizing cache / rerender behavior
- improving performance under large particle count / large size
- adding better motion blur
- starting AE camera integration
- adding harder-edged particle shapes

## Current status summary
The plugin is in a safer state than before, but not fully stable/finished.
Main current state:
- AE crash frequency reduced with panic boundary and oversized buffer guards
- parameter changes trigger rerender more aggressively
- image/source cache invalidation is stronger
- particle display should be visible again after previous camera experiments
- AE camera support is partially wired but still not correct/reliable
- `Edge Softness` parameter exists and is now placed under `Rendering > Shape`
- hard edges no longer produce obvious dotted raster artifacts because raster LOD is disabled when edge softness is low

## Important current user concerns
1. They want actual AE camera-linked 3D behavior.
2. They want hard-edged circles / shapes without fade.
3. They care about visual quality; dotty/aliased results are not acceptable.
4. They care about performance, but not at the cost of obviously broken visuals.
5. They want the UI organized sensibly.

## Files to inspect first
- `src/lib.rs`
- `src/renderer.rs`
- `src/particle.rs`
- `build.rs`

## What was changed recently

### Crash / safety
In `src/lib.rs`:
- `panic::catch_unwind` added around AE command handling
- oversized output buffers rejected
- `SmartPreRender` rectangle expansion is now heuristic-based instead of fixed
- `SmartRenderGpu` is still not implemented

### Cache / rerender
In `src/lib.rs`:
- `UserChangedParam` now forces rerender
- source-related cache invalidation happens when:
  - `ImageSourceLayer` changes
  - `ImageProxyScale` changes
  - `RefreshImageCache` pressed
- helper functions added:
  - `should_refresh_ui`
  - `should_invalidate_source_cache`

In `build.rs`:
- flags were simplified for stability
- currently `OutFlags2::IUse3DCamera` is in PiPL
- `QueryDynamicFlags` no longer changes flags dynamically because AE complained:
  - “effect cannot set flag bits for PF_Cmd_QUERY_DYNAMIC_FLAGS which were not set in the PiPL”

### Performance / LOD
In `src/particle.rs`:
- simple live-particle cap via spawn thinning:
  - `MAX_LIVE_PARTICLES = 12_000`
  - `spawn_stride_for_window()`

In `src/renderer.rs`:
- rasterization LOD via `particle_raster_step()`
- BUT:
  - if `edge_softness <= 0.2`, raster LOD is disabled to avoid dotty hard-edged particles

### Motion blur
In `src/renderer.rs`:
- motion blur is no longer fake radius inflation only
- now it uses multiple velocity-based samples:
  - `blur_offset = p.velocity * config.frame_dt * motion_blur_amount`
  - up to `MAX_MOTION_BLUR_SAMPLES = 8`

### Edge softness / hard shapes
In `src/lib.rs`:
- new param `EdgeSoftness`
- moved to directly under `Rendering > Shape`

In `src/renderer.rs`:
- `RenderConfig` now includes `edge_softness`
- `shape_coverage()` uses `edge_softness`
- for circles:
  - `edge_softness == 0` gives hard binary edge
  - higher values feather the edge

Current limitation:
- UI placement suggests it applies to all shapes
- implementation is strongest/clearest for circles
- other shapes still use their existing soft falloff logic and may need refinement for fully consistent “hard edge” behavior

## AE camera integration status
This is the main unfinished area.

Current implementation:
- `try_get_camera_projection()` in `src/lib.rs` calls `in_data.effect().camera_matrix(time)`
- it builds a `CameraProjection` struct and passes it into renderer
- `draw_particle()` in `src/renderer.rs` attempts 3D projection via `transform_particle_3d()`
- if projected coordinates look suspicious or transform fails, it falls back to 2D rendering

Important details:
- `CameraProjection` currently stores:
  - `matrix`
  - `invert_matrix`
  - `image_plane_dist`
  - `image_plane_width`
  - `image_plane_height`
- right now `invert_matrix` is set to `false`
- previously there was an inverse-matrix approach, but it caused invisibility / bad projection
- current camera integration is therefore only a tentative attempt and not trustworthy yet

Observed behavior history:
- when camera projection was more aggressively enabled, particles disappeared
- this was rolled back / softened by adding fallback-to-2D behavior
- user says it still is not actually linked to AE camera as desired

Conclusion:
- camera support is not done
- it likely needs proper interpretation of AE’s camera matrix / coordinate system
- may need testing of:
  - whether matrix should be inverted
  - handedness / sign conventions
  - image plane scaling
  - comp-space vs layer-space assumptions

## Known likely issues / technical debt

### 1. `src/lib.rs` is too large
It mixes:
- AE command handling
- param setup
- cache logic
- image/layer utilities
- smart render logic

Refactor candidate split:
- `commands.rs`
- `params.rs`
- `cache.rs`
- `layer_io.rs`

### 2. AE camera support incomplete
Most important unresolved feature request.

### 3. Hard edge behavior is not fully uniform across all shapes
`EdgeSoftness` is placed for all shapes, but current implementation is most explicit for circles.
Square/triangle/star/line may still need shape-specific hard-edge handling if visual parity is desired.

### 4. LOD still affects visuals in some cases
Even with the hard-edge exception, performance optimizations are still heuristic and may trade quality for speed.

### 5. 8bpc assumptions are still strong
The flat buffer helpers assume 4 bytes/pixel style handling.
16/32 bpc support is not solved.

## Recent user-visible fixes
- `Emitter` group starts collapsed
- `Edge Softness` moved under `Shape`
- hard-edged particles should no longer look dotted
- moving particle position should no longer make them disappear due to bad SmartPreRender bounds
  - bounds now union input rect with estimated emitter bounds

## Things the user explicitly asked for
- AE camera-linked 3D behavior
- no-fade crisp circular particles
- clean parameter placement in UI
- stable behavior when moving params and emitter position

## Best next steps for Claude

### Priority 1: Fix actual AE camera linkage
Investigate:
- what `effect().camera_matrix()` actually returns in AE terms
- whether it must be inverted
- how layer/comp coordinates should be transformed
- whether emitter/world positions need comp-space conversion before projection
- whether AE camera data should instead be pulled via another interface / helper

Good places:
- `src/lib.rs` `try_get_camera_projection`
- `src/renderer.rs` `transform_particle_3d`
- any docs/examples in `after-effects` crate checkout

### Priority 2: Make `EdgeSoftness` truly apply cleanly to all procedural shapes
Right now it is conceptually for all shapes, but renderer logic is not fully unified.
Consider:
- square hard edge
- triangle hard edge
- star hard edge
- line hard edge
with consistent edge softness semantics

### Priority 3: Decide whether current performance LOD is acceptable
Potentially:
- disable raster LOD for more shapes when edge softness is low
- separate preview-vs-final quality
- make quality mode user-configurable

### Priority 4: Clean camera fallback logic
Current fallback:
- if projected values look suspicious, render in 2D
This is safe, but can hide broken 3D implementation.
Once camera math is correct, remove or tighten fallback.

## Build / deploy
Current standard deploy flow:
1. `cargo build --release`
2. copy `target\release\onmk_particle_lab.dll` to
   `C:\Users\optim\AEPluginBuild\ONMK_ParticleLab.aex`

## Current build state
As of the latest pass:
- release build succeeded
- latest plugin copied to deploy folder

## Notes
The user is iterating interactively inside AE. They notice regressions quickly.
Prefer conservative fixes that keep particles visible over ambitious camera math that makes output disappear.
