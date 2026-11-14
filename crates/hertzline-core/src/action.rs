//! Optimization action trait (HZ-017 / AC-7.1, AC-8.1).
//!
//! Every mutating action snapshots before apply, then supports restore and
//! verify. The explanation schema carries what / expected effect / risk /
//! how to undo. Real OS actions ship later (HZ-020+); this module defines
//! the contract and a labeled in-memory mock for tests.

use crate::snapshot::{ChangeLogEntry, SnapshotError, SnapshotStore};

/// Plain-language explanation required for every action (AC-8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionExplanation {
    /// What the action does.
    pub what: String,
    /// Expected effect in plain language (never a fabricated "+X FPS").
    pub expected_effect: String,
    /// Risk if the change is applied.
    pub risk: String,
    /// How to undo the change.
    pub how_to_undo: String,
    /// Local before/after evidence note (AC-8.2 wording; full evidence later).
    pub local_measurement: LocalMeasurementNote,
}

impl ActionExplanation {
    /// Build an explanation. Empty trimmed fields fail [`validate_explanation`].
    pub fn new(
        what: impl Into<String>,
        expected_effect: impl Into<String>,
        risk: impl Into<String>,
        how_to_undo: impl Into<String>,
    ) -> Self {
        Self {
            what: what.into(),
            expected_effect: expected_effect.into(),
            risk: risk.into(),
            how_to_undo: how_to_undo.into(),
            local_measurement: LocalMeasurementNote::NoneYet,
        }
    }
}

/// Whether a local before/after benchmark exists for this action on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalMeasurementNote {
    /// Default until a local benchmark exists (AC-8.2).
    NoneYet,
}

impl LocalMeasurementNote {
    pub const NONE_YET_MESSAGE: &'static str = "no local measurement exists yet";

    pub fn message(&self) -> &'static str {
        match self {
            Self::NoneYet => Self::NONE_YET_MESSAGE,
        }
    }
}

/// Result of comparing live state to an expected value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyResult {
    Matches,
    Mismatch { expected: String, actual: String },
}

impl VerifyResult {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Matches)
    }
}

/// Errors from action apply / restore / verify or the snapshot guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    EmptyId,
    IncompleteExplanation(&'static str),
    Snapshot(SnapshotError),
    ApplyFailed(String),
    RestoreFailed(String),
    VerifyFailed(String),
    NotPossible(String),
    UnknownEntry,
    AlreadyRestored,
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyId => write!(f, "action id is empty"),
            Self::IncompleteExplanation(field) => {
                write!(f, "explanation field `{field}` is empty")
            }
            Self::Snapshot(err) => write!(f, "snapshot: {err}"),
            Self::ApplyFailed(reason) => write!(f, "apply failed: {reason}"),
            Self::RestoreFailed(reason) => write!(f, "restore failed: {reason}"),
            Self::VerifyFailed(reason) => write!(f, "verify failed: {reason}"),
            Self::NotPossible(reason) => write!(f, "not possible: {reason}"),
            Self::UnknownEntry => write!(f, "change log entry not found"),
            Self::AlreadyRestored => write!(f, "change log entry is already restored"),
        }
    }
}

impl std::error::Error for ActionError {}

impl From<SnapshotError> for ActionError {
    fn from(value: SnapshotError) -> Self {
        match value {
            SnapshotError::UnknownEntry => Self::UnknownEntry,
            SnapshotError::AlreadyRestored => Self::AlreadyRestored,
            other => Self::Snapshot(other),
        }
    }
}

/// Reject incomplete AC-8.1 explanation fields.
pub fn validate_explanation(explanation: &ActionExplanation) -> Result<(), ActionError> {
    if explanation.what.trim().is_empty() {
        return Err(ActionError::IncompleteExplanation("what"));
    }
    if explanation.expected_effect.trim().is_empty() {
        return Err(ActionError::IncompleteExplanation("expected_effect"));
    }
    if explanation.risk.trim().is_empty() {
        return Err(ActionError::IncompleteExplanation("risk"));
    }
    if explanation.how_to_undo.trim().is_empty() {
        return Err(ActionError::IncompleteExplanation("how_to_undo"));
    }
    Ok(())
}

/// Contract for every optimization action: apply, restore, verify (AC-7.1, AC-8.1).
pub trait Action {
    fn id(&self) -> &str;
    fn explanation(&self) -> &ActionExplanation;
    /// True when apply/restore must go through the privileged helper.
    fn requires_elevation(&self) -> bool;

    /// Read the live setting value used for snapshots and verify.
    fn read_state(&self) -> Result<String, ActionError>;
    /// Mutate to the action's target state. Caller must snapshot first.
    fn apply(&mut self) -> Result<(), ActionError>;
    /// Revert to the snapshotted before-state.
    fn restore(&mut self, before: &str) -> Result<(), ActionError>;
    /// Confirm live state equals `expected`.
    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError>;
}

