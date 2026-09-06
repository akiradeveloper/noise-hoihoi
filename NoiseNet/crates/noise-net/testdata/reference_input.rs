use noise_net::SAMPLE_RATE;

#[allow(clippy::cast_precision_loss)]
pub fn sample(index: usize) -> f32 {
    let time = index as f32 / SAMPLE_RATE as f32;
    let syllable = (std::f32::consts::PI * 3.0 * time).sin().max(0.0);
    let voice = syllable
        * (0.18 * (std::f32::consts::TAU * 137.0 * time).sin()
            + 0.08 * (std::f32::consts::TAU * 274.0 * time).sin()
            + 0.04 * (std::f32::consts::TAU * 822.0 * time).sin());
    let fan = 0.025 * (std::f32::consts::TAU * 83.0 * time).sin();
    let click_phase = index % 9_600;
    let click = if click_phase < 180 {
        0.22 * (1.0 - click_phase as f32 / 180.0) * (std::f32::consts::TAU * 3_700.0 * time).sin()
    } else {
        0.0
    };
    let pseudo_noise =
        ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 16) & 0x7fff) as f32 / 16_384.0
            - 1.0;
    voice + fan + click + 0.008 * pseudo_noise
}
