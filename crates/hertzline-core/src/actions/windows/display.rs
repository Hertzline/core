//! Refresh rate check and reversible change; VRR status honesty (AC-4.10).
//!
//! Official APIs (verified 2026-11-14):
//! - EnumDisplaySettingsW / ChangeDisplaySettingsExW (dmDisplayFrequency)
//!   https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaysettingsw
//!   https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changedisplaysettingsexw
//! - QueryDisplayConfig / GetDisplayConfigBufferSizes (CCD refresh readout)
//!   https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig
//!
//! VRR (G-SYNC / FreeSync) enablement is not exposed by public Win32/CCD APIs.
//! Vendor SDKs (for example NVAPI) are out of scope for v1. Status is labeled unavailable.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.display_refresh";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changedisplaysettingsexw";
pub const VRR_NOTE: &str =
    "vrr=unavailable (no public Win32/CCD API for FreeSync/G-SYNC enablement in v1)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayState {
    pub device: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

impl DisplayState {
    pub fn encode(&self) -> String {
        format!(
            "device={};{}x{}@{}Hz;{}",
            encode_field(&self.device),
            self.width,
            self.height,
            self.refresh_hz,
            VRR_NOTE
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut device = None;
        let mut refresh_hz = None;
        let mut width = None;
        let mut height = None;
        for part in raw.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("device=") {
                device = Some(decode_field(rest));
            } else if let Some((wh, hz)) = part.split_once('@') {
                if let Some((w, h)) = wh.split_once('x') {
                    width = w.parse().ok();
                    height = h.parse().ok();
                }
                if let Some(n) = hz.strip_suffix("Hz") {
                    refresh_hz = n.parse().ok();
                }
            }
        }
        Ok(Self {
            device: device.unwrap_or_default(),
            width: width.unwrap_or(0),
            height: height.unwrap_or(0),
            refresh_hz: refresh_hz.ok_or_else(|| {
                ActionError::RestoreFailed(format!("invalid display snapshot: {raw}"))
            })?,
        })
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/").replace(';', "%3B").replace('=', "%3D")
}

fn decode_field(s: &str) -> String {
    s.replace("%3D", "=").replace("%3B", ";").replace('/', "\\")
}

pub trait DisplayBackend {
    fn get(&self) -> Result<DisplayState, ActionError>;
    fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real display mode change).
#[derive(Debug, Clone)]
pub struct MockDisplayBackend {
    pub state: DisplayState,
}

impl DisplayBackend for std::cell::RefCell<MockDisplayBackend> {
    fn get(&self) -> Result<DisplayState, ActionError> {
        Ok(self.borrow().state.clone())
    }

    fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError> {
        self.borrow_mut().state.refresh_hz = hz;
        Ok(())
    }
}

/// Report refresh rate; optionally apply a reversible Hz change (AC-4.10).
pub struct DisplayRefreshAction<B: DisplayBackend> {
    backend: B,
    target_hz: Option<u32>,
    explanation: ActionExplanation,
}

impl<B: DisplayBackend> DisplayRefreshAction<B> {
    pub fn new(backend: B, target_hz: Option<u32>) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report the active display refresh rate. Optionally set a supported refresh rate. VRR status is unavailable via public Win32 APIs.",
            "Matching refresh rate to the panel can reduce tearing for some setups. Effect is machine-specific; measure locally. VRR cannot be toggled here.",
            "Low--medium: a bad mode can blank the display briefly; Windows usually reverts. VRR vendor toggles are not modified.",
            "Restore the previous refresh rate via this action, or Settings > System > Display > Advanced display.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            target_hz,
            explanation,
        }
    }

    pub fn report_only(backend: B) -> Self {
        Self::new(backend, None)
    }

    pub fn set_hz(backend: B, hz: u32) -> Self {
        Self::new(backend, Some(hz))
    }
}

