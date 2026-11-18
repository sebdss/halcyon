//! Export / restauration de la configuration, et mode hors ligne (surveillance de la
//! connexion, resynchronisation automatique au retour du réseau).

use crate::controller::{post, Controller};
use crate::{AppUi, SettingsUi};
use halcyon_core::util;
use halcyon_integrations::config_archive::{self, ExportOptions, Manifest};
use halcyon_integrations::saves;
use halcyon_storage::{now_unix, saves_mods, ActivityKind, Database};
use slint::ComponentHandle;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Hôtes testés pour savoir si Internet est joignable (connexion TCP seule, sans données).
const PROBES: &[&str] = &["store.steampowered.com:443", "retroachievements.org:443", "1.1.1.1:443"];

fn internet_reachable() -> bool {
    PROBES.iter().any(|h| {
        h.to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next())
            .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
    })
}

/// Horodatage local pour les noms de fichiers (« 20261103-170222 »).
pub(crate) fn stamp(now: i64) -> String {
    let now = now + crate::ctl_bigpicture::local_offset_secs();
    let (y, m, d) = util::civil_from_days(now.div_euclid(86_400));
    let s = now.rem_euclid(86_400);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

impl Controller {
    pub(crate) fn backups_dir(&self) -> PathBuf {
        self.paths.data_dir.join("backups")
    }

    /// Le réseau peut-il être utilisé (connexion détectée et mode hors ligne non forcé) ?
    pub(crate) fn net_allowed(&self) -> bool {
        !self.settings.borrow().offline_forced && self.online.get()
    }

    fn manifest(&self) -> Manifest {
        Manifest {
            format: config_archive::FORMAT,
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: saves_mods::schema_version(),
            created_at: now_unix(),
            machine: saves::machine_name(),
            games: self.lib.borrow().games.len(),
            includes_secrets: false,
            includes_mod_files: false,
            sections: vec![],
        }
    }

    fn set_backup_status(&self, msg: &str) {
        if let Some(app) = self.app() {
            app.global::<SettingsUi>().set_backup_status(msg.into());
        }
    }

    pub(crate) fn export_config(&self, secrets: bool, mod_files: bool) {
        self.save_settings();
        let dir = self.backups_dir();
        let data_dir = self.paths.data_dir.clone();
        let db = self.db.clone();
        let manifest = self.manifest();
        let dest = dir.join(format!("halcyon-config-{}.zip", stamp(now_unix())));
        self.set_backup_status(&tr!("Export en cours…"));
        let task = self.task_start("export", tr!("Export de la configuration"));
        self.task_update(task, Some(-1.0), None);
        std::thread::spawn(move || {
            let r = (|| -> Result<u64, String> {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let copy = dir.join("library-export.db.tmp");
                db.lock().unwrap_or_else(|e| e.into_inner()).export_copy(&copy).map_err(|e| e.to_string())?;
                let opts = ExportOptions { include_secrets: secrets, include_mod_files: mod_files };
                let size = config_archive::export(&data_dir, &copy, &dest, &opts, manifest).map_err(|e| e.to_string());
                let _ = std::fs::remove_file(&copy);
                size
            })();
            post(move |c| match r {
                Ok(size) => {
                    let msg = tr!("Exporté : {dest} ({size})", dest = dest.display(), size = util::format_size(size));
                    let _ = c.db().log_activity(ActivityKind::Settings, None, &tr_later!("Configuration exportée"), &dest.to_string_lossy());
                    c.set_backup_status(&msg);
                    c.task_finish(task, true, msg.clone(), crate::tasks::Action::OpenFolder(dest.parent().map(|p| p.display().to_string()).unwrap_or_default()));
                    c.sound(halcyon_audio::Cue::Success);
                    c.toast(tr!("Configuration exportée"), false);
                }
                Err(e) => {
                    c.set_backup_status(&tr!("Export impossible : {e}", e = e));
                    c.task_finish(task, false, tr!("Export impossible : {e}", e = e), crate::tasks::Action::None);
                    c.toast(tr!("Export impossible : {e}", e = e), true);
                }
            });
        });
    }

    /// Restauration : validation, confirmation, export de sécurité, remplacement, redémarrage.
    pub(crate) fn import_config(self: &Rc<Self>, path: &str) {
        let src = PathBuf::from(path.trim().trim_matches('"'));
        let manifest = match config_archive::inspect(&src) {
            Ok(m) => m,
            Err(e) => {
                self.set_backup_status(&tr!("Archive refusée : {e}", e = e));
                return self.toast(tr!("Archive refusée : {e}", e = e), true);
            }
        };
        if manifest.schema_version > saves_mods::schema_version() {
            return self.toast(tr!("Archive créée par une version plus récente de Halcyon : mettez Halcyon à jour d'abord"), true);
        }
        let text = trn!(
            manifest.games,
            "Archive du {date} créée sur « {machine} » (Halcyon {version}) : {n} jeu, {sections}. Vos données actuelles seront d'abord exportées dans le dossier des exports, puis remplacées ; Halcyon redémarrera. Les fichiers des jeux, sauvegardes de parties et mods installés ne sont pas modifiés.",
            "Archive du {date} créée sur « {machine} » (Halcyon {version}) : {n} jeux, {sections}. Vos données actuelles seront d'abord exportées dans le dossier des exports, puis remplacées ; Halcyon redémarrera. Les fichiers des jeux, sauvegardes de parties et mods installés ne sont pas modifiés.",
            date = util::format_date(manifest.created_at),
            machine = manifest.machine,
            version = manifest.app_version,
            sections = manifest.sections.join(", ")
        );
        // Une archive contient des commandes que Halcyon exécutera (scripts avant et après une
        // session, émulateurs, extensions) : la confirmation le dit.
        let text = format!(
            "{text}\n\n{}",
            tr!("Une archive contient aussi des commandes exécutées au lancement des jeux (scripts des profils, émulateurs, extensions) : ne restaurez qu'une archive créée par vous ou par une personne de confiance.")
        );
        self.confirm(&tr!("Restaurer cette configuration ?"), &text, &tr!("Restaurer et redémarrer"), move |c| c.run_import(src));
    }

    fn run_import(self: &Rc<Self>, src: PathBuf) {
        self.set_backup_status(&tr!("Restauration : préparation…"));
        let staging = self.paths.data_dir.join(".restore-staging");
        let dir = self.backups_dir();
        let data_dir = self.paths.data_dir.clone();
        let db = self.db.clone();
        let manifest = self.manifest();
        std::thread::spawn(move || {
            let r = (|| -> Result<PathBuf, String> {
                config_archive::extract(&src, &staging).map_err(|e| e.to_string())?;
                let version = saves_mods::inspect_database(&staging.join("library.db")).map_err(|e| tr!("base de l'archive : {e}", e = e))?;
                if version > saves_mods::schema_version() {
                    return Err(tr!("base créée par une version plus récente"));
                }
                // Filet de sécurité : export complet de l'état actuel.
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let safety = dir.join(format!("avant-restauration-{}.zip", stamp(now_unix())));
                let copy = dir.join("library-safety.db.tmp");
                db.lock().unwrap_or_else(|e| e.into_inner()).export_copy(&copy).map_err(|e| e.to_string())?;
                config_archive::export(
                    &data_dir,
                    &copy,
                    &safety,
                    &ExportOptions { include_secrets: true, include_mod_files: false },
                    manifest,
                )
                .map_err(|e| tr!("export de sécurité impossible, restauration annulée : {e}", e = e))?;
                let _ = std::fs::remove_file(&copy);
                Ok(safety)
            })();
            post(move |c| match r {
                Ok(safety) => c.finish_import(&staging, &safety),
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    c.set_backup_status(&tr!("Restauration annulée : {e}", e = e));
                    c.toast(tr!("Restauration annulée : {e}", e = e), true);
                }
            });
        });
    }

    fn finish_import(&self, staging: &std::path::Path, safety: &std::path::Path) {
        let previous = self.backups_dir().join(format!("remplace-{}", stamp(now_unix())));
        // La base est fermée (connexion temporaire en mémoire) le temps du remplacement.
        {
            let mut db = self.db();
            match Database::open_in_memory() {
                Ok(mem) => *db = mem,
                Err(e) => return self.toast(tr!("Restauration impossible : {e}", e = e), true),
            }
        }
        let r = config_archive::install(staging, &self.paths.data_dir, &previous);
        let _ = std::fs::remove_dir_all(staging);
        if let Err(e) = r {
            log::error!("restauration interrompue : {e} ; données d'origine dans {}", previous.display());
            self.toast(
                tr!("Restauration interrompue : {e}. Vos données d'origine sont dans {previous} et {safety}", e = e, previous = previous.display(), safety = safety.display()),
                true,
            );
            return;
        }
        log::info!("configuration restaurée ; données précédentes : {}", previous.display());
        // Les paramètres en mémoire ne doivent pas écraser ceux restaurés.
        self.no_save.set(true);
        self.restart();
    }

    /// Redémarre Halcyon (après une restauration).
    pub(crate) fn restart(&self) {
        if let Ok(exe) = std::env::current_exe() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            if let Err(e) = std::process::Command::new(exe).args(args).spawn() {
                self.toast(tr!("Redémarrez Halcyon manuellement ({e})", e = e), true);
                return;
            }
        }
        let _ = slint::quit_event_loop();
    }

    // ------------------------------------------------------------------ connexion

    /// Vérifie la connexion toutes les 45 s (connexion TCP seule) et resynchronise au retour.
    pub(crate) fn start_connectivity_monitor(&self) {
        std::thread::Builder::new()
            .name("halcyon-net".into())
            .spawn(move || {
                loop {
                    let online = internet_reachable();
                    post(move |c| c.set_online(online));
                    std::thread::sleep(Duration::from_secs(45));
                }
            })
            .ok();
    }

    fn set_online(&self, reachable: bool) {
        let forced = self.settings.borrow().offline_forced;
        let was = self.online.replace(reachable);
        if was && !reachable && !forced {
            self.sound(halcyon_audio::Cue::Warning);
        }
        if let Some(app) = self.app() {
            app.global::<AppUi>().set_online(reachable && !forced);
        }
        // Le dépôt de sauvegardes (NAS) peut revenir indépendamment d'Internet.
        let root_ok = self.settings.borrow().save_root.as_ref().map(|r| r.is_dir());
        if let Some(ok) = root_ok {
            if ok && !self.save_root_ok.replace(true) {
                log::info!("dépôt de sauvegardes de nouveau accessible");
                self.sync_pending_saves();
            } else if !ok {
                self.save_root_ok.set(false);
            }
        }
        if reachable && !was && !forced {
            self.on_reconnected();
        }
    }

    /// Retour de la connexion : on relance ce qui attendait le réseau.
    fn on_reconnected(&self) {
        log::info!("connexion rétablie : synchronisation");
        let _ = self.db().log_activity(ActivityKind::Sync, None, &tr_later!("Connexion rétablie"), &tr_later!("Synchronisation des succès, visuels et métadonnées"));
        let s = self.settings.borrow().clone();
        self.start_online_jobs();
        if s.sync_achievements_on_startup {
            self.start_ach_sync(None, false);
        }
        if s.check_updates {
            crate::controller::post(|c| c.hus_check(false));
        }
        self.sync_pending_saves();
    }
}
