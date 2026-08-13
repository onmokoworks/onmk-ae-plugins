# ParticleLab Engine Core Design

ParticleLab should grow a reusable engine core inside the current plugin before
the node UI and Lattice Lab are built out. The goal is not to make a generic
framework too early. The goal is to put a clean boundary between host/UI data,
versioned documents, and render-time execution.

For product-level evolution rules, see `PARTICLELAB_EVOLUTION.md`.

## Goals

- Keep the shipped ParticleLab AE effect stable.
- Let classic AE parameters, presets, and future node graphs produce the same
  render configuration shape.
- Keep render code free from old preset versions, old graph versions, and AE
  parameter compatibility details.
- Make the first node UI useful for ParticleLab while keeping the graph model
  reusable for Lattice Lab later.
- Share low-level utility code with Lattice Lab without coupling the two
  products to the same host parameter ABI.

## Non-Goals

- Do not turn ParticleLab into a mode switcher for every future effect.
- Do not represent arbitrary nodes and edges as AE parameters.
- Do not extract a broad shared framework before the boundaries are clear in
  ParticleLab. Keep shared crates limited to proven, AE-free core utilities.
- Do not make the renderer understand multiple historical schemas.

## Target Layers

```text
AE host params       Preset JSON          GraphDocument
     |                   |                     |
     v                   v                     v
classic adapter     preset migrate        graph migrate
     |                   |                     |
     +---------+---------+----------+----------+
               |                    |
               v                    v
        ParticleEngineConfig   ParticleGraphCompiler
               |                    |
               +---------+----------+
                         |
                         v
                  ParticleRenderPlan
                         |
                         v
                  simulation + render
```

The important boundary is `ParticleEngineConfig`. Everything above it is input
format. Everything below it is runtime execution.

## Proposed Module Layout

The ParticleLab host crate keeps the AE adapter in `ParticleLabRust/src`, while
AE-free execution code now lives in `crates/particlelab_engine_core`:

```text
crates/particlelab_engine_core/src/
  engine.rs
  particle.rs
  renderer.rs
  render_core.rs
  node_graph_core.rs

src/
  lib.rs                  # AE adapter and shipped ParticleKit ABI
  classic_params.rs       # classic AE params -> ParticleEngineConfig
  graph.rs                # ParticleLab graph schema/compiler
  preset.rs               # ParticleLab preset schema/migration
  project_state.rs        # ParticleLab AE sequence state
```

The thin `src/engine.rs`, `src/particle.rs`, and `src/renderer.rs` modules
re-export the core crate for existing ParticleLab code paths, so this extraction
does not change the shipped AE parameter ABI.

## Core Types

### ParticleEngineConfig

The canonical current configuration for ParticleLab rendering.

It should contain only product-level render intent:

- emitter config
- motion config
- physics config
- appearance config
- child particle config
- sprite/image sampling config
- render/composite config
- seed and deterministic options

It should not contain:

- AE parameter indices
- UI enabled/disabled state
- preset version fields
- graph node IDs
- file dialog state
- cache handles

Current `extract_configs()` already points in this direction. The next version
should return one struct instead of a tuple:

```rust
pub struct ParticleEngineConfig {
    pub emitter: EmitterConfig,
    pub physics: PhysicsConfig,
    pub appearance: AppearanceConfig,
    pub child: ChildConfig,
    pub render: RenderConfig,
    pub seed: u64,
}
```

Current implementation also provides
`ParticleEngineConfig::normalized_for_render()`. Every classic-param, preset,
or graph-produced config is normalized before runtime data is applied. This is
the compatibility boundary for future engine fields: adapters may preserve old
defaults and migration behavior, while render receives finite, bounded current
values and no embedded runtime assets.

### ParticleRuntimeInputs

Data that changes per render request and cannot live inside a preset or graph:

- current time
- frame duration
- output surface and origin
- camera projection
- checked-out source layers converted to flat image data
- sampled path points
- layer alpha emitter points

