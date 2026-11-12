//! Lancement des jeux et ouverture de dossiers, indépendants des plateformes.

use halcyon_core::launch_args::split_args;
use halcyon_core::profiles::ProcessPriority;
use halcyon_core::LaunchTarget;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum LaunchError {
    MissingExecutable(PathBuf),
    MissingWorkingDir(PathBuf),
    Spawn(std::io::Error),
    Uri(std::io::Error),
    NotLaunchable,
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExecutable(p) => write!(f, "exécutable introuvable : {}", p.display()),
            Self::MissingWorkingDir(p) => write!(f, "répertoire de travail introuvable : {}", p.display()),
            Self::Spawn(e) => write!(f, "le jeu n'a pas pu démarrer : {e}"),
            Self::Uri(e) => write!(f, "le client de la plateforme n'a pas pu être appelé : {e}"),
            Self::NotLaunchable => write!(f, "aucun moyen de lancement n'est configuré pour ce jeu"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Résultat d'un lancement.
pub enum Launched {
    /// Processus lancé directement : sa durée peut être mesurée.
    Process(Child),
    /// Délégué au système ou au client de la plateforme : pas de suivi direct.
    Detached,
}

/// Options de lancement dépendant du système.
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// Linux/macOS : commande préfixée aux `.exe` (« wine », « umu-run », « flatpak run … »).
    pub windows_runner: String,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self { windows_runner: "wine".into() }
    }
}

/// Réglages d'un profil de lancement appliqués au processus démarré.
#[derive(Clone, Debug, Default)]
pub struct LaunchExtras {
    /// Remplace les arguments d'un exécutable (déjà fusionnés avec ceux du jeu).
    pub args: Option<Vec<String>>,
    pub env: Vec<(String, String)>,
    pub priority: Option<ProcessPriority>,
    pub working_dir: Option<PathBuf>,
}

impl LaunchExtras {
    fn is_empty(&self) -> bool {
        self.args.is_none() && self.env.is_empty() && self.priority.is_none() && self.working_dir.is_none()
    }
}

/// Lance une cible. Vérifie les chemins avant d'appeler le système.
pub fn launch(target: Option<&LaunchTarget>, opts: &LaunchOptions) -> Result<Launched, LaunchError> {
    launch_with(target, opts, &LaunchExtras::default()).map(|(l, _)| l)
}

/// Comme [`launch`], en appliquant les réglages d'un profil. Renvoie aussi les
/// réglages qui n'ont pas pu être appliqués.
pub fn launch_with(
    target: Option<&LaunchTarget>,
    opts: &LaunchOptions,
    extras: &LaunchExtras,
) -> Result<(Launched, Vec<String>), LaunchError> {
    let mut warnings = Vec::new();
    let launched = spawn_target(target, opts, extras)?;
    match &launched {
        Launched::Process(child) => {
            if let Some(p) = extras.priority {
                if let Err(e) = apply_priority_after_spawn(child, p) {
                    warnings.push(e);
                }
            }
        }
        Launched::Detached if !extras.is_empty() => {
            warnings.push("réglages du profil non applicables : le lancement est délégué au système".into());
        }
        Launched::Detached => {}
    }
    Ok((launched, warnings))
}

