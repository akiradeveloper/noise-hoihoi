//! Fixed 960-point Vorbis STFT with 50% overlap for the model's 48 kHz stream.
use crate::FRAME_SIZE;
use anyhow::Result;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex32};
use std::sync::Arc;

const FFT_SIZE: usize = 960;
pub(crate) struct Stft {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    window: [f32; FFT_SIZE],
    previous: [f32; FRAME_SIZE],
    overlap: [f32; FRAME_SIZE],
    time: [f32; FFT_SIZE],
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
}
impl Default for Stft {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn default() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(FFT_SIZE);
        let inverse = planner.plan_fft_inverse(FFT_SIZE);
        let window = std::array::from_fn(|n| {
            let sine = (std::f64::consts::PI * (n as f64 + 0.5) / FFT_SIZE as f64).sin();
            (std::f64::consts::FRAC_PI_2 * sine * sine).sin() as f32
        });
        Self {
            forward_scratch: forward.make_scratch_vec(),
            inverse_scratch: inverse.make_scratch_vec(),
            forward,
            inverse,
            window,
            previous: [0.0; FRAME_SIZE],
            overlap: [0.0; FRAME_SIZE],
            time: [0.0; FFT_SIZE],
        }
    }
}
impl Stft {
    pub(crate) fn analysis(
        &mut self,
        input: &[f32; FRAME_SIZE],
        output: &mut [Complex32],
    ) -> Result<()> {
        for (n, &sample) in input.iter().enumerate() {
            self.time[n] = self.previous[n] * self.window[n];
            self.time[n + FRAME_SIZE] = sample * self.window[n + FRAME_SIZE];
        }
        self.previous.copy_from_slice(input);
        self.forward
            .process_with_scratch(&mut self.time, output, &mut self.forward_scratch)?;
        Ok(())
    }
    // The caller supplies the inverse FFT's 1/960 normalization.
    pub(crate) fn synthesis(
        &mut self,
        input: &mut [Complex32],
        output: &mut [f32; FRAME_SIZE],
    ) -> Result<()> {
        // A real signal has real DC/Nyquist bins. Discard network imaginary values there.
        input[0].im = 0.0;
        input[FFT_SIZE / 2].im = 0.0;
        self.inverse
            .process_with_scratch(input, &mut self.time, &mut self.inverse_scratch)?;
        for (n, sample) in output.iter_mut().enumerate() {
            *sample = self.time[n] * self.window[n] + self.overlap[n];
            self.overlap[n] = self.time[n + FRAME_SIZE] * self.window[n + FRAME_SIZE];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn overlap_add_reconstructs_with_one_hop_delay() {
        let mut stft = Stft::default();
        let mut previous = [0.0; FRAME_SIZE];
        let mut spectrum = [Complex32::new(0.0, 0.0); FFT_SIZE / 2 + 1];
        for hop in 0..12 {
            let input = std::array::from_fn(|n| {
                if hop == 11 {
                    0.0
                } else {
                    ((hop * FRAME_SIZE + n) as f32 * 0.071).sin() * 0.3
                }
            });
            stft.analysis(&input, &mut spectrum).unwrap();
            for bin in &mut spectrum {
                *bin /= FFT_SIZE as f32;
            }
            let mut output = [0.0; FRAME_SIZE];
            stft.synthesis(&mut spectrum, &mut output).unwrap();
            let peak = output
                .iter()
                .zip(previous)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(peak < 2e-7, "hop {hop}, peak {peak}");
            previous = input;
        }
    }
}
