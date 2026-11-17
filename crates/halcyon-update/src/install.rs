//! Installation transactionnelle d'un paquet vérifié, retour arrière et finalisation.
//!
//! Déroulement (exécuté par `halcyon-updater`, Halcyon fermé) :
//! 1. revérification du paquet en attente (signature, manifeste, SHA-256, structure) ;
//! 2. extraction et vérification dans `.hus/staging` (aucun fichier installé touché) ;
//! 3. journal de transaction écrit et synchronisé **avant** chaque modification ;
//! 4. pour chaque fichier : ancienne version conservée par lien physique dans `.hus/backup`
//!    (copie à défaut), puis remplacement par renommage atomique ;
//! 5. état `HealthCheck` : la nouvelle version est lancée et doit confirmer son démarrage ;
//! 6. succès : `finalize` (ancienne version gardée dans `.hus/previous`) ; échec, coupure,
//!    updater arrêté : `rollback` rejoue le journal à l'envers (opération idempotente).

use crate::core::{PackageFormat, Platform, State};
use crate::manifest::{Checked, Manifest, MANIFEST_NAME, SIGNATURE_NAME};
use crate::recovery::{self, log, write_atomic, Installed, Layout, INSTALLED};
use crate::security::{self, PublicKey};
use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Fiche de l'installateur `halcyon-setup` (tenue à jour si elle existe).
pub const SETUP_RECORD: &str = "install.json";
/// Nom stable de l'AppImage installée (Linux).
pub const APPIMAGE_FILE: &str = "Halcyon.AppImage";
/// Exécutable de l'installation Linux par `halcyon-setup` (devient un lien vers l'AppImage).
pub const LEGACY_LINUX_MAIN: &str = "halcyon";

/// Forme de l'installation à mettre à jour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// Dossier Windows (installateur, ou ZIP portable) : fichiers du ZIP.
    Zip,
    /// AppImage : `file` dans le dossier de l'installation.
    Appimage { file: String },
    /// Installation Linux par `halcyon-setup` : passage à l'AppImage, `halcyon` devient un
    /// lien vers `Halcyon.AppImage` (raccourcis, menu et `~/.local/bin` conservés).
    Migrate,
}

impl Target {
    pub fn format(&self) -> PackageFormat {
        match self {
            Target::Zip => PackageFormat::Zip,
            _ => PackageFormat::Appimage,
        }
    }

    /// Exécutable à lancer après installation.
    pub fn main_exe(&self, root: &Path) -> PathBuf {
        match self {
            Target::Zip => root.join(security::WINDOWS_MAIN),
            Target::Appimage { file } => root.join(file),
            Target::Migrate => root.join(APPIMAGE_FILE),
        }
    }

    pub fn arg(&self) -> String {
        match self {
            Target::Zip => "zip".into(),
            Target::Appimage { file } => format!("appimage:{file}"),
            Target::Migrate => "migrate".into(),
        }
    }

    pub fn from_arg(s: &str) -> Option<Target> {
        match s {
            "zip" => Some(Target::Zip),
            "migrate" => Some(Target::Migrate),
            _ => {
                let file = s.strip_prefix("appimage:")?;
                (security::safe_relative(file).as_deref() == Some(file) && !file.contains('/')).then(|| Target::Appimage { file: file.into() })
            }
        }
    }
}

/// Panne simulée (tests) : le processus s'arrête net à l'étape nommée par `HUS_FAULT`.
pub fn fault(layout: &Layout, point: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("HUS_FAULT").ok().as_deref() == Some(point) {
        log(layout, &tr!("panne simulée : {point}", point = point));
        std::process::exit(86);
    }
    let _ = (layout, point);
}

// ------------------------------------------------------------------ paquet en attente

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub version: String,
    pub package: String,
    pub target: Target,
    pub staged_at: u64,
}

/// Paquet en attente, revérifié.
#[derive(Clone, Debug)]
pub struct Verified {
    pub pending: Pending,
    pub checked: Checked,
    pub package_path: PathBuf,
}

