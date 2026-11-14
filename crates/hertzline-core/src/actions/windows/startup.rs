//! Startup apps review with reversible enable/disable (AC-4.5).
//!
//! Official sources (verified 2026-11-14):
//! - Desktop Startup apps (Run keys + Startup folders; Task Manager disable)
//!   https://learn.microsoft.com/en-us/windows/compatibility/startup-apps
//! - Run key (current user): HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//! - Enable/disable state used by Task Manager / Settings (Windows 8+):
//!   HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run
//!   First DWORD even (commonly 0x02/0x06) = enabled; odd (commonly 0x03) = disabled.
//!
//! Scope: current-user Run entries only (no elevation). HKLM Run is listed as
//! guidance-only / not mutated without a future elevated op.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.startup_apps";
pub const SOURCE_URL: &str = "https://learn.microsoft.com/en-us/windows/compatibility/startup-apps";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

impl StartupEntry {
    pub fn encode(&self) -> String {
        format!(
            "name={};enabled={};cmd={}",
            encode_field(&self.name),
            if self.enabled { "1" } else { "0" },
            encode_field(&self.command)
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut name = None;
        let mut enabled = None;
        let mut cmd = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "name" => name = Some(decode_field(v.trim())),
                "enabled" => {
                    enabled = Some(match v.trim() {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        other => {
                            return Err(ActionError::RestoreFailed(format!(
                                "invalid enabled flag: {other}"
                            )))
                        }
                    });
                }
                "cmd" => cmd = Some(decode_field(v.trim())),
                _ => {}
            }
        }
        Ok(Self {
            name: name.ok_or_else(|| ActionError::RestoreFailed("missing name".into()))?,
            command: cmd.unwrap_or_default(),
            enabled: enabled.ok_or_else(|| ActionError::RestoreFailed("missing enabled".into()))?,
        })
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/").replace(';', "%3B").replace('=', "%3D")
}

fn decode_field(s: &str) -> String {
    s.replace("%3D", "=").replace("%3B", ";").replace('/', "\\")
}

/// List and toggle current-user startup entries (user-chosen name only).
pub trait StartupAppsBackend {
    fn list(&self) -> Result<Vec<StartupEntry>, ActionError>;
    fn get(&self, name: &str) -> Result<StartupEntry, ActionError>;
    fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real registry write).
#[derive(Debug, Clone)]
pub struct MockStartupAppsBackend {
    pub entries: Vec<StartupEntry>,
}

impl MockStartupAppsBackend {
    pub fn with_entries(entries: Vec<StartupEntry>) -> Self {
        Self { entries }
    }
}

impl StartupAppsBackend for std::cell::RefCell<MockStartupAppsBackend> {
    fn list(&self) -> Result<Vec<StartupEntry>, ActionError> {
        Ok(self.borrow().entries.clone())
    }

    fn get(&self, name: &str) -> Result<StartupEntry, ActionError> {
        self.borrow()
            .entries
            .iter()
            .find(|e| e.name == name)
            .cloned()
            .ok_or_else(|| ActionError::ApplyFailed(format!("startup entry not found: {name}")))
    }

    fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        let entry = b
            .entries
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or_else(|| ActionError::ApplyFailed(format!("startup entry not found: {name}")))?;
        entry.enabled = enabled;
        Ok(())
    }
}

/// Disable or re-enable one user-chosen HKCU Run startup entry (AC-4.5).
pub struct StartupAppsAction<B: StartupAppsBackend> {
    backend: B,
    name: String,
    target_enabled: bool,
    explanation: ActionExplanation,
}

impl<B: StartupAppsBackend> StartupAppsAction<B> {
    pub fn new(backend: B, name: impl Into<String>, target_enabled: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Enable or disable a current-user startup app the user explicitly chose (HKCU Run + StartupApproved).",
            "Disabling reduces logon work from that entry. Effect on game sessions is indirect; measure locally if relevant.",
            "Low: user-scoped. Disabling may delay tray apps until launched manually. Does not delete the Run command.",
            "Restore the previous StartupApproved enabled/disabled state for that named entry.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            name: name.into(),
            target_enabled,
            explanation,
        }
    }

    pub fn disable(backend: B, name: impl Into<String>) -> Self {
        Self::new(backend, name, false)
    }

    pub fn enable(backend: B, name: impl Into<String>) -> Self {
        Self::new(backend, name, true)
    }

    pub fn entry_name(&self) -> &str {
        &self.name
    }

    pub fn list_entries(&self) -> Result<Vec<StartupEntry>, ActionError> {
        self.backend.list()
    }
}

