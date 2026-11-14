//! Windows power plan switch (AC-4.1).
//!
//! Uses PowrProf PowerGetActiveScheme / PowerSetActiveScheme (current user).
//! No elevation. Apply targets High Performance when that scheme exists.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

/// High Performance scheme GUID (GUID_MIN_POWER_SAVINGS personality).
pub const GUID_HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
/// Balanced scheme GUID (GUID_TYPICAL_POWER_SAVINGS).
pub const GUID_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
/// Power Saver scheme GUID (GUID_MAX_POWER_SAVINGS).
pub const GUID_POWER_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";

pub const ACTION_ID: &str = "windows.power_plan";

/// Read / write the active power scheme GUID (lowercase, no braces).
pub trait PowerPlanBackend {
    fn get_active(&self) -> Result<String, ActionError>;
    fn set_active(&self, scheme_guid: &str) -> Result<(), ActionError>;
    fn scheme_exists(&self, scheme_guid: &str) -> Result<bool, ActionError>;
}

/// Labeled mock for unit tests (not a real OS power plan).
#[derive(Debug, Clone)]
pub struct MockPowerPlanBackend {
    pub active: String,
    pub schemes: Vec<String>,
}

impl MockPowerPlanBackend {
    pub fn new(active: impl Into<String>, schemes: &[&str]) -> Self {
        Self {
            active: normalize_guid(&active.into()),
            schemes: schemes.iter().map(|s| normalize_guid(s)).collect(),
        }
    }
}

impl PowerPlanBackend for std::cell::RefCell<MockPowerPlanBackend> {
    fn get_active(&self) -> Result<String, ActionError> {
        Ok(self.borrow().active.clone())
    }

    fn set_active(&self, scheme_guid: &str) -> Result<(), ActionError> {
        let guid = normalize_guid(scheme_guid);
        let mut inner = self.borrow_mut();
        if !inner.schemes.iter().any(|s| s == &guid) {
            return Err(ActionError::ApplyFailed(format!(
                "mock: scheme {guid} not present"
            )));
        }
        inner.active = guid;
        Ok(())
    }

    fn scheme_exists(&self, scheme_guid: &str) -> Result<bool, ActionError> {
        let guid = normalize_guid(scheme_guid);
        Ok(self.borrow().schemes.iter().any(|s| s == &guid))
    }
}

/// Switch the active Windows power plan (AC-4.1).
pub struct PowerPlanAction<B: PowerPlanBackend> {
    backend: B,
    target: String,
    explanation: ActionExplanation,
}

impl<B: PowerPlanBackend> PowerPlanAction<B> {
    pub fn new(backend: B, target_scheme: &str) -> Self {
        let target = normalize_guid(target_scheme);
        let mut explanation = ActionExplanation::new(
            "Switch the active Windows power plan (power scheme) for the current user.",
            "Uses a higher-performance plan when available so the CPU/GPU stay less constrained while gaming. Effect varies by machine; measure locally.",
            "Higher power use and heat on laptops; battery drains faster. Group Policy can block scheme changes.",
            "Restore the previous power plan GUID recorded before apply, or use Windows Settings > System > Power.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target,
            explanation,
        }
    }

    /// Default v1 target: High Performance when present.
    pub fn high_performance(backend: B) -> Self {
        Self::new(backend, GUID_HIGH_PERFORMANCE)
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

impl<B: PowerPlanBackend> Action for PowerPlanAction<B> {
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
        self.backend.get_active().map(|g| normalize_guid(&g))
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        if !self.backend.scheme_exists(&self.target)? {
            return Err(ActionError::NotPossible(format!(
                "power scheme {} is not installed on this system",
                self.target
            )));
        }
        self.backend.set_active(&self.target)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let guid = normalize_guid(before);
        if !self.backend.scheme_exists(&guid)? {
            return Err(ActionError::RestoreFailed(format!(
                "previous scheme {guid} is no longer present"
            )));
        }
        self.backend.set_active(&guid)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = normalize_guid(&self.backend.get_active()?);
        let expected = normalize_guid(expected);
        if actual == expected {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch { expected, actual })
        }
    }
}

