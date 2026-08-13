# ParticleLab Architecture

This project should keep three concerns separate:

- ParticleLab: the stable particle effect.
- Lattice Lab: the standalone point-network effect, formerly discussed with
  the internal Plexus prototype name.
- Node UI: the graph editor and graph document model that can drive ParticleLab
  later without turning every node into AE host parameters.

## Product Boundaries

### ParticleLab

ParticleLab owns the particle simulation, particle rendering, particle presets,
and the existing AE effect identity. Its host parameter list is append-only.
Shipped parameters are never removed, renamed, reordered, or repurposed in ways
that would change old project data.

ParticleLab may eventually gain a node UI, but the node graph should compile
down to the same kind of engine config the current renderer already consumes.
The renderer should not need to know whether a config came from classic AE
controls, a preset, or a node graph.

The AE-free Particle execution path now lives in
`crates/particlelab_engine_core`: `engine.rs` owns `ParticleEngineConfig`,
`ParticleRuntimeInputs`, `ParticleRenderPlan`, and the shared 8-bit render entry
point; `particle.rs` owns simulation; `renderer.rs` owns particle drawing,
blitting, and final composite helpers. The ParticleLab host crate re-exports
those modules through thin `src/` wrappers while keeping the shipped
`ParticleKit` ABI in `src/lib.rs`.

Future particle-family effects should depend on
`particlelab_engine_core::prelude` for this surface rather than importing
ParticleLab's AE host crate. The integration test
`crates/particlelab_engine_core/tests/public_api.rs` compiles from outside the
core crate, renders particles without any AE types, and includes a tiny
downstream adapter example that compiles a product-specific config into
`ParticleEngineConfig`. That test also renders into a padded ARGB8 buffer via
`ParticleRuntimeInputs::argb8_with_row_bytes`, so host row-stride assumptions do
not leak into the particle engine contract.

The detailed engine-core plan lives in `ENGINE_CORE_DESIGN.md`. The product
evolution rules live in `PARTICLELAB_EVOLUTION.md`.

### Lattice Lab

Lattice Lab is the planned standalone point-network effect. It should not be a
mode inside ParticleLab. It should have its own:

- AE `Match_Name`
- display name
- parameter ABI
- preset folder
- graph/config schema
- release versioning

Low-level code can still be shared with ParticleLab: camera projection, color
math, image flattening, compositing, cache helpers, product-neutral graph
traversal helpers, ARGB8 render frame/surface validation, and common AE
utilities. Avoid sharing host parameter enums or product-specific preset
structs.

Implementation note: `crates/lattice_lab_plugin/src/lattice.rs` contains the
product boundary: separate Lattice config structs, a `lattice.*` graph document,
a Lattice graph compiler, and a core render plan that produces points, links,
and mesh edges. It also has a core 8-bit ARGB raster path for those primitives.
`crates/lattice_lab_plugin/src/lattice_project_state.rs` adds a separate
project-state schema and Node UI snapshot/bootstrap payload.
`crates/lattice_lab_plugin/src/lattice_ae_adapter.rs` adds the standalone
host-adapter layer: effect identity constants, a Lattice-only ABI manifest, host
params that compile to `LatticeEngineConfig`, graph override resolution, and an
ARGB8 render request helper.

`crates/lattice_lab_plugin` is the first separate AE effect crate. It has its
own PiPL, `EffectMain`, `LatticeLab` match name, host parameter enum, sequence
data, and render path. It depends on `crates/particlelab_engine_core` for
product-neutral graph traversal, Node UI catalog descriptors, ARGB8 render
surface validation, and ARGB8 blending, while still reusing only the
Lattice-specific modules inside its own crate. ParticleLab's existing
`ParticleKit` identity and parameter ABI stay untouched, and ParticleLab no
longer compiles Lattice product modules. Build it explicitly with
`cargo build --release -p lattice_lab_plugin`; the workspace default members are
ParticleLab plus the shared engine-core crate.
Build and deployment names are documented in `BUILD_AND_DEPLOY.md`.

### Node UI

The node UI should be a graph editor over a versioned `GraphDocument`, not a
large dynamic AE parameter list. AE parameters are good for stable public knobs,
entry points, compatibility, and essential fallback controls. They are not a
good storage model for arbitrary nodes, sockets, and edges.

The graph flow should be:

```text
GraphDocument -> migrate -> compile -> EngineConfig -> render
```

Only the migration layer should understand old graph schemas. Render code should
consume current configs only.

