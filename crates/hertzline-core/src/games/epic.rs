//! Epic Games Launcher detection via documented Manifests `*.item` JSON.
//!
//! Official support documents the Manifests folder under
//! `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests` (Epic Games Store
//! support: IS-0009 / Data folder). Each `.item` is JSON with `DisplayName`,
//! `InstallLocation`, `AppName` / `CatalogItemId`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::types::{DetectedGame, GameSource};

#[derive(Debug, Deserialize)]
struct EpicItem {
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,
    #[serde(rename = "AppName")]
    app_name: Option<String>,
    #[serde(rename = "CatalogItemId")]
    catalog_item_id: Option<String>,
    #[serde(rename = "bIsApplication", default)]
    is_application: Option<bool>,
}

/// Parse one Epic `.item` manifest body.
pub fn parse_epic_item(text: &str) -> Option<DetectedGame> {
    let item: EpicItem = serde_json::from_str(text).ok()?;
    if item.is_application == Some(false) {
        return None;
    }
    let name = item.display_name.filter(|s| !s.trim().is_empty())?;
    let install = item.install_location.filter(|s| !s.trim().is_empty())?;
    let id = item
        .app_name
        .or(item.catalog_item_id)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    Some(DetectedGame::new(
        GameSource::Epic,
        id,
        name,
        PathBuf::from(install),
    ))
}

/// Scan an Epic Manifests directory for `*.item` files.
pub fn detect_from_manifests_dir(manifests: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(manifests) else {
        return games;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(game) = parse_epic_item(&text) {
            games.push(game);
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_item() {
        let text = include_str!("fixtures/epic/NuclearThrone.item");
        let game = parse_epic_item(text).expect("item");
        assert_eq!(game.source, GameSource::Epic);
        assert_eq!(game.name, "Nuclear Throne");
        assert_eq!(game.id, "Turaco");
        assert!(game
            .install_path
            .to_string_lossy()
            .contains("NuclearThrone"));
    }
}
