# Talk Local Streaming ASR Protocol

Talk's local ASR boundary is a small, engine-neutral protocol. The desktop app
must not know whether the backend is sherpa-onnx Zipformer, Paraformer,
SenseVoice, faster-whisper, or another local engine. It only opens a localhost
streaming session, sends PCM audio chunks, and consumes partial/final text
events.

## Goals

- Keep the first visible text on the local path.
- Keep ASR engines hot in a long-lived service instead of cold-starting per
  dictation.
- Allow sherpa-onnx Zipformer or Paraformer to become the default without
  hard-coding engine names into the desktop interaction layer.
- Preserve the existing cloud correction model: local text appears first; cloud
  correction can patch the same target only when it is still safe.

## Transport

Initial production transport:

```text
WebSocket on loopback
Default endpoint: ws://127.0.0.1:53171/asr
Audio format: 16 kHz, mono, little-endian signed 16-bit PCM chunks
Control/event format: UTF-8 JSON messages
```

The service may also expose a JSONL stdio adapter for smoke tests, but the
desktop-facing contract is the WebSocket message schema below.

## Client messages

### start

```json
{
  "type": "start",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347",
  "sample_rate_hz": 16000,
  "channels": 1,
  "language": "zh"
}
```

`language` is optional. If omitted, the local service may auto-detect or use its
configured default.

### audio

```json
{
  "type": "audio",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347",
  "sequence": 42,
  "pcm_base64": "AAABAAD//w=="
}
```

`sequence` is monotonic per session. `pcm_base64` is raw PCM bytes, not WAV.

### stop

```json
{
  "type": "stop",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347"
}
```

### cancel

```json
{
  "type": "cancel",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347"
}
```

## Server messages

### ready

```json
{
  "type": "ready",
  "engine": "sherpa-onnx",
  "model": "zipformer-streaming-zh",
  "sample_rate_hz": 16000,
  "channels": 1
}
```

### partial

```json
{
  "type": "partial",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347",
  "segment_id": "seg-1",
  "text": "你好"
}
```

### final

```json
{
  "type": "final",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347",
  "segment_id": "seg-1",
  "text": "你好呀。"
}
```

### error

```json
{
  "type": "error",
  "session_id": "7ab87d30-9ef6-4e77-b5c1-bf11d4775347",
  "message": "model is not loaded"
}
```

## Talk config

```toml
[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "provider_text_processor"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
```

`local_asr = "external_command"` remains supported as a batch fallback. The
streaming service path is the target path for Typeless/OpenLess-like live
partial text.

When the standalone `Talk.exe` starts and no service is already listening at the
configured endpoint, it verifies the embedded runtime payload and extracts the
hidden `talk-local-asr-sherpa.exe` worker plus its Sherpa/ONNX DLLs into
`%LOCALAPPDATA%\Talk\runtime\<payload-hash>`. The worker is launched from that
content-addressed directory and is not visible beside `Talk.exe`.
The worker and its required native libraries are embedded in `Talk.exe` during
product publishing.

Talk then bootstraps the pinned Zipformer model in
`%LOCALAPPDATA%\Talk\models\sherpa-onnx`. Download, archive digest, safe
extraction, required-file validation, and atomic installation all happen before
the local route is marked ready. Bootstrap status is exposed as `downloading`,
`verifying`, `ready`, `fallback_cloud`, or `error`. If the local route is not
ready when a session starts, Talk records the per-session effective route and
uses cloud transcription when the configured provider supports it.

The following nested daemon table is an engineering/source-checkout override
for alternate models. It is not required in the product `talk.toml` for the
pinned Zipformer path:

```toml
[speculative.streaming_service.local_daemon]
mode = "sherpa-online"
model_family = "transducer"
model = "zipformer-bilingual-zh-en"
tokens = "C:/models/zipformer/tokens.txt"
encoder = "C:/models/zipformer/encoder-epoch-99-avg-1.int8.onnx"
decoder = "C:/models/zipformer/decoder-epoch-99-avg-1.onnx"
joiner = "C:/models/zipformer/joiner-epoch-99-avg-1.int8.onnx"
provider = "cpu"
num_threads = 2
sample_rate_hz = 16000
decoding_method = "greedy_search"
```

