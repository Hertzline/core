//! Hardware-accelerated GPU scheduling (HAGS) (AC-4.2).
//!
//! Registry (machine-wide, requires elevation to write):
//! HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\HwSchMode
//! DWORD: 1 = Off, 2 = On. Reboot required for the change to take effect.
//!
//! Sources:
//! - https://devblogs.microsoft.com/directx/hardware-accelerated-gpu-scheduling/
//! - Microsoft Q&A / Settings UI path: System > Display > Graphics > Change default graphics settings
//!
//! Apply only when the value already exists (OS/driver exposed HAGS). If missing,
//! report not_possible with guidance -- do not invent support.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.hags";
pub const REG_PATH: &str = r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers";
pub const REG_VALUE: &str = "HwSchMode";
pub const HAGS_OFF: &str = "1";
pub const HAGS_ON: &str = "2";
pub const SOURCE_URL: &str =
    "https://devblogs.microsoft.com/directx/hardware-accelerated-gpu-scheduling/";

/// Read / write HAGS mode. "unsupported" means HwSchMode is absent.
pub trait HagsBackend {
    fn get(&self) -> Result<String, ActionError>;
    /// Set mode to "1" (off) or "2" (on). Requires elevation on real OS.
    fn set(&self, mode: &str) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real HAGS setting).
#[derive(Debug, Clone)]
pub struct MockHagsBackend {
    pub mode: Option<String>,
    pub elevated: bool,
}

impl MockHagsBackend {
    pub fn supported(mode: &str) -> Self {
        Self {
            mode: Some(mode.to_string()),
            elevated: true,
        }
    }

    pub fn unsupported() -> Self {
        Self {
            mode: None,
            elevated: true,
        }
    }

    pub fn without_elevation(mode: &str) -> Self {
        Self {
            mode: Some(mode.to_string()),
            elevated: false,
        }
    }
}

impl HagsBackend for std::cell::RefCell<MockHagsBackend> {
    fn get(&self) -> Result<String, ActionError> {
        Ok(match &self.borrow().mode {
            Some(m) => m.clone(),
            None => "unsupported".into(),
        })
    }

    fn set(&self, mode: &str) -> Result<(), ActionError> {
        let mode = mode.trim();
        if mode != HAGS_OFF && mode != HAGS_ON {
            return Err(ActionError::ApplyFailed(format!(
                "invalid HAGS mode {mode}; expected {HAGS_OFF} or {HAGS_ON}"
            )));
        }
        let mut inner = self.borrow_mut();
        if !inner.elevated {
            return Err(ActionError::ApplyFailed(
                "mock: elevation required to write HwSchMode".into(),
            ));
        }
        if inner.mode.is_none() {
            return Err(ActionError::NotPossible(
                "HAGS is not exposed on this system (HwSchMode missing)".into(),
            ));
        }
        inner.mode = Some(mode.to_string());
        Ok(())
    }
}

/// Toggle HAGS when the OS exposes it (AC-4.2). Elevation required to write.
pub struct HagsAction<B: HagsBackend> {
    backend: B,
    target: String,
    explanation: ActionExplanation,
}

impl<B: HagsBackend> HagsAction<B> {
    pub fn new(backend: B, target_mode: &str) -> Self {
        let mut explanation = ActionExplanation::new(
            "Toggle Hardware-accelerated GPU scheduling (HAGS) via HwSchMode when Windows exposes it.",
            "May reduce scheduling latency on supported GPUs/drivers. A reboot is required before the new mode takes effect. Measure locally; no fixed FPS claim.",
            "Machine-wide change needs elevation. Unsupported GPUs/drivers show no toggle -- Hertzline will not force-enable. Wrong mode can cause instability on some systems; reboot to apply.",
            "Restore the previous HwSchMode value (1=Off, 2=On), then reboot. Or use Settings > System > Display > Graphics > Change default graphics settings.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target: target_mode.trim().to_string(),
            explanation,
        }
    }

    pub fn enable(backend: B) -> Self {
        Self::new(backend, HAGS_ON)
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

impl<B: HagsBackend> Action for HagsAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        true
    }

    fn read_state(&self) -> Result<String, ActionError> {
        self.backend.get()
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let current = self.backend.get()?;
        if current == "unsupported" {
            return Err(ActionError::NotPossible(
                "HAGS is not exposed (HwSchMode missing); GPU/driver may lack support".into(),
            ));
        }
        self.backend.set(&self.target)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let before = before.trim();
        if before == "unsupported" {
            return Err(ActionError::RestoreFailed(
                "cannot restore unsupported HAGS state".into(),
            ));
        }
        self.backend.set(before)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.backend.get()?;
        let expected = expected.trim().to_string();
        if actual == expected {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch { expected, actual })
        }
    }
}

