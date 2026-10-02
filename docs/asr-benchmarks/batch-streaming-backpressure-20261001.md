# Buffered-recording library deadline

This hardens the public `run_local_streaming_asr_service_from_recording` API.
It is not an established ordinary desktop regression: at baseline
`7a35ae7be5c86fb245f8e7f1cac53e7a9bc42367`, streaming-enabled desktop recordings
always own a live session. Startup failure cancels before installing the active
recording; live pump failure retains its terminal outer session. The defensive
missing-session branch is therefore not produced by normal desktop construction.
The normal Talk CLI does not call this helper either.

The public batch helper nevertheless has a reproducible liveness problem: its
PCM and Stop sends can block before the configured final-result timeout starts.
The new guard bounds that post-Ready transfer using `final_timeout_ms` and keeps
the full separate final-response window. A failed or cancelled operation drops
its connection and returns an error, without retrying audio or returning saved
partials as success. The caller continues to own capture and should freeze it
before invoking the helper. PCM validation, order, language and wire format are
unchanged, as are the public signature and configuration defaults/limits.

## Evidence

[JSON evidence and source hashes](batch-streaming-backpressure-20261001.json)
pin the baseline and exact test inputs. Both revisions ran the same public probe
on Linux x86_64 with Rust 1.95.0 debug builds and shared CPU resources. The peer
completed Start/Ready, requested a 1,024-byte TCP receive buffer, then stopped
reading. The silent RecordingSession contained 90 seconds of 16 kHz mono s16le
PCM (2,880,000 bytes), matching the checked-in streaming example's recording
limit. No microphone, speech model, private recording, or external provider was
used.

| Same public probe | Result with 100 ms final timeout |
| --- | --- |
| Baseline | Still pending when its 750 ms watchdog expired at 751.939 ms |
| Candidate | Returned a transfer-timeout error at 165.370 ms |

These are individual whole-call observations, including connect/Ready work and
synchronous encoding. The baseline is a lower bound, not a measured natural
completion time. This is neither an ASR speed percentage nor an accuracy result.

The focused regression suite also observed a 3,200-byte-chunk transfer timeout
at 153.050 ms with a 150 ms transfer budget. A controlled 1.1 s PCM-preparation
step plus 1.5 s final-response delay succeeded in 2643.662 ms with separate 2 s
phase budgets. Sharing one budget makes that test fail at about 2 s. Relaxing
the transfer deadline to 10 s makes the stalled-transfer test fail at its 2 s
watchdog. The intended source was restored before passing validation.

Eight regressions cover healthy format/order/language/events, transfer timeout,
external cancellation, empty input, mismatched format, partial-only final
response timeout, preservation of the final window, and the public API with a
buffered RecordingSession. They check that timeout/cancellation sends no replay. The cancellation fixture
waits for a first-drain notification and verifies both drained and remaining PCM.
For a single large frame, Windows may buffer all PCM and Stop before blocking;
that public test accepts either a transfer or final-response timeout. A bounded
small-chunk source separately tests the transfer guard without relying on the
single-frame assumption.

## Reproduce

```sh
cargo test -p talk-runtime --lib batch_ --locked -- --nocapture
cargo test -p talk-runtime --test batch_backpressure_probe --locked -- --ignored --nocapture
```

To compare the baseline, copy only
`crates/talk-runtime/tests/batch_backpressure_probe.rs` into a checkout of the
baseline commit and run the second command. The probe reports both possible
outcomes rather than treating an OS-specific buffer size as a guarantee.

Formatting, all-target checking, 498 workspace tests, and Clippy passed locally
(4 tests intentionally ignored; existing warnings remain). Platform CI must
verify the final PR separately. No Windows microphone/UI session was exercised.
Async deadlines can overshoot during synchronous work or descheduling; this
change does not make PCM preparation preemptible. With default settings, the
post-Ready transfer can wait up to 7 s and the final response another 7 s.
Startup connect/Ready behavior, raw client sends and the benchmark CLI's direct
send path are unchanged.
