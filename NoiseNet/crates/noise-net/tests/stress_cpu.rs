use noise_net::{FRAME_SIZE, NoiseNet, SAMPLE_RATE};

const STRESS_FRAMES: usize = 1_000;

#[test]
#[ignore = "long-running CPU test; run with `just test-noisenet-full`"]
#[allow(clippy::cast_precision_loss)]
fn sustained_stream_remains_finite() {
    let mut net = NoiseNet::new_cpu().expect("Burn Flex is required for NoiseNet v0.3");
    let mut peak = 0.0_f32;
    for frame_index in 0..STRESS_FRAMES {
        let input = std::array::from_fn(|sample| {
            let index = frame_index * FRAME_SIZE + sample;
            if frame_index % 127 < 7 {
                return 0.0;
            }
            let time = index as f32 / SAMPLE_RATE as f32;
            let pseudo_noise = ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16)
                & 0x7fff) as f32
                / 16_384.0
                - 1.0;
            0.15 * (std::f32::consts::TAU * 190.0 * time).sin() + 0.02 * pseudo_noise
        });
        let mut output = [0.0; FRAME_SIZE];
        net.process_frame(&input, &mut output);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(net.last_local_snr_db().is_finite());
        peak = output
            .iter()
            .map(|sample| sample.abs())
            .fold(peak, f32::max);
    }
    assert!(peak > 1.0e-5);
    assert!(peak < 10.0, "unexpectedly unstable output peak: {peak}");
}
