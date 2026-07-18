$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Set-TalkDefaultAsrModel.ps1'
$installScriptPath = Join-Path $scriptRoot 'Install-TalkSherpaModel.ps1'

. $scriptPath

function New-TestTalkSelectedDefaultAsrModelJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
        [bool]$EvidenceReady = $true
    )

    $selection = [ordered]@{
        schemaVersion = 1
        kind = 'talk-default-asr-model-selection'
        evidenceReady = $EvidenceReady
        comparisonJson = 'C:\reports\asr-model-comparison.json'
        outputJson = $Path
        minSamples = 3
        requiredLocalModelIds = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en')
        globalSelectedEngine = 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest'
        selectedModelId = $ModelId
        selectedEngine = 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8'
        cloudBaselinePresent = $true
        cloudBaselineEngines = @('cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest')
        rankedLocalCandidates = @()
    }
    $selection | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function New-TestTalkSherpaModelFiles {
    param(
        [Parameter(Mandatory = $true)][string]$ModelRoot,
        [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms'
    )

    $modelDir = Join-Path $ModelRoot $ModelId
    New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
    foreach ($name in @(
        'tokens.txt',
        'encoder-epoch-99-avg-1.int8.onnx',
        'decoder-epoch-99-avg-1.onnx',
        'joiner-epoch-99-avg-1.int8.onnx'
    )) {
        Set-Content -LiteralPath (Join-Path $modelDir $name) -Value 'fixture' -Encoding UTF8
    }
    $modelDir
}

Describe 'Set-TalkDefaultAsrModel' {
    It 'replaces the active local daemon block with the evidence-selected installed model snippet' {
        $tempRoot = Join-Path $env:TEMP ('talk-set-default-asr-model-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelRoot = Join-Path $tempRoot 'models'
            $modelDir = New-TestTalkSherpaModelFiles -ModelRoot $modelRoot
            $selectionPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            New-TestTalkSelectedDefaultAsrModelJson -Path $selectionPath

            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[speculative]
enabled = true
local_asr = "streaming_service"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1

[speculative.streaming_service.local_daemon]
mode = "sherpa-online"
model_family = "transducer"
model = "old-model"
tokens = "C:/old/tokens.txt"
encoder = "C:/old/encoder.onnx"
decoder = "C:/old/decoder.onnx"
joiner = "C:/old/joiner.onnx"

[output]
mode = "clipboard_paste"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8

            $result = Set-TalkDefaultAsrModel `
                -SelectionJson $selectionPath `
                -ConfigPath $configPath `
                -ModelRoot $modelRoot `
                -InstallScriptPath $installScriptPath `
                -PassThru

            $result.SelectedModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $result.ModelDir | Should Be ([System.IO.Path]::GetFullPath($modelDir))
            $result.BackupPath | Should Not BeNullOrEmpty
            Test-Path -LiteralPath $result.BackupPath | Should Be $true

            $updated = Get-Content -LiteralPath $configPath -Raw
            $updated | Should Match 'model = "x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8"'
            $updated | Should Match 'tokens = ".*tokens\.txt"'
            $updated | Should Not Match 'old-model'
            $updated | Should Match '\[output\]\s+mode = "clipboard_paste"'
            ([regex]::Matches($updated, '\[speculative\.streaming_service\.local_daemon\]').Count) | Should Be 1
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'appends an active local daemon block when the desktop config only has comments' {
        $tempRoot = Join-Path $env:TEMP ('talk-set-default-asr-model-append-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelRoot = Join-Path $tempRoot 'models'
            New-TestTalkSherpaModelFiles -ModelRoot $modelRoot | Out-Null
            $selectionPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            New-TestTalkSelectedDefaultAsrModelJson -Path $selectionPath
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"

# [speculative.streaming_service.local_daemon]
# model = "example"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8

            Set-TalkDefaultAsrModel `
                -SelectionJson $selectionPath `
                -ConfigPath $configPath `
                -ModelRoot $modelRoot `
                -InstallScriptPath $installScriptPath `
                -NoBackup | Out-Null

            $updated = Get-Content -LiteralPath $configPath -Raw
            $updated | Should Match '# \[speculative\.streaming_service\.local_daemon\]'
            $updated | Should Match '\[speculative\.streaming_service\.local_daemon\]'
            ([regex]::Matches($updated, '(?m)^\[speculative\.streaming_service\.local_daemon\]\r?$').Count) | Should Be 1
            Test-Path -LiteralPath ($configPath + '.bak') | Should Be $false
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preserves LF-only desktop config newlines when replacing the active local daemon block' {
        $tempRoot = Join-Path $env:TEMP ('talk-set-default-asr-model-newlines-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelRoot = Join-Path $tempRoot 'models'
            New-TestTalkSherpaModelFiles -ModelRoot $modelRoot | Out-Null
            $selectionPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            New-TestTalkSelectedDefaultAsrModelJson -Path $selectionPath
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            $lfOnlyConfig = @(
                '[speculative]'
                'enabled = true'
                ''
                '[speculative.streaming_service.local_daemon]'
                'mode = "sherpa-online"'
                'model = "old-model"'
                ''
                '[output]'
                'mode = "clipboard_paste"'
                ''
            ) -join "`n"
            $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
            [System.IO.File]::WriteAllText($configPath, $lfOnlyConfig, $utf8NoBom)

            Set-TalkDefaultAsrModel `
                -SelectionJson $selectionPath `
                -ConfigPath $configPath `
                -ModelRoot $modelRoot `
                -InstallScriptPath $installScriptPath `
                -NoBackup | Out-Null

            $updated = [System.IO.File]::ReadAllText($configPath)
            $updated.Contains("`r`n") | Should Be $false
            $updated | Should Match "(?m)^\[speculative\.streaming_service\.local_daemon\]$"
            $updated | Should Match "(?m)^\[output\]$"
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a selection record that did not pass the evidence gate' {
        $tempRoot = Join-Path $env:TEMP ('talk-set-default-asr-model-weak-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $selectionPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            New-TestTalkSelectedDefaultAsrModelJson -Path $selectionPath -EvidenceReady $false
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            Set-Content -LiteralPath $configPath -Value '[speculative.streaming_service]' -Encoding UTF8

            {
                Set-TalkDefaultAsrModel `
                    -SelectionJson $selectionPath `
                    -ConfigPath $configPath `
                    -ModelRoot (Join-Path $tempRoot 'models') `
                    -InstallScriptPath $installScriptPath
            } | Should Throw 'evidenceReady'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
