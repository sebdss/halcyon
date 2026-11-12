//! Installation et vérification d'une version : fichier de version, client, bibliothèques,
//! natives, assets, configuration des journaux. Les fichiers partagés (bibliothèques, assets)
//! ne sont téléchargés qu'une fois pour toutes les versions.

use crate::http::{self, Error, Http, Result};
use crate::meta::{AssetIndex, Env, Manifest, Version, RESOURCES, VERSION_MANIFEST};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Arborescence de Minecraft dans Halcyon (indépendante du `.minecraft` officiel).
#[derive(Clone, Debug)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn natives(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }
    pub fn profiles(&self) -> PathBuf {
        self.root.join("profiles")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
}

/// Progression réelle d'une opération.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Étape en cours (libellé).
    Step(String),
    /// Octets reçus / total connu, fichiers terminés / total.
    Progress { bytes: u64, total_bytes: u64, files: usize, total_files: usize },
    /// Ligne d'information (installateur Forge…).
    Log(String),
}

pub type Sink<'a> = &'a (dyn Fn(Event) + Sync);

/// Fichier à télécharger.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Job {
    /// Déjà présent et complet (taille, et empreinte si `check_hash`).
    pub fn satisfied(&self, check_hash: bool) -> bool {
        let Ok(meta) = std::fs::metadata(&self.dest) else { return false };
        if !meta.is_file() || self.size.is_some_and(|s| s != meta.len()) {
            return false;
        }
        match (&self.sha1, check_hash) {
            (Some(want), true) => http::sha1_file(&self.dest).is_ok_and(|h| h.eq_ignore_ascii_case(want)),
            _ => true,
        }
    }
}

/// Télécharge les fichiers manquants en parallèle (8 connexions), avec progression réelle.
pub fn fetch_all(http: &Http, jobs: Vec<Job>, check_hash: bool, cancel: &AtomicBool, sink: Sink) -> Result<()> {
    let todo: Vec<Job> = jobs.into_iter().filter(|j| !j.satisfied(check_hash)).collect();
    let total_files = todo.len();
    let total_bytes: u64 = todo.iter().filter_map(|j| j.size).sum();
    if todo.is_empty() {
        sink(Event::Progress { bytes: 0, total_bytes: 0, files: 0, total_files: 0 });
        return Ok(());
    }
    let bytes = AtomicU64::new(0);
    let files = AtomicUsize::new(0);
    let queue = Mutex::new(todo);
    let first_error: Mutex<Option<Error>> = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let last_report = Mutex::new(std::time::Instant::now());
    let report = |force: bool| {
        let mut last = last_report.lock().unwrap_or_else(|e| e.into_inner());
        if force || last.elapsed() > std::time::Duration::from_millis(100) {
            *last = std::time::Instant::now();
            sink(Event::Progress { bytes: bytes.load(Ordering::Relaxed), total_bytes, files: files.load(Ordering::Relaxed), total_files });
        }
    };
    std::thread::scope(|s| {
        for _ in 0..8.min(total_files) {
            s.spawn(|| loop {
                if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                    return;
                }
                let Some(job) = queue.lock().unwrap_or_else(|e| e.into_inner()).pop() else { return };
                let mut attempt = 0;
                loop {
                    let mut got = 0u64;
                    let r = http.download(&job.url, &job.dest, job.sha1.as_deref(), cancel, &mut |n| {
                        got += n;
                        bytes.fetch_add(n, Ordering::Relaxed);
                        report(false);
                    });
                    match r {
                        Ok(()) => break,
                        // Coupure passagère : deux nouvelles tentatives.
                        Err(Error::Network(_)) if attempt < 2 => {
                            bytes.fetch_sub(got, Ordering::Relaxed);
                            attempt += 1;
                            std::thread::sleep(std::time::Duration::from_millis(800 * attempt));
                        }
                        Err(e) => {
                            stop.store(true, Ordering::Relaxed);
                            let mut fe = first_error.lock().unwrap_or_else(|e| e.into_inner());
                            if fe.is_none() {
                                *fe = Some(match e {
                                    Error::Http(code, _) => Error::Http(code, job.url.to_string()),
                                    other => other,
                                });
                            }
                            return;
                        }
                    }
                }
                files.fetch_add(1, Ordering::Relaxed);
                report(false);
            });
        }
    });
    report(true);
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    match first_error.into_inner().unwrap_or(None) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------- versions

