use super::*;
use std::hint::black_box;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn busy_capture_buffer_drops_callback_without_waiting() {
    for frames in [256, 441, 480, 960, 1024, 2048] {
        for channels in [1, 2] {
            let samples = Arc::new(Mutex::new(Vec::new()));
            let reader = samples.lock().unwrap();
            let callback_samples = Arc::clone(&samples);
            let (sender, receiver) = mpsc::channel();
            let callback = std::thread::spawn(move || {
                let admitted = append_captured_input_samples(
                    vec![0.25; frames * channels].into_iter(),
                    &callback_samples,
                    48_000 * channels,
                );
                sender.send(admitted).unwrap();
            });
            let outcome = receiver.recv_timeout(Duration::from_secs(1));
            drop(reader);
            callback.join().unwrap();
            let admitted_busy = outcome.unwrap();
            assert_eq!(
                admitted_busy, 0,
                "callback must not wait for the reader lock"
            );
            assert!(samples.lock().unwrap().is_empty());
            let admitted_available = append_captured_input_samples(
                vec![0.25; frames * channels].into_iter(),
                &samples,
                48_000 * channels,
            );
            assert_eq!(admitted_available, frames * channels);
            println!("forced_callback,frames={frames},channels={channels},admitted_busy={admitted_busy},admitted_available={admitted_available}");
        }
    }
}

#[test]
fn input_callback_keeps_capture_memory_bounded() {
    let samples = Mutex::new(vec![0.0; 6]);
    assert_eq!(
        append_captured_input_samples([0.1, 0.2, 0.3, 0.4].into_iter(), &samples, 8),
        2
    );
    assert_eq!(
        samples.lock().unwrap().as_slice(),
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 0.2]
    );
    assert_eq!(
        append_captured_input_samples([0.5].into_iter(), &samples, 8),
        0
    );
    assert_eq!(samples.lock().unwrap().len(), 8);
}

fn summary(label: &str, rate: u32, channels: u16, mut samples: Vec<u128>) {
    samples.sort_unstable();
    println!(
        "{label},rate={rate},channels={channels},iterations={},p50_ns={},p95_ns={},max_ns={}",
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100],
        samples.last().unwrap()
    );
}

#[test]
fn snapshots_preserve_summary_values_and_release_the_capture_lock() {
    for channels in [1, 2, 6] {
        for frames in [0, 1, 17, 2048, 10_000] {
            let original = (0..frames * usize::from(channels))
                .map(|i| ((i * 7 % 101) as f32 - 50.0) / 50.0)
                .collect::<Vec<_>>();
            let samples = Mutex::new(original.clone());
            for trailing_frames in [0, 1, 127, 8640] {
                let snapshot =
                    snapshot_recent_capture_samples(&samples, channels, trailing_frames).unwrap();
                assert!(snapshot.len() <= trailing_frames * usize::from(channels));
                assert_eq!(snapshot.len() % usize::from(channels), 0);
                assert_eq!(
                    summarize_recent_interleaved_audio_level(&snapshot, channels, trailing_frames)
                        .unwrap(),
                    summarize_recent_interleaved_audio_level(&original, channels, trailing_frames)
                        .unwrap()
                );
                for buckets in [0, 1, 9, 32, 257] {
                    assert_eq!(
                        summarize_recent_interleaved_audio_waveform(
                            &snapshot,
                            channels,
                            trailing_frames,
                            buckets
                        )
                        .unwrap(),
                        summarize_recent_interleaved_audio_waveform(
                            &original,
                            channels,
                            trailing_frames,
                            buckets
                        )
                        .unwrap()
                    );
                }
                // Keeping/processing the UI snapshot must not exclude input.
                assert_eq!(
                    append_captured_input_samples(
                        std::iter::repeat_n(0.5, usize::from(channels)),
                        &samples,
                        original.len() + usize::from(channels)
                    ),
                    usize::from(channels)
                );
                *samples.lock().unwrap() = original.clone();
            }
        }
    }
}

