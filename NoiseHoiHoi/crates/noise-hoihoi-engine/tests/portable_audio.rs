use noise_hoihoi_engine::{
    AudioProcessor, EngineConfig, EngineState, PIPELINE_SAMPLE_RATE, RunningAudioEngine,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

struct HalfGain(Arc<AtomicBool>);
impl AudioProcessor for HalfGain {
    fn process(&mut self, samples: &mut [f32]) -> Result<(), String> {
        for sample in samples {
            *sample *= 0.5;
        }
        Ok(())
    }
}
impl Drop for HalfGain {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn processes_and_monitors_audio_without_native_devices_and_joins_on_drop() {
    let dropped = Arc::new(AtomicBool::new(false));
    let (mut engine, mut ports) = RunningAudioEngine::prepare(
        &EngineConfig::new("in-memory"),
        PIPELINE_SAMPLE_RATE,
        HalfGain(Arc::clone(&dropped)),
    )
    .unwrap();
    let metrics = engine.metrics();
    engine.set_signal_monitor_enabled(true);
    for _ in 0..4800 {
        ports.input.push(0.25).unwrap();
    }
    ports.worker.unpark();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut samples = Vec::new();
    while samples.len() < 480 {
        engine.drain_signal_monitor_samples(&mut samples);
        assert!(
            Instant::now() < deadline,
            "worker did not publish monitor samples"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(samples.iter().any(|sample| sample.output.abs() > 0.1));
    for sample in samples {
        assert!((sample.output - sample.input * 0.5).abs() < 1e-6);
        assert!((sample.difference() - (sample.input - sample.output)).abs() < 1e-6);
    }
    assert!(
        ports
            .output
            .read_chunk(ports.output.slots())
            .unwrap()
            .into_iter()
            .any(|sample| sample.abs() > 0.1)
    );
    drop(engine);
    assert!(ports.stop.load(Ordering::Acquire));
    assert!(
        dropped.load(Ordering::Acquire),
        "processing worker must be joined"
    );
    assert_eq!(metrics.snapshot().state, EngineState::Stopped);
}
