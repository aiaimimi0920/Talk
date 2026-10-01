$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Install-TalkSherpaModel.ps1'

. $scriptPath

Describe 'Install-TalkSherpaModel helpers' {
    It 'catalogs the evidence-selected zh-en punctuation Zipformer as recommended' {
        $modelId = 'zipformer-zh-en-punct-int8-480ms'
        $archiveName = 'sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2'
        $catalog = Get-TalkSherpaModelCatalog
        $model = $catalog | Where-Object { $_.Id -eq $modelId } | Select-Object -First 1
        $multilingualModel = $catalog | Where-Object { $_.Id -eq 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' } | Select-Object -First 1

        $model | Should Not Be $null
        $model.Recommended | Should Be $true
        $model.Family | Should Be 'transducer'
        $model.ModelName | Should Be 'x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8'
        $model.ArchiveName | Should Be $archiveName
        $model.ArchiveUrl | Should Be "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/$archiveName"
        $model.Sha256 | Should Be 'fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74'
        $multilingualModel | Should Not Be $null
        $multilingualModel.Recommended | Should Be $false
        @($catalog | Where-Object { $_.Recommended }).Count | Should Be 1
    }

    It 'defaults every installer entry point to the evidence-selected zh-en punctuation model' {
        $tokens = $null
        $parseErrors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            $scriptPath,
            [ref]$tokens,
            [ref]$parseErrors)
        $defaults = @($ast.FindAll({
                    param($node)
                    $node -is [System.Management.Automation.Language.ParameterAst] -and
                    $node.Name.VariablePath.UserPath -eq 'ModelId' -and
                    $null -ne $node.DefaultValue
                }, $true))

        $parseErrors.Count | Should Be 0
        $defaults.Count | Should Be 2
        foreach ($default in $defaults) {
            [string]$default.DefaultValue.SafeGetValue() |
                Should Be 'zipformer-zh-en-punct-int8-480ms'
        }
    }

    It 'catalogs the offline bilingual Zipformer without a streaming config snippet' {
        $modelId = 'offline-zipformer-zh-en-int8-2023-11-22'
        $model = Get-TalkSherpaModelCatalog |
            Where-Object { $_.Id -eq $modelId } |
            Select-Object -First 1

        $model | Should Not Be $null
        $model.RuntimeMode | Should Be 'offline'
        $model.Family | Should Be 'transducer'
        $model.ModelName | Should Be 'sherpa-onnx-zipformer-zh-en-2023-11-22'
        $model.SizeBytes | Should Be 312398028

        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-offline-model-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $tempRoot 'tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'encoder.int8.onnx') -Value 'encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'decoder.onnx') -Value 'decoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'joiner.int8.onnx') -Value 'joiner' -Encoding ASCII

            $validation = Test-TalkSherpaModelInstall -ModelId $modelId -ModelDir $tempRoot

            $validation.RuntimeMode | Should Be 'offline'
            $validation.ConfigSnippet | Should Be ''
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'catalogs and validates the official offline SenseVoice int8 model' {
        $modelId = 'offline-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09'
        $model = Get-TalkSherpaModelCatalog |
            Where-Object { $_.Id -eq $modelId } |
            Select-Object -First 1

        $model | Should Not Be $null
        $model.RuntimeMode | Should Be 'offline'
        $model.Family | Should Be 'sense-voice'
        $model.SizeBytes | Should Be 165783878
        $model.Sha256 | Should Be '7305f7905bfcf77fa0b39388a313f3da35c68d971661a65475b56fb2162c8e63'

        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-sense-voice-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $tempRoot 'tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'model.int8.onnx') -Value 'model' -Encoding ASCII

            $validation = Test-TalkSherpaModelInstall -ModelId $modelId -ModelDir $tempRoot

            $validation.ModelFamily | Should Be 'sense-voice'
            $validation.ModelPath | Should Be (Join-Path $tempRoot 'model.int8.onnx')
            $validation.EncoderPath | Should Be ''
            $validation.DecoderPath | Should Be ''
            $validation.JoinerPath | Should Be ''
            $validation.ConfigSnippet | Should Be ''
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'catalogs and validates the offline multilingual Whisper base int8 model' {
        $modelId = 'offline-whisper-base-int8'
        $catalog = Get-TalkSherpaModelCatalog
        $model = $catalog |
            Where-Object { $_.Id -eq $modelId } |
            Select-Object -First 1
        $smallModel = $catalog |
            Where-Object { $_.Id -eq 'offline-whisper-small-int8' } |
            Select-Object -First 1

        $model | Should Not Be $null
        $model.RuntimeMode | Should Be 'offline'
        $model.Family | Should Be 'whisper'
        $model.SizeBytes | Should Be 207557382
        $model.ArchiveName | Should Be 'sherpa-onnx-whisper-base.tar.bz2'
        $smallModel | Should Not Be $null
        $smallModel.Family | Should Be 'whisper'
        $smallModel.SizeBytes | Should Be 639387718
        $smallModel.ArchiveName | Should Be 'sherpa-onnx-whisper-small.tar.bz2'

        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-whisper-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $tempRoot 'base-tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'base-encoder.int8.onnx') -Value 'encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'base-decoder.int8.onnx') -Value 'decoder' -Encoding ASCII

            $validation = Test-TalkSherpaModelInstall -ModelId $modelId -ModelDir $tempRoot

            $validation.ModelFamily | Should Be 'whisper'
            $validation.TokensPath | Should Be (Join-Path $tempRoot 'base-tokens.txt')
            $validation.EncoderPath | Should Be (Join-Path $tempRoot 'base-encoder.int8.onnx')
            $validation.DecoderPath | Should Be (Join-Path $tempRoot 'base-decoder.int8.onnx')
            $validation.JoinerPath | Should Be ''
            $validation.ModelPath | Should Be ''
            $validation.ConfigSnippet | Should Be ''
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'validates an extracted transducer model directory and emits a desktop config snippet' {
        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-model-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelDir = Join-Path $tempRoot 'model'
            New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'encoder-epoch-99-avg-1.int8.onnx') -Value 'encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'decoder-epoch-99-avg-1.onnx') -Value 'decoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'joiner-epoch-99-avg-1.int8.onnx') -Value 'joiner' -Encoding ASCII

            $validation = Test-TalkSherpaModelInstall `
                -ModelId 'zipformer-zh-en-punct-int8-480ms' `
                -ModelDir $modelDir

            $validation.Valid | Should Be $true
            $validation.ModelFamily | Should Be 'transducer'
            $validation.TokensPath | Should Be (Join-Path $modelDir 'tokens.txt')
            $validation.EncoderPath | Should Be (Join-Path $modelDir 'encoder-epoch-99-avg-1.int8.onnx')
            $validation.DecoderPath | Should Be (Join-Path $modelDir 'decoder-epoch-99-avg-1.onnx')
            $validation.JoinerPath | Should Be (Join-Path $modelDir 'joiner-epoch-99-avg-1.int8.onnx')
            $validation.ConfigSnippet | Should Match '\[speculative\.streaming_service\.local_daemon\]'
            $validation.ConfigSnippet | Should Match 'mode = "sherpa-online"'
            $validation.ConfigSnippet | Should Match 'model_family = "transducer"'
            $validation.ConfigSnippet | Should Match 'joiner = "'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a transducer model directory without a joiner file' {
        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-model-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelDir = Join-Path $tempRoot 'model'
            New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'encoder.onnx') -Value 'encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'decoder.onnx') -Value 'decoder' -Encoding ASCII

            {
                Test-TalkSherpaModelInstall `
                    -ModelId 'zipformer-zh-en-punct-int8-480ms' `
                    -ModelDir $modelDir
            } | Should Throw 'missing required joiner'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'validates a Paraformer model directory without requiring a joiner file' {
        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-model-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $modelDir = Join-Path $tempRoot 'model'
            New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Value '<blk>' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'encoder.onnx') -Value 'encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'decoder.onnx') -Value 'decoder' -Encoding ASCII

            $validation = Test-TalkSherpaModelInstall `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ModelDir $modelDir

            $validation.Valid | Should Be $true
            $validation.ModelFamily | Should Be 'paraformer'
            $validation.JoinerPath | Should Be ''
            $validation.ConfigSnippet | Should Match 'model_family = "paraformer"'
            $validation.ConfigSnippet | Should Not Match 'joiner = "'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'replaces an existing model directory when Force is passed with an archive' {
        $tempRoot = Join-Path $env:TEMP ('talk-sherpa-model-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $destinationRoot = Join-Path $tempRoot 'models'
            $modelDir = Join-Path $destinationRoot 'zipformer-zh-en-punct-int8-480ms'
            New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Value 'old-tokens' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'encoder.onnx') -Value 'old-encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'decoder.onnx') -Value 'old-decoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $modelDir 'joiner.onnx') -Value 'old-joiner' -Encoding ASCII

            $archiveSourceRoot = Join-Path $tempRoot 'archive-source'
            $archiveModelDir = Join-Path $archiveSourceRoot 'zipformer-zh-en-punct-int8-480ms'
            New-Item -ItemType Directory -Path $archiveModelDir -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $archiveModelDir 'tokens.txt') -Value 'new-tokens' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $archiveModelDir 'encoder-epoch-99-avg-1.int8.onnx') -Value 'new-encoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $archiveModelDir 'decoder-epoch-99-avg-1.onnx') -Value 'new-decoder' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $archiveModelDir 'joiner-epoch-99-avg-1.int8.onnx') -Value 'new-joiner' -Encoding ASCII

            $archivePath = Join-Path $tempRoot 'model.tar.bz2'
            & tar.exe -cjf $archivePath -C $archiveSourceRoot 'zipformer-zh-en-punct-int8-480ms'
            if ($LASTEXITCODE -ne 0) {
                throw "tar.exe failed to create test archive with exit code $LASTEXITCODE"
            }

            $validation = Install-TalkSherpaModel `
                -ModelId 'zipformer-zh-en-punct-int8-480ms' `
                -DestinationRoot $destinationRoot `
                -ArchivePath $archivePath `
                -SkipDownload `
                -Force `
                -PassThru

            $validation.Valid | Should Be $true
            (Get-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Raw).Trim() | Should Be 'new-tokens'
            Test-Path -LiteralPath (Join-Path $modelDir 'encoder-epoch-99-avg-1.int8.onnx') | Should Be $true
            Test-Path -LiteralPath (Join-Path $modelDir 'talk-local-daemon.toml.snippet') | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
