//! Compositor bypass hints for documented sessions (AC-4.14).
//!
//! Official sources (verified 2026-11-14):
//! - KWin kcfg `WindowsBlockCompositing` under group Compositing (default true)
//!   https://lxr.kde.org/source/plasma/kwin/src/kwin.kcfg (stable-kf6-qt6)
//! - gamescope WSI: `GAMESCOPE_WSI_FORCE_BYPASS` nonzero forces XWayland bypass
//!   https://github.com/ValveSoftware/gamescope/blob/master/layer/VkLayer_FROG_gamescope_wsi.cpp
//!
//! Unsupported desktop environments report NotPossible (never invent a global
//! compositor kill). Preference for gamescope is stored under Hertzline config
//! for launch wrappers; KWin mutates `~/.config/kwinrc`.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.compositor_hint";
pub const SOURCE_URL_KWIN: &str =
    "https://lxr.kde.org/source/plasma/kwin/src/kwin.kcfg?v=stable-kf6-qt6";
pub const SOURCE_URL_GAMESCOPE: &str =
    "https://github.com/ValveSoftware/gamescope/blob/master/layer/VkLayer_FROG_gamescope_wsi.cpp";
pub const ENV_GAMESCOPE_FORCE_BYPASS: &str = "GAMESCOPE_WSI_FORCE_BYPASS";
pub const KWIN_KEY: &str = "WindowsBlockCompositing";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorSession {
    Kwin,
    Gamescope,
    Unsupported,
}

impl CompositorSession {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kwin => "kwin",
            Self::Gamescope => "gamescope",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, ActionError> {
        match raw.trim() {
            "kwin" => Ok(Self::Kwin),
            "gamescope" => Ok(Self::Gamescope),
            "unsupported" => Ok(Self::Unsupported),
            other => Err(ActionError::RestoreFailed(format!(
                "unknown compositor session: {other}"
            ))),
        }
    }
}

/// Encoded: `session=<kind>;desktop=<name>;hint=<value>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorHintState {
    pub session: CompositorSession,
    pub desktop: String,
    pub hint: String,
}

impl CompositorHintState {
    pub fn encode(&self) -> String {
        format!(
            "session={};desktop={};hint={}",
            self.session.as_str(),
            encode_field(&self.desktop),
            encode_field(&self.hint)
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ActionError> {
        let mut session = None;
        let mut desktop = None;
        let mut hint = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "session" => session = Some(CompositorSession::parse(v)?),
                "desktop" => desktop = Some(decode_field(v.trim())),
                "hint" => hint = Some(decode_field(v.trim())),
                _ => {}
            }
        }
        Ok(Self {
            session: session.ok_or_else(|| ActionError::RestoreFailed("missing session".into()))?,
            desktop: desktop.unwrap_or_default(),
            hint: hint.ok_or_else(|| ActionError::RestoreFailed("missing hint".into()))?,
        })
    }
}

fn encode_field(s: &str) -> String {
    s.replace(';', "%3B").replace('=', "%3D")
}

fn decode_field(s: &str) -> String {
    s.replace("%3B", ";").replace("%3D", "=")
}

/// Probe session and read/write the documented hint for that session.
pub trait CompositorBackend {
    fn detect(&self) -> Result<(CompositorSession, String), ActionError>;
    fn read_hint(&self, session: CompositorSession) -> Result<String, ActionError>;
    fn write_hint(&self, session: CompositorSession, hint: &str) -> Result<(), ActionError>;
}

/// Labeled mock for unit tests (not a real compositor).
#[derive(Debug, Clone)]
pub struct MockCompositorBackend {
    pub session: CompositorSession,
    pub desktop: String,
    pub hint: String,
}

impl MockCompositorBackend {
    pub fn kwin(hint: &str) -> Self {
        Self {
            session: CompositorSession::Kwin,
            desktop: "KDE".into(),
            hint: hint.into(),
        }
    }

