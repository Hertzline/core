//! Steam detection via documented libraryfolders.vdf + appmanifest_*.acf.
//!
//! Default roots (Valve / Steam client layout):
//! - Windows: `%ProgramFiles(x86)%\Steam`, `%ProgramFiles%\Steam`
//! - Linux: `~/.local/share/Steam`, `~/.steam/steam`, `~/.steam/root`,
//!   Flatpak `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`
//! - macOS: `~/Library/Application Support/Steam`
//!
//! Each library root is listed in `steamapps/libraryfolders.vdf` (`path` keys).
//! Installed titles: `steamapps/appmanifest_<appid>.acf` with `appid`, `name`,
//! `installdir` under `steamapps/common/<installdir>`.

use std::fs;
use std::path::{Path, PathBuf};

use super::types::{DetectedGame, GameSource};
use super::vdf::{vdf_all_paths, vdf_quoted_value};

/// Parse libraryfolders.vdf text into library root paths.
pub fn parse_libraryfolders_vdf(text: &str) -> Vec<PathBuf> {
    vdf_all_paths(text).into_iter().map(PathBuf::from).collect()
}

/// Parse one appmanifest_*.acf into a detected game under `steamapps`.
pub fn parse_appmanifest_acf(text: &str, steamapps: &Path) -> Option<DetectedGame> {
    let appid = vdf_quoted_value(text, "appid")?;
    let name = vdf_quoted_value(text, "name")?;
    let installdir = vdf_quoted_value(text, "installdir")?;
    if appid.is_empty() || name.is_empty() || installdir.is_empty() {
        return None;
    }
    let install_path = steamapps.join("common").join(installdir);
    Some(DetectedGame::new(
        GameSource::Steam,
        appid,
        name,
        install_path,
    ))
}

/// Scan a Steam install root for libraryfolders + appmanifests.
pub fn detect_from_steam_root(steam_root: &Path) -> Vec<DetectedGame> {
    let mut libraries = Vec::new();
    let steamapps = steam_root.join("steamapps");
    let vdf_path = steamapps.join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(&vdf_path) {
        libraries.extend(parse_libraryfolders_vdf(&text));
    }
    let config_vdf = steam_root.join("config").join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(&config_vdf) {
        for path in parse_libraryfolders_vdf(&text) {
            if !libraries.iter().any(|p| p == &path) {
                libraries.push(path);
            }
        }
    }
    if libraries.is_empty() && steamapps.is_dir() {
        libraries.push(steam_root.to_path_buf());
    }

    let mut games = Vec::new();
    for lib in libraries {
        let lib_steamapps = if lib.join("steamapps").is_dir() {
            lib.join("steamapps")
        } else if lib.ends_with("steamapps") {
            lib
        } else {
            lib.join("steamapps")
        };
        games.extend(scan_steamapps(&lib_steamapps));
    }
    games
}

fn scan_steamapps(steamapps: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(steamapps) else {
        return games;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(game) = parse_appmanifest_acf(&text, steamapps) {
            games.push(game);
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_manifest() {
        let text = include_str!("fixtures/steam/appmanifest_480.acf");
        let game = parse_appmanifest_acf(text, Path::new("/Steam/steamapps")).expect("manifest");
        assert_eq!(game.source, GameSource::Steam);
        assert_eq!(game.id, "480");
        assert_eq!(game.name, "Spacewar");
        assert_eq!(
            game.install_path,
            PathBuf::from("/Steam/steamapps/common/Spacewar")
        );
    }

    #[test]
    fn parses_fixture_libraryfolders() {
        let text = include_str!("fixtures/steam/libraryfolders.vdf");
        let libs = parse_libraryfolders_vdf(text);
        assert_eq!(libs.len(), 2);
        assert!(libs[0].to_string_lossy().contains("Steam"));
    }
}
