param(
    [ValidateSet("EvalFile", "RunFile", "StartupFolder")]
    [string]$ScriptLaunchMode = "RunFile",
    [int]$TimeoutSeconds = 240,
    [switch]$NoBuild,
    [switch]$AllowRunningAe
)

$ErrorActionPreference = "Stop"

$SmokeScript = Join-Path $PSScriptRoot "run_ae_smoke.ps1"
$PowerShellExe = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function ConvertTo-CommandLineArgument {
    param([string]$Value)

    if ($Value -notmatch '[\s"]') {
        return $Value
    }

    '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-FinalSmoke {
    $smokeArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $SmokeScript
    )
    if (-not $NoBuild) {
        $smokeArgs += "-Build"
    }
    $smokeArgs += @(
        "-Deploy",
        "-DeployTarget", "SystemMediaCore",
        "-ScriptLaunchMode", $ScriptLaunchMode,
        "-TimeoutSeconds", "$TimeoutSeconds",
        "-RestoreDeployBackupAfterSmoke"
    )
    if ($AllowRunningAe) {
        $smokeArgs += "-AllowRunningAe"
    }

    if (Test-IsAdministrator) {
        & $PowerShellExe @smokeArgs
        exit $LASTEXITCODE
    }

    Write-Host "Requesting Administrator PowerShell for the final SystemMediaCore AE smoke..."
    Write-Host "Command: $PowerShellExe $($smokeArgs -join ' ')"
    $argumentLine = ($smokeArgs | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join " "
    $process = Start-Process `
        -FilePath $PowerShellExe `
        -Verb RunAs `
        -ArgumentList $argumentLine `
        -Wait `
        -PassThru

    exit $process.ExitCode
}

Invoke-FinalSmoke
