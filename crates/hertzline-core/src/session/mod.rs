//! Measurement session model and CSV/JSON export (HZ-010 / AC-1.8).
//!
//! Collectors (HZ-011, HZ-012) fill the session; this module owns the
//! canonical field list so CSV and JSON always share names and units.

mod export;
mod model;

pub use export::{ExportDocument, ExportRow, export_csv, export_json, parse_csv, parse_json};
pub use model::{
    DiskSummary, FrameSummary, HardwareSummary, MeasurementSession, MetricF64, NetworkSummary,
    SessionMeta, TextMetric,
};
