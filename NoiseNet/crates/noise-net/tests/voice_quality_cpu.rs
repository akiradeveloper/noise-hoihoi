use std::io::Cursor;

use noise_net::{ALGORITHM_LATENCY_SAMPLES, FRAME_SIZE, NoiseNet, SAMPLE_RATE};

const RECORDED_VOICE: &[u8] = include_bytes!("../../../../sample-sound/aiueo.wav");
const RECORDED_NOISES: [&[u8]; 2] = [
    include_bytes!("../../../../sample-sound/controller.wav"),
    include_bytes!("../../../../sample-sound/keyboard.wav"),
];

fn left_channel(bytes: &[u8]) -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, SAMPLE_RATE);
    assert_eq!(spec.bits_per_sample, 16);
    assert!(spec.channels >= 1);
    // The supplied stereo recordings have a silent right channel. Preserve
    // the left channel's original level instead of averaging the two.
    reader
        .into_samples::<i16>()
        .step_by(usize::from(spec.channels))
        .map(|v| f32::from(v.unwrap()) / 32_768.0)
        .collect()
}

fn speech() -> Vec<f32> {
    hound::WavReader::new(Cursor::new(include_bytes!(
        "../testdata/ian-skillen-clean-48k.wav"
    )))
    .unwrap()
    .into_samples::<i16>()
    .map(|v| f32::from(v.unwrap()) / 32_768.0)
    .collect()
}

fn process(net: &mut NoiseNet, samples: &[f32]) -> Vec<f32> {
    let mut delayed = vec![];
    for frame in 0..samples.len().div_ceil(FRAME_SIZE) + 3 {
        let mut input = [0.0; FRAME_SIZE];
        let start = frame * FRAME_SIZE;
        if start < samples.len() {
            let end = (start + FRAME_SIZE).min(samples.len());
            input[..end - start].copy_from_slice(&samples[start..end]);
        }
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(&input, &mut output);
        assert!(output.iter().all(|v| v.is_finite()));
        delayed.extend(output);
    }
    delayed[ALGORITHM_LATENCY_SAMPLES..ALGORITHM_LATENCY_SAMPLES + samples.len()].to_vec()
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&v| f64::from(v).powi(2)).sum()
}

fn reference_sdr(input: &[f32], output: &[f32]) -> f64 {
    assert_eq!(input.len(), output.len());
    let error = input
        .iter()
        .zip(output)
        .map(|(s, y)| (f64::from(*s) - f64::from(*y)).powi(2))
        .sum::<f64>();
    10.0 * (energy(input) / error.max(1.0e-30)).log10()
}

#[allow(clippy::cast_precision_loss)]
fn assert_preserved(input: &[f32], output: &[f32]) {
    let attenuation = 10.0 * (energy(input) / energy(output).max(1.0e-30)).log10();
    let sdr = reference_sdr(input, output);
    let threshold = energy(input) / input.len() as f64 * 0.04;
    let mut active = 0;
    let mut drops = 0;
    for (s, y) in input
        .chunks_exact(FRAME_SIZE)
        .zip(output.chunks_exact(FRAME_SIZE))
    {
        if energy(s) / FRAME_SIZE as f64 > threshold {
            active += 1;
            if energy(y) < energy(s) * 0.25 {
                drops += 1;
            }
        }
    }
    eprintln!(
        "voice attenuation={attenuation:.3} dB, reference SDR={sdr:.3} dB, drops={drops}/{active}"
    );
    assert!(
        attenuation.abs() < 1.0,
        "voice level changed by {attenuation:.2} dB"
    );
    assert!(sdr > 20.0, "voice distortion: SDR={sdr:.2} dB");
    assert!(
        drops * 100 < active * 3,
        "too many 10 ms voice intervals lost over 6 dB"
    );
}

#[test]
fn quiet_clean_speech_survives_without_output_gain_compensation() {
    let source = speech();
    let mut net = NoiseNet::new_cpu().unwrap();
    for gain in [1.0, 0.251_188_64, 0.063_095_73, 0.015_848_93] {
        net.reset();
        let input = source.iter().map(|v| v * gain).collect::<Vec<_>>();
        let output = process(&mut net, &input);
        assert_preserved(&input, &output);
    }
}

#[test]
fn product_reset_clears_level_and_recurrent_history() {
    let input = speech();
    let mut net = NoiseNet::new_cpu().unwrap();
    let first = process(&mut net, &input);
    let quiet = input.iter().map(|v| v * 0.01).collect::<Vec<_>>();
    let _ = process(&mut net, &quiet);
    net.reset();
    let reset = process(&mut net, &input);
    assert!(first.iter().zip(reset).all(|(a, b)| (a - b).abs() < 1.0e-6));
}

#[test]
fn a_loud_to_quiet_transition_does_not_erase_the_next_utterance() {
    let source = speech();
    let quiet = source.iter().map(|v| v * 0.063_095_73).collect::<Vec<_>>();
    let mut input = source.clone();
    input.extend(&quiet);
    let mut net = NoiseNet::new_cpu().unwrap();
    let output = process(&mut net, &input);
    assert_preserved(&quiet, &output[source.len()..]);
}

