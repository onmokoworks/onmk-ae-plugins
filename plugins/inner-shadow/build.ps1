$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    Copy-Item -LiteralPath target/release/inner_shadow.dll -Destination dist/InnerShadow.aex
    Get-FileHash -LiteralPath dist/InnerShadow.aex -Algorithm SHA256
} finally { Pop-Location }
