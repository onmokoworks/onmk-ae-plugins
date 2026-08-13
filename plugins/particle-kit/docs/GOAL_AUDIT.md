# ParticleLab Migration Goal Audit

Objective:

```text
Migrate ParticleLab to an engine-core design while preserving existing AE
project compatibility and enabling Node UI, downstream particle-family effects,
and Lattice Lab.
```

This file is the requirement map for deciding whether the migration is actually
done. It does not replace tests; it points to the evidence that should prove
each part of the objective.

## Existing AE Project Compatibility

Local evidence:

- `docs/COMPATIBILITY_EVIDENCE.md`
- `docs/PARTICLEKIT_ABI_MANIFEST.json`
- `tools/verify_param_abi_manifest.ps1`
- `tools/verify_compatibility_contract.ps1`
- `params_enum_order_is_append_only_abi`
- `params_setup_order_covers_each_abi_slot_once`
- `legacy_plexus_slots_stay_registered_for_project_compatibility`
- `project_state_unflatten_reads_v1_without_published_value_overrides`
- `project_state_unflatten_reads_v2_without_host_float_bindings`
- `unknown_project_state_version_falls_back_to_classic_params`

Meaning: the shipped `ParticleKit` match name, host parameter stream, retired
legacy slots, preset migration, graph migration, and sequence-data migration are
guarded before render code sees current engine config.

## Engine-Core Migration

Local evidence:

- `crates/particlelab_engine_core/src/engine.rs`
- `crates/particlelab_engine_core/src/particle.rs`
- `crates/particlelab_engine_core/src/renderer.rs`
- `crates/particlelab_engine_core/src/render_core.rs`
- `crates/particlelab_engine_core/src/node_graph_core.rs`
- `src/classic_params.rs`
- thin host wrappers in `src/engine.rs`, `src/particle.rs`, `src/renderer.rs`,
  `src/render_core.rs`, and `src/node_graph_core.rs`
- `tools/verify_architecture.ps1`

Meaning: AE-free particle execution now lives in engine-core, while the host
crate remains the compatibility adapter and shipped AE ABI owner.

## Node UI Path

Local evidence:

- `src/graph.rs`
- `src/project_state.rs`
- `NodeUiGraphStateSnapshot` version 2
- `NodeUiBootstrapPayload` version 1
- appended `Node Graph` and `Node UI Sidecar` AE controls
- `tools/node-ui-shell`
- `tools/verify_node_ui_shell.ps1`

Meaning: Node UI data is stored as versioned graph/project state, not as dynamic
AE parameters. External UI exchange can use bootstrap or snapshot JSON, with
atomic commit behavior and compatibility migrations.

## Downstream Particle Effects

Local evidence:

- `particlelab_engine_core::prelude`
- `crates/particlelab_engine_core/tests/public_api.rs`
- `downstream_particle_effect_can_compile_its_own_adapter_to_core`

Meaning: another particle-family effect can compile product-specific input into
`ParticleEngineConfig` and render through the shared engine without importing AE
or the ParticleLab host crate.

## Lattice Lab

Local evidence:

- `crates/lattice_lab_plugin`
- `crates/lattice_lab_plugin/src/lattice.rs`
- `crates/lattice_lab_plugin/src/lattice_ae_adapter.rs`
- `crates/lattice_lab_plugin/src/lattice_project_state.rs`
- `docs/LATTICE_LAB_ABI_MANIFEST.json`
- `lattice_lab_param_manifest_is_append_only_ordered`
- `tools/verify_ae_release.ps1`

Meaning: Lattice Lab is a separate AE effect package with `LatticeLab` identity,
its own ABI, its own project state, its own graph schema, and shared use of only
product-neutral engine-core utilities.

## Local Verification Gate

Run:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_particlelab_migration.ps1
```

This is the repeatable non-elevated gate. It includes formatting, architecture,
ABI manifests, compatibility contract, Node UI shell, Rust tests, release
builds, and release binary identity checks.

## Remaining External Gate

Run the final elevated helper when ready, or invoke it from an elevated
PowerShell directly. This is the elevated PowerShell gate:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_final_ae_smoke_elevated.ps1
```

Completion is not proven until this SystemMediaCore host smoke passes for the
current `ParticleKit.aex` and `LatticeLab.aex`. The smoke runner writes
`target/ae-smoke/ae_smoke_report.json` and
`target/ae-smoke/deployed_plugin_evidence.json`, and
`tools/verify_ae_smoke_report.ps1` checks both. The deploy path checks for
Administrator rights before writing to `SystemMediaCore`, so permission failures
should happen before any plugin file is copied.

The helper defaults to `RunFile` script launch because that path has been
observed to execute the smoke JSX in AE 26.2 on this workstation. It relaunches
through Windows UAC if the current shell is not elevated, then delegates to:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Build -Deploy -DeployTarget SystemMediaCore -ScriptLaunchMode RunFile -RestoreDeployBackupAfterSmoke
```

For a quick preflight snapshot of the local AE install, current shell elevation,
and the SystemMediaCore/UserMediaCore plugin hashes, run:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\inspect_ae_host_environment.ps1
```

This writes `target/migration-audit/ae_host_environment.json`. It is diagnostic
evidence only; it does not replace the SystemMediaCore host smoke.
