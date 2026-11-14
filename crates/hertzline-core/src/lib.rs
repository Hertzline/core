//! Hertzline core library (measurement, export; spikes D1 / D3 / D4).
//!
//! No UI. No permanent elevation. Frame pipeline: HZ-011. Collectors: HZ-012.
//! Benchmark engine: HZ-014. Noise-band verdict: HZ-015.
//! Snapshot store + change log: HZ-016. Action trait: HZ-017.
//! Restore-all / crash / uninstall: HZ-018.
//! Windows System Restore offer gate: HZ-019.
//! Windows power / Game Mode / HAGS: HZ-020.
//! Windows priority / background / startup: HZ-021.
//! Windows Game Bar / cleanup / storage / drivers / display: HZ-022.
//! Linux GameMode / energy profile / nice+ionice: HZ-023.
//! Linux compositor / MangoHud / Proton info / shader cache: HZ-024.
//! macOS monitoring + honest capability surface: HZ-025.
//! macOS Game Mode status / background apps / LPM / refresh: HZ-026.
//! Game detection from documented paths: HZ-027.
//! Profile apply on start / restore on exit: HZ-028.

pub mod action;
pub mod actions;
pub mod benchmark;
pub mod collectors;
pub mod frame_capture;
pub mod games;
pub mod ipc;
pub mod paths;
pub mod profiles;
pub mod restore;
pub mod sensors;
pub mod session;
pub mod snapshot;
pub mod system_restore;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
