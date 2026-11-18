//! Gestion des sauvegardes de jeux : détection des emplacements, instantanés
//! versionnés, restauration sûre, comparaison et synchronisation entre PC.
//!
//! Principes de sûreté :
//! - un instantané n'est jamais modifié après sa création (écriture dans un dossier
//!   temporaire puis renommage) ;
//! - toute restauration commence par un instantané de l'état actuel, puis remplace les
//!   fichiers par échange de dossiers, avec retour arrière en cas d'erreur ;
//! - les emplacements sont stockés avec des jetons (`{documents}`, `{steam}`…) pour
//!   être résolus correctement sur chaque PC.

use crate::archive::{self, entry_name, sha256_bytes, sha256_file, walk_files};
use halcyon_core::{util, GameSource};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Plafond de sécurité : au-delà, ce n'est vraisemblablement pas un dossier de sauvegarde.
pub const MAX_FILES: usize = 20_000;
pub const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Dossiers connus de l'utilisateur, servant de jetons dans les chemins stockés.
#[derive(Clone, Debug, Default)]
pub struct SaveEnv {
    pub tokens: Vec<(&'static str, PathBuf)>,
    pub steam_root: Option<PathBuf>,
    pub steam_account: Option<u64>,
}

impl SaveEnv {
    pub fn current(steam_root: Option<PathBuf>, steam_account: Option<u64>) -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let mut tokens: Vec<(&'static str, PathBuf)> = vec![("home", home.clone())];
        if let Some(d) = dirs::document_dir() {
            tokens.push(("documents", d));
        }
        if cfg!(windows) {
            tokens.push(("savedgames", home.join("Saved Games")));
            if let Some(d) = dirs::config_dir() {
                tokens.push(("appdata", d));
            }
            if let Some(d) = dirs::data_local_dir() {
                tokens.push(("localappdata", d));
            }
            tokens.push(("locallow", home.join("AppData").join("LocalLow")));
        } else {
            if let Some(d) = dirs::config_dir() {
                tokens.push(("config", d));
            }
            if let Some(d) = dirs::data_dir() {
                tokens.push(("data", d));
            }
        }
        if let Some(s) = &steam_root {
            tokens.push(("steam", s.clone()));
        }
        Self { tokens, steam_root, steam_account }
    }

    /// Remplace le préfixe le plus spécifique par un jeton (`{documents}/My Games/X`).
    pub fn tokenize(&self, path: &Path, install_dir: Option<&Path>) -> String {
        let mut best: Option<(usize, String)> = None;
        let mut consider = |name: &str, base: &Path| {
            if let Ok(rest) = path.strip_prefix(base) {
                let len = base.as_os_str().len();
                if best.as_ref().is_none_or(|(l, _)| len > *l) && !base.as_os_str().is_empty() {
                    let rest = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                    best = Some((len, if rest.is_empty() { format!("{{{name}}}") } else { format!("{{{name}}}/{rest}") }));
                }
            }
        };
        for (name, base) in &self.tokens {
            consider(name, base);
        }
        if let Some(i) = install_dir {
            consider("install", i);
        }
        best.map(|(_, s)| s).unwrap_or_else(|| path.to_string_lossy().into_owned())
    }

    /// Résout un chemin à jetons sur ce PC.
    pub fn resolve(&self, s: &str, install_dir: Option<&Path>) -> Option<PathBuf> {
        let Some(rest) = s.strip_prefix('{') else { return Some(PathBuf::from(s)) };
        let (name, tail) = rest.split_once('}')?;
        let base = if name == "install" {
            install_dir?.to_path_buf()
        } else {
            self.tokens.iter().find(|(n, _)| *n == name)?.1.clone()
        };
        let tail = tail.trim_start_matches('/');
        Some(if tail.is_empty() { base } else { tail.split('/').fold(base, |p, part| p.join(part)) })
    }
}

/// Informations du jeu utiles à la détection.
#[derive(Clone, Debug, Default)]
pub struct GameInfo {
    pub name: String,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub install_dir: Option<PathBuf>,
    pub source: Option<GameSource>,
    pub source_key: String,
    /// ROM (jeux émulés).
    pub rom: Option<PathBuf>,
    /// Emplacements supplémentaires fournis par des extensions (jetons acceptés, `{name}` = nom du jeu).
    pub extra: Vec<String>,
}

/// Emplacement proposé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub reason: String,
    /// Correspondance exacte : associée automatiquement.
    pub confident: bool,
}

