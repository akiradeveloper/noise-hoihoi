use std::{collections::VecDeque, sync::Mutex};

use eframe::egui::{self, Color32, RichText, Stroke, StrokeKind};
use noise_hoihoi_engine::{
    EngineMetrics, EngineState, MetricsHandle, PIPELINE_SAMPLE_RATE, SignalMonitorSample,
};

const HISTORY_FRAMES: usize = PIPELINE_SAMPLE_RATE as usize;
const PLOT_HEIGHT: f32 = 112.0;
const MIN_MAIN_SCALE: f32 = 0.01;
const MIN_DIFFERENCE_SCALE: f32 = 0.001;

#[derive(Clone, Debug, Default)]
pub(super) struct SignalMonitorHistory {
    samples: VecDeque<SignalMonitorSample>,
}

impl SignalMonitorHistory {
    pub(super) fn append(&mut self, samples: &[SignalMonitorSample]) {
        for &sample in samples {
            if self.samples.len() == HISTORY_FRAMES {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    pub(super) fn clear(&mut self) {
        self.samples.clear();
    }

    fn stats(&self) -> MonitorStats {
        let mut stats = MonitorStats::default();
        for &sample in &self.samples {
            stats.input.add(sample.input);
            stats.output.add(sample.output);
            stats.difference.add(sample.difference());
        }
        stats.finish(self.samples.len())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SignalStats {
    peak: f32,
    square_sum: f32,
    rms: f32,
}

impl SignalStats {
    fn add(&mut self, sample: f32) {
        self.peak = self.peak.max(sample.abs());
        self.square_sum += sample * sample;
    }

    fn finish(mut self, sample_count: usize) -> Self {
        if let Ok(sample_count) = u16::try_from(sample_count)
            && sample_count != 0
        {
            self.rms = (self.square_sum / f32::from(sample_count)).sqrt();
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MonitorStats {
    input: SignalStats,
    output: SignalStats,
    difference: SignalStats,
}

impl MonitorStats {
    fn finish(mut self, sample_count: usize) -> Self {
        self.input = self.input.finish(sample_count);
        self.output = self.output.finish(sample_count);
        self.difference = self.difference.finish(sample_count);
        self
    }
}

#[derive(Clone, Copy)]
enum SignalKind {
    Input,
    Output,
    Difference,
}

impl SignalKind {
    fn sample(self, sample: SignalMonitorSample) -> f32 {
        match self {
            Self::Input => sample.input,
            Self::Output => sample.output,
            Self::Difference => sample.difference(),
        }
    }
}

pub(super) fn draw(
    ui: &mut egui::Ui,
    history: &Mutex<SignalMonitorHistory>,
    metrics_handle: Option<&MetricsHandle>,
) {
    let metrics = metrics_handle.map_or_else(EngineMetrics::default, MetricsHandle::snapshot);
    let error = metrics_handle.and_then(MetricsHandle::last_error);
    // Release the history lock before invoking egui. A viewport repaint must
    // never hold it while the root viewport updates or clears the history.
    let history = history
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let stats = history.stats();
    let main_scale = (stats.input.peak.max(stats.output.peak) * 1.1).max(MIN_MAIN_SCALE);
    let difference_scale = (stats.difference.peak * 1.1).max(MIN_DIFFERENCE_SCALE);

    ui.heading("Signal Monitor");
    ui.horizontal(|ui| {
        ui.label("State");
        let (state_text, color) = state_label(metrics.state, ui.visuals());
        ui.label(RichText::new(state_text).color(color).strong());
        ui.separator();
        ui.weak("Processor-aligned 48 kHz mono · latest 1 second");
    });
    ui.add_space(8.0);

    draw_plot(
        ui,
        "Input",
        &history,
        SignalKind::Input,
        stats.input,
        main_scale,
        Color32::from_rgb(25, 110, 65),
    );
    draw_plot(
        ui,
        "Output",
        &history,
        SignalKind::Output,
        stats.output,
        main_scale,
        Color32::from_rgb(30, 95, 180),
    );
    draw_plot(
        ui,
        "Difference (Input - Output)",
        &history,
        SignalKind::Difference,
        stats.difference,
        difference_scale,
        Color32::from_rgb(180, 35, 45),
    );

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    draw_metrics(ui, metrics);

    if let Some(error) = error {
        ui.add_space(6.0);
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.add_space(6.0);
    ui.weak("Input is aligned to the processor delay before calculating Difference.");
}

fn draw_plot(
    ui: &mut egui::Ui,
    title: &str,
    history: &SignalMonitorHistory,
    kind: SignalKind,
    stats: SignalStats,
    scale: f32,
    color: Color32,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!(
                "scale ±{} · RMS {} · Peak {}",
                amplitude_text(scale),
                dbfs_text(stats.rms),
                dbfs_text(stats.peak),
            ));
        });
    });

    let desired_size = egui::vec2(ui.available_width().max(200.0), PLOT_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    painter.hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );

    if history.samples.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting for audio…",
            egui::FontId::proportional(13.0),
            ui.visuals().weak_text_color(),
        );
        return;
    }

    draw_envelope(&painter, rect, history, kind, scale, color);
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn draw_envelope(
    painter: &egui::Painter,
    rect: egui::Rect,
    history: &SignalMonitorHistory,
    kind: SignalKind,
    scale: f32,
    color: Color32,
) {
    let columns = (rect.width().floor() as usize).max(1);
    let history_start = HISTORY_FRAMES.saturating_sub(history.samples.len());
    let columns_f32 = columns as f32;
    let half_height = (rect.height() * 0.5 - 3.0).max(1.0);

    for column in 0..columns {
        let frame_start = column * HISTORY_FRAMES / columns;
        let frame_end = ((column + 1) * HISTORY_FRAMES / columns).max(frame_start + 1);
        let visible_start = frame_start.max(history_start);
        let visible_end = frame_end.min(HISTORY_FRAMES);
        if visible_start >= visible_end {
            continue;
        }

        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for sample_index in visible_start..visible_end {
            if let Some(&sample) = history.samples.get(sample_index - history_start) {
                let value = kind.sample(sample);
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
        }
        if !minimum.is_finite() || !maximum.is_finite() {
            continue;
        }

        let x = rect.left() + (column as f32 + 0.5) * rect.width() / columns_f32;
        let top = rect.center().y - (maximum / scale).clamp(-1.0, 1.0) * half_height;
        let bottom = rect.center().y - (minimum / scale).clamp(-1.0, 1.0) * half_height;
        painter.line_segment(
            [egui::pos2(x, top), egui::pos2(x, bottom)],
            Stroke::new(1.0, color),
        );
    }
}

fn draw_metrics(ui: &mut egui::Ui, metrics: EngineMetrics) {
    ui.label(RichText::new("Audio health").strong());
    egui::Grid::new("signal-monitor-metrics")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            metric(
                ui,
                "Output buffer",
                format_buffer(metrics.buffered_output_frames),
            );
            metric(
                ui,
                "Processed",
                format!("{} frames", metrics.processed_frames),
            );
            ui.end_row();
            metric(
                ui,
                "Dropped input",
                metrics.dropped_input_frames.to_string(),
            );
            metric(
                ui,
                "Inserted silence",
                metrics.inserted_silence_frames.to_string(),
            );
            ui.end_row();
            metric(
                ui,
                "Input discontinuities",
                metrics.input_discontinuities.to_string(),
            );
            metric(
                ui,
                "Output discontinuities",
                metrics.output_discontinuities.to_string(),
            );
            ui.end_row();
            metric(ui, "Stream faults", metrics.stream_faults.to_string());
            metric(
                ui,
                "Monitor drops",
                metrics.dropped_signal_monitor_frames.to_string(),
            );
            ui.end_row();
            metric(
                ui,
                "Max processing",
                format_processing_time(metrics.max_processing_time_us),
            );
            metric(
                ui,
                "Deadline misses",
                metrics.processing_deadline_misses.to_string(),
            );
            ui.end_row();
        });
}

fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.weak(label);
    ui.label(value);
}

fn format_buffer(frames: u32) -> String {
    let milliseconds = f64::from(frames) * 1_000.0 / f64::from(PIPELINE_SAMPLE_RATE);
    format!("{frames} frames ({milliseconds:.1} ms)")
}

fn format_processing_time(microseconds: u64) -> String {
    format!(
        "{}.{:02} ms",
        microseconds / 1_000,
        microseconds % 1_000 / 10
    )
}

fn amplitude_text(amplitude: f32) -> String {
    if amplitude >= 0.1 {
        format!("{amplitude:.2}")
    } else {
        format!("{amplitude:.3}")
    }
}

fn dbfs_text(amplitude: f32) -> String {
    if amplitude > 0.0 {
        format!("{:.1} dBFS", 20.0 * amplitude.log10())
    } else {
        "−∞ dBFS".to_owned()
    }
}

fn state_label(state: EngineState, visuals: &egui::Visuals) -> (&'static str, Color32) {
    match state {
        EngineState::Starting => ("Starting", visuals.warn_fg_color),
        EngineState::Running => ("Running", Color32::from_rgb(25, 110, 65)),
        EngineState::Faulted => ("Audio error", visuals.error_fg_color),
        EngineState::Stopped => ("Stopped", visuals.weak_text_color()),
    }
}

#[cfg(test)]
mod tests {
    use noise_hoihoi_engine::SignalMonitorSample;

    use super::{HISTORY_FRAMES, SignalMonitorHistory};

    #[test]
    fn history_keeps_only_the_latest_second() {
        let mut history = SignalMonitorHistory::default();
        history.append(&vec![SignalMonitorSample::default(); HISTORY_FRAMES]);
        history.append(&[SignalMonitorSample {
            input: 1.0,
            output: 0.0,
        }]);

        assert_eq!(history.samples.len(), HISTORY_FRAMES);
        assert_eq!(history.samples.back().map(|sample| sample.input), Some(1.0));
    }
}
