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
}
