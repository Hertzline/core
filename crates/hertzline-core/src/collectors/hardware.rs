//! Hardware stats collector (AC-1.2).
//!
//! Primary source: sysinfo 0.40.3 (MIT, D4). Temps/clocks/throttle stay unavailable
//! when the OS/source does not expose them.

use std::thread;
use std::time::Duration;

use sysinfo::{Components, Gpus, System};

use crate::session::{HardwareSummary, MeasurementSession, MetricF64, TextMetric};

/// One hardware sample; `None` means the sensor was not exposed (never invent).
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareSample {
    pub cpu_usage_percent: Option<f64>,
    pub gpu_usage_percent: Option<f64>,
    pub ram_used_bytes: Option<f64>,
    pub ram_total_bytes: Option<f64>,
    pub vram_used_bytes: Option<f64>,
    pub vram_total_bytes: Option<f64>,
    pub cpu_temp_celsius: Option<f64>,
    pub gpu_temp_celsius: Option<f64>,
    pub cpu_clock_mhz: Option<f64>,
    pub gpu_clock_mhz: Option<f64>,
    pub throttling: Option<String>,
}

impl HardwareSample {
    pub fn all_missing() -> Self {
        Self {
            cpu_usage_percent: None,
            gpu_usage_percent: None,
            ram_used_bytes: None,
            ram_total_bytes: None,
            vram_used_bytes: None,
            vram_total_bytes: None,
            cpu_temp_celsius: None,
            gpu_temp_celsius: None,
            cpu_clock_mhz: None,
            gpu_clock_mhz: None,
            throttling: None,
        }
    }
}

/// Pluggable sampler for unit tests (mocks must be labeled in test names/comments).
pub trait HardwareSampler {
    fn sample(&mut self) -> HardwareSample;
}

/// Map a sample to the session hardware block; missing fields get explicit reasons.
pub fn hardware_summary_from_sample(sample: &HardwareSample) -> HardwareSummary {
    HardwareSummary {
        cpu_usage_percent: opt_f64(sample.cpu_usage_percent, "percent", "cpu usage not exposed"),
        gpu_usage_percent: opt_f64(sample.gpu_usage_percent, "percent", "gpu usage not exposed"),
        ram_used_bytes: opt_f64(sample.ram_used_bytes, "bytes", "ram used not exposed"),
        ram_total_bytes: opt_f64(sample.ram_total_bytes, "bytes", "ram total not exposed"),
        vram_used_bytes: opt_f64(
            sample.vram_used_bytes,
            "bytes",
            if cfg!(target_os = "macos") {
                "vram not possible on macOS via sysinfo (shared memory)"
            } else {
                "vram used not exposed"
            },
        ),
        vram_total_bytes: opt_f64(
            sample.vram_total_bytes,
            "bytes",
            if cfg!(target_os = "macos") {
                "vram not possible on macOS via sysinfo (shared memory)"
            } else {
                "vram total not exposed"
            },
        ),
        cpu_temp_celsius: opt_f64(
            sample.cpu_temp_celsius,
            "celsius",
            "cpu temperature not exposed",
        ),
        gpu_temp_celsius: opt_f64(
            sample.gpu_temp_celsius,
            "celsius",
            "gpu temperature not exposed",
        ),
        cpu_clock_mhz: opt_f64(sample.cpu_clock_mhz, "mhz", "cpu clock not exposed"),
        gpu_clock_mhz: opt_f64(
            sample.gpu_clock_mhz,
            "mhz",
            "gpu clock not in sysinfo primary stack (D4)",
        ),
        throttling: match &sample.throttling {
            Some(v) => TextMetric::available(v.clone()),
            None => TextMetric::unavailable(
                "throttle flag not in sysinfo primary stack (D4)",
            ),
        },
    }
}

fn opt_f64(value: Option<f64>, unit: &str, missing_reason: &str) -> MetricF64 {
    match value {
        Some(v) if v.is_finite() => MetricF64::available(v, unit),
        Some(_) => MetricF64::unavailable("non-finite sample rejected"),
        None => MetricF64::unavailable(missing_reason),
    }
}

/// Collects one hardware summary and can write it into a session.
pub struct HardwareCollector<S: HardwareSampler> {
    sampler: S,
}

impl<S: HardwareSampler> HardwareCollector<S> {
    pub fn new(sampler: S) -> Self {
        Self { sampler }
    }

    pub fn collect(&mut self) -> HardwareSummary {
        hardware_summary_from_sample(&self.sampler.sample())
    }

    pub fn collect_into(&mut self, session: &mut MeasurementSession) -> HardwareSummary {
        let summary = self.collect();
        session.hardware = summary.clone();
        summary
    }
}

/// Live sysinfo-backed sampler (CPU usage needs a short differential refresh).
pub struct SysinfoHardwareSampler {
    cpu_interval: Duration,
}

impl Default for SysinfoHardwareSampler {
    fn default() -> Self {
        Self {
            cpu_interval: sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)),
        }
    }
}

