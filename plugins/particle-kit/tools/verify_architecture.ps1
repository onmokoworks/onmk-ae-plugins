$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )
    Write-Host "==> $Name"
    & $Script
}

function Read-Text {
    param([string]$RelativePath)
    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath)
}

function Assert-FileExists {
    param([string]$RelativePath)
    $path = Join-Path $Root $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing expected file: $RelativePath"
    }
}

function Assert-FileMissing {
    param([string]$RelativePath)
    $path = Join-Path $Root $RelativePath
    if (Test-Path -LiteralPath $path) {
        throw "Unexpected file exists: $RelativePath"
    }
}

function Assert-Contains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Label
    )
    if (-not $Text.Contains($Needle)) {
        throw "$Label does not contain expected text: $Needle"
    }
}

function Assert-NotContains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Label
    )
    if ($Text.Contains($Needle)) {
        throw "$Label unexpectedly contains text: $Needle"
    }
}

Invoke-Step "Check ParticleKit identity remains stable" {
    $build = Read-Text "build.rs"
    Assert-Contains $build 'Property::Name("Particle Kit")' "build.rs"
    Assert-Contains $build 'Property::AE_Effect_Match_Name("ParticleKit")' "build.rs"
    Assert-NotContains $build "LatticeLab" "build.rs"
}

Invoke-Step "Check workspace product boundaries" {
    $cargo = Read-Text "Cargo.toml"
    Assert-Contains $cargo 'particlelab_engine_core = { path = "crates/particlelab_engine_core" }' "Cargo.toml"
    Assert-Contains $cargo 'members = ["crates/particlelab_engine_core", "crates/lattice_lab_plugin"]' "Cargo.toml"
    Assert-Contains $cargo 'default-members = [".", "crates/particlelab_engine_core"]' "Cargo.toml"
    Assert-NotContains $cargo 'default-members = [".", "crates/particlelab_engine_core", "crates/lattice_lab_plugin"]' "Cargo.toml"
}

Invoke-Step "Check engine-core is AE-free and owns Particle execution" {
    $coreCargo = Read-Text "crates\particlelab_engine_core\Cargo.toml"
    Assert-NotContains $coreCargo "after-effects" "particlelab_engine_core/Cargo.toml"
    Assert-NotContains $coreCargo "pipl" "particlelab_engine_core/Cargo.toml"

    foreach ($file in @(
        "crates\particlelab_engine_core\src\engine.rs",
        "crates\particlelab_engine_core\src\particle.rs",
        "crates\particlelab_engine_core\src\renderer.rs",
        "crates\particlelab_engine_core\src\render_core.rs",
        "crates\particlelab_engine_core\src\node_graph_core.rs"
    )) {
        Assert-FileExists $file
        $text = Read-Text $file
        Assert-NotContains $text "after_effects" $file
        Assert-NotContains $text "pipl::" $file
        Assert-NotContains $text "ae::" $file
    }

    $coreLib = Read-Text "crates\particlelab_engine_core\src\lib.rs"
    Assert-Contains $coreLib "pub mod engine;" "engine-core lib.rs"
    Assert-Contains $coreLib "pub mod particle;" "engine-core lib.rs"
    Assert-Contains $coreLib "pub mod renderer;" "engine-core lib.rs"
    Assert-Contains $coreLib "pub mod render_core;" "engine-core lib.rs"
    Assert-Contains $coreLib "pub mod node_graph_core;" "engine-core lib.rs"
    Assert-Contains $coreLib "pub mod prelude" "engine-core lib.rs"

    Assert-FileExists "crates\particlelab_engine_core\tests\public_api.rs"
    $publicApiTest = Read-Text "crates\particlelab_engine_core\tests\public_api.rs"
    Assert-Contains $publicApiTest "particlelab_engine_core::prelude" "engine-core public_api.rs"
    Assert-Contains $publicApiTest "downstream_particle_effect_can_render_without_ae_host_types" "engine-core public_api.rs"
    Assert-Contains $publicApiTest "downstream_particle_effect_can_compile_its_own_adapter_to_core" "engine-core public_api.rs"
    Assert-Contains $publicApiTest "ParticleRuntimeInputs::argb8_with_row_bytes" "engine-core public_api.rs"
    Assert-NotContains $publicApiTest "after_effects" "engine-core public_api.rs"
}

