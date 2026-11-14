//! macOS refresh rate check (HZ-026 / AC-4.22).
//!
//! Official sources (verified 2026-11-14):
//! - CGDisplayCopyDisplayMode / CGDisplayModeGetRefreshRate / CGDisplaySetDisplayMode
//!   https://developer.apple.com/documentation/coregraphics/cgdisplaymode/refreshrate
//!   https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/QuartzDisplayServicesConceptual/Articles/DisplayModes.html
//!
//! VRR (ProMotion adaptive sync) enablement is not exposed as a simple public
//! toggle for third-party apps in v1; status is labeled unavailable.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "macos.refresh_rate";
pub const SOURCE_URL: &str =
    "https://developer.apple.com/documentation/coregraphics/cgdisplaymode/refreshrate";
pub const VRR_NOTE: &str =
    "vrr=unavailable (no public third-party toggle for ProMotion/VRR enablement in v1)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsDisplayState {
    pub display_id: u32,
    pub width: u32,
    pub height: u32,
    /// Integer Hz; 0 means the mode reported 0 (common for some LCDs / default).
    pub refresh_hz: u32,
    pub refresh_available: bool,
}

impl MacOsDisplayState {
    pub fn encode(&self) -> String {
        format!(
            "display={};{}x{}@{}Hz;refresh_available={};{}",
            self.display_id,
            self.width,
            self.height,
            self.refresh_hz,
            if self.refresh_available { "1" } else { "0" },
            VRR_NOTE
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut display_id = None;
        let mut refresh_hz = None;
        let mut width = None;
        let mut height = None;
        let mut refresh_available = true;
        for part in raw.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("display=") {
                display_id = rest.parse().ok();
            } else if let Some(rest) = part.strip_prefix("refresh_available=") {
                refresh_available = rest == "1" || rest == "true";
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
            display_id: display_id.unwrap_or(0),
            width: width.unwrap_or(0),
            height: height.unwrap_or(0),
            refresh_hz: refresh_hz.ok_or_else(|| {
                ActionError::RestoreFailed(format!("invalid display snapshot: {raw}"))
            })?,
            refresh_available,
        })
    }
}

pub trait MacOsRefreshBackend {
    fn get(&self) -> Result<MacOsDisplayState, ActionError>;
    fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError>;
}

/// Labeled mock (not a real display mode change).
#[derive(Debug, Clone)]
pub struct MockMacOsRefreshBackend {
    pub state: MacOsDisplayState,
}

impl MacOsRefreshBackend for std::cell::RefCell<MockMacOsRefreshBackend> {
    fn get(&self) -> Result<MacOsDisplayState, ActionError> {
        Ok(self.borrow().state.clone())
    }

    fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if !b.state.refresh_available {
            return Err(ActionError::NotPossible(
                "refresh rate change not available in mock".into(),
            ));
        }
        b.state.refresh_hz = hz;
        Ok(())
    }
}

/// Report refresh rate; optionally apply a reversible Hz change (AC-4.22).
pub struct MacOsRefreshAction<B: MacOsRefreshBackend> {
    backend: B,
    target_hz: Option<u32>,
    explanation: ActionExplanation,
}

