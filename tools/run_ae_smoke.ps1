param(
    [string]$AfterFxPath = "",
    [string]$ReportPath = "",
    [int]$TimeoutSeconds = 180,
    [switch]$Build,
    [switch]$Deploy,
    [ValidateSet("SystemMediaCore", "UserMediaCore")]
    [string]$DeployTarget = "SystemMediaCore",
    [ValidateSet("EvalFile", "RunFile", "StartupFolder")]
    [string]$ScriptLaunchMode = "EvalFile",
    [string]$DeployBackupDir = "",
    [switch]$NoDeployBackup,
    [switch]$RestoreDeployBackupAfterSmoke,
    [switch]$VerifyDeploymentOnly,
    [switch]$AllowRunningAe
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ReleaseDir = Join-Path $Root "target\release"
$ParticleDll = Join-Path $ReleaseDir "particlekit.dll"
$LatticeDll = Join-Path $ReleaseDir "lattice_lab_plugin.dll"
$SystemMediaCoreDir = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
$UserMediaCoreDir = Join-Path $env:APPDATA "Adobe\Common\Plug-ins\7.0\MediaCore"
$MediaCoreDir = if ($DeployTarget -eq "UserMediaCore") { $UserMediaCoreDir } else { $SystemMediaCoreDir }
$SmokeDir = Join-Path $Root "target\ae-smoke"
$PluginEvidencePath = Join-Path $SmokeDir "deployed_plugin_evidence.json"
$SmokeRunId = [guid]::NewGuid().ToString("N")

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )
    Write-Host "==> $Name"
    & $Script
}

function Find-AfterFx {
    $candidates = Get-ChildItem -Path "C:\Program Files\Adobe" -Recurse -Filter AfterFX.exe -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending
    if (-not $candidates -or $candidates.Count -eq 0) {
        throw "AfterFX.exe was not found under C:\Program Files\Adobe."
    }
    return $candidates[0].FullName
}

function Assert-File {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing file: $Path"
    }
}

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-SystemMediaCoreWriteAllowed {
    if ($DeployTarget -ne "SystemMediaCore") {
        return
    }
    if (-not (Test-IsAdministrator)) {
        throw "SystemMediaCore deploy writes to $SystemMediaCoreDir and requires an elevated PowerShell. Run this command as Administrator, or use -DeployTarget UserMediaCore for non-elevated deployment checks."
    }
}

function Assert-CleanAeSession {
    if ($AllowRunningAe) {
        return
    }
    $running = Get-Process -Name AfterFX -ErrorAction SilentlyContinue
    if ($running) {
        $summary = ($running | ForEach-Object { "pid=$($_.Id) path=$($_.Path)" }) -join [Environment]::NewLine
        throw "After Effects is already running. Close AE or pass -AllowRunningAe if you intentionally want to run against the active session.$([Environment]::NewLine)$summary"
    }
}

function Stop-SmokeAfterFxProcess {
    param([switch]$Force)
    if ($AllowRunningAe -or -not $script:AfterFxProcess) {
        return
    }
    $process = Get-Process -Id $script:AfterFxProcess.Id -ErrorAction SilentlyContinue
    if (-not $process) {
        return
    }
    $process.CloseMainWindow() | Out-Null
    Start-Sleep -Seconds 5
    $process = Get-Process -Id $script:AfterFxProcess.Id -ErrorAction SilentlyContinue
    if ($process -and $Force) {
        Stop-Process -Id $script:AfterFxProcess.Id -Force -ErrorAction SilentlyContinue
    } elseif ($process) {
        Write-Warning "AfterFX PID $($script:AfterFxProcess.Id) is still running."
    }
}

