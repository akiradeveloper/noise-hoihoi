use std::collections::VecDeque;

use df::{
    Complex32, DFState, MEAN_NORM_INIT, UNIT_NORM_INIT, band_mean_norm_erb, band_unit_norm,
    compute_band_corr,
};

use crate::recovery::SpeechRecovery;
use crate::{DF_BINS, DF_LOOKAHEAD, DF_ORDER, ERB_BANDS, FFT_BINS, FRAME_SIZE};

const CONV_LOOKAHEAD: usize = 2;
const NORMALIZATION_ALPHA: f32 = 0.99;
const NOISY_HISTORY_LENGTH: usize = if DF_ORDER > DF_LOOKAHEAD {
    DF_ORDER
} else {
    DF_LOOKAHEAD
};
const NOISY_OUTPUT_INDEX: usize = NOISY_HISTORY_LENGTH - DF_LOOKAHEAD - 1;
// Prominent low/mid bands and coherent harmonics across the full spectrum
// supplement the model. At 48 kHz / 960 samples, bin 80 is 4 kHz.
const LOW_VOICE_BINS: usize = 81;
const VOICE_GAIN_FLOOR: f32 = 0.98;

pub(crate) struct Features {
    pub(crate) erb: [f32; ERB_BANDS],
    pub(crate) complex: [[f32; DF_BINS]; 2],
}

pub(crate) struct DspState {
    state: DFState,
    current_spectrum: Vec<Complex32>,
    enhanced_spectrum: Vec<Complex32>,
    noisy_spectra: VecDeque<Vec<Complex32>>,
    masked_spectra: VecDeque<Vec<Complex32>>,
    mean_norm_state: [f32; ERB_BANDS],
    unit_norm_state: [f32; DF_BINS],
    voice_history: [bool; DF_LOOKAHEAD + 1],
    voice_strength: f32,
    speech_recovery: SpeechRecovery,
}

impl DspState {
    pub(crate) fn new() -> Self {
        let state = DFState::default();
        Self {
            state,
            current_spectrum: vec![Complex32::new(0.0, 0.0); FFT_BINS],
            enhanced_spectrum: vec![Complex32::new(0.0, 0.0); FFT_BINS],
            noisy_spectra: std::iter::repeat_with(|| vec![Complex32::new(0.0, 0.0); FFT_BINS])
                .take(NOISY_HISTORY_LENGTH)
                .collect(),
            masked_spectra: std::iter::repeat_with(|| vec![Complex32::new(0.0, 0.0); FFT_BINS])
                .take(DF_ORDER + CONV_LOOKAHEAD)
                .collect(),
            mean_norm_state: linear_state(MEAN_NORM_INIT[0], MEAN_NORM_INIT[1]),
            unit_norm_state: linear_state(UNIT_NORM_INIT[0], UNIT_NORM_INIT[1]),
            voice_history: [false; DF_LOOKAHEAD + 1],
            voice_strength: 0.0,
            speech_recovery: SpeechRecovery::default(),
        }
    }

    pub(crate) fn analyze(&mut self, input: &[f32; FRAME_SIZE], feature_gain: f32) -> Features {
        self.state.analysis(input, &mut self.current_spectrum);
        advance_spectrum_history(&mut self.noisy_spectra, &self.current_spectrum);
        advance_spectrum_history(&mut self.masked_spectra, &self.current_spectrum);

        let mut erb = [0.0; ERB_BANDS];
        compute_band_corr(
            &mut erb,
            &self.current_spectrum,
            &self.current_spectrum,
            &self.state.erb,
        );
        for value in &mut erb {
            *value = (*value * feature_gain * feature_gain + 1.0e-10).log10() * 10.0;
        }
        band_mean_norm_erb(&mut erb, &mut self.mean_norm_state, NORMALIZATION_ALPHA);
        let mut normalized = vec![Complex32::new(0.0, 0.0); DF_BINS];
        normalized.copy_from_slice(&self.current_spectrum[..DF_BINS]);
        for value in &mut normalized {
            *value *= feature_gain;
        }
        band_unit_norm(
            &mut normalized,
            &mut self.unit_norm_state,
            NORMALIZATION_ALPHA,
        );
        let mut complex = [[0.0; DF_BINS]; 2];
        for (index, value) in normalized.into_iter().enumerate() {
            complex[0][index] = value.re;
            complex[1][index] = value.im;
        }
        Features { erb, complex }
    }