/// Snapshot, apply, record, then verify the after-state (AC-7.1).
///
/// `apply` is not called if the snapshot fails. On apply failure the pending
/// snapshot stays so the caller can abort or retry.
pub fn apply_with_snapshot(
    action: &mut dyn Action,
    store: &mut SnapshotStore,
    now_unix_ms: i64,
) -> Result<ChangeLogEntry, ActionError> {
    let id = action.id().trim();
    if id.is_empty() {
        return Err(ActionError::EmptyId);
    }
    validate_explanation(action.explanation())?;
    let before = action.read_state()?;
    let token = store.snapshot_before(id, before, now_unix_ms)?;
    action.apply()?;
    let after = action.read_state()?;
    match action.verify(&after)? {
        VerifyResult::Matches => Ok(store.record_applied(&token, after, now_unix_ms)?),
        VerifyResult::Mismatch { expected, actual } => Err(ActionError::VerifyFailed(format!(
            "after apply expected {expected:?} got {actual:?}"
        ))),
    }
}

/// Restore from a change-log entry, verify the before-state, mark restored.
pub fn restore_with_log(
    action: &mut dyn Action,
    store: &mut SnapshotStore,
    entry_id: u64,
) -> Result<ChangeLogEntry, ActionError> {
    let id = action.id().trim();
    if id.is_empty() {
        return Err(ActionError::EmptyId);
    }
    validate_explanation(action.explanation())?;
    let entry = store
        .change_log()
        .iter()
        .find(|row| row.id == entry_id)
        .cloned()
        .ok_or(ActionError::UnknownEntry)?;
    if entry.action != id {
        return Err(ActionError::RestoreFailed(format!(
            "entry action {} does not match {}",
            entry.action, id
        )));
    }
    if entry.restore_status == crate::snapshot::RestoreStatus::Restored {
        return Err(ActionError::AlreadyRestored);
    }
    action.restore(&entry.before)?;
    match action.verify(&entry.before)? {
        VerifyResult::Matches => Ok(store.mark_restored(entry_id)?),
        VerifyResult::Mismatch { expected, actual } => Err(ActionError::VerifyFailed(format!(
            "after restore expected {expected:?} got {actual:?}"
        ))),
    }
}

/// In-memory mock action for unit tests. Not a real OS tweak.
#[derive(Debug, Clone)]
pub struct MockToggleAction {
    id: String,
    explanation: ActionExplanation,
    state: String,
    target: String,
    requires_elevation: bool,
    fail_apply: Option<String>,
    fail_restore: Option<String>,
}

impl MockToggleAction {
    /// Labeled mock: toggles an in-memory string setting between states.
    pub fn new(
        id: impl Into<String>,
        initial: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            explanation: ActionExplanation::new(
                "Mock toggle for unit tests (not a real OS setting).",
                "Changes only an in-memory string; no system effect.",
                "None: this is a labeled mock.",
                "Call restore with the snapshotted before-state.",
            ),
            state: initial.into(),
            target: target.into(),
            requires_elevation: false,
            fail_apply: None,
            fail_restore: None,
        }
    }

    pub fn with_elevation(mut self, required: bool) -> Self {
        self.requires_elevation = required;
        self
    }

    pub fn fail_apply(mut self, reason: impl Into<String>) -> Self {
        self.fail_apply = Some(reason.into());
        self
    }

    pub fn fail_restore(mut self, reason: impl Into<String>) -> Self {
        self.fail_restore = Some(reason.into());
        self
    }

    pub fn with_explanation(mut self, explanation: ActionExplanation) -> Self {
        self.explanation = explanation;
        self
    }

    pub fn state(&self) -> &str {
        &self.state
    }
}

impl Action for MockToggleAction {
    fn id(&self) -> &str {
        &self.id
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        self.requires_elevation
    }

