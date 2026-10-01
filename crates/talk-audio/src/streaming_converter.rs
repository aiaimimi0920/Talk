use std::collections::VecDeque;

const DOWNMIX_DECISION_MIN_PEAK: f32 = 0.01;
const MAX_AVERAGE_TO_STRONGEST_ENERGY_RATIO: f64 = 0.25;

/// Converts sequential native capture frames into one canonical streaming PCM
/// format while retaining resampler phase and filter history across callbacks.
pub(crate) struct StreamingCaptureConverter {
    source_channels: usize,
    target_channels: usize,
    selected_downmix_channel: Option<Option<usize>>,
    interleaved_carry: Vec<f32>,
    mono_scratch: Vec<f32>,
    resampled_scratch: Vec<f32>,
    channel_energy_scratch: Vec<f64>,
    resampler: StreamingMonoResampler,
}

impl StreamingCaptureConverter {
    pub(crate) fn new(
        source_sample_rate_hz: u32,
        source_channels: u16,
        target_sample_rate_hz: u32,
        target_channels: u16,
    ) -> Result<Self, String> {
        if source_sample_rate_hz == 0 {
            return Err("source sample rate must be greater than 0".to_string());
        }
        if source_channels == 0 {
            return Err("source channel count must be greater than 0".to_string());
        }
        if target_sample_rate_hz == 0 {
            return Err("target sample rate must be greater than 0".to_string());
        }
        if target_channels == 0 {
            return Err("target channel count must be greater than 0".to_string());
        }

        Ok(Self {
            source_channels: usize::from(source_channels),
            target_channels: usize::from(target_channels),
            selected_downmix_channel: (source_channels == 1).then_some(None),
            interleaved_carry: Vec::new(),
            mono_scratch: Vec::new(),
            resampled_scratch: Vec::new(),
            channel_energy_scratch: vec![0.0; usize::from(source_channels)],
            resampler: StreamingMonoResampler::new(source_sample_rate_hz, target_sample_rate_hz),
        })
    }

    pub(crate) fn push_interleaved(
        &mut self,
        input: &[f32],
        output: &mut Vec<f32>,
    ) -> Result<(), String> {
        output.clear();
        if input.is_empty() {
            return Ok(());
        }

        self.interleaved_carry.extend_from_slice(input);
        let aligned_sample_count =
            self.interleaved_carry.len() - (self.interleaved_carry.len() % self.source_channels);
        if aligned_sample_count == 0 {
            return Ok(());
        }

        let aligned = &self.interleaved_carry[..aligned_sample_count];
        if self.selected_downmix_channel.is_none() {
            self.selected_downmix_channel = select_phase_safe_channel(
                aligned,
                self.source_channels,
                &mut self.channel_energy_scratch,
            );
        }
        let selected_channel = self.selected_downmix_channel.flatten();

        self.mono_scratch.clear();
        self.mono_scratch
            .reserve(aligned_sample_count / self.source_channels);
        self.mono_scratch
            .extend(aligned.chunks_exact(self.source_channels).map(|frame| {
                selected_channel.map_or_else(
                    || frame.iter().copied().sum::<f32>() / self.source_channels as f32,
                    |channel| frame[channel],
                )
            }));
        self.interleaved_carry.drain(..aligned_sample_count);

        self.resampler
            .push(&self.mono_scratch, &mut self.resampled_scratch);
        duplicate_channels(&self.resampled_scratch, self.target_channels, output);
        Ok(())
    }

    pub(crate) fn finish(&mut self, output: &mut Vec<f32>) -> Result<(), String> {
        output.clear();
        if !self.interleaved_carry.is_empty() {
            return Err(format!(
                "native capture ended with {} sample(s) that do not form a complete {}-channel frame",
                self.interleaved_carry.len(),
                self.source_channels
            ));
        }

        self.resampler.finish(&mut self.resampled_scratch);
        duplicate_channels(&self.resampled_scratch, self.target_channels, output);
        Ok(())
    }
}

fn duplicate_channels(mono: &[f32], target_channels: usize, output: &mut Vec<f32>) {
    output.reserve(mono.len().saturating_mul(target_channels));
    for sample in mono.iter().copied() {
        output.extend(std::iter::repeat_n(sample, target_channels));
    }
}

