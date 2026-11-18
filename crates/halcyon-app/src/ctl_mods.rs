//! Mods : bibliothèque par jeu, profils (ordre de chargement), déploiement réversible.

use crate::controller::{post, Controller};
use crate::ctl_tracking::date_time;
use crate::{DetailUi, ModRow};
use halcyon_core::{util, Game, GameId};
use halcyon_integrations::artwork::Downloader;
use halcyon_integrations::mods::{self, Adapter, ModStore};
use halcyon_storage::{now_unix, ActivityKind, ModEntry, ModProfile};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::path::PathBuf;


impl Controller {
    pub(crate) fn mod_store(&self) -> ModStore {
        ModStore::new(self.paths.data_dir.join("mods"))
    }

    /// Adaptateur choisi pour ce jeu (ou conseillé d'après ses fichiers).
    pub(crate) fn mod_adapter(&self, g: &Game) -> (Adapter, String) {
        let state = self.db().mod_state(g.id).unwrap_or_default();
        let suggested = g.install_dir.as_deref().map(mods::suggest);
        match state.adapter.as_deref().and_then(|a| serde_json::from_str::<Adapter>(a).ok()) {
            Some(a) => (a, tr!("Méthode choisie pour ce jeu.")),
            None => suggested.unwrap_or((Adapter::Generic { target: String::new() }, String::new())),
        }
    }

    /// Profil de mods actif, créé à la première utilisation.
    fn active_mod_profile(&self, g: &Game) -> ModProfile {
        let state = self.db().mod_state(g.id).unwrap_or_default();
        let profiles = self.db().mod_profiles(g.id).unwrap_or_default();
        if let Some(p) = state.active_profile.and_then(|id| profiles.iter().find(|p| p.id == id)) {
            return p.clone();
        }
        if let Some(p) = profiles.first() {
            return p.clone();
        }
        // Pas encore enregistré : créé à la première modification.
        ModProfile { id: 0, game_id: g.id, name: tr!("Par défaut"), entries: vec![] }
    }

    /// Les mods de la bibliothèque absents du profil y sont ajoutés (désactivés).
    fn complete_profile(&self, key: &str, p: &mut ModProfile) -> Vec<mods::ModInfo> {
        let infos = self.mod_store().list(key);
        p.entries.retain(|e| infos.iter().any(|m| m.id == e.mod_id));
        for m in &infos {
            if !p.entries.iter().any(|e| e.mod_id == m.id) {
                p.entries.push(ModEntry { mod_id: m.id.clone(), enabled: false });
            }
        }
        infos
    }

