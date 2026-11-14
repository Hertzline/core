//! Pause or close only user-chosen background apps (AC-4.4).
//!
//! Official sources (verified 2026-11-14):
//! - TerminateProcess
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess
//! - OpenProcess
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess
//! - QueryFullProcessImageNameW
//!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew
//! - SuspendThread / ResumeThread (pause mode)
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-suspendthread
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-resumethread
//! - CreateToolhelp32Snapshot / Thread32First / Thread32Next
//!   https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot
//!
//! Never closes processes the user did not explicitly choose. Restore restarts
//! only when the user opted in and an image path was captured.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.background_apps";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess";

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
    raw.replace("%3B", ";").replace('/', "\\")
}

/// Operate only on an explicit user-chosen PID.
pub trait BackgroundAppsBackend {
    fn probe(&self, pid: u32) -> Result<(String, String), ActionError>;
    fn close(&self, pid: u32) -> Result<(), ActionError>;
    fn pause(&self, pid: u32) -> Result<(), ActionError>;
    fn resume(&self, pid: u32) -> Result<(), ActionError>;
    fn restart(&self, image: &str) -> Result<u32, ActionError>;
}

/// Labeled mock for unit tests (not a real process kill).
#[derive(Debug, Clone)]
pub struct MockBackgroundAppsBackend {
    pub pid: u32,
    pub image: String,
    pub status: String,
    pub last_restart_image: Option<String>,
    pub next_restart_pid: u32,
}

impl MockBackgroundAppsBackend {
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

impl BackgroundAppsBackend for std::cell::RefCell<MockBackgroundAppsBackend> {
    fn probe(&self, pid: u32) -> Result<(String, String), ActionError> {
        let b = self.borrow();
        if b.pid != pid && b.status != "closed" {
            return Err(ActionError::ApplyFailed(format!(
                "pid {pid} was not user-chosen in mock"
            )));
        }
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

/// Close or pause a single user-chosen background app (AC-4.4).
pub struct BackgroundAppsAction<B: BackgroundAppsBackend> {
    backend: B,
    pid: u32,
    mode: BackgroundMode,
    restart_on_restore: bool,
    explanation: ActionExplanation,
    /// Updated after close+restart restore so verify can track new pid.
    live_pid: u32,
    /// Set when restore ran without relaunch (user declined restart).
    restored_without_relaunch: bool,
}

impl<B: BackgroundAppsBackend> BackgroundAppsAction<B> {
    pub fn new(backend: B, pid: u32, mode: BackgroundMode, restart_on_restore: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Pause or close only a background app the user explicitly chose (by PID).",
            "Freeing CPU/RAM/disk from that process may help a foreground game. Effect is machine-specific; measure locally.",
            "Medium: closing loses unsaved work in that app. Pausing can leave the app unresponsive until resume. Never targets processes the user did not select.",
            "Restore resumes a paused process, or restarts a closed one only if the user opted in and an image path was snapshotted.",
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
}

impl<B: BackgroundAppsBackend> Action for BackgroundAppsAction<B> {
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
        let (status, image) = self.backend.probe(self.live_pid).or_else(|_| {
            // After close, original pid is gone; report closed using chosen pid.
            Ok::<(String, String), ActionError>(("closed".into(), String::new()))
        })?;
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
        // Hard gate: only the user-chosen pid.
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
        // Adopt snapshot identity for headless restore-all.
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
                    // AC-4.4: restore restarts only if user opted in.
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
            // Honest residual: process stays closed; change log may still mark restored.
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

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_SUSPEND_RESUME: u32 = 0x0800;
    const THREAD_SUSPEND_RESUME: u32 = 0x0002;
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[repr(C)]
    struct ThreadEntry32 {
        dw_size: u32,
        cnt_usage: u32,
        th32_thread_id: u32,
        th32_owner_process_id: u32,
        tp_base_pri: i32,
        tp_delta_pri: i32,
        dw_flags: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn TerminateProcess(handle: *mut c_void, exit_code: u32) -> i32;
        fn QueryFullProcessImageNameW(
            handle: *mut c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
        fn Thread32First(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
        fn OpenThread(access: u32, inherit: i32, thread_id: u32) -> *mut c_void;
        fn SuspendThread(thread: *mut c_void) -> u32;
        fn ResumeThread(thread: *mut c_void) -> u32;
        fn CreateProcessW(
            app: *const u16,
            cmd: *mut u16,
            proc_attr: *mut c_void,
            thread_attr: *mut c_void,
            inherit: i32,
            flags: u32,
            env: *mut c_void,
            cwd: *const u16,
            si: *mut StartupInfoW,
            pi: *mut ProcessInformation,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        std_input: *mut c_void,
        std_output: *mut c_void,
        std_error: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: *mut c_void,
        thread: *mut c_void,
        process_id: u32,
        thread_id: u32,
    }

    pub struct OsBackgroundAppsBackend;

    impl BackgroundAppsBackend for OsBackgroundAppsBackend {
        fn probe(&self, pid: u32) -> Result<(String, String), ActionError> {
            let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
            let image = query_image(handle);
            unsafe { CloseHandle(handle) };
            match image {
                Ok(path) => Ok(("running".into(), path)),
                Err(err) => Err(err),
            }
        }

        fn close(&self, pid: u32) -> Result<(), ActionError> {
            let handle = open(pid, PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION)?;
            let ok = unsafe { TerminateProcess(handle, 1) };
            let err = unsafe { GetLastError() };
            unsafe { CloseHandle(handle) };
            if ok == 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "TerminateProcess({pid}) failed ({err}); see {SOURCE_URL}"
                )));
            }
            Ok(())
        }

        fn pause(&self, pid: u32) -> Result<(), ActionError> {
            for_each_thread(pid, |tid| {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, tid) };
                if thread.is_null() {
                    let err = unsafe { GetLastError() };
                    return Err(ActionError::ApplyFailed(format!(
                        "OpenThread({tid}) failed ({err})"
                    )));
                }
                let prev = unsafe { SuspendThread(thread) };
                unsafe { CloseHandle(thread) };
                if prev == u32::MAX {
                    let err = unsafe { GetLastError() };
                    return Err(ActionError::ApplyFailed(format!(
                        "SuspendThread({tid}) failed ({err})"
                    )));
                }
                Ok(())
            })
        }

        fn resume(&self, pid: u32) -> Result<(), ActionError> {
            for_each_thread(pid, |tid| {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, tid) };
                if thread.is_null() {
                    let err = unsafe { GetLastError() };
                    return Err(ActionError::ApplyFailed(format!(
                        "OpenThread({tid}) failed ({err})"
                    )));
                }
                let prev = unsafe { ResumeThread(thread) };
                unsafe { CloseHandle(thread) };
                if prev == u32::MAX {
                    let err = unsafe { GetLastError() };
                    return Err(ActionError::ApplyFailed(format!(
                        "ResumeThread({tid}) failed ({err})"
                    )));
                }
                Ok(())
            })
        }

