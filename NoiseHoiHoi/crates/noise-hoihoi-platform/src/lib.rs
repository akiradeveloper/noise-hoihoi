//! Windows/Linux adapters. No macOS implementation is provided.
mod audio;
mod compute;
mod processor;
mod settings;
mod vb_cable;

pub use audio::{input_devices, start};
pub use compute::{ComputeProcessor, ComputeRuntime, ProcessorKind};
pub use noise_hoihoi_engine::*;
pub use processor::NoiseReduction;
pub use vb_cable::{
    VB_CABLE_PLAYBACK_ENDPOINT_NAME, VB_CABLE_RECORDING_ENDPOINT_NAME,
    is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint,
};

#[must_use]
pub fn compute_processors() -> Vec<ComputeProcessor> {
    compute::processors()
}

#[cfg(target_os = "linux")]
pub const OUTPUT_MICROPHONE_NAME: &str = "NoiseHoiHoi Microphone";
#[cfg(not(target_os = "linux"))]
pub const OUTPUT_MICROPHONE_NAME: &str = VB_CABLE_RECORDING_ENDPOINT_NAME;

pub struct NativeBackend;
impl noise_hoihoi_session::SessionBackend for NativeBackend {
    fn input_devices(&self) -> Result<Vec<AudioDevice>, EngineError> {
        input_devices()
    }
    fn processors(&self) -> Vec<noise_hoihoi_session::ComputeProcessor> {
        compute_processors()
            .into_iter()
            .map(|p| noise_hoihoi_session::ComputeProcessor {
                id: p.id().to_owned(),
                name: p.name().to_owned(),
                is_gpu: p.is_gpu(),
                runtime: p.default_runtime().to_string(),
            })
            .collect()
    }
    fn output_name(&self) -> &str {
        OUTPUT_MICROPHONE_NAME
    }
    fn load_settings(&self) -> (noise_hoihoi_session::Settings, Option<String>) {
        settings::SettingsFile::load()
    }
    fn save_settings(&self, settings: &noise_hoihoi_session::Settings) -> Result<(), String> {
        settings::SettingsFile::save(settings).map_err(|error| error.to_string())
    }
    fn start(
        &self,
        config: &EngineConfig,
        processor: Option<&str>,
    ) -> Result<RunningAudioEngine, EngineError> {
        if let Some(id) = processor {
            let processor = compute_processors()
                .into_iter()
                .find(|p| p.id() == id)
                .ok_or_else(|| {
                    EngineError::NoiseReduction("selected processor is no longer available".into())
                })?;
            let processor = NoiseReduction::new(&processor, processor.default_runtime())?;
            start(config, processor)
        } else {
            start(config, PassThrough)
        }
    }
}
