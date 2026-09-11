use noise_net_iree::{DpdfNet, FRAME_SIZE};

#[test]
#[ignore = "requires packaged IREE and a physical Vulkan GPU"]
fn gpu_reset_and_independent_streams_match_reference() {
    check(&noise_net_iree::devices().unwrap().remove(0).uri);
}
#[test]
#[ignore = "requires packaged IREE runtime"]
fn cpu_reset_and_independent_streams_match_reference() {
    check("cpu");
}
fn check(uri: &str) {
    let input = decode(include_bytes!("../testdata/dpdfnet8-input.f32le"));
    let reference = decode(include_bytes!("../testdata/dpdfnet8-output.f32le"));
    let mut first = DpdfNet::new(uri).unwrap();
    let initial = run(&mut first, &input);
    first.reset().unwrap();
    let repeated = run(&mut first, &input);
    assert_eq!(
        initial, repeated,
        "reset must clear model state and both STFT histories"
    );
    let second = DpdfNet::new(uri).unwrap();
    let independent = std::thread::spawn(move || run(&mut { second }, &input))
        .join()
        .unwrap();
    assert_eq!(
        initial, independent,
        "sessions must not share mutable state"
    );
    let peak = initial
        .iter()
        .zip(&reference)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(peak < 2e-4, "reference peak difference {peak}");
    assert!(DpdfNet::new("vulkan://GPU-no-longer-connected").is_err());
}
fn decode(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn run(net: &mut DpdfNet, input: &[f32]) -> Vec<f32> {
    let mut output = Vec::with_capacity(input.len());
    for frame in input.chunks_exact(FRAME_SIZE) {
        let mut enhanced = [0.0; FRAME_SIZE];
        net.process_frame(frame.try_into().unwrap(), &mut enhanced)
            .unwrap();
        output.extend(enhanced);
    }
    output
}
