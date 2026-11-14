//! Windows optimization actions (HZ-020--HZ-022 / AC-4.1--AC-4.10, AC-8.1, AC-8.2).
//!
//! Official sources (verified 2026-11-14):
//! - Power: PowerGetActiveScheme / PowerSetActiveScheme / PowerEnumerate
//!   https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powergetactivescheme
//!   https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powersetactivescheme
//!   https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powerenumerate
//! - Scheme GUIDs (WinNT / power setting personalities):
//!   https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids
//! - Game Mode: HKCU AutoGameModeEnabled (Microsoft settings reference)
//!   https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11
//! - HAGS: Hardware-accelerated GPU scheduling (DirectX blog + HwSchMode)
//!   https://devblogs.microsoft.com/directx/hardware-accelerated-gpu-scheduling/
//! - Priority / affinity: SetPriorityClass / SetProcessAffinityMask
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass
//!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setprocessaffinitymask
//! - Background apps: TerminateProcess / SuspendThread (user-chosen PID only)
//!   https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess
//! - Startup apps: Run keys + StartupApproved (compatibility cookbook)
//!   https://learn.microsoft.com/en-us/windows/compatibility/startup-apps
//! - Game Bar capture: HKCU GameDVR VKMToggleRecording (settings reference)
//!   https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11
//! - Temp path: GetTempPathW
//!   https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw
//! - Free space: GetDiskFreeSpaceExW
//!   https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw
//! - Display refresh: EnumDisplaySettingsW / ChangeDisplaySettingsExW
//!   https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changedisplaysettingsexw
//!
//! Non-Windows: actions report [`crate::action::ActionError::NotPossible`].

pub mod background;
pub mod cleanup;
pub mod display;
pub mod drivers;
pub mod game_bar;
pub mod game_mode;
pub mod hags;
pub mod power_plan;
pub mod priority;
pub mod startup;
pub mod storage;

pub use background::{
    BackgroundAppsAction, BackgroundAppsBackend, BackgroundMode, MockBackgroundAppsBackend,
};
pub use cleanup::{CleanupAction, CleanupBackend, CleanupPreview, MockCleanupBackend};
pub use display::{DisplayBackend, DisplayRefreshAction, DisplayState, MockDisplayBackend};
pub use drivers::{
    DriverCheckAction, DriverInfo, DriversBackend, GpuVendor, MockDriversBackend, AMD_DRIVERS_URL,
    INTEL_DRIVERS_URL, NVIDIA_DRIVERS_URL,
};
pub use game_bar::{GameBarAction, GameBarBackend, MockGameBarBackend};
pub use game_mode::{GameModeAction, GameModeBackend, MockGameModeBackend};
pub use hags::{HagsAction, HagsBackend, MockHagsBackend, HAGS_OFF, HAGS_ON};
pub use power_plan::{
    MockPowerPlanBackend, PowerPlanAction, PowerPlanBackend, GUID_BALANCED, GUID_HIGH_PERFORMANCE,
    GUID_POWER_SAVER,
};
pub use priority::{
    MockProcessPriorityBackend, ProcessPriorityAction, ProcessPriorityBackend, HIGH_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS,
};
pub use startup::{MockStartupAppsBackend, StartupAppsAction, StartupAppsBackend, StartupEntry};
pub use storage::{MockStorageBackend, StorageBackend, StorageHealthAction, VolumeSpace};

/// True when this host is Windows (runtime actions apply).
pub fn is_windows_host() -> bool {
    cfg!(windows)
}