    pub(crate) fn refresh_mods_tab(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let key = self.game_key(g);
        let store = self.mod_store();
        let (adapter, hint) = self.mod_adapter(g);
        let mut profile = self.active_mod_profile(g);
        let infos = self.complete_profile(&key, &mut profile);
        let deployment = store.deployment(&key);
        let order: Vec<String> = profile.entries.iter().filter(|e| e.enabled).map(|e| e.mod_id.clone()).collect();
        let plan = store.plan(&key, &order, &adapter).unwrap_or_default();
        let mut conflicts_by_mod: HashMap<String, usize> = HashMap::new();
        for c in &plan.conflicts {
            for m in &c.mods {
                *conflicts_by_mod.entry(m.clone()).or_default() += 1;
            }
        }
        let by_id: HashMap<&str, &mods::ModInfo> = infos.iter().map(|m| (m.id.as_str(), m)).collect();
        let rows: Vec<ModRow> = profile
            .entries
            .iter()
            .filter_map(|e| {
                let m = by_id.get(e.mod_id.as_str())?;
                Some(ModRow {
                    id: m.id.as_str().into(),
                    name: m.name.as_str().into(),
                    version: m.version.as_str().into(),
                    latest: m
                        .latest_version
                        .as_deref()
                        .filter(|l| *l != m.version && !m.version.is_empty())
                        .unwrap_or("")
                        .into(),
                    enabled: e.enabled,
                    conflicts: conflicts_by_mod.get(&m.id).copied().unwrap_or(0) as i32,
                    files: m.files as i32,
                    size: util::format_size(m.size).into(),
                    deployed: deployment.files.iter().any(|f| f.mod_id == m.id),
                    nexus_id: m.nexus_id.map(|n| n.to_string()).unwrap_or_default().into(),
                })
            })
            .collect();
        dui.set_mods(ModelRc::new(VecModel::from(rows)));
        dui.set_mod_adapter(match adapter {
            Adapter::Generic { .. } => 0,
            Adapter::BepInEx => 1,
            Adapter::UnrealPaks { .. } => 2,
        });
        dui.set_mod_target(match &adapter {
            Adapter::Generic { target } => target.clone(),
            Adapter::UnrealPaks { paks } => paks.clone(),
            Adapter::BepInEx => String::new(),
        }
        .into());
        dui.set_mod_hint(hint.into());
        let profiles = self.db().mod_profiles(g.id).unwrap_or_default();
        let mut labels: Vec<SharedString> = profiles.iter().map(|p| SharedString::from(p.name.as_str())).collect();
        if labels.is_empty() {
            labels.push(profile.name.as_str().into());
        }
        dui.set_mod_profile_labels(ModelRc::new(VecModel::from(labels)));
        dui.set_mod_profile(profiles.iter().position(|p| p.id == profile.id).unwrap_or(0) as i32);
        let mut launch_labels: Vec<SharedString> = vec![tr!("Ne pas changer les mods").into()];
        launch_labels.extend(profiles.iter().map(|p| SharedString::from(tr!("Mods : {name}", name = p.name))));
        dui.set_mod_launch_labels(ModelRc::new(VecModel::from(launch_labels)));
        let deployed_profile = deployment.profile.and_then(|id| profiles.iter().find(|p| p.id == id)).map(|p| p.name.clone());
        dui.set_mod_status(
            if deployment.files.is_empty() {
                tr!("Aucun mod installé par Halcyon dans ce jeu.")
            } else {
                trn!(deployment.files.len(), "Installés le {at} : {n} fichier{v}. Retirer les mods remet le jeu dans son état d'origine.", "Installés le {at} : {n} fichiers{v}. Retirer les mods remet le jeu dans son état d'origine.", at = date_time(deployment.at), v = deployed_profile.map(|n| format!(" (profil « {n} »)")).unwrap_or_default())
            }
            .into(),
        );
        let mut conflicts: Vec<SharedString> = plan
            .conflicts
            .iter()
            .take(30)
            .map(|c| {
                let names: Vec<String> =
                    c.mods.iter().map(|id| by_id.get(id.as_str()).map(|m| m.name.clone()).unwrap_or_else(|| id.clone())).collect();
                SharedString::from(tr!("{rel} — {join} (gagnant : {v})", rel = c.rel, join = names.join(", "), v = names.last().cloned().unwrap_or_default()))
            })
            .collect();
        if plan.ignored > 0 {
            conflicts.push(trn!(plan.ignored, "{n} fichier ignoré par cette méthode d'installation", "{n} fichiers ignorés par cette méthode d'installation").into());
        }
        dui.set_mod_conflicts(ModelRc::new(VecModel::from(conflicts)));
        let external = g.install_dir.as_deref().map(|d| store.external(&key, d, &adapter)).unwrap_or_default();
        dui.set_mod_external(ModelRc::new(VecModel::from(
            external.iter().take(40).map(|s| SharedString::from(s.as_str())).collect::<Vec<_>>(),
        )));
        dui.set_mod_nexus_domain(self.db().mod_state(g.id).unwrap_or_default().nexus_domain.unwrap_or_default().into());
        dui.set_mod_available(g.install_dir.as_deref().is_some_and(|d| d.is_dir()));
        *self.mod_profile_cache.borrow_mut() = Some(profile);
    }

    fn save_active_profile(&self, g: &Game, p: &ModProfile) -> Option<i64> {
        let result = self.db().save_mod_profile(p);
        let id = match result {
            Ok(id) => id,
            Err(e) => {
                self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
                return None;
            }
        };
        if p.id == 0 {
            let mut st = self.db().mod_state(g.id).unwrap_or_default();
            st.active_profile = Some(id);
            let _ = self.db().set_mod_state(g.id, &st);
        }
        self.refresh_mods_tab(g);
        Some(id)
    }

