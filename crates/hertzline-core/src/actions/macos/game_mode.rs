//! macOS Game Mode status (HZ-026 / AC-4.19).
//!
//! Official sources (verified 2026-11-14):
//! - WWDC25 "Level up your games": Game Mode enables when an eligible game
//!   goes full screen; games opt in with Info.plist `LSSupportsGameMode`.
//!   https://developer.apple.com/videos/play/wwdc2025/209/
//! - No public API documents reading or toggling system Game Mode status.
//!   Private tools (`gamepolicyctl`) and unified-log scraping are out of scope.
//!
//! AC-4.19: report status where the OS exposes it; no fake toggle if unsupported.
//! Hertzline therefore always labels status `not_possible` and never exposes
//! an apply switch for Game Mode on macOS.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "macos.game_mode_status";
pub const SOURCE_URL: &str = "https://developer.apple.com/videos/play/wwdc2025/209/";
pub const STATUS_NOT_POSSIBLE: &str = "not_possible";
pub const DETAIL_NO_PUBLIC_API: &str =
    "no public API to query or toggle macOS Game Mode status; LSSupportsGameMode is a game Info.plist opt-in only";

/// Read Game Mode status. Always reports not_possible via public APIs.
pub trait MacOsGameModeBackend {
    fn status(&self) -> Result<MacOsGameModeReport, ActionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsGameModeReport {
    pub status: String,
    pub detail: String,
}

impl MacOsGameModeReport {
    pub fn encode(&self) -> String {
        format!(
            "status={};detail={}",
            encode_field(&self.status),
            encode_field(&self.detail)
        )
    }

    pub fn not_possible() -> Self {
        Self {
            status: STATUS_NOT_POSSIBLE.into(),
            detail: DETAIL_NO_PUBLIC_API.into(),
        }
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/")
        .replace(';', "%3B")
        .replace('=', "%3D")
        .replace('|', "%7C")
}

/// Labeled mock (not a live Game Mode probe).
#[derive(Debug, Clone)]
pub struct MockMacOsGameModeBackend {
    pub report: MacOsGameModeReport,
}

impl MacOsGameModeBackend for MockMacOsGameModeBackend {
    fn status(&self) -> Result<MacOsGameModeReport, ActionError> {
        Ok(self.report.clone())
    }
}

impl MacOsGameModeBackend for std::cell::RefCell<MockMacOsGameModeBackend> {
    fn status(&self) -> Result<MacOsGameModeReport, ActionError> {
        Ok(self.borrow().report.clone())
    }
}

/// Report-only Game Mode status (AC-4.19). Never mutates; never toggles.
pub struct MacOsGameModeStatusAction<B: MacOsGameModeBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: MacOsGameModeBackend> MacOsGameModeStatusAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report whether macOS Game Mode status is readable. Apple enables Game Mode automatically for eligible full-screen games (LSSupportsGameMode). There is no public API to query or force that system status.",
            "Status is informational only. Hertzline does not claim FPS gains from Game Mode without a local measurement.",
            "None: this action is read-only and never changes system settings.",
            "Nothing to undo; no toggle is offered when the OS does not expose one.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    pub fn report(&self) -> Result<MacOsGameModeReport, ActionError> {
        self.backend.status()
    }

    /// AC-4.19 / AC-4.23: never a fake toggle.
    pub fn exposes_toggle(&self) -> bool {
        false
    }
}

impl<B: MacOsGameModeBackend> Action for MacOsGameModeStatusAction<B> {
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
        Ok(self.backend.status()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        // Report-only: re-read status. Never toggles (AC-4.19).
        let _ = self.backend.status()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        if actual == expected.trim() {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

/// Host backend: public APIs never expose Game Mode status (any OS build).
pub mod host {
    use super::*;

    pub struct HostMacOsGameModeBackend;

    impl MacOsGameModeBackend for HostMacOsGameModeBackend {
        fn status(&self) -> Result<MacOsGameModeReport, ActionError> {
            Ok(MacOsGameModeReport::not_possible())
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
    fn mock_reports_not_possible_and_no_toggle() {
        let action = MacOsGameModeStatusAction::new(MockMacOsGameModeBackend {
            report: MacOsGameModeReport::not_possible(),
        });
        let report = action.report().expect("mock");
        assert_eq!(report.status, STATUS_NOT_POSSIBLE);
        assert!(report.detail.contains("no public API"));
        assert!(!action.exposes_toggle());
        assert_eq!(
            action.explanation().local_measurement.message(),
            "no local measurement exists yet"
        );
    }

    #[test]
    fn apply_restore_verify_round_trip_report_only() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockMacOsGameModeBackend {
            report: MacOsGameModeReport::not_possible(),
        });
        let mut action = MacOsGameModeStatusAction::new(backend);
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        assert!(entry.before.contains("status=not_possible"));
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        let verify = action.verify(&entry.before).expect("verify");
        assert!(verify.is_match());
    }

    #[test]
    fn host_backend_always_not_possible() {
        let report = host::HostMacOsGameModeBackend.status().expect("host");
        assert_eq!(report.status, STATUS_NOT_POSSIBLE);
    }
}