/// Registry helpers shared with the privileged helper (same path/value).
#[cfg(windows)]
pub mod os {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const KEY_READ: u32 = 0x20019;
    const KEY_SET_VALUE: u32 = 0x0002;
    const KEY_WRITE: u32 = 0x20006;
    const REG_DWORD: u32 = 4;
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as i32 as isize;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: *mut c_void,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut *mut c_void,
        ) -> u32;
        fn RegQueryValueExW(
            hkey: *mut c_void,
            value_name: *const u16,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_size: *mut u32,
        ) -> u32;
        fn RegSetValueExW(
            hkey: *mut c_void,
            value_name: *const u16,
            reserved: u32,
            value_type: u32,
            data: *const u8,
            data_size: u32,
        ) -> u32;
        fn RegCloseKey(hkey: *mut c_void) -> u32;
    }

    pub struct OsHagsBackend;

    impl HagsBackend for OsHagsBackend {
        fn get(&self) -> Result<String, ActionError> {
            read_hw_sch_mode()
        }

        fn set(&self, mode: &str) -> Result<(), ActionError> {
            write_hw_sch_mode(mode)
        }
    }

    pub fn read_hw_sch_mode() -> Result<String, ActionError> {
        let path = wide(REG_PATH);
        let mut key = ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE as *mut c_void,
                path.as_ptr(),
                0,
                KEY_READ,
                &mut key,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok("unsupported".into());
        }
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegOpenKeyExW GraphicsDrivers failed ({status})"
            )));
        }
        let mut ty = 0u32;
        let mut data = [0u8; 4];
        let mut size = 4u32;
        let name = wide(REG_VALUE);
        let q = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                ptr::null_mut(),
                &mut ty,
                data.as_mut_ptr(),
                &mut size,
            )
        };
        unsafe { RegCloseKey(key) };
        if q == ERROR_FILE_NOT_FOUND {
            return Ok("unsupported".into());
        }
        if q != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegQueryValueExW HwSchMode failed ({q}); see {SOURCE_URL}"
            )));
        }
        if ty != REG_DWORD || size < 4 {
            return Err(ActionError::ApplyFailed(
                "HwSchMode is not a REG_DWORD".into(),
            ));
        }
        Ok(u32::from_le_bytes(data).to_string())
    }

    pub fn write_hw_sch_mode(mode: &str) -> Result<(), ActionError> {
        let mode = mode.trim();
        if mode != HAGS_OFF && mode != HAGS_ON {
            return Err(ActionError::ApplyFailed(format!(
                "invalid HAGS mode {mode}"
            )));
        }
        // Refuse to create HwSchMode if missing -- apply-only-where-OS-allows.
        let current = read_hw_sch_mode()?;
        if current == "unsupported" {
            return Err(ActionError::NotPossible(
                "HAGS is not exposed (HwSchMode missing)".into(),
            ));
        }
        let path = wide(REG_PATH);
        let mut key = ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE as *mut c_void,
                path.as_ptr(),
                0,
                KEY_WRITE | KEY_SET_VALUE,
                &mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegOpenKeyExW GraphicsDrivers write failed ({status}); elevation required"
            )));
        }
        let value: u32 = mode.parse().unwrap_or(0);
        let bytes = value.to_le_bytes();
        let name = wide(REG_VALUE);
        let set = unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4) };
        unsafe { RegCloseKey(key) };
        if set != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegSetValueExW HwSchMode failed ({set})"
            )));
        }
        Ok(())
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_hags_readable() {
            let state = read_hw_sch_mode().expect("read HAGS");
            assert!(
                state == "unsupported"
                    || state == HAGS_OFF
                    || state == HAGS_ON
                    || state.parse::<u32>().is_ok(),
                "unexpected HAGS state {state}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log, validate_explanation};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    #[test]
    fn mock_apply_restore_verify_hags() {
        // mock: elevated in-memory HwSchMode
        let backend = RefCell::new(MockHagsBackend::supported(HAGS_OFF));
        let mut action = HagsAction::enable(backend);
        assert!(action.requires_elevation());
        assert_eq!(action.read_state().unwrap(), HAGS_OFF);
        action.apply().unwrap();
        assert_eq!(action.read_state().unwrap(), HAGS_ON);
        action.restore(HAGS_OFF).unwrap();
        assert!(action.verify(HAGS_OFF).unwrap().is_match());
    }

    #[test]
    fn unsupported_is_not_possible() {
        let backend = RefCell::new(MockHagsBackend::unsupported());
        let mut action = HagsAction::enable(backend);
        assert_eq!(action.read_state().unwrap(), "unsupported");
        assert!(matches!(
            action.apply().unwrap_err(),
            ActionError::NotPossible(_)
        ));
    }

    #[test]
    fn without_elevation_apply_fails() {
        let backend = RefCell::new(MockHagsBackend::without_elevation(HAGS_OFF));
        let mut action = HagsAction::enable(backend);
        assert!(matches!(
            action.apply().unwrap_err(),
            ActionError::ApplyFailed(_)
        ));
    }

    #[test]
    fn snapshot_round_trip() {
        let backend = RefCell::new(MockHagsBackend::supported(HAGS_OFF));
        let mut action = HagsAction::enable(backend);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 22).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        assert_eq!(entry.before, HAGS_OFF);
        assert_eq!(entry.after, HAGS_ON);
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert_eq!(action.read_state().unwrap(), HAGS_OFF);
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockHagsBackend::supported(HAGS_ON));
        let action = HagsAction::enable(backend);
        validate_explanation(action.explanation()).unwrap();
        assert_eq!(
            action.explanation().local_measurement.message(),
            LocalMeasurementNote::NONE_YET_MESSAGE
        );
    }
}