/// Place un paquet téléchargé et vérifié dans `.hus/pending` (installation immédiate ou au
/// prochain démarrage). Le manifeste et sa signature l'accompagnent : l'updater revérifie tout.
pub fn stage(layout: &Layout, manifest_bytes: &[u8], signature: &str, package: &Path, version: &Version, target: &Target) -> Result<(), String> {
    let name = package.file_name().and_then(|n| n.to_str()).ok_or(tr!("nom de paquet invalide"))?.to_string();
    let dir = layout.pending();
    let tmp = layout.hus().join("pending.tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    write_atomic(&tmp.join(MANIFEST_NAME), manifest_bytes).map_err(|e| e.to_string())?;
    write_atomic(&tmp.join(SIGNATURE_NAME), signature.as_bytes()).map_err(|e| e.to_string())?;
    if std::fs::rename(package, tmp.join(&name)).is_err() {
        std::fs::copy(package, tmp.join(&name)).map_err(|e| tr!("paquet : {e}", e = e))?;
        let _ = std::fs::remove_file(package);
    }
    let p = Pending { version: version.to_string(), package: name, target: target.clone(), staged_at: recovery::now() };
    write_atomic(&tmp.join("pending.json"), &serde_json::to_vec_pretty(&p).unwrap_or_default()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir).map_err(|e| tr!("mise en attente : {e}", e = e))?;
    let mut r = recovery::load(layout);
    r.state = State::Staging;
    r.to = version.to_string();
    r.message = String::new();
    recovery::save(layout, &r)?;
    log(layout, &tr!("paquet {version} vérifié et mis en attente ({arg})", version = version, arg = target.arg()));
    Ok(())
}

pub fn read_pending(layout: &Layout) -> Option<Pending> {
    std::fs::read(layout.pending().join("pending.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn discard_pending(layout: &Layout) {
    let _ = std::fs::remove_dir_all(layout.pending());
}

/// Revérification complète du paquet en attente : rien n'est cru sur parole.
pub fn verify_pending(layout: &Layout, keys: &[PublicKey], platform: Platform) -> Result<Verified, String> {
    let dir = layout.pending();
    let pending = read_pending(layout).ok_or(tr!("aucune mise à jour en attente"))?;
    let bytes = std::fs::read(dir.join(MANIFEST_NAME)).map_err(|e| tr!("manifeste : {e}", e = e))?;
    let sig = std::fs::read_to_string(dir.join(SIGNATURE_NAME)).map_err(|e| tr!("signature : {e}", e = e))?;
    security::verify_signature(keys, &bytes, &sig)?;
    let checked = Manifest::parse(&bytes)?.check(None, None)?;
    if checked.version.to_string() != pending.version {
        return Err(tr!("paquet en attente incohérent avec son manifeste").into());
    }
    let pkg = checked.package(platform).ok_or(tr!("aucun paquet pour ce système dans le manifeste"))?;
    if pkg.name != pending.package || pkg.format != pending.target.format() {
        return Err(tr!("paquet en attente différent du manifeste signé").into());
    }
    let path = dir.join(&pkg.name);
    let len = std::fs::metadata(&path).map(|m| m.len()).map_err(|e| tr!("paquet : {e}", e = e))?;
    if len != pkg.size || security::sha256_file(&path).map_err(|e| e.to_string())? != pkg.sha256 {
        return Err(tr!("paquet en attente altéré (SHA-256 différent du manifeste signé)").into());
    }
    security::check_package(&path, pkg.format)?;
    Ok(Verified { pending, checked, package_path: path })
}

// ------------------------------------------------------------------ transaction

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Op {
    /// Fichier remplacé (ou créé) depuis `staging`.
    Replace,
    /// Fichier d'une ancienne version absent de la nouvelle.
    Remove,
    /// Lien symbolique `rel` → `link_to` (Linux).
    Link,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    rel: String,
    op: Op,
    had_old: bool,
    #[serde(default)]
    link_to: String,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    txid: String,
    from: String,
    to: String,
    #[serde(default)]
    security_floor: Option<String>,
    entries: Vec<Entry>,
}

fn rel_path(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let ok = !rel.is_empty() && !rel.starts_with('/') && !rel.contains(':') && rel.split('/').all(|p| !p.is_empty() && p != "." && p != "..");
    if ok { Ok(base.join(rel)) } else { Err(tr!("chemin refusé : {rel}", rel = rel)) }
}

fn save_journal(layout: &Layout, j: &Journal) -> Result<(), String> {
    write_atomic(&layout.journal(), &serde_json::to_vec(j).unwrap_or_default()).map_err(|e| tr!("journal de transaction : {e}", e = e))
}

#[cfg(unix)]
fn set_exec(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{} : {e}", path.display()))
}

#[cfg(not(unix))]
fn set_exec(_: &Path) -> Result<(), String> {
    Ok(())
}

/// Extrait le ZIP vérifié dans `staging` ; renvoie les fichiers (chemin, taille, SHA-256).
fn extract_zip(package: &Path, staging: &Path) -> Result<Vec<(String, u64, String)>, String> {
    let names = security::check_zip(package)?;
    let file = std::fs::File::open(package).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).map_err(|e| e.to_string())?;
        if e.is_dir() {
            continue;
        }
        let raw = e.name().map_err(|e| e.to_string())?.to_string();
        let rel = security::safe_relative(&raw).ok_or_else(|| tr!("chemin refusé : {raw}", raw = raw))?;
        debug_assert!(names.contains(&rel));
        let exec = e.unix_mode().is_some_and(|m| m & 0o111 != 0) || rel.ends_with(".exe") || rel == security::WINDOWS_MAIN;
        let dest = rel_path(staging, &rel)?;
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&dest).map_err(|e| format!("{rel} : {e}"))?;
        // La lecture complète vérifie le CRC-32 de chaque entrée.
        let n = std::io::copy(&mut e, &mut f).map_err(|e| tr!("{rel} : extraction impossible ({e})", rel = rel, e = e))?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        if exec {
            set_exec(&dest)?;
        }
        out.push((rel.clone(), n, security::sha256_file(&dest).map_err(|e| e.to_string())?));
    }
    Ok(out)
}

