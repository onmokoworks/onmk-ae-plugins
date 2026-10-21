$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    Copy-Item -LiteralPath target/release/distance_gradation.dll -Destination dist/ONMK_DistanceGradation.aex
    Copy-Item -LiteralPath LICENSE,NOTICE -Destination dist
    Get-FileHash -LiteralPath dist/ONMK_DistanceGradation.aex -Algorithm SHA256
} finally { Pop-Location }
