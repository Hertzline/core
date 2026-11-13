//! Benchmark runner: warm-up + fixed-duration passes (AC-2.1).

use crate::frame_capture::{compute_stats_ms, FrameTimeStats};
use crate::session::FrameSummary;

use super::stats::MetricSpread;

/// Minimum measurement passes required by AC-2.1.
pub const MIN_PASS_COUNT: u32 = 3;

/// Fixed timing for a repeatable benchmark run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkConfig {
    /// Warm-up window discarded before each pass set (once at start).
    pub warmup_ms: u64,
    /// Fixed measurement duration per pass.
    pub measure_ms: u64,
    /// Number of measurement passes (must be >= [`MIN_PASS_COUNT`]).
    pub pass_count: u32,
}

impl BenchmarkConfig {
    pub fn validate(&self) -> Result<(), BenchmarkError> {
        if self.warmup_ms == 0 {
            return Err(BenchmarkError::InvalidConfig(
                "warmup_ms must be > 0".into(),
            ));
        }
        if self.measure_ms == 0 {
            return Err(BenchmarkError::InvalidConfig(
                "measure_ms must be > 0".into(),
            ));
        }
        if self.pass_count < MIN_PASS_COUNT {
            return Err(BenchmarkError::InvalidConfig(format!(
                "pass_count must be >= {MIN_PASS_COUNT} (got {})",
                self.pass_count
            )));
        }
        Ok(())
    }
}

/// One completed measurement pass.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkPass {
    pub index: u32,
    pub stats: FrameTimeStats,
    pub summary: FrameSummary,
}

/// Aggregated benchmark outcome (AC-2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub config: BenchmarkConfig,
    pub passes: Vec<BenchmarkPass>,
    pub avg_fps: MetricSpread,
    pub low_1pct_fps: MetricSpread,
    pub low_0_1pct_fps: MetricSpread,
    pub avg_frame_time_ms: MetricSpread,
}

/// Supplies frame times for a timed window. Live capture or labeled mocks.
pub trait FrameWindowSource {
    /// Collect frame times (ms) for approximately `duration_ms`.
    ///
    /// Unit tests must label mock sources. Live backends may sleep/block.
    fn collect_window(&mut self, duration_ms: u64) -> Result<Vec<f64>, BenchmarkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchmarkError {
    InvalidConfig(String),
    Capture(String),
    InsufficientSamples { pass_index: u32, detail: String },
}

impl std::fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            Self::Capture(msg) => write!(f, "capture failed: {msg}"),
            Self::InsufficientSamples { pass_index, detail } => {
                write!(f, "pass {pass_index}: insufficient samples ({detail})")
            }
        }
    }
}

impl std::error::Error for BenchmarkError {}

/// Run warm-up (discarded) then `pass_count` fixed-duration measurement passes.
pub fn run_benchmark<S: FrameWindowSource>(
    config: BenchmarkConfig,
    source: &mut S,
) -> Result<BenchmarkResult, BenchmarkError> {
    config.validate()?;

    // Warm-up: discard samples so pass timing is not cold-start biased.
    let _warmup = source.collect_window(config.warmup_ms)?;

    let mut passes = Vec::with_capacity(config.pass_count as usize);
    for i in 0..config.pass_count {
        let samples = source.collect_window(config.measure_ms)?;
        let stats =
            compute_stats_ms(&samples).ok_or_else(|| BenchmarkError::InsufficientSamples {
                pass_index: i,
                detail: format!(
                    "need at least 2 positive frame samples, got {}",
                    samples
                        .iter()
                        .filter(|t| t.is_finite() && **t > 0.0)
                        .count()
                ),
            })?;
        let summary = FrameSummary::from_stats(&stats);
        passes.push(BenchmarkPass {
            index: i,
            stats,
            summary,
        });
    }

    aggregate_passes(config, passes)
}

