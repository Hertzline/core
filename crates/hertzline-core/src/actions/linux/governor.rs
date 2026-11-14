//! CPU energy profile / governor via power-profiles-daemon or tuned (AC-4.12).
//!
//! Primary: org.freedesktop.UPower.PowerProfiles ActiveProfile
//!   power-saver | balanced | performance
//!   https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html
//! Fallback: tuned-adm active / tuned-adm profile <name>
//! If neither is available: NotPossible (never invent a sysfs write).

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.energy_profile";
pub const SOURCE_URL_PPD: &str =
    "https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html";
pub const SOURCE_URL_TUNED: &str = "https://tuned-project.org/";

pub const PROFILE_POWER_SAVER: &str = "power-saver";
pub const PROFILE_BALANCED: &str = "balanced";
pub const PROFILE_PERFORMANCE: &str = "performance";

/// Encoded: `backend=ppd|tuned|unavailable;profile=<name>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnergyProfileState {
    pub backend: String,
    pub profile: String,
}

impl EnergyProfileState {
    pub fn encode(&self) -> String {
        format!("backend={};profile={}", self.backend, self.profile)
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut backend = None;
        let mut profile = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "backend" => backend = Some(v.trim().to_string()),
                "profile" => profile = Some(v.trim().to_string()),
                _ => {}
            }
        }
        Ok(Self {
            backend: backend.ok_or_else(|| ActionError::RestoreFailed("missing backend".into()))?,
            profile: profile.ok_or_else(|| ActionError::RestoreFailed("missing profile".into()))?,
        })
    }
}

pub fn validate_ppd_profile(profile: &str) -> Result<(), ActionError> {
    match profile {
        PROFILE_POWER_SAVER | PROFILE_BALANCED | PROFILE_PERFORMANCE => Ok(()),
        other => Err(ActionError::ApplyFailed(format!(
            "unsupported power-profiles-daemon profile: {other}"
        ))),
    }
}

/// Read / write energy profile through the preferred available backend.
pub trait EnergyProfileBackend {
    /// Which backend is live: "ppd", "tuned", or error NotPossible.
    fn backend_name(&self) -> Result<&'static str, ActionError>;
    fn get_profile(&self) -> Result<String, ActionError>;
    fn set_profile(&self, profile: &str) -> Result<(), ActionError>;
    fn profile_available(&self, profile: &str) -> Result<bool, ActionError>;
}

/// Labeled mock for unit tests (not a real energy daemon).
#[derive(Debug, Clone)]
pub struct MockEnergyProfileBackend {
    pub backend: &'static str,
    pub active: String,
    pub profiles: Vec<String>,
}

impl MockEnergyProfileBackend {
    pub fn ppd(active: &str) -> Self {
        Self {
            backend: "ppd",
            active: active.into(),
            profiles: vec![
                PROFILE_POWER_SAVER.into(),
                PROFILE_BALANCED.into(),
                PROFILE_PERFORMANCE.into(),
            ],
        }
    }

    pub fn tuned(active: &str) -> Self {
        Self {
            backend: "tuned",
            active: active.into(),
            profiles: vec![
                "balanced".into(),
                "throughput-performance".into(),
                "latency-performance".into(),
            ],
        }
    }

    pub fn unavailable() -> Self {
        Self {
            backend: "unavailable",
            active: String::new(),
            profiles: Vec::new(),
        }
    }
}

impl EnergyProfileBackend for std::cell::RefCell<MockEnergyProfileBackend> {
    fn backend_name(&self) -> Result<&'static str, ActionError> {
        let name = self.borrow().backend;
        if name == "unavailable" {
            return Err(ActionError::NotPossible(
                "mock: no power-profiles-daemon or tuned".into(),
            ));
        }
        Ok(name)
    }

    fn get_profile(&self) -> Result<String, ActionError> {
        let _ = self.backend_name()?;
        Ok(self.borrow().active.clone())
    }

    fn set_profile(&self, profile: &str) -> Result<(), ActionError> {
        let _ = self.backend_name()?;
        let mut inner = self.borrow_mut();
        if !inner.profiles.iter().any(|p| p == profile) {
            return Err(ActionError::ApplyFailed(format!(
                "mock: profile {profile} not present"
            )));
        }
        inner.active = profile.into();
        Ok(())
    }

    fn profile_available(&self, profile: &str) -> Result<bool, ActionError> {
        let _ = self.backend_name()?;
        Ok(self.borrow().profiles.iter().any(|p| p == profile))
    }
}

