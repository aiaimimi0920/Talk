$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$talkRoot = Split-Path (Split-Path $here -Parent) -Parent
$validatorPath = Join-Path $talkRoot 'scripts\Test-TalkProductRelease.ps1'

. $validatorPath

Describe 'Talk product release validator' {
    It 'accepts exactly Talk.exe and talk.toml with the embedded payload trailer' {
        $root = Join-Path $env:TEMP ('talk-product-validator-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $root -Force | Out-Null
        try {
            [byte[]]$prefix = [System.Text.Encoding]::ASCII.GetBytes('MZ-product')
            [byte[]]$trailer = New-Object byte[] 60
            [byte[]]$magic = [System.Text.Encoding]::ASCII.GetBytes('TLPAY001')
            [Array]::Copy($magic, 0, $trailer, 0, $magic.Length)
            [System.IO.File]::WriteAllBytes((Join-Path $root 'Talk.exe'), $prefix + $trailer)
            Set-Content -LiteralPath (Join-Path $root 'talk.toml') -Value 'voice_mode = "smart"' -Encoding UTF8

            $result = Test-TalkProductRelease -ProductPath $root

            $result.PayloadTrailer | Should Be 'TLPAY001'
            $result.Files | Should Be @('Talk.exe', 'talk.toml')
        } finally {
            Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
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
}