The AE bridge should collect this data on the allowed thread, then pass owned
runtime inputs into render code.

```rust
pub struct RenderFrame {
    pub surface: RenderSurface,
    pub origin_x: i32,
    pub origin_y: i32,
    pub time: f32,
    pub dt: f32,
}

pub struct ParticleRuntimeInputs {
    pub frame: RenderFrame,
    pub source_points: Option<Arc<Vec<glam::Vec3>>>,
    pub sprite_images: Vec<SpriteImage>,
    pub camera_projection: Option<CameraProjection>,
}
```

`RenderFrame` is intentionally product-neutral. ParticleLab and Lattice Lab now
share it for ARGB8 surface, origin, time, and frame delta validation, while each
product keeps its own runtime assets and render plan.

### ParticleRenderPlan

A validated, ready-to-render combination of config and runtime inputs.

This layer can clamp expensive values, resolve fallbacks, choose final composite
mode, and precompute derived values. Rendering should consume a plan instead of
re-reading AE params.

```rust
pub struct ParticleRenderPlan {
    pub runtime: ParticleRuntimeInputs,
    pub final_apply_mode: Option<ApplyMode>,
}
```

Current implementation passes `ParticleEngineConfig` alongside
`ParticleRenderPlan` to the render helper. The plan owns runtime data and
applies it to cloned render-time structs, so persisted configs are not mutated
with frame-specific AE assets.

## Classic Parameter Adapter

Classic ParticleLab controls should compile to `ParticleEngineConfig`:

```text
ae::Parameters<Params> -> classic::extract_engine_config()
```

This adapter is the only layer that should know `Params`. It can preserve old
AE behavior, apply compatibility defaults, and hide legacy controls. The engine
core should not import `Params`.

## Preset Adapter

Preset loading should be:

```text
JSON -> PresetSnapshotVn -> migrate -> current PresetSnapshot -> EngineConfig
```

Applying a preset to AE controls is still useful for the classic UI. But the
canonical path should also support compiling a preset directly to engine config
for future graph/node workflows.

Current code: `src/preset.rs` owns `PresetSnapshot`, preset migration, and
`PresetSnapshot::to_engine_config()`. Presets remain backward-compatible classic
snapshots by default, and may also carry an optional `graph_document` field.
When that graph compiles, it becomes the engine source; otherwise the classic
snapshot fields remain the fallback path. Graph presets may also carry
`graph_published_values`, the portable published-value overrides used by Node UI
and future fixed host bindings.

## Graph Adapter

The first graph version should be ParticleLab-focused. It should not try to
model Lattice Lab yet, but it should use namespaced node types from day one:

```json
{
  "type": "particle.emitter.point",
  "version": 1
}
```

The graph compiler should be product-specific:

```text
GraphDocument -> ParticleGraphCompiler -> ParticleEngineConfig
```

Later, Lattice Lab can add:

```text
GraphDocument -> LatticeGraphCompiler -> LatticeEngineConfig
```

The document format can be shared. The compilers and engine configs should stay
separate.

## Node UI Version 0

A useful first node set for ParticleLab:

- `particle.emitter.point`
- `particle.emitter.box`
- `particle.emitter.layer_alpha`
- `particle.motion.direction`
- `particle.force.gravity`
- `particle.force.wind`
- `particle.force.turbulence`
- `particle.appearance.color_over_life`
- `particle.appearance.size_over_life`
- `particle.sprite.source`
- `particle.render.output`

Version 0 should compile to features that already exist in the classic engine.
That keeps the first UI project low-risk: the node editor changes how the user
describes the effect, not what the renderer must support.

