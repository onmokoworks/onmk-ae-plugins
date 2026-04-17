# ONMK ParticleLab v1 UI Spec

## Purpose

This document defines the parameter UX for v1.
It is not a custom panel design.
It describes how the standard After Effects effect controls should be organized and labeled.

## UI Principles

- immediate readability over feature density
- sensible defaults over maximum flexibility
- grouped controls that reflect user mental model
- no product-clone terminology
- no hidden complexity unless the control is genuinely unsafe or unfinished

The user should be able to answer three questions quickly:

- where do particles come from?
- how do they move?
- what do they look like?

## Top-Level Grouping

The v1 effect UI should be organized in this order:

1. Emitter
2. Motion
3. Physics
4. Appearance
5. Rendering
6. Child Particles
7. System

This order reflects setup flow.
Users typically place the emitter first, then shape motion, then tune look.

## Group Details

### 1. Emitter

Purpose:
Define emission source and base lifecycle.

Controls:

- `Emitter Type`
- `Position X`
- `Position Y`
- `Emitter Size X`
- `Emitter Size Y`
- `Emitter Size Z`
- `Birth Rate`
- `Lifespan`
- `Lifespan Variation`

Labeling Notes:

- emitter dimensions are exposed per-axis so `Box / Grid / Sphere` can be shaped independently
- `Birth Rate` should remain particles-per-second in concept even if the internal sim is stepped

Default Intent:

- spawn visibly from center-ish frame position
- enough particles to clearly demonstrate effect on first apply

### 2. Motion

Purpose:
Define initial launch behavior.

Controls:

- `Speed`
- `Speed Variation`
- `Direction X`
- `Direction Y`
- `Spread (deg)`
- `Particle Size`
- `Size Variation`
- `Initial Rotation`
- `Rotation Speed`

Labeling Notes:

- `Direction X / Y` are acceptable for engineering, but a later polish pass may convert to angle-based UI if it improves usability
- `Spread (deg)` should remain explicit in degrees

Behavior Notes:

- if base direction is zero-length, fallback behavior must still emit predictably

### 3. Physics

Purpose:
Add secondary motion.

Controls:

- `Gravity`
- `Wind X`
- `Wind Y`
- `Turbulence`
- `Turbulence Scale`
- `Turbulence Speed`
- `Bounce`
- `Bounce Damping`

Labeling Notes:

- `Bounce` should remain an enable toggle
- if floor behavior becomes exposed in later versions, v1 naming may need revision

### 4. Appearance

Purpose:
Shape particles over life.

Controls:

- `Color Start`
- `Color End`
- `Opacity Start`
- `Opacity End`
- `Size Over Life`

Behavior Notes:

- v1 uses simple ramp behavior, not a graph editor
- defaults should demonstrate visible fade rather than invisible subtle change

### 5. Rendering

Purpose:
Define primitive type and compositing feel.

Controls:

- `Shape`
- `Blend Mode`
- `Motion Blur`
- `Depth of Field`
- `DOF Focal Distance`
- `DOF Aperture`
- `Size Multiplier`
- `Composite on Original`

UI Rules:

- `DOF` controls should remain visible only if the feature is kept in v1
- if pseudo-DOF remains visually weak or misleading, remove or hide it before final v1

### 6. Child Particles

Purpose:
Create secondary breakup from parent particles.

Controls:

- `Child Particles`
- `Child Count`
- `Child Inherit Velocity`
- `Child Lifespan`
- `Child Speed`
- `Child Spread`
- `Child Size Scale`

UI Rules:

- all child controls should remain grouped together
- if host grouping/collapse behavior is improved later, this group should default collapsed

### 7. System

Purpose:
Determinism and variation control.

Controls:

- `Random Seed`

UI Rules:

- this should live at the bottom
- users should not hit it first during normal setup

## Suggested Default Experience

On first apply, the effect should:

- emit from near comp center
- show visible upward spray or burst
- have readable color and opacity ramp
- composite onto source in an expected way
- not require more than 2-3 parameter edits to get a pleasing result

## UX Problems To Avoid

- too many controls reading as raw engineering values
- multiple controls that appear to do the same thing
- defaults that produce a dead or barely visible result
- parameters whose names imply capabilities that do not really exist
- leaving "future" controls exposed if they are not v1 quality

## Parameter Naming Review Candidates

The following names are acceptable for now but should be reviewed later:

- `Direction X`
- `Direction Y`
- `Size Over Life`
- `Child Particles`
- `Composite on Original`

Possible future alternatives can be evaluated after usage testing, but no rename should happen unless it clearly improves comprehension.

## About Box Copy

Current direction:

- product name
- short description
- core capabilities
- authorship / support reference

It should remain functional and brief.

## Future UI Expansion

Not v1:

- node graph editor
- custom viewport gizmo system
- preset browser
- custom panel
- advanced curve editor

These should not leak into the v1 UI architecture.
