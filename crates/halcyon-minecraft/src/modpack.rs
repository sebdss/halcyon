//! Modpacks : format `.mrpack` de Modrinth (installation, mise à jour, import, export) et
//! archives CurseForge (`manifest.json`, avec la clé d'API). Les fichiers du modpack sont
//! téléchargés depuis les domaines autorisés par la spécification de Modrinth et vérifiés
//! par empreinte ; aucun chemin ne peut sortir du dossier de l'installation.

use crate::http::{self, Error, Http, Result};
use crate::install::{fetch_all, Event, Job, Layout, Sink};
use crate::loaders::Loader;
use crate::profiles::{self, InstalledMod, ModpackRef, Profile};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Domaines de téléchargement autorisés par la spécification `.mrpack`.
const ALLOWED_HOSTS: [&str; 4] = ["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"];

/// Chemin relatif sûr (pas de `..`, pas de racine, pas de lettre de lecteur).
pub fn safe_rel(p: &str) -> Option<PathBuf> {
    let path = Path::new(p);
    if p.is_empty() || p.contains(':') || p.starts_with(['/', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn allowed_url(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?']).next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host)
}

/// Contenu lu d'un `.mrpack`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub mc: String,
    pub loader: Loader,
    pub loader_version: String,
    pub files: Vec<(PathBuf, Job)>,
}

pub fn read_index(v: &Value) -> Result<Pack> {
    if v.get("game").and_then(|g| g.as_str()) != Some("minecraft") {
        return Err(Error::Invalid(tr!("ce modpack n'est pas destiné à Minecraft").into()));
    }
    let deps = v.get("dependencies").and_then(|d| d.as_object()).ok_or_else(|| Error::Invalid(tr!("modpack sans dépendances").into()))?;
    let mc = deps.get("minecraft").and_then(|x| x.as_str()).ok_or_else(|| Error::Invalid(tr!("modpack sans version de Minecraft").into()))?.to_string();
    let mut loader = Loader::Vanilla;
    let mut loader_version = String::new();
    for (k, l) in [("fabric-loader", Loader::Fabric), ("quilt-loader", Loader::Quilt), ("forge", Loader::Forge), ("neoforge", Loader::NeoForge)] {
        if let Some(x) = deps.get(k).and_then(|x| x.as_str()) {
            loader = l;
            loader_version = x.to_string();
        }
    }
    let mut files = Vec::new();
    for f in v.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default() {
        if f.pointer("/env/client").and_then(|e| e.as_str()) == Some("unsupported") {
            continue;
        }
        let path = f.get("path").and_then(|p| p.as_str()).unwrap_or_default();
        let rel = safe_rel(path).ok_or_else(|| Error::Invalid(tr!("chemin refusé dans le modpack : {path}", path = path)))?;
        let url = f
            .get("downloads")
            .and_then(|d| d.as_array())
            .and_then(|a| a.iter().filter_map(|u| u.as_str()).find(|u| allowed_url(u)))
            .ok_or_else(|| Error::Invalid(tr!("{path} : aucune adresse de téléchargement autorisée", path = path)))?;
        files.push((
            rel,
            Job {
                url: url.to_string(),
                dest: PathBuf::new(),
                sha1: f.pointer("/hashes/sha1").and_then(|h| h.as_str()).map(str::to_string),
                size: f.get("fileSize").and_then(|s| s.as_u64()),
            },
        ));
    }
    Ok(Pack {
        name: v.get("name").and_then(|x| x.as_str()).unwrap_or("Modpack").to_string(),
        version: v.get("versionId").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        summary: v.get("summary").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        mc,
        loader,
        loader_version,
        files,
    })
}

/// Lit `modrinth.index.json` d'une archive `.mrpack`.
pub fn read_mrpack(path: &Path) -> Result<Pack> {
    let f = std::fs::File::open(path).map_err(|e| Error::Disk(format!("{} : {e}", path.display())))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| Error::Invalid(tr!("archive illisible : {e}", e = e)))?;
    let mut text = String::new();
    zip.by_name("modrinth.index.json")
        .map_err(|_| Error::Invalid(tr!("ce fichier n'est pas un modpack Modrinth (.mrpack)").into()))?
        .read_to_string(&mut text)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    read_index(&serde_json::from_str(&text).map_err(|e| Error::Invalid(tr!("index illisible : {e}", e = e)))?)
}

/// Extrait `overrides/` puis `client-overrides/` dans le dossier de jeu ; renvoie les fichiers écrits.
fn extract_overrides(zip_path: &Path, dir: &Path, prefixes: &[&str]) -> Result<Vec<String>> {
    let f = std::fs::File::open(zip_path).map_err(|e| Error::Disk(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut written = Vec::new();
    for prefix in prefixes {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| Error::Invalid(e.to_string()))?;
            let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else { continue };
            let Some(rel) = name.strip_prefix(&format!("{prefix}/")) else { continue };
            if entry.is_dir() || rel.is_empty() {
                continue;
            }
            let rel_path = safe_rel(rel).ok_or_else(|| Error::Invalid(tr!("chemin refusé dans le modpack : {rel}", rel = rel)))?;
            let dest = dir.join(&rel_path);
            if let Some(d) = dest.parent() {
                std::fs::create_dir_all(d).map_err(|e| Error::Disk(e.to_string()))?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| Error::Disk(e.to_string()))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| Error::Disk(e.to_string()))?;
            written.push(rel.to_string());
        }
    }
    Ok(written)
}

