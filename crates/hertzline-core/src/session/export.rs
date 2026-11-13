//! CSV and JSON export share one ordered field list (AC-1.8).

use serde::{Deserialize, Serialize};

use super::model::{MeasurementSession, MetricF64, TextMetric, SESSION_SCHEMA_VERSION};

/// One export cell: name + unit + optional value / unavailable reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportRow {
    pub name: String,
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// Document wrapper for JSON export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportDocument {
    pub schema_version: u32,
    pub format: String,
    pub fields: Vec<ExportRow>,
}

fn push_meta_text(rows: &mut Vec<ExportRow>, name: &str, value: &str) {
    rows.push(ExportRow {
        name: name.to_string(),
        unit: "".to_string(),
        value: Some(value.to_string()),
        unavailable_reason: None,
    });
}

fn push_meta_i64(rows: &mut Vec<ExportRow>, name: &str, unit: &str, value: i64) {
    rows.push(ExportRow {
        name: name.to_string(),
        unit: unit.to_string(),
        value: Some(value.to_string()),
        unavailable_reason: None,
    });
}

fn push_f64(rows: &mut Vec<ExportRow>, name: &str, metric: &MetricF64) {
    match metric {
        MetricF64::Available { value, unit } => {
            rows.push(ExportRow {
                name: name.to_string(),
                unit: unit.clone(),
                value: Some(format_number(*value)),
                unavailable_reason: None,
            });
        }
        MetricF64::Unavailable { reason } => {
            rows.push(ExportRow {
                name: name.to_string(),
                unit: unit_for_unavailable_f64(name).to_string(),
                value: None,
                unavailable_reason: Some(reason.clone()),
            });
        }
    }
}

fn push_text(rows: &mut Vec<ExportRow>, name: &str, metric: &TextMetric) {
    match metric {
        TextMetric::Available { value } => {
            rows.push(ExportRow {
                name: name.to_string(),
                unit: "".to_string(),
                value: Some(value.clone()),
                unavailable_reason: None,
            });
        }
        TextMetric::Unavailable { reason } => {
            rows.push(ExportRow {
                name: name.to_string(),
                unit: "".to_string(),
                value: None,
                unavailable_reason: Some(reason.clone()),
            });
        }
    }
}

fn unit_for_unavailable_f64(name: &str) -> &'static str {
    match name {
        "sample_count" => "count",
        "avg_fps" | "low_1pct_fps" | "low_0_1pct_fps" => "fps",
        "frame_time_variance_ms2" => "ms^2",
        "avg_frame_time_ms" | "network_latency_ms" | "network_jitter_ms" | "duration_ms" => "ms",
        "cpu_usage_percent" | "gpu_usage_percent" | "network_packet_loss_percent" => "percent",
        "ram_used_bytes" | "ram_total_bytes" | "vram_used_bytes" | "vram_total_bytes" => "bytes",
        "cpu_temp_celsius" | "gpu_temp_celsius" => "celsius",
        "cpu_clock_mhz" | "gpu_clock_mhz" => "mhz",
        "disk_read_bytes_per_sec" | "disk_write_bytes_per_sec" => "bytes_per_sec",
        "disk_queue_depth" => "count",
        _ => "",
    }
}

