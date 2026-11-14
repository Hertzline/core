//! Windows System Restore point offer gate (HZ-019 / AC-7.4).
//!
//! On Windows, before the first system-level change Hertzline must offer a
//! System Restore point. Refusal is recorded. After a refusal, every later
//! system-level change still requires explicit consent (not a blanket skip).
//! Non-Windows hosts: not applicable (ARCHITECTURE capability matrix = N).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::system_restore_gate_path;

/// On-disk schema. Reject other versions until a migrator exists.
pub const GATE_SCHEMA_VERSION: u32 = 1;

/// Default description passed to the helper create_restore_point op.
pub const DEFAULT_RESTORE_POINT_DESCRIPTION: &str = "Hertzline before system change";

/// Whether the host OS exposes System Restore (Windows only).
pub fn is_applicable() -> bool {
    cfg!(windows)
}

/// What the UI / caller must do before applying a system-level action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareVerdict {
    /// Linux / macOS / non-system action.
    NotApplicable,
    /// First system-level change: show the System Restore offer.
    OfferRestorePoint,
    /// First offer was accepted; restore point already created (or attempted).
    Ready,
    /// User refused the restore-point offer; this action needs explicit consent.
    RequireExplicitConsent,
}

/// Outcome of the first System Restore offer (AC-7.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum FirstOffer {
    Accepted {
        at_unix_ms: i64,
        sequence_number: Option<i64>,
        detail: String,
    },
    Refused {
        at_unix_ms: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GateDocument {
    schema_version: u32,
    first_offer: Option<FirstOffer>,
    /// Action ids that received explicit consent after a refusal (AC-7.4).
    explicit_consents: Vec<ConsentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConsentRecord {
    action_id: String,
    at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    EmptyAction,
    NotSystemLevel,
    MustOfferFirst,
    ConsentRequired,
    OfferAlreadyRecorded,
    StoreIo(String),
    InvalidDocument(String),
    UnsupportedSchema(u32),
    NotApplicable,
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAction => write!(f, "action id is empty"),
            Self::NotSystemLevel => write!(f, "action is not system-level"),
            Self::MustOfferFirst => {
                write!(
                    f,
                    "System Restore point must be offered before first system-level change"
                )
            }
            Self::ConsentRequired => {
                write!(
                    f,
                    "explicit consent required after System Restore offer was refused"
                )
            }
            Self::OfferAlreadyRecorded => write!(f, "first System Restore offer already recorded"),
            Self::StoreIo(reason) => write!(f, "system restore gate io error: {reason}"),
            Self::InvalidDocument(reason) => {
                write!(f, "invalid system restore gate document: {reason}")
            }
            Self::UnsupportedSchema(version) => {
                write!(
                    f,
                    "unsupported system restore gate schema version {version}"
                )
            }
            Self::NotApplicable => write!(f, "System Restore offer is not applicable on this OS"),
        }
    }
}

impl std::error::Error for GateError {}

/// Durable AC-7.4 gate: offer once, record refusal, require consent afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRestoreGate {
    path: Option<PathBuf>,
    /// Host applicability; tests may force Windows semantics on any OS.
    applicable: bool,
    first_offer: Option<FirstOffer>,
    explicit_consents: HashSet<String>,
    consent_times: Vec<ConsentRecord>,
}

impl SystemRestoreGate {
    /// In-memory gate (unit tests). Does not touch disk.
    pub fn new() -> Self {
        Self {
            path: None,
            applicable: is_applicable(),
            first_offer: None,
            explicit_consents: HashSet::new(),
            consent_times: Vec::new(),
        }
    }

    /// Labeled mock: force Windows AC-7.4 semantics for cross-OS unit tests.
    pub fn new_windows_semantics_for_tests() -> Self {
        let mut gate = Self::new();
        gate.applicable = true;
        gate
    }

