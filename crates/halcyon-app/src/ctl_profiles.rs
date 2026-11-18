//! Lancement des jeux (profils, scripts, programmes compagnons) et suivi des sessions.
//!
//! Déroulé d'un lancement :
//! 1. choix du profil et calcul de ce qui peut réellement être appliqué (`ProfilePlan`) ;
//! 2. hors du thread de l'interface : commande « avant », programmes compagnons,
//!    préférence GPU (Windows, sur autorisation) ;
//! 3. lancement du jeu ;
//! 4. suivi de la session : processus direct, ou détection du jeu lancé par son client ;
//! 5. fin de session : commande « après », fermeture des compagnons, rétablissement du GPU.

use crate::controller::{post, Controller};
use crate::{CompanionRow, DetailUi, ProfileRow};
use halcyon_core::profiles::{
    steam_run_uri, Companion, DisplayMode, Engine, GpuPreference, HostInfo, LaunchKind, LaunchProfile, ProcessPriority,
};
use halcyon_core::{Game, GameId, GameSource, LaunchTarget};
use halcyon_integrations::gpu::GpuGuard;
use halcyon_integrations::launcher::{self, LaunchExtras, Launched};
use halcyon_integrations::process::{GameProbe, Watcher};
use halcyon_integrations::{engines, gpu, steam};
use crate::ctl_mods::ModJob;
use crate::ctl_saves::SaveJob;
use halcyon_storage::{now_unix, ActivityKind, Database};
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::process::Child;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Profil demandé pour un lancement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProfileChoice {
    /// Profil par défaut du jeu, s'il en a un.
    Default,
    NoProfile,
    Id(i64),
}

/// Lancement prêt, transmis au thread de préparation.
struct Prepared {
    game: Game,
    target: LaunchTarget,
    extras: LaunchExtras,
    profile: Option<LaunchProfile>,
    notes: Vec<String>,
    gpu: Option<(PathBuf, GpuPreference)>,
    save_job: Option<SaveJob>,
    mod_job: Option<ModJob>,
}

/// Ce qu'il faut défaire à la fin de la session.
struct Cleanup {
    companions: Vec<(Child, bool)>,
    gpu: Option<GpuGuard>,
    post_command: String,
    working_dir: Option<PathBuf>,
    /// Sauvegarde automatique de fin de session.
    save: Option<(SaveJob, Arc<Mutex<Database>>)>,
}

impl Cleanup {
    fn run(self, game: &str) {
        if let Some((job, db)) = &self.save {
            if job.auto_after {
                match job.run(db, "after_session", "", true) {
                    Ok(Some(m)) => log::info!("sauvegarde de fin de session de « {game} » : {} fichier(s)", m.files.len()),
                    Ok(None) => {}
                    Err(e) => log::warn!("sauvegarde de fin de session de « {game} » impossible : {e}"),
                }
            }
        }
        for (mut child, close) in self.companions {
            if close {
                halcyon_integrations::process::terminate(&mut child);
            }
        }
        if let Some(g) = self.gpu {
            g.restore();
        }
        if let Err(e) = launcher::run_hook(&self.post_command, self.working_dir.as_deref(), Duration::from_secs(60)) {
            log::warn!("commande de fin de session de « {game} » : {e}");
        }
    }
}

fn host_info(target: &LaunchTarget, windows_runner_used: bool) -> HostInfo {
    let _ = target;
    HostInfo {
        windows: cfg!(windows),
        nvidia: std::path::Path::new("/proc/driver/nvidia/version").exists(),
        via_wine: windows_runner_used,
    }
}

impl Controller {
    pub(crate) fn play(self: &Rc<Self>, id: GameId) {
        self.play_with(id, ProfileChoice::Default);
    }

    /// Bouton Jouer de la fiche : `arg` = identifiant de profil, ou -1 pour le choix de la liste.
    pub(crate) fn play_from_detail(self: &Rc<Self>, id: GameId, arg: i32) {
        let choice = if arg > 0 {
            ProfileChoice::Id(arg as i64)
        } else {
            let index = self.app().map(|a| a.global::<DetailUi>().get_profile_index()).unwrap_or(0);
            let profiles = self.db().launch_profiles(id).unwrap_or_default();
            match (index as usize).checked_sub(1).and_then(|i| profiles.get(i)) {
                Some(p) => ProfileChoice::Id(p.id),
                None if profiles.is_empty() => ProfileChoice::Default,
                None => ProfileChoice::NoProfile,
            }
        };
        self.play_with(id, choice);
    }

