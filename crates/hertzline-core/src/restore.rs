//! Restore-all and crash / uninstall recovery (HZ-018 / AC-7.2, AC-7.3).
//!
//! Headless: no UI required. Outstanding change-log rows are undone newest
//! first. Pending snapshots (crash mid-apply) are restored when an action is
//! registered, then aborted. Missing action handlers are reported, never
//! silently marked restored.

use std::collections::HashMap;
use std::path::Path;

use crate::action::{restore_with_log, Action, ActionError, ActionMap};
use crate::snapshot::{RestoreStatus, SnapshotError, SnapshotStore};

/// Why restore-all was invoked. Must work without the desktop UI (AC-7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreTrigger {
    Explicit,
    Crash,
    Uninstall,
    Watchdog,
}

impl RestoreTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Crash => "crash",
            Self::Uninstall => "uninstall",
            Self::Watchdog => "watchdog",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token.trim() {
            "explicit" => Some(Self::Explicit),
            "crash" => Some(Self::Crash),
            "uninstall" => Some(Self::Uninstall),
            "watchdog" => Some(Self::Watchdog),
            _ => None,
        }
    }
}

/// Per-entry outcome for restore-all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreItemStatus {
    Restored,
    Failed,
    MissingAction,
    PendingRestored,
    PendingAbortedOnly,
}

impl RestoreItemStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Restored => "restored",
            Self::Failed => "failed",
            Self::MissingAction => "missing_action",
            Self::PendingRestored => "pending_restored",
            Self::PendingAbortedOnly => "pending_aborted_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreItemResult {
    pub entry_id: Option<u64>,
    pub action: String,
    pub status: RestoreItemStatus,
    pub detail: String,
}

/// Headless restore-all report (AC-7.2, AC-7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreAllReport {
    pub trigger: RestoreTrigger,
    pub requires_ui: bool,
    pub items: Vec<RestoreItemResult>,
    pub ok: bool,
}

impl RestoreAllReport {
    pub fn status_label(&self) -> &'static str {
        if self.ok {
            "ok"
        } else {
            "partial"
        }
    }
}

/// Hash map registry used by tests and headless hosts.
#[derive(Default)]
pub struct MapActionRegistry {
    actions: HashMap<String, Box<dyn Action>>,
}

impl MapActionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, action: Box<dyn Action>) {
        self.actions.insert(action.id().to_string(), action);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Action> {
        self.actions.get(id).map(|a| a.as_ref())
    }
}

impl ActionMap for MapActionRegistry {
    fn get_mut(&mut self, id: &str) -> Option<&mut dyn Action> {
        self.actions
            .get_mut(id)
            .map(|a| a.as_mut() as &mut dyn Action)
    }
}