function New-DeployBackupDirectory {
    if ($NoDeployBackup) {
        return $null
    }
    if (-not [string]::IsNullOrWhiteSpace($DeployBackupDir)) {
        $dir = [System.IO.Path]::GetFullPath($DeployBackupDir)
    } else {
        $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
        $targetLabel = if ($DeployTarget -eq "UserMediaCore") { "user" } else { "system" }
        $dir = Join-Path $Root "target\ae-smoke\deploy-backups\$stamp-$targetLabel"
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    return $dir
}

function Backup-PluginTarget {
    param([string]$TargetPath)
    if ($NoDeployBackup) {
        return
    }
    if (-not $script:EffectiveDeployBackupDir) {
        $script:EffectiveDeployBackupDir = New-DeployBackupDirectory
        Write-Host "Backup: $script:EffectiveDeployBackupDir"
    }

    $name = Split-Path -Leaf $TargetPath
    $backupPath = Join-Path $script:EffectiveDeployBackupDir $name
    $missingPath = Join-Path $script:EffectiveDeployBackupDir "$name.missing"
    if ((Test-Path -LiteralPath $backupPath) -or (Test-Path -LiteralPath $missingPath)) {
        return
    }
    if (Test-Path -LiteralPath $TargetPath) {
        Copy-Item -LiteralPath $TargetPath -Destination $backupPath -Force
    } else {
        Set-Content -LiteralPath $missingPath -Value "missing before deploy" -Encoding ASCII
    }
}

function Restore-DeployBackupIfRequested {
    if (-not $RestoreDeployBackupAfterSmoke -or -not $script:EffectiveDeployBackupDir) {
        return
    }
    Invoke-Step "Restore deploy backup" {
        foreach ($name in @("ParticleKit.aex", "LatticeLab.aex")) {
            $targetPath = Join-Path $MediaCoreDir $name
            $backupPath = Join-Path $script:EffectiveDeployBackupDir $name
            $missingPath = Join-Path $script:EffectiveDeployBackupDir "$name.missing"
            if (Test-Path -LiteralPath $backupPath) {
                Copy-Item -LiteralPath $backupPath -Destination $targetPath -Force
                Write-Host "Restored $name"
            } elseif (Test-Path -LiteralPath $missingPath) {
                if (Test-Path -LiteralPath $targetPath) {
                    Remove-Item -LiteralPath $targetPath -Force
                    Write-Host "Removed newly deployed $name"
                }
            }
        }
    }
}

function Copy-Plugin {
    param(
        [string]$Source,
        [string]$TargetName
    )
    Assert-File $Source
    if (-not (Test-Path -LiteralPath $MediaCoreDir)) {
        New-Item -ItemType Directory -Force -Path $MediaCoreDir | Out-Null
    }
    $targetPath = Join-Path $MediaCoreDir $TargetName
    Backup-PluginTarget $targetPath
    Copy-Item -LiteralPath $Source -Destination $targetPath -Force
}

function Get-FileEvidence {
    param([string]$Path)
    Assert-File $Path
    $item = Get-Item -LiteralPath $Path
    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $Path
    [pscustomobject]@{
        path = [System.IO.Path]::GetFullPath($Path)
        bytes = $item.Length
        sha256 = $hash.Hash.ToLowerInvariant()
    }
}

function New-PluginEvidence {
    param(
        [string]$Name,
        [string]$ReleasePath,
        [string]$TargetName
    )

    $targetPath = Join-Path $MediaCoreDir $TargetName
    $release = Get-FileEvidence $ReleasePath
    $deployed = if (Test-Path -LiteralPath $targetPath) { Get-FileEvidence $targetPath } else { $null }
    $hashesMatch = $false
    if ($deployed) {
        $hashesMatch = $release.sha256 -eq $deployed.sha256
    }

    [pscustomobject]@{
        name = $Name
        deployName = $TargetName
        release = $release
        deployed = $deployed
        hashesMatch = $hashesMatch
    }
}

function Write-PluginDeploymentEvidence {
    param([switch]$RequireMatch)

    if (-not ((Test-Path -LiteralPath $ParticleDll) -and (Test-Path -LiteralPath $LatticeDll))) {
        if ($RequireMatch) {
            Assert-File $ParticleDll
            Assert-File $LatticeDll
        }
        Write-Warning "Release DLLs were not found; deployed plugin hash evidence was not written."
        return
    }

    New-Item -ItemType Directory -Force -Path $SmokeDir | Out-Null
    $plugins = @(
        (New-PluginEvidence -Name "ParticleKit" -ReleasePath $ParticleDll -TargetName "ParticleKit.aex"),
        (New-PluginEvidence -Name "LatticeLab" -ReleasePath $LatticeDll -TargetName "LatticeLab.aex")
    )
    $evidence = [pscustomobject]@{
        smokeRunId = $SmokeRunId
        generatedAt = (Get-Date).ToString("o")
        deployTarget = $DeployTarget
        mediaCoreDir = $MediaCoreDir
        requiredCurrentRelease = [bool]$RequireMatch
        plugins = $plugins
    }
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $PluginEvidencePath -Encoding UTF8
    Write-Host "Plugin evidence: $PluginEvidencePath"

    foreach ($plugin in $plugins) {
        $status = if ($plugin.hashesMatch) { "match" } else { "mismatch" }
        $deployedHash = if ($plugin.deployed) { $plugin.deployed.sha256 } else { "<missing>" }
        Write-Host "$($plugin.name): $status release=$($plugin.release.sha256) deployed=$deployedHash"
    }

    if ($RequireMatch) {
        $mismatches = @($plugins | Where-Object { -not $_.hashesMatch })
        if ($mismatches.Count -gt 0) {
            $names = ($mismatches | ForEach-Object { $_.name }) -join ", "
            throw "Deployed plugin file(s) do not match the current release build: $names. See $PluginEvidencePath"
        }
    }
}

function Escape-JsxString {
    param([string]$Value)
    return $Value.Replace("\", "\\").Replace('"', '\"')
}

function Resolve-AeStartupScriptPath {
    param([string]$ResolvedAfterFxPath)
    $afterEffectsPrefs = Join-Path $env:APPDATA "Adobe\After Effects"
    if (-not (Test-Path -LiteralPath $afterEffectsPrefs)) {
        throw "After Effects preferences folder was not found: $afterEffectsPrefs"
    }

    $majorVersion = $null
    if ($ResolvedAfterFxPath -match "After Effects (\d{4})") {
        $majorVersion = ([int]$Matches[1]) - 2000
    }

    if ($null -ne $majorVersion) {
        $versionDir = Get-ChildItem -LiteralPath $afterEffectsPrefs -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "$majorVersion.*" } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    } else {
        $versionDir = Get-ChildItem -LiteralPath $afterEffectsPrefs -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match "^\d+\.\d+$" } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }

    if (-not $versionDir) {
        throw "Could not resolve an After Effects Startup Scripts folder for $ResolvedAfterFxPath."
    }

    $startupDir = Join-Path $versionDir.FullName "Scripts\Startup"
    New-Item -ItemType Directory -Force -Path $startupDir | Out-Null
    Join-Path $startupDir "ParticleLab_AE_Smoke_Startup.jsx"
}