fn format_number(value: f64) -> String {
    // Stable round-trip friendly decimal; avoid scientific for typical ranges.
    let s = format!("{value:.12}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Canonical ordered export rows for a session (CSV and JSON both use this).
pub fn export_rows(session: &MeasurementSession) -> Vec<ExportRow> {
    let mut rows = Vec::with_capacity(32);

    push_meta_i64(
        &mut rows,
        "schema_version",
        "count",
        i64::from(session.schema_version),
    );
    push_meta_text(&mut rows, "session_id", &session.meta.session_id);
    push_meta_i64(
        &mut rows,
        "started_at_unix_ms",
        "ms",
        session.meta.started_at_unix_ms,
    );
    match session.meta.ended_at_unix_ms {
        Some(end) => push_meta_i64(&mut rows, "ended_at_unix_ms", "ms", end),
        None => rows.push(ExportRow {
            name: "ended_at_unix_ms".to_string(),
            unit: "ms".to_string(),
            value: None,
            unavailable_reason: Some("session not completed".to_string()),
        }),
    }
    match session.duration_ms() {
        Some(d) => push_meta_i64(&mut rows, "duration_ms", "ms", d),
        None => rows.push(ExportRow {
            name: "duration_ms".to_string(),
            unit: "ms".to_string(),
            value: None,
            unavailable_reason: Some("session not completed".to_string()),
        }),
    }
    match &session.meta.target_process {
        Some(p) => push_meta_text(&mut rows, "target_process", p),
        None => rows.push(ExportRow {
            name: "target_process".to_string(),
            unit: "".to_string(),
            value: None,
            unavailable_reason: Some("not set".to_string()),
        }),
    }
    push_meta_text(&mut rows, "os", &session.meta.os);
    match &session.meta.notes {
        Some(n) => push_meta_text(&mut rows, "notes", n),
        None => rows.push(ExportRow {
            name: "notes".to_string(),
            unit: "".to_string(),
            value: None,
            unavailable_reason: Some("not set".to_string()),
        }),
    }

    push_f64(&mut rows, "sample_count", &session.frame.sample_count);
    push_f64(&mut rows, "avg_fps", &session.frame.avg_fps);
    push_f64(&mut rows, "low_1pct_fps", &session.frame.low_1pct_fps);
    push_f64(&mut rows, "low_0_1pct_fps", &session.frame.low_0_1pct_fps);
    push_f64(
        &mut rows,
        "frame_time_variance_ms2",
        &session.frame.frame_time_variance_ms2,
    );
    push_f64(
        &mut rows,
        "avg_frame_time_ms",
        &session.frame.avg_frame_time_ms,
    );

    push_f64(
        &mut rows,
        "cpu_usage_percent",
        &session.hardware.cpu_usage_percent,
    );
    push_f64(
        &mut rows,
        "gpu_usage_percent",
        &session.hardware.gpu_usage_percent,
    );
    push_f64(
        &mut rows,
        "ram_used_bytes",
        &session.hardware.ram_used_bytes,
    );
    push_f64(
        &mut rows,
        "ram_total_bytes",
        &session.hardware.ram_total_bytes,
    );
    push_f64(
        &mut rows,
        "vram_used_bytes",
        &session.hardware.vram_used_bytes,
    );
    push_f64(
        &mut rows,
        "vram_total_bytes",
        &session.hardware.vram_total_bytes,
    );
    push_f64(
        &mut rows,
        "cpu_temp_celsius",
        &session.hardware.cpu_temp_celsius,
    );
    push_f64(
        &mut rows,
        "gpu_temp_celsius",
        &session.hardware.gpu_temp_celsius,
    );
    push_f64(&mut rows, "cpu_clock_mhz", &session.hardware.cpu_clock_mhz);
    push_f64(&mut rows, "gpu_clock_mhz", &session.hardware.gpu_clock_mhz);
    push_text(&mut rows, "throttling", &session.hardware.throttling);

    push_f64(
        &mut rows,
        "disk_read_bytes_per_sec",
        &session.disk.disk_read_bytes_per_sec,
    );
    push_f64(
        &mut rows,
        "disk_write_bytes_per_sec",
        &session.disk.disk_write_bytes_per_sec,
    );
    push_f64(
        &mut rows,
        "disk_queue_depth",
        &session.disk.disk_queue_depth,
    );

    push_text(&mut rows, "network_host", &session.network.network_host);
    push_f64(
        &mut rows,
        "network_latency_ms",
        &session.network.network_latency_ms,
    );
    push_f64(
        &mut rows,
        "network_jitter_ms",
        &session.network.network_jitter_ms,
    );
    push_f64(
        &mut rows,
        "network_packet_loss_percent",
        &session.network.network_packet_loss_percent,
    );

    rows
}

/// JSON export (same fields/units as CSV).
pub fn export_json(session: &MeasurementSession) -> Result<String, serde_json::Error> {
    let doc = ExportDocument {
        schema_version: SESSION_SCHEMA_VERSION,
        format: "hertzline-session-fields".to_string(),
        fields: export_rows(session),
    };
    serde_json::to_string_pretty(&doc)
}

/// Parse a JSON export document back to rows.
pub fn parse_json(text: &str) -> Result<ExportDocument, serde_json::Error> {
    serde_json::from_str(text)
}

/// CSV export (header + one row per field; same fields/units as JSON).
pub fn export_csv(session: &MeasurementSession) -> String {
    let mut out = String::from("name,unit,value,unavailable_reason\n");
    for row in export_rows(session) {
        out.push_str(&escape_csv(&row.name));
        out.push(',');
        out.push_str(&escape_csv(&row.unit));
        out.push(',');
        out.push_str(&escape_csv(row.value.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&escape_csv(row.unavailable_reason.as_deref().unwrap_or("")));
        out.push('\n');
    }
    out
}

fn escape_csv(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_string()
    }
}

/// Parse CSV produced by [`export_csv`] back to rows.
pub fn parse_csv(text: &str) -> Result<Vec<ExportRow>, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| "csv: empty".to_string())?;
    if header != "name,unit,value,unavailable_reason" {
        return Err(format!("csv: unexpected header: {header}"));
    }

    let mut rows = Vec::new();
    for (idx, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let cols = split_csv_line(line).map_err(|e| format!("csv line {}: {e}", idx + 2))?;
        if cols.len() != 4 {
            return Err(format!(
                "csv line {}: expected 4 columns, got {}",
                idx + 2,
                cols.len()
            ));
        }
        let value = if cols[2].is_empty() {
            None
        } else {
            Some(cols[2].clone())
        };
        let unavailable_reason = if cols[3].is_empty() {
            None
        } else {
            Some(cols[3].clone())
        };
        rows.push(ExportRow {
            name: cols[0].clone(),
            unit: cols[1].clone(),
            value,
            unavailable_reason,
        });
    }
    Ok(rows)
}

