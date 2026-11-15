//! Persisted overlay preferences (AC-6.2, AC-6.3).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Environment override for the overlay settings JSON path.
pub const OVERLAY_SETTINGS_ENV: &str = "HERTZLINE_OVERLAY_SETTINGS";

/// Default file name under the Hertzline data directory.
pub const OVERLAY_SETTINGS_FILE: &str = "overlay_settings.json";

/// User preferences for the optional overlay.
///
/// Defaults: all flags false (optional; off; no anti-cheat opt-in).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OverlaySettings {
    /// One global kill switch (AC-6.3). When true, overlay never enables.
    pub globally_disabled: bool,
    /// Master preference: user wants the overlay when policy allows.
    /// Defaults to false (optional feature).
    pub user_enabled: bool,
    /// Explicit opt-in to show overlay when anti-cheat markers are present
    /// *and* the method is documented as non-injection (AC-6.2).
    pub allow_with_anti_cheat: bool,
}

impl OverlaySettings {
    /// Apply the global disable control (AC-6.3).
    pub fn disable_globally(&mut self) {
        self.globally_disabled = true;
        self.user_enabled = false;
    }

    /// Clear the global disable and leave other prefs unchanged.
    pub fn clear_global_disable(&mut self) {
        self.globally_disabled = false;
    }
}

/// Resolve the overlay settings path (env override wins).
pub fn overlay_settings_path() -> PathBuf {
    if let Ok(custom) = std::env::var(OVERLAY_SETTINGS_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    crate::paths::hertzline_data_dir().join(OVERLAY_SETTINGS_FILE)
}

/// Load settings from disk; missing file yields defaults.
pub fn load_settings(path: &Path) -> io::Result<OverlaySettings> {
    if !path.exists() {
        return Ok(OverlaySettings::default());
    }
    let text = fs::read_to_string(path)?;
    let settings: OverlaySettings =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(settings)
}

/// Persist settings (creates parent dirs).
pub fn save_settings(path: &Path, settings: &OverlaySettings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, text)
}

/// Load from the resolved default / env path.
pub fn load_host_settings() -> io::Result<OverlaySettings> {
    load_settings(&overlay_settings_path())
}

/// Save to the resolved default / env path.
pub fn save_host_settings(settings: &OverlaySettings) -> io::Result<()> {
    save_settings(&overlay_settings_path(), settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_file(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "hertzline-overlay-settings-{}-{}-{}.json",
            label,
            std::process::id(),
            n
        ))
    }

    #[test]
    fn defaults_are_off() {
        let s = OverlaySettings::default();
        assert!(!s.globally_disabled);
        assert!(!s.user_enabled);
        assert!(!s.allow_with_anti_cheat);
    }

    #[test]
    fn round_trip_disk() {
        let path = temp_file("round");
        let s = OverlaySettings {
            user_enabled: true,
            allow_with_anti_cheat: true,
            ..Default::default()
        };
        save_settings(&path, &s).unwrap();
        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded, s);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_default() {
        let path = temp_file("missing");
        let _ = fs::remove_file(&path);
        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded, OverlaySettings::default());
    }

    #[test]
    fn global_disable_clears_user_enabled() {
        let mut s = OverlaySettings {
            globally_disabled: false,
            user_enabled: true,
            allow_with_anti_cheat: true,
        };
        s.disable_globally();
        assert!(s.globally_disabled);
        assert!(!s.user_enabled);
    }

    #[test]
    fn env_override_wins() {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os(OVERLAY_SETTINGS_ENV);
        std::env::set_var(OVERLAY_SETTINGS_ENV, "C:\\tmp\\hz-overlay-settings.json");
        let path = overlay_settings_path();
        match previous {
            Some(value) => std::env::set_var(OVERLAY_SETTINGS_ENV, value),
            None => std::env::remove_var(OVERLAY_SETTINGS_ENV),
        }
        assert_eq!(path, PathBuf::from("C:\\tmp\\hz-overlay-settings.json"));
    }
}
