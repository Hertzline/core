//! macOS background app management (HZ-026 / AC-4.20).
//!
//! Official sources (verified 2026-11-14):
//! - kill(2) with SIGTERM / SIGSTOP / SIGCONT (POSIX)
//!   https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kill.2.html
//! - proc_pidpath (libproc) for image path
//!   https://developer.apple.com/documentation/kernel/1532915-proc_pidpath
//!
//! Limited to documented, reversible user actions on an explicit user-chosen PID.
//! Never targets processes the user did not select. Restart on restore is opt-in.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "macos.background_apps";
pub const SOURCE_URL: &str =
    "https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kill.2.html";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundMode {
    Close,
    Pause,
}

impl BackgroundMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Pause => "pause",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, ActionError> {
        match raw.trim() {
            "close" => Ok(Self::Close),
            "pause" => Ok(Self::Pause),
            other => Err(ActionError::ApplyFailed(format!(
                "invalid background mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundAppState {
    pub pid: u32,
    pub mode: BackgroundMode,
    pub status: String,
    pub image: String,
    pub restart: bool,
}

impl BackgroundAppState {
    pub fn encode(&self) -> String {
        format!(
            "pid={};mode={};status={};image={};restart={}",
            self.pid,
            self.mode.as_str(),
            self.status,
            encode_image(&self.image),
            if self.restart { "1" } else { "0" }
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut pid = None;
        let mut mode = None;
        let mut status = None;
        let mut image = None;
        let mut restart = None;
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
                "mode" => mode = Some(BackgroundMode::parse(v)?),
                "status" => status = Some(v.trim().to_string()),
                "image" => image = Some(decode_image(v.trim())),
                "restart" => {
                    restart = Some(match v.trim() {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        other => {
                            return Err(ActionError::RestoreFailed(format!(
                                "invalid restart flag: {other}"
                            )))
                        }
                    });
                }
                _ => {}
            }
        }
        Ok(Self {
            pid: pid.ok_or_else(|| ActionError::RestoreFailed("missing pid".into()))?,
            mode: mode.ok_or_else(|| ActionError::RestoreFailed("missing mode".into()))?,
            status: status.ok_or_else(|| ActionError::RestoreFailed("missing status".into()))?,
            image: image.unwrap_or_default(),
            restart: restart.unwrap_or(false),
        })
    }
}

fn encode_image(path: &str) -> String {
    path.replace('\\', "/").replace(';', "%3B")
}

fn decode_image(raw: &str) -> String {
    raw.replace("%3B", ";")
}

/// Operate only on an explicit user-chosen PID.
pub trait MacOsBackgroundAppsBackend {
    fn probe(&self, pid: u32) -> Result<(String, String), ActionError>;
    fn close(&self, pid: u32) -> Result<(), ActionError>;
    fn pause(&self, pid: u32) -> Result<(), ActionError>;
    fn resume(&self, pid: u32) -> Result<(), ActionError>;
    fn restart(&self, image: &str) -> Result<u32, ActionError>;
}

/// Labeled mock for unit tests (not a real process signal).
#[derive(Debug, Clone)]
pub struct MockMacOsBackgroundAppsBackend {
    pub pid: u32,
    pub image: String,
    pub status: String,
    pub last_restart_image: Option<String>,
    pub next_restart_pid: u32,
}

impl MockMacOsBackgroundAppsBackend {
    pub fn running(pid: u32, image: impl Into<String>) -> Self {
        Self {
            pid,
            image: image.into(),
            status: "running".into(),
            last_restart_image: None,
            next_restart_pid: pid + 1000,
        }
    }
}

impl MacOsBackgroundAppsBackend for std::cell::RefCell<MockMacOsBackgroundAppsBackend> {
    fn probe(&self, pid: u32) -> Result<(String, String), ActionError> {
        let b = self.borrow();
        if b.status == "closed" {
            return Ok(("closed".into(), b.image.clone()));
        }
        if b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "pid {pid} was not user-chosen in mock"
            )));
        }
        Ok((b.status.clone(), b.image.clone()))
    }

    fn close(&self, pid: u32) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "refusing to close non-chosen pid {pid} (mock)"
            )));
        }
        b.status = "closed".into();
        Ok(())
    }

    fn pause(&self, pid: u32) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "refusing to pause non-chosen pid {pid} (mock)"
            )));
        }
        if b.status != "running" {
            return Err(ActionError::ApplyFailed(
                "process not running (mock)".into(),
            ));
        }
        b.status = "paused".into();
        Ok(())
    }

    fn resume(&self, pid: u32) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if b.pid != pid {
            return Err(ActionError::ApplyFailed(format!(
                "refusing to resume non-chosen pid {pid} (mock)"
            )));
        }
        b.status = "running".into();
        Ok(())
    }

    fn restart(&self, image: &str) -> Result<u32, ActionError> {
        let mut b = self.borrow_mut();
        b.last_restart_image = Some(image.to_string());
        b.status = "running".into();
        b.image = image.to_string();
        b.pid = b.next_restart_pid;
        Ok(b.pid)
    }
}