fn aggregate_passes(
    config: BenchmarkConfig,
    passes: Vec<BenchmarkPass>,
) -> Result<BenchmarkResult, BenchmarkError> {
    let avg_fps: Vec<f64> = passes.iter().map(|p| p.stats.avg_fps).collect();
    let low_1: Vec<f64> = passes.iter().map(|p| p.stats.low_1pct_fps).collect();
    let low_01: Vec<f64> = passes.iter().map(|p| p.stats.low_0_1pct_fps).collect();
    let ft: Vec<f64> = passes.iter().map(|p| p.stats.avg_frame_time_ms).collect();

    Ok(BenchmarkResult {
        config,
        avg_fps: MetricSpread::from_values(&avg_fps)
            .ok_or_else(|| BenchmarkError::InvalidConfig("no finite avg_fps values".into()))?,
        low_1pct_fps: MetricSpread::from_values(&low_1)
            .ok_or_else(|| BenchmarkError::InvalidConfig("no finite low_1pct_fps values".into()))?,
        low_0_1pct_fps: MetricSpread::from_values(&low_01).ok_or_else(|| {
            BenchmarkError::InvalidConfig("no finite low_0_1pct_fps values".into())
        })?,
        avg_frame_time_ms: MetricSpread::from_values(&ft).ok_or_else(|| {
            BenchmarkError::InvalidConfig("no finite avg_frame_time_ms values".into())
        })?,
        passes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn default_config() -> BenchmarkConfig {
        BenchmarkConfig {
            warmup_ms: 1000,
            measure_ms: 5000,
            pass_count: 3,
        }
    }

    #[test]
    fn rejects_fewer_than_three_passes() {
        let cfg = BenchmarkConfig {
            warmup_ms: 100,
            measure_ms: 100,
            pass_count: 2,
        };
        let err = cfg.validate().expect_err("must reject");
        assert!(matches!(err, BenchmarkError::InvalidConfig(_)));
    }

    #[test]
    fn rejects_zero_durations() {
        assert!(BenchmarkConfig {
            warmup_ms: 0,
            measure_ms: 100,
            pass_count: 3,
        }
        .validate()
        .is_err());
        assert!(BenchmarkConfig {
            warmup_ms: 100,
            measure_ms: 0,
            pass_count: 3,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn mock_three_passes_median_and_spread() {
        // mock: warmup + 3 passes at 16 / 15 / 17 ms (~62.5 / 66.7 / 58.8 FPS)
        let mut source = MockWindowSource::new(vec![
            constant_frames(16.0, 10), // warm-up discarded
            constant_frames(16.0, 50),
            constant_frames(15.0, 50),
            constant_frames(17.0, 50),
        ]);
        let result = run_benchmark(default_config(), &mut source).expect("run");
        assert_eq!(result.passes.len(), 3);
        assert_eq!(result.config.pass_count, 3);
        assert_eq!(result.config.warmup_ms, 1000);
        assert_eq!(result.config.measure_ms, 5000);

        let fps: Vec<f64> = result.passes.iter().map(|p| p.stats.avg_fps).collect();
        assert!((fps[0] - 62.5).abs() < 1e-9);
        assert!((fps[1] - (1000.0 / 15.0)).abs() < 1e-9);
        assert!((fps[2] - (1000.0 / 17.0)).abs() < 1e-9);

        assert!((result.avg_fps.median - 62.5).abs() < 1e-9);
        assert!((result.avg_fps.min - (1000.0 / 17.0)).abs() < 1e-9);
        assert!((result.avg_fps.max - (1000.0 / 15.0)).abs() < 1e-9);
        assert!(result.avg_fps.iqr >= 0.0);

        assert!(result.low_1pct_fps.sample_count == 3);
        assert!(result.low_0_1pct_fps.sample_count == 3);
        assert!(result.avg_frame_time_ms.median > 0.0);
    }

    #[test]
    fn mock_reproducibility_identical_inputs() {
        // automated reproducibility: same mock windows -> identical aggregates
        let windows = vec![
            constant_frames(16.0, 8),
            constant_frames(16.0, 40),
            constant_frames(16.5, 40),
            constant_frames(15.5, 40),
        ];
        let cfg = default_config();
        let a =
            run_benchmark(cfg.clone(), &mut MockWindowSource::new(windows.clone())).expect("run a");
        let b = run_benchmark(cfg, &mut MockWindowSource::new(windows)).expect("run b");
        assert_eq!(a.avg_fps, b.avg_fps);
        assert_eq!(a.low_1pct_fps, b.low_1pct_fps);
        assert_eq!(a.low_0_1pct_fps, b.low_0_1pct_fps);
        assert_eq!(a.avg_frame_time_ms, b.avg_frame_time_ms);
        assert_eq!(a.passes.len(), b.passes.len());
        for (pa, pb) in a.passes.iter().zip(b.passes.iter()) {
            assert_eq!(pa.stats, pb.stats);
        }
    }

    #[test]
    fn mock_warmup_not_counted_as_pass() {
        // mock: only 1 + 3 windows; if warm-up were counted we'd fail on 4th measure
        let mut source = MockWindowSource::new(vec![
            constant_frames(20.0, 5),
            constant_frames(16.0, 20),
            constant_frames(16.0, 20),
            constant_frames(16.0, 20),
        ]);
        let result = run_benchmark(default_config(), &mut source).expect("run");
        assert_eq!(result.passes.len(), 3);
        assert!(source.next == 4);
        // all measure passes ~62.5 FPS; warm-up 20 ms never appears in passes
        for p in &result.passes {
            assert!((p.stats.avg_fps - 62.5).abs() < 1e-9);
        }
    }

    #[test]
    fn insufficient_samples_on_pass() {
        let mut source = MockWindowSource::new(vec![
            constant_frames(16.0, 5),
            constant_frames(16.0, 50),
            vec![16.0], // pass 1: only one sample
            constant_frames(16.0, 50),
        ]);
        let err = run_benchmark(default_config(), &mut source).expect_err("need samples");
        assert!(matches!(
            err,
            BenchmarkError::InsufficientSamples { pass_index: 1, .. }
        ));
    }
}
