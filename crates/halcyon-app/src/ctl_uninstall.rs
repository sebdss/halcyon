//! Désinstallation des jeux (1.0.0-alpha.6).
//!
//! Halcyon ne remplace jamais les plateformes : Steam (`steam://uninstall`), Legendary
//! (`legendary uninstall`), les autres launchers ouvrent leur page. Seul le dossier d'un jeu
//! autonome détecté dans un dossier de jeux enregistré est supprimé par Halcyon, vers la
//! corbeille, après les garde-fous de `halcyon_core::uninstall::check_deletable`. Les ROM et
//! les jeux ajoutés à la main ne sont jamais supprimés.
//!
//! Toujours une confirmation explicite (nom, taille, chemin, « Sauvegarder avant »), au
//! clavier, à la souris comme à la manette (le focus part sur « Annuler »). Chaque
//! désinstallation est inscrite au journal d'activité et au journal de sécurité. Le jeu
//! reste dans la bibliothèque, « Non installé » : favoris, temps de jeu et collections sont
//! conservés.

use crate::controller::{post, Controller};
use crate::UninstallUi;
use halcyon_core::uninstall::{self, UnavailableReason, UninstallPlan};
use halcyon_core::{GameId, GameSource};
use halcyon_storage::{audit, ActivityKind};
use slint::ComponentHandle;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Durée maximale laissée à `legendary uninstall`.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct UninstallState {
    game: Option<GameId>,
    plan: Option<UninstallPlan>,
    busy: bool,
}

/// Issue d'une désinstallation (journalisée et affichée).
enum Outcome {
    Done,
    Delegated(String),
    Failed(String),
}

impl Controller {
    pub(crate) fn wire_uninstall(self: &Rc<Self>, app: &crate::AppWindow) {
        let weak = Rc::downgrade(self);
        let ui = app.global::<UninstallUi>();
        let w = weak.clone();
        ui.on_confirm(move || {
            if let Some(c) = w.upgrade() {
                c.uninstall_confirm();
            }
        });
        let w = weak.clone();
        ui.on_cancel(move || {
            if let Some(c) = w.upgrade() {
                c.uninstall_close();
            }
        });
        let w = weak.clone();
        ui.on_key(move |k| {
            if let Some(c) = w.upgrade() {
                c.uninstall_key(&k);
            }
        });
        ui.on_request(move |id| {
            if let Some(c) = weak.upgrade() {
                c.uninstall_request(GameId::from(id));
            }
        });
    }

    pub(crate) fn uninstall_open(&self) -> bool {
        self.app().is_some_and(|a| a.global::<UninstallUi>().get_open())
    }