    pub(crate) fn play_with(self: &Rc<Self>, id: GameId, choice: ProfileChoice) {
        let Some(game) = self.lib.borrow().get(id).cloned() else { return };
        if game.source == GameSource::Minecraft {
            // Minecraft se lance par l'onglet Minecraft (installation, loader, mods, compte).
            return self.mc_play_from_library(id);
        }
        if !game.installed {
            if game.source == GameSource::Steam {
                match launcher::open_uri(&format!("steam://install/{}", game.source_key)) {
                    Ok(()) => self.toast(tr!("Installation de « {name} » demandée à Steam", name = game.name), false),
                    Err(e) => self.toast(tr!("Steam n'a pas pu être appelé : {e}", e = e), true),
                }
            } else {
                self.toast(tr!("« {name} » n'est pas installé", name = game.name), true);
            }
            return;
        }
        if self.active_session.get() == Some(id) {
            self.toast(tr!("« {name} » est déjà en cours", name = game.name), false);
            return;
        }
        let profile = {
            let profiles = self.db().launch_profiles(id).unwrap_or_default();
            match choice {
                ProfileChoice::Default => profiles.into_iter().find(|p| p.is_default),
                ProfileChoice::NoProfile => None,
                ProfileChoice::Id(pid) => profiles.into_iter().find(|p| p.id == pid),
            }
        };
        let target = if game.source == GameSource::Emulated {
            match self.emulator_target(&game, profile.as_ref().and_then(|p| p.controller_profile)) {
                Ok(t) => t,
                Err(e) => {
                    self.toast(e, true);
                    return;
                }
            }
        } else {
            match game.launch_target() {
                Some(t) => t,
                None => {
                    self.toast(tr!("Impossible de lancer « {name} » : aucun moyen de lancement configuré", name = game.name), true);
                    return;
                }
            }
        };
        let mut prepared = self.prepare(game, target, profile);
        prepared.save_job = self.save_job_for(&prepared.game);
        prepared.mod_job = self.mod_job_for_launch(&prepared.game, prepared.profile.as_ref().and_then(|p| p.mod_profile));
        let needs_thread = prepared.profile.as_ref().is_some_and(|p| {
            !p.pre_command.trim().is_empty() || !p.companions.is_empty()
        }) || prepared.gpu.is_some()
            || prepared.mod_job.is_some()
            || prepared.save_job.as_ref().is_some_and(|j| j.auto_before);
        if needs_thread {
            self.toast(tr!("Préparation de « {name} »…", name = prepared.game.name), false);
            let db = self.db.clone();
            std::thread::Builder::new()
                .name("halcyon-prelaunch".into())
                .spawn(move || {
                    let (companions, gpu, error) = run_prelaunch(&prepared, &db);
                    post(move |c| match error {
                        Some(e) => {
                            Cleanup { companions, gpu, post_command: String::new(), working_dir: None, save: None }
                                .run(&prepared.game.name);
                            c.toast(tr!("Lancement annulé : {e}", e = e), true);
                        }
                        None => c.finish_launch(prepared, companions, gpu),
                    });
                })
                .ok();
        } else {
            self.finish_launch(prepared, Vec::new(), None);
        }
    }

