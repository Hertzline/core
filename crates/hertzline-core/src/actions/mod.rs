//! OS optimization actions (HZ-020+).
//!
//! Each action implements [`crate::action::Action`] (apply / restore / verify)
//! with AC-8.1 explanations. Windows first: power plan, Game Mode, HAGS.

pub mod windows;

pub use windows::{
    GameModeAction, HagsAction, PowerPlanAction, GUID_BALANCED, GUID_HIGH_PERFORMANCE,
    GUID_POWER_SAVER, HAGS_OFF, HAGS_ON,
};
