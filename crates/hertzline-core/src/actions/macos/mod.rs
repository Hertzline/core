//! macOS capability surface, monitoring, and partial actions (HZ-025 / HZ-026).
//!
//! HZ-025: honest capability matrix + supported-metrics monitoring.
//! HZ-026: Game Mode status, background apps, Low Power Mode warnings, refresh.

pub mod background;
pub mod capability;
pub mod game_mode;
pub mod low_power;
pub mod monitor;
pub mod refresh;

pub use background::{
    BackgroundAppState, BackgroundMode, MacOsBackgroundAppsAction, MacOsBackgroundAppsBackend,
    MockMacOsBackgroundAppsBackend,
};
pub use capability::{
    macos_capability_matrix, MacOsCapability, MacOsCapabilityKind, MacOsCapabilitySurface,
    MacOsLevel, MACOS_HONESTY_BANNER,
};
pub use game_mode::{
    MacOsGameModeBackend, MacOsGameModeReport, MacOsGameModeStatusAction, MockMacOsGameModeBackend,
    DETAIL_NO_PUBLIC_API, STATUS_NOT_POSSIBLE,
};
pub use low_power::{
    parse_pmset_low_power, LowPowerBackend, LowPowerModeWarningAction, LowPowerState,
    MockLowPowerBackend, WARNING_PERF_WHILE_LPM,
};
pub use monitor::{
    snapshot_from_host_probes, MacOsMonitorAction, MockMonitorBackend, MonitorBackend,
    MonitorSnapshot, MonitoredMetric, ACTION_ID as MONITOR_ACTION_ID, SOURCE_CVDISPLAYLINK,
    SOURCE_SYSINFO_GPU,
};
pub use refresh::{
    MacOsDisplayState, MacOsRefreshAction, MacOsRefreshBackend, MockMacOsRefreshBackend, VRR_NOTE,
};

/// True when this host is macOS (runtime monitoring path).
pub fn is_macos_host() -> bool {
    cfg!(target_os = "macos")
}