    /// Applique le profil à la cible de lancement.
    fn prepare(&self, game: Game, target: LaunchTarget, profile: Option<LaunchProfile>) -> Prepared {
        let Some(profile) = profile else {
            return Prepared {
                game,
                target,
                extras: LaunchExtras::default(),
                profile: None,
                notes: Vec::new(),
                gpu: None,
                save_job: None,
                mod_job: None,
            };
        };
        let engine = if game.source == GameSource::Emulated {
            Engine::Unknown
        } else {
            game.install_dir.as_deref().map(engines::detect).unwrap_or_default()
        };
        let (kind, base_args, via_wine) = match &target {
            LaunchTarget::Uri(u) if u.starts_with("steam://") => (LaunchKind::Steam, String::new(), false),
            LaunchTarget::Uri(_) => (LaunchKind::PlatformClient, String::new(), false),
            LaunchTarget::Executable { path, args, .. } => {
                let wine = !cfg!(windows) && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
                (LaunchKind::Direct, args.clone(), wine)
            }
            LaunchTarget::Process { .. } => (LaunchKind::Direct, String::new(), false),
        };
        let plan = profile.plan(&base_args, engine, kind, host_info(&target, via_wine));
        let mut target = target;
        let mut extras = LaunchExtras {
            args: None,
            env: plan.env.clone(),
            priority: plan.priority,
            working_dir: plan.working_dir.clone(),
        };
        if let Some(args) = plan.args.clone() {
            match &mut target {
                LaunchTarget::Uri(u) if kind == LaunchKind::Steam => {
                    if !args.is_empty() {
                        *u = steam_run_uri(&game.source_key, &args);
                    }
                }
                LaunchTarget::Executable { .. } => extras.args = Some(args),
                LaunchTarget::Process { args: a, .. } => {
                    // Émulateur : options avant la ROM (dernier argument) ; Legendary : à la suite.
                    if game.source == GameSource::Emulated && !a.is_empty() {
                        let rom = a.pop();
                        a.extend(args);
                        a.extend(rom);
                    } else {
                        a.extend(args);
                    }
                }
                LaunchTarget::Uri(_) => {}
            }
        }
        let gpu = plan.gpu_registry.and_then(|pref| match &target {
            LaunchTarget::Executable { path, .. } => Some((path.clone(), pref)),
            _ => None,
        });
        Prepared { game, target, extras, notes: plan.notes, profile: Some(profile), gpu, save_job: None, mod_job: None }
    }