/// Switch CPU energy profile (AC-4.12).
pub struct EnergyProfileAction<B: EnergyProfileBackend> {
    backend: B,
    target: String,
    explanation: ActionExplanation,
}

impl<B: EnergyProfileBackend> EnergyProfileAction<B> {
    pub fn new(backend: B, target: &str) -> Self {
        let mut explanation = ActionExplanation::new(
            "Switch the system energy / performance profile via power-profiles-daemon (ActiveProfile) or tuned-adm when ppd is absent.",
            "A performance-oriented profile can keep the CPU less power-constrained while gaming. Effect varies by machine and firmware; measure locally.",
            "Higher power use and heat on laptops; battery drains faster. Some systems lack a performance profile.",
            "Restore the previous backend+profile snapshot, or use the desktop power settings / powerprofilesctl / tuned-adm.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target: target.trim().to_string(),
            explanation,
        }
    }

    /// Default v1 target for ppd: performance (when present).
    pub fn performance(backend: B) -> Self {
        Self::new(backend, PROFILE_PERFORMANCE)
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

impl<B: EnergyProfileBackend> Action for EnergyProfileAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        // ppd ActiveProfile is typically unprivileged; tuned-adm may need auth
        // depending on distro polkit. Helper may elevate for tuned writes.
        false
    }

