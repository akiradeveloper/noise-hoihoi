/// One aligned sample captured immediately before and after audio processing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SignalMonitorSample {
    pub input: f32,
    pub output: f32,
}

impl SignalMonitorSample {
    #[cfg(any(windows, test))]
    pub(crate) fn new(input: f32, output: f32) -> Self {
        Self { input, output }
    }

    /// The component removed or otherwise changed by the processor.
    #[must_use]
    pub fn difference(self) -> f32 {
        self.input - self.output
    }
}

#[cfg(test)]
mod tests {
    use super::SignalMonitorSample;

    #[test]
    fn difference_is_input_minus_output() {
        let sample = SignalMonitorSample::new(0.75, 0.25);

        assert_eq!(sample.difference().to_bits(), 0.5_f32.to_bits());
    }

    #[test]
    fn pass_through_has_no_difference() {
        let sample = SignalMonitorSample::new(-0.375, -0.375);

        assert_eq!(sample.difference().to_bits(), 0.0_f32.to_bits());
    }
}