/// Returns `None` while the chunk is still below the speech/noise decision
/// threshold. `Some(None)` selects normal averaging; `Some(Some(index))`
/// selects a single channel when averaging would destructively cancel it.
fn select_phase_safe_channel(
    interleaved: &[f32],
    channels: usize,
    channel_energy: &mut Vec<f64>,
) -> Option<Option<usize>> {
    if channels <= 1 {
        return Some(None);
    }

    channel_energy.clear();
    channel_energy.resize(channels, 0.0);
    let mut average_energy = 0.0_f64;
    let mut peak = 0.0_f32;
    for frame in interleaved.chunks_exact(channels) {
        let mut sum = 0.0_f64;
        for (channel_index, sample) in frame.iter().copied().enumerate() {
            peak = peak.max(sample.abs());
            let sample = f64::from(sample);
            channel_energy[channel_index] += sample * sample;
            sum += sample;
        }
        let average = sum / channels as f64;
        average_energy += average * average;
    }

    if peak < DOWNMIX_DECISION_MIN_PEAK {
        return None;
    }
    let (strongest_channel, strongest_energy) = channel_energy
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))?;
    if strongest_energy <= f64::EPSILON
        || average_energy > strongest_energy * MAX_AVERAGE_TO_STRONGEST_ENERGY_RATIO
    {
        Some(None)
    } else {
        Some(Some(strongest_channel))
    }
}

struct StreamingMonoResampler {
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
    source: VecDeque<f32>,
    source_base_index: u64,
    total_source_samples: u64,
    next_target_sample: u64,
}

impl StreamingMonoResampler {
    fn new(source_sample_rate_hz: u32, target_sample_rate_hz: u32) -> Self {
        Self {
            source_sample_rate_hz,
            target_sample_rate_hz,
            source: VecDeque::new(),
            source_base_index: 0,
            total_source_samples: 0,
            next_target_sample: 0,
        }
    }

    fn push(&mut self, input: &[f32], output: &mut Vec<f32>) {
        output.clear();
        if input.is_empty() {
            return;
        }

        if self.source_sample_rate_hz == self.target_sample_rate_hz {
            output.extend_from_slice(input);
            self.total_source_samples =
                self.total_source_samples.saturating_add(input.len() as u64);
            self.next_target_sample = self.total_source_samples;
            self.source_base_index = self.total_source_samples;
            return;
        }

        self.source.extend(input.iter().copied());
        self.total_source_samples = self.total_source_samples.saturating_add(input.len() as u64);
        self.emit_available(false, output);
    }

    fn finish(&mut self, output: &mut Vec<f32>) {
        output.clear();
        if self.source_sample_rate_hz == self.target_sample_rate_hz {
            return;
        }
        self.emit_available(true, output);
        self.source.clear();
        self.source_base_index = self.total_source_samples;
    }

    fn emit_available(&mut self, final_input: bool, output: &mut Vec<f32>) {
        if self.total_source_samples == 0 {
            return;
        }

        let final_target_count = final_input.then(|| {
            ((u128::from(self.total_source_samples) * u128::from(self.target_sample_rate_hz))
                / u128::from(self.source_sample_rate_hz))
            .max(1) as u64
        });

        if self.target_sample_rate_hz > self.source_sample_rate_hz {
            self.emit_upsampled(final_target_count, output);
        } else {
            self.emit_downsampled(final_target_count, output);
        }
        self.drop_unneeded_history();
    }

    fn emit_upsampled(&mut self, final_target_count: Option<u64>, output: &mut Vec<f32>) {
        let step = f64::from(self.source_sample_rate_hz) / f64::from(self.target_sample_rate_hz);
        loop {
            if final_target_count.is_some_and(|count| self.next_target_sample >= count) {
                break;
            }
            let source_position = self.next_target_sample as f64 * step;
            let lower = source_position.floor() as u64;
            if final_target_count.is_none() && lower.saturating_add(1) >= self.total_source_samples
            {
                break;
            }
            let upper = lower
                .saturating_add(1)
                .min(self.total_source_samples.saturating_sub(1));
            let fraction = (source_position - lower as f64) as f32;
            let lower_sample = self.sample_at(lower);
            let upper_sample = self.sample_at(upper);
            output.push(lower_sample * (1.0 - fraction) + upper_sample * fraction);
            self.next_target_sample = self.next_target_sample.saturating_add(1);
        }
    }

    fn emit_downsampled(&mut self, final_target_count: Option<u64>, output: &mut Vec<f32>) {
        let step = f64::from(self.source_sample_rate_hz) / f64::from(self.target_sample_rate_hz);
        let radius = step.ceil().max(1.0);
        let window = radius + 1.0;
        loop {
            if final_target_count.is_some_and(|count| self.next_target_sample >= count) {
                break;
            }
            let center = self.next_target_sample as f64 * step;
            let required_right = (center + radius).ceil() as u64;
            if final_target_count.is_none() && required_right >= self.total_source_samples {
                break;
            }

            let first = (center - radius).floor() as i64;
            let last = (center + radius).ceil() as i64;
            let mut weighted_sum = 0.0_f64;
            let mut weight_total = 0.0_f64;
            for tap in first..=last {
                let distance = (tap as f64 - center).abs();
                if distance >= window {
                    continue;
                }
                let weight = 0.5 * (1.0 + (std::f64::consts::PI * distance / window).cos());
                let clamped = tap.clamp(0, self.total_source_samples as i64 - 1) as u64;
                weighted_sum += f64::from(self.sample_at(clamped)) * weight;
                weight_total += weight;
            }
            output.push((weighted_sum / weight_total) as f32);
            self.next_target_sample = self.next_target_sample.saturating_add(1);
        }
    }

