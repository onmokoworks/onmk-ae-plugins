# ONMK ParticleLab v1 Design

## Positioning

ONMK ParticleLab is a general-purpose particle effect plugin for Adobe After Effects.
It is intended as an alternative in the broad "motion graphics particle tool" space, but it is not a clone of any specific commercial product.

The v1 goal is narrow and practical:

- deliver a usable, fast, parameter-driven particle effect
- behave like a standard AE effect without requiring a custom node graph
- prioritize emitter control, motion behavior, look development, and render predictability

ParticleLab v1 is closest in product shape to a conventional emitter-based particle effect.
Graph-based workflows, point-connection systems, and mesh/surface generation are explicitly out of scope for v1.

## Product Goal

Build a stable AE-native particle effect that can cover common motion design use cases:

- energy bursts
- sparks
- dust
- embers
- abstract motion streaks
- simple stylized particle fields

The plugin should be useful before it is deep.
That means a clean parameter model, predictable playback, and render consistency matter more than feature count.

## v1 Scope

### Included

- 2D particle rendering in layer space
- emitter-based generation
- deterministic simulation from time and seed
- basic physics controls
- simple lifetime-driven appearance shaping
- child particle spawning
- multiple primitive render shapes
- standard blend modes for compositing
- AE SmartFX-compatible render path

### Excluded

- node-based authoring
- point/line/facet connection systems
- true 3D camera-aware rendering
- mesh generation
- volumetrics
- textured sprites / sprite sheets
- layer, mask, path, text, or 3D model emitters
- preset browser
- custom panel UI
- authored preset format
- fully realized GPU renderer

## User Value Proposition

The v1 plugin should provide:

- fast setup for particle looks without leaving AE
- enough controls to cover common motion-design particle tasks
- deterministic output for repeatable renders
- a foundation that can later grow toward more advanced systems

## Runtime Model

ParticleLab v1 uses a deterministic simulation model.
For a given parameter set, time, and seed, the rendered result should be reproducible.

Simulation is rebuilt from time rather than relying on persistent runtime state.
This is simpler, safer for AE rendering, and easier to keep stable across previews and exports.

Implications:

- rendering is stateless from the host perspective
- randomization must be seed-driven
- current frame result must not depend on preview history
- behavior should remain stable in Smart Render and threaded contexts

## Coordinate System

- All emitter positions are defined in layer/image space.
- The plugin renders in AE output world coordinates and must respect world origin.
- SmartFX output rectangles may be subregions; rendering must offset particle positions by the output world's local origin.
- v1 assumes 2D comp-space style placement, not full 3D scene integration.

## Parameter Model

The current v1 parameter model is grouped into six areas.

### 1. Emitter

Purpose:
Control where particles are born and how many are generated.

Parameters:

- `Emitter Type`
  - `Point`
  - `Box`
  - `Sphere`
  - `Grid`
- `Position X`
- `Position Y`
- `Emitter Size X`
- `Emitter Size Y`
- `Emitter Size Z`
- `Birth Rate`
- `Lifespan`
- `Lifespan Variation`

Expected behavior:

- `Point` emits from a single position
- `Box` emits from a rectangular or box-like region
- `Sphere` emits from a radial region
- `Grid` emits from discrete distributed positions

### 2. Motion

Purpose:
Define launch direction and basic kinematic variation.

Parameters:

- `Speed`
- `Speed Variation`
- `Direction X`
- `Direction Y`
- `Spread`
- `Particle Size`
- `Size Variation`
- `Initial Rotation`
- `Rotation Speed`

Expected behavior:

- direction is a normalized base vector
- spread widens the angular distribution around the base direction
- size and speed variation are seed-driven and deterministic
- rotation is shape-facing only and does not imply 3D orientation

### 3. Physics

Purpose:
Apply secondary motion after emission.

Parameters:

- `Gravity`
- `Wind X`
- `Wind Y`
- `Turbulence`
- `Turbulence Scale`
- `Turbulence Speed`
- `Bounce`
- `Bounce Damping`

Expected behavior:

- gravity applies constant acceleration
- wind applies directional acceleration
- turbulence applies procedural directional noise
- bounce is a simple floor-style response, not full scene collision

### 4. Appearance

Purpose:
Control look development over particle lifetime.

Parameters:

- `Color Start`
- `Color End`
- `Opacity Start`
- `Opacity End`
- `Size Over Life`

Expected behavior:

- color interpolates over normalized lifetime
- alpha interpolates over normalized lifetime
- size scales over normalized lifetime
- v1 uses simple curve-like shaping, not a multi-point editor

