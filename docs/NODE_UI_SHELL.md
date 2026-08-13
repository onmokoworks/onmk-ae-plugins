# Node UI Shell

`tools/node-ui-shell` is a static development shell for the future ParticleLab
and Lattice Lab node editor. It is not an AE panel yet. It exercises the same
versioned JSON boundary that the current AE `Node Graph` tool group exports and
imports.

Open:

```text
tools/node-ui-shell/index.html
```

Inside AE, the plugin also exposes a `Node UI Sidecar` group with `Open Shell`.
That command writes the embedded shell assets to:

```text
Documents/Particle Kit/node-ui-shell
```

It also writes `startup-payload.js` from the current effect's
`NodeUiBootstrapPayload`, then opens `index.html` with the OS shell. This is
still a sidecar workflow, not an embedded AEGP panel, but it gives AE users the
same tested JSON editor shape without depending on the repository checkout or a
manual first export/import step.

The standalone Lattice Lab effect exposes the same appended `Node UI Sidecar`
launch group. It writes its shell assets to:

```text
Documents/Lattice Lab/node-ui-shell
```

and its generated startup payload uses the Lattice bootstrap/state schema.

The shell can:

- load `NodeUiBootstrapPayload` JSON exported from ParticleLab
- load `LatticeNodeUiBootstrapPayload` JSON exported from Lattice Lab
- load older plain `NodeUiGraphStateSnapshot` JSON
- load checked ParticleLab and Lattice Lab demo payloads from the toolbar
- show the catalog node palette, graph nodes, edges, published controls, and
  host-float bindings
- edit existing node labels and value fields in the loaded graph document
- save either bootstrap JSON or plain snapshot JSON for AE import when no
  blocking diagnostics are present

Fixtures:

- `fixtures/particlelab-bootstrap.json`
- `fixtures/latticelab-bootstrap.json`

These fixtures are part of the Rust test surface. ParticleLab and Lattice
project-state tests parse and commit them, so the static shell samples stay
aligned with the real import boundary instead of becoming display-only JSON.

Verification:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_node_ui_shell.ps1
```

The verifier checks that `index.html` references the shell assets, every
`document.getElementById()` target in `app.js` exists in the HTML, `app.js`
passes `node --check`, the default startup-payload bridge has the expected
globals, and both checked fixtures have valid catalog/document/edge/published
binding structure. The top-level migration verifier runs this check before Rust
tests so shell drift is caught without opening AE.

Compatibility rules:

- The shell edits only JSON payloads; it does not mutate AE params directly.
- Bootstrap JSON contains catalog plus state for editor startup.
- Snapshot JSON remains the portable state payload and is still accepted by
  ParticleLab import.
- Product graph documents remain separate: ParticleLab uses `particle.*` nodes,
  Lattice Lab uses `lattice.*` nodes.
- ParticleLab exports/imports through its `Node Graph` tool group. The standalone
  Lattice Lab plugin exposes the same Export/Import/Seed/Disable bridge for its
  own `lattice.*` payloads. Both products can open the same static sidecar shell
  with a generated startup payload for the active effect instance.
- AE parameter ABI remains append-only. Adding UI shell behavior does not add,
  remove, rename, or reorder host params.
- Diagnostics check duplicate nodes, missing value sockets, broken edge sockets,
  published value mismatches, and invalid host-float bindings before saving.
- The AE launch button is appended after the existing `Node Graph` controls in
  a new group so older project parameter streams remain stable.

Current limitation:

- The shell is a development UI, not a compiled AEGP panel or embedded AE UI.
  The next integration step is to connect this shell shape to an AE-hosted panel
  or a sidecar panel workflow.
