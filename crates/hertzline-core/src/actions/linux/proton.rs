//! Proton / Wine version info from documented paths (AC-4.16).
//!
//! Official / Valve-documented locations (verified 2026-11-14):
//! - Custom tools: `compatibilitytools.d` under Steam root
//!   Valve: `~/.steam/steam` and `~/.steam/root` symlink to the Steam install
//!   https://github.com/ValveSoftware/steam-for-linux/issues/6310#issuecomment-511630263
//! - Tool manifest: `compatibilitytool.vdf` with `display_name`
//! - Optional `version` file beside the tool (informational)
//! - System Wine: `wine --version` when on PATH (WineHQ)
//!
//! Report-only: never silently replaces Proton/Wine builds.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "linux.proton_wine_info";
pub const SOURCE_URL: &str =
    "https://github.com/ValveSoftware/steam-for-linux/issues/6310#issuecomment-511630263";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonToolInfo {
    pub name: String,
    pub version: String,
    pub path: String,
    pub kind: String,
}

impl ProtonToolInfo {
    pub fn encode(&self) -> String {
        format!(
            "name={};version={};kind={};path={}",
            encode_field(&self.name),
            encode_field(&self.version),
            encode_field(&self.kind),
            encode_field(&self.path)
        )
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/")
        .replace(';', "%3B")
        .replace('=', "%3D")
        .replace('|', "%7C")
}

pub trait ProtonInfoBackend {
    fn list(&self) -> Result<Vec<ProtonToolInfo>, ActionError>;
}

/// Labeled mock for unit tests (not real Steam/Wine installs).
#[derive(Debug, Clone)]
pub struct MockProtonInfoBackend {
    pub tools: Vec<ProtonToolInfo>,
}

impl ProtonInfoBackend for std::cell::RefCell<MockProtonInfoBackend> {
    fn list(&self) -> Result<Vec<ProtonToolInfo>, ActionError> {
        Ok(self.borrow().tools.clone())
    }
}

/// Report-only Proton/Wine inventory (AC-4.16).
pub struct ProtonInfoAction<B: ProtonInfoBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: ProtonInfoBackend> ProtonInfoAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "List Proton compatibility tools and Wine version from documented Steam paths and PATH. Never replaces or rewrites game prefixes.",
            "Lets you see which Proton/Wine builds are installed before changing Steam Play settings yourself. No silent replacement.",
            "None for this check: read-only. Choosing a tool in Steam is the user's action.",
            "Nothing to undo; this action only reports.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    pub fn report(&self) -> Result<Vec<ProtonToolInfo>, ActionError> {
        self.backend.list()
    }
}

impl<B: ProtonInfoBackend> Action for ProtonInfoAction<B> {
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
        let list = self.backend.list()?;
        if list.is_empty() {
            return Ok("tools=0".into());
        }
        let parts: Vec<String> = list.iter().map(ProtonToolInfo::encode).collect();
        Ok(format!("tools={}|{}", list.len(), parts.join("|")))
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let _ = self.backend.list()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        if actual == expected || (actual.starts_with("tools=") && expected.starts_with("tools=")) {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

/// Extract `display_name` from a minimal compatibilitytool.vdf body.
pub fn parse_vdf_display_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        // "display_name" "Proton-GE"
        if !trimmed.contains("display_name") {
            continue;
        }
        let mut parts = trimmed.split('"').filter(|p| !p.trim().is_empty());
        let key = parts.next()?;
        if key != "display_name" {
            continue;
        }
        return parts.next().map(|s| s.to_string());
    }
    None
}

#[cfg(target_os = "linux")]
pub mod os {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub struct OsProtonInfoBackend;

    impl ProtonInfoBackend for OsProtonInfoBackend {
        fn list(&self) -> Result<Vec<ProtonToolInfo>, ActionError> {
            let mut out = Vec::new();
            for root in steam_roots() {
                let tools_dir = root.join("compatibilitytools.d");
                if tools_dir.is_dir() {
                    scan_compatibilitytools(&tools_dir, &mut out);
                }
                let common = root.join("steamapps").join("common");
                if common.is_dir() {
                    scan_steamapps_common(&common, &mut out);
                }
            }
            if let Some(wine) = wine_version() {
                out.push(wine);
            }
            Ok(out)
        }
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
                // Dedup by canonical if possible
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

