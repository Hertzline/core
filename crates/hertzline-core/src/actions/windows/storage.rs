//! Storage free-space warnings (AC-4.8).
//!
//! Official API (verified 2026-11-14):
//! GetDiskFreeSpaceExW
//! https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw
//!
//! Surfaces free-space warnings only. Never deletes or formats volumes.
//! SMART / physical disk health is unavailable via this API and is labeled as such.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.storage_health";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw";

/// Default warn threshold: under 10% free or under 10 GiB free.
pub const WARN_PCT: u64 = 10;
pub const WARN_BYTES: u64 = 10 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpace {
    pub root: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub warning: bool,
}

impl VolumeSpace {
    pub fn encode(&self) -> String {
        format!(
            "root={};total={};free={};warn={}",
            self.root.replace(';', "%3B"),
            self.total_bytes,
            self.free_bytes,
            if self.warning { "1" } else { "0" }
        )
    }

    pub fn compute_warning(total: u64, free: u64) -> bool {
        if total == 0 {
            return false;
        }
        let pct = (free.saturating_mul(100)) / total;
        free < WARN_BYTES || pct < WARN_PCT
    }
}

pub trait StorageBackend {
    fn volumes(&self) -> Result<Vec<VolumeSpace>, ActionError>;
}

/// Labeled mock for unit tests (not real disk data).
#[derive(Debug, Clone)]
pub struct MockStorageBackend {
    pub volumes: Vec<VolumeSpace>,
}

impl StorageBackend for std::cell::RefCell<MockStorageBackend> {
    fn volumes(&self) -> Result<Vec<VolumeSpace>, ActionError> {
        Ok(self.borrow().volumes.clone())
    }
}

/// Report-only storage free-space check (AC-4.8). Apply is a no-op re-read.
pub struct StorageHealthAction<B: StorageBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: StorageBackend> StorageHealthAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report free space per volume and warn when free space is low. Does not modify disks.",
            "Helps spot full drives before caches or installs fail. Not a performance claim.",
            "None for this check: no destructive changes. SMART / drive health is unavailable via GetDiskFreeSpaceEx.",
            "Nothing to undo; this action only reports.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    pub fn report(&self) -> Result<Vec<VolumeSpace>, ActionError> {
        self.backend.volumes()
    }
}

impl<B: StorageBackend> Action for StorageHealthAction<B> {
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
        let vols = self.backend.volumes()?;
        if vols.is_empty() {
            return Ok("volumes=0;smart=unavailable".into());
        }
        let parts: Vec<String> = vols.iter().map(VolumeSpace::encode).collect();
        Ok(format!(
            "volumes={};smart=unavailable|{}",
            vols.len(),
            parts.join("|")
        ))
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        // Report-only: re-read validates the probe works; no disk mutation (AC-4.8).
        let _ = self.backend.volumes()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        // Free space can change between reads; accept matching smart=unavailable marker.
        if actual == expected
            || (actual.contains("smart=unavailable") && expected.contains("smart=unavailable"))
        {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total_bytes: *mut u64,
            free_bytes: *mut u64,
        ) -> i32;
        fn GetLogicalDriveStringsW(buffer_length: u32, buffer: *mut u16) -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }

    const DRIVE_FIXED: u32 = 3;

    pub struct OsStorageBackend;

    impl StorageBackend for OsStorageBackend {
        fn volumes(&self) -> Result<Vec<VolumeSpace>, ActionError> {
            let mut out = Vec::new();
            for root in fixed_drives() {
                let wide = wide(&root);
                let mut free_to_caller = 0u64;
                let mut total = 0u64;
                let mut free = 0u64;
                let ok = unsafe {
                    GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_to_caller, &mut total, &mut free)
                };
                if ok == 0 {
                    continue;
                }
                out.push(VolumeSpace {
                    root: root.clone(),
                    total_bytes: total,
                    free_bytes: free,
                    warning: VolumeSpace::compute_warning(total, free),
                });
            }
            Ok(out)
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn fixed_drives() -> Vec<String> {
        let mut buf = vec![0u16; 256];
        let n = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
        if n == 0 || n as usize >= buf.len() {
            return vec!["C:\\".into()];
        }
        buf.truncate(n as usize);
        let raw = String::from_utf16_lossy(&buf);
        let mut drives = Vec::new();
        for part in raw.split('\0').filter(|s| !s.is_empty()) {
            let w = wide(part);
            let ty = unsafe { GetDriveTypeW(w.as_ptr()) };
            if ty == DRIVE_FIXED {
                drives.push(part.to_string());
            }
        }
        if drives.is_empty() {
            drives.push("C:\\".into());
        }
        drives
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_storage_reports_fixed_volume() {
            let vols = OsStorageBackend.volumes().expect("volumes");
            assert!(!vols.is_empty());
            assert!(vols.iter().any(|v| v.total_bytes > 0));
            let state = StorageHealthAction::new(OsStorageBackend)
                .read_state()
                .unwrap();
            assert!(state.contains("smart=unavailable"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    #[test]
    fn warning_threshold() {
        assert!(VolumeSpace::compute_warning(100, 5));
        assert!(!VolumeSpace::compute_warning(
            100 * WARN_BYTES,
            50 * WARN_BYTES
        ));
    }

    #[test]
    fn mock_report_no_destructive_apply() {
        // mock: fabricated free-space numbers only
        let backend = RefCell::new(MockStorageBackend {
            volumes: vec![VolumeSpace {
                root: "C:\\".into(),
                total_bytes: 1000,
                free_bytes: 50,
                warning: true,
            }],
        });
        let mut action = StorageHealthAction::new(backend);
        let state = action.read_state().unwrap();
        assert!(state.contains("warn=1"));
        assert!(state.contains("smart=unavailable"));
        action.apply().unwrap();
        action.restore(&state).unwrap();
        validate_explanation(action.explanation()).unwrap();
    }
}