impl<B: StartupAppsBackend> Action for StartupAppsAction<B> {
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
        Ok(self.backend.get(&self.name)?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        // Gate: name must exist in the listed user-chosen catalog.
        let _ = self.backend.get(&self.name)?;
        self.backend.set_enabled(&self.name, self.target_enabled)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let entry = StartupEntry::decode(before)?;
        // Adopt snapshot name for headless restore-all.
        self.name = entry.name;
        self.backend.set_enabled(&self.name, entry.enabled)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_entry = StartupEntry::decode(expected)?;
        let actual_entry = StartupEntry::decode(&actual)?;
        if actual_entry.name == expected_entry.name
            && actual_entry.enabled == expected_entry.enabled
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

/// Decode StartupApproved first DWORD: odd => disabled, even/missing => enabled.
pub fn approved_enabled(first_dword: Option<u32>) -> bool {
    match first_dword {
        None => true,
        Some(v) => v % 2 == 0,
    }
}

pub fn approved_blob(enabled: bool) -> Vec<u8> {
    let first: u32 = if enabled { 0x02 } else { 0x03 };
    let mut out = first.to_le_bytes().to_vec();
    out.extend_from_slice(&[0u8; 8]);
    out
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
    const REG_SZ: u32 = 1;
    const REG_BINARY: u32 = 3;
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const ERROR_NO_MORE_ITEMS: u32 = 259;
    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;

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
        fn RegEnumValueW(
            hkey: *mut c_void,
            index: u32,
            value_name: *mut u16,
            value_name_len: *mut u32,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_size: *mut u32,
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

    pub struct OsStartupAppsBackend;

    impl StartupAppsBackend for OsStartupAppsBackend {
        fn list(&self) -> Result<Vec<StartupEntry>, ActionError> {
            let Some(run) = open_key(RUN_KEY, KEY_READ)? else {
                return Ok(Vec::new());
            };
            let approved = open_key(APPROVED_KEY, KEY_READ)?;
            let mut entries = Vec::new();
            let mut index = 0u32;
            loop {
                let mut name_buf = vec![0u16; 256];
                let mut name_len = name_buf.len() as u32;
                let mut ty = 0u32;
                let mut data = vec![0u8; 32768];
                let mut data_size = data.len() as u32;
                let status = unsafe {
                    RegEnumValueW(
                        run,
                        index,
                        name_buf.as_mut_ptr(),
                        &mut name_len,
                        ptr::null_mut(),
                        &mut ty,
                        data.as_mut_ptr(),
                        &mut data_size,
                    )
                };
                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status != ERROR_SUCCESS {
                    unsafe { RegCloseKey(run) };
                    if let Some(a) = approved {
                        unsafe { RegCloseKey(a) };
                    }
                    return Err(ActionError::ApplyFailed(format!(
                        "RegEnumValueW Run failed ({status})"
                    )));
                }
                if ty == REG_SZ {
                    let name = utf16_to_string(&name_buf[..name_len as usize]);
                    let cmd = utf16_bytes_to_string(&data[..data_size as usize]);
                    let enabled = match approved {
                        Some(a) => approved_enabled(read_approved_dword(a, &name)?),
                        None => true,
                    };
                    entries.push(StartupEntry {
                        name,
                        command: cmd,
                        enabled,
                    });
                }
                index += 1;
            }
            unsafe { RegCloseKey(run) };
            if let Some(a) = approved {
                unsafe { RegCloseKey(a) };
            }
            Ok(entries)
        }

        fn get(&self, name: &str) -> Result<StartupEntry, ActionError> {
            self.list()?
                .into_iter()
                .find(|e| e.name == name)
                .ok_or_else(|| ActionError::ApplyFailed(format!("startup entry not found: {name}")))
        }

        fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), ActionError> {
            // Ensure the Run entry exists (user-chosen).
            let _ = self.get(name)?;
            let key = create_key(APPROVED_KEY, KEY_WRITE | KEY_SET_VALUE)?;
            let blob = approved_blob(enabled);
            let wide_name = wide(name);
            let status = unsafe {
                RegSetValueExW(
                    key,
                    wide_name.as_ptr(),
                    0,
                    REG_BINARY,
                    blob.as_ptr(),
                    blob.len() as u32,
                )
            };
            unsafe { RegCloseKey(key) };
            if status != ERROR_SUCCESS {
                return Err(ActionError::ApplyFailed(format!(
                    "RegSetValueExW StartupApproved failed ({status}); see {SOURCE_URL}"
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

    fn utf16_to_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    fn utf16_bytes_to_string(bytes: &[u8]) -> String {
        if bytes.len() < 2 {
            return String::new();
        }
        let u16s: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        utf16_to_string(&u16s)
    }

    fn open_key(path: &str, access: u32) -> Result<Option<*mut c_void>, ActionError> {
        let path_w = wide(path);
        let mut key = ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER as *mut c_void,
                path_w.as_ptr(),
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
                "RegOpenKeyExW {path} failed ({status})"
            )));
        }
        Ok(Some(key))
    }

    fn create_key(path: &str, access: u32) -> Result<*mut c_void, ActionError> {
        let path_w = wide(path);
        let mut key = ptr::null_mut();
        let mut disposition = 0u32;
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER as *mut c_void,
                path_w.as_ptr(),
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
                "RegCreateKeyExW {path} failed ({status})"
            )));
        }
        Ok(key)
    }

