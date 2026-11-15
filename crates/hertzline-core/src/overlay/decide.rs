//! Overlay enable decision (AC-6.1--AC-6.3).

use super::anti_cheat::AntiCheatScan;
use super::method::{probe_overlay_method, OverlayMethod, OverlayMethodReport};
use super::settings::OverlaySettings;
use crate::frame_capture::ProbeStatus;
use serde::{Deserialize, Serialize};

/// Why the overlay is on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayBlockReason {
    GloballyDisabled,
    UserPreferenceOff,
    MethodUnavailable,
    MethodNotPossible,
    MethodUnsupported,
    AntiCheatDefaultOff,
    AntiCheatMethodNotAllowed,
}

impl OverlayBlockReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GloballyDisabled => "globally_disabled",
            Self::UserPreferenceOff => "user_preference_off",
            Self::MethodUnavailable => "method_unavailable",
            Self::MethodNotPossible => "method_not_possible",
            Self::MethodUnsupported => "method_unsupported",
            Self::AntiCheatDefaultOff => "anti_cheat_default_off",
            Self::AntiCheatMethodNotAllowed => "anti_cheat_method_not_allowed",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::GloballyDisabled => "overlay disabled globally (AC-6.3)",
            Self::UserPreferenceOff => "overlay preference is off",
            Self::MethodUnavailable => "overlay capture method unavailable on this machine",
            Self::MethodNotPossible => "overlay capture method not possible on this OS",
            Self::MethodUnsupported => "no supported overlay method",
            Self::AntiCheatDefaultOff => {
                "anti-cheat markers found; overlay off by default (AC-6.2)"
            }
            Self::AntiCheatMethodNotAllowed => {
                "overlay method is not documented as allowed with anti-cheat"
            }
        }
    }
}

/// Final policy decision for a game / session context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayDecision {
    pub enabled: bool,
    pub method: OverlayMethod,
    pub method_id: String,
    pub block_reason: Option<OverlayBlockReason>,
    pub message: String,
    pub anti_cheat_detected: bool,
    pub injects_into_game: bool,
    pub settings: OverlaySettings,
    pub method_report: OverlayMethodReport,
}

/// Optional game context for the decision.
#[derive(Debug, Clone, Default)]
pub struct OverlayContext {
    pub anti_cheat: Option<AntiCheatScan>,
}

/// Decide whether the overlay may show for this host + settings + context.
pub fn decide_overlay(settings: &OverlaySettings, context: &OverlayContext) -> OverlayDecision {
    let method_report = probe_overlay_method();
    decide_with_method(settings, context, method_report)
}

