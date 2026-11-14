//! Battle.net detection via documented `product.db`.
//!
//! Path: `C:\ProgramData\Battle.net\Agent\product.db` (Blizzard Agent; also
//! referenced by Lutris `BlizzardProductDbParser`). The file is a protobuf
//! ProductDb blob; install paths and product codes appear as UTF-8 strings.
//! We extract path-like strings and nearby product codes without spawning
//! Battle.net or reading process memory.

use std::path::PathBuf;

use super::types::{DetectedGame, GameSource};

/// Relative Agent path under ProgramData / prefix ProgramData.
pub const BATTLENET_PRODUCT_DB_REL: &str = "Battle.net/Agent/product.db";

/// Known non-game Agent product codes (Lutris NOT_GAMES).
const NOT_GAMES: &[&str] = &["bna", "agent"];

/// Parse product.db bytes into detected installs (path + product code pairs).
pub fn parse_battlenet_product_db(bytes: &[u8]) -> Vec<DetectedGame> {
    // Product codes are short (e.g. "wow", "d3"); paths are longer.
    let strings = extract_printable_strings(bytes, 2);
    let mut games = Vec::new();
    let mut i = 0;
    while i < strings.len() {
        let s = &strings[i];
        if looks_like_install_path(s) {
            let code = find_nearby_product_code(&strings, i);
            if let Some(code) = code {
                if !NOT_GAMES.contains(&code.as_str()) {
                    games.push(DetectedGame::new(
                        GameSource::BattleNet,
                        code.clone(),
                        code,
                        PathBuf::from(s),
                    ));
                }
            }
        }
        i += 1;
    }
    games
}

fn find_nearby_product_code(strings: &[String], path_index: usize) -> Option<String> {
    // Prefer a short alphanumeric code immediately before the path.
    let before_start = path_index.saturating_sub(3);
    for code in strings[before_start..path_index].iter().rev() {
        if is_product_code(code) {
            return Some(code.clone());
        }
    }
    let after_end = (path_index + 3).min(strings.len());
    for code in &strings[path_index + 1..after_end] {
        if is_product_code(code) {
            return Some(code.clone());
        }
    }
    None
}

fn is_product_code(s: &str) -> bool {
    let len = s.len();
    if !(2..=32).contains(&len) {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && s.chars().any(|c| c.is_ascii_alphabetic())
        && !looks_like_install_path(s)
}

fn looks_like_install_path(s: &str) -> bool {
    if s.contains(":\\") || s.contains(":/") {
        return true;
    }
    s.starts_with('/') && s.len() > 3 && s.contains('/')
}

fn extract_printable_strings(bytes: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for &b in bytes {
        if (32..=126).contains(&b) {
            current.push(b);
        } else if current.len() >= min_len {
            if let Ok(s) = String::from_utf8(current.clone()) {
                out.push(s);
            }
            current.clear();
        } else {
            current.clear();
        }
    }
    if current.len() >= min_len {
        if let Ok(s) = String::from_utf8(current) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_product_db_strings() {
        let bytes = include_bytes!("fixtures/battlenet/product.db");
        let games = parse_battlenet_product_db(bytes);
        assert!(
            games.iter().any(|g| g.id == "wow"
                && g.install_path
                    .to_string_lossy()
                    .contains("World of Warcraft")),
            "expected wow install, got {games:?}"
        );
        assert!(!games.iter().any(|g| g.id == "agent" || g.id == "bna"));
    }
}
