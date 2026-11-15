//! Per-OS overlay capture / display method (AC-6.1).
//!
//! Only documented, non-injection paths. PresentMon ETW feeds a Hertzline-owned
//! panel (Intel PresentMon: no Present hook / no game DLL inject). Linux uses
//! user-opted MangoHud wrap. macOS is partial (own-window metrics only).

use crate::frame_capture::{
    probe_frame_capture, CaptureCapability, CaptureMethod, ProbeReport, ProbeStatus,
};
use serde::{Deserialize, Serialize};

/// Stable overlay method id for settings / UI / logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayMethod {
    /// Windows: PresentMon ETW -> Hertzline-owned FPS / frame-time panel.
    PresentMonEtwPanel,
    /// Linux: user-launched `mangohud <app>` overlay + optional CSV ingest.
    MangoHudUserOptIn,
    /// macOS: Hertzline window with partial metrics (no external game FPS).
    MacOsPartialPanel,
    /// No supported overlay path on this host.
    Unsupported,
}

impl OverlayMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PresentMonEtwPanel => "presentmon_etw_panel",
            Self::MangoHudUserOptIn => "mangohud_user_opt_in",
            Self::MacOsPartialPanel => "macos_partial_panel",
            Self::Unsupported => "unsupported",
        }
    }

    /// Plain-language description for the UI.
    pub const fn summary(self) -> &'static str {
        match self {
            Self::PresentMonEtwPanel => {
                "PresentMon ETW feeds a Hertzline-owned FPS and frame-time panel (no game injection)"
            }
            Self::MangoHudUserOptIn => {
                "MangoHud overlay only when you launch with mangohud; Hertzline never silent-attaches"
            }
            Self::MacOsPartialPanel => {
                "macOS: Hertzline panel with partial metrics only; external game FPS is not possible"
            }
            Self::Unsupported => "no supported overlay method on this OS",
        }
    }

    /// PresentMon ETW and MangoHud user wrap are documented as non-injection.
    /// They may still be blocked by a title; Hertzline still defaults overlay off
    /// when anti-cheat markers are found (AC-6.2) unless the user opts in.
    pub const fn documented_non_injection(self) -> bool {
        match self {
            Self::PresentMonEtwPanel | Self::MangoHudUserOptIn => true,
            Self::MacOsPartialPanel => true,
            Self::Unsupported => false,
        }
    }

    /// Whether this method is considered allowed *for the anti-cheat context*
    /// (still off by default; see settings.allow_with_anti_cheat).
    pub const fn allowed_in_anti_cheat_context(self) -> bool {
        match self {
            Self::PresentMonEtwPanel | Self::MangoHudUserOptIn => true,
            // Own-window partial metrics never attach to the game.
            Self::MacOsPartialPanel => true,
            Self::Unsupported => false,
        }
    }
}

/// Probe result for the host overlay path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayMethodReport {
    pub method: OverlayMethod,
    pub method_id: String,
    pub capability: CaptureCapability,
    pub status: ProbeStatus,
    pub detail: String,
    pub injects_into_game: bool,
    pub license_spdx: Option<String>,
    pub frame_probe: ProbeReport,
}

/// Map the D3 frame probe onto an overlay method (AC-6.1).
pub fn probe_overlay_method() -> OverlayMethodReport {
    let frame = probe_frame_capture();
    let (method, detail) = match frame.method {
        CaptureMethod::PresentMonEtw => (
            OverlayMethod::PresentMonEtwPanel,
            format!(
                "{}; overlay draws in a Hertzline-owned panel from ETW samples (AC-6.1)",
                frame.detail
            ),
        ),
        CaptureMethod::MangoHudLog => (
            OverlayMethod::MangoHudUserOptIn,
            format!(
                "{}; in-game HUD is MangoHud's own overlay when you wrap the launch (AC-6.1)",
                frame.detail
            ),
        ),
        CaptureMethod::MacosDisplayRefreshOnly => (
            OverlayMethod::MacOsPartialPanel,
            format!(
                "{}; Hertzline panel only -- no external FPS overlay (AC-6.1 / AC-1.7)",
                frame.detail
            ),
        ),
        CaptureMethod::None => (OverlayMethod::Unsupported, frame.detail.clone()),
    };

    OverlayMethodReport {
        method,
        method_id: method.as_str().into(),
        capability: frame.capability,
        status: frame.status,
        detail,
        injects_into_game: false,
        license_spdx: frame.license_spdx.clone(),
        frame_probe: frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_never_claims_injection() {
        let report = probe_overlay_method();
        assert!(!report.injects_into_game);
        assert!(!report.frame_probe.injects_into_game);
        assert_eq!(report.method_id, report.method.as_str());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_method_is_presentmon_panel() {
        let report = probe_overlay_method();
        assert_eq!(report.method, OverlayMethod::PresentMonEtwPanel);
        assert!(report.method.documented_non_injection());
        assert!(report.method.allowed_in_anti_cheat_context());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_method_is_mangohud() {
        let report = probe_overlay_method();
        assert_eq!(report.method, OverlayMethod::MangoHudUserOptIn);
        assert!(report.method.documented_non_injection());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_method_is_partial() {
        let report = probe_overlay_method();
        assert_eq!(report.method, OverlayMethod::MacOsPartialPanel);
        assert_eq!(report.status, ProbeStatus::NotPossible);
        assert_eq!(report.capability, CaptureCapability::Partial);
    }
}
