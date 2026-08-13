param(
    [string]$InstallDir = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$CrateDir = Join-Path $RepoRoot "rust\celglow"
$BuiltDll = Join-Path $CrateDir "target\release\celglow.dll"
$InstallPath = Join-Path $InstallDir "CelGlow.aex"

if (-not $SkipBuild) {
    Push-Location $CrateDir
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $BuiltDll)) {
    throw "Build output not found: $BuiltDll"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -LiteralPath $BuiltDll -Destination $InstallPath -Force

Write-Host "Installed CelGlow:"
Write-Host "  $InstallPath"
Write-Host "Restart After Effects and open Effects > onmk > CelGlow."
