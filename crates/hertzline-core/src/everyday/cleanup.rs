//! Everyday disk cleanup with path/size preview (AC-5.4).
//!
//! Reuses documented Windows temp/shader paths (AC-4.7) and Linux shader cache
//! paths (AC-4.17). Apply requires confirm after preview. Deletion is irreversible.

use crate::action::ActionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPath {
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverydayCleanupPreview {
    pub host_os: String,
    pub note: String,
    pub entries: Vec<CleanupPath>,
    pub total_bytes: u64,
}

impl EverydayCleanupPreview {
    pub fn from_entries(
        host_os: impl Into<String>,
        note: impl Into<String>,
        entries: Vec<CleanupPath>,
    ) -> Self {
        let total_bytes = entries.iter().map(|e| e.bytes).sum();
        Self {
            host_os: host_os.into(),
            note: note.into(),
            entries,
            total_bytes,
        }
    }
}

/// Pluggable cleanup backend (mocks labeled in tests).
pub trait EverydayCleanupBackend {
    fn preview(&self) -> Result<EverydayCleanupPreview, ActionError>;
    fn apply_delete(&self) -> Result<u64, ActionError>;
}

/// Labeled mock (not a real filesystem cleanup).
#[derive(Debug, Clone)]
pub struct MockEverydayCleanupBackend {
    pub entries: Vec<CleanupPath>,
    pub deleted: bool,
}

impl MockEverydayCleanupBackend {
    pub fn with_entries(entries: Vec<CleanupPath>) -> Self {
        Self {
            entries,
            deleted: false,
        }
    }
}

impl EverydayCleanupBackend for std::cell::RefCell<MockEverydayCleanupBackend> {
    fn preview(&self) -> Result<EverydayCleanupPreview, ActionError> {
        let b = self.borrow();
        let entries = if b.deleted {
            b.entries
                .iter()
                .map(|e| CleanupPath {
                    label: e.label.clone(),
                    path: e.path.clone(),
                    bytes: 0,
                    source: e.source.clone(),
                })
                .collect()
        } else {
            b.entries.clone()
        };
        Ok(EverydayCleanupPreview::from_entries(
            "mock",
            "labeled mock cleanup preview (not live filesystem)",
            entries,
        ))
    }

    fn apply_delete(&self) -> Result<u64, ActionError> {
        let mut b = self.borrow_mut();
        let total = b.entries.iter().map(|e| e.bytes).sum();
        b.deleted = true;
        Ok(total)
    }
}

/// Preview paths and sizes before any delete (AC-5.4).
pub fn preview_everyday_cleanup<B: EverydayCleanupBackend>(
    backend: &B,
) -> Result<EverydayCleanupPreview, ActionError> {
    backend.preview()
}

/// Delete only after preview + explicit confirm (AC-5.4). Irreversible.
pub fn apply_everyday_cleanup<B: EverydayCleanupBackend>(
    backend: &B,
    confirm: bool,
) -> Result<u64, ActionError> {
    if !confirm {
        return Err(ActionError::ApplyFailed(
            "disk cleanup requires confirm=true after size preview (AC-5.4)".into(),
        ));
    }
    let preview = backend.preview()?;
    if preview.total_bytes == 0 {
        return Ok(0);
    }
    backend.apply_delete()
}

/// Host cleanup paths for the current OS.
pub struct HostEverydayCleanupBackend;

impl EverydayCleanupBackend for HostEverydayCleanupBackend {
    fn preview(&self) -> Result<EverydayCleanupPreview, ActionError> {
        host_preview()
    }

    fn apply_delete(&self) -> Result<u64, ActionError> {
        host_apply_delete()
    }
}

fn host_preview() -> Result<EverydayCleanupPreview, ActionError> {
    let host_os = std::env::consts::OS.to_string();
    #[cfg(windows)]
    {
        use crate::actions::windows::cleanup::os::OsCleanupBackend;
        use crate::actions::windows::CleanupBackend;
        let preview = OsCleanupBackend.preview()?;
        let entries = preview
            .entries
            .into_iter()
            .map(|e| CleanupPath {
                label: e.label,
                path: e.path,
                bytes: e.bytes,
                source: "windows.temp_shader_cleanup".into(),
            })
            .collect();
        Ok(EverydayCleanupPreview::from_entries(
            host_os,
            "Preview only until you confirm. Deletion is irreversible (AC-5.4 / AC-4.7).",
            entries,
        ))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::actions::linux::shader_cache::os::OsShaderCacheBackend;
        use crate::actions::linux::ShaderCacheBackend;
        let preview = OsShaderCacheBackend.preview()?;
        let entries = preview
            .entries
            .into_iter()
            .map(|e| CleanupPath {
                label: e.label,
                path: e.path,
                bytes: e.bytes,
                source: "linux.shader_cache_cleanup".into(),
            })
            .collect();
        Ok(EverydayCleanupPreview::from_entries(
            host_os,
            "Preview only until you confirm. Deletion is irreversible (AC-5.4 / AC-4.17).",
            entries,
        ))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(EverydayCleanupPreview::from_entries(
            host_os,
            "macOS everyday cleanup has no documented safe auto paths in v1; clear caches manually or use Actions when available.",
            Vec::new(),
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        Ok(EverydayCleanupPreview::from_entries(
            host_os,
            "disk cleanup is not available on this OS",
            Vec::new(),
        ))
    }
}

fn host_apply_delete() -> Result<u64, ActionError> {
    #[cfg(windows)]
    {
        use crate::actions::windows::cleanup::os::OsCleanupBackend;
        use crate::actions::windows::CleanupBackend;
        OsCleanupBackend.apply_delete()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::actions::linux::shader_cache::os::OsShaderCacheBackend;
        use crate::actions::linux::ShaderCacheBackend;
        OsShaderCacheBackend.apply_delete()
    }
    #[cfg(target_os = "macos")]
    {
        Err(ActionError::NotPossible(
            "macOS everyday cleanup apply is not possible in v1".into(),
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(ActionError::NotPossible(
            "disk cleanup is not available on this OS".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn preview_before_delete_and_confirm_required() {
        // labeled mock: not live FS
        let backend = RefCell::new(MockEverydayCleanupBackend::with_entries(vec![
            CleanupPath {
                label: "Temp".into(),
                path: "/tmp/mock".into(),
                bytes: 1024,
                source: "mock".into(),
            },
        ]));
        let preview = preview_everyday_cleanup(&backend).unwrap();
        assert_eq!(preview.total_bytes, 1024);
        assert_eq!(preview.entries.len(), 1);
        assert!(apply_everyday_cleanup(&backend, false).is_err());
        let removed = apply_everyday_cleanup(&backend, true).unwrap();
        assert_eq!(removed, 1024);
        let after = preview_everyday_cleanup(&backend).unwrap();
        assert_eq!(after.total_bytes, 0);
    }

    #[test]
    fn host_preview_ok() {
        let preview = preview_everyday_cleanup(&HostEverydayCleanupBackend).expect("preview");
        assert!(!preview.host_os.is_empty());
        assert!(!preview.note.is_empty());
    }
}
