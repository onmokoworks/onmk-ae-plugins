# Compatibility Evidence

This document maps the migration goal to concrete guards. It is intentionally
boring: when ParticleLab grows, the compatibility evidence should grow before
the feature feels shippable.

## Existing AE Project Compatibility

- `docs/PARTICLEKIT_ABI_MANIFEST.json` keeps the `Particle Kit` display name,
  `ParticleKit` match name, enum ABI order, setup ABI order, and retired legacy
  slots.
- `tools/verify_param_abi_manifest.ps1` compares the checked-in manifest against
  current Rust source.
- `params_enum_order_is_append_only_abi`
- `params_setup_order_covers_each_abi_slot_once`
- `legacy_plexus_slots_stay_registered_for_project_compatibility`
- `published_host_float_slots_stay_before_node_graph_and_sidecar_tail_params`

These guards prove the old host parameter stream is still registered and that
new Node Graph / Node UI Sidecar controls live after the shipped parameter
surface.

## Preset And Graph Compatibility

- `load_preset_snapshot_migrates_apply_mode_from_composite_flag`
- `missing_graph_published_values_defaults_to_empty_for_old_presets`
- `load_preset_snapshot_migrates_embedded_graph_document`
- `missing_published_params_defaults_to_empty_for_old_graph_json`
- `graph_document_from_value_migrates_old_graph_json_shape`

These tests keep old preset JSON and early graph JSON loadable before they reach
the current render path.

## Node UI Compatibility

- `project_state_unflatten_reads_v1_without_published_value_overrides`
- `project_state_unflatten_reads_v2_without_host_float_bindings`
- `unknown_project_state_version_falls_back_to_classic_params`
- `node_ui_graph_state_snapshot_rejects_future_versions_without_mutating_state`
- `node_ui_import_still_accepts_plain_snapshot_json`
- `tools/verify_node_ui_shell.ps1`

These guards prove the AE sequence-data payload remains migratable, newer Node
UI snapshots fail atomically, old plain snapshot JSON is still accepted, and the
static sidecar assets still match the JSON boundary.

## Engine-Core Expansion

- `downstream_particle_effect_can_compile_its_own_adapter_to_core`
- `crates/particlelab_engine_core/tests/public_api.rs`

These tests prove a future particle-family effect can depend on
`particlelab_engine_core::prelude`, compile a product-specific adapter into
`ParticleEngineConfig`, and render without importing AE host types.

## Lattice Lab Separation

- `docs/LATTICE_LAB_ABI_MANIFEST.json` keeps the standalone `Lattice Lab`
  display name, `LatticeLab` match name, and its separate host ABI order.
- `lattice_lab_param_manifest_is_append_only_ordered`
- `tools/verify_ae_release.ps1`

These guards prove Lattice Lab is a separate product identity and does not share
ParticleKit's parameter ABI.

## AE Host Smoke Gate

- `tools/run_ae_smoke.ps1`
- `tools/verify_ae_smoke_report.ps1`
- `target/ae-smoke/ae_smoke_report.json`
- `target/ae-smoke/deployed_plugin_evidence.json`

This is the remaining external gate. It must be run from an elevated shell when
deploying to `SystemMediaCore`. The smoke report and deployment evidence share a
`smokeRunId`, and `-Deploy` requires deployed `.aex` hashes to match the
current release DLLs before AE starts.
