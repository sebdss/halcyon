//! Sauvegardes des jeux : configuration par jeu, instantanés automatiques autour des
//! sessions, restauration, comparaison, dépôt partagé (NAS, autre disque) et conflits.
//!
//! Les opérations sur les fichiers (empreintes, archives) tournent hors du thread de
//! l'interface ; leurs résultats reviennent par `post()`.

use crate::controller::{post, Controller};
use crate::ctl_tracking::date_time;
use crate::{DetailUi, SaveCandRow, SaveGameRow, SaveLocRow, SavesUi, SnapshotRow};
use halcyon_core::{util, Game, GameId, GameSource};
use halcyon_integrations::saves::{
    self, content_hash, diff, scan_locations, Candidate, Created, GameInfo, Location, SaveEnv, SaveStore, SnapshotMeta,
    SyncState,
};
use halcyon_integrations::steam;
use halcyon_storage::{now_unix, ActivityKind, Database, SaveConfig};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

/// Tout ce qu'il faut pour sauvegarder un jeu depuis un autre thread.
#[derive(Clone)]
pub(crate) struct SaveJob {
    pub game_id: GameId,
    pub key: String,
    pub name: String,
    pub locations: Vec<Location>,
    pub primary: SaveStore,
    pub mirrors: Vec<SaveStore>,
    pub machine: String,
    pub keep: usize,
    pub auto_before: bool,
    pub auto_after: bool,
}

impl SaveJob {
    /// Crée un instantané (ignoré s'il est identique au précédent quand `dedup`),
    /// le copie sur les disques secondaires et purge les anciens automatiques.
    pub fn run(&self, db: &Arc<Mutex<Database>>, reason: &str, note: &str, dedup: bool) -> Result<Option<SnapshotMeta>, String> {
        self.run_at(db, reason, note, dedup, now_unix())
    }

    /// Comme [`run`](Self::run), daté de `at` (une version choisie comme référence doit
    /// passer devant celles des autres PC, même si leurs horloges sont en avance).
    pub fn run_at(&self, db: &Arc<Mutex<Database>>, reason: &str, note: &str, dedup: bool, at: i64) -> Result<Option<SnapshotMeta>, String> {
        if !self.primary.available() {
            return Err(format!("dépôt de sauvegardes inaccessible : {}", self.primary.root.display()));
        }
        let created = self
            .primary
            .create(&self.key, &self.name, &self.locations, reason, &self.machine, note, at, dedup)
            .map_err(|e| e.to_string())?;
        let meta = match created {
            Created::New(m) => m,
            Created::Unchanged(m) => {
                self.remember(db, &m);
                return Ok(None);
            }
            Created::Empty => return Err("aucun fichier de sauvegarde trouvé dans les emplacements".into()),
        };
        self.remember(db, &meta);
        for m in &self.mirrors {
            if m.available() {
                if let Err(e) = self.primary.copy_to(m, &self.key, &meta.id) {
                    log::warn!("copie de la sauvegarde vers {} impossible : {e}", m.root.display());
                }
            }
        }
        let pruned = self.primary.prune(&self.key, self.keep);
        if pruned > 0 {
            log::info!("{} : {pruned} ancienne(s) sauvegarde(s) automatique(s) purgée(s)", self.name);
        }
        let lock = db.lock().unwrap_or_else(|e| e.into_inner());
        let _ = lock.log_activity(
            ActivityKind::Save,
            Some(self.game_id),
            &format!("Sauvegarde de {}", self.name),
            &format!("{} · {} fichier(s) · {}", meta.reason_label(), meta.files.len(), util::format_size(meta.total_size)),
        );
        Ok(Some(meta))
    }

    fn remember(&self, db: &Arc<Mutex<Database>>, meta: &SnapshotMeta) {
        let lock = db.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(Some(mut c)) = lock.save_config(self.game_id) {
            c.last_id = Some(meta.id.clone());
            c.last_hash = Some(meta.content_hash.clone());
            c.last_at = Some(meta.created_at);
            let _ = lock.set_save_config(self.game_id, &c);
        }
    }
}

/// Résultat de l'analyse d'un jeu (thread de fond → interface).
struct SaveScan {
    game: GameId,
    snapshots: Vec<SnapshotMeta>,
    state: SyncState,
    local_files: usize,
    local_size: u64,
    /// L'état local correspond à la dernière sauvegarde créée ou restaurée sur ce PC.
    matches_last: bool,
    error: Option<String>,
}