/// Installe (ou met à jour) un `.mrpack` dans une installation. Les fichiers posés par la
/// version précédente du modpack sont retirés ; mondes, captures et réglages restent.
/// Le loader et la version de Minecraft sont installés ensuite par l'appelant.
pub fn install_mrpack(http: &Http, layout: &Layout, mrpack: &Path, reference: ModpackRef, existing: Option<Profile>, cancel: &AtomicBool, sink: Sink) -> Result<Profile> {
    let pack = read_mrpack(mrpack)?;
    sink(Event::Step(format!("Modpack {} {}", pack.name, pack.version)));
    let mut profile = match existing {
        Some(p) => p,
        None => profiles::new_profile(layout, &pack.name, &pack.mc, pack.loader, &pack.loader_version, ""),
    };
    let dir = profile.game_dir(layout);
    std::fs::create_dir_all(&dir).map_err(|e| Error::Disk(e.to_string()))?;
    if let Some(old) = &profile.modpack {
        for rel in &old.files {
            if let Some(r) = safe_rel(rel) {
                let _ = std::fs::remove_file(dir.join(&r));
                let _ = std::fs::remove_file(dir.join(format!("{rel}.disabled")));
            }
        }
        profile.mods.retain(|m| m.source != "modpack");
    }
    let jobs: Vec<Job> = pack.files.iter().map(|(rel, j)| Job { dest: dir.join(rel), ..j.clone() }).collect();
    sink(Event::Step(tr!("Téléchargement des fichiers du modpack ({jobs})", jobs = jobs.len())));
    fetch_all(http, jobs, true, cancel, sink)?;
    sink(Event::Step(tr!("Fichiers de configuration du modpack").into()));
    let mut files: Vec<String> = pack.files.iter().map(|(rel, _)| rel.to_string_lossy().replace('\\', "/")).collect();
    files.extend(extract_overrides(mrpack, &dir, &["overrides", "client-overrides"])?);
    for (rel, job) in &pack.files {
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        if let Some(file) = rel_s.strip_prefix("mods/") {
            profile.mods.push(InstalledMod {
                file: file.to_string(),
                source: "modpack".into(),
                project_id: String::new(),
                version_id: String::new(),
                name: file.trim_end_matches(".jar").to_string(),
                version: String::new(),
                sha1: job.sha1.clone().unwrap_or_default(),
                dependency: false,
            });
        }
    }
    files.sort();
    files.dedup();
    profile.mc = pack.mc.clone();
    profile.loader = pack.loader;
    profile.loader_version = pack.loader_version.clone();
    profile.modpack = Some(ModpackRef { files, name: pack.name.clone(), version: pack.version.clone(), ..reference });
    profiles::save(layout, &profile).map_err(|e| Error::Disk(e.to_string()))?;
    Ok(profile)
}

