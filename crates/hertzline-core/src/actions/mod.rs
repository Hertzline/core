//! OS optimization actions (HZ-020+).
//!
//! Each action implements [`crate::action::Action`] (apply / restore / verify)
//! with AC-8.1 explanations. Windows: power, Game Mode, HAGS, priority,
//! background apps, startup apps.

pub mod windows;

pub use windows::{
    BackgroundAppsAction, GameModeAction, HagsAction, PowerPlanAction, ProcessPriorityAction,
    StartupAppsAction, GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_POWER_SAVER, HAGS_OFF, HAGS_ON,
    HIGH_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
};
