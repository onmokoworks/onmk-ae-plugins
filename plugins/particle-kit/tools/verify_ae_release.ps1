param(
    [switch]$Build
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ReleaseDir = Join-Path $Root "target\release"
$ParticleDll = Join-Path $ReleaseDir "particlekit.dll"
$LatticeDll = Join-Path $ReleaseDir "lattice_lab_plugin.dll"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )
    Write-Host "==> $Name"
    & $Script
}

function Assert-File {
    param(
        [string]$Path,
        [int64]$MinBytes
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing file: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -lt $MinBytes) {
        throw "File is unexpectedly small: $Path ($($item.Length) bytes)"
    }
    return $item
}

function Read-BinaryAscii {
    param([string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    [Text.Encoding]::ASCII.GetString($bytes)
}

function Assert-Contains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Label
    )
    if (-not $Text.Contains($Needle)) {
        throw "$Label does not contain expected marker: $Needle"
    }
}

function Assert-NotContains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Label
    )
    if ($Text.Contains($Needle)) {
        throw "$Label unexpectedly contains marker: $Needle"
    }
}

if ($Build) {
    Invoke-Step "Build ParticleKit release" {
        Push-Location $Root
        try {
            cargo build --release
        } finally {
            Pop-Location
        }
    }
    Invoke-Step "Build LatticeLab release" {
        Push-Location $Root
        try {
            cargo build --release -p lattice_lab_plugin
        } finally {
            Pop-Location
        }
    }
}

Invoke-Step "Check release DLLs" {
    $particle = Assert-File -Path $ParticleDll -MinBytes 65536
    $lattice = Assert-File -Path $LatticeDll -MinBytes 65536
    Write-Host "ParticleKit: $($particle.Length) bytes"
    Write-Host "LatticeLab:  $($lattice.Length) bytes"
}

$particleText = Read-BinaryAscii $ParticleDll
$latticeText = Read-BinaryAscii $LatticeDll

Invoke-Step "Check AE entry points" {
    Assert-Contains $particleText "EffectMain" "ParticleKit"
    Assert-Contains $particleText "PluginDataEntryFunction2" "ParticleKit"
    Assert-Contains $latticeText "EffectMain" "LatticeLab"
    Assert-Contains $latticeText "PluginDataEntryFunction2" "LatticeLab"
}

Invoke-Step "Check effect identities" {
    Assert-Contains $particleText "ParticleKit" "ParticleKit"
    Assert-Contains $particleText "Particle Kit" "ParticleKit"
    Assert-Contains $latticeText "LatticeLab" "LatticeLab"
    Assert-Contains $latticeText "Lattice Lab" "LatticeLab"
    Assert-NotContains $particleText "LatticeLab" "ParticleKit"
    Assert-NotContains $latticeText "ParticleKit" "LatticeLab"
}

Invoke-Step "Check deploy names" {
    $deploy = Get-Content -Raw -LiteralPath (Join-Path $Root "deploy.bat")
    $deployPs1 = Get-Content -Raw -LiteralPath (Join-Path $Root "tools\deploy_ae_plugins.ps1")
    Assert-Contains $deploy "tools\deploy_ae_plugins.ps1" "deploy.bat"
    Assert-Contains $deployPs1 "ParticleKit.aex" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployPs1 "LatticeLab.aex" "tools/deploy_ae_plugins.ps1"
    Assert-Contains $deployPs1 "RestoreFrom" "tools/deploy_ae_plugins.ps1"
}

Write-Host "AE release verification passed."
