use std::io::Cursor;

use df::{Complex32, DFState};
use noise_net::{ALGORITHM_LATENCY_SAMPLES, FFT_BINS, FRAME_SIZE, NoiseNet, SAMPLE_RATE};

const CONTROLLER: &[u8] = include_bytes!("../testdata/recorded-controller-48k.wav");
const VOICES: [(&str, &[u8]); 3] = [
    (
        "low-a",
        include_bytes!("../testdata/recorded-low-a-48k.wav"),
    ),
    (
        "high-a",
        include_bytes!("../testdata/recorded-high-a-48k.wav"),
    ),
    (
        "speech",
        include_bytes!("../testdata/recorded-speech-48k.wav"),
    ),
];

fn recording(bytes: &[u8]) -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().bits_per_sample, 16);
    reader
        .into_samples::<i16>()
        .map(|sample| f32::from(sample.unwrap()) / 32_768.0)
        .collect()
}

fn process(net: &mut NoiseNet, samples: &[f32]) -> Vec<f32> {
    net.reset();
    let frames = samples.len().div_ceil(FRAME_SIZE);
    let flush = ALGORITHM_LATENCY_SAMPLES.div_ceil(FRAME_SIZE);
    let mut delayed = Vec::with_capacity((frames + flush) * FRAME_SIZE);
    for index in 0..frames + flush {
        let mut input = [0.0; FRAME_SIZE];
        let mut output = [0.0; FRAME_SIZE];
        if index < frames {
            let start = index * FRAME_SIZE;
            let end = (start + FRAME_SIZE).min(samples.len());
            input[..end - start].copy_from_slice(&samples[start..end]);
        }
        net.process_frame(&input, &mut output);
        assert!(output.iter().all(|sample| sample.is_finite()));
        delayed.extend(output);
    }
    delayed[ALGORITHM_LATENCY_SAMPLES..ALGORITHM_LATENCY_SAMPLES + samples.len()].to_vec()
}

fn energy(samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|&sample| f64::from(sample).powi(2))
        .sum()
}

fn attenuation(input_energy: f64, output_energy: f64) -> f64 {
    10.0 * (input_energy / output_energy.max(1.0e-30)).log10()
}

/// Inspect noise-dominated and speech-dominated time/frequency cells separately.
/// Inference is nonlinear: subtracting two model outputs would also count
/// changes in the voice as "noise", even when the click itself was removed.
/// Here the known input components identify cells with at least 20 dB of
/// separation. Noise is measured only while the source voice is active.
#[allow(clippy::cast_precision_loss)]
fn speech_and_noise_attenuation(speech: &[f32], noise: &[f32], output: &[f32]) -> (f64, f64) {
    let mut speech_dsp = DFState::default();
    let mut noise_dsp = DFState::default();
    let mut output_dsp = DFState::default();
    let mut speech_spectrum = [Complex32::new(0.0, 0.0); FFT_BINS];
    let mut noise_spectrum = speech_spectrum;
    let mut output_spectrum = speech_spectrum;
    let speech_activity = energy(speech) / speech.len() as f64 * 0.04;
    let mut speech_input = 0.0;
    let mut speech_output = 0.0;
    let mut noise_input = 0.0;
    let mut noise_output = 0.0;
    let mut noise_cells = 0;
    for ((speech_frame, noise_frame), output_frame) in speech
        .chunks_exact(FRAME_SIZE)
        .zip(noise.chunks_exact(FRAME_SIZE))
        .zip(output.chunks_exact(FRAME_SIZE))
    {
        speech_dsp.analysis(speech_frame, &mut speech_spectrum);
        noise_dsp.analysis(noise_frame, &mut noise_spectrum);
        output_dsp.analysis(output_frame, &mut output_spectrum);
        if energy(speech_frame) / FRAME_SIZE as f64 <= speech_activity {
            continue;
        }
        let noise_peak = noise_spectrum
            .iter()
            .map(Complex32::norm_sqr)
            .fold(0.0, f32::max);
        for ((speech_bin, noise_bin), output_bin) in speech_spectrum
            .iter()
            .zip(&noise_spectrum)
            .zip(&output_spectrum)
        {
            let s = f64::from(speech_bin.norm_sqr());
            let n = f64::from(noise_bin.norm_sqr());
            let y = f64::from(output_bin.norm_sqr());
            if s > 100.0 * n {
                speech_input += s;
                speech_output += y;
            }
            if n > 100.0 * s && n > f64::from(noise_peak) * 0.001 {
                noise_input += n;
                noise_output += y;
                noise_cells += 1;
            }
        }
    }
    assert!(
        noise_cells >= 100,
        "fixture must exercise noise during speech"
    );
    assert!(speech_input > 0.0 && noise_input > 0.0);
    (
        attenuation(speech_input, speech_output),
        attenuation(noise_input, noise_output),
    )
}

