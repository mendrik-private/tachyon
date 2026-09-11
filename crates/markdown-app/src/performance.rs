use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::{FrameDurationSnapshot, InputLatencySnapshot, Window};
use hdrhistogram::Histogram;
use serde::Serialize;

const NANOS_PER_MILLISECOND: f64 = 1_000_000.0;

/// Developer-only, content-free JSON diagnostics on stderr. No file writes or
/// document preferences are inferred from enabling this inspection surface.
pub fn layout_trace_mode() -> document_view::LayoutTraceMode {
    match env::var("TACHYON_LAYOUT_TRACE").as_deref() {
        Ok("details") => document_view::LayoutTraceMode::Details,
        Ok("1" | "true" | "summary") => document_view::LayoutTraceMode::Summary,
        _ => document_view::LayoutTraceMode::Off,
    }
}

pub fn emit_layout_diagnostics(report: &document_view::LayoutDiagnosticsReport) {
    match serde_json::to_string(report) {
        Ok(json) => eprintln!("TACHYON_LAYOUT_TRACE {json}"),
        Err(error) => eprintln!("Layout diagnostics could not be encoded: {error}"),
    }
}

#[derive(Clone, Debug)]
pub struct PerformanceConfig {
    pub output: PathBuf,
    pub duration: Duration,
    pub warmup: Duration,
    pub refresh_hz: f64,
    pub label: String,
    pub scenario: String,
    pub input_source: String,
    pub exercise_resize: bool,
}

impl PerformanceConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        let Some(output) = env::var_os("TACHYON_PERF_OUTPUT") else {
            return Ok(None);
        };
        let seconds = parse_positive_f64("TACHYON_PERF_SECONDS", 60.0)?;
        let warmup_ms = parse_nonnegative_u64("TACHYON_PERF_WARMUP_MS", 2_500)?;
        let refresh_hz = parse_positive_f64("TACHYON_PERF_REFRESH_HZ", 120.0)?;
        Ok(Some(Self {
            output: PathBuf::from(output),
            duration: Duration::from_secs_f64(seconds),
            warmup: Duration::from_millis(warmup_ms),
            refresh_hz,
            label: env_value("TACHYON_PERF_LABEL", "unnamed"),
            scenario: env_value("TACHYON_PERF_SCENARIO", "external-interaction"),
            input_source: env_value("TACHYON_PERF_INPUT_SOURCE", "Wayland seat"),
            exercise_resize: env_flag("TACHYON_PERF_RESIZE"),
        }))
    }

    pub fn refresh_period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.refresh_hz)
    }
}

#[derive(Clone, Debug)]
pub struct StartupConfig {
    pub output: PathBuf,
    pub label: String,
    pub cache_state: String,
    pub reuses_render_process: bool,
}

impl StartupConfig {
    pub fn from_env() -> Option<Self> {
        env::var_os("TACHYON_STARTUP_OUTPUT").map(|output| Self {
            output: PathBuf::from(output),
            label: env_value("TACHYON_STARTUP_LABEL", "unnamed"),
            cache_state: env_value("TACHYON_STARTUP_CACHE_STATE", "unspecified"),
            reuses_render_process: false,
        })
    }
}

fn env_value(name: &str, fallback: &str) -> String {
    env::var(name).unwrap_or_else(|_| fallback.to_owned())
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

fn parse_positive_f64(name: &str, fallback: f64) -> Result<f64, String> {
    let value = match env::var(name) {
        Ok(value) => value
            .parse::<f64>()
            .map_err(|error| format!("{name} must be a number: {error}"))?,
        Err(env::VarError::NotPresent) => fallback,
        Err(error) => return Err(format!("{name} is not valid Unicode: {error}")),
    };
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{name} must be finite and greater than zero"));
    }
    Ok(value)
}

fn parse_nonnegative_u64(name: &str, fallback: u64) -> Result<u64, String> {
    match env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|error| format!("{name} must be a nonnegative integer: {error}")),
        Err(env::VarError::NotPresent) => Ok(fallback),
        Err(error) => Err(format!("{name} is not valid Unicode: {error}")),
    }
}

pub struct PerformanceCapture {
    frame: FrameDurationSnapshot,
    input: InputLatencySnapshot,
}

impl PerformanceCapture {
    pub fn start(window: &Window) -> Self {
        Self {
            frame: window.frame_duration_snapshot(),
            input: window.input_latency_snapshot(),
        }
    }

