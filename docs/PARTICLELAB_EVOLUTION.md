# ParticleLab Evolution Plan

ParticleLab should evolve as a stable product with an expanding engine core, not
as a single large host-parameter surface. The outside of the effect stays
compatible. The inside can keep growing.

## Core Idea

Keep this boundary stable:

```text
AE project data -> compatibility adapter -> current engine config
```

Let this boundary evolve:

```text
current engine config -> simulation -> render
```

The plugin can gain new emitters, forces, render modes, node workflows, and
advanced controls as long as old project data is migrated or adapted before it
reaches render code.

## What Is Allowed To Grow

ParticleLab can add:

- new emitter types
- new force and field types
- collision and constraint systems
- improved sprite sampling
- new color, opacity, and size models
- new renderer features
- new composite/apply modes
- new node-only advanced controls
- new preset and graph schema versions

These changes should be represented in the current `ParticleEngineConfig`, then
fed by classic params, presets, or graph compilers as needed.

## What Must Stay Stable

These are compatibility surfaces:

- AE `Match_Name`
- shipped AE parameter order, type, and ID
- preset migration behavior
- graph document migration behavior once graph documents ship
- deterministic interpretation of existing seeds and old saved values when
  possible

If a behavior change is intentional and cannot preserve old looks, it must go
through a migration decision instead of being hidden inside render code.

## Feature Addition Path

For a new ParticleLab feature, use this order:

1. Add engine/runtime capability.
2. Add fields to the current `ParticleEngineConfig`.
3. Decide whether the feature is classic UI, node-only, preset-only, or shared.
4. If classic UI needs it, append AE params at the end of the shipped list.
5. If node UI needs it, add or version a node type.
6. If presets need it, bump preset version only when old data needs migration.
7. Add defaults in adapters, not scattered through render code.
8. Keep renderer consuming current config only.

This keeps new work from leaking backward into old compatibility layers.

## Feature Deprecation Path

For a retired feature:

1. Leave shipped AE params registered.
2. Hide obsolete controls in UI updates.
3. Keep old preset/graph fields loadable.
4. Compile retired data into an equivalent current config when possible.
5. If no equivalent exists, preserve the old data but make it inert.
6. Do not make renderer branch on old schema versions.

## Classic UI And Node UI

Classic UI should remain a good direct-control workflow. Node UI should be a
graph authoring workflow.

They should meet at `ParticleEngineConfig`:

```text
classic params -> ParticleEngineConfig
node graph     -> ParticleEngineConfig
preset         -> ParticleEngineConfig
```

Classic controls do not need to expose every advanced node feature. Some
features can be node-only once graph storage is stable.

## Node UI Adoption Stages

### Stage 0: Classic Only

Current ParticleLab behavior. AE params are the source of truth.

### Stage 1: Engine Config Boundary

Classic params compile to `ParticleEngineConfig`. Render code no longer cares
where the config came from.

### Stage 2: Graph Document Storage

ParticleLab can store a `GraphDocument`, migrate it, and compile it, but the
visible UI can still be limited. Committed graph edits should be atomic: validate
the graph and published metadata, keep the previous active graph if validation
fails, and avoid writing node internals into the AE parameter ABI. External UI
interchange uses a versioned `NodeUiGraphStateSnapshot` rather than raw sequence
data.

### Stage 3: Node UI Beta

Node UI edits a ParticleLab graph that maps to existing engine features. Classic
params can remain visible as fallback or published controls.

### Stage 4: Published Parameters