/// Exporte une installation en `.mrpack` : les mods connus de Modrinth sont référencés (avec
/// leurs empreintes), les autres fichiers et `config/` vont dans `overrides/`.
pub fn export_mrpack(http: &Http, layout: &Layout, profile: &Profile, out: &Path, version: &str) -> Result<usize> {
    let dir = profile.game_dir(layout);
    let mods_dir = profile.mods_dir(layout);
    let mut hashes = Vec::new();
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&mods_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".jar") {
                let h = http::sha1_file(&e.path()).map_err(|e| Error::Disk(e.to_string()))?;
                hashes.push(h.clone());
                entries.push((h, e.path()));
            }
        }
    }
    let known: Value = if hashes.is_empty() {
        Value::Null
    } else {
        http.post_json(&format!("{}/version_files", crate::mods::MODRINTH), &serde_json::json!({"hashes": hashes, "algorithm": "sha1"}), &[]).unwrap_or(Value::Null)
    };
    let mut files = Vec::new();
    let mut overrides: Vec<(String, PathBuf)> = Vec::new();
    for (h, path) in &entries {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let f = known.get(h).and_then(|v| v.get("files")).and_then(|f| f.as_array()).and_then(|a| a.iter().find(|f| f.pointer("/hashes/sha1").and_then(|x| x.as_str()) == Some(h.as_str())));
        match f {
            Some(f) => files.push(serde_json::json!({
                "path": format!("mods/{name}"),
                "hashes": {"sha1": h, "sha512": f.pointer("/hashes/sha512").cloned().unwrap_or(Value::Null)},
                "env": {"client": "required", "server": "required"},
                "downloads": [f.get("url").cloned().unwrap_or(Value::Null)],
                "fileSize": f.get("size").cloned().unwrap_or(Value::Null),
            })),
            None => overrides.push((format!("overrides/mods/{name}"), path.clone())),
        }
    }
    for sub in ["config", "resourcepacks", "shaderpacks"] {
        collect(&dir.join(sub), &format!("overrides/{sub}"), &mut overrides);
    }
    let mut deps = serde_json::Map::new();
    deps.insert("minecraft".into(), Value::String(profile.mc.clone()));
    let key = match profile.loader {
        Loader::Fabric => Some("fabric-loader"),
        Loader::Quilt => Some("quilt-loader"),
        Loader::Forge => Some("forge"),
        Loader::NeoForge => Some("neoforge"),
        Loader::Vanilla => None,
    };
    if let Some(k) = key {
        deps.insert(k.into(), Value::String(profile.loader_version.clone()));
    }
    let index = serde_json::json!({"formatVersion": 1, "game": "minecraft", "versionId": version, "name": profile.name, "files": files, "dependencies": deps});
    let tmp = out.with_extension("mrpack.part");
    let f = std::fs::File::create(&tmp).map_err(|e| Error::Disk(e.to_string()))?;
    let mut zip = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("modrinth.index.json", opts).map_err(|e| Error::Disk(e.to_string()))?;
    zip.write_all(serde_json::to_string_pretty(&index).unwrap_or_default().as_bytes()).map_err(|e| Error::Disk(e.to_string()))?;
    for (name, path) in &overrides {
        zip.start_file(name.as_str(), opts).map_err(|e| Error::Disk(e.to_string()))?;
        let bytes = std::fs::read(path).map_err(|e| Error::Disk(e.to_string()))?;
        zip.write_all(&bytes).map_err(|e| Error::Disk(e.to_string()))?;
    }
    zip.finish().map_err(|e| Error::Disk(e.to_string()))?;
    std::fs::rename(&tmp, out).map_err(|e| Error::Disk(e.to_string()))?;
    Ok(files.len())
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        if p.is_dir() {
            collect(&p, &format!("{prefix}/{name}"), out);
        } else {
            out.push((format!("{prefix}/{name}"), p));
        }
    }
}

// ------------------------------------------------------------------------------ CurseForge

/// Lit le `manifest.json` d'une archive CurseForge : (nom, version, mc, loader, version du
/// loader, [(projet, fichier)]).
#[allow(clippy::type_complexity)]
pub fn read_curseforge(path: &Path) -> Result<(String, String, String, Loader, String, Vec<(u64, u64)>)> {
    let f = std::fs::File::open(path).map_err(|e| Error::Disk(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| Error::Invalid(tr!("archive illisible : {e}", e = e)))?;
    let mut text = String::new();
    zip.by_name("manifest.json")
        .map_err(|_| Error::Invalid(tr!("ce fichier n'est ni un .mrpack ni un modpack CurseForge").into()))?
        .read_to_string(&mut text)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mc = v.pointer("/minecraft/version").and_then(|x| x.as_str()).ok_or_else(|| Error::Invalid(tr!("manifeste sans version de Minecraft").into()))?.to_string();
    let primary = v
        .pointer("/minecraft/modLoaders")
        .and_then(|l| l.as_array())
        .and_then(|a| a.iter().find(|l| l.get("primary").and_then(|p| p.as_bool()).unwrap_or(false)).or(a.first()))
        .and_then(|l| l.get("id").and_then(|i| i.as_str()))
        .unwrap_or("");
    let (loader, lv) = match primary.split_once('-') {
        Some((l, ver)) => (Loader::from_id(l).unwrap_or(Loader::Vanilla), ver.to_string()),
        None => (Loader::Vanilla, String::new()),
    };
    let files = v
        .get("files")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().filter_map(|f| Some((f.get("projectID")?.as_u64()?, f.get("fileID")?.as_u64()?))).collect())
        .unwrap_or_default();
    Ok((
        v.get("name").and_then(|x| x.as_str()).unwrap_or("Modpack").to_string(),
        v.get("version").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        mc,
        loader,
        lv,
        files,
    ))
}

