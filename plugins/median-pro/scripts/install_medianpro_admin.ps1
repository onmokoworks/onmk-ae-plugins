param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path,
  [string]$PluginDir = $env:MEDIANPRO_PLUGIN_DIR
)

$ErrorActionPreference = 'Stop'

if (-not $PluginDir) {
  $PluginDir = 'C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore'
}

$source = Join-Path $RepoRoot 'rust\target\release\MedianPro.aex'
if (-not (Test-Path -LiteralPath $source)) {
  throw "Build artifact not found: $source. Run scripts\build_release.ps1 first."
}

New-Item -ItemType Directory -Force -Path $PluginDir | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $PluginDir 'MedianPro.aex') -Force
Write-Host "Installed MedianPro.aex to $PluginDir"
