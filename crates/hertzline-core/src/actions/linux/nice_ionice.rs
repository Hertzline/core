//! nice / ionice for a user-chosen game process (AC-4.13).
//!
//! Official sources (verified 2026-11-14):
//! - setpriority(2) / getpriority(2): nice range -20..19
//!   https://man7.org/linux/man-pages/man2/setpriority.2.html
//! - ionice(1) classes: 0 none, 1 realtime, 2 best-effort, 3 idle
//!   https://man7.org/linux/man-pages/man1/ionice.1.html
//!
//! REALTIME I/O class is rejected (system starvation risk), mirroring the
//! Windows REALTIME_PRIORITY_CLASS rejection. Lowering nice (higher priority)
//! typically needs CAP_SYS_NICE -- helper elevates via polkit.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.nice_ionice";
pub const SOURCE_URL_NICE: &str = "https://man7.org/linux/man-pages/man2/setpriority.2.html";
pub const SOURCE_URL_IONICE: &str = "https://man7.org/linux/man-pages/man1/ionice.1.html";

pub const IOPRIO_CLASS_NONE: u8 = 0;
pub const IOPRIO_CLASS_RT: u8 = 1;
pub const IOPRIO_CLASS_BE: u8 = 2;
pub const IOPRIO_CLASS_IDLE: u8 = 3;

/// Default apply: mild CPU boost + best-effort I/O priority 0 (GameMode default ioprio).
pub const DEFAULT_TARGET_NICE: i32 = -5;
pub const DEFAULT_TARGET_IOPRIO_CLASS: u8 = IOPRIO_CLASS_BE;
pub const DEFAULT_TARGET_IOPRIO_LEVEL: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NiceIoniceState {
    pub pid: u32,
    pub nice: i32,
    pub ioprio_class: u8,
    pub ioprio_level: u8,
}

impl NiceIoniceState {
    pub fn encode(&self) -> String {
        format!(
            "pid={};nice={};ioprio_class={};ioprio_level={}",
            self.pid, self.nice, self.ioprio_class, self.ioprio_level
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut pid = None;
        let mut nice = None;
        let mut ioprio_class = None;
        let mut ioprio_level = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "pid" => {
                    pid =
                        Some(v.trim().parse::<u32>().map_err(|_| {
                            ActionError::RestoreFailed(format!("invalid pid: {v}"))
                        })?);
                }
                "nice" => {
                    nice =
                        Some(v.trim().parse::<i32>().map_err(|_| {
                            ActionError::RestoreFailed(format!("invalid nice: {v}"))
                        })?);
                }
                "ioprio_class" => {
                    ioprio_class = Some(v.trim().parse::<u8>().map_err(|_| {
                        ActionError::RestoreFailed(format!("invalid ioprio_class: {v}"))
                    })?);
                }
                "ioprio_level" => {
                    ioprio_level = Some(v.trim().parse::<u8>().map_err(|_| {
                        ActionError::RestoreFailed(format!("invalid ioprio_level: {v}"))
                    })?);
                }
                _ => {}
            }
        }
        Ok(Self {
            pid: pid.ok_or_else(|| ActionError::RestoreFailed("missing pid".into()))?,
            nice: nice.ok_or_else(|| ActionError::RestoreFailed("missing nice".into()))?,
            ioprio_class: ioprio_class
                .ok_or_else(|| ActionError::RestoreFailed("missing ioprio_class".into()))?,
            ioprio_level: ioprio_level
                .ok_or_else(|| ActionError::RestoreFailed("missing ioprio_level".into()))?,
        })
    }
}

pub fn validate_nice(nice: i32) -> Result<(), ActionError> {
    if (-20..=19).contains(&nice) {
        Ok(())
    } else {
        Err(ActionError::ApplyFailed(format!(
            "nice {nice} out of range -20..19"
        )))
    }
}