    fn read_state(&self) -> Result<String, ActionError> {
        Ok(self.state.clone())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        if let Some(reason) = &self.fail_apply {
            return Err(ActionError::ApplyFailed(reason.clone()));
        }
        self.state = self.target.clone();
        Ok(())
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        if let Some(reason) = &self.fail_restore {
            return Err(ActionError::RestoreFailed(reason.clone()));
        }
        self.state = before.to_string();
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        if self.state == expected {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.to_string(),
                actual: self.state.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{RestoreStatus, SnapshotStore};

    #[test]
    fn explanation_schema_requires_all_ac81_fields() {
        let good = ActionExplanation::new("what", "effect", "risk", "undo");
        assert!(validate_explanation(&good).is_ok());
        assert_eq!(
            good.local_measurement.message(),
            LocalMeasurementNote::NONE_YET_MESSAGE
        );

        let missing_what = ActionExplanation::new("  ", "effect", "risk", "undo");
        assert_eq!(
            validate_explanation(&missing_what),
            Err(ActionError::IncompleteExplanation("what"))
        );
        let missing_effect = ActionExplanation::new("what", "", "risk", "undo");
        assert_eq!(
            validate_explanation(&missing_effect),
            Err(ActionError::IncompleteExplanation("expected_effect"))
        );
        let missing_risk = ActionExplanation::new("what", "effect", "\t", "undo");
        assert_eq!(
            validate_explanation(&missing_risk),
            Err(ActionError::IncompleteExplanation("risk"))
        );
        let missing_undo = ActionExplanation::new("what", "effect", "risk", " ");
        assert_eq!(
            validate_explanation(&missing_undo),
            Err(ActionError::IncompleteExplanation("how_to_undo"))
        );
    }

    #[test]
    fn mock_apply_restore_verify_round_trip() {
        // mock: in-memory toggle only
        let mut action = MockToggleAction::new("mock_toggle", "off", "on");
        assert_eq!(action.read_state().unwrap(), "off");
        assert_eq!(action.verify("off").unwrap(), VerifyResult::Matches);
        action.apply().unwrap();
        assert_eq!(action.state(), "on");
        assert!(action.verify("on").unwrap().is_match());
        assert_eq!(
            action.verify("off").unwrap(),
            VerifyResult::Mismatch {
                expected: "off".into(),
                actual: "on".into(),
            }
        );
        action.restore("off").unwrap();
        assert!(action.verify("off").unwrap().is_match());
    }

    #[test]
    fn apply_with_snapshot_writes_before_mutate() {
        // mock store + mock action
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "balanced", "high_performance");
        let before = action.read_state().unwrap();
        let token = store
            .snapshot_before(action.id(), before, 1_700_000_000_000)
            .expect("snapshot before apply");
        assert_eq!(store.snapshots().len(), 1);
        assert_eq!(store.snapshots()[0].before, "balanced");
        // mutate only after snapshot exists
        action.apply().unwrap();
        let after = action.read_state().unwrap();
        let entry = store
            .record_applied(&token, after, 1_700_000_000_001)
            .unwrap();
        assert_eq!(entry.before, "balanced");
        assert_eq!(entry.after, "high_performance");
        assert_eq!(entry.restore_status, RestoreStatus::Outstanding);
    }

    #[test]
    fn apply_with_snapshot_guard_records_change_log() {
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "off", "on");
        let entry = apply_with_snapshot(&mut action, &mut store, 42).unwrap();
        assert_eq!(entry.action, "mock_toggle");
        assert_eq!(entry.before, "off");
        assert_eq!(entry.after, "on");
        assert_eq!(entry.restore_status, RestoreStatus::Outstanding);
        assert!(action.verify("on").unwrap().is_match());
        assert_eq!(store.change_log().len(), 1);
    }

    #[test]
    fn failed_apply_keeps_pending_snapshot_and_skips_log() {
        let mut store = SnapshotStore::new();
        let mut action =
            MockToggleAction::new("mock_toggle", "off", "on").fail_apply("mock apply boom");
        let err = apply_with_snapshot(&mut action, &mut store, 7).unwrap_err();
        assert_eq!(err, ActionError::ApplyFailed("mock apply boom".into()));
        assert_eq!(action.state(), "off");
        assert_eq!(store.snapshots().len(), 1);
        assert!(store.change_log().is_empty());
        assert!(store.pending_token("mock_toggle").is_some());
    }

    #[test]
    fn restore_with_log_marks_restored_after_verify() {
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "off", "on");
        let entry = apply_with_snapshot(&mut action, &mut store, 10).unwrap();
        assert_eq!(action.state(), "on");
        let restored = restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert_eq!(restored.restore_status, RestoreStatus::Restored);
        assert_eq!(action.state(), "off");
        assert!(action.verify("off").unwrap().is_match());
        assert_eq!(
            restore_with_log(&mut action, &mut store, entry.id).unwrap_err(),
            ActionError::AlreadyRestored
        );
    }

    #[test]
    fn incomplete_explanation_blocks_apply() {
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "off", "on").with_explanation(
            ActionExplanation::new("does a thing", "maybe faster", "", "restore"),
        );
        assert_eq!(
            apply_with_snapshot(&mut action, &mut store, 1).unwrap_err(),
            ActionError::IncompleteExplanation("risk")
        );
        assert!(store.snapshots().is_empty());
        assert_eq!(action.state(), "off");
    }

    #[test]
    fn empty_action_id_is_rejected() {
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("  ", "off", "on");
        assert_eq!(
            apply_with_snapshot(&mut action, &mut store, 1).unwrap_err(),
            ActionError::EmptyId
        );
    }

    #[test]
    fn mock_reports_elevation_flag() {
        let action = MockToggleAction::new("elev", "a", "b").with_elevation(true);
        assert!(action.requires_elevation());
        assert!(!MockToggleAction::new("user", "a", "b").requires_elevation());
    }
}
