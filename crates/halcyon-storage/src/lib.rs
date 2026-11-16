//! Persistance locale : base SQLite, configuration JSON, journaux et emplacements.
//!
//! La base locale est la source de vérité de la bibliothèque : elle reste
//! entièrement utilisable hors ligne.

pub mod achievements;
pub mod activity;
pub mod audit;
pub mod db;
pub mod logging;
mod migrations;
pub mod paths;
pub mod perf;
pub mod repo;
pub mod saves_mods;
pub mod secrets;
pub mod settings;
pub mod tracking;

pub use db::Database;
pub use perf::PerfSession;
pub use paths::AppPaths;
pub use activity::{Activity, ActivityKind};
pub use repo::{ArtworkKind, MergeStats};
pub use saves_mods::{ModEntry, ModProfile, ModState, SaveConfig};
pub use tracking::{Session, SmartCollection, VersionChange};
pub use settings::{EmulatorSettings, RomFolder, Settings};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("base de données : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON : {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Conflict(String),
    #[error("jeu introuvable (id {0})")]
    NotFound(i64),
}

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Horodatage Unix courant (secondes).
pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
