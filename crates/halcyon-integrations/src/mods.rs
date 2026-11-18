//! Gestionnaire de mods : bibliothèque de mods par jeu, déploiement réversible
//! selon un adaptateur propre au jeu, détection des conflits.
//!
//! Aucun gestionnaire unique n'est imposé : chaque jeu utilise un adaptateur
//! (dossier générique, BepInEx pour Unity, paquets `.pak` pour Unreal). Le
//! déploiement copie les fichiers dans le dossier du jeu en sauvegardant au
//! préalable tout fichier d'origine remplacé ; le retrait remet le jeu dans son
//! état initial. Un fichier modifié depuis le déploiement n'est jamais supprimé.

use crate::archive::{self, entry_name, safe_relative, sha256_file, walk_files};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// Méthode d'installation des mods d'un jeu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adapter {
    /// Fichiers copiés tels quels sous `target` (relatif au dossier du jeu, vide = racine).
    Generic { target: String },
    /// Greffons BepInEx (jeux Unity) : `BepInEx/plugins/<mod>/…`.
    BepInEx,
    /// Paquets Unreal (`.pak`, `.utoc`, `.ucas`) : `<paks>/~mods/`.
    UnrealPaks { paks: String },
}

impl Adapter {
    pub fn label(&self) -> String {
        match self {
            Adapter::Generic { target } if target.is_empty() => tr!("Copie dans le dossier du jeu"),
            Adapter::Generic { target } => tr!("Copie dans {target}", target = target),
            Adapter::BepInEx => "BepInEx (Unity)".into(),
            Adapter::UnrealPaks { paks } => tr!("Paquets Unreal ({paks}/~mods)", paks = paks),
        }
    }

    /// Dossiers (relatifs au jeu) où des mods externes peuvent se trouver.
    pub fn watched_dirs(&self) -> Vec<String> {
        match self {
            Adapter::Generic { target } if !target.is_empty() => vec![target.clone()],
            Adapter::Generic { .. } => vec![],
            Adapter::BepInEx => vec!["BepInEx/plugins".into()],
            Adapter::UnrealPaks { paks } => vec![format!("{paks}/~mods")],
        }
    }

    /// Destination d'un fichier de mod, ou `None` s'il ne concerne pas cet adaptateur.
    pub fn map(&self, mod_name: &str, rel: &str) -> Option<String> {
        let lower = rel.to_lowercase();
        match self {
            Adapter::Generic { target } => Some(if target.is_empty() { rel.to_string() } else { format!("{target}/{rel}") }),
            Adapter::BepInEx => {
                // Archive déjà structurée (BepInEx/plugins/…, BepInEx/config/…) : conservée.
                if lower.starts_with("bepinex/") {
                    return Some(rel.to_string());
                }
                if let Some(rest) = strip_ci(rel, "plugins/") {
                    return Some(format!("BepInEx/plugins/{rest}"));
                }
                if let Some(rest) = strip_ci(rel, "config/") {
                    return Some(format!("BepInEx/config/{rest}"));
                }
                Some(format!("BepInEx/plugins/{}/{rel}", sanitize(mod_name)))
            }
            Adapter::UnrealPaks { paks } => {
                let file = rel.rsplit('/').next().unwrap_or(rel);
                let ext = file.rsplit('.').next().unwrap_or("").to_lowercase();
                matches!(ext.as_str(), "pak" | "utoc" | "ucas" | "sig").then(|| format!("{paks}/~mods/{file}"))
            }
        }
    }
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() > prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)).then(|| &s[prefix.len()..])
}

fn sanitize(s: &str) -> String {
    let c: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    let c = c.trim().to_string();
    if c.is_empty() { "mod".into() } else { c }
}

/// Adaptateur conseillé d'après les fichiers du jeu, avec explication.
pub fn suggest(install_dir: &Path) -> (Adapter, String) {
    if install_dir.join("BepInEx").is_dir() {
        return (Adapter::BepInEx, tr!("BepInEx est installé dans ce jeu."));
    }
    // Unreal : premier dossier contenant Content/Paks.
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for e in entries.flatten().take(300) {
            let paks = e.path().join("Content").join("Paks");
            if paks.is_dir() {
                let rel = format!("{}/Content/Paks", e.file_name().to_string_lossy());
                return (Adapter::UnrealPaks { paks: rel }, tr!("Jeu Unreal Engine : les mods sont des paquets .pak."));
            }
        }
    }
    if crate::engines::detect(install_dir) == halcyon_core::profiles::Engine::Unity {
        return (
            Adapter::Generic { target: String::new() },
            tr!("Jeu Unity sans BepInEx : installez BepInEx (en tant que mod) puis choisissez l'adaptateur BepInEx."),
        );
    }
    (Adapter::Generic { target: String::new() }, tr!("Méthode générique : les fichiers du mod sont copiés dans le dossier du jeu."))
}

