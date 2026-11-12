//! Détection des jeux en cours d'exécution, quand ils sont lancés par un client
//! (Steam, Epic, Heroic) et non directement par Halcyon.
//!
//! Méthode : on observe la liste des processus du système (lecture seule) et on
//! cherche ceux dont l'exécutable ou la ligne de commande se trouve dans le
//! dossier d'installation du jeu. Pour Steam, l'identifiant du jeu en cours
//! (`RunningAppID`) publié par le client est aussi utilisé. Rien n'est injecté
//! dans le jeu : compatible avec les systèmes anti-triche.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Ce qui permet de reconnaître le jeu.
#[derive(Clone, Debug, Default)]
pub struct GameProbe {
    pub install_dir: Option<PathBuf>,
    /// Steam : (dossier du client, appid).
    pub steam: Option<(PathBuf, u64)>,
}

/// Session observée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observed {
    /// Horodatage Unix du premier signe de vie du jeu.
    pub started_at: i64,
    pub duration_secs: u64,
}

pub struct Watcher {
    /// `None` si le système ne permet pas de lister les processus.
    sys: Option<System>,
    probe: GameProbe,
    dir_key: Option<String>,
}

fn normalize(s: &str) -> String {
    let mut s = s.replace('\\', "/").to_lowercase();
    // Chemins Wine/Proton : « z:/home/… ».
    if s.len() > 2 && s.as_bytes()[1] == b':' && s.starts_with('z') {
        s = s[2..].to_string();
    }
    s
}

impl Watcher {
    pub fn new(probe: GameProbe) -> Self {
        let dir_key = probe
            .install_dir
            .as_deref()
            .map(|d| normalize(&d.to_string_lossy()))
            .map(|d| d.trim_end_matches('/').to_string() + "/")
            // Un dossier trop court (« c:/ ») reconnaîtrait n'importe quoi.
            .filter(|d| d.len() > 8);
        let sys = System::new().map_err(|e| log::warn!("liste des processus indisponible : {e}")).ok();
        Self { sys, probe, dir_key }
    }

    /// Le jeu tourne-t-il en ce moment ?
    pub fn is_running(&mut self) -> bool {
        if let Some((root, appid)) = &self.probe.steam {
            if crate::steam::running_app_id(root) == Some(*appid) {
                return true;
            }
        }
        let (Some(dir), Some(sys)) = (&self.dir_key, self.sys.as_mut()) else { return false };
        let kind = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        sys.processes().values().any(|p| {
            p.exe().is_some_and(|e| normalize(&e.to_string_lossy()).starts_with(dir.as_str()))
                || p.cmd().iter().any(|a| is_program_in(&normalize(&a.to_string_lossy()), dir))
        })
    }

    /// Attend le démarrage du jeu (`start_timeout`), puis sa fin. Les courtes
    /// interruptions (relance par un lanceur intermédiaire) sont tolérées.
    pub fn wait_session(&mut self, start_timeout: Duration, cancel: &AtomicBool) -> Option<Observed> {
        self.wait_session_with(start_timeout, cancel, || {})
    }

    /// Comme `wait_session`, avec `on_start` appelé dès que le jeu est vu en cours d'exécution.
    pub fn wait_session_with(&mut self, start_timeout: Duration, cancel: &AtomicBool, on_start: impl FnOnce()) -> Option<Observed> {
        let deadline = Instant::now() + start_timeout;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            if self.is_running() {
                break;
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_secs(3));
        }
        on_start();
        let started = Instant::now();
        let started_at = unix_now();
        let mut last_seen = Instant::now();
        let mut misses = 0;
        while misses < 3 && !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(5));
            if self.is_running() {
                misses = 0;
                last_seen = Instant::now();
            } else {
                misses += 1;
            }
        }
        Some(Observed { started_at, duration_secs: last_seen.duration_since(started).as_secs() })
    }
}

/// Argument désignant un programme du dossier (et non un simple fichier ouvert par
/// un éditeur) : `…/Jeu/Game.exe` (Wine/Proton), `…/Jeu/start.sh`.
fn is_program_in(arg: &str, dir: &str) -> bool {
    let Some(rest) = arg.strip_prefix(dir) else { return false };
    let file = rest.rsplit('/').next().unwrap_or(rest);
    match file.rsplit_once('.') {
        Some((_, ext)) => matches!(ext, "exe" | "sh" | "x86_64" | "x86" | "appimage" | "bin" | "bat" | "jar" | "py"),
        None => !file.is_empty(),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Termine un processus démarré par Halcyon (programmes compagnons).
pub fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Le dossier `install_dir` contient-il l'exécutable d'un processus en cours ?
pub fn any_running_in(install_dir: &Path) -> bool {
    Watcher::new(GameProbe { install_dir: Some(install_dir.to_path_buf()), steam: None }).is_running()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_programs_count() {
        let dir = "/home/a/games/hades/";
        assert!(is_program_in("/home/a/games/hades/hades.exe", dir));
        assert!(is_program_in("/home/a/games/hades/bin/hades", dir));
        assert!(!is_program_in("/home/a/games/hades/config.ini", dir), "éditeur ouvrant un fichier");
        assert!(!is_program_in("vim /home/a/games/hades/hades.exe", dir));
    }

    #[test]
    fn normalizes_wine_paths() {
        assert_eq!(normalize("Z:\\home\\sebi\\Games\\Hades\\Hades.exe"), "/home/sebi/games/hades/hades.exe");
        assert_eq!(normalize("C:\\Games\\X"), "c:/games/x");
    }

    #[cfg(unix)]
    #[test]
    fn detects_a_running_process_by_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("MonJeuDeTest");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("game.sh");
        std::fs::write(&script, "sleep 4\n").unwrap();
        let mut w = Watcher::new(GameProbe { install_dir: Some(dir.clone()), steam: None });
        assert!(!w.is_running());
        let mut child = std::process::Command::new("sh").arg(&script).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(w.is_running(), "processus reconnu par sa ligne de commande");
        terminate(&mut child);
        std::thread::sleep(Duration::from_millis(200));
        assert!(!w.is_running());
    }
}