    pub fn gamescope(hint: &str) -> Self {
        Self {
            session: CompositorSession::Gamescope,
            desktop: "gamescope".into(),
            hint: hint.into(),
        }
    }

    pub fn unsupported(desktop: &str) -> Self {
        Self {
            session: CompositorSession::Unsupported,
            desktop: desktop.into(),
            hint: "n/a".into(),
        }
    }
}

impl CompositorBackend for std::cell::RefCell<MockCompositorBackend> {
    fn detect(&self) -> Result<(CompositorSession, String), ActionError> {
        let b = self.borrow();
        Ok((b.session, b.desktop.clone()))
    }

    fn read_hint(&self, session: CompositorSession) -> Result<String, ActionError> {
        let b = self.borrow();
        if b.session != session && session != CompositorSession::Unsupported {
            return Err(ActionError::NotPossible("mock: session mismatch".into()));
        }
        Ok(b.hint.clone())
    }

    fn write_hint(&self, session: CompositorSession, hint: &str) -> Result<(), ActionError> {
        let mut b = self.borrow_mut();
        if b.session == CompositorSession::Unsupported {
            return Err(ActionError::NotPossible(format!(
                "mock: unsupported DE {}",
                b.desktop
            )));
        }
        if b.session != session {
            return Err(ActionError::ApplyFailed("mock: session mismatch".into()));
        }
        b.hint = hint.to_string();
        Ok(())
    }
}

/// Apply documented compositor bypass hint for the current session (AC-4.14).
pub struct CompositorHintAction<B: CompositorBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: CompositorBackend> CompositorHintAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "Enable the documented compositor bypass hint for the current session (KWin WindowsBlockCompositing, or gamescope GAMESCOPE_WSI_FORCE_BYPASS). Unsupported desktops are reported as not possible.",
            "May reduce compositor overhead for fullscreen games when the session honors the hint. Effect is machine-specific; measure locally. No FPS claim without a local measurement.",
            "Low--medium: KWin setting affects which apps may block compositing; gamescope env only applies when launch wrappers export it. Wrong DE reports not possible.",
            "Restore the previous hint value (kwinrc or Hertzline gamescope preference).",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    fn target_hint(session: CompositorSession) -> Result<&'static str, ActionError> {
        match session {
            CompositorSession::Kwin => Ok("true"),
            CompositorSession::Gamescope => Ok("1"),
            CompositorSession::Unsupported => Err(ActionError::NotPossible(
                "compositor bypass hints are not documented for this desktop environment".into(),
            )),
        }
    }
}

impl<B: CompositorBackend> Action for CompositorHintAction<B> {
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
        let (session, desktop) = self.backend.detect()?;
        let hint = if session == CompositorSession::Unsupported {
            "n/a".into()
        } else {
            self.backend.read_hint(session)?
        };
        Ok(CompositorHintState {
            session,
            desktop,
            hint,
        }
        .encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let (session, desktop) = self.backend.detect()?;
        if session == CompositorSession::Unsupported {
            return Err(ActionError::NotPossible(format!(
                "compositor bypass not possible for desktop '{desktop}'; see {SOURCE_URL_KWIN} / {SOURCE_URL_GAMESCOPE}"
            )));
        }
        let target = Self::target_hint(session)?;
        self.backend.write_hint(session, target)
    }

    fn restore(&mut self, before: &str) -> Result<(), ActionError> {
        let state = CompositorHintState::decode(before)?;
        if state.session == CompositorSession::Unsupported {
            return Ok(());
        }
        self.backend.write_hint(state.session, &state.hint)
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected_state = CompositorHintState::decode(expected)?;
        let actual_state = CompositorHintState::decode(&actual)?;
        if actual_state.session == expected_state.session
            && actual_state.hint == expected_state.hint
        {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.to_string(),
                actual,
            })
        }
    }
}