impl Controller {
    pub(crate) fn save_env(&self) -> SaveEnv {
        let root = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root();
        let account = self.local_steam_id().and_then(|id| id.checked_sub(STEAMID64_BASE));
        SaveEnv::current(root, account)
    }

    pub(crate) fn machine(&self) -> String {
        let m = self.settings.borrow().machine_name.trim().to_string();
        if m.is_empty() { saves::machine_name() } else { m }
    }

    /// Dépôt principal (celui choisi s'il est accessible, sinon local) et copies.
    pub(crate) fn save_stores(&self) -> (SaveStore, bool, Vec<SaveStore>) {
        let s = self.settings.borrow();
        let local = SaveStore::new(self.paths.data_dir.join("saves"));
        let mirrors: Vec<SaveStore> = s.save_mirrors.iter().map(SaveStore::new).collect();
        match &s.save_root {
            Some(root) => {
                let store = SaveStore::new(root);
                if store.available() {
                    (store, false, mirrors)
                } else {
                    // Dépôt indisponible (NAS éteint, disque débranché) : on sauvegarde en local,
                    // la copie sera faite à son retour. Rien n'est jamais perdu.
                    (local, true, mirrors)
                }
            }
            None => (local, false, mirrors),
        }
    }

    pub(crate) fn game_key(&self, g: &Game) -> String {
        saves::game_key(g.source, &g.source_key, &g.name, g.platform.as_deref())
    }

    fn game_info(&self, g: &Game) -> GameInfo {
        GameInfo {
            name: g.name.clone(),
            developer: g.developer.clone(),
            publisher: g.publisher.clone(),
            install_dir: g.install_dir.clone(),
            source: Some(g.source),
            source_key: g.source_key.clone(),
            rom: (g.source == GameSource::Emulated).then(|| g.executable.clone()).flatten(),
            extra: self.plugin_save_patterns(g),
        }
    }

    /// Emplacements fournis par les extensions (voir `ctl_plugins`).
    pub(crate) fn plugin_save_patterns(&self, g: &Game) -> Vec<String> {
        let key = halcyon_core::util::search_key(&g.name).replace(' ', "");
        self.plugin_saves.borrow().get(&key).cloned().unwrap_or_default()
    }

    fn resolved_locations(&self, g: &Game, c: &SaveConfig, env: &SaveEnv) -> Vec<Location> {
        c.locations
            .iter()
            .filter_map(|t| env.resolve(t, g.install_dir.as_deref()).map(|path| Location { token: t.clone(), path }))
            .collect()
    }

    pub(crate) fn save_job_for(&self, g: &Game) -> Option<SaveJob> {
        let c = self.db().save_config(g.id).ok().flatten()?;
        if c.locations.is_empty() {
            return None;
        }
        let env = self.save_env();
        let (primary, _, mirrors) = self.save_stores();
        Some(SaveJob {
            game_id: g.id,
            key: self.game_key(g),
            name: g.name.clone(),
            locations: self.resolved_locations(g, &c, &env),
            primary,
            mirrors,
            machine: self.machine(),
            keep: self.settings.borrow().save_keep_auto as usize,
            auto_before: c.auto_before,
            auto_after: c.auto_after,
        })
    }

    // ------------------------------------------------------------------ onglet de la fiche

