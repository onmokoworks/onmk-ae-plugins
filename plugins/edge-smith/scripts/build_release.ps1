param([string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path)
$ErrorActionPreference = 'Stop'
$dll = Join-Path $RepoRoot 'target\release\edge_smith.dll'
$aex = Join-Path $RepoRoot 'target\release\EdgeSmith.aex'
Push-Location $RepoRoot
try {
  cargo build --release
  if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed: $LASTEXITCODE" }
  Copy-Item -LiteralPath $dll -Destination $aex -Force
  Write-Host "Built $aex"
}
finally { Pop-Location }
