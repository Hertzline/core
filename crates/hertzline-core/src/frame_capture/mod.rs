//! Frame-time capture: probe, CSV ingest, and pipeline (HZ-006 / HZ-011).
//!
//! Capture backends are selected per OS. Probe failure yields an explicit
//! unavailable / not-possible state -- never invented samples.

mod metrics;
mod parse;
mod pipeline;
mod probe;

pub use metrics::{compute_stats_ms, FrameTimeStats};
pub use parse::{parse_mangohud_log, parse_presentmon_csv, FrameParseError};
pub use pipeline::{
    frame_summary_from_probe_and_samples, frame_summary_from_samples_ms, FramePipeline,
    FramePipelineError, FramePipelineResult,
};
pub use probe::{probe_frame_capture, CaptureCapability, CaptureMethod, ProbeReport, ProbeStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_host_platform_report() {
        let report = probe_frame_capture();
        assert!(!report.os.is_empty());
        assert!(!report.method_id.is_empty());
        match report.status {
            ProbeStatus::Available | ProbeStatus::Unavailable | ProbeStatus::NotPossible => {}
        }
    }
}
