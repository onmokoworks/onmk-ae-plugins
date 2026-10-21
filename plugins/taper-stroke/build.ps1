$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path (Join-Path $PSScriptRoot 'dist') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target/release/taper_stroke.dll') -Destination (Join-Path $PSScriptRoot 'dist/ONMK_TaperStroke.aex')
    Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'dist/ONMK_TaperStroke.aex') -Algorithm SHA256
} finally { Pop-Location }