Published parameters live inside `GraphDocument` as explicit metadata:
stable ID, label, target node/socket, value type, and default value. Missing
`published_params` defaults to an empty list so old graph JSON remains loadable.
Validation checks duplicate IDs, target sockets, and default-value types before
any future UI maps them to AE controls. `GraphProjectState` can now store
published value overrides by stable ID and compile them through the same graph
compiler path; this keeps arbitrary node internals out of the AE parameter ABI
while still giving Node UI and future fixed host controls a stable binding
surface. Project render uses a compatibility compile path for published values:
stale or type-mismatched overrides are ignored and the valid graph still
renders, while strict override validation remains available for tools and tests.
Node UI should commit graph edits through `ParticleLabProjectState` instead of
mutating sequence fields directly: `commit_node_graph_document()` validates the
graph and published metadata before replacing the active document, preserves only
compatible published-value overrides, and leaves the previous renderable graph in
place when an invalid edit is rejected.
The external UI exchange shape is `NodeUiGraphStateSnapshot` version 2:
`version`, `enabled`, `document`, `published_values`, and
`host_float_bindings`. It is deliberately separate from raw AE sequence data,
and it migrates preset-shaped aliases (`graph_document`,
`graph_published_values`) before commit so preset interchange and panel
interchange can share graph payloads without exposing project internals. Future
snapshot versions are rejected before commit, preserving the previous renderable
graph state.

The editor bootstrap shape is `NodeUiBootstrapPayload` version 1. It contains
the read-only catalog and a `NodeUiGraphStateSnapshot`; it is useful for a
visible editor or external panel that needs the node palette and current project
state in one file/message. Import remains backward-compatible by accepting both
plain snapshot JSON and bootstrap JSON.

`host_float_bindings` is the bridge to AE keyframable controls. It stores only
stable published IDs and fixed float-slot numbers, not node IDs or sockets.
ParticleLab now appends four fixed `Published Float` host parameters at the end
of the AE parameter ABI. When a graph binding maps a stable ID to one of those
slots, render reads the keyed host value through the binding table without
changing graph JSON. The same binding table drives AE parameter UI state:
unbound slots are disabled, and bound slots are renamed from their fixed ABI
names to short labels such as `F1: Birth Rate`.

The AE adapter also appends a collapsed `Node Graph` tool group after the
published host controls. It is not the final node editor; it is a compatibility
bridge for external panels and development tools. The group can export the
current `NodeUiBootstrapPayload`, import bootstrap or snapshot JSON through the
strict project-state commit path, seed graph state from current classic params,
or disable graph rendering while leaving the document in sequence data. A
separate appended `Node UI Sidecar` group can write the embedded static shell to
`Documents/Particle Kit/node-ui-shell`, generate `startup-payload.js` from the
current effect's bootstrap payload, and open it through the OS shell. This is the
compatibility-safe bridge before a real embedded panel exists: it adds only new
tail parameters and does not insert controls into the existing ABI range.

`tools/node-ui-shell` is the first visible shell for that exchange format. It is
static HTML/JS, loads bootstrap or snapshot JSON, renders a catalog-driven node
palette and graph view, lets existing node values be edited, and can export
bootstrap or snapshot JSON back to the AE import path when no blocking
diagnostics are present. Its ParticleLab and Lattice fixture payloads are parsed
and committed by Rust tests, so the shell samples stay aligned with the real
project-state import boundary. It deliberately stays outside the AE parameter
ABI.

`particlelab_engine_core::node_graph_core` now owns the product-neutral Node UI
catalog descriptor types. ParticleLab exposes a read-only
`particle_node_ui_catalog()` for Node UI tools. It describes the current
ParticleLab node types, connection sockets, editable value sockets, value types,
and enum options without using AE parameters or sequence data. This keeps the
future visible editor aligned with the compiler/published-param validation
surface instead of duplicating socket lists in UI code.

## Lattice Lab Reuse

Lattice Lab should reuse utilities, not ParticleLab product state.

Good shared candidates:

- `particlelab_engine_core::node_graph_core`
- `particlelab_engine_core::render_core`
- Node UI catalog descriptors
- `camera`
- `color`
- `blend`
- `flat_image`
- `mip`
- `cache_key`
- `render_buffer`
- `ae_dialog`
- `ae_logging`

