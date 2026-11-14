//! Laptop battery / performance presets (AC-5.3).
//!
//! Maps named presets onto OS power APIs:
//! - Windows: power scheme GUIDs (AC-4.1)
//! - Linux: power-profiles-daemon / tuned profiles (AC-4.12)
//! - macOS: not possible via a public battery-preset API in v1 (honest)

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};
use crate::actions::linux::{PROFILE_BALANCED, PROFILE_PERFORMANCE, PROFILE_POWER_SAVER};
use crate::actions::windows::{GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_POWER_SAVER};

pub const ACTION_ID: &str = "everyday.power_preset";

pub const PRESET_BATTERY: &str = "battery_saver";
pub const PRESET_BALANCED: &str = "balanced";
pub const PRESET_PERFORMANCE: &str = "performance";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EverydayPreset {
    BatterySaver,
    Balanced,
    Performance,
}

impl EverydayPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BatterySaver => PRESET_BATTERY,
            Self::Balanced => PRESET_BALANCED,
            Self::Performance => PRESET_PERFORMANCE,
        }
    }

    pub fn parse(raw: &str) -> Result<Self, ActionError> {
        match raw.trim() {
            PRESET_BATTERY | "battery" | "power-saver" | "power_saver" => Ok(Self::BatterySaver),
            PRESET_BALANCED => Ok(Self::Balanced),
            PRESET_PERFORMANCE => Ok(Self::Performance),
            other => Err(ActionError::ApplyFailed(format!(
                "unknown everyday power preset: {other}"
            ))),
        }
    }

    pub fn windows_guid(self) -> &'static str {
        match self {
            Self::BatterySaver => GUID_POWER_SAVER,
            Self::Balanced => GUID_BALANCED,
            Self::Performance => GUID_HIGH_PERFORMANCE,
        }
    }

    pub fn linux_ppd_profile(self) -> &'static str {
        match self {
            Self::BatterySaver => PROFILE_POWER_SAVER,
            Self::Balanced => PROFILE_BALANCED,
            Self::Performance => PROFILE_PERFORMANCE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerPresetState {
    pub backend: String,
    pub preset: EverydayPreset,
    pub raw: String,
}

impl PowerPresetState {
    pub fn encode(&self) -> String {
        format!(
            "backend={};preset={};raw={}",
            self.backend,
            self.preset.as_str(),
            encode_field(&self.raw)
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut backend = None;
        let mut preset = None;
        let mut mapped = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "backend" => backend = Some(v.trim().to_string()),
                "preset" => preset = Some(EverydayPreset::parse(v)?),
                "raw" => mapped = Some(decode_field(v.trim())),
                _ => {}
            }
        }
        Ok(Self {
            backend: backend.ok_or_else(|| ActionError::RestoreFailed("missing backend".into()))?,
            preset: preset.ok_or_else(|| ActionError::RestoreFailed("missing preset".into()))?,
            raw: mapped.ok_or_else(|| ActionError::RestoreFailed("missing raw".into()))?,
        })
    }
}

fn encode_field(s: &str) -> String {
    s.replace(';', "%3B").replace('=', "%3D")
}

fn decode_field(s: &str) -> String {
    s.replace("%3B", ";").replace("%3D", "=")
}

/// Read / write the mapped OS power preset.
pub trait PowerPresetBackend {
    fn read(&self) -> Result<PowerPresetState, ActionError>;
    fn apply_preset(&self, preset: EverydayPreset) -> Result<(), ActionError>;
    fn restore_raw(&self, state: &PowerPresetState) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real OS power API).
#[derive(Debug, Clone)]
pub struct MockPowerPresetBackend {
    pub backend: String,
    pub preset: EverydayPreset,
    pub raw: String,
}

impl MockPowerPresetBackend {
    pub fn new(backend: impl Into<String>, preset: EverydayPreset, raw: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            preset,
            raw: raw.into(),
        }
    }
}

impl PowerPresetBackend for std::cell::RefCell<MockPowerPresetBackend> {
    fn read(&self) -> Result<PowerPresetState, ActionError> {
        let b = self.borrow();
        Ok(PowerPresetState {
            backend: b.backend.clone(),
            preset: b.preset,
            raw: b.raw.clone(),
        })
    }

    fn apply_preset(&self, preset: EverydayPreset) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        b.preset = preset;
        b.raw = format!("mock:{}", preset.as_str());
        Ok(())
    }

    fn restore_raw(&self, state: &PowerPresetState) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        b.backend = state.backend.clone();
        b.preset = state.preset;
        b.raw = state.raw.clone();
        Ok(())
    }
}

/// Apply / restore / verify a laptop power preset (AC-5.3).
pub struct PowerPresetAction<B: PowerPresetBackend> {
    backend: B,
    target: EverydayPreset,
    explanation: ActionExplanation,
}

impl<B: PowerPresetBackend> PowerPresetAction<B> {
    pub fn new(backend: B, target: EverydayPreset) -> Self {
        let mut explanation = ActionExplanation::new(
            "Switch the laptop battery/performance preset through the OS power API for this platform.",
            "Battery saver favors efficiency; performance favors responsiveness. Effect varies; measure locally.",
            "Higher performance increases heat and battery drain. Battery saver may limit clocks.",
            "Restore the previous preset recorded before apply, or use the OS power settings UI.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target,
            explanation,
        }
    }

    pub fn target(&self) -> EverydayPreset {
        self.target
    }
}