    /// Dossiers jamais supprimés, ni eux ni leurs parents.
    fn protected_dirs(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = vec![self.paths.data_dir.clone(), self.paths.cache_dir.clone(), self.paths.logs_dir.clone()];
        v.extend(dirs::home_dir());
        v.extend(dirs::document_dir());
        v.extend(dirs::desktop_dir());
        v.extend(dirs::download_dir());
        v.extend(dirs::data_dir());
        v.extend(dirs::config_dir());
        v.extend(dirs::cache_dir());
        if cfg!(windows) {
            for var in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "APPDATA", "LOCALAPPDATA"] {
                v.extend(std::env::var_os(var).map(PathBuf::from));
            }
        } else {
            for p in ["/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/opt", "/proc", "/root", "/sbin", "/sys", "/usr", "/var", "/home", "/media", "/mnt", "/run", "/srv", "/tmp"] {
                v.push(PathBuf::from(p));
            }
        }
        let s = self.settings.borrow();
        v.extend(s.steam_path.clone());
        v.extend(steam_root());
        v
    }

    /// Dossiers d'autres jeux, bibliothèques de ROM et dépôts de sauvegardes.
    fn other_dirs(&self, except: GameId) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.lib.borrow().games.iter().filter(|g| g.id != except).filter_map(|g| g.install_dir.clone()).collect();
        let s = self.settings.borrow();
        v.extend(s.rom_folders.iter().map(|r| r.path.clone()));
        v.extend(s.save_root.clone());
        v.extend(s.save_mirrors.iter().cloned());
        v
    }

    /// Ouvre la confirmation de désinstallation d'un jeu.
    pub(crate) fn uninstall_request(&self, id: GameId) {
        let Some(app) = self.app() else { return };
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        if self.active_session.get() == Some(id) {
            return self.toast(tr!("Fermez le jeu avant de le désinstaller"), true);
        }
        let plan = uninstall::plan(&g, cfg!(windows));
        let ui = app.global::<UninstallUi>();
        let size = g.size_bytes.map(halcyon_core::util::format_size).unwrap_or_default();
        let path = g.install_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        let mut refusal = String::new();
        let (method, confirm, danger) = match &plan {
            UninstallPlan::Unavailable(UnavailableReason::NotInstalled) => {
                return self.toast(tr!("« {name} » n'est pas installé", name = g.name), false);
            }
            UninstallPlan::Unavailable(UnavailableReason::Minecraft) => {
                return self.toast(tr!("Les installations de Minecraft se gèrent dans l'onglet Minecraft"), false);
            }
            UninstallPlan::Unavailable(UnavailableReason::NoInstallFolder) => {
                return self.toast(tr!("Dossier d'installation inconnu : désinstallez « {name} » à la main", name = g.name), true);
            }
            UninstallPlan::Uri(_) => (tr!("Steam s'ouvre et vous demande confirmation : c'est Steam qui désinstalle le jeu."), tr!("Désinstaller avec Steam"), false),
            UninstallPlan::Command { .. } => (tr!("Legendary désinstalle le jeu (commande « legendary uninstall »)."), tr!("Désinstaller"), true),
            UninstallPlan::Delegate { uri, launcher } => (
                if uri.is_some() {
                    tr!("La désinstallation se fait dans {launcher} : Halcyon ouvre sa page, terminez-y l'opération.", launcher = launcher)
                } else {
                    tr!("La désinstallation se fait dans {launcher} : ouvrez-le pour terminer l'opération.", launcher = launcher)
                },
                tr!("Ouvrir {launcher}", launcher = launcher),
                false,
            ),
            UninstallPlan::DeleteFolder(dir) => {
                let registered = self.settings.borrow().scan_folders.clone();
                let check = uninstall::check_deletable(dir, &registered, &self.protected_dirs(), &self.other_dirs(id));
                if let Err(r) = check {
                    refusal = tr!("Suppression refusée : {reason}.", reason = r.describe());
                } else if std::fs::symlink_metadata(dir).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                    refusal = tr!("Suppression refusée : le dossier est un lien symbolique.");
                } else if !dir.is_dir() {
                    refusal = tr!("Dossier introuvable : le jeu a peut-être déjà été supprimé.");
                }
                (tr!("Le dossier du jeu est envoyé à la corbeille : vous pourrez le récupérer tant qu'elle n'est pas vidée."), tr!("Mettre à la corbeille"), true)
            }
            UninstallPlan::RemoveFromLibrary => (
                tr!("Jeu ajouté à la main : il est seulement retiré de la bibliothèque. Aucun fichier n'est supprimé."),
                tr!("Retirer de la bibliothèque"),
                false,
            ),
            UninstallPlan::HideFromLibrary => (
                tr!("ROM : elle est seulement masquée de la bibliothèque (« Jeux masqués » pour la retrouver). Le fichier n'est jamais supprimé."),
                tr!("Masquer de la bibliothèque"),
                false,
            ),
        };
        let can_backup = !matches!(plan, UninstallPlan::RemoveFromLibrary | UninstallPlan::HideFromLibrary) && self.save_job_for(&g).is_some();
        ui.set_title(g.name.as_str().into());
        ui.set_method(method.into());
        ui.set_size(size.into());
        ui.set_path(path.into());
        ui.set_can_backup(can_backup);
        ui.set_backup(can_backup);
        ui.set_backup_note(
            if can_backup {
                tr!("Copie des sauvegardes du jeu dans le dépôt de Halcyon, avant tout le reste")
            } else {
                tr!("Aucun emplacement de sauvegarde associé à ce jeu")
            }
            .into(),
        );
        ui.set_confirm_label(confirm.into());
        ui.set_danger(danger);
        ui.set_refusal(refusal.into());
        ui.set_result("".into());
        ui.set_phase(0);
        ui.set_focus(1);
        ui.set_console(self.bp.borrow().active);
        ui.set_open(true);
        *self.uninstall.borrow_mut() = UninstallState { game: Some(id), plan: Some(plan.clone()), busy: false };
        // Taille inconnue d'un jeu autonome : calculée sur un fil.
        if let (UninstallPlan::DeleteFolder(dir), None) = (&plan, g.size_bytes) {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let size = halcyon_integrations::trash::dir_size(&dir);
                post(move |c| {
                    if c.uninstall.borrow().game == Some(id) {
                        if let Some(app) = c.app() {
                            app.global::<UninstallUi>().set_size(halcyon_core::util::format_size(size).into());
                        }
                    }
                });
            });
        }
    }

    /// Clavier et manette dans le dialogue : précédent / suivant / valider / retour.
    pub(crate) fn uninstall_key(self: &Rc<Self>, key: &str) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<UninstallUi>();
        let phase = ui.get_phase();
        if phase == 1 {
            return;
        }
        if phase == 2 {
            if matches!(key, "accept" | "back") {
                self.uninstall_close();
            }
            return;
        }
        let first = if ui.get_can_backup() { 0 } else { 1 };
        let last = if ui.get_refusal().is_empty() { 2 } else { 1 };
        let f = ui.get_focus();
        match key {
            "prev" => ui.set_focus((f - 1).max(first)),
            "next" => ui.set_focus((f + 1).min(last)),
            "back" => self.uninstall_close(),
            "accept" => match f {
                0 => ui.set_backup(!ui.get_backup()),
                2 => self.uninstall_confirm(),
                _ => self.uninstall_close(),
            },
            _ => {}
        }
    }

    pub(crate) fn uninstall_close(&self) {
        if self.uninstall.borrow().busy {
            return;
        }
        let was_confirming = self.app().map(|a| a.global::<UninstallUi>().get_phase() == 0).unwrap_or(false);
        let game = self.uninstall.borrow().game;
        if let (true, Some(id)) = (was_confirming, game) {
            let name = self.lib.borrow().get(id).map(|g| g.name.clone()).unwrap_or_default();
            log::info!("désinstallation annulée : {name}");
            let _ = self.db().log_activity(ActivityKind::Uninstalled, Some(id), &tr_later!("Désinstallation de « {name} » annulée", name = name), "");
        }
        *self.uninstall.borrow_mut() = UninstallState::default();
        if let Some(app) = self.app() {
            app.global::<UninstallUi>().set_open(false);
        }
        if self.bp.borrow().active {
            self.refresh_bp();
        }
    }

    fn uninstall_confirm(self: &Rc<Self>) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<UninstallUi>();
        if ui.get_phase() != 0 || !ui.get_refusal().is_empty() {
            return;
        }
        let (Some(id), Some(plan)) = ({
            let st = self.uninstall.borrow();
            (st.game, st.plan.clone())
        }) else {
            return;
        };
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        // Actions instantanées sur la bibliothèque (aucun fichier touché).
        match &plan {
            UninstallPlan::RemoveFromLibrary => {
                self.remove_game(id);
                self.uninstall_finish(id, &g.name, &plan, Outcome::Done);
                return;
            }
            UninstallPlan::HideFromLibrary => {
                if !g.hidden {
                    self.toggle_hidden(id);
                }
                self.uninstall_finish(id, &g.name, &plan, Outcome::Done);
                return;
            }
            _ => {}
        }
        let backup = if ui.get_backup() && ui.get_can_backup() { self.save_job_for(&g) } else { None };
        ui.set_phase(1);
        self.uninstall.borrow_mut().busy = true;
        let db = self.db.clone();
        let name = g.name.clone();
        std::thread::Builder::new()
            .name("halcyon-uninstall".into())
            .spawn(move || {
                // Sauvegarde d'abord : si elle échoue, rien n'est désinstallé.
                if let Some(job) = backup {
                    let note = tr!("Avant la désinstallation");
                    if let Err(e) = job.run(&db, "manual", &note, true) {
                        let msg = tr!("Sauvegarde impossible ({e}) : désinstallation annulée, rien n'a été supprimé.", e = e);
                        post(move |c| c.uninstall_finish(id, &name, &plan, Outcome::Failed(msg)));
                        return;
                    }
                }
                let outcome = run_plan(&plan);
                post(move |c| c.uninstall_finish(id, &name, &plan, outcome));
            })
            .ok();
    }

    /// Résultat : base, bibliothèque, journaux, message (dans le dialogue, lisible aussi en
    /// mode console).
    fn uninstall_finish(&self, id: GameId, name: &str, plan: &UninstallPlan, outcome: Outcome) {
        self.uninstall.borrow_mut().busy = false;
        let how = match plan {
            UninstallPlan::Uri(_) => "steam",
            UninstallPlan::Command { .. } => "legendary",
            UninstallPlan::Delegate { .. } => "launcher",
            UninstallPlan::DeleteFolder(_) => "trash",
            UninstallPlan::RemoveFromLibrary => "remove",
            UninstallPlan::HideFromLibrary => "hide",
            UninstallPlan::Unavailable(_) => "none",
        };
        let path = match plan {
            UninstallPlan::DeleteFolder(d) => d.display().to_string(),
            _ => self.lib.borrow().get(id).and_then(|g| g.install_dir.as_ref().map(|p| p.display().to_string())).unwrap_or_default(),
        };
        let (text, error) = match &outcome {
            Outcome::Done => {
                if matches!(plan, UninstallPlan::DeleteFolder(_) | UninstallPlan::Command { .. }) {
                    if let Err(e) = self.db().set_installed(id, false) {
                        log::warn!("état « non installé » non enregistré : {e}");
                    }
                    if let Some(g) = self.lib.borrow_mut().get_mut(id) {
                        g.installed = false;
                    }
                }
                let _ = self.db().log_activity(ActivityKind::Uninstalled, Some(id), &tr_later!("« {name} » désinstallé", name = name), &path);
                audit::event("game.uninstall", &format!("{name} · {how} · réussi · {path}"));
                (
                    match plan {
                        UninstallPlan::RemoveFromLibrary => tr!("« {name} » a été retiré de la bibliothèque. Aucun fichier n'a été supprimé.", name = name),
                        UninstallPlan::HideFromLibrary => tr!("« {name} » est masqué de la bibliothèque. La ROM est intacte.", name = name),
                        _ => tr!("« {name} » est désinstallé. Il reste dans la bibliothèque comme « Non installé » : favoris, temps de jeu et collections sont conservés.", name = name),
                    },
                    false,
                )
            }
            Outcome::Delegated(launcher) => {
                let _ = self.db().log_activity(
                    ActivityKind::Uninstalled,
                    Some(id),
                    &tr_later!("Désinstallation de « {name} » confiée à {launcher}", name = name, launcher = launcher),
                    &path,
                );
                audit::event("game.uninstall", &format!("{name} · {how} · délégué à {launcher} · {path}"));
                (
                    tr!("Désinstallation confiée à {launcher} : terminez-la là-bas. Halcyon verra le jeu « Non installé » à la prochaine analyse.", launcher = launcher),
                    false,
                )
            }
            Outcome::Failed(e) => {
                let _ = self.db().log_activity(ActivityKind::Error, Some(id), &tr_later!("Désinstallation de « {name} » impossible", name = name), e);
                audit::event("game.uninstall", &format!("{name} · {how} · échec : {e}"));
                (e.clone(), true)
            }
        };
        if let Some(app) = self.app() {
            let ui = app.global::<UninstallUi>();
            if ui.get_open() {
                ui.set_result(text.as_str().into());
                ui.set_result_error(error);
                ui.set_phase(2);
            } else {
                self.toast(text.clone(), error);
            }
        }
        if error {
            self.sound(halcyon_audio::Cue::Error);
        }
        self.game_changed(id);
        self.refresh_library();
        if self.bp.borrow().active {
            self.rebuild_bp_rows();
            self.refresh_bp();
        }
        if self.app().is_some_and(|a| a.global::<crate::Nav>().get_page() == crate::Page::Storage) {
            self.refresh_storage();
        }
    }
}

