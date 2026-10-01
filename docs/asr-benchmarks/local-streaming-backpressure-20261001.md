# Live local-ASR backpressure and failure boundaries

Baseline: `6bbca5d31a8d21775101ca749dbcc7fd71d6a61a`. The adjacent
[JSON evidence](local-streaming-backpressure-20261001.json) pins the tested source
files by SHA-256 and records the raw observations. No microphone, ASR model,
private recording, or external provider was used.

## Reproduction

The Windows 48 ms HUD timer holds desktop shared state while synchronously
calling the live audio pump. On the baseline, that pump drains PCM and awaits
WebSocket sends before its 1 ms receive-idle budget starts. An unresponsive local
peer can therefore defer UI handling and Stop indefinitely. A continuously
producing source or non-idle partial stream can also extend the same operation.

A loopback peer completed Start/Ready, then stopped reading. Its requested TCP
receive buffer was 1,024 bytes so a bounded synthetic workload reaches
backpressure. At 16 kHz mono, each 3,200-byte s16le chunk represents 100 ms. The
raw client probe stalled at sequence 371 after 1,187,200 prior PCM bytes. It was
still pending when the external 500 ms watchdog expired (501.356 ms). This is a
lower bound on that wait, not an observed natural timeout. The raw client send
API itself is unchanged by this patch.

The live runtime now sends at most one current snapshot per pump and enforces a
configurable whole-pump deadline. Its default is 100 ms, roughly two HUD ticks.
Client ownership leaves the session while the operation is pending and returns
only on success. A failed or cancelled operation closes the socket permanently;
an already queued frame is ambiguous and must not be replayed. Desktop failure
handling enters Stop before its stale-partial promotion path. Stop still freezes
capture first, then refuses a failed session instead of returning saved partials.

## Observations

Rust 1.95.0 debug tests on shared Linux x86_64, no exclusive CPU reservation.
Independent tests ran concurrently. These are individual liveness observations,
not latency percentiles or real recognition speed/accuracy measurements.

| Case | Budget | Observed outcome |
| --- | --- | --- |
| Stalled live pump, 3,200-byte chunks | 100 ms | Terminal error in 100.867 ms, at the 372nd drained chunk |
| Stalled final PCM drain | 150 ms transfer budget | Error in 151.044 ms; capture stayed stopped |
| Cancel with an existing blocked write | 100 ms | Error in 101.420 ms and connection dropped |
| Two healthy small live pumps | 100 ms each | Both succeeded in 4.805 ms total |
| Slow valid peer with 2 MiB accumulated PCM | 2 s configured pump budget | Succeeded in 503.373 ms |
| Slow final transfer plus 1.7 s final-response delay | Separate 2 s transfer and 2 s response budgets | Succeeded in 2456.236 ms total |

The last case checks that the existing full final-response window is preserved.
Stop now bounds its transfer phase separately using `final_timeout_ms`, so total
network waiting can reach twice that setting. A slow recognizer returning after
150 ms also succeeds with the default 100 ms live pump: the pump does not wait
for the recognizer to finish.

Tests check repeated pump/Stop after failure, external cancellation of pump and
Stop, no duplicate audio sequence or new Stop/Cancel after terminal failure,
one snapshot per pump, continuous partial messages, and existing callback-tail
ordering. A control with the pump deadline relaxed to 10 s fails the regression
at its 1 s watchdog. The intended code was restored and checked afterward.

The Cancel guard test deliberately interrupts the low-level send to leave a
pending write before invoking Cancel; production live-pump ownership prevents
that socket from being reused. This is a targeted transport fixture, not a
normal user flow.

## Reproduce locally

```sh
cargo test -p talk-client --test backpressure_probe --locked -- --ignored --nocapture
cargo test -p talk-runtime --lib live_ --locked -- --nocapture
cargo test -p talk-core --test config_contract --locked
```

The client probe can also run on the baseline after copying only
`crates/talk-client/tests/backpressure_probe.rs` into that checkout. Fixture
sources and tested production files are identified in the JSON hash manifest.

The synchronous pump can still pause the UI up to its configured budget and
overshoot while converting/serializing PCM or while the process is descheduled.
A slow valid peer or a scheduler pause can fail a session at the 100 ms default;
raising `pump_timeout_ms` trades responsiveness for tolerance. This patch does
not make the desktop fully nonblocking. TCP may deliver the single ambiguous
queued frame before close, but the failed session sends no replay or subsequent
control frame. Already inserted text cannot be recalled. The standalone batch
helper and low-level client sends are outside these new live-session bounds.
No real Windows microphone/UI session or model-quality benchmark was run here.
