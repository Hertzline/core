//! Per-OS sensor source probe (AC-1.2, AC-1.3 license/source readiness).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use sysinfo::{Components, Disks, Gpus, System};

/// Chosen primary sensor stack for Hertzline v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorSource {
    Sysinfo,
}

/// Stable metric identifiers for the probe matrix (AC-1.2 / AC-1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricId {
    CpuUsage,
    CpuFrequency,
    RamUsage,
    GpuUsage,
    VramUsage,
    Temperature,
    GpuClock,
    ThrottleFlag,
    DiskThroughput,
    DiskQueue,
}

/// Whether a metric can be read on this host right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricAvailability {
    Available,
    Partial,
    Unavailable,
    NotPossible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricProbe {
    pub id: MetricId,
    pub availability: MetricAvailability,
    pub source: String,
    pub detail: String,
    /// Present only for live smoke values; never invent when unavailable.
    pub sample: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensorsProbeReport {
    pub os: String,
    pub primary_source: SensorSource,
    pub primary_license_spdx: String,
    pub rejected_sources: Vec<RejectedSource>,
    pub optional_runtime: Vec<OptionalRuntime>,
    pub metrics: Vec<MetricProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedSource {
    pub name: String,
    pub license_spdx: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalRuntime {
    pub name: String,
    pub license_note: String,
    pub status: String,
    pub detail: String,
}

/// Probe sensor sources and record per-metric availability on this host.
pub fn probe_sensors() -> SensorsProbeReport {
    let rejected = vec![
        RejectedSource {
            name: "LibreHardwareMonitor / LibreHardwareMonitorLib".into(),
            license_spdx: "MPL-2.0".into(),
            reason: "Not MIT/Apache/BSD; D4 requires SPDX-compatible permissive libs for linked third-party code (official LICENSE verified 2026-11-13)".into(),
        },
        RejectedSource {
            name: "AMD ADLX / ADL SDK (redistributed into Hertzline binary)".into(),
            license_spdx: "LicenseRef-AMD-ADLX-EULA".into(),
            reason: "Proprietary SDK EULA; not MIT/Apache/BSD. v1 does not link or redistribute ADLX; AMD GPU metrics via sysinfo/OS only, else unavailable".into(),
        },
    ];

    let optional_runtime = vec![probe_optional_nvml()];

    let mut metrics = Vec::new();
    metrics.extend(probe_sysinfo_metrics());

    SensorsProbeReport {
        os: std::env::consts::OS.to_string(),
        primary_source: SensorSource::Sysinfo,
        primary_license_spdx: "MIT".into(),
        rejected_sources: rejected,
        optional_runtime,
        metrics,
    }
}

fn probe_sysinfo_metrics() -> Vec<MetricProbe> {
    let mut out = Vec::new();

    let sys = match System::new_all() {
        Ok(mut sys) => {
            sys.refresh_cpu_all();
            sys.refresh_memory();
            // CPU usage is differential; second sample after the documented minimum interval.
            thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)));
            sys.refresh_cpu_all();
            Some(sys)
        }
        Err(err) => {
            out.push(MetricProbe {
                id: MetricId::CpuUsage,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::System".into(),
                detail: format!("System::new_all failed: {err}"),
                sample: None,
            });
            out.push(MetricProbe {
                id: MetricId::CpuFrequency,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::System".into(),
                detail: "system init failed".into(),
                sample: None,
            });
            out.push(MetricProbe {
                id: MetricId::RamUsage,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::System".into(),
                detail: "system init failed".into(),
                sample: None,
            });
            None
        }
    };

    if let Some(sys) = sys.as_ref() {
        let cpu_count = sys.cpus().len();
        let global = sys.global_cpu_usage();
        out.push(MetricProbe {
            id: MetricId::CpuUsage,
            availability: if cpu_count > 0 {
                MetricAvailability::Available
            } else {
                MetricAvailability::Unavailable
            },
            source: "sysinfo::System (OS APIs)".into(),
            detail: format!("cpus={cpu_count}; SPDX MIT sysinfo 0.40.3"),
            sample: if cpu_count > 0 {
                Some(format!("global_cpu_usage={global:.2}%"))
            } else {
                None
            },
        });

        let freq = sys.cpus().iter().find_map(|c| {
            let f = c.frequency();
            if f > 0 {
                Some(f)
            } else {
                None
            }
        });
        out.push(MetricProbe {
            id: MetricId::CpuFrequency,
            availability: if freq.is_some() {
                MetricAvailability::Available
            } else {
                MetricAvailability::Unavailable
            },
            source: "sysinfo::Cpu::frequency".into(),
            detail: "MHz when the OS exposes it; else unavailable".into(),
            sample: freq.map(|f| format!("{f} MHz")),
        });

        let total = sys.total_memory();
        let used = sys.used_memory();
        out.push(MetricProbe {
            id: MetricId::RamUsage,
            availability: if total > 0 {
                MetricAvailability::Available
            } else {
                MetricAvailability::Unavailable
            },
            source: "sysinfo::System memory".into(),
            detail: "bytes from OS".into(),
            sample: if total > 0 {
                Some(format!("used={used} total={total}"))
            } else {
                None
            },
        });
    }

    match Gpus::new_with_refreshed_list() {
        Ok(mut gpus) => {
            // Usage is time-diff based on Windows; refresh twice.
            thread::sleep(Duration::from_millis(200));
            gpus.refresh(true);
            let count = gpus.iter().count();
            let usage = gpus.iter().find_map(|g| g.usage());
            let vram = gpus
                .iter()
                .find_map(|g| match (g.used_memory(), g.total_memory()) {
                    (Some(u), Some(t)) => Some((u, t)),
                    _ => None,
                });
            out.push(MetricProbe {
                id: MetricId::GpuUsage,
                availability: if count == 0 {
                    MetricAvailability::Unavailable
                } else if usage.is_some() {
                    MetricAvailability::Available
                } else {
                    MetricAvailability::Partial
                },
                source: "sysinfo::Gpus (OS / vendor interfaces)".into(),
                detail: format!(
                    "gpus={count}; macOS VRAM N/A (shared); Linux often NVIDIA/AMD only"
                ),
                sample: usage.map(|u| format!("usage={u}%")),
            });
            out.push(MetricProbe {
                id: MetricId::VramUsage,
                availability: if count == 0 {
                    MetricAvailability::Unavailable
                } else if vram.is_some() {
                    MetricAvailability::Available
                } else if cfg!(target_os = "macos") {
                    MetricAvailability::NotPossible
                } else {
                    MetricAvailability::Unavailable
                },
                source: "sysinfo::Gpu memory".into(),
                detail: "dedicated VRAM only; shared/system memory not counted; macOS always None per sysinfo docs".into(),
                sample: vram.map(|(u, t)| format!("used={u} total={t}")),
            });
        }
        Err(err) => {
            out.push(MetricProbe {
                id: MetricId::GpuUsage,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::Gpus".into(),
                detail: format!("Gpus::new_with_refreshed_list failed: {err}"),
                sample: None,
            });
            out.push(MetricProbe {
                id: MetricId::VramUsage,
                availability: if cfg!(target_os = "macos") {
                    MetricAvailability::NotPossible
                } else {
                    MetricAvailability::Unavailable
                },
                source: "sysinfo::Gpus".into(),
                detail: "gpu list unavailable".into(),
                sample: None,
            });
        }
    }

    match Components::new_with_refreshed_list() {
        Ok(components) => {
            let with_temp: Vec<_> = components
                .iter()
                .filter_map(|c| c.temperature().map(|t| (c.label().to_string(), t)))
                .collect();
            out.push(MetricProbe {
                id: MetricId::Temperature,
                availability: if with_temp.is_empty() {
                    MetricAvailability::Unavailable
                } else {
                    MetricAvailability::Partial
                },
                source: "sysinfo::Components (hwmon/IOKit/WMI as exposed)".into(),
                detail: format!(
                    "components_with_temp={}; Windows often sparse without vendor sensors",
                    with_temp.len()
                ),
                sample: with_temp.first().map(|(label, t)| format!("{label}={t}C")),
            });
        }
        Err(err) => {
            out.push(MetricProbe {
                id: MetricId::Temperature,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::Components".into(),
                detail: format!("Components::new_with_refreshed_list failed: {err}"),
                sample: None,
            });
        }
    }

    // GPU clocks and throttle flags are not exposed by sysinfo Gpu API (0.40.3).
    out.push(MetricProbe {
        id: MetricId::GpuClock,
        availability: MetricAvailability::Unavailable,
        source: "none in v1 primary stack".into(),
        detail: "sysinfo Gpu has no clock API; optional system NVML (NVIDIA driver, not redistributed) may cover later; else unavailable".into(),
        sample: None,
    });
    out.push(MetricProbe {
        id: MetricId::ThrottleFlag,
        availability: MetricAvailability::Unavailable,
        source: "none in v1 primary stack".into(),
        detail: "no portable MIT/Apache/BSD throttle flag in sysinfo; label unavailable until a documented OS path is wired in HZ-012".into(),
        sample: None,
    });

    match Disks::new_with_refreshed_list() {
        Ok(disks) => {
            let count = disks.iter().count();
            // DiskUsage on Disk is cumulative/space oriented; throughput needs refresh deltas in HZ-012.
            let free = disks.iter().map(|d| d.available_space()).sum::<u64>();
            let total = disks.iter().map(|d| d.total_space()).sum::<u64>();
            out.push(MetricProbe {
                id: MetricId::DiskThroughput,
                availability: if count > 0 {
                    MetricAvailability::Partial
                } else {
                    MetricAvailability::Unavailable
                },
                source: "sysinfo::Disks + process DiskUsage deltas (HZ-012)".into(),
                detail: format!(
                    "disks={count}; space probe ok; read/write throughput needs timed refresh (AC-1.3)"
                ),
                sample: if count > 0 {
                    Some(format!("available_bytes={free} total_bytes={total}"))
                } else {
                    None
                },
            });
            out.push(MetricProbe {
                id: MetricId::DiskQueue,
                availability: MetricAvailability::Unavailable,
                source: "OS-specific (not in sysinfo Disk)".into(),
                detail: "queue pressure where OS exposes it in HZ-012; else unavailable".into(),
                sample: None,
            });
        }
        Err(err) => {
            out.push(MetricProbe {
                id: MetricId::DiskThroughput,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::Disks".into(),
                detail: format!("Disks::new_with_refreshed_list failed: {err}"),
                sample: None,
            });
            out.push(MetricProbe {
                id: MetricId::DiskQueue,
                availability: MetricAvailability::Unavailable,
                source: "sysinfo::Disks".into(),
                detail: "disk list unavailable".into(),
                sample: None,
            });
        }
    }

    out
}