fn compact(s: &str) -> String {
    util::search_key(s).replace(' ', "")
}

fn name_keys(g: &GameInfo) -> Vec<String> {
    let mut keys = vec![compact(&g.name)];
    for sep in [":", " - ", " – "] {
        if let Some((head, _)) = g.name.split_once(sep) {
            keys.push(compact(head));
        }
    }
    if let Some(d) = g.install_dir.as_deref().and_then(|d| d.file_name()) {
        keys.push(compact(&d.to_string_lossy()));
    }
    keys.retain(|k| k.len() >= 3);
    keys.sort();
    keys.dedup();
    keys
}

/// Dossiers de `base` dont le nom correspond au jeu : (chemin, exact).
fn matching_children(base: &Path, keys: &[String]) -> Vec<(PathBuf, bool)> {
    let Ok(entries) = std::fs::read_dir(base) else { return vec![] };
    let mut out = Vec::new();
    for e in entries.flatten().take(2000) {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let c = compact(&e.file_name().to_string_lossy());
        if c.len() < 3 {
            continue;
        }
        if keys.contains(&c) {
            out.push((e.path(), true));
        } else if c.len() >= 5 && keys.iter().any(|k| k.len() >= 5 && (k.starts_with(&c) || c.starts_with(k.as_str()))) {
            out.push((e.path(), false));
        }
    }
    out
}

const SAVE_DIR_NAMES: &[&str] = &["save", "saves", "savegames", "savegame", "savedata", "userdata", "profiles"];
const ROM_SAVE_EXTS: &[&str] = &["srm", "sav", "sra", "eep", "fla", "mcr", "mcd", "dsv", "state", "state1", "state2", "state3"];

