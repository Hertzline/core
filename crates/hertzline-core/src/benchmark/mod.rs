//! Repeatable before/after benchmark engine (HZ-014 / AC-2.1, AC-2.2).
//!
//! Fixed duration, warm-up discard, at least 3 measurement passes.
//! Aggregates median and spread (IQR + min--max) of per-pass FPS / lows.
//! Noise-band verdict: HZ-015 / AC-2.3, AC-2.4.

mod engine;
mod stats;
mod verdict;

pub use engine::{
    run_benchmark, BenchmarkConfig, BenchmarkError, BenchmarkPass, BenchmarkResult,
    FrameWindowSource, MIN_PASS_COUNT,
};
pub use stats::{median_f64, MetricSpread};
pub use verdict::{
    combined_noise_band, evaluate_benchmarks, evaluate_benchmarks_metric, evaluate_spreads,
    noise_band, ChangeVerdict, VerdictMetric, VerdictReport,
};
