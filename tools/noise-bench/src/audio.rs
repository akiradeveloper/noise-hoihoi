use std::{fs, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use noise_net::SAMPLE_RATE;
use sha2::{Digest, Sha256};

pub fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

pub fn read(path: &Path) -> Result<Vec<f32>> {
    read_channel(path, None)
}

pub fn read_channel(path: &Path, channel: Option<u16>) -> Result<Vec<f32>> {
    let reader =
        hound::WavReader::open(path).with_context(|| format!("reading {}", path.display()))?;
    decode(reader, channel).with_context(|| format!("decoding {}", path.display()))
}

#[allow(clippy::cast_precision_loss)]
fn decode<R: Read>(mut reader: hound::WavReader<R>, channel: Option<u16>) -> Result<Vec<f32>> {
    let spec = reader.spec();
    ensure!(
        spec.sample_rate == SAMPLE_RATE,
        "sample rate must be {SAMPLE_RATE} Hz"
    );
    ensure!(spec.channels > 0, "WAV has no channels");
    ensure!(
        spec.channels == 1 || channel.is_some(),
        "multichannel sources require an explicit --channel (1 = left); no automatic downmix"
    );
    let selected = channel.unwrap_or(1);
    ensure!(
        (1..=spec.channels).contains(&selected),
        "channel {selected} is outside 1..={}",
        spec.channels
    );
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    ensure!(!samples.is_empty(), "empty WAV");
    ensure!(
        samples.len() % usize::from(spec.channels) == 0,
        "incomplete multichannel frame"
    );
    ensure!(
        samples.iter().all(|s| s.is_finite()),
        "WAV contains nonfinite samples"
    );
    Ok(samples
        .into_iter()
        .skip(usize::from(selected - 1))
        .step_by(usize::from(spec.channels))
        .collect())
}

pub fn write(path: &Path, samples: &[f32]) -> Result<()> {
    ensure!(samples.iter().all(|s| s.is_finite()), "nonfinite output");
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

pub fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&s| f64::from(s).powi(2)).sum()
}

/// Finite reporting floor, not an acoustic measurement of 200 dB dynamic range.
pub fn ratio_db(reference: f64, measured: f64) -> f64 {
    10.0 * (reference.max(1.0e-30) / measured.max(reference * 1.0e-20).max(1.0e-30)).log10()
}

/// Preserve both component gains when headroom is needed; never clip mixtures.
pub fn mix(
    speech: &[f32],
    noise: &[f32],
    gain_db: f32,
    snr_db: f32,
) -> Result<(Vec<f32>, Vec<f32>, f32)> {
    ensure!(
        gain_db.is_finite() && snr_db.is_finite(),
        "gains must be finite"
    );
    ensure!(speech.len() == noise.len(), "component lengths differ");
    let speech_energy = energy(speech);
    let noise_energy = energy(noise);
    ensure!(
        speech_energy > 0.0 && noise_energy > 0.0,
        "SNR needs non-silent sources"
    );
    #[allow(clippy::cast_possible_truncation)]
    let noise_gain = (speech_energy / noise_energy).sqrt() as f32 * 10.0_f32.powf(-snr_db / 20.0);
    let gain = 10.0_f32.powf(gain_db / 20.0);
    let peak = speech
        .iter()
        .zip(noise)
        .map(|(s, n)| ((s + n * noise_gain) * gain).abs())
        .fold(0.0, f32::max);
    ensure!(peak.is_finite(), "mixture gain overflow");
    let headroom = (0.98 / peak.max(0.98)).min(1.0);
    Ok((
        speech.iter().map(|s| s * gain * headroom).collect(),
        noise
            .iter()
            .map(|n| n * noise_gain * gain * headroom)
            .collect(),
        headroom,
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{decode, energy, mix, ratio_db};

    #[test]
    fn explicit_channel_selection_preserves_level_and_rejects_implicit_downmix() {
        let mut bytes = Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(
            &mut bytes,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in [16_384_i16, 0, -16_384, 0] {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        let reader = || hound::WavReader::new(Cursor::new(bytes.get_ref())).unwrap();
        assert!(decode(reader(), None).is_err());
        assert!(decode(reader(), Some(0)).is_err());
        assert!(decode(reader(), Some(3)).is_err());
        let left = decode(reader(), Some(1)).unwrap();
        assert_eq!(left.len(), 2);
        assert!((left[0] - 0.5).abs() < 1.0e-6 && (left[1] + 0.5).abs() < 1.0e-6);
        assert!(
            decode(reader(), Some(2))
                .unwrap()
                .iter()
                .all(|s| s.abs() < 1.0e-6)
        );
    }

    #[test]
    fn mixture_preserves_requested_ratio_when_headroom_is_required() {
        let (s, n, headroom) = mix(&[0.9, -0.9, 0.9, -0.9], &[0.8; 4], 12.0, -5.0).unwrap();
        assert!(headroom < 1.0);
        assert!((ratio_db(energy(&s), energy(&n)) + 5.0).abs() < 1.0e-5);
        assert!(s.iter().zip(n).all(|(s, n)| (s + n).abs() <= 0.981));
    }

    #[test]
    fn rejects_undefined_snr() {
        assert!(mix(&[0.0; 4], &[1.0; 4], 0.0, 0.0).is_err());
        assert!(mix(&[1.0; 4], &[1.0; 4], f32::NAN, 0.0).is_err());
    }
}
