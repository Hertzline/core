//! Windows Game Mode toggle (AC-4.2).
//!
//! Official setting reference (Microsoft Learn):
//! HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled REG_DWORD 0|1
//! https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11
//!
//! Current-user registry; no elevation.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.game_mode";
pub const REG_PATH: &str = r"Software\Microsoft\GameBar";
pub const REG_VALUE: &str = "AutoGameModeEnabled";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11";

/// Read / write Game Mode (0 off, 1 on). Missing key reports "missing".
pub trait GameModeBackend {
    fn get(&self) -> Result<String, ActionError>;
    fn set(&self, enabled: bool) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real Game Mode setting).
#[derive(Debug, Clone)]
pub struct MockGameModeBackend {
    /// None means the registry value is absent.
    pub value: Option<u32>,
}

impl MockGameModeBackend {
    pub fn new(enabled: bool) -> Self {
        Self {
            value: Some(if enabled { 1 } else { 0 }),
        }
    }

    pub fn missing() -> Self {
        Self { value: None }
    }
}

impl GameModeBackend for std::cell::RefCell<MockGameModeBackend> {
    fn get(&self) -> Result<String, ActionError> {
        Ok(match self.borrow().value {
            Some(0) => "0".into(),
            Some(1) => "1".into(),
            Some(other) => other.to_string(),
            None => "missing".into(),
        })
    }

    fn set(&self, enabled: bool) -> Result<(), ActionError> {
        self.borrow_mut().value = Some(if enabled { 1 } else { 0 });
        Ok(())
    }
}

/// Enable or restore Windows Game Mode (AC-4.2).
pub struct GameModeAction<B: GameModeBackend> {
    backend: B,
    target_enabled: bool,
    explanation: ActionExplanation,
}

impl<B: GameModeBackend> GameModeAction<B> {
    pub fn new(backend: B, target_enabled: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Toggle Windows Game Mode (Settings > Gaming > Game Mode) via AutoGameModeEnabled.",
            "When on, Windows prioritizes the foreground game and can defer some background work. Effect is machine-specific; measure locally.",
            "Low: user-scoped setting. Some titles behave differently with Game Mode on or off.",
            "Restore the previous AutoGameModeEnabled value, or use Settings > Gaming > Game Mode.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target_enabled,
            explanation,
        }
    }

    pub fn enable(backend: B) -> Self {
        Self::new(backend, true)
    }
}

