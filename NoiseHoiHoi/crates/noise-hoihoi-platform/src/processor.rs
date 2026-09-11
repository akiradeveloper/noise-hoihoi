use noise_hoihoi_engine::AudioProcessor;
use noise_net_iree::{DpdfNet, FRAME_SIZE};

/// The reference model and STFT, with a native backend for the selected processor.
pub struct NoiseReduction {
    inner: DpdfNet,
}
impl NoiseReduction {
    /// Load and warm up the selected device before opening audio.
    /// # Errors
    /// Returns a selection, library, model or provider initialization error.
    pub fn new(
        processor: &crate::ComputeProcessor,
        runtime: crate::ComputeRuntime,
    ) -> Result<Self, crate::EngineError> {
        if runtime != processor.default_runtime() {
            return Err(crate::EngineError::NoiseReduction(
                "Unsupported processor/runtime combination".into(),
            ));
        }
        let inner = DpdfNet::new(processor.id());
        inner
            .map(|inner| Self { inner })
            .map_err(|e| crate::EngineError::NoiseReduction(format!("{e:#}")))
    }
}
impl AudioProcessor for NoiseReduction {
    fn process(&mut self, samples: &mut [f32]) -> Result<(), String> {
        let input: [f32; FRAME_SIZE] = (&*samples)
            .try_into()
            .map_err(|_| "Invalid audio frame size")?;
        let mut output = [0.0; FRAME_SIZE];
        self.inner
            .process_frame(&input, &mut output)
            .map_err(|e| format!("Inference failed: {e:#}"))?;
        samples.copy_from_slice(&output);
        Ok(())
    }
    fn frame_size(&self) -> Option<usize> {
        Some(FRAME_SIZE)
    }
    fn latency_samples(&self) -> usize {
        noise_net_iree::LATENCY_SAMPLES
    }
}
