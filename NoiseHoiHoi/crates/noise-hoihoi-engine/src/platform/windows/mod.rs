mod devices;
mod streams;
use super::worker;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use cpal::{Stream, traits::StreamTrait as _};
use rtrb::RingBuffer;

use crate::config::{latency_frames, scale_frames_to_rate};
use crate::metrics::SharedMetrics;
use crate::{
    AudioProcessor, EngineConfig, EngineError, EngineState, MetricsHandle, PIPELINE_SAMPLE_RATE,
};
use streams::{build_input_stream, build_output_stream};
use worker::{SignalMonitorWriter, WorkerConfig, spawn_worker, stop_worker};

pub use devices::input_devices;

const MIN_OUTPUT_RING_FRAMES: usize = 8_192;
const SIGNAL_MONITOR_RING_FRAMES: usize = PIPELINE_SAMPLE_RATE as usize;

/// Owns both WASAPI streams and their dedicated processing thread.
///
/// Dropping this value immediately tears down the route. This is intentional:
/// `NoiseHoiHoi` has no service or tray-resident process.
pub struct RunningAudioEngine {
    input_stream: Option<Stream>,
    output_stream: Option<Stream>,
    worker: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    metrics: MetricsHandle,
    signal_monitor_enabled: Arc<AtomicBool>,
    signal_monitor_consumer: rtrb::Consumer<crate::SignalMonitorSample>,
}

impl std::fmt::Debug for RunningAudioEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunningAudioEngine")
            .field("metrics", &self.metrics.snapshot())
            .finish_non_exhaustive()
    }
}

impl RunningAudioEngine {
    #[must_use]
    pub fn metrics(&self) -> MetricsHandle {
        self.metrics.clone()
    }

    /// Enable or disable collection for the optional signal-monitor window.
    pub fn set_signal_monitor_enabled(&mut self, enabled: bool) {
        self.signal_monitor_enabled.store(false, Ordering::Release);
        while self.signal_monitor_consumer.pop().is_ok() {}
        self.signal_monitor_enabled
            .store(enabled, Ordering::Release);
    }

    /// Drain all currently available aligned input/output samples.
    pub fn drain_signal_monitor_samples(
        &mut self,
        destination: &mut Vec<crate::SignalMonitorSample>,
    ) {
        // Bound GUI work even if the producer continues writing concurrently.
        let available = self.signal_monitor_consumer.slots();
        for _ in 0..available {
            if let Ok(sample) = self.signal_monitor_consumer.pop() {
                destination.push(sample);
            }
        }
    }

    pub fn stop(self) {
        drop(self);
    }

    fn shutdown(&mut self) {
        // Stop callbacks before joining the worker. The worker is explicitly
        // unparked so closing an idle application never waits for a timeout.
        self.input_stream.take();
        self.output_stream.take();
        self.signal_monitor_enabled.store(false, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
        self.metrics.0.set_state(EngineState::Stopped);
    }
}

impl Drop for RunningAudioEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start physical microphone -> processor -> VB-CABLE playback endpoint.
///
/// # Errors
///
/// Returns an error for an invalid selection, unsupported endpoint format, a
/// missing virtual endpoint, or a stream/worker startup failure.
pub fn start<P: AudioProcessor>(
    config: &EngineConfig,
    processor: P,
) -> Result<RunningAudioEngine, EngineError> {
    config.validate()?;

    let host = cpal::default_host();
    let input = devices::find_input_device(&host, &config.input_device_id)?;
    let output = devices::find_vb_cable_playback_device(&host)?;
    let input_format = devices::default_input_format(&input)?;
    let output_format = devices::vb_cable_output_format(&output)?;

    let input_rate = input_format.sample_rate();
    let input_channels = usize::from(input_format.channels());
    let output_channels = usize::from(output_format.channels());
    if input_rate == 0 || input_channels == 0 || output_channels == 0 {
        return Err(EngineError::UnsupportedFormat(
            "an endpoint reported zero channels or sample rate".to_owned(),
        ));
    }

    let target_output_frames = latency_frames(PIPELINE_SAMPLE_RATE, config.target_latency_ms)?;
    let output_ring_capacity = (target_output_frames * 4).max(MIN_OUTPUT_RING_FRAMES);
    let input_ring_capacity = scale_frames_to_rate(output_ring_capacity, input_rate)?;
    let (input_producer, input_consumer) = RingBuffer::<f32>::new(input_ring_capacity);
    let (mut output_producer, output_consumer) = RingBuffer::<f32>::new(output_ring_capacity);
    let (signal_monitor_producer, signal_monitor_consumer) =
        RingBuffer::<crate::SignalMonitorSample>::new(SIGNAL_MONITOR_RING_FRAMES);

    // Pre-roll silence separates the independently clocked endpoints from the
    // first callback and gives the worker time to begin producing audio.
    for _ in 0..target_output_frames {
        output_producer.push(0.0).map_err(|_| {
            EngineError::Start("failed to initialize the output pre-roll".to_owned())
        })?;
    }

    let shared_metrics = Arc::new(SharedMetrics::default());
    shared_metrics.set_state(EngineState::Starting);
    shared_metrics.set_buffered_output_frames(target_output_frames);
    let metrics = MetricsHandle(Arc::clone(&shared_metrics));
    let stop = Arc::new(AtomicBool::new(false));
    let signal_monitor_enabled = Arc::new(AtomicBool::new(false));

    let worker = spawn_worker(
        input_consumer,
        output_producer,
        processor,
        WorkerConfig {
            input_rate,
            output_capacity: output_ring_capacity,
            target_output_frames,
        },
        SignalMonitorWriter::new(signal_monitor_producer, Arc::clone(&signal_monitor_enabled)),
        Arc::clone(&stop),
        Arc::clone(&shared_metrics),
    )?;
    let worker_thread = worker.thread().clone();

    let output_stream = match build_output_stream(
        &output,
        &output_format,
        output_channels,
        output_consumer,
        Arc::clone(&shared_metrics),
    ) {
        Ok(stream) => stream,
        Err(error) => {
            stop_worker(&stop, worker);
            return Err(error);
        }
    };

    let input_stream = match build_input_stream(
        &input,
        &input_format,
        input_channels,
        input_producer,
        worker_thread,
        Arc::clone(&shared_metrics),
    ) {
        Ok(stream) => stream,
        Err(error) => {
            drop(output_stream);
            stop_worker(&stop, worker);
            return Err(error);
        }
    };

    if let Err(error) = output_stream.play() {
        drop(input_stream);
        drop(output_stream);
        stop_worker(&stop, worker);
        return Err(EngineError::Start(format!(
            "failed to start VB-CABLE output: {error}"
        )));
    }
    if let Err(error) = input_stream.play() {
        drop(input_stream);
        drop(output_stream);
        stop_worker(&stop, worker);
        return Err(EngineError::Start(format!(
            "failed to start microphone input: {error}"
        )));
    }

    shared_metrics.set_state(EngineState::Running);
    Ok(RunningAudioEngine {
        input_stream: Some(input_stream),
        output_stream: Some(output_stream),
        worker: Some(worker),
        stop,
        metrics,
        signal_monitor_enabled,
        signal_monitor_consumer,
    })
}
