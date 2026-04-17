# Node Graph Roadmap

## Goal

Evolve ParticleLab from a single-system particle effect into a graph-driven particle tool without losing AE-native usability or preview speed.

## Phase 1: Internal graph only

Represent the particle pipeline as a small graph in code, even if the UI is still standard AE params.

Initial node families:

- `EmitterNode`
- `MotionNode`
- `ForceNode`
- `DeformNode`
- `RenderNode`

Rules:

- One root render node.
- One or more emitter chains.
- Deterministic evaluation order.
- Immutable node config snapshots for render threads.

## Phase 2: Stack UI mapped to graph

Before a freeform graph editor, expose a stack-like UI:

- Emitter stack
- Force stack
- Deform stack
- Render stack

This gives most of the workflow benefit while keeping AE-compatible controls.

## Phase 3: Serializable graph state

Store graph data in plugin-owned serialized state.

Requirements:

- Stable versioning.
- Backward-compatible upgrades.
- Safe read-only snapshots for MFR.
- Preset import/export compatibility.

## Phase 4: Custom graph UI

Only after the graph runtime and serialization are stable:

- Node canvas
- Links between nodes
- Node presets
- Grouping and bypass
- Preview quality switching per node chain

## Minimal first graph for v-next

`Emitter -> Force -> Render`

Concrete first nodes:

- `Emitter.Point`
- `Emitter.Box`
- `Emitter.Grid`
- `Force.Gravity`
- `Force.Wind`
- `Force.Turbulence`
- `Render.Sprite`
- `Render.Shape`

## Design constraints

- Rendering must remain deterministic for the same time and seed.
- Graph evaluation must be thread-safe for MFR.
- Heavy source-derived caches must be snapshotable or rebuildable without UI thread coupling.
- AE standard controls should remain usable for the common path, even after graph mode exists.
