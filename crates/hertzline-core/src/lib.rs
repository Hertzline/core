//! Hertzline core library (measurement, export; spikes D1 / D3 / D4).
//!
//! No UI. No permanent elevation. Collectors and actions land in later tasks.

pub mod frame_capture;
pub mod ipc;
pub mod sensors;
pub mod session;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
