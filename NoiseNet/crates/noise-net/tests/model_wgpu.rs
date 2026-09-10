use noise_net_runtime::ComputeProcessor;
use noise_net_runtime::{create_device, processors};
mod common;

use std::io::Cursor;

use common::{
    REFERENCE_FRAMES, assert_sustained_vowel_is_protected, expected_reference, process_reference,
    reference_error,
};
use noise_net::{FRAME_SIZE, NoiseNet, SAMPLE_RATE};

fn quiet_recorded_input() -> Vec<f32> {
    let reader = hound::WavReader::new(Cursor::new(include_bytes!(
        "../../../../sample-sound/aiueo.wav"
    )))
    .unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, SAMPLE_RATE);
    assert_eq!(spec.bits_per_sample, 16);
    reader
        .into_samples::<i16>()
        .step_by(usize::from(spec.channels))
        .take(180 * FRAME_SIZE)
        .map(|s| f32::from(s.unwrap()) / 32_768.0 * 0.063_095_73)
        .collect()
}

fn process_product(net: &mut NoiseNet, input: &[f32]) -> Vec<f32> {
    net.reset();
    let mut result = vec![];
    for frame in input.chunks_exact(FRAME_SIZE) {
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(frame.try_into().unwrap(), &mut output);
        assert!(output.iter().all(|s| s.is_finite()));
        result.extend(output);
    }
    result
}

#[test]
#[ignore = "requires a physical WGPU adapter; run with `just test-noisenet-wgpu`"]
fn every_wgpu_processor_matches_the_official_reference() {
    let processors = processors()
        .into_iter()
        .filter(ComputeProcessor::is_gpu)
        .collect::<Vec<_>>();
    assert!(!processors.is_empty(), "no selectable WGPU processor found");
    let expected = expected_reference();
    let quiet_input = quiet_recorded_input();
    let expected_product = process_product(&mut NoiseNet::new_cpu().unwrap(), &quiet_input);

    for processor in processors {
        let mut net = NoiseNet::from_device(
            create_device(&processor, processor.default_runtime()).unwrap(),
            true,
        )
        .unwrap_or_else(|error| panic!("{} failed to initialize: {error}", processor.name()));
        let actual = process_reference(&mut net, REFERENCE_FRAMES);
        let (rmse, max_error) = reference_error(&actual, &expected);
        assert!(
            rmse <= 2.0e-4 && max_error <= 1.0e-3,
            "{} differs from the official reference: rmse={rmse:e}, max={max_error:e}",
            processor.name()
        );
        assert_sustained_vowel_is_protected(&mut net, 105.0, 12);
        let actual_product = process_product(&mut net, &quiet_input);
        let (rmse, max_error) = reference_error(&actual_product, &expected_product);
        eprintln!(
            "{} quiet recorded product: rmse={rmse:e}, max={max_error:e}",
            processor.name()
        );
        assert!(
            rmse <= 1.0e-4 && max_error <= 5.0e-4,
            "{} product output differs from CPU: rmse={rmse:e}, max={max_error:e}",
            processor.name()
        );
    }
}
