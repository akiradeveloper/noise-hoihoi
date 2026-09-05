#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{RunningAudioEngine, input_devices, start};

#[cfg(not(windows))]
mod unsupported {
    use crate::{
        AudioDevice, AudioProcessor, EngineConfig, EngineError, MetricsHandle, SignalMonitorSample,
    };

    /// Placeholder type that keeps platform-independent workspace checks useful.
    #[derive(Debug)]
    pub struct RunningAudioEngine;

    impl RunningAudioEngine {
        #[must_use]
        pub fn metrics(&self) -> MetricsHandle {
            unreachable!("a running engine cannot exist off Windows")
        }

        pub fn set_signal_monitor_enabled(&mut self, _enabled: bool) {}

        pub fn drain_signal_monitor_samples(
            &mut self,
            _destination: &mut Vec<SignalMonitorSample>,
        ) {
        }

        pub fn stop(self) {}
    }

    /// # Errors
    ///
    /// Always returns [`EngineError::UnsupportedPlatform`] outside Windows.
    pub fn input_devices() -> Result<Vec<AudioDevice>, EngineError> {
        Err(EngineError::UnsupportedPlatform)
    }

    /// # Errors
    ///
    /// Always returns [`EngineError::UnsupportedPlatform`] outside Windows.
    pub fn start<P: AudioProcessor>(
        _config: &EngineConfig,
        _processor: P,
    ) -> Result<RunningAudioEngine, EngineError> {
        Err(EngineError::UnsupportedPlatform)
    }
}

#[cfg(not(windows))]
pub use unsupported::{RunningAudioEngine, input_devices, start};