Implementation note: `src/graph.rs` currently provides the first versioned
`GraphDocument` shape, a ParticleLab compiler, and a
`ParticleEngineConfig -> GraphDocument` adapter for seeding Node UI state from
classic controls. It also exposes `particle_node_ui_catalog()`, a read-only
catalog of current ParticleLab node types, connection sockets, editable value
sockets, value types, and enum options for the future visible editor or
external panel. The catalog descriptor types live in
`crates/particlelab_engine_core/src/node_graph_core.rs` so Lattice Lab can
publish its own catalog without sharing ParticleLab graph documents.
`src/project_state.rs` stores optional graph state in AE sequence data
and lets render use compiled graph config while classic AE params remain the
fallback. Embedded graph JSON is migrated through a graph-specific adapter
before compile, rather than letting render code interpret old schemas.
Node UI commits should go through `ParticleLabProjectState` edit methods so a
bad document or bad published override cannot partially mutate the active
renderable graph. External Node UI/panel interchange should use
`NodeUiGraphStateSnapshot` version 2, not raw sequence data. The snapshot carries
`enabled`, `document`, `published_values`, and `host_float_bindings`, and can
migrate preset-shaped `graph_document` / `graph_published_values` aliases.
Snapshots with a newer version are rejected without mutating the current graph.
For editor startup, `NodeUiBootstrapPayload` wraps the current catalog plus that
snapshot, while import still accepts plain snapshot JSON for compatibility.
ParticleLab exposes an append-only `Node Graph` AE tool group for this bridge:
Export writes bootstrap JSON, Import accepts bootstrap or snapshot JSON, Seed
creates a graph from the current classic params, and Disable switches rendering
back to classic params without erasing the stored graph document.

`tools/node-ui-shell` is the first visible development UI over this payload. It
is a static shell that can load/export bootstrap or snapshot JSON, inspect the
catalog, visualize graph nodes and edges, and edit existing node values. It is
not an AE panel yet, but it exercises the same JSON boundary that an AE-hosted
panel should use.

## Data Compatibility Layers

### AE Host Parameters

AE host parameters are an ABI. See `PARAMETER_COMPATIBILITY.md`.

### Presets

Preset JSON should keep a top-level `version`. Loading a preset should migrate
it into the current snapshot type before applying values to AE parameters or
engine config.

Current presets are owned by `src/preset.rs`. They keep classic fields for
backward-compatible AE UI application and may carry an optional
`graph_document` plus `graph_published_values` for Node UI interchange. Preset
schema version 6 adds the published override payload while loading older v5 or
missing-version presets with an empty override list.

Current project graph state is owned by `src/project_state.rs` and serialized
through AE sequence data. Loading a classic project with no sequence data falls
back to classic params. Sequence payload version 3 stores graph published value
overrides and append-only host-float bindings for the fixed `Published Float` AE
controls, while still accepting v1/v2 graph payloads without the newer fields.

### Graph Documents

Graph documents should use a schema like:

```json
{
  "schema_version": 1,
  "nodes": [
    {
      "id": "emit_01",
      "type": "emitter.point",
      "version": 1,
      "params": {
        "birth_rate": 180.0,
        "position": [50.0, 50.0, 0.0]
      }
    }
  ],
  "edges": [
    { "from": "emit_01.out", "to": "render_01.in" }
  ],
  "published_params": []
}
```

Every node type should also have its own version so individual node migrations
can be small and explicit.

Current migration code materializes missing document-level fields, missing node
versions, and early un-namespaced ParticleLab node type aliases before serde
decoding. Future graph schema bumps should extend that migration layer instead
of branching inside render code.

`published_params` is graph metadata, not host ABI. Each item names a stable
published ID, label, target node/socket, value type, and default value. The AE
adapter maps selected float published IDs onto shipped keyframable fixed slots
without storing arbitrary graph internals in the host parameter list.
Project sequence data may store current published values by stable ID; the graph
compiler applies those values to a cloned graph document before producing
`ParticleEngineConfig`. Sequence payload version 3 also stores
`host_float_bindings`, a fixed-slot bridge for append-only AE keyframable float
controls. The host adapter uses those bindings to enable and relabel only the
slots that are currently bound, while unbound fixed slots remain disabled. Stale
published values and stale host bindings are treated as compatibility debris and
ignored during project render so old sequence data can survive graph document
edits. The stricter Node UI bridge rejects invalid committed edits, deduplicates
overrides by stable ID, and preserves only compatible overrides/bindings when a
document is replaced. Versioned
`NodeUiGraphStateSnapshot` JSON is the portable project-facing UI state, while AE
sequence data remains the host-owned persistence layer.

## Naming

Do not ship the standalone point-network effect as `Plexus`. Use that only as a
legacy/internal reference when dealing with old prototype parameter slots.

Working title: `Lattice Lab`

Shortlist:

- `Lattice Lab`: fits point grids, links, meshes, and the existing Lab naming.
- `PointWeave`: expressive, good for point and line networks.
- `LinkField`: technical, good for procedural connection fields.
- `Constellation`: visual and friendly, but narrower than mesh/network work.

Until the first standalone release, keep user-facing docs and plugin metadata on
the working title and keep internal migration code explicit about any old
prototype names it preserves.
