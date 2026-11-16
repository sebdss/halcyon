//! Moteur d'installation : détection de l'existant, vérifications, installation et mise à
//! jour transactionnelles, réparation, désinstallation.
//!
//! Règles :
//! - seuls les fichiers listés dans le manifeste sont écrits, déplacés ou supprimés ; un
//!   fichier inconnu du dossier (dossier `data/` du mode portable, par exemple) n'est jamais
//!   touché ;
//! - les nouveaux fichiers sont d'abord écrits et vérifiés dans `.halcyon-staging`, puis mis
//!   en place ; les fichiers remplacés sont gardés dans `.halcyon-backup` jusqu'au succès et
//!   restaurés en cas d'échec ; un journal (`.halcyon-transaction.json`) permet de reprendre
//!   après une coupure ;
//! - la progression est calculée sur les octets réellement écrits puis vérifiés ;
//! - les données personnelles ne sont supprimées que si la désinstallation le demande
//!   explicitement.

use crate::payload::{safe_relative, sha256_file, FileEntry, Payload};
use crate::platform::{self, Env};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const RECORD: &str = "install.json";
const STAGING: &str = ".halcyon-staging";
const BACKUP: &str = ".halcyon-backup";
const JOURNAL: &str = ".halcyon-transaction.json";

/// Choix de l'utilisateur.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    pub dir: PathBuf,
    pub menu_entry: bool,
    pub desktop_shortcut: bool,
    pub autostart: bool,
    pub all_users: bool,
}

/// Trace d'une installation (`install.json` dans le dossier).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub version: String,
    pub platform: String,
    pub installed_at: i64,
    pub options: Options,
    pub files: Vec<FileEntry>,
    /// Raccourcis et entrées créés hors du dossier.
    #[serde(default)]
    pub integration: Vec<PathBuf>,
}

/// État d'un fichier installé.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Health {
    pub missing: Vec<String>,
    pub corrupt: Vec<String>,
}

impl Health {
    pub fn ok(&self) -> bool {
        self.missing.is_empty() && self.corrupt.is_empty()
    }
}

/// Ce que l'installateur trouve sur la machine.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// Aucune installation.
    Nothing,
    /// Installation gérée par cet installateur.
    Installed { record: Record, health: Health },
    /// Installation sans manifeste : ancien installateur Inno Setup, ancien script
    /// `install.sh`, ou installation partielle.
    Unmanaged { dir: PathBuf, version: Option<String>, origin: Origin },
    /// Transaction interrompue (coupure pendant une installation).
    Interrupted { dir: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Origin {
    /// Installateur Inno Setup (Windows, 0.1 à 0.4.0).
    InnoSetup,
    /// Script `install.sh` de l'archive Linux (0.2 à 0.4.0).
    ArchiveScript,
    /// Dossier contenant Halcyon sans manifeste (copie manuelle, installation partielle).
    Partial,
}

/// Opération proposée ou choisie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Install,
    Update,
    Reinstall,
    Repair,
}

/// Évènement de progression (toujours issu d'une opération réelle).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Étape courante (index dans `STEPS`).
    Step(usize),
    /// Octets traités dans l'étape / total de l'étape.
    Bytes(u64, u64),
    /// Fichier en cours.
    File(String),
    /// Composant terminé (libellé, réussite, détail).
    Component(String, bool, String),
    Warning(String),
}

/// Étapes de `repair_integration`.
pub const REPAIR_STEPS: [&str; 4] = ["Préparation", "Vérification des fichiers", "Raccourcis et intégration", "Enregistrement"];

pub const STEPS: [&str; 6] = [
    "Préparation",
    "Copie des fichiers",
    "Vérification de l'intégrité",
    "Mise en place",
    "Raccourcis et intégration",
    "Enregistrement",
];

