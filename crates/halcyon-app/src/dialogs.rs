//! Boîtes de dialogue natives : Windows (API système), Linux (portail XDG, compatible
//! GNOME, KDE…). Si aucun portail n'est disponible, la boîte ne s'ouvre pas et
//! l'utilisateur peut saisir le chemin dans le champ voisin.

pub const AVAILABLE: bool = true;

pub fn pick_file() -> Option<String> {
    let dialog = rfd::FileDialog::new().set_title(tr!("Choisir un fichier"));
    #[cfg(windows)]
    let dialog = dialog
        .add_filter(tr!("Jeux et raccourcis"), &["exe", "bat", "cmd", "lnk", "url"])
        .add_filter(tr!("Tous les fichiers"), &["*"]);
    dialog.pick_file().map(|p| p.display().to_string())
}

pub fn pick_image() -> Option<String> {
    rfd::FileDialog::new()
        .set_title(tr!("Choisir une image"))
        .add_filter(tr!("Images"), &["jpg", "jpeg", "png", "webp"])
        .pick_file()
        .map(|p| p.display().to_string())
}

pub fn pick_folder() -> Option<String> {
    rfd::FileDialog::new().set_title(tr!("Choisir un dossier")).pick_folder().map(|p| p.display().to_string())
}

/// Modpack à importer (`.mrpack` de Modrinth, `.zip` de CurseForge).
pub fn pick_modpack() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_title(tr!("Importer un modpack")).add_filter("Modpacks", &["mrpack", "zip"]).pick_file()
}

/// Emplacement d'enregistrement d'un fichier.
pub fn save_file(name: &str, ext: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_title(tr!("Enregistrer")).set_file_name(name).add_filter(ext, &[ext]).save_file()
}