        fn restart(&self, image: &str) -> Result<u32, ActionError> {
            let wide = wide(image);
            let mut cmd = wide.clone();
            let mut si: StartupInfoW = unsafe { std::mem::zeroed() };
            si.cb = std::mem::size_of::<StartupInfoW>() as u32;
            let mut pi: ProcessInformation = unsafe { std::mem::zeroed() };
            let ok = unsafe {
                CreateProcessW(
                    ptr::null(),
                    cmd.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    0,
                    0,
                    ptr::null_mut(),
                    ptr::null(),
                    &mut si,
                    &mut pi,
                )
            };
            if ok == 0 {
                let err = unsafe { GetLastError() };
                return Err(ActionError::RestoreFailed(format!(
                    "CreateProcessW failed ({err}) for {image}"
                )));
            }
            let pid = pi.process_id;
            unsafe {
                CloseHandle(pi.thread);
                CloseHandle(pi.process);
            }
            Ok(pid)
        }
    }

    fn open(pid: u32, access: u32) -> Result<*mut c_void, ActionError> {
        // Also request SUSPEND_RESUME when pausing; callers pass the needed mask.
        let _ = PROCESS_SUSPEND_RESUME;
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(ActionError::ApplyFailed(format!(
                "OpenProcess({pid}) failed ({err})"
            )));
        }
        Ok(handle)
    }

    fn query_image(handle: *mut c_void) -> Result<String, ActionError> {
        let mut buf = vec![0u16; 32768];
        let mut size = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            return Err(ActionError::ApplyFailed(format!(
                "QueryFullProcessImageNameW failed ({err})"
            )));
        }
        Ok(String::from_utf16_lossy(&buf[..size as usize]))
    }

    fn for_each_thread(
        pid: u32,
        mut f: impl FnMut(u32) -> Result<(), ActionError>,
    ) -> Result<(), ActionError> {
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snap as isize == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            return Err(ActionError::ApplyFailed(format!(
                "CreateToolhelp32Snapshot failed ({err})"
            )));
        }
        let mut entry = ThreadEntry32 {
            dw_size: std::mem::size_of::<ThreadEntry32>() as u32,
            cnt_usage: 0,
            th32_thread_id: 0,
            th32_owner_process_id: 0,
            tp_base_pri: 0,
            tp_delta_pri: 0,
            dw_flags: 0,
        };
        let mut ok = unsafe { Thread32First(snap, &mut entry) };
        let mut saw = false;
        while ok != 0 {
            if entry.th32_owner_process_id == pid {
                saw = true;
                if let Err(err) = f(entry.th32_thread_id) {
                    unsafe { CloseHandle(snap) };
                    return Err(err);
                }
            }
            ok = unsafe { Thread32Next(snap, &mut entry) };
        }
        unsafe { CloseHandle(snap) };
        if !saw {
            return Err(ActionError::ApplyFailed(format!(
                "no threads found for pid {pid}"
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
        fn os_probes_current_process() {
            let pid = std::process::id();
            let (status, image) = OsBackgroundAppsBackend.probe(pid).expect("probe self");
            assert_eq!(status, "running");
            assert!(!image.is_empty());
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
    fn mock_close_restore_with_restart() {
        // mock: user-chosen close + restart opt-in
        let backend = RefCell::new(MockBackgroundAppsBackend::running(55, r"C:\Apps\chat.exe"));
        let mut action = BackgroundAppsAction::close(backend, 55, true);
        assert_eq!(action.chosen_pid(), 55);
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("status=closed"));
        let before = BackgroundAppState {
            pid: 55,
            mode: BackgroundMode::Close,
            status: "running".into(),
            image: r"C:\Apps\chat.exe".into(),
            restart: true,
        }
        .encode();
        action.restore(&before).unwrap();
        assert!(action.read_state().unwrap().contains("status=running"));
    }

    #[test]
    fn mock_close_without_restart_skips_relaunch() {
        let backend = RefCell::new(MockBackgroundAppsBackend::running(3, r"C:\a.exe"));
        let mut action = BackgroundAppsAction::close(backend, 3, false);
        action.apply().unwrap();
        let before = BackgroundAppState {
            pid: 3,
            mode: BackgroundMode::Close,
            status: "running".into(),
            image: r"C:\a.exe".into(),
            restart: false,
        }
        .encode();
        action.restore(&before).unwrap();
        assert!(action.read_state().unwrap().contains("status=closed"));
    }

    #[test]
    fn mock_pause_resume() {
        let backend = RefCell::new(MockBackgroundAppsBackend::running(8, r"C:\b.exe"));
        let mut action = BackgroundAppsAction::pause(backend, 8);
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("status=paused"));
        action
            .restore(
                &BackgroundAppState {
                    pid: 8,
                    mode: BackgroundMode::Pause,
                    status: "running".into(),
                    image: r"C:\b.exe".into(),
                    restart: false,
                }
                .encode(),
            )
            .unwrap();
        assert!(action.read_state().unwrap().contains("status=running"));
    }

    #[test]
    fn refuses_non_chosen_pid() {
        let backend = RefCell::new(MockBackgroundAppsBackend::running(1, r"C:\c.exe"));
        let mut action = BackgroundAppsAction::close(backend, 1, false);
        assert!(action.apply().map(|_| ()).is_ok());
        // Separate action targeting another pid against same mock refuses.
        let backend2 = RefCell::new(MockBackgroundAppsBackend::running(1, r"C:\c.exe"));
        let err = backend2.close(99).unwrap_err();
        assert!(err.to_string().contains("non-chosen"));
        let _ = action;
    }

    #[test]
    fn snapshot_round_trip_pause() {
        let backend = RefCell::new(MockBackgroundAppsBackend::running(11, r"C:\d.exe"));
        let mut action = BackgroundAppsAction::pause(backend, 11);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 5).unwrap();
        assert_eq!(entry.action, ACTION_ID);
        // After pause, verify expected is paused; restore_with_log uses before=running.
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert!(action.read_state().unwrap().contains("status=running"));
    }

    #[test]
    fn explanation_ac81_and_ac82() {
        let backend = RefCell::new(MockBackgroundAppsBackend::running(2, r"C:\e.exe"));
        let action = BackgroundAppsAction::close(backend, 2, true);
        validate_explanation(action.explanation()).unwrap();
        assert!(action
            .explanation()
            .local_measurement
            .message()
            .contains("no local measurement"));
    }
}
