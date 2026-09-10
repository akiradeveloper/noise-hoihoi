use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering},
};

/// Lifecycle state visible to the GUI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum EngineState {
    Starting = 0,
    Running = 1,
    Faulted = 2,
    #[default]
    Stopped = 3,
}

impl EngineState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Starting,
            1 => Self::Running,
            2 => Self::Faulted,
            _ => Self::Stopped,
        }
    }
}

/// Point-in-time, allocation-free counters collected by the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineMetrics {
    pub state: EngineState,
    pub input_peak: f32,
    /// Input frames dropped because the processing ring was full.
    pub dropped_input_frames: u64,
    /// Silence frames inserted because processed output was unavailable.
    pub inserted_silence_frames: u64,
    /// Recoverable discontinuities reported by the physical input stream.
    pub input_discontinuities: u64,
    /// Recoverable discontinuities reported by the virtual microphone output stream.
    pub output_discontinuities: u64,
    pub stream_faults: u64,
    pub processed_frames: u64,
    /// Longest individual processor call since this engine started.
    pub max_processing_time_us: u64,
    /// Processor calls that exceeded the duration of their audio block.
    pub processing_deadline_misses: u64,
    pub buffered_output_frames: u32,
    /// Signal-monitor frames dropped without affecting the audio route.
    pub dropped_signal_monitor_frames: u64,
}

#[derive(Debug)]
pub struct SharedMetrics {
    state: AtomicU8,
    input_peak_bits: AtomicU32,
    dropped_input_frames: AtomicU64,
    inserted_silence_frames: AtomicU64,
    input_discontinuities: AtomicU64,
    output_discontinuities: AtomicU64,
    stream_faults: AtomicU64,
    processed_frames: AtomicU64,
    max_processing_time_us: AtomicU64,
    processing_deadline_misses: AtomicU64,
    buffered_output_frames: AtomicU32,
    dropped_signal_monitor_frames: AtomicU64,
    fault_message: Mutex<Option<String>>,
}

impl Default for SharedMetrics {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(EngineState::Stopped as u8),
            input_peak_bits: AtomicU32::new(0.0_f32.to_bits()),
            dropped_input_frames: AtomicU64::new(0),
            inserted_silence_frames: AtomicU64::new(0),
            input_discontinuities: AtomicU64::new(0),
            output_discontinuities: AtomicU64::new(0),
            stream_faults: AtomicU64::new(0),
            processed_frames: AtomicU64::new(0),
            max_processing_time_us: AtomicU64::new(0),
            processing_deadline_misses: AtomicU64::new(0),
            buffered_output_frames: AtomicU32::new(0),
            dropped_signal_monitor_frames: AtomicU64::new(0),
            fault_message: Mutex::new(None),
        }
    }
}

impl SharedMetrics {
    pub fn set_state(&self, state: EngineState) {
        self.state.store(state as u8, Ordering::Release);
    }

