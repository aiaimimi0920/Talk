[CmdletBinding()]
param(
    [string]$SelectionJson,
    [string]$ConfigPath,
    [string]$ModelRoot,
    [string]$InstallScriptPath,
    [switch]$NoBackup,
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$entrySelectionJson = $SelectionJson
$entryConfigPath = $ConfigPath
$entryModelRoot = $ModelRoot
$entryInstallScriptPath = $InstallScriptPath
$entryNoBackup = [bool]$NoBackup
$entryPassThru = [bool]$PassThru

function Get-TalkDefaultModelJsonProperty {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Context
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        throw "$Context is missing required property [$Name]"
    }

    $property.Value
}

function Resolve-TalkDefaultModelBaseDir {
    $scriptLeaf = Split-Path -Leaf $PSScriptRoot
    if ($scriptLeaf -eq 'scripts') {
        return Split-Path -Parent $PSScriptRoot
    }

    $PSScriptRoot
}

function Resolve-TalkDefaultModelConfigPath {
    param([string]$ConfigPath)

    if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
        return [System.IO.Path]::GetFullPath($ConfigPath)
    }

    $baseDir = Resolve-TalkDefaultModelBaseDir
    $releaseConfigPath = Join-Path $baseDir 'talk-desktop.toml'
    if (Test-Path -LiteralPath $releaseConfigPath -PathType Leaf) {
        return [System.IO.Path]::GetFullPath($releaseConfigPath)
    }

    [System.IO.Path]::GetFullPath((Join-Path $baseDir 'examples\desktop-streaming-service-speculative-config.toml'))
}

function Resolve-TalkDefaultModelRoot {
    param([string]$ModelRoot)

    if (-not [string]::IsNullOrWhiteSpace($ModelRoot)) {
        return [System.IO.Path]::GetFullPath($ModelRoot)
    }

    [System.IO.Path]::GetFullPath((Join-Path (Resolve-TalkDefaultModelBaseDir) '.runtime\models\sherpa-onnx'))
}

function Resolve-TalkDefaultModelInstallScriptPath {
    param([string]$InstallScriptPath)

    if (-not [string]::IsNullOrWhiteSpace($InstallScriptPath)) {
        return [System.IO.Path]::GetFullPath($InstallScriptPath)
    }

    $baseDir = Resolve-TalkDefaultModelBaseDir
    $releaseScriptPath = Join-Path $baseDir 'Install-TalkSherpaModel.ps1'
    if (Test-Path -LiteralPath $releaseScriptPath -PathType Leaf) {
        return [System.IO.Path]::GetFullPath($releaseScriptPath)
    }

    [System.IO.Path]::GetFullPath((Join-Path $baseDir 'scripts\Install-TalkSherpaModel.ps1'))
}

function Read-TalkDefaultModelSelection {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$SelectionJson)

    $resolvedSelectionJson = [System.IO.Path]::GetFullPath($SelectionJson)
    if (-not (Test-Path -LiteralPath $resolvedSelectionJson -PathType Leaf)) {
        throw "Talk default ASR selection json does not exist: $resolvedSelectionJson"
    }

    $selection = Get-Content -LiteralPath $resolvedSelectionJson -Raw -Encoding UTF8 | ConvertFrom-Json
    $kind = [string](Get-TalkDefaultModelJsonProperty -Object $selection -Name 'kind' -Context $resolvedSelectionJson)
    if ($kind -ne 'talk-default-asr-model-selection') {
        throw "Talk default ASR selection has unexpected kind [$kind]: $resolvedSelectionJson"
    }

    $evidenceReady = [bool](Get-TalkDefaultModelJsonProperty -Object $selection -Name 'evidenceReady' -Context $resolvedSelectionJson)
    if (-not $evidenceReady) {
        throw "Talk default ASR selection evidenceReady must be true before applying a default model: $resolvedSelectionJson"
    }

    $selectedModelId = [string](Get-TalkDefaultModelJsonProperty -Object $selection -Name 'selectedModelId' -Context $resolvedSelectionJson)
    if ([string]::IsNullOrWhiteSpace($selectedModelId)) {
        throw "Talk default ASR selection selectedModelId must not be blank: $resolvedSelectionJson"
    }
    if ($selectedModelId.Trim() -ne $selectedModelId) {
        throw "Talk default ASR selection selectedModelId must not have leading or trailing whitespace: $resolvedSelectionJson"
    }
    if ($selectedModelId -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') {
        throw "Talk default ASR selection selectedModelId [$selectedModelId] contains unsupported characters"
    }

    [pscustomobject]@{
        Path = $resolvedSelectionJson
        SelectedModelId = $selectedModelId
        SelectedEngine = [string](Get-TalkDefaultModelJsonProperty -Object $selection -Name 'selectedEngine' -Context $resolvedSelectionJson)
        GlobalSelectedEngine = [string](Get-TalkDefaultModelJsonProperty -Object $selection -Name 'globalSelectedEngine' -Context $resolvedSelectionJson)
    }
}

function Get-TalkDefaultModelPreferredNewLine {
    param([Parameter(Mandatory = $true)][string]$Text)

    if ($Text.Contains("`r`n")) {
        return "`r`n"
    }
    if ($Text.Contains("`n")) {
        return "`n"
    }
    if ($Text.Contains("`r")) {
        return "`r"
    }

    [Environment]::NewLine
}

function Convert-TalkDefaultModelNewLines {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$NewLine
    )

    ($Text -replace "`r`n|`n|`r", $NewLine)
}

