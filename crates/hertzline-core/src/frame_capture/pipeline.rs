//! Frame-time pipeline: probe gate -> samples -> AC-1.1 summary (HZ-011).
//!
//! Probe failure / not-possible never invents FPS numbers (AC-1.5--AC-1.7).

use crate::session::{FrameSummary, MeasurementSession};

use super::metrics::compute_stats_ms;
use super::parse::{parse_mangohud_log, parse_presentmon_csv, FrameParseError};
use super::probe::{probe_frame_capture, ProbeReport, ProbeStatus};

/// Host probe outcome applied to a measurement session's frame block.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePipelineResult {
    pub probe: ProbeReport,
    pub summary: FrameSummary,
}

/// In-memory capture pipeline for one measurement pass.
#[derive(Debug, Clone)]
pub struct FramePipeline {
    probe: ProbeReport,
    samples_ms: Vec<f64>,
}

impl FramePipeline {
    /// Open a pipeline after probing the host capture source.
    pub fn open() -> Self {
        Self::open_with_probe(probe_frame_capture())
    }

    /// Open with an injected probe (unit tests / mocks must label fixtures).
    pub fn open_with_probe(probe: ProbeReport) -> Self {
        Self {
            probe,
            samples_ms: Vec::new(),
        }
    }

    pub fn probe(&self) -> &ProbeReport {
        &self.probe
    }

    pub fn sample_count(&self) -> usize {
        self.samples_ms.len()
    }

    /// True only when the host probe reported Available (live capture allowed).
    pub fn is_capture_allowed(&self) -> bool {
        self.probe.status == ProbeStatus::Available
    }

    pub fn push_ms(&mut self, frame_time_ms: f64) {
        if frame_time_ms.is_finite() && frame_time_ms > 0.0 {
            self.samples_ms.push(frame_time_ms);
        }
    }

    pub fn extend_ms(&mut self, frame_times_ms: impl IntoIterator<Item = f64>) {
        for ms in frame_times_ms {
            self.push_ms(ms);
        }
    }

    /// Ingest PresentMon CSV (`MsBetweenPresents`). Requires Available probe.
    pub fn ingest_presentmon_csv(&mut self, csv: &str) -> Result<usize, FramePipelineError> {
        self.require_available()?;
        let times = parse_presentmon_csv(csv)?;
        let n = times.len();
        self.extend_ms(times);
        Ok(n)
    }

    /// Ingest MangoHud log (`frametime` ms). Requires Available probe.
    pub fn ingest_mangohud_log(&mut self, text: &str) -> Result<usize, FramePipelineError> {
        self.require_available()?;
        let times = parse_mangohud_log(text)?;
        let n = times.len();
        self.extend_ms(times);
        Ok(n)
    }

    fn require_available(&self) -> Result<(), FramePipelineError> {
        match self.probe.status {
            ProbeStatus::Available => Ok(()),
            ProbeStatus::Unavailable => Err(FramePipelineError::ProbeUnavailable {
                detail: self.probe.detail.clone(),
            }),
            ProbeStatus::NotPossible => Err(FramePipelineError::NotPossible {
                detail: self.probe.detail.clone(),
            }),
        }
    }

    /// Finish: probe failure -> unavailable; else stats or insufficient samples.
    pub fn finish(self) -> FramePipelineResult {
        let summary = frame_summary_from_probe_and_samples(&self.probe, &self.samples_ms);
        FramePipelineResult {
            probe: self.probe,
            summary,
        }
    }

    /// Finish and write `session.frame`.
    pub fn finish_into(self, session: &mut MeasurementSession) -> FramePipelineResult {
        let result = self.finish();
        session.frame = result.summary.clone();
        result
    }
}

/// Map probe + samples to a frame summary without inventing values.
pub fn frame_summary_from_probe_and_samples(
    probe: &ProbeReport,
    frame_times_ms: &[f64],
) -> FrameSummary {
    match probe.status {
        ProbeStatus::Unavailable => {
            FrameSummary::unavailable(format!("probe unavailable: {}", probe.detail))
        }
        ProbeStatus::NotPossible => {
            FrameSummary::unavailable(format!("not possible: {}", probe.detail))
        }
        ProbeStatus::Available => match compute_stats_ms(frame_times_ms) {
            Some(stats) => FrameSummary::from_stats(&stats),
            None => {
                FrameSummary::unavailable("insufficient positive frame samples (need at least 2)")
            }
        },
    }
}

