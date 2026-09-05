//! Real-time microphone capture and VB-CABLE output for `NoiseHoiHoi`.

mod config;
mod metrics;
mod platform;
mod processor;
mod signal_monitor;
mod vb_cable;

pub use config::{AudioDevice, EngineConfig, PIPELINE_SAMPLE_RATE};
pub use metrics::{EngineMetrics, EngineState, MetricsHandle};
pub use platform::{RunningAudioEngine, input_devices, start};
pub use processor::{AudioProcessor, PassThrough};
pub use signal_monitor::SignalMonitorSample;
pub use vb_cable::{
    VB_CABLE_PLAYBACK_ENDPOINT_NAME, VB_CABLE_RECORDING_ENDPOINT_NAME,
    is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint,
};

use thiserror::Error;

/// Errors surfaced by the audio engine to the UI.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("NoiseHoiHoi currently supports Windows 11 x64")]
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

    #[error("invalid engine configuration: {0}")]
    InvalidConfiguration(String),
}
