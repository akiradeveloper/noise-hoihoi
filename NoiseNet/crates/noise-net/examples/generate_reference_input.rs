use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use noise_net::{FRAME_SIZE, SAMPLE_RATE};

const FRAMES: usize = 120;

#[derive(Debug, Parser)]
struct Args {
    output: PathBuf,
    /// Number of 10 ms frames to generate.
    #[arg(long, default_value_t = FRAMES)]
    frames: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&args.output, spec)
        .with_context(|| format!("could not create {}", args.output.display()))?;
    for index in 0..args.frames * FRAME_SIZE {
        writer.write_sample(reference_input(index))?;
    }
    writer.finalize()?;
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn reference_input(index: usize) -> f32 {
    let time = index as f32 / SAMPLE_RATE as f32;
    let syllable = (std::f32::consts::PI * 3.0 * time).sin().max(0.0);
    let voice = syllable
        * (0.18 * (std::f32::consts::TAU * 137.0 * time).sin()
            + 0.08 * (std::f32::consts::TAU * 274.0 * time).sin()
            + 0.04 * (std::f32::consts::TAU * 822.0 * time).sin());
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