/// Mod de la bibliothèque.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    /// Fichier ou dossier d'origine.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub nexus_id: Option<u64>,
    #[serde(default)]
    pub latest_version: Option<String>,
    pub added_at: i64,
    pub files: usize,
    pub size: u64,
}

/// Fichier déployé dans le jeu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployedFile {
    pub rel: String,
    pub mod_id: String,
    pub hash: String,
    /// Un fichier d'origine a été mis de côté et sera remis au retrait.
    pub backed_up: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Deployment {
    pub files: Vec<DeployedFile>,
    pub profile: Option<i64>,
    pub at: i64,
    /// Dossiers créés par Halcyon (retirés s'ils sont vides).
    #[serde(default)]
    pub created_dirs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub rel: String,
    /// Mods fournissant ce fichier, dans l'ordre de chargement ; le dernier l'emporte.
    pub mods: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// (destination relative, mod, fichier source)
    pub files: Vec<(String, String, PathBuf)>,
    pub conflicts: Vec<Conflict>,
    /// Fichiers ignorés par l'adaptateur (ex. readme dans un mod Unreal).
    pub ignored: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UndeployReport {
    pub removed: usize,
    pub restored: usize,
    /// Fichiers modifiés depuis le déploiement : laissés en place.
    pub kept_modified: Vec<String>,
}

/// Dépôt des mods : `<racine>/<jeu>/library/<mod>/{files/, mod.json}`, `backup/`, `deployed.json`.
#[derive(Clone, Debug)]
pub struct ModStore {
    pub root: PathBuf,
}

impl ModStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn game(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    fn mod_dir(&self, key: &str, id: &str) -> PathBuf {
        self.game(key).join("library").join(id)
    }

    pub fn list(&self, key: &str) -> Vec<ModInfo> {
        let Ok(entries) = std::fs::read_dir(self.game(key).join("library")) else { return vec![] };
        let mut out: Vec<ModInfo> = entries
            .flatten()
            .filter_map(|e| std::fs::read(e.path().join("mod.json")).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    pub fn save_info(&self, key: &str, info: &ModInfo) -> io::Result<()> {
        let p = self.mod_dir(key, &info.id).join("mod.json");
        std::fs::write(&p, serde_json::to_vec_pretty(info).map_err(io::Error::other)?)
    }

    /// Importe un mod depuis une archive `.zip` ou un dossier.
    pub fn import(&self, key: &str, src: &Path, name: &str, now: i64) -> io::Result<ModInfo> {
        let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "mod".into());
        let base = sanitize(if name.trim().is_empty() { &stem } else { name.trim() });
        let mut id = base.to_lowercase().replace(' ', "-");
        let mut n = 2;
        while self.mod_dir(key, &id).exists() {
            id = format!("{}-{n}", base.to_lowercase().replace(' ', "-"));
            n += 1;
        }
        let dir = self.mod_dir(key, &id);
        let files_dir = dir.join("files");
        std::fs::create_dir_all(&files_dir)?;
        let result = if src.is_dir() {
            archive::copy_dir(src, &files_dir).map(|_| ())
        } else if src.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            archive::extract_prefix(src, "", &files_dir).map(|_| ())
        } else {
            Err(io::Error::other(tr!("format non pris en charge : utilisez une archive .zip ou un dossier")))
        };
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        // Archive contenant un unique dossier racine : on le « déballe ».
        flatten_single_root(&files_dir)?;
        let files = walk_files(&files_dir, 100_000)?;
        let size = files.iter().filter_map(|f| files_dir.join(f).metadata().ok()).map(|m| m.len()).sum();
        let info = ModInfo {
            id,
            name: base,
            version: String::new(),
            source: src.to_string_lossy().into_owned(),
            nexus_id: None,
            latest_version: None,
            added_at: now,
            files: files.len(),
            size,
        };
        self.save_info(key, &info)?;
        Ok(info)
    }

    /// Supprime un mod de la bibliothèque (refusé s'il est déployé).
    pub fn remove(&self, key: &str, id: &str) -> io::Result<()> {
        if self.deployment(key).files.iter().any(|f| f.mod_id == id) {
            return Err(io::Error::other(tr!("ce mod est actuellement installé dans le jeu : retirez les mods d'abord")));
        }
        if id.contains(['/', '\\']) || id.contains("..") {
            return Err(io::Error::other(tr!("identifiant invalide")));
        }
        std::fs::remove_dir_all(self.mod_dir(key, id))
    }

    pub fn deployment(&self, key: &str) -> Deployment {
        std::fs::read(self.game(key).join("deployed.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save_deployment(&self, key: &str, d: &Deployment) -> io::Result<()> {
        let p = self.game(key).join("deployed.json");
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(d).map_err(io::Error::other)?)?;
        std::fs::rename(tmp, p)
    }

    /// Fichiers à déployer pour une liste ordonnée de mods activés.
    pub fn plan(&self, key: &str, order: &[String], adapter: &Adapter) -> io::Result<Plan> {
        let infos: BTreeMap<String, ModInfo> = self.list(key).into_iter().map(|m| (m.id.clone(), m)).collect();
        let mut by_target: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
        let mut ignored = 0;
        for id in order {
            let Some(info) = infos.get(id) else { continue };
            let files_dir = self.mod_dir(key, id).join("files");
            for rel in walk_files(&files_dir, 100_000)? {
                let rel_s = entry_name("", &rel);
                match adapter.map(&info.name, &rel_s).and_then(|t| safe_relative(&t).map(|_| t)) {
                    Some(target) => by_target.entry(target).or_default().push((id.clone(), files_dir.join(&rel))),
                    None => ignored += 1,
                }
            }
        }
        let mut plan = Plan { ignored, ..Default::default() };
        for (target, providers) in by_target {
            if providers.len() > 1 {
                plan.conflicts.push(Conflict { rel: target.clone(), mods: providers.iter().map(|p| p.0.clone()).collect() });
            }
            let (id, src) = providers.into_iter().last().expect("au moins un fournisseur");
            plan.files.push((target, id, src));
        }
        Ok(plan)
    }

    /// Retire les mods déployés : supprime nos fichiers intacts et remet les originaux.
    pub fn undeploy(&self, key: &str, install_dir: &Path) -> io::Result<UndeployReport> {
        let d = self.deployment(key);
        let backup = self.game(key).join("backup");
        let mut report = UndeployReport::default();
        for f in &d.files {
            let Some(rel) = safe_relative(&f.rel) else { continue };
            let target = install_dir.join(&rel);
            if target.exists() {
                if sha256_file(&target)? == f.hash {
                    std::fs::remove_file(&target)?;
                    report.removed += 1;
                } else {
                    report.kept_modified.push(f.rel.clone());
                    continue;
                }
            }
            if f.backed_up {
                let saved = backup.join(&rel);
                if saved.exists() {
                    std::fs::rename(&saved, &target).or_else(|_| std::fs::copy(&saved, &target).map(|_| ()))?;
                    report.restored += 1;
                }
            }
        }
        let mut dirs = d.created_dirs.clone();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
        for dir in dirs {
            if let Some(rel) = safe_relative(&dir) {
                let _ = std::fs::remove_dir(install_dir.join(rel));
            }
        }
        // Les fichiers modifiés restent suivis : ils ne sont pas oubliés.
        let remaining: Vec<DeployedFile> =
            d.files.into_iter().filter(|f| report.kept_modified.contains(&f.rel)).collect();
        if remaining.is_empty() {
            let _ = std::fs::remove_file(self.game(key).join("deployed.json"));
        } else {
            self.save_deployment(key, &Deployment { files: remaining, ..Default::default() })?;
        }
        Ok(report)
    }

    /// Déploie un plan (les mods déjà déployés sont retirés d'abord).
    pub fn deploy(&self, key: &str, install_dir: &Path, plan: &Plan, profile: Option<i64>, now: i64) -> io::Result<Deployment> {
        if !install_dir.is_dir() {
            return Err(io::Error::other(tr!("dossier du jeu introuvable")));
        }
        let report = self.undeploy(key, install_dir)?;
        if !report.kept_modified.is_empty() {
            return Err(io::Error::other(trn!(report.kept_modified.len(), "{n} fichier de mod a été modifié depuis son installation ; vérifiez-le avant de redéployer", "{n} fichiers de mods ont été modifiés depuis leur installation ; vérifiez-les avant de redéployer")));
        }
        let backup = self.game(key).join("backup");
        let mut d = Deployment { profile, at: now, ..Default::default() };
        for (target, mod_id, src) in &plan.files {
            let rel = safe_relative(target).ok_or_else(|| io::Error::other(tr!("chemin refusé : {target}", target = target)))?;
            let full = install_dir.join(&rel);
            // Dossiers créés (retirés s'ils redeviennent vides).
            let mut cur = install_dir.to_path_buf();
            for comp in rel.parent().map(|p| p.components().collect::<Vec<_>>()).unwrap_or_default() {
                cur.push(comp);
                if !cur.exists() {
                    std::fs::create_dir(&cur)?;
                    if let Ok(r) = cur.strip_prefix(install_dir) {
                        d.created_dirs.push(entry_name("", r));
                    }
                }
            }
            let mut backed_up = false;
            if full.exists() {
                let saved = backup.join(&rel);
                if let Some(p) = saved.parent() {
                    std::fs::create_dir_all(p)?;
                }
                if !saved.exists() {
                    std::fs::copy(&full, &saved)?;
                }
                backed_up = true;
            }
            std::fs::copy(src, &full)?;
            d.files.push(DeployedFile { rel: target.clone(), mod_id: mod_id.clone(), hash: sha256_file(&full)?, backed_up });
            // Enregistrement au fil de l'eau : un arrêt brutal reste réversible.
            if d.files.len().is_multiple_of(50) {
                self.save_deployment(key, &d)?;
            }
        }
        self.save_deployment(key, &d)?;
        Ok(d)
    }

    /// Fichiers présents dans les dossiers de mods du jeu sans avoir été installés par Halcyon.
    pub fn external(&self, key: &str, install_dir: &Path, adapter: &Adapter) -> Vec<String> {
        let ours: std::collections::HashSet<String> = self.deployment(key).files.into_iter().map(|f| f.rel).collect();
        let mut out = Vec::new();
        for dir in adapter.watched_dirs() {
            let Some(rel) = safe_relative(&dir) else { continue };
            let base = install_dir.join(&rel);
            for f in walk_files(&base, 5000).unwrap_or_default() {
                let r = format!("{dir}/{}", entry_name("", &f));
                if !ours.contains(&r) {
                    out.push(r);
                }
            }
        }
        out
    }
}

/// Si `dir` ne contient qu'un dossier, son contenu remonte d'un niveau.
fn flatten_single_root(dir: &Path) -> io::Result<()> {
    let entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    if entries.len() == 1 && entries[0].file_type()?.is_dir() {
        let inner = entries[0].path();
        // Un dossier « BepInEx » ou « plugins » est une structure significative : on le garde.
        let name = entries[0].file_name().to_string_lossy().to_lowercase();
        if matches!(name.as_str(), "bepinex" | "plugins" | "config" | "data" | "content") {
            return Ok(());
        }
        let tmp = dir.with_extension("flatten");
        std::fs::rename(&inner, &tmp)?;
        std::fs::remove_dir(dir)?;
        std::fs::rename(&tmp, dir)?;
    }
    Ok(())
}

/// Version publiée d'un mod sur Nexus Mods (clé d'API personnelle, facultative).
pub fn nexus_latest(dl: &crate::artwork::Downloader, api_key: &str, domain: &str, mod_id: u64) -> Result<String, String> {
    let url = format!("https://api.nexusmods.com/v1/games/{domain}/mods/{mod_id}.json");
    let v = dl.get_json_with(&url, &[("apikey", api_key), ("accept", "application/json")]).map_err(|e| e.to_string())?;
    v.get("version").and_then(|x| x.as_str()).map(str::to_string).ok_or_else(|| tr!("réponse inattendue de Nexus Mods"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn adapters_map_files() {
        let b = Adapter::BepInEx;
        assert_eq!(b.map("Better UI", "BetterUI.dll").unwrap(), "BepInEx/plugins/Better UI/BetterUI.dll");
        assert_eq!(b.map("X", "BepInEx/config/x.cfg").unwrap(), "BepInEx/config/x.cfg");
        assert_eq!(b.map("X", "plugins/x.dll").unwrap(), "BepInEx/plugins/x.dll");
        let u = Adapter::UnrealPaks { paks: "Game/Content/Paks".into() };
        assert_eq!(u.map("m", "sub/Mod_P.pak").unwrap(), "Game/Content/Paks/~mods/Mod_P.pak");
        assert!(u.map("m", "readme.txt").is_none());
        let g = Adapter::Generic { target: "data".into() };
        assert_eq!(g.map("m", "a/b.txt").unwrap(), "data/a/b.txt");
    }

    #[test]
    fn deploy_conflicts_and_clean_undeploy() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        fs::create_dir_all(game.join("data")).unwrap();
        fs::write(game.join("data/textures.bin"), b"ORIGINAL").unwrap();
        fs::write(game.join("game.exe"), b"exe").unwrap();
        let src_a = tmp.path().join("ModA");
        fs::create_dir_all(src_a.join("ModA/data")).unwrap();
        fs::write(src_a.join("ModA/data/textures.bin"), b"A").unwrap();
        fs::write(src_a.join("ModA/data/new.txt"), b"A-new").unwrap();
        let src_b = tmp.path().join("ModB");
        fs::create_dir_all(src_b.join("data/extra")).unwrap();
        fs::write(src_b.join("data/textures.bin"), b"B").unwrap();
        fs::write(src_b.join("data/extra/b.txt"), b"B-only").unwrap();

        let store = ModStore::new(tmp.path().join("mods"));
        let a = store.import("pc-g", &src_a, "Mod A", 1).unwrap();
        assert_eq!(a.files, 2, "dossier racine unique déballé");
        let b = store.import("pc-g", &src_b, "", 2).unwrap();
        assert_eq!(store.list("pc-g").len(), 2);

        let adapter = Adapter::Generic { target: String::new() };
        let plan = store.plan("pc-g", &[a.id.clone(), b.id.clone()], &adapter).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].rel, "data/textures.bin");
        store.deploy("pc-g", &game, &plan, None, 3).unwrap();
        assert_eq!(fs::read(game.join("data/textures.bin")).unwrap(), b"B", "le dernier mod l'emporte");
        assert!(game.join("data/extra/b.txt").exists());
        assert!(store.remove("pc-g", &a.id).is_err(), "suppression refusée tant que déployé");

        // Nouvel ordre : redéploiement propre.
        let plan = store.plan("pc-g", &[b.id.clone(), a.id.clone()], &adapter).unwrap();
        store.deploy("pc-g", &game, &plan, Some(7), 4).unwrap();
        assert_eq!(fs::read(game.join("data/textures.bin")).unwrap(), b"A");
        assert_eq!(store.deployment("pc-g").profile, Some(7));

        let r = store.undeploy("pc-g", &game).unwrap();
        assert_eq!(r.restored, 1);
        assert!(r.kept_modified.is_empty());
        assert_eq!(fs::read(game.join("data/textures.bin")).unwrap(), b"ORIGINAL", "original rétabli");
        assert!(!game.join("data/new.txt").exists());
        assert!(!game.join("data/extra").exists(), "dossier créé retiré");
        assert!(game.join("game.exe").exists());

        // Un fichier modifié après déploiement n'est jamais supprimé.
        let plan = store.plan("pc-g", std::slice::from_ref(&a.id), &adapter).unwrap();
        store.deploy("pc-g", &game, &plan, None, 5).unwrap();
        fs::write(game.join("data/new.txt"), b"modifie par le joueur").unwrap();
        let r = store.undeploy("pc-g", &game).unwrap();
        assert_eq!(r.kept_modified, ["data/new.txt"]);
        assert!(game.join("data/new.txt").exists());
        assert_eq!(fs::read(game.join("data/textures.bin")).unwrap(), b"ORIGINAL");
    }

    #[test]
    fn suggests_adapters_and_lists_external_mods() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("u");
        fs::create_dir_all(g.join("BepInEx/plugins")).unwrap();
        fs::write(g.join("BepInEx/plugins/Other.dll"), b"x").unwrap();
        assert_eq!(suggest(&g).0, Adapter::BepInEx);
        let store = ModStore::new(tmp.path().join("mods"));
        assert_eq!(store.external("k", &g, &Adapter::BepInEx), ["BepInEx/plugins/Other.dll"]);
        let ue = tmp.path().join("ue");
        fs::create_dir_all(ue.join("Proj/Content/Paks")).unwrap();
        assert_eq!(suggest(&ue).0, Adapter::UnrealPaks { paks: "Proj/Content/Paks".into() });
    }
}
