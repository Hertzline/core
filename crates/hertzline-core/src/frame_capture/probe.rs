//! Per-OS frame-time source probe (AC-1.5, AC-1.6, AC-1.7).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// High-level capability for the host OS capture path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureCapability {
    /// Full external present/frame capture is a supported target.
    Supported,
    /// Only partial metrics (for example display refresh) are possible.
    Partial,
    /// No supported public path for external game frame times in v1.
    NotPossible,
}

/// Chosen capture method identifier (stable string for logs / UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    PresentMonEtw,
    MangoHudLog,
    MacosDisplayRefreshOnly,
    None,
}

/// Probe outcome for a capture source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Available,
    Unavailable,
    NotPossible,
}

/// JSON-friendly probe report for spikes and later UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub os: String,
    pub method_id: String,
    pub method: CaptureMethod,
    pub capability: CaptureCapability,
    pub status: ProbeStatus,
    pub detail: String,
    pub binary_path: Option<String>,
    pub injects_into_game: bool,
    pub license_spdx: Option<String>,
}

/// Probe the preferred frame-time source for this host OS.
pub fn probe_frame_capture() -> ProbeReport {
    #[cfg(target_os = "windows")]
    {
        return probe_windows();
    }
    #[cfg(target_os = "linux")]
    {
        return probe_linux();
    }
    #[cfg(target_os = "macos")]
    {
        return probe_macos();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        ProbeReport {
            os: std::env::consts::OS.to_string(),
            method_id: "none".into(),
            method: CaptureMethod::None,
            capability: CaptureCapability::NotPossible,
            status: ProbeStatus::NotPossible,
            detail: "unsupported OS for Hertzline v1 frame capture".into(),
            binary_path: None,
            injects_into_game: false,
            license_spdx: None,
        }
    }
}

#[cfg(target_os = "windows")]
fn probe_windows() -> ProbeReport {
    // PresentMon consumes ETW present events; it does not inject into the game
    // process (GameTechDev/PresentMon README; Flash Injector removed in v2.6.0).
    let found = find_presentmon();
    match found {
        Some(path) => {
            let help_ok = Command::new(&path)
                .arg("--help")
                .output()
                .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
                .unwrap_or(false);
            if help_ok {
                ProbeReport {
                    os: "windows".into(),
                    method_id: "presentmon_etw".into(),
                    method: CaptureMethod::PresentMonEtw,
                    capability: CaptureCapability::Supported,
                    status: ProbeStatus::Available,
                    detail: "PresentMon binary responds; ETW present capture (MIT). Real-time session needs admin or Performance Log Users; elevate PresentMon only, never the whole app"
                        .into(),
                    binary_path: Some(path.display().to_string()),
                    injects_into_game: false,
                    license_spdx: Some("MIT".into()),
                }
            } else {
                ProbeReport {
                    os: "windows".into(),
                    method_id: "presentmon_etw".into(),
                    method: CaptureMethod::PresentMonEtw,
                    capability: CaptureCapability::Supported,
                    status: ProbeStatus::Unavailable,
                    detail: format!(
                        "PresentMon found at {} but --help failed; treat capture as unavailable",
                        path.display()
                    ),
                    binary_path: Some(path.display().to_string()),
                    injects_into_game: false,
                    license_spdx: Some("MIT".into()),
                }
            }
        }
        None => ProbeReport {
            os: "windows".into(),
            method_id: "presentmon_etw".into(),
            method: CaptureMethod::PresentMonEtw,
            capability: CaptureCapability::Supported,
            status: ProbeStatus::Unavailable,
            detail: "PresentMon not on PATH or HERTZLINE_PRESENTMON; probe failure -> unavailable (AC-1.5)"
                .into(),
            binary_path: None,
            injects_into_game: false,
            license_spdx: Some("MIT".into()),
        },
    }
}

#[cfg(target_os = "windows")]
fn find_presentmon() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("HERTZLINE_PRESENTMON") {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
    }
    for name in [
        "PresentMon-2.6.0-x64.exe",
        "PresentMon.exe",
        "presentmon.exe",
    ] {
        if let Some(p) = find_on_path(name) {
            return Some(p);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn probe_linux() -> ProbeReport {
    // MangoHud is a Vulkan layer / OpenGL intercept launched with the game
    // (`mangohud <app>`). Hertzline must not silently inject into running
    // anti-cheat titles; integration is opt-in wrap + CSV log ingest.
    let found = find_on_path("mangohud");
    match found {
        Some(path) => {
            let version = Command::new(&path)
                .arg("--version")
                .output()
                .ok()
                .and_then(|o| {
                    let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if text.is_empty() {
                        let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                        if err.is_empty() {
                            None
                        } else {
                            Some(err)
                        }
                    } else {
                        Some(text)
                    }
                });
            ProbeReport {
                os: "linux".into(),
                method_id: "mangohud_log".into(),
                method: CaptureMethod::MangoHudLog,
                capability: CaptureCapability::Supported,
                status: ProbeStatus::Available,
                detail: format!(
                    "mangohud present ({}); use user-opted wrap + output_folder CSV (MIT); no silent inject",
                    version.unwrap_or_else(|| "version unknown".into())
                ),
                binary_path: Some(path.display().to_string()),
                injects_into_game: false,
                license_spdx: Some("MIT".into()),
            }
        }
        None => ProbeReport {
            os: "linux".into(),
            method_id: "mangohud_log".into(),
            method: CaptureMethod::MangoHudLog,
            capability: CaptureCapability::Supported,
            status: ProbeStatus::Unavailable,
            detail: "mangohud not on PATH; probe failure -> unavailable (AC-1.6)".into(),
            binary_path: None,
            injects_into_game: false,
            license_spdx: Some("MIT".into()),
        },
    }
}

#[cfg(target_os = "macos")]
fn probe_macos() -> ProbeReport {
    // Apple documents CVDisplayLink / CADisplayLink for an app's own render
    // loop, not for sampling another process's presents. Metal GPU capture
    // requires MTL_CAPTURE_ENABLED and is a debug tool, not a supported
    // external FPS path. External game frame times: not possible in v1.
    ProbeReport {
        os: "macos".into(),
        method_id: "macos_display_refresh_only".into(),
        method: CaptureMethod::MacosDisplayRefreshOnly,
        capability: CaptureCapability::Partial,
        status: ProbeStatus::NotPossible,
        detail: "external process frame times not possible via supported public APIs; display refresh (CVDisplayLink / CGDisplay) is partial only (AC-1.7)"
            .into(),
        binary_path: None,
        injects_into_game: false,
        license_spdx: None,
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
        let report = probe_frame_capture();
        let json = serde_json::to_string(&report).expect("serialize");
        let back: ProbeReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.method_id, report.method_id);
        assert_eq!(back.injects_into_game, false);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_external_capture_not_possible() {
        let report = probe_frame_capture();
        assert_eq!(report.status, ProbeStatus::NotPossible);
        assert_eq!(report.capability, CaptureCapability::Partial);
        assert_eq!(report.method, CaptureMethod::MacosDisplayRefreshOnly);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_method_is_presentmon() {
        let report = probe_frame_capture();
        assert_eq!(report.method, CaptureMethod::PresentMonEtw);
        assert_eq!(report.capability, CaptureCapability::Supported);
        assert!(!report.injects_into_game);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_method_is_mangohud() {
        let report = probe_frame_capture();
        assert_eq!(report.method, CaptureMethod::MangoHudLog);
        assert_eq!(report.capability, CaptureCapability::Supported);
        assert!(!report.injects_into_game);
    }
}
