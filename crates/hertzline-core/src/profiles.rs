//! Per-game profiles: apply on start, restore on exit (HZ-028 / AC-3.3, AC-3.4).
//!
//! Normal exit restores every setting the profile applied. Crash of the game or
//! of Hertzline while a profile is active uses [`crate::restore::recover_from_crash`]
//! (HZ-018 / AC-7.3). Process presence is injected; no undocumented scrape.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::action::{apply_with_snapshot, restore_with_log, ActionError, ActionMap};
use crate::games::{DetectedGame, GameSource};
use crate::restore::{recover_from_crash, RestoreAllReport, RestoreError};
use crate::snapshot::{SnapshotError, SnapshotStore};

/// Schema for the durable active-profile marker.
pub const ACTIVE_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Stable key for a detected install: `{source}:{id}`.
pub fn game_key(source: GameSource, id: &str) -> String {
    format!("{}:{}", source.as_str(), id)
}

/// User-defined action set bound to one game (AC-3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameProfile {
    pub game_key: String,
    pub name: String,
    /// Action ids applied in order when the game starts.
    pub action_ids: Vec<String>,
}

impl GameProfile {
    pub fn for_game(game: &DetectedGame, action_ids: Vec<String>) -> Self {
        Self {
            game_key: game_key(game.source, &game.id),
            name: game.name.clone(),
            action_ids,
        }
    }
}

/// Lifecycle phase of an applied profile session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilePhase {
    Active,
    Restored,
}

/// On-disk marker so crash recovery knows a profile was active (AC-3.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveProfileMarker {
    pub schema_version: u32,
    pub game_key: String,
    pub name: String,
    pub action_ids: Vec<String>,
    pub entry_ids: Vec<u64>,
    pub started_at_unix_ms: i64,
    pub phase: ProfilePhase,
}

/// Injected game-process presence (labeled mock in tests; no scrape).
pub trait GameRunning {
    fn is_running(&self) -> bool;
}

/// Labeled mock: tests flip [`MockGameRunning::running`].
#[derive(Debug, Clone, Copy)]
pub struct MockGameRunning {
    pub running: bool,
}

impl GameRunning for MockGameRunning {
    fn is_running(&self) -> bool {
        self.running
    }
}