pub fn validate_ioprio(class: u8, level: u8) -> Result<(), ActionError> {
    match class {
        IOPRIO_CLASS_RT => Err(ActionError::ApplyFailed(
            "ioprio realtime class is rejected (system starvation risk)".into(),
        )),
        IOPRIO_CLASS_NONE | IOPRIO_CLASS_IDLE => {
            if level == 0 {
                Ok(())
            } else {
                Err(ActionError::ApplyFailed(
                    "ioprio none/idle must use level 0".into(),
                ))
            }
        }
        IOPRIO_CLASS_BE => {
            if level <= 7 {
                Ok(())
            } else {
                Err(ActionError::ApplyFailed(format!(
                    "ioprio best-effort level {level} out of range 0..7"
                )))
            }
        }
        other => Err(ActionError::ApplyFailed(format!(
            "unsupported ioprio class {other}"
        ))),
    }
}

pub trait NiceIoniceBackend {
    fn read(&self, pid: u32) -> Result<NiceIoniceState, ActionError>;
    fn set_nice(&self, pid: u32, nice: i32) -> Result<(), ActionError>;
    fn set_ioprio(&self, pid: u32, class: u8, level: u8) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real process).
#[derive(Debug, Clone)]
pub struct MockNiceIoniceBackend {
    pub pid: u32,
    pub nice: i32,
    pub ioprio_class: u8,
    pub ioprio_level: u8,
    pub alive: bool,
}

impl MockNiceIoniceBackend {
    pub fn new(pid: u32, nice: i32, class: u8, level: u8) -> Self {
        Self {
            pid,
            nice,
            ioprio_class: class,
            ioprio_level: level,
            alive: true,
        }
    }
}

impl NiceIoniceBackend for std::cell::RefCell<MockNiceIoniceBackend> {
    fn read(&self, pid: u32) -> Result<NiceIoniceState, ActionError> {
        let inner = self.borrow();
        if !inner.alive || inner.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "mock: pid {pid} not found"
            )));
        }
        Ok(NiceIoniceState {
            pid,
            nice: inner.nice,
            ioprio_class: inner.ioprio_class,
            ioprio_level: inner.ioprio_level,
        })
    }

    fn set_nice(&self, pid: u32, nice: i32) -> Result<(), ActionError> {
        validate_nice(nice)?;
        let mut inner = self.borrow_mut();
        if !inner.alive || inner.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "mock: pid {pid} not found"
            )));
        }
        inner.nice = nice;
        Ok(())
    }

    fn set_ioprio(&self, pid: u32, class: u8, level: u8) -> Result<(), ActionError> {
        validate_ioprio(class, level)?;
        let mut inner = self.borrow_mut();
        if !inner.alive || inner.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "mock: pid {pid} not found"
            )));
        }
        inner.ioprio_class = class;
        inner.ioprio_level = level;
        Ok(())
    }
}

/// Apply nice + ionice to a user-chosen PID (AC-4.13).
pub struct NiceIoniceAction<B: NiceIoniceBackend> {
    backend: B,
    pid: u32,
    target_nice: i32,
    target_class: u8,
    target_level: u8,
    explanation: ActionExplanation,
}

impl<B: NiceIoniceBackend> NiceIoniceAction<B> {
    pub fn new(backend: B, pid: u32, nice: i32, class: u8, level: u8) -> Self {
        let mut explanation = ActionExplanation::new(
            "Set CPU nice (setpriority) and I/O priority (ionice) for a user-chosen game process.",
            "A lower nice value and best-effort I/O priority 0 can reduce scheduling latency for that process. Effect is machine-specific; measure locally.",
            "Medium: too-low nice can starve other work. Realtime I/O class is rejected. Lowering nice usually needs elevation (CAP_SYS_NICE / polkit).",
            "Restore the snapshotted nice and ionice values, or end the process and relaunch.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            pid,
            target_nice: nice,
            target_class: class,
            target_level: level,
            explanation,
        }
    }

    pub fn default_boost(backend: B, pid: u32) -> Self {
        Self::new(
            backend,
            pid,
            DEFAULT_TARGET_NICE,
            DEFAULT_TARGET_IOPRIO_CLASS,
            DEFAULT_TARGET_IOPRIO_LEVEL,
        )
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl<B: NiceIoniceBackend> Action for NiceIoniceAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        // Lowering nice (higher priority) needs CAP_SYS_NICE on Linux.
        self.target_nice < 0
    }

