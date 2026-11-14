//! Honest macOS capability surface (HZ-025 / HZ-026 / AC-4.18, AC-4.23, AC-1.7).
//!
//! Matrix levels match `ARCHITECTURE.md` OS capability matrix (macOS column).
//! Not-possible entries never expose a toggle. HZ-026 enables toggles only where
//! a real documented apply path exists (background apps, optional refresh Hz).
//!
//! Sources (verified 2026-11-13 / 2026-11-14):
//! - CVDisplayLink (own-app display sync, not other-process FPS)
//!   https://developer.apple.com/documentation/corevideo/cvdisplaylink
//! - sysinfo 0.40.3 Gpu: macOS VRAM always None (shared memory)
//!   https://docs.rs/sysinfo/0.40.3/sysinfo/struct.Gpu.html
//! - D3 / D4 spikes in `core/spikes/`
//! - Game Mode: no public status API (WWDC25 / LSSupportsGameMode only)
//! - LPM: NSProcessInfo.isLowPowerModeEnabled / pmset
//! - Refresh: CGDisplayModeGetRefreshRate

/// Support level for one capability on macOS (ARCHITECTURE legend).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOsLevel {
    /// Supported target for v1.
    Supported,
    /// Partial / best-effort; missing pieces labeled.
    Partial,
    /// Not possible in v1; state plainly (AC-1.7 / AC-4.23).
    NotPossible,
}

impl MacOsLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "S",
            Self::Partial => "P",
            Self::NotPossible => "N",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOsCapabilityKind {
    Measurement,
    Action,
    Safety,
}

/// One row of the macOS capability surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsCapability {
    pub id: &'static str,
    pub kind: MacOsCapabilityKind,
    pub level: MacOsLevel,
    /// When false, UI must not show an apply switch (AC-4.23: no fake toggles).
    pub exposes_toggle: bool,
    pub summary: &'static str,
}

/// Plain honesty banner required by AC-4.23.
pub const MACOS_HONESTY_BANNER: &str = "macOS exposes far fewer tunables than Windows or Linux. \
Hertzline only lists supported public APIs; unsupported metrics and actions are labeled not possible. \
There are no placeholder fake options.";