impl<B: PowerPresetBackend> Action for PowerPresetAction<B> {
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
        Ok(self.backend.read()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        self.backend.apply_preset(self.target)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = PowerPresetState::decode(before)?;
        self.backend.restore_raw(&state)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        if actual == expected {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

/// Host backend: Windows power schemes, Linux energy profiles, macOS not possible.
pub struct HostPowerPresetBackend;

impl PowerPresetBackend for HostPowerPresetBackend {
    fn read(&self) -> Result<PowerPresetState, ActionError> {
        host_read()
    }

    fn apply_preset(&self, preset: EverydayPreset) -> Result<(), ActionError> {
        host_apply(preset)
    }

    fn restore_raw(&self, state: &PowerPresetState) -> Result<(), ActionError> {
        host_restore(state)
    }
}

fn host_read() -> Result<PowerPresetState, ActionError> {
    #[cfg(windows)]
    {
        use crate::actions::windows::power_plan::os::OsPowerPlanBackend;
        use crate::actions::windows::PowerPlanBackend;
        let guid = OsPowerPlanBackend.get_active()?;
        let preset = match guid.as_str() {
            g if g == GUID_POWER_SAVER => EverydayPreset::BatterySaver,
            g if g == GUID_HIGH_PERFORMANCE => EverydayPreset::Performance,
            _ => EverydayPreset::Balanced,
        };
        Ok(PowerPresetState {
            backend: "windows_power_plan".into(),
            preset,
            raw: guid,
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::actions::linux::governor::os::OsEnergyProfileBackend;
        use crate::actions::linux::EnergyProfileBackend;
        let backend = OsEnergyProfileBackend;
        let name = backend.backend_name()?;
        let profile = backend.get_profile()?;
        let preset = EverydayPreset::parse(&profile).unwrap_or(EverydayPreset::Balanced);
        Ok(PowerPresetState {
            backend: format!("linux_{name}"),
            preset,
            raw: profile,
        })
    }
    #[cfg(target_os = "macos")]
    {
        Err(ActionError::NotPossible(
            "macOS has no public battery/performance preset API for Hertzline v1; use System Settings > Battery and Low Power Mode warnings (AC-4.21, AC-5.3)".into(),
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(ActionError::NotPossible(
            "power presets are not available on this OS".into(),
        ))
    }
}

fn host_apply(preset: EverydayPreset) -> Result<(), ActionError> {
    #[cfg(windows)]
    {
        use crate::actions::windows::power_plan::os::OsPowerPlanBackend;
        use crate::actions::windows::PowerPlanBackend;
        let backend = OsPowerPlanBackend;
        let guid = preset.windows_guid();
        if !backend.scheme_exists(guid)? {
            return Err(ActionError::NotPossible(format!(
                "power scheme {guid} is not installed"
            )));
        }
        backend.set_active(guid)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::actions::linux::governor::os::OsEnergyProfileBackend;
        use crate::actions::linux::EnergyProfileBackend;
        let backend = OsEnergyProfileBackend;
        let profile = preset.linux_ppd_profile();
        if !backend.profile_available(profile)? {
            return Err(ActionError::NotPossible(format!(
                "energy profile {profile} is not available"
            )));
        }
        backend.set_profile(profile)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = preset;
        Err(ActionError::NotPossible(
            "macOS everyday power presets are not possible via public APIs in v1".into(),
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = preset;
        Err(ActionError::NotPossible(
            "power presets are not available on this OS".into(),
        ))
    }
}

fn host_restore(state: &PowerPresetState) -> Result<(), ActionError> {
    #[cfg(windows)]
    {
        use crate::actions::windows::power_plan::os::OsPowerPlanBackend;
        use crate::actions::windows::PowerPlanBackend;
        OsPowerPlanBackend.set_active(&state.raw)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::actions::linux::governor::os::OsEnergyProfileBackend;
        use crate::actions::linux::EnergyProfileBackend;
        OsEnergyProfileBackend.set_profile(&state.raw)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = state;
        Err(ActionError::NotPossible(
            "macOS everyday power presets cannot be restored via public APIs in v1".into(),
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = state;
        Err(ActionError::NotPossible(
            "power presets are not available on this OS".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    #[test]
    fn labeled_mock_apply_restore_verify() {
        // labeled mock: not a real OS power API
        let backend = RefCell::new(MockPowerPresetBackend::new(
            "mock",
            EverydayPreset::Balanced,
            "mock:balanced",
        ));
        let mut action = PowerPresetAction::new(backend, EverydayPreset::Performance);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        assert!(entry.after.contains(PRESET_PERFORMANCE));
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        let after = PowerPresetState::decode(&action.read_state().unwrap()).unwrap();
        assert_eq!(after.preset, EverydayPreset::Balanced);
    }

    #[test]
    fn preset_parse_aliases() {
        assert_eq!(
            EverydayPreset::parse("power-saver").unwrap(),
            EverydayPreset::BatterySaver
        );
        assert!(EverydayPreset::parse("turbo").is_err());
    }

    #[test]
    fn host_read_is_honest_on_this_machine() {
        match HostPowerPresetBackend.read() {
            Ok(state) => {
                assert!(!state.backend.is_empty());
                assert!(!state.raw.is_empty());
            }
            Err(ActionError::NotPossible(msg)) => {
                assert!(msg.contains("macOS") || msg.contains("not available"));
            }
            Err(other) => panic!("unexpected host_read error: {other}"),
        }
    }
}
