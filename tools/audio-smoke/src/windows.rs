use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, ensure};
use clap::Parser;
use cpal::{
    Device, Error as CpalError, ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream,
    SupportedStreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};
use noise_hoihoi_platform::{
    PIPELINE_SAMPLE_RATE, VB_CABLE_PLAYBACK_ENDPOINT_NAME, VB_CABLE_RECORDING_ENDPOINT_NAME,
    is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint,
};

const TEST_FREQUENCY_HZ: f32 = 997.0;
const TEST_AMPLITUDE: f32 = 0.2;
const PIPELINE_SAMPLE_RATE_F32: f32 = 48_000.0;

#[derive(Debug, Parser)]
#[command(about = "Verify the bundled VB-CABLE render-to-capture bridge")]
struct Arguments {
    /// Seconds of known-tone audio to measure.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u64).range(1..=30))]
    seconds: u64,
}

pub fn run() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let host = cpal::default_host();
    let render = find_vb_cable_output(&host)?;
    let capture = find_vb_cable_input(&host)?;
    let render_format = format_at_48khz(&render, Direction::Output)?;
    let capture_format = format_at_48khz(&capture, Direction::Input)?;
    let stats = Arc::new(AtomicStats::default());
    let failures = Arc::new(StreamFailures::default());

    let capture_stream = build_capture(
        &capture,
        &capture_format,
        Arc::clone(&stats),
        Arc::clone(&failures),
    )?;
    let render_stream = build_render(&render, &render_format, Arc::clone(&failures))?;
    capture_stream
        .play()
        .context("failed to start virtual capture")?;
    render_stream
        .play()
        .context("failed to start virtual render")?;

    thread::sleep(Duration::from_secs(arguments.seconds));
    drop(render_stream);
    drop(capture_stream);
    failures.check()?;

    let result = stats.snapshot();
    let expected_minimum = u64::from(PIPELINE_SAMPLE_RATE) * arguments.seconds / 2;
    ensure!(
        result.samples >= expected_minimum,
        "captured only {} frames; expected at least {expected_minimum}",
        result.samples
    );
    ensure!(
        result.rms > 0.05,
        "captured signal is silent (RMS {:.4})",
        result.rms
    );
    ensure!(
        result.tone_amplitude > 0.1,
        "997 Hz tone was not preserved (amplitude {:.4})",
        result.tone_amplitude
    );
    ensure!(
        result.peak < 0.95,
        "captured signal clipped (peak {:.4})",
        result.peak
    );

    println!(
        "PASS: {} frames, RMS {:.4}, peak {:.4}, 997 Hz amplitude {:.4}",
        result.samples, result.rms, result.peak, result.tone_amplitude
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Output,
}

fn find_vb_cable_input(host: &cpal::Host) -> anyhow::Result<Device> {
    host.input_devices()
        .context("failed to enumerate capture endpoints")?
        .find(|device| {
            device
                .description()
                .is_ok_and(|description| is_vb_cable_recording_endpoint(description.name()))
        })
        .with_context(|| {
            format!("capture endpoint '{VB_CABLE_RECORDING_ENDPOINT_NAME}' is not installed")
        })
}

fn find_vb_cable_output(host: &cpal::Host) -> anyhow::Result<Device> {
    host.output_devices()
        .context("failed to enumerate render endpoints")?
        .find(|device| {
            device
                .description()
                .is_ok_and(|description| is_vb_cable_playback_endpoint(description.name()))
        })
        .with_context(|| {
            format!("render endpoint '{VB_CABLE_PLAYBACK_ENDPOINT_NAME}' is not installed")
        })
}