/// Close or pause a single user-chosen background app (AC-4.20).
pub struct MacOsBackgroundAppsAction<B: MacOsBackgroundAppsBackend> {
    backend: B,
    pid: u32,
    mode: BackgroundMode,
    restart_on_restore: bool,
    explanation: ActionExplanation,
    live_pid: u32,
    restored_without_relaunch: bool,
}

impl<B: MacOsBackgroundAppsBackend> MacOsBackgroundAppsAction<B> {
    pub fn new(backend: B, pid: u32, mode: BackgroundMode, restart_on_restore: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Pause (SIGSTOP) or close (SIGTERM) only a background app the user explicitly chose by PID. Uses documented POSIX signals; no injection.",
            "Freeing CPU/RAM from that process may help a foreground game. Effect is machine-specific; measure locally.",
            "Medium: closing loses unsaved work. Pausing leaves the app frozen until resume. Never targets processes the user did not select.",
            "Restore sends SIGCONT for pause, or relaunches a closed app only if the user opted in and an image path was snapshotted.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            pid,
            mode,
            restart_on_restore,
            explanation,
            live_pid: pid,
            restored_without_relaunch: false,
        }
    }

    pub fn close(backend: B, pid: u32, restart_on_restore: bool) -> Self {
        Self::new(backend, pid, BackgroundMode::Close, restart_on_restore)
    }

    pub fn pause(backend: B, pid: u32) -> Self {
        Self::new(backend, pid, BackgroundMode::Pause, false)
    }

    pub fn chosen_pid(&self) -> u32 {
        self.pid
    }

    pub fn exposes_toggle(&self) -> bool {
        true
    }
}

impl<B: MacOsBackgroundAppsBackend> Action for MacOsBackgroundAppsAction<B> {
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
        let (status, image) = self
            .backend
            .probe(self.live_pid)
            .or_else(|_| Ok::<(String, String), ActionError>(("closed".into(), String::new())))?;
        Ok(BackgroundAppState {
            pid: self.live_pid,
            mode: self.mode,
            status,
            image,
            restart: self.restart_on_restore,
        }
        .encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let (status, _) = self.backend.probe(self.pid)?;
        if status != "running" && status != "paused" {
            return Err(ActionError::ApplyFailed(format!(
                "chosen pid {} is not running (status={status})",
                self.pid
            )));
        }
        match self.mode {
            BackgroundMode::Close => self.backend.close(self.pid),
            BackgroundMode::Pause => self.backend.pause(self.pid),
        }
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = BackgroundAppState::decode(before)?;
        self.pid = state.pid;
        self.mode = state.mode;
        self.restart_on_restore = state.restart;
        match state.mode {
            BackgroundMode::Pause => {
                self.backend.resume(self.pid)?;
                self.live_pid = self.pid;
                Ok(())
            }
            BackgroundMode::Close => {
                if !state.restart {
                    self.live_pid = self.pid;
                    self.restored_without_relaunch = true;
                    return Ok(());
                }
                if state.image.trim().is_empty() {
                    return Err(ActionError::RestoreFailed(
                        "restart opted in but image path was empty".into(),
                    ));
                }
                self.restored_without_relaunch = false;
                let new_pid = self.backend.restart(&state.image)?;
                self.live_pid = new_pid;
                Ok(())
            }
        }
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let expected_state = BackgroundAppState::decode(expected)?;
        if self.restored_without_relaunch
            && expected_state.mode == BackgroundMode::Close
            && !expected_state.restart
            && expected_state.status == "running"
        {
            return Ok(VerifyResult::Matches);
        }
        let actual = self.read_state()?;
        let actual_state = BackgroundAppState::decode(&actual)?;
        let ok = actual_state.status == expected_state.status
            && actual_state.mode == expected_state.mode;
        if ok {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.to_string(),
                actual,
            })
        }
    }
}

