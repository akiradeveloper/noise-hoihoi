//! Ten-second observation of the active audio route. No extra inference runs.

use std::time::{Duration, Instant};

use noise_hoihoi_engine::{EngineMetrics, EngineState};

pub const CHECK_DURATION: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Headroom,
    Limited,
    TooSlow,
    Interruptions,
    InsufficientData,
}

#[derive(Clone, Debug)]
pub struct PerformanceReport {
    pub combination: String,
    pub elapsed: Duration,
    pub calls: u64,
    pub mean_ms: f64,
    pub deadline_misses: u64,
    pub dropped_input_frames: u64,
    pub inserted_silence_frames: u64,
    pub discontinuities: u64,
    pub verdict: Verdict,
}

impl PerformanceReport {
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self.verdict {
            Verdict::Headroom => "Processing headroom available",
            Verdict::Limited => "Limited processing headroom",
            Verdict::TooSlow => "Processing cannot keep up",
            Verdict::Interruptions => "Audio interruptions detected",
            Verdict::InsufficientData => "Not enough audio to judge",
        }
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn details(&self) -> String {
        if self.calls == 0 {
            return "No completed processing calls. Check the microphone and audio status.".into();
        }
        format!(
            "Mean {:.2} ms / 10 ms · over budget {}/{} ({:.1}%)\nDropped input: {} frames · inserted silence: {} frames\nAudio discontinuities: {}",
            self.mean_ms,
            self.deadline_misses,
            self.calls,
            100.0 * self.deadline_misses as f64 / self.calls as f64,
            self.dropped_input_frames,
            self.inserted_silence_frames,
            self.discontinuities,
        )
    }

    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "{}\n{}\nObserved {:.1} s\n{}\nThis checks performance under the current load, not noise-removal quality or guaranteed future performance.",
            self.combination,
            self.title(),
            self.elapsed.as_secs_f64(),
            self.details()
        )
    }
}

struct Observation {
    started: Instant,
    baseline: EngineMetrics,
    combination: String,
}

#[derive(Default)]
pub struct PerformanceCheck {
    active: Option<Observation>,
    report: Option<PerformanceReport>,
}

impl PerformanceCheck {
    /// Begin only after the route is running. Old failures are excluded by the baseline.
    pub fn start(&mut self, now: Instant, metrics: EngineMetrics, combination: String) {
        if metrics.state != EngineState::Running || self.active.is_some() {
            return;
        }
        self.report = None;
        self.active = Some(Observation {
            started: now,
            baseline: metrics,
            combination,
        });
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.active.is_some()
    }

