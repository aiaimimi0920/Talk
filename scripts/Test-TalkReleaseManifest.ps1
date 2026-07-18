[CmdletBinding()]
param(
    [string]$ManifestPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-TalkReleaseManifestHasProperty {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($Object -is [System.Collections.IDictionary]) {
        return $Object.Contains($Name)
    }

    $null -ne $Object.PSObject.Properties[$Name]
}

function Get-TalkReleaseManifestPropertyValue {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($Object -is [System.Collections.IDictionary]) {
        if ($Object.Contains($Name)) {
            return $Object[$Name]
        }
        return $null
    }

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }

    $property.Value
}

function Test-TalkReleaseManifestMapValue {
    param($Value)

    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [System.Collections.IDictionary]) {
        return $true
    }
    if ($Value -is [string] -or $Value -is [ValueType] -or $Value -is [System.Array]) {
        return $false
    }

    $true
}

function Test-TalkReleaseManifestCollectionValue {
    param($Value)

    if ($null -eq $Value) {
        return $true
    }
    if ($Value -is [System.Array]) {
        return $true
    }
    if ($Value -is [string] -or $Value -is [ValueType]) {
        return $false
    }

    $true
}

function Get-TalkReleaseManifestCollectionItems {
    param($Value)

    if ($null -eq $Value) {
        return @()
    }
    if ($Value -is [System.Array]) {
        return @($Value)
    }

    @($Value)
}

function Add-TalkReleaseManifestValidationError {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Message
    )

    $Errors.Add(('{0}: {1}' -f $Path, $Message)) | Out-Null
}

function Test-TalkReleaseManifestNonEmptyString {
    param($Value)

    ($Value -is [string]) -and (-not [string]::IsNullOrWhiteSpace($Value))
}

function Validate-TalkReleaseManifestRequiredString {
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

function Validate-TalkReleaseManifestOptionalString {
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

function Validate-TalkReleaseManifestOptionalInteger {
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

function Validate-TalkReleaseManifestStringMap {
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

function Validate-TalkReleaseManifestCommandRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $CommandRecord,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $CommandRecord)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $CommandRecord -Name 'display' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $CommandRecord -Name 'workingDirectory' -Path $Path
}

function Validate-TalkReleaseManifestFileRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Record,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Record)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'kind' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'name' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'path' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'sha256' -Path $Path

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Record -Name 'bytes')) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "missing required integer property 'bytes'"
    } else {
        $bytes = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'bytes'
        if (-not ($bytes -is [int] -or $bytes -is [long])) {
            Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property 'bytes' must be an integer"
        }
    }
}

function Validate-TalkReleaseManifestBuildInfo {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $BuildInfo,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $BuildInfo)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $BuildInfo -Name 'kind' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $BuildInfo -Name 'path' -Path $Path
}

function Validate-TalkReleaseManifestBuildLogRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Record,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Record)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'kind' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'path' -Path $Path
}

function Validate-TalkReleaseManifestSupportFileRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Record,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Record)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'kind' -Path $Path
    Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name 'path' -Path $Path
}

function Validate-TalkReleaseManifestNativePreflightRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Record,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Record)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    foreach ($name in @('name', 'configPath', 'evidencePath', 'expectedError', 'outputText')) {
        Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name $name -Path $Path
    }

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Record -Name 'exitCode')) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "missing required integer property 'exitCode'"
    } else {
        $exitCode = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'exitCode'
        if (-not ($exitCode -is [int] -or $exitCode -is [long])) {
            Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property 'exitCode' must be an integer"
        }
    }
}

