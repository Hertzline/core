//! macOS monitoring of supported metrics only (HZ-025 / AC-4.18, AC-1.7).
//!
//! Report-only: never invents values. Unsupported metrics stay
//! available / unavailable / not_possible with an explicit reason.
//!
//! Sources:
//! - Sensor probe: sysinfo 0.40.3 MIT (D4)
//! - Frame probe: macOS external FPS not possible; display refresh partial (D3)
//!   https://developer.apple.com/documentation/corevideo/cvdisplaylink

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};
use crate::actions::macos::capability::{MacOsCapabilitySurface, MACOS_HONESTY_BANNER};
use crate::frame_capture::{CaptureCapability, ProbeStatus};
use crate::sensors::{MetricAvailability, MetricId};

pub const ACTION_ID: &str = "macos.monitor";
pub const SOURCE_CVDISPLAYLINK: &str =
    "https://developer.apple.com/documentation/corevideo/cvdisplaylink";
pub const SOURCE_SYSINFO_GPU: &str = "https://docs.rs/sysinfo/0.40.3/sysinfo/struct.Gpu.html";

/// One monitored metric with an honest availability label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitoredMetric {
    pub id: String,
    pub availability: String,
    pub detail: String,
}

impl MonitoredMetric {
    pub fn encode(&self) -> String {
        format!(
            "id={};avail={};detail={}",
            encode_field(&self.id),
            encode_field(&self.availability),
            encode_field(&self.detail)
        )
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/")
        .replace(';', "%3B")
        .replace('=', "%3D")
        .replace('|', "%7C")
}

/// Snapshot returned by the monitor backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSnapshot {
    pub honesty_banner: String,
    pub frame_capability: String,
    pub frame_status: String,
    pub frame_detail: String,
    pub metrics: Vec<MonitoredMetric>,
}

impl MonitorSnapshot {
    pub fn encode(&self) -> String {
        let mut parts = Vec::with_capacity(self.metrics.len() + 4);
        parts.push(format!("honesty={}", encode_field(&self.honesty_banner)));
        parts.push(format!(
            "frame_cap={};frame_status={};frame_detail={}",
            encode_field(&self.frame_capability),
            encode_field(&self.frame_status),
            encode_field(&self.frame_detail)
        ));
        parts.push(format!("metrics={}", self.metrics.len()));
        for m in &self.metrics {
            parts.push(m.encode());
        }
        parts.join("|")
    }
}

pub trait MonitorBackend {
    fn snapshot(&self) -> Result<MonitorSnapshot, ActionError>;
}

/// Labeled mock for unit tests (not a live macOS probe).
#[derive(Debug, Clone)]
pub struct MockMonitorBackend {
    pub snapshot: MonitorSnapshot,
}

impl MonitorBackend for std::cell::RefCell<MockMonitorBackend> {
    fn snapshot(&self) -> Result<MonitorSnapshot, ActionError> {
        Ok(self.borrow().snapshot.clone())
    }
}

impl MonitorBackend for MockMonitorBackend {
    fn snapshot(&self) -> Result<MonitorSnapshot, ActionError> {
        Ok(self.snapshot.clone())
    }
}

/// Report-only macOS monitoring action (AC-4.18). Apply re-reads; no mutation.
pub struct MacOsMonitorAction<B: MonitorBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: MonitorBackend> MacOsMonitorAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report macOS metrics that public APIs expose (CPU, RAM, GPU usage when available, network). Unsupported metrics such as external game FPS and dedicated VRAM are labeled not possible.",
            "Shows what Hertzline can actually measure on this Mac. No performance claim without a local session.",
            "None for this check: read-only. Does not change system settings.",
            "Nothing to undo; this action only reports.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    pub fn report(&self) -> Result<MonitorSnapshot, ActionError> {
        self.backend.snapshot()
    }

    pub fn capability_surface(&self) -> MacOsCapabilitySurface {
        MacOsCapabilitySurface::load()
    }
}

impl<B: MonitorBackend> Action for MacOsMonitorAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        false
    }

    fn read_state(&self) -> Result<String, ActionError> {
        Ok(self.backend.snapshot()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let _ = self.backend.snapshot()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        if actual == expected
            || (actual.contains("honesty=")
                && expected.contains("honesty=")
                && actual.contains("frame_status=")
                && expected.contains("frame_status="))
        {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

fn availability_label(a: MetricAvailability) -> &'static str {
    match a {
        MetricAvailability::Available => "available",
        MetricAvailability::Partial => "partial",
        MetricAvailability::Unavailable => "unavailable",
        MetricAvailability::NotPossible => "not_possible",
    }
}

fn frame_capability_label(c: CaptureCapability) -> &'static str {
    match c {
        CaptureCapability::Supported => "supported",
        CaptureCapability::Partial => "partial",
        CaptureCapability::NotPossible => "not_possible",
    }
}

fn frame_status_label(s: ProbeStatus) -> &'static str {
    match s {
        ProbeStatus::Available => "available",
        ProbeStatus::Unavailable => "unavailable",
        ProbeStatus::NotPossible => "not_possible",
    }
}

