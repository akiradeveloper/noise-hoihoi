#![cfg(target_os = "linux")]

use noise_hoihoi_platform::{EngineConfig, EngineState, PassThrough, input_devices, start};
use std::{
    fs,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn pactl(args: &[&str]) -> String {
    let result = Command::new("pactl").args(args).output().unwrap();
    assert!(
        result.status.success(),
        "pactl {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

/// Uses only the private server configured by scripts/docker/test-linux-audio.sh.
#[test]
#[ignore = "requires an isolated audio server; run just test-linux-audio"]
fn virtual_microphone_routes_audio_and_cleans_up() {
    assert_eq!(std::env::var("NOISE_HOIHOI_AUDIO_TEST").as_deref(), Ok("1"));
    let runtime = std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let input_file = runtime.join("input.raw");
    let output_file = runtime.join("output.raw");
    let samples: Vec<u8> = (0..48_000_u16)
        .cycle()
        .take(48_000 * 6)
        .flat_map(|i| {
            // Reproduce a Linux microphone whose positive DC offset shifted
            // the monitor waveform toward the top of every plot.
            (0.23 + 0.25 * (std::f32::consts::TAU * 440.0 * f32::from(i) / 48_000.0).sin())
                .to_le_bytes()
        })
        .collect();
    fs::write(&input_file, samples).unwrap();
    assert!(
        input_devices()
            .unwrap()
            .iter()
            .any(|d| d.id == "test_microphone")
    );
    assert!(start(&EngineConfig::new("missing_microphone"), PassThrough).is_err());
    assert!(!pactl(&["list", "short", "sinks"]).contains("noise_hoihoi_output"));

    for _ in 0..2 {
        let mut engine = start(&EngineConfig::new("test_microphone"), PassThrough).unwrap();
        engine.set_signal_monitor_enabled(true);
        assert!(pactl(&["list", "sources"]).contains("NoiseHoiHoi Microphone"));
        assert!(
            !input_devices()
                .unwrap()
                .iter()
                .any(|d| d.id.contains("noise_hoihoi") || d.id.ends_with(".monitor"))
        );
        assert!(start(&EngineConfig::new("test_microphone"), PassThrough).is_err());
        assert!(start(&EngineConfig::new("noise_hoihoi_microphone"), PassThrough).is_err());
        let recorder = Process(
            Command::new("parec")
                .args([
                    "--raw",
                    "--format=float32le",
                    "--rate=48000",
                    "--channels=1",
                    "--device=noise_hoihoi_microphone",
                    "--latency-msec=20",
                ])
                .stdout(Stdio::from(fs::File::create(&output_file).unwrap()))
                .spawn()
                .unwrap(),
        );
        let player = Process(
            Command::new("pacat")
                .args([
                    "--playback",
                    "--raw",
                    "--format=float32le",
                    "--rate=48000",
                    "--channels=1",
                    "--device=test_sink",
                    "--latency-msec=20",
                ])
                .stdin(Stdio::from(fs::File::open(&input_file).unwrap()))
                .spawn()
                .unwrap(),
        );
        let mut monitor = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            engine.drain_signal_monitor_samples(&mut monitor);
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(engine.metrics().snapshot().state, EngineState::Running);
        assert!(engine.metrics().snapshot().processed_frames > 48_000);
        engine.drain_signal_monitor_samples(&mut monitor);
        assert_monitor_tone(&monitor);
        drop(player);
        drop(recorder);
        assert_recorded_tone(&output_file);
        let now = Instant::now();
        engine.stop();
        assert!(now.elapsed() < Duration::from_secs(2));
        assert!(!pactl(&["list", "short", "sources"]).contains("noise_hoihoi_microphone"));
        assert!(!pactl(&["list", "short", "sinks"]).contains("noise_hoihoi_output"));
    }
    // A conflicting source must not cause an extra null sink to be created.
    let occupied = pactl(&[
        "load-module",
        "module-remap-source",
        "master=test_sink.monitor",
        "source_name=noise_hoihoi_microphone",
    ]);
    assert!(start(&EngineConfig::new("test_microphone"), PassThrough).is_err());
    assert!(!pactl(&["list", "short", "sinks"]).contains("noise_hoihoi_output"));
    pactl(&["unload-module", occupied.trim()]);
    assert_endpoint_loss_faults();
    assert_recovers_after_force_kill(&runtime);
    assert_cpu_reduction_with_monitor(&input_file);
}

fn assert_monitor_tone(monitor: &[noise_hoihoi_platform::SignalMonitorSample]) {
    assert!(!monitor.is_empty());
    assert!(
        monitor
            .iter()
            .all(|s| s.input.to_bits() == s.output.to_bits())
    );
    let mean = monitor.iter().map(|s| f64::from(s.input)).sum::<f64>()
        / f64::from(u32::try_from(monitor.len()).unwrap());
    assert!(mean.abs() < 0.005, "monitor DC offset: {mean}");
    assert!(monitor.iter().any(|s| s.input < -0.2));
    assert!(monitor.iter().any(|s| s.input > 0.2));
}

fn assert_recorded_tone(path: &std::path::Path) {
    let data = fs::read(path).unwrap();
    let audio: Vec<f32> = data
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert!(
        audio.len() > 48_000,
        "only {} captured samples",
        audio.len()
    );
    assert!(audio.iter().all(|x| x.is_finite()));
    let loudest_rms = audio
        .chunks_exact(4_800)
        .map(|chunk| (chunk.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / 4_800.0).sqrt())
        .fold(0.0_f64, f64::max);
    assert!(
        (0.14..0.21).contains(&loudest_rms),
        "440 Hz signal RMS: {loudest_rms}"
    );
}

fn assert_endpoint_loss_faults() {
    let engine = start(&EngineConfig::new("test_microphone"), PassThrough).unwrap();
    let modules = pactl(&["list", "short", "modules"]);
    let sink_module = modules
        .lines()
        .find(|line| line.contains("sink_name=noise_hoihoi_output"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    pactl(&["unload-module", sink_module]);
    let deadline = Instant::now() + Duration::from_secs(2);
    while engine.metrics().snapshot().state != EngineState::Faulted && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(engine.metrics().snapshot().state, EngineState::Faulted);
    engine.stop();
    assert!(!pactl(&["list", "short", "sources"]).contains("noise_hoihoi_microphone"));
}

fn assert_recovers_after_force_kill(runtime: &std::path::Path) {
    let marker = runtime.join("crash-child-ready");
    let mut child = Process(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "crash_route_child"])
            .env("NOISE_HOIHOI_CRASH_CHILD", &marker)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        assert!(child.0.try_wait().unwrap().is_none());
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "child route did not start");
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(pactl(&["list", "short", "sources"]).contains("noise_hoihoi_microphone"));
    let engine = start(&EngineConfig::new("test_microphone"), PassThrough).unwrap();
    assert_eq!(engine.metrics().snapshot().state, EngineState::Running);
    engine.stop();
    assert!(!pactl(&["list", "short", "sources"]).contains("noise_hoihoi_microphone"));
}

#[test]
#[ignore = "subprocess fixture for the forced-exit regression"]
fn crash_route_child() {
    let Some(marker) = std::env::var_os("NOISE_HOIHOI_CRASH_CHILD") else {
        return;
    };
    let _engine = start(&EngineConfig::new("test_microphone"), PassThrough).unwrap();
    fs::write(marker, b"ready").unwrap();
    loop {
        thread::park_timeout(Duration::from_secs(1));
    }
}

fn assert_cpu_reduction_with_monitor(input_file: &std::path::Path) {
    let cpu = noise_hoihoi_platform::compute_processors()
        .into_iter()
        .find(|p| !p.is_gpu())
        .unwrap();
    for _ in 0..2 {
        let processor =
            noise_hoihoi_platform::NoiseReduction::new(&cpu, cpu.default_runtime()).unwrap();
        let mut engine = start(&EngineConfig::new("test_microphone"), processor).unwrap();
        engine.set_signal_monitor_enabled(true);
        let _player = Process(
            Command::new("pacat")
                .args([
                    "--playback",
                    "--raw",
                    "--format=float32le",
                    "--rate=48000",
                    "--channels=1",
                    "--device=test_sink",
                    "--latency-msec=20",
                ])
                .stdin(Stdio::from(fs::File::open(input_file).unwrap()))
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut monitor = Vec::new();
        let mut received = 0;
        let mut heard_input = false;
        while received < 48_000 && Instant::now() < deadline {
            monitor.clear();
            engine.drain_signal_monitor_samples(&mut monitor);
            received += monitor.len();
            heard_input |= monitor.iter().any(|s| s.input.abs() > 0.01);
            assert!(
                monitor
                    .iter()
                    .all(|s| s.input.is_finite() && s.output.is_finite())
            );
            assert_eq!(engine.metrics().snapshot().state, EngineState::Running);
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            received >= 48_000 && heard_input,
            "CPU inference/monitor stopped making progress"
        );
        let stopped = Instant::now();
        engine.stop();
        assert!(stopped.elapsed() < Duration::from_secs(2));
    }
}