function Test-TalkDefaultModelSherpaInstall {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$InstallScriptPath,
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][string]$ModelDir
    )

    $resolvedInstallScriptPath = [System.IO.Path]::GetFullPath($InstallScriptPath)
    if (-not (Test-Path -LiteralPath $resolvedInstallScriptPath -PathType Leaf)) {
        throw "Talk sherpa installer script does not exist: $resolvedInstallScriptPath"
    }

    . $resolvedInstallScriptPath
    if (-not (Get-Command Test-TalkSherpaModelInstall -ErrorAction SilentlyContinue)) {
        throw "Talk sherpa installer did not expose Test-TalkSherpaModelInstall: $resolvedInstallScriptPath"
    }

    $validation = Test-TalkSherpaModelInstall -ModelId $ModelId -ModelDir $ModelDir
    $validation | Add-Member -NotePropertyName InstallScriptPath -NotePropertyValue $resolvedInstallScriptPath -Force
    $validation
}

function Set-TalkDefaultModelLocalDaemonBlock {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigText,
        [Parameter(Mandatory = $true)][string]$ConfigSnippet,
        [Parameter(Mandatory = $true)][string]$SelectedModelId,
        [Parameter(Mandatory = $true)][string]$SelectionJson
    )

    $newLine = Get-TalkDefaultModelPreferredNewLine -Text $ConfigText
    $normalizedSnippet = Convert-TalkDefaultModelNewLines -Text $ConfigSnippet.Trim() -NewLine $newLine
    $replacement = @(
        '# Talk evidence-selected default local ASR model.'
        "# selected_model_id = `"$SelectedModelId`""
        "# selection_json = `"$SelectionJson`""
        $normalizedSnippet
    ) -join $newLine
    $replacement = $replacement + $newLine

    $activeBlockPattern = '(?ms)^[ \t]*\[speculative\.streaming_service\.local_daemon\][\s\S]*?(?=^[ \t]*\[[^\r\n]+\]|\z)'
    if ([regex]::IsMatch($ConfigText, $activeBlockPattern)) {
        return [regex]::Replace(
            $ConfigText,
            $activeBlockPattern,
            [System.Text.RegularExpressions.MatchEvaluator] { param($match) $replacement }
        )
    }

    $separator = if ($ConfigText.EndsWith("`r`n") -or $ConfigText.EndsWith("`n")) {
        $newLine
    } else {
        $newLine + $newLine
    }

    $ConfigText.TrimEnd("`r", "`n") + $separator + $replacement
}

function Write-TalkDefaultModelUtf8NoBomText {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $utf8NoBom)
}

function Set-TalkDefaultAsrModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$SelectionJson,
        [string]$ConfigPath,
        [string]$ModelRoot,
        [string]$InstallScriptPath,
        [switch]$NoBackup,
        [switch]$PassThru
    )

    $selection = Read-TalkDefaultModelSelection -SelectionJson $SelectionJson
    $resolvedConfigPath = Resolve-TalkDefaultModelConfigPath -ConfigPath $ConfigPath
    if (-not (Test-Path -LiteralPath $resolvedConfigPath -PathType Leaf)) {
        throw "Talk desktop config does not exist: $resolvedConfigPath"
    }
    $resolvedModelRoot = Resolve-TalkDefaultModelRoot -ModelRoot $ModelRoot
    $modelDir = Join-Path $resolvedModelRoot $selection.SelectedModelId
    $validation = Test-TalkDefaultModelSherpaInstall `
        -InstallScriptPath (Resolve-TalkDefaultModelInstallScriptPath -InstallScriptPath $InstallScriptPath) `
        -ModelId $selection.SelectedModelId `
        -ModelDir $modelDir

    $configText = Get-Content -LiteralPath $resolvedConfigPath -Raw -Encoding UTF8
    $updatedConfigText = Set-TalkDefaultModelLocalDaemonBlock `
        -ConfigText $configText `
        -ConfigSnippet ([string]$validation.ConfigSnippet) `
        -SelectedModelId $selection.SelectedModelId `
        -SelectionJson $selection.Path
    if (-not ($updatedConfigText.EndsWith("`r`n") -or $updatedConfigText.EndsWith("`n") -or $updatedConfigText.EndsWith("`r"))) {
        $updatedConfigText += Get-TalkDefaultModelPreferredNewLine -Text $updatedConfigText
    }

    $backupPath = $null
    if (-not $NoBackup) {
        $backupPath = $resolvedConfigPath + '.bak'
        Copy-Item -LiteralPath $resolvedConfigPath -Destination $backupPath -Force
    }
    Write-TalkDefaultModelUtf8NoBomText -Path $resolvedConfigPath -Content $updatedConfigText

    $result = [pscustomobject]@{
        Applied = $true
        SelectionJson = $selection.Path
        SelectedModelId = $selection.SelectedModelId
        SelectedEngine = $selection.SelectedEngine
        GlobalSelectedEngine = $selection.GlobalSelectedEngine
        ConfigPath = $resolvedConfigPath
        BackupPath = $backupPath
        ModelRoot = $resolvedModelRoot
        ModelDir = [System.IO.Path]::GetFullPath($validation.ModelDir)
        InstallScriptPath = [string]$validation.InstallScriptPath
    }

    if ($PassThru) {
        return $result
    }

    $result
}

if ($MyInvocation.InvocationName -ne '.') {
    Set-TalkDefaultAsrModel `
        -SelectionJson $entrySelectionJson `
        -ConfigPath $entryConfigPath `
        -ModelRoot $entryModelRoot `
        -InstallScriptPath $entryInstallScriptPath `
        -NoBackup:$entryNoBackup `
        -PassThru:$entryPassThru
}