function Validate-TalkReleaseManifestNativeReadiness {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $NativeReadiness,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if ($null -eq $NativeReadiness) {
        return
    }
    if (-not (Test-TalkReleaseManifestMapValue -Value $NativeReadiness)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object or null'
        return
    }

    foreach ($name in @('configPath', 'evidencePath', 'outputText')) {
        Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $NativeReadiness -Name $name -Path $Path
    }

    $audio = Get-TalkReleaseManifestPropertyValue -Object $NativeReadiness -Name 'audio'
    if (-not (Test-TalkReleaseManifestMapValue -Value $audio)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path ($Path + '.audio') -Message 'must be an object'
    } else {
        Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $audio -Name 'status' -Path ($Path + '.audio')
        Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $audio -Name 'reason' -Path ($Path + '.audio')
        Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $audio -Name 'deviceName' -Path ($Path + '.audio')
        Validate-TalkReleaseManifestOptionalInteger -Errors $Errors -Object $audio -Name 'defaultSampleRateHz' -Path ($Path + '.audio')
        Validate-TalkReleaseManifestOptionalInteger -Errors $Errors -Object $audio -Name 'defaultChannels' -Path ($Path + '.audio')
        Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $audio -Name 'sampleFormat' -Path ($Path + '.audio')
    }

    $clipboard = Get-TalkReleaseManifestPropertyValue -Object $NativeReadiness -Name 'clipboard'
    if (-not (Test-TalkReleaseManifestMapValue -Value $clipboard)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path ($Path + '.clipboard') -Message 'must be an object'
    } else {
        Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $clipboard -Name 'status' -Path ($Path + '.clipboard')
        Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $clipboard -Name 'reason' -Path ($Path + '.clipboard')
    }
}

function Validate-TalkReleaseManifestDesktopSmokeRecord {
    param(
        [System.Collections.Generic.List[string]]$Errors,
        $Record,
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-TalkReleaseManifestMapValue -Value $Record)) {
        Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message 'must be an object'
        return
    }

    foreach ($name in @('scenario', 'binaryPath')) {
        Validate-TalkReleaseManifestRequiredString -Errors $Errors -Object $Record -Name $name -Path $Path
    }

    foreach ($name in @(
        'configPath',
        'primaryConfigPath',
        'secondaryConfigPath',
        'logPath',
        'status',
        'failureKind',
        'failureSummary',
        'failureEvidencePath',
        'insertTargetDiagnosticPath',
        'dialogText',
        'statusKind',
        'statusSummary',
        'beforeReloadDialogText',
        'beforeReloadStatusKind',
        'beforeReloadStatusSummary',
        'afterReloadDialogText',
        'afterReloadStatusKind',
        'afterReloadStatusSummary'
    )) {
        Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $Record -Name $name -Path $Path
    }

    Validate-TalkReleaseManifestOptionalInteger -Errors $Errors -Object $Record -Name 'retryCount' -Path $Path
    Validate-TalkReleaseManifestOptionalString -Errors $Errors -Object $Record -Name 'retryReason' -Path $Path

    $statusFields = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'statusFields'
    $statusSnapshot = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'statusSnapshot'
    $beforeReloadStatusFields = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'beforeReloadStatusFields'
    $beforeReloadStatusSnapshot = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'beforeReloadStatusSnapshot'
    $afterReloadStatusFields = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'afterReloadStatusFields'
    $afterReloadStatusSnapshot = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'afterReloadStatusSnapshot'

    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $statusFields -Path ($Path + '.statusFields')
    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $statusSnapshot -Path ($Path + '.statusSnapshot')
    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $beforeReloadStatusFields -Path ($Path + '.beforeReloadStatusFields')
    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $beforeReloadStatusSnapshot -Path ($Path + '.beforeReloadStatusSnapshot')
    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $afterReloadStatusFields -Path ($Path + '.afterReloadStatusFields')
    Validate-TalkReleaseManifestStringMap -Errors $Errors -Value $afterReloadStatusSnapshot -Path ($Path + '.afterReloadStatusSnapshot')

    $dialogText = Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'dialogText'
    if ($null -ne $dialogText) {
        foreach ($pair in @(
            @{ Name = 'statusKind'; Value = (Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'statusKind') }
            @{ Name = 'statusSummary'; Value = (Get-TalkReleaseManifestPropertyValue -Object $Record -Name 'statusSummary') }
        )) {
            if (-not (Test-TalkReleaseManifestNonEmptyString -Value $pair.Value)) {
                Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message ("property '{0}' must be present when dialogText is present" -f $pair.Name)
            }
        }
        if ($null -eq $statusFields) {
            Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property 'statusFields' must be present when dialogText is present"
        }
        if ($null -eq $statusSnapshot) {
            Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message "property 'statusSnapshot' must be present when dialogText is present"
        }
    }

    foreach ($prefix in @('beforeReload', 'afterReload')) {
        $dialog = Get-TalkReleaseManifestPropertyValue -Object $Record -Name ($prefix + 'DialogText')
        $kind = Get-TalkReleaseManifestPropertyValue -Object $Record -Name ($prefix + 'StatusKind')
        $summary = Get-TalkReleaseManifestPropertyValue -Object $Record -Name ($prefix + 'StatusSummary')
        $fields = Get-TalkReleaseManifestPropertyValue -Object $Record -Name ($prefix + 'StatusFields')
        $snapshot = Get-TalkReleaseManifestPropertyValue -Object $Record -Name ($prefix + 'StatusSnapshot')

        if ($null -ne $dialog) {
            if (-not (Test-TalkReleaseManifestNonEmptyString -Value $kind)) {
                Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message ("property '{0}StatusKind' must be present when {0}DialogText is present" -f $prefix)
            }
            if (-not (Test-TalkReleaseManifestNonEmptyString -Value $summary)) {
                Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message ("property '{0}StatusSummary' must be present when {0}DialogText is present" -f $prefix)
            }
            if ($null -eq $fields) {
                Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message ("property '{0}StatusFields' must be present when {0}DialogText is present" -f $prefix)
            }
            if ($null -eq $snapshot) {
                Add-TalkReleaseManifestValidationError -Errors $Errors -Path $Path -Message ("property '{0}StatusSnapshot' must be present when {0}DialogText is present" -f $prefix)
            }
        }
    }
}