/// Revert every outstanding Hertzline change (AC-7.2). Does not need a UI.
pub fn restore_all(
    store: &mut SnapshotStore,
    actions: &mut dyn ActionMap,
    trigger: RestoreTrigger,
) -> RestoreAllReport {
    let mut items = Vec::new();

    // Pending snapshots: crash between snapshot_before and record_applied.
    let pending_actions: Vec<(String, String)> = store
        .snapshots()
        .iter()
        .filter(|snap| {
            !store
                .change_log()
                .iter()
                .any(|entry| entry.snapshot_id == snap.id)
        })
        .map(|snap| (snap.action_id.clone(), snap.before.clone()))
        .collect();

    for (action_id, before) in pending_actions {
        match actions.get_mut(&action_id) {
            Some(action) => match action.restore(&before) {
                Ok(()) => match action.verify(&before) {
                    Ok(verify) if verify.is_match() => match store.abort_pending(&action_id) {
                        Ok(_) => items.push(RestoreItemResult {
                            entry_id: None,
                            action: action_id,
                            status: RestoreItemStatus::PendingRestored,
                            detail: "pending apply aborted after restore to before-state".into(),
                        }),
                        Err(err) => items.push(RestoreItemResult {
                            entry_id: None,
                            action: action_id,
                            status: RestoreItemStatus::Failed,
                            detail: format!("restored live state but abort_pending failed: {err}"),
                        }),
                    },
                    Ok(verify) => items.push(RestoreItemResult {
                        entry_id: None,
                        action: action_id,
                        status: RestoreItemStatus::Failed,
                        detail: format!("pending restore verify failed: {verify:?}"),
                    }),
                    Err(err) => items.push(RestoreItemResult {
                        entry_id: None,
                        action: action_id,
                        status: RestoreItemStatus::Failed,
                        detail: format!("pending restore verify error: {err}"),
                    }),
                },
                Err(err) => items.push(RestoreItemResult {
                    entry_id: None,
                    action: action_id,
                    status: RestoreItemStatus::Failed,
                    detail: format!("pending restore failed: {err}"),
                }),
            },
            None => match store.abort_pending(&action_id) {
                Ok(_) => items.push(RestoreItemResult {
                    entry_id: None,
                    action: action_id,
                    status: RestoreItemStatus::PendingAbortedOnly,
                    detail: "no action handler; pending snapshot dropped (live state may be dirty)"
                        .into(),
                }),
                Err(err) => items.push(RestoreItemResult {
                    entry_id: None,
                    action: action_id,
                    status: RestoreItemStatus::Failed,
                    detail: format!("abort_pending failed: {err}"),
                }),
            },
        }
    }

    // Outstanding recorded changes: newest first (LIFO undo).
    let outstanding: Vec<u64> = store
        .change_log()
        .iter()
        .rev()
        .filter(|entry| entry.restore_status == RestoreStatus::Outstanding)
        .map(|entry| entry.id)
        .collect();

    for entry_id in outstanding {
        let action_id = match store.change_log().iter().find(|row| row.id == entry_id) {
            Some(row) => row.action.clone(),
            None => {
                items.push(RestoreItemResult {
                    entry_id: Some(entry_id),
                    action: String::new(),
                    status: RestoreItemStatus::Failed,
                    detail: "change log entry disappeared".into(),
                });
                continue;
            }
        };

        match actions.get_mut(&action_id) {
            None => items.push(RestoreItemResult {
                entry_id: Some(entry_id),
                action: action_id,
                status: RestoreItemStatus::MissingAction,
                detail: "no action handler registered for restore".into(),
            }),
            Some(action) => match restore_with_log(action, store, entry_id) {
                Ok(_) => items.push(RestoreItemResult {
                    entry_id: Some(entry_id),
                    action: action_id,
                    status: RestoreItemStatus::Restored,
                    detail: format!("restored via trigger {}", trigger.as_str()),
                }),
                Err(err) => items.push(RestoreItemResult {
                    entry_id: Some(entry_id),
                    action: action_id,
                    status: RestoreItemStatus::Failed,
                    detail: err.to_string(),
                }),
            },
        }
    }

    let ok = items.iter().all(|item| {
        matches!(
            item.status,
            RestoreItemStatus::Restored | RestoreItemStatus::PendingRestored
        )
    });

    RestoreAllReport {
        trigger,
        requires_ui: false,
        items,
        ok,
    }
}

/// Load (or create) the durable store and run restore-all for crash recovery.
pub fn recover_from_crash(
    store_path: &Path,
    actions: &mut dyn ActionMap,
) -> Result<RestoreAllReport, RestoreError> {
    let mut store = if store_path.exists() {
        SnapshotStore::load_from_path(store_path)?
    } else {
        return Ok(RestoreAllReport {
            trigger: RestoreTrigger::Crash,
            requires_ui: false,
            items: Vec::new(),
            ok: true,
        });
    };
    Ok(restore_all(&mut store, actions, RestoreTrigger::Crash))
}

/// Open the default (or env-overridden) store and restore for a trigger.
pub fn restore_all_at_default_path(
    actions: &mut dyn ActionMap,
    trigger: RestoreTrigger,
) -> Result<RestoreAllReport, RestoreError> {
    let path = crate::paths::snapshot_store_path();
    if !path.exists() {
        return Ok(RestoreAllReport {
            trigger,
            requires_ui: false,
            items: Vec::new(),
            ok: true,
        });
    }
    let mut store = SnapshotStore::load_from_path(&path)?;
    Ok(restore_all(&mut store, actions, trigger))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    Snapshot(SnapshotError),
    Action(ActionError),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot(err) => write!(f, "snapshot: {err}"),
            Self::Action(err) => write!(f, "action: {err}"),
        }
    }
}

impl std::error::Error for RestoreError {}

impl From<SnapshotError> for RestoreError {
    fn from(value: SnapshotError) -> Self {
        Self::Snapshot(value)
    }
}

impl From<ActionError> for RestoreError {
    fn from(value: ActionError) -> Self {
        Self::Action(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, MockToggleAction};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_path(label: &str) -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hertzline-restore-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("snapshots.json")
    }