/// Liste officielle des versions (mise en cache pour l'usage hors ligne).
pub fn manifest(http: &Http, layout: &Layout, online: bool) -> Result<Manifest> {
    let cache = layout.cache().join("version_manifest_v2.json");
    let value = if online {
        match http.get_json(VERSION_MANIFEST, &[]) {
            Ok(v) => {
                let _ = std::fs::create_dir_all(layout.cache());
                let _ = std::fs::write(&cache, v.to_string());
                v
            }
            Err(e) => read_json(&cache).map_err(|_| e)?,
        }
    } else {
        read_json(&cache).map_err(|_| Error::Network("hors ligne et liste des versions jamais téléchargée".into()))?
    };
    serde_json::from_value(value).map_err(|e| Error::Invalid(format!("liste des versions illisible : {e}")))
}

pub fn read_json(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Disk(format!("{} : {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("{} : {e}", path.display())))
}

fn write_json(path: &Path, v: &serde_json::Value) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::Disk(e.to_string()))?;
    }
    let tmp = path.with_extension("json.part");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default()).map_err(|e| Error::Disk(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::Disk(e.to_string()))
}

/// Fichier de version : local s'il existe, sinon téléchargé depuis la liste officielle.
pub fn version_json(http: &Http, layout: &Layout, id: &str, manifest: Option<&Manifest>) -> Result<serde_json::Value> {
    let path = layout.version_json(id);
    if path.is_file() {
        return read_json(&path);
    }
    let m = manifest.ok_or_else(|| Error::Invalid(format!("version {id} absente : liste des versions nécessaire")))?;
    let entry = m.get(id).ok_or_else(|| Error::Invalid(format!("version inconnue : {id}")))?;
    let v = http.get_json(&entry.url, &[])?;
    write_json(&path, &v)?;
    Ok(v)
}

/// Fichier de version complet, parents fusionnés (Fabric, Forge…).
pub fn resolve(http: &Http, layout: &Layout, id: &str, manifest: Option<&Manifest>) -> Result<Version> {
    let mut chain = Vec::new();
    let mut current = id.to_string();
    for _ in 0..5 {
        let v = Version::parse(&version_json(http, layout, &current, manifest)?).map_err(Error::Invalid)?;
        let parent = v.inherits_from.clone();
        chain.push(v);
        match parent {
            Some(p) => current = p,
            None => break,
        }
    }
    let mut merged = chain.pop().ok_or_else(|| Error::Invalid("version vide".into()))?;
    while let Some(child) = chain.pop() {
        merged = Version::merge(child, merged);
    }
    Ok(merged)
}

/// Résolution sans réseau (lancement hors ligne, vérification).
pub fn resolve_local(layout: &Layout, id: &str) -> Result<Version> {
    resolve(&Http::new(), layout, id, None)
}

/// Version dont le `.jar` sert de client.
pub fn client_jar_id(v: &Version) -> String {
    v.jar.clone().unwrap_or_else(|| v.id.clone())
}

/// Tous les fichiers d'une version (index des assets compris, téléchargé si absent).
pub fn jobs(http: &Http, layout: &Layout, v: &Version, env: &Env) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    let jar_id = client_jar_id(v);
    if let Some(c) = &v.client {
        jobs.push(Job { url: c.url.clone(), dest: layout.version_jar(&jar_id), sha1: c.sha1.clone(), size: c.size });
    }
    for f in v.library_files(env) {
        if f.url.is_empty() {
            // Bibliothèque produite localement (installateur Forge) : doit déjà exister.
            continue;
        }
        jobs.push(Job { url: f.url, dest: layout.libraries().join(&f.path), sha1: f.sha1, size: f.size });
    }
    if let Some(l) = &v.logging {
        jobs.push(Job {
            url: l.file.url.clone(),
            dest: layout.assets().join("log_configs").join(&l.file.id),
            sha1: l.file.sha1.clone(),
            size: None,
        });
    }
    if let Some(ai) = &v.asset_index {
        let index_path = layout.assets().join("indexes").join(format!("{}.json", ai.id));
        let index_job = Job { url: ai.url.clone(), dest: index_path.clone(), sha1: ai.sha1.clone(), size: ai.size };
        if !index_job.satisfied(true) {
            http.download(&ai.url, &index_path, ai.sha1.as_deref(), &AtomicBool::new(false), &mut |_| {})?;
        }
        let index = AssetIndex::parse(&read_json(&index_path)?);
        for (_, hash, size) in &index.objects {
            jobs.push(Job {
                url: format!("{RESOURCES}/{}/{hash}", &hash[..2]),
                dest: layout.assets().join("objects").join(&hash[..2]).join(hash),
                sha1: Some(hash.clone()),
                size: Some(*size),
            });
        }
    }
    // Une même destination n'est téléchargée qu'une fois (assets en double dans l'index).
    jobs.sort_by(|a, b| a.dest.cmp(&b.dest));
    jobs.dedup_by(|a, b| a.dest == b.dest);
    Ok(jobs)
}