/// Installe un modpack CurseForge (clé d'API requise). Renvoie l'installation et la liste des
/// fichiers que l'auteur interdit de télécharger hors du site (à déposer à la main).
pub fn install_curseforge(http: &Http, key: &str, layout: &Layout, zip_path: &Path, cancel: &AtomicBool, sink: Sink) -> Result<(Profile, Vec<String>)> {
    if key.trim().is_empty() {
        return Err(Error::Invalid(tr!("CurseForge demande une clé d'API (Minecraft › Paramètres)").into()));
    }
    let (name, version, mc, loader, lv, files) = read_curseforge(zip_path)?;
    sink(Event::Step(format!("Modpack {name} {version}")));
    let mut profile = profiles::new_profile(layout, &name, &mc, loader, &lv, "");
    let dir = profile.game_dir(layout);
    std::fs::create_dir_all(dir.join("mods")).map_err(|e| Error::Disk(e.to_string()))?;
    let ids: Vec<u64> = files.iter().map(|(_, f)| *f).collect();
    let resp = http.post_json(&format!("{}/mods/files", crate::mods::CURSEFORGE), &serde_json::json!({"fileIds": ids}), &[("x-api-key", key.trim())])?;
    let mut jobs = Vec::new();
    let mut manual = Vec::new();
    let mut written = Vec::new();
    for f in resp.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default() {
        let fname = f.get("fileName").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        if !crate::mods::safe_name(&fname) {
            continue;
        }
        match f.get("downloadUrl").and_then(|u| u.as_str()).filter(|u| u.starts_with("https://")) {
            Some(url) => {
                let sha1 = f.get("hashes").and_then(|h| h.as_array()).and_then(|a| a.iter().find(|h| h.get("algo").and_then(|x| x.as_u64()) == Some(1))).and_then(|h| h.get("value")?.as_str().map(str::to_string));
                jobs.push(Job { url: url.to_string(), dest: dir.join("mods").join(&fname), sha1: sha1.clone(), size: f.get("fileLength").and_then(|x| x.as_u64()) });
                written.push(format!("mods/{fname}"));
                profile.mods.push(InstalledMod {
                    file: fname.clone(),
                    source: "curseforge".into(),
                    project_id: f.get("modId").and_then(|x| x.as_u64()).map(|x| x.to_string()).unwrap_or_default(),
                    version_id: f.get("id").and_then(|x| x.as_u64()).map(|x| x.to_string()).unwrap_or_default(),
                    name: f.get("displayName").and_then(|x| x.as_str()).unwrap_or(&fname).to_string(),
                    version: String::new(),
                    sha1: sha1.unwrap_or_default(),
                    dependency: false,
                });
            }
            None => manual.push(fname),
        }
    }
    fetch_all(http, jobs, true, cancel, sink)?;
    written.extend(extract_overrides(zip_path, &dir, &["overrides"])?);
    profile.modpack = Some(ModpackRef { source: "curseforge".into(), project_id: String::new(), version_id: String::new(), name, version, files: written });
    profiles::save(layout, &profile).map_err(|e| Error::Disk(e.to_string()))?;
    Ok((profile, manual))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_pack(dir: &Path, index: &Value, extra: &[(&str, &[u8])]) -> PathBuf {
        let p = dir.join("pack.mrpack");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("modrinth.index.json", o).unwrap();
        z.write_all(index.to_string().as_bytes()).unwrap();
        for (n, b) in extra {
            z.start_file(*n, o).unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
        p
    }

    #[test]
    fn index_validation() {
        let ok = json!({"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Pack",
            "files":[{"path":"mods/a.jar","hashes":{"sha1":"x"},"downloads":["https://cdn.modrinth.com/a.jar"],"fileSize":1},
                     {"path":"mods/server.jar","env":{"client":"unsupported"},"downloads":["https://cdn.modrinth.com/s.jar"]}],
            "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.7"}});
        let p = read_index(&ok).unwrap();
        assert_eq!((p.mc.as_str(), p.loader, p.loader_version.as_str()), ("1.20.1", Loader::Fabric, "0.15.7"));
        assert_eq!(p.files.len(), 1, "fichier serveur ignoré");
        let mut bad = ok.clone();
        bad["files"][0]["path"] = json!("../../evil.jar");
        assert!(read_index(&bad).is_err());
        let mut bad = ok.clone();
        bad["files"][0]["downloads"] = json!(["https://evil.example/a.jar"]);
        assert!(read_index(&bad).is_err(), "domaine non autorisé");
        assert!(safe_rel("C:/x").is_none() && safe_rel("/etc/x").is_none() && safe_rel("config/a.toml").is_some());
    }

    #[test]
    fn install_update_and_export_without_network() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path().join("mc"));
        let v1 = json!({"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Mon Pack","files":[],"dependencies":{"minecraft":"1.20.1","forge":"47.2.0"}});
        let pack = make_pack(t.path(), &v1, &[("overrides/config/a.toml", b"a=1"), ("overrides/mods/local.jar", b"jar"), ("client-overrides/options.txt", b"o")]);
        let r = ModpackRef { source: "file".into(), project_id: String::new(), version_id: String::new(), name: String::new(), version: String::new(), files: vec![] };
        let p = install_mrpack(&Http::new(), &l, &pack, r.clone(), None, &AtomicBool::new(false), &|_| {}).unwrap();
        let dir = p.game_dir(&l);
        assert_eq!((p.loader, p.mc.as_str()), (Loader::Forge, "1.20.1"));
        assert!(dir.join("config/a.toml").is_file() && dir.join("options.txt").is_file() && dir.join("mods/local.jar").is_file());
        std::fs::create_dir_all(dir.join("saves/Monde")).unwrap();
        std::fs::write(dir.join("saves/Monde/level.dat"), b"x").unwrap();
        // Mise à jour : l'ancien fichier du modpack disparaît, le monde reste.
        let v2 = json!({"formatVersion":1,"game":"minecraft","versionId":"2.0","name":"Mon Pack","files":[],"dependencies":{"minecraft":"1.20.1","forge":"47.3.0"}});
        let d2 = t.path().join("v2");
        std::fs::create_dir_all(&d2).unwrap();
        let pack2 = make_pack(&d2, &v2, &[("overrides/config/b.toml", b"b=1")]);
        let p2 = install_mrpack(&Http::new(), &l, &pack2, r, Some(p.clone()), &AtomicBool::new(false), &|_| {}).unwrap();
        assert_eq!(p2.id, profiles::list(&l)[0].id);
        assert!(!dir.join("config/a.toml").exists() && dir.join("config/b.toml").is_file());
        assert!(dir.join("saves/Monde/level.dat").is_file(), "monde conservé");
        assert_eq!(p2.loader_version, "47.3.0");
        let out = t.path().join("export.mrpack");
        export_mrpack(&Http::new(), &l, &p2, &out, "3.0").unwrap();
        let back = read_mrpack(&out).unwrap();
        assert_eq!((back.mc.as_str(), back.loader, back.version.as_str()), ("1.20.1", Loader::Forge, "3.0"));
    }

    #[test]
    fn curseforge_manifest() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("cf.zip");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        z.start_file("manifest.json", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(br#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},"manifestType":"minecraftModpack","name":"CF","version":"1.2","files":[{"projectID":1,"fileID":2,"required":true}],"overrides":"overrides"}"#).unwrap();
        z.finish().unwrap();
        let (name, _, mc, loader, lv, files) = read_curseforge(&p).unwrap();
        assert_eq!((name.as_str(), mc.as_str(), loader, lv.as_str(), files), ("CF", "1.20.1", Loader::Forge, "47.2.0", vec![(1, 2)]));
        assert!(install_curseforge(&Http::new(), "", &Layout::new(t.path()), &p, &AtomicBool::new(false), &|_| {}).is_err(), "clé obligatoire");
    }
}
