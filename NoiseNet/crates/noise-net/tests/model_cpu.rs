use std::io::Cursor;

use noise_net::{ALGORITHM_LATENCY_SAMPLES, FRAME_SIZE, NoiseNet, SAMPLE_RATE};

const REFERENCE_FRAMES: usize = 120;
const REFERENCE_WAV: &[u8] = include_bytes!("../testdata/dfn3-reference-synthetic.wav");
const CLEAN_SPEECH_WAV: &[u8] = include_bytes!("../testdata/ian-skillen-clean-48k.wav");

#[allow(clippy::cast_precision_loss)]
fn reference_input(index: usize) -> f32 {
    let time = index as f32 / SAMPLE_RATE as f32;
    let voice = reference_voice(index);
    let fan = 0.025 * (std::f32::consts::TAU * 83.0 * time).sin();
    let click_phase = index % 9_600;
    let click = if click_phase < 180 {
        0.22 * (1.0 - click_phase as f32 / 180.0) * (std::f32::consts::TAU * 3_700.0 * time).sin()
    } else {
        0.0
    };
    let pseudo_noise =
        ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16) & 0x7fff) as f32 / 16_384.0
            - 1.0;
    voice + fan + click + 0.008 * pseudo_noise
}

#[allow(clippy::cast_precision_loss)]
fn reference_voice(index: usize) -> f32 {
    let time = index as f32 / SAMPLE_RATE as f32;
    let syllable = (std::f32::consts::PI * 3.0 * time).sin().max(0.0);
    syllable
        * (0.18 * (std::f32::consts::TAU * 137.0 * time).sin()
            + 0.08 * (std::f32::consts::TAU * 274.0 * time).sin()
            + 0.04 * (std::f32::consts::TAU * 822.0 * time).sin())
}

fn process_reference_input(net: &mut NoiseNet, frames: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(frames * FRAME_SIZE);
    for frame_index in 0..frames {
        let input =
            std::array::from_fn(|sample| reference_input(frame_index * FRAME_SIZE + sample));
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(&input, &mut output);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(net.last_local_snr_db().is_finite());
        result.extend(output);
    }
    result
}

fn official_reference() -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(REFERENCE_WAV))
        .expect("embedded official reference must be a WAV file");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, SAMPLE_RATE);
    assert_eq!(spec.bits_per_sample, 16);
    reader
        .into_samples::<i16>()
        .map(|sample| f32::from(sample.expect("reference WAV must be valid")) / f32::from(i16::MAX))
        .collect()
}

fn clean_speech() -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(CLEAN_SPEECH_WAV))
        .expect("embedded clean-speech fixture must be a WAV file");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, SAMPLE_RATE);
    assert_eq!(spec.bits_per_sample, 16);
    let mut samples = reader
        .into_samples::<i16>()
        .map(|sample| {
            f32::from(sample.expect("clean-speech WAV must be valid")) / f32::from(i16::MAX)
        })
        .collect::<Vec<_>>();
    samples.truncate(samples.len() / FRAME_SIZE * FRAME_SIZE);
    samples
}

#[allow(clippy::cast_precision_loss)]
fn keyboard_click(index: usize) -> f32 {
    const INTERVAL: usize = 12_000;
    const CLICK_LENGTH: usize = 960;

    let phase = index % INTERVAL;
    if phase >= CLICK_LENGTH {
        return 0.0;
    }

    let time = index as f32 / SAMPLE_RATE as f32;
    let envelope = (-(phase as f32) / 170.0).exp();
    let pseudo_noise =
        ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16) & 0x7fff) as f32 / 16_384.0
            - 1.0;
    envelope
        * (0.38 * pseudo_noise
            + 0.22 * (std::f32::consts::TAU * 2_900.0 * time).sin()
            + 0.16 * (std::f32::consts::TAU * 5_300.0 * time).sin())
}

fn process_samples(net: &mut NoiseNet, samples: &[f32]) -> Vec<f32> {
    let mut result = Vec::with_capacity(samples.len());
    for input in samples.chunks_exact(FRAME_SIZE) {
        let input: &[f32; FRAME_SIZE] = input.try_into().expect("chunk size is fixed");
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(input, &mut output);
        assert!(output.iter().all(|sample| sample.is_finite()));
        result.extend(output);
    }
    result
}

#[allow(clippy::cast_precision_loss)]
fn assert_keyboard_reduction(net: &mut NoiseNet) {
    let clean = clean_speech();
    let noisy = clean
        .iter()
        .enumerate()
        .map(|(index, clean)| (clean + keyboard_click(index)).clamp(-1.0, 1.0))
        .collect::<Vec<_>>();

    net.reset();
    let enhanced_clean = process_samples(net, &clean);
    net.reset();
    let enhanced_noisy = process_samples(net, &noisy);

    let compared_samples = clean.len() - ALGORITHM_LATENCY_SAMPLES;
    let (input_noise_energy, output_noise_energy) =
        (0..compared_samples).fold((0.0_f32, 0.0_f32), |(input_sum, output_sum), index| {
            let input_residual = noisy[index] - clean[index];
            let output_index = index + ALGORITHM_LATENCY_SAMPLES;
            let output_residual = enhanced_noisy[output_index] - enhanced_clean[output_index];
            (
                input_sum + input_residual * input_residual,
                output_sum + output_residual * output_residual,
            )
        });
    let input_rms = (input_noise_energy / compared_samples as f32).sqrt();
    let output_rms = (output_noise_energy / compared_samples as f32).sqrt();
    let reduction_db = 20.0 * (input_rms / output_rms).log10();
    eprintln!(
        "CC0 speech + keyboard residual: input={input_rms:e}, output={output_rms:e}, reduction={reduction_db:.2} dB"
    );
    assert!(
        reduction_db >= 3.0,
        "keyboard-noise residual must be reduced by at least 3 dB, got {reduction_db:.2} dB"
    );
}

#[test]
#[allow(clippy::cast_precision_loss)]
fn model_reference_suite() {
    let mut net = NoiseNet::new_cpu().expect("Burn Flex is required for NoiseNet v0.3");
    let actual = process_reference_input(&mut net, REFERENCE_FRAMES);
    let expected = official_reference();

    assert_eq!(actual.len(), REFERENCE_FRAMES * FRAME_SIZE);
    assert_eq!(actual.len(), expected.len());
    let errors = actual
        .iter()
        .zip(&expected)
        .map(|(actual, expected)| (actual - expected).abs())
        .collect::<Vec<_>>();
    let max_error = errors.iter().copied().fold(0.0_f32, f32::max);
    let rmse = (errors.iter().map(|error| error * error).sum::<f32>() / errors.len() as f32).sqrt();
    eprintln!("official reference error: rmse={rmse:e}, max={max_error:e}");
    assert!(
        rmse <= 5.0e-5 && max_error <= 2.0e-4,
        "Burn output differs from the pinned official DeepFilterNet3 reference: rmse={rmse:e}, max={max_error:e}"
    );

    net.reset();
    let after_reset = process_reference_input(&mut net, 12);
    for (first, reset) in actual.iter().zip(after_reset) {
        assert!((first - reset).abs() <= 1.0e-6);
    }

    assert_keyboard_reduction(&mut net);
}
