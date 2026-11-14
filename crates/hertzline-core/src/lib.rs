//! Hertzline core library (measurement, export; spikes D1 / D3 / D4).
//!
//! No UI. No permanent elevation. Frame pipeline: HZ-011. Collectors: HZ-012.
//! Benchmark engine: HZ-014. Noise-band verdict: HZ-015.
//! Snapshot store + change log: HZ-016. Action trait: HZ-017.
//! Restore-all / crash / uninstall: HZ-018.

pub mod action;
pub mod benchmark;
pub mod collectors;
pub mod frame_capture;
pub mod ipc;
pub mod paths;
pub mod restore;
pub mod sensors;
pub mod session;
pub mod snapshot;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
