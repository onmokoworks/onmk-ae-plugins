# ONMK ParticleLab v1 AE Test Plan

## Purpose

This document defines what must be checked inside After Effects before v1 can be treated as stable.

It is focused on practical host validation, not unit testing.

## Test Environment

For each test, record:

- AE version
- OS
- comp size
- comp bit depth
- renderer mode if relevant
- whether preview or final render was used

Recommended baseline comps:

- `1920x1080`
- `1080x1080`
- `3840x2160`
- transparent comp / transparent source
- solid layer source

## Test Categories

1. Load and discovery
2. Spatial correctness
3. Simulation correctness
4. Rendering correctness
5. Host integration
6. Stress and edge cases

## 1. Load and Discovery

### Test 1.1

Objective:
Plugin appears in AE and can be applied.

Steps:

1. Launch AE from a clean session.
2. Create a new comp.
3. Search for `ParticleLab`.
4. Apply the effect to a layer.

Expected:

- plugin loads without host warning
- effect appears under expected category
- effect UI opens correctly

### Test 1.2

Objective:
Repeated apply/remove does not destabilize host.

Steps:

1. Apply the effect.
2. Remove the effect.
3. Reapply several times.

Expected:

- no crash
- no load failure
- no parameter corruption

## 2. Spatial Correctness

### Test 2.1

Objective:
Emitter position matches UI coordinates.

Steps:

1. Set emitter to `Point`.
2. Place `Position X/Y` at comp center.
3. Observe spawn location.
4. Move to corners and center again.

Expected:

- visible particles originate at intended coordinates
- no consistent right/up drift

### Test 2.2

Objective:
Emitter alignment holds on different comp sizes.

Steps:

1. Repeat center/corner placement on `1080x1080` and `3840x2160`.

Expected:

- same coordinate semantics across sizes

### Test 2.3

Objective:
Output remains aligned during partial redraw / SmartFX behavior.

Steps:

1. Scrub timeline.
2. Zoom and pan viewer if relevant.
3. Trigger cached and uncached redraws.

Expected:

- no visible render offset between frames
- no tile-origin drift

## 3. Simulation Correctness

### Test 3.1

Objective:
Random seed is deterministic.

Steps:

1. Set a seed.
2. Preview frame at fixed time.
3. Note particle arrangement.
4. Reopen comp or purge cache and re-evaluate.

Expected:

- same frame renders identically

### Test 3.2

Objective:
Changing seed changes result.

Steps:

1. Duplicate layer.
2. Change only `Random Seed`.

Expected:

- visible variation with all other behavior preserved

### Test 3.3

Objective:
Birth rate and lifespan scale logically.

Steps:

1. Test low, medium, and high `Birth Rate`.
2. Test short and long `Lifespan`.

Expected:

- density responds predictably
- no obvious stepping artifact beyond acceptable v1 limits

## 4. Rendering Correctness

### Test 4.1

Objective:
`Composite on Original` behaves correctly.

Steps:

1. Enable compositing on a solid footage layer.
2. Disable compositing.

Expected:

- enabled: source remains visible under particles
- disabled: particle-only render path behaves as expected

### Test 4.2

Objective:
Blend modes behave plausibly.

Steps:

1. Test `Normal`
2. Test `Add`
3. Test `Screen`

Expected:

- each mode is visually distinct
- no unexpected alpha inversion or dark fringe

### Test 4.3

Objective:
All shapes render and rotate plausibly.

Steps:

1. Cycle `Circle`, `Square`, `Triangle`, `Star`, `Line`
2. Vary `Initial Rotation` and `Rotation Speed`

Expected:

- all shapes appear
- line shape reads as directional
- no broken bounds or clipping from shape selection alone

### Test 4.4

Objective:
Appearance ramps work.

Steps:

1. Test wide color transition
2. Test opacity fade
3. Test size-over-life extremes

Expected:

- particle lifetime interpolation is obvious and stable

## 5. Host Integration

### Test 5.1

Objective:
RAM preview and render queue match.

Steps:

1. Preview short segment.
2. Export same segment through render queue.
3. Compare representative frames.

Expected:

- no meaningful mismatch

### Test 5.2

Objective:
Duplicate layers remain independent.

Steps:

1. Duplicate effect layer.
2. Change one parameter on only one copy.

Expected:

- no cross-layer contamination

### Test 5.3

Objective:
Project save/reopen stability.

Steps:

1. Save project with effect applied.
2. Reopen project.

Expected:

- effect reloads
- parameters persist
- no load error

## 6. Stress and Edge Cases

### Test 6.1

Objective:
High particle count degradation is graceful.

Steps:

1. Push `Birth Rate` high.
2. Increase lifespan.

Expected:

- slowdown may occur, but no crash or corruption

### Test 6.2

Objective:
Extreme positions do not break rendering.

Steps:

1. Move emitter far off-screen.
2. Use large emitter size.

Expected:

- no crash
- clipping remains bounded and sane

### Test 6.3

Objective:
Transparent input behavior is correct.

Steps:

1. Apply effect to transparent solid or empty alpha source.

Expected:

- particles still render as intended
- compositing logic remains sensible

### Test 6.4

Objective:
Child particle controls remain stable.

Steps:

1. Enable child particles.
2. Sweep child count and spread.

Expected:

- richer output
- no explosive instability or obviously broken spawn behavior

## Pass / Fail Rule

v1 host validation passes only if:

- plugin loads cleanly
- emitter alignment is correct
- no recurring render offset remains
- preview and final render are consistent
- no host crash occurs during normal use

## Bug Logging Format

For each failed test, log:

- test ID
- AE version
- comp settings
- exact parameter state
- screenshot or frame reference
- whether issue occurs in preview, final render, or both
- whether issue is deterministic

## Immediate Manual Test Focus

Because a coordinate bug was recently fixed, run these first:

1. `Test 2.1`
2. `Test 2.3`
3. `Test 4.1`
4. `Test 5.1`