impl SysinfoHardwareSampler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl HardwareSampler for SysinfoHardwareSampler {
    fn sample(&mut self) -> HardwareSample {
        let mut sample = HardwareSample::all_missing();

        match System::new_all() {
            Ok(mut sys) => {
                sys.refresh_cpu_all();
                sys.refresh_memory();
                thread::sleep(self.cpu_interval);
                sys.refresh_cpu_all();
                sys.refresh_memory();

                if !sys.cpus().is_empty() {
                    let usage = f64::from(sys.global_cpu_usage());
                    if usage.is_finite() {
                        sample.cpu_usage_percent = Some(usage);
                    }
                }

                let freq = sys.cpus().iter().find_map(|c| {
                    let f = c.frequency();
                    if f > 0 {
                        Some(f as f64)
                    } else {
                        None
                    }
                });
                sample.cpu_clock_mhz = freq;

                let total = sys.total_memory();
                let used = sys.used_memory();
                if total > 0 {
                    sample.ram_total_bytes = Some(total as f64);
                    sample.ram_used_bytes = Some(used as f64);
                }
            }
            Err(_) => {
                // leave CPU/RAM as None -> unavailable reasons
            }
        }

        if let Ok(mut gpus) = Gpus::new_with_refreshed_list() {
            thread::sleep(Duration::from_millis(200));
            gpus.refresh(true);
            if let Some(gpu) = gpus.iter().next() {
                if let Some(usage) = gpu.usage() {
                    let u = f64::from(usage);
                    if u.is_finite() {
                        sample.gpu_usage_percent = Some(u);
                    }
                }
                if let (Some(u), Some(t)) = (gpu.used_memory(), gpu.total_memory()) {
                    sample.vram_used_bytes = Some(u as f64);
                    sample.vram_total_bytes = Some(t as f64);
                }
            }
        }

        if let Ok(components) = Components::new_with_refreshed_list() {
            for c in components.iter() {
                let Some(temp) = c.temperature() else {
                    continue;
                };
                let t = f64::from(temp);
                if !t.is_finite() {
                    continue;
                }
                let label = c.label().to_ascii_lowercase();
                if sample.cpu_temp_celsius.is_none()
                    && (label.contains("cpu") || label.contains("package") || label.contains("tdie"))
                {
                    sample.cpu_temp_celsius = Some(t);
                } else if sample.gpu_temp_celsius.is_none()
                    && (label.contains("gpu") || label.contains("edge"))
                {
                    sample.gpu_temp_celsius = Some(t);
                }
            }
            // If only unlabeled temps exist, do not guess CPU vs GPU -- leave unavailable.
        }

        // gpu_clock_mhz and throttling stay None (D4: not in sysinfo primary stack).
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionMeta;

    struct MockHardwareSampler {
        sample: HardwareSample,
    }

    impl HardwareSampler for MockHardwareSampler {
        fn sample(&mut self) -> HardwareSample {
            self.sample.clone()
        }
    }

    #[test]
    fn mock_partial_hardware_keeps_missing_explicit() {
        // mock: synthetic hardware sample (not a live sensor read)
        let sample = HardwareSample {
            cpu_usage_percent: Some(55.0),
            gpu_usage_percent: Some(70.0),
            ram_used_bytes: Some(4_000_000_000.0),
            ram_total_bytes: Some(8_000_000_000.0),
            vram_used_bytes: Some(2_000_000_000.0),
            vram_total_bytes: Some(8_000_000_000.0),
            cpu_temp_celsius: None,
            gpu_temp_celsius: None,
            cpu_clock_mhz: Some(3600.0),
            gpu_clock_mhz: None,
            throttling: None,
        };
        let summary = hardware_summary_from_sample(&sample);
        match summary.cpu_usage_percent {
            MetricF64::Available { value, unit } => {
                assert_eq!(unit, "percent");
                assert!((value - 55.0).abs() < 1e-9);
            }
            MetricF64::Unavailable { reason } => panic!("cpu should be available: {reason}"),
        }
        match summary.cpu_temp_celsius {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("not exposed"));
            }
            MetricF64::Available { .. } => panic!("must not invent cpu temp"),
        }
        match summary.gpu_clock_mhz {
            MetricF64::Unavailable { reason } => {
                assert!(reason.contains("sysinfo") || reason.contains("D4"));
            }
            MetricF64::Available { .. } => panic!("must not invent gpu clock"),
        }
        match summary.throttling {
            TextMetric::Unavailable { reason } => {
                assert!(reason.contains("throttle") || reason.contains("D4"));
            }
            TextMetric::Available { .. } => panic!("must not invent throttle"),
        }
    }

    #[test]
    fn mock_collector_writes_session() {
        // mock: labeled fixture for session wiring
        let mut collector = HardwareCollector::new(MockHardwareSampler {
            sample: HardwareSample {
                cpu_usage_percent: Some(10.0),
                ..HardwareSample::all_missing()
            },
        });
        let mut session = MeasurementSession::new(SessionMeta {
            session_id: "mock-hw".into(),
            started_at_unix_ms: 1,
            ended_at_unix_ms: None,
            target_process: None,
            os: "test".into(),
            notes: Some("mock hardware".into()),
        });
        collector.collect_into(&mut session);
        match session.hardware.cpu_usage_percent {
            MetricF64::Available { value, .. } => assert!((value - 10.0).abs() < 1e-9),
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
        match session.hardware.ram_total_bytes {
            MetricF64::Unavailable { .. } => {}
            MetricF64::Available { .. } => panic!("ram was not in mock"),
        }
    }

    #[test]
    fn host_sysinfo_sample_never_panics() {
        // automated: live host sample; values may be available or unavailable
        let mut collector = HardwareCollector::new(SysinfoHardwareSampler::new());
        let summary = collector.collect();
        match summary.gpu_clock_mhz {
            MetricF64::Unavailable { .. } => {}
            MetricF64::Available { .. } => {
                panic!("sysinfo primary stack must not invent gpu clock")
            }
        }
        match summary.throttling {
            TextMetric::Unavailable { .. } => {}
            TextMetric::Available { .. } => {
                panic!("sysinfo primary stack must not invent throttle")
            }
        }
    }
}