Poor shared candidates:

- ParticleLab `Params`
- ParticleLab preset snapshot
- ParticleLab graph compiler
- ParticleLab engine config
- Particle simulation state

When adding another effect, depend on the small engine-core crate only for
utilities that have already proven stable in more than one product. Keep
product schemas and compilers separate.

Status: started. `crates/particlelab_engine_core` now contains the AE-free
Particle execution core (`engine`, `particle`, `renderer`) plus
product-neutral graph helpers, Node UI catalog descriptor types, ARGB8 render
surface validation, `RenderFrame`, and ARGB8 pixel blending. ParticleLab's
`GraphDocument` and Lattice's `LatticeGraphDocument` keep separate serialized
schemas, node namespaces, compiler errors, and engine configs, but both compiler
paths use the same engine-core traversal primitives. Lattice Lab is a separate
workspace package and depends on this crate for shared core behavior instead of
path including those modules.

The crate also exposes `particlelab_engine_core::prelude` for future
particle-family effects. `crates/particlelab_engine_core/tests/public_api.rs`
is an external integration test that constructs `ParticleEngineConfig`,
`ParticleRuntimeInputs`, and `ParticleRenderPlan`, renders into an ARGB buffer,
validates `RenderSurface`, and compiles a small product-specific adapter into
the core engine without importing AE or ParticleLab host modules. It renders one
case through `ParticleRuntimeInputs::argb8_with_row_bytes` so new hosts can use
padded output buffers without manually constructing core internals. Treat that
test as the minimum downstream compatibility contract for new particle effects.

## Migration Plan

### Phase 1: Name The Boundary

- Add `ParticleEngineConfig`.
- Make existing `extract_configs()` return `ParticleEngineConfig`.
- Keep render behavior unchanged.
- Add unit tests around default config extraction where possible.

Status: started. `ParticleEngineConfig` now lives in
`crates/particlelab_engine_core/src/engine.rs`, re-exported through the
ParticleLab host crate, and the classic AE parameter adapter lives in
`src/classic_params.rs`.

### Phase 2: Move Runtime Inputs Out Of AE Calls

- Introduce `ParticleRuntimeInputs`.
- Move source image, path, camera, and output rect data into owned runtime data.
- Keep SmartPreRender as the place that touches AE APIs.

Status: started. `ParticleRuntimeInputs` now carries a shared
`particlelab_engine_core::render_core::RenderFrame` plus Particle-specific
source points, sprite images, and camera projection. The AE host adapter
collects those inputs during legacy render or SmartPreRender, then
`ParticleRenderPlan` applies them to the
render-time emitter/render structs. Classic params, preset snapshots, and graph
documents continue to produce stable `ParticleEngineConfig` data without
mutating it with per-frame assets.

### Phase 3: Render From A Plan

- Add `ParticleRenderPlan`.
- Move final apply mode, output size, origin, and derived values into the plan.
- Make legacy render and SmartFX render share more of the same execution path.

Status: started. `ParticleRenderPlan` now lives in
`crates/particlelab_engine_core/src/engine.rs` and is used by both legacy render
and SmartFX render for the shared `RenderFrame` and final apply-mode resolution.
`render_particle_engine_8bit()` also centralizes particle-system creation,
simulation, and 8-bit particle draw. Origin-aware ARGB blit and final composite
now live in `crates/particlelab_engine_core/src/renderer.rs`. AE layer checkout
and output writes remain in the host adapter layer.

### Phase 4: Preset Compile Path

- Move preset snapshot and migration into `preset/`.
- Add conversion from current preset snapshot to `ParticleEngineConfig`.
- Keep classic "apply preset to AE params" as a UI operation, not the only
  way presets can be used.

