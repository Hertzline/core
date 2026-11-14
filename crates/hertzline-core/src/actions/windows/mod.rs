//! Windows optimization actions (HZ-020 / AC-4.1, AC-4.2, AC-8.1, AC-8.2).
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
//!
//! Non-Windows: actions report [`crate::action::ActionError::NotPossible`].

pub mod game_mode;
pub mod hags;
pub mod power_plan;

pub use game_mode::{GameModeAction, GameModeBackend, MockGameModeBackend};
pub use hags::{HagsAction, HagsBackend, MockHagsBackend, HAGS_OFF, HAGS_ON};
pub use power_plan::{
    MockPowerPlanBackend, PowerPlanAction, PowerPlanBackend, GUID_BALANCED, GUID_HIGH_PERFORMANCE,
    GUID_POWER_SAVER,
};

/// True when this host is Windows (runtime actions apply).
pub fn is_windows_host() -> bool {
    cfg!(windows)
}
