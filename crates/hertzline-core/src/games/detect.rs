//! Aggregate detection over injectable roots (fixtures + live OS paths).

use std::fs;
use std::path::PathBuf;

use super::battlenet::{parse_battlenet_product_db, BATTLENET_PRODUCT_DB_REL};
use super::epic::detect_from_manifests_dir;
use super::gog::detect_from_gog_library;
use super::heroic::{detect_from_heroic_dir, detect_from_legendary_dir};
use super::lutris::detect_from_lutris_games_dir;
use super::manual::manual_game;
use super::steam::detect_from_steam_root;
use super::types::DetectedGame;
use super::xbox::{detect_from_xbox_games_root, parse_gaming_root};

/// Injectable roots so unit tests never touch the live machine layout.
#[derive(Debug, Clone, Default)]
pub struct DetectionRoots {
    pub steam_roots: Vec<PathBuf>,
    pub epic_manifests: Vec<PathBuf>,
    pub gog_libraries: Vec<PathBuf>,
    pub xbox_games_roots: Vec<PathBuf>,
    pub gaming_root_files: Vec<PathBuf>,
    pub battlenet_product_db: Vec<PathBuf>,
    pub lutris_games_dirs: Vec<PathBuf>,
    pub heroic_dirs: Vec<PathBuf>,
    pub legendary_dirs: Vec<PathBuf>,
    pub manual: Vec<(String, PathBuf)>,
}

impl DetectionRoots {
    /// Build default live roots from OS conventions (documented paths only).
    pub fn from_os_defaults() -> Self {
        Self {
            steam_roots: default_steam_roots(),
            epic_manifests: default_epic_manifests(),
            gog_libraries: default_gog_libraries(),
            xbox_games_roots: default_xbox_games_roots(),
            gaming_root_files: default_gaming_root_files(),
            battlenet_product_db: default_battlenet_product_db(),
            lutris_games_dirs: default_lutris_games_dirs(),
            heroic_dirs: default_heroic_dirs(),
            legendary_dirs: default_legendary_dirs(),
            manual: Vec::new(),
        }
    }
}

/// Detect installed games from the given roots (AC-3.1, AC-3.2).
pub fn detect_games(roots: &DetectionRoots) -> Vec<DetectedGame> {
    let mut games = Vec::new();

    for root in &roots.steam_roots {
        games.extend(detect_from_steam_root(root));
    }
    for manifests in &roots.epic_manifests {
        games.extend(detect_from_manifests_dir(manifests));
    }
    for lib in &roots.gog_libraries {
        games.extend(detect_from_gog_library(lib));
    }
    for xbox in &roots.xbox_games_roots {
        games.extend(detect_from_xbox_games_root(xbox));
    }
    for marker in &roots.gaming_root_files {
        if let Ok(bytes) = fs::read(marker) {
            if let Some(rel) = parse_gaming_root(&bytes) {
                let base = marker.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let xbox_root = if rel.is_absolute() {
                    rel
                } else {
                    base.join(rel)
                };
                games.extend(detect_from_xbox_games_root(&xbox_root));
            }
        }
    }
    for db in &roots.battlenet_product_db {
        if let Ok(bytes) = fs::read(db) {
            games.extend(parse_battlenet_product_db(&bytes));
        }
    }
    for dir in &roots.lutris_games_dirs {
        games.extend(detect_from_lutris_games_dir(dir));
    }
    for dir in &roots.legendary_dirs {
        games.extend(detect_from_legendary_dir(dir));
    }
    for dir in &roots.heroic_dirs {
        games.extend(detect_from_heroic_dir(dir));
    }
    for (name, path) in &roots.manual {
        games.push(manual_game(name.clone(), path.clone()));
    }

    dedupe(games)
}

fn dedupe(games: Vec<DetectedGame>) -> Vec<DetectedGame> {
    let mut out = Vec::new();
    for game in games {
        let exists = out.iter().any(|g: &DetectedGame| {
            g.source == game.source && g.id == game.id && g.install_path == game.install_path
        });
        if !exists {
            out.push(game);
        }
    }
    out
}

fn home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        let t = h.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(p) = std::env::var("USERPROFILE") {
            let t = p.trim();
            if !t.is_empty() {
                return PathBuf::from(t);
            }
        }
    }
    PathBuf::from(".")
}