    struct TempFile(std::path::PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn restore_all_reverts_outstanding_newest_first() {
        // mock: two in-memory toggles, not OS settings
        let mut store = SnapshotStore::new();
        let mut a = MockToggleAction::new("mock_a", "off", "on");
        let mut b = MockToggleAction::new("mock_b", "low", "high");
        let ea = apply_with_snapshot(&mut a, &mut store, 10).unwrap();
        let eb = apply_with_snapshot(&mut b, &mut store, 20).unwrap();
        assert_eq!(a.state(), "on");
        assert_eq!(b.state(), "high");

        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(a));
        registry.insert(Box::new(b));
        let report = restore_all(&mut store, &mut registry, RestoreTrigger::Explicit);
        assert!(report.ok);
        assert!(!report.requires_ui);
        assert_eq!(report.items.len(), 2);
        assert_eq!(report.items[0].entry_id, Some(eb.id));
        assert_eq!(report.items[1].entry_id, Some(ea.id));
        assert_eq!(registry.get("mock_a").unwrap().read_state().unwrap(), "off");
        assert_eq!(registry.get("mock_b").unwrap().read_state().unwrap(), "low");
        assert!(store
            .change_log()
            .iter()
            .all(|e| e.restore_status == RestoreStatus::Restored));
    }

    #[test]
    fn crash_rollback_reloads_store_and_restores() {
        // mock crash: durable log survives; in-memory action state is rebuilt dirty
        let path = scratch_path("crash");
        let _keep = TempFile(path.clone());
        {
            let mut store = SnapshotStore::open(&path).unwrap();
            let mut action = MockToggleAction::new("mock_toggle", "balanced", "high");
            apply_with_snapshot(&mut action, &mut store, 100).unwrap();
            assert_eq!(action.state(), "high");
            // process "crashes": store dropped; action state lost
        }

        let dirty = MockToggleAction::new("mock_toggle", "high", "high");
        assert_eq!(dirty.state(), "high");
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(dirty));
        let report = recover_from_crash(&path, &mut registry).unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(report.trigger, RestoreTrigger::Crash);
        assert!(!report.requires_ui);
        assert_eq!(
            registry.get("mock_toggle").unwrap().read_state().unwrap(),
            "balanced"
        );
        let loaded = SnapshotStore::load_from_path(&path).unwrap();
        assert_eq!(
            loaded.change_log()[0].restore_status,
            RestoreStatus::Restored
        );
    }

    #[test]
    fn crash_mid_apply_restores_pending_before_state() {
        let path = scratch_path("pending");
        let _keep = TempFile(path.clone());
        {
            let mut store = SnapshotStore::open(&path).unwrap();
            let mut action = MockToggleAction::new("mock_toggle", "off", "on");
            let before = action.read_state().unwrap();
            let _token = store.snapshot_before(action.id(), before, 1).unwrap();
            // mutate after snapshot, crash before record_applied
            action.apply().unwrap();
            assert_eq!(action.state(), "on");
            assert!(store.change_log().is_empty());
            assert!(store.pending_token("mock_toggle").is_some());
        }

        let dirty = MockToggleAction::new("mock_toggle", "on", "on");
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(dirty));
        let report = recover_from_crash(&path, &mut registry).unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(report.items[0].status, RestoreItemStatus::PendingRestored);
        assert_eq!(
            registry.get("mock_toggle").unwrap().read_state().unwrap(),
            "off"
        );
        let loaded = SnapshotStore::load_from_path(&path).unwrap();
        assert!(loaded.pending_token("mock_toggle").is_none());
        assert!(loaded.change_log().is_empty());
    }

    #[test]
    fn missing_action_is_not_silently_marked_restored() {
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "off", "on");
        apply_with_snapshot(&mut action, &mut store, 1).unwrap();
        let mut empty = MapActionRegistry::new();
        let report = restore_all(&mut store, &mut empty, RestoreTrigger::Uninstall);
        assert!(!report.ok);
        assert_eq!(report.items[0].status, RestoreItemStatus::MissingAction);
        assert_eq!(
            store.change_log()[0].restore_status,
            RestoreStatus::Outstanding
        );
    }

    #[test]
    fn empty_store_restore_is_ok() {
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        let report = restore_all(&mut store, &mut registry, RestoreTrigger::Watchdog);
        assert!(report.ok);
        assert!(report.items.is_empty());
        assert!(!report.requires_ui);
    }

    #[test]
    fn trigger_parse_covers_headless_set() {
        for token in ["crash", "uninstall", "watchdog", "explicit"] {
            assert_eq!(RestoreTrigger::parse(token).unwrap().as_str(), token);
        }
        assert!(RestoreTrigger::parse("ui-only").is_none());
    }

    #[test]
    fn recover_missing_file_is_ok() {
        let path = scratch_path("missing");
        let _keep = TempFile(path.clone());
        let _ = std::fs::remove_file(&path);
        let mut registry = MapActionRegistry::new();
        let report = recover_from_crash(&path, &mut registry).unwrap();
        assert!(report.ok);
        assert!(report.items.is_empty());
    }
}
