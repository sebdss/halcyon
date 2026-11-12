//! Installations de Minecraft dans Halcyon : une par version, loader ou modpack, chacune avec
//! son dossier de jeu (`mods`, `config`, `saves`…) pour que rien ne se mélange.
//! Elles sont créées automatiquement (pas d'éditeur d'instances personnalisées).

use crate::install::Layout;
use crate::loaders::Loader;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROFILE_FILE: &str = "halcyon-profile.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModpackRef {
    /// `modrinth`, `curseforge` ou `file`.
    pub source: String,
    pub project_id: String,
    pub version_id: String,
    pub name: String,
    pub version: String,
    /// Fichiers posés par le modpack (retirés à la mise à jour), chemins relatifs.
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstalledMod {
    pub file: String,
    /// `modrinth`, `curseforge`, `modpack` ou `local`.
    pub source: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub sha1: String,
    /// Installé comme dépendance d'un autre mod.
    #[serde(default)]
    pub dependency: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mc: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: String,
    /// Version lancée (`1.20.1`, `fabric-loader-0.15.7-1.20.1`…).
    pub version_id: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub last_played: i64,
    /// Temps de jeu mesuré (secondes).
    #[serde(default)]
    pub played_secs: i64,
    #[serde(default)]
    pub modpack: Option<ModpackRef>,
    #[serde(default)]
    pub mods: Vec<InstalledMod>,
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Identifiant de dossier sûr à partir d'un nom.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches(['-', '.']).to_string();
    if out.is_empty() {
        "installation".into()
    } else {
        out
    }
}

impl Profile {
    pub fn game_dir(&self, layout: &Layout) -> PathBuf {
        layout.profiles().join(&self.id)
    }

    pub fn mods_dir(&self, layout: &Layout) -> PathBuf {
        self.game_dir(layout).join("mods")
    }

    pub fn display_loader(&self) -> String {
        if self.loader == Loader::Vanilla {
            "Vanilla".into()
        } else {
            format!("{} {}", self.loader.name(), self.loader_version)
        }
    }
}

pub fn path(layout: &Layout, id: &str) -> PathBuf {
    layout.profiles().join(id).join(PROFILE_FILE)
}

pub fn load(layout: &Layout, id: &str) -> Option<Profile> {
    let p: Profile = serde_json::from_str(&std::fs::read_to_string(path(layout, id)).ok()?).ok()?;
    (p.id == id).then_some(p)
}

pub fn save(layout: &Layout, p: &Profile) -> std::io::Result<()> {
    let dir = layout.profiles().join(&p.id);
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(PROFILE_FILE);
    let tmp = file.with_extension("json.part");
    std::fs::write(&tmp, serde_json::to_vec_pretty(p).unwrap_or_default())?;
    std::fs::rename(tmp, file)
}

pub fn list(layout: &Layout) -> Vec<Profile> {
    let mut out: Vec<Profile> = std::fs::read_dir(layout.profiles())
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().and_then(|id| load(layout, id))).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(b.created.cmp(&a.created)));
    out
}

/// Installation existante pour cette version et ce loader (hors modpack), sinon une nouvelle.
pub fn for_version(layout: &Layout, mc: &str, loader: Loader, loader_version: &str, version_id: &str) -> Profile {
    if let Some(p) = list(layout).into_iter().find(|p| p.modpack.is_none() && p.mc == mc && p.loader == loader) {
        return Profile { loader_version: loader_version.to_string(), version_id: version_id.to_string(), ..p };
    }
    let name = if loader == Loader::Vanilla { format!("Minecraft {mc}") } else { format!("{} {mc}", loader.name()) };
    new_profile(layout, &name, mc, loader, loader_version, version_id)
}

/// Nouvelle installation, identifiant unique.
pub fn new_profile(layout: &Layout, name: &str, mc: &str, loader: Loader, loader_version: &str, version_id: &str) -> Profile {
    let base = slug(name);
    let mut id = base.clone();
    let mut n = 2;
    while layout.profiles().join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    Profile {
        id,
        name: name.to_string(),
        mc: mc.to_string(),
        loader,
        loader_version: loader_version.to_string(),
        version_id: version_id.to_string(),
        created: now(),
        last_played: 0,
        played_secs: 0,
        modpack: None,
        mods: Vec::new(),
    }
}

/// Supprime une installation et **tout** son dossier (mondes compris) : à n'appeler qu'après
/// confirmation explicite.
pub fn delete(layout: &Layout, id: &str) -> std::io::Result<()> {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return Err(std::io::Error::other("identifiant refusé"));
    }
    let dir = layout.profiles().join(id);
    if !dir.join(PROFILE_FILE).is_file() {
        return Err(std::io::Error::other("ce dossier n'est pas une installation Halcyon"));
    }
    std::fs::remove_dir_all(dir)
}

/// Mondes de l'installation (dossiers de `saves`).
pub fn worlds(game_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(game_dir.join("saves"))
        .map(|rd| rd.flatten().filter(|e| e.path().join("level.dat").is_file()).filter_map(|e| e.file_name().to_str().map(str::to_string)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_isolated_and_reused() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path());
        assert_eq!(slug("Fabric 1.20.1 / Test !"), "fabric-1.20.1-test");
        assert_eq!(slug("///"), "installation");
        let p = for_version(&l, "1.20.1", Loader::Fabric, "0.15.7", "fabric-loader-0.15.7-1.20.1");
        assert_eq!(p.id, "fabric-1.20.1");
        save(&l, &p).unwrap();
        let again = for_version(&l, "1.20.1", Loader::Fabric, "0.16.0", "fabric-loader-0.16.0-1.20.1");
        assert_eq!(again.id, p.id, "même installation, loader mis à jour");
        assert_eq!(again.loader_version, "0.16.0");
        let other = for_version(&l, "1.20.1", Loader::Forge, "47.2.0", "1.20.1-forge-47.2.0");
        assert_ne!(other.id, p.id);
        let dup = new_profile(&l, "Fabric 1.20.1", "1.20.1", Loader::Fabric, "", "x");
        assert_eq!(dup.id, "fabric-1.20.1-2");
        assert_eq!(list(&l).len(), 1);
        std::fs::create_dir_all(p.game_dir(&l).join("saves/Monde/")).unwrap();
        std::fs::write(p.game_dir(&l).join("saves/Monde/level.dat"), b"x").unwrap();
        assert_eq!(worlds(&p.game_dir(&l)), vec!["Monde".to_string()]);
        assert!(delete(&l, "..").is_err());
        std::fs::create_dir_all(l.profiles().join("autre")).unwrap();
        assert!(delete(&l, "autre").is_err(), "dossier non géré conservé");
        delete(&l, &p.id).unwrap();
        assert!(list(&l).is_empty());
    }
}
