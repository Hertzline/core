//! Disk activity collector (AC-1.3).
//!
//! Throughput from timed sysinfo `Disk::usage` interval bytes (`read_bytes` /
//! `written_bytes` since last refresh). Queue depth stays unavailable until a
//! documented OS path is wired (sysinfo Disk has no queue API).

use std::thread;
use std::time::Duration;

use sysinfo::Disks;

use crate::session::{DiskSummary, MeasurementSession, MetricF64};

/// Bytes transferred during one measured interval (not lifetime totals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskByteCounters {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// Pluggable disk IO source (mocks labeled in tests).
pub trait DiskIoSource {
    /// Measure bytes transferred over roughly `interval`; return counters + elapsed seconds.
    fn measure_interval(
        &mut self,
        interval: Duration,
    ) -> Result<(DiskByteCounters, f64), String>;
}

/// Build a disk summary from interval byte counts over `elapsed_secs`.
///
/// Queue depth is unavailable unless `queue_depth` is provided.
pub fn disk_summary_from_delta(
    read_delta: u64,
    write_delta: u64,
    elapsed_secs: f64,
    queue_depth: Option<f64>,
) -> DiskSummary {
    if !(elapsed_secs.is_finite() && elapsed_secs > 0.0) {
        return DiskSummary {
            disk_read_bytes_per_sec: MetricF64::unavailable("elapsed interval invalid"),
            disk_write_bytes_per_sec: MetricF64::unavailable("elapsed interval invalid"),
            disk_queue_depth: queue_or_unavailable(queue_depth),
        };
    }

    DiskSummary {
        disk_read_bytes_per_sec: MetricF64::available(
            (read_delta as f64) / elapsed_secs,
            "bytes_per_sec",
        ),
        disk_write_bytes_per_sec: MetricF64::available(
            (write_delta as f64) / elapsed_secs,
            "bytes_per_sec",
        ),
        disk_queue_depth: queue_or_unavailable(queue_depth),
    }
}

fn queue_or_unavailable(queue_depth: Option<f64>) -> MetricF64 {
    match queue_depth {
        Some(v) if v.is_finite() && v >= 0.0 => MetricF64::available(v, "count"),
        Some(_) => MetricF64::unavailable("non-finite or negative queue depth rejected"),
        None => MetricF64::unavailable(
            "disk queue depth not exposed by sysinfo; OS-specific path not wired",
        ),
    }
}

/// Timed disk throughput collector.
pub struct DiskCollector<S: DiskIoSource> {
    source: S,
    sample_interval: Duration,
}

impl<S: DiskIoSource> DiskCollector<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            sample_interval: Duration::from_millis(500),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.sample_interval = interval;
        self
    }

    /// One timed IO sample; queue stays unavailable.
    pub fn collect(&mut self) -> DiskSummary {
        match self.source.measure_interval(self.sample_interval) {
            Ok((counters, elapsed)) => {
                disk_summary_from_delta(counters.read_bytes, counters.write_bytes, elapsed, None)
            }
            Err(reason) => DiskSummary::all_unavailable(reason),
        }
    }

    pub fn collect_into(&mut self, session: &mut MeasurementSession) -> DiskSummary {
        let summary = self.collect();
        session.disk = summary.clone();
        summary
    }
}

/// Live sysinfo disk IO using documented `DiskUsage` interval fields.
pub struct SysinfoDiskIo;

impl DiskIoSource for SysinfoDiskIo {
    fn measure_interval(
        &mut self,
        interval: Duration,
    ) -> Result<(DiskByteCounters, f64), String> {
        let mut disks = Disks::new_with_refreshed_list()
            .map_err(|e| format!("Disks::new_with_refreshed_list failed: {e}"))?;
        // Prime so the next refresh reports bytes since this point.
        disks.refresh(true);
        let started = std::time::Instant::now();
        thread::sleep(interval);
        disks.refresh(true);
        let elapsed = started.elapsed().as_secs_f64();

        let mut read_bytes = 0_u64;
        let mut write_bytes = 0_u64;
        let mut count = 0_usize;
        for disk in disks.iter() {
            let usage = disk.usage();
            read_bytes = read_bytes.saturating_add(usage.read_bytes);
            write_bytes = write_bytes.saturating_add(usage.written_bytes);
            count += 1;
        }
        if count == 0 {
            return Err("no disks reported by sysinfo".into());
        }
        Ok((
            DiskByteCounters {
                read_bytes,
                write_bytes,
            },
            elapsed,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionMeta;

    struct MockDiskIo {
        result: Result<(DiskByteCounters, f64), String>,
    }

    impl DiskIoSource for MockDiskIo {
        fn measure_interval(
            &mut self,
            _interval: Duration,
        ) -> Result<(DiskByteCounters, f64), String> {
            self.result.clone()
        }
    }

    #[test]
    fn mock_delta_yields_throughput_queue_unavailable() {
        // mock: 1 MiB read and 512 KiB write over 1.0 s
        let summary = disk_summary_from_delta(1_048_576, 524_288, 1.0, None);
        match summary.disk_read_bytes_per_sec {
            MetricF64::Available { value, unit } => {
                assert_eq!(unit, "bytes_per_sec");
                assert!((value - 1_048_576.0).abs() < 1e-6);
            }
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
        match summary.disk_write_bytes_per_sec {
            MetricF64::Available { value, .. } => {
                assert!((value - 524_288.0).abs() < 1e-6);
            }
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
        match summary.disk_queue_depth {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("queue"));
            }
            MetricF64::Available { .. } => panic!("must not invent queue depth"),
        }
    }

    #[test]
    fn mock_collector_writes_session() {
        // mock: labeled interval counters
        let mut collector = DiskCollector::new(MockDiskIo {
            result: Ok((
                DiskByteCounters {
                    read_bytes: 2_000,
                    write_bytes: 500,
                },
                0.5,
            )),
        });

        let mut session = MeasurementSession::new(SessionMeta {
            session_id: "mock-disk".into(),
            started_at_unix_ms: 1,
            ended_at_unix_ms: None,
            target_process: None,
            os: "test".into(),
            notes: Some("mock disk".into()),
        });
        let summary = collector.collect_into(&mut session);
        match summary.disk_read_bytes_per_sec {
            MetricF64::Available { value, .. } => assert!((value - 4000.0).abs() < 1e-6),
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
        assert_eq!(session.disk, summary);
        match session.disk.disk_queue_depth {
            MetricF64::Unavailable { .. } => {}
            MetricF64::Available { .. } => panic!("queue must stay unavailable"),
        }
    }

    #[test]
    fn mock_source_error_marks_unavailable() {
        // mock: IO failure path
        let mut collector = DiskCollector::new(MockDiskIo {
            result: Err("mock: disks unavailable".into()),
        });
        let summary = collector.collect();
        match summary.disk_read_bytes_per_sec {
            MetricF64::Unavailable { reason } => assert!(reason.contains("mock:")),
            MetricF64::Available { .. } => panic!("expected unavailable"),
        }
    }

    #[test]
    fn invalid_elapsed_unavailable() {
        let summary = disk_summary_from_delta(100, 100, 0.0, None);
        match summary.disk_read_bytes_per_sec {
            MetricF64::Unavailable { reason } => assert!(reason.contains("elapsed")),
            MetricF64::Available { .. } => panic!("zero elapsed must fail"),
        }
    }
}
