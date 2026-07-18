[CmdletBinding()]
param(
    [string]$SummaryPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedSummaryPath = $SummaryPath
$manifestValidatorScriptPath = Join-Path $PSScriptRoot 'Test-TalkReleaseManifest.ps1'
if (-not (Test-Path -LiteralPath $manifestValidatorScriptPath)) {
    throw "Missing Talk release manifest validator script: $manifestValidatorScriptPath"
}
. $manifestValidatorScriptPath
$SummaryPath = $requestedSummaryPath

function Validate-TalkReleaseSummaryRequiredString {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Object -Name $Name)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "missing required string property '$Name'"
        return
    }

    $value = Get-TalkReleaseManifestPropertyValue -Object $Object -Name $Name
    if (-not (Test-TalkReleaseManifestNonEmptyString -Value $value)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property '$Name' must be a non-empty string"
    }
}

function Validate-TalkReleaseSummaryOptionalString {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Object -Name $Name)) {
        return
    }

    $value = Get-TalkReleaseManifestPropertyValue -Object $Object -Name $Name
    if ($null -eq $value) {
        return
    }
    if (-not ($value -is [string])) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property '$Name' must be a string or null"
    }
}

function Validate-TalkReleaseSummaryRequiredInteger {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Object -Name $Name)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "missing required integer property '$Name'"
        return
    }

    $value = Get-TalkReleaseManifestPropertyValue -Object $Object -Name $Name
    if (-not ($value -is [int] -or $value -is [long])) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property '$Name' must be an integer"
    }
}

function Validate-TalkReleaseSummaryOptionalInteger {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Object -Name $Name)) {
        return
    }

    $value = Get-TalkReleaseManifestPropertyValue -Object $Object -Name $Name
    if ($null -eq $value) {
        return
    }
    if (-not ($value -is [int] -or $value -is [long])) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property '$Name' must be an integer or null"
    }
}

function Validate-TalkReleaseSummaryRequiredBoolean {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Object -Name $Name)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "missing required boolean property '$Name'"
        return
    }

    $value = Get-TalkReleaseManifestPropertyValue -Object $Object -Name $Name
    if (-not ($value -is [bool])) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property '$Name' must be a boolean"
    }
}

