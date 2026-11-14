//! Startup impact review (AC-5.1).
//!
//! Lists startup entries with estimated (never invented timed) impact and notes
//! that toggles are reversible via the Windows startup action where the OS allows.

use crate::action::ActionError;
use crate::actions::windows::StartupEntry;

/// How impact was derived -- never invent milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupImpactKind {
    /// No local timing exists yet (honest default).
    NotMeasured,
    /// Qualitative estimate from enable/disable state only.
    Estimated,
}

impl StartupImpactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotMeasured => "not_measured",
            Self::Estimated => "estimated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupImpactSource {
    WindowsRunKey,
    Unavailable,
}

impl StartupImpactSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WindowsRunKey => "windows_run_key",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupImpactRow {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub impact_kind: StartupImpactKind,
    pub impact_summary: String,
    pub reversible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupImpactReport {
    pub host_os: String,
    pub source: StartupImpactSource,
    pub note: String,
    pub rows: Vec<StartupImpactRow>,
}

/// Qualitative estimate only -- no fabricated ms/boot cost (RULES.md).
pub fn estimate_startup_impact(entry: &StartupEntry) -> (StartupImpactKind, String) {
    if entry.enabled {
        (
            StartupImpactKind::Estimated,
            "Runs at sign-in; impact not timed on this machine".into(),
        )
    } else {
        (
            StartupImpactKind::Estimated,
            "Disabled at sign-in; no startup cost while disabled".into(),
        )
    }
}

#[cfg(any(windows, test))]
fn rows_from_entries(entries: Vec<StartupEntry>) -> Vec<StartupImpactRow> {
    entries
        .into_iter()
        .map(|entry| {
            let (impact_kind, impact_summary) = estimate_startup_impact(&entry);
            StartupImpactRow {
                name: entry.name,
                command: entry.command,
                enabled: entry.enabled,
                impact_kind,
                impact_summary,
                reversible: true,
            }
        })
        .collect()
}

/// Build the startup impact review for the current host (AC-5.1).
pub fn list_startup_impact() -> Result<StartupImpactReport, ActionError> {
    let host_os = std::env::consts::OS.to_string();
    #[cfg(windows)]
    {
        use crate::actions::windows::startup::os::OsStartupAppsBackend;
        use crate::actions::windows::StartupAppsBackend;
        let entries = OsStartupAppsBackend.list()?;
        Ok(StartupImpactReport {
            host_os,
            source: StartupImpactSource::WindowsRunKey,
            note: "HKCU Run entries only. Toggle enable/disable is reversible (AC-4.5 / AC-5.1). Impact is estimated from state, not timed.".into(),
            rows: rows_from_entries(entries),
        })
    }
    #[cfg(not(windows))]
    {
        Ok(StartupImpactReport {
            host_os,
            source: StartupImpactSource::Unavailable,
            note: "Startup impact review uses Windows HKCU Run keys. On this OS, use the system login items UI; Hertzline does not invent a toggle.".into(),
            rows: Vec::new(),
        })
    }
}

/// Test helper: build a report from labeled mock entries (not live OS).
#[cfg(test)]
pub fn report_from_mock_entries(entries: Vec<StartupEntry>) -> StartupImpactReport {
    StartupImpactReport {
        host_os: "mock".into(),
        source: StartupImpactSource::WindowsRunKey,
        note: "labeled mock startup impact (not live OS)".into(),
        rows: rows_from_entries(entries),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_distinguishes_enabled_vs_disabled() {
        let on = StartupEntry {
            name: "One".into(),
            command: "a.exe".into(),
            enabled: true,
        };
        let off = StartupEntry {
            name: "Two".into(),
            command: "b.exe".into(),
            enabled: false,
        };
        let (k1, s1) = estimate_startup_impact(&on);
        let (k2, s2) = estimate_startup_impact(&off);
        assert_eq!(k1, StartupImpactKind::Estimated);
        assert_eq!(k2, StartupImpactKind::Estimated);
        assert!(s1.contains("sign-in"));
        assert!(s2.contains("Disabled"));
        assert!(!s1.chars().any(|c| c.is_ascii_digit()) || !s1.contains("ms"));
    }

    #[test]
    fn labeled_mock_report_lists_reversible_rows() {
        // labeled mock: not a real Run key
        let report = report_from_mock_entries(vec![
            StartupEntry {
                name: "MockApp".into(),
                command: "C:/mock/app.exe".into(),
                enabled: true,
            },
            StartupEntry {
                name: "OffApp".into(),
                command: "C:/mock/off.exe".into(),
                enabled: false,
            },
        ]);
        assert_eq!(report.rows.len(), 2);
        assert!(report.rows.iter().all(|r| r.reversible));
        assert_eq!(report.rows[0].impact_kind.as_str(), "estimated");
        assert!(report.note.contains("mock"));
    }

    #[test]
    fn host_list_startup_impact_returns_ok() {
        let report = list_startup_impact().expect("startup impact");
        assert!(!report.host_os.is_empty());
        assert!(!report.note.is_empty());
        if cfg!(windows) {
            assert_eq!(report.source, StartupImpactSource::WindowsRunKey);
        } else {
            assert_eq!(report.source, StartupImpactSource::Unavailable);
            assert!(report.rows.is_empty());
        }
    }
}
