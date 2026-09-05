use std::{sync::Arc, thread};

use cpal::{
    Device, Error as CpalError, ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream,
    StreamConfig, SupportedStreamConfig, traits::DeviceTrait as _,
};
use rtrb::{Consumer, Producer};

use crate::{EngineError, metrics::SharedMetrics};

pub(super) fn build_input_stream(
    device: &Device,
    format: &SupportedStreamConfig,
    channels: usize,
    producer: Producer<f32>,
    worker: thread::Thread,
    metrics: Arc<SharedMetrics>,
) -> Result<Stream, EngineError> {
    let config = format.config();
    match format.sample_format() {
        SampleFormat::I8 => build_input::<i8>(device, config, channels, producer, worker, metrics),
        SampleFormat::I16 => {
            build_input::<i16>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::I24 => {
            build_input::<cpal::I24>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::I32 => {
            build_input::<i32>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::I64 => {
            build_input::<i64>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::U8 => build_input::<u8>(device, config, channels, producer, worker, metrics),
        SampleFormat::U16 => {
            build_input::<u16>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::U24 => {
            build_input::<cpal::U24>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::U32 => {
            build_input::<u32>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::U64 => {
            build_input::<u64>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::F32 => {
            build_input::<f32>(device, config, channels, producer, worker, metrics)
        }
        SampleFormat::F64 => {
            build_input::<f64>(device, config, channels, producer, worker, metrics)
        }
        unsupported => Err(EngineError::UnsupportedFormat(format!(
            "unsupported input sample format {unsupported}"
        ))),
    }
}

fn build_input<T>(
    device: &Device,
    config: StreamConfig,
    channels: usize,
    mut producer: Producer<f32>,
    worker: thread::Thread,
    metrics: Arc<SharedMetrics>,
) -> Result<Stream, EngineError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let error_metrics = Arc::clone(&metrics);
    let channel_count = u16::try_from(channels).map_err(|_| {
        EngineError::UnsupportedFormat("input channel count is too large".to_owned())
    })?;
    let channel_scale = 1.0 / f32::from(channel_count);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |samples, _| {
                let mut peak = 0.0_f32;
                let mut dropped_frames = 0_u64;
                for frame in samples.chunks_exact(channels) {
                    let mono =
                        frame.iter().copied().map(f32::from_sample).sum::<f32>() * channel_scale;
                    peak = peak.max(mono.abs());
                    if producer.push(mono).is_err() {
                        dropped_frames += 1;
                    }
                }
                metrics.set_input_peak(peak);
                if dropped_frames != 0 {
                    metrics.add_dropped_input_frames(dropped_frames);
                }
                worker.unpark();
            },
            move |error| handle_input_stream_error(&error_metrics, &error),
            None,
        )
        .map_err(|error| EngineError::Start(format!("failed to build microphone input: {error}")))
}

pub(super) fn build_output_stream(
    device: &Device,
    format: &SupportedStreamConfig,
    channels: usize,
    consumer: Consumer<f32>,
    metrics: Arc<SharedMetrics>,
) -> Result<Stream, EngineError> {
    let config = format.config();
    match format.sample_format() {
        SampleFormat::I8 => build_output::<i8>(device, config, channels, consumer, metrics),
        SampleFormat::I16 => build_output::<i16>(device, config, channels, consumer, metrics),
        SampleFormat::I24 => build_output::<cpal::I24>(device, config, channels, consumer, metrics),
        SampleFormat::I32 => build_output::<i32>(device, config, channels, consumer, metrics),
        SampleFormat::I64 => build_output::<i64>(device, config, channels, consumer, metrics),
        SampleFormat::U8 => build_output::<u8>(device, config, channels, consumer, metrics),
        SampleFormat::U16 => build_output::<u16>(device, config, channels, consumer, metrics),
        SampleFormat::U24 => build_output::<cpal::U24>(device, config, channels, consumer, metrics),
        SampleFormat::U32 => build_output::<u32>(device, config, channels, consumer, metrics),
        SampleFormat::U64 => build_output::<u64>(device, config, channels, consumer, metrics),
        SampleFormat::F32 => build_output::<f32>(device, config, channels, consumer, metrics),
        SampleFormat::F64 => build_output::<f64>(device, config, channels, consumer, metrics),
        unsupported => Err(EngineError::UnsupportedFormat(format!(
            "unsupported output sample format {unsupported}"
        ))),
    }
}

fn build_output<T>(
    device: &Device,
    config: StreamConfig,
    channels: usize,
    mut consumer: Consumer<f32>,
    metrics: Arc<SharedMetrics>,
) -> Result<Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let error_metrics = Arc::clone(&metrics);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |samples, _| {
                let mut silence_frames = 0_u64;
                for frame in samples.chunks_exact_mut(channels) {
                    let mono = if let Ok(sample) = consumer.pop() {
                        sample
                    } else {
                        silence_frames += 1;
                        0.0
                    };
                    frame.fill(T::from_sample(mono));
                }
                if silence_frames != 0 {
                    metrics.add_inserted_silence_frames(silence_frames);
                }
                metrics.set_buffered_output_frames(consumer.slots());
            },
            move |error| handle_output_stream_error(&error_metrics, &error),
            None,
        )
        .map_err(|error| EngineError::Start(format!("failed to build VB-CABLE output: {error}")))
}

fn handle_input_stream_error(metrics: &SharedMetrics, error: &CpalError) {
    match error.kind() {
        // WASAPI reports a discontinuity as Xrun and keeps the stream alive.
        ErrorKind::Xrun => metrics.add_input_discontinuity(),
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
        _ => metrics.mark_stream_fault(format!("input stream: {error}")),
    }
}

fn handle_output_stream_error(metrics: &SharedMetrics, error: &CpalError) {
    match error.kind() {
        // As with capture, WASAPI can continue after an isolated output xrun.
        ErrorKind::Xrun => metrics.add_output_discontinuity(),
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
        _ => metrics.mark_stream_fault(format!("VB-CABLE output stream: {error}")),
    }
}