#[test]
fn updated_recorded_voice_survives_at_normal_and_quiet_levels() {
    let source = left_channel(RECORDED_VOICE);
    let mut net = NoiseNet::new_cpu().unwrap();
    for gain in [1.0, 0.063_095_73] {
        net.reset();
        let input = source.iter().map(|s| s * gain).collect::<Vec<_>>();
        assert_preserved(&input, &process(&mut net, &input));
    }
}

#[test]
fn updated_operation_sounds_are_suppressed_without_voice() {
    let mut net = NoiseNet::new_cpu().unwrap();
    for bytes in RECORDED_NOISES {
        let input = left_channel(bytes);
        net.reset();
        let output = process(&mut net, &input);
        let reduction = 10.0 * (energy(&input) / energy(&output).max(1.0e-30)).log10();
        eprintln!("updated operation sound: reduction={reduction:.2} dB");
        assert!(
            reduction > 25.0,
            "operation sound remains: {reduction:.2} dB"
        );
    }
}

#[allow(clippy::cast_possible_truncation)]
fn assert_updated_mixture_recovery(bytes: &[u8]) {
    let source = left_channel(RECORDED_VOICE);
    let mut net = NoiseNet::new_cpu().unwrap();
    let noise = left_channel(bytes);
    let cyclic = (0..source.len())
        .map(|i| noise[(i + 13_700) % noise.len()])
        .collect::<Vec<_>>();
    for snr in [0.0_f32, 10.0] {
        let noise_gain =
            (energy(&source) / energy(&cyclic)).sqrt() as f32 * 10.0_f32.powf(-snr / 20.0);
        // Quiet input, with a common headroom scale preserving the SNR.
        let peak = source
            .iter()
            .zip(&cyclic)
            .map(|(s, n)| (s + n * noise_gain).abs())
            .fold(0.0, f32::max);
        let gain = 0.063_095_73_f32.min(0.98 / peak);
        let reference = source.iter().map(|s| s * gain).collect::<Vec<_>>();
        let input = source
            .iter()
            .zip(&cyclic)
            .map(|(s, n)| (s + n * noise_gain) * gain)
            .collect::<Vec<_>>();
        assert!(input.iter().all(|s| s.abs() < 1.0));
        net.reset();
        let sdr = reference_sdr(&reference, &process(&mut net, &input));
        eprintln!("updated mixture: input SNR={snr:.0}, output reference SDR={sdr:.2} dB");
        // Bound combined voice distortion and residual noise. These
        // thresholds also reject muting, pass-through and the old model.
        let minimum = if snr < 5.0 { 12.0 } else { 18.0 };
        assert!(sdr > minimum, "mixture recovery failed: SDR={sdr:.2} dB");
    }
}

// Keep the independent recordings separate so each case stays within the
// normal CPU test group's timeout even in an unoptimized build.
#[test]
fn updated_recorded_voice_is_recovered_from_controller_mixtures() {
    assert_updated_mixture_recovery(RECORDED_NOISES[0]);
}

#[test]
fn updated_recorded_voice_is_recovered_from_keyboard_mixtures() {
    assert_updated_mixture_recovery(RECORDED_NOISES[1]);
}

#[test]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn strong_controller_overlap_does_not_erase_short_voice_intervals() {
    let source = left_channel(RECORDED_VOICE);
    let controller = left_channel(RECORDED_NOISES[0]);
    let noise = (0..source.len())
        .map(|i| controller[(i + 13_700) % controller.len()])
        .collect::<Vec<_>>();
    // Controller energy is ten times speech energy: -10 dB whole-clip SNR.
    let noise_gain = (10.0 * energy(&source) / energy(&noise)).sqrt() as f32;
    let peak = source
        .iter()
        .zip(&noise)
        .map(|(s, n)| (s + n * noise_gain).abs())
        .fold(0.0, f32::max);
    let gain = 0.063_095_73_f32.min(0.98 / peak);
    let speech = source.iter().map(|s| s * gain).collect::<Vec<_>>();
    let input = source
        .iter()
        .zip(noise)
        .map(|(s, n)| (s + n * noise_gain) * gain)
        .collect::<Vec<_>>();
    let output = process(&mut NoiseNet::new_cpu().unwrap(), &input);
    let threshold = energy(&speech) / speech.len() as f64 * 0.04;
    let mut worst_attenuation = 0.0_f64;
    let mut active = 0;
    for (s, y) in speech.chunks_exact(2_400).zip(output.chunks_exact(2_400)) {
        if energy(s) / 2_400.0 > threshold {
            active += 1;
            let attenuation = 10.0 * (energy(s) / energy(y).max(1.0e-30)).log10();
            worst_attenuation = worst_attenuation.max(attenuation);
        }
    }
    let sdr = reference_sdr(&speech, &output);
    eprintln!(
        "strong controller: worst active 50 ms attenuation={worst_attenuation:.2} dB, reference SDR={sdr:.2} dB"
    );
    assert!(active > 100, "recording must exercise many voice intervals");
    assert!(
        worst_attenuation < 12.0,
        "short voice interval was erased: {worst_attenuation:.2} dB"
    );
    // Residual noise can disguise level loss. Also bound the combined voice
    // error and noise; pass-through, muting and simple amplification fail this.
    assert!(sdr > 10.5, "poor voice/noise recovery: SDR={sdr:.2} dB");
}
