//! A/B resampler harness: convert an input WAV to 16 kHz mono s16 either via
//! Talk's current band-limited front-end (`write_captured_wav`) or via the old
//! nearest-neighbour decimation, so the two can be compared through the ASR
//! benchmark (CER) on real 48 kHz audio.
//!
//! Usage: `cargo run -p talk-audio --example resample_wav -- <in.wav> <out.wav> <talk|nearest>`

use std::path::PathBuf;

use talk_audio::{write_captured_wav, AudioArtifact, CapturedAudioBuffer, WavSettings};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: resample_wav <in.wav> <out.wav> <talk|nearest>");
        std::process::exit(2);
    }
    let (input, output, mode) = (&args[1], &args[2], args[3].as_str());

    let mut reader = hound::WavReader::open(input).expect("open input wav");
    let spec = reader.spec();
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|sample| f32::from(sample.expect("read sample")) / 32768.0)
        .collect();
    let source = CapturedAudioBuffer {
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        samples,
    };

    match mode {
        "talk" => {
            let artifact = AudioArtifact::new(PathBuf::from(output), "audio/wav");
            write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
                .expect("write talk-resampled wav");
        }
        "nearest" => write_nearest_neighbor_16k_mono(&source, output),
        other => {
            eprintln!("unknown mode: {other} (expected talk|nearest)");
            std::process::exit(2);
        }
    }
    println!("wrote {output} via {mode}");
}

/// Reproduces the previous behaviour: nearest-neighbour source-frame picking
/// with no anti-aliasing filter, downmixed to mono at 16 kHz.
fn write_nearest_neighbor_16k_mono(source: &CapturedAudioBuffer, output: &str) {
    let target_rate = 16_000u32;
    let channels = usize::from(source.channels).max(1);
    let source_frames = source.samples.len() / channels;
    let target_frames = (source_frames as u128 * u128::from(target_rate)
        / u128::from(source.sample_rate_hz))
    .max(1) as usize;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: target_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output, spec).expect("create nearest wav");
    for target_index in 0..target_frames {
        let source_index = ((target_index as u128 * u128::from(source.sample_rate_hz))
            / u128::from(target_rate)) as usize;
        let source_index = source_index.min(source_frames.saturating_sub(1));
        let start = source_index * channels;
        let mono = source.samples[start..start + channels].iter().sum::<f32>() / channels as f32;
        let value = (mono.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        writer.write_sample(value).expect("write sample");
    }
    writer.finalize().expect("finalize nearest wav");
}
