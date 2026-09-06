use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use noise_net::{FRAME_SIZE, SAMPLE_RATE};

#[path = "../testdata/reference_input.rs"]
mod reference_input;

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
        writer.write_sample(reference_input::sample(index))?;
    }
    writer.finalize()?;
    Ok(())
}
