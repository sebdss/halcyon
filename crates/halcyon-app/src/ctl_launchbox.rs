//! Dos et tranches de boîtiers depuis la base LaunchBox (1.0.0-alpha.7, option en ligne,
//! désactivée par défaut).
//!
//! - **Index** : construit une fois à la demande (Paramètres › Comportement), dans un fil : archive
//!   téléchargée dans le cache, lue en flux, supprimée ensuite ; seul un petit index SQLite reste
//!   (`launchbox.sqlite`).
//! - **Images** : dans `case-art/` du cache, `<id>-back.<ext>` et `<id>-spine.<ext>` ; un jeu
//!   introuvable laisse `<id>.none` (pas de nouvel essai avant la prochaine reconstruction de
//!   l'index). Le dossier est lu une fois au démarrage, puis tenu à jour en mémoire : aucun accès
//!   disque quand les boîtiers défilent.
//! - **Quand** : le dos et la tranche du boîtier sélectionné (étagère, musée) sont cherchés à la
//!   volée ; « Récupérer pour toute la bibliothèque » parcourt les autres, un jeu à la fois.
//! - Jamais sans réseau autorisé (mode hors ligne respecté), jamais pour un jeu caché.

use crate::controller::{post, Controller};
use crate::images::{ImageKey, Kind};
use halcyon_core::launchbox::{normalize_title, platform_for};
use halcyon_core::{Game, GameId};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use slint::ComponentHandle;

#[derive(Clone, Debug, Default)]
pub struct CaseArt {
    pub back: Option<PathBuf>,
    pub spine: Option<PathBuf>,
    /// Cherché sans succès.
    pub none: bool,
}

#[derive(Default)]
pub struct LbState {
    pub loaded: bool,
    pub art: HashMap<GameId, CaseArt>,
    pub fetching: HashSet<GameId>,
    /// Construction de l'index ou récupération groupée en cours (une seule à la fois).
    pub job: Option<Arc<AtomicBool>>,
    pub status: String,
}

impl Controller {
    fn lb_dir(&self) -> PathBuf {
        self.paths.cache_dir.join("case-art")
    }

    pub(crate) fn lb_index_path(&self) -> PathBuf {
        self.paths.cache_dir.join("launchbox.sqlite")
    }

