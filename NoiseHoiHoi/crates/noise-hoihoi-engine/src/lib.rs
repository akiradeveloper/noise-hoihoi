//! Portable real-time audio processing for `NoiseHoiHoi`.

mod capture;
mod config;
mod metrics;
mod processor;
mod running;
mod signal_monitor;
mod worker;

pub use capture::CaptureFilter;
pub use config::{AudioDevice, EngineConfig, PIPELINE_SAMPLE_RATE};
pub use metrics::SharedMetrics;
pub use metrics::{EngineMetrics, EngineState, MetricsHandle};
pub use processor::{AudioProcessor, PassThrough};
pub use running::{AudioPorts, RunningAudioEngine};
pub use signal_monitor::SignalMonitorSample;

use thiserror::Error;

/// Errors surfaced by the audio engine to the UI.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("no native audio backend is available for this platform")]
    UnsupportedPlatform,

    #[error("failed to enumerate audio devices: {0}")]
    DeviceEnumeration(String),

    #[error("input device was not found: {0}")]
    InputDeviceNotFound(String),

    #[error("virtual output is unavailable: {0}")]
    VirtualOutputUnavailable(String),

    #[error("audio device has an unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("failed to start audio: {0}")]
    Start(String),

    #[error("failed to initialize noise reduction: {0}")]
    NoiseReduction(String),

    #[error("invalid engine configuration: {0}")]
    InvalidConfiguration(String),
}
