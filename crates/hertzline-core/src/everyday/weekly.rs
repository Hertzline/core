//! Weekly health report (AC-5.6).
//!
//! Summarizes archived measurements plus applied actions and rollbacks from the
//! snapshot change log for the past 7 days.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::snapshot::{ChangeLogEntry, RestoreStatus, SnapshotStore};

/// Seven days in milliseconds.
pub const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub const ARCHIVE_SCHEMA_VERSION: u32 = 1;

/// Compact measurement summary stored for weekly reports (not full session export).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasurementSummary {
    pub session_id: String,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: Option<i64>,
    pub target_process: Option<String>,
    pub os: String,
    /// Honest frame status string (available / unavailable / not_possible), never invented FPS.
    pub frame_status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ArchiveDocument {
    schema_version: u32,
    entries: Vec<MeasurementSummary>,
}

/// On-disk archive of measurement summaries.
#[derive(Debug, Clone, Default)]
pub struct MeasurementArchive {
    entries: Vec<MeasurementSummary>,
    path: Option<PathBuf>,
}

impl MeasurementArchive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if path.exists() {
            return Self::load_from_path(&path);
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let mut archive = Self::new();
        archive.path = Some(path);
        archive.persist()?;
        Ok(archive)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let doc: ArchiveDocument = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if doc.schema_version != ARCHIVE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported measurement archive schema {}",
                doc.schema_version
            ));
        }
        Ok(Self {
            entries: doc.entries,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn entries(&self) -> &[MeasurementSummary] {
        &self.entries
    }

    pub fn append(&mut self, summary: MeasurementSummary) -> Result<(), String> {
        self.entries.push(summary);
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let doc = ArchiveDocument {
            schema_version: ARCHIVE_SCHEMA_VERSION,
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}

/// Open the default archive path (or env override).
pub fn load_measurement_archive() -> Result<MeasurementArchive, String> {
    MeasurementArchive::open(crate::paths::measurement_archive_path())
}

/// Append one summary to the default archive.
pub fn append_measurement_summary(summary: MeasurementSummary) -> Result<(), String> {
    let mut archive = load_measurement_archive()?;
    archive.append(summary)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyHealthReport {
    pub window_start_unix_ms: i64,
    pub window_end_unix_ms: i64,
    pub measurements: Vec<MeasurementSummary>,
    pub applied_actions: Vec<ChangeLogEntry>,
    pub rollbacks: Vec<ChangeLogEntry>,
    pub note: String,
}

/// Generate the past-7-days health report (AC-5.6).
pub fn generate_weekly_report(
    store: &SnapshotStore,
    archive: &MeasurementArchive,
    now_unix_ms: i64,
) -> WeeklyHealthReport {
    let start = now_unix_ms.saturating_sub(WEEK_MS);
    let measurements = archive
        .entries()
        .iter()
        .filter(|m| m.started_at_unix_ms >= start && m.started_at_unix_ms <= now_unix_ms)
        .cloned()
        .collect::<Vec<_>>();

    let mut applied_actions = Vec::new();
    let mut rollbacks = Vec::new();
    for entry in store.change_log() {
        if entry.time_unix_ms < start || entry.time_unix_ms > now_unix_ms {
            continue;
        }
        match entry.restore_status {
            RestoreStatus::Outstanding => applied_actions.push(entry.clone()),
            RestoreStatus::Restored => {
                applied_actions.push(entry.clone());
                rollbacks.push(entry.clone());
            }
        }
    }

    WeeklyHealthReport {
        window_start_unix_ms: start,
        window_end_unix_ms: now_unix_ms,
        measurements,
        applied_actions,
        rollbacks,
        note: "Weekly health report for the past 7 days. Measurements come from archived sessions; actions and rollbacks come from the local change log.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log, MockToggleAction};
    use crate::snapshot::SnapshotStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hz033-weekly-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn weekly_report_filters_window_and_lists_rollbacks() {
        let dir = temp_dir("report");
        let mut store = SnapshotStore::open(dir.join("snapshots.json")).unwrap();
        let mut action = MockToggleAction::new("mock_weekly", "off", "on");
        let now = 1_000_000_000_i64;
        let entry = apply_with_snapshot(&mut action, &mut store, now - 1_000).unwrap();
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        // Outside window -- ignored
        let mut old = MockToggleAction::new("mock_old", "a", "b");
        let _ = apply_with_snapshot(&mut old, &mut store, now - WEEK_MS - 5).unwrap();

        let mut archive = MeasurementArchive::open(dir.join("archive.json")).unwrap();
        archive
            .append(MeasurementSummary {
                session_id: "in".into(),
                started_at_unix_ms: now - 50,
                ended_at_unix_ms: Some(now - 40),
                target_process: Some("game.exe".into()),
                os: "windows".into(),
                frame_status: "unavailable".into(),
                notes: None,
            })
            .unwrap();
        archive
            .append(MeasurementSummary {
                session_id: "out".into(),
                started_at_unix_ms: now - WEEK_MS - 10,
                ended_at_unix_ms: None,
                target_process: None,
                os: "windows".into(),
                frame_status: "unavailable".into(),
                notes: None,
            })
            .unwrap();

        let report = generate_weekly_report(&store, &archive, now);
        assert_eq!(report.measurements.len(), 1);
        assert_eq!(report.measurements[0].session_id, "in");
        assert_eq!(report.applied_actions.len(), 1);
        assert_eq!(report.rollbacks.len(), 1);
        assert_eq!(report.rollbacks[0].action, "mock_weekly");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_round_trip() {
        let dir = temp_dir("archive");
        let path = dir.join("archive.json");
        let mut archive = MeasurementArchive::open(&path).unwrap();
        archive
            .append(MeasurementSummary {
                session_id: "s1".into(),
                started_at_unix_ms: 10,
                ended_at_unix_ms: Some(20),
                target_process: None,
                os: "mock".into(),
                frame_status: "unavailable".into(),
                notes: Some("labeled mock".into()),
            })
            .unwrap();
        let loaded = MeasurementArchive::load_from_path(&path).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].session_id, "s1");
        let _ = std::fs::remove_dir_all(dir);
    }
}