    pub fn finish(
        baseline: Self,
        window: &Window,
        config: &PerformanceConfig,
        actual_duration: Duration,
    ) -> Result<PerformanceReport, String> {
        let mut frame = window.frame_duration_snapshot();
        let mut input = window.input_latency_snapshot();
        subtract(
            &mut frame.draw_duration_histogram,
            &baseline.frame.draw_duration_histogram,
            "draw duration",
        )?;
        subtract(
            &mut frame.present_interval_histogram,
            &baseline.frame.present_interval_histogram,
            "presentation interval",
        )?;
        subtract(
            &mut input.latency_histogram,
            &baseline.input.latency_histogram,
            "input latency",
        )?;
        subtract(
            &mut input.events_per_frame_histogram,
            &baseline.input.events_per_frame_histogram,
            "events per frame",
        )?;
        let mid_draw_events_dropped = input
            .mid_draw_events_dropped
            .checked_sub(baseline.input.mid_draw_events_dropped)
            .ok_or_else(|| "input profiler counter moved backwards".to_owned())?;
        let viewport = window.viewport_size();
        let refresh_period_ns = config.refresh_period().as_nanos() as u64;

        Ok(PerformanceReport {
            schema_version: 2,
            label: config.label.clone(),
            scenario: config.scenario.clone(),
            input_source: config.input_source.clone(),
            configured_duration_seconds: config.duration.as_secs_f64(),
            actual_duration_seconds: actual_duration.as_secs_f64(),
            configured_refresh_hz: config.refresh_hz,
            refresh_period_ms: nanos_to_ms(refresh_period_ns),
            viewport_logical_px: ViewportSize {
                width: f32::from(viewport.width),
                height: f32::from(viewport.height),
            },
            scale_factor: window.scale_factor(),
            draw: Distribution::from_histogram(&frame.draw_duration_histogram, Some(6.0)),
            application_stalls_at_least_25_ms: count_at_or_above(
                &frame.draw_duration_histogram,
                25_000_000,
            ),
            input: InputReport {
                latency: Distribution::from_histogram(&input.latency_histogram, Some(16.7)),
                events_per_frame: CountDistribution::from_histogram(
                    &input.events_per_frame_histogram,
                ),
                mid_draw_events_dropped,
            },
            presentation: PresentationReport::from_histogram(
                &frame.present_interval_histogram,
                refresh_period_ns,
            ),
        })
    }
}

fn subtract(
    histogram: &mut Histogram<u64>,
    baseline: &Histogram<u64>,
    name: &str,
) -> Result<(), String> {
    histogram
        .subtract(baseline)
        .map_err(|error| format!("could not subtract {name} baseline: {error}"))
}

pub fn sustain_frames(window: &mut Window, until: Instant) {
    window.on_next_frame(move |window, _| {
        if Instant::now() < until {
            window.refresh();
            sustain_frames(window, until);
        }
    });
    window.refresh();
}

#[derive(Debug, Serialize)]
pub struct PerformanceReport {
    schema_version: u32,
    label: String,
    scenario: String,
    input_source: String,
    configured_duration_seconds: f64,
    actual_duration_seconds: f64,
    configured_refresh_hz: f64,
    refresh_period_ms: f64,
    viewport_logical_px: ViewportSize,
    scale_factor: f32,
    draw: Distribution,
    application_stalls_at_least_25_ms: u64,
    input: InputReport,
    presentation: PresentationReport,
}

#[derive(Debug, Serialize)]
pub struct StartupReport {
    schema_version: u32,
    label: String,
    cache_state: String,
    measurement: &'static str,
    process_model: &'static str,
    elapsed_ms: f64,
    source_bytes: usize,
    viewport_logical_px: ViewportSize,
    scale_factor: f32,
}

impl StartupReport {
    pub fn capture(
        config: &StartupConfig,
        elapsed: Duration,
        source_bytes: usize,
        viewport_width: f32,
        viewport_height: f32,
        scale_factor: f32,
    ) -> Self {
        Self {
            schema_version: 2,
            label: config.label.clone(),
            cache_state: config.cache_state.clone(),
            measurement: "invoking process main entry to completed GPUI paint of the first editable document scene",
            process_model: if config.reuses_render_process {
                "resident render process reused"
            } else {
                "fresh render process"
            },
            elapsed_ms: elapsed.as_secs_f64() * 1_000.0,
            source_bytes,
            viewport_logical_px: ViewportSize {
                width: viewport_width,
                height: viewport_height,
            },
            scale_factor,
        }
    }
}

#[derive(Debug, Serialize)]
struct ViewportSize {
    width: f32,
    height: f32,
}

#[derive(Debug, Serialize)]
struct Distribution {
    samples: u64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    threshold_ms: Option<f64>,
    over_threshold: Option<u64>,
    over_threshold_percent: Option<f64>,
}

