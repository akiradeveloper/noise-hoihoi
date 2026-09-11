//! Application state without GPUI, native audio APIs, or GPU discovery dependencies.
pub mod performance;

use noise_hoihoi_engine::{
    AudioDevice, EngineConfig, EngineError, EngineMetrics, RunningAudioEngine, SignalMonitorSample,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::JoinHandle,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub input_device_id: Option<String>,
    pub noise_reduction: bool,
    pub processor_id: Option<String>,
}

/// Presentation data only; native compute handles remain in the backend.
#[derive(Clone, Debug)]
pub struct ComputeProcessor {
    pub id: String,
    pub name: String,
    pub is_gpu: bool,
    pub runtime: String,
}
impl ComputeProcessor {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        self.is_gpu
    }
    #[must_use]
    pub fn default_runtime(&self) -> &str {
        &self.runtime
    }
}

/// Control-plane services. Audio callbacks never call this interface.
pub trait SessionBackend: Send + Sync + 'static {
    /// Enumerate physical input devices.
    ///
    /// # Errors
    /// Returns an error when the native audio host is unavailable.
    fn input_devices(&self) -> Result<Vec<AudioDevice>, EngineError>;
    fn processors(&self) -> Vec<ComputeProcessor>;
    fn output_name(&self) -> &str;
    fn load_settings(&self) -> (Settings, Option<String>);
    /// Persist the requested settings.
    ///
    /// # Errors
    /// Returns an error when settings cannot be written.
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    /// Open a route, using passthrough when `processor` is `None`.
    ///
    /// # Errors
    /// Returns an error if the device, compute runtime, or route cannot initialize.
    fn start(
        &self,
        config: &EngineConfig,
        processor: Option<&str>,
    ) -> Result<RunningAudioEngine, EngineError>;
}

struct EngineStart {
    receiver: Receiver<Result<RunningAudioEngine, EngineError>>,
    worker: JoinHandle<()>,
}

/// Editable settings are applied explicitly; callers must not edit while `busy()`.
pub struct Session<B: SessionBackend> {
    backend: Arc<B>,
    pub settings: Settings,
    pub devices: Vec<AudioDevice>,
    pub processors: Vec<ComputeProcessor>,
    pub error: Option<String>,
    pub notice: Option<String>,
    engine: Option<RunningAudioEngine>,
    engine_start: Option<EngineStart>,
    monitor_enabled: bool,
    stopping: bool,
}

