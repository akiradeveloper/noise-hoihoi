#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{input_devices, start};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{input_devices, start};
#[cfg(not(any(windows, target_os = "linux")))]
mod unsupported {
    use crate::{AudioDevice, AudioProcessor, EngineConfig, EngineError, RunningAudioEngine};
    pub fn input_devices() -> Result<Vec<AudioDevice>, EngineError> {
        Err(EngineError::UnsupportedPlatform)
    }
    pub fn start<P: AudioProcessor>(
        _: &EngineConfig,
        _: P,
    ) -> Result<RunningAudioEngine, EngineError> {
        Err(EngineError::UnsupportedPlatform)
    }
}
#[cfg(not(any(windows, target_os = "linux")))]
pub use unsupported::{input_devices, start};
