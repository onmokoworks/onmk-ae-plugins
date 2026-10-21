$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
$previousGpuRequirement = $env:INNER_SHADOW_REQUIRE_GPU
try {
    $env:INNER_SHADOW_REQUIRE_GPU = '1'
    cargo test --locked -- --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
    & ./build.ps1
    cargo run --locked --release --example render_fixture -- verification
    if ($LASTEXITCODE -ne 0) { throw 'fixture rendering failed' }
} finally {
    $env:INNER_SHADOW_REQUIRE_GPU = $previousGpuRequirement
    Pop-Location
}