if ($Build) {
    Invoke-Step "Build ParticleKit release" {
        Push-Location $Root
        try {
            cargo build --release
        } finally {
            Pop-Location
        }
    }
    Invoke-Step "Build LatticeLab release" {
        Push-Location $Root
        try {
            cargo build --release -p lattice_lab_plugin
        } finally {
            Pop-Location
        }
    }
}

if ($Deploy) {
    Invoke-Step "Check deploy privilege" {
        Assert-SystemMediaCoreWriteAllowed
    }
    Invoke-Step "Deploy release plugins to $DeployTarget" {
        Write-Host "Target: $MediaCoreDir"
        Copy-Plugin -Source $ParticleDll -TargetName "ParticleKit.aex"
        Copy-Plugin -Source $LatticeDll -TargetName "LatticeLab.aex"
    }
}

Invoke-Step "Record deployed plugin evidence" {
    Write-PluginDeploymentEvidence -RequireMatch:$Deploy
}

if ($VerifyDeploymentOnly) {
    Restore-DeployBackupIfRequested
    Write-Host "Deployment evidence verification passed."
    exit 0
}

Invoke-Step "Check AE session safety" {
    Assert-CleanAeSession
}

if ([string]::IsNullOrWhiteSpace($AfterFxPath)) {
    $AfterFxPath = Find-AfterFx
}
Assert-File $AfterFxPath

New-Item -ItemType Directory -Force -Path $SmokeDir | Out-Null
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Join-Path $SmokeDir "ae_smoke_report.json"
}
$ReportPath = [System.IO.Path]::GetFullPath($ReportPath)
$ReportDir = Split-Path -Parent $ReportPath
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$WrapperPath = Join-Path $SmokeDir "run_ae_smoke.jsx"
$WrapperPingPath = Join-Path $SmokeDir "wrapper_started.txt"
$SmokeScriptPath = Join-Path $Root "tools\ae_smoke_test.jsx"

$reportEscaped = Escape-JsxString $ReportPath
$pingEscaped = Escape-JsxString ([System.IO.Path]::GetFullPath($WrapperPingPath))
$scriptEscaped = Escape-JsxString ([System.IO.Path]::GetFullPath($SmokeScriptPath))

@"
var particleLabSmokePing = new File("$pingEscaped");
particleLabSmokePing.encoding = "UTF-8";
if (particleLabSmokePing.open("w")) {
    particleLabSmokePing.write("started");
    particleLabSmokePing.close();
}
$.global.PARTICLELAB_AE_SMOKE_REPORT = "$reportEscaped";
$.global.PARTICLELAB_AE_SMOKE_QUIT = true;
$.global.PARTICLELAB_AE_SMOKE_RUN_ID = "$SmokeRunId";
$.evalFile("$scriptEscaped");
"@ | Set-Content -LiteralPath $WrapperPath -Encoding ASCII

