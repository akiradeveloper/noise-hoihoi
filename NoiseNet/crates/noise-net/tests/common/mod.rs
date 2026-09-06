use std::io::Cursor;

use noise_net::{
    ALGORITHM_LATENCY_SAMPLES, ATTENUATION_LIMIT_DB, FRAME_SIZE, NoiseNet, SAMPLE_RATE,
};

#[path = "../../testdata/reference_input.rs"]
mod reference_input;

pub const REFERENCE_FRAMES: usize = 120;

const REFERENCE_WAV: &[u8] = include_bytes!("../../testdata/dfn3-reference-synthetic.wav");

pub fn process_reference(net: &mut NoiseNet, frames: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(frames * FRAME_SIZE);
    for frame_index in 0..frames {
        let input = std::array::from_fn(|sample| {
            reference_input::sample(frame_index * FRAME_SIZE + sample)
        });
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame_unprotected(&input, &mut output);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(net.last_local_snr_db().is_finite());
        result.extend(output);
    }
    result
}

pub fn expected_reference() -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(REFERENCE_WAV))
        .expect("embedded official reference must be a WAV file");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, SAMPLE_RATE);
    assert_eq!(spec.bits_per_sample, 16);
    let dry_mix = 10.0_f32.powf(-ATTENUATION_LIMIT_DB / 20.0);
    reader
        .into_samples::<i16>()
        .enumerate()
        .map(|(index, sample)| {
            let enhanced =
                f32::from(sample.expect("reference WAV must be valid")) / f32::from(i16::MAX);
            let dry = index
                .checked_sub(ALGORITHM_LATENCY_SAMPLES)
                .map_or(0.0, reference_input::sample);
            enhanced * (1.0 - dry_mix) + dry * dry_mix
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
pub fn reference_error(actual: &[f32], expected: &[f32]) -> (f32, f32) {
    assert_eq!(actual.len(), expected.len());
    assert!(!actual.is_empty());
    let (squared_error, max_error) = actual.iter().zip(expected).fold(
        (0.0_f32, 0.0_f32),
        |(squared_error, max_error), (&actual, &expected)| {
            let error = (actual - expected).abs();
            (squared_error + error * error, max_error.max(error))
        },
    );
    ((squared_error / actual.len() as f32).sqrt(), max_error)
}

#[allow(clippy::cast_precision_loss)]
pub fn assert_sustained_vowel_is_protected(net: &mut NoiseNet, fundamental_hz: f32, frames: usize) {
    let input = (0..frames * FRAME_SIZE)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            0.08 * (std::f32::consts::TAU * fundamental_hz * time).sin()
                + 0.035 * (std::f32::consts::TAU * fundamental_hz * 2.0 * time).sin()
                + 0.015 * (std::f32::consts::TAU * fundamental_hz * 3.0 * time).sin()
        })
        .collect::<Vec<_>>();

    net.reset();
    let mut protected_frames = 0;
    let mut protected_streak = 0;
    for (frame_index, frame) in input.chunks_exact(FRAME_SIZE).enumerate() {
        let frame: &[f32; FRAME_SIZE] = frame.try_into().expect("chunk size is fixed");
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(frame, &mut output);
        if net.last_voice_protected() {
            protected_frames += 1;
            protected_streak += 1;
            if protected_streak >= 3 {
                let expected_start = frame_index * FRAME_SIZE - ALGORITHM_LATENCY_SAMPLES;
                for (&actual, &expected) in output
                    .iter()
                    .zip(&input[expected_start..expected_start + FRAME_SIZE])
                {
                    assert!((actual - expected).abs() <= 2.0e-5);
                }
            }
        } else {
            protected_streak = 0;
        }
    }
    assert!(
        protected_frames >= frames.saturating_sub(10),
        "sustained {fundamental_hz} Hz vowel should trigger voice protection, got {protected_frames}/{frames} frames"
    );
    assert!(protected_streak >= 3);
}