/// Exécute la voie choisie (fil de fond). Aucun shell : programme et arguments séparés.
fn run_plan(plan: &UninstallPlan) -> Outcome {
    match plan {
        UninstallPlan::Uri(uri) => match halcyon_integrations::launcher::open_uri(uri) {
            Ok(()) => Outcome::Delegated("Steam".into()),
            Err(e) => Outcome::Failed(tr!("Steam n'a pas pu être ouvert : {e}", e = e)),
        },
        UninstallPlan::Delegate { uri, launcher } => match uri {
            Some(u) => match halcyon_integrations::launcher::open_uri(u) {
                Ok(()) => Outcome::Delegated(launcher.clone()),
                Err(e) => Outcome::Failed(tr!("{launcher} n'a pas pu être ouvert : {e}", launcher = launcher, e = e)),
            },
            None => Outcome::Delegated(launcher.clone()),
        },
        UninstallPlan::Command { program, args, stdin } => run_command(program, args, stdin.as_deref()),
        UninstallPlan::DeleteFolder(dir) => match halcyon_integrations::trash::move_to_trash(dir) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(tr!("Le dossier n'a pas été supprimé : {e}.", e = e)),
        },
        UninstallPlan::RemoveFromLibrary | UninstallPlan::HideFromLibrary | UninstallPlan::Unavailable(_) => Outcome::Done,
    }
}