    fn read_state(&self) -> Result<String, ActionError> {
        match self.backend.backend_name() {
            Ok(name) => Ok(EnergyProfileState {
                backend: name.into(),
                profile: self.backend.get_profile()?,
            }
            .encode()),
            Err(ActionError::NotPossible(_)) => Ok(EnergyProfileState {
                backend: "unavailable".into(),
                profile: "unavailable".into(),
            }
            .encode()),
            Err(err) => Err(err),
        }
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let name = self.backend.backend_name()?;
        if name == "ppd" {
            validate_ppd_profile(&self.target)?;
        }
        if !self.backend.profile_available(&self.target)? {
            return Err(ActionError::NotPossible(format!(
                "energy profile '{}' is not available via {name}",
                self.target
            )));
        }
        self.backend.set_profile(&self.target)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = EnergyProfileState::decode(before)?;
        if state.backend == "unavailable" {
            return Ok(());
        }
        let live = self.backend.backend_name()?;
        if live != state.backend {
            return Err(ActionError::RestoreFailed(format!(
                "backend changed from {} to {live}; cannot restore safely",
                state.backend
            )));
        }
        if !self.backend.profile_available(&state.profile)? {
            return Err(ActionError::RestoreFailed(format!(
                "previous profile {} is no longer present",
                state.profile
            )));
        }
        self.backend.set_profile(&state.profile)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = EnergyProfileState::decode(expected)?;
        let actual_state = EnergyProfileState::decode(&actual)?;
        if actual_state == expected_state {
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

    /// Prefer powerprofilesctl; fall back to tuned-adm.
    pub struct OsEnergyProfileBackend;

    impl EnergyProfileBackend for OsEnergyProfileBackend {
        fn backend_name(&self) -> Result<&'static str, ActionError> {
            if powerprofilesctl_present() {
                return Ok("ppd");
            }
            if tuned_adm_present() {
                return Ok("tuned");
            }
            Err(ActionError::NotPossible(
                "neither power-profiles-daemon (powerprofilesctl) nor tuned-adm is available"
                    .into(),
            ))
        }

        fn get_profile(&self) -> Result<String, ActionError> {
            match self.backend_name()? {
                "ppd" => {
                    let out = run_capture(&["powerprofilesctl", "get"])?;
                    Ok(out.trim().to_string())
                }
                "tuned" => {
                    let out = run_capture(&["tuned-adm", "active"])?;
                    // Typical: "Current active profile: balanced"
                    let trimmed = out.trim();
                    if let Some(rest) = trimmed.strip_prefix("Current active profile:") {
                        Ok(rest.trim().to_string())
                    } else {
                        Ok(trimmed.to_string())
                    }
                }
                other => Err(ActionError::NotPossible(format!("unknown backend {other}"))),
            }
        }

        fn set_profile(&self, profile: &str) -> Result<(), ActionError> {
            match self.backend_name()? {
                "ppd" => {
                    validate_ppd_profile(profile)?;
                    run_status(&["powerprofilesctl", "set", profile])
                }
                "tuned" => run_status(&["tuned-adm", "profile", profile]),
                other => Err(ActionError::NotPossible(format!("unknown backend {other}"))),
            }
        }

        fn profile_available(&self, profile: &str) -> Result<bool, ActionError> {
            match self.backend_name()? {
                "ppd" => {
                    let out = run_capture(&["powerprofilesctl", "list"])?;
                    // Lines look like: `* performance:` or `  balanced:`
                    Ok(out.lines().any(|line| {
                        let t = line.trim().trim_start_matches('*').trim();
                        t.starts_with(profile)
                            && (t.len() == profile.len()
                                || t.as_bytes().get(profile.len()) == Some(&b':'))
                    }))
                }
                "tuned" => {
                    let out = run_capture(&["tuned-adm", "list"])?;
                    Ok(out.lines().any(|line| {
                        let t = line.trim().trim_start_matches('-').trim();
                        t == profile || t.starts_with(&format!("{profile} "))
                    }))
                }
                other => Err(ActionError::NotPossible(format!("unknown backend {other}"))),
            }
        }
    }

    fn powerprofilesctl_present() -> bool {
        Command::new("powerprofilesctl")
            .arg("get")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn tuned_adm_present() -> bool {
        Command::new("tuned-adm")
            .arg("active")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn run_capture(argv: &[&str]) -> Result<String, ActionError> {
        let (bin, args) = argv.split_first().expect("argv non-empty");
        let output = Command::new(bin)
            .args(args)
            .output()
            .map_err(|e| ActionError::ApplyFailed(format!("{bin} spawn failed: {e}")))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(ActionError::ApplyFailed(format!("{bin} failed: {err}")));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn run_status(argv: &[&str]) -> Result<(), ActionError> {
        let _ = run_capture(argv)?;
        Ok(())
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn backend_probe_is_honest() {
            match OsEnergyProfileBackend.backend_name() {
                Ok(name) => assert!(name == "ppd" || name == "tuned"),
                Err(ActionError::NotPossible(_)) => {}
                Err(err) => panic!("unexpected: {err}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use std::cell::RefCell;

    #[test]
    fn mock_ppd_apply_restore_verify() {
        let backend = RefCell::new(MockEnergyProfileBackend::ppd(PROFILE_BALANCED));
        let mut action = EnergyProfileAction::performance(backend);
        let before = action.read_state().unwrap();
        assert!(before.contains("backend=ppd"));
        assert!(before.contains("profile=balanced"));
        action.apply().unwrap();
        let after = action.read_state().unwrap();
        assert!(after.contains("profile=performance"));
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn mock_tuned_apply_restore() {
        let backend = RefCell::new(MockEnergyProfileBackend::tuned("balanced"));
        let mut action = EnergyProfileAction::new(backend, "throughput-performance");
        let before = action.read_state().unwrap();
        action.apply().unwrap();
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn mock_unavailable() {
        let backend = RefCell::new(MockEnergyProfileBackend::unavailable());
        let mut action = EnergyProfileAction::performance(backend);
        let state = action.read_state().unwrap();
        assert!(state.contains("unavailable"));
        assert!(matches!(
            action.apply().unwrap_err(),
            ActionError::NotPossible(_)
        ));
    }

    #[test]
    fn reject_bad_ppd_profile() {
        assert!(validate_ppd_profile("turbo-boost").is_err());
    }

    #[test]
    fn explanation_no_fake_fps() {
        let backend = RefCell::new(MockEnergyProfileBackend::ppd(PROFILE_BALANCED));
        let action = EnergyProfileAction::performance(backend);
        let e = action.explanation();
        assert!(!e.what.is_empty());
        assert!(!e.how_to_undo.is_empty());
        assert!(
            !e.expected_effect.to_lowercase().contains("+")
                || !e.expected_effect.to_lowercase().contains("fps")
        );
    }
}