/// Classify desktop from XDG env values (no network).
pub fn classify_desktop(current_desktop: &str, gamescope_wayland: bool) -> CompositorSession {
    if gamescope_wayland {
        return CompositorSession::Gamescope;
    }
    let lower = current_desktop.to_ascii_lowercase();
    if lower.split(':').any(|p| p.trim() == "gamescope") || lower.contains("gamescope") {
        return CompositorSession::Gamescope;
    }
    if lower.split(':').any(|p| {
        let p = p.trim();
        p == "kde" || p == "plasma" || p == "plasmax11" || p == "plasmawayland"
    }) || lower.contains("kde")
        || lower.contains("plasma")
    {
        return CompositorSession::Kwin;
    }
    CompositorSession::Unsupported
}

#[cfg(target_os = "linux")]
pub mod os {
    use super::*;
    use crate::paths::hertzline_data_dir;
    use std::fs;
    use std::path::PathBuf;

    pub struct OsCompositorBackend;

    impl CompositorBackend for OsCompositorBackend {
        fn detect(&self) -> Result<(CompositorSession, String), ActionError> {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
            let gamescope = std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some()
                || std::env::var_os("GAMESCOPE_DISPLAY").is_some();
            let session = classify_desktop(&desktop, gamescope);
            let label = if desktop.trim().is_empty() {
                if gamescope {
                    "gamescope".into()
                } else {
                    "unknown".into()
                }
            } else {
                desktop
            };
            Ok((session, label))
        }

        fn read_hint(&self, session: CompositorSession) -> Result<String, ActionError> {
            match session {
                CompositorSession::Kwin => read_kwin_windows_block(),
                CompositorSession::Gamescope => read_gamescope_pref(),
                CompositorSession::Unsupported => Ok("n/a".into()),
            }
        }

        fn write_hint(&self, session: CompositorSession, hint: &str) -> Result<(), ActionError> {
            match session {
                CompositorSession::Kwin => write_kwin_windows_block(hint),
                CompositorSession::Gamescope => write_gamescope_pref(hint),
                CompositorSession::Unsupported => {
                    Err(ActionError::NotPossible("unsupported desktop".into()))
                }
            }
        }
    }