    /// Lit une fois le dossier des dos et tranches.
    fn lb_load(&self) {
        if self.lb.borrow().loaded {
            return;
        }
        let mut art: HashMap<GameId, CaseArt> = HashMap::new();
        if let Ok(rd) = std::fs::read_dir(self.lb_dir()) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
                let (id, part) = stem.split_once('-').unwrap_or((stem, ""));
                let Ok(id) = id.parse::<GameId>() else { continue };
                let entry = art.entry(id).or_default();
                match (part, ext) {
                    ("back", "jpg" | "png") => entry.back = Some(e.path()),
                    ("spine", "jpg" | "png") => entry.spine = Some(e.path()),
                    ("", "none") => entry.none = true,
                    _ => {}
                }
            }
        }
        let mut lb = self.lb.borrow_mut();
        lb.art = art;
        lb.loaded = true;
    }

    /// Dos réel d'un boîtier (scan), s'il existe ; sinon l'interface dessine un dos généré.
    /// Demande au passage le dos et la tranche du jeu s'ils n'ont jamais été cherchés.
    pub(crate) fn case_back(&self, g: &Game) -> Option<slint::Image> {
        self.lb_load();
        let art = self.lb.borrow().art.get(&g.id).cloned().unwrap_or_default();
        if art.back.is_none() && art.spine.is_none() && !art.none {
            self.lb_request(g);
        }
        let p = art.back?;
        self.images.get(ImageKey::remote(&p.to_string_lossy(), Kind::CoverLarge), Some(&p))
    }

    /// Tranche réelle d'un boîtier, si elle a été récupérée (aucun accès disque : mémoire).
    pub(crate) fn case_spine(&self, g: &Game) -> Option<slint::Image> {
        self.lb_load();
        let p = self.lb.borrow().art.get(&g.id).and_then(|a| a.spine.clone())?;
        self.images.get(ImageKey::remote(&p.to_string_lossy(), Kind::Spine), Some(&p))
    }

    fn lb_ready(&self) -> bool {
        self.settings.borrow().launchbox_art && self.net_allowed() && self.lb_index_path().is_file()
    }

    /// Cherche le dos et la tranche d'un jeu dans un fil (un seul à la fois par jeu).
    fn lb_request(&self, g: &Game) {
        if !self.lb_ready() || g.hidden {
            return;
        }
        let Some(platform) = platform_for(g.source, g.platform.as_deref()) else { return };
        if !self.lb.borrow_mut().fetching.insert(g.id) {
            return;
        }
        let (id, key, index, dir) = (g.id, normalize_title(&g.name), self.lb_index_path(), self.lb_dir());
        let _ = std::thread::Builder::new().name("launchbox".into()).spawn(move || {
            let art = fetch_one(id, &key, platform, &index, &dir);
            post(move |c| c.lb_fetched(id, art));
        });
    }

    fn lb_fetched(&self, id: GameId, art: CaseArt) {
        {
            let mut lb = self.lb.borrow_mut();
            lb.fetching.remove(&id);
            lb.art.insert(id, art);
        }
        self.gl3d_forget(id);
        if self.bp.borrow().active {
            self.refresh_bp();
        }
    }

    /// Paramètres : activer / désactiver l'option ; à l'activation, l'index est construit s'il
    /// n'existe pas encore.
    pub(crate) fn lb_set_enabled(self: &Rc<Self>, on: bool) {
        self.settings.borrow_mut().launchbox_art = on;
        self.save_settings();
        if on && !self.lb_index_path().is_file() {
            self.lb_build();
        }
        self.lb_publish();
    }

    /// Construit (ou reconstruit) l'index : téléchargement puis lecture en flux.
    pub(crate) fn lb_build(self: &Rc<Self>) {
        if self.lb.borrow().job.is_some() {
            return;
        }
        if !self.net_allowed() {
            self.lb.borrow_mut().status = tr!("Hors ligne : la base LaunchBox sera téléchargée une fois la connexion revenue.");
            return self.lb_publish();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.lb.borrow_mut().job = Some(cancel.clone());
        let (zip, index, dir) = (self.paths.cache_dir.join("launchbox-Metadata.zip"), self.lb_index_path(), self.lb_dir());
        self.lb_status(tr!("Téléchargement de la base LaunchBox…"));
        halcyon_storage::audit::event("launchbox.download", halcyon_core::launchbox::METADATA_URL);
        let _ = std::thread::Builder::new().name("launchbox-index".into()).spawn(move || {
            let mut last = std::time::Instant::now();
            let mut tick = move |text: String| {
                if last.elapsed() > std::time::Duration::from_millis(400) {
                    last = std::time::Instant::now();
                    post(move |c| c.lb_status(text));
                }
            };
            let r = halcyon_integrations::launchbox::download_metadata(
                &zip,
                &mut |done, total| {
                    let mb = done / (1024 * 1024);
                    tick(match total {
                        Some(t) if t > 0 => tr!("Téléchargement de la base LaunchBox : {mb} Mo sur {t} Mo", mb = mb, t = t / (1024 * 1024)),
                        _ => tr!("Téléchargement de la base LaunchBox : {mb} Mo", mb = mb),
                    });
                },
                &cancel,
            )
            .and_then(|_| {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                let mut last = std::time::Instant::now();
                halcyon_integrations::launchbox::build_index(
                    &zip,
                    &index,
                    &mut |read, total| {
                        if last.elapsed() > std::time::Duration::from_millis(400) {
                            last = std::time::Instant::now();
                            let pct = read * 100 / total.max(1);
                            post(move |c| c.lb_status(tr!("Préparation de l'index : {pct} %", pct = pct)));
                        }
                    },
                    &cancel,
                    now,
                )
            });
            // L'archive ne sert plus (l'index suffit) ; les « introuvable » sont réessayés.
            let _ = std::fs::remove_file(&zip);
            let _ = std::fs::remove_file(zip.with_extension("part"));
            if r.is_ok() {
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "none")) {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            post(move |c| c.lb_built(r));
        });
    }

    fn lb_built(&self, r: Result<(), String>) {
        {
            let mut lb = self.lb.borrow_mut();
            lb.job = None;
            lb.loaded = false;
        }
        let status = match r {
            Ok(()) => {
                let n = halcyon_storage::lbindex::LbIndex::open(&self.lb_index_path()).map(|i| i.games_with_art()).unwrap_or(0);
                halcyon_storage::audit::event("launchbox.index", &format!("{n}"));
                trn!(n as usize, "Index prêt : {n} jeu avec dos ou tranche.", "Index prêt : {n} jeux avec dos ou tranche.")
            }
            Err(e) => {
                log::warn!("LaunchBox : {e}");
                tr!("Échec : {e}", e = e)
            }
        };
        self.lb_status(status);
        if self.bp.borrow().active {
            self.refresh_bp();
        }
    }

    fn lb_status(&self, text: String) {
        self.lb.borrow_mut().status = text;
        self.lb_publish();
    }

    /// Récupère dos et tranches pour toute la bibliothèque (jeux visibles, jamais cherchés).
    pub(crate) fn lb_fetch_all(self: &Rc<Self>) {
        if !self.lb_ready() || self.lb.borrow().job.is_some() {
            return;
        }
        self.lb_load();
        let todo: Vec<(GameId, String, &'static str)> = {
            let lib = self.lib.borrow();
            let lb = self.lb.borrow();
            lib.games
                .iter()
                .filter(|g| !g.hidden)
                .filter(|g| lb.art.get(&g.id).is_none_or(|a| a.back.is_none() && a.spine.is_none() && !a.none))
                .filter_map(|g| platform_for(g.source, g.platform.as_deref()).map(|p| (g.id, normalize_title(&g.name), p)))
                .collect()
        };
        if todo.is_empty() {
            return self.lb_status(tr!("Tous les jeux ont déjà été cherchés."));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.lb.borrow_mut().job = Some(cancel.clone());
        let (index, dir) = (self.lb_index_path(), self.lb_dir());
        let total = todo.len();
        let _ = std::thread::Builder::new().name("launchbox-all".into()).spawn(move || {
            for (n, (id, key, platform)) in todo.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let art = fetch_one(id, &key, platform, &index, &dir);
                post(move |c| {
                    c.lb_fetched(id, art);
                    c.lb_status(tr!("Dos et tranches : {n} sur {total}", n = n + 1, total = total));
                });
                // Une requête à la fois, sans marteler le serveur.
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            post(|c| {
                c.lb.borrow_mut().job = None;
                c.lb_status(tr!("Dos et tranches à jour."));
            });
        });
    }

    /// Arrête la construction ou la récupération en cours.
    pub(crate) fn lb_cancel(&self) {
        if let Some(c) = self.lb.borrow().job.as_ref() {
            c.store(true, Ordering::Relaxed);
        }
    }

    /// Paramètres : état de l'option.
    pub(crate) fn lb_publish(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<crate::SettingsUi>();
        let (busy, status) = {
            let lb = self.lb.borrow();
            (lb.job.is_some(), lb.status.clone())
        };
        let ready = self.lb_index_path().is_file();
        ui.set_launchbox_art(self.settings.borrow().launchbox_art);
        ui.set_launchbox_busy(busy);
        ui.set_launchbox_ready(ready);
        let status = if status.is_empty() && ready {
            let n = halcyon_storage::lbindex::LbIndex::open(&self.lb_index_path()).map(|i| i.games_with_art()).unwrap_or(0);
            trn!(n as usize, "Index prêt : {n} jeu avec dos ou tranche.", "Index prêt : {n} jeux avec dos ou tranche.")
        } else {
            status
        };
        ui.set_launchbox_status(status.into());
    }
}

/// Cherche un jeu dans l'index puis télécharge son dos et sa tranche (dans un fil).
fn fetch_one(id: GameId, key: &str, platform: &str, index: &Path, dir: &Path) -> CaseArt {
    let mut art = CaseArt::default();
    let found = halcyon_storage::lbindex::LbIndex::open(index).and_then(|i| i.lookup(key, platform).ok()).unwrap_or_default();
    let get = |file: &Option<String>, part: &str| -> Option<PathBuf> {
        let file = file.as_ref()?;
        let ext = if file.to_ascii_lowercase().ends_with(".png") { "png" } else { "jpg" };
        let dest = dir.join(format!("{id}-{part}.{ext}"));
        match halcyon_integrations::launchbox::fetch_part(file, &dest) {
            Ok(()) => Some(dest),
            Err(e) => {
                log::info!("LaunchBox {file} : {e}");
                None
            }
        }
    };
    art.back = get(&found.back, "back");
    art.spine = get(&found.spine, "spine");
    if art.back.is_none() && art.spine.is_none() {
        // Pas dans la base : noté sur le disque (pas de nouvel essai). Échec réseau : noté
        // seulement en mémoire (nouvel essai au prochain démarrage).
        art.none = true;
        if found.back.is_none() && found.spine.is_none() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("{id}.none")), b"");
        }
    }
    art
}