/// Offline CSV -> summary without host probe (imported logs / mocks).
///
/// Live measurement must use [`FramePipeline`] so AC-1.5--AC-1.7 probe gates apply.
pub fn frame_summary_from_samples_ms(frame_times_ms: &[f64]) -> FrameSummary {
    match compute_stats_ms(frame_times_ms) {
        Some(stats) => FrameSummary::from_stats(&stats),
        None => FrameSummary::unavailable("insufficient positive frame samples (need at least 2)"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramePipelineError {
    ProbeUnavailable { detail: String },
    NotPossible { detail: String },
    Parse(FrameParseError),
}

impl From<FrameParseError> for FramePipelineError {
    fn from(value: FrameParseError) -> Self {
        Self::Parse(value)
    }
}

impl std::fmt::Display for FramePipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProbeUnavailable { detail } => write!(f, "probe unavailable: {detail}"),
            Self::NotPossible { detail } => write!(f, "not possible: {detail}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for FramePipelineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_capture::{CaptureCapability, CaptureMethod, ProbeStatus};
    use crate::session::{MetricF64, SessionMeta};

    fn mock_probe(status: ProbeStatus, detail: &str) -> ProbeReport {
        // mock: synthetic probe for pipeline unit tests (not a live OS probe)
        ProbeReport {
            os: "test".into(),
            method_id: "mock".into(),
            method: CaptureMethod::None,
            capability: CaptureCapability::Supported,
            status,
            detail: detail.into(),
            binary_path: None,
            injects_into_game: false,
            license_spdx: Some("MIT".into()),
        }
    }

    fn assert_available_fps(summary: &FrameSummary) {
        match &summary.avg_fps {
            MetricF64::Available { value, unit } => {
                assert_eq!(unit, "fps");
                assert!(*value > 0.0);
            }
            MetricF64::Unavailable { reason } => panic!("expected available fps, got {reason}"),
        }
        match &summary.low_1pct_fps {
            MetricF64::Available { .. } => {}
            MetricF64::Unavailable { reason } => panic!("expected 1% low, got {reason}"),
        }
        match &summary.low_0_1pct_fps {
            MetricF64::Available { .. } => {}
            MetricF64::Unavailable { reason } => panic!("expected 0.1% low, got {reason}"),
        }
        match &summary.frame_time_variance_ms2 {
            MetricF64::Available { .. } => {}
            MetricF64::Unavailable { reason } => panic!("expected variance, got {reason}"),
        }
    }

    #[test]
    fn mock_available_probe_yields_avg_lows_variance() {
        let mut pipe = FramePipeline::open_with_probe(mock_probe(
            ProbeStatus::Available,
            "mock presentmon available",
        ));
        pipe.extend_ms(vec![16.0_f64; 100]);
        let result = pipe.finish();
        assert_eq!(result.probe.status, ProbeStatus::Available);
        assert_available_fps(&result.summary);
        match result.summary.avg_fps {
            MetricF64::Available { value, .. } => assert!((value - 62.5).abs() < 1e-9),
            _ => unreachable!(),
        }
    }

    #[test]
    fn probe_unavailable_ignores_samples() {
        let mut pipe = FramePipeline::open_with_probe(mock_probe(
            ProbeStatus::Unavailable,
            "PresentMon missing",
        ));
        pipe.extend_ms(vec![16.0, 16.0, 16.0]);
        let result = pipe.finish();
        match result.summary.avg_fps {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("probe unavailable"));
                assert!(reason.contains("PresentMon missing"));
            }
            MetricF64::Available { .. } => panic!("must not invent FPS on probe failure"),
        }
    }

    #[test]
    fn probe_not_possible_mac_style() {
        let pipe = FramePipeline::open_with_probe(mock_probe(
            ProbeStatus::NotPossible,
            "external FPS not possible",
        ));
        let result = pipe.finish();
        match result.summary.avg_fps {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("not possible"));
            }
            MetricF64::Available { .. } => panic!("must not invent FPS"),
        }
    }

    #[test]
    fn ingest_blocked_when_probe_unavailable() {
        let mut pipe =
            FramePipeline::open_with_probe(mock_probe(ProbeStatus::Unavailable, "no binary"));
        let csv = "Application,MsBetweenPresents\ngame.exe,16.0\ngame.exe,16.0\n";
        let err = pipe.ingest_presentmon_csv(csv).expect_err("blocked");
        assert!(matches!(err, FramePipelineError::ProbeUnavailable { .. }));
    }

    #[test]
    fn mock_presentmon_ingest_then_finish_into_session() {
        let mut pipe = FramePipeline::open_with_probe(mock_probe(ProbeStatus::Available, "mock"));
        let csv = "\
Application,ProcessID,MsBetweenPresents
game.exe,1,10.0
game.exe,1,10.0
game.exe,1,10.0
game.exe,1,50.0
";
        let n = pipe.ingest_presentmon_csv(csv).expect("ingest");
        assert_eq!(n, 4);

        let mut session = MeasurementSession::new(SessionMeta {
            session_id: "mock-hz011".into(),
            started_at_unix_ms: 1,
            ended_at_unix_ms: None,
            target_process: Some("game.exe".into()),
            os: "test".into(),
            notes: Some("mock pipeline".into()),
        });
        let result = pipe.finish_into(&mut session);
        assert_available_fps(&session.frame);
        assert_eq!(session.frame, result.summary);
        match &session.frame.low_1pct_fps {
            MetricF64::Available { value, .. } => assert!(*value < 100.0),
            _ => panic!("expected 1% low"),
        }
    }

    #[test]
    fn available_probe_insufficient_samples() {
        let mut pipe = FramePipeline::open_with_probe(mock_probe(ProbeStatus::Available, "mock"));
        pipe.push_ms(16.0);
        let result = pipe.finish();
        match result.summary.avg_fps {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("insufficient"));
            }
            MetricF64::Available { .. } => panic!("need >=2 samples"),
        }
    }

    #[test]
    fn offline_summary_from_samples() {
        let summary = frame_summary_from_samples_ms(&[16.0, 16.0, 16.0]);
        assert_available_fps(&summary);
    }

    #[test]
    fn host_open_finish_never_panics() {
        // automated: real host probe; may be available/unavailable/not_possible
        let result = FramePipeline::open().finish();
        match result.probe.status {
            ProbeStatus::Available | ProbeStatus::Unavailable | ProbeStatus::NotPossible => {}
        }
        match result.summary.avg_fps {
            MetricF64::Unavailable { .. } => {}
            MetricF64::Available { .. } => {
                panic!("open().finish() with no samples must not invent FPS")
            }
        }
    }
}
