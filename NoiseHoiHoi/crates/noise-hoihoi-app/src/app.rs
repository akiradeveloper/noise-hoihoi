use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::JoinHandle,
    time::Duration,
};

use eframe::egui::{self, Color32, RichText};
use noise_hoihoi_engine::{
    AudioDevice, ComputeProcessor, EngineConfig, EngineError, EngineState, MetricsHandle,
    NoiseReduction, OUTPUT_MICROPHONE_NAME, PassThrough, RunningAudioEngine, SignalMonitorSample,
    compute_processors, input_devices, start,
};
use serde::{Deserialize, Serialize};

use crate::signal_monitor::{self, SignalMonitorHistory};

const APP_NAME: &str = "NoiseHoiHoi";
const SETTINGS_KEY: &str = "noise-hoihoi-settings";
const INITIAL_LAYOUT_WIDTH: f32 = 440.0;
const PANEL_MARGIN: i8 = 8;
const SIGNAL_MONITOR_TITLE: &str = "NoiseHoiHoi - Signal Monitor";
const SIGNAL_MONITOR_WIDTH: f32 = 760.0;
const SIGNAL_MONITOR_HEIGHT: f32 = 620.0;
const SIGNAL_MONITOR_VIEWPORT_ID: &str = "noise-hoihoi-signal-monitor";
const UI_REFRESH_INTERVAL: Duration = Duration::from_millis(50);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        // On Wayland, waiting for swap on a hidden/occluded monitor viewport
        // can block the single GUI event loop. Repaints are already timed.
        // https://docs.rs/glutin/0.32.3/glutin/surface/enum.SwapInterval.html
        #[cfg(target_os = "linux")]
        glow_options: egui_glow::GlowConfiguration {
            vsync: false,
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default()
            .with_app_id("NoiseHoiHoi")
            .with_icon(app_icon())
            // Bootstrap the first layout; subsequent sizes come from its contents.
            .with_inner_size([INITIAL_LAYOUT_WIDTH, 160.0])
            .with_resizable(true),
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|context| Ok(Box::new(NoiseHoiHoiApp::new(context)))),
    )
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    input_device_id: Option<String>,
    #[serde(default)]
    noise_reduction: bool,
    #[serde(default)]
    processor_id: Option<String>,
}

struct NoiseHoiHoiApp {
    settings: Settings,
    devices: Vec<AudioDevice>,
    processors: Vec<ComputeProcessor>,
    engine: Option<RunningAudioEngine>,
    engine_start: Option<EngineStart>,
    error: Option<String>,
    notice: Option<String>,
    content_size: Option<egui::Vec2>,
    resize_attempts: u8,
    signal_monitor_open: Arc<AtomicBool>,
    signal_monitor_enabled: bool,
    signal_history: Arc<Mutex<SignalMonitorHistory>>,
    pending_signal_samples: Vec<SignalMonitorSample>,
}

struct EngineStart {
    receiver: Receiver<Result<RunningAudioEngine, EngineError>>,
    worker: JoinHandle<()>,
}

