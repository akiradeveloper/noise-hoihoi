use crate::PIPELINE_SAMPLE_RATE;

/// Linux capture sources can carry a substantial DC offset. Remove it
/// before resampling, inference, and metering so it cannot dominate the monitor
/// scale or the model's input level. The fixed 10 Hz cutoff is below speech.
pub struct CaptureFilter {
    previous_input: Option<f32>,
    previous_output: f32,
    feedback: f32,
}

impl Default for CaptureFilter {
    #[allow(clippy::cast_possible_truncation)]
    fn default() -> Self {
        Self {
            previous_input: None,
            previous_output: 0.0,
            feedback: (-std::f64::consts::TAU * 10.0 / f64::from(PIPELINE_SAMPLE_RATE)).exp()
                as f32,
        }
    }
}

impl CaptureFilter {
    pub fn reset(&mut self) {
        self.previous_input = None;
        self.previous_output = 0.0;
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        if !sample.is_finite() {
            self.reset();
            return 0.0;
        }
        let sample = sample.clamp(-1.0, 1.0);
        // Seed from the first real sample, including after a capture hole, to
        // avoid turning a device's DC offset into a startup impulse.
        let previous_input = self.previous_input.replace(sample).unwrap_or(sample);
        let gain = (1.0 + self.feedback) * 0.5;
        let output = gain * (sample - previous_input) + self.feedback * self.previous_output;
        self.previous_output = output;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::CaptureFilter;

    #[test]
    fn constant_offset_is_silent_from_the_first_sample() {
        for offset in [-0.23, 0.0, 0.23] {
            let mut filter = CaptureFilter::default();
            for _ in 0..4_800 {
                assert_eq!(filter.process(offset).to_bits(), 0.0_f32.to_bits());
            }
        }
    }

    #[test]
    fn biased_audio_becomes_centered_without_losing_voice_amplitude() {
        let mut filter = CaptureFilter::default();
        let mut sum = 0.0_f64;
        let mut squares = 0.0_f64;
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        // Model the observed microphone: +0.23 DC with a much smaller AC signal.
        for i in 0..48_000 {
            #[allow(clippy::cast_possible_truncation)]
            let voice =
                (0.01 * (std::f64::consts::TAU * 440.0 * f64::from(i) / 48_000.0).sin()) as f32;
            let output = filter.process(0.23 + voice);
            if i >= 24_000 {
                sum += f64::from(output);
                squares += f64::from(output).powi(2);
                minimum = minimum.min(output);
                maximum = maximum.max(output);
            }
        }
        assert!((sum / 24_000.0).abs() < 0.000_01);
        let expected_rms = 0.01 / std::f64::consts::SQRT_2;
        assert!(((squares / 24_000.0).sqrt() / expected_rms - 1.0).abs() < 0.01);
        assert!(minimum < -0.009 && maximum > 0.009);
    }

    #[test]
    fn offset_changes_settle_without_a_permanent_baseline_shift() {
        let mut filter = CaptureFilter::default();
        filter.process(0.23);
        let mut output = 0.0;
        for _ in 0..12_000 {
            output = filter.process(-0.15);
        }
        assert!(output.abs() < 0.000_01);
    }

    #[test]
    fn capture_holes_and_invalid_samples_reset_the_baseline() {
        let mut filter = CaptureFilter::default();
        filter.process(0.23);
        filter.process(0.24);
        filter.reset();
        assert_eq!(filter.process(-0.15).to_bits(), 0.0_f32.to_bits());
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            filter.process(-0.14);
            assert_eq!(filter.process(invalid).to_bits(), 0.0_f32.to_bits());
            assert_eq!(filter.process(0.23).to_bits(), 0.0_f32.to_bits());
        }
    }
}
