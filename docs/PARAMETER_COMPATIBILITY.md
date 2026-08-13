# ParticleLab Parameter Compatibility

After Effects project files keep effect parameter streams. Treat the parameter
list as an ABI: once a build has shipped, parameter IDs, types, and setup order
are persistent data.

## Rules

- Never remove, rename, or reorder a shipped `Params` variant.
- Never remove or reorder the matching `params_setup` registration call.
- Add new host parameters only after every existing shipped parameter slot.
- If a feature is retired, keep its old parameters registered as hidden legacy
  slots and ignore them in render code.
- Keep the original parameter type for retired slots. A popup stays a popup, a
  slider stays a slider, a group start/end stays a group start/end.
- If a new parameter changes old-project behavior, set its normal `default` for
  newly applied effects, set its `value` to the old-project behavior, and add
  `ParamFlag::USE_VALUE_FOR_OLD_PROJECTS`.
- Presets are a separate JSON format. Bump `PRESET_VERSION` and migrate in
  `src/preset.rs::load_preset_snapshot` when omitted fields or changed defaults
  would alter existing looks.
- Node graph/project state is separate AE sequence data. Bump
  `PROJECT_STATE_VERSION` in `src/project_state.rs` when changing that payload;
  do not add host parameters just to store node graph internals.
- Current preset schema version 6 stores optional `graph_published_values`.
  Version 5 presets and missing-version presets load with an empty published
  override list.
- Current project sequence payload version 3 stores graph published value
  overrides plus `host_float_bindings` for AE keyframable fixed float slots.
  Version 1 graph payloads still load and default overrides/bindings to empty;
  version 2 payloads load with empty host bindings.
- Node UI graph edits should use `ParticleLabProjectState` commit/update methods
  instead of mutating sequence fields directly, so invalid documents or published
  values cannot corrupt the active graph state.
- External Node UI/panel interchange should use `NodeUiGraphStateSnapshot`
  version 2. This JSON shape is separate from both AE parameter streams and raw
  AE sequence data. Newer snapshot versions are rejected before commit so an
  older plugin cannot partially apply unknown graph UI state.
- Editor startup/export can use `NodeUiBootstrapPayload` version 1, which wraps
  the current shared node UI catalog plus the snapshot. Import must continue to
  accept plain snapshot JSON.
- Published host controls are append-only fixed slots. The project state stores
  stable published IDs in `host_float_bindings`; it must not store arbitrary
  graph node IDs or sockets in AE parameter streams. Binding changes may relabel
  or disable those shipped slots through `PF_UpdateParamUI`, but they must not
  rename, remove, or reorder the underlying `Params` variants.
- The append-only `Node Graph` tool buttons are host-adapter commands for
  external panel JSON exchange. They must stay at the tail of the AE parameter
  setup order unless newer controls are appended after them.
- Lattice Lab has its own host ABI in
  `crates/lattice_lab_plugin/src/lattice_ae_adapter.rs::LATTICE_LAB_PARAM_ABI_ORDER`
  and its own AE package in `crates/lattice_lab_plugin`. Changes there are also
  append-only, but they must not reuse, reorder, or migrate through ParticleLab
  `Params`.

## Code Guards

`src/lib.rs` locks two related surfaces with unit tests:

- `PARAM_ENUM_ABI_ORDER`: protects the enum discriminant order used to generate
  stable parameter IDs from `Params` names.
- `PARAM_SETUP_ABI_ORDER`: records the current AE registration order, including
  groups and hidden retired slots.

`docs/PARTICLEKIT_ABI_MANIFEST.json` is the checked-in ParticleKit ABI manifest.
It records the effect display/match name, `PARAM_ENUM_ABI_ORDER`,
`PARAM_SETUP_ABI_ORDER`, and retired legacy slots. Update it only by running:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_param_abi_manifest.ps1 -Update
```

Then run the verifier without `-Update` and review the manifest diff.

`crates/lattice_lab_plugin/src/lattice_ae_adapter.rs` separately locks the
Lattice Lab ABI manifest. That test protects the future standalone effect
without changing ParticleLab project compatibility.

`docs/LATTICE_LAB_ABI_MANIFEST.json` is the equivalent standalone Lattice Lab
host ABI manifest. It is checked by the same verifier but is intentionally
separate from ParticleKit.

`tools/verify_ae_release.ps1` is the release-level smoke check. After release
builds it checks the ParticleKit and LatticeLab DLLs for AE entry point markers,
expected identity strings, and distinct deploy names. This does not prove render
behavior inside AE, but it guards against accidentally shipping the wrong match
name or packaging the two effects under the same identity.

`tools/verify_architecture.ps1` is the source-level compatibility guard. It
checks the ParticleKit identity in `build.rs`, verifies engine-core stays
AE-free, confirms the Lattice modules are not compiled by the ParticleLab host
crate, and ensures the append-only ABI tests remain present.

`tools/verify_param_abi_manifest.ps1` compares the checked-in ABI manifest JSON
files with the current Rust source. A mismatch means host parameter order or
identity changed and must be reviewed deliberately.

`docs/COMPATIBILITY_EVIDENCE.md` maps the compatibility requirements to the
specific tests, manifests, and smoke artifacts that prove them.
`tools/verify_compatibility_contract.ps1` checks that this map still names real
tests and current manifests, and `tools/verify_particlelab_migration.ps1` runs
it before the full Rust test suite.

`tools/run_ae_smoke.ps1` is the AE-host check after deployment. It launches
After Effects from a clean session, adds `ParticleKit` and `LatticeLab` by match
name, and writes a JSON report under `target\ae-smoke`. Use this when validating
that the split still loads as two distinct effects in the host.

When adding a public host parameter, append the new `Params` variant at the end
of the enum ABI order, register it deliberately in `params_setup`, then update
the ABI test list in the same commit. Treat a failing ABI test as a review gate,
not as a formatting chore.

## Current Retired Slots

The former point-network prototype controls are registered by
`add_legacy_plexus_params()` and hidden in `update_shape_dependent_ui()`. The
function name intentionally preserves the old internal label so its purpose is
obvious when comparing old project data and diffs. The standalone product should
not ship under that name; see `ARCHITECTURE.md`.

These slots preserve old project stream positions while allowing the particle
renderer to ignore the retired data.

Future retired features should follow the same pattern: leave the host ABI in
place, hide the UI, and remove only the render-time dependency on those values.