/// Errors from profile apply / exit restore / crash recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    EmptyGameKey,
    MissingAction(String),
    AlreadyActive,
    NotActive,
    Action(ActionError),
    Snapshot(SnapshotError),
    Restore(RestoreError),
    MarkerIo(String),
    InvalidMarker(String),
    UnsupportedMarkerSchema(u32),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyGameKey => write!(f, "profile game_key is empty"),
            Self::MissingAction(id) => write!(f, "missing action `{id}`"),
            Self::AlreadyActive => write!(f, "profile session is already active"),
            Self::NotActive => write!(f, "profile session is not active"),
            Self::Action(err) => write!(f, "action: {err}"),
            Self::Snapshot(err) => write!(f, "snapshot: {err}"),
            Self::Restore(err) => write!(f, "restore: {err}"),
            Self::MarkerIo(reason) => write!(f, "active profile marker io: {reason}"),
            Self::InvalidMarker(reason) => write!(f, "invalid active profile marker: {reason}"),
            Self::UnsupportedMarkerSchema(v) => {
                write!(f, "unsupported active profile marker schema {v}")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<ActionError> for ProfileError {
    fn from(value: ActionError) -> Self {
        Self::Action(value)
    }
}

impl From<SnapshotError> for ProfileError {
    fn from(value: SnapshotError) -> Self {
        Self::Snapshot(value)
    }
}

impl From<RestoreError> for ProfileError {
    fn from(value: RestoreError) -> Self {
        Self::Restore(value)
    }
}

/// Active profile: applied actions + durable marker path (AC-3.3).
#[derive(Debug, Clone)]
pub struct ProfileSession {
    profile: GameProfile,
    entry_ids: Vec<u64>,
    phase: ProfilePhase,
    marker_path: Option<PathBuf>,
    started_at_unix_ms: i64,
}

impl ProfileSession {
    pub fn profile(&self) -> &GameProfile {
        &self.profile
    }

    pub fn phase(&self) -> ProfilePhase {
        self.phase
    }

    pub fn entry_ids(&self) -> &[u64] {
        &self.entry_ids
    }

    pub fn marker_path(&self) -> Option<&Path> {
        self.marker_path.as_deref()
    }

    /// Apply every profile action when the game starts (AC-3.3).
    ///
    /// Writes the active-profile marker before returning so a crash mid-session
    /// can run HZ-018 recovery. On a mid-apply failure, already-applied entries
    /// are restored newest-first before the error is returned.
    pub fn apply_on_start(
        profile: GameProfile,
        store: &mut SnapshotStore,
        actions: &mut dyn ActionMap,
        now_unix_ms: i64,
        marker_path: Option<&Path>,
    ) -> Result<Self, ProfileError> {
        if profile.game_key.trim().is_empty() {
            return Err(ProfileError::EmptyGameKey);
        }

        let mut entry_ids = Vec::new();
        for action_id in &profile.action_ids {
            let Some(action) = actions.get_mut(action_id) else {
                rollback_entries(&entry_ids, store, actions)?;
                return Err(ProfileError::MissingAction(action_id.clone()));
            };
            match apply_with_snapshot(action, store, now_unix_ms) {
                Ok(entry) => entry_ids.push(entry.id),
                Err(err) => {
                    let _ = rollback_entries(&entry_ids, store, actions);
                    return Err(ProfileError::Action(err));
                }
            }
        }

        let session = Self {
            profile,
            entry_ids,
            phase: ProfilePhase::Active,
            marker_path: marker_path.map(Path::to_path_buf),
            started_at_unix_ms: now_unix_ms,
        };
        if let Some(path) = marker_path {
            write_active_marker(path, &session.to_marker())?;
        }
        Ok(session)
    }

    /// Restore every setting this session applied (normal exit, AC-3.3).
    pub fn restore_on_exit(
        &mut self,
        store: &mut SnapshotStore,
        actions: &mut dyn ActionMap,
    ) -> Result<(), ProfileError> {
        if self.phase != ProfilePhase::Active {
            return Err(ProfileError::NotActive);
        }
        rollback_entries(&self.entry_ids, store, actions)?;
        self.phase = ProfilePhase::Restored;
        if let Some(path) = &self.marker_path {
            clear_active_marker(path)?;
        }
        Ok(())
    }

    /// When the injected game process stops, restore (AC-3.3). Returns true if restored.
    pub fn poll_game_exit(
        &mut self,
        game: &dyn GameRunning,
        store: &mut SnapshotStore,
        actions: &mut dyn ActionMap,
    ) -> Result<bool, ProfileError> {
        if self.phase != ProfilePhase::Active {
            return Ok(false);
        }
        if game.is_running() {
            return Ok(false);
        }
        self.restore_on_exit(store, actions)?;
        Ok(true)
    }

    fn to_marker(&self) -> ActiveProfileMarker {
        ActiveProfileMarker {
            schema_version: ACTIVE_PROFILE_SCHEMA_VERSION,
            game_key: self.profile.game_key.clone(),
            name: self.profile.name.clone(),
            action_ids: self.profile.action_ids.clone(),
            entry_ids: self.entry_ids.clone(),
            started_at_unix_ms: self.started_at_unix_ms,
            phase: self.phase,
        }
    }
}

fn rollback_entries(
    entry_ids: &[u64],
    store: &mut SnapshotStore,
    actions: &mut dyn ActionMap,
) -> Result<(), ProfileError> {
    for entry_id in entry_ids.iter().rev() {
        let entry = store
            .change_log()
            .iter()
            .find(|row| row.id == *entry_id)
            .cloned()
            .ok_or(ProfileError::Action(ActionError::UnknownEntry))?;
        if entry.restore_status == crate::snapshot::RestoreStatus::Restored {
            continue;
        }
        let Some(action) = actions.get_mut(&entry.action) else {
            return Err(ProfileError::MissingAction(entry.action));
        };
        restore_with_log(action, store, *entry_id)?;
    }
    Ok(())
}

/// Write the active-profile marker (creates parent dirs).
pub fn write_active_marker(path: &Path, marker: &ActiveProfileMarker) -> Result<(), ProfileError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| ProfileError::MarkerIo(e.to_string()))?;
        }
    }
    let json =
        serde_json::to_string_pretty(marker).map_err(|e| ProfileError::MarkerIo(e.to_string()))?;
    fs::write(path, json).map_err(|e| ProfileError::MarkerIo(e.to_string()))
}