    fn finish_launch(self: &Rc<Self>, prep: Prepared, companions: Vec<(Child, bool)>, gpu: Option<GpuGuard>) {
        let Prepared { game, target, extras, profile, mut notes, save_job, .. } = prep;
        let id = game.id;
        let cleanup = Cleanup {
            companions,
            gpu,
            post_command: profile.as_ref().map(|p| p.post_command.clone()).unwrap_or_default(),
            working_dir: game.install_dir.clone(),
            save: save_job.map(|j| (j, self.db.clone())),
        };
        let started = now_unix();
        let result = launcher::launch_with(Some(&target), &self.launch_options(), &extras);
        let (launched, warnings) = match result {
            Ok(r) => r,
            Err(e) => {
                cleanup.run(&game.name);
                log::warn!("échec du lancement de « {} » : {e}", game.name);
                self.toast(tr!("Impossible de lancer « {name} » : {e}", name = game.name, e = e), true);
                let _ = self.db().log_activity(ActivityKind::Error, Some(id), &tr_later!("Échec du lancement de {name}", name = game.name), &e.to_string());
                return;
            }
        };
        notes.extend(warnings);
        if let Err(e) = self.db().mark_launched(id, started) {
            log::error!("{e}");
        }
        let profile_name = profile.as_ref().map(|p| p.name.clone());
        let detail = match &profile_name {
            Some(n) => tr!("Profil « {n} »", n = n),
            None => String::new(),
        };
        let _ = self.db().log_activity(ActivityKind::Launch, Some(id), &tr_later!("Lancement de {name}", name = game.name), &detail);
        // Avant le passage en « partie active » (qui peut rendre Halcyon silencieux).
        self.sound(halcyon_audio::Cue::Launch);
        self.mark_played(id, started);
        self.active_session.set(Some(id));
        self.open_companion(id);
        self.performance_session_started();
        if let Some(app) = self.app() {
            app.global::<crate::AppUi>().set_playing(game.name.clone().into());
        }
        let task = self.task_start("session", tr!("En jeu : {name}", name = game.name));
        self.task_update(task, Some(-1.0), Some(&tr!("Session suivie par Halcyon")));
        let mut msg = match &profile_name {
            Some(n) => tr!("Lancement de « {name} » (profil « {n} »)", name = game.name, n = n),
            None => tr!("Lancement de « {name} »", name = game.name),
        };
        if !notes.is_empty() {
            for n in &notes {
                log::info!("profil de « {} » : {n}", game.name);
            }
            msg.push_str(&trn!(notes.len(), " · {n} réglage non appliqué", " · {n} réglages non appliqués"));
            if self.detail_id.get() == Some(id) {
                if let Some(app) = self.app() {
                    let v: Vec<SharedString> = notes.iter().map(|n| n.as_str().into()).collect();
                    app.global::<DetailUi>().set_profile_notes(ModelRc::new(VecModel::from(v)));
                }
            }
        }
        self.toast(msg, false);

        let db = self.db.clone();
        let probe = self.probe_for(&game);
        let name = game.name.clone();
        // Battle.net, Riot, Xbox : le programme lancé est le client (ou l'Explorateur), qui peut
        // rester ouvert après le jeu. La session est suivie par le dossier du jeu, pas par lui.
        let launched = match launched {
            Launched::Process(child) if matches!(game.source, GameSource::BattleNet | GameSource::Riot | GameSource::Xbox) => {
                log::info!("« {name} » confié à son client (pid {})", child.id());
                Launched::Detached
            }
            other => other,
        };
        match launched {
            Launched::Process(mut child) => {
                log::info!("lancement de « {name} » (pid {})", child.id());
                let follow_dir = game.source != GameSource::Emulated;
                std::thread::Builder::new()
                    .name(format!("session-{id}"))
                    .spawn(move || {
                        let start = std::time::Instant::now();
                        let status = child.wait();
                        let mut end = start.elapsed();
                        // Lanceurs intermédiaires : le jeu peut continuer après la fin du processus lancé.
                        if follow_dir && probe.install_dir.is_some() {
                            let mut w = Watcher::new(GameProbe { steam: None, ..probe });
                            let mut misses = if w.is_running() { 0 } else { 2 };
                            while misses < 2 {
                                std::thread::sleep(Duration::from_secs(4));
                                if w.is_running() {
                                    misses = 0;
                                    end = start.elapsed();
                                } else {
                                    misses += 1;
                                }
                            }
                        }
                        let secs = end.as_secs();
                        log::info!("session terminée pour « {name} » : {secs} s ({status:?})");
                        if let Err(e) = db.lock().unwrap_or_else(|e| e.into_inner()).record_session(id, started, secs) {
                            log::error!("enregistrement de la session impossible : {e}");
                        }
                        cleanup.run(&name);
                        post(move |c| c.on_session_ended(id, secs, true));
                    })
                    .ok();
            }
            Launched::Detached => {
                log::info!("lancement de « {name} » via {target:?}");
                std::thread::Builder::new()
                    .name(format!("watch-{id}"))
                    .spawn(move || {
                        let cancel = AtomicBool::new(false);
                        // Jeu lancé par sa plateforme : confirmation sonore quand il apparaît.
                        let observed = Watcher::new(probe).wait_session_with(Duration::from_secs(150), &cancel, || {
                            post(move |c| {
                                c.sound(halcyon_audio::Cue::Launched);
                                c.launch_screen_running(id);
                            });
                        });
                        match observed {
                            Some(o) => {
                                log::info!("session observée pour « {name} » : {} s", o.duration_secs);
                                if let Err(e) = db.lock().unwrap_or_else(|e| e.into_inner()).record_session(id, o.started_at, o.duration_secs) {
                                    log::error!("enregistrement de la session impossible : {e}");
                                }
                                cleanup.run(&name);
                                post(move |c| c.on_session_ended(id, o.duration_secs, true));
                            }
                            None => {
                                log::info!("« {name} » n'a pas été détecté en cours d'exécution : durée non mesurée");
                                // Les compagnons restent ouverts : on ignore quand le jeu se termine.
                                drop(cleanup.gpu);
                                post(move |c| c.on_session_ended(id, 0, false));
                            }
                        }
                    })
                    .ok();
            }
        }
        self.start_polling(id);
        self.after_launch(&game);
    }