#[derive(Serialize, Deserialize)]
struct Journal {
    /// Fichiers dont l'ancienne version est dans `.halcyon-backup` (chemins relatifs).
    moved: Vec<String>,
    /// Fichiers nouvellement posés (sans ancienne version).
    created: Vec<String>,
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Compare deux versions « x.y.z » (`None` si illisibles).
/// Comparaison semver (pré-versions comprises : `1.0.0-alpha.1` < `1.0.0`). Avant la 1.0,
/// une pré-version rendait la comparaison impossible et l'installateur proposait une
/// réinstallation au lieu d'une mise à jour.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use halcyon_core::util::version_newer;
    let ok = |v: &str| v.trim().trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit());
    if !ok(a) || !ok(b) {
        return None;
    }
    Some(if version_newer(b, a) {
        std::cmp::Ordering::Less
    } else if version_newer(a, b) {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    })
}

pub fn read_record(dir: &Path) -> Option<Record> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(RECORD)).ok()?).ok()
}

/// Vérifie chaque fichier enregistré (taille puis empreinte).
pub fn check_health(dir: &Path, files: &[FileEntry]) -> Health {
    let mut h = Health::default();
    for f in files {
        let Some(rel) = safe_relative(&f.path) else { continue };
        let p = dir.join(rel);
        match std::fs::metadata(&p) {
            Err(_) => h.missing.push(f.path.clone()),
            Ok(m) if m.len() != f.size => h.corrupt.push(f.path.clone()),
            Ok(_) => {
                if sha256_file(&p).map(|s| s != f.sha256).unwrap_or(true) {
                    h.corrupt.push(f.path.clone());
                }
            }
        }
    }
    h
}

/// Cherche une installation : emplacement mémorisé, registre (Windows), dossier par défaut,
/// puis anciennes méthodes d'installation.
pub fn detect(env: &Env) -> Found {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(env.pointer_file()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(d) = v.get("dir").and_then(|d| d.as_str()) {
                candidates.push(PathBuf::from(d));
            }
        }
    }
    #[cfg(windows)]
    let registered = platform::windows::registered();
    #[cfg(windows)]
    if let Some((dir, _, _, _)) = &registered {
        candidates.push(dir.clone());
    }
    candidates.push(env.default_dir.clone());
    for dir in &candidates {
        if dir.join(JOURNAL).is_file() {
            return Found::Interrupted { dir: dir.clone() };
        }
        if let Some(record) = read_record(dir) {
            let health = check_health(dir, &record.files);
            return Found::Installed { record, health };
        }
    }
    #[cfg(windows)]
    if let Some((dir, version, inno, _)) = registered {
        if dir.join(platform::main_exe_name()).is_file() {
            let origin = if inno { Origin::InnoSetup } else { Origin::Partial };
            return Found::Unmanaged { dir, version, origin };
        }
    }
    for dir in &candidates {
        if dir.join(platform::main_exe_name()).is_file() {
            return Found::Unmanaged { dir: dir.clone(), version: None, origin: Origin::Partial };
        }
    }
    // Ancien install.sh : exécutable copié (pas un lien) dans ~/.local/bin.
    #[cfg(not(windows))]
    {
        let legacy = platform::linux::bin_link(env);
        if legacy.symlink_metadata().is_ok_and(|m| m.file_type().is_file()) {
            return Found::Unmanaged { dir: env.bin_dir.clone(), version: None, origin: Origin::ArchiveScript };
        }
    }
    Found::Nothing
}

/// Opération recommandée face à l'existant et à la version de la charge utile.
pub fn recommend(found: &Found, payload_version: &str) -> Mode {
    match found {
        Found::Nothing => Mode::Install,
        Found::Interrupted { .. } => Mode::Repair,
        Found::Unmanaged { .. } => Mode::Update,
        Found::Installed { record, health } => match compare_versions(&record.version, payload_version) {
            Some(std::cmp::Ordering::Less) => Mode::Update,
            _ if !health.ok() => Mode::Repair,
            _ => Mode::Reinstall,
        },
    }
}

// -------------------------------------------------------------------------- vérifications

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub label: String,
    pub detail: String,
    pub level: Level,
}

fn check(label: &str, level: Level, detail: impl Into<String>) -> Check {
    Check { label: label.into(), detail: detail.into(), level }
}

