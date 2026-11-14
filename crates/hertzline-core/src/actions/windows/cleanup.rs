//! Temp and shader-cache cleanup with size preview (AC-4.7).
//!
//! Official / documented paths only (user profile; no elevation):
//! - %TEMP% via GetTempPathW
//!   https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw
//! - DirectX shader cache: %LOCALAPPDATA%\D3DSCache (Microsoft DirectX)
//! - Vendor user caches under %LOCALAPPDATA% (NVIDIA DXCache/GLCache, AMD DxCache)
//!
//! Apply deletes only after preview and opt-in. Deletion is irreversible;
//! restore reports NotPossible (honest). Cleanup is logged via snapshot before/after sizes.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.temp_shader_cleanup";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupEntry {
    pub label: String,
    pub path: String,
    pub bytes: u64,
}

impl CleanupEntry {
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
pub struct CleanupPreview {
    pub entries: Vec<CleanupEntry>,
    pub total_bytes: u64,
}

impl CleanupPreview {
    pub fn encode(&self) -> String {
        let parts: Vec<String> = self.entries.iter().map(CleanupEntry::encode).collect();
        format!("total={};{}", self.total_bytes, parts.join(";"))
    }

    pub fn from_entries(entries: Vec<CleanupEntry>) -> Self {
        let total_bytes = entries.iter().map(|e| e.bytes).sum();
        Self {
            entries,
            total_bytes,
        }
    }
}

/// Preview sizes and optionally delete listed user cache/temp trees.
pub trait CleanupBackend {
    fn preview(&self) -> Result<CleanupPreview, ActionError>;
    /// Delete contents of previewed paths. Returns bytes removed estimate.
    fn apply_delete(&self) -> Result<u64, ActionError>;
}

/// Labeled mock for unit tests (not a real filesystem cleanup).
#[derive(Debug, Clone)]
pub struct MockCleanupBackend {
    pub entries: Vec<CleanupEntry>,
    pub deleted: bool,
}

impl MockCleanupBackend {
    pub fn with_entries(entries: Vec<CleanupEntry>) -> Self {
        Self {
            entries,
            deleted: false,
        }
    }
}

impl CleanupBackend for std::cell::RefCell<MockCleanupBackend> {
    fn preview(&self) -> Result<CleanupPreview, ActionError> {
        let b = self.borrow();
        if b.deleted {
            return Ok(CleanupPreview::from_entries(
                b.entries
                    .iter()
                    .map(|e| CleanupEntry {
                        label: e.label.clone(),
                        path: e.path.clone(),
                        bytes: 0,
                    })
                    .collect(),
            ));
        }
        Ok(CleanupPreview::from_entries(b.entries.clone()))
    }

