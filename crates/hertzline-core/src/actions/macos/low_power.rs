//! macOS Low Power Mode warning (HZ-026 / AC-4.21).
//!
//! Official sources (verified 2026-11-14):
//! - NSProcessInfo.isLowPowerModeEnabled / NSProcessInfoPowerStateDidChange
//!   https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/LowPowerMode.html
//!   WWDC25 also documents adapting when LPM is active for games.
//! - pmset(1) `-g` reports `lowpowermode` (Apple power management CLI).
//!
//! AC-4.21: warn when LPM is active and performance-sensitive work is requested.
//! Hertzline does not toggle Low Power Mode (user-owned battery setting).

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "macos.low_power_mode";
pub const SOURCE_URL: &str = "https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/LowPowerMode.html";
pub const WARNING_PERF_WHILE_LPM: &str =
    "Low Power Mode is active; performance-sensitive work may be limited by the OS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowPowerState {
    pub active: bool,
    pub available: bool,
    pub detail: String,
    pub warning: Option<String>,
}

impl LowPowerState {
    pub fn encode(&self) -> String {
        format!(
            "active={};available={};detail={};warning={}",
            if self.active { "1" } else { "0" },
            if self.available { "1" } else { "0" },
            encode_field(&self.detail),
            encode_field(self.warning.as_deref().unwrap_or(""))
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut active = None;
        let mut available = None;
        let mut detail = String::new();
        let mut warning = String::new();
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "active" => active = Some(v.trim() == "1" || v.trim() == "true"),
                "available" => available = Some(v.trim() == "1" || v.trim() == "true"),
                "detail" => detail = decode_field(v),
                "warning" => warning = decode_field(v),
                _ => {}
            }
        }
        Ok(Self {
            active: active.unwrap_or(false),
            available: available.unwrap_or(false),
            detail,
            warning: if warning.is_empty() {
                None
            } else {
                Some(warning)
            },
        })
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/")
        .replace(';', "%3B")
        .replace('=', "%3D")
        .replace('|', "%7C")
}

fn decode_field(s: &str) -> String {
    s.replace("%7C", "|")
        .replace("%3D", "=")
        .replace("%3B", ";")
        .replace('/', "\\")
}

pub trait LowPowerBackend {
    fn read(&self) -> Result<LowPowerState, ActionError>;
}

/// Labeled mock (not a live LPM probe).
#[derive(Debug, Clone)]
pub struct MockLowPowerBackend {
    pub state: LowPowerState,
}

impl LowPowerBackend for MockLowPowerBackend {
    fn read(&self) -> Result<LowPowerState, ActionError> {
        Ok(self.state.clone())
    }
}

impl LowPowerBackend for std::cell::RefCell<MockLowPowerBackend> {
    fn read(&self) -> Result<LowPowerState, ActionError> {
        Ok(self.borrow().state.clone())
    }
}

/// Warn when Low Power Mode is active during performance-sensitive work (AC-4.21).
pub struct LowPowerModeWarningAction<B: LowPowerBackend> {
    backend: B,
    performance_sensitive: bool,
    explanation: ActionExplanation,
}

impl<B: LowPowerBackend> LowPowerModeWarningAction<B> {
    pub fn new(backend: B, performance_sensitive: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report whether macOS Low Power Mode is active (NSProcessInfo.isLowPowerModeEnabled / pmset). Warn when it is on and performance-sensitive work is requested.",
            "Low Power Mode can reduce CPU/GPU performance. Hertzline warns; it does not disable LPM for you.",
            "None for this check: read-only. Changing LPM remains a user Battery setting.",
            "Nothing to undo; this action only reports and warns.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            performance_sensitive,
            explanation,
        }
    }

    pub fn for_performance_work(backend: B) -> Self {
        Self::new(backend, true)
    }

    pub fn report(&self) -> Result<LowPowerState, ActionError> {
        let mut state = self.backend.read()?;
        if self.performance_sensitive && state.available && state.active {
            state.warning = Some(WARNING_PERF_WHILE_LPM.into());
        }
        Ok(state)
    }

    pub fn exposes_toggle(&self) -> bool {
        false
    }
}

impl<B: LowPowerBackend> Action for LowPowerModeWarningAction<B> {
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
        Ok(self.report()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let _ = self.report()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let a = LowPowerState::decode(&actual)?;
        let e = LowPowerState::decode(expected.trim())?;
        if a.active == e.active && a.available == e.available {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

/// Parse `pmset -g` stdout for `lowpowermode` (documented Apple CLI).
pub fn parse_pmset_low_power(stdout: &str) -> Option<bool> {
    for line in stdout.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("lowpowermode")
            .or_else(|| line.strip_prefix("LowPowerMode"))
        else {
            continue;
        };
        let value = rest.trim().trim_start_matches([':', '=']).trim();
        return Some(matches!(value, "1" | "true" | "on" | "YES" | "yes"));
    }
    None
}

#[cfg(target_os = "macos")]
pub mod os {
    use super::*;
    use std::process::Command;

    pub struct OsLowPowerBackend;

    impl LowPowerBackend for OsLowPowerBackend {
        fn read(&self) -> Result<LowPowerState, ActionError> {
            // Prefer pmset (no objc dependency). Conceptual API: NSProcessInfo.
            let output = Command::new("pmset")
                .arg("-g")
                .output()
                .map_err(|e| ActionError::ApplyFailed(format!("pmset -g failed: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            match parse_pmset_low_power(&stdout) {
                Some(active) => Ok(LowPowerState {
                    active,
                    available: true,
                    detail:
                        "pmset -g lowpowermode (NSProcessInfo.isLowPowerModeEnabled equivalent)"
                            .into(),
                    warning: None,
                }),
                None => Ok(LowPowerState {
                    active: false,
                    available: false,
                    detail: "pmset -g did not report lowpowermode; treat as unavailable".into(),
                    warning: None,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    #[test]
    fn parse_pmset_samples() {
        assert_eq!(
            parse_pmset_low_power("lowpowermode         1\n"),
            Some(true)
        );
        assert_eq!(parse_pmset_low_power("  lowpowermode 0\n"), Some(false));
        assert_eq!(parse_pmset_low_power("something else\n"), None);
    }

    #[test]
    fn warns_when_lpm_active_and_perf_requested() {
        let action = LowPowerModeWarningAction::for_performance_work(MockLowPowerBackend {
            state: LowPowerState {
                active: true,
                available: true,
                detail: "mock lpm on".into(),
                warning: None,
            },
        });
        let report = action.report().expect("mock");
        assert!(report.active);
        assert_eq!(report.warning.as_deref(), Some(WARNING_PERF_WHILE_LPM));
        assert!(!action.exposes_toggle());
    }

    #[test]
    fn no_warning_when_lpm_off() {
        let action = LowPowerModeWarningAction::for_performance_work(MockLowPowerBackend {
            state: LowPowerState {
                active: false,
                available: true,
                detail: "mock lpm off".into(),
                warning: None,
            },
        });
        assert!(action.report().expect("mock").warning.is_none());
    }

    #[test]
    fn apply_restore_verify_round_trip() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockLowPowerBackend {
            state: LowPowerState {
                active: true,
                available: true,
                detail: "mock".into(),
                warning: None,
            },
        });
        let mut action = LowPowerModeWarningAction::for_performance_work(backend);
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        assert!(entry.before.contains("active=1"));
        assert!(entry.before.contains("warning="));
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        assert!(action.verify(&entry.before).expect("verify").is_match());
    }
}
