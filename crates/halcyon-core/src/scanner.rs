//! Contrat commun des sources de jeux (Steam, Epic, dossiers…).
//!
//! Ajouter une plateforme revient à implémenter [`GameScanner`] : le reste de
//! l'application (fusion, affichage, lancement) n'a pas à changer.

use crate::model::{DiscoveredGame, GameSource};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Contexte transmis à un scan : annulation coopérative et remontée de progression.
#[derive(Clone)]
pub struct ScanContext {
    cancel: Arc<AtomicBool>,
    progress: Arc<dyn Fn(&str) + Send + Sync>,
}

impl ScanContext {
    pub fn new(cancel: Arc<AtomicBool>, progress: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        Self { cancel, progress }
    }

    /// Contexte sans annulation ni progression (tests, outils).
    pub fn silent() -> Self {
        Self::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}))
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self, msg: &str) {
        (self.progress)(msg)
    }
}

/// Résultat d'un scan.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub games: Vec<DiscoveredGame>,
    /// Problèmes non bloquants (manifeste illisible, dossier inaccessible…).
    pub warnings: Vec<String>,
    /// `true` si la source a été entièrement parcourue : les jeux connus absents
    /// du rapport peuvent alors être marqués comme désinstallés.
    pub complete: bool,
}

pub trait GameScanner: Send + Sync {
    fn source(&self) -> GameSource;

    /// Parcourt la source. Une erreur signifie « source indisponible » :
    /// la bibliothèque existante n'est alors pas modifiée.
    fn scan(&self, ctx: &ScanContext) -> crate::Result<ScanReport>;
}