fn split_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut cols = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' if !in_quotes => {
                in_quotes = true;
            }
            ',' if !in_quotes => {
                cols.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if in_quotes {
        return Err("unclosed quote".to_string());
    }
    cols.push(cur);
    Ok(cols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_capture::compute_stats_ms;
    use crate::session::model::{
        DiskSummary, FrameSummary, HardwareSummary, MeasurementSession, MetricF64, NetworkSummary,
        SessionMeta, TextMetric,
    };

    fn mock_completed_session() -> MeasurementSession {
        // mock: synthetic session for export round-trip (not a live capture)
        let frames = vec![16.0_f64; 100];
        let stats = compute_stats_ms(&frames).expect("mock stats");
        let mut session = MeasurementSession::new(SessionMeta {
            session_id: "mock-session-1".to_string(),
            started_at_unix_ms: 1_700_000_000_000,
            ended_at_unix_ms: None,
            target_process: Some("mock-game.exe".to_string()),
            os: "windows".to_string(),
            notes: Some("mock note, with comma".to_string()),
        });
        session.complete(1_700_000_060_000);
        session.frame = FrameSummary::from_stats(&stats);
        session.hardware = HardwareSummary {
            cpu_usage_percent: MetricF64::available(42.5, "percent"),
            gpu_usage_percent: MetricF64::available(80.0, "percent"),
            ram_used_bytes: MetricF64::available(8_589_934_592.0, "bytes"),
            ram_total_bytes: MetricF64::available(17_179_869_184.0, "bytes"),
            vram_used_bytes: MetricF64::available(3_221_225_472.0, "bytes"),
            vram_total_bytes: MetricF64::available(8_589_934_592.0, "bytes"),
            cpu_temp_celsius: MetricF64::unavailable("mock: sensor not exposed"),
            gpu_temp_celsius: MetricF64::unavailable("mock: sensor not exposed"),
            cpu_clock_mhz: MetricF64::unavailable("mock: sensor not exposed"),
            gpu_clock_mhz: MetricF64::unavailable("mock: sensor not exposed"),
            throttling: TextMetric::unavailable("mock: sensor not exposed"),
        };
        session.disk = DiskSummary {
            disk_read_bytes_per_sec: MetricF64::available(1_048_576.0, "bytes_per_sec"),
            disk_write_bytes_per_sec: MetricF64::available(524_288.0, "bytes_per_sec"),
            disk_queue_depth: MetricF64::unavailable("mock: queue not exposed"),
        };
        session.network = NetworkSummary {
            network_host: TextMetric::available("1.1.1.1"),
            network_latency_ms: MetricF64::available(12.5, "ms"),
            network_jitter_ms: MetricF64::available(1.2, "ms"),
            network_packet_loss_percent: MetricF64::available(0.0, "percent"),
        };
        session
    }

    fn field_map(
        rows: &[ExportRow],
    ) -> std::collections::BTreeMap<String, (String, Option<String>, Option<String>)> {
        rows.iter()
            .map(|r| {
                (
                    r.name.clone(),
                    (
                        r.unit.clone(),
                        r.value.clone(),
                        r.unavailable_reason.clone(),
                    ),
                )
            })
            .collect()
    }

    #[test]
    fn csv_and_json_share_identical_fields_and_units() {
        let session = mock_completed_session();
        let json_text = export_json(&session).expect("json");
        let doc = parse_json(&json_text).expect("parse json");
        let csv_text = export_csv(&session);
        let csv_rows = parse_csv(&csv_text).expect("parse csv");

        assert_eq!(doc.fields.len(), csv_rows.len());
        for (j, c) in doc.fields.iter().zip(csv_rows.iter()) {
            assert_eq!(j.name, c.name, "field name mismatch");
            assert_eq!(j.unit, c.unit, "unit mismatch for {}", j.name);
            assert_eq!(j.value, c.value, "value mismatch for {}", j.name);
            assert_eq!(
                j.unavailable_reason, c.unavailable_reason,
                "unavailable mismatch for {}",
                j.name
            );
        }
    }

    #[test]
    fn json_round_trip_preserves_export_rows() {
        let session = mock_completed_session();
        let json_text = export_json(&session).expect("json");
        let doc = parse_json(&json_text).expect("parse");
        let again = serde_json::to_string_pretty(&doc).expect("re-serialize");
        let doc2 = parse_json(&again).expect("re-parse");
        assert_eq!(doc, doc2);
        assert_eq!(field_map(&doc.fields), field_map(&export_rows(&session)));
    }

    #[test]
    fn csv_round_trip_preserves_export_rows() {
        let session = mock_completed_session();
        let csv_text = export_csv(&session);
        let rows = parse_csv(&csv_text).expect("parse");
        assert_eq!(rows, export_rows(&session));
        // comma inside notes must survive
        let notes = rows.iter().find(|r| r.name == "notes").expect("notes");
        assert_eq!(notes.value.as_deref(), Some("mock note, with comma"));
    }

    #[test]
    fn session_serde_round_trip() {
        let session = mock_completed_session();
        let raw = serde_json::to_string(&session).expect("serialize session");
        let back: MeasurementSession = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(session, back);
    }

    #[test]
    fn unavailable_keeps_declared_unit() {
        let session = mock_completed_session();
        let rows = export_rows(&session);
        let temp = rows
            .iter()
            .find(|r| r.name == "cpu_temp_celsius")
            .expect("cpu_temp");
        assert_eq!(temp.unit, "celsius");
        assert!(temp.value.is_none());
        assert!(temp.unavailable_reason.as_ref().unwrap().contains("mock:"));
    }
}