    /// Indices permettant de reconnaître le jeu en cours d'exécution.
    fn probe_for(&self, game: &Game) -> GameProbe {
        let steam = (game.source == GameSource::Steam)
            .then(|| steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root())
            .flatten()
            .zip(game.source_key.parse::<u64>().ok());
        GameProbe { install_dir: game.install_dir.clone().filter(|d| d.is_dir()), steam }
    }

    pub(crate) fn on_session_ended(&self, id: GameId, secs: u64, measured: bool) {
        self.active_session.set(None);
        self.stop_break_reminder();
        self.update_presence();
        self.close_companion();
        self.performance_session_ended();
        if let Some(app) = self.app() {
            app.global::<crate::AppUi>().set_playing("".into());
        }
        let detail = if measured { tr!("Session de {secs}", secs = halcyon_core::util::format_playtime(secs)) } else { tr!("Session terminée (durée non mesurée)") };
        let task = self.tasks.borrow().find_kind("session");
        if let Some(task) = task {
            self.task_finish(task, true, detail, crate::tasks::Action::OpenGame(id));
        }
        if measured {
            if let Some(g) = self.lib.borrow_mut().get_mut(id) {
                g.playtime_secs += secs;
            }
        }
        self.game_changed(id);
        self.refresh_home();
        self.stop_polling();
        if self.detail_id.get() == Some(id) {
            self.refresh_game_tools();
        }
        if measured {
            // Dernière vérification des succès (seulement si un compte est configuré pour ce jeu).
            let mode = self.lib.borrow().get(id).map(|g| self.ach_mode(g));
            if matches!(mode, Some("steam" | "retroachievements")) {
                self.start_ach_sync(Some(id), true);
            }
            if self.settings.borrow().minimize_on_launch {
                if let Some(app) = self.app() {
                    app.window().set_minimized(false);
                }
            }
        }
    }

    // ------------------------------------------------------------------ édition des profils

    pub(crate) fn refresh_profiles_ui(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let profiles = self.db().launch_profiles(g.id).unwrap_or_default();
        let mod_profiles = self.db().mod_profiles(g.id).unwrap_or_default();
        let pads = self.settings.borrow().pad_profiles.clone();
        let rows: Vec<ProfileRow> = profiles.iter().map(|p| profile_row(p, &mod_profiles, &pads)).collect();
        let mut pad_labels: Vec<SharedString> = vec![tr!("Profil de manette actif").into()];
        pad_labels.extend(pads.iter().map(|p| SharedString::from(p.name.as_str())));
        dui.set_pad_profile_labels(ModelRc::new(VecModel::from(pad_labels)));
        let mut labels: Vec<SharedString> = vec![tr!("Sans profil").into()];
        labels.extend(profiles.iter().map(|p| SharedString::from(p.name.as_str())));
        let default = profiles.iter().position(|p| p.is_default).map(|i| i + 1).unwrap_or(0);
        dui.set_profiles(ModelRc::new(VecModel::from(rows)));
        dui.set_profile_labels(ModelRc::new(VecModel::from(labels)));
        dui.set_profile_index(default as i32);
        let engine = if g.source == GameSource::Emulated {
            tr!("émulateur (options transmises avant la ROM)")
        } else {
            g.install_dir.as_deref().map(engines::detect).unwrap_or_default().label().to_string()
        };
        dui.set_engine(engine.into());
        dui.set_is_windows(cfg!(windows));
    }

    pub(crate) fn new_profile(&self, game: GameId) {
        let Some(app) = self.app() else { return };
        let mut p = LaunchProfile::new(game, "");
        p.is_default = self.db().launch_profiles(game).map(|l| l.is_empty()).unwrap_or(true);
        self.edit_companions.set_vec(Vec::new());
        let dui = app.global::<DetailUi>();
        let mod_profiles = self.db().mod_profiles(game).unwrap_or_default();
        let pads = self.settings.borrow().pad_profiles.clone();
        dui.set_editing(profile_row(&p, &mod_profiles, &pads));
        *self.editing_profile.borrow_mut() = Some(p);
        dui.set_profile_notes(ModelRc::default());
        dui.set_editing_open(true);
    }

