//! Visuels des jeux émulés depuis **libretro-thumbnails** (projet RetroArch) : jaquettes,
//! captures en jeu et écrans titres, gratuits, sans clé ni compte.
//!
//! Les images sont rangées par système (`Nintendo - Super Nintendo Entertainment System`) et
//! nommées d'après le titre No-Intro du jeu, celui des ROM bien nommées
//! (`Super Mario World (USA)`). Les caractères `&*/:`<>?\|"` y sont remplacés par `_`.
//! Téléchargement en HTTPS depuis les dépôts GitHub du projet.

use crate::plugins::encode;
use std::path::Path;

const RAW: &str = "https://raw.githubusercontent.com/libretro-thumbnails";

/// Types d'images proposés par libretro-thumbnails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumb {
    /// Jaquette (boîte).
    Boxart,
    /// Capture en jeu.
    Snap,
    /// Écran titre.
    Title,
}

impl Thumb {
    fn folder(self) -> &'static str {
        match self {
            Thumb::Boxart => "Named_Boxarts",
            Thumb::Snap => "Named_Snaps",
            Thumb::Title => "Named_Titles",
        }
    }
}

/// Nom de fichier d'une image pour un titre (caractères réservés remplacés par `_`).
pub fn file_name(title: &str) -> String {
    let cleaned: String = title.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect();
    format!("{}.png", cleaned.trim())
}

/// Adresse d'une image. `system` : nom du système libretro (« Sony - PlayStation »).
pub fn url(system: &str, kind: Thumb, title: &str) -> String {
    let repo = system.replace(' ', "_");
    format!("{RAW}/{}/master/{}/{}", encode(&repo), kind.folder(), encode(&file_name(title)))
}

/// Titres à essayer pour un jeu : nom du fichier de la ROM (souvent le titre No-Intro exact),
/// puis nom affiché dans Halcyon s'il diffère. Un jeu en dossier (PS3) n'a que son nom.
pub fn titles(rom: Option<&Path>, game_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(stem) = rom.filter(|r| r.is_file() || r.extension().is_some()).and_then(|r| r.file_stem()).and_then(|s| s.to_str()) {
        let stem = stem.trim();
        if !stem.is_empty() {
            out.push(stem.to_string());
        }
    }
    let name = game_name.trim();
    if !name.is_empty() && !out.iter().any(|t| t == name) {
        out.push(name.to_string());
    }
    out
}

/// Adresses à essayer dans l'ordre pour un type d'image.
pub fn candidates(system: &str, kind: Thumb, titles: &[String]) -> Vec<String> {
    titles.iter().map(|t| url(system, kind, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn names_and_urls() {
        assert_eq!(file_name("Q*bert's Qubes"), "Q_bert's Qubes.png");
        assert_eq!(file_name("Beavis & Butt-Head (USA)"), "Beavis _ Butt-Head (USA).png");
        assert_eq!(file_name("Batman: Return"), "Batman_ Return.png");
        assert_eq!(
            url("Nintendo - Super Nintendo Entertainment System", Thumb::Boxart, "Super Mario World (USA)"),
            "https://raw.githubusercontent.com/libretro-thumbnails/Nintendo_-_Super_Nintendo_Entertainment_System/master/Named_Boxarts/Super%20Mario%20World%20%28USA%29.png"
        );
    }

    #[test]
    fn titles_from_rom_then_name() {
        let rom = PathBuf::from("/roms/snes/Super Mario World (USA).sfc");
        assert_eq!(titles(Some(&rom), "Super Mario World"), ["Super Mario World (USA)", "Super Mario World"]);
        assert_eq!(titles(Some(&rom), "Super Mario World (USA)"), ["Super Mario World (USA)"]);
        assert_eq!(titles(None, "Wipeout"), ["Wipeout"]);
    }
}