    fn with_profile(&self, id: GameId, f: impl FnOnce(&mut ModProfile)) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let mut p = self.mod_profile_cache.borrow().clone().filter(|p| p.game_id == id).unwrap_or_else(|| self.active_mod_profile(&g));
        f(&mut p);
        self.save_active_profile(&g, &p);
    }

    pub(crate) fn toggle_mod(&self, id: GameId, mod_id: &str) {
        self.with_profile(id, |p| {
            if let Some(e) = p.entries.iter_mut().find(|e| e.mod_id == mod_id) {
                e.enabled = !e.enabled;
            }
        });
    }

    pub(crate) fn move_mod(&self, id: GameId, mod_id: &str, delta: i32) {
        self.with_profile(id, |p| {
            if let Some(i) = p.entries.iter().position(|e| e.mod_id == mod_id) {
                let j = (i as i32 + delta).clamp(0, p.entries.len() as i32 - 1) as usize;
                let e = p.entries.remove(i);
                p.entries.insert(j, e);
            }
        });
    }

    pub(crate) fn import_mod(&self, id: GameId, path: &str, name: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let src = PathBuf::from(path.trim().trim_matches('"'));
        if !src.exists() {
            return self.toast(tr!("Introuvable : {src}", src = src.display()), true);
        }
        let key = self.game_key(&g);
        let store = self.mod_store();
        let name = name.to_string();
        std::thread::spawn(move || {
            let r = store.import(&key, &src, &name, now_unix());
            post(move |c| {
                match r {
                    Ok(m) => {
                        let _ = c.db().log_activity(ActivityKind::Mod, Some(id), &tr_later!("Mod « {name} » ajouté", name = m.name), &g.name);
                        c.toast(trn!(m.files, "Mod « {name} » importé ({n} fichier) — activez-le puis appliquez", "Mod « {name} » importé ({n} fichiers) — activez-le puis appliquez", name = m.name), false);
                    }
                    Err(e) => c.toast(tr!("Import impossible : {e}", e = e), true),
                }
                c.refresh_mods_tab(&g);
            });
        });
    }

    pub(crate) fn delete_mod(&self, id: GameId, mod_id: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let key = self.game_key(&g);
        let mid = mod_id.to_string();
        self.confirm(
            &tr!("Supprimer ce mod de la bibliothèque ?"),
            &tr!("Ses fichiers sont supprimés de la bibliothèque de mods de Halcyon. Le jeu n'est pas modifié."),
            &tr!("Supprimer"),
            move |c| {
                match c.mod_store().remove(&key, &mid) {
                    Ok(()) => c.toast(tr!("Mod supprimé"), false),
                    Err(e) => c.toast(e.to_string(), true),
                }
                c.refresh_mods_tab(&g);
            },
        );
    }

    pub(crate) fn save_mod_info(&self, id: GameId, mod_id: &str, version: &str, nexus: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let key = self.game_key(&g);
        let store = self.mod_store();
        let Some(mut m) = store.list(&key).into_iter().find(|m| m.id == mod_id) else { return };
        m.version = version.trim().to_string();
        m.nexus_id = match nexus.trim() {
            "" => None,
            v => match v.parse() {
                Ok(n) => Some(n),
                Err(_) => return self.toast(tr!("Identifiant Nexus : nombre attendu (visible dans l'adresse de la page du mod)"), true),
            },
        };
        if let Err(e) = store.save_info(&key, &m) {
            return self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
        }
        self.refresh_mods_tab(&g);
    }

    pub(crate) fn set_mod_adapter(&self, id: GameId, index: usize, target: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let target = target.trim().trim_matches(['/', '\\']).replace('\\', "/");
        if halcyon_integrations::archive::safe_relative(&target).is_none() && !target.is_empty() {
            return self.toast(tr!("Dossier cible : chemin relatif au dossier du jeu attendu"), true);
        }
        let adapter = match index {
            1 => Adapter::BepInEx,
            2 => {
                let paks = if target.is_empty() {
                    match g.install_dir.as_deref().map(mods::suggest) {
                        Some((Adapter::UnrealPaks { paks }, _)) => paks,
                        _ => return self.toast(tr!("Dossier Content/Paks introuvable : indiquez-le (ex. MonJeu/Content/Paks)"), true),
                    }
                } else {
                    target
                };
                Adapter::UnrealPaks { paks }
            }
            _ => Adapter::Generic { target },
        };
        let mut st = self.db().mod_state(id).unwrap_or_default();
        st.adapter = serde_json::to_string(&adapter).ok();
        if let Err(e) = self.db().set_mod_state(id, &st) {
            return self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
        }
        self.refresh_mods_tab(&g);
    }

    pub(crate) fn select_mod_profile(&self, id: GameId, index: usize) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let profiles = self.db().mod_profiles(id).unwrap_or_default();
        if let Some(p) = profiles.get(index) {
            let mut st = self.db().mod_state(id).unwrap_or_default();
            st.active_profile = Some(p.id);
            let _ = self.db().set_mod_state(id, &st);
        }
        self.refresh_mods_tab(&g);
    }

    pub(crate) fn new_mod_profile(&self, id: GameId, name: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let current = self.mod_profile_cache.borrow().clone();
        let p = ModProfile {
            id: 0,
            game_id: id,
            name: name.trim().to_string(),
            entries: current.map(|c| c.entries).unwrap_or_default(),
        };
        let result = self.db().save_mod_profile(&p);
        match result {
            Ok(pid) => {
                let mut st = self.db().mod_state(id).unwrap_or_default();
                st.active_profile = Some(pid);
                let _ = self.db().set_mod_state(id, &st);
                self.toast(tr!("Profil de mods « {name} » créé (copie du profil actuel)", name = p.name), false);
            }
            Err(e) => self.toast(e.to_string(), true),
        }
        self.refresh_mods_tab(&g);
    }

    pub(crate) fn delete_mod_profile(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(p) = self.mod_profile_cache.borrow().clone() else { return };
        if p.id == 0 || self.db().mod_profiles(id).map(|l| l.len()).unwrap_or(0) <= 1 {
            return self.toast(tr!("Gardez au moins un profil de mods"), true);
        }
        self.confirm(
            &tr!("Supprimer ce profil de mods ?"),
            &tr!("Le profil « {name} » (ordre et activation) sera supprimé. Les mods restent dans la bibliothèque.", name = p.name),
            &tr!("Supprimer"),
            move |c| {
                let _ = c.db().delete_mod_profile(p.id);
                c.refresh_mods_tab(&g);
            },
        );
    }

    /// Applique un profil de mods au jeu (hors du thread de l'interface).
    pub(crate) fn deploy_mods(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(dir) = g.install_dir.clone() else { return };
        if self.active_session.get() == Some(id) {
            return self.toast(tr!("Fermez le jeu avant de modifier ses mods"), true);
        }
        let Some(mut p) = self.mod_profile_cache.borrow().clone() else { return };
        if p.id == 0 {
            match self.save_active_profile(&g, &p) {
                Some(id) => p.id = id,
                None => return,
            }
        }
        let job = self.mod_job(&g, &p, dir);
        let db = self.db.clone();
        std::thread::spawn(move || {
            let r = job.run();
            if let Ok(n) = &r {
                let lock = db.lock().unwrap_or_else(|e| e.into_inner());
                let _ = lock.log_activity(ActivityKind::Mod, Some(id), &tr_later!("Mods appliqués à {name}", name = g.name), &trn_later!(*n, "Profil « {name} » · {n} fichier", "Profil « {name} » · {n} fichiers", name = p.name));
            }
            post(move |c| {
                match r {
                    Ok(n) => c.toast(trn!(n, "Mods appliqués ({n} fichier)", "Mods appliqués ({n} fichiers)"), false),
                    Err(e) => c.toast(tr!("Application impossible : {e}", e = e), true),
                }
                c.refresh_mods_tab(&g);
            });
        });
    }

    pub(crate) fn undeploy_mods(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let Some(dir) = g.install_dir.clone() else { return };
        if self.active_session.get() == Some(id) {
            return self.toast(tr!("Fermez le jeu avant de modifier ses mods"), true);
        }
        let key = self.game_key(&g);
        let store = self.mod_store();
        let db = self.db.clone();
        std::thread::spawn(move || {
            let r = store.undeploy(&key, &dir);
            if r.is_ok() {
                let lock = db.lock().unwrap_or_else(|e| e.into_inner());
                let _ = lock.log_activity(ActivityKind::Mod, Some(id), &tr_later!("Mods retirés de {name}", name = g.name), "");
            }
            post(move |c| {
                match r {
                    Ok(rep) if rep.kept_modified.is_empty() => {
                        c.toast(tr!("Mods retirés · fichiers supprimés : {removed} · originaux rétablis : {restored}", removed = rep.removed, restored = rep.restored), false)
                    }
                    Ok(rep) => c.toast(
                        trn!(rep.kept_modified.len(), "{n} fichier modifié depuis l'installation a été laissé en place", "{n} fichiers modifiés depuis l'installation ont été laissés en place"),
                        true,
                    ),
                    Err(e) => c.toast(tr!("Retrait impossible : {e}", e = e), true),
                }
                c.refresh_mods_tab(&g);
            });
        });
    }

    pub(crate) fn mod_job(&self, g: &Game, p: &ModProfile, dir: PathBuf) -> ModJob {
        ModJob {
            key: self.game_key(g),
            store: self.mod_store(),
            adapter: self.mod_adapter(g).0,
            order: p.entries.iter().filter(|e| e.enabled).map(|e| e.mod_id.clone()).collect(),
            profile: p.id,
            dir,
        }
    }

    /// Profil de mods d'un profil de lancement, à appliquer s'il n'est pas déjà en place.
    pub(crate) fn mod_job_for_launch(&self, g: &Game, mod_profile: Option<i64>) -> Option<ModJob> {
        let pid = mod_profile?;
        let dir = g.install_dir.clone()?;
        let p = self.db().mod_profiles(g.id).ok()?.into_iter().find(|p| p.id == pid)?;
        let job = self.mod_job(g, &p, dir);
        (job.store.deployment(&job.key).profile != Some(pid)).then_some(job)
    }

    pub(crate) fn check_mod_versions(&self, id: GameId, domain: &str) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let key_api = self.settings.borrow().nexus_api_key.trim().to_string();
        if key_api.is_empty() {
            return self.toast(tr!("Ajoutez votre clé d'API Nexus Mods dans les paramètres (Comptes)"), true);
        }
        if self.settings.borrow().offline_forced {
            return self.toast(tr!("Mode hors ligne activé"), true);
        }
        let domain = domain.trim().to_lowercase();
        if domain.is_empty() || !domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return self.toast(tr!("Indiquez le nom du jeu sur Nexus Mods (ex. « skyrimspecialedition », visible dans l'adresse)"), true);
        }
        let mut st = self.db().mod_state(id).unwrap_or_default();
        st.nexus_domain = Some(domain.clone());
        let _ = self.db().set_mod_state(id, &st);
        let key = self.game_key(&g);
        let store = self.mod_store();
        std::thread::spawn(move || {
            let dl = Downloader::new();
            let mut checked = 0;
            let mut newer = 0;
            let mut errors = Vec::new();
            for mut m in store.list(&key) {
                let Some(nid) = m.nexus_id else { continue };
                match mods::nexus_latest(&dl, &key_api, &domain, nid) {
                    Ok(v) => {
                        checked += 1;
                        if !m.version.is_empty() && v != m.version {
                            newer += 1;
                        }
                        m.latest_version = Some(v);
                        let _ = store.save_info(&key, &m);
                    }
                    Err(e) => errors.push(format!("{} : {e}", m.name)),
                }
            }
            post(move |c| {
                if let Some(e) = errors.first() {
                    c.toast(format!("Nexus Mods : {e}"), true);
                } else if checked == 0 {
                    c.toast(tr!("Aucun mod n'a d'identifiant Nexus : renseignez-le dans la ligne du mod"), true);
                } else {
                    c.toast(tr!("Mods vérifiés : {checked} · mises à jour disponibles : {newer}", checked = checked, newer = newer), false);
                }
                c.refresh_mods_tab(&g);
            });
        });
    }
}

/// Déploiement de mods exécutable hors du thread de l'interface.
pub(crate) struct ModJob {
    key: String,
    store: ModStore,
    adapter: Adapter,
    order: Vec<String>,
    profile: i64,
    dir: PathBuf,
}

impl ModJob {
    pub fn run(&self) -> Result<usize, String> {
        let plan = self.store.plan(&self.key, &self.order, &self.adapter).map_err(|e| e.to_string())?;
        let d = self.store.deploy(&self.key, &self.dir, &plan, Some(self.profile), now_unix()).map_err(|e| e.to_string())?;
        Ok(d.files.len())
    }
}
