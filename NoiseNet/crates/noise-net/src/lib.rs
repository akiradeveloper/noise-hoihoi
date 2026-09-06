//! Streaming `DeepFilterNet3` inference implemented with Burn.

mod dsp;
mod generated;
mod network;

use std::panic::{AssertUnwindSafe, catch_unwind};

use dsp::DspState;
use network::StreamingNetwork;
use thiserror::Error;

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SIZE: usize = 480;
pub const FFT_SIZE: usize = 960;
pub const FFT_BINS: usize = FFT_SIZE / 2 + 1;
pub const ERB_BANDS: usize = 32;
pub const DF_BINS: usize = 96;
pub const DF_ORDER: usize = 5;
pub const DF_LOOKAHEAD: usize = 2;
pub const ALGORITHM_LATENCY_SAMPLES: usize = (FFT_SIZE - FRAME_SIZE) + DF_LOOKAHEAD * FRAME_SIZE;

#[derive(Debug, Error)]
pub enum NoiseNetError {
    #[error("Burn Flex could not initialize or load DeepFilterNet3")]
    Initialization,
}

/// A mono, 48 kHz, stateful `DeepFilterNet3` inference stream.
pub struct NoiseNet {
    network: StreamingNetwork,
    dsp: DspState,
    last_local_snr_db: f32,
}

impl NoiseNet {
    /// Load the official `DeepFilterNet3` weights on Burn Flex's CPU device.
    ///
    /// # Errors
    ///
    /// Returns [`NoiseNetError::Initialization`] if the CPU runtime or embedded
    /// model cannot be initialized.
    pub fn new_cpu() -> Result<Self, NoiseNetError> {
        catch_unwind(AssertUnwindSafe(|| Self {
            network: StreamingNetwork::new_cpu(),
            dsp: DspState::new(),
            last_local_snr_db: -15.0,
        }))
        .map_err(|_| NoiseNetError::Initialization)
    }

    /// Process exactly one 10 ms frame.
    pub fn process_frame(&mut self, input: &[f32; FRAME_SIZE], output: &mut [f32; FRAME_SIZE]) {
        let features = self.dsp.analyze(input);
        let inference = self.network.infer(features.erb, &features.complex);
        self.dsp.synthesize(
            inference.mask.as_deref(),
            inference.coefficients.as_deref(),
            output,
        );
        self.last_local_snr_db = inference.local_snr_db;
    }

    /// Reset all recurrent, normalization, FFT, and history state.
    pub fn reset(&mut self) {
        self.network.reset();
        self.dsp = DspState::new();
        self.last_local_snr_db = -15.0;
    }

    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        ALGORITHM_LATENCY_SAMPLES
    }

    #[must_use]
    pub const fn last_local_snr_db(&self) -> f32 {
        self.last_local_snr_db
    }
}