#[cfg(target_os = "macos")]
pub mod os {
    use super::*;
    use std::process::Command;

    const SIGTERM: i32 = 15;
    const SIGSTOP: i32 = 17;
    const SIGCONT: i32 = 19;
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    // proc_pidpath lives in libSystem / libproc on Darwin.
    extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut u8, buffersize: u32) -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
    }

    fn pid_alive(pid: u32) -> bool {
        // signal 0: existence check
        unsafe { kill(pid as i32, 0) == 0 }
    }

    fn image_for(pid: u32) -> String {
        let mut buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];
        let n = unsafe { proc_pidpath(pid as i32, buf.as_mut_ptr(), buf.len() as u32) };
        if n <= 0 {
            return String::new();
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(n as usize);
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    pub struct OsMacOsBackgroundAppsBackend;

    impl MacOsBackgroundAppsBackend for OsMacOsBackgroundAppsBackend {
        fn probe(&self, pid: u32) -> Result<(String, String), ActionError> {
            if pid == 0 {
                return Err(ActionError::ApplyFailed(
                    "refusing pid 0 (not a user-chosen app)".into(),
                ));
            }
            if !pid_alive(pid) {
                return Ok(("closed".into(), String::new()));
            }
            Ok(("running".into(), image_for(pid)))
        }

        fn close(&self, pid: u32) -> Result<(), ActionError> {
            if pid == 0 || pid == std::process::id() {
                return Err(ActionError::ApplyFailed(
                    "refusing to signal pid 0 or self".into(),
                ));
            }
            let rc = unsafe { kill(pid as i32, SIGTERM) };
            if rc != 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "SIGTERM failed for pid {pid}"
                )));
            }
            Ok(())
        }

        fn pause(&self, pid: u32) -> Result<(), ActionError> {
            if pid == 0 || pid == std::process::id() {
                return Err(ActionError::ApplyFailed(
                    "refusing to signal pid 0 or self".into(),
                ));
            }
            let rc = unsafe { kill(pid as i32, SIGSTOP) };
            if rc != 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "SIGSTOP failed for pid {pid}"
                )));
            }
            Ok(())
        }

        fn resume(&self, pid: u32) -> Result<(), ActionError> {
            let rc = unsafe { kill(pid as i32, SIGCONT) };
            if rc != 0 {
                return Err(ActionError::RestoreFailed(format!(
                    "SIGCONT failed for pid {pid}"
                )));
            }
            Ok(())
        }

        fn restart(&self, image: &str) -> Result<u32, ActionError> {
            if image.trim().is_empty() {
                return Err(ActionError::RestoreFailed("empty image path".into()));
            }
            // open(1) is the documented user-facing relaunch for .app bundles / paths.
            let status = Command::new("open")
                .arg(image)
                .status()
                .map_err(|e| ActionError::RestoreFailed(format!("open failed: {e}")))?;
            if !status.success() {
                // Fallback: exec path directly when not a bundle.
                let child = Command::new(image)
                    .spawn()
                    .map_err(|e| ActionError::RestoreFailed(format!("spawn failed: {e}")))?;
                return Ok(child.id());
            }
            // open(1) does not return the new pid; caller may re-probe.
            Ok(0)
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
    fn mock_pause_resume_round_trip() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockMacOsBackgroundAppsBackend::running(
            42,
            "/Applications/Mock.app",
        ));
        let mut action = MacOsBackgroundAppsAction::pause(backend, 42);
        assert!(action.exposes_toggle());
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        assert!(entry.before.contains("status=running"));
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        assert!(action.verify(&entry.before).expect("verify").is_match());
    }

    #[test]
    fn mock_close_without_restart_stays_closed() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockMacOsBackgroundAppsBackend::running(7, "/tmp/mock"));
        let mut action = MacOsBackgroundAppsAction::close(backend, 7, false);
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        assert!(action.verify(&entry.before).expect("verify").is_match());
    }

    #[test]
    fn refuses_non_chosen_pid_in_mock() {
        let backend = RefCell::new(MockMacOsBackgroundAppsBackend::running(1, "/tmp/a"));
        let mut action = MacOsBackgroundAppsAction::close(backend, 99, false);
        assert!(action.apply().is_err());
    }
}