Invoke-Step "Check ParticleLab host crate uses thin core wrappers" {
    Assert-Contains (Read-Text "src\engine.rs") "particlelab_engine_core::engine::*" "src/engine.rs"
    Assert-Contains (Read-Text "src\particle.rs") "particlelab_engine_core::particle::*" "src/particle.rs"
    Assert-Contains (Read-Text "src\renderer.rs") "particlelab_engine_core::renderer::*" "src/renderer.rs"
    Assert-Contains (Read-Text "src\render_core.rs") "particlelab_engine_core::render_core::*" "src/render_core.rs"
    Assert-Contains (Read-Text "src\node_graph_core.rs") "particlelab_engine_core::node_graph_core::*" "src/node_graph_core.rs"
}

Invoke-Step "Check Lattice Lab is standalone" {
    Assert-FileMissing "src\lattice.rs"
    Assert-FileMissing "src\lattice_ae_adapter.rs"
    Assert-FileMissing "src\lattice_project_state.rs"
    Assert-FileExists "crates\lattice_lab_plugin\src\lattice.rs"
    Assert-FileExists "crates\lattice_lab_plugin\src\lattice_ae_adapter.rs"
    Assert-FileExists "crates\lattice_lab_plugin\src\lattice_project_state.rs"

    $particleLib = Read-Text "src\lib.rs"
    Assert-NotContains $particleLib "mod lattice;" "src/lib.rs"
    Assert-NotContains $particleLib "mod lattice_ae_adapter;" "src/lib.rs"
    Assert-NotContains $particleLib "mod lattice_project_state;" "src/lib.rs"
    Assert-NotContains $particleLib 'LatticeLab' "src/lib.rs"

    $latticeCargo = Read-Text "crates\lattice_lab_plugin\Cargo.toml"
    Assert-Contains $latticeCargo 'particlelab_engine_core = { path = "../particlelab_engine_core" }' "lattice_lab_plugin/Cargo.toml"

    $latticeLib = Read-Text "crates\lattice_lab_plugin\src\lib.rs"
    Assert-Contains $latticeLib "mod lattice;" "lattice_lab_plugin/src/lib.rs"
    Assert-Contains $latticeLib "mod lattice_ae_adapter;" "lattice_lab_plugin/src/lib.rs"
    Assert-Contains $latticeLib "mod lattice_project_state;" "lattice_lab_plugin/src/lib.rs"
    Assert-Contains $latticeLib "OpenNodeUiShell" "lattice_lab_plugin/src/lib.rs"
    Assert-NotContains $latticeLib '#[path = "../../../src/lattice.rs"]' "lattice_lab_plugin/src/lib.rs"
    Assert-NotContains $latticeLib '#[path = "../../../src/lattice_ae_adapter.rs"]' "lattice_lab_plugin/src/lib.rs"
    Assert-NotContains $latticeLib '#[path = "../../../src/lattice_project_state.rs"]' "lattice_lab_plugin/src/lib.rs"

    $latticeBuild = Read-Text "crates\lattice_lab_plugin\build.rs"
    Assert-Contains $latticeBuild 'Property::Name("Lattice Lab")' "lattice_lab_plugin/build.rs"
    Assert-Contains $latticeBuild 'Property::AE_Effect_Match_Name("LatticeLab")' "lattice_lab_plugin/build.rs"
    Assert-NotContains $latticeBuild "ParticleKit" "lattice_lab_plugin/build.rs"

    $latticeAdapter = Read-Text "crates\lattice_lab_plugin\src\lattice_ae_adapter.rs"
    Assert-Contains $latticeAdapter "NodeUiSidecarGroupStart" "lattice_lab_plugin/src/lattice_ae_adapter.rs"
    Assert-Contains $latticeAdapter "NodeUiSidecarGroupEnd" "lattice_lab_plugin/src/lattice_ae_adapter.rs"
}