    #[must_use]
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.active.as_ref().map(|observation| {
            CHECK_DURATION.saturating_sub(now.saturating_duration_since(observation.started))
        })
    }

    #[must_use]
    pub const fn report(&self) -> Option<&PerformanceReport> {
        self.report.as_ref()
    }

    /// Cancel a check or invalidate a result when the selected route changes.
    pub fn clear(&mut self) {
        self.active = None;
        self.report = None;
    }

    pub fn poll(&mut self, now: Instant, metrics: EngineMetrics) {
        let Some(active) = &self.active else {
            return;
        };
        if now.saturating_duration_since(active.started) < CHECK_DURATION
            && metrics.state == EngineState::Running
        {
            return;
        }
        if let Some(active) = self.active.take() {
            self.report = Some(evaluate(&active, now, metrics));
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn evaluate(observation: &Observation, now: Instant, end: EngineMetrics) -> PerformanceReport {
    let start = observation.baseline;
    let elapsed = now.saturating_duration_since(observation.started);
    let calls = end.processing_calls.saturating_sub(start.processing_calls);
    let total_us = end
        .processing_total_time_us
        .saturating_sub(start.processing_total_time_us);
    let deadline_misses = end
        .processing_deadline_misses
        .saturating_sub(start.processing_deadline_misses);
    let dropped_input_frames = end
        .dropped_input_frames
        .saturating_sub(start.dropped_input_frames);
    let inserted_silence_frames = end
        .inserted_silence_frames
        .saturating_sub(start.inserted_silence_frames);
    let discontinuities = end
        .input_discontinuities
        .saturating_sub(start.input_discontinuities)
        + end
            .output_discontinuities
            .saturating_sub(start.output_discontinuities);
    let mean_ms = if calls == 0 {
        0.0
    } else {
        total_us as f64 / calls as f64 / 1000.0
    };
    // A restarted route or counter rollback must never produce a successful result.
    let reset = end.processing_calls < start.processing_calls
        || end.processing_total_time_us < start.processing_total_time_us
        || end.processed_frames < start.processed_frames;
    let verdict = if reset {
        Verdict::InsufficientData
    } else if end.state == EngineState::Faulted
        || end.stream_faults > start.stream_faults
        || dropped_input_frames > 0
        || inserted_silence_frames > 0
        || discontinuities > 0
    {
        Verdict::Interruptions
    } else if end.state != EngineState::Running || calls == 0 || elapsed < CHECK_DURATION {
        Verdict::InsufficientData
    } else if mean_ms >= 10.0 {
        Verdict::TooSlow
    } else if calls as f64 * 0.010 < elapsed.as_secs_f64() * 0.9 {
        Verdict::InsufficientData
    } else if deadline_misses > 0 || mean_ms > 8.0 {
        Verdict::Limited
    } else {
        Verdict::Headroom
    };
    PerformanceReport {
        combination: observation.combination.clone(),
        elapsed,
        calls,
        mean_ms,
        deadline_misses,
        dropped_input_frames,
        inserted_silence_frames,
        discontinuities,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> EngineMetrics {
        EngineMetrics {
            state: EngineState::Running,
            ..EngineMetrics::default()
        }
    }

    fn measured(mean_us: u64) -> EngineMetrics {
        EngineMetrics {
            processing_calls: 1000,
            processing_total_time_us: mean_us * 1000,
            processed_frames: 480_000,
            ..running()
        }
    }

    fn verdict(end: EngineMetrics) -> Verdict {
        let now = Instant::now();
        let mut check = PerformanceCheck::default();
        check.start(now, running(), "test".into());
        check.poll(now + CHECK_DURATION, end);
        check.report().unwrap().verdict
    }

    #[test]
    fn distinguishes_headroom_spikes_slow_compute_and_route_faults() {
        assert_eq!(verdict(measured(4000)), Verdict::Headroom);
        assert_eq!(verdict(measured(9000)), Verdict::Limited);
        assert_eq!(verdict(measured(14000)), Verdict::TooSlow);
        assert_eq!(
            verdict(EngineMetrics {
                processing_deadline_misses: 1,
                ..measured(4000)
            }),
            Verdict::Limited
        );
        assert_eq!(
            verdict(EngineMetrics {
                dropped_input_frames: 480,
                ..measured(4000)
            }),
            Verdict::Interruptions
        );
        assert_eq!(
            verdict(EngineMetrics {
                inserted_silence_frames: 1,
                ..measured(4000)
            }),
            Verdict::Interruptions
        );
        assert_eq!(
            verdict(EngineMetrics {
                state: EngineState::Faulted,
                ..running()
            }),
            Verdict::Interruptions
        );
        assert_eq!(verdict(running()), Verdict::InsufficientData);
        assert_eq!(
            verdict(EngineMetrics {
                processing_calls: 20,
                processing_total_time_us: 80000,
                ..running()
            }),
            Verdict::InsufficientData
        );
    }

    #[test]
    fn uses_window_deltas_and_clears_cancelled_or_stale_results() {
        let now = Instant::now();
        let mut check = PerformanceCheck::default();
        let start = EngineMetrics {
            dropped_input_frames: 2000,
            processing_deadline_misses: 100,
            max_processing_time_us: 90000,
            ..measured(14000)
        };
        check.start(now, start, "Test GPU".into());
        check.poll(now + Duration::from_secs(9), start);
        assert!(check.is_running());
        check.poll(
            now + CHECK_DURATION,
            EngineMetrics {
                processing_calls: 2000,
                processing_total_time_us: 18_000_000,
                processed_frames: 960_000,
                ..start
            },
        );
        let report = check.report().unwrap();
        assert_eq!(report.verdict, Verdict::Headroom);
        assert!((report.mean_ms - 4.0).abs() < 1e-6);
        assert_eq!(report.dropped_input_frames, 0);
        check.clear();
        assert!(check.report().is_none());
        check.start(now, start, "test".into());
        check.poll(now + CHECK_DURATION, measured(4000));
        assert_eq!(check.report().unwrap().verdict, Verdict::InsufficientData);
        check.start(now, running(), "test".into());
        check.clear();
        check.poll(now + CHECK_DURATION, measured(4000));
        assert!(check.report().is_none());
    }
}