/// Emplacements de sauvegarde probables (uniquement ceux qui existent).
pub fn detect(g: &GameInfo, env: &SaveEnv) -> Vec<Candidate> {
    let keys = name_keys(g);
    let mut out: Vec<Candidate> = Vec::new();
    let mut push = |path: PathBuf, reason: String, confident: bool| {
        if path.exists() && !out.iter().any(|c| c.path == path) {
            out.push(Candidate { path, reason, confident });
        }
    };
    let devs: Vec<String> =
        [&g.developer, &g.publisher].iter().filter_map(|d| d.as_deref()).map(compact).filter(|d| d.len() >= 3).collect();

    // Bases de recherche : (dossier, libellé). Les éditeurs y ont souvent un sous-dossier.
    let mut bases: Vec<(PathBuf, String)> = Vec::new();
    let tok = |n: &str| env.tokens.iter().find(|(k, _)| *k == n).map(|(_, p)| p.clone());
    if let Some(d) = tok("documents") {
        bases.push((d.join("My Games"), "Documents/My Games".into()));
        bases.push((d.clone(), "Documents".into()));
    }
    for (t, label) in [("savedgames", "Saved Games"), ("appdata", "AppData/Roaming"), ("localappdata", "AppData/Local"), ("locallow", "AppData/LocalLow"), ("config", "~/.config"), ("data", "~/.local/share")] {
        if let Some(d) = tok(t) {
            bases.push((d, label.into()));
        }
    }
    if let Some(d) = tok("config") {
        bases.push((d.join("unity3d"), "~/.config/unity3d".into()));
    }

    // Steam : copie locale de Steam Cloud et préfixe Proton.
    if g.source == Some(GameSource::Steam) {
        if let (Some(root), Some(acct)) = (&env.steam_root, env.steam_account) {
            let remote = root.join("userdata").join(acct.to_string()).join(&g.source_key).join("remote");
            push(remote, tr!("Steam Cloud (copie locale, synchronisée par Steam)"), true);
        }
        let mut prefixes: Vec<PathBuf> = Vec::new();
        if let Some(root) = &env.steam_root {
            prefixes.push(root.join("steamapps/compatdata").join(&g.source_key));
        }
        if let Some(lib) = g.install_dir.as_deref().and_then(|d| d.parent()).and_then(|d| d.parent()) {
            prefixes.push(lib.join("compatdata").join(&g.source_key));
        }
        for p in prefixes {
            let user = p.join("pfx/drive_c/users/steamuser");
            for (rel, label) in [
                ("Documents/My Games", "Proton · Documents/My Games"),
                ("Documents", "Proton · Documents"),
                ("Saved Games", "Proton · Saved Games"),
                ("AppData/Roaming", "Proton · AppData/Roaming"),
                ("AppData/Local", "Proton · AppData/Local"),
                ("AppData/LocalLow", "Proton · AppData/LocalLow"),
            ] {
                bases.push((user.join(rel), label.into()));
            }
        }
    }

    for (base, label) in &bases {
        for (p, exact) in matching_children(base, &keys) {
            push(p.clone(), format!("{label}/{}", p.file_name().unwrap_or_default().to_string_lossy()), exact);
        }
        // Base/<éditeur>/<jeu> (Unity : LocalLow/<Studio>/<Jeu>).
        for (dev_dir, _) in matching_children(base, &devs) {
            for (p, exact) in matching_children(&dev_dir, &keys) {
                push(p, format!("{label}/{}/…", dev_dir.file_name().unwrap_or_default().to_string_lossy()), exact);
            }
        }
    }

    // Dossier du jeu : Saves/, Saved/SaveGames (Unreal).
    if let Some(dir) = &g.install_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten().take(500) {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let n = e.file_name().to_string_lossy().to_lowercase();
                if SAVE_DIR_NAMES.contains(&n.as_str()) {
                    push(p.clone(), tr!("Dossier du jeu/{file_name}", file_name = e.file_name().to_string_lossy()), true);
                }
                let unreal = p.join("Saved").join("SaveGames");
                if unreal.is_dir() {
                    push(unreal, format!("Unreal · {}/Saved/SaveGames", e.file_name().to_string_lossy()), true);
                }
            }
        }
    }

    // ROM : sauvegardes à côté du fichier et dans RetroArch.
    if let Some(rom) = &g.rom {
        let stem = rom.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut dirs_to_check: Vec<PathBuf> = rom.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
        if let Some(c) = dirs::config_dir() {
            dirs_to_check.push(c.join("retroarch/saves"));
            dirs_to_check.push(c.join("retroarch/states"));
            dirs_to_check.push(c.join("RetroArch/saves"));
            dirs_to_check.push(c.join("RetroArch/states"));
        }
        for d in dirs_to_check {
            for ext in ROM_SAVE_EXTS {
                push(d.join(format!("{stem}.{ext}")), tr!("Sauvegarde d'émulateur ({ext})", ext = ext), true);
            }
        }
    }

    // Emplacements fournis par les extensions.
    for pattern in &g.extra {
        let p = pattern.replace("{name}", &g.name);
        if let Some(path) = env.resolve(&p, g.install_dir.as_deref()) {
            push(path, tr!("Définition d'extension"), true);
        }
    }
    out
}

// ---------------------------------------------------------------- instantanés

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub loc: usize,
    pub rel: String,
    pub size: u64,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub game_key: String,
    pub game_name: String,
    pub created_at: i64,
    /// `manual`, `before_session`, `after_session`, `before_restore`.
    pub reason: String,
    pub machine: String,
    #[serde(default)]
    pub note: String,
    /// Emplacements (jetons) ; `files[].loc` y fait référence.
    pub locations: Vec<String>,
    /// Emplacement de type fichier (et non dossier).
    #[serde(default)]
    pub file_locations: Vec<bool>,
    pub files: Vec<FileEntry>,
    pub total_size: u64,
    pub content_hash: String,
}

