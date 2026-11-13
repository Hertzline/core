//! Hardware / disk sensor probe (HZ-007 / D4).
//!
//! Prefer OS APIs via SPDX MIT `sysinfo`. Missing sensors are labeled
//! unavailable -- never invented. Session collectors: `crate::collectors` (HZ-012).

mod probe;

pub use probe::{probe_sensors, MetricAvailability, MetricId, SensorSource, SensorsProbeReport};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_host_report() {
        let report = probe_sensors();
        assert!(!report.os.is_empty());
        assert_eq!(report.primary_source, SensorSource::Sysinfo);
        assert_eq!(report.primary_license_spdx, "MIT");
        assert!(!report.metrics.is_empty());
    }
}
