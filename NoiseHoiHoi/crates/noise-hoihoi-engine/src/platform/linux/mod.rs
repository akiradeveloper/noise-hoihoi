mod capture;
mod pulse;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use libpulse_binding::{
    def::BufferAttr,
    sample::{Format, Spec},
    stream::{FlagSet, PeekResult, SeekMode, State, Stream},
};
use rtrb::{Consumer, Producer, RingBuffer};

use super::worker::{SignalMonitorWriter, WorkerConfig, spawn_worker};
use crate::{
    AudioDevice, AudioProcessor, EngineConfig, EngineError, EngineState, MetricsHandle,
    PIPELINE_SAMPLE_RATE, SignalMonitorSample, config::latency_frames, metrics::SharedMetrics,
};
use capture::CaptureFilter;
use pulse::{Connection, SINK_NAME, error};

/// Owns the audio-server thread, processing thread, and virtual microphone.
/// Dropping the engine disconnects streams and removes its virtual devices.
#[derive(Debug)]
pub struct RunningAudioEngine {
    io: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    metrics: MetricsHandle,
    monitor_enabled: Arc<AtomicBool>,
    monitor: Consumer<SignalMonitorSample>,
}

impl RunningAudioEngine {
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
        // Bound GUI work even if the producer continues writing concurrently.
        let available = self.monitor.slots();
        for _ in 0..available {
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
        for handle in [&mut self.io, &mut self.worker] {
            if let Some(handle) = handle.take() {
                handle.thread().unpark();
                let _ = handle.join();
            }
        }
        self.metrics.0.set_state(EngineState::Stopped);
    }
}

/// Enumerate capture sources, excluding sink monitors and our virtual microphone.
///
/// # Errors
/// Returns an error if the user's PulseAudio-compatible server is unavailable.
pub fn input_devices() -> Result<Vec<AudioDevice>, EngineError> {
    Connection::new()?.inputs()
}

/// Route a selected microphone through the processor into a virtual microphone.
///
/// # Errors
/// Returns an error for invalid configuration, missing devices, an unavailable
/// audio server, or virtual-device/stream/worker initialization failure.
pub fn start<P: AudioProcessor>(
    config: &EngineConfig,
    processor: P,
) -> Result<RunningAudioEngine, EngineError> {
    config.validate()?;
    let target = latency_frames(PIPELINE_SAMPLE_RATE, config.target_latency_ms)?;
    let capacity = (target * 4).max(8_192);
    let (input, input_consumer) = RingBuffer::new(capacity);
    let (mut output_producer, output) = RingBuffer::new(capacity);
    let (monitor_producer, monitor) = RingBuffer::new(PIPELINE_SAMPLE_RATE as usize);
    for _ in 0..target {
        output_producer.push(0.0).map_err(error)?;
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
            input_rate: PIPELINE_SAMPLE_RATE,
            output_capacity: capacity,
            target_output_frames: target,
        },
        SignalMonitorWriter::new(monitor_producer, Arc::clone(&monitor_enabled)),
        Arc::clone(&stop),
        Arc::clone(&shared),
    )?;
    let processing_thread = worker.thread().clone();
    let mut engine = RunningAudioEngine {
        io: None,
        worker: Some(worker),
        stop: Arc::clone(&stop),
        metrics: MetricsHandle(Arc::clone(&shared)),
        monitor_enabled,
        monitor,
    };
    let config = config.clone();
    let (ready, receiver) = mpsc::sync_channel(1);
    engine.io = Some(
        thread::Builder::new()
            .name("noise-hoihoi-pulse".to_owned())
            .spawn(move || {
                // The lock spans both startup and cleanup, including failures. It is a
                // kernel lock, automatically released even if the process is killed.
                let setup = session_lock().and_then(|lock| {
                    Route::new(&config, Arc::clone(&shared)).map(|route| (lock, route))
                });
                match setup {
                    Ok((_lock, mut route)) => {
                        shared.set_state(EngineState::Running);
                        if ready.send(Ok(())).is_err() {
                            return;
                        }
                        if let Err(err) =
                            route.run(input, output, &processing_thread, &stop, &shared)
                        {
                            shared.mark_stream_fault(err.to_string());
                        }
                        stop.store(true, Ordering::Release);
                        processing_thread.unpark();
                    }
                    Err(err) => {
                        let _ = ready.send(Err(err));
                    }
                }
            })
            .map_err(error)?,
    );
    receiver.recv().map_err(error)??;
    Ok(engine)
}

fn session_lock() -> Result<std::fs::File, EngineError> {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| error("XDG_RUNTIME_DIR is not set; launch from your desktop session"))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::path::PathBuf::from(directory).join("noise-hoihoi-audio.lock"))
        .map_err(error)?;
    lock.try_lock()
        .map_err(|_| error("another NoiseHoiHoi instance is using the virtual microphone"))?;
    Ok(lock)
}

