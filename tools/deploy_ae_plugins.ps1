param(
    [ValidateSet("SystemMediaCore", "UserMediaCore")]
    [string]$DeployTarget = "SystemMediaCore",
    [string]$BackupDir = "",
    [string]$RestoreFrom = "",
    [switch]$NoBackup
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ReleaseDir = Join-Path $Root "target\release"
$ParticleDll = Join-Path $ReleaseDir "particlekit.dll"
$LatticeDll = Join-Path $ReleaseDir "lattice_lab_plugin.dll"
$SystemMediaCoreDir = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
$UserMediaCoreDir = Join-Path $env:APPDATA "Adobe\Common\Plug-ins\7.0\MediaCore"
$MediaCoreDir = if ($DeployTarget -eq "UserMediaCore") { $UserMediaCoreDir } else { $SystemMediaCoreDir }

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

function New-BackupDirectory {
    if ($NoBackup) {
        return $null
    }
    if (-not [string]::IsNullOrWhiteSpace($BackupDir)) {
        $dir = [System.IO.Path]::GetFullPath($BackupDir)
    } else {
        $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
        $targetLabel = if ($DeployTarget -eq "UserMediaCore") { "user" } else { "system" }
        $dir = Join-Path $Root "target\deploy-backups\$stamp-$targetLabel"
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    return $dir
}

function Backup-Target {
    param(
        [string]$TargetPath,
        [string]$EffectiveBackupDir
    )
    if ($NoBackup) {
        return
    }
    $name = Split-Path -Leaf $TargetPath
    $backupPath = Join-Path $EffectiveBackupDir $name
    $missingPath = Join-Path $EffectiveBackupDir "$name.missing"
    if (Test-Path -LiteralPath $TargetPath) {
        Copy-Item -LiteralPath $TargetPath -Destination $backupPath -Force
    } else {
        Set-Content -LiteralPath $missingPath -Value "missing before deploy" -Encoding ASCII
    }
}

function Restore-Backup {
    param([string]$BackupPath)
    Assert-File $BackupPath
    foreach ($name in @("ParticleKit.aex", "LatticeLab.aex")) {
        $targetPath = Join-Path $MediaCoreDir $name
        $fileBackup = Join-Path $BackupPath $name
        $missingMarker = Join-Path $BackupPath "$name.missing"
        if (Test-Path -LiteralPath $fileBackup) {
            Copy-Item -LiteralPath $fileBackup -Destination $targetPath -Force
            Write-Host "Restored $targetPath"
        } elseif (Test-Path -LiteralPath $missingMarker) {
            if (Test-Path -LiteralPath $targetPath) {
                Remove-Item -LiteralPath $targetPath -Force
                Write-Host "Removed $targetPath"
            }
        }
    }
}

function Assert-CopyMatches {
    param(
        [string]$Source,
        [string]$Target
    )

    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Source).Hash.ToLowerInvariant()
    $targetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Target).Hash.ToLowerInvariant()
    if ($sourceHash -ne $targetHash) {
        throw "Deploy hash mismatch for $Target. source=$sourceHash target=$targetHash"
    }
    Write-Host "Verified $Target SHA256 $targetHash"
}

if (-not [string]::IsNullOrWhiteSpace($RestoreFrom)) {
    Assert-SystemMediaCoreWriteAllowed
    Restore-Backup ([System.IO.Path]::GetFullPath($RestoreFrom))
    Write-Host "Deploy backup restored."
    exit 0
}

Assert-SystemMediaCoreWriteAllowed
Assert-File $ParticleDll
Assert-File $LatticeDll

if (-not (Test-Path -LiteralPath $MediaCoreDir)) {
    New-Item -ItemType Directory -Force -Path $MediaCoreDir | Out-Null
}

$effectiveBackupDir = New-BackupDirectory
if ($effectiveBackupDir) {
    Write-Host "Backup: $effectiveBackupDir"
}

$particleTarget = Join-Path $MediaCoreDir "ParticleKit.aex"
$latticeTarget = Join-Path $MediaCoreDir "LatticeLab.aex"

Backup-Target $particleTarget $effectiveBackupDir
Backup-Target $latticeTarget $effectiveBackupDir

Copy-Item -LiteralPath $ParticleDll -Destination $particleTarget -Force
Copy-Item -LiteralPath $LatticeDll -Destination $latticeTarget -Force
Assert-CopyMatches -Source $ParticleDll -Target $particleTarget
Assert-CopyMatches -Source $LatticeDll -Target $latticeTarget

Write-Host "Deployed ParticleKit: $particleTarget"
Write-Host "Deployed LatticeLab:  $latticeTarget"
if ($effectiveBackupDir) {
    Write-Host "Restore command:"
    Write-Host "powershell -NoProfile -ExecutionPolicy Bypass -File tools\deploy_ae_plugins.ps1 -DeployTarget $DeployTarget -RestoreFrom `"$effectiveBackupDir`""
}