Status: implemented for the current classic preset surface. `src/preset.rs`
owns `PresetSnapshot`, version migration, and
`PresetSnapshot::to_engine_config()`. Classic "apply preset to AE params" stays
in `src/lib.rs` as a host adapter operation. Preset schema version 6 can carry
optional `graph_document` data and `graph_published_values`, letting Node UI
presets compile through the same engine path without changing the AE parameter
ABI. Version 5 and missing-version presets migrate to the current snapshot, with
missing published values materialized as an empty override list.

### Phase 5: Graph Document V0

- Add `GraphDocument`, node IDs, sockets, edges, and node versions.
- Add migration stubs even if only schema version 1 exists.
- Add `ParticleGraphCompiler` that produces `ParticleEngineConfig`.
- Start with nodes that map to existing classic controls.

Status: started. `src/graph.rs` contains a versioned `GraphDocument`, namespaced
ParticleLab node types, sockets, edges, and a compiler that can reproduce the
classic simple point-emitter default. `PresetSnapshot` can hold a
`graph_document`, and `src/project_state.rs` can persist graph runtime state in
AE sequence data. `GraphDocument::from_engine_config()` can seed a graph from
the current classic engine config, including a generic emitter node for Point,
Box, Sphere, Grid, Layer Alpha, and Path emitters. UI editing is not wired yet.
Graph loading now goes through `GraphDocument::from_value()` /
`migrate_graph_document_value()`, so presets and AE sequence data have a single
place to materialize old graph JSON into the current document shape before
compilation.

### Phase 6: Node UI Bridge

- Store graph data separately from the AE parameter list.
- AE params expose only stable public controls and fallback behavior.
- Node UI edits `GraphDocument`; render code receives compiled config.

Status: started. `ParticleLabProjectState` is now the effect sequence-data
payload. It serializes to AE project data independently of the AE parameter
ABI, stores optional graph state, and lets render choose between classic params
and a compiled `GraphDocument`. Existing projects with no sequence data default
to classic params. A Node UI can now initialize graph state from classic params
without shifting or rewriting the AE parameter ABI. `GraphDocument` also now
stores and validates graph-published parameter metadata, and
`GraphProjectState` stores optional published value overrides that are applied
before compile. Sequence payload version 3 adds host-float binding data while
still reading version 1 graph payloads without overrides and version 2 payloads
without host bindings. Stale override values and stale bindings in older
sequence data are ignored rather than forcing a fallback to classic params. The
Node UI bridge can now commit renderable graph documents atomically, toggle graph
usage only when a document exists, strictly upsert or clear published-value
overrides by stable ID, bind compatible float published params to append-only AE
float slots, and compile with bound host-float values overriding stored sequence
values. It also exports/imports a versioned `NodeUiGraphStateSnapshot` for
external Node UI panels, including migration from preset-style graph field names.
The shipped AE UI now includes a small append-only `Node Graph` tool group for
that exchange path: Export, Import, Seed, and Disable. Those buttons are host
adapter controls only; the graph document and published values still live in
sequence data.

### Phase 7: Lattice Lab

- Create standalone effect identity and ABI.
- Add `LatticeEngineConfig`.
- Reuse proven low-level modules.
- Add Lattice node types and a Lattice graph compiler.
- Add Lattice project state and Node UI exchange payloads.
- Add a Lattice render plan that produces product-owned geometry before any AE
  host integration.
- Add a core Lattice raster path that can fill an ARGB buffer before the AE
  adapter exists.

Status: started and now host-splittable.
`crates/lattice_lab_plugin/src/lattice.rs` defines a separate
`LatticeEngineConfig`, `LatticeGraphDocument`, Lattice node namespace, graph
compiler tests, and a core `LatticeRenderPlan` that turns compiled config into
points, links, and mesh edges. It also has a core 8-bit ARGB raster function for
links, mesh edges, and points, plus a read-only `lattice_node_ui_catalog()`
built with the same product-neutral catalog descriptors as ParticleLab.
`crates/lattice_lab_plugin/src/lattice_project_state.rs` adds separate Lattice
sequence-state, snapshot, and bootstrap payload types.
`crates/lattice_lab_plugin/src/lattice_ae_adapter.rs` owns the standalone
host-adapter boundary: distinct `Lattice Lab` identity constants, a separate
append-only ABI manifest, classic host params that compile to
`LatticeEngineConfig`, graph-state override resolution, and an ARGB8 render
request path that clears and draws into host-sized output buffers.

