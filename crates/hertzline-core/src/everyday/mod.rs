//! Everyday mode (HZ-033 / AC-5.1--AC-5.6).
//!
//! Startup impact review, background resource hogs (no silent kills), laptop
//! power presets, disk cleanup preview, thermal warnings, and weekly health
//! report. Mutating steps reuse the Action trait + snapshot store.

mod cleanup;
mod hogs;
mod preset;
mod startup;
mod thermal;
mod weekly;

pub use cleanup::{
    apply_everyday_cleanup, preview_everyday_cleanup, CleanupPath, EverydayCleanupPreview,
    HostEverydayCleanupBackend, MockEverydayCleanupBackend,
};
pub use hogs::{
    list_resource_hogs, refuse_kill_without_confirm, MockResourceHogsBackend, ResourceHog,
    ResourceHogReport, ResourceHogsBackend, SysinfoResourceHogs,
};
pub use preset::{
    EverydayPreset, HostPowerPresetBackend, MockPowerPresetBackend, PowerPresetAction,
    PowerPresetBackend, PowerPresetState, ACTION_ID as POWER_PRESET_ACTION_ID, PRESET_BALANCED,
    PRESET_BATTERY, PRESET_PERFORMANCE,
};
pub use startup::{
    estimate_startup_impact, list_startup_impact, StartupImpactKind, StartupImpactReport,
    StartupImpactRow, StartupImpactSource,
};
pub use thermal::{
    assess_thermal, ThermalLevel, ThermalReport, ThermalWarning, HIGH_TEMP_CELSIUS, THROTTLE_HINTS,
};
pub use weekly::{
    append_measurement_summary, generate_weekly_report, load_measurement_archive,
    MeasurementArchive, MeasurementSummary, WeeklyHealthReport, WEEK_MS,
};
