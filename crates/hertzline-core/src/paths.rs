//! User-data paths for Hertzline state (HZ-018).
//!
//! Override with `HERTZLINE_SNAPSHOT_STORE` for tests, uninstall hooks, or
//! non-default installs. Default locations follow OS conventions without
//! inventing a roaming sync story.

use std::path::PathBuf;

/// Environment override for the snapshot / change-log JSON path.
pub const SNAPSHOT_STORE_ENV: &str = "HERTZLINE_SNAPSHOT_STORE";

/// Default file name under the Hertzline data directory.
pub const SNAPSHOT_STORE_FILE: &str = "snapshots.json";

/// Environment override for the Windows System Restore offer gate (AC-7.4).
pub const SYSTEM_RESTORE_GATE_ENV: &str = "HERTZLINE_SYSTEM_RESTORE_GATE";

/// Default file name for the System Restore offer / consent gate.
pub const SYSTEM_RESTORE_GATE_FILE: &str = "system_restore_gate.json";

/// Environment override for the active per-game profile marker (HZ-028 / AC-3.4).
pub const ACTIVE_PROFILE_ENV: &str = "HERTZLINE_ACTIVE_PROFILE";

/// Default file name for the active profile session marker.
pub const ACTIVE_PROFILE_FILE: &str = "active_profile.json";

/// Environment override for the measurement summary archive (HZ-033 / AC-5.6).
pub const MEASUREMENT_ARCHIVE_ENV: &str = "HERTZLINE_MEASUREMENT_ARCHIVE";

/// Default file name for archived measurement summaries (weekly health report).
pub const MEASUREMENT_ARCHIVE_FILE: &str = "measurement_archive.json";

/// Resolve the snapshot store path (env override wins).
pub fn snapshot_store_path() -> PathBuf {
    if let Ok(custom) = std::env::var(SNAPSHOT_STORE_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    hertzline_data_dir().join(SNAPSHOT_STORE_FILE)
}

/// Resolve the System Restore gate path (env override wins).
pub fn system_restore_gate_path() -> PathBuf {
    if let Ok(custom) = std::env::var(SYSTEM_RESTORE_GATE_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    hertzline_data_dir().join(SYSTEM_RESTORE_GATE_FILE)
}

/// Resolve the active profile marker path (env override wins).
pub fn active_profile_path() -> PathBuf {
    if let Ok(custom) = std::env::var(ACTIVE_PROFILE_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    hertzline_data_dir().join(ACTIVE_PROFILE_FILE)
}

/// Resolve the measurement archive path (env override wins).
pub fn measurement_archive_path() -> PathBuf {
    if let Ok(custom) = std::env::var(MEASUREMENT_ARCHIVE_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    hertzline_data_dir().join(MEASUREMENT_ARCHIVE_FILE)
}

/// Hertzline per-user data directory (created by callers that need it).
pub fn hertzline_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let trimmed = local.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("Hertzline");
            }
        }
        fallback_home()
            .join("AppData")
            .join("Local")
            .join("Hertzline")
    }
    #[cfg(target_os = "macos")]
    {
        fallback_home()
            .join("Library")
            .join("Application Support")
            .join("Hertzline")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("hertzline");
            }
        }
        fallback_home()
            .join(".local")
            .join("share")
            .join("hertzline")
    }
    #[cfg(not(any(windows, unix)))]
    {
        fallback_home().join("hertzline")
    }
}

fn fallback_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let trimmed = profile.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn env_override_wins() {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os(SNAPSHOT_STORE_ENV);
        std::env::set_var(SNAPSHOT_STORE_ENV, "C:\\tmp\\hz-test-snapshots.json");
        let path = snapshot_store_path();
        match previous {
            Some(value) => std::env::set_var(SNAPSHOT_STORE_ENV, value),
            None => std::env::remove_var(SNAPSHOT_STORE_ENV),
        }
        assert_eq!(path, PathBuf::from("C:\\tmp\\hz-test-snapshots.json"));
    }

    #[test]
    fn data_dir_is_non_empty() {
        let dir = hertzline_data_dir();
        assert!(!dir.as_os_str().is_empty());
        assert!(dir.ends_with("Hertzline") || dir.ends_with("hertzline"));
    }

    #[test]
    fn system_restore_gate_env_override_wins() {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os(SYSTEM_RESTORE_GATE_ENV);
        std::env::set_var(SYSTEM_RESTORE_GATE_ENV, "C:\\tmp\\hz-sr-gate.json");
        let path = system_restore_gate_path();
        match previous {
            Some(value) => std::env::set_var(SYSTEM_RESTORE_GATE_ENV, value),
            None => std::env::remove_var(SYSTEM_RESTORE_GATE_ENV),
        }
        assert_eq!(path, PathBuf::from("C:\\tmp\\hz-sr-gate.json"));
    }

    #[test]
    fn active_profile_env_override_wins() {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os(ACTIVE_PROFILE_ENV);
        std::env::set_var(ACTIVE_PROFILE_ENV, "C:\\tmp\\hz-active-profile.json");
        let path = active_profile_path();
        match previous {
            Some(value) => std::env::set_var(ACTIVE_PROFILE_ENV, value),
            None => std::env::remove_var(ACTIVE_PROFILE_ENV),
        }
        assert_eq!(path, PathBuf::from("C:\\tmp\\hz-active-profile.json"));
    }

    #[test]
    fn measurement_archive_env_override_wins() {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os(MEASUREMENT_ARCHIVE_ENV);
        std::env::set_var(MEASUREMENT_ARCHIVE_ENV, "C:\\tmp\\hz-measure-archive.json");
        let path = measurement_archive_path();
        match previous {
            Some(value) => std::env::set_var(MEASUREMENT_ARCHIVE_ENV, value),
            None => std::env::remove_var(MEASUREMENT_ARCHIVE_ENV),
        }
        assert_eq!(path, PathBuf::from("C:\\tmp\\hz-measure-archive.json"));
    }
}