function Get-TalkReleaseManifestValidationErrors {
    param([Parameter(Mandatory = $true)]$Manifest)

    $errors = New-Object System.Collections.Generic.List[string]
    if (-not (Test-TalkReleaseManifestMapValue -Value $Manifest)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$' -Message 'manifest must be an object'
        return $errors.ToArray()
    }

    if (-not (Test-TalkReleaseManifestHasProperty -Object $Manifest -Name 'schemaVersion')) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$' -Message "missing required integer property 'schemaVersion'"
    } else {
        $schemaVersion = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'schemaVersion'
        if (-not ($schemaVersion -is [int] -or $schemaVersion -is [long])) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.schemaVersion' -Message 'must be an integer'
        } elseif ($schemaVersion -ne 2) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.schemaVersion' -Message 'must equal 2'
        }
    }

    foreach ($name in @(
        'app',
        'sourceProject',
        'versionId',
        'builtAt',
        'profile',
        'target',
        'repoRoot',
        'releaseRoot',
        'destination',
        'checksums'
    )) {
        Validate-TalkReleaseManifestRequiredString -Errors $errors -Object $Manifest -Name $name -Path '$'
    }

    $app = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'app'
    if ($app -is [string] -and $app -ne 'Talk') {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.app' -Message "must equal 'Talk'"
    }
    $sourceProject = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'sourceProject'
    if ($sourceProject -is [string] -and $sourceProject -ne 'Talk') {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.sourceProject' -Message "must equal 'Talk'"
    }

    $commands = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'commands'
    if (-not (Test-TalkReleaseManifestCollectionValue -Value $commands)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.commands' -Message 'must be an array'
    } else {
        $commandItems = @(Get-TalkReleaseManifestCollectionItems -Value $commands)
        for ($index = 0; $index -lt $commandItems.Count; $index++) {
            Validate-TalkReleaseManifestCommandRecord -Errors $errors -CommandRecord $commandItems[$index] -Path ('$.commands[{0}]' -f $index)
        }
    }

    $exes = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'exes'
    if (-not (Test-TalkReleaseManifestCollectionValue -Value $exes)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.exes' -Message 'must be an array'
    } else {
        $exeItems = @(Get-TalkReleaseManifestCollectionItems -Value $exes)
        if ($exeItems.Count -eq 0) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.exes' -Message 'must contain at least one executable record'
        }
        for ($index = 0; $index -lt $exeItems.Count; $index++) {
            Validate-TalkReleaseManifestFileRecord -Errors $errors -Record $exeItems[$index] -Path ('$.exes[{0}]' -f $index)
        }

        $exeNames = @($exeItems | ForEach-Object { Get-TalkReleaseManifestPropertyValue -Object $_ -Name 'name' })
        foreach ($requiredExeName in @('talk-desktop.exe')) {
            if ($exeNames -notcontains $requiredExeName) {
                Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.exes' -Message ("must include executable '{0}'" -f $requiredExeName)
            }
        }
    }

    foreach ($arrayName in @('supportFiles', 'buildLogs', 'nativePreflight', 'artifacts')) {
        $value = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name $arrayName
        if (-not (Test-TalkReleaseManifestCollectionValue -Value $value)) {
            Add-TalkReleaseManifestValidationError -Errors $errors -Path ('$.{0}' -f $arrayName) -Message 'must be an array'
        }
    }

    $supportFiles = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'supportFiles'
    if (Test-TalkReleaseManifestCollectionValue -Value $supportFiles) {
        $supportFileItems = @(Get-TalkReleaseManifestCollectionItems -Value $supportFiles)
        for ($index = 0; $index -lt $supportFileItems.Count; $index++) {
            Validate-TalkReleaseManifestSupportFileRecord -Errors $errors -Record $supportFileItems[$index] -Path ('$.supportFiles[{0}]' -f $index)
        }
    }

    $buildInfo = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'buildInfo'
    Validate-TalkReleaseManifestBuildInfo -Errors $errors -BuildInfo $buildInfo -Path '$.buildInfo'

    $buildLogs = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'buildLogs'
    if (Test-TalkReleaseManifestCollectionValue -Value $buildLogs) {
        $buildLogItems = @(Get-TalkReleaseManifestCollectionItems -Value $buildLogs)
        for ($index = 0; $index -lt $buildLogItems.Count; $index++) {
            Validate-TalkReleaseManifestBuildLogRecord -Errors $errors -Record $buildLogItems[$index] -Path ('$.buildLogs[{0}]' -f $index)
        }
    }

    $nativePreflight = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'nativePreflight'
    if (Test-TalkReleaseManifestCollectionValue -Value $nativePreflight) {
        $nativePreflightItems = @(Get-TalkReleaseManifestCollectionItems -Value $nativePreflight)
        for ($index = 0; $index -lt $nativePreflightItems.Count; $index++) {
            Validate-TalkReleaseManifestNativePreflightRecord `
                -Errors $errors `
                -Record $nativePreflightItems[$index] `
                -Path ('$.nativePreflight[{0}]' -f $index)
        }
    }

    $desktopSmoke = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'desktopSmoke'
    if (-not (Test-TalkReleaseManifestCollectionValue -Value $desktopSmoke)) {
        Add-TalkReleaseManifestValidationError -Errors $errors -Path '$.desktopSmoke' -Message 'must be an array or null'
    } elseif ($null -ne $desktopSmoke) {
        $desktopSmokeItems = @(Get-TalkReleaseManifestCollectionItems -Value $desktopSmoke)
        for ($index = 0; $index -lt $desktopSmokeItems.Count; $index++) {
            Validate-TalkReleaseManifestDesktopSmokeRecord `
                -Errors $errors `
                -Record $desktopSmokeItems[$index] `
                -Path ('$.desktopSmoke[{0}]' -f $index)
        }
    }

    $nativeReadiness = Get-TalkReleaseManifestPropertyValue -Object $Manifest -Name 'nativeReadiness'
    Validate-TalkReleaseManifestNativeReadiness -Errors $errors -NativeReadiness $nativeReadiness -Path '$.nativeReadiness'

    $errors.ToArray()
}