impl NoiseHoiHoiApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_theme(egui::Theme::Light);
        context.egui_ctx.set_visuals_of(
            egui::Theme::Light,
            egui::Visuals {
                weak_text_color: Some(Color32::from_gray(80)),
                warn_fg_color: Color32::from_rgb(135, 85, 0),
                error_fg_color: Color32::from_rgb(180, 35, 45),
                ..egui::Visuals::light()
            },
        );
        let settings = context
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        let mut app = Self {
            settings,
            devices: Vec::new(),
            processors: Vec::new(),
            engine: None,
            engine_start: None,
            error: None,
            notice: None,
            content_size: None,
            resize_attempts: 0,
            signal_monitor_open: Arc::new(AtomicBool::new(false)),
            signal_monitor_enabled: false,
            signal_history: Arc::new(Mutex::new(SignalMonitorHistory::default())),
            pending_signal_samples: Vec::new(),
        };
        app.refresh_devices();
        app.refresh_processors();
        if app.settings.input_device_id.is_some() {
            app.start();
        }
        app
    }

    fn refresh_processors(&mut self) {
        self.processors = compute_processors();
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
                "The selected processor is no longer available; using the CPU instead.".to_owned(),
            );
        }
    }

    fn refresh_devices(&mut self) {
        match input_devices() {
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

    fn selected_name(&self) -> &str {
        self.settings
            .input_device_id
            .as_ref()
            .and_then(|id| self.devices.iter().find(|device| device.id == *id))
            .map_or("Select a microphone", |device| device.name.as_str())
    }

    fn selected_processor(&self) -> Option<&ComputeProcessor> {
        self.settings.processor_id.as_ref().and_then(|id| {
            self.processors
                .iter()
                .find(|processor| processor.id() == id)
        })
    }

    fn selected_processor_name(&self) -> &str {
        self.selected_processor()
            .map_or("Select a processor", ComputeProcessor::name)
    }

    fn start(&mut self) {
        if self.engine_start.is_some() {
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
            Some(processor)
        } else {
            None
        };

        // Shutdown can wait for inference or the audio server. Keep it on the
        // startup thread so changing settings never joins audio from the GUI.
        let previous_engine = self.engine.take();
        self.clear_signal_history();
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("noise-hoihoi-start".to_owned())
            .spawn(move || {
                drop(previous_engine);
                let result = if let Some(processor) = processor {
                    NoiseReduction::new(&processor, processor.default_runtime())
                        .and_then(|processor| start(&config, processor))
                } else {
                    start(&config, PassThrough)
                };
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

    fn poll_engine_start(&mut self) {
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
                let monitor_open = self.signal_monitor_open.load(Ordering::Acquire);
                engine.set_signal_monitor_enabled(monitor_open);
                self.engine = Some(engine);
                self.signal_monitor_enabled = monitor_open;
                self.clear_signal_history();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn stop(&mut self) {
        if let Some(engine) = self.engine.take() {
            engine.stop();
        }
        self.clear_signal_history();
    }

    fn finish_pending_start(&mut self) {
        if let Some(starting) = self.engine_start.take() {
            drop(starting.receiver);
            let _ = starting.worker.join();
        }
    }

    fn draw_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(self.engine_start.is_none(), |ui| {
            self.draw_enabled_controls(ui);
        });
    }

    fn draw_enabled_controls(&mut self, ui: &mut egui::Ui) {
        let selection_before = self.settings.input_device_id.clone();
        let reduction_before = self.settings.noise_reduction;
        let processor_before = self.settings.processor_id.clone();
        ui.horizontal(|ui| {
            ui.label("Input");
            egui::ComboBox::from_id_salt("input-device")
                .selected_text(self.selected_name())
                .width(285.0)
                .show_ui(ui, |ui| {
                    for device in &self.devices {
                        let label = if device.is_default {
                            format!("{} (Default)", device.name)
                        } else {
                            device.name.clone()
                        };
                        ui.selectable_value(
                            &mut self.settings.input_device_id,
                            Some(device.id.clone()),
                            label,
                        );
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Output");
            ui.label(OUTPUT_MICROPHONE_NAME);
        });
        ui.horizontal(|ui| {
            ui.label("Noise reduction");
            ui.checkbox(&mut self.settings.noise_reduction, "Enabled");
        });
        if self.settings.noise_reduction {
            ui.horizontal(|ui| {
                ui.label("Processor");
                egui::ComboBox::from_id_salt("compute-processor")
                    .selected_text(self.selected_processor_name())
                    .width(285.0)
                    .show_ui(ui, |ui| {
                        for processor in &self.processors {
                            ui.selectable_value(
                                &mut self.settings.processor_id,
                                Some(processor.id().to_owned()),
                                processor.name(),
                            );
                        }
                    });
            });
            if let Some(processor) = self
                .selected_processor()
                .filter(|processor| processor.is_gpu())
            {
                ui.horizontal(|ui| {
                    ui.label("Runtime");
                    let mut runtime = processor.default_runtime();
                    egui::ComboBox::from_id_salt("compute-runtime")
                        .selected_text(runtime.to_string())
                        .width(285.0)
                        .show_ui(ui, |ui| {
                            for &available_runtime in processor.runtimes() {
                                ui.selectable_value(
                                    &mut runtime,
                                    available_runtime,
                                    available_runtime.to_string(),
                                );
                            }
                        });
                });
            }
        }

        if selection_before != self.settings.input_device_id
            || reduction_before != self.settings.noise_reduction
            || processor_before != self.settings.processor_id
        {
            if processor_before != self.settings.processor_id {
                self.notice = None;
            }
            self.start();
        }
    }

    fn draw_status(&mut self, ui: &mut egui::Ui) {
        let metrics_handle = self.engine.as_ref().map(RunningAudioEngine::metrics);
        let metrics = metrics_handle.as_ref().map(MetricsHandle::snapshot);
        let runtime_error = metrics_handle.as_ref().and_then(MetricsHandle::last_error);
        let (status, status_color) = match (
            self.engine_start.is_some(),
            metrics.map(|value| value.state),
        ) {
            (true, _) | (false, Some(EngineState::Starting)) => {
                ("Starting", ui.visuals().warn_fg_color)
            }
            (false, Some(EngineState::Running)) => ("Running", Color32::from_rgb(25, 110, 65)),
            (false, Some(EngineState::Faulted)) => ("Audio error", ui.visuals().error_fg_color),
            _ => ("Stopped", ui.visuals().weak_text_color()),
        };
        let can_retry = self.engine_start.is_none()
            && (self.engine.is_none()
                || metrics.is_some_and(|value| value.state == EngineState::Faulted));
        ui.horizontal(|ui| {
            ui.label("Status");
            ui.label(RichText::new(status).color(status_color).strong());
            if can_retry
                && ui
                    .add_enabled(
                        self.settings.input_device_id.is_some(),
                        egui::Button::new("Retry"),
                    )
                    .clicked()
            {
                self.start();
            }
        });

        let peak = metrics.map_or(0.0, |value| value.input_peak);
        let peak_db = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            -60.0
        };
        ui.horizontal(|ui| {
            ui.label("Input level");
            ui.add(
                egui::ProgressBar::new(peak)
                    .desired_width(280.0)
                    .text(format!("{peak_db:.1} dBFS")),
            );
        });
        if let Some(error) = self.error.as_ref().or(runtime_error.as_ref()) {
            ui.add_space(8.0);
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else if let Some(notice) = &self.notice {
            ui.add_space(8.0);
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
    }

    fn draw_signal_monitor_button(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        if ui.button("Signal monitor…").clicked() {
            if self.signal_monitor_open.load(Ordering::Acquire) {
                ui.ctx().send_viewport_cmd_to(
                    signal_monitor_viewport_id(),
                    egui::ViewportCommand::Focus,
                );
            } else {
                self.set_signal_monitor_open(true);
            }
        }
    }

    fn draw_panel(&mut self, ui: &mut egui::Ui) -> egui::Vec2 {
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(PANEL_MARGIN))
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Separators stretch to the viewport, so measure the widgets separately.
                        let controls = ui.vertical(|ui| self.draw_controls(ui));
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);
                        let status = ui.vertical(|ui| self.draw_status(ui));
                        self.draw_signal_monitor_button(ui);
                        controls
                            .response
                            .rect
                            .width()
                            .max(status.response.rect.width())
                    })
            });
        egui::vec2(panel.inner.inner, panel.inner.content_size.y)
            + egui::Vec2::splat(2.0 * f32::from(PANEL_MARGIN))
    }

    fn resize_to_content(&mut self, context: &egui::Context, content_size: egui::Vec2) {
        let (actual_size, monitor_size, maximized) = context.input(|input| {
            let viewport = input.viewport();
            (
                input.content_rect().size(),
                viewport.monitor_size,
                viewport.maximized == Some(true) || viewport.fullscreen == Some(true),
            )
        });
        // Leave room for window decorations and the desktop panel on small displays.
        let limit = monitor_size.map_or(egui::Vec2::INFINITY, |size| {
            (size - egui::Vec2::splat(64.0)).max(egui::Vec2::splat(1.0))
        });
        let desired_size = content_size.ceil().min(limit);
        if self.content_size != Some(desired_size) {
            self.content_size = Some(desired_size);
            self.resize_attempts = 3;
        }
        if maximized || (actual_size - desired_size).abs().max_elem() < 1.0 {
            self.resize_attempts = 0;
        } else if self.resize_attempts > 0 {
            // Verify the actual viewport on later frames; a resize request is asynchronous.
            context.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
            self.resize_attempts -= 1;
            context.request_repaint_after(UI_REFRESH_INTERVAL);
        }
    }

    fn set_signal_monitor_open(&mut self, open: bool) {
        self.signal_monitor_open.store(open, Ordering::Release);
        self.sync_signal_monitor_state();
    }

    fn sync_signal_monitor_state(&mut self) {
        let requested = self.signal_monitor_open.load(Ordering::Acquire);
        if requested == self.signal_monitor_enabled {
            return;
        }

        if let Some(engine) = self.engine.as_mut() {
            engine.set_signal_monitor_enabled(requested);
        }
        self.signal_monitor_enabled = requested;
        self.clear_signal_history();
    }

    fn collect_signal_samples(&mut self) {
        if !self.signal_monitor_enabled {
            return;
        }
        let Some(engine) = self.engine.as_mut() else {
            return;
        };

        self.pending_signal_samples.clear();
        engine.drain_signal_monitor_samples(&mut self.pending_signal_samples);
        if self.pending_signal_samples.is_empty() {
            return;
        }
        self.signal_history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .append(&self.pending_signal_samples);
    }

    fn clear_signal_history(&mut self) {
        self.pending_signal_samples.clear();
        self.signal_history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn show_signal_monitor(&self, context: &egui::Context) {
        if !self.signal_monitor_open.load(Ordering::Acquire) {
            return;
        }

        let open = Arc::clone(&self.signal_monitor_open);
        let history = Arc::clone(&self.signal_history);
        let metrics = self.engine.as_ref().map(RunningAudioEngine::metrics);
        context.show_viewport_deferred(
            signal_monitor_viewport_id(),
            egui::ViewportBuilder::default()
                .with_title(SIGNAL_MONITOR_TITLE)
                .with_icon(app_icon())
                .with_inner_size([SIGNAL_MONITOR_WIDTH, SIGNAL_MONITOR_HEIGHT])
                .with_min_inner_size([560.0, 420.0])
                .with_resizable(true),
            move |ui, _class| {
                let close_requested = ui.input(|input| input.viewport().close_requested());
                if close_requested {
                    open.store(false, Ordering::Release);
                    // `Close` only queues another close request for a child
                    // viewport. Hide it now; the next root pass stops declaring
                    // the viewport and lets eframe destroy it.
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                    return;
                }

                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        signal_monitor::draw(ui, &history, metrics.as_ref());
                    });
                });
                // During a restart this callback can temporarily have no engine.
                // Keep repainting so the viewport adopts the next callback with
                // fresh metrics after startup completes, even without mouse input.
                ui.ctx().request_repaint_after(UI_REFRESH_INTERVAL);
            },
        );
    }
}

