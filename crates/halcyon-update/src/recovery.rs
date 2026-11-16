//! État persistant, journal d'opérations, verrou et contrôle de santé.
//!
//! Tout ce qui concerne une installation vit dans son dossier `.hus/` (même disque que les
//! fichiers installés : les renommages y sont atomiques) :
//!
//! | Élément | Rôle |
//! |---|---|
//! | `state.json` | état courant (écriture atomique), versions, message, plancher de sécurité |
//! | `hus.log` | journal lisible de chaque opération |
//! | `pending/` | paquet vérifié prêt à installer, avec son manifeste signé |
//! | `staging/` | nouveaux fichiers extraits et vérifiés |
//! | `backup/` | anciens fichiers pendant une installation |
//! | `transaction.json` | journal de transaction (retour arrière après coupure) |
//! | `previous/` | version précédente conservée après succès |
//! | `health/` | preuves de démarrage de la nouvelle version |
//! | `runner/` | copie de l'updater en cours d'exécution |
//! | `downloads/` | téléchargements en cours (reprenables) |

use crate::core::State;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Emplacements d'une installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Layout {
        Layout { root: root.into() }
    }
    pub fn hus(&self) -> PathBuf {
        self.root.join(".hus")
    }
    pub fn state_file(&self) -> PathBuf {
        self.hus().join("state.json")
    }
    pub fn log_file(&self) -> PathBuf {
        self.hus().join("hus.log")
    }
    pub fn pending(&self) -> PathBuf {
        self.hus().join("pending")
    }
    pub fn staging(&self) -> PathBuf {
        self.hus().join("staging")
    }
    pub fn backup(&self) -> PathBuf {
        self.hus().join("backup")
    }
    pub fn journal(&self) -> PathBuf {
        self.hus().join("transaction.json")
    }
    pub fn previous(&self) -> PathBuf {
        self.hus().join("previous")
    }
    pub fn health(&self) -> PathBuf {
        self.hus().join("health")
    }
    pub fn runner(&self) -> PathBuf {
        self.hus().join("runner")
    }
    pub fn downloads(&self) -> PathBuf {
        self.hus().join("downloads")
    }
    pub fn lock(&self) -> PathBuf {
        self.hus().join("updater.lock")
    }
    pub fn installed(&self) -> PathBuf {
        self.root.join(INSTALLED)
    }
}

/// Liste des fichiers posés par HUS (permet de retirer ceux qu'une version abandonne, sans
/// jamais toucher un fichier inconnu).
pub const INSTALLED: &str = ".hus-installed.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    pub files: Vec<String>,
}

pub fn read_installed(layout: &Layout) -> Option<Installed> {
    std::fs::read(layout.installed()).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// État persistant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub state: State,
    #[serde(default)]
    pub txid: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub message: String,
    /// Horodatage Unix de la dernière modification.
    #[serde(default)]
    pub updated: u64,
    /// Version la plus haute jamais installée ici : plancher anti-retour.
    #[serde(default)]
    pub highest: Option<String>,
    /// Plancher de sécurité annoncé par le dernier manifeste installé.
    #[serde(default)]
    pub security_floor: Option<String>,
    /// Version dont l'installation a échoué (pas de nouvel essai automatique).
    #[serde(default)]
    pub blocked: Option<String>,
    /// Résultat déjà signalé à l'utilisateur.
    #[serde(default)]
    pub acknowledged: bool,
}

