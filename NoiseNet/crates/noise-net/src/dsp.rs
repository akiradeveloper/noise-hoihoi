use std::collections::VecDeque;

use df::{
    Complex32, DFState, MEAN_NORM_INIT, UNIT_NORM_INIT, band_mean_norm_erb, band_unit_norm,
    compute_band_corr,
};

use crate::{DF_BINS, DF_LOOKAHEAD, DF_ORDER, ERB_BANDS, FFT_BINS, FRAME_SIZE};

const CONV_LOOKAHEAD: usize = 2;
const NORMALIZATION_ALPHA: f32 = 0.99;

pub(crate) struct Features {
    pub(crate) erb: [f32; ERB_BANDS],
    pub(crate) complex: [[f32; DF_BINS]; 2],
}

pub(crate) struct DspState {
    state: DFState,
    current_spectrum: Vec<Complex32>,
    noisy_spectra: VecDeque<Vec<Complex32>>,
    masked_spectra: VecDeque<Vec<Complex32>>,
    mean_norm_state: [f32; ERB_BANDS],
    unit_norm_state: [f32; DF_BINS],
}

impl DspState {
    pub(crate) fn new() -> Self {
        let state = DFState::default();
        Self {
            state,
            current_spectrum: vec![Complex32::new(0.0, 0.0); FFT_BINS],
            noisy_spectra: std::iter::repeat_with(|| vec![Complex32::new(0.0, 0.0); FFT_BINS])
                .take(DF_ORDER.max(DF_LOOKAHEAD))
                .collect(),
            masked_spectra: std::iter::repeat_with(|| vec![Complex32::new(0.0, 0.0); FFT_BINS])
                .take(DF_ORDER + CONV_LOOKAHEAD)
                .collect(),
            mean_norm_state: linear_state(MEAN_NORM_INIT[0], MEAN_NORM_INIT[1]),
            unit_norm_state: linear_state(UNIT_NORM_INIT[0], UNIT_NORM_INIT[1]),
        }
    }

    pub(crate) fn analyze(&mut self, input: &[f32; FRAME_SIZE]) -> Features {
        self.state.analysis(input, &mut self.current_spectrum);
        self.noisy_spectra.pop_front();
        self.noisy_spectra.push_back(self.current_spectrum.clone());
        self.masked_spectra.pop_front();
        self.masked_spectra.push_back(self.current_spectrum.clone());

        let mut erb = [0.0; ERB_BANDS];
        compute_band_corr(
            &mut erb,
            &self.current_spectrum,
            &self.current_spectrum,
            &self.state.erb,
        );
        for value in &mut erb {
            *value = (*value + 1.0e-10).log10() * 10.0;
        }
        band_mean_norm_erb(&mut erb, &mut self.mean_norm_state, NORMALIZATION_ALPHA);
        let mut normalized = vec![Complex32::new(0.0, 0.0); DF_BINS];
        normalized.copy_from_slice(&self.current_spectrum[..DF_BINS]);
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
        output: &mut [f32; FRAME_SIZE],
    ) {
        let masked_index = DF_ORDER - 1;
        if let Some(mask) = mask {
            assert_eq!(mask.len(), ERB_BANDS);
            apply_mask(
                &mut self.masked_spectra[masked_index],
                mask,
                &self.state.erb,
            );
        }
        let mut enhanced = self.masked_spectra[masked_index].clone();
        if let Some(coefficients) = coefficients {
            assert_eq!(coefficients.len(), DF_BINS * DF_ORDER * 2);
            for (bin, enhanced_bin) in enhanced.iter_mut().enumerate().take(DF_BINS) {
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
        self.state.synthesis(&mut enhanced, output);
    }
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

impl Default for DspState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{DspState, apply_mask, linear_state};
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
        for frame in input.chunks_exact(FRAME_SIZE) {
            let frame: &[f32; FRAME_SIZE] = frame.try_into().unwrap();
            let _ = dsp.analyze(frame);
            let mut synthesized = [0.0; FRAME_SIZE];
            dsp.synthesize(None, None, &mut synthesized);
            output.extend(synthesized);
        }

        for (&actual, &expected) in output[ALGORITHM_LATENCY_SAMPLES..]
            .iter()
            .zip(&input[..input.len() - ALGORITHM_LATENCY_SAMPLES])
        {
            assert!((actual - expected).abs() <= 2.0e-5);
        }
    }
}
