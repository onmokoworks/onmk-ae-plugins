param(
    [string]$ReportPath = "",
    [string]$EvidencePath = "",
    [switch]$RequireCurrentRelease,
    [ValidateSet("", "SystemMediaCore", "UserMediaCore")]
    [string]$ExpectedDeployTarget = ""
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Join-Path $Root "target\ae-smoke\ae_smoke_report.json"
}
if ([string]::IsNullOrWhiteSpace($EvidencePath)) {
    $EvidencePath = Join-Path $Root "target\ae-smoke\deployed_plugin_evidence.json"
}

function Assert-File {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing file: $Path"
    }
}

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equals {
    param(
        $Actual,
        $Expected,
        [string]$Message
    )

    if ($Actual -ne $Expected) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function As-Array {
    param($Value)

    if ($null -eq $Value) {
        return @()
    }
    return @($Value)
}

function Read-Json {
    param([string]$Path)

    Assert-File $Path
    Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
}

function Test-HashString {
    param($Value)

    return $Value -is [string] -and $Value -match "^[0-9a-fA-F]{64}$"
}

function Assert-EffectCheck {
    param(
        $Report,
        [string]$MatchName,
        [string]$DisplayName
    )

    $matches = @(As-Array $Report.checks | Where-Object { $_.requestedMatchName -eq $MatchName })
    Assert-Equals $matches.Count 1 "Smoke report must contain exactly one check for $MatchName."
    $check = $matches[0]
    Assert-Equals $check.matchName $MatchName "$MatchName matchName mismatch."
    Assert-Equals $check.expectedDisplayName $DisplayName "$MatchName expected display name mismatch."
    Assert-Equals $check.name $DisplayName "$MatchName display name mismatch."
    Assert-True ($check.propertyCount -is [int] -or $check.propertyCount -is [long]) "$MatchName propertyCount must be an integer."
    Assert-True ($check.propertyCount -gt 0) "$MatchName propertyCount must be positive."
    Assert-Equals $check.pass $true "$MatchName smoke check did not pass."
}

function Assert-PluginEvidence {
    param(
        $Evidence,
        [string]$Name,
        [string]$DeployName
    )

    $matches = @(As-Array $Evidence.plugins | Where-Object { $_.name -eq $Name })
    Assert-Equals $matches.Count 1 "Deploy evidence must contain exactly one plugin record for $Name."
    $plugin = $matches[0]
    Assert-Equals $plugin.deployName $DeployName "$Name deploy name mismatch."
    Assert-Equals $plugin.hashesMatch $true "$Name release/deployed hashes do not match."
    Assert-True ($null -ne $plugin.release) "$Name release evidence is missing."
    Assert-True ($null -ne $plugin.deployed) "$Name deployed evidence is missing."
    Assert-True (Test-HashString $plugin.release.sha256) "$Name release SHA256 is invalid."
    Assert-True (Test-HashString $plugin.deployed.sha256) "$Name deployed SHA256 is invalid."
    Assert-Equals $plugin.deployed.sha256 $plugin.release.sha256 "$Name SHA256 mismatch."
    Assert-True ($plugin.release.bytes -gt 65536) "$Name release file is unexpectedly small."
    Assert-Equals ([System.IO.Path]::GetFileName($plugin.deployed.path)) $DeployName "$Name deployed path leaf mismatch."
}

$report = Read-Json ([System.IO.Path]::GetFullPath($ReportPath))
Assert-Equals $report.pass $true "AE smoke report did not pass."
Assert-True (-not [string]::IsNullOrWhiteSpace([string]$report.appVersion)) "AE smoke report is missing appVersion."
Assert-Equals (As-Array $report.errors).Count 0 "AE smoke report contains errors."
Assert-EffectCheck -Report $report -MatchName "ParticleKit" -DisplayName "Particle Kit"
Assert-EffectCheck -Report $report -MatchName "LatticeLab" -DisplayName "Lattice Lab"

if ($RequireCurrentRelease) {
    $evidence = Read-Json ([System.IO.Path]::GetFullPath($EvidencePath))
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$report.smokeRunId)) "AE smoke report is missing smokeRunId."
    Assert-Equals $evidence.smokeRunId $report.smokeRunId "Deploy evidence smokeRunId does not match the AE smoke report."
    Assert-Equals $evidence.requiredCurrentRelease $true "Deploy evidence was not captured with current-release enforcement."
    if (-not [string]::IsNullOrWhiteSpace($ExpectedDeployTarget)) {
        Assert-Equals $evidence.deployTarget $ExpectedDeployTarget "Deploy evidence target mismatch."
    }
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$evidence.mediaCoreDir)) "Deploy evidence is missing mediaCoreDir."
    Assert-PluginEvidence -Evidence $evidence -Name "ParticleKit" -DeployName "ParticleKit.aex"
    Assert-PluginEvidence -Evidence $evidence -Name "LatticeLab" -DeployName "LatticeLab.aex"
}

Write-Host "AE smoke report verification passed."