if (Test-Path -LiteralPath $ReportPath) {
    Remove-Item -LiteralPath $ReportPath -Force
}
if (Test-Path -LiteralPath $WrapperPingPath) {
    Remove-Item -LiteralPath $WrapperPingPath -Force
}

Invoke-Step "Run AE smoke script" {
    Write-Host "AfterFX: $AfterFxPath"
    Write-Host "Report:  $ReportPath"
    Write-Host "Mode:    $ScriptLaunchMode"
    $script:StartupWrapperPath = $null
    if ($ScriptLaunchMode -eq "StartupFolder") {
        $script:StartupWrapperPath = Resolve-AeStartupScriptPath $AfterFxPath
        Copy-Item -LiteralPath $WrapperPath -Destination $script:StartupWrapperPath -Force
        Write-Host "Startup: $script:StartupWrapperPath"
        $arguments = @()
    } elseif ($ScriptLaunchMode -eq "RunFile") {
        $arguments = @("-r", $WrapperPath)
    } else {
        $wrapperEscaped = Escape-JsxString ([System.IO.Path]::GetFullPath($WrapperPath))
        $arguments = @("-s", "`$.evalFile(`"$wrapperEscaped`");")
    }
    $afterFxWorkingDir = Split-Path -Parent $AfterFxPath
    if ($arguments.Count -gt 0) {
        $script:AfterFxProcess = Start-Process `
            -FilePath $AfterFxPath `
            -WorkingDirectory $afterFxWorkingDir `
            -ArgumentList $arguments `
            -PassThru
    } else {
        $script:AfterFxProcess = Start-Process `
            -FilePath $AfterFxPath `
            -WorkingDirectory $afterFxWorkingDir `
            -PassThru
    }
    Write-Host "PID:     $($script:AfterFxProcess.Id)"
}

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $deadline) {
    if (Test-Path -LiteralPath $ReportPath) {
        break
    }
    Start-Sleep -Seconds 2
}

if (-not (Test-Path -LiteralPath $ReportPath)) {
    $pingStatus = if (Test-Path -LiteralPath $WrapperPingPath) { "wrapper script started" } else { "wrapper script did not start" }
    if ($script:StartupWrapperPath -and (Test-Path -LiteralPath $script:StartupWrapperPath)) {
        Remove-Item -LiteralPath $script:StartupWrapperPath -Force -ErrorAction SilentlyContinue
    }
    Stop-SmokeAfterFxProcess -Force
    Restore-DeployBackupIfRequested
    throw "AE smoke report was not written within $TimeoutSeconds seconds ($pingStatus). AfterFX PID was $($script:AfterFxProcess.Id)."
}

if ($script:StartupWrapperPath -and (Test-Path -LiteralPath $script:StartupWrapperPath)) {
    Remove-Item -LiteralPath $script:StartupWrapperPath -Force -ErrorAction SilentlyContinue
}

$report = Get-Content -Raw -LiteralPath $ReportPath | ConvertFrom-Json
Write-Host "AE: $($report.appName) $($report.appVersion)"
foreach ($check in $report.checks) {
    Write-Host "Effect: $($check.requestedMatchName) -> $($check.matchName) / $($check.name) properties=$($check.propertyCount)"
}
if (-not $report.pass) {
    $errors = ($report.errors | ForEach-Object { "- $_" }) -join [Environment]::NewLine
    Stop-SmokeAfterFxProcess -Force
    Restore-DeployBackupIfRequested
    throw "AE smoke test failed:$([Environment]::NewLine)$errors"
}

try {
    Invoke-Step "Verify AE smoke report" {
        $verifyArgs = @(
            "-ReportPath", $ReportPath,
            "-EvidencePath", $PluginEvidencePath
        )
        if ($Deploy) {
            $verifyArgs += @("-RequireCurrentRelease", "-ExpectedDeployTarget", $DeployTarget)
        }
        & (Join-Path $PSScriptRoot "verify_ae_smoke_report.ps1") @verifyArgs
    }
}
catch {
    Stop-SmokeAfterFxProcess -Force
    Restore-DeployBackupIfRequested
    throw
}

Stop-SmokeAfterFxProcess
Restore-DeployBackupIfRequested
Write-Host "AE smoke test passed."