Selected graph node params can be exposed back to AE as stable public controls.
These published params must be explicit and append-only on the AE side.
Current code stores published-param metadata in `GraphDocument` and validates
stable IDs, target sockets, and default-value types. Sequence state can also
store published value overrides by stable ID and compile them without touching
the AE parameter ABI. Stale overrides are ignored during project render so graph
document evolution does not break older saved sequence data. Mapping those
descriptors to shipped AE controls remains a separate host-adapter step. Node UI
published override edits now have a strict project-state path that deduplicates
by stable ID and rejects unknown or type-mismatched values before mutation.
Snapshot import can also migrate preset-shaped `graph_document` and
`graph_published_values` aliases, so graph presets and panel state can share the
same document payload.
The graph core now exposes a read-only node UI catalog so external tools do not
need to duplicate ParticleLab node/socket/enum metadata. The catalog descriptor
types are product-neutral in `particlelab_engine_core`, and Lattice Lab can
publish its own catalog without depending on ParticleLab graph documents.
The project state now stores `host_float_bindings` by stable published ID and
fixed slot number. Bound host float values can override stored sequence values at
compile time without exposing node IDs or sockets through the AE parameter ABI.
ParticleLab also appends four fixed `Published Float` AE controls at the end of
the host parameter list; they affect render only when graph state binds a stable
published float ID to a slot. Bound slots are renamed in AE from their fixed ABI
names to short graph-facing labels such as `F1: Birth Rate`, while unbound slots
stay disabled.
After those controls, ParticleLab appends a collapsed `Node Graph` tool group.
It gives the current plugin a minimal host-side bridge for exporting/importing
Node UI JSON, seeding graph state from classic params, and temporarily disabling
graph rendering without deleting the saved graph document. Export writes
`NodeUiBootstrapPayload` so external editors receive both catalog and state;
Import accepts both bootstrap JSON and older plain `NodeUiGraphStateSnapshot`
JSON.
`tools/node-ui-shell` is now a visible development shell for that flow: it loads
bootstrap or snapshot JSON, displays the catalog and graph, edits existing node
values, and writes JSON that can go back through the same import path.
ParticleLab also appends a separate `Node UI Sidecar` launch group after the
Node Graph controls. That command writes the embedded shell assets to the user's
Documents folder, generates a startup payload for the current effect instance,
and opens them through the OS shell, giving AE users a host-launched editor
workflow without inserting new controls into the existing ABI range.

### Stage 5: Advanced Node Features

Add features that are easier or only practical in graph form, such as layered
forces, multiple emitters, masks as graph inputs, or per-branch render controls.

## Relationship To Lattice Lab

ParticleLab should build the graph/document/editor pattern first because it has
an existing renderer and real workflows. Lattice Lab should come after the
pattern is proven.

Reusable:

- graph document shape
- migration conventions
- shared editor UI ideas
- low-level image/camera/color/render-buffer utilities

Separate:

- AE effect identity
- host parameter ABI
- engine config
- graph compiler
- product presets
- simulation and topology logic

Lattice Lab can use the same graph language style, but it should compile into
`LatticeEngineConfig`, not `ParticleEngineConfig`.

## Practical First Implementation Steps

1. Introduce `ParticleEngineConfig`.
2. Make `extract_configs()` return `ParticleEngineConfig`.
3. Move classic extraction toward `engine/classic.rs`.
4. Add `ParticleRenderPlan` without changing output.
5. Split preset snapshot/migration into `preset/`.
6. Add graph document structs with no UI yet.
7. Add a graph compiler that can reproduce a simple classic point emitter.

The first code changes should be boring on purpose. If a refactor changes output
pixels, it is probably doing too much for that phase.

Current implementation status:

- Steps 1-3 are started: `ParticleEngineConfig` lives in
  `crates/particlelab_engine_core/src/engine.rs`, and classic AE params compile
  through `classic_params::extract_engine_config()`.
  `ParticleEngineConfig::normalized_for_render()` now acts as the current-config
  boundary before render, clamping unsafe graph/preset values and removing
  runtime assets without changing the AE parameter ABI. ParticleLab keeps thin
  wrapper modules in `src/` for compatibility with existing host-adapter code.
