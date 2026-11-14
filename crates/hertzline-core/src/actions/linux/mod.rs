//! Linux optimization actions (HZ-023 / AC-4.11--AC-4.13, AC-8.1, AC-8.2).
//!
//! Official sources (verified 2026-11-14):
//! - Feral GameMode D-Bus: com.feralinteractive.GameMode
//!   RegisterGame / UnregisterGame / QueryStatus (i -> i)
//!   https://github.com/FeralInteractive/gamemode
//! - power-profiles-daemon ActiveProfile (power-saver | balanced | performance)
//!   https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html
//! - tuned-adm (fallback when ppd missing): distribution tuned package
//! - setpriority(2) / getpriority(2): https://man7.org/linux/man-pages/man2/setpriority.2.html
//! - ionice(1) / ioprio: https://man7.org/linux/man-pages/man1/ionice.1.html
//!
//! Non-Linux: actions report [`crate::action::ActionError::NotPossible`].

pub mod gamemode;
pub mod governor;
pub mod nice_ionice;

pub use gamemode::{FeralGameModeAction, FeralGameModeBackend, MockFeralGameModeBackend};
pub use governor::{
    EnergyProfileAction, EnergyProfileBackend, MockEnergyProfileBackend, PROFILE_BALANCED,
    PROFILE_PERFORMANCE, PROFILE_POWER_SAVER,
};
pub use nice_ionice::{
    MockNiceIoniceBackend, NiceIoniceAction, NiceIoniceBackend, NiceIoniceState, IOPRIO_CLASS_BE,
    IOPRIO_CLASS_IDLE, IOPRIO_CLASS_NONE, IOPRIO_CLASS_RT,
};

/// True when this host is Linux (runtime actions apply).
pub fn is_linux_host() -> bool {
    cfg!(target_os = "linux")
}
