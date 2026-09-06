#[cfg(any(target_os = "windows", test))]
use std::collections::VecDeque;

/// In-place processing stage between microphone capture and virtual output.
///
/// Implementations run on the dedicated processing thread, never on the GUI
/// or an operating-system audio callback thread.
pub trait AudioProcessor: Send + 'static {
    fn process(&mut self, mono_48khz: &mut [f32]);

    /// Required processing block size, or `None` for arbitrary slices.
    fn frame_size(&self) -> Option<usize> {
        None
    }

    /// Content delay introduced by the processor at 48 kHz.
    fn latency_samples(&self) -> usize {
        0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PassThrough;

impl AudioProcessor for PassThrough {
    fn process(&mut self, _mono_48khz: &mut [f32]) {}
}

/// `DeepFilterNet3` running on Burn Flex's CPU device.
pub struct NoiseReduction {
    inner: noise_net::NoiseNet,
}

impl NoiseReduction {
    /// Load the embedded `DeepFilterNet3` model.
    ///
    /// # Errors
    ///
    /// Returns an error if the CPU runtime or model cannot initialize.
    pub fn new_cpu() -> Result<Self, crate::EngineError> {
        noise_net::NoiseNet::new_cpu()
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

/// Adapts variable resampler chunks to a processor's fixed frame size and
/// delay-aligns the corresponding monitor input.
#[cfg(any(target_os = "windows", test))]
pub(crate) struct ProcessorPipeline<P> {
    processor: P,
    frame_size: Option<usize>,
    pending: VecDeque<f32>,
    frame: Vec<f32>,
    aligned_input: Vec<f32>,
    input_delay: VecDeque<f32>,
}

#[cfg(any(target_os = "windows", test))]
impl<P: AudioProcessor> ProcessorPipeline<P> {
    pub(crate) fn new(processor: P) -> Result<Self, &'static str> {
        let frame_size = processor.frame_size();
        if frame_size == Some(0) {
            return Err("processor reported a zero frame size");
        }
        let capacity = frame_size.unwrap_or_default();
        let latency = processor.latency_samples();
        Ok(Self {
            processor,
            frame_size,
            pending: VecDeque::with_capacity(capacity * 2),
            frame: vec![0.0; capacity],
            aligned_input: Vec::with_capacity(capacity),
            input_delay: std::iter::repeat_n(0.0, latency).collect(),
        })
    }

    pub(crate) const fn frame_size(&self) -> Option<usize> {
        self.frame_size
    }

    pub(crate) fn process<E>(
        &mut self,
        input: &[f32],
        mut publish: impl FnMut(&[f32], &[f32]) -> Result<(), E>,
    ) -> Result<usize, E> {
        let mut processed = 0;
        if let Some(frame_size) = self.frame_size {
            self.pending.extend(input);
            while self.pending.len() >= frame_size {
                for sample in &mut self.frame {
                    *sample = self
                        .pending
                        .pop_front()
                        .expect("the complete processor frame was checked");
                }
                align_input(&mut self.input_delay, &self.frame, &mut self.aligned_input);
                self.processor.process(&mut self.frame);
                publish(&self.aligned_input, &self.frame)?;
                processed += frame_size;
            }
        } else {
            self.frame.clear();
            self.frame.extend_from_slice(input);
            align_input(&mut self.input_delay, &self.frame, &mut self.aligned_input);
            self.processor.process(&mut self.frame);
            publish(&self.aligned_input, &self.frame)?;
            processed = input.len();
        }
        Ok(processed)
    }
}

#[cfg(any(target_os = "windows", test))]
fn align_input(delay: &mut VecDeque<f32>, input: &[f32], aligned: &mut Vec<f32>) {
    aligned.clear();
    aligned.reserve(input.len());
    for &sample in input {
        delay.push_back(sample);
        aligned.push(delay.pop_front().expect("delay line is never empty"));
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioProcessor, PassThrough, ProcessorPipeline};

    #[test]
    fn pass_through_is_bit_exact() {
        let mut samples = [-1.0, -0.25, 0.0, 0.5, 1.0];
        let expected = samples;

        PassThrough.process(&mut samples);

        assert_eq!(samples.map(f32::to_bits), expected.map(f32::to_bits));
    }

    #[test]
    fn pass_through_has_no_block_or_content_delay() {
        assert_eq!(PassThrough.frame_size(), None);
        assert_eq!(PassThrough.latency_samples(), 0);
    }

    #[derive(Clone, Copy)]
    struct Doubler;

    impl AudioProcessor for Doubler {
        fn process(&mut self, samples: &mut [f32]) {
            for sample in samples {
                *sample *= 2.0;
            }
        }

        fn frame_size(&self) -> Option<usize> {
            Some(4)
        }

        fn latency_samples(&self) -> usize {
            2
        }
    }

    #[test]
    fn fixed_frames_survive_arbitrary_chunk_boundaries_and_align_monitor_input() {
        let mut pipeline = ProcessorPipeline::new(Doubler).unwrap();
        assert_eq!(pipeline.frame_size(), Some(4));
        let mut published = Vec::new();
        let mut collect = |input: &[f32], output: &[f32]| -> Result<(), ()> {
            published.push((input.to_vec(), output.to_vec()));
            Ok(())
        };

        assert_eq!(pipeline.process(&[1.0, 2.0, 3.0], &mut collect), Ok(0));
        assert_eq!(
            pipeline.process(&[4.0, 5.0, 6.0, 7.0, 8.0, 9.0], &mut collect),
            Ok(8)
        );
        assert_eq!(
            published,
            vec![
                (vec![0.0, 0.0, 1.0, 2.0], vec![2.0, 4.0, 6.0, 8.0]),
                (vec![3.0, 4.0, 5.0, 6.0], vec![10.0, 12.0, 14.0, 16.0]),
            ]
        );
    }

    #[test]
    fn arbitrary_processor_publishes_each_chunk_without_delay() {
        let mut pipeline = ProcessorPipeline::new(PassThrough).unwrap();
        let mut published = Vec::new();
        let processed = pipeline
            .process(&[0.25, -0.5], |input, output| {
                published.push((input.to_vec(), output.to_vec()));
                Ok::<_, ()>(())
            })
            .unwrap();

        assert_eq!(processed, 2);
        assert_eq!(published, vec![(vec![0.25, -0.5], vec![0.25, -0.5])]);
    }
}
