//! Hertzline core library (measurement, export; spikes D1 / D3 / D4).
//!
//! No UI. No permanent elevation. Frame pipeline: HZ-011. Collectors: HZ-012.

pub mod collectors;
pub mod frame_capture;
pub mod ipc;
pub mod sensors;
pub mod session;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
