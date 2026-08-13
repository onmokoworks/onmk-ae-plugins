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

function Read-Json {
    param([string]$RelativePath)
    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath) | ConvertFrom-Json
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

function Assert-Equals {
    param(
        $Actual,
        $Expected,
        [string]$Label
    )

    if ($Actual -ne $Expected) {
        throw "$Label expected '$Expected', got '$Actual'."
    }
}

function Assert-ArrayContains {
    param(
        $Array,
        [string]$Expected,
        [string]$Label
    )

    if (-not (@($Array) -contains $Expected)) {
        throw "$Label does not contain $Expected."
    }
}

function Assert-ArrayOrder {
    param(
        $Array,
        [string[]]$ExpectedOrder,
        [string]$Label
    )

    $items = @($Array)
    $lastIndex = -1
    foreach ($expected in $ExpectedOrder) {
        $index = [array]::IndexOf($items, $expected)
        if ($index -lt 0) {
            throw "$Label does not contain $expected."
        }
        if ($index -le $lastIndex) {
            throw "$Label has $expected out of order."
        }
        $lastIndex = $index
    }
}

function Assert-TestPresent {
    param(
        [string]$RelativePath,
        [string]$TestName,
        [string]$EvidenceDoc
    )

    $text = Read-Text $RelativePath
    Assert-Contains $text "fn $TestName" $RelativePath
    Assert-Contains $EvidenceDoc $TestName "docs/COMPATIBILITY_EVIDENCE.md"
}

Invoke-Step "Check compatibility evidence document" {
    Assert-FileExists "docs\COMPATIBILITY_EVIDENCE.md"
    $doc = Read-Text "docs\COMPATIBILITY_EVIDENCE.md"
    Assert-Contains $doc "Existing AE Project Compatibility" "docs/COMPATIBILITY_EVIDENCE.md"
    Assert-Contains $doc "Node UI Compatibility" "docs/COMPATIBILITY_EVIDENCE.md"
    Assert-Contains $doc "Lattice Lab Separation" "docs/COMPATIBILITY_EVIDENCE.md"
    Assert-Contains $doc "AE Host Smoke Gate" "docs/COMPATIBILITY_EVIDENCE.md"
}

Invoke-Step "Check ABI manifest identities and append-only tail" {
    $particle = Read-Json "docs\PARTICLEKIT_ABI_MANIFEST.json"
    Assert-Equals $particle.effect.display_name "Particle Kit" "ParticleKit display name"
    Assert-Equals $particle.effect.match_name "ParticleKit" "ParticleKit match name"
    Assert-ArrayContains $particle.retired_legacy_slots.params "PlexusGroupStart" "ParticleKit retired legacy slots"
    Assert-ArrayContains $particle.retired_legacy_slots.params "PlexusGroupEnd" "ParticleKit retired legacy slots"
    Assert-ArrayOrder $particle.setup_abi_order @(
        "PublishedFloat1",
        "PublishedFloat2",
        "PublishedFloat3",
        "PublishedFloat4",
        "NodeGraphGroupStart",
        "ExportNodeGraphState",
        "ImportNodeGraphState",
        "SeedNodeGraphFromParams",
        "DisableNodeGraph",
        "NodeGraphGroupEnd",
        "NodeUiSidecarGroupStart",
        "OpenNodeUiShell",
        "NodeUiSidecarGroupEnd"
    ) "ParticleKit setup ABI order"

    $lattice = Read-Json "docs\LATTICE_LAB_ABI_MANIFEST.json"
    Assert-Equals $lattice.effect.display_name "Lattice Lab" "Lattice Lab display name"
    Assert-Equals $lattice.effect.match_name "LatticeLab" "Lattice Lab match name"
    Assert-ArrayContains $lattice.param_abi_order "OpenNodeUiShell" "Lattice Lab ABI order"
}

Invoke-Step "Check compatibility test coverage names" {
    $doc = Read-Text "docs\COMPATIBILITY_EVIDENCE.md"
    foreach ($entry in @(
        @{ Path = "src\lib.rs"; Name = "params_enum_order_is_append_only_abi" },
        @{ Path = "src\lib.rs"; Name = "params_setup_order_covers_each_abi_slot_once" },
        @{ Path = "src\lib.rs"; Name = "legacy_plexus_slots_stay_registered_for_project_compatibility" },
        @{ Path = "src\lib.rs"; Name = "published_host_float_slots_stay_before_node_graph_and_sidecar_tail_params" },
        @{ Path = "src\lib.rs"; Name = "load_preset_snapshot_migrates_apply_mode_from_composite_flag" },
        @{ Path = "src\graph.rs"; Name = "missing_published_params_defaults_to_empty_for_old_graph_json" },
        @{ Path = "src\graph.rs"; Name = "graph_document_from_value_migrates_old_graph_json_shape" },
        @{ Path = "src\preset.rs"; Name = "missing_graph_published_values_defaults_to_empty_for_old_presets" },
        @{ Path = "src\preset.rs"; Name = "load_preset_snapshot_migrates_embedded_graph_document" },
        @{ Path = "src\project_state.rs"; Name = "project_state_unflatten_reads_v1_without_published_value_overrides" },
        @{ Path = "src\project_state.rs"; Name = "project_state_unflatten_reads_v2_without_host_float_bindings" },
        @{ Path = "src\project_state.rs"; Name = "unknown_project_state_version_falls_back_to_classic_params" },
        @{ Path = "src\project_state.rs"; Name = "node_ui_graph_state_snapshot_rejects_future_versions_without_mutating_state" },
        @{ Path = "src\project_state.rs"; Name = "node_ui_import_still_accepts_plain_snapshot_json" },
        @{ Path = "crates\particlelab_engine_core\tests\public_api.rs"; Name = "downstream_particle_effect_can_compile_its_own_adapter_to_core" },
        @{ Path = "crates\lattice_lab_plugin\src\lattice_ae_adapter.rs"; Name = "lattice_lab_param_manifest_is_append_only_ordered" }
    )) {
        Assert-TestPresent $entry.Path $entry.Name $doc
    }
}

Invoke-Step "Check compatibility verifiers are part of migration gate" {
    $migration = Read-Text "tools\verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_compatibility_contract.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_param_abi_manifest.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migration "verify_node_ui_shell.ps1" "tools/verify_particlelab_migration.ps1"
    Assert-Contains $migration '"test"' "tools/verify_particlelab_migration.ps1"
}

Write-Host "Compatibility contract verification passed."
