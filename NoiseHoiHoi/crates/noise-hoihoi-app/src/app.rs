use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crate::signal_monitor::{self, SignalMonitor, SignalMonitorHistory};
use gpui_kit::component::{
    ActiveTheme, Disableable as _, IndexPath, Root, StyledExt as _, TitleBar,
    button::Button,
    select::{Select, SelectEvent, SelectItem, SelectState},
    switch::Switch,
};
use gpui_kit::{
    App, AppContext as _, Bounds, ClipboardItem, Context, Entity, Image, ImageFormat, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Task, TitlebarOptions, Window,
    WindowBounds, WindowDecorations, WindowHandle, WindowOptions, div, img, prelude::*, px,
    relative, size,
};
use noise_hoihoi_engine::{EngineMetrics, EngineState, SignalMonitorSample};
use noise_hoihoi_platform::NativeBackend;
use noise_hoihoi_session::{
    Session,
    performance::{PerformanceCheck, Verdict},
};

const UI_REFRESH_INTERVAL: Duration = Duration::from_millis(50);
const APP_ICON: &[u8] = include_bytes!("../../../../assets/NoiseHoiHoi.png");
type DeviceSelect = Entity<SelectState<Vec<DeviceChoice>>>;

pub fn run() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let options = window_options("NoiseHoiHoi", 440.0, 640.0, cx);
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| NoiseHoiHoiApp::new(window, cx));
                let weak = view.downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = weak.update(cx, NoiseHoiHoiApp::shutdown);
                    false
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Could not open the NoiseHoiHoi window");
            cx.activate(true);
        });
}

pub(super) fn window_options(title: &str, width: f32, height: f32, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.to_owned().into()),
            ..TitleBar::title_bar_options()
        }),
        app_id: Some("NoiseHoiHoi".into()),
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(size(px(width.min(360.0)), px(300.0))),
        icon: Some(std::sync::Arc::new(
            image::load_from_memory(APP_ICON)
                .expect("bundled application icon")
                .into_rgba8(),
        )),
        ..Default::default()
    }
}

pub(super) fn window_title(title: &'static str) -> impl IntoElement {
    static ICON: OnceLock<Arc<Image>> = OnceLock::new();
    let icon =
        ICON.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, APP_ICON.to_vec())));
    div()
        .h_flex()
        .gap_2()
        .child(img(icon.clone()).size(px(16.0)).flex_shrink_0())
        .child(title)
}

#[derive(Clone)]
struct DeviceChoice {
    id: String,
    label: SharedString,
}

impl SelectItem for DeviceChoice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &String {
        &self.id
    }
}

struct NoiseHoiHoiApp {
    session: Session<NativeBackend>,
    input_select: DeviceSelect,
    processor_select: DeviceSelect,
    monitor: Option<WindowHandle<Root>>,
    signal_view: Option<Entity<SignalMonitor>>,
    signal_history: SignalMonitorHistory,
    pending_signal_samples: Vec<SignalMonitorSample>,
    shutting_down: bool,
    performance: PerformanceCheck,
    subscriptions: Vec<Subscription>,
    refresh: Option<Task<()>>,
}

