$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$talkRoot = Split-Path (Split-Path $here -Parent) -Parent
$validatorPath = Join-Path $talkRoot 'scripts\Test-TalkProductRelease.ps1'
$publisherPath = Join-Path $talkRoot 'scripts\Publish-TalkRelease.ps1'

. $validatorPath
. $publisherPath

function New-TestTalkProductFixture {
    $fixtureRoot = Join-Path $env:TEMP ('talk-product-validator-' + [guid]::NewGuid().ToString('N'))
    $productRoot = Join-Path $fixtureRoot 'product'
    $sourceRoot = Join-Path $fixtureRoot 'sources'
    New-Item -ItemType Directory -Path $productRoot, $sourceRoot -Force | Out-Null

    $basePath = Join-Path $sourceRoot 'talk-desktop.exe'
    Set-Content -LiteralPath $basePath -Value 'base executable bytes' -Encoding ASCII
    $payloadNames = @(
        'talk-local-asr-sherpa.exe',
        'sherpa-onnx-c-api.dll',
        'sherpa-onnx-cxx-api.dll',
        'onnxruntime.dll',
        'onnxruntime_providers_shared.dll'
    )
    $payloadFiles = foreach ($name in $payloadNames) {
        $path = Join-Path $sourceRoot $name
        Set-Content -LiteralPath $path -Value $name -Encoding ASCII
        [pscustomobject]@{ Name = $name; Path = $path }
    }

    $embedded = New-TalkEmbeddedRuntimeExecutable `
        -BaseExecutablePath $basePath `
        -PayloadFiles $payloadFiles `
        -OutputPath (Join-Path $productRoot 'Talk.exe')
    Set-Content -LiteralPath (Join-Path $productRoot 'talk.toml') -Value 'voice_mode = "smart"' -Encoding UTF8

    [pscustomobject]@{
        Root = $fixtureRoot
        ProductPath = $productRoot
        ExecutablePath = Join-Path $productRoot 'Talk.exe'
        Builder = $embedded
    }
}

function Get-TestTalkPayloadLayout {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)

    $trailerLength = 60
    $trailerStart = $Bytes.Length - $trailerLength
    [uint64]$archiveLength = [System.BitConverter]::ToUInt64($Bytes, $trailerStart + 12)
    [uint64]$manifestLength = [System.BitConverter]::ToUInt64($Bytes, $trailerStart + 20)
    [int]$archiveStart = $trailerStart - [int]$archiveLength - [int]$manifestLength
    [pscustomobject]@{
        TrailerStart = $trailerStart
        ArchiveStart = $archiveStart
        ArchiveLength = [int]$archiveLength
        ManifestStart = $archiveStart + [int]$archiveLength
        ManifestLength = [int]$manifestLength
    }
}

function Set-TestTalkPayloadManifest {
    param(
        [Parameter(Mandatory = $true)][string]$ExecutablePath,
        [Parameter(Mandatory = $true)][string]$ManifestText
    )

    [byte[]]$bytes = [System.IO.File]::ReadAllBytes($ExecutablePath)
    $layout = Get-TestTalkPayloadLayout -Bytes $bytes
    [byte[]]$prefix = $bytes[0..($layout.ManifestStart - 1)]
    [byte[]]$trailer = $bytes[$layout.TrailerStart..($bytes.Length - 1)]
    [byte[]]$manifestBytes = [System.Text.Encoding]::UTF8.GetBytes($ManifestText)
    [byte[]]$updated = $prefix + $manifestBytes + $trailer
    $updatedTrailerStart = $updated.Length - 60
    [Array]::Copy(
        [System.BitConverter]::GetBytes([uint64]$manifestBytes.Length),
        0,
        $updated,
        $updatedTrailerStart + 20,
        8
    )
    [System.IO.File]::WriteAllBytes($ExecutablePath, $updated)
}

Describe 'Talk product release validator' {
    It 'accepts a valid five-member payload and reports its integrity metadata' {
        $fixture = New-TestTalkProductFixture
        try {
            $result = Test-TalkProductRelease -ProductPath $fixture.ProductPath

            $result.PayloadTrailer | Should Be 'TLPAY001'
            $result.PayloadVersion | Should Be 1
            $result.Files | Should Be @('Talk.exe', 'talk.toml')
            $result.EmbeddedRuntimeSha256 | Should Be $fixture.Builder.ArchiveSha256
            $result.PayloadFiles | Should Be @(
                'onnxruntime.dll',
                'onnxruntime_providers_shared.dll',
                'sherpa-onnx-c-api.dll',
                'sherpa-onnx-cxx-api.dll',
                'talk-local-asr-sherpa.exe'
            )
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a magic-only zero trailer' {
        $root = Join-Path $env:TEMP ('talk-product-validator-zero-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $root -Force | Out-Null
        try {
            [byte[]]$prefix = [System.Text.Encoding]::ASCII.GetBytes('MZ-product')
            [byte[]]$trailer = New-Object byte[] 60
            [byte[]]$magic = [System.Text.Encoding]::ASCII.GetBytes('TLPAY001')
            [Array]::Copy($magic, 0, $trailer, 0, $magic.Length)
            [System.IO.File]::WriteAllBytes((Join-Path $root 'Talk.exe'), $prefix + $trailer)
            Set-Content -LiteralPath (Join-Path $root 'talk.toml') -Value 'voice_mode = "smart"' -Encoding UTF8

            { Test-TalkProductRelease -ProductPath $root } | Should Throw
        } finally {
            Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects an unsupported payload version' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            [Array]::Copy([System.BitConverter]::GetBytes([uint32]2), 0, $bytes, $layout.TrailerStart + 8, 4)
            [System.IO.File]::WriteAllBytes($fixture.ExecutablePath, $bytes)

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects manifest property names with non-canonical casing' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            $manifest = [System.Text.Encoding]::UTF8.GetString(
                $bytes[$layout.ManifestStart..($layout.ManifestStart + $layout.ManifestLength - 1)]
            )
            Set-TestTalkPayloadManifest `
                -ExecutablePath $fixture.ExecutablePath `
                -ManifestText $manifest.Replace('"schemaVersion"', '"SchemaVersion"')

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects non-integer manifest schema versions that compare equal to one' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            $manifest = [System.Text.Encoding]::UTF8.GetString(
                $bytes[$layout.ManifestStart..($layout.ManifestStart + $layout.ManifestLength - 1)]
            )
            foreach ($value in @('"1"', 'true', '1.0')) {
                $mutated = $manifest -replace '"schemaVersion":1', ('"schemaVersion":' + $value)
                Set-TestTalkPayloadManifest `
                    -ExecutablePath $fixture.ExecutablePath `
                    -ManifestText $mutated

                { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
            }
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects extra or non-canonical manifest properties' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            $manifest = [System.Text.Encoding]::UTF8.GetString(
                $bytes[$layout.ManifestStart..($layout.ManifestStart + $layout.ManifestLength - 1)]
            )
            $mutations = @(
                $manifest.Replace('{"schemaVersion":1,', '{"schemaVersion":1,"extra":true,'),
                $manifest.Replace('"files":[{"path":', '"files":[{"extra":true,"path":'),
                $manifest.Replace('"files":[{"path":', '"files":[{"Path":')
            )
            foreach ($mutated in $mutations) {
                Set-TestTalkPayloadManifest `
                    -ExecutablePath $fixture.ExecutablePath `
                    -ManifestText $mutated

                { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
            }
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects payload lengths that exceed the executable' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            [Array]::Copy([System.BitConverter]::GetBytes([uint64]::MaxValue), 0, $bytes, $layout.TrailerStart + 12, 8)
            [System.IO.File]::WriteAllBytes($fixture.ExecutablePath, $bytes)

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects an archive byte mutation through the archive hash' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            $bytes[$layout.ArchiveStart] = $bytes[$layout.ArchiveStart] -bxor 0x01
            [System.IO.File]::WriteAllBytes($fixture.ExecutablePath, $bytes)

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a member hash mutation even when the archive hash is unchanged' {
        $fixture = New-TestTalkProductFixture
        try {
            $bytes = [System.IO.File]::ReadAllBytes($fixture.ExecutablePath)
            $layout = Get-TestTalkPayloadLayout -Bytes $bytes
            $manifest = [System.Text.Encoding]::UTF8.GetString(
                $bytes[$layout.ManifestStart..($layout.ManifestStart + $layout.ManifestLength - 1)]
            )
            $manifest = $manifest -replace '"sha256":"[0-9a-f]', '"sha256":"0'
            [byte[]]$manifestBytes = [System.Text.Encoding]::UTF8.GetBytes($manifest)
            [Array]::Copy($manifestBytes, 0, $bytes, $layout.ManifestStart, $manifestBytes.Length)
            [System.IO.File]::WriteAllBytes($fixture.ExecutablePath, $bytes)

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects engineering files in the product directory' {
        $root = Join-Path $env:TEMP ('talk-product-validator-extra-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $root -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $root 'Talk.exe') -Value 'not enough bytes' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $root 'talk.toml') -Value 'voice_mode = "smart"' -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $root 'debug.ps1') -Value 'developer tool' -Encoding ASCII

            { Test-TalkProductRelease -ProductPath $root } | Should Throw
        } finally {
            Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects hidden engineering files in the product directory' {
        $fixture = New-TestTalkProductFixture
        try {
            $hiddenPath = Join-Path $fixture.ProductPath '.debug.txt'
            Set-Content -LiteralPath $hiddenPath -Value 'debug' -Encoding ASCII
            $hiddenItem = Get-Item -LiteralPath $hiddenPath -Force
            $hiddenItem.Attributes = $hiddenItem.Attributes -bor [System.IO.FileAttributes]::Hidden

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects hidden engineering directories in the product directory' {
        $fixture = New-TestTalkProductFixture
        try {
            $hiddenPath = Join-Path $fixture.ProductPath '.debug'
            New-Item -ItemType Directory -Path $hiddenPath -Force | Out-Null
            $hiddenItem = Get-Item -LiteralPath $hiddenPath -Force
            $hiddenItem.Attributes = $hiddenItem.Attributes -bor [System.IO.FileAttributes]::Hidden

            { Test-TalkProductRelease -ProductPath $fixture.ProductPath } | Should Throw
        } finally {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