`crates/lattice_lab_plugin` registers `Lattice Lab` as a separate AE effect
package with its own PiPL and `EffectMain`. It renders through the Lattice
adapter, persists `LatticeLabProjectState`, and can seed/disable graph state or
export/import Node UI bootstrap JSON without touching ParticleLab `Params`,
`PresetSnapshot`, `GraphDocument`, or `ParticleEngineConfig`. The root workspace
defaults to ParticleLab plus `particlelab_engine_core`; build the standalone
effect explicitly with `cargo build --release -p lattice_lab_plugin` or include
it in a workspace build. ParticleLab no longer compiles the Lattice product
modules; the standalone crate now shares only product-neutral behavior with
ParticleLab through `particlelab_engine_core`: graph traversal, Node UI
catalog descriptors, `RenderFrame`, `RenderSurface`, and ARGB8 blending for
output surface validation and rasterization. Lattice Lab also appends its own
`Node UI Sidecar` launch group and writes a generated Lattice startup payload to
`Documents/Lattice Lab/node-ui-shell`.

`tools/verify_ae_release.ps1` now provides a lightweight release smoke check for
this split. It verifies both release DLLs, AE entry point markers, expected
ParticleKit/LatticeLab identity strings, and deploy names before the binaries are
copied into AE's plugin folder.

`tools/run_ae_smoke.ps1` is the deployed-host follow-up check. It runs an
ExtendScript smoke script in After Effects, adds both effects by match name in a
temporary empty project, writes `target\ae-smoke\ae_smoke_report.json`, and
closes the temporary project without saving. This keeps the identity split
testable inside AE without coupling the engine-core crate to host APIs.

`tools/verify_architecture.ps1` is the source-level boundary guard. It checks
that ParticleKit keeps the shipped match name, engine-core has no AE/PiPL
dependency, Particle execution modules are owned by engine-core and re-exported
by thin host wrappers, Lattice Lab owns its product modules inside its standalone
crate, and the compatibility guard tests remain present.

`tools/verify_particlelab_migration.ps1` is the non-elevated migration gate. It
collects the source boundary verifier, ABI manifest verifier, compatibility
contract verifier, goal audit verifier, Node UI shell verifier, Rust tests,
release builds, and binary release verifier into one command. AE deployment and
host loading remain a separate external gate through `tools/run_ae_smoke.ps1`
with `SystemMediaCore`, so the local migration contract can stay repeatable
without requiring Program Files write access.

## Invariants

- Engine code does not import AE `Params`.
- Renderer code does not parse JSON.
- Migration code does not render.
- Graph node IDs do not leak into particle simulation.
- Node UI catalog descriptors are shared, but serialized product graph documents
  remain separate.
- Product engines validate output surfaces through shared engine-core helpers,
  not AE adapter assumptions.
- AE parameters stay append-only.
- Lattice Lab gets its own ABI and product config.
- Lattice Lab gets its own project-state schema and Node UI exchange payloads.
- Classic UI and Node UI compile to the same current engine config.
- Local migration verification stays non-elevated; deployed AE host smoke is the
  only expected elevated compatibility gate.

## Open Design Questions

- AE project graph storage uses sequence data first. Preset JSON still supports
  `graph_document` as the portable interchange/storage shape.
- Should Node UI be embedded directly in the effect UI or controlled by an AEGP
  panel that edits the effect data?
- Which classic controls remain visible once a graph is active?
- Should Node UI preset saves write both classic fallback fields and
  `graph_document`, or graph-only with generated fallback fields?
