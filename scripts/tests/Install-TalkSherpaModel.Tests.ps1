$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Install-TalkSherpaModel.ps1'

. $scriptPath

Describe 'Install-TalkSherpaModel helpers' {
    It 'includes a current recommended streaming Zipformer transducer model' {
        $catalog = Get-TalkSherpaModelCatalog
        $model = $catalog | Where-Object { $_.Id -eq 'zipformer-zh-en-punct-int8-480ms' } | Select-Object -First 1

        $model | Should Not Be $null
        $model.Recommended | Should Be $true
        $model.Family | Should Be 'transducer'
        $model.ModelName | Should Be 'x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8'
        $model.ArchiveUrl | Should Be 'https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2'
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
