//! macOS capability surface + monitoring (HZ-025 / AC-4.18, AC-4.23, AC-1.7).
//!
//! No fake toggles. Unsupported metrics and actions are labeled not possible.
//! Mutating macOS actions (Game Mode status, background apps, LPM, refresh)
//! ship in HZ-026.

pub mod capability;
pub mod monitor;

pub use capability::{
    macos_capability_matrix, MacOsCapability, MacOsCapabilityKind, MacOsCapabilitySurface,
    MacOsLevel, MACOS_HONESTY_BANNER,
};
pub use monitor::{
    snapshot_from_host_probes, MacOsMonitorAction, MockMonitorBackend, MonitorBackend,
    MonitorSnapshot, MonitoredMetric, ACTION_ID, SOURCE_CVDISPLAYLINK, SOURCE_SYSINFO_GPU,
};

/// True when this host is macOS (runtime monitoring path).
pub fn is_macos_host() -> bool {
    cfg!(target_os = "macos")
}