fn format_at_48khz(device: &Device, direction: Direction) -> anyhow::Result<SupportedStreamConfig> {
    let rate = PIPELINE_SAMPLE_RATE;
    let ranges: Box<dyn Iterator<Item = cpal::SupportedStreamConfigRange>> = match direction {
        Direction::Input => Box::new(
            device
                .supported_input_configs()
                .context("failed to query virtual capture formats")?,
        ),
        Direction::Output => Box::new(
            device
                .supported_output_configs()
                .context("failed to query virtual render formats")?,
        ),
    };
    ranges
        .filter(|range| range.contains_rate(rate) && !range.sample_format().is_dsd())
        .min_by_key(|range| sample_format_rank(range.sample_format()))
        .map(|range| range.with_sample_rate(rate))
        .context("virtual endpoint does not support 48 kHz PCM")
}

fn sample_format_rank(format: SampleFormat) -> u8 {
    match format {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::I24 => 2,
        SampleFormat::I32 => 3,
        SampleFormat::F64 => 4,
        _ if format.is_dsd() => u8::MAX,
        _ => 5,
    }
}

fn build_render(
    device: &Device,
    format: &SupportedStreamConfig,
    failures: Arc<StreamFailures>,
) -> anyhow::Result<Stream> {
    let config = format.config();
    let channels = usize::from(format.channels());
    match format.sample_format() {
        SampleFormat::I8 => render::<i8>(device, config, channels, failures),
        SampleFormat::I16 => render::<i16>(device, config, channels, failures),
        SampleFormat::I24 => render::<cpal::I24>(device, config, channels, failures),
        SampleFormat::I32 => render::<i32>(device, config, channels, failures),
        SampleFormat::I64 => render::<i64>(device, config, channels, failures),
        SampleFormat::U8 => render::<u8>(device, config, channels, failures),
        SampleFormat::U16 => render::<u16>(device, config, channels, failures),
        SampleFormat::U24 => render::<cpal::U24>(device, config, channels, failures),
        SampleFormat::U32 => render::<u32>(device, config, channels, failures),
        SampleFormat::U64 => render::<u64>(device, config, channels, failures),
        SampleFormat::F32 => render::<f32>(device, config, channels, failures),
        SampleFormat::F64 => render::<f64>(device, config, channels, failures),
        other => anyhow::bail!("unsupported render sample format {other}"),
    }
}

fn render<T>(
    device: &Device,
    config: cpal::StreamConfig,
    channels: usize,
    failures: Arc<StreamFailures>,
) -> anyhow::Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let phase_step = std::f32::consts::TAU * TEST_FREQUENCY_HZ / PIPELINE_SAMPLE_RATE_F32;
    let mut phase = 0.0_f32;
    device
        .build_output_stream::<T, _, _>(
            config,
            move |samples, _| {
                for frame in samples.chunks_exact_mut(channels) {
                    let sample = TEST_AMPLITUDE * phase.sin();
                    frame.fill(T::from_sample(sample));
                    phase = (phase + phase_step) % std::f32::consts::TAU;
                }
            },
            move |error| failures.record("virtual render", &error),
            None,
        )
        .context("failed to build virtual render stream")
}

fn build_capture(
    device: &Device,
    format: &SupportedStreamConfig,
    stats: Arc<AtomicStats>,
    failures: Arc<StreamFailures>,
) -> anyhow::Result<Stream> {
    let config = format.config();
    let channels = usize::from(format.channels());
    match format.sample_format() {
        SampleFormat::I8 => capture::<i8>(device, config, channels, stats, failures),
        SampleFormat::I16 => capture::<i16>(device, config, channels, stats, failures),
        SampleFormat::I24 => capture::<cpal::I24>(device, config, channels, stats, failures),
        SampleFormat::I32 => capture::<i32>(device, config, channels, stats, failures),
        SampleFormat::I64 => capture::<i64>(device, config, channels, stats, failures),
        SampleFormat::U8 => capture::<u8>(device, config, channels, stats, failures),
        SampleFormat::U16 => capture::<u16>(device, config, channels, stats, failures),
        SampleFormat::U24 => capture::<cpal::U24>(device, config, channels, stats, failures),
        SampleFormat::U32 => capture::<u32>(device, config, channels, stats, failures),
        SampleFormat::U64 => capture::<u64>(device, config, channels, stats, failures),
        SampleFormat::F32 => capture::<f32>(device, config, channels, stats, failures),
        SampleFormat::F64 => capture::<f64>(device, config, channels, stats, failures),
        other => anyhow::bail!("unsupported capture sample format {other}"),
    }
}