/// Installe (ou complète) une version et ses parents. `check_hash` : relit chaque fichier
/// présent (vérification complète) au lieu de se fier à sa taille.
pub fn install(http: &Http, layout: &Layout, id: &str, manifest: Option<&Manifest>, check_hash: bool, cancel: &AtomicBool, sink: Sink) -> Result<Version> {
    sink(Event::Step(format!("Lecture de la version {id}")));
    let v = resolve(http, layout, id, manifest)?;
    let env = Env::current();
    sink(Event::Step("Préparation de la liste des fichiers".into()));
    let jobs = jobs(http, layout, &v, &env)?;
    sink(Event::Step(format!("Téléchargement des fichiers ({})", jobs.len())));
    fetch_all(http, jobs, check_hash, cancel, sink)?;
    sink(Event::Step("Assets".into()));
    prepare_legacy_assets(layout, &v, None)?;
    Ok(v)
}

/// Fichiers manquants ou endommagés d'une version installée (sans réseau).
pub fn verify(layout: &Layout, id: &str) -> Result<Vec<PathBuf>> {
    let v = resolve_local(layout, id)?;
    let env = Env::current();
    let mut bad = Vec::new();
    let jar_id = client_jar_id(&v);
    if let Some(c) = &v.client {
        let j = Job { url: c.url.clone(), dest: layout.version_jar(&jar_id), sha1: c.sha1.clone(), size: c.size };
        if !j.satisfied(true) {
            bad.push(j.dest);
        }
    } else if !layout.version_jar(&jar_id).is_file() {
        bad.push(layout.version_jar(&jar_id));
    }
    for f in v.library_files(&env) {
        let j = Job { url: f.url, dest: layout.libraries().join(&f.path), sha1: f.sha1, size: f.size };
        if !j.satisfied(true) {
            bad.push(j.dest);
        }
    }
    if let Some(ai) = &v.asset_index {
        let index_path = layout.assets().join("indexes").join(format!("{}.json", ai.id));
        match read_json(&index_path) {
            Ok(index) => {
                for (_, hash, size) in AssetIndex::parse(&index).objects {
                    let p = layout.assets().join("objects").join(&hash[..2]).join(&hash);
                    if std::fs::metadata(&p).map(|m| m.len()).ok() != Some(size) {
                        bad.push(p);
                    }
                }
            }
            Err(_) => bad.push(index_path),
        }
    }
    Ok(bad)
}

/// Copie des assets pour les anciennes versions (`virtual`, `map_to_resources`).
pub fn prepare_legacy_assets(layout: &Layout, v: &Version, game_dir: Option<&Path>) -> Result<()> {
    let Some(ai) = &v.asset_index else { return Ok(()) };
    let index_path = layout.assets().join("indexes").join(format!("{}.json", ai.id));
    let Ok(raw) = read_json(&index_path) else { return Ok(()) };
    let index = AssetIndex::parse(&raw);
    if !index.virtual_ && !index.map_to_resources {
        return Ok(());
    }
    let target = if index.map_to_resources {
        match game_dir {
            Some(g) => g.join("resources"),
            None => return Ok(()),
        }
    } else {
        layout.assets().join("virtual").join(&ai.id)
    };
    for (name, hash, _) in &index.objects {
        let dest = target.join(name);
        if dest.is_file() {
            continue;
        }
        let src = layout.assets().join("objects").join(&hash[..2]).join(hash);
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| Error::Disk(e.to_string()))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| Error::Disk(format!("{} : {e}", dest.display())))?;
    }
    Ok(())
}

