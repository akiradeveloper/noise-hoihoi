use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use noise_net::{ALGORITHM_LATENCY_SAMPLES, FRAME_SIZE, NoiseNet, SAMPLE_RATE, processors};

#[derive(Debug, Parser)]
#[command(about = "Enhance a 48 kHz WAV with Burn DeepFilterNet3")]
struct Args {
    /// Print selectable CPU/GPU processors and exit.
    #[arg(long)]
    list_processors: bool,

    /// Processor ID reported by --list-processors.
    #[arg(long, default_value = "cpu")]
    processor: String,

    /// Input WAV. Multiple channels are averaged to mono.
    #[arg(required_unless_present = "list_processors")]
    input: Option<PathBuf>,

    /// Destination 32-bit float mono WAV.
    #[arg(required_unless_present = "list_processors")]
    output: Option<PathBuf>,
}

#[allow(clippy::cast_precision_loss)]
fn main() -> Result<()> {
    let args = Args::parse();
    let processors = processors();
    if args.list_processors {
        for processor in processors {
            println!(
                "{}\t{}\t{}",
                processor.id(),
                processor.name(),
                processor.default_runtime()
            );
        }
        return Ok(());
    }

    let processor = processors
        .iter()
        .find(|processor| processor.id() == args.processor)
        .with_context(|| format!("processor '{}' is not available", args.processor))?;
    let input = args.input.expect("clap requires an input path");
    let output = args.output.expect("clap requires an output path");
    let samples = read_wav(&input)?;
    let mut net = NoiseNet::new(processor, processor.default_runtime())
        .with_context(|| format!("could not initialize NoiseNet on {}", processor.name()))?;
    let started = Instant::now();
    let enhanced = enhance(&mut net, &samples);
    let elapsed = started.elapsed();
    write_wav(&output, &enhanced)?;

    let audio_seconds = samples.len() as f64 / f64::from(SAMPLE_RATE);
    let real_time_factor = elapsed.as_secs_f64() / audio_seconds.max(f64::EPSILON);
    eprintln!(
        "enhanced {:.2}s in {:.2}s (RTF {:.3})",
        audio_seconds,
        elapsed.as_secs_f64(),
        real_time_factor
    );
    Ok(())
}

fn enhance(net: &mut NoiseNet, samples: &[f32]) -> Vec<f32> {
    let input_frames = samples.len().div_ceil(FRAME_SIZE);
    let flush_frames = ALGORITHM_LATENCY_SAMPLES.div_ceil(FRAME_SIZE);
    let mut delayed = Vec::with_capacity((input_frames + flush_frames) * FRAME_SIZE);
    for frame_index in 0..input_frames + flush_frames {
        let mut input = [0.0; FRAME_SIZE];
        if frame_index < input_frames {
            let start = frame_index * FRAME_SIZE;
            let end = (start + FRAME_SIZE).min(samples.len());
            input[..end - start].copy_from_slice(&samples[start..end]);
        }
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(&input, &mut output);
        delayed.extend(output);
    }
    delayed[ALGORITHM_LATENCY_SAMPLES..ALGORITHM_LATENCY_SAMPLES + samples.len()].to_vec()
}

#[allow(clippy::cast_precision_loss)]
fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("could not open {}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE {
        bail!(
            "{} Hz input is unsupported; NoiseNet requires {SAMPLE_RATE} Hz",
            spec.sample_rate
        );
    }
    if spec.channels == 0 {
        bail!("input WAV has no channels");
    }
    let interleaved = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .context("could not decode float WAV")?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let scale = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i16>()
                .map(|sample| sample.map(|value| f32::from(value) / scale))
                .collect::<Result<Vec<_>, _>>()
                .context("could not decode integer WAV")?
        }
        hound::SampleFormat::Int if spec.bits_per_sample <= 32 => {
            let scale = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .context("could not decode integer WAV")?
        }
        hound::SampleFormat::Int => {
            bail!("{}-bit integer WAV is unsupported", spec.bits_per_sample)
        }
    };
    let channels = usize::from(spec.channels);
    let scale = 1.0 / f32::from(spec.channels);
    Ok(interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() * scale)
        .collect())
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("could not create {}", path.display()))?;
    for &sample in samples {
        writer.write_sample(sample.clamp(-1.0, 1.0))?;
    }
    writer.finalize()?;
    Ok(())
}
