param(
    [string]$OutPath = ""
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ReleaseDir = Join-Path $Root "target\release"
$SystemMediaCoreDir = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
$UserMediaCoreDir = Join-Path $env:APPDATA "Adobe\Common\Plug-ins\7.0\MediaCore"
$DefaultOutDir = Join-Path $Root "target\migration-audit"
if ([string]::IsNullOrWhiteSpace($OutPath)) {
    $OutPath = Join-Path $DefaultOutDir "ae_host_environment.json"
}
$OutPath = [System.IO.Path]::GetFullPath($OutPath)

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-FileEvidence {
    param([string]$Path)

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $fullPath)) {
        return [ordered]@{
            exists = $false
            path = $fullPath
            bytes = $null
            sha256 = $null
            lastWriteTime = $null
        }
    }

    $item = Get-Item -LiteralPath $fullPath
    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $fullPath
    [ordered]@{
        exists = $true
        path = $fullPath
        bytes = $item.Length
        sha256 = $hash.Hash.ToLowerInvariant()
        lastWriteTime = $item.LastWriteTime.ToString("o")
    }
}

function Get-ProductEvidence {
    param(
        [hashtable]$Product,
        [string]$MediaCoreDir
    )

    $release = Get-FileEvidence (Join-Path $ReleaseDir $Product.releaseDll)
    $deployed = Get-FileEvidence (Join-Path $MediaCoreDir $Product.deployName)
    $matchesRelease = $false
    if ($release.exists -and $deployed.exists) {
        $matchesRelease = $release.sha256 -eq $deployed.sha256
    }

    [ordered]@{
        name = $Product.name
        deployName = $Product.deployName
        releaseDll = $Product.releaseDll
        release = $release
        deployed = $deployed
        matchesRelease = $matchesRelease
    }
}

function Get-MediaCoreEvidence {
    param(
        [string]$Label,
        [string]$Path,
        [object[]]$Products
    )

    $pluginEvidence = @()
    foreach ($product in $Products) {
        $pluginEvidence += Get-ProductEvidence -Product $product -MediaCoreDir $Path
    }

    $allCurrent = $true
    foreach ($plugin in $pluginEvidence) {
        if (-not $plugin.matchesRelease) {
            $allCurrent = $false
        }
    }

    [ordered]@{
        label = $Label
        path = $Path
        exists = Test-Path -LiteralPath $Path
        allCurrentReleasePluginsPresent = $allCurrent
        plugins = $pluginEvidence
    }
}

function Find-AfterFxInstallations {
    $root = "C:\Program Files\Adobe"
    if (-not (Test-Path -LiteralPath $root)) {
        return @()
    }

    @(Get-ChildItem -Path $root -Recurse -Filter AfterFX.exe -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending |
        ForEach-Object {
            [ordered]@{
                path = $_.FullName
                lastWriteTime = $_.LastWriteTime.ToString("o")
            }
        })
}

$products = @(
    @{
        name = "ParticleKit"
        deployName = "ParticleKit.aex"
        releaseDll = "particlekit.dll"
    },
    @{
        name = "LatticeLab"
        deployName = "LatticeLab.aex"
        releaseDll = "lattice_lab_plugin.dll"
    }
)

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$systemEvidence = Get-MediaCoreEvidence -Label "SystemMediaCore" -Path $SystemMediaCoreDir -Products $products
$userEvidence = Get-MediaCoreEvidence -Label "UserMediaCore" -Path $UserMediaCoreDir -Products $products
$releaseReadyInSystem = [bool]$systemEvidence.allCurrentReleasePluginsPresent

$report = [ordered]@{
    generatedAt = (Get-Date).ToString("o")
    user = $identity.Name
    isAdministrator = Test-IsAdministrator
    afterEffectsInstallations = Find-AfterFxInstallations
    releaseDir = [System.IO.Path]::GetFullPath($ReleaseDir)
    mediaCore = [ordered]@{
        system = $systemEvidence
        user = $userEvidence
    }
    readiness = [ordered]@{
        systemMediaCoreHasCurrentReleasePlugins = $releaseReadyInSystem
        systemMediaCoreSmokeRequiresElevation = -not (Test-IsAdministrator)
    }
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutPath) | Out-Null
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $OutPath -Encoding UTF8

Write-Host "AE host environment: $OutPath"
Write-Host "User: $($report.user)"
Write-Host "Administrator: $($report.isAdministrator)"
Write-Host "AfterFX installs: $(@($report.afterEffectsInstallations).Count)"
foreach ($install in $report.afterEffectsInstallations) {
    Write-Host "  $($install.path)"
}

foreach ($mediaCore in @($systemEvidence, $userEvidence)) {
    Write-Host "$($mediaCore.label): $($mediaCore.path)"
    foreach ($plugin in $mediaCore.plugins) {
        $state = if ($plugin.matchesRelease) {
            "current"
        } elseif ($plugin.deployed.exists) {
            "stale-or-different"
        } else {
            "missing"
        }
        Write-Host "  $($plugin.deployName): $state"
    }
}

if (-not $releaseReadyInSystem) {
    Write-Host "SystemMediaCore is not ready for the final current-release smoke yet."
    Write-Host "Run the SystemMediaCore smoke from an elevated PowerShell after deployment."
}
