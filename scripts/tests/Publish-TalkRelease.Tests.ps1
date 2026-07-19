$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Publish-TalkRelease.ps1'
$talkRoot = Split-Path (Split-Path $here -Parent) -Parent
$summaryValidatorScriptPath = Join-Path (Split-Path $here -Parent) 'Test-TalkReleaseSummary.ps1'

. $scriptPath
. $summaryValidatorScriptPath

Describe 'Publish-TalkRelease helpers' {
    It 'publishes a product profile as exactly Talk.exe and talk.toml' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-product-profile-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
        try {
            $result = Publish-TalkRelease `
                -VersionId 'talk-product-profile-red' `
                -ReleaseRoot $releaseRoot `
                -ProductProfile `
                -SkipVerification `
                -SkipBuild `
                -SkipSmoke `
                -SkipNativePreflight `
                -SkipNativeReadiness

            $relativeFiles = @(Get-ChildItem -LiteralPath $result.DestinationDir -Recurse -File |
                ForEach-Object { $_.FullName.Substring($result.DestinationDir.Length).TrimStart('\\') } |
                Sort-Object)
            $relativeFiles | Should Be @('Talk.exe', 'talk.toml')
        } finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'embeds the five-member runtime payload into the product executable' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-payload-builder-' + [guid]::NewGuid().ToString())
        $outputPath = Join-Path $tempRoot 'Talk.exe'
        $sourceRoot = Join-Path $tempRoot 'sources'
        New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null
        try {
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

            $result = New-TalkEmbeddedRuntimeExecutable `
                -BaseExecutablePath $basePath `
                -PayloadFiles $payloadFiles `
                -OutputPath $outputPath

            $bytes = [System.IO.File]::ReadAllBytes($outputPath)
            $magic = [System.Text.Encoding]::ASCII.GetBytes('TLPAY001')
            $magicOffset = $bytes.Length - 60
            $bytes[$magicOffset..($magicOffset + 7)] | Should Be $magic
            $result.ArchiveSha256.Length | Should Be 64
        } finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'resolves a standalone Talk checkout as the release repository root' {
        $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('talk-release-standalone-' + [guid]::NewGuid().ToString('N'))
        $standaloneRoot = Join-Path $tempRoot 'Talk'
        New-Item -ItemType Directory -Path (Join-Path $standaloneRoot '.git') -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $standaloneRoot 'Cargo.toml') -Value '[workspace]' -Encoding UTF8

        try {
            $context = Resolve-TalkReleaseRepositoryContext -TalkRepoRoot $standaloneRoot

            $context.RepositoryRoot | Should Be ([System.IO.Path]::GetFullPath($standaloneRoot))
            $context.WorkingDirectory | Should Be ([System.IO.Path]::GetFullPath($standaloneRoot))
            $context.ManifestPath | Should Be 'Cargo.toml'
        } finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'resolves a Talk subdirectory checkout through the Neuro repository root' {
        $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('talk-release-monorepo-' + [guid]::NewGuid().ToString('N'))
        $talkRootFixture = Join-Path $tempRoot 'Talk'
        New-Item -ItemType Directory -Path (Join-Path $tempRoot '.git') -Force | Out-Null
        New-Item -ItemType Directory -Path $talkRootFixture -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $talkRootFixture 'Cargo.toml') -Value '[workspace]' -Encoding UTF8

        try {
            $context = Resolve-TalkReleaseRepositoryContext -TalkRepoRoot $talkRootFixture

            $context.RepositoryRoot | Should Be ([System.IO.Path]::GetFullPath($tempRoot))
            $context.WorkingDirectory | Should Be ([System.IO.Path]::GetFullPath($tempRoot))
            $context.ManifestPath | Should Be 'Talk/Cargo.toml'
        } finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'builds verification commands from the resolved manifest path' {
        $steps = Get-VerificationSteps -Skipped $false -ManifestPath 'Cargo.toml'

        $steps | Should Be @(
            'cargo fmt --manifest-path Cargo.toml --all -- --check',
            'cargo check --manifest-path Cargo.toml --workspace --all-targets',
            'cargo test --manifest-path Cargo.toml --workspace'
        )
    }

    It 'resolves missing release runtime DLLs from the sherpa shared prebuilt cache' {
        $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('talk-release-runtime-dlls-' + [guid]::NewGuid().ToString('N'))
        $releaseDir = Join-Path $tempRoot 'target\release'
        $prebuiltLibDir = Join-Path $tempRoot 'target\sherpa-onnx-prebuilt\sherpa-onnx-v1.13.4-win-x64-shared-MT-Release-lib\lib'
        $dllNames = @(
            'sherpa-onnx-c-api.dll',
            'sherpa-onnx-cxx-api.dll',
            'onnxruntime.dll',
            'onnxruntime_providers_shared.dll'
        )
        New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
        New-Item -ItemType Directory -Path $prebuiltLibDir -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $releaseDir $dllNames[0]) -Value 'release dll' -Encoding ASCII
        foreach ($dllName in $dllNames) {
            Set-Content -LiteralPath (Join-Path $prebuiltLibDir $dllName) -Value 'prebuilt dll' -Encoding ASCII
        }

        try {
            $sources = @(
                Resolve-TalkReleaseRuntimeDllSources `
                    -TalkRepoRoot $tempRoot `
                    -DllNames $dllNames
            )

            $sources | Should Be @(
                (Join-Path $releaseDir $dllNames[0]),
                (Join-Path $prebuiltLibDir $dllNames[1]),
                (Join-Path $prebuiltLibDir $dllNames[2]),
                (Join-Path $prebuiltLibDir $dllNames[3])
            )
        } finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'renders a packaged api key into the desktop config when one is provided' {
        $configText = New-TalkReleaseDesktopConfigContent -PackagedApiKey 'packed-key'

        $configText | Should Match 'api_key = "packed-key"'
        $configText | Should Not Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
    }

    It 'can disable environment and local-file api key auto-discovery for public releases' {
        $tempHome = Join-Path ([System.IO.Path]::GetTempPath()) ('talk-release-no-key-' + [guid]::NewGuid().ToString('N'))
        $credentialDir = Join-Path $tempHome '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
        $credentialPath = Join-Path $credentialDir 'manual-live.json'
        New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
        Set-Content -LiteralPath $credentialPath -Value '{"apiKey":"auto-discovered-test-key"}' -Encoding UTF8
        $oldUserProfile = $env:USERPROFILE
        $oldHome = $env:HOME
        $oldApiKey = $env:TALK_PROVIDER_API_KEY

        try {
            $env:USERPROFILE = $tempHome
            $env:HOME = $tempHome
            $env:TALK_PROVIDER_API_KEY = 'environment-test-key'

            $resolved = Resolve-TalkReleasePackagedApiKey `
                -ConfigText 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' `
                -DisableAutoDiscovery

            $resolved | Should BeNullOrEmpty
        } finally {
            $env:USERPROFILE = $oldUserProfile
            if ($null -eq $oldHome) {
                Remove-Item Env:HOME -ErrorAction SilentlyContinue
            } else {
                $env:HOME = $oldHome
            }
            if ($null -eq $oldApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $oldApiKey
            }
            Remove-Item -LiteralPath $tempHome -Recurse -Force
        }
    }

    It 'keeps the Talk desktop release manifest fixture aligned with the current schema' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-contract-v2' `
            -BuiltAt '2026-07-05T04:00:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-contract-v2' `
            -CommandRecords @(
                [pscustomobject]@{
                    Display = 'cargo test --manifest-path Talk/Cargo.toml --workspace'
                    WorkingDirectory = 'C:\Users\Public\nas_home\AI\GameEditor\Neuro'
                }
            ) `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 202
                    sha256 = 'sha-talk-desktop'
                }
            ) `
            -SupportFileRecords @(
                [pscustomobject]@{
                    kind = 'desktop-config'
                    path = 'talk-desktop.toml'
                },
                [pscustomobject]@{
                    kind = 'desktop-launcher'
                    path = 'Start-TalkDesktop.ps1'
                }
            ) `
            -BuildLogRecords @(
                [pscustomobject]@{
                    kind = 'build-log'
                    path = 'logs\talk-release-01.log'
                }
            ) `
            -NativePreflightRecords @(
                [pscustomobject]@{
                    Name = 'audio-native-disabled'
                    ConfigPath = 'C:\Talk\.runtime\native-preflight\audio\config.toml'
                    EvidencePath = 'C:\Talk\.runtime\native-preflight\audio\session.json'
                    ExpectedError = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                    ExitCode = 1
                    OutputText = 'Error: audio error'
                }
            ) `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'native-unavailable-status'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\.runtime\native-status\config.toml'
                    DialogText = "Current: Talk: native unavailable`nAudio backend: native_windows"
                    StatusKind = 'native_unavailable'
                    StatusSummary = 'current=Talk: native unavailable; audio_backend=native_windows; audio_backend_readiness=unavailable'
                    StatusFields = [ordered]@{
                        Current = 'Talk: native unavailable'
                        'Audio backend' = 'native_windows'
                        'Audio backend readiness' = 'unavailable'
                    }
                    StatusSnapshot = [ordered]@{
                        current = 'Talk: native unavailable'
                        audioBackend = 'native_windows'
                        audioBackendReadiness = 'unavailable'
                    }
                },
                [pscustomobject]@{
                    Scenario = 'broken-config-recovery'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\.runtime\broken-config\config.toml'
                    LogPath = 'C:\Talk\.runtime\broken-config\session.json'
                    Status = 'cancelled'
                    BeforeReloadDialogText = 'Current: Talk: config unavailable'
                    BeforeReloadStatusKind = 'config_unavailable'
                    BeforeReloadStatusSummary = 'current=Talk: config unavailable; hotkey=unconfigured'
                    BeforeReloadStatusFields = [ordered]@{
                        Current = 'Talk: config unavailable'
                        Hotkey = 'unconfigured'
                    }
                    BeforeReloadStatusSnapshot = [ordered]@{
                        current = 'Talk: config unavailable'
                        hotkey = 'unconfigured'
                    }
                    AfterReloadDialogText = 'Current: Talk: idle'
                    AfterReloadStatusKind = 'idle'
                    AfterReloadStatusSummary = 'current=Talk: idle; hotkey=Ctrl+Alt+F22'
                    AfterReloadStatusFields = [ordered]@{
                        Current = 'Talk: idle'
                        Hotkey = 'Ctrl+Alt+F22'
                    }
                    AfterReloadStatusSnapshot = [ordered]@{
                        current = 'Talk: idle'
                        hotkey = 'Ctrl+Alt+F22'
                    }
                }
            ) `
            -NativeReadinessResult ([pscustomobject]@{
                ConfigPath = 'C:\Talk\.runtime\native-readiness\config.toml'
                EvidencePath = 'C:\Talk\.runtime\native-readiness\readiness.json'
                AudioStatus = 'ready'
                AudioReason = $null
                AudioDeviceName = 'Microphone Array'
                AudioDefaultSampleRateHz = 48000
                AudioDefaultChannels = 2
                AudioSampleFormat = 'f32'
                ClipboardStatus = 'ready'
                ClipboardReason = $null
                OutputText = '{"app":"talk","allReady":true}'
            })

        $fixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-manifest.json'
        $expected = Get-Content -Raw $fixturePath | ConvertFrom-Json | ConvertTo-Json -Depth 8 -Compress
        $actual = $manifest | ConvertTo-Json -Depth 8 -Compress

        $manifest.schemaVersion | Should Be 2
        $actual | Should Be $expected
    }

    It 'builds BUILD_INFO text with smoke verification and evidence paths' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260704-v6' `
            -BuiltAt '2026-07-04T23:40:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @(
                'cargo fmt --manifest-path Talk/Cargo.toml --all -- --check',
                'cargo test --manifest-path Talk/Cargo.toml --workspace'
            ) `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'cancel-and-status'
                    LogPath = 'C:\Talk\.runtime\cancel\abc.json'
                },
                [pscustomobject]@{
                    Scenario = 'broken-config-recovery'
                    LogPath = 'C:\Talk\.runtime\reload\def.json'
                }
            )

        $buildInfo | Should Match 'version_id: desktop-shell-20260704-v6'
        $buildInfo | Should Match '  - talk-desktop\.exe'
        $buildInfo | Should Match 'desktop smoke: cancel-and-status'
        $buildInfo | Should Match 'desktop smoke: broken-config-recovery'
        $buildInfo | Should Match 'smoke_artifacts:'
        $buildInfo | Should Match 'abc\.json'
        $buildInfo | Should Match 'def\.json'
    }

    It 'builds BUILD_INFO text with explicit smoke skipped note when smoke is omitted' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260704-v6' `
            -BuiltAt '2026-07-04T23:40:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo check --manifest-path Talk/Cargo.toml --workspace --all-targets') `
            -SmokeResults @()

        $buildInfo | Should Match 'desktop smoke: skipped'
        $buildInfo | Should Not Match 'smoke_artifacts:'
    }

    It 'builds BUILD_INFO text with native_windows preflight evidence' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v1' `
            -BuiltAt '2026-07-05T00:10:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @() `
            -NativePreflightResults @(
                [pscustomobject]@{
                    Name = 'audio-native-disabled'
                    EvidencePath = 'C:\Talk\.runtime\preflight\audio\session.json'
                    ExpectedError = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                },
                [pscustomobject]@{
                    Name = 'clipboard-native-disabled'
                    EvidencePath = 'C:\Talk\.runtime\preflight\clipboard\session.json'
                    ExpectedError = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
                }
            )

        $buildInfo | Should Match 'native_preflight:'
        $buildInfo | Should Match 'audio-native-disabled'
        $buildInfo | Should Match 'clipboard-native-disabled'
        $buildInfo | Should Match 'TALK_DISABLE_NATIVE_AUDIO'
        $buildInfo | Should Match 'TALK_DISABLE_NATIVE_CLIPBOARD'
    }

    It 'builds BUILD_INFO text with positive native readiness evidence' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v2' `
            -BuiltAt '2026-07-05T01:10:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @() `
            -NativeReadinessResult ([pscustomobject]@{
                EvidencePath = 'C:\Talk\.runtime\readiness\readiness.json'
                AudioStatus = 'ready'
                AudioDeviceName = 'Microphone Array'
                AudioDefaultSampleRateHz = 48000
                AudioDefaultChannels = 2
                AudioSampleFormat = 'F32'
                ClipboardStatus = 'ready'
            })

        $buildInfo | Should Match 'native_readiness:'
        $buildInfo | Should Match 'audio_status: ready'
        $buildInfo | Should Match 'clipboard_status: ready'
        $buildInfo | Should Match 'Microphone Array'
        $buildInfo | Should Match 'readiness\.json'
    }

    It 'builds BUILD_INFO text with structured desktop smoke summaries' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v2b' `
            -BuiltAt '2026-07-05T02:15:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'native-unavailable-status'
                    StatusKind = 'native_unavailable'
                    StatusSummary = 'current=Talk: native unavailable; audio_backend=native_windows; audio_backend_readiness=unavailable'
                }
            )

        $buildInfo | Should Match 'desktop_smoke_status:'
        $buildInfo | Should Match 'native-unavailable-status \[native_unavailable\]'
        $buildInfo | Should Match 'audio_backend_readiness=unavailable'
    }

    It 'builds BUILD_INFO text with hostile foreground smoke classification and evidence paths' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v2c' `
            -BuiltAt '2026-07-05T02:25:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    Status = 'completed'
                    FailureKind = 'hostile_foreground_environment'
                    FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                    FailureEvidencePath = 'C:\Talk\.runtime\release-smoke\failure-diagnostic.json'
                }
            )

        $buildInfo | Should Match 'desktop_smoke_failures:'
        $buildInfo | Should Match 'hostile_foreground_environment'
        $buildInfo | Should Match 'clipboard paste'
        $buildInfo | Should Match 'failure-diagnostic\.json'
    }

    It 'builds BUILD_INFO text with structured desktop smoke retry summaries' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v2d' `
            -BuiltAt '2026-07-05T02:35:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    Status = 'completed'
                    RetryCount = 1
                    RetryReason = 'hostile_foreground_environment'
                }
            )

        $buildInfo | Should Match 'desktop_smoke_retries:'
        $buildInfo | Should Match 'retry_count=1'
        $buildInfo | Should Match 'retry_reason=hostile_foreground_environment'
    }

    It 'builds BUILD_INFO text with insert-target diagnostic artifacts when present' {
        $buildInfo = New-TalkReleaseBuildInfoText `
            -VersionId 'desktop-shell-20260705-v2e' `
            -BuiltAt '2026-07-05T02:40:00+08:00' `
            -SourceWorkspace 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk' `
            -ArtifactNames @('talk-desktop.exe') `
            -VerificationSteps @('cargo test --manifest-path Talk/Cargo.toml --workspace') `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    Status = 'completed'
                    LogPath = 'C:\Talk\.runtime\release-smoke\session.json'
                    InsertTargetDiagnosticPath = 'C:\Talk\.runtime\release-smoke\session.desktop-insert-target.json'
                }
            )

        $buildInfo | Should Match 'session\.desktop-insert-target\.json'
    }

    It 'builds manifest object with structured desktop smoke status fields' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-20260705-v3' `
            -BuiltAt '2026-07-05T02:10:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-20260705-v3' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 1
                    sha256 = 'abc'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'native-unavailable-status'
                    ConfigPath = 'C:\Talk\config.toml'
                    DialogText = "Current: Talk: native unavailable`nAudio backend: native_windows"
                    StatusKind = 'native_unavailable'
                    StatusSummary = 'current=Talk: native unavailable; audio_backend=native_windows'
                    StatusFields = [ordered]@{
                        Current = 'Talk: native unavailable'
                        'Audio backend' = 'native_windows'
                        'Audio backend readiness' = 'unavailable'
                    }
                    StatusSnapshot = [ordered]@{
                        current = 'Talk: native unavailable'
                        audioBackend = 'native_windows'
                        audioBackendReadiness = 'unavailable'
                    }
                }
            )

        $manifest.desktopSmoke.Count | Should Be 1
        $manifest.desktopSmoke[0].scenario | Should Be 'native-unavailable-status'
        $manifest.desktopSmoke[0].statusKind | Should Be 'native_unavailable'
        $manifest.desktopSmoke[0].statusSummary | Should Match 'audio_backend=native_windows'
        $manifest.desktopSmoke[0].statusFields.Current | Should Be 'Talk: native unavailable'
        $manifest.desktopSmoke[0].statusFields.'Audio backend' | Should Be 'native_windows'
        $manifest.desktopSmoke[0].statusFields.'Audio backend readiness' | Should Be 'unavailable'
        $manifest.desktopSmoke[0].statusSnapshot.current | Should Be 'Talk: native unavailable'
        $manifest.desktopSmoke[0].statusSnapshot.audioBackend | Should Be 'native_windows'
        $manifest.desktopSmoke[0].statusSnapshot.audioBackendReadiness | Should Be 'unavailable'
    }

    It 'builds manifest object with structured desktop smoke failure fields' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-20260705-v3a' `
            -BuiltAt '2026-07-05T02:15:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-20260705-v3a' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 1
                    sha256 = 'abc'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\config.toml'
                    LogPath = 'C:\Talk\session.json'
                    Status = 'completed'
                    FailureKind = 'hostile_foreground_environment'
                    FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                    FailureEvidencePath = 'C:\Talk\failure-diagnostic.json'
                }
            )

        $manifest.desktopSmoke[0].failureKind | Should Be 'hostile_foreground_environment'
        $manifest.desktopSmoke[0].failureSummary | Should Match 'clipboard paste'
        $manifest.desktopSmoke[0].failureEvidencePath | Should Be 'C:\Talk\failure-diagnostic.json'
    }

    It 'builds manifest object with structured desktop smoke retry fields' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-20260705-v3c' `
            -BuiltAt '2026-07-05T02:18:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-20260705-v3c' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 1
                    sha256 = 'abc'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\config.toml'
                    Status = 'completed'
                    RetryCount = 1
                    RetryReason = 'hostile_foreground_environment'
                }
            )

        $manifest.desktopSmoke[0].retryCount | Should Be 1
        $manifest.desktopSmoke[0].retryReason | Should Be 'hostile_foreground_environment'
    }

    It 'builds manifest object with insert-target diagnostic path when present' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-20260705-v3d' `
            -BuiltAt '2026-07-05T02:19:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-20260705-v3d' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 1
                    sha256 = 'abc'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\config.toml'
                    Status = 'completed'
                    InsertTargetDiagnosticPath = 'C:\Talk\session.desktop-insert-target.json'
                }
            )

        $manifest.desktopSmoke[0].insertTargetDiagnosticPath | Should Be 'C:\Talk\session.desktop-insert-target.json'
    }

    It 'builds manifest object with normalized reload status snapshots' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-20260705-v3b' `
            -BuiltAt '2026-07-05T02:20:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-20260705-v3b' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 1
                    sha256 = 'abc'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'broken-config-recovery'
                    ConfigPath = 'C:\Talk\config.toml'
                    StatusSnapshot = [ordered]@{
                        current = 'Talk: idle'
                        hotkey = 'Ctrl+Alt+F22'
                    }
                    BeforeReloadStatusSnapshot = [ordered]@{
                        current = 'Talk: config unavailable'
                        configPath = 'C:\Talk\config.toml'
                    }
                    AfterReloadStatusSnapshot = [ordered]@{
                        current = 'Talk: idle'
                        hotkey = 'Ctrl+Alt+F22'
                        audioBackend = 'silent'
                        clipboardBackend = 'dry_run'
                    }
                }
            )

        $manifest.desktopSmoke[0].statusSnapshot.current | Should Be 'Talk: idle'
        $manifest.desktopSmoke[0].statusSnapshot.hotkey | Should Be 'Ctrl+Alt+F22'
        $manifest.desktopSmoke[0].beforeReloadStatusSnapshot.current | Should Be 'Talk: config unavailable'
        $manifest.desktopSmoke[0].beforeReloadStatusSnapshot.configPath | Should Be 'C:\Talk\config.toml'
        $manifest.desktopSmoke[0].afterReloadStatusSnapshot.current | Should Be 'Talk: idle'
        $manifest.desktopSmoke[0].afterReloadStatusSnapshot.hotkey | Should Be 'Ctrl+Alt+F22'
        $manifest.desktopSmoke[0].afterReloadStatusSnapshot.audioBackend | Should Be 'silent'
        $manifest.desktopSmoke[0].afterReloadStatusSnapshot.clipboardBackend | Should Be 'dry_run'
    }

    It 'builds native_windows preflight config content for audio backend' {
        $configText = New-TalkNativePreflightConfigContent `
            -Kind 'audio-native-disabled' `
            -SessionRoot 'C:\Talk\.runtime\preflight\audio'

        $configText | Should Match 'backend = "native_windows"'
        $configText | Should Match 'mode = "dry_run"'
    }

    It 'treats missing optional session properties as null' {
        $session = [pscustomobject]@{
            status = 'failed'
            output_text = 'hello_native_clipboard'
        }

        (Get-OptionalPsPropertyValue -Object $session -Name 'insert_outcome') | Should Be $null
    }

    It 'returns a structured command record for internal logging' {
        $result = Invoke-PowerShellCommand `
            -Command "Write-Output 'verification noise'" `
            -WorkingDirectory 'C:\Users\Public\nas_home\AI\GameEditor\Neuro'

        $result.Display | Should Be "Write-Output 'verification noise'"
        $result.WorkingDirectory | Should Be 'C:\Users\Public\nas_home\AI\GameEditor\Neuro'
        $result.OutputText | Should Be 'verification noise'
        $result.ExitCode | Should Be 0
    }

    It 'captures stderr from talk-style child processes without throwing' {
        $result = Invoke-TalkProcess `
            -FilePath 'powershell.exe' `
            -Arguments @('-NoProfile', '-Command', "Write-Error 'native_windows failed'; exit 7") `
            -WorkingDirectory 'C:\Users\Public\nas_home\AI\GameEditor\Neuro'

        $result.ExitCode | Should Be 7
        $result.OutputText | Should Match 'native_windows failed'
    }

    It 'passes an explicit smoke root into the desktop smoke invocation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        $expectedSmokeRoot = Join-Path $tempRoot 'explicit-smoke-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-TalkDesktopReleaseSmoke {
                param([string]$ReleaseDir, [string]$SmokeRoot)
                [pscustomobject]@{
                    Scenario = 'mock-smoke'
                    BinaryPath = (Join-Path $ReleaseDir 'talk-desktop.exe')
                    ConfigPath = (Join-Path $SmokeRoot 'mock-config.toml')
                    ReleaseDir = $ReleaseDir
                    SmokeRoot = $SmokeRoot
                    LogPath = (Join-Path $SmokeRoot 'mock-log.json')
                    Status = 'cancelled'
                    DialogText = "Current: Talk: idle`nHotkey: Ctrl+Alt+F24"
                    StatusKind = 'idle'
                    StatusSummary = 'current=Talk: idle; hotkey=Ctrl+Alt+F24'
                    StatusFields = [ordered]@{
                        Current = 'Talk: idle'
                        Hotkey = 'Ctrl+Alt+F24'
                    }
                    StatusSnapshot = [ordered]@{
                        current = 'Talk: idle'
                        hotkey = 'Ctrl+Alt+F24'
                    }
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight {
                param([string]$TalkBinaryPath, [string]$SmokeRoot)
                @(
                    [pscustomobject]@{
                        Name = 'audio-native-disabled'
                        ConfigPath = (Join-Path $SmokeRoot 'audio\config.toml')
                        EvidencePath = (Join-Path $SmokeRoot 'audio\session.json')
                        ExpectedError = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                        ExitCode = 1
                        OutputText = 'Error: audio error'
                    },
                    [pscustomobject]@{
                        Name = 'clipboard-native-disabled'
                        ConfigPath = (Join-Path $SmokeRoot 'clipboard\config.toml')
                        EvidencePath = (Join-Path $SmokeRoot 'clipboard\session.json')
                        ExpectedError = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
                        ExitCode = 1
                        OutputText = 'Error: clipboard error'
                    }
                )
            }
            Mock Invoke-TalkNativeWindowsReadiness {
                param([string]$TalkBinaryPath, [string]$SmokeRoot)
                [pscustomobject]@{
                    ConfigPath = (Join-Path $SmokeRoot 'readiness\config.toml')
                    EvidencePath = (Join-Path $SmokeRoot 'readiness\readiness.json')
                    AudioStatus = 'ready'
                    AudioDeviceName = 'Microphone Array'
                    AudioDefaultSampleRateHz = 48000
                    AudioDefaultChannels = 2
                    AudioSampleFormat = 'F32'
                    ClipboardStatus = 'ready'
                    ClipboardReason = $null
                    OutputText = '{"app":"talk","allReady":true}'
                }
            }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test' `
                -ReleaseRoot $releaseRoot `
                -SmokeRoot $expectedSmokeRoot `
                -SkipVerification `
                -SkipBuild

            $result.SmokeResults[0].SmokeRoot | Should Be ([System.IO.Path]::GetFullPath($expectedSmokeRoot))
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes release artifacts before failing on classified desktop smoke interference' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        $smokeRoot = Join-Path $tempRoot 'smoke-root'
        $destinationDir = Join-Path $releaseRoot 'desktop-shell-smoke-classified'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }
            Mock Invoke-TalkDesktopReleaseSmoke {
                param([string]$ReleaseDir, [string]$SmokeRoot, [switch]$ContinueOnFailure)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = (Join-Path $ReleaseDir 'talk-desktop.exe')
                    ConfigPath = (Join-Path $SmokeRoot 'config.toml')
                    LogPath = (Join-Path $SmokeRoot 'session.json')
                    Status = 'completed'
                    FailureKind = 'hostile_foreground_environment'
                    FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                    FailureEvidencePath = (Join-Path $SmokeRoot 'failure-diagnostic.json')
                }
            }

            $publishError = $null
            try {
                Publish-TalkRelease `
                    -VersionId 'desktop-shell-smoke-classified' `
                    -ReleaseRoot $releaseRoot `
                    -SmokeRoot $smokeRoot `
                    -SkipVerification `
                    -SkipBuild
            } catch {
                $publishError = $_
            }

            $publishError | Should Not Be $null
            [string]$publishError.Exception.Message | Should Match 'hostile_foreground_environment'
            Test-Path -LiteralPath (Join-Path $destinationDir 'manifest.json') | Should Be $true
            Test-Path -LiteralPath (Join-Path $destinationDir 'release-summary.json') | Should Be $true

            $manifest = Get-Content -LiteralPath (Join-Path $destinationDir 'manifest.json') -Raw | ConvertFrom-Json
            $summary = Get-Content -LiteralPath (Join-Path $destinationDir 'release-summary.json') -Raw | ConvertFrom-Json

            $manifest.desktopSmoke[0].failureKind | Should Be 'hostile_foreground_environment'
            $summary.desktopSmoke.scenarios[0].failureKind | Should Be 'hostile_foreground_environment'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'retries hostile foreground desktop smoke once and keeps the successful retry result' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        $smokeRoot = Join-Path $tempRoot 'smoke-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $script:retrySmokeInvocation = 0
            Mock Invoke-TalkDesktopReleaseSmoke {
                param([string]$ReleaseDir, [string]$SmokeRoot, [string[]]$Scenario, [switch]$ContinueOnFailure)
                $script:retrySmokeInvocation += 1
                if ($script:retrySmokeInvocation -eq 1) {
                    return [pscustomobject]@{
                        Scenario = 'openai-compatible-audio-input-insert-success'
                        BinaryPath = (Join-Path $ReleaseDir 'talk-desktop.exe')
                        ConfigPath = (Join-Path $SmokeRoot 'config.toml')
                        LogPath = (Join-Path $SmokeRoot 'session.json')
                        Status = 'completed'
                        FailureKind = 'hostile_foreground_environment'
                        FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                        FailureEvidencePath = (Join-Path $SmokeRoot 'failure-diagnostic.json')
                    }
                }

                return [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = (Join-Path $ReleaseDir 'talk-desktop.exe')
                    ConfigPath = (Join-Path $SmokeRoot 'retry-config.toml')
                    LogPath = (Join-Path $SmokeRoot 'retry-session.json')
                    Status = 'completed'
                }
            }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-smoke-retry-success' `
                -ReleaseRoot $releaseRoot `
                -SmokeRoot $smokeRoot `
                -SkipVerification `
                -SkipBuild

            $script:retrySmokeInvocation | Should Be 2
            $result.SmokeResults.Count | Should Be 1
            (Get-OptionalPsPropertyValue -Object $result.SmokeResults[0] -Name 'FailureKind') | Should Be $null
            $result.SmokeResults[0].RetryCount | Should Be 1
            $result.SmokeResults[0].RetryReason | Should Be 'hostile_foreground_environment'

            $summary = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'release-summary.json') -Raw | ConvertFrom-Json
            $summary.desktopSmoke.scenarios[0].retryCount | Should Be 1
            $summary.desktopSmoke.scenarios[0].retryReason | Should Be 'hostile_foreground_environment'
        }
        finally {
            Remove-Variable -Name retrySmokeInvocation -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes manifest.json and command logs for a publish run' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight {
                param([string]$TalkBinaryPath, [string]$SmokeRoot)
                @(
                    [pscustomobject]@{
                        Name = 'audio-native-disabled'
                        ConfigPath = (Join-Path $SmokeRoot 'audio\config.toml')
                        EvidencePath = (Join-Path $SmokeRoot 'audio\session.json')
                        ExpectedError = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                        ExitCode = 1
                        OutputText = 'Error: audio error'
                    },
                    [pscustomobject]@{
                        Name = 'clipboard-native-disabled'
                        ConfigPath = (Join-Path $SmokeRoot 'clipboard\config.toml')
                        EvidencePath = (Join-Path $SmokeRoot 'clipboard\session.json')
                        ExpectedError = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
                        ExitCode = 1
                        OutputText = 'Error: clipboard error'
                    }
                )
            }
            Mock Invoke-TalkNativeWindowsReadiness {
                param([string]$TalkBinaryPath, [string]$SmokeRoot)
                [pscustomobject]@{
                    ConfigPath = (Join-Path $SmokeRoot 'readiness\config.toml')
                    EvidencePath = (Join-Path $SmokeRoot 'readiness\readiness.json')
                    AudioStatus = 'ready'
                    AudioDeviceName = 'Microphone Array'
                    AudioDefaultSampleRateHz = 48000
                    AudioDefaultChannels = 2
                    AudioSampleFormat = 'F32'
                    ClipboardStatus = 'ready'
                    ClipboardReason = $null
                    OutputText = '{"app":"talk","allReady":true}'
                }
            }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-manifest' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $manifestPath = Join-Path $result.DestinationDir 'manifest.json'
            $summaryPath = Join-Path $result.DestinationDir 'release-summary.json'
            $log1Path = Join-Path $result.DestinationDir 'logs\talk-release-01.log'
            $log4Path = Join-Path $result.DestinationDir 'logs\talk-release-04.log'

            Test-Path -LiteralPath $manifestPath | Should Be $true
            Test-Path -LiteralPath $summaryPath | Should Be $true
            Test-Path -LiteralPath $log1Path | Should Be $true
            Test-Path -LiteralPath $log4Path | Should Be $true

            $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
            $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $manifest.schemaVersion | Should Be 2
            $manifest.app | Should Be 'Talk'
            $manifest.sourceProject | Should Be 'Talk'
            $manifest.exes.Count | Should Be 1
            $manifest.buildLogs.Count | Should Be 4
            $manifest.nativePreflight.Count | Should Be 2
            $manifest.desktopSmoke | Should Be $null
            $manifest.supportFiles.Count | Should Be 23
            $manifest.supportFiles[0].kind | Should Be 'release-summary'
            $manifest.supportFiles[0].path | Should Be 'release-summary.json'
            $manifest.supportFiles[1].kind | Should Be 'desktop-config'
            $manifest.supportFiles[1].path | Should Be 'talk-desktop.toml'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-daemon' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-daemon' })[0].path | Should Be '.internal/talk-local-asr-sherpa.exe'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-benchmark-tool' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-benchmark-tool' })[0].path | Should Be '.internal/asr-bench.exe'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-benchmark-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-benchmark-helper' })[0].path | Should Be 'Invoke-TalkAsrCorpusBenchmark.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-recorder-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-recorder-helper' })[0].path | Should Be 'Invoke-TalkAsrCorpusRecorder.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-prompt-template' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-prompt-template' })[0].path | Should Be 'asr-real-mic-prompts.json'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-selector' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-selector' })[0].path | Should Be 'Select-TalkDefaultAsrModel.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-applier' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-applier' })[0].path | Should Be 'Set-TalkDefaultAsrModel.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-workflow' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-workflow' })[0].path | Should Be 'Invoke-TalkAsrDefaultModelWorkflow.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-real-mic-default-model-workflow' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-real-mic-default-model-workflow' })[0].path | Should Be 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-runtime' }).Count | Should Be 4
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-launcher' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-launcher' })[0].path | Should Be 'Start-TalkDesktop.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-hotkey-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-hotkey-probe' })[0].path | Should Be 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-operator-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-operator-probe' })[0].path | Should Be 'Invoke-TalkDesktopLiveOperatorProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-soak-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-soak-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-native-mic-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-native-mic-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenNativeMicProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-smoke-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-smoke-helper' })[0].path | Should Be 'Invoke-TalkDesktopReleaseSmoke.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-model-installer' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-model-installer' })[0].path | Should Be 'Install-TalkSherpaModel.ps1'
            $manifest.nativeReadiness.audio.status | Should Be 'ready'
            $manifest.nativeReadiness.audio.deviceName | Should Be 'Microphone Array'
            $manifest.nativeReadiness.clipboard.status | Should Be 'ready'
            $manifest.nativePreflight[0].name | Should Be 'audio-native-disabled'
            $manifest.nativePreflight[0].expectedError | Should Match 'TALK_DISABLE_NATIVE_AUDIO'
            $manifest.buildInfo.path | Should Be 'BUILD_INFO.txt'
            $manifest.checksums | Should Be 'checksums.sha256'
            {
                Assert-TalkReleaseManifestObject -Manifest $manifest -Context $manifestPath
            } | Should Not Throw

            $summary.schemaVersion | Should Be 1
            $summary.manifestSchemaVersion | Should Be 2
            $summary.manifestPath | Should Be 'manifest.json'
            $summary.buildInfoPath | Should Be 'BUILD_INFO.txt'
            $summary.checksumPath | Should Be 'checksums.sha256'
            $summary.binaries.talkDesktopPath | Should Be 'talk-desktop.exe'
            $summary.desktopSmoke.skipped | Should Be $true
            $summary.nativeReadiness.audioStatus | Should Be 'ready'
            $summary.nativePreflight.checkCount | Should Be 2
            {
                Assert-TalkReleaseSummaryObject -Summary $summary -Context $summaryPath
            } | Should Not Throw

            $buildInfo = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'BUILD_INFO.txt') -Raw
            $buildInfo | Should Match 'native_readiness:'
            $buildInfo | Should Match 'audio_status: ready'

            $log1 = Get-Content -LiteralPath $log1Path -Raw
            $log4 = Get-Content -LiteralPath $log4Path -Raw
            $log1 | Should Match 'cargo fmt --manifest-path Talk/Cargo.toml --all -- --check'
            $log4 | Should Match 'cargo build --manifest-path Talk/Cargo.toml --release -p talk-daemon -p talk-desktop'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'hides the non-GUI Talk helper under .internal and exposes only talk-desktop.exe as a release executable' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-desktop-only-exe' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            (Test-Path -LiteralPath (Join-Path $result.DestinationDir 'talk.exe')) | Should Be $false
            (Test-Path -LiteralPath (Join-Path $result.DestinationDir '.internal\talk.exe')) | Should Be $true

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            $manifest.exes.Count | Should Be 1
            $manifest.exes[0].name | Should Be 'talk-desktop.exe'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages a default desktop config template for release-side direct launch' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        $originalTalkProviderApiKey = $env:TALK_PROVIDER_API_KEY
        try {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot

            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-default-config' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $desktopConfigPath = Join-Path $result.DestinationDir 'talk-desktop.toml'
            Test-Path -LiteralPath $desktopConfigPath | Should Be $true

            $desktopConfigText = Get-Content -LiteralPath $desktopConfigPath -Raw
            $desktopConfigText | Should Match 'voice_mode = "smart"'
            $desktopConfigText | Should Match 'mode = "toggle"'
            $desktopConfigText | Should Match 'toggle_shortcut = "RightAlt"'
            $desktopConfigText | Should Match 'transcribe_shortcut = "RightCtrl\+1"'
            $desktopConfigText | Should Match 'document_shortcut = "RightCtrl\+2"'
            $desktopConfigText | Should Match 'command_shortcut = "RightCtrl\+3"'
            $desktopConfigText | Should Match 'generate_shortcut = "RightCtrl\+4"'
            $desktopConfigText | Should Match 'smart_shortcut = "RightCtrl\+5"'
            $desktopConfigText | Should Match 'translate_shortcut = "RightAlt\+/"'
            $desktopConfigText | Should Match 'ask_shortcut = "RightAlt\+Space"'
            $desktopConfigText | Should Match 'backend = "native_windows"'
            $desktopConfigText | Should Match 'transcription_transport = "chat_completions_audio_input"'
            $desktopConfigText | Should Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
            $desktopConfigText | Should Match 'mode = "clipboard_paste"'
            $desktopConfigText | Should Match 'clipboard_backend = "native_windows"'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-config' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-config' })[0].path | Should Be 'talk-desktop.toml'
        }
        finally {
            if ($null -eq $originalTalkProviderApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $originalTalkProviderApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages an auto-discovered local qwen dashscope key into the desktop config for direct exe launch' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-autokey-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        $originalTalkProviderApiKey = $env:TALK_PROVIDER_API_KEY
        try {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot

            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"auto-json-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8

            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-packaged-autokey' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $desktopConfigPath = Join-Path $result.DestinationDir 'talk-desktop.toml'
            Test-Path -LiteralPath $desktopConfigPath | Should Be $true

            $desktopConfigText = Get-Content -LiteralPath $desktopConfigPath -Raw
            $desktopConfigText | Should Match 'api_key = "auto-json-key"'
            $desktopConfigText | Should Not Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
        }
        finally {
            if ($null -eq $originalTalkProviderApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $originalTalkProviderApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages an explicitly provided api key into the desktop config for direct exe launch' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-directkey-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-directkey' `
                -ReleaseRoot $releaseRoot `
                -PackagedApiKey 'direct-release-key' `
                -SkipSmoke

            $desktopConfigPath = Join-Path $result.DestinationDir 'talk-desktop.toml'
            Test-Path -LiteralPath $desktopConfigPath | Should Be $true

            $desktopConfigText = Get-Content -LiteralPath $desktopConfigPath -Raw
            $desktopConfigText | Should Match 'api_key = "direct-release-key"'
            $desktopConfigText | Should Not Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages a desktop launcher script for release-side direct launch' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-launcher' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $launcherPath = Join-Path $result.DestinationDir 'Start-TalkDesktop.ps1'
            Test-Path -LiteralPath $launcherPath | Should Be $true
            $launcherText = Get-Content -LiteralPath $launcherPath -Raw
            $launcherText | Should Match 'function Start-TalkDesktop'
            $launcherText | Should Match 'Resolve-TalkDesktopLaunchApiKey'
            $launcherText | Should Match 'talk-desktop\.exe'
            $launcherText | Should Match 'talk-desktop\.toml'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-launcher' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-launcher' })[0].path | Should Be 'Start-TalkDesktop.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the sherpa model installer for release-side local ASR setup' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-sherpa-installer' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $installerPath = Join-Path $result.DestinationDir 'Install-TalkSherpaModel.ps1'
            Test-Path -LiteralPath $installerPath | Should Be $true
            $installerText = Get-Content -LiteralPath $installerPath -Raw
            $installerText | Should Match 'function Install-TalkSherpaModel'
            $installerText | Should Match 'zipformer-zh-en-punct-int8-480ms'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-model-installer' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'local-asr-model-installer' })[0].path | Should Be 'Install-TalkSherpaModel.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the ASR benchmark tool for release-side local ASR validation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-bench' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            Test-Path -LiteralPath (Join-Path $result.DestinationDir '.internal\asr-bench.exe') | Should Be $true

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-benchmark-tool' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-benchmark-tool' })[0].path | Should Be '.internal/asr-bench.exe'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the ASR corpus benchmark helper for same-corpus model selection' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-corpus-bench' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $helperPath = Join-Path $result.DestinationDir 'Invoke-TalkAsrCorpusBenchmark.ps1'
            Test-Path -LiteralPath $helperPath | Should Be $true
            $helperText = Get-Content -LiteralPath $helperPath -Raw
            $helperText | Should Match 'function Invoke-TalkAsrCorpusBenchmark'
            $helperText | Should Match 'Read-TalkAsrCorpusManifest'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-benchmark-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-benchmark-helper' })[0].path | Should Be 'Invoke-TalkAsrCorpusBenchmark.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the ASR corpus recorder helper for real microphone corpus capture' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-corpus-recorder' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $helperPath = Join-Path $result.DestinationDir 'Invoke-TalkAsrCorpusRecorder.ps1'
            Test-Path -LiteralPath $helperPath | Should Be $true
            $helperText = Get-Content -LiteralPath $helperPath -Raw
            $helperText | Should Match 'function Invoke-TalkAsrCorpusRecorder'
            $helperText | Should Match 'Read-TalkAsrCorpusRecorderPrompts'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-recorder-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-recorder-helper' })[0].path | Should Be 'Invoke-TalkAsrCorpusRecorder.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages a real microphone ASR prompt template for release-side corpus capture' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-prompt-template' `
                -ReleaseRoot $releaseRoot `
                -SkipVerification `
                -SkipSmoke

            $promptTemplatePath = Join-Path $result.DestinationDir 'asr-real-mic-prompts.json'
            Test-Path -LiteralPath $promptTemplatePath | Should Be $true

            $promptTemplate = Get-Content -LiteralPath $promptTemplatePath -Raw | ConvertFrom-Json
            $promptTemplate.schemaVersion | Should Be 1
            @($promptTemplate.samples).Count | Should Not BeLessThan 3
            @($promptTemplate.samples | Where-Object { $_.sampleId -eq 'short-search-001' }).Count | Should Be 1
            @($promptTemplate.samples | Where-Object { $_.sampleId -eq 'mixed-english-001' }).Count | Should Be 1
            @($promptTemplate.samples | Where-Object { $_.sampleId -eq 'punctuation-001' }).Count | Should Be 1

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-prompt-template' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-corpus-prompt-template' })[0].path | Should Be 'asr-real-mic-prompts.json'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the default ASR model selector helper for evidence-gated model selection' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-default-selector' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $selectorPath = Join-Path $result.DestinationDir 'Select-TalkDefaultAsrModel.ps1'
            Test-Path -LiteralPath $selectorPath | Should Be $true
            $selectorText = Get-Content -LiteralPath $selectorPath -Raw
            $selectorText | Should Match 'function Select-TalkDefaultAsrModel'
            $selectorText | Should Match 'RequiredLocalModelId'
            $selectorText | Should Match 'StatusOnly'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-selector' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-selector' })[0].path | Should Be 'Select-TalkDefaultAsrModel.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the default ASR model applier helper for release-side config locking' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-default-applier' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $applierPath = Join-Path $result.DestinationDir 'Set-TalkDefaultAsrModel.ps1'
            Test-Path -LiteralPath $applierPath | Should Be $true
            $applierText = Get-Content -LiteralPath $applierPath -Raw
            $applierText | Should Match 'function Set-TalkDefaultAsrModel'
            $applierText | Should Match 'Test-TalkSherpaModelInstall'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-applier' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-applier' })[0].path | Should Be 'Set-TalkDefaultAsrModel.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the default ASR model workflow helper for one-command release-side selection and config locking' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-default-workflow' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $workflowPath = Join-Path $result.DestinationDir 'Invoke-TalkAsrDefaultModelWorkflow.ps1'
            Test-Path -LiteralPath $workflowPath | Should Be $true
            $workflowText = Get-Content -LiteralPath $workflowPath -Raw
            $workflowText | Should Match 'function Invoke-TalkAsrDefaultModelWorkflow'
            $workflowText | Should Match 'Select-TalkDefaultAsrModel'
            $workflowText | Should Match 'EvidenceStatusJson'
            $workflowText | Should Match 'StatusOnly'
            $workflowText | Should Match 'Set-TalkDefaultAsrModel'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-workflow' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-default-model-workflow' })[0].path | Should Be 'Invoke-TalkAsrDefaultModelWorkflow.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the real microphone ASR default model workflow helper for end-to-end Task 6 locking' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-asr-real-mic-default-workflow' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $workflowPath = Join-Path $result.DestinationDir 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'
            Test-Path -LiteralPath $workflowPath | Should Be $true
            $workflowText = Get-Content -LiteralPath $workflowPath -Raw
            $workflowText | Should Match 'function Invoke-TalkAsrRealMicDefaultModelWorkflow'
            $workflowText | Should Match 'Invoke-TalkAsrCorpusRecorder'
            $workflowText | Should Match 'Invoke-TalkAsrDefaultModelWorkflow'
            $workflowText | Should Match 'qwen3-asr-flash'
            $workflowText | Should Not Match 'qwen-audio-asr-latest'
            $workflowText | Should Match 'PreflightOnly'
            $workflowText | Should Match 'ProbeAudio'
            $workflowText | Should Match 'RecordOnly'
            $workflowText | Should Match 'microphone_signal'
            $workflowText | Should Match 'cloud_baseline_api_key'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-real-mic-default-model-workflow' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'asr-real-mic-default-model-workflow' })[0].path | Should Be 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages a desktop live hotkey probe script for release-side operator validation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-live-hotkey-probe' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $probePath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'
            $helperPath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopReleaseSmoke.ps1'
            Test-Path -LiteralPath $probePath | Should Be $true
            Test-Path -LiteralPath $helperPath | Should Be $true
            $probeText = Get-Content -LiteralPath $probePath -Raw
            $helperText = Get-Content -LiteralPath $helperPath -Raw
            $probeText | Should Match 'function Invoke-TalkDesktopLiveHotkeyProbe'
            $probeText | Should Match 'Invoke-TalkDesktopGlobalHotkeyOperation'
            $probeText | Should Match 'AudioProbeSeconds'
            $probeText | Should Match 'Invoke-TalkDesktopLiveHotkeyAudioProbe'
            $probeText | Should Match 'ProviderAudioTranscriptionsEndpoint'
            $probeText | Should Match 'ProviderChatCompletionsEndpoint'
            $probeText | Should Match 'clipboard_paste'
            $probeText | Should Match 'chat_completions_audio_input'
            $helperText | Should Match 'function Start-TalkTextCaptureTarget'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-hotkey-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-hotkey-probe' })[0].path | Should Be 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-smoke-helper' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-smoke-helper' })[0].path | Should Be 'Invoke-TalkDesktopReleaseSmoke.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages a desktop live operator probe script for release-side manual microphone validation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-live-operator-probe' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $probePath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopLiveOperatorProbe.ps1'
            Test-Path -LiteralPath $probePath | Should Be $true

            $probeText = Get-Content -LiteralPath $probePath -Raw
            $probeText | Should Match 'function Invoke-TalkDesktopLiveOperatorProbe'
            $probeText | Should Match 'Press and hold'
            $probeText | Should Match 'AudioProbeSeconds'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-operator-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-live-operator-probe' })[0].path | Should Be 'Invoke-TalkDesktopLiveOperatorProbe.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages qwen desktop probe scripts for release-side provider validation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-qwen-probes' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $globalProbePath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
            $nativeMicProbePath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopQwenNativeMicProbe.ps1'
            Test-Path -LiteralPath $globalProbePath | Should Be $true
            Test-Path -LiteralPath $nativeMicProbePath | Should Be $true

            $globalProbeText = Get-Content -LiteralPath $globalProbePath -Raw
            $nativeMicProbeText = Get-Content -LiteralPath $nativeMicProbePath -Raw
            $globalProbeText | Should Match 'function Invoke-TalkDesktopQwenGlobalHotkeyProbe'
            $globalProbeText | Should Match 'Send-TalkDesktopGlobalHotkeyChord'
            $nativeMicProbeText | Should Match 'function Invoke-TalkDesktopQwenNativeMicProbe'
            $nativeMicProbeText | Should Match 'Resolve-TalkDesktopQwenNativeMicProbeSpeakerOutputDevice'
            $nativeMicProbeText | Should Match 'Invoke-TalkDesktopNativeAudioSignalProbe'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-native-mic-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-native-mic-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenNativeMicProbe.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'packages the qwen global hotkey soak probe for release-side stability validation' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-publish-test-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $tempRoot 'release-root'
        New-Item -ItemType Directory -Path $releaseRoot | Out-Null
        try {
            Mock Invoke-PowerShellCommand {
                param([string]$Command, [string]$WorkingDirectory)
                [pscustomobject]@{
                    Display = $Command
                    WorkingDirectory = $WorkingDirectory
                    OutputText = "mock output for $Command"
                    ExitCode = 0
                }
            }
            Mock Invoke-TalkNativeWindowsPreflight { @() }
            Mock Invoke-TalkNativeWindowsReadiness { $null }

            $result = Publish-TalkRelease `
                -VersionId 'desktop-shell-test-qwen-soak' `
                -ReleaseRoot $releaseRoot `
                -SkipSmoke

            $soakProbePath = Join-Path $result.DestinationDir 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'
            Test-Path -LiteralPath $soakProbePath | Should Be $true

            $soakProbeText = Get-Content -LiteralPath $soakProbePath -Raw
            $soakProbeText | Should Match 'function Invoke-TalkDesktopQwenGlobalHotkeySoak'
            $soakProbeText | Should Match 'Invoke-TalkDesktopQwenGlobalHotkeyProbe'

            $manifest = Get-Content -LiteralPath (Join-Path $result.DestinationDir 'manifest.json') -Raw | ConvertFrom-Json
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-soak-probe' }).Count | Should Be 1
            @($manifest.supportFiles | Where-Object { $_.kind -eq 'desktop-qwen-global-hotkey-soak-probe' })[0].path | Should Be 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