    /// Open or create the durable gate file.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, GateError> {
        let path = path.into();
        if path.exists() {
            Self::load_from_path(&path)
        } else {
            let mut gate = Self::new();
            gate.path = Some(path);
            gate.persist()?;
            Ok(gate)
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self, GateError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| GateError::StoreIo(format!("read {}: {e}", path.display())))?;
        let doc: GateDocument =
            serde_json::from_str(&raw).map_err(|e| GateError::InvalidDocument(e.to_string()))?;
        if doc.schema_version != GATE_SCHEMA_VERSION {
            return Err(GateError::UnsupportedSchema(doc.schema_version));
        }
        let mut explicit_consents = HashSet::new();
        for row in &doc.explicit_consents {
            explicit_consents.insert(row.action_id.clone());
        }
        Ok(Self {
            path: Some(path.to_path_buf()),
            applicable: is_applicable(),
            first_offer: doc.first_offer,
            explicit_consents,
            consent_times: doc.explicit_consents,
        })
    }

    /// Default path under Hertzline data dir (env override wins).
    pub fn open_default() -> Result<Self, GateError> {
        Self::open(system_restore_gate_path())
    }

    pub fn first_offer(&self) -> Option<&FirstOffer> {
        self.first_offer.as_ref()
    }

    pub fn has_explicit_consent(&self, action_id: &str) -> bool {
        self.explicit_consents.contains(action_id)
    }

    /// Decide what must happen before applying `action_id` when `is_system_level`.
    pub fn prepare(
        &self,
        action_id: &str,
        is_system_level: bool,
    ) -> Result<PrepareVerdict, GateError> {
        if !self.applicable {
            return Ok(PrepareVerdict::NotApplicable);
        }
        if !is_system_level {
            return Ok(PrepareVerdict::NotApplicable);
        }
        let id = action_id.trim();
        if id.is_empty() {
            return Err(GateError::EmptyAction);
        }
        match &self.first_offer {
            None => Ok(PrepareVerdict::OfferRestorePoint),
            Some(FirstOffer::Accepted { .. }) => Ok(PrepareVerdict::Ready),
            Some(FirstOffer::Refused { .. }) => {
                if self.explicit_consents.contains(id) {
                    Ok(PrepareVerdict::Ready)
                } else {
                    Ok(PrepareVerdict::RequireExplicitConsent)
                }
            }
        }
    }

    /// Block apply unless prepare says Ready / NotApplicable.
    pub fn assert_allowed(&self, action_id: &str, is_system_level: bool) -> Result<(), GateError> {
        match self.prepare(action_id, is_system_level)? {
            PrepareVerdict::NotApplicable | PrepareVerdict::Ready => Ok(()),
            PrepareVerdict::OfferRestorePoint => Err(GateError::MustOfferFirst),
            PrepareVerdict::RequireExplicitConsent => Err(GateError::ConsentRequired),
        }
    }

    /// Record that the user accepted the first offer (helper create result attached).
    pub fn record_accepted(
        &mut self,
        at_unix_ms: i64,
        sequence_number: Option<i64>,
        detail: impl Into<String>,
    ) -> Result<&FirstOffer, GateError> {
        if !self.applicable {
            return Err(GateError::NotApplicable);
        }
        if self.first_offer.is_some() {
            return Err(GateError::OfferAlreadyRecorded);
        }
        self.first_offer = Some(FirstOffer::Accepted {
            at_unix_ms,
            sequence_number,
            detail: detail.into(),
        });
        self.persist()?;
        Ok(self.first_offer.as_ref().expect("just set"))
    }

    /// Record that the user refused the System Restore point offer (AC-7.4).
    pub fn record_refused(&mut self, at_unix_ms: i64) -> Result<&FirstOffer, GateError> {
        if !self.applicable {
            return Err(GateError::NotApplicable);
        }
        if self.first_offer.is_some() {
            return Err(GateError::OfferAlreadyRecorded);
        }
        self.first_offer = Some(FirstOffer::Refused { at_unix_ms });
        self.persist()?;
        Ok(self.first_offer.as_ref().expect("just set"))
    }

    /// After a refusal, record explicit consent for one system-level action.
    pub fn record_explicit_consent(
        &mut self,
        action_id: &str,
        at_unix_ms: i64,
    ) -> Result<(), GateError> {
        if !self.applicable {
            return Err(GateError::NotApplicable);
        }
        let id = action_id.trim();
        if id.is_empty() {
            return Err(GateError::EmptyAction);
        }
        match &self.first_offer {
            Some(FirstOffer::Refused { .. }) => {}
            Some(FirstOffer::Accepted { .. }) => {
                // Consent bookkeeping is only required after refusal.
                return Ok(());
            }
            None => return Err(GateError::MustOfferFirst),
        }
        if self.explicit_consents.insert(id.to_string()) {
            self.consent_times.push(ConsentRecord {
                action_id: id.to_string(),
                at_unix_ms,
            });
            self.persist()?;
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), GateError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GateError::StoreIo(format!("create {}: {e}", parent.display())))?;
        }
        let doc = GateDocument {
            schema_version: GATE_SCHEMA_VERSION,
            first_offer: self.first_offer.clone(),
            explicit_consents: self.consent_times.clone(),
        };
        let encoded = serde_json::to_string_pretty(&doc)
            .map_err(|e| GateError::StoreIo(format!("serialize: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, encoded.as_bytes())
            .map_err(|e| GateError::StoreIo(format!("write {}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| GateError::StoreIo(format!("rename {}: {e}", path.display())))?;
        Ok(())
    }
}

impl Default for SystemRestoreGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the durable gate path (env override wins).
pub fn gate_path() -> PathBuf {
    system_restore_gate_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_path(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hertzline-sr-gate-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("system_restore_gate.json")
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
    fn non_system_level_is_not_applicable() {
        let gate = SystemRestoreGate::new();
        assert_eq!(
            gate.prepare("mock_toggle", false).unwrap(),
            PrepareVerdict::NotApplicable
        );
        assert!(gate.assert_allowed("mock_toggle", false).is_ok());
    }

    #[test]
    fn host_gate_not_applicable_on_non_windows() {
        let gate = SystemRestoreGate::new();
        if is_applicable() {
            assert_eq!(
                gate.prepare("win_power_plan", true).unwrap(),
                PrepareVerdict::OfferRestorePoint
            );
        } else {
            assert_eq!(
                gate.prepare("win_power_plan", true).unwrap(),
                PrepareVerdict::NotApplicable
            );
            assert_eq!(
                gate.clone().record_refused(1).unwrap_err(),
                GateError::NotApplicable
            );
        }
    }

    #[test]
    fn first_system_change_requires_offer() {
        // mock: forced Windows semantics (cross-OS)
        let gate = SystemRestoreGate::new_windows_semantics_for_tests();
        assert_eq!(
            gate.prepare("win_power_plan", true).unwrap(),
            PrepareVerdict::OfferRestorePoint
        );
        assert_eq!(
            gate.assert_allowed("win_power_plan", true).unwrap_err(),
            GateError::MustOfferFirst
        );
    }

    #[test]
    fn refusal_is_recorded_and_later_needs_consent() {
        // mock: forced Windows semantics
        let mut gate = SystemRestoreGate::new_windows_semantics_for_tests();
        gate.record_refused(100).unwrap();
        assert!(matches!(
            gate.first_offer(),
            Some(FirstOffer::Refused { at_unix_ms: 100 })
        ));
        assert_eq!(
            gate.prepare("win_hags", true).unwrap(),
            PrepareVerdict::RequireExplicitConsent
        );
        assert_eq!(
            gate.assert_allowed("win_hags", true).unwrap_err(),
            GateError::ConsentRequired
        );

        gate.record_explicit_consent("win_hags", 101).unwrap();
        assert_eq!(
            gate.prepare("win_hags", true).unwrap(),
            PrepareVerdict::Ready
        );
        assert!(gate.assert_allowed("win_hags", true).is_ok());

        // a different later system-level change still needs its own consent
        assert_eq!(
            gate.prepare("win_game_mode", true).unwrap(),
            PrepareVerdict::RequireExplicitConsent
        );
    }

    #[test]
    fn accept_allows_ready_without_per_action_consent() {
        // mock: forced Windows semantics
        let mut gate = SystemRestoreGate::new_windows_semantics_for_tests();
        gate.record_accepted(50, Some(42), "mock: restore point created")
            .unwrap();
        assert_eq!(
            gate.prepare("win_power_plan", true).unwrap(),
            PrepareVerdict::Ready
        );
        assert!(gate.assert_allowed("win_power_plan", true).is_ok());
        assert_eq!(
            gate.prepare("win_hags", true).unwrap(),
            PrepareVerdict::Ready
        );
    }

    #[test]
    fn double_offer_record_is_rejected() {
        let mut gate = SystemRestoreGate::new_windows_semantics_for_tests();
        gate.record_refused(1).unwrap();
        assert_eq!(
            gate.record_accepted(2, None, "late").unwrap_err(),
            GateError::OfferAlreadyRecorded
        );
        assert_eq!(
            gate.record_refused(3).unwrap_err(),
            GateError::OfferAlreadyRecorded
        );
    }

    #[test]
    fn durable_round_trip_persists_refusal_and_consent() {
        let path = scratch_path("durable");
        let _keep = TempFile(path.clone());
        {
            let mut gate = SystemRestoreGate::new_windows_semantics_for_tests();
            gate.path = Some(path.clone());
            gate.record_refused(10).unwrap();
            gate.record_explicit_consent("win_power_plan", 11).unwrap();
        }
        let mut loaded = SystemRestoreGate::load_from_path(&path).unwrap();
        // load uses host applicability; force Windows semantics for assert
        loaded.applicable = true;
        assert!(matches!(
            loaded.first_offer(),
            Some(FirstOffer::Refused { at_unix_ms: 10 })
        ));
        assert!(loaded.has_explicit_consent("win_power_plan"));
        assert!(!loaded.has_explicit_consent("win_hags"));
        assert_eq!(
            loaded.prepare("win_power_plan", true).unwrap(),
            PrepareVerdict::Ready
        );
        assert_eq!(
            loaded.prepare("win_hags", true).unwrap(),
            PrepareVerdict::RequireExplicitConsent
        );
    }

    #[test]
    fn empty_action_id_rejected() {
        let gate = SystemRestoreGate::new_windows_semantics_for_tests();
        assert_eq!(
            gate.prepare("  ", true).unwrap_err(),
            GateError::EmptyAction
        );
    }
}
