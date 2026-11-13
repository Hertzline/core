//! Measurement session model and CSV/JSON export (HZ-010 / AC-1.8).
//!
//! Frame pipeline (HZ-011) and collectors (HZ-012) fill the session;
//! this module owns the canonical field list so CSV and JSON share names/units.

mod export;
mod model;

pub use export::{export_csv, export_json, parse_csv, parse_json, ExportDocument, ExportRow};
pub use model::{
    DiskSummary, FrameSummary, HardwareSummary, MeasurementSession, MetricF64, NetworkSummary,
    SessionMeta, TextMetric,
};