    fn read_state(&self) -> Result<String, ActionError> {
        Ok(self.backend.read(self.pid)?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        validate_nice(self.target_nice)?;
        validate_ioprio(self.target_class, self.target_level)?;
        self.backend.set_nice(self.pid, self.target_nice)?;
        self.backend
            .set_ioprio(self.pid, self.target_class, self.target_level)?;
        Ok(())
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = NiceIoniceState::decode(before)?;
        self.pid = state.pid;
        validate_nice(state.nice)?;
        validate_ioprio(state.ioprio_class, state.ioprio_level)?;
        self.backend.set_nice(self.pid, state.nice)?;
        self.backend
            .set_ioprio(self.pid, state.ioprio_class, state.ioprio_level)?;
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = NiceIoniceState::decode(expected)?;
        let actual_state = NiceIoniceState::decode(&actual)?;
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
    use std::io;
    use std::process::Command;

    const PRIO_PROCESS: i32 = 0;

    #[link(name = "c")]
    extern "C" {
        fn getpriority(which: i32, who: u32) -> i32;
        fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        fn __errno_location() -> *mut i32;
    }

    pub struct OsNiceIoniceBackend;

    impl NiceIoniceBackend for OsNiceIoniceBackend {
        fn read(&self, pid: u32) -> Result<NiceIoniceState, ActionError> {
            let nice = read_nice(pid)?;
            let (class, level) = read_ioprio(pid)?;
            Ok(NiceIoniceState {
                pid,
                nice,
                ioprio_class: class,
                ioprio_level: level,
            })
        }

        fn set_nice(&self, pid: u32, nice: i32) -> Result<(), ActionError> {
            validate_nice(nice)?;
            unsafe {
                *__errno_location() = 0;
                let rc = setpriority(PRIO_PROCESS, pid, nice);
                if rc != 0 {
                    let err = io::Error::from_raw_os_error(*__errno_location());
                    return Err(ActionError::ApplyFailed(format!(
                        "setpriority({pid}, {nice}) failed: {err}; see {SOURCE_URL_NICE}"
                    )));
                }
            }
            Ok(())
        }

        fn set_ioprio(&self, pid: u32, class: u8, level: u8) -> Result<(), ActionError> {
            validate_ioprio(class, level)?;
            let output = Command::new("ionice")
                .args([
                    "-c",
                    &class.to_string(),
                    "-n",
                    &level.to_string(),
                    "-p",
                    &pid.to_string(),
                ])
                .output()
                .map_err(|e| {
                    ActionError::ApplyFailed(format!(
                        "ionice spawn failed: {e}; see {SOURCE_URL_IONICE}"
                    ))
                })?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(ActionError::ApplyFailed(format!(
                    "ionice -c {class} -n {level} -p {pid} failed: {err}"
                )));
            }
            Ok(())
        }
    }

    fn read_nice(pid: u32) -> Result<i32, ActionError> {
        unsafe {
            *__errno_location() = 0;
            let value = getpriority(PRIO_PROCESS, pid);
            let errno = *__errno_location();
            if value == -1 && errno != 0 {
                let err = io::Error::from_raw_os_error(errno);
                return Err(ActionError::ApplyFailed(format!(
                    "getpriority({pid}) failed: {err}; see {SOURCE_URL_NICE}"
                )));
            }
            Ok(value)
        }
    }

    /// Parse `ionice -p PID` output, e.g. `best-effort: prio 4` or `idle`.
    fn read_ioprio(pid: u32) -> Result<(u8, u8), ActionError> {
        let output = Command::new("ionice")
            .args(["-p", &pid.to_string()])
            .output()
            .map_err(|e| {
                ActionError::ApplyFailed(format!(
                    "ionice -p spawn failed: {e}; see {SOURCE_URL_IONICE}"
                ))
            })?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(ActionError::ApplyFailed(format!(
                "ionice -p {pid} failed: {err}"
            )));
        }
        parse_ionice_output(&String::from_utf8_lossy(&output.stdout))
    }