    pub(crate) fn edit_profile(&self, id: i64) {
        let (Some(app), Some(game)) = (self.app(), self.detail_id.get()) else { return };
        let Some(p) = self.db().launch_profiles(game).unwrap_or_default().into_iter().find(|p| p.id == id) else { return };
        self.edit_companions.set_vec(
            p.companions
                .iter()
                .map(|c| CompanionRow {
                    path: c.path.to_string_lossy().into_owned().into(),
                    args: c.args.as_str().into(),
                    close_after: c.close_after,
                })
                .collect::<Vec<_>>(),
        );
        let dui = app.global::<DetailUi>();
        let mod_profiles = self.db().mod_profiles(game).unwrap_or_default();
        let pads = self.settings.borrow().pad_profiles.clone();
        dui.set_editing(profile_row(&p, &mod_profiles, &pads));
        *self.editing_profile.borrow_mut() = Some(p);
        dui.set_editing_open(true);
    }

    pub(crate) fn add_companion(&self, path: &str, args: &str, close: bool) {
        let path = path.trim().trim_matches('"');
        if !std::path::Path::new(path).is_file() {
            self.toast(tr!("Programme introuvable : {path}", path = path), true);
            return;
        }
        self.edit_companions.push(CompanionRow { path: path.into(), args: args.trim().into(), close_after: close });
    }

    pub(crate) fn remove_companion(&self, index: usize) {
        if index < self.edit_companions.row_count() {
            self.edit_companions.remove(index);
        }
    }

    pub(crate) fn save_profile(&self, game: GameId, row: ProfileRow) {
        let Some(app) = self.app() else { return };
        let mut p = self.editing_profile.borrow().clone().unwrap_or_else(|| LaunchProfile::new(game, ""));
        p.id = row.id as i64;
        p.game_id = game;
        p.name = row.name.trim().to_string();
        p.is_default = row.is_default;
        p.args = row.args.trim().to_string();
        p.replace_args = row.replace_args;
        p.resolution = match parse_resolution(&row.resolution) {
            Ok(r) => r,
            Err(e) => return self.toast(e, true),
        };
        p.display = DisplayMode::ALL.get(row.display as usize).copied().unwrap_or_default();
        p.fps_limit = match row.fps.trim() {
            "" => None,
            v => match v.parse::<u32>() {
                Ok(n) if (10..=1000).contains(&n) => Some(n),
                _ => return self.toast(tr!("Limite d'images : nombre entre 10 et 1000 attendu"), true),
            },
        };
        p.gpu = GpuPreference::ALL.get(row.gpu as usize).copied().unwrap_or_default();
        p.allow_gpu_registry = row.allow_gpu_registry && p.gpu != GpuPreference::Default;
        p.priority = ProcessPriority::ALL.get(row.priority as usize).copied().unwrap_or_default();
        p.working_dir = Some(PathBuf::from(row.working_dir.trim())).filter(|d| !d.as_os_str().is_empty());
        if let Some(wd) = &p.working_dir {
            if !wd.is_dir() {
                return self.toast(tr!("Répertoire introuvable : {wd}", wd = wd.display()), true);
            }
        }
        p.env = match parse_env(&row.env) {
            Ok(e) => e,
            Err(e) => return self.toast(e, true),
        };
        let mod_profiles = self.db().mod_profiles(game).unwrap_or_default();
        p.mod_profile = (row.mod_profile as usize).checked_sub(1).and_then(|i| mod_profiles.get(i)).map(|m| m.id);
        p.controller_profile = (row.pad_profile as usize)
            .checked_sub(1)
            .and_then(|i| self.settings.borrow().pad_profiles.get(i).map(|x| x.id));
        p.pre_command = row.pre.trim().to_string();
        p.post_command = row.post.trim().to_string();
        p.companions = self
            .edit_companions
            .iter()
            .map(|c| Companion { path: PathBuf::from(c.path.as_str()), args: c.args.to_string(), close_after: c.close_after })
            .collect();
        let result = self.db().save_launch_profile(&p);
        match result {
            Ok(_) => {
                let g = self.lib.borrow().get(game).cloned();
                if let Some(g) = g {
                    // Ce qui ne s'appliquera pas, calculé comme au lancement.
                    let target = if g.source == GameSource::Emulated {
                        self.emulator_target(&g, p.controller_profile).ok()
                    } else {
                        g.launch_target()
                    };
                    let notes = target.map(|t| self.prepare(g.clone(), t, Some(p.clone())).notes).unwrap_or_default();
                    let v: Vec<SharedString> = notes.iter().map(|n| n.as_str().into()).collect();
                    app.global::<DetailUi>().set_profile_notes(ModelRc::new(VecModel::from(v)));
                    self.refresh_profiles_ui(&g);
                    let _ = self.db().log_activity(
                        ActivityKind::Profile,
                        Some(game),
                        &tr_later!("Profil « {name} » enregistré", name = p.name),
                        &g.name,
                    );
                }
                app.global::<DetailUi>().set_editing_open(false);
                *self.editing_profile.borrow_mut() = None;
                self.toast(tr!("Profil « {name} » enregistré", name = p.name), false);
            }
            Err(e) => self.toast(tr!("Enregistrement impossible : {e}", e = e), true),
        }
    }

