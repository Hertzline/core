//! Background resource hogs (AC-5.2).
//!
//! Report CPU / RAM / disk offenders by name. Never kills a process unless the
//! caller passes an explicit confirm flag (still refused here -- kills go through
//! the background-apps action with a user-chosen PID).

use std::thread;
use std::time::Duration;

use sysinfo::{ProcessesToUpdate, System};

use crate::action::ActionError;

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceHog {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub ram_bytes: u64,
    /// Per-process disk delta when sysinfo exposes it; otherwise None.
    pub disk_read_bytes: Option<u64>,
    pub disk_write_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceHogReport {
    pub host_os: String,
    pub note: String,
    pub hogs: Vec<ResourceHog>,
}

/// Pluggable sampler for unit tests (mocks must be labeled).
pub trait ResourceHogsBackend {
    fn sample(&mut self) -> Result<Vec<ResourceHog>, ActionError>;
}

/// Live sysinfo process scan (MIT, D4). Requires a short refresh for CPU %.
pub struct SysinfoResourceHogs {
    cpu_interval: Duration,
}

impl Default for SysinfoResourceHogs {
    fn default() -> Self {
        Self {
            cpu_interval: sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)),
        }
    }
}

impl SysinfoResourceHogs {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ResourceHogsBackend for SysinfoResourceHogs {
    fn sample(&mut self) -> Result<Vec<ResourceHog>, ActionError> {
        let mut system = System::new()
            .map_err(|e| ActionError::ApplyFailed(format!("sysinfo System::new failed: {e}")))?;
        system.refresh_processes(ProcessesToUpdate::All, true);
        thread::sleep(self.cpu_interval);
        system.refresh_processes(ProcessesToUpdate::All, true);

        let mut hogs = Vec::new();
        for (pid, process) in system.processes() {
            let name = process.name().to_string_lossy().trim().to_string();
            if name.is_empty() {
                continue;
            }
            let disk = process.disk_usage();
            hogs.push(ResourceHog {
                pid: pid.as_u32(),
                name,
                cpu_percent: process.cpu_usage(),
                ram_bytes: process.memory(),
                // Interval deltas from sysinfo; 0 means idle in this window, not invented.
                disk_read_bytes: Some(disk.read_bytes),
                disk_write_bytes: Some(disk.written_bytes),
            });
        }
        Ok(hogs)
    }
}

/// Labeled mock backend (not live OS process list).
#[derive(Debug, Clone)]
pub struct MockResourceHogsBackend {
    pub hogs: Vec<ResourceHog>,
}

impl ResourceHogsBackend for MockResourceHogsBackend {
    fn sample(&mut self) -> Result<Vec<ResourceHog>, ActionError> {
        Ok(self.hogs.clone())
    }
}

fn rank_hogs(mut hogs: Vec<ResourceHog>, limit: usize) -> Vec<ResourceHog> {
    hogs.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.ram_bytes.cmp(&a.ram_bytes))
    });
    if hogs.len() > limit {
        hogs.truncate(limit);
    }
    hogs
}

/// List top offenders. Never kills (AC-5.2).
pub fn list_resource_hogs<B: ResourceHogsBackend>(
    backend: &mut B,
    limit: usize,
) -> Result<ResourceHogReport, ActionError> {
    let limit = limit.clamp(1, 50);
    let hogs = rank_hogs(backend.sample()?, limit);
    Ok(ResourceHogReport {
        host_os: std::env::consts::OS.to_string(),
        note: "Report only. Hertzline never kills a process unless you confirm a user-chosen PID in Actions (AC-5.2).".into(),
        hogs,
    })
}

/// Explicit guard used by UI/API before any kill path (AC-5.2).
pub fn refuse_kill_without_confirm(confirm: bool) -> Result<(), ActionError> {
    if confirm {
        Err(ActionError::ApplyFailed(
            "everyday mode does not kill processes; use the background-apps action with a user-chosen PID after confirm".into(),
        ))
    } else {
        Err(ActionError::ApplyFailed(
            "refusing to kill: confirm required and everyday mode never silent-kills (AC-5.2)"
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labeled_mock_ranks_by_cpu_then_ram() {
        // labeled mock: not live processes
        let mut backend = MockResourceHogsBackend {
            hogs: vec![
                ResourceHog {
                    pid: 1,
                    name: "low".into(),
                    cpu_percent: 1.0,
                    ram_bytes: 9_000_000_000,
                    disk_read_bytes: None,
                    disk_write_bytes: None,
                },
                ResourceHog {
                    pid: 2,
                    name: "hot".into(),
                    cpu_percent: 80.0,
                    ram_bytes: 100,
                    disk_read_bytes: Some(10),
                    disk_write_bytes: Some(20),
                },
            ],
        };
        let report = list_resource_hogs(&mut backend, 10).expect("list");
        assert_eq!(report.hogs[0].name, "hot");
        assert_eq!(report.hogs[1].name, "low");
        assert!(report.note.contains("never kills"));
    }

    #[test]
    fn refuse_kill_always_errors() {
        let without = refuse_kill_without_confirm(false).unwrap_err();
        let with = refuse_kill_without_confirm(true).unwrap_err();
        assert!(without.to_string().contains("silent"));
        assert!(with.to_string().contains("background-apps"));
    }

    #[test]
    fn host_sysinfo_sample_ok() {
        let mut backend = SysinfoResourceHogs::new();
        let report = list_resource_hogs(&mut backend, 5).expect("sysinfo hogs");
        assert!(report.hogs.len() <= 5);
        assert!(!report.host_os.is_empty());
    }
}
