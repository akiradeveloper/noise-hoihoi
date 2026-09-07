//! Real-time microphone capture and virtual microphone output for `NoiseHoiHoi`.

mod config;
mod metrics;
mod platform;
mod processor;
mod signal_monitor;
mod vb_cable;

pub use config::{AudioDevice, EngineConfig, PIPELINE_SAMPLE_RATE};
pub use metrics::{EngineMetrics, EngineState, MetricsHandle};
pub use noise_net::{ComputeProcessor, ComputeRuntime, ProcessorKind};
pub use platform::{RunningAudioEngine, input_devices, start};
pub use processor::{AudioProcessor, NoiseReduction, PassThrough};
pub use signal_monitor::SignalMonitorSample;
pub use vb_cable::{
    VB_CABLE_PLAYBACK_ENDPOINT_NAME, VB_CABLE_RECORDING_ENDPOINT_NAME,
    is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint,
};

use thiserror::Error;

/// Enumerate compute processors supported by the current `NoiseNet` runtimes.
#[must_use]
pub fn compute_processors() -> Vec<ComputeProcessor> {
    noise_net::processors()
}

/// Errors surfaced by the audio engine to the UI.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("NoiseHoiHoi currently supports Windows 11 x64 and Linux x86_64")]
    UnsupportedPlatform,

    #[error("failed to enumerate audio devices: {0}")]
    DeviceEnumeration(String),

    #[error("input device was not found: {0}")]
    InputDeviceNotFound(String),

    #[error(
        "VB-CABLE playback endpoint '{VB_CABLE_PLAYBACK_ENDPOINT_NAME}' is not available; install or enable VB-CABLE and restart Windows"
    )]
    VbCablePlaybackNotFound,

    #[error("audio device has an unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("failed to start audio: {0}")]
    Start(String),

    #[error("failed to initialize noise reduction: {0}")]
    NoiseReduction(String),

    #[error("invalid engine configuration: {0}")]
    InvalidConfiguration(String),
}

/// Microphone name to select in streaming and recording applications.
#[cfg(target_os = "linux")]
pub const OUTPUT_MICROPHONE_NAME: &str = "NoiseHoiHoi Microphone";
#[cfg(not(target_os = "linux"))]
pub const OUTPUT_MICROPHONE_NAME: &str = VB_CABLE_RECORDING_ENDPOINT_NAME;
