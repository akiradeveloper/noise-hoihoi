use std::collections::VecDeque;

use gpui_kit::component::{ActiveTheme, StyledExt as _, TitleBar};
use gpui_kit::{
    App, Bounds, Context, Hsla, IntoElement, ParentElement, Pixels, Render, Styled, Window, canvas,
    div, fill, point, prelude::*, px, size,
};
use noise_hoihoi_engine::{EngineMetrics, EngineState, PIPELINE_SAMPLE_RATE, SignalMonitorSample};

const HISTORY_FRAMES: usize = PIPELINE_SAMPLE_RATE as usize;
const MIN_SCALE: f32 = 0.01;
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

#[derive(Default)]
pub(super) struct SignalMonitor {
    history: SignalMonitorHistory,
    metrics: EngineMetrics,
    error: Option<String>,
}

impl SignalMonitor {
    pub fn update(
        &mut self,
        history: &SignalMonitorHistory,
        metrics: EngineMetrics,
        error: Option<String>,
    ) {
        self.history.clone_from(history);
        self.metrics = metrics;
        self.error = error;
    }
}

impl Render for SignalMonitor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stats = self.history.stats();
        let peak = stats
            .input
            .peak
            .max(stats.output.peak)
            .max(stats.difference.peak);
        let scale = (peak * 1.1).max(MIN_SCALE);
        let (label, color) = state_label(self.metrics.state, cx);
        let content = div().id("signal-scroll").w_full().flex_1().min_h_0().overflow_y_scroll()
            .bg(cx.theme().background).text_color(cx.theme().foreground).text_sm()
            .child(div().v_flex().p_5().gap_3()
                .child(div().h_flex().justify_between().child(div().font_semibold().child("Signal Monitor")).child(div().text_color(color).child(label)))
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Processor-aligned 48 kHz mono · latest 1 second"))
                .child(plot("Input", &self.history, SignalKind::Input, stats.input, scale, cx.theme().success, cx))
                .child(plot("Output", &self.history, SignalKind::Output, stats.output, scale, cx.theme().primary, cx))
                .child(plot("Difference (Input − Output)", &self.history, SignalKind::Difference, stats.difference, scale, cx.theme().danger, cx))
                .child(div().h_flex().justify_between().text_xs().text_color(cx.theme().muted_foreground).child("−1.0 s").child("−0.5 s").child("0 s"))
                .child(div().border_t_1().border_color(cx.theme().border).pt_3().v_flex().gap_1()
                    .child(div().font_semibold().child("Audio health"))
                    .children(metric_rows(self.metrics).into_iter().map(|(label, value)| div().h_flex().justify_between().gap_3().text_xs().child(label).child(value))))
                .when_some(self.error.clone(), |panel, error| panel.child(div().text_color(cx.theme().danger).child(error)))
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Input is aligned to processor delay. Difference includes all changes to the signal, including voice.")));
        div()
            .v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(crate::app::window_title("NoiseHoiHoi - Signal Monitor")))
            .child(content)
    }
}

fn metric_rows(metrics: EngineMetrics) -> Vec<(&'static str, String)> {
    vec![
        (
            "Output buffer",
            format!(
                "{} frames ({:.1} ms)",
                metrics.buffered_output_frames,
                f64::from(metrics.buffered_output_frames) / 48.0
            ),
        ),
        ("Processed", format!("{} frames", metrics.processed_frames)),
        (
            "Dropped input / inserted silence",
            format!(
                "{} / {}",
                metrics.dropped_input_frames, metrics.inserted_silence_frames
            ),
        ),
        (
            "Input / output discontinuities",
            format!(
                "{} / {}",
                metrics.input_discontinuities, metrics.output_discontinuities
            ),
        ),
        (
            "Stream faults / monitor drops",
            format!(
                "{} / {}",
                metrics.stream_faults, metrics.dropped_signal_monitor_frames
            ),
        ),
        (
            "Max processing",
            format!(
                "{}.{:02} ms",
                metrics.max_processing_time_us / 1_000,
                metrics.max_processing_time_us % 1_000 / 10
            ),
        ),
        (
            "Deadline misses",
            metrics.processing_deadline_misses.to_string(),
        ),
    ]
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn plot(
    title: &'static str,
    history: &SignalMonitorHistory,
    kind: SignalKind,
    stats: SignalStats,
    scale: f32,
    color: Hsla,
    cx: &App,
) -> impl IntoElement {
    let samples = history
        .samples
        .iter()
        .map(|sample| kind.sample(*sample))
        .collect::<Vec<_>>();
    let grid_color = cx.theme().border;
    let empty = samples.is_empty();
    div()
        .v_flex()
        .gap_1()
        .child(
            div()
                .h_flex()
                .justify_between()
                .flex_wrap()
                .gap_1()
                .text_xs()
                .child(title)
                .child(div().text_color(cx.theme().muted_foreground).child(format!(
                    "Scale ±{scale:.3} · RMS {} · Peak {}",
                    dbfs_text(stats.rms),
                    dbfs_text(stats.peak)
                ))),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(104.0))
                .rounded_md()
                .bg(cx.theme().muted)
                .child(
                    canvas(
                        move |bounds, _, _| {
                            envelope(&samples, f32::from(bounds.size.width).max(1.0) as usize)
                        },
                        move |bounds, columns, window, _| {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(bounds.left(), bounds.center().y),
                                    size(bounds.size.width, px(1.0)),
                                ),
                                grid_color,
                            ));
                            paint_envelope(bounds, &columns, scale, color, window);
                        },
                    )
                    .size_full(),
                )
                .when(empty, |plot| {
                    plot.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Waiting for audio…"),
                    )
                }),
        )
}