    pub(crate) fn synthesize(
        &mut self,
        mask: Option<&[f32]>,
        coefficients: Option<&[f32]>,
        protect_voice: bool,
        output: &mut [f32; FRAME_SIZE],
    ) -> bool {
        let masked_index = DF_ORDER - 1;
        if let Some(mask) = mask {
            assert_eq!(mask.len(), ERB_BANDS);
            apply_mask(
                &mut self.masked_spectra[masked_index],
                mask,
                &self.state.erb,
            );
        }
        self.enhanced_spectrum
            .copy_from_slice(&self.masked_spectra[masked_index]);
        if let Some(coefficients) = coefficients {
            assert_eq!(coefficients.len(), DF_BINS * DF_ORDER * 2);
            for (bin, enhanced_bin) in self.enhanced_spectrum.iter_mut().enumerate().take(DF_BINS) {
                let mut filtered = Complex32::new(0.0, 0.0);
                for order in 0..DF_ORDER {
                    let coefficient_index = bin * DF_ORDER * 2 + order * 2;
                    let coefficient = Complex32::new(
                        coefficients[coefficient_index],
                        coefficients[coefficient_index + 1],
                    );
                    filtered += self.noisy_spectra[order][bin] * coefficient;
                }
                *enhanced_bin = filtered;
            }
        }
        // Align the detector with the spectrum being emitted, then fade its
        // protection over 20 ms on attack and 50 ms on release. A brief click
        // may disrupt periodicity without ending the surrounding vowel.
        self.voice_history.rotate_left(1);
        self.voice_history[DF_LOOKAHEAD] = protect_voice;
        self.voice_strength = if self.voice_history[0] {
            (self.voice_strength + 0.5).min(1.0)
        } else {
            (self.voice_strength - 0.2).max(0.0)
        };
        let recovering = self.speech_recovery.apply(
            &mut self.enhanced_spectrum,
            &self.noisy_spectra[NOISY_OUTPUT_INDEX],
            self.voice_history[0],
        );
        if self.voice_strength > 0.0 {
            protect_voice_bands(
                &mut self.enhanced_spectrum,
                &self.noisy_spectra,
                self.voice_strength,
            );
        }
        self.state.synthesis(&mut self.enhanced_spectrum, output);
        self.voice_strength > 0.0 || recovering
    }
}

fn advance_spectrum_history(history: &mut VecDeque<Vec<Complex32>>, current: &[Complex32]) {
    let mut spectrum = history
        .pop_front()
        .expect("spectrum histories are initialized with fixed capacity");
    spectrum.copy_from_slice(current);
    history.push_back(spectrum);
}

#[allow(clippy::cast_precision_loss)]
fn linear_state<const N: usize>(start: f32, end: f32) -> [f32; N] {
    std::array::from_fn(|index| start + index as f32 * (end - start) / (N - 1) as f32)
}

fn apply_mask(spectrum: &mut [Complex32], mask: &[f32], erb_widths: &[usize]) {
    let mut bin = 0;
    for (&width, &gain) in erb_widths.iter().zip(mask) {
        for value in &mut spectrum[bin..bin + width] {
            *value *= gain;
        }
        bin += width;
    }
}

