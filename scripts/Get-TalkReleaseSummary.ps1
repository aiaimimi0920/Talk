[CmdletBinding()]
param(
    [string]$ManifestPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedManifestPath = $ManifestPath
$manifestValidatorScriptPath = Join-Path $PSScriptRoot 'Test-TalkReleaseManifest.ps1'
if (-not (Test-Path -LiteralPath $manifestValidatorScriptPath)) {
    throw "Missing Talk release manifest validator script: $manifestValidatorScriptPath"
}
. $manifestValidatorScriptPath
$ManifestPath = $requestedManifestPath

function Convert-TalkReleaseSummaryMapValue {
    param($Value)

    if ($null -eq $Value) {
        return $null
    }

    $result = [ordered]@{}
    foreach ($property in $Value.PSObject.Properties) {
        $result[[string]$property.Name] = $property.Value
    }
    [pscustomobject]$result
}

function Convert-TalkReleaseSummaryOptionalString {
    param($Value)

    if ($null -eq $Value) {
        return $null
    }

    $text = [string]$Value
    if ([string]::IsNullOrWhiteSpace($text)) {
        return $null
    }

    $text
}

function New-TalkReleaseSummaryPhaseObject {
    param(
        $StatusKind,
        $StatusSummary,
        $Snapshot
    )

    $resolvedStatusKind = Convert-TalkReleaseSummaryOptionalString -Value $StatusKind
    $resolvedStatusSummary = Convert-TalkReleaseSummaryOptionalString -Value $StatusSummary
    if ([string]::IsNullOrWhiteSpace($resolvedStatusKind) -and
        [string]::IsNullOrWhiteSpace($resolvedStatusSummary) -and
        $null -eq $Snapshot) {
        return $null
    }

    [pscustomobject][ordered]@{
        statusKind = $resolvedStatusKind
        statusSummary = $resolvedStatusSummary
        snapshot = Convert-TalkReleaseSummaryMapValue -Value $Snapshot
    }
}

function New-TalkReleaseSummaryObjectFromManifest {
    param([Parameter(Mandatory = $true)]$Manifest)

    Assert-TalkReleaseManifestObject -Manifest $Manifest -Context 'Talk release manifest summary source'

    $talkDesktopExeRecord = @($Manifest.exes | Where-Object { $_.name -eq 'talk-desktop.exe' } | Select-Object -First 1)[0]

    $desktopSmokeItems = if ($null -eq $Manifest.desktopSmoke) {
        @()
    } else {
        @(Get-TalkReleaseManifestCollectionItems -Value $Manifest.desktopSmoke)
    }
    $nativePreflightItems = @(Get-TalkReleaseManifestCollectionItems -Value $Manifest.nativePreflight)
    $commandItems = @(Get-TalkReleaseManifestCollectionItems -Value $Manifest.commands)
    $desktopSmokeScenarioCount = @($desktopSmokeItems).Count
    $nativePreflightCheckCount = @($nativePreflightItems).Count
    $verificationCommandCount = @($commandItems).Count

    [pscustomobject][ordered]@{
        schemaVersion = 1
        app = [string]$Manifest.app
        sourceProject = [string]$Manifest.sourceProject
        versionId = [string]$Manifest.versionId
        builtAt = [string]$Manifest.builtAt
        manifestSchemaVersion = $Manifest.schemaVersion
        manifestPath = 'manifest.json'
        buildInfoPath = [string]$Manifest.buildInfo.path
        checksumPath = [string]$Manifest.checksums
        binaries = [pscustomobject][ordered]@{
            talkDesktopPath = [string]$talkDesktopExeRecord.path
        }
        verification = [pscustomobject][ordered]@{
            skipped = ($verificationCommandCount -eq 0)
            commands = @($commandItems | ForEach-Object { [string]$_.display })
        }
        desktopSmoke = [pscustomobject][ordered]@{
            skipped = ($null -eq $Manifest.desktopSmoke)
            scenarioCount = $desktopSmokeScenarioCount
            scenarios = @(
                foreach ($record in $desktopSmokeItems) {
                    $scenarioRecord = [ordered]@{
                        scenario = [string]$record.scenario
                        status = Convert-TalkReleaseSummaryOptionalString -Value $record.status
                        failureKind = Convert-TalkReleaseSummaryOptionalString -Value (Get-TalkReleaseManifestPropertyValue -Object $record -Name 'failureKind')
                        failureSummary = Convert-TalkReleaseSummaryOptionalString -Value (Get-TalkReleaseManifestPropertyValue -Object $record -Name 'failureSummary')
                        failureEvidencePath = Convert-TalkReleaseSummaryOptionalString -Value (Get-TalkReleaseManifestPropertyValue -Object $record -Name 'failureEvidencePath')
                        insertTargetDiagnosticPath = Convert-TalkReleaseSummaryOptionalString -Value (Get-TalkReleaseManifestPropertyValue -Object $record -Name 'insertTargetDiagnosticPath')
                        statusKind = Convert-TalkReleaseSummaryOptionalString -Value $record.statusKind
                        statusSummary = Convert-TalkReleaseSummaryOptionalString -Value $record.statusSummary
                        snapshot = Convert-TalkReleaseSummaryMapValue -Value $record.statusSnapshot
                        beforeReload = New-TalkReleaseSummaryPhaseObject `
                            -StatusKind $record.beforeReloadStatusKind `
                            -StatusSummary $record.beforeReloadStatusSummary `
                            -Snapshot $record.beforeReloadStatusSnapshot
                        afterReload = New-TalkReleaseSummaryPhaseObject `
                            -StatusKind $record.afterReloadStatusKind `
                            -StatusSummary $record.afterReloadStatusSummary `
                            -Snapshot $record.afterReloadStatusSnapshot
                    }

                    $retryCount = Get-TalkReleaseManifestPropertyValue -Object $record -Name 'retryCount'
                    if ($retryCount -is [int] -or $retryCount -is [long]) {
                        $scenarioRecord.retryCount = $retryCount
                    }

                    $retryReason = Convert-TalkReleaseSummaryOptionalString -Value (Get-TalkReleaseManifestPropertyValue -Object $record -Name 'retryReason')
                    if (-not [string]::IsNullOrWhiteSpace($retryReason)) {
                        $scenarioRecord.retryReason = $retryReason
                    }

                    [pscustomobject]$scenarioRecord
                }
            )
        }
        nativePreflight = [pscustomobject][ordered]@{
            skipped = ($nativePreflightCheckCount -eq 0)
            checkCount = $nativePreflightCheckCount
            checks = @(
                foreach ($record in $nativePreflightItems) {
                    [pscustomobject][ordered]@{
                        name = [string]$record.name
                        expectedError = [string]$record.expectedError
                        exitCode = $record.exitCode
                        evidencePath = [string]$record.evidencePath
                    }
                }
            )
        }
        nativeReadiness = [pscustomobject][ordered]@{
            skipped = ($null -eq $Manifest.nativeReadiness)
            evidencePath = if ($null -eq $Manifest.nativeReadiness) { $null } else { [string]$Manifest.nativeReadiness.evidencePath }
            audioStatus = if ($null -eq $Manifest.nativeReadiness) { $null } else { [string]$Manifest.nativeReadiness.audio.status }
            audioReason = if ($null -eq $Manifest.nativeReadiness) { $null } else { Get-TalkReleaseManifestPropertyValue -Object $Manifest.nativeReadiness.audio -Name 'reason' }
            clipboardStatus = if ($null -eq $Manifest.nativeReadiness) { $null } else { [string]$Manifest.nativeReadiness.clipboard.status }
            clipboardReason = if ($null -eq $Manifest.nativeReadiness) { $null } else { Get-TalkReleaseManifestPropertyValue -Object $Manifest.nativeReadiness.clipboard -Name 'reason' }
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    $manifest = Read-TalkReleaseManifest -ManifestPath $ManifestPath
    New-TalkReleaseSummaryObjectFromManifest -Manifest $manifest
}