impl<B: SessionBackend> Session<B> {
    pub fn new(backend: B) -> Self {
        let (settings, notice) = backend.load_settings();
        let mut session = Self {
            backend: Arc::new(backend),
            settings,
            notice,
            devices: Vec::new(),
            processors: Vec::new(),
            error: None,
            engine: None,
            engine_start: None,
            monitor_enabled: false,
            stopping: false,
        };
        session.refresh_devices();
        session.refresh_processors();
        session
    }
    #[must_use]
    pub fn busy(&self) -> bool {
        self.starting() || self.stopping
    }
    #[must_use]
    pub fn starting(&self) -> bool {
        self.engine_start.is_some()
    }
    #[must_use]
    pub fn has_engine(&self) -> bool {
        self.engine.is_some()
    }
    #[must_use]
    pub fn output_name(&self) -> &str {
        self.backend.output_name()
    }
    pub fn apply_settings(&mut self) {
        if self.busy() {
            return;
        }
        if let Err(error) = self.backend.save_settings(&self.settings) {
            self.notice = Some(format!("Could not save settings: {error}"));
        }
        self.start();
    }
    pub fn set_monitor_enabled(&mut self, enabled: bool) {
        self.monitor_enabled = enabled;
        if let Some(engine) = &mut self.engine {
            engine.set_signal_monitor_enabled(enabled);
        }
    }
    pub fn drain_monitor(&mut self, samples: &mut Vec<SignalMonitorSample>) {
        if let Some(engine) = &mut self.engine {
            engine.drain_signal_monitor_samples(samples);
        }
    }
    /// Move blocking cleanup to the caller's background executor.
    pub fn take_shutdown(&mut self) -> Shutdown {
        self.stopping = true;
        Shutdown {
            engine: self.engine.take(),
            starting: self.engine_start.take(),
        }
    }
    pub fn metrics(&self) -> EngineMetrics {
        self.engine
            .as_ref()
            .map_or_else(EngineMetrics::default, |engine| engine.metrics().snapshot())
    }
    pub fn runtime_error(&self) -> Option<String> {
        self.error.clone().or_else(|| {
            self.engine
                .as_ref()
                .and_then(|engine| engine.metrics().last_error())
        })
    }
    pub fn refresh_processors(&mut self) {
        self.processors = self.backend.processors();
        let selection_is_valid = self
            .settings
            .processor_id
            .as_ref()
            .is_some_and(|id| self.processors.iter().any(|processor| processor.id() == id));
        if selection_is_valid {
            return;
        }

        let missing_selection = self.settings.processor_id.take();
        self.settings.processor_id = self
            .processors
            .iter()
            .find(|processor| !processor.is_gpu())
            .or_else(|| self.processors.first())
            .map(|processor| processor.id().to_owned());
        if missing_selection.is_some() {
            self.notice = Some(
                "The selected processor is no longer available; select an available processor."
                    .to_owned(),
            );
        }
    }
    pub fn refresh_devices(&mut self) {
        match self.backend.input_devices() {
            Ok(devices) => {
                self.devices = devices;
                let selection_is_valid = self
                    .settings
                    .input_device_id
                    .as_ref()
                    .is_some_and(|id| self.devices.iter().any(|device| device.id == *id));
                if !selection_is_valid {
                    self.settings.input_device_id = self
                        .devices
                        .iter()
                        .find(|device| device.is_default)
                        .or_else(|| self.devices.first())
                        .map(|device| device.id.clone());
                }
                self.error = if self.devices.is_empty() {
                    Some("No physical input microphone was found.".to_owned())
                } else {
                    None
                };
            }
            Err(error) => {
                self.devices.clear();
                self.error = Some(error.to_string());
            }
        }
    }
    pub fn selected_processor(&self) -> Option<&ComputeProcessor> {
        self.settings.processor_id.as_ref().and_then(|id| {
            self.processors
                .iter()
                .find(|processor| processor.id() == id)
        })
    }
    pub fn start(&mut self) {
        if self.busy() {
            return;
        }
        let Some(input_device_id) = self.settings.input_device_id.clone() else {
            self.error = Some("Select an input microphone first.".to_owned());
            return;
        };
        let config = EngineConfig::new(input_device_id);
        let processor = if self.settings.noise_reduction {
            let Some(processor) = self.selected_processor().cloned() else {
                self.error = Some("Select a compute processor first.".to_owned());
                return;
            };
            Some(processor.id().to_owned())
        } else {
            None
        };

        // Shutdown can wait for inference or the audio server. Keep it on the
        // startup thread so changing settings never joins audio from the GUI.
        let previous_engine = self.engine.take();
        let backend = Arc::clone(&self.backend);
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("noise-hoihoi-start".to_owned())
            .spawn(move || {
                drop(previous_engine);
                let result = backend.start(&config, processor.as_deref());
                let _ = sender.send(result);
            }) {
            Ok(worker) => {
                self.engine_start = Some(EngineStart { receiver, worker });
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Failed to start the audio initialization: {error}"));
            }
        }
    }
    pub fn poll_engine_start(&mut self) {
        let result = match self.engine_start.as_ref() {
            Some(starting) => match starting.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(EngineError::Start(
                    "audio initialization stopped unexpectedly".to_owned(),
                ))),
            },
            None => None,
        };
        let Some(result) = result else {
            return;
        };

        if let Some(starting) = self.engine_start.take() {
            let _ = starting.worker.join();
        }
        match result {
            Ok(mut engine) => {
                engine.set_signal_monitor_enabled(self.monitor_enabled);
                self.engine = Some(engine);
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}
impl<B: SessionBackend> Drop for Session<B> {
    fn drop(&mut self) {
        self.take_shutdown().finish();
    }
}

#[must_use = "finish this cleanup on a background thread"]
pub struct Shutdown {
    engine: Option<RunningAudioEngine>,
    starting: Option<EngineStart>,
}
impl Shutdown {
    pub fn finish(self) {
        drop(self);
    }
}
impl Drop for Shutdown {
    fn drop(&mut self) {
        self.engine.take();
        if let Some(starting) = self.starting.take() {
            // Disconnect before joining so a late route is dropped on the startup worker.
            drop(starting.receiver);
            let _ = starting.worker.join();
        }
    }
}