For streaming Paraformer:

```toml
[speculative.streaming_service.local_daemon]
mode = "sherpa-online"
model_family = "paraformer"
model = "paraformer-streaming-zh"
tokens = "C:/models/paraformer/tokens.txt"
encoder = "C:/models/paraformer/encoder.onnx"
decoder = "C:/models/paraformer/decoder.onnx"
provider = "cpu"
num_threads = 2
sample_rate_hz = 16000
decoding_method = "greedy_search"
```

The engineering desktop config validates that real mode has the required model
arguments, and the daemon performs final file-existence validation at startup.
See `docs/LOCAL_SHERPA_MODELS.md` for the source/CI installer and benchmark
workflow. Product users should let `Talk.exe` perform the first-run bootstrap.

## Reference dry-run daemon

`tools/talk-local-asr-sherpa` is the first protocol-compatible daemon skeleton.
The executable name points at the preferred production adapter family, but Talk
desktop still talks only to the engine-neutral WebSocket protocol above.

Dry-run start:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa -- `
  --bind 127.0.0.1:53171 `
  --dry-run-partial-text "你好" `
  --dry-run-text "你好。"
```

The dry-run daemon accepts `start`, any number of `audio` messages, and `stop`;
after the first valid audio chunk it can emit a `partial`, then it emits one
`final` message after `stop`. Real sherpa-onnx Zipformer / Paraformer loading
should keep the same transport and message schema.

Real sherpa-onnx online mode is explicit in source/CI because model packages are
large and alternate model selection is an engineering concern. The product
default is bootstrapped automatically as described above:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa -- `
  --bind 127.0.0.1:53171 `
  --mode sherpa-online `
  --model-family transducer `
  --model zipformer-bilingual-zh-en `
  --tokens C:\models\zipformer\tokens.txt `
  --encoder C:\models\zipformer\encoder-epoch-99-avg-1.int8.onnx `
  --decoder C:\models\zipformer\decoder-epoch-99-avg-1.onnx `
  --joiner C:\models\zipformer\joiner-epoch-99-avg-1.int8.onnx `
  --provider cpu `
  --num-threads 2
```

Paraformer uses the same protocol but does not need a joiner:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa -- `
  --bind 127.0.0.1:53171 `
  --mode sherpa-online `
  --model-family paraformer `
  --model paraformer-streaming-zh `
  --tokens C:\models\paraformer\tokens.txt `
  --encoder C:\models\paraformer\encoder.onnx `
  --decoder C:\models\paraformer\decoder.onnx `
  --provider cpu `
  --num-threads 2
```

On Windows the Talk build currently links `sherpa-onnx` in shared mode. Product
publishing embeds `sherpa-onnx-c-api.dll`, `sherpa-onnx-cxx-api.dll`,
`onnxruntime.dll`, and `onnxruntime_providers_shared.dll` in the `Talk.exe`
payload; the runtime extractor places them beside the hidden worker only under
the per-user runtime cache.

## Engine adapters

Talk should ship adapters behind this protocol, not protocol forks:

- `talk-local-asr-sherpa.exe` for sherpa-onnx Zipformer and Paraformer.
- `talk-local-asr-sensevoice.exe` for batch/fallback finalization.
- `talk-local-asr-whisper.exe` for optional high-quality local fallback.

Each adapter must:

1. Load the model once at process start.
2. Respond with `ready` only after the model is usable.
3. Emit low-latency `partial` events during recording.
4. Emit a stable `final` after `stop`.
5. Keep errors session-scoped and recoverable where possible.

## Current development slice

The current implementation has:

1. a loopback WebSocket client in `talk-client`;
2. a packaged local ASR daemon launcher in `talk-desktop`;
3. a real `sherpa-online` adapter path in `talk-local-asr-sherpa`;
4. a typed desktop config path for passing model arguments into that daemon.

The remaining production-hardening work is real speech benchmarking against the
installed Zipformer and Paraformer packages.
