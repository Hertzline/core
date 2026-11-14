//! Thermal warnings from sensor samples (AC-5.5).
//!
//! Warns only when temperatures or throttle flags are actually exposed.
//! Missing sensors stay unavailable -- never invented.

use crate::collectors::HardwareSample;

/// Default high-temperature threshold in Celsius when a sensor value exists.
pub const HIGH_TEMP_CELSIUS: f64 = 90.0;

/// Substrings that indicate throttling when present in a throttle flag string.
pub const THROTTLE_HINTS: &[&str] = &["throttl", "thermal", "power limit", "prochot"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalLevel {
    Ok,
    Warning,
    Unavailable,
}

impl ThermalLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalWarning {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalReport {
    pub level: ThermalLevel,
    pub cpu_temp_celsius: Option<f64>,
    pub gpu_temp_celsius: Option<f64>,
    pub throttling: Option<String>,
    pub warnings: Vec<ThermalWarning>,
    pub note: String,
}

fn looks_throttled(flag: &str) -> bool {
    let lower = flag.to_ascii_lowercase();
    THROTTLE_HINTS.iter().any(|hint| lower.contains(hint))
}

/// Assess thermal state from a hardware sample (AC-5.5).
pub fn assess_thermal(sample: &HardwareSample) -> ThermalReport {
    let mut warnings = Vec::new();

    if let Some(temp) = sample.cpu_temp_celsius {
        if temp.is_finite() && temp >= HIGH_TEMP_CELSIUS {
            warnings.push(ThermalWarning {
                source: "cpu_temp".into(),
                message: format!(
                    "CPU temperature {temp:.1} C is at or above {HIGH_TEMP_CELSIUS} C"
                ),
            });
        }
    }
    if let Some(temp) = sample.gpu_temp_celsius {
        if temp.is_finite() && temp >= HIGH_TEMP_CELSIUS {
            warnings.push(ThermalWarning {
                source: "gpu_temp".into(),
                message: format!(
                    "GPU temperature {temp:.1} C is at or above {HIGH_TEMP_CELSIUS} C"
                ),
            });
        }
    }
    if let Some(flag) = &sample.throttling {
        if looks_throttled(flag) {
            warnings.push(ThermalWarning {
                source: "throttling".into(),
                message: format!("Throttle flag reports: {flag}"),
            });
        }
    }

    let any_temp = sample.cpu_temp_celsius.is_some() || sample.gpu_temp_celsius.is_some();
    let any_throttle = sample.throttling.is_some();

    let (level, note) = if !warnings.is_empty() {
        (
            ThermalLevel::Warning,
            "Thermal warning from exposed sensors. Hertzline does not change fans or power limits in v1.".into(),
        )
    } else if any_temp || any_throttle {
        (
            ThermalLevel::Ok,
            "Exposed sensors are below the warning threshold and no throttle hint is present."
                .into(),
        )
    } else {
        (
            ThermalLevel::Unavailable,
            "CPU/GPU temperature and throttle flags are not exposed on this host (D4 / AC-5.5); never invented.".into(),
        )
    };

    ThermalReport {
        level,
        cpu_temp_celsius: sample.cpu_temp_celsius,
        gpu_temp_celsius: sample.gpu_temp_celsius,
        throttling: sample.throttling.clone(),
        warnings,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_when_no_sensors() {
        let report = assess_thermal(&HardwareSample::all_missing());
        assert_eq!(report.level, ThermalLevel::Unavailable);
        assert!(report.warnings.is_empty());
        assert!(report.note.contains("not exposed"));
    }

    #[test]
    fn warning_on_high_temp_and_throttle() {
        let mut sample = HardwareSample::all_missing();
        sample.cpu_temp_celsius = Some(95.0);
        sample.throttling = Some("thermal throttling".into());
        let report = assess_thermal(&sample);
        assert_eq!(report.level, ThermalLevel::Warning);
        assert_eq!(report.warnings.len(), 2);
    }

    #[test]
    fn ok_when_cool() {
        let mut sample = HardwareSample::all_missing();
        sample.gpu_temp_celsius = Some(60.0);
        sample.throttling = Some("none".into());
        let report = assess_thermal(&sample);
        assert_eq!(report.level, ThermalLevel::Ok);
        assert!(report.warnings.is_empty());
    }
}
