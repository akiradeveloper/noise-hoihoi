use df::{Complex32, DFState};
use noise_net::{FFT_BINS, FRAME_SIZE};
use serde::Serialize;

use crate::audio::{energy, ratio_db};

#[derive(Debug, Serialize)]
pub struct Scores {
    pub reference_sdr_db: Option<f64>,
    pub input_sdr_db: Option<f64>,
    pub output_attenuation_db: f64,
    pub active_frames_drop6_percent: Option<f64>,
    pub longest_active_drop6_ms: usize,
    pub speech_dominant_attenuation_db: Option<f64>,
    pub noise_dominant_attenuation_db: Option<f64>,
    pub speech_dominant_cells: usize,
    pub noise_dominant_cells: usize,
    pub reference_band_attenuation_db: [Option<f64>; 6],
    pub output_peak: f32,
}

#[allow(clippy::cast_precision_loss)]
pub fn score(speech: &[f32], noise: &[f32], input: &[f32], output: &[f32]) -> Scores {
    assert_eq!(speech.len(), noise.len());
    assert_eq!(speech.len(), input.len());
    assert_eq!(speech.len(), output.len());
    let s_energy = energy(speech);
    let threshold = s_energy / speech.len() as f64 * 0.04;
    let error_energy = speech
        .iter()
        .zip(output)
        .map(|(s, y)| (f64::from(*s) - f64::from(*y)).powi(2))
        .sum();
    let input_error = speech
        .iter()
        .zip(input)
        .map(|(s, x)| (f64::from(*s) - f64::from(*x)).powi(2))
        .sum();
    let (mut active, mut drops, mut run, mut longest) = (0_usize, 0_usize, 0, 0);
    let mut s_dsp = DFState::default();
    let mut n_dsp = DFState::default();
    let mut y_dsp = DFState::default();
    let mut ss = [Complex32::new(0.0, 0.0); FFT_BINS];
    let mut ns = ss;
    let mut ys = ss;
    let (mut speech_in, mut speech_out, mut noise_in, mut noise_out) = (0.0, 0.0, 0.0, 0.0);
    let (mut speech_cells, mut noise_cells) = (0, 0);
    let mut bands = [(0.0, 0.0); 6];
    let mut previous_active = false;
    // Flush one analysis frame so the final half-window is included.
    for frame in 0..=speech.len().div_ceil(FRAME_SIZE) {
        let start = frame * FRAME_SIZE;
        let padded = |samples: &[f32]| {
            let mut result = [0.0; FRAME_SIZE];
            if start < samples.len() {
                let end = (start + FRAME_SIZE).min(samples.len());
                result[..end - start].copy_from_slice(&samples[start..end]);
            }
            result
        };
        let s = padded(speech);
        let n = padded(noise);
        let y = padded(output);
        s_dsp.analysis(&s, &mut ss);
        n_dsp.analysis(&n, &mut ns);
        y_dsp.analysis(&y, &mut ys);
        let current_active = energy(&s) / FRAME_SIZE as f64 > threshold;
        active += usize::from(current_active);
        if current_active && ratio_db(energy(&s), energy(&y)) > 6.0 {
            drops += 1;
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
        let spectral_active = current_active || previous_active;
        previous_active = current_active;
        if !spectral_active {
            continue;
        }
        let noise_peak = ns.iter().map(Complex32::norm_sqr).fold(0.0, f32::max);
        for bin in 0..FFT_BINS {
            let s = f64::from(ss[bin].norm_sqr());
            let n = f64::from(ns[bin].norm_sqr());
            let y = f64::from(ys[bin].norm_sqr());
            if s > 100.0 * n {
                speech_in += s;
                speech_out += y;
                speech_cells += 1;
            }
            if n > 100.0 * s && n > f64::from(noise_peak) * 0.001 {
                noise_in += n;
                noise_out += y;
                noise_cells += 1;
            }
            let band = match bin {
                0..=4 => 0,
                5..=19 => 1,
                20..=39 => 2,
                40..=79 => 3,
                80..=159 => 4,
                _ => 5,
            };
            bands[band].0 += s;
            bands[band].1 += y;
        }
    }
    Scores {
        reference_sdr_db: (s_energy > 0.0).then(|| ratio_db(s_energy, error_energy)),
        input_sdr_db: (s_energy > 0.0).then(|| ratio_db(s_energy, input_error)),
        output_attenuation_db: ratio_db(energy(input), energy(output)),
        active_frames_drop6_percent: (active > 0).then(|| 100.0 * drops as f64 / active as f64),
        longest_active_drop6_ms: longest * 10,
        speech_dominant_attenuation_db: (speech_in > 0.0).then(|| ratio_db(speech_in, speech_out)),
        noise_dominant_attenuation_db: (noise_in > 0.0).then(|| ratio_db(noise_in, noise_out)),
        speech_dominant_cells: speech_cells,
        noise_dominant_cells: noise_cells,
        reference_band_attenuation_db: bands.map(|(s, y)| (s > 0.0).then(|| ratio_db(s, y))),
        output_peak: output.iter().map(|v| v.abs()).fold(0.0, f32::max),
    }
}

#[cfg(test)]
mod tests {
    use super::score;

    fn signal() -> Vec<f32> {
        (0..4800)
            .map(|i| if i % 48 < 24 { 0.1 } else { -0.1 })
            .collect()
    }

    #[test]
    fn exact_voice_recovery_beats_pass_through_and_muting() {
        let s = signal();
        let n = vec![0.03; s.len()];
        let x = s.iter().zip(&n).map(|(s, n)| s + n).collect::<Vec<_>>();
        let oracle = score(&s, &n, &x, &s);
        let bypass = score(&s, &n, &x, &x);
        let mute = score(&s, &n, &x, &vec![0.0; s.len()]);
        assert!(oracle.reference_sdr_db.unwrap() > 100.0);
        assert!((bypass.reference_sdr_db.unwrap() - bypass.input_sdr_db.unwrap()).abs() < 1e-10);
        assert!(mute.reference_sdr_db.unwrap().abs() < 1e-10);
        assert_eq!(mute.active_frames_drop6_percent, Some(100.0));
    }

    #[test]
    fn scores_penalize_voice_gain_loss_without_normalizing_it_away() {
        let s = signal();
        let y = s.iter().map(|v| v * 0.1).collect::<Vec<_>>();
        let result = score(&s, &vec![0.0; s.len()], &s, &y);
        assert!((result.output_attenuation_db - 20.0).abs() < 1e-5);
        assert!(result.reference_sdr_db.unwrap() < 1.0);
        assert_eq!(result.active_frames_drop6_percent, Some(100.0));
    }
}