/// Fichiers de l'ancienne version (connus de HUS ou de l'installateur), jamais au-delà.
fn old_files(layout: &Layout) -> Vec<String> {
    if let Some(i) = recovery::read_installed(layout) {
        return i.files;
    }
    std::fs::read(layout.root.join(SETUP_RECORD))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("files")?.as_array().map(|a| a.iter().filter_map(|f| Some(f.get("path")?.as_str()?.to_string())).collect()))
        .unwrap_or_default()
}

/// Fiche `install.json` de l'installateur mise à jour (version et fichiers), s'il y en a une.
fn updated_setup_record(layout: &Layout, version: &Version, files: &[(String, u64, String)]) -> Option<Vec<u8>> {
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(layout.root.join(SETUP_RECORD)).ok()?).ok()?;
    let obj = v.as_object_mut()?;
    obj.insert("version".into(), serde_json::json!(version.to_string()));
    let list: Vec<serde_json::Value> = files
        .iter()
        .map(|(p, size, sha)| serde_json::json!({"path": p, "size": size, "sha256": sha, "executable": p.ends_with(".exe") || p == APPIMAGE_FILE || p == LEGACY_LINUX_MAIN}))
        .collect();
    obj.insert("files".into(), serde_json::Value::Array(list));
    serde_json::to_vec_pretty(&v).ok()
}

