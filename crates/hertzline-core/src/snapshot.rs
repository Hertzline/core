//! Snapshot store and change log (HZ-016 / AC-7.1, AC-7.5).
//!
//! `snapshot_before` stores the before-state before any mutate callback runs.
//! When the store is opened on a path, that JSON is written to disk first.
//! `record_applied` appends a change-log row only if the token matches that
//! snapshot. The row lists time, action, before, after, and restore status.
//! Action apply/restore/verify is HZ-017; restore-all / crash hooks are HZ-018.
//! This module only stores state and status.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// On-disk schema for the snapshot store. Reject other versions until a migrator exists.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Wall-clock milliseconds since Unix epoch for callers that do not inject a clock.
pub fn unix_ms_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Before-state captured prior to a setting change (AC-7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingSnapshot {
    pub id: u64,
    pub action_id: String,
    pub captured_at_unix_ms: i64,
    pub before: String,
}

/// Proof that `snapshot_before` already stored `before` for `action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotToken {
    pub id: u64,
    pub action: String,
    pub before: String,
    pub captured_at_unix_ms: i64,
}

impl SnapshotToken {
    fn from_snapshot(snap: &SettingSnapshot) -> Self {
        Self {
            id: snap.id,
            action: snap.action_id.clone(),
            before: snap.before.clone(),
            captured_at_unix_ms: snap.captured_at_unix_ms,
        }
    }
}

/// Whether a recorded change has been restored (AC-7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreStatus {
    /// Applied and not yet restored.
    Outstanding,
    /// Marked restored after a successful action restore (HZ-017+).
    Restored,
}

impl RestoreStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Outstanding => "outstanding",
            Self::Restored => "restored",
        }
    }
}