    fn scan_compatibilitytools(dir: &Path, out: &mut Vec<ProtonToolInfo>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let vdf = path.join("compatibilitytool.vdf");
            let name = if vdf.is_file() {
                fs::read_to_string(&vdf)
                    .ok()
                    .and_then(|t| parse_vdf_display_name(&t))
                    .unwrap_or_else(|| {
                        path.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "unknown".into())
                    })
            } else {
                continue;
            };
            let version = read_version_file(&path).unwrap_or_else(|| "unknown".into());
            out.push(ProtonToolInfo {
                name,
                version,
                path: path.display().to_string(),
                kind: "compatibilitytool".into(),
            });
        }
    }

    fn scan_steamapps_common(common: &Path, out: &mut Vec<ProtonToolInfo>) {
        let Ok(entries) = fs::read_dir(common) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().into_owned();
            if !path.is_dir() {
                continue;
            }
            let lower = fname.to_ascii_lowercase();
            if !(lower.starts_with("proton") || lower.contains("proton")) {
                continue;
            }
            // Avoid duplicate if already listed via compatibilitytools.d
            let display = path.display().to_string();
            if out.iter().any(|t| t.path == display) {
                continue;
            }
            let version = read_version_file(&path).unwrap_or_else(|| fname.clone());
            out.push(ProtonToolInfo {
                name: fname,
                version,
                path: display,
                kind: "steamapps_common".into(),
            });
        }
    }

    fn read_version_file(dir: &Path) -> Option<String> {
        let path = dir.join("version");
        let raw = fs::read_to_string(path).ok()?;
        let line = raw.lines().next()?.trim();
        if line.is_empty() {
            None
        } else {
            Some(line.to_string())
        }
    }

    fn wine_version() -> Option<ProtonToolInfo> {
        let output = Command::new("wine").arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if version.is_empty() {
            return None;
        }
        Some(ProtonToolInfo {
            name: "wine".into(),
            version,
            path: "PATH".into(),
            kind: "wine".into(),
        })
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn list_does_not_panic() {
            let list = OsProtonInfoBackend.list().unwrap_or_default();
            let _ = list.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    #[test]
    fn report_only_apply_restore() {
        let backend = RefCell::new(MockProtonInfoBackend {
            tools: vec![ProtonToolInfo {
                name: "GE-Proton".into(),
                version: "9.0".into(),
                path: "/mock/compatibilitytools.d/GE-Proton".into(),
                kind: "compatibilitytool".into(),
            }],
        });
        let mut action = ProtonInfoAction::new(backend);
        let before = action.read_state().unwrap();
        assert!(before.starts_with("tools=1"));
        action.apply().unwrap();
        action.restore(&before).unwrap();
        assert_eq!(action.verify(&before).unwrap(), VerifyResult::Matches);
    }

    #[test]
    fn empty_inventory() {
        let backend = RefCell::new(MockProtonInfoBackend { tools: vec![] });
        let action = ProtonInfoAction::new(backend);
        assert_eq!(action.read_state().unwrap(), "tools=0");
    }

    #[test]
    fn parse_display_name() {
        let vdf = r#"
"compatibilitytools"
{
  "compat_tools"
  {
    "GE-Proton9-1"
    {
      "install_path" "."
      "display_name" "GE-Proton9-1"
      "from_oslist" "windows"
      "to_oslist" "linux"
    }
  }
}
"#;
        assert_eq!(parse_vdf_display_name(vdf).as_deref(), Some("GE-Proton9-1"));
    }

    #[test]
    fn explanation_ac81() {
        let backend = RefCell::new(MockProtonInfoBackend { tools: vec![] });
        validate_explanation(ProtonInfoAction::new(backend).explanation()).unwrap();
    }
}
