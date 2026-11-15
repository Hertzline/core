//! Anti-cheat markers from documented install-tree paths only (AC-6.2).
//!
//! Scans the game install directory for well-known vendor folder / file names.
//! Never reads process memory or injects. Unknown trees return no detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Known anti-cheat families Hertzline can identify from install layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiCheatKind {
    EasyAntiCheat,
    BattlEye,
    Vanguard,
    Ricochet,
}

impl AntiCheatKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EasyAntiCheat => "easy_anti_cheat",
            Self::BattlEye => "battleye",
            Self::Vanguard => "vanguard",
            Self::Ricochet => "ricochet",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::EasyAntiCheat => "Easy Anti-Cheat",
            Self::BattlEye => "BattlEye",
            Self::Vanguard => "Riot Vanguard",
            Self::Ricochet => "Ricochet",
        }
    }
}

/// One filesystem hit under a game install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiCheatMarker {
    pub kind: AntiCheatKind,
    pub relative_path: String,
}

/// Result of scanning an install path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiCheatScan {
    pub install_path: String,
    pub markers: Vec<AntiCheatMarker>,
    pub note: String,
}

impl AntiCheatScan {
    pub fn detected(&self) -> bool {
        !self.markers.is_empty()
    }

    pub fn kinds(&self) -> Vec<AntiCheatKind> {
        let mut out = Vec::new();
        for marker in &self.markers {
            if !out.contains(&marker.kind) {
                out.push(marker.kind);
            }
        }
        out
    }
}

/// Relative path fragments that indicate an anti-cheat install (case-insensitive).
const MARKERS: &[(&str, AntiCheatKind)] = &[
    ("easyanticheat", AntiCheatKind::EasyAntiCheat),
    ("easyanti-cheat", AntiCheatKind::EasyAntiCheat),
    ("easyanticheat_eos", AntiCheatKind::EasyAntiCheat),
    ("battleye", AntiCheatKind::BattlEye),
    ("beclient", AntiCheatKind::BattlEye),
    ("beservice", AntiCheatKind::BattlEye),
    ("vanguard", AntiCheatKind::Vanguard),
    ("vgk.sys", AntiCheatKind::Vanguard),
    ("ricochet", AntiCheatKind::Ricochet),
];

/// Scan `install_path` for anti-cheat markers (depth-limited directory walk).
pub fn scan_install_path(install_path: &Path) -> AntiCheatScan {
    let display = install_path.display().to_string();
    if !install_path.exists() {
        return AntiCheatScan {
            install_path: display,
            markers: Vec::new(),
            note: "install path missing; no anti-cheat markers found".into(),
        };
    }

    let mut markers = Vec::new();
    walk_limited(install_path, install_path, 0, 4, &mut markers);

    let note = if markers.is_empty() {
        "no known anti-cheat folders/files under install path (filesystem scan only)".into()
    } else {
        format!(
            "anti-cheat markers found ({}); overlay off by default (AC-6.2)",
            markers
                .iter()
                .map(|m| m.kind.display_name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    AntiCheatScan {
        install_path: display,
        markers,
        note,
    }
}

fn walk_limited(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<AntiCheatMarker>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if let Some(kind) = match_name(&name) {
            let rel = relative_display(root, &path);
            if !out.iter().any(|m| m.kind == kind && m.relative_path == rel) {
                out.push(AntiCheatMarker {
                    kind,
                    relative_path: rel,
                });
            }
        }
        if path.is_dir() {
            // Skip huge trees that are never anti-cheat roots.
            if name == "shadercache"
                || name == "__pycache__"
                || name == ".git"
                || name == "node_modules"
            {
                continue;
            }
            walk_limited(root, &path, depth + 1, max_depth, out);
        }
    }
}

fn match_name(name: &str) -> Option<AntiCheatKind> {
    let stem = name.trim_end_matches(".exe").trim_end_matches(".dll");
    for (needle, kind) in MARKERS {
        if stem == *needle || stem.contains(needle) || name == *needle {
            return Some(*kind);
        }
    }
    None
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Build a scan from an explicit declared kind (manual / UI), no filesystem.
pub fn declared(kind: AntiCheatKind) -> AntiCheatScan {
    AntiCheatScan {
        install_path: String::new(),
        markers: vec![AntiCheatMarker {
            kind,
            relative_path: "(user declared)".into(),
        }],
        note: format!(
            "{} declared by user; overlay off by default (AC-6.2)",
            kind.display_name()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hertzline-ac-{}-{}-{}",
            label,
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_easy_anti_cheat_folder() {
        let root = temp_dir("eac");
        fs::create_dir_all(root.join("EasyAntiCheat")).unwrap();
        fs::write(root.join("EasyAntiCheat").join("settings.json"), "{}").unwrap();
        let scan = scan_install_path(&root);
        assert!(scan.detected());
        assert_eq!(scan.kinds(), vec![AntiCheatKind::EasyAntiCheat]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_battleye_folder() {
        let root = temp_dir("be");
        fs::create_dir_all(root.join("BattlEye")).unwrap();
        let scan = scan_install_path(&root);
        assert!(scan.detected());
        assert!(scan.kinds().contains(&AntiCheatKind::BattlEye));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_install_has_no_markers() {
        let root = temp_dir("clean");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin").join("game.exe"), b"mock").unwrap();
        let scan = scan_install_path(&root);
        assert!(!scan.detected());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_path_is_not_detected() {
        let scan = scan_install_path(Path::new("C:\\definitely\\missing\\hertzline-ac-scan-xyz"));
        assert!(!scan.detected());
    }

    #[test]
    fn declared_marks_detected() {
        let scan = declared(AntiCheatKind::Vanguard);
        assert!(scan.detected());
        assert_eq!(scan.kinds(), vec![AntiCheatKind::Vanguard]);
    }
}