impl Distribution {
    fn from_histogram(histogram: &Histogram<u64>, threshold_ms: Option<f64>) -> Self {
        let samples = histogram.len();
        let over_threshold = threshold_ms.map(|threshold| {
            count_at_or_above(histogram, (threshold * NANOS_PER_MILLISECOND).ceil() as u64)
        });
        Self {
            samples,
            p50_ms: nanos_to_ms(histogram.value_at_quantile(0.50)),
            p95_ms: nanos_to_ms(histogram.value_at_quantile(0.95)),
            p99_ms: nanos_to_ms(histogram.value_at_quantile(0.99)),
            max_ms: nanos_to_ms(histogram.max()),
            threshold_ms,
            over_threshold,
            over_threshold_percent: over_threshold.map(|count| percent(count, samples)),
        }
    }
}

#[derive(Debug, Serialize)]
struct CountDistribution {
    samples: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

impl CountDistribution {
    fn from_histogram(histogram: &Histogram<u64>) -> Self {
        Self {
            samples: histogram.len(),
            p50: histogram.value_at_quantile(0.50),
            p95: histogram.value_at_quantile(0.95),
            p99: histogram.value_at_quantile(0.99),
            max: histogram.max(),
        }
    }
}

#[derive(Debug, Serialize)]
struct InputReport {
    latency: Distribution,
    events_per_frame: CountDistribution,
    mid_draw_events_dropped: u64,
}

#[derive(Debug, Serialize)]
struct PresentationReport {
    interval: Distribution,
    missed_deadlines: u64,
    deadline_opportunities: u64,
    missed_deadline_percent: f64,
    intervals_at_least_25_ms: u64,
    deadline_method: &'static str,
}

impl PresentationReport {
    fn from_histogram(histogram: &Histogram<u64>, refresh_period_ns: u64) -> Self {
        let mut missed_deadlines = 0_u64;
        let mut stalls = 0_u64;
        for value in histogram.iter_recorded() {
            let interval_ns = value.value_iterated_to();
            let count = value.count_at_value();
            let periods = ((interval_ns as f64 / refresh_period_ns as f64).round() as u64).max(1);
            missed_deadlines =
                missed_deadlines.saturating_add(periods.saturating_sub(1).saturating_mul(count));
            if interval_ns >= 25_000_000 {
                stalls = stalls.saturating_add(count);
            }
        }
        let deadline_opportunities = histogram.len().saturating_add(missed_deadlines);
        Self {
            interval: Distribution::from_histogram(histogram, None),
            missed_deadlines,
            deadline_opportunities,
            missed_deadline_percent: percent(missed_deadlines, deadline_opportunities),
            intervals_at_least_25_ms: stalls,
            deadline_method: "round(interval / configured refresh period); missed = max(periods - 1, 0)",
        }
    }
}

fn count_at_or_above(histogram: &Histogram<u64>, threshold: u64) -> u64 {
    histogram
        .iter_recorded()
        .filter(|value| value.value_iterated_to() >= threshold)
        .map(|value| value.count_at_value())
        .sum()
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn nanos_to_ms(nanos: u64) -> f64 {
    nanos as f64 / NANOS_PER_MILLISECOND
}

pub fn write_report(path: &Path, report: &PerformanceReport) -> Result<(), String> {
    write_json(path, report)
}

pub fn write_startup_report(path: &Path, report: &StartupReport) -> Result<(), String> {
    write_json(path, report)
}

fn write_json(path: &Path, report: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "could not create report directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("could not serialize report: {error}"))?;
    fs::write(path, bytes)
        .map_err(|error| format!("could not write report {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram(values: &[u64]) -> Histogram<u64> {
        let mut histogram = Histogram::new(3).expect("histogram");
        for value in values {
            histogram.record(*value).expect("record");
        }
        histogram
    }

    #[test]
    fn presentation_deadlines_count_skipped_refresh_periods() {
        let period = 8_333_333;
        let report = PresentationReport::from_histogram(
            &histogram(&[period, period + 100_000, period * 2, period * 4]),
            period,
        );

        assert_eq!(report.missed_deadlines, 4);
        assert_eq!(report.deadline_opportunities, 8);
        assert_eq!(report.intervals_at_least_25_ms, 1);
        assert!((report.missed_deadline_percent - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn distribution_counts_values_at_or_above_budget() {
        let report = Distribution::from_histogram(
            // Keep the below-threshold value outside the three-significant-digit
            // histogram bucket that contains the exact boundary.
            &histogram(&[1_000_000, 5_900_000, 6_000_000, 8_000_000]),
            Some(6.0),
        );

        assert_eq!(report.samples, 4);
        assert_eq!(report.over_threshold, Some(2));
        assert_eq!(report.over_threshold_percent, Some(50.0));
    }

    #[test]
    fn subtract_removes_only_warmup_samples() {
        let baseline = histogram(&[1_000_000, 2_000_000]);
        let mut current = baseline.clone();
        current.record(4_000_000).expect("record measured value");
        subtract(&mut current, &baseline, "test").expect("subtract baseline");

        assert_eq!(current.len(), 1);
        assert_eq!(current.value_at_quantile(1.0), 4_001_791);
    }
}