impl<B: DisplayBackend> Action for DisplayRefreshAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        false
    }

    fn read_state(&self) -> Result<String, ActionError> {
        Ok(self.backend.get()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let Some(hz) = self.target_hz else {
            // Report-only apply: probe current mode.
            let _ = self.backend.get()?;
            return Ok(());
        };
        self.backend.set_refresh_hz(hz)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let prev = DisplayState::decode(before)?;
        self.backend.set_refresh_hz(prev.refresh_hz)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        if actual == expected {
            return Ok(VerifyResult::Matches);
        }
        // Compare refresh Hz when full string drifts (device path encoding).
        let a = DisplayState::decode(&actual)?;
        let e = DisplayState::decode(expected)?;
        if a.refresh_hz == e.refresh_hz {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const ENUM_CURRENT_SETTINGS: u32 = 0xFFFF_FFFF;
    const DM_DISPLAYFREQUENCY: u32 = 0x0040_0000;
    const DM_PELSWIDTH: u32 = 0x0008_0000;
    const DM_PELSHEIGHT: u32 = 0x0010_0000;
    const CDS_UPDATEREGISTRY: u32 = 0x0000_0001;
    const DISP_CHANGE_SUCCESSFUL: i32 = 0;

    #[repr(C)]
    struct DevModeW {
        dm_device_name: [u16; 32],
        dm_spec_version: u16,
        dm_driver_version: u16,
        dm_size: u16,
        dm_driver_extra: u16,
        dm_fields: u32,
        dm_position_x: i32,
        dm_position_y: i32,
        dm_display_orientation: u32,
        dm_display_fixed_output: u32,
        dm_color: i16,
        dm_duplex: i16,
        dm_y_resolution: i16,
        dm_tt_option: i16,
        dm_collate: i16,
        dm_form_name: [u16; 32],
        dm_log_pixels: u16,
        dm_bits_per_pel: u32,
        dm_pels_width: u32,
        dm_pels_height: u32,
        dm_display_flags: u32,
        dm_display_frequency: u32,
        dm_icm_method: u32,
        dm_icm_intent: u32,
        dm_media_type: u32,
        dm_dither_type: u32,
        dm_reserved1: u32,
        dm_reserved2: u32,
        dm_panning_width: u32,
        dm_panning_height: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplaySettingsW(device: *const u16, mode_num: u32, dev_mode: *mut DevModeW) -> i32;
        fn ChangeDisplaySettingsExW(
            device: *const u16,
            dev_mode: *mut DevModeW,
            hwnd: *mut core::ffi::c_void,
            flags: u32,
            param: *mut core::ffi::c_void,
        ) -> i32;
    }

    pub struct OsDisplayBackend;

    impl DisplayBackend for OsDisplayBackend {
        fn get(&self) -> Result<DisplayState, ActionError> {
            let mut mode: DevModeW = unsafe { mem::zeroed() };
            mode.dm_size = mem::size_of::<DevModeW>() as u16;
            let ok = unsafe { EnumDisplaySettingsW(ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) };
            if ok == 0 {
                return Err(ActionError::ApplyFailed(
                    "EnumDisplaySettingsW failed for current mode".into(),
                ));
            }
            Ok(DisplayState {
                device: "primary".into(),
                width: mode.dm_pels_width,
                height: mode.dm_pels_height,
                refresh_hz: mode.dm_display_frequency,
            })
        }

        fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError> {
            let mut mode: DevModeW = unsafe { mem::zeroed() };
            mode.dm_size = mem::size_of::<DevModeW>() as u16;
            let ok = unsafe { EnumDisplaySettingsW(ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) };
            if ok == 0 {
                return Err(ActionError::ApplyFailed(
                    "EnumDisplaySettingsW failed before set".into(),
                ));
            }
            mode.dm_fields = DM_DISPLAYFREQUENCY | DM_PELSWIDTH | DM_PELSHEIGHT;
            mode.dm_display_frequency = hz;
            let status = unsafe {
                ChangeDisplaySettingsExW(
                    ptr::null(),
                    &mut mode,
                    ptr::null_mut(),
                    CDS_UPDATEREGISTRY,
                    ptr::null_mut(),
                )
            };
            if status != DISP_CHANGE_SUCCESSFUL {
                return Err(ActionError::ApplyFailed(format!(
                    "ChangeDisplaySettingsExW failed ({status}); see {SOURCE_URL}"
                )));
            }
            Ok(())
        }
    }

    // Silence unused wide helper warning if we later pass device names.
    #[allow(dead_code)]
    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_display_reports_refresh_and_vrr_unavailable() {
            let state = OsDisplayBackend.get().expect("display");
            assert!(state.refresh_hz > 0, "refresh {}", state.refresh_hz);
            let encoded = state.encode();
            assert!(encoded.contains("vrr=unavailable"));
            assert!(encoded.contains("Hz"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log, validate_explanation};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    #[test]
    fn mock_apply_restore_refresh() {
        // mock: in-memory Hz only
        let backend = RefCell::new(MockDisplayBackend {
            state: DisplayState {
                device: "primary".into(),
                width: 1920,
                height: 1080,
                refresh_hz: 60,
            },
        });
        let mut action = DisplayRefreshAction::set_hz(backend, 144);
        assert!(action.read_state().unwrap().contains("@60Hz"));
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("@144Hz"));
        action
            .restore("device=primary;1920x1080@60Hz;vrr=unavailable")
            .unwrap();
        let state = action.read_state().unwrap();
        assert!(state.contains("@60Hz"));
        assert!(state.contains("vrr=unavailable"));
    }

    #[test]
    fn snapshot_round_trip() {
        let backend = RefCell::new(MockDisplayBackend {
            state: DisplayState {
                device: "primary".into(),
                width: 1920,
                height: 1080,
                refresh_hz: 60,
            },
        });
        let mut action = DisplayRefreshAction::set_hz(backend, 120);
        let mut store = SnapshotStore::new();
        let entry = apply_with_snapshot(&mut action, &mut store, 1).unwrap();
        restore_with_log(&mut action, &mut store, entry.id).unwrap();
        assert!(action.read_state().unwrap().contains("@60Hz"));
    }

    #[test]
    fn explanation_mentions_vrr_honesty() {
        let backend = RefCell::new(MockDisplayBackend {
            state: DisplayState {
                device: "primary".into(),
                width: 1,
                height: 1,
                refresh_hz: 60,
            },
        });
        let action = DisplayRefreshAction::report_only(backend);
        validate_explanation(action.explanation()).unwrap();
        assert!(action.explanation().what.contains("VRR"));
    }
}
