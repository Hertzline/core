//! Hertzline core library (spike skeleton for D1 / D3).
//!
//! No UI. No permanent elevation. Measurement and actions land in later tasks.

pub mod frame_capture;
pub mod ipc;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