fn default_steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for key in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(pf) = std::env::var(key) {
                let p = PathBuf::from(pf).join("Steam");
                if p.is_dir() {
                    roots.push(p);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let p = home()
            .join("Library")
            .join("Application Support")
            .join("Steam");
        if p.is_dir() {
            roots.push(p);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let h = home();
        for rel in [
            ".local/share/Steam",
            ".steam/steam",
            ".steam/root",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        ] {
            let p = h.join(rel);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots
}

fn default_epic_manifests() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(pd) = std::env::var("ProgramData") {
            let p = PathBuf::from(pd)
                .join("Epic")
                .join("EpicGamesLauncher")
                .join("Data")
                .join("Manifests");
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    // Linux/macOS Epic native launcher is uncommon; Heroic/Legendary cover those.
    out
}

fn default_gog_libraries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        for key in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(pf) = std::env::var(key) {
                let p = PathBuf::from(pf).join("GOG Galaxy").join("Games");
                if p.is_dir() {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn default_xbox_games_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\XboxGames", letter as char));
            if root.is_dir() {
                out.push(root);
            }
        }
    }
    out
}

fn default_gaming_root_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'C'..=b'Z' {
            let marker = PathBuf::from(format!("{}:\\.GamingRoot", letter as char));
            if marker.is_file() {
                out.push(marker);
            }
        }
    }
    out
}

fn default_battlenet_product_db() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(pd) = std::env::var("ProgramData") {
            let p = PathBuf::from(pd).join(BATTLENET_PRODUCT_DB_REL);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

fn default_lutris_games_dirs() -> Vec<PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let h = home();
        let mut out = Vec::new();
        for rel in [
            ".local/share/lutris/games",
            ".config/lutris/games",
            ".var/app/net.lutris.Lutris/data/lutris/games",
        ] {
            let p = h.join(rel);
            if p.is_dir() {
                out.push(p);
            }
        }
        return out;
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        Vec::new()
    }
}

fn default_heroic_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let h = home();
    #[cfg(windows)]
    {
        for p in [
            h.join(".config").join("heroic"),
            h.join("AppData").join("Roaming").join("heroic"),
        ] {
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for p in [
            h.join(".config").join("heroic"),
            h.join("Library").join("Application Support").join("heroic"),
        ] {
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for rel in [
            ".config/heroic",
            ".var/app/com.heroicgameslauncher.hgl/config/heroic",
        ] {
            let p = h.join(rel);
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out
}

fn default_legendary_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let h = home();
    for rel in [
        ".config/legendary",
        ".var/app/com.heroicgameslauncher.hgl/config/legendary",
    ] {
        let p = h.join(rel);
        if p.is_dir() {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::GameSource;
    use std::path::Path;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("games")
            .join("fixtures")
    }

    #[test]
    fn detects_all_fixture_sources() {
        // automated: labeled fixtures under games/fixtures (not live installs)
        let base = fixture_dir();
        let roots = DetectionRoots {
            steam_roots: vec![base.join("steam").join("root")],
            epic_manifests: vec![base.join("epic").join("Manifests")],
            gog_libraries: vec![base.join("gog").join("library")],
            xbox_games_roots: vec![base.join("xbox").join("XboxGames")],
            gaming_root_files: vec![],
            battlenet_product_db: vec![base.join("battlenet").join("product.db")],
            lutris_games_dirs: vec![base.join("lutris").join("games")],
            heroic_dirs: vec![base.join("heroic").join("heroic_config")],
            legendary_dirs: vec![base.join("heroic").join("legendary")],
            manual: vec![(
                "Manual Fixture".into(),
                base.join("manual").join("CustomGame"),
            )],
        };

        let games = detect_games(&roots);
        let sources: Vec<_> = games.iter().map(|g| g.source).collect();

        assert!(
            sources.contains(&GameSource::Steam),
            "steam missing: {games:?}"
        );
        assert!(
            sources.contains(&GameSource::Epic),
            "epic missing: {games:?}"
        );
        assert!(sources.contains(&GameSource::Gog), "gog missing: {games:?}");
        assert!(
            sources.contains(&GameSource::Xbox),
            "xbox missing: {games:?}"
        );
        assert!(
            sources.contains(&GameSource::BattleNet),
            "battlenet missing: {games:?}"
        );
        assert!(
            sources.contains(&GameSource::Lutris),
            "lutris missing: {games:?}"
        );
        assert!(
            sources.contains(&GameSource::Heroic),
            "heroic missing: {games:?}"
        );
        assert!(
            sources.contains(&GameSource::Manual),
            "manual missing: {games:?}"
        );
    }

    #[test]
    fn never_uses_undocumented_source_labels() {
        let roots = DetectionRoots::default();
        let games = detect_games(&roots);
        for g in games {
            assert!(
                matches!(
                    g.source,
                    GameSource::Steam
                        | GameSource::Epic
                        | GameSource::Gog
                        | GameSource::Xbox
                        | GameSource::BattleNet
                        | GameSource::Lutris
                        | GameSource::Heroic
                        | GameSource::Manual
                ),
                "undocumented source {:?}",
                g.source
            );
        }
    }
}
