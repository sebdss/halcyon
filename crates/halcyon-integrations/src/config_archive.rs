//! Archive de configuration : export complet des données de Halcyon (bibliothèque,
//! métadonnées personnalisées, collections, profils, émulateurs, paramètres, thèmes,
//! statistiques locales) et restauration sur ce PC ou un autre.
//!
//! Les fichiers des jeux eux-mêmes ne sont jamais inclus. Les clés d'API ne le sont
//! que sur demande explicite.

use crate::archive::{self, entry_name, walk_files};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

pub const FORMAT: u32 = 1;
pub const MANIFEST: &str = "halcyon-archive.json";

/// Dossiers de données inclus dans l'archive (relatifs au dossier de données).
pub const DIRS: &[&str] = &["custom-artwork", "emulators", "platforms", "themes", "plugins"];

/// Champs de `settings.json` contenant des secrets (conservés dans le coffre `secrets/`
/// depuis la 1.0.0 ; réinjectés dans l'archive seulement sur demande explicite).
use halcyon_storage::settings::SECRET_FIELDS as SECRET_KEYS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: i64,
    pub machine: String,
    pub games: usize,
    pub includes_secrets: bool,
    pub includes_mod_files: bool,
    pub sections: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    pub include_secrets: bool,
    pub include_mod_files: bool,
}

/// Construit l'archive. `db_copy` : copie cohérente de la base (VACUUM INTO).
pub fn export(data_dir: &Path, db_copy: &Path, dest: &Path, opts: &ExportOptions, mut manifest: Manifest) -> io::Result<u64> {
    let mut entries: Vec<(String, PathBuf)> = vec![("library.db".into(), db_copy.to_path_buf())];
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    let mut sections = vec![tr!("bibliothèque"), "statistiques".to_string()];

    let settings = data_dir.join("settings.json");
    if let Ok(bytes) = std::fs::read(&settings) {
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if let Some(obj) = v.as_object_mut() {
            let vault = halcyon_storage::settings::secret_store(&settings);
            for k in SECRET_KEYS {
                // Le coffre n'est jamais copié tel quel : sous Windows son contenu est lié au
                // compte (DPAPI) et serait illisible ailleurs. Les clés passent en clair dans
                // l'archive uniquement si l'utilisateur l'a demandé ; à la restauration,
                // `Settings::load` les replace dans le coffre.
                let value = if opts.include_secrets { vault.get(k).or_else(|| obj.get(*k).and_then(|x| x.as_str()).map(String::from)).unwrap_or_default() } else { String::new() };
                obj.insert((*k).to_string(), serde_json::Value::String(value));
            }
        }
        extra.push(("settings.json".into(), serde_json::to_vec_pretty(&v).map_err(io::Error::other)?));
        sections.push(tr!("paramètres").into());
    }
    for dir in DIRS {
        let base = data_dir.join(dir);
        if !base.is_dir() {
            continue;
        }
        let files = walk_files(&base, 50_000)?;
        if !files.is_empty() {
            sections.push((*dir).to_string());
        }
        for f in files {
            entries.push((entry_name(&format!("{dir}/"), &f), base.join(&f)));
        }
    }
    // Mods : métadonnées toujours, fichiers sur demande ; jamais l'état de déploiement
    // (propre au dossier du jeu de ce PC) ni les originaux mis de côté.
    let mods = data_dir.join("mods");
    if mods.is_dir() {
        for f in walk_files(&mods, 500_000)? {
            let rel = entry_name("", &f);
            let parts: Vec<&str> = rel.split('/').collect();
            let keep = match parts.as_slice() {
                [_, "library", _, "mod.json"] => true,
                [_, "library", _, "files", ..] => opts.include_mod_files,
                _ => false,
            };
            if keep {
                entries.push((format!("mods/{rel}"), mods.join(&f)));
            }
        }
        sections.push("mods".into());
    }
    manifest.format = FORMAT;
    manifest.includes_secrets = opts.include_secrets;
    manifest.includes_mod_files = opts.include_mod_files;
    manifest.sections = sections;
    extra.insert(0, (MANIFEST.into(), serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?));
    archive::write_zip(dest, &entries, &extra)?;
    Ok(dest.metadata()?.len())
}

/// Lit et valide le manifeste d'une archive.
pub fn inspect(path: &Path) -> io::Result<Manifest> {
    let bytes = archive::read_entry(path, MANIFEST)
        .map_err(|_| io::Error::other(tr!("ce fichier n'est pas une archive de configuration Halcyon")))?;
    let m: Manifest = serde_json::from_slice(&bytes).map_err(|e| io::Error::other(tr!("manifeste illisible : {e}", e = e)))?;
    if m.format > FORMAT {
        return Err(io::Error::other(tr!("archive créée par une version plus récente de Halcyon")));
    }
    if !archive::list_entries(path)?.iter().any(|n| n == "library.db") {
        return Err(io::Error::other(tr!("archive incomplète : base de la bibliothèque absente")));
    }
    Ok(m)
}

