//! Feral Interactive GameMode integration (AC-4.11).
//!
//! Session D-Bus service `com.feralinteractive.GameMode` at
//! `/com/feralinteractive/GameMode`:
//! - RegisterGame(i) -> i
//! - UnregisterGame(i) -> i
//! - QueryStatus(i) -> i  (0 inactive, 1 active, 2 active+this client registered)
//!
//! Official: https://github.com/FeralInteractive/gamemode
//! Client header documents the same semantics via gamemode_query_status().

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.feral_gamemode";
pub const SOURCE_URL: &str = "https://github.com/FeralInteractive/gamemode";
pub const DBUS_DEST: &str = "com.feralinteractive.GameMode";
pub const DBUS_PATH: &str = "/com/feralinteractive/GameMode";
pub const DBUS_IFACE: &str = "com.feralinteractive.GameMode";

/// Encoded state: `pid=<n>;status=<code|unavailable|missing>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeralGameModeState {
    pub pid: u32,
    /// QueryStatus result, or "unavailable" / "missing" when daemon absent.
    pub status: String,
}

impl FeralGameModeState {
    pub fn encode(&self) -> String {
        format!("pid={};status={}", self.pid, self.status)
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut pid = None;
        let mut status = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "pid" => {
                    pid = Some(v.trim().parse::<u32>().map_err(|_| {
                        ActionError::RestoreFailed(format!("invalid pid in state: {v}"))
                    })?);
                }
                "status" => status = Some(v.trim().to_string()),
                _ => {}
            }
        }
        Ok(Self {
            pid: pid.ok_or_else(|| ActionError::RestoreFailed("missing pid".into()))?,
            status: status.ok_or_else(|| ActionError::RestoreFailed("missing status".into()))?,
        })
    }
}

/// Probe / register / unregister a user-chosen PID with GameMode.
pub trait FeralGameModeBackend {
    fn probe(&self) -> Result<bool, ActionError>;
    fn query_status(&self, pid: u32) -> Result<i32, ActionError>;
    fn register(&self, pid: u32) -> Result<i32, ActionError>;
    fn unregister(&self, pid: u32) -> Result<i32, ActionError>;
}

/// Labeled mock for unit tests (not a real GameMode daemon).
#[derive(Debug, Clone)]
pub struct MockFeralGameModeBackend {
    pub available: bool,
    pub registered: Vec<u32>,
    pub active: bool,
}

impl MockFeralGameModeBackend {
    pub fn new_available() -> Self {
        Self {
            available: true,
            registered: Vec::new(),
            active: false,
        }
    }

    pub fn missing() -> Self {
        Self {
            available: false,
            registered: Vec::new(),
            active: false,
        }
    }
}

impl FeralGameModeBackend for std::cell::RefCell<MockFeralGameModeBackend> {
    fn probe(&self) -> Result<bool, ActionError> {
        Ok(self.borrow().available)
    }

    fn query_status(&self, pid: u32) -> Result<i32, ActionError> {
        let inner = self.borrow();
        if !inner.available {
            return Err(ActionError::NotPossible(
                "mock: Feral GameMode daemon unavailable".into(),
            ));
        }
        if inner.registered.contains(&pid) {
            Ok(2)
        } else if inner.active || !inner.registered.is_empty() {
            Ok(1)
        } else {
            Ok(0)
        }
    }

    fn register(&self, pid: u32) -> Result<i32, ActionError> {
        let mut inner = self.borrow_mut();
        if !inner.available {
            return Err(ActionError::NotPossible(
                "mock: Feral GameMode daemon unavailable".into(),
            ));
        }
        if !inner.registered.contains(&pid) {
            inner.registered.push(pid);
        }
        inner.active = true;
        Ok(0)
    }

    fn unregister(&self, pid: u32) -> Result<i32, ActionError> {
        let mut inner = self.borrow_mut();
        if !inner.available {
            return Err(ActionError::NotPossible(
                "mock: Feral GameMode daemon unavailable".into(),
            ));
        }
        inner.registered.retain(|p| *p != pid);
        if inner.registered.is_empty() {
            inner.active = false;
        }
        Ok(0)
    }
}

/// Register a game PID with Feral GameMode (AC-4.11).
pub struct FeralGameModeAction<B: FeralGameModeBackend> {
    backend: B,
    pid: u32,
    explanation: ActionExplanation,
}

impl<B: FeralGameModeBackend> FeralGameModeAction<B> {
    pub fn new(backend: B, pid: u32) -> Self {
        let mut explanation = ActionExplanation::new(
            "Request Feral GameMode for a user-chosen game process via the documented D-Bus API (RegisterGame).",
            "GameMode can raise CPU governor / platform profile and related host tweaks while the PID is registered. Effect is machine-specific; measure locally.",
            "Low--medium: depends on gamemoded config (governor, renice, ioprio). Requires gamemoded installed and reachable on the session bus.",
            "UnregisterGame for the same PID, or wait until the process exits (daemon reaps clients).",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            pid,
            explanation,
        }
    }

