//! Before/after noise-band verdict (HZ-015 / AC-2.3, AC-2.4).
//!
//! A change is Helpful only when the median gain exceeds the measured
//! run-to-run noise band. Otherwise the API reports "no measurable effect"
//! and offers revert.

use super::engine::BenchmarkResult;
use super::stats::MetricSpread;

/// Primary metric used for helpful / no-effect decisions in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictMetric {
    AvgFps,
    Low1PctFps,
}

/// Outcome of comparing a before and after benchmark (AC-2.3, AC-2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeVerdict {
    /// Median gain exceeds the noise band.
    Helpful,
    /// Gain does not exceed noise; UI must report "no measurable effect".
    NoMeasurableEffect,
    /// Median loss exceeds the noise band (still offers revert).
    Harmful,
}

impl ChangeVerdict {
    /// Exact UI string for AC-2.4 when applicable.
    pub const NO_MEASURABLE_EFFECT_MESSAGE: &'static str = "no measurable effect";

    pub fn message(self) -> &'static str {
        match self {
            Self::Helpful => "helpful",
            Self::NoMeasurableEffect => Self::NO_MEASURABLE_EFFECT_MESSAGE,
            Self::Harmful => "harmful",
        }
    }
}

/// Full verdict with numbers for UI / change-log (AC-2.3, AC-2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictReport {
    pub metric: VerdictMetric,
    pub verdict: ChangeVerdict,
    pub before_median: f64,
    pub after_median: f64,
    /// `after_median - before_median` (positive = higher FPS).
    pub gain: f64,
    /// Run-to-run noise half-width used as the helpfulness threshold.
    pub noise_band: f64,
    /// True when the API offers revert (no measurable effect or harmful).
    pub offer_revert: bool,
    pub message: &'static str,
}

/// Run-to-run noise band for one metric spread.
///
/// Uses the larger of IQR and half the pass min--max range so identical
/// passes (IQR 0, range 0) yield a zero band and any real gain counts.
pub fn noise_band(spread: &MetricSpread) -> f64 {
    let half_range = (spread.max - spread.min) / 2.0;
    spread.iqr.max(half_range).max(0.0)
}

/// Combined noise from before and after runs (machine + workload).
pub fn combined_noise_band(before: &MetricSpread, after: &MetricSpread) -> f64 {
    noise_band(before).max(noise_band(after))
}

/// Compare two metric spreads: helpful only if gain > noise (AC-2.3).
pub fn evaluate_spreads(
    metric: VerdictMetric,
    before: &MetricSpread,
    after: &MetricSpread,
) -> VerdictReport {
    let before_median = before.median;
    let after_median = after.median;
    let gain = after_median - before_median;
    let noise = combined_noise_band(before, after);
    let verdict = if gain > noise {
        ChangeVerdict::Helpful
    } else if gain < -noise {
        ChangeVerdict::Harmful
    } else {
        ChangeVerdict::NoMeasurableEffect
    };
    let offer_revert = matches!(
        verdict,
        ChangeVerdict::NoMeasurableEffect | ChangeVerdict::Harmful
    );
    VerdictReport {
        metric,
        verdict,
        before_median,
        after_median,
        gain,
        noise_band: noise,
        offer_revert,
        message: verdict.message(),
    }
}

/// Compare before/after benchmark results on avg FPS (default AC-2 path).
pub fn evaluate_benchmarks(before: &BenchmarkResult, after: &BenchmarkResult) -> VerdictReport {
    evaluate_spreads(VerdictMetric::AvgFps, &before.avg_fps, &after.avg_fps)
}