    fn read_approved_dword(key: *mut c_void, name: &str) -> Result<Option<u32>, ActionError> {
        let wide_name = wide(name);
        let mut ty = 0u32;
        let mut data = [0u8; 16];
        let mut size = data.len() as u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                wide_name.as_ptr(),
                ptr::null_mut(),
                &mut ty,
                data.as_mut_ptr(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegQueryValueExW StartupApproved failed ({status})"
            )));
        }
        if ty != REG_BINARY || size < 4 {
            return Ok(None);
        }
        Ok(Some(u32::from_le_bytes([
            data[0], data[1], data[2], data[3],
        ])))
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_lists_without_panic() {
            let list = OsStartupAppsBackend.list().expect("list startup");
            // Machine-specific; just ensure the call is honest.
            for entry in &list {
                assert!(!entry.name.is_empty());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log, validate_explanation};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    fn mock_backend() -> RefCell<MockStartupAppsBackend> {
        RefCell::new(MockStartupAppsBackend::with_entries(vec![
            StartupEntry {
                name: "Chat".into(),
                command: r"C:\Apps\chat.exe".into(),
                enabled: true,
            },
            StartupEntry {
                name: "Updater".into(),
                command: r"C:\Apps\upd.exe".into(),
                enabled: true,
            },
        ]))
    }

    #[test]
    fn mock_disable_restore() {
        // mock: reversible StartupApproved-style toggle
        let backend = mock_backend();
        let mut action = StartupAppsAction::disable(backend, "Chat");
        assert_eq!(action.entry_name(), "Chat");
        assert_eq!(action.list_entries().unwrap().len(), 2);
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("enabled=0"));
        action
            .restore(
                &StartupEntry {
                    name: "Chat".into(),
                    command: r"C:\Apps\chat.exe".into(),
                    enabled: true,
                }
                .encode(),
            )
            .unwrap();
        assert!(action.read_state().unwrap().contains("enabled=1"));
    }

    #[test]
    fn refuses_unknown_name() {
        let backend = mock_backend();
        let mut action = StartupAppsAction::disable(backend, "Missing");
        assert!(action
            .apply()
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    #[test]
    fn snapshot_round_trip() {
        let backend = mock_backend();
        let mut action = StartupAppsAction::disable(backend, "Updater");
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 3).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert!(action.read_state().unwrap().contains("enabled=1"));
    }

    #[test]
    fn approved_blob_parity() {
        assert!(approved_enabled(None));
        assert!(approved_enabled(Some(0x02)));
        assert!(approved_enabled(Some(0x06)));
        assert!(!approved_enabled(Some(0x03)));
        assert_eq!(approved_blob(true)[0], 0x02);
        assert_eq!(approved_blob(false)[0], 0x03);
    }

    #[test]
    fn explanation_ac81_and_ac82() {
        let backend = mock_backend();
        let action = StartupAppsAction::disable(backend, "Chat");
        validate_explanation(action.explanation()).unwrap();
        assert!(action
            .explanation()
            .local_measurement
            .message()
            .contains("no local measurement"));
    }
}
