//! Parse PresentMon and MangoHud capture logs into frame times (ms).
//!
//! Column names follow official docs (PresentMon console README; MangoHud
//! `src/logging.cpp` header). No invented metrics.

use std::fmt;

/// Failed to extract frame times from a capture log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameParseError {
    pub reason: String,
}

impl FrameParseError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for FrameParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for FrameParseError {}

/// Parse PresentMon CSV text; use `MsBetweenPresents` (ms between Present calls).
///
/// Source: PresentMon console application README, CSV columns section.
pub fn parse_presentmon_csv(csv: &str) -> Result<Vec<f64>, FrameParseError> {
    let mut lines = csv.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines
        .next()
        .ok_or_else(|| FrameParseError::new("PresentMon CSV is empty"))?;
    let columns: Vec<&str> = split_csv_line(header);
    let col = columns
        .iter()
        .position(|c| eq_ignore_ascii_case(c, "MsBetweenPresents"))
        .ok_or_else(|| {
            FrameParseError::new(
                "PresentMon CSV missing MsBetweenPresents column (see PresentMon console README)",
            )
        })?;

    let mut out = Vec::new();
    for (idx, line) in lines.enumerate() {
        if line.starts_with('#') {
            continue;
        }
        let fields = split_csv_line(line);
        let raw = fields.get(col).ok_or_else(|| {
            FrameParseError::new(format!(
                "PresentMon CSV row {} missing column index {}",
                idx + 2,
                col
            ))
        })?;
        if raw.eq_ignore_ascii_case("NA") || raw.is_empty() {
            continue;
        }
        let ms: f64 = raw.parse().map_err(|_| {
            FrameParseError::new(format!(
                "PresentMon CSV row {}: invalid MsBetweenPresents `{raw}`",
                idx + 2
            ))
        })?;
        if ms.is_finite() && ms > 0.0 {
            out.push(ms);
        }
    }
    Ok(out)
}

/// Parse a MangoHud FPS log; use the `frametime` column (milliseconds).
///
/// Skips optional system-info preamble until the `fps,frametime,...` header.
/// Source: MangoHud README FPS logging + `writeFileHeaders` in logging.cpp.
pub fn parse_mangohud_log(text: &str) -> Result<Vec<f64>, FrameParseError> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let mut header_cols: Option<Vec<&str>> = None;
    let mut frametime_col = 0usize;

    for line in lines.by_ref() {
        let cols = split_csv_line(line);
        if cols
            .iter()
            .any(|c| eq_ignore_ascii_case(c, "frametime"))
            && cols.iter().any(|c| eq_ignore_ascii_case(c, "fps"))
        {
            frametime_col = cols
                .iter()
                .position(|c| eq_ignore_ascii_case(c, "frametime"))
                .expect("frametime present");
            header_cols = Some(cols);
            break;
        }
    }

    if header_cols.is_none() {
        return Err(FrameParseError::new(
            "MangoHud log missing fps,frametime header (set output_folder and toggle logging)",
        ));
    }

    let mut out = Vec::new();
    for (idx, line) in lines.enumerate() {
        if line.starts_with('-') {
            continue;
        }
        let fields = split_csv_line(line);
        // Skip leftover system-info style rows (non-numeric frametime).
        let raw = match fields.get(frametime_col) {
            Some(v) => *v,
            None => continue,
        };
        if raw.is_empty() {
            continue;
        }
        let ms: f64 = match raw.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if ms.is_finite() && ms > 0.0 {
            out.push(ms);
        } else if idx == 0 && out.is_empty() {
            // keep scanning; non-positive at start is ignored
        }
    }
    Ok(out)
}

fn split_csv_line(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

fn eq_ignore_ascii_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_presentmon_csv_ms_between_presents() {
        // mock: PresentMon-shaped CSV (not a live ETW capture)
        let csv = "\
Application,ProcessID,MsBetweenPresents,MsBetweenDisplayChange
game.exe,1000,16.0,16.1
game.exe,1000,16.5,16.4
game.exe,1000,NA,16.0
game.exe,1000,15.5,15.6
";
        let times = parse_presentmon_csv(csv).expect("parse");
        assert_eq!(times, vec![16.0, 16.5, 15.5]);
    }

    #[test]
    fn presentmon_missing_column_errors() {
        let csv = "Application,ProcessID\ngame.exe,1\n";
        let err = parse_presentmon_csv(csv).expect_err("missing col");
        assert!(err.reason.contains("MsBetweenPresents"));
    }

    #[test]
    fn mock_mangohud_log_with_preamble() {
        // mock: MangoHud log shape from logging.cpp headers (not a live wrap)
        let text = "\
---------------------SYSTEM INFO---------------------
os,cpu,gpu,ram,kernel,driver,cpuscheduler
Linux,CPU,GPU,16,6.1,nvidia,sched
--------------------FRAME METRICS--------------------
fps,frametime,cpu_load,cpu_power,gpu_load,cpu_temp,gpu_temp,gpu_core_clock,gpu_mem_clock,gpu_vram_used,gpu_power,ram_used,swap_used,process_rss,cpu_mhz,elapsed
62.5,16.0,10,0,50,40,50,1000,1000,1,50,8,0,0.5,3000,0
60.0,16.6667,10,0,50,40,50,1000,1000,1,50,8,0,0.5,3000,16000000
";
        let times = parse_mangohud_log(text).expect("parse");
        assert_eq!(times.len(), 2);
        assert!((times[0] - 16.0).abs() < 1e-9);
        assert!((times[1] - 16.6667).abs() < 1e-6);
    }

    #[test]
    fn mangohud_missing_header_errors() {
        let err = parse_mangohud_log("no metrics here\n").expect_err("header");
        assert!(err.reason.contains("frametime"));
    }
}