fn protect_voice_bands(
    enhanced: &mut [Complex32],
    noisy_history: &VecDeque<Vec<Complex32>>,
    strength: f32,
) {
    // The existing five spectra surround the output by two frames on each
    // side. The second-smallest magnitude rejects bursts spanning up to three
    // frames; a click cannot establish its own protection floor. No extra
    // lookahead or allocation is needed.
    let stable: [f32; FFT_BINS] = std::array::from_fn(|bin| {
        let mut powers: [f32; NOISY_HISTORY_LENGTH] =
            std::array::from_fn(|frame| noisy_history[frame][bin].norm_sqr());
        powers.sort_unstable_by(f32::total_cmp);
        powers[1].sqrt()
    });
    let peak = stable[1..LOW_VOICE_BINS]
        .iter()
        .copied()
        .fold(0.0, f32::max);
    if peak <= 1.0e-10 {
        return;
    }
    let noisy = &noisy_history[NOISY_OUTPUT_INDEX];
    for bin in 1..FFT_BINS {
        let magnitude = noisy[bin].norm();
        if magnitude <= 1.0e-10 {
            continue;
        }
        // Weak harmonics can have stable phase advances even when they are
        // far below the fundamental. Coherence does not require a large
        // magnitude relative to that fundamental. The temporal magnitude
        // statistic still limits restoration of a simultaneous short click.
        let low_band = if bin < LOW_VOICE_BINS {
            ((stable[bin] / peak - 0.02) / 0.06).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let coherent = ((spectral_coherence(noisy_history, bin) - 0.8) / 0.18).clamp(0.0, 1.0);
        let prominence = low_band.max(coherent);
        let floor = strength * VOICE_GAIN_FLOOR * prominence * (stable[bin] / magnitude).min(1.0);
        if floor > 0.0 {
            preserve_response(&mut enhanced[bin], noisy[bin], floor);
        }
    }
}

fn spectral_coherence(history: &VecDeque<Vec<Complex32>>, bin: usize) -> f32 {
    let mut correlation = Complex32::new(0.0, 0.0);
    let (mut left_energy, mut right_energy) = (0.0, 0.0);
    for frame in 1..history.len() {
        let left = history[frame - 1][bin];
        let right = history[frame][bin];
        correlation += right * left.conj();
        left_energy += left.norm_sqr();
        right_energy += right.norm_sqr();
    }
    correlation.norm() / (left_energy * right_energy).sqrt().max(1.0e-20)
}

fn preserve_response(enhanced: &mut Complex32, noisy: Complex32, floor: f32) {
    // Constrain the response along the original phase. A magnitude-only gain
    // misses phase cancellation when mixing the estimated and original bins.
    let response = (*enhanced * noisy.conj()).re / noisy.norm_sqr();
    if response < floor {
        let mix = (floor - response) / (1.0 - response);
        *enhanced = *enhanced * (1.0 - mix) + noisy * mix;
    }
}

impl Default for DspState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{DspState, apply_mask, linear_state, preserve_response};
    use crate::{ALGORITHM_LATENCY_SAMPLES, ERB_BANDS, FFT_BINS, FRAME_SIZE};
    use df::Complex32;

    #[test]
    fn normalization_states_include_the_official_endpoints() {
        let state = linear_state::<32>(-60.0, -90.0);
        assert_eq!(state[0].to_bits(), (-60.0_f32).to_bits());
        assert_eq!(state[31].to_bits(), (-90.0_f32).to_bits());
    }

    #[test]
    fn erb_filterbank_covers_every_fft_bin() {
        let dsp = DspState::new();
        assert_eq!(dsp.state.erb.len(), ERB_BANDS);
        assert_eq!(dsp.state.erb.iter().sum::<usize>(), FFT_BINS);
        assert!(dsp.state.erb.iter().all(|width| *width >= 2));
    }

    #[test]
    fn zero_mask_clears_the_whole_spectrum() {
        let dsp = DspState::new();
        let mut spectrum = vec![Complex32::new(1.0, -0.5); FFT_BINS];
        apply_mask(&mut spectrum, &[0.0; ERB_BANDS], &dsp.state.erb);
        assert!(spectrum.iter().all(|value| value.norm_sqr() == 0.0));
    }

    #[test]
    fn suppressed_noise_is_not_mixed_back_into_the_output() {
        let mut dsp = DspState::new();
        for _ in 0..16 {
            let input = std::array::from_fn(|sample| if sample % 2 == 0 { 0.2 } else { -0.2 });
            dsp.analyze(&input, 1.0);
            let mut output = [0.0; FRAME_SIZE];
            let protected = dsp.synthesize(Some(&[0.0; ERB_BANDS]), None, false, &mut output);
            assert!(!protected);
            assert!(output.iter().all(|sample| sample.abs() < 1.0e-8));
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn voice_protection_does_not_restore_a_simultaneous_short_burst() {
        let frames = 30;
        let voice = (0..frames * FRAME_SIZE)
            .map(|index| 0.1 * (std::f32::consts::TAU * 250.0 * index as f32 / 48_000.0).sin())
            .collect::<Vec<_>>();
        let burst = (0..voice.len())
            .map(|index| {
                if (15 * FRAME_SIZE..16 * FRAME_SIZE).contains(&index) {
                    0.2 * (std::f32::consts::TAU * 2_500.0 * index as f32 / 48_000.0).sin()
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>();
        let process = |noisy: bool| {
            let mut dsp = DspState::new();
            let mut result = Vec::new();
            for frame in 0..frames {
                let input = std::array::from_fn(|sample| {
                    let index = frame * FRAME_SIZE + sample;
                    voice[index] + if noisy { burst[index] } else { 0.0 }
                });
                dsp.analyze(&input, 1.0);
                let mut output = [0.0; FRAME_SIZE];
                // A model suppressing everything is the most demanding case
                // for the voice guard, and used to restore the entire click.
                dsp.synthesize(Some(&[0.0; ERB_BANDS]), None, true, &mut output);
                result.extend(output);
            }
            result
        };
        let clean = process(false);
        let mixed = process(true);
        let residual = clean
            .iter()
            .zip(&mixed)
            .map(|(c, m)| (c - m).powi(2))
            .sum::<f32>();
        let noise_energy = burst.iter().map(|x| x * x).sum::<f32>();
        assert!(
            residual < noise_energy * 0.01,
            "voice guard restored a click"
        );
        let start = 10 * FRAME_SIZE;
        let output_energy = clean[start + ALGORITHM_LATENCY_SAMPLES..]
            .iter()
            .map(|x| x * x)
            .sum::<f32>();
        let input_energy = voice[start..voice.len() - ALGORITHM_LATENCY_SAMPLES]
            .iter()
            .map(|x| x * x)
            .sum::<f32>();
        assert!(
            output_energy > input_energy * 0.5,
            "voice was suppressed along with the click"
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn analysis_synthesis_has_the_documented_delay() {
        let frame_count = 12;
        let input: Vec<f32> = (0..frame_count * FRAME_SIZE)
            .map(|index| {
                let time = index as f32 / 48_000.0;
                0.25 * (std::f32::consts::TAU * 440.0 * time).sin()
            })
            .collect();
        let mut dsp = DspState::new();
        let mut output = Vec::with_capacity(input.len());
        for (index, frame) in input.chunks_exact(FRAME_SIZE).enumerate() {
            let frame: &[f32; FRAME_SIZE] = frame.try_into().unwrap();
            // Feature gain changes must not affect waveform gain or delay.
            let _ = dsp.analyze(frame, if index % 2 == 0 { 1.0 } else { 64.0 });
            let mut synthesized = [0.0; FRAME_SIZE];
            dsp.synthesize(None, None, false, &mut synthesized);
            output.extend(synthesized);
        }

        for (&actual, &expected) in output[ALGORITHM_LATENCY_SAMPLES..]
            .iter()
            .zip(&input[..input.len() - ALGORITHM_LATENCY_SAMPLES])
        {
            assert!((actual - expected).abs() <= 2.0e-5);
        }
    }

    #[test]
    fn protected_response_cannot_cancel_against_the_original_phase() {
        let original = Complex32::new(0.4, -0.3);
        let mut enhanced = -original * 0.8;
        preserve_response(&mut enhanced, original, 0.98);
        assert!((enhanced - original * 0.98).norm() < 1.0e-6);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn protects_a_weak_harmonic_above_the_former_voice_band_limit() {
        let mut dsp = DspState::new();
        let (mut projected, mut expected_energy) = (0.0, 0.0);
        for frame in 0..40 {
            let input = std::array::from_fn(|sample| {
                let t = (frame * FRAME_SIZE + sample) as f32 / 48_000.0;
                0.1 * (std::f32::consts::TAU * 250.0 * t).sin()
                    + 0.001 * (std::f32::consts::TAU * 5_250.0 * t).sin()
            });
            dsp.analyze(&input, 1.0);
            let mut output = [0.0; FRAME_SIZE];
            dsp.synthesize(Some(&[0.0; ERB_BANDS]), None, true, &mut output);
            if frame >= 10 {
                for (sample, &actual) in output.iter().enumerate() {
                    let t =
                        (frame * FRAME_SIZE + sample - ALGORITHM_LATENCY_SAMPLES) as f32 / 48_000.0;
                    let expected = 0.001 * (std::f32::consts::TAU * 5_250.0 * t).sin();
                    projected += f64::from(actual) * f64::from(expected);
                    expected_energy += f64::from(expected).powi(2);
                }
            }
        }
        assert!(
            projected / expected_energy > 0.9,
            "weak upper harmonic was lost"
        );
    }
}
