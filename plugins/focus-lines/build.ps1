$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    cargo build --release --offline --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path (Join-Path $PSScriptRoot 'dist') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target/release/focus_lines.dll') -Destination (Join-Path $PSScriptRoot 'dist/ONMK_FocusLines.aex')
    Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'dist/ONMK_FocusLines.aex') -Algorithm SHA256
} finally { Pop-Location }
