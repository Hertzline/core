//! Windows process priority and optional CPU affinity (AC-4.3).
//!
//! Official sources (verified 2026-11-14):
//! - SetPriorityClass / GetPriorityClass
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getpriorityclass
//! - SetProcessAffinityMask / GetProcessAffinityMask
//!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setprocessaffinitymask
//!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getprocessaffinitymask
//! - OpenProcess
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess
//!
//! REALTIME_PRIORITY_CLASS is rejected (system-starvation risk). Affinity is optional.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.process_priority";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass";

pub const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
pub const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
pub const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
pub const REALTIME_PRIORITY_CLASS: u32 = 0x0000_0100;

/// Parsed live / snapshot state for one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPriorityState {
    pub pid: u32,
    pub priority: u32,
    pub affinity: Option<u64>,
}

impl ProcessPriorityState {
    pub fn encode(&self) -> String {
        let aff = match self.affinity {
            Some(mask) => format!("0x{mask:x}"),
            None => "none".into(),
        };
        format!(
            "pid={};priority=0x{:x};affinity={}",
            self.pid, self.priority, aff
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut pid = None;
        let mut priority = None;
        let mut affinity = None;
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
                "priority" => {
                    priority = Some(parse_u32_token(v.trim()).map_err(|e| {
                        ActionError::RestoreFailed(format!("invalid priority: {e}"))
                    })?);
                }
                "affinity" => {
                    let v = v.trim();
                    affinity = if v.eq_ignore_ascii_case("none") {
                        Some(None)
                    } else {
                        Some(Some(parse_u64_token(v).map_err(|e| {
                            ActionError::RestoreFailed(format!("invalid affinity: {e}"))
                        })?))
                    };
                }
                _ => {}
            }
        }
        Ok(Self {
            pid: pid.ok_or_else(|| ActionError::RestoreFailed("missing pid".into()))?,
            priority: priority
                .ok_or_else(|| ActionError::RestoreFailed("missing priority".into()))?,
            affinity: affinity.unwrap_or(None),
        })
    }
}

fn parse_u32_token(v: &str) -> Result<u32, String> {
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        v.parse::<u32>().map_err(|e| e.to_string())
    }
}

fn parse_u64_token(v: &str) -> Result<u64, String> {
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        v.parse::<u64>().map_err(|e| e.to_string())
    }
}

/// Reject REALTIME; allow documented priority classes only.
pub fn validate_priority_class(priority: u32) -> Result<(), ActionError> {
    match priority {
        NORMAL_PRIORITY_CLASS
        | HIGH_PRIORITY_CLASS
        | ABOVE_NORMAL_PRIORITY_CLASS
        | BELOW_NORMAL_PRIORITY_CLASS
        | IDLE_PRIORITY_CLASS => Ok(()),
        REALTIME_PRIORITY_CLASS => Err(ActionError::ApplyFailed(
            "REALTIME_PRIORITY_CLASS is rejected (system starvation risk)".into(),
        )),
        other => Err(ActionError::ApplyFailed(format!(
            "unsupported priority class 0x{other:x}"
        ))),
    }
}

/// Read / write process priority and optional affinity for a user-chosen PID.
pub trait ProcessPriorityBackend {
    fn read(&self, pid: u32) -> Result<ProcessPriorityState, ActionError>;
    fn set_priority(&self, pid: u32, priority: u32) -> Result<(), ActionError>;
    fn set_affinity(&self, pid: u32, mask: u64) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real process).
#[derive(Debug, Clone)]
pub struct MockProcessPriorityBackend {
    pub pid: u32,
    pub priority: u32,
    pub affinity: Option<u64>,
    pub alive: bool,
}

impl MockProcessPriorityBackend {
    pub fn new(pid: u32, priority: u32, affinity: Option<u64>) -> Self {
        Self {
            pid,
            priority,
            affinity,
            alive: true,
        }
    }
}

impl ProcessPriorityBackend for std::cell::RefCell<MockProcessPriorityBackend> {
    fn read(&self, pid: u32) -> Result<ProcessPriorityState, ActionError> {
        let b = self.borrow();
        if !b.alive || b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "process {pid} not found (mock)"
            )));
        }
        Ok(ProcessPriorityState {
            pid: b.pid,
            priority: b.priority,
            affinity: b.affinity,
        })
    }

    fn set_priority(&self, pid: u32, priority: u32) -> Result<(), ActionError> {
        validate_priority_class(priority)?;
        let mut b = self.borrow_mut();
        if !b.alive || b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "process {pid} not found (mock)"
            )));
        }
        b.priority = priority;
        Ok(())
    }

    fn set_affinity(&self, pid: u32, mask: u64) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if !b.alive || b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "process {pid} not found (mock)"
            )));
        }
        b.affinity = Some(mask);
        Ok(())
    }
}

