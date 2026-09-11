use noise_hoihoi_platform::{AudioProcessor, ComputeRuntime, NoiseReduction, compute_processors};

#[test]
#[ignore = "requires packaged native IREE runtime"]
fn selected_cpu_matches_the_independent_reference() {
    check(false);
}

#[test]
#[ignore = "requires packaged IREE runtime and physical GPU"]
fn selected_native_gpu_matches_the_independent_reference() {
    check(true);
}

#[allow(clippy::cast_precision_loss)]
fn check(gpu: bool) {
    let devices = compute_processors();
    let device = devices
        .iter()
        .find(|d| d.is_gpu() == gpu)
        .expect("required test processor");
    let runtime = device.default_runtime();
    assert_eq!(
        runtime,
        if gpu {
            ComputeRuntime::Vulkan
        } else {
            ComputeRuntime::Cpu
        }
    );
    let processor = NoiseReduction::new(device, runtime).unwrap();
    let decode = |bytes: &[u8]| {
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect::<Vec<_>>()
    };
    let input = decode(include_bytes!(
        "../../../../NoiseNet/crates/noise-net-iree/testdata/dpdfnet8-input.f32le"
    ));
    let expected = decode(include_bytes!(
        "../../../../NoiseNet/crates/noise-net-iree/testdata/dpdfnet8-output.f32le"
    ));
    let actual = std::thread::spawn(move || {
        let mut processor = processor;
        assert_eq!(processor.frame_size(), Some(480));
        assert_eq!(processor.latency_samples(), 2400);
        let mut audio = input;
        for frame in audio.chunks_exact_mut(480) {
            processor.process(frame).unwrap();
        }
        audio
    })
    .join()
    .unwrap();
    let peak = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    let mse = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| f64::from(a - b).powi(2))
        .sum::<f64>()
        / expected.len() as f64;
    eprintln!("{runtime}: RMSE {}, peak {peak}", mse.sqrt());
    assert!(mse.sqrt() < 5e-5 && peak < 2e-4);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_rejects_unavailable_processor_before_opening_audio() {
    use noise_hoihoi_platform::{EngineConfig, EngineError, NativeBackend};
    use noise_hoihoi_session::SessionBackend;

    // An unavailable processor must not select a different physical GPU.
    let devices = NativeBackend.processors();
    assert_eq!(devices[0].id(), "cpu");
    assert!(!devices[0].is_gpu());
    let result = NativeBackend.start(
        &EngineConfig::new("unused-test-microphone"),
        Some("unavailable-processor"),
    );
    assert!(matches!(result, Err(EngineError::NoiseReduction(message))
        if message == "selected processor is no longer available"));
}