/// Installe le paquet en attente. Renvoie l'identifiant de transaction (état `HealthCheck`) ;
/// en cas d'échec, l'ancienne installation est rétablie avant de renvoyer l'erreur.
pub fn install(layout: &Layout, v: &Verified, installed: &Version) -> Result<String, String> {
    let to = &v.checked.version;
    let record = recovery::load(layout);
    let floor = [record.highest.as_deref(), record.security_floor.as_deref()].into_iter().flatten().filter_map(|s| s.parse::<Version>().ok()).max();
    // Plancher : plus haute version jamais installée ici et plancher de sécurité annoncé.
    // Même si des fichiers plus anciens ont été remis en place à la main, aucune version
    // inférieure à ce plancher n'est plus acceptée.
    security::check_version_policy(to, installed, floor.as_ref())?;
    // Reliquat d'une transaction interrompue : d'abord revenir à un état cohérent.
    if layout.journal().exists() {
        rollback(layout)?;
    }
    let txid = recovery::new_txid();
    recovery::set_state(layout, State::Installing, &format!("{installed} → {to}"))?;
    match apply(layout, v, installed, &txid) {
        Ok(()) => {
            let mut r = recovery::load(layout);
            r.state = State::HealthCheck;
            r.txid = txid.clone();
            r.from = installed.to_string();
            r.to = to.to_string();
            r.message = String::new();
            recovery::save(layout, &r)?;
            log(layout, &tr!("fichiers de {to} en place (transaction {txid}), contrôle de santé", to = to, txid = txid));
            Ok(txid)
        }
        Err(e) => {
            log(layout, &tr!("échec de l'installation : {e}", e = e));
            rollback(layout).map_err(|r| tr!("{e} ; retour arrière incomplet : {r}", e = e, r = r))?;
            Err(e)
        }
    }
}

fn apply(layout: &Layout, v: &Verified, installed: &Version, txid: &str) -> Result<(), String> {
    let root = &layout.root;
    let staging = layout.staging();
    let backup = layout.backup();
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
    let to = &v.checked.version;

    // 1. Nouveaux fichiers dans staging, vérifiés.
    let (files, mut entries): (Vec<(String, u64, String)>, Vec<Entry>) = match &v.pending.target {
        Target::Zip => {
            let files = extract_zip(&v.package_path, &staging)?;
            let mut entries: Vec<Entry> = files.iter().map(|(rel, ..)| Entry { rel: rel.clone(), op: Op::Replace, had_old: false, link_to: String::new() }).collect();
            for old in old_files(layout) {
                if security::safe_relative(&old).as_deref() == Some(old.as_str()) && !files.iter().any(|(r, ..)| r.eq_ignore_ascii_case(&old)) {
                    entries.push(Entry { rel: old, op: Op::Remove, had_old: false, link_to: String::new() });
                }
            }
            (files, entries)
        }
        Target::Appimage { file } => {
            let dest = staging.join(file);
            std::fs::copy(&v.package_path, &dest).map_err(|e| format!("AppImage : {e}"))?;
            set_exec(&dest)?;
            let f = (file.clone(), v.checked.package(Platform::LinuxX64).map(|p| p.size).unwrap_or(0), security::sha256_file(&dest).map_err(|e| e.to_string())?);
            (vec![f], vec![Entry { rel: file.clone(), op: Op::Replace, had_old: false, link_to: String::new() }])
        }
        Target::Migrate => {
            let dest = staging.join(APPIMAGE_FILE);
            std::fs::copy(&v.package_path, &dest).map_err(|e| format!("AppImage : {e}"))?;
            set_exec(&dest)?;
            let sha = security::sha256_file(&dest).map_err(|e| e.to_string())?;
            let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            let mut files: Vec<(String, u64, String)> = vec![(APPIMAGE_FILE.into(), size, sha.clone()), (LEGACY_LINUX_MAIN.into(), size, sha)];
            // Les autres fichiers de l'installateur (icône, documentation) restent en place.
            for old in old_files(layout) {
                if old != LEGACY_LINUX_MAIN && old != APPIMAGE_FILE {
                    if let Ok(p) = rel_path(root, &old) {
                        if let (Ok(m), Ok(h)) = (std::fs::metadata(&p), security::sha256_file(&p)) {
                            files.push((old, m.len(), h));
                        }
                    }
                }
            }
            let entries = vec![
                Entry { rel: APPIMAGE_FILE.into(), op: Op::Replace, had_old: false, link_to: String::new() },
                Entry { rel: LEGACY_LINUX_MAIN.into(), op: Op::Link, had_old: false, link_to: APPIMAGE_FILE.into() },
            ];
            (files, entries)
        }
    };
    // Re-contrôle du format après copie (le fichier installé est celui qui a été vérifié).
    if v.pending.target.format() == PackageFormat::Appimage {
        let name = match &v.pending.target {
            Target::Appimage { file } => file.clone(),
            _ => APPIMAGE_FILE.into(),
        };
        security::check_appimage(&staging.join(name))?;
    }

    // 2. Fiches : liste des fichiers HUS, et fiche de l'installateur s'il y en a une.
    let listed: Vec<String> = files.iter().map(|(r, ..)| r.clone()).collect();
    let installed_list = Installed { version: to.to_string(), files: listed };
    write_atomic(&staging.join(INSTALLED), &serde_json::to_vec_pretty(&installed_list).unwrap_or_default()).map_err(|e| e.to_string())?;
    entries.push(Entry { rel: INSTALLED.into(), op: Op::Replace, had_old: false, link_to: String::new() });
    if let Some(rec) = updated_setup_record(layout, to, &files) {
        write_atomic(&staging.join(SETUP_RECORD), &rec).map_err(|e| e.to_string())?;
        entries.push(Entry { rel: SETUP_RECORD.into(), op: Op::Replace, had_old: false, link_to: String::new() });
    }
    for e in &mut entries {
        let target = rel_path(root, &e.rel)?;
        if target.is_dir() && target.symlink_metadata().map(|m| !m.file_type().is_symlink()).unwrap_or(false) {
            return Err(tr!("{rel} : un dossier porte ce nom, rien n'a été modifié", rel = e.rel));
        }
        e.had_old = target.symlink_metadata().is_ok();
    }
    entries.retain(|e| e.op != Op::Remove || e.had_old);

    // 3. Journal, synchronisé avant la première modification.
    let journal = Journal {
        txid: txid.into(),
        from: installed.to_string(),
        to: to.to_string(),
        security_floor: v.checked.security_floor.as_ref().map(|f| f.to_string()),
        entries: entries.clone(),
    };
    save_journal(layout, &journal)?;
    log(layout, &trn!(entries.len(), "transaction {txid} : {n} opération", "transaction {txid} : {n} opérations", txid = txid));
    fault(layout, "after-journal");

    // 4. Mise en place.
    for (i, e) in entries.iter().enumerate() {
        let target = rel_path(root, &e.rel)?;
        if e.had_old {
            let b = rel_path(&backup, &e.rel)?;
            if let Some(d) = b.parent() {
                std::fs::create_dir_all(d).map_err(|err| err.to_string())?;
            }
            preserve(&target, &b).map_err(|err| tr!("sauvegarde de {rel} : {err}", rel = e.rel, err = err))?;
        }
        if let Some(d) = target.parent() {
            std::fs::create_dir_all(d).map_err(|err| err.to_string())?;
        }
        match e.op {
            Op::Replace => std::fs::rename(rel_path(&staging, &e.rel)?, &target).map_err(|err| format!("{} : {err}", e.rel))?,
            Op::Remove => std::fs::remove_file(&target).map_err(|err| format!("{} : {err}", e.rel))?,
            Op::Link => link_atomic(&e.link_to, &target).map_err(|err| format!("{} : {err}", e.rel))?,
        }
        if i == 0 {
            fault(layout, "mid-install");
        }
    }
    fault(layout, "after-install");
    Ok(())
}