/// Espace libre sur le volume qui contiendra `dir` (octets).
pub fn free_space(dir: &Path) -> Option<u64> {
    let probe = nearest_existing(dir)?;
    let probe = probe.canonicalize().unwrap_or(probe);
    let disks = sysinfo::Disks::new_with_refreshed_list_specifics(sysinfo::DiskRefreshKind::nothing().with_storage()).ok()?;
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn nearest_existing(dir: &Path) -> Option<PathBuf> {
    let mut p = dir.to_path_buf();
    loop {
        if p.exists() {
            return Some(p);
        }
        if !p.pop() {
            return None;
        }
    }
}

/// Peut-on écrire dans `dir` (ou dans le premier dossier parent existant) ?
pub fn writable(dir: &Path) -> Result<(), String> {
    let base = nearest_existing(dir).ok_or("aucun dossier parent existant")?;
    let probe = base.join(format!(".halcyon-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Halcyon est-il en cours d'exécution depuis `dir` ?
pub fn running_from(dir: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let Ok(mut sys) = System::new() else { return false };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let main = dir.join(platform::main_exe_name());
    let main = main.canonicalize().unwrap_or(main);
    sys.processes().values().any(|p| p.exe().is_some_and(|e| e == main))
}

/// Version de la glibc (Linux), par `getconf`.
pub fn glibc_version() -> Option<String> {
    let out = std::process::Command::new("getconf").arg("GNU_LIBC_VERSION").output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.split_whitespace().nth(1).map(str::to_string)
}

/// Bibliothèques partagées présentes (`ldconfig -p`) ; `None` si l'outil est absent.
pub fn shared_libs() -> Option<String> {
    for tool in ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"] {
        if let Ok(o) = std::process::Command::new(tool).arg("-p").output() {
            if o.status.success() {
                return Some(String::from_utf8_lossy(&o.stdout).into_owned());
            }
        }
    }
    None
}

/// Vérifications avant installation. Une ligne `Error` bloque l'opération.
pub fn system_checks(payload: &Payload, opts: &Options, found: &Found, mode: Mode) -> Vec<Check> {
    let m = &payload.manifest;
    let mut out = Vec::new();
    let here = platform::current_platform();
    out.push(if m.platform == here {
        check("Système", Level::Ok, sysinfo::System::long_os_version().unwrap_or(here))
    } else {
        check("Système", Level::Error, format!("ce programme d'installation est prévu pour {}, ce système est {here}", m.platform))
    });
    out.push(if std::env::consts::ARCH == "x86_64" {
        check("Architecture", Level::Ok, "x86_64 (64 bits)")
    } else {
        check("Architecture", Level::Error, format!("{} non prise en charge (x86_64 requis)", std::env::consts::ARCH))
    });
    if let Some(min) = &m.min_glibc {
        out.push(match glibc_version() {
            Some(v) if compare_versions(&v, min).is_some_and(|o| o != std::cmp::Ordering::Less) => {
                check("Bibliothèque C (glibc)", Level::Ok, format!("{v} (minimum {min})"))
            }
            Some(v) => check("Bibliothèque C (glibc)", Level::Error, format!("{v} trop ancienne : {min} ou plus récente requise (Ubuntu 24.04, Debian 13, Fedora 40 ou équivalent)")),
            None => check("Bibliothèque C (glibc)", Level::Warning, format!("version non déterminée (minimum {min})")),
        });
    }
    if cfg!(target_os = "linux") {
        match shared_libs() {
            Some(libs) => {
                let missing: Vec<&str> = [("libasound.so.2", "ALSA (son)"), ("libfontconfig.so.1", "Fontconfig (polices)"), ("libudev.so.1", "udev (manettes)")]
                    .iter()
                    .filter(|(lib, _)| !libs.contains(lib))
                    .map(|(_, name)| *name)
                    .collect();
                out.push(if missing.is_empty() {
                    check("Dépendances", Level::Ok, "ALSA, Fontconfig, udev présents")
                } else {
                    check("Dépendances", Level::Error, format!("manquantes : {} (installez-les avec le gestionnaire de paquets)", missing.join(", ")))
                });
            }
            None => out.push(check("Dépendances", Level::Warning, "non vérifiées (ldconfig introuvable)")),
        }
    }
    out.push(space_check(free_space(&opts.dir), required_space(m.total_size())));
    out.push(match writable(&opts.dir) {
        Ok(()) => check("Permissions", Level::Ok, format!("écriture possible dans {}", opts.dir.display())),
        Err(e) => check("Permissions", Level::Error, format!("écriture impossible dans {} : {e}", opts.dir.display())),
    });
    out.push(match found {
        Found::Nothing => check("Installation existante", Level::Ok, "aucune : nouvelle installation"),
        Found::Installed { record, health } => {
            let state = if health.ok() { "intacte".to_string() } else { format!("{} fichier(s) manquant(s), {} endommagé(s)", health.missing.len(), health.corrupt.len()) };
            let level = if health.ok() { Level::Info } else { Level::Warning };
            check("Installation existante", level, format!("version {} dans {} ({state})", record.version, record.options.dir.display()))
        }
        Found::Unmanaged { dir, version, origin } => {
            let what = match origin {
                Origin::InnoSetup => "ancien installateur Windows",
                Origin::ArchiveScript => "archive (install.sh)",
                Origin::Partial => "installation sans manifeste",
            };
            check(
                "Installation existante",
                Level::Info,
                format!("{}{} dans {} : elle sera reprise, vos données sont conservées", version.as_deref().map(|v| format!("version {v}, ")).unwrap_or_default(), what, dir.display()),
            )
        }
        Found::Interrupted { dir } => check("Installation existante", Level::Warning, format!("opération interrompue dans {} : elle sera réparée", dir.display())),
    });
    let installed_version = match found {
        Found::Installed { record, .. } => Some(record.version.clone()),
        Found::Unmanaged { version, .. } => version.clone(),
        _ => None,
    };
    if let Some(v) = installed_version {
        if compare_versions(&v, &m.version) == Some(std::cmp::Ordering::Greater) && mode != Mode::Repair {
            out.push(check("Version", Level::Warning, format!("la version installée ({v}) est plus récente que {} : retour à une version antérieure", m.version)));
        }
    }
    if running_from(&opts.dir) {
        out.push(check("Halcyon en cours d'exécution", Level::Error, "fermez Halcyon (icône de la zone de notification › Quitter), puis relancez la vérification"));
    }
    #[cfg(not(windows))]
    if Path::new("/usr/bin/halcyon").exists() {
        out.push(check("Conflit", Level::Warning, "Halcyon est aussi installé par le paquet système (/usr/bin/halcyon) : le menu peut afficher deux entrées"));
    }
    if opts.all_users && cfg!(not(windows)) {
        out.push(check("Installation pour tous", Level::Error, "sous Linux, utilisez le paquet .deb (géré par le système) pour tous les utilisateurs"));
    }
    out
}

/// Espace nécessaire : fichiers + 10 % de marge (copie temporaire et journal).
pub fn required_space(total: u64) -> u64 {
    total + total / 10
}

pub fn space_check(free: Option<u64>, need: u64) -> Check {
    match free {
        Some(free) if free >= need => check("Espace disque", Level::Ok, format!("{} libres, {} nécessaires", fmt_size(free), fmt_size(need))),
        Some(free) => check("Espace disque", Level::Error, format!("{} libres, {} nécessaires : libérez de la place ou choisissez un autre dossier", fmt_size(free), fmt_size(need))),
        None => check("Espace disque", Level::Warning, "espace libre non déterminé"),
    }
}

pub fn fmt_size(b: u64) -> String {
    let mb = b as f64 / 1_000_000.0;
    if mb >= 1000.0 { format!("{:.1} Go", mb / 1000.0).replace('.', ",") } else { format!("{mb:.0} Mo") }
}

// ---------------------------------------------------------------------------- opérations

/// Résultat d'une installation.
#[derive(Debug, Default)]
pub struct Report {
    pub warnings: Vec<String>,
    pub replaced: usize,
    pub written: usize,
}

fn rel_path(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    safe_relative(rel).map(|r| dir.join(r)).ok_or_else(|| format!("chemin refusé : {rel}"))
}

/// Reprise après coupure : remet en place les anciens fichiers et nettoie.
pub fn recover(dir: &Path) -> Result<(), String> {
    let jpath = dir.join(JOURNAL);
    if let Ok(text) = std::fs::read_to_string(&jpath) {
        let j: Journal = serde_json::from_str(&text).map_err(|e| format!("journal illisible : {e}"))?;
        for rel in &j.created {
            let _ = std::fs::remove_file(rel_path(dir, rel)?);
        }
        for rel in &j.moved {
            let from = rel_path(&dir.join(BACKUP), rel)?;
            let to = rel_path(dir, rel)?;
            if from.exists() {
                let _ = std::fs::remove_file(&to);
                std::fs::rename(&from, &to).map_err(|e| format!("restauration de {rel} : {e}"))?;
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir.join(STAGING));
    let _ = std::fs::remove_dir_all(dir.join(BACKUP));
    let _ = std::fs::remove_file(&jpath);
    Ok(())
}

/// Installation, mise à jour, réinstallation ou réparation.
#[allow(clippy::too_many_arguments)]
pub fn run(
    payload: &Payload,
    opts: &Options,
    env: &Env,
    mode: Mode,
    previous: Option<&Record>,
    stub: Option<&Path>,
    cancel: &AtomicBool,
    on: &mut dyn FnMut(Event),
) -> Result<Report, String> {
    let m = &payload.manifest;
    let dir = &opts.dir;
    let mut report = Report::default();

    // 1. Préparation
    on(Event::Step(0));
    std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    recover(dir)?;
    // Réparation : seuls les fichiers absents ou altérés sont réécrits.
    let todo: Vec<&FileEntry> = if mode == Mode::Repair {
        let h = check_health(dir, &m.files);
        m.files.iter().filter(|f| h.missing.contains(&f.path) || h.corrupt.contains(&f.path)).collect()
    } else {
        m.files.iter().collect()
    };
    on(Event::Component(format!("{} fichier(s) à écrire", todo.len()), true, String::new()));

    // 2. Copie dans le dossier temporaire, empreinte vérifiée au fil de l'écriture.
    on(Event::Step(1));
    let staging = dir.join(STAGING);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let total: u64 = todo.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    on(Event::Bytes(0, total));
    for f in &todo {
        if cancel.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err("installation annulée : rien n'a été modifié".into());
        }
        on(Event::File(f.path.clone()));
        let dest = rel_path(&staging, &f.path)?;
        let res = payload.extract(f, &dest, &mut |n| {
            done += n;
            on(Event::Bytes(done, total));
        });
        if let Err(e) = res {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    }

    // 3. Relecture complète des fichiers écrits.
    on(Event::Step(2));
    let mut checked = 0u64;
    on(Event::Bytes(0, total));
    for f in &todo {
        on(Event::File(f.path.clone()));
        let p = rel_path(&staging, &f.path)?;
        if sha256_file(&p).map_err(|e| e.to_string())? != f.sha256 {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("{} : empreinte incorrecte après écriture (disque défaillant ?)", f.path));
        }
        checked += f.size;
        on(Event::Bytes(checked, total));
    }
    on(Event::Component("Intégrité des fichiers".into(), true, format!("{} vérifié(s)", todo.len())));

    // 4. Mise en place : anciens fichiers mis de côté, nouveaux posés ; retour arrière si
    //    une étape échoue.
    on(Event::Step(3));
    let backup = dir.join(BACKUP);
    let mut journal = Journal { moved: Vec::new(), created: Vec::new() };
    let save_journal = |j: &Journal| std::fs::write(dir.join(JOURNAL), serde_json::to_vec(j).unwrap_or_default()).map_err(|e| e.to_string());
    save_journal(&journal)?;
    let place = |rel: &str, from_staging: bool, journal: &mut Journal| -> Result<(), String> {
        let target = rel_path(dir, rel)?;
        if target.is_dir() {
            // Jamais de dossier déplacé ou supprimé : il peut contenir des données.
            return Err(format!("{rel} : un dossier porte déjà ce nom dans {}", dir.display()));
        }
        if target.exists() {
            let b = rel_path(&backup, rel)?;
            if let Some(d) = b.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&target, &b).map_err(|e| format!("{rel} : {e}"))?;
            journal.moved.push(rel.to_string());
        } else if from_staging {
            journal.created.push(rel.to_string());
        }
        save_journal(journal)?;
        if from_staging {
            if let Some(d) = target.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::rename(rel_path(&staging, rel)?, &target).map_err(|e| format!("{rel} : {e}"))?;
        }
        Ok(())
    };
    let mut result: Result<(), String> = Ok(());
    for f in &todo {
        on(Event::File(f.path.clone()));
        if let Err(e) = place(&f.path, true, &mut journal) {
            result = Err(e);
            break;
        }
        report.written += 1;
    }
    // Fichiers de l'ancienne version absents de la nouvelle : mis de côté (supprimés au succès).
    if result.is_ok() {
        if let Some(prev) = previous {
            for old in &prev.files {
                if !m.files.iter().any(|f| f.path == old.path) && rel_path(dir, &old.path).is_ok_and(|p| p.exists()) {
                    if let Err(e) = place(&old.path, false, &mut journal) {
                        result = Err(e);
                        break;
                    }
                }
            }
        }
    }
    if let Err(e) = result {
        recover(dir).map_err(|r| format!("{e} ; retour arrière incomplet : {r}"))?;
        return Err(format!("{e} (l'installation précédente a été rétablie)"));
    }
    report.replaced = journal.moved.len();
    // Programme de maintenance (réparation, désinstallation) : l'installateur sans sa charge utile.
    if let Some(stub) = stub {
        if let Err(e) = copy_stub(stub, &dir.join(platform::maintenance_exe_name())) {
            report.warnings.push(format!("programme de désinstallation non copié : {e}"));
            on(Event::Warning(format!("programme de désinstallation non copié : {e}")));
        }
    }
    on(Event::Component("Fichiers de Halcyon".into(), true, format!("{} écrit(s), {} remplacé(s)", report.written, report.replaced)));

    // 5. Intégration au système.
    on(Event::Step(4));
    let integration = match integrate(env, opts, m, dir) {
        Ok(list) => {
            on(Event::Component("Raccourcis et menu".into(), true, format!("{} élément(s)", list.len())));
            list
        }
        Err(e) => {
            // Les fichiers sont en place et valides : l'intégration se refait par « Réparer ».
            on(Event::Component("Raccourcis et menu".into(), false, e.clone()));
            report.warnings.push(format!("intégration au système incomplète : {e}"));
            Vec::new()
        }
    };
    cleanup_legacy(env, dir, &mut report, on);

    // 6. Enregistrement de l'installation, puis fin de transaction.
    on(Event::Step(5));
    let record = Record {
        version: m.version.clone(),
        platform: m.platform.clone(),
        installed_at: now_unix(),
        options: opts.clone(),
        files: m.files.clone(),
        integration,
    };
    std::fs::write(dir.join(RECORD), serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let pointer = env.pointer_file();
    if let Some(d) = pointer.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&pointer, serde_json::json!({ "dir": dir }).to_string());
    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_file(dir.join(JOURNAL));
    on(Event::Component("Installation enregistrée".into(), true, format!("version {}", m.version)));
    Ok(report)
}

fn copy_stub(stub: &Path, dest: &Path) -> Result<(), String> {
    let bytes = std::fs::read(stub).map_err(|e| e.to_string())?;
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, crate::payload::stub_bytes(&bytes)).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    if dest.exists() && std::fs::remove_file(dest).is_err() {
        // Windows : programme de maintenance en cours d'exécution (réparation lancée depuis lui).
        let _ = std::fs::remove_file(&tmp);
        return Ok(());
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

fn integrate(env: &Env, opts: &Options, m: &crate::payload::Manifest, dir: &Path) -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        platform::windows::integrate(env, dir, &m.version, m.total_size() / 1024, opts.menu_entry, opts.desktop_shortcut, opts.autostart, opts.all_users)
    }
    #[cfg(not(windows))]
    {
        let _ = m;
        platform::linux::integrate(env, dir, opts.menu_entry, opts.desktop_shortcut, opts.autostart)
    }
}

/// Reprise d'une installation antérieure : éléments propres à l'ancien installateur.
fn cleanup_legacy(env: &Env, dir: &Path, report: &mut Report, on: &mut dyn FnMut(Event)) {
    let _ = env;
    #[cfg(windows)]
    {
        // Inno Setup : programme de désinstallation et son journal (l'entrée « Applications »
        // est remplacée par la nôtre).
        let mut removed = 0;
        for name in ["unins000.exe", "unins000.dat", "unins000.msg"] {
            if std::fs::remove_file(dir.join(name)).is_ok() {
                removed += 1;
            }
        }
        if removed > 0 {
            for all in [false, true] {
                let root = winreg::RegKey::predef(if all { winreg::enums::HKEY_LOCAL_MACHINE } else { winreg::enums::HKEY_CURRENT_USER });
                let _ = root.delete_subkey_all(platform::windows::INNO_KEY);
            }
            on(Event::Component("Ancien installateur retiré".into(), true, String::new()));
        }
    }
    let _ = (dir, report, &on);
}

/// Réparation depuis le programme de maintenance (sans charge utile) : les fichiers sont
/// vérifiés un à un ; s'ils sont intacts, raccourcis et entrées du système sont recréés.
/// Un fichier abîmé ne peut être rétabli qu'avec le programme d'installation complet.
pub fn repair_integration(record: &Record, env: &Env, on: &mut dyn FnMut(Event)) -> Result<Report, String> {
    let dir = &record.options.dir;
    let mut report = Report::default();
    on(Event::Step(0));
    recover(dir)?;
    on(Event::Step(1));
    let health = check_health(dir, &record.files);
    if !health.ok() {
        let n = health.missing.len() + health.corrupt.len();
        on(Event::Component("Intégrité des fichiers".into(), false, format!("{n} fichier(s) manquant(s) ou endommagé(s)")));
        return Err(format!(
            "{n} fichier(s) manquant(s) ou endommagé(s) ({}). Ce programme de maintenance ne contient pas les fichiers de Halcyon : \
             lancez le programme d'installation de la version {} pour les rétablir.",
            health.missing.iter().chain(&health.corrupt).take(3).cloned().collect::<Vec<_>>().join(", "),
            record.version
        ));
    }
    on(Event::Component("Intégrité des fichiers".into(), true, format!("{} vérifié(s)", record.files.len())));
    on(Event::Step(2));
    let m = crate::payload::Manifest {
        product: "Halcyon".into(),
        version: record.version.clone(),
        platform: record.platform.clone(),
        main: platform::main_exe_name().into(),
        min_glibc: None,
        files: record.files.clone(),
    };
    let integration = integrate(env, &record.options, &m, dir)?;
    on(Event::Component("Raccourcis et menu".into(), true, format!("{} élément(s)", integration.len())));
    on(Event::Step(3));
    let rec = Record { integration, ..record.clone() };
    std::fs::write(dir.join(RECORD), serde_json::to_vec_pretty(&rec).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    on(Event::Component("Installation enregistrée".into(), true, format!("version {}", rec.version)));
    report.written = 0;
    Ok(report)
}

// ------------------------------------------------------------------------ désinstallation

#[derive(Clone, Debug, Default)]
pub struct UninstallOptions {
    pub remove_cache: bool,
    /// Bibliothèque, paramètres, statistiques, copies de sauvegardes de jeux : seulement
    /// après une confirmation explicite dans l'interface.
    pub remove_data: bool,
}

/// Désinstallation : seuls les éléments enregistrés sont retirés.
pub fn uninstall(record: &Record, env: &Env, opts: &UninstallOptions, on: &mut dyn FnMut(Event)) -> Result<Report, String> {
    let dir = &record.options.dir;
    let mut report = Report::default();
    on(Event::Step(0));
    if running_from(dir) {
        return Err("Halcyon est ouvert : quittez-le (icône de la zone de notification › Quitter), puis relancez la désinstallation. Rien n'a été retiré.".into());
    }
    // Raccourcis et entrées du système.
    for p in &record.integration {
        #[cfg(not(windows))]
        let removed = platform::linux::remove_if_ours(p, dir);
        #[cfg(windows)]
        let removed = std::fs::remove_file(p).is_ok();
        if !removed && p.exists() {
            report.warnings.push(format!("{} conservé (modifié depuis l'installation)", p.display()));
        }
    }
    #[cfg(windows)]
    platform::windows::unregister(record.options.all_users);
    // Raccourcis de jeux créés par Halcyon (Linux : `halcyon-jeu-<id>.desktop`), seulement
    // s'ils lancent bien cette installation. Sous Windows, ils portent le nom du jeu et ne
    // sont pas identifiables avec certitude : ils sont conservés.
    #[cfg(not(windows))]
    {
        let mut n = 0;
        for d in [Some(env.data_home.join("applications")), env.desktop.clone()].into_iter().flatten() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("halcyon-jeu-") && name.ends_with(".desktop") && platform::linux::remove_if_ours(&e.path(), dir) {
                    n += 1;
                }
            }
        }
        if n > 0 {
            on(Event::Component("Raccourcis de jeux".into(), true, format!("{n} retiré(s)")));
        }
        platform::linux::refresh_caches(env);
    }
    on(Event::Component("Raccourcis et entrées du système".into(), true, String::new()));
    // Fichiers du programme.
    on(Event::Step(1));
    let total: u64 = record.files.iter().map(|f| f.size).sum();
    let mut done = 0;
    for f in &record.files {
        on(Event::File(f.path.clone()));
        if let Ok(p) = rel_path(dir, &f.path) {
            if std::fs::remove_file(&p).is_ok() {
                report.written += 1;
            }
            remove_empty_parents(&p, dir);
        }
        done += f.size;
        on(Event::Bytes(done, total));
    }
    let _ = std::fs::remove_file(dir.join(RECORD));
    // Programme de maintenance : sous Windows, il peut être celui en cours d'exécution
    // (la copie temporaire lancée par `--uninstall` le supprime ensuite).
    let _ = std::fs::remove_file(dir.join(platform::maintenance_exe_name()));
    let _ = std::fs::remove_dir(dir); // seulement s'il est vide : fichiers inconnus conservés
    let _ = std::fs::remove_file(env.pointer_file());
    if dir.exists() {
        report.warnings.push(format!("{} conservé : il contient des fichiers qui ne viennent pas de l'installation", dir.display()));
    }
    on(Event::Component("Fichiers de Halcyon".into(), true, format!("{} supprimé(s)", report.written)));
    // Cache et données, sur demande.
    on(Event::Step(2));
    if opts.remove_cache {
        for c in &env.user_cache {
            if c.exists() {
                std::fs::remove_dir_all(c).map_err(|e| format!("{} : {e}", c.display()))?;
            }
        }
        on(Event::Component("Cache".into(), true, "supprimé".into()));
    }
    if opts.remove_data {
        for d in &env.user_data {
            if d.exists() {
                std::fs::remove_dir_all(d).map_err(|e| format!("{} : {e}", d.display()))?;
            }
        }
        // Mode portable : données à côté de l'exécutable.
        let portable = dir.join("data");
        if portable.is_dir() {
            std::fs::remove_dir_all(&portable).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(dir.join("portable.txt"));
            let _ = std::fs::remove_dir(dir);
        }
        on(Event::Component("Données personnelles".into(), true, "supprimées".into()));
    } else {
        on(Event::Component("Données personnelles".into(), true, "conservées".into()));
    }
    Ok(report)
}

fn remove_empty_parents(file: &Path, stop: &Path) {
    let mut p = file.parent();
    while let Some(d) = p {
        if d == stop || std::fs::remove_dir(d).is_err() {
            break;
        }
        p = d.parent();
    }
}
