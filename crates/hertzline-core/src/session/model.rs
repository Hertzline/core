//! In-memory measurement session (AC-1.8 data holder).
//!
//! Missing sensors stay `Unavailable` with a reason -- never invented numbers.

use serde::{Deserialize, Serialize};

use crate::frame_capture::FrameTimeStats;

/// Schema version for on-disk / exported sessions.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// Scalar metric with an explicit unit, or unavailable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MetricF64 {
    Available { value: f64, unit: String },
    Unavailable { reason: String },
}

impl MetricF64 {
    pub fn available(value: f64, unit: &str) -> Self {
        Self::Available {
            value,
            unit: unit.to_string(),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
}

/// Text metric or unavailable (for flags / host names).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TextMetric {
    Available { value: String },
    Unavailable { reason: String },
}

impl TextMetric {
    pub fn available(value: impl Into<String>) -> Self {
        Self::Available {
            value: value.into(),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: Option<i64>,
    pub target_process: Option<String>,
    pub os: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameSummary {
    pub sample_count: MetricF64,
    pub avg_fps: MetricF64,
    pub low_1pct_fps: MetricF64,
    pub low_0_1pct_fps: MetricF64,
    pub frame_time_variance_ms2: MetricF64,
    pub avg_frame_time_ms: MetricF64,
}

impl FrameSummary {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            sample_count: MetricF64::unavailable(reason.clone()),
            avg_fps: MetricF64::unavailable(reason.clone()),
            low_1pct_fps: MetricF64::unavailable(reason.clone()),
            low_0_1pct_fps: MetricF64::unavailable(reason.clone()),
            frame_time_variance_ms2: MetricF64::unavailable(reason.clone()),
            avg_frame_time_ms: MetricF64::unavailable(reason),
        }
    }

    pub fn from_stats(stats: &FrameTimeStats) -> Self {
        Self {
            sample_count: MetricF64::available(stats.sample_count as f64, "count"),
            avg_fps: MetricF64::available(stats.avg_fps, "fps"),
            low_1pct_fps: MetricF64::available(stats.low_1pct_fps, "fps"),
            low_0_1pct_fps: MetricF64::available(stats.low_0_1pct_fps, "fps"),
            frame_time_variance_ms2: MetricF64::available(
                stats.frame_time_variance_ms2,
                "ms^2",
            ),
            avg_frame_time_ms: MetricF64::available(stats.avg_frame_time_ms, "ms"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareSummary {
    pub cpu_usage_percent: MetricF64,
    pub gpu_usage_percent: MetricF64,
    pub ram_used_bytes: MetricF64,
    pub ram_total_bytes: MetricF64,
    pub vram_used_bytes: MetricF64,
    pub vram_total_bytes: MetricF64,
    pub cpu_temp_celsius: MetricF64,
    pub gpu_temp_celsius: MetricF64,
    pub cpu_clock_mhz: MetricF64,
    pub gpu_clock_mhz: MetricF64,
    pub throttling: TextMetric,
}

impl HardwareSummary {
    pub fn all_unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            cpu_usage_percent: MetricF64::unavailable(reason.clone()),
            gpu_usage_percent: MetricF64::unavailable(reason.clone()),
            ram_used_bytes: MetricF64::unavailable(reason.clone()),
            ram_total_bytes: MetricF64::unavailable(reason.clone()),
            vram_used_bytes: MetricF64::unavailable(reason.clone()),
            vram_total_bytes: MetricF64::unavailable(reason.clone()),
            cpu_temp_celsius: MetricF64::unavailable(reason.clone()),
            gpu_temp_celsius: MetricF64::unavailable(reason.clone()),
            cpu_clock_mhz: MetricF64::unavailable(reason.clone()),
            gpu_clock_mhz: MetricF64::unavailable(reason.clone()),
            throttling: TextMetric::unavailable(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskSummary {
    pub disk_read_bytes_per_sec: MetricF64,
    pub disk_write_bytes_per_sec: MetricF64,
    pub disk_queue_depth: MetricF64,
}

impl DiskSummary {
    pub fn all_unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            disk_read_bytes_per_sec: MetricF64::unavailable(reason.clone()),
            disk_write_bytes_per_sec: MetricF64::unavailable(reason.clone()),
            disk_queue_depth: MetricF64::unavailable(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkSummary {
    pub network_host: TextMetric,
    pub network_latency_ms: MetricF64,
    pub network_jitter_ms: MetricF64,
    pub network_packet_loss_percent: MetricF64,
}

impl NetworkSummary {
    pub fn all_unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            network_host: TextMetric::unavailable(reason.clone()),
            network_latency_ms: MetricF64::unavailable(reason.clone()),
            network_jitter_ms: MetricF64::unavailable(reason.clone()),
            network_packet_loss_percent: MetricF64::unavailable(reason),
        }
    }
}

/// Completed or in-progress measurement session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementSession {
    pub schema_version: u32,
    pub meta: SessionMeta,
    pub frame: FrameSummary,
    pub hardware: HardwareSummary,
    pub disk: DiskSummary,
    pub network: NetworkSummary,
}

impl MeasurementSession {
    pub fn new(meta: SessionMeta) -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            meta,
            frame: FrameSummary::unavailable("not measured"),
            hardware: HardwareSummary::all_unavailable("not measured"),
            disk: DiskSummary::all_unavailable("not measured"),
            network: NetworkSummary::all_unavailable("not measured"),
        }
    }

    pub fn complete(&mut self, ended_at_unix_ms: i64) {
        self.meta.ended_at_unix_ms = Some(ended_at_unix_ms);
    }

    pub fn duration_ms(&self) -> Option<i64> {
        let end = self.meta.ended_at_unix_ms?;
        Some(end.saturating_sub(self.meta.started_at_unix_ms))
    }
}