/// One visible change-log row (AC-7.5): time, action, before, after, restore status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeLogEntry {
    pub id: u64,
    pub time_unix_ms: i64,
    pub action: String,
    pub before: String,
    pub after: String,
    pub restore_status: RestoreStatus,
    pub snapshot_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    EmptyAction,
    MissingSnapshot,
    PendingSnapshot,
    TokenMismatch,
    UnknownEntry,
    AlreadyRestored,
    ApplyFailed(String),
    StoreIo(String),
    InvalidDocument(String),
    UnsupportedSchema(u32),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAction => write!(f, "action id is empty"),
            Self::MissingSnapshot => write!(f, "no snapshot stored before this change"),
            Self::PendingSnapshot => write!(f, "a snapshot for this action is already waiting"),
            Self::TokenMismatch => {
                write!(f, "snapshot token does not match the stored before-state")
            }
            Self::UnknownEntry => write!(f, "change log entry not found"),
            Self::AlreadyRestored => write!(f, "change log entry is already restored"),
            Self::ApplyFailed(reason) => write!(f, "apply failed after snapshot: {reason}"),
            Self::StoreIo(reason) => write!(f, "snapshot store io error: {reason}"),
            Self::InvalidDocument(reason) => write!(f, "invalid snapshot document: {reason}"),
            Self::UnsupportedSchema(version) => {
                write!(f, "unsupported snapshot schema version {version}")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotDocument {
    schema_version: u32,
    next_id: u64,
    snapshots: Vec<SettingSnapshot>,
    change_log: Vec<ChangeLogEntry>,
}

/// In-memory snapshot store. `open` also rewrites a JSON file before mutate.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    next_id: u64,
    snapshots: Vec<SettingSnapshot>,
    log: Vec<ChangeLogEntry>,
    persist_path: Option<PathBuf>,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            snapshots: Vec::new(),
            log: Vec::new(),
            persist_path: None,
        }
    }

    /// Create or load `path`. Parent directories are created. The file is user data.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SnapshotError> {
        let path = path.into();
        if path.exists() {
            return Self::load_from_path(&path);
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| SnapshotError::StoreIo(e.to_string()))?;
            }
        }
        let mut store = Self::new();
        store.persist_path = Some(path);
        store.persist()?;
        Ok(store)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, SnapshotError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| SnapshotError::StoreIo(e.to_string()))?;
        let mut store = Self::from_json(&text)?;
        store.persist_path = Some(path.to_path_buf());
        Ok(store)
    }

    pub fn path(&self) -> Option<&Path> {
        self.persist_path.as_deref()
    }

    pub fn snapshots(&self) -> &[SettingSnapshot] {
        &self.snapshots
    }

    pub fn change_log(&self) -> &[ChangeLogEntry] {
        &self.log
    }

    pub fn pending_token(&self, action: &str) -> Option<SnapshotToken> {
        let action = action.trim();
        let idx = self.pending_index(action)?;
        self.snapshots.get(idx).map(SnapshotToken::from_snapshot)
    }

    /// Store `before` before the caller mutates anything (AC-7.1).
    pub fn snapshot_before(
        &mut self,
        action: &str,
        before: impl Into<String>,
        now_unix_ms: i64,
    ) -> Result<SnapshotToken, SnapshotError> {
        let action = require_action(action)?.to_string();
        if self.pending_index(&action).is_some() {
            return Err(SnapshotError::PendingSnapshot);
        }
        let id = self.alloc_id();
        let before = before.into();
        self.snapshots.push(SettingSnapshot {
            id,
            action_id: action.clone(),
            captured_at_unix_ms: now_unix_ms,
            before: before.clone(),
        });
        if let Err(err) = self.persist() {
            self.snapshots.pop();
            self.next_id = id;
            return Err(err);
        }
        Ok(SnapshotToken {
            id,
            action,
            before,
            captured_at_unix_ms: now_unix_ms,
        })
    }

    /// Append a change-log row. Fails when `token` is not an uncommitted snapshot.
    pub fn record_applied(
        &mut self,
        token: &SnapshotToken,
        after: impl Into<String>,
        now_unix_ms: i64,
    ) -> Result<ChangeLogEntry, SnapshotError> {
        let action = require_action(&token.action)?.to_string();
        let idx = self
            .pending_index(&action)
            .ok_or(SnapshotError::MissingSnapshot)?;
        let snap = &self.snapshots[idx];
        if snap.id != token.id
            || snap.before != token.before
            || snap.captured_at_unix_ms != token.captured_at_unix_ms
            || snap.action_id != action
        {
            return Err(SnapshotError::TokenMismatch);
        }
        let snapshot_id = snap.id;
        let before = snap.before.clone();
        let entry = ChangeLogEntry {
            id: self.alloc_id(),
            time_unix_ms: now_unix_ms,
            action,
            before,
            after: after.into(),
            restore_status: RestoreStatus::Outstanding,
            snapshot_id,
        };
        self.log.push(entry.clone());
        if let Err(err) = self.persist() {
            self.log.pop();
            self.next_id = entry.id;
            return Err(err);
        }
        Ok(entry)
    }

    /// Snapshot, then run `mutate`, then record. `mutate` is not called if the snapshot fails.
    ///
    /// `mutate` is a mockable setting change. It must not run until the snapshot is stored.
    pub fn apply_guarded<F>(
        &mut self,
        action: &str,
        before: impl Into<String>,
        now_unix_ms: i64,
        mutate: F,
    ) -> Result<ChangeLogEntry, SnapshotError>
    where
        F: FnOnce(&SnapshotToken) -> Result<String, String>,
    {
        let token = self.snapshot_before(action, before, now_unix_ms)?;
        let after = match mutate(&token) {
            Ok(after) => after,
            Err(reason) => return Err(SnapshotError::ApplyFailed(reason)),
        };
        self.record_applied(&token, after, now_unix_ms)
    }

    /// Drop an uncommitted snapshot so the action can be snapshotted again.
    pub fn abort_pending(&mut self, action: &str) -> Result<SettingSnapshot, SnapshotError> {
        let action = require_action(action)?;
        let idx = self
            .pending_index(action)
            .ok_or(SnapshotError::MissingSnapshot)?;
        let snap = self.snapshots.remove(idx);
        if let Err(err) = self.persist() {
            self.snapshots.insert(idx, snap);
            return Err(err);
        }
        Ok(snap)
    }

    /// Set restore status. Does not change time, action, before, or after.
    pub fn mark_restored(&mut self, entry_id: u64) -> Result<ChangeLogEntry, SnapshotError> {
        let idx = self
            .log
            .iter()
            .position(|entry| entry.id == entry_id)
            .ok_or(SnapshotError::UnknownEntry)?;
        if self.log[idx].restore_status == RestoreStatus::Restored {
            return Err(SnapshotError::AlreadyRestored);
        }
        let previous = self.log[idx].restore_status;
        self.log[idx].restore_status = RestoreStatus::Restored;
        if let Err(err) = self.persist() {
            self.log[idx].restore_status = previous;
            return Err(err);
        }
        Ok(self.log[idx].clone())
    }

    pub fn to_json(&self) -> Result<String, SnapshotError> {
        let doc = SnapshotDocument {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            next_id: self.next_id,
            snapshots: self.snapshots.clone(),
            change_log: self.log.clone(),
        };
        serde_json::to_string(&doc).map_err(|e| SnapshotError::InvalidDocument(e.to_string()))
    }

    pub fn from_json(text: &str) -> Result<Self, SnapshotError> {
        let doc: SnapshotDocument = serde_json::from_str(text)
            .map_err(|e| SnapshotError::InvalidDocument(e.to_string()))?;
        validate_document(&doc)?;
        Ok(Self {
            next_id: repaired_next_id(&doc),
            snapshots: doc.snapshots,
            log: doc.change_log,
            persist_path: None,
        })
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn pending_index(&self, action: &str) -> Option<usize> {
        self.snapshots
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, snap)| {
                let pending = snap.action_id == action
                    && !self.log.iter().any(|entry| entry.snapshot_id == snap.id);
                pending.then_some(index)
            })
    }

    // shortcut: rewrite the whole file, no fsync; switch to append + fsync if HZ-018
    // crash tests show torn reads or the log grows past a full rewrite.
    fn persist(&self) -> Result<(), SnapshotError> {
        let Some(path) = &self.persist_path else {
            return Ok(());
        };
        let bytes = self.to_json()?;
        let mut tmp = path.clone();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!("{e}.tmp"))
            .unwrap_or_else(|| "tmp".to_string());
        tmp.set_extension(ext);
        std::fs::write(&tmp, bytes.as_bytes())
            .map_err(|e| SnapshotError::StoreIo(e.to_string()))?;
        replace_file(&tmp, path)
    }
}