/// Extrait l'archive dans un dossier de préparation (vide), sans toucher aux données.
pub fn extract(path: &Path, staging: &Path) -> io::Result<usize> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    archive::extract_prefix(path, "", staging)
}

/// Remplace les données par celles d'un dossier préparé. Les éléments remplacés
/// sont déplacés dans `previous` (récupérables). La base doit être fermée.
pub fn install(staging: &Path, data_dir: &Path, previous: &Path) -> io::Result<()> {
    std::fs::create_dir_all(previous)?;
    let mut items: Vec<String> = vec!["library.db".into(), "settings.json".into(), "mods".into()];
    items.extend(DIRS.iter().map(|d| d.to_string()));
    for item in &items {
        let src = staging.join(item);
        if !src.exists() {
            continue;
        }
        let dest = data_dir.join(item);
        if dest.exists() {
            std::fs::rename(&dest, previous.join(item))?;
        }
        if item == "library.db" {
            for ext in ["library.db-wal", "library.db-shm"] {
                let side = data_dir.join(ext);
                if side.exists() {
                    std::fs::rename(&side, previous.join(ext))?;
                }
            }
        }
        if item == "mods" {
            // Les fichiers déployés et originaux de ce PC sont conservés.
            if let Ok(games) = std::fs::read_dir(previous.join("mods")) {
                for g in games.flatten() {
                    for keep in ["deployed.json", "backup"] {
                        let p = g.path().join(keep);
                        if p.exists() {
                            let to = src.join(g.file_name()).join(keep);
                            if let Some(parent) = to.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::rename(&p, &to)?;
                        }
                    }
                }
            }
        }
        std::fs::rename(&src, &dest).or_else(|_| archive::copy_dir(&src, &dest).map(|_| ()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn manifest() -> Manifest {
        Manifest {
            format: 0,
            app_version: "test".into(),
            schema_version: 4,
            created_at: 1,
            machine: "pc".into(),
            games: 3,
            includes_secrets: false,
            includes_mod_files: false,
            sections: vec![],
        }
    }

    #[test]
    fn export_strips_secrets_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        fs::create_dir_all(data.join("custom-artwork")).unwrap();
        fs::write(data.join("custom-artwork/1-cover.png"), b"png").unwrap();
        fs::write(data.join("settings.json"), r#"{"steam_api_key":"SECRET","card_width":168}"#).unwrap();
        fs::create_dir_all(data.join("mods/pc-g/library/m/files")).unwrap();
        fs::write(data.join("mods/pc-g/library/m/mod.json"), b"{}").unwrap();
        fs::write(data.join("mods/pc-g/library/m/files/big.pak"), b"data").unwrap();
        fs::write(data.join("mods/pc-g/deployed.json"), b"{}").unwrap();
        let db = tmp.path().join("copy.db");
        fs::write(&db, b"SQLite").unwrap();
        fs::write(data.join("library.db"), b"old").unwrap();

        let out = tmp.path().join("export.zip");
        export(&data, &db, &out, &ExportOptions::default(), manifest()).unwrap();
        let m = inspect(&out).unwrap();
        assert_eq!(m.format, FORMAT);
        assert!(!m.includes_secrets);
        let names = archive::list_entries(&out).unwrap();
        assert!(names.contains(&"mods/pc-g/library/m/mod.json".to_string()));
        assert!(!names.iter().any(|n| n.ends_with("big.pak") || n.ends_with("deployed.json")));
        let settings: serde_json::Value = serde_json::from_slice(&archive::read_entry(&out, "settings.json").unwrap()).unwrap();
        assert_eq!(settings["steam_api_key"], "", "clé d'API retirée");
        assert_eq!(settings["card_width"], 168);

        let staging = tmp.path().join("staging");
        extract(&out, &staging).unwrap();
        let prev = tmp.path().join("previous");
        install(&staging, &data, &prev).unwrap();
        assert_eq!(fs::read(data.join("library.db")).unwrap(), b"SQLite");
        assert_eq!(fs::read(prev.join("library.db")).unwrap(), b"old", "ancienne base conservée");
        assert!(data.join("mods/pc-g/deployed.json").exists(), "état de déploiement local conservé");
        assert!(data.join("custom-artwork/1-cover.png").exists());

        assert!(inspect(&tmp.path().join("copy.db")).is_err());
    }
}