/// Architecture matrix as of HZ-025 (macOS column only).
pub fn macos_capability_matrix() -> &'static [MacOsCapability] {
    &[
        // Measurement
        MacOsCapability {
            id: "measure.frame_times",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "display refresh only (CVDisplayLink / CGDisplay); external process FPS not possible via public APIs",
        },
        MacOsCapability {
            id: "measure.cpu_ram",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "CPU and RAM via sysinfo (MIT)",
        },
        MacOsCapability {
            id: "measure.gpu_vram_clocks_temps",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "GPU usage often available; dedicated VRAM not possible (shared memory); clocks/temps when OS exposes them",
        },
        MacOsCapability {
            id: "measure.throttling",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "throttle flags only when a documented source exposes them; otherwise unavailable",
        },
        MacOsCapability {
            id: "measure.disk_activity",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "disk throughput best-effort via sysinfo; queue pressure often unavailable",
        },
        MacOsCapability {
            id: "measure.network",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "latency / jitter / loss to a user-chosen host",
        },
        MacOsCapability {
            id: "measure.export_csv_json",
            kind: MacOsCapabilityKind::Measurement,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "session export with identical fields and units",
        },
        // Actions -- HZ-026 toggles only for real apply paths
        MacOsCapability {
            id: "action.power_plan",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "Low Power Mode warnings only in v1 (HZ-026); no fake power-plan switch",
        },
        MacOsCapability {
            id: "action.game_mode",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "Game Mode status not possible via public APIs; no fake toggle (AC-4.19)",
        },
        MacOsCapability {
            id: "action.feral_gamemode",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "Feral GameMode is Linux-only",
        },
        MacOsCapability {
            id: "action.process_priority",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "priority / affinity limited on macOS; deferred beyond HZ-025",
        },
        MacOsCapability {
            id: "action.background_apps",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: true,
            summary: "user-chosen PID pause/close via SIGSTOP/SIGTERM; restore SIGCONT or opt-in relaunch (AC-4.20)",
        },
        MacOsCapability {
            id: "action.startup_review",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "startup review partial; no silent disable",
        },
        MacOsCapability {
            id: "action.game_bar",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "Xbox Game Bar is Windows-only",
        },
        MacOsCapability {
            id: "action.temp_shader_cleanup",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "cleanup only with size preview when paths are documented",
        },
        MacOsCapability {
            id: "action.storage_health",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "free-space warnings when public APIs allow",
        },
        MacOsCapability {
            id: "action.official_driver_links",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "Apple / vendor official pages only; no third-party packs",
        },
        MacOsCapability {
            id: "action.refresh_vrr",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: true,
            summary: "refresh rate check/set via CGDisplayMode (AC-4.22); VRR unavailable",
        },
        MacOsCapability {
            id: "action.compositor_bypass",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "compositor bypass hints are not a macOS v1 target",
        },
        MacOsCapability {
            id: "action.mangohud",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "MangoHud is Linux-only",
        },
        MacOsCapability {
            id: "action.proton_wine",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "Proton / Wine info is Linux-only",
        },
        MacOsCapability {
            id: "action.overlay",
            kind: MacOsCapabilityKind::Action,
            level: MacOsLevel::Partial,
            exposes_toggle: false,
            summary: "optional overlay later (HZ-034); off by default with anti-cheat",
        },
        // Safety
        MacOsCapability {
            id: "safety.snapshot",
            kind: MacOsCapabilityKind::Safety,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "snapshot before mutate",
        },
        MacOsCapability {
            id: "safety.restore_all",
            kind: MacOsCapabilityKind::Safety,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "restore-all / crash / uninstall hooks",
        },
        MacOsCapability {
            id: "safety.system_restore",
            kind: MacOsCapabilityKind::Safety,
            level: MacOsLevel::NotPossible,
            exposes_toggle: false,
            summary: "Windows System Restore point offer does not apply on macOS",
        },
        MacOsCapability {
            id: "safety.change_log",
            kind: MacOsCapabilityKind::Safety,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "visible change log",
        },
        MacOsCapability {
            id: "safety.per_action_elevation",
            kind: MacOsCapabilityKind::Safety,
            level: MacOsLevel::Supported,
            exposes_toggle: false,
            summary: "SMAppService helper; elevation per named op only",
        },
    ]
}

/// Full surface for UI / docs: honesty banner + matrix rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsCapabilitySurface {
    pub honesty_banner: &'static str,
    pub capabilities: &'static [MacOsCapability],
}

impl MacOsCapabilitySurface {
    pub fn load() -> Self {
        Self {
            honesty_banner: MACOS_HONESTY_BANNER,
            capabilities: macos_capability_matrix(),
        }
    }

