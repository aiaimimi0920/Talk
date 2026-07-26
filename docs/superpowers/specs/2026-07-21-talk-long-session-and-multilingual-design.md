# Talk Long Session And Multilingual Design

## Goal

Fix four production issues without changing Talk's single-executable product shape:

1. Long transcript HUD content must scroll with the mouse wheel and retain the user's position.
2. Long transcripts and long recordings must not block the Win32 message loop or grow unbounded state.
3. Yellow-to-white correction latency must remove avoidable local queue and clipboard delays and expose timing evidence.
4. The product default ASR model must recognize Chinese, English, and Japanese, including mixed-language speech.

## Confirmed Root Causes

- `hud_window_proc` does not process `WM_MOUSEWHEEL`.
- The 48 ms recording timer resets the scroll offset to the bottom whenever scrollbar dragging is inactive.
- The same UI timer synchronously pumps every available PCM chunk while holding desktop shared state.
- Every live clipboard insertion can sleep for 500 ms before restoring the clipboard.
- Live correction has one unbounded FIFO consumer, so provider latency accumulates across segments.
- Correction requests have no queue/provider/apply timing split and no bounded stop drain.
- `LocalStreamingAsrLiveSession` retains all ASR events.
- The current product model has no Japanese tokens.

The official `sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10` archive was verified locally with the existing worker. Chinese, English, Japanese, and a concatenated Chinese-English-Japanese sample all produced multilingual output. The archive SHA-256 is `28044b67324f7f831689f0a3761473dd2ade380e93aa53f1dbcd479ef71c40d4`.

## Design

### HUD scrolling

Add persistent `auto_follow` state to the HUD overlay. New transcript content moves to the bottom only while auto-follow is enabled. Dragging or wheel-scrolling above the bottom disables auto-follow; reaching the bottom enables it again. Handle `WM_MOUSEWHEEL` with three logical lines per wheel notch and clamp the result to the current maximum offset.

### Long-session responsiveness

Keep the existing desktop/runtime ownership boundaries, but bound each UI-timer pump to one PCM chunk. The stop path remains responsible for draining remaining PCM. Retain only the latest bounded ASR event history required for final selection. Avoid recomputing transcript layout on waveform-only timer ticks; recompute when transcript text, geometry, or scroll position changes.

The live correction queue becomes bounded. Queue saturation must fail the individual speculative job cleanly rather than block the UI or grow memory without limit. Stop waits for live jobs only within a fixed drain budget and then continues with the final aggregate correction path.

### Correction latency and evidence

Run at most three provider correction requests concurrently. Preserve transcript display order through the tracker segment order; each result updates its matching segment by ID. Live clipboard insertions use a dedicated short restore-settle delay instead of the generic 500 ms final-insert delay.

For every correction job, emit non-secret timing evidence for queue wait, provider processing, apply processing, total elapsed time, and outcome. Logs must not include API keys or full transcript text.

### Multilingual model

Make the verified eight-language streaming Zipformer the product bootstrap default. The installer continues to download, SHA-256 verify, extract atomically, and validate required files. Product-managed daemon discovery prefers the new model while retaining the existing legacy model discovery path for explicit or already-installed engineering configurations.

### Release contract

The release remains a single product executable plus configuration:

- `Talk.exe`
- `talk.toml`

No PowerShell scripts, worker executables, model archives, or other files may appear beside them. The managed worker stays embedded in `Talk.exe`; the ASR model remains a first-run verified download.

## Test Strategy

- Pure tests for wheel direction, clamping, and auto-follow transitions.
- Runtime tests proving a live pump processes only its configured chunk budget and ASR history stays bounded.
- Insert tests for a configurable live restore-settle delay while preserving the 500 ms default behavior.
- Correction scheduling tests for queue bounds, three-way concurrency, timing outcomes, and drain timeout behavior.
- Model catalog and daemon launch tests for the multilingual model ID, SHA-256, and archive filenames.
- Existing workspace tests, Pester release tests, isolated-port desktop smoke, and final package-content verification.

## Non-Goals

- Do not modify Hook.
- Do not stop or replace the running r5 processes.
- Do not add a second user-visible executable.
- Do not commit or push unless the user explicitly requests it.
