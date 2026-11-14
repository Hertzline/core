//! Linux shader-cache cleanup with size preview (AC-4.17).
//!
//! Documented paths (verified 2026-11-14):
//! - Steam: `<SteamRoot>/steamapps/shadercache` (Valve / Proton issues)
//!   https://github.com/ValveSoftware/Proton/issues/8926
//! - Mesa default cache: `$XDG_CACHE_HOME/mesa_shader_cache` (+ `_db`)
//!   Mesa `MESA_SHADER_CACHE_DIR` docs
//!
//! Never touches `compatdata` (Proton prefixes / saves). Apply deletes only
//! after preview and opt-in. Deletion is irreversible.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.shader_cache_cleanup";
pub const SOURCE_URL: &str = "https://github.com/ValveSoftware/Proton/issues/8926";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderCacheEntry {
    pub label: String,
    pub path: String,
    pub bytes: u64,
}

impl ShaderCacheEntry {
    pub fn encode(&self) -> String {
        format!(
            "{}={}@{}",
            encode_field(&self.label),
            self.bytes,
            encode_field(&self.path)
        )
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/").replace(';', "%3B").replace('=', "%3D")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderCachePreview {
    pub entries: Vec<ShaderCacheEntry>,
    pub total_bytes: u64,
}

impl ShaderCachePreview {
    pub fn encode(&self) -> String {
        let parts: Vec<String> = self.entries.iter().map(ShaderCacheEntry::encode).collect();
        format!("total={};{}", self.total_bytes, parts.join(";"))
    }

    pub fn from_entries(entries: Vec<ShaderCacheEntry>) -> Self {
        let total_bytes = entries.iter().map(|e| e.bytes).sum();
        Self {
            entries,
            total_bytes,
        }
    }
}

pub trait ShaderCacheBackend {
    fn preview(&self) -> Result<ShaderCachePreview, ActionError>;
    fn apply_delete(&self) -> Result<u64, ActionError>;
}

/// Labeled mock for unit tests (not a real filesystem cleanup).
#[derive(Debug, Clone)]
pub struct MockShaderCacheBackend {
    pub entries: Vec<ShaderCacheEntry>,
    pub deleted: bool,
}

impl MockShaderCacheBackend {
    pub fn with_entries(entries: Vec<ShaderCacheEntry>) -> Self {
        Self {
            entries,
            deleted: false,
        }
    }
}

impl ShaderCacheBackend for std::cell::RefCell<MockShaderCacheBackend> {
    fn preview(&self) -> Result<ShaderCachePreview, ActionError> {
        let b = self.borrow();
        if b.deleted {
            return Ok(ShaderCachePreview::from_entries(
                b.entries
                    .iter()
                    .map(|e| ShaderCacheEntry {
                        label: e.label.clone(),
                        path: e.path.clone(),
                        bytes: 0,
                    })
                    .collect(),
            ));
        }
        Ok(ShaderCachePreview::from_entries(b.entries.clone()))
    }

    fn apply_delete(&self) -> Result<u64, ActionError> {
        let mut b = self.borrow_mut();
        let total = b.entries.iter().map(|e| e.bytes).sum();
        b.deleted = true;
        Ok(total)
    }
}

/// Opt-in shader cache cleanup (AC-4.17). Requires `confirm=true` before apply.
pub struct ShaderCacheAction<B: ShaderCacheBackend> {
    backend: B,
    confirm: bool,
    explanation: ActionExplanation,
}

impl<B: ShaderCacheBackend> ShaderCacheAction<B> {
    pub fn new(backend: B, confirm: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Preview and optionally delete Steam shadercache and Mesa shader cache directories. Never deletes Proton compatdata.",
            "Can free disk space; games may hitch while shaders recompile. No FPS claim without a local measurement.",
            "Medium: deleted caches cannot be restored. First launches after cleanup may stutter.",
            "Deletion is irreversible. Restore is not possible; re-launch games to rebuild caches.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            confirm,
            explanation,
        }
    }

    pub fn preview_only(backend: B) -> Self {
        Self::new(backend, false)
    }

    pub fn confirmed(backend: B) -> Self {
        Self::new(backend, true)
    }

    pub fn preview(&self) -> Result<ShaderCachePreview, ActionError> {
        self.backend.preview()
    }
}

impl<B: ShaderCacheBackend> Action for ShaderCacheAction<B> {
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
        Ok(self.backend.preview()?.encode())
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        if !self.confirm {
            return Err(ActionError::ApplyFailed(
                "shader cache cleanup requires opt-in confirm=true after size preview (AC-4.17)"
                    .into(),
            ));
        }
        let preview = self.backend.preview()?;
        if preview.total_bytes == 0 {
            return Ok(());
        }
        let _removed = self.backend.apply_delete()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Err(ActionError::NotPossible(
            "shader cache cleanup deletes files; restore is not possible (AC-4.17)".into(),
        ))
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        if actual == expected.trim() {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.trim().into(),
                actual,
            })
        }
    }
}