impl eframe::App for NoiseHoiHoiApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_engine_start();
        self.sync_signal_monitor_state();
        self.collect_signal_samples();
        if self.engine.is_some() || self.engine_start.is_some() {
            context.request_repaint_after(UI_REFRESH_INTERVAL);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let content_size = self.draw_panel(ui);
        self.resize_to_content(&context, content_size);

        self.show_signal_monitor(&context);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.signal_monitor_open.store(false, Ordering::Release);
        self.stop();
        self.finish_pending_start();
    }
}

fn app_icon() -> Arc<egui::IconData> {
    static ICON: std::sync::OnceLock<Arc<egui::IconData>> = std::sync::OnceLock::new();
    Arc::clone(ICON.get_or_init(|| {
        Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!("../../../../assets/NoiseHoiHoi.png"))
                .expect("the bundled NoiseHoiHoi icon must be a valid PNG"),
        )
    }))
}

fn signal_monitor_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(SIGNAL_MONITOR_VIEWPORT_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_without_audio() -> NoiseHoiHoiApp {
        NoiseHoiHoiApp {
            settings: Settings::default(),
            devices: Vec::new(),
            processors: Vec::new(),
            engine: None,
            engine_start: None,
            error: None,
            notice: None,
            content_size: None,
            resize_attempts: 0,
            signal_monitor_open: Arc::new(AtomicBool::new(false)),
            signal_monitor_enabled: false,
            signal_history: Arc::new(Mutex::new(SignalMonitorHistory::default())),
            pending_signal_samples: Vec::new(),
        }
    }

    fn measure(app: &mut NoiseHoiHoiApp, viewport_size: egui::Vec2) -> egui::Vec2 {
        let context = egui::Context::default();
        let mut content_size = egui::Vec2::ZERO;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size)),
                ..Default::default()
            },
            |ui| content_size = app.draw_panel(ui),
        );
        output.textures_delta.clear();
        content_size.ceil()
    }

    #[test]
    fn content_size_does_not_include_unused_viewport_space() {
        let mut app = app_without_audio();
        let compact = measure(&mut app, egui::vec2(INITIAL_LAYOUT_WIDTH, 160.0));
        let expanded = measure(&mut app, egui::vec2(1000.0, 800.0));
        assert_eq!(compact, expanded);
        assert!(compact.x < INITIAL_LAYOUT_WIDTH);
        assert!(compact.y > 160.0);
    }

    #[test]
    fn content_height_grows_and_shrinks_with_processor_controls() {
        let mut app = app_without_audio();
        let viewport = egui::vec2(INITIAL_LAYOUT_WIDTH, 600.0);
        let pass_through = measure(&mut app, viewport);
        app.settings.noise_reduction = true;
        let reduction = measure(&mut app, viewport);
        assert!(reduction.y > pass_through.y);
        app.settings.noise_reduction = false;
        assert_eq!(measure(&mut app, viewport), pass_through);
    }
}