/// Raise (or restore) priority / optional affinity for a user-chosen game PID (AC-4.3).
pub struct ProcessPriorityAction<B: ProcessPriorityBackend> {
    backend: B,
    pid: u32,
    target_priority: u32,
    target_affinity: Option<u64>,
    explanation: ActionExplanation,
}

impl<B: ProcessPriorityBackend> ProcessPriorityAction<B> {
    pub fn new(backend: B, pid: u32, target_priority: u32, target_affinity: Option<u64>) -> Self {
        let mut explanation = ActionExplanation::new(
            "Set the user-chosen game process priority class and optionally its CPU affinity mask.",
            "Higher priority can reduce scheduling latency for that process; affinity can pin it to selected CPUs. Effect is machine-specific; measure locally.",
            "Medium: HIGH can starve other work if the game is CPU-bound. Wrong affinity can hurt performance on hybrid CPUs. Never uses REALTIME.",
            "Restore the snapshotted priority and affinity via SetPriorityClass / SetProcessAffinityMask, or end the process and relaunch.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            pid,
            target_priority,
            target_affinity,
            explanation,
        }
    }

    pub fn high_priority(backend: B, pid: u32) -> Self {
        Self::new(backend, pid, HIGH_PRIORITY_CLASS, None)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl<B: ProcessPriorityBackend> Action for ProcessPriorityAction<B> {
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
        Ok(self.backend.read(self.pid)?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        validate_priority_class(self.target_priority)?;
        self.backend.set_priority(self.pid, self.target_priority)?;
        if let Some(mask) = self.target_affinity {
            self.backend.set_affinity(self.pid, mask)?;
        }
        Ok(())
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = ProcessPriorityState::decode(before)?;
        // Adopt snapshot pid so headless restore-all can reuse one registered action.
        self.pid = state.pid;
        validate_priority_class(state.priority)?;
        self.backend.set_priority(self.pid, state.priority)?;
        if let Some(mask) = state.affinity {
            self.backend.set_affinity(self.pid, mask)?;
        }
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = ProcessPriorityState::decode(expected)?;
        let actual_state = ProcessPriorityState::decode(&actual)?;
        // Affinity may be omitted from target apply; compare priority always,
        // and affinity only when expected encodes one.
        let priority_ok = actual_state.priority == expected_state.priority
            && actual_state.pid == expected_state.pid;
        let affinity_ok = match expected_state.affinity {
            None => true,
            Some(mask) => actual_state.affinity == Some(mask),
        };
        if priority_ok && affinity_ok {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.to_string(),
                actual,
            })
        }
    }
}

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::ffi::c_void;

    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_SET_INFORMATION: u32 = 0x0200;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetPriorityClass(handle: *mut c_void) -> u32;
        fn SetPriorityClass(handle: *mut c_void, priority: u32) -> i32;
        fn GetProcessAffinityMask(
            handle: *mut c_void,
            process_mask: *mut usize,
            system_mask: *mut usize,
        ) -> i32;
        fn SetProcessAffinityMask(handle: *mut c_void, mask: usize) -> i32;
        fn GetLastError() -> u32;
    }

    pub struct OsProcessPriorityBackend;

    impl ProcessPriorityBackend for OsProcessPriorityBackend {
        fn read(&self, pid: u32) -> Result<ProcessPriorityState, ActionError> {
            let handle = open(
                pid,
                PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            )?;
            let priority = unsafe { GetPriorityClass(handle) };
            if priority == 0 {
                let err = unsafe { GetLastError() };
                unsafe { CloseHandle(handle) };
                return Err(ActionError::ApplyFailed(format!(
                    "GetPriorityClass failed ({err}); see {SOURCE_URL}"
                )));
            }
            let mut process_mask = 0usize;
            let mut system_mask = 0usize;
            let ok = unsafe { GetProcessAffinityMask(handle, &mut process_mask, &mut system_mask) };
            unsafe { CloseHandle(handle) };
            let affinity = if ok != 0 {
                Some(process_mask as u64)
            } else {
                None
            };
            Ok(ProcessPriorityState {
                pid,
                priority,
                affinity,
            })
        }

        fn set_priority(&self, pid: u32, priority: u32) -> Result<(), ActionError> {
            validate_priority_class(priority)?;
            let handle = open(
                pid,
                PROCESS_SET_INFORMATION
                    | PROCESS_QUERY_INFORMATION
                    | PROCESS_QUERY_LIMITED_INFORMATION,
            )?;
            let ok = unsafe { SetPriorityClass(handle, priority) };
            let err = unsafe { GetLastError() };
            unsafe { CloseHandle(handle) };
            if ok == 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "SetPriorityClass failed ({err}); see {SOURCE_URL}"
                )));
            }
            Ok(())
        }

        fn set_affinity(&self, pid: u32, mask: u64) -> Result<(), ActionError> {
            let handle = open(
                pid,
                PROCESS_SET_INFORMATION
                    | PROCESS_QUERY_INFORMATION
                    | PROCESS_QUERY_LIMITED_INFORMATION,
            )?;
            let ok = unsafe { SetProcessAffinityMask(handle, mask as usize) };
            let err = unsafe { GetLastError() };
            unsafe { CloseHandle(handle) };
            if ok == 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "SetProcessAffinityMask failed ({err}); see https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setprocessaffinitymask"
                )));
            }
            Ok(())
        }
    }

    fn open(pid: u32, access: u32) -> Result<*mut c_void, ActionError> {
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(ActionError::ApplyFailed(format!(
                "OpenProcess({pid}) failed ({err}); see https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess"
            )));
        }
        Ok(handle)
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_reads_current_process() {
            let pid = std::process::id();
            let state = OsProcessPriorityBackend
                .read(pid)
                .expect("read current process");
            assert_eq!(state.pid, pid);
            assert_ne!(state.priority, 0);
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
    fn mock_apply_restore_verify_priority() {
        // mock: in-memory priority only
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            42,
            NORMAL_PRIORITY_CLASS,
            Some(0xf),
        ));
        let mut action = ProcessPriorityAction::high_priority(backend, 42);
        assert!(!action.requires_elevation());
        assert!(action.read_state().unwrap().contains("priority=0x20"));
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("priority=0x80"));
        assert!(action
            .verify(
                &ProcessPriorityState {
                    pid: 42,
                    priority: HIGH_PRIORITY_CLASS,
                    affinity: Some(0xf),
                }
                .encode()
            )
            .unwrap()
            .is_match());
        action
            .restore(
                &ProcessPriorityState {
                    pid: 42,
                    priority: NORMAL_PRIORITY_CLASS,
                    affinity: Some(0xf),
                }
                .encode(),
            )
            .unwrap();
        assert!(action.read_state().unwrap().contains("priority=0x20"));
    }

    #[test]
    fn mock_optional_affinity_apply() {
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            7,
            NORMAL_PRIORITY_CLASS,
            Some(0xff),
        ));
        let mut action =
            ProcessPriorityAction::new(backend, 7, ABOVE_NORMAL_PRIORITY_CLASS, Some(0x3));
        action.apply().unwrap();
        let state = ProcessPriorityState::decode(&action.read_state().unwrap()).unwrap();
        assert_eq!(state.priority, ABOVE_NORMAL_PRIORITY_CLASS);
        assert_eq!(state.affinity, Some(0x3));
    }

    #[test]
    fn rejects_realtime() {
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            1,
            NORMAL_PRIORITY_CLASS,
            None,
        ));
        let mut action = ProcessPriorityAction::new(backend, 1, REALTIME_PRIORITY_CLASS, None);
        assert!(action.apply().unwrap_err().to_string().contains("REALTIME"));
    }

    #[test]
    fn snapshot_round_trip() {
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            9,
            NORMAL_PRIORITY_CLASS,
            None,
        ));
        let mut action = ProcessPriorityAction::high_priority(backend, 9);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 21).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert!(action.read_state().unwrap().contains("priority=0x20"));
    }

    #[test]
    fn explanation_ac81_and_ac82() {
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            1,
            NORMAL_PRIORITY_CLASS,
            None,
        ));
        let action = ProcessPriorityAction::high_priority(backend, 1);
        validate_explanation(action.explanation()).unwrap();
        assert!(action
            .explanation()
            .local_measurement
            .message()
            .contains("no local measurement"));
    }

    #[test]
    fn user_chosen_pid_only() {
        let backend = RefCell::new(MockProcessPriorityBackend::new(
            100,
            NORMAL_PRIORITY_CLASS,
            None,
        ));
        let action = ProcessPriorityAction::high_priority(backend, 100);
        assert_eq!(action.pid(), 100);
        let other = RefCell::new(MockProcessPriorityBackend::new(
            100,
            NORMAL_PRIORITY_CLASS,
            None,
        ));
        assert!(other
            .read(999)
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }
}
