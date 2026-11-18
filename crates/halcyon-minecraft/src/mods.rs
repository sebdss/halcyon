//! Mods et modpacks : recherche et téléchargement par les API officielles de Modrinth (sans
//! clé) et de CurseForge (clé d'API accordée par CurseForge). Un fichier qu'un auteur
//! CurseForge interdit de distribuer hors du site n'est jamais téléchargé : son lien est donné.

use crate::http::{self, Error, Http, Result};
use crate::install::{Event, Layout, Sink};
use crate::loaders::Loader;
use crate::profiles::{self, InstalledMod, Profile};
use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub const MODRINTH: &str = "https://api.modrinth.com/v2";
pub const CURSEFORGE: &str = "https://api.curseforge.com/v1";
const CF_MINECRAFT: u32 = 432;
const CF_CLASS_MODS: u32 = 6;
const CF_CLASS_MODPACKS: u32 = 4471;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Modrinth,
    CurseForge,
}

impl Source {
    pub fn id(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mod,
    Modpack,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub source: Source,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub page_url: String,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    pub url: Option<String>,
    pub filename: String,
    pub sha1: Option<String>,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dependency {
    pub project_id: String,
    pub version_id: Option<String>,
    /// `required`, `optional`, `incompatible`, `embedded`.
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectVersion {
    pub source: Source,
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub number: String,
    /// `release`, `beta`, `alpha`.
    pub channel: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub file: FileRef,
    pub dependencies: Vec<Dependency>,
    pub date: String,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn cf_loader(l: Loader) -> Option<u32> {
    match l {
        Loader::Forge => Some(1),
        Loader::Fabric => Some(4),
        Loader::Quilt => Some(5),
        Loader::NeoForge => Some(6),
        Loader::Vanilla => None,
    }
}

pub struct Client<'a> {
    pub http: &'a Http,
    /// Clé CurseForge (vide : CurseForge indisponible).
    pub curseforge_key: &'a str,
}

impl Client<'_> {
    fn cf(&self) -> Result<[(&'static str, &str); 1]> {
        if self.curseforge_key.trim().is_empty() {
            return Err(Error::Invalid(tr!("CurseForge demande une clé d'API (Minecraft › Paramètres)")));
        }
        Ok([("x-api-key", self.curseforge_key.trim())])
    }

    /// Recherche (mods ou modpacks) filtrée par version de Minecraft et loader.
    pub fn search(&self, source: Source, kind: Kind, query: &str, mc: Option<&str>, loader: Option<Loader>, offset: usize) -> Result<(Vec<Project>, u64)> {
        match source {
            Source::Modrinth => {
                let mut facets = vec![format!("[\"project_type:{}\"]", if kind == Kind::Mod { "mod" } else { "modpack" })];
                if let Some(l) = loader.filter(|l| *l != Loader::Vanilla) {
                    facets.push(format!("[\"categories:{}\"]", l.id()));
                }
                if let Some(v) = mc {
                    facets.push(format!("[\"versions:{v}\"]"));
                }
                let url = format!(
                    "{MODRINTH}/search?query={}&facets={}&limit=20&offset={offset}&index={}",
                    http::encode(query),
                    http::encode(&format!("[{}]", facets.join(","))),
                    if query.trim().is_empty() { "downloads" } else { "relevance" }
                );
                let v = self.http.get_json(&url, &[])?;
                let hits = v.get("hits").and_then(|h| h.as_array()).cloned().unwrap_or_default();
                let projects = hits
                    .iter()
                    .map(|h| Project {
                        source,
                        id: s(h, "project_id"),
                        slug: s(h, "slug"),
                        title: s(h, "title"),
                        description: s(h, "description"),
                        author: s(h, "author"),
                        icon_url: h.get("icon_url").and_then(|x| x.as_str()).filter(|u| !u.is_empty()).map(str::to_string),
                        downloads: h.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0),
                        page_url: format!("https://modrinth.com/{}/{}", if kind == Kind::Mod { "mod" } else { "modpack" }, s(h, "slug")),
                        categories: strs(h, "categories"),
                    })
                    .collect();
                Ok((projects, v.get("total_hits").and_then(|x| x.as_u64()).unwrap_or(0)))
            }
            Source::CurseForge => {
                let headers = self.cf()?;
                let mut url = format!(
                    "{CURSEFORGE}/mods/search?gameId={CF_MINECRAFT}&classId={}&searchFilter={}&sortField={}&sortOrder=desc&pageSize=20&index={offset}",
                    if kind == Kind::Mod { CF_CLASS_MODS } else { CF_CLASS_MODPACKS },
                    http::encode(query),
                    if query.trim().is_empty() { 6 } else { 2 }
                );
                if let Some(v) = mc {
                    url.push_str(&format!("&gameVersion={}", http::encode(v)));
                }
                if let Some(id) = loader.and_then(cf_loader) {
                    url.push_str(&format!("&modLoaderType={id}"));
                }
                let v = self.http.get_json(&url, &headers)?;
                let data = v.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
                let projects = data
                    .iter()
                    .map(|m| Project {
                        source,
                        id: m.get("id").and_then(|x| x.as_u64()).unwrap_or(0).to_string(),
                        slug: s(m, "slug"),
                        title: s(m, "name"),
                        description: s(m, "summary"),
                        author: m.pointer("/authors/0/name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                        icon_url: m.pointer("/logo/thumbnailUrl").and_then(|x| x.as_str()).map(str::to_string),
                        downloads: m.get("downloadCount").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64,
                        page_url: m.pointer("/links/websiteUrl").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                        categories: m.get("categories").and_then(|c| c.as_array()).map(|a| a.iter().filter_map(|c| c.get("name")?.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                    })
                    .collect();
                Ok((projects, v.pointer("/pagination/totalCount").and_then(|x| x.as_u64()).unwrap_or(0)))
            }
        }
    }

    /// Versions d'un projet compatibles, la plus récente en premier.
    pub fn versions(&self, source: Source, project: &str, mc: Option<&str>, loader: Option<Loader>) -> Result<Vec<ProjectVersion>> {
        match source {
            Source::Modrinth => {
                let mut url = format!("{MODRINTH}/project/{}/version?", http::encode(project));
                if let Some(l) = loader.filter(|l| *l != Loader::Vanilla) {
                    // Quilt charge aussi les mods Fabric.
                    let ls = if l == Loader::Quilt { "[\"quilt\",\"fabric\"]".to_string() } else { format!("[\"{}\"]", l.id()) };
                    url.push_str(&format!("loaders={}&", http::encode(&ls)));
                }
                if let Some(v) = mc {
                    url.push_str(&format!("game_versions={}", http::encode(&format!("[\"{v}\"]"))));
                }
                let v = self.http.get_json(&url, &[])?;
                Ok(v.as_array().map(|a| a.iter().filter_map(parse_modrinth_version).collect()).unwrap_or_default())
            }
            Source::CurseForge => {
                let headers = self.cf()?;
                let mut url = format!("{CURSEFORGE}/mods/{}/files?pageSize=50", http::encode(project));
                if let Some(v) = mc {
                    url.push_str(&format!("&gameVersion={}", http::encode(v)));
                }
                if let Some(id) = loader.and_then(cf_loader) {
                    url.push_str(&format!("&modLoaderType={id}"));
                }
                let v = self.http.get_json(&url, &headers)?;
                Ok(v.get("data").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|f| parse_cf_file(f, project)).collect()).unwrap_or_default())
            }
        }
    }

    pub fn version(&self, source: Source, project: &str, version_id: &str) -> Result<ProjectVersion> {
        match source {
            Source::Modrinth => parse_modrinth_version(&self.http.get_json(&format!("{MODRINTH}/version/{}", http::encode(version_id)), &[])?)
                .ok_or_else(|| Error::Invalid(tr!("version Modrinth illisible"))),
            Source::CurseForge => {
                let headers = self.cf()?;
                let v = self.http.get_json(&format!("{CURSEFORGE}/mods/{}/files/{}", http::encode(project), http::encode(version_id)), &headers)?;
                v.get("data").and_then(|f| parse_cf_file(f, project)).ok_or_else(|| Error::Invalid(tr!("fichier CurseForge illisible")))
            }
        }
    }

    /// Meilleure version : publiée (`release`) de préférence, sinon la plus récente.
    pub fn best(&self, source: Source, project: &str, mc: &str, loader: Loader) -> Result<Option<ProjectVersion>> {
        let list = self.versions(source, project, Some(mc), Some(loader))?;
        Ok(list.iter().find(|v| v.channel == "release").or(list.first()).cloned())
    }

    /// Installe un mod et ses dépendances obligatoires dans l'installation ; refuse un mod
    /// incompatible avec un mod déjà installé. Renvoie les mods ajoutés.
    pub fn install_mod(&self, layout: &Layout, profile: &mut Profile, version: ProjectVersion, cancel: &AtomicBool, sink: Sink) -> Result<Vec<InstalledMod>> {
        let mut queue = vec![(version, false)];
        let mut added = Vec::new();
        let mut seen: Vec<String> = profile.mods.iter().map(|m| m.project_id.clone()).filter(|p| !p.is_empty()).collect();
        while let Some((v, as_dep)) = queue.pop() {
            if let Some(conflict) = v.dependencies.iter().filter(|d| d.kind == "incompatible").find(|d| seen.contains(&d.project_id)) {
                let name = profile.mods.iter().find(|m| m.project_id == conflict.project_id).map(|m| m.name.clone()).unwrap_or_else(|| conflict.project_id.clone());
                return Err(Error::Invalid(tr!("{name} est incompatible avec {name2}, déjà installé", name = v.name, name2 = name)));
            }
            if profile.mods.iter().any(|m| m.project_id == v.project_id && m.version_id == v.id) {
                continue;
            }
            let Some(url) = v.file.url.clone() else {
                return Err(Error::Invalid(tr!("l'auteur de {name} n'autorise pas le téléchargement hors de CurseForge : téléchargez-le depuis sa page puis déposez-le dans le dossier mods", name = v.name)));
            };
            if !safe_name(&v.file.filename) {
                return Err(Error::Invalid(tr!("nom de fichier refusé : {filename}", filename = v.file.filename)));
            }
            sink(Event::Step(tr!("Téléchargement de {name}", name = v.name)));
            let dir = profile.mods_dir(layout);
            http::Http::download(self.http, &url, &dir.join(&v.file.filename), v.file.sha1.as_deref(), cancel, &mut |_| {})?;
            // Ancienne version du même projet retirée (mise à jour).
            if let Some(old) = profile.mods.iter().position(|m| m.project_id == v.project_id && m.source == v.source.id()) {
                let old = profile.mods.remove(old);
                if old.file != v.file.filename {
                    let _ = std::fs::remove_file(dir.join(&old.file));
                    let _ = std::fs::remove_file(dir.join(format!("{}.disabled", old.file)));
                }
            }
            let m = InstalledMod {
                file: v.file.filename.clone(),
                source: v.source.id().into(),
                project_id: v.project_id.clone(),
                version_id: v.id.clone(),
                name: v.name.clone(),
                version: v.number.clone(),
                sha1: v.file.sha1.clone().unwrap_or_default(),
                dependency: as_dep,
            };
            profile.mods.push(m.clone());
            added.push(m);
            seen.push(v.project_id.clone());
            for d in v.dependencies.iter().filter(|d| d.kind == "required") {
                if seen.contains(&d.project_id) || d.project_id.is_empty() {
                    continue;
                }
                let dep = match &d.version_id {
                    Some(id) => Some(self.version(v.source, &d.project_id, id)?),
                    None => self.best(v.source, &d.project_id, &profile.mc, profile.loader)?,
                };
                match dep {
                    Some(mut dv) => {
                        // Le nom affiché est celui du projet (la version n'a souvent qu'un numéro).
                        if dv.source == Source::Modrinth {
                            if let Some(t) = modrinth_project_title(self.http, &d.project_id) {
                                dv.name = format!("{t} {}", dv.number);
                            }
                        }
                        if dv.name.is_empty() {
                            dv.name = d.project_id.clone();
                        }
                        seen.push(d.project_id.clone());
                        queue.push((dv, true));
                    }
                    None => {
                        return Err(Error::Invalid(tr!("{name} demande une dépendance sans version compatible avec Minecraft {mc} / {name2} ({project_id})", name = v.name, mc = profile.mc, name2 = profile.loader.name(), project_id = d.project_id)))
                    }
                }
            }
        }
        profiles::save(layout, profile).map_err(|e| Error::Disk(e.to_string()))?;
        Ok(added)
    }
}

/// Mod Modrinth : nom de projet réel pour l'affichage (la version ne le donne pas).
pub fn modrinth_project_title(http: &Http, id: &str) -> Option<String> {
    http.get_json(&format!("{MODRINTH}/project/{}", http::encode(id)), &[]).ok().map(|p| s(&p, "title")).filter(|t| !t.is_empty())
}

fn parse_modrinth_version(v: &Value) -> Option<ProjectVersion> {
    let files = v.get("files")?.as_array()?;
    let f = files.iter().find(|f| f.get("primary").and_then(|p| p.as_bool()).unwrap_or(false)).or(files.first())?;
    Some(ProjectVersion {
        source: Source::Modrinth,
        id: s(v, "id"),
        project_id: s(v, "project_id"),
        name: s(v, "name"),
        number: s(v, "version_number"),
        channel: s(v, "version_type"),
        game_versions: strs(v, "game_versions"),
        loaders: strs(v, "loaders"),
        file: FileRef {
            url: f.get("url").and_then(|u| u.as_str()).map(str::to_string),
            filename: s(f, "filename"),
            sha1: f.pointer("/hashes/sha1").and_then(|h| h.as_str()).map(str::to_string),
            size: f.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
        },
        dependencies: v
            .get("dependencies")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .map(|d| Dependency {
                        project_id: s(d, "project_id"),
                        version_id: d.get("version_id").and_then(|x| x.as_str()).map(str::to_string),
                        kind: s(d, "dependency_type"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        date: s(v, "date_published"),
    })
}

fn parse_cf_file(f: &Value, project: &str) -> Option<ProjectVersion> {
    let gv = strs(f, "gameVersions");
    let loaders: Vec<String> = gv.iter().filter_map(|g| Loader::from_id(g)).filter(|l| *l != Loader::Vanilla).map(|l| l.id().to_string()).collect();
    let channel = match f.get("releaseType").and_then(|r| r.as_u64()) {
        Some(1) => "release",
        Some(2) => "beta",
        _ => "alpha",
    };
    Some(ProjectVersion {
        source: Source::CurseForge,
        id: f.get("id")?.as_u64()?.to_string(),
        project_id: f.get("modId").and_then(|m| m.as_u64()).map(|m| m.to_string()).unwrap_or_else(|| project.to_string()),
        name: s(f, "displayName"),
        number: s(f, "displayName"),
        channel: channel.into(),
        game_versions: gv.iter().filter(|g| g.chars().next().is_some_and(|c| c.is_ascii_digit())).cloned().collect(),
        loaders,
        file: FileRef {
            url: f.get("downloadUrl").and_then(|u| u.as_str()).filter(|u| u.starts_with("https://")).map(str::to_string),
            filename: s(f, "fileName"),
            sha1: f.get("hashes").and_then(|h| h.as_array()).and_then(|a| a.iter().find(|h| h.get("algo").and_then(|x| x.as_u64()) == Some(1))).map(|h| s(h, "value")),
            size: f.get("fileLength").and_then(|x| x.as_u64()).unwrap_or(0),
        },
        dependencies: f
            .get("dependencies")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|d| {
                        let kind = match d.get("relationType").and_then(|r| r.as_u64())? {
                            3 => "required",
                            5 => "incompatible",
                            2 => "optional",
                            1 => "embedded",
                            _ => return None,
                        };
                        Some(Dependency { project_id: d.get("modId")?.as_u64()?.to_string(), version_id: None, kind: kind.into() })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        date: s(f, "fileDate"),
    })
}

/// Nom de fichier sans chemin ni caractère spécial.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', ':']) && !name.starts_with('.') && name.len() < 200
}

// ------------------------------------------------------------- mods d'une installation

#[derive(Clone, Debug, PartialEq)]
pub struct ModFile {
    pub file: String,
    pub enabled: bool,
    pub size: u64,
    /// Informations connues (installé par Halcyon), sinon mod déposé à la main.
    pub info: Option<InstalledMod>,
}

/// Mods présents dans le dossier `mods` (activés `.jar`, désactivés `.jar.disabled`).
pub fn scan(layout: &Layout, profile: &Profile) -> Vec<ModFile> {
    let dir = profile.mods_dir(layout);
    let mut out: Vec<ModFile> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_string();
                    let (file, enabled) = if let Some(base) = name.strip_suffix(".disabled") {
                        (base.to_string(), false)
                    } else {
                        (name.clone(), true)
                    };
                    if !file.ends_with(".jar") && !file.ends_with(".zip") {
                        return None;
                    }
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    let info = profile.mods.iter().find(|m| m.file == file).cloned();
                    Some(ModFile { file, enabled, size, info })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|a| a.file.to_lowercase());
    out
}

/// Active ou désactive un mod (renommage `.disabled`, réversible).
pub fn set_enabled(layout: &Layout, profile: &Profile, file: &str, enabled: bool) -> std::io::Result<()> {
    if !safe_name(file) {
        return Err(std::io::Error::other(tr!("nom refusé")));
    }
    let dir = profile.mods_dir(layout);
    let (on, off) = (dir.join(file), dir.join(format!("{file}.disabled")));
    match (enabled, on.is_file(), off.is_file()) {
        (true, false, true) => std::fs::rename(off, on),
        (false, true, false) => std::fs::rename(on, off),
        _ => Ok(()),
    }
}

/// Supprime un mod (fichier et fiche).
pub fn remove(layout: &Layout, profile: &mut Profile, file: &str) -> std::io::Result<()> {
    if !safe_name(file) {
        return Err(std::io::Error::other(tr!("nom refusé")));
    }
    let dir = profile.mods_dir(layout);
    for p in [dir.join(file), dir.join(format!("{file}.disabled"))] {
        if p.is_file() {
            std::fs::remove_file(p)?;
        }
    }
    profile.mods.retain(|m| m.file != file);
    profiles::save(layout, profile)
}

/// Mises à jour disponibles (mods Modrinth) : (fichier, nouvelle version).
pub fn modrinth_updates(http: &Http, layout: &Layout, profile: &Profile) -> Result<Vec<(String, ProjectVersion)>> {
    let dir = profile.mods_dir(layout);
    let mut hashes: Vec<(String, String)> = Vec::new();
    for m in profile.mods.iter().filter(|m| m.source == "modrinth") {
        let path = dir.join(&m.file);
        let path = if path.is_file() { path } else { dir.join(format!("{}.disabled", m.file)) };
        if let Ok(h) = http::sha1_file(&path) {
            hashes.push((m.file.clone(), h));
        }
    }
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let mut loaders = vec![profile.loader.id().to_string()];
    if profile.loader == Loader::Quilt {
        loaders.push("fabric".into());
    }
    let body = serde_json::json!({
        "hashes": hashes.iter().map(|(_, h)| h).collect::<Vec<_>>(),
        "algorithm": "sha1",
        "loaders": loaders,
        "game_versions": [profile.mc],
    });
    let v = http.post_json(&format!("{MODRINTH}/version_files/update"), &body, &[])?;
    let mut out = Vec::new();
    for (file, h) in hashes {
        if let Some(nv) = v.get(&h).and_then(parse_modrinth_version) {
            let current = profile.mods.iter().find(|m| m.file == file).map(|m| m.version_id.clone()).unwrap_or_default();
            if nv.id != current {
                out.push((file, nv));
            }
        }
    }
    Ok(out)
}

/// Taille lisible.
pub fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => tr!("{v} Go", v = halcyon_i18n::decimal(b as f64 / (1u64 << 30) as f64, 1)),
        b if b >= 1 << 20 => tr!("{v} Mo", v = halcyon_i18n::decimal(b as f64 / (1u64 << 20) as f64, 1)),
        b if b >= 1 << 10 => tr!("{v} Ko", v = halcyon_i18n::number(b >> 10)),
        b => tr!("{v} o", v = b),
    }
}

/// Le dossier des mods existe (création à l'ouverture).
pub fn ensure_dirs(layout: &Layout, profile: &Profile) {
    let _ = std::fs::create_dir_all(profile.mods_dir(layout));
    let _ = std::fs::create_dir_all(profile.game_dir(layout).join("resourcepacks"));
}

/// Utilisé par les tests et l'export : fichier présent dans `mods`.
pub fn mod_path(layout: &Layout, profile: &Profile, file: &str) -> Option<std::path::PathBuf> {
    let dir = profile.mods_dir(layout);
    [dir.join(file), dir.join(format!("{file}.disabled"))].into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_modrinth_and_curseforge() {
        let v = parse_modrinth_version(&json!({
            "id":"AbCd","project_id":"P1","name":"Sodium 0.5.8","version_number":"mc1.20.1-0.5.8","version_type":"release",
            "game_versions":["1.20.1"],"loaders":["fabric","quilt"],"date_published":"2024-01-01",
            "files":[{"url":"https://cdn.modrinth.com/a.jar","filename":"sodium.jar","primary":true,"size":10,"hashes":{"sha1":"aa","sha512":"bb"}}],
            "dependencies":[{"project_id":"FAPI","version_id":null,"dependency_type":"required"},{"project_id":"OF","dependency_type":"incompatible"}]
        }))
        .unwrap();
        assert_eq!(v.file.filename, "sodium.jar");
        assert_eq!(v.dependencies.len(), 2);
        assert_eq!(v.dependencies[0].kind, "required");
        let cf = parse_cf_file(
            &json!({"id": 4567, "modId": 238222, "displayName":"JEI 15.2", "fileName":"jei.jar", "releaseType":1, "fileLength":20,
                    "downloadUrl": null, "gameVersions":["1.20.1","Forge","NeoForge"],
                    "hashes":[{"value":"cc","algo":1},{"value":"md5","algo":2}],
                    "dependencies":[{"modId":1,"relationType":3},{"modId":2,"relationType":4}]}),
            "238222",
        )
        .unwrap();
        assert_eq!(cf.file.url, None, "distribution interdite par l'auteur");
        assert_eq!(cf.file.sha1.as_deref(), Some("cc"));
        assert_eq!(cf.loaders, vec!["forge", "neoforge"]);
        assert_eq!(cf.game_versions, vec!["1.20.1"]);
        assert_eq!(cf.dependencies.len(), 1);
    }

    #[test]
    fn enable_disable_remove() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path());
        let mut p = profiles::new_profile(&l, "Fabric 1.20.1", "1.20.1", Loader::Fabric, "0.15.7", "x");
        profiles::save(&l, &p).unwrap();
        ensure_dirs(&l, &p);
        std::fs::write(p.mods_dir(&l).join("a.jar"), b"x").unwrap();
        std::fs::write(p.mods_dir(&l).join("notes.txt"), b"x").unwrap();
        assert_eq!(scan(&l, &p).len(), 1);
        set_enabled(&l, &p, "a.jar", false).unwrap();
        assert!(p.mods_dir(&l).join("a.jar.disabled").is_file());
        assert!(!scan(&l, &p)[0].enabled);
        set_enabled(&l, &p, "a.jar", true).unwrap();
        assert!(scan(&l, &p)[0].enabled);
        assert!(set_enabled(&l, &p, "../x.jar", true).is_err());
        remove(&l, &mut p, "a.jar").unwrap();
        assert!(scan(&l, &p).is_empty());
        assert!(p.mods_dir(&l).join("notes.txt").is_file());
        assert!(!safe_name("../a.jar") && safe_name("a b.jar"));
        assert_eq!(human(1536 * 1024), "1,5 Mo", "format de la langue de référence (aucun catalogue chargé)");
    }
}