    pub fn set_input_peak(&self, peak: f32) {
        self.input_peak_bits
            .store(peak.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn add_dropped_input_frames(&self, count: u64) {
        self.dropped_input_frames
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_inserted_silence_frames(&self, count: u64) {
        self.inserted_silence_frames
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_input_discontinuity(&self) {
        self.input_discontinuities.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_output_discontinuity(&self) {
        self.output_discontinuities.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_stream_fault(&self, message: impl Into<String>) {
        let mut fault_message = self
            .fault_message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if fault_message.is_none() {
            *fault_message = Some(message.into());
        }
        self.stream_faults.fetch_add(1, Ordering::Relaxed);
        self.set_state(EngineState::Faulted);
    }

    pub fn add_processed_frames(&self, count: u64) {
        self.processed_frames.fetch_add(count, Ordering::Relaxed);
    }

    pub fn observe_processing(&self, elapsed: std::time::Duration, deadline_misses: u64) {
        let microseconds = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.max_processing_time_us
            .fetch_max(microseconds, Ordering::Relaxed);
        self.processing_deadline_misses
            .fetch_add(deadline_misses, Ordering::Relaxed);
    }

    pub fn set_buffered_output_frames(&self, count: usize) {
        self.buffered_output_frames
            .store(u32::try_from(count).unwrap_or(u32::MAX), Ordering::Relaxed);
    }

    pub fn add_dropped_signal_monitor_frames(&self, count: u64) {
        self.dropped_signal_monitor_frames
            .fetch_add(count, Ordering::Relaxed);
    }

    fn snapshot(&self) -> EngineMetrics {
        EngineMetrics {
            state: EngineState::from_u8(self.state.load(Ordering::Acquire)),
            input_peak: f32::from_bits(self.input_peak_bits.load(Ordering::Relaxed)),
            dropped_input_frames: self.dropped_input_frames.load(Ordering::Relaxed),
            inserted_silence_frames: self.inserted_silence_frames.load(Ordering::Relaxed),
            input_discontinuities: self.input_discontinuities.load(Ordering::Relaxed),
            output_discontinuities: self.output_discontinuities.load(Ordering::Relaxed),
            stream_faults: self.stream_faults.load(Ordering::Relaxed),
            processed_frames: self.processed_frames.load(Ordering::Relaxed),
            max_processing_time_us: self.max_processing_time_us.load(Ordering::Relaxed),
            processing_deadline_misses: self.processing_deadline_misses.load(Ordering::Relaxed),
            buffered_output_frames: self.buffered_output_frames.load(Ordering::Relaxed),
            dropped_signal_monitor_frames: self
                .dropped_signal_monitor_frames
                .load(Ordering::Relaxed),
        }
    }
}

/// Cheap cloneable view used by the UI without locking the audio threads.
#[derive(Clone, Debug)]
pub struct MetricsHandle(pub(crate) Arc<SharedMetrics>);

impl MetricsHandle {
    #[must_use]
    pub fn snapshot(&self) -> EngineMetrics {
        self.0.snapshot()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.0
            .fault_message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{EngineState, MetricsHandle, SharedMetrics};

    #[test]
    fn snapshot_reflects_atomic_updates() {
        let shared = Arc::new(SharedMetrics::default());
        shared.set_state(EngineState::Running);
        shared.set_input_peak(0.75);
        shared.add_dropped_input_frames(2);
        shared.add_inserted_silence_frames(3);
        shared.add_input_discontinuity();
        shared.add_output_discontinuity();
        shared.add_processed_frames(480);
        shared.observe_processing(std::time::Duration::from_micros(750), 2);
        shared.observe_processing(std::time::Duration::from_micros(500), 0);
        shared.set_buffered_output_frames(1_920);
        shared.add_dropped_signal_monitor_frames(4);
        shared.mark_stream_fault("output stream: device invalidated");

        let handle = MetricsHandle(shared);
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.state, EngineState::Faulted);
        assert_eq!(snapshot.input_peak.to_bits(), 0.75_f32.to_bits());
        assert_eq!(snapshot.dropped_input_frames, 2);
        assert_eq!(snapshot.inserted_silence_frames, 3);
        assert_eq!(snapshot.input_discontinuities, 1);
        assert_eq!(snapshot.output_discontinuities, 1);
        assert_eq!(snapshot.processed_frames, 480);
        assert_eq!(snapshot.max_processing_time_us, 750);
        assert_eq!(snapshot.processing_deadline_misses, 2);
        assert_eq!(snapshot.buffered_output_frames, 1_920);
        assert_eq!(snapshot.dropped_signal_monitor_frames, 4);
        assert_eq!(
            handle.last_error().as_deref(),
            Some("output stream: device invalidated")
        );
    }

    #[test]
    fn preserves_the_first_fault_message() {
        let shared = Arc::new(SharedMetrics::default());
        shared.mark_stream_fault("root cause");
        shared.mark_stream_fault("follow-on failure");

        let handle = MetricsHandle(shared);
        assert_eq!(handle.snapshot().stream_faults, 2);
        assert_eq!(handle.last_error().as_deref(), Some("root cause"));
    }
}