    pub(crate) fn refresh_saves_tab(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let env = self.save_env();
        let config = self.db().save_config(g.id).ok().flatten().unwrap_or(SaveConfig {
            auto_before: true,
            auto_after: true,
            ..Default::default()
        });
        let locs: Vec<SaveLocRow> = config
            .locations
            .iter()
            .map(|t| {
                let p = env.resolve(t, g.install_dir.as_deref());
                SaveLocRow {
                    token: t.as_str().into(),
                    path: p.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "Introuvable sur ce PC".into()).into(),
                    exists: p.as_ref().is_some_and(|p| p.exists()),
                }
            })
            .collect();
        let associated: Vec<PathBuf> =
            config.locations.iter().filter_map(|t| env.resolve(t, g.install_dir.as_deref())).collect();
        let cands: Vec<Candidate> = saves::detect(&self.game_info(g), &env).into_iter().filter(|c| !associated.contains(&c.path)).collect();
        *self.save_candidates.borrow_mut() = cands.clone();
        dui.set_save_cands(ModelRc::new(VecModel::from(
            cands
                .iter()
                .map(|c| SaveCandRow { path: c.path.to_string_lossy().into_owned().into(), reason: c.reason.as_str().into(), confident: c.confident })
                .collect::<Vec<_>>(),
        )));
        dui.set_save_locs(ModelRc::new(VecModel::from(locs)));
        dui.set_save_auto_before(config.auto_before);
        dui.set_save_auto_after(config.auto_after);
        let (store, fallback, _) = self.save_stores();
        dui.set_save_root(
            if fallback {
                format!("Dépôt choisi inaccessible : sauvegarde locale ({}), copiée à son retour", store.root.display())
            } else {
                format!("Dépôt : {}", store.root.display())
            }
            .into(),
        );
        if config.locations.is_empty() {
            dui.set_save_state(0);
            dui.set_save_state_text("Aucun emplacement associé : associez un emplacement détecté ou ajoutez un dossier.".into());
            dui.set_snapshots(ModelRc::default());
            return;
        }
        // Empreintes et liste des instantanés : en arrière-plan.
        dui.set_save_busy(true);
        let locations = self.resolved_locations(g, &config, &env);
        let key = self.game_key(g);
        let machine = self.machine();
        let id = g.id;
        std::thread::spawn(move || {
            let snapshots = store.list(&key);
            let mut matches_last = false;
            let (state, local_files, local_size, error) = match scan_locations(&locations) {
                Ok((files, _)) => {
                    let hash = (!files.is_empty()).then(|| content_hash(&files));
                    matches_last = hash.is_some() && hash == config.last_hash;
                    let st = saves::sync_state(
                        snapshots.first(),
                        hash.as_deref(),
                        config.last_id.as_deref(),
                        config.last_hash.as_deref(),
                        &machine,
                    );
                    (st, files.len(), files.iter().map(|f| f.size).sum(), None)
                }
                Err(e) => (SyncState::NoSnapshot, 0, 0, Some(e.to_string())),
            };
            post(move |c| c.on_save_scan(SaveScan { game: id, snapshots, state, local_files, local_size, matches_last, error }));
        });
    }

    fn on_save_scan(&self, s: SaveScan) {
        let Some(app) = self.app() else { return };
        if self.detail_id.get() != Some(s.game) {
            return;
        }
        let dui = app.global::<DetailUi>();
        dui.set_save_busy(false);
        let machine = self.machine();
        let rows: Vec<SnapshotRow> = s
            .snapshots
            .iter()
            .map(|m| SnapshotRow {
                id: m.id.as_str().into(),
                date: date_time(m.created_at).into(),
                reason: m.reason_label().into(),
                machine: m.machine.as_str().into(),
                other_machine: m.machine != machine,
                size: util::format_size(m.total_size).into(),
                files: m.files.len() as i32,
                note: m.note.as_str().into(),
            })
            .collect();
        dui.set_snapshots(ModelRc::new(VecModel::from(rows)));
        let local = format!("{} fichier(s), {} en local", s.local_files, util::format_size(s.local_size));
        let (state, text) = match (&s.error, &s.state) {
            (Some(e), _) => (5, format!("Lecture impossible : {e}")),
            (None, SyncState::NoSnapshot) => (2, format!("Aucune sauvegarde pour l'instant · {local}")),
            (None, SyncState::UpToDate) => (1, format!("À jour : identique à la dernière sauvegarde · {local}")),
            (None, SyncState::LocalChanged) if s.matches_last => (1, format!("Identique à la sauvegarde restaurée · {local}")),
            (None, SyncState::LocalChanged) => (2, format!("Modifié depuis la dernière sauvegarde · {local}")),
            (None, SyncState::RemoteNewer(m)) => (
                3,
                format!("Version plus récente sauvegardée sur « {} » le {} : restaurez-la pour continuer sur ce PC.", m.machine, date_time(m.created_at)),
            ),
            (None, SyncState::Conflict(m)) => (
                4,
                format!(
                    "Conflit : « {} » a sauvegardé une autre progression le {}, et les fichiers de ce PC ont aussi changé. \
                     Choisissez la version à garder (l'autre reste disponible dans l'historique).",
                    m.machine,
                    date_time(m.created_at)
                ),
            ),
        };
        if let SyncState::RemoteNewer(m) | SyncState::Conflict(m) = &s.state {
            *self.save_remote.borrow_mut() = Some(m.id.clone());
        } else {
            *self.save_remote.borrow_mut() = None;
        }
        dui.set_save_state(state);
        dui.set_save_state_text(text.into());
    }

    fn with_config(&self, id: GameId, f: impl FnOnce(&mut SaveConfig)) {
        let mut c = self.db().save_config(id).ok().flatten().unwrap_or(SaveConfig { auto_before: true, auto_after: true, ..Default::default() });
        f(&mut c);
        if let Err(e) = self.db().set_save_config(id, &c) {
            self.toast(format!("Enregistrement impossible : {e}"), true);
        }
    }

    pub(crate) fn add_save_location(&self, id: GameId, path: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let p = PathBuf::from(path.trim().trim_matches('"'));
        if !p.exists() {
            return self.toast(format!("Emplacement introuvable : {}", p.display()), true);
        }
        let token = self.save_env().tokenize(&p, g.install_dir.as_deref());
        self.with_config(id, |c| {
            if !c.locations.contains(&token) {
                c.locations.push(token);
            }
        });
        let _ = self.db().log_activity(ActivityKind::Save, Some(id), &format!("Emplacement de sauvegarde ajouté pour {}", g.name), path);
        self.refresh_saves_tab(&g);
    }

    pub(crate) fn associate_candidate(&self, id: GameId, index: usize) {
        let cand = self.save_candidates.borrow().get(index).cloned();
        if let Some(c) = cand {
            self.add_save_location(id, &c.path.to_string_lossy());
        }
    }

    pub(crate) fn remove_save_location(&self, id: GameId, index: usize) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        self.with_config(id, |c| {
            if index < c.locations.len() {
                c.locations.remove(index);
            }
        });
        self.refresh_saves_tab(&g);
    }

    pub(crate) fn set_save_auto(&self, id: GameId, before: bool, after: bool) {
        self.with_config(id, |c| {
            c.auto_before = before;
            c.auto_after = after;
        });
    }

    pub(crate) fn save_now(&self, id: GameId, note: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(job) = self.save_job_for(&g) else {
            return self.toast("Associez d'abord un emplacement de sauvegarde", true);
        };
        if let Some(app) = self.app() {
            app.global::<DetailUi>().set_save_busy(true);
        }
        let db = self.db.clone();
        let note = note.to_string();
        std::thread::spawn(move || {
            let r = job.run(&db, "manual", &note, true);
            post(move |c| {
                match r {
                    Ok(Some(m)) => c.toast(format!("Sauvegarde créée · {} fichier(s)", m.files.len()), false),
                    Ok(None) => c.toast("Rien n'a changé depuis la dernière sauvegarde", false),
                    Err(e) => c.toast(format!("Sauvegarde impossible : {e}"), true),
                }
                c.refresh_game_tools();
            });
        });
    }

    /// Restaure un instantané après confirmation ; l'état actuel est sauvegardé d'abord.
    pub(crate) fn restore_snapshot(&self, id: GameId, snap_id: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(job) = self.save_job_for(&g) else { return };
        let Some(snap) = job.primary.get(&job.key, snap_id) else {
            return self.toast("Sauvegarde introuvable dans le dépôt", true);
        };
        if self.active_session.get() == Some(id) {
            return self.toast("Fermez le jeu avant de restaurer une sauvegarde", true);
        }
        let env = self.save_env();
        let targets: Option<Vec<PathBuf>> = snap.locations.iter().map(|t| env.resolve(t, g.install_dir.as_deref())).collect();
        let Some(targets) = targets else {
            return self.toast("Un emplacement de cette sauvegarde n'existe pas sur ce PC", true);
        };
        let text = format!(
            "Les fichiers actuels de « {} » seront remplacés par la sauvegarde du {} ({}, {}). \
             Une sauvegarde de l'état actuel est créée juste avant : vous pourrez revenir en arrière.",
            g.name,
            date_time(snap.created_at),
            snap.machine,
            snap.reason_label().to_lowercase()
        );
        let db = self.db.clone();
        self.confirm("Restaurer cette sauvegarde ?", &text, "Restaurer", move |c| {
            if let Some(app) = c.app() {
                app.global::<DetailUi>().set_save_busy(true);
            }
            std::thread::spawn(move || {
                let r = (|| -> Result<usize, String> {
                    // 1. Instantané de sécurité (sauf si rien à sauvegarder).
                    match job.run(&db, "before_restore", "Avant restauration", true) {
                        Ok(_) => {}
                        Err(e) if e.starts_with("aucun fichier") => {}
                        Err(e) => return Err(format!("sauvegarde de sécurité impossible, restauration annulée : {e}")),
                    }
                    // 2. Restauration vérifiée.
                    let n = job.primary.restore(&snap, &targets).map_err(|e| e.to_string())?;
                    job.remember(&db, &snap);
                    let lock = db.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = lock.log_activity(
                        ActivityKind::Save,
                        Some(job.game_id),
                        &format!("Sauvegarde restaurée pour {}", job.name),
                        &format!("{} · {}", date_time(snap.created_at), snap.machine),
                    );
                    Ok(n)
                })();
                post(move |c| {
                    match r {
                        Ok(n) => c.toast(format!("Sauvegarde restaurée ({n} fichier(s))"), false),
                        Err(e) => c.toast(format!("Restauration impossible : {e}"), true),
                    }
                    c.refresh_game_tools();
                });
            });
        });
    }

    /// Conflit résolu en gardant la version de ce PC (nouvelle sauvegarde de référence).
    pub(crate) fn keep_local_saves(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(job) = self.save_job_for(&g) else { return };
        let db = self.db.clone();
        let machine = self.machine();
        std::thread::spawn(move || {
            let latest = job.primary.list(&job.key).first().map(|m| m.created_at).unwrap_or(0);
            let at = now_unix().max(latest + 1);
            let r = job.run_at(&db, "manual", &format!("Version de {machine} conservée"), false, at);
            post(move |c| {
                match r {
                    Ok(_) => c.toast("Version de ce PC enregistrée comme référence", false),
                    Err(e) => c.toast(format!("Sauvegarde impossible : {e}"), true),
                }
                c.refresh_game_tools();
            });
        });
    }

    pub(crate) fn restore_remote(&self, id: GameId) {
        let remote = self.save_remote.borrow().clone();
        if let Some(snap) = remote {
            self.restore_snapshot(id, &snap);
        }
    }

    pub(crate) fn compare_snapshot(&self, id: GameId, snap_id: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(job) = self.save_job_for(&g) else { return };
        let Some(snap) = job.primary.get(&job.key, snap_id) else { return };
        std::thread::spawn(move || {
            let r = scan_locations(&job.locations).map(|(now, _)| diff(&snap.files, &now));
            post(move |c| {
                let Some(app) = c.app() else { return };
                let dui = app.global::<DetailUi>();
                match r {
                    Ok(d) => {
                        let mut lines: Vec<SharedString> = Vec::new();
                        lines.extend(d.modified.iter().take(40).map(|f| SharedString::from(format!("~ {f}"))));
                        lines.extend(d.added.iter().take(40).map(|f| SharedString::from(format!("+ {f}"))));
                        lines.extend(d.removed.iter().take(40).map(|f| SharedString::from(format!("− {f}"))));
                        dui.set_save_diff_title(
                            format!(
                                "Sauvegarde du {} → fichiers actuels : {} modifié(s), {} ajouté(s), {} supprimé(s), {} identique(s)",
                                date_time(snap.created_at),
                                d.modified.len(),
                                d.added.len(),
                                d.removed.len(),
                                d.unchanged
                            )
                            .into(),
                        );
                        dui.set_save_diff(ModelRc::new(VecModel::from(lines)));
                    }
                    Err(e) => c.toast(format!("Comparaison impossible : {e}"), true),
                }
            });
        });
    }

    pub(crate) fn delete_snapshot(&self, id: GameId, snap_id: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(job) = self.save_job_for(&g) else { return };
        let Some(snap) = job.primary.get(&job.key, snap_id) else { return };
        let sid = snap_id.to_string();
        self.confirm(
            "Supprimer cette sauvegarde ?",
            &format!(
                "La sauvegarde du {} ({}) sera définitivement supprimée du dépôt. Les fichiers actuels du jeu ne sont pas touchés.",
                date_time(snap.created_at),
                snap.reason_label().to_lowercase()
            ),
            "Supprimer",
            move |c| {
                match job.primary.delete(&job.key, &sid) {
                    Ok(()) => c.toast("Sauvegarde supprimée", false),
                    Err(e) => c.toast(format!("Suppression impossible : {e}"), true),
                }
                c.refresh_game_tools();
            },
        );
    }

    // ------------------------------------------------------------------ page globale

    pub(crate) fn refresh_saves_page(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SavesUi>();
        let s = self.settings.borrow().clone();
        let (store, fallback, mirrors) = self.save_stores();
        ui.set_root(s.save_root.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default().into());
        ui.set_effective_root(store.root.to_string_lossy().into_owned().into());
        ui.set_root_fallback(fallback);
        ui.set_mirrors(ModelRc::new(VecModel::from(
            mirrors
                .iter()
                .map(|m| SharedString::from(format!("{}{}", m.root.display(), if m.available() { "" } else { "  (inaccessible)" })))
                .collect::<Vec<_>>(),
        )));
        ui.set_keep(s.save_keep_auto as i32);
        ui.set_machine(self.machine().into());
        let configs = self.db().save_configs().unwrap_or_default();
        let games: Vec<(GameId, String, String)> = {
            let lib = self.lib.borrow();
            configs.iter().filter_map(|(id, _)| lib.get(*id).map(|g| (*id, g.name.clone(), self.game_key(g)))).collect()
        };
        ui.set_busy(true);
        std::thread::spawn(move || {
            let rows: Vec<(GameId, String, usize, Option<i64>, u64, bool)> = games
                .into_iter()
                .map(|(id, name, key)| {
                    let list = store.list(&key);
                    let size: u64 = list.iter().map(|m| m.total_size).sum();
                    let other = list.first().is_some_and(|m| m.machine != saves::machine_name());
                    (id, name, list.len(), list.first().map(|m| m.created_at), size, other)
                })
                .collect();
            post(move |c| {
                let Some(app) = c.app() else { return };
                let ui = app.global::<SavesUi>();
                let total: u64 = rows.iter().map(|r| r.4).sum();
                let count: usize = rows.iter().map(|r| r.2).sum();
                ui.set_summary(format!("{} jeu(x) suivis · {count} sauvegarde(s) · {}", rows.len(), util::format_size(total)).into());
                let lib = c.lib.borrow();
                let out: Vec<SaveGameRow> = rows
                    .iter()
                    .map(|(id, name, n, last, size, other)| SaveGameRow {
                        id: *id as i32,
                        title: name.as_str().into(),
                        count: *n as i32,
                        last: last.map(date_time).unwrap_or_else(|| "Jamais".into()).into(),
                        size: util::format_size(*size).into(),
                        other_machine: *other,
                        card: lib.get(*id).map(|g| crate::models::card_for(g, &c.images, now_unix())).unwrap_or_default(),
                    })
                    .collect();
                ui.set_games(ModelRc::new(VecModel::from(out)));
                ui.set_busy(false);
            });
        });
    }

    /// Associe automatiquement les emplacements certains aux jeux installés sans configuration.
    pub(crate) fn detect_all_saves(&self) {
        let env = self.save_env();
        let configured: std::collections::HashSet<GameId> =
            self.db().save_configs().unwrap_or_default().into_iter().map(|(id, _)| id).collect();
        let games: Vec<(GameId, GameInfo, Option<PathBuf>)> = self
            .lib
            .borrow()
            .games
            .iter()
            .filter(|g| g.installed && !g.hidden && !configured.contains(&g.id))
            .map(|g| (g.id, self.game_info(g), g.install_dir.clone()))
            .collect();
        if let Some(app) = self.app() {
            app.global::<SavesUi>().set_busy(true);
        }
        let db = self.db.clone();
        std::thread::spawn(move || {
            let mut associated = 0;
            let mut games_done = 0;
            for (id, info, install) in games {
                let found: Vec<String> = saves::detect(&info, &env)
                    .into_iter()
                    .filter(|c| c.confident)
                    .map(|c| env.tokenize(&c.path, install.as_deref()))
                    .collect();
                if found.is_empty() {
                    continue;
                }
                associated += found.len();
                games_done += 1;
                let lock = db.lock().unwrap_or_else(|e| e.into_inner());
                let _ = lock.set_save_config(
                    id,
                    &SaveConfig { locations: found, auto_before: true, auto_after: true, ..Default::default() },
                );
            }
            post(move |c| {
                if games_done > 0 {
                    let _ = c.db().log_activity(
                        ActivityKind::Save,
                        None,
                        &format!("Sauvegardes détectées pour {games_done} jeu(x)"),
                        &format!("{associated} emplacement(s) associé(s) automatiquement"),
                    );
                }
                c.toast(
                    if games_done == 0 {
                        "Aucun nouvel emplacement certain trouvé (vous pouvez en associer à la main dans chaque fiche)".to_string()
                    } else {
                        format!("{associated} emplacement(s) associé(s) pour {games_done} jeu(x)")
                    },
                    false,
                );
                c.refresh_saves_page();
            });
        });
    }

    pub(crate) fn set_save_root(&self, path: &str) {
        let p = path.trim();
        let root = if p.is_empty() { None } else { Some(PathBuf::from(p)) };
        if let Some(r) = &root {
            if std::fs::create_dir_all(r).is_err() {
                return self.toast(format!("Dossier inaccessible : {}", r.display()), true);
            }
        }
        self.settings.borrow_mut().save_root = root;
        self.save_settings();
        self.sync_pending_saves();
        self.refresh_saves_page();
    }

    pub(crate) fn add_save_mirror(&self, path: &str) {
        let p = PathBuf::from(path.trim());
        if p.as_os_str().is_empty() || std::fs::create_dir_all(&p).is_err() {
            return self.toast("Dossier inaccessible", true);
        }
        {
            let mut s = self.settings.borrow_mut();
            if !s.save_mirrors.contains(&p) {
                s.save_mirrors.push(p);
            }
        }
        self.save_settings();
        self.sync_pending_saves();
        self.refresh_saves_page();
    }

    pub(crate) fn remove_save_mirror(&self, index: usize) {
        {
            let mut s = self.settings.borrow_mut();
            if index < s.save_mirrors.len() {
                s.save_mirrors.remove(index);
            }
        }
        self.save_settings();
        self.refresh_saves_page();
    }

    pub(crate) fn set_save_keep(&self, value: &str) {
        match value.trim().parse::<u32>() {
            Ok(n) if n <= 1000 => {
                self.settings.borrow_mut().save_keep_auto = n;
                self.save_settings();
                self.refresh_saves_page();
            }
            _ => self.toast("Nombre entre 0 (illimité) et 1000 attendu", true),
        }
    }

    pub(crate) fn set_machine_name(&self, value: &str) {
        self.settings.borrow_mut().machine_name = value.trim().to_string();
        self.save_settings();
        self.refresh_saves_page();
    }

    /// Recopie vers le dépôt choisi et les disques secondaires les sauvegardes qui
    /// n'y sont pas encore (dépôt revenu après une absence). Jamais de suppression.
    pub(crate) fn sync_pending_saves(&self) {
        let s = self.settings.borrow().clone();
        let local = SaveStore::new(self.paths.data_dir.join("saves"));
        let mut targets: Vec<SaveStore> = s.save_mirrors.iter().map(SaveStore::new).collect();
        let primary = s.save_root.as_ref().map(SaveStore::new);
        let keys: Vec<String> = {
            let lib = self.lib.borrow();
            let configs = self.db().save_configs().unwrap_or_default();
            configs.iter().filter_map(|(id, _)| lib.get(*id).map(|g| self.game_key(g))).collect()
        };
        std::thread::spawn(move || {
            let mut copied = 0;
            let mut sources = vec![local.clone()];
            if let Some(p) = &primary {
                if p.available() {
                    targets.insert(0, p.clone());
                    sources.push(p.clone());
                }
            }
            for key in &keys {
                for src in &sources {
                    for snap in src.list(key) {
                        for t in targets.iter().filter(|t| t.root != src.root && t.available()) {
                            if t.get(key, &snap.id).is_none() && src.copy_to(t, key, &snap.id).is_ok() {
                                copied += 1;
                            }
                        }
                    }
                }
            }
            if copied > 0 {
                log::info!("{copied} sauvegarde(s) recopiée(s) vers les dépôts");
                post(move |c| c.toast(format!("{copied} sauvegarde(s) synchronisée(s) vers les dépôts"), false));
            }
        });
    }
}
