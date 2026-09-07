use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use rtrb::{Consumer, Producer};
use rubato::{
    Adjustable as _, Async, FixedAsync, Resampler as _, SincInterpolationParameters,
    WindowFunction, audioadapter_buffers::direct::SequentialSliceOfVecs,
};

use crate::{
    AudioProcessor, EngineError, PIPELINE_SAMPLE_RATE, SignalMonitorSample, metrics::SharedMetrics,
    processor::ProcessorPipeline,
};

const PROCESSING_CHUNK_MS: usize = 10;
const MAX_DRIFT_CORRECTION: f64 = 0.002;
const DRIFT_GAIN_PER_FRAME: f64 = 0.000_001;

pub(super) struct WorkerConfig {
    pub input_rate: u32,
    pub output_capacity: usize,
    pub target_output_frames: usize,
}

pub(super) struct SignalMonitorWriter {
    producer: Producer<SignalMonitorSample>,
    enabled: Arc<AtomicBool>,
}

impl SignalMonitorWriter {
    pub(super) fn new(producer: Producer<SignalMonitorSample>, enabled: Arc<AtomicBool>) -> Self {
        Self { producer, enabled }
    }
}

pub(super) fn spawn_worker<P: AudioProcessor>(
    input: Consumer<f32>,
    output: Producer<f32>,
    processor: P,
    config: WorkerConfig,
    signal_monitor: SignalMonitorWriter,
    stop: Arc<AtomicBool>,
    metrics: Arc<SharedMetrics>,
) -> Result<JoinHandle<()>, EngineError> {
    let input_rate = usize::try_from(config.input_rate)
        .map_err(|_| EngineError::UnsupportedFormat("input sample rate is too large".to_owned()))?;
    let input_chunk = (input_rate * PROCESSING_CHUNK_MS / 1_000).max(4);
    let base_ratio = f64::from(PIPELINE_SAMPLE_RATE) / f64::from(config.input_rate);
    let params = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2);
    let resampler =
        Async::<f32>::new_sinc(base_ratio, 1.01, &params, input_chunk, 1, FixedAsync::Input)
            .map_err(|error| EngineError::Start(format!("failed to create resampler: {error}")))?;
    let input_buffer = vec![vec![0.0_f32; resampler.input_frames_max()]];
    let output_buffer = vec![vec![0.0_f32; resampler.output_frames_max()]];
    let processor = ProcessorPipeline::new(processor)
        .map_err(|message| EngineError::Start(message.to_owned()))?;
    let processor_frame_size = processor.frame_size();

    thread::Builder::new()
        .name("noise-hoihoi-audio".to_owned())
        .spawn(move || {
            let mut worker = Worker {
                input,
                output,
                processor,
                config,
                base_ratio,
                resampler,
                input_buffer,
                output_buffer,
                processor_frame_size,
                signal_monitor,
                stop,
                metrics,
            };
            let result = catch_unwind(AssertUnwindSafe(|| worker.run()));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(message)) => worker.metrics.mark_stream_fault(message),
                Err(_) => worker.metrics.mark_stream_fault("audio worker panicked"),
            }
        })
        .map_err(|error| EngineError::Start(format!("failed to spawn audio worker: {error}")))
}

struct Worker<P> {
    input: Consumer<f32>,
    output: Producer<f32>,
    processor: ProcessorPipeline<P>,
    config: WorkerConfig,
    base_ratio: f64,
    resampler: Async<f32>,
    input_buffer: Vec<Vec<f32>>,
    output_buffer: Vec<Vec<f32>>,
    processor_frame_size: Option<usize>,
    signal_monitor: SignalMonitorWriter,
    stop: Arc<AtomicBool>,
    metrics: Arc<SharedMetrics>,
}

impl<P: AudioProcessor> Worker<P> {
    fn run(&mut self) -> Result<(), String> {
        let target_frames = f64::from(
            u32::try_from(self.config.target_output_frames)
                .map_err(|_| "target latency is too large".to_owned())?,
        );

        while !self.stop.load(Ordering::Acquire) {
            let buffered = self.buffered_output_frames();
            let buffered = f64::from(u32::try_from(buffered).unwrap_or(u32::MAX));
            let correction = ((target_frames - buffered) * DRIFT_GAIN_PER_FRAME)
                .clamp(-MAX_DRIFT_CORRECTION, MAX_DRIFT_CORRECTION);
            self.resampler
                .set_resample_ratio(self.base_ratio * (1.0 + correction), true)
                .map_err(|error| format!("audio worker could not adjust resampling: {error}"))?;

            let input_frames = self.resampler.input_frames_next();
            let output_frames = self.resampler.output_frames_next();
            let output_slots_needed =
                output_frames.saturating_add(self.processor_frame_size.unwrap_or_default());
            if self.input.slots() < input_frames || self.output.slots() < output_slots_needed {
                thread::park_timeout(Duration::from_millis(1));
                continue;
            }

            for sample in &mut self.input_buffer[0][..input_frames] {
                *sample = self
                    .input
                    .pop()
                    .map_err(|_| "audio worker input buffer underflow".to_owned())?;
            }

            let input_adapter = SequentialSliceOfVecs::new(&self.input_buffer, 1, input_frames)
                .map_err(|error| {
                    format!("audio worker could not read resampling input: {error}")
                })?;
            let mut output_adapter =
                SequentialSliceOfVecs::new_mut(&mut self.output_buffer, 1, output_frames).map_err(
                    |error| format!("audio worker could not write resampling output: {error}"),
                )?;
            let (_, produced_frames) = self
                .resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|error| format!("audio resampling failed: {error}"))?;

            self.process_resampled(produced_frames)?;
            self.metrics
                .set_buffered_output_frames(self.buffered_output_frames());
        }

        Ok(())
    }

    fn buffered_output_frames(&self) -> usize {
        self.config
            .output_capacity
            .saturating_sub(self.output.slots())
    }

    fn process_resampled(&mut self, produced_frames: usize) -> Result<(), String> {
        let samples = &self.output_buffer[0][..produced_frames];
        let output = &mut self.output;
        let signal_monitor = &mut self.signal_monitor;
        let metrics = &self.metrics;
        let report = self.processor.process(samples, |input, output_samples| {
            publish(output, signal_monitor, metrics, input, output_samples)
        })?;
        self.metrics
            .add_processed_frames(u64::try_from(report.processed_samples).unwrap_or(u64::MAX));
        self.metrics
            .observe_processing(report.max_processing_time, report.deadline_misses);
        Ok(())
    }
}

fn publish(
    output_ring: &mut Producer<f32>,
    signal_monitor: &mut SignalMonitorWriter,
    metrics: &SharedMetrics,
    aligned_input: &[f32],
    output: &[f32],
) -> Result<(), String> {
    let monitor_enabled = signal_monitor.enabled.load(Ordering::Acquire);
    let mut dropped_frames = 0_u64;
    for (&input, &output) in aligned_input.iter().zip(output) {
        if monitor_enabled
            && signal_monitor
                .producer
                .push(SignalMonitorSample::new(input, output))
                .is_err()
        {
            dropped_frames += 1;
        }
        output_ring
            .push(output)
            .map_err(|_| "audio worker output buffer overflow".to_owned())?;
    }
    if dropped_frames != 0 {
        metrics.add_dropped_signal_monitor_frames(dropped_frames);
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn stop_worker(stop: &AtomicBool, worker: JoinHandle<()>) {
    stop.store(true, Ordering::Release);
    worker.thread().unpark();
    let _ = worker.join();
}