/// Same as [`decide_overlay`] with an injected method probe (tests / mocks).
pub fn decide_with_method(
    settings: &OverlaySettings,
    context: &OverlayContext,
    method_report: OverlayMethodReport,
) -> OverlayDecision {
    let anti_cheat_detected = context
        .anti_cheat
        .as_ref()
        .map(|s| s.detected())
        .unwrap_or(false);

    let base =
        |enabled: bool, block: Option<OverlayBlockReason>, message: String| OverlayDecision {
            enabled,
            method: method_report.method,
            method_id: method_report.method_id.clone(),
            block_reason: block,
            message,
            anti_cheat_detected,
            injects_into_game: false,
            settings: settings.clone(),
            method_report: method_report.clone(),
        };

    if settings.globally_disabled {
        return base(
            false,
            Some(OverlayBlockReason::GloballyDisabled),
            OverlayBlockReason::GloballyDisabled.message().into(),
        );
    }

    if !settings.user_enabled {
        return base(
            false,
            Some(OverlayBlockReason::UserPreferenceOff),
            OverlayBlockReason::UserPreferenceOff.message().into(),
        );
    }

    if method_report.method == OverlayMethod::Unsupported {
        return base(
            false,
            Some(OverlayBlockReason::MethodUnsupported),
            OverlayBlockReason::MethodUnsupported.message().into(),
        );
    }

    // AC-6.2 before method availability so the anti-cheat default stays visible
    // even when PresentMon / MangoHud is missing on the host.
    if anti_cheat_detected {
        if !method_report.method.allowed_in_anti_cheat_context() {
            return base(
                false,
                Some(OverlayBlockReason::AntiCheatMethodNotAllowed),
                OverlayBlockReason::AntiCheatMethodNotAllowed
                    .message()
                    .into(),
            );
        }
        if !settings.allow_with_anti_cheat {
            let detail = context
                .anti_cheat
                .as_ref()
                .map(|s| s.note.clone())
                .unwrap_or_else(|| OverlayBlockReason::AntiCheatDefaultOff.message().into());
            return base(false, Some(OverlayBlockReason::AntiCheatDefaultOff), detail);
        }
    }

    match method_report.status {
        ProbeStatus::Unavailable => {
            return base(
                false,
                Some(OverlayBlockReason::MethodUnavailable),
                format!(
                    "{}; {}",
                    OverlayBlockReason::MethodUnavailable.message(),
                    method_report.detail
                ),
            );
        }
        ProbeStatus::NotPossible => {
            // macOS partial panel can still show non-FPS honesty text when the
            // user enables overlay; external FPS remains not possible.
            if method_report.method != OverlayMethod::MacOsPartialPanel {
                return base(
                    false,
                    Some(OverlayBlockReason::MethodNotPossible),
                    format!(
                        "{}; {}",
                        OverlayBlockReason::MethodNotPossible.message(),
                        method_report.detail
                    ),
                );
            }
        }
        ProbeStatus::Available => {}
    }

    let message = if anti_cheat_detected {
        format!(
            "overlay enabled with user anti-cheat opt-in; method={} (non-injection)",
            method_report.method.as_str()
        )
    } else {
        format!(
            "overlay enabled; method={} (non-injection)",
            method_report.method.as_str()
        )
    };

    base(true, None, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_capture::{CaptureCapability, CaptureMethod, ProbeReport, ProbeStatus};
    use crate::overlay::anti_cheat::{declared, AntiCheatKind};

    fn mock_method(method: OverlayMethod, status: ProbeStatus) -> OverlayMethodReport {
        // mock: synthetic probe; not a live OS capture
        OverlayMethodReport {
            method,
            method_id: method.as_str().into(),
            capability: CaptureCapability::Supported,
            status,
            detail: "mock overlay method".into(),
            injects_into_game: false,
            license_spdx: Some("MIT".into()),
            frame_probe: ProbeReport {
                os: "test".into(),
                method_id: "mock".into(),
                method: CaptureMethod::PresentMonEtw,
                capability: CaptureCapability::Supported,
                status,
                detail: "mock".into(),
                binary_path: None,
                injects_into_game: false,
                license_spdx: Some("MIT".into()),
            },
        }
    }

    #[test]
    fn global_disable_wins() {
        let settings = OverlaySettings {
            globally_disabled: true,
            user_enabled: true,
            allow_with_anti_cheat: true,
        };
        let d = decide_with_method(
            &settings,
            &OverlayContext::default(),
            mock_method(OverlayMethod::PresentMonEtwPanel, ProbeStatus::Available),
        );
        assert!(!d.enabled);
        assert_eq!(d.block_reason, Some(OverlayBlockReason::GloballyDisabled));
        assert!(!d.injects_into_game);
    }

    #[test]
    fn preference_off_blocks() {
        let settings = OverlaySettings::default();
        let d = decide_with_method(
            &settings,
            &OverlayContext::default(),
            mock_method(OverlayMethod::PresentMonEtwPanel, ProbeStatus::Available),
        );
        assert!(!d.enabled);
        assert_eq!(d.block_reason, Some(OverlayBlockReason::UserPreferenceOff));
    }

    #[test]
    fn anti_cheat_off_by_default() {
        let settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: false,
        };
        let ctx = OverlayContext {
            anti_cheat: Some(declared(AntiCheatKind::EasyAntiCheat)),
        };
        let d = decide_with_method(
            &settings,
            &ctx,
            mock_method(OverlayMethod::PresentMonEtwPanel, ProbeStatus::Available),
        );
        assert!(!d.enabled);
        assert!(d.anti_cheat_detected);
        assert_eq!(
            d.block_reason,
            Some(OverlayBlockReason::AntiCheatDefaultOff)
        );
    }

    #[test]
    fn anti_cheat_opt_in_allows_documented_method() {
        let settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: true,
        };
        let ctx = OverlayContext {
            anti_cheat: Some(declared(AntiCheatKind::BattlEye)),
        };
        let d = decide_with_method(
            &settings,
            &ctx,
            mock_method(OverlayMethod::PresentMonEtwPanel, ProbeStatus::Available),
        );
        assert!(d.enabled);
        assert!(d.anti_cheat_detected);
        assert!(d.block_reason.is_none());
        assert!(!d.injects_into_game);
    }

    #[test]
    fn unavailable_method_blocks_even_when_enabled() {
        let settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: false,
        };
        let d = decide_with_method(
            &settings,
            &OverlayContext::default(),
            mock_method(OverlayMethod::MangoHudUserOptIn, ProbeStatus::Unavailable),
        );
        assert!(!d.enabled);
        assert_eq!(d.block_reason, Some(OverlayBlockReason::MethodUnavailable));
    }

    #[test]
    fn enabled_without_anti_cheat() {
        let settings = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: false,
        };
        let d = decide_with_method(
            &settings,
            &OverlayContext::default(),
            mock_method(OverlayMethod::PresentMonEtwPanel, ProbeStatus::Available),
        );
        assert!(d.enabled);
        assert!(!d.anti_cheat_detected);
    }
}
