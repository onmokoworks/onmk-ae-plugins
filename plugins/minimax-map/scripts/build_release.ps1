param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'

$rustDir = Join-Path $RepoRoot 'rust'
$dllPath = Join-Path $rustDir 'target\release\minimax_map.dll'
$aexPath = Join-Path $rustDir 'target\release\MinimaxMap.aex'

Push-Location $rustDir
try {
  cargo build --release
  if ($LASTEXITCODE -ne 0) {
    throw "cargo build --release failed with exit code $LASTEXITCODE"
  }
  Copy-Item -LiteralPath $dllPath -Destination $aexPath -Force
  Write-Host "Built $aexPath"
}
finally {
  Pop-Location
}
