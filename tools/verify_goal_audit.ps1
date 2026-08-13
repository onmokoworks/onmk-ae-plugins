$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$AuditOutDir = Join-Path $Root "target\migration-audit"
$AuditJsonPath = Join-Path $AuditOutDir "goal_audit.json"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    Write-Host "==> $Name"
    & $Script
}

function Assert-FileExists {
    param([string]$RelativePath)

    $path = Join-Path $Root $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing expected file: $RelativePath"
    }
}

function Read-Text {
    param([string]$RelativePath)
    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath)
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

function Add-Requirement {
    param(
        [System.Collections.ArrayList]$Items,
        [string]$Id,
        [string]$Status,
        [string[]]$Evidence
    )

    [void]$Items.Add([ordered]@{
        id = $Id
        status = $Status
        evidence = $Evidence
    })
}

Invoke-Step "Check goal audit document" {
    Assert-FileExists "docs\GOAL_AUDIT.md"
    $doc = Read-Text "docs\GOAL_AUDIT.md"
    foreach ($needle in @(
        "ParticleLab Migration Goal Audit",
        "Existing AE Project Compatibility",
        "Engine-Core Migration",
        "Node UI Path",
        "Downstream Particle Effects",
        "Lattice Lab",
        "Remaining External Gate",
        "elevated PowerShell",
        "run_ae_smoke.ps1 -Build -Deploy -DeployTarget SystemMediaCore"
    )) {
        Assert-Contains $doc $needle "docs/GOAL_AUDIT.md"
    }
}

Invoke-Step "Check referenced evidence exists" {
    foreach ($path in @(
        "docs\COMPATIBILITY_EVIDENCE.md",
        "docs\PARTICLEKIT_ABI_MANIFEST.json",
        "docs\LATTICE_LAB_ABI_MANIFEST.json",
        "crates\particlelab_engine_core\src\engine.rs",
        "crates\particlelab_engine_core\src\particle.rs",
        "crates\particlelab_engine_core\src\renderer.rs",
        "crates\particlelab_engine_core\src\render_core.rs",
        "crates\particlelab_engine_core\src\node_graph_core.rs",
        "crates\particlelab_engine_core\tests\public_api.rs",
        "src\classic_params.rs",
        "src\graph.rs",
        "src\preset.rs",
        "src\project_state.rs",
        "tools\node-ui-shell\index.html",
        "tools\verify_node_ui_shell.ps1",
        "crates\lattice_lab_plugin\src\lattice.rs",
        "crates\lattice_lab_plugin\src\lattice_ae_adapter.rs",
        "crates\lattice_lab_plugin\src\lattice_project_state.rs",
        "tools\verify_particlelab_migration.ps1",
        "tools\run_ae_smoke.ps1",
        "tools\run_final_ae_smoke_elevated.ps1",
        "tools\verify_ae_smoke_report.ps1",
        "tools\inspect_ae_host_environment.ps1"
    )) {
        Assert-FileExists $path
    }
}

Invoke-Step "Check goal audit is wired into migration verification" {
    $migration = Read-Text "tools\verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_goal_audit.ps1" "tools/verify_particlelab_migration.ps1"

    $architecture = Read-Text "tools\verify_architecture.ps1"
    Assert-Contains $architecture "GOAL_AUDIT.md" "tools/verify_architecture.ps1"
    Assert-Contains $architecture "verify_goal_audit.ps1" "tools/verify_architecture.ps1"
}

Invoke-Step "Write machine-readable goal audit summary" {
    New-Item -ItemType Directory -Force -Path $AuditOutDir | Out-Null
    $items = [System.Collections.ArrayList]::new()
    Add-Requirement $items "existing_ae_project_compatibility" "local_evidence_present" @(
        "docs/COMPATIBILITY_EVIDENCE.md",
        "docs/PARTICLEKIT_ABI_MANIFEST.json",
        "tools/verify_param_abi_manifest.ps1",
        "tools/verify_compatibility_contract.ps1",
        "cargo test"
    )
    Add-Requirement $items "engine_core_migration" "local_evidence_present" @(
        "crates/particlelab_engine_core/src/engine.rs",
        "crates/particlelab_engine_core/src/particle.rs",
        "crates/particlelab_engine_core/src/renderer.rs",
        "crates/particlelab_engine_core/tests/public_api.rs",
        "tools/verify_architecture.ps1"
    )
    Add-Requirement $items "node_ui_path" "local_evidence_present" @(
        "src/graph.rs",
        "src/project_state.rs",
        "tools/node-ui-shell",
        "tools/verify_node_ui_shell.ps1"
    )
    Add-Requirement $items "downstream_particle_effects" "local_evidence_present" @(
        "particlelab_engine_core::prelude",
        "downstream_particle_effect_can_compile_its_own_adapter_to_core"
    )
    Add-Requirement $items "lattice_lab_separation" "local_evidence_present" @(
        "crates/lattice_lab_plugin",
        "docs/LATTICE_LAB_ABI_MANIFEST.json",
        "cargo test -p lattice_lab_plugin",
        "tools/verify_ae_release.ps1"
    )
    Add-Requirement $items "ae_host_smoke" "pending_external_gate" @(
        "tools/run_ae_smoke.ps1 -Build -Deploy -DeployTarget SystemMediaCore -ScriptLaunchMode RunFile -RestoreDeployBackupAfterSmoke",
        "tools/run_final_ae_smoke_elevated.ps1",
        "tools/verify_ae_smoke_report.ps1",
        "tools/inspect_ae_host_environment.ps1",
        "target/ae-smoke/ae_smoke_report.json",
        "target/ae-smoke/deployed_plugin_evidence.json"
    )

    $summary = [ordered]@{
        generatedAt = (Get-Date).ToString("o")
        objective = "ParticleLab engine-core migration with existing AE project compatibility, Node UI, downstream particle effects, and Lattice Lab."
        localStatus = "non_elevated_evidence_present"
        remainingExternalGate = "SystemMediaCore AE host smoke"
        requirements = $items
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $AuditJsonPath -Encoding UTF8
    Write-Host "Goal audit: $AuditJsonPath"
}

Write-Host "Goal audit verification passed."