/// Conserve `target` dans `backup` : lien physique (instantané, même contenu), copie à défaut.
/// Un lien symbolique est recréé tel quel.
fn preserve(target: &Path, backup: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(backup);
    let meta = target.symlink_metadata()?;
    #[cfg(unix)]
    if meta.file_type().is_symlink() {
        return std::os::unix::fs::symlink(std::fs::read_link(target)?, backup);
    }
    let _ = meta;
    if std::fs::hard_link(target, backup).is_ok() {
        return Ok(());
    }
    std::fs::copy(target, backup).map(|_| ())
}

#[cfg(unix)]
fn link_atomic(to: &str, at: &Path) -> std::io::Result<()> {
    let tmp = at.with_extension("hus-link");
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(to, &tmp)?;
    std::fs::rename(&tmp, at)
}

#[cfg(not(unix))]
fn link_atomic(_: &str, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other(tr!("liens symboliques non utilisés sous Windows")))
}

/// Rejoue le journal à l'envers : chaque fichier retrouve sa version d'avant la transaction.
/// Idempotent (peut être relancé après une nouvelle coupure). `Ok(false)` : rien à faire.
pub fn rollback(layout: &Layout) -> Result<bool, String> {
    let Ok(text) = std::fs::read(layout.journal()) else {
        let _ = std::fs::remove_dir_all(layout.staging());
        return Ok(false);
    };
    let j: Journal = serde_json::from_slice(&text).map_err(|e| tr!("journal de transaction illisible : {e}", e = e))?;
    log(layout, &tr!("retour arrière de la transaction {txid} ({from} → {to})", txid = j.txid, from = j.from, to = j.to));
    let backup = layout.backup();
    let mut errors = Vec::new();
    for (i, e) in j.entries.iter().enumerate().rev() {
        let target = rel_path(&layout.root, &e.rel)?;
        let b = rel_path(&backup, &e.rel)?;
        let res = if e.had_old {
            if b.symlink_metadata().is_ok() {
                std::fs::rename(&b, &target)
            } else {
                // Sauvegarde jamais faite : le fichier d'origine n'a pas été touché.
                Ok(())
            }
        } else {
            match std::fs::remove_file(&target) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        if let Err(err) = res {
            errors.push(format!("{} : {err}", e.rel));
        }
        if i == j.entries.len() / 2 {
            fault(layout, "mid-rollback");
        }
    }
    if !errors.is_empty() {
        let msg = errors.join(" ; ");
        log(layout, &tr!("retour arrière incomplet : {msg}", msg = msg));
        return Err(msg);
    }
    let _ = std::fs::remove_dir_all(layout.staging());
    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::remove_file(layout.journal());
    log(layout, &tr!("version {from} rétablie", from = j.from));
    Ok(true)
}

/// Transaction confirmée par le contrôle de santé : l'ancienne version est gardée dans
/// `.hus/previous`, le reste est nettoyé, l'état passe à `Completed`.
pub fn finalize(layout: &Layout, txid: &str) -> Result<(), String> {
    let j: Journal = serde_json::from_slice(&std::fs::read(layout.journal()).map_err(|e| tr!("journal : {e}", e = e))?).map_err(|e| e.to_string())?;
    if j.txid != txid {
        return Err(tr!("transaction inattendue").into());
    }
    let prev = layout.previous();
    let _ = std::fs::remove_dir_all(&prev);
    if layout.backup().exists() {
        std::fs::create_dir_all(&prev).map_err(|e| e.to_string())?;
        std::fs::rename(layout.backup(), prev.join(&j.from)).map_err(|e| tr!("conservation de la version précédente : {e}", e = e))?;
    }
    let _ = std::fs::remove_file(layout.journal());
    let _ = std::fs::remove_dir_all(layout.staging());
    let _ = std::fs::remove_dir_all(layout.health());
    discard_pending(layout);
    let mut r = recovery::load(layout);
    let highest = [r.highest.as_deref(), Some(j.to.as_str())].into_iter().flatten().filter_map(|s| s.parse::<Version>().ok()).max();
    r.highest = highest.map(|v| v.to_string());
    if j.security_floor.is_some() {
        r.security_floor = j.security_floor.clone();
    }
    r.state = State::Completed;
    r.txid = txid.into();
    r.from = j.from.clone();
    r.to = j.to.clone();
    r.blocked = None;
    r.acknowledged = false;
    r.message = tr_later!("Halcyon {to} installé", to = j.to);
    recovery::save(layout, &r)?;
    log(layout, &tr!("mise à jour {from} → {to} terminée", from = j.from, to = j.to));
    Ok(())
}

/// Une transaction attend-elle sa conclusion (coupure, updater arrêté) ?
pub fn interrupted(layout: &Layout) -> bool {
    layout.journal().exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_args_round_trip() {
        for t in [Target::Zip, Target::Migrate, Target::Appimage { file: "Halcyon.AppImage".into() }] {
            assert_eq!(Target::from_arg(&t.arg()), Some(t));
        }
        assert_eq!(Target::from_arg("appimage:../../x"), None);
        assert_eq!(Target::from_arg("appimage:a/b"), None);
        assert!(rel_path(Path::new("/r"), "../x").is_err());
    }
}
