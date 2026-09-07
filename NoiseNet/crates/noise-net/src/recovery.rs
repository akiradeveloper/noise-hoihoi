use df::Complex32;

const HOLD_FRAMES: u8 = 50;
const ATTENUATION_EXPONENT: f32 = 0.65;
const MAX_BOOST: f32 = 4.0;
const TRANSIENT_POWER_RATIO: f32 = 3.0;

/// Briefly soften model attenuation after detected speech. Repeated operation
/// sounds can obscure periodicity while the speaker continues talking.
/// This rescales only the model's estimate, preserving its phase and zeros.
#[derive(Default)]
pub(crate) struct SpeechRecovery {
    hold: u8,
    transient_hold: u8,
    high_band_power: f32,
    strength: f32,
}

impl SpeechRecovery {
    pub(crate) fn apply(
        &mut self,
        enhanced: &mut [Complex32],
        original: &[Complex32],
        voiced: bool,
    ) -> bool {
        // Operation sounds create rapid power increases in the 1-8 kHz
        // region. Relative power makes the trigger independent of mic gain.
        // A stationary background must not enable recovery indefinitely.
        let power = original
            .iter()
            .skip(20)
            .take(141)
            .map(Complex32::norm_sqr)
            .sum::<f32>();
        let transient = power > 1.0e-12 && power > self.high_band_power * TRANSIENT_POWER_RATIO;
        self.high_band_power += 0.1 * (power - self.high_band_power);
        self.transient_hold = if transient {
            HOLD_FRAMES
        } else {
            self.transient_hold.saturating_sub(1)
        };
        self.hold = if voiced {
            HOLD_FRAMES
        } else {
            self.hold.saturating_sub(1)
        };
        self.strength = if !voiced && self.hold > 0 && self.transient_hold > 0 {
            (self.strength + 0.5).min(1.0)
        } else {
            (self.strength - 0.2).max(0.0)
        };
        if self.strength <= 0.0 {
            return false;
        }

        // For magnitude response g < 1, g^0.65 eases attenuation. Limit
        // recovery to 12 dB, including a smooth onset and release. The
        // resulting response stays below unity relative to the input bin.
        let exponent = (ATTENUATION_EXPONENT - 1.0) * self.strength;
        let limit = MAX_BOOST.powf(self.strength);
        for (estimate, input) in enhanced.iter_mut().zip(original) {
            let input_magnitude = input.norm();
            let magnitude = estimate.norm();
            if input_magnitude > 1.0e-10 && magnitude > 1.0e-12 && magnitude < input_magnitude {
                let gain = magnitude / input_magnitude;
                *estimate *= gain.powf(exponent).min(limit);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::SpeechRecovery;
    use crate::FFT_BINS;
    use df::Complex32;

    #[test]
    fn recovery_is_bounded_and_preserves_the_model_phase_and_zeros() {
        let original = [Complex32::new(0.2, 0.1); FFT_BINS];
        let mut estimate = [Complex32::new(0.0, 0.0); FFT_BINS];
        estimate[30] = Complex32::new(0.0001, -0.0001);
        estimate[32] = original[32] * 2.0;
        let mut recovery = SpeechRecovery::default();
        recovery.apply(&mut estimate.clone(), &original, true);
        for _ in 0..5 {
            let mut output = estimate;
            assert!(recovery.apply(&mut output, &original, false));
            let response = output[30] / estimate[30];
            assert!(response.re > 1.0 && response.re <= 4.000_001);
            assert!(response.im.abs() < 1.0e-6);
            assert!(output[30].norm() < original[30].norm());
            assert_eq!(output[31], estimate[31]);
            assert_eq!(output[32], estimate[32]);
        }
    }

    #[test]
    fn noise_cannot_start_or_indefinitely_extend_recovery() {
        let original = [Complex32::new(0.2, 0.1); FFT_BINS];
        let estimate = [Complex32::new(0.001, -0.001); FFT_BINS];
        let mut recovery = SpeechRecovery::default();
        for _ in 0..100 {
            let mut output = estimate;
            assert!(!recovery.apply(&mut output, &original, false));
            assert_eq!(output, estimate);
        }
        recovery.apply(&mut estimate.clone(), &original, true);
        let original = original.map(|bin| bin * 4.0);
        for _ in 0..10 {
            let mut output = estimate;
            assert!(recovery.apply(&mut output, &original, false));
            assert!(output[0].norm() > estimate[0].norm());
        }
        for _ in 0..70 {
            recovery.apply(&mut estimate.clone(), &original, false);
        }
        let mut output = estimate;
        assert!(!recovery.apply(&mut output, &original, false));
        assert_eq!(output, estimate);
    }

    #[test]
    fn steady_background_does_not_trigger_recovery_when_periodicity_is_lost() {
        let original = [Complex32::new(0.2, 0.1); FFT_BINS];
        let estimate = [Complex32::new(0.001, -0.001); FFT_BINS];
        let mut recovery = SpeechRecovery::default();
        for _ in 0..100 {
            assert!(!recovery.apply(&mut estimate.clone(), &original, true));
        }
        for _ in 0..100 {
            let mut output = estimate;
            assert!(!recovery.apply(&mut output, &original, false));
            assert_eq!(output, estimate);
        }
    }
}
