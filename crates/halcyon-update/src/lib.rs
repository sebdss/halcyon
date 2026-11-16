//! Halcyon Update System (HUS).
//!
//! - [`version`], [`core`] : SemVer strict, canaux Stable/Beta, états, plateformes ;
//! - [`manifest`], [`security`] : manifeste signé, Ed25519, SHA-256, anti-retour, contrôle
//!   de structure des paquets ;
//! - [`network`] : publications GitHub (ETag, requêtes conditionnelles, erreurs d'API) ;
//! - [`download`] : téléchargement reprenable (HTTP Range), progression réelle, annulation ;
//! - [`install`], [`recovery`] : installation transactionnelle (Windows ZIP, Linux AppImage),
//!   contrôle de santé, retour arrière, reprise après coupure, journal.
//!
//! Le lanceur (application Slint) utilise la bibliothèque pour vérifier, télécharger et
//! préparer ; l'installation elle-même est faite par l'exécutable indépendant
//! `halcyon-updater`, lancé depuis une copie pour ne jamais se remplacer lui-même.

pub mod core;
pub mod download;
pub mod manifest;
pub mod network;
pub mod security;
#[cfg(feature = "fault-injection")]
pub mod testkit;
pub mod version;

pub use crate::core::{Channel, PackageFormat, Platform, State};
pub use version::Version;