    fn sample_at(&self, absolute_index: u64) -> f32 {
        let local_index = absolute_index
            .checked_sub(self.source_base_index)
            .expect("resampler requested discarded input history");
        self.source[local_index as usize]
    }

    fn drop_unneeded_history(&mut self) {
        if self.source.is_empty() {
            return;
        }
        let step = f64::from(self.source_sample_rate_hz) / f64::from(self.target_sample_rate_hz);
        let radius = if self.target_sample_rate_hz < self.source_sample_rate_hz {
            step.ceil().max(1.0)
        } else {
            0.0
        };
        let minimum_needed = (self.next_target_sample as f64 * step - radius)
            .floor()
            .max(0.0) as u64;
        while self.source_base_index < minimum_needed && !self.source.is_empty() {
            self.source.pop_front();
            self.source_base_index = self.source_base_index.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamingCaptureConverter;

    fn reference_downsample(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
        let target_frames =
            ((input.len() as u128 * u128::from(target_rate)) / u128::from(source_rate)) as usize;
        let step = f64::from(source_rate) / f64::from(target_rate);
        let radius = step.ceil().max(1.0);
        let window = radius + 1.0;
        let last_index = input.len() - 1;
        (0..target_frames)
            .map(|target_index| {
                let center = target_index as f64 * step;
                let first = (center - radius).floor() as isize;
                let last = (center + radius).ceil() as isize;
                let mut weighted_sum = 0.0_f64;
                let mut weight_total = 0.0_f64;
                for tap in first..=last {
                    let distance = (tap as f64 - center).abs();
                    if distance >= window {
                        continue;
                    }
                    let weight = 0.5 * (1.0 + (std::f64::consts::PI * distance / window).cos());
                    let index = tap.clamp(0, last_index as isize) as usize;
                    weighted_sum += f64::from(input[index]) * weight;
                    weight_total += weight;
                }
                (weighted_sum / weight_total) as f32
            })
            .collect()
    }

    fn convert_in_chunks(
        input: &[f32],
        source_rate: u32,
        source_channels: u16,
        target_rate: u32,
        chunk_samples: usize,
    ) -> Vec<f32> {
        let mut converter =
            StreamingCaptureConverter::new(source_rate, source_channels, target_rate, 1)
                .expect("create converter");
        let mut output = Vec::new();
        let mut combined = Vec::new();
        for chunk in input.chunks(chunk_samples) {
            converter
                .push_interleaved(chunk, &mut output)
                .expect("convert chunk");
            combined.extend_from_slice(&output);
        }
        converter.finish(&mut output).expect("finish converter");
        combined.extend_from_slice(&output);
        combined
    }

    #[test]
    fn stateful_downsampling_matches_whole_buffer_across_irregular_chunks() {
        for source_rate in [44_100_u32, 48_000_u32] {
            let input = (0..source_rate as usize)
                .map(|index| {
                    let phase = index as f32 * 440.0 * std::f32::consts::TAU / source_rate as f32;
                    phase.sin() * 0.7
                })
                .collect::<Vec<_>>();
            let actual = convert_in_chunks(&input, source_rate, 1, 16_000, 997);
            let expected = reference_downsample(&input, source_rate, 16_000);

            assert_eq!(actual.len(), expected.len());
            let max_error = actual
                .iter()
                .zip(&expected)
                .map(|(left, right)| (left - right).abs())
                .fold(0.0_f32, f32::max);
            assert!(
                max_error <= 1.0e-6,
                "source_rate={source_rate}, max_error={max_error}"
            );
        }
    }

    #[test]
    fn stateful_converter_preserves_all_same_rate_samples_across_partial_frames() {
        let stereo = (0..5_001)
            .flat_map(|index| {
                let sample = ((index % 31) as f32 - 15.0) / 15.0;
                [sample, sample]
            })
            .collect::<Vec<_>>();
        let actual = convert_in_chunks(&stereo, 16_000, 2, 16_000, 333);
        let expected = stereo
            .chunks_exact(2)
            .map(|frame| frame[0])
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn stateful_converter_keeps_antiphase_speech_energy() {
        let stereo = (0..4_800)
            .flat_map(|index| {
                let phase = index as f32 * 1_000.0 * std::f32::consts::TAU / 48_000.0;
                let sample = phase.sin() * 0.5;
                [sample, -sample]
            })
            .collect::<Vec<_>>();
        let actual = convert_in_chunks(&stereo, 48_000, 2, 16_000, 1_001);
        let peak = actual.iter().copied().map(f32::abs).fold(0.0_f32, f32::max);
        assert!(
            peak > 0.25,
            "anti-phase speech must not collapse, peak={peak}"
        );
    }
}
