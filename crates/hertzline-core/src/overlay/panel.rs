//! Minimal FPS / frame-time values for the Hertzline-owned overlay panel (AC-6.1).
//!
//! Values come only from measured frame samples (or an explicit unavailable state).
//! Never invents FPS.

use crate::frame_capture::{compute_stats_ms, FrameTimeStats};
use serde::{Deserialize, Serialize};

/// One overlay panel reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPanel {
    pub status: OverlayPanelStatus,
    pub avg_fps: Option<f64>,
    pub avg_frame_time_ms: Option<f64>,
    pub low_1pct_fps: Option<f64>,
    pub sample_count: usize,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPanelStatus {
    Available,
    Unavailable,
    Disabled,
}

impl OverlayPanel {
    pub fn disabled(note: impl Into<String>) -> Self {
        Self {
            status: OverlayPanelStatus::Disabled,
            avg_fps: None,
            avg_frame_time_ms: None,
            low_1pct_fps: None,
            sample_count: 0,
            note: note.into(),
        }
    }

    pub fn unavailable(note: impl Into<String>) -> Self {
        Self {
            status: OverlayPanelStatus::Unavailable,
            avg_fps: None,
            avg_frame_time_ms: None,
            low_1pct_fps: None,
            sample_count: 0,
            note: note.into(),
        }
    }

    pub fn from_stats(stats: &FrameTimeStats) -> Self {
        Self {
            status: OverlayPanelStatus::Available,
            avg_fps: Some(stats.avg_fps),
            avg_frame_time_ms: Some(stats.avg_frame_time_ms),
            low_1pct_fps: Some(stats.low_1pct_fps),
            sample_count: stats.sample_count,
            note: "from measured frame times".into(),
        }
    }
}

/// Build a panel from frame times in milliseconds.
///
/// Returns unavailable when samples are insufficient (never invents numbers).
pub fn panel_from_frame_times_ms(frame_times_ms: &[f64]) -> OverlayPanel {
    match compute_stats_ms(frame_times_ms) {
        Some(stats) => OverlayPanel::from_stats(&stats),
        None => OverlayPanel::unavailable(
            "not enough positive frame-time samples for FPS / frame-time overlay",
        ),
    }
}

/// Panel when policy disabled the overlay.
pub fn panel_when_disabled(decision_message: &str) -> OverlayPanel {
    OverlayPanel::disabled(decision_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_constant_frames_produce_fps() {
        // mock: 60 identical 16.667 ms frames
        let frames = vec![1000.0 / 60.0; 60];
        let panel = panel_from_frame_times_ms(&frames);
        assert_eq!(panel.status, OverlayPanelStatus::Available);
        let fps = panel.avg_fps.expect("fps");
        assert!((fps - 60.0).abs() < 0.1);
        assert!(panel.avg_frame_time_ms.unwrap() > 0.0);
        assert_eq!(panel.sample_count, 60);
    }

    #[test]
    fn empty_samples_unavailable() {
        let panel = panel_from_frame_times_ms(&[]);
        assert_eq!(panel.status, OverlayPanelStatus::Unavailable);
        assert!(panel.avg_fps.is_none());
    }

    #[test]
    fn disabled_panel_has_no_numbers() {
        let panel = panel_when_disabled("globally disabled");
        assert_eq!(panel.status, OverlayPanelStatus::Disabled);
        assert!(panel.avg_fps.is_none());
    }
}