fn probe_optional_nvml() -> OptionalRuntime {
    // Do not link or redistribute the NVIDIA SDK. Only detect a system-provided
    // nvidia-smi / NVML runtime for a possible later optional path.
    let smi = find_on_path("nvidia-smi");
    let nvml_lib = find_nvml_library();
    match (smi, nvml_lib) {
        (Some(smi), lib) => OptionalRuntime {
            name: "NVIDIA NVML (system driver runtime)".into(),
            license_note: "nvml-wrapper crate MIT OR Apache-2.0; NVML runtime is NVIDIA driver EULA -- load from system, never redistribute SDK".into(),
            status: "runtime_detected".into(),
            detail: format!(
                "nvidia-smi={}; nvml_lib={}; not linked in this spike",
                smi.display(),
                lib.map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not found".into())
            ),
        },
        (None, Some(lib)) => OptionalRuntime {
            name: "NVIDIA NVML (system driver runtime)".into(),
            license_note: "nvml-wrapper crate MIT OR Apache-2.0; NVML runtime is NVIDIA driver EULA -- load from system, never redistribute SDK".into(),
            status: "runtime_detected".into(),
            detail: format!("nvml_lib={}; nvidia-smi missing; not linked in this spike", lib.display()),
        },
        (None, None) => OptionalRuntime {
            name: "NVIDIA NVML (system driver runtime)".into(),
            license_note: "nvml-wrapper crate MIT OR Apache-2.0; NVML runtime is NVIDIA driver EULA -- load from system, never redistribute SDK".into(),
            status: "unavailable".into(),
            detail: "no nvidia-smi or NVML library on this host; GPU temps/clocks beyond sysinfo stay unavailable".into(),
        },
    }
}

