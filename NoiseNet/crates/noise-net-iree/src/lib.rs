//! IREE CPU/Vulkan inference with persistent recurrent state and a fixed streaming STFT.
mod native;
mod stft;
use anyhow::{Result, ensure};
use realfft::num_complex::Complex32;
use stft::Stft;

pub const FRAME_SIZE: usize = 480;
pub const LATENCY_SAMPLES: usize = 2400;

#[derive(Clone, Debug)]
pub struct Device {
    pub uri: String,
    pub name: String,
    pub integrated: bool,
}
/// Enumerate physical GPUs with persistent Vulkan UUID paths.
/// # Errors
/// Returns a library or driver discovery error.
pub fn devices() -> Result<Vec<Device>> {
    native::devices()
}

pub struct DpdfNet {
    stream: native::Stream,
    dsp: Stft,
    spectrum: Vec<Complex32>,
    input: [f32; 962],
    output: [f32; 962],
    primed: bool,
}
impl DpdfNet {
    /// Load and warm `cpu` or the selected Vulkan UUID before opening audio.
    /// # Errors
    /// Returns a library, device, model or warm-up error.
    pub fn new(uri: &str) -> Result<Self> {
        ensure!(
            uri == "cpu" || devices()?.iter().any(|d| d.uri == uri),
            "selected processor is no longer available"
        );
        let mut net = Self {
            stream: native::Stream::new(uri, &initial_state())?,
            dsp: Stft::default(),
            spectrum: vec![Complex32::new(0.0, 0.0); 481],
            input: [0.0; 962],
            output: [0.0; 962],
            primed: false,
        };
        for _ in 0..33 {
            net.process_frame(&[0.01; FRAME_SIZE], &mut [0.0; FRAME_SIZE])?;
        }
        net.reset()?;
        Ok(net)
    }
    /// Process one 10 ms hop, including synchronous upload, readback and DSP.
    /// # Errors
    /// Returns an inference or invalid output error.
    pub fn process_frame(
        &mut self,
        input: &[f32; FRAME_SIZE],
        output: &mut [f32; FRAME_SIZE],
    ) -> Result<()> {
        self.dsp.analysis(input, &mut self.spectrum)?;
        if !self.primed {
            self.primed = true;
            output.fill(0.0);
            return Ok(());
        }
        for (ri, bin) in self.input.chunks_exact_mut(2).zip(&self.spectrum) {
            ri[0] = bin.re;
            ri[1] = bin.im;
        }
        self.stream.process(&self.input, &mut self.output)?;
        ensure!(
            self.output.iter().all(|v| v.is_finite()),
            "Processor produced non-finite audio"
        );
        for (bin, ri) in self.spectrum.iter_mut().zip(self.output.chunks_exact(2)) {
            *bin = Complex32::new(ri[0] / 960.0, ri[1] / 960.0);
        }
        self.dsp.synthesis(&mut self.spectrum, output)?;
        Ok(())
    }
    /// Clear recurrent state and analysis/synthesis history.
    /// # Errors
    /// Returns a device state upload error.
    pub fn reset(&mut self) -> Result<()> {
        self.stream.reset(&initial_state())?;
        self.dsp = Stft::default();
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.primed = false;
        Ok(())
    }
    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        LATENCY_SAMPLES
    }
}
fn initial_state() -> Box<[f32; 90_228]> {
    let mut state: Box<[f32; 90_228]> = vec![0.0; 90_228]
        .into_boxed_slice()
        .try_into()
        .expect("fixed state size");
    for (value, bytes) in state
        .iter_mut()
        .zip(include_bytes!("../model/dpdfnet8-norm.f32le").chunks_exact(4))
    {
        *value = f32::from_le_bytes(bytes.try_into().expect("four-byte f32"));
    }
    state
}
