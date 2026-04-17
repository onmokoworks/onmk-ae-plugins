# ONMK ParticleLab v1 Tasks

## Purpose

This document turns `DESIGN_V1.md` into executable engineering work.
The goal is to keep v1 constrained and measurable.

## Milestone Structure

v1 is divided into five milestones:

- M1: Core correctness
- M2: Visual usability
- M3: AE integration hardening
- M4: UX and parameter polish
- M5: Release readiness

## M1: Core Correctness

### Done

- effect entry point compiles
- parameter extraction exists
- deterministic seed-based particle simulation exists
- primitive renderer exists
- child particle support exists
- SmartFX path exists
- SmartFX origin offset bug has been addressed
- release `.aex` can be produced

### Remaining

- verify render alignment in actual AE viewport
- verify emitter position consistency between `Render` and `SmartRender`
- verify output when `Composite on Original` is enabled
- verify output when `Composite on Original` is disabled
- verify no clipping errors caused by local output origin handling
- verify behavior for non-default comp sizes

## M2: Visual Usability

### Priority

High. The plugin already renders, but the current look is still "engineering baseline."

### Tasks

- improve particle falloff and softness for each primitive shape
- improve additive and screen blending feel
- improve turbulence character so it reads less synthetic
- improve child particle breakup so it creates more useful secondary motion
- tune default parameter values to produce a good first-use preset
- review default color ramp and opacity ramp
- improve line shape behavior so it can read as streak-like motion
- decide whether current pseudo-DOF should remain visible in v1 or be reduced

### Exit Criteria

- default preset looks intentional rather than test-like
- one-click "energy burst" look is possible
- one-click "embers/dust" look is possible
- child particles add value instead of visual noise

## M3: AE Integration Hardening

### Priority

High. Stability outranks feature growth.

### Tasks

- validate plugin loading in AE clean session
- validate repeated application/removal does not destabilize host
- validate RAM preview consistency
- validate render queue consistency
- validate effect behavior on transparent inputs
- validate effect behavior on solid layers
- validate threaded render safety assumptions
- verify Smart Render output on cropped/partial render regions
- verify behavior under different bit depths in AE
- review PiPL flags against actual implementation

### Open Technical Questions

- should GPU flags remain enabled before real GPU path exists?
- should custom UI flag remain enabled if no custom UI is implemented?
- should float/deep color flags be narrowed until higher bit-depth handling is improved?

### Exit Criteria

- no host load failure
- no obvious coordinate drift
- no obvious render path mismatch
- no obvious flag/behavior contradiction that risks host instability

## M4: UX and Parameter Polish

### Priority

Medium-high. v1 succeeds or fails on speed of use.

### Tasks

- reorganize parameter presentation into clearer groups
- review naming for each parameter against motion-design expectations
- remove ambiguous labels
- decide final naming for `ParticleLab` or replacement product name
- improve `About` text
- reduce unused or misleading controls if they do not pay for themselves
- choose whether advanced controls need "v1 hidden" treatment

### Exit Criteria

- user can understand emitter, motion, physics, and appearance groups quickly
- defaults make sense
- labels are not engineering-internal

## M5: Release Readiness

### Priority

Medium.

### Tasks

- finalize product name
- finalize match name
- finalize support URL
- create install/build notes
- create changelog baseline
- create example presets or documented starting values
- freeze version `1.0.0` only once AE validation is complete

### Exit Criteria

- release build is reproducible
- shipping binary name is final
- documentation and plugin identity are consistent

## Immediate Next Tasks

These are the next concrete steps from current status:

1. Re-test emitter alignment in AE with the updated build.
2. Validate whether the right/up drift is fully fixed.
3. Review PiPL flags and remove anything that over-promises implementation.
4. Tune default values for first-use quality.
5. Improve renderer softness and blending.

## Deferred Features

The following are explicitly deferred beyond v1:

- path/mask/text emitters
- sprite particles
- node graph
- point connection rendering
- mesh/facet rendering
- real 3D camera integration
- preset browser
- full GPU renderer

## Task Ownership Notes

When implementing new work, follow these rules:

- do not expand scope silently
- if a task suggests `Plexus`-style or `Stardust`-style systems, move it to post-v1
- keep changes deterministic and AE-safe first