fn capture<T>(
    device: &Device,
    config: cpal::StreamConfig,
    channels: usize,
    stats: Arc<AtomicStats>,
    failures: Arc<StreamFailures>,
) -> anyhow::Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut local = Statistics::default();
    let channel_count = u16::try_from(channels).context("capture channel count is too large")?;
    let channel_scale = 1.0 / f32::from(channel_count);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |samples, _| {
                for frame in samples.chunks_exact(channels) {
                    let mono =
                        frame.iter().copied().map(f32::from_sample).sum::<f32>() * channel_scale;
                    local.push(mono);
                }
                stats.store(local);
            },
            move |error| failures.record("virtual capture", &error),
            None,
        )
        .context("failed to build virtual capture stream")
}

#[derive(Debug, Default)]
struct StreamFailures {
    first: Mutex<Option<String>>,
}

impl StreamFailures {
    fn record(&self, stream: &str, error: &CpalError) {
        match error.kind() {
            ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
            _ => {
                let mut first = self
                    .first
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if first.is_none() {
                    *first = Some(format!("{stream} stream failed: {error}"));
                }
            }
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        let first = self
            .first
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match first.as_deref() {
            Some(message) => anyhow::bail!(message.to_owned()),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Statistics {
    samples: u64,
    sum_squares: f64,
    peak: f64,
    tone_sin: f64,
    tone_cos: f64,
    tone_phase: f64,
}

impl Statistics {
    fn push(&mut self, sample: f32) {
        let value = f64::from(sample);
        self.sum_squares += value * value;
        self.peak = self.peak.max(value.abs());
        self.tone_sin += value * self.tone_phase.sin();
        self.tone_cos += value * self.tone_phase.cos();
        let phase_step =
            std::f64::consts::TAU * f64::from(TEST_FREQUENCY_HZ) / f64::from(PIPELINE_SAMPLE_RATE);
        self.tone_phase = (self.tone_phase + phase_step) % std::f64::consts::TAU;
        self.samples += 1;
    }
}

#[derive(Debug, Default)]
struct AtomicStats {
    samples: AtomicU64,
    sum_squares: AtomicU64,
    peak: AtomicU64,
    tone_sin: AtomicU64,
    tone_cos: AtomicU64,
}

impl AtomicStats {
    fn store(&self, stats: Statistics) {
        self.sum_squares
            .store(stats.sum_squares.to_bits(), Ordering::Relaxed);
        self.peak.store(stats.peak.to_bits(), Ordering::Relaxed);
        self.tone_sin
            .store(stats.tone_sin.to_bits(), Ordering::Relaxed);
        self.tone_cos
            .store(stats.tone_cos.to_bits(), Ordering::Relaxed);
        self.samples.store(stats.samples, Ordering::Release);
    }

    fn snapshot(&self) -> ResultStatistics {
        let samples = self.samples.load(Ordering::Acquire);
        if samples == 0 {
            return ResultStatistics::default();
        }
        let sum_squares = f64::from_bits(self.sum_squares.load(Ordering::Relaxed));
        let tone_sin = f64::from_bits(self.tone_sin.load(Ordering::Relaxed));
        let tone_cos = f64::from_bits(self.tone_cos.load(Ordering::Relaxed));
        let sample_count = f64::from(u32::try_from(samples).unwrap_or(u32::MAX));
        ResultStatistics {
            samples,
            rms: (sum_squares / sample_count).sqrt(),
            peak: f64::from_bits(self.peak.load(Ordering::Relaxed)),
            tone_amplitude: 2.0 * tone_sin.hypot(tone_cos) / sample_count,
        }
    }
}

#[derive(Debug, Default)]
struct ResultStatistics {
    samples: u64,
    rms: f64,
    peak: f64,
    tone_amplitude: f64,
}