/// Extrait les natives (DLL, .so) dans un dossier propre à la version.
pub fn extract_natives(layout: &Layout, v: &Version) -> Result<PathBuf> {
    let dir = layout.natives(&v.id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| Error::Disk(e.to_string()))?;
    let env = Env::current();
    for f in v.library_files(&env).into_iter().filter(|f| f.natives) {
        let jar = layout.libraries().join(&f.path);
        let file = std::fs::File::open(&jar).map_err(|e| Error::Disk(format!("{} : {e}", jar.display())))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Invalid(format!("{} : {e}", jar.display())))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| Error::Invalid(e.to_string()))?;
            let Ok(name) = entry.name().map(|n| n.to_string()) else { continue };
            if entry.is_dir() || name.starts_with("META-INF/") || f.exclude.iter().any(|x| name.starts_with(x.as_str())) {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let wanted = [".dll", ".so", ".dylib", ".jnilib"].iter().any(|e| lower.ends_with(e)) || lower.contains(".so.");
            if !wanted {
                continue;
            }
            // Nom de fichier seul : jamais de chemin relatif sortant du dossier.
            let Some(file_name) = Path::new(&name).file_name() else { continue };
            let dest = dir.join(file_name);
            let mut out = std::fs::File::create(&dest).map_err(|e| Error::Disk(e.to_string()))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| Error::Disk(e.to_string()))?;
        }
    }
    Ok(dir)
}

/// Versions installées (dossiers de `versions/` contenant un fichier de version).
pub fn installed(layout: &Layout) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(layout.versions())
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|id| layout.version_json(id).is_file()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Supprime une version (son dossier) ; bibliothèques et assets partagés sont conservés.
/// Refuse si une autre version installée en dépend.
pub fn remove(layout: &Layout, id: &str) -> Result<()> {
    if id.is_empty() || id.contains(['/', '\\']) || id == "." || id == ".." {
        return Err(Error::Invalid(format!("identifiant refusé : {id}")));
    }
    for other in installed(layout) {
        if other != id {
            if let Ok(v) = read_json(&layout.version_json(&other)) {
                if v.get("inheritsFrom").and_then(|p| p.as_str()) == Some(id) {
                    return Err(Error::Invalid(format!("{other} dépend de {id} : supprimez-le d'abord")));
                }
            }
        }
    }
    let dir = layout.version_dir(id);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|e| Error::Disk(format!("{} : {e}", dir.display())))?;
    }
    Ok(())
}

/// Taille des fichiers (affichage).
pub fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_resolution_verification_and_removal() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path());
        let parent = json!({"id":"1.20.1","mainClass":"M","downloads":{"client":{"sha1":"a9993e364706816aba3e25717850c26c9cd0d89d","size":3,"url":"https://x/c.jar"}},
            "libraries":[{"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar","sha1":"a9993e364706816aba3e25717850c26c9cd0d89d","size":3,"url":"https://x/b.jar"}}}]});
        let child = json!({"id":"fabric-1.20.1","inheritsFrom":"1.20.1","mainClass":"K","libraries":[]});
        write_json(&l.version_json("1.20.1"), &parent).unwrap();
        write_json(&l.version_json("fabric-1.20.1"), &child).unwrap();
        let v = resolve_local(&l, "fabric-1.20.1").unwrap();
        assert_eq!(v.main_class, "K");
        assert_eq!(client_jar_id(&v), "1.20.1");
        assert_eq!(verify(&l, "fabric-1.20.1").unwrap().len(), 2, "client et bibliothèque manquants");
        std::fs::write(l.version_jar("1.20.1"), b"abc").unwrap();
        std::fs::create_dir_all(l.libraries().join("a/b/1")).unwrap();
        std::fs::write(l.libraries().join("a/b/1/b-1.jar"), b"abd").unwrap();
        assert_eq!(verify(&l, "fabric-1.20.1").unwrap(), vec![l.libraries().join("a/b/1/b-1.jar")], "fichier altéré détecté");
        assert_eq!(installed(&l), vec!["1.20.1".to_string(), "fabric-1.20.1".to_string()]);
        assert!(remove(&l, "1.20.1").is_err(), "parent d'une autre version");
        remove(&l, "fabric-1.20.1").unwrap();
        remove(&l, "1.20.1").unwrap();
        assert!(installed(&l).is_empty());
        assert!(remove(&l, "../x").is_err());
    }

    #[test]
    fn satisfied_jobs_are_skipped_without_network() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        let job = Job { url: "https://invalide.example/f".into(), dest: p, sha1: Some("a9993e364706816aba3e25717850c26c9cd0d89d".into()), size: Some(3) };
        assert!(job.satisfied(true));
        let events = Mutex::new(Vec::new());
        fetch_all(&Http::new(), vec![job], true, &AtomicBool::new(false), &|e| events.lock().unwrap().push(e)).unwrap();
        assert_eq!(events.lock().unwrap().last(), Some(&Event::Progress { bytes: 0, total_bytes: 0, files: 0, total_files: 0 }));
    }
}
