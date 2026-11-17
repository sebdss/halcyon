//! Langue de l'interface : catalogues intégrés au binaire, choix au démarrage (préférence,
//! sinon langue proposée par l'installateur au premier lancement, sinon langue du système,
//! sinon anglais) et changement à chaud.

use halcyon_storage::Settings;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/locales/catalogs.rs"));

/// Fichier écrit par l'installateur à côté de l'exécutable : langue choisie pendant
/// l'installation, proposée par défaut au premier lancement.
pub const INSTALL_LANGUAGE_FILE: &str = "install-language.txt";

/// Enregistre les catalogues et choisit la langue de départ (avant tout texte affiché).
/// `first_run` : aucun fichier de paramètres n'existait encore.
pub fn init(settings: &mut Settings, first_run: bool) -> String {
    if let Err(e) = halcyon_i18n::register(CATALOGS) {
        log::error!("catalogues de traduction invalides : {e}");
    }
    if first_run && settings.language.is_none() {
        if let Some(code) = installer_language() {
            settings.language = Some(code);
        }
    }
    let code = halcyon_i18n::startup_language(settings.language.as_deref());
    halcyon_i18n::set_language(&code);
    log::info!("langue de l'interface : {code} (préférence : {})", settings.language.as_deref().unwrap_or("système"));
    code
}

/// Langue retenue par l'installateur, si elle fait partie des langues disponibles.
pub fn installer_language() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    read_install_language(exe.parent()?)
}

pub fn read_install_language(dir: &Path) -> Option<String> {
    let code = std::fs::read_to_string(dir.join(INSTALL_LANGUAGE_FILE)).ok()?;
    let code = code.trim();
    halcyon_i18n::languages().iter().any(|l| l.code == code).then(|| code.to_string())
}

/// Applique la langue courante aux textes de l'interface Slint (`@tr`).
pub fn apply_to_ui() {
    if let Err(e) = slint::select_bundled_translation(&halcyon_i18n::language()) {
        log::warn!("traduction de l'interface non appliquée : {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_language_file_is_read_only_for_available_languages() {
        halcyon_i18n::register(CATALOGS).unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_install_language(dir.path()), None, "pas de fichier");
        std::fs::write(dir.path().join(INSTALL_LANGUAGE_FILE), "it-IT\n").unwrap();
        assert_eq!(read_install_language(dir.path()).as_deref(), Some("it-IT"));
        std::fs::write(dir.path().join(INSTALL_LANGUAGE_FILE), "ja-JP").unwrap();
        assert_eq!(read_install_language(dir.path()), None, "langue indisponible ignorée");
    }

    #[test]
    fn all_six_languages_are_bundled() {
        halcyon_i18n::register(CATALOGS).unwrap();
        let codes: Vec<String> = halcyon_i18n::languages().into_iter().map(|l| l.code).collect();
        assert_eq!(codes, ["de-DE", "en-US", "es-ES", "fr-FR", "it-IT", "pt-PT"]);
    }
}
