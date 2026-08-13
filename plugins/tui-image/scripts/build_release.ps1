param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'

$rustDir = Join-Path $RepoRoot 'rust'
$dllPath = Join-Path $rustDir 'target\release\tui_image_renderer.dll'
$aexPath = Join-Path $rustDir 'target\release\TuiImage.aex'

Push-Location $rustDir
try {
  cargo build --release
  Copy-Item -LiteralPath $dllPath -Destination $aexPath -Force
  Write-Host "Built $aexPath"
}
finally {
  Pop-Location
}

