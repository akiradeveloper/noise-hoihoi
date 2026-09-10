use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{JoinHandle, Thread},
};

use crate::{
    AudioProcessor, EngineConfig, EngineError, EngineState, MetricsHandle, PIPELINE_SAMPLE_RATE,
    SharedMetrics, SignalMonitorSample,
    config::{latency_frames, scale_frames_to_rate},
    worker::{SignalMonitorWriter, WorkerConfig, spawn_worker},
};
use rtrb::{Consumer, Producer, RingBuffer};

/// Adapter boundary: mono f32 input at the negotiated rate, mono f32 output at 48 kHz.
/// The rings are process-local; adapters own their native stream handles separately.
pub struct AudioPorts {
    pub input: Producer<f32>,
    pub output: Consumer<f32>,
    pub metrics: Arc<SharedMetrics>,
    pub stop: Arc<AtomicBool>,
    pub worker: Thread,
}

/// Common owner of the processing worker, monitor, and native route lifetime.
/// Route destruction happens after cancellation and before joining the worker.
pub struct RunningAudioEngine {
    route: Option<Box<dyn Send>>,
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    metrics: MetricsHandle,
    monitor_enabled: Arc<AtomicBool>,
    monitor: Consumer<SignalMonitorSample>,
}

impl std::fmt::Debug for RunningAudioEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningAudioEngine")
            .field("metrics", &self.metrics)
            .finish_non_exhaustive()
    }
}

impl RunningAudioEngine {
    /// Allocate queues and start processing. The adapter must retain its I/O in `with_route`.
    ///
    /// # Errors
    /// Returns an error for invalid configuration or worker initialization failure.
    pub fn prepare<P: AudioProcessor>(
        config: &EngineConfig,
        input_rate: u32,
        processor: P,
    ) -> Result<(Self, AudioPorts), EngineError> {
        config.validate()?;
        if input_rate == 0 {
            return Err(EngineError::UnsupportedFormat(
                "input sample rate is zero".into(),
            ));
        }
        let target = latency_frames(PIPELINE_SAMPLE_RATE, config.target_latency_ms)?;
        let capacity = (target * 4).max(8_192);
        let (input, input_consumer) = RingBuffer::new(scale_frames_to_rate(capacity, input_rate)?);
        let (mut output_producer, output) = RingBuffer::new(capacity);
        let (monitor_producer, monitor) = RingBuffer::new(PIPELINE_SAMPLE_RATE as usize);
        for _ in 0..target {
            output_producer
                .push(0.0)
                .map_err(|_| EngineError::Start("output pre-roll overflow".into()))?;
        }
        let shared = Arc::new(SharedMetrics::default());
        shared.set_state(EngineState::Starting);
        shared.set_buffered_output_frames(target);
        let stop = Arc::new(AtomicBool::new(false));
        let monitor_enabled = Arc::new(AtomicBool::new(false));
        let worker = spawn_worker(
            input_consumer,
            output_producer,
            processor,
            WorkerConfig {
                input_rate,
                output_capacity: capacity,
                target_output_frames: target,
            },
            SignalMonitorWriter::new(monitor_producer, Arc::clone(&monitor_enabled)),
            Arc::clone(&stop),
            Arc::clone(&shared),
        )?;
        let ports = AudioPorts {
            input,
            output,
            metrics: Arc::clone(&shared),
            stop: Arc::clone(&stop),
            worker: worker.thread().clone(),
        };
        Ok((
            Self {
                route: None,
                worker: Some(worker),
                stop,
                metrics: MetricsHandle(shared),
                monitor_enabled,
                monitor,
            },
            ports,
        ))
    }

    /// Attach an adapter-owned RAII route. Its destructor must close/join native I/O.
    ///
    /// # Panics
    /// Panics if a route was already attached.
    #[must_use]
    pub fn with_route(mut self, route: impl Send + 'static) -> Self {
        assert!(self.route.is_none(), "a route is already attached");
        self.route = Some(Box::new(route));
        self
    }

    #[must_use]
    pub fn metrics(&self) -> MetricsHandle {
        self.metrics.clone()
    }

    pub fn set_signal_monitor_enabled(&mut self, enabled: bool) {
        self.monitor_enabled.store(false, Ordering::Release);
        while self.monitor.pop().is_ok() {}
        self.monitor_enabled.store(enabled, Ordering::Release);
    }

    pub fn drain_signal_monitor_samples(&mut self, destination: &mut Vec<SignalMonitorSample>) {
        for _ in 0..self.monitor.slots() {
            if let Ok(sample) = self.monitor.pop() {
                destination.push(sample);
            }
        }
    }

    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for RunningAudioEngine {
    fn drop(&mut self) {
        self.monitor_enabled.store(false, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        self.route.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.metrics.0.set_state(EngineState::Stopped);
    }
}