impl NoiseHoiHoiApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_select = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let processor_select = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let mut app = Self {
            session: Session::new(NativeBackend),
            input_select,
            processor_select,
            monitor: None,
            signal_view: None,
            signal_history: SignalMonitorHistory::default(),
            pending_signal_samples: Vec::new(),
            shutting_down: false,
            performance: PerformanceCheck::default(),
            subscriptions: Vec::new(),
            refresh: None,
        };
        app.sync_choices(window, cx);
        app.subscriptions.push(cx.subscribe_in(
            &app.input_select,
            window,
            |app, _, event, _, cx| {
                let SelectEvent::Confirm(value) = event;
                if app.session.settings.input_device_id != *value && !app.busy() {
                    app.session.settings.input_device_id.clone_from(value);
                    app.apply_settings(cx);
                }
            },
        ));
        app.subscriptions.push(cx.subscribe_in(
            &app.processor_select,
            window,
            |app, _, event, _, cx| {
                let SelectEvent::Confirm(value) = event;
                if app.session.settings.processor_id != *value && !app.busy() {
                    app.session.settings.processor_id.clone_from(value);
                    app.session.notice = None;
                    app.apply_settings(cx);
                }
            },
        ));
        // OS/session quit also tears down the route when no window close callback runs.
        app.subscriptions.push(cx.on_app_quit(|app, cx| {
            let shutdown = app.session.take_shutdown();
            cx.background_executor().spawn(async move {
                shutdown.finish();
            })
        }));
        app.refresh = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(UI_REFRESH_INTERVAL).await;
                if this.update(cx, NoiseHoiHoiApp::tick).is_err() {
                    break;
                }
            }
        }));
        if app.session.settings.input_device_id.is_some() {
            app.session.start();
        }
        app
    }

    fn sync_choices(&self, window: &mut Window, cx: &mut App) {
        let inputs = self
            .session
            .devices
            .iter()
            .map(|device| DeviceChoice {
                id: device.id.clone(),
                label: if device.is_default {
                    format!("{} (Default)", device.name)
                } else {
                    device.name.clone()
                }
                .into(),
            })
            .collect();
        let processors = self
            .session
            .processors
            .iter()
            .map(|processor| DeviceChoice {
                id: processor.id().to_owned(),
                label: processor.name().to_owned().into(),
            })
            .collect();
        set_choices(
            &self.input_select,
            inputs,
            self.session.settings.input_device_id.as_ref(),
            window,
            cx,
        );
        set_choices(
            &self.processor_select,
            processors,
            self.session.settings.processor_id.as_ref(),
            window,
            cx,
        );
    }

    fn busy(&self) -> bool {
        self.session.busy() || self.shutting_down || self.performance.is_running()
    }

    fn apply_settings(&mut self, cx: &mut Context<Self>) {
        self.performance.clear();
        self.session.apply_settings();
        self.signal_history.clear();
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.shutting_down {
            return;
        }
        let was_checking = self.performance.is_running();
        self.performance.poll(Instant::now(), self.metrics());
        if was_checking {
            cx.notify();
        }
        let was_starting = self.session.starting();
        self.session.poll_engine_start();
        if was_starting && !self.session.starting() {
            self.signal_history.clear();
        }
        if self.monitor.is_some_and(|handle| handle.read(cx).is_err()) {
            self.monitor = None;
            self.signal_view = None;
            self.session.set_monitor_enabled(false);
            self.signal_history.clear();
        }
        if let Some(view) = &self.signal_view {
            self.pending_signal_samples.clear();
            self.session.drain_monitor(&mut self.pending_signal_samples);
            self.signal_history.append(&self.pending_signal_samples);
            let metrics = self.metrics();
            let error = self.runtime_error();
            view.update(cx, |monitor, cx| {
                monitor.update(&self.signal_history, metrics, error);
                cx.notify();
            });
        }
        if self.session.has_engine() || self.session.starting() || was_starting {
            cx.notify();
        }
    }

    fn metrics(&self) -> EngineMetrics {
        self.session.metrics()
    }

    fn runtime_error(&self) -> Option<String> {
        self.session.runtime_error()
    }

    fn open_monitor(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.monitor
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let options = window_options("NoiseHoiHoi - Signal Monitor", 760.0, 680.0, cx);
        let view = cx.new(|_| SignalMonitor::default());
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| Root::new(view.clone(), window, cx))
        }) {
            Ok(handle) => {
                self.monitor = Some(handle);
                self.signal_view = Some(view);
                self.signal_history.clear();
                self.session.set_monitor_enabled(true);
            }
            Err(error) => {
                self.session.error = Some(format!("Could not open the signal monitor: {error}"));
            }
        }
        cx.notify();
    }

    fn check_performance(&mut self, cx: &mut Context<Self>) {
        if self.performance.is_running() {
            self.performance.clear();
        } else if !self.busy()
            && self.session.settings.noise_reduction
            && let Some(processor) = self.session.selected_processor()
        {
            let combination = processor.name().to_owned();
            self.performance
                .start(Instant::now(), self.metrics(), combination);
        }
        cx.notify();
    }

    fn render_performance(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let checking = self.performance.is_running();
        let can_start = !self.busy()
            && self.session.settings.noise_reduction
            && self.session.selected_processor().is_some()
            && self.metrics().state == EngineState::Running;
        let progress = self.performance.remaining(Instant::now()).map(|remaining| {
            format!(
                "Observing live audio… {} s remaining",
                remaining.as_secs().saturating_add(1)
            )
        });
        let report = self.performance.report();
        div().v_flex().gap_2().border_t_1().border_color(cx.theme().border).pt_4()
            .child(Button::new("performance-check")
                .label(if checking { "Cancel check" } else { "Check performance (10 s)" })
                .w_full()
                .disabled(self.shutting_down || (!checking && !can_start))
                .on_click(cx.listener(|app, _, _, cx| app.check_performance(cx))))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(
                if self.session.settings.noise_reduction {
                    "Checks the active microphone and processor under the current load. Audio continues normally."
                } else {
                    "Enable noise reduction and start the microphone to check performance."
                }))
            .when_some(progress, |panel, text| panel.child(div().text_xs().child(text)))
            .when_some(report, |panel, report| {
                let color = match report.verdict {
                    Verdict::Headroom => cx.theme().success,
                    Verdict::Limited | Verdict::InsufficientData => cx.theme().warning,
                    Verdict::Interruptions | Verdict::TooSlow => cx.theme().danger,
                };
                let text = report.text();
                panel
                    .child(div().font_semibold().text_color(color).child(report.title()))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(report.combination.clone()))
                    .child(div().text_xs().child(report.details()))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child("Performance at the current load; noise-removal quality is not assessed."))
                    .child(Button::new("copy-performance-report").label("Copy result")
                        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))))
            })
    }

    fn shutdown(&mut self, cx: &mut Context<Self>) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        self.performance.clear();
        self.refresh = None;
        if let Some(handle) = self.monitor.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.signal_view = None;
        let shutdown = self.session.take_shutdown();
        let stop = cx.background_executor().spawn(async move {
            shutdown.finish();
        });
        cx.spawn(async move |_, cx| {
            stop.await;
            cx.update(|cx| cx.quit());
        })
        .detach();
        cx.notify();
    }
}

