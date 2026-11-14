//! Game detection from documented launcher paths only (HZ-027 / AC-3.1, AC-3.2).
//!
//! Sources: Steam, Epic, GOG, Xbox, Battle.net, Lutris, Heroic, manual.
//! Never scrapes undocumented APIs, reads game process memory, or injects.

mod battlenet;
mod detect;
mod epic;
mod gog;
mod heroic;
mod lutris;
mod manual;
mod steam;
mod types;
mod vdf;
mod xbox;

pub use battlenet::{parse_battlenet_product_db, BATTLENET_PRODUCT_DB_REL};
pub use detect::{detect_games, DetectionRoots};
pub use epic::parse_epic_item;
pub use gog::parse_gog_info;
pub use heroic::{parse_heroic_sideload_library, parse_legendary_installed};
pub use lutris::parse_lutris_game_yml;
pub use manual::manual_game;
pub use steam::{parse_appmanifest_acf, parse_libraryfolders_vdf};
pub use types::{DetectedGame, GameSource};
pub use xbox::parse_microsoft_game_config;
