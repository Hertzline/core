//! Manual path entries (user-supplied; AC-3.1).

use std::path::PathBuf;

use super::types::{DetectedGame, GameSource};

/// Build a manual detected game from a user-chosen install directory.
pub fn manual_game(name: impl Into<String>, install_path: impl Into<PathBuf>) -> DetectedGame {
    let install_path = install_path.into();
    let name = name.into();
    let id = install_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());
    DetectedGame::new(GameSource::Manual, id, name, install_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_manual_entry() {
        let game = manual_game("Custom Title", "/opt/games/custom");
        assert_eq!(game.source, GameSource::Manual);
        assert_eq!(game.name, "Custom Title");
        assert_eq!(game.id, "custom");
        assert_eq!(game.install_path, PathBuf::from("/opt/games/custom"));
    }
}
