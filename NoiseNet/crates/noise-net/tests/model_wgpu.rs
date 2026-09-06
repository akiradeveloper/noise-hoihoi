mod common;

use common::{
    REFERENCE_FRAMES, assert_sustained_vowel_is_protected, expected_reference, process_reference,
    reference_error,
};
use noise_net::{ComputeProcessor, NoiseNet, processors};

#[test]
#[ignore = "requires a physical WGPU adapter; run with `just test-noisenet-wgpu`"]
fn every_wgpu_processor_matches_the_official_reference() {
    let processors = processors()
        .into_iter()
        .filter(ComputeProcessor::is_gpu)
        .collect::<Vec<_>>();
    assert!(!processors.is_empty(), "no selectable WGPU processor found");
    let expected = expected_reference();

    for processor in processors {
        let mut net = NoiseNet::new(&processor, processor.default_runtime())
            .unwrap_or_else(|error| panic!("{} failed to initialize: {error}", processor.name()));
        let actual = process_reference(&mut net, REFERENCE_FRAMES);
        let (rmse, max_error) = reference_error(&actual, &expected);
        assert!(
            rmse <= 2.0e-4 && max_error <= 1.0e-3,
            "{} differs from the official reference: rmse={rmse:e}, max={max_error:e}",
            processor.name()
        );
        assert_sustained_vowel_is_protected(&mut net, 105.0, 12);
    }
}