#[cfg(target_os = "linux")]
pub mod os {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    pub struct OsShaderCacheBackend;

    impl ShaderCacheBackend for OsShaderCacheBackend {
        fn preview(&self) -> Result<ShaderCachePreview, ActionError> {
            Ok(ShaderCachePreview::from_entries(collect_entries()))
        }

        fn apply_delete(&self) -> Result<u64, ActionError> {
            let entries = collect_entries();
            let mut removed = 0u64;
            for entry in entries {
                removed = removed.saturating_add(clear_dir_contents(Path::new(&entry.path))?);
            }
            Ok(removed)
        }
    }

    fn collect_entries() -> Vec<ShaderCacheEntry> {
        let mut out = Vec::new();
        for root in steam_roots() {
            let path = root.join("steamapps").join("shadercache");
            if path.is_dir() {
                out.push(ShaderCacheEntry {
                    label: "steam_shadercache".into(),
                    path: path.display().to_string(),
                    bytes: dir_size(&path),
                });
            }
        }
        let cache_home = xdg_cache_home();
        for (label, name) in [
            ("mesa_shader_cache", "mesa_shader_cache"),
            ("mesa_shader_cache_db", "mesa_shader_cache_db"),
        ] {
            let path = cache_home.join(name);
            if path.is_dir() {
                out.push(ShaderCacheEntry {
                    label: label.into(),
                    path: path.display().to_string(),
                    bytes: dir_size(&path),
                });
            }
        }
        out
    }

    fn steam_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        for rel in [
            ".steam/root",
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        ] {
            let path = home.join(rel);
            if path.is_dir() {
                let key = fs::canonicalize(&path).unwrap_or(path.clone());
                if !roots
                    .iter()
                    .any(|r| fs::canonicalize(r).unwrap_or_else(|_| r.clone()) == key)
                {
                    roots.push(path);
                }
            }
        }
        roots
    }

    fn xdg_cache_home() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".cache"))
            .unwrap_or_else(|_| PathBuf::from(".cache"))
    }

    fn dir_size(path: &Path) -> u64 {
        let mut total = 0u64;
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total = total.saturating_add(dir_size(&path));
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
        total
    }

    fn clear_dir_contents(path: &Path) -> Result<u64, ActionError> {
        if !path.is_dir() {
            return Ok(0);
        }
        // Refuse anything that looks like compatdata
        let display = path.display().to_string();
        if display.contains("compatdata") {
            return Err(ActionError::ApplyFailed(
                "refusing to delete compatdata path".into(),
            ));
        }
        let mut removed = 0u64;
        let entries = fs::read_dir(path)
            .map_err(|e| ActionError::ApplyFailed(format!("read_dir {}: {e}", path.display())))?;
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                removed = removed.saturating_add(dir_size(&child));
                let _ = fs::remove_dir_all(&child);
            } else if let Ok(meta) = entry.metadata() {
                removed = removed.saturating_add(meta.len());
                let _ = fs::remove_file(&child);
            }
        }
        Ok(removed)
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn preview_does_not_panic() {
            let preview = OsShaderCacheBackend.preview().expect("preview");
            let _ = preview.total_bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    fn sample_entries() -> Vec<ShaderCacheEntry> {
        vec![
            ShaderCacheEntry {
                label: "steam_shadercache".into(),
                path: "/mock/.steam/steam/steamapps/shadercache".into(),
                bytes: 2000,
            },
            ShaderCacheEntry {
                label: "mesa_shader_cache".into(),
                path: "/mock/.cache/mesa_shader_cache".into(),
                bytes: 500,
            },
        ]
    }

    #[test]
    fn preview_before_delete_required() {
        let backend = RefCell::new(MockShaderCacheBackend::with_entries(sample_entries()));
        let action = ShaderCacheAction::preview_only(backend);
        let preview = action.preview().unwrap();
        assert_eq!(preview.total_bytes, 2500);
    }

    #[test]
    fn apply_without_confirm_fails() {
        let backend = RefCell::new(MockShaderCacheBackend::with_entries(sample_entries()));
        let mut action = ShaderCacheAction::preview_only(backend);
        let err = action.apply().unwrap_err();
        assert!(err.to_string().contains("confirm"));
    }

    #[test]
    fn confirmed_apply_then_restore_not_possible() {
        let backend = RefCell::new(MockShaderCacheBackend::with_entries(sample_entries()));
        let mut action = ShaderCacheAction::confirmed(backend);
        let before = action.read_state().unwrap();
        assert!(before.contains("total=2500"));
        action.apply().unwrap();
        assert!(action.read_state().unwrap().contains("total=0"));
        let err = action.restore(&before).unwrap_err();
        assert!(matches!(err, ActionError::NotPossible(_)));
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockShaderCacheBackend::with_entries(sample_entries()));
        validate_explanation(ShaderCacheAction::confirmed(backend).explanation()).unwrap();
    }
}