fn find_nvml_library() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        for name in ["nvml.dll"] {
            if let Some(p) = find_on_path(name) {
                return Some(p);
            }
            if let Ok(root) = std::env::var("ProgramW6432") {
                let candidate = PathBuf::from(root)
                    .join("NVIDIA Corporation")
                    .join("NVSMI")
                    .join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            // Driver store path varies; presence of nvidia-smi is enough for the spike.
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        for name in ["libnvidia-ml.so.1", "libnvidia-ml.so"] {
            if let Some(p) = find_on_path(name) {
                return Some(p);
            }
            for dir in ["/usr/lib", "/usr/lib64", "/usr/lib/x86_64-linux-gnu"] {
                let candidate = Path::new(dir).join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

fn find_on_path(file_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(target_os = "windows")]
        {
            let with_exe = dir.join(format!("{file_name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    let _ = Path::new(file_name);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_serializes() {
        let report = probe_sensors();
        let json = serde_json::to_string(&report).expect("serialize");
        let back: SensorsProbeReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.primary_source, SensorSource::Sysinfo);
        assert!(back
            .rejected_sources
            .iter()
            .any(|r| r.license_spdx == "MPL-2.0"));
    }

    #[test]
    fn never_invents_gpu_clock_sample() {
        let report = probe_sensors();
        let clock = report
            .metrics
            .iter()
            .find(|m| m.id == MetricId::GpuClock)
            .expect("gpu_clock");
        assert_eq!(clock.availability, MetricAvailability::Unavailable);
        assert!(clock.sample.is_none());
    }

    #[test]
    fn cpu_or_explicit_unavailable() {
        let report = probe_sensors();
        let cpu = report
            .metrics
            .iter()
            .find(|m| m.id == MetricId::CpuUsage)
            .expect("cpu");
        match cpu.availability {
            MetricAvailability::Available => assert!(cpu.sample.is_some()),
            MetricAvailability::Unavailable => assert!(cpu.sample.is_none()),
            other => panic!("unexpected cpu availability: {other:?}"),
        }
    }
}