impl Default for Record {
    fn default() -> Self {
        Record { state: State::Idle, txid: String::new(), from: String::new(), to: String::new(), message: String::new(), updated: 0, highest: None, security_floor: None, blocked: None, acknowledged: true }
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Écriture atomique : fichier temporaire synchronisé, puis renommage.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    if let Some(d) = path.parent() {
        if let Ok(dir) = std::fs::File::open(d) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

pub fn load(layout: &Layout) -> Record {
    std::fs::read(layout.state_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(layout: &Layout, record: &Record) -> Result<(), String> {
    let mut r = record.clone();
    r.updated = now();
    write_atomic(&layout.state_file(), &serde_json::to_vec_pretty(&r).unwrap_or_default()).map_err(|e| format!("état de mise à jour : {e}"))
}

/// Change d'état (et journalise).
pub fn set_state(layout: &Layout, state: State, message: &str) -> Result<Record, String> {
    let mut r = load(layout);
    r.state = state;
    r.message = message.to_string();
    if matches!(state, State::Completed | State::RolledBack | State::Failed) {
        r.acknowledged = false;
    }
    save(layout, &r)?;
    log(layout, &format!("état {state:?}{}", if message.is_empty() { String::new() } else { format!(" : {message}") }));
    Ok(r)
}

/// Ligne horodatée dans `.hus/hus.log` (taille bornée : le journal est tronqué à 1 Mo).
pub fn log(layout: &Layout, line: &str) {
    let path = layout.log_file();
    let _ = std::fs::create_dir_all(layout.hus());
    if std::fs::metadata(&path).map(|m| m.len() > 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} [{}] {line}", now(), std::process::id());
    }
}

// ------------------------------------------------------------------ processus et verrou

/// Le processus `pid` est-il encore vivant ? (Un processus zombie est considéré terminé.)
pub fn pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};
    let Ok(mut sys) = System::new() else {
        // Sans liste des processus, on ne présume pas qu'un processus est terminé.
        return true;
    };
    let p = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
    sys.process(p).is_some_and(|proc_| !matches!(proc_.status(), ProcessStatus::Zombie | ProcessStatus::Dead))
}

/// Attend la fin de `pid` (au plus `timeout`).
pub fn wait_pid(pid: u32, timeout: std::time::Duration) -> bool {
    let until = std::time::Instant::now() + timeout;
    while pid_alive(pid) {
        if std::time::Instant::now() >= until {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    true
}

/// Verrou exclusif de l'updater (libéré à la fin, ou repris s'il appartient à un processus mort).
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    pub fn acquire(layout: &Layout) -> Result<Lock, String> {
        let path = layout.lock();
        let _ = std::fs::create_dir_all(layout.hus());
        for _ in 0..2 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = write!(f, "{}", std::process::id());
                    let _ = f.sync_all();
                    return Ok(Lock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let owner = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse::<u32>().ok());
                    match owner {
                        Some(pid) if pid != std::process::id() && pid_alive(pid) => return Err("une mise à jour est déjà en cours".into()),
                        _ => {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
                Err(e) => return Err(format!("verrou de mise à jour : {e}")),
            }
        }
        Err("verrou de mise à jour indisponible".into())
    }

    /// Un updater vivant tient-il le verrou ?
    pub fn held(layout: &Layout) -> bool {
        std::fs::read_to_string(layout.lock()).ok().and_then(|s| s.trim().parse::<u32>().ok()).is_some_and(|pid| pid != std::process::id() && pid_alive(pid))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ------------------------------------------------------------------ contrôle de santé

/// Variable d'environnement transmise à la nouvelle version : dossier de l'installation.
pub const ENV_ROOT: &str = "HUS_ROOT";
/// Argument transmis à la nouvelle version : identifiant de transaction à confirmer.
pub const ARG_HEALTH: &str = "--hus-health";
/// Argument transmis à l'ancienne version relancée après un retour arrière.
pub const ARG_ROLLED_BACK: &str = "--hus-rolled-back";

/// Appelée par Halcyon une fois sa fenêtre affichée et sa boucle d'évènements active.
pub fn confirm_health(layout: &Layout, txid: &str) -> Result<(), String> {
    if txid.is_empty() || !txid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("identifiant de transaction invalide".into());
    }
    write_atomic(&layout.health().join(format!("{txid}.ok")), b"ok").map_err(|e| format!("contrôle de santé : {e}"))?;
    log(layout, &format!("santé confirmée pour la transaction {txid}"));
    Ok(())
}

pub fn health_confirmed(layout: &Layout, txid: &str) -> bool {
    layout.health().join(format!("{txid}.ok")).is_file()
}

/// Identifiant de transaction (unique, lisible).
pub fn new_txid() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("{}-{}-{nanos:08x}", now(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trip_and_lock() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path());
        assert_eq!(load(&l).state, State::Idle);
        let r = set_state(&l, State::Failed, "réseau").unwrap();
        assert!(!r.acknowledged);
        assert_eq!(load(&l).message, "réseau");
        assert!(std::fs::read_to_string(l.log_file()).unwrap().contains("Failed"));
        let lock = Lock::acquire(&l).unwrap();
        assert!(l.lock().is_file());
        drop(lock);
        assert!(!l.lock().exists());
        // Verrou d'un processus disparu : repris.
        std::fs::write(l.lock(), "4000000000").unwrap();
        assert!(Lock::acquire(&l).is_ok());
        assert!(pid_alive(std::process::id()));
        assert!(confirm_health(&l, "../x").is_err());
        confirm_health(&l, "abc-1").unwrap();
        assert!(health_confirmed(&l, "abc-1"));
    }
}