/// Build a monitor snapshot from live frame + sensor probes (any host; macOS labels apply).
pub fn snapshot_from_host_probes() -> MonitorSnapshot {
    let frame = crate::frame_capture::probe_frame_capture();
    let sensors = crate::sensors::probe_sensors();
    let metrics: Vec<MonitoredMetric> = sensors
        .metrics
        .into_iter()
        .map(|m| MonitoredMetric {
            id: metric_id_snake(m.id).into(),
            availability: availability_label(m.availability).into(),
            detail: m.detail,
        })
        .collect();

    MonitorSnapshot {
        honesty_banner: MACOS_HONESTY_BANNER.into(),
        frame_capability: frame_capability_label(frame.capability).into(),
        frame_status: frame_status_label(frame.status).into(),
        frame_detail: frame.detail,
        metrics,
    }
}

fn metric_id_snake(id: MetricId) -> &'static str {
    match id {
        MetricId::CpuUsage => "cpu_usage",
        MetricId::CpuFrequency => "cpu_frequency",
        MetricId::RamUsage => "ram_usage",
        MetricId::GpuUsage => "gpu_usage",
        MetricId::VramUsage => "vram_usage",
        MetricId::Temperature => "temperature",
        MetricId::GpuClock => "gpu_clock",
        MetricId::ThrottleFlag => "throttle_flag",
        MetricId::DiskThroughput => "disk_throughput",
        MetricId::DiskQueue => "disk_queue",
    }
}

#[cfg(target_os = "macos")]
pub mod os {
    use super::*;

    pub struct OsMonitorBackend;

    impl MonitorBackend for OsMonitorBackend {
        fn snapshot(&self) -> Result<MonitorSnapshot, ActionError> {
            Ok(snapshot_from_host_probes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    fn labeled_mock_snapshot() -> MonitorSnapshot {
        // mock: labeled fixture, not a live macOS probe
        MonitorSnapshot {
            honesty_banner: MACOS_HONESTY_BANNER.into(),
            frame_capability: "partial".into(),
            frame_status: "not_possible".into(),
            frame_detail: "external process frame times not possible via supported public APIs"
                .into(),
            metrics: vec![
                MonitoredMetric {
                    id: "cpu_usage".into(),
                    availability: "available".into(),
                    detail: "mock cpu".into(),
                },
                MonitoredMetric {
                    id: "vram_usage".into(),
                    availability: "not_possible".into(),
                    detail: "vram not possible on macOS via sysinfo (shared memory)".into(),
                },
            ],
        }
    }

    #[test]
    fn mock_report_labels_not_possible_vram_and_fps() {
        let action = MacOsMonitorAction::new(MockMonitorBackend {
            snapshot: labeled_mock_snapshot(),
        });
        let snap = action.report().expect("mock report");
        assert!(snap.honesty_banner.contains("far fewer tunables"));
        assert_eq!(snap.frame_status, "not_possible");
        let vram = snap
            .metrics
            .iter()
            .find(|m| m.id == "vram_usage")
            .expect("vram");
        assert_eq!(vram.availability, "not_possible");
        assert!(!action.explanation().what.is_empty());
        assert_eq!(
            action.explanation().local_measurement.message(),
            "no local measurement exists yet"
        );
    }

    #[test]
    fn mock_apply_restore_verify_round_trip() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockMonitorBackend {
            snapshot: labeled_mock_snapshot(),
        });
        let mut action = MacOsMonitorAction::new(backend);
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        let verify = action.verify(&entry.before).expect("verify");
        assert!(verify.is_match());
    }

    #[test]
    fn capability_surface_never_toggles_not_possible() {
        let action = MacOsMonitorAction::new(MockMonitorBackend {
            snapshot: labeled_mock_snapshot(),
        });
        for c in action.capability_surface().not_possible() {
            assert!(!c.exposes_toggle, "{} must not toggle", c.id);
        }
    }

    #[test]
    fn host_probe_snapshot_includes_honesty() {
        // automated: live host probe on whatever OS; values may be unavailable
        let snap = snapshot_from_host_probes();
        assert!(snap.honesty_banner.contains("far fewer tunables"));
        assert!(!snap.frame_status.is_empty());
        assert!(!snap.metrics.is_empty());
        assert!(snap.encode().contains("honesty="));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_os_backend_marks_external_fps_not_possible() {
        let snap = os::OsMonitorBackend.snapshot().expect("os snapshot");
        assert_eq!(snap.frame_status, "not_possible");
        assert_eq!(snap.frame_capability, "partial");
        let vram = snap.metrics.iter().find(|m| m.id == "vram_usage");
        if let Some(v) = vram {
            assert!(
                v.availability == "not_possible" || v.availability == "unavailable",
                "vram must not be invented as available; got {}",
                v.availability
            );
        }
    }
}