fn set_choices(
    select: &DeviceSelect,
    choices: Vec<DeviceChoice>,
    selected: Option<&String>,
    window: &mut Window,
    cx: &mut App,
) {
    let index = choices
        .iter()
        .position(|choice| Some(&choice.id) == selected);
    select.update(cx, |state, cx| {
        state.set_items(choices, window, cx);
        state.set_selected_index(index.map(|row| IndexPath::default().row(row)), window, cx);
    });
}

impl Render for NoiseHoiHoiApp {
    #[allow(
        clippy::too_many_lines,
        reason = "declarative layout for the single control panel"
    )]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let metrics = self.metrics();
        let (status, color) = if self.shutting_down {
            ("Stopping…", cx.theme().warning)
        } else if self.session.starting() {
            ("Starting…", cx.theme().warning)
        } else {
            signal_monitor::state_label(metrics.state, cx)
        };
        let busy = self.busy();
        let can_retry =
            !busy && (!self.session.has_engine() || metrics.state == EngineState::Faulted);
        let error = self.runtime_error();
        let content = div()
            .id("main-scroll")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(
                div()
                    .v_flex()
                    .p_5()
                    .gap_4()
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .child(div().font_semibold().child("NoiseHoiHoi"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                            ),
                    )
                    .child(
                        div().v_flex().gap_2().child("Input microphone").child(
                            Select::new(&self.input_select)
                                .w_full()
                                .disabled(busy)
                                .placeholder("Select a microphone")
                                .accessibility_label("Input microphone"),
                        ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .justify_between()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Input level")
                                    .child(signal_monitor::dbfs_text(metrics.input_peak)),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(6.0))
                                    .rounded_sm()
                                    .bg(cx.theme().muted)
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(level_fraction(metrics.input_peak)))
                                            .rounded_sm()
                                            .bg(cx.theme().success),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .pt_4()
                            .child(
                                Switch::new("noise-reduction")
                                    .label("Noise reduction")
                                    .checked(self.session.settings.noise_reduction)
                                    .disabled(busy)
                                    .on_click(cx.listener(|app, checked, _, cx| {
                                        if !app.busy() {
                                            app.session.settings.noise_reduction = *checked;
                                            app.apply_settings(cx);
                                        }
                                    })),
                            ),
                    )
                    .when(self.session.settings.noise_reduction, |panel| {
                        panel.child(
                            div().v_flex().gap_2().child("Processor").child(
                                Select::new(&self.processor_select)
                                    .w_full()
                                    .disabled(busy)
                                    .placeholder("Select a processor")
                                    .accessibility_label("Compute processor"),
                            ),
                        )
                    })
                    .when(!self.session.settings.noise_reduction, |panel| {
                        panel.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Pass-through · audio is forwarded unchanged"),
                        )
                    })
                    .child(
                        div()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .pt_4()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Output microphone"),
                            )
                            .child(self.session.output_name().to_owned()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().text_color(color).child(status))
                            .when(can_retry, |row| {
                                row.child(Button::new("retry").label("Retry").on_click(
                                    cx.listener(|app, _, window, cx| {
                                        app.performance.clear();
                                        app.session.refresh_devices();
                                        app.session.refresh_processors();
                                        app.sync_choices(window, cx);
                                        app.session.start();
                                        cx.notify();
                                    }),
                                ))
                            }),
                    )
                    .when_some(error, |panel, error| {
                        panel.child(div().text_color(cx.theme().danger).child(error))
                    })
                    .when_some(self.session.notice.clone(), |panel, notice| {
                        panel.child(div().text_xs().text_color(cx.theme().warning).child(notice))
                    })
                    .child(self.render_performance(cx))
                    .child(
                        div()
                            .w_full()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .pt_4()
                            .child(
                                Button::new("signal-monitor")
                                    .label("Signal Monitor")
                                    .w_full()
                                    .disabled(self.shutting_down)
                                    .on_click(cx.listener(|app, _, _, cx| app.open_monitor(cx))),
                            ),
                    ),
            );
        div()
            .v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new()
                    .child(window_title("NoiseHoiHoi"))
                    .on_close_window(cx.listener(|app, _, _, cx| app.shutdown(cx))),
            )
            .child(content)
    }
}

fn level_fraction(peak: f32) -> f32 {
    if peak > 0.0 {
        ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}
