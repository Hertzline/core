//! Detected game record and launcher source labels.

use std::path::PathBuf;

/// Launcher or origin of a detected install (AC-3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSource {
    Steam,
    Epic,
    Gog,
    Xbox,
    BattleNet,
    Lutris,
    Heroic,
    Manual,
}

impl GameSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Steam => "steam",
            Self::Epic => "epic",
            Self::Gog => "gog",
            Self::Xbox => "xbox",
            Self::BattleNet => "battlenet",
            Self::Lutris => "lutris",
            Self::Heroic => "heroic",
            Self::Manual => "manual",
        }
    }
}

/// One installed game found via a documented path (AC-3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedGame {
    pub source: GameSource,
    /// Stable id within the source (app id, catalog id, slug, or path hash).
    pub id: String,
    pub name: String,
    pub install_path: PathBuf,
}

impl DetectedGame {
    pub fn new(
        source: GameSource,
        id: impl Into<String>,
        name: impl Into<String>,
        install_path: PathBuf,
    ) -> Self {
        Self {
            source,
            id: id.into(),
            name: name.into(),
            install_path,
        }
    }
}
