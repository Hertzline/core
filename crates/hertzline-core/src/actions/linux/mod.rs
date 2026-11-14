//! Linux optimization actions (HZ-023 / AC-4.11--AC-4.13, HZ-024 / AC-4.14--AC-4.17, AC-8.1, AC-8.2).
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
//! - KWin `WindowsBlockCompositing` (kwin.kcfg Compositing group)
//! - gamescope `GAMESCOPE_WSI_FORCE_BYPASS` (ValveSoftware/gamescope WSI layer)
//! - MangoHud `MANGOHUD=1` / mangohud wrap (mangohud(1))
//! - Steam compatibilitytools.d + steamapps/shadercache (Valve-documented layout)
//!
//! Non-Linux: actions report [`crate::action::ActionError::NotPossible`] via backends.

pub mod compositor;
pub mod gamemode;
pub mod governor;
pub mod mangohud;
pub mod nice_ionice;
pub mod proton;
pub mod shader_cache;

pub use compositor::{
    classify_desktop, CompositorHintAction, CompositorHintState, CompositorSession,
    MockCompositorBackend, ENV_GAMESCOPE_FORCE_BYPASS,
};
pub use gamemode::{FeralGameModeAction, FeralGameModeBackend, MockFeralGameModeBackend};
pub use governor::{
    EnergyProfileAction, EnergyProfileBackend, MockEnergyProfileBackend, PROFILE_BALANCED,
    PROFILE_PERFORMANCE, PROFILE_POWER_SAVER,
};
pub use mangohud::{MangoHudAction, MangoHudBackend, MockMangoHudBackend, ENV_MANGOHUD};
pub use nice_ionice::{
    MockNiceIoniceBackend, NiceIoniceAction, NiceIoniceBackend, NiceIoniceState, IOPRIO_CLASS_BE,
    IOPRIO_CLASS_IDLE, IOPRIO_CLASS_NONE, IOPRIO_CLASS_RT,
};
pub use proton::{
    parse_vdf_display_name, MockProtonInfoBackend, ProtonInfoAction, ProtonInfoBackend,
    ProtonToolInfo,
};
pub use shader_cache::{
    MockShaderCacheBackend, ShaderCacheAction, ShaderCacheBackend, ShaderCacheEntry,
    ShaderCachePreview,
};

/// True when this host is Linux (runtime actions apply).
pub fn is_linux_host() -> bool {
    cfg!(target_os = "linux")
}
