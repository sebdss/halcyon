//! Déroulement complet côté updater (`halcyon-updater apply` / `recover`) et décisions du
//! lanceur au démarrage (contrôle de santé, reprise, installation en attente).

use crate::core::{Platform, State};
use crate::install::{self, Target};
use crate::recovery::{self, log, Layout, Lock, ARG_HEALTH, ARG_ROLLED_BACK, ENV_ROOT};
use crate::security::PublicKey;
use crate::version::Version;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Première version de l'updater compatible avec le protocole actuel (format des paquets,
/// `pending/`, journal) : valeur par défaut de `min_updater` dans les manifestes. À relever
/// seulement quand un changement exige le nouvel updater (Halcyon l'extrait alors du paquet).
pub const PROTOCOL_MIN: &str = "1.0.0-alpha.1";

/// Version de l'updater (comparée à `min_updater` des manifestes).
pub fn updater_version() -> Version {
    Version::current()
}

/// Lance un programme détaché du processus courant (il survit à la fin de l'updater ou du
/// lanceur), sans console.
pub fn spawn_detached(exe: &Path, args: &[String], root: &Path) -> std::io::Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.args(args).env(ENV_ROOT, root).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd.spawn()
}

/// Attend la preuve de démarrage de la nouvelle version. Échec si elle s'arrête sans l'avoir
/// donnée, ou si le délai est dépassé (elle est alors arrêtée).
pub fn await_health(layout: &Layout, txid: &str, child: &mut Child, timeout: Duration) -> Result<(), String> {
    let until = Instant::now() + timeout;
    loop {
        if recovery::health_confirmed(layout, txid) {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            // Dernière chance : la preuve a pu être écrite juste avant la sortie.
            if recovery::health_confirmed(layout, txid) {
                return Ok(());
            }
            return Err(format!("la nouvelle version s'est arrêtée au démarrage ({status})"));
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("la nouvelle version n'a pas confirmé son démarrage en {} s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[derive(Clone, Debug)]
pub struct ApplyOptions {
    pub root: PathBuf,
    /// Processus du lanceur à attendre avant de toucher aux fichiers.
    pub wait_pid: Option<u32>,
    /// Version installée annoncée par le lanceur (contrôlée par la fiche HUS si elle existe).
    pub current: Version,
    pub health_timeout: Duration,
    pub keys: Vec<PublicKey>,
    /// Relancer Halcyon à la fin (toujours vrai, sauf pour certains tests).
    pub relaunch: bool,
}

/// Relance l'installation présente (après un échec ou un retour arrière).
fn relaunch_current(layout: &Layout, target: &Target, args: &[String]) {
    let exe = target.main_exe(&layout.root);
    match spawn_detached(&exe, args, &layout.root) {
        Ok(_) => log(layout, &format!("Halcyon relancé ({})", exe.display())),
        Err(e) => log(layout, &format!("relance impossible de {} : {e}", exe.display())),
    }
}

fn fail(layout: &Layout, state: State, message: &str, blocked: Option<&str>) {
    let mut r = recovery::load(layout);
    r.state = state;
    r.message = message.to_string();
    r.acknowledged = false;
    if let Some(b) = blocked {
        r.blocked = Some(b.to_string());
    }
    let _ = recovery::save(layout, &r);
    log(layout, &format!("état {state:?} : {message}"));
}

/// `halcyon-updater apply` : installation du paquet en attente, contrôle de santé, et retour
/// arrière en cas d'échec. Renvoie l'état final.
pub fn apply(opts: &ApplyOptions) -> Result<State, String> {
    let layout = Layout::new(&opts.root);
    let _lock = Lock::acquire(&layout)?;
    log(&layout, &format!("updater {} : installation demandée (version annoncée {})", updater_version(), opts.current));
    let pending = install::read_pending(&layout).ok_or("aucune mise à jour en attente")?;
    let target = pending.target.clone();

    if let Some(pid) = opts.wait_pid {
        if !recovery::wait_pid(pid, Duration::from_secs(90)) {
            let msg = "Halcyon ne s'est pas fermé : mise à jour reportée, rien n'a été modifié";
            fail(&layout, State::Failed, msg, None);
            return Err(msg.into());
        }
    }
    if install::interrupted(&layout) {
        recovery::set_state(&layout, State::Recovery, "transaction précédente interrompue")?;
        install::rollback(&layout)?;
    }
    // Plateforme du paquet : celle de l'installation visée (ZIP : Windows, AppImage : Linux),
    // qui doit être celle de ce système (les tests seuls installent un ZIP sous Linux).
    let platform = match target.format() {
        crate::core::PackageFormat::Zip => Platform::WindowsX64,
        crate::core::PackageFormat::Appimage => Platform::LinuxX64,
    };
    if !cfg!(feature = "fault-injection") && Platform::current() != Some(platform) {
        let msg = "paquet en attente destiné à un autre système";
        install::discard_pending(&layout);
        fail(&layout, State::Failed, msg, None);
        return Err(msg.into());
    }
    recovery::set_state(&layout, State::Verifying, &format!("revérification de {}", pending.version))?;
    let verified = match install::verify_pending(&layout, &opts.keys, platform) {
        Ok(v) => v,
        Err(e) => {
            install::discard_pending(&layout);
            fail(&layout, State::Failed, &format!("paquet refusé : {e}"), Some(&pending.version));
            if opts.relaunch {
                relaunch_current(&layout, &target, &[]);
            }
            return Err(e);
        }
    };
    if verified.checked.min_updater > updater_version() {
        let msg = format!("cet updater ({}) est trop ancien pour {} (minimum {})", updater_version(), verified.checked.version, verified.checked.min_updater);
        fail(&layout, State::Failed, &msg, None);
        if opts.relaunch {
            relaunch_current(&layout, &target, &[]);
        }
        return Err(msg);
    }
    let installed = recovery::read_installed(&layout).and_then(|i| i.version.parse::<Version>().ok()).map(|v| v.max(opts.current.clone())).unwrap_or_else(|| opts.current.clone());
    let to = verified.checked.version.to_string();
    let txid = match install::install(&layout, &verified, &installed) {
        Ok(t) => t,
        Err(e) => {
            install::discard_pending(&layout);
            fail(&layout, State::Failed, &format!("installation de {to} impossible : {e}. La version {installed} est conservée."), Some(&to));
            if opts.relaunch {
                relaunch_current(&layout, &target, &[ARG_ROLLED_BACK.into()]);
            }
            return Ok(State::Failed);
        }
    };
    install::fault(&layout, "before-health");
    let exe = target.main_exe(&layout.root);
    let health = spawn_detached(&exe, &[ARG_HEALTH.into(), txid.clone()], &layout.root)
        .map_err(|e| format!("démarrage de {} impossible : {e}", exe.display()))
        .and_then(|mut child| await_health(&layout, &txid, &mut child, opts.health_timeout));
    match health {
        Ok(()) => {
            install::finalize(&layout, &txid)?;
            Ok(State::Completed)
        }
        Err(e) => {
            log(&layout, &format!("contrôle de santé échoué : {e}"));
            recovery::set_state(&layout, State::Recovery, &e)?;
            install::rollback(&layout)?;
            install::discard_pending(&layout);
            fail(&layout, State::RolledBack, &format!("Halcyon {to} n'a pas démarré correctement ({e}). La version {installed} a été restaurée."), Some(&to));
            if opts.relaunch {
                relaunch_current(&layout, &target, &[ARG_ROLLED_BACK.into()]);
            }
            Ok(State::RolledBack)
        }
    }
}

/// `halcyon-updater recover` : termine proprement une transaction interrompue (coupure,
/// arrêt brutal) en restaurant l'ancienne installation, puis relance Halcyon.
pub fn recover(root: &Path, wait_pid: Option<u32>, target: &Target, relaunch: bool) -> Result<State, String> {
    let layout = Layout::new(root);
    let _lock = Lock::acquire(&layout)?;
    if let Some(pid) = wait_pid {
        if !recovery::wait_pid(pid, Duration::from_secs(90)) {
            return Err("Halcyon ne s'est pas fermé".into());
        }
    }
    let state = if install::interrupted(&layout) {
        let r = recovery::load(&layout);
        recovery::set_state(&layout, State::Recovery, "reprise après interruption")?;
        install::rollback(&layout)?;
        install::discard_pending(&layout);
        let to = r.to.clone();
        fail(&layout, State::RolledBack, &format!("L'installation de Halcyon {to} a été interrompue ; la version {} a été restaurée.", r.from), (!to.is_empty()).then_some(to.as_str()));
        State::RolledBack
    } else {
        recovery::load(&layout).state
    };
    if relaunch {
        let args: Vec<String> = if state == State::RolledBack { vec![ARG_ROLLED_BACK.into()] } else { Vec::new() };
        relaunch_current(&layout, target, &args);
    }
    Ok(state)
}

/// Ce que le lanceur doit faire au démarrage.
#[derive(Clone, Debug, PartialEq)]
pub enum Startup {
    /// Rien de particulier.
    Normal,
    /// Lancé pour le contrôle de santé : preuve écrite, l'updater conclut.
    HealthConfirmed,
    /// Transaction confirmée par ce démarrage (l'updater avait été arrêté).
    Finalized,
    /// Transaction interrompue : lancer `halcyon-updater recover` puis quitter.
    NeedsRecovery,
}

/// Décision au démarrage du lanceur. `health` : valeur de `--hus-health`.
pub fn on_launcher_start(layout: &Layout, health: Option<&str>) -> Startup {
    if let Some(tx) = health {
        if recovery::confirm_health(layout, tx).is_ok() {
            if Lock::held(layout) {
                return Startup::HealthConfirmed;
            }
            // L'updater a disparu entre-temps : ce démarrage réussi conclut la transaction.
            if install::finalize(layout, tx).is_ok() {
                return Startup::Finalized;
            }
            return Startup::HealthConfirmed;
        }
    }
    if !install::interrupted(layout) || Lock::held(layout) {
        return Startup::Normal;
    }
    let r = recovery::load(layout);
    if r.state == State::HealthCheck && !r.txid.is_empty() {
        // Nouvelle version en place et qui démarre : la transaction est confirmée.
        let _ = recovery::confirm_health(layout, &r.txid);
        if install::finalize(layout, &r.txid).is_ok() {
            return Startup::Finalized;
        }
    }
    Startup::NeedsRecovery
}

/// Copie l'updater dans `.hus/runner` et renvoie le chemin de la copie : l'updater installé
/// peut alors être remplacé par la nouvelle version pendant qu'il travaille.
pub fn prepare_runner(layout: &Layout, updater: &Path) -> Result<PathBuf, String> {
    let dir = layout.runner();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Anciennes copies : supprimées si elles ne sont plus en cours d'exécution.
    if let Ok(list) = std::fs::read_dir(&dir) {
        for e in list.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let name = format!("halcyon-updater-{}{}", std::process::id(), std::env::consts::EXE_SUFFIX);
    let dest = dir.join(name);
    std::fs::copy(updater, &dest).map_err(|e| format!("copie de l'updater : {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Updater contenu dans un paquet vérifié (quand l'updater installé est trop ancien pour
/// `min_updater`) : extrait du ZIP, ou de l'AppImage par `--appimage-extract`.
pub fn updater_from_package(layout: &Layout, package: &Path, target: &Target) -> Result<PathBuf, String> {
    let dir = layout.runner().join("bootstrap");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    match target {
        Target::Zip => {
            let file = std::fs::File::open(package).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut entry = zip.by_name(crate::security::WINDOWS_UPDATER).map_err(|e| format!("updater absent du paquet : {e}"))?;
            let dest = dir.join(crate::security::WINDOWS_UPDATER);
            let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            Ok(dest)
        }
        _ => {
            let status = Command::new(package)
                .args(["--appimage-extract", "usr/bin/halcyon-updater"])
                .current_dir(&dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| format!("extraction de l'updater : {e}"))?;
            let p = dir.join("squashfs-root/usr/bin/halcyon-updater");
            if !status.success() || !p.is_file() {
                return Err("updater absent de l'AppImage".into());
            }
            Ok(p)
        }
    }
}
