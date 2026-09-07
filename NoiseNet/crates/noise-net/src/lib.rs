//! Streaming `DeepFilterNet3` inference implemented with Burn.

mod dsp;
mod generated;
mod level;
mod network;
mod recovery;
mod voicing;

use std::panic::{AssertUnwindSafe, catch_unwind};

use dsp::DspState;
use level::InputLevel;
use network::{InferenceMode, StreamingNetwork};
use thiserror::Error;
use voicing::VoicedGuard;

pub use noise_net_runtime::{ComputeProcessor, ComputeRuntime, ProcessorKind, processors};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SIZE: usize = 480;
pub const FFT_SIZE: usize = 960;
pub const FFT_BINS: usize = FFT_SIZE / 2 + 1;
pub const ERB_BANDS: usize = 32;
pub const DF_BINS: usize = 96;
pub const DF_ORDER: usize = 5;
pub const DF_LOOKAHEAD: usize = 2;
pub const ALGORITHM_LATENCY_SAMPLES: usize = (FFT_SIZE - FRAME_SIZE) + DF_LOOKAHEAD * FRAME_SIZE;
/// Local SNR below which the upstream runtime considers a frame noise-only.
pub(crate) const NOISE_ONLY_THRESHOLD_DB: f32 = -10.0;

#[derive(Debug, Error)]
pub enum NoiseNetError {
    #[error("Burn could not initialize or load DeepFilterNet3: {0}")]
    Initialization(String),
}

/// A mono, 48 kHz, stateful `DeepFilterNet3` inference stream.
pub struct NoiseNet {
    network: StreamingNetwork,
    dsp: DspState,
    voiced_guard: VoicedGuard,
    input_level: InputLevel,
    last_local_snr_db: f32,
    last_voice_protected: bool,
}

impl NoiseNet {
    /// Load the official `DeepFilterNet3` weights on Burn Flex's CPU device.
    ///
    /// # Errors
    ///
    /// Returns [`NoiseNetError::Initialization`] if the CPU runtime or embedded
    /// model cannot be initialized.
    pub fn new_cpu() -> Result<Self, NoiseNetError> {
        let processor = noise_net_runtime::cpu();
        Self::new(&processor, ComputeRuntime::Flex)
    }

    /// Load the official `DeepFilterNet3` weights on the selected processor.
    ///
    /// # Errors
    ///
    /// Returns [`NoiseNetError::Initialization`] if the runtime, processor, or
    /// embedded model cannot be initialized.
    pub fn new(
        processor: &ComputeProcessor,
        runtime: ComputeRuntime,
    ) -> Result<Self, NoiseNetError> {
        match catch_unwind(AssertUnwindSafe(|| {
            let device = noise_net_runtime::create_device(processor, runtime)
                .map_err(|error| error.to_string())?;
            let mut network = Self {
                network: StreamingNetwork::new(device),
                dsp: DspState::new(),
                voiced_guard: VoicedGuard::new(),
                input_level: InputLevel::default(),
                last_local_snr_db: -15.0,
                last_voice_protected: false,
            };
            if runtime == ComputeRuntime::Wgpu {
                network.warm_up();
                network.reset();
            }
            Ok::<_, String>(network)
        })) {
            Ok(Ok(network)) => Ok(network),
            Ok(Err(error)) => Err(NoiseNetError::Initialization(error)),
            Err(_) => Err(NoiseNetError::Initialization(format!(
                "{} with {runtime}",
                processor.name()
            ))),
        }
    }

    /// Process one 10 ms frame with continuous inference, adaptive feature
    /// levels and coherent speech protection. Synthesis preserves input scale.
    pub fn process_frame(&mut self, input: &[f32; FRAME_SIZE], output: &mut [f32; FRAME_SIZE]) {
        let feature_gain = self.input_level.update(input);
        let protect_voice = self.voiced_guard.update(&input.map(|v| v * feature_gain));
        self.process_frame_inner(
            input,
            output,
            protect_voice,
            feature_gain,
            InferenceMode::Continuous,
        );
    }

    /// Process one 10 ms frame with the upstream reference pipeline, including
    /// its processing thresholds and without feature-level adaptation or protection.
    ///
    /// This entry point exists for exact model-reference diagnostics. Product
    /// audio paths should use [`Self::process_frame`]. Reset the stream before
    /// switching between protected and unprotected processing.
    pub fn process_frame_unprotected(
        &mut self,
        input: &[f32; FRAME_SIZE],
        output: &mut [f32; FRAME_SIZE],
    ) {
        self.process_frame_inner(input, output, false, 1.0, InferenceMode::Reference);
    }

    fn process_frame_inner(
        &mut self,
        input: &[f32; FRAME_SIZE],
        output: &mut [f32; FRAME_SIZE],
        protect_voice: bool,
        feature_gain: f32,
        mode: InferenceMode,
    ) {
        let features = self.dsp.analyze(input, feature_gain);
        let inference = self.network.infer(features.erb, &features.complex, mode);
        self.last_voice_protected = self.dsp.synthesize(
            inference.mask.as_deref(),
            inference.coefficients.as_deref(),
            protect_voice,
            output,
        );
        self.last_local_snr_db = inference.local_snr_db;
    }

    /// Reset all recurrent, normalization, FFT, and history state.
    pub fn reset(&mut self) {
        self.network.reset();
        self.dsp = DspState::new();
        self.voiced_guard = VoicedGuard::new();
        self.input_level = InputLevel::default();
        self.last_local_snr_db = -15.0;
        self.last_voice_protected = false;
    }

    fn warm_up(&mut self) {
        for frame_index in 0..24 {
            let amplitude = match frame_index {
                0..=3 => 0.0,
                4..=11 => 0.08,
                _ => 0.35,
            };
            let input = std::array::from_fn(|sample| {
                let index = frame_index * FRAME_SIZE + sample;
                let bits = u16::try_from(
                    (index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16) & 0xffff,
                )
                .expect("the pseudo-random sample is limited to 16 bits");
                amplitude * (f32::from(bits) / f32::from(u16::MAX) * 2.0 - 1.0)
            });
            let mut output = [0.0; FRAME_SIZE];
            self.process_frame(&input, &mut output);
        }
    }

    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        ALGORITHM_LATENCY_SAMPLES
    }

    #[must_use]
    pub const fn last_local_snr_db(&self) -> f32 {
        self.last_local_snr_db
    }

    /// Whether voice-band protection or speech recovery was active for the latest spectral frame.
    #[must_use]
    pub const fn last_voice_protected(&self) -> bool {
        self.last_voice_protected
    }
}
