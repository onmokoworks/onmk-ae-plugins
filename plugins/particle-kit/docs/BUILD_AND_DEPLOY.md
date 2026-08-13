# Build And Deploy

ParticleLab and Lattice Lab are separate AE effect binaries.

## Build

Build the existing ParticleKit effect:

```text
cargo build --release
```

Build the standalone Lattice Lab effect:

```text
cargo build --release -p lattice_lab_plugin
```

The release outputs are:

```text
target/release/particlekit.dll
target/release/lattice_lab_plugin.dll
```

## AE Plugin Names

Copy or deploy them as:

```text
ParticleKit.aex
LatticeLab.aex
```

`ParticleKit.aex` keeps the existing `ParticleKit` match name for old AE
projects. `LatticeLab.aex` registers the separate `LatticeLab` match name.

## Local Deploy Helper

`deploy.bat` calls `tools\deploy_ae_plugins.ps1`, which copies both release DLLs
into:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore
```

Run it from an elevated shell if Windows blocks writes to Program Files. The
script checks for Administrator rights before writing to `SystemMediaCore`. It
does not build automatically; run the release builds first. Existing
`ParticleKit.aex` and `LatticeLab.aex` files are backed up under
`target\deploy-backups` before they are overwritten.

The same deploy helper can be run directly:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\deploy_ae_plugins.ps1 -DeployTarget SystemMediaCore
```

After copying, the helper compares SHA256 hashes for each source DLL and target
`.aex`. A deploy command fails if the copied `ParticleKit.aex` or
`LatticeLab.aex` does not match the current release DLL.

It prints a matching restore command, for example:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\deploy_ae_plugins.ps1 -DeployTarget SystemMediaCore -RestoreFrom target\deploy-backups\YYYYMMDD_HHMMSS-system
```

## Verification

Before deploying a build, run the non-elevated migration verifier:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_particlelab_migration.ps1
```

To inspect the local AE host/deploy state without copying files or launching AE:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\inspect_ae_host_environment.ps1
```

This writes `target\migration-audit\ae_host_environment.json` with the detected
AfterFX installs, current shell elevation, and SHA256 comparison for
`ParticleKit.aex` and `LatticeLab.aex` in both SystemMediaCore and
UserMediaCore.

For a faster source-level pass that skips release packaging:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_particlelab_migration.ps1 -SourceOnly
```

The verifier runs the local gates that should stay green during ParticleLab
engine-core migration:

```text
cargo fmt --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_architecture.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_param_abi_manifest.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_compatibility_contract.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_goal_audit.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_node_ui_shell.ps1
cargo test -p particlelab_engine_core
cargo test
cargo test -p lattice_lab_plugin
cargo build --release
cargo build --release -p lattice_lab_plugin
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_ae_release.ps1
```

The architecture verifier checks the source-level split: ParticleKit keeps the
existing identity, engine-core stays AE-free, Particle execution lives in
engine-core, and Lattice Lab remains a standalone crate with its own ABI. The
ABI manifest verifier checks the checked-in ParticleKit and Lattice Lab host
parameter manifests against the current Rust source. The release verifier checks
that both DLLs exist, expose the AE entry point symbols, contain the expected
ParticleKit/LatticeLab identity markers, and map to the deploy `.aex` names.
These are not replacements for an AE host render test. They are the repeatable
non-elevated gate for boundary, identity, parameter-order, source API, and
Node UI shell, packaging regressions before deployment.

To build and verify in one command:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_ae_release.ps1 -Build
```

## AE Host Smoke Test

After the plugins are deployed, run the AE host smoke test from a clean/empty AE
session:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1
```

The script launches the newest installed `AfterFX.exe`, creates a temporary
empty project, adds `ParticleKit` and `LatticeLab` by match name, records their
reported names/property counts, writes `target\ae-smoke\ae_smoke_report.json`,
then closes the temporary project without saving.

When release DLLs exist, the smoke runner also writes
`target\ae-smoke\deployed_plugin_evidence.json` with SHA256 hashes for the
release DLLs and deployed `.aex` files in the selected MediaCore folder. When
`-Deploy` is passed, those hashes must match before AE is launched. This keeps a
passing host smoke from accidentally proving an older installed `ParticleKit`
instead of the current release build.

After AE writes `target\ae-smoke\ae_smoke_report.json`, the runner calls
`tools\verify_ae_smoke_report.ps1`. That verifier requires both `ParticleKit`
and `LatticeLab` to load by match name, report their expected display names, and
have positive property counts. With `-Deploy`, it also requires
`deployed_plugin_evidence.json` to prove the deployed `.aex` files match the
current release DLL hashes. The report and evidence also share a `smokeRunId`,
so a verifier cannot accidentally combine an old hash-evidence file with a
newer AE report.

To test deploy/copy/hash/restore behavior without launching AE, use:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Deploy -DeployTarget UserMediaCore -VerifyDeploymentOnly -RestoreDeployBackupAfterSmoke
```

To rebuild, deploy, and run the same host check:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Build -Deploy
```

`-Deploy` copies `ParticleKit.aex` and `LatticeLab.aex` into MediaCore and may
require an elevated shell. When `-DeployTarget SystemMediaCore` is used, the
runner checks for Administrator rights before copying either plugin. It
overwrites those two plugin files only.

For a non-elevated smoke test, deploy to the user MediaCore folder instead:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Deploy -DeployTarget UserMediaCore
```

This writes to:

```text
%APPDATA%\Adobe\Common\Plug-ins\7.0\MediaCore
```

The smoke runner refuses to start if an existing `AfterFX.exe` process is
running, unless `-AllowRunningAe` is passed deliberately.

If command-line script launch does not run on a local AE install, use the
startup-script fallback:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Deploy -DeployTarget UserMediaCore -ScriptLaunchMode StartupFolder
```

`StartupFolder` temporarily writes the smoke wrapper into the matching AE user
`Scripts\Startup` folder, removes it after the report/timeout, and closes the
AE process that it started when the check fails.

For a reversible elevated smoke test, add `-RestoreDeployBackupAfterSmoke`:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_ae_smoke.ps1 -Build -Deploy -DeployTarget SystemMediaCore -ScriptLaunchMode RunFile -RestoreDeployBackupAfterSmoke
```

This is the remaining external gate after
`tools\verify_particlelab_migration.ps1` passes. It must use `SystemMediaCore`
on AE installs that do not scan the user MediaCore folder for native `.aex`
effects, and may require an elevated shell.

The convenience wrapper below requests UAC when needed and runs the same final
gate with `RunFile`:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_final_ae_smoke_elevated.ps1
```

Some AE installs do not scan the user MediaCore folder for native `.aex`
effects. If the smoke report can add `ParticleKit` but fails to add
`LatticeLab`, deploy to `SystemMediaCore` from an elevated shell and rerun the
same smoke command.
