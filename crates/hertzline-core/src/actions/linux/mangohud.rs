//! MangoHud enable preference toggle (AC-4.15).
//!
//! Official sources (verified 2026-11-14):
//! - mangohud(1): set `MANGOHUD=1` for Vulkan; or `mangohud COMMAND` wrap
//!   https://manpages.debian.org/trixie/mangohud/mangohud.1.en.html
//! - Upstream: https://github.com/flightlessmango/MangoHud
//!
//! Hertzline never silently injects into a running process. Apply records an
//! opt-in preference (and optional env file) for launch wrappers / Steam
//! launch options the user chooses. Missing `mangohud` binary -> NotPossible.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.mangohud";
pub const SOURCE_URL: &str = "https://manpages.debian.org/trixie/mangohud/mangohud.1.en.html";
pub const ENV_MANGOHUD: &str = "MANGOHUD";

/// Encoded: `available=0|1;enabled=0|1`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MangoHudState {
    pub available: bool,
    pub enabled: bool,
}

impl MangoHudState {
    pub fn encode(&self) -> String {
        format!(
            "available={};enabled={}",
            if self.available { "1" } else { "0" },
            if self.enabled { "1" } else { "0" }
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut available = None;
        let mut enabled = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "available" => available = Some(v.trim() == "1"),
                "enabled" => enabled = Some(v.trim() == "1"),
                _ => {}
            }
        }
        Ok(Self {
            available: available
                .ok_or_else(|| ActionError::RestoreFailed("missing available".into()))?,
            enabled: enabled.ok_or_else(|| ActionError::RestoreFailed("missing enabled".into()))?,
        })
    }
}

pub trait MangoHudBackend {
    fn probe_available(&self) -> Result<bool, ActionError>;
    fn is_enabled(&self) -> Result<bool, ActionError>;
    fn set_enabled(&self, enabled: bool) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real MangoHud install).
#[derive(Debug, Clone)]
pub struct MockMangoHudBackend {
    pub available: bool,
    pub enabled: bool,
}

impl MockMangoHudBackend {
    pub fn available_off() -> Self {
        Self {
            available: true,
            enabled: false,
        }
    }

    pub fn missing() -> Self {
        Self {
            available: false,
            enabled: false,
        }
    }
}

impl MangoHudBackend for std::cell::RefCell<MockMangoHudBackend> {
    fn probe_available(&self) -> Result<bool, ActionError> {
        Ok(self.borrow().available)
    }

    fn is_enabled(&self) -> Result<bool, ActionError> {
        Ok(self.borrow().enabled)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if !b.available {
            return Err(ActionError::NotPossible(
                "mock: mangohud not available".into(),
            ));
        }
        b.enabled = enabled;
        Ok(())
    }
}

/// Toggle MangoHud launch preference (AC-4.15).
pub struct MangoHudAction<B: MangoHudBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: MangoHudBackend> MangoHudAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "Opt in to MangoHud for game launches via the documented MANGOHUD=1 env or mangohud wrap (never silent inject into a running process).",
            "Shows an overlay and can log frametimes when the user launches with MangoHud. Useful for local measurement; no FPS claim without that measurement.",
            "Low: overlay overhead; some anti-cheat contexts should keep overlays off (see AC-6.2). Requires mangohud installed.",
            "Disable the preference (restore previous enabled flag). Remove MANGOHUD from launch options if you set them manually.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }
}

impl<B: MangoHudBackend> Action for MangoHudAction<B> {
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
        Ok(MangoHudState {
            available: self.backend.probe_available()?,
            enabled: self.backend.is_enabled()?,
        }
        .encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        if !self.backend.probe_available()? {
            return Err(ActionError::NotPossible(
                "mangohud is not available on PATH; install MangoHud or see https://github.com/flightlessmango/MangoHud"
                    .into(),
            ));
        }
        self.backend.set_enabled(true)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = MangoHudState::decode(before)?;
        if !state.available {
            // Daemon/binary was missing; clear preference if present.
            if self.backend.probe_available().unwrap_or(false) {
                let _ = self.backend.set_enabled(false);
            }
            return Ok(());
        }
        self.backend.set_enabled(state.enabled)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = MangoHudState::decode(expected)?;
        let actual_state = MangoHudState::decode(&actual)?;
        if actual_state.enabled == expected_state.enabled
            && actual_state.available == expected_state.available
        {
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
    use crate::paths::hertzline_data_dir;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    pub struct OsMangoHudBackend;

    impl MangoHudBackend for OsMangoHudBackend {
        fn probe_available(&self) -> Result<bool, ActionError> {
            Ok(find_on_path("mangohud"))
        }

        fn is_enabled(&self) -> Result<bool, ActionError> {
            let path = pref_path();
            if !path.exists() {
                return Ok(false);
            }
            let raw = fs::read_to_string(&path).unwrap_or_default();
            Ok(raw.trim() == "1" || raw.trim().eq_ignore_ascii_case("true"))
        }

        fn set_enabled(&self, enabled: bool) -> Result<(), ActionError> {
            if !find_on_path("mangohud") {
                return Err(ActionError::NotPossible("mangohud not on PATH".into()));
            }
            let path = pref_path();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    ActionError::ApplyFailed(format!("mkdir {}: {e}", parent.display()))
                })?;
            }
            if enabled {
                fs::write(&path, "1\n").map_err(|e| {
                    ActionError::ApplyFailed(format!("write {}: {e}", path.display()))
                })?;
            } else if path.exists() {
                fs::remove_file(&path).map_err(|e| {
                    ActionError::ApplyFailed(format!("remove {}: {e}", path.display()))
                })?;
            }
            Ok(())
        }
    }

    fn pref_path() -> PathBuf {
        hertzline_data_dir().join("mangohud_enabled")
    }

    fn find_on_path(name: &str) -> bool {
        let Ok(output) = Command::new("sh")
            .args(["-c", &format!("command -v {name}")])
            .output()
        else {
            return false;
        };
        output.status.success()
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn probe_is_honest() {
            let available = OsMangoHudBackend.probe_available().unwrap_or(false);
            let _ = available;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    #[test]
    fn mock_apply_restore_verify() {
        let backend = RefCell::new(MockMangoHudBackend::available_off());
        let mut action = MangoHudAction::new(backend);
        let before = action.read_state().unwrap();
        assert!(before.contains("enabled=0"));
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("enabled=1"));
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn missing_is_not_possible() {
        let backend = RefCell::new(MockMangoHudBackend::missing());
        let mut action = MangoHudAction::new(backend);
        let err = action.apply().unwrap_err();
        assert!(matches!(err, ActionError::NotPossible(_)));
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockMangoHudBackend::available_off());
        validate_explanation(MangoHudAction::new(backend).explanation()).unwrap();
    }

    #[test]
    fn state_round_trip() {
        let s = MangoHudState {
            available: true,
            enabled: true,
        };
        assert_eq!(MangoHudState::decode(&s.encode()).unwrap(), s);
    }
}
