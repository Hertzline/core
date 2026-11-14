//! Lutris detection via documented per-game YAML under the Lutris data dir.
//!
//! Official Lutris README: `~/.local/share/lutris` holds `games/*.yml` and
//! `pga.db`. We read only `games/*.yml` (no SQLite dependency) for `name` and
//! install directory (`game.exe` parent or `game.prefix` / working dir).

use std::fs;
use std::path::{Path, PathBuf};

use super::types::{DetectedGame, GameSource};

/// Parse one Lutris `games/<slug>-*.yml` body.
pub fn parse_lutris_game_yml(text: &str, file_stem: &str) -> Option<DetectedGame> {
    let name = yaml_top_level_string(text, "name").filter(|s| !s.trim().is_empty())?;
    let exe = yaml_under_game(text, "exe");
    let prefix = yaml_under_game(text, "prefix");
    let working_dir = yaml_under_game(text, "working_dir");
    let directory = yaml_top_level_string(text, "directory");

    let install_path = resolve_install_path(
        exe.as_deref(),
        prefix.as_deref(),
        working_dir.as_deref(),
        directory.as_deref(),
    )?;
    let id = file_stem
        .split('-')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(file_stem)
        .to_string();
    Some(DetectedGame::new(
        GameSource::Lutris,
        id,
        name,
        install_path,
    ))
}

fn resolve_install_path(
    exe: Option<&str>,
    prefix: Option<&str>,
    working_dir: Option<&str>,
    directory: Option<&str>,
) -> Option<PathBuf> {
    if let Some(dir) = directory.filter(|s| !s.trim().is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if let Some(wd) = working_dir.filter(|s| !s.trim().is_empty()) {
        return Some(PathBuf::from(wd));
    }
    if let Some(exe) = exe.filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(exe);
        if path.is_absolute() {
            return path.parent().map(|p| p.to_path_buf()).or(Some(path));
        }
        if let Some(prefix) = prefix {
            return Some(PathBuf::from(prefix).join(exe));
        }
        return path.parent().map(|p| p.to_path_buf()).or(Some(path));
    }
    prefix.filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}

/// Scan a Lutris `games/` directory for `*.yml` / `*.yaml`.
pub fn detect_from_lutris_games_dir(games_dir: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(games_dir) else {
        return games;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yml" && ext != "yaml" {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "lutris".into());
        if let Some(game) = parse_lutris_game_yml(&text, &stem) {
            games.push(game);
        }
    }
    games
}

fn yaml_top_level_string(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return Some(unquote(rest.trim()));
        }
    }
    None
}

fn yaml_under_game(text: &str, key: &str) -> Option<String> {
    let mut in_game = false;
    let prefix = format!("{key}:");
    for line in text.lines() {
        let trimmed = line.trim_end();
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_game = trimmed.trim() == "game:";
            continue;
        }
        if !in_game {
            continue;
        }
        let inner = trimmed.trim();
        if let Some(rest) = inner.strip_prefix(&prefix) {
            return Some(unquote(rest.trim()));
        }
    }
    None
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_yml() {
        let text = include_str!("fixtures/lutris/quake-darkplaces.yml");
        let game = parse_lutris_game_yml(text, "quake-darkplaces").expect("yml");
        assert_eq!(game.source, GameSource::Lutris);
        assert_eq!(game.name, "Quake");
        assert_eq!(game.id, "quake");
        assert!(game.install_path.to_string_lossy().contains("Games/quake"));
    }
}
