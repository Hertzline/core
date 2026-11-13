//! Repeatable before/after benchmark engine (HZ-014 / AC-2.1, AC-2.2).
//!
//! Fixed duration, warm-up discard, at least 3 measurement passes.
//! Aggregates median and spread (IQR + min--max) of per-pass FPS / lows.
//! Noise-band verdict is HZ-015.

mod engine;
mod stats;

pub use engine::{
    run_benchmark, BenchmarkConfig, BenchmarkError, BenchmarkPass, BenchmarkResult,
    FrameWindowSource, MIN_PASS_COUNT,
};
pub use stats::{median_f64, MetricSpread};