impl SnapshotMeta {
    pub fn reason_label(&self) -> String {
        match self.reason.as_str() {
            "before_session" => tr!("Avant la session"),
            "after_session" => tr!("Après la session"),
            "before_restore" => tr!("Avant restauration"),
            "imported" => tr!("Importée"),
            _ => tr!("Manuelle"),
        }
    }

    pub fn is_automatic(&self) -> bool {
        matches!(self.reason.as_str(), "before_session" | "after_session" | "before_restore")
    }
}

/// Emplacement d'un jeu : chemin stocké (jetons) et chemin réel sur ce PC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub token: String,
    pub path: PathBuf,
}

/// Empreinte globale d'un ensemble de fichiers.
pub fn content_hash(files: &[FileEntry]) -> String {
    let mut lines: Vec<String> = files.iter().map(|f| format!("{}/{}:{}", f.loc, f.rel, f.hash)).collect();
    lines.sort();
    sha256_bytes(lines.join("\n").as_bytes())
}

/// État actuel des fichiers des emplacements (empreintes calculées).
pub fn scan_locations(locations: &[Location]) -> io::Result<(Vec<FileEntry>, Vec<bool>)> {
    let mut files = Vec::new();
    let mut kinds = Vec::new();
    let mut total = 0u64;
    for (i, loc) in locations.iter().enumerate() {
        let is_file = loc.path.is_file();
        kinds.push(is_file);
        let rels: Vec<PathBuf> = if is_file {
            vec![PathBuf::from(loc.path.file_name().unwrap_or_default())]
        } else if loc.path.is_dir() {
            walk_files(&loc.path, MAX_FILES)?
        } else {
            vec![]
        };
        for rel in rels {
            let full = if is_file { loc.path.clone() } else { loc.path.join(&rel) };
            let size = full.metadata()?.len();
            total += size;
            if total > MAX_BYTES {
                return Err(io::Error::other(tr!("plus de 4 Go de données : emplacement trop volumineux pour une sauvegarde")));
            }
            files.push(FileEntry { loc: i, rel: entry_name("", &rel), size, hash: sha256_file(&full)? });
        }
    }
    Ok((files, kinds))
}

/// Différences entre deux états.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub unchanged: usize,
}

pub fn diff(old: &[FileEntry], new: &[FileEntry]) -> SaveDiff {
    use std::collections::HashMap;
    let key = |f: &FileEntry| format!("{}/{}", f.loc, f.rel);
    let o: HashMap<String, &FileEntry> = old.iter().map(|f| (key(f), f)).collect();
    let n: HashMap<String, &FileEntry> = new.iter().map(|f| (key(f), f)).collect();
    let mut d = SaveDiff::default();
    for (k, f) in &n {
        match o.get(k) {
            None => d.added.push(f.rel.clone()),
            Some(of) if of.hash != f.hash => d.modified.push(f.rel.clone()),
            Some(_) => d.unchanged += 1,
        }
    }
    for (k, f) in &o {
        if !n.contains_key(k) {
            d.removed.push(f.rel.clone());
        }
    }
    d.added.sort();
    d.removed.sort();
    d.modified.sort();
    d
}

/// Résultat d'une création.
#[derive(Debug)]
pub enum Created {
    New(SnapshotMeta),
    /// Rien n'a changé depuis le dernier instantané.
    Unchanged(SnapshotMeta),
    /// Aucun fichier dans les emplacements.
    Empty,
}

/// Dépôt d'instantanés : `<racine>/<jeu>/<instantané>/{snapshot.zip, snapshot.json}`.
#[derive(Clone, Debug)]
pub struct SaveStore {
    pub root: PathBuf,
}

fn new_id(now: i64) -> String {
    let (y, m, d) = util::civil_from_days(now.div_euclid(86_400));
    let s = now.rem_euclid(86_400);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{:04x}", s / 3600, s / 60 % 60, s % 60, nanos & 0xffff)
}

