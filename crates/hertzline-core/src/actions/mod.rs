//! OS optimization actions (HZ-020+).
//!
//! Each action implements [`crate::action::Action`] (apply / restore / verify)
//! with AC-8.1 explanations. Windows: power, Game Mode, HAGS, priority,
//! background apps, startup apps, Game Bar, cleanup, storage, drivers, display.

pub mod windows;

pub use windows::{
    BackgroundAppsAction, CleanupAction, DisplayRefreshAction, DriverCheckAction, GameBarAction,
    GameModeAction, HagsAction, PowerPlanAction, ProcessPriorityAction, StartupAppsAction,
    StorageHealthAction, AMD_DRIVERS_URL, GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_POWER_SAVER,
    HAGS_OFF, HAGS_ON, HIGH_PRIORITY_CLASS, INTEL_DRIVERS_URL, NORMAL_PRIORITY_CLASS,
    NVIDIA_DRIVERS_URL,
};