    fn apply_delete(&self) -> Result<u64, ActionError> {
        let mut b = self.borrow_mut();
        let total = b.entries.iter().map(|e| e.bytes).sum();
        b.deleted = true;
        Ok(total)
    }
}

/// Opt-in temp/shader cleanup (AC-4.7). Requires `confirm=true` before apply.
pub struct CleanupAction<B: CleanupBackend> {
    backend: B,
    confirm: bool,
    explanation: ActionExplanation,
}

impl<B: CleanupBackend> CleanupAction<B> {
    pub fn new(backend: B, confirm: bool) -> Self {
        let mut explanation = ActionExplanation::new(
            "Preview and optionally delete user temp files and known shader caches under the profile.",
            "Can free disk space and force shaders to recompile on next launch. No FPS claim without a local measurement.",
            "Medium: deleted files cannot be restored. Games may hitch while caches rebuild. Paths are user-scoped only.",
            "Deletion is irreversible. Restore is not possible; reinstall or re-run the game to rebuild caches.",
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

    pub fn preview(&self) -> Result<CleanupPreview, ActionError> {
        self.backend.preview()
    }
}

impl<B: CleanupBackend> Action for CleanupAction<B> {
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
                "cleanup requires opt-in confirm=true after size preview (AC-4.7)".into(),
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
            "temp/shader cleanup deletes files; restore is not possible (AC-4.7)".into(),
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

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetTempPathW(buffer_length: u32, buffer: *mut u16) -> u32;
        fn GetEnvironmentVariableW(name: *const u16, buffer: *mut u16, size: u32) -> u32;
    }

    pub struct OsCleanupBackend;

    impl CleanupBackend for OsCleanupBackend {
        fn preview(&self) -> Result<CleanupPreview, ActionError> {
            Ok(CleanupPreview::from_entries(collect_entries()?))
        }

        fn apply_delete(&self) -> Result<u64, ActionError> {
            let entries = collect_entries()?;
            let mut removed = 0u64;
            for entry in entries {
                removed += clear_dir_contents(Path::new(&entry.path))?;
            }
            Ok(removed)
        }
    }

    fn collect_entries() -> Result<Vec<CleanupEntry>, ActionError> {
        let mut out = Vec::new();
        if let Some(temp) = get_temp_path() {
            let bytes = dir_size(&temp);
            out.push(CleanupEntry {
                label: "temp".into(),
                path: temp.display().to_string(),
                bytes,
            });
        }
        if let Some(local) = env_path("LOCALAPPDATA") {
            for (label, rel) in [
                ("d3ds_cache", "D3DSCache"),
                ("nvidia_dxcache", r"NVIDIA\DXCache"),
                ("nvidia_glcache", r"NVIDIA\GLCache"),
                ("amd_dxcache", r"AMD\DxCache"),
            ] {
                let path = local.join(rel);
                if path.is_dir() {
                    let bytes = dir_size(&path);
                    out.push(CleanupEntry {
                        label: label.into(),
                        path: path.display().to_string(),
                        bytes,
                    });
                }
            }
        }
        Ok(out)
    }

    fn get_temp_path() -> Option<PathBuf> {
        let mut buf = vec![0u16; 520];
        let n = unsafe { GetTempPathW(buf.len() as u32, buf.as_mut_ptr()) };
        if n == 0 || n as usize >= buf.len() {
            return None;
        }
        buf.truncate(n as usize);
        let s = String::from_utf16_lossy(&buf);
        Some(PathBuf::from(s.trim_end_matches('\0')))
    }

    fn env_path(name: &str) -> Option<PathBuf> {
        let wide: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut buf = vec![0u16; 520];
        let n =
            unsafe { GetEnvironmentVariableW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if n == 0 || n as usize >= buf.len() {
            return None;
        }
        buf.truncate(n as usize);
        Some(PathBuf::from(String::from_utf16_lossy(&buf)))
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
        fn os_cleanup_preview_lists_temp() {
            let preview = OsCleanupBackend.preview().expect("preview");
            assert!(
                preview.entries.iter().any(|e| e.label == "temp"),
                "expected temp entry: {:?}",
                preview.entries
            );
            // Size may be 0 on empty temp; still a valid preview.
            let _ = preview.total_bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    fn sample_entries() -> Vec<CleanupEntry> {
        vec![
            CleanupEntry {
                label: "temp".into(),
                path: r"C:\Users\mock\AppData\Local\Temp".into(),
                bytes: 1000,
            },
            CleanupEntry {
                label: "d3ds_cache".into(),
                path: r"C:\Users\mock\AppData\Local\D3DSCache".into(),
                bytes: 500,
            },
        ]
    }

    #[test]
    fn preview_before_delete_required() {
        // mock: sizes only
        let backend = RefCell::new(MockCleanupBackend::with_entries(sample_entries()));
        let action = CleanupAction::preview_only(backend);
        let preview = action.preview().unwrap();
        assert_eq!(preview.total_bytes, 1500);
        assert_eq!(preview.entries.len(), 2);
    }

    #[test]
    fn apply_without_confirm_fails() {
        let backend = RefCell::new(MockCleanupBackend::with_entries(sample_entries()));
        let mut action = CleanupAction::preview_only(backend);
        let err = action.apply().unwrap_err();
        assert!(err.to_string().contains("confirm"));
    }

    #[test]
    fn confirmed_apply_then_restore_not_possible() {
        let backend = RefCell::new(MockCleanupBackend::with_entries(sample_entries()));
        let mut action = CleanupAction::confirmed(backend);
        let before = action.read_state().unwrap();
        assert!(before.contains("total=1500"));
        action.apply().unwrap();
        let after = action.read_state().unwrap();
        assert!(after.contains("total=0"));
        let err = action.restore(&before).unwrap_err();
        assert!(matches!(err, ActionError::NotPossible(_)));
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockCleanupBackend::with_entries(sample_entries()));
        let action = CleanupAction::confirmed(backend);
        validate_explanation(action.explanation()).unwrap();
    }
}
