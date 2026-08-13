param(
    [switch]$Update
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

function Read-Text {
    param([string]$RelativePath)
    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath)
}

function Extract-ParamsList {
    param(
        [string]$Text,
        [string]$ConstName,
        [string]$ParamType
    )
    $pattern = "const\s+$ConstName\s*:\s*&\[$ParamType\]\s*=\s*(?:&?params!\[|&\[)(.*?)\];"
    $match = [regex]::Match($Text, $pattern, [System.Text.RegularExpressions.RegexOptions]::Singleline)
    if (-not $match.Success) {
        throw "Could not find params list: $ConstName"
    }
    Extract-NamesFromListBody $match.Groups[1].Value
}

function Extract-NamesFromListBody {
    param([string]$Body)
    $names = New-Object System.Collections.Generic.List[string]
    foreach ($line in ($Body -split "`r?`n")) {
        $clean = ($line -replace "//.*$", "").Trim()
        if ([string]::IsNullOrWhiteSpace($clean)) {
            continue
        }
        $clean = $clean.TrimEnd(",").Trim()
        $clean = $clean -replace "^Params::", ""
        if ($clean -match "^[A-Za-z_][A-Za-z0-9_]*$") {
            $names.Add($clean)
        }
    }
    $names.ToArray()
}

function Extract-LatticeParamsList {
    param([string]$Text)
    $pattern = "pub\(crate\)\s+const\s+LATTICE_LAB_PARAM_ABI_ORDER\s*:\s*&\[LatticeLabParam\]\s*=\s*&\[(.*?)\];"
    $match = [regex]::Match($Text, $pattern, [System.Text.RegularExpressions.RegexOptions]::Singleline)
    if (-not $match.Success) {
        throw "Could not find LATTICE_LAB_PARAM_ABI_ORDER"
    }

    $names = New-Object System.Collections.Generic.List[string]
    foreach ($line in ($match.Groups[1].Value -split "`r?`n")) {
        $clean = ($line -replace "//.*$", "").Trim()
        if ([string]::IsNullOrWhiteSpace($clean)) {
            continue
        }
        $clean = $clean.TrimEnd(",").Trim()
        $clean = $clean -replace "^LatticeLabParam::", ""
        if ($clean -match "^[A-Za-z_][A-Za-z0-9_]*$") {
            $names.Add($clean)
        }
    }
    $names.ToArray()
}

function Extract-EffectIdentity {
    param(
        [string]$BuildText,
        [string]$Label
    )
    $name = [regex]::Match($BuildText, 'Property::Name\("([^"]+)"\)')
    $matchName = [regex]::Match($BuildText, 'Property::AE_Effect_Match_Name\("([^"]+)"\)')
    if (-not $name.Success -or -not $matchName.Success) {
        throw "Could not extract effect identity from $Label"
    }
    [ordered]@{
        display_name = $name.Groups[1].Value
        match_name = $matchName.Groups[1].Value
    }
}

function New-ParticleManifest {
    $lib = Read-Text "src\lib.rs"
    $build = Read-Text "build.rs"
    $enumOrder = Extract-ParamsList $lib "PARAM_ENUM_ABI_ORDER" "Params"
    $setupOrder = Extract-ParamsList $lib "PARAM_SETUP_ABI_ORDER" "Params"
    $legacy = Extract-ParamsList $lib "LEGACY_PLEXUS_PARAMS" "Params"
    [ordered]@{
        manifest_version = 1
        product_id = "particlekit"
        effect = Extract-EffectIdentity $build "build.rs"
        param_count = $enumOrder.Count
        enum_abi_order = $enumOrder
        setup_abi_order = $setupOrder
        retired_legacy_slots = [ordered]@{
            label = "legacy_plexus"
            params = $legacy
        }
    }
}

function New-LatticeManifest {
    $adapter = Read-Text "crates\lattice_lab_plugin\src\lattice_ae_adapter.rs"
    $build = Read-Text "crates\lattice_lab_plugin\build.rs"
    $order = Extract-LatticeParamsList $adapter
    [ordered]@{
        manifest_version = 1
        product_id = "latticelab"
        effect = Extract-EffectIdentity $build "crates/lattice_lab_plugin/build.rs"
        param_count = $order.Count
        param_abi_order = $order
    }
}

function Write-Manifest {
    param(
        [object]$Manifest,
        [string]$RelativePath
    )
    $path = Join-Path $Root $RelativePath
    $json = $Manifest | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $path -Value ($json + [Environment]::NewLine) -Encoding UTF8
}

function Assert-ArrayEqual {
    param(
        [string]$Label,
        [object[]]$Expected,
        [object[]]$Actual
    )
    if ($Expected.Count -ne $Actual.Count) {
        throw "$Label length mismatch: expected $($Expected.Count), actual $($Actual.Count)"
    }
    for ($i = 0; $i -lt $Expected.Count; $i++) {
        if ([string]$Expected[$i] -ne [string]$Actual[$i]) {
            throw "$Label mismatch at index ${i}: expected $($Expected[$i]), actual $($Actual[$i])"
        }
    }
}

function Assert-ManifestEqual {
    param(
        [string]$Label,
        [object]$Expected,
        [object]$Actual
    )
    if ($Expected.manifest_version -ne $Actual.manifest_version) {
        throw "$Label manifest version mismatch"
    }
    if ($Expected.product_id -ne $Actual.product_id) {
        throw "$Label product_id mismatch"
    }
    if ($Expected.effect.display_name -ne $Actual.effect.display_name) {
        throw "$Label display_name mismatch"
    }
    if ($Expected.effect.match_name -ne $Actual.effect.match_name) {
        throw "$Label match_name mismatch"
    }
    if ($Expected.param_count -ne $Actual.param_count) {
        throw "$Label param_count mismatch"
    }
}

$particleManifestPath = "docs\PARTICLEKIT_ABI_MANIFEST.json"
$latticeManifestPath = "docs\LATTICE_LAB_ABI_MANIFEST.json"
$particle = New-ParticleManifest
$lattice = New-LatticeManifest

if ($Update) {
    Write-Manifest $particle $particleManifestPath
    Write-Manifest $lattice $latticeManifestPath
    Write-Host "ABI manifests updated."
    exit 0
}

$particleCurrent = Get-Content -Raw -LiteralPath (Join-Path $Root $particleManifestPath) | ConvertFrom-Json
$latticeCurrent = Get-Content -Raw -LiteralPath (Join-Path $Root $latticeManifestPath) | ConvertFrom-Json

Assert-ManifestEqual "ParticleKit ABI manifest" $particle $particleCurrent
Assert-ArrayEqual "ParticleKit enum_abi_order" $particle.enum_abi_order $particleCurrent.enum_abi_order
Assert-ArrayEqual "ParticleKit setup_abi_order" $particle.setup_abi_order $particleCurrent.setup_abi_order
Assert-ArrayEqual "ParticleKit retired_legacy_slots" $particle.retired_legacy_slots.params $particleCurrent.retired_legacy_slots.params

Assert-ManifestEqual "Lattice Lab ABI manifest" $lattice $latticeCurrent
Assert-ArrayEqual "Lattice Lab param_abi_order" $lattice.param_abi_order $latticeCurrent.param_abi_order

Write-Host "ABI manifest verification passed."