### 5. Rendering

Purpose:
Choose particle primitives and compositing behavior.

Parameters:

- `Shape`
  - `Circle`
  - `Square`
  - `Triangle`
  - `Star`
  - `Line`
- `Blend Mode`
  - `Normal`
  - `Add`
  - `Screen`
- `Motion Blur`
- `Depth of Field`
- `DOF Focal Distance`
- `DOF Aperture`
- `Size Multiplier`
- `Composite on Original`

Expected behavior:

- shapes are analytic primitives, not image sprites
- motion blur in v1 is an approximation, not shutter-accurate sampling
- DOF in v1 is an approximation based on pseudo-depth, not true camera-aware bokeh
- compositing can render over input or onto an empty output

### 6. Child Particles

Purpose:
Allow secondary breakup and richer motion from a simple parent system.

Parameters:

- `Child Particles`
- `Child Count`
- `Child Inherit Velocity`
- `Child Lifespan`
- `Child Speed`
- `Child Spread`
- `Child Size Scale`

Expected behavior:

- child particles spawn from parent particles
- child particles inherit some portion of parent velocity
- child particles are lighter and shorter-lived by default
- this is not a full event system yet; it is a built-in secondary emission layer

### 7. System

Purpose:
Preserve deterministic random behavior.

Parameters:

- `Random Seed`

Expected behavior:

- same seed + same time + same params = same output

## Rendering Behavior

### Bit Depth

v1 currently renders through an 8-bit-oriented custom path.
The plugin advertises deeper color awareness and SmartFX compatibility, but the actual rendering path should be treated as "functional baseline" rather than final color pipeline architecture.

This is acceptable for v1 as long as:

- the plugin loads and renders reliably
- color behavior is predictable
- future refactoring to 16/32-bpc remains possible

### Blend Modes

v1 blend modes:

- `Normal`
- `Add`
- `Screen`

These are enough for initial motion graphics use cases.
More advanced compositing modes are deferred.

### Shape Strategy

v1 uses procedural primitive shapes instead of textures.

Benefits:

- no asset management burden
- easy deterministic rendering
- simple parameterization
- lower implementation complexity

Tradeoff:

- cannot yet produce textured particle looks or sprite animation

## Performance Goals

v1 performance priorities:

- responsive interaction at moderate particle counts
- consistent render output in AE
- no frame-history dependence
- predictable behavior under Smart Render

Performance is more important than adding marginal features.
If a feature materially destabilizes render predictability, it should be deferred.

## Stability Requirements

The plugin should:

- compile cleanly in release mode
- load as an AE effect without relying on external assets
- render consistently via normal render and Smart Render paths
- avoid coordinate drift when AE gives subregion output worlds
- remain deterministic under seed-based re-renders

## Non-Goals

These are explicitly not v1 goals:

- beating full commercial particle suites on breadth
- reproducing another product's workflow or naming
- shipping a node graph
- shipping a connection/plexus-style rendering system
- shipping a physically correct 3D solver

## Naming Guidance

The product should be positioned as an original ONMK particle tool.

Rules:

- do not use `Furikake` in product naming
- do not imply compatibility branding with third-party suites
- do not describe the tool internally or externally as a clone

Current working name:

- `ONMK ParticleLab`

This is a temporary but acceptable engineering name.
Marketing/product naming can be revised later without changing the core design.

## Implementation Status Snapshot

Current codebase already includes:

- AE parameter-driven entry point
- emitter configuration extraction
- deterministic particle simulation
- child particle support
- primitive particle renderer
- SmartFX render path
- output-origin compensation for SmartFX subregion rendering

Current gaps relative to a stronger v1:

- parameter UI grouping and presentation polish
- higher-quality render look
- better pseudo-depth model
- improved motion blur quality
- deeper bit-depth correctness
- production-safe AE validation

## v1 Acceptance Criteria

ParticleLab v1 is "functionally defined" when all of the following are true:

- plugin loads in AE under the `Particle` category
- emitter position aligns correctly with AE layer coordinates
- basic particle looks can be made with existing controls
- child particles work predictably
- SmartFX path renders in the correct place
- release builds can be produced and deployed as `.aex`
- naming and documentation no longer reference `Furikake`

## Post-v1 Expansion Paths

Potential v2+ tracks:

- layer/mask/text/path emitters
- sprite particles
- richer size/color curve editors
- event-based child emission
- 3D camera-aware controls
- line/point connection rendering
- node graph authoring
- GPU-native render path
- preset system

These should be treated as separate product expansions, not silent v1 creep.