    pub(crate) fn delete_profile(self: &Rc<Self>, id: i64) {
        let Some(game) = self.detail_id.get() else { return };
        let Some(p) = self.db().launch_profiles(game).unwrap_or_default().into_iter().find(|p| p.id == id) else { return };
        self.confirm(
            &tr!("Supprimer le profil ?"),
            &tr!("Le profil « {name} » sera supprimé. Le jeu et ses fichiers ne sont pas touchés.", name = p.name),
            &tr!("Supprimer"),
            move |c| {
                if let Err(e) = c.db().delete_launch_profile(id) {
                    return c.toast(tr!("Suppression impossible : {e}", e = e), true);
                }
                let g = c.lib.borrow().get(game).cloned();
                if let Some(g) = g {
                    c.refresh_profiles_ui(&g);
                }
                c.toast(tr!("Profil « {name} » supprimé", name = p.name), false);
            },
        );
    }
}

/// Préparation hors du thread de l'interface : mods, sauvegarde, commande « avant »,
/// compagnons, GPU.
fn run_prelaunch(p: &Prepared, db: &Arc<Mutex<Database>>) -> (Vec<(Child, bool)>, Option<GpuGuard>, Option<String>) {
    let mut companions = Vec::new();
    if let Some(job) = &p.mod_job {
        if let Err(e) = job.run() {
            return (companions, None, Some(tr!("application des mods : {e}", e = e)));
        }
    }
    if let Some(job) = p.save_job.as_ref().filter(|j| j.auto_before) {
        // Une sauvegarde impossible ne bloque pas la partie, mais elle est signalée.
        if let Err(e) = job.run(db, "before_session", "", true) {
            log::warn!("sauvegarde avant la session de « {} » impossible : {e}", p.game.name);
        }
    }
    let Some(profile) = &p.profile else { return (companions, None, None) };
    if let Err(e) = launcher::run_hook(&profile.pre_command, p.game.install_dir.as_deref(), Duration::from_secs(60)) {
        return (companions, None, Some(tr!("commande avant le lancement : {e}", e = e)));
    }
    for c in &profile.companions {
        match launcher::spawn_companion(&c.path, &c.args) {
            Ok(child) => companions.push((child, c.close_after)),
            Err(e) => log::warn!("programme compagnon non démarré : {e}"),
        }
    }
    let gpu = p.gpu.as_ref().and_then(|(exe, pref)| match gpu::apply(exe, *pref) {
        Ok(g) => Some(g),
        Err(e) => {
            log::warn!("{e}");
            None
        }
    });
    (companions, gpu, None)
}

fn parse_resolution(s: &str) -> Result<Option<(u32, u32)>, String> {
    let s = s.trim().to_lowercase().replace(' ', "");
    if s.is_empty() {
        return Ok(None);
    }
    let (w, h) = s.split_once(['x', '*', '×']).ok_or(tr!("Résolution : format attendu 1920x1080"))?;
    match (w.parse::<u32>(), h.parse::<u32>()) {
        (Ok(w), Ok(h)) if (320..=16_384).contains(&w) && (200..=16_384).contains(&h) => Ok(Some((w, h))),
        _ => Err(tr!("Résolution : format attendu 1920x1080")),
    }
}