    pub fn for_pid(backend: B, pid: u32) -> Self {
        Self::new(backend, pid)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl<B: FeralGameModeBackend> Action for FeralGameModeAction<B> {
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
        if !self.backend.probe()? {
            return Ok(FeralGameModeState {
                pid: self.pid,
                status: "unavailable".into(),
            }
            .encode());
        }
        match self.backend.query_status(self.pid) {
            Ok(code) => Ok(FeralGameModeState {
                pid: self.pid,
                status: code.to_string(),
            }
            .encode()),
            Err(ActionError::NotPossible(_)) => Ok(FeralGameModeState {
                pid: self.pid,
                status: "unavailable".into(),
            }
            .encode()),
            Err(err) => Err(err),
        }
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        if !self.backend.probe()? {
            return Err(ActionError::NotPossible(
                "Feral GameMode (gamemoded) is not available on this system".into(),
            ));
        }
        let status = self.backend.register(self.pid)?;
        if status < 0 {
            return Err(ActionError::ApplyFailed(format!(
                "RegisterGame returned {status}; see {SOURCE_URL}"
            )));
        }
        Ok(())
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = FeralGameModeState::decode(before)?;
        self.pid = state.pid;
        if state.status == "unavailable" || state.status == "missing" {
            // Nothing to restore if daemon was absent; best-effort unregister if now present.
            if self.backend.probe().unwrap_or(false) {
                let _ = self.backend.unregister(self.pid);
            }
            return Ok(());
        }
        if !self.backend.probe()? {
            return Err(ActionError::RestoreFailed(
                "Feral GameMode daemon unavailable during restore".into(),
            ));
        }
        // If before was registered (2), leave registered; otherwise unregister.
        if state.status == "2" {
            let status = self.backend.register(self.pid)?;
            if status < 0 {
                return Err(ActionError::RestoreFailed(format!(
                    "RegisterGame during restore returned {status}"
                )));
            }
        } else {
            let status = self.backend.unregister(self.pid)?;
            if status < 0 {
                return Err(ActionError::RestoreFailed(format!(
                    "UnregisterGame returned {status}"
                )));
            }
        }
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = FeralGameModeState::decode(expected)?;
        let actual_state = FeralGameModeState::decode(&actual)?;
        if actual_state.pid == expected_state.pid && actual_state.status == expected_state.status {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.to_string(),
                actual,
            })
        }
    }
}

#[cfg(target_os = "linux")]
pub mod os {
    use super::*;
    use std::process::Command;

    /// Live GameMode backend via `busctl --user` (systemd D-Bus CLI).
    pub struct OsFeralGameModeBackend;

    impl FeralGameModeBackend for OsFeralGameModeBackend {
        fn probe(&self) -> Result<bool, ActionError> {
            let output = Command::new("busctl")
                .args(["--user", "status", DBUS_DEST])
                .output()
                .map_err(|e| ActionError::NotPossible(format!("busctl unavailable: {e}")))?;
            Ok(output.status.success())
        }

        fn query_status(&self, pid: u32) -> Result<i32, ActionError> {
            dbus_call("QueryStatus", pid)
        }

        fn register(&self, pid: u32) -> Result<i32, ActionError> {
            dbus_call("RegisterGame", pid)
        }

        fn unregister(&self, pid: u32) -> Result<i32, ActionError> {
            dbus_call("UnregisterGame", pid)
        }
    }

    fn dbus_call(method: &str, pid: u32) -> Result<i32, ActionError> {
        let output = Command::new("busctl")
            .args([
                "--user",
                "call",
                DBUS_DEST,
                DBUS_PATH,
                DBUS_IFACE,
                method,
                "i",
                &pid.to_string(),
            ])
            .output()
            .map_err(|e| {
                ActionError::ApplyFailed(format!("busctl {method} failed to spawn: {e}"))
            })?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(ActionError::NotPossible(format!(
                "GameMode {method} failed: {err}; see {SOURCE_URL}"
            )));
        }
        parse_busctl_i(&String::from_utf8_lossy(&output.stdout))
    }

    /// busctl prints e.g. `i 0` or `i 2`.
    fn parse_busctl_i(stdout: &str) -> Result<i32, ActionError> {
        let trimmed = stdout.trim();
        let mut parts = trimmed.split_whitespace();
        let tag = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        if tag != "i" {
            return Err(ActionError::ApplyFailed(format!(
                "unexpected busctl reply (want i N): {trimmed}"
            )));
        }
        value
            .parse::<i32>()
            .map_err(|_| ActionError::ApplyFailed(format!("invalid busctl int: {trimmed}")))
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn probe_is_honest_when_missing() {
            let available = OsFeralGameModeBackend.probe().unwrap_or(false);
            // Either available or not -- never panic; label only.
            let _ = available;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use std::cell::RefCell;

    #[test]
    fn mock_apply_restore_verify() {
        let backend = RefCell::new(MockFeralGameModeBackend::new_available());
        let mut action = FeralGameModeAction::for_pid(backend, 4242);
        let before = action.read_state().expect("read");
        assert!(before.contains("status=0"));
        action.apply().expect("apply");
        let after = action.read_state().expect("after");
        assert!(after.contains("status=2"));
        action.restore(&before).expect("restore");
        let restored = action.read_state().expect("restored");
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
        assert!(restored.contains("status=0"));
    }

    #[test]
    fn mock_missing_is_not_possible() {
        let backend = RefCell::new(MockFeralGameModeBackend::missing());
        let mut action = FeralGameModeAction::for_pid(backend, 1);
        let state = action.read_state().expect("read");
        assert!(state.contains("unavailable"));
        let err = action.apply().expect_err("apply");
        assert!(matches!(err, ActionError::NotPossible(_)));
    }

    #[test]
    fn explanation_complete() {
        let backend = RefCell::new(MockFeralGameModeBackend::new_available());
        let action = FeralGameModeAction::for_pid(backend, 1);
        let e = action.explanation();
        assert!(!e.what.trim().is_empty());
        assert!(!e.expected_effect.trim().is_empty());
        assert!(!e.risk.trim().is_empty());
        assert!(!e.how_to_undo.trim().is_empty());
        assert!(
            !e.expected_effect.contains("+") || !e.expected_effect.to_lowercase().contains("fps")
        );
    }

    #[test]
    fn state_round_trip() {
        let s = FeralGameModeState {
            pid: 99,
            status: "2".into(),
        };
        assert_eq!(FeralGameModeState::decode(&s.encode()).unwrap(), s);
    }
}