function Assert-TalkReleaseManifestObject {
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [string]$Context = 'Talk release manifest'
    )

    $errors = @(Get-TalkReleaseManifestValidationErrors -Manifest $Manifest)
    if ($errors.Count -eq 0) {
        return
    }

    throw ("{0} is invalid:`n - {1}" -f $Context, ($errors -join "`n - "))
}

function Read-TalkReleaseManifest {
    param([Parameter(Mandatory = $true)][string]$ManifestPath)

    $resolvedPath = [System.IO.Path]::GetFullPath($ManifestPath)
    if (-not (Test-Path -LiteralPath $resolvedPath)) {
        throw "Talk release manifest does not exist: $resolvedPath"
    }

    Get-Content -LiteralPath $resolvedPath -Raw | ConvertFrom-Json
}

if ($MyInvocation.InvocationName -ne '.') {
    $manifest = Read-TalkReleaseManifest -ManifestPath $ManifestPath
    Assert-TalkReleaseManifestObject -Manifest $manifest -Context $ManifestPath
    [pscustomobject]@{
        ManifestPath = [System.IO.Path]::GetFullPath($ManifestPath)
        SchemaVersion = $manifest.schemaVersion
        DesktopSmokeCount = @($manifest.desktopSmoke).Count
        NativePreflightCount = @($manifest.nativePreflight).Count
    }
}