impl<B: GameModeBackend> Action for GameModeAction<B> {
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
        self.backend.get()
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        self.backend.set(self.target_enabled)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        match before.trim() {
            "0" => self.backend.set(false),
            "1" => self.backend.set(true),
            "missing" => {
                // Recreate off as the closest reversible state when the value was absent.
                self.backend.set(false)
            }
            other => Err(ActionError::RestoreFailed(format!(
                "invalid Game Mode snapshot state: {other}"
            ))),
        }
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.backend.get()?;
        let expected = expected.trim().to_string();
        // After apply/restore of missing->0, verify against what we wrote.
        if actual == expected {
            Ok(VerifyResult::Matches)
        } else if expected == "missing" && actual == "0" {
            // Honest: restore of missing writes 0; callers verifying restore of "missing"
            // should pass "0" after restore. Mismatch if they pass "missing".
            Ok(VerifyResult::Mismatch { expected, actual })
        } else {
            Ok(VerifyResult::Mismatch { expected, actual })
        }
    }
}

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

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: *mut c_void,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut *mut c_void,
        ) -> u32;
        fn RegCreateKeyExW(
            hkey: *mut c_void,
            sub_key: *const u16,
            reserved: u32,
            class: *const u16,
            options: u32,
            sam_desired: u32,
            security: *const c_void,
            result: *mut *mut c_void,
            disposition: *mut u32,
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

    // HKEY_CURRENT_USER
    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;

    pub struct OsGameModeBackend;

    impl GameModeBackend for OsGameModeBackend {
        fn get(&self) -> Result<String, ActionError> {
            let Some(key) = open_key(KEY_READ)? else {
                return Ok("missing".into());
            };
            let mut ty = 0u32;
            let mut data = [0u8; 4];
            let mut size = 4u32;
            let name = wide(REG_VALUE);
            let status = unsafe {
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
            if status == ERROR_FILE_NOT_FOUND {
                return Ok("missing".into());
            }
            if status != ERROR_SUCCESS {
                return Err(ActionError::ApplyFailed(format!(
                    "RegQueryValueExW AutoGameModeEnabled failed ({status}); see {SOURCE_URL}"
                )));
            }
            if ty != REG_DWORD || size < 4 {
                return Err(ActionError::ApplyFailed(
                    "AutoGameModeEnabled is not a REG_DWORD".into(),
                ));
            }
            let value = u32::from_le_bytes(data);
            Ok(value.to_string())
        }

        fn set(&self, enabled: bool) -> Result<(), ActionError> {
            let key = create_key(KEY_WRITE | KEY_SET_VALUE)?;
            let name = wide(REG_VALUE);
            let value: u32 = if enabled { 1 } else { 0 };
            let bytes = value.to_le_bytes();
            let status =
                unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4) };
            unsafe { RegCloseKey(key) };
            if status != ERROR_SUCCESS {
                return Err(ActionError::ApplyFailed(format!(
                    "RegSetValueExW AutoGameModeEnabled failed ({status}); see {SOURCE_URL}"
                )));
            }
            Ok(())
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn open_key(access: u32) -> Result<Option<*mut c_void>, ActionError> {
        let path = wide(REG_PATH);
        let mut key = ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER as *mut c_void,
                path.as_ptr(),
                0,
                access,
                &mut key,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegOpenKeyExW GameBar failed ({status})"
            )));
        }
        Ok(Some(key))
    }

    fn create_key(access: u32) -> Result<*mut c_void, ActionError> {
        let path = wide(REG_PATH);
        let mut key = ptr::null_mut();
        let mut disposition = 0u32;
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER as *mut c_void,
                path.as_ptr(),
                0,
                ptr::null(),
                0,
                access,
                ptr::null(),
                &mut key,
                &mut disposition,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegCreateKeyExW GameBar failed ({status})"
            )));
        }
        Ok(key)
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_game_mode_readable() {
            let backend = OsGameModeBackend;
            let state = backend.get().expect("read Game Mode");
            assert!(
                state == "0" || state == "1" || state == "missing" || state.parse::<u32>().is_ok(),
                "unexpected state {state}"
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
    fn mock_apply_restore_verify_game_mode() {
        // mock: in-memory DWORD only
        let backend = RefCell::new(MockGameModeBackend::new(false));
        let mut action = GameModeAction::enable(backend);
        assert!(!action.requires_elevation());
        assert_eq!(action.read_state().unwrap(), "0");
        action.apply().unwrap();
        assert_eq!(action.read_state().unwrap(), "1");
        assert!(action.verify("1").unwrap().is_match());
        action.restore("0").unwrap();
        assert!(action.verify("0").unwrap().is_match());
    }

    #[test]
    fn snapshot_round_trip() {
        let backend = RefCell::new(MockGameModeBackend::new(false));
        let mut action = GameModeAction::enable(backend);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 11).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        assert_eq!(entry.before, "0");
        assert_eq!(entry.after, "1");
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert_eq!(action.read_state().unwrap(), "0");
    }

    #[test]
    fn explanation_ac81_and_ac82() {
        let backend = RefCell::new(MockGameModeBackend::new(true));
        let action = GameModeAction::enable(backend);
        validate_explanation(action.explanation()).unwrap();
        assert!(action
            .explanation()
            .local_measurement
            .message()
            .contains("no local measurement"));
    }
}
