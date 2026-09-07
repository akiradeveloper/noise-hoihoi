use crate::FRAME_SIZE;

const TARGET_RMS: f32 = 0.08;
const MAX_FEATURE_GAIN: f32 = 64.0;
// Peak-envelope release is about two seconds at a 10 ms hop. A loud onset
// reduces the feature gain immediately; pauses cannot rapidly pump it up.
const RELEASE: f32 = 0.995;

/// Adapts only model features and the detector, never the emitted waveform.
/// Keeping synthesis in the original scale avoids inverse-AGC delay errors.
#[derive(Default)]
pub(crate) struct InputLevel {
    envelope: f32,
}

impl InputLevel {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub(crate) fn update(&mut self, frame: &[f32; FRAME_SIZE]) -> f32 {
        let power = frame.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / FRAME_SIZE as f64;
        self.envelope = (power.sqrt() as f32).max(self.envelope * RELEASE);
        (TARGET_RMS / self.envelope.max(TARGET_RMS / MAX_FEATURE_GAIN)).clamp(1.0, MAX_FEATURE_GAIN)
    }
}

#[cfg(test)]
mod tests {
    use super::InputLevel;
    use crate::FRAME_SIZE;

    #[test]
    #[allow(clippy::float_cmp)] // Clamp endpoints are intentionally exact.
    fn feature_gain_is_bounded_through_silence_and_recovers_on_loud_onsets() {
        let mut level = InputLevel::default();
        for _ in 0..1_000 {
            assert_eq!(level.update(&[0.0; FRAME_SIZE]), 64.0);
        }
        assert_eq!(level.update(&[0.2; FRAME_SIZE]), 1.0);
        assert_eq!(level.update(&[0.0; FRAME_SIZE]), 1.0);
        for _ in 0..1_000 {
            assert!(level.update(&[0.0; FRAME_SIZE]).is_finite());
        }
        assert!((level.update(&[0.01; FRAME_SIZE]) - 8.0).abs() < 1e-5);
        assert_eq!(level.update(&[0.8; FRAME_SIZE]), 1.0);
    }

    #[test]
    fn a_short_pause_does_not_instantly_raise_the_gain() {
        let mut level = InputLevel::default();
        let before = level.update(&[0.01; FRAME_SIZE]);
        let after = level.update(&[0.0; FRAME_SIZE]);
        assert!(after >= before && after / before < 1.01);
    }
}
