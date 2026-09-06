use std::collections::VecDeque;

use crate::{FRAME_SIZE, SAMPLE_RATE};

const WINDOW_SAMPLES: usize = FRAME_SIZE * 3;
const MIN_PITCH_HZ: usize = 70;
const MAX_PITCH_HZ: usize = 400;
const MIN_LAG: usize = SAMPLE_RATE as usize / MAX_PITCH_HZ;
const MAX_LAG: usize = (SAMPLE_RATE as usize).div_ceil(MIN_PITCH_HZ);
const MIN_RMS: f32 = 0.005_623_413;
const MIN_PERIODICITY: f32 = 0.75;
const ATTACK_FRAMES: u8 = 3;

/// Conservative detector used to prevent model suppression of voiced speech.
pub(crate) struct VoicedGuard {
    samples: VecDeque<f32>,
    consecutive_frames: u8,
}

impl VoicedGuard {
    pub(crate) fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(WINDOW_SAMPLES),
            consecutive_frames: 0,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn update(&mut self, input: &[f32; FRAME_SIZE]) -> bool {
        for &sample in input {
            if self.samples.len() == WINDOW_SAMPLES {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }

        let energy = input.iter().map(|sample| sample * sample).sum::<f32>();
        let rms = (energy / FRAME_SIZE as f32).sqrt();
        let periodic = rms >= MIN_RMS && self.periodicity() >= MIN_PERIODICITY;
        if periodic {
            self.consecutive_frames = self.consecutive_frames.saturating_add(1);
        } else {
            self.consecutive_frames = 0;
        }
        self.consecutive_frames >= ATTACK_FRAMES
    }

    #[allow(clippy::cast_precision_loss)]
    fn periodicity(&mut self) -> f32 {
        if self.samples.len() < WINDOW_SAMPLES {
            return 0.0;
        }
        let samples = self.samples.make_contiguous();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        (MIN_LAG..=MAX_LAG)
            .step_by(4)
            .map(|lag| normalized_correlation(samples, mean, lag))
            .fold(0.0, f32::max)
    }
}

impl Default for VoicedGuard {
    fn default() -> Self {
        Self::new()
    }
}

fn normalized_correlation(samples: &[f32], mean: f32, lag: usize) -> f32 {
    let delayed = &samples[..samples.len() - lag];
    let current = &samples[lag..];
    let (numerator, delayed_energy, current_energy) = delayed.iter().zip(current).fold(
        (0.0_f32, 0.0_f32, 0.0_f32),
        |(correlation, left_energy, right_energy), (&left, &right)| {
            let left = left - mean;
            let right = right - mean;
            (
                correlation + left * right,
                left_energy + left * left,
                right_energy + right * right,
            )
        },
    );
    numerator
        / (delayed_energy * current_energy)
            .sqrt()
            .max(f32::MIN_POSITIVE)
}

#[cfg(test)]
mod tests {
    use super::VoicedGuard;
    use crate::{FRAME_SIZE, SAMPLE_RATE};

    #[allow(clippy::cast_precision_loss)]
    fn protects_vowel(fundamental_hz: f32) {
        let mut guard = VoicedGuard::new();
        let mut protected = false;
        for frame_index in 0..8 {
            let input = std::array::from_fn(|sample| {
                let index = frame_index * FRAME_SIZE + sample;
                let time = index as f32 / SAMPLE_RATE as f32;
                0.08 * (std::f32::consts::TAU * fundamental_hz * time).sin()
                    + 0.035 * (std::f32::consts::TAU * fundamental_hz * 2.0 * time).sin()
                    + 0.015 * (std::f32::consts::TAU * fundamental_hz * 3.0 * time).sin()
            });
            protected = guard.update(&input);
        }
        assert!(protected);
    }

    #[test]
    fn protects_a_sustained_low_vowel() {
        protects_vowel(105.0);
    }

    #[test]
    fn protects_a_sustained_high_vowel() {
        protects_vowel(260.0);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn does_not_protect_broadband_keyboard_noise() {
        let mut guard = VoicedGuard::new();
        for frame_index in 0..20 {
            let input = std::array::from_fn(|sample| {
                let index = frame_index * FRAME_SIZE + sample;
                let random = ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16)
                    & 0x7fff) as f32
                    / 16_384.0
                    - 1.0;
                let transient = if sample < 80 {
                    0.15 * (1.0 - sample as f32 / 80.0)
                } else {
                    0.0
                };
                transient * random
            });
            assert!(!guard.update(&input));
        }
    }

    #[test]
    fn does_not_protect_silence() {
        let mut guard = VoicedGuard::new();
        for _ in 0..8 {
            assert!(!guard.update(&[0.0; FRAME_SIZE]));
        }
    }
}
