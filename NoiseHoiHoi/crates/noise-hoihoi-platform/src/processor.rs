use noise_hoihoi_engine::AudioProcessor;
/// `DeepFilterNet3` running on the selected compute processor.
pub struct NoiseReduction {
    inner: noise_net::NoiseNet,
}

impl NoiseReduction {
    /// Load `DeepFilterNet3` on the selected compute processor and runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested pair or model cannot initialize.
    pub fn new(
        processor: &crate::ComputeProcessor,
        runtime: crate::ComputeRuntime,
    ) -> Result<Self, crate::EngineError> {
        let device = noise_net_runtime::create_device(processor, runtime)
            .map_err(|error| crate::EngineError::NoiseReduction(error.to_string()))?;
        noise_net::NoiseNet::from_device(device, runtime == crate::ComputeRuntime::Wgpu)
            .map(|inner| Self { inner })
            .map_err(|error| crate::EngineError::NoiseReduction(error.to_string()))
    }
}

impl AudioProcessor for NoiseReduction {
    fn process(&mut self, mono_48khz: &mut [f32]) {
        let input: [f32; noise_net::FRAME_SIZE] = (&*mono_48khz)
            .try_into()
            .expect("the audio worker must honor NoiseNet's frame size");
        let mut output = [0.0; noise_net::FRAME_SIZE];
        self.inner.process_frame(&input, &mut output);
        mono_48khz.copy_from_slice(&output);
    }

    fn frame_size(&self) -> Option<usize> {
        Some(noise_net::FRAME_SIZE)
    }

    fn latency_samples(&self) -> usize {
        noise_net::ALGORITHM_LATENCY_SAMPLES
    }
}