/// Normalize to lowercase hex without braces.
pub fn normalize_guid(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_lowercase()
}

/// Friendly label for known Microsoft built-in schemes.
pub fn known_scheme_name(guid: &str) -> Option<&'static str> {
    match normalize_guid(guid).as_str() {
        GUID_HIGH_PERFORMANCE => Some("High performance"),
        GUID_BALANCED => Some("Balanced"),
        GUID_POWER_SAVER => Some("Power saver"),
        _ => None,
    }
}

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::ffi::c_void;
    use std::ptr;

    const ACCESS_SCHEME: u32 = 16;
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_MORE_DATA: u32 = 234;
    const SOURCE: &str =
        "https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powergetactivescheme";

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[link(name = "powrprof")]
    extern "system" {
        fn PowerGetActiveScheme(
            user_root_power_key: *mut c_void,
            active_policy_guid: *mut *mut Guid,
        ) -> u32;
        fn PowerSetActiveScheme(user_root_power_key: *mut c_void, scheme_guid: *const Guid) -> u32;
        fn PowerEnumerate(
            root_power_key: *mut c_void,
            scheme_guid: *const Guid,
            subgroup: *const Guid,
            access_flags: u32,
            index: u32,
            buffer: *mut u8,
            buffer_size: *mut u32,
        ) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    pub struct OsPowerPlanBackend;

    impl PowerPlanBackend for OsPowerPlanBackend {
        fn get_active(&self) -> Result<String, ActionError> {
            let mut ptr: *mut Guid = ptr::null_mut();
            let status = unsafe { PowerGetActiveScheme(ptr::null_mut(), &mut ptr) };
            if status != ERROR_SUCCESS || ptr.is_null() {
                return Err(ActionError::ApplyFailed(format!(
                    "PowerGetActiveScheme failed ({status}); see {SOURCE}"
                )));
            }
            let guid = unsafe { *ptr };
            unsafe { LocalFree(ptr.cast()) };
            Ok(guid_to_string(&guid))
        }

        fn set_active(&self, scheme_guid: &str) -> Result<(), ActionError> {
            let guid = parse_guid(scheme_guid).ok_or_else(|| {
                ActionError::ApplyFailed(format!("invalid power scheme GUID: {scheme_guid}"))
            })?;
            let status = unsafe { PowerSetActiveScheme(ptr::null_mut(), &guid) };
            if status != ERROR_SUCCESS {
                return Err(ActionError::ApplyFailed(format!(
                    "PowerSetActiveScheme failed ({status}); see {SOURCE}"
                )));
            }
            Ok(())
        }

        fn scheme_exists(&self, scheme_guid: &str) -> Result<bool, ActionError> {
            let want = normalize_guid(scheme_guid);
            for existing in enumerate_schemes()? {
                if existing == want {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }

    fn enumerate_schemes() -> Result<Vec<String>, ActionError> {
        let mut out = Vec::new();
        let mut index = 0u32;
        loop {
            let mut size = 0u32;
            let probe = unsafe {
                PowerEnumerate(
                    ptr::null_mut(),
                    ptr::null(),
                    ptr::null(),
                    ACCESS_SCHEME,
                    index,
                    ptr::null_mut(),
                    &mut size,
                )
            };
            if probe != ERROR_MORE_DATA && probe != ERROR_SUCCESS {
                break;
            }
            if size == 0 {
                break;
            }
            let mut buf = vec![0u8; size as usize];
            let status = unsafe {
                PowerEnumerate(
                    ptr::null_mut(),
                    ptr::null(),
                    ptr::null(),
                    ACCESS_SCHEME,
                    index,
                    buf.as_mut_ptr(),
                    &mut size,
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            if size as usize >= std::mem::size_of::<Guid>() {
                let guid = unsafe { ptr::read_unaligned(buf.as_ptr().cast::<Guid>()) };
                out.push(guid_to_string(&guid));
            }
            index += 1;
        }
        Ok(out)
    }

    fn guid_to_string(g: &Guid) -> String {
        format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            g.data1,
            g.data2,
            g.data3,
            g.data4[0],
            g.data4[1],
            g.data4[2],
            g.data4[3],
            g.data4[4],
            g.data4[5],
            g.data4[6],
            g.data4[7]
        )
    }

    fn parse_guid(s: &str) -> Option<Guid> {
        let n = normalize_guid(s);
        let parts: Vec<&str> = n.split('-').collect();
        if parts.len() != 5 {
            return None;
        }
        let data1 = u32::from_str_radix(parts[0], 16).ok()?;
        let data2 = u16::from_str_radix(parts[1], 16).ok()?;
        let data3 = u16::from_str_radix(parts[2], 16).ok()?;
        let mut data4 = [0u8; 8];
        let p3 = parts[3];
        let p4 = parts[4];
        if p3.len() != 4 || p4.len() != 12 {
            return None;
        }
        data4[0] = u8::from_str_radix(&p3[0..2], 16).ok()?;
        data4[1] = u8::from_str_radix(&p3[2..4], 16).ok()?;
        for i in 0..6 {
            data4[2 + i] = u8::from_str_radix(&p4[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Guid {
            data1,
            data2,
            data3,
            data4,
        })
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_get_active_returns_guid() {
            let backend = OsPowerPlanBackend;
            let active = backend.get_active().expect("PowerGetActiveScheme");
            assert_eq!(active.len(), 36);
            assert!(active.contains('-'));
            assert!(backend.scheme_exists(&active).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    #[test]
    fn mock_apply_restore_verify_power_plan() {
        // mock: in-memory schemes only
        let backend = RefCell::new(MockPowerPlanBackend::new(
            GUID_BALANCED,
            &[GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_POWER_SAVER],
        ));
        let mut action = PowerPlanAction::high_performance(backend);
        assert!(!action.requires_elevation());
        assert_eq!(action.read_state().unwrap(), GUID_BALANCED);
        action.apply().unwrap();
        assert_eq!(action.read_state().unwrap(), GUID_HIGH_PERFORMANCE);
        assert!(action.verify(GUID_HIGH_PERFORMANCE).unwrap().is_match());
        action.restore(GUID_BALANCED).unwrap();
        assert!(action.verify(GUID_BALANCED).unwrap().is_match());
    }

    #[test]
    fn missing_target_scheme_is_not_possible() {
        let backend = RefCell::new(MockPowerPlanBackend::new(GUID_BALANCED, &[GUID_BALANCED]));
        let mut action = PowerPlanAction::high_performance(backend);
        assert_eq!(
            action.apply().unwrap_err(),
            ActionError::NotPossible(format!(
                "power scheme {GUID_HIGH_PERFORMANCE} is not installed on this system"
            ))
        );
    }

    #[test]
    fn snapshot_round_trip_records_guids() {
        let backend = RefCell::new(MockPowerPlanBackend::new(
            GUID_POWER_SAVER,
            &[GUID_POWER_SAVER, GUID_HIGH_PERFORMANCE],
        ));
        let mut action = PowerPlanAction::high_performance(backend);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 100).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        assert_eq!(entry.before, GUID_POWER_SAVER);
        assert_eq!(entry.after, GUID_HIGH_PERFORMANCE);
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert_eq!(action.read_state().unwrap(), GUID_POWER_SAVER);
    }

    #[test]
    fn explanation_covers_ac81() {
        let backend = RefCell::new(MockPowerPlanBackend::new(GUID_BALANCED, &[GUID_BALANCED]));
        let action = PowerPlanAction::high_performance(backend);
        crate::action::validate_explanation(action.explanation()).unwrap();
        assert_eq!(
            action.explanation().local_measurement.message(),
            LocalMeasurementNote::NONE_YET_MESSAGE
        );
    }

    #[test]
    fn normalize_strips_braces() {
        assert_eq!(
            normalize_guid(&format!("{{{GUID_BALANCED}}}")),
            GUID_BALANCED
        );
    }
}
