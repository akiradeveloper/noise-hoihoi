use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use eframe::egui::{self, Color32, RichText};
use noise_hoihoi_engine::{
    AudioDevice, EngineConfig, EngineState, MetricsHandle, PassThrough, RunningAudioEngine,
    SignalMonitorSample, VB_CABLE_RECORDING_ENDPOINT_NAME, input_devices, start,
};
use serde::{Deserialize, Serialize};

use crate::signal_monitor::{self, SignalMonitorHistory};

const APP_NAME: &str = "NoiseHoiHoi";
const SETTINGS_KEY: &str = "noise-hoihoi-settings";
const WINDOW_WIDTH: f32 = 440.0;
const INITIAL_WINDOW_HEIGHT: f32 = 160.0;
const PANEL_MARGIN: f32 = 16.0;
const SIGNAL_MONITOR_TITLE: &str = "NoiseHoiHoi - Signal Monitor";
const SIGNAL_MONITOR_WIDTH: f32 = 760.0;
const SIGNAL_MONITOR_HEIGHT: f32 = 620.0;
const SIGNAL_MONITOR_VIEWPORT_ID: &str = "noise-hoihoi-signal-monitor";
const UI_REFRESH_INTERVAL: Duration = Duration::from_millis(50);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("NoiseHoiHoi")
            .with_inner_size([WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT])
            .with_resizable(false),
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
}

struct NoiseHoiHoiApp {
    settings: Settings,
    devices: Vec<AudioDevice>,
    engine: Option<RunningAudioEngine>,
    error: Option<String>,
    window_size: Option<egui::Vec2>,
    signal_monitor_open: Arc<AtomicBool>,
    signal_monitor_enabled: bool,
    signal_history: Arc<Mutex<SignalMonitorHistory>>,
    pending_signal_samples: Vec<SignalMonitorSample>,
}

impl NoiseHoiHoiApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        let settings = context
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        let mut app = Self {
            settings,
            devices: Vec::new(),
            engine: None,
            error: None,
            window_size: None,
            signal_monitor_open: Arc::new(AtomicBool::new(false)),
            signal_monitor_enabled: false,
            signal_history: Arc::new(Mutex::new(SignalMonitorHistory::default())),
            pending_signal_samples: Vec::new(),
        };
        app.refresh_devices();
        if app.settings.input_device_id.is_some() {
            app.start();
        }
        app
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

    fn start(&mut self) {
        let Some(input_device_id) = self.settings.input_device_id.clone() else {
            self.error = Some("Select an input microphone first.".to_owned());
            return;
        };
        let config = EngineConfig::new(input_device_id);
        match start(&config, PassThrough) {
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

    fn draw_controls(&mut self, ui: &mut egui::Ui) {
        let selection_before = self.settings.input_device_id.clone();
        let mut refresh_requested = false;
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
            refresh_requested = ui.button("Refresh").clicked();
        });

        if refresh_requested {
            self.stop();
            self.refresh_devices();
            if self.settings.input_device_id.is_some() {
                self.start();
            }
        } else if selection_before != self.settings.input_device_id {
            self.stop();
            self.start();
        }

        ui.horizontal(|ui| {
            ui.label("Output");
            ui.label(VB_CABLE_RECORDING_ENDPOINT_NAME);
        });
        ui.horizontal(|ui| {
            ui.label("Noise reduction");
            let mut reduction = false;
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut reduction, "Off (available in v0.3)"),
            );
        });
    }

    fn draw_status(&mut self, ui: &mut egui::Ui) {
        let metrics_handle = self.engine.as_ref().map(RunningAudioEngine::metrics);
        let metrics = metrics_handle.as_ref().map(MetricsHandle::snapshot);
        let runtime_error = metrics_handle.as_ref().and_then(MetricsHandle::last_error);
        let (status, status_color) = match metrics.map(|value| value.state) {
            Some(EngineState::Starting) => ("Starting", Color32::YELLOW),
            Some(EngineState::Running) => ("Running", Color32::LIGHT_GREEN),
            Some(EngineState::Faulted) => ("Audio error", Color32::LIGHT_RED),
            _ => ("Stopped", Color32::GRAY),
        };
        let can_retry = self.engine.is_none()
            || metrics.is_some_and(|value| value.state == EngineState::Faulted);
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
                self.stop();
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
            ui.colored_label(Color32::LIGHT_RED, error);
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
        let monitor_is_running = metrics.is_some();
        context.show_viewport_deferred(
            signal_monitor_viewport_id(),
            egui::ViewportBuilder::default()
                .with_title(SIGNAL_MONITOR_TITLE)
                .with_inner_size([SIGNAL_MONITOR_WIDTH, SIGNAL_MONITOR_HEIGHT])
                .with_min_inner_size([560.0, 420.0])
                .with_resizable(true),
            move |ui, _class| {
                let close_requested = ui.input(|input| input.viewport().close_requested());
                if close_requested {
                    open.store(false, Ordering::Release);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                    return;
                }

                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        signal_monitor::draw(ui, &history, metrics.as_ref());
                    });
                });
                if monitor_is_running {
                    ui.ctx().request_repaint_after(UI_REFRESH_INTERVAL);
                }
            },
        );
    }
}

impl eframe::App for NoiseHoiHoiApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_signal_monitor_state();
        self.collect_signal_samples();
        if self.engine.is_some() {
            context.request_repaint_after(UI_REFRESH_INTERVAL);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let panel = egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical(|ui| {
                self.draw_controls(ui);
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                self.draw_status(ui);
                self.draw_signal_monitor_button(ui);
            })
        });

        let content_height = panel.inner.response.rect.height();
        let desired_size = egui::vec2(WINDOW_WIDTH, (content_height + PANEL_MARGIN).ceil());
        if self.window_size != Some(desired_size) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
            self.window_size = Some(desired_size);
        }

        self.show_signal_monitor(&context);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.signal_monitor_open.store(false, Ordering::Release);
        self.stop();
    }
}

fn signal_monitor_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(SIGNAL_MONITOR_VIEWPORT_ID)
}