function Validate-TalkReleaseSummaryStringMap {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Value,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if ($null -eq $Value) {
        return
    }

    if (-not (Test-TalkReleaseManifestMapValue -Value $Value)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object or null'
        return
    }

    foreach ($property in $Value.PSObject.Properties) {
        if (-not ($property.Value -is [string])) {
            Add-TalkReleaseManifestValidationError `
                -Errors $Errors `
                -Path ($Path + '.' + [string]$property.Name) `
                -Message 'map values must be strings'
        }
    }
}

function Validate-TalkReleaseSummaryPhaseObject {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Value,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if ($null -eq $Value) {
        return
    }
    if (-not (Test-TalkReleaseManifestMapValue -Value $Value)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object or null'
        return
    }

    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'statusKind' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'statusSummary' -Path $Path
    Validate-TalkReleaseSummaryStringMap -Errors $Errors -Value (Get-TalkReleaseManifestPropertyValue -Object $Value -Name 'snapshot') -Path ($Path + '.snapshot')
}

function Validate-TalkReleaseSummaryScenario {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Value,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Value)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseSummaryRequiredString -Errors $Errors -Object $Value -Name 'scenario' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'status' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'failureKind' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'failureSummary' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'failureEvidencePath' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'insertTargetDiagnosticPath' -Path $Path
    Validate-TalkReleaseSummaryOptionalInteger -Errors $Errors -Object $Value -Name 'retryCount' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'retryReason' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'statusKind' -Path $Path
    Validate-TalkReleaseSummaryOptionalString -Errors $Errors -Object $Value -Name 'statusSummary' -Path $Path
    Validate-TalkReleaseSummaryStringMap -Errors $Errors -Value (Get-TalkReleaseManifestPropertyValue -Object $Value -Name 'snapshot') -Path ($Path + '.snapshot')
    Validate-TalkReleaseSummaryPhaseObject -Errors $Errors -Value (Get-TalkReleaseManifestPropertyValue -Object $Value -Name 'beforeReload') -Path ($Path + '.beforeReload')
    Validate-TalkReleaseSummaryPhaseObject -Errors $Errors -Value (Get-TalkReleaseManifestPropertyValue -Object $Value -Name 'afterReload') -Path ($Path + '.afterReload')
}

function Test-TalkReleaseSummaryStringCollectionValue {
    param($Value)

    if ($null -eq $Value) {
        return $true
    }
    if ($Value -is [System.Array]) {
        return $true
    }
    if ($Value -is [string]) {
        return $true
    }

    $false
}

function Get-TalkReleaseSummaryValidationErrors {
    param([Parameter(Mandatory = $true)]$Summary)

    $errors = New-Object System.Collections.Generic.List[string]
    if (-not (Test-TalkReleaseManifestMapValue -Value $Summary)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$' -Message 'summary must be an object'
        return $errors.ToArray()
    }

    Validate-TalkReleaseSummaryRequiredInteger -Errors $errors -Object $Summary -Name 'schemaVersion' -Path '$'
    $schemaVersion = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'schemaVersion'
    if (($schemaVersion -is [int] -or $schemaVersion -is [long]) -and $schemaVersion -ne 1) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.schemaVersion' -Message 'must equal 1'
    }

    Validate-TalkReleaseSummaryRequiredString -Errors $errors -Object $Summary -Name 'app' -Path '$'
    $app = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'app'
    if ($app -is [string] -and $app -ne 'Talk') {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.app' -Message "must equal 'Talk'"
    }

    Validate-TalkReleaseSummaryRequiredString -Errors $errors -Object $Summary -Name 'sourceProject' -Path '$'
    $sourceProject = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'sourceProject'
    if ($sourceProject -is [string] -and $sourceProject -ne 'Talk') {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.sourceProject' -Message "must equal 'Talk'"
    }

    foreach ($name in @('versionId', 'builtAt', 'manifestPath', 'buildInfoPath', 'checksumPath')) {
        Validate-TalkReleaseSummaryRequiredString -Errors $errors -Object $Summary -Name $name -Path '$'
    }

    Validate-TalkReleaseSummaryRequiredInteger -Errors $errors -Object $Summary -Name 'manifestSchemaVersion' -Path '$'
    $manifestSchemaVersion = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'manifestSchemaVersion'
    if (($manifestSchemaVersion -is [int] -or $manifestSchemaVersion -is [long]) -and $manifestSchemaVersion -ne 2) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.manifestSchemaVersion' -Message 'must equal 2'
    }

    $binaries = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'binaries'
    if (-not (Test-TalkReleaseManifestMapValue -Value $binaries)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.binaries' -Message 'must be an object'
    } else {
        Validate-TalkReleaseSummaryRequiredString -Errors $errors -Object $binaries -Name 'talkDesktopPath' -Path '$.binaries'
    }

    $verification = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'verification'
    if (-not (Test-TalkReleaseManifestMapValue -Value $verification)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.verification' -Message 'must be an object'
    } else {
        Validate-TalkReleaseSummaryRequiredBoolean -Errors $errors -Object $verification -Name 'skipped' -Path '$.verification'
        $commands = Get-TalkReleaseManifestPropertyValue -Object $verification -Name 'commands'
        if (-not (Test-TalkReleaseSummaryStringCollectionValue -Value $commands)) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.verification.commands' -Message 'must be an array'
        } else {
            $commandItems = @(Get-TalkReleaseManifestCollectionItems -Value $commands)
            for ($index = 0; $index -lt $commandItems.Count; $index++) {
                if (-not (Test-TalkReleaseManifestNonEmptyString -Value $commandItems[$index])) {
                    Add-TalkReleaseManifestValidationError -Errors $errors -Path ('$.verification.commands[{0}]' -f $index) -Message 'must be a non-empty string'
                }
            }
        }
    }

    $desktopSmoke = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'desktopSmoke'
    if (-not (Test-TalkReleaseManifestMapValue -Value $desktopSmoke)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.desktopSmoke' -Message 'must be an object'
    } else {
        Validate-TalkReleaseSummaryRequiredBoolean -Errors $errors -Object $desktopSmoke -Name 'skipped' -Path '$.desktopSmoke'
        Validate-TalkReleaseSummaryRequiredInteger -Errors $errors -Object $desktopSmoke -Name 'scenarioCount' -Path '$.desktopSmoke'
        $scenarios = Get-TalkReleaseManifestPropertyValue -Object $desktopSmoke -Name 'scenarios'
        if (-not (Test-TalkReleaseManifestCollectionValue -Value $scenarios)) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.desktopSmoke.scenarios' -Message 'must be an array'
        } else {
            $scenarioItems = @(Get-TalkReleaseManifestCollectionItems -Value $scenarios)
            $scenarioCount = Get-TalkReleaseManifestPropertyValue -Object $desktopSmoke -Name 'scenarioCount'
            if ($scenarioCount -is [int] -or $scenarioCount -is [long]) {
                if ($scenarioCount -ne $scenarioItems.Count) {
                    Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.desktopSmoke.scenarioCount' -Message 'must match the number of desktopSmoke.scenarios entries'
                }
            }
            for ($index = 0; $index -lt $scenarioItems.Count; $index++) {
                Validate-TalkReleaseSummaryScenario -Errors $errors -Value $scenarioItems[$index] -Path ('$.desktopSmoke.scenarios[{0}]' -f $index)
            }
        }
    }

    $nativePreflight = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'nativePreflight'
    if (-not (Test-TalkReleaseManifestMapValue -Value $nativePreflight)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.nativePreflight' -Message 'must be an object'
    } else {
        Validate-TalkReleaseSummaryRequiredBoolean -Errors $errors -Object $nativePreflight -Name 'skipped' -Path '$.nativePreflight'
        Validate-TalkReleaseSummaryRequiredInteger -Errors $errors -Object $nativePreflight -Name 'checkCount' -Path '$.nativePreflight'
        $checks = Get-TalkReleaseManifestPropertyValue -Object $nativePreflight -Name 'checks'
        if (-not (Test-TalkReleaseManifestCollectionValue -Value $checks)) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.nativePreflight.checks' -Message 'must be an array'
        } else {
            $checkItems = @(Get-TalkReleaseManifestCollectionItems -Value $checks)
            $checkCount = Get-TalkReleaseManifestPropertyValue -Object $nativePreflight -Name 'checkCount'
            if ($checkCount -is [int] -or $checkCount -is [long]) {
                if ($checkCount -ne $checkItems.Count) {
                    Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.nativePreflight.checkCount' -Message 'must match the number of nativePreflight.checks entries'
                }
            }
            for ($index = 0; $index -lt $checkItems.Count; $index++) {
                $check = $checkItems[$index]
                $checkPath = '$.nativePreflight.checks[{0}]' -f $index
                if (-not (Test-TalkReleaseManifestMapValue -Value $check)) {
                    Add-TalkReleaseManifestValidationError -Errors $errors -Path $checkPath -Message 'must be an object'
                    continue
                }
                foreach ($name in @('name', 'expectedError', 'evidencePath')) {
                    Validate-TalkReleaseSummaryRequiredString -Errors $errors -Object $check -Name $name -Path $checkPath
                }
                Validate-TalkReleaseSummaryRequiredInteger -Errors $errors -Object $check -Name 'exitCode' -Path $checkPath
            }
        }
    }

    $nativeReadiness = Get-TalkReleaseManifestPropertyValue -Object $Summary -Name 'nativeReadiness'
    if (-not (Test-TalkReleaseManifestMapValue -Value $nativeReadiness)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.nativeReadiness' -Message 'must be an object'
    } else {
        Validate-TalkReleaseSummaryRequiredBoolean -Errors $errors -Object $nativeReadiness -Name 'skipped' -Path '$.nativeReadiness'
        foreach ($name in @('evidencePath', 'audioStatus', 'audioReason', 'clipboardStatus', 'clipboardReason')) {
            Validate-TalkReleaseSummaryOptionalString -Errors $errors -Object $nativeReadiness -Name $name -Path '$.nativeReadiness'
        }
    }

    $errors.ToArray()
}

function Assert-TalkReleaseSummaryObject {
    param(
        [Parameter(Mandatory = $true)]$Summary,
        [string]$Context = 'Talk release summary'
    )

    $errors = @(Get-TalkReleaseSummaryValidationErrors -Summary $Summary)
    if ($errors.Count -eq 0) {
        return
    }

    throw ("{0} is invalid:`n - {1}" -f $Context, ($errors -join "`n - "))
}

function Read-TalkReleaseSummary {
    param([Parameter(Mandatory = $true)][string]$SummaryPath)

    $resolvedPath = [System.IO.Path]::GetFullPath($SummaryPath)
    if (-not (Test-Path -LiteralPath $resolvedPath)) {
        throw "Talk release summary does not exist: $resolvedPath"
    }

    Get-Content -LiteralPath $resolvedPath -Raw | ConvertFrom-Json
}

if ($MyInvocation.InvocationName -ne '.') {
    $summary = Read-TalkReleaseSummary -SummaryPath $SummaryPath
    Assert-TalkReleaseSummaryObject -Summary $summary -Context $SummaryPath
    [pscustomobject]@{
        SummaryPath = [System.IO.Path]::GetFullPath($SummaryPath)
        SchemaVersion = $summary.schemaVersion
        ManifestSchemaVersion = $summary.manifestSchemaVersion
        DesktopSmokeScenarioCount = $summary.desktopSmoke.scenarioCount
        NativePreflightCheckCount = $summary.nativePreflight.checkCount
    }
}