    fn kwinrc_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("kwinrc");
            }
        }
        dirs_home().join(".config").join("kwinrc")
    }

    fn dirs_home() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    fn gamescope_pref_path() -> PathBuf {
        hertzline_data_dir().join("gamescope_wsi_force_bypass")
    }

    fn read_gamescope_pref() -> Result<String, ActionError> {
        let path = gamescope_pref_path();
        if !path.exists() {
            return Ok("unset".into());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| ActionError::ApplyFailed(format!("read {}: {e}", path.display())))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Ok("unset".into())
        } else {
            Ok(trimmed.to_string())
        }
    }

    fn write_gamescope_pref(hint: &str) -> Result<(), ActionError> {
        let path = gamescope_pref_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ActionError::ApplyFailed(format!("mkdir {}: {e}", parent.display()))
            })?;
        }
        if hint == "unset" {
            let _ = fs::remove_file(&path);
            return Ok(());
        }
        fs::write(&path, format!("{hint}\n"))
            .map_err(|e| ActionError::ApplyFailed(format!("write {}: {e}", path.display())))?;
        Ok(())
    }

    fn read_kwin_windows_block() -> Result<String, ActionError> {
        let path = kwinrc_path();
        if !path.exists() {
            // kcfg default is true when key absent
            return Ok("true".into());
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| ActionError::ApplyFailed(format!("read kwinrc: {e}")))?;
        Ok(ini_get(&text, "Compositing", KWIN_KEY).unwrap_or_else(|| "true".into()))
    }

    fn write_kwin_windows_block(hint: &str) -> Result<(), ActionError> {
        let path = kwinrc_path();
        let existing = if path.exists() {
            fs::read_to_string(&path)
                .map_err(|e| ActionError::ApplyFailed(format!("read kwinrc: {e}")))?
        } else {
            String::new()
        };
        let updated = ini_set(&existing, "Compositing", KWIN_KEY, hint);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ActionError::ApplyFailed(format!("mkdir {}: {e}", parent.display()))
            })?;
        }
        fs::write(&path, updated)
            .map_err(|e| ActionError::ApplyFailed(format!("write kwinrc: {e}")))?;
        let _ = path;
        Ok(())
    }

    /// Minimal INI get for kwinrc-style files.
    pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
        let mut in_section = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_section = trimmed[1..trimmed.len() - 1] == *section;
                continue;
            }
            if !in_section || trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
        None
    }

    /// Minimal INI set; creates section/key when missing. Preserves other lines.
    pub fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
        let mut out = String::new();
        let mut in_section = false;
        let mut section_found = false;
        let mut key_written = false;
        let section_header = format!("[{section}]");
        let new_line = format!("{key}={value}");

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                if in_section && !key_written {
                    out.push_str(&new_line);
                    out.push('\n');
                    key_written = true;
                }
                in_section = trimmed == section_header;
                if in_section {
                    section_found = true;
                }
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if in_section {
                if let Some((k, _)) = trimmed.split_once('=') {
                    if k.trim() == key {
                        out.push_str(&new_line);
                        out.push('\n');
                        key_written = true;
                        continue;
                    }
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        if in_section && !key_written {
            out.push_str(&new_line);
            out.push('\n');
            key_written = true;
        }
        if !section_found {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&section_header);
            out.push('\n');
            out.push_str(&new_line);
            out.push('\n');
            let _ = key_written;
        }
        out
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn ini_round_trip_windows_block() {
            let text = "[Compositing]\nEnabled=true\nWindowsBlockCompositing=false\n";
            assert_eq!(
                ini_get(text, "Compositing", KWIN_KEY).as_deref(),
                Some("false")
            );
            let updated = ini_set(text, "Compositing", KWIN_KEY, "true");
            assert_eq!(
                ini_get(&updated, "Compositing", KWIN_KEY).as_deref(),
                Some("true")
            );
            assert!(updated.contains("Enabled=true"));
        }

        #[test]
        fn detect_is_honest_on_host() {
            let (session, desktop) = OsCompositorBackend.detect().expect("detect");
            let _ = (session, desktop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    #[test]
    fn mock_kwin_apply_restore_verify() {
        let backend = RefCell::new(MockCompositorBackend::kwin("false"));
        let mut action = CompositorHintAction::new(backend);
        let before = action.read_state().unwrap();
        assert!(before.contains("session=kwin"));
        assert!(before.contains("hint=false"));
        action.apply().unwrap();
        let after = action.read_state().unwrap();
        assert!(after.contains("hint=true"));
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn mock_gamescope_apply_restore() {
        let backend = RefCell::new(MockCompositorBackend::gamescope("unset"));
        let mut action = CompositorHintAction::new(backend);
        let before = action.read_state().unwrap();
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("hint=1"));
        action.restore(&before).unwrap();
        assert!(action.read_state().unwrap().contains("hint=unset"));
    }

    #[test]
    fn unsupported_de_is_not_possible() {
        let backend = RefCell::new(MockCompositorBackend::unsupported("GNOME"));
        let mut action = CompositorHintAction::new(backend);
        let state = action.read_state().unwrap();
        assert!(state.contains("session=unsupported"));
        let err = action.apply().unwrap_err();
        assert!(matches!(err, ActionError::NotPossible(_)));
    }

    #[test]
    fn classify_desktop_matrix() {
        assert_eq!(classify_desktop("KDE", false), CompositorSession::Kwin);
        assert_eq!(
            classify_desktop("ubuntu:GNOME", false),
            CompositorSession::Unsupported
        );
        assert_eq!(
            classify_desktop("gamescope", false),
            CompositorSession::Gamescope
        );
        assert_eq!(classify_desktop("", true), CompositorSession::Gamescope);
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockCompositorBackend::kwin("true"));
        let action = CompositorHintAction::new(backend);
        validate_explanation(action.explanation()).unwrap();
    }
}
