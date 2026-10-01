# Capture reader contention (2026-10-01)

The native input callback uses `try_lock` to avoid waiting on the capture
buffer. If a reader holds that mutex, the callback discards its entire incoming
buffer. The desktop waveform reader previously kept the mutex while calculating
peaks over 180 ms of audio. The public level reader similarly calculated peak
and RMS over 120 ms while holding the mutex.

This change copies only that bounded window, releases the mutex, and then
computes the summary. Allocation happens before locking, uses fallible reserve,
and the reserved capacity covers the copy. The callback remains nonblocking;
recording limits, PCM conversion and Stop ordering are unchanged. This reduces
one source of contention, **not all possible dropped audio**.

## Reproduction and safety checks

Baseline: `c41f6976858e6476c05f5fba89d93e4d5c2c17ed`.

The callback append implementation was extracted without changing its lock or
sample-limit policy so its actual logic can be tested on Linux. A synchronized
reader holds the mutex while another thread offers 256, 441, 480, 960, 1024 or
2048 frames in mono/stereo. All twelve cases return immediately without admitting
samples. Repeating each callback after unlocking admits every sample. One such
collision loses roughly 5.3–46.4 ms of audio at 44.1/48 kHz, depending on the
callback size. This is a **forced-contention reproduction**, not an estimate of
how often a real microphone experiences it.

Additional tests cover:

- Exact level/waveform equivalence for empty, short and long mono/stereo/six-channel buffers, including nine HUD buckets
- Appending input while an owned snapshot remains alive
- Maximum capture sample counts and full-buffer behavior
- Zero/invalid channels, partial frames, empty windows, size overflow and unrepresentable allocation requests
- Poisoned locks returning errors for readers and never blocking input

The desktop calls `current_waveform(9)`; the final profile uses nine buckets.
The level API is profiled too, although the desktop currently uses waveform
summaries rather than that API. An initial 32-bucket characterization was not
used for the results below.

## Optimized measurements

[Machine-readable observations](capture-reader-contention-20261001.json) record
source hashes, actual forced-callback counts, p50/p95/max timings and method.
The paired probe runs with Rust 1.95.0 in release mode on a shared Linux x86_64
cloud executor (AMD EPYC 9V74, nine CPUs in the process affinity). Each condition
uses two seconds of deterministic sample values, 100 warmup iterations and
10,000 measured iterations. Both implementations run in the same process and
condition; ordering is fixed and recorded in the JSON.

All figures below are median microseconds:

| Input | Previous level calculation under lock | New level snapshot, full call | Previous waveform calculation under lock | New waveform snapshot, full call |
| --- | ---: | ---: | ---: | ---: |
| 44.1 kHz mono | 5.308 | 0.321 | 13.040 | 0.621 |
| 44.1 kHz stereo | 10.576 | 0.831 | 16.986 | 1.182 |
| 48 kHz mono | 5.778 | 0.401 | 14.181 | 0.671 |
| 48 kHz stereo | 11.517 | 0.881 | 18.478 | 1.292 |

The new columns time the **whole snapshot helper**, including allocation outside
the lock and lock/unlock overhead. They are conservative upper bounds on its
critical-section duration, not direct measurements of only the locked copy.
The old columns time summary calculation while the guard is held and exclude
lock acquisition and deallocation of a returned waveform vector.

The candidate still performs the same summary computation afterward. Its total
probe medians range from 5.638 to 19.780 µs, reflecting additional copying rather
than an overall CPU reduction. The total waveform probe also drops its output
inside that measurement, so it is not a precisely matched end-to-end CPU
comparison. At 48 kHz stereo, requested temporary storage is 45 KiB for the level
window or 67.5 KiB for the waveform window; it does not grow with recording length.

Shared scheduling produced millisecond outliers. These observations are not
hard real-time deadlines or portable Windows performance guarantees. A callback
that collides with the shorter copy lock still follows the original drop policy.
No model inference, CER/WER, real microphone drop frequency or end-to-end ASR
latency was measured, and no improvement in those metrics is claimed.

## Run the checks

```bash
cargo test -p talk-audio --release --locked
cargo test -p talk-audio --release --locked --lib \
  busy_capture_buffer_drops_callback_without_waiting -- --nocapture
cargo test -p talk-audio --release --locked --lib \
  profile_capture_reader_lock_duration -- --ignored --nocapture
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked
```

The profiling test is opt-in and has no timing pass/fail thresholds. It uses no
microphone, audio fixture download, model download or external inference service.
Windows CI remains necessary to compile the actual native capture call sites.