/// Compare before/after on a chosen metric.
pub fn evaluate_benchmarks_metric(
    before: &BenchmarkResult,
    after: &BenchmarkResult,
    metric: VerdictMetric,
) -> VerdictReport {
    let (b, a) = match metric {
        VerdictMetric::AvgFps => (&before.avg_fps, &after.avg_fps),
        VerdictMetric::Low1PctFps => (&before.low_1pct_fps, &after.low_1pct_fps),
    };
    evaluate_spreads(metric, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::engine::{
        run_benchmark, BenchmarkConfig, BenchmarkError, FrameWindowSource,
    };

    /// mock: deterministic frame windows; not a live OS capture.
    struct MockWindowSource {
        windows: Vec<Vec<f64>>,
        next: usize,
    }

    impl MockWindowSource {
        fn new(windows: Vec<Vec<f64>>) -> Self {
            Self { windows, next: 0 }
        }
    }

    impl FrameWindowSource for MockWindowSource {
        fn collect_window(&mut self, _duration_ms: u64) -> Result<Vec<f64>, BenchmarkError> {
            if self.next >= self.windows.len() {
                return Err(BenchmarkError::Capture("mock: no more windows".into()));
            }
            let w = self.windows[self.next].clone();
            self.next += 1;
            Ok(w)
        }
    }

    fn constant_frames(ms: f64, n: usize) -> Vec<f64> {
        vec![ms; n]
    }

    fn cfg() -> BenchmarkConfig {
        BenchmarkConfig {
            warmup_ms: 100,
            measure_ms: 100,
            pass_count: 3,
        }
    }

    fn mock_result(frame_ms: f64) -> crate::benchmark::BenchmarkResult {
        let mut source = MockWindowSource::new(vec![
            constant_frames(frame_ms, 5),
            constant_frames(frame_ms, 40),
            constant_frames(frame_ms, 40),
            constant_frames(frame_ms, 40),
        ]);
        run_benchmark(cfg(), &mut source).expect("mock run")
    }

    fn mock_result_spread(ms_a: f64, ms_b: f64, ms_c: f64) -> crate::benchmark::BenchmarkResult {
        let mut source = MockWindowSource::new(vec![
            constant_frames(ms_a, 5),
            constant_frames(ms_a, 40),
            constant_frames(ms_b, 40),
            constant_frames(ms_c, 40),
        ]);
        run_benchmark(cfg(), &mut source).expect("mock run")
    }

    #[test]
    fn above_noise_is_helpful() {
        // mock: before ~60 FPS flat; after ~90 FPS flat; noise 0 -> gain exceeds
        let before = mock_result(1000.0 / 60.0);
        let after = mock_result(1000.0 / 90.0);
        let report = evaluate_benchmarks(&before, &after);
        assert_eq!(report.verdict, ChangeVerdict::Helpful);
        assert!(report.gain > report.noise_band);
        assert!(!report.offer_revert);
        assert_eq!(report.message, "helpful");
    }

    #[test]
    fn below_noise_is_no_measurable_effect_and_offers_revert() {
        // mock: before passes 58/60/62 FPS; after 59/61/63 -- gain inside IQR noise
        let before = mock_result_spread(1000.0 / 58.0, 1000.0 / 60.0, 1000.0 / 62.0);
        let after = mock_result_spread(1000.0 / 59.0, 1000.0 / 61.0, 1000.0 / 63.0);
        let report = evaluate_benchmarks(&before, &after);
        assert_eq!(report.verdict, ChangeVerdict::NoMeasurableEffect);
        assert!(report.gain.abs() <= report.noise_band + 1e-9);
        assert!(report.offer_revert);
        assert_eq!(report.message, ChangeVerdict::NO_MEASURABLE_EFFECT_MESSAGE);
        assert_eq!(report.message, "no measurable effect");
    }

    #[test]
    fn zero_gain_offers_revert() {
        // mock: identical before/after
        let before = mock_result(16.0);
        let after = mock_result(16.0);
        let report = evaluate_benchmarks(&before, &after);
        assert_eq!(report.verdict, ChangeVerdict::NoMeasurableEffect);
        assert_eq!(report.gain, 0.0);
        assert!(report.offer_revert);
        assert_eq!(report.message, "no measurable effect");
    }

    #[test]
    fn harmful_when_loss_exceeds_noise() {
        let before = mock_result(1000.0 / 90.0);
        let after = mock_result(1000.0 / 60.0);
        let report = evaluate_benchmarks(&before, &after);
        assert_eq!(report.verdict, ChangeVerdict::Harmful);
        assert!(report.gain < -report.noise_band);
        assert!(report.offer_revert);
    }

    #[test]
    fn evaluate_spreads_unit_above_and_below() {
        // mock labeled MetricSpread values (not live capture)
        let quiet = MetricSpread::from_values(&[60.0, 60.0, 60.0]).expect("spread");
        let noisy = MetricSpread::from_values(&[58.0, 60.0, 62.0]).expect("spread");
        let better = MetricSpread::from_values(&[70.0, 72.0, 74.0]).expect("spread");
        let tiny = MetricSpread::from_values(&[60.5, 61.0, 61.5]).expect("spread");

        let helpful = evaluate_spreads(VerdictMetric::AvgFps, &quiet, &better);
        assert_eq!(helpful.verdict, ChangeVerdict::Helpful);
        assert!(!helpful.offer_revert);

        let no_effect = evaluate_spreads(VerdictMetric::AvgFps, &noisy, &tiny);
        assert_eq!(no_effect.verdict, ChangeVerdict::NoMeasurableEffect);
        assert!(no_effect.offer_revert);
        assert_eq!(no_effect.message, "no measurable effect");
    }

    #[test]
    fn low_1pct_metric_path() {
        let before = mock_result(16.0);
        let after = mock_result(10.0); // higher FPS
        let report = evaluate_benchmarks_metric(&before, &after, VerdictMetric::Low1PctFps);
        assert_eq!(report.metric, VerdictMetric::Low1PctFps);
        assert_eq!(report.verdict, ChangeVerdict::Helpful);
    }

    #[test]
    fn noise_band_uses_iqr_or_half_range() {
        let s = MetricSpread::from_values(&[50.0, 60.0, 70.0]).expect("spread");
        let n = noise_band(&s);
        assert!(n > 0.0);
        assert!((n - s.iqr.max((s.max - s.min) / 2.0)).abs() < 1e-12);
    }
}