fn parse_env(s: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for line in s.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (k, v) = line.split_once('=').ok_or_else(|| tr!("Variable invalide (NOM=valeur attendu) : {line}", line = line))?;
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(tr!("Nom de variable invalide : {k}", k = k));
        }
        out.push((k.to_string(), v.trim().to_string()));
    }
    Ok(out)
}

fn profile_row(p: &LaunchProfile, mod_profiles: &[halcyon_storage::ModProfile], pads: &[halcyon_storage::settings::PadProfile]) -> ProfileRow {
    let pad_index = p.controller_profile.and_then(|id| pads.iter().position(|x| x.id == id)).map(|i| i + 1).unwrap_or(0);
    let mod_index = p.mod_profile.and_then(|id| mod_profiles.iter().position(|m| m.id == id)).map(|i| i + 1).unwrap_or(0);
    let mut parts: Vec<String> = Vec::new();
    if p.display != DisplayMode::Default {
        parts.push(p.display.label().to_string());
    }
    if let Some((w, h)) = p.resolution {
        parts.push(format!("{w}×{h}"));
    }
    if let Some(f) = p.fps_limit {
        parts.push(tr!("{f} i/s max", f = f));
    }
    if p.gpu != GpuPreference::Default {
        parts.push(p.gpu.label().to_string());
    }
    if p.priority != ProcessPriority::Normal {
        parts.push(tr!("priorité {v}", v = halcyon_i18n::lower_first(&p.priority.label())));
    }
    if !p.args.is_empty() {
        parts.push(format!("« {} »", p.args));
    }
    if !p.companions.is_empty() {
        parts.push(trn!(p.companions.len(), "{n} programme associé", "{n} programmes associés"));
    }
    if !p.pre_command.is_empty() || !p.post_command.is_empty() {
        parts.push("scripts".into());
    }
    if !p.env.is_empty() {
        parts.push(trn!(p.env.len(), "{n} variable", "{n} variables"));
    }
    if let Some(m) = p.mod_profile.and_then(|id| mod_profiles.iter().find(|m| m.id == id)) {
        parts.push(tr!("mods « {name} »", name = m.name));
    }
    if let Some(x) = p.controller_profile.and_then(|id| pads.iter().find(|x| x.id == id)) {
        parts.push(tr!("manette « {name} »", name = x.name));
    }
    ProfileRow {
        id: p.id as i32,
        name: p.name.as_str().into(),
        summary: if parts.is_empty() { tr!("Paramètres du jeu").into() } else { parts.join(" · ").into() },
        is_default: p.is_default,
        args: p.args.as_str().into(),
        replace_args: p.replace_args,
        resolution: p.resolution.map(|(w, h)| format!("{w}x{h}")).unwrap_or_default().into(),
        display: DisplayMode::ALL.iter().position(|d| *d == p.display).unwrap_or(0) as i32,
        fps: p.fps_limit.map(|f| f.to_string()).unwrap_or_default().into(),
        gpu: GpuPreference::ALL.iter().position(|g| *g == p.gpu).unwrap_or(0) as i32,
        allow_gpu_registry: p.allow_gpu_registry,
        priority: ProcessPriority::ALL.iter().position(|x| *x == p.priority).unwrap_or(2) as i32,
        working_dir: p.working_dir.as_ref().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default().into(),
        env: p.env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n").into(),
        pre: p.pre_command.as_str().into(),
        post: p.post_command.as_str().into(),
        mod_profile: mod_index as i32,
        pad_profile: pad_index as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_fields() {
        assert_eq!(parse_resolution("1920 x 1080"), Ok(Some((1920, 1080))));
        assert_eq!(parse_resolution(""), Ok(None));
        assert!(parse_resolution("abc").is_err());
        assert!(parse_resolution("10x10").is_err());
        assert_eq!(parse_env("A=1\n# commentaire\n B = deux mots "), Ok(vec![("A".into(), "1".into()), ("B".into(), "deux mots".into())]));
        assert!(parse_env("pas une variable").is_err());
        assert!(parse_env("A-B=1").is_err());
    }
}
