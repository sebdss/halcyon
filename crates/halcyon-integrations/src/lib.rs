//! Adaptateurs des plateformes de jeux et lancement.
//!
//! Chaque source implémente [`halcyon_core::GameScanner`] et ne lit que des
//! fichiers locaux publics des clients (manifestes, caches) : aucune
//! authentification n'est contournée, aucun DRM n'est touché. Le lancement des
//! jeux Steam/Epic passe toujours par le client officiel (URI de protocole).

#[macro_use]
extern crate halcyon_i18n;

pub mod archive;
pub mod artwork;
pub mod config_archive;
pub mod emulators;
pub mod engines;
pub mod frametimes;
pub mod epic;
pub mod gog;
pub mod gpu;
pub mod launcher;
pub mod mods;
pub mod news;
pub mod plugin_trust;
pub mod plugins;
pub mod process;
pub mod rahash;
pub mod remote_play;
pub mod retro;
pub mod roms;
pub mod saves;
pub mod standalone;
pub mod steam;
pub mod steam_web;
pub mod steamgriddb;
pub mod sysmon;
pub mod vdf;

use std::path::Path;

/// Lit un fichier texte en tolérant un éventuel BOM UTF-8 et un encodage invalide.
pub(crate) fn read_text(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn io_err(msg: impl Into<String>) -> halcyon_core::CoreError {
    halcyon_core::CoreError::Other(msg.into())
}