impl SaveStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn available(&self) -> bool {
        std::fs::create_dir_all(&self.root).is_ok() && self.root.is_dir()
    }

    fn game_dir(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    pub fn list(&self, key: &str) -> Vec<SnapshotMeta> {
        let Ok(entries) = std::fs::read_dir(self.game_dir(key)) else { return vec![] };
        let mut out: Vec<SnapshotMeta> = entries
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
            .filter_map(|e| std::fs::read(e.path().join("snapshot.json")).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        out
    }

    pub fn get(&self, key: &str, id: &str) -> Option<SnapshotMeta> {
        let b = std::fs::read(self.game_dir(key).join(id).join("snapshot.json")).ok()?;
        serde_json::from_slice(&b).ok()
    }

    pub fn archive_path(&self, key: &str, id: &str) -> PathBuf {
        self.game_dir(key).join(id).join("snapshot.zip")
    }

    /// Crée un instantané. `dedup` : ignore si identique au plus récent.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        key: &str,
        name: &str,
        locations: &[Location],
        reason: &str,
        machine: &str,
        note: &str,
        now: i64,
        dedup: bool,
    ) -> io::Result<Created> {
        let (files, kinds) = scan_locations(locations)?;
        if files.is_empty() {
            return Ok(Created::Empty);
        }
        let hash = content_hash(&files);
        if dedup {
            if let Some(latest) = self.list(key).into_iter().next() {
                if latest.content_hash == hash {
                    return Ok(Created::Unchanged(latest));
                }
            }
        }
        let id = new_id(now);
        let dir = self.game_dir(key);
        let tmp = dir.join(format!("{id}.tmp"));
        std::fs::create_dir_all(&tmp)?;
        let meta = SnapshotMeta {
            id: id.clone(),
            game_key: key.to_string(),
            game_name: name.to_string(),
            created_at: now,
            reason: reason.to_string(),
            machine: machine.to_string(),
            note: note.to_string(),
            locations: locations.iter().map(|l| l.token.clone()).collect(),
            file_locations: kinds.clone(),
            total_size: files.iter().map(|f| f.size).sum(),
            content_hash: hash,
            files: files.clone(),
        };
        let entries: Vec<(String, PathBuf)> = files
            .iter()
            .map(|f| {
                let loc = &locations[f.loc];
                let src = if kinds[f.loc] { loc.path.clone() } else { f.rel.split('/').fold(loc.path.clone(), |p, s| p.join(s)) };
                (format!("{}/{}", f.loc, f.rel), src)
            })
            .collect();
        let result = archive::write_zip(&tmp.join("snapshot.zip"), &entries, &[])
            .and_then(|_| std::fs::write(tmp.join("snapshot.json"), serde_json::to_vec_pretty(&meta).map_err(io::Error::other)?))
            .and_then(|_| std::fs::rename(&tmp, dir.join(&id)));
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        Ok(Created::New(meta))
    }

    /// Supprime un instantané.
    pub fn delete(&self, key: &str, id: &str) -> io::Result<()> {
        if id.contains(['/', '\\']) || id.contains("..") {
            return Err(io::Error::other(tr!("identifiant invalide")));
        }
        std::fs::remove_dir_all(self.game_dir(key).join(id))
    }

    /// Garde au plus `keep` instantanés automatiques (les manuels ne sont jamais purgés).
    pub fn prune(&self, key: &str, keep: usize) -> usize {
        if keep == 0 {
            return 0;
        }
        let autos: Vec<SnapshotMeta> = self.list(key).into_iter().filter(|s| s.is_automatic()).collect();
        let mut removed = 0;
        for s in autos.iter().skip(keep) {
            if self.delete(key, &s.id).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Copie un instantané vers un autre dépôt (disque secondaire, NAS).
    pub fn copy_to(&self, other: &SaveStore, key: &str, id: &str) -> io::Result<()> {
        let src = self.game_dir(key).join(id);
        let dest_dir = other.game_dir(key);
        if dest_dir.join(id).join("snapshot.json").is_file() {
            return Ok(());
        }
        let tmp = dest_dir.join(format!("{id}.tmp"));
        let r = archive::copy_dir(&src, &tmp).and_then(|_| {
            // Vérification de l'archive copiée avant de la publier.
            if archive::sha256_file(&src.join("snapshot.zip"))? != archive::sha256_file(&tmp.join("snapshot.zip"))? {
                return Err(io::Error::other(tr!("copie corrompue")));
            }
            std::fs::rename(&tmp, dest_dir.join(id))
        });
        if r.is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        r
    }

    /// Restaure un instantané dans les emplacements donnés (`targets[i]` = emplacement i).
    /// L'appelant doit avoir créé un instantané de l'état actuel au préalable.
    pub fn restore(&self, snap: &SnapshotMeta, targets: &[PathBuf]) -> io::Result<usize> {
        if targets.len() != snap.locations.len() {
            return Err(io::Error::other(tr!("emplacements incohérents avec l'instantané")));
        }
        let archive = self.archive_path(&snap.game_key, &snap.id);
        let staging_root = self.root.join(".restore").join(&snap.id);
        let _ = std::fs::remove_dir_all(&staging_root);
        // 1. Extraction complète et vérification des empreintes, sans toucher aux originaux.
        let mut count = 0;
        for i in 0..targets.len() {
            count += archive::extract_prefix(&archive, &format!("{i}/"), &staging_root.join(i.to_string()))?;
        }
        for f in &snap.files {
            let p = f.rel.split('/').fold(staging_root.join(f.loc.to_string()), |p, s| p.join(s));
            if sha256_file(&p)? != f.hash {
                let _ = std::fs::remove_dir_all(&staging_root);
                return Err(io::Error::other(tr!("archive corrompue : {rel}", rel = f.rel)));
            }
        }
        // 2. Remplacement emplacement par emplacement, avec retour arrière.
        for (i, target) in targets.iter().enumerate() {
            let staged = staging_root.join(i.to_string());
            let is_file = snap.file_locations.get(i).copied().unwrap_or(false);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if is_file {
                let Some(entry) = snap.files.iter().find(|f| f.loc == i) else { continue };
                let src = staged.join(&entry.rel);
                let old = target.with_extension("halcyon-old");
                if target.exists() {
                    std::fs::rename(target, &old)?;
                }
                if let Err(e) = std::fs::copy(&src, target) {
                    let _ = std::fs::rename(&old, target);
                    return Err(e);
                }
                let _ = std::fs::remove_file(&old);
            } else {
                let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "saves".into());
                let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
                let old = parent.join(format!(".{name}.halcyon-old"));
                let incoming = parent.join(format!(".{name}.halcyon-new"));
                let _ = std::fs::remove_dir_all(&incoming);
                archive::copy_dir(&staged, &incoming)?;
                let _ = std::fs::remove_dir_all(&old);
                if target.exists() {
                    std::fs::rename(target, &old)?;
                }
                if let Err(e) = std::fs::rename(&incoming, target) {
                    if old.exists() {
                        let _ = std::fs::rename(&old, target);
                    }
                    return Err(e);
                }
                let _ = std::fs::remove_dir_all(&old);
            }
        }
        let _ = std::fs::remove_dir_all(&staging_root);
        Ok(count)
    }
}

/// Situation des sauvegardes d'un jeu entre ce PC et le dépôt partagé.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncState {
    NoSnapshot,
    UpToDate,
    /// Les fichiers locaux ont changé depuis le dernier instantané connu.
    LocalChanged,
    /// Un autre PC a sauvegardé une version plus récente ; les fichiers locaux n'ont pas bougé.
    RemoteNewer(SnapshotMeta),
    /// Les deux côtés ont changé : à arbitrer par le joueur.
    Conflict(SnapshotMeta),
}