- Step 4 is started: `ParticleRenderPlan` carries output surface, time, and
  resolved apply mode for both legacy and SmartFX render paths.
  `ParticleRuntimeInputs` now carries the shared
  `particlelab_engine_core::render_core::RenderFrame` plus per-frame source
  points, sprite images, and camera projection so saved engine config is not
  rewritten with host runtime data. The shared engine-core helper now owns
  particle-system creation, simulation, and particle drawing, while engine-core
  renderer helpers own origin-aware ARGB blit and final composite. Future
  particle-family effects can start from `particlelab_engine_core::prelude`;
  the external `public_api` integration test proves this render path compiles
  and runs without AE host types, including a product-specific adapter shape and
  padded host row-stride output.
- Step 5 is implemented for the current classic preset surface: `src/preset.rs`
  owns preset schema, migration, and direct compilation to
  `ParticleEngineConfig`. Classic AE "apply preset to params" remains a host
  adapter in `src/lib.rs`. Graph presets can carry both `graph_document` and
  published value overrides. Preset schema version 6 stores those overrides and
  migrates v5/missing-version presets with an empty override list.
- Steps 6-7 are started: `src/graph.rs` has GraphDocument V0 and a simple
  point-emitter compiler path. Preset JSON can now carry an optional
  `graph_document` and `graph_published_values` for portable Node UI
  interchange. `src/project_state.rs` now stores graph runtime state in AE
  sequence data and render can use a
  compiled graph instead of classic params. `GraphDocument::from_engine_config()`
  can seed a graph from existing classic params, including all current emitter
  types. Graph document migration is now centralized for preset and sequence
  data loads. Graph-published parameter metadata and sequence-state value
  overrides are stored and validated. Sequence payload version 3 adds published
  value overrides plus host-float bindings while continuing to load v1 and v2
  graph sequence data. Node UI has a project-state commit bridge for atomic
  document replacement and strict published-value updates, plus a versioned
  `NodeUiGraphStateSnapshot` JSON exchange shape. Four append-only AE host float
  controls are registered and can drive bound graph-published floats. An
  append-only `Node Graph` tool group can export/import snapshots, seed a graph
  from classic params, and disable graph rendering. Bound host float slots now
  show graph-facing labels in AE; the visible node editor is not wired yet.
- Lattice Lab has a standalone path now:
  `crates/lattice_lab_plugin/src/lattice.rs` owns separate config, a separate
  `lattice.*` graph document, a separate compiler, and a core render plan that
  produces points, links, and mesh edges. It also has a core 8-bit ARGB raster
  path for those primitives, plus a Lattice-specific node UI catalog using
  shared catalog descriptors.
  `crates/lattice_lab_plugin/src/lattice_project_state.rs` adds
  Lattice-specific project state, Node UI snapshot, and bootstrap payloads.
  `crates/lattice_lab_plugin/src/lattice_ae_adapter.rs` adds the Lattice
  host-adapter boundary with effect identity constants, its own ABI manifest,
  classic host params, graph override resolution, and ARGB8 rendering into host
  buffers. `crates/lattice_lab_plugin` registers a separate AE effect package
  with its own PiPL/`EffectMain`,
  `LatticeLab` match name, parameter ABI, sequence data, render path, and
  Node UI JSON export/import buttons. It is intentionally not connected to
  ParticleLab's AE effect identity, parameter ABI, preset snapshot, graph
  document, or project state. Its compiler now shares only product-neutral
  `particlelab_engine_core` traversal/catalog helpers with ParticleLab, and its
  runtime/renderer share `RenderFrame`, `RenderSurface`, and ARGB8 blending from
  that crate for buffer validation and drawing. Lattice Lab also has its own
  appended Node UI sidecar launch group that writes a Lattice startup payload.
  ParticleLab no longer compiles Lattice product modules.

## Design Checks

Before adding a feature, ask:

- Does this belong to ParticleLab, Lattice Lab, or a shared utility?
- Is this a host parameter, graph node parameter, preset field, or runtime input?
- What is the default for old projects?
- Can classic params and graph compile to the same current config?
- Does render code stay free of old schema checks?
- Is this addition append-only at the AE parameter layer?
- Does `tools\verify_particlelab_migration.ps1` still pass before AE deployment?

If the answer is unclear, write the adapter/migration shape first and postpone
the UI surface.