    /// Action catalog entries that may show an apply switch.
    pub fn toggleable_actions(&self) -> impl Iterator<Item = &MacOsCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.kind == MacOsCapabilityKind::Action && c.exposes_toggle)
    }

    pub fn not_possible(&self) -> impl Iterator<Item = &MacOsCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.level == MacOsLevel::NotPossible)
    }

    pub fn encode(&self) -> String {
        let mut parts = Vec::with_capacity(self.capabilities.len() + 1);
        parts.push(format!(
            "honesty={}",
            self.honesty_banner.replace(';', "%3B").replace('|', "%7C")
        ));
        for c in self.capabilities {
            parts.push(format!(
                "id={};level={};toggle={};kind={}",
                c.id,
                c.level.as_str(),
                if c.exposes_toggle { "1" } else { "0" },
                match c.kind {
                    MacOsCapabilityKind::Measurement => "measure",
                    MacOsCapabilityKind::Action => "action",
                    MacOsCapabilityKind::Safety => "safety",
                }
            ));
        }
        parts.join("|")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honesty_banner_states_fewer_tunables() {
        let surface = MacOsCapabilitySurface::load();
        assert!(surface.honesty_banner.contains("far fewer tunables"));
        assert!(surface.honesty_banner.contains("not possible"));
        assert!(surface
            .honesty_banner
            .contains("no placeholder fake options"));
    }

    #[test]
    fn not_possible_never_exposes_toggle() {
        let surface = MacOsCapabilitySurface::load();
        for c in surface.not_possible() {
            assert!(
                !c.exposes_toggle,
                "{} is N but exposes_toggle=true (forbidden fake option)",
                c.id
            );
        }
    }

    #[test]
    fn hz026_toggles_only_real_apply_paths() {
        // automated: Game Mode / LPM stay report-only; background + refresh may toggle
        let surface = MacOsCapabilitySurface::load();
        let ids: Vec<&str> = surface.toggleable_actions().map(|c| c.id).collect();
        assert_eq!(ids, vec!["action.background_apps", "action.refresh_vrr"]);
        let game = surface
            .capabilities
            .iter()
            .find(|c| c.id == "action.game_mode")
            .expect("game_mode");
        assert!(!game.exposes_toggle);
        let power = surface
            .capabilities
            .iter()
            .find(|c| c.id == "action.power_plan")
            .expect("power_plan");
        assert!(!power.exposes_toggle);
    }

    #[test]
    fn matrix_matches_architecture_macos_column() {
        let surface = MacOsCapabilitySurface::load();
        let level = |id: &str| {
            surface
                .capabilities
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.level)
                .unwrap_or_else(|| panic!("missing capability {id}"))
        };

        assert_eq!(level("measure.frame_times"), MacOsLevel::Partial);
        assert_eq!(level("measure.cpu_ram"), MacOsLevel::Supported);
        assert_eq!(level("measure.gpu_vram_clocks_temps"), MacOsLevel::Partial);
        assert_eq!(level("measure.throttling"), MacOsLevel::Partial);
        assert_eq!(level("measure.disk_activity"), MacOsLevel::Partial);
        assert_eq!(level("measure.network"), MacOsLevel::Supported);
        assert_eq!(level("measure.export_csv_json"), MacOsLevel::Supported);

        assert_eq!(level("action.power_plan"), MacOsLevel::Partial);
        assert_eq!(level("action.game_mode"), MacOsLevel::Partial);
        assert_eq!(level("action.feral_gamemode"), MacOsLevel::NotPossible);
        assert_eq!(level("action.process_priority"), MacOsLevel::Partial);
        assert_eq!(level("action.background_apps"), MacOsLevel::Partial);
        assert_eq!(level("action.startup_review"), MacOsLevel::Partial);
        assert_eq!(level("action.game_bar"), MacOsLevel::NotPossible);
        assert_eq!(level("action.temp_shader_cleanup"), MacOsLevel::Partial);
        assert_eq!(level("action.storage_health"), MacOsLevel::Partial);
        assert_eq!(level("action.official_driver_links"), MacOsLevel::Partial);
        assert_eq!(level("action.refresh_vrr"), MacOsLevel::Partial);
        assert_eq!(level("action.compositor_bypass"), MacOsLevel::NotPossible);
        assert_eq!(level("action.mangohud"), MacOsLevel::NotPossible);
        assert_eq!(level("action.proton_wine"), MacOsLevel::NotPossible);
        assert_eq!(level("action.overlay"), MacOsLevel::Partial);

        assert_eq!(level("safety.snapshot"), MacOsLevel::Supported);
        assert_eq!(level("safety.restore_all"), MacOsLevel::Supported);
        assert_eq!(level("safety.system_restore"), MacOsLevel::NotPossible);
        assert_eq!(level("safety.change_log"), MacOsLevel::Supported);
        assert_eq!(level("safety.per_action_elevation"), MacOsLevel::Supported);
    }

    #[test]
    fn encode_includes_honesty_and_levels() {
        let encoded = MacOsCapabilitySurface::load().encode();
        assert!(encoded.contains("honesty="));
        assert!(encoded.contains("id=measure.frame_times;level=P;toggle=0"));
        assert!(encoded.contains("id=action.game_bar;level=N;toggle=0"));
        assert!(encoded.contains("id=measure.cpu_ram;level=S;toggle=0"));
    }
}