pub fn sync_state(
    latest: Option<&SnapshotMeta>,
    local_hash: Option<&str>,
    last_known_id: Option<&str>,
    last_known_hash: Option<&str>,
    machine: &str,
) -> SyncState {
    let Some(latest) = latest else { return SyncState::NoSnapshot };
    if local_hash == Some(latest.content_hash.as_str()) {
        return SyncState::UpToDate;
    }
    let known = latest.machine == machine || last_known_id == Some(latest.id.as_str());
    if known {
        return SyncState::LocalChanged;
    }
    match (local_hash, last_known_hash) {
        // Rien en local, ou local inchangé depuis notre dernière synchronisation.
        (None, _) => SyncState::RemoteNewer(latest.clone()),
        (Some(l), Some(k)) if l == k => SyncState::RemoteNewer(latest.clone()),
        _ => SyncState::Conflict(latest.clone()),
    }
}

/// Clé stable d'un jeu dans le dépôt (identique sur tous les PC).
pub fn game_key(source: GameSource, source_key: &str, name: &str, platform: Option<&str>) -> String {
    let clean = |s: &str| -> String {
        let c: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        let c = c.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
        if c.len() > 60 { format!("{:016x}", util::stable_hash(s)) } else { c }
    };
    match source {
        GameSource::Steam => format!("steam-{}", clean(source_key)),
        GameSource::Epic => format!("epic-{}", clean(source_key)),
        GameSource::Emulated => format!("rom-{}-{}", platform.unwrap_or("x"), clean(&util::search_key(name))),
        _ => format!("pc-{}", clean(&util::search_key(name))),
    }
}