fn prepare(cmd: &mut Command, extras: &LaunchExtras) {
    cmd.envs(extras.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    #[cfg(windows)]
    if let Some(p) = extras.priority {
        use std::os::windows::process::CommandExt;
        let flag: u32 = match p {
            ProcessPriority::Low => 0x0000_0040,
            ProcessPriority::BelowNormal => 0x0000_4000,
            ProcessPriority::Normal => 0x0000_0020,
            ProcessPriority::AboveNormal => 0x0000_8000,
            ProcessPriority::High => 0x0000_0080,
        };
        cmd.creation_flags(flag);
    }
}

#[cfg(unix)]
fn apply_priority_after_spawn(child: &Child, p: ProcessPriority) -> Result<(), String> {
    let pid = child.id();
    // SAFETY : appels système sans pointeur, sur un PID que nous venons de créer.
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, p.nice()) };
    if rc != 0 {
        return Err(format!(
            "priorité « {} » refusée par le système (droits insuffisants pour l'augmenter)",
            p.label()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_priority_after_spawn(_child: &Child, _p: ProcessPriority) -> Result<(), String> {
    // Windows : appliquée à la création du processus (`creation_flags`).
    Ok(())
}

fn spawn_target(target: Option<&LaunchTarget>, opts: &LaunchOptions, extras: &LaunchExtras) -> Result<Launched, LaunchError> {
    match target.ok_or(LaunchError::NotLaunchable)? {
        LaunchTarget::Uri(uri) => {
            open_uri(uri).map_err(LaunchError::Uri)?;
            Ok(Launched::Detached)
        }
        LaunchTarget::Process { program, args, working_dir } => {
            // Programme nu (« legendary ») : résolu via le PATH par le système.
            if program.components().count() > 1 && !program.is_file() {
                return Err(LaunchError::MissingExecutable(program.clone()));
            }
            let mut cmd = Command::new(program);
            cmd.args(args);
            if let Some(wd) = extras.working_dir.as_ref().or(working_dir.as_ref()).filter(|d| d.is_dir()) {
                cmd.current_dir(wd);
            }
            prepare(&mut cmd, extras);
            Ok(Launched::Process(cmd.spawn().map_err(LaunchError::Spawn)?))
        }
        LaunchTarget::Executable { path, args, working_dir } => {
            if !path.is_file() {
                return Err(LaunchError::MissingExecutable(path.clone()));
            }
            let wd = extras
                .working_dir
                .clone()
                .or_else(|| working_dir.clone())
                .or_else(|| path.parent().map(Path::to_path_buf));
            if let Some(wd) = &wd {
                if !wd.is_dir() {
                    return Err(LaunchError::MissingWorkingDir(wd.clone()));
                }
            }
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let args = extras.args.clone().unwrap_or_else(|| split_args(args));
            match ext.as_str() {
                // Raccourcis : délégués au shell, qui sait les résoudre.
                "lnk" | "url" => {
                    open_path(path).map_err(LaunchError::Spawn)?;
                    Ok(Launched::Detached)
                }
                "bat" | "cmd" if cfg!(windows) => {
                    let mut cmd = Command::new("cmd");
                    cmd.arg("/C").arg(path).args(&args);
                    if let Some(wd) = &wd {
                        cmd.current_dir(wd);
                    }
                    prepare(&mut cmd, extras);
                    Ok(Launched::Process(cmd.spawn().map_err(LaunchError::Spawn)?))
                }
                // Hors Windows : les .exe passent par Wine/Proton, les scripts par sh.
                "exe" if !cfg!(windows) => {
                    let mut runner = split_args(&opts.windows_runner);
                    if runner.is_empty() {
                        runner.push("wine".into());
                    }
                    let mut cmd = Command::new(&runner[0]);
                    cmd.args(&runner[1..]).arg(path).args(&args);
                    if let Some(wd) = &wd {
                        cmd.current_dir(wd);
                    }
                    prepare(&mut cmd, extras);
                    Ok(Launched::Process(cmd.spawn().map_err(|e| {
                        LaunchError::Spawn(std::io::Error::new(
                            e.kind(),
                            format!("« {} » est requis pour lancer un jeu Windows ({e})", runner[0]),
                        ))
                    })?))
                }
                "sh" if !cfg!(windows) => {
                    let mut cmd = Command::new("sh");
                    cmd.arg(path).args(&args);
                    if let Some(wd) = &wd {
                        cmd.current_dir(wd);
                    }
                    prepare(&mut cmd, extras);
                    Ok(Launched::Process(cmd.spawn().map_err(LaunchError::Spawn)?))
                }
                _ => {
                    let mut cmd = Command::new(path);
                    cmd.args(&args);
                    if let Some(wd) = &wd {
                        cmd.current_dir(wd);
                    }
                    prepare(&mut cmd, extras);
                    Ok(Launched::Process(cmd.spawn().map_err(LaunchError::Spawn)?))
                }
            }
        }
    }
}

/// Exécute une commande du joueur (script avant/après la session) via le shell,
/// en attendant au plus `timeout`.
pub fn run_hook(command: &str, working_dir: Option<&Path>, timeout: Duration) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command).creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    if let Some(wd) = working_dir.filter(|d| d.is_dir()) {
        cmd.current_dir(wd);
    }
    let mut child = cmd.spawn().map_err(|e| format!("commande impossible à lancer : {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("la commande s'est terminée avec le code {}", status.code().unwrap_or(-1))),
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("commande interrompue après {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Démarre un programme compagnon (outil lancé avec le jeu).
pub fn spawn_companion(path: &Path, args: &str) -> Result<Child, String> {
    if !path.is_file() {
        return Err(format!("programme introuvable : {}", path.display()));
    }
    let mut cmd = Command::new(path);
    cmd.args(split_args(args));
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map_err(|e| format!("{} : {e}", path.display()))
}

/// Transmet une URI (`steam://…`) au gestionnaire de protocole du système.
pub fn open_uri(uri: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `cmd /C start` interpréterait les « & » de l'URI : on passe par le gestionnaire d'URL.
        Command::new("rundll32").args(["url.dll,FileProtocolHandler", uri]).creation_flags(CREATE_NO_WINDOW).spawn()?;
    }
    #[cfg(target_os = "macos")]
    Command::new("open").arg(uri).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(uri).spawn()?;
    Ok(())
}

/// Ouvre un fichier ou un dossier avec l'application par défaut (Explorateur…).
pub fn open_path(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        if path.is_dir() {
            Command::new("explorer").arg(path).spawn()?;
        } else {
            Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(path).spawn()?;
        }
    }
    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_paths_before_launch() {
        let o = LaunchOptions::default();
        assert!(matches!(launch(None, &o), Err(LaunchError::NotLaunchable)));
        let t = LaunchTarget::Executable { path: "/definitely/missing.exe".into(), args: String::new(), working_dir: None };
        assert!(matches!(launch(Some(&t), &o), Err(LaunchError::MissingExecutable(_))));

        let exe = std::env::current_exe().unwrap();
        let t = LaunchTarget::Executable { path: exe, args: String::new(), working_dir: Some("/missing/dir".into()) };
        assert!(matches!(launch(Some(&t), &o), Err(LaunchError::MissingWorkingDir(_))));
        let t = LaunchTarget::Process { program: "/missing/emu".into(), args: vec![], working_dir: None };
        assert!(matches!(launch(Some(&t), &o), Err(LaunchError::MissingExecutable(_))));
    }

    #[cfg(unix)]
    #[test]
    fn hooks_and_profile_extras() {
        assert!(run_hook("exit 0", None, Duration::from_secs(5)).is_ok());
        assert!(run_hook("exit 4", None, Duration::from_secs(5)).unwrap_err().contains('4'));
        assert!(run_hook("sleep 5", None, Duration::from_millis(300)).unwrap_err().contains("interrompue"));

        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out.txt");
        let t = LaunchTarget::Executable { path: "/bin/sh".into(), args: "-c \"exit 9\"".into(), working_dir: None };
        let extras = LaunchExtras {
            args: Some(vec!["-c".into(), format!("echo \"$HALCYON_TEST $(pwd)\" > {}", out.display())]),
            env: vec![("HALCYON_TEST".into(), "ok".into())],
            priority: Some(ProcessPriority::BelowNormal),
            working_dir: Some(tmp.path().to_path_buf()),
        };
        let (launched, warnings) = launch_with(Some(&t), &LaunchOptions::default(), &extras).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let Launched::Process(mut child) = launched else { panic!() };
        assert!(child.wait().unwrap().success(), "les arguments du profil remplacent ceux du jeu");
        let text = std::fs::read_to_string(&out).unwrap();
        assert_eq!(text.trim(), format!("ok {}", tmp.path().display()));
    }

    #[cfg(unix)]
    #[test]
    fn spawns_and_tracks_a_process() {
        let o = LaunchOptions::default();
        let t = LaunchTarget::Executable { path: "/bin/sh".into(), args: "-c \"exit 0\"".into(), working_dir: None };
        match launch(Some(&t), &o).unwrap() {
            Launched::Process(mut child) => assert!(child.wait().unwrap().success()),
            Launched::Detached => panic!("un exécutable doit être suivi"),
        }
        // Un .exe hors Windows passe par le lanceur configuré (ici « sh -c 'exit 3' »).
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("game.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let o = LaunchOptions { windows_runner: "sh -c \"exit 3\"".into() };
        let t = LaunchTarget::Executable { path: exe, args: String::new(), working_dir: None };
        match launch(Some(&t), &o).unwrap() {
            Launched::Process(mut child) => assert_eq!(child.wait().unwrap().code(), Some(3)),
            Launched::Detached => panic!(),
        }
        let t = LaunchTarget::Process { program: "/bin/sh".into(), args: vec!["-c".into(), "exit 0".into()], working_dir: None };
        assert!(matches!(launch(Some(&t), &o), Ok(Launched::Process(_))));
    }
}