Invoke-Step "Check Node UI sidecar startup path" {
    Assert-FileExists "tools\node-ui-shell\startup-payload.js"
    Assert-FileExists "tools\verify_node_ui_shell.ps1"
    $index = Read-Text "tools\node-ui-shell\index.html"
    Assert-Contains $index "startup-payload.js" "tools/node-ui-shell/index.html"

    $app = Read-Text "tools\node-ui-shell\app.js"
    Assert-Contains $app "PARTICLELAB_NODE_UI_BOOTSTRAP" "tools/node-ui-shell/app.js"
    Assert-Contains $app "PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE" "tools/node-ui-shell/app.js"

    $particleLib = Read-Text "src\lib.rs"
    Assert-Contains $particleLib "node_ui_shell_startup_payload_js" "src/lib.rs"
    Assert-Contains $particleLib "startup-payload.js" "src/lib.rs"

    $latticeLib = Read-Text "crates\lattice_lab_plugin\src\lib.rs"
    Assert-Contains $latticeLib "lattice_node_ui_shell_startup_payload_js" "lattice_lab_plugin/src/lib.rs"
    Assert-Contains $latticeLib "startup-payload.js" "lattice_lab_plugin/src/lib.rs"

    $nodeUiVerifier = Read-Text "tools\verify_node_ui_shell.ps1"
    Assert-Contains $nodeUiVerifier "node --check" "tools/verify_node_ui_shell.ps1"
    Assert-Contains $nodeUiVerifier "particlelab-bootstrap.json" "tools/verify_node_ui_shell.ps1"
    Assert-Contains $nodeUiVerifier "latticelab-bootstrap.json" "tools/verify_node_ui_shell.ps1"
}

Invoke-Step "Check deploy helpers preserve rollback path" {
    Assert-FileExists "tools\deploy_ae_plugins.ps1"
    $deployScript = Read-Text "tools\deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "Backup:" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "RestoreFrom" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "ParticleKit.aex" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "LatticeLab.aex" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "Assert-CopyMatches" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "Get-FileHash" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "Assert-SystemMediaCoreWriteAllowed" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployScript "Test-IsAdministrator" "tools/deploy_ae_plugins.ps1"

    $smokeScript = Read-Text "tools\run_ae_smoke.ps1"
    Assert-Contains $smokeScript "RestoreDeployBackupAfterSmoke" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "DeployBackupDir" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "deployed_plugin_evidence.json" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "Write-PluginDeploymentEvidence" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "hashesMatch" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "VerifyDeploymentOnly" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "verify_ae_smoke_report.ps1" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "PARTICLELAB_AE_SMOKE_RUN_ID" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "smokeRunId" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "Assert-SystemMediaCoreWriteAllowed" "tools/run_ae_smoke.ps1"
    Assert-Contains $smokeScript "Check deploy privilege" "tools/run_ae_smoke.ps1"

    Assert-FileExists "tools\verify_ae_smoke_report.ps1"
    Assert-FileExists "tools\inspect_ae_host_environment.ps1"
    Assert-FileExists "tools\run_final_ae_smoke_elevated.ps1"
    $smokeReportVerifier = Read-Text "tools\verify_ae_smoke_report.ps1"
    Assert-Contains $smokeReportVerifier "ParticleKit" "tools/verify_ae_smoke_report.ps1"
    Assert-Contains $smokeReportVerifier "LatticeLab" "tools/verify_ae_smoke_report.ps1"
    Assert-Contains $smokeReportVerifier "RequireCurrentRelease" "tools/verify_ae_smoke_report.ps1"
    Assert-Contains $smokeReportVerifier "hashesMatch" "tools/verify_ae_smoke_report.ps1"
    Assert-Contains $smokeReportVerifier "smokeRunId" "tools/verify_ae_smoke_report.ps1"

    $hostInspector = Read-Text "tools\inspect_ae_host_environment.ps1"
    Assert-Contains $hostInspector "SystemMediaCore" "tools/inspect_ae_host_environment.ps1"
    Assert-Contains $hostInspector "UserMediaCore" "tools/inspect_ae_host_environment.ps1"
    Assert-Contains $hostInspector "ParticleKit.aex" "tools/inspect_ae_host_environment.ps1"
    Assert-Contains $hostInspector "LatticeLab.aex" "tools/inspect_ae_host_environment.ps1"
    Assert-Contains $hostInspector "matchesRelease" "tools/inspect_ae_host_environment.ps1"

    $finalSmoke = Read-Text "tools\run_final_ae_smoke_elevated.ps1"
    Assert-Contains $finalSmoke "RunFile" "tools/run_final_ae_smoke_elevated.ps1"
    Assert-Contains $finalSmoke "SystemMediaCore" "tools/run_final_ae_smoke_elevated.ps1"
    Assert-Contains $finalSmoke "RestoreDeployBackupAfterSmoke" "tools/run_final_ae_smoke_elevated.ps1"
    Assert-Contains $finalSmoke "Verb RunAs" "tools/run_final_ae_smoke_elevated.ps1"

    $smokeJsx = Read-Text "tools\ae_smoke_test.jsx"
    Assert-Contains $smokeJsx "displayNameMatches" "tools/ae_smoke_test.jsx"
    Assert-Contains $smokeJsx "propertyCountPositive" "tools/ae_smoke_test.jsx"
    Assert-Contains $smokeJsx "PARTICLELAB_AE_SMOKE_RUN_ID" "tools/ae_smoke_test.jsx"

    $deployBat = Read-Text "deploy.bat"
    Assert-Contains $deployBat "tools\deploy_ae_plugins.ps1" "deploy.bat"
}

