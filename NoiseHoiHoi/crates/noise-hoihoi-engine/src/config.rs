use crate::EngineError;

/// Canonical format passed to the processor in every `NoiseHoiHoi` release.
pub const PIPELINE_SAMPLE_RATE: u32 = 48_000;

/// A physical microphone offered by the current audio host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// User-selected settings needed to start the real-time engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub input_device_id: String,
    /// Software buffering target. This excludes endpoint and driver latency.
    pub target_latency_ms: u32,
}

impl EngineConfig {
    pub fn new(input_device_id: impl Into<String>) -> Self {
        Self {
            input_device_id: input_device_id.into(),
            target_latency_ms: 40,
        }
    }

    /// Check the device selection and buffering bounds.
    ///
    /// # Errors
    /// Returns an error for an empty input identifier or latency outside 10–250 ms.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.input_device_id.trim().is_empty() {
            return Err(EngineError::InvalidConfiguration(
                "an input device must be selected".to_owned(),
            ));
        }
        if !(10..=250).contains(&self.target_latency_ms) {
            return Err(EngineError::InvalidConfiguration(
                "target latency must be between 10 and 250 ms".to_owned(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn latency_frames(sample_rate: u32, latency_ms: u32) -> Result<usize, EngineError> {
    usize::try_from(u64::from(sample_rate) * u64::from(latency_ms) / 1_000).map_err(|_| {
        EngineError::InvalidConfiguration("latency does not fit this target".to_owned())
    })
}

pub(crate) fn scale_frames_to_rate(
    output_frames: usize,
    input_rate: u32,
) -> Result<usize, EngineError> {
    let output_frames = u64::try_from(output_frames)
        .map_err(|_| EngineError::InvalidConfiguration("audio buffer is too large".to_owned()))?;
    let numerator = output_frames
        .checked_mul(u64::from(input_rate))
        .ok_or_else(|| EngineError::InvalidConfiguration("audio buffer is too large".to_owned()))?;
    let scaled = numerator.div_ceil(u64::from(PIPELINE_SAMPLE_RATE));
    usize::try_from(scaled)
        .map_err(|_| EngineError::InvalidConfiguration("audio buffer is too large".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{EngineConfig, latency_frames, scale_frames_to_rate};

    #[test]
    fn default_configuration_is_valid() {
        assert!(EngineConfig::new("device-id").validate().is_ok());
    }

    #[test]
    fn rejects_empty_device_id() {
        assert!(EngineConfig::new("  ").validate().is_err());
    }

    #[test]
    fn rejects_excessive_latency() {
        let mut config = EngineConfig::new("device-id");
        config.target_latency_ms = 251;
        assert!(config.validate().is_err());
    }

    #[test]
    fn converts_latency_to_frames() {
        assert_eq!(latency_frames(48_000, 40).unwrap(), 1_920);
    }

    #[test]
    fn scales_ring_capacity_to_the_input_sample_rate() {
        assert_eq!(scale_frames_to_rate(8_192, 48_000).unwrap(), 8_192);
        assert_eq!(scale_frames_to_rate(8_192, 96_000).unwrap(), 16_384);
        assert_eq!(scale_frames_to_rate(8_192, 44_100).unwrap(), 7_527);
    }
}
