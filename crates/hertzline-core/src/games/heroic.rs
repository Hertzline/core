//! Heroic Games Launcher detection via documented config JSON files.
//!
//! Heroic wiki (Troubleshooting):
//! - Legendary Epic installs: `~/.config/legendary/installed.json`
//! - Heroic config: `~/.config/heroic` (+ Flatpak / Windows / macOS variants)
//! - Sideload / GOG store libraries under heroic `store/` or `sideload_apps/`
//!
//! Read-only JSON; no process scrape.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::types::{DetectedGame, GameSource};

#[derive(Debug, Deserialize)]
struct LegendaryInstalled {
    #[serde(flatten)]
    games: HashMap<String, LegendaryGame>,
}

#[derive(Debug, Deserialize)]
struct LegendaryGame {
    #[serde(rename = "app_name")]
    app_name: Option<String>,
    title: Option<String>,
    #[serde(rename = "install_path")]
    install_path: Option<String>,
    #[serde(default)]
    is_dlc: bool,
}

#[derive(Debug, Deserialize)]
struct HeroicLibrary {
    games: Option<Vec<HeroicGame>>,
    library: Option<Vec<HeroicGame>>,
}

#[derive(Debug, Deserialize)]
struct HeroicGame {
    #[serde(rename = "app_name")]
    app_name: Option<String>,
    title: Option<String>,
    #[serde(default)]
    is_installed: Option<bool>,
    install: Option<HeroicInstall>,
    #[serde(rename = "folder_name")]
    folder_name: Option<String>,
    #[serde(rename = "install_path")]
    install_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HeroicInstall {
    #[serde(rename = "install_path")]
    install_path: Option<String>,
    executable: Option<String>,
}

/// Parse Legendary `installed.json`.
pub fn parse_legendary_installed(text: &str) -> Vec<DetectedGame> {
    let Ok(doc) = serde_json::from_str::<LegendaryInstalled>(text) else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for (key, g) in doc.games {
        if g.is_dlc {
            continue;
        }
        let Some(path) = g.install_path.filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let name = g
            .title
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| key.clone());
        let id = g.app_name.unwrap_or(key);
        games.push(DetectedGame::new(
            GameSource::Heroic,
            id,
            name,
            PathBuf::from(path),
        ));
    }
    games
}

/// Parse Heroic sideload / store `library.json` (`games` or `library` array).
pub fn parse_heroic_sideload_library(text: &str) -> Vec<DetectedGame> {
    let Ok(doc) = serde_json::from_str::<HeroicLibrary>(text) else {
        return Vec::new();
    };
    let list = doc
        .games
        .into_iter()
        .flatten()
        .chain(doc.library.into_iter().flatten());
    let mut games = Vec::new();
    for g in list {
        if g.is_installed == Some(false) {
            continue;
        }
        let path = g
            .install
            .as_ref()
            .and_then(|i| i.install_path.clone())
            .or(g.install_path)
            .or_else(|| {
                g.install
                    .as_ref()
                    .and_then(|i| i.executable.as_ref())
                    .map(|exe| {
                        Path::new(exe)
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| exe.clone())
                    })
            })
            .or(g.folder_name);
        let Some(path) = path.filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let name = g
            .title
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Heroic game".into());
        let id = g.app_name.unwrap_or_else(|| name.clone());
        games.push(DetectedGame::new(
            GameSource::Heroic,
            id,
            name,
            PathBuf::from(path),
        ));
    }
    games
}

/// Detect from a Legendary config dir (`installed.json`).
pub fn detect_from_legendary_dir(legendary_dir: &Path) -> Vec<DetectedGame> {
    let path = legendary_dir.join("installed.json");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_legendary_installed(&text)
}

/// Detect from a Heroic config dir (sideload + store libraries).
pub fn detect_from_heroic_dir(heroic_dir: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let candidates = [
        heroic_dir.join("sideload_apps").join("library.json"),
        heroic_dir
            .join("store")
            .join("sideload_apps")
            .join("library.json"),
        heroic_dir
            .join("store")
            .join("gog_store")
            .join("library.json"),
        heroic_dir.join("gog_library.json"),
        heroic_dir.join("store").join("gog_library.json"),
    ];
    for path in candidates {
        if let Ok(text) = fs::read_to_string(&path) {
            games.extend(parse_heroic_sideload_library(&text));
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legendary_fixture() {
        let text = include_str!("fixtures/heroic/legendary_installed.json");
        let games = parse_legendary_installed(text);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Fixture Epic Game");
        assert_eq!(games[0].source, GameSource::Heroic);
    }

    #[test]
    fn parses_sideload_fixture() {
        let text = include_str!("fixtures/heroic/sideload_library.json");
        let games = parse_heroic_sideload_library(text);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Sideload Fixture");
    }
}