/// Load an active-profile marker if present.
pub fn load_active_marker(path: &Path) -> Result<Option<ActiveProfileMarker>, ProfileError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| ProfileError::MarkerIo(e.to_string()))?;
    let marker: ActiveProfileMarker =
        serde_json::from_str(&text).map_err(|e| ProfileError::InvalidMarker(e.to_string()))?;
    if marker.schema_version != ACTIVE_PROFILE_SCHEMA_VERSION {
        return Err(ProfileError::UnsupportedMarkerSchema(marker.schema_version));
    }
    Ok(Some(marker))
}

/// Remove the active-profile marker if it exists.
pub fn clear_active_marker(path: &Path) -> Result<(), ProfileError> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|e| ProfileError::MarkerIo(e.to_string()))
}

/// AC-3.4: if a marker remains, run HZ-018 crash restore then clear the marker.
pub fn recover_active_profile_after_crash(
    marker_path: &Path,
    store_path: &Path,
    actions: &mut dyn ActionMap,
) -> Result<Option<RestoreAllReport>, ProfileError> {
    let Some(marker) = load_active_marker(marker_path)? else {
        return Ok(None);
    };
    if marker.phase != ProfilePhase::Active {
        clear_active_marker(marker_path)?;
        return Ok(None);
    }
    let report = recover_from_crash(store_path, actions)?;
    clear_active_marker(marker_path)?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, Action, MockToggleAction};
    use crate::games::GameSource;
    use crate::restore::{MapActionRegistry, RestoreTrigger};
    use crate::snapshot::RestoreStatus;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("hertzline-hz028-{label}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn sample_game() -> DetectedGame {
        DetectedGame::new(
            GameSource::Steam,
            "480",
            "Spacewar",
            PathBuf::from("C:/Games/Spacewar"),
        )
    }

    #[test]
    fn game_key_uses_source_and_id() {
        assert_eq!(game_key(GameSource::Steam, "480"), "steam:480");
        assert_eq!(game_key(GameSource::Manual, "abc"), "manual:abc");
    }

    #[test]
    fn apply_on_start_then_restore_on_exit() {
        // mock: in-memory toggles, not real OS tweaks
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(MockToggleAction::new("mock_a", "off", "on")));
        registry.insert(Box::new(MockToggleAction::new("mock_b", "low", "high")));

        let profile = GameProfile::for_game(&sample_game(), vec!["mock_a".into(), "mock_b".into()]);
        let dir = temp_dir("exit");
        let marker = dir.join("active_profile.json");

        let mut session = ProfileSession::apply_on_start(
            profile,
            &mut store,
            &mut registry,
            1_000,
            Some(&marker),
        )
        .expect("apply");
        assert_eq!(session.phase(), ProfilePhase::Active);
        assert!(marker.exists());
        assert_eq!(registry.get("mock_a").unwrap().read_state().unwrap(), "on");
        assert_eq!(
            registry.get("mock_b").unwrap().read_state().unwrap(),
            "high"
        );
        assert_eq!(store.change_log().len(), 2);

        session
            .restore_on_exit(&mut store, &mut registry)
            .expect("restore");
        assert_eq!(session.phase(), ProfilePhase::Restored);
        assert!(!marker.exists());
        assert_eq!(registry.get("mock_a").unwrap().read_state().unwrap(), "off");
        assert_eq!(registry.get("mock_b").unwrap().read_state().unwrap(), "low");
        assert!(store
            .change_log()
            .iter()
            .all(|e| e.restore_status == RestoreStatus::Restored));
    }

    #[test]
    fn poll_game_exit_restores_when_process_stops() {
        // mock: GameRunning flipped by test, not a real process probe
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(MockToggleAction::new("mock_toggle", "off", "on")));
        let profile = GameProfile {
            game_key: "steam:1".into(),
            name: "Mock Game".into(),
            action_ids: vec!["mock_toggle".into()],
        };
        let mut session =
            ProfileSession::apply_on_start(profile, &mut store, &mut registry, 50, None)
                .expect("apply");

        let still = MockGameRunning { running: true };
        assert!(!session
            .poll_game_exit(&still, &mut store, &mut registry)
            .unwrap());
        assert_eq!(session.phase(), ProfilePhase::Active);

        let stopped = MockGameRunning { running: false };
        assert!(session
            .poll_game_exit(&stopped, &mut store, &mut registry)
            .unwrap());
        assert_eq!(session.phase(), ProfilePhase::Restored);
        assert_eq!(
            registry.get("mock_toggle").unwrap().read_state().unwrap(),
            "off"
        );
    }

    #[test]
    fn crash_recovery_uses_hz018_restore_all() {
        // mock: durable store + marker left as if Hertzline crashed mid-session
        let dir = temp_dir("crash");
        let store_path = dir.join("snapshots.json");
        let marker_path = dir.join("active_profile.json");

        let mut store = SnapshotStore::open(&store_path).expect("open store");
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(MockToggleAction::new("mock_toggle", "off", "on")));
        let profile = GameProfile {
            game_key: "gog:9".into(),
            name: "Crash Fixture".into(),
            action_ids: vec!["mock_toggle".into()],
        };
        let session = ProfileSession::apply_on_start(
            profile,
            &mut store,
            &mut registry,
            99,
            Some(&marker_path),
        )
        .expect("apply");
        drop(session);
        drop(store);

        // Simulate crash: live state still "on", marker still Active, process gone.
        let dirty = MockToggleAction::new("mock_toggle", "on", "on");
        assert_eq!(dirty.read_state().unwrap(), "on");

        let mut recovery = MapActionRegistry::new();
        recovery.insert(Box::new(MockToggleAction::new("mock_toggle", "on", "on")));
        let report = recover_active_profile_after_crash(&marker_path, &store_path, &mut recovery)
            .expect("recover")
            .expect("marker present");
        assert_eq!(report.trigger, RestoreTrigger::Crash);
        assert!(report.ok);
        assert!(!marker_path.exists());
        assert_eq!(
            recovery.get("mock_toggle").unwrap().read_state().unwrap(),
            "off"
        );
    }

    #[test]
    fn recover_without_marker_is_noop() {
        let dir = temp_dir("noop");
        let mut registry = MapActionRegistry::new();
        let out = recover_active_profile_after_crash(
            &dir.join("missing.json"),
            &dir.join("missing-store.json"),
            &mut registry,
        )
        .expect("ok");
        assert!(out.is_none());
    }

    #[test]
    fn mid_apply_failure_rolls_back_earlier_actions() {
        // mock: second action fails apply
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        registry.insert(Box::new(MockToggleAction::new("mock_ok", "off", "on")));
        registry.insert(Box::new(
            MockToggleAction::new("mock_fail", "a", "b").fail_apply("mock apply boom"),
        ));
        let profile = GameProfile {
            game_key: "epic:x".into(),
            name: "Partial".into(),
            action_ids: vec!["mock_ok".into(), "mock_fail".into()],
        };
        let err = ProfileSession::apply_on_start(profile, &mut store, &mut registry, 7, None)
            .expect_err("must fail");
        assert!(matches!(
            err,
            ProfileError::Action(ActionError::ApplyFailed(_))
        ));
        assert_eq!(
            registry.get("mock_ok").unwrap().read_state().unwrap(),
            "off"
        );
    }

    #[test]
    fn missing_action_id_errors() {
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        let profile = GameProfile {
            game_key: "steam:1".into(),
            name: "Missing".into(),
            action_ids: vec!["nope".into()],
        };
        let err = ProfileSession::apply_on_start(profile, &mut store, &mut registry, 1, None)
            .expect_err("missing");
        assert_eq!(err, ProfileError::MissingAction("nope".into()));
    }

    #[test]
    fn empty_game_key_rejected() {
        let mut store = SnapshotStore::new();
        let mut registry = MapActionRegistry::new();
        let profile = GameProfile {
            game_key: "  ".into(),
            name: "Bad".into(),
            action_ids: vec![],
        };
        let err = ProfileSession::apply_on_start(profile, &mut store, &mut registry, 1, None)
            .expect_err("empty");
        assert_eq!(err, ProfileError::EmptyGameKey);
    }

    #[test]
    fn apply_with_snapshot_still_works_inside_profile_path() {
        // sanity: profile stack shares HZ-017 guard
        let mut store = SnapshotStore::new();
        let mut action = MockToggleAction::new("mock_toggle", "off", "on");
        let entry = apply_with_snapshot(&mut action, &mut store, 1).unwrap();
        assert_eq!(entry.action, "mock_toggle");
    }
}
