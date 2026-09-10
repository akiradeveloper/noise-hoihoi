use noise_hoihoi_engine::{
    AudioDevice, AudioPorts, EngineConfig, EngineError, EngineState, PIPELINE_SAMPLE_RATE,
    PassThrough, RunningAudioEngine,
};
use noise_hoihoi_session::{ComputeProcessor, Session, SessionBackend, Settings};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    events: Mutex<Vec<String>>,
    saved: Mutex<Option<Settings>>,
    fail: AtomicBool,
    gate: Mutex<Option<mpsc::Receiver<()>>>,
}
struct Backend(Arc<State>);
struct Route {
    state: Arc<State>,
    ports: AudioPorts,
}
impl Drop for Route {
    fn drop(&mut self) {
        assert!(self.ports.stop.load(Ordering::Acquire));
        self.state.events.lock().unwrap().push("closed".into());
    }
}
impl SessionBackend for Backend {
    fn input_devices(&self) -> Result<Vec<AudioDevice>, EngineError> {
        Ok(vec![AudioDevice {
            id: "mic".into(),
            name: "Test mic".into(),
            is_default: true,
        }])
    }
    fn processors(&self) -> Vec<ComputeProcessor> {
        vec![ComputeProcessor {
            id: "cpu".into(),
            name: "Test CPU".into(),
            is_gpu: false,
            runtime: "Test".into(),
        }]
    }
    fn output_name(&self) -> &'static str {
        "Test output"
    }
    fn load_settings(&self) -> (Settings, Option<String>) {
        (
            Settings {
                input_device_id: Some("missing-mic".into()),
                processor_id: Some("missing-gpu".into()),
                noise_reduction: false,
            },
            None,
        )
    }
    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        *self.0.saved.lock().unwrap() = Some(settings.clone());
        Ok(())
    }
    fn start(
        &self,
        config: &EngineConfig,
        processor: Option<&str>,
    ) -> Result<RunningAudioEngine, EngineError> {
        assert_eq!(config.input_device_id, "mic");
        self.0
            .events
            .lock()
            .unwrap()
            .push(format!("start:{}", processor.unwrap_or("bypass")));
        if let Some(gate) = self.0.gate.lock().unwrap().take() {
            gate.recv_timeout(Duration::from_secs(5))
                .expect("test must release startup");
        }
        if self.0.fail.swap(false, Ordering::AcqRel) {
            return Err(EngineError::Start("test failure".into()));
        }
        let (engine, ports) =
            RunningAudioEngine::prepare(config, PIPELINE_SAMPLE_RATE, PassThrough)?;
        ports.metrics.set_state(EngineState::Running);
        Ok(engine.with_route(Route {
            state: Arc::clone(&self.0),
            ports,
        }))
    }
}
fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for lifecycle transition"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn finish_start(session: &mut Session<Backend>) {
    wait_until(|| {
        session.poll_engine_start();
        !session.starting()
    });
}

#[test]
fn missing_devices_fall_back_and_start_failure_can_be_retried() {
    let state = Arc::new(State::default());
    state.fail.store(true, Ordering::Release);
    let mut session = Session::new(Backend(Arc::clone(&state)));
    assert_eq!(session.settings.input_device_id.as_deref(), Some("mic"));
    assert_eq!(session.settings.processor_id.as_deref(), Some("cpu"));
    assert!(session.notice.is_some());
    session.start();
    finish_start(&mut session);
    assert!(!session.has_engine());
    assert!(session.runtime_error().unwrap().contains("test failure"));
    session.start();
    finish_start(&mut session);
    assert!(session.has_engine());
    assert!(session.runtime_error().is_none());
    assert_eq!(session.metrics().state, EngineState::Running);
    session.take_shutdown().finish();
    assert_eq!(
        *state.events.lock().unwrap(),
        ["start:bypass", "start:bypass", "closed"]
    );
}

#[test]
fn reconfiguration_closes_previous_route_before_starting_and_saves_settings() {
    let state = Arc::new(State::default());
    let mut session = Session::new(Backend(Arc::clone(&state)));
    session.start();
    finish_start(&mut session);
    session.settings.noise_reduction = true;
    session.apply_settings();
    finish_start(&mut session);
    assert_eq!(*state.saved.lock().unwrap(), Some(session.settings.clone()));
    assert_eq!(
        *state.events.lock().unwrap(),
        ["start:bypass", "closed", "start:cpu"]
    );
    drop(session);
    assert_eq!(state.events.lock().unwrap().last().unwrap(), "closed");
}

#[test]
fn closing_during_startup_disposes_the_late_route_and_prevents_restart() {
    let state = Arc::new(State::default());
    let (release, gate) = mpsc::channel();
    *state.gate.lock().unwrap() = Some(gate);
    let mut session = Session::new(Backend(Arc::clone(&state)));
    session.start();
    wait_until(|| !state.events.lock().unwrap().is_empty());
    session.start(); // A second request while pending must not open another route.
    let shutdown = session.take_shutdown();
    assert!(session.busy());
    session.start();
    assert!(!session.starting());
    let cleanup = thread::spawn(move || shutdown.finish());
    release.send(()).unwrap();
    cleanup.join().unwrap();
    assert_eq!(*state.events.lock().unwrap(), ["start:bypass", "closed"]);
}