fn require_action(action: &str) -> Result<&str, SnapshotError> {
    let action = action.trim();
    if action.is_empty() {
        Err(SnapshotError::EmptyAction)
    } else {
        Ok(action)
    }
}

fn replace_file(tmp: &Path, dest: &Path) -> Result<(), SnapshotError> {
    match std::fs::rename(tmp, dest) {
        Ok(()) => Ok(()),
        Err(first) => {
            // Windows rename does not replace an existing destination.
            let _ = std::fs::remove_file(dest);
            std::fs::rename(tmp, dest)
                .map_err(|err| SnapshotError::StoreIo(format!("{err}; initial rename: {first}")))
        }
    }
}

fn repaired_next_id(doc: &SnapshotDocument) -> u64 {
    let mut next = doc.next_id.max(1);
    for snap in &doc.snapshots {
        if snap.id >= next {
            next = snap.id.saturating_add(1);
        }
    }
    for entry in &doc.change_log {
        if entry.id >= next {
            next = entry.id.saturating_add(1);
        }
    }
    next
}

fn validate_document(doc: &SnapshotDocument) -> Result<(), SnapshotError> {
    if doc.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotError::UnsupportedSchema(doc.schema_version));
    }
    let mut seen_snap = std::collections::BTreeSet::new();
    for snap in &doc.snapshots {
        if snap.action_id.trim().is_empty() || snap.action_id.trim() != snap.action_id {
            return Err(SnapshotError::InvalidDocument(
                "snapshot action id is empty".into(),
            ));
        }
        if !seen_snap.insert(snap.id) {
            return Err(SnapshotError::InvalidDocument(
                "duplicate snapshot id".into(),
            ));
        }
    }
    let mut seen_entry = std::collections::BTreeSet::new();
    let mut committed = std::collections::BTreeSet::new();
    for entry in &doc.change_log {
        if !seen_entry.insert(entry.id) {
            return Err(SnapshotError::InvalidDocument(
                "duplicate change log id".into(),
            ));
        }
        let snap = doc
            .snapshots
            .iter()
            .find(|snap| snap.id == entry.snapshot_id)
            .ok_or_else(|| {
                SnapshotError::InvalidDocument("change log entry missing snapshot".into())
            })?;
        if snap.before != entry.before || snap.action_id != entry.action {
            return Err(SnapshotError::InvalidDocument(
                "change log does not match snapshot".into(),
            ));
        }
        if !committed.insert(entry.snapshot_id) {
            return Err(SnapshotError::InvalidDocument(
                "snapshot referenced twice".into(),
            ));
        }
    }
    let mut pending_actions = std::collections::BTreeSet::new();
    for snap in &doc.snapshots {
        if committed.contains(&snap.id) {
            continue;
        }
        if !pending_actions.insert(snap.action_id.as_str()) {
            return Err(SnapshotError::InvalidDocument(
                "two pending snapshots for one action".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_path(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hertzline-snapshot-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("snapshots.json")
    }

    struct TempFile(PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn snapshot_is_written_before_mutate() {
        // mock: setting lives in a String, not an OS API
        let mut current = "balanced".to_string();
        let mut store = SnapshotStore::new();
        let token = store
            .snapshot_before("power_plan", current.clone(), 1_700_000_000_000)
            .expect("snapshot");
        assert_eq!(store.snapshots().len(), 1);
        assert!(store.change_log().is_empty());
        assert_eq!(store.snapshots()[0].before, "balanced");
        assert_eq!(store.snapshots()[0].action_id, "power_plan");
        current = "high_performance".to_string();
        let entry = store
            .record_applied(&token, current.clone(), 1_700_000_000_500)
            .expect("record");
        assert_eq!(entry.before, "balanced");
        assert_eq!(entry.after, "high_performance");
        assert_eq!(entry.time_unix_ms, 1_700_000_000_500);
        assert_eq!(current, "high_performance");
    }

    #[test]
    fn record_without_snapshot_is_rejected() {
        let mut store = SnapshotStore::new();
        let forged = SnapshotToken {
            id: 1,
            action: "power_plan".into(),
            before: "balanced".into(),
            captured_at_unix_ms: 1,
        };
        let err = store
            .record_applied(&forged, "high_performance", 2)
            .expect_err("missing");
        assert_eq!(err, SnapshotError::MissingSnapshot);
        assert!(store.change_log().is_empty());
        assert!(store.snapshots().is_empty());
    }

    #[test]
    fn token_mismatch_does_not_append() {
        let mut store = SnapshotStore::new();
        let token = store
            .snapshot_before("power_plan", "balanced", 1)
            .expect("snapshot");
        let mut bad = token.clone();
        bad.before = "other".into();
        let err = store
            .record_applied(&bad, "high_performance", 2)
            .expect_err("mismatch");
        assert_eq!(err, SnapshotError::TokenMismatch);
        assert!(store.change_log().is_empty());
        assert_eq!(store.snapshots().len(), 1);
        assert_eq!(store.snapshots()[0].before, "balanced");
    }

    #[test]
    fn second_record_with_same_token_is_rejected() {
        let mut store = SnapshotStore::new();
        let token = store.snapshot_before("power_plan", "balanced", 1).unwrap();
        store.record_applied(&token, "high_performance", 2).unwrap();
        let err = store
            .record_applied(&token, "high_performance", 3)
            .unwrap_err();
        assert_eq!(err, SnapshotError::MissingSnapshot);
        assert_eq!(store.change_log().len(), 1);
    }

    #[test]
    fn mutate_is_not_called_without_a_snapshot() {
        let mut store = SnapshotStore::new();
        let mut called = false;
        let err = store
            .apply_guarded("  ", "balanced", 1, |_| {
                called = true;
                Ok("high_performance".into())
            })
            .unwrap_err();
        assert_eq!(err, SnapshotError::EmptyAction);
        assert!(!called);
        assert!(store.snapshots().is_empty());
        assert!(store.change_log().is_empty());
    }

    #[test]
    fn failed_apply_keeps_snapshot_and_skips_log() {
        let mut store = SnapshotStore::new();
        let err = store
            .apply_guarded("power_plan", "balanced", 4, |_| {
                Err("mock: os refused".into())
            })
            .unwrap_err();
        assert!(matches!(err, SnapshotError::ApplyFailed(_)));
        assert_eq!(store.snapshots().len(), 1);
        assert_eq!(store.snapshots()[0].before, "balanced");
        assert!(store.change_log().is_empty());
        assert!(store.pending_token("power_plan").is_some());
    }

    #[test]
    fn pending_snapshot_blocks_a_second_snapshot() {
        let mut store = SnapshotStore::new();
        store.snapshot_before("power_plan", "balanced", 1).unwrap();
        let err = store
            .snapshot_before("power_plan", "balanced", 2)
            .unwrap_err();
        assert_eq!(err, SnapshotError::PendingSnapshot);
        assert_eq!(store.snapshots().len(), 1);
        store.snapshot_before("game_mode", "off", 3).unwrap();
        assert_eq!(store.snapshots().len(), 2);
    }

    #[test]
    fn abort_pending_allows_a_new_snapshot() {
        let mut store = SnapshotStore::new();
        store.snapshot_before("power_plan", "balanced", 1).unwrap();
        let removed = store.abort_pending("power_plan").unwrap();
        assert_eq!(removed.before, "balanced");
        assert!(store.snapshots().is_empty());
        let token = store.snapshot_before("power_plan", "balanced", 2).unwrap();
        let entry = store.record_applied(&token, "high_performance", 3).unwrap();
        assert_eq!(entry.restore_status, RestoreStatus::Outstanding);
    }

    #[test]
    fn change_log_fields_round_trip_json() {
        let mut store = SnapshotStore::new();
        let entry = store
            .apply_guarded("power_plan", "balanced", 1_700_000_000_000, |_| {
                Ok("high_performance".into())
            })
            .unwrap();
        assert_eq!(entry.action, "power_plan");
        assert_eq!(entry.before, "balanced");
        assert_eq!(entry.after, "high_performance");
        assert_eq!(entry.time_unix_ms, 1_700_000_000_000);
        assert_eq!(entry.restore_status, RestoreStatus::Outstanding);
        assert_eq!(entry.restore_status.as_str(), "outstanding");
        let updated = store.mark_restored(entry.id).unwrap();
        assert_eq!(updated.restore_status, RestoreStatus::Restored);
        assert_eq!(updated.time_unix_ms, entry.time_unix_ms);
        assert_eq!(updated.action, entry.action);
        assert_eq!(updated.before, entry.before);
        assert_eq!(updated.after, entry.after);
        assert_eq!(
            store.mark_restored(entry.id).unwrap_err(),
            SnapshotError::AlreadyRestored
        );
        assert_eq!(
            store.mark_restored(999).unwrap_err(),
            SnapshotError::UnknownEntry
        );

        let json = store.to_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let row = &value["change_log"][0];
        assert_eq!(row["time_unix_ms"], 1_700_000_000_000_i64);
        assert_eq!(row["action"], "power_plan");
        assert_eq!(row["before"], "balanced");
        assert_eq!(row["after"], "high_performance");
        assert_eq!(row["restore_status"], "restored");
        let loaded = SnapshotStore::from_json(&json).unwrap();
        assert_eq!(loaded.change_log(), store.change_log());
        assert_eq!(loaded.snapshots()[0].before, "balanced");
    }

    #[test]
    fn durable_snapshot_is_on_disk_before_mutate() {
        let path = scratch_path("before");
        let _keep = TempFile(path.clone());
        let mut store = SnapshotStore::open(&path).unwrap();
        let mut called = false;
        let entry = store
            .apply_guarded("power_plan", "balanced", 1_700_000_000_000, |token| {
                called = true;
                let loaded = SnapshotStore::load_from_path(&path).unwrap();
                assert_eq!(loaded.snapshots().len(), 1);
                assert_eq!(loaded.snapshots()[0].id, token.id);
                assert_eq!(loaded.snapshots()[0].before, "balanced");
                assert_eq!(loaded.snapshots()[0].action_id, "power_plan");
                assert!(
                    loaded.change_log().is_empty(),
                    "change log waits until mutate returns"
                );
                // mock: in-memory setting flip, not an OS change
                Ok("high_performance".into())
            })
            .unwrap();
        assert!(called);
        assert_eq!(entry.time_unix_ms, 1_700_000_000_000);
        assert_eq!(entry.action, "power_plan");
        assert_eq!(entry.before, "balanced");
        assert_eq!(entry.after, "high_performance");
        assert_eq!(entry.restore_status, RestoreStatus::Outstanding);
        let loaded = SnapshotStore::load_from_path(&path).unwrap();
        assert_eq!(loaded.change_log().len(), 1);
        assert_eq!(loaded.change_log()[0], entry);
        let restored = store.mark_restored(entry.id).unwrap();
        assert_eq!(restored.restore_status.as_str(), "restored");
        let reloaded = SnapshotStore::load_from_path(&path).unwrap();
        assert_eq!(
            reloaded.change_log()[0].restore_status,
            RestoreStatus::Restored
        );
        assert_eq!(reloaded.change_log()[0].before, "balanced");
        assert_eq!(reloaded.change_log()[0].after, "high_performance");
    }

    #[test]
    fn load_bumps_next_id_past_existing_ids() {
        let text = r#"{"schema_version":1,"next_id":1,"snapshots":[{"id":1,"action_id":"power_plan","captured_at_unix_ms":5,"before":"balanced"}],"change_log":[]}"#;
        let mut store = SnapshotStore::from_json(text).unwrap();
        let token = store.pending_token("power_plan").unwrap();
        assert_eq!(token.id, 1);
        let entry = store.record_applied(&token, "high_performance", 6).unwrap();
        assert_ne!(entry.id, token.id);
        assert_eq!(entry.snapshot_id, token.id);
    }

    #[test]
    fn unsupported_schema_and_orphan_log_are_rejected() {
        let schema = r#"{"schema_version":2,"next_id":1,"snapshots":[],"change_log":[]}"#;
        assert!(matches!(
            SnapshotStore::from_json(schema),
            Err(SnapshotError::UnsupportedSchema(2))
        ));
        let orphan = r#"{"schema_version":1,"next_id":3,"snapshots":[],"change_log":[{"id":2,"time_unix_ms":1,"action":"power_plan","before":"a","after":"b","restore_status":"outstanding","snapshot_id":1}]}"#;
        assert!(matches!(
            SnapshotStore::from_json(orphan),
            Err(SnapshotError::InvalidDocument(_))
        ));
    }
}
