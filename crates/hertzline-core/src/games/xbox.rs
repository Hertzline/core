//! Xbox PC / Game Pass detection via documented flat-file layout.
//!
//! Microsoft GDK docs: packages install under `[drive]:\XboxGames` with
//! `MicrosoftGame.config` in the content root
//! (`[drive]:\XboxGames\<Game>\Content\MicrosoftGame.config`).
//! Drive markers: hidden `.GamingRoot` at the drive root (Xbox app).
//! We read only those files; no injection or memory scrape.

use std::fs;
use std::path::{Path, PathBuf};

use super::types::{DetectedGame, GameSource};

/// Parse `MicrosoftGame.config` XML for DisplayName / Name / Identity Name.
pub fn parse_microsoft_game_config(text: &str, content_dir: &Path) -> Option<DetectedGame> {
    let name = xml_attr_or_elem(text, "ShellVisuals", "DefaultDisplayName")
        .or_else(|| xml_elem_text(text, "Name"))
        .or_else(|| xml_attr_or_elem(text, "Identity", "Name"))
        .filter(|s| !s.trim().is_empty())?;
    let id = xml_attr_or_elem(text, "Identity", "Name")
        .or_else(|| xml_elem_text(text, "Name"))
        .unwrap_or_else(|| name.clone());
    let install_path = content_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| content_dir.to_path_buf());
    Some(DetectedGame::new(GameSource::Xbox, id, name, install_path))
}

/// Read `.GamingRoot` (UTF-16LE or UTF-8) for a relative/absolute XboxGames path.
pub fn parse_gaming_root(bytes: &[u8]) -> Option<PathBuf> {
    let text = decode_gaming_root(bytes)?;
    let trimmed = text.trim().trim_matches('\0').trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn decode_gaming_root(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 2 && bytes[1] == 0 {
        // UTF-16LE heuristic
        let (chunks, _) = bytes.as_chunks::<2>();
        let u16s: Vec<u16> = chunks
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&u16s).ok()
    } else {
        String::from_utf8(bytes.to_vec()).ok()
    }
}

/// Scan an XboxGames root for `*/Content/MicrosoftGame.config`.
pub fn detect_from_xbox_games_root(xbox_games: &Path) -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(xbox_games) else {
        return games;
    };
    for entry in entries.flatten() {
        let game_dir = entry.path();
        if !game_dir.is_dir() {
            continue;
        }
        let config = game_dir.join("Content").join("MicrosoftGame.config");
        if !config.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&config) else {
            continue;
        };
        if let Some(game) = parse_microsoft_game_config(&text, &game_dir.join("Content")) {
            games.push(game);
        }
    }
    games
}

fn xml_elem_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let after_open = &text[start..];
    let gt = after_open.find('>')?;
    let inner_start = start + gt + 1;
    let end = text[inner_start..].find(&close)? + inner_start;
    let value = text[inner_start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn xml_attr_or_elem(text: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = text.find(&open)?;
    let after = &text[start..];
    let end = after.find('>').map(|i| start + i + 1).unwrap_or(text.len());
    let slice = &text[start..end];
    let needle = format!("{attr}=\"");
    if let Some(a) = slice.find(&needle) {
        let rest = &slice[a + needle.len()..];
        if let Some(q) = rest.find('"') {
            let v = &rest[..q];
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    xml_elem_text(text, attr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_config() {
        let text = include_str!("fixtures/xbox/MicrosoftGame.config");
        let game = parse_microsoft_game_config(text, Path::new("C:/XboxGames/Sample/Content"))
            .expect("cfg");
        assert_eq!(game.source, GameSource::Xbox);
        assert_eq!(game.name, "Sample Game");
        assert_eq!(game.install_path, PathBuf::from("C:/XboxGames/Sample"));
    }

    #[test]
    fn parses_utf16_gaming_root() {
        let path = "XboxGames";
        let mut bytes = Vec::new();
        for u in path.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let parsed = parse_gaming_root(&bytes).expect("root");
        assert_eq!(parsed, PathBuf::from("XboxGames"));
    }
}