/// Nom de ce PC (synchronisation multi-PC).
pub fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()))
        .or_else(|| std::env::var("HOSTNAME").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ce-pc".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tokens_round_trip() {
        let env = SaveEnv {
            tokens: vec![("home", "/home/a".into()), ("documents", "/home/a/Documents".into())],
            ..Default::default()
        };
        let t = env.tokenize(Path::new("/home/a/Documents/My Games/X"), None);
        assert_eq!(t, "{documents}/My Games/X");
        assert_eq!(env.resolve(&t, None), Some(PathBuf::from("/home/a/Documents/My Games/X")));
        let t = env.tokenize(Path::new("/games/x/Saves"), Some(Path::new("/games/x")));
        assert_eq!(t, "{install}/Saves");
        assert_eq!(env.resolve(&t, Some(Path::new("/other/x"))), Some(PathBuf::from("/other/x/Saves")));
        assert_eq!(env.resolve("{unknown}/a", None), None);
    }

    #[test]
    fn detects_common_locations() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        fs::create_dir_all(home.join("Documents/My Games/Hollow Knight")).unwrap();
        fs::create_dir_all(home.join("LocalLow/Team Cherry/Hollow Knight/save1")).unwrap();
        fs::create_dir_all(home.join("Documents/Unrelated")).unwrap();
        let install = home.join("games/HollowKnight");
        fs::create_dir_all(install.join("Saves")).unwrap();
        fs::create_dir_all(install.join("HK/Saved/SaveGames")).unwrap();
        let steam = home.join("Steam");
        fs::create_dir_all(steam.join("userdata/42/367520/remote")).unwrap();
        let env = SaveEnv {
            tokens: vec![("documents", home.join("Documents")), ("locallow", home.join("LocalLow"))],
            steam_root: Some(steam),
            steam_account: Some(42),
        };
        let g = GameInfo {
            name: "Hollow Knight".into(),
            developer: Some("Team Cherry".into()),
            install_dir: Some(install),
            source: Some(GameSource::Steam),
            source_key: "367520".into(),
            ..Default::default()
        };
        let found = detect(&g, &env);
        let reasons: Vec<&str> = found.iter().map(|c| c.reason.as_str()).collect();
        assert!(reasons.iter().any(|r| r.starts_with("Steam Cloud")), "{reasons:?}");
        assert!(reasons.contains(&"Documents/My Games/Hollow Knight"));
        assert!(reasons.iter().any(|r| r.starts_with("AppData/LocalLow/Team Cherry")));
        assert!(reasons.contains(&"Dossier du jeu/Saves"));
        assert!(reasons.iter().any(|r| r.starts_with("Unreal")));
        assert!(!reasons.iter().any(|r| r.contains("Unrelated")));
        assert!(found.iter().all(|c| c.confident));
    }

    #[test]
    fn snapshot_dedup_diff_restore_and_prune() {
        let tmp = tempfile::tempdir().unwrap();
        let saves = tmp.path().join("game/saves");
        fs::create_dir_all(saves.join("slot")).unwrap();
        fs::write(saves.join("slot/1.sav"), b"niveau 1").unwrap();
        fs::write(saves.join("options.ini"), b"vol=5").unwrap();
        let single = tmp.path().join("rom.srm");
        fs::write(&single, b"sram").unwrap();
        let locs = vec![
            Location { token: "{install}/saves".into(), path: saves.clone() },
            Location { token: "/x/rom.srm".into(), path: single.clone() },
        ];
        let store = SaveStore::new(tmp.path().join("store"));
        let Created::New(a) = store.create("pc-g", "G", &locs, "manual", "pc1", "", 1000, true).unwrap() else { panic!() };
        assert_eq!(a.files.len(), 3);
        assert!(matches!(store.create("pc-g", "G", &locs, "after_session", "pc1", "", 1001, true).unwrap(), Created::Unchanged(_)));

        fs::write(saves.join("slot/1.sav"), b"niveau 2").unwrap();
        fs::write(saves.join("slot/2.sav"), b"nouveau").unwrap();
        fs::remove_file(saves.join("options.ini")).unwrap();
        let Created::New(b) = store.create("pc-g", "G", &locs, "after_session", "pc1", "", 1002, true).unwrap() else { panic!() };
        let d = diff(&a.files, &b.files);
        assert_eq!(d.added, ["slot/2.sav"]);
        assert_eq!(d.removed, ["options.ini"]);
        assert_eq!(d.modified, ["slot/1.sav"]);
        assert_eq!(d.unchanged, 1);
        assert_eq!(store.list("pc-g").len(), 2);

        // Restauration de A : les fichiers reviennent exactement.
        fs::write(&single, b"corrompu").unwrap();
        store.restore(&a, &[saves.clone(), single.clone()]).unwrap();
        assert_eq!(fs::read(saves.join("slot/1.sav")).unwrap(), b"niveau 1");
        assert!(saves.join("options.ini").exists());
        assert!(!saves.join("slot/2.sav").exists());
        assert_eq!(fs::read(&single).unwrap(), b"sram");
        let (now, _) = scan_locations(&locs).unwrap();
        assert_eq!(content_hash(&now), a.content_hash);

        // Copie vers un second disque.
        let mirror = SaveStore::new(tmp.path().join("nas"));
        store.copy_to(&mirror, "pc-g", &b.id).unwrap();
        assert_eq!(mirror.list("pc-g").len(), 1);

        // Purge : les manuelles sont conservées.
        assert_eq!(store.prune("pc-g", 1), 0);
        let _ = store.create("pc-g", "G", &locs, "before_session", "pc1", "", 1003, false).unwrap();
        assert_eq!(store.prune("pc-g", 1), 1);
        assert!(store.list("pc-g").iter().any(|s| s.id == a.id), "manuelle conservée");
    }

    #[test]
    fn sync_states() {
        let snap = |machine: &str, hash: &str| SnapshotMeta {
            id: "s1".into(),
            game_key: "k".into(),
            game_name: "G".into(),
            created_at: 1,
            reason: "manual".into(),
            machine: machine.into(),
            note: String::new(),
            locations: vec![],
            file_locations: vec![],
            files: vec![],
            total_size: 0,
            content_hash: hash.into(),
        };
        assert_eq!(sync_state(None, Some("a"), None, None, "pc1"), SyncState::NoSnapshot);
        assert_eq!(sync_state(Some(&snap("pc2", "a")), Some("a"), None, None, "pc1"), SyncState::UpToDate);
        assert_eq!(sync_state(Some(&snap("pc1", "b")), Some("a"), None, None, "pc1"), SyncState::LocalChanged);
        assert!(matches!(sync_state(Some(&snap("pc2", "b")), Some("a"), Some("s0"), Some("a"), "pc1"), SyncState::RemoteNewer(_)));
        assert!(matches!(sync_state(Some(&snap("pc2", "b")), Some("c"), Some("s0"), Some("a"), "pc1"), SyncState::Conflict(_)));
        assert_eq!(game_key(GameSource::Steam, "620", "Portal 2", None), "steam-620");
        assert_eq!(game_key(GameSource::Manual, "", "Hadès : Édition", None), "pc-hades-edition");
    }
}