Invoke-Step "Check migration verifier documents the local and host gates" {
    Assert-FileExists "tools\verify_particlelab_migration.ps1"
    $migrationScript = Read-Text "tools\verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "verify_architecture.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "verify_param_abi_manifest.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "verify_node_ui_shell.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "cargo" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "verify_ae_release.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "run_ae_smoke.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "SystemMediaCore" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migrationScript "RestoreDeployBackupAfterSmoke" "tools/verify_particlelab_migration.ps1"
}

Invoke-Step "Check compatibility guards are present" {
    $lib = Read-Text "src\lib.rs"
    Assert-Contains $lib "PARAM_ENUM_ABI_ORDER" "src/lib.rs"
    Assert-Contains $lib "PARAM_SETUP_ABI_ORDER" "src/lib.rs"
    Assert-Contains $lib "legacy_plexus_slots_stay_registered_for_project_compatibility" "src/lib.rs"
    Assert-Contains $lib "published_host_float_slots_stay_before_node_graph_and_sidecar_tail_params" "src/lib.rs"
    Assert-FileExists "docs\PARTICLEKIT_ABI_MANIFEST.json"
    Assert-FileExists "docs\LATTICE_LAB_ABI_MANIFEST.json"
    Assert-FileExists "docs\COMPATIBILITY_EVIDENCE.md"
    Assert-FileExists "docs\GOAL_AUDIT.md"
    Assert-FileExists "tools\verify_param_abi_manifest.ps1"
    Assert-FileExists "tools\verify_compatibility_contract.ps1"
    Assert-FileExists "tools\verify_goal_audit.ps1"

    $compatibilityVerifier = Read-Text "tools\verify_compatibility_contract.ps1"
    Assert-Contains $compatibilityVerifier "COMPATIBILITY_EVIDENCE.md" "tools/verify_compatibility_contract.ps1"
    Assert-Contains $compatibilityVerifier "project_state_unflatten_reads_v1_without_published_value_overrides" "tools/verify_compatibility_contract.ps1"
    Assert-Contains $compatibilityVerifier "downstream_particle_effect_can_compile_its_own_adapter_to_core" "tools/verify_compatibility_contract.ps1"

    $migration = Read-Text "tools\verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_compatibility_contract.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_goal_audit.ps1" "tools/verify_particlelab_migration.ps1"

    $goalAudit = Read-Text "docs\GOAL_AUDIT.md"
    Assert-Contains $goalAudit "Remaining External Gate" "docs/GOAL_AUDIT.md"
    Assert-Contains $goalAudit "verify_particlelab_migration.ps1" "docs/GOAL_AUDIT.md"
    Assert-Contains $goalAudit "run_ae_smoke.ps1 -Build -Deploy -DeployTarget SystemMediaCore" "docs/GOAL_AUDIT.md"

    $projectState = Read-Text "src\project_state.rs"
    Assert-Contains $projectState "PROJECT_STATE_VERSION" "src/project_state.rs"
    Assert-Contains $projectState "NODE_UI_GRAPH_STATE_VERSION" "src/project_state.rs"
    Assert-Contains $projectState "UnsupportedNodeUiSnapshotVersion" "src/project_state.rs"
}

Write-Host "Architecture verification passed."
