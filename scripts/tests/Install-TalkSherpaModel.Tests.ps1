$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Install-TalkSherpaModel.ps1'

. $scriptPath

Describe 'Install-TalkSherpaModel helpers' {
    It 'catalogs the product multilingual Zipformer as recommended while retaining the legacy model' {
        $modelId = 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10'
        $archiveName = "$modelId.tar.bz2"
        $catalog = Get-TalkSherpaModelCatalog
        $model = $catalog | Where-Object { $_.Id -eq $modelId } | Select-Object -First 1
        $legacyModel = $catalog | Where-Object { $_.Id -eq 'zipformer-zh-en-punct-int8-480ms' } | Select-Object -First 1

        $model | Should Not Be $null
        $model.Recommended | Should Be $true
        $model.Family | Should Be 'transducer'
        $model.ModelName | Should Be $modelId
        $model.ArchiveName | Should Be $archiveName
        $model.ArchiveUrl | Should Be "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/$archiveName"
        $model.Sha256 | Should Be '28044b67324f7f831689f0a3761473dd2ade380e93aa53f1dbcd479ef71c40d4'
        $legacyModel | Should Not Be $null
        $legacyModel.Recommended | Should Be $false
        @($catalog | Where-Object { $_.Recommended }).Count | Should Be 1
    }

    It 'defaults every installer entry point to the product multilingual model' {
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
                Should Be 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10'
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
