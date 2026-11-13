//! Frame-time capture probe and metrics helpers (HZ-006 / D3).
//!
//! Capture backends are selected per OS. Probe failure yields an explicit
//! unavailable / not-possible state -- never invented samples.

mod metrics;
mod probe;

pub use metrics::{FrameTimeStats, compute_stats_ms};
pub use probe::{
    CaptureCapability, CaptureMethod, ProbeReport, ProbeStatus, probe_frame_capture,
};

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