#[test]
fn snapshot_rejects_invalid_frames_and_keeps_empty_capture_empty() {
    let samples = Mutex::new(vec![0.0; 3]);
    assert!(snapshot_recent_capture_samples(&samples, 0, 1).is_err());
    assert!(snapshot_recent_capture_samples(&samples, 2, 1).is_err());
    assert!(snapshot_recent_capture_samples(&samples, 2, usize::MAX).is_err());
    assert!(snapshot_recent_capture_samples(&samples, 1, usize::MAX).is_err());
    assert!(snapshot_recent_capture_samples(&samples, 2, 0)
        .unwrap()
        .is_empty());
    assert!(
        snapshot_recent_capture_samples(&Mutex::new(Vec::new()), 2, 8640)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn poisoned_capture_buffer_rejects_snapshots_and_does_not_block_input() {
    let samples = Arc::new(Mutex::new(vec![0.25; 4]));
    let poison = Arc::clone(&samples);
    assert!(std::thread::spawn(move || {
        let _guard = poison.lock().unwrap();
        panic!("fixture lock poisoning");
    })
    .join()
    .is_err());
    assert!(snapshot_recent_capture_samples(&samples, 1, 4).is_err());
    assert!(snapshot_recent_capture_samples(&samples, 1, 0).is_err());
    assert_eq!(
        append_captured_input_samples([0.5].into_iter(), &samples, 8),
        0
    );
}

#[test]
#[ignore = "optimized, synthetic critical-section timing probe; no microphone"]
fn profile_capture_reader_lock_duration() {
    for rate in [44_100, 48_000] {
        for channels in [1, 2] {
            let data = (0..rate as usize * usize::from(channels) * 2)
                .map(|i| (i % 97) as f32 / 97.0)
                .collect::<Vec<_>>();
            let samples = Mutex::new(data);
            let mut level = Vec::new();
            let mut waveform = Vec::new();
            let mut level_snapshot = Vec::new();
            let mut waveform_snapshot = Vec::new();
            let mut level_total = Vec::new();
            let mut waveform_total = Vec::new();
            for iteration in 0..10_100 {
                let locked = samples.lock().unwrap();
                let start = Instant::now();
                let value = summarize_recent_interleaved_audio_level(
                    &locked,
                    channels,
                    live_level_trailing_frames(rate),
                )
                .unwrap();
                let elapsed = start.elapsed().as_nanos();
                drop(locked);
                black_box(value);
                if iteration >= 100 {
                    level.push(elapsed);
                }
                let locked = samples.lock().unwrap();
                let start = Instant::now();
                let value = summarize_recent_interleaved_audio_waveform(
                    &locked,
                    channels,
                    live_waveform_trailing_frames(rate),
                    9,
                )
                .unwrap();
                let elapsed = start.elapsed().as_nanos();
                drop(locked);
                black_box(value);
                if iteration >= 100 {
                    waveform.push(elapsed);
                }
                let start = Instant::now();
                let snapshot = snapshot_recent_capture_samples(
                    &samples,
                    channels,
                    live_level_trailing_frames(rate),
                )
                .unwrap();
                let elapsed = start.elapsed().as_nanos();
                black_box(
                    summarize_recent_interleaved_audio_level(
                        &snapshot,
                        channels,
                        live_level_trailing_frames(rate),
                    )
                    .unwrap(),
                );
                let total = start.elapsed().as_nanos();
                if iteration >= 100 {
                    level_snapshot.push(elapsed);
                    level_total.push(total);
                }
                let start = Instant::now();
                let snapshot = snapshot_recent_capture_samples(
                    &samples,
                    channels,
                    live_waveform_trailing_frames(rate),
                )
                .unwrap();
                let elapsed = start.elapsed().as_nanos();
                black_box(
                    summarize_recent_interleaved_audio_waveform(
                        &snapshot,
                        channels,
                        live_waveform_trailing_frames(rate),
                        9,
                    )
                    .unwrap(),
                );
                let total = start.elapsed().as_nanos();
                if iteration >= 100 {
                    waveform_snapshot.push(elapsed);
                    waveform_total.push(total);
                }
            }
            summary("legacy_level_lock", rate, channels, level);
            summary("legacy_waveform_lock", rate, channels, waveform);
            // Full snapshot includes allocation outside the lock and lock/unlock:
            // this is a conservative upper bound on the new critical section.
            summary(
                "candidate_level_snapshot_upper_bound",
                rate,
                channels,
                level_snapshot,
            );
            summary(
                "candidate_waveform_snapshot_upper_bound",
                rate,
                channels,
                waveform_snapshot,
            );
            summary("candidate_level_total", rate, channels, level_total);
            summary("candidate_waveform_total", rate, channels, waveform_total);
        }
    }
}
