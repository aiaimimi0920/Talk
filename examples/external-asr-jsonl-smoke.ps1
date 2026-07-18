param(
    [Parameter(Mandatory = $false)]
    [string] $AudioPath = $env:TALK_LOCAL_ASR_AUDIO_FILE
)

if ([string]::IsNullOrWhiteSpace($AudioPath)) {
    Write-Error "AudioPath or TALK_LOCAL_ASR_AUDIO_FILE is required"
    exit 2
}

# Smoke adapter only. Replace this script with a real local ASR executable.
Write-Output '{"type":"partial","segment_id":"seg-1","text":"本地语音识别"}'
Write-Output '{"type":"final","segment_id":"seg-1","text":"本地语音识别。"}'
