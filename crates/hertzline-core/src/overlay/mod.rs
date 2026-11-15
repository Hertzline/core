//! Optional FPS / frame-time overlay (HZ-034 / AC-6.1--AC-6.3).
//!
//! Supported methods only (PresentMon ETW panel / MangoHud user wrap / macOS
//! partial). Off by default when anti-cheat install markers are found. One
//! global disable control. Never injects into the game process.

mod anti_cheat;
mod decide;
mod method;
mod panel;
mod settings;

pub use anti_cheat::{
    declared as declare_anti_cheat, scan_install_path, AntiCheatKind, AntiCheatMarker,
    AntiCheatScan,
};
pub use decide::{
    decide_overlay, decide_with_method, OverlayBlockReason, OverlayContext, OverlayDecision,
};
pub use method::{probe_overlay_method, OverlayMethod, OverlayMethodReport};
pub use panel::{panel_from_frame_times_ms, panel_when_disabled, OverlayPanel, OverlayPanelStatus};
pub use settings::{
    load_host_settings, load_settings, overlay_settings_path, save_host_settings, save_settings,
    OverlaySettings, OVERLAY_SETTINGS_ENV, OVERLAY_SETTINGS_FILE,
};

use std::path::Path;

/// Convenience: decide overlay for an install path using host settings.
pub fn decide_for_install_path(
    settings: &OverlaySettings,
    install_path: Option<&Path>,
) -> OverlayDecision {
    let anti_cheat = install_path.map(scan_install_path);
    let context = OverlayContext { anti_cheat };
    decide_overlay(settings, &context)
}

/// Build the panel that the UI may show: disabled panel when policy blocks,
/// otherwise stats from supplied frame times (or unavailable).
pub fn resolve_panel(decision: &OverlayDecision, frame_times_ms: &[f64]) -> OverlayPanel {
    if !decision.enabled {
        return panel_when_disabled(&decision.message);
    }
    panel_from_frame_times_ms(frame_times_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_capture::{CaptureCapability, CaptureMethod, ProbeReport, ProbeStatus};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hertzline-overlay-mod-{}-{}-{}",
            label,
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mock_available_presentmon() -> OverlayMethodReport {
        // mock: available PresentMon panel; not a live probe
        OverlayMethodReport {
            method: OverlayMethod::PresentMonEtwPanel,
            method_id: OverlayMethod::PresentMonEtwPanel.as_str().into(),
            capability: CaptureCapability::Supported,
            status: ProbeStatus::Available,
            detail: "mock available".into(),
            injects_into_game: false,
            license_spdx: Some("MIT".into()),
            frame_probe: ProbeReport {
                os: "test".into(),
                method_id: "mock".into(),
                method: CaptureMethod::PresentMonEtw,
                capability: CaptureCapability::Supported,
                status: ProbeStatus::Available,
                detail: "mock".into(),
                binary_path: None,
                injects_into_game: false,
                license_spdx: Some("MIT".into()),
            },
        }
    }

    #[test]
    fn install_with_eac_defaults_off() {
        let root = temp_dir("eac");
        fs::create_dir_all(root.join("EasyAntiCheat")).unwrap();
        let settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: false,
        };
        let scan = scan_install_path(&root);
        assert!(scan.detected());
        let decision = decide_with_method(
            &settings,
            &OverlayContext {
                anti_cheat: Some(scan),
            },
            mock_available_presentmon(),
        );
        assert!(!decision.enabled);
        assert!(decision.anti_cheat_detected);
        assert_eq!(
            decision.block_reason,
            Some(OverlayBlockReason::AntiCheatDefaultOff)
        );
        let panel = resolve_panel(&decision, &[16.0; 30]);
        assert_eq!(panel.status, OverlayPanelStatus::Disabled);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn global_disable_is_one_control() {
        let mut settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: true,
        };
        settings.disable_globally();
        let decision = decide_for_install_path(&settings, None);
        assert!(!decision.enabled);
        assert_eq!(
            decision.block_reason,
            Some(OverlayBlockReason::GloballyDisabled)
        );
    }
}
