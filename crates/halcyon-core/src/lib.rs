//! Cœur de Halcyon : modèle de domaine, contrats d'extension et logique pure.
//!
//! Ce crate ne fait aucune entrée/sortie : tout ce qu'il contient est déterministe
//! et testable unitairement. Les intégrations (Steam, Epic…) et la persistance
//! dépendent de lui, jamais l'inverse.

#[macro_use]
extern crate halcyon_i18n;

pub mod achievements;
pub mod desktop;
pub mod export;
pub mod launch_args;
pub mod model;
pub mod profiles;
pub mod query;
pub mod recommend;
pub mod rules;
pub mod scanner;
pub mod search;
pub mod stats;
pub mod transfer;
pub mod util;

pub use model::*;
pub use query::{LibraryQuery, SortKey, SourceFilter};
pub use search::{QueryContext, SearchQuery};
pub use scanner::{GameScanner, ScanContext, ScanReport};

/// Erreur générique du domaine.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
    #[error("format invalide : {0}")]
    Parse(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
