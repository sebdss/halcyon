//! Préférence de carte graphique sous Windows (portables à double GPU).
//!
//! Windows mémorise ce choix par exécutable dans
//! `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` (le même réglage que
//! Paramètres > Affichage > Graphiques). Halcyon ne le modifie qu'avec
//! l'autorisation explicite du joueur, et rétablit la valeur précédente à la
//! fin de la session.

use halcyon_core::profiles::GpuPreference;
use std::path::Path;

/// Valeur d'origine à rétablir après la session.
#[derive(Debug)]
pub struct GpuGuard {
    #[allow(dead_code)]
    exe: String,
    #[allow(dead_code)]
    previous: Option<String>,
}

#[cfg(windows)]
const KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

pub fn value_for(pref: GpuPreference) -> Option<&'static str> {
    match pref {
        GpuPreference::Default => None,
        GpuPreference::PowerSaving => Some("GpuPreference=1;"),
        GpuPreference::HighPerformance => Some("GpuPreference=2;"),
    }
}

#[cfg(windows)]
pub fn apply(exe: &Path, pref: GpuPreference) -> Result<GpuGuard, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;
    let value = value_for(pref).ok_or(tr!("aucune préférence à appliquer"))?;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(KEY, KEY_READ | KEY_WRITE)
        .map_err(|e| tr!("préférence graphique inaccessible : {e}", e = e))?;
    let name = exe.to_string_lossy().into_owned();
    let previous: Option<String> = key.get_value(&name).ok();
    key.set_value(&name, &value).map_err(|e| tr!("préférence graphique non modifiée : {e}", e = e))?;
    Ok(GpuGuard { exe: name, previous })
}

#[cfg(not(windows))]
pub fn apply(_exe: &Path, _pref: GpuPreference) -> Result<GpuGuard, String> {
    Err(tr!("préférence graphique du système : Windows uniquement").into())
}

impl GpuGuard {
    /// Rétablit la valeur d'origine (ou supprime celle ajoutée par Halcyon).
    pub fn restore(self) {
        #[cfg(windows)]
        {
            use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
            use winreg::RegKey;
            if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(KEY, KEY_WRITE) {
                let r = match &self.previous {
                    Some(v) => key.set_value(&self.exe, v),
                    None => key.delete_value(&self.exe),
                };
                if let Err(e) = r {
                    log::warn!("préférence graphique non rétablie pour {} : {e}", self.exe);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_match_windows_settings() {
        assert_eq!(value_for(GpuPreference::HighPerformance), Some("GpuPreference=2;"));
        assert_eq!(value_for(GpuPreference::Default), None);
        #[cfg(not(windows))]
        assert!(apply(Path::new("x.exe"), GpuPreference::HighPerformance).is_err());
    }
}
