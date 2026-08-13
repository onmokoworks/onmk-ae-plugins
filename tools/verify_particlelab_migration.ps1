param(
    [switch]$SourceOnly
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    Write-Host ""
    Write-Host "==> $Name"
    & $Script
}

function Invoke-RepoCommand {
    param(
        [string]$Name,
        [string]$Command,
        [string[]]$Arguments = @()
    )

    Invoke-Step $Name {
        Push-Location $Root
        try {
            & $Command @Arguments
            if ($LASTEXITCODE -ne 0) {
                throw "$Name failed with exit code $LASTEXITCODE."
            }
        }
        finally {
            Pop-Location
        }
    }
}

function Test-PowerShellSyntax {
    param([string[]]$RelativePaths)

    foreach ($relativePath in $RelativePaths) {
        $path = Join-Path $Root $relativePath
        if (-not (Test-Path -LiteralPath $path)) {
            throw "Missing expected script: $relativePath"
        }

        $tokens = $null
        $parseErrors = $null
        [System.Management.Automation.Language.Parser]::ParseFile(
            $path,
            [ref]$tokens,
            [ref]$parseErrors
        ) | Out-Null

        if ($parseErrors.Count -gt 0) {
            $messages = $parseErrors | ForEach-Object {
                "${relativePath}:$($_.Extent.StartLineNumber): $($_.Message)"
            }
            throw "PowerShell syntax check failed:`n$($messages -join "`n")"
        }
    }
}

$powershellArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File")

Invoke-Step "PowerShell verifier syntax" {
    Test-PowerShellSyntax @(
        "tools\deploy_ae_plugins.ps1",
        "tools\inspect_ae_host_environment.ps1",
        "tools\run_final_ae_smoke_elevated.ps1",
        "tools\run_ae_smoke.ps1",
        "tools\verify_ae_smoke_report.ps1",
        "tools\verify_ae_release.ps1",
        "tools\verify_architecture.ps1",
        "tools\verify_compatibility_contract.ps1",
        "tools\verify_goal_audit.ps1",
        "tools\verify_node_ui_shell.ps1",
        "tools\verify_param_abi_manifest.ps1",
        "tools\verify_particlelab_migration.ps1"
    )
}

Invoke-RepoCommand "Rust format check" "cargo" @("fmt", "--check")
Invoke-RepoCommand "Architecture boundary verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_architecture.ps1")))
Invoke-RepoCommand "AE parameter ABI manifest verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_param_abi_manifest.ps1")))
Invoke-RepoCommand "Compatibility contract verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_compatibility_contract.ps1")))
Invoke-RepoCommand "Goal audit verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_goal_audit.ps1")))
Invoke-RepoCommand "Node UI shell verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_node_ui_shell.ps1")))
Invoke-RepoCommand "Engine-core public/API tests" "cargo" @("test", "-p", "particlelab_engine_core")
Invoke-RepoCommand "ParticleKit default tests" "cargo" @("test")
Invoke-RepoCommand "Lattice Lab tests" "cargo" @("test", "-p", "lattice_lab_plugin")

if (-not $SourceOnly) {
    Invoke-RepoCommand "Build ParticleKit release" "cargo" @("build", "--release")
    Invoke-RepoCommand "Build Lattice Lab release" "cargo" @("build", "--release", "-p", "lattice_lab_plugin")
    Invoke-RepoCommand "AE release binary verifier" "powershell" ($powershellArgs + @((Join-Path $Root "tools\verify_ae_release.ps1")))
}

Write-Host ""
if ($SourceOnly) {
    Write-Host "Source-level ParticleLab migration verification passed."
    Write-Host "Release packaging checks were skipped because -SourceOnly was passed."
}
else {
    Write-Host "Non-elevated ParticleLab migration verification passed."
}

Write-Host ""
Write-Host "Remaining external gate: run the deployed AE host smoke from an elevated shell when ready:"
Write-Host "powershell -NoProfile -ExecutionPolicy Bypass -File tools\run_final_ae_smoke_elevated.ps1"
Write-Host "This helper deploys the current release to SystemMediaCore and runs the AE smoke with RunFile launch."
Write-Host "Delegated command includes -RestoreDeployBackupAfterSmoke for reversible deployment."
