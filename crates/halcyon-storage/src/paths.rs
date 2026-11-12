//! Emplacements des données de l'application.
//!
//! Ordre de résolution :
//! 1. variable d'environnement `HALCYON_DATA_DIR` (tests, développement) ;
//! 2. mode portable : fichier `portable.txt` à côté de l'exécutable → dossier `data/` voisin ;
//! 3. emplacements standards du système (`%APPDATA%\Halcyon`, `%LOCALAPPDATA%\Halcyon`).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    /// Base de données et configuration (à sauvegarder).
    pub data_dir: PathBuf,
    /// Données reconstructibles (miniatures, images téléchargées).
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        if let Some(dir) = std::env::var_os("HALCYON_DATA_DIR") {
            return Self::rooted(PathBuf::from(dir));
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            if exe_dir.join("portable.txt").exists() {
                return Self::rooted(exe_dir.join("data"));
            }
        }
        let data = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Halcyon");
        let local = dirs::data_local_dir().unwrap_or_else(|| data.clone()).join("Halcyon");
        let cache = dirs::cache_dir().map(|c| c.join("Halcyon")).unwrap_or_else(|| local.join("cache"));
        Self {
            data_dir: data,
            // Sous Windows, cache_dir == data_local_dir : on isole le cache dans un sous-dossier.
            cache_dir: if cache == local { local.join("cache") } else { cache },
            logs_dir: local.join("logs"),
        }
    }

    /// Tout sous une même racine (mode portable, tests).
    pub fn rooted(root: PathBuf) -> Self {
        Self { data_dir: root.clone(), cache_dir: root.join("cache"), logs_dir: root.join("logs") }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("library.db")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    /// Miniatures redimensionnées pour l'affichage.
    pub fn thumbs_dir(&self) -> PathBuf {
        self.cache_dir.join("thumbs")
    }

    /// Visuels originaux téléchargés depuis Internet.
    pub fn artwork_dir(&self) -> PathBuf {
        self.cache_dir.join("artwork")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [&self.data_dir, &self.cache_dir, &self.logs_dir, &self.thumbs_dir(), &self.artwork_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}