// Preserve impulses by retaining both extrema per pixel, not every Nth sample.
fn envelope(samples: &[f32], columns: usize) -> Vec<Option<(f32, f32)>> {
    let history_start = HISTORY_FRAMES.saturating_sub(samples.len());
    (0..columns)
        .map(|column| {
            let frame_start = column * HISTORY_FRAMES / columns;
            let start = frame_start.max(history_start);
            let end = (((column + 1) * HISTORY_FRAMES / columns).max(frame_start + 1))
                .min(HISTORY_FRAMES);
            if start >= end {
                return None;
            }
            samples
                .get(start - history_start..end - history_start)
                .map(|values| {
                    values
                        .iter()
                        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), &value| {
                            (min.min(value), max.max(value))
                        })
                })
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn paint_envelope(
    bounds: Bounds<Pixels>,
    columns: &[Option<(f32, f32)>],
    scale: f32,
    color: Hsla,
    window: &mut Window,
) {
    let half_height = (f32::from(bounds.size.height) * 0.5 - 3.0).max(1.0);
    let width = f32::from(bounds.size.width) / columns.len().max(1) as f32;
    for (index, column) in columns.iter().enumerate() {
        if let Some((minimum, maximum)) = column {
            let top = bounds.center().y - px((maximum / scale).clamp(-1.0, 1.0) * half_height);
            let bottom = bounds.center().y - px((minimum / scale).clamp(-1.0, 1.0) * half_height);
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left() + px(index as f32 * width), top),
                    size(px(width.max(1.0)), (bottom - top).max(px(1.0))),
                ),
                color,
            ));
        }
    }
}

pub(super) fn dbfs_text(amplitude: f32) -> String {
    if amplitude > 0.0 {
        format!("{:.1} dBFS", 20.0 * amplitude.log10())
    } else {
        "−∞ dBFS".to_owned()
    }
}

pub(super) fn state_label(state: EngineState, cx: &App) -> (&'static str, Hsla) {
    match state {
        EngineState::Starting => ("Starting", cx.theme().warning),
        EngineState::Running => ("Running", cx.theme().success),
        EngineState::Faulted => ("Audio error", cx.theme().danger),
        EngineState::Stopped => ("Stopped", cx.theme().muted_foreground),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_retains_latest_second_and_preserves_impulses_in_envelope() {
        let mut history = SignalMonitorHistory::default();
        history.append(&vec![SignalMonitorSample::default(); HISTORY_FRAMES]);
        history.append(&[SignalMonitorSample {
            input: 1.0,
            output: 0.25,
        }]);
        assert_eq!(history.samples.len(), HISTORY_FRAMES);
        let values: Vec<_> = history.samples.iter().map(|sample| sample.input).collect();
        let columns = envelope(&values, 400);
        assert_eq!(columns.last(), Some(&Some((0.0, 1.0))));
        assert_eq!(
            history.samples.back().unwrap().difference().to_bits(),
            0.75_f32.to_bits()
        );
    }

    #[test]
    fn partial_history_is_right_aligned_and_pass_through_difference_is_zero() {
        let mut history = SignalMonitorHistory::default();
        history.append(&vec![
            SignalMonitorSample {
                input: -0.25,
                output: -0.25
            };
            HISTORY_FRAMES / 2
        ]);
        let values: Vec<_> = history
            .samples
            .iter()
            .map(|sample| sample.difference())
            .collect();
        let columns = envelope(&values, 100);
        assert!(columns[..50].iter().all(Option::is_none));
        assert!(columns[50..].iter().all(|value| *value == Some((0.0, 0.0))));
        history.clear();
        assert!(envelope(&[], 100).iter().all(Option::is_none));
    }
}
