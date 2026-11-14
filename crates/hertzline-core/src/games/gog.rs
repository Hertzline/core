//! GOG detection via documented per-game `goggame-*.info` JSON.
//!
//! GOG ships `goggame-<productId>.info` beside each install (GOG build / Galaxy
//! layout). Fields used: `gameId`, `name`. Library roots are scanned one level
//! deep; no Galaxy SQLite scrape and no process memory.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::types::{DetectedGame, GameSource};

#[derive(Debug, Deserialize)]
struct GogInfo {
    #[serde(rename = "gameId")]
    game_id: Option<String>,
    name: Option<String>,
}

/// Parse one `goggame-*.info` body; `install_dir` is the directory containing it.
pub fn parse_gog_info(text: &str, install_dir: &Path) -> Option<DetectedGame> {
    let info: GogInfo = serde_json::from_str(text).ok()?;
    let name = info.name.filter(|s| !s.trim().is_empty())?;
    let id = info
        .game_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    Some(DetectedGame::new(
        GameSource::Gog,
        id,
        name,
        install_dir.to_path_buf(),
    ))
}

/// Scan a GOG library root for child folders containing `goggame-*.info`.
pub fn detect_from_gog_library(library_root: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    // Also allow the root itself to be a single-game folder.
    games.extend(scan_dir_for_info(library_root));
    let Ok(entries) = fs::read_dir(library_root) else {
        return games;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        games.extend(scan_dir_for_info(&path));
    }
    games
}

fn scan_dir_for_info(dir: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return games;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = entry.file_name().to_string_lossy().into_owned();
        if !fname.starts_with("goggame-") || !fname.ends_with(".info") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(game) = parse_gog_info(&text, dir) {
            games.push(game);
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_info() {
        let text = include_str!("fixtures/gog/goggame-1237807960.info");
        let game = parse_gog_info(text, Path::new("D:/GOG Games/Dead Cells")).expect("info");
        assert_eq!(game.source, GameSource::Gog);
        assert_eq!(game.id, "1237807960");
        assert_eq!(game.name, "Dead Cells");
    }
}