#[test]
fn controller_recording_is_suppressed_without_speech() {
    let input = recording(CONTROLLER);
    let mut net = NoiseNet::new_cpu().unwrap();
    let output = process(&mut net, &input);
    let reduction_db = attenuation(energy(&input), energy(&output));
    eprintln!("recorded controller alone: reduction={reduction_db:.2} dB");
    assert!(
        reduction_db >= 30.0,
        "controller residual: {reduction_db:.2} dB"
    );
}

fn assert_recorded_voice(name: &str, bytes: &[u8]) {
    let controller = recording(CONTROLLER);
    let mut net = NoiseNet::new_cpu().unwrap();
    let speech = recording(bytes);
    let clean_output = process(&mut net, &speech);
    let clean_attenuation = attenuation(energy(&speech), energy(&clean_output));
    eprintln!("recorded {name}: clean attenuation={clean_attenuation:.2} dB");
    assert!(
        clean_attenuation.abs() <= 3.0,
        "{name} changed by {clean_attenuation:.2} dB"
    );

    // Each voice starts at a different point in the click recording, so
    // this also exercises clicks landing on different phonemes.
    let offset = match name {
        "low-a" => 0,
        "high-a" => 3_700,
        _ => 11_700,
    };
    let noise = (0..speech.len())
        .map(|index| controller[(index + offset) % controller.len()])
        .collect::<Vec<_>>();
    let mixed = speech
        .iter()
        .zip(&noise)
        .map(|(s, n)| s + n)
        .collect::<Vec<_>>();
    assert!(mixed.iter().all(|sample| sample.abs() < 1.0));
    let output = process(&mut net, &mixed);
    let (speech_db, noise_db) = speech_and_noise_attenuation(&speech, &noise, &output);
    eprintln!(
        "recorded {name} + controller: speech attenuation={speech_db:.2} dB, noise-band reduction={noise_db:.2} dB"
    );
    assert!(
        speech_db.abs() <= 3.0,
        "{name} voice changed by {speech_db:.2} dB"
    );
    assert!(noise_db >= 8.0, "{name} click residual: {noise_db:.2} dB");
}

#[allow(clippy::cast_precision_loss)]
fn assert_sustained_vowel(fundamental: f32, level: f32) {
    let controller = recording(CONTROLLER);
    let mut net = NoiseNet::new_cpu().unwrap();
    let speech = (0..controller.len())
        .map(|index| {
            let phase = std::f32::consts::TAU * fundamental * index as f32 / SAMPLE_RATE as f32;
            0.08 * phase.sin() + 0.035 * (2.0 * phase).sin() + 0.015 * (3.0 * phase).sin()
        })
        .collect::<Vec<_>>();
    let noise = controller
        .iter()
        .map(|sample| sample * level)
        .collect::<Vec<_>>();
    let mixed = speech
        .iter()
        .zip(&noise)
        .map(|(s, n)| s + n)
        .collect::<Vec<_>>();
    // Keep valid microphone levels without clipping the source. Apply
    // the same gain to both components to retain their noise ratio.
    let peak = mixed.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
    let scale = (0.95 / peak).min(1.0);
    let speech = speech
        .iter()
        .map(|sample| sample * scale)
        .collect::<Vec<_>>();
    let noise = noise
        .iter()
        .map(|sample| sample * scale)
        .collect::<Vec<_>>();
    let mixed = mixed
        .iter()
        .map(|sample| sample * scale)
        .collect::<Vec<_>>();
    let output = process(&mut net, &mixed);
    let (speech_db, noise_db) = speech_and_noise_attenuation(&speech, &noise, &output);
    eprintln!(
        "{fundamental} Hz vowel + controller x{level}: speech attenuation={speech_db:.2} dB, noise-band reduction={noise_db:.2} dB"
    );
    assert!(
        speech_db.abs() <= 3.0,
        "vowel level changed by {speech_db:.2} dB"
    );
    assert!(noise_db >= 8.0, "click residual: {noise_db:.2} dB");
}

#[test]
fn recorded_low_voice_survives_controller() {
    let (name, bytes) = VOICES[0];
    assert_recorded_voice(name, bytes);
}

#[test]
fn recorded_high_voice_survives_controller() {
    let (name, bytes) = VOICES[1];
    assert_recorded_voice(name, bytes);
}

#[test]
fn recorded_speech_survives_controller() {
    let (name, bytes) = VOICES[2];
    assert_recorded_voice(name, bytes);
}

#[test]
fn low_vowel_with_quiet_controller() {
    assert_sustained_vowel(105.0, 0.5);
}

#[test]
fn low_vowel_with_normal_controller() {
    assert_sustained_vowel(105.0, 1.0);
}

#[test]
fn low_vowel_with_loud_controller() {
    assert_sustained_vowel(105.0, 2.0);
}

#[test]
fn high_vowel_with_quiet_controller() {
    assert_sustained_vowel(260.0, 0.5);
}

#[test]
fn high_vowel_with_normal_controller() {
    assert_sustained_vowel(260.0, 1.0);
}

#[test]
fn high_vowel_with_loud_controller() {
    assert_sustained_vowel(260.0, 2.0);
}