struct Route {
    // Disconnect/drop streams before unloading their modules and context.
    input: Stream,
    output: Stream,
    connection: Connection,
}

impl Route {
    fn new(config: &EngineConfig, metrics: Arc<SharedMetrics>) -> Result<Self, EngineError> {
        let mut connection = Connection::new()?;
        if !connection
            .inputs()?
            .iter()
            .any(|d| d.id == config.input_device_id)
        {
            return Err(EngineError::InputDeviceNotFound(
                config.input_device_id.clone(),
            ));
        }
        connection.create_microphone()?;
        let spec = Spec {
            format: Format::FLOAT32NE,
            channels: 1,
            rate: PIPELINE_SAMPLE_RATE,
        };
        let mut input = Stream::new(&mut connection.context, "Microphone capture", &spec, None)
            .ok_or_else(|| error("cannot create capture stream"))?;
        let mut output = Stream::new(&mut connection.context, "Processed voice", &spec, None)
            .ok_or_else(|| error("cannot create playback stream"))?;
        let attr = BufferAttr {
            maxlength: 48_000 * 4 / 4,
            tlength: 48_000 * 4 / 50,
            prebuf: u32::MAX,
            minreq: 48_000 * 4 / 100,
            fragsize: 48_000 * 4 / 100,
        };
        let flags = FlagSet::ADJUST_LATENCY | FlagSet::DONT_MOVE;
        input
            .connect_record(Some(&config.input_device_id), Some(&attr), flags)
            .map_err(error)?;
        output
            .connect_playback(Some(SINK_NAME), Some(&attr), flags, None, None)
            .map_err(error)?;
        let input_metrics = Arc::clone(&metrics);
        input.set_overflow_callback(Some(Box::new(move || {
            input_metrics.add_input_discontinuity();
        })));
        output.set_underflow_callback(Some(Box::new(move || metrics.add_output_discontinuity())));
        let mut route = Self {
            input,
            output,
            connection,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while route.input.get_state() != State::Ready || route.output.get_state() != State::Ready {
            route.tick()?;
            if Instant::now() >= deadline {
                return Err(error("audio stream startup timed out"));
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(route)
    }

    fn tick(&mut self) -> Result<(), EngineError> {
        self.connection.tick()?;
        if [&self.input, &self.output]
            .iter()
            .any(|s| matches!(s.get_state(), State::Failed | State::Terminated))
        {
            return Err(error(
                "microphone or output disconnected; select a microphone and Retry",
            ));
        }
        Ok(())
    }

    fn run(
        &mut self,
        mut input: Producer<f32>,
        mut output: Consumer<f32>,
        worker: &thread::Thread,
        stop: &AtomicBool,
        metrics: &SharedMetrics,
    ) -> Result<(), EngineError> {
        let mut buffer = [0_u8; 1_920];
        let mut capture_filter = CaptureFilter::default();
        while !stop.load(Ordering::Acquire) {
            self.tick()?;
            // Limit each iteration so shutdown remains responsive under load.
            for _ in 0..16 {
                let mut peak = 0.0_f32;
                let mut dropped = 0;
                match self.input.peek().map_err(error)? {
                    PeekResult::Empty => break,
                    PeekResult::Hole(bytes) => {
                        metrics.add_input_discontinuity();
                        capture_filter.reset();
                        for _ in 0..bytes / 4 {
                            if input.push(0.0).is_err() {
                                dropped += 1;
                            }
                        }
                    }
                    PeekResult::Data(data) => {
                        for bytes in data.chunks_exact(4) {
                            let sample = f32::from_ne_bytes(bytes.try_into().map_err(error)?);
                            let sample = capture_filter.process(sample);
                            peak = peak.max(sample.abs());
                            if input.push(sample).is_err() {
                                dropped += 1;
                            }
                        }
                    }
                }
                self.input.discard().map_err(error)?;
                metrics.set_input_peak(peak);
                metrics.add_dropped_input_frames(dropped);
                worker.unpark();
            }
            let writable = self
                .output
                .writable_size()
                .ok_or_else(|| error("cannot query playback buffer"))?;
            let bytes = writable.min(buffer.len()) / 4 * 4;
            let mut silence = 0;
            for destination in buffer[..bytes].chunks_exact_mut(4) {
                let sample = output.pop().unwrap_or_else(|_| {
                    silence += 1;
                    0.0
                });
                destination.copy_from_slice(&sample.to_ne_bytes());
            }
            if bytes > 0 {
                self.output
                    .write_copy(&buffer[..bytes], 0, SeekMode::Relative)
                    .map_err(error)?;
                metrics.add_inserted_silence_frames(silence);
                worker.unpark();
            }
            thread::park_timeout(Duration::from_millis(1));
        }
        Ok(())
    }
}

impl Drop for Route {
    fn drop(&mut self) {
        let _ = self.input.disconnect();
        let _ = self.output.disconnect();
    }
}
