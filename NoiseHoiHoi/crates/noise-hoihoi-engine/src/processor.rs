/// In-place processing stage between microphone capture and virtual output.
///
/// Implementations run on the dedicated processing thread, never on the GUI
/// or an operating-system audio callback thread.
pub trait AudioProcessor: Send + 'static {
    fn process(&mut self, mono_48khz: &mut [f32]);
}

/// The only processor exposed by v0.1.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassThrough;

impl AudioProcessor for PassThrough {
    fn process(&mut self, _mono_48khz: &mut [f32]) {}
}

#[cfg(test)]
mod tests {
    use super::{AudioProcessor, PassThrough};

    #[test]
    fn pass_through_is_bit_exact() {
        let mut samples = [-1.0, -0.25, 0.0, 0.5, 1.0];
        let expected = samples;

        PassThrough.process(&mut samples);

        assert_eq!(samples.map(f32::to_bits), expected.map(f32::to_bits));
    }
}