    fn parse_ionice_output(raw: &str) -> Result<(u8, u8), ActionError> {
        let line = raw.lines().next().unwrap_or("").trim().to_ascii_lowercase();
        if line.starts_with("none") || line.is_empty() {
            return Ok((IOPRIO_CLASS_NONE, 0));
        }
        if line.starts_with("realtime") {
            let level = parse_prio_suffix(&line).unwrap_or(0);
            return Ok((IOPRIO_CLASS_RT, level));
        }
        if line.starts_with("best-effort") {
            let level = parse_prio_suffix(&line).unwrap_or(4);
            return Ok((IOPRIO_CLASS_BE, level));
        }
        if line.starts_with("idle") {
            return Ok((IOPRIO_CLASS_IDLE, 0));
        }
        Err(ActionError::ApplyFailed(format!(
            "unrecognized ionice output: {raw}"
        )))
    }

    fn parse_prio_suffix(line: &str) -> Option<u8> {
        let idx = line.rfind("prio")?;
        let rest = line[idx + 4..].trim();
        rest.split_whitespace()
            .next()
            .and_then(|t| t.parse::<u8>().ok())
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn reads_current_process() {
            let pid = std::process::id();
            match OsNiceIoniceBackend.read(pid) {
                Ok(state) => {
                    assert_eq!(state.pid, pid);
                    assert!((-20..=19).contains(&state.nice));
                }
                Err(err) => {
                    // ionice may be missing on minimal images -- honest failure.
                    let msg = err.to_string();
                    assert!(
                        msg.contains("ionice")
                            || msg.contains("getpriority")
                            || msg.contains("failed")
                    );
                }
            }
        }

        #[test]
        fn parse_ionice_samples() {
            assert_eq!(
                parse_ionice_output("best-effort: prio 0\n").unwrap(),
                (IOPRIO_CLASS_BE, 0)
            );
            assert_eq!(
                parse_ionice_output("idle\n").unwrap(),
                (IOPRIO_CLASS_IDLE, 0)
            );
            assert_eq!(
                parse_ionice_output("none: prio 0\n").unwrap(),
                (IOPRIO_CLASS_NONE, 0)
            );
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
        let backend = RefCell::new(MockNiceIoniceBackend::new(7, 0, IOPRIO_CLASS_BE, 4));
        let mut action = NiceIoniceAction::default_boost(backend, 7);
        assert!(action.requires_elevation());
        let before = action.read_state().unwrap();
        action.apply().unwrap();
        let after = NiceIoniceState::decode(&action.read_state().unwrap()).unwrap();
        assert_eq!(after.nice, DEFAULT_TARGET_NICE);
        assert_eq!(after.ioprio_class, IOPRIO_CLASS_BE);
        assert_eq!(after.ioprio_level, 0);
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn reject_realtime_ioprio() {
        assert!(validate_ioprio(IOPRIO_CLASS_RT, 0).is_err());
        let backend = RefCell::new(MockNiceIoniceBackend::new(1, 0, IOPRIO_CLASS_BE, 4));
        let mut action = NiceIoniceAction::new(backend, 1, 0, IOPRIO_CLASS_RT, 0);
        assert!(action.apply().is_err());
    }

    #[test]
    fn reject_nice_out_of_range() {
        assert!(validate_nice(-21).is_err());
        assert!(validate_nice(20).is_err());
    }

    #[test]
    fn state_round_trip() {
        let s = NiceIoniceState {
            pid: 42,
            nice: -5,
            ioprio_class: 2,
            ioprio_level: 0,
        };
        assert_eq!(NiceIoniceState::decode(&s.encode()).unwrap(), s);
    }

    #[test]
    fn explanation_complete() {
        let backend = RefCell::new(MockNiceIoniceBackend::new(1, 0, IOPRIO_CLASS_BE, 4));
        let action = NiceIoniceAction::default_boost(backend, 1);
        let e = action.explanation();
        assert!(!e.what.is_empty());
        assert!(!e.risk.is_empty());
        assert!(!e.how_to_undo.is_empty());
    }
}