fn run_command(program: &str, args: &[String], stdin: Option<&str>) -> Outcome {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(tr!("{program} introuvable : {e}", program = program, e = e)),
    };
    if let (Some(text), Some(mut input)) = (stdin, child.stdin.take()) {
        let _ = input.write_all(text.as_bytes());
    }
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
                return if status.success() {
                    Outcome::Done
                } else {
                    let tail: String = out.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ");
                    Outcome::Failed(tr!("{program} a échoué : {detail}", program = program, detail = tail.chars().take(300).collect::<String>()))
                };
            }
            Ok(None) if start.elapsed() < COMMAND_TIMEOUT => std::thread::sleep(Duration::from_millis(250)),
            Ok(None) => {
                let _ = child.kill();
                return Outcome::Failed(tr!("{program} ne répond plus : opération interrompue", program = program));
            }
            Err(e) => return Outcome::Failed(e.to_string()),
        }
    }
}

/// Dossier Steam détecté (jamais supprimable, même s'il est dans un dossier de jeux).
fn steam_root() -> Option<PathBuf> {
    halcyon_integrations::steam::SteamScanner::new(None).root()
}

/// Jeux dont la désinstallation est proposée (bibliothèque, console, page Stockage).
pub fn can_uninstall(g: &halcyon_core::Game) -> bool {
    match g.source {
        GameSource::Manual | GameSource::Emulated => true,
        GameSource::Minecraft => false,
        _ => g.installed,
    }
}