impl<B: MacOsRefreshBackend> MacOsRefreshAction<B> {
    pub fn new(backend: B, target_hz: Option<u32>) -> Self {
        let mut explanation = ActionExplanation::new(
            "Report the main display refresh rate via CGDisplayModeGetRefreshRate. Optionally switch to another supported mode with the same resolution. VRR/ProMotion enablement is unavailable via public APIs in v1.",
            "Matching refresh rate to the panel can reduce tearing for some setups. Effect is machine-specific; measure locally.",
            "Low--medium: a mode switch can blank the display briefly. VRR vendor toggles are not modified.",
            "Restore the previous refresh rate via this action, or System Settings > Displays.",
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

    pub fn exposes_toggle(&self) -> bool {
        self.target_hz.is_some()
    }
}

impl<B: MacOsRefreshBackend> Action for MacOsRefreshAction<B> {
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
            let _ = self.backend.get()?;
            return Ok(());
        };
        self.backend.set_refresh_hz(hz)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let prev = MacOsDisplayState::decode(before)?;
        if !prev.refresh_available {
            return Ok(());
        }
        self.backend.set_refresh_hz(prev.refresh_hz)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        if actual == expected {
            return Ok(VerifyResult::Matches);
        }
        let a = MacOsDisplayState::decode(&actual)?;
        let e = MacOsDisplayState::decode(expected)?;
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

#[cfg(target_os = "macos")]
pub mod os {
    use super::*;
    use std::ptr;

    type CGDirectDisplayID = u32;
    type CGDisplayModeRef = *mut std::ffi::c_void;
    type CGError = i32;
    type CFArrayRef = *const std::ffi::c_void;
    type CFIndex = isize;
    type CFDictionaryRef = *const std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        fn CGDisplayCopyDisplayMode(display: CGDirectDisplayID) -> CGDisplayModeRef;
        fn CGDisplayModeRelease(mode: CGDisplayModeRef);
        fn CGDisplayModeGetRefreshRate(mode: CGDisplayModeRef) -> f64;
        fn CGDisplayModeGetWidth(mode: CGDisplayModeRef) -> usize;
        fn CGDisplayModeGetHeight(mode: CGDisplayModeRef) -> usize;
        fn CGDisplayCopyAllDisplayModes(
            display: CGDirectDisplayID,
            options: CFDictionaryRef,
        ) -> CFArrayRef;
        fn CGDisplaySetDisplayMode(
            display: CGDirectDisplayID,
            mode: CGDisplayModeRef,
            options: CFDictionaryRef,
        ) -> CGError;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const std::ffi::c_void;
        fn CFRelease(cf: *const std::ffi::c_void);
    }

    pub struct OsMacOsRefreshBackend;

    impl MacOsRefreshBackend for OsMacOsRefreshBackend {
        fn get(&self) -> Result<MacOsDisplayState, ActionError> {
            let display = unsafe { CGMainDisplayID() };
            let mode = unsafe { CGDisplayCopyDisplayMode(display) };
            if mode.is_null() {
                return Err(ActionError::ApplyFailed(
                    "CGDisplayCopyDisplayMode returned null".into(),
                ));
            }
            let rate = unsafe { CGDisplayModeGetRefreshRate(mode) };
            let width = unsafe { CGDisplayModeGetWidth(mode) } as u32;
            let height = unsafe { CGDisplayModeGetHeight(mode) } as u32;
            unsafe { CGDisplayModeRelease(mode) };
            // Some LCD modes report 0; still a successful probe.
            Ok(MacOsDisplayState {
                display_id: display,
                width,
                height,
                refresh_hz: rate.round() as u32,
                refresh_available: true,
            })
        }

        fn set_refresh_hz(&self, hz: u32) -> Result<(), ActionError> {
            let display = unsafe { CGMainDisplayID() };
            let current = unsafe { CGDisplayCopyDisplayMode(display) };
            if current.is_null() {
                return Err(ActionError::ApplyFailed(
                    "CGDisplayCopyDisplayMode returned null".into(),
                ));
            }
            let cur_w = unsafe { CGDisplayModeGetWidth(current) };
            let cur_h = unsafe { CGDisplayModeGetHeight(current) };
            unsafe { CGDisplayModeRelease(current) };

            let modes = unsafe { CGDisplayCopyAllDisplayModes(display, ptr::null()) };
            if modes.is_null() {
                return Err(ActionError::NotPossible(
                    "CGDisplayCopyAllDisplayModes unavailable".into(),
                ));
            }
            let count = unsafe { CFArrayGetCount(modes) };
            let mut found: CGDisplayModeRef = ptr::null_mut();
            for i in 0..count {
                let mode = unsafe { CFArrayGetValueAtIndex(modes, i) } as CGDisplayModeRef;
                if mode.is_null() {
                    continue;
                }
                let w = unsafe { CGDisplayModeGetWidth(mode) };
                let h = unsafe { CGDisplayModeGetHeight(mode) };
                let rate = unsafe { CGDisplayModeGetRefreshRate(mode) }.round() as u32;
                if w == cur_w && h == cur_h && rate == hz {
                    found = mode;
                    break;
                }
            }
            if found.is_null() {
                unsafe { CFRelease(modes as *const _) };
                return Err(ActionError::NotPossible(format!(
                    "no display mode at {cur_w}x{cur_h}@{hz}Hz"
                )));
            }
            let err = unsafe { CGDisplaySetDisplayMode(display, found, ptr::null()) };
            unsafe { CFRelease(modes as *const _) };
            if err != 0 {
                return Err(ActionError::ApplyFailed(format!(
                    "CGDisplaySetDisplayMode failed: {err}"
                )));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{apply_with_snapshot, restore_with_log};
    use crate::snapshot::SnapshotStore;
    use std::cell::RefCell;

    fn mock_state(hz: u32) -> MacOsDisplayState {
        MacOsDisplayState {
            display_id: 1,
            width: 1920,
            height: 1080,
            refresh_hz: hz,
            refresh_available: true,
        }
    }

    #[test]
    fn report_only_includes_vrr_unavailable() {
        let action = MacOsRefreshAction::report_only(RefCell::new(MockMacOsRefreshBackend {
            state: mock_state(60),
        }));
        let encoded = action.read_state().expect("read");
        assert!(encoded.contains("@60Hz"));
        assert!(encoded.contains("vrr=unavailable"));
        assert!(!action.exposes_toggle());
    }

    #[test]
    fn set_hz_apply_restore_verify() {
        let mut store = SnapshotStore::new();
        let backend = RefCell::new(MockMacOsRefreshBackend {
            state: mock_state(60),
        });
        let mut action = MacOsRefreshAction::set_hz(backend, 120);
        assert!(action.exposes_toggle());
        let entry = apply_with_snapshot(&mut action, &mut store, 1).expect("apply");
        assert!(entry.before.contains("@60Hz"));
        assert!(action.read_state().unwrap().contains("@120Hz"));
        restore_with_log(&mut action, &mut store, entry.id).expect("restore");
        assert!(action.verify(&entry.before).expect("verify").is_match());
    }

    #[test]
    fn encode_decode_round_trip() {
        let s = mock_state(75);
        let again = MacOsDisplayState::decode(&s.encode()).expect("decode");
        assert_eq!(again.refresh_hz, 75);
        assert_eq!(again.width, 1920);
    }
}
